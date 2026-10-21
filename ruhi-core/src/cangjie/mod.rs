/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/cangjie/mod.rs - 倉頡與英文零切換智慧混打引擎 (Cangjie-English Zero-Switch Engine)

pub mod table;

use crate::engine::EngineResponse;
use crate::router::transitions::TransitionEngine;
use std::path::Path;
use table::CangjieTable;

#[derive(Debug, Clone)]
pub struct CangjieEngine {
    pub table: CangjieTable,
    pub transitions: TransitionEngine,
    pub buffer: String,
    pub is_english_forced: bool,
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
    pub last_committed_word: String,
}

impl Default for CangjieEngine {
    fn default() -> Self {
        Self {
            table: CangjieTable::new(),
            transitions: TransitionEngine::new(),
            buffer: String::new(),
            is_english_forced: false,
            candidates: Vec::new(),
            candidate_idx: 0,
            last_committed_word: String::new(),
        }
    }
}

impl CangjieEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_tsv(path: &Path) -> Self {
        let mut engine = Self::default();
        let _ = engine.table.load_tsv(path);
        engine
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.is_english_forced = false;
        self.candidates.clear();
        self.candidate_idx = 0;
    }

    pub fn has_buffer(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// 記錄已上屏之字詞或數字脈絡，供字形重碼（倉頡、速成、大易、行列）進行上下文智慧消歧
    pub fn record_committed(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        // 若連續單鍵輸入阿拉伯數字或小數點（如先按 '1' 再按 '0'），自動累加成完整數值詞元 "10"
        if trimmed.len() == 1
            && trimmed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '%')
            && !self.last_committed_word.is_empty()
            && self
                .last_committed_word
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        {
            self.last_committed_word.push_str(trimmed);
            return;
        }

        let chars: Vec<char> = trimmed.chars().collect();
        if let Some(&last_ch) = chars.last() {
            if matches!(
                last_ch,
                '，' | '。' | '！' | '？' | '；' | '：' | '、' | ',' | '.' | '!' | '?' | ';' | ':' | '\n'
            ) {
                self.last_committed_word.clear();
                return;
            }
            if last_ch.is_ascii_digit() || last_ch == '%' {
                let mut start_idx = chars.len() - 1;
                while start_idx > 0 {
                    let prev_c = chars[start_idx - 1];
                    if prev_c.is_ascii_digit() || prev_c == '.' || prev_c == ',' {
                        start_idx -= 1;
                    } else {
                        break;
                    }
                }
                self.last_committed_word = chars[start_idx..].iter().collect();
                return;
            }
        }
        self.last_committed_word = trimmed.to_string();
    }

    /// 重新計算當前輸入緩衝區的候選列表與狀態（對重碼字套用後端語法構式與二階轉移消歧）
    fn update_candidates(&mut self) {
        self.update_candidates_with_lexicon(None);
    }

    /// 支援傳入外部詞庫與內建轉移引擎，對倉頡／速成／大易／行列之重碼候選進行上下文智慧排序
    pub fn update_candidates_with_lexicon(
        &mut self,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) {
        self.candidates.clear();
        self.candidate_idx = 0;

        if self.buffer.is_empty() {
            return;
        }

        // 若長度大於 5 碼，或者含有大寫字母，自動判定為英文模式
        let has_uppercase = self.buffer.chars().any(|c| c.is_ascii_uppercase());
        if self.buffer.len() > 5 || has_uppercase || self.is_english_forced {
            self.is_english_forced = true;
            self.candidates.push(self.buffer.clone());
            return;
        }

        // 倉頡／字形碼表查詢 (1~5 碼全小寫)
        let code = self.buffer.to_ascii_lowercase();
        if let Some(entries) = self.table.lookup(&code) {
            if entries.len() <= 1 || self.last_committed_word.is_empty() {
                for entry in entries {
                    self.candidates.push(entry.hanzi.clone());
                }
            } else {
                // 當該字形碼存在 2 個以上重碼字且具備前文脈絡時，啟動共用 TransitionEngine 語意消歧：
                // 唯一碼 100% 維持確定性直出；重碼組則依「靜態字頻基準 + 語法構式/二階轉移 + 詞庫雙字凝聚力」智慧排序
                let prev = self.last_committed_word.as_str();
                let prev_tail: String = prev
                    .chars()
                    .rev()
                    .take(1)
                    .collect::<Vec<char>>()
                    .into_iter()
                    .rev()
                    .collect();

                let mut scored: Vec<(usize, String, f64)> = entries
                    .iter()
                    .enumerate()
                    .map(|(idx, entry)| {
                        let base_score = (entry.weight as f64) / 25.0;
                        let mut trans_bonus =
                            self.transitions.calculate_transition_bonus(prev, "", &entry.hanzi, "");
                        if trans_bonus == 0.0 && !prev_tail.is_empty() && prev_tail != prev {
                            trans_bonus = self
                                .transitions
                                .calculate_transition_bonus(&prev_tail, "", &entry.hanzi, "");
                        }
                        let mut lex_bonus = 0.0;
                        if let Some(lex) = lexicon {
                            let cand_word = format!("{}{}", prev, entry.hanzi);
                            let tail_word = format!("{}{}", prev_tail, entry.hanzi);
                            if lex
                                .entries
                                .iter()
                                .any(|e| e.word == cand_word || (!prev_tail.is_empty() && e.word == tail_word))
                            {
                                lex_bonus = 28.0;
                            }
                        }
                        (idx, entry.hanzi.clone(), base_score + trans_bonus + lex_bonus)
                    })
                    .collect();

                scored.sort_by(|a, b| {
                    b.2.partial_cmp(&a.2)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.0.cmp(&b.0))
                });
                for (_, hanzi, _) in scored {
                    self.candidates.push(hanzi);
                }
            }
            // 英文免切換候選：在末尾自動附帶原始英文字串
            if !self.candidates.contains(&self.buffer) {
                self.candidates.push(self.buffer.clone());
            }
        } else {
            // 未命中漢字，直接提供原始英文字串作為第一候選
            self.candidates.push(self.buffer.clone());
        }
    }

    /// 生成當前編輯區 (Preedit) 顯示字串
    fn make_preedit(&self) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }

        if self.is_english_forced {
            // 英文模式直接顯示英文
            self.buffer.clone()
        } else {
            // 倉頡模式：顯示「字母 + [字根]」，例如 "ab [日月]"
            let radicals = CangjieTable::code_to_radicals(&self.buffer);
            if radicals.is_empty() {
                self.buffer.clone()
            } else {
                format!("{} [{}]", self.buffer, radicals)
            }
        }
    }

    /// 核心字元輸入處理
    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        match ch {
            // 1. 空白鍵 (Space)
            ' ' => {
                if self.buffer.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "cangjie".to_string(),
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

                // 若有候選，空白鍵提交第 1 候選（漢字或英文）
                let commit_text = if !self.candidates.is_empty() {
                    self.candidates[0].clone()
                } else {
                    self.buffer.clone()
                };

                self.record_committed(&commit_text);
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "cangjie".to_string(),
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

            // 2. Enter 鍵 (\n 或 \r) —— 零切換英文直通！
            '\n' | '\r' => {
                if self.buffer.is_empty() {
                    self.last_committed_word.clear();
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "cangjie".to_string(),
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

                // Enter 鍵絕對鐵律：直通提交原始輸入字串（英文、變數名、命令等）
                let raw_str = self.buffer.clone();
                self.record_committed(&raw_str);
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "cangjie".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: raw_str,
                }
            }

            // 3. 退格鍵 (Backspace: \x08)
            '\x08' => {
                if self.buffer.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "cangjie".to_string(),
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

                self.buffer.pop();
                if self.buffer.is_empty() {
                    self.reset();
                    return EngineResponse {
                        action: "reset".to_string(),
                        mode: "cangjie".to_string(),
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
                    mode: if self.is_english_forced { "cangjie_en".to_string() } else { "cangjie".to_string() },
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: self.buffer.len(),
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }

            // 4. Escape 鍵 (\x1b) —— 放棄輸入並重設
            '\x1b' => {
                self.reset();
                EngineResponse {
                    action: "reset".to_string(),
                    mode: "cangjie".to_string(),
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

            // 5. 數字鍵 1~9：若在組字中，選取對應序號候選字
            '1'..='9' if !self.buffer.is_empty() => {
                let digit_idx = (ch as u8 - b'1') as usize;
                if digit_idx < self.candidates.len() {
                    let commit_text = self.candidates[digit_idx].clone();
                    self.record_committed(&commit_text);
                    self.reset();
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "cangjie".to_string(),
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
                    // 超出候選範圍，保持組字
                    let preedit = self.make_preedit();
                    let active = self.candidates.first().cloned().unwrap_or_default();
                    EngineResponse {
                        action: "compose".to_string(),
                        mode: if self.is_english_forced { "cangjie_en".to_string() } else { "cangjie".to_string() },
                        preedit,
                        composed: active.clone(),
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    }
                }
            }

            // 6. 英文字母 a~z / A~Z
            c if c.is_ascii_alphabetic() => {
                if c.is_ascii_uppercase() {
                    self.is_english_forced = true;
                }
                self.buffer.push(c);
                self.update_candidates();

                let preedit = self.make_preedit();
                let active = self.candidates.first().cloned().unwrap_or_default();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: if self.is_english_forced { "cangjie_en".to_string() } else { "cangjie".to_string() },
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: self.buffer.len(),
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }

            // 7. 其他按鍵（阿拉伯數字直通、標點符號、底線等）
            _ => {
                if self.buffer.is_empty() {
                    let s = ch.to_string();
                    self.record_committed(&s);
                    EngineResponse {
                        action: "pass".to_string(),
                        mode: "cangjie".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: s,
                    }
                } else {
                    // 若當前有組字，底線或特定符號視為英文識別（例如 snake_case）
                    if ch == '_' || ch == '-' {
                        self.is_english_forced = true;
                        self.buffer.push(ch);
                        self.update_candidates();
                        let preedit = self.make_preedit();
                        EngineResponse {
                            action: "compose".to_string(),
                            mode: "cangjie_en".to_string(),
                            preedit,
                            composed: self.buffer.clone(),
                            candidates: self.candidates.clone(),
                            active_candidate: self.buffer.clone(),
                            candidate_idx: 0,
                            cursor_pos: self.buffer.len(),
                            q_press_count: 0,
                            committed_chunk: String::new(),
                        }
                    } else {
                        // 遇到標點符號或數字，先自動提交第一候選，再放行該字元並記錄脈絡
                        let commit_text = if !self.candidates.is_empty() {
                            self.candidates[0].clone()
                        } else {
                            self.buffer.clone()
                        };
                        let full_committed = format!("{}{}", commit_text, ch);
                        self.record_committed(&full_committed);
                        self.reset();
                        EngineResponse {
                            action: "commit".to_string(),
                            mode: "cangjie".to_string(),
                            preedit: String::new(),
                            composed: String::new(),
                            candidates: Vec::new(),
                            active_candidate: String::new(),
                            candidate_idx: 0,
                            cursor_pos: 0,
                            q_press_count: 0,
                            committed_chunk: full_committed,
                        }
                    }
                }
            }
        }
    }

    /// 支援具名鍵位字串
    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        match key_str {
            "Space" => self.process_key(' '),
            "Return" | "Enter" => self.process_key('\n'),
            "BackSpace" => self.process_key('\x08'),
            "Shift+BackSpace" | "Shift+backspace" | "Ctrl+BackSpace" | "Ctrl+backspace" => {
                self.buffer.clear();
                self.candidates.clear();
                EngineResponse {
                    action: "update_preedit".to_string(),
                    mode: "cangjie".to_string(),
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
            "Escape" => self.process_key('\x1b'),
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap();
                self.process_key(ch)
            }
            _ => EngineResponse {
                action: "pass".to_string(),
                mode: "cangjie".to_string(),
                preedit: self.make_preedit(),
                composed: self.candidates.first().cloned().unwrap_or_default(),
                candidates: self.candidates.clone(),
                active_candidate: self.candidates.first().cloned().unwrap_or_default(),
                candidate_idx: 0,
                cursor_pos: self.buffer.len(),
                q_press_count: 0,
                committed_chunk: String::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cangjie_standard_chinese_commit() {
        let mut engine = CangjieEngine::new();

        // 1. 輸入 'a' -> [日]
        let r1 = engine.process_key('a');
        assert_eq!(r1.action, "compose");
        assert_eq!(r1.preedit, "a [日]");
        assert!(r1.candidates.contains(&"日".to_string()));

        // 2. 輸入 'b' -> "ab" -> [日月] -> "明"
        let r2 = engine.process_key('b');
        assert_eq!(r2.action, "compose");
        assert_eq!(r2.preedit, "ab [日月]");
        assert_eq!(r2.candidates[0], "明");
        // 候選列表中包含原始英文字串作為可選備選
        assert!(r2.candidates.contains(&"ab".to_string()));

        // 3. 按 Space -> 上屏 "明"
        let r3 = engine.process_key(' ');
        assert_eq!(r3.action, "commit");
        assert_eq!(r3.committed_chunk, "明");
        assert!(!engine.has_buffer());
    }

    #[test]
    fn test_cangjie_english_zero_switch_via_enter() {
        let mut engine = CangjieEngine::new();

        // 輸入 "ab"，原本對應倉頡 "明"
        engine.process_key('a');
        engine.process_key('b');

        // 但使用者想打的是英文變數 "ab"，直接按 Enter 鍵！
        let r = engine.process_key('\n');
        assert_eq!(r.action, "commit");
        assert_eq!(r.committed_chunk, "ab"); // 零切換直通出英文！
        assert!(!engine.has_buffer());
    }

    #[test]
    fn test_cangjie_automatic_english_over_5_chars() {
        let mut engine = CangjieEngine::new();

        // 輸入超過 5 碼的英文單字 "commit"
        for ch in "commit".chars() {
            engine.process_key(ch);
        }

        // 超過 5 碼自動進入純英文模式
        assert!(engine.is_english_forced);
        assert_eq!(engine.candidates[0], "commit");

        // 按 Space 直接上屏英文
        let r = engine.process_key(' ');
        assert_eq!(r.action, "commit");
        assert_eq!(r.committed_chunk, "commit");
        assert!(!engine.has_buffer());
    }

    #[test]
    fn test_cangjie_uppercase_and_camelcase() {
        let mut engine = CangjieEngine::new();

        // 輸入大寫開頭的 "Git"
        engine.process_key('G');
        engine.process_key('i');
        engine.process_key('t');

        // 大寫自動判定為英文模式
        assert!(engine.is_english_forced);
        assert_eq!(engine.candidates[0], "Git");

        let r = engine.process_key(' ');
        assert_eq!(r.action, "commit");
        assert_eq!(r.committed_chunk, "Git");
    }

    #[test]
    fn test_cangjie_number_candidate_selection() {
        let mut engine = CangjieEngine::new();

        // "ab" 有多個候選字：1. 明, 2. 冐, 3. ab
        engine.process_key('a');
        engine.process_key('b');

        // 使用者按數字 2 選取 "冐"
        let r = engine.process_key('2');
        assert_eq!(r.action, "commit");
        assert_eq!(r.committed_chunk, "冐");
        assert!(!engine.has_buffer());
    }

    #[test]
    fn test_cangjie_backspace_and_escape() {
        let mut engine = CangjieEngine::new();

        engine.process_key('a');
        engine.process_key('b');
        assert_eq!(engine.buffer, "ab");

        // 退格
        let r_back = engine.process_key('\x08');
        assert_eq!(r_back.action, "compose");
        assert_eq!(engine.buffer, "a");
        assert_eq!(r_back.candidates[0], "日");

        // Escape 取消
        let r_esc = engine.process_key('\x1b');
        assert_eq!(r_esc.action, "reset");
        assert!(!engine.has_buffer());
    }

    #[test]
    fn test_cangjie_and_sucheng_contextual_disambiguation() {
        let mut engine = CangjieEngine::new();

        // 1. 無前文時，重碼字依靜態字頻排序（例如 "wg" 預設第 1 候選為「理」，"mmu" 預設為「頑」，"gm" 預設為「瓶」）
        engine.process_key('w');
        let r_no_ctx = engine.process_key('g');
        assert_eq!(r_no_ctx.candidates[0], "理");
        engine.process_key('\x1b');

        // 2. 前文為處所名詞「硬碟」時，方位後置詞構式自動將重碼字「裡」提升至第 1 候選（免按數字鍵選字）
        engine.record_committed("硬碟");
        engine.process_key('w');
        let r_loc = engine.process_key('g');
        assert_eq!(
            r_loc.candidates[0],
            "裡",
            "字形重碼在處所名詞『硬碟』後必由方位構式自動將『裡』排上第 1 順位"
        );
        let commit_loc = engine.process_key(' ');
        assert_eq!(commit_loc.committed_chunk, "裡");

        // 3. 前文連續輸入數字 '1', '0', '0' 後，數詞單位構式自動將重碼字「元」(mmu)、「坪」(gm)、「度」(it)、「月」(to) 排上第 1 順位
        engine.process_key('1');
        engine.process_key('0');
        engine.process_key('0');
        assert_eq!(engine.last_committed_word, "100");
        engine.process_key('m');
        engine.process_key('m');
        let r_cur = engine.process_key('u');
        assert_eq!(
            r_cur.candidates[0],
            "元",
            "數字 100 後字形重碼必由數詞單位構式自動將『元』排上第 1 順位（勝過『頑』）"
        );
        engine.process_key(' ');

        engine.process_key('5');
        engine.process_key('0');
        engine.process_key('g');
        let r_area = engine.process_key('m');
        assert_eq!(
            r_area.candidates[0],
            "坪",
            "數字 50 後字形重碼必自動將面積單位『坪』排上第 1 順位（勝過『瓶』）"
        );
        engine.process_key(' ');

        engine.process_key('3');
        engine.process_key('7');
        engine.process_key('i');
        let r_temp = engine.process_key('t');
        assert_eq!(
            r_temp.candidates[0],
            "度",
            "數字 37 後字形重碼必自動將溫度單位『度』排上第 1 順位（勝過『渡』）"
        );
    }
}
