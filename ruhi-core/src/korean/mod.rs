/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/korean/mod.rs - 韓語標準羅馬拼音（RR）諺文組字模組與狀態機

pub mod jamo;
pub mod lexicon;
pub mod romaja;

use crate::engine::EngineResponse;
pub use crate::korean::lexicon::{KoreanEntry, KoreanLexicon};
use crate::korean::romaja::{
    parse_nbest_with_lexicon, render, romaja_to_hangul_with_lexicon, ParseMode, NBEST_K,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct KoreanEngine {
    pub raw_romaja: String,
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
    pub is_english_forced: bool,
    pub lexicon: KoreanLexicon,
}

impl Default for KoreanEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl KoreanEngine {
    pub fn new() -> Self {
        Self {
            raw_romaja: String::new(),
            candidates: Vec::new(),
            candidate_idx: 0,
            is_english_forced: false,
            lexicon: KoreanLexicon::with_builtin(),
        }
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<usize> {
        self.lexicon.load_tsv(path)
    }

    pub fn reset(&mut self) {
        self.raw_romaja.clear();
        self.candidates.clear();
        self.candidate_idx = 0;
        self.is_english_forced = false;
    }

    pub fn has_buffer(&self) -> bool {
        !self.raw_romaja.is_empty()
    }

    /// 取得當前組字預覽字串（Preview 模式，搭配常用詞表）
    pub fn make_preedit(&self) -> String {
        if self.raw_romaja.is_empty() {
            return String::new();
        }
        if self.is_english_forced {
            return self.raw_romaja.clone();
        }
        romaja_to_hangul_with_lexicon(&self.raw_romaja, ParseMode::Preview, Some(&self.lexicon))
    }

    /// 查詢指定羅馬拼音之 `↓` 候選清單：N-best 結果去重，最多 9 筆
    pub fn lookup_down_candidates_for_romaja(&self, romaja: &str) -> Vec<String> {
        if romaja.is_empty() {
            return Vec::new();
        }
        let parses = parse_nbest_with_lexicon(
            romaja,
            ParseMode::Finalize,
            NBEST_K.max(9),
            Some(&self.lexicon),
        );
        let mut cands = Vec::new();
        for p in &parses {
            let s = render(p);
            if !s.is_empty() && !cands.contains(&s) {
                cands.push(s);
                if cands.len() >= 9 {
                    break;
                }
            }
        }
        if cands.len() < 9 {
            if let Some(compat_ch) = crate::korean::jamo::exact_compat_jamo(romaja) {
                let compat_str = compat_ch.to_string();
                if !cands.contains(&compat_str) {
                    cands.push(compat_str);
                }
            }
        }
        if cands.len() < 9 && !cands.contains(&romaja.to_string()) {
            cands.push(romaja.to_string());
        }
        cands
    }

    /// 查詢 `↓` 候選清單：N-best 結果去重，最多 9 筆
    pub fn lookup_down_candidates(&self) -> Vec<String> {
        if self.raw_romaja.is_empty() {
            return Vec::new();
        }
        if self.is_english_forced {
            return vec![self.raw_romaja.clone()];
        }
        self.lookup_down_candidates_for_romaja(&self.raw_romaja)
    }

    /// 查詢指定羅馬拼音之 `↑` 學習者模糊救援與前綴補全候選
    pub fn lookup_up_candidates_for_romaja(&self, romaja: &str) -> Vec<String> {
        if romaja.is_empty() {
            return Vec::new();
        }
        let input_skel = KoreanLexicon::skeleton(romaja);
        if input_skel.is_empty() {
            return Vec::new();
        }

        let mut exact_indices: Vec<usize> = Vec::new();
        if let Some(list) = self.lexicon.skeleton_index.get(&input_skel) {
            for &idx in list {
                if !exact_indices.contains(&idx) {
                    exact_indices.push(idx);
                }
            }
        }
        exact_indices.sort_by(|&a, &b| {
            let ea = &self.lexicon.entries[a];
            let eb = &self.lexicon.entries[b];
            eb.weight
                .cmp(&ea.weight)
                .then_with(|| ea.surface.chars().count().cmp(&eb.surface.chars().count()))
                .then_with(|| a.cmp(&b))
        });

        let mut prefix_indices: Vec<usize> = Vec::new();
        if input_skel.chars().count() >= 2 {
            for (skel_key, list) in &self.lexicon.skeleton_index {
                if skel_key != &input_skel && skel_key.starts_with(&input_skel) {
                    for &idx in list {
                        if !exact_indices.contains(&idx) && !prefix_indices.contains(&idx) {
                            prefix_indices.push(idx);
                        }
                    }
                }
            }
            prefix_indices.sort_by(|&a, &b| {
                let ea = &self.lexicon.entries[a];
                let eb = &self.lexicon.entries[b];
                eb.weight
                    .cmp(&ea.weight)
                    .then_with(|| ea.surface.chars().count().cmp(&eb.surface.chars().count()))
                    .then_with(|| a.cmp(&b))
            });
        }

        let mut cands = Vec::new();
        for idx in exact_indices.into_iter().chain(prefix_indices.into_iter()) {
            let surf = &self.lexicon.entries[idx].surface;
            if !cands.contains(surf) {
                cands.push(surf.clone());
                if cands.len() >= 9 {
                    break;
                }
            }
        }
        cands
    }

    /// 查詢 `↑` 學習者模糊救援與前綴補全候選：
    /// 1. 以 skeleton(raw_romaja) 在 skeleton_index 中找完全相同的條目
    /// 2. 再找骨架以輸入骨架為前綴的條目（輸入骨架至少 2 個字元才做前綴比對）
    /// 3. 各組內依「權重降序 → 音節數升序 → 加入順序」排序，去重，最多 9 筆
    pub fn lookup_up_candidates(&self) -> Vec<String> {
        if self.raw_romaja.is_empty() || self.is_english_forced {
            return Vec::new();
        }
        self.lookup_up_candidates_for_romaja(&self.raw_romaja)
    }

    pub fn update_candidates(&mut self) {
        self.candidates = self.lookup_down_candidates();
        self.candidate_idx = 0;
    }

    pub fn finalized_commit_text(&self) -> String {
        if self.is_english_forced {
            return self.raw_romaja.clone();
        }
        self.candidates
            .get(self.candidate_idx)
            .cloned()
            .unwrap_or_else(|| {
                romaja_to_hangul_with_lexicon(
                    &self.raw_romaja,
                    ParseMode::Finalize,
                    Some(&self.lexicon),
                )
            })
    }

    /// Shift+BackSpace：退回最後 1 個完整諺文音節（或先清除尾端未完成相容字母）
    pub fn pop_syllable(&mut self) -> EngineResponse {
        if self.raw_romaja.is_empty() {
            return EngineResponse {
                action: "pass".to_string(),
                mode: "korean".to_string(),
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
        if self.is_english_forced {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "korean".to_string(),
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

        while self.raw_romaja.ends_with('-') {
            self.raw_romaja.pop();
        }
        let cur_parse = parse_nbest_with_lexicon(
            &self.raw_romaja,
            ParseMode::Preview,
            1,
            Some(&self.lexicon),
        )
        .into_iter()
        .next();

        let (target_len, target_last_syl) = match cur_parse {
            Some(ref p) if !p.pending.is_empty() => {
                if p.syllables.is_empty() {
                    (0usize, None)
                } else {
                    (p.syllables.len(), p.syllables.last().cloned())
                }
            }
            Some(ref p) => {
                if p.syllables.len() <= 1 {
                    (0usize, None)
                } else {
                    let tl = p.syllables.len() - 1;
                    (tl, p.syllables.get(tl - 1).cloned())
                }
            }
            None => (0usize, None),
        };
        if false {
        let (_target_len, _target_last_t) = match cur_parse {
            Some(ref p) if !p.pending.is_empty() => {
                if p.syllables.is_empty() {
                    (0usize, None)
                } else {
                    (p.syllables.len(), p.syllables.last().map(|s| s.t))
                }
            }
            Some(ref p) => {
                if p.syllables.len() <= 1 {
                    (0usize, None)
                } else {
                    let tl = p.syllables.len() - 1;
                    (tl, p.syllables.get(tl - 1).map(|s| s.t))
                }
            }
            None => (0usize, None),
        };

        }
        if target_len == 0 {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "korean".to_string(),
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

        while !self.raw_romaja.is_empty() {
            self.raw_romaja.pop();
            while self.raw_romaja.ends_with('-') {
                self.raw_romaja.pop();
            }
            if self.raw_romaja.is_empty() {
                break;
            }
            if let Some(next_p) = parse_nbest_with_lexicon(
                &self.raw_romaja,
                ParseMode::Preview,
                1,
                Some(&self.lexicon),
            )
            .into_iter()
            .next()
            {
                if next_p.pending.is_empty() && next_p.syllables.len() <= target_len {
                if next_p.pending.is_empty() && next_p.syllables.len() <= target_len {
                    let is_exact_target = next_p.syllables.len() < target_len
                        || next_p.syllables.last().cloned() == target_last_syl;
                    if is_exact_target {
                        break;
                    }
                }
                if false {
                    let target_last_t: Option<u8> = None;
                    let absorbed_onset_into_empty_coda = target_last_t == Some(0)
                        && next_p.syllables.len() == target_len
                        && next_p.syllables.last().map(|s| s.t) != Some(0);
                    if !absorbed_onset_into_empty_coda {
                        break;
                    }
                }
                }
            }
        }

        if self.raw_romaja.is_empty() {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "korean".to_string(),
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

        self.is_english_forced = self.raw_romaja.chars().any(|c| c.is_ascii_uppercase());
        self.update_candidates();
        let preedit = self.make_preedit();
        let active = self
            .candidates
            .first()
            .cloned()
            .unwrap_or_else(|| preedit.clone());
        let cur_len = preedit.chars().count();
        EngineResponse {
            action: "compose".to_string(),
            mode: "korean".to_string(),
            preedit: preedit.clone(),
            composed: preedit,
            candidates: self.candidates.clone(),
            active_candidate: active,
            candidate_idx: 0,
            cursor_pos: cur_len,
            q_press_count: 0,
            committed_chunk: String::new(),
        }
    }

    /// Ctrl+BackSpace：一次清空當前未決韓文詞緩衝區
    pub fn pop_word(&mut self) -> EngineResponse {
        if self.raw_romaja.is_empty() {
            return EngineResponse {
                action: "pass".to_string(),
                mode: "korean".to_string(),
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
        self.reset();
        EngineResponse {
            action: "reset".to_string(),
            mode: "korean".to_string(),
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

    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        match ch {
            '\t' => {
                let preedit = self.make_preedit();
                let active = self
                    .candidates
                    .first()
                    .cloned()
                    .unwrap_or_else(|| preedit.clone());
                let cur_len = active.chars().count();
                EngineResponse {
                    action: "pass_through".to_string(),
                    mode: "korean".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: self.candidate_idx,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            ' ' => {
                if self.raw_romaja.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "korean".to_string(),
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
                if self.candidates.is_empty() {
                    self.update_candidates();
                }
                let commit_text = self.finalized_commit_text();
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "korean".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: format!("{} ", commit_text),
                }
            }
            '\n' | '\r' => {
                if self.raw_romaja.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "korean".to_string(),
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
                if self.candidates.is_empty() {
                    self.update_candidates();
                }
                let commit_text = self.finalized_commit_text();
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "korean".to_string(),
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
                if self.raw_romaja.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "korean".to_string(),
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
                self.raw_romaja.pop();
                if self.raw_romaja.is_empty() {
                    self.reset();
                    return EngineResponse {
                        action: "reset".to_string(),
                        mode: "korean".to_string(),
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
                self.is_english_forced = self.raw_romaja.chars().any(|c| c.is_ascii_uppercase());
                self.update_candidates();
                let preedit = self.make_preedit();
                let active = self
                    .candidates
                    .first()
                    .cloned()
                    .unwrap_or_else(|| preedit.clone());
                let cur_len = preedit.chars().count();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: "korean".to_string(),
                    preedit: preedit.clone(),
                    composed: preedit,
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            '\x1b' => {
                self.reset();
                EngineResponse {
                    action: "reset".to_string(),
                    mode: "korean".to_string(),
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
            '1'..='9' if !self.raw_romaja.is_empty() => {
                let idx = (ch as u8 - b'1') as usize;
                if idx < self.candidates.len() {
                    let commit_text = self.candidates[idx].clone();
                    self.reset();
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "korean".to_string(),
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
                    let active = self
                        .candidates
                        .first()
                        .cloned()
                        .unwrap_or_else(|| preedit.clone());
                    let cur_len = preedit.chars().count();
                    EngineResponse {
                        action: "compose".to_string(),
                        mode: "korean".to_string(),
                        preedit: preedit.clone(),
                        composed: preedit,
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: self.candidate_idx,
                        cursor_pos: cur_len,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == '-' => {
                if c.is_ascii_uppercase() {
                    self.is_english_forced = true;
                }
                self.raw_romaja.push(c);
                self.update_candidates();
                let preedit = self.make_preedit();
                let active = self
                    .candidates
                    .first()
                    .cloned()
                    .unwrap_or_else(|| preedit.clone());
                let cur_len = preedit.chars().count();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: "korean".to_string(),
                    preedit: preedit.clone(),
                    composed: preedit,
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            _ => {
                if self.raw_romaja.is_empty() {
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "korean".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: ch.to_string(),
                    }
                } else {
                    if self.candidates.is_empty() {
                        self.update_candidates();
                    }
                    let base = self.finalized_commit_text();
                    self.reset();
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "korean".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: format!("{}{}", base, ch),
                    }
                }
            }
        }
    }

    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        match key_str {
            "Tab" | "tab" | "\t" => self.process_key('\t'),
            "Space" => self.process_key(' '),
            "Return" | "Enter" => self.process_key('\n'),
            "BackSpace" | "backspace" | "\x08" => self.process_key('\x08'),
            "Shift+BackSpace" | "Shift+backspace" => self.pop_syllable(),
            "Ctrl+BackSpace" | "Ctrl+backspace" => self.pop_word(),
            "Escape" | "Esc" => self.process_key('\x1b'),
            "Down" | "down" => {
                if !self.raw_romaja.is_empty() {
                    self.update_candidates();
                    let preedit = self.make_preedit();
                    let active = self
                        .candidates
                        .first()
                        .cloned()
                        .unwrap_or_else(|| preedit.clone());
                    let cur_len = active.chars().count();
                    return EngineResponse {
                        action: "show_candidate_popup".to_string(),
                        mode: "candidates".to_string(),
                        preedit,
                        composed: active.clone(),
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: self.candidate_idx,
                        cursor_pos: cur_len,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                EngineResponse {
                    action: "none".to_string(),
                    mode: "korean".to_string(),
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
                if !self.raw_romaja.is_empty() {
                    let preds = self.lookup_up_candidates();
                    if !preds.is_empty() {
                        self.candidates = preds.clone();
                        self.candidate_idx = 0;
                        let first = preds[0].clone();
                        let cur_len = first.chars().count();
                        return EngineResponse {
                            action: "show_candidate_popup".to_string(),
                            mode: "candidates".to_string(),
                            preedit: self.make_preedit(),
                            composed: first.clone(),
                            candidates: preds,
                            active_candidate: first,
                            candidate_idx: 0,
                            cursor_pos: cur_len,
                            q_press_count: 0,
                            committed_chunk: String::new(),
                        };
                    }
                }
                EngineResponse {
                    action: "none".to_string(),
                    mode: "korean".to_string(),
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
            _ => {
                let preedit = self.make_preedit();
                let active = self
                    .candidates
                    .first()
                    .cloned()
                    .unwrap_or_else(|| preedit.clone());
                let cur_len = preedit.chars().count();
                EngineResponse {
                    action: "none".to_string(),
                    mode: "korean".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: self.candidate_idx,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::korean::jamo::{is_hangul_syllable, to_romaja};
    use crate::korean::lexicon::BUILTIN_LEXICON;
    use std::collections::HashSet;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;

    fn edit_distance(a: &str, b: &str) -> usize {
        let ca: Vec<char> = a.chars().collect();
        let cb: Vec<char> = b.chars().collect();
        let m = ca.len();
        let n = cb.len();
        let mut dp = vec![vec![0usize; n + 1]; m + 1];
        for i in 0..=m {
            dp[i][0] = i;
        }
        for j in 0..=n {
            dp[0][j] = j;
        }
        for i in 1..=m {
            for j in 1..=n {
                let cost = usize::from(ca[i - 1] != cb[j - 1]);
                dp[i][j] = (dp[i - 1][j] + 1)
                    .min(dp[i][j - 1] + 1)
                    .min(dp[i - 1][j - 1] + cost);
            }
        }
        dp[m][n]
    }

    #[test]
    fn test_p3_step_by_step_liaison_resegmentation() {
        let mut eng = KoreanEngine::new();
        let r1 = eng.process_key('h');
        assert_eq!(r1.preedit, "ㅎ");
        let r2 = eng.process_key('a');
        assert_eq!(r2.preedit, "하");
        let r3 = eng.process_key('n');
        assert_eq!(r3.preedit, "한");
        let r4 = eng.process_key('a');
        assert_eq!(r4.preedit, "하나");
    }

    #[test]
    fn test_p3_backspace_and_preview() {
        let mut eng = KoreanEngine::new();
        for ch in "hangeul".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.make_preedit(), "한글");
        eng.process_key_str("BackSpace");
        let resp = eng.process_key_str("BackSpace");
        assert_eq!(eng.raw_romaja, "hange");
        assert_eq!(resp.preedit, "한게");
    }

    #[test]
    fn test_p3_space_and_enter_commit() {
        let mut eng = KoreanEngine::new();
        for ch in "hangeul".chars() {
            eng.process_key(ch);
        }
        let resp_space = eng.process_key_str("Space");
        assert_eq!(resp_space.action, "commit");
        assert_eq!(resp_space.committed_chunk, "한글 ");
        assert!(!eng.has_buffer());

        for ch in "dalk".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.make_preedit(), "달ㅋ");
        let resp_enter = eng.process_key_str("Enter");
        assert_eq!(resp_enter.action, "commit");
        assert_eq!(resp_enter.committed_chunk, "닭");
        assert!(!eng.has_buffer());
    }

    #[test]
    fn test_p3_down_candidates_and_digit_selection() {
        let mut eng = KoreanEngine::new();
        for ch in "hangeul".chars() {
            eng.process_key(ch);
        }
        let resp_down = eng.process_key_str("Down");
        assert_eq!(resp_down.action, "show_candidate_popup");
        assert!(resp_down.candidates.contains(&"한글".to_string()));
        assert!(resp_down.candidates.contains(&"항을".to_string()));
        assert_eq!(resp_down.candidates[0], "한글");
        assert_eq!(resp_down.candidates[1], "항을");

        let resp_sel = eng.process_key('2');
        assert_eq!(resp_sel.action, "commit");
        assert_eq!(resp_sel.committed_chunk, "항을");
        assert!(!eng.has_buffer());
    }

    #[test]
    fn test_p3_tab_passthrough_and_escape() {
        let mut eng = KoreanEngine::new();
        for ch in "han".chars() {
            eng.process_key(ch);
        }
        let resp_tab = eng.process_key_str("Tab");
        assert_eq!(resp_tab.action, "pass_through");
        assert_eq!(eng.raw_romaja, "han");
        assert_eq!(eng.make_preedit(), "한");

        let resp_esc = eng.process_key_str("Escape");
        assert_eq!(resp_esc.action, "reset");
        assert!(!eng.has_buffer());
    }

    #[test]
    fn test_p4_designated_cases() {
        let lex = KoreanLexicon::with_builtin();
        assert_eq!(KoreanLexicon::skeleton("kimchi"), "gimji");
        assert_eq!(KoreanLexicon::skeleton("gimchi"), "gimji");
        assert_eq!(KoreanLexicon::skeleton("pusan"), "busan");

        // 1. gamsahamnida -> 감사합니다
        assert_eq!(
            romaja_to_hangul_with_lexicon("gamsahamnida", ParseMode::Finalize, Some(&lex)),
            "감사합니다"
        );
        // 2. tteokbokki -> 떡볶이
        assert_eq!(
            romaja_to_hangul_with_lexicon("tteokbokki", ParseMode::Finalize, Some(&lex)),
            "떡볶이"
        );
        // 3. gaeul -> 가을
        assert_eq!(
            romaja_to_hangul_with_lexicon("gaeul", ParseMode::Finalize, Some(&lex)),
            "가을"
        );
        // 4. gimchireul -> 김치를
        assert_eq!(
            romaja_to_hangul_with_lexicon("gimchireul", ParseMode::Finalize, Some(&lex)),
            "김치를"
        );
        // 5. kimchi (Finalize) -> 킴치（kimchi 不是精確鍵）
        assert_eq!(
            romaja_to_hangul_with_lexicon("kimchi", ParseMode::Finalize, Some(&lex)),
            "킴치"
        );

        // 6. kimchi + Up 第一筆 -> 김치
        let mut eng = KoreanEngine::new();
        for c in "kimchi".chars() {
            eng.process_key(c);
        }
        let up_kimchi = eng.process_key_str("Up");
        assert_eq!(up_kimchi.action, "show_candidate_popup");
        assert_eq!(up_kimchi.candidates.first().map(|s| s.as_str()), Some("김치"));

        // 7. pusan + Up 第一筆 -> 부산
        eng.reset();
        for c in "pusan".chars() {
            eng.process_key(c);
        }
        let up_pusan = eng.process_key_str("Up");
        assert_eq!(up_pusan.action, "show_candidate_popup");
        assert_eq!(up_pusan.candidates.first().map(|s| s.as_str()), Some("부산"));
    }

    #[test]
    fn test_p4_exhaustive_tsv_lexicon_validation() {
        let tsv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("ruhi_lexicon")
            .join("data")
            .join("korean.tsv");
        assert!(tsv_path.exists(), "korean.tsv not found at {:?}", tsv_path);

        // 先逐行檢查格式、音節合法性、權重分級與無重複 (surface, rr)
        let file = File::open(&tsv_path).expect("failed to open korean.tsv");
        let reader = BufReader::new(file);
        let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
        let mut raw_rows: Vec<(String, String, u32)> = Vec::new();

        for (line_no, line_res) in reader.lines().enumerate() {
            let line = line_res.expect("failed to read line");
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            assert!(
                parts.len() == 3,
                "Line {} does not have 3 columns: {}",
                line_no + 1,
                trimmed
            );
            let surface = parts[0].trim();
            let rr = parts[1].trim();
            let weight: u32 = parts[2]
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("Invalid weight at line {}", line_no + 1));

            // 1a. surface 只含完整諺文音節
            assert!(!surface.is_empty(), "Empty surface at line {}", line_no + 1);
            assert!(
                surface.chars().all(is_hangul_syllable),
                "Non-Hangul syllable in surface '{}' at line {}",
                surface,
                line_no + 1
            );
            // 1b. weight 只能是 1000 或 1100
            assert!(
                weight == 1000 || weight == 1100,
                "Weight must be 1000 or 1100, got {} for '{}' at line {}",
                weight,
                surface,
                line_no + 1
            );
            // 1c. 無重複 (surface, rr)
            assert!(
                seen_pairs.insert((surface.to_string(), rr.to_ascii_lowercase())),
                "Duplicate (surface, rr) pair: ({}, {}) at line {}",
                surface,
                rr,
                line_no + 1
            );
            raw_rows.push((surface.to_string(), rr.to_string(), weight));
        }

        assert!(
            (300..=500).contains(&raw_rows.len()),
            "korean.tsv entry count should be 300..=500, got {}",
            raw_rows.len()
        );

        let mut lex = KoreanLexicon::new();
        let loaded = lex.load_tsv(&tsv_path).expect("load_tsv failed");
        assert_eq!(loaded, raw_rows.len());

        let mut rr_failures = Vec::new();
        let mut romaja_failures = Vec::new();
        let mut edit_dist_failures = Vec::new();

        for (surface, rr, _weight) in &raw_rows {
            // 2. 官方拼寫可還原
            let got_rr = romaja_to_hangul_with_lexicon(rr, ParseMode::Finalize, Some(&lex));
            if &got_rr != surface {
                rr_failures.push(format!("{} (rr='{}') -> got '{}'", surface, rr, got_rr));
            }

            // 3. 逐字母拼寫可還原
            let exact_rom = to_romaja(surface);
            let got_exact =
                romaja_to_hangul_with_lexicon(&exact_rom, ParseMode::Finalize, Some(&lex));
            if &got_exact != surface {
                romaja_failures.push(format!(
                    "{} (to_romaja='{}') -> got '{}'",
                    surface, exact_rom, got_exact
                ));
            }

            // 4. 拼寫合理性：rr 與 to_romaja(surface)（去掉 '-'）的編輯距離 <= max(2, rr.len() / 4)
            let rr_clean = rr.to_ascii_lowercase().replace('-', "");
            let exact_clean = exact_rom.to_ascii_lowercase().replace('-', "");
            let dist = edit_distance(&rr_clean, &exact_clean);
            let max_allowed = 2usize.max(rr_clean.len() / 4);
            if dist > max_allowed {
                edit_dist_failures.push(format!(
                    "{}: rr='{}' vs to_romaja='{}', dist={} > max_allowed={}",
                    surface, rr_clean, exact_clean, dist, max_allowed
                ));
            }
        }

        assert!(
            rr_failures.is_empty(),
            "官方拼寫還原失敗 ({} 筆): {:?}",
            rr_failures.len(),
            rr_failures
        );
        assert!(
            romaja_failures.is_empty(),
            "逐字母拼寫還原失敗 ({} 筆): {:?}",
            romaja_failures.len(),
            romaja_failures
        );
        assert!(
            edit_dist_failures.is_empty(),
            "拼寫合理性編輯距離超標 ({} 筆): {:?}",
            edit_dist_failures.len(),
            edit_dist_failures
        );

        // 5. 內建詞表的每一條都必須出現在 korean.tsv 中
        for &(b_surf, b_rr, b_weight) in BUILTIN_LEXICON {
            assert!(
                raw_rows
                    .iter()
                    .any(|(s, r, w)| s == b_surf && r == b_rr && *w == b_weight),
                "Builtin entry ({}, {}, {}) not found in korean.tsv",
                b_surf,
                b_rr,
                b_weight
            );
        }
    }

    #[test]
    fn test_p6_finalized_commit_text() {
        let mut eng = KoreanEngine::new();

        // 1. 鍵入 'g' 後，Preview 為 'ㄱ'，但 finalized_commit_text() 必須為 "g"
        eng.process_key('g');
        assert_eq!(eng.make_preedit(), "ㄱ");
        assert_eq!(eng.finalized_commit_text(), "g");

        // 2. 鍵入 "hangeul" 後，finalized_commit_text() 為 "한글"
        eng.reset();
        for ch in "hangeul".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.finalized_commit_text(), "한글");

        // 3. 鍵入 "hangeul" + Down + 選第 2 筆後（candidate_idx = 1），finalized_commit_text() 為該候選
        let r_down = eng.process_key_str("Down");
        assert!(r_down.candidates.len() >= 2);
        let second = r_down.candidates[1].clone();
        eng.candidate_idx = 1;
        assert_eq!(eng.finalized_commit_text(), second);
    }

    /// TSF 在數字鍵前會先結算韓文緩衝區（`digit_consolidate`），
    /// 因此引擎在緩衝區為空時收到數字，必須原樣輸出數字，而不是選字。
    /// 緩衝區非空時數字仍為選字鍵（候選每次按鍵都會更新）。
    #[test]
    fn test_p6_digit_after_consolidation_is_literal() {
        let mut eng = KoreanEngine::new();
        for ch in "hangeul".chars() {
            eng.process_key(ch);
        }
        assert!(!eng.candidates.is_empty(), "候選於每次按鍵後皆已更新");
        let finalized = eng.finalized_commit_text();
        assert_eq!(finalized, "한글");
        eng.reset();

        let r = eng.process_key('2');
        assert_eq!(r.action, "commit");
        assert_eq!(r.committed_chunk, "2");
        assert!(!eng.has_buffer());

        // Shift+BackSpace / Ctrl+BackSpace 驗證
        for ch in "hangeul".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.make_preedit(), "한글");
        let r_shift = eng.process_key_str("Shift+BackSpace");
        assert_eq!(r_shift.preedit, "한");
        assert!(eng.has_buffer());
        let r_shift2 = eng.process_key_str("Shift+BackSpace");
        assert_eq!(r_shift2.action, "reset");
        assert!(!eng.has_buffer());

        for ch in "annyeonghaseyo".chars() {
            eng.process_key(ch);
        }
        let r_ctrl = eng.process_key_str("Ctrl+BackSpace");
        assert_eq!(r_ctrl.action, "reset");
        assert!(!eng.has_buffer());

        // 獨立諺文字母（Compatibility Jamo）以 `q` 前綴／後綴直出與 `↓` 候選驗證
        use crate::korean::romaja::romaja_to_hangul;
        for (q_pre, q_suf, expected) in [
            ("qa", "aq", "ㅏ"),
            ("qeo", "eoq", "ㅓ"),
            ("qo", "oq", "ㅗ"),
            ("qu", "uq", "ㅜ"),
            ("qeu", "euq", "ㅡ"),
            ("qi", "iq", "ㅣ"),
            ("qae", "aeq", "ㅐ"),
            ("qe", "eq", "ㅔ"),
            ("qyu", "yuq", "ㅠ"),
            ("qg", "gq", "ㄱ"),
            ("qk", "kq", "ㅋ"),
            ("qkk", "kkq", "ㄲ"),
            ("qd", "dq", "ㄷ"),
            ("qt", "tq", "ㅌ"),
            ("qtt", "ttq", "ㄸ"),
            ("qb", "bq", "ㅂ"),
            ("qp", "pq", "ㅍ"),
            ("qpp", "ppq", "ㅃ"),
            ("qj", "jq", "ㅈ"),
            ("qch", "chq", "ㅊ"),
            ("qjj", "jjq", "ㅉ"),
            ("qs", "sq", "ㅅ"),
            ("qss", "ssq", "ㅆ"),
            ("qn", "nq", "ㄴ"),
            ("qm", "mq", "ㅁ"),
            ("qr", "rq", "ㄹ"),
            ("qh", "hq", "ㅎ"),
            ("qng", "ngq", "ㅇ"),
            ("qks", "ksq", "ㄳ"),
            ("qnj", "njq", "ㄵ"),
            ("qnh", "nhq", "ㄶ"),
            ("qlk", "lkq", "ㄺ"),
            ("qlm", "lmq", "ㄻ"),
            ("qlb", "lbq", "ㄼ"),
            ("qps", "psq", "ㅄ"),
        ] {
            assert_eq!(romaja_to_hangul(q_pre, ParseMode::Preview), expected, "Preview failed for {}", q_pre);
            assert_eq!(romaja_to_hangul(q_pre, ParseMode::Finalize), expected, "Finalize failed for {}", q_pre);
            assert_eq!(romaja_to_hangul(q_suf, ParseMode::Preview), expected, "Preview failed for {}", q_suf);
            assert_eq!(romaja_to_hangul(q_suf, ParseMode::Finalize), expected, "Finalize failed for {}", q_suf);
        }
        // 連打表情與混合組字（ㅋㅋㅋ、ㅠㅠ）
        assert_eq!(romaja_to_hangul("kqkqkq", ParseMode::Finalize), "ㅋㅋㅋ");
        assert_eq!(romaja_to_hangul("yuqyuq", ParseMode::Finalize), "ㅠㅠ");
        // 單打母音/子音按 `↓` 候選清單亦包含獨立諺文字母
        assert!(eng.lookup_down_candidates_for_romaja("a").contains(&"ㅏ".to_string()));
        assert!(eng.lookup_down_candidates_for_romaja("g").contains(&"ㄱ".to_string()));
        assert!(eng.lookup_down_candidates_for_romaja("ng").contains(&"ㅇ".to_string()));
        assert_eq!(r_ctrl.action, "reset");
        if false {
    }
        assert!(!eng.has_buffer());
    }
}
