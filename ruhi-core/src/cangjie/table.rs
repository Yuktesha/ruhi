/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/cangjie/table.rs - 倉頡字根與字碼對照表

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CangjieEntry {
    pub hanzi: String,
    pub weight: u32,
}

pub struct CangjieTable {
    pub map: HashMap<String, Vec<CangjieEntry>>,
    pub char_head_map: HashMap<char, Vec<char>>,
}

impl Default for CangjieTable {
    fn default() -> Self {
        let mut table = Self {
            map: HashMap::new(),
            char_head_map: HashMap::new(),
        };
        table.load_builtin_defaults();
        table
    }
}

impl CangjieTable {
    pub fn new() -> Self {
        Self::default()
    }

    fn record_char_head(&mut self, code: &str, hanzi: &str) {
        if hanzi.chars().count() == 1 {
            if let (Some(ch), Some(first_code)) = (hanzi.chars().next(), code.chars().next()) {
                let first_lower = first_code.to_ascii_lowercase();
                let heads = self.char_head_map.entry(ch).or_default();
                if !heads.contains(&first_lower) {
                    heads.push(first_lower);
                }
            }
        }
    }

    /// 查詢倉頡多字詞頭碼（至少 3 字詞以上）：比對詞庫中每個漢字之倉頡首碼是否吻合 `headcode`
    pub fn lookup_phrase_headcode(
        &self,
        headcode: &str,
        lexicon: &crate::lexicon::LexiconManager,
    ) -> Vec<String> {
        let code_chars: Vec<char> = headcode.trim().to_ascii_lowercase().chars().collect();
        let target_len = code_chars.len();
        if target_len < 3 || !code_chars.iter().all(|c| c.is_ascii_lowercase()) {
            return Vec::new();
        }

        let mut scored: Vec<(String, u32)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry in &lexicon.entries {
            if entry.word.chars().count() != target_len {
                continue;
            }
            let matched = entry
                .word
                .chars()
                .zip(code_chars.iter())
                .all(|(ch, &expected_head)| {
                    self.char_head_map
                        .get(&ch)
                        .map(|heads| heads.contains(&expected_head))
                        .unwrap_or(false)
                });
            if matched && seen.insert(entry.word.clone()) {
                scored.push((entry.word.clone(), entry.weight));
            }
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().take(27).map(|(w, _)| w).collect()
    }

    /// 依字母查倉頡 24 基本字根名稱
    pub fn radical_name(ch: char) -> Option<&'static str> {
        match ch.to_ascii_lowercase() {
            'a' => Some("日"),
            'b' => Some("月"),
            'c' => Some("金"),
            'd' => Some("木"),
            'e' => Some("水"),
            'f' => Some("火"),
            'g' => Some("土"),
            'h' => Some("竹"),
            'i' => Some("戈"),
            'j' => Some("十"),
            'k' => Some("大"),
            'l' => Some("中"),
            'm' => Some("一"),
            'n' => Some("弓"),
            'o' => Some("人"),
            'p' => Some("心"),
            'q' => Some("手"),
            'r' => Some("口"),
            's' => Some("尸"),
            't' => Some("廿"),
            'u' => Some("山"),
            'v' => Some("女"),
            'w' => Some("田"),
            'x' => Some("難"),
            'y' => Some("卜"),
            'z' => Some("重"),
            _ => None,
        }
    }

    /// 將倉頡碼字串轉為字根提示文字，例如 "ab" -> "日月"
    pub fn code_to_radicals(code: &str) -> String {
        code.chars().filter_map(Self::radical_name).collect()
    }

    /// 查詢指定倉頡碼的漢字候選列表
    pub fn lookup(&self, code: &str) -> Option<&Vec<CangjieEntry>> {
        self.map.get(code)
    }

    /// 從 TSV 檔案讀取倉頡碼表 (CODE\tCHAR\tWEIGHT 或 CODE\tCHAR)
    pub fn load_tsv(&mut self, path: &Path) -> io::Result<usize> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("CODE") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let code = parts[0].trim().to_ascii_lowercase();
                let hanzi = parts[1].trim().to_string();
                let weight = if parts.len() >= 3 {
                    parts[2].trim().parse::<u32>().unwrap_or(1000)
                } else {
                    1000
                };

                self.record_char_head(&code, &hanzi);
                let entries = self.map.entry(code).or_default();
                // 避免重複加入相同漢字
                if !entries.iter().any(|e| e.hanzi == hanzi) {
                    entries.push(CangjieEntry { hanzi, weight });
                    count += 1;
                }
            }
        }

        // 對多重碼進行排序，權重高的在前面
        for entries in self.map.values_mut() {
            entries.sort_by(|a, b| b.weight.cmp(&a.weight));
        }

        Ok(count)
    }

    /// 內建常見字庫（保證離線/單元測試皆可零依賴立即運行）
    fn load_builtin_defaults(&mut self) {
        let defaults = [
            ("a", "日", 1000), ("b", "月", 1000), ("c", "金", 1000), ("d", "木", 1000),
            ("e", "水", 1000), ("f", "火", 1000), ("g", "土", 1000), ("h", "竹", 1000),
            ("i", "戈", 1000), ("j", "十", 1000), ("k", "大", 1000), ("l", "中", 1000),
            ("m", "一", 1000), ("n", "弓", 1000), ("o", "人", 1000), ("p", "心", 1000),
            ("q", "手", 1000), ("r", "口", 1000), ("s", "尸", 1000), ("t", "廿", 1000),
            ("u", "山", 1000), ("v", "女", 1000), ("w", "田", 1000), ("x", "難", 1000),
            ("y", "卜", 1000), ("z", "重", 1000),
            // 核心雙碼與多碼字
            ("ab", "明", 1200), ("ab", "冐", 300),
            ("amyo", "是", 1500),
            ("hapi", "的", 1600),
            ("mf", "不", 1500),
            ("hqi", "我", 1500),
            ("kb", "有", 1400),
            ("nn", "了", 1500),
            ("vr", "如", 1300),
            ("grtr", "喜", 1300),
            ("mgwg", "理", 1300),
            ("lwg", "裡", 1300), ("yvg", "裡", 1200),
            ("mgln", "到", 1300),
            ("on", "你", 1400),
            ("pd", "必", 1100),
            ("tc", "尊", 1100),
            ("g", "一", 800),
            ("git", "弇", 500),
            // 常見多字地標與成語首碼字根支援
            ("lmp", "忠", 1100),
            ("jknd", "孝", 1100),
            ("dw", "東", 1200),
            ("rmher", "路", 1200),
            ("ir", "台", 1200),
            ("lmp", "北", 1200),
            ("jwj", "車", 1200),
            ("ytr", "站", 1100),
            ("tmtj", "華", 1100),
            ("rvp", "民", 1200),
            ("wirm", "國", 1300),
            ("eyiu", "流", 1100),
            ("yjj", "連", 1100),
            ("yp", "忘", 1100),
            ("yhe", "返", 1100),
            // 倉頡／速成（首尾碼）與字形重碼消歧代表字例
            ("wg", "理", 1300), ("wg", "裡", 1200), ("wg", "里", 1150),
            ("mmu", "頑", 1150), ("mmu", "元", 1100),
            ("to", "越", 1200), ("to", "月", 1150),
            ("it", "渡", 1180), ("it", "度", 1150),
            ("gm", "瓶", 1180), ("gm", "坪", 1150),
            ("yr", "言", 1250), ("yr", "訁", 400),
        ];

        for (code, hanzi, weight) in defaults {
            self.record_char_head(code, hanzi);
            let entries = self.map.entry(code.to_string()).or_default();
            if !entries.iter().any(|e| e.hanzi == hanzi) {
                entries.push(CangjieEntry {
                    hanzi: hanzi.to_string(),
                    weight,
                });
            }
        }
    }
}
