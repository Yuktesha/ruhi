//! 智慧型應用程式設定持久化、參數組合 (Profiles) 與視窗幾何管理器 (`config_store.rs`)
//!
//! 對應 Python 版 `_lib/UniversalUI.py` 之 `AppConfig` 與 `_lib/dialog_engine.py` 之 `reset_all_dialog_geometries`。
//!
//! ### 核心功能
//! 1. **雙軌路徑相容**：自動支援 `_cfg/<app_name>_cfg.json` 與舊版 `<app_name>_cfg.json`。
//! 2. **簽章驗證與智慧遷移 (`config_signature`)**：防止不同應用程式誤讀設定，並可掃描同目錄下相容簽章之設定檔。
//! 3. **視窗與對話框幾何位置持久化**：記憶主視窗與各子對話框 (`window_<state_id>`) 之座標與尺寸，
//!    並提供 [`AppConfig::reset_all_dialog_geometries`] 一鍵重設所有彈出視窗回歸主視窗置中對齊。
//! 4. **個人化參數組合 (`Profiles`)**：支援將當前設定另存為具名組合 (`_cfg/profiles/<name>.json`)、載入與列出組合。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 視窗或對話框之持久化解算幾何紀錄
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl WindowGeometry {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            x: None,
            y: None,
            width,
            height,
            maximized: false,
        }
    }

    /// 計算子對話框相對於母視窗之預設動態置中座標 (`rootx + (pw - w) / 2`, `rooty + (ph - h) / 2`)。
    pub fn centered_in_parent(&self, parent: &WindowGeometry) -> (f32, f32) {
        let px = parent.x.unwrap_or(100.0);
        let py = parent.y.unwrap_or(100.0);
        let cx = px + ((parent.width - self.width) * 0.5).max(0.0);
        let cy = py + ((parent.height - self.height) * 0.5).max(0.0);
        (cx.max(10.0), cy.max(10.0))
    }
}

/// 智慧型應用程式設定儲存庫
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_name: String,
    pub signature: String,
    pub base_dir: PathBuf,
    pub cfg_dir: PathBuf,
    pub file_path: PathBuf,
    defaults: Map<String, Value>,
    pub data: Map<String, Value>,
}

impl AppConfig {
    /// 以應用程式名稱、唯一簽章與預設鍵值建立設定儲存庫（預設以當前工作目錄或執行檔目錄為基準）。
    pub fn new(app_name: &str, signature: &str, defaults: Option<Map<String, Value>>) -> Self {
        let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::with_base_dir(app_name, signature, base_dir, defaults)
    }

    /// 指定基準目錄建立設定儲存庫（便於自動化測試與自訂路徑）。
    pub fn with_base_dir(
        app_name: &str,
        signature: &str,
        base_dir: impl AsRef<Path>,
        defaults: Option<Map<String, Value>>,
    ) -> Self {
        let base_dir = base_dir.as_ref().to_path_buf();
        let legacy_path = base_dir.join(format!("{}_cfg.json", app_name));
        let cfg_dir = base_dir.join("_cfg");
        let new_path = cfg_dir.join(format!("{}_cfg.json", app_name));

        let file_path = if legacy_path.exists() {
            legacy_path
        } else {
            new_path
        };

        let mut def_map = defaults.unwrap_or_default();
        def_map.insert(
            "config_signature".to_string(),
            Value::String(signature.to_string()),
        );

        let mut cfg = Self {
            app_name: app_name.to_string(),
            signature: signature.to_string(),
            base_dir,
            cfg_dir,
            file_path,
            defaults: def_map.clone(),
            data: def_map,
        };
        cfg.load();
        cfg
    }

    /// 從磁碟載入設定；若目標檔案不存在但同目錄有相同 `config_signature` 的舊版檔案，自動採納最新一份。
    pub fn load(&mut self) {
        if self.file_path.exists() {
            self.data = self.read_merged_file(&self.file_path);
            return;
        }
        let candidates = self.scan_compatible_configs();
        if let Some(best) = candidates.first() {
            self.data = self.read_merged_file(best);
            let _ = self.save();
        } else {
            self.data = self.defaults.clone();
        }
    }

    fn read_merged_file(&self, path: &Path) -> Map<String, Value> {
        let mut merged = self.defaults.clone();
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Ok(Value::Object(saved_map)) = serde_json::from_str::<Value>(&raw) {
                for (k, v) in saved_map {
                    merged.insert(k, v);
                }
            }
        }
        merged.insert(
            "config_signature".to_string(),
            Value::String(self.signature.clone()),
        );
        merged
    }

    /// 掃描基準目錄與 `_cfg` 目錄下所有具備相同 `config_signature` 之 `*_cfg.json` 候選設定檔（依修改時間由新到舊排序）。
    pub fn scan_compatible_configs(&self) -> Vec<PathBuf> {
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for dir in [&self.base_dir, &self.cfg_dir] {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p == self.file_path || !p.is_file() {
                    continue;
                }
                let Some(fname) = p.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !fname.ends_with("_cfg.json") {
                    continue;
                }
                if let Ok(raw) = std::fs::read_to_string(&p) {
                    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) {
                        if map.get("config_signature").and_then(|v| v.as_str())
                            == Some(&self.signature)
                        {
                            let mtime = entry
                                .metadata()
                                .and_then(|m| m.modified())
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            found.push((mtime, p));
                        }
                    }
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found.into_iter().map(|(_, p)| p).collect()
    }

    /// 將當前設定寫入磁碟 JSON 檔案。
    pub fn save(&mut self) -> std::io::Result<()> {
        self.data.insert(
            "config_signature".to_string(),
            Value::String(self.signature.clone()),
        );
        if let Some(parent) = self.file_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json_str = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.file_path, json_str)
    }

    /// 讀取強型別設定值。
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.data
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 讀取強型別設定值，若不存在或型別不符則回傳 `default`。
    pub fn get_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(default)
    }

    /// 讀取字串設定值，若不存在則回傳 `default`。
    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.get::<String>(key)
            .unwrap_or_else(|| default.to_string())
    }

    /// 讀取布林設定值，若不存在則回傳 `default`。
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get_or(key, default)
    }

    /// 讀取 `f64` 浮點數設定值，若不存在則回傳 `default`。
    pub fn get_f64(&self, key: &str, default: f64) -> f64 {
        self.get_or(key, default)
    }

    /// 讀取 `i64` 整數設定值，若不存在則回傳 `default`。
    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        self.get_or(key, default)
    }

    /// 讀取原始 `serde_json::Value`。
    pub fn get_raw(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// 設定鍵值（接受任何可序列化為 JSON 之型別）。
    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(val) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), val);
        }
    }

    /// 刪除指定設定鍵。
    pub fn delete(&mut self, key: &str) -> Option<Value> {
        self.data.remove(key)
    }

    /// 恢復原廠預設值並立即存檔。
    pub fn restore_factory_defaults(&mut self) -> std::io::Result<()> {
        self.data = self.defaults.clone();
        self.save()
    }

    /// 讀取指定視窗或對話框 (`window_<state_id>`) 之持久化幾何設定。
    pub fn get_window_geometry(&self, state_id: &str) -> Option<WindowGeometry> {
        let key = format!("window_{}", state_id);
        self.get(&key)
    }

    /// 儲存指定視窗或對話框 (`window_<state_id>`) 之幾何設定。
    pub fn set_window_geometry(&mut self, state_id: &str, geo: WindowGeometry) {
        let key = format!("window_{}", state_id);
        self.set(&key, geo);
    }

    /// 一鍵清除所有子對話框之自訂位置紀錄 (`window_*`，保留主視窗 `window_main` 與 `window_geometry`)，
    /// 使所有彈出對話框恢復初始預設之主視窗中央置中對齊。回傳清除之項目數量。
    pub fn reset_all_dialog_geometries(&mut self) -> usize {
        let keys_to_del: Vec<String> = self
            .data
            .keys()
            .filter(|k| {
                k.starts_with("window_")
                    && k.as_str() != "window_main"
                    && k.as_str() != "window_geometry"
            })
            .cloned()
            .collect();

        let count = keys_to_del.len();
        for k in keys_to_del {
            self.data.remove(&k);
        }
        let _ = self.save();
        count
    }

    /// 取得個人化參數組合 (`_cfg/profiles`) 目錄路徑。
    pub fn profiles_dir(&self) -> PathBuf {
        self.cfg_dir.join("profiles")
    }

    /// 儲存目前設定為具名參數組合 (`<profile_name>.json`)。
    pub fn save_profile(&self, profile_name: &str) -> std::io::Result<()> {
        let p_dir = self.profiles_dir();
        if !p_dir.exists() {
            std::fs::create_dir_all(&p_dir)?;
        }
        let safe_name = profile_name.trim();
        let path = p_dir.join(format!("{}.json", safe_name));
        let json_str = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(path, json_str)
    }

    /// 載入具名參數組合並套用至當前設定。
    pub fn load_profile(&mut self, profile_name: &str) -> std::io::Result<bool> {
        let path = self.profiles_dir().join(format!("{}.json", profile_name.trim()));
        if !path.exists() {
            return Ok(false);
        }
        self.data = self.read_merged_file(&path);
        self.save()?;
        Ok(true)
    }

    /// 列出所有已儲存之個人化參數組合名稱（按字母排序）。
    pub fn list_profiles(&self) -> Vec<String> {
        let p_dir = self.profiles_dir();
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(p_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        names.sort();
        names
    }
}

/// 獨立函式介面：一鍵清除 `AppConfig` 中所有已持久化之對話框位置紀錄。
pub fn reset_all_dialog_geometries(config: &mut AppConfig) -> usize {
    config.reset_all_dialog_geometries()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_save_load_and_profiles() {
        let temp_dir = std::env::temp_dir().join("atg_lib_test_config");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let mut defaults = Map::new();
        defaults.insert("ui_scale".to_string(), serde_json::json!(1.0));
        defaults.insert("language".to_string(), serde_json::json!("zh_TW"));

        let mut cfg = AppConfig::with_base_dir("test_app", "sig_v1", &temp_dir, Some(defaults));
        assert_eq!(cfg.get_or("ui_scale", 0.0_f32), 1.0);

        cfg.set("ui_scale", 1.25_f32);
        cfg.set_window_geometry(
            "main",
            WindowGeometry {
                x: Some(50.0),
                y: Some(60.0),
                width: 1200.0,
                height: 800.0,
                maximized: false,
            },
        );
        cfg.set_window_geometry("prefs_dlg", WindowGeometry::new(500.0, 400.0));
        cfg.set_window_geometry("eq_dlg", WindowGeometry::new(600.0, 380.0));
        cfg.save().unwrap();

        // 測試儲存與讀取 Profile
        cfg.save_profile("Podcast_Mode").unwrap();
        assert_eq!(cfg.list_profiles(), vec!["Podcast_Mode".to_string()]);

        // 測試一鍵重設子對話框幾何位置（應清除 prefs_dlg 與 eq_dlg，但保留 window_main）
        let cleared = cfg.reset_all_dialog_geometries();
        assert_eq!(cleared, 2);
        assert!(cfg.get_window_geometry("main").is_some());
        assert!(cfg.get_window_geometry("prefs_dlg").is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
