/// RuhI (如喜) Windows 11 TSF Native Text Service Implementation
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::System::DataExchange::{OpenClipboard, CloseClipboard, GetClipboardData, SetClipboardData, EmptyClipboard};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

#[derive(Clone, Debug)]
pub struct EditorSnapshot {
    pub pending: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub syllables: Vec<Option<String>>,
    pub segments: Vec<(usize, usize, String, String)>,
    pub manual_cuts: std::collections::HashSet<usize>,
    pub pinned: std::collections::HashSet<usize>,
}

pub(crate) fn normalize_clipboard_paste_text(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn normalize_clipboard_copy_text(text: &str) -> String {
    normalize_clipboard_paste_text(text).replace('\n', "\r\n")
}

fn get_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let handle = GetClipboardData(13); // CF_UNICODETEXT = 13
        let text = if let Ok(h) = handle {
            if !h.0.is_null() {
                let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(h.0)) as *const u16;
                if !ptr.is_null() {
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(ptr, len);
                    let s = String::from_utf16_lossy(slice);
                    let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(h.0));
                    Some(s)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let _ = CloseClipboard();
        text
    }
}

fn set_clipboard_text(text: &str) -> bool {
    let normalized = normalize_clipboard_copy_text(text);
    let wide: Vec<u16> = normalized.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * std::mem::size_of::<u16>();
    unsafe {
        let h_global = match GlobalAlloc(GMEM_MOVEABLE, byte_len) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let ptr = GlobalLock(h_global) as *mut u16;
        if ptr.is_null() {
            let _ = windows::Win32::Foundation::GlobalFree(h_global);
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(h_global);

        if OpenClipboard(None).is_err() {
            let _ = windows::Win32::Foundation::GlobalFree(h_global);
            return false;
        }
        let _ = EmptyClipboard();
        let res = SetClipboardData(13, windows::Win32::Foundation::HANDLE(h_global.0));
        let _ = CloseClipboard();
        if res.is_err() {
            let _ = windows::Win32::Foundation::GlobalFree(h_global);
            false
        } else {
            true
        }
    }
}

use ruhi::engine::RuhiEngine;
use ruhi::lexicon::LexiconManager;
use ruhi::fsm::HsuSyllable;
use ruhi::router::LatticeDAG;
use crate::edit_session::{EditSessionAction, RuhiEditSession};
use crate::logger;

pub const CARET_UNPROBED: i32 = i32::MIN;

static LAST_CARET_POS: Mutex<(i32, i32)> = Mutex::new((CARET_UNPROBED, CARET_UNPROBED));

pub fn set_last_caret_pos(x: i32, y: i32) {
    if let Ok(mut lock) = LAST_CARET_POS.lock() {
        *lock = (x, y);
    }
}

pub fn get_last_caret_pos() -> (i32, i32) {
    LAST_CARET_POS
        .lock()
        .map(|p| *p)
        .unwrap_or((CARET_UNPROBED, CARET_UNPROBED))
}

/// 偵測當前聚焦之 Win32 視窗是否為密碼輸入控制項 (`ES_PASSWORD` 或 `EM_GETPASSWORDCHAR != 0`)
/// 僅針對視窗類別名稱含 `Edit` 之文字輸入控制項生效，確保在密碼框自動直通英數且絕不記錄個人修訂日誌。
pub fn is_focused_password_control() -> bool {
    unsafe {
        let hwnd = GetFocus();
        if hwnd.0.is_null() {
            return false;
        }
        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class_buf);
        if len <= 0 {
            return false;
        }
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]).to_ascii_lowercase();
        if !class_name.contains("edit") {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if (style & 0x0020) != 0 {
            // ES_PASSWORD = 0x0020
            return true;
        }
        // EM_GETPASSWORDCHAR = 0x00D2
        let pwd_char = SendMessageW(hwnd, 0x00D2, WPARAM(0), LPARAM(0));
        pwd_char.0 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostNewlineKind {
    /// 傳統 Win32 `Edit` / WinForms `EDIT` 控制項：內部 TextStore 以 `\r\n`（2 個 ACP 碼元）表示換行
    Win32EditCrlf,
    /// Windows 11 Notepad（`RichEditD2DPT` / `NotepadTextBox`）與 RichEdit 系列（`RichEdit*` / `msftedit*`）：
    /// 內部 `CTxtStory` 一律以單一 `\r`（1 個 ACP 碼元）儲存段落換行
    RichEditCr,
    /// 其餘宿主（Chromium/Electron、Firefox、LibreOffice `ITextStoreAnchor`、CoreWindow 等）：
    /// 以 `\n` 搭配分段送出（`split_lf_commit_chunks`）驅動原生換行與跨段落游標推進
    GenericLfChunked,
}

pub(crate) fn classify_host_newline_kind(class_name: &str) -> HostNewlineKind {
    let clean = class_name
        .trim_matches(|c: char| c.is_whitespace() || c == '\0')
        .to_ascii_lowercase();
    if clean == "notepadtextbox" || clean.contains("richedit") || clean.contains("msftedit") {
        HostNewlineKind::RichEditCr
    } else if clean == "edit" || clean.contains(".edit.") {
        HostNewlineKind::Win32EditCrlf
    } else {
        HostNewlineKind::GenericLfChunked
    }
}

#[cfg(test)]
pub(crate) fn class_name_uses_crlf(class_name: &str) -> bool {
    classify_host_newline_kind(class_name) == HostNewlineKind::Win32EditCrlf
}

pub(crate) fn normalize_commit_newlines_for_kind(text: &str, kind: HostNewlineKind) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    match kind {
        HostNewlineKind::Win32EditCrlf => unified.replace('\n', "\r\n"),
        HostNewlineKind::RichEditCr => unified.replace('\n', "\r"),
        HostNewlineKind::GenericLfChunked => unified,
    }
}

#[cfg(test)]
pub(crate) fn normalize_commit_newlines(text: &str, use_crlf: bool) -> String {
    normalize_commit_newlines_for_kind(
        text,
        if use_crlf {
            HostNewlineKind::Win32EditCrlf
        } else {
            HostNewlineKind::GenericLfChunked
        },
    )
}

pub(crate) fn split_lf_commit_chunks(text: &str) -> Vec<String> {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = unified.split('\n').collect();
    if lines.len() <= 1 {
        return vec![unified];
    }
    let mut chunks = Vec::new();
    chunks.push(lines[0].to_string());
    for k in 1..lines.len() {
        chunks.push("\n".to_string());
        if !lines[k].is_empty() {
            chunks.push(lines[k].to_string());
        }
    }
    chunks
}

fn classify_hwnd_newline_kind(hwnd: HWND) -> Option<HostNewlineKind> {
    unsafe {
        if hwnd.0.is_null() {
            return None;
        }
        let mut class_buf = [0u16; 128];
        let len = GetClassNameW(hwnd, &mut class_buf);
        if len <= 0 {
            return None;
        }
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
        Some(classify_host_newline_kind(&class_name))
    }
}

pub(crate) fn context_host_newline_kind(ctx: &ITfContext) -> HostNewlineKind {
    unsafe {
        if let Ok(view) = ctx.GetActiveView() {
            if let Ok(hwnd) = view.GetWnd() {
                if let Some(kind) = classify_hwnd_newline_kind(hwnd) {
                    if kind != HostNewlineKind::GenericLfChunked {
                        return kind;
                    }
                }
            }
        }
        let focus_hwnd = GetFocus();
        classify_hwnd_newline_kind(focus_hwnd).unwrap_or(HostNewlineKind::GenericLfChunked)
    }
}

pub(crate) fn should_skip_joint_reeval_on_key(
    _pre_has_pending: bool,
    post_has_pending: bool,
    pre_syl_len: usize,
    post_syl_len: usize,
    pre_composed_len: usize,
    post_composed_len: usize,
    action: &str,
) -> bool {
    if action.is_empty() || action == "none" {
        return true;
    }
    if post_has_pending && pre_syl_len == post_syl_len && pre_composed_len == post_composed_len {
        return true;
    }
    false
}

pub(crate) fn compute_joint_reeval_window(
    block_start: usize,
    cur: usize,
    eng_syl_len: usize,
    block_end: usize,
    segs: &[(usize, usize, String, String)],
    max_left: usize,
    max_right: usize,
) -> (usize, usize) {
    if block_end <= block_start {
        return (block_start, block_end);
    }
    if block_end - block_start <= max_left + max_right && eng_syl_len == 0 {
        return (block_start, block_end);
    }

    let raw_left = cur.saturating_sub(max_left).max(block_start);
    let mut win_start = raw_left;
    for s in segs {
        if s.0 < raw_left && raw_left < s.1 {
            win_start = s.0.max(block_start);
            break;
        }
    }
    if win_start > cur {
        win_start = cur;
    }

    let right_allowance = max_right.saturating_sub(eng_syl_len);
    let raw_right = (cur + right_allowance).min(block_end);
    let mut win_end = raw_right;
    for s in segs {
        if s.0 < raw_right && raw_right < s.1 {
            win_end = s.1.min(block_end);
            break;
        }
    }
    if win_end < cur {
        win_end = cur;
    }

    (win_start.clamp(block_start, cur), win_end.clamp(cur, block_end))
}

pub(crate) fn upgrade_an_before_ascii_key_in_buffers(
    chars: &mut [char],
    syls: &[Option<String>],
    _segments: &[(usize, usize, String, String)],
    pinned: &std::collections::HashSet<usize>,
    cur: usize,
) -> bool {
    let c = cur.min(chars.len());
    if c == 0 {
        return false;
    }

    let mut token_end = c;
    while token_end > 0 && chars[token_end - 1].is_whitespace() {
        token_end -= 1;
    }
    if token_end == 0 {
        return false;
    }

    let mut token_start = token_end;
    while token_start > 0 {
        let ch = chars[token_start - 1];
        if ch.is_ascii_alphanumeric() || ch == '+' || ch == '_' {
            token_start -= 1;
        } else {
            break;
        }
    }
    if token_start >= token_end {
        return false;
    }

    let token_slice = &chars[token_start..token_end];
    let token_str: String = token_slice.iter().collect();
    let token_lower = token_str.to_ascii_lowercase();

    const RECOGNIZED_KEYS: &[&str] = &[
        "shift", "enter", "return", "tab", "ctrl", "control", "alt", "esc", "escape",
        "space", "backspace", "del", "delete", "insert", "ins", "home", "end",
        "pageup", "pgup", "pagedown", "pgdn", "win", "cmd", "capslock", "caps",
        "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
    ];

    let is_matched_key = if let Some(plus_idx) = token_lower.find('+') {
        let prefix = &token_lower[..plus_idx];
        let suffix = &token_lower[plus_idx + 1..];
        RECOGNIZED_KEYS.contains(&prefix) || RECOGNIZED_KEYS.contains(&suffix)
    } else {
        RECOGNIZED_KEYS.contains(&token_lower.as_str())
    };

    if !is_matched_key {
        return false;
    }

    let mut han_pos = token_start;
    if han_pos > 0 && chars[han_pos - 1] == ' ' {
        han_pos -= 1;
    }
    if han_pos == 0 {
        return false;
    }
    let han_idx = han_pos - 1;

    let target_ch = chars[han_idx];
    if target_ch != '案' && target_ch != '岸' && target_ch != '暗' {
        return false;
    }

    // 1) 讀音驗證：若有指定注音讀音，必須為 ㄢˋ
    if let Some(Some(ref s)) = syls.get(han_idx) {
        if s != "ㄢˋ" {
            return false;
        }
    }

    // 2) 釘選防護：使用者明確釘選不改
    if pinned.contains(&han_idx) {
        return false;
    }

    // 3) 已知名詞前綴複合詞防護
    const NOUN_COMPOUNDS: &[&str] = &[
        "方案", "專案", "檔案", "報案", "提案", "議案", "草案", "備案",
        "結案", "定案", "立案", "文案", "教案", "個案", "重案", "大案",
        "小案", "本案", "此案", "該案", "舊案", "新案", "懸案", "疑案",
        "公案", "案件", "案由", "對岸", "彼岸", "兩岸", "海岸", "河岸",
        "上岸", "靠岸", "登岸", "黑暗", "陰暗", "灰暗", "明暗", "幽暗",
    ];

    if han_idx > 0 {
        let prev_ch = chars[han_idx - 1];
        let pair: String = [prev_ch, target_ch].iter().collect();
        if NOUN_COMPOUNDS.contains(&pair.as_str()) {
            return false;
        }
    }
    if han_idx + 1 < chars.len() {
        let next_ch = chars[han_idx + 1];
        let pair: String = [target_ch, next_ch].iter().collect();
        if NOUN_COMPOUNDS.contains(&pair.as_str()) {
            return false;
        }
    }

    chars[han_idx] = '按';
    true
}

static LAST_CONFIG_CHECK: Mutex<Option<(std::time::Instant, std::time::SystemTime, RuhiConfig)>> = Mutex::new(None);
static INSTANCE_FC_MTIMES: Mutex<Option<std::collections::HashMap<usize, std::time::SystemTime>>> = Mutex::new(None);
static INSTANCE_TPL_MTIMES: Mutex<Option<std::collections::HashMap<usize, std::time::SystemTime>>> = Mutex::new(None);
static CURRENT_DLL_INSTANCES: Mutex<Option<std::collections::HashSet<usize>>> = Mutex::new(None);
static INSTANCE_LEX_MTIMES: Mutex<Option<std::collections::HashMap<usize, std::time::SystemTime>>> = Mutex::new(None);
static TAB_RANGE_POPUP_INSTANCES: std::sync::OnceLock<Mutex<std::collections::HashSet<usize>>> =
    std::sync::OnceLock::new();

pub(crate) fn set_tab_range_popup_active(inst_key: usize, active: bool) {
    let set_mtx = TAB_RANGE_POPUP_INSTANCES
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if let Ok(mut set) = set_mtx.lock() {
        if active {
            set.insert(inst_key);
        } else {
            set.remove(&inst_key);
        }
    }
}

pub(crate) fn take_tab_range_popup_active(inst_key: usize) -> bool {
    TAB_RANGE_POPUP_INSTANCES
        .get()
        .and_then(|m| m.lock().ok().map(|mut s| s.remove(&inst_key)))
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug)]
struct CompositionEpoch {
    edit_gen: u64,
    focus_gen: u64,
    last_doc_ptr: usize,
}

impl Default for CompositionEpoch {
    fn default() -> Self {
        Self {
            edit_gen: 1,
            focus_gen: 1,
            last_doc_ptr: 0,
        }
    }
}

static INSTANCE_COMPOSITION_EPOCHS: std::sync::OnceLock<
    Mutex<std::collections::HashMap<usize, CompositionEpoch>>,
> = std::sync::OnceLock::new();

pub(crate) fn current_composition_epoch(
    holder: &Arc<Mutex<Option<ITfComposition>>>,
) -> (u64, u64) {
    let comp_key = Arc::as_ptr(holder) as usize;
    let map_mutex = INSTANCE_COMPOSITION_EPOCHS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
    let ep = guard.entry(comp_key).or_default();
    (ep.edit_gen, ep.focus_gen)
}

fn bump_composition_edit_epoch(holder: &Arc<Mutex<Option<ITfComposition>>>) {
    let comp_key = Arc::as_ptr(holder) as usize;
    let map_mutex = INSTANCE_COMPOSITION_EPOCHS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
    let ep = guard.entry(comp_key).or_default();
    ep.edit_gen = ep.edit_gen.wrapping_add(1).max(1);
}

fn bump_composition_focus_epoch(
    holder: &Arc<Mutex<Option<ITfComposition>>>,
    new_doc_ptr: Option<usize>,
) {
    let comp_key = Arc::as_ptr(holder) as usize;
    let map_mutex = INSTANCE_COMPOSITION_EPOCHS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
    let ep = guard.entry(comp_key).or_default();
    match new_doc_ptr {
        Some(ptr) if ptr != 0 => {
            if ep.last_doc_ptr != 0 && ep.last_doc_ptr != ptr {
                ep.focus_gen = ep.focus_gen.wrapping_add(1).max(1);
                ep.edit_gen = ep.edit_gen.wrapping_add(1).max(1);
            }
            ep.last_doc_ptr = ptr;
        }
        _ => {
            ep.focus_gen = ep.focus_gen.wrapping_add(1).max(1);
            ep.edit_gen = ep.edit_gen.wrapping_add(1).max(1);
            ep.last_doc_ptr = 0;
        }
    }
}

fn remove_composition_epoch(comp_key: usize) {
    if let Some(map_mutex) = INSTANCE_COMPOSITION_EPOCHS.get() {
        let mut guard = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(&comp_key);
    }
}

pub(crate) fn is_composition_epoch_valid(
    holder_ptr: usize,
    expected_edit_gen: u64,
    expected_focus_gen: u64,
    is_commit: bool,
) -> bool {
    if expected_edit_gen == 0 && expected_focus_gen == 0 {
        return true;
    }
    let Some(map_mutex) = INSTANCE_COMPOSITION_EPOCHS.get() else {
        return false;
    };
    let guard = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
    let Some(ep) = guard.get(&holder_ptr) else {
        return false;
    };
    if is_commit {
        ep.focus_gen == expected_focus_gen
    } else {
        ep.edit_gen == expected_edit_gen && ep.focus_gen == expected_focus_gen
    }
}

thread_local! {
    static TESTED_PWD_TOKEN: std::cell::RefCell<Option<(usize, u32, std::time::Instant, bool)>> =
        const { std::cell::RefCell::new(None) };
}

fn store_tested_pwd_token_at(
    ctx_ptr: usize,
    vk: u32,
    ts: std::time::Instant,
    is_pwd: bool,
) {
    TESTED_PWD_TOKEN.with(|cell| {
        *cell.borrow_mut() = Some((ctx_ptr, vk, ts, is_pwd));
    });
}

fn clear_tested_pwd_token() {
    TESTED_PWD_TOKEN.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

fn take_valid_tested_pwd_token(ctx_ptr: usize, vk: u32) -> Option<bool> {
    TESTED_PWD_TOKEN.with(|cell| {
        let token = cell.borrow_mut().take()?;
        if token.0 == ctx_ptr && token.1 == vk && token.2.elapsed().as_millis() < 50 {
            Some(token.3)
        } else {
            None
        }
    })
}

#[derive(Clone)]
struct WesternSpaceRollback {
    pre_space_snapshot: EditorSnapshot,
    corrected_text_len: usize,
    corrected_cursor: usize,
    pin_range: (usize, usize),
}

static INSTANCE_LAST_WESTERN_AUTOCORRECT: std::sync::OnceLock<
    Mutex<std::collections::HashMap<usize, WesternSpaceRollback>>,
> = std::sync::OnceLock::new();

static WESTERN_LEADING_SIGILS: std::sync::OnceLock<
    Mutex<std::collections::HashMap<usize, char>>,
> = std::sync::OnceLock::new();

static WESTERN_AUTOCORRECT_UNDO_PINS: std::sync::OnceLock<
    Mutex<std::collections::HashMap<usize, Vec<(usize, usize, usize)>>>,
> = std::sync::OnceLock::new();

fn record_western_leading_sigil(inst_key: usize, sigil: char) {
    let map_mutex = WESTERN_LEADING_SIGILS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(mut map) = map_mutex.lock() {
        map.insert(inst_key, sigil);
    }
}

fn take_western_leading_sigil(inst_key: usize) -> Option<char> {
    let map_mutex = WESTERN_LEADING_SIGILS.get()?;
    map_mutex.lock().ok()?.remove(&inst_key)
}

fn clear_western_leading_sigil(inst_key: usize) {
    if let Some(map_mutex) = WESTERN_LEADING_SIGILS.get() {
        if let Ok(mut map) = map_mutex.lock() {
            map.remove(&inst_key);
        }
    }
}

fn record_western_undo_pin(inst_key: usize, depth: usize, pin_start: usize, pin_end: usize) {
    let map_mutex = WESTERN_AUTOCORRECT_UNDO_PINS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(mut map) = map_mutex.lock() {
        let list = map.entry(inst_key).or_default();
        list.retain(|(d, _, _)| *d < depth);
        list.push((depth, pin_start, pin_end));
    }
}

fn take_western_undo_pin_for_depth(inst_key: usize, popped_depth: usize) -> Option<(usize, usize)> {
    let map_mutex = WESTERN_AUTOCORRECT_UNDO_PINS.get()?;
    let mut map = map_mutex.lock().ok()?;
    let list = map.get_mut(&inst_key)?;
    if let Some(pos) = list.iter().rposition(|(d, _, _)| *d == popped_depth) {
        let (_, s, e) = list.remove(pos);
        Some((s, e))
    } else {
        None
    }
}

fn shift_western_undo_pins_after_cap_pop(inst_key: usize) {
    if let Some(map_mutex) = WESTERN_AUTOCORRECT_UNDO_PINS.get() {
        if let Ok(mut map) = map_mutex.lock() {
            if let Some(list) = map.get_mut(&inst_key) {
                list.retain_mut(|(d, _, _)| {
                    if *d <= 1 {
                        false
                    } else {
                        *d -= 1;
                        true
                    }
                });
            }
        }
    }
}

fn prune_western_undo_pins_above(inst_key: usize, new_depth: usize) {
    if let Some(map_mutex) = WESTERN_AUTOCORRECT_UNDO_PINS.get() {
        if let Ok(mut map) = map_mutex.lock() {
            if let Some(list) = map.get_mut(&inst_key) {
                list.retain(|(d, _, _)| *d < new_depth);
            }
        }
    }
}

fn clear_western_undo_pins(inst_key: usize) {
    if let Some(map_mutex) = WESTERN_AUTOCORRECT_UNDO_PINS.get() {
        if let Ok(mut map) = map_mutex.lock() {
            map.remove(&inst_key);
        }
    }
}

fn cleanup_instance_side_tables(inst_key: usize, comp_key: usize) {
    {
        let mut guard = CURRENT_DLL_INSTANCES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(set) = guard.as_mut() {
            set.remove(&inst_key);
        }
    }
    {
        let mut guard = INSTANCE_LEX_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(map) = guard.as_mut() {
            map.remove(&inst_key);
        }
    }
    {
        let mut guard = INSTANCE_FC_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(map) = guard.as_mut() {
            map.remove(&inst_key);
        }
    }
    {
        let mut guard = INSTANCE_TPL_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(map) = guard.as_mut() {
            map.remove(&inst_key);
        }
    }
    if let Some(map_mutex) = INSTANCE_LAST_WESTERN_AUTOCORRECT.get() {
        let mut guard = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(&inst_key);
    }
    clear_western_leading_sigil(inst_key);
    clear_western_undo_pins(inst_key);
    set_tab_range_popup_active(inst_key, false);
    remove_composition_epoch(comp_key);
}

fn apply_config_to_engine(eng: &mut RuhiEngine, cfg: &RuhiConfig) {
    eng.keyboard_mode = cfg.keyboard_mode.clone();
    eng.last_keyboard_mode = cfg.last_keyboard_mode.clone();
    eng.set_enabled_keyboard_modes(cfg.enabled_keyboard_modes.clone());
    eng.set_western_languages(&cfg.western_languages);
    eng.backtick_inline_mode = cfg.backtick_inline_mode.clone();
    eng.tilde_toggle_mode = cfg.tilde_toggle_mode.clone();
    eng.shift_down_mode = cfg.shift_down_mode.clone();
    eng.enable_mixed_english = cfg.enable_mixed_english;
    eng.mixed_english_mode = cfg.mixed_english_mode.clone();
    eng.mixed_english_min_len = cfg.mixed_english_min_len;
    eng.set_mixed_typing_modes(cfg.mixed_typing_modes.clone());
    eng.enable_array30_coach = cfg.enable_array30_coach;
    eng.reverse_coach_target = cfg.reverse_coach_target.clone();
    eng.four_corner.keymap_profile = cfg.four_corner_keymap.clone();
    eng.fullwidth_mode = cfg.fullwidth_mode;
    eng.shift_space_toggle_fullwidth = cfg.shift_space_toggle_fullwidth;
    eng.halfwidth_punct_mode = cfg.halfwidth_punct_mode.clone();
    eng.tab_key_mode = cfg.tab_key_mode.clone();
    eng.candidate_quick_key = cfg.candidate_quick_key.clone();
    eng.set_classical_mode(ruhi::router::ClassicalRegisterMode::from_config_str(
        &cfg.classical_mode,
    ));
}

#[inline]
const fn fnv1a_mix_u64(mut hash: u64, val: u64) -> u64 {
    let bytes = val.to_le_bytes();
    let mut i = 0;
    while i < 8 {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
        i += 1;
    }
    hash
}

pub(crate) fn compute_layer_a_abi_hash() -> u64 {
    use std::mem::{align_of, needs_drop, offset_of, size_of};
    const LAYER_B_ABI_EPOCH: usize = 2;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let dims: [usize; 53] = [
        LAYER_B_ABI_EPOCH,
        size_of::<usize>(),
        size_of::<RuhiTextService>(),
        align_of::<RuhiTextService>(),
        needs_drop::<RuhiTextService>() as usize,
        offset_of!(RuhiTextService, thread_mgr),
        offset_of!(RuhiTextService, client_id),
        offset_of!(RuhiTextService, engine),
        offset_of!(RuhiTextService, composition),
        offset_of!(RuhiTextService, thread_mgr_sink_cookie),
        offset_of!(RuhiTextService, langbar_item),
        offset_of!(RuhiTextService, candidate_window),
        offset_of!(RuhiTextService, pending_compose),
        offset_of!(RuhiTextService, sentence_cursor),
        offset_of!(RuhiTextService, candidate_selecting),
        offset_of!(RuhiTextService, active_candidates),
        offset_of!(RuhiTextService, selected_candidate_idx),
        offset_of!(RuhiTextService, target_char_idx),
        offset_of!(RuhiTextService, target_range),
        offset_of!(RuhiTextService, config),
        offset_of!(RuhiTextService, sel_anchor),
        offset_of!(RuhiTextService, insert_mode),
        offset_of!(RuhiTextService, q_press_count),
        offset_of!(RuhiTextService, q_last_char_idx),
        offset_of!(RuhiTextService, pending_syllables),
        offset_of!(RuhiTextService, pending_segments),
        offset_of!(RuhiTextService, last_raw_ascii),
        offset_of!(RuhiTextService, manual_cuts),
        offset_of!(RuhiTextService, pending_pinned),
        offset_of!(RuhiTextService, deleted_words),
        offset_of!(RuhiTextService, undo_stack),
        offset_of!(RuhiTextService, redo_stack),
        size_of::<RuhiConfig>(),
        align_of::<RuhiConfig>(),
        size_of::<EditorSnapshot>(),
        align_of::<EditorSnapshot>(),
        size_of::<crate::candidate_window::CandidateWindow>(),
        align_of::<crate::candidate_window::CandidateWindow>(),
        size_of::<crate::candidate_window::CandidateState>(),
        align_of::<crate::candidate_window::CandidateState>(),
        size_of::<RuhiEngine>(),
        align_of::<RuhiEngine>(),
        size_of::<ruhi::engine::ShiftDownRuntimeState>(),
        align_of::<ruhi::engine::ShiftDownRuntimeState>(),
        size_of::<LexiconManager>(),
        align_of::<LexiconManager>(),
        size_of::<ruhi::western::WesternEngine>(),
        align_of::<ruhi::western::WesternEngine>(),
        needs_drop::<ruhi::western::WesternEngine>() as usize,
        size_of::<ruhi::symbols::SymbolsManager>(),
        align_of::<ruhi::symbols::SymbolsManager>(),
        size_of::<ruhi::templates::TemplatesManager>(),
        align_of::<ruhi::templates::TemplatesManager>(),
    ];
    for d in dims {
        h = fnv1a_mix_u64(h, d as u64);
    }
    h
}

pub fn invalidate_config_cache() {
    let mut lock = LAST_CONFIG_CHECK.lock().unwrap_or_else(|e| e.into_inner());
    *lock = None;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RuhiConfig {
    #[serde(default = "default_keyboard_mode")]
    pub keyboard_mode: String,
    #[serde(default = "default_last_keyboard_mode")]
    pub last_keyboard_mode: String,
    #[serde(default = "default_enabled_keyboard_modes")]
    pub enabled_keyboard_modes: Vec<String>,
    #[serde(default = "default_backtick_inline_mode")]
    pub backtick_inline_mode: String,
    #[serde(default = "default_tilde_toggle_mode")]
    pub tilde_toggle_mode: String,
    #[serde(default = "default_aux_mode")]
    pub shift_down_mode: String,
    #[serde(default = "default_candidate_quick_key")]
    pub candidate_quick_key: String,
    #[serde(default = "default_punctuation_commit_mode")]
    pub punctuation_commit_mode: String,
    #[serde(default = "default_input_display_mode")]
    pub input_display_mode: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_candidate_font_size")]
    pub candidate_font_size: u32,
    #[serde(default = "default_desk_opacity")]
    pub desk_opacity: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_ui_lang")]
    pub ui_lang: String,
    #[serde(default = "default_orientation")]
    pub orientation: String,
    #[serde(default = "default_user_dict_path")]
    pub user_dict_path: String,
    #[serde(default = "default_desk_x")]
    pub desk_x: i32,
    #[serde(default = "default_desk_y")]
    pub desk_y: i32,
    #[serde(default = "default_max_composition_chars")]
    pub max_composition_chars: usize,
    #[serde(default = "default_q_cycle_max")]
    pub q_cycle_max: usize,
    #[serde(default = "default_enable_mixed_english")]
    pub enable_mixed_english: bool,
    #[serde(default = "default_mixed_english_mode")]
    pub mixed_english_mode: String,
    #[serde(default = "default_mixed_english_min_len")]
    pub mixed_english_min_len: usize,
    #[serde(default = "default_mixed_typing_modes")]
    pub mixed_typing_modes: Vec<String>,
    #[serde(default = "default_capslock_switch")]
    pub capslock_switch: String,
    #[serde(default = "default_capsule_mode")]
    pub capsule_mode: String,
    #[serde(default = "default_confirm_add_word")]
    pub confirm_add_word: bool,
    #[serde(default = "default_confirm_suppress_word")]
    pub confirm_suppress_word: bool,
    #[serde(default)]
    pub enable_array30_coach: bool,
    #[serde(default = "default_reverse_coach_target")]
    pub reverse_coach_target: String,
    #[serde(default = "default_four_corner_keymap")]
    pub four_corner_keymap: String,
    #[serde(default = "default_telemetry_consent")]
    pub telemetry_consent: bool,
    #[serde(default = "default_upload_schedule")]
    pub upload_schedule: String,
    #[serde(default)]
    pub last_upload_timestamp: u64,
    #[serde(default = "default_tailcat_auto_sync")]
    pub tailcat_auto_sync: bool,
    #[serde(default = "default_tailcat_update_mode")]
    pub tailcat_update_mode: String,
    #[serde(default)]
    pub last_tailcat_sync_timestamp: u64,
    #[serde(default)]
    pub fullwidth_mode: bool,
    #[serde(default = "default_shift_space_toggle_fullwidth")]
    pub shift_space_toggle_fullwidth: bool,
    #[serde(default = "default_halfwidth_punct_mode")]
    pub halfwidth_punct_mode: String,
    #[serde(default = "default_tab_key_mode")]
    pub tab_key_mode: String,
    #[serde(default = "default_western_languages")]
    pub western_languages: Vec<String>,
    #[serde(default = "default_classical_mode")]
    pub classical_mode: String,
}

impl Default for RuhiConfig {
    fn default() -> Self {
        Self {
            keyboard_mode: default_keyboard_mode(),
            last_keyboard_mode: default_last_keyboard_mode(),
            enabled_keyboard_modes: default_enabled_keyboard_modes(),
            backtick_inline_mode: default_backtick_inline_mode(),
            tilde_toggle_mode: default_tilde_toggle_mode(),
            shift_down_mode: default_aux_mode(),
            candidate_quick_key: default_candidate_quick_key(),
            punctuation_commit_mode: default_punctuation_commit_mode(),
            input_display_mode: default_input_display_mode(),
            font_size: default_font_size(),
            candidate_font_size: default_candidate_font_size(),
            desk_opacity: default_desk_opacity(),
            theme: default_theme(),
            ui_lang: default_ui_lang(),
            orientation: default_orientation(),
            user_dict_path: default_user_dict_path(),
            desk_x: default_desk_x(),
            desk_y: default_desk_y(),
            max_composition_chars: default_max_composition_chars(),
            q_cycle_max: default_q_cycle_max(),
            enable_mixed_english: default_enable_mixed_english(),
            mixed_english_mode: default_mixed_english_mode(),
            mixed_english_min_len: default_mixed_english_min_len(),
            mixed_typing_modes: default_mixed_typing_modes(),
            capslock_switch: default_capslock_switch(),
            capsule_mode: default_capsule_mode(),
            confirm_add_word: default_confirm_add_word(),
            confirm_suppress_word: default_confirm_suppress_word(),
            enable_array30_coach: false,
            reverse_coach_target: default_reverse_coach_target(),
            four_corner_keymap: default_four_corner_keymap(),
            telemetry_consent: default_telemetry_consent(),
            upload_schedule: default_upload_schedule(),
            last_upload_timestamp: 0,
            tailcat_auto_sync: default_tailcat_auto_sync(),
            tailcat_update_mode: default_tailcat_update_mode(),
            last_tailcat_sync_timestamp: 0,
            fullwidth_mode: false,
            shift_space_toggle_fullwidth: default_shift_space_toggle_fullwidth(),
            halfwidth_punct_mode: default_halfwidth_punct_mode(),
            tab_key_mode: default_tab_key_mode(),
            western_languages: default_western_languages(),
            classical_mode: default_classical_mode(),
        }
    }
}

fn default_western_languages() -> Vec<String> { vec!["en".to_string()] }
fn default_tab_key_mode() -> String { "associative".to_string() }
fn default_classical_mode() -> String { "auto".to_string() }
fn default_shift_space_toggle_fullwidth() -> bool { true }
fn default_halfwidth_punct_mode() -> String { "fullwidth".to_string() }
fn default_telemetry_consent() -> bool { false }
fn default_reverse_coach_target() -> String { "none".to_string() }
fn default_tailcat_auto_sync() -> bool { true }
fn default_upload_schedule() -> String { "weekly".to_string() }
fn default_tailcat_update_mode() -> String { "notify_confirm".to_string() }

fn default_keyboard_mode() -> String { "hsu".to_string() }
fn default_last_keyboard_mode() -> String { "english".to_string() }
fn default_enabled_keyboard_modes() -> Vec<String> { vec!["hsu".to_string(), "longkan".to_string(), "japanese".to_string()] }
fn default_backtick_inline_mode() -> String { "user_abbrev".to_string() }
fn default_aux_mode() -> String { "longkan".to_string() }
fn default_tilde_toggle_mode() -> String { "punct".to_string() }
fn default_candidate_quick_key() -> String { "auto".to_string() }
fn default_punctuation_commit_mode() -> String { "A".to_string() }
fn default_input_display_mode() -> String { "smart_auto".to_string() }
fn default_font_size() -> u32 { 32 }
fn default_candidate_font_size() -> u32 { 24 }
fn default_desk_opacity() -> u32 { 95 }
fn default_theme() -> String { "system".to_string() }
fn default_ui_lang() -> String { "auto".to_string() }

static ACTIVE_TSF_LANGID: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

pub fn get_active_tsf_langid() -> u16 {
    ACTIVE_TSF_LANGID.load(std::sync::atomic::Ordering::SeqCst)
}

/// 透過 TSF `ITfInputProcessorProfileMgr::GetActiveProfile` 查詢目前作用中之鍵盤設定檔 `LANGID`
pub fn query_active_tsf_langid() -> u16 {
    unsafe {
        let mgr_res: Result<ITfInputProcessorProfileMgr> = windows::Win32::System::Com::CoCreateInstance(
            &CLSID_TF_InputProcessorProfiles,
            None,
            windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
        );
        if let Ok(mgr) = mgr_res {
            let mut prof = TF_INPUTPROCESSORPROFILE::default();
            if mgr.GetActiveProfile(&crate::guid::GUID_TFCAT_TIP_KEYBOARD, &mut prof).is_ok() {
                if prof.clsid == crate::guid::CLSID_RUHI_TEXT_SERVICE && prof.langid != 0 {
                    return prof.langid;
                }
            }
        }
        0
    }
}

/// 將 Windows `LANGID` 轉換為標準介面語系代碼 (`"zh-TW"`, `"en-US"`, `"ja-JP"`, `"ko-KR"`)
pub fn langid_to_ui_locale(langid: u16) -> &'static str {
    match langid & 0x03FF {
        0x04 => "zh-TW",
        0x11 => "ja-JP",
        0x12 => "ko-KR",
        0x09 | 0x0C | 0x07 | 0x0A => "en-US",
        _ => "en-US",
    }
}

/// 解析目前實際生效之介面語系 (`"zh-TW"`, `"en-US"`, `"ja-JP"`, `"ko-KR"`)
pub fn resolve_effective_ui_lang(configured_ui_lang: &str) -> &'static str {
    let norm = configured_ui_lang.trim().to_ascii_lowercase();
    match norm.as_str() {
        "zh" | "zh-tw" | "zh_tw" | "zh-hant" => return "zh-TW",
        "en" | "en-us" | "en_us" | "en-gb" => return "en-US",
        "ja" | "ja-jp" | "ja_jp" | "jp" => return "ja-JP",
        "ko" | "ko-kr" | "ko_kr" | "kr" => return "ko-KR",
        _ => {}
    }
    let mut langid = get_active_tsf_langid();
    if langid == 0 {
        langid = query_active_tsf_langid();
        if langid != 0 {
            ACTIVE_TSF_LANGID.store(langid, std::sync::atomic::Ordering::SeqCst);
        }
    }
    if langid != 0 {
        return langid_to_ui_locale(langid);
    }
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    let sys_lang = unsafe { GetUserDefaultUILanguage() };
    if sys_lang != 0 {
        langid_to_ui_locale(sys_lang)
    } else {
        "zh-TW"
    }
}

/// 當使用者從 Windows 語言選單切換不同語系入口之 RuhI 設定檔時，同步更新作用中語系與預設輸入模式
pub fn sync_active_tsf_profile_locale(cfg: &mut RuhiConfig) {
    let active_langid = query_active_tsf_langid();
    if active_langid == 0 {
        return;
    }
    let prev_langid = ACTIVE_TSF_LANGID.swap(active_langid, std::sync::atomic::Ordering::SeqCst);
    let primary = active_langid & 0x03FF;
    let prev_primary = prev_langid & 0x03FF;
    // 僅當使用者明確在兩個不同語言入口間切換（prev_primary != 0 且 prev_langid != active_langid）時，才自動切換對應語系之鍵盤模式，
    // 避免新程序或新 DLL 剛載入 (prev_langid == 0) 時覆寫使用者剛切換好的輸入模式或觸發重入鎖定。
    if prev_primary != 0 && prev_langid != active_langid && prev_primary != primary {
        let target_mode = match primary {
            0x09 | 0x0C | 0x07 | 0x0A => Some("english"),
            0x11 => Some("japanese"),
            0x12 => Some("korean"),
            0x04 => Some(
                cfg.enabled_keyboard_modes
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("hsu"),
            ),
            _ => None,
        };
        if let Some(m) = target_mode {
            let m_owned = m.to_string();
            if !cfg.keyboard_mode.eq_ignore_ascii_case(&m_owned) {
                let prev_kb = cfg.keyboard_mode.clone();
                cfg.last_keyboard_mode = prev_kb.clone();
                cfg.keyboard_mode = m_owned.clone();
                crate::candidate_window::save_keyboard_mode_with_last(&m_owned, Some(&prev_kb));
            }
        }
    }
}
fn default_orientation() -> String { "horizontal".to_string() }
fn default_user_dict_path() -> String { "user_dict.tsv".to_string() }
fn default_desk_x() -> i32 { 350 }
fn default_desk_y() -> i32 { 550 }
fn default_max_composition_chars() -> usize { 30 }
fn default_q_cycle_max() -> usize { 3 }
fn default_enable_mixed_english() -> bool { true }
fn default_mixed_english_mode() -> String { "contextual_stream".to_string() }
fn default_mixed_english_min_len() -> usize { 4 }
fn default_mixed_typing_modes() -> Vec<String> {
    vec![
        "standard".to_string(),
        "cangjie".to_string(),
        "four_corner".to_string(),
        "array30".to_string(),
    ]
}
fn default_capslock_switch() -> String { "capital_lock".to_string() }
fn default_capsule_mode() -> String { "mini_capsule".to_string() }
fn default_confirm_add_word() -> bool { true }
fn default_confirm_suppress_word() -> bool { true }
fn default_four_corner_keymap() -> String { "blind_closed_loop".to_string() }

#[derive(Debug, Clone)]
pub struct RuhiPaths {
    pub data_dir: std::path::PathBuf,
    pub sandbox_dir: std::path::PathBuf,
    pub core_lexicon: std::path::PathBuf,
    pub mcbopomofo: std::path::PathBuf,
    pub moe: std::path::PathBuf,
    pub domain: std::path::PathBuf,
    pub user_dict: std::path::PathBuf,
    pub transitions: std::path::PathBuf,
    pub symbols: std::path::PathBuf,
    pub config: std::path::PathBuf,
    pub english: std::path::PathBuf,
    pub array30: std::path::PathBuf,
    pub four_corner: std::path::PathBuf,
    pub longkan: std::path::PathBuf,
    pub deleted_dict: std::path::PathBuf,
    pub rules_json: std::path::PathBuf,
    pub templates: std::path::PathBuf,
}

impl RuhiPaths {
    pub fn resolve() -> Self {
        // Dynamic path resolution: Priority = Local Portable Dir > In-Tree Dev Root > %APPDATA%\Ruhi
        let appdata_ruhi = std::env::var("APPDATA")
            .map(|p| std::path::PathBuf::from(p).join("Ruhi"))
            .unwrap_or_else(|_| std::path::PathBuf::from("."));

        let dll_dir = {
            let hinstance_val = crate::DLL_HINSTANCE.load(std::sync::atomic::Ordering::SeqCst);
            if hinstance_val != 0 {
                let hmodule = HMODULE(hinstance_val as *mut std::ffi::c_void);
                let mut buf = vec![0u16; 1024];
                let len = unsafe { windows::Win32::System::LibraryLoader::GetModuleFileNameW(hmodule, &mut buf) };
                if len > 0 {
                    let dll_path_str = String::from_utf16_lossy(&buf[..len as usize]);
                    std::path::Path::new(&dll_path_str).parent().map(|p| p.to_path_buf())
                } else {
                    None
                }
            } else {
                None
            }
        }.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));

        // Resolve data directory
        let has_full_lexicon = |dir: &std::path::Path| {
            dir.join("lexicon_core.tsv").exists()
                || dir.join("lexicon_mcbopomofo.tsv").exists()
                || dir.join("lexicon_moe.tsv").exists()
        };

        let dll_parent = dll_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| dll_dir.join(".."));
        let dll_grandparent = dll_parent
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| dll_parent.join(".."));

        let data_dir = if dll_parent.join("ruhi_lexicon").join("data").is_dir() && has_full_lexicon(&dll_parent.join("ruhi_lexicon").join("data")) {
            dll_parent.join("ruhi_lexicon").join("data")
        } else if dll_dir.join("ruhi_lexicon").join("data").is_dir() && has_full_lexicon(&dll_dir.join("ruhi_lexicon").join("data")) {
            dll_dir.join("ruhi_lexicon").join("data")
        } else if dll_grandparent.join("ruhi_lexicon").join("data").is_dir() && has_full_lexicon(&dll_grandparent.join("ruhi_lexicon").join("data")) {
            dll_grandparent.join("ruhi_lexicon").join("data")
        } else if dll_dir.join("data").is_dir() && has_full_lexicon(&dll_dir.join("data")) {
            dll_dir.join("data")
        } else if appdata_ruhi.join("data").is_dir() && has_full_lexicon(&appdata_ruhi.join("data")) {
            appdata_ruhi.join("data")
        } else if dll_dir.join("data").is_dir() {
            dll_dir.join("data")
        } else {
            dll_dir.clone()
        };

        // Resolve config/user directory (優先以專案根目錄為單一權威組態源，確保 scripts/ 與根目錄永不分裂)
        let sandbox_dir = if dll_parent.join("config.json").exists() && dll_parent.join("ruhi_lexicon").is_dir() {
            dll_parent.clone()
        } else if dll_dir.join("config.json").exists() {
            dll_dir.clone()
        } else if dll_parent.join("config.json").exists() {
            dll_parent.clone()
        } else if dll_grandparent.join("config.json").exists() {
            dll_grandparent.clone()
        } else {
            let _ = std::fs::create_dir_all(&appdata_ruhi);
            appdata_ruhi.clone()
        };

        let find_file = |name: &str| {
            if data_dir.join(name).exists() {
                data_dir.join(name)
            } else if dll_dir.join("data").join(name).exists() {
                dll_dir.join("data").join(name)
            } else if dll_dir.join("ruhi_lexicon").join("data").join(name).exists() {
                dll_dir.join("ruhi_lexicon").join("data").join(name)
            } else if dll_dir.join("..").join("ruhi_lexicon").join("data").join(name).exists() {
                dll_dir.join("..").join("ruhi_lexicon").join("data").join(name)
            } else if dll_dir.join("..").join("..").join("ruhi_lexicon").join("data").join(name).exists() {
                dll_dir.join("..").join("..").join("ruhi_lexicon").join("data").join(name)
            } else if appdata_ruhi.join("data").join(name).exists() {
                appdata_ruhi.join("data").join(name)
            } else {
                data_dir.join(name)
            }
        };

        let core_lexicon = find_file("lexicon_core.tsv");
        let mcbopomofo = find_file("lexicon_mcbopomofo.tsv");
        let moe = find_file("lexicon_moe.tsv");
        let domain = find_file("lexicon_domain.tsv");
        let transitions = if sandbox_dir.join("transitions_user.tsv").exists() {
            sandbox_dir.join("transitions_user.tsv")
        } else {
            find_file("transitions_user.tsv")
        };
        let symbols = find_file("symbols.tsv");
        let config = sandbox_dir.join("config.json");
        let user_dict = sandbox_dir.join("user_dict.tsv");
        let english = find_file("lexicon_english.tsv");
        let array30 = find_file("array30.tsv");
        let four_corner = find_file("four_corner.tsv");
        let longkan = find_file("longkan.tsv");
        let deleted_dict = if sandbox_dir.join("deleted_dict.tsv").exists() {
            sandbox_dir.join("deleted_dict.tsv")
        } else {
            find_file("deleted_dict.tsv")
        };
        let rules_json = if sandbox_dir.join("user_semantic_rules.json").exists() {
            sandbox_dir.join("user_semantic_rules.json")
        } else {
            data_dir.join("user_semantic_rules.json")
        };
        let templates = if sandbox_dir.join("templates.tsv").exists() {
            sandbox_dir.join("templates.tsv")
        } else {
            find_file("templates.tsv")
        };

        Self {
            data_dir,
            sandbox_dir,
            core_lexicon,
            mcbopomofo,
            moe,
            domain,
            user_dict,
            transitions,
            symbols,
            config,
            english,
            array30,
            four_corner,
            longkan,
            deleted_dict,
            rules_json,
            templates,
        }
    }
}

pub fn load_ruhi_config(paths: &RuhiPaths) -> RuhiConfig {
    let now = std::time::Instant::now();
    if let Ok(lock) = LAST_CONFIG_CHECK.lock() {
        if let Some((last_instant, _, ref cached_cfg)) = *lock {
            if now.duration_since(last_instant).as_millis() <= 300 {
                return cached_cfg.clone();
            }
        }
    }
    if let Ok(metadata) = std::fs::metadata(&paths.config) {
        if let Ok(mtime) = metadata.modified() {
            if let Ok(mut lock) = LAST_CONFIG_CHECK.lock() {
                if let Some((ref mut last_instant, prev_mtime, ref cached_cfg)) = *lock {
                    if mtime == prev_mtime {
                        *last_instant = now;
                        return cached_cfg.clone();
                    }
                }
            }
            if let Ok(content) = std::fs::read_to_string(&paths.config) {
                if let Ok(cfg) = serde_json::from_str::<RuhiConfig>(&content) {
                    logger::log(&format!(
                        "Loaded config from {:?}: punct_mode={}, quick_key={}",
                        paths.config, cfg.punctuation_commit_mode, cfg.candidate_quick_key
                    ));
                    if let Ok(mut lock) = LAST_CONFIG_CHECK.lock() {
                        *lock = Some((now, mtime, cfg.clone()));
                    }
                    return cfg;
                }
            }
        }
    }
    logger::log("Using default RuhiConfig (mode B, asdf)");
    RuhiConfig::default()
}

pub fn save_ruhi_config(paths: &RuhiPaths, config: &RuhiConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let mut targets = vec![
            paths.config.clone(),
            paths.sandbox_dir.join("scripts").join("config.json"),
            paths.sandbox_dir.join("dist").join("config.json"),
        ];
        targets.dedup();
        for t in &targets {
            if t == &paths.config || t.exists() {
                let _ = std::fs::write(t, &json);
            }
        }
        invalidate_config_cache();
        logger::log(&format!("Saved updated RuhiConfig to {:?}", paths.config));
    }
}

fn get_lexicon_mtime(paths: &RuhiPaths) -> std::time::SystemTime {
    let files = [&paths.core_lexicon, &paths.mcbopomofo, &paths.moe, &paths.domain, &paths.user_dict];
    let mut max_time = std::time::SystemTime::UNIX_EPOCH;
    for f in &files {
        if let Ok(meta) = std::fs::metadata(f) {
            if let Ok(t) = meta.modified() {
                if t > max_time {
                    max_time = t;
                }
            }
        }
    }
    max_time
}

fn load_all_lexicons(paths: &RuhiPaths) -> (std::time::SystemTime, Arc<LexiconManager>) {
    let mut mgr = LexiconManager::new();
    let mut loaded_count = 0;

    if paths.core_lexicon.exists() {
        if let Ok(cnt) = mgr.load_tsv(&paths.core_lexicon, "core") {
            loaded_count += cnt;
            logger::log(&format!("Loaded core lexicon: {} entries", cnt));
        }
    } else {
        // 1. McBopomofo 主力詞庫 (172k 現代詞頻權重詞彙)
        if paths.mcbopomofo.exists() {
            if let Ok(cnt) = mgr.load_tsv(&paths.mcbopomofo, "mcbopomofo") {
                loaded_count += cnt;
                logger::log(&format!("Loaded mcbopomofo lexicon: {} entries", cnt));
            }
        }
        // 2. MOE 教育部詞庫 (補充)
        if paths.moe.exists() {
            if let Ok(cnt) = mgr.load_tsv(&paths.moe, "moe") {
                loaded_count += cnt;
                logger::log(&format!("Loaded moe lexicon: {} entries", cnt));
            }
        }
        // 3. 領域專業詞庫 (砥睿者、邏霸式等)
        if paths.domain.exists() {
            if let Ok(cnt) = mgr.load_tsv(&paths.domain, "domain") {
                loaded_count += cnt;
                logger::log(&format!("Loaded domain lexicon: {} entries", cnt));
            }
        }
    }
    // 4. 使用者自建詞庫 (謹此、至上意識等，最高優先)
    let user_dict_file = if paths.user_dict.exists() {
        paths.user_dict.clone()
    } else {
        paths.sandbox_dir.join("user_dict.tsv")
    };
    if user_dict_file.exists() {
        if let Ok(cnt) = mgr.load_tsv(&user_dict_file, "user") {
            loaded_count += cnt;
            logger::log(&format!("Loaded user_dict lexicon: {} entries from {:?}", cnt, user_dict_file));
        }
    }
    let mtime = get_lexicon_mtime(paths);
    logger::log(&format!("Total loaded lexicon entries: {}", loaded_count));
    (mtime, Arc::new(mgr))
}

static SHARED_LEXICON: OnceLock<std::sync::RwLock<(std::time::SystemTime, Arc<LexiconManager>)>> = OnceLock::new();

fn get_shared_lexicon_with_mtime(paths: &RuhiPaths) -> (std::time::SystemTime, Arc<LexiconManager>) {
    let holder = SHARED_LEXICON.get_or_init(|| {
        let (mtime, mgr) = load_all_lexicons(paths);
        std::sync::RwLock::new((mtime, mgr))
    });

    let current_mtime = get_lexicon_mtime(paths);
    if let Ok(read_guard) = holder.read() {
        if current_mtime > read_guard.0 {
            drop(read_guard);
            if let Ok(mut write_guard) = holder.write() {
                if current_mtime > write_guard.0 {
                    logger::log(&format!("Detected modified lexicon file(s) (mtime={:?}), reloading...", current_mtime));
                    let (mtime, mgr) = load_all_lexicons(paths);
                    *write_guard = (mtime, mgr);
                }
            }
        }
    }

    holder
        .read()
        .map(|g| (g.0, g.1.clone()))
        .unwrap_or_else(|_| (std::time::SystemTime::UNIX_EPOCH, Arc::new(LexiconManager::new())))
}

fn get_shared_lexicon(paths: &RuhiPaths) -> Arc<LexiconManager> {
    get_shared_lexicon_with_mtime(paths).1
}

pub fn force_reload_lexicon(paths: &RuhiPaths) -> Arc<LexiconManager> {
    let holder = SHARED_LEXICON.get_or_init(|| {
        let (mtime, mgr) = load_all_lexicons(paths);
        std::sync::RwLock::new((mtime, mgr))
    });
    if let Ok(mut write_guard) = holder.write() {
        let (mtime, mgr) = load_all_lexicons(paths);
        *write_guard = (mtime, mgr);
        logger::log(&format!("Force reloaded shared lexicon ({} entries)", write_guard.1.entries.len()));
        return write_guard.1.clone();
    }
    get_shared_lexicon(paths)
}

fn is_punctuation_vk(vk: u32, is_shift: bool) -> bool {
    matches!(vk,
        0xBA | // VK_OEM_1: ; 與 :
        0xDE | // VK_OEM_7: ' 與 "
        0xDB | // VK_OEM_4: [ 與 {
        0xDD | // VK_OEM_6: ] 與 }
        0xBF | // VK_OEM_2: / 與 ?
        0xBC | // VK_OEM_COMMA: , 與 <
        0xBE | // VK_OEM_PERIOD: . (。) 與 Shift+. (……)
        0xBD | // VK_OEM_MINUS: - (─) 與 _ (＿)
        0xDC | // VK_OEM_5: \ 與 |
        0xBB | // VK_OEM_PLUS: = 與 +
        0x6A | // VK_MULTIPLY: 九宮格乘號 *
        0x6B | // VK_ADD: 九宮格加號 +
        0x6D | // VK_SUBTRACT: 九宮格減號 -
        0x6F   // VK_DIVIDE: 九宮格除號 /
    ) || (is_shift && matches!(vk,
        0x31 | // 1 + Shift ➔ ！
        0x32 | // 2 + Shift ➔ ＠
        0x33 | // 3 + Shift ➔ ＃
        0x34 | // 4 + Shift ➔ ＄
        0x35 | // 5 + Shift ➔ ％
        0x36 | // 6 + Shift ➔ ……
        0x37 | // 7 + Shift ➔ ＆
        0x38 | // 8 + Shift ➔ ＊
        0x39 | // 9 + Shift ➔ （
        0x30   // 0 + Shift ➔ ）
    ))
}

pub(crate) fn is_passthrough_non_typing_vk(vk: u32) -> bool {
    matches!(
        vk,
        0x10..=0x13 // VK_SHIFT, VK_CONTROL, VK_MENU, VK_PAUSE
            | 0x2C // VK_SNAPSHOT (PrintScreen)
            | 0x5B..=0x5F // VK_LWIN, VK_RWIN, VK_APPS, reserved, VK_SLEEP
            | 0x70..=0x87 // VK_F1 ..= VK_F24
            | 0x90..=0x91 // VK_NUMLOCK, VK_SCROLL
            | 0xA0..=0xB7 // VK_LSHIFT..VK_RMENU, Browser/Volume/Media/Launch keys
    )
}

fn is_win_modifier_down() -> bool {
    unsafe {
        ((GetKeyState(0x5B) as u16 & 0x8000) != 0)
            || ((GetAsyncKeyState(0x5B) as u16 & 0x8000) != 0)
            || ((GetKeyState(0x5C) as u16 & 0x8000) != 0)
            || ((GetAsyncKeyState(0x5C) as u16 & 0x8000) != 0)
    }
}

pub(crate) fn resolve_ctrl_symbol_combo(
    vk: u32,
    is_shift: bool,
    has_buffer: bool,
    is_english_mode: bool,
    _is_four_corner_mode: bool,
) -> Option<String> {
    // 無組字狀態（全體 10 種鍵盤模式一致放行宿主快捷鍵）或歐文模式下，絕不攔截 Ctrl+符號
    if !has_buffer || is_english_mode {
        return None;
    }
    map_vk_to_ctrl_combo(vk, is_shift)
}

pub(crate) fn is_symbol_tree_hotkey_combo(
    vk: u32,
    is_ctrl: bool,
    is_alt: bool,
    is_shift: bool,
    is_win: bool,
    is_altgr: bool,
    is_english_mode: bool,
) -> bool {
    vk == 0xBC && is_ctrl && is_alt && !is_shift && !is_win && !(is_altgr && is_english_mode)
}

fn is_symbol_tree_accepted_vk(vk: u32) -> bool {
    vk == VK_ESCAPE.0 as u32
        || vk == VK_BACK.0 as u32
        || vk == VK_RETURN.0 as u32
        || vk == VK_TAB.0 as u32
        || vk == VK_SPACE.0 as u32
        || vk == VK_LEFT.0 as u32
        || vk == VK_UP.0 as u32
        || vk == VK_RIGHT.0 as u32
        || vk == VK_DOWN.0 as u32
        || vk == VK_PRIOR.0 as u32
        || vk == VK_NEXT.0 as u32
        || (0x30..=0x39).contains(&vk)
        || (0x41..=0x5A).contains(&vk)
        || (0x60..=0x6F).contains(&vk)
        || (0xBA..=0xDF).contains(&vk)
}

fn symbol_tree_vk_to_key_str(vk: u32, is_shift: bool) -> Option<String> {
    if vk == VK_ESCAPE.0 as u32 {
        return Some("Escape".to_string());
    }
    if vk == VK_BACK.0 as u32 {
        return Some("BackSpace".to_string());
    }
    if vk == VK_RETURN.0 as u32 {
        return Some("Enter".to_string());
    }
    if vk == VK_TAB.0 as u32 {
        return Some(if is_shift { "Shift+Tab" } else { "Tab" }.to_string());
    }
    if vk == VK_SPACE.0 as u32 {
        return Some("Space".to_string());
    }
    if vk == VK_DOWN.0 as u32 {
        return Some("Down".to_string());
    }
    if vk == VK_UP.0 as u32 {
        return Some("Up".to_string());
    }
    if vk == VK_RIGHT.0 as u32 {
        return Some("Right".to_string());
    }
    if vk == VK_LEFT.0 as u32 {
        return Some("Left".to_string());
    }
    if vk == VK_NEXT.0 as u32 {
        return Some("PageDown".to_string());
    }
    if vk == VK_PRIOR.0 as u32 {
        return Some("PageUp".to_string());
    }
    match vk {
        0x60..=0x69 if !is_shift => {
            Some(((b'0' + (vk - 0x60) as u8) as char).to_string())
        }
        0x6D | 0xBD if !is_shift => Some("-".to_string()),
        0x6E | 0xBE if !is_shift => Some(".".to_string()),
        0x6F | 0xBF if !is_shift => Some("/".to_string()),
        0xDC if is_shift => Some("|".to_string()),
        0x30..=0x39 if !is_shift => {
            Some(((b'0' + (vk - 0x30) as u8) as char).to_string())
        }
        0x41..=0x5A => {
            Some(((b'a' + (vk - 0x41) as u8) as char).to_string())
        }
        _ => map_vk_to_char(vk, is_shift).map(|c| c.to_string()),
    }
}

fn map_vk_to_ctrl_combo(vk: u32, is_shift: bool) -> Option<String> {
    let key_part = match vk {
        0xBE => Some("."),
        0xBC => Some(","),
        0xBD => Some("-"),
        0xBA => Some(";"),
        0xDE => Some("'"),
        0xBF => Some("/"),
        0xDB => Some("["),
        0xDD => Some("]"),
        0xDC => Some("\\"),
        0xC0 => Some("`"),
        0x31 => Some("1"),
        _ => None,
    };

    if let Some(k) = key_part {
        if is_shift {
            Some(format!("Ctrl+Shift+{}", k))
        } else {
            Some(format!("Ctrl+{}", k))
        }
    } else {
        None
    }
}

fn map_vk_to_char(vk: u32, is_shift: bool) -> Option<char> {
    // 中文注音與符號對應：
    match vk {
        0xBA => Some(if is_shift { ':' } else { ';' }),
        0xDE => Some(if is_shift { '"' } else { '\'' }),
        0xDB => Some(if is_shift { '{' } else { '[' }),
        0xDD => Some(if is_shift { '}' } else { ']' }),
        0xBF => Some(if is_shift { '?' } else { '/' }),
        0xBC => Some(if is_shift { '<' } else { ',' }),
        0xBE => Some(if is_shift { '>' } else { '.' }),
        0xBD => Some(if is_shift { '_' } else { '-' }),
        0xDC => Some(if is_shift { '|' } else { '\\' }),
        0xBB => Some(if is_shift { '+' } else { '=' }),
        0x36 if is_shift => Some('^'),
        0x31 if is_shift => Some('!'),
        0x32 if is_shift => Some('@'),
        0x33 if is_shift => Some('#'),
        0x34 if is_shift => Some('$'),
        0x35 if is_shift => Some('%'),
        0x37 if is_shift => Some('&'),
        0x38 if is_shift => Some('*'),
        0x39 if is_shift => Some('('),
        0x30 if is_shift => Some(')'),
        0xC0 => Some(if is_shift { '~' } else { '`' }),
        0x41..=0x5A => {
            let base = if is_shift {
                (vk as u8 as char).to_ascii_uppercase()
            } else {
                (vk as u8 as char).to_ascii_lowercase()
            };
            Some(base)
        }
        0x30..=0x39 => Some(vk as u8 as char),
        0x60..=0x69 => Some(('0' as u8 + (vk - 0x60) as u8) as char), // 九宮格數字鍵 Numpad 0~9
        0x6A => Some('*'), // 九宮格乘號 Numpad Multiply *
        0x6B => Some('+'), // 九宮格加號 Numpad Add +
        0x6D => Some('-'), // 九宮格減號 Numpad Subtract -
        0x6E => Some('.'), // 九宮格小數點 Numpad Decimal .
        0x6F => Some('/'), // 九宮格除號 Numpad Divide /
        0x20 => Some(' '),
        0x0D => Some('\n'),
        0x08 | 0x2E => Some('\x08'),
        0x1B => Some('\x1B'),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnglishVkTranslation {
    Text(String),
    DeadKey,
    None,
}

fn is_rmenu_down() -> bool {
    unsafe {
        ((GetKeyState(VK_RMENU.0 as i32) as u16 & 0x8000) != 0)
            || ((GetAsyncKeyState(VK_RMENU.0 as i32) as u16 & 0x8000) != 0)
    }
}

fn current_klid_string() -> Option<String> {
    let mut klid = [0u16; 9];
    if unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutNameW(&mut klid) }.is_ok() {
        let len = klid.iter().position(|&c| c == 0).unwrap_or(klid.len());
        Some(String::from_utf16_lossy(&klid[..len]).to_ascii_uppercase())
    } else {
        None
    }
}

fn translate_vk_in_english_layout_with_hkl(
    vk: u32,
    is_shift: bool,
    is_altgr: bool,
    is_caps_effective: bool,
    is_numlock_on: bool,
    hkl: windows::Win32::UI::Input::KeyboardAndMouse::HKL,
    klid_opt: Option<&str>,
    mutate_dead_key_state: bool,
) -> EnglishVkTranslation {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MAPVK_VK_TO_VSC, MapVirtualKeyExW, ToUnicodeEx, VK_LCONTROL, VK_NUMLOCK,
    };

    let is_plain_us = matches!(klid_opt, Some("00000409" | "00000404"));
    if is_plain_us && !is_altgr && vk != 0xE2 && vk != 0xDF {
        if (0x41..=0x5A).contains(&vk) {
            let ch = vk as u8 as char;
            let mapped = if is_shift ^ is_caps_effective {
                ch.to_ascii_uppercase()
            } else {
                ch.to_ascii_lowercase()
            };
            return EnglishVkTranslation::Text(mapped.to_string());
        }
        return match map_vk_to_char(vk, is_shift) {
            Some(ch) if !ch.is_control() => EnglishVkTranslation::Text(ch.to_string()),
            _ => EnglishVkTranslation::None,
        };
    }

    let mut key_state = [0u8; 256];
    if is_shift {
        key_state[VK_SHIFT.0 as usize] = 0x80;
    }
    if is_caps_effective {
        key_state[VK_CAPITAL.0 as usize] = 0x01;
    }
    if is_numlock_on {
        key_state[VK_NUMLOCK.0 as usize] = 0x01;
    }
    if is_altgr {
        key_state[VK_CONTROL.0 as usize] = 0x80;
        key_state[VK_LCONTROL.0 as usize] = 0x80;
        key_state[VK_MENU.0 as usize] = 0x80;
        key_state[VK_RMENU.0 as usize] = 0x80;
    }

    let scan_code = unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, hkl) };
    let mut out = [0u16; 8];
    // wflags bit 2 (0x04)：OnTestKeyDown 非破壞性探測，保留作業系統核心 Dead Key 狀態至 OnKeyDown 再行消費
    let wflags: u32 = if mutate_dead_key_state { 0 } else { 0x04 };
    let rc = unsafe { ToUnicodeEx(vk, scan_code, &key_state, &mut out, wflags, hkl) };
    if rc < 0 {
        return EnglishVkTranslation::DeadKey;
    }
    if rc > 0 {
        let count = (rc as usize).min(out.len());
        let text: String = String::from_utf16_lossy(&out[..count])
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        if !text.is_empty() {
            return EnglishVkTranslation::Text(text);
        }
        return EnglishVkTranslation::None;
    }

    if !is_altgr && vk != 0xE2 && vk != 0xDF {
        if (0x41..=0x5A).contains(&vk) {
            let ch = vk as u8 as char;
            let mapped = if is_shift ^ is_caps_effective {
                ch.to_ascii_uppercase()
            } else {
                ch.to_ascii_lowercase()
            };
            return EnglishVkTranslation::Text(mapped.to_string());
        }
        if let Some(ch) = map_vk_to_char(vk, is_shift) {
            if !ch.is_control() {
                return EnglishVkTranslation::Text(ch.to_string());
            }
        }
    }
    EnglishVkTranslation::None
}

fn map_asdf_to_idx(vk: u32) -> Option<usize> {
    match vk {
        0x41 => Some(0), // 'A'
        0x53 => Some(1), // 'S'
        0x44 => Some(2), // 'D'
        0x46 => Some(3), // 'F'
        0x47 => Some(4), // 'G'
        0x48 => Some(5), // 'H'
        0x4A => Some(6), // 'J'
        0x4B => Some(7), // 'K'
        0x4C => Some(8), // 'L'
        _ => None,
    }
}

static SUPPRESS_SYNTHETIC_CAPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LAST_SYNTHETIC_CAPS_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn is_synthetic_caps_active() -> bool {
    let last = LAST_SYNTHETIC_CAPS_MS.load(std::sync::atomic::Ordering::SeqCst);
    if last > 0 && now_epoch_ms().saturating_sub(last) < 85 {
        true
    } else {
        SUPPRESS_SYNTHETIC_CAPS.store(false, std::sync::atomic::Ordering::SeqCst);
        false
    }
}

pub fn trigger_synthetic_caps_toggle() {
    unsafe {
        SUPPRESS_SYNTHETIC_CAPS.store(true, std::sync::atomic::Ordering::SeqCst);
        LAST_SYNTHETIC_CAPS_MS.store(now_epoch_ms(), std::sync::atomic::Ordering::SeqCst);
        keybd_event(VK_CAPITAL.0 as u8, 0x3A, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(VK_CAPITAL.0 as u8, 0x3A, KEYEVENTF_KEYUP, 0);
    }
}

thread_local! {
    static LANGBAR_SINKS: std::cell::RefCell<Vec<(u32, ITfLangBarItemSink)>> = const { std::cell::RefCell::new(Vec::new()) };
}
static NEXT_LANGBAR_COOKIE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub fn notify_langbar_mode_update() {
    let sinks_snapshot: Vec<ITfLangBarItemSink> = LANGBAR_SINKS.with(|sinks_cell| {
        if let Ok(sinks) = sinks_cell.try_borrow() {
            sinks.iter().map(|(_, sink)| sink.clone()).collect()
        } else {
            Vec::new()
        }
    });
    for sink in sinks_snapshot {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            let _ = sink.OnUpdate(TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP | TF_LBI_STATUS);
        }));
    }
}

#[implement(ITfLangBarItemButton, ITfSource)]
pub struct RuhiLangBarItemButton;

impl ITfLangBarItem_Impl for RuhiLangBarItemButton_Impl {
    fn GetInfo(&self, pinfo: *mut TF_LANGBARITEMINFO) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if pinfo.is_null() {
                return Err(E_INVALIDARG.into());
            }
            unsafe {
                (*pinfo).clsidService = crate::guid::CLSID_RUHI_TEXT_SERVICE;
                (*pinfo).guidItem = GUID_LBI_INPUTMODE;
                (*pinfo).dwStyle = TF_LBI_STYLE_BTN_BUTTON | TF_LBI_STYLE_SHOWNINTRAY;
                (*pinfo).ulSort = 0;
                let desc_str = match resolve_effective_ui_lang("auto") {
                    "en-US" => "RuhI Input Mode",
                    "ja-JP" => "RuhI 入力モード",
                    "ko-KR" => "RuhI 입력 모드",
                    _ => "RuhI 輸入模式",
                };
                let desc: Vec<u16> = desc_str.encode_utf16().collect();
                for (i, &c) in desc.iter().enumerate().take((*pinfo).szDescription.len() - 1) {
                    (*pinfo).szDescription[i] = c;
                }
            }
            Ok(())
        }))
        .unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn GetStatus(&self) -> Result<u32> {
        Ok(0)
    }

    fn Show(&self, _fshow: BOOL) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            notify_langbar_mode_update();
            Ok(())
        }))
        .unwrap_or_else(|_| Ok(()))
    }

    fn GetTooltipString(&self) -> Result<BSTR> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let paths = RuhiPaths::resolve();
            let cfg = load_ruhi_config(&paths);
            let ui_lang = resolve_effective_ui_lang(&cfg.ui_lang);
            let (badge, mode_name) = crate::candidate_window::get_mode_badge_and_name_i18n(&cfg.keyboard_mode, false, ui_lang);
            let tip = match ui_lang {
                "en-US" => format!("RuhI — {} ({}) [Left-click: Switch / Right-click: Menu]", mode_name, badge),
                "ja-JP" => format!("RuhI 如喜 — {} ({}) [左クリック: 切替 / 右クリック: メニュー]", mode_name, badge),
                "ko-KR" => format!("RuhI 루히 — {} ({}) [좌클릭: 전환 / 우클릭: 메뉴]", mode_name, badge),
                _ => format!("RuhI 如喜 — {} ({}) [左鍵輪流切換／右鍵開啟選單]", mode_name, badge),
            };
            Ok(BSTR::from(tip))
        }))
        .unwrap_or_else(|_| Ok(BSTR::from("RuhI")))
    }
}

impl ITfLangBarItemButton_Impl for RuhiLangBarItemButton_Impl {
    fn OnClick(&self, click: TfLBIClick, pt: &POINT, _prcarea: *const RECT) -> Result<()> {
        let click_pt = *pt;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            logger::log(&format!(
                "RuhiLangBarItemButton::OnClick click={:?}, pt=({}, {})",
                click.0, click_pt.x, click_pt.y
            ));
            unsafe {
                if click == TF_LBI_CLK_LEFT {
                    crate::candidate_window::cycle_next_keyboard_mode_global(Some(click_pt));
                } else {
                    crate::candidate_window::show_taskbar_ri_menu(Some(click_pt));
                }
            }
            Ok(())
        }))
        .unwrap_or_else(|_| Ok(()))
    }

    fn InitMenu(&self, pmenu: Option<&ITfMenu>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(menu) = pmenu {
                let paths = RuhiPaths::resolve();
                let cfg = load_ruhi_config(&paths);
                let cur_kb = cfg.keyboard_mode.to_lowercase();
                let normalized_cur = ruhi::engine::RuhiEngine::normalize_keyboard_mode(&cur_kb);
                let normalized_enabled: Vec<String> = cfg
                    .enabled_keyboard_modes
                    .iter()
                    .map(|m| ruhi::engine::RuhiEngine::normalize_keyboard_mode(m))
                    .filter(|m| !m.is_empty())
                    .collect();
                let is_direct_en_caps = cfg.capslock_switch == "english_direct";
                let is_none_caps = cfg.capslock_switch == "none";
                let ui_lang = resolve_effective_ui_lang(&cfg.ui_lang);
                let items = crate::candidate_window::get_mode_menu_items_i18n(ui_lang);
                let null_bmp = windows::Win32::Graphics::Gdi::HBITMAP::default();
                let cur_mode_id = ruhi::engine::find_mode_descriptor(&cur_kb)
                    .map(|d| d.id)
                    .unwrap_or(normalized_cur.as_str());
                unsafe {
                    for &(id, mode_code, label) in items {
                        if !normalized_enabled.is_empty()
                            && !normalized_enabled.iter().any(|m| m == mode_code)
                            && normalized_cur != mode_code
                        {
                            continue;
                        }
                        let is_active = cur_kb == mode_code || cur_mode_id == mode_code;
                        let lbl = if is_active { format!("✓ {}", label) } else { format!("   {}", label) };
                        let wide: Vec<u16> = lbl.encode_utf16().collect();
                        let _ = menu.AddMenuItem(id, 0, null_bmp, null_bmp, &wide, std::ptr::null_mut());
                    }
                    let is_en_active = cur_mode_id == "english"
                        || matches!(cur_kb.as_str(), "english" | "en" | "latin" | "western" | "euro" | "sanskrit");
                    let show_english_item = normalized_enabled.is_empty()
                        || normalized_enabled.len() >= 2
                        || normalized_enabled.iter().any(|m| m == "english")
                        || is_en_active;
                    let (en_base_lbl, mru_prefix, tune_lbl, set_lbl, about_lbl) = match ui_lang {
                        "en-US" => (
                            "Pan-European / Roman (EU)",
                            "Quick Toggle Last Two Modes",
                            "Show/Hide Inline Tuner Bar (Tune ▾)",
                            "Preferences Center (&S)...",
                            "About RuhI (&A)",
                        ),
                        "ja-JP" => (
                            "汎欧州／ローマ字モード (欧)",
                            "直近 2 モードの高速切替",
                            "インライン調整バーの表示／非表示 (調整 ▾)",
                            "設定センター (&S)...",
                            "RuhI について (&A)",
                        ),
                        "ko-KR" => (
                            "범유럽 / 로마자 모드 (EU)",
                            "최근 두 입력 모드 빠른 전환",
                            "인라인 조정 바 표시/숨기기 (조정 ▾)",
                            "환경설정 (&S)...",
                            "RuhI 정보 (&A)",
                        ),
                        _ => (
                            "泛歐／羅馬字模式 (歐)",
                            "最後兩種輸入模式快速切換",
                            "顯示／隱藏輸入座內嵌調校列 (調校 ▾)",
                            "偏好設定中心 (&S)...",
                            "關於 RuhI 如喜 (&A)",
                        ),
                    };
                    if show_english_item {
                        let en_hotkey = if is_direct_en_caps { "CapsLock" } else { "Ctrl/Shift + CapsLock" };
                        let en_lbl = if is_en_active {
                            format!("✓ {}\t{}", en_base_lbl, en_hotkey)
                        } else {
                            format!("   {}\t{}", en_base_lbl, en_hotkey)
                        };
                        let en_wide: Vec<u16> = en_lbl.encode_utf16().collect();
                        let _ = menu.AddMenuItem(2006, 0, null_bmp, null_bmp, &en_wide, std::ptr::null_mut());
                    }

                    let has_multiple_modes = normalized_enabled.is_empty() || normalized_enabled.len() >= 2;
                    if !is_direct_en_caps && !is_none_caps && has_multiple_modes {
                        if let Some(last_code) = crate::candidate_window::resolve_mru_toggle_target(
                            &cfg.keyboard_mode,
                            &cfg.last_keyboard_mode,
                            &normalized_enabled,
                        ) {
                            let (cur_badge, _) = crate::candidate_window::get_mode_badge_and_name_i18n(&cfg.keyboard_mode, false, ui_lang);
                            let (last_badge, _) = crate::candidate_window::get_mode_badge_and_name_i18n(&last_code, false, ui_lang);
                            let mru_lbl = format!("   {} ({} ⇄ {})\tCapsLock", mru_prefix, cur_badge, last_badge);
                            let mru_wide: Vec<u16> = mru_lbl.encode_utf16().collect();
                            let _ = menu.AddMenuItem(2008, 0, null_bmp, null_bmp, &mru_wide, std::ptr::null_mut());
                        }
                    }

                    let tune_wide: Vec<u16> = format!("   {}", tune_lbl).encode_utf16().collect();
                    let _ = menu.AddMenuItem(3001, 0, null_bmp, null_bmp, &tune_wide, std::ptr::null_mut());

                    let set_wide: Vec<u16> = format!("   {}", set_lbl).encode_utf16().collect();
                    let _ = menu.AddMenuItem(1001, 0, null_bmp, null_bmp, &set_wide, std::ptr::null_mut());
                    let about_wide: Vec<u16> = format!("   {}", about_lbl).encode_utf16().collect();
                    let _ = menu.AddMenuItem(1002, 0, null_bmp, null_bmp, &about_wide, std::ptr::null_mut());
                }
            }
            Ok(())
        }))
        .unwrap_or_else(|_| Ok(()))
    }

    fn OnMenuSelect(&self, wid: u32) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            logger::log(&format!("RuhiLangBarItemButton::OnMenuSelect wid={}", wid));
            let is_caps = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };
            if wid == 2008 {
                crate::candidate_window::toggle_last_two_keyboard_modes_global(None);
                if is_caps {
                    trigger_synthetic_caps_toggle();
                }
                return Ok(());
            }
            let chosen = crate::candidate_window::mode_code_from_menu_cmd_id(wid);
            if let Some(m) = chosen {
                crate::candidate_window::save_keyboard_mode(m);
                if is_caps {
                    trigger_synthetic_caps_toggle();
                }
                notify_langbar_mode_update();
            } else if wid == 3001 {
                crate::candidate_window::toggle_ui_tuner_bar_global();
            } else if wid == 1001 {
                crate::candidate_window::launch_settings();
            } else if wid == 1002 {
                crate::candidate_window::show_about_dialog(HWND::default());
            }
            Ok(())
        }))
        .unwrap_or_else(|_| Ok(()))
    }

    fn GetIcon(&self) -> Result<HICON> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let paths = RuhiPaths::resolve();
            let cfg = load_ruhi_config(&paths);
            let ui_lang = resolve_effective_ui_lang(&cfg.ui_lang);
            let (badge, _) = crate::candidate_window::get_mode_badge_and_name_i18n(&cfg.keyboard_mode, false, ui_lang);
            let hicon = unsafe { crate::candidate_window::create_mode_hicon(badge, false) };
            Ok(hicon)
        }))
        .unwrap_or_else(|_| unsafe {
            Ok(LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
        })
    }

    fn GetText(&self) -> Result<BSTR> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let paths = RuhiPaths::resolve();
            let cfg = load_ruhi_config(&paths);
            let ui_lang = resolve_effective_ui_lang(&cfg.ui_lang);
            let (badge, _) = crate::candidate_window::get_mode_badge_and_name_i18n(&cfg.keyboard_mode, false, ui_lang);
            Ok(BSTR::from(badge))
        }))
        .unwrap_or_else(|_| Ok(BSTR::from("許")))
    }
}

impl ITfSource_Impl for RuhiLangBarItemButton_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Option<&IUnknown>) -> Result<u32> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if riid.is_null() {
                return Err(E_INVALIDARG.into());
            }
            let iid = unsafe { *riid };
            if iid != ITfLangBarItemSink::IID {
                return Err(E_NOINTERFACE.into());
            }
            let unk = punk.ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            let sink: ITfLangBarItemSink = unk.cast()?;
            let cookie = NEXT_LANGBAR_COOKIE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            LANGBAR_SINKS.with(|sinks_cell| {
                if let Ok(mut sinks) = sinks_cell.try_borrow_mut() {
                    sinks.push((cookie, sink.clone()));
                }
            });
            unsafe {
                let _ = sink.OnUpdate(TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP | TF_LBI_STATUS);
            }
            Ok(cookie)
        }))
        .unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn UnadviseSink(&self, dwcookie: u32) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut removed = false;
            LANGBAR_SINKS.with(|sinks_cell| {
                if let Ok(mut sinks) = sinks_cell.try_borrow_mut() {
                    if let Some(pos) = sinks.iter().position(|(c, _)| *c == dwcookie) {
                        sinks.remove(pos);
                        removed = true;
                    }
                }
            });
            if removed {
                Ok(())
            } else {
                Err(OLE_E_NOCONNECTION.into())
            }
        }))
        .unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

#[implement(
    ITfTextInputProcessor,
    ITfTextInputProcessorEx,
    ITfKeyEventSink,
    ITfThreadMgrEventSink,
    ITfCompositionSink,
    ITfDisplayAttributeProvider
)]
#[repr(C)]
pub struct RuhiTextService {
    thread_mgr: Arc<Mutex<Option<ITfThreadMgr>>>,
    client_id: Arc<Mutex<u32>>,
    engine: Arc<Mutex<RuhiEngine>>,
    composition: Arc<Mutex<Option<ITfComposition>>>,
    thread_mgr_sink_cookie: Arc<Mutex<u32>>,
    langbar_item: Arc<Mutex<Option<ITfLangBarItem>>>,
    candidate_window: Arc<Mutex<Option<crate::candidate_window::CandidateWindow>>>,
    pending_compose: Arc<Mutex<String>>,
    sentence_cursor: Arc<Mutex<usize>>,
    candidate_selecting: Arc<Mutex<bool>>,
    active_candidates: Arc<Mutex<Vec<String>>>,
    selected_candidate_idx: Arc<Mutex<usize>>,
    target_char_idx: Arc<Mutex<usize>>,
    target_range: Arc<Mutex<(usize, usize)>>,
    config: Arc<Mutex<RuhiConfig>>,
    // ── 沙盒移植：選取狀態與插入/覆蓋模式 ──
    sel_anchor: Arc<Mutex<Option<usize>>>,  // 選取錨點（None = 無選取）
    insert_mode: Arc<Mutex<bool>>,          // true = 插入, false = 覆蓋
    // ── Q 鍵快速候選輪換狀態 ──
    q_press_count: Arc<Mutex<usize>>,
    q_last_char_idx: Arc<Mutex<Option<usize>>>,
    // ── 詞網格斷詞與注音保留（破音字嚴守輸入讀音與同音詞優先原則） ──
    pending_syllables: Arc<Mutex<Vec<Option<String>>>>,
    pending_segments: Arc<Mutex<Vec<(usize, usize, String, String)>>>,
    // ── 最後一次結算進 pending 之原始英數鍵串 (start, end, raw_ascii)，供跨軌 ↑ 歐文救援 ──
    last_raw_ascii: Arc<Mutex<Option<(usize, usize, String)>>>,
    // ── Tab 鍵手動斷詞點集合 (Manual Cuts) ──
    manual_cuts: Arc<Mutex<std::collections::HashSet<usize>>>,
    pending_pinned: Arc<Mutex<std::collections::HashSet<usize>>>,
    // ── 抑制厭惡詞即時過濾集合 ──
    deleted_words: Arc<Mutex<std::collections::HashSet<String>>>,
    // ── 編輯器復原/重做歷史快照堆疊 (Undo / Redo Stacks) ──
    undo_stack: Arc<Mutex<VecDeque<EditorSnapshot>>>,
    redo_stack: Arc<Mutex<VecDeque<EditorSnapshot>>>,
}

impl Drop for RuhiTextService {
    fn drop(&mut self) {
        let inst_key = Arc::as_ptr(&self.client_id) as usize;
        let comp_key = Arc::as_ptr(&self.composition) as usize;
        cleanup_instance_side_tables(inst_key, comp_key);
        if Arc::strong_count(&self.engine) == 1 {
            if let Ok(eng) = self.engine.try_lock() {
                eng.clear_symbol_tree_mru();
            }
        }
    }
}

impl RuhiTextService {
    pub fn new() -> Self {
        logger::log("RuhiTextService::new() starting...");

        let paths = RuhiPaths::resolve();
        let config = load_ruhi_config(&paths);
        let del_words = crate::user_word_dialog::load_deleted_words(&paths);
        let mut engine = RuhiEngine::new_without_core_lexicon(&paths.data_dir.to_string_lossy());
        apply_config_to_engine(&mut engine, &config);
        let (lex_mtime, shared_lex) = get_shared_lexicon_with_mtime(&paths);
        engine.lexicon = (*shared_lex).clone();

        if paths.transitions.exists() && paths.transitions != paths.data_dir.join("transitions_user.tsv") {
            if let Some(p_str) = paths.transitions.to_str() {
                let _ = engine.transitions.load_tsv(p_str);
            }
        }
        engine.longkan.table.configure_data_dir(&paths.data_dir);
        let initial_fc_mtime = std::fs::metadata(&paths.four_corner)
            .ok()
            .and_then(|m| m.modified().ok());
        let mut initial_tpl_mtime = None;
        if paths.templates.exists() {
            let _ = engine.templates.load_tsv(&paths.templates);
            initial_tpl_mtime = std::fs::metadata(&paths.templates)
                .ok()
                .and_then(|m| m.modified().ok());
        }
        let sandbox_sym_tree = paths.sandbox_dir.join("symbol_tree.tsv");
        if sandbox_sym_tree.exists() {
            ruhi::symbol_tree::configure_tsv_path(&sandbox_sym_tree);
        }

        logger::log(&format!("RuhiTextService::new() initialized with data_dir={:?}, punct_mode={}, quick_key={}",
            paths.data_dir, config.punctuation_commit_mode, config.candidate_quick_key));

        let cw = match crate::candidate_window::CandidateWindow::new() {
            Ok(mut w) => {
                w.apply_config(&config);
                logger::log("CandidateWindow created successfully and config applied.");
                Some(w)
            }
            Err(e) => {
                logger::log(&format!("CandidateWindow creation failed: {:?}", e));
                None
            }
        };

        let client_id = Arc::new(Mutex::new(0));
        let inst_key = Arc::as_ptr(&client_id) as usize;
        if let Ok(mut guard) = CURRENT_DLL_INSTANCES.lock() {
            guard
                .get_or_insert_with(std::collections::HashSet::new)
                .insert(inst_key);
        }
        if let Ok(mut mguard) = INSTANCE_LEX_MTIMES.lock() {
            mguard
                .get_or_insert_with(std::collections::HashMap::new)
                .insert(inst_key, lex_mtime);
        }
        if let Some(fc_mtime) = initial_fc_mtime {
            if let Ok(mut guard) = INSTANCE_FC_MTIMES.lock() {
                guard
                    .get_or_insert_with(std::collections::HashMap::new)
                    .insert(inst_key, fc_mtime);
            }
        }
        if let Some(tpl_mtime) = initial_tpl_mtime {
            if let Ok(mut guard) = INSTANCE_TPL_MTIMES.lock() {
                guard
                    .get_or_insert_with(std::collections::HashMap::new)
                    .insert(inst_key, tpl_mtime);
            }
        }

        Self {
            thread_mgr: Arc::new(Mutex::new(None)),
            client_id,
            engine: Arc::new(Mutex::new(engine)),
            composition: Arc::new(Mutex::new(None)),
            thread_mgr_sink_cookie: Arc::new(Mutex::new(0)),
            langbar_item: Arc::new(Mutex::new(None)),
            candidate_window: Arc::new(Mutex::new(cw)),
            pending_compose: Arc::new(Mutex::new(String::new())),
            sentence_cursor: Arc::new(Mutex::new(0)),
            candidate_selecting: Arc::new(Mutex::new(false)),
            active_candidates: Arc::new(Mutex::new(Vec::new())),
            selected_candidate_idx: Arc::new(Mutex::new(0)),
            target_char_idx: Arc::new(Mutex::new(0)),
            target_range: Arc::new(Mutex::new((0, 0))),
            config: Arc::new(Mutex::new(config)),
            sel_anchor: Arc::new(Mutex::new(None)),
            insert_mode: Arc::new(Mutex::new(true)),
            q_press_count: Arc::new(Mutex::new(0)),
            q_last_char_idx: Arc::new(Mutex::new(None)),
            pending_syllables: Arc::new(Mutex::new(Vec::new())),
            pending_segments: Arc::new(Mutex::new(Vec::new())),
            last_raw_ascii: Arc::new(Mutex::new(None)),
            manual_cuts: Arc::new(Mutex::new(std::collections::HashSet::new())),
            pending_pinned: Arc::new(Mutex::new(std::collections::HashSet::new())),
            deleted_words: Arc::new(Mutex::new(del_words)),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
            redo_stack: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn ensure_instance_upgraded(&self) {
        let inst_key = Arc::as_ptr(&self.client_id) as usize;
        let already_upgraded = {
            let mut guard = CURRENT_DLL_INSTANCES.lock().unwrap_or_else(|e| e.into_inner());
            let set = guard.get_or_insert_with(std::collections::HashSet::new);
            if set.contains(&inst_key) {
                true
            } else {
                set.insert(inst_key);
                false
            }
        };
        if already_upgraded {
            return;
        }

        logger::log(&format!(
            "Hot-upgrading legacy RuhiTextService instance in-place (inst_key={:#x})...",
            inst_key
        ));

        let paths = RuhiPaths::resolve();
        let mut config = load_ruhi_config(&paths);
        sync_active_tsf_profile_locale(&mut config);
        let del_words = crate::user_word_dialog::load_deleted_words(&paths);
        let mut engine = RuhiEngine::new_without_core_lexicon(&paths.data_dir.to_string_lossy());
        apply_config_to_engine(&mut engine, &config);
        let (lex_mtime, shared_lex) = get_shared_lexicon_with_mtime(&paths);
        engine.lexicon = (*shared_lex).clone();
        {
            let mut guard = INSTANCE_LEX_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .get_or_insert_with(std::collections::HashMap::new)
                .insert(inst_key, lex_mtime);
        }

        if paths.transitions.exists() && paths.transitions != paths.data_dir.join("transitions_user.tsv") {
            if let Some(p_str) = paths.transitions.to_str() {
                let _ = engine.transitions.load_tsv(p_str);
            }
        }
        if let Ok(fc_meta) = std::fs::metadata(&paths.four_corner) {
            if let Ok(fc_mtime) = fc_meta.modified() {
                let mut guard = INSTANCE_FC_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
                guard
                    .get_or_insert_with(std::collections::HashMap::new)
                    .insert(inst_key, fc_mtime);
            }
        }
        if paths.templates.exists() {
            let _ = engine.templates.load_tsv(&paths.templates);
            if let Ok(tpl_meta) = std::fs::metadata(&paths.templates) {
                if let Ok(tpl_mtime) = tpl_meta.modified() {
                    let mut guard = INSTANCE_TPL_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
                    guard
                        .get_or_insert_with(std::collections::HashMap::new)
                        .insert(inst_key, tpl_mtime);
                }
            }
        }
        let sandbox_sym_tree = paths.sandbox_dir.join("symbol_tree.tsv");
        if sandbox_sym_tree.exists() {
            ruhi::symbol_tree::configure_tsv_path(&sandbox_sym_tree);
        }
        engine.longkan.table.configure_data_dir(&paths.data_dir);

        *self.engine.lock().unwrap_or_else(|e| e.into_inner()) = engine;
        {
            let mut cw_guard = self.candidate_window.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing_cw) = cw_guard.as_mut() {
                existing_cw.apply_config(&config);
                existing_cw.hide();
            } else {
                *cw_guard = match crate::candidate_window::CandidateWindow::new() {
                    Ok(mut w) => {
                        w.apply_config(&config);
                        Some(w)
                    }
                    Err(_) => None,
                };
            }
        }
        self.pending_compose.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.sentence_cursor.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.candidate_selecting.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.active_candidates.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.selected_candidate_idx.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.target_char_idx.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.target_range.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0);
        *self.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
        *self.sel_anchor.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.insert_mode.lock().unwrap_or_else(|e| e.into_inner()) = true;
        *self.q_press_count.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.q_last_char_idx.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.pending_syllables.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.pending_segments.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.last_raw_ascii.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.manual_cuts.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.pending_pinned.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.deleted_words.lock().unwrap_or_else(|e| e.into_inner()) = del_words;
        self.undo_stack.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.redo_stack.lock().unwrap_or_else(|e| e.into_inner()).clear();

        let tm_opt = self.thread_mgr.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(tm) = tm_opt {
            if let Ok(langbar_mgr) = tm.cast::<ITfLangBarItemMgr>() {
                let mut item_guard = self.langbar_item.lock().unwrap_or_else(|e| e.into_inner());
                if item_guard.is_none() {
                    unsafe {
                        if let Ok(existing) = langbar_mgr.GetItem(&GUID_LBI_INPUTMODE) {
                            let _ = langbar_mgr.RemoveItem(&existing);
                        }
                    }
                    let btn: ITfLangBarItemButton = RuhiLangBarItemButton.into();
                    if let Ok(item) = btn.cast::<ITfLangBarItem>() {
                        unsafe {
                            if langbar_mgr.AddItem(&item).is_ok() {
                                let _ = item.Show(BOOL::from(true));
                                *item_guard = Some(item);
                            }
                        }
                    }
                }
                notify_langbar_mode_update();
            }
        }

        logger::log("Hot-upgraded legacy RuhiTextService instance in-place successfully!");
    }
}

impl ITfTextInputProcessor_Impl for RuhiTextService_Impl {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.ensure_instance_upgraded();
            logger::log(&format!("Activate called with tid = {}", tid));
            let tm = match ptim {
                Some(t) => t,
                None => {
                    logger::log("Activate: ptim is None!");
                    return Err(Error::from_hresult(E_FAIL));
                }
            };
            *self.thread_mgr.lock().unwrap() = Some(tm.clone());
            *self.client_id.lock().unwrap() = tid;
            if let Ok(mut cfg) = self.config.lock() {
                sync_active_tsf_profile_locale(&mut cfg);
                if let Ok(mut eng) = self.engine.lock() {
                    apply_config_to_engine(&mut eng, &cfg);
                }
            }
            {
                let mut cw_guard = self.candidate_window.lock().unwrap();
                if cw_guard.is_none() {
                    if let Ok(mut w) = crate::candidate_window::CandidateWindow::new() {
                        if let Ok(cfg) = self.config.lock() {
                            w.apply_config(&cfg);
                        }
                        *cw_guard = Some(w);
                    }
                }
            }

            let keystroke_mgr: ITfKeystrokeMgr = tm.cast()?;
            let sink_key: ITfKeyEventSink = self.to_object().cast()?;
            unsafe {
                keystroke_mgr.AdviseKeyEventSink(tid, &sink_key, true)?;
            }

            if let Ok(source) = tm.cast::<ITfSource>() {
                if let Ok(sink_tm) = self.to_object().cast::<ITfThreadMgrEventSink>() {
                    unsafe {
                        if let Ok(cookie) = source.AdviseSink(&ITfThreadMgrEventSink::IID, &sink_tm) {
                            *self.thread_mgr_sink_cookie.lock().unwrap() = cookie;
                        }
                    }
                }
            }

            if let Ok(langbar_mgr) = tm.cast::<ITfLangBarItemMgr>() {
                unsafe {
                    if let Ok(existing) = langbar_mgr.GetItem(&GUID_LBI_INPUTMODE) {
                        let _ = langbar_mgr.RemoveItem(&existing);
                    }
                }
                let btn: ITfLangBarItemButton = RuhiLangBarItemButton.into();
                if let Ok(item) = btn.cast::<ITfLangBarItem>() {
                    unsafe {
                        if langbar_mgr.AddItem(&item).is_ok() {
                            let _ = item.Show(BOOL::from(true));
                            if let Ok(mut guard) = self.langbar_item.lock() {
                                *guard = Some(item);
                            }
                        }
                    }
                }
                notify_langbar_mode_update();
            }

            logger::log("Activate finished successfully (S_OK)!");
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn Deactivate(&self) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            logger::log("Deactivate called.");
            let inst_key = Arc::as_ptr(&self.client_id) as usize;
            let comp_key = Arc::as_ptr(&self.composition) as usize;
            cleanup_instance_side_tables(inst_key, comp_key);
            self.hide_candidate_window();
            if let Ok(mut cw_guard) = self.candidate_window.try_lock() {
                *cw_guard = None;
            }
            set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);
            if let Ok(tm_lock) = self.thread_mgr.try_lock() {
                if let Some(tm) = tm_lock.as_ref() {
                    let tid = self.client_id.try_lock().map(|g| *g).unwrap_or(0);
                    if let Ok(keystroke_mgr) = tm.cast::<ITfKeystrokeMgr>() {
                        unsafe { let _ = keystroke_mgr.UnadviseKeyEventSink(tid); }
                    }
                    if let Ok(source) = tm.cast::<ITfSource>() {
                        let tm_cookie = self.thread_mgr_sink_cookie.try_lock().map(|g| *g).unwrap_or(0);
                        if tm_cookie != 0 {
                            unsafe { let _ = source.UnadviseSink(tm_cookie); }
                        }
                    }
                    if let Ok(langbar_mgr) = tm.cast::<ITfLangBarItemMgr>() {
                        if let Ok(mut guard) = self.langbar_item.try_lock() {
                            if let Some(item) = guard.take() {
                                unsafe { let _ = langbar_mgr.RemoveItem(&item); }
                            }
                        }
                    }
                }
            }
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

impl ITfTextInputProcessorEx_Impl for RuhiTextService_Impl {
    fn ActivateEx(&self, ptim: Option<&ITfThreadMgr>, tid: u32, dwflags: u32) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            logger::log(&format!("ActivateEx called with tid = {}, flags = {:#x}", tid, dwflags));
            self.Activate(ptim, tid)
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

impl ITfKeyEventSink_Impl for RuhiTextService_Impl {
    fn OnSetFocus(&self, fforeground: BOOL) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.ensure_instance_upgraded();
            if fforeground.as_bool() {
                let paths = RuhiPaths::resolve();
                let (lex_mtime, shared_lex) = get_shared_lexicon_with_mtime(&paths);
                let inst_key = Arc::as_ptr(&self.client_id) as usize;
                let need_lex_clone = if let Ok(guard) = INSTANCE_LEX_MTIMES.lock() {
                    guard.as_ref().and_then(|m| m.get(&inst_key).copied()) != Some(lex_mtime)
                } else {
                    true
                };
                if let Ok(mut eng) = self.engine.lock() {
                    if need_lex_clone || eng.lexicon.entries.is_empty() {
                        eng.lexicon = (*shared_lex).clone();
                        if let Ok(mut guard) = INSTANCE_LEX_MTIMES.lock() {
                            guard
                                .get_or_insert_with(std::collections::HashMap::new)
                                .insert(inst_key, lex_mtime);
                        }
                    }
                    eng.transitions.maybe_reload();
                    eng.symbols.maybe_reload();
                    if paths.array30.exists() && eng.array30.table.reverse_lookup('㐀').is_none() {
                        let _ = eng.array30.table.load_tsv(&paths.array30);
                    }
                    if paths.four_corner.exists() && eng.four_corner.table.code_to_entries.len() < 1000 {
                        let _ = eng.four_corner.load_tsv(&paths.four_corner);
                    }
                    eng.longkan.table.configure_data_dir(&paths.data_dir);
                }
                let mut new_cfg = load_ruhi_config(&paths);
                sync_active_tsf_profile_locale(&mut new_cfg);
                if let Ok(mut eng) = self.engine.lock() {
                    apply_config_to_engine(&mut eng, &new_cfg);
                }
                if let Ok(mut cw_guard) = self.candidate_window.lock() {
                    if let Some(cw) = cw_guard.as_mut() {
                        cw.apply_config(&new_cfg);
                    }
                }
                *self.config.lock().unwrap_or_else(|e| e.into_inner()) = new_cfg.clone();
                crate::candidate_window::maybe_spawn_background_telemetry_upload(&paths, &new_cfg);
                notify_langbar_mode_update();
            } else {
                // 當前視窗失去焦點時，立即隱藏輸入座，絕不在背景凍結殘留！
                bump_composition_focus_epoch(&self.composition, None);
                clear_western_leading_sigil(self.western_instance_key());
                self.hide_candidate_window();
                set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);
            }
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn OnTestKeyDown(&self, pic: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            clear_tested_pwd_token();
            let vk = wparam.0 as u32;
            if is_passthrough_non_typing_vk(vk) {
                return Ok(BOOL::from(false));
            }

            let is_ctrl = unsafe { ((GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) };
            let is_alt = unsafe { ((GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) };
            let is_shift = unsafe { ((GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) };
            let is_win = is_win_modifier_down();
            let is_altgr = !is_win && is_ctrl && is_alt && is_rmenu_down();
            let maybe_sym_hk = vk == 0xBC && is_ctrl && is_alt && !is_shift && !is_win;

            if is_win || (is_alt && !is_altgr && !maybe_sym_hk) {
                return Ok(BOOL::from(false));
            }

            self.ensure_instance_upgraded();
            self.check_reload_config();

            let is_synthetic_caps = if vk == VK_CAPITAL.0 as u32 {
                is_synthetic_caps_active() || SUPPRESS_SYNTHETIC_CAPS.load(std::sync::atomic::Ordering::SeqCst)
            } else {
                false
            };

            if !self.evaluate_test_key_down_full(vk, is_ctrl, is_alt, is_shift, is_win, is_altgr, false, is_synthetic_caps) {
                return Ok(BOOL::from(false));
            }

            let (has_buffer, ..) = self.snapshot_key_routing_flags();
            // Win32 與 TSF InputScope 密碼控制項隱私保護：若當前聚焦於密碼欄位且無未決組字，自動放行直通英數，絕不攔截組字或寫入修訂記錄
            if !has_buffer {
                let ctx_ptr = pic
                    .map(|c| windows_core::Interface::as_raw(c) as usize)
                    .unwrap_or(0);
                let is_pwd = self.is_focused_or_tsf_password(pic);
                store_tested_pwd_token_at(ctx_ptr, vk, std::time::Instant::now(), is_pwd);
                if is_pwd {
                    clear_western_leading_sigil(self.western_instance_key());
                    return Ok(BOOL::from(false));
                }
            }

            Ok(BOOL::from(true))
        })).unwrap_or_else(|_| Ok(BOOL::from(false)))
    }



    fn OnKeyDown(&self, pic: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let ctx = match pic {
                Some(c) => c,
                None => {
                    return Ok(BOOL::from(false));
                }
            };

            let vk = wparam.0 as u32;
            if is_passthrough_non_typing_vk(vk) {
                return Ok(BOOL::from(false));
            }

            let is_ctrl = unsafe { ((GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) };
            let is_alt = unsafe { ((GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) };
            let is_shift = unsafe { ((GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) };
            let is_win = is_win_modifier_down();
            let is_altgr = !is_win && is_ctrl && is_alt && is_rmenu_down();
            let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };

            let maybe_sym_hk = vk == 0xBC && is_ctrl && is_alt && !is_shift && !is_win;
            if is_win || (is_alt && !is_altgr && !maybe_sym_hk) {
                return Ok(BOOL::from(false));
            }

            self.ensure_instance_upgraded();
            self.check_reload_config();

            let (
                has_buffer,
                is_english_mode,
                is_four_corner_mode,
                is_longkan_mode,
                _is_standard_mode,
                is_en_inline_or_special,
                tilde_triggers_mode,
            ) = self.snapshot_key_routing_flags();
            let symbol_tree_hotkey = is_symbol_tree_hotkey_combo(
                vk,
                is_ctrl,
                is_alt,
                is_shift,
                is_win,
                is_altgr,
                is_english_mode,
            );
            if is_alt && !(is_altgr && is_english_mode) && !symbol_tree_hotkey {
                return Ok(BOOL::from(false));
            }
            let (is_fullwidth, allow_shift_space, punct_mode) = self
                .config
                .lock()
                .map(|c| {
                    (
                        c.fullwidth_mode,
                        c.shift_space_toggle_fullwidth,
                        c.punctuation_commit_mode.clone(),
                    )
                })
                .unwrap_or_else(|_| (false, true, "B".to_string()));

            // 空白狀態快速放行閘門（先於密碼欄位 COM 探測與一切模式邏輯，全體 10 種鍵盤模式行為一致）
            if !has_buffer {
                if is_ctrl && !is_altgr && vk != VK_CAPITAL.0 as u32 && !symbol_tree_hotkey {
                    return Ok(BOOL::from(false));
                }
                if !is_ctrl || is_altgr {
                    if vk == VK_SPACE.0 as u32 {
                        if !is_shift && !is_fullwidth {
                            return Ok(BOOL::from(false));
                        }
                        if is_shift && !allow_shift_space {
                            return Ok(BOOL::from(false));
                        }
                    }
                    if vk == VK_RETURN.0 as u32
                        || vk == VK_BACK.0 as u32
                        || vk == VK_DELETE.0 as u32
                        || vk == VK_INSERT.0 as u32
                        || vk == VK_TAB.0 as u32
                        || vk == VK_ESCAPE.0 as u32
                        || (vk == 0x6E && (!is_four_corner_mode || is_shift))
                        || vk == VK_LEFT.0 as u32
                        || vk == VK_RIGHT.0 as u32
                        || vk == VK_UP.0 as u32
                        || vk == VK_DOWN.0 as u32
                        || vk == VK_HOME.0 as u32
                        || vk == VK_END.0 as u32
                        || vk == VK_PRIOR.0 as u32
                        || vk == VK_NEXT.0 as u32
                    {
                        return Ok(BOOL::from(false));
                    }
                }
            }

            // Win32 與 TSF InputScope 密碼控制項隱私保護：沿用 50ms 內單鍵 OnTestKeyDown 探測結果或補測，且絕不於密碼框寫入按鍵日誌
            if !has_buffer {
                let ctx_ptr = windows_core::Interface::as_raw(ctx) as usize;
                let is_pwd = take_valid_tested_pwd_token(ctx_ptr, vk)
                    .unwrap_or_else(|| self.is_focused_or_tsf_password(Some(ctx)));
                if is_pwd {
                    return Ok(BOOL::from(false));
                }
            }

            logger::log(&format!("OnKeyDown: vk={:#x}, ctrl={}, shift={}", vk, is_ctrl, is_shift));

            // Ctrl + Alt + ,：9×9×9 多層分類樹符號系統快捷鍵（兼顧已開啟時切換關閉）
            if symbol_tree_hotkey {
                let already_open = self
                    .engine
                    .lock()
                    .map(|e| e.mode == ruhi::engine::EngineMode::SymbolTree)
                    .unwrap_or(false);
                *self.candidate_selecting.lock().unwrap() = false;
                if already_open {
                    self.engine.lock().unwrap().close_symbol_tree();
                } else {
                    if self.get_manual_selection_range().is_none() {
                        let should_consolidate = {
                            let eng = self.engine.lock().unwrap();
                            !eng.composed_sentence.is_empty() && !eng.fsm.has_pending()
                        };
                        if should_consolidate {
                            self.consolidate_engine_to_pending();
                        }
                    }
                    let has_sel = self.get_manual_selection_range().is_some();
                    self.engine.lock().unwrap().open_symbol_tree(has_sel);
                }
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            if self
                .engine
                .lock()
                .map(|e| e.mode == ruhi::engine::EngineMode::SymbolTree)
                .unwrap_or(false)
            {
                return self.handle_symbol_tree_key_down(ctx, vk, is_ctrl, is_shift);
            }

            // Shift + Space：半形 (`半`) 與全形 (`全`) 模式切換
            if vk == VK_SPACE.0 as u32 && is_shift && !is_ctrl && allow_shift_space {
                let (new_fw, cur_kb_mode) = {
                    let mut eng = self.engine.lock().unwrap();
                    eng.fullwidth_mode = !eng.fullwidth_mode;
                    (eng.fullwidth_mode, eng.keyboard_mode.clone())
                };
                if let Ok(mut cfg) = self.config.lock() {
                    cfg.fullwidth_mode = new_fw;
                    if let Ok(mut cw_guard) = self.candidate_window.lock() {
                        if let Some(cw) = cw_guard.as_mut() {
                            cw.apply_config(&cfg);
                        }
                    }
                }
                crate::candidate_window::save_fullwidth_mode(new_fw);
                let (full, _) = self.get_full_display_and_cursor();
                if !full.is_empty() {
                    self.sync_input_display(ctx)?;
                } else {
                    let (cx, cy) = Self::get_caret_screen_pos();
                    if let Ok(mut cw_guard) = self.candidate_window.lock() {
                        if let Some(cw) = cw_guard.as_mut() {
                            cw.show_mode_toast(&cur_kb_mode, is_caps_on, cx, cy);
                        }
                    }
                }
                notify_langbar_mode_update();
                return Ok(BOOL::from(true));
            }

            // Caps Lock 三向智慧切換處理：
            // 1. 單獨按下 Caps Lock ➔ 最後兩個輸入模式快速對切（如 英⇄漢、日⇄漢、英⇄日）
            // 2. Shift + Caps Lock ➔ 在切換列表中往前切換 (Prev)
            // 3. Ctrl + Caps Lock ➔ 在切換列表中往後切換 (Next)
            // 並自動中和硬體 Caps Lock 燈號反轉，保持鍵盤 Caps Lock 燈號熄滅且組字串絕不中斷！
            if vk == VK_CAPITAL.0 as u32 {
                if is_synthetic_caps_active() || SUPPRESS_SYNTHETIC_CAPS.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    return Ok(BOOL::from(false));
                }
                let capslock_none = self
                    .config
                    .lock()
                    .map(|c| c.capslock_switch == "none")
                    .unwrap_or(false);
                let is_single_mode = self
                    .engine
                    .lock()
                    .map(|e| e.get_full_cycle_modes().len() <= 1)
                    .unwrap_or(false);
                if (!is_ctrl && !is_shift && capslock_none) || is_single_mode {
                    return Ok(BOOL::from(false));
                }
                let action = if is_ctrl {
                    "next"
                } else if is_shift {
                    "prev"
                } else {
                    "last_two"
                };
                self.switch_keyboard_mode_action(ctx, action)?;
                if is_caps_on {
                    trigger_synthetic_caps_toggle();
                }
                crate::candidate_window::broadcast_tray_update();
                return Ok(BOOL::from(true));
            }

            // 無組字緩衝區下全形模式單按空白鍵：直接送出全形空白「　」
            if !has_buffer && vk == VK_SPACE.0 as u32 && !is_shift && !is_ctrl && is_fullwidth {
                self.update_tsf_composition(ctx, "\u{3000}", 1, None, true)?;
                return Ok(BOOL::from(true));
            }

            let quick_mode = self.effective_quick_key_mode();

            // 1. 處理 Ctrl 組合鍵與編輯器操作 (Ctrl+A, Ctrl+C, Ctrl+Insert, Ctrl+V, Ctrl+X, Ctrl+Z, Ctrl+Y/R, Ctrl+Home/End/Delete 等；必須優先於歐文與各輸入模式！)
            if is_ctrl && !is_altgr {
                // 1.0 優先檢查 Ctrl 符號組合鍵 (如 Ctrl+. 輸出刪節號 ……、Ctrl+, 輸出全形逗號等；惟歐文與空白四角模式下絕不攔截 IDE/編輯器快捷鍵)
                if let Some(combo) = resolve_ctrl_symbol_combo(
                    vk,
                    is_shift,
                    has_buffer,
                    is_english_mode,
                    is_four_corner_mode,
                ) {
                    let has_symbol = {
                        let mut engine = self.engine.lock().unwrap();
                        engine.symbols.contains_key(&combo)
                    };

                    if has_symbol {
                        self.consolidate_engine_to_pending();

                        let mut engine = self.engine.lock().unwrap();
                        let resp = engine.process_key_str(&combo);
                        let committed_chunk = resp.committed_chunk;
                        drop(engine);

                        if !committed_chunk.is_empty() {
                            self.push_undo();

                            let is_multiline_desk = self
                                .pending_compose
                                .lock()
                                .map(|p| p.contains('\n'))
                                .unwrap_or(false);
                            let is_sentence_terminator = matches!(
                                committed_chunk.as_str(),
                                "。" | "？" | "！" | "；" | "…" | "……" | "?" | "!"
                            );
                            let should_keep_in_stream = is_multiline_desk
                                || match punct_mode.as_str() {
                                    "C" => true,
                                    "B" => !is_sentence_terminator,
                                    _ => false, // 模式 A：經典即時結算
                                };

                            if should_keep_in_stream {
                                let p_chars: Vec<char> = committed_chunk.chars().collect();
                                let p_syls: Vec<Option<String>> = vec![None; p_chars.len()];
                                self.splice_pending_chunk(&p_chars, &p_syls, &[]);
                                self.sync_input_display(ctx)?;
                            } else {
                                // 結算送出全句！
                                let full = format!("{}{}", *self.pending_compose.lock().unwrap(), committed_chunk);
                                self.clear_all_pending();
                                *self.sentence_cursor.lock().unwrap() = 0;
                                let c = full.chars().count();
                                self.update_tsf_composition(ctx, &full, c, None, true)?;
                                self.hide_candidate_window();
                            }
                        }
                        return Ok(BOOL::from(true));
                    }
                }

                if !has_buffer {
                    return Ok(BOOL::from(false));
                }

                // 1.1 方向鍵、Home/End 與退格/刪除鍵放行至後方常式處理 (詞間快進/快退/首尾/刪詞)
                if vk == VK_LEFT.0 as u32
                    || vk == VK_RIGHT.0 as u32
                    || vk == VK_UP.0 as u32
                    || vk == VK_DOWN.0 as u32
                    || vk == VK_HOME.0 as u32
                    || vk == VK_END.0 as u32
                    || vk == VK_BACK.0 as u32
                    || vk == VK_DELETE.0 as u32
                {
                    // 放行至方向鍵或退格／刪除鍵處理常式，執行詞間快進／快退、移至首尾或字詞刪除！
                } else if vk == 0x41 {
                    // Ctrl+A：全選組字行
                    let (full_text, _) = self.get_full_display_and_cursor();
                    let total_chars = full_text.chars().count();
                    if total_chars > 0 {
                        *self.sel_anchor.lock().unwrap() = Some(0);
                        *self.sentence_cursor.lock().unwrap() = total_chars;
                        self.sync_input_display(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                } else if vk == 0x43 {
                    // Ctrl+C：複製 / 取消
                    if let Some((start, end)) = self.get_manual_selection_range() {
                        let (full, _) = self.get_full_display_and_cursor();
                        let chars: Vec<char> = full.chars().collect();
                        if start < end && end <= chars.len() {
                            let text: String = chars[start..end].iter().collect();
                            set_clipboard_text(&text);
                        }
                    } else {
                        // 無選取範圍時，依約定作為取消 (Break/Cancel)，全句撤銷清空
                        self.clear_all_pending();
                        *self.sentence_cursor.lock().unwrap() = 0;
                        *self.sel_anchor.lock().unwrap() = None;
                        self.engine.lock().unwrap().reset_buffer();
                        self.hide_candidate_window();
                        self.end_tsf_composition(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                } else if vk == VK_INSERT.0 as u32 && !is_shift {
                    // Ctrl+Insert：通用 CUA 複製選取文字
                    if let Some((start, end)) = self.get_manual_selection_range() {
                        let (full, _) = self.get_full_display_and_cursor();
                        let chars: Vec<char> = full.chars().collect();
                        if start < end && end <= chars.len() {
                            let text: String = chars[start..end].iter().collect();
                            set_clipboard_text(&text);
                        }
                    }
                    return Ok(BOOL::from(true));
                } else if vk == 0x58 {
                    // Ctrl+X：剪下
                    if let Some((start, end)) = self.get_manual_selection_range() {
                        let (full, _) = self.get_full_display_and_cursor();
                        let chars: Vec<char> = full.chars().collect();
                        if start < end && end <= chars.len() {
                            let text: String = chars[start..end].iter().collect();
                            set_clipboard_text(&text);
                            self.push_undo();
                            self.delete_selection_if_any();
                            let (after_full, _) = self.get_full_display_and_cursor();
                            if !after_full.is_empty() {
                                self.sync_input_display(ctx)?;
                            } else {
                                self.hide_candidate_window();
                                self.end_tsf_composition(ctx)?;
                            }
                        }
                    }
                    return Ok(BOOL::from(true));
                } else if vk == 0x56 {
                    // Ctrl+V：貼上剪貼簿文字（標準化換行並原子覆寫選區／保存撤銷快照）
                    if let Some(text) = get_clipboard_text() {
                        if self.paste_text_into_desk(&text) {
                            self.sync_input_display(ctx)?;
                        }
                    }
                    return Ok(BOOL::from(true));
                } else if (vk == 0x5A && is_shift) || vk == 0x59 || vk == 0x52 {
                    // Ctrl+Shift+Z 或 Ctrl+Y 或 Ctrl+R：重做 Redo
                    if self.redo() {
                        self.sync_input_display(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                } else if vk == 0x5A {
                    // Ctrl+Z：復原 Undo
                    if self.undo() {
                        let (full, _) = self.get_full_display_and_cursor();
                        if !full.is_empty() {
                            self.sync_input_display(ctx)?;
                        } else {
                            self.hide_candidate_window();
                            self.end_tsf_composition(ctx)?;
                        }
                    }
                    return Ok(BOOL::from(true));
                } else if vk == VK_RETURN.0 as u32 && !is_shift {
                    // Ctrl+Enter：相當於連按兩下 Enter（將輸入座全句結算上屏並同步換行）
                    let ar30_passthrough = {
                        let mut engine = self.engine.lock().unwrap();
                        if engine.mode == ruhi::engine::EngineMode::LexiconSwitch {
                            let _ = engine.process_key('\n');
                            Some(("switch_lexicon".to_string(), String::new()))
                        } else if engine.cangjie.has_buffer()
                            || engine.array30.has_buffer()
                            || engine.four_corner.has_buffer()
                            || engine.longkan.has_buffer()
                            || engine.inline_longkan_active
                            || matches!(
                                engine.mode,
                                ruhi::engine::EngineMode::QuickNum | ruhi::engine::EngineMode::TemplateSelect
                            )
                        {
                            let resp = engine.process_key('\n');
                            Some((resp.action, resp.committed_chunk))
                        } else {
                            None
                        }
                    };
                    if let Some((act, chunk)) = ar30_passthrough {
                        if act == "switch_lexicon" {
                            self.sync_input_display(ctx)?;
                            return Ok(BOOL::from(true));
                        }
                        if !chunk.is_empty() {
                            let c_chars: Vec<char> = chunk.chars().collect();
                            let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                            self.splice_pending_chunk(&c_chars, &c_syls, &[]);
                        }
                    }
                    self.commit_current_composition_with_newline(ctx)?;
                    return Ok(BOOL::from(true));
                } else {
                    // 【全域防禦】：組字行尚有未送出內容時，其餘 Ctrl 組合鍵（如 Ctrl+S, Ctrl+W, Ctrl+Q 等）予以攔截保護組字現場！
                    return Ok(BOOL::from(true));
                }
            }

            // 英數／歐文模式 (english) 且未處於候選字選取模式時：
            // 若處於候選字選取模式（如按 ↓ 選 café / á，或按 ↑ 選 thanks），則放行至下方選字邏輯！
            if is_english_mode
                && !*self.candidate_selecting.lock().unwrap()
                && !is_en_inline_or_special
                && (!is_ctrl || is_altgr)
                && (!is_alt || is_altgr)
                && !is_win
            {
                let is_bs_pending = self
                    .engine
                    .lock()
                    .map(|e| e.is_backslash_escape_pending())
                    .unwrap_or(false);
                if vk == VK_SPACE.0 as u32 && has_buffer {
                    self.engine.lock().unwrap().set_backslash_escape_pending(false);
                    self.delete_selection_if_any();
                    self.consolidate_engine_to_pending();
                    let resolved =
                        self.translate_vk_in_english_mode(vk, is_shift, is_altgr, is_caps_on, true);
                    match resolved {
                        EnglishVkTranslation::Text(s) if s != " " => {
                            for ch in s.chars() {
                                self.insert_pending_char(ch, None);
                            }
                        }
                        _ => {
                            if !is_shift
                                && !is_ctrl
                                && !is_alt
                                && !is_fullwidth
                                && self.try_western_space_autocorrect_and_insert(' ')
                            {
                                // Tier 1 高確定性自動勘誤與空白已原子化套用
                            } else {
                                self.insert_pending_char(
                                    if is_fullwidth { '\u{3000}' } else { ' ' },
                                    None,
                                );
                            }
                        }
                    }
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                let is_non_char_nav_vk = matches!(
                    vk,
                    x if x == VK_RETURN.0 as u32
                        || x == VK_BACK.0 as u32
                        || x == VK_ESCAPE.0 as u32
                        || x == VK_DELETE.0 as u32
                        || x == VK_TAB.0 as u32
                        || x == VK_INSERT.0 as u32
                        || x == VK_LEFT.0 as u32
                        || x == VK_RIGHT.0 as u32
                        || x == VK_UP.0 as u32
                        || x == VK_DOWN.0 as u32
                        || x == VK_HOME.0 as u32
                        || x == VK_END.0 as u32
                        || x == VK_PRIOR.0 as u32
                        || x == VK_NEXT.0 as u32
                        || x == VK_SPACE.0 as u32
                );
                if !is_non_char_nav_vk {
                    let peek = self.translate_vk_in_english_mode(
                        vk,
                        is_shift,
                        is_altgr,
                        is_caps_on,
                        false,
                    );
                    let triggers_aux_mode = !is_bs_pending
                        && matches!(
                            &peek,
                            EnglishVkTranslation::Text(s)
                                if (!has_buffer && s == "`") || (tilde_triggers_mode && s == "~")
                        );
                    if triggers_aux_mode {
                        // 無組字狀態單按 `` ` `` 統一交由下方引擎進入「🔖 自訂詞庫」；
                        // 或當 `~` 設定為輔助輸入法對切／返回時，按 `~` 交由下方引擎處理
                        clear_western_leading_sigil(self.western_instance_key());
                    } else {
                        let trans = self.translate_vk_in_english_mode(
                            vk,
                            is_shift,
                            is_altgr,
                            is_caps_on,
                            true,
                        );
                        match trans {
                            EnglishVkTranslation::DeadKey => {
                                self.engine.lock().unwrap().set_backslash_escape_pending(false);
                                if has_buffer {
                                    return Ok(BOOL::from(true));
                                }
                                clear_western_leading_sigil(self.western_instance_key());
                                return Ok(BOOL::from(false));
                            }
                            EnglishVkTranslation::Text(s) => {
                                let is_single_bs = s == "\\";
                                if !has_buffer
                                    && !is_bs_pending
                                    && !is_single_bs
                                    && !s.chars().any(Self::is_western_leading_word_char)
                                {
                                    if s.chars().all(
                                        ruhi::engine::RuhiEngine::is_western_token_boundary_blocker,
                                    ) {
                                        if let Some(ch) = s.chars().last() {
                                            record_western_leading_sigil(
                                                self.western_instance_key(),
                                                ch,
                                            );
                                        }
                                    } else {
                                        clear_western_leading_sigil(self.western_instance_key());
                                    }
                                    return Ok(BOOL::from(false));
                                }
                                self.delete_selection_if_any();
                                self.consolidate_engine_to_pending();
                                if is_bs_pending {
                                    self.engine.lock().unwrap().set_backslash_escape_pending(false);
                                    if matches!(s.as_str(), "`" | "\\" | "~") {
                                        let cur = *self.sentence_cursor.lock().unwrap();
                                        let prev_is_bs = cur > 0
                                            && self
                                                .pending_compose
                                                .lock()
                                                .map(|p| {
                                                    matches!(
                                                        p.chars().nth(cur - 1),
                                                        Some('\\' | '＼')
                                                    )
                                                })
                                                .unwrap_or(false);
                                        if prev_is_bs {
                                            self.remove_pending_char_at(cur - 1);
                                            *self.sentence_cursor.lock().unwrap() = cur - 1;
                                        }
                                        for ch in s.chars() {
                                            self.insert_pending_char(ch, None);
                                        }
                                    } else if is_single_bs {
                                        self.insert_pending_char(
                                            if is_fullwidth { '＼' } else { '\\' },
                                            None,
                                        );
                                        self.engine
                                            .lock()
                                            .unwrap()
                                            .set_backslash_escape_pending(true);
                                    } else {
                                        for ch in s.chars() {
                                            self.insert_pending_char(ch, None);
                                        }
                                    }
                                } else if is_single_bs {
                                    self.insert_pending_char(
                                        if is_fullwidth { '＼' } else { '\\' },
                                        None,
                                    );
                                    self.engine.lock().unwrap().set_backslash_escape_pending(true);
                                } else {
                                    self.engine.lock().unwrap().set_backslash_escape_pending(false);
                                    for ch in s.chars() {
                                        self.insert_pending_char(ch, None);
                                    }
                                }
                                self.sync_input_display(ctx)?;
                                return Ok(BOOL::from(true));
                            }
                            EnglishVkTranslation::None => {
                                return Ok(BOOL::from(false));
                            }
                        }
                    }
                }
            }

            // 2. 候選字選取模式 (Candidate Selecting Mode)
            let is_cand_sel = *self.candidate_selecting.lock().unwrap();
            if is_cand_sel {
                let cands = self.active_candidates.lock().unwrap().clone();
                let cands_len = cands.len();

                // 若在 Shift+↓ 跨輸入法查詢視窗中按下 Tab 鍵：即時循環切換跨查輸入模式並原地刷新候選字詞
                // 否則在普通候選字詞選單中按下 Tab / Shift+Tab：向前／向後循環選取候選項目
                if vk == VK_TAB.0 as u32 {
                    let is_lk_popup = self
                        .engine
                        .lock()
                        .map(|e| e.is_shift_down_longkan_popup())
                        .unwrap_or(false);
                    if is_lk_popup {
                        let next_mode = self.engine.lock().unwrap().cycle_shift_down_mode();
                        crate::candidate_window::save_shift_down_mode(&next_mode);
                        let cur = *self.sentence_cursor.lock().unwrap();
                        let (full_text, _) = self.get_full_display_and_cursor();
                        let (target_idx, target_range, lk_cands) =
                            self.detect_shift_down_longkan_target_and_cands(&full_text, cur);
                        if !lk_cands.is_empty() {
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_shift_down_longkan_popup(true);
                            }
                            set_tab_range_popup_active(self.western_instance_key(), false);
                            *self.target_char_idx.lock().unwrap() = target_idx;
                            *self.target_range.lock().unwrap() = target_range;
                            *self.sentence_cursor.lock().unwrap() = target_idx;
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = lk_cands;
                            self.sync_input_display(ctx)?;
                        }
                        return Ok(BOOL::from(true));
                    } else if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        *sel = if is_shift {
                            if *sel == 0 {
                                cands_len - 1
                            } else {
                                *sel - 1
                            }
                        } else {
                            (*sel + 1) % cands_len
                        };
                        let new_idx = *sel;
                        drop(sel);
                        self.update_candidate_selection_ui(new_idx, &cands);
                        return Ok(BOOL::from(true));
                    }
                }

                // ↓ 下方向鍵：下一個候選（若尚未進入 Shift+↓ 跨查模式而此時按下 Shift+↓，則即時切換為跨輸入法候選清單）
                if vk == VK_DOWN.0 as u32 {
                    let is_lk_popup = self
                        .engine
                        .lock()
                        .map(|e| e.is_shift_down_longkan_popup())
                        .unwrap_or(false);
                    if is_shift && !is_lk_popup {
                        let cur = *self.sentence_cursor.lock().unwrap();
                        let (full_text, _) = self.get_full_display_and_cursor();
                        let (target_idx, target_range, lk_cands) =
                            self.detect_shift_down_longkan_target_and_cands(&full_text, cur);
                        if !lk_cands.is_empty() {
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_shift_down_longkan_popup(true);
                            }
                            set_tab_range_popup_active(self.western_instance_key(), false);
                            *self.target_char_idx.lock().unwrap() = target_idx;
                            *self.target_range.lock().unwrap() = target_range;
                            *self.sentence_cursor.lock().unwrap() = target_idx;
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = lk_cands;
                            self.sync_input_display(ctx)?;
                            return Ok(BOOL::from(true));
                        }
                    }
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        *sel = (*sel + 1) % cands_len;
                        let new_idx = *sel;
                        drop(sel);
                        self.update_candidate_selection_ui(new_idx, &cands);
                    }
                    return Ok(BOOL::from(true));
                }

                // → 右方向鍵：下一個候選（候選區彈出時左右方向鍵在候選字詞間移動，提升 UX 體驗）
                if vk == VK_RIGHT.0 as u32 && !is_shift && !is_ctrl {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        *sel = (*sel + 1) % cands_len;
                        let new_idx = *sel;
                        drop(sel);
                        self.update_candidate_selection_ui(new_idx, &cands);
                    }
                    return Ok(BOOL::from(true));
                }

                // ↑ 上方向鍵 / ← 左方向鍵：上一個候選
                if vk == VK_UP.0 as u32 || (vk == VK_LEFT.0 as u32 && !is_shift && !is_ctrl) {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        *sel = if *sel == 0 { cands_len - 1 } else { *sel - 1 };
                        let new_idx = *sel;
                        drop(sel);
                        self.update_candidate_selection_ui(new_idx, &cands);
                    }
                    return Ok(BOOL::from(true));
                }

                let page_sz = if is_four_corner_mode { 10usize } else { 9usize };

                // Page Down 鍵 (VK_NEXT) 或四角模式下之 `+`：向下翻一頁（四角 10 個候選／一般 9 個候選）
                if vk == VK_NEXT.0 as u32 || (is_four_corner_mode && (vk == 0x6B || (vk == 0xBB && is_shift))) {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        let cur_page = *sel / page_sz;
                        let total_pages = (cands_len + page_sz - 1) / page_sz;
                        let next_page = (cur_page + 1) % total_pages.max(1);
                        *sel = (next_page * page_sz).min(cands_len - 1);
                        let new_idx = *sel;
                        drop(sel);
                        self.update_candidate_selection_ui(new_idx, &cands);
                    }
                    return Ok(BOOL::from(true));
                }

                // Page Up 鍵 (VK_PRIOR) 或四角模式下之 `-`：向上翻一頁（四角 10 個候選／一般 9 個候選）
                if vk == VK_PRIOR.0 as u32 || (is_four_corner_mode && (vk == 0x6D || (vk == 0xBD && !is_shift))) {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        let cur_page = *sel / page_sz;
                        let total_pages = (cands_len + page_sz - 1) / page_sz;
                        let prev_page = if cur_page > 0 { cur_page - 1 } else { total_pages.saturating_sub(1) };
                        *sel = (prev_page * page_sz).min(cands_len - 1);
                        let new_idx = *sel;
                        drop(sel);
                        self.update_candidate_selection_ui(new_idx, &cands);
                    }
                    return Ok(BOOL::from(true));
                }

                let quick_idx = if !is_four_corner_mode && quick_mode == "asdf" {
                    if !is_shift { map_asdf_to_idx(vk) } else { None }
                } else if !is_shift && (0x31..=0x39).contains(&vk) {
                    Some((vk - 0x31) as usize)
                } else if !is_shift && (0x61..=0x69).contains(&vk) {
                    Some((vk - 0x61) as usize)
                } else if !is_shift && is_four_corner_mode && (vk == 0x30 || vk == 0x60) {
                    Some(9usize)
                } else {
                    None
                };

                let chosen_cand_idx = if let Some(q_idx) = quick_idx {
                    let cur_page = *self.selected_candidate_idx.lock().unwrap() / page_sz;
                    let target_cand = cur_page * page_sz + q_idx;
                    if target_cand < cands_len { Some(target_cand) } else { None }
                } else if vk == VK_SPACE.0 as u32 || vk == VK_RETURN.0 as u32 {
                    let sel = *self.selected_candidate_idx.lock().unwrap();
                    if sel < cands_len { Some(sel) } else { None }
                } else {
                    None
                };

                if let Some(cand_i) = chosen_cand_idx {
                    let chosen = &cands[cand_i];

                    // 若目前為「Shift+↓ 跨輸入法查詢模式選擇器」（如已在龍龕模式按 Shift+↓ 彈出詢問選單）：
                    // 選中目標輸入模式後，立即切換 shift_down_mode、寫入設定檔，並原地刷新為該模式之候選字詞！
                    let is_picker = self
                        .engine
                        .lock()
                        .map(|e| e.is_shift_down_mode_picker())
                        .unwrap_or(false);
                    if is_picker {
                        if let Some(new_mode) = RuhiEngine::parse_shift_down_picker_candidate(chosen) {
                            {
                                let mut eng = self.engine.lock().unwrap();
                                eng.shift_down_mode = new_mode.clone();
                                eng.set_active_shift_down_mode_override(Some(new_mode.clone()));
                                eng.set_shift_down_mode_picker(false);
                                eng.set_shift_down_longkan_popup(true);
                            }
                            crate::candidate_window::save_shift_down_mode(&new_mode);
                            let cur = *self.sentence_cursor.lock().unwrap();
                            let (full_text, _) = self.get_full_display_and_cursor();
                            let (target_idx, target_range, lk_cands) =
                                self.detect_shift_down_longkan_target_and_cands(&full_text, cur);
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_shift_down_longkan_popup(true);
                                eng.set_shift_down_mode_picker(false);
                            }
                            set_tab_range_popup_active(self.western_instance_key(), false);
                            *self.target_char_idx.lock().unwrap() = target_idx;
                            *self.target_range.lock().unwrap() = target_range;
                            *self.sentence_cursor.lock().unwrap() = target_idx;
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = lk_cands;
                            self.sync_input_display(ctx)?;
                            return Ok(BOOL::from(true));
                        }
                    }

                    self.apply_candidate_selection_in_desk(chosen, &cands);
                    if is_four_corner_mode && vk == VK_RETURN.0 as u32 {
                        // 四角號碼模式下，Enter 確認選字後立即將整句送出至目標視窗（`+` 則留在輸入座繼續組句）
                        self.commit_current_composition(ctx)?;
                    } else {
                        self.sync_input_display(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                }

                if vk == VK_ESCAPE.0 as u32 {
                    if let Ok(eng) = self.engine.lock() {
                        eng.set_shift_down_longkan_popup(false);
                        eng.set_shift_down_mode_picker(false);
                        eng.set_active_shift_down_mode_override(None);
                    }
                    set_tab_range_popup_active(self.western_instance_key(), false);
                    *self.sel_anchor.lock().unwrap() = None;
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                // 若在候選窗開啟狀態下按下 Home/End 或帶 Ctrl/Shift 之左/右/上/下方向鍵，立即關閉候選座並允許游標繼續移動！
                if vk == VK_HOME.0 as u32
                    || vk == VK_END.0 as u32
                    || ((vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32) && (is_shift || is_ctrl))
                    || ((vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32) && is_ctrl)
                {
                    if let Ok(eng) = self.engine.lock() {
                        eng.set_shift_down_longkan_popup(false);
                        eng.set_shift_down_mode_picker(false);
                        eng.set_active_shift_down_mode_override(None);
                    }
                    set_tab_range_popup_active(self.western_instance_key(), false);
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                    // 不直接 return，放行給下方第 4 節處理游標移動
                }
            }

            // 2.2 智慧四角號碼模式 (four_corner) 雙區分工架構：
            //     - 右側九宮格小鍵盤 (0x60..=0x6F) 專責四角編碼、`*` 萬用字元、`-` 詞前導、`+` 字詞分隔、`/` 字詞選單、`.` 標點選單。
            //     - 左側主鍵盤區 (A~Z、上方橫排 0~9、主鍵盤符號) 負責正常輸入一般英數字與符號：
            //       * 當無組字緩衝區 (!has_buffer) 時，主鍵盤按鍵直接放行至目標視窗 (Pass-through)，完全無須切換中英模式！
            //       * 當輸入座有組字緩衝區 (has_buffer) 時，主鍵盤按鍵自動將未決四角碼收攏入座並直接追加英數字元，實現中英數無縫混打！
            if is_four_corner_mode
                && !is_cand_sel
                && !is_en_inline_or_special
                && !is_ctrl
                && !is_alt
                && !is_win
            {
                let is_main_kb_printable = (0x41..=0x5A).contains(&vk)
                    || (0x30..=0x39).contains(&vk)
                    || matches!(vk, 0xBA..=0xBF | 0xDB..=0xDF);
                let is_bs_escape_step = (vk == 0xDC && !is_shift)
                    || self
                        .engine
                        .lock()
                        .map(|e| e.is_backslash_escape_pending())
                        .unwrap_or(false);
                if is_main_kb_printable && !is_bs_escape_step {
                    if !has_buffer {
                        return Ok(BOOL::from(false));
                    }
                    let is_fc_mixed = self
                        .engine
                        .lock()
                        .map(|e| e.is_mixed_typing_enabled_for_mode("four_corner"))
                        .unwrap_or(true);
                    if is_fc_mixed {
                        if let Some(ch) = map_vk_to_char(vk, is_shift) {
                            self.delete_selection_if_any();
                            self.consolidate_engine_to_pending();
                            self.insert_pending_char(ch, None);
                            self.sync_input_display(ctx)?;
                            return Ok(BOOL::from(true));
                        }
                    } else {
                        return Ok(BOOL::from(true));
                    }
                }
            }

            // 2.5 空白鍵在無未決注音聲調時的處理 (例如先前輸入了「1.」或已確認詞句)
            // 絕不可將未處理的空白鍵外洩至目標視窗，必須納入組字緩衝區！
            // 日文模式下：
            // - 第 1 下 Space（有未決羅馬字緩衝區時）：作為詞彙間隔，將當前日文詞彙（如 watashiwa ➔ 私は）固定至輸入座長句流，不送出亦不插入空白，方便繼續拼打下一個詞組成完整長句！
            // - 第 2 下 Space（當前詞彙已固定、無未決羅馬字時）：直接在輸入座長句中插入半形空格，無須切換至英文模式！
            // - 整句由 Enter（或句末標點 。/？/！）統一送出上屏！
            if vk == VK_SPACE.0 as u32 && !has_buffer && !is_shift && !is_ctrl && !is_alt {
                let is_fw = self.config.lock().map(|c| c.fullwidth_mode).unwrap_or(false);
                if is_fw {
                    self.update_tsf_composition(ctx, "\u{3000}", 1, None, true)?;
                    return Ok(BOOL::from(true));
                }
            }
            if vk == VK_SPACE.0 as u32 && has_buffer {
                let (is_jp_mode, has_jp_buf, is_kr_mode, has_kr_buf, has_fc_buf, has_lk_buf, has_pending_fsm, is_latin, has_shape_buf, passthrough_to_engine) = {
                    let engine = self.engine.lock().unwrap();
                    let jp_buf = engine.japanese.has_buffer();
                    let kr_buf = engine.korean.has_buffer();
                    let fc_buf = engine.four_corner.has_buffer();
                    let lk_buf = engine.longkan.has_buffer();
                    let passthrough = engine.inline_longkan_active
                        || engine.absorb_next_space_after_inline_lk
                        || engine.is_backslash_escape_pending()
                        || matches!(
                            engine.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        );
                    (
                        matches!(engine.keyboard_mode.as_str(), "japanese" | "jp" | "romaji"),
                        jp_buf,
                        matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja"),
                        kr_buf,
                        fc_buf,
                        lk_buf,
                        engine.fsm.has_pending(),
                        engine.is_latin_mode,
                        engine.cangjie.has_buffer() || engine.array30.has_buffer() || fc_buf || lk_buf || jp_buf || kr_buf,
                        passthrough,
                    )
                };
                if !passthrough_to_engine {
                    if is_four_corner_mode {
                        if has_fc_buf {
                            self.consolidate_engine_to_pending();
                        } else {
                            self.insert_pending_char(' ', None);
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                    if is_longkan_mode {
                        if has_lk_buf {
                            self.consolidate_engine_to_pending();
                        } else {
                            self.insert_pending_char(' ', None);
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                    if is_jp_mode {
                        if has_jp_buf {
                            self.consolidate_engine_to_pending();
                        } else {
                            self.insert_pending_char(' ', None);
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                    if is_kr_mode {
                        if has_kr_buf {
                            let committed_chunk = {
                                let mut engine = self.engine.lock().unwrap();
                                let resp = engine.process_key_str("Space");
                                if !resp.committed_chunk.is_empty() {
                                    engine.record_committed(&resp.committed_chunk);
                                }
                                resp.committed_chunk
                            };
                            if !committed_chunk.is_empty() {
                                let c_chars: Vec<char> = committed_chunk.chars().collect();
                                let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                                let c_segs = if c_chars.len() > 1 && committed_chunk != " " {
                                    vec![(0, c_chars.len(), String::new(), committed_chunk.clone())]
                                } else {
                                    Vec::new()
                                };
                                self.splice_pending_chunk(&c_chars, &c_syls, &c_segs);
                            }
                        } else {
                            self.insert_pending_char(' ', None);
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                    if !has_pending_fsm && !is_latin && !has_shape_buf {
                        // 當已完成全部音節組字（無未決注音部件）時，空白鍵結算確認送出整句！
                        self.commit_current_composition(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                }
            }

            // 3. 【自然輸入法 Shift+大寫字母流】：混打模式放行進入引擎智慧判斷，關閉混打時則直接納入組字流
            let (is_mixed, is_fw_upper, is_special_or_inline) = self
                .engine
                .lock()
                .map(|e| {
                    let special = e.inline_longkan_active
                        || e.is_backslash_escape_pending()
                        || matches!(
                            e.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        );
                    (e.is_mixed_english_active(), e.fullwidth_mode, special)
                })
                .unwrap_or((false, false, false));
            if !is_mixed
                && !is_special_or_inline
                && is_shift
                && !is_ctrl
                && !is_alt
                && !is_win
                && (0x41..=0x5A).contains(&vk)
            {
                let base_upper = (vk as u8 as char).to_ascii_uppercase();
                let upper_char = if is_fw_upper {
                    ruhi::engine::RuhiEngine::to_fullwidth_ascii(base_upper)
                } else {
                    base_upper
                };
                self.consolidate_engine_to_pending();
                self.insert_pending_char(upper_char, None);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.5 Insert 鍵：Shift+Insert 貼上剪貼簿文字；純 Insert 切換插入/覆蓋模式 (INS/OVR)
            if vk == VK_INSERT.0 as u32 && has_buffer {
                if is_shift {
                    if let Some(text) = get_clipboard_text() {
                        if self.paste_text_into_desk(&text) {
                            self.sync_input_display(ctx)?;
                        }
                    }
                    return Ok(BOOL::from(true));
                }
                let mut mode = self.insert_mode.lock().unwrap();
                *mode = !*mode;
                logger::log(&format!("Insert toggle: mode={}", if *mode { "INS" } else { "OVR" }));
                drop(mode);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.6 九宮格小數點 (0x6E / VK_DECIMAL) 永遠保持半形「.」（惟智慧四角號碼模式下 `.` 為萬用字元／選字鍵，交由四角引擎處理）
            if vk == 0x6E && has_buffer && !is_four_corner_mode {
                self.consolidate_engine_to_pending();
                self.insert_pending_char('.', None);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.7 Tab 鍵：
            // - 特殊選單模式或自訂縮略模式：Tab / Shift+Tab 於候選座直接循環選擇
            if vk == VK_TAB.0 as u32 {
                let is_menu_or_abbrev = {
                    let eng = self.engine.lock().unwrap();
                    matches!(
                        eng.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    ) || (eng.inline_longkan_active
                        && matches!(
                            eng.inline_target_mode.as_str(),
                            "user_abbrev" | "abbrev" | "user_dict"
                        ))
                };
                if is_menu_or_abbrev {
                    let key_str = if is_shift { "Shift+Tab" } else { "Tab" };
                    let resp = {
                        let mut eng = self.engine.lock().unwrap();
                        eng.process_key_str(key_str)
                    };
                    if !resp.committed_chunk.is_empty() {
                        self.push_undo();
                        self.consolidate_engine_to_pending();
                        let chars: Vec<char> = resp.committed_chunk.chars().collect();
                        let cur = *self.sentence_cursor.lock().unwrap();
                        self.replace_pending_range(cur, cur, &chars, None);
                        *self.sentence_cursor.lock().unwrap() = cur + chars.len();
                    }
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            // - 龍龕形音模式 (is_longkan_mode)：作為詞彙簡碼模式（`🐉⇥詞:`）的引導與結束間隔符號，將組出的字詞收攏至如喜輸入座（`pending_compose`），待按 Enter 才送出至目標視窗
            // - 許氏注音模式：手動斷詞切換 (Manual Word Boundary Split / Cut)
            if vk == VK_TAB.0 as u32 && (has_buffer || is_longkan_mode) {
                set_tab_range_popup_active(self.western_instance_key(), false);
                if *self.candidate_selecting.lock().unwrap() {
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                }

                let is_latin_mode = self
                    .engine
                    .lock()
                    .map(|e| e.is_latin_mode)
                    .unwrap_or(false);

                // Guard A：歐文模式／臨時拉丁切換下行首／純空白縮排保護，按 Tab 直接插入縮排字元而非劫持跳出候選窗
                if (is_english_mode || is_latin_mode) && !is_shift {
                    let (full_text, cur) = self.get_full_display_and_cursor();
                    let cur = cur.min(full_text.chars().count());
                    let before_cur: String = full_text.chars().take(cur).collect();
                    let cur_line = before_cur.rsplit('\n').next().unwrap_or(&before_cur);
                    if cur_line.chars().all(|c| matches!(c, ' ' | '\t' | '\u{3000}')) {
                        self.consolidate_engine_to_pending();
                        self.insert_pending_char('\t', None);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                }

                let (_is_inline_lk_tab, has_active_raw_sub_buf) = {
                    let eng = self.engine.lock().unwrap();
                    let inline_tab = eng.inline_longkan_active
                        && matches!(
                            eng.inline_target_mode.as_str(),
                            "user_abbrev"
                                | "abbrev"
                                | "user_dict"
                                | "longkan"
                                | "lk"
                                | "four_corner"
                                | "4corner"
                                | "fc"
                                | "japanese"
                                | "jp"
                                | "romaji"
                                | "korean"
                                | "ko"
                                | "hangul"
                                | "romaja"
                                | "hsu"
                                | "standard"
                                | "daikien"
                                | "et26"
                                | "eten26"
                        );
                    let raw_sub = eng.longkan.has_buffer()
                        || (eng.inline_longkan_active
                            && (eng.has_inline_sub_buffer()
                                || (eng.inline_target_mode == "user_abbrev"
                                    && !eng.current_candidates.is_empty())));
                    (inline_tab, raw_sub)
                };

                // Guard B：字形模式（倉頡／行列 30／四角號碼）組字中未上屏碼按 Tab 展開前綴候選窗（必須早於 consolidate_engine_to_pending）
                if !is_shift && !has_active_raw_sub_buf && self.get_manual_selection_range().is_none() {
                    let is_shape_buf = self
                        .engine
                        .lock()
                        .map(|e| {
                            (e.keyboard_mode == "cangjie"
                                && e.cangjie.has_buffer()
                                && !e.cangjie.is_english_forced)
                                || (matches!(e.keyboard_mode.as_str(), "array30" | "ar30")
                                    && e.array30.has_buffer()
                                    && !e.array30.is_english_forced)
                                || (matches!(
                                    e.keyboard_mode.as_str(),
                                    "four_corner" | "4corner" | "fc"
                                ) && e.four_corner.has_buffer())
                        })
                        .unwrap_or(false);
                    if is_shape_buf {
                        let mut shape_cands = {
                            let mut eng = self.engine.lock().unwrap();
                            eng.collect_shape_prefix_tab_candidates(12)
                        };
                        {
                            let del_words = self.deleted_words.lock().unwrap();
                            shape_cands.retain(|c| !del_words.contains(c));
                        }
                        if !shape_cands.is_empty() {
                            let pending_len = self.pending_compose.lock().unwrap().chars().count();
                            let cur = (*self.sentence_cursor.lock().unwrap()).min(pending_len);
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_shift_down_longkan_popup(false);
                                eng.set_shift_down_mode_picker(false);
                            }
                            *self.target_char_idx.lock().unwrap() = cur;
                            *self.target_range.lock().unwrap() = (cur, cur);
                            *self.sentence_cursor.lock().unwrap() = cur;
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = shape_cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                            self.sync_input_display(ctx)?;
                            return Ok(BOOL::from(true));
                        }
                    }
                }

                // 優先檢查：若使用者已透過 Shift + ←/→ 標記了 2~4 個部件漢字（或剛輸入「口3」、「金3」等疊字倍數簡寫，且無正在拼打的未決字根），
                // 無論處於許氏注音、標準大千、龍龕形音或倉頡／行列模式，按 Tab 一律優先執行「事後部件熔煉合體」！
                let has_mult_shorthand_before_cursor = if !has_active_raw_sub_buf && self.get_manual_selection_range().is_none() {
                    self.consolidate_engine_to_pending();
                    let cur = *self.sentence_cursor.lock().unwrap();
                    let pending_str = self.pending_compose.lock().unwrap().clone();
                    let pending_chars: Vec<char> = pending_str.chars().collect();
                    let pending_syls = self.pending_syllables.lock().unwrap().clone();
                    if cur >= 2 && cur <= pending_chars.len() && matches!(pending_chars[cur - 1], '2' | '3' | '4' | '二' | '三' | '四') {
                        let engine = self.engine.lock().unwrap();
                        engine.has_verified_longkan_component_fusion_for_slice(
                            &pending_chars,
                            &pending_syls,
                            cur - 2,
                            cur,
                        )
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !is_shift && !has_active_raw_sub_buf && (self.get_manual_selection_range().is_some() || has_mult_shorthand_before_cursor) {
                    self.push_undo();
                    self.consolidate_engine_to_pending();
                    let manual_sel_range = self.get_manual_selection_range();
                    let has_manual_sel = manual_sel_range.is_some();
                    let eff_sel_range = manual_sel_range.or_else(|| {
                        let cur = *self.sentence_cursor.lock().unwrap();
                        if has_mult_shorthand_before_cursor && cur >= 2 {
                            Some((cur - 2, cur))
                        } else {
                            None
                        }
                    });
                    if let Some((mut start, end)) = eff_sel_range {
                        let (adj_start, is_verified_lk, postal_opt) = {
                            let pending_str = self.pending_compose.lock().unwrap().clone();
                            let pending_chars: Vec<char> = pending_str.chars().collect();
                            let pending_syls = self.pending_syllables.lock().unwrap().clone();
                            let engine = self.engine.lock().unwrap();
                            let mut s = start;
                            if end.saturating_sub(s) == 1
                                && s >= 1
                                && engine.has_verified_longkan_component_fusion_for_slice(
                                    &pending_chars,
                                    &pending_syls,
                                    s - 1,
                                    end,
                                )
                            {
                                s -= 1;
                            }
                            let verified_lk = (2..=4).contains(&end.saturating_sub(s))
                                && engine.has_verified_longkan_component_fusion_for_slice(
                                    &pending_chars,
                                    &pending_syls,
                                    s,
                                    end,
                                );
                            let postal = if !verified_lk && has_manual_sel {
                                engine.resolve_postal_code_for_marked_slice(&pending_chars, s, end)
                            } else {
                                None
                            };
                            (s, verified_lk, postal)
                        };
                        start = adj_start;
                        if !is_verified_lk {
                            if let Some(postal_str) = postal_opt {
                                let rep_chars: Vec<char> = postal_str.chars().collect();
                                self.replace_pending_range(start, start, &rep_chars, None);
                                *self.sel_anchor.lock().unwrap() = None;
                                *self.sentence_cursor.lock().unwrap() = end + rep_chars.len();
                                self.sync_input_display(ctx)?;
                                return Ok(BOOL::from(true));
                            }
                            if has_manual_sel && start < end {
                                let marked_str: String = {
                                    let p = self.pending_compose.lock().unwrap();
                                    let pcs: Vec<char> = p.chars().collect();
                                    pcs.get(start..end).map(|s| s.iter().collect()).unwrap_or_default()
                                };
                                let sym_target = {
                                    self.engine.lock().unwrap().lookup_symbol_tab_target(&marked_str)
                                };
                                match sym_target {
                                    Some(ruhi::symbol_tree::SymbolTabTarget::OpenTree { major_idx, sub_idx }) => {
                                        self.replace_pending_range(start, end, &[], None);
                                        *self.sel_anchor.lock().unwrap() = None;
                                        *self.sentence_cursor.lock().unwrap() = start;
                                        *self.candidate_selecting.lock().unwrap() = false;
                                        {
                                            let mut eng = self.engine.lock().unwrap();
                                            eng.open_symbol_tree_at(major_idx, sub_idx, false);
                                        }
                                        self.sync_input_display(ctx)?;
                                        return Ok(BOOL::from(true));
                                    }
                                    Some(ruhi::symbol_tree::SymbolTabTarget::Candidates(sym_cands)) if !sym_cands.is_empty() => {
                                        if let Ok(eng) = self.engine.lock() {
                                            eng.set_shift_down_longkan_popup(false);
                                            eng.set_shift_down_mode_picker(false);
                                        }
                                        set_tab_range_popup_active(self.western_instance_key(), true);
                                        *self.target_char_idx.lock().unwrap() = start;
                                        *self.target_range.lock().unwrap() = (start, end);
                                        *self.sel_anchor.lock().unwrap() = None;
                                        *self.sentence_cursor.lock().unwrap() = start;
                                        *self.selected_candidate_idx.lock().unwrap() = 0;
                                        *self.active_candidates.lock().unwrap() = sym_cands;
                                        *self.candidate_selecting.lock().unwrap() = true;
                                        self.sync_input_display(ctx)?;
                                        return Ok(BOOL::from(true));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if end.saturating_sub(start) >= 2 && end.saturating_sub(start) <= 4 {
                            let (fused_cands, fused_bpm) = {
                                let pending_str = self.pending_compose.lock().unwrap().clone();
                                let pending_chars: Vec<char> = pending_str.chars().collect();
                                let pending_syls = self.pending_syllables.lock().unwrap().clone();
                                let engine = self.engine.lock().unwrap();
                                let hsu_syls = engine.slice_hsu_syllables(
                                    &pending_chars,
                                    &pending_syls,
                                    start,
                                    end,
                                );
                                let cands = if (2..=4).contains(&hsu_syls.len()) {
                                    engine.fuse_syllables_with_longkan(&hsu_syls)
                                } else {
                                    Vec::new()
                                };
                                let first_bpm = cands.first().and_then(|fc| {
                                    engine
                                        .lexicon
                                        .word_index
                                        .get(fc)
                                        .and_then(|ids| {
                                            ids.iter()
                                                .filter_map(|&eid| engine.lexicon.entries.get(eid))
                                                .max_by(|a, b| {
                                                    a.weight
                                                        .partial_cmp(&b.weight)
                                                        .unwrap_or(std::cmp::Ordering::Equal)
                                                })
                                        })
                                        .map(|e| e.bopomofo.clone())
                                });
                                (cands, first_bpm)
                            };
                            if let Some(first_cand) = fused_cands.first() {
                                let rep_chars: Vec<char> = first_cand.chars().collect();
                                self.replace_pending_range(start, end, &rep_chars, fused_bpm);
                                if let Ok(eng) = self.engine.lock() {
                                    eng.set_last_fused_longkan(Some((start, fused_cands.clone())));
                                }
                                *self.sel_anchor.lock().unwrap() = None;
                                *self.sentence_cursor.lock().unwrap() = start + rep_chars.len();
                                self.sync_input_display(ctx)?;
                                return Ok(BOOL::from(true));
                            }
                        }
                    }
                }

                if !is_shift && (has_active_raw_sub_buf || (is_longkan_mode && !has_buffer)) {
                    let (action, committed_chunk, lk_buf_before, still_has_any) = {
                        let mut engine = self.engine.lock().unwrap();
                        let before = if engine.inline_target_mode == "user_abbrev" {
                            engine.inline_abbrev_buffer.clone()
                        } else {
                            engine.longkan.buffer.clone()
                        };
                        let resp = engine.process_key('\t');
                        let has_any = engine.longkan.has_buffer()
                            || !engine.composed_sentence.is_empty()
                            || !self.pending_compose.lock().map(|p| p.is_empty()).unwrap_or(true);
                        (resp.action, resp.committed_chunk, before, has_any)
                    };
                    self.consolidate_engine_to_pending();
                    if action.starts_with("commit") && !committed_chunk.is_empty() {
                        let cur_before = *self.sentence_cursor.lock().unwrap();
                        let c_chars: Vec<char> = committed_chunk.chars().collect();
                        let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                        let c_segs = if c_chars.len() > 1 && committed_chunk != " " {
                            vec![(0, c_chars.len(), committed_chunk.clone(), lk_buf_before.clone())]
                        } else {
                            Vec::new()
                        };
                        self.splice_pending_chunk(&c_chars, &c_syls, &c_segs);
                        if !lk_buf_before.is_empty() && !c_chars.is_empty() {
                            *self.last_raw_ascii.lock().unwrap() =
                                Some((cur_before, cur_before + c_chars.len(), lk_buf_before));
                        }
                    }
                    if still_has_any || !self.pending_compose.lock().map(|p| p.is_empty()).unwrap_or(true) {
                        self.sync_input_display(ctx)?;
                    } else {
                        self.hide_candidate_window();
                        self.end_tsf_composition(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                }

                let tab_mode = self
                    .config
                    .lock()
                    .map(|c| c.tab_key_mode.clone())
                    .unwrap_or_else(|_| "associative".to_string());
                let should_trigger_associative = if is_english_mode || is_latin_mode {
                    !is_shift
                } else if tab_mode == "boundary_cut" {
                    is_shift
                } else {
                    !is_shift
                };

                if should_trigger_associative {
                    self.consolidate_engine_to_pending();
                    let cur = *self.sentence_cursor.lock().unwrap();
                    let (full_text, _) = self.get_full_display_and_cursor();
                    let chars: Vec<char> = full_text.chars().collect();

                    let mut handled_geo = false;
                    if let Some((replace_range, mut geo_cands)) =
                        ruhi::router::geo_hierarchy::CjkGeoHierarchy::global()
                            .resolve_address_associative_action(&chars, cur)
                    {
                        {
                            let del_words = self.deleted_words.lock().unwrap();
                            geo_cands.retain(|c| !del_words.contains(c));
                        }
                        if !geo_cands.is_empty() {
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_shift_down_longkan_popup(false);
                                eng.set_shift_down_mode_picker(false);
                            }
                            *self.target_char_idx.lock().unwrap() = replace_range.0;
                            *self.target_range.lock().unwrap() = replace_range;
                            *self.sentence_cursor.lock().unwrap() = cur;
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = geo_cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                            handled_geo = true;
                        }
                    }

                    if !handled_geo {
                        let caret_after_space = cur > 0
                            && cur <= chars.len()
                            && matches!(chars[cur - 1], ' ' | '\t' | '\u{3000}');
                        let caret_after_open_paren =
                            cur > 0 && cur <= chars.len() && chars[cur - 1] == '(';
                        let mut handled_eu_assoc = false;
                        if caret_after_space || caret_after_open_paren {
                            let prefix: String = chars[..cur].iter().collect();
                            let mut eu_assoc = {
                                let engine = self.engine.lock().unwrap();
                                engine.western.lookup_associative_continuations(&prefix, 18)
                            };
                            {
                                let del_words = self.deleted_words.lock().unwrap();
                                eu_assoc.retain(|c| !del_words.contains(c));
                            }
                            if !eu_assoc.is_empty() {
                                if let Ok(eng) = self.engine.lock() {
                                    eng.set_shift_down_longkan_popup(false);
                                    eng.set_shift_down_mode_picker(false);
                                }
                                *self.target_char_idx.lock().unwrap() = cur;
                                *self.target_range.lock().unwrap() = (cur, cur);
                                *self.sentence_cursor.lock().unwrap() = cur;
                                *self.selected_candidate_idx.lock().unwrap() = 0;
                                *self.active_candidates.lock().unwrap() = eu_assoc;
                                *self.candidate_selecting.lock().unwrap() = true;
                                handled_eu_assoc = true;
                            }
                        }
                        if !handled_eu_assoc && !caret_after_space {
                            let prev_is_latin = cur > 0
                                && chars.get(cur - 1).map_or(false, |&c| {
                                    matches!(
                                        c,
                                        'A'..='Z'
                                            | 'a'..='z'
                                            | '\u{00C0}'..='\u{024F}'
                                            | '\u{1E00}'..='\u{1EFF}'
                                    ) || ruhi::engine::RuhiEngine::is_western_non_ascii_letter_or_mark(c)
                                });
                            let mut assoc_cands = if is_english_mode || is_latin_mode || prev_is_latin {
                                Vec::new()
                            } else {
                                let engine = self.engine.lock().unwrap();
                                engine
                                    .lexicon
                                    .lookup_associative_continuations(&chars, cur, &engine.transitions)
                            };
                            {
                                let del_words = self.deleted_words.lock().unwrap();
                                assoc_cands.retain(|c| !del_words.contains(c));
                            }
                            if !assoc_cands.is_empty() {
                                if let Ok(eng) = self.engine.lock() {
                                    eng.set_shift_down_longkan_popup(false);
                                    eng.set_shift_down_mode_picker(false);
                                }
                                *self.target_char_idx.lock().unwrap() = cur;
                                *self.target_range.lock().unwrap() = (cur, cur);
                                *self.sentence_cursor.lock().unwrap() = cur;
                                *self.selected_candidate_idx.lock().unwrap() = 0;
                                *self.active_candidates.lock().unwrap() = assoc_cands;
                                *self.candidate_selecting.lock().unwrap() = true;
                            } else {
                                let (target_idx, target_range, up_cands) =
                                    self.detect_up_arrow_target_and_cands(&full_text, cur);
                                if !up_cands.is_empty() {
                                    if let Ok(eng) = self.engine.lock() {
                                        eng.set_shift_down_longkan_popup(false);
                                        eng.set_shift_down_mode_picker(false);
                                    }
                                    *self.target_char_idx.lock().unwrap() = target_idx;
                                    *self.target_range.lock().unwrap() = target_range;
                                    *self.sentence_cursor.lock().unwrap() = target_idx;
                                    *self.selected_candidate_idx.lock().unwrap() = 0;
                                    *self.active_candidates.lock().unwrap() = up_cands;
                                    *self.candidate_selecting.lock().unwrap() = true;
                                }
                            }
                        }
                    }
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                self.push_undo();
                self.consolidate_engine_to_pending();
                let cur = *self.sentence_cursor.lock().unwrap();
                let (full_text, _) = self.get_full_display_and_cursor();
                let total_chars = full_text.chars().count();

                if cur > 0 && cur < total_chars {
                    self.toggle_manual_cut_at_cursor(ctx, cur)?;
                    return Ok(BOOL::from(true));
                } else {
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            // 3.8 Q 鍵快速候選字輪換 (Q-Key Quick Candidate Cycle)
            // 游標所在處單字／雙字同音詞快速切換：若是排序1直接跳排序2，最多循環 q_cycle_max 次後自動彈出候選座
            // （惟許氏注音下若正在輸入聲母 ㄓ(j) / ㄔ(v) / ㄕ(c)，按 q 為轉化為 ㄐ / ㄑ / ㄒ 之合法雙拼鍵，必須放行至引擎！）
            let kb_mode = self.config.lock().map(|c| c.keyboard_mode.clone()).unwrap_or_else(|_| "hsu".to_string());
            let (is_inline_lk_or_special_mode, is_hsu_jqx_transform) = {
                let engine = self.engine.lock().unwrap();
                let special = engine.inline_longkan_active
                    || matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    );
                let jqx = engine.fsm.has_pending()
                    && matches!(engine.fsm.cur_initial.as_str(), "ㄓ" | "ㄔ" | "ㄕ")
                    && engine.fsm.cur_medial.is_empty()
                    && engine.fsm.cur_final.is_empty();
                (special, jqx)
            };
            if vk == 0x51
                && !is_shift
                && !is_ctrl
                && !is_alt
                && has_buffer
                && kb_mode == "hsu"
                && !is_inline_lk_or_special_mode
                && !is_hsu_jqx_transform
            {
                self.consolidate_engine_to_pending();
                let (full_text, _) = self.get_full_display_and_cursor();
                let chars: Vec<char> = full_text.chars().collect();
                if !chars.is_empty() {
                    let cur = *self.sentence_cursor.lock().unwrap();
                    let target_idx = if cur >= chars.len() {
                        chars.len().saturating_sub(1)
                    } else if cur > 0 {
                        cur - 1
                    } else {
                        0
                    };

                    let q_max = self.config.lock().map(|c| c.q_cycle_max).unwrap_or(3);
                    let mut q_count = self.q_press_count.lock().unwrap();
                    let mut last_idx = self.q_last_char_idx.lock().unwrap();

                    if *last_idx == Some(target_idx) {
                        *q_count += 1;
                    } else {
                        *q_count = 1;
                        *last_idx = Some(target_idx);
                    }

                    // 檢查 target_idx 是否落在多字詞區間（如「證明」⇄「正名」）：若有同音多字詞，優先輪換多字詞！
                    let matched_seg: Option<(usize, usize, String, String)> = {
                        let segs = self.pending_segments.lock().unwrap();
                        segs.iter()
                            .find(|s| target_idx >= s.0 && target_idx < s.1 && (s.1 - s.0) >= 2)
                            .cloned()
                    };

                    let target_char = chars[target_idx];
                    let char_syl = self.pending_syllables.lock().unwrap().get(target_idx).cloned().flatten();
                    let (word_cands_opt, cands) = {
                        let engine = self.engine.lock().unwrap();
                        let w_cands = matched_seg.as_ref().and_then(|(s, e, w, bpm)| {
                            let mut wc = engine.lexicon.lookup_word_homophones_with_bpm(w, Some(bpm));
                            let del_words = self.deleted_words.lock().unwrap();
                            wc.retain(|c| !del_words.contains(c));
                            if wc.len() >= 2 {
                                Some((*s, *e, bpm.clone(), wc))
                            } else {
                                None
                            }
                        });
                        let mut ch_cands = engine.lexicon.lookup_char_candidates_with_bpm(target_char, char_syl.as_deref());
                        if ch_cands.is_empty() && (target_char.is_ascii_alphabetic() || ruhi::western::WesternEngine::strip_char_diacritic(target_char).is_ascii_alphabetic()) {
                            ch_cands = engine.western.lookup_down_arrow_candidates(&target_char.to_string(), Some(target_char));
                        }
                        (w_cands, ch_cands)
                    };

                    if *q_count >= q_max {
                        // 超過上限次數：重設計數並自動開啟選字座 (等同按下 ↓，支援同音多字詞與單字清單)
                        *q_count = 0;
                        *last_idx = None;
                        drop(q_count);
                        drop(last_idx);

                        let (t_idx, t_range, full_cands) = self.detect_target_word_and_cands(&full_text, cur);
                        *self.target_char_idx.lock().unwrap() = t_idx;
                        *self.target_range.lock().unwrap() = t_range;
                        *self.sentence_cursor.lock().unwrap() = t_idx;

                        if !full_cands.is_empty() {
                            set_tab_range_popup_active(self.western_instance_key(), false);
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = full_cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    } else if let Some((seg_start, seg_end, seg_bpm, w_cands)) = word_cands_opt {
                        let cur_seg_str: String = chars[seg_start..seg_end.min(chars.len())].iter().collect();
                        let cur_rank = w_cands.iter().position(|c| c == &cur_seg_str);
                        let next_idx = match cur_rank {
                            Some(r) => (r + 1) % w_cands.len(),
                            None => 1 % w_cands.len(),
                        };
                        let next_word = &w_cands[next_idx];
                        let next_chars: Vec<char> = next_word.chars().collect();
                        self.replace_pending_range(seg_start, seg_end, &next_chars, Some(seg_bpm.clone()));
                        if let Ok(mut segs) = self.pending_segments.lock() {
                            if let Some(seg) = segs.iter_mut().find(|s| s.0 == seg_start && s.1 == seg_end) {
                                seg.2 = next_word.clone();
                            }
                        }
                        drop(q_count);
                        drop(last_idx);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    } else if !cands.is_empty() {
                        let cur_str = target_char.to_string();
                        let cur_rank = cands.iter().position(|c| c == &cur_str);
                        let next_idx = match cur_rank {
                            Some(r) => (r + 1) % cands.len(),
                            None => 1 % cands.len(),
                        };
                        if let Some(ch) = cands[next_idx].chars().next() {
                            self.replace_pending_range(target_idx, target_idx + 1, &[ch], char_syl);
                        }
                        drop(q_count);
                        drop(last_idx);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                }
            } else if vk != 0x51 {
                *self.q_press_count.lock().unwrap() = 0;
                *self.q_last_char_idx.lock().unwrap() = None;
            }

            // 3.9 智慧四角號碼／龍龕形音 2.0（含通用內嵌 ` 輔助輸入與 `` `` `` / `~~` 選單）即時拆碼預覽翻頁與上下左右選字 (PgDn / PgUp / ↓ / ↑ / ← / →)
            let is_arrow_or_page = vk == VK_NEXT.0 as u32
                || vk == VK_PRIOR.0 as u32
                || vk == VK_DOWN.0 as u32
                || vk == VK_UP.0 as u32
                || (!is_ctrl && !is_shift && (vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32));
            if is_arrow_or_page {
                let handled_live_nav = {
                    let mut engine = self.engine.lock().unwrap();
                    let is_special_menu = matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    );
                    if is_special_menu
                        || (engine.inline_longkan_active
                            && (engine.has_inline_sub_buffer()
                                || !engine.current_candidates.is_empty()
                                || matches!(
                                    engine.inline_target_mode.as_str(),
                                    "user_abbrev" | "abbrev" | "user_dict"
                                )))
                    {
                        let key_str = match vk {
                            v if v == VK_NEXT.0 as u32 => "PageDown",
                            v if v == VK_PRIOR.0 as u32 => "PageUp",
                            v if v == VK_DOWN.0 as u32 => "Down",
                            v if v == VK_UP.0 as u32 => "Up",
                            v if v == VK_LEFT.0 as u32 => "Left",
                            _ => "Right",
                        };
                        let _ = engine.process_key_str(key_str);
                        true
                    } else if (vk == VK_NEXT.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32)
                        && is_four_corner_mode && engine.four_corner.has_buffer()
                    {
                        let key_str = match vk {
                            v if v == VK_NEXT.0 as u32 => "PageDown",
                            v if v == VK_PRIOR.0 as u32 => "PageUp",
                            v if v == VK_DOWN.0 as u32 => "Down",
                            _ => "Up",
                        };
                        let _ = engine.process_key_str(key_str);
                        true
                    } else if (vk == VK_NEXT.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32)
                        && is_longkan_mode
                        && engine.longkan.has_buffer()
                        && (vk != VK_UP.0 as u32 || engine.longkan.candidate_idx > 0)
                    {
                        let key_str = match vk {
                            v if v == VK_NEXT.0 as u32 => "PageDown",
                            v if v == VK_PRIOR.0 as u32 => "PageUp",
                            v if v == VK_DOWN.0 as u32 => "Down",
                            _ => "Up",
                        };
                        let _ = engine.process_key_str(key_str);
                        true
                    } else {
                        false
                    }
                };
                if handled_live_nav {
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            // 3.9B 龍龕形音模式下，當緩衝區尚有未決拆碼（例如忘記先按 Tab 而已輸入 `cerci`）時，
            //      按 Home / Left / Right / End 可在當前詞界內移動 `buffer_cursor` 以便補按 Tab 切換為詞彙簡碼模式
            if is_longkan_mode
                && !is_shift
                && (vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32)
            {
                let handled_lk_cursor = {
                    let mut engine = self.engine.lock().unwrap();
                    let is_raw_unmatched = engine
                        .longkan
                        .candidates
                        .first()
                        .map(|c| c == &engine.longkan.buffer)
                        .unwrap_or(false);
                    if engine.longkan.has_buffer() && is_raw_unmatched {
                        let key_str = match vk {
                            v if v == VK_HOME.0 as u32 => Some("Home"),
                            v if v == VK_END.0 as u32 => Some("End"),
                            v if v == VK_LEFT.0 as u32 && engine.longkan.buffer_cursor > 0 => Some("Left"),
                            v if v == VK_RIGHT.0 as u32 && engine.longkan.buffer_cursor < engine.longkan.buffer.len() => Some("Right"),
                            _ => None,
                        };
                        if let Some(ks) = key_str {
                            let _ = engine.process_key_str(ks);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if handled_lk_cursor {
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            // 四角全盲打閉環模式下，當四角碼緩衝區已空（剛按過 `+` 收攏字詞至輸入座）而輸入座尚有句子時：
            // - 按小鍵盤 `/` (0x6F 字詞選單) 等同按 `↓` 開啟輸入座游標處字詞之候選視窗
            // - 小鍵盤 `-` (0x6D) 保留作為下一個詞彙之「詞前導符號 (-)」，`*` (0x6A) 保留作為萬用字元
            let fc_slash_as_down = if is_four_corner_mode && has_buffer {
                let engine = self.engine.lock().unwrap();
                let is_blind = engine.four_corner.keymap_profile != "direct_punct";
                let no_fc_buf = !engine.four_corner.has_buffer();
                is_blind && no_fc_buf && vk == 0x6F
            } else {
                false
            };

            // 4. 方向鍵與全句游標穿透（徹底消除「結界」，游標自由穿梭標點與前後文字）
            if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32 || fc_slash_as_down {
                self.consolidate_engine_to_pending();
                let (full_text, _) = self.get_full_display_and_cursor();
                let total_chars = full_text.chars().count();

                if total_chars > 0 {
                    // ── Shift+方向鍵選取 (移植自沙盒，含 Ctrl+Shift+上/下 標點區間選取) ──
                    if is_shift
                        && (vk == VK_LEFT.0 as u32
                            || vk == VK_RIGHT.0 as u32
                            || vk == VK_HOME.0 as u32
                            || vk == VK_END.0 as u32
                            || (is_ctrl && (vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32)))
                    {
                        // 設立選取錨點
                        {
                            let mut anchor = self.sel_anchor.lock().unwrap();
                            if anchor.is_none() {
                                let cur = *self.sentence_cursor.lock().unwrap();
                                *anchor = Some(cur);
                            }
                        }
                        // 移動游標（擴展選取）
                        {
                            let chars_vec: Vec<char> = full_text.chars().collect();
                            let is_comb = |c: char| ('\u{0300}'..='\u{036F}').contains(&c);
                            let mut cur = self.sentence_cursor.lock().unwrap();
                            match vk {
                                v if v == VK_LEFT.0 as u32 => {
                                    if is_ctrl {
                                        let boundaries = self.get_word_boundaries(&full_text);
                                        *cur = Self::find_prev_word_boundary(&boundaries, &full_text, *cur);
                                    } else if *cur > 0 {
                                        *cur -= 1;
                                        while *cur > 0 && *cur < chars_vec.len() && is_comb(chars_vec[*cur]) {
                                            *cur -= 1;
                                        }
                                    }
                                }
                                v if v == VK_RIGHT.0 as u32 => {
                                    if is_ctrl {
                                        let boundaries = self.get_word_boundaries(&full_text);
                                        *cur = Self::find_next_word_boundary(&boundaries, &full_text, *cur, total_chars);
                                    } else if *cur < total_chars {
                                        *cur += 1;
                                        while *cur < total_chars && is_comb(chars_vec[*cur]) {
                                            *cur += 1;
                                        }
                                    }
                                }
                                v if v == VK_UP.0 as u32 && is_ctrl => {
                                    *cur = Self::find_prev_punctuation_boundary(&full_text, *cur);
                                }
                                v if v == VK_DOWN.0 as u32 && is_ctrl => {
                                    *cur = Self::find_next_punctuation_boundary(&full_text, *cur, total_chars);
                                }
                                v if v == VK_HOME.0 as u32 => { *cur = 0; }
                                v if v == VK_END.0 as u32 => { *cur = total_chars; }
                                _ => {}
                            }
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 非 Shift 方向鍵：先記錄是否已有 Shift+左右 標記區間（供 ↓ 展開戶籍地址候選），再清除選取
                    let manual_sel_before_clear = self.get_manual_selection_range();
                    if !is_shift {
                        *self.sel_anchor.lock().unwrap() = None;
                    }

                    // 4.1 向左方向鍵 ← (自由穿透標點，移動至前文，支援 Ctrl+Left 詞間快退與組合變音字素跨越)
                    if vk == VK_LEFT.0 as u32 {
                        let chars_vec: Vec<char> = full_text.chars().collect();
                        let is_comb = |c: char| ('\u{0300}'..='\u{036F}').contains(&c);
                        let mut cur = self.sentence_cursor.lock().unwrap();
                        if is_ctrl {
                            let boundaries = self.get_word_boundaries(&full_text);
                            *cur = Self::find_prev_word_boundary(&boundaries, &full_text, *cur);
                        } else {
                            let mut c = (*cur).min(total_chars);
                            if c > 0 {
                                c -= 1;
                                while c > 0 && c < chars_vec.len() && is_comb(chars_vec[c]) {
                                    c -= 1;
                                }
                                *cur = c;
                            } else {
                                *cur = 0;
                            }
                        }
                        drop(cur);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.2 向右方向鍵 → (自由穿透標點，支援 Ctrl+Right 詞間快進與組合變音字素跨越)
                    if vk == VK_RIGHT.0 as u32 {
                        let chars_vec: Vec<char> = full_text.chars().collect();
                        let is_comb = |c: char| ('\u{0300}'..='\u{036F}').contains(&c);
                        let mut cur = self.sentence_cursor.lock().unwrap();
                        if is_ctrl {
                            let boundaries = self.get_word_boundaries(&full_text);
                            *cur = Self::find_next_word_boundary(&boundaries, &full_text, *cur, total_chars);
                        } else {
                            let mut c = (*cur).min(total_chars);
                            if c < total_chars {
                                c += 1;
                                while c < total_chars && is_comb(chars_vec[c]) {
                                    c += 1;
                                }
                                *cur = c;
                            } else {
                                *cur = total_chars;
                            }
                        }
                        drop(cur);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.3 Home 鍵
                    if vk == VK_HOME.0 as u32 {
                        *self.sentence_cursor.lock().unwrap() = 0;
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.4 End 鍵
                    if vk == VK_END.0 as u32 {
                        *self.sentence_cursor.lock().unwrap() = total_chars;
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.4B Ctrl + 上/下方向鍵：在前後標點符號之間快速移動游標
                    if is_ctrl && (vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32) {
                        let mut cur = self.sentence_cursor.lock().unwrap();
                        if vk == VK_UP.0 as u32 {
                            *cur = Self::find_prev_punctuation_boundary(&full_text, *cur);
                        } else {
                            *cur = Self::find_next_punctuation_boundary(&full_text, *cur, total_chars);
                        }
                        drop(cur);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.5 向下方向鍵 ↓（或四角模式已收攏字詞後按 `/` 字詞選單）：
                    // - 若按 `Shift + ↓`：於任意輸入模式、任意字詞或標記區間喚起「龍龕形音」候選編碼字詞視窗
                    // - 若單按 `↓`：精準開啟游標所在目標詞/字的同音／同碼候選座（含歐文重音變體與同形異體查詢）
                    if vk == VK_DOWN.0 as u32 || fc_slash_as_down {
                        let cur = *self.sentence_cursor.lock().unwrap();
                        let (full_text, _) = self.get_full_display_and_cursor();
                        let use_shift_down_lk = vk == VK_DOWN.0 as u32 && is_shift;
                        let (target_idx, target_range, cands) = if use_shift_down_lk {
                            let (ti, tr, lk_cands) =
                                self.detect_shift_down_longkan_target_and_cands(&full_text, cur);
                            if !lk_cands.is_empty() {
                                if let Ok(eng) = self.engine.lock() {
                                    eng.set_shift_down_longkan_popup(true);
                                }
                                (ti, tr, lk_cands)
                            } else {
                                if let Ok(eng) = self.engine.lock() {
                                    eng.set_shift_down_longkan_popup(false);
                                }
                                self.detect_target_word_and_cands(&full_text, cur)
                            }
                        } else {
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_shift_down_longkan_popup(false);
                            }
                            let mut addr_expanded: Option<(usize, (usize, usize), Vec<String>)> = None;
                            if let Some((sel_s, sel_e)) = manual_sel_before_clear {
                                if sel_e > sel_s {
                                    let chars_vec: Vec<char> = full_text.chars().collect();
                                    if let Ok(eng) = self.engine.lock() {
                                        let addr_cands = eng.expand_household_address_for_marked_slice(
                                            &chars_vec,
                                            sel_s,
                                            sel_e,
                                        );
                                        if !addr_cands.is_empty() {
                                            addr_expanded = Some((sel_s, (sel_s, sel_e), addr_cands));
                                        }
                                    }
                                }
                            }
                            if let Some(res) = addr_expanded {
                                res
                            } else {
                                self.detect_target_word_and_cands(&full_text, cur)
                            }
                        };

                        *self.target_char_idx.lock().unwrap() = target_idx;
                        *self.target_range.lock().unwrap() = target_range;
                        *self.sentence_cursor.lock().unwrap() = target_idx;

                        if !cands.is_empty() {
                            set_tab_range_popup_active(self.western_instance_key(), false);
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.6 向上方向鍵 ↑：雙軌智慧拼寫救援與上下文模糊推理選字
                    // - 歐文軌：縮寫展開（tks ➔ thanks）、漏撇號還原（dont ➔ don't）、子音骨架與拼寫校正（cmptr ➔ computer）、跨軌救援
                    // - 亞洲軌：跨聲調＋近音模糊音＋前後文語法搭配推論（答按 ➔ 答案、可能地答案 ➔ 可能的答案）
                    if vk == VK_UP.0 as u32 {
                        let cur = *self.sentence_cursor.lock().unwrap();
                        let (full_text, _) = self.get_full_display_and_cursor();
                        let (target_idx, target_range, cands) = self.detect_up_arrow_target_and_cands(&full_text, cur);

                        if !cands.is_empty() {
                            set_tab_range_popup_active(self.western_instance_key(), false);
                            *self.target_char_idx.lock().unwrap() = target_idx;
                            *self.target_range.lock().unwrap() = target_range;
                            *self.sentence_cursor.lock().unwrap() = target_idx;
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                }
                return Ok(BOOL::from(true));
            }

            // 5. Enter / Shift+Enter 鍵：自建詞加詞 / 刪詞抑制，或全句結算送出
            if vk == VK_RETURN.0 as u32 {
                if let Some((start, end)) = self.get_manual_selection_range() {
                    if is_shift {
                        return self.handle_suppress_or_delete_word(ctx, start, end);
                    } else {
                        return self.handle_add_user_word(ctx, start, end);
                    }
                }
                let ar30_passthrough = {
                    let mut engine = self.engine.lock().unwrap();
                    if engine.mode == ruhi::engine::EngineMode::LexiconSwitch {
                        let _ = engine.process_key('\n');
                        Some(("switch_lexicon".to_string(), String::new()))
                    } else if engine.cangjie.has_buffer()
                        || engine.array30.has_buffer()
                        || engine.four_corner.has_buffer()
                        || engine.longkan.has_buffer()
                        || engine.inline_longkan_active
                        || matches!(
                            engine.mode,
                            ruhi::engine::EngineMode::QuickNum | ruhi::engine::EngineMode::TemplateSelect
                        )
                    {
                        let resp = engine.process_key('\n');
                        Some((resp.action, resp.committed_chunk))
                    } else {
                        None
                    }
                };
                if let Some((act, chunk)) = ar30_passthrough {
                    if act == "switch_lexicon" {
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                    if !chunk.is_empty() {
                        let c_chars: Vec<char> = chunk.chars().collect();
                        let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                        self.splice_pending_chunk(&c_chars, &c_syls, &[]);
                    }
                }
                if is_shift {
                    // Shift+Enter（無選取範圍時）：在多行輸入座內強制換行（插入 '\n'），繼續留在輸入座組字
                    self.push_undo();
                    self.consolidate_engine_to_pending();
                    self.insert_pending_char('\n', None);
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
                self.commit_current_composition(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 6.1 Backspace 退格鍵 (全句自由回退刪除 + 選取範圍刪除)
            if vk == VK_BACK.0 as u32 {
                // 選取範圍刪除優先（預存撤銷快照以便 Ctrl+Z 完整還原）
                if self.get_manual_selection_range().is_some() {
                    self.push_undo();
                }
                if self.delete_selection_if_any() {
                    let (full, _) = self.get_full_display_and_cursor();
                    if !full.is_empty() {
                        self.sync_input_display(ctx)?;
                    } else {
                        self.hide_candidate_window();
                        self.end_tsf_composition(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                }

                // Tier-1 歐文空白鍵自動勘誤單鍵 Backspace 還原（並釘選原始單字以防重複觸發）
                if !is_ctrl && !is_shift && !is_alt {
                    if let Some(rb) = self.take_last_western_autocorrect_if_matching() {
                        self.restore_western_space_rollback(rb);
                        let (full, _) = self.get_full_display_and_cursor();
                        if !full.is_empty() {
                            self.sync_input_display(ctx)?;
                        } else {
                            self.hide_candidate_window();
                            self.end_tsf_composition(ctx)?;
                        }
                        return Ok(BOOL::from(true));
                    }
                }

                let eng_had_uncommitted = {
                    let mut engine = self.engine.lock().unwrap();
                    if engine.inline_longkan_active
                        || matches!(
                            engine.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        )
                    {
                        let _ = engine.process_key('\x08');
                        true
                    } else if !engine.is_english_keyboard_mode()
                        && !engine.is_latin_mode
                        && engine.is_backslash_escape_pending()
                    {
                        engine.set_backslash_escape_pending(false);
                        true
                    } else if engine.japanese.has_buffer() {
                        if is_ctrl {
                            let _ = engine.process_key_str("Ctrl+BackSpace");
                        } else if is_shift {
                            let _ = engine.process_key_str("Shift+BackSpace");
                        } else {
                            let _ = engine.process_key('\x08');
                        }
                        true
                    } else if engine.korean.has_buffer() {
                        if is_ctrl {
                            let _ = engine.process_key_str("Ctrl+BackSpace");
                        } else if is_shift {
                            let _ = engine.process_key_str("Shift+BackSpace");
                        } else {
                            let _ = engine.process_key('\x08');
                        }
                        true
                    } else if engine.cangjie.has_buffer() {
                        if is_ctrl || is_shift {
                            let _ = engine.cangjie.process_key_str("Shift+BackSpace");
                            engine.composed_sentence.clear();
                            engine.cursor_pos = 0;
                        } else {
                            let _ = engine.process_key('\x08');
                        }
                        true
                    } else if engine.array30.has_buffer() {
                        if is_ctrl || is_shift {
                            let _ = engine.array30.process_key_str("Shift+BackSpace");
                            engine.composed_sentence.clear();
                            engine.cursor_pos = 0;
                        } else {
                            let _ = engine.process_key('\x08');
                        }
                        true
                    } else if engine.four_corner.has_buffer() {
                        if is_ctrl || is_shift {
                            let _ = engine.four_corner.process_key_str("Shift+BackSpace");
                            engine.composed_sentence.clear();
                            engine.cursor_pos = 0;
                        } else {
                            let _ = engine.process_key('\x08');
                        }
                        true
                    } else if engine.longkan.has_buffer()
                        || engine.inline_longkan_active
                        || matches!(
                            engine.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        )
                    {
                        if (is_ctrl || is_shift) && engine.longkan.has_buffer() && !engine.inline_longkan_active {
                            let _ = engine.longkan.process_key_str("Shift+BackSpace");
                            engine.composed_sentence.clear();
                            engine.cursor_pos = 0;
                        } else {
                            let _ = engine.process_key('\x08');
                        }
                        true
                    } else if engine.fsm.has_pending() || !engine.fsm.syllables.is_empty() || engine.is_latin_mode || !engine.raw_ascii_buffer.is_empty() {
                        if is_ctrl {
                            let _ = engine.pop_word();
                        } else if is_shift {
                            let _ = engine.pop_syllable();
                        } else {
                            let _ = engine.pop_component();
                        }
                        true
                    } else {
                        false
                    }
                };

                if !eng_had_uncommitted {
                    self.consolidate_engine_to_pending();
                    let cur_val = *self.sentence_cursor.lock().unwrap();
                    let is_ctrl_del = is_ctrl && self.consolidate_pending_ctrl_backspace(cur_val);
                    if cur_val > 0 && !is_ctrl_del {
                        if is_ctrl {
                            let pending = self.pending_compose.lock().unwrap().clone();
                            let chars: Vec<char> = pending.chars().collect();
                            let mut delete_start = cur_val;
                            let prev_idx = cur_val - 1;
                            if prev_idx < chars.len() {
                                let c = chars[prev_idx];
                                if c.is_ascii_alphanumeric() {
                                    while delete_start > 0 && chars[delete_start - 1].is_ascii_alphanumeric() {
                                        delete_start -= 1;
                                    }
                                } else if c.is_whitespace() {
                                    while delete_start > 0 && chars[delete_start - 1].is_whitespace() {
                                        delete_start -= 1;
                                    }
                                } else {
                                    delete_start = prev_idx;
                                }
                            }
                            let count_to_delete = cur_val - delete_start;
                            for _ in 0..count_to_delete.max(1) {
                                let cur_now = *self.sentence_cursor.lock().unwrap();
                                if cur_now > 0 {
                                    self.remove_pending_char_at(cur_now - 1);
                                    *self.sentence_cursor.lock().unwrap() = cur_now - 1;
                                }
                            }
                        } else {
                            self.remove_pending_char_at(cur_val - 1);
                            *self.sentence_cursor.lock().unwrap() = cur_val - 1;
                            if let Ok(eng) = self.engine.lock() {
                                eng.set_backslash_escape_pending(false);
                            }
                            self.re_evaluate_joint_pending_and_engine();
                        }
                    }
                }

                let (full, _) = self.get_full_display_and_cursor();
                if !full.is_empty() {
                    self.sync_input_display(ctx)?;
                } else {
                    self.hide_candidate_window();
                    self.end_tsf_composition(ctx)?;
                }
                return Ok(BOOL::from(true));
            }

            // 6.2 Delete 刪除鍵 (全句自由向後刪除 + Ctrl+Delete 刪詞 + Shift+Delete 剪下選取)
            if vk == VK_DELETE.0 as u32 {
                // 選取範圍刪除優先（若同時按 Shift 則執行 CUA 剪下至剪貼簿）
                if let Some((start, end)) = self.get_manual_selection_range() {
                    if is_shift {
                        let (full, _) = self.get_full_display_and_cursor();
                        let chars: Vec<char> = full.chars().collect();
                        if start < end && end <= chars.len() {
                            let text: String = chars[start..end].iter().collect();
                            set_clipboard_text(&text);
                        }
                    }
                    self.push_undo();
                    self.delete_selection_if_any();
                    let (full, _) = self.get_full_display_and_cursor();
                    if !full.is_empty() {
                        self.sync_input_display(ctx)?;
                    } else {
                        self.hide_candidate_window();
                        self.end_tsf_composition(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                }

                self.delete_forward_in_desk(is_ctrl);
                let (full, _) = self.get_full_display_and_cursor();
                if !full.is_empty() {
                    self.sync_input_display(ctx)?;
                } else {
                    self.hide_candidate_window();
                    self.end_tsf_composition(ctx)?;
                }
                return Ok(BOOL::from(true));
            }

            // 7. Escape 鍵 (4 級階梯式撤銷 — 移植自沙盒)
            if vk == VK_ESCAPE.0 as u32 {
                let has_selection = self.sel_anchor.lock().unwrap().is_some();
                let is_cand_sel_now = *self.candidate_selecting.lock().unwrap();
                let has_engine_preedit = {
                    let engine = self.engine.lock().unwrap();
                    engine.fsm.has_pending()
                        || !engine.fsm.syllables.is_empty()
                        || !engine.composed_sentence.is_empty()
                        || !engine.raw_ascii_buffer.is_empty()
                        || engine.cangjie.has_buffer()
                        || engine.array30.has_buffer()
                        || engine.four_corner.has_buffer()
                        || engine.longkan.has_buffer()
                        || engine.japanese.has_buffer()
                        || engine.korean.has_buffer()
                        || engine.inline_longkan_active
                        || engine.is_backslash_escape_pending()
                        || matches!(
                            engine.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        )
                };
                let has_pending = !self.pending_compose.lock().unwrap().is_empty();

                if has_selection {
                    // Level 1：清除選取（保留文字）
                    *self.sel_anchor.lock().unwrap() = None;
                    self.sync_input_display(ctx)?;
                } else if is_cand_sel_now {
                    // Level 2：退出候選字選取模式
                    set_tab_range_popup_active(self.western_instance_key(), false);
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                    self.sync_input_display(ctx)?;
                } else if has_engine_preedit {
                    // Level 3：若處於單按 `` ` `` 臨時插入或 `` `` `` / `~~` 選單，僅退出該臨時子模式；否則清除引擎未決緩衝區
                    let still_has_eng = {
                        let mut eng = self.engine.lock().unwrap();
                        if eng.inline_longkan_active
                            || matches!(
                                eng.mode,
                                ruhi::engine::EngineMode::QuickNum
                                    | ruhi::engine::EngineMode::TemplateSelect
                                    | ruhi::engine::EngineMode::LexiconSwitch
                            )
                        {
                            let _ = eng.process_key('\x1b');
                        } else {
                            eng.reset_buffer();
                        }
                        !eng.composed_sentence.is_empty() || eng.fsm.has_pending()
                    };
                    if has_pending || still_has_eng {
                        self.sync_input_display(ctx)?;
                    } else {
                        self.hide_candidate_window();
                        self.end_tsf_composition(ctx)?;
                    }
                } else if has_pending {
                    // Level 4：全部清空重置
                    self.clear_all_pending();
                    *self.sentence_cursor.lock().unwrap() = 0;
                    *self.sel_anchor.lock().unwrap() = None;
                    self.hide_candidate_window();
                    self.end_tsf_composition(ctx)?;
                } else {
                    // 完全空白狀態：放行
                    return Ok(BOOL::from(false));
                }
                return Ok(BOOL::from(true));
            }

            // 8. 許氏注音、倉頡、行列 30 與標點符號輸入處理
            let char_opt = if is_english_mode
                && (!is_ctrl || is_altgr)
                && (!is_alt || is_altgr)
                && !is_win
            {
                let tr = self.translate_vk_in_english_mode(
                    vk,
                    is_shift,
                    is_altgr,
                    is_caps_on,
                    !is_en_inline_or_special,
                );
                match tr {
                    EnglishVkTranslation::Text(s)
                        if !is_en_inline_or_special || matches!(s.as_str(), "`" | "~") =>
                    {
                        s.chars().next()
                    }
                    _ => map_vk_to_char(vk, is_shift),
                }
            } else if !is_ctrl && !is_alt && !is_win {
                map_vk_to_char(vk, is_shift)
            } else {
                None
            };
            if let Some(ch) = char_opt {
                if !has_buffer
                    && (0x60..=0x69).contains(&vk)
                    && (is_shift || (!is_four_corner_mode && !is_fullwidth))
                {
                    return Ok(BOOL::from(false));
                }
                self.delete_selection_if_any();
                let (is_punct, should_consolidate_before_digit) = {
                    let mut engine = self.engine.lock().unwrap();
                    let is_jp = matches!(engine.keyboard_mode.as_str(), "japanese" | "jp" | "romaji");
                    let is_jp_romaji_key = is_jp && matches!(ch, '-' | '\'');
                    let is_kr = matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja");
                    let is_kr_romaja_key = is_kr && engine.korean.has_buffer() && ch == '-';
                    let is_ar30 = matches!(engine.keyboard_mode.as_str(), "array30" | "ar30");
                    let is_ar30_key = is_ar30 && (matches!(ch, ',' | '.' | ';' | '/') || (engine.array30.has_buffer() && matches!(ch, '?' | '*' | '_' | '-')));
                    let is_fc = matches!(engine.keyboard_mode.as_str(), "four_corner" | "4corner" | "fc");
                    let is_fc_key = is_fc && (ch.is_ascii_digit() || matches!(ch, '+' | '*' | '/' | '-' | '.' | '?'));
                    let is_lk = matches!(engine.keyboard_mode.as_str(), "longkan" | "lk") || engine.inline_longkan_active;
                    let is_special_mode = matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    );
                    let tilde_is_punct = matches!(
                        engine.tilde_toggle_mode.trim().to_ascii_lowercase().as_str(),
                        "punct" | "symbol" | "tilde" | "fullwidth" | "none" | ""
                    ) && engine.tilde_return_mode.is_none();
                    let is_user_abbrev_inline =
                        engine.inline_longkan_active && engine.inline_target_mode == "user_abbrev";
                    let is_lk_key = (is_lk && (engine.longkan.has_buffer() || engine.inline_longkan_active) && (ch.is_ascii_digit() || matches!(ch, '?' | '*' | '`')))
                        || is_user_abbrev_inline
                        || is_special_mode
                        || ch == '`'
                        || (ch == '~' && !tilde_is_punct);
                    let is_standard_bpm_punct = matches!(engine.keyboard_mode.as_str(), "standard" | "daikien")
                        && matches!(ch, ',' | '.' | ';' | '/' | '-');
                    let is_inline_digit_op = !engine.fullwidth_mode
                        && !is_standard_bpm_punct
                        && !engine.fsm.has_pending()
                        && engine
                            .composed_sentence
                            .chars()
                            .last()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(false)
                        && matches!(ch, '.' | '%' | '+' | '*' | '=' | ':');
                    let bs_pending = engine.is_backslash_escape_pending();
                    let punct = !is_ar30_key
                        && !is_fc_key
                        && !is_lk_key
                        && !is_standard_bpm_punct
                        && !is_inline_digit_op
                        && !is_jp_romaji_key
                        && !is_kr_romaja_key
                        && ch != '\\'
                        && !bs_pending
                        && !matches!(engine.mode, ruhi::engine::EngineMode::QuickNum | ruhi::engine::EngineMode::TemplateSelect | ruhi::engine::EngineMode::LexiconSwitch)
                        && (engine.resolve_punctuation_char(ch).is_some()
                            || (is_jp && matches!(ch, ',' | '.' | '?' | '!' | '[' | ']' | '(' | ')' | ':' | ';')));
                    let raw_lower = engine.raw_ascii_buffer.to_ascii_lowercase();
                    let is_en_punct_token = engine.is_mixed_english_active()
                        && (engine.is_latin_mode
                            || (!engine.has_any_compound_word()
                                && engine.raw_ascii_buffer.len() >= 3
                                && engine.western.lexicon.contains(&raw_lower)));
                    let punct_consolidate = punct
                        && !is_en_punct_token
                        && !engine.is_latin_mode
                        && !engine.cangjie.has_buffer()
                        && !engine.array30.has_buffer()
                        && !engine.four_corner.has_buffer()
                        && !engine.longkan.has_buffer()
                        && !engine.japanese.has_buffer()
                        && engine.fsm.has_pending();
                    let digit_consolidate = ch.is_ascii_digit()
                        && !is_fc
                        && !is_lk_key
                        && !engine.is_latin_mode
                        && !engine.cangjie.has_buffer()
                        && !engine.array30.has_buffer()
                        && !engine.four_corner.has_buffer()
                        && !engine.longkan.has_buffer()
                        && !matches!(engine.mode, ruhi::engine::EngineMode::QuickNum | ruhi::engine::EngineMode::TemplateSelect | ruhi::engine::EngineMode::LexiconSwitch)
                        && (!engine.fsm.syllables.is_empty() || engine.fsm.has_pending() || engine.japanese.has_buffer() || engine.korean.has_buffer());
                    if (punct_consolidate || digit_consolidate) && engine.fsm.has_pending() {
                        engine.fsm.commit_current('1');
                        engine.update_sentence_composition();
                    }
                    (punct, digit_consolidate)
                };
                let is_stream_mode = punct_mode == "B" || punct_mode == "C";
                let should_consolidate_before_tilde = ch == '~' && !is_punct && {
                    let eng = self.engine.lock().unwrap();
                    !eng.inline_longkan_active
                        && !eng.is_backslash_escape_pending()
                        && !matches!(
                            eng.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        )
                };
                if (is_punct && is_stream_mode) || should_consolidate_before_digit || should_consolidate_before_tilde {
                    self.consolidate_engine_to_pending();
                }

                let mut engine = self.engine.lock().unwrap();
                let fc_buf_before = if !engine.four_corner.buffer.is_empty() {
                    engine.four_corner.buffer.clone()
                } else if !engine.longkan.buffer.is_empty() {
                    engine.longkan.buffer.clone()
                } else if !engine.array30.buffer.is_empty() {
                    engine.array30.buffer.clone()
                } else {
                    String::new()
                };
                let prev_kb_mode = engine.keyboard_mode.clone();
                if engine.fsm.is_empty() && engine.composed_sentence.is_empty() {
                    let cur_pos = *self.sentence_cursor.lock().unwrap();
                    let p_str = self.pending_compose.lock().unwrap().clone();
                    let p_chars: Vec<char> = p_str.chars().collect();
                    let c_idx = cur_pos.min(p_chars.len());
                    if c_idx > 0 {
                        let left_ctx: String = p_chars[..c_idx].iter().collect();
                        engine.record_committed(&left_ctx);
                    }
                }
                let pre_has_pending = engine.fsm.has_pending();
                let pre_syl_len = engine.fsm.syllables.len();
                let pre_composed_len = engine.composed_sentence.len();

                let resp = engine.process_key(ch);
                let post_has_pending = engine.fsm.has_pending();
                let post_syl_len = engine.fsm.syllables.len();
                let post_composed_len = engine.composed_sentence.len();

                let action = resp.action;
                let committed_chunk = resp.committed_chunk;
                let new_kb_mode = engine.keyboard_mode.clone();
                let new_last_mode = engine.last_keyboard_mode.clone();
                let _eng_disp = engine.get_display_text();
                let _eng_cur = engine.cursor_pos;
                drop(engine);

                let skip_joint = should_skip_joint_reeval_on_key(
                    pre_has_pending,
                    post_has_pending,
                    pre_syl_len,
                    post_syl_len,
                    pre_composed_len,
                    post_composed_len,
                    &action,
                );
                if !skip_joint {
                    self.re_evaluate_joint_pending_and_engine();
                }
                if action == "switch_keyboard_mode"
                    || action == "show_lexicon_switcher"
                    || new_kb_mode != prev_kb_mode
                {
                    if let Ok(mut cfg) = self.config.lock() {
                        cfg.keyboard_mode = new_kb_mode.clone();
                        cfg.last_keyboard_mode = new_last_mode.clone();
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.apply_config(&cfg);
                            }
                        }
                    }
                    crate::candidate_window::save_keyboard_mode_with_last(
                        &new_kb_mode,
                        Some(&new_last_mode),
                    );
                    crate::candidate_window::broadcast_tray_update();
                    notify_langbar_mode_update();
                }

                if action == "switch_keyboard_mode" {
                    if !committed_chunk.is_empty() {
                        let c_chars: Vec<char> = committed_chunk.chars().collect();
                        let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                        self.splice_pending_chunk(&c_chars, &c_syls, &[]);
                    }
                    let (full, _) = self.get_full_display_and_cursor();
                    if !full.is_empty() {
                        self.sync_input_display(ctx)?;
                    } else {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.show_mode_toast(&new_kb_mode, false, cx, cy);
                            }
                        }
                    }
                    return Ok(BOOL::from(true));
                }

                if !action.is_empty() && action != "none" {
                    if action == "commit_punctuation" {
                        // 標點結算行為支援三種策略：
                        // A: 經典即時送出（微軟新注音風格，任何標點即時結算送出）
                        // B: 智慧句末送出（自然長句折衷風格，逗號保留維持長句，句末標點。/？/！/；/…等自動結算送出）
                        // C: 全手動長句模式（純 Enter 送出，所有標點均保留在長句流中，絕不自動送出）
                        let is_multiline_desk = self
                            .pending_compose
                            .lock()
                            .map(|p| p.contains('\n'))
                            .unwrap_or(false);
                        let is_sentence_terminator = matches!(committed_chunk.as_str(), "。" | "？" | "！" | "；" | "…" | "……" | "?" | "!");
                        let should_keep_in_stream = is_multiline_desk
                            || match punct_mode.as_str() {
                                "C" => true,
                                "B" => !is_sentence_terminator,
                                _ => false, // 模式 A：經典即時結算
                            };

                        if should_keep_in_stream {
                            // 追加進暫存流，維持底線長句！
                            if matches!(committed_chunk.as_str(), "？" | "?") {
                                let mut pending = self.pending_compose.lock().unwrap();
                                if pending.ends_with("でしだが") {
                                    let new_len = pending.len() - "でしだが".len();
                                    pending.truncate(new_len);
                                    pending.push_str("でしたか");
                                } else if pending.ends_with("ましだが") {
                                    let new_len = pending.len() - "ましだが".len();
                                    pending.truncate(new_len);
                                    pending.push_str("ましたか");
                                } else if pending.ends_with("ですが") || pending.ends_with("ますが") {
                                    let new_len = pending.len() - "が".len();
                                    pending.truncate(new_len);
                                    pending.push('か');
                                }
                            }
                            if matches!(committed_chunk.as_str(), "「" | "『") {
                                let cur_pos = *self.sentence_cursor.lock().unwrap();
                                let pinned_set = self.pending_pinned.lock().unwrap().clone();
                                let syls_vec = self.pending_syllables.lock().unwrap().clone();
                                let mut pending = self.pending_compose.lock().unwrap();
                                let mut chars_vec: Vec<char> = pending.chars().collect();
                                if cur_pos > 0
                                    && cur_pos <= chars_vec.len()
                                    && !pinned_set.contains(&(cur_pos - 1))
                                    && matches!(chars_vec[cur_pos - 1], '案' | '岸' | '暗')
                                    && syls_vec.get(cur_pos - 1).and_then(|s| s.as_deref()) == Some("ㄢˋ")
                                {
                                    let prev_is_noun_prefix = cur_pos >= 2
                                        && matches!(
                                            chars_vec[cur_pos - 2],
                                            '方' | '專' | '草' | '檔' | '提' | '法' | '個' | '圖' | '備'
                                                | '報' | '定' | '結' | '破' | '命' | '慘' | '血' | '懸'
                                                | '冤' | '雙' | '對' | '答' | '海' | '河' | '上'
                                                | '兩' | '左' | '右' | '黑' | '陰' | '昏' | '幽'
                                        );
                                    if !prev_is_noun_prefix {
                                        chars_vec[cur_pos - 1] = '按';
                                        *pending = chars_vec.into_iter().collect();
                                    }
                                }
                            }
                            let p_chars: Vec<char> = committed_chunk.chars().collect();
                            let p_syls: Vec<Option<String>> = vec![None; p_chars.len()];
                            self.splice_pending_chunk(&p_chars, &p_syls, &[]);
                            self.sync_input_display(ctx)?;
                        } else {
                            // 結算送出全句！
                            let mut pending_base = self.pending_compose.lock().unwrap().clone();
                            if matches!(committed_chunk.as_str(), "？" | "?") {
                                if pending_base.ends_with("でしだが") {
                                    let new_len = pending_base.len() - "でしだが".len();
                                    pending_base.truncate(new_len);
                                    pending_base.push_str("でしたか");
                                } else if pending_base.ends_with("ましだが") {
                                    let new_len = pending_base.len() - "ましだが".len();
                                    pending_base.truncate(new_len);
                                    pending_base.push_str("ましたか");
                                } else if pending_base.ends_with("ですが") || pending_base.ends_with("ますが") {
                                    let new_len = pending_base.len() - "が".len();
                                    pending_base.truncate(new_len);
                                    pending_base.push('か');
                                }
                            }
                            let full = format!("{}{}", pending_base, committed_chunk);
                            if !full.is_empty() {
                                if let Ok(mut eng) = self.engine.lock() {
                                    eng.record_committed(&full);
                                }
                            }
                            self.clear_all_pending();
                            *self.sentence_cursor.lock().unwrap() = 0;
                            let c = full.chars().count();
                            self.update_tsf_composition(ctx, &full, c, None, true)?;
                            self.hide_candidate_window();
                        }
                    } else if action.starts_with("commit") {
                        if !committed_chunk.is_empty() {
                            if let Ok(mut eng) = self.engine.lock() {
                                eng.record_committed(&committed_chunk);
                            }
                        }
                        let cur_before = *self.sentence_cursor.lock().unwrap();
                        let c_chars: Vec<char> = committed_chunk.chars().collect();
                        let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                        let c_segs = if c_chars.len() > 1 && committed_chunk != " " {
                            vec![(0, c_chars.len(), committed_chunk.clone(), fc_buf_before.clone())]
                        } else {
                            Vec::new()
                        };
                        self.splice_pending_chunk(&c_chars, &c_syls, &c_segs);
                        if !fc_buf_before.is_empty() && !c_chars.is_empty() {
                            *self.last_raw_ascii.lock().unwrap() =
                                Some((cur_before, cur_before + c_chars.len(), fc_buf_before));
                        }
                        self.sync_input_display(ctx)?;
                    } else {
                        // 一般組字中：更新長句顯示，絕不主動彈出候選窗
                        // 若引擎回傳了先前的結算字串 (例如「1.」)，立即保全納入 pending_compose，絕不丟失！
                        if !committed_chunk.is_empty() {
                            let c_chars: Vec<char> = committed_chunk.chars().collect();
                            let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                            let c_segs = if c_chars.len() > 1 && committed_chunk != " " {
                                vec![(0, c_chars.len(), String::new(), committed_chunk.clone())]
                            } else {
                                Vec::new()
                            };
                            self.splice_pending_chunk(&c_chars, &c_syls, &c_segs);
                        }

                        self.sync_input_display(ctx)?;
                    }
                    return Ok(BOOL::from(true));
                }
                if (0x41..=0x5A).contains(&vk) || has_buffer {
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            Ok(BOOL::from(false))
        })).unwrap_or_else(|_| Ok(BOOL::from(false)))
    }

    fn OnTestKeyUp(&self, _pic: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Ok(BOOL::from(false))
        })).unwrap_or_else(|_| Ok(BOOL::from(false)))
    }

    fn OnKeyUp(&self, _pic: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Ok(BOOL::from(false))
        })).unwrap_or_else(|_| Ok(BOOL::from(false)))
    }

    fn OnPreservedKey(&self, _pic: Option<&ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Ok(BOOL::from(false))
        })).unwrap_or_else(|_| Ok(BOOL::from(false)))
    }
}

impl RuhiTextService_Impl {
    fn snapshot_key_routing_flags(&self) -> (bool, bool, bool, bool, bool, bool, bool) {
        let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
        let engine = self.engine.lock().unwrap();
        let has_buffer = pending_len > 0
            || engine.fsm.has_pending()
            || !engine.fsm.syllables.is_empty()
            || !engine.composed_sentence.is_empty()
            || !engine.raw_ascii_buffer.is_empty()
            || engine.cangjie.has_buffer()
            || engine.array30.has_buffer()
            || engine.four_corner.has_buffer()
            || engine.longkan.has_buffer()
            || engine.japanese.has_buffer()
            || engine.korean.has_buffer()
            || engine.inline_longkan_active
            || engine.is_backslash_escape_pending()
            || matches!(
                engine.mode,
                ruhi::engine::EngineMode::QuickNum
                    | ruhi::engine::EngineMode::TemplateSelect
                    | ruhi::engine::EngineMode::LexiconSwitch
                    | ruhi::engine::EngineMode::SymbolTree
            );
        let lower_kb = engine.keyboard_mode.to_lowercase();
        let is_english_mode = matches!(
            lower_kb.as_str(),
            "english" | "en" | "latin" | "western" | "euro" | "sanskrit"
        );
        let is_four_corner_mode = matches!(lower_kb.as_str(), "four_corner" | "4corner" | "fc");
        let is_longkan_mode = matches!(lower_kb.as_str(), "longkan" | "lk");
        let is_standard_mode = matches!(lower_kb.as_str(), "standard" | "daikien");
        let is_en_inline_or_special = engine.inline_longkan_active
            || matches!(
                engine.mode,
                ruhi::engine::EngineMode::QuickNum
                    | ruhi::engine::EngineMode::TemplateSelect
                    | ruhi::engine::EngineMode::LexiconSwitch
                    | ruhi::engine::EngineMode::SymbolTree
            );
        let tilde_is_punct = matches!(
            engine.tilde_toggle_mode.trim().to_ascii_lowercase().as_str(),
            "punct" | "symbol" | "tilde" | "fullwidth" | "none" | ""
        ) && engine.tilde_return_mode.is_none();
        let tilde_triggers_mode = !tilde_is_punct;
        (
            has_buffer,
            is_english_mode,
            is_four_corner_mode,
            is_longkan_mode,
            is_standard_mode,
            is_en_inline_or_special,
            tilde_triggers_mode,
        )
    }

    pub(crate) fn paste_text_into_desk(&self, raw_text: &str) -> bool {
        let normalized = normalize_clipboard_paste_text(raw_text);
        if normalized.is_empty() {
            return false;
        }
        self.consolidate_engine_to_pending();
        let p_chars: Vec<char> = normalized.chars().collect();
        if let Some((start, end)) = self.get_manual_selection_range() {
            if start < end {
                self.replace_pending_range(start, end, &p_chars, None);
                {
                    let mut syls = self.pending_syllables.lock().unwrap();
                    let clear_end = (start + p_chars.len()).min(syls.len());
                    for item in syls[start..clear_end].iter_mut() {
                        *item = None;
                    }
                }
                *self.sel_anchor.lock().unwrap() = None;
                *self.sentence_cursor.lock().unwrap() = start + p_chars.len();
                return true;
            }
        }
        let p_syls: Vec<Option<String>> = vec![None; p_chars.len()];
        self.splice_pending_chunk(&p_chars, &p_syls, &[]);
        true
    }

    pub(crate) fn delete_forward_in_desk(&self, is_ctrl: bool) -> bool {
        self.consolidate_engine_to_pending();
        let cur = *self.sentence_cursor.lock().unwrap();
        let chars: Vec<char> = self.pending_compose.lock().unwrap().chars().collect();
        if cur >= chars.len() {
            return false;
        }
        if is_ctrl {
            let mut delete_end = cur;
            let c = chars[cur];
            if c.is_ascii_alphanumeric() {
                while delete_end < chars.len() && chars[delete_end].is_ascii_alphanumeric() {
                    delete_end += 1;
                }
            } else if c.is_whitespace() {
                while delete_end < chars.len() && chars[delete_end].is_whitespace() {
                    delete_end += 1;
                }
            } else {
                delete_end = cur + 1;
            }
            self.replace_pending_range(cur, delete_end, &[], None);
            *self.sentence_cursor.lock().unwrap() = cur;
            self.re_evaluate_joint_pending_and_engine();
            true
        } else {
            self.remove_pending_char_at(cur);
            self.re_evaluate_joint_pending_and_engine();
            true
        }
    }

    fn is_western_leading_word_char(c: char) -> bool {
        c.is_ascii_alphabetic()
            || ruhi::engine::RuhiEngine::is_western_non_ascii_letter_or_mark(c)
    }

    fn translate_vk_in_english_mode(
        &self,
        vk: u32,
        is_shift: bool,
        is_altgr: bool,
        is_caps_on: bool,
        mutate_dead_key_state: bool,
    ) -> EnglishVkTranslation {
        let capslock_none = self
            .config
            .lock()
            .map(|c| c.capslock_switch == "none")
            .unwrap_or(false);
        let is_caps_effective = capslock_none && is_caps_on;
        let is_numlock_on = unsafe { (GetKeyState(VK_NUMLOCK.0 as i32) as u16 & 0x0001) != 0 };
        let hkl = unsafe { GetKeyboardLayout(0) };
        let klid = current_klid_string();
        translate_vk_in_english_layout_with_hkl(
            vk,
            is_shift,
            is_altgr,
            is_caps_effective,
            is_numlock_on,
            hkl,
            klid.as_deref(),
            mutate_dead_key_state,
        )
    }

    pub(crate) fn classify_english_key_translation(
        &self,
        has_buffer: bool,
        tilde_triggers_mode: bool,
        is_bs_pending: bool,
        is_password_field: bool,
        update_sigil: bool,
        trans: &EnglishVkTranslation,
    ) -> bool {
        let inst_key = self.western_instance_key();
        match trans {
            EnglishVkTranslation::DeadKey => {
                if !has_buffer && update_sigil {
                    clear_western_leading_sigil(inst_key);
                }
                has_buffer
            }
            EnglishVkTranslation::None => {
                if !has_buffer && update_sigil {
                    clear_western_leading_sigil(inst_key);
                }
                false
            }
            EnglishVkTranslation::Text(s) => {
                if s == "`" || (tilde_triggers_mode && s == "~") {
                    if !has_buffer && update_sigil {
                        clear_western_leading_sigil(inst_key);
                    }
                    return true;
                }
                if s == "\\" {
                    if !has_buffer && update_sigil {
                        clear_western_leading_sigil(inst_key);
                    }
                    return true;
                }
                if has_buffer || (is_bs_pending && !s.is_empty()) {
                    return true;
                }
                if s.chars().any(Self::is_western_leading_word_char) {
                    // 保留先前直通記錄之前導邊界字元（如 /teh 的 '/'），待空白鍵勘誤消費
                    return true;
                }
                if update_sigil {
                    if !is_password_field
                        && !s.is_empty()
                        && s.chars()
                            .all(ruhi::engine::RuhiEngine::is_western_token_boundary_blocker)
                    {
                        if let Some(ch) = s.chars().last() {
                            record_western_leading_sigil(inst_key, ch);
                        }
                    } else {
                        clear_western_leading_sigil(inst_key);
                    }
                }
                false
            }
        }
    }

    pub(crate) fn evaluate_test_key_down_with_mods(
        &self,
        vk: u32,
        is_ctrl: bool,
        is_alt: bool,
        is_shift: bool,
        is_win: bool,
        is_password_field: bool,
        is_synthetic_caps: bool,
    ) -> bool {
        self.evaluate_test_key_down_full(
            vk,
            is_ctrl,
            is_alt,
            is_shift,
            is_win,
            false,
            is_password_field,
            is_synthetic_caps,
        )
    }

    pub(crate) fn evaluate_test_key_down_full(
        &self,
        vk: u32,
        is_ctrl: bool,
        is_alt: bool,
        is_shift: bool,
        is_win: bool,
        is_altgr: bool,
        is_password_field: bool,
        is_synthetic_caps: bool,
    ) -> bool {
        if is_passthrough_non_typing_vk(vk) || is_win {
            return false;
        }
        let maybe_sym_hk = vk == 0xBC && is_ctrl && is_alt && !is_shift && !is_win;
        if is_alt && !is_altgr && !maybe_sym_hk {
            return false;
        }

        let (
            has_buffer,
            is_english_mode,
            is_four_corner_mode,
            _is_longkan_mode,
            is_standard_mode,
            is_en_inline_or_special,
            tilde_triggers_mode,
        ) = self.snapshot_key_routing_flags();

        let sym_hk = is_symbol_tree_hotkey_combo(
            vk,
            is_ctrl,
            is_alt,
            is_shift,
            is_win,
            is_altgr,
            is_english_mode,
        );
        if is_alt && !(is_altgr && is_english_mode) && !sym_hk {
            return false;
        }

        if !is_english_mode {
            clear_western_leading_sigil(self.western_instance_key());
        }

        if !has_buffer && is_password_field {
            clear_western_leading_sigil(self.western_instance_key());
            return false;
        }

        if sym_hk {
            return true;
        }
        if self
            .engine
            .lock()
            .map(|e| e.mode == ruhi::engine::EngineMode::SymbolTree)
            .unwrap_or(false)
        {
            return !is_ctrl && !is_alt && !is_win && is_symbol_tree_accepted_vk(vk);
        }

        if vk == VK_CAPITAL.0 as u32 {
            clear_western_leading_sigil(self.western_instance_key());
            if is_synthetic_caps {
                return false;
            }
            let capslock_none = self
                .config
                .lock()
                .map(|c| c.capslock_switch == "none")
                .unwrap_or(false);
            let is_single_mode = self
                .engine
                .lock()
                .map(|e| e.get_full_cycle_modes().len() <= 1)
                .unwrap_or(false);
            if (!is_ctrl && !is_shift && capslock_none) || is_single_mode {
                return false;
            }
            return true;
        }

        let (allow_shift_space, is_fullwidth, is_direct_fc_punct) = self
            .config
            .lock()
            .map(|c| {
                (
                    c.shift_space_toggle_fullwidth,
                    c.fullwidth_mode,
                    c.four_corner_keymap == "direct_punct",
                )
            })
            .unwrap_or((true, false, false));

        if vk == VK_SPACE.0 as u32 && is_shift && !is_ctrl {
            if allow_shift_space {
                clear_western_leading_sigil(self.western_instance_key());
                return true;
            }
        }

        if is_ctrl && !is_altgr {
            if !has_buffer {
                clear_western_leading_sigil(self.western_instance_key());
                return false;
            }
            if let Some(combo) =
                resolve_ctrl_symbol_combo(vk, is_shift, has_buffer, is_english_mode, is_four_corner_mode)
            {
                if self
                    .engine
                    .lock()
                    .map(|mut e| e.symbols.contains_key(&combo))
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            return has_buffer;
        }

        let is_cand_sel = *self.candidate_selecting.lock().unwrap();
        if is_cand_sel {
            let quick_mode = self.effective_quick_key_mode();
            if vk == VK_DOWN.0 as u32
                || vk == VK_UP.0 as u32
                || vk == VK_NEXT.0 as u32
                || vk == VK_PRIOR.0 as u32
                || vk == VK_TAB.0 as u32
            {
                return true;
            }
            if is_four_corner_mode
                && (vk == 0x6B || vk == 0x6D || (vk == 0xBB && is_shift) || (vk == 0xBD && !is_shift))
            {
                return true;
            }
            if quick_mode == "asdf" && map_asdf_to_idx(vk).is_some() && !is_shift {
                return true;
            }
            if (quick_mode == "1234" || is_four_corner_mode)
                && ((0x30..=0x39).contains(&vk) || (0x60..=0x69).contains(&vk))
                && !is_shift
            {
                return true;
            }
            if vk == VK_SPACE.0 as u32 || vk == VK_RETURN.0 as u32 || vk == VK_ESCAPE.0 as u32 {
                return true;
            }
        }

        if !has_buffer {
            if vk == VK_SPACE.0 as u32 {
                clear_western_leading_sigil(self.western_instance_key());
                return is_fullwidth && !is_shift;
            }
            if matches!(
                vk,
                x if x == VK_RETURN.0 as u32
                    || x == VK_BACK.0 as u32
                    || x == VK_DELETE.0 as u32
                    || x == VK_INSERT.0 as u32
                    || x == VK_TAB.0 as u32
                    || x == VK_ESCAPE.0 as u32
                    || (x == 0x6E && (!is_four_corner_mode || is_shift))
                    || x == VK_LEFT.0 as u32
                    || x == VK_RIGHT.0 as u32
                    || x == VK_UP.0 as u32
                    || x == VK_DOWN.0 as u32
                    || x == VK_HOME.0 as u32
                    || x == VK_END.0 as u32
                    || x == VK_PRIOR.0 as u32
                    || x == VK_NEXT.0 as u32
            ) {
                clear_western_leading_sigil(self.western_instance_key());
                return false;
            }
        }

        if is_english_mode {
            if is_en_inline_or_special && vk == VK_OEM_3.0 as u32 {
                clear_western_leading_sigil(self.western_instance_key());
                return true;
            }
            if !is_en_inline_or_special {
                if has_buffer
                    && (vk == VK_RETURN.0 as u32
                        || vk == VK_SPACE.0 as u32
                        || vk == VK_BACK.0 as u32
                        || vk == VK_ESCAPE.0 as u32
                        || vk == VK_DELETE.0 as u32
                        || vk == VK_TAB.0 as u32
                        || vk == VK_INSERT.0 as u32
                        || vk == VK_LEFT.0 as u32
                        || vk == VK_RIGHT.0 as u32
                        || vk == VK_UP.0 as u32
                        || vk == VK_DOWN.0 as u32
                        || vk == VK_HOME.0 as u32
                        || vk == VK_END.0 as u32
                        || vk == VK_PRIOR.0 as u32
                        || vk == VK_NEXT.0 as u32)
                {
                    return true;
                }
                let is_bs_pending = self
                    .engine
                    .lock()
                    .map(|e| e.is_backslash_escape_pending())
                    .unwrap_or(false);
                let is_caps_on =
                    unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };
                let trans = self.translate_vk_in_english_mode(
                    vk,
                    is_shift,
                    is_altgr,
                    is_caps_on,
                    false,
                );
                return self.classify_english_key_translation(
                    has_buffer,
                    tilde_triggers_mode,
                    is_bs_pending,
                    is_password_field,
                    true,
                    &trans,
                );
            }
        }

        if is_shift && (0x41..=0x5A).contains(&vk) && (!is_four_corner_mode || has_buffer) {
            return true;
        }

        if has_buffer {
            vk == VK_RETURN.0 as u32
                || vk == VK_SPACE.0 as u32
                || vk == VK_BACK.0 as u32
                || vk == VK_ESCAPE.0 as u32
                || vk == VK_DELETE.0 as u32
                || vk == VK_TAB.0 as u32
                || vk == VK_INSERT.0 as u32
                || (0x41..=0x5A).contains(&vk)
                || (((0x30..=0x39).contains(&vk) || (0x60..=0x6F).contains(&vk)) && !is_shift)
                || is_punctuation_vk(vk, is_shift)
                || vk == VK_OEM_3.0 as u32
                || vk == VK_LEFT.0 as u32
                || vk == VK_RIGHT.0 as u32
                || vk == VK_UP.0 as u32
                || vk == VK_DOWN.0 as u32
                || vk == VK_HOME.0 as u32
                || vk == VK_END.0 as u32
                || vk == VK_PRIOR.0 as u32
                || vk == VK_NEXT.0 as u32
        } else if vk == VK_OEM_3.0 as u32 {
            true
        } else if is_four_corner_mode {
            if vk == 0xDC && !is_shift {
                true
            } else if !is_direct_fc_punct && matches!(vk, 0x6B | 0x6F) {
                false
            } else {
                matches!(vk, 0x60..=0x6B | 0x6D..=0x6F) && !is_shift
            }
        } else if is_punctuation_vk(vk, is_shift) || (0x41..=0x5A).contains(&vk) {
            true
        } else if is_standard_mode
            && matches!(vk, 0x30 | 0x31 | 0x32 | 0x35 | 0x37 | 0x38 | 0x39)
            && !is_shift
        {
            true
        } else {
            is_fullwidth && ((0x30..=0x39).contains(&vk) || (0x60..=0x69).contains(&vk)) && !is_shift
        }
    }

    fn effective_quick_key_mode(&self) -> String {
        if let Ok(eng) = self.engine.lock() {
            match eng.effective_candidate_quick_key() {
                "asdf" => "asdf".to_string(),
                _ => "1234".to_string(),
            }
        } else {
            "asdf".to_string()
        }
    }
    fn get_word_boundaries(&self, full_text: &str) -> Vec<usize> {
        use std::collections::BTreeSet;
        let chars: Vec<char> = full_text.chars().collect();
        let total = chars.len();
        if total == 0 {
            return vec![0];
        }

        let mut bset = BTreeSet::new();
        bset.insert(0);
        bset.insert(total);

        // 1. 納入 pending_segments 斷詞邊界
        let segs = self.pending_segments.lock().unwrap();
        let mut covered = vec![false; total];
        for seg in segs.iter() {
            if seg.0 < total && seg.1 <= total && seg.0 < seg.1 {
                bset.insert(seg.0);
                bset.insert(seg.1);
                for i in seg.0..seg.1 {
                    covered[i] = true;
                }
            }
        }

        // 2. 未切詞區域（歐文單字、數字、標點、空白、獨立漢字）依字元屬性轉換判定邊界
        #[derive(PartialEq, Eq, Clone, Copy)]
        enum Kind {
            AsciiWord,
            HangulWord,
            Whitespace,
            Punct,
            CjkSingle,
            Other,
        }

        let classify = |ch: char| -> Kind {
            if ch.is_ascii_alphanumeric()
                || ch == '_'
                || ch == '\''
                || ('\u{0300}'..='\u{036F}').contains(&ch)
                || ruhi::western::WesternEngine::strip_char_diacritic(ch).is_ascii_alphabetic()
            {
                Kind::AsciiWord
            } else if ('\u{AC00}'..='\u{D7A3}').contains(&ch) || ('\u{3131}'..='\u{318E}').contains(&ch) {
                Kind::HangulWord
            } else if ch.is_whitespace() {
                Kind::Whitespace
            } else if ch.is_ascii_punctuation() || "，。、！？：；（）「」『』—…～《》〈〉".contains(ch) {
                Kind::Punct
            } else if ('\u{4e00}'..='\u{9fa5}').contains(&ch) || ('\u{3400}'..='\u{4dbf}').contains(&ch) {
                Kind::CjkSingle
            } else {
                Kind::Other
            }
        };

        // 僅對未被分詞涵蓋之單獨漢字建立逐字邊界；歐文單字 (AsciiWord) 保持整詞連貫！
        for i in 0..total {
            if !covered[i] && classify(chars[i]) == Kind::CjkSingle {
                bset.insert(i);
                bset.insert(i + 1);
            }
        }

        for i in 0..total.saturating_sub(1) {
            let k1 = classify(chars[i]);
            let k2 = classify(chars[i + 1]);
            if k1 != k2 {
                bset.insert(i + 1);
            }
        }

        bset.into_iter().collect()
    }

    fn consolidate_pending_ctrl_backspace(&self, cur_val: usize) -> bool {
        if cur_val == 0 {
            return false;
        }
        let pending = self.pending_compose.lock().unwrap().clone();
        let boundaries = self.get_word_boundaries(&pending);
        let delete_start = Self::find_prev_word_boundary(&boundaries, &pending, cur_val);
        let count_to_delete = cur_val.saturating_sub(delete_start).max(1);
        for _ in 0..count_to_delete {
            let cur_now = *self.sentence_cursor.lock().unwrap();
            if cur_now > 0 {
                self.remove_pending_char_at(cur_now - 1);
                *self.sentence_cursor.lock().unwrap() = cur_now - 1;
            }
        }
        true
    }

    fn find_prev_word_boundary(boundaries: &[usize], full_text: &str, cur: usize) -> usize {
        let chars: Vec<char> = full_text.chars().collect();
        let mut pos = cur.min(chars.len());
        // 遵循標準文書軟體 (Word / VS Code / Notepad) 習慣：Ctrl+Left 先跳過游標左側緊鄰之空白，再停於前一個單詞開頭
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        boundaries.iter().rev().copied().find(|&b| b < pos).unwrap_or(0)
    }

    fn find_next_word_boundary(boundaries: &[usize], full_text: &str, cur: usize, total: usize) -> usize {
        let chars: Vec<char> = full_text.chars().collect();
        let mut pos = cur.min(total);
        // 若當前游標位於空白上，先跳過空白；否則跳至當前單詞結尾並跨越緊隨其後之空白（直達下一詞開頭）
        if pos < total && chars[pos].is_whitespace() {
            while pos < total && chars[pos].is_whitespace() {
                pos += 1;
            }
            return pos;
        }
        let mut next_b = boundaries.iter().copied().find(|&b| b > pos).unwrap_or(total);
        while next_b < total && chars[next_b].is_whitespace() {
            next_b += 1;
        }
        next_b
    }

    fn find_prev_punctuation_boundary(full_text: &str, cur: usize) -> usize {
        let chars: Vec<char> = full_text.chars().collect();
        let pos = cur.min(chars.len());
        for i in (0..pos).rev() {
            if ruhi::engine::RuhiEngine::is_clause_punctuation(chars[i]) {
                return i;
            }
        }
        0
    }

    fn find_next_punctuation_boundary(full_text: &str, cur: usize, total: usize) -> usize {
        let chars: Vec<char> = full_text.chars().collect();
        let pos = cur.min(total);
        if pos < total {
            for i in (pos + 1)..total {
                if ruhi::engine::RuhiEngine::is_clause_punctuation(chars[i]) {
                    return i;
                }
            }
        }
        total
    }

    fn take_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            pending: self.pending_compose.lock().unwrap().clone(),
            cursor: *self.sentence_cursor.lock().unwrap(),
            anchor: *self.sel_anchor.lock().unwrap(),
            syllables: self.pending_syllables.lock().unwrap().clone(),
            segments: self.pending_segments.lock().unwrap().clone(),
            manual_cuts: self.manual_cuts.lock().unwrap().clone(),
            pinned: self.pending_pinned.lock().unwrap().clone(),
        }
    }

    fn push_undo(&self) {
        self.set_last_western_autocorrect(None);
        let inst_key = self.western_instance_key();
        let snap = self.take_snapshot();
        let mut undo = self.undo_stack.lock().unwrap();
        prune_western_undo_pins_above(inst_key, undo.len() + 1);
        undo.push_back(snap);
        if undo.len() > 100 {
            undo.pop_front();
            shift_western_undo_pins_after_cap_pop(inst_key);
        }
        self.redo_stack.lock().unwrap().clear();
    }

    fn undo(&self) -> bool {
        let inst_key = self.western_instance_key();
        let matched_rb = self.take_last_western_autocorrect_if_matching();
        self.set_last_western_autocorrect(None);
        let (snap_opt, popped_depth) = {
            let mut undo = self.undo_stack.lock().unwrap();
            let popped = undo.pop_back();
            let depth = undo.len() + 1;
            (popped, depth)
        };
        if let Some(mut snap) = snap_opt {
            let pin_opt = take_western_undo_pin_for_depth(inst_key, popped_depth)
                .or_else(|| matched_rb.map(|rb| rb.pin_range));
            if let Some((p_start, p_end)) = pin_opt {
                for idx in p_start..p_end {
                    snap.pinned.insert(idx);
                }
            }
            let current = self.take_snapshot();
            self.redo_stack.lock().unwrap().push_back(current);

            *self.pending_compose.lock().unwrap() = snap.pending;
            *self.sentence_cursor.lock().unwrap() = snap.cursor;
            *self.sel_anchor.lock().unwrap() = snap.anchor;
            *self.pending_syllables.lock().unwrap() = snap.syllables;
            *self.pending_segments.lock().unwrap() = snap.segments;
            *self.manual_cuts.lock().unwrap() = snap.manual_cuts;
            *self.pending_pinned.lock().unwrap() = snap.pinned;
            self.engine.lock().unwrap().reset_buffer();
            true
        } else {
            false
        }
    }

    fn redo(&self) -> bool {
        self.set_last_western_autocorrect(None);
        let snap_opt = self.redo_stack.lock().unwrap().pop_back();
        if let Some(snap) = snap_opt {
            let current = self.take_snapshot();
            self.undo_stack.lock().unwrap().push_back(current);

            *self.pending_compose.lock().unwrap() = snap.pending;
            *self.sentence_cursor.lock().unwrap() = snap.cursor;
            *self.sel_anchor.lock().unwrap() = snap.anchor;
            *self.pending_syllables.lock().unwrap() = snap.syllables;
            *self.pending_segments.lock().unwrap() = snap.segments;
            *self.manual_cuts.lock().unwrap() = snap.manual_cuts;
            *self.pending_pinned.lock().unwrap() = snap.pinned;
            self.engine.lock().unwrap().reset_buffer();
            true
        } else {
            false
        }
    }

    fn insert_pending_char(&self, ch: char, syl: Option<String>) {
        self.push_undo();
        let mut pending = self.pending_compose.lock().unwrap();
        let mut cur = self.sentence_cursor.lock().unwrap();
        let mut chars: Vec<char> = pending.chars().collect();
        let insert_pos = (*cur).min(chars.len());
        chars.insert(insert_pos, ch);
        *cur = insert_pos + 1;
        *pending = chars.iter().collect();

        let syls = self.pending_syllables.lock().unwrap();
        let mut syls = syls;
        let s_pos = insert_pos.min(syls.len());
        syls.insert(s_pos, syl);
        let pinned_snapshot = {
            let mut pinned = self.pending_pinned.lock().unwrap();
            if !pinned.is_empty() {
                *pinned = pinned
                    .iter()
                    .map(|&idx| if idx >= insert_pos { idx + 1 } else { idx })
                    .collect();
            }
            let mut cuts = self.manual_cuts.lock().unwrap();
            if !cuts.is_empty() {
                *cuts = cuts
                    .iter()
                    .map(|&idx| if idx > insert_pos { idx + 1 } else { idx })
                    .collect();
            }
            if ch.is_ascii() {
                pinned.clone()
            } else {
                std::collections::HashSet::new()
            }
        };

        let mut segs = self.pending_segments.lock().unwrap();
        for seg in segs.iter_mut() {
            if seg.0 >= insert_pos {
                seg.0 += 1;
                seg.1 += 1;
            } else if seg.1 > insert_pos {
                seg.1 += 1;
            }
        }
        if ch.is_ascii() {
            if upgrade_an_before_ascii_key_in_buffers(&mut chars, &syls, &segs, &pinned_snapshot, *cur) {
                *pending = chars.iter().collect();
                for seg in segs.iter_mut() {
                    if seg.1.saturating_sub(seg.0) == 1 && chars.get(seg.0) == Some(&'按') && seg.2 != "按" {
                        seg.2 = "按".to_string();
                    }
                }
            }
        }
    }

    fn splice_pending_chunk(&self, chunk_chars: &[char], chunk_syls: &[Option<String>], chunk_segs: &[(usize, usize, String, String)]) {
        if chunk_chars.is_empty() {
            return;
        }
        self.push_undo();
        let count = chunk_chars.len();
        let mut pending = self.pending_compose.lock().unwrap();
        let mut cur = self.sentence_cursor.lock().unwrap();
        let mut chars: Vec<char> = pending.chars().collect();
        let insert_pos = (*cur).min(chars.len());
        chars.splice(insert_pos..insert_pos, chunk_chars.iter().copied());
        *cur = insert_pos + count;
        *pending = chars.iter().collect();

        let has_ascii = chunk_chars.iter().any(|c| c.is_ascii());
        let mut syls = self.pending_syllables.lock().unwrap();
        let s_pos = insert_pos.min(syls.len());
        syls.splice(s_pos..s_pos, chunk_syls.iter().cloned());
        let pinned_snapshot = {
            let mut pinned = self.pending_pinned.lock().unwrap();
            if !pinned.is_empty() {
                *pinned = pinned
                    .iter()
                    .map(|&idx| if idx >= insert_pos { idx + count } else { idx })
                    .collect();
            }
            let mut cuts = self.manual_cuts.lock().unwrap();
            if !cuts.is_empty() {
                *cuts = cuts
                    .iter()
                    .map(|&idx| if idx > insert_pos { idx + count } else { idx })
                    .collect();
            }
            if has_ascii {
                pinned.clone()
            } else {
                std::collections::HashSet::new()
            }
        };

        let mut segs = self.pending_segments.lock().unwrap();
        for seg in segs.iter_mut() {
            if seg.0 >= insert_pos {
                seg.0 += count;
                seg.1 += count;
            }
        }
        for seg in chunk_segs {
            segs.push((insert_pos + seg.0, insert_pos + seg.1, seg.2.clone(), seg.3.clone()));
        }
        if has_ascii {
            if upgrade_an_before_ascii_key_in_buffers(&mut chars, &syls, &segs, &pinned_snapshot, *cur) {
                *pending = chars.iter().collect();
                for seg in segs.iter_mut() {
                    if seg.1.saturating_sub(seg.0) == 1 && chars.get(seg.0) == Some(&'按') && seg.2 != "按" {
                        seg.2 = "按".to_string();
                    }
                }
            }
        }
    }

    pub(crate) fn wrap_pending_range_with_pair(&self, start: usize, end: usize, open: char, close: char) {
        let valid_bounds = {
            let pending = self.pending_compose.lock().unwrap();
            let len = pending.chars().count();
            start < end && end <= len
        };
        if !valid_bounds {
            return;
        }
        self.push_undo();
        {
            let mut pending = self.pending_compose.lock().unwrap();
            let mut chars: Vec<char> = pending.chars().collect();
            if end <= chars.len() && start < end {
                chars.insert(end, close);
                chars.insert(start, open);
                *pending = chars.iter().collect();
            }
        }
        {
            let mut syls = self.pending_syllables.lock().unwrap();
            let s_end = end.min(syls.len());
            let s_start = start.min(s_end);
            syls.insert(s_end, None);
            syls.insert(s_start, None);
        }
        {
            let mut pinned = self.pending_pinned.lock().unwrap();
            if !pinned.is_empty() {
                *pinned = pinned
                    .iter()
                    .map(|&idx| {
                        if idx >= end {
                            idx + 2
                        } else if idx >= start {
                            idx + 1
                        } else {
                            idx
                        }
                    })
                    .collect();
            }
            pinned.insert(start);
            pinned.insert(end + 1);
            let mut cuts = self.manual_cuts.lock().unwrap();
            if !cuts.is_empty() {
                *cuts = cuts
                    .iter()
                    .map(|&idx| {
                        if idx > end {
                            idx + 2
                        } else if idx > start {
                            idx + 1
                        } else {
                            idx
                        }
                    })
                    .collect();
            }
        }
        let mut segs = self.pending_segments.lock().unwrap();
        for seg in segs.iter_mut() {
            seg.0 = if seg.0 >= end {
                seg.0 + 2
            } else if seg.0 >= start {
                seg.0 + 1
            } else {
                seg.0
            };
            seg.1 = if seg.1 > end {
                seg.1 + 2
            } else if seg.1 > start {
                seg.1 + 1
            } else {
                seg.1
            };
        }
    }

    /// 將符號樹選取之符號寫入 pending_compose：
    /// 回傳 true 表示應由呼叫端直接 commit 至目標視窗（無預編區且非成對括號），回傳 false 表示留駐組字區並以 sync_input_display 呈現
    pub(crate) fn commit_symbol_into_pending(&self, symbol: &str, is_wrap_pair: bool) -> bool {
        let chars_len = self.pending_compose.lock().unwrap().chars().count();
        let cur = (*self.sentence_cursor.lock().unwrap()).min(chars_len);
        let sel_range = self.get_manual_selection_range();
        let sym_chars: Vec<char> = symbol.chars().collect();

        if let Some((start, end)) = sel_range {
            if start < end && end <= chars_len {
                if is_wrap_pair && sym_chars.len() == 2 {
                    self.wrap_pending_range_with_pair(start, end, sym_chars[0], sym_chars[1]);
                    *self.sel_anchor.lock().unwrap() = None;
                    *self.sentence_cursor.lock().unwrap() = end + 2;
                } else {
                    self.replace_pending_range(start, end, &sym_chars, None);
                    *self.sel_anchor.lock().unwrap() = None;
                    *self.sentence_cursor.lock().unwrap() = start + sym_chars.len();
                }
                return false;
            }
        }

        if is_wrap_pair && sym_chars.len() == 2 {
            self.replace_pending_range(cur, cur, &sym_chars, None);
            *self.sel_anchor.lock().unwrap() = None;
            *self.sentence_cursor.lock().unwrap() = cur + 1;
            return false;
        }

        if chars_len > 0 {
            self.replace_pending_range(cur, cur, &sym_chars, None);
            *self.sel_anchor.lock().unwrap() = None;
            *self.sentence_cursor.lock().unwrap() = cur + sym_chars.len();
            return false;
        }

        true
    }

    fn handle_symbol_tree_key_down(&self, ctx: &ITfContext, vk: u32, is_ctrl: bool, is_shift: bool) -> Result<BOOL> {
        if matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0..=0xA5) || is_ctrl {
            return Ok(BOOL::from(false));
        }
        let Some(key_str) = symbol_tree_vk_to_key_str(vk, is_shift) else {
            return Ok(BOOL::from(false));
        };
        let step = {
            let mut eng = self.engine.lock().unwrap();
            eng.step_symbol_tree_key(&key_str)
        };
        match step {
            ruhi::symbol_tree::SymbolTreeStep::Stay | ruhi::symbol_tree::SymbolTreeStep::Exit => {
                self.sync_input_display(ctx)?;
            }
            ruhi::symbol_tree::SymbolTreeStep::CommitSymbol { symbol, is_wrap_pair } => {
                let direct_commit = self.commit_symbol_into_pending(&symbol, is_wrap_pair);
                if direct_commit {
                    self.hide_candidate_window();
                    let c = symbol.chars().count();
                    self.update_tsf_composition(ctx, &symbol, c, None, true)?;
                } else {
                    self.sync_input_display(ctx)?;
                }
            }
            ruhi::symbol_tree::SymbolTreeStep::InsertStickyBox { symbol, .. } => {
                let rep: Vec<char> = symbol.chars().collect();
                let rep_len = rep.len();
                let cur = {
                    let pending_len = self.pending_compose.lock().unwrap().chars().count();
                    (*self.sentence_cursor.lock().unwrap()).min(pending_len)
                };
                self.replace_pending_range(cur, cur, &rep, None);
                *self.sel_anchor.lock().unwrap() = None;
                *self.sentence_cursor.lock().unwrap() = cur + rep_len;
                self.sync_input_display(ctx)?;
            }
            ruhi::symbol_tree::SymbolTreeStep::DeletePrevStickyBox => {
                let cur = *self.sentence_cursor.lock().unwrap();
                let pending_len = self.pending_compose.lock().unwrap().chars().count();
                if cur > 0 && cur <= pending_len {
                    self.remove_pending_char_at(cur - 1);
                    *self.sentence_cursor.lock().unwrap() = cur - 1;
                }
                self.sync_input_display(ctx)?;
            }
        }
        Ok(BOOL::from(true))
    }

    fn replace_pending_range(&self, start: usize, end: usize, replacement: &[char], new_syl: Option<String>) {
        self.push_undo();
        let mut pending = self.pending_compose.lock().unwrap();
        let mut chars: Vec<char> = pending.chars().collect();
        if start <= chars.len() && end <= chars.len() && start <= end {
            chars.splice(start..end, replacement.iter().copied());
            *pending = chars.iter().collect();
        }

        let rep_len = replacement.len();
        let old_len = end.saturating_sub(start);
        {
            let mut pinned = self.pending_pinned.lock().unwrap();
            let diff = rep_len as isize - old_len as isize;
            if diff != 0 && !pinned.is_empty() {
                *pinned = pinned
                    .iter()
                    .filter(|&&idx| idx < start || idx >= end)
                    .map(|&idx| {
                        if idx >= end {
                            (idx as isize + diff).max(0) as usize
                        } else {
                            idx
                        }
                    })
                    .collect();
            }
            for i in start..(start + rep_len) {
                pinned.insert(i);
            }
            if diff != 0 {
                let mut cuts = self.manual_cuts.lock().unwrap();
                if !cuts.is_empty() {
                    *cuts = cuts
                        .iter()
                        .filter(|&&idx| idx <= start || idx >= end)
                        .map(|&idx| {
                            if idx >= end {
                                (idx as isize + diff).max(0) as usize
                            } else {
                                idx
                            }
                        })
                        .collect();
                }
            }
        }

        let rep_str_check: String = replacement.iter().collect();
        let is_sym_rep = new_syl.is_none()
            && !rep_str_check.is_empty()
            && ruhi::symbol_tree::is_symbol_candidate(&rep_str_check);

        let mut syls = self.pending_syllables.lock().unwrap();
        let s_start = start.min(syls.len());
        let s_end = end.min(syls.len());
        if s_start <= s_end {
            if rep_len == 1 && old_len == 1 {
                if s_start < syls.len() {
                    syls[s_start] = new_syl.clone();
                }
            } else if is_sym_rep {
                let none_syls: Vec<Option<String>> = (0..rep_len).map(|_| None).collect();
                syls.splice(s_start..s_end, none_syls);
            } else if rep_len == old_len && rep_len > 1 {
                // 同長度多字詞替換（同音詞替換）：保留既有對應音節；若有缺漏嘗試自詞典補齊
                let has_all_syls = syls[s_start..s_end].iter().all(|s| s.is_some());
                if !has_all_syls {
                    let rep_str: String = replacement.iter().collect();
                    let eng = self.engine.lock().unwrap();
                    if let Some(entry) = eng.lexicon.word_index.get(&rep_str).and_then(|ids| ids.first().and_then(|&idx| eng.lexicon.entries.get(idx))) {
                        let bpm_parts: Vec<&str> = entry.bopomofo.split_whitespace().collect();
                        if bpm_parts.len() == rep_len {
                            for (i, part) in bpm_parts.iter().enumerate() {
                                syls[s_start + i] = Some(part.to_string());
                            }
                        }
                    }
                }
            } else {
                let rep_str: String = replacement.iter().collect();
                let eng = self.engine.lock().unwrap();
                let mut looked_up: Vec<Option<String>> = Vec::new();
                if let Some(entry) = eng.lexicon.word_index.get(&rep_str).and_then(|ids| ids.first().and_then(|&idx| eng.lexicon.entries.get(idx))) {
                    let bpm_parts: Vec<&str> = entry.bopomofo.split_whitespace().collect();
                    if bpm_parts.len() == rep_len {
                        looked_up = bpm_parts.iter().map(|p| Some(p.to_string())).collect();
                    }
                }
                if looked_up.len() == rep_len {
                    syls.splice(s_start..s_end, looked_up);
                } else {
                    let new_syls: Vec<Option<String>> = (0..rep_len).map(|_| new_syl.clone()).collect();
                    syls.splice(s_start..s_end, new_syls);
                }
            }
        }

        let mut segs = self.pending_segments.lock().unwrap();
        if is_sym_rep {
            let diff = rep_len as isize - old_len as isize;
            if old_len > 0 {
                segs.retain(|s| s.1 <= start || s.0 >= end);
            }
            if diff != 0 {
                for seg in segs.iter_mut() {
                    if seg.0 >= end {
                        seg.0 = (seg.0 as isize + diff).max(0) as usize;
                        seg.1 = (seg.1 as isize + diff).max(0) as usize;
                    } else if seg.1 > start {
                        seg.1 = (seg.1 as isize + diff).max(0) as usize;
                    }
                }
            }
        } else if rep_len == old_len && rep_len > 1 {
            let new_word: String = replacement.iter().collect();
            for seg in segs.iter_mut() {
                if seg.0 == start && seg.1 == end {
                    seg.2 = new_word.clone();
                }
            }
        } else if rep_len != old_len {
            let diff = rep_len as isize - old_len as isize;
            if old_len > 0 {
                segs.retain(|s| !(s.0 >= start && s.1 <= end));
            }
            for seg in segs.iter_mut() {
                if seg.0 >= end {
                    seg.0 = (seg.0 as isize + diff).max(0) as usize;
                    seg.1 = (seg.1 as isize + diff).max(0) as usize;
                } else if seg.1 > start {
                    seg.1 = (seg.1 as isize + diff).max(0) as usize;
                }
            }
        } else if rep_len == 1 && old_len == 1 {
            // 單字替換：若修改了多字詞中的單字，該多字詞語意斷裂，將該 segment 拆分為單字區間
            let mut to_split = Vec::new();
            for (idx, seg) in segs.iter().enumerate() {
                if start >= seg.0 && start < seg.1 && (seg.1 - seg.0) > 1 {
                    to_split.push(idx);
                }
            }
            if !to_split.is_empty() {
                for idx in to_split.into_iter().rev() {
                    let seg = segs.remove(idx);
                    for i in seg.0..seg.1 {
                        let ch = chars.get(i).copied().unwrap_or(' ');
                        let ch_syl = syls.get(i).cloned().flatten().unwrap_or_default();
                        segs.push((i, i + 1, ch.to_string(), ch_syl));
                    }
                }
                segs.sort_by_key(|s| s.0);
            } else {
                for seg in segs.iter_mut() {
                    if start >= seg.0 && start < seg.1 {
                        seg.2 = replacement[0].to_string();
                    }
                }
            }
        }
    }

    fn remove_pending_char_at(&self, idx: usize) {
        self.push_undo();
        let mut pending = self.pending_compose.lock().unwrap();
        let mut chars: Vec<char> = pending.chars().collect();
        if idx < chars.len() {
            chars.remove(idx);
            *pending = chars.into_iter().collect();
        }

        let mut syls = self.pending_syllables.lock().unwrap();
        if idx < syls.len() {
            syls.remove(idx);
        }

        let mut segs = self.pending_segments.lock().unwrap();
        segs.retain(|s| !(s.0 <= idx && idx < s.1 && (s.1 - s.0) <= 1));
        {
            let mut pinned = self.pending_pinned.lock().unwrap();
            if !pinned.is_empty() {
                *pinned = pinned
                    .iter()
                    .filter(|&&p| p != idx)
                    .map(|&p| if p > idx { p - 1 } else { p })
                    .collect();
            }
            let mut cuts = self.manual_cuts.lock().unwrap();
            if !cuts.is_empty() {
                *cuts = cuts
                    .iter()
                    .filter(|&&c| c != idx + 1)
                    .map(|&c| if c > idx { c - 1 } else { c })
                    .filter(|&c| c > 0)
                    .collect();
            }
        }
        for seg in segs.iter_mut() {
            if seg.0 > idx {
                seg.0 = seg.0.saturating_sub(1);
                seg.1 = seg.1.saturating_sub(1);
            } else if seg.1 > idx {
                seg.1 = seg.1.saturating_sub(1);
            }
        }
    }

    fn clear_all_pending(&self) {
        self.pending_compose.lock().unwrap().clear();
        self.pending_syllables.lock().unwrap().clear();
        self.pending_segments.lock().unwrap().clear();
        self.manual_cuts.lock().unwrap().clear();
        self.pending_pinned.lock().unwrap().clear();
        *self.last_raw_ascii.lock().unwrap() = None;
        *self.sel_anchor.lock().unwrap() = None;
        *self.candidate_selecting.lock().unwrap() = false;
        self.active_candidates.lock().unwrap().clear();
        *self.target_range.lock().unwrap() = (0, 0);
        if let Ok(eng) = self.engine.lock() {
            eng.set_last_fused_longkan(None);
            eng.set_shift_down_longkan_popup(false);
            eng.set_backslash_escape_pending(false);
        }
        let inst_key = self.western_instance_key();
        clear_western_leading_sigil(inst_key);
        clear_western_undo_pins(inst_key);
        set_tab_range_popup_active(inst_key, false);
        self.set_last_western_autocorrect(None);
    }

    pub(crate) fn apply_candidate_selection_in_desk(&self, chosen: &str, cands: &[String]) {
        let target_range = *self.target_range.lock().unwrap();
        let target_idx = *self.target_char_idx.lock().unwrap();
        let (full_text, _) = self.get_full_display_and_cursor();
        let chars: Vec<char> = full_text.chars().collect();

        let target_len = target_range.1.saturating_sub(target_range.0);
        let chosen_len = chosen.chars().count();
        let is_shift_down_lk = self
            .engine
            .lock()
            .map(|e| e.is_shift_down_longkan_popup())
            .unwrap_or(false);
        let is_sym_cand = ruhi::symbol_tree::is_symbol_candidate(chosen);
        let is_tab_range_popup = take_tab_range_popup_active(self.western_instance_key());

        let has_active_shape_buf = self
            .engine
            .lock()
            .map(|e| {
                e.cangjie.has_buffer()
                    || e.array30.has_buffer()
                    || e.four_corner.has_buffer()
            })
            .unwrap_or(false);
        let pending_char_len = self.pending_compose.lock().unwrap().chars().count();
        let is_zero_len_assoc = target_range.0 == target_range.1
            && ((target_range.0 > 0 && target_range.1 <= chars.len())
                || (has_active_shape_buf && target_range.1 <= pending_char_len));
        let (replace_start, replace_end) = if is_zero_len_assoc {
            (target_range.0, target_range.1)
        } else if target_len == 1 && chosen_len > 1 && !is_sym_cand && !is_tab_range_popup {
            let seg_match = {
                let segs = self.pending_segments.lock().unwrap();
                segs.iter()
                    .find(|s| target_idx >= s.0 && target_idx < s.1 && (s.1 - s.0) == chosen_len && s.1 <= chars.len())
                    .map(|s| (s.0, s.1))
            };
            if let Some((s0, s1)) = seg_match {
                (s0, s1)
            } else if target_range.1 <= chars.len() {
                (target_range.0, target_range.1)
            } else if target_idx < chars.len() {
                (target_idx, target_idx + 1)
            } else {
                (chars.len(), chars.len())
            }
        } else if target_len >= 1
            && target_range.1 <= chars.len()
            && (chosen_len == target_len
                || chosen_len > 1
                || target_len == 1
                || is_shift_down_lk
                || is_sym_cand
                || is_tab_range_popup
                || self.get_manual_selection_range().is_some())
        {
            (target_range.0, target_range.1)
        } else if target_idx < chars.len() {
            (target_idx, target_idx + 1)
        } else {
            (chars.len(), chars.len())
        };

        let rejected: String = if replace_start < replace_end && replace_end <= chars.len() {
            chars[replace_start..replace_end].iter().collect()
        } else {
            String::new()
        };
        let prev_context: String = if replace_start > 0 {
            let p_start = replace_start.saturating_sub(2);
            chars[p_start..replace_start].iter().collect()
        } else {
            String::new()
        };
        let next_context: String = if replace_end < chars.len() {
            let n_end = (replace_end + 2).min(chars.len());
            chars[replace_end..n_end].iter().collect()
        } else {
            String::new()
        };
        let mut char_syl = self.pending_syllables.lock().unwrap().get(target_idx).cloned().flatten();
        if (is_shift_down_lk || replace_end.saturating_sub(replace_start) != chosen_len) && chosen_len == 1 {
            if let Ok(eng) = self.engine.lock() {
                let should_update_bpm = match &char_syl {
                    Some(existing_syl) => {
                        let norm_existing = ruhi::lexicon::LexiconEntry::normalize_bpm(existing_syl);
                        let has_matching = eng.lexicon.word_index.get(chosen).map_or(false, |ids| {
                            ids.iter().any(|&eid| {
                                eng.lexicon.entries.get(eid).map_or(false, |e| {
                                    ruhi::lexicon::LexiconEntry::normalize_bpm(&e.bopomofo) == norm_existing
                                })
                            })
                        });
                        !has_matching
                    }
                    None => true,
                };
                if should_update_bpm {
                    if let Some(found_bpm) = eng
                        .lexicon
                        .word_index
                        .get(chosen)
                        .and_then(|ids| {
                            ids.iter()
                                .filter_map(|&eid| eng.lexicon.entries.get(eid))
                                .max_by(|a, b| {
                                    a.weight
                                        .partial_cmp(&b.weight)
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                })
                        })
                        .map(|e| e.bopomofo.clone())
                    {
                        char_syl = Some(found_bpm);
                    }
                }
            }
        }
        let is_wrap_pair = ruhi::symbol_tree::is_paired_wrap_symbol(chosen);
        let treat_as_sym = is_sym_cand || is_tab_range_popup;
        let eff_char_syl = if treat_as_sym { None } else { char_syl };
        let bpm_str = eff_char_syl.clone().unwrap_or_default();

        if !treat_as_sym && !rejected.is_empty() && rejected != *chosen {
            let mut eng = self.engine.lock().unwrap();
            eng.record_revision(&bpm_str, &rejected, chosen, &prev_context, &next_context);
        }

        let replacement: Vec<char> = chosen.chars().collect();
        self.replace_pending_range(replace_start, replace_end, &replacement, eff_char_syl);
        {
            let mut eng = self.engine.lock().unwrap();
            eng.reset_buffer();
            if is_shift_down_lk && chosen_len == 1 {
                eng.set_last_fused_longkan(Some((replace_start, cands.to_vec())));
            }
        }
        *self.sentence_cursor.lock().unwrap() = if is_wrap_pair && chosen_len == 2 {
            replace_start + 1
        } else {
            replace_start + chosen_len
        };

        *self.sel_anchor.lock().unwrap() = None;
        *self.candidate_selecting.lock().unwrap() = false;
    }

    fn western_instance_key(&self) -> usize {
        std::sync::Arc::as_ptr(&self.client_id) as usize
    }

    fn set_last_western_autocorrect(&self, rollback: Option<WesternSpaceRollback>) {
        let map_mutex = INSTANCE_LAST_WESTERN_AUTOCORRECT
            .get_or_init(|| Mutex::new(std::collections::HashMap::new()));
        if let Ok(mut map) = map_mutex.lock() {
            let key = self.western_instance_key();
            if let Some(rb) = rollback {
                map.insert(key, rb);
            } else {
                map.remove(&key);
            }
        }
    }

    fn take_last_western_autocorrect_if_matching(&self) -> Option<WesternSpaceRollback> {
        let map_mutex = INSTANCE_LAST_WESTERN_AUTOCORRECT.get()?;
        let mut map = map_mutex.lock().ok()?;
        let rb = map.remove(&self.western_instance_key())?;
        let cur_len = self.pending_compose.lock().unwrap().chars().count();
        let cur_pos = *self.sentence_cursor.lock().unwrap();
        if cur_len == rb.corrected_text_len && cur_pos == rb.corrected_cursor {
            Some(rb)
        } else {
            None
        }
    }

    fn restore_western_space_rollback(&self, rb: WesternSpaceRollback) {
        let mut snap = rb.pre_space_snapshot;
        for idx in rb.pin_range.0..rb.pin_range.1 {
            snap.pinned.insert(idx);
        }
        *self.pending_compose.lock().unwrap() = snap.pending;
        *self.sentence_cursor.lock().unwrap() = snap.cursor;
        *self.sel_anchor.lock().unwrap() = snap.anchor;
        *self.pending_syllables.lock().unwrap() = snap.syllables;
        *self.pending_segments.lock().unwrap() = snap.segments;
        *self.manual_cuts.lock().unwrap() = snap.manual_cuts;
        *self.pending_pinned.lock().unwrap() = snap.pinned;
        self.engine.lock().unwrap().reset_buffer();
    }

    /// 歐文模式下按空白鍵執行高確定性 Tier 1 自動勘誤並原子化寫入復原快照（單次 Ctrl+Z 或 Backspace 即可一鍵還原）
    fn try_western_space_autocorrect_and_insert(&self, space_ch: char) -> bool {
        let inst_key = self.western_instance_key();
        let leading_sigil = take_western_leading_sigil(inst_key);
        if space_ch != ' ' {
            return false;
        }
        let pre_snap = self.take_snapshot();
        let chars: Vec<char> = pre_snap.pending.chars().collect();
        let cur = pre_snap.cursor.min(chars.len());
        if chars.is_empty() || cur == 0 {
            return false;
        }

        let before_cur = &chars[..cur];
        let mut tok_start = before_cur.len();
        while tok_start > 0
            && (before_cur[tok_start - 1].is_ascii_alphabetic()
                || ruhi::engine::RuhiEngine::is_western_non_ascii_letter_or_mark(
                    before_cur[tok_start - 1],
                ))
        {
            tok_start -= 1;
        }
        if tok_start >= cur {
            return false;
        }
        if tok_start > 0
            && (before_cur[tok_start - 1].is_alphanumeric()
                || ruhi::engine::RuhiEngine::is_western_token_boundary_blocker(
                    before_cur[tok_start - 1],
                ))
        {
            return false;
        }

        let mut prefix_before_token: String = before_cur[..tok_start].iter().collect();
        if tok_start == 0 {
            if let Some(sigil) = leading_sigil {
                if ruhi::engine::RuhiEngine::is_western_token_boundary_blocker(sigil) {
                    return false;
                }
                prefix_before_token.push(sigil);
            }
        }
        let token: String = before_cur[tok_start..].iter().collect();
        let ac_opt = {
            let eng = self.engine.lock().unwrap();
            eng.western
                .evaluate_space_autocorrect(&prefix_before_token, &token)
        };
        let Some(ac) = ac_opt else {
            return false;
        };

        let rep_start = cur.saturating_sub(ac.replace_chars);
        if (rep_start..cur).any(|idx| pre_snap.pinned.contains(&idx)) {
            return false;
        }

        // 原子化推入單一 Undo 快照，讓一次 Ctrl+Z 直接還原為勘誤前狀態
        {
            let mut undo = self.undo_stack.lock().unwrap();
            prune_western_undo_pins_above(inst_key, undo.len() + 1);
            undo.push_back(pre_snap.clone());
            if undo.len() > 100 {
                undo.pop_front();
                shift_western_undo_pins_after_cap_pop(inst_key);
            }
            record_western_undo_pin(inst_key, undo.len(), rep_start, cur);
            self.redo_stack.lock().unwrap().clear();
        }

        let rep_chars: Vec<char> = ac.replacement.chars().collect();
        let rep_len = rep_chars.len();
        let old_len = cur.saturating_sub(rep_start);
        let diff = (rep_len + 1) as isize - old_len as isize;

        let new_len = {
            let mut pending = self.pending_compose.lock().unwrap();
            let mut buf: Vec<char> = pending.chars().collect();
            let mut rep_with_space = rep_chars.clone();
            rep_with_space.push(space_ch);
            buf.splice(rep_start..cur, rep_with_space);
            *pending = buf.iter().collect();
            buf.len()
        };

        let new_cur = rep_start + rep_len + 1;
        *self.sentence_cursor.lock().unwrap() = new_cur;

        {
            let mut syls = self.pending_syllables.lock().unwrap();
            let s_start = rep_start.min(syls.len());
            let s_end = cur.min(syls.len());
            let empty_syls: Vec<Option<String>> = vec![None; rep_len + 1];
            syls.splice(s_start..s_end, empty_syls);
        }

        {
            let mut pinned = self.pending_pinned.lock().unwrap();
            if diff != 0 && !pinned.is_empty() {
                *pinned = pinned
                    .iter()
                    .filter(|&&idx| idx < rep_start || idx >= cur)
                    .map(|&idx| {
                        if idx >= cur {
                            (idx as isize + diff).max(0) as usize
                        } else {
                            idx
                        }
                    })
                    .collect();
            }
            for i in rep_start..(rep_start + rep_len) {
                pinned.insert(i);
            }
        }

        if diff != 0 {
            let mut cuts = self.manual_cuts.lock().unwrap();
            if !cuts.is_empty() {
                *cuts = cuts
                    .iter()
                    .filter(|&&idx| idx <= rep_start || idx >= cur)
                    .map(|&idx| {
                        if idx >= cur {
                            (idx as isize + diff).max(0) as usize
                        } else {
                            idx
                        }
                    })
                    .collect();
            }
        }

        self.set_last_western_autocorrect(Some(WesternSpaceRollback {
            pre_space_snapshot: pre_snap,
            corrected_text_len: new_len,
            corrected_cursor: new_cur,
            pin_range: (tok_start, cur),
        }));
        true
    }

    fn consolidate_engine_to_pending(&self) {
        let engine = self.engine.lock().unwrap();
        let saved_fused = engine.get_last_fused_longkan();
        let is_kr_chunk = engine.korean.has_buffer();
        let is_fc_chunk = engine.four_corner.has_buffer();
        let is_lk_chunk = engine.longkan.has_buffer();
        let has_bs_pending = !engine.is_english_keyboard_mode()
            && !engine.is_latin_mode
            && engine.is_backslash_escape_pending();
        let has_composed = !engine.composed_sentence.is_empty() || is_kr_chunk || is_fc_chunk || is_lk_chunk;
        let has_pending = engine.fsm.has_pending();
        if has_composed || has_pending || has_bs_pending {
            let composed_for_ctx = if is_kr_chunk {
                engine.korean.finalized_commit_text()
            } else if is_fc_chunk {
                if (engine.four_corner.buffer == "-" || engine.four_corner.buffer == ".")
                    && !engine.four_corner.is_selecting_candidate
                {
                    String::new()
                } else {
                    engine
                        .four_corner
                        .candidates
                        .get(engine.four_corner.candidate_idx)
                        .cloned()
                        .unwrap_or_default()
                }
            } else if is_lk_chunk {
                let lk_chosen = if engine.longkan.buffer == "-" {
                    String::new()
                } else {
                    engine
                        .longkan
                        .candidates
                        .get(engine.longkan.candidate_idx)
                        .cloned()
                        .unwrap_or_default()
                };
                if engine.inline_longkan_active && !engine.composed_sentence.is_empty() {
                    format!("{}{}", engine.composed_sentence, lk_chosen)
                } else {
                    lk_chosen
                }
            } else {
                engine.composed_sentence.clone()
            };
            let is_jp_chunk = !engine.japanese.raw_romaji.is_empty();
            let raw_ascii = if is_kr_chunk {
                engine.korean.raw_romaja.clone()
            } else if is_jp_chunk {
                engine.japanese.raw_romaji.clone()
            } else if !engine.cangjie.buffer.is_empty() {
                engine.cangjie.buffer.clone()
            } else if !engine.array30.buffer.is_empty() {
                engine.array30.buffer.clone()
            } else if !engine.four_corner.buffer.is_empty() {
                engine.four_corner.buffer.clone()
            } else if !engine.longkan.buffer.is_empty() {
                engine.longkan.buffer.clone()
            } else {
                engine.raw_ascii_buffer.clone()
            };
            let mut eng_chars: Vec<char> = composed_for_ctx.chars().collect();
            let eng_count = eng_chars.len();
            let mut eng_syls: Vec<Option<String>> = (0..eng_count)
                .map(|i| engine.get_syllable_at(i))
                .collect();
            let eng_pinned: Vec<usize> = engine.user_pinned.keys().copied().collect();
            let eng_cuts: Vec<usize> = engine.manual_cuts.iter().copied().collect();
            let mut eng_segs: Vec<(usize, usize, String, String)> = engine.segments.iter()
                .map(|n| (n.start_idx, n.end_idx, n.word.clone(), n.bopomofo.clone()))
                .collect();
            if (is_jp_chunk || is_kr_chunk || (is_fc_chunk && eng_count > 1)) && eng_segs.is_empty() && eng_count > 0 {
                eng_segs.push((0, eng_count, composed_for_ctx.clone(), raw_ascii.clone()));
            }
            if has_pending {
                let pending_bpmf = format!(
                    "{}{}{}",
                    engine.fsm.cur_initial,
                    engine.fsm.cur_medial,
                    engine.fsm.cur_final
                );
                let pending_chars: Vec<char> = pending_bpmf.chars().collect();
                for (idx, ch) in pending_chars.iter().enumerate() {
                    eng_chars.push(*ch);
                    if idx == 0 && pending_chars.len() == 1 {
                        eng_syls.push(Some(pending_bpmf.clone()));
                    } else {
                        eng_syls.push(Some(ch.to_string()));
                    }
                }
            }
            if has_bs_pending && !has_pending {
                let bs_ch = if engine.halfwidth_punct_mode == "halfwidth" && !engine.fullwidth_mode {
                    '\\'
                } else {
                    '＼'
                };
                eng_chars.push(bs_ch);
                eng_syls.push(None);
            }
            drop(engine);

            let cur_before = *self.sentence_cursor.lock().unwrap();
            self.splice_pending_chunk(&eng_chars, &eng_syls, &eng_segs);
            if !raw_ascii.is_empty() && !eng_chars.is_empty() {
                *self.last_raw_ascii.lock().unwrap() = Some((cur_before, cur_before + eng_chars.len(), raw_ascii));
            }

            let mut eng = self.engine.lock().unwrap();
            if !composed_for_ctx.is_empty() {
                eng.record_committed(&composed_for_ctx);
            }
            eng.reset_buffer();
            {
                let mut pinned = self.pending_pinned.lock().unwrap();
                for p in eng_pinned {
                    if p < eng_count {
                        pinned.insert(cur_before + p);
                    }
                }
                let mut cuts = self.manual_cuts.lock().unwrap();
                for c in eng_cuts {
                    if c > 0 && c < eng_count {
                        cuts.insert(cur_before + c);
                    }
                }
            }
            eng.set_last_fused_longkan(saved_fused);
            drop(eng);
        }
    }

    /// 當使用者在未上屏長句中移動游標至句中插入／修改音節（或分段輸入）時，
    /// 將 `pending_syllables[block_start..cur]` + `engine.fsm.syllables` + `pending_syllables[cur..block_end]`
    /// 視為同一個連續漢字音節區間進行聯合 Viterbi 求解（同時嚴格保留 `pending_pinned` 與 `engine.user_pinned` 手動選字鎖定），
    /// 解決句中補字（如於「基隆｜河一路13號」插入「市」➔「基隆市和一路13號」，而非孤立解碼為「是」）的問題。
    fn re_evaluate_joint_pending_and_engine(&self) {
        let p_str = self
            .pending_compose
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut p_chars: Vec<char> = p_str.chars().collect();
        let p_syls = self
            .pending_syllables
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let p_len = p_chars.len();
        if p_len == 0 || p_syls.len() != p_len {
            return;
        }
        let cur = (*self
            .sentence_cursor
            .lock()
            .unwrap_or_else(|e| e.into_inner()))
        .min(p_len);
        // 嚴守鎖順序：先在鎖定 engine 前快照所有介面狀態集合，杜絕 engine 與輔助 Mutex 互鎖反轉死結
        let pending_segs_snapshot = self
            .pending_segments
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let cuts_snapshot = self
            .manual_cuts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let pinned_snapshot = self
            .pending_pinned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let is_non_bpm_mode = matches!(
            engine.keyboard_mode.as_str(),
            "japanese"
                | "jp"
                | "romaji"
                | "korean"
                | "ko"
                | "hangul"
                | "romaja"
                | "cangjie"
                | "cj"
                | "array30"
                | "ar30"
                | "four_corner"
                | "4corner"
                | "fc"
                | "longkan"
                | "lk"
        ) || engine.inline_longkan_active
            || engine.is_latin_mode
            || matches!(
                engine.mode,
                ruhi::engine::EngineMode::QuickNum
                    | ruhi::engine::EngineMode::TemplateSelect
                    | ruhi::engine::EngineMode::LexiconSwitch
                    | ruhi::engine::EngineMode::SymbolTree
            );
        if is_non_bpm_mode {
            return;
        }

        let eng_syls = engine.fsm.syllables.clone();
        let eng_len = eng_syls.len();
        if eng_len != engine.sentence_chars.len() {
            return;
        }

        let mut block_start = cur;
        while block_start > 0 {
            if let Some(ref bpm) = p_syls[block_start - 1] {
                if HsuSyllable::from_bopomofo_display(bpm).is_some() {
                    block_start -= 1;
                    continue;
                }
            }
            break;
        }
        let mut block_end = cur;
        while block_end < p_len {
            if let Some(ref bpm) = p_syls[block_end] {
                if HsuSyllable::from_bopomofo_display(bpm).is_some() {
                    block_end += 1;
                    continue;
                }
            }
            break;
        }

        const JOINT_REEVAL_MAX_LEFT: usize = 28;
        const JOINT_REEVAL_MAX_RIGHT: usize = 20;
        let (win_start, win_end) = compute_joint_reeval_window(
            block_start,
            cur,
            eng_len,
            block_end,
            &pending_segs_snapshot,
            JOINT_REEVAL_MAX_LEFT,
            JOINT_REEVAL_MAX_RIGHT,
        );

        let left_len = cur - win_start;
        let right_len = win_end - cur;
        if left_len == 0 && right_len == 0 && win_start == 0 && eng_len == 0 {
            return;
        }
        let total_len = left_len + eng_len + right_len;
        if total_len == 0 || (total_len < 2 && win_start == 0 && eng_len == 0) {
            return;
        }

        let mut joint_syls = Vec::with_capacity(total_len);
        for i in win_start..cur {
            if let Some(ref bpm) = p_syls[i] {
                if let Some(hs) = HsuSyllable::from_bopomofo_display(bpm) {
                    joint_syls.push(hs);
                } else {
                    return;
                }
            } else {
                return;
            }
        }
        for hs in &eng_syls {
            joint_syls.push(hs.clone());
        }
        for i in cur..win_end {
            if let Some(ref bpm) = p_syls[i] {
                if let Some(hs) = HsuSyllable::from_bopomofo_display(bpm) {
                    joint_syls.push(hs);
                } else {
                    return;
                }
            } else {
                return;
            }
        }

        let mut joint_cuts = std::collections::HashSet::new();
        for &c in &cuts_snapshot {
            if c > win_start && c <= cur {
                if !(c == cur && eng_len > 0) {
                    joint_cuts.insert(c - win_start);
                }
            } else if c > cur && c < win_end {
                joint_cuts.insert(c - win_start + eng_len);
            }
        }
        for &c in &engine.manual_cuts {
            if c > 0 && c < eng_len {
                joint_cuts.insert(left_len + c);
            }
        }

        let mut joint_pinned: std::collections::HashMap<usize, char> = std::collections::HashMap::new();
        for i in win_start..cur {
            if pinned_snapshot.contains(&i) {
                joint_pinned.insert(i - win_start, p_chars[i]);
            }
        }
        for (&idx, pinned_w) in &engine.user_pinned {
            if idx < eng_len {
                if let Some(ch) = pinned_w.chars().next() {
                    joint_pinned.insert(left_len + idx, ch);
                }
            }
        }
        for i in cur..win_end {
            if pinned_snapshot.contains(&i) {
                joint_pinned.insert(left_len + eng_len + (i - cur), p_chars[i]);
            }
        }

        let left_pending_slice: String = if win_start > 0 {
            p_chars[..win_start].iter().collect()
        } else {
            String::new()
        };
        let prefix_str: String = engine
            .compute_joint_reeval_prefix(&left_pending_slice)
            .unwrap_or_default();
        let prefix_opt = if prefix_str.is_empty() {
            None
        } else {
            Some(prefix_str.as_str())
        };

        let nodes = {
            let RuhiEngine {
                ref router,
                ref mut transitions,
                ref lexicon,
                ..
            } = *engine;
            let dag_builder = LatticeDAG::new(lexicon);
            let mut dag = dag_builder.build_dag_with_cuts(&joint_syls, true, &joint_cuts);
            if !joint_pinned.is_empty() {
                for node_list in dag.values_mut() {
                    let matching_count = node_list
                        .iter()
                        .filter(|n| {
                            let n_chars: Vec<char> = n.word.chars().collect();
                            (n.start_idx..n.end_idx).all(|pos| {
                                if let Some(&p_ch) = joint_pinned.get(&pos) {
                                    n_chars.get(pos - n.start_idx) == Some(&p_ch)
                                } else {
                                    true
                                }
                            })
                        })
                        .count();
                    if matching_count > 0 {
                        node_list.retain(|n| {
                            let n_chars: Vec<char> = n.word.chars().collect();
                            (n.start_idx..n.end_idx).all(|pos| {
                                if let Some(&p_ch) = joint_pinned.get(&pos) {
                                    n_chars.get(pos - n.start_idx) == Some(&p_ch)
                                } else {
                                    true
                                }
                            })
                        });
                    }
                }
            }
            let (solved_nodes, _) =
                router.solve_best_path_with_prefix(total_len, &dag, Some(transitions), prefix_opt);
            solved_nodes
        };

        if nodes.is_empty() {
            return;
        }
        let solved_text: String = nodes.iter().map(|n| n.word.as_str()).collect();
        let mut solved_chars: Vec<char> = solved_text.chars().collect();
        if solved_chars.len() != total_len {
            return;
        }
        for (&pos, &p_ch) in &joint_pinned {
            if pos < solved_chars.len() {
                solved_chars[pos] = p_ch;
            }
        }

        for i in win_start..cur {
            p_chars[i] = solved_chars[i - win_start];
        }
        for i in cur..win_end {
            p_chars[i] = solved_chars[left_len + eng_len + (i - cur)];
        }
        let updated_pending: String = p_chars.iter().collect();

        if eng_len > 0 {
            engine.sentence_chars = solved_chars[left_len..(left_len + eng_len)]
                .iter()
                .map(|c| c.to_string())
                .collect();
            engine.composed_sentence = engine.sentence_chars.join("");
            engine.viterbi_raw_sentence = engine.composed_sentence.clone();
            let mut eng_nodes = Vec::new();
            for n in &nodes {
                let s = n.start_idx.max(left_len);
                let e = n.end_idx.min(left_len + eng_len);
                if s < e {
                    let sub_w: String = solved_chars[s..e].iter().collect();
                    let sub_bpm: String = joint_syls[s..e]
                        .iter()
                        .map(|hs| hs.bopomofo_display())
                        .collect::<Vec<_>>()
                        .join(" ");
                    eng_nodes.push(ruhi::router::LatticeNode {
                        start_idx: s - left_len,
                        end_idx: e - left_len,
                        word: sub_w,
                        bopomofo: sub_bpm,
                        weight: n.weight,
                    });
                }
            }
            engine.segments = eng_nodes;
            engine.update_active_candidates();
        }
        drop(engine);

        *self
            .pending_compose
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = updated_pending;

        let mut segs = self
            .pending_segments
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        segs.retain(|s| s.1 <= win_start || s.0 >= win_end);
        for n in &nodes {
            if eng_len == 0 {
                let w: String = solved_chars[n.start_idx..n.end_idx].iter().collect();
                segs.push((
                    win_start + n.start_idx,
                    win_start + n.end_idx,
                    w,
                    n.bopomofo.clone(),
                ));
            } else {
                let left_e = n.end_idx.min(left_len);
                if n.start_idx < left_e {
                    let w: String = solved_chars[n.start_idx..left_e].iter().collect();
                    let bpm: String = joint_syls[n.start_idx..left_e]
                        .iter()
                        .map(|hs| hs.bopomofo_display())
                        .collect::<Vec<_>>()
                        .join(" ");
                    segs.push((win_start + n.start_idx, win_start + left_e, w, bpm));
                }
                let right_s = n.start_idx.max(left_len + eng_len);
                if right_s < n.end_idx {
                    let w: String = solved_chars[right_s..n.end_idx].iter().collect();
                    let bpm: String = joint_syls[right_s..n.end_idx]
                        .iter()
                        .map(|hs| hs.bopomofo_display())
                        .collect::<Vec<_>>()
                        .join(" ");
                    segs.push((
                        win_start + right_s - eng_len,
                        win_start + n.end_idx - eng_len,
                        w,
                        bpm,
                    ));
                }
            }
        }
        {
        segs.sort_by_key(|s| s.0);
        }
    }

    /// 三向智慧切換輸入法模式：
    /// - `"last_two"` (單按 Caps Lock)：最後兩個使用的輸入模式快速對切（如 英⇄漢、日⇄漢、英⇄日）
    /// - `"prev"` (Shift + Caps Lock)：在切換列表中往前切換
    /// - `"next"` (Ctrl + Caps Lock)：在切換列表中往後切換
    fn switch_keyboard_mode_action(&self, ctx: &ITfContext, action: &str) -> Result<()> {
        self.consolidate_engine_to_pending();
        let paths = RuhiPaths::resolve();
        let latest_cfg = load_ruhi_config(&paths);
        let (next_mode, new_last_mode) = {
            let mut eng = self.engine.lock().unwrap();
            eng.keyboard_mode = latest_cfg.keyboard_mode;
            eng.last_keyboard_mode = latest_cfg.last_keyboard_mode;
            eng.set_enabled_keyboard_modes(latest_cfg.enabled_keyboard_modes);
            let next = match action {
                "last_two" => {
                    let allowed = eng.get_full_cycle_modes();
                    if allowed.len() <= 1 {
                        let sole = allowed
                            .first()
                            .cloned()
                            .unwrap_or_else(|| RuhiEngine::normalize_keyboard_mode(&eng.keyboard_mode));
                        eng.set_keyboard_mode(&sole);
                        eng.keyboard_mode.clone()
                    } else if latest_cfg.capslock_switch == "english_direct" {
                        let cur_norm = RuhiEngine::normalize_keyboard_mode(&eng.keyboard_mode);
                        let target = if cur_norm == "english" {
                            let last_norm = RuhiEngine::normalize_keyboard_mode(&eng.last_keyboard_mode);
                            if !last_norm.is_empty()
                                && last_norm != "english"
                                && allowed.iter().any(|m| m == &last_norm)
                            {
                                last_norm
                            } else {
                                allowed
                                    .iter()
                                    .find(|m| m.as_str() != "english")
                                    .cloned()
                                    .unwrap_or_else(|| "english".to_string())
                            }
                        } else {
                            "english".to_string()
                        };
                        eng.set_keyboard_mode(&target);
                        eng.keyboard_mode.clone()
                    } else {
                        eng.toggle_last_two_keyboard_modes()
                    }
                }
                "prev" => eng.cycle_keyboard_mode_step(false),
                _ => eng.cycle_keyboard_mode_step(true),
            };
            (next, eng.last_keyboard_mode.clone())
        };
        if let Ok(mut cfg) = self.config.lock() {
            cfg.keyboard_mode = next_mode.clone();
            cfg.last_keyboard_mode = new_last_mode.clone();
            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.apply_config(&cfg);
                }
            }
        }
        crate::candidate_window::save_keyboard_mode_with_last(&next_mode, Some(&new_last_mode));
        crate::candidate_window::broadcast_tray_update();
        let (full, _) = self.get_full_display_and_cursor();
        if !full.is_empty() {
            self.sync_input_display(ctx)?;
        } else {
            let (cx, cy) = Self::get_caret_screen_pos();
            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.show_mode_toast(&next_mode, false, cx, cy);
                }
            }
        }
        Ok(())
    }

    /// 在游標處切換（Toggle）手動斷詞點 (Manual Word Boundary Split / Cut)
    /// 1. 尋找游標前後相鄰的連續漢字區間 [block_start, block_end)
    /// 2. 將此區間音節依手動斷詞點（manual_cuts）約束重新建立 DAG
    /// 3. 由 Viterbi 演算法重新求解最佳分詞與選字，切斷跨邊界黏著詞
    fn toggle_manual_cut_at_cursor(&self, ctx: &ITfContext, cur: usize) -> Result<()> {
        let (full_text, _) = self.get_full_display_and_cursor();
        let chars: Vec<char> = full_text.chars().collect();
        let total = chars.len();
        if cur == 0 || cur >= total {
            return Ok(());
        }

        // 1. 尋找包含 cur 的連續漢字音節區間 [block_start, block_end)
        let syls = self.pending_syllables.lock().unwrap().clone();
        if syls.len() != total {
            return Ok(());
        }

        // 檢查 cur 前後是否都有有效音節（即斷詞點兩側均為有注音的漢字）
        if syls[cur - 1].is_none() || syls[cur].is_none() {
            return Ok(());
        }

        let mut block_start = cur;
        while block_start > 0 && syls[block_start - 1].is_some() {
            block_start -= 1;
        }
        let mut block_end = cur;
        while block_end < total && syls[block_end].is_some() {
            block_end += 1;
        }

        let block_len = block_end - block_start;
        if block_len < 2 {
            return Ok(());
        }

        // 2. 切換本游標處的手動斷詞點 (Toggle)
        {
            let mut cuts_guard = self.manual_cuts.lock().unwrap();
            if cuts_guard.contains(&cur) {
                cuts_guard.remove(&cur);
            } else {
                cuts_guard.insert(cur);
            }
        }

        let cuts_snapshot = self.manual_cuts.lock().unwrap().clone();
        // 收集屬於此 block 的相對斷詞點
        let block_cuts: std::collections::HashSet<usize> = cuts_snapshot
            .iter()
            .filter(|&&c| c > block_start && c < block_end)
            .map(|&c| c - block_start)
            .collect();

        // 3. 轉換區間音節為 HsuSyllable
        let mut block_syls = Vec::new();
        for i in block_start..block_end {
            if let Some(ref bpm) = syls[i] {
                if let Some(hs) = HsuSyllable::from_bopomofo_display(bpm) {
                    block_syls.push(hs);
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        }

        // 4. 構建帶 cuts 約束的 DAG 並由 Viterbi 求解最佳路徑
        let mut engine = self.engine.lock().unwrap();
        let RuhiEngine { ref router, ref mut transitions, ref lexicon, .. } = *engine;
        let dag_builder = LatticeDAG::new(lexicon);
        let dag = dag_builder.build_dag_with_cuts(&block_syls, true, &block_cuts);
        let (nodes, _) = router.solve_best_path(block_syls.len(), &dag, Some(transitions));
        drop(engine);

        if nodes.is_empty() {
            return Ok(());
        }

        // 5. 替換 pending_compose 中 block 範圍的字元
        let new_block_text: String = nodes.iter().map(|n| n.word.as_str()).collect();
        let new_chars: Vec<char> = new_block_text.chars().collect();
        if new_chars.len() == block_len {
            let mut pending = self.pending_compose.lock().unwrap();
            let mut p_chars: Vec<char> = pending.chars().collect();
            if block_end <= p_chars.len() {
                for i in 0..block_len {
                    p_chars[block_start + i] = new_chars[i];
                }
                *pending = p_chars.into_iter().collect();
            }
        }

        // 6. 更新 pending_segments 中的分段
        let mut segs = self.pending_segments.lock().unwrap();
        segs.retain(|s| !(s.0 >= block_start && s.1 <= block_end));
        for n in &nodes {
            segs.push((
                block_start + n.start_idx,
                block_start + n.end_idx,
                n.word.clone(),
                n.bopomofo.clone(),
            ));
        }
        segs.sort_by_key(|s| s.0);
        drop(segs);

        // 7. 同步畫面並維持游標在 cur 處
        self.sync_input_display(ctx)?;
        Ok(())
    }

    fn get_manual_selection_range(&self) -> Option<(usize, usize)> {
        let anchor_opt = *self.sel_anchor.lock().unwrap();
        let cur = *self.sentence_cursor.lock().unwrap();
        let (full_text, _) = self.get_full_display_and_cursor();
        let total_chars = full_text.chars().count();
        if let Some(anchor) = anchor_opt {
            let start = anchor.min(cur);
            let end = anchor.max(cur);
            if start < end && end <= total_chars && (end - start) >= 1 {
                return Some((start, end));
            }
        }
        None
    }

    fn handle_add_user_word(&self, ctx: &ITfContext, start: usize, end: usize) -> Result<BOOL> {
        let (full_text, _) = self.get_full_display_and_cursor();
        let chars: Vec<char> = full_text.chars().collect();
        if start >= end || end > chars.len() {
            return Ok(BOOL::from(false));
        }

        let word: String = chars[start..end].iter().collect();

        // 1. 取得注音
        let bpmf: String = {
            let syls_guard = self.pending_syllables.lock().unwrap();
            let sub_syls: Vec<&str> = syls_guard[start..end].iter().filter_map(|s| s.as_deref()).collect();
            if sub_syls.len() == (end - start) {
                sub_syls.join(" ")
            } else {
                let eng = self.engine.lock().unwrap();
                if let Some(entry) = eng.lexicon.word_index.get(&word).and_then(|ids| ids.first().and_then(|&i| eng.lexicon.entries.get(i))) {
                    entry.bopomofo.clone()
                } else {
                    let mut b_list = Vec::new();
                    for &c in &chars[start..end] {
                        let c_s = eng.lexicon.lookup_char_candidates(c);
                        if let Some(first_c) = c_s.first() {
                            if let Some(e) = eng.lexicon.word_index.get(first_c).and_then(|ids| ids.first().and_then(|&i| eng.lexicon.entries.get(i))) {
                                b_list.push(e.bopomofo.clone());
                            } else {
                                b_list.push("".to_string());
                            }
                        } else {
                            b_list.push("".to_string());
                        }
                    }
                    b_list.join(" ")
                }
            }
        };

        // 2. 取得前驅詞 (上下文引力前綴)
        let prev_word: Option<String> = {
            let segs = self.pending_segments.lock().unwrap();
            if let Some(s) = segs.iter().find(|seg| seg.1 == start) {
                Some(s.2.clone())
            } else if start > 0 {
                let prev_c = chars[start - 1];
                if !prev_c.is_whitespace() && !"，。、！？：；（）「」『』—…～《》〈〉".contains(prev_c) {
                    Some(prev_c.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        };

        let safe_weight = crate::user_word_dialog::calculate_safe_weight(&word);
        let paths = RuhiPaths::resolve();

        // 3. 確認對話框
        let need_confirm = self.config.lock().map(|c| c.confirm_add_word).unwrap_or(true);
        if need_confirm {
            let title = "如喜輸入法 - 新增自建詞確認";
            let instr = format!("確定要將「{}」加入使用者自建詞庫嗎？", word);
            let prev_display = prev_word.as_deref().unwrap_or("（無）");
            let (sub_entries, _) = ruhi::lexicon::LexiconManager::decompose_entity_subentries(
                &word,
                &bpmf,
                safe_weight,
            );
            let sub_info = if sub_entries.is_empty() {
                String::new()
            } else {
                let names: Vec<String> = sub_entries.iter().map(|e| format!("{} ({})", e.word, e.bopomofo)).collect();
                format!("\n智慧拆分衍生詞：{}", names.join("、"))
            };
            let content = format!(
                "詞彙：{}\n注音：{}\n安全基準權重：{}\n上下文引力前綴：{}{}",
                word, bpmf, safe_weight, prev_display, sub_info
            );
            let (confirmed, do_not_ask) = crate::user_word_dialog::show_confirm_dialog(
                title,
                &instr,
                &content,
                "不要再問我（日後直接加入）",
            );
            if !confirmed {
                return Ok(BOOL::from(true));
            }
            if do_not_ask {
                if let Ok(mut cfg) = self.config.lock() {
                    cfg.confirm_add_word = false;
                    save_ruhi_config(&paths, &cfg);
                }
            }
        }

        // 4. 寫入詞庫與引力場
        let ok = crate::user_word_dialog::add_user_word_entry(
            &paths,
            &word,
            &bpmf,
            safe_weight,
            prev_word.as_deref(),
        );

        if ok {
            crate::user_word_dialog::log_user_word_event(&paths, &word, &full_text, &bpmf);
            self.deleted_words.lock().unwrap().remove(&word);
            let mgr = force_reload_lexicon(&paths);
            if let Ok(mut eng) = self.engine.lock() {
                eng.lexicon = (*mgr).clone();
                if let Some(p_str) = paths.transitions.to_str() {
                    let _ = eng.transitions.load_tsv(p_str);
                }
            }
        }

        // 5. 解除選取狀態並重新整理預選顯示
        *self.sel_anchor.lock().unwrap() = None;
        self.sync_input_display(ctx)?;
        Ok(BOOL::from(true))
    }

    fn handle_suppress_or_delete_word(&self, ctx: &ITfContext, start: usize, end: usize) -> Result<BOOL> {
        let (full_text, _) = self.get_full_display_and_cursor();
        let chars: Vec<char> = full_text.chars().collect();
        if start >= end || end > chars.len() {
            return Ok(BOOL::from(false));
        }

        let word: String = chars[start..end].iter().collect();
        let paths = RuhiPaths::resolve();
        let is_custom = crate::user_word_dialog::is_in_user_dict(&paths, &word);

        // 1. 取得注音
        let bpmf: String = {
            let syls_guard = self.pending_syllables.lock().unwrap();
            let sub_syls: Vec<&str> = syls_guard[start..end].iter().filter_map(|s| s.as_deref()).collect();
            if sub_syls.len() == (end - start) {
                sub_syls.join(" ")
            } else {
                let eng = self.engine.lock().unwrap();
                if let Some(entry) = eng.lexicon.word_index.get(&word).and_then(|ids| ids.first().and_then(|&i| eng.lexicon.entries.get(i))) {
                    entry.bopomofo.clone()
                } else {
                    let mut b_list = Vec::new();
                    for &c in &chars[start..end] {
                        let c_s = eng.lexicon.lookup_char_candidates(c);
                        if let Some(first_c) = c_s.first() {
                            if let Some(e) = eng.lexicon.word_index.get(first_c).and_then(|ids| ids.first().and_then(|&i| eng.lexicon.entries.get(i))) {
                                b_list.push(e.bopomofo.clone());
                            } else {
                                b_list.push("".to_string());
                            }
                        } else {
                            b_list.push("".to_string());
                        }
                    }
                    b_list.join(" ")
                }
            }
        };

        // 2. 確認對話框
        let need_confirm = self.config.lock().map(|c| c.confirm_suppress_word).unwrap_or(true);
        if need_confirm {
            let (title, instr, desc) = if is_custom {
                (
                    "如喜輸入法 - 刪除自建詞確認",
                    format!("確定要將「{}」自自建詞庫中刪除嗎？", word),
                    "刪除後該詞將不再享有自建詞優先級。"
                )
            } else {
                (
                    "如喜輸入法 - 抑制厭惡詞確認",
                    format!("確定要抑制並遮蔽「{}」（列為厭惡詞）嗎？", word),
                    "加入遮蔽清單並施加負向抑制，選字候選中將不再出現此詞彙。"
                )
            };
            let content = format!("詞彙：{}\n注音：{}\n說明：{}", word, bpmf, desc);
            let (confirmed, do_not_ask) = crate::user_word_dialog::show_confirm_dialog(
                title,
                &instr,
                &content,
                "不要再問我（日後直接執行）",
            );
            if !confirmed {
                return Ok(BOOL::from(true));
            }
            if do_not_ask {
                if let Ok(mut cfg) = self.config.lock() {
                    cfg.confirm_suppress_word = false;
                    save_ruhi_config(&paths, &cfg);
                }
            }
        }

        // 3. 執行刪詞或列入抑制清單
        let (ok, action) = crate::user_word_dialog::suppress_or_delete_entry(&paths, &word, &bpmf);
        if ok {
            if action == "suppressed" {
                self.deleted_words.lock().unwrap().insert(word.clone());
            } else {
                self.deleted_words.lock().unwrap().remove(&word);
            }
            let mgr = force_reload_lexicon(&paths);
            if let Ok(mut eng) = self.engine.lock() {
                eng.lexicon = (*mgr).clone();
                if let Some(p_str) = paths.transitions.to_str() {
                    let _ = eng.transitions.load_tsv(p_str);
                }
            }
        }

        // 4. 解除選取狀態並重新整理預選顯示
        *self.sel_anchor.lock().unwrap() = None;
        self.sync_input_display(ctx)?;
        Ok(BOOL::from(true))
    }

    fn detect_target_word_and_cands(&self, full_text: &str, cur: usize) -> (usize, (usize, usize), Vec<String>) {
        let chars: Vec<char> = full_text.chars().collect();
        if chars.is_empty() {
            return (0, (0, 0), Vec::new());
        }

        let is_western_char = |c: char| -> bool {
            c.is_ascii_alphabetic()
                || c == '\''
                || ('\u{0300}'..='\u{036F}').contains(&c)
                || ruhi::western::WesternEngine::strip_char_diacritic(c).is_ascii_alphabetic()
                || ruhi::engine::RuhiEngine::is_western_non_ascii_letter_or_mark(c)
        };

        let target_idx = if cur < chars.len() {
            if chars[cur].is_whitespace() && cur > 0 {
                // 當游標位於空白上（如 "cafe| world" 或 "cafe| "），自動向左鎖定緊鄰之單詞末尾字元
                let mut i = cur;
                while i > 0 && chars[i].is_whitespace() {
                    i -= 1;
                }
                i
            } else if cur > 0 && is_western_char(chars[cur - 1]) && !is_western_char(chars[cur]) {
                // 當游標位於歐文單詞右緣與標點交界（如 "cafe|, world"），鎖定左側剛打完之歐文單詞
                cur - 1
            } else {
                cur
            }
        } else if cur > 0 {
            let mut i = cur - 1;
            while i > 0 && chars[i].is_whitespace() {
                i -= 1;
            }
            i
        } else {
            0
        };

        // 1. 若有手動 Shift+方向鍵選取（長度 >= 1），採納選取區間
        let manual_range = self.get_manual_selection_range();

        let engine = self.engine.lock().unwrap();
        let mut cands = Vec::new();

        // 0. 日文羅馬字模式 (`japanese`)：優先提供日文漢字／平假名／片假名／羅馬字／英語意念橋接候選
        if matches!(engine.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") {
            if let Some((start, end)) = manual_range {
                let target_word: String = chars[start..end].iter().collect();
                let mut jp_cands = engine.japanese.lookup_down_candidates_for_surface(&target_word);
                if jp_cands.is_empty() {
                    jp_cands = engine.japanese.lookup_down_candidates_for_romaji(&target_word);
                }
                if !jp_cands.is_empty() {
                    return (start, (start, end), jp_cands);
                }
            } else if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let jp_cands = engine.japanese.lookup_down_candidates_for_romaji(raw_ascii);
                    if !jp_cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), jp_cands);
                    }
                }
            }
            // 若游標所在處屬於已按 Space 定錨之日文詞段（pending_segments），針對該詞段查詢候選
            {
                let segs = self.pending_segments.lock().unwrap();
                for (s_start, s_end, ref s_surf, ref s_raw) in segs.iter() {
                    if target_idx >= *s_start && target_idx < *s_end && *s_end <= chars.len() {
                        let mut jp_seg_cands = if !s_raw.is_empty() {
                            engine.japanese.lookup_down_candidates_for_romaji(s_raw)
                        } else {
                            Vec::new()
                        };
                        for c in engine.japanese.lookup_down_candidates_for_surface(s_surf) {
                            if !jp_seg_cands.contains(&c) {
                                jp_seg_cands.push(c);
                            }
                        }
                        if !jp_seg_cands.is_empty() {
                            return (*s_start, (*s_start, *s_end), jp_seg_cands);
                        }
                    }
                }
            }
            // 若游標所在處為空白分隔之獨立詞段，優先針對該詞段查詢
            if target_idx < chars.len() && !chars[target_idx].is_whitespace() {
                let mut tok_start = target_idx;
                while tok_start > 0 && !chars[tok_start - 1].is_whitespace() {
                    tok_start -= 1;
                }
                let mut tok_end = target_idx + 1;
                while tok_end < chars.len() && !chars[tok_end].is_whitespace() {
                    tok_end += 1;
                }
                let tok_surf: String = chars[tok_start..tok_end].iter().collect();
                let mut jp_tok_cands = engine.japanese.lookup_down_candidates_for_surface(&tok_surf);
                if jp_tok_cands.is_empty() {
                    jp_tok_cands = engine.japanese.lookup_down_candidates_for_romaji(&tok_surf);
                }
                if !jp_tok_cands.is_empty() {
                    return (tok_start, (tok_start, tok_end), jp_tok_cands);
                }
            }
            let full_surf: String = chars.iter().collect();
            let mut jp_surf_cands = engine.japanese.lookup_down_candidates_for_surface(&full_surf);
            if jp_surf_cands.is_empty() {
                jp_surf_cands = engine.japanese.lookup_down_candidates_for_romaji(&full_surf);
            }
            if !jp_surf_cands.is_empty() {
                return (0, (0, chars.len()), jp_surf_cands);
            }
        }

        // 0b. 韓文羅馬拼音模式 (`korean`)：提供韓文 N-best 候選清單
        if matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja") {
            if engine.korean.has_buffer() {
                let kr_cands = engine.korean.lookup_down_candidates();
                if !kr_cands.is_empty() {
                    let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
                    let end = chars.len().max(pending_len);
                    let start = pending_len.min(end);
                    return (start, (start, end), kr_cands);
                }
            }
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let kr_cands = engine.korean.lookup_down_candidates_for_romaja(raw_ascii);
                    if !kr_cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), kr_cands);
                    }
                }
            }
            {
                let segs = self.pending_segments.lock().unwrap();
                for (s_start, s_end, ref _s_surf, ref s_raw) in segs.iter() {
                    if target_idx >= *s_start && target_idx < *s_end && *s_end <= chars.len() && !s_raw.is_empty() {
                        let mut kr_seg_cands = engine.korean.lookup_down_candidates_for_romaja(s_raw);
                        for sc in engine.korean.lookup_down_candidates_for_surface(_s_surf) {
                            if !kr_seg_cands.contains(&sc) {
                                kr_seg_cands.push(sc);
                            }
                        }
                        if !kr_seg_cands.is_empty() {
                            return (*s_start, (*s_start, *s_end), kr_seg_cands);
                        }
                    }
                }
            }
        }

        if matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja") {
            // 若游標所在處為已定字之韓語詞段（表面詞反查 N-best 同音／近音與漢字詞）
            if target_idx < chars.len() && !chars[target_idx].is_whitespace() {
                let mut tok_start = target_idx;
                while tok_start > 0 && !chars[tok_start - 1].is_whitespace() {
                    tok_start -= 1;
                }
                let mut tok_end = target_idx + 1;
                while tok_end < chars.len() && !chars[tok_end].is_whitespace() {
                    tok_end += 1;
                }
                let tok_surf: String = chars[tok_start..tok_end].iter().collect();
                let mut kr_tok_cands = engine.korean.lookup_down_candidates_for_surface(&tok_surf);
                if kr_tok_cands.is_empty() {
                    kr_tok_cands = engine.korean.lookup_down_candidates_for_romaja(&tok_surf);
                }
                if !kr_tok_cands.is_empty() {
                    return (tok_start, (tok_start, tok_end), kr_tok_cands);
                }
            }
            let full_surf: String = chars.iter().collect();
            let mut kr_surf_cands = engine.korean.lookup_down_candidates_for_surface(&full_surf);
            if kr_surf_cands.is_empty() {
                kr_surf_cands = engine.korean.lookup_down_candidates_for_romaja(&full_surf);
            }
            if !kr_surf_cands.is_empty() {
                return (0, (0, chars.len()), kr_surf_cands);
            }
        }

        // 0.4 中文模式簡易英語語義橋接（不混打前提下輸入 tks/thanks/hello 等按 ↓ 直接提供中文致意詞）
        if !matches!(
            engine.keyboard_mode.as_str(),
            "japanese" | "jp" | "romaji" | "korean" | "ko" | "hangul" | "romaja" | "english" | "en" | "latin" | "western" | "euro" | "sanskrit"
        ) && manual_range.is_none()
        {
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let clean_ascii: String = raw_ascii.trim().chars().filter(|c| c.is_ascii_alphabetic()).collect();
                    let mut zh_bridge = ruhi::western::WesternEngine::lookup_chinese_semantic_bridge(&clean_ascii);
                    if !zh_bridge.is_empty() {
                        for w in engine.western.lookup_up_arrow_candidates(&clean_ascii) {
                            if !zh_bridge.contains(&w) {
                                zh_bridge.push(w);
                            }
                        }
                        let del_words = self.deleted_words.lock().unwrap();
                        zh_bridge.retain(|c| !del_words.contains(c));
                        if !zh_bridge.is_empty() {
                            return (raw_start, (raw_start, raw_end), zh_bridge);
                        }
                    }
                }
            }
        }

        // 0.5 行列 30 模式 (`array30`)：若游標位於剛組出的單字上，優先提供同碼重碼字，再補充同音字
        if matches!(engine.keyboard_mode.as_str(), "array30" | "ar30") && manual_range.is_none() {
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let mut ar_cands = engine.array30.table.lookup_compose_candidates(raw_ascii, 20);
                    let homo_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], None);
                    for hc in homo_cands {
                        if !ar_cands.contains(&hc) {
                            ar_cands.push(hc);
                        }
                    }
                    let del_words = self.deleted_words.lock().unwrap();
                    ar_cands.retain(|c| !del_words.contains(c));
                    if !ar_cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), ar_cands);
                    }
                }
            }
        }

        // 0.5b 龍龕形音模式 (`longkan`)：若游標位於剛組出的詞或字上，優先提供同龍龕碼候選字詞，再補充同音字
        if matches!(engine.keyboard_mode.as_str(), "longkan" | "lk") && manual_range.is_none() {
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let mut lk_cands = engine.longkan.table.lookup_compose_candidates(raw_ascii, 24);
                    if raw_end.saturating_sub(raw_start) == 1 {
                        let homo_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], None);
                        for hc in homo_cands {
                            if !lk_cands.contains(&hc) {
                                lk_cands.push(hc);
                            }
                        }
                    }
                    let del_words = self.deleted_words.lock().unwrap();
                    lk_cands.retain(|c| !del_words.contains(c));
                    if !lk_cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), lk_cands);
                    }
                }
            }
        }

        // 0.6 智慧四角號碼模式 (`four_corner`)：若游標位於剛組出的詞或字上，優先提供分流後之同四角碼候選字詞，再於單字時補充同音字
        if matches!(engine.keyboard_mode.as_str(), "four_corner" | "4corner" | "fc") && manual_range.is_none() {
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let mut fc_cands: Vec<String> = engine
                        .four_corner
                        .table
                        .lookup_for_reselect(raw_ascii);
                    if !raw_ascii.starts_with('-') && raw_end.saturating_sub(raw_start) == 1 {
                        let homo_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], None);
                        for hc in homo_cands {
                            if !fc_cands.contains(&hc) {
                                fc_cands.push(hc);
                            }
                        }
                    }
                    let del_words = self.deleted_words.lock().unwrap();
                    fc_cands.retain(|c| !del_words.contains(c));
                    if !fc_cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), fc_cands);
                    }
                }
            }
        }

        if let Some((start, end)) = manual_range {
            // 使用者主動 Shift 選取區間：提供該多字詞或單字的同音詞/字，或歐文重音變體
            let target_word: String = chars[start..end].iter().collect();
            let is_western_sel = chars[start..end]
                .iter()
                .all(|&c| c.is_ascii_alphabetic() || ruhi::western::WesternEngine::strip_char_diacritic(c).is_ascii_alphabetic());
            if is_western_sel {
                let single_ch = if end - start == 1 { Some(chars[start]) } else { None };
                cands = engine.western.lookup_down_arrow_candidates(&target_word, single_ch);
                return (start, (start, end), cands);
            }
            let syl_seq: Option<String> = {
                let syls_guard = self.pending_syllables.lock().unwrap();
                let sub_syls: Vec<&str> = syls_guard[start..end].iter().filter_map(|s| s.as_deref()).collect();
                if sub_syls.len() == (end - start) {
                    Some(sub_syls.join(" "))
                } else {
                    None
                }
            };
            if end - start == 1 {
                let start_syl = self.pending_syllables.lock().unwrap().get(start).cloned().flatten();
                cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[start], start_syl.as_deref());
            } else {
                let word_cands = engine.lexicon.lookup_word_homophones_with_bpm(&target_word, syl_seq.as_deref());
                for w in word_cands {
                    if !cands.contains(&w) {
                        cands.push(w);
                    }
                }
                // 備用：若找不到同音詞，則以選取範圍首字同音字補充
                if cands.is_empty() && start < chars.len() {
                    let start_syl = self.pending_syllables.lock().unwrap().get(start).cloned().flatten();
                    cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[start], start_syl.as_deref());
                }
            }
            let del_words = self.deleted_words.lock().unwrap();
            cands.retain(|c| !del_words.contains(c));
            (start, (start, end), cands)
        } else {
            // 預設單字/目標字編輯模式：
            let ch = chars[target_idx];
            let is_punct = matches!(ch, '，' | '。' | '！' | '？' | '；' | '：' | '、' | '…' | '⋯' | '·' | '‧' | '—' | '─' | '－' | '-' | '～' | '~' | '(' | ')' | '（' | '）' | '[' | ']' | '{' | '}' | '<' | '>' | '《' | '》' | '「' | '」' | '『' | '』' | ',' | '.' | '?' | '!' | ';' | ':');

            if is_punct {
                let punct_cands: Vec<String> = match ch {
                    '，' | ',' => vec!["，".into(), "、".into(), ",".into(), "﹐".into(), "﹑".into()],
                    '、' => vec!["、".into(), "，".into(), "﹑".into(), ",".into()],
                    '。' | '.' => vec!["。".into(), "……".into(), "…".into(), ".".into(), "．".into(), "·".into(), "‧".into(), "•".into(), "⋯".into(), "...".into()],
                    '？' | '?' => vec!["？".into(), "?".into(), "⁇".into(), "⁈".into(), "¿".into()],
                    '！' | '!' => vec!["！".into(), "!".into(), "‼".into(), "⁉".into(), "¡".into()],
                    '：' | ':' => vec!["：".into(), ":".into(), "；".into(), ";".into()],
                    '；' | ';' => vec!["；".into(), ";".into(), "：".into(), ":".into()],
                    '…' | '⋯' => vec!["……".into(), "…".into(), "⋯".into(), "...".into()],
                    '·' | '‧' => vec!["·".into(), "‧".into(), "•".into(), "。".into(), "．".into()],
                    '—' | '－' | '-' | '─' => vec!["──".into(), "——".into(), "—".into(), "─".into(), "～".into(), "~".into(), "－".into(), "-".into(), "＿".into(), "_".into()],
                    '～' | '~' => vec!["～".into(), "~".into(), "～～".into(), "〰".into(), "〜".into(), "—".into(), "──".into(), "——".into(), "－".into(), "-".into()],
                    '「' | '『' => vec!["「".into(), "『".into(), "“".into(), "‘".into(), "\"".into(), "'".into(), "《".into(), "〈".into(), "【".into(), "〔".into(), "（".into()],
                    '」' | '』' => vec!["」".into(), "』".into(), "”".into(), "’".into(), "\"".into(), "'".into(), "》".into(), "〉".into(), "】".into(), "〕".into(), "）".into()],
                    '《' | '〈' | '<' => vec!["《".into(), "〈".into(), "「".into(), "『".into(), "【".into(), "〔".into(), "<".into()],
                    '》' | '〉' | '>' => vec!["》".into(), "〉".into(), "」".into(), "』".into(), "】".into(), "〕".into(), ">".into()],
                    '（' | '(' => vec!["（".into(), "(".into(), "【".into(), "〔".into(), "[".into(), "｛".into(), "{".into(), "《".into()],
                    '）' | ')' => vec!["）".into(), ")".into(), "】".into(), "〕".into(), "]".into(), "｝".into(), "}".into(), "》".into()],
                    '[' | ']' | '〔' | '〕' | '【' | '】' => vec!["【".into(), "】".into(), "〔".into(), "〕".into(), "[".into(), "]".into(), "（".into(), "）".into()],
                    _ => vec![ch.to_string()],
                };
                let target_range = (target_idx, (target_idx + 1).min(chars.len()));
                return (target_idx, target_range, punct_cands);
            }

            // ── 歐文軌 ↓ 智慧查詢：拉丁字母與外來語重音變體（cafe ➔ café、a ➔ á, à, â, ä...）──
            if is_western_char(ch) {
                let mut w_start = target_idx;
                while w_start > 0 && is_western_char(chars[w_start - 1]) {
                    w_start -= 1;
                }
                let mut w_end = target_idx + 1;
                while w_end < chars.len() && is_western_char(chars[w_end]) {
                    w_end += 1;
                }
                let span_word: String = chars[w_start..w_end].iter().collect();
                if w_end - w_start >= 2 {
                    let focus_offset = target_idx.saturating_sub(w_start);
                    let loan_cands = engine.western.lookup_word_down_candidates_at_offset(
                        &span_word,
                        Some(focus_offset),
                        Some(ch),
                    );
                    if !loan_cands.is_empty() {
                        return (w_start, (w_start, w_end), loan_cands);
                    }
                }
                let ch_cands = engine.western.lookup_down_arrow_candidates(&ch.to_string(), Some(ch));
                if !ch_cands.is_empty() {
                    return (target_idx, (target_idx, target_idx + 1), ch_cands);
                }
            }

            // ── 拓撲結構 II 對角線恆等流形諧音雙關候選（如「一試再試」➔「一事再事／一世再世／一室再室」）──
            {
                let syls_snapshot = self.pending_syllables.lock().unwrap().clone();
                if let Some(((d_start, d_end), mut diag_cands)) =
                    engine.lexicon.lookup_diagonal_pun_constructions(&chars, &syls_snapshot, target_idx)
                {
                    if target_idx == d_start || target_idx + 1 == d_end {
                        let char_syl = syls_snapshot.get(target_idx).cloned().flatten();
                        for c in engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], char_syl.as_deref()) {
                            if !diag_cands.contains(&c) {
                                diag_cands.push(c);
                            }
                        }
                        let del_words = self.deleted_words.lock().unwrap();
                        diag_cands.retain(|c| !del_words.contains(c));
                        return (target_idx, (d_start, d_end), diag_cands);
                    }
                }
            }

            // ── 動態滑動窗口探索 (Sliding Window N-Gram) ──
            // 當游標停在詞中字（如「有益」的「益」）或未決字上時，向後探索 2~4 字之跨詞同音詞 (如「益+意」➔「意義」)
            let mut forward_cand_info: Option<(usize, usize, Vec<String>)> = None;
            for span_len in (2..=4).rev() {
                if target_idx + span_len <= chars.len() {
                    let syls_guard = self.pending_syllables.lock().unwrap();
                    let sub_syls: Vec<&str> = syls_guard[target_idx..target_idx + span_len]
                        .iter()
                        .filter_map(|s| s.as_deref())
                        .collect();
                    if sub_syls.len() == span_len {
                        let bpm_seq = sub_syls.join(" ");
                        let span_words = engine.lexicon.lookup_bopomofo(&bpm_seq, true);
                        let mut valid_words: Vec<String> = span_words
                            .into_iter()
                            .filter(|e| e.word.chars().count() == span_len)
                            .map(|e| e.word.clone())
                            .collect();
                        if !valid_words.is_empty() {
                            let curr_span_str: String = chars[target_idx..target_idx + span_len].iter().collect();
                            valid_words.retain(|w| w != &curr_span_str);
                            if !valid_words.is_empty() {
                                forward_cand_info = Some((target_idx, target_idx + span_len, valid_words));
                                break;
                            }
                        }
                    }
                }
            }

            // 【同音詞優先原則】：以輸入行判定之詞為單位比對同音詞，接續該字之同音字
            let matched_seg: Option<(usize, usize, String, String)> = {
                let segs = self.pending_segments.lock().unwrap();
                segs.iter().find(|s| target_idx >= s.0 && target_idx < s.1 && (s.1 - s.0) >= 2).cloned()
            };

            let char_syl: Option<String> = self.pending_syllables.lock().unwrap().get(target_idx).cloned().flatten();

            if let Some((start, end, word, bpm)) = matched_seg {
                if target_idx > start {
                    // 游標停在詞中字（如「有益」的「益」，索引 1 > 起始 0）
                    // 若有後續跨詞同音詞（如「益+意」➔「意義」），優先以跨詞替換範圍為目標！
                    if let Some((f_start, f_end, f_words)) = forward_cand_info {
                        for w in f_words {
                            if !cands.contains(&w) { cands.push(w); }
                        }
                        let char_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], char_syl.as_deref());
                        for c in char_cands {
                            if !cands.contains(&c) { cands.push(c); }
                        }
                        let word_cands = engine.lexicon.lookup_word_homophones_with_bpm(&word, Some(&bpm));
                        for w in word_cands {
                            if !cands.contains(&w) { cands.push(w); }
                        }
                        let del_words = self.deleted_words.lock().unwrap();
                        cands.retain(|c| !del_words.contains(c));
                        let target_range = (f_start, f_end);
                        (target_idx, target_range, cands)
                    } else {
                        // 無跨詞同音詞時，鎖定游標所在字之單字替換，接續該詞同音詞
                        let char_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], char_syl.as_deref());
                        for c in char_cands {
                            if !cands.contains(&c) { cands.push(c); }
                        }
                        let word_cands = engine.lexicon.lookup_word_homophones_with_bpm(&word, Some(&bpm));
                        for w in word_cands {
                            if !cands.contains(&w) { cands.push(w); }
                        }
                        let del_words = self.deleted_words.lock().unwrap();
                        cands.retain(|c| !del_words.contains(c));
                        let target_range = (target_idx, target_idx + 1);
                        (target_idx, target_range, cands)
                    }
                } else {
                    // 游標停在詞首字（如「有益」的「有」）：優先提供該詞的同長度同音詞
                    let word_cands = engine.lexicon.lookup_word_homophones_with_bpm(&word, Some(&bpm));
                    for w in word_cands {
                        if !cands.contains(&w) {
                            cands.push(w);
                        }
                    }
                    let char_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], char_syl.as_deref());
                    for c in char_cands {
                        if !cands.contains(&c) {
                            cands.push(c);
                        }
                    }

                    let del_words = self.deleted_words.lock().unwrap();
                    cands.retain(|c| !del_words.contains(c));
                    let target_range = (start, end);
                    (target_idx, target_range, cands)
                }
            } else if let Some((f_start, f_end, f_words)) = forward_cand_info {
                for w in f_words {
                    if !cands.contains(&w) { cands.push(w); }
                }
                let char_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], char_syl.as_deref());
                for c in char_cands {
                    if !cands.contains(&c) { cands.push(c); }
                }
                let del_words = self.deleted_words.lock().unwrap();
                cands.retain(|c| !del_words.contains(c));
                let target_range = (f_start, f_end);
                (target_idx, target_range, cands)
            } else {
                // 單字模式：嚴格依輸入讀音 A 篩選同音字，杜絕破音讀音 B 劫掠
                let mut final_idx = target_idx;
                let mut final_cands = engine.lexicon.lookup_char_candidates_with_bpm(chars[target_idx], char_syl.as_deref());

                if final_cands.is_empty() && target_idx > 0 {
                    for i in (0..target_idx).rev() {
                        let test_ch = chars[i];
                        let test_punct = matches!(test_ch, '，' | '。' | '！' | '？' | '；' | '：' | '、' | '…' | '—' | '～');
                        if !test_punct {
                            let test_syl = self.pending_syllables.lock().unwrap().get(i).cloned().flatten();
                            let test_cands = engine.lexicon.lookup_char_candidates_with_bpm(test_ch, test_syl.as_deref());
                            if !test_cands.is_empty() {
                                final_idx = i;
                                final_cands = test_cands;
                                break;
                            }
                        }
                    }
                }

                if final_cands.is_empty() && final_idx < chars.len() && !chars[final_idx].is_whitespace() {
                    final_cands.push(chars[final_idx].to_string());
                }

                let del_words = self.deleted_words.lock().unwrap();
                final_cands.retain(|c| !del_words.contains(c));
                let target_range = (final_idx, (final_idx + 1).min(chars.len()));
                (final_idx, target_range, final_cands)
            }
        }
    }

    /// `Shift + ↓`：於任意輸入模式、任意游標字詞或標記區間喚起跨輸入法候選編碼字詞清單（預設龍龕形音，若衝突則彈出模式選擇器）
    fn detect_shift_down_longkan_target_and_cands(
        &self,
        full_text: &str,
        cur: usize,
    ) -> (usize, (usize, usize), Vec<String>) {
        let chars: Vec<char> = full_text.chars().collect();
        if chars.is_empty() {
            return (0, (0, 0), Vec::new());
        }
        let target_idx = if cur < chars.len() {
            if chars[cur].is_whitespace() && cur > 0 {
                let mut i = cur;
                while i > 0 && chars[i].is_whitespace() {
                    i -= 1;
                }
                i
            } else {
                cur
            }
        } else {
            let mut i = chars.len() - 1;
            while i > 0 && chars[i].is_whitespace() {
                i -= 1;
            }
            i
        };
        let manual_range = self.get_manual_selection_range();
        let pending_syls = self.pending_syllables.lock().unwrap().clone();
        let engine = self.engine.lock().unwrap();
        let cur_mode = RuhiEngine::normalize_keyboard_mode(&engine.keyboard_mode);
        let cfg_mode = RuhiEngine::normalize_keyboard_mode(&engine.shift_down_mode);
        if cur_mode == cfg_mode && engine.get_active_shift_down_mode_override().is_none() {
            engine.set_shift_down_mode_picker(true);
            let r_range = manual_range.unwrap_or((target_idx, (target_idx + 1).min(chars.len())));
            let picker_cands: Vec<String> = engine
                .get_shift_down_mode_picker_options()
                .into_iter()
                .map(|(_, label)| label)
                .collect();
            return (target_idx, r_range, picker_cands);
        }
        engine.set_shift_down_mode_picker(false);
        let (r_idx, r_range, mut cands) = engine.compute_shift_down_longkan_for_slice(
            &chars,
            &pending_syls,
            manual_range,
            target_idx,
        );
        let del_words = self.deleted_words.lock().unwrap();
        cands.retain(|c| !del_words.contains(c));
        (r_idx, r_range, cands)
    }

    /// 向上方向鍵 `↑`：歐漢日多軌智慧拼寫救援、多字詞頭碼展開與上下文近音模糊推理
    fn detect_up_arrow_target_and_cands(&self, full_text: &str, cur: usize) -> (usize, (usize, usize), Vec<String>) {
        let chars: Vec<char> = full_text.chars().collect();
        if chars.is_empty() {
            return (0, (0, 0), Vec::new());
        }

        let is_western_char = |c: char| -> bool {
            c.is_ascii_alphabetic()
                || c == '\''
                || ('\u{0300}'..='\u{036F}').contains(&c)
                || ruhi::western::WesternEngine::strip_char_diacritic(c).is_ascii_alphabetic()
                || ruhi::engine::RuhiEngine::is_western_non_ascii_letter_or_mark(c)
        };

        let target_idx = if cur < chars.len() {
            if chars[cur].is_whitespace() && cur > 0 {
                let mut i = cur;
                while i > 0 && chars[i].is_whitespace() {
                    i -= 1;
                }
                i
            } else if cur > 0 && is_western_char(chars[cur - 1]) && !is_western_char(chars[cur]) {
                cur - 1
            } else {
                cur
            }
        } else if cur > 0 {
            let mut i = cur - 1;
            while i > 0 && chars[i].is_whitespace() {
                i -= 1;
            }
            i
        } else {
            0
        };

        let manual_range = self.get_manual_selection_range();
        let syls_snapshot = self.pending_syllables.lock().unwrap().clone();
        let mut engine = self.engine.lock().unwrap();
        let del_words = self.deleted_words.lock().unwrap();

        // 0. 日文羅馬字模式 (`japanese`)：日文長句／敬語前綴預測展開 + 英語學習者意念橋接 + 跨軌歐文縮寫救援
        if matches!(engine.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") {
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let mut cands = Vec::new();
                    let has_jp_bridge = !ruhi::japanese::JapaneseEngine::lookup_english_learner_bridge(raw_ascii).is_empty();
                    if !has_jp_bridge && engine.western.has_high_confidence_rescue(raw_ascii) {
                        for w in engine.western.lookup_up_arrow_candidates(raw_ascii) {
                            if !cands.contains(&w) {
                                cands.push(w);
                            }
                        }
                        for w in engine.japanese.lookup_up_predictions_for_romaji(raw_ascii) {
                            if !cands.contains(&w) {
                                cands.push(w);
                            }
                        }
                    } else {
                        cands.extend(engine.japanese.lookup_up_predictions_for_romaji(raw_ascii));
                        for w in engine.western.lookup_up_arrow_candidates(raw_ascii) {
                            if !cands.contains(&w) {
                                cands.push(w);
                            }
                        }
                    }
                    cands.retain(|c| !del_words.contains(c));
                    if !cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), cands);
                    }
                }
            }
            {
                let segs = self.pending_segments.lock().unwrap();
                for (s_start, s_end, ref s_surf, ref s_raw) in segs.iter() {
                    if target_idx >= *s_start && target_idx < *s_end && *s_end <= chars.len() {
                        let mut seg_preds = Vec::new();
                        if !s_raw.is_empty() {
                            seg_preds.extend(engine.japanese.lookup_up_predictions_for_romaji(s_raw));
                        }
                        for w in engine.japanese.lookup_up_predictions_for_hira(s_surf) {
                            if !seg_preds.contains(&w) {
                                seg_preds.push(w);
                            }
                        }
                        seg_preds.retain(|c| !del_words.contains(c));
                        if !seg_preds.is_empty() {
                            return (*s_start, (*s_start, *s_end), seg_preds);
                        }
                    }
                }
            }
            if target_idx < chars.len() && !chars[target_idx].is_whitespace() {
                let mut tok_start = target_idx;
                while tok_start > 0 && !chars[tok_start - 1].is_whitespace() {
                    tok_start -= 1;
                }
                let mut tok_end = target_idx + 1;
                while tok_end < chars.len() && !chars[tok_end].is_whitespace() {
                    tok_end += 1;
                }
                let tok_surf: String = chars[tok_start..tok_end].iter().collect();
                let mut tok_preds = engine.japanese.lookup_up_predictions_for_hira(&tok_surf);
                for w in engine.japanese.lookup_up_predictions_for_romaji(&tok_surf) {
                    if !tok_preds.contains(&w) {
                        tok_preds.push(w);
                    }
                }
                tok_preds.retain(|c| !del_words.contains(c));
                if !tok_preds.is_empty() {
                    return (tok_start, (tok_start, tok_end), tok_preds);
                }
            }
            let full_surf: String = chars.iter().collect();
            let mut jp_preds = engine.japanese.lookup_up_predictions_for_hira(&full_surf);
            for w in engine.japanese.lookup_up_predictions_for_romaji(&full_surf) {
                if !jp_preds.contains(&w) {
                    jp_preds.push(w);
                }
            }
            jp_preds.retain(|c| !del_words.contains(c));
            if !jp_preds.is_empty() {
                return (0, (0, chars.len()), jp_preds);
            }
        }

        // 0b. 韓文羅馬拼音模式 (`korean`)：學習者骨架容錯與常用詞前綴預測
        if matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja") {
            if engine.korean.has_buffer() {
                let mut kr_cands = engine.korean.lookup_up_candidates();
                kr_cands.retain(|c| !del_words.contains(c));
                if !kr_cands.is_empty() {
                    let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
                    let end = chars.len().max(pending_len);
                    let start = pending_len.min(end);
                    return (start, (start, end), kr_cands);
                }
            }
            if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
                if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                    let mut kr_cands = engine.korean.lookup_up_candidates_for_romaja(raw_ascii);
                    kr_cands.retain(|c| !del_words.contains(c));
                    if !kr_cands.is_empty() {
                        return (raw_start, (raw_start, raw_end), kr_cands);
                    }
                }
            }
            {
                let segs = self.pending_segments.lock().unwrap();
                for (s_start, s_end, ref _s_surf, ref s_raw) in segs.iter() {
                    if target_idx >= *s_start && target_idx < *s_end && *s_end <= chars.len() && !s_raw.is_empty() {
                        let mut kr_seg_cands = engine.korean.lookup_up_candidates_for_romaja(s_raw);
                        for sc in engine.korean.lookup_up_candidates_for_surface(_s_surf) {
                            if !kr_seg_cands.contains(&sc) {
                                kr_seg_cands.push(sc);
                            }
                        }
                        kr_seg_cands.retain(|c| !del_words.contains(c));
                        if !kr_seg_cands.is_empty() {
                            return (*s_start, (*s_start, *s_end), kr_seg_cands);
                        }
                    }
                }
            }
        }

        if matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja") {
            if target_idx < chars.len() && !chars[target_idx].is_whitespace() {
                let mut tok_start = target_idx;
                while tok_start > 0 && !chars[tok_start - 1].is_whitespace() {
                    tok_start -= 1;
                }
                let mut tok_end = target_idx + 1;
                while tok_end < chars.len() && !chars[tok_end].is_whitespace() {
                    tok_end += 1;
                }
                let tok_surf: String = chars[tok_start..tok_end].iter().collect();
                let mut kr_tok_cands = engine.korean.lookup_up_candidates_for_surface(&tok_surf);
                if kr_tok_cands.is_empty() {
                    kr_tok_cands = engine.korean.lookup_up_candidates_for_romaja(&tok_surf);
                }
                kr_tok_cands.retain(|c| !del_words.contains(c));
                if !kr_tok_cands.is_empty() {
                    return (tok_start, (tok_start, tok_end), kr_tok_cands);
                }
            }
            let full_surf: String = chars.iter().collect();
            let mut kr_surf_cands = engine.korean.lookup_up_candidates_for_surface(&full_surf);
            if kr_surf_cands.is_empty() {
                kr_surf_cands = engine.korean.lookup_up_candidates_for_romaja(&full_surf);
            }
            kr_surf_cands.retain(|c| !del_words.contains(c));
            if !kr_surf_cands.is_empty() {
                return (0, (0, chars.len()), kr_surf_cands);
            }
        }

        // 1. 若有手動 Shift+方向鍵選取區間
        if let Some((start, end)) = manual_range {
            let sel_text: String = chars[start..end].iter().collect();
            if chars[start..end].iter().all(|&c| is_western_char(c)) {
                let prefix_before_word: String = chars[..start].iter().collect();
                let next_after_word: String = chars[end..].iter().collect();
                let mut cands = engine.western.lookup_contextual_up_arrow_candidates(
                    &prefix_before_word,
                    &sel_text,
                    &next_after_word,
                );
                cands.retain(|c| !del_words.contains(c));
                return (start, (start, end), cands);
            }
            let RuhiEngine { ref lexicon, ref mut transitions, .. } = *engine;
            let (r_idx, r_range, mut cands) = lexicon.lookup_contextual_fuzzy_rescue(
                &chars,
                &syls_snapshot,
                cur,
                Some((start, end)),
                transitions,
            );
            cands.retain(|c| !del_words.contains(c));
            return (r_idx, r_range, cands);
        }

        // 2. 若游標所在處為拉丁／歐文字母（例如輸入 tks、dont、recieve，或頭碼 tbcj、llxf、drj、ljdr）
        let ch = chars[target_idx];
        if is_western_char(ch) {
            let mut w_start = target_idx;
            while w_start > 0 && is_western_char(chars[w_start - 1]) {
                w_start -= 1;
            }
            let mut w_end = target_idx + 1;
            while w_end < chars.len() && is_western_char(chars[w_end]) {
                w_end += 1;
            }
            let word: String = chars[w_start..w_end].iter().collect();
            let prev_ctx: Option<String> = if w_start > 0 {
                let p_start = w_start.saturating_sub(2);
                Some(chars[p_start..w_start].iter().collect())
            } else if !engine.last_committed_word.is_empty() {
                Some(engine.last_committed_word.clone())
            } else {
                None
            };
            let kb_mode = engine.keyboard_mode.clone();
            let is_western_mode = engine.is_latin_mode
                || matches!(
                    kb_mode.to_lowercase().as_str(),
                    "english" | "en" | "latin" | "western" | "euro" | "sanskrit"
                );
            let high_conf_en = is_western_mode || engine.western.has_high_confidence_rescue(&word);
            let mut cands = Vec::new();
            if word.chars().count() >= 3 && !high_conf_en {
                if kb_mode == "cangjie" {
                    cands.extend(engine.cangjie.table.lookup_phrase_headcode(&word, &engine.lexicon));
                } else if matches!(kb_mode.as_str(), "array30" | "ar30") {
                    cands.extend(engine.array30.table.lookup_phrase_headcode(&word, &engine.lexicon));
                } else if matches!(kb_mode.as_str(), "longkan" | "lk") {
                    cands.extend(engine.longkan.table.lookup_phrase_headcode(&word, &engine.lexicon));
                } else {
                    let RuhiEngine { ref lexicon, ref mut transitions, .. } = *engine;
                    cands.extend(lexicon.lookup_multi_char_headcode(
                        &word,
                        &kb_mode,
                        prev_ctx.as_deref(),
                        Some(transitions),
                    ));
                }
                for w in engine.western.lookup_up_arrow_candidates(&word) {
                    if !cands.contains(&w) {
                        cands.push(w);
                    }
                }
            } else {
                let prefix_before_word: String = chars[..w_start].iter().collect();
                let w_end = (w_start + word.chars().count()).min(chars.len());
                let next_after_word: String = chars[w_end..].iter().collect();
                for w in engine.western.lookup_contextual_up_arrow_candidates(
                    &prefix_before_word,
                    &word,
                    &next_after_word,
                ) {
                    if !cands.contains(&w) {
                        cands.push(w);
                    }
                }
                if word.chars().count() >= 3 {
                    let extra_hc = if kb_mode == "cangjie" {
                        engine.cangjie.table.lookup_phrase_headcode(&word, &engine.lexicon)
                    } else if matches!(kb_mode.as_str(), "array30" | "ar30") {
                        engine.array30.table.lookup_phrase_headcode(&word, &engine.lexicon)
                    } else if matches!(kb_mode.as_str(), "longkan" | "lk") {
                        engine.longkan.table.lookup_phrase_headcode(&word, &engine.lexicon)
                    } else {
                        let RuhiEngine { ref lexicon, ref mut transitions, .. } = *engine;
                        lexicon.lookup_multi_char_headcode(
                            &word,
                            &kb_mode,
                            prev_ctx.as_deref(),
                            Some(transitions),
                        )
                    };
                    for w in extra_hc {
                        if !cands.contains(&w) {
                            cands.push(w);
                        }
                    }
                }
            }
            cands.retain(|c| !del_words.contains(c));
            return (w_start, (w_start, w_end), cands);
        }

        // 3. 跨軌救援與多字詞頭碼展開（Cross-Track Rescue & Headcode Expansion）：
        // 若使用者在許氏／大千／倉頡／行列 30 模式下輸入頭碼（如 jcdl 被組為「者ㄌ」）或誤打英文縮寫（如 tks），檢查其對應之 raw_ascii
        let mut cross_track_cands: Option<(usize, usize, Vec<String>)> = None;
        if let Some((raw_start, raw_end, ref raw_ascii)) = *self.last_raw_ascii.lock().unwrap() {
            if target_idx >= raw_start && target_idx < raw_end && raw_end <= chars.len() {
                let clean_ascii: String = raw_ascii.trim().chars().filter(|c| !c.is_whitespace()).collect();
                let kb_mode = engine.keyboard_mode.clone();
                let prev_ctx: Option<String> = if raw_start > 0 {
                    let p_start = raw_start.saturating_sub(2);
                    Some(chars[p_start..raw_start].iter().collect())
                } else if !engine.last_committed_word.is_empty() {
                    Some(engine.last_committed_word.clone())
                } else {
                    None
                };
                if clean_ascii.chars().count() >= 3 && !engine.western.has_high_confidence_rescue(&clean_ascii) {
                    let mut hc_cands = if kb_mode == "cangjie" {
                        engine.cangjie.table.lookup_phrase_headcode(&clean_ascii, &engine.lexicon)
                    } else if matches!(kb_mode.as_str(), "array30" | "ar30") {
                        engine.array30.table.lookup_phrase_headcode(&clean_ascii, &engine.lexicon)
                    } else if matches!(kb_mode.as_str(), "longkan" | "lk") {
                        engine.longkan.table.lookup_phrase_headcode(&clean_ascii, &engine.lexicon)
                    } else {
                        let RuhiEngine { ref lexicon, ref mut transitions, .. } = *engine;
                        lexicon.lookup_multi_char_headcode(
                            &clean_ascii,
                            &kb_mode,
                            prev_ctx.as_deref(),
                            Some(transitions),
                        )
                    };
                    hc_cands.retain(|c| !del_words.contains(c));
                    if !hc_cands.is_empty() {
                        for w in engine.western.lookup_up_arrow_candidates(&clean_ascii) {
                            if !hc_cands.contains(&w) && !del_words.contains(&w) {
                                hc_cands.push(w);
                            }
                        }
                        return (raw_start, (raw_start, raw_end), hc_cands);
                    }
                }
                let alpha_ascii: String = clean_ascii.chars().filter(|c| c.is_ascii_alphabetic()).collect();
                if alpha_ascii.len() >= 2 {
                    let mut w_cands = ruhi::western::WesternEngine::lookup_chinese_semantic_bridge(&alpha_ascii);
                    for w in engine.western.lookup_up_arrow_candidates(&alpha_ascii) {
                        if !w_cands.contains(&w) {
                            w_cands.push(w);
                        }
                    }
                    if !w_cands.is_empty() {
                        cross_track_cands = Some((raw_start, raw_end, w_cands));
                    }
                }
            }
        }

        // 4. 亞洲軌（漢字／注音）上下文近音模糊推理救援（如「答按」➔「答案」、「可能地答案」➔「可能的答案」）
        let RuhiEngine { ref lexicon, ref mut transitions, .. } = *engine;
        let (r_idx, (r_start, r_end), mut asian_cands) = lexicon.lookup_contextual_fuzzy_rescue(
            &chars,
            &syls_snapshot,
            target_idx,
            None,
            transitions,
        );
        asian_cands.retain(|c| !del_words.contains(c));

        if let Some((ct_start, ct_end, mut ct_cands)) = cross_track_cands {
            // 若同時有亞洲模糊候選與跨軌歐文候選，若區間一致則合併；若亞洲無候選則直接採用跨軌歐文救援
            if asian_cands.is_empty() {
                return (ct_start, (ct_start, ct_end), ct_cands);
            } else if ct_start == r_start && ct_end == r_end {
                for w in ct_cands.drain(..) {
                    if !asian_cands.contains(&w) {
                        asian_cands.push(w);
                    }
                }
            } else {
                // 附加原始英文與第一順位縮寫展開供選取
                for w in ct_cands.into_iter().take(3) {
                    if !asian_cands.contains(&w) {
                        asian_cands.push(w);
                    }
                }
            }
        }

        if !asian_cands.is_empty() {
            return (r_idx, (r_start, r_end), asian_cands);
        }

        (target_idx, (target_idx, (target_idx + 1).min(chars.len())), Vec::new())
    }

    fn commit_current_composition(&self, ctx: &ITfContext) -> Result<()> {
        {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            eng.capture_sentence_revision_before_commit("");
        }
        self.consolidate_engine_to_pending();
        let (full, _) = self.get_full_display_and_cursor();
        self.clear_all_pending();
        {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            if eng.has_pending_revisions() {
                eng.capture_sentence_revision_before_commit("");
            }
            if !full.is_empty() {
                eng.record_committed(&format!("{}\n", full));
            }
            eng.reset_buffer();
        }
        *self.sentence_cursor.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.candidate_selecting.lock().unwrap_or_else(|e| e.into_inner()) = false;

        self.hide_candidate_window();
        set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);

        if !full.is_empty() {
            let c = full.chars().count();
            self.update_tsf_composition(ctx, &full, c, None, true)?;
        }
        Ok(())
    }

    fn commit_current_composition_with_newline(&self, ctx: &ITfContext) -> Result<()> {
        {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            eng.capture_sentence_revision_before_commit("");
        }
        self.consolidate_engine_to_pending();
        let (mut full, _) = self.get_full_display_and_cursor();
        self.clear_all_pending();
        {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            if eng.has_pending_revisions() {
                eng.capture_sentence_revision_before_commit("");
            }
            if !full.is_empty() {
                eng.record_committed(&format!("{}\n", full));
            }
            eng.reset_buffer();
        }
        *self.sentence_cursor.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.candidate_selecting.lock().unwrap_or_else(|e| e.into_inner()) = false;

        self.hide_candidate_window();
        set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);

        if !full.is_empty() {
            full.push('\n');
            let c = full.chars().count();
            self.update_tsf_composition(ctx, &full, c, None, true)?;
        }
        Ok(())
    }

    fn get_full_display_and_cursor(&self) -> (String, usize) {
        let pending = self.pending_compose.lock().unwrap().clone();
        let cur = *self.sentence_cursor.lock().unwrap();
        let engine = self.engine.lock().unwrap();
        let eng_disp = engine.get_display_text();
        let eng_cur = if engine.is_backslash_escape_pending()
            && !engine.is_latin_mode
            && !engine.is_english_keyboard_mode()
        {
            eng_disp.chars().count()
        } else {
            engine.cursor_pos
        };

        let chars: Vec<char> = pending.chars().collect();
        if cur <= chars.len() {
            let left: String = chars[..cur].iter().collect();
            let right: String = chars[cur..].iter().collect();
            let full = format!("{}{}{}", left, eng_disp, right);
            let c = cur + eng_cur;
            (full, c)
        } else {
            let full = format!("{}{}", pending, eng_disp);
            let c = chars.len() + eng_cur;
            (full, c)
        }
    }

    fn hide_candidate_window(&self) {
        if let Ok(mut cw_guard) = self.candidate_window.lock() {
            if let Some(cw) = cw_guard.as_mut() {
                cw.hide();
            }
        }
    }

    /// 候選字選取中（上下方向鍵／PageUp／PageDown）更新高亮與即時通用反查教練提示
    fn update_candidate_selection_ui(&self, new_idx: usize, cands: &[String]) {
        let coach_hint = {
            let engine = self.engine.lock().unwrap();
            if engine.is_shift_down_mode_picker() {
                let cur_name = RuhiEngine::mode_display_name(&engine.keyboard_mode);
                format!(
                    "💡 目前已在【{}】模式！請選擇 Shift+↓ 要跨查哪一種輸入模式的候選字詞（按數字/空白鍵或點選右側快切鈕）",
                    cur_name
                )
            } else if engine.is_shift_down_longkan_popup() && new_idx < cands.len() {
                engine.format_shift_down_coach_for_text(&cands[new_idx])
            } else if (engine.is_reverse_coach_enabled()
                || matches!(engine.keyboard_mode.as_str(), "array30" | "ar30" | "longkan" | "lk"))
                && new_idx < cands.len()
            {
                engine.format_universal_coach_for_text(&cands[new_idx])
            } else {
                String::new()
            }
        };
        if let Ok(mut cw_guard) = self.candidate_window.lock() {
            if let Some(cw) = cw_guard.as_mut() {
                let coach_opt = if coach_hint.is_empty() {
                    None
                } else {
                    Some(coach_hint.as_str())
                };
                cw.sync_selection_with_candidates(cands, new_idx, coach_opt);
            }
        }
    }

    /// 計算目前輸入狀態下的「許氏注音 ➔ 行列 30 即時反查教練提示」與候選即時預覽列
    fn compute_coach_hint_and_preview(
        &self,
        full_text: &str,
        cur_pos: usize,
        is_cand_sel: bool,
        cands: &[String],
        sel_idx: usize,
        sel_range: Option<(usize, usize)>,
    ) -> (String, bool, bool, bool, Vec<String>) {
        let engine = self.engine.lock().unwrap();
        let is_cj_mode = matches!(engine.keyboard_mode.as_str(), "cangjie" | "cj");
        let is_ar30_mode = matches!(engine.keyboard_mode.as_str(), "array30" | "ar30");
        let is_fc_mode = matches!(engine.keyboard_mode.as_str(), "four_corner" | "4corner" | "fc");
        let is_lk_mode = matches!(engine.keyboard_mode.as_str(), "longkan" | "lk");

        // 0. 智慧四角號碼正在拆碼中：回傳即時 10 格候選預覽列與四角筆形拆解教練提示
        if is_fc_mode && engine.four_corner.has_buffer() {
            let live_cands = engine.four_corner.candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let mut hint = engine.four_corner.get_live_coach_hint();
            if engine.is_reverse_coach_enabled() {
                let rev = engine.get_universal_coach_hint();
                if !rev.is_empty() {
                    hint = format!("{} ｜ {}", hint, rev);
                }
            }
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, true, false, display_cands);
        }

        // 0.5 通用 `` ` `` 臨時輔助輸入法或龍龕形音 2.0 正在拆碼中：回傳即時候選預覽列與教練提示
        if engine.inline_longkan_active {
            let live_cands = engine.current_candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let (hint, force_num) = if engine.inline_target_mode == "user_abbrev" {
                let qk = engine.effective_candidate_quick_key();
                let key_label_hint = if qk == "asdf" {
                    "a~l"
                } else if qk == "1234_10" {
                    "1~9/0"
                } else {
                    "1~9"
                };
                let h = if engine.inline_abbrev_buffer.is_empty() && !engine.inline_cand_selecting {
                    "🔖 自訂詞庫：請輸入頭碼或自訂縮略碼（忘記縮略碼請按 Tab 展開候選列；再按 ` 切換自訂範本，按 ~ 進數字快打）".to_string()
                } else if engine.inline_abbrev_buffer.is_empty() && engine.inline_cand_selecting {
                    let chosen_word = live_cands.get(sel_idx).or_else(|| live_cands.first());
                    if let Some(cw) = chosen_word {
                        if let Some(badge) = engine.lexicon.format_user_word_abbrev_badge(cw) {
                            format!("🔖 自訂詞庫 ➔ {}（按 {} 直接選詞，或按 Tab/←/→/↓/↑ 切換、空白鍵確認）", badge, key_label_hint)
                        } else {
                            format!("🔖 自訂詞庫：按 {} 直接選詞，或按 Tab/←/→/↓/↑ 切換、空白鍵確認", key_label_hint)
                        }
                    } else {
                        format!("🔖 自訂詞庫：按 {} 直接選詞，或按 Tab/←/→/↓/↑ 切換、空白鍵確認", key_label_hint)
                    }
                } else {
                    let chosen_word = live_cands.get(sel_idx).or_else(|| live_cands.first());
                    if let Some(cw) = chosen_word {
                        if let Some(badge) = engine.lexicon.format_user_word_abbrev_badge(cw) {
                            format!("🔖 自訂詞庫 [{}] ➔ {}（空白鍵確認，按 {} 直接選詞，或按 Tab/←/→/↓/↑ 切換）", engine.inline_abbrev_buffer, badge, key_label_hint)
                        } else {
                            format!("🔖 自訂詞庫 [{}]：空白鍵確認，按 {} 直接選詞，或按 Tab/←/→/↓/↑ 切換", engine.inline_abbrev_buffer, key_label_hint)
                        }
                    } else {
                        format!("🔖 自訂詞庫 [{}]：空白鍵確認，按 {} 直接選詞，或按 Tab/←/→/↓/↑ 切換", engine.inline_abbrev_buffer, key_label_hint)
                    }
                };
                (h, false)
            } else if !engine.has_inline_sub_buffer() {
                let (_, badge) = engine.make_inline_preedit_display();
                (
                    format!(
                        "{}（輸入單一字／詞後即自動返回原輸入法；再按 ` 開啟使用者自訂文字範本）",
                        badge
                    ),
                    true,
                )
            } else {
                (engine.get_universal_coach_hint(), true)
            };
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, force_num, false, display_cands);
        }

        if is_lk_mode && engine.longkan.has_buffer() {
            let live_cands = engine.longkan.candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let hint = engine.get_universal_coach_hint();
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, true, false, display_cands);
        }

        if matches!(
            engine.mode,
            ruhi::engine::EngineMode::QuickNum
                | ruhi::engine::EngineMode::TemplateSelect
                | ruhi::engine::EngineMode::LexiconSwitch
                | ruhi::engine::EngineMode::SymbolTree
        ) {
            let live_cands = engine.current_candidates.clone();
            let is_live_preview = !live_cands.is_empty();
            if engine.mode == ruhi::engine::EngineMode::SymbolTree {
                return (
                    engine.symbol_tree_coach_hint(),
                    is_live_preview,
                    false,
                    false,
                    live_cands,
                );
            } else if engine.mode == ruhi::engine::EngineMode::LexiconSwitch {
                let key_label_hint = if engine.effective_candidate_quick_key() == "asdf" { "a~e" } else { "1~5" };
                let hint = format!("📚 請按 {}、Q 鍵或 Tab/←/→/↓/↑ 切換專業詞庫（目前：{}；再按 ` 回自訂詞庫，按 ~ 進數字快打）", key_label_hint, engine.active_lexicon_profile);
                return (hint, is_live_preview, false, false, live_cands);
            } else if engine.mode == ruhi::engine::EngineMode::TemplateSelect {
                let qk = engine.effective_candidate_quick_key();
                let key_label_hint = if qk == "asdf" {
                    "a~l"
                } else if qk == "1234_10" {
                    "1~9/0"
                } else {
                    "1~9"
                };
                let chosen_tpl = live_cands.get(sel_idx).or_else(|| live_cands.first());
                let tpl_badge = chosen_tpl.and_then(|ct| engine.templates.format_template_badge(ct));

                let hint = if engine.template_trigger_buffer.is_empty() && !engine.template_cand_selecting {
                    "📋 自訂範本：請輸入頭碼或範本縮略碼（忘記縮略碼請按 Tab 展開候選列；再按 ` 切換專業詞庫，按 ~ 進數字快打）".to_string()
                } else if engine.template_trigger_buffer.is_empty() && engine.template_cand_selecting {
                    if let Some(badge) = tpl_badge {
                        format!("📋 自訂範本 ➔ {}（按 {} 直接選取，或按 Tab/←/→/↓/↑ 切換、空白鍵確認）", badge, key_label_hint)
                    } else {
                        format!("📋 自訂範本：按 {} 直接選取，或按 Tab/←/→/↓/↑ 切換、空白鍵確認", key_label_hint)
                    }
                } else if let Some(badge) = tpl_badge {
                    format!("📋 自訂範本 [{}] ➔ {}（空白鍵確認，按 {} 直接選取，或按 Tab/←/→/↓/↑ 切換）", engine.template_trigger_buffer, badge, key_label_hint)
                } else if live_cands.is_empty() {
                    format!("📋 自訂範本 [{}]：查無相符範本（按 Backspace 修改，或按 Esc 取消）", engine.template_trigger_buffer)
                } else {
                    format!("📋 自訂範本 [{}]：空白鍵確認，按 {} 直接選取，或按 Tab/←/→/↓/↑ 切換", engine.template_trigger_buffer, key_label_hint)
                };
                return (hint, is_live_preview, false, false, live_cands);
            } else if engine.quick_num_buffer.is_empty() {
                let hint = "🔢 數字快打模式：請輸入數字或日期 (0-9 / . / -，如 20261112 / 2026/11/12) 即時轉金額與日期（Tab/←/→/↓/↑ 切換格式，空白鍵確認）".to_string();
                return (hint, is_live_preview, false, true, live_cands);
            } else {
                let hint = format!("🔢 數字快打：{}（按 a~l 直接選取，或按 Tab/←/→/↓/↑ 切換格式，空白鍵確認）", engine.quick_num_buffer);
                return (hint, is_live_preview, false, true, live_cands);
            };
        }

        // 1. 倉頡／行列 30 正在拆碼中：回傳即時候選預覽列與拆碼提示
        if is_cj_mode && engine.cangjie.has_buffer() {
            let live_cands = engine.cangjie.candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let hint = engine.get_universal_coach_hint();
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, true, false, display_cands);
        }

        if is_ar30_mode && engine.array30.has_buffer() {
            let live_cands = engine.array30.candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let hint = engine.get_universal_coach_hint();
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, true, false, display_cands);
        }

        if engine.is_shift_down_mode_picker() && is_cand_sel {
            let cur_name = RuhiEngine::mode_display_name(&engine.keyboard_mode);
            let hint = format!(
                "💡 目前已在【{}】模式！請選擇 Shift+↓ 要跨查哪一種輸入模式的候選字詞（按數字/空白鍵或點選右側快切鈕）",
                cur_name
            );
            return (hint, false, false, false, cands.to_vec());
        }

        if engine.is_shift_down_longkan_popup() && is_cand_sel && sel_idx < cands.len() {
            let hint = engine.format_shift_down_coach_for_text(&cands[sel_idx]);
            return (hint, false, false, false, cands.to_vec());
        }

        if !engine.is_reverse_coach_enabled() && !is_ar30_mode && !is_lk_mode {
            return (String::new(), false, false, false, cands.to_vec());
        }

        // 2. 若正在開啟候選窗選字，優先顯示當前高亮候選詞／字之目標輸入法反查拆碼
        if is_cand_sel && sel_idx < cands.len() {
            let hint = engine.format_universal_coach_for_text(&cands[sel_idx]);
            return (hint, false, false, false, cands.to_vec());
        }

        // 3. 若有 Shift+方向鍵選取範圍，優先顯示選取範圍之目標輸入法反查拆碼
        let chars: Vec<char> = full_text.chars().collect();
        if let Some((start, end)) = sel_range {
            if start < end && end <= chars.len() {
                let slice: String = chars[start..end].iter().collect();
                let hint = engine.format_universal_coach_for_text(&slice);
                if !hint.is_empty() {
                    return (hint, false, false, false, cands.to_vec());
                }
            }
        }

        // 4. 一般組字／長句輸入中：顯示游標鄰近（最多 4 個漢字）之目標輸入法反查拆碼
        if !chars.is_empty() {
            let window_str: String = if chars.len() <= 4 {
                full_text.to_string()
            } else {
                let end_pos = cur_pos.min(chars.len()).max(1);
                let start_pos = end_pos.saturating_sub(4);
                chars[start_pos..end_pos].iter().collect()
            };
            let hint = engine.format_universal_coach_for_text(&window_str);
            if !hint.is_empty() {
                return (hint, false, false, false, cands.to_vec());
            }
        }

        (engine.get_universal_coach_hint(), false, false, false, cands.to_vec())
    }

    /// 刪除選取範圍（移植自沙盒 _delete_selection）
    /// 回傳 true 表示有選取且已刪除，false 表示無選取
    fn delete_selection_if_any(&self) -> bool {
        let anchor = *self.sel_anchor.lock().unwrap();
        if let Some(anch) = anchor {
            let cur = *self.sentence_cursor.lock().unwrap();
            let start = anch.min(cur);
            let end = anch.max(cur);
            if start != end {
                let mut pending = self.pending_compose.lock().unwrap();
                let mut chars: Vec<char> = pending.chars().collect();
                let end_clamped = end.min(chars.len());
                let start_clamped = start.min(chars.len());
                if start_clamped < end_clamped {
                    chars.drain(start_clamped..end_clamped);
                    *pending = chars.into_iter().collect();
                    *self.sentence_cursor.lock().unwrap() = start_clamped;

                    let mut syls = self.pending_syllables.lock().unwrap();
                    if start_clamped < syls.len() {
                        let end_syl = end_clamped.min(syls.len());
                        syls.drain(start_clamped..end_syl);
                    }
                    let mut segs = self.pending_segments.lock().unwrap();
                    let diff = end_clamped - start_clamped;
                    segs.retain(|s| !(s.0 >= start_clamped && s.1 <= end_clamped));
                    {
                        let mut pinned = self.pending_pinned.lock().unwrap();
                        if !pinned.is_empty() {
                            *pinned = pinned
                                .iter()
                                .filter(|&&p| p < start_clamped || p >= end_clamped)
                                .map(|&p| if p >= end_clamped { p.saturating_sub(diff) } else { p })
                                .collect();
                        }
                        let mut cuts = self.manual_cuts.lock().unwrap();
                        if !cuts.is_empty() {
                            *cuts = cuts
                                .iter()
                                .filter(|&&c| c <= start_clamped || c >= end_clamped)
                                .map(|&c| if c >= end_clamped { c.saturating_sub(diff) } else { c })
                                .filter(|&c| c > 0)
                                .collect();
                        }
                    }
                    for seg in segs.iter_mut() {
                        if seg.0 >= end_clamped {
                            seg.0 = seg.0.saturating_sub(diff);
                            seg.1 = seg.1.saturating_sub(diff);
                        }
                    }
                }
            }
            *self.sel_anchor.lock().unwrap() = None;
            true
        } else {
            false
        }
    }

    fn check_reload_config(&self) {
        self.ensure_instance_upgraded();
        let now = std::time::Instant::now();
        let paths = RuhiPaths::resolve();
        let inst_key = Arc::as_ptr(&self.client_id) as usize;

        let should_check = {
            let lock = LAST_CONFIG_CHECK.lock().unwrap_or_else(|e| e.into_inner());
            match *lock {
                Some((last_instant, _, _)) => now.duration_since(last_instant).as_millis() > 300,
                None => true,
            }
        };

        if should_check {
            if let Ok(fc_meta) = std::fs::metadata(&paths.four_corner) {
                if let Ok(fc_mtime) = fc_meta.modified() {
                    let needs_fc_reload = {
                        let mut fc_lock = INSTANCE_FC_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
                        let map = fc_lock.get_or_insert_with(std::collections::HashMap::new);
                        let reload = match map.get(&inst_key).copied() {
                            Some(prev) => fc_mtime != prev,
                            None => true,
                        };
                        map.insert(inst_key, fc_mtime);
                        reload
                    };
                    if needs_fc_reload {
                        if let Ok(mut eng) = self.engine.lock() {
                            if let Ok(cnt) = eng.four_corner.load_tsv(&paths.four_corner) {
                                logger::log(&format!(
                                    "Hot-reloaded four_corner.tsv from {:?}: {} entries",
                                    paths.four_corner, cnt
                                ));
                                if eng.four_corner.has_buffer() {
                                    eng.four_corner.refresh_candidates(None, None, None);
                                }
                            }
                        }
                    }
                }
            }
            if let Ok(tpl_meta) = std::fs::metadata(&paths.templates) {
                if let Ok(tpl_mtime) = tpl_meta.modified() {
                    let needs_tpl_reload = {
                        let mut tpl_lock = INSTANCE_TPL_MTIMES.lock().unwrap_or_else(|e| e.into_inner());
                        let map = tpl_lock.get_or_insert_with(std::collections::HashMap::new);
                        let reload = match map.get(&inst_key).copied() {
                            Some(prev) => tpl_mtime != prev,
                            None => true,
                        };
                        map.insert(inst_key, tpl_mtime);
                        reload
                    };
                    if needs_tpl_reload {
                        if let Ok(mut eng) = self.engine.lock() {
                            if let Ok(cnt) = eng.templates.load_tsv(&paths.templates) {
                                logger::log(&format!(
                                    "Hot-reloaded templates.tsv from {:?}: {} entries",
                                    paths.templates, cnt
                                ));
                                if eng.mode == ruhi::engine::EngineMode::TemplateSelect {
                                    eng.refresh_quick_num_or_template_candidates();
                                }
                            }
                        }
                    }
                }
            }
            if let Ok(metadata) = std::fs::metadata(&paths.config) {
                if let Ok(mtime) = metadata.modified() {
                    let needs_disk_reload = {
                        let mut lock = LAST_CONFIG_CHECK.lock().unwrap_or_else(|e| e.into_inner());
                        let reload = match *lock {
                            Some((_, prev_mtime, _)) => mtime != prev_mtime,
                            None => true,
                        };
                        if !reload {
                            if let Some(ref mut entry) = *lock {
                                entry.0 = now;
                            }
                        }
                        reload
                    };
                    if needs_disk_reload {
                        let new_cfg = load_ruhi_config(&paths);
                        logger::log(&format!(
                            "Reloaded config: mode={}, keyboard={}, font_size={}",
                            new_cfg.input_display_mode,
                            new_cfg.keyboard_mode,
                            new_cfg.font_size
                        ));
                        let mut lock = LAST_CONFIG_CHECK.lock().unwrap_or_else(|e| e.into_inner());
                        *lock = Some((now, mtime, new_cfg));
                    }
                }
            }
        }

        let cached_cfg_opt = {
            let lock = LAST_CONFIG_CHECK.lock().unwrap_or_else(|e| e.into_inner());
            lock.as_ref().map(|(_, _, cfg)| cfg.clone())
        };
        if let Some(new_cfg) = cached_cfg_opt {
            let needs_instance_sync = self
                .config
                .lock()
                .map(|c| *c != new_cfg)
                .unwrap_or(false);
            if needs_instance_sync {
                if let Ok(mut eng) = self.engine.lock() {
                    apply_config_to_engine(&mut eng, &new_cfg);
                }
                if let Ok(mut cw_guard) = self.candidate_window.lock() {
                    if let Some(cw) = cw_guard.as_mut() {
                        cw.apply_config(&new_cfg);
                    }
                }
                if let Ok(mut cfg_guard) = self.config.lock() {
                    *cfg_guard = new_cfg;
                }
                notify_langbar_mode_update();
            }
        }
    }

    fn probe_caret(&self, ctx: &ITfContext) -> Result<()> {
        let client_id = *self.client_id.lock().unwrap_or_else(|e| e.into_inner());
        let sink: ITfCompositionSink = self.to_object().cast()?;
        let edit_session: ITfEditSession = RuhiEditSession::new(
            ctx.clone(),
            EditSessionAction::ProbeCaret,
            self.composition.clone(),
            sink,
        ).into();
        unsafe {
            // 與 update_tsf_composition 完全相同：使用 TF_ES_SYNC | TF_ES_READWRITE，LibreOffice 等宿主 100% 同步准予！
            let hr = ctx.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READWRITE);
            if hr.is_err() {
                let _ = ctx.RequestEditSession(client_id, &edit_session, TF_ES_ASYNC | TF_ES_READWRITE);
            }
        }
        Ok(())
    }

    /// 整合 Win32 (`ES_PASSWORD` / `EM_GETPASSWORDCHAR`) 與現代 TSF (`GUID_PROP_INPUTSCOPE` 之 `IS_PASSWORD` / `IS_PIN` 及 `GUID_COMPARTMENT_KEYBOARD_DISABLED`) 密碼欄位偵測
    fn is_focused_or_tsf_password(&self, pic: Option<&ITfContext>) -> bool {
        if is_focused_password_control() {
            return true;
        }
        let Some(ctx) = pic else {
            return false;
        };
        // 1. 檢查 TSF Compartment 是否標示為 Keyboard Disabled（許多 UWP/Chromium 密碼框會同步設置此項）
        if let Ok(comp_mgr) = ctx.cast::<ITfCompartmentMgr>() {
            unsafe {
                if let Ok(comp) = comp_mgr.GetCompartment(&crate::guid::GUID_COMPARTMENT_KEYBOARD_DISABLED) {
                    if let Ok(var) = comp.GetValue() {
                        let var_ptr = &var as *const windows_core::VARIANT as *const u8;
                        let vt = *(var_ptr as *const u16);
                        if vt == 3 {
                            // VT_I4 = 3
                            let val = *(var_ptr.add(8) as *const i32);
                            if val != 0 {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        // 2. 透過同步只讀 EditSession 探測當前游標之 GUID_PROP_INPUTSCOPE 是否包含 IS_PASSWORD (32) 或 PIN (64..=66)
        let client_id = *self.client_id.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(sink) = self.to_object().cast::<ITfCompositionSink>() else {
            return false;
        };
        let is_pwd = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let edit_session: ITfEditSession = RuhiEditSession::new(
            ctx.clone(),
            EditSessionAction::ProbeInputScope {
                is_password: is_pwd.clone(),
            },
            self.composition.clone(),
            sink,
        )
        .into();
        unsafe {
            let _ = ctx.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READ);
        }
        is_pwd.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn is_complex_layout_editor() -> bool {
        let mut buf = [0u16; 260];
        let len = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleFileNameW(None, &mut buf)
        };
        if len > 0 {
            let exe_path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
            if exe_path.ends_with("soffice.bin") 
                || exe_path.ends_with("soffice.exe") 
                || exe_path.contains("libreoffice") 
                || exe_path.ends_with("photoshop.exe")
                || exe_path.ends_with("indesign.exe") 
                || exe_path.ends_with("illustrator.exe")
                || exe_path.ends_with("acad.exe")
            {
                return true;
            }
        }
        false
    }

    fn sync_input_display(&self, ctx: &ITfContext) -> Result<()> {
        let has_buffer = {
            let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
            let engine = self.engine.lock().unwrap();
            pending_len > 0
                || engine.fsm.has_pending()
                || !engine.fsm.syllables.is_empty()
                || !engine.composed_sentence.is_empty()
                || engine.cangjie.has_buffer()
                || engine.array30.has_buffer()
                || engine.four_corner.has_buffer()
                || engine.longkan.has_buffer()
                || engine.japanese.has_buffer()
                || engine.korean.has_buffer()
                || engine.inline_longkan_active
                || matches!(
                    engine.mode,
                    ruhi::engine::EngineMode::QuickNum
                        | ruhi::engine::EngineMode::TemplateSelect
                        | ruhi::engine::EngineMode::LexiconSwitch
                        | ruhi::engine::EngineMode::SymbolTree
                )
        };

        if !has_buffer {
            self.hide_candidate_window();
            let _ = self.end_tsf_composition(ctx);
            return Ok(());
        }

        let (full_text, cur_pos) = self.get_full_display_and_cursor();
        let is_cand_sel = *self.candidate_selecting.lock().unwrap();
        let target_idx = *self.target_char_idx.lock().unwrap();
        let cands = self.active_candidates.lock().unwrap().clone();
        let mut sel_idx = *self.selected_candidate_idx.lock().unwrap();
        {
            let engine = self.engine.lock().unwrap();
            if matches!(engine.keyboard_mode.as_str(), "four_corner" | "4corner" | "fc")
                && engine.four_corner.has_buffer()
                && !is_cand_sel
            {
                sel_idx = engine.four_corner.candidate_idx;
            } else if (engine.inline_longkan_active
                || matches!(
                    engine.mode,
                    ruhi::engine::EngineMode::QuickNum
                        | ruhi::engine::EngineMode::TemplateSelect
                        | ruhi::engine::EngineMode::LexiconSwitch
                        | ruhi::engine::EngineMode::SymbolTree
                ))
                && !is_cand_sel
            {
                sel_idx = engine.candidate_cycle_idx;
            }
        }
        let raw_mode = self.config.lock().map(|c| c.input_display_mode.clone()).unwrap_or_else(|_| "smart_auto".to_string());
        let input_mode = match raw_mode.as_str() {
            "smart_auto" => {
                if Self::is_complex_layout_editor() {
                    "inline_underline".to_string()
                } else {
                    "floating_follow".to_string()
                }
            }
            "floating_follow" => {
                // 強制跟隨模式：若遇極其複雜排版軟體依然進行安全自適應，保障零飄移
                if Self::is_complex_layout_editor() {
                    "inline_underline".to_string()
                } else {
                    "floating_follow".to_string()
                }
            }
            "floating_fixed" => "floating_fixed".to_string(),
            _ => "inline_underline".to_string(),
        };
        let pending_syl = {
            let engine = self.engine.lock().unwrap();
            if engine.inline_longkan_active {
                engine.make_inline_preedit_display().1
            } else if engine.fsm.has_pending() {
                format!("{}{}{}", engine.fsm.cur_initial, engine.fsm.cur_medial, engine.fsm.cur_final)
            } else if engine.array30.has_buffer() {
                engine.array30.make_preedit()
            } else if engine.longkan.has_buffer() {
                engine.longkan.make_preedit()
            } else if engine.mode == ruhi::engine::EngineMode::TemplateSelect {
                if engine.template_trigger_buffer.is_empty() {
                    "📋自訂範本".to_string()
                } else {
                    format!("📋{}", engine.template_trigger_buffer)
                }
            } else if engine.mode == ruhi::engine::EngineMode::QuickNum {
                if engine.quick_num_buffer.is_empty() {
                    "🔢數字快打".to_string()
                } else {
                    format!("🔢{}", engine.quick_num_buffer)
                }
            } else if engine.mode == ruhi::engine::EngineMode::LexiconSwitch {
                format!("📚詞庫切換（目前：{}）", engine.active_lexicon_profile)
            } else if engine.mode == ruhi::engine::EngineMode::SymbolTree {
                engine.symbol_tree_breadcrumb_badge()
            } else {
                String::new()
            }
        };

        let sel_range = self.get_manual_selection_range();
        let (is_shift_down_popup, eff_shift_down_mode) = {
            let engine = self.engine.lock().unwrap();
            (
                is_cand_sel && engine.is_shift_down_longkan_popup(),
                engine.effective_shift_down_mode(),
            )
        };
        if is_shift_down_popup {
            let eng_arc = self.engine.clone();
            let syls_arc = self.pending_syllables.clone();
            let del_arc = self.deleted_words.clone();
            let active_cands_arc = self.active_candidates.clone();
            let sel_idx_arc = self.selected_candidate_idx.clone();
            let target_idx_arc = self.target_char_idx.clone();
            let target_range_arc = self.target_range.clone();
            let full_text_snap = full_text.clone();
            let cur_snap = cur_pos;
            let manual_range_snap = sel_range;
            crate::candidate_window::register_shift_down_quick_switch_handler(move |new_mode: &str| {
                let chars: Vec<char> = full_text_snap.chars().collect();
                if chars.is_empty() {
                    return None;
                }
                let target_i = if cur_snap < chars.len() {
                    if chars[cur_snap].is_whitespace() && cur_snap > 0 {
                        let mut i = cur_snap;
                        while i > 0 && chars[i].is_whitespace() {
                            i -= 1;
                        }
                        i
                    } else {
                        cur_snap
                    }
                } else {
                    let mut i = chars.len() - 1;
                    while i > 0 && chars[i].is_whitespace() {
                        i -= 1;
                    }
                    i
                };
                let syls = syls_arc.lock().ok()?.clone();
                let (r_idx, r_range, mut new_cands, coach_hint) = {
                    let mut eng = eng_arc.lock().ok()?;
                    let norm = RuhiEngine::normalize_keyboard_mode(new_mode);
                    eng.shift_down_mode = norm.clone();
                    eng.set_active_shift_down_mode_override(Some(norm));
                    eng.set_shift_down_mode_picker(false);
                    eng.set_shift_down_longkan_popup(true);
                    let (ri, rr, cands_raw) = eng.compute_shift_down_longkan_for_slice(
                        &chars,
                        &syls,
                        manual_range_snap,
                        target_i,
                    );
                    let first_cand = cands_raw.first().cloned().unwrap_or_default();
                    let hint = if !first_cand.is_empty() {
                        eng.format_shift_down_coach_for_text(&first_cand)
                    } else {
                        String::new()
                    };
                    (ri, rr, cands_raw, hint)
                };
                if let Ok(del) = del_arc.lock() {
                    new_cands.retain(|c| !del.contains(c));
                }
                if let Ok(mut ti) = target_idx_arc.lock() {
                    *ti = r_idx;
                }
                if let Ok(mut tr) = target_range_arc.lock() {
                    *tr = r_range;
                }
                if let Ok(mut si) = sel_idx_arc.lock() {
                    *si = 0;
                }
                if let Ok(mut ac) = active_cands_arc.lock() {
                    *ac = new_cands.clone();
                }
                Some((new_cands, coach_hint))
            });
        }
        let (coach_hint, is_ar30_live_preview, force_num_labels, force_asdf_labels, display_cands) =
            self.compute_coach_hint_and_preview(&full_text, cur_pos, is_cand_sel, &cands, sel_idx, sel_range);
        let eff_quick_mode = self.effective_quick_key_mode();
        let quick_edit_kind = {
            let engine = self.engine.lock().unwrap();
            if engine.mode == ruhi::engine::EngineMode::TemplateSelect {
                "templates"
            } else if engine.inline_longkan_active && matches!(engine.inline_target_mode.as_str(), "user_abbrev" | "abbrev" | "user_dict") {
                "user_dict"
            } else {
                ""
            }
        };

        if input_mode == "inline_underline" {
            self.update_tsf_composition(ctx, &full_text, cur_pos, sel_range, false)?;
            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.set_quick_key_mode(&eff_quick_mode);
                    cw.set_quick_edit_kind(quick_edit_kind);
                    cw.set_shift_down_popup_state(is_shift_down_popup, &eff_shift_down_mode);
                    cw.set_coach_hint_with_label_mode(&coach_hint, is_ar30_live_preview, force_num_labels, force_asdf_labels);
                    if (is_cand_sel || is_ar30_live_preview) && !display_cands.is_empty() {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        cw.show_candidates_only(&display_cands, sel_idx, cx, cy);
                    } else if let Some(sel_pair) = sel_range {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };
                        let target_range = *self.target_range.lock().unwrap_or_else(|e| e.into_inner());
                        cw.show_desk(
                            &full_text,
                            cur_pos,
                            &pending_syl,
                            &display_cands,
                            sel_idx,
                            is_cand_sel,
                            target_idx,
                            target_range,
                            sel_pair,
                            is_caps_on,
                            cx,
                            cy,
                        );
                    } else if !coach_hint.is_empty() {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        cw.show_candidates_only(&[], 0, cx, cy);
                    } else {
                        cw.hide();
                    }
                }
            }
        } else {
            // 如喜輸入座模式 (floating_follow 或 floating_fixed)
            // 每次探測前先重設為 CARET_UNPROBED，確保坐標絕對反映當前按鍵時點的最新文字位置（並允許負座標副螢幕）
            set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);

            // 1. 優先調用 TSF probe_caret 探測宿主文件中的當前游標 Selection 位置（感知換行與空白行）
            let _ = self.probe_caret(ctx);
            let (mut cx, mut cy) = get_last_caret_pos();

            // 2. 若 TSF 未回傳有效座標，備援嘗試 Win32 GetGUIThreadInfo
            if cx == CARET_UNPROBED || cy == CARET_UNPROBED {
                let (win_cx, win_cy) = Self::get_caret_screen_pos();
                if win_cx != CARET_UNPROBED && win_cy != CARET_UNPROBED {
                    cx = win_cx;
                    cy = win_cy;
                }
            }

            let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };

            let target_range = *self.target_range.lock().unwrap_or_else(|e| e.into_inner());
            let sel_range_pair = sel_range.unwrap_or((0, 0));

            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.set_quick_key_mode(&eff_quick_mode);
                    cw.set_quick_edit_kind(quick_edit_kind);
                    cw.set_shift_down_popup_state(is_shift_down_popup, &eff_shift_down_mode);
                    cw.set_coach_hint_with_label_mode(&coach_hint, is_ar30_live_preview, force_num_labels, force_asdf_labels);
                    cw.show_desk(&full_text, cur_pos, &pending_syl, &display_cands, sel_idx, is_cand_sel, target_idx, target_range, sel_range_pair, is_caps_on, cx, cy);
                }
            }
        }
        Ok(())
    }

    fn submit_single_tsf_composition(&self, ctx: &ITfContext, text: &str, cursor_pos: usize, selection: Option<(usize, usize)>, is_commit: bool) -> Result<()> {
        let client_id = *self.client_id.lock().unwrap_or_else(|e| e.into_inner());
        let (expected_edit_gen, expected_focus_gen) = current_composition_epoch(&self.composition);
        let sink: ITfCompositionSink = self.to_object().cast()?;
        let edit_session: ITfEditSession = RuhiEditSession::new(
            ctx.clone(),
            EditSessionAction::UpdateComposition {
                text: text.to_string(),
                cursor_pos,
                selection,
                is_commit,
                expected_edit_gen,
                expected_focus_gen,
            },
            self.composition.clone(),
            sink,
        ).into();
        unsafe {
            let hr = ctx.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READWRITE);
            if hr.is_err() {
                let _ = ctx.RequestEditSession(client_id, &edit_session, TF_ES_ASYNC | TF_ES_READWRITE);
            }
        }
        Ok(())
    }

    fn update_tsf_composition(&self, ctx: &ITfContext, text: &str, cursor_pos: usize, selection: Option<(usize, usize)>, is_commit: bool) -> Result<()> {
        if is_commit
            && (text.contains('\n') || text.contains('\r'))
            && context_host_newline_kind(ctx) == HostNewlineKind::GenericLfChunked
        {
            let chunks = split_lf_commit_chunks(text);
            for chunk in chunks {
                self.submit_single_tsf_composition(ctx, &chunk, chunk.chars().count(), None, true)?;
            }
            Ok(())
        } else {
            self.submit_single_tsf_composition(ctx, text, cursor_pos, selection, is_commit)
        }
    }

    fn end_tsf_composition(&self, ctx: &ITfContext) -> Result<()> {
        bump_composition_edit_epoch(&self.composition);
        self.hide_candidate_window();
        set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);
        let client_id = *self.client_id.lock().unwrap_or_else(|e| e.into_inner());
        let sink: ITfCompositionSink = self.to_object().cast()?;
        let edit_session: ITfEditSession = RuhiEditSession::new(
            ctx.clone(),
            EditSessionAction::EndComposition,
            self.composition.clone(),
            sink,
        ).into();
        unsafe {
            let hr = ctx.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READWRITE);
            if hr.is_err() {
                let _ = ctx.RequestEditSession(client_id, &edit_session, TF_ES_ASYNC | TF_ES_READWRITE);
            }
        }
        Ok(())
    }

    fn get_caret_screen_pos() -> (i32, i32) {
        let mut pt = POINT::default();
        let mut got_caret = false;

        let mut gui_info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };

        let fg_hwnd = unsafe { GetForegroundWindow() };
        let fg_tid = if !fg_hwnd.0.is_null() {
            unsafe { GetWindowThreadProcessId(fg_hwnd, None) }
        } else {
            0
        };

        // 1. 優先嘗試 GetGUIThreadInfo（對 LibreOffice、WordPad、Scintilla 等 Win32 視窗最具備即時精準度！）
        for &tid in &[fg_tid, 0] {
            if unsafe { GetGUIThreadInfo(tid, &mut gui_info) }.is_ok() {
                if gui_info.rcCaret.left != 0 || gui_info.rcCaret.top != 0 || gui_info.rcCaret.right != 0 || gui_info.rcCaret.bottom != 0 {
                    pt.x = gui_info.rcCaret.left;
                    pt.y = gui_info.rcCaret.bottom + 4;
                    let target = if !gui_info.hwndCaret.0.is_null() {
                        gui_info.hwndCaret
                    } else if !gui_info.hwndFocus.0.is_null() {
                        gui_info.hwndFocus
                    } else {
                        fg_hwnd
                    };
                    if !target.0.is_null() {
                        let mapped = unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(target, &mut pt).as_bool() };
                        if mapped && pt.x != CARET_UNPROBED && pt.y != CARET_UNPROBED {
                            logger::log(&format!("get_caret_screen_pos from GetGUIThreadInfo(tid={}): ({}, {})", tid, pt.x, pt.y));
                            return (pt.x, pt.y);
                        }
                    }
                }
            }
        }

        // 2. 備援嘗試 TSF ActiveView 探測之坐標（適用於 Chromium、UWP 等自繪不調用 CreateCaret 之環境）
        let (cx, cy) = get_last_caret_pos();
        if cx != CARET_UNPROBED && cy != CARET_UNPROBED {
            logger::log(&format!("get_caret_screen_pos from TSF ActiveView: ({}, {})", cx, cy));
            return (cx, cy);
        }

        // 3. 備援嘗試 GetCaretPos
        if !got_caret {
            let mut caret_pt = POINT::default();
            if unsafe { GetCaretPos(&mut caret_pt) }.is_ok() && (caret_pt.x != 0 || caret_pt.y != 0) {
                if !fg_hwnd.0.is_null() {
                    unsafe { let _ = windows::Win32::Graphics::Gdi::ClientToScreen(fg_hwnd, &mut caret_pt); }
                    pt.x = caret_pt.x;
                    pt.y = caret_pt.y + 24;
                    got_caret = true;
                }
            }
        }

        // 4. 最後備援：跟隨滑鼠位置
        if !got_caret {
            let mut mouse_pt = POINT::default();
            if unsafe { GetCursorPos(&mut mouse_pt) }.is_ok() {
                pt.x = mouse_pt.x;
                pt.y = mouse_pt.y + 20;
            } else {
                pt.x = 200;
                pt.y = 300;
            }
        }

        logger::log(&format!("get_caret_screen_pos fallback: x={}, y={}, got_caret={}", pt.x, pt.y, got_caret));
        (pt.x, pt.y)
    }
}

impl ITfThreadMgrEventSink_Impl for RuhiTextService_Impl {
    fn OnInitDocumentMgr(&self, _pdim: Option<&ITfDocumentMgr>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Ok(()))).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
    fn OnUninitDocumentMgr(&self, _pdim: Option<&ITfDocumentMgr>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Ok(()))).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
    fn OnSetFocus(&self, pdimfocus: Option<&ITfDocumentMgr>, _pdimprevfocus: Option<&ITfDocumentMgr>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.ensure_instance_upgraded();
            if let Some(dim) = pdimfocus {
                let doc_ptr = windows_core::Interface::as_raw(dim) as usize;
                bump_composition_focus_epoch(&self.composition, Some(doc_ptr));
                self.check_reload_config();
                notify_langbar_mode_update();
            } else {
                bump_composition_focus_epoch(&self.composition, None);
                clear_western_leading_sigil(self.western_instance_key());
                self.hide_candidate_window();
            }
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
    fn OnPushContext(&self, _pic: Option<&ITfContext>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Ok(()))).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
    fn OnPopContext(&self, _pic: Option<&ITfContext>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Ok(()))).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

impl ITfCompositionSink_Impl for RuhiTextService_Impl {
    fn OnCompositionTerminated(&self, _ecwrite: u32, _pcomposition: Option<&ITfComposition>) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bump_composition_edit_epoch(&self.composition);
            if let Ok(mut comp) = self.composition.try_lock() {
                *comp = None;
            }
            if let Ok(mut pending) = self.pending_compose.try_lock() {
                pending.clear();
            }
            if let Ok(mut eng) = self.engine.try_lock() {
                eng.reset_buffer();
            }
            if let Ok(mut cur) = self.sentence_cursor.try_lock() {
                *cur = 0;
            }
            if let Ok(mut sel) = self.candidate_selecting.try_lock() {
                *sel = false;
            }
            if let Ok(mut anch) = self.sel_anchor.try_lock() {
                *anch = None;
            }
            if let Ok(mut cands) = self.active_candidates.try_lock() {
                cands.clear();
            }
            if let Ok(mut tr) = self.target_range.try_lock() {
                *tr = (0, 0);
            }
            self.hide_candidate_window();
            set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

impl ITfDisplayAttributeProvider_Impl for RuhiTextService_Impl {
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Ok(crate::display_attribute::RuhiDisplayAttributeEnum::new().into())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if guid.is_null() {
                return Err(E_INVALIDARG.into());
            }
            let g = unsafe { *guid };
            if g == crate::guid::GUID_RUHI_DISPLAY_ATTR {
                Ok(crate::display_attribute::RuhiDisplayAttributeInfo::new().into())
            } else if g == crate::guid::GUID_RUHI_SELECTION_DISPLAY_ATTR {
                Ok(crate::display_attribute::RuhiSelectionDisplayAttributeInfo::new().into())
            } else {
                Err(E_INVALIDARG.into())
            }
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crlf_detection_and_commit_chunking() {
        // 1. 宿主換行協定與視窗類別名稱三分類偵測
        // 1a. 傳統 Win32 Edit / WinForms EDIT（\r\n，2 ACP 碼元）
        assert_eq!(classify_host_newline_kind("Edit"), HostNewlineKind::Win32EditCrlf);
        assert_eq!(classify_host_newline_kind("EDIT"), HostNewlineKind::Win32EditCrlf);
        assert_eq!(classify_host_newline_kind("  Edit\0"), HostNewlineKind::Win32EditCrlf);
        assert_eq!(
            classify_host_newline_kind("WindowsForms10.EDIT.app.0.141b42a"),
            HostNewlineKind::Win32EditCrlf
        );
        assert!(class_name_uses_crlf("Edit"));
        assert!(class_name_uses_crlf("EDIT"));
        assert!(class_name_uses_crlf("  Edit\0"));

        // 1b. Windows 11 Notepad 與 RichEdit 系列（單一 \r，1 ACP 碼元，嚴禁誤轉 \r\n）
        for cls in [
            "RichEditD2DPT",
            "NotepadTextBox",
            "notepadtextbox",
            "RichEdit20W",
            "RICHEDIT60W",
            "msftedit_class",
        ] {
            assert_eq!(
                classify_host_newline_kind(cls),
                HostNewlineKind::RichEditCr,
                "{cls} 應分類為 RichEditCr"
            );
            assert!(
                !class_name_uses_crlf(cls),
                "{cls} 不得誤判為 Win32 Edit CRLF"
            );
        }

        // 1c. 通用宿主（Chromium、Firefox、LibreOffice、ConsoleWindowClass、CoreWindow）
        for cls in [
            "Chrome_WidgetWin_1",
            "MozillaWindowClass",
            "SALFRAME",
            "ConsoleWindowClass",
            "Windows.UI.Core.CoreWindow",
        ] {
            assert_eq!(
                classify_host_newline_kind(cls),
                HostNewlineKind::GenericLfChunked,
                "{cls} 應分類為 GenericLfChunked"
            );
            assert!(!class_name_uses_crlf(cls));
        }

        // 2. 換行字元標準化
        assert_eq!(normalize_commit_newlines("a\nb\r\nc\rd", true), "a\r\nb\r\nc\r\nd");
        assert_eq!(normalize_commit_newlines("a\nb\r\nc\rd", false), "a\nb\nc\nd");

        // 3. 多行送出切塊（通用宿主專用）
        assert_eq!(
            split_lf_commit_chunks("第一行\n第二行"),
            vec!["第一行", "\n", "第二行"]
        );
        assert_eq!(
            split_lf_commit_chunks("第一行\n"),
            vec!["第一行", "\n"]
        );
        assert_eq!(
            split_lf_commit_chunks("\n第二行"),
            vec!["", "\n", "第二行"]
        );
        assert_eq!(
            split_lf_commit_chunks("第一行\n\n第三行"),
            vec!["第一行", "\n", "\n", "第三行"]
        );
        assert_eq!(
            split_lf_commit_chunks("單行文字"),
            vec!["單行文字"]
        );
    }

    #[test]
    fn test_notepad_richedit_multiline_and_ctrl_enter_acp_invariants_261115() {
        // 場景一：於輸入座按 Shift+Enter 產生多行內容後，按 Enter 送出至 Windows 11 Notepad (RichEditD2DPT / NotepadTextBox)
        let multiline = normalize_commit_newlines_for_kind("第一行\n第二行", HostNewlineKind::RichEditCr);
        assert_eq!(multiline, "第一行\r第二行");
        assert_eq!(
            multiline.encode_utf16().count(),
            7,
            "RichEdit CTxtStory 段落換行固定為 1 個 \\r（總長 7 ACP 碼元，與 SetText 後實際長度嚴格一致）"
        );

        // 場景二：於輸入座按 Shift+Enter 產生多行內容後，按 Ctrl+Enter 送出（含結尾換行）
        let multiline_ctrl_enter =
            normalize_commit_newlines_for_kind("第一行\n第二行\n", HostNewlineKind::RichEditCr);
        assert_eq!(multiline_ctrl_enter, "第一行\r第二行\r");
        assert_eq!(multiline_ctrl_enter.encode_utf16().count(), 8);

        // 場景三：單行內容按 Ctrl+Enter 送出（尾端附加換行）
        let single_ctrl_enter =
            normalize_commit_newlines_for_kind("測試\n", HostNewlineKind::RichEditCr);
        assert_eq!(single_ctrl_enter, "測試\r");
        assert_eq!(
            single_ctrl_enter.encode_utf16().count(),
            3,
            "單行 Ctrl+Enter 送出至 RichEdit 必須保持 UTF-16 長度 3，避免 \\r\\n (4 > 3) 觸發 TS_E_INVALIDPOS"
        );

        // 場景四：傳統 Win32 Edit 對照（仍維持 \r\n）
        let win32_multiline =
            normalize_commit_newlines_for_kind("第一行\n第二行", HostNewlineKind::Win32EditCrlf);
        assert_eq!(win32_multiline, "第一行\r\n第二行");
        assert_eq!(win32_multiline.encode_utf16().count(), 8);

        // 場景五：通用宿主對照（維持 \n 且切塊不變）
        let generic =
            normalize_commit_newlines_for_kind("a\nb\r\nc\rd", HostNewlineKind::GenericLfChunked);
        assert_eq!(generic, "a\nb\nc\nd");
        assert_eq!(
            split_lf_commit_chunks("第一行\n第二行\n"),
            vec!["第一行", "\n", "第二行", "\n"]
        );
    }

    #[test]
    fn test_quick_edit_button_labels() {
        use crate::candidate_window::quick_edit_button_label;

        // 範本編輯
        assert_eq!(quick_edit_button_label("templates", "zh-TW"), Some("✏️ 編輯範本"));
        assert_eq!(quick_edit_button_label("templates", "en-US"), Some("✏️ Edit Templates"));
        assert_eq!(quick_edit_button_label("templates", "ja-JP"), Some("✏️ テンプレート編集"));
        assert_eq!(quick_edit_button_label("templates", "ko-KR"), Some("✏️ 템플릿 편집"));

        // 詞庫編輯
        assert_eq!(quick_edit_button_label("user_dict", "zh-TW"), Some("✏️ 編輯詞庫"));
        assert_eq!(quick_edit_button_label("user_dict", "en-US"), Some("✏️ Edit Lexicon"));
        assert_eq!(quick_edit_button_label("user_dict", "ja-JP"), Some("✏️ ユーザー辞書編集"));
        assert_eq!(quick_edit_button_label("user_dict", "ko-KR"), Some("✏️ 사용자 사전 편집"));

        // 無效或空白
        assert_eq!(quick_edit_button_label("", "zh-TW"), None);
        assert_eq!(quick_edit_button_label("unknown", "zh-TW"), None);
    }

    #[test]
    fn test_bounded_composition_layout() {
        use crate::candidate_window::CandidateWindow;

        let chars: Vec<char> = "歡迎新模型這個新模型都能正確支援完整支援核心模型".chars().collect();
        let (clusters_unbounded, lines_unbounded, _, _) =
            CandidateWindow::layout_composition_clusters(&chars, 24.0, 80);
        assert_eq!(lines_unbounded, 1);
        assert_eq!(clusters_unbounded.len(), chars.len());

        let (clusters_bounded, lines_bounded, max_w, _) =
            CandidateWindow::layout_composition_clusters_bounded(&chars, 24.0, 80, Some(200));
        assert!(lines_bounded > 1, "有界上限 200px 應觸發多行折行，實際行數: {}", lines_bounded);
        assert!(max_w <= 240, "使用寬度不應嚴重超出上限，實際: {}", max_w);
        assert_eq!(clusters_bounded.len(), chars.len());
    }

    #[test]
    fn test_should_skip_joint_reeval_logic() {
        // 空動作或 none 應跳過
        assert!(should_skip_joint_reeval_on_key(false, false, 0, 0, 0, 0, "none"));
        assert!(should_skip_joint_reeval_on_key(false, false, 0, 0, 0, 0, ""));

        // 鍵入純未定音節（例如打入聲母 p 尚未帶韻母與調號）：音節數與成句字數均不變，且 post_has_pending 為 true
        assert!(should_skip_joint_reeval_on_key(false, true, 2, 2, 5, 5, "update_preedit"));
        assert!(should_skip_joint_reeval_on_key(true, true, 2, 2, 5, 5, "update_preedit"));

        // 定音（完成新音節，音節數增加）：必須不跳過（回傳 false）以重新求解
        assert!(!should_skip_joint_reeval_on_key(true, false, 2, 3, 5, 5, "compose"));

        // 成句字數增加：必須不跳過
        assert!(!should_skip_joint_reeval_on_key(false, false, 2, 2, 5, 6, "compose"));

        // 候選彈窗或換頁：必須不跳過
        assert!(!should_skip_joint_reeval_on_key(false, false, 2, 2, 5, 5, "show_candidate_popup"));
    }

    #[test]
    fn test_compute_joint_reeval_window() {
        let segs = vec![
            (0, 2, "歡迎".to_string(), "ㄏㄨㄢ ㄧㄥˊ".to_string()),
            (2, 4, "新模".to_string(), "ㄒㄧㄣ ㄇㄛˊ".to_string()),
            (4, 6, "型這".to_string(), "ㄒㄧㄥˊ ㄓㄜˋ".to_string()),
            (6, 8, "個新".to_string(), "ㄍㄜ˙ ㄒㄧㄣ".to_string()),
            (8, 10, "模型".to_string(), "ㄇㄛˊ ㄒㄧㄥˊ".to_string()),
            (10, 12, "都能".to_string(), "ㄉㄡ ㄋㄥˊ".to_string()),
            (12, 14, "正確".to_string(), "ㄓㄥˋ ㄑㄩㄝˋ".to_string()),
            (14, 16, "支援".to_string(), "ㄓ ㄩㄢˊ".to_string()),
            (16, 18, "完整".to_string(), "ㄨㄢˊ ㄓㄥˇ".to_string()),
            (18, 20, "支援".to_string(), "ㄓ ㄩㄢˊ".to_string()),
            (20, 22, "核心".to_string(), "ㄏㄜˊ ㄒㄧㄣ".to_string()),
            (22, 24, "模型".to_string(), "ㄇㄛˊ ㄒㄧㄥˊ".to_string()),
            (24, 26, "系統".to_string(), "ㄒㄧˋ ㄊㄨㄥˇ".to_string()),
            (26, 28, "架構".to_string(), "ㄐㄧㄚˋ ㄍㄡˋ".to_string()),
            (28, 30, "設計".to_string(), "ㄕㄜˋ ㄐㄧˋ".to_string()),
        ];

        // 1. 短句子（總長 10 <= 24）直接返回全範圍
        let (s, e) = compute_joint_reeval_window(0, 5, 0, 10, &segs, 14, 10);
        assert_eq!((s, e), (0, 10));

        // 2. 長句子游標在 18（總長 30）：視窗應對齊至詞邊界且包含 cur
        let (s, e) = compute_joint_reeval_window(0, 18, 0, 30, &segs, 14, 10);
        assert!(s <= 18);
        assert!(e >= 18);
        assert!(s % 2 == 0, "win_start 應對齊雙字詞邊界，實際: {}", s);
        assert!(e % 2 == 0, "win_end 應對齊雙字詞邊界，實際: {}", e);
        assert!(e - s <= 28, "視窗寬度應受上限限制，實際: {}", e - s);

        // 3. 游標在開頭 (cur = 0)
        let (s0, e0) = compute_joint_reeval_window(0, 0, 0, 30, &segs, 14, 10);
        assert_eq!(s0, 0);
        assert!(e0 >= 10);
        assert!(e0 % 2 == 0);

        // 4. 游標在末尾 (cur = 30)
        let (sn, en) = compute_joint_reeval_window(0, 30, 0, 30, &segs, 14, 10);
        assert_eq!(en, 30);
        assert!(sn <= 16);
        assert!(sn % 2 == 0);

        // 5. 48 音節聯合重估視窗 (max_left = 28, max_right = 20)
        let segs_60: Vec<(usize, usize, String, String)> = (0..30)
            .map(|k| (k * 2, k * 2 + 2, String::new(), String::new()))
            .collect();
        let (s48, e48) = compute_joint_reeval_window(0, 36, 0, 60, &segs_60, 28, 20);
        assert!(s48 <= 36 && e48 >= 36);
        assert!(e48 - s48 <= 52);
        assert!(e48 - s48 >= 46);
        assert!(s48 % 2 == 0 && e48 % 2 == 0);

        let segs_48: Vec<(usize, usize, String, String)> = (0..24)
            .map(|k| (k * 2, k * 2 + 2, String::new(), String::new()))
            .collect();
        let (sf, ef) = compute_joint_reeval_window(0, 28, 0, 48, &segs_48, 28, 20);
        assert_eq!((sf, ef), (0, 48));
    }

    #[test]
    fn test_upgrade_an_before_ascii_key_in_buffers() {
        let pinned = std::collections::HashSet::new();

        // 1. 成功案例：「中案shift」➔「中按shift」
        let mut chars1: Vec<char> = "中案shift".chars().collect();
        let len1 = chars1.len();
        let syls1: Vec<Option<String>> = vec![
            Some("ㄓㄨㄥ".to_string()),
            Some("ㄢˋ".to_string()),
            None, None, None, None, None,
        ];
        let ok1 = upgrade_an_before_ascii_key_in_buffers(&mut chars1, &syls1, &[], &pinned, len1);
        assert!(ok1);
        assert_eq!(chars1.iter().collect::<String>(), "中按shift");

        // 2. 成功案例：「案enter」➔「按enter」
        let mut chars2: Vec<char> = "案enter".chars().collect();
        let len2 = chars2.len();
        let syls2: Vec<Option<String>> = vec![Some("ㄢˋ".to_string()), None, None, None, None, None];
        let ok2 = upgrade_an_before_ascii_key_in_buffers(&mut chars2, &syls2, &[], &pinned, len2);
        assert!(ok2);
        assert_eq!(chars2.iter().collect::<String>(), "按enter");

        // 3. 成功案例：「案 tab」➔「按 tab」（隔 1 空白）
        let mut chars3: Vec<char> = "案 tab".chars().collect();
        let len3 = chars3.len();
        let syls3: Vec<Option<String>> = vec![Some("ㄢˋ".to_string()), None, None, None, None];
        let ok3 = upgrade_an_before_ascii_key_in_buffers(&mut chars3, &syls3, &[], &pinned, len3);
        assert!(ok3);
        assert_eq!(chars3.iter().collect::<String>(), "按 tab");

        // 4. 成功案例：「案shift+enter」➔「按shift+enter」
        let mut chars4: Vec<char> = "案shift+enter".chars().collect();
        let len4 = chars4.len();
        let syls4: Vec<Option<String>> = vec![Some("ㄢˋ".to_string()), None, None, None, None, None, None, None, None, None, None, None];
        let ok4 = upgrade_an_before_ascii_key_in_buffers(&mut chars4, &syls4, &[], &pinned, len4);
        assert!(ok4);
        assert_eq!(chars4.iter().collect::<String>(), "按shift+enter");

        // 5. 對照組：「方案ctrl」保持不變（方案為既有名詞複合詞）
        let mut chars_ctrl1: Vec<char> = "方案ctrl".chars().collect();
        let len_ctrl1 = chars_ctrl1.len();
        let syls_ctrl1 = vec![Some("ㄈㄤ".to_string()), Some("ㄢˋ".to_string()), None, None, None, None];
        let ok_ctrl1 = upgrade_an_before_ascii_key_in_buffers(&mut chars_ctrl1, &syls_ctrl1, &[], &pinned, len_ctrl1);
        assert!(!ok_ctrl1);
        assert_eq!(chars_ctrl1.iter().collect::<String>(), "方案ctrl");

        // 6. 對照組：「檔案tab」保持不變（檔案為既有名詞複合詞）
        let mut chars_ctrl2: Vec<char> = "檔案tab".chars().collect();
        let len_ctrl2 = chars_ctrl2.len();
        let syls_ctrl2 = vec![Some("ㄉㄤˇ".to_string()), Some("ㄢˋ".to_string()), None, None, None];
        let ok_ctrl2 = upgrade_an_before_ascii_key_in_buffers(&mut chars_ctrl2, &syls_ctrl2, &[], &pinned, len_ctrl2);
        assert!(!ok_ctrl2);
        assert_eq!(chars_ctrl2.iter().collect::<String>(), "檔案tab");

        // 7. 對照組：釘選 '案' 保持不變
        let mut chars_pinned: Vec<char> = "案enter".chars().collect();
        let len_pinned = chars_pinned.len();
        let syls_pinned = vec![Some("ㄢˋ".to_string()), None, None, None, None, None];
        let mut pin_set = std::collections::HashSet::new();
        pin_set.insert(0); // 釘選第 0 字
        let ok_pinned = upgrade_an_before_ascii_key_in_buffers(&mut chars_pinned, &syls_pinned, &[], &pin_set, len_pinned);
        assert!(!ok_pinned);
        assert_eq!(chars_pinned.iter().collect::<String>(), "案enter");
    }

    #[test]
    fn test_tsf_com_abi_invariance_and_western_three_pillars_261115() {
        // 1. COM ABI 不變量：嚴格維持 RuhiTextService 既有 27 個 Arc<Mutex<...>> 欄位佈局（零欄位增減）
        assert_eq!(
            std::mem::size_of::<RuhiTextService>(),
            27 * std::mem::size_of::<usize>(),
            "RuhiTextService COM ABI 結構體大小不可變動"
        );

        let mut engine = RuhiEngine::new_without_core_lexicon(".");
        engine.set_keyboard_mode("english");
        let svc = ComObject::new(RuhiTextService {
            thread_mgr: Arc::new(Mutex::new(None)),
            client_id: Arc::new(Mutex::new(0)),
            engine: Arc::new(Mutex::new(engine)),
            composition: Arc::new(Mutex::new(None)),
            thread_mgr_sink_cookie: Arc::new(Mutex::new(0)),
            langbar_item: Arc::new(Mutex::new(None)),
            candidate_window: Arc::new(Mutex::new(None)),
            pending_compose: Arc::new(Mutex::new(String::new())),
            sentence_cursor: Arc::new(Mutex::new(0)),
            candidate_selecting: Arc::new(Mutex::new(false)),
            active_candidates: Arc::new(Mutex::new(Vec::new())),
            selected_candidate_idx: Arc::new(Mutex::new(0)),
            target_char_idx: Arc::new(Mutex::new(0)),
            target_range: Arc::new(Mutex::new((0, 0))),
            config: Arc::new(Mutex::new(RuhiConfig::default())),
            sel_anchor: Arc::new(Mutex::new(None)),
            insert_mode: Arc::new(Mutex::new(true)),
            q_press_count: Arc::new(Mutex::new(0)),
            q_last_char_idx: Arc::new(Mutex::new(None)),
            pending_syllables: Arc::new(Mutex::new(Vec::new())),
            pending_segments: Arc::new(Mutex::new(Vec::new())),
            last_raw_ascii: Arc::new(Mutex::new(None)),
            manual_cuts: Arc::new(Mutex::new(std::collections::HashSet::new())),
            pending_pinned: Arc::new(Mutex::new(std::collections::HashSet::new())),
            deleted_words: Arc::new(Mutex::new(std::collections::HashSet::new())),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
            redo_stack: Arc::new(Mutex::new(VecDeque::new())),
        });

        // 2. 歐文空白鍵 Tier-1 自動勘誤 + 單鍵 Backspace 還原與釘選防復發
        for ch in "teh".chars() {
            svc.insert_pending_char(ch, None);
        }
        assert!(svc.try_western_space_autocorrect_and_insert(' '));
        assert_eq!(*svc.pending_compose.lock().unwrap(), "the ");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 4);

        let rb = svc
            .take_last_western_autocorrect_if_matching()
            .expect("應具備完全吻合之單步還原快照");
        svc.restore_western_space_rollback(rb);
        assert_eq!(*svc.pending_compose.lock().unwrap(), "teh");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 3);
        assert!(svc.pending_pinned.lock().unwrap().contains(&0));
        assert!(
            !svc.try_western_space_autocorrect_and_insert(' '),
            "還原並釘選後再觸發空白鍵不得重複自動勘誤"
        );

        // 3. 原子化 Ctrl+Z (undo)：單次 undo 同時還原自動勘誤與空白鍵
        svc.clear_all_pending();
        for ch in "recieve".chars() {
            svc.insert_pending_char(ch, None);
        }
        assert!(svc.try_western_space_autocorrect_and_insert(' '));
        assert_eq!(*svc.pending_compose.lock().unwrap(), "receive ");
        assert!(svc.undo());
        assert_eq!(*svc.pending_compose.lock().unwrap(), "recieve");

        // 4. detect_up_arrow_target_and_cands 搭配詞、文法構式與雙向右文消歧檢查
        let (_ti1, _tr1, y_cands) = svc.detect_up_arrow_target_and_cands("thank y", 7);
        assert!(y_cands.contains(&"you".to_string()), "thank y 應前置 you: {:?}", y_cands);
        let (_ti2, _tr2, then_cands) = svc.detect_up_arrow_target_and_cands("more then", 9);
        assert_eq!(then_cands[0], "than", "more then 語境消歧首位應為 than: {:?}", then_cands);
        let (_ti3, _tr3, where_g) = svc.detect_up_arrow_target_and_cands("Where are you g", 15);
        assert_eq!(
            where_g.first().map(|s| s.as_str()),
            Some("going"),
            "Where are you g 進行式語境首位應為 going: {:?}",
            where_g
        );
        let (_ti4, _tr4, today_s) = svc.detect_up_arrow_target_and_cands("Today, we are going to s", 24);
        assert_eq!(
            today_s.first().map(|s| s.as_str()),
            Some("see"),
            "Today, we are going to s 不定詞語境首位應為 see: {:?}",
            today_s
        );
        let (_ti5, _tr5, their_are) = svc.detect_up_arrow_target_and_cands("their are", 5);
        assert_eq!(
            their_are.first().map(|s| s.as_str()),
            Some("there"),
            "their are 右文消歧首位應為 there: {:?}",
            their_are
        );
    }

    fn make_p1_t4_test_svc(mode: &str) -> ComObject<RuhiTextService> {
        let mut engine = RuhiEngine::new_without_core_lexicon(".");
        engine.set_keyboard_mode(mode);
        ComObject::new(RuhiTextService {
            thread_mgr: Arc::new(Mutex::new(None)),
            client_id: Arc::new(Mutex::new(0)),
            engine: Arc::new(Mutex::new(engine)),
            composition: Arc::new(Mutex::new(None)),
            thread_mgr_sink_cookie: Arc::new(Mutex::new(0)),
            langbar_item: Arc::new(Mutex::new(None)),
            candidate_window: Arc::new(Mutex::new(None)),
            pending_compose: Arc::new(Mutex::new(String::new())),
            sentence_cursor: Arc::new(Mutex::new(0)),
            candidate_selecting: Arc::new(Mutex::new(false)),
            active_candidates: Arc::new(Mutex::new(Vec::new())),
            selected_candidate_idx: Arc::new(Mutex::new(0)),
            target_char_idx: Arc::new(Mutex::new(0)),
            target_range: Arc::new(Mutex::new((0, 0))),
            config: Arc::new(Mutex::new(RuhiConfig::default())),
            sel_anchor: Arc::new(Mutex::new(None)),
            insert_mode: Arc::new(Mutex::new(true)),
            q_press_count: Arc::new(Mutex::new(0)),
            q_last_char_idx: Arc::new(Mutex::new(None)),
            pending_syllables: Arc::new(Mutex::new(Vec::new())),
            pending_segments: Arc::new(Mutex::new(Vec::new())),
            last_raw_ascii: Arc::new(Mutex::new(None)),
            manual_cuts: Arc::new(Mutex::new(std::collections::HashSet::new())),
            pending_pinned: Arc::new(Mutex::new(std::collections::HashSet::new())),
            deleted_words: Arc::new(Mutex::new(std::collections::HashSet::new())),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
            redo_stack: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    #[test]
    fn test_p1_t4_undo_pins_autocorrect_and_prevents_space_retrigger() {
        let svc = make_p1_t4_test_svc("english");
        for ch in "teh".chars() {
            svc.insert_pending_char(ch, None);
        }
        assert_eq!(*svc.pending_compose.lock().unwrap(), "teh");
        assert!(
            svc.try_western_space_autocorrect_and_insert(' '),
            "首次 teh + Space 應觸發 Tier 1 自動勘誤"
        );
        assert_eq!(*svc.pending_compose.lock().unwrap(), "the ");

        assert!(svc.undo(), "Ctrl+Z undo 應還原至 teh");
        assert_eq!(*svc.pending_compose.lock().unwrap(), "teh");
        assert!(
            !svc.try_western_space_autocorrect_and_insert(' '),
            "undo 還原後該詞區間須已固定 (pinned)，再按 Space 不得重新觸發自動勘誤"
        );

        // 深層 undo 堆疊（在勘誤後繼續推送快照，多步 undo 回該層仍應保留固定保護）
        svc.clear_all_pending();
        for ch in "teh".chars() {
            svc.insert_pending_char(ch, None);
        }
        assert!(svc.try_western_space_autocorrect_and_insert(' '));
        svc.insert_pending_char('a', None);
        svc.insert_pending_char('b', None);
        assert!(svc.undo());
        assert!(svc.undo());
        assert!(svc.undo());
        assert_eq!(*svc.pending_compose.lock().unwrap(), "teh");
        assert!(
            !svc.try_western_space_autocorrect_and_insert(' '),
            "跨多層快照 undo 回到勘誤前狀態時，仍須防止再次自動勘誤"
        );
    }

    #[test]
    fn test_p1_t4_leading_sigil_passthrough_blocks_space_autocorrect() {
        let svc = make_p1_t4_test_svc("english");
        for sigil in ['/', '.', '_'] {
            svc.clear_all_pending();
            assert!(
                !svc.classify_english_key_translation(
                    false,
                    false,
                    false,
                    false,
                    true,
                    &EnglishVkTranslation::Text(sigil.to_string()),
                ),
                "空白狀態輸入邊界前綴 '{sigil}' 應直接放行至宿主視窗"
            );
            for ch in "teh".chars() {
                svc.insert_pending_char(ch, None);
            }
            assert!(
                !svc.try_western_space_autocorrect_and_insert(' '),
                "前導直通 '{sigil}' + teh 後按 Space 不得誤修正為 '{sigil}the '"
            );
        }

        // 下一詞（已消費前導直通字元後）應恢復正常自動勘誤
        svc.clear_all_pending();
        for ch in "teh".chars() {
            svc.insert_pending_char(ch, None);
        }
        assert!(
            svc.try_western_space_autocorrect_and_insert(' '),
            "未緊鄰前導邊界字元之獨立 teh + Space 應恢復正常勘誤"
        );
        assert_eq!(*svc.pending_compose.lock().unwrap(), "the ");

        // 密碼欄位不得記錄前導字元
        svc.clear_all_pending();
        assert!(!svc.classify_english_key_translation(
            false,
            false,
            false,
            true,
            true,
            &EnglishVkTranslation::Text("/".to_string()),
        ));
        assert_eq!(take_western_leading_sigil(svc.western_instance_key()), None);
    }

    #[test]
    fn test_p1_t4_english_layout_translation_altgr_deadkey_and_western_languages() {
        let svc = make_p1_t4_test_svc("english");
        let hkl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout(0) };

        assert_eq!(
            translate_vk_in_english_layout_with_hkl(
                0x41,
                false,
                false,
                false,
                false,
                hkl,
                Some("00000409"),
                false,
            ),
            EnglishVkTranslation::Text("a".to_string())
        );
        assert_eq!(
            translate_vk_in_english_layout_with_hkl(
                0x41,
                false,
                false,
                true,
                false,
                hkl,
                Some("00000409"),
                false,
            ),
            EnglishVkTranslation::Text("A".to_string()),
            "capslock_switch=none 下有效 CapsLock 應套用大小寫反轉"
        );
        assert_eq!(
            translate_vk_in_english_layout_with_hkl(
                0x41,
                true,
                false,
                true,
                false,
                hkl,
                Some("00000409"),
                false,
            ),
            EnglishVkTranslation::Text("a".to_string())
        );
        assert_eq!(
            translate_vk_in_english_layout_with_hkl(
                0xC0,
                false,
                false,
                false,
                false,
                hkl,
                Some("00000409"),
                false,
            ),
            EnglishVkTranslation::Text("`".to_string())
        );
        // 呼叫實際 Win32 ToUnicodeEx 路徑（不經 US fast-path）驗證安全與非破壞性探測
        assert!(matches!(
            translate_vk_in_english_layout_with_hkl(0x41, false, false, false, false, hkl, None, false),
            EnglishVkTranslation::Text(_)
        ));

        // Dead key 與歐陸鍵盤字母分類驗證
        assert!(!svc.classify_english_key_translation(
            false,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::DeadKey,
        ));
        assert!(svc.classify_english_key_translation(
            true,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::DeadKey,
        ));
        assert!(svc.classify_english_key_translation(
            false,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("ö".to_string()),
        ));
        assert!(svc.classify_english_key_translation(
            false,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("ù".to_string()),
        ));
        assert!(svc.classify_english_key_translation(
            false,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("é".to_string()),
        ));
        assert!(!svc.classify_english_key_translation(
            false,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("<".to_string()),
        ));
        assert!(svc.classify_english_key_translation(
            true,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("<".to_string()),
        ));
        assert!(!svc.classify_english_key_translation(
            false,
            false,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("~".to_string()),
        ));
        assert!(svc.classify_english_key_translation(
            false,
            true,
            false,
            false,
            true,
            &EnglishVkTranslation::Text("~".to_string()),
        ));

        // 非歐文模式下即使 is_altgr=true 也應完全放行
        {
            svc.engine.lock().unwrap().set_keyboard_mode("hsu");
        }
        assert!(!svc.evaluate_test_key_down_full(0x41, true, true, false, false, true, false, false));

        // RuhiConfig::western_languages 預設與同步至 RuhiEngine 驗證
        let mut cfg = RuhiConfig::default();
        assert_eq!(cfg.western_languages, vec!["en".to_string()]);
        cfg.western_languages = vec!["en".to_string(), "sa_am".to_string()];
        {
            let mut eng = svc.engine.lock().unwrap();
            apply_config_to_engine(&mut eng, &cfg);
            assert!(eng.western.is_language_enabled("en"));
            assert!(eng.western.is_language_enabled("sa_am"));
        }
    }

    #[test]
    fn test_p1_t4_is_latin_mode_and_unicode_diacritic_up_arrow_boundaries() {
        let svc = make_p1_t4_test_svc("hsu");
        {
            let mut eng = svc.engine.lock().unwrap();
            eng.is_latin_mode = true;
        }
        for ch in "thank y".chars() {
            svc.insert_pending_char(ch, None);
        }
        let (_ti, tr, cands) = svc.detect_up_arrow_target_and_cands("thank y", 7);
        assert_eq!(tr, (6, 7));
        assert!(
            cands.first().map(|s| s.as_str()) == Some("you"),
            "暫時切換拉丁模式 (is_latin_mode) 下 ↑ 應觸發歐文搭配詞補全: {:?}",
            cands
        );

        let (_t_idx, range, _) = svc.detect_up_arrow_target_and_cands("café nāma", 4);
        assert_eq!(
            range,
            (0, 4),
            "Unicode 附標字母邊界應完整辨識 café（不於附標處斷開）"
        );
    }

    #[test]
    fn test_shortcut_passthrough_and_input_desk_combos_all_modes_261115() {
        // 1. COM ABI 不變量：嚴格維持 RuhiTextService 既有 27 個欄位佈局
        assert_eq!(
            std::mem::size_of::<RuhiTextService>(),
            27 * std::mem::size_of::<usize>(),
            "RuhiTextService COM ABI 結構體大小不可變動"
        );

        let mut engine = RuhiEngine::new_without_core_lexicon(".");
        engine.set_keyboard_mode("english");
        let svc = ComObject::new(RuhiTextService {
            thread_mgr: Arc::new(Mutex::new(None)),
            client_id: Arc::new(Mutex::new(0)),
            engine: Arc::new(Mutex::new(engine)),
            composition: Arc::new(Mutex::new(None)),
            thread_mgr_sink_cookie: Arc::new(Mutex::new(0)),
            langbar_item: Arc::new(Mutex::new(None)),
            candidate_window: Arc::new(Mutex::new(None)),
            pending_compose: Arc::new(Mutex::new(String::new())),
            sentence_cursor: Arc::new(Mutex::new(0)),
            candidate_selecting: Arc::new(Mutex::new(false)),
            active_candidates: Arc::new(Mutex::new(Vec::new())),
            selected_candidate_idx: Arc::new(Mutex::new(0)),
            target_char_idx: Arc::new(Mutex::new(0)),
            target_range: Arc::new(Mutex::new((0, 0))),
            config: Arc::new(Mutex::new(RuhiConfig::default())),
            sel_anchor: Arc::new(Mutex::new(None)),
            insert_mode: Arc::new(Mutex::new(true)),
            q_press_count: Arc::new(Mutex::new(0)),
            q_last_char_idx: Arc::new(Mutex::new(None)),
            pending_syllables: Arc::new(Mutex::new(Vec::new())),
            pending_segments: Arc::new(Mutex::new(Vec::new())),
            last_raw_ascii: Arc::new(Mutex::new(None)),
            manual_cuts: Arc::new(Mutex::new(std::collections::HashSet::new())),
            pending_pinned: Arc::new(Mutex::new(std::collections::HashSet::new())),
            deleted_words: Arc::new(Mutex::new(std::collections::HashSet::new())),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
            redo_stack: Arc::new(Mutex::new(VecDeque::new())),
        });

        let all_modes: Vec<&'static str> = ruhi::engine::ALL_MODES.iter().map(|d| d.id).collect();

        // 2. 無組字狀態 (!has_buffer) 下，全體 10 種輸入模式皆必須無條件放行所有通用應用程式快捷鍵
        let ctrl_letters: [u32; 18] = [
            0x56, // Ctrl+V
            0x43, // Ctrl+C
            0x58, // Ctrl+X
            0x5A, // Ctrl+Z
            0x59, // Ctrl+Y
            0x41, // Ctrl+A
            0x53, // Ctrl+S
            0x46, // Ctrl+F
            0x48, // Ctrl+H
            0x4E, // Ctrl+N
            0x4F, // Ctrl+O
            0x50, // Ctrl+P
            0x57, // Ctrl+W
            0x54, // Ctrl+T
            0x52, // Ctrl+R
            0x42, // Ctrl+B
            0x49, // Ctrl+I
            0x55, // Ctrl+U
        ];

        let ctrl_digits_and_nav: [u32; 22] = [
            0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0xBB, // Ctrl+=
            0xBD, // Ctrl+-
            0xBF, // Ctrl+/
            0x60, 0x6B, 0x6D, // Ctrl+Numpad 0/+/−
            0x09, // Ctrl+Tab
            0x2D, // Ctrl+Insert
            0x08, // Ctrl+Backspace
            0x2E, // Ctrl+Delete
            0x25, 0x24, // Ctrl+Left, Ctrl+Home
        ];

        let idle_non_typing_vks: [u32; 15] = [
            0x2D, // Insert
            0x2E, // Delete
            0x08, // Backspace
            0x0D, // Enter
            0x09, // Tab
            0x1B, // Escape
            0x25, 0x26, 0x27, 0x28, // Arrows
            0x24, 0x23, 0x21, 0x22, // Home, End, PgUp, PgDn
            0x20, // Halfwidth Space
        ];

        let standalone_mods: [u32; 12] = [
            0x10, 0x11, 0x12, 0x13, 0x5B, 0x5C, 0x5D, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4,
        ];

        let all_ctrl_syms: [(u32, bool); 18] = [
            (0xBE, false), (0xBE, true), // Ctrl+. / Ctrl+Shift+.
            (0xBC, false), (0xBC, true), // Ctrl+, / Ctrl+Shift+,
            (0xDB, false), (0xDB, true), // Ctrl+[ / Ctrl+Shift+[
            (0xDD, false), (0xDD, true), // Ctrl+] / Ctrl+Shift+]
            (0xDC, false), (0xDC, true), // Ctrl+\ / Ctrl+Shift+\
            (0xC0, false), (0xC0, true), // Ctrl+` / Ctrl+Shift+`
            (0xBA, false), (0xBA, true), // Ctrl+; / Ctrl+Shift+;
            (0xDE, false), (0xDE, true), // Ctrl+' / Ctrl+Shift+'
            (0x31, false), (0xBD, false), // Ctrl+1 / Ctrl+-
        ];

        for mode in all_modes {
            svc.clear_all_pending();
            svc.engine.lock().unwrap().reset_buffer();
            svc.engine.lock().unwrap().set_keyboard_mode(mode);
            let (is_en, is_fc) = (mode == "english", mode == "four_corner");

            for &vk in &ctrl_letters {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, true, false, false, false, false, false),
                    "模式 {mode} 空白狀態下 Ctrl+{vk:#x} 應直接放行"
                );
            }
            for vk in [0x56u32, 0x43, 0x5A] {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, true, false, true, false, false, false),
                    "模式 {mode} 空白狀態下 Ctrl+Shift+{vk:#x} 應直接放行"
                );
            }
            for &vk in &ctrl_digits_and_nav {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, true, false, false, false, false, false),
                    "模式 {mode} 空白狀態下 Ctrl+{vk:#x} 應直接放行"
                );
            }
            for vk in [0xBDu32, 0xBF, 0x09] {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, true, false, true, false, false, false),
                    "模式 {mode} 空白狀態下 Ctrl+Shift+{vk:#x} 應直接放行"
                );
            }
            for &(vk, shift) in &all_ctrl_syms {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, true, false, shift, false, false, false),
                    "模式 {mode} 無組字狀態下 Ctrl+符號 ({vk:#x}, shift={shift}) 應一致直接放行"
                );
                assert_eq!(
                    resolve_ctrl_symbol_combo(vk, shift, false, is_en, is_fc),
                    None,
                    "模式 {mode} 無組字狀態下 resolve_ctrl_symbol_combo 應一律回傳 None"
                );
            }
            for &vk in &[0x26u32, 0x27, 0x23, 0x21, 0x22] {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, true, false, false, false, false, false),
                    "模式 {mode} 空白狀態下 Ctrl+{vk:#x} 應直接放行"
                );
            }

            // 無組字狀態下單按 `` ` `` (0xC0, shift=false) 全體 10 種模式一致進入「🔖 自訂詞庫」
            assert!(
                svc.evaluate_test_key_down_with_mods(0xC0, false, false, false, false, false, false),
                "模式 {mode} 無組字狀態下單按 ` 應一致進入自訂詞庫"
            );

            // 全體 10 種模式一致支援 Caps Lock / Shift+Caps / Ctrl+Caps / Shift+Space（且於密碼欄一律放行）
            assert!(svc.evaluate_test_key_down_with_mods(0x14, false, false, false, false, false, false));
            assert!(svc.evaluate_test_key_down_with_mods(0x14, false, false, true, false, false, false));
            assert!(svc.evaluate_test_key_down_with_mods(0x14, true, false, false, false, false, false));
            assert!(svc.evaluate_test_key_down_with_mods(0x20, false, false, true, false, false, false));
            assert!(!svc.evaluate_test_key_down_with_mods(0x14, false, false, false, false, true, false));
            assert!(!svc.evaluate_test_key_down_with_mods(0x14, false, false, true, false, true, false));
            assert!(!svc.evaluate_test_key_down_with_mods(0x14, true, false, false, false, true, false));
            assert!(!svc.evaluate_test_key_down_with_mods(0x20, false, false, true, false, true, false));

            // Shift+VK_DECIMAL 與 Shift+Numpad 0..9 無組字狀態下全模式皆應一致放行
            assert!(
                !svc.evaluate_test_key_down_with_mods(0x6E, false, false, true, false, false, false),
                "模式 {mode} 無組字狀態下 Shift+NumpadDecimal 應放行"
            );
            assert!(
                !svc.evaluate_test_key_down_with_mods(0x60, false, false, true, false, false, false),
                "模式 {mode} 無組字狀態下 Shift+Numpad0 應放行"
            );

            // Shift+Insert / Shift+Delete / Shift+Tab / Shift+Enter / Shift+Arrows / Shift+Home..PgDn
            for &vk in &idle_non_typing_vks {
                assert!(
                    !svc.evaluate_test_key_down_with_mods(vk, false, false, false, false, false, false),
                    "模式 {mode} 空白狀態下 {vk:#x} 應直接放行"
                );
                if vk != 0x20 {
                    assert!(
                        !svc.evaluate_test_key_down_with_mods(vk, false, false, true, false, false, false),
                        "模式 {mode} 空白狀態下 Shift+{vk:#x} 應直接放行"
                    );
                }
            }

            // Alt / Ctrl+Alt (AltGr) / Win / F1..F24 / 獨立修飾鍵：無論是否有組字皆直接放行
            for has_buf in [false, true] {
                if has_buf {
                    svc.insert_pending_char('a', None);
                }
                assert!(!svc.evaluate_test_key_down_with_mods(0x56, false, true, false, false, false, false)); // Alt+V
                assert!(!svc.evaluate_test_key_down_with_mods(0x20, false, true, false, false, false, false)); // Alt+Space
                assert!(!svc.evaluate_test_key_down_with_mods(0x0D, false, true, false, false, false, false)); // Alt+Enter
                assert!(!svc.evaluate_test_key_down_with_mods(0x73, false, true, false, false, false, false)); // Alt+F4
                assert!(!svc.evaluate_test_key_down_with_mods(0x25, false, true, false, false, false, false)); // Alt+Left
                assert!(!svc.evaluate_test_key_down_with_mods(0x44, false, true, false, false, false, false)); // Alt+D
                assert!(!svc.evaluate_test_key_down_with_mods(0x41, true, true, false, false, false, false)); // Ctrl+Alt+A (AltGr)
                assert!(!svc.evaluate_test_key_down_with_mods(0x56, false, false, false, true, false, false)); // Win+V
                assert!(!svc.evaluate_test_key_down_with_mods(0xBE, false, false, false, true, false, false)); // Win+.
                assert!(!svc.evaluate_test_key_down_with_mods(0xBA, false, false, false, true, false, false)); // Win+;
                assert!(!svc.evaluate_test_key_down_with_mods(0x53, false, false, true, true, false, false)); // Win+Shift+S
                assert!(!svc.evaluate_test_key_down_with_mods(0x20, false, false, false, true, false, false)); // Win+Space
                for fvk in 0x70u32..=0x87 {
                    assert!(!svc.evaluate_test_key_down_with_mods(fvk, false, false, false, false, false, false));
                }
                assert!(!svc.evaluate_test_key_down_with_mods(0x79, false, false, true, false, false, false)); // Shift+F10
                assert!(!svc.evaluate_test_key_down_with_mods(0x73, true, false, false, false, false, false)); // Ctrl+F4
                assert!(!svc.evaluate_test_key_down_with_mods(0x74, true, false, false, false, false, false)); // Ctrl+F5
                for &mvk in &standalone_mods {
                    assert!(!svc.evaluate_test_key_down_with_mods(mvk, false, false, false, false, false, false));
                }
            }
        }

        // 2b. 標準大千注音：上方數字聲符 (0,1,2,5,7,8,9) 需攔截組字，聲調 (3,4,6) 空白時放行，右側九宮格 0..9 半形空白時放行
        svc.clear_all_pending();
        svc.engine.lock().unwrap().reset_buffer();
        svc.engine.lock().unwrap().set_keyboard_mode("standard");
        for vk in [0x30u32, 0x31, 0x32, 0x35, 0x37, 0x38, 0x39] {
            assert!(
                svc.evaluate_test_key_down_with_mods(vk, false, false, false, false, false, false),
                "大千模式主鍵盤數字注音碼 {vk:#x} 應進入組字"
            );
        }
        for vk in [0x33u32, 0x34, 0x36] {
            assert!(
                !svc.evaluate_test_key_down_with_mods(vk, false, false, false, false, false, false),
                "大千模式空白狀態主鍵盤聲調鍵 {vk:#x} 應放行"
            );
        }
        for vk in 0x60u32..=0x69 {
            assert!(
                !svc.evaluate_test_key_down_with_mods(vk, false, false, false, false, false, false),
                "大千半形空白狀態九宮格數字鍵 {vk:#x} 應放行"
            );
        }

        // 3. 有組字狀態下之 Ctrl+符號與歐文模式 `` ` `` / `~` 狀態轉換行為
        let active_ctrl_syms: [(u32, bool); 10] = [
            (0xBE, false), // Ctrl+.
            (0xBC, false), // Ctrl+,
            (0xDB, false), // Ctrl+[
            (0xDD, false), // Ctrl+]
            (0xDC, false), // Ctrl+\
            (0xC0, false), // Ctrl+`
            (0xDB, true),  // Ctrl+Shift+[
            (0x31, false), // Ctrl+1
            (0xBD, false), // Ctrl+-
            (0xBF, false), // Ctrl+/
        ];
        for mode in [
            "hsu",
            "standard",
            "et26",
            "cangjie",
            "array30",
            "four_corner",
            "longkan",
            "japanese",
            "korean",
        ] {
            svc.clear_all_pending();
            svc.engine.lock().unwrap().reset_buffer();
            svc.engine.lock().unwrap().set_keyboard_mode(mode);
            svc.insert_pending_char('測', None);
            let is_fc = mode == "four_corner";
            for &(vk, shift) in &active_ctrl_syms {
                assert!(
                    svc.evaluate_test_key_down_with_mods(vk, true, false, shift, false, false, false),
                    "模式 {mode} 有組字狀態下 Ctrl+符號 ({vk:#x}, shift={shift}) 應攔截處理"
                );
                assert!(
                    resolve_ctrl_symbol_combo(vk, shift, true, false, is_fc).is_some(),
                    "模式 {mode} 有組字狀態下 resolve_ctrl_symbol_combo 應回傳 Some"
                );
            }
        }
        // 歐文模式有組字期間：resolve_ctrl_symbol_combo 一律為 None（僅防禦組字不輸出中文標點）
        svc.clear_all_pending();
        svc.engine.lock().unwrap().reset_buffer();
        svc.engine.lock().unwrap().set_keyboard_mode("english");
        svc.insert_pending_char('a', None);
        for &(vk, shift) in &active_ctrl_syms {
            assert_eq!(resolve_ctrl_symbol_combo(vk, shift, true, true, false), None);
            assert!(svc.evaluate_test_key_down_with_mods(vk, true, false, shift, false, false, false));
        }
        // 歐文模式一般組字中（非自訂選單子狀態）按 `` ` `` 維持於輸入座內插入半形反引號
        assert!(svc.evaluate_test_key_down_with_mods(0xC0, false, false, false, false, false, false));

        // 歐文模式無組字按 `` ` `` 進入三模式輪切與 `~` 統一進數字快打，及預設 punct ~ 直出驗證
        svc.clear_all_pending();
        {
            let mut eng = svc.engine.lock().unwrap();
            eng.reset_buffer();
            eng.set_keyboard_mode("english");
            eng.tilde_toggle_mode = "punct".to_string();
        }
        // 預設 punct 下單按 `~` 直接放行直出半形 ~；若設為輔助輸入法對切則攔截處理
        assert!(!svc.evaluate_test_key_down_with_mods(0xC0, false, false, true, false, false, false));
        {
            svc.engine.lock().unwrap().tilde_toggle_mode = "longkan".to_string();
        }
        assert!(svc.evaluate_test_key_down_with_mods(0xC0, false, false, true, false, false, false));
        {
            let mut eng = svc.engine.lock().unwrap();
            eng.tilde_toggle_mode = "punct".to_string();
            eng.reset_buffer();
            eng.process_key('`');
            assert!(eng.inline_longkan_active);
            eng.process_key('`');
            assert_eq!(eng.mode, ruhi::engine::EngineMode::TemplateSelect);
            eng.process_key('`');
            assert_eq!(eng.mode, ruhi::engine::EngineMode::LexiconSwitch);
        }
        assert!(
            svc.evaluate_test_key_down_with_mods(0xC0, false, false, true, false, false, false),
            "歐文模式下進入五大詞庫時按 ~ (Shift+`) 必須由輸入法接手進入數字快打"
        );
        {
            let mut eng = svc.engine.lock().unwrap();
            let r_qn = eng.process_key('~');
            assert_eq!(r_qn.action, "show_quick_num");
            assert_eq!(eng.mode, ruhi::engine::EngineMode::QuickNum);
        }

        // 4. 輸入座內編輯輔助函式驗證：貼上換行標準化／原子覆寫選區／單步撤銷、複製轉 CRLF、Ctrl+Delete 向後刪詞
        svc.clear_all_pending();
        for ch in "abCD".chars() {
            svc.insert_pending_char(ch, None);
        }
        *svc.sel_anchor.lock().unwrap() = Some(1);
        *svc.sentence_cursor.lock().unwrap() = 3;
        assert!(svc.paste_text_into_desk("第一行\r\n第二行"));
        assert_eq!(*svc.pending_compose.lock().unwrap(), "a第一行\n第二行D");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 8);
        assert_eq!(*svc.sel_anchor.lock().unwrap(), None);
        assert!(svc.undo());
        assert_eq!(*svc.pending_compose.lock().unwrap(), "abCD");

        assert_eq!(normalize_clipboard_copy_text("甲\n乙"), "甲\r\n乙");

        svc.clear_all_pending();
        for ch in "hello world".chars() {
            svc.insert_pending_char(ch, None);
        }
        *svc.sentence_cursor.lock().unwrap() = 0;
        assert!(svc.delete_forward_in_desk(true));
        assert_eq!(*svc.pending_compose.lock().unwrap(), " world");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 0);
        assert!(svc.undo());
        assert_eq!(*svc.pending_compose.lock().unwrap(), "hello world");

        // 5. 反斜線轉義路由、CapsLock 不接管 ("none") 與單模式獨立運作路由驗證
        svc.clear_all_pending();
        {
            let mut eng = svc.engine.lock().unwrap();
            eng.reset_buffer();
            eng.set_keyboard_mode("english");
        }
        assert!(
            svc.evaluate_test_key_down_with_mods(0xDC, false, false, false, false, false, false),
            "歐文模式空白狀態按未帶 Shift 的 \\ (0xDC) 必須進入轉義前導待命"
        );
        assert!(
            !svc.evaluate_test_key_down_with_mods(0xDC, false, false, true, false, false, false),
            "歐文模式空白狀態按 Shift+\\ (|) 仍須直接放行"
        );
        {
            let mut eng = svc.engine.lock().unwrap();
            eng.reset_buffer();
            eng.set_keyboard_mode("four_corner");
        }
        assert!(
            svc.evaluate_test_key_down_with_mods(0xDC, false, false, false, false, false, false),
            "四角號碼模式空白狀態按 \\ (0xDC) 必須支援轉義輸入"
        );
        {
            let mut cfg = svc.config.lock().unwrap();
            cfg.capslock_switch = "none".to_string();
        }
        assert!(
            !svc.evaluate_test_key_down_with_mods(0x14, false, false, false, false, false, false),
            "capslock_switch=none 時單按 CapsLock 必須直接放行還原系統原生大寫鎖定"
        );
        assert!(
            svc.evaluate_test_key_down_with_mods(0x14, true, false, false, false, false, false),
            "capslock_switch=none 但多模式啟用時 Ctrl+CapsLock 仍可循環切換"
        );
        {
            let mut cfg = svc.config.lock().unwrap();
            cfg.capslock_switch = "capital_lock".to_string();
            let mut eng = svc.engine.lock().unwrap();
            eng.set_enabled_keyboard_modes(vec!["cangjie".to_string()]);
        }
        assert!(
            !svc.evaluate_test_key_down_with_mods(0x14, false, false, false, false, false, false),
            "僅單獨掛載一種鍵盤模式時 CapsLock 應直接放行，不強迫切換至歐文或許氏"
        );
    }

    #[test]
    fn test_layer_a_abi_layout_hash_nonzero_and_deterministic() {
        let h1 = compute_layer_a_abi_hash();
        let h2 = crate::ruhi_tsf_abi_layout_hash();
        assert_ne!(h1, 0);
        assert_eq!(h1, h2);
        assert_eq!(std::mem::size_of::<RuhiTextService>(), 27 * std::mem::size_of::<usize>());
        assert!(std::mem::needs_drop::<ruhi::western::WesternEngine>());
        assert!(std::mem::needs_drop::<RuhiTextService>());
    }

    #[test]
    fn test_layer_b_undo_vecdeque_ring_cap_and_drop_cleanup() {
        let svc = make_p1_t4_test_svc("hsu");
        for i in 0..105usize {
            *svc.pending_compose.lock().unwrap() = format!("s{i}");
            *svc.sentence_cursor.lock().unwrap() = i;
            svc.push_undo();
        }
        {
            let undo = svc.undo_stack.lock().unwrap();
            assert_eq!(undo.len(), 100, "undo_stack VecDeque 必須嚴格維持上限 100 筆");
            assert_eq!(undo.front().map(|s| s.pending.as_str()), Some("s5"));
            assert_eq!(undo.back().map(|s| s.pending.as_str()), Some("s104"));
        }
        assert!(svc.undo());
        assert_eq!(*svc.pending_compose.lock().unwrap(), "s104");

        let inst_key = Arc::as_ptr(&svc.client_id) as usize;
        let comp_key = Arc::as_ptr(&svc.composition) as usize;
        {
            let mut guard = CURRENT_DLL_INSTANCES.lock().unwrap();
            guard
                .get_or_insert_with(std::collections::HashSet::new)
                .insert(inst_key);
        }
        let (e1, f1) = current_composition_epoch(&svc.composition);
        assert!(is_composition_epoch_valid(comp_key, e1, f1, true));

        drop(svc);
        assert!(!is_composition_epoch_valid(comp_key, e1, f1, true));
        let guard = CURRENT_DLL_INSTANCES.lock().unwrap();
        assert!(
            !guard.as_ref().is_some_and(|set| set.contains(&inst_key)),
            "RuhiTextService Drop 後必須清除 CURRENT_DLL_INSTANCES 條目"
        );
    }

    #[test]
    fn test_caret_unprobed_sentinel_and_negative_multimonitor_pos() {
        set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);
        assert_eq!(get_last_caret_pos(), (CARET_UNPROBED, CARET_UNPROBED));
        set_last_caret_pos(-1280, -340);
        assert_eq!(get_last_caret_pos(), (-1280, -340));
        set_last_caret_pos(CARET_UNPROBED, CARET_UNPROBED);
    }

    #[test]
    fn test_single_key_password_token_consume_and_expiry() {
        clear_tested_pwd_token();
        store_tested_pwd_token_at(0xCAFE_u32 as usize, 0x41, std::time::Instant::now(), true);
        assert_eq!(take_valid_tested_pwd_token(0xBEEF_u32 as usize, 0x41), None);
        assert_eq!(take_valid_tested_pwd_token(0xCAFE_u32 as usize, 0x41), None);

        store_tested_pwd_token_at(0xCAFE_u32 as usize, 0x41, std::time::Instant::now(), true);
        assert_eq!(take_valid_tested_pwd_token(0xCAFE_u32 as usize, 0x41), Some(true));
        assert_eq!(take_valid_tested_pwd_token(0xCAFE_u32 as usize, 0x41), None);

        let expired = std::time::Instant::now() - std::time::Duration::from_millis(80);
        store_tested_pwd_token_at(0xCAFE_u32 as usize, 0x42, expired, false);
        assert_eq!(take_valid_tested_pwd_token(0xCAFE_u32 as usize, 0x42), None);
    }

    #[test]
    fn test_composition_epoch_async_preedit_and_focus_guard() {
        let holder = Arc::new(Mutex::new(None));
        let ptr = Arc::as_ptr(&holder) as usize;
        let (e1, f1) = current_composition_epoch(&holder);
        assert!(is_composition_epoch_valid(ptr, e1, f1, false));
        assert!(is_composition_epoch_valid(ptr, e1, f1, true));

        bump_composition_edit_epoch(&holder);
        assert!(!is_composition_epoch_valid(ptr, e1, f1, false));
        assert!(is_composition_epoch_valid(ptr, e1, f1, true));

        let (e2, f2) = current_composition_epoch(&holder);
        assert!(is_composition_epoch_valid(ptr, e2, f2, false));

        bump_composition_focus_epoch(&holder, None);
        assert!(!is_composition_epoch_valid(ptr, e2, f2, false));
        assert!(!is_composition_epoch_valid(ptr, e2, f2, true));

        remove_composition_epoch(ptr);
        assert!(!is_composition_epoch_valid(ptr, e2, f2, true));
    }

    #[test]
    fn test_symbol_tree_ctrl_alt_comma_wrap_and_tab_semantic_lookup_261117() {
        assert_eq!(
            std::mem::size_of::<RuhiTextService>(),
            27 * std::mem::size_of::<usize>(),
            "RuhiTextService COM ABI 結構體大小不可變動"
        );

        let mut engine = RuhiEngine::new_without_core_lexicon(".");
        engine.set_keyboard_mode("hsu");
        let svc = ComObject::new(RuhiTextService {
            thread_mgr: Arc::new(Mutex::new(None)),
            client_id: Arc::new(Mutex::new(0)),
            engine: Arc::new(Mutex::new(engine)),
            composition: Arc::new(Mutex::new(None)),
            thread_mgr_sink_cookie: Arc::new(Mutex::new(0)),
            langbar_item: Arc::new(Mutex::new(None)),
            candidate_window: Arc::new(Mutex::new(None)),
            pending_compose: Arc::new(Mutex::new(String::new())),
            sentence_cursor: Arc::new(Mutex::new(0)),
            candidate_selecting: Arc::new(Mutex::new(false)),
            active_candidates: Arc::new(Mutex::new(Vec::new())),
            selected_candidate_idx: Arc::new(Mutex::new(0)),
            target_char_idx: Arc::new(Mutex::new(0)),
            target_range: Arc::new(Mutex::new((0, 0))),
            config: Arc::new(Mutex::new(RuhiConfig::default())),
            sel_anchor: Arc::new(Mutex::new(None)),
            insert_mode: Arc::new(Mutex::new(true)),
            q_press_count: Arc::new(Mutex::new(0)),
            q_last_char_idx: Arc::new(Mutex::new(None)),
            pending_syllables: Arc::new(Mutex::new(Vec::new())),
            pending_segments: Arc::new(Mutex::new(Vec::new())),
            last_raw_ascii: Arc::new(Mutex::new(None)),
            manual_cuts: Arc::new(Mutex::new(std::collections::HashSet::new())),
            pending_pinned: Arc::new(Mutex::new(std::collections::HashSet::new())),
            deleted_words: Arc::new(Mutex::new(std::collections::HashSet::new())),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
            redo_stack: Arc::new(Mutex::new(VecDeque::new())),
        });

        // 1. Ctrl + Alt + , 熱鍵路由（中文模式／歐文左鍵放行、歐文 AltGr 不攔截、密碼欄位隔離）
        assert!(is_symbol_tree_hotkey_combo(0xBC, true, true, false, false, false, false));
        assert!(is_symbol_tree_hotkey_combo(0xBC, true, true, false, false, false, true));
        assert!(!is_symbol_tree_hotkey_combo(0xBC, true, true, false, false, true, true));
        assert!(!is_symbol_tree_hotkey_combo(0xBC, true, true, true, false, false, false));

        assert!(svc.evaluate_test_key_down_full(0xBC, true, true, false, false, false, false, false));
        assert!(
            !svc.evaluate_test_key_down_full(0xBC, true, true, false, false, false, true, false),
            "空白狀態於密碼欄位按 Ctrl+Alt+, 必須維持隱私隔離放行"
        );
        {
            svc.engine.lock().unwrap().set_keyboard_mode("english");
        }
        assert!(svc.evaluate_test_key_down_full(0xBC, true, true, false, false, false, false, false));
        assert!(
            !svc.evaluate_test_key_down_full(0xBC, true, true, false, false, true, false, false),
            "歐文模式 AltGr+, 不可攔截"
        );
        {
            svc.engine.lock().unwrap().set_keyboard_mode("hsu");
        }

        // 2. Shift+選取範圍包夾（重要通知 -> 【重要通知】）與單步復原
        svc.clear_all_pending();
        for ch in "重要通知".chars() {
            svc.insert_pending_char(ch, Some("ㄗ".to_string()));
        }
        *svc.sel_anchor.lock().unwrap() = Some(0);
        *svc.sentence_cursor.lock().unwrap() = 4;
        assert!(!svc.commit_symbol_into_pending("【】", true));
        assert_eq!(*svc.pending_compose.lock().unwrap(), "【重要通知】");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 6);
        assert_eq!(*svc.sel_anchor.lock().unwrap(), None);
        assert!(svc.undo());
        assert_eq!(*svc.pending_compose.lock().unwrap(), "重要通知");

        // 3. 空白狀態插入成對括號：游標置中於 【|】 且不殘留音節
        svc.clear_all_pending();
        assert!(!svc.commit_symbol_into_pending("【】", true));
        assert_eq!(*svc.pending_compose.lock().unwrap(), "【】");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 1);
        assert_eq!(*svc.pending_syllables.lock().unwrap(), vec![None, None]);

        // 4. 等長同字替換成符號時不可沿用「括號」舊音節或舊詞段
        svc.clear_all_pending();
        svc.insert_pending_char('括', Some("ㄎㄨㄛˋ".to_string()));
        svc.insert_pending_char('號', Some("ㄏㄠˋ".to_string()));
        svc.replace_pending_range(0, 2, &['【', '】'], None);
        assert_eq!(*svc.pending_compose.lock().unwrap(), "【】");
        assert_eq!(*svc.pending_syllables.lock().unwrap(), vec![None, None]);
        assert!(svc.pending_segments.lock().unwrap().is_empty());

        // 5. Shift+標記 -> Tab 語意查表
        let tab_target = { svc.engine.lock().unwrap().lookup_symbol_tab_target("方框") };
        match tab_target {
            Some(ruhi::symbol_tree::SymbolTabTarget::Candidates(cands)) => {
                assert_eq!(cands.first().map(String::as_str), Some("□"));
            }
            other => panic!("預期「方框」回傳 Candidates，實際 {:?}", other),
        }
    }

    #[test]
    fn test_symbol_tab_single_char_full_range_replace_and_period_ellipsis_cands_261118() {
        let svc = make_p1_t4_test_svc("hsu");

        // 1. 標記 -> Tab -> 選取 1 字元符號時，必須完整覆蓋整個標記詞區間，不可殘留尾字
        let sym_cases = [
            ("打叉", "❌"),
            ("圓形", "○"),
            ("三角形", "▲"),
            ("方框", "⬜"),
            ("上箭頭", "↑"),
        ];
        for (keyword, pick) in sym_cases {
            svc.clear_all_pending();
            for ch in keyword.chars() {
                svc.insert_pending_char(ch, Some("ㄗ".to_string()));
            }
            let kw_len = keyword.chars().count();
            let cands = match svc.engine.lock().unwrap().lookup_symbol_tab_target(keyword) {
                Some(ruhi::symbol_tree::SymbolTabTarget::Candidates(c)) => c,
                other => panic!("預期 `{keyword}` 回傳 Candidates，實際 {other:?}"),
            };
            assert!(
                cands.iter().any(|c| c == pick),
                "`{keyword}` 候選清單應包含 `{pick}`: {cands:?}"
            );
            set_tab_range_popup_active(svc.western_instance_key(), true);
            *svc.target_char_idx.lock().unwrap() = 0;
            *svc.target_range.lock().unwrap() = (0, kw_len);
            *svc.sel_anchor.lock().unwrap() = None;
            *svc.sentence_cursor.lock().unwrap() = 0;
            *svc.active_candidates.lock().unwrap() = cands.clone();
            *svc.candidate_selecting.lock().unwrap() = true;

            svc.apply_candidate_selection_in_desk(pick, &cands);
            assert_eq!(
                *svc.pending_compose.lock().unwrap(),
                pick,
                "`{keyword}` -> Tab -> `{pick}` 不得殘留尾字"
            );
            assert_eq!(*svc.sentence_cursor.lock().unwrap(), pick.chars().count());
            assert_eq!(*svc.pending_syllables.lock().unwrap(), vec![None]);
        }

        // 1b. 句中標記區間置換：今日事項：打叉完成 -> 今日事項：❌完成
        svc.clear_all_pending();
        for ch in "今日事項：打叉完成".chars() {
            svc.insert_pending_char(ch, Some("ㄗ".to_string()));
        }
        let dacha_cands = match svc.engine.lock().unwrap().lookup_symbol_tab_target("打叉") {
            Some(ruhi::symbol_tree::SymbolTabTarget::Candidates(c)) => c,
            other => panic!("unexpected 打叉 lookup: {other:?}"),
        };
        set_tab_range_popup_active(svc.western_instance_key(), true);
        *svc.target_char_idx.lock().unwrap() = 5;
        *svc.target_range.lock().unwrap() = (5, 7);
        *svc.sel_anchor.lock().unwrap() = None;
        *svc.sentence_cursor.lock().unwrap() = 5;
        *svc.active_candidates.lock().unwrap() = dacha_cands.clone();
        *svc.candidate_selecting.lock().unwrap() = true;
        svc.apply_candidate_selection_in_desk("❌", &dacha_cands);
        assert_eq!(*svc.pending_compose.lock().unwrap(), "今日事項：❌完成");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 6);

        // 2. 未標記同音詞首選單字不可誤刪後續字：「假設」選「甲」->「甲設」
        svc.clear_all_pending();
        svc.insert_pending_char('假', Some("ㄐㄧㄚˇ".to_string()));
        svc.insert_pending_char('設', Some("ㄕㄜˋ".to_string()));
        *svc.pending_segments.lock().unwrap() = vec![(0, 2, "假設".to_string(), "ㄐㄧㄚˇ ㄕㄜˋ".to_string())];
        set_tab_range_popup_active(svc.western_instance_key(), false);
        *svc.target_char_idx.lock().unwrap() = 0;
        *svc.target_range.lock().unwrap() = (0, 2);
        *svc.sel_anchor.lock().unwrap() = None;
        *svc.sentence_cursor.lock().unwrap() = 0;
        let homo_cands = vec!["假設".to_string(), "假".to_string(), "甲".to_string()];
        *svc.active_candidates.lock().unwrap() = homo_cands.clone();
        *svc.candidate_selecting.lock().unwrap() = true;
        svc.apply_candidate_selection_in_desk("甲", &homo_cands);
        assert_eq!(*svc.pending_compose.lock().unwrap(), "甲設");
        assert_eq!(
            *svc.pending_syllables.lock().unwrap(),
            vec![Some("ㄐㄧㄚˇ".to_string()), Some("ㄕㄜˋ".to_string())]
        );

        // 3. 按「.」得「。」再按「↓」應包含「……」，選後游標停於刪節號後並完整替換
        svc.clear_all_pending();
        svc.insert_pending_char('測', Some("ㄘㄜˋ".to_string()));
        svc.insert_pending_char('試', Some("ㄕˋ".to_string()));
        svc.insert_pending_char('。', None);
        *svc.sentence_cursor.lock().unwrap() = 3;
        let (ti, tr, period_cands) = svc.detect_target_word_and_cands("測試。", 3);
        assert_eq!(ti, 2);
        assert_eq!(tr, (2, 3));
        assert_eq!(period_cands.get(0).map(String::as_str), Some("。"));
        assert_eq!(period_cands.get(1).map(String::as_str), Some("……"));

        *svc.target_char_idx.lock().unwrap() = ti;
        *svc.target_range.lock().unwrap() = tr;
        *svc.sentence_cursor.lock().unwrap() = ti;
        *svc.active_candidates.lock().unwrap() = period_cands.clone();
        *svc.candidate_selecting.lock().unwrap() = true;
        svc.apply_candidate_selection_in_desk("……", &period_cands);
        assert_eq!(*svc.pending_compose.lock().unwrap(), "測試……");
        assert_eq!(*svc.sentence_cursor.lock().unwrap(), 4);
    }

    #[test]
    fn test_shift_enter_pending_roster_joint_reeval_and_abi_hash_261118() {
        let h1 = compute_layer_a_abi_hash();
        assert_eq!(h1, crate::ruhi_tsf_abi_layout_hash());

        let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("data");
        if !data_dir.join("lexicon_core.tsv").exists() {
            return;
        }

        let svc = make_p1_t4_test_svc("hsu");
        {
            let mut eng = svc.engine.lock().unwrap();
            *eng = RuhiEngine::new_with_data_dir(data_dir.to_str().unwrap());
            eng.set_keyboard_mode("hsu");

            let prefix = eng.compute_joint_reeval_prefix("我們這一隊的隊員有\n曹苡詩\n");
            assert_eq!(prefix.as_deref(), Some("__ROSTER__\t\n"));
        }

        svc.clear_all_pending();
        let line1: &[(&str, &str)] = &[
            ("我", "ㄨㄛˇ"), ("們", "ㄇㄣˊ"), ("這", "ㄓㄜˋ"), ("一", "ㄧˊ"),
            ("隊", "ㄉㄨㄟˋ"), ("的", "˙ㄉㄜ"), ("隊", "ㄉㄨㄟˋ"), ("員", "ㄩㄢˊ"), ("有", "ㄧㄡˇ"),
        ];
        for (ch_s, bpm) in line1 {
            svc.insert_pending_char(ch_s.chars().next().unwrap(), Some((*bpm).to_string()));
        }
        svc.insert_pending_char('\n', None);
        for (ch_s, bpm) in [("曹", "ㄘㄠˊ"), ("苡", "ㄧˇ"), ("詩", "ㄕ")] {
            svc.insert_pending_char(ch_s.chars().next().unwrap(), Some((*bpm).to_string()));
        }
        svc.insert_pending_char('\n', None);
        for (ch_s, bpm) in [("潘", "ㄆㄢ"), ("江", "ㄐㄧㄤ"), ("衛", "ㄨㄟˋ")] {
            svc.insert_pending_char(ch_s.chars().next().unwrap(), Some((*bpm).to_string()));
        }
        let cur = svc.pending_compose.lock().unwrap().chars().count();
        *svc.sentence_cursor.lock().unwrap() = cur;

        svc.re_evaluate_joint_pending_and_engine();
        assert_eq!(
            *svc.pending_compose.lock().unwrap(),
            "我們這一隊的隊員有\n曹苡詩\n潘江衛"
        );
    }

    #[test]
    fn test_classical_mode_config_sync_and_default_261120() {
        let mut cfg = RuhiConfig::default();
        assert_eq!(cfg.classical_mode, "auto");

        // 舊版設定檔缺少 classical_mode 欄位時必須安全反序列化為 "auto"
        let legacy: RuhiConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.classical_mode, "auto");

        let mut eng = RuhiEngine::new();
        apply_config_to_engine(&mut eng, &cfg);
        assert_eq!(
            eng.router.classical_mode,
            ruhi::router::ClassicalRegisterMode::Auto
        );

        cfg.classical_mode = "off".to_string();
        apply_config_to_engine(&mut eng, &cfg);
        assert_eq!(
            eng.router.classical_mode,
            ruhi::router::ClassicalRegisterMode::ModernOnly
        );

        cfg.classical_mode = "explicit".to_string();
        apply_config_to_engine(&mut eng, &cfg);
        assert_eq!(
            eng.router.classical_mode,
            ruhi::router::ClassicalRegisterMode::ClassicalExplicit
        );

        cfg.classical_mode = "invalid_value".to_string();
        apply_config_to_engine(&mut eng, &cfg);
        assert_eq!(
            eng.router.classical_mode,
            ruhi::router::ClassicalRegisterMode::Auto
        );
    }
}
