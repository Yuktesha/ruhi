//! 試算表驅動多語言國際化核心引擎 (`i18n.rs`)
//!
//! 對應 Python 版 `_lib/i18n.py`，遵循 ATG 第二核心哲學：
//! 「在地化語意純粹與尊嚴定律 (L10n Semantic Precision & Linguistic Sovereignty Law)」。
//!
//! ### 核心特色
//! 1. **內建基礎詞庫 + 外部試算表疊加**：
//!    編譯期內嵌標準 UI 詞彙表 (`assets/lang/languages.tsv`)，開箱即用；
//!    執行期可透過 [`I18nEngine::load_from_dir`] 或 [`I18nEngine::load_from_file`] 載入應用程式專屬 `_lang/languages.tsv`。
//! 2. **極簡試算表擴充性**：
//!    支援 Tab 分隔 (`.tsv`) 與逗號分隔 (`.csv`) 矩陣格式，以及 `#` 開頭的分類註解行。
//!    在試算表最右側新增一欄（例如 `ko_KR (한국어)`），引擎即自動偵測並納入可用語言清單。
//! 3. **動態字串插值與多層回退**：
//!    支援 `{name}` 具名參數插值（[`I18nEngine::tf`]），查詢順序為：
//!    `當前語系` ➔ `回退語系 (fallback_lang)` ➔ `預設語系 (default_lang)` ➔ `呼叫端預設字串 / Key 本身`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

const BUILTIN_TSV: &str = include_str!("../assets/lang/languages.tsv");

/// 單一語系之代碼與顯示名稱資訊
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    /// 語系代碼，例如 `"zh_TW"`、`"en_US"`、`"zh_CN"`、`"ja_JP"`
    pub code: String,
    /// 選單完整顯示名稱，例如 `"zh_TW (繁體中文)"`
    pub display_name: String,
}

/// 通用試算表多語言管理器
#[derive(Debug, Clone)]
pub struct I18nEngine {
    default_lang: String,
    fallback_lang: String,
    current_lang: String,
    lang_dir: Option<PathBuf>,
    languages: Vec<LanguageInfo>,
    /// 儲存結構：`key -> (lang_code -> translated_text)`
    table: HashMap<String, HashMap<String, String>>,
}

impl Default for I18nEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl I18nEngine {
    /// 建立預設多語言引擎（載入內建基礎詞庫，預設語系為 `zh_TW`）。
    pub fn new() -> Self {
        Self::with_defaults("zh_TW", "zh_TW")
    }

    /// 指定預設語系與回退語系建立引擎。
    pub fn with_defaults(default_lang: &str, fallback_lang: &str) -> Self {
        let mut engine = Self {
            default_lang: default_lang.to_string(),
            fallback_lang: fallback_lang.to_string(),
            current_lang: default_lang.to_string(),
            lang_dir: None,
            languages: Vec::new(),
            table: HashMap::new(),
        };
        engine.load_from_tsv_str(BUILTIN_TSV);
        engine
    }

    /// 指定應用程式的 `_lang` 目錄並載入其中的 `languages.tsv`、`languages.csv` 或個別語系檔。
    pub fn load_from_dir(&mut self, dir: impl AsRef<Path>) -> std::io::Result<()> {
        let dir_path = dir.as_ref().to_path_buf();
        self.lang_dir = Some(dir_path.clone());
        self.reload_languages()
    }

    /// 重新掃描並載入內建詞庫與 `lang_dir` 目錄下的語言試算表。
    pub fn reload_languages(&mut self) -> std::io::Result<()> {
        self.table.clear();
        self.languages.clear();
        self.load_from_tsv_str(BUILTIN_TSV);

        let Some(ref dir) = self.lang_dir.clone() else {
            return Ok(());
        };
        if !dir.exists() {
            return Ok(());
        }

        let tsv_path = dir.join("languages.tsv");
        let csv_path = dir.join("languages.csv");

        if tsv_path.exists() {
            self.load_from_file(&tsv_path)?;
        } else if csv_path.exists() {
            let content = std::fs::read_to_string(&csv_path)?;
            let clean = content.trim_start_matches('\u{feff}');
            self.load_from_delimited_str(clean, ',');
        } else {
            // 載入個別語系檔案 (例如 zh_TW.tsv, en_US.tsv)
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                let ext_lower = ext.to_ascii_lowercase();
                if ext_lower != "tsv" && ext_lower != "csv" {
                    continue;
                }
                let Some(code) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let delim = if ext_lower == "tsv" { '\t' } else { ',' };
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let clean = content.trim_start_matches('\u{feff}');
                    self.load_single_lang_str(code, clean, delim);
                }
            }
        }
        Ok(())
    }

    /// 從外部 `.tsv` 檔案載入或覆寫翻譯詞條（自動處理 UTF-8 BOM）。
    pub fn load_from_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let clean = content.trim_start_matches('\u{feff}');
        self.load_from_tsv_str(clean);
        Ok(())
    }

    /// 解析 Tab 分隔之試算表字串 (`KEY\tzh_TW (...)\ten_US (...)`)。
    pub fn load_from_tsv_str(&mut self, content: &str) {
        let clean = content.trim_start_matches('\u{feff}');
        self.load_from_delimited_str(clean, '\t');
    }

    /// 解析指定分隔符之多欄式語言矩陣試算表字串。
    pub fn load_from_delimited_str(&mut self, content: &str, delimiter: char) {
        let mut col_codes: Vec<String> = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim_end_matches(['\r', '\n']);
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }

            let cols = split_delimited_row(line, delimiter);
            if cols.is_empty() {
                continue;
            }

            if col_codes.is_empty() && cols[0].trim().eq_ignore_ascii_case("KEY") {
                for header_cell in &cols[1..] {
                    let h = header_cell.trim();
                    if h.is_empty() {
                        continue;
                    }
                    let code = parse_header_lang_code(h);
                    col_codes.push(code.clone());
                    if let Some(existing) = self.languages.iter_mut().find(|l| l.code == code) {
                        existing.display_name = h.to_string();
                    } else {
                        self.languages.push(LanguageInfo {
                            code,
                            display_name: h.to_string(),
                        });
                    }
                }
                continue;
            }

            if col_codes.is_empty() {
                continue;
            }

            let key = cols[0].trim();
            if key.is_empty() {
                continue;
            }

            let entry = self.table.entry(key.to_string()).or_default();
            for (idx, lang_code) in col_codes.iter().enumerate() {
                if let Some(val) = cols.get(idx + 1) {
                    let v = val.trim().replace("\\n", "\n").replace("\\t", "\t");
                    if !v.is_empty() {
                        entry.insert(lang_code.clone(), v);
                    }
                }
            }
        }
    }

    fn load_single_lang_str(&mut self, lang_code: &str, content: &str, delimiter: char) {
        if !self.languages.iter().any(|l| l.code == lang_code) {
            self.languages.push(LanguageInfo {
                code: lang_code.to_string(),
                display_name: lang_code.to_string(),
            });
        }
        for raw_line in content.lines() {
            let line = raw_line.trim_end_matches(['\r', '\n']);
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let cols = split_delimited_row(line, delimiter);
            if cols.len() < 2 {
                continue;
            }
            let k = cols[0].trim();
            if k.is_empty() || k.eq_ignore_ascii_case("KEY") {
                continue;
            }
            let v = cols[1].trim().replace("\\n", "\n").replace("\\t", "\t");
            if !v.is_empty() {
                self.table
                    .entry(k.to_string())
                    .or_default()
                    .insert(lang_code.to_string(), v);
            }
        }
    }

    /// 偵測作業系統環境語系並對應至已載入之語言代碼。
    pub fn detect_system_language(&self) -> String {
        for env_key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(env_key) {
                if let Some(mapped) = self.map_locale_str(&val) {
                    return mapped;
                }
            }
        }
        self.default_lang.clone()
    }

    fn map_locale_str(&self, loc: &str) -> Option<String> {
        let loc_l = loc.to_ascii_lowercase();
        for info in &self.languages {
            if loc_l.starts_with(&info.code.to_ascii_lowercase()) {
                return Some(info.code.clone());
            }
        }
        if loc_l.contains("zh_tw")
            || loc_l.contains("zh-tw")
            || loc_l.contains("zh_hk")
            || loc_l.contains("hant")
        {
            return Some("zh_TW".to_string());
        }
        if loc_l.contains("zh_cn") || loc_l.contains("zh-cn") || loc_l.contains("hans") {
            return Some("zh_CN".to_string());
        }
        if loc_l.contains("ja") {
            return Some("ja_JP".to_string());
        }
        if loc_l.contains("en") {
            return Some("en_US".to_string());
        }
        None
    }

    /// 取得所有已載入之語言清單。
    pub fn available_languages(&self) -> &[LanguageInfo] {
        &self.languages
    }

    /// 取得目前啟用之語言代碼（如 `"zh_TW"`）。
    pub fn active_language(&self) -> &str {
        &self.current_lang
    }

    /// 取得目前啟用之語言代碼（相容 Python `get_language()` 命名）。
    pub fn get_language(&self) -> &str {
        &self.current_lang
    }

    /// 切換當前語言（支援傳入 `"auto"`、`"zh_TW"` 或完整標籤 `"zh_TW (繁體中文)"`）。
    pub fn set_language(&mut self, lang_code: &str) {
        if lang_code.eq_ignore_ascii_case("auto") {
            self.current_lang = self.detect_system_language();
            return;
        }
        let clean = parse_header_lang_code(lang_code);
        if self.languages.iter().any(|l| l.code == clean) {
            self.current_lang = clean;
        } else {
            self.current_lang = self.default_lang.clone();
        }
    }

    /// 零配置快速查詢翻譯字串；若找不到則依序回退 `fallback_lang`、`default_lang`、`default_fallback` 或 `key`。
    pub fn t<'a>(&'a self, key: &'a str, default_fallback: &'a str) -> &'a str {
        if let Some(lang_map) = self.table.get(key) {
            if let Some(val) = lang_map.get(&self.current_lang) {
                return val.as_str();
            }
            if self.current_lang != self.fallback_lang {
                if let Some(val) = lang_map.get(&self.fallback_lang) {
                    return val.as_str();
                }
            }
            if self.fallback_lang != self.default_lang {
                if let Some(val) = lang_map.get(&self.default_lang) {
                    return val.as_str();
                }
            }
            if let Some(val) = lang_map.get("zh_TW") {
                return val.as_str();
            }
            if let Some(val) = lang_map.get("en_US") {
                return val.as_str();
            }
        }
        if !default_fallback.is_empty() {
            default_fallback
        } else {
            key
        }
    }

    /// 支援 `{placeholder}` 具名參數替換之翻譯函式（對應 Python `i18n.t("key", col="標題")`）。
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut out = self.t(key, "").to_string();
        for (k, v) in args {
            let token = format!("{{{}}}", k);
            out = out.replace(&token, v);
        }
        out
    }
}

fn parse_header_lang_code(header_cell: &str) -> String {
    let trimmed = header_cell.trim();
    if let Some(idx) = trimmed.find(['(', ' ']) {
        trimmed[..idx].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn split_delimited_row(line: &str, delimiter: char) -> Vec<String> {
    if delimiter == '\t' {
        return line.split('\t').map(|s| s.to_string()).collect();
    }
    // 簡易 CSV 引號感知切分
    let mut result = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                cur.push('"');
                let _ = chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if c == delimiter && !in_quotes {
            result.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    result.push(cur);
    result
}

// ============================================================================
// 全域單例便捷介面 (Module-Level Global Singleton API)
// ============================================================================

static GLOBAL_I18N: OnceLock<RwLock<I18nEngine>> = OnceLock::new();

fn global_lock() -> &'static RwLock<I18nEngine> {
    GLOBAL_I18N.get_or_init(|| RwLock::new(I18nEngine::new()))
}

/// 初始化全域多語言引擎並載入指定 `_lang` 目錄。
pub fn init(lang_dir: Option<&Path>, default_lang: &str) {
    if let Ok(mut guard) = global_lock().write() {
        *guard = I18nEngine::with_defaults(default_lang, default_lang);
        if let Some(dir) = lang_dir {
            let _ = guard.load_from_dir(dir);
        }
    }
}

/// 切換全域語言代碼。
pub fn set_language(lang_code: &str) {
    if let Ok(mut guard) = global_lock().write() {
        guard.set_language(lang_code);
    }
}

/// 取得全域當前語言代碼。
pub fn get_language() -> String {
    global_lock()
        .read()
        .map(|g| g.get_language().to_string())
        .unwrap_or_else(|_| "zh_TW".to_string())
}

/// 從全域引擎取得翻譯字串。
pub fn tr(key: &str) -> String {
    global_lock()
        .read()
        .map(|g| g.t(key, "").to_string())
        .unwrap_or_else(|_| key.to_string())
}

/// 從全域引擎取得帶參數插值之翻譯字串。
pub fn tr_args(key: &str, args: &[(&str, &str)]) -> String {
    global_lock()
        .read()
        .map(|g| g.tf(key, args))
        .unwrap_or_else(|_| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_tsv_and_language_switching() {
        let mut i18n = I18nEngine::new();
        assert!(i18n.available_languages().len() >= 4);
        assert_eq!(i18n.t("menu_file", ""), "檔案 (F)");

        i18n.set_language("en_US");
        assert_eq!(i18n.t("menu_file", ""), "File (F)");

        i18n.set_language("zh_CN");
        assert_eq!(i18n.t("menu_file", ""), "文件 (F)");

        i18n.set_language("ja_JP");
        assert_eq!(i18n.t("menu_file", ""), "ファイル (F)");
    }

    #[test]
    fn test_formatted_interpolation_and_fallback() {
        let mut i18n = I18nEngine::new();
        i18n.set_language("zh_TW");
        let msg = i18n.tf("tree_menu_fit_col", &[("col", "曲目標題")]);
        assert_eq!(msg, "調整「曲目標題」至最適欄寬");

        assert_eq!(i18n.t("non_existent_key", "預設文字"), "預設文字");
        assert_eq!(i18n.t("non_existent_key", ""), "non_existent_key");
    }

    #[test]
    fn test_custom_tsv_overlay_adds_new_language() {
        let mut i18n = I18nEngine::new();
        let custom_tsv = "\
KEY\tzh_TW (繁體中文)\tko_KR (한국어)
custom_greeting\t您好，{user}！\t안녕하세요, {user}!
";
        i18n.load_from_tsv_str(custom_tsv);
        assert!(i18n.available_languages().iter().any(|l| l.code == "ko_KR"));

        i18n.set_language("ko_KR (한국어)");
        assert_eq!(i18n.active_language(), "ko_KR");
        assert_eq!(
            i18n.tf("custom_greeting", &[("user", "ATG")]),
            "안녕하세요, ATG!"
        );
    }
}
