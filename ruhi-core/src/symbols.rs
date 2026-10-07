/// RuhI (如喜) Input Method Framework - Rust Core
/// src/symbols.rs - 自訂符號對照表與熱置換管理器 (Hot-swappable Symbols Manager)

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn normalize_key(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() == 1 {
        return trimmed.to_string();
    }
    let s_clean: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    let lower = s_clean.to_lowercase();
    if lower.starts_with("ctrl+") {
        let rest = &s_clean[5..];
        format!("Ctrl+{}", rest)
    } else if lower.starts_with("alt+") {
        let rest = &s_clean[4..];
        format!("Alt+{}", rest)
    } else if lower.starts_with("shift+") {
        let rest = &s_clean[6..];
        format!("Shift+{}", rest)
    } else {
        s_clean
    }
}

#[derive(Debug, Clone)]
pub struct SymbolsManager {
    pub map: HashMap<String, String>,
    file_path: Option<PathBuf>,
    last_modified: Option<SystemTime>,
}

impl Default for SymbolsManager {
    fn default() -> Self {
        let mut mgr = Self {
            map: HashMap::new(),
            file_path: None,
            last_modified: None,
        };
        mgr.init_defaults();
        mgr
    }
}

impl SymbolsManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 預設經典許氏、自然輸入法與震漢國音標點符號對照
    fn init_defaults(&mut self) {
        let defaults: &[(&str, &str)] = &[
            ("Ctrl+.", "……"), // Ctrl + . -> …… (自然輸入法標準刪節號)
            ("Ctrl+,", "，"), // Ctrl + , -> ，
            ("Ctrl+-", "──"), // Ctrl + - -> ── (雙破折號)
            ("<", "《"),      // Shift + , -> 《 (開書名號)
            (">", "》"),      // Shift + . -> 》 (閉書名號)
            ("^", "……"),     // Shift + 6 -> …… (刪節號)
            (",", "，"),      // 單按逗號 -> ，
            (".", "。"),      // 單按句點 -> 。
            ("?", "？"),      // Shift + / -> ？
            ("/", "／"),      // 單按斜線 -> ／
            ("!", "！"),      // Shift + 1 -> ！
            (":", "："),      // Shift + ; -> ：
            (";", "；"),      // 單按分號 -> ；
            ("[", "「"),      // 單按左括號 -> 「
            ("]", "」"),      // 單按右括號 -> 」
            ("{", "『"),      // Shift + [ -> 『
            ("}", "』"),      // Shift + ] -> 』
            ("(", "（"),      // Shift + 9 -> （
            (")", "）"),      // Shift + 0 -> ）
            ("'", "、"),      // 單按單引號 -> 、
            ("\"", "”"),     // Shift + ' -> ”
            ("-", "─"),      // 單按減號 -> ─
            ("_", "＿"),      // Shift + - -> ＿
            ("\\", "＼"),     // 單按反斜線 -> ＼
            ("|", "｜"),      // Shift + \ -> ｜
            ("~", "～"),      // Shift + ` -> ～
            ("+", "＋"),      // 加號 -> 全形加號
            ("=", "＝"),      // 等號 -> 全形等號
            ("*", "＊"),      // 星號 -> 全形星號
        ];

        for &(k, v) in defaults {
            self.map.insert(k.to_string(), v.to_string());
        }
    }

    /// 從 TSV 檔案載入使用者自訂符號表
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let p = path.as_ref();
        let file = File::open(p)?;
        let reader = BufReader::new(file);
        let mut count = 0;

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with("KEY") || trimmed.starts_with("按鍵") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let key_str = normalize_key(parts[0]);
                let val_str = parts[1].trim();

                if !key_str.is_empty() && !val_str.is_empty() {
                    self.map.insert(key_str, val_str.to_string());
                    count += 1;
                }
            }
        }

        self.file_path = Some(p.to_path_buf());
        if let Ok(meta) = std::fs::metadata(p) {
            self.last_modified = meta.modified().ok();
        }

        Ok(count)
    }

    /// 熱置換檢查：若 TSV 設定檔在外部被修改，自動即時重載
    pub fn maybe_reload(&mut self) {
        if let Some(ref path) = self.file_path {
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(mtime) = meta.modified() {
                    let need_reload = match self.last_modified {
                        Some(last) => mtime > last,
                        None => true,
                    };
                    if need_reload {
                        let p = path.clone();
                        let _ = self.load_tsv(p);
                    }
                }
            }
        }
    }

    /// 查詢字串鍵值（支援 "Ctrl+."、"Ctrl+,"、"<"、"," 等）
    pub fn get(&mut self, key: &str) -> Option<String> {
        self.maybe_reload();
        let norm = normalize_key(key);
        self.map.get(&norm).cloned().or_else(|| self.map.get(key).cloned())
    }

    /// 查詢單一字元
    pub fn get_char(&mut self, ch: char) -> Option<String> {
        let mut buf = [0u8; 4];
        self.get(ch.encode_utf8(&mut buf))
    }

    /// 是否包含指定按鍵或按鍵組合
    pub fn contains_key(&mut self, key: &str) -> bool {
        self.get(key).is_some()
    }
}
