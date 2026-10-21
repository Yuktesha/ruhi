//! # `config_sync` — 如喜輸入法雙軌設定檔載入與跨目錄同步引擎
//!
//! 完整相容並取代原 `settings_window.py` 之 `load_settings` 與 `save_settings`：
//! 1. 讀寫結構化偏好設定檔 `settings_config.json`（涵蓋 `appearance`、`keyboard`、`grammar`、`ai_and_privacy` 四大區塊）。
//! 2. 自動將核心打字與候選座參數同步寫入 `ruhi-tsf` 讀取的扁平化 `config.json`（涵蓋專案根目錄、`scripts/`、`dist/` 與 `%APPDATA%/Ruhi/`），確保設定變更即時生效，且完整保留既有 AI API 金鑰與自訂欄位。

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// 外觀與候選座設定 (`appearance`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppearanceSettings {
    pub input_display_mode: String,
    pub orientation: String,
    pub font_size: i32,
    #[serde(default = "default_candidate_font_size")]
    pub candidate_font_size: i32,
    #[serde(default = "default_desk_opacity")]
    pub desk_opacity: i32,
    pub max_composition_chars: i32,
    pub theme: String,
    pub show_bopomofo: bool,
    pub capsule_mode: String,
    pub zoom_level: i32,
    pub geometry: String,
    pub sidebar_width: i32,
}

fn default_candidate_font_size() -> i32 {
    24
}

fn default_desk_opacity() -> i32 {
    95
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            input_display_mode: "smart_auto".to_string(),
            orientation: "horizontal".to_string(),
            font_size: 32,
            candidate_font_size: 24,
            desk_opacity: 95,
            max_composition_chars: 30,
            theme: "system".to_string(),
            show_bopomofo: true,
            capsule_mode: "mini_capsule".to_string(),
            zoom_level: 0,
            geometry: "1020x740".to_string(),
            sidebar_width: 260,
        }
    }
}

/// 鍵盤配置、歐文混打與標點結算設定 (`keyboard`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyboardSettings {
    pub layout: String,
    pub enabled_keyboard_modes: Vec<String>,
    pub space_behavior: String,
    pub capslock_switch: String,
    pub candidate_keys: String,
    pub backspace_mode: String,
    pub enable_mixed_english: bool,
    pub mixed_english_mode: String,
    pub mixed_english_min_len: i32,
    pub punctuation_commit_mode: String,
    pub q_cycle_max: i32,
    #[serde(default)]
    pub enable_array30_coach: bool,
    #[serde(default = "default_last_keyboard_mode")]
    pub last_keyboard_mode: String,
}

fn default_last_keyboard_mode() -> String {
    "english".to_string()
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            layout: "hsu".to_string(),
            enabled_keyboard_modes: vec!["hsu".to_string(), "japanese".to_string()],
            space_behavior: "commit_first".to_string(),
            capslock_switch: "capital_lock".to_string(),
            candidate_keys: "123456789".to_string(),
            backspace_mode: "component".to_string(),
            enable_mixed_english: true,
            mixed_english_mode: "contextual_stream".to_string(),
            mixed_english_min_len: 4,
            punctuation_commit_mode: "A".to_string(),
            q_cycle_max: 3,
            enable_array30_coach: false,
            last_keyboard_mode: "english".to_string(),
        }
    }
}

/// 現代漢語八大語法引擎設定 (`grammar`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrammarSettings {
    pub enable_all_eight_phases: bool,
    pub enable_de_vs_de_translingual: bool,
    pub enable_colloquial_biases: bool,
    pub enable_hot_reload: bool,
}

impl Default for GrammarSettings {
    fn default() -> Self {
        Self {
            enable_all_eight_phases: true,
            enable_de_vs_de_translingual: true,
            enable_colloquial_biases: true,
            enable_hot_reload: true,
        }
    }
}

/// 雲端試算表 Apps Script Webhook 網址之內建預設值（刻意留空，不內建任何網址）。
/// 實際網址一律由本機未入庫之設定檔或環境變數 `RUHI_CLOUD_SHEET_WEBHOOK_URL` 提供，
/// 嚴防網址隨原始碼、設定範本或安裝包公開外流。
pub const DEFAULT_CLOUD_SHEET_WEBHOOK_URL: &str = "";

/// 存放 Webhook 網址之環境變數名稱
pub const CLOUD_SHEET_WEBHOOK_ENV: &str = "RUHI_CLOUD_SHEET_WEBHOOK_URL";

/// 解析實際使用之 Webhook 網址：優先採用設定值，其次環境變數；皆無則回傳空字串（呼叫端應略過上傳）
pub fn resolve_cloud_sheet_webhook_url(configured: &str) -> String {
    let c = configured.trim();
    if !c.is_empty() {
        return c.to_string();
    }
    std::env::var(CLOUD_SHEET_WEBHOOK_ENV)
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

/// 官方預設之 Tailcat 語意規則、詞庫與核心版本發布清單網址 (GitHub Raw + ETag CDN)
pub const DEFAULT_TAILCAT_FEED_URL: &str =
    "https://raw.githubusercontent.com/Yuktesha/ruhi/main/ruhi_lexicon/data/tailcat_official_feed.json";

/// 目前執行中之如喜核心引擎版本號
pub const CURRENT_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 目前引擎支援之最高 Tailcat Schema 版本號
pub const CURRENT_TAILCAT_SCHEMA_VERSION: u32 = 1;

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_upload_schedule() -> String {
    "weekly".to_string()
}

fn default_cloud_sheet_webhook_url() -> String {
    DEFAULT_CLOUD_SHEET_WEBHOOK_URL.to_string()
}

fn default_tailcat_update_mode() -> String {
    "notify_confirm".to_string()
}

fn default_tailcat_feed_url() -> String {
    DEFAULT_TAILCAT_FEED_URL.to_string()
}

/// 隱私安全閘門、匿名語料回傳與 Tailcat 雲端更新設定 (`ai_and_privacy`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPrivacySettings {
    pub zero_pii_gateway: bool,
    pub ollama_model: String,
    pub enable_cloud_colab: bool,
    /// 使用者是否同意匿名記錄手動改字整句對照並回傳至官方雲端硬碟資料表（預設關閉 Opt-In，安裝時徵詢，隨時可在設定修改）
    #[serde(default = "default_false")]
    pub telemetry_consent: bool,
    /// 回傳週期設定：`"weekly"` (每週自動回傳) | `"daily"` (每日自動回傳) | `"manual"` (僅手動回傳) | `"disabled"` (關閉)
    #[serde(default = "default_upload_schedule")]
    pub upload_schedule: String,
    /// Google Apps Script Web App (`doPost`) 網址（內建預設官方網址，進階使用者亦可自訂）
    #[serde(default = "default_cloud_sheet_webhook_url")]
    pub cloud_sheet_webhook_url: String,
    /// 上次成功回傳之 Unix 時間戳（秒）
    #[serde(default)]
    pub last_upload_timestamp: u64,
    /// 是否啟用 Tailcat 官方語意規則與詞庫背景自動熱同步 (`ETag` / `HTTP 304`)
    #[serde(default = "default_true")]
    pub tailcat_auto_sync: bool,
    /// Tailcat 更新與版本升級模式：
    /// - `"notify_confirm"` (預設推薦)：詞庫與語意規則自動熱同步；發現新主程式版本時彈出通知，經點擊同意後自動下載安裝
    /// - `"auto_install"`：預先授權同意，詞庫熱更新與新版主程式均自動下載安裝
    /// - `"lexicon_only"` (念舊模式)：僅自動更新向下相容之詞庫與語意規則，永不升級主程式（不支援的新格式自動優雅略過）
    /// - `"manual"`：僅手動點擊檢查更新
    #[serde(default = "default_tailcat_update_mode")]
    pub tailcat_update_mode: String,
    /// 官方 Tailcat 清單網址
    #[serde(default = "default_tailcat_feed_url")]
    pub tailcat_feed_url: String,
    /// 上次執行 Tailcat 同步檢查之 Unix 時間戳（秒）
    #[serde(default)]
    pub last_tailcat_sync_timestamp: u64,
}

impl Default for AiPrivacySettings {
    fn default() -> Self {
        Self {
            zero_pii_gateway: true,
            ollama_model: "gemma4:latest".to_string(),
            enable_cloud_colab: true,
            telemetry_consent: false,
            upload_schedule: default_upload_schedule(),
            cloud_sheet_webhook_url: default_cloud_sheet_webhook_url(),
            last_upload_timestamp: 0,
            tailcat_auto_sync: true,
            tailcat_update_mode: default_tailcat_update_mode(),
            tailcat_feed_url: default_tailcat_feed_url(),
            last_tailcat_sync_timestamp: 0,
        }
    }
}

/// 如喜完整偏好設定結構 (`settings_config.json`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RuhiSettings {
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub keyboard: KeyboardSettings,
    #[serde(default)]
    pub grammar: GrammarSettings,
    #[serde(default)]
    pub ai_and_privacy: AiPrivacySettings,
}

/// 自動解析如喜專案根目錄 (`C:\ATGprjs\ruhi`)
pub fn resolve_ruhi_root() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.to_path_buf());
            if let Some(p2) = parent.parent() {
                candidates.push(p2.to_path_buf());
                if let Some(p3) = p2.parent() {
                    candidates.push(p3.to_path_buf());
                    if let Some(p4) = p3.parent() {
                        candidates.push(p4.to_path_buf());
                    }
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.clone());
        if let Some(p) = cwd.parent() {
            candidates.push(p.to_path_buf());
        }
    }
    candidates.push(PathBuf::from(r"C:\ATGprjs\ruhi"));

    for cand in &candidates {
        if cand.join("ruhi-core").is_dir() && cand.join("ruhi-tsf").is_dir() {
            return cand.clone();
        }
    }
    for cand in &candidates {
        if cand.join("settings_config.json").is_file() || cand.join("config.json").is_file() {
            return cand.clone();
        }
    }
    PathBuf::from(r"C:\ATGprjs\ruhi")
}

/// 解析詞庫資料夾 (`ruhi_lexicon/data` 或 `data`)
pub fn resolve_data_dir(root: &Path) -> PathBuf {
    let cands = [
        root.join("ruhi_lexicon").join("data"),
        root.join("data"),
        root.join("dist").join("data"),
    ];
    for c in &cands {
        if c.is_dir() {
            return c.clone();
        }
    }
    cands[0].clone()
}

/// 若根目錄缺少執行期設定檔，自 `*.example.json` 範本複製一份（設定檔不入庫，首次啟動時建立）
pub fn ensure_runtime_config_files(root: &Path) {
    for (live, example) in [
        ("settings_config.json", "settings_config.example.json"),
        ("config.json", "config.example.json"),
    ] {
        let live_p = root.join(live);
        let ex_p = root.join(example);
        if !live_p.is_file() && ex_p.is_file() {
            let _ = fs::copy(&ex_p, &live_p);
        }
    }
}

/// 從 `settings_config.json` 與 `config.json` 載入並合併最新設定
pub fn load_settings(root: &Path) -> RuhiSettings {
    ensure_runtime_config_files(root);
    let mut settings = RuhiSettings::default();
    let settings_candidates = [
        root.join("settings_config.json"),
        root.join("scripts").join("settings_config.json"),
        root.join("dist").join("settings_config.json"),
    ];

    for path in &settings_candidates {
        if path.is_file() {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                    merge_settings_from_value(&mut settings, &val);
                    break;
                }
            }
        }
    }

    // 若扁平 config.json 有更新的組態（例如由輸入座 UI 直接調校或 TSF 修改），同步對齊至 Studio
    let flat_path = root.join("config.json");
    if flat_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&flat_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) {
                sync_from_flat_map(&mut settings, &map);
            }
        }
    }

    ensure_keyboard_modes_consistent(&mut settings);
    settings
}

fn merge_settings_from_value(settings: &mut RuhiSettings, val: &Value) {
    if let Some(app) = val.get("appearance").and_then(Value::as_object) {
        if let Some(v) = app.get("input_display_mode").and_then(Value::as_str) {
            settings.appearance.input_display_mode = v.to_string();
        }
        if let Some(v) = app.get("orientation").and_then(Value::as_str) {
            settings.appearance.orientation = v.to_string();
        }
        if let Some(v) = app.get("font_size").and_then(Value::as_i64) {
            settings.appearance.font_size = (v as i32).clamp(16, 96);
        }
        if let Some(v) = app.get("candidate_font_size").and_then(Value::as_i64) {
            settings.appearance.candidate_font_size = (v as i32).clamp(14, 72);
        }
        if let Some(v) = app.get("desk_opacity").and_then(Value::as_i64) {
            settings.appearance.desk_opacity = (v as i32).clamp(20, 100);
        }
        if let Some(v) = app.get("max_composition_chars").and_then(Value::as_i64) {
            settings.appearance.max_composition_chars = (v as i32).clamp(8, 80);
        }
        if let Some(v) = app.get("theme").and_then(Value::as_str) {
            settings.appearance.theme = v.to_string();
        }
        if let Some(v) = app.get("show_bopomofo").and_then(Value::as_bool) {
            settings.appearance.show_bopomofo = v;
        }
        if let Some(v) = app.get("capsule_mode").and_then(Value::as_str) {
            settings.appearance.capsule_mode = v.to_string();
        }
        if let Some(v) = app.get("zoom_level").and_then(Value::as_i64) {
            settings.appearance.zoom_level = (v as i32).clamp(-4, 10);
        }
        if let Some(v) = app.get("geometry").and_then(Value::as_str) {
            settings.appearance.geometry = v.to_string();
        }
        if let Some(v) = app.get("sidebar_width").and_then(Value::as_i64) {
            settings.appearance.sidebar_width = (v as i32).clamp(180, 600);
        }
    }

    if let Some(kb) = val.get("keyboard").and_then(Value::as_object) {
        if let Some(v) = kb.get("layout").and_then(Value::as_str) {
            settings.keyboard.layout = v.to_string();
        }
        if let Some(arr) = kb.get("enabled_keyboard_modes").and_then(Value::as_array) {
            let modes: Vec<String> = arr
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect();
            if !modes.is_empty() {
                settings.keyboard.enabled_keyboard_modes = modes;
            }
        }
        if let Some(v) = kb.get("space_behavior").and_then(Value::as_str) {
            settings.keyboard.space_behavior = v.to_string();
        }
        if let Some(v) = kb.get("capslock_switch").and_then(Value::as_str) {
            settings.keyboard.capslock_switch = v.to_string();
        }
        if let Some(v) = kb.get("candidate_keys").and_then(Value::as_str) {
            settings.keyboard.candidate_keys = v.to_string();
        }
        if let Some(v) = kb.get("backspace_mode").and_then(Value::as_str) {
            settings.keyboard.backspace_mode = v.to_string();
        }
        if let Some(v) = kb.get("enable_mixed_english").and_then(Value::as_bool) {
            settings.keyboard.enable_mixed_english = v;
        }
        if let Some(v) = kb.get("mixed_english_mode").and_then(Value::as_str) {
            settings.keyboard.mixed_english_mode = v.to_string();
        }
        if let Some(v) = kb.get("mixed_english_min_len").and_then(Value::as_i64) {
            settings.keyboard.mixed_english_min_len = (v as i32).clamp(3, 6);
        }
        if let Some(v) = kb.get("punctuation_commit_mode").and_then(Value::as_str) {
            settings.keyboard.punctuation_commit_mode = v.to_string();
        }
        if let Some(v) = kb.get("q_cycle_max").and_then(Value::as_i64) {
            settings.keyboard.q_cycle_max = (v as i32).clamp(1, 8);
        }
        if let Some(v) = kb.get("enable_array30_coach").and_then(Value::as_bool) {
            settings.keyboard.enable_array30_coach = v;
        }
        if let Some(v) = kb.get("last_keyboard_mode").and_then(Value::as_str) {
            settings.keyboard.last_keyboard_mode = v.to_string();
        }
    }

    if let Some(gr) = val.get("grammar").and_then(Value::as_object) {
        if let Some(v) = gr.get("enable_all_eight_phases").and_then(Value::as_bool) {
            settings.grammar.enable_all_eight_phases = v;
        }
        if let Some(v) = gr
            .get("enable_de_vs_de_translingual")
            .and_then(Value::as_bool)
        {
            settings.grammar.enable_de_vs_de_translingual = v;
        }
        if let Some(v) = gr.get("enable_colloquial_biases").and_then(Value::as_bool) {
            settings.grammar.enable_colloquial_biases = v;
        }
        if let Some(v) = gr.get("enable_hot_reload").and_then(Value::as_bool) {
            settings.grammar.enable_hot_reload = v;
        }
    }

    if let Some(ai) = val.get("ai_and_privacy").and_then(Value::as_object) {
        if let Some(v) = ai.get("zero_pii_gateway").and_then(Value::as_bool) {
            settings.ai_and_privacy.zero_pii_gateway = v;
        }
        if let Some(v) = ai.get("ollama_model").and_then(Value::as_str) {
            settings.ai_and_privacy.ollama_model = v.to_string();
        }
        if let Some(v) = ai.get("enable_cloud_colab").and_then(Value::as_bool) {
            settings.ai_and_privacy.enable_cloud_colab = v;
        }
        if let Some(v) = ai.get("telemetry_consent").and_then(Value::as_bool) {
            settings.ai_and_privacy.telemetry_consent = v;
        }
        if let Some(v) = ai.get("upload_schedule").and_then(Value::as_str) {
            settings.ai_and_privacy.upload_schedule = v.to_string();
        }
        if let Some(v) = ai.get("cloud_sheet_webhook_url").and_then(Value::as_str) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                settings.ai_and_privacy.cloud_sheet_webhook_url = trimmed.to_string();
            }
        }
        if let Some(v) = ai.get("last_upload_timestamp").and_then(Value::as_u64) {
            settings.ai_and_privacy.last_upload_timestamp = v;
        }
        if let Some(v) = ai.get("tailcat_auto_sync").and_then(Value::as_bool) {
            settings.ai_and_privacy.tailcat_auto_sync = v;
        }
        if let Some(v) = ai.get("tailcat_update_mode").and_then(Value::as_str) {
            if !v.trim().is_empty() {
                settings.ai_and_privacy.tailcat_update_mode = v.trim().to_string();
            }
        }
        if let Some(v) = ai.get("tailcat_feed_url").and_then(Value::as_str) {
            if !v.trim().is_empty() {
                settings.ai_and_privacy.tailcat_feed_url = v.trim().to_string();
            }
        }
        if let Some(v) = ai.get("last_tailcat_sync_timestamp").and_then(Value::as_u64) {
            settings.ai_and_privacy.last_tailcat_sync_timestamp = v;
        }
    }
}

fn sync_from_flat_map(settings: &mut RuhiSettings, map: &Map<String, Value>) {
    if let Some(v) = map.get("input_display_mode").and_then(Value::as_str) {
        settings.appearance.input_display_mode = v.to_string();
    }
    if let Some(v) = map.get("theme").and_then(Value::as_str) {
        settings.appearance.theme = v.to_string();
    }
    if let Some(v) = map.get("font_size").and_then(Value::as_i64) {
        settings.appearance.font_size = (v as i32).clamp(16, 96);
    }
    if let Some(v) = map.get("candidate_font_size").and_then(Value::as_i64) {
        settings.appearance.candidate_font_size = (v as i32).clamp(14, 72);
    }
    if let Some(v) = map.get("desk_opacity").and_then(Value::as_i64) {
        settings.appearance.desk_opacity = (v as i32).clamp(20, 100);
    }
    if let Some(v) = map.get("max_composition_chars").and_then(Value::as_i64) {
        settings.appearance.max_composition_chars = (v as i32).clamp(8, 80);
    }
    if let Some(v) = map.get("keyboard_mode").and_then(Value::as_str) {
        settings.keyboard.layout = v.to_string();
    }
    if let Some(v) = map.get("last_keyboard_mode").and_then(Value::as_str) {
        settings.keyboard.last_keyboard_mode = v.to_string();
    }
    if let Some(v) = map.get("capslock_switch").and_then(Value::as_str) {
        settings.keyboard.capslock_switch = v.to_string();
    }
    if let Some(v) = map.get("punctuation_commit_mode").and_then(Value::as_str) {
        settings.keyboard.punctuation_commit_mode = v.to_string();
    }
    if let Some(v) = map.get("q_cycle_max").and_then(Value::as_i64) {
        settings.keyboard.q_cycle_max = (v as i32).clamp(1, 8);
    }
    if let Some(v) = map.get("enable_array30_coach").and_then(Value::as_bool) {
        settings.keyboard.enable_array30_coach = v;
    }
    if let Some(v) = map.get("telemetry_consent").and_then(Value::as_bool) {
        settings.ai_and_privacy.telemetry_consent = v;
    }
    if let Some(v) = map.get("upload_schedule").and_then(Value::as_str) {
        settings.ai_and_privacy.upload_schedule = v.to_string();
    }
    if let Some(v) = map.get("cloud_sheet_webhook_url").and_then(Value::as_str) {
        if !v.trim().is_empty() {
            settings.ai_and_privacy.cloud_sheet_webhook_url = v.trim().to_string();
        }
    }
    if let Some(v) = map.get("last_upload_timestamp").and_then(Value::as_u64) {
        settings.ai_and_privacy.last_upload_timestamp = v;
    }
    if let Some(v) = map.get("tailcat_auto_sync").and_then(Value::as_bool) {
        settings.ai_and_privacy.tailcat_auto_sync = v;
    }
    if let Some(v) = map.get("tailcat_update_mode").and_then(Value::as_str) {
        if !v.trim().is_empty() {
            settings.ai_and_privacy.tailcat_update_mode = v.trim().to_string();
        }
    }
    if let Some(v) = map.get("tailcat_feed_url").and_then(Value::as_str) {
        if !v.trim().is_empty() {
            settings.ai_and_privacy.tailcat_feed_url = v.trim().to_string();
        }
    }
    if let Some(v) = map.get("last_tailcat_sync_timestamp").and_then(Value::as_u64) {
        settings.ai_and_privacy.last_tailcat_sync_timestamp = v;
    }
}

/// 確保目前預設鍵盤模式一定包含在 `enabled_keyboard_modes` 清單中
pub fn ensure_keyboard_modes_consistent(settings: &mut RuhiSettings) {
    if settings.keyboard.enabled_keyboard_modes.is_empty() {
        settings
            .keyboard
            .enabled_keyboard_modes
            .push(settings.keyboard.layout.clone());
    } else if !settings
        .keyboard
        .enabled_keyboard_modes
        .contains(&settings.keyboard.layout)
    {
        settings
            .keyboard
            .enabled_keyboard_modes
            .insert(0, settings.keyboard.layout.clone());
    }
}

/// 儲存 `settings_config.json` 並同步所有 `config.json` 路徑（供 `ruhi-tsf` 即時讀取）
pub fn save_settings(root: &Path, settings: &mut RuhiSettings) -> Result<(), String> {
    ensure_keyboard_modes_consistent(settings);

    let pretty_settings = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("序列化 settings_config.json 失敗: {e}"))?;

    let settings_targets = [
        root.join("settings_config.json"),
        root.join("scripts").join("settings_config.json"),
        root.join("dist").join("settings_config.json"),
    ];

    for (idx, path) in settings_targets.iter().enumerate() {
        if idx == 0 || path.parent().map(|p| p.is_dir()).unwrap_or(false) {
            if idx == 0 || path.is_file() {
                let _ = fs::write(path, &pretty_settings);
            }
        }
    }

    // 同步更新所有 config.json（保留原有 AI 金鑰與擴充欄位）
    let mut flat_targets = vec![
        root.join("config.json"),
        root.join("scripts").join("config.json"),
        root.join("dist").join("config.json"),
    ];
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_cfg = PathBuf::from(appdata).join("Ruhi").join("config.json");
        if appdata_cfg.is_file() {
            flat_targets.push(appdata_cfg);
        }
    }

    for (idx, flat_path) in flat_targets.iter().enumerate() {
        let should_write = idx == 0 || flat_path.is_file();
        if !should_write {
            continue;
        }
        let mut map = if flat_path.is_file() {
            fs::read_to_string(flat_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Map<String, Value>>(&s).ok())
                .unwrap_or_default()
        } else {
            Map::new()
        };

        map.insert(
            "input_display_mode".to_string(),
            json!(settings.appearance.input_display_mode),
        );
        map.insert(
            "orientation".to_string(),
            json!(settings.appearance.orientation),
        );
        map.insert("theme".to_string(), json!(settings.appearance.theme));
        map.insert(
            "font_size".to_string(),
            json!(settings.appearance.font_size),
        );
        map.insert(
            "candidate_font_size".to_string(),
            json!(settings.appearance.candidate_font_size),
        );
        map.insert(
            "desk_opacity".to_string(),
            json!(settings.appearance.desk_opacity),
        );
        map.insert(
            "max_composition_chars".to_string(),
            json!(settings.appearance.max_composition_chars),
        );
        map.insert(
            "capsule_mode".to_string(),
            json!(settings.appearance.capsule_mode),
        );
        map.insert(
            "keyboard_mode".to_string(),
            json!(settings.keyboard.layout),
        );
        if !map.contains_key("last_keyboard_mode") {
            map.insert(
                "last_keyboard_mode".to_string(),
                json!(settings.keyboard.last_keyboard_mode),
            );
        }
        map.insert(
            "enabled_keyboard_modes".to_string(),
            json!(settings.keyboard.enabled_keyboard_modes),
        );
        map.insert(
            "space_behavior".to_string(),
            json!(settings.keyboard.space_behavior),
        );
        map.insert(
            "capslock_switch".to_string(),
            json!(settings.keyboard.capslock_switch),
        );
        map.insert(
            "punctuation_commit_mode".to_string(),
            json!(settings.keyboard.punctuation_commit_mode),
        );
        map.insert(
            "q_cycle_max".to_string(),
            json!(settings.keyboard.q_cycle_max),
        );
        map.insert(
            "enable_array30_coach".to_string(),
            json!(settings.keyboard.enable_array30_coach),
        );
        map.insert(
            "enable_mixed_english".to_string(),
            json!(settings.keyboard.enable_mixed_english),
        );
        map.insert(
            "mixed_english_mode".to_string(),
            json!(settings.keyboard.mixed_english_mode),
        );
        map.insert(
            "mixed_english_min_len".to_string(),
            json!(settings.keyboard.mixed_english_min_len),
        );
        if !settings.ai_and_privacy.ollama_model.is_empty() {
            map.insert(
                "ollama_model".to_string(),
                json!(settings.ai_and_privacy.ollama_model),
            );
        }
        map.insert(
            "telemetry_consent".to_string(),
            json!(settings.ai_and_privacy.telemetry_consent),
        );
        map.insert(
            "upload_schedule".to_string(),
            json!(settings.ai_and_privacy.upload_schedule),
        );
        map.insert(
            "cloud_sheet_webhook_url".to_string(),
            json!(settings.ai_and_privacy.cloud_sheet_webhook_url),
        );
        map.insert(
            "last_upload_timestamp".to_string(),
            json!(settings.ai_and_privacy.last_upload_timestamp),
        );
        map.insert(
            "tailcat_auto_sync".to_string(),
            json!(settings.ai_and_privacy.tailcat_auto_sync),
        );
        map.insert(
            "tailcat_update_mode".to_string(),
            json!(settings.ai_and_privacy.tailcat_update_mode),
        );
        map.insert(
            "tailcat_feed_url".to_string(),
            json!(settings.ai_and_privacy.tailcat_feed_url),
        );
        map.insert(
            "last_tailcat_sync_timestamp".to_string(),
            json!(settings.ai_and_privacy.last_tailcat_sync_timestamp),
        );

        if let Ok(s) = serde_json::to_string_pretty(&Value::Object(map)) {
            let _ = fs::write(flat_path, s);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings_and_mode_consistency() {
        let mut s = RuhiSettings::default();
        s.keyboard.layout = "cangjie".to_string();
        ensure_keyboard_modes_consistent(&mut s);
        assert_eq!(s.keyboard.enabled_keyboard_modes[0], "cangjie");
        assert_eq!(s.appearance.font_size, 32);
        assert_eq!(s.keyboard.punctuation_commit_mode, "A");
    }

    /// 防外流：內建預設 Webhook 必須為空，且所有入庫之設定範本與原始碼不得含 Apps Script 部署網址
    #[test]
    fn test_no_cloud_webhook_url_leak_in_defaults_and_templates() {
        assert!(DEFAULT_CLOUD_SHEET_WEBHOOK_URL.is_empty());
        assert!(AiPrivacySettings::default().cloud_sheet_webhook_url.is_empty());
        assert_eq!(resolve_cloud_sheet_webhook_url("  https://example.test/x  "), "https://example.test/x");

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let needle = ["script.google.com", "/macros/s/"].concat();
        for rel in [
            "config.example.json",
            "settings_config.example.json",
            "ruhi-studio/src/config_sync.rs",
            "ruhi-studio/src/tuner.rs",
            "ruhi-studio/src/main.rs",
        ] {
            let p = root.join(rel);
            if let Ok(txt) = fs::read_to_string(&p) {
                assert!(!txt.contains(&needle), "{rel} 含有 Apps Script 部署網址，禁止入庫");
            }
        }
    }
}
