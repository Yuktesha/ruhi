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
    pub manual_cuts: std::collections::HashSet<usize>,
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
use ruhi::fsm::HsuSyllable;
use ruhi::router::LatticeDAG;
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

static LAST_CONFIG_CHECK: Mutex<Option<(std::time::Instant, std::time::SystemTime, RuhiConfig)>> = Mutex::new(None);
static LAST_FC_MTIME: Mutex<Option<std::time::SystemTime>> = Mutex::new(None);
static CURRENT_DLL_INSTANCES: Mutex<Option<std::collections::HashSet<usize>>> = Mutex::new(None);

pub fn invalidate_config_cache() {
    if let Ok(mut lock) = LAST_CONFIG_CHECK.lock() {
        *lock = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RuhiConfig {
    #[serde(default = "default_keyboard_mode")]
    pub keyboard_mode: String,
    #[serde(default = "default_last_keyboard_mode")]
    pub last_keyboard_mode: String,
    #[serde(default = "default_enabled_keyboard_modes")]
    pub enabled_keyboard_modes: Vec<String>,
    #[serde(default = "default_aux_mode")]
    pub backtick_inline_mode: String,
    #[serde(default = "default_aux_mode")]
    pub tilde_toggle_mode: String,
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
}

impl Default for RuhiConfig {
    fn default() -> Self {
        Self {
            keyboard_mode: default_keyboard_mode(),
            last_keyboard_mode: default_last_keyboard_mode(),
            enabled_keyboard_modes: default_enabled_keyboard_modes(),
            backtick_inline_mode: default_aux_mode(),
            tilde_toggle_mode: default_aux_mode(),
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
        }
    }
}

fn default_telemetry_consent() -> bool { false }
fn default_reverse_coach_target() -> String { "none".to_string() }
fn default_tailcat_auto_sync() -> bool { true }
fn default_upload_schedule() -> String { "weekly".to_string() }
fn default_tailcat_update_mode() -> String { "notify_confirm".to_string() }

fn default_keyboard_mode() -> String { "hsu".to_string() }
fn default_last_keyboard_mode() -> String { "english".to_string() }
fn default_enabled_keyboard_modes() -> Vec<String> { vec!["hsu".to_string(), "japanese".to_string()] }
fn default_aux_mode() -> String { "longkan".to_string() }
fn default_candidate_quick_key() -> String { "asdf".to_string() }
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
        let _ = windows::Win32::System::Com::CoInitialize(None);
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
    // 若使用者從非中文語言入口（如英文、日文、韓文）啟動 RuhI，或在兩個不同語言入口間切換：
    if prev_langid != active_langid && (primary != 0x04 || prev_primary != 0) {
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
fn default_capslock_switch() -> String { "capital_lock".to_string() }
fn default_capsule_mode() -> String { "mini_capsule".to_string() }
fn default_confirm_add_word() -> bool { true }
fn default_confirm_suppress_word() -> bool { true }
fn default_four_corner_keymap() -> String { "blind_closed_loop".to_string() }

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
    pub array30: std::path::PathBuf,
    pub four_corner: std::path::PathBuf,
    pub longkan: std::path::PathBuf,
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

        // Resolve config/user directory (優先以專案根目錄為單一權威組態源，確保 scripts/ 與根目錄永不分裂)
        let sandbox_dir = if dll_dir.join("..").join("config.json").exists() && dll_dir.join("..").join("ruhi_lexicon").is_dir() {
            dll_dir.join("..")
        } else if dll_dir.join("config.json").exists() {
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
            array30,
            four_corner,
            longkan,
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
    LANGBAR_SINKS.with(|sinks_cell| {
        if let Ok(sinks) = sinks_cell.try_borrow() {
            for (_, sink) in sinks.iter() {
                unsafe {
                    let _ = sink.OnUpdate(TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP | TF_LBI_STATUS);
                }
            }
        }
    });
}

#[implement(ITfLangBarItemButton, ITfSource)]
pub struct RuhiLangBarItemButton;

impl ITfLangBarItem_Impl for RuhiLangBarItemButton_Impl {
    fn GetInfo(&self, pinfo: *mut TF_LANGBARITEMINFO) -> Result<()> {
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
    }

    fn GetStatus(&self) -> Result<u32> {
        Ok(0)
    }

    fn Show(&self, _fshow: BOOL) -> Result<()> {
        notify_langbar_mode_update();
        Ok(())
    }

    fn GetTooltipString(&self) -> Result<BSTR> {
        let paths = RuhiPaths::resolve();
        let cfg = load_ruhi_config(&paths);
        let (_badge_unused, _mode_name_unused) = crate::candidate_window::get_mode_badge_and_name(&cfg.keyboard_mode, false);
        let ui_lang = resolve_effective_ui_lang(&cfg.ui_lang);
        let (badge, mode_name) = crate::candidate_window::get_mode_badge_and_name_i18n(&cfg.keyboard_mode, false, ui_lang);
        let tip = match ui_lang {
            "en-US" => format!("RuhI — {} ({}) [Left-click: Switch / Right-click: Menu]", mode_name, badge),
            "ja-JP" => format!("RuhI 如喜 — {} ({}) [左クリック: 切替 / 右クリック: メニュー]", mode_name, badge),
            "ko-KR" => format!("RuhI 루히 — {} ({}) [좌클릭: 전환 / 우클릭: 메뉴]", mode_name, badge),
            _ => format!("RuhI 如喜 — {} ({}) [左鍵輪流切換／右鍵開啟選單]", mode_name, badge),
        };
        Ok(BSTR::from(tip))
    }
}

impl ITfLangBarItemButton_Impl for RuhiLangBarItemButton_Impl {
    fn OnClick(&self, click: TfLBIClick, pt: &POINT, _prcarea: *const RECT) -> Result<()> {
        unsafe {
            if click == TF_LBI_CLK_LEFT {
                crate::candidate_window::cycle_next_keyboard_mode_global(Some(*pt));
            } else {
                crate::candidate_window::show_taskbar_ri_menu(Some(*pt));
            }
        }
        Ok(())
    }

    fn InitMenu(&self, pmenu: Option<&ITfMenu>) -> Result<()> {
        if let Some(menu) = pmenu {
            let paths = RuhiPaths::resolve();
            let cfg = load_ruhi_config(&paths);
            let cur_kb = cfg.keyboard_mode.to_lowercase();
            let is_direct_en_caps = cfg.capslock_switch == "english_direct";
            let ui_lang = resolve_effective_ui_lang(&cfg.ui_lang);
            let items = crate::candidate_window::get_mode_menu_items_i18n(ui_lang);
            let _unused_items: &[(u32, &str, &str)] = &[
                (2001, "hsu", "許氏雙拼注音 (許)\tCtrl/Shift + CapsLock"),
                (2005, "japanese", "日文羅馬字輸入 (日)\tCtrl/Shift + CapsLock"),
                (2009, "korean", "韓文羅馬拼音輸入 (韓)\tCtrl/Shift + CapsLock"),
                (2004, "cangjie", "倉頡輸入法 (倉)\tCtrl/Shift + CapsLock"),
                (2007, "array30", "行列 30 輸入法 (行)\tCtrl/Shift + CapsLock"),
                (2002, "standard", "標準大千注音 (注)\tCtrl/Shift + CapsLock"),
                (2003, "et26", "倚天 26 鍵注音 (倚)\tCtrl/Shift + CapsLock"),
            ];
            let null_bmp = windows::Win32::Graphics::Gdi::HBITMAP::default();
            unsafe {
                for &(id, mode_code, label) in items {
                    let is_active = cur_kb == mode_code
                        || (mode_code == "japanese" && (cur_kb == "jp" || cur_kb == "romaji" || cur_kb == "kana"))
                        || (mode_code == "korean" && (cur_kb == "ko" || cur_kb == "hangul" || cur_kb == "romaja"))
                        || (mode_code == "array30" && cur_kb == "ar30")
                        || (mode_code == "four_corner" && (cur_kb == "4corner" || cur_kb == "fc"))
                        || (mode_code == "longkan" && cur_kb == "lk")
                        || (mode_code == "standard" && cur_kb == "daikien")
                        || (mode_code == "et26" && cur_kb == "eten26");
                    let lbl = if is_active { format!("✓ {}", label) } else { format!("   {}", label) };
                    let wide: Vec<u16> = lbl.encode_utf16().collect();
                    let _ = menu.AddMenuItem(id, 0, null_bmp, null_bmp, &wide, std::ptr::null_mut());
                }
                let is_en_active = matches!(cur_kb.as_str(), "english" | "en" | "latin" | "western" | "euro" | "sanskrit");
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
                let en_hotkey = if is_direct_en_caps { "CapsLock" } else { "Ctrl/Shift + CapsLock" };
                let en_lbl = if is_en_active {
                    format!("✓ {}\t{}", en_base_lbl, en_hotkey)
                } else {
                    format!("   {}\t{}", en_base_lbl, en_hotkey)
                };
                let _unused_en_lbl = if false {
                    if is_en_active {
                        "✓ 泛歐／羅馬字模式 (歐)\tCapsLock".to_string()
                    } else {
                        "   泛歐／羅馬字模式 (歐)\tCapsLock".to_string()
                    }
                } else {
                    if is_en_active {
                        "✓ 泛歐／羅馬字模式 (歐)\tCtrl/Shift + CapsLock".to_string()
                    } else {
                        "   泛歐／羅馬字模式 (歐)\tCtrl/Shift + CapsLock".to_string()
                    }
                };
                let en_wide: Vec<u16> = en_lbl.encode_utf16().collect();
                let _ = menu.AddMenuItem(2006, 0, null_bmp, null_bmp, &en_wide, std::ptr::null_mut());

                if !is_direct_en_caps {
                    let (cur_badge, _) = crate::candidate_window::get_mode_badge_and_name_i18n(&cfg.keyboard_mode, false, ui_lang);
                    let last_code = if cfg.last_keyboard_mode.eq_ignore_ascii_case(&cfg.keyboard_mode) {
                        if is_en_active {
                            cfg.enabled_keyboard_modes.first().map(|s| s.as_str()).unwrap_or("hsu")
                        } else {
                            "english"
                        }
                    } else {
                        cfg.last_keyboard_mode.as_str()
                    };
                    let (last_badge, _) = crate::candidate_window::get_mode_badge_and_name_i18n(last_code, false, ui_lang);
                    let mru_lbl = format!("   {} ({} ⇄ {})\tCapsLock", mru_prefix, cur_badge, last_badge);
                    let mru_wide: Vec<u16> = mru_lbl.encode_utf16().collect();
                    let _ = menu.AddMenuItem(2008, 0, null_bmp, null_bmp, &mru_wide, std::ptr::null_mut());
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
    }

    fn OnMenuSelect(&self, wid: u32) -> Result<()> {
        let is_caps = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };
        if wid == 2008 {
            crate::candidate_window::toggle_last_two_keyboard_modes_global(None);
            if is_caps {
                trigger_synthetic_caps_toggle();
            }
            return Ok(());
        }
        let chosen = match wid {
            2001 => Some("hsu"),
            2002 => Some("standard"),
            2003 => Some("et26"),
            2004 => Some("cangjie"),
            2005 => Some("japanese"),
            2006 => Some("english"),
            2007 => Some("array30"),
            2009 => Some("korean"),
            2010 => Some("four_corner"),
            2013 => Some("longkan"),
            _ => None,
        };
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
    }

    fn GetIcon(&self) -> Result<HICON> {
        let paths = RuhiPaths::resolve();
        let cfg = load_ruhi_config(&paths);
        let (badge, _) = crate::candidate_window::get_mode_badge_and_name(&cfg.keyboard_mode, false);
        let hicon = unsafe { crate::candidate_window::create_mode_hicon(badge, false) };
        Ok(hicon)
    }

    fn GetText(&self) -> Result<BSTR> {
        let paths = RuhiPaths::resolve();
        let cfg = load_ruhi_config(&paths);
        let (badge, _) = crate::candidate_window::get_mode_badge_and_name(&cfg.keyboard_mode, false);
        Ok(BSTR::from(badge))
    }
}

impl ITfSource_Impl for RuhiLangBarItemButton_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Option<&IUnknown>) -> Result<u32> {
        if riid.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let iid = unsafe { *riid };
        if iid != ITfLangBarItemSink::IID {
            return Err(E_NOINTERFACE.into());
        }
        let unk = punk.ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let sink: ITfLangBarItemSink = unk.cast()?;
        unsafe {
            let _ = sink.OnUpdate(TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP | TF_LBI_STATUS);
        }
        let cookie = NEXT_LANGBAR_COOKIE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        LANGBAR_SINKS.with(|sinks_cell| {
            if let Ok(mut sinks) = sinks_cell.try_borrow_mut() {
                sinks.push((cookie, sink));
            }
        });
        Ok(cookie)
    }

    fn UnadviseSink(&self, dwcookie: u32) -> Result<()> {
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
        engine.last_keyboard_mode = config.last_keyboard_mode.clone();
        engine.set_enabled_keyboard_modes(config.enabled_keyboard_modes.clone());
        engine.backtick_inline_mode = config.backtick_inline_mode.clone();
        engine.tilde_toggle_mode = config.tilde_toggle_mode.clone();
        engine.enable_mixed_english = config.enable_mixed_english;
        engine.mixed_english_mode = config.mixed_english_mode.clone();
        engine.mixed_english_min_len = config.mixed_english_min_len;
        engine.enable_array30_coach = config.enable_array30_coach;
        engine.reverse_coach_target = config.reverse_coach_target.clone();
        engine.four_corner.keymap_profile = config.four_corner_keymap.clone();
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
        if paths.array30.exists() && engine.array30.table.reverse_lookup('㐀').is_none() {
            let _ = engine.array30.table.load_tsv(&paths.array30);
        }
        if paths.four_corner.exists() && engine.four_corner.table.code_to_entries.len() < 1000 {
            let _ = engine.four_corner.load_tsv(&paths.four_corner);
        }
        engine.longkan.table.configure_data_dir(&paths.data_dir);
        if let Ok(fc_meta) = std::fs::metadata(&paths.four_corner) {
            if let Ok(fc_mtime) = fc_meta.modified() {
                if let Ok(mut fc_lock) = LAST_FC_MTIME.lock() {
                    *fc_lock = Some(fc_mtime);
                }
            }
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
            deleted_words: Arc::new(Mutex::new(del_words)),
            undo_stack: Arc::new(Mutex::new(Vec::new())),
            redo_stack: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(invalid_reference_casting)]
    fn ensure_instance_upgraded(&self) {
        let inst_key = Arc::as_ptr(&self.client_id) as usize;
        let already_upgraded = {
            if let Ok(mut guard) = CURRENT_DLL_INSTANCES.lock() {
                let set = guard.get_or_insert_with(std::collections::HashSet::new);
                if set.contains(&inst_key) {
                    true
                } else {
                    set.insert(inst_key);
                    false
                }
            } else {
                true
            }
        };
        if already_upgraded {
            return;
        }

        logger::log(&format!(
            "Hot-upgrading legacy RuhiTextService instance in-place (inst_key={:#x})...",
            inst_key
        ));

        if let Ok(cw_guard) = self.candidate_window.lock() {
            if let Some(ref w) = *cw_guard {
                if !w.hwnd.0.is_null() {
                    unsafe {
                        let _ = KillTimer(w.hwnd, 2001);
                        let _ = DestroyWindow(w.hwnd);
                    }
                }
            }
        }

        let paths = RuhiPaths::resolve();
        let mut config = load_ruhi_config(&paths);
        sync_active_tsf_profile_locale(&mut config);
        let del_words = crate::user_word_dialog::load_deleted_words(&paths);
        let mut engine = RuhiEngine::new_with_data_dir(&paths.data_dir.to_string_lossy());
        engine.keyboard_mode = config.keyboard_mode.clone();
        engine.last_keyboard_mode = config.last_keyboard_mode.clone();
        engine.set_enabled_keyboard_modes(config.enabled_keyboard_modes.clone());
        engine.backtick_inline_mode = config.backtick_inline_mode.clone();
        engine.tilde_toggle_mode = config.tilde_toggle_mode.clone();
        engine.enable_mixed_english = config.enable_mixed_english;
        engine.mixed_english_mode = config.mixed_english_mode.clone();
        engine.mixed_english_min_len = config.mixed_english_min_len;
        engine.enable_array30_coach = config.enable_array30_coach;
        engine.reverse_coach_target = config.reverse_coach_target.clone();
        engine.four_corner.keymap_profile = config.four_corner_keymap.clone();
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
        if paths.array30.exists() && engine.array30.table.reverse_lookup('㐀').is_none() {
            let _ = engine.array30.table.load_tsv(&paths.array30);
        }
        if paths.four_corner.exists() && engine.four_corner.table.code_to_entries.len() < 1000 {
            let _ = engine.four_corner.load_tsv(&paths.four_corner);
        }
        engine.longkan.table.configure_data_dir(&paths.data_dir);

        let cw = match crate::candidate_window::CandidateWindow::new() {
            Ok(mut w) => {
                w.apply_config(&config);
                Some(w)
            }
            Err(_) => None,
        };

        unsafe {
            std::ptr::write(&self.engine as *const _ as *mut _, Arc::new(Mutex::new(engine)));
            std::ptr::write(&self.candidate_window as *const _ as *mut _, Arc::new(Mutex::new(cw)));
            std::ptr::write(&self.pending_compose as *const _ as *mut _, Arc::new(Mutex::new(String::new())));
            std::ptr::write(&self.sentence_cursor as *const _ as *mut _, Arc::new(Mutex::new(0usize)));
            std::ptr::write(&self.candidate_selecting as *const _ as *mut _, Arc::new(Mutex::new(false)));
            std::ptr::write(&self.active_candidates as *const _ as *mut _, Arc::new(Mutex::new(Vec::<String>::new())));
            std::ptr::write(&self.selected_candidate_idx as *const _ as *mut _, Arc::new(Mutex::new(0usize)));
            std::ptr::write(&self.target_char_idx as *const _ as *mut _, Arc::new(Mutex::new(0usize)));
            std::ptr::write(&self.target_range as *const _ as *mut _, Arc::new(Mutex::new((0usize, 0usize))));
            std::ptr::write(&self.config as *const _ as *mut _, Arc::new(Mutex::new(config)));
            std::ptr::write(&self.sel_anchor as *const _ as *mut _, Arc::new(Mutex::new(None::<usize>)));
            std::ptr::write(&self.insert_mode as *const _ as *mut _, Arc::new(Mutex::new(true)));
            std::ptr::write(&self.q_press_count as *const _ as *mut _, Arc::new(Mutex::new(0usize)));
            std::ptr::write(&self.q_last_char_idx as *const _ as *mut _, Arc::new(Mutex::new(None::<usize>)));
            std::ptr::write(&self.pending_syllables as *const _ as *mut _, Arc::new(Mutex::new(Vec::<Option<String>>::new())));
            std::ptr::write(&self.pending_segments as *const _ as *mut _, Arc::new(Mutex::new(Vec::<(usize, usize, String, String)>::new())));
            std::ptr::write(&self.last_raw_ascii as *const _ as *mut _, Arc::new(Mutex::new(None::<(usize, usize, String)>)));
            std::ptr::write(&self.manual_cuts as *const _ as *mut _, Arc::new(Mutex::new(std::collections::HashSet::<usize>::new())));
            std::ptr::write(&self.deleted_words as *const _ as *mut _, Arc::new(Mutex::new(del_words)));
            std::ptr::write(&self.undo_stack as *const _ as *mut _, Arc::new(Mutex::new(Vec::<EditorSnapshot>::new())));
            std::ptr::write(&self.redo_stack as *const _ as *mut _, Arc::new(Mutex::new(Vec::<EditorSnapshot>::new())));
        }

        if let Ok(tm_guard) = self.thread_mgr.lock() {
            if let Some(ref tm) = *tm_guard {
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
                                std::ptr::write(&self.langbar_item as *const _ as *mut _, Arc::new(Mutex::new(Some(item))));
                            }
                        }
                    }
                    notify_langbar_mode_update();
                }
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
                    eng.keyboard_mode = cfg.keyboard_mode.clone();
                    eng.last_keyboard_mode = cfg.last_keyboard_mode.clone();
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
            self.ensure_instance_upgraded();
            logger::log("Deactivate called.");
            self.hide_candidate_window();
            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                *cw_guard = None;
            }
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
                if let Ok(langbar_mgr) = tm.cast::<ITfLangBarItemMgr>() {
                    if let Ok(mut guard) = self.langbar_item.lock() {
                        if let Some(item) = guard.take() {
                            unsafe { let _ = langbar_mgr.RemoveItem(&item); }
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
                    if paths.array30.exists() && eng.array30.table.reverse_lookup('㐀').is_none() {
                        let _ = eng.array30.table.load_tsv(&paths.array30);
                    }
                    if paths.four_corner.exists() && eng.four_corner.table.code_to_entries.len() < 1000 {
                        let _ = eng.four_corner.load_tsv(&paths.four_corner);
                    }
                    if paths.longkan.exists() && eng.longkan.table.reverse_lookup('鋪').is_none() {
                        let _ = eng.longkan.table.load_tsv(&paths.longkan);
                    }
                }
                let mut new_cfg = load_ruhi_config(&paths);
                sync_active_tsf_profile_locale(&mut new_cfg);
                if let Ok(mut eng) = self.engine.lock() {
                    eng.keyboard_mode = new_cfg.keyboard_mode.clone();
                    eng.last_keyboard_mode = new_cfg.last_keyboard_mode.clone();
                    eng.set_enabled_keyboard_modes(new_cfg.enabled_keyboard_modes.clone());
                    eng.backtick_inline_mode = new_cfg.backtick_inline_mode.clone();
                    eng.tilde_toggle_mode = new_cfg.tilde_toggle_mode.clone();
                    eng.enable_array30_coach = new_cfg.enable_array30_coach;
                    eng.four_corner.keymap_profile = new_cfg.four_corner_keymap.clone();
                }
                if let Ok(mut cw_guard) = self.candidate_window.lock() {
                    if let Some(cw) = cw_guard.as_mut() {
                        cw.apply_config(&new_cfg);
                    }
                }
                *self.config.lock().unwrap() = new_cfg.clone();
                crate::candidate_window::maybe_spawn_background_telemetry_upload(&paths, &new_cfg);
                notify_langbar_mode_update();
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
            // 全域快捷鍵：Ctrl+Alt+R 喚醒語意除錯調教工具
            if is_ctrl && is_alt && (vk == 0x52 || vk == 0x54) {
                return Ok(BOOL::from(true));
            }

            if is_alt {
                return Ok(BOOL::from(false));
            }

            let (has_buffer, is_english_mode, is_four_corner_mode) = {
                let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
                let engine = self.engine.lock().unwrap();
                let hb = pending_len > 0
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
                    || matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    );
                let lower_kb = engine.keyboard_mode.to_lowercase();
                let is_en = matches!(lower_kb.as_str(), "english" | "en" | "latin" | "western");
                let is_fc = matches!(lower_kb.as_str(), "four_corner" | "4corner" | "fc");
                (hb, is_en, is_fc)
            };

            // Win32 與 TSF InputScope 密碼控制項隱私保護：若當前聚焦於密碼欄位且無未決組字，自動放行直通英數，絕不攔截組字或寫入修訂記錄
            if !has_buffer && self.is_focused_or_tsf_password(_pic) {
                return Ok(BOOL::from(false));
            }

            // Caps Lock 三向智慧切換處理：
            // 1. 單獨按下 Caps Lock ➔ 最後使用的兩個輸入模式快速對切（如 英⇄漢、日⇄漢、英⇄日）
            // 2. Shift + Caps Lock ➔ 在切換列表中往前切換 (Prev)
            // 3. Ctrl + Caps Lock ➔ 在切換列表中往後切換 (Next)
            if vk == VK_CAPITAL.0 as u32 {
                if is_synthetic_caps_active() || SUPPRESS_SYNTHETIC_CAPS.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    return Ok(BOOL::from(false));
                }
                return Ok(BOOL::from(true));
            }

            if is_ctrl {
                // 優先檢查是否為符號組合鍵 (如 Ctrl+. 輸出刪節號 ……、Ctrl+, 輸出全形逗號等)
                if let Some(combo) = map_vk_to_ctrl_combo(vk, is_shift) {
                    let mut engine = self.engine.lock().unwrap();
                    if engine.symbols.contains_key(&combo) {
                        return Ok(BOOL::from(true));
                    }
                }

                if has_buffer {
                    // 組字行有文字時，全部攔截！防禦外部快捷鍵外洩破壞組字現場
                    return Ok(BOOL::from(true));
                }
                // 組字行無文字時，全部放行給宿主應用程式
                return Ok(BOOL::from(false));
            }

            let is_cand_sel = *self.candidate_selecting.lock().unwrap();
            let quick_mode = self.config.lock().map(|c| c.candidate_quick_key.clone()).unwrap_or_else(|_| "asdf".to_string());

            if is_cand_sel {
                if vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_NEXT.0 as u32 || vk == VK_PRIOR.0 as u32 {
                    return Ok(BOOL::from(true));
                }
                if is_four_corner_mode && (vk == 0x6B || vk == 0x6D || (vk == 0xBB && is_shift) || (vk == 0xBD && !is_shift)) {
                    return Ok(BOOL::from(true));
                }
                if quick_mode == "asdf" && map_asdf_to_idx(vk).is_some() && !is_shift {
                    return Ok(BOOL::from(true));
                }
                if (quick_mode == "1234" || is_four_corner_mode)
                    && ((0x30..=0x39).contains(&vk) || (0x60..=0x69).contains(&vk))
                    && !is_shift
                {
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

            // 【自然輸入法 UI/UX 精神】：英數／歐文模式 (english) 時，直接納入組字流中，絕不提前送出！
            if is_english_mode {
                let is_en_special_or_tilde = {
                    let eng = self.engine.lock().unwrap();
                    eng.inline_longkan_active
                        || eng.tilde_return_mode.is_some()
                        || matches!(
                            eng.mode,
                            ruhi::engine::EngineMode::QuickNum
                                | ruhi::engine::EngineMode::TemplateSelect
                                | ruhi::engine::EngineMode::LexiconSwitch
                        )
                };
                if is_en_special_or_tilde && vk == VK_OEM_3.0 as u32 {
                    return Ok(BOOL::from(true));
                }
                if !is_en_special_or_tilde {
                    if (0x41..=0x5A).contains(&vk) {
                        return Ok(BOOL::from(true));
                    }
                    if has_buffer {
                        if vk == VK_RETURN.0 as u32 || vk == VK_SPACE.0 as u32 || vk == VK_BACK.0 as u32 || vk == VK_ESCAPE.0 as u32 || vk == VK_DELETE.0 as u32 {
                            return Ok(BOOL::from(true));
                        }
                        if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_NEXT.0 as u32 {
                            return Ok(BOOL::from(true));
                        }
                        if (0x30..=0x39).contains(&vk) || is_punctuation_vk(vk, is_shift) {
                            return Ok(BOOL::from(true));
                        }
                    }
                    return Ok(BOOL::from(false));
                }
            }

            // 【自然輸入法關鍵修正】：Shift+字母直接納入組字流，絕不洩漏給外部應用程式
            // （惟智慧四角模式下，主鍵盤區負責正常輸入一般英數字，無組字緩衝區時直接放行！）
            if is_shift && (0x41..=0x5A).contains(&vk) && (!is_four_corner_mode || has_buffer) {
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
                if vk == VK_OEM_3.0 as u32 {
                    eaten = true;
                } else if is_four_corner_mode {
                    let is_direct_fc_punct = self
                        .config
                        .lock()
                        .map(|c| c.four_corner_keymap == "direct_punct")
                        .unwrap_or(false);
                    if !is_direct_fc_punct && matches!(vk, 0x6B | 0x6F) {
                        // 四角全盲打閉環模式下，當輸入座完全空白無組字時，小鍵盤 `+`(字詞分隔)、`/`(字詞選單) 直接放行；
                        // 惟小鍵盤 `-`(0x6D 詞前導)、`.`(0x6E 標點選單前導)、`*`(0x6A 萬用字元) 及 `0~9`(0x60..=0x69) 必須攔截進入對應模式！
                        eaten = false;
                    } else if matches!(vk, 0x60..=0x6B | 0x6D..=0x6F) && !is_shift {
                        // 九宮格小鍵盤專責四角編碼與選單控制
                        eaten = true;
                    } else {
                        // 主鍵盤區（A~Z、上方橫排數字 0~9、主鍵盤符號）在無組字緩衝區時一律直接放行輸出正常英數字與符號！
                        eaten = false;
                    }
                } else if is_punctuation_vk(vk, is_shift) {
                    eaten = true;
                } else if (0x41..=0x5A).contains(&vk) {
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
            logger::log(&format!("OnKeyDown: vk={:#x}", vk));

            let is_ctrl = unsafe { ((GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0) };
            let is_alt = unsafe { ((GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0) };
            let is_shift = unsafe { ((GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) || ((GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0) };
            let is_caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };

            // 全域快捷鍵：Ctrl+Alt+R 或 Ctrl+Alt+T 喚起「語意除錯調校工作室」
            if is_ctrl && is_alt && (vk == 0x52 || vk == 0x54) {
                crate::candidate_window::launch_semantic_tuner();
                return Ok(BOOL::from(true));
            }

            if is_alt {
                return Ok(BOOL::from(false));
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

            let (has_buffer, is_english_mode, is_four_corner_mode, is_longkan_mode) = {
                let pending_len = self.pending_compose.lock().map(|p| p.chars().count()).unwrap_or(0);
                let engine = self.engine.lock().unwrap();
                let hb = pending_len > 0
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
                    || matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    );
                let lower_kb = engine.keyboard_mode.to_lowercase();
                let is_en = matches!(lower_kb.as_str(), "english" | "en" | "latin" | "western");
                let is_fc = matches!(lower_kb.as_str(), "four_corner" | "4corner" | "fc");
                let is_lk = matches!(lower_kb.as_str(), "longkan" | "lk");
                (hb, is_en, is_fc, is_lk)
            };

            // Win32 與 TSF InputScope 密碼控制項隱私保護：若當前聚焦於密碼欄位且無未決組字，自動放行直通英數，絕不攔截組字或寫入修訂記錄
            if !has_buffer && self.is_focused_or_tsf_password(Some(ctx)) {
                return Ok(BOOL::from(false));
            }

            // 當完全沒有組字緩衝區時，Backspace、Delete、Enter、Space、Tab（龍龕模式除外，龍龕可按 Tab 引導詞彙簡碼模式）、方向鍵等一律直接放行給目標視窗！
            if !has_buffer {
                if vk == VK_RETURN.0 as u32 || vk == VK_BACK.0 as u32 || vk == VK_DELETE.0 as u32 || vk == VK_SPACE.0 as u32 || (vk == VK_TAB.0 as u32 && !is_longkan_mode) || vk == VK_ESCAPE.0 as u32 || (vk == 0x6E && !is_four_corner_mode) {
                    return Ok(BOOL::from(false));
                }
                if vk == VK_LEFT.0 as u32 || vk == VK_RIGHT.0 as u32 || vk == VK_UP.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_HOME.0 as u32 || vk == VK_END.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_NEXT.0 as u32 {
                    return Ok(BOOL::from(false));
                }
            }

            // 英數／歐文模式 (english) 且未處於候選字選取模式時：
            // 若處於候選字選取模式（如按 ↓ 選 café / á，或按 ↑ 選 thanks），則放行至下方選字邏輯！
            let is_en_inline_or_special = {
                let eng = self.engine.lock().unwrap();
                eng.inline_longkan_active
                    || matches!(
                        eng.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    )
            };
            if is_english_mode && !*self.candidate_selecting.lock().unwrap() && !is_en_inline_or_special {
                if vk == VK_OEM_3.0 as u32 && self.engine.lock().map(|e| e.tilde_return_mode.is_some()).unwrap_or(false) {
                    // 若由 `~` 臨時切換至英數模式，按 `~` / `` ` `` 交由下方引擎處理以返回原輸入法
                } else {
                    if (0x41..=0x5A).contains(&vk) {
                        let base = vk as u8 as char;
                        let ch = if is_shift { base.to_ascii_uppercase() } else { base.to_ascii_lowercase() };

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
            }

            let punct_mode = self.config.lock().map(|c| c.punctuation_commit_mode.clone()).unwrap_or_else(|_| "B".to_string());
            let quick_mode = self.config.lock().map(|c| c.candidate_quick_key.clone()).unwrap_or_else(|_| "asdf".to_string());

            // 1. 處理 Ctrl 組合鍵與編輯器操作 (Ctrl+A, Ctrl+C, Ctrl+V, Ctrl+X, Ctrl+Z, Ctrl+Y/R 等)
            if is_ctrl {
                // 1.0 優先檢查 Ctrl 符號組合鍵 (如 Ctrl+. 輸出刪節號 ……、Ctrl+, 輸出全形逗號等)
                if let Some(combo) = map_vk_to_ctrl_combo(vk, is_shift) {
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

                            let is_sentence_terminator = matches!(committed_chunk.as_str(), "。" | "？" | "！" | "；" | "…" | "……" | "?" | "!");
                            let should_keep_in_stream = match punct_mode.as_str() {
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
                        self.update_candidate_selection_ui(new_idx, &cands);
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
                    let target_range = *self.target_range.lock().unwrap();
                    let target_idx = *self.target_char_idx.lock().unwrap();
                    let (full_text, _) = self.get_full_display_and_cursor();
                    let chars: Vec<char> = full_text.chars().collect();

                    let target_len = target_range.1.saturating_sub(target_range.0);
                    let chosen_len = chosen.chars().count();

                    let (replace_start, replace_end) = if target_len == 1 && chosen_len > 1 {
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
                        && (chosen_len == target_len || chosen_len > 1 || target_len == 1)
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
                    if is_four_corner_mode && vk == VK_RETURN.0 as u32 {
                        // 四角號碼模式下，Enter 確認選字後立即將整句送出至目標視窗（`+` 則留在輸入座繼續組句）
                        self.commit_current_composition(ctx)?;
                    } else {
                        self.sync_input_display(ctx)?;
                    }
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

            // 2.2 智慧四角號碼模式 (four_corner) 雙區分工架構：
            //     - 右側九宮格小鍵盤 (0x60..=0x6F) 專責四角編碼、`*` 萬用字元、`-` 詞前導、`+` 字詞分隔、`/` 字詞選單、`.` 標點選單。
            //     - 左側主鍵盤區 (A~Z、上方橫排 0~9、主鍵盤符號) 負責正常輸入一般英數字與符號：
            //       * 當無組字緩衝區 (!has_buffer) 時，主鍵盤按鍵直接放行至目標視窗 (Pass-through)，完全無須切換中英模式！
            //       * 當輸入座有組字緩衝區 (has_buffer) 時，主鍵盤按鍵自動將未決四角碼收攏入座並直接追加英數字元，實現中英數無縫混打！
            if is_four_corner_mode && !is_cand_sel && !is_en_inline_or_special {
                let is_main_kb_printable = (0x41..=0x5A).contains(&vk)
                    || (0x30..=0x39).contains(&vk)
                    || matches!(vk, 0xBA..=0xBF | 0xDB..=0xDF);
                if is_main_kb_printable {
                    if !has_buffer {
                        return Ok(BOOL::from(false));
                    }
                    if let Some(ch) = map_vk_to_char(vk, is_shift) {
                        self.delete_selection_if_any();
                        self.consolidate_engine_to_pending();
                        self.insert_pending_char(ch, None);
                        self.sync_input_display(ctx)?;
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
            if vk == VK_SPACE.0 as u32 && has_buffer {
                let (is_jp_mode, has_jp_buf, is_kr_mode, has_kr_buf, has_fc_buf, has_lk_buf, has_pending_fsm, is_latin, has_shape_buf, passthrough_to_engine) = {
                    let engine = self.engine.lock().unwrap();
                    let jp_buf = engine.japanese.has_buffer();
                    let kr_buf = engine.korean.has_buffer();
                    let fc_buf = engine.four_corner.has_buffer();
                    let lk_buf = engine.longkan.has_buffer();
                    let passthrough = engine.inline_longkan_active
                        || engine.absorb_next_space_after_inline_lk
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

            // 3.6 九宮格小數點 (0x6E / VK_DECIMAL) 永遠保持半形「.」（惟智慧四角號碼模式下 `.` 為萬用字元／選字鍵，交由四角引擎處理）
            if vk == 0x6E && has_buffer && !is_four_corner_mode {
                self.consolidate_engine_to_pending();
                self.insert_pending_char('.', None);
                self.sync_input_display(ctx)?;
                return Ok(BOOL::from(true));
            }

            // 3.7 Tab 鍵：
            // - 龍龕形音模式 (is_longkan_mode)：作為詞彙簡碼模式（`🐉⇥詞:`）的引導與結束間隔符號，將組出的字詞收攏至如喜輸入座（`pending_compose`），待按 Enter 才送出至目標視窗
            // - 許氏注音模式：手動斷詞切換 (Manual Word Boundary Split / Cut)
            if vk == VK_TAB.0 as u32 && (has_buffer || is_longkan_mode) {
                if *self.candidate_selecting.lock().unwrap() {
                    *self.candidate_selecting.lock().unwrap() = false;
                    self.hide_candidate_window();
                }

                let is_inline_lk_tab = {
                    let eng = self.engine.lock().unwrap();
                    eng.inline_longkan_active
                        && matches!(
                            eng.inline_target_mode.as_str(),
                            "longkan"
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
                        )
                };
                if is_longkan_mode || is_inline_lk_tab {
                    let (action, committed_chunk, lk_buf_before, still_has_any) = {
                        let mut engine = self.engine.lock().unwrap();
                        let before = engine.longkan.buffer.clone();
                        let resp = engine.process_key('\t');
                        let has_any = engine.longkan.has_buffer()
                            || !self.pending_compose.lock().map(|p| p.is_empty()).unwrap_or(true);
                        (resp.action, resp.committed_chunk, before, has_any)
                    };
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

                self.push_undo();
                self.consolidate_engine_to_pending();
                if let Some((start, end)) = self.get_manual_selection_range() {
                    if end.saturating_sub(start) >= 2 && end.saturating_sub(start) <= 4 {
                        let fused_cands = {
                            let pending_str = self.pending_compose.lock().unwrap().clone();
                            let pending_chars: Vec<char> = pending_str.chars().collect();
                            let pending_syls = self.pending_syllables.lock().unwrap().clone();
                            let engine = self.engine.lock().unwrap();
                            let mut hsu_syls = Vec::new();
                            for idx in start..end {
                                let bpm_opt = pending_syls
                                    .get(idx)
                                    .and_then(|s| s.clone())
                                    .or_else(|| {
                                        pending_chars.get(idx).and_then(|ch| {
                                            let s = ch.to_string();
                                            engine
                                                .lexicon
                                                .word_index
                                                .get(&s)
                                                .and_then(|ids| ids.first())
                                                .and_then(|&eid| engine.lexicon.entries.get(eid))
                                                .map(|e| e.bopomofo.clone())
                                        })
                                    });
                                if let Some(bpm) = bpm_opt {
                                    if let Some(hs) = ruhi::fsm::HsuSyllable::from_bopomofo_display(&bpm) {
                                        hsu_syls.push(hs);
                                    }
                                }
                            }
                            if hsu_syls.len() == end - start {
                                engine.fuse_syllables_with_longkan(&hsu_syls)
                            } else {
                                Vec::new()
                            }
                        };
                        if let Some(first_cand) = fused_cands.first() {
                            let rep_chars: Vec<char> = first_cand.chars().collect();
                            self.replace_pending_range(start, end, &rep_chars, None);
                            *self.sel_anchor.lock().unwrap() = None;
                            *self.sentence_cursor.lock().unwrap() = start + rep_chars.len();
                            self.sync_input_display(ctx)?;
                            return Ok(BOOL::from(true));
                        }
                    }
                }
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
            // 游標所在處單字快速切換：若是排序1直接跳排序2，最多循環 q_cycle_max 次後自動彈出候選座
            let kb_mode = self.config.lock().map(|c| c.keyboard_mode.clone()).unwrap_or_else(|_| "hsu".to_string());
            let is_inline_lk_or_special_mode = {
                let engine = self.engine.lock().unwrap();
                engine.inline_longkan_active
                    || matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    )
            };
            if vk == 0x51 && !is_shift && !is_ctrl && !is_alt && has_buffer && kb_mode == "hsu" && !is_inline_lk_or_special_mode {
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
                    let mut cands = engine.lexicon.lookup_char_candidates_with_bpm(target_char, char_syl.as_deref());
                    if cands.is_empty() && (target_char.is_ascii_alphabetic() || ruhi::western::WesternEngine::strip_char_diacritic(target_char).is_ascii_alphabetic()) {
                        cands = engine.western.lookup_down_arrow_candidates(&target_char.to_string(), Some(target_char));
                    }
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

            // 3.9 智慧四角號碼／龍龕形音 2.0（含通用內嵌 ` 輔助輸入與 `` `` `` / `~~` 選單）即時拆碼預覽翻頁與上下選字 (PgDn / PgUp / ↓ / ↑)
            if vk == VK_NEXT.0 as u32 || vk == VK_PRIOR.0 as u32 || vk == VK_DOWN.0 as u32 || vk == VK_UP.0 as u32
            {
                let handled_live_nav = {
                    let mut engine = self.engine.lock().unwrap();
                    let is_special_menu = matches!(
                        engine.mode,
                        ruhi::engine::EngineMode::QuickNum
                            | ruhi::engine::EngineMode::TemplateSelect
                            | ruhi::engine::EngineMode::LexiconSwitch
                    );
                    if is_special_menu
                        || (engine.inline_longkan_active && engine.has_inline_sub_buffer())
                    {
                        let key_str = match vk {
                            v if v == VK_NEXT.0 as u32 => "PageDown",
                            v if v == VK_PRIOR.0 as u32 => "PageUp",
                            v if v == VK_DOWN.0 as u32 => "Down",
                            _ => "Up",
                        };
                        let _ = engine.process_key_str(key_str);
                        true
                    } else if is_four_corner_mode && engine.four_corner.has_buffer() {
                        let key_str = match vk {
                            v if v == VK_NEXT.0 as u32 => "PageDown",
                            v if v == VK_PRIOR.0 as u32 => "PageUp",
                            v if v == VK_DOWN.0 as u32 => "Down",
                            _ => "Up",
                        };
                        let _ = engine.process_key_str(key_str);
                        true
                    } else if is_longkan_mode
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

                    // 4.5 向下方向鍵 ↓（或四角模式已收攏字詞後按 `/` 字詞選單）：精準開啟游標所在目標詞/字的候選座（含歐文重音變體與同形異體查詢）
                    if vk == VK_DOWN.0 as u32 || fc_slash_as_down {
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

                    // 4.6 向上方向鍵 ↑：雙軌智慧拼寫救援與上下文模糊推理選字
                    // - 歐文軌：縮寫展開（tks ➔ thanks）、漏撇號還原（dont ➔ don't）、子音骨架與拼寫校正（cmptr ➔ computer）、跨軌救援
                    // - 亞洲軌：跨聲調＋近音模糊音＋前後文語法搭配推論（答按 ➔ 答案、可能地答案 ➔ 可能的答案）
                    if vk == VK_UP.0 as u32 {
                        let cur = *self.sentence_cursor.lock().unwrap();
                        let (full_text, _) = self.get_full_display_and_cursor();
                        let (target_idx, target_range, cands) = self.detect_up_arrow_target_and_cands(&full_text, cur);

                        if !cands.is_empty() {
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
                    } else if engine.array30.has_buffer()
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
                        } else {
                            let _ = engine.cangjie.process_key('\x08');
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
            let char_opt = map_vk_to_char(vk, is_shift);
            if let Some(ch) = char_opt {
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
                    let is_lk_key = (is_lk && (engine.longkan.has_buffer() || engine.inline_longkan_active) && (ch.is_ascii_digit() || matches!(ch, '?' | '*' | '`')))
                        || is_special_mode
                        || matches!(ch, '`' | '~');
                    let is_standard_bpm_punct = matches!(engine.keyboard_mode.as_str(), "standard" | "daikien")
                        && matches!(ch, ',' | '.' | ';' | '/' | '-');
                    let punct = !is_ar30_key
                        && !is_fc_key
                        && !is_lk_key
                        && !is_standard_bpm_punct
                        && !is_jp_romaji_key
                        && !is_kr_romaja_key
                        && (engine.symbols.get_char(ch).is_some()
                            || (is_jp && matches!(ch, ',' | '.' | '?' | '!' | '[' | ']' | '(' | ')' | ':' | ';')));
                    let raw_lower = engine.raw_ascii_buffer.to_ascii_lowercase();
                    let is_en_punct_token = engine.enable_mixed_english
                        && engine.mixed_english_mode != "disabled"
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
                        && (!engine.fsm.syllables.is_empty() || engine.fsm.has_pending() || engine.japanese.has_buffer() || engine.korean.has_buffer());
                    if (punct_consolidate || digit_consolidate) && engine.fsm.has_pending() {
                        engine.fsm.commit_current('1');
                        engine.update_sentence_composition();
                    }
                    (punct, digit_consolidate)
                };
                let is_stream_mode = punct_mode == "B" || punct_mode == "C";
                let should_consolidate_before_tilde = ch == '~' && {
                    let eng = self.engine.lock().unwrap();
                    !eng.inline_longkan_active
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
                let resp = engine.process_key(ch);
                let action = resp.action;
                let committed_chunk = resp.committed_chunk;
                let new_kb_mode = engine.keyboard_mode.clone();
                let new_last_mode = engine.last_keyboard_mode.clone();
                let _eng_disp = engine.get_display_text();
                let _eng_cur = engine.cursor_pos;
                drop(engine);
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
                        let is_sentence_terminator = matches!(committed_chunk.as_str(), "。" | "？" | "！" | "；" | "…" | "……" | "?" | "!");
                        let should_keep_in_stream = match punct_mode.as_str() {
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

    fn take_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            pending: self.pending_compose.lock().unwrap().clone(),
            cursor: *self.sentence_cursor.lock().unwrap(),
            anchor: *self.sel_anchor.lock().unwrap(),
            syllables: self.pending_syllables.lock().unwrap().clone(),
            segments: self.pending_segments.lock().unwrap().clone(),
            manual_cuts: self.manual_cuts.lock().unwrap().clone(),
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
            *self.manual_cuts.lock().unwrap() = snap.manual_cuts;
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
            *self.manual_cuts.lock().unwrap() = snap.manual_cuts;
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
        self.manual_cuts.lock().unwrap().clear();
        *self.last_raw_ascii.lock().unwrap() = None;
        *self.sel_anchor.lock().unwrap() = None;
        *self.candidate_selecting.lock().unwrap() = false;
        self.active_candidates.lock().unwrap().clear();
        *self.target_range.lock().unwrap() = (0, 0);
    }

    fn consolidate_engine_to_pending(&self) {
        let engine = self.engine.lock().unwrap();
        let is_kr_chunk = engine.korean.has_buffer();
        let is_fc_chunk = engine.four_corner.has_buffer();
        let is_lk_chunk = engine.longkan.has_buffer();
        let has_composed = !engine.composed_sentence.is_empty() || is_kr_chunk || is_fc_chunk || is_lk_chunk;
        let has_pending = engine.fsm.has_pending();
        if has_composed || has_pending {
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
                    if latest_cfg.capslock_switch == "english_direct" {
                        let cur_norm = RuhiEngine::normalize_keyboard_mode(&eng.keyboard_mode);
                        let target = if cur_norm == "english" {
                            let last_norm = RuhiEngine::normalize_keyboard_mode(&eng.last_keyboard_mode);
                            if !last_norm.is_empty() && last_norm != "english" {
                                last_norm
                            } else {
                                eng.enabled_keyboard_modes
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| "hsu".to_string())
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
                let mut cands = engine.western.lookup_up_arrow_candidates(&sel_text);
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
            let is_western_mode = matches!(
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
                cands.extend(engine.western.lookup_up_arrow_candidates(&word));
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
            cur,
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
                eng.record_committed(&full);
            }
            eng.reset_buffer();
        }
        *self.sentence_cursor.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.candidate_selecting.lock().unwrap_or_else(|e| e.into_inner()) = false;

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

    /// 候選字選取中（上下方向鍵／PageUp／PageDown）更新高亮與即時通用反查教練提示
    fn update_candidate_selection_ui(&self, new_idx: usize, cands: &[String]) {
        let coach_hint = {
            let engine = self.engine.lock().unwrap();
            if (engine.is_reverse_coach_enabled()
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
                if !coach_hint.is_empty() {
                    cw.set_selected_with_coach(new_idx, &coach_hint);
                } else {
                    cw.set_selected(new_idx);
                }
            }
        }
    }

    /// 計算目前輸入狀態下的「許氏注音 ➔ 行列 30 即時反查教練提示」與行列 30 即時簡碼／符號預覽列
    fn compute_coach_hint_and_preview(
        &self,
        full_text: &str,
        cur_pos: usize,
        is_cand_sel: bool,
        cands: &[String],
        sel_idx: usize,
        sel_range: Option<(usize, usize)>,
    ) -> (String, bool, Vec<String>) {
        let engine = self.engine.lock().unwrap();
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
            return (hint, is_live_preview, display_cands);
        }

        // 0.5 通用 `` ` `` 臨時輔助輸入法或龍龕形音 2.0 正在拆碼中：回傳即時候選預覽列與教練提示
        if engine.inline_longkan_active {
            let live_cands = engine.current_candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let hint = if !engine.has_inline_sub_buffer() {
                let (_, badge) = engine.make_inline_preedit_display();
                format!(
                    "{}（輸入單一字／詞後即自動返回原輸入法；再按 ` 開啟使用者自訂文字範本）",
                    badge
                )
            } else {
                engine.get_universal_coach_hint()
            };
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, display_cands);
        }

        if is_lk_mode && engine.longkan.has_buffer() {
            let live_cands = engine.longkan.candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let hint = engine.get_universal_coach_hint();
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, display_cands);
        }

        if matches!(
            engine.mode,
            ruhi::engine::EngineMode::QuickNum
                | ruhi::engine::EngineMode::TemplateSelect
                | ruhi::engine::EngineMode::LexiconSwitch
        ) {
            let live_cands = engine.current_candidates.clone();
            let is_live_preview = !live_cands.is_empty();
            let hint = if engine.mode == ruhi::engine::EngineMode::LexiconSwitch {
                format!("📚 請按數字 1~5、Q 鍵或空白鍵切換專業詞庫（目前：{}）", engine.active_lexicon_profile)
            } else if engine.mode == ruhi::engine::EngineMode::TemplateSelect {
                "📋 使用者自訂文字範本：按數字鍵 1~9/0 直接選取，或按 Q/↓/↑ 切換、空白鍵確認".to_string()
            } else if engine.quick_num_buffer.is_empty() {
                "🔢 數字快打模式：請輸入數字 (0-9 / .) 即時轉中文大小寫與財務金額（Q/↓/↑ 切換格式，空白鍵確認）".to_string()
            } else {
                format!("🔢 數字快打：{}（按 Q/↓/↑ 切換大寫/小寫格式，空白鍵確認）", engine.quick_num_buffer)
            };
            return (hint, is_live_preview, live_cands);
        }

        // 1. 行列 30 正在拆碼中：回傳即時 10 格一二級簡碼／符號／萬用字元預覽列與拆碼提示
        if is_ar30_mode && engine.array30.has_buffer() {
            let live_cands = engine.array30.candidates.clone();
            let is_live_preview = !is_cand_sel && !live_cands.is_empty();
            let hint = engine.get_universal_coach_hint();
            let display_cands = if is_cand_sel { cands.to_vec() } else { live_cands };
            return (hint, is_live_preview, display_cands);
        }

        if !engine.is_reverse_coach_enabled() && !is_ar30_mode && !is_lk_mode {
            return (String::new(), false, cands.to_vec());
        }

        // 2. 若正在開啟候選窗選字，優先顯示當前高亮候選詞／字之目標輸入法反查拆碼
        if is_cand_sel && sel_idx < cands.len() {
            let hint = engine.format_universal_coach_for_text(&cands[sel_idx]);
            return (hint, false, cands.to_vec());
        }

        // 3. 若有 Shift+方向鍵選取範圍，優先顯示選取範圍之目標輸入法反查拆碼
        let chars: Vec<char> = full_text.chars().collect();
        if let Some((start, end)) = sel_range {
            if start < end && end <= chars.len() {
                let slice: String = chars[start..end].iter().collect();
                let hint = engine.format_universal_coach_for_text(&slice);
                if !hint.is_empty() {
                    return (hint, false, cands.to_vec());
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
                return (hint, false, cands.to_vec());
            }
        }

        (engine.get_universal_coach_hint(), false, cands.to_vec())
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
        self.ensure_instance_upgraded();
        let now = std::time::Instant::now();
        let paths = RuhiPaths::resolve();

        let should_check = {
            let lock = LAST_CONFIG_CHECK.lock().unwrap();
            match *lock {
                Some((last_instant, _, _)) => now.duration_since(last_instant).as_millis() > 300,
                None => true,
            }
        };

        if should_check {
            if let Ok(fc_meta) = std::fs::metadata(&paths.four_corner) {
                if let Ok(fc_mtime) = fc_meta.modified() {
                    let needs_fc_reload = {
                        let mut fc_lock = LAST_FC_MTIME.lock().unwrap();
                        let reload = match *fc_lock {
                            Some(prev) => fc_mtime > prev,
                            None => true,
                        };
                        *fc_lock = Some(fc_mtime);
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
            if let Ok(metadata) = std::fs::metadata(&paths.config) {
                if let Ok(mtime) = metadata.modified() {
                    let mut lock = LAST_CONFIG_CHECK.lock().unwrap();
                    let needs_disk_reload = match *lock {
                        Some((_, prev_mtime, _)) => mtime != prev_mtime,
                        None => true,
                    };
                    if needs_disk_reload {
                        let mut new_cfg = load_ruhi_config(&paths);
                        sync_active_tsf_profile_locale(&mut new_cfg);
                        logger::log(&format!(
                            "Reloaded config: mode={}, keyboard={}, font_size={}",
                            new_cfg.input_display_mode,
                            new_cfg.keyboard_mode,
                            new_cfg.font_size
                        ));
                        *lock = Some((now, mtime, new_cfg));
                    } else if let Some(ref mut entry) = *lock {
                        entry.0 = now;
                    }
                }
            }
        }

        if let Ok(lock) = LAST_CONFIG_CHECK.lock() {
            if let Some((_, _, ref cached_cfg)) = *lock {
                let needs_instance_sync = self
                    .config
                    .lock()
                    .map(|c| &*c != cached_cfg)
                    .unwrap_or(false);
                if needs_instance_sync {
                    let new_cfg = cached_cfg.clone();
                    drop(lock);
                    if let Ok(mut eng) = self.engine.lock() {
                        eng.keyboard_mode = new_cfg.keyboard_mode.clone();
                        eng.last_keyboard_mode = new_cfg.last_keyboard_mode.clone();
                        eng.backtick_inline_mode = new_cfg.backtick_inline_mode.clone();
                        eng.tilde_toggle_mode = new_cfg.tilde_toggle_mode.clone();
                        eng.set_enabled_keyboard_modes(new_cfg.enabled_keyboard_modes.clone());
                        eng.enable_mixed_english = new_cfg.enable_mixed_english;
                        eng.mixed_english_mode = new_cfg.mixed_english_mode.clone();
                        eng.mixed_english_min_len = new_cfg.mixed_english_min_len;
                        eng.enable_array30_coach = new_cfg.enable_array30_coach;
                        eng.reverse_coach_target = new_cfg.reverse_coach_target.clone();
                        eng.four_corner.keymap_profile = new_cfg.four_corner_keymap.clone();
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
                "📋文字範本".to_string()
            } else if engine.mode == ruhi::engine::EngineMode::QuickNum {
                if engine.quick_num_buffer.is_empty() {
                    "🔢數字快打".to_string()
                } else {
                    format!("🔢{}", engine.quick_num_buffer)
                }
            } else if engine.mode == ruhi::engine::EngineMode::LexiconSwitch {
                format!("📚詞庫切換（目前：{}）", engine.active_lexicon_profile)
            } else {
                String::new()
            }
        };

        let sel_range = self.get_manual_selection_range();
        let (coach_hint, is_ar30_live_preview, display_cands) =
            self.compute_coach_hint_and_preview(&full_text, cur_pos, is_cand_sel, &cands, sel_idx, sel_range);

        if input_mode == "inline_underline" {
            self.update_tsf_composition(ctx, &full_text, cur_pos, sel_range, false)?;
            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.set_coach_hint(&coach_hint, is_ar30_live_preview);
                    if (is_cand_sel || is_ar30_live_preview) && !display_cands.is_empty() {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        cw.show_candidates_only(&display_cands, sel_idx, cx, cy);
                    } else if let Some(sel_pair) = sel_range {
                        let (cx, cy) = Self::get_caret_screen_pos();
                        let is_caps_on = false;
                        let target_range = *self.target_range.lock().unwrap();
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
            // 每次探測前先清零快取，確保坐標絕對反映當前按鍵時點的最新文字位置
            set_last_caret_pos(0, 0);

            // 1. 優先調用 TSF probe_caret 探測宿主文件中的當前游標 Selection 位置（感知換行與空白行）
            let _ = self.probe_caret(ctx);
            let (mut cx, mut cy) = get_last_caret_pos();

            // 2. 若 TSF 未回傳有效座標，備援嘗試 Win32 GetGUIThreadInfo
            if cx <= 0 || cy <= 0 {
                let (win_cx, win_cy) = Self::get_caret_screen_pos();
                if win_cx > 0 && win_cy > 0 {
                    cx = win_cx;
                    cy = win_cy;
                }
            }

            let is_caps_on = false;

            let target_range = *self.target_range.lock().unwrap();
            let sel_range_pair = sel_range.unwrap_or((0, 0));

            if let Ok(mut cw_guard) = self.candidate_window.lock() {
                if let Some(cw) = cw_guard.as_mut() {
                    cw.set_coach_hint(&coach_hint, is_ar30_live_preview);
                    cw.show_desk(&full_text, cur_pos, &pending_syl, &display_cands, sel_idx, is_cand_sel, target_idx, target_range, sel_range_pair, is_caps_on, cx, cy);
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
        self.hide_candidate_window();
        set_last_caret_pos(0, 0);
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
                        unsafe { let _ = windows::Win32::Graphics::Gdi::ClientToScreen(target, &mut pt); }
                        if pt.x > 0 && pt.y > 0 {
                            logger::log(&format!("get_caret_screen_pos from GetGUIThreadInfo(tid={}): ({}, {})", tid, pt.x, pt.y));
                            return (pt.x, pt.y);
                        }
                    }
                }
            }
        }

        // 2. 備援嘗試 TSF ActiveView 探測之坐標（適用於 Chromium、UWP 等自繪不調用 CreateCaret 之環境）
        let (cx, cy) = get_last_caret_pos();
        if cx > 0 && cy > 0 {
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
            if pdimfocus.is_none() {
                self.hide_candidate_window();
            } else {
                self.check_reload_config();
                notify_langbar_mode_update();
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
            self.ensure_instance_upgraded();
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
        } else if g == crate::guid::GUID_RUHI_SELECTION_DISPLAY_ATTR {
            Ok(crate::display_attribute::RuhiSelectionDisplayAttributeInfo::new().into())
        } else {
            Err(E_INVALIDARG.into())
        }
    }
}
