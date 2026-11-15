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
    #[serde(default)]
    pub custom_abbrevs: Vec<String>,
    #[serde(default)]
    pub is_user_word: bool,
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
            custom_abbrevs: Vec::new(),
            is_user_word: false,
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

    /// 將含「不 (ㄅㄨˊ)」或「一 (ㄧˊ / ㄧˋ)」連讀變調之複詞注音，還原為本調注音（如「不再 ㄅㄨˊ ㄗㄞˋ」➔「ㄅㄨˋ ㄗㄞˋ」、「一開始 ㄧˋ ㄎㄞ ㄕˇ」➔「ㄧ ㄎㄞ ㄕˇ」）
    pub fn citation_sandhi_bpm(word: &str, norm_bpm: &str) -> Option<String> {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() < 2 {
            return None;
        }
        let syls: Vec<&str> = norm_bpm.split_whitespace().collect();
        if chars.len() != syls.len() {
            return None;
        }
        if chars.len() == 2 && chars[0] == chars[1] {
            let s0 = syls[0];
            let s1 = syls[1];
            if Self::strip_tones(s0) == Self::strip_tones(s1) {
                if s1.starts_with('˙') && !s0.starts_with('˙') {
                    return Some(format!("{s0} {s0}"));
                }
                if s0.ends_with('ˊ') && s1.ends_with('ˇ') {
                    return Some(format!("{s1} {s1}"));
                }
            }
        }
        let mut changed = false;
        let mut out_syls: Vec<String> = Vec::with_capacity(syls.len());
        for (ch, &syl) in chars.iter().zip(syls.iter()) {
            if *ch == '不' && syl == "ㄅㄨˊ" {
                out_syls.push("ㄅㄨˋ".to_string());
                changed = true;
            } else if *ch == '一' && (syl == "ㄧˊ" || syl == "ㄧˋ") {
                out_syls.push("ㄧ".to_string());
                changed = true;
            } else {
                out_syls.push(syl.to_string());
            }
        }
        if changed {
            Some(out_syls.join(" "))
        } else {
            None
        }
    }

    /// 代數連讀變調與輕聲等價類展開器 (`sandhi_equivalence_readings`)：
    /// 詞庫 (`lexicon_core.tsv`) 中每個詞條僅需儲存唯一一筆標準讀音（如 `一丁不識 ㄧˋ ㄉㄧㄥ ㄅㄨˊ ㄕˋ`、`一個 ㄧˊ ˙ㄍㄜ`），
    /// 載入索引時由本函式自動推導所有合法之「一 (`ㄧ/ㄧˊ/ㄧˋ`)」、「不 (`ㄅㄨˋ/ㄅㄨˊ`)」、「個 (`ㄍㄜˋ/˙ㄍㄜ`)」、「兒 (`ㄦ/ㄦˊ`)」及疊字變調組合，
    /// 徹底消弭詞庫檔案中窮舉排列組合之冗餘列。
    pub fn sandhi_equivalence_readings(word: &str, norm_bpm: &str) -> Vec<String> {
        let mut results = vec![norm_bpm.to_string()];
        let chars: Vec<char> = word.chars().collect();
        if chars.len() < 2 {
            return results;
        }
        let syls: Vec<&str> = norm_bpm.split_whitespace().collect();
        if chars.len() != syls.len() {
            return results;
        }

        // 1. 雙字疊字變調與輕聲還原（如 謝謝 ㄒㄧㄝˋ ˙ㄒㄧㄝ ➔ ㄒㄧㄝˋ ㄒㄧㄝˋ；水水 ㄕㄨㄟˊ ㄕㄨㄟˇ ➔ ㄕㄨㄟˇ ㄕㄨㄟˇ）
        if chars.len() == 2 && chars[0] == chars[1] {
            let s0 = syls[0];
            let s1 = syls[1];
            if Self::strip_tones(s0) == Self::strip_tones(s1) {
                if s1.starts_with('˙') && !s0.starts_with('˙') {
                    let redup = format!("{s0} {s0}");
                    if !results.contains(&redup) {
                        results.push(redup);
                    }
                } else if !s1.starts_with('˙') && !s0.starts_with('˙') && s0 == s1 {
                    let neutral_redup = format!("{s0} ˙{}", Self::strip_tones(s1));
                    if !results.contains(&neutral_redup) {
                        results.push(neutral_redup);
                    }
                }
                if s0.ends_with('ˊ') && s1.ends_with('ˇ') {
                    let redup3 = format!("{s1} {s1}");
                    if !results.contains(&redup3) {
                        results.push(redup3);
                    }
                }
            }
        }

        // 2. 「一 (ㄧ/ㄧˊ/ㄧˋ)」、「不 (ㄅㄨˋ/ㄅㄨˊ)」、「個 (ㄍㄜˋ/˙ㄍㄜ)」、「兒 (ㄦ/ㄦˊ)」正交笛卡兒積展開
        let mut per_pos_choices: Vec<Vec<&str>> = Vec::with_capacity(chars.len());
        let mut has_sandhi_slot = false;
        for (i, (&ch, &syl)) in chars.iter().zip(syls.iter()).enumerate() {
            if ch == '一' && matches!(syl, "ㄧ" | "ㄧˊ" | "ㄧˋ") {
                per_pos_choices.push(vec!["ㄧ", "ㄧˊ", "ㄧˋ"]);
                has_sandhi_slot = true;
            } else if ch == '不' && matches!(syl, "ㄅㄨˋ" | "ㄅㄨˊ") {
                per_pos_choices.push(vec!["ㄅㄨˋ", "ㄅㄨˊ"]);
                has_sandhi_slot = true;
            } else if ch == '個' && matches!(syl, "ㄍㄜˋ" | "˙ㄍㄜ") {
                per_pos_choices.push(vec!["ㄍㄜˋ", "˙ㄍㄜ"]);
                has_sandhi_slot = true;
            } else if ch == '得' && matches!(syl, "ㄉㄜˊ" | "˙ㄉㄜ") {
                per_pos_choices.push(vec!["ㄉㄜˊ", "˙ㄉㄜ"]);
                has_sandhi_slot = true;
            } else if i + 1 == chars.len() && ch == '兒' && matches!(syl, "ㄦ" | "ㄦˊ") {
                per_pos_choices.push(vec!["ㄦ", "ㄦˊ"]);
                has_sandhi_slot = true;
            } else {
                per_pos_choices.push(vec![syl]);
            }
        }

        if has_sandhi_slot {
            let mut combs: Vec<Vec<&str>> = vec![Vec::with_capacity(chars.len())];
            for choices in per_pos_choices {
                let mut next = Vec::with_capacity(combs.len() * choices.len());
                for prefix in &combs {
                    for &c in &choices {
                        let mut p = prefix.clone();
                        p.push(c);
                        next.push(p);
                    }
                }
                combs = next;
                if combs.len() > 36 {
                    combs.truncate(36);
                }
            }
            for comb in combs {
                let joined = comb.join(" ");
                if !results.contains(&joined) {
                    results.push(joined);
                }
            }
        }

        results
    }

    pub fn bpm_char_to_hsu(c: char) -> Option<char> {
        match c {
            'ㄅ' => Some('b'), 'ㄆ' => Some('p'), 'ㄇ' => Some('m'), 'ㄈ' => Some('f'),
            'ㄉ' => Some('d'), 'ㄊ' => Some('t'), 'ㄋ' => Some('n'), 'ㄌ' => Some('l'),
            'ㄍ' => Some('g'), 'ㄎ' => Some('k'), 'ㄏ' => Some('h'),
            'ㄐ' => Some('j'), 'ㄑ' => Some('v'), 'ㄒ' => Some('c'),
            'ㄓ' => Some('j'), 'ㄔ' => Some('v'), 'ㄕ' => Some('c'), 'ㄖ' => Some('r'),
            'ㄗ' => Some('z'), 'ㄘ' => Some('a'), 'ㄙ' => Some('s'),
            'ㄧ' => Some('e'), 'ㄨ' => Some('x'), 'ㄩ' => Some('u'),
            'ㄚ' => Some('y'), 'ㄛ' => Some('h'), 'ㄜ' => Some('g'), 'ㄝ' => Some('e'),
            'ㄞ' => Some('i'), 'ㄟ' => Some('a'), 'ㄠ' => Some('w'), 'ㄡ' => Some('o'),
            'ㄢ' => Some('m'), 'ㄣ' => Some('n'), 'ㄤ' => Some('k'), 'ㄥ' => Some('l'),
            'ㄦ' => Some('l'),
            _ => None,
        }
    }

    pub fn extract_headcode(bpm: &str) -> String {
        let mut res = String::new();
        for s in bpm.split_whitespace() {
            let clean = s.replace(['˙', 'ˊ', 'ˇ', 'ˋ'], "");
            if let Some(first) = clean.chars().next() {
                if let Some(code) = Self::bpm_char_to_hsu(first) {
                    res.push(code);
                }
            }
        }
        res
    }

    /// 擷取注音符號首碼序列（如「ㄓㄨㄥ ㄒㄧㄠˋ ㄉㄨㄥ ㄌㄨˋ」➔「ㄓㄒㄉㄌ」、「ㄧ ㄕˊ ㄦˋ ㄋㄧㄠˇ」➔「ㄧㄕㄦㄋ」）
    pub fn extract_bpm_headcode(bpm: &str) -> String {
        let mut res = String::new();
        for s in bpm.split_whitespace() {
            let clean = s.replace(['˙', 'ˊ', 'ˇ', 'ˋ'], "");
            if let Some(first) = clean.chars().next() {
                if ('\u{3105}'..='\u{3129}').contains(&first) {
                    res.push(first);
                }
            }
        }
        res
    }

    /// 將標準大千 (`standard`) 或倚天 26 鍵 (`et26`) 之原始按鍵序列轉為注音首碼字串
    pub fn convert_keys_to_bpm_headcode(raw_keys: &str, keyboard_mode: &str) -> Option<String> {
        let mut res = String::new();
        match keyboard_mode {
            "standard" | "daikien" => {
                for ch in raw_keys.chars() {
                    let k = ch.to_ascii_lowercase();
                    let bpm_ch = match k {
                        '1' => Some('ㄅ'), 'q' => Some('ㄆ'), 'a' => Some('ㄇ'), 'z' => Some('ㄈ'),
                        '2' => Some('ㄉ'), 'w' => Some('ㄊ'), 's' => Some('ㄋ'), 'x' => Some('ㄌ'),
                        'e' => Some('ㄍ'), 'd' => Some('ㄎ'), 'c' => Some('ㄏ'),
                        'r' => Some('ㄐ'), 'f' => Some('ㄑ'), 'v' => Some('ㄒ'),
                        '5' => Some('ㄓ'), 't' => Some('ㄔ'), 'g' => Some('ㄕ'), 'b' => Some('ㄖ'),
                        'y' => Some('ㄗ'), 'h' => Some('ㄘ'), 'n' => Some('ㄙ'),
                        'u' => Some('ㄧ'), 'j' => Some('ㄨ'), 'm' => Some('ㄩ'),
                        '8' => Some('ㄚ'), 'i' => Some('ㄛ'), 'k' => Some('ㄜ'), ',' => Some('ㄝ'),
                        '9' => Some('ㄞ'), 'o' => Some('ㄟ'), 'l' => Some('ㄠ'), '.' => Some('ㄡ'),
                        '0' => Some('ㄢ'), 'p' => Some('ㄣ'), ';' => Some('ㄤ'), '/' => Some('ㄥ'),
                        '-' => Some('ㄦ'),
                        _ => None,
                    }?;
                    res.push(bpm_ch);
                }
                Some(res)
            }
            "et26" | "eten26" => {
                for ch in raw_keys.chars() {
                    let k = ch.to_ascii_lowercase();
                    let bpm_ch = match k {
                        'b' => Some('ㄅ'), 'p' => Some('ㄆ'), 'm' => Some('ㄇ'), 'f' => Some('ㄈ'),
                        'd' => Some('ㄉ'), 't' => Some('ㄊ'), 'n' => Some('ㄋ'), 'l' => Some('ㄌ'),
                        'v' => Some('ㄍ'), 'k' => Some('ㄎ'), 'h' => Some('ㄏ'),
                        'g' => Some('ㄐ'), 'c' => Some('ㄑ'), 'x' => Some('ㄒ'),
                        'j' => Some('ㄓ'), 'q' => Some('ㄔ'), 'w' => Some('ㄕ'), 'r' => Some('ㄖ'),
                        'z' => Some('ㄗ'), 'a' => Some('ㄘ'), 's' => Some('ㄙ'),
                        'e' => Some('ㄧ'), 'u' => Some('ㄨ'), 'i' => Some('ㄩ'),
                        'o' => Some('ㄛ'),
                        _ => None,
                    }?;
                    res.push(bpm_ch);
                }
                Some(res)
            }
            _ => None,
        }
    }

    /// 將使用者在當前輸入法模式 typed 之按鍵轉換為注音符號首碼字串
    pub fn raw_query_to_bpm_headcode(query: &str, keyboard_mode: &str) -> Option<String> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        if q.chars().all(|c| ('\u{3105}'..='\u{3129}').contains(&c)) {
            return Some(q.to_string());
        }
        match keyboard_mode {
            "standard" | "daikien" => Self::convert_keys_to_bpm_headcode(q, "standard"),
            "et26" | "eten26" => Self::convert_keys_to_bpm_headcode(q, "et26"),
            _ => None,
        }
    }

    /// 解析 user_dict.tsv 之第 6 欄 (`HEADCODE`) 與第 7 欄起之自訂縮略語 (`ABBREVIATIONS`)
    /// 支援以 Tab `\t` 或連續空格 `\s+` 隔開多個自訂縮略語
    pub fn parse_user_dict_extra_columns(parts: &[&str], auto_bpm_hc: &str) -> (String, Vec<String>) {
        let mut abbrevs: Vec<String> = Vec::new();
        let push_tok = |list: &mut Vec<String>, tok: &str| {
            let t = tok.trim();
            if !t.is_empty() {
                let norm = if t.chars().all(|c| c.is_ascii()) {
                    t.to_ascii_lowercase()
                } else {
                    t.to_string()
                };
                if !list.contains(&norm) {
                    list.push(norm);
                }
            }
        };

        if parts.len() > 5 {
            let col5 = parts[5].trim();
            let tokens5: Vec<&str> = col5.split_whitespace().collect();
            if !tokens5.is_empty() {
                let first = tokens5[0];
                let is_bpm = first == auto_bpm_hc
                    || first.chars().all(|c| ('\u{3105}'..='\u{3129}').contains(&c));
                if is_bpm {
                    for &t in &tokens5[1..] {
                        push_tok(&mut abbrevs, t);
                    }
                } else {
                    for &t in &tokens5 {
                        push_tok(&mut abbrevs, t);
                    }
                }
            }
        }
        for col in parts.iter().skip(6) {
            for t in col.split_whitespace() {
                push_tok(&mut abbrevs, t);
            }
        }
        (auto_bpm_hc.to_string(), abbrevs)
    }

    /// 自動檢查並補齊 user_dict.tsv 的第 6 欄注音首碼 (HEADCODE)，僅在檔案內容有變更時才重新寫入磁碟以維護 mtime
    pub fn backfill_user_dict_headcodes<P: AsRef<Path>>(path: P) {
        let p = path.as_ref();
        let content = match std::fs::read_to_string(p) {
            Ok(c) => c,
            Err(_) => return,
        };
        if content.trim().is_empty() {
            return;
        }

        let mut out_lines: Vec<String> = Vec::new();
        let mut has_help_comment = content.lines().any(|l| l.contains("第 6 欄 HEADCODE"));

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                if trimmed.contains("格式規範:") {
                    out_lines.push("# 格式規範: WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE\tHEADCODE\tABBREVIATIONS".to_string());
                    if !has_help_comment {
                        out_lines.push("# 說明: 第 6 欄 HEADCODE 自動補上注音頭碼；第 7 欄起可自由輸入多個自訂縮略語（支援以 Tab 或連續空格隔開）".to_string());
                        has_help_comment = true;
                    }
                } else {
                    out_lines.push(line.to_string());
                }
                continue;
            }
            if trimmed.is_empty() {
                out_lines.push(line.to_string());
                continue;
            }

            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() >= 2 {
                let norm_bpm = Self::normalize_bpm(cols[1].trim());
                let auto_hc = Self::extract_bpm_headcode(&norm_bpm);
                if !auto_hc.is_empty() {
                    let w = cols[0].trim();
                    let bpm = cols[1].trim();
                    let wt = cols.get(2).map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or("600");
                    let cat = cols.get(3).map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or("user");
                    let note = cols.get(4).copied().unwrap_or("").trim();

                    let mut new_cols: Vec<String> = vec![
                        w.to_string(),
                        bpm.to_string(),
                        wt.to_string(),
                        cat.to_string(),
                        note.to_string(),
                    ];

                    if cols.len() <= 5 {
                        new_cols.push(auto_hc);
                    } else {
                        let col5 = cols[5].trim();
                        let first_tok = col5.split_whitespace().next().unwrap_or("");
                        if col5.is_empty() {
                            new_cols.push(auto_hc);
                            for c in cols.iter().skip(6) {
                                new_cols.push(c.to_string());
                            }
                        } else if !first_tok.chars().all(|c| ('\u{3105}'..='\u{3129}').contains(&c)) {
                            new_cols.push(auto_hc);
                            for c in cols.iter().skip(5) {
                                new_cols.push(c.to_string());
                            }
                        } else {
                            new_cols.push(cols[5].to_string());
                            for c in cols.iter().skip(6) {
                                new_cols.push(c.to_string());
                            }
                        }
                    }
                    out_lines.push(new_cols.join("\t"));
                    continue;
                }
            }
            out_lines.push(line.to_string());
        }

        let ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let mut new_content = out_lines.join(ending);
        if content.ends_with('\n') {
            new_content.push_str(ending);
        }
        if new_content != content {
            let _ = std::fs::write(p, new_content);
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchedLexiconFile {
    pub path: std::path::PathBuf,
    pub category: String,
    pub last_modified: Option<std::time::SystemTime>,
    pub last_line_count: usize,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct LexiconManager {
    pub entries: Vec<LexiconEntry>,
    pub bopomofo_index: HashMap<String, Vec<usize>>,
    pub notone_index: HashMap<String, Vec<usize>>,
    pub headcode_index: HashMap<String, Vec<usize>>,
    pub word_index: HashMap<String, Vec<usize>>,
    #[serde(default)]
    pub user_abbrev_index: HashMap<String, Vec<usize>>,
    #[serde(default)]
    pub user_word_indices: Vec<usize>,
    #[serde(skip)]
    pub watched_files: Vec<WatchedLexiconFile>,
    #[serde(skip)]
    pub last_check_instant: Option<std::time::Instant>,
}

impl LexiconManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 註冊使用者詞條之首碼與多組自訂縮略語索引（適用於 1 字、2 字、3 字及 4+ 字自建詞）
    pub fn register_user_entry_indices(&mut self, idx: usize) {
        if idx >= self.entries.len() {
            return;
        }
        self.entries[idx].is_user_word = true;
        if !self.user_word_indices.contains(&idx) {
            self.user_word_indices.push(idx);
        }
        let hsu_hc = self.entries[idx].headcode.to_ascii_lowercase();
        let bpm_hc = LexiconEntry::extract_bpm_headcode(&self.entries[idx].bopomofo);
        let abbrevs = self.entries[idx].custom_abbrevs.clone();
        let char_count = self.entries[idx].word.chars().count();

        if !bpm_hc.is_empty() {
            let list = self.user_abbrev_index.entry(bpm_hc.clone()).or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        }
        if !hsu_hc.is_empty() {
            let list = self.user_abbrev_index.entry(hsu_hc.clone()).or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        }
        for abbr in &abbrevs {
            let norm = abbr.to_ascii_lowercase();
            let list = self.user_abbrev_index.entry(norm).or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        }

        // 同步登記至 headcode_index，供 ↑/lookup_multi_char_headcode 擴展
        if char_count >= 3 {
            if !bpm_hc.is_empty() {
                let list = self.headcode_index.entry(bpm_hc).or_default();
                if !list.contains(&idx) {
                    list.push(idx);
                }
            }
            if !hsu_hc.is_empty() {
                let list = self.headcode_index.entry(hsu_hc).or_default();
                if !list.contains(&idx) {
                    list.push(idx);
                }
            }
        }
        for abbr in &abbrevs {
            if abbr.chars().count() >= 2 {
                let list = self.headcode_index.entry(abbr.to_ascii_lowercase()).or_default();
                if !list.contains(&idx) {
                    list.push(idx);
                }
            }
        }
    }

    pub fn watch_dynamic_file<P: AsRef<Path>>(&mut self, path: P, category: &str) {
        let p = path.as_ref().to_path_buf();
        let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        if let Some(existing) = self.watched_files.iter_mut().find(|w| w.path == p) {
            existing.last_modified = mtime;
            existing.category = category.to_string();
        } else {
            self.watched_files.push(WatchedLexiconFile {
                path: p,
                category: category.to_string(),
                last_modified: mtime,
                last_line_count: 0,
            });
        }
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P, default_cat: &str) -> Result<usize, std::io::Error> {
        let orig_p = path.as_ref();
        let redirected_core;
        let p: &Path = if !orig_p.exists() {
            let fname = orig_p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let core_candidate = orig_p.with_file_name("lexicon_core.tsv");
            if core_candidate.exists() {
                match fname {
                    "lexicon_mcbopomofo.tsv" => {
                        redirected_core = core_candidate;
                        &redirected_core
                    }
                    "lexicon_moe.tsv" | "lexicon_domain.tsv" => {
                        return Ok(0);
                    }
                    _ => orig_p,
                }
            } else {
                orig_p
            }
        } else {
            orig_p
        };

        let is_user_dict = p.file_name().and_then(|n| n.to_str()).map(|s| s == "user_dict.tsv").unwrap_or(false);
        if is_user_dict {
            LexiconEntry::backfill_user_dict_headcodes(p);
        }

        let file = File::open(p)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        let mut total_lines = 0;

        for (idx, line_res) in reader.lines().enumerate() {
            total_lines = idx + 1;
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
                let norm_bpm = LexiconEntry::normalize_bpm(bpm);
                let weight: u32 = parts.get(2).and_then(|w| w.parse().ok()).unwrap_or(150);
                let raw_cat = parts.get(3).map(|s| s.trim()).unwrap_or("");
                let cat = if raw_cat.is_empty() {
                    if default_cat == "user" {
                        "user"
                    } else if word.chars().count() == 2 && weight < 90 && word != "諺文" {
                        "moe_unattested2"
                    } else {
                        default_cat
                    }
                } else {
                    raw_cat
                };
                let note = parts.get(4).unwrap_or(&"").trim();

                let mut entry = LexiconEntry::new(word, &norm_bpm, weight, cat, note);
                if cat == "user" || default_cat == "user" {
                    let (_, abbrevs) = LexiconEntry::parse_user_dict_extra_columns(&parts, &LexiconEntry::extract_bpm_headcode(&norm_bpm));
                    entry.category = "user".to_string();
                    entry.custom_abbrevs = abbrevs;
                    entry.is_user_word = true;
                    self.upsert_entry(entry);
                    let (sub_entries, _) = Self::decompose_entity_subentries(word, &norm_bpm, weight);
                    for mut sub in sub_entries {
                        sub.category = "user".to_string();
                        sub.is_user_word = true;
                        self.upsert_entry(sub);
                    }
                } else {
                    self.add_entry(entry);
                }
                count += 1;
            }
        }

        let is_dynamic = matches!(default_cat, "domain" | "user")
            || p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s == "lexicon_core.tsv" || s == "lexicon_domain.tsv" || s == "user_dict.tsv")
                .unwrap_or(false);
        if is_dynamic {
            let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let pb = p.to_path_buf();
            if let Some(existing) = self.watched_files.iter_mut().find(|w| w.path == pb) {
                existing.last_modified = mtime;
                existing.last_line_count = total_lines;
            } else {
                self.watched_files.push(WatchedLexiconFile {
                    path: pb,
                    category: default_cat.to_string(),
                    last_modified: mtime,
                    last_line_count: total_lines,
                });
            }
        }

        self.sort_indices();
        Ok(count)
    }

    /// 新增或提升既有詞條權重（防止同一詞條與讀音重複堆疊，同時確保使用者詞條與拆分子詞權重生效）
    pub fn upsert_entry(&mut self, mut entry: LexiconEntry) {
        if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
            return;
        }
        let norm_bpm = LexiconEntry::normalize_bpm(&entry.bopomofo);
        if entry.note == "auto_given_name" || entry.note == "ephemeral_given_name" {
            let mut max_homophone_wt = 0u32;
            if let Some(indices) = self.bopomofo_index.get(&norm_bpm) {
                for &idx in indices {
                    let e = &self.entries[idx];
                    if e.word.chars().count() == 2 && e.weight > max_homophone_wt {
                        max_homophone_wt = e.weight;
                    }
                }
            }
            entry.weight = entry.weight.max(1150).max(max_homophone_wt.saturating_add(100)).min(1800);
        }
        if let Some(indices) = self.word_index.get(&entry.word) {
            for &idx in indices {
                let e_bpm = &self.entries[idx].bopomofo;
                if e_bpm == &entry.bopomofo || LexiconEntry::normalize_bpm(e_bpm) == norm_bpm {
                    if entry.weight > self.entries[idx].weight {
                        self.entries[idx].weight = entry.weight;
                    }
                    if entry.category == "user" {
                        self.entries[idx].category = "user".to_string();
                        self.entries[idx].is_user_word = true;
                        for a in entry.custom_abbrevs {
                            if !self.entries[idx].custom_abbrevs.contains(&a) {
                                self.entries[idx].custom_abbrevs.push(a);
                            }
                        }
                        self.register_user_entry_indices(idx);
                    }
                    return;
                }
            }
        }
        self.add_entry(entry);
    }

    /// 註冊跨句 Session 臨時名字詞條（依非古音全排列組合反查注音，並將權重提升至高於既有同音詞）
    pub fn register_ephemeral_given_name(&mut self, given_word: &str) {
        let chars: Vec<char> = given_word.chars().collect();
        if chars.len() != 2 {
            return;
        }
        let g1 = chars[0].to_string();
        let g2 = chars[1].to_string();

        let mut bpms_g1 = Vec::new();
        if let Some(indices) = self.word_index.get(&g1) {
            for &idx in indices {
                let e = &self.entries[idx];
                if !Self::is_archaic_polyphone(&e.word, &e.bopomofo) && e.weight >= 120 {
                    let norm = LexiconEntry::normalize_bpm(&e.bopomofo);
                    if !bpms_g1.contains(&norm) {
                        bpms_g1.push(norm);
                    }
                }
            }
        }

        let mut bpms_g2 = Vec::new();
        if let Some(indices) = self.word_index.get(&g2) {
            for &idx in indices {
                let e = &self.entries[idx];
                if !Self::is_archaic_polyphone(&e.word, &e.bopomofo) && e.weight >= 120 {
                    let norm = LexiconEntry::normalize_bpm(&e.bopomofo);
                    if !bpms_g2.contains(&norm) {
                        bpms_g2.push(norm);
                    }
                }
            }
        }

        let mut modified = false;
        for b1 in &bpms_g1 {
            for b2 in &bpms_g2 {
                let combined = format!("{} {}", b1, b2);
                let norm_bpm = LexiconEntry::normalize_bpm(&combined);
                let mut max_exact_wt = 0u32;
                if let Some(indices) = self.bopomofo_index.get(&norm_bpm) {
                    for &idx in indices {
                        let e = &self.entries[idx];
                        if e.word.chars().count() == 2 && e.weight > max_exact_wt {
                            max_exact_wt = e.weight;
                        }
                    }
                }
                let eff_wt = 1150u32.max(max_exact_wt.saturating_add(100)).min(1800);
                self.upsert_entry(LexiconEntry::new(
                    given_word,
                    &combined,
                    eff_wt,
                    "user",
                    "ephemeral_given_name",
                ));
                modified = true;
            }
        }
        if modified {
            self.sort_indices();
        }
    }

    /// 執行期增量熱重載（內建 2000ms 節流防護，僅針對 lexicon_domain.tsv 與 user_dict.tsv 動態感知新增或權重更新詞條）
    pub fn maybe_reload_dynamic_lexicons(&mut self) -> usize {
        if self.watched_files.is_empty() {
            return 0;
        }
        let now = std::time::Instant::now();
        if let Some(last_check) = self.last_check_instant {
            if now.duration_since(last_check).as_millis() < 2000 {
                return 0;
            }
        }
        self.last_check_instant = Some(now);

        let mut pending_updates: Vec<(usize, Option<std::time::SystemTime>, usize, Vec<LexiconEntry>)> = Vec::new();

        for (w_idx, wf) in self.watched_files.iter().enumerate() {
            if let Ok(meta) = std::fs::metadata(&wf.path) {
                let mtime = meta.modified().ok();
                let need_reload = match (wf.last_modified, mtime) {
                    (Some(last), Some(curr)) => curr > last,
                    (None, Some(_)) => true,
                    _ => false,
                };
                if need_reload {
                    let is_user_dict = wf.path.file_name().and_then(|n| n.to_str()) == Some("user_dict.tsv");
                    if is_user_dict {
                        LexiconEntry::backfill_user_dict_headcodes(&wf.path);
                    }
                    if let Ok(file) = File::open(&wf.path) {
                        let reader = BufReader::new(file);
                        let mut file_entries = Vec::new();
                        let mut line_count = 0;
                        let default_cat = wf.category.as_str();
                        for (idx, line_res) in reader.lines().enumerate() {
                            line_count = idx + 1;
                            if let Ok(line) = line_res {
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
                                    let norm_bpm = LexiconEntry::normalize_bpm(bpm);
                                    let weight: u32 = parts.get(2).and_then(|w| w.parse().ok()).unwrap_or(150);
                                    let raw_cat = parts.get(3).map(|s| s.trim()).unwrap_or("");
                                    let cat = if raw_cat.is_empty() {
                                        if default_cat == "user" {
                                            "user"
                                        } else if word.chars().count() == 2 && weight < 90 && word != "諺文" {
                                            "moe_unattested2"
                                        } else {
                                            default_cat
                                        }
                                    } else {
                                        raw_cat
                                    };
                                    let note = parts.get(4).unwrap_or(&"").trim();

                                    let mut entry = LexiconEntry::new(word, &norm_bpm, weight, cat, note);
                                    if cat == "user" || default_cat == "user" {
                                        let (_, abbrevs) = LexiconEntry::parse_user_dict_extra_columns(&parts, &LexiconEntry::extract_bpm_headcode(&norm_bpm));
                                        entry.category = "user".to_string();
                                        entry.custom_abbrevs = abbrevs;
                                        entry.is_user_word = true;
                                        file_entries.push(entry);
                                        let (sub_entries, _) = Self::decompose_entity_subentries(word, &norm_bpm, weight);
                                        for mut sub in sub_entries {
                                            sub.category = "user".to_string();
                                            sub.is_user_word = true;
                                            file_entries.push(sub);
                                        }
                                    } else {
                                        file_entries.push(entry);
                                    }
                                }
                            }
                        }
                        pending_updates.push((w_idx, mtime, line_count, file_entries));
                    }
                }
            }
        }

        if pending_updates.is_empty() {
            return 0;
        }

        let mut added_or_updated = 0;
        for (w_idx, mtime, line_count, file_entries) in pending_updates {
            self.watched_files[w_idx].last_modified = mtime;
            self.watched_files[w_idx].last_line_count = line_count;

            let is_user_dict = self.watched_files[w_idx].path.file_name().and_then(|n| n.to_str()) == Some("user_dict.tsv");
            if is_user_dict {
                self.user_abbrev_index.clear();
                self.user_word_indices.clear();
                for e in &mut self.entries {
                    if e.category == "user" {
                        e.is_user_word = false;
                        e.custom_abbrevs.clear();
                    }
                }
            }

            for mut entry in file_entries {
                if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
                    continue;
                }
                let norm_bpm = LexiconEntry::normalize_bpm(&entry.bopomofo);
                if entry.note == "auto_given_name" || entry.note == "ephemeral_given_name" {
                    let mut max_homophone_wt = 0u32;
                    if let Some(indices) = self.bopomofo_index.get(&norm_bpm) {
                        for &idx in indices {
                            let e = &self.entries[idx];
                            if e.word.chars().count() == 2 && e.weight > max_homophone_wt {
                                max_homophone_wt = e.weight;
                            }
                        }
                    }
                    entry.weight = entry.weight.max(1150).max(max_homophone_wt.saturating_add(100)).min(1800);
                }
                let mut existing_idx = None;
                if let Some(indices) = self.word_index.get(&entry.word) {
                    for &idx in indices {
                        let e_bpm = &self.entries[idx].bopomofo;
                        if e_bpm == &entry.bopomofo || LexiconEntry::normalize_bpm(e_bpm) == norm_bpm {
                            existing_idx = Some(idx);
                            break;
                        }
                    }
                }
                if let Some(idx) = existing_idx {
                    if self.entries[idx].weight != entry.weight {
                        self.entries[idx].weight = entry.weight;
                        added_or_updated += 1;
                    }
                    if entry.category == "user" {
                        self.entries[idx].category = "user".to_string();
                        self.entries[idx].is_user_word = true;
                        for a in entry.custom_abbrevs {
                            if !self.entries[idx].custom_abbrevs.contains(&a) {
                                self.entries[idx].custom_abbrevs.push(a);
                            }
                        }
                        self.register_user_entry_indices(idx);
                        added_or_updated += 1;
                    }
                } else {
                    self.add_entry(entry);
                    added_or_updated += 1;
                }
            }
        }

        if added_or_updated > 0 {
            self.sort_indices();
        }
        added_or_updated
    }

fn is_archaic_polyphone(word: &str, bpm: &str) -> bool {
    let clean_bpm = bpm.replace('˙', "");
    matches!(
        (word, clean_bpm.as_str()),
        ("尾", "ㄧˇ")
            | ("會", "ㄏㄨㄟˇ")
            | ("𣇫", _)
            | ("篼", _)
            | ("荊婦", _)
            | ("時後", _)
            | ("字擠", _)
            | ("慧覺", _)
            | ("姻黨", _)
            | ("內外姻黨", _)
            | ("赭衣", _)
            | ("繡衣", _)
            | ("瘐死", _)
            | ("劈刺", _)
            | ("家祠", _)
            | ("常語", _)
            | ("彈雨", _)
            | ("旦會", _)
            | ("試晬", _)
            | ("啖蔗", _)
            | ("市語", _)
            | ("傴兜", _)
            | ("鵲語", _)
            | ("确瘠", _)
            | ("在疚", _)
            | ("不擬", _)
            | ("野狠", _)
            | ("野語", _)
            | ("卻纔", _)
            | ("版型與", _)
            | ("批次間", _)
            | ("查至關重", _)
            | ("域比較", _)
            | ("化與", _)
            | ("於梁柱間", _)
            | ("入以", _)
            | ("度與", _)
            | ("美呈現其", _)
            | ("將這", _)
            | ("此部份", _)
            | ("的數位", _)
            | ("構與模", _)
            | ("開駛", _)
            | ("簡釋", _)
            | ("衣胞", _)
            | ("遺業", _)
            | ("縊死", _)
            | ("熠燿", _)
            | ("擬去", _)
            | ("裳幃", _)
            | ("有服", _)
            | ("箭石", _)
            | ("荐食", _)
            | ("踐石", _)
            | ("見示", _)
            | ("賤事", _)
            | ("賤視", _)
            | ("賤室", _)
            | ("祇是", "ㄓ ㄕˋ")
            | ("法繪", _)
            | ("慈石", _)
            | ("丁丁", "ㄓㄥ ㄓㄥ")
            | ("大家", "ㄉㄚˋ ㄍㄨ")
            | ("公車", "ㄍㄨㄥ ㄐㄩ")
            | ("它們", "ㄊㄨㄛ ㄇㄣ")
            | ("她們", "ㄊㄨㄛ ㄇㄣ")
            | ("他們", "ㄊㄨㄛ ㄇㄣ")
            | ("傾家", "ㄎㄥ ㄐㄧㄚ")
            | ("啞啞", "ㄜˋ ㄜˋ")
            | ("張說", "ㄓㄤ ㄩㄝˋ")
            | ("宋朝", "ㄙㄨㄥˋ ㄓㄠ")
            | ("姊姊", "ㄗˇ ㄗ")
            | ("姊夫", "ㄗˇ ㄈㄨ")
            | ("家姊", "ㄐㄧㄚ ㄗˇ")
            | ("堂姊", "ㄊㄤˊ ㄗˇ")
            | ("上色", "ㄕㄤˋ ㄕㄞˇ")
            | ("咳血", "ㄎㄚˇ ㄒㄧㄝˇ")
            | ("咳痰", "ㄎㄚˇ ㄊㄢˊ")
            | ("噁心", "ㄨˋ ㄒㄧㄣ")
            | ("底", "ㄉㄜ")
            | ("底", "˙ㄉㄜ")
            | ("底", "ㄉㄜ˙")
            | ("比", "ㄆㄧˊ")
            | ("比", "ㄅㄧˋ")
            | ("吃", "ㄐㄧˊ")
            | ("騎", "ㄐㄧˋ")
            | ("杷", "ㄅㄚ")
            | ("中的", "ㄓㄨㄥˋ ㄉㄧˋ")
            | ("的當", "ㄉㄧˊ ㄉㄤˋ")
            | ("的見", "ㄉㄧˊ ㄐㄧㄢˋ")
            | ("的知", "ㄉㄧˊ ㄓ")
            | ("的信", "ㄉㄧˊ ㄒㄧㄣˋ")
            | ("的實", "ㄉㄧˊ ㄕˊ")
            | ("的歷", "ㄉㄧˋ ㄌㄧˋ")
            | ("的決", "ㄉㄧˊ ㄐㄩㄝˊ")
            | ("的是", "ㄉㄧˊ ㄕˋ")
            | ("的的", "ㄉㄧˊ ㄉㄧˊ")
            | ("的耗", "ㄉㄧˊ ㄏㄠˋ")
            | ("的些", "ㄉㄧˋ ㄒㄧㄝ")
            | ("的然", "ㄉㄧˊ ㄖㄢˊ")
            | ("會得", _)
            | ("到得", _)
            | ("乘得", "ㄔㄥˊ ㄉㄜ˙")
            | ("嶳", _)
            | ("㡳", _)
            | ("埅", _)
            | ("埊", _)
            | ("㑘", _)
            | ("価", _)
            | ("有庳", _)
            | ("汙庳", _)
            | ("庳", _)
            | ("闞闞", _)
            | ("宅", "ㄓㄜˋ")
            | ("宅到", _)
            | ("暴", "ㄆㄨˋ")
            | ("喔", "ㄨㄛˋ")
            | ("雨", "ㄩˋ")
            | ("渴", "ㄏㄜˊ")
            | ("渴", "ㄐㄧㄝˊ")
            | ("㳀", _)
            | ("𩟂", _)
            | ("它", "ㄊㄨㄛ")
            | ("它", "ㄊㄨㄛˊ")
            | ("她", "ㄊㄨㄛ")
            | ("他", "ㄊㄨㄛ")
            | ("啖噬", _)
            | ("見背", _)
            | ("鑒核", _)
            | ("暗黝", _)
            | ("危語", _)
            | ("為禍", _)
            | ("狠到", _)
            | ("狠得", _)
            | ("蟻象", _)
            | ("諛詞", _)
            | ("諛辭", _)
            | ("餘步", _)
            | ("就於", _)
            | ("于于", _)
            | ("于思", _)
            | ("互於", _)
            | ("不托", _)
            | ("付著", _)
            | ("亦就", _)
            | ("什件", _)
            | ("伴食", _)
            | ("令友", _)
            | ("乞和", _)
            | ("乃能", _)
            | ("乖違", _)
            | ("踐位", _)
            | ("畏憚", _)
            | ("核備", _)
            | ("霉潮", _)
            | ("魚道", _)
            | ("倒裳", _)
            | ("帷裳", _)
            | ("慈幃", _)
            | ("支骸", _)
            | ("於何", _)
            | ("備位", _)
            | ("兜的", _)
            | ("可著", _)
            | ("可罪", _)
            | ("啖食", _)
            | ("噬臍", _)
            | ("嚮晦", _)
            | ("帷扆", _)
            | ("盜竽", _)
            | ("枝辭", _)
            | ("不仕", _)
            | ("不字", _)
            | ("事從", _)
            | ("以時", _)
            | ("佈讓", _)
    )
}

    pub fn add_entry(&mut self, mut entry: LexiconEntry) {
        if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
            return;
        }

        entry.bopomofo = LexiconEntry::normalize_bpm(&entry.bopomofo);
        let char_count = entry.word.chars().count();
        let is_pure_cjk = char_count >= 2
            && entry
                .word
                .chars()
                .all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        if is_pure_cjk && char_count != entry.bopomofo.split_whitespace().count() {
            // 過濾字數與音節數不符之兒化合音異常詞條（如 3 字詞僅標 2 音節）
            return;
        }

        let norm_bpm = entry.bopomofo.clone();
        let sandhi_equivs = LexiconEntry::sandhi_equivalence_readings(&entry.word, &norm_bpm);
        let incoming_cit = LexiconEntry::citation_sandhi_bpm(&entry.word, &norm_bpm);
        let incoming_is_moe = matches!(
            entry.category.as_str(),
            "common"
                | "modern"
                | "classic"
                | "archaic"
                | "rare"
                | "saying"
                | "twister"
                | "word2"
                | "word3"
                | "word4"
                | "moe"
                | "moe_unattested2"
                | "idiom"
        );

        // 去重合併：若同一 (詞彙, 正規化注音或本調變調對應注音) 已存在，直接於原節點更新權重，杜絕重複產生 LatticeNode 拖慢 Viterbi
        let mut matched_existing_idx: Option<usize> = None;
        let mut has_mcb_other_reading = false;
        if let Some(indices) = self.word_index.get(&entry.word) {
            for &existing_idx in indices {
                let existing = &self.entries[existing_idx];
                if existing.category.starts_with("mcb") {
                    has_mcb_other_reading = true;
                }
                let existing_norm = LexiconEntry::normalize_bpm(&existing.bopomofo);
                let existing_cit = LexiconEntry::citation_sandhi_bpm(&existing.word, &existing_norm);
                let bpm_matches = existing.bopomofo == entry.bopomofo
                    || existing_norm == norm_bpm
                    || sandhi_equivs.contains(&existing_norm)
                    || existing_cit.as_deref() == Some(norm_bpm.as_str())
                    || incoming_cit.as_deref() == Some(existing_norm.as_str())
                    || (existing_cit.is_some() && existing_cit == incoming_cit);
                if bpm_matches {
                    matched_existing_idx = Some(existing_idx);
                    break;
                }
            }
        }

        if let Some(existing_idx) = matched_existing_idx {
            let existing_is_mcb = self.entries[existing_idx].category.starts_with("mcb");
            // 若已有小麥真實語料庫權重（無論單字或複詞），均不被萌典合成齊頭權重（如 word2=420, core=800, char）覆蓋
            if !(existing_is_mcb && incoming_is_moe) {
                if entry.weight > self.entries[existing_idx].weight {
                    self.entries[existing_idx].weight = entry.weight;
                }
            }
            if !incoming_is_moe && self.entries[existing_idx].category == "moe_unattested2" {
                self.entries[existing_idx].category = entry.category.clone();
            }
            // 確保所有變調等價讀音皆能索引至同一節點（如 ㄅㄨˋ ㄗㄞˋ 與 ㄅㄨˊ ㄗㄞˋ 共享「不再 591」）
            for r in &sandhi_equivs {
                let list = self.bopomofo_index.entry(r.clone()).or_default();
                if !list.contains(&existing_idx) {
                    list.push(existing_idx);
                }
            }
            let list = self.bopomofo_index.entry(norm_bpm.clone()).or_default();
            if !list.contains(&existing_idx) {
                list.push(existing_idx);
            }
            if entry.bopomofo != norm_bpm {
                let raw_list = self.bopomofo_index.entry(entry.bopomofo.clone()).or_default();
                if !raw_list.contains(&existing_idx) {
                    raw_list.push(existing_idx);
                }
            }
            if let Some(cit_bpm) = incoming_cit {
                let cit_list = self.bopomofo_index.entry(cit_bpm).or_default();
                if !cit_list.contains(&existing_idx) {
                    cit_list.push(existing_idx);
                }
            }
            if entry.category == "user" {
                self.entries[existing_idx].category = "user".to_string();
                self.entries[existing_idx].is_user_word = true;
                for a in entry.custom_abbrevs {
                    if !self.entries[existing_idx].custom_abbrevs.contains(&a) {
                        self.entries[existing_idx].custom_abbrevs.push(a);
                    }
                }
                self.register_user_entry_indices(existing_idx);
            }
            return;
        }

        // 若萌典複詞因字面與小麥同形而誤獲 420 齊頭權重，但讀音實為小麥未收錄之生僻古音/又音，自動降回冷僻詞基礎權重 80
        if incoming_is_moe && has_mcb_other_reading && char_count >= 2 && entry.weight >= 400 {
            entry.weight = 80;
        }

        // 標記僅存在於萌典（小麥現代語料庫未收錄該詞或該讀音）且權重 <= 180 之無現代語料佐證古語/生僻雙字詞，
        // 或 weight < 90 之雙字保留詞（在三欄位詞庫中專供古典保留詞使用），
        // 供 LatticeDAG 於連續長句（n >= 3）進行跨詞界劫掠免疫防護，同時 100% 保留單詞輸入（n == 2）與 ↑/↓ 選字列可達性
        if ((incoming_is_moe && entry.weight <= 180) || entry.weight < 90)
            && char_count == 2
            && !matches!(entry.word.as_str(), "參雜" | "耆德" | "衚衕" | "諺文")
        {
            entry.category = "moe_unattested2".to_string();
        }

        let idx = self.entries.len();
        let notone = LexiconEntry::strip_tones(&entry.bopomofo);
        let headcode = entry.headcode.clone();

        self.word_index.entry(entry.word.clone()).or_default().push(idx);
        for r in &sandhi_equivs {
            let list = self.bopomofo_index.entry(r.clone()).or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        }
        let raw_list = self.bopomofo_index.entry(entry.bopomofo.clone()).or_default();
        if !raw_list.contains(&idx) {
            raw_list.push(idx);
        }
        if norm_bpm != entry.bopomofo {
            let list = self.bopomofo_index.entry(norm_bpm).or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        }
        if let Some(cit_bpm) = incoming_cit {
            let cit_list = self.bopomofo_index.entry(cit_bpm).or_default();
            if !cit_list.contains(&idx) {
                cit_list.push(idx);
            }
        }
        self.notone_index.entry(notone).or_default().push(idx);
        if !headcode.is_empty() {
            self.headcode_index.entry(headcode).or_default().push(idx);
        }
        if char_count >= 3 {
            let bpm_hc = LexiconEntry::extract_bpm_headcode(&entry.bopomofo);
            if bpm_hc.chars().count() >= 3 {
                self.headcode_index.entry(bpm_hc).or_default().push(idx);
            }
        }

        self.entries.push(entry);
        if self.entries[idx].category == "user" {
            self.register_user_entry_indices(idx);
        }
    }

    pub fn sort_indices(&mut self) {
        let entries = &self.entries;
        for indices in self.word_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
            indices.dedup();
        }
        for indices in self.bopomofo_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
            indices.dedup();
        }
        for indices in self.notone_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
            indices.dedup();
        }
        for indices in self.headcode_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
            indices.dedup();
        }
        for indices in self.user_abbrev_index.values_mut() {
            indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
            indices.dedup();
        }
        self.user_word_indices.sort_by(|&a, &b| entries[b].weight.cmp(&entries[a].weight));
        self.user_word_indices.dedup();
    }

    /// 將記憶體中已去重、清洗並完成語體分級之核心詞庫，無損匯出為單一統整詞庫 `lexicon_core.tsv`
    pub fn export_unified_core_tsv<P: AsRef<Path>>(&self, path: P) -> Result<usize, std::io::Error> {
        use std::io::{BufWriter, Write};
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "# RuhI (如喜) 通用智慧型輸入法 — 單一統整核心詞庫 (Unified Core Lexicon)")?;
        writeln!(writer, "# 欄位格式: WORD\tBOPOMOFO\tWEIGHT")?;
        writeln!(writer, "# 致敬與開源授權聲明請參閱專案根目錄 THIRD_PARTY_LICENSES.md")?;

        let mut written_lines = 0;
        for e in &self.entries {
            writeln!(writer, "{}\t{}\t{}", e.word, e.bopomofo, e.weight)?;
            written_lines += 1;
        }
        writer.flush()?;
        Ok(written_lines)
    }

    pub fn lookup_bopomofo(&self, bpm: &str, exact_tone: bool) -> Vec<&LexiconEntry> {
        if exact_tone {
            if let Some(indices) = self.bopomofo_index.get(bpm) {
                return indices.iter().map(|&i| &self.entries[i]).collect();
            }
            let norm = LexiconEntry::normalize_bpm(bpm);
            if norm != bpm {
                if let Some(indices) = self.bopomofo_index.get(&norm) {
                    return indices.iter().map(|&i| &self.entries[i]).collect();
                }
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
        let key = if code.chars().all(|c| c.is_ascii()) {
            code.to_ascii_lowercase()
        } else {
            code.replace(' ', "")
        };
        if let Some(indices) = self.headcode_index.get(&key) {
            return indices.iter().map(|&i| &self.entries[i]).collect();
        }
        Vec::new()
    }

    /// 3 字詞以上（`>= 3`）多模式注音頭碼智慧查詢（支援許氏雙拼、標準大千、倚天 26 鍵及直接注音符號序列）
    /// 嚴格過濾 2 字以下詞彙，防止頭碼碰撞污染日常雙字／單字輸入，並結合前驅詞轉移權重排序
    pub fn lookup_multi_char_headcode(
        &self,
        query: &str,
        keyboard_mode: &str,
        prev_context: Option<&str>,
        mut transitions: Option<&mut crate::router::transitions::TransitionEngine>,
    ) -> Vec<String> {
        let trimmed: String = query.chars().filter(|c| !c.is_whitespace()).collect();
        let query_len = trimmed.chars().count();
        if query_len < 3 {
            return Vec::new();
        }

        // 建立候選查詢鍵集合：
        // 1. 若輸入本身為注音符號序列（如「ㄓㄒㄉㄌ」、「ㄊㄅㄔㄓ」、「ㄉㄖㄓ」），直接查詢注音首碼與對應許氏首碼
        // 2. 若為標準大千／倚天 26 鍵之按鍵序列（如 "5v2x"、"w1t5"、"jxdl"），優先轉為注音首碼查詢，再輔以許氏首碼
        // 3. 若為許氏雙拼按鍵序列（如 "jcdl"、"tbcj"、"llxf"、"ecln"、"drj"），直接查詢許氏首碼
        let mut search_keys: Vec<String> = Vec::new();
        if trimmed.chars().all(|c| ('\u{3105}'..='\u{3129}').contains(&c)) {
            search_keys.push(trimmed.clone());
            let spaced: String = trimmed.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
            let hsu_hc = LexiconEntry::extract_headcode(&spaced);
            if hsu_hc.chars().count() == query_len && !search_keys.contains(&hsu_hc) {
                search_keys.push(hsu_hc);
            }
        } else {
            if let Some(bpm_hc) = LexiconEntry::convert_keys_to_bpm_headcode(&trimmed, keyboard_mode) {
                if bpm_hc.chars().count() == query_len {
                    search_keys.push(bpm_hc);
                }
            }
            if trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
                let lower = trimmed.to_ascii_lowercase();
                if !search_keys.contains(&lower) {
                    search_keys.push(lower);
                }
            }
        }

        if search_keys.is_empty() {
            return Vec::new();
        }

        let prev_w = prev_context.unwrap_or("").trim();
        let mut scored: Vec<(String, f64)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for key in &search_keys {
            if let Some(indices) = self.headcode_index.get(key) {
                for &idx in indices {
                    let entry = &self.entries[idx];
                    let w_len = entry.word.chars().count();
                    if w_len < 3 || w_len != query_len {
                        continue;
                    }
                    if seen.insert(entry.word.clone()) {
                        let mut score = entry.weight as f64;
                        if !prev_w.is_empty() {
                            if let Some(ref mut trans) = transitions {
                                score += trans.calculate_transition_bonus(prev_w, "", &entry.word, "") * 15.0;
                            }
                        }
                        scored.push((entry.word.clone(), score));
                    }
                }
            }
        }

        // 若精確字數（例如 3 碼查 3 字詞）結果較少，允許補充 4~5 字專有名詞之前綴匹配
        if scored.len() < 5 && query_len >= 3 {
            for (hc_key, indices) in &self.headcode_index {
                let hc_len = hc_key.chars().count();
                if hc_len > query_len && hc_len <= query_len + 2 {
                    if search_keys.iter().any(|k| hc_key.starts_with(k)) {
                        for &idx in indices.iter().take(4) {
                            let entry = &self.entries[idx];
                            let w_len = entry.word.chars().count();
                            if w_len >= 4 && w_len == hc_len && entry.weight >= 450 && seen.insert(entry.word.clone()) {
                                let score = (entry.weight as f64) * 0.75;
                                scored.push((entry.word.clone(), score));
                            }
                        }
                    }
                }
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(27).map(|(w, _)| w).collect()
    }

    /// 自訂詞庫縮略輸入與注音頭碼智慧查詢
    pub fn lookup_user_abbrev_candidates(
        &self,
        query: &str,
        keyboard_mode: &str,
        prev_context: Option<&str>,
        mut transitions: Option<&mut crate::router::transitions::TransitionEngine>,
    ) -> Vec<String> {
        let q = query.trim();
        let prev_w = prev_context.unwrap_or("").trim();

        if q.is_empty() {
            let mut cands: Vec<(String, u32)> = Vec::new();
            for &idx in &self.user_word_indices {
                if idx < self.entries.len() {
                    let entry = &self.entries[idx];
                    if entry.note == "ephemeral_given_name" {
                        continue;
                    }
                    if !cands.iter().any(|(w, _)| w == &entry.word) {
                        cands.push((entry.word.clone(), entry.weight));
                    }
                }
            }
            cands.sort_by(|a, b| b.1.cmp(&a.1));
            return cands.into_iter().take(200).map(|(w, _)| w).collect();
        }

        let q_lower = q.to_ascii_lowercase();
        let q_bpm = LexiconEntry::raw_query_to_bpm_headcode(q, keyboard_mode);

        let mut scored: Vec<(u8, i32, String)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for &idx in &self.user_word_indices {
            if idx >= self.entries.len() {
                continue;
            }
            let entry = &self.entries[idx];
            if entry.note == "ephemeral_given_name" {
                continue;
            }
            let entry_bpm_hc = LexiconEntry::extract_bpm_headcode(&entry.bopomofo);
            let entry_hsu_hc = entry.headcode.to_ascii_lowercase();

            let mut ctx_bonus = 0i32;
            if !prev_w.is_empty() {
                if let Some(ref mut trans) = transitions {
                    ctx_bonus = (trans.calculate_transition_bonus(prev_w, "", &entry.word, "") * 8.0).round() as i32;
                }
            }

            // Tier 1: Exact Custom Abbreviation
            if entry.custom_abbrevs.iter().any(|a| a.eq_ignore_ascii_case(q) || a == q) {
                let score = (entry.weight as i32) + 2000 + ctx_bonus;
                scored.push((1, score, entry.word.clone()));
                seen.insert(entry.word.clone());
            }
            // Tier 2: Exact Bopomofo Headcode in current keyboard layout or direct Bopomofo, or Hsu headcode
            else if entry_bpm_hc == q || q_bpm.as_deref() == Some(entry_bpm_hc.as_str()) || (matches!(keyboard_mode, "hsu" | "") && entry_hsu_hc == q_lower) || entry_hsu_hc == q_lower {
                let layout_bonus = if entry_bpm_hc == q || q_bpm.as_deref() == Some(entry_bpm_hc.as_str()) || (matches!(keyboard_mode, "hsu" | "") && entry_hsu_hc == q_lower) {
                    150
                } else {
                    0
                };
                let score = (entry.weight as i32) + 1200 + layout_bonus + ctx_bonus;
                scored.push((2, score, entry.word.clone()));
                seen.insert(entry.word.clone());
            }
            // Tier 3: Prefix of Custom Abbreviation
            else if let Some(min_len) = entry.custom_abbrevs.iter().filter_map(|a| {
                let al = a.to_ascii_lowercase();
                if al.starts_with(&q_lower) || a.starts_with(q) {
                    Some(a.chars().count())
                } else {
                    None
                }
            }).min() {
                let len_diff = min_len.saturating_sub(q.chars().count()) as i32;
                let score = (entry.weight as i32) + 600 - (len_diff * 20) + ctx_bonus;
                scored.push((3, score, entry.word.clone()));
                seen.insert(entry.word.clone());
            }
            // Tier 4: Prefix of Bopomofo / Hsu Headcode
            else if entry_bpm_hc.starts_with(q) || q_bpm.as_deref().map(|qb| entry_bpm_hc.starts_with(qb)).unwrap_or(false) || entry_hsu_hc.starts_with(&q_lower) {
                let target_len = if entry_bpm_hc.starts_with(q) || q_bpm.as_deref().map(|qb| entry_bpm_hc.starts_with(qb)).unwrap_or(false) {
                    entry_bpm_hc.chars().count()
                } else {
                    entry_hsu_hc.chars().count()
                };
                let len_diff = target_len.saturating_sub(q.chars().count()) as i32;
                let score = (entry.weight as i32) + 300 - (len_diff * 20) + ctx_bonus;
                scored.push((4, score, entry.word.clone()));
                seen.insert(entry.word.clone());
            }
        }

        // 排序：tier ASC (1 > 2 > 3 > 4), score DESC
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));

        let mut out: Vec<String> = Vec::new();
        for (_, _, w) in scored {
            if !out.contains(&w) {
                out.push(w);
            }
        }

        // Tier 5: 3 字以上詞彙且結果不足時，輔以核心詞庫之 3+ 字頭碼查詢結果
        if q.chars().count() >= 3 && out.len() < 20 {
            let core_cands = self.lookup_multi_char_headcode(&q_lower, keyboard_mode, prev_context, transitions.as_deref_mut());
            for cw in core_cands {
                if !out.contains(&cw) {
                    out.push(cw);
                    if out.len() >= 27 {
                        break;
                    }
                }
            }
        }

        out
    }

    /// 產生自訂詞條之頭碼與縮略語標籤（供候選窗教練提示使用）
    pub fn format_user_word_abbrev_badge(&self, word: &str) -> Option<String> {
        for &idx in &self.user_word_indices {
            if idx < self.entries.len() {
                let entry = &self.entries[idx];
                if entry.word == word {
                    let bpm_hc = LexiconEntry::extract_bpm_headcode(&entry.bopomofo);
                    let abbrev_str = if entry.custom_abbrevs.is_empty() {
                        String::new()
                    } else {
                        format!(" ｜ 縮略碼: {}", entry.custom_abbrevs.join(", "))
                    };
                    return Some(format!("頭碼: {}{}", bpm_hc, abbrev_str));
                }
            }
        }
        None
    }

    pub fn has_word(&self, word: &str) -> bool {
        self.word_index.contains_key(word)
    }

    /// 取得許氏鍵盤同一按鍵所映射之全套獨立注音符號（供單獨輸出 `ㄐ`, `ㄑ`, `ㄒ`, `ㄍ`, `ㄌ`, `ㄏ`, `ㄇ`, `ㄋ` 等注音符號使用）
    pub fn get_hsu_bopomofo_symbols(bpm_or_ch: &str) -> Vec<String> {
        let trimmed = bpm_or_ch.trim();
        let group: &[&str] = match trimmed {
            "ㄓ" | "ㄐ" => &["ㄓ", "ㄐ", "ˋ"],
            "ㄔ" | "ㄑ" => &["ㄔ", "ㄑ"],
            "ㄕ" | "ㄒ" => &["ㄕ", "ㄒ"],
            "ㄜ" | "ㄍ" => &["ㄜ", "ㄍ", "ㄐ"],
            "ㄦ" | "ㄌ" | "ㄥ" => &["ㄦ", "ㄌ", "ㄥ", "ㄤ"],
            "ㄛ" | "ㄏ" => &["ㄛ", "ㄏ"],
            "ㄢ" | "ㄇ" => &["ㄢ", "ㄇ"],
            "ㄣ" | "ㄋ" => &["ㄣ", "ㄋ"],
            "ㄎ" | "ㄤ" => &["ㄎ", "ㄤ"],
            "ㄘ" | "ㄟ" => &["ㄘ", "ㄟ"],
            "ㄧ" | "ㄝ" => &["ㄧ", "ㄝ"],
            "ㄚ" | "ㄖ" => &["ㄚ", "ㄖ"],
            "ㄨ" => &["ㄨ", "ㄒ", "ㄠ"],
            "ㄩ" | "ㄙ" => &["ㄩ", "ㄙ"],
            "ㄠ" => &["ㄠ", "ㄨ"],
            "ㄡ" | "ㄈ" => &["ㄡ", "ㄈ"],
            "ㄞ" | "ㄉ" => &["ㄞ", "ㄉ", "ˊ"],
            "ㄅ" => &["ㄅ"],
            "ㄆ" => &["ㄆ"],
            "ㄊ" => &["ㄊ"],
            "ㄗ" => &["ㄗ"],
            _ => &[],
        };
        if group.is_empty() {
            if trimmed.chars().count() == 1 {
                let c = trimmed.chars().next().unwrap();
                if ('\u{3105}'..='\u{3129}').contains(&c) || matches!(c, 'ˊ' | 'ˇ' | 'ˋ' | '˙') {
                    return vec![trimmed.to_string()];
                }
            }
            return Vec::new();
        }
        let mut out = Vec::with_capacity(group.len());
        out.push(trimmed.to_string());
        for &s in group {
            if s != trimmed {
                out.push(s.to_string());
            }
        }
        out
    }

    /// 完整中文人名（姓＋名）與公司行號全稱（主要名稱＋固定模式附加名）智慧拆分器
    /// 回傳：(拆分子詞條清單, 建議二元轉移清單 (prev, curr, bonus))
    pub fn decompose_entity_subentries(
        word: &str,
        bpm: &str,
        weight: u32,
    ) -> (Vec<LexiconEntry>, Vec<(String, String, f64)>) {
        let chars: Vec<char> = word.chars().collect();
        let syls: Vec<&str> = bpm.split_whitespace().collect();
        if chars.len() != syls.len() || chars.len() < 3 {
            return (Vec::new(), Vec::new());
        }

        let mut sub_entries = Vec::new();
        let mut sub_transitions = Vec::new();
        let sub_weight = weight.max(920);

        // 1. 公司行號與組織全稱拆分（如「如喜科技有限公司」->「如喜科技」、「如喜」；「大十數位股份有限公司」->「大十數位」、「大十」）
        const LEGAL_SUFFIXES: &[&str] = &[
            "股份有限公司",
            "醫療財團法人",
            "社團法人",
            "財團法人",
            "有限合夥",
            "有限公司",
            "責任公司",
            "股份公司",
            "企業社",
            "工作室",
            "事務所",
            "基金會",
            "研究院",
            "實驗室",
        ];
        const INDUSTRY_MODIFIERS: &[&str] = &[
            "半導體", "科技", "數位", "資訊", "國際", "實業", "企業", "文創", "生技", "電子",
            "電機", "光電", "網路", "軟體", "精密", "工業", "貿易", "開發", "建設", "營造",
            "設計", "顧問", "管理", "投資", "控股", "金融", "傳播", "媒體", "文化", "出版",
            "教育", "醫療", "醫藥", "生物", "食品", "餐飲", "能源", "環保", "材料", "化學",
            "機械", "智慧", "智能", "法律", "會計", "建築", "物流", "航運", "航空", "電信",
            "通訊", "光學", "農業", "海洋", "資產", "創投", "娛樂", "影視", "遊戲", "動漫",
            "藝術", "音樂",
        ];

        for &legal_sfx in LEGAL_SUFFIXES {
            if word.ends_with(legal_sfx) {
                let sfx_len = legal_sfx.chars().count();
                if chars.len() > sfx_len && chars.len() - sfx_len >= 2 {
                    let stem_len = chars.len() - sfx_len;
                    let stem_word: String = chars[0..stem_len].iter().collect();
                    let stem_bpm = syls[0..stem_len].join(" ");
                    sub_entries.push(LexiconEntry::new(
                        &stem_word,
                        &stem_bpm,
                        sub_weight,
                        "user",
                        "auto_company_stem",
                    ));
                    sub_transitions.push((stem_word.clone(), legal_sfx.to_string(), 28.0));

                    // 若 stem_word 尾端帶有產業修飾詞（如「如喜科技」、「大十數位」），進一步拆出核心字號（「如喜」、「大十」）
                    for &ind_mod in INDUSTRY_MODIFIERS {
                        if stem_word.ends_with(ind_mod) {
                            let mod_len = ind_mod.chars().count();
                            if stem_len > mod_len && stem_len - mod_len >= 2 {
                                let core_len = stem_len - mod_len;
                                let core_brand: String = chars[0..core_len].iter().collect();
                                let core_bpm = syls[0..core_len].join(" ");
                                sub_entries.push(LexiconEntry::new(
                                    &core_brand,
                                    &core_bpm,
                                    sub_weight,
                                    "user",
                                    "auto_company_brand",
                                ));
                                sub_transitions.push((core_brand, ind_mod.to_string(), 28.0));
                                sub_transitions.push((ind_mod.to_string(), legal_sfx.to_string(), 26.0));
                            }
                            break;
                        }
                    }
                    return (sub_entries, sub_transitions);
                }
            }
        }

        // 2. 四字複姓與雙姓／冠夫姓中文人名拆分（如「歐陽娜娜」-> 姓「歐陽」+ 名「娜娜」；「陳吳玟蓉」-> 姓「陳吳」+ 名「玟蓉」）
        const COMPOUND_SURNAMES: &[&str] = &[
            "歐陽", "司馬", "上官", "諸葛", "司徒", "司空", "獨孤", "慕容", "皇甫", "尉遲",
            "公孫", "長孫", "宇文", "軒轅", "令狐", "鍾離", "端木", "東方", "西門", "南宮",
            "百里", "東郭", "南門", "呼延", "羊舌", "微生", "范姜", "張簡",
        ];
        // 台灣常見與百家姓 350+ 單姓
        const SINGLE_SURNAMES: &str = "陳林黃張李王吳劉蔡楊許鄭謝郭洪曾邱廖賴周徐蘇葉莊呂江何蕭羅高簡朱鍾施游詹沈彭胡余盧潘顏梁趙柯翁魏方孫戴范宋鄧杜侯曹薛傅丁溫紀蔣歐藍唐馬董石卓程姚康馮古姜湯汪白田涂鄒巫尤龔嚴韓袁金童陸夏柳邵錢莫秦賈韋萬龐樊伍倪祝葛苗熊崔譚黎龍顧孟雷賀殷畢常于齊尹成屈項席駱凌霍虞管房單包裴榮富薄蒲蒙喬聞牛燕浦柴閻艾容易戈耿文寇關查荊權蓋辜饒利闕花官曲宮甯商阮戚柏章雲費岑魯鄔鮑史廉滕郝安樂時皮卞湛祁毛禹狄米貝明臧計伏談茅舒閔麻強路婁危梅盛支經裘繆干賁杭諸左吉鈕嵇邢滑荀羊於惠甄麴封芮羿儲靳汲邴糜松井段烏弓牧隗山宓蓬郗班仰秋仲伊仇欒暴钭厲戎祖武符景束幸司韶郜薊印宿懷邰從鄂索咸籍藺屠陰鬱胥蒼雙莘黨翟貢勞逄扶堵冉宰酈雍卻璩桑桂濮壽通扈冀郟尚農別晏瞿充慕茹習宦魚慎庾終暨居衡步滿弘匡國廣祿東殳沃蔚越夔隆師鞏厙聶晁勾敖融冷訾辛那空毋沙乜養鞠須豐巢蒯後紅竺逯益桓晉楚閆法汝鄢塗欽歸海岳帥緱亢況琴牟佘佴伯賞墨哈譙笪年愛陽佟池季向谷卜焦巴甘刁車申穆解應宗宣郁全邊姬昝";

        if chars.len() == 4 {
            let prefix2: String = chars[0..2].iter().collect();
            if COMPOUND_SURNAMES.contains(&prefix2.as_str()) {
                let surname_bpm = syls[0..2].join(" ");
                let given_name: String = chars[2..4].iter().collect();
                let given_bpm = syls[2..4].join(" ");
                sub_entries.push(LexiconEntry::new(
                    &prefix2,
                    &surname_bpm,
                    sub_weight,
                    "user",
                    "auto_compound_surname",
                ));
                sub_entries.push(LexiconEntry::new(
                    &given_name,
                    &given_bpm,
                    sub_weight,
                    "user",
                    "auto_given_name",
                ));
                sub_transitions.push((prefix2, given_name.clone(), 28.0));
                sub_transitions.push(("叫".to_string(), given_name.clone(), 20.0));
                sub_transitions.push(("找".to_string(), given_name.clone(), 18.0));
                sub_transitions.push(("我是".to_string(), given_name, 22.0));
                return (sub_entries, sub_transitions);
            } else if SINGLE_SURNAMES.contains(chars[0]) && SINGLE_SURNAMES.contains(chars[1]) {
                let prefix2_str = prefix2.clone();
                let last_char = chars[3];
                let is_non_name_prefix = matches!(
                    prefix2_str.as_str(),
                    "輸入" | "候選" | "使用" | "高解" | "不需" | "完全" | "自動" | "手動" | "預設"
                        | "系統" | "資料" | "記憶" | "處理" | "工作" | "管理" | "開發" | "測試"
                );
                let is_non_name_suffix = matches!(
                    last_char,
                    '框' | '鍵' | '欄' | '列' | '檔' | '頁' | '表' | '圖' | '庫' | '區' | '鈕' | '窗'
                        | '器' | '機' | '線' | '網' | '站' | '室' | '所' | '院' | '會' | '社' | '廠'
                        | '店' | '局' | '處' | '部' | '科' | '組' | '隊' | '團' | '學' | '論' | '法'
                        | '率' | '度' | '性' | '化' | '感' | '力' | '權' | '費' | '單' | '書' | '卡'
                        | '券' | '票' | '證' | '號' | '碼' | '體' | '類' | '型' | '式' | '態' | '版'
                        | '本' | '次' | '回' | '下' | '裡' | '內' | '外' | '中' | '上' | '前' | '後'
                        | '間' | '旁' | '邊' | '面' | '頭' | '尾' | '的' | '得' | '地' | '了' | '著'
                        | '過' | '們' | '者' | '員' | '師' | '家' | '生' | '長' | '物' | '品' | '質'
                        | '量' | '值' | '數' | '點' | '層' | '級' | '段' | '期' | '日' | '月' | '年'
                        | '時' | '分' | '秒'
                );
                if !is_non_name_prefix && !is_non_name_suffix {
                    let given_name: String = chars[2..4].iter().collect();
                    let given_bpm = syls[2..4].join(" ");
                    sub_entries.push(LexiconEntry::new(
                        &given_name,
                        &given_bpm,
                        sub_weight,
                        "user",
                        "auto_given_name",
                    ));
                    sub_transitions.push((prefix2_str, given_name.clone(), 28.0));
                    sub_transitions.push((chars[1].to_string(), given_name.clone(), 24.0));
                    sub_transitions.push(("叫".to_string(), given_name.clone(), 20.0));
                    sub_transitions.push(("找".to_string(), given_name.clone(), 18.0));
                    sub_transitions.push(("我是".to_string(), given_name, 22.0));
                    return (sub_entries, sub_transitions);
                }
            }
        }

        // 3. 三字單姓中文人名拆分（如「吳玟蓉」->「玟蓉」、「曹大十」->「大十」）
        if chars.len() == 3 && SINGLE_SURNAMES.contains(chars[0]) {
            let prefix2: String = chars[0..2].iter().collect();
            let last_char = chars[2];
            let is_non_name_prefix = matches!(
                prefix2.as_str(),
                "輸入" | "候選" | "使用" | "高解" | "不需" | "完全" | "自動" | "手動" | "預設"
                    | "系統" | "資料" | "記憶" | "處理" | "工作" | "管理" | "開發" | "測試"
            );
            let is_non_name_suffix = matches!(
                last_char,
                '框' | '鍵' | '欄' | '列' | '檔' | '頁' | '表' | '圖' | '庫' | '區' | '鈕' | '窗'
                    | '器' | '機' | '線' | '網' | '站' | '室' | '所' | '院' | '會' | '社' | '廠'
                    | '店' | '局' | '處' | '部' | '科' | '組' | '隊' | '團' | '學' | '論' | '法'
                    | '率' | '度' | '性' | '化' | '感' | '力' | '權' | '費' | '單' | '書' | '卡'
                    | '券' | '票' | '證' | '號' | '碼' | '體' | '類' | '型' | '式' | '態' | '版'
                    | '本' | '次' | '回' | '下' | '裡' | '內' | '外' | '中' | '上' | '前' | '後'
                    | '間' | '旁' | '邊' | '面' | '頭' | '尾' | '的' | '得' | '地' | '了' | '著'
                    | '過' | '們' | '者' | '員' | '師' | '家' | '生' | '長' | '物' | '品' | '質'
                    | '量' | '值' | '數' | '點' | '層' | '級' | '段' | '期' | '日' | '月' | '年'
                    | '時' | '分' | '秒'
            );
            if !is_non_name_prefix && !is_non_name_suffix {
                let surname = chars[0].to_string();
                let given_name: String = chars[1..3].iter().collect();
                let given_bpm = syls[1..3].join(" ");
                sub_entries.push(LexiconEntry::new(
                    &given_name,
                    &given_bpm,
                    sub_weight,
                    "user",
                    "auto_given_name",
                ));
                sub_transitions.push((surname, given_name.clone(), 28.0));
                sub_transitions.push(("叫".to_string(), given_name.clone(), 20.0));
                sub_transitions.push(("找".to_string(), given_name.clone(), 18.0));
                sub_transitions.push(("我是".to_string(), given_name, 22.0));
                return (sub_entries, sub_transitions);
            }
        }

        (sub_entries, sub_transitions)
    }

    /// 反查單一中文字的同音候選字 (Homophone candidate lookup for a single character)
    /// 嚴格純單字：優先同音同調單字，接續同音異調單字。
    /// 若 ch 本身為注音符號（如許氏單鍵 ㄓ/ㄐ、ㄔ/ㄑ、ㄕ/ㄒ 等），則將同鍵位注音符號列於最前排！
    pub fn lookup_char_candidates_with_bpm(&self, ch: char, preferred_bpm: Option<&str>) -> Vec<String> {
        let ch_str = ch.to_string();
        let mut results = Vec::new();
        let is_bpm_char = ('\u{3105}'..='\u{3129}').contains(&ch) || matches!(ch, 'ˊ' | 'ˇ' | 'ˋ' | '˙');

        if is_bpm_char {
            for sym in Self::get_hsu_bopomofo_symbols(&ch_str) {
                if !results.contains(&sym) {
                    results.push(sym);
                }
            }
            let lookup_key = preferred_bpm.unwrap_or(&ch_str);
            for e in self.lookup_bopomofo(lookup_key, true) {
                if e.word.chars().count() == 1 && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }
            return results;
        }

        if let Some(target_bpm) = preferred_bpm {
            let target_base = LexiconEntry::strip_tones(target_bpm);

            // 1. 指定讀音之「同音同調」單字 (Exact Tone Single Characters) - 優先排序高頻字
            let entries = self.lookup_bopomofo(target_bpm, true);
            for e in entries {
                if e.word.chars().count() == 1 && !results.contains(&e.word) {
                    results.push(e.word.clone());
                }
            }

            // 若為單一注音符號音節（如 j+Space -> ㄓ、v+Space -> ㄔ、c+Space -> ㄕ），於第 1 頁前段附加該許氏按鍵對應之獨立注音符號（如 ㄐ、ㄑ、ㄒ）
            if target_bpm.chars().count() == 1 {
                let hsu_syms = Self::get_hsu_bopomofo_symbols(target_bpm);
                let insert_pos = results.len().min(3);
                let mut offset = 0;
                for sym in hsu_syms {
                    if !results.contains(&sym) {
                        results.insert(insert_pos + offset, sym);
                        offset += 1;
                    }
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

        // 5. 若為 4 字對角線重疊構式（如 `一試再試` / `做好做滿` / `走來走去`），附加同音對角諧音組合
        if char_count == 4 {
            let w_chars: Vec<char> = word.chars().collect();
            let syl_opts: Vec<Option<String>> = if let Some(b) = preferred_bpm {
                let parts: Vec<&str> = b.split_whitespace().collect();
                if parts.len() == 4 {
                    parts.into_iter().map(|s| Some(s.to_string())).collect()
                } else {
                    vec![None; 4]
                }
            } else {
                vec![None; 4]
            };
            if let Some((_, diag_puns)) = self.lookup_diagonal_pun_constructions(&w_chars, &syl_opts, 0) {
                for pun in diag_puns {
                    if !results.contains(&pun) {
                        results.push(pun);
                    }
                }
            }
        }

        results
    }

    /// 拓撲結構 II — 對角線恆等流形（$\Delta = \{(x, y) \mid x = y\}$）之台灣諧音魂候選展開器：
    /// 當游標所在之 4 字構式符合對角線重疊骨架：
    ///   - Frame A: `一 + X + 再 + X`（如 `一試再試` ➔ `一試再試`, `一事再事`, `一世再世`, `一室再室`, `一式再式`, `一視再視`, `一是再是`...）
    ///   - Frame B: `X + 好 + X + 滿`（如 `做好做滿` ➔ `做好做滿`, `坐好坐滿`, `作好作滿`, `座好座滿`...）
    ///   - Frame C: `X + 來 + X + 去`（如 `走來走去` / `看來看去` / `算來算去` ➔ 同音對角諧音組合）
    /// 保持首選為正統動詞構式，同時將同音對角組合納入候選字詞列，滿足台灣使用者諧音雙關選詞需求。
    pub fn lookup_diagonal_pun_constructions(
        &self,
        chars: &[char],
        syllables: &[Option<String>],
        cursor_idx: usize,
    ) -> Option<((usize, usize), Vec<String>)> {
        if chars.len() < 4 {
            return None;
        }
        let cur = cursor_idx.min(chars.len() - 1);
        let ont = crate::router::morpheme_ontology::MorphemeOntology::global();
        use crate::router::morpheme_ontology::MorphemeClass as MC;

        for s in 0..=chars.len() - 4 {
            if cur < s || cur >= s + 4 {
                continue;
            }
            let c0 = chars[s];
            let c1 = chars[s + 1];
            let c2 = chars[s + 2];
            let c3 = chars[s + 3];

            let syl_eq = |idx_a: usize, idx_b: usize, ch_a: char, ch_b: char| -> Option<String> {
                let sa = syllables.get(idx_a).and_then(|o| o.as_deref());
                let sb = syllables.get(idx_b).and_then(|o| o.as_deref());
                if let (Some(ba), Some(bb)) = (sa, sb) {
                    if ba == bb && !ba.is_empty() {
                        return Some(ba.to_string());
                    }
                }
                if ch_a == ch_b {
                    if let Some(b) = sa.or(sb) {
                        if !b.is_empty() {
                            return Some(b.to_string());
                        }
                    }
                    let ch_s = ch_a.to_string();
                    if let Some(ids) = self.word_index.get(&ch_s) {
                        if let Some(&first_id) = ids.first() {
                            return Some(self.entries[first_id].bopomofo.clone());
                        }
                    }
                }
                None
            };

            // Frame A: `一 + X + 再 + X`
            if c0 == '一' && c2 == '再' {
                if let Some(x_bpm) = syl_eq(s + 1, s + 3, c1, c3) {
                    let mut scored_x: Vec<(String, f64)> = Vec::new();
                    for e in self.lookup_bopomofo(&x_bpm, true) {
                        if e.word.chars().count() != 1 || e.word == "稔" {
                            continue;
                        }
                        let yi_x = format!("一{}", e.word);
                        let yi_x_w = self.max_compound_weight(&yi_x);
                        let is_rep_v = ont.has_class(&e.word, MC::RepeatableActionVerb);
                        if !is_rep_v && yi_x_w == 0 && e.weight < 450 {
                            continue;
                        }
                        let mut score = e.weight as f64;
                        if is_rep_v {
                            score += 5000.0;
                        } else if yi_x_w > 0
                            || matches!(
                                e.word.as_str(),
                                "事" | "世" | "室" | "式" | "視" | "市" | "是" | "勢" | "適" | "釋"
                            )
                        {
                            score += 2000.0 + (yi_x_w as f64);
                        }
                        if !scored_x.iter().any(|(w, _)| w == &e.word) {
                            scored_x.push((e.word.clone(), score));
                        }
                    }
                    scored_x.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let mut puns: Vec<String> = Vec::new();
                    let orig_4: String = chars[s..s + 4].iter().collect();
                    if c1 == c3 {
                        puns.push(orig_4);
                    }
                    for (x, _) in scored_x.into_iter().take(12) {
                        let cand = format!("一{}再{}", x, x);
                        if !puns.contains(&cand) {
                            puns.push(cand);
                        }
                    }
                    if puns.len() >= 2 {
                        return Some(((s, s + 4), puns));
                    }
                }
            }

            // Frame B: `X + 好 + X + 滿` / Frame C: `X + 來 + X + 去`
            let frame_pair: Option<(&str, &str)> = if c1 == '好' && c3 == '滿' {
                Some(("好", "滿"))
            } else if c1 == '來' && c3 == '去' {
                Some(("來", "去"))
            } else {
                None
            };
            if let Some((mid, tail)) = frame_pair {
                if let Some(x_bpm) = syl_eq(s, s + 2, c0, c2) {
                    let mut scored_x: Vec<(String, f64)> = Vec::new();
                    for e in self.lookup_bopomofo(&x_bpm, true) {
                        if e.word.chars().count() != 1 || e.weight < 420 {
                            continue;
                        }
                        let is_rep_v = ont.has_class(&e.word, MC::RepeatableActionVerb);
                        let score = (e.weight as f64) + if is_rep_v { 3000.0 } else { 0.0 };
                        if !scored_x.iter().any(|(w, _)| w == &e.word) {
                            scored_x.push((e.word.clone(), score));
                        }
                    }
                    scored_x.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let mut puns: Vec<String> = Vec::new();
                    let orig_4: String = chars[s..s + 4].iter().collect();
                    if c0 == c2 {
                        puns.push(orig_4);
                    }
                    for (x, _) in scored_x.into_iter().take(10) {
                        let cand = format!("{}{}{}{}", x, mid, x, tail);
                        if !puns.contains(&cand) {
                            puns.push(cand);
                        }
                    }
                    if puns.len() >= 2 {
                        return Some(((s, s + 4), puns));
                    }
                }
            }
        }
        None
    }

    pub fn lookup_word_homophones(&self, word: &str) -> Vec<String> {
        self.lookup_word_homophones_with_bpm(word, None)
    }

    pub fn get_fuzzy_bpms(bpm: &str) -> Vec<String> {
        let mut variants = Vec::new();
        let map_pairs = [
            ("ㄓ", "ㄗ"), ("ㄗ", "ㄓ"),
            ("ㄔ", "ㄘ"), ("ㄘ", "ㄔ"),
            ("ㄕ", "ㄙ"), ("ㄙ", "ㄕ"),
            ("ㄣ", "ㄥ"), ("ㄥ", "ㄣ"),
            ("ㄢ", "ㄤ"), ("ㄤ", "ㄢ"),
            ("ㄌ", "ㄋ"), ("ㄋ", "ㄌ"),
            ("ㄌ", "ㄖ"), ("ㄖ", "ㄌ"),
            ("ㄈ", "ㄏ"), ("ㄏ", "ㄈ"),
            ("ㄜ", "ㄦ"), ("ㄦ", "ㄜ"),
            ("ㄛ", "ㄜ"), ("ㄜ", "ㄛ"),
            ("ㄝ", "ㄟ"), ("ㄟ", "ㄝ"),
        ];
        for (from, to) in map_pairs {
            if bpm.contains(from) {
                let swapped = bpm.replace(from, to);
                if !variants.contains(&swapped) {
                    variants.push(swapped);
                }
            }
        }
        // 若為多音節字串（以空白分隔），支援逐音節獨立模糊替換組合
        if bpm.contains(' ') {
            let syls: Vec<&str> = bpm.split_whitespace().collect();
            if syls.len() >= 2 && syls.len() <= 3 {
                let mut per_syl_variants: Vec<Vec<String>> = Vec::with_capacity(syls.len());
                for s in &syls {
                    let mut v = vec![(*s).to_string()];
                    for (from, to) in map_pairs {
                        if s.contains(from) {
                            let sw = s.replace(from, to);
                            if !v.contains(&sw) {
                                v.push(sw);
                            }
                        }
                    }
                    per_syl_variants.push(v);
                }
                let mut combs = vec![Vec::new()];
                for sv in per_syl_variants {
                    let mut next = Vec::new();
                    for c in &combs {
                        for item in &sv {
                            let mut n = c.clone();
                            n.push(item.clone());
                            next.push(n);
                        }
                    }
                    combs = next;
                    if combs.len() > 24 {
                        combs.truncate(24);
                    }
                }
                for c in combs {
                    let joined = c.join(" ");
                    if joined != bpm && !variants.contains(&joined) {
                        variants.push(joined);
                    }
                }
            }
        }
        variants
    }

    /// 查詢指定詞彙在詞庫中的最高權重（僅計算長度 >= 2 之合法複合詞）
    pub fn max_compound_weight(&self, word: &str) -> u32 {
        if word.chars().count() < 2 {
            return 0;
        }
        if let Some(indices) = self.word_index.get(word) {
            indices
                .iter()
                .filter_map(|&i| self.entries.get(i))
                .map(|e| e.weight)
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// 針對多字詞視窗 `slice_chars` 進行跨聲調、平捲舌／前後鼻音模糊音及一字錨定之複合詞救援查詢
    fn lookup_fuzzy_compounds_in_window(
        &self,
        slice_chars: &[char],
        slice_syls: &[Option<String>],
        prev_ctx: &str,
        next_ctx: &str,
        transitions: &mut crate::router::transitions::TransitionEngine,
    ) -> Vec<(String, f64)> {
        let len = slice_chars.len();
        if len < 2 || len > 3 {
            return Vec::new();
        }
        let orig_word: String = slice_chars.iter().collect();

        // 收集每一字位可能的無聲調基礎音節（含指定音節、字庫讀音及模糊音變體）
        let mut per_pos_bases: Vec<Vec<String>> = Vec::with_capacity(len);
        for i in 0..len {
            let mut bases = Vec::new();
            if let Some(Some(syl)) = slice_syls.get(i) {
                let b = LexiconEntry::strip_tones(syl);
                if !b.is_empty() {
                    bases.push(b.clone());
                    for fb in Self::get_fuzzy_bpms(&b) {
                        let fbs = LexiconEntry::strip_tones(&fb);
                        if !fbs.is_empty() && !bases.contains(&fbs) {
                            bases.push(fbs);
                        }
                    }
                }
            }
            let ch_s = slice_chars[i].to_string();
            if let Some(indices) = self.word_index.get(&ch_s) {
                for &idx in indices.iter().take(4) {
                    let b = LexiconEntry::strip_tones(&self.entries[idx].bopomofo);
                    if !b.is_empty() && !bases.contains(&b) {
                        bases.push(b.clone());
                    }
                    for fb in Self::get_fuzzy_bpms(&b) {
                        let fbs = LexiconEntry::strip_tones(&fb);
                        if !fbs.is_empty() && !bases.contains(&fbs) {
                            bases.push(fbs);
                        }
                    }
                }
            }
            if bases.is_empty() {
                return Vec::new();
            }
            if bases.len() > 8 {
                bases.truncate(8);
            }
            per_pos_bases.push(bases);
        }

        let mut combinations = vec![Vec::new()];
        for bases in &per_pos_bases {
            let mut next_comb = Vec::new();
            for existing in &combinations {
                for b in bases {
                    let mut c = existing.clone();
                    c.push(b.clone());
                    next_comb.push(c);
                }
            }
            combinations = next_comb;
            if combinations.len() > 36 {
                combinations.truncate(36);
            }
        }

        let primary_base_joined: String = per_pos_bases
            .iter()
            .map(|v| v[0].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let exact_bpm_joined: Option<String> = {
            let mut parts = Vec::with_capacity(len);
            let mut all_some = true;
            for i in 0..len {
                if let Some(Some(syl)) = slice_syls.get(i) {
                    let norm = LexiconEntry::normalize_bpm(syl);
                    if !norm.is_empty() {
                        parts.push(norm);
                    } else {
                        all_some = false;
                        break;
                    }
                } else {
                    all_some = false;
                    break;
                }
            }
            if all_some && parts.len() == len {
                Some(parts.join(" "))
            } else {
                None
            }
        };
        let orig_compound_weight = self.max_compound_weight(&orig_word);

        let mut scored: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for comb in &combinations {
            let key = comb.join(" ");
            let is_primary_base = key == primary_base_joined;
            if let Some(indices) = self.notone_index.get(&key) {
                for &idx in indices {
                    let entry = &self.entries[idx];
                    if entry.word.chars().count() != len {
                        continue;
                    }
                    let is_exact_tone_homophone = exact_bpm_joined
                        .as_ref()
                        .map(|eb| LexiconEntry::normalize_bpm(&entry.bopomofo) == *eb)
                        .unwrap_or(false);
                    let mut score = entry.weight as f64;
                    if is_primary_base {
                        score += 220.0;
                    }
                    // 計算與原字串共享錨定字元數（如「答按」與「答案」共享「答」）
                    let shared_chars = entry
                        .word
                        .chars()
                        .zip(slice_chars.iter())
                        .filter(|(a, b)| a == *b)
                        .count();
                    if shared_chars > 0 && entry.word != orig_word {
                        score += (shared_chars as f64) * 180.0;
                    }
                    if !prev_ctx.is_empty() {
                        score += transitions.calculate_transition_bonus(
                            prev_ctx,
                            "",
                            &entry.word,
                            &entry.bopomofo,
                        ) * 18.0;
                    }
                    if !next_ctx.is_empty() {
                        score += transitions.calculate_transition_bonus(
                            &entry.word,
                            &entry.bopomofo,
                            next_ctx,
                            "",
                        ) * 18.0;
                    }
                    // 上箭頭專司「近音字詞（異調／聲韻模糊）」與「斷詞救濟」：
                    // 若原雙字詞已是合法成詞，同音同調詞已由下箭頭 `↓` 列出，故於 `↑` 退居近音詞之後
                    if entry.word == orig_word {
                        score -= 500.0;
                    } else if is_exact_tone_homophone && orig_compound_weight >= 350 {
                        score -= 380.0;
                    }
                    let cur = scored.entry(entry.word.clone()).or_insert(f64::NEG_INFINITY);
                    if score > *cur {
                        *cur = score;
                    }
                }
            }
        }

        let mut vec_scored: Vec<(String, f64)> = scored.into_iter().collect();
        vec_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        vec_scored
    }

    /// 亞洲語文軌 `↑` 上箭頭：近音字詞（異調＋平捲舌／前後鼻音／ㄈㄏ／ㄋㄌㄖ模糊音）與上下文語意救濟
    /// 回傳 `(actual_target_idx, (range_start, range_end), candidates)`
    pub fn lookup_contextual_fuzzy_rescue(
        &self,
        chars: &[char],
        syllables: &[Option<String>],
        target_idx: usize,
        manual_range: Option<(usize, usize)>,
        transitions: &mut crate::router::transitions::TransitionEngine,
    ) -> (usize, (usize, usize), Vec<String>) {
        if chars.is_empty() {
            return (0, (0, 0), Vec::new());
        }
        let idx = target_idx.min(chars.len() - 1);

        // 1. 若使用者已明確框選多字範圍，直接針對該範圍進行跨聲調／模糊複合詞或 >=3 字頭碼救援
        if let Some((s, e)) = manual_range {
            if e > s + 1 && e <= chars.len() {
                let prev_ctx = if s >= 2 {
                    let p2: String = chars[s - 2..s].iter().collect();
                    if self.max_compound_weight(&p2) > 0 {
                        p2
                    } else {
                        chars[s - 1].to_string()
                    }
                } else if s == 1 {
                    chars[0].to_string()
                } else {
                    String::new()
                };
                let next_ctx = if e + 1 < chars.len() {
                    let n2: String = chars[e..e + 2].iter().collect();
                    if self.max_compound_weight(&n2) > 0 {
                        n2
                    } else {
                        chars[e].to_string()
                    }
                } else if e < chars.len() {
                    chars[e].to_string()
                } else {
                    String::new()
                };
                let syl_slice = if e <= syllables.len() {
                    &syllables[s..e]
                } else {
                    &[]
                };
                let scored = self.lookup_fuzzy_compounds_in_window(
                    &chars[s..e],
                    syl_slice,
                    &prev_ctx,
                    &next_ctx,
                    transitions,
                );
                if !scored.is_empty() {
                    let mut cands: Vec<String> = scored.into_iter().map(|(w, _)| w).collect();
                    let orig: String = chars[s..e].iter().collect();
                    if !cands.contains(&orig) {
                        cands.push(orig);
                    }
                    return (s, (s, e), cands);
                }
                if e - s >= 3 {
                    let mut bpm_hc = String::new();
                    for pos in s..e {
                        if ('\u{3105}'..='\u{3129}').contains(&chars[pos]) {
                            bpm_hc.push(chars[pos]);
                        } else if let Some(Some(ref syl)) = syllables.get(pos) {
                            let clean = syl.replace(['˙', 'ˊ', 'ˇ', 'ˋ'], "");
                            if let Some(first) = clean.chars().next() {
                                bpm_hc.push(first);
                            }
                        }
                    }
                    if bpm_hc.chars().count() == e - s {
                        let hc_cands = self.lookup_multi_char_headcode(&bpm_hc, "standard", Some(&prev_ctx), Some(transitions));
                        if !hc_cands.is_empty() {
                            return (s, (s, e), hc_cands);
                        }
                    }
                }
            }
        }

        // 1.5 檢查游標處是否為 >= 3 字之純聲母／注音頭碼序列（如「ㄓㄒㄉㄌ」➔「忠孝東路」、「ㄊㄅㄔㄓ」➔「台北車站」）
        if chars.len() >= 3 {
            let is_headcode_pos = |pos: usize| -> Option<char> {
                let c = chars[pos];
                if ('\u{3105}'..='\u{3129}').contains(&c) {
                    return Some(c);
                }
                if let Some(Some(ref syl)) = syllables.get(pos) {
                    let clean = syl.replace(['˙', 'ˊ', 'ˇ', 'ˋ'], "");
                    if clean.chars().count() == 1 {
                        return clean.chars().next();
                    }
                }
                None
            };
            if is_headcode_pos(idx).is_some() {
                let mut h_start = idx;
                while h_start > 0 && is_headcode_pos(h_start - 1).is_some() {
                    h_start -= 1;
                }
                let mut h_end = idx + 1;
                while h_end < chars.len() && is_headcode_pos(h_end).is_some() {
                    h_end += 1;
                }
                if h_end - h_start >= 3 {
                    let bpm_hc: String = (h_start..h_end).filter_map(is_headcode_pos).collect();
                    let prev_ctx = if h_start >= 2 {
                        let p2: String = chars[h_start - 2..h_start].iter().collect();
                        if self.max_compound_weight(&p2) > 0 { p2 } else { chars[h_start - 1].to_string() }
                    } else if h_start == 1 {
                        chars[0].to_string()
                    } else {
                        String::new()
                    };
                    let hc_cands = self.lookup_multi_char_headcode(&bpm_hc, "standard", Some(&prev_ctx), Some(transitions));
                    if !hc_cands.is_empty() {
                        return (h_start, (h_start, h_end), hc_cands);
                    }
                }
            }
        }

        // 輔助閉包：評估單字位 `pos` 的近音單字（異調＋平捲舌／前後鼻音／ㄈㄏ／ㄋㄌㄖ模糊音）與上下文語法救濟候選
        let eval_single_char_rescue = |pos: usize, trans: &mut crate::router::transitions::TransitionEngine| -> (Vec<String>, f64) {
            let target_ch = chars[pos];
            let target_ch_str = target_ch.to_string();
            let target_bpm = syllables.get(pos).and_then(|s| s.as_deref());

            let prev_word = if pos >= 2 {
                let p2: String = chars[pos - 2..pos].iter().collect();
                if self.max_compound_weight(&p2) >= 300 {
                    p2
                } else {
                    chars[pos - 1].to_string()
                }
            } else if pos == 1 {
                chars[0].to_string()
            } else {
                String::new()
            };
            let prev_char = if pos >= 1 { chars[pos - 1].to_string() } else { String::new() };

            let next_word = if pos + 2 < chars.len() {
                let n2: String = chars[pos + 1..pos + 3].iter().collect();
                if self.max_compound_weight(&n2) >= 300 {
                    n2
                } else {
                    chars[pos + 1].to_string()
                }
            } else if pos + 1 < chars.len() {
                chars[pos + 1].to_string()
            } else {
                String::new()
            };
            let next_char = if pos + 1 < chars.len() { chars[pos + 1].to_string() } else { String::new() };

            // 常見語法與易混字群 (Confusable Syntactic Clusters)
            let confusable_groups: &[&[&str]] = &[
                &["的", "地", "得"],
                &["在", "再"],
                &["那", "哪"],
                &["做", "作"],
                &["象", "像", "相"],
                &["已", "以"],
                &["須", "需"],
                &["練", "煉"],
                &["度", "渡"],
                &["歷", "曆", "勵"],
                &["帳", "脹", "障"],
            ];

            let exact_homophones: Vec<String> = self.lookup_char_candidates_with_bpm(target_ch, target_bpm);
            let exact_same_tone_set: std::collections::HashSet<String> = exact_homophones.iter().cloned().collect();

            // `pool` 儲存 `(候選單字, 基礎權重, 是否為近音或語法救濟)`：
            // 關鍵修正：絕不可先將 50+ 個同音同調字塞滿 pool 再 truncate(48) 導致近音字被截斷！
            let mut pool: Vec<(String, u32, bool)> = Vec::new();
            let mut seen_pool: std::collections::HashSet<String> = std::collections::HashSet::new();

            for grp in confusable_groups {
                if grp.contains(&target_ch_str.as_str()) {
                    for &item in *grp {
                        let s = item.to_string();
                        if seen_pool.insert(s.clone()) {
                            pool.push((s, 600, true));
                        }
                    }
                }
            }

            // A. 若同音同調字能與左鄰或右鄰直接構成高頻合法雙字詞（上下文斷詞救濟），優先納入
            for c in &exact_homophones {
                let forms_left = !prev_char.is_empty() && self.max_compound_weight(&format!("{}{}", prev_char, c)) >= 300;
                let forms_right = !next_char.is_empty() && self.max_compound_weight(&format!("{}{}", c, next_char)) >= 300;
                if (forms_left || forms_right) && seen_pool.insert(c.clone()) {
                    pool.push((c.clone(), 580, true));
                }
            }

            // B. 收集基礎讀音（含指定注音與字庫讀音）
            let mut base_bpms = Vec::new();
            if let Some(b) = target_bpm {
                if !b.is_empty() {
                    base_bpms.push(b.to_string());
                }
            }
            if let Some(indices) = self.word_index.get(&target_ch_str) {
                for &i in indices.iter().take(3) {
                    let b = &self.entries[i].bopomofo;
                    if !b.is_empty() && !base_bpms.contains(b) {
                        base_bpms.push(b.clone());
                    }
                }
            }

            // C. 第一主力：同音節異調近音單字（Cross-Tone Near-Homophones，排除已在 ↓ 出現之同音同調字）
            let mut cross_tone_cands: Vec<(String, u32)> = Vec::new();
            for b in &base_bpms {
                for e in self.lookup_bopomofo(b, false) {
                    if e.word.chars().count() == 1
                        && !exact_same_tone_set.contains(&e.word)
                        && e.weight >= 180
                    {
                        if let Some(existing) = cross_tone_cands.iter_mut().find(|x| x.0 == e.word) {
                            if e.weight > existing.1 {
                                existing.1 = e.weight;
                            }
                        } else {
                            cross_tone_cands.push((e.word.clone(), e.weight));
                        }
                    }
                }
            }
            cross_tone_cands.sort_by(|a, b| b.1.cmp(&a.1));
            for (w, wt) in cross_tone_cands.into_iter().take(28) {
                if seen_pool.insert(w.clone()) {
                    pool.push((w, wt + 80, true));
                }
            }

            // D. 第二主力：聲韻模糊近音單字（平捲舌 ㄓ↔ㄗ/ㄔ↔ㄘ/ㄕ↔ㄙ、前後鼻音 ㄢ↔ㄤ/ㄣ↔ㄥ、ㄈ↔ㄏ、ㄋ↔ㄌ↔ㄖ、ㄜ↔ㄦ/ㄛ）
            let mut fuzzy_cands: Vec<(String, u32)> = Vec::new();
            for b in &base_bpms {
                let stripped_b = LexiconEntry::strip_tones(b);
                for fb in Self::get_fuzzy_bpms(b) {
                    if LexiconEntry::strip_tones(&fb) == stripped_b {
                        continue;
                    }
                    for e in self.lookup_bopomofo(&fb, false) {
                        if e.word.chars().count() == 1
                            && !exact_same_tone_set.contains(&e.word)
                            && e.weight >= 220
                        {
                            if let Some(existing) = fuzzy_cands.iter_mut().find(|x| x.0 == e.word) {
                                if e.weight > existing.1 {
                                    existing.1 = e.weight;
                                }
                            } else {
                                fuzzy_cands.push((e.word.clone(), e.weight));
                            }
                        }
                    }
                }
            }
            fuzzy_cands.sort_by(|a, b| b.1.cmp(&a.1));
            for (w, wt) in fuzzy_cands.into_iter().take(28) {
                if seen_pool.insert(w.clone()) {
                    pool.push((w, wt + 40, true));
                }
            }

            // E. 保底包含原字（供計算 best_alt_gain 比較）與少量同音字尾段保底
            if seen_pool.insert(target_ch_str.clone()) {
                pool.push((target_ch_str.clone(), 500, false));
            }
            for c in exact_homophones.into_iter().take(8) {
                if pool.len() < 56 && seen_pool.insert(c.clone()) {
                    pool.push((c, 300, false));
                }
            }

            let mut scored_cands: Vec<(String, f64)> = Vec::with_capacity(pool.len());
            let mut orig_contextual_score = 0.0;

            for (cand, base_wt, is_near_or_rescue) in &pool {
                let mut score = (*base_wt as f64) * 0.35 + if *is_near_or_rescue { 120.0 } else { -180.0 };

                // A. 與前後鄰字能否直接構成詞庫合法雙字詞（如「答」+「案」=「答案」）
                if !prev_char.is_empty() {
                    let left_comp = format!("{}{}", prev_char, cand);
                    let w = self.max_compound_weight(&left_comp);
                    if w > 0 {
                        score += 350.0 + (w as f64) * 0.8;
                    }
                }
                if !next_char.is_empty() {
                    let right_comp = format!("{}{}", cand, next_char);
                    let w = self.max_compound_weight(&right_comp);
                    if w > 0 {
                        score += 350.0 + (w as f64) * 0.8;
                    }
                }

                // B. Layer 1 語法規則與 Layer 2 二元搭配轉移加分
                let mut ctx_bonus = 0.0;
                if !prev_word.is_empty() {
                    ctx_bonus += trans.calculate_transition_bonus(&prev_word, "", cand, "");
                }
                if !next_word.is_empty() {
                    ctx_bonus += trans.calculate_transition_bonus(cand, "", &next_word, "");
                }

                // 結構助詞「的／地／得」精準語法構式判別
                if matches!(cand.as_str(), "的" | "地" | "得") {
                    let prev_is_adv_manner = prev_word.chars().count() == 2
                        && {
                            let pc: Vec<char> = prev_word.chars().collect();
                            pc[0] == pc[1] // 疊字副詞：慢慢、漸漸、悄悄、靜靜
                        };
                    let next_is_noun = !next_word.is_empty() && self.max_compound_weight(&next_word) >= 350;
                    if cand == "的" && (next_is_noun || (!prev_word.is_empty() && !prev_is_adv_manner)) {
                        ctx_bonus += 18.0;
                    } else if cand == "地" && prev_is_adv_manner && !next_is_noun {
                        ctx_bonus += 20.0;
                    }
                }

                score += ctx_bonus * 25.0;
                if cand == &target_ch_str {
                    orig_contextual_score = score;
                    score -= 400.0; // 原字排於近音救濟候選之後
                }
                scored_cands.push((cand.clone(), score));
            }

            scored_cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let best_alt_gain = scored_cands
                .iter()
                .find(|(w, _)| w != &target_ch_str)
                .map(|(_, s)| *s - orig_contextual_score)
                .unwrap_or(0.0);
            let final_list: Vec<String> = scored_cands.into_iter().map(|(w, _)| w).collect();
            (final_list, best_alt_gain)
        };

        // 2. 檢查游標所在處是否構成「未成詞之雙字模糊／跨聲調複合詞」（如「答按」➔「答案」、「支源」➔「支援」）
        //    但嚴格禁止拆裂旁邊已經成立的高權重雙字詞（如「可能地答案」中游標在「地」時，絕不拆裂「可能」或「答案」）
        let is_particle_char = matches!(chars[idx], '的' | '地' | '得' | '了' | '著' | '過' | '嗎' | '呢' | '吧');
        let mut valid_word_fuzzy_cands: Option<(usize, usize, Vec<String>)> = None;
        if !is_particle_char && chars.len() >= 2 {
            let mut candidate_windows = Vec::new();
            if idx >= 1 {
                candidate_windows.push((idx - 1, idx + 1));
            }
            if idx + 1 < chars.len() {
                candidate_windows.push((idx, idx + 2));
            }

            for (s, e) in candidate_windows {
                let win_str: String = chars[s..e].iter().collect();
                let cur_weight = self.max_compound_weight(&win_str);
                // 檢查是否會拆裂左側或右側既有之合法高頻雙字詞
                let splits_left = s >= 1 && {
                    let left_w: String = chars[s - 1..s + 1].iter().collect();
                    self.max_compound_weight(&left_w) >= 400 && cur_weight < 400
                };
                let splits_right = e < chars.len() && {
                    let right_w: String = chars[e - 1..e + 1].iter().collect();
                    self.max_compound_weight(&right_w) >= 400 && cur_weight < 400
                };
                if splits_left || splits_right {
                    continue;
                }

                let prev_ctx = if s >= 2 {
                    let p2: String = chars[s - 2..s].iter().collect();
                    if self.max_compound_weight(&p2) > 0 { p2 } else { chars[s - 1].to_string() }
                } else if s == 1 {
                    chars[0].to_string()
                } else {
                    String::new()
                };
                let next_ctx = if e + 1 < chars.len() {
                    let n2: String = chars[e..e + 2].iter().collect();
                    if self.max_compound_weight(&n2) > 0 { n2 } else { chars[e].to_string() }
                } else if e < chars.len() {
                    chars[e].to_string()
                } else {
                    String::new()
                };
                let syl_slice = if e <= syllables.len() { &syllables[s..e] } else { &[] };
                let scored = self.lookup_fuzzy_compounds_in_window(
                    &chars[s..e],
                    syl_slice,
                    &prev_ctx,
                    &next_ctx,
                    transitions,
                );

                if cur_weight < 400 {
                    // 若該雙字視窗尚未成詞（如「答按」），且找到高信心救濟雙字詞（如「答案」），直接回傳雙字區間救濟
                    if let Some((best_w, best_score)) = scored.first() {
                        if (best_w != &win_str && *best_score >= 450.0) || (cur_weight == 0 && *best_score >= 300.0) {
                            let mut cands: Vec<String> = scored.into_iter().map(|(w, _)| w).collect();
                            if !cands.contains(&win_str) {
                                cands.push(win_str);
                            }
                            return (s, (s, e), cands);
                        }
                    }
                } else if valid_word_fuzzy_cands.is_none() {
                    // 若該雙字視窗本身已是合法詞（cur_weight >= 400），保留其「近音雙字詞（異調詞／模糊音詞）」候選，
                    // 待步驟 3 檢查完句中助詞誤用後，一併提供雙字近音詞與單字近音字！
                    let near_words: Vec<String> = scored
                        .into_iter()
                        .filter(|(w, sc)| w != &win_str && *sc >= 220.0)
                        .map(|(w, _)| w)
                        .take(16)
                        .collect();
                    if !near_words.is_empty() {
                        valid_word_fuzzy_cands = Some((s, e, near_words));
                    }
                }
            }
        }

        // 3. 若游標停在句尾（如剛打完「可能地答案」未移動游標直接按 `↑`），而句尾雙字詞本身已是最佳合法詞，
        //    主動掃描句中是否存在極高信心度的「的／地／得、在／再」語法助詞誤用並精準定位救援！
        if idx + 1 == chars.len() && chars.len() >= 3 && !is_particle_char {
            for scan_pos in (1..chars.len() - 1).rev() {
                if matches!(chars[scan_pos], '的' | '地' | '得' | '在' | '再' | '那' | '哪') {
                    let (scan_cands, gain) = eval_single_char_rescue(scan_pos, transitions);
                    if gain >= 250.0 && !scan_cands.is_empty() && scan_cands[0] != chars[scan_pos].to_string() {
                        return (scan_pos, (scan_pos, scan_pos + 1), scan_cands);
                    }
                }
            }
        }

        // 4. 單字位與雙字詞之近音候選列（近音雙字詞 + 近音單字）
        let (single_cands, _) = eval_single_char_rescue(idx, transitions);
        if let Some((ws, we, mut near_words)) = valid_word_fuzzy_cands {
            for sc in single_cands {
                if !near_words.contains(&sc) {
                    near_words.push(sc);
                }
            }
            return (idx, (ws, we), near_words);
        }
        (idx, (idx, idx + 1), single_cands)
    }

    /// 根據游標左側已組出之字詞前綴（優先匹配 3 字 ➔ 2 字 ➔ 1 字前綴），
    /// 結合詞庫長詞後半截續寫與二階轉移／動賓搭配，回傳「只列出後半截字詞」之聯想候選清單。
    pub fn lookup_associative_continuations(
        &self,
        chars: &[char],
        cur: usize,
        transitions: &crate::router::transitions::TransitionEngine,
    ) -> Vec<String> {
        let p_end = cur.min(chars.len());
        if p_end == 0 {
            return Vec::new();
        }
        let is_word_char = |c: char| -> bool {
            !c.is_whitespace()
                && !c.is_ascii_punctuation()
                && !matches!(
                    c,
                    '，' | '。' | '、' | '；' | '：' | '？' | '！' | '「' | '」'
                        | '『' | '』' | '（' | '）' | '【' | '】' | '《' | '》'
                        | '〈' | '〉' | '—' | '…' | '～' | '·'
                )
        };
        if !is_word_char(chars[p_end - 1]) {
            return Vec::new();
        }

        let p3: Option<String> = if p_end >= 3 && chars[p_end - 3..p_end].iter().all(|&c| is_word_char(c)) {
            Some(chars[p_end - 3..p_end].iter().collect())
        } else {
            None
        };
        let p2: Option<String> = if p_end >= 2 && chars[p_end - 2..p_end].iter().all(|&c| is_word_char(c)) {
            Some(chars[p_end - 2..p_end].iter().collect())
        } else {
            None
        };
        let p1: String = chars[p_end - 1].to_string();

        let mut scored: HashMap<String, f64> = HashMap::new();
        let mut upsert = |s: String, sc: f64| {
            if s.is_empty() {
                return;
            }
            let e = scored.entry(s).or_insert(sc);
            if sc > *e {
                *e = sc;
            }
        };

        let mut p2_hit_count = 0usize;

        for entry in &self.entries {
            if let Some(ref pre3) = p3 {
                if entry.word.starts_with(pre3.as_str()) && entry.word.len() > pre3.len() {
                    let suf = &entry.word[pre3.len()..];
                    upsert(suf.to_string(), 30000.0 + entry.weight as f64);
                }
            }
            if let Some(ref pre2) = p2 {
                if entry.word.starts_with(pre2.as_str()) && entry.word.len() > pre2.len() {
                    let suf = &entry.word[pre2.len()..];
                    let suf_len = suf.chars().count();
                    let len_bonus = if suf_len == 2 {
                        150.0
                    } else if suf_len > 2 {
                        80.0
                    } else {
                        0.0
                    };
                    upsert(suf.to_string(), 20000.0 + entry.weight as f64 + len_bonus);
                    p2_hit_count += 1;
                }
            }
        }

        if let Some(ref pre2) = p2 {
            for ((prev, curr), &bonus) in transitions
                .builtin_transitions
                .iter()
                .chain(transitions.transitions.iter())
            {
                if prev == pre2
                    && bonus > 5.0
                    && !curr.is_empty()
                    && !transitions.is_excluded_pair(prev, curr)
                {
                    let w = self.max_compound_weight(curr).max(400) as f64;
                    upsert(curr.clone(), 18000.0 + bonus * 25.0 + w * 0.5);
                    p2_hit_count += 1;
                }
            }
            for vo in &transitions.vo_pairs {
                if &vo.verb == pre2 && !vo.object.is_empty() {
                    upsert(vo.object.clone(), 18500.0 + vo.bonus_score * 25.0);
                    p2_hit_count += 1;
                }
            }
        }

        // 2b. 單字動詞動賓（VO）搭配、可離合體貌／量詞構式橋接（洗個->澡、吃了一頓->飯、開了一場->會）及同子句領域語境提升
        {
            let ctx_end = cur.min(chars.len());
            if ctx_end >= 1 {
                let tail1: String = chars[ctx_end - 1..ctx_end].iter().collect();
                for vo in &transitions.vo_pairs {
                    if vo.verb == tail1 && !vo.object.is_empty() {
                        upsert(vo.object.clone(), 18200.0 + vo.bonus_score * 25.0);
                    }
                }

                // 往回穿透最多 4 個透明體貌／數詞／量詞字元（了/過/著/一/兩/三/幾/這/那/半/個/頓/場/杯/碗...）
                let is_sep_bridge = |ch: char| -> bool {
                    matches!(
                        ch,
                        '了' | '過' | '著' | '着' | '一' | '兩' | '三' | '幾' | '這' | '那' | '半'
                            | '個' | '頓' | '場' | '杯' | '碗' | '盤' | '瓶' | '罐' | '壺' | '趟'
                            | '次' | '遍' | '點' | '些' | '陣' | '把' | '口' | '件' | '條' | '張'
                            | '首' | '部' | '片' | '通' | '封' | '門' | '堂' | '節'
                    )
                };
                let mut bridge_start = ctx_end;
                let min_start = ctx_end.saturating_sub(4);
                while bridge_start > min_start && is_sep_bridge(chars[bridge_start - 1]) {
                    bridge_start -= 1;
                }
                if bridge_start < ctx_end && bridge_start >= 1 {
                    let bridge_chars = &chars[bridge_start..ctx_end];
                    let verb1: String = chars[bridge_start - 1..bridge_start].iter().collect();
                    let verb2: Option<String> = if bridge_start >= 2 {
                        Some(chars[bridge_start - 2..bridge_start].iter().collect())
                    } else {
                        None
                    };
                    let classifier = bridge_chars
                        .iter()
                        .copied()
                        .rev()
                        .find(|&c| {
                            matches!(
                                c,
                                '頓' | '場' | '杯' | '碗' | '瓶' | '罐' | '壺' | '首' | '部'
                                    | '片' | '通' | '封' | '個'
                            )
                        });
                    let classifier_affinity = |cl: Option<char>, obj: &str| -> f64 {
                        match cl {
                            Some('頓') => {
                                if matches!(
                                    obj,
                                    "飯" | "早餐" | "午餐" | "晚餐" | "宵夜" | "便當" | "大餐" | "餐點"
                                ) {
                                    3200.0
                                } else {
                                    -1200.0
                                }
                            }
                            Some('場') => {
                                if matches!(
                                    obj,
                                    "會" | "會議" | "比賽" | "演出" | "電影" | "夢" | "戲" | "雨" | "講座"
                                ) {
                                    3200.0
                                } else {
                                    -1200.0
                                }
                            }
                            Some('杯' | '瓶' | '罐' | '壺') => {
                                if matches!(
                                    obj,
                                    "咖啡" | "茶" | "水" | "牛奶" | "果汁" | "酒" | "飲料" | "奶茶"
                                ) {
                                    3000.0
                                } else {
                                    -1000.0
                                }
                            }
                            Some('首') => {
                                if matches!(obj, "歌" | "歌曲" | "音樂" | "詩") {
                                    3000.0
                                } else {
                                    -1000.0
                                }
                            }
                            Some('部' | '片') => {
                                if matches!(obj, "電影" | "影片" | "紀錄片" | "劇") {
                                    3000.0
                                } else {
                                    -1000.0
                                }
                            }
                            Some('通') => {
                                if matches!(obj, "電話") {
                                    3000.0
                                } else {
                                    -1000.0
                                }
                            }
                            Some('封') => {
                                if matches!(obj, "信" | "郵件" | "簡訊") {
                                    3000.0
                                } else {
                                    -1000.0
                                }
                            }
                            Some('個') => {
                                if matches!(obj, "澡" | "覺" | "便當" | "電話" | "會") {
                                    1800.0
                                } else {
                                    0.0
                                }
                            }
                            _ => 0.0,
                        }
                    };

                    for vo in &transitions.vo_pairs {
                        let verb_matched = vo.verb == verb1
                            || verb2.as_ref().map_or(false, |v2| &vo.verb == v2);
                        if verb_matched && !vo.object.is_empty() {
                            let aff = classifier_affinity(classifier, vo.object.as_str());
                            let sc = 18400.0 + vo.bonus_score * 25.0 + aff;
                            if sc > 15000.0 {
                                upsert(vo.object.clone(), sc);
                            }
                        }
                    }

                    // 典型動賓+量詞構式直映（確保 vo_lexicon 未收錄特定高頻詞對時仍可命中）
                    match (verb1.as_str(), classifier) {
                        ("吃", Some('頓')) => {
                            upsert("飯".to_string(), 22000.0);
                            upsert("大餐".to_string(), 21000.0);
                            upsert("晚餐".to_string(), 20800.0);
                        }
                        ("開", Some('場')) => {
                            upsert("會".to_string(), 22000.0);
                            upsert("會議".to_string(), 21200.0);
                        }
                        ("喝", Some('杯')) => {
                            upsert("咖啡".to_string(), 22000.0);
                            upsert("水".to_string(), 21400.0);
                            upsert("茶".to_string(), 21200.0);
                        }
                        ("洗", Some('個')) => {
                            upsert("澡".to_string(), 22500.0);
                        }
                        ("睡", Some('個')) => {
                            upsert("覺".to_string(), 22000.0);
                        }
                        _ => {}
                    }
                }

                // 同子句語意場域連貫提升（停止於任何中英標點或換行）
                let is_zh_clause_break = |ch: char| -> bool {
                    matches!(
                        ch,
                        '，' | '。' | '、' | '；' | '：' | '！' | '？' | '「' | '」' | '（' | '）'
                            | ',' | '.' | ';' | ':' | '!' | '?' | '\n' | '\r'
                    )
                };
                let mut clause_start = ctx_end;
                let clause_min = ctx_end.saturating_sub(18);
                while clause_start > clause_min && !is_zh_clause_break(chars[clause_start - 1]) {
                    clause_start -= 1;
                }
                let clause_str: String = chars[clause_start..ctx_end].iter().collect();
                if tail1 == "看" {
                    if ["醫院", "診所", "掛號", "生病", "發燒", "感冒", "傷科", "門診"]
                        .iter()
                        .any(|k| clause_str.contains(k))
                    {
                        upsert("醫生".to_string(), 21800.0);
                        upsert("醫師".to_string(), 21400.0);
                        upsert("診".to_string(), 21000.0);
                        upsert("病".to_string(), 20500.0);
                    } else if ["電影院", "戲院", "影城", "買票", "電影票"]
                        .iter()
                        .any(|k| clause_str.contains(k))
                    {
                        upsert("電影".to_string(), 21800.0);
                        upsert("影片".to_string(), 21000.0);
                    }
                }
            }
        }

        if p2_hit_count < 6 {
            for entry in &self.entries {
                if entry.word.starts_with(p1.as_str()) && entry.word.len() > p1.len() {
                    let suf = &entry.word[p1.len()..];
                    let suf_len = suf.chars().count();
                    if suf_len <= 3 {
                        let len_bonus = if suf_len == 1 {
                            60.0
                        } else if suf_len == 2 {
                            30.0
                        } else {
                            0.0
                        };
                        upsert(suf.to_string(), 5000.0 + entry.weight as f64 + len_bonus);
                    }
                }
            }
            for ((prev, curr), &bonus) in transitions
                .builtin_transitions
                .iter()
                .chain(transitions.transitions.iter())
            {
                if prev == &p1
                    && bonus > 8.0
                    && !curr.is_empty()
                    && !transitions.is_excluded_pair(prev, curr)
                {
                    let w = self.max_compound_weight(curr).max(400) as f64;
                    upsert(curr.clone(), 4500.0 + bonus * 20.0 + w * 0.5);
                }
            }
        }

        let mut vec_scored: Vec<(String, f64)> = scored.into_iter().collect();
        vec_scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.chars().count().cmp(&b.0.chars().count()))
                .then_with(|| a.0.cmp(&b.0))
        });
        vec_scored.into_iter().take(45).map(|(w, _)| w).collect()
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

    /// 驗證永久黑名單帳本 (`rejected_readings.tsv`)：回傳所有意外殘留於詞庫中之違規 `(詞彙, 注音)` 項目
    pub fn verify_rejected_readings_tsv<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>, std::io::Error> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        let mut violations = Vec::new();
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let word = parts[0].trim();
                let bpm = parts[1].trim();
                if let Some(indices) = self.word_index.get(word) {
                    for &idx in indices {
                        let entry = &self.entries[idx];
                        let clean_entry_bpm = entry.bopomofo.replace('˙', "");
                        let clean_target_bpm = bpm.replace('˙', "");
                        if bpm == "*" || clean_entry_bpm == clean_target_bpm {
                            violations.push(format!("{} ({})", entry.word, entry.bopomofo));
                        }
                    }
                }
            }
        }
        Ok(violations)
    }

    /// 驗證可達性迴歸契約帳本 (`reachability.tsv`)：回傳所有未能在詞庫中透過指定注音查得之缺失項目清單
    pub fn verify_reachability_tsv<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>, std::io::Error> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        let mut missing = Vec::new();
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let word = parts[0].trim();
                let bpm = parts[1].trim();
                let norm_bpm = LexiconEntry::normalize_bpm(bpm);
                let found = self
                    .lookup_bopomofo(&norm_bpm, true)
                    .iter()
                    .any(|e| e.word == word)
                    || self
                        .lookup_bopomofo(bpm, true)
                        .iter()
                        .any(|e| e.word == word);
                if !found {
                    missing.push(format!("{} ({})", word, bpm));
                }
            }
        }
        Ok(missing)
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
            custom_abbrevs: Vec::new(),
            is_user_word: false,
        });
        mgr.add_entry(LexiconEntry {
            word: "納".to_string(),
            bopomofo: "ㄋㄚˋ".to_string(),
            headcode: "n".to_string(),
            weight: 800,
            category: "general".to_string(),
            note: String::new(),
            custom_abbrevs: Vec::new(),
            is_user_word: false,
        });
        mgr.add_entry(LexiconEntry {
            word: "哪".to_string(),
            bopomofo: "ㄋㄚˇ".to_string(),
            headcode: "n".to_string(),
            weight: 900,
            category: "general".to_string(),
            note: String::new(),
            custom_abbrevs: Vec::new(),
            is_user_word: false,
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
    fn test_export_and_verify_unified_core_lexicon() {
        let core_path = std::path::Path::new("../ruhi_lexicon/data/lexicon_core.tsv");
        let mc_path = std::path::Path::new("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv");
        if mc_path.exists() {
            let mut mgr_old = LexiconManager::new();
            let _ = mgr_old.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
            let _ = mgr_old.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
            let _ = mgr_old.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");

            let written = mgr_old.export_unified_core_tsv(core_path).expect("匯出 lexicon_core.tsv 失敗");
            println!("已匯出單一統整詞庫 lexicon_core.tsv，共寫入 {} 行", written);

            let mut mgr_new = LexiconManager::new();
            let _ = mgr_new.load_tsv(core_path, "core");

            println!("mgr_old entries: {}, mgr_new entries: {}", mgr_old.entries.len(), mgr_new.entries.len());
            println!("mgr_old bpm index: {}, mgr_new bpm index: {}", mgr_old.bopomofo_index.len(), mgr_new.bopomofo_index.len());
            println!("mgr_old notone index: {}, mgr_new notone index: {}", mgr_old.notone_index.len(), mgr_new.notone_index.len());

            assert_eq!(mgr_new.entries.len(), mgr_old.entries.len(), "詞條總數必須 100% 等價一致");
            assert_eq!(mgr_new.bopomofo_index.len(), mgr_old.bopomofo_index.len(), "注音索引鍵數必須 100% 等價一致");
            assert_eq!(mgr_new.notone_index.len(), mgr_old.notone_index.len(), "無聲調索引鍵數必須 100% 等價一致");
        }
    }

    #[test]
    fn test_candidate_priority_modern_over_archaic() {
        let mut mgr = LexiconManager::new();
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_core.tsv", "core");

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

    #[test]
    fn test_rejected_readings_and_reachability_ledgers() {
        let mut mgr = LexiconManager::new();
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_core.tsv", "core");

        let violations = mgr
            .verify_rejected_readings_tsv("../ruhi_lexicon/data/rejected_readings.tsv")
            .expect("讀取 rejected_readings.tsv 失敗");
        assert!(
            violations.is_empty(),
            "rejected_readings.tsv 中之黑名單詞條不得殘留於 LexiconManager：{:?}",
            violations
        );

        let missing = mgr
            .verify_reachability_tsv("../ruhi_lexicon/data/reachability.tsv")
            .expect("讀取 reachability.tsv 失敗");
        assert!(
            missing.is_empty(),
            "reachability.tsv 中之必達詞條必須 100% 可透過標準注音檢索命中，缺失項目：{:?}",
            missing
        );
    }

    #[test]
    fn test_purified_core_lexicon_3col_and_algebraic_sandhi() {
        let core_path = std::path::Path::new("../ruhi_lexicon/data/lexicon_core.tsv");
        assert!(core_path.exists(), "lexicon_core.tsv 必須存在");

        // 1. 驗證所有非註解行嚴格為 3 欄格式 (WORD\tBOPOMOFO\tWEIGHT) 且權重在 [60, 1000]
        let content = std::fs::read_to_string(core_path).expect("讀取 lexicon_core.tsv 失敗");
        let mut yi_ding_bu_shi_count = 0;
        let mut yi_ge_count = 0;

        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            assert_eq!(
                parts.len(),
                3,
                "第 {} 行必須嚴格為 3 欄格式 (WORD\\tBOPOMOFO\\tWEIGHT)，實際為：{:?}",
                line_no + 1,
                line
            );
            let word = parts[0];
            let weight: u32 = parts[2].parse().expect("權重必須為非負整數");
            assert!(
                (60..=1000).contains(&weight),
                "第 {} 行詞條「{}」權重 {} 必須介於 60..=1000",
                line_no + 1,
                word,
                weight
            );

            if word == "一丁不識" {
                yi_ding_bu_shi_count += 1;
            }
            if word == "一個" {
                yi_ge_count += 1;
            }
        }

        assert_eq!(yi_ding_bu_shi_count, 1, "「一丁不識」在 lexicon_core.tsv 中必須僅出現 1 次");
        assert_eq!(yi_ge_count, 1, "「一個」在 lexicon_core.tsv 中必須僅出現 1 次");

        // 2. 載入 LexiconManager 驗證代數連讀變調等價類展開
        let mut mgr = LexiconManager::new();
        let _ = mgr.load_tsv(core_path, "core");

        // 驗證「一丁不識」的全部 6 種連讀變調組合 ({ㄧ, ㄧˊ, ㄧˋ} × {ㄉㄧㄥ} × {ㄅㄨˋ, ㄅㄨˊ} × {ㄕˋ}) 皆能 100% 精確命中
        let sandhi_tests_ydbs = [
            "ㄧ ㄉㄧㄥ ㄅㄨˋ ㄕˋ",
            "ㄧ ㄉㄧㄥ ㄅㄨˊ ㄕˋ",
            "ㄧˊ ㄉㄧㄥ ㄅㄨˋ ㄕˋ",
            "ㄧˊ ㄉㄧㄥ ㄅㄨˊ ㄕˋ",
            "ㄧˋ ㄉㄧㄥ ㄅㄨˋ ㄕˋ",
            "ㄧˋ ㄉㄧㄥ ㄅㄨˊ ㄕˋ",
        ];
        for bpm in &sandhi_tests_ydbs {
            let res = mgr.lookup_bopomofo(bpm, true);
            assert!(
                res.iter().any(|e| e.word == "一丁不識"),
                "注音「{}」必須能命中「一丁不識」",
                bpm
            );
        }

        // 驗證「一個」的全部 6 種本調/變調/輕聲組合 ({ㄧ, ㄧˊ, ㄧˋ} × {ㄍㄜˋ, ˙ㄍㄜ}) 皆能 100% 精確命中
        let sandhi_tests_yg = [
            "ㄧ ㄍㄜˋ",
            "ㄧ ˙ㄍㄜ",
            "ㄧˊ ㄍㄜˋ",
            "ㄧˊ ˙ㄍㄜ",
            "ㄧˋ ㄍㄜˋ",
            "ㄧˋ ˙ㄍㄜ",
        ];
        for bpm in &sandhi_tests_yg {
            let res = mgr.lookup_bopomofo(bpm, true);
            assert!(
                res.iter().any(|e| e.word == "一個"),
                "注音「{}」必須能命中「一個」",
                bpm
            );
        }

        // 3. 驗證荒謬野生數字已被徹底清洗，絕不存在於詞庫中
        let wild_numbers = ["一七三", "一七九", "一七二", "一七五", "一七八", "一七六", "一七四"];
        for num in &wild_numbers {
            let res = mgr.word_index.get(*num);
            assert!(
                res.is_none() || res.unwrap().is_empty(),
                "野生隨機數字「{}」不得殘留於詞庫中",
                num
            );
        }
    }
}
