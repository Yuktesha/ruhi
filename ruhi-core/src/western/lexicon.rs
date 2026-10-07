/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/lexicon.rs - 正規歐文字典與頻率表 (Western Lexicon)

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 內建高頻歐文核心詞（確保即便離線或無外掛檔案時，常用詞亦 100% 具備正向語意信心）
pub const EMBEDDED_CORE_ENGLISH_WORDS: &[(&str, f32)] = &[
    // 頂級虛詞、動詞、代名詞、介系詞
    ("the", 100.0), ("be", 99.0), ("to", 99.0), ("of", 99.0), ("and", 99.0),
    ("a", 99.0), ("in", 99.0), ("that", 95.0), ("have", 95.0), ("i", 98.0),
    ("it", 95.0), ("for", 95.0), ("not", 95.0), ("on", 95.0), ("with", 95.0),
    ("he", 95.0), ("as", 95.0), ("you", 95.0), ("do", 95.0), ("at", 95.0),
    ("this", 95.0), ("but", 95.0), ("his", 95.0), ("by", 95.0), ("from", 95.0),
    ("they", 95.0), ("we", 95.0), ("say", 90.0), ("her", 90.0), ("she", 90.0),
    ("or", 95.0), ("an", 95.0), ("will", 90.0), ("my", 95.0), ("one", 90.0),
    ("all", 90.0), ("would", 90.0), ("there", 90.0), ("their", 90.0), ("what", 90.0),
    ("so", 90.0), ("up", 90.0), ("out", 90.0), ("if", 90.0), ("about", 90.0),
    ("who", 90.0), ("get", 90.0), ("which", 90.0), ("go", 90.0), ("me", 90.0),
    ("when", 90.0), ("make", 90.0), ("can", 90.0), ("like", 90.0), ("time", 90.0),
    ("no", 90.0), ("just", 90.0), ("him", 90.0), ("know", 90.0), ("take", 90.0),
    ("people", 88.0), ("into", 88.0), ("year", 88.0), ("your", 88.0), ("good", 88.0),
    ("some", 88.0), ("could", 88.0), ("them", 88.0), ("see", 88.0), ("other", 88.0),
    ("than", 88.0), ("then", 88.0), ("now", 88.0), ("look", 88.0), ("only", 88.0),
    ("come", 88.0), ("its", 88.0), ("over", 88.0), ("think", 88.0), ("also", 88.0),
    ("back", 88.0), ("after", 88.0), ("use", 88.0), ("two", 88.0), ("how", 88.0),
    ("our", 88.0), ("work", 88.0), ("first", 88.0), ("well", 88.0), ("way", 88.0),
    ("even", 88.0), ("new", 88.0), ("want", 88.0), ("because", 88.0), ("any", 88.0),
    ("these", 88.0), ("give", 88.0), ("day", 88.0), ("most", 88.0), ("us", 88.0),
    ("is", 100.0), ("am", 95.0), ("are", 98.0), ("was", 95.0), ("were", 95.0),
    ("been", 92.0), ("has", 92.0), ("had", 92.0), ("did", 92.0), ("does", 92.0),
    ("far", 90.0), ("let", 90.0), ("run", 90.0), ("put", 90.0), ("end", 90.0),
    ("why", 90.0), ("too", 90.0), ("yes", 92.0), ("off", 90.0), ("try", 88.0),
    ("few", 88.0), ("tell", 88.0), ("much", 90.0), ("very", 92.0), ("long", 88.0),
    ("last", 88.0), ("same", 88.0), ("never", 88.0), ("under", 88.0), ("right", 88.0),
    ("left", 88.0), ("down", 90.0), ("need", 88.0), ("keep", 88.0), ("call", 88.0),
    ("ask", 88.0), ("feel", 88.0), ("seem", 88.0), ("leave", 88.0), ("show", 88.0),
    ("hear", 88.0), ("play", 88.0), ("move", 88.0), ("live", 88.0), ("bring", 88.0),
    ("man", 88.0), ("boy", 88.0), ("girl", 88.0), ("dog", 88.0), ("cat", 88.0),
    ("car", 88.0), ("red", 88.0), ("hot", 88.0), ("bad", 88.0), ("sun", 88.0),
    // 高頻名詞、動詞、科技與日常詞
    ("book", 90.0), ("sample", 90.0), ("test", 90.0), ("word", 90.0), ("text", 90.0),
    ("file", 90.0), ("code", 90.0), ("system", 90.0), ("program", 90.0), ("app", 90.0),
    ("window", 90.0), ("desktop", 90.0), ("screen", 90.0), ("keyboard", 90.0), ("mouse", 90.0),
    ("input", 90.0), ("output", 90.0), ("data", 90.0), ("user", 90.0), ("project", 90.0),
    ("server", 90.0), ("client", 90.0), ("page", 90.0), ("web", 90.0), ("browser", 90.0),
    ("apple", 90.0), ("google", 90.0), ("microsoft", 90.0), ("windows", 90.0), ("linux", 90.0),
    ("rust", 90.0), ("python", 90.0), ("javascript", 90.0), ("html", 90.0), ("css", 90.0),
    ("small", 85.0), ("big", 85.0), ("large", 85.0), ("great", 85.0), ("high", 85.0),
    ("open", 85.0), ("close", 85.0), ("read", 85.0), ("write", 85.0), ("build", 85.0),
    ("create", 85.0), ("delete", 85.0), ("edit", 85.0), ("save", 85.0), ("help", 85.0),
    ("start", 85.0), ("stop", 85.0), ("run", 85.0), ("search", 85.0), ("find", 85.0),
    ("true", 85.0), ("false", 85.0), ("string", 85.0), ("number", 85.0), ("boolean", 85.0),
    ("array", 85.0), ("list", 85.0), ("table", 85.0), ("node", 85.0), ("key", 85.0),
    ("value", 85.0), ("name", 85.0), ("type", 85.0), ("class", 85.0), ("method", 85.0),
    ("function", 85.0), ("error", 85.0), ("warning", 85.0), ("info", 85.0), ("status", 85.0),
    ("max", 85.0), ("min", 85.0), ("size", 85.0), ("buffer", 85.0), ("len", 85.0),
    ("count", 85.0), ("pos", 85.0), ("get", 90.0), ("set", 85.0), ("element", 85.0),
    ("by", 95.0), ("id", 90.0), ("return", 90.0), ("hi", 92.0), ("ok", 95.0),
    ("hey", 90.0), ("bye", 90.0), ("china", 90.0), ("chinese", 90.0),
    ("taiwan", 90.0), ("english", 90.0), ("mix", 88.0), ("typing", 88.0), ("type", 88.0),
];

#[derive(Debug, Clone)]
pub struct WesternLexicon {
    /// 詞條 -> (詞頻, 正規化權重)
    words: HashMap<String, (u32, f32)>,
    /// 前綴集合（用於快速判斷是否正在輸入有效歐文單字）
    prefixes: HashMap<String, usize>,
}

impl Default for WesternLexicon {
    fn default() -> Self {
        let mut lex = Self {
            words: HashMap::new(),
            prefixes: HashMap::new(),
        };
        lex.load_embedded();
        lex
    }
}

impl WesternLexicon {
    pub fn new() -> Self {
        Self::default()
    }

    /// 載入內建高頻單字表
    fn load_embedded(&mut self) {
        for &(w, weight) in EMBEDDED_CORE_ENGLISH_WORDS {
            self.insert_word(w, 1000, weight);
        }
    }

    /// 插入詞條並維護前綴索引
    pub fn insert_word(&mut self, word: &str, freq: u32, weight: f32) {
        let w_lower = word.to_ascii_lowercase();
        if w_lower.is_empty() {
            return;
        }

        // 維護前綴
        let chars: Vec<char> = w_lower.chars().collect();
        for i in 1..=chars.len() {
            let pfx: String = chars[..i].iter().collect();
            *self.prefixes.entry(pfx).or_insert(0) += 1;
        }

        self.words.insert(w_lower, (freq, weight));
    }

    /// 檢查單字是否存在於字典中
    pub fn contains(&self, word: &str) -> bool {
        let w_lower = word.to_ascii_lowercase();
        self.words.contains_key(&w_lower)
    }

    /// 取得單字之權重（0.0 ~ 100.0）
    pub fn get_weight(&self, word: &str) -> Option<f32> {
        let w_lower = word.to_ascii_lowercase();
        self.words.get(&w_lower).map(|&(_, w)| w)
    }

    /// 檢查特定前綴是否能組成合法歐文單字
    pub fn has_prefix(&self, prefix: &str) -> bool {
        let pfx_lower = prefix.to_ascii_lowercase();
        self.prefixes.contains_key(&pfx_lower)
    }

    /// 取得前綴命中單字總數
    pub fn prefix_count(&self, prefix: &str) -> usize {
        let pfx_lower = prefix.to_ascii_lowercase();
        self.prefixes.get(&pfx_lower).copied().unwrap_or(0)
    }

    /// 取得詞庫總量
    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// 從 TSV 檔案（word \t freq \t weight）熱載入或補充外部歐文字典
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(0);
        }
        let file = File::open(p)?;
        let reader = BufReader::new(file);
        let mut count = 0;

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if !parts.is_empty() {
                let word = parts[0];
                let freq = if parts.len() > 1 {
                    parts[1].parse::<u32>().unwrap_or(100)
                } else {
                    100
                };
                let weight = if parts.len() > 2 {
                    parts[2].parse::<f32>().unwrap_or(50.0)
                } else {
                    50.0
                };
                self.insert_word(word, freq, weight);
                count += 1;
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_lexicon() {
        let lex = WesternLexicon::new();
        assert!(lex.contains("the"));
        assert!(lex.contains("this"));
        assert!(lex.contains("book"));
        assert!(lex.contains("sample"));
        assert!(lex.contains("google"));
        assert!(lex.contains("rust"));
        assert!(!lex.contains("fyen"));
        assert!(!lex.contains("celd"));

        assert!(lex.has_prefix("sam"));
        assert!(lex.has_prefix("boo"));
        assert!(lex.has_prefix("thi"));
        assert!(!lex.has_prefix("xyzq"));
    }
}
