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

#[derive(Debug, Clone)]
pub struct NeuralRerankerConfig {
    pub enabled: bool,
    pub lambda: f64,
    pub timeout_ms: u16,
    pub pipe_name: String,
    pub min_syllables: usize,
    pub score_margin: f64,
}

impl Default for NeuralRerankerConfig {
    fn default() -> Self {
        let env_enabled = std::env::var("RUHI_NEURAL_RERANK")
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);
        Self {
            enabled: env_enabled,
            lambda: 0.50,
            timeout_ms: 15,
            pipe_name: DEFAULT_PIPE_NAME.to_string(),
            min_syllables: 4,
            score_margin: 15.0,
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
static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static TLS_INPROC_CACHES: RefCell<HashMap<usize, RlnmKvCache>> = RefCell::new(HashMap::new());
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

/// 若已啟用（且在行程內模型或背景 Sidecar 可回應於逾時限制內），以 `base_score + lambda * neural_logp` 重排競爭候選；
/// 否則回傳 `None` 讓呼端無縫沿用既定 Top-1 結果。
pub fn rerank_nbest_if_enabled(
    prefix_word: Option<&str>,
    candidates: &[(Vec<LatticeNode>, f64)],
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

    let top1_score = candidates.iter().map(|(_, s)| *s).fold(f64::NEG_INFINITY, f64::max);
    let mut active_indices = Vec::with_capacity(candidates.len());
    let mut cand_texts: Vec<String> = Vec::with_capacity(candidates.len());
    for (idx, (nodes, sc)) in candidates.iter().enumerate() {
        if top1_score - *sc <= cfg.score_margin {
            active_indices.push(idx);
            let txt: String = nodes.iter().map(|n| n.word.as_str()).collect();
            cand_texts.push(txt);
        }
    }
    if active_indices.len() < 2 {
        return None;
    }

    let cand_refs: Vec<&str> = cand_texts.iter().map(|s| s.as_str()).collect();
    let prefix_clean = prefix_word.unwrap_or("");
    let neural_scores = if let Some(model) = inproc_opt {
        let model_key = Arc::as_ptr(&model) as usize;
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

    let mut best_idx = active_indices[0];
    let mut best_total = f64::NEG_INFINITY;
    for (&cand_idx, &nl_sc) in active_indices.iter().zip(neural_scores.iter()) {
        if !nl_sc.is_finite() {
            continue;
        }
        let total = candidates[cand_idx].1 + cfg.lambda * (nl_sc as f64);
        if total > best_total {
            best_total = total;
            best_idx = cand_idx;
        }
    }

    if best_total > f64::NEG_INFINITY {
        Some((candidates[best_idx].0.clone(), best_total))
    } else {
        None
    }
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
}
