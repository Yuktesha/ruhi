/// RuhI (如喜) Windows 11 TSF Native Text Service Implementation
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
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * std::mem::size_of::<u16>();
    unsafe {
        let h_global = match GlobalAlloc(GMEM_MOVEABLE, byte_len) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let ptr = GlobalLock(h_global) as *mut u16;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(h_global);

        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let res = SetClipboardData(13, windows::Win32::Foundation::HANDLE(h_global.0));
        let _ = CloseClipboard();
        res.is_ok()
    }
}

use ruhi::engine::RuhiEngine;
use ruhi::lexicon::LexiconManager;
use crate::edit_session::{EditSessionAction, RuhiEditSession};
use crate::logger;

static LAST_CARET_POS: Mutex<(i32, i32)> = Mutex::new((0, 0));

pub fn set_last_caret_pos(x: i32, y: i32) {
    if let Ok(mut lock) = LAST_CARET_POS.lock() {
        *lock = (x, y);
    }
}

pub fn get_last_caret_pos() -> (i32, i32) {
    LAST_CARET_POS.lock().map(|p| *p).unwrap_or((0, 0))
}

static LAST_CONFIG_CHECK: Mutex<Option<(std::time::Instant, std::time::SystemTime)>> = Mutex::new(None);

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct RuhiConfig {
    #[serde(default = "default_keyboard_mode")]
    pub keyboard_mode: String,
    #[serde(default = "default_candidate_quick_key")]
    pub candidate_quick_key: String,
    #[serde(default = "default_punctuation_commit_mode")]
    pub punctuation_commit_mode: String,
    #[serde(default = "default_input_display_mode")]
    pub input_display_mode: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
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
    #[serde(default = "default_capslock_switch")]
    pub capslock_switch: String,
    #[serde(default = "default_capsule_mode")]
    pub capsule_mode: String,
    #[serde(default = "default_confirm_add_word")]
    pub confirm_add_word: bool,
    #[serde(default = "default_confirm_suppress_word")]
    pub confirm_suppress_word: bool,
}

impl Default for RuhiConfig {
    fn default() -> Self {
        Self {
            keyboard_mode: default_keyboard_mode(),
            candidate_quick_key: default_candidate_quick_key(),
            punctuation_commit_mode: default_punctuation_commit_mode(),
            input_display_mode: default_input_display_mode(),
            font_size: default_font_size(),
            theme: default_theme(),
            user_dict_path: default_user_dict_path(),
            desk_x: default_desk_x(),
            desk_y: default_desk_y(),
            max_composition_chars: default_max_composition_chars(),
            q_cycle_max: default_q_cycle_max(),
            enable_mixed_english: default_enable_mixed_english(),
            mixed_english_mode: default_mixed_english_mode(),
            mixed_english_min_len: default_mixed_english_min_len(),
            capslock_switch: default_capslock_switch(),
            capsule_mode: default_capsule_mode(),
            confirm_add_word: default_confirm_add_word(),
            confirm_suppress_word: default_confirm_suppress_word(),
        }
    }
}

fn default_keyboard_mode() -> String { "hsu".to_string() }
fn default_candidate_quick_key() -> String { "asdf".to_string() }
fn default_punctuation_commit_mode() -> String { "B".to_string() }
fn default_input_display_mode() -> String { "inline_underline".to_string() }
fn default_font_size() -> u32 { 32 }
fn default_theme() -> String { "system".to_string() }
fn default_user_dict_path() -> String { "user_dict.tsv".to_string() }
fn default_desk_x() -> i32 { 350 }
fn default_desk_y() -> i32 { 550 }
fn default_max_composition_chars() -> usize { 30 }
fn default_q_cycle_max() -> usize { 3 }
fn default_enable_mixed_english() -> bool { true }
fn default_mixed_english_mode() -> String { "contextual_stream".to_string() }
fn default_mixed_english_min_len() -> usize { 4 }
fn default_capslock_switch() -> String { "capital_lock".to_string() }
fn default_capsule_mode() -> String { "mini_capsule".to_string() }
fn default_confirm_add_word() -> bool { true }
fn default_confirm_suppress_word() -> bool { true }

#[derive(Debug, Clone)]
pub struct RuhiPaths {
    pub data_dir: std::path::PathBuf,
    pub sandbox_dir: std::path::PathBuf,
    pub mcbopomofo: std::path::PathBuf,
    pub moe: std::path::PathBuf,
    pub domain: std::path::PathBuf,
    pub user_dict: std::path::PathBuf,
    pub transitions: std::path::PathBuf,
    pub symbols: std::path::PathBuf,
    pub config: std::path::PathBuf,
    pub english: std::path::PathBuf,
    pub deleted_dict: std::path::PathBuf,
    pub rules_json: std::path::PathBuf,
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
            dir.join("lexicon_mcbopomofo.tsv").exists() || dir.join("lexicon_moe.tsv").exists()
        };

        let data_dir = if dll_dir.join("data").is_dir() && has_full_lexicon(&dll_dir.join("data")) {
            dll_dir.join("data")
        } else if dll_dir.join("ruhi_lexicon").join("data").is_dir() && has_full_lexicon(&dll_dir.join("ruhi_lexicon").join("data")) {
            dll_dir.join("ruhi_lexicon").join("data")
        } else if dll_dir.join("..").join("ruhi_lexicon").join("data").is_dir() && has_full_lexicon(&dll_dir.join("..").join("ruhi_lexicon").join("data")) {
            dll_dir.join("..").join("ruhi_lexicon").join("data")
        } else if dll_dir.join("..").join("..").join("ruhi_lexicon").join("data").is_dir() && has_full_lexicon(&dll_dir.join("..").join("..").join("ruhi_lexicon").join("data")) {
            dll_dir.join("..").join("..").join("ruhi_lexicon").join("data")
        } else if appdata_ruhi.join("data").is_dir() && has_full_lexicon(&appdata_ruhi.join("data")) {
            appdata_ruhi.join("data")
        } else if dll_dir.join("data").is_dir() {
            dll_dir.join("data")
        } else {
            dll_dir.clone()
        };

        // Resolve config/user directory (sandbox_dir for backwards compatibility with code)
        let sandbox_dir = if dll_dir.join("config.json").exists() {
            dll_dir.clone()
        } else if dll_dir.join("..").join("config.json").exists() {
            dll_dir.join("..")
        } else if dll_dir.join("..").join("..").join("config.json").exists() {
            dll_dir.join("..").join("..")
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
            } else if std::path::Path::new("C:\\ATGprjs\\ruhi\\ruhi_lexicon\\data").join(name).exists() {
                std::path::PathBuf::from("C:\\ATGprjs\\ruhi\\ruhi_lexicon\\data").join(name)
            } else if appdata_ruhi.join("data").join(name).exists() {
                appdata_ruhi.join("data").join(name)
            } else {
                data_dir.join(name)
            }
        };

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

        Self {
            data_dir,
            sandbox_dir,
            mcbopomofo,
            moe,
            domain,
            user_dict,
            transitions,
            symbols,
            config,
            english,
            deleted_dict,
            rules_json,
        }
    }
}

pub fn load_ruhi_config(paths: &RuhiPaths) -> RuhiConfig {
    if paths.config.exists() {
        if let Ok(content) = std::fs::read_to_string(&paths.config) {
            if let Ok(cfg) = serde_json::from_str::<RuhiConfig>(&content) {
                logger::log(&format!("Loaded config from {:?}: punct_mode={}, quick_key={}", paths.config, cfg.punctuation_commit_mode, cfg.candidate_quick_key));
                return cfg;
            }
        }
    }
    logger::log("Using default RuhiConfig (mode B, asdf)");
    RuhiConfig::default()
}

pub fn save_ruhi_config(paths: &RuhiPaths, config: &RuhiConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(&paths.config, json);
        logger::log(&format!("Saved updated RuhiConfig to {:?}", paths.config));
    }
}

fn get_lexicon_mtime(paths: &RuhiPaths) -> std::time::SystemTime {
    let files = [&paths.mcbopomofo, &paths.moe, &paths.domain, &paths.user_dict];
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

fn load_all_lexicons(paths: &RuhiPaths) -> Arc<LexiconManager> {
    let mut mgr = LexiconManager::new();
    let mut loaded_count = 0;

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
    logger::log(&format!("Total loaded lexicon entries: {}", loaded_count));
    Arc::new(mgr)
}

static SHARED_LEXICON: OnceLock<std::sync::RwLock<(std::time::SystemTime, Arc<LexiconManager>)>> = OnceLock::new();

fn get_shared_lexicon(paths: &RuhiPaths) -> Arc<LexiconManager> {
    let holder = SHARED_LEXICON.get_or_init(|| {
        let mtime = get_lexicon_mtime(paths);
        let mgr = load_all_lexicons(paths);
        std::sync::RwLock::new((mtime, mgr))
    });

    let current_mtime = get_lexicon_mtime(paths);
    if let Ok(read_guard) = holder.read() {
        if current_mtime > read_guard.0 {
            drop(read_guard);
            if let Ok(mut write_guard) = holder.write() {
                if current_mtime > write_guard.0 {
                    logger::log(&format!("Detected modified lexicon file(s) (mtime={:?}), reloading...", current_mtime));
                    let mgr = load_all_lexicons(paths);
                    *write_guard = (current_mtime, mgr);
                }
            }
        }
    }

    holder.read().map(|g| g.1.clone()).unwrap_or_else(|_| Arc::new(LexiconManager::new()))
}

pub fn force_reload_lexicon(paths: &RuhiPaths) -> Arc<LexiconManager> {
    let holder = SHARED_LEXICON.get_or_init(|| {
        let mtime = get_lexicon_mtime(paths);
        let mgr = load_all_lexicons(paths);
        std::sync::RwLock::new((mtime, mgr))
    });
    if let Ok(mut write_guard) = holder.write() {
        let mgr = load_all_lexicons(paths);
        let mtime = get_lexicon_mtime(paths);
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

#[implement(
    ITfTextInputProcessor,
    ITfTextInputProcessorEx,
    ITfKeyEventSink,
    ITfThreadMgrEventSink,
    ITfCompositionSink,
    ITfDisplayAttributeProvider
)]
pub struct RuhiTextService {
    thread_mgr: Arc<Mutex<Option<ITfThreadMgr>>>,
    client_id: Arc<Mutex<u32>>,
    engine: Arc<Mutex<RuhiEngine>>,
    composition: Arc<Mutex<Option<ITfComposition>>>,
    thread_mgr_sink_cookie: Arc<Mutex<u32>>,
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
    // ── 抑制厭惡詞即時過濾集合 ──
    deleted_words: Arc<Mutex<std::collections::HashSet<String>>>,
    // ── 編輯器復原/重做歷史快照堆疊 (Undo / Redo Stacks) ──
    undo_stack: Arc<Mutex<Vec<EditorSnapshot>>>,
    redo_stack: Arc<Mutex<Vec<EditorSnapshot>>>,
}

impl RuhiTextService {
    pub fn new() -> Self {
        logger::log("RuhiTextService::new() starting...");

        let paths = RuhiPaths::resolve();
        let config = load_ruhi_config(&paths);
        let del_words = crate::user_word_dialog::load_deleted_words(&paths);
        let mut engine = RuhiEngine::new_with_data_dir(&paths.data_dir.to_string_lossy());
        engine.keyboard_mode = config.keyboard_mode.clone();
        engine.enable_mixed_english = config.enable_mixed_english;
        engine.mixed_english_mode = config.mixed_english_mode.clone();
        engine.mixed_english_min_len = config.mixed_english_min_len;
        let shared_lex = get_shared_lexicon(&paths);
        engine.lexicon = (*shared_lex).clone();

        if paths.transitions.exists() {
            if let Some(p_str) = paths.transitions.to_str() {
                let _ = engine.transitions.load_tsv(p_str);
            }
        }
        if paths.symbols.exists() {
            if let Some(p_str) = paths.symbols.to_str() {
                let _ = engine.symbols.load_tsv(p_str);
            }
        }
        if paths.english.exists() {
            if let Some(p_str) = paths.english.to_str() {
                let _ = engine.western.load_tsv(p_str);
            }
        }

        logger::log(&format!("RuhiTextService::new() initialized with data_dir={:?}, punct_mode={}, quick_key={}",
            paths.data_dir, config.punctuation_commit_mode, config.candidate_quick_key));

        let cw = match crate::candidate_window::CandidateWindow::new() {
            Ok(mut w) => {
                w.set_quick_key_mode(&config.candidate_quick_key);
                w.set_keyboard_mode(&config.keyboard_mode);
                logger::log("CandidateWindow created successfully.");
                Some(w)
            }
            Err(e) => {
                logger::log(&format!("CandidateWindow creation failed: {:?}", e));
                None
            }
        };

        Self {
            thread_mgr: Arc::new(Mutex::new(None)),
            client_id: Arc::new(Mutex::new(0)),
            engine: Arc::new(Mutex::new(engine)),
            composition: Arc::new(Mutex::new(None)),
            thread_mgr_sink_cookie: Arc::new(Mutex::new(0)),
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
            deleted_words: Arc::new(Mutex::new(del_words)),
            undo_stack: Arc::new(Mutex::new(Vec::new())),
            redo_stack: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ITfTextInputProcessor_Impl for RuhiTextService_Impl {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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

            logger::log("Activate finished successfully (S_OK)!");
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn Deactivate(&self) -> Result<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            logger::log("Deactivate called.");
            self.hide_candidate_window();
            set_last_caret_pos(0, 0);
            let tm_lock = self.thread_mgr.lock().unwrap();
            if let Some(tm) = tm_lock.as_ref() {
                let tid = *self.client_id.lock().unwrap();
                if let Ok(keystroke_mgr) = tm.cast::<ITfKeystrokeMgr>() {
                    unsafe { let _ = keystroke_mgr.UnadviseKeyEventSink(tid); }
                }
                if let Ok(source) = tm.cast::<ITfSource>() {
                    let tm_cookie = *self.thread_mgr_sink_cookie.lock().unwrap();
                    if tm_cookie != 0 {
                        unsafe { let _ = source.UnadviseSink(tm_cookie); }
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
            if fforeground.as_bool() {
                let paths = RuhiPaths::resolve();
                let shared_lex = get_shared_lexicon(&paths);
                if let Ok(mut eng) = self.engine.lock() {
                    eng.lexicon = (*shared_lex).clone();
                    if paths.transitions.exists() {
                        if let Some(p_str) = paths.transitions.to_str() {
                            let _ = eng.transitions.load_tsv(p_str);
                        }
                    }
                    if paths.symbols.exists() {
                        if let Some(p_str) = paths.symbols.to_str() {
                            let _ = eng.symbols.load_tsv(p_str);
                        }
                    }
                    if paths.english.exists() {
                        if let Some(p_str) = paths.english.to_str() {
                            let _ = eng.western.load_tsv(p_str);
                        }
                    }
                }
                let new_cfg = load_ruhi_config(&paths);
                if let Ok(mut eng) = self.engine.lock() {
                    eng.keyboard_mode = new_cfg.keyboard_mode.clone();
                }
                if let Ok(mut cw_guard) = self.candidate_window.lock() {
                    if let Some(cw) = cw_guard.as_mut() {
                        cw.set_quick_key_mode(&new_cfg.candidate_quick_key);
                        cw.set_keyboard_mode(&new_cfg.keyboard_mode);
                    }
                }
                *self.config.lock().unwrap() = new_cfg;
            } else {
                // 當前視窗失去焦點時，立即隱藏輸入座，絕不在背景凍結殘留！
                self.hide_candidate_window();
                set_last_caret_pos(0, 0);
            }
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }

    fn OnTestKeyDown(&self, _pic: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.check_reload_config();
            let vk = wparam.0 as u32;

            let is_ctrl = unsafe { ((GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) };
            let is_alt = unsafe { ((GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) };
            let is_shift = unsafe { ((GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) };
            let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };

            // 全域快捷鍵：Ctrl+Alt+R 喚醒語意除錯調教工具
            if is_ctrl && is_alt && (vk == 0x52 || vk == 0x54) {
                return Ok(BOOL::from(true));
            }

            if is_alt {
                return Ok(BOOL::from(false));
            }

            let has_buffer = {
                let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
                let engine = self.engine.lock().unwrap();
                pending_len > 0 || engine.fsm.has_pending() || !engine.fsm.syllables.is_empty() || !engine.composed_sentence.is_empty() || !engine.raw_ascii_buffer.is_empty() || engine.cangjie.has_buffer()
            };

            if is_ctrl {
                if has_buffer {
                    // 組字行有文字時，全部攔截！防禦外部快捷鍵外洩破壞組字現場
                    return Ok(BOOL::from(true));
                }
                // 組字行無文字時，全部放行給宿主應用程式
                return Ok(BOOL::from(false));
            }

            // Caps Lock 鍵本身：放行給系統切換 LED 狀態，組字串絕不中斷也不送出！
            if vk == VK_CAPITAL.0 as u32 {
                return Ok(BOOL::from(false));
            }

            let is_cand_sel = *self.candidate_selecting.lock().unwrap();
            let quick_mode = self.config.lock().map(|c| c.candidate_quick_key.clone()).unwrap_or_else(|_| "asdf".to_string());

            if is_cand_sel {
                if vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_NEXT.0 as u32 || vk == VK_PRIOR.0 as u32 {
                    return Ok(BOOL::from(true));
                }
                if quick_mode == "asdf" && map_asdf_to_idx(vk).is_some() && !is_shift {
                    return Ok(BOOL::from(true));
                }
                if quick_mode == "1234" && (0x31..=0x39).contains(&vk) && !is_shift {
                    return Ok(BOOL::from(true));
                }
                if vk == VK_SPACE.0 as u32 || vk == VK_RETURN.0 as u32 || vk == VK_ESCAPE.0 as u32 {
                    return Ok(BOOL::from(true));
                }
            }

            // 當完全沒有組字緩衝區時，Backspace、Delete、Enter、Space、Tab、方向鍵等一律直接放行給目標視窗！
            if !has_buffer {
                if vk == VK_RETURN.0 as u32 || vk == VK_BACK.0 as u32 || vk == VK_DELETE.0 as u32 || vk == VK_SPACE.0 as u32 || vk == VK_TAB.0 as u32 || vk == VK_ESCAPE.0 as u32 {
                    return Ok(BOOL::from(false));
                }
                if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_NEXT.0 as u32 {
                    return Ok(BOOL::from(false));
                }
            }

            // 【自然輸入法 UI/UX 精神】：Caps Lock 開啟時為大寫英數模式，直接納入組字流中，絕不提前送出！
            if is_caps_on {
                if (0x41..=0x5A).contains(&vk) {
                    return Ok(BOOL::from(true));
                }
                if has_buffer {
                    if vk == VK_RETURN.0 as u32 || vk == VK_SPACE.0 as u32 || vk == VK_BACK.0 as u32 || vk == VK_ESCAPE.0 as u32 || vk == VK_DELETE.0 as u32 {
                        return Ok(BOOL::from(true));
                    }
                    if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 {
                        return Ok(BOOL::from(true));
                    }
                    if (0x30..=0x39).contains(&vk) || is_punctuation_vk(vk, is_shift) {
                        return Ok(BOOL::from(true));
                    }
                }
                return Ok(BOOL::from(false));
            }

            // 【自然輸入法關鍵修正】：Shift+字母直接納入組字流，絕不洩漏給外部應用程式
            if is_shift && (0x41..=0x5A).contains(&vk) {
                return Ok(BOOL::from(true));
            }

            let mut eaten = false;

            if has_buffer {
                if vk == VK_RETURN.0 as u32 || vk == VK_SPACE.0 as u32 || vk == VK_BACK.0 as u32 || vk == VK_ESCAPE.0 as u32 || vk == VK_DELETE.0 as u32 {
                    eaten = true;
                } else if vk == VK_INSERT.0 as u32 {
                    eaten = true;  // Insert 切換 INS/OVR
                } else if (0x41..=0x5A).contains(&vk) {
                    eaten = true;
                } else if ((0x30..=0x39).contains(&vk) || (0x60..=0x6F).contains(&vk)) && !is_shift {
                    eaten = true;
                } else if is_punctuation_vk(vk, is_shift) {
                    eaten = true;
                } else if vk == VK_OEM_3.0 as u32 {
                    eaten = true;
                } else if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_NEXT.0 as u32 {
                    eaten = true;  // 含 Shift 組合、PageUp/Down
                }
            } else {
                if is_punctuation_vk(vk, is_shift) {
                    eaten = true;
                } else if (0x41..=0x5A).contains(&vk) {
                    eaten = true;
                } else if vk == VK_OEM_3.0 as u32 {
                    eaten = true;
                } else {
                    eaten = false;
                }
            }

            Ok(BOOL::from(eaten))
        })).unwrap_or_else(|_| Ok(BOOL::from(false)))
    }

    fn OnKeyDown(&self, pic: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.check_reload_config();
            let ctx = match pic {
                Some(c) => c,
                None => {
                    return Ok(BOOL::from(false));
                }
            };

            let vk = wparam.0 as u32;

            let is_ctrl = unsafe { ((GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) };
            let is_alt = unsafe { ((GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) };
            let is_shift = unsafe { ((GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) };
            let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };

            // 全域快捷鍵：Ctrl+Alt+R 或 Ctrl+Alt+T 喚起「語意除錯調教工具」
            if is_ctrl && is_alt && (vk == 0x52 || vk == 0x54) {
                let paths = RuhiPaths::resolve();
                let script_path = if paths.sandbox_dir.join("scripts").join("語意除錯調教.ps1").exists() {
                    paths.sandbox_dir.join("scripts").join("語意除錯調教.ps1")
                } else if std::path::Path::new("C:\\ATGprjs\\ruhi\\scripts\\語意除錯調教.ps1").exists() {
                    std::path::PathBuf::from("C:\\ATGprjs\\ruhi\\scripts\\語意除錯調教.ps1")
                } else {
                    paths.sandbox_dir.join("語意除錯調教.ps1")
                };
                let _ = std::process::Command::new("powershell.exe")
                    .args(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &script_path.to_string_lossy()])
                    .spawn();
                return Ok(BOOL::from(true));
            }

            if is_alt {
                return Ok(BOOL::from(false));
            }

            // Caps Lock 鍵本身：放行給系統切換 LED 狀態，組字串絕不中斷也不送出，讓英文自由留在輸入列！
            if vk == VK_CAPITAL.0 as u32 {
                return Ok(BOOL::from(false));
            }

            let has_buffer = {
                let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
                let engine = self.engine.lock().unwrap();
                pending_len > 0 || engine.fsm.has_pending() || !engine.fsm.syllables.is_empty() || !engine.composed_sentence.is_empty() || !engine.raw_ascii_buffer.is_empty() || engine.cangjie.has_buffer()
            };

            // 當完全沒有組字緩衝區時，Backspace、Delete、Enter、Space、Tab、方向鍵等一律直接放行給目標視窗！
            if !has_buffer {
                if vk == VK_RETURN.0 as u32 || vk == VK_BACK.0 as u32 || vk == VK_DELETE.0 as u32 || vk == VK_SPACE.0 as u32 || vk == VK_TAB.0 as u32 || vk == VK_ESCAPE.0 as u32 || vk == 0x6E {
                    return Ok(BOOL::from(false));
                }
                if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_NEXT.0 as u32 {
                    return Ok(BOOL::from(false));
                }
            }

            // Caps Lock 開啟時：依照設定切換「大寫鎖定」或「英打模式」
            if is_caps_on {
                if (0x41..=0x5A).contains(&vk) {
                    let base = vk as u8 as char;
                    let capslock_mode = self.config.lock().map(|c| c.capslock_switch.clone()).unwrap_or_else(|_| "capital_lock".to_string());
                    let ch = if capslock_mode == "capital_lock" {
                        if is_shift { base.to_ascii_lowercase() } else { base.to_ascii_uppercase() }
                    } else {
                        if is_shift { base.to_ascii_uppercase() } else { base.to_ascii_lowercase() }
                    };

                    self.consolidate_engine_to_pending();
                    self.insert_pending_char(ch, None);
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                if vk == VK_SPACE.0 as u32 && has_buffer {
                    self.consolidate_engine_to_pending();
                    self.insert_pending_char(' ', None);
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                if ((0x30..=0x39).contains(&vk) || (0x60..=0x69).contains(&vk) || vk == 0x6E || is_punctuation_vk(vk, is_shift)) && has_buffer {
                    let ch = if vk == 0x6E {
                        '.'
                    } else {
                        map_vk_to_char(vk, is_shift).unwrap_or(' ')
                    };
                    self.consolidate_engine_to_pending();
                    self.insert_pending_char(ch, None);
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            let punct_mode = self.config.lock().map(|c| c.punctuation_commit_mode.clone()).unwrap_or_else(|_| "B".to_string());
            let quick_mode = self.config.lock().map(|c| c.candidate_quick_key.clone()).unwrap_or_else(|_| "asdf".to_string());

            // 1. 處理 Ctrl 組合鍵與編輯器操作 (Ctrl+A, Ctrl+C, Ctrl+V, Ctrl+X, Ctrl+Z, Ctrl+Y/R 等)
            if is_ctrl {
                if !has_buffer {
                    return Ok(BOOL::from(false));
                }

                // 1.1 方向鍵與退格鍵放行至後方常式處理 (詞間快進/快退/刪詞)
                if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_BACK.0 as u32 {
                    // 放行至方向鍵或退格鍵處理常式，執行詞間快進／快退或字詞刪除！
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
                } else if vk == 0x58 {
                    // Ctrl+X：剪下
                    if let Some((start, end)) = self.get_manual_selection_range() {
                        let (full, _) = self.get_full_display_and_cursor();
                        let chars: Vec<char> = full.chars().collect();
                        if start < end && end <= chars.len() {
                            let text: String = chars[start..end].iter().collect();
                            set_clipboard_text(&text);
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
                    // Ctrl+V：貼上剪貼簿文字
                    if let Some(text) = get_clipboard_text() {
                        if !text.is_empty() {
                            self.delete_selection_if_any();
                            self.consolidate_engine_to_pending();
                            let p_chars: Vec<char> = text.chars().collect();
                            let p_syls: Vec<Option<String>> = vec![None; p_chars.len()];
                            self.splice_pending_chunk(&p_chars, &p_syls, &[]);
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
                } else if let Some(combo) = map_vk_to_ctrl_combo(vk, is_shift) {
                    let mut engine = self.engine.lock().unwrap();
                    if engine.symbols.contains_key(&combo) {
                        let resp = engine.process_key_str(&combo);
                        let sym = resp.committed_chunk;
                        drop(engine);

                        self.push_undo();
                        let mut pending = self.pending_compose.lock().unwrap();
                        pending.push_str(&sym);
                        let full_text = pending.clone();
                        let cur_pos = full_text.chars().count();
                        *self.sentence_cursor.lock().unwrap() = cur_pos;
                        drop(pending);

                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }
                    // 其他無映射之符號快捷鍵，在組字中有文字時全部攔截！
                    return Ok(BOOL::from(true));
                } else {
                    // 【全域防禦】：其餘任何 Ctrl 組合鍵（如 Ctrl+S, Ctrl+W, Ctrl+Q 等），一律攔截吃掉！
                    return Ok(BOOL::from(true));
                }
            }

            // 2. 候選字選取模式 (Candidate Selecting Mode)
            let is_cand_sel = *self.candidate_selecting.lock().unwrap();
            if is_cand_sel {
                let cands = self.active_candidates.lock().unwrap().clone();
                let cands_len = cands.len();

                // ↓ 下方向鍵：下一個候選
                if vk == VK_DOWN.0 as u32 {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        *sel = (*sel + 1) % cands_len;
                        let new_idx = *sel;
                        drop(sel);
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.set_selected(new_idx);
                            }
                        }
                    }
                    return Ok(BOOL::from(true));
                }

                // ↑ 上方向鍵：上一個候選
                if vk == VK_UP.0 as u32 {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        *sel = if *sel == 0 { cands_len - 1 } else { *sel - 1 };
                        let new_idx = *sel;
                        drop(sel);
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.set_selected(new_idx);
                            }
                        }
                    }
                    return Ok(BOOL::from(true));
                }

                // Page Down 鍵 (VK_NEXT)：向下翻一頁 (9 個候選)
                if vk == VK_NEXT.0 as u32 {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        let cur_page = *sel / 9;
                        let total_pages = (cands_len + 8) / 9;
                        let next_page = (cur_page + 1) % total_pages;
                        *sel = (next_page * 9).min(cands_len - 1);
                        let new_idx = *sel;
                        drop(sel);
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.set_selected(new_idx);
                            }
                        }
                    }
                    return Ok(BOOL::from(true));
                }

                // Page Up 鍵 (VK_PRIOR)：向上翻一頁 (9 個候選)
                if vk == VK_PRIOR.0 as u32 {
                    if cands_len > 0 {
                        let mut sel = self.selected_candidate_idx.lock().unwrap();
                        let cur_page = *sel / 9;
                        let total_pages = (cands_len + 8) / 9;
                        let prev_page = if cur_page > 0 { cur_page - 1 } else { total_pages - 1 };
                        *sel = (prev_page * 9).min(cands_len - 1);
                        let new_idx = *sel;
                        drop(sel);
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.set_selected(new_idx);
                            }
                        }
                    }
                    return Ok(BOOL::from(true));
                }

                let quick_idx = if quick_mode == "asdf" {
                    if !is_shift { map_asdf_to_idx(vk) } else { None }
                } else if !is_shift && (0x31..=0x39).contains(&vk) {
                    Some((vk - 0x31) as usize)
                } else if !is_shift && (0x61..=0x69).contains(&vk) {
                    Some((vk - 0x61) as usize)
                } else {
                    None
                };

                let chosen_cand_idx = if let Some(q_idx) = quick_idx {
                    let cur_page = *self.selected_candidate_idx.lock().unwrap() / 9;
                    let target_cand = cur_page * 9 + q_idx;
                    if target_cand < cands_len { Some(target_cand) } else { None }
                } else if vk == VK_SPACE.0 as u32 || vk == VK_RETURN.0 as u32 {
                    let sel = *self.selected_candidate_idx.lock().unwrap();
                    if sel < cands_len { Some(sel) } else { None }
                } else {
                    None
                };

                if let Some(cand_i) = chosen_cand_idx {
                    let chosen = &cands[cand_i];
                    let target_range = *self.target_range.lock().unwrap();
                    let target_idx = *self.target_char_idx.lock().unwrap();
                    let (full_text, _) = self.get_full_display_and_cursor();
                    let chars: Vec<char> = full_text.chars().collect();

                    let target_len = target_range.1.saturating_sub(target_range.0);
                    let chosen_len = chosen.chars().count();

                    let (replace_start, replace_end) = if target_len > 1 && chosen_len == target_len && target_range.1 <= chars.len() {
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
                    let char_syl = self.pending_syllables.lock().unwrap().get(target_idx).cloned().flatten();
                    let bpm_str = char_syl.clone().unwrap_or_default();

                    if !rejected.is_empty() && rejected != *chosen {
                        let mut eng = self.engine.lock().unwrap();
                        eng.record_revision(&bpm_str, &rejected, chosen, &prev_context, &next_context);
                    }

                    let replacement: Vec<char> = chosen.chars().collect();
                    self.replace_pending_range(replace_start, replace_end, &replacement, char_syl);
                    self.engine.lock().unwrap().reset_buffer();
                    *self.sentence_cursor.lock().unwrap() = replace_start + chosen_len;

                    *self.sel_anchor.lock().unwrap() = None;
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                if vk == VK_ESCAPE.0 as u32 {
                    *self.sel_anchor.lock().unwrap() = None;
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }

                // 若在候選窗開啟狀態下按下左/右/Home/End 方向鍵，立即關閉候選座並允許游標繼續移動！
                if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 {
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                    // 不直接 return，放行給下方第 4 節處理游標移動
                }
            }

            // 2.5 空白鍵在無未決注音聲調時的處理 (例如先前輸入了「1.」或已確認詞句)
            // 絕不可將未處理的空白鍵外洩至目標視窗，必須納入組字緩衝區！
            // 但若引擎處於歐文模式，放行交由 engine 結算歐文單字！
            if vk == VK_SPACE.0 as u32 && has_buffer {
                let (has_pending_fsm, is_latin) = {
                    let engine = self.engine.lock().unwrap();
                    (engine.fsm.has_pending(), engine.is_latin_mode)
                };
                if !has_pending_fsm && !is_latin {
                    self.consolidate_engine_to_pending();
                    self.insert_pending_char(' ', None);
                    self.sync_input_display(ctx)?;
                    return Ok(BOOL::from(true));
                }
            }

            // 3. 【自然輸入法 Shift+大寫字母流】：混打模式放行進入引擎智慧判斷，關閉混打時則直接納入組字流
            let is_mixed = self.config.lock().map(|c| c.enable_mixed_english).unwrap_or(true);
            if !is_mixed && is_shift && (0x41..=0x5A).contains(&vk) {
                let upper_char = (vk as u8 as char).to_ascii_uppercase();
                self.consolidate_engine_to_pending();
                self.insert_pending_char(upper_char, None);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.5 Insert 鍵切換插入/覆蓋模式 (移植自沙盒)
            if vk == VK_INSERT.0 as u32 && has_buffer {
                let mut mode = self.insert_mode.lock().unwrap();
                *mode = !*mode;
                logger::log(&format!("Insert toggle: mode={}", if *mode { "INS" } else { "OVR" }));
                drop(mode);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.6 九宮格小數點 (0x6E / VK_DECIMAL) 永遠保持半形「.」
            if vk == 0x6E && has_buffer {
                self.consolidate_engine_to_pending();
                self.insert_pending_char('.', None);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.8 Q 鍵快速候選字輪換 (Q-Key Quick Candidate Cycle)
            // 游標所在處單字快速切換：若是排序1直接跳排序2，最多循環 q_cycle_max 次後自動彈出候選座
            let kb_mode = self.config.lock().map(|c| c.keyboard_mode.clone()).unwrap_or_else(|_| "hsu".to_string());
            if vk == 0x51 && !is_shift && !is_ctrl && !is_alt && has_buffer && kb_mode == "hsu" {
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

                    let target_char = chars[target_idx];
                    let char_syl = self.pending_syllables.lock().unwrap().get(target_idx).cloned().flatten();
                    let engine = self.engine.lock().unwrap();
                    let cands = engine.lexicon.lookup_char_candidates_with_bpm(target_char, char_syl.as_deref());
                    drop(engine);

                    if *q_count >= q_max {
                        // 超過上限次數：重設計數並自動開啟選字座 (等同按下 ↓)
                        *q_count = 0;
                        *last_idx = None;
                        drop(q_count);
                        drop(last_idx);

                        *self.target_char_idx.lock().unwrap() = target_idx;
                        *self.target_range.lock().unwrap() = (target_idx, (target_idx + 1).min(chars.len()));
                        *self.sentence_cursor.lock().unwrap() = target_idx;

                        if !cands.is_empty() {
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                        }
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

            // 4. 方向鍵與全句游標穿透（徹底消除「結界」，游標自由穿梭標點與前後文字）
            if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32 {
                self.consolidate_engine_to_pending();
                let (full_text, _) = self.get_full_display_and_cursor();
                let total_chars = full_text.chars().count();

                if total_chars > 0 {
                    // ── Shift+方向鍵選取 (移植自沙盒) ──
                    if is_shift && (vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32) {
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
                            let mut cur = self.sentence_cursor.lock().unwrap();
                            match vk {
                                v if v == VK_LEFT.0 as u32 => {
                                    if is_ctrl {
                                        let boundaries = self.get_word_boundaries(&full_text);
                                        *cur = Self::find_prev_word_boundary(&boundaries, *cur);
                                    } else {
                                        if *cur > 0 { *cur -= 1; }
                                    }
                                }
                                v if v == VK_RIGHT.0 as u32 => {
                                    if is_ctrl {
                                        let boundaries = self.get_word_boundaries(&full_text);
                                        *cur = Self::find_next_word_boundary(&boundaries, *cur, total_chars);
                                    } else {
                                        if *cur < total_chars { *cur += 1; }
                                    }
                                }
                                v if v == VK_HOME.0 as u32 => { *cur = 0; }
                                v if v == VK_END.0 as u32 => { *cur = total_chars; }
                                _ => {}
                            }
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 非 Shift 方向鍵：清除選取
                    if !is_shift {
                        *self.sel_anchor.lock().unwrap() = None;
                    }

                    // 4.1 向左方向鍵 ← (自由穿透標點，移動至前文，支援 Ctrl+Left 詞間快退)
                    if vk == VK_LEFT.0 as u32 {
                        let mut cur = self.sentence_cursor.lock().unwrap();
                        if is_ctrl {
                            let boundaries = self.get_word_boundaries(&full_text);
                            *cur = Self::find_prev_word_boundary(&boundaries, *cur);
                        } else {
                            let c = (*cur).min(total_chars);
                            if c > 0 {
                                *cur = c - 1;
                            } else {
                                *cur = 0;
                            }
                        }
                        drop(cur);
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.2 向右方向鍵 → (自由穿透標點，支援 Ctrl+Right 詞間快進)
                    if vk == VK_RIGHT.0 as u32 {
                        let mut cur = self.sentence_cursor.lock().unwrap();
                        if is_ctrl {
                            let boundaries = self.get_word_boundaries(&full_text);
                            *cur = Self::find_next_word_boundary(&boundaries, *cur, total_chars);
                        } else {
                            let c = (*cur).min(total_chars);
                            if c < total_chars {
                                *cur = c + 1;
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

                    // 4.5 向下方向鍵 ↓：精準開啟游標所在目標詞/字的候選座
                    if vk == VK_DOWN.0 as u32 {
                        let cur = *self.sentence_cursor.lock().unwrap();
                        let (full_text, _) = self.get_full_display_and_cursor();
                        let (target_idx, target_range, cands) = self.detect_target_word_and_cands(&full_text, cur);

                        *self.target_char_idx.lock().unwrap() = target_idx;
                        *self.target_range.lock().unwrap() = target_range;
                        *self.sentence_cursor.lock().unwrap() = target_idx;

                        if !cands.is_empty() {
                            *self.selected_candidate_idx.lock().unwrap() = 0;
                            *self.active_candidates.lock().unwrap() = cands;
                            *self.candidate_selecting.lock().unwrap() = true;
                        }
                        self.sync_input_display(ctx)?;
                        return Ok(BOOL::from(true));
                    }

                    // 4.6 向上方向鍵 ↑：非選字模式下維持原位（絕不擅自跳至句首，更絕不外洩穿透至目標視窗）
                    if vk == VK_UP.0 as u32 {
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
                self.commit_current_composition(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 6.1 Backspace 退格鍵 (全句自由回退刪除 + 選取範圍刪除)
            if vk == VK_BACK.0 as u32 {
                // 選取範圍刪除優先
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

                let eng_had_uncommitted = {
                    let mut engine = self.engine.lock().unwrap();
                    if engine.cangjie.has_buffer() {
                        if is_ctrl || is_shift {
                            let _ = engine.cangjie.process_key_str("Shift+BackSpace");
                        } else {
                            let _ = engine.cangjie.process_key('\x08');
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
                    let mut cur = self.sentence_cursor.lock().unwrap();
                    if *cur > 0 {
                        if is_ctrl {
                            let pending = self.pending_compose.lock().unwrap().clone();
                            let chars: Vec<char> = pending.chars().collect();
                            let mut delete_start = *cur;
                            let prev_idx = *cur - 1;
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
                            let count_to_delete = *cur - delete_start;
                            for _ in 0..count_to_delete.max(1) {
                                if *cur > 0 {
                                    self.remove_pending_char_at(*cur - 1);
                                    *cur -= 1;
                                }
                            }
                        } else {
                            self.remove_pending_char_at(*cur - 1);
                            *cur -= 1;
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

            // 6.2 Delete 刪除鍵 (全句自由向後刪除 + 選取範圍刪除)
            if vk == VK_DELETE.0 as u32 {
                // 選取範圍刪除優先
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

                self.consolidate_engine_to_pending();
                let cur = *self.sentence_cursor.lock().unwrap();
                let full_len = self.get_full_display_and_cursor().0.chars().count();
                if cur < full_len {
                    self.remove_pending_char_at(cur);
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

            // 7. Escape 鍵 (4 級階梯式撤銷 — 移植自沙盒)
            if vk == VK_ESCAPE.0 as u32 {
                let has_selection = self.sel_anchor.lock().unwrap().is_some();
                let is_cand_sel_now = *self.candidate_selecting.lock().unwrap();
                let has_engine_preedit = {
                    let engine = self.engine.lock().unwrap();
                    engine.fsm.has_pending() || !engine.fsm.syllables.is_empty()
                };
                let has_pending = !self.pending_compose.lock().unwrap().is_empty();

                if has_selection {
                    // Level 1：清除選取（保留文字）
                    *self.sel_anchor.lock().unwrap() = None;
                    self.sync_input_display(ctx)?;
                } else if is_cand_sel_now {
                    // Level 2：退出候選字選取模式
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                    self.sync_input_display(ctx)?;
                } else if has_engine_preedit {
                    // Level 3：清除引擎中的未決注音（保留已組的 pending 文字）
                    self.engine.lock().unwrap().reset_buffer();
                    if has_pending {
                        self.sync_input_display(ctx)?;
                    } else {
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

            // 8. 許氏注音與標點符號輸入處理
            let char_opt = map_vk_to_char(vk, is_shift);
            if let Some(ch) = char_opt {
                self.delete_selection_if_any();
                let is_punct = {
                    let mut engine = self.engine.lock().unwrap();
                    engine.symbols.get_char(ch).is_some()
                };
                if is_punct && punct_mode == "B" {
                    self.consolidate_engine_to_pending();
                }

                let mut engine = self.engine.lock().unwrap();
                let resp = engine.process_key(ch);
                let action = resp.action;
                let committed_chunk = resp.committed_chunk;
                let _eng_disp = engine.get_display_text();
                let _eng_cur = engine.cursor_pos;
                drop(engine);

                if !action.is_empty() && action != "none" {
                    if action == "commit_punctuation" {
                        if punct_mode == "B" {
                            // 自然模式 B：標點直接追加進句子暫存流，維持底線長句！
                            let p_chars: Vec<char> = committed_chunk.chars().collect();
                            let p_syls: Vec<Option<String>> = vec![None; p_chars.len()];
                            self.splice_pending_chunk(&p_chars, &p_syls, &[]);
                            self.sync_input_display(ctx)?;
                        } else {
                            // 模式 A：結算送出
                            let full = format!("{}{}", *self.pending_compose.lock().unwrap(), committed_chunk);
                            self.clear_all_pending();
                            *self.sentence_cursor.lock().unwrap() = 0;
                            let c = full.chars().count();
                            self.update_tsf_composition(ctx, &full, c, None, true)?;
                            self.hide_candidate_window();
                        }
                    } else if action.starts_with("commit") {
                        let c_chars: Vec<char> = committed_chunk.chars().collect();
                        let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                        self.splice_pending_chunk(&c_chars, &c_syls, &[]);
                        self.sync_input_display(ctx)?;
                    } else {
                        // 一般組字中：更新長句顯示，絕不主動彈出候選窗
                        // 若引擎回傳了先前的結算字串 (例如「1.」)，立即保全納入 pending_compose，絕不丟失！
                        if !committed_chunk.is_empty() {
                            let c_chars: Vec<char> = committed_chunk.chars().collect();
                            let c_syls: Vec<Option<String>> = vec![None; c_chars.len()];
                            self.splice_pending_chunk(&c_chars, &c_syls, &[]);
                        }

                        self.sync_input_display(ctx)?;
                    }
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

        // 2. 未切詞區域（英文單字、數字、標點、空白、獨立漢字）依字元屬性轉換判定邊界
        #[derive(PartialEq, Eq, Clone, Copy)]
        enum Kind {
            AsciiWord,
            Whitespace,
            Punct,
            CjkSingle,
            Other,
        }

        let classify = |ch: char| -> Kind {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                Kind::AsciiWord
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

        for i in 0..total {
            if !covered[i] {
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

    fn find_prev_word_boundary(boundaries: &[usize], cur: usize) -> usize {
        boundaries.iter().rev().copied().find(|&b| b < cur).unwrap_or(0)
    }

    fn find_next_word_boundary(boundaries: &[usize], cur: usize, total: usize) -> usize {
        boundaries.iter().copied().find(|&b| b > cur).unwrap_or(total)
    }

    fn take_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            pending: self.pending_compose.lock().unwrap().clone(),
            cursor: *self.sentence_cursor.lock().unwrap(),
            anchor: *self.sel_anchor.lock().unwrap(),
            syllables: self.pending_syllables.lock().unwrap().clone(),
            segments: self.pending_segments.lock().unwrap().clone(),
        }
    }

    fn push_undo(&self) {
        let snap = self.take_snapshot();
        let mut undo = self.undo_stack.lock().unwrap();
        undo.push(snap);
        if undo.len() > 100 {
            undo.remove(0);
        }
        self.redo_stack.lock().unwrap().clear();
    }

    fn undo(&self) -> bool {
        let snap_opt = self.undo_stack.lock().unwrap().pop();
        if let Some(snap) = snap_opt {
            let current = self.take_snapshot();
            self.redo_stack.lock().unwrap().push(current);

            *self.pending_compose.lock().unwrap() = snap.pending;
            *self.sentence_cursor.lock().unwrap() = snap.cursor;
            *self.sel_anchor.lock().unwrap() = snap.anchor;
            *self.pending_syllables.lock().unwrap() = snap.syllables;
            *self.pending_segments.lock().unwrap() = snap.segments;
            self.engine.lock().unwrap().reset_buffer();
            true
        } else {
            false
        }
    }

    fn redo(&self) -> bool {
        let snap_opt = self.redo_stack.lock().unwrap().pop();
        if let Some(snap) = snap_opt {
            let current = self.take_snapshot();
            self.undo_stack.lock().unwrap().push(current);

            *self.pending_compose.lock().unwrap() = snap.pending;
            *self.sentence_cursor.lock().unwrap() = snap.cursor;
            *self.sel_anchor.lock().unwrap() = snap.anchor;
            *self.pending_syllables.lock().unwrap() = snap.syllables;
            *self.pending_segments.lock().unwrap() = snap.segments;
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
        *pending = chars.into_iter().collect();

        let mut syls = self.pending_syllables.lock().unwrap();
        let s_pos = insert_pos.min(syls.len());
        syls.insert(s_pos, syl);

        let mut segs = self.pending_segments.lock().unwrap();
        for seg in segs.iter_mut() {
            if seg.0 >= insert_pos {
                seg.0 += 1;
                seg.1 += 1;
            } else if seg.1 > insert_pos {
                seg.1 += 1;
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
        *pending = chars.into_iter().collect();

        let mut syls = self.pending_syllables.lock().unwrap();
        let s_pos = insert_pos.min(syls.len());
        syls.splice(s_pos..s_pos, chunk_syls.iter().cloned());

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

        let mut syls = self.pending_syllables.lock().unwrap();
        let s_start = start.min(syls.len());
        let s_end = end.min(syls.len());
        if s_start <= s_end {
            if rep_len == 1 && old_len == 1 {
                if s_start < syls.len() {
                    syls[s_start] = new_syl.clone();
                }
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
        if rep_len == old_len && rep_len > 1 {
            let new_word: String = replacement.iter().collect();
            for seg in segs.iter_mut() {
                if seg.0 == start && seg.1 == end {
                    seg.2 = new_word.clone();
                }
            }
        } else if rep_len != old_len {
            let diff = rep_len as isize - old_len as isize;
            segs.retain(|s| !(s.0 >= start && s.1 <= end));
            for seg in segs.iter_mut() {
                if seg.0 >= end {
                    seg.0 = (seg.0 as isize + diff).max(0) as usize;
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
    }

    fn consolidate_engine_to_pending(&self) {
        let engine = self.engine.lock().unwrap();
        if !engine.composed_sentence.is_empty() {
            let eng_chars: Vec<char> = engine.composed_sentence.chars().collect();
            let eng_count = eng_chars.len();
            let eng_syls: Vec<Option<String>> = (0..eng_count)
                .map(|i| engine.get_syllable_at(i))
                .collect();
            let eng_segs: Vec<(usize, usize, String, String)> = engine.segments.iter()
                .map(|n| (n.start_idx, n.end_idx, n.word.clone(), n.bopomofo.clone()))
                .collect();
            drop(engine);

            self.splice_pending_chunk(&eng_chars, &eng_syls, &eng_segs);

            self.engine.lock().unwrap().reset_buffer();
        }
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
            let content = format!("詞彙：{}\n注音：{}\n安全基準權重：{}\n上下文引力前綴：{}", word, bpmf, safe_weight, prev_display);
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

        let target_idx = if cur < chars.len() {
            cur
        } else if cur > 0 {
            cur - 1
        } else {
            0
        };

        // 1. 若有手動 Shift+方向鍵選取（長度 >= 1），採納選取區間
        let manual_range = self.get_manual_selection_range();

        let engine = self.engine.lock().unwrap();
        let mut cands = Vec::new();

        if let Some((start, end)) = manual_range {
            // 使用者主動 Shift 選取區間：提供該多字詞或單字的同音詞/字
            let target_word: String = chars[start..end].iter().collect();
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
            let is_punct = matches!(ch, '，' | '。' | '！' | '？' | '；' | '：' | '、' | '…' | '—' | '～' | '(' | ')' | '（' | '）' | '[' | ']' | '{' | '}' | '<' | '>' | '《' | '》' | '「' | '」' | '『' | '』' | ',' | '.' | '?' | '!' | ';' | ':');

            if is_punct {
                let punct_cands: Vec<String> = match ch {
                    '，' | ',' => vec!["，".into(), "、".into(), ",".into(), "﹐".into(), "﹑".into()],
                    '、' => vec!["、".into(), "，".into(), "﹑".into(), ",".into()],
                    '。' | '.' => vec!["。".into(), ".".into(), "．".into(), "·".into(), "•".into()],
                    '？' | '?' => vec!["？".into(), "?".into(), "⁇".into(), "⁈".into(), "¿".into()],
                    '！' | '!' => vec!["！".into(), "!".into(), "‼".into(), "⁉".into(), "¡".into()],
                    '：' | ':' => vec!["：".into(), ":".into(), "；".into(), ";".into()],
                    '；' | ';' => vec!["；".into(), ";".into(), "：".into(), ":".into()],
                    '…' => vec!["……".into(), "…".into(), "⋯".into(), "...".into()],
                    '—' | '-' | '─' => vec!["──".into(), "——".into(), "—".into(), "─".into(), "－".into(), "-".into(), "～".into(), "~".into(), "＿".into(), "_".into()],
                    '～' | '~' => vec!["～".into(), "~".into(), "—".into(), "──".into()],
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

                if final_cands.is_empty() && final_idx < chars.len() {
                    final_cands.push(chars[final_idx].to_string());
                }

                let del_words = self.deleted_words.lock().unwrap();
                final_cands.retain(|c| !del_words.contains(c));
                let target_range = (final_idx, (final_idx + 1).min(chars.len()));
                (final_idx, target_range, final_cands)
            }
        }
    }

    fn commit_current_composition(&self, ctx: &ITfContext) -> Result<()> {
        self.consolidate_engine_to_pending();
        let (full, _) = self.get_full_display_and_cursor();
        self.pending_compose.lock().unwrap().clear();
        {
            let mut eng = self.engine.lock().unwrap();
            if eng.has_pending_revisions() {
                let _ = eng.auto_synthesize_and_apply_revisions();
            }
            eng.reset_buffer();
        }
        *self.sentence_cursor.lock().unwrap() = 0;
        *self.candidate_selecting.lock().unwrap() = false;

        self.hide_candidate_window();
        set_last_caret_pos(0, 0);

        if !full.is_empty() {
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
        let eng_cur = engine.cursor_pos;

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
        let now = std::time::Instant::now();
        let paths = RuhiPaths::resolve();

        let should_check = {
            let lock = LAST_CONFIG_CHECK.lock().unwrap();
            match *lock {
                Some((last_instant, _)) => now.duration_since(last_instant).as_millis() > 300,
                None => true,
            }
        };

        if should_check {
            if let Ok(metadata) = std::fs::metadata(&paths.config) {
                if let Ok(mtime) = metadata.modified() {
                    let mut lock = LAST_CONFIG_CHECK.lock().unwrap();
                    let needs_reload = match *lock {
                        Some((_, prev_mtime)) => mtime > prev_mtime,
                        None => true,
                    };
                    *lock = Some((now, mtime));
                    if needs_reload {
                        let new_cfg = load_ruhi_config(&paths);
                        logger::log(&format!("Reloaded config: mode={}, keyboard={}, font_size={}", new_cfg.input_display_mode, new_cfg.keyboard_mode, new_cfg.font_size));
                        if let Ok(mut eng) = self.engine.lock() {
                            eng.keyboard_mode = new_cfg.keyboard_mode.clone();
                            eng.enable_mixed_english = new_cfg.enable_mixed_english;
                            eng.mixed_english_mode = new_cfg.mixed_english_mode.clone();
                            eng.mixed_english_min_len = new_cfg.mixed_english_min_len;
                        }
                        if let Ok(mut cw_guard) = self.candidate_window.lock() {
                            if let Some(cw) = cw_guard.as_mut() {
                                cw.apply_config(&new_cfg);
                            }
                        }
                        *self.config.lock().unwrap() = new_cfg;
                    }
                }
            }
        }
    }

    fn probe_caret(&self, ctx: &ITfContext) -> Result<()> {
        let client_id = *self.client_id.lock().unwrap();
        let sink: ITfCompositionSink = self.to_object().cast()?;
        let edit_session: ITfEditSession = RuhiEditSession::new(
            ctx.clone(),
            EditSessionAction::ProbeCaret,
            self.composition.clone(),
            sink,
        ).into();
        unsafe {
            // TF_ES_SYNC | TF_ES_READ 幾乎 100% 同步准予，絕不阻塞宿主渲染管線
            let hr = ctx.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READ);
            if hr.is_err() {
                let _ = ctx.RequestEditSession(client_id, &edit_session, TF_ES_ASYNC | TF_ES_READ);
            }
        }
        Ok(())
    }

    fn sync_input_display(&self, ctx: &ITfContext) -> Result<()> {
        let has_buffer = {
            let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
            let engine = self.engine.lock().unwrap();
            pending_len > 0 || engine.fsm.has_pending() || !engine.fsm.syllables.is_empty() || !engine.composed_sentence.is_empty()
        };

        if !has_buffer {
            self.hide_candidate_window();
            let _ = self.end_tsf_composition(ctx);
            return Ok(());
        }

        // ── 檢查組字行長度是否超出設定上限，若超長則將現有第一個標點符號之前（左方含標點）的內容自動結算送出 ──
        let max_comp_len = self.config.lock().map(|c| c.max_composition_chars).unwrap_or(30);
        let (full_text_pre, cur_pos_pre) = self.get_full_display_and_cursor();
        let full_char_count = full_text_pre.chars().count();

        if full_char_count > max_comp_len {
            let is_punct = |c: char| matches!(c, '，' | '。' | '！' | '？' | '；' | '：' | '、' | '…' | '—' | '～' | ',' | '.' | '?' | '!' | ';' | ':');
            let punct_pos = full_text_pre.char_indices().enumerate().find(|&(_, (_, c))| is_punct(c)).map(|(idx, _)| idx);

            if let Some(p_idx) = punct_pos {
                self.consolidate_engine_to_pending();
                let chars: Vec<char> = full_text_pre.chars().collect();
                let commit_len = p_idx + 1;
                let commit_chunk: String = chars[..commit_len].iter().collect();
                let remain_chunk: String = chars[commit_len..].iter().collect();

                let _ = self.update_tsf_composition(ctx, &commit_chunk, commit_len, None, true);

                *self.pending_compose.lock().unwrap() = remain_chunk;
                self.engine.lock().unwrap().reset_buffer();
                let new_cur = cur_pos_pre.saturating_sub(commit_len);
                *self.sentence_cursor.lock().unwrap() = new_cur;

                return self.sync_input_display(ctx);
            }
        }

        let (full_text, cur_pos) = self.get_full_display_and_cursor();
        let is_cand_sel = *self.candidate_selecting.lock().unwrap();
        let target_idx = *self.target_char_idx.lock().unwrap();
        let cands = self.active_candidates.lock().unwrap().clone();
        let sel_idx = *self.selected_candidate_idx.lock().unwrap();
        let input_mode = self.config.lock().map(|c| c.input_display_mode.clone()).unwrap_or_else(|_| "inline_underline".to_string());
        let pending_syl = {
            let engine = self.engine.lock().unwrap();
            if engine.fsm.has_pending() {
                format!("{}{}{}", engine.fsm.cur_initial, engine.fsm.cur_medial, engine.fsm.cur_final)
            } else {
                String::new()
            }
        };

        let sel_range = self.get_manual_selection_range();

        if input_mode == "inline_underline" {
            self.update_tsf_composition(ctx, &full_text, cur_pos, sel_range, false)?;
            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    if is_cand_sel && !cands.is_empty() {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        cw.show_candidates_only(&cands, sel_idx, cx, cy);
                    } else {
                        cw.hide();
                    }
                }
            }
        } else {
            // 如喜輸入座模式 (floating_follow 或 floating_fixed)
            let _ = self.probe_caret(ctx);
            let (cx, cy) = Self::get_caret_screen_pos();
            let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };

            let target_range = *self.target_range.lock().unwrap();
            let sel_range_pair = sel_range.unwrap_or((0, 0));

            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.show_desk(&full_text, cur_pos, &pending_syl, &cands, sel_idx, is_cand_sel, target_idx, target_range, sel_range_pair, is_caps_on, cx, cy);
                }
            }
        }
        Ok(())
    }

    fn update_tsf_composition(&self, ctx: &ITfContext, text: &str, cursor_pos: usize, selection: Option<(usize, usize)>, is_commit: bool) -> Result<()> {
        let client_id = *self.client_id.lock().unwrap();
        let sink: ITfCompositionSink = self.to_object().cast()?;
        let edit_session: ITfEditSession = RuhiEditSession::new(
            ctx.clone(),
            EditSessionAction::UpdateComposition {
                text: text.to_string(),
                cursor_pos,
                selection,
                is_commit,
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

    fn end_tsf_composition(&self, ctx: &ITfContext) -> Result<()> {
        set_last_caret_pos(0, 0);
        let client_id = *self.client_id.lock().unwrap();
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
        let (cx, cy) = get_last_caret_pos();
        if cx > 0 && cy > 0 {
            logger::log(&format!("get_caret_screen_pos from TSF ActiveView: ({}, {})", cx, cy));
            return (cx, cy);
        }

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

        if unsafe { GetGUIThreadInfo(fg_tid, &mut gui_info) }.is_ok() {
            if gui_info.rcCaret.left != 0 || gui_info.rcCaret.top != 0 || gui_info.rcCaret.right != 0 {
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
                    unsafe { let _ = windows::Win32::Graphics::Gdi::ClientToScreen(target, &mut pt); }
                    got_caret = true;
                }
            }
        }

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

        logger::log(&format!("get_caret_screen_pos: x={}, y={}, got_caret={}", pt.x, pt.y, got_caret));
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
            if pdimfocus.is_none() {
                let over_window = self.candidate_window.lock().ok()
                    .and_then(|guard| guard.as_ref().map(|cw| cw.is_cursor_over()))
                    .unwrap_or(false);
                if !over_window {
                    self.hide_candidate_window();
                }
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
            self.hide_candidate_window();
            set_last_caret_pos(0, 0);
            Ok(())
        })).unwrap_or_else(|_| Err(E_FAIL.into()))
    }
}

impl ITfDisplayAttributeProvider_Impl for RuhiTextService_Impl {
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        Ok(crate::display_attribute::RuhiDisplayAttributeEnum::new().into())
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        if guid.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let g = unsafe { *guid };
        if g == crate::guid::GUID_RUHI_DISPLAY_ATTR {
            Ok(crate::display_attribute::RuhiDisplayAttributeInfo::new().into())
        } else {
            Err(E_INVALIDARG.into())
        }
    }
}
