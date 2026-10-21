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

    pub fn extract_headcode(bpm: &str) -> String {
        let initial_map: HashMap<char, char> = [
            ('ㄅ', 'b'), ('ㄆ', 'p'), ('ㄇ', 'm'), ('ㄈ', 'f'),
            ('ㄉ', 'd'), ('ㄊ', 't'), ('ㄋ', 'n'), ('ㄌ', 'l'),
            ('ㄍ', 'g'), ('ㄎ', 'k'), ('ㄏ', 'h'),
            ('ㄐ', 'j'), ('ㄑ', 'v'), ('ㄒ', 'c'),
            ('ㄓ', 'j'), ('ㄔ', 'v'), ('ㄕ', 'c'), ('ㄖ', 'r'),
            ('ㄗ', 'z'), ('ㄘ', 'a'), ('ㄙ', 's'),
            ('ㄧ', 'e'), ('ㄨ', 'x'), ('ㄩ', 'u'),
            // 零聲母韻母字首對應許氏鍵位（如 安 ㄢ->m、歐 ㄡ->o、愛 ㄞ->i、二 ㄦ->l、阿 ㄚ->y、餓 ㄜ->g）
            ('ㄚ', 'y'), ('ㄛ', 'h'), ('ㄜ', 'g'), ('ㄝ', 'e'),
            ('ㄞ', 'i'), ('ㄟ', 'a'), ('ㄠ', 'w'), ('ㄡ', 'o'),
            ('ㄢ', 'm'), ('ㄣ', 'n'), ('ㄤ', 'k'), ('ㄥ', 'l'),
            ('ㄦ', 'l'),
        ].iter().cloned().collect();

        let mut res = String::new();
        for s in bpm.split_whitespace() {
            let clean = s.replace(['˙', 'ˊ', 'ˇ', 'ˋ'], "");
            if let Some(first) = clean.chars().next() {
                if let Some(&code) = initial_map.get(&first) {
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
    #[serde(skip)]
    pub watched_files: Vec<WatchedLexiconFile>,
    #[serde(skip)]
    pub last_check_instant: Option<std::time::Instant>,
}

impl LexiconManager {
    pub fn new() -> Self {
        Self::default()
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
        let p = path.as_ref();
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
                let weight: u32 = parts.get(2).and_then(|w| w.parse().ok()).unwrap_or(150);
                let cat = parts.get(3).unwrap_or(&default_cat).trim();
                let note = parts.get(4).unwrap_or(&"").trim();

                let entry = LexiconEntry::new(word, bpm, weight, cat, note);
                if cat == "user" || default_cat == "user" {
                    self.upsert_entry(entry);
                    let (sub_entries, _) = Self::decompose_entity_subentries(word, bpm, weight);
                    for sub in sub_entries {
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
                .map(|s| s == "lexicon_domain.tsv" || s == "user_dict.tsv")
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
    pub fn upsert_entry(&mut self, entry: LexiconEntry) {
        if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
            return;
        }
        let norm_bpm = LexiconEntry::normalize_bpm(&entry.bopomofo);
        if let Some(indices) = self.word_index.get(&entry.word) {
            for &idx in indices {
                let e_bpm = &self.entries[idx].bopomofo;
                if e_bpm == &entry.bopomofo || LexiconEntry::normalize_bpm(e_bpm) == norm_bpm {
                    if entry.weight > self.entries[idx].weight {
                        self.entries[idx].weight = entry.weight;
                    }
                    return;
                }
            }
        }
        self.add_entry(entry);
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
                                    let weight: u32 = parts.get(2).and_then(|w| w.parse().ok()).unwrap_or(150);
                                    let cat = parts.get(3).unwrap_or(&default_cat).trim();
                                    let note = parts.get(4).unwrap_or(&"").trim();
                                    file_entries.push(LexiconEntry::new(word, bpm, weight, cat, note));
                                    if cat == "user" || default_cat == "user" {
                                        let (sub_entries, _) = Self::decompose_entity_subentries(word, bpm, weight);
                                        file_entries.extend(sub_entries);
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

            for entry in file_entries {
                if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
                    continue;
                }
                let norm_bpm = LexiconEntry::normalize_bpm(&entry.bopomofo);
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
            | ("的數位", _)
            | ("構與模", _)
            | ("開駛", _)
            | ("簡釋", _)
            | ("衣胞", _)
            | ("遺業", _)
            | ("縊死", _)
            | ("熠燿", _)
            | ("擬去", _)
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
            | ("吃", "ㄐㄧˊ")
            | ("有庳", _)
            | ("汙庳", _)
            | ("庳", _)
    )
}

    pub fn add_entry(&mut self, mut entry: LexiconEntry) {
        if Self::is_archaic_polyphone(&entry.word, &entry.bopomofo) {
            return;
        }

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

        let norm_bpm = LexiconEntry::normalize_bpm(&entry.bopomofo);
        let incoming_cit = LexiconEntry::citation_sandhi_bpm(&entry.word, &norm_bpm);
        let incoming_is_moe = matches!(
            entry.category.as_str(),
            "core"
                | "core_compound"
                | "common"
                | "modern"
                | "classic"
                | "archaic"
                | "rare"
                | "saying"
                | "twister"
                | "char"
                | "word2"
                | "word3"
                | "word4"
                | "moe"
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
            // 確保本調與變調注音皆能索引至同一節點（如 ㄅㄨˋ ㄗㄞˋ 與 ㄅㄨˊ ㄗㄞˋ 共享「不再 591」）
            let list = self.bopomofo_index.entry(norm_bpm.clone()).or_default();
            if !list.contains(&existing_idx) {
                list.push(existing_idx);
            }
            if let Some(cit_bpm) = incoming_cit {
                let cit_list = self.bopomofo_index.entry(cit_bpm).or_default();
                if !cit_list.contains(&existing_idx) {
                    cit_list.push(existing_idx);
                }
            }
            return;
        }

        // 若萌典複詞因字面與小麥同形而誤獲 420 齊頭權重，但讀音實為小麥未收錄之生僻古音/又音，自動降回冷僻詞基礎權重 80
        if incoming_is_moe && has_mcb_other_reading && char_count >= 2 && entry.weight >= 400 {
            entry.weight = 80;
        }

        let idx = self.entries.len();
        let notone = LexiconEntry::strip_tones(&entry.bopomofo);
        let headcode = entry.headcode.clone();

        self.word_index.entry(entry.word.clone()).or_default().push(idx);
        self.bopomofo_index.entry(entry.bopomofo.clone()).or_default().push(idx);
        if norm_bpm != entry.bopomofo {
            self.bopomofo_index.entry(norm_bpm).or_default().push(idx);
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

        // 2. 四字複姓中文人名拆分（如「歐陽娜娜」-> 姓「歐陽」+ 名「娜娜」）
        const COMPOUND_SURNAMES: &[&str] = &[
            "歐陽", "司馬", "上官", "諸葛", "司徒", "司空", "獨孤", "慕容", "皇甫", "尉遲",
            "公孫", "長孫", "宇文", "軒轅", "令狐", "鍾離", "端木", "東方", "西門", "南宮",
            "百里", "東郭", "南門", "呼延", "羊舌", "微生", "范姜", "張簡",
        ];
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
                sub_transitions.push((prefix2, given_name, 28.0));
                return (sub_entries, sub_transitions);
            }
        }

        // 3. 三字單姓中文人名拆分（如「吳玟蓉」->「玟蓉」、「曹大十」->「大十」）
        const SINGLE_SURNAMES: &str = "陳林黃張李王吳劉蔡楊許鄭謝郭洪曾邱廖賴周徐蘇葉莊呂江何蕭羅高簡朱鍾施游詹沈彭胡余盧潘顏梁趙柯翁魏方孫戴范宋鄧杜侯曹薛傅丁溫紀蔣歐藍連唐馬董石卓程姚康馮古姜湯汪白田涂鄒巫尤鐘龔嚴韓袁金童陸夏柳凃邵錢盧莫秦戴賈韋剛萬龐樊伍倪祝葛苗池季向谷涂卜焦巴甘紀刁車申穆解應宗宣郁全邊姬連昝";
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
                sub_transitions.push((surname, given_name, 28.0));
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

        results
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
                    // 上箭頭旨在智慧糾錯救濟，原字若為誤拼或已有更優詞，適度退居真實救援詞之後
                    if entry.word == orig_word {
                        score -= 160.0;
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

    /// 亞洲語文軌 `↑` 上箭頭：上下文語意推斷與模糊音／跨聲調智慧救濟
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

        // 輔助閉包：評估單字位 `pos` 的上下文語法與易混字救濟候選
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

            // 常見語法與同音易混字群 (Confusable Syntactic & Homophone Clusters)
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

            let mut pool: Vec<String> = Vec::new();
            for grp in confusable_groups {
                if grp.contains(&target_ch_str.as_str()) {
                    for &item in *grp {
                        let s = item.to_string();
                        if !pool.contains(&s) {
                            pool.push(s);
                        }
                    }
                }
            }

            // 加入同音、異調與平捲舌／鼻音模糊單字候選
            for c in self.lookup_char_candidates_with_bpm(target_ch, target_bpm) {
                if !pool.contains(&c) {
                    pool.push(c);
                }
            }
            let mut base_bpms = Vec::new();
            if let Some(b) = target_bpm {
                base_bpms.push(b.to_string());
            }
            if let Some(indices) = self.word_index.get(&target_ch_str) {
                for &i in indices.iter().take(3) {
                    let b = &self.entries[i].bopomofo;
                    if !base_bpms.contains(b) {
                        base_bpms.push(b.clone());
                    }
                }
            }
            for b in &base_bpms {
                for fb in Self::get_fuzzy_bpms(b) {
                    for e in self.lookup_bopomofo(&fb, false) {
                        if e.word.chars().count() == 1 && !pool.contains(&e.word) {
                            pool.push(e.word.clone());
                        }
                    }
                }
            }

            if pool.len() > 48 {
                pool.truncate(48);
            }

            let mut scored_cands: Vec<(String, f64)> = Vec::with_capacity(pool.len());
            let mut orig_contextual_score = 0.0;

            for (rank_idx, cand) in pool.iter().enumerate() {
                let mut score = (50usize.saturating_sub(rank_idx)) as f64 * 2.0;

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
                    score -= 40.0; // 鼓勵將真正需要替換的救濟候選排至第 1 位
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
                // 若該雙字視窗本身已是詞庫中的高頻合法複合詞（如「可能」、「答案」），不視為拼寫錯誤，交由後續助詞推斷或單字救濟處理
                if cur_weight >= 400 {
                    continue;
                }
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

                // 若找到高信心救濟雙字詞（例如「答按」找到「答案」，且該詞不等於原詞或原詞非合法詞）
                if let Some((best_w, best_score)) = scored.first() {
                    if (best_w != &win_str && *best_score >= 450.0) || (cur_weight == 0 && *best_score >= 300.0) {
                        let mut cands: Vec<String> = scored.into_iter().map(|(w, _)| w).collect();
                        if !cands.contains(&win_str) {
                            cands.push(win_str);
                        }
                        return (s, (s, e), cands);
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

        // 4. 單字位上下文語法推斷與模糊同音救濟
        let (single_cands, _) = eval_single_char_rescue(idx, transitions);
        (idx, (idx, idx + 1), single_cands)
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

    #[test]
    fn test_rejected_readings_and_reachability_ledgers() {
        let mut mgr = LexiconManager::new();
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = mgr.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");

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
}
