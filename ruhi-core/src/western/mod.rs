/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/mod.rs - 純歐文鍵盤輸入引擎 (Western Input Engine)

pub mod fsm;
pub mod lexicon;
pub mod morphology;
pub mod scoring;

pub use fsm::{FsmAction, TokenCasing, WesternFSM, WesternToken, WesternTokenKind};
pub use lexicon::WesternLexicon;
pub use morphology::{MorphologicalVariant, MorphologyEngine};
pub use scoring::WesternConfidenceScorer;

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct WesternResponse {
    pub action: String,
    pub committed_chunk: String,
    pub display_text: String,
    pub confidence: f64,
    pub is_code_identifier: bool,
}

impl WesternResponse {
    pub fn new(action: &str, chunk: &str, display: &str, confidence: f64, is_identifier: bool) -> Self {
        Self {
            action: action.to_string(),
            committed_chunk: chunk.to_string(),
            display_text: display.to_string(),
            confidence,
            is_code_identifier: is_identifier,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WesternEngine {
    pub fsm: WesternFSM,
    pub lexicon: WesternLexicon,
    pub scorer: WesternConfidenceScorer,
}

impl WesternEngine {
    pub fn new() -> Self {
        Self {
            fsm: WesternFSM::new(),
            lexicon: WesternLexicon::new(),
            scorer: WesternConfidenceScorer::new(),
        }
    }

    pub fn new_with_lexicon(lexicon: WesternLexicon) -> Self {
        Self {
            fsm: WesternFSM::new(),
            lexicon,
            scorer: WesternConfidenceScorer::new(),
        }
    }

    /// 從 TSV 檔案載入完整歐文詞庫
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        self.lexicon.load_tsv(path)
    }

    /// 處理按鍵輸入
    pub fn process_key(&mut self, ch: char) -> WesternResponse {
        let fsm_act = self.fsm.process_char(ch);
        let cur_tok = self.fsm.current_token();
        let confidence = self.scorer.score_token(cur_tok, &self.lexicon);
        let is_id = self.fsm.is_current_identifier();
        let disp = self.fsm.get_display_text();

        match fsm_act {
            FsmAction::CommitToken(chunk) => {
                WesternResponse::new("commit_token", &chunk, &disp, confidence, is_id)
            }
            FsmAction::CommitSentence(chunk) => {
                WesternResponse::new("commit_sentence", &chunk, &disp, confidence, is_id)
            }
            FsmAction::AppendPreedit => {
                WesternResponse::new("update_preedit", "", &disp, confidence, is_id)
            }
            FsmAction::DeleteChar => {
                WesternResponse::new("update_preedit", "", &disp, confidence, is_id)
            }
            FsmAction::None => {
                WesternResponse::new("none", "", &disp, confidence, is_id)
            }
        }
    }

    /// 退格鍵處理
    pub fn backspace(&mut self) -> WesternResponse {
        let _ = self.fsm.backspace();
        let cur_tok = self.fsm.current_token();
        let confidence = self.scorer.score_token(cur_tok, &self.lexicon);
        let is_id = self.fsm.is_current_identifier();
        let disp = self.fsm.get_display_text();
        WesternResponse::new("update_preedit", "", &disp, confidence, is_id)
    }

    /// 取得目前顯示字串
    pub fn get_display_text(&self) -> String {
        self.fsm.get_display_text()
    }

    /// 取得目前詞元
    pub fn current_token(&self) -> &str {
        self.fsm.current_token()
    }

    /// 評估目前詞元之歐文正向信心度
    pub fn get_current_confidence(&self) -> f64 {
        self.scorer.score_token(self.fsm.current_token(), &self.lexicon)
    }

    /// 評估任意字串之歐文正向信心度
    pub fn score_string(&self, s: &str) -> f64 {
        self.scorer.score_token(s, &self.lexicon)
    }

    /// 重設狀態機
    pub fn reset(&mut self) {
        self.fsm.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_western_engine_sentence_typing() {
        let mut eng = WesternEngine::new();

        // 模擬鍵入: "this is a sample book\n"
        let mut committed = String::new();
        for c in "this is a sample book\n".chars() {
            let resp = eng.process_key(c);
            if !resp.committed_chunk.is_empty() {
                committed.push_str(&resp.committed_chunk);
            }
        }
        assert_eq!(committed, "this is a sample book", "歐文引擎必須精準完成分詞與標點連綴");
    }

    #[test]
    fn test_western_engine_code_identifier() {
        let mut eng = WesternEngine::new();

        // 鍵入代碼 camelCase
        for c in "getElementById".chars() {
            eng.process_key(c);
        }
        assert_eq!(eng.current_token(), "getElementById");
        assert!(eng.fsm.is_current_identifier());
        assert!(eng.get_current_confidence() >= 0.90, "代碼標識符必須具備高正向信心");

        let resp = eng.process_key(' ');
        assert_eq!(resp.committed_chunk, "getElementById ");
    }
}
