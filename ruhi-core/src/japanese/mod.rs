/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/japanese/mod.rs - 日文羅馬字（Romaji）平假名／片假名自動機與漢字連文節智慧轉換引擎

use crate::engine::EngineResponse;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct JapaneseEntry {
    pub reading_hira: String,
    pub surface: String,
    pub weight: u32,
}

#[derive(Debug, Clone)]
pub struct JapaneseEngine {
    /// 當前輸入之原始羅馬字緩衝區
    pub raw_romaji: String,
    /// 已轉換之平假名緩衝區（含未決尾碼如單獨 'n' 或子音前綴）
    pub hira_buffer: String,
    /// 尚未構成完整假名之尾端羅馬字（如 "k", "sh", "ch", "ts", "n"）
    pub pending_romaji: String,
    /// 平假名讀音 -> 候選詞列表（依權重降序）
    pub lexicon: HashMap<String, Vec<JapaneseEntry>>,
    /// 當前候選詞清單
    pub candidates: Vec<String>,
    /// 當前選中候選索引
    pub candidate_idx: usize,
    /// 強制英文直通模式（大寫開頭或識別字）
    pub is_english_forced: bool,
}

impl Default for JapaneseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JapaneseEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            raw_romaji: String::new(),
            hira_buffer: String::new(),
            pending_romaji: String::new(),
            lexicon: HashMap::new(),
            candidates: Vec::new(),
            candidate_idx: 0,
            is_english_forced: false,
        };
        engine.load_builtin_lexicon();
        engine
    }

    pub fn reset(&mut self) {
        self.raw_romaji.clear();
        self.hira_buffer.clear();
        self.pending_romaji.clear();
        self.candidates.clear();
        self.candidate_idx = 0;
        self.is_english_forced = false;
    }

    pub fn has_buffer(&self) -> bool {
        !self.raw_romaji.is_empty()
    }

    pub fn add_lexicon_entry(&mut self, reading_hira: &str, surface: &str, weight: u32) {
        let list = self.lexicon.entry(reading_hira.to_string()).or_default();
        if let Some(existing) = list.iter_mut().find(|e| e.surface == surface) {
            if weight > existing.weight {
                existing.weight = weight;
            }
        } else {
            list.push(JapaneseEntry {
                reading_hira: reading_hira.to_string(),
                surface: surface.to_string(),
                weight,
            });
        }
        list.sort_by(|a, b| b.weight.cmp(&a.weight));
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let surface = parts[0].trim();
                let reading = parts[1].trim();
                let weight: u32 = parts.get(2).and_then(|w| w.trim().parse().ok()).unwrap_or(1000);
                self.add_lexicon_entry(reading, surface, weight);
                count += 1;
            }
        }
        Ok(count)
    }

    /// 將平假名字串轉為全形片假名（ひらがな U+3041..U+3096 ➔ カタカナ U+30A1..U+30F6）
    pub fn hira_to_kata(hira: &str) -> String {
        hira.chars()
            .map(|c| {
                let code = c as u32;
                if (0x3041..=0x3096).contains(&code) {
                    std::char::from_u32(code + 0x60).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect()
    }

    /// 將原始羅馬字完整轉換為 (已確定平假名, 未決尾碼)
    pub fn convert_romaji_stream(raw: &str) -> (String, String) {
        let lower = raw.to_ascii_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        let mut hira = String::new();
        let mut i = 0;
        let n = chars.len();

        let is_vowel = |c: char| matches!(c, 'a' | 'i' | 'u' | 'e' | 'o');

        while i < n {
            let rem = n - i;
            let ch = chars[i];

            // 1. 長音符號 '-'
            if ch == '-' {
                hira.push('ー');
                i += 1;
                continue;
            }

            // 2. 明確撥音 "nn" 或 "n'"
            if ch == 'n' && rem >= 2 && (chars[i + 1] == 'n' || chars[i + 1] == '\'') {
                hira.push('ん');
                i += 2;
                continue;
            }

            // 3. 單 'n' 後接非母音、非 'y'、非 'n' 之子音 -> 自動結算為撥音 'ん'
            if ch == 'n' && rem >= 2 {
                let next = chars[i + 1];
                if !is_vowel(next) && next != 'y' && next != 'n' && next != '\'' {
                    hira.push('ん');
                    i += 1;
                    continue;
                }
            }

            // 4. 促音（雙子音重疊，如 kk, ss, tt, pp, cc, gg, zz, dd, bb, tch）
            if rem >= 3 && chars[i] == 't' && chars[i + 1] == 'c' && chars[i + 2] == 'h' {
                hira.push('っ');
                i += 1;
                continue;
            }
            if rem >= 2
                && chars[i] == chars[i + 1]
                && chars[i].is_ascii_alphabetic()
                && !is_vowel(chars[i])
                && chars[i] != 'n'
            {
                hira.push('っ');
                i += 1;
                continue;
            }

            // 5. 嘗試匹配 3 碼羅馬字音節（拗音與特殊音節）
            if rem >= 3 {
                let s3: String = chars[i..i + 3].iter().collect();
                if let Some(mapped) = Self::lookup_romaji_table(&s3) {
                    hira.push_str(mapped);
                    i += 3;
                    continue;
                }
            }

            // 6. 嘗試匹配 2 碼羅馬字音節
            if rem >= 2 {
                let s2: String = chars[i..i + 2].iter().collect();
                if let Some(mapped) = Self::lookup_romaji_table(&s2) {
                    hira.push_str(mapped);
                    i += 2;
                    continue;
                }
            }

            // 7. 嘗試匹配 1 碼母音（a, i, u, e, o）
            let s1: String = chars[i..i + 1].iter().collect();
            if let Some(mapped) = Self::lookup_romaji_table(&s1) {
                hira.push_str(mapped);
                i += 1;
                continue;
            }

            // 8. 剩餘無法立即配對之尾端字元（如打字進行中的子音 k, sh, ch, ts, n）
            let pending: String = chars[i..].iter().collect();
            return (hira, pending);
        }

        (hira, String::new())
    }

    /// 當結算（空白鍵／Enter／選字）時，若尾端為單獨 'n'，自動收斂為撥音 'ん'
    pub fn finalized_hira(&self) -> String {
        if self.pending_romaji == "n" {
            format!("{}ん", self.hira_buffer)
        } else if self.pending_romaji.is_empty() {
            self.hira_buffer.clone()
        } else {
            format!("{}{}", self.hira_buffer, self.pending_romaji)
        }
    }

    fn lookup_romaji_table(rom: &str) -> Option<&'static str> {
        match rom {
            // 母音
            "a" => Some("あ"), "i" => Some("い"), "u" => Some("う"), "e" => Some("え"), "o" => Some("お"),
            // か行
            "ka" => Some("か"), "ki" => Some("き"), "ku" => Some("く"), "ke" => Some("け"), "ko" => Some("こ"),
            "kya" => Some("きゃ"), "kyu" => Some("きゅ"), "kyo" => Some("きょ"),
            // さ行
            "sa" => Some("さ"), "shi" | "si" => Some("し"), "su" => Some("す"), "se" => Some("せ"), "so" => Some("そ"),
            "sha" | "sya" => Some("しゃ"), "shu" | "syu" => Some("しゅ"), "sho" | "syo" => Some("しょ"), "she" => Some("しぇ"),
            // た行
            "ta" => Some("た"), "chi" | "ti" => Some("ち"), "tsu" | "tu" => Some("つ"), "te" => Some("て"), "to" => Some("と"),
            "cha" | "tya" | "cya" => Some("ちゃ"), "chu" | "tyu" | "cyu" => Some("ちゅ"), "cho" | "tyo" | "cyo" => Some("ちょ"), "che" => Some("ちぇ"),
            // な行
            "na" => Some("な"), "ni" => Some("に"), "nu" => Some("ぬ"), "ne" => Some("ね"), "no" => Some("の"),
            "nya" => Some("にゃ"), "nyu" => Some("にゅ"), "nyo" => Some("にょ"),
            // は行
            "ha" => Some("は"), "hi" => Some("ひ"), "fu" | "hu" => Some("ふ"), "he" => Some("へ"), "ho" => Some("ほ"),
            "hya" => Some("ひゃ"), "hyu" => Some("ひゅ"), "hyo" => Some("ひょ"),
            "fa" => Some("ふぁ"), "fi" => Some("ふぃ"), "fe" => Some("ふぇ"), "fo" => Some("ふぉ"),
            // ま行
            "ma" => Some("ま"), "mi" => Some("み"), "mu" => Some("む"), "me" => Some("め"), "mo" => Some("も"),
            "mya" => Some("みゃ"), "myu" => Some("みゅ"), "myo" => Some("みょ"),
            // や行
            "ya" => Some("や"), "yu" => Some("ゆ"), "yo" => Some("よ"),
            // ら行
            "ra" | "la" => Some("ら"), "ri" | "li" => Some("り"), "ru" | "lu" => Some("る"), "re" | "le" => Some("れ"), "ro" | "lo" => Some("ろ"),
            "rya" | "lya" => Some("りゃ"), "ryu" | "lyu" => Some("りゅ"), "ryo" | "lyo" => Some("りょ"),
            // わ行
            "wa" => Some("わ"), "wi" => Some("うぃ"), "we" => Some("うぇ"), "wo" => Some("を"),
            // が行（濁音）
            "ga" => Some("が"), "gi" => Some("ぎ"), "gu" => Some("ぐ"), "ge" => Some("げ"), "go" => Some("ご"),
            "gya" => Some("ぎゃ"), "gyu" => Some("ぎゅ"), "gyo" => Some("ぎょ"),
            // ざ行（濁音）
            "za" => Some("ざ"), "ji" | "zi" => Some("じ"), "zu" => Some("ず"), "ze" => Some("ぜ"), "zo" => Some("ぞ"),
            "ja" | "jya" | "zya" => Some("じゃ"), "ju" | "jyu" | "zyu" => Some("じゅ"), "jo" | "jyo" | "zyo" => Some("じょ"), "je" => Some("じぇ"),
            // だ行（濁音）
            "da" => Some("だ"), "di" => Some("ぢ"), "du" => Some("づ"), "de" => Some("で"), "do" => Some("ど"),
            "dya" => Some("ぢゃ"), "dyu" => Some("ぢゅ"), "dyo" => Some("ぢょ"),
            // ば行（濁音）
            "ba" => Some("ば"), "bi" => Some("び"), "bu" => Some("ぶ"), "be" => Some("べ"), "bo" => Some("ぼ"),
            "bya" => Some("びゃ"), "byu" => Some("びゅ"), "byo" => Some("びょ"),
            // ぱ行（半濁音）
            "pa" => Some("ぱ"), "pi" => Some("ぴ"), "pu" => Some("ぷ"), "pe" => Some("ぺ"), "po" => Some("ぽ"),
            "pya" => Some("ぴゃ"), "pyu" => Some("ぴゅ"), "pyo" => Some("ぴょ"),
            // 小假名 (la/xa...)
            "xa" => Some("ぁ"), "xi" => Some("ぃ"), "xu" => Some("ぅ"), "xe" => Some("ぇ"), "xo" => Some("ぉ"),
            "xya" => Some("ゃ"), "xyu" => Some("ゅ"), "xyo" => Some("ょ"), "xtu" | "xtsu" => Some("っ"),
            "vu" => Some("ゔ"),
            _ => None,
        }
    }

    /// 貪婪最長前綴連文節假名漢字轉換（如「わたしはにほんごがすきです」➔「私は日本語が好きです」）
    pub fn compose_kanji_sentence(&self, hira: &str) -> String {
        if hira.is_empty() {
            return String::new();
        }
        if let Some(entries) = self.lexicon.get(hira) {
            if let Some(first) = entries.first() {
                return first.surface.clone();
            }
        }
        let chars: Vec<char> = hira.chars().collect();
        let n = chars.len();
        let mut out = String::new();
        let mut i = 0;
        while i < n {
            let mut matched_len = 0;
            let mut matched_surface = String::new();
            let max_span = (n - i).min(12);
            for span in (2..=max_span).rev() {
                let sub: String = chars[i..i + span].iter().collect();
                if let Some(entries) = self.lexicon.get(&sub) {
                    if let Some(best) = entries.first() {
                        matched_len = span;
                        matched_surface = best.surface.clone();
                        break;
                    }
                }
            }
            if matched_len == 0 {
                // 嘗試單假名詞彙（僅限高權重實詞如 私/今/本/車/駅，避免助詞 は/が/を/に 被誤轉為漢字）
                let sub1: String = chars[i..i + 1].iter().collect();
                let is_particle = matches!(
                    chars[i],
                    'は' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の' | 'か' | 'ね' | 'よ'
                );
                if !is_particle {
                    if let Some(entries) = self.lexicon.get(&sub1) {
                        if let Some(best) = entries.first() {
                            if best.weight >= 1450 {
                                matched_len = 1;
                                matched_surface = best.surface.clone();
                            }
                        }
                    }
                }
            }
            if matched_len > 0 {
                out.push_str(&matched_surface);
                i += matched_len;
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    }

    pub fn update_candidates(&mut self) {
        self.candidates.clear();
        self.candidate_idx = 0;
        if self.raw_romaji.is_empty() {
            return;
        }
        if self.is_english_forced {
            self.candidates.push(self.raw_romaji.clone());
            return;
        }
        let (hira, pending) = Self::convert_romaji_stream(&self.raw_romaji);
        self.hira_buffer = hira;
        self.pending_romaji = pending;

        let full_hira = self.finalized_hira();
        let full_kata = Self::hira_to_kata(&full_hira);

        // 1. 精確命中整詞漢字／片假名候選
        if let Some(entries) = self.lexicon.get(&full_hira) {
            for e in entries {
                if !self.candidates.contains(&e.surface) {
                    self.candidates.push(e.surface.clone());
                }
            }
        }

        // 2. 連文節組合候選
        let composed = self.compose_kanji_sentence(&full_hira);
        if !composed.is_empty() && !self.candidates.contains(&composed) {
            self.candidates.insert(0, composed);
        }

        // 3. 平假名、片假名、原始羅馬字保底候選
        if !full_hira.is_empty() && !self.candidates.contains(&full_hira) {
            self.candidates.push(full_hira);
        }
        if !full_kata.is_empty() && !self.candidates.contains(&full_kata) {
            self.candidates.push(full_kata);
        }
        if !self.candidates.contains(&self.raw_romaji) {
            self.candidates.push(self.raw_romaji.clone());
        }
    }

    /// 依指定羅馬字串查詢 `↓` 下箭頭候選（漢字、連文節、平假名、片假名、羅馬字）
    pub fn lookup_down_candidates_for_romaji(&self, romaji: &str) -> Vec<String> {
        if romaji.is_empty() {
            return Vec::new();
        }
        let (hira, pending) = Self::convert_romaji_stream(romaji);
        let full_hira = if pending == "n" {
            format!("{}ん", hira)
        } else if pending.is_empty() {
            hira
        } else {
            format!("{}{}", hira, pending)
        };
        let full_kata = Self::hira_to_kata(&full_hira);
        let mut cands = Vec::new();
        if let Some(entries) = self.lexicon.get(&full_hira) {
            for e in entries {
                if !cands.contains(&e.surface) {
                    cands.push(e.surface.clone());
                }
            }
        }
        let composed = self.compose_kanji_sentence(&full_hira);
        if !composed.is_empty() && !cands.contains(&composed) {
            cands.insert(0, composed);
        }
        if !full_hira.is_empty() && !cands.contains(&full_hira) {
            cands.push(full_hira);
        }
        if !full_kata.is_empty() && !cands.contains(&full_kata) {
            cands.push(full_kata);
        }
        if !cands.contains(&romaji.to_string()) {
            cands.push(romaji.to_string());
        }
        cands
    }

    /// 依畫面已顯示之日文漢字／假名反查同音候選與平／片假名變體
    pub fn lookup_down_candidates_for_surface(&self, surface: &str) -> Vec<String> {
        if surface.is_empty() {
            return Vec::new();
        }
        let mut cands = Vec::new();
        if let Some(entries) = self.lexicon.get(surface) {
            for e in entries {
                if !cands.contains(&e.surface) {
                    cands.push(e.surface.clone());
                }
            }
        }
        for (reading, entries) in &self.lexicon {
            if entries.iter().any(|e| e.surface == surface) {
                for e in entries {
                    if !cands.contains(&e.surface) {
                        cands.push(e.surface.clone());
                    }
                }
                if !cands.contains(reading) {
                    cands.push(reading.clone());
                }
                let kata = Self::hira_to_kata(reading);
                if !cands.contains(&kata) {
                    cands.push(kata);
                }
            }
        }
        let kata_surf = Self::hira_to_kata(surface);
        if kata_surf != surface && !cands.contains(&kata_surf) {
            cands.push(kata_surf);
        }
        cands
    }

    /// `↑` 上箭頭：依指定羅馬字串進行日文前綴預測補全與長詞展開
    pub fn lookup_up_predictions_for_romaji(&self, romaji: &str) -> Vec<String> {
        if romaji.is_empty() {
            return Vec::new();
        }
        let (hira, pending) = Self::convert_romaji_stream(romaji);
        let full_hira = if pending == "n" {
            format!("{}ん", hira)
        } else if pending.is_empty() {
            hira
        } else {
            format!("{}{}", hira, pending)
        };
        self.lookup_up_predictions_for_hira(&full_hira)
    }

    pub fn lookup_up_predictions_for_hira(&self, full_hira: &str) -> Vec<String> {
        if full_hira.is_empty() {
            return Vec::new();
        }
        let mut matches: Vec<(String, u32, usize)> = Vec::new();
        for (reading, entries) in &self.lexicon {
            if reading.starts_with(full_hira) && reading.chars().count() >= full_hira.chars().count() {
                for e in entries {
                    matches.push((e.surface.clone(), e.weight, reading.chars().count()));
                }
            }
        }
        matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        let mut res = Vec::new();
        for (surf, _, _) in matches {
            if !res.contains(&surf) {
                res.push(surf);
            }
            if res.len() >= 12 {
                break;
            }
        }
        let kata = Self::hira_to_kata(full_hira);
        if !res.contains(&full_hira.to_string()) {
            res.push(full_hira.to_string());
        }
        if !res.contains(&kata) {
            res.push(kata);
        }
        res
    }

    /// `↑` 上箭頭：日文前綴預測補全與長詞展開（如輸入 `ariga` + `↑` ➔ `ありがとう`, `ありがとうございます`）
    pub fn lookup_up_arrow_predictions(&self) -> Vec<String> {
        let full_hira = self.finalized_hira();
        self.lookup_up_predictions_for_hira(&full_hira)
    }

    pub fn make_preedit(&self) -> String {
        if self.raw_romaji.is_empty() {
            return String::new();
        }
        if self.is_english_forced {
            return self.raw_romaji.clone();
        }
        format!("{}{}", self.hira_buffer, self.pending_romaji)
    }

    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        match ch {
            ' ' => {
                if self.raw_romaji.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: " ".to_string(),
                    };
                }
                self.update_candidates();
                let commit_text = self
                    .candidates
                    .get(self.candidate_idx)
                    .cloned()
                    .unwrap_or_else(|| self.finalized_hira());
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: commit_text,
                }
            }
            '\n' | '\r' => {
                if self.raw_romaji.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                self.update_candidates();
                let commit_text = if self.is_english_forced {
                    self.raw_romaji.clone()
                } else {
                    self.candidates
                        .get(self.candidate_idx)
                        .cloned()
                        .unwrap_or_else(|| self.finalized_hira())
                };
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: commit_text,
                }
            }
            '\x08' => {
                if self.raw_romaji.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                self.raw_romaji.pop();
                if self.raw_romaji.is_empty() {
                    self.reset();
                    return EngineResponse {
                        action: "reset".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                self.update_candidates();
                let preedit = self.make_preedit();
                let active = self.candidates.first().cloned().unwrap_or_default();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: "japanese".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: self.raw_romaji.len(),
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            '\x1b' => {
                self.reset();
                EngineResponse {
                    action: "reset".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            '1'..='9' if !self.raw_romaji.is_empty() => {
                let idx = (ch as u8 - b'1') as usize;
                if idx < self.candidates.len() {
                    let commit_text = self.candidates[idx].clone();
                    self.reset();
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: commit_text,
                    }
                } else {
                    let preedit = self.make_preedit();
                    let active = self.candidates.first().cloned().unwrap_or_default();
                    EngineResponse {
                        action: "compose".to_string(),
                        mode: "japanese".to_string(),
                        preedit,
                        composed: active.clone(),
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: self.candidate_idx,
                        cursor_pos: self.raw_romaji.len(),
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == '-' || (c == '\'' && !self.raw_romaji.is_empty()) => {
                if c.is_ascii_uppercase() {
                    self.is_english_forced = true;
                }
                self.raw_romaji.push(c);
                self.update_candidates();
                let preedit = self.make_preedit();
                let active = self.candidates.first().cloned().unwrap_or_default();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: "japanese".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: self.raw_romaji.len(),
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            _ => {
                let punct = match ch {
                    ',' => "、",
                    '.' => "。",
                    '?' => "？",
                    '!' => "！",
                    '[' => "「",
                    ']' => "」",
                    _ => "",
                };
                if self.raw_romaji.is_empty() {
                    let out = if !punct.is_empty() { punct.to_string() } else { ch.to_string() };
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: out,
                    }
                } else {
                    self.update_candidates();
                    let base = self
                        .candidates
                        .get(self.candidate_idx)
                        .cloned()
                        .unwrap_or_else(|| self.finalized_hira());
                    let tail = if !punct.is_empty() { punct.to_string() } else { ch.to_string() };
                    self.reset();
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: format!("{}{}", base, tail),
                    }
                }
            }
        }
    }

    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        match key_str {
            "Space" => self.process_key(' '),
            "Return" | "Enter" => self.process_key('\n'),
            "BackSpace" | "backspace" | "\x08" => self.process_key('\x08'),
            "Escape" => self.process_key('\x1b'),
            "Down" | "down" => {
                if !self.raw_romaji.is_empty() {
                    self.update_candidates();
                    let active = self.candidates.first().cloned().unwrap_or_default();
                    return EngineResponse {
                        action: "show_candidate_popup".to_string(),
                        mode: "candidates".to_string(),
                        preedit: self.make_preedit(),
                        composed: active.clone(),
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: self.candidate_idx,
                        cursor_pos: self.raw_romaji.len(),
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                EngineResponse {
                    action: "none".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            "Up" | "up" => {
                if !self.raw_romaji.is_empty() {
                    let preds = self.lookup_up_arrow_predictions();
                    if !preds.is_empty() {
                        self.candidates = preds.clone();
                        self.candidate_idx = 0;
                        let first = preds[0].clone();
                        return EngineResponse {
                            action: "show_candidate_popup".to_string(),
                            mode: "candidates".to_string(),
                            preedit: self.make_preedit(),
                            composed: first.clone(),
                            candidates: preds,
                            active_candidate: first,
                            candidate_idx: 0,
                            cursor_pos: self.raw_romaji.len(),
                            q_press_count: 0,
                            committed_chunk: String::new(),
                        };
                    }
                }
                EngineResponse {
                    action: "none".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            s if s.chars().count() == 1 => {
                let ch = s.chars().next().unwrap();
                self.process_key(ch)
            }
            _ => EngineResponse {
                action: "none".to_string(),
                mode: "japanese".to_string(),
                preedit: self.make_preedit(),
                composed: self.candidates.first().cloned().unwrap_or_default(),
                candidates: self.candidates.clone(),
                active_candidate: self.candidates.first().cloned().unwrap_or_default(),
                candidate_idx: self.candidate_idx,
                cursor_pos: self.raw_romaji.len(),
                q_press_count: 0,
                committed_chunk: String::new(),
            },
        }
    }

    fn load_builtin_lexicon(&mut self) {
        const BUILTIN: &[(&str, &str, u32)] = &[
            // 常用寒暄與慣用句
            ("ありがとう", "ありがとう", 1650),
            ("ありがとう", "有難う", 1450),
            ("ありがとうございます", "ありがとうございます", 1700),
            ("こんにちは", "こんにちは", 1650),
            ("こんにちは", "今日は", 1400),
            ("こんばんは", "こんばんは", 1650),
            ("こんばんは", "今晩は", 1400),
            ("おはよう", "おはよう", 1650),
            ("おはようございます", "おはようございます", 1700),
            ("すみません", "すみません", 1650),
            ("ごめんなさい", "ごめんなさい", 1600),
            ("よろしく", "よろしく", 1600),
            ("よろしくおねがいします", "よろしくお願いします", 1750),
            ("おねがい", "お願い", 1600),
            ("おねがいします", "お願いします", 1700),
            ("おげんきですか", "お元気ですか", 1650),
            ("げんき", "元気", 1600),
            ("だいじょうぶ", "大丈夫", 1650),
            ("なるほど", "なるほど", 1600),
            ("おつかれさま", "お疲れ様", 1650),
            ("おつかれさまでした", "お疲れ様でした", 1700),
            ("いただきます", "いただきます", 1650),
            ("ごちそうさまでした", "ご馳走様でした", 1650),
            ("はじめまして", "初めまして", 1650),
            ("さようなら", "さようなら", 1600),
            ("おやすみ", "おやすみ", 1600),
            ("おやすみなさい", "おやすみなさい", 1650),
            // 代名詞與人稱
            ("わたし", "私", 1700),
            ("ぼく", "僕", 1600),
            ("あなた", "貴方", 1500),
            ("かれ", "彼", 1550),
            ("かのじょ", "彼女", 1600),
            ("みんな", "皆", 1550),
            ("みなさん", "皆さん", 1650),
            ("せんせい", "先生", 1650),
            ("がくせい", "学生", 1600),
            ("ともだち", "友達", 1600),
            ("かぞく", "家族", 1600),
            // 國家、城市與地標
            ("にほん", "日本", 1700),
            ("にっぽん", "日本", 1650),
            ("にほんご", "日本語", 1700),
            ("たいわん", "台湾", 1700),
            ("たいぺい", "台北", 1650),
            ("とうきょう", "東京", 1700),
            ("おおさか", "大阪", 1650),
            ("きょうと", "京都", 1650),
            ("ほっかいどう", "北海道", 1650),
            ("おきなわ", "沖縄", 1600),
            ("ふじさん", "富士山", 1650),
            ("せかい", "世界", 1650),
            // 時間與日常名詞
            ("いま", "今", 1600),
            ("きょう", "今日", 1650),
            ("あした", "明日", 1650),
            ("きのう", "昨日", 1650),
            ("まいにち", "毎日", 1600),
            ("じかん", "時間", 1650),
            ("でんしゃ", "電車", 1650),
            ("しんかんせん", "新幹線", 1700),
            ("えき", "駅", 1600),
            ("くうこう", "空港", 1600),
            ("がっこう", "学校", 1650),
            ("だいがく", "大学", 1650),
            ("かいしゃ", "会社", 1650),
            ("しごと", "仕事", 1650),
            ("でんわ", "電話", 1600),
            ("りょこう", "旅行", 1650),
            ("たべもの", "食べ物", 1650),
            ("のみもの", "飲み物", 1650),
            ("おちゃ", "お茶", 1600),
            ("みず", "水", 1550),
            ("ほん", "本", 1550),
            ("おんがく", "音楽", 1600),
            ("えいが", "映画", 1600),
            ("しゃしん", "写真", 1600),
            ("てんき", "天気", 1600),
            ("こころ", "心", 1600),
            ("へいわ", "平和", 1600),
            ("じゆう", "自由", 1600),
            ("きぼう", "希望", 1600),
            ("みらい", "未来", 1600),
            ("れきし", "歴史", 1600),
            ("ぶんか", "文化", 1600),
            ("かがく", "科学", 1600),
            ("ぎじゅつ", "技術", 1600),
            ("はる", "春", 1550),
            ("なつ", "夏", 1550),
            ("あき", "秋", 1550),
            ("ふゆ", "冬", 1550),
            ("さくら", "桜", 1600),
            ("かぜ", "風", 1550),
            ("そら", "空", 1550),
            ("うみ", "海", 1550),
            ("やま", "山", 1550),
            ("かわ", "川", 1550),
            ("はな", "花", 1550),
            ("あめ", "雨", 1550),
            ("ゆき", "雪", 1550),
            ("ほし", "星", 1550),
            ("つき", "月", 1550),
            ("ひかり", "光", 1550),
            // 常用形容詞與動詞
            ("すき", "好き", 1650),
            ("だいすき", "大好き", 1650),
            ("おいしい", "美味しい", 1650),
            ("かわいい", "可愛い", 1650),
            ("たのしい", "楽しい", 1650),
            ("うれしい", "嬉しい", 1650),
            ("むずかしい", "難しい", 1600),
            ("やさしい", "優しい", 1600),
            ("あたらしい", "新しい", 1600),
            ("おおきい", "大きい", 1600),
            ("ちいさい", "小さい", 1600),
            ("たべる", "食べる", 1600),
            ("のむ", "飲む", 1600),
            ("いく", "行く", 1600),
            ("くる", "来る", 1600),
            ("みる", "見る", 1600),
            ("きく", "聞く", 1600),
            ("はなす", "話す", 1600),
            ("よむ", "読む", 1600),
            ("かく", "書く", 1600),
            ("わかる", "分かる", 1600),
            ("おもう", "思う", 1600),
            // 常用片假名外來語
            ("こーひー", "コーヒー", 1700),
            ("らーめん", "ラーメン", 1700),
            ("ぱそこん", "パソコン", 1650),
            ("すまほ", "スマホ", 1650),
            ("ほてる", "ホテル", 1650),
            ("こんびに", "コンビニ", 1650),
            ("てれび", "テレビ", 1650),
            ("かめら", "カメラ", 1650),
            ("あにめ", "アニメ", 1650),
            ("げーむ", "ゲーム", 1650),
        ];
        for &(reading, surface, weight) in BUILTIN {
            self.add_lexicon_entry(reading, surface, weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_romaji_to_kana_and_kanji_composition() {
        let mut jp = JapaneseEngine::new();

        // 1. 撥音與促音自動機：nihongo ➔ 日本語 (にほんご)、gakkou ➔ 学校 (がっこう)、shinkansen ➔ 新幹線
        for ch in "nihongo".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.finalized_hira(), "にほんご");
        assert_eq!(jp.candidates[0], "日本語");
        assert!(jp.candidates.contains(&"にほんご".to_string()));
        assert!(jp.candidates.contains(&"ニホンゴ".to_string()));
        let r = jp.process_key(' ');
        assert_eq!(r.committed_chunk, "日本語");

        // 2. 連文節轉換：watashihanihongogasuki ➔ 私は日本語が好き
        for ch in "watashihanihongogasuki".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "私は日本語が好き");
        jp.reset();

        // 3. 外來語長音與片假名：ko-hi- ➔ コーヒー
        for ch in "ko-hi-".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "コーヒー");
        jp.reset();

        // 4. 上箭頭 `↑` 前綴預測補全：ariga + Up ➔ ありがとうございます / ありがとう
        for ch in "ariga".chars() {
            jp.process_key(ch);
        }
        let up_resp = jp.process_key_str("Up");
        assert!(up_resp.candidates.contains(&"ありがとうございます".to_string()));
        assert!(up_resp.candidates.contains(&"ありがとう".to_string()));
    }
}
