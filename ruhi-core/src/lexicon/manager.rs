/// RuhI (如喜) Input Method Framework - Rust Core
/// src/lexicon/manager.rs - 繁體 16 萬詞庫與首碼索引管理器

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LexiconEntry {
    pub word: String,
    pub bopomofo: String,
    pub weight: u32,
    pub category: String,
    pub note: String,
    pub headcode: String,
}

impl LexiconEntry {
    pub fn new(word: &str, bopomofo: &str, weight: u32, category: &str, note: &str) -> Self {
        let headcode = Self::extract_headcode(bopomofo);
        Self {
            word: word.to_string(),
            bopomofo: bopomofo.to_string(),
            weight,
            category: category.to_string(),
            note: note.to_string(),
            headcode,
        }
    }

    pub fn strip_tones(bpm: &str) -> String {
        bpm.replace(['ˊ', 'ˇ', 'ˋ', '˙'], "")
    }

    pub fn normalize_bpm(bpm: &str) -> String {
        bpm.split_whitespace()
            .map(|s| {
                if s.ends_with('˙') {
                    format!("˙{}", &s[..s.len() - '˙'.len_utf8()])
                } else {
                    s.to_string()
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    }

    fn extract_headcode(bpm: &str) -> String {
        let initial_map: HashMap<char, char> = [
            ('ㄅ', 'b'), ('ㄆ', 'p'), ('ㄇ', 'm'), ('ㄈ', 'f'),
            ('ㄉ', 'd'), ('ㄊ', 't'), ('ㄋ', 'n'), ('ㄌ', 'l'),
            ('ㄍ', 'g'), ('ㄎ', 'k'), ('ㄏ', 'h'),
            ('ㄐ', 'j'), ('ㄑ', 'v'), ('ㄒ', 'c'),
            ('ㄓ', 'j'), ('ㄔ', 'v'), ('ㄕ', 'c'), ('ㄖ', 'r'),
            ('ㄗ', 'z'), ('ㄘ', 'a'), ('ㄙ', 's'),
            ('ㄧ', 'e'), ('ㄨ', 'x'), ('ㄩ', 'u')
        ].iter().cloned().collect();

        let mut res = String::new();
        for s in bpm.split_whitespace() {
            let clean = s.replace('˙', "");
            if let Some(first) = clean.chars().next() {
                if let Some(&code) = initial_map.get(&first) {
                    res.push(code);
                }
            }
        }
        res
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LexiconManager {
    pub entries: Vec<LexiconEntry>,
    pub bopomofo_index: HashMap<String, Vec<usize>>,
    pub notone_index: HashMap<String, Vec<usize>>,
    pub headcode_index: HashMap<String, Vec<usize>>,
    pub word_index: HashMap<String, Vec<usize>>,
}

impl LexiconManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P, default_cat: &str) -> Result<usize, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if idx == 0 && (trimmed.contains("WORD") || trimmed.contains("詞彙")) {
                continue;
            }

            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let word = parts[0].trim();
                let bpm = parts[1].trim();
                let weight: u32 = parts.get(2).and_then(|w| w.parse().ok()).unwrap_or(150);
                let cat = parts.get(3).unwrap_or(&default_cat).trim();
                let note = parts.get(4).unwrap_or(&"").trim();

                let entry = LexiconEntry::new(word, bpm, weight, cat, note);
                self.add_entry(entry);
                count += 1;
            }
        }

        self.sort_indices();
        Ok(count)
    }

fn is_archaic_polyphone(word: &str, bpm: &str) -> bool {
    let clean_bpm = bpm.replace('˙', "");
    matches!((word, clean_bpm.as_str()),
        ("尾", "ㄧˇ") |      // 尾巴只讀 ㄨㄟˇ，絕不讀 ㄧˇ 劫掠「以/已」
        ("會", "ㄏㄨㄟˇ") |   // 會只讀 ㄏㄨㄟˋ 或 ㄎㄨㄞˋ，絕不讀 ㄏㄨㄟˇ 劫掠「毀/悔」
        ("𣇫", _) |          // 罕見字
        ("篼", _) |          // 篼只在竹篼，防劫掠「都」
        ("荊婦", _) |        // 杜絕「尾荊婦」
        ("時後", _) |        // 杜絕破壞「時候」
        ("字擠", _) |        // 杜絕破壞「自己」
        ("慧覺", _) |        // 杜絕破壞「會覺/會覺得」
        ("姻黨", _) |        // 古代外戚朋黨，杜絕劫掠現代資訊詞「音檔」
        ("內外姻黨", _) |
        ("赭衣", _) |        // 漢代刑徒囚衣，杜絕劫掠「~者一~」邊界
        ("瘐死", _) |        // 古代獄死冷僻詞，杜絕劫掠「~與死~」邊界
        ("劈刺", _) |        // 刺槍軍事古語，杜絕劫掠現代資訊詞「批次」
        ("家祠", _)         // 古代宗族祠堂冷僻詞，杜絕劫掠現代詞「加詞」
    )
}

    pub fn add_entry(&mut self, entry: LexiconEntry) {
        if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
            return;
        }

        let idx = self.entries.len();
        let notone = LexiconEntry::strip_tones(&entry.bopomofo);
        let headcode = entry.headcode.clone();
        let norm_bpm = LexiconEntry::normalize_bpm(&entry.bopomofo);

        self.word_index.entry(entry.word.clone()).or_default().push(idx);
        self.bopomofo_index.entry(entry.bopomofo.clone()).or_default().push(idx);
        if norm_bpm != entry.bopomofo {
            self.bopomofo_index.entry(norm_bpm).or_default().push(idx);
        }
        self.notone_index.entry(notone).or_default().push(idx);
        if !headcode.is_empty() {
            self.headcode_index.entry(headcode).or_default().push(idx);
        }

        self.entries.push(entry);
    }

    fn sort_indices(&mut self) {
        let entries = &self.entries;
        for indices in self.word_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
        }
        for indices in self.bopomofo_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
        }
        for indices in self.notone_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
        }
        for indices in self.headcode_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
        }
    }

    pub fn lookup_bopomofo(&self, bpm: &str, exact_tone: bool) -> Vec<&LexiconEntry> {
        if exact_tone {
            if let Some(indices) = self.bopomofo_index.get(bpm) {
                return indices.iter().map(|&i| &self.entries[i]).collect();
            }
        } else {
            let notone = LexiconEntry::strip_tones(bpm);
            if let Some(indices) = self.notone_index.get(&notone) {
                return indices.iter().map(|&i| &self.entries[i]).collect();
            }
        }
        Vec::new()
    }

    pub fn lookup_headcode(&self, code: &str) -> Vec<&LexiconEntry> {
        if let Some(indices) = self.headcode_index.get(&code.to_ascii_lowercase()) {
            return indices.iter().map(|&i| &self.entries[i]).collect();
        }
        Vec::new()
    }

    pub fn has_word(&self, word: &str) -> bool {
        self.word_index.contains_key(word)
    }

    /// 反查單一中文字的同音候選字 (Homophone candidate lookup for a single character)
    /// 嚴格純單字：優先同音同調單字，接續同音異調單字。
    /// 若指定 preferred_bpm，則嚴格以該注音之同音（及同音節異調）為唯一首要候選，絕對嚴禁其他破音讀音（讀音B）排擠劫掠！
    pub fn lookup_char_candidates_with_bpm(&self, ch: char, preferred_bpm: Option<&str>) -> Vec<String> {
        let ch_str = ch.to_string();
        let mut results = Vec::new();

        if let Some(target_bpm) = preferred_bpm {
            let target_base = LexiconEntry::strip_tones(target_bpm);

            // 1. 指定讀音之「同音同調」單字 (Exact Tone Single Characters) - 優先排序高頻字
            let entries = self.lookup_bopomofo(target_bpm, true);
            for e in entries {
                if e.word.chars().count() == 1 && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }

            // 2. 指定讀音之「同音異調」單字 (Other Tones of SAME base syllable)
            // 嚴格比對 strip_tones，絕不放行不同聲母/介音/韻母之破音字！
            let all_tone_entries = self.lookup_bopomofo(target_bpm, false);
            for e in all_tone_entries {
                if e.word.chars().count() == 1 && LexiconEntry::strip_tones(&e.bopomofo) == target_base && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }

            // 3. 備用容錯：若該字另有其他破音讀音，僅於清單最後方補遺，絕不搶奪前排！
            let mut other_bpms = Vec::new();
            if let Some(indices) = self.word_index.get(&ch_str) {
                for &idx in indices {
                    let bpm = &self.entries[idx].bopomofo;
                    if LexiconEntry::strip_tones(bpm) != target_base && !other_bpms.contains(bpm) {
                        other_bpms.push(bpm.clone());
                    }
                }
            }
            for bpm in other_bpms {
                let poly_entries = self.lookup_bopomofo(&bpm, true);
                for e in poly_entries {
                    if e.word.chars().count() == 1 && !results.contains(&e.word) {
                        results.push(e.word.clone());
                    }
                }
            }

            return results;
        }

        // 未指定 preferred_bpm 時：
        // 依 word_index 中權重最高之第一讀音 (Reading A) 為主要讀音，絕不跨讀音交錯混雜！
        let mut bpms_found = Vec::new();
        if let Some(indices) = self.word_index.get(&ch_str) {
            for &idx in indices {
                let bpm = &self.entries[idx].bopomofo;
                if !bpms_found.contains(bpm) {
                    bpms_found.push(bpm.clone());
                }
            }
        } else {
            for entry in &self.entries {
                if entry.word == ch_str && !bpms_found.contains(&entry.bopomofo) {
                    bpms_found.push(entry.bopomofo.clone());
                }
            }
        }
        if bpms_found.is_empty() {
            return Vec::new();
        }

        // 依序處理各讀音：先滿足第一主讀音 (Exact -> Other Tones)，再處理後續次要讀音
        for bpm in &bpms_found {
            let base = LexiconEntry::strip_tones(bpm);
            // 同音同調
            let entries = self.lookup_bopomofo(bpm, true);
            for e in entries {
                if e.word.chars().count() == 1 && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }
            // 同音異調 (限同音節)
            let all_tone_entries = self.lookup_bopomofo(bpm, false);
            for e in all_tone_entries {
                if e.word.chars().count() == 1 && LexiconEntry::strip_tones(&e.bopomofo) == base && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }
        }

        results
    }

    pub fn lookup_char_candidates(&self, ch: char) -> Vec<String> {
        self.lookup_char_candidates_with_bpm(ch, None)
    }

    /// 反查詞彙或單字的同音候選詞 (Homophone candidate lookup for a word/phrase)
    /// 嚴格同長度：優先同音同調同長度詞彙，接續同音異調同長度詞彙與平捲舌容錯，保證不塞入多餘字數排擠候選詞！
    /// 若提供 preferred_bpm，則精確比對該注音序列，杜絕破音詞相互干擾。
    pub fn lookup_word_homophones_with_bpm(&self, word: &str, preferred_bpm: Option<&str>) -> Vec<String> {
        let char_count = word.chars().count();
        if char_count == 0 {
            return Vec::new();
        }
        if char_count == 1 {
            return self.lookup_char_candidates_with_bpm(word.chars().next().unwrap(), preferred_bpm);
        }

        let mut bpms_list: Vec<String> = Vec::new();
        if let Some(target_bpm) = preferred_bpm {
            bpms_list.push(target_bpm.to_string());
        } else {
            // 找出該詞所有可能的注音序列
            if let Some(indices) = self.word_index.get(word) {
                for &idx in indices {
                    let bpm = &self.entries[idx].bopomofo;
                    if !bpms_list.contains(bpm) {
                        bpms_list.push(bpm.clone());
                    }
                }
            }

            // 若該詞本身不在詞庫，由逐字注音排列組合出拼音序列 (Cartesian product)
            if bpms_list.is_empty() {
                let mut per_char_bpms: Vec<Vec<String>> = Vec::new();
                for ch in word.chars() {
                    let ch_str = ch.to_string();
                    let mut char_bpms = Vec::new();
                    if let Some(indices) = self.word_index.get(&ch_str) {
                        for &idx in indices {
                            let bpm = &self.entries[idx].bopomofo;
                            if !char_bpms.contains(bpm) {
                                char_bpms.push(bpm.clone());
                            }
                        }
                    }
                    if char_bpms.is_empty() {
                        return Vec::new();
                    }
                    per_char_bpms.push(char_bpms);
                }

                // 組合注音
                let mut combinations = vec![Vec::new()];
                for char_bpms in per_char_bpms {
                    let mut next_comb = Vec::new();
                    for existing in &combinations {
                        for b in &char_bpms {
                            let mut c = existing.clone();
                            c.push(b.clone());
                            next_comb.push(c);
                        }
                    }
                    combinations = next_comb;
                    if combinations.len() > 16 {
                        combinations.truncate(16);
                    }
                }

                for comb in combinations {
                    let s = comb.join(" ");
                    if !bpms_list.contains(&s) {
                        bpms_list.push(s);
                    }
                }
            }
        }

        let mut results = Vec::new();

        // 2. 第一優先級：同音同調且同字數之詞彙 (Exact Tone Homophone Words)
        for bpm in &bpms_list {
            let entries = self.lookup_bopomofo(bpm, true);
            for e in entries {
                if e.word.chars().count() == char_count && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }
        }

        // 3. 第二優先級：同音異調且同字數之詞彙 (Relaxed Tone Homophone Words)
        for bpm in &bpms_list {
            let entries = self.lookup_bopomofo(bpm, false);
            for e in entries {
                if e.word.chars().count() == char_count && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }
        }

        // 4. 第三優先級：臺灣國語平捲舌容錯同長度詞彙 (ㄓ ↔ ㄗ, ㄔ ↔ ㄘ, ㄕ ↔ ㄙ)
        for bpm in &bpms_list {
            let fuzzy_bpms = Self::get_fuzzy_bpms(bpm);
            for fb in fuzzy_bpms {
                let entries = self.lookup_bopomofo(&fb, false);
                for e in entries {
                    if e.word.chars().count() == char_count && !results.contains(&e.word) {
                        results.push(e.word.clone());
                    }
                }
            }
        }

        results
    }

    pub fn lookup_word_homophones(&self, word: &str) -> Vec<String> {
        self.lookup_word_homophones_with_bpm(word, None)
    }

    fn get_fuzzy_bpms(bpm: &str) -> Vec<String> {
        let mut variants = Vec::new();
        let map_pairs = [
            ("ㄓ", "ㄗ"), ("ㄗ", "ㄓ"),
            ("ㄔ", "ㄘ"), ("ㄘ", "ㄔ"),
            ("ㄕ", "ㄙ"), ("ㄙ", "ㄕ"),
            ("ㄣ", "ㄥ"), ("ㄥ", "ㄣ"),
        ];
        for (from, to) in map_pairs {
            if bpm.contains(from) {
                let swapped = bpm.replace(from, to);
                if !variants.contains(&swapped) {
                    variants.push(swapped);
                }
            }
        }
        variants
    }

    /// 將詞庫序列化為二進位 bincode 格式 (約 3~4 MB，啟動速度提升 10 倍以上)
    pub fn save_bin<P: AsRef<Path>>(&self, path: P) -> Result<usize, Box<dyn std::error::Error>> {
        let encoded = bincode::serialize(self)?;
        let size = encoded.len();
        std::fs::write(path, &encoded)?;
        Ok(size)
    }

    /// 從二進位 bincode 格式直接還原完整詞庫 (含三重索引)
    pub fn load_bin<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = std::fs::read(path)?;
        let mgr: Self = bincode::deserialize(&data)?;
        Ok(mgr)
    }

    /// 智慧載入：優先使用 .bin 快取，若不存在或過期則從 .tsv 載入並自動生成快取
    pub fn load_tsv_with_cache<P: AsRef<Path>>(&mut self, tsv_path: P, default_cat: &str) -> Result<usize, Box<dyn std::error::Error>> {
        let tsv = tsv_path.as_ref();
        let bin_path = tsv.with_extension("bin");
        
        // 檢查 .bin 是否存在且比 .tsv 新
        if bin_path.exists() {
            let tsv_mtime = std::fs::metadata(tsv)?.modified()?;
            let bin_mtime = std::fs::metadata(&bin_path)?.modified()?;
            if bin_mtime >= tsv_mtime {
                // .bin 比 .tsv 新，直接載入
                match Self::load_bin(&bin_path) {
                    Ok(loaded) => {
                        let count = loaded.entries.len();
                        *self = loaded;
                        return Ok(count);
                    }
                    Err(_) => {} // .bin 損壞，退回 .tsv
                }
            }
        }
        
        // 從 .tsv 載入
        let count = self.load_tsv(tsv, default_cat)?;
        
        // 自動生成 .bin 快取
        let _ = self.save_bin(&bin_path);
        
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_char_candidates_polyphonic() {
        let mut mgr = LexiconManager::new();
        mgr.add_entry(LexiconEntry {
            word: "那".to_string(),
            bopomofo: "ㄋㄚˋ".to_string(),
            headcode: "n".to_string(),
            weight: 1000,
            category: "general".to_string(),
            note: String::new(),
        });
        mgr.add_entry(LexiconEntry {
            word: "納".to_string(),
            bopomofo: "ㄋㄚˋ".to_string(),
            headcode: "n".to_string(),
            weight: 800,
            category: "general".to_string(),
            note: String::new(),
        });
        mgr.add_entry(LexiconEntry {
            word: "哪".to_string(),
            bopomofo: "ㄋㄚˇ".to_string(),
            headcode: "n".to_string(),
            weight: 900,
            category: "general".to_string(),
            note: String::new(),
        });
        mgr.sort_indices();

        let cands = mgr.lookup_char_candidates('那');
        assert!(cands.contains(&"那".to_string()));
        assert!(cands.contains(&"納".to_string()));
        assert!(cands.contains(&"哪".to_string()), "「那」必須能跨聲調選到「哪」(口那)");
    }

    #[test]
    fn test_lookup_word_homophones() {
        let mut mgr = LexiconManager::new();
        // 單字
        mgr.add_entry(LexiconEntry::new("之", "ㄓ", 1000, "general", ""));
        mgr.add_entry(LexiconEntry::new("源", "ㄩㄢˊ", 900, "general", ""));
        mgr.add_entry(LexiconEntry::new("支", "ㄓ", 800, "general", ""));
        mgr.add_entry(LexiconEntry::new("援", "ㄩㄢˊ", 800, "general", ""));
        // 雙字同音詞
        mgr.add_entry(LexiconEntry::new("支援", "ㄓ ㄩㄢˊ", 2000, "general", ""));
        mgr.add_entry(LexiconEntry::new("志願", "ㄓˋ ㄩㄢˋ", 1500, "general", ""));
        mgr.add_entry(LexiconEntry::new("資源", "ㄗ ㄩㄢˊ", 1800, "general", ""));
        // 排除多字或少字詞
        mgr.add_entry(LexiconEntry::new("支援前線", "ㄓ ㄩㄢˊ ㄑㄧㄢˊ ㄒㄧㄢˋ", 500, "general", ""));
        mgr.sort_indices();

        let cands = mgr.lookup_word_homophones("之源");
        assert!(cands.contains(&"支援".to_string()), "「之源」必須能查出同音同調雙字詞「支援」");
        assert!(cands.contains(&"志願".to_string()), "「之源」必須能查出同音異調雙字詞「志願」");
        assert!(cands.contains(&"資源".to_string()), "「之源」必須能容錯平捲舌查出「資源」");
        assert!(!cands.contains(&"支援前線".to_string()), "絕不可混入 4 字延伸詞「支援前線」排擠候選！");
        for c in &cands {
            assert_eq!(c.chars().count(), 2, "所有同音詞長度必須嚴格為 2");
        }
    }

    #[test]
    fn test_lookup_char_candidates_polyphonic_reading_purity() {
        let mut mgr = LexiconManager::new();
        // 「的」讀音 A: ㄉㄜ˙ (輕聲)
        mgr.add_entry(LexiconEntry::new("的", "ㄉㄜ˙", 2000, "general", ""));
        mgr.add_entry(LexiconEntry::new("得", "ㄉㄜ˙", 1500, "general", ""));
        // 「的」讀音 B: ㄉㄧˊ / ㄉㄧˋ (破音)
        mgr.add_entry(LexiconEntry::new("的", "ㄉㄧˊ", 300, "general", ""));
        mgr.add_entry(LexiconEntry::new("第", "ㄉㄧˋ", 1800, "general", ""));
        mgr.add_entry(LexiconEntry::new("帝", "ㄉㄧˋ", 1200, "general", ""));
        mgr.sort_indices();

        // 1. 指定讀音 A (ㄉㄜ˙)：首位與前段必須嚴格為 ㄉㄜ˙ 相關單字，絕不可由「第」(ㄉㄧˋ) 搶奪！
        let cands_de = mgr.lookup_char_candidates_with_bpm('的', Some("ㄉㄜ˙"));
        assert_eq!(cands_de[0], "的");
        assert_eq!(cands_de[1], "得");
        // 「第」與「帝」絕不可排在「得」之前！
        let idx_de = cands_de.iter().position(|w| w == "得").unwrap();
        if let Some(idx_di) = cands_de.iter().position(|w| w == "第") {
            assert!(idx_di > idx_de, "讀音B「第」若出現，順序必須遠在讀音A候選之後！");
        }

        // 2. 指定讀音 B (ㄉㄧˋ)：首位必須為「第」或「帝」，絕非「的/得」
        let cands_di = mgr.lookup_char_candidates_with_bpm('第', Some("ㄉㄧˋ"));
        assert_eq!(cands_di[0], "第");
        assert_eq!(cands_di[1], "帝");
    }

    #[test]
    fn test_candidate_priority_modern_over_archaic() {
        let mut mgr = LexiconManager::new();
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");

        // 1. 高效 (ㄍㄠ ㄒㄧㄠˋ) 必優先於 高校
        let cands_gaoxiao = mgr.lookup_word_homophones_with_bpm("高效", Some("ㄍㄠ ㄒㄧㄠˋ"));
        assert!(!cands_gaoxiao.is_empty());
        assert_eq!(cands_gaoxiao[0], "高效", "「高效」候選排序必位列第一，絕不可由「高校」劫掠；實際候選為：{:?}", cands_gaoxiao);

        // 2. 音檔 (ㄧㄣ ㄉㄤˇ) 必優先，且古語「姻黨」絕不入列
        let cands_yindang = mgr.lookup_word_homophones_with_bpm("音檔", Some("ㄧㄣ ㄉㄤˇ"));
        assert!(!cands_yindang.is_empty());
        assert_eq!(cands_yindang[0], "音檔", "「音檔」候選排序必位列第一；實際候選為：{:?}", cands_yindang);
        assert!(!cands_yindang.contains(&"姻黨".to_string()), "古語「姻黨」已被過濾，絕不可出現在候選中");

        // 3. 批次 (ㄆㄧ ㄘˋ) 必優先，且古語「劈刺」絕不入列
        let cands_pici = mgr.lookup_word_homophones_with_bpm("批次", Some("ㄆㄧ ㄘˋ"));
        assert!(!cands_pici.is_empty());
        assert_eq!(cands_pici[0], "批次", "「批次」候選排序必位列第一；實際候選為：{:?}", cands_pici);
        assert!(!cands_pici.contains(&"劈刺".to_string()), "軍事古語「劈刺」已被過濾，絕不可出現在候選中");

        // 4. 支援 (ㄓ ㄩㄢˊ) 必優先於 之源
        let cands_zhiyuan = mgr.lookup_word_homophones_with_bpm("支援", Some("ㄓ ㄩㄢˊ"));
        assert!(!cands_zhiyuan.is_empty());
        assert_eq!(cands_zhiyuan[0], "支援", "「支援」候選排序必位列第一；實際候選為：{:?}", cands_zhiyuan);
    }
}
