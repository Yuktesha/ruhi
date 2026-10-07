/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/fsm.rs - 歐文詞法狀態機與專業輸入行為 (Western Lexical FSM)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenCasing {
    LowerCase,           // word
    UpperCase,           // NASA, HTML
    TitleCase,           // Google, Taiwan
    CamelCase,           // getElementById, iPhone
    PascalCase,          // CandidateWindow, WesternEngine
    SnakeCase,           // user_name, max_buffer_size
    ScreamingSnakeCase,  // VK_RETURN, MAX_LEN
    KebabCase,           // ruhi-core, web-app
    Mixed,               // other mixed tokens
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WesternTokenKind {
    Word(TokenCasing),
    Identifier(TokenCasing), // 代碼識別符（駝峰/底線/連字號，嚴格保護不切斷）
    Number,
    Punctuation,
    Whitespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WesternToken {
    pub raw: String,
    pub kind: WesternTokenKind,
}

impl WesternToken {
    pub fn new(text: &str) -> Self {
        let kind = Self::classify(text);
        Self {
            raw: text.to_string(),
            kind,
        }
    }

    pub fn classify(text: &str) -> WesternTokenKind {
        if text.is_empty() {
            return WesternTokenKind::Word(TokenCasing::LowerCase);
        }

        // 數字
        if text.chars().all(|c| c.is_ascii_digit() || c == '.') && text.chars().any(|c| c.is_ascii_digit()) {
            return WesternTokenKind::Number;
        }

        let chars: Vec<char> = text.chars().collect();
        let has_underscore = chars.contains(&'_');
        let has_hyphen = chars.contains(&'-');
        let has_upper = chars.iter().any(|c| c.is_ascii_uppercase());
        let has_lower = chars.iter().any(|c| c.is_ascii_lowercase());

        // 代碼標識符判定（底線、連字號、或駝峰大小寫交錯）
        if has_underscore {
            if has_upper && !has_lower {
                return WesternTokenKind::Identifier(TokenCasing::ScreamingSnakeCase);
            }
            return WesternTokenKind::Identifier(TokenCasing::SnakeCase);
        }

        if has_hyphen && chars.iter().all(|c| c.is_ascii_alphanumeric() || *c == '-') {
            return WesternTokenKind::Identifier(TokenCasing::KebabCase);
        }

        // 駝峰命名檢驗 (camelCase / PascalCase)
        if has_upper && has_lower {
            let first_is_upper = chars[0].is_ascii_uppercase();
            // 檢查內部是否有 lower -> upper 或 upper -> lower 的轉換
            let has_internal_camel = chars.windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
            if has_internal_camel {
                if first_is_upper {
                    return WesternTokenKind::Identifier(TokenCasing::PascalCase);
                } else {
                    return WesternTokenKind::Identifier(TokenCasing::CamelCase);
                }
            } else if first_is_upper && chars[1..].iter().all(|c| c.is_ascii_lowercase() || *c == '\'') {
                return WesternTokenKind::Word(TokenCasing::TitleCase);
            } else {
                return WesternTokenKind::Word(TokenCasing::Mixed);
            }
        }

        if has_upper && !has_lower {
            return WesternTokenKind::Word(TokenCasing::UpperCase);
        }

        WesternTokenKind::Word(TokenCasing::LowerCase)
    }

    /// 判斷是否為應受保護之專業代碼識別符
    pub fn is_code_identifier(&self) -> bool {
        matches!(self.kind, WesternTokenKind::Identifier(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmAction {
    AppendPreedit,
    CommitToken(String),
    CommitSentence(String),
    DeleteChar,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct WesternFSM {
    pub current_token: String,
    pub committed_tokens: Vec<WesternToken>,
    pub caps_lock: bool,
    pub is_shift: bool,
    pub last_was_punct: bool,
}

impl WesternFSM {
    pub fn new() -> Self {
        Self::default()
    }

    /// 輸入字元處理
    pub fn process_char(&mut self, ch: char) -> FsmAction {
        // 1. 空格鍵 (Space) -> 詞邊界結算
        if ch == ' ' {
            if !self.current_token.is_empty() {
                let token = WesternToken::new(&self.current_token);
                let committed_str = format!("{} ", self.current_token);
                self.committed_tokens.push(token);
                self.current_token.clear();
                self.last_was_punct = false;
                return FsmAction::CommitToken(committed_str);
            } else {
                return FsmAction::CommitToken(" ".to_string());
            }
        }

        // 2. 換行/Enter 鍵 -> 結算當前未上屏詞元
        if ch == '\n' || ch == '\r' {
            let result = if !self.current_token.is_empty() {
                self.current_token.clone()
            } else {
                String::new()
            };
            self.reset();
            return FsmAction::CommitToken(result);
        }

        // 3. 西文標點符號 (依附前字並觸發結算)
        if matches!(ch, ',' | '.' | '!' | '?' | ';' | ':') {
            let mut chunk = String::new();
            if !self.current_token.is_empty() {
                chunk.push_str(&self.current_token);
                let token = WesternToken::new(&self.current_token);
                self.committed_tokens.push(token);
                self.current_token.clear();
            }
            chunk.push(ch);
            chunk.push(' '); // 標點後自然留空
            self.last_was_punct = true;
            return FsmAction::CommitToken(chunk);
        }

        // 4. 英數字元與代碼連接符 (底線、連字號、撇號)
        // 撇號: 僅在有前綴字母時作為單字內撇號 (如 don't, it's)，開頭單獨鍵入則不吸附
        let is_valid_char = ch.is_ascii_alphanumeric()
            || ch == '_'
            || (ch == '-' && !self.current_token.is_empty())
            || (ch == '\'' && !self.current_token.is_empty());

        if is_valid_char {
            self.current_token.push(ch);
            self.last_was_punct = false;
            return FsmAction::AppendPreedit;
        }

        FsmAction::None
    }

    /// 退格鍵處理
    pub fn backspace(&mut self) -> FsmAction {
        if !self.current_token.is_empty() {
            self.current_token.pop();
            FsmAction::DeleteChar
        } else if let Some(last_tok) = self.committed_tokens.pop() {
            self.current_token = last_tok.raw;
            FsmAction::DeleteChar
        } else {
            FsmAction::None
        }
    }

    /// 取得目前完整顯示字串
    pub fn get_display_text(&self) -> String {
        let mut text = String::new();
        for (i, tok) in self.committed_tokens.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            text.push_str(&tok.raw);
        }
        if !self.current_token.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&self.current_token);
        }
        text
    }

    /// 取得目前詞元
    pub fn current_token(&self) -> &str {
        &self.current_token
    }

    /// 判斷目前詞元是否為代碼標識符 (CamelCase, snake_case 等)
    pub fn is_current_identifier(&self) -> bool {
        WesternToken::new(&self.current_token).is_code_identifier()
    }

    pub fn reset(&mut self) {
        self.current_token.clear();
        self.committed_tokens.clear();
        self.last_was_punct = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_casing_classification() {
        assert_eq!(WesternToken::classify("apple"), WesternTokenKind::Word(TokenCasing::LowerCase));
        assert_eq!(WesternToken::classify("NASA"), WesternTokenKind::Word(TokenCasing::UpperCase));
        assert_eq!(WesternToken::classify("Google"), WesternTokenKind::Word(TokenCasing::TitleCase));
        assert_eq!(WesternToken::classify("getElementById"), WesternTokenKind::Identifier(TokenCasing::CamelCase));
        assert_eq!(WesternToken::classify("CandidateWindow"), WesternTokenKind::Identifier(TokenCasing::PascalCase));
        assert_eq!(WesternToken::classify("max_buffer_size"), WesternTokenKind::Identifier(TokenCasing::SnakeCase));
        assert_eq!(WesternToken::classify("VK_RETURN"), WesternTokenKind::Identifier(TokenCasing::ScreamingSnakeCase));
        assert_eq!(WesternToken::classify("ruhi-core"), WesternTokenKind::Identifier(TokenCasing::KebabCase));
    }

    #[test]
    fn test_fsm_stream_typing() {
        let mut fsm = WesternFSM::new();
        // 鍵入 "this "
        for c in "this ".chars() {
            fsm.process_char(c);
        }
        assert_eq!(fsm.committed_tokens.len(), 1);
        assert_eq!(fsm.committed_tokens[0].raw, "this");

        // 鍵入代碼標識符 "getElementById "
        for c in "getElementById ".chars() {
            fsm.process_char(c);
        }
        assert_eq!(fsm.committed_tokens.len(), 2);
        assert_eq!(fsm.committed_tokens[1].raw, "getElementById");
        assert!(fsm.committed_tokens[1].is_code_identifier());
    }
}
