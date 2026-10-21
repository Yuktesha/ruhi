/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/korean/mod.rs - 韓語標準羅馬拼音（RR）諺文組字模組與狀態機

pub mod jamo;
pub mod romaja;

use crate::engine::EngineResponse;
use crate::korean::romaja::{parse_nbest, render, romaja_to_hangul, ParseMode, NBEST_K};

/// 韓語常用詞表（P3 為空結構，於 P4 擴充完整詞表與骨架索引）
#[derive(Debug, Clone, Default)]
pub struct KoreanLexicon;

impl KoreanLexicon {
    pub fn new() -> Self {
        Self
    }
}

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
            lexicon: KoreanLexicon::new(),
        }
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

    /// 取得當前組字預覽字串（Preview 模式）
    pub fn make_preedit(&self) -> String {
        if self.raw_romaja.is_empty() {
            return String::new();
        }
        if self.is_english_forced {
            return self.raw_romaja.clone();
        }
        romaja_to_hangul(&self.raw_romaja, ParseMode::Preview)
    }

    /// 查詢 `↓` 候選清單：N-best 結果去重，最多 9 筆
    pub fn lookup_down_candidates(&self) -> Vec<String> {
        if self.raw_romaja.is_empty() {
            return Vec::new();
        }
        if self.is_english_forced {
            return vec![self.raw_romaja.clone()];
        }
        let parses = parse_nbest(&self.raw_romaja, ParseMode::Finalize, NBEST_K.max(9));
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
        if cands.len() < 9 && !cands.contains(&self.raw_romaja) {
            cands.push(self.raw_romaja.clone());
        }
        cands
    }

    pub fn update_candidates(&mut self) {
        self.candidates = self.lookup_down_candidates();
        self.candidate_idx = 0;
    }

    fn finalized_commit_text(&self) -> String {
        if self.is_english_forced {
            return self.raw_romaja.clone();
        }
        self.candidates
            .get(self.candidate_idx)
            .cloned()
            .unwrap_or_else(|| romaja_to_hangul(&self.raw_romaja, ParseMode::Finalize))
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
}
