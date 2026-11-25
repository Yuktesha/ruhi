// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/neural_reranker.rs — v1.0 神經重排序二進位 IPC 用戶端與 15 ms 硬逾時回退管理器
//
// 依據 docs/RUHI_V1_NEURAL_RERANKER_SPEC.md 實作：
// - 以全域 OnceLock 旁表管理選配狀態與連線退避，嚴格維持 RuhiEngine / TransitionEngine / RuhiTextService 二進位佈局不變；
// - 預設關閉；開啟時透過 Windows Named Pipe (FILE_FLAG_OVERLAPPED + 15 ms 硬逾時) 查詢 ruhi_reranker.exe，
//   服務未啟動或逾時立即靜默回退原 Viterbi / Discourse 結果，並進入 2,000 ms 失敗退避快取；
// - 另支援 `in_process_model` 供 `ruhi_eval --neural-rerank` 零 IPC 執行純 Rust INT8 全量基準評估。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use super::lattice::LatticeNode;
use super::rlnm::{RlnmKvCache, RlnmModel};

pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\RuhI_NeuralReranker_v1";
pub const SINGLETON_MUTEX_NAME: &str = r"Local\RuhI_NeuralReranker_Singleton_v1";
pub const REQ_MAGIC: &[u8; 4] = b"RNQ1";
pub const RESP_MAGIC: &[u8; 4] = b"RNR1";
pub const MAX_IPC_FRAME_BYTES: usize = 1_048_576;
const CONNECT_FAIL_BACKOFF_MS: u64 = 2_000;
const SPAWN_COOLDOWN_SECS: u64 = 30;
pub const MAX_CROSS_SENTENCE_PREFIX_CHARS: usize = 48;
const MAX_TRACKED_CONTEXT_ENGINES: usize = 64;

#[derive(Debug, Clone)]
pub struct NeuralRerankerConfig {
    pub enabled: bool,
    pub lambda: f64,
    pub timeout_ms: u16,
    pub pipe_name: String,
    pub min_syllables: usize,
    pub score_margin: f64,
    pub max_trie_nodes: usize,
}

impl Default for NeuralRerankerConfig {
    fn default() -> Self {
        let env_enabled = std::env::var("RUHI_NEURAL_RERANK")
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);
        Self {
            enabled: env_enabled,
            lambda: 2.25,
            timeout_ms: 25,
            pipe_name: DEFAULT_PIPE_NAME.to_string(),
            min_syllables: 4,
            score_margin: 25.0,
            max_trie_nodes: 200,
        }
    }
}

struct RuntimeState {
    cfg: NeuralRerankerConfig,
    in_process_model: Option<Arc<RlnmModel>>,
    last_pipe_failure: Option<Instant>,
    exe_path: Option<std::path::PathBuf>,
    model_path: Option<std::path::PathBuf>,
    last_spawn_attempt: Option<Instant>,
}

static STATE: OnceLock<RwLock<RuntimeState>> = OnceLock::new();
static ENGINE_COMMITTED_CONTEXT: OnceLock<RwLock<HashMap<usize, String>>> = OnceLock::new();
static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static TLS_INPROC_CACHES: RefCell<HashMap<usize, RlnmKvCache>> = RefCell::new(HashMap::new());
    static TLS_SCOPED_PREFIX: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn context_map() -> &'static RwLock<HashMap<usize, String>> {
    ENGINE_COMMITTED_CONTEXT.get_or_init(|| RwLock::new(HashMap::new()))
}

fn trim_last_chars(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        s.to_string()
    } else {
        s.chars().skip(total - max_chars).collect()
    }
}

/// 依字元邊界去重疊合併 `a`（既有長前文）與 `b`（最近單詞），並裁切至最多 `max_chars` 字
fn merge_prefix_with_overlap(a: &str, b: &str, max_chars: usize) -> String {
    if a.is_empty() {
        return trim_last_chars(b, max_chars);
    }
    if b.is_empty() || a.ends_with(b) {
        return trim_last_chars(a, max_chars);
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let max_k = a_chars.len().min(b_chars.len());
    let mut overlap = 0usize;
    for k in (1..=max_k).rev() {
        if a_chars[a_chars.len() - k..] == b_chars[..k] {
            overlap = k;
            break;
        }
    }
    let mut merged = String::with_capacity(a.len() + b.len());
    merged.push_str(a);
    for &ch in &b_chars[overlap..] {
        merged.push(ch);
    }
    trim_last_chars(&merged, max_chars)
}

/// 紀錄 `engine_id` 剛送出的跨句前文（旁表維持 `RuhiEngine` ABI 不變；連續換行／段落清空時重置）
pub fn push_committed_context(engine_id: usize, text: &str) {
    let is_para_break = text.contains("\n\n")
        || text.contains("\r\n\r\n")
        || (text.trim().is_empty() && (text.contains('\n') || text.contains('\r')));
    let Ok(mut map) = context_map().write() else {
        return;
    };
    if is_para_break {
        map.remove(&engine_id);
        return;
    }
    let clean: String = text
        .chars()
        .filter(|&c| !c.is_control() && c != '\t')
        .collect();
    if clean.is_empty() {
        return;
    }
    if !map.contains_key(&engine_id) && map.len() >= MAX_TRACKED_CONTEXT_ENGINES {
        map.clear();
    }
    let entry = map.entry(engine_id).or_default();
    let merged = if entry.ends_with(&clean) {
        trim_last_chars(entry, MAX_CROSS_SENTENCE_PREFIX_CHARS)
    } else {
        let mut s = String::with_capacity(entry.len() + clean.len());
        s.push_str(entry);
        s.push_str(&clean);
        trim_last_chars(&s, MAX_CROSS_SENTENCE_PREFIX_CHARS)
    };
    *entry = merged;
}

pub fn clear_committed_context(engine_id: usize) {
    if let Ok(mut map) = context_map().write() {
        map.remove(&engine_id);
    }
}

pub fn get_committed_context(engine_id: usize) -> Option<String> {
    context_map()
        .read()
        .ok()
        .and_then(|m| m.get(&engine_id).cloned())
        .filter(|s| !s.is_empty())
}

/// 執行緒區域 RAII 前文守衛，使 `viterbi.rs` 透過現行介面自動取得跨句前文而不更動公開簽章
pub struct ScopedContextPrefix {
    prev: Option<String>,
}

impl ScopedContextPrefix {
    pub fn for_engine(engine_id: usize) -> Self {
        Self::with_prefix(get_committed_context(engine_id).as_deref())
    }

    pub fn with_prefix(prefix: Option<&str>) -> Self {
        let next = prefix
            .map(|s| trim_last_chars(s, MAX_CROSS_SENTENCE_PREFIX_CHARS))
            .filter(|s| !s.is_empty());
        let prev = TLS_SCOPED_PREFIX.with(|cell| cell.borrow_mut().replace(next.unwrap_or_default()));
        if TLS_SCOPED_PREFIX.with(|cell| cell.borrow().as_deref() == Some("")) {
            TLS_SCOPED_PREFIX.with(|cell| *cell.borrow_mut() = None);
        }
        Self { prev }
    }
}

impl Drop for ScopedContextPrefix {
    fn drop(&mut self) {
        let prev = self.prev.take();
        TLS_SCOPED_PREFIX.with(|cell| {
            *cell.borrow_mut() = prev;
        });
    }
}

fn state() -> &'static RwLock<RuntimeState> {
    STATE.get_or_init(|| {
        RwLock::new(RuntimeState {
            cfg: NeuralRerankerConfig::default(),
            in_process_model: None,
            last_pipe_failure: None,
            exe_path: None,
            model_path: None,
            last_spawn_attempt: None,
        })
    })
}

pub fn set_sidecar_paths(
    exe_path: Option<std::path::PathBuf>,
    model_path: Option<std::path::PathBuf>,
) {
    if let Ok(mut g) = state().write() {
        g.exe_path = exe_path;
        g.model_path = model_path;
    }
}

pub fn is_sidecar_running() -> bool {
    win_pipe_client::is_singleton_mutex_present()
}

pub fn ensure_sidecar_running_async() -> bool {
    let (enabled, exe_opt, model_opt, in_cooldown) = match state().read() {
        Ok(g) => {
            let cooldown = g
                .last_spawn_attempt
                .is_some_and(|t| t.elapsed() < Duration::from_secs(SPAWN_COOLDOWN_SECS));
            (g.cfg.enabled, g.exe_path.clone(), g.model_path.clone(), cooldown)
        }
        Err(_) => return false,
    };
    if !enabled {
        return false;
    }
    let (Some(exe), Some(model)) = (exe_opt, model_opt) else {
        return false;
    };
    if !exe.is_file() || !model.is_file() {
        return false;
    }
    if is_sidecar_running() {
        return true;
    }
    if in_cooldown {
        return false;
    }
    {
        let Ok(mut g) = state().write() else {
            return false;
        };
        if let Some(t) = g.last_spawn_attempt {
            if t.elapsed() < Duration::from_secs(SPAWN_COOLDOWN_SECS) {
                return false;
            }
        }
        g.last_spawn_attempt = Some(Instant::now());
    }
    win_pipe_client::spawn_windowless_sidecar(&exe, &model)
}

pub fn set_runtime_config(enabled: bool, lambda: f64, timeout_ms: u16) {
    if let Ok(mut g) = state().write() {
        g.cfg.enabled = enabled;
        g.cfg.lambda = lambda.clamp(0.0, 10.0);
        g.cfg.timeout_ms = timeout_ms.clamp(1, 500);
        if enabled {
            g.last_pipe_failure = None;
        }
    }
}

pub fn set_pipe_name_for_test(pipe_name: &str) {
    if let Ok(mut g) = state().write() {
        g.cfg.pipe_name = pipe_name.to_string();
        g.last_pipe_failure = None;
    }
}

pub fn set_in_process_model(model: Option<Arc<RlnmModel>>, lambda: f64) {
    if let Ok(mut g) = state().write() {
        g.cfg.enabled = model.is_some();
        g.cfg.lambda = lambda.clamp(0.0, 10.0);
        g.in_process_model = model;
        g.last_pipe_failure = None;
    }
}

pub fn set_tuning_overrides(score_margin: Option<f64>, max_trie_nodes: Option<usize>) {
    if let Ok(mut g) = state().write() {
        if let Some(m) = score_margin {
            g.cfg.score_margin = m.clamp(0.0, 200.0);
        }
        if let Some(n) = max_trie_nodes {
            g.cfg.max_trie_nodes = n.clamp(2, 4096);
        }
    }
}

pub fn current_config() -> NeuralRerankerConfig {
    state()
        .read()
        .map(|g| g.cfg.clone())
        .unwrap_or_default()
}

#[inline]
pub fn is_enabled_for_syllables(n_syllables: usize) -> bool {
    if let Ok(g) = state().read() {
        g.cfg.enabled && n_syllables >= g.cfg.min_syllables
    } else {
        false
    }
}

/// 編碼含 4-byte 小端長度前綴之 `RNQ1` 請求幀
pub fn encode_request(req_id: u64, timeout_ms: u16, prefix: &str, cands: &[&str]) -> Vec<u8> {
    let pref_bytes = prefix.as_bytes();
    let pref_len = pref_bytes.len().min(u16::MAX as usize);
    let k = cands.len().min(u16::MAX as usize);

    let mut payload = Vec::with_capacity(16 + pref_len + k * 24);
    payload.extend_from_slice(REQ_MAGIC);
    payload.extend_from_slice(&req_id.to_le_bytes());
    payload.extend_from_slice(&timeout_ms.to_le_bytes());
    payload.extend_from_slice(&(pref_len as u16).to_le_bytes());
    payload.extend_from_slice(&pref_bytes[..pref_len]);
    payload.extend_from_slice(&(k as u16).to_le_bytes());
    for &c in &cands[..k] {
        let cb = c.as_bytes();
        let clen = cb.len().min(u16::MAX as usize);
        payload.extend_from_slice(&(clen as u16).to_le_bytes());
        payload.extend_from_slice(&cb[..clen]);
    }

    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    framed.extend_from_slice(&payload);
    framed
}

/// 解碼不含 4-byte 長度前綴之 `RNQ1` 請求載荷
pub fn decode_request(payload: &[u8]) -> Option<(u64, u16, String, Vec<String>)> {
    if payload.len() < 16 || &payload[0..4] != REQ_MAGIC {
        return None;
    }
    let req_id = u64::from_le_bytes(payload[4..12].try_into().ok()?);
    let timeout_ms = u16::from_le_bytes(payload[12..14].try_into().ok()?);
    let pref_len = u16::from_le_bytes(payload[14..16].try_into().ok()?) as usize;
    let mut off = 16usize;
    if payload.len() < off + pref_len + 2 {
        return None;
    }
    let prefix = std::str::from_utf8(&payload[off..off + pref_len]).ok()?.to_string();
    off += pref_len;
    let k = u16::from_le_bytes(payload[off..off + 2].try_into().ok()?) as usize;
    off += 2;
    let mut cands = Vec::with_capacity(k);
    for _ in 0..k {
        if payload.len() < off + 2 {
            return None;
        }
        let clen = u16::from_le_bytes(payload[off..off + 2].try_into().ok()?) as usize;
        off += 2;
        if payload.len() < off + clen {
            return None;
        }
        let s = std::str::from_utf8(&payload[off..off + clen]).ok()?.to_string();
        off += clen;
        cands.push(s);
    }
    Some((req_id, timeout_ms, prefix, cands))
}

/// 編碼含 4-byte 小端長度前綴之 `RNR1` 回應幀
pub fn encode_response(req_id: u64, status: u8, scores: &[f32]) -> Vec<u8> {
    let k = scores.len().min(u16::MAX as usize);
    let mut payload = Vec::with_capacity(15 + k * 4);
    payload.extend_from_slice(RESP_MAGIC);
    payload.extend_from_slice(&req_id.to_le_bytes());
    payload.push(status);
    payload.extend_from_slice(&(k as u16).to_le_bytes());
    for &sc in &scores[..k] {
        payload.extend_from_slice(&sc.to_le_bytes());
    }
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    framed.extend_from_slice(&payload);
    framed
}

/// 解碼不含 4-byte 長度前綴之 `RNR1` 回應載荷
pub fn decode_response(payload: &[u8]) -> Option<(u64, u8, Vec<f32>)> {
    if payload.len() < 15 || &payload[0..4] != RESP_MAGIC {
        return None;
    }
    let req_id = u64::from_le_bytes(payload[4..12].try_into().ok()?);
    let status = payload[12];
    let k = u16::from_le_bytes(payload[13..15].try_into().ok()?) as usize;
    let expected_len = 15usize.checked_add(k.checked_mul(4)?)?;
    if payload.len() != expected_len {
        return None;
    }
    let mut scores = Vec::with_capacity(k);
    for idx in 0..k {
        let pos = 15 + idx * 4;
        scores.push(f32::from_le_bytes(payload[pos..pos + 4].try_into().ok()?));
    }
    Some((req_id, status, scores))
}

#[cfg(target_os = "windows")]
mod win_pipe_client {
    use super::{MAX_IPC_FRAME_BYTES, SINGLETON_MUTEX_NAME};
    use std::ffi::c_void;
    use std::path::Path;
    use std::time::{Duration, Instant};

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const INVALID_HANDLE_VALUE: isize = -1;
    const ERROR_IO_PENDING: u32 = 997;
    const ERROR_PIPE_BUSY: u32 = 231;
    const WAIT_OBJECT_0: u32 = 0;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const STARTF_USESHOWWINDOW: u32 = 0x0000_0001;

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        lp_reserved: *mut u16,
        lp_desktop: *mut u16,
        lp_title: *mut u16,
        dw_x: u32,
        dw_y: u32,
        dw_x_size: u32,
        dw_y_size: u32,
        dw_x_count_chars: u32,
        dw_y_count_chars: u32,
        dw_fill_attribute: u32,
        dw_flags: u32,
        w_show_window: u16,
        cb_reserved2: u16,
        lp_reserved2: *mut u8,
        h_std_input: isize,
        h_std_output: isize,
        h_std_error: isize,
    }

    #[repr(C)]
    struct ProcessInformation {
        h_process: isize,
        h_thread: isize,
        dw_process_id: u32,
        dw_thread_id: u32,
    }

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        h_event: isize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: *const c_void,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: isize,
        ) -> isize;
        fn WaitNamedPipeW(lpNamedPipeName: *const u16, nTimeOut: u32) -> i32;
        fn CreateEventW(
            lpEventAttributes: *const c_void,
            bManualReset: i32,
            bInitialState: i32,
            lpName: *const u16,
        ) -> isize;
        fn ResetEvent(hEvent: isize) -> i32;
        fn WriteFile(
            hFile: isize,
            lpBuffer: *const u8,
            nNumberOfBytesToWrite: u32,
            lpNumberOfBytesWritten: *mut u32,
            lpOverlapped: *mut Overlapped,
        ) -> i32;
        fn ReadFile(
            hFile: isize,
            lpBuffer: *mut u8,
            nNumberOfBytesToRead: u32,
            lpNumberOfBytesRead: *mut u32,
            lpOverlapped: *mut Overlapped,
        ) -> i32;
        fn WaitForSingleObject(hHandle: isize, dwMilliseconds: u32) -> u32;
        fn GetOverlappedResult(
            hFile: isize,
            lpOverlapped: *mut Overlapped,
            lpNumberOfBytesTransferred: *mut u32,
            bWait: i32,
        ) -> i32;
        fn CancelIoEx(hFile: isize, lpOverlapped: *mut Overlapped) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
        fn GetLastError() -> u32;
        fn OpenMutexW(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const u16) -> isize;
        fn CreateProcessW(
            lpApplicationName: *const u16,
            lpCommandLine: *mut u16,
            lpProcessAttributes: *const c_void,
            lpThreadAttributes: *const c_void,
            bInheritHandles: i32,
            dwCreationFlags: u32,
            lpEnvironment: *const c_void,
            lpCurrentDirectory: *const u16,
            lpStartupInfo: *const StartupInfoW,
            lpProcessInformation: *mut ProcessInformation,
        ) -> i32;
    }

    pub fn is_singleton_mutex_present() -> bool {
        let mut wide: Vec<u16> = SINGLETON_MUTEX_NAME.encode_utf16().collect();
        wide.push(0);
        let h = unsafe { OpenMutexW(SYNCHRONIZE, 0, wide.as_ptr()) };
        if h != 0 {
            unsafe {
                CloseHandle(h);
            }
            true
        } else {
            false
        }
    }

    pub fn spawn_windowless_sidecar(exe: &Path, model: &Path) -> bool {
        let mut cmd_wide: Vec<u16> = format!("\"{}\" --model \"{}\"", exe.display(), model.display())
            .encode_utf16()
            .collect();
        cmd_wide.push(0);
        let si = StartupInfoW {
            cb: std::mem::size_of::<StartupInfoW>() as u32,
            lp_reserved: core::ptr::null_mut(),
            lp_desktop: core::ptr::null_mut(),
            lp_title: core::ptr::null_mut(),
            dw_x: 0,
            dw_y: 0,
            dw_x_size: 0,
            dw_y_size: 0,
            dw_x_count_chars: 0,
            dw_y_count_chars: 0,
            dw_fill_attribute: 0,
            dw_flags: STARTF_USESHOWWINDOW,
            w_show_window: 0,
            cb_reserved2: 0,
            lp_reserved2: core::ptr::null_mut(),
            h_std_input: 0,
            h_std_output: 0,
            h_std_error: 0,
        };
        let mut pi = ProcessInformation {
            h_process: 0,
            h_thread: 0,
            dw_process_id: 0,
            dw_thread_id: 0,
        };
        let ok = unsafe {
            CreateProcessW(
                core::ptr::null(),
                cmd_wide.as_mut_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                0,
                CREATE_NO_WINDOW,
                core::ptr::null(),
                core::ptr::null(),
                &si,
                &mut pi,
            )
        };
        if ok != 0 {
            unsafe {
                if pi.h_thread != 0 {
                    CloseHandle(pi.h_thread);
                }
                if pi.h_process != 0 {
                    CloseHandle(pi.h_process);
                }
            }
            true
        } else {
            false
        }
    }

    struct HandleGuard(isize);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    fn write_all_timed(h_pipe: isize, h_event: isize, buf: &[u8], deadline: Instant) -> bool {
        let mut written = 0usize;
        while written < buf.len() {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let rem_ms = deadline.duration_since(now).as_millis().clamp(1, u32::MAX as u128) as u32;
            unsafe {
                ResetEvent(h_event);
            }
            let mut ov = Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                h_event,
            };
            let mut n_xfer = 0u32;
            let ok = unsafe {
                WriteFile(
                    h_pipe,
                    buf[written..].as_ptr(),
                    (buf.len() - written) as u32,
                    &mut n_xfer,
                    &mut ov,
                )
            };
            if ok == 0 {
                let err = unsafe { GetLastError() };
                if err != ERROR_IO_PENDING {
                    return false;
                }
                let wait = unsafe { WaitForSingleObject(h_event, rem_ms) };
                if wait != WAIT_OBJECT_0 {
                    unsafe {
                        CancelIoEx(h_pipe, &mut ov);
                    }
                    return false;
                }
                let res = unsafe { GetOverlappedResult(h_pipe, &mut ov, &mut n_xfer, 0) };
                if res == 0 || n_xfer == 0 {
                    return false;
                }
            }
            written += n_xfer as usize;
        }
        true
    }

    fn read_exact_timed(h_pipe: isize, h_event: isize, dst: &mut [u8], deadline: Instant) -> bool {
        let mut read_total = 0usize;
        while read_total < dst.len() {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let rem_ms = deadline.duration_since(now).as_millis().clamp(1, u32::MAX as u128) as u32;
            unsafe {
                ResetEvent(h_event);
            }
            let mut ov = Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                h_event,
            };
            let mut n_xfer = 0u32;
            let ok = unsafe {
                ReadFile(
                    h_pipe,
                    dst[read_total..].as_mut_ptr(),
                    (dst.len() - read_total) as u32,
                    &mut n_xfer,
                    &mut ov,
                )
            };
            if ok == 0 {
                let err = unsafe { GetLastError() };
                if err != ERROR_IO_PENDING {
                    return false;
                }
                let wait = unsafe { WaitForSingleObject(h_event, rem_ms) };
                if wait != WAIT_OBJECT_0 {
                    unsafe {
                        CancelIoEx(h_pipe, &mut ov);
                    }
                    return false;
                }
                let res = unsafe { GetOverlappedResult(h_pipe, &mut ov, &mut n_xfer, 0) };
                if res == 0 || n_xfer == 0 {
                    return false;
                }
            } else if n_xfer == 0 {
                return false;
            }
            read_total += n_xfer as usize;
        }
        true
    }

    pub fn query_named_pipe(pipe_name: &str, req_framed: &[u8], timeout_ms: u16) -> Option<Vec<u8>> {
        let mut wide: Vec<u16> = pipe_name.encode_utf16().collect();
        wide.push(0);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1) as u64);

        let mut h_pipe = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                core::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                0,
            )
        };
        if h_pipe == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            if err == ERROR_PIPE_BUSY {
                let now = Instant::now();
                if now < deadline {
                    let wait_ms = deadline.duration_since(now).as_millis().min(5) as u32;
                    if unsafe { WaitNamedPipeW(wide.as_ptr(), wait_ms.max(1)) } != 0 {
                        h_pipe = unsafe {
                            CreateFileW(
                                wide.as_ptr(),
                                GENERIC_READ | GENERIC_WRITE,
                                0,
                                core::ptr::null(),
                                OPEN_EXISTING,
                                FILE_FLAG_OVERLAPPED,
                                0,
                            )
                        };
                    }
                }
            }
        }
        if h_pipe == INVALID_HANDLE_VALUE {
            return None;
        }
        let _pipe_guard = HandleGuard(h_pipe);

        let h_event = unsafe { CreateEventW(core::ptr::null(), 1, 0, core::ptr::null()) };
        if h_event == 0 {
            return None;
        }
        let _ev_guard = HandleGuard(h_event);

        if !write_all_timed(h_pipe, h_event, req_framed, deadline) {
            return None;
        }
        let mut len_buf = [0u8; 4];
        if !read_exact_timed(h_pipe, h_event, &mut len_buf, deadline) {
            return None;
        }
        let resp_len = u32::from_le_bytes(len_buf) as usize;
        if resp_len == 0 || resp_len > MAX_IPC_FRAME_BYTES {
            return None;
        }
        let mut resp_payload = vec![0u8; resp_len];
        if !read_exact_timed(h_pipe, h_event, &mut resp_payload, deadline) {
            return None;
        }
        Some(resp_payload)
    }
}

#[cfg(not(target_os = "windows"))]
mod win_pipe_client {
    use std::path::Path;

    pub fn query_named_pipe(_pipe_name: &str, _req_framed: &[u8], _timeout_ms: u16) -> Option<Vec<u8>> {
        None
    }

    pub fn is_singleton_mutex_present() -> bool {
        false
    }

    pub fn spawn_windowless_sidecar(_exe: &Path, _model: &Path) -> bool {
        false
    }
}

fn score_via_pipe(cfg: &NeuralRerankerConfig, prefix: &str, cands: &[&str]) -> Option<Vec<f32>> {
    // 若處於連線失敗冷卻窗內，立即跳過（< 0.01 ms），避免拖慢每鍵組字
    if let Ok(g) = state().read() {
        if let Some(failed_at) = g.last_pipe_failure {
            if failed_at.elapsed() < Duration::from_millis(CONNECT_FAIL_BACKOFF_MS) {
                return None;
            }
        }
    }

    let req_id = NEXT_REQ_ID.fetch_add(1, Ordering::Relaxed);
    let framed = encode_request(req_id, cfg.timeout_ms, prefix, cands);
    let payload_opt = win_pipe_client::query_named_pipe(&cfg.pipe_name, &framed, cfg.timeout_ms);
    let Some(payload) = payload_opt else {
        let _ = ensure_sidecar_running_async();
        if let Ok(mut g) = state().write() {
            g.last_pipe_failure = Some(Instant::now());
        }
        return None;
    };
    let (resp_id, status, scores) = decode_response(&payload)?;
    if resp_id != req_id || status != 0 || scores.len() != cands.len() {
        return None;
    }
    Some(scores)
}

/// 漸進式字元前綴樹非葉節點統計器（對齊 `rlnm.rs::score_candidates_trie` 批次張量維度 `m_batch`）
struct PrefixTrieBudget {
    children: Vec<Vec<(char, usize)>>,
    base_prefix_nodes: usize,
}

impl PrefixTrieBudget {
    fn new(prefix: &str) -> Self {
        Self {
            children: vec![Vec::new()],
            base_prefix_nodes: 1 + prefix.chars().count().min(16),
        }
    }

    #[inline]
    fn current_nodes(&self) -> usize {
        self.base_prefix_nodes + self.children.len().saturating_sub(1)
    }

    /// 計算插入 `cand`（僅前 `L - 1` 字元為非葉節點）所新增之節點數
    fn count_additional_nodes(&self, cand: &str) -> usize {
        let len = cand.chars().count();
        if len <= 1 {
            return 0;
        }
        let mut cur = 0usize;
        let mut diverged = false;
        let mut added = 0usize;
        for ch in cand.chars().take(len - 1) {
            if !diverged {
                if let Some(&(_, next_idx)) = self.children[cur].iter().find(|&&(c, _)| c == ch) {
                    cur = next_idx;
                    continue;
                }
                diverged = true;
            }
            added += 1;
        }
        added
    }

    fn insert(&mut self, cand: &str) {
        let len = cand.chars().count();
        if len <= 1 {
            return;
        }
        let mut cur = 0usize;
        for ch in cand.chars().take(len - 1) {
            if let Some(&(_, next_idx)) = self.children[cur].iter().find(|&&(c, _)| c == ch) {
                cur = next_idx;
            } else {
                let next_idx = self.children.len();
                self.children.push(Vec::new());
                self.children[cur].push((ch, next_idx));
                cur = next_idx;
            }
        }
    }
}

/// 估算給定前文與候選清單於 `RlnmModel::score_candidates_trie` 中所需之非葉節點總數（含 `<bos>` 與 `prefix`）
pub fn estimate_trie_non_leaf_nodes(prefix: &str, cands: &[&str]) -> usize {
    let mut budget = PrefixTrieBudget::new(prefix);
    for &c in cands {
        budget.insert(c);
    }
    budget.current_nodes()
}

/// 依 `max_trie_nodes` 預算由前至後漸進篩選候選子集（保底保留前 2 名），回傳 `(入選索引, 實際非葉節點數)`
pub fn select_candidates_with_trie_budget(
    prefix: &str,
    cands: &[&str],
    max_trie_nodes: usize,
) -> (Vec<usize>, usize) {
    let mut budget = PrefixTrieBudget::new(prefix);
    let mut kept = Vec::with_capacity(cands.len());
    for (idx, &c) in cands.iter().enumerate() {
        let extra = budget.count_additional_nodes(c);
        if kept.len() >= 2 && budget.current_nodes() + extra > max_trie_nodes {
            continue;
        }
        budget.insert(c);
        kept.push(idx);
    }
    (kept, budget.current_nodes())
}

/// D1-B 5 折交叉驗證校準之結構融合加分（無須變更 IPC 協定，隨 `lambda` 平滑縮放）
#[inline]
pub fn structural_fusion_bonus(nodes: &[LatticeNode], discourse_delta: f64, lambda: f64) -> f64 {
    if nodes.is_empty() || lambda <= 0.0 {
        return 0.0;
    }
    let scale = (lambda / 2.25).clamp(0.0, 2.0);
    let wc = nodes.len() as f64;
    let single_char = nodes.iter().filter(|n| n.word.chars().count() == 1).count() as f64;
    let min_w = nodes.iter().map(|n| n.weight).min().unwrap_or(0) as f64;
    let avg_w = nodes.iter().map(|n| n.weight as f64).sum::<f64>() / wc;
    scale
        * (-0.0545 * wc
            - 0.2223 * single_char
            + 1.7879 * min_w.max(0.0).ln_1p()
            + 5.5713 * avg_w.max(0.0).ln_1p()
            + 2.3295 * discourse_delta)
}

#[derive(Debug, Clone)]
struct DisjointIsland {
    diff_start: usize,
    diff_end: usize,
    replacement_nodes: Vec<LatticeNode>,
    base_delta: f64,
    disc_delta: f64,
    gain: f64,
}

/// 抽取候選 `cand` 相對於 `p_star` 之單一連續差異區間 `[diff_start, diff_end)`；若屬多段分散或整句替換則回傳 `None`
fn extract_single_diff_island(
    p_star: &[LatticeNode],
    cand: &[LatticeNode],
    base_delta: f64,
    disc_delta: f64,
    gain: f64,
) -> Option<DisjointIsland> {
    let n_syl = p_star.last().map(|n| n.end_idx).unwrap_or(0);
    if n_syl == 0 || cand.last().map(|n| n.end_idx) != Some(n_syl) {
        return None;
    }
    let eq_node = |a: &LatticeNode, b: &LatticeNode| -> bool {
        a.start_idx == b.start_idx && a.end_idx == b.end_idx && a.word == b.word
    };
    let mut pref = 0usize;
    while pref < p_star.len() && pref < cand.len() && eq_node(&p_star[pref], &cand[pref]) {
        pref += 1;
    }
    let mut suff = 0usize;
    while pref + suff < p_star.len()
        && pref + suff < cand.len()
        && eq_node(
            &p_star[p_star.len() - 1 - suff],
            &cand[cand.len() - 1 - suff],
        )
    {
        suff += 1;
    }
    let p_end_idx = p_star.len() - suff;
    let c_end_idx = cand.len() - suff;
    if pref >= p_end_idx || pref >= c_end_idx {
        return None;
    }
    let diff_start = p_star[pref].start_idx;
    let diff_end = p_star[p_end_idx - 1].end_idx;
    if cand[pref].start_idx != diff_start || cand[c_end_idx - 1].end_idx != diff_end {
        return None;
    }
    if diff_start >= diff_end || (diff_end - diff_start) >= n_syl {
        return None;
    }
    Some(DisjointIsland {
        diff_start,
        diff_end,
        replacement_nodes: cand[pref..c_end_idx].to_vec(),
        base_delta,
        disc_delta,
        gain,
    })
}

/// D2-B 互不重疊多島重組：於基準 `P*` 上貪婪拼接最多 3 組互不重疊（且間隔 >= 1 音節）之已改善區間，
/// 回傳 `(重組節點序列, 估計基準分數累加, 估計語篇分數累加)`。
pub fn try_build_disjoint_multi_island_recombination(
    p_star: &[LatticeNode],
    p_star_base: f64,
    p_star_disc: f64,
    improved_candidates: &[(&[LatticeNode], f64, f64, f64)],
) -> Option<(Vec<LatticeNode>, f64, f64)> {
    let n_syl = p_star.last().map(|n| n.end_idx).unwrap_or(0);
    if n_syl < 4 || improved_candidates.len() < 2 {
        return None;
    }

    let mut islands: Vec<DisjointIsland> = improved_candidates
        .iter()
        .filter_map(|&(nodes, c_base, c_disc, gain)| {
            if gain <= 0.5 {
                return None;
            }
            extract_single_diff_island(p_star, nodes, c_base - p_star_base, c_disc - p_star_disc, gain)
        })
        .collect();

    if islands.len() < 2 {
        return None;
    }
    islands.sort_by(|a, b| {
        b.gain
            .partial_cmp(&a.gain)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut picked: Vec<DisjointIsland> = Vec::with_capacity(3);
    for isl in islands {
        let disjoint = picked.iter().all(|p| {
            isl.diff_end + 1 <= p.diff_start || p.diff_end + 1 <= isl.diff_start
        });
        if disjoint {
            picked.push(isl);
            if picked.len() >= 3 {
                break;
            }
        }
    }
    if picked.len() < 2 {
        return None;
    }
    picked.sort_by_key(|p| p.diff_start);

    let mut recomb_nodes: Vec<LatticeNode> = Vec::with_capacity(p_star.len() + 4);
    let mut est_base = p_star_base;
    let mut est_disc = p_star_disc;
    let mut p_idx = 0usize;
    let mut i_idx = 0usize;

    while p_idx < p_star.len() {
        if i_idx < picked.len() && p_star[p_idx].start_idx == picked[i_idx].diff_start {
            let isl = &picked[i_idx];
            recomb_nodes.extend(isl.replacement_nodes.iter().cloned());
            est_base += isl.base_delta;
            est_disc += isl.disc_delta;
            while p_idx < p_star.len() && p_star[p_idx].start_idx < isl.diff_end {
                p_idx += 1;
            }
            i_idx += 1;
        } else {
            recomb_nodes.push(p_star[p_idx].clone());
            p_idx += 1;
        }
    }
    if i_idx != picked.len() {
        return None;
    }

    // 驗證合成節點序列完整無斷層涵蓋 `0..n_syl`
    let mut cur_pos = 0usize;
    for nd in &recomb_nodes {
        if nd.start_idx != cur_pos || nd.end_idx <= nd.start_idx {
            return None;
        }
        cur_pos = nd.end_idx;
    }
    if cur_pos != n_syl {
        return None;
    }

    Some((recomb_nodes, est_base, est_disc))
}

/// 若已啟用（且在行程內模型或背景 Sidecar 可回應於逾時限制內），以多特徵結構＋神經分數重排競爭候選（含 D2-B 多島重組）；
/// 否則回傳 `None` 讓呼端無縫沿用既定 Top-1 結果。
pub fn rerank_nbest_if_enabled(
    prefix_word: Option<&str>,
    candidates: &[(Vec<LatticeNode>, f64)],
) -> Option<(Vec<LatticeNode>, f64)> {
    rerank_nbest_if_enabled_with_discourse(prefix_word, candidates, None)
}

/// 支援傳入語篇代詞重排加分之神經重排序介面（D1-B ＋ D2-B）
pub fn rerank_nbest_if_enabled_with_discourse(
    prefix_word: Option<&str>,
    candidates: &[(Vec<LatticeNode>, f64)],
    discourse_deltas: Option<&[f64]>,
) -> Option<(Vec<LatticeNode>, f64)> {
    if candidates.len() < 2 {
        return None;
    }

    let (cfg, inproc_opt) = {
        let g = state().read().ok()?;
        if !g.cfg.enabled {
            return None;
        }
        (g.cfg.clone(), g.in_process_model.clone())
    };
    if cfg.lambda <= 0.0 {
        return None;
    }
    let eff_lambda = cfg.lambda * (2.3024 / 2.25);

    // 依基礎分數由高至低排序索引，確保高分候選優先佔用 Trie 節點延遲預算
    let mut ordered: Vec<usize> = (0..candidates.len()).collect();
    ordered.sort_by(|&a, &b| {
        candidates[b]
            .1
            .partial_cmp(&candidates[a].1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let scoped_prefix = TLS_SCOPED_PREFIX.with(|cell| cell.borrow().clone());
    let prefix_storage: String = match scoped_prefix.as_deref() {
        Some(sc) if !sc.is_empty() => merge_prefix_with_overlap(
            sc,
            prefix_word.unwrap_or(""),
            MAX_CROSS_SENTENCE_PREFIX_CHARS,
        ),
        _ => prefix_word.unwrap_or("").to_string(),
    };
    let prefix_clean = prefix_storage.as_str();
    let top1_score = candidates[ordered[0]].1;
    let mut active_indices = Vec::with_capacity(candidates.len());
    let mut cand_texts: Vec<String> = Vec::with_capacity(candidates.len());
    let mut trie_budget = PrefixTrieBudget::new(prefix_clean);

    for idx in ordered {
        let (nodes, sc) = &candidates[idx];
        if top1_score - *sc > cfg.score_margin {
            break;
        }
        let txt: String = nodes.iter().map(|n| n.word.as_str()).collect();
        let extra = trie_budget.count_additional_nodes(&txt);
        if active_indices.len() >= 2
            && trie_budget.current_nodes() + extra > cfg.max_trie_nodes
        {
            continue;
        }
        trie_budget.insert(&txt);
        active_indices.push(idx);
        cand_texts.push(txt);
    }
    if active_indices.len() < 2 {
        return None;
    }

    let cand_refs: Vec<&str> = cand_texts.iter().map(|s| s.as_str()).collect();
    let neural_scores = if let Some(ref model) = inproc_opt {
        let model_key = Arc::as_ptr(model) as usize;
        TLS_INPROC_CACHES.with(|cell| {
            let mut map = cell.borrow_mut();
            let cache = map.entry(model_key).or_insert_with(|| model.alloc_kv_cache());
            model.score_candidates_trie(prefix_clean, &cand_refs, cache, None)
        })
    } else {
        score_via_pipe(&cfg, prefix_clean, &cand_refs)?
    };

    if neural_scores.len() != active_indices.len() {
        return None;
    }

    let mut best_pos = 0usize;
    let mut best_idx = active_indices[0];
    let mut best_total = f64::NEG_INFINITY;
    let mut pass1_totals: Vec<f64> = Vec::with_capacity(active_indices.len());
    for (pos, (&cand_idx, &nl_sc)) in active_indices.iter().zip(neural_scores.iter()).enumerate() {
        let disc_d = discourse_deltas.and_then(|v| v.get(cand_idx).copied()).unwrap_or(0.0);
        let total = if nl_sc.is_finite() {
            candidates[cand_idx].1
                + eff_lambda * (nl_sc as f64)
                + structural_fusion_bonus(&candidates[cand_idx].0, disc_d, cfg.lambda)
        } else {
            f64::NEG_INFINITY
        };
        pass1_totals.push(total);
        if total > best_total {
            best_total = total;
            best_idx = cand_idx;
            best_pos = pos;
        }
    }

    if best_total == f64::NEG_INFINITY {
        return None;
    }

    // D2-B：若有多個互不重疊區間同時勝過 P*，合成重組候選並僅執行 1 次輕量 2 候選複評（善用前綴 KV 快取）
    let p_star_idx = active_indices[0];
    let p_star_total = pass1_totals[0];
    let p_star_disc = discourse_deltas.and_then(|v| v.get(p_star_idx).copied()).unwrap_or(0.0);
    if p_star_total.is_finite() {
        let mut improved: Vec<(&[LatticeNode], f64, f64, f64)> = Vec::new();
        for (pos, &cand_idx) in active_indices.iter().enumerate().skip(1) {
            let tot = pass1_totals[pos];
            if tot > p_star_total + 0.5 {
                let c_disc = discourse_deltas.and_then(|v| v.get(cand_idx).copied()).unwrap_or(0.0);
                improved.push((
                    &candidates[cand_idx].0,
                    candidates[cand_idx].1,
                    c_disc,
                    tot - p_star_total,
                ));
            }
        }
        if let Some((recomb_nodes, recomb_base, recomb_disc)) = try_build_disjoint_multi_island_recombination(
            &candidates[p_star_idx].0,
            candidates[p_star_idx].1,
            p_star_disc,
            &improved,
        ) {
            let recomb_text: String = recomb_nodes.iter().map(|n| n.word.as_str()).collect();
            if !cand_texts.iter().any(|t| t == &recomb_text) {
                let pair_refs = [cand_texts[best_pos].as_str(), recomb_text.as_str()];
                let pair_scores = if let Some(ref model) = inproc_opt {
                    let model_key = Arc::as_ptr(model) as usize;
                    Some(TLS_INPROC_CACHES.with(|cell| {
                        let mut map = cell.borrow_mut();
                        let cache = map.entry(model_key).or_insert_with(|| model.alloc_kv_cache());
                        model.score_candidates_trie(prefix_clean, &pair_refs, cache, None)
                    }))
                } else {
                    score_via_pipe(&cfg, prefix_clean, &pair_refs)
                };
                if let Some(scores) = pair_scores {
                    if scores.len() == 2 && scores[1].is_finite() {
                        let recomb_total = recomb_base
                            + eff_lambda * (scores[1] as f64)
                            + structural_fusion_bonus(&recomb_nodes, recomb_disc, cfg.lambda);
                        if recomb_total > best_total {
                            return Some((recomb_nodes, recomb_total));
                        }
                    }
                }
            }
        }
    }

    Some((candidates[best_idx].0.clone(), best_total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_frame_roundtrip_and_malformed_rejection() {
        let cands = ["再補上三句", "在補上三句", "已取消南下旅程"];
        let framed = encode_request(42, 15, "我們", &cands);
        assert!(framed.len() > 4);
        let plen = u32::from_le_bytes(framed[0..4].try_into().unwrap()) as usize;
        assert_eq!(plen, framed.len() - 4);

        let (rid, tmo, pref, dec_cands) =
            decode_request(&framed[4..]).expect("valid RNQ1 frame must decode");
        assert_eq!(rid, 42);
        assert_eq!(tmo, 15);
        assert_eq!(pref, "我們");
        assert_eq!(dec_cands, vec!["再補上三句", "在補上三句", "已取消南下旅程"]);

        assert!(decode_request(b"BAD1\x00\x00").is_none());
        assert!(decode_request(&framed[4..12]).is_none());

        let resp = encode_response(42, 0, &[-25.4, -29.7, -42.1]);
        let rlen = u32::from_le_bytes(resp[0..4].try_into().unwrap()) as usize;
        assert_eq!(rlen, resp.len() - 4);
        let (rrid, status, scores) = decode_response(&resp[4..]).expect("valid RNR1 frame must decode");
        assert_eq!(rrid, 42);
        assert_eq!(status, 0);
        assert_eq!(scores.len(), 3);
        assert!((scores[0] - (-25.4)).abs() < 1e-5);
        assert!(decode_response(b"RNR1truncated").is_none());
    }

    #[test]
    fn test_missing_pipe_graceful_fallback_and_backoff() {
        set_pipe_name_for_test(r"\\.\pipe\RuhI_NonExistent_TestPipe_99999");
        set_runtime_config(true, 0.5, 15);

        let mk = |w: &str| vec![LatticeNode::new(0, w.chars().count(), w, 500, "")];
        let cands = vec![(mk("在補上三句"), 95.0), (mk("再補上三句"), 90.0)];

        let t0 = Instant::now();
        let res1 = rerank_nbest_if_enabled(None, &cands);
        let el1 = t0.elapsed();
        assert!(res1.is_none());
        assert!(el1 < Duration::from_millis(20), "missing pipe took too long: {:?}", el1);

        // 退避快取生效期間應極速回傳 (< 1 ms)
        let t1 = Instant::now();
        let res2 = rerank_nbest_if_enabled(None, &cands);
        let el2 = t1.elapsed();
        assert!(res2.is_none());
        assert!(el2 < Duration::from_millis(5), "backoff should be immediate: {:?}", el2);

        // 還原預設關閉狀態
        set_pipe_name_for_test(DEFAULT_PIPE_NAME);
        set_runtime_config(false, 0.5, 15);
    }

    #[test]
    fn test_sidecar_paths_and_missing_binary_safe_noop() {
        set_runtime_config(true, 0.5, 25);
        set_sidecar_paths(
            Some(std::path::PathBuf::from(r"C:\__ruhi_nonexistent__\ruhi_reranker.exe")),
            Some(std::path::PathBuf::from(r"C:\__ruhi_nonexistent__\rlnm_zh_tw.bin")),
        );
        assert!(!ensure_sidecar_running_async());
        set_sidecar_paths(None, None);
        set_runtime_config(false, 0.5, 25);
    }

    #[test]
    fn test_trie_node_budget_caps_long_candidates_safely_261125() {
        // 驗證多組高度分歧長句在 max_trie_nodes 預算下仍至少保留 Top-2 且總非葉節點數不超標
        let prefix = "我們";
        let cands = [
            "此刻視力模糊難以便是滑鼠底下大概只有",
            "此刻視力模糊難以辨識滑鼠底下大概只有",
            "斯客事理模式難以便是化屬地夏大改只有",
            "思科事例模胡難以便是畫書底夏大該只有",
            "私課試力磨糊難以便是話術底霞大蓋只有",
        ];
        let full_nodes = estimate_trie_non_leaf_nodes(prefix, &cands);
        assert!(full_nodes > 50);

        let mut budget = PrefixTrieBudget::new(prefix);
        let mut kept: Vec<&str> = Vec::new();
        let max_nodes = 45usize;
        for &c in &cands {
            let extra = budget.count_additional_nodes(c);
            if kept.len() >= 2 && budget.current_nodes() + extra > max_nodes {
                continue;
            }
            budget.insert(c);
            kept.push(c);
        }
        assert!(kept.len() >= 2 && kept.len() < cands.len());
        assert!(budget.current_nodes() <= max_nodes);

        // 即使 max_nodes 極小，前 2 名候選仍應保底入選以維持比較資格
        let mut tiny_budget = PrefixTrieBudget::new(prefix);
        let mut tiny_kept: Vec<&str> = Vec::new();
        for &c in &cands {
            let extra = tiny_budget.count_additional_nodes(c);
            if tiny_kept.len() >= 2 && tiny_budget.current_nodes() + extra > 10 {
                continue;
            }
            tiny_budget.insert(c);
            tiny_kept.push(c);
        }
        assert_eq!(tiny_kept.len(), 2);
    }

    #[test]
    fn test_disjoint_multi_island_recombination_helper_261125() {
        let mk = |s: usize, e: usize, w: &str| LatticeNode::new(s, e, w, 400, "");
        // P* = "在補上 這台 以取消 南下 行程" (total syllables = 10)
        let p_star = vec![
            mk(0, 1, "在"),
            mk(1, 3, "補上"),
            mk(3, 5, "這台"),
            mk(5, 8, "以取消"),
            mk(8, 10, "行程"),
        ];
        // Island 1: "在" -> "再" ([0, 1))
        let c1 = vec![
            mk(0, 1, "再"),
            mk(1, 3, "補上"),
            mk(3, 5, "這台"),
            mk(5, 8, "以取消"),
            mk(8, 10, "行程"),
        ];
        // Island 2: "以取消" -> "已取消" ([5, 8))
        let c2 = vec![
            mk(0, 1, "在"),
            mk(1, 3, "補上"),
            mk(3, 5, "這台"),
            mk(5, 8, "已取消"),
            mk(8, 10, "行程"),
        ];
        // 重疊對比：涵蓋 [0, 3) 與 Island 1 重疊，必須被拒絕（若僅傳 c1 + overlap，不得合成）
        let c_overlap = vec![
            mk(0, 3, "再捕尚"),
            mk(3, 5, "這台"),
            mk(5, 8, "以取消"),
            mk(8, 10, "行程"),
        ];

        let res = try_build_disjoint_multi_island_recombination(
            &p_star,
            100.0,
            0.0,
            &[(&c1, 98.0, 0.0, 4.0), (&c2, 97.5, 1.0, 3.5)],
        )
        .expect("two disjoint islands separated by >=1 syllable should recombine");
        let text: String = res.0.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(text, "再補上這台已取消行程");
        assert!((res.1 - 95.5).abs() < 1e-6);
        assert!((res.2 - 1.0).abs() < 1e-6);

        let rej = try_build_disjoint_multi_island_recombination(
            &p_star,
            100.0,
            0.0,
            &[(&c1, 98.0, 0.0, 4.0), (&c_overlap, 97.0, 0.0, 3.0)],
        );
        assert!(rej.is_none(), "overlapping diff islands must not recombine");
    }
}
