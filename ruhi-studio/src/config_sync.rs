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
    pub max_composition_chars: i32,
    pub theme: String,
    pub show_bopomofo: bool,
    pub capsule_mode: String,
    pub zoom_level: i32,
    pub geometry: String,
    pub sidebar_width: i32,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            input_display_mode: "smart_auto".to_string(),
            orientation: "horizontal".to_string(),
            font_size: 32,
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

/// 隱私安全閘門與 AI 模型設定 (`ai_and_privacy`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPrivacySettings {
    pub zero_pii_gateway: bool,
    pub ollama_model: String,
    pub enable_cloud_colab: bool,
}

impl Default for AiPrivacySettings {
    fn default() -> Self {
        Self {
            zero_pii_gateway: true,
            ollama_model: "gemma4:latest".to_string(),
            enable_cloud_colab: true,
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

/// 從 `settings_config.json` 與 `config.json` 載入並合併最新設定
pub fn load_settings(root: &Path) -> RuhiSettings {
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

    // 若扁平 config.json 有更新的組態（例如由舊版工具或 TSF 修改），亦做溫和對齊
    let flat_path = root.join("config.json");
    if flat_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&flat_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) {
                if !settings_candidates.iter().any(|p| p.is_file()) {
                    sync_from_flat_map(&mut settings, &map);
                }
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
            settings.appearance.font_size = (v as i32).clamp(18, 54);
        }
        if let Some(v) = app.get("max_composition_chars").and_then(Value::as_i64) {
            settings.appearance.max_composition_chars = (v as i32).clamp(15, 60);
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
        settings.appearance.font_size = (v as i32).clamp(18, 54);
    }
    if let Some(v) = map.get("max_composition_chars").and_then(Value::as_i64) {
        settings.appearance.max_composition_chars = (v as i32).clamp(15, 60);
    }
    if let Some(v) = map.get("keyboard_mode").and_then(Value::as_str) {
        settings.keyboard.layout = v.to_string();
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
}
