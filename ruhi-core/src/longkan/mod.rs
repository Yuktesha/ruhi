// ruhi-core/src/longkan/mod.rs — 如喜・龍龕形音輸入法 (Longkan Morpho-Phonetic 4-Code IME) 智慧引擎
//
// 核心設計特色：
// 1. 源自西元 997 年遼代高僧行均《龍龕手鑑》「依視覺字形歸部、結合四聲語音檢索」之精神：
//    - 軌道 A（有聲部首 2 碼 ＋ 聲符／部件音 2 碼）：羅馬／台羅與許氏雙拼雙基底並容（如 鋪=kmfu/jnfx、龕=helo/holg/hgll）
//    - 軌道 B（視覺英文字母象形 1 碼 ＋ 聲符音 2 碼 ＋ Space/聲調補碼 12345/dfjs）：如 肴=xeo/xeo2/xeo3/xeof、凶=xu/xu1
//    - 軌道 C（部件 × 數字 C × N）：如 區=cko3/cpn/cpn3/cpnf、品=ko3/ko33/ko3f、森=mu3、協=tli3、蕊=wsm3
//    - 軌道 D（高頻雙字詞 4 碼簡碼）：首字前 2 碼 ＋ 尾字前 2 碼
// 2. 結合如喜 Viterbi 語意詞網與 TransitionEngine：
//    - 重碼候選依據前文脈絡 (`last_committed_word`) 與 Layer 1~3 語意轉移自動排序消歧
// 3. 零切換中英混打 (Zero-Switch English Passthrough)：
//    - 按空白鍵確認組字上屏；按 Enter 鍵直接輸出原始英文字串

pub mod table;

pub use self::table::{LongkanEntry, LongkanReverseEntry, LongkanTable};
use crate::engine::EngineResponse;
use crate::router::transitions::TransitionEngine;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LongkanEngine {
    pub table: LongkanTable,
    pub transitions: TransitionEngine,
    pub buffer: String,
    /// 緩衝區內的編輯游標位置（用於移動游標至詞界開頭 `0` 處補按 `Tab` 開啟詞彙簡碼模式）
    pub buffer_cursor: usize,
    pub is_english_forced: bool,
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
    pub last_committed_word: String,
    /// 當「九九圓滿（滿 81 日 & 滿 81,000 字）」雙重達標且未啟用通行碼時，進入優雅降級模式：
    /// 保留 100% 單字基礎輸入（絕不斷糧），僅暫停雙字詞 4 碼連打與前文脈絡智慧消歧加成
    pub grace_degraded: bool,
}

impl Default for LongkanEngine {
    fn default() -> Self {
        Self {
            table: LongkanTable::new(),
            transitions: TransitionEngine::new(),
            buffer: String::new(),
            buffer_cursor: 0,
            is_english_forced: false,
            candidates: Vec::new(),
            candidate_idx: 0,
            last_committed_word: String::new(),
            grace_degraded: false,
        }
    }
}

impl LongkanEngine {
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
        self.buffer_cursor = 0;
        self.is_english_forced = false;
        self.candidates.clear();
        self.candidate_idx = 0;
    }

    pub fn has_buffer(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn record_committed(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
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
        }
        self.last_committed_word = trimmed.to_string();
    }

    pub fn make_preedit(&self) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }
        if self.is_english_forced {
            return self.buffer.clone();
        }
        if self.buffer.starts_with('-') {
            format!("🐉⇥詞:{}", self.buffer[1..].to_ascii_uppercase())
        } else {
            format!("🐉{}", self.buffer.to_ascii_uppercase())
        }
    }

    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        self.process_key_with_lexicon(ch, None)
    }

    pub fn process_key_with_lexicon(
        &mut self,
        ch: char,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) -> EngineResponse {
        // 1. Backspace ('\x08')
        if ch == '\x08' {
            if self.buffer.is_empty() {
                return Self::pass_response("");
            }
            self.buffer.pop();
            self.buffer_cursor = self.buffer.len();
            if self.buffer.is_empty() {
                self.reset();
                return EngineResponse {
                    action: "reset".to_string(),
                    mode: "longkan".to_string(),
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
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 2. Escape ('\x1b')
        if ch == '\x1b' {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "longkan".to_string(),
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

        // 3. Enter ('\n' 或 '\r')：確認當前組字收攏入輸入座並通知上層將輸入座整句送出至目標視窗（若為強制英文模式則保留原始英文字母）
        if ch == '\n' || ch == '\r' {
            if self.buffer.is_empty() || self.buffer == "-" {
                self.reset();
                self.last_committed_word.clear();
                return Self::pass_response("");
            }
            let committed = if self.is_english_forced {
                self.buffer.trim_start_matches('-').to_string()
            } else {
                self.candidates
                    .get(self.candidate_idx)
                    .or_else(|| self.candidates.first())
                    .cloned()
                    .unwrap_or_else(|| self.buffer.trim_start_matches('-').to_string())
            };
            self.record_committed(&committed);
            self.reset();
            return Self::commit_response(committed);
        }

        // 4. Space (' ')：不足 4 碼時補足收攏並將首選字詞收攏至如喜輸入座（不直接送往目標視窗，整句待 Enter 確認後才送出）
        if ch == ' ' {
            if self.buffer.is_empty() {
                return Self::pass_response(" ");
            }
            if self.is_english_forced {
                let word = self.buffer.clone();
                self.record_committed(&word);
                self.reset();
                return Self::commit_response(word);
            }
            let committed = self
                .candidates
                .get(self.candidate_idx)
                .or_else(|| self.candidates.first())
                .cloned()
                .unwrap_or_else(|| self.buffer.clone());
            self.record_committed(&committed);
            self.reset();
            return Self::commit_response(committed);
        }

        // 5. 數字鍵 '0'..'9'：
        //    - 若當前緩衝區未滿上限（單字 4 碼、詞彙 6 碼），且加上該數字（如 '1'..'5'）在龍龕碼表中構成有效編碼或前綴（即「聲調補碼 12345」或「部件×數字」），則優先作為組字碼！
        //    - 否則作為候選字快選鍵（1..9 -> 0..8, 0 -> 9）！
        if ch.is_ascii_digit() {
            if self.buffer.is_empty() {
                let s = ch.to_string();
                self.record_committed(&s);
                return Self::pass_response(&s);
            }
            if !self.is_english_forced {
                let lower_buf = self.buffer.to_ascii_lowercase();
                let is_word_mode = lower_buf.starts_with('-');
                let code_len = lower_buf.trim_start_matches('-').len();
                let max_code_len = if is_word_mode { 12 } else { 12 };
                if code_len >= 1 && code_len < max_code_len && matches!(ch, '1'..='5') {
                    let probe = format!("{}{}", lower_buf, ch);
                    if self.table.has_exact_or_prefix(&probe) {
                        self.buffer.push(ch);
                        self.buffer_cursor = self.buffer.len();
                        self.update_candidates_with_lexicon(lexicon);
                        return self.build_compose_response();
                    }
                }
                let idx = if ch == '0' {
                    9
                } else {
                    (ch as usize) - ('1' as usize)
                };
                if idx < self.candidates.len() {
                    let chosen = self.candidates[idx].clone();
                    self.record_committed(&chosen);
                    self.reset();
                    return Self::commit_response(chosen);
                }
                return self.build_compose_response();
            }
            self.buffer.push(ch);
            self.buffer_cursor = self.buffer.len();
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 5B. 104 主鍵盤 Tab ('\t')（或 '-'）作為詞彙簡碼輸入的間隔（引導與結束）符號 + 游標詞界補按機制：
        //     - 規範簡碼公式：第一字前兩碼 + 中間字第一碼們 + 最後一字後兩碼（2字詞4碼、3字詞5碼、4字詞6碼）
        //     1. 空緩衝區時按 Tab 或 '-'：引導開啟 Tab 詞彙簡碼模式（`🐉⇥詞:`）
        //     2. 已在 Tab 詞彙簡碼模式（`self.buffer.starts_with('-')`）時按 Tab：作為結束符號，將詞彙候選直接上屏並結束詞彙模式
        //     3. 單字模式下：
        //        - 若忘記先按 Tab，將文字游標移至詞界開頭（`self.buffer_cursor == 0`）按 Tab（或直接按 '-'）：於詞界處補入前導符切換為詞彙簡碼模式
        //        - 若游標在緩衝區末端（`self.buffer_cursor > 0`）按 Tab：遵循許氏鍵盤直覺作為「詞界阻斷符號」，將當前單字確認上屏
        if ch == '\t' || (ch == '-' && !self.is_english_forced) {
            if self.buffer.is_empty() {
                if !self.grace_degraded {
                    self.buffer.push('-');
                    self.buffer_cursor = self.buffer.len();
                    self.update_candidates_with_lexicon(lexicon);
                    return self.build_compose_response();
                }
                let s = ch.to_string();
                return Self::pass_response(&s);
            }
            if !self.grace_degraded
                && (!self.is_english_forced
                    || (self.buffer_cursor == 0
                        && !self.buffer.chars().any(|c| c.is_ascii_uppercase())))
            {
                if self.buffer.starts_with('-') {
                    if ch == '\t' {
                        if self.buffer.len() > 1 {
                            let committed = self
                                .candidates
                                .get(self.candidate_idx)
                                .or_else(|| self.candidates.first())
                                .cloned()
                                .unwrap_or_else(|| self.buffer[1..].to_string());
                            self.record_committed(&committed);
                            self.reset();
                            return Self::commit_response(committed);
                        } else {
                            self.reset();
                            return EngineResponse {
                                action: "reset".to_string(),
                                mode: "longkan".to_string(),
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
                    }
                } else if self.buffer_cursor == 0 || ch == '-' {
                    self.is_english_forced = false;
                    self.buffer.insert(0, '-');
                    self.buffer_cursor = self.buffer.len();
                    self.update_candidates_with_lexicon(lexicon);
                    return self.build_compose_response();
                } else {
                    // 單字模式且游標在詞尾：Tab 作為詞與詞之間的界限／阻斷符號，直接將當前選取單字上屏
                    let committed = self
                        .candidates
                        .get(self.candidate_idx)
                        .or_else(|| self.candidates.first())
                        .cloned()
                        .unwrap_or_else(|| self.buffer.clone());
                    self.record_committed(&committed);
                    self.reset();
                    return Self::commit_response(committed);
                }
            }
        }

        // 6. 萬用字元 `?` 與 `*`
        if (ch == '?' || ch == '*') && !self.buffer.is_empty() && !self.is_english_forced {
            self.buffer.push(ch);
            self.buffer_cursor = self.buffer.len();
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 7. 英文字母 'a'..'z' 或 'A'..'Z'
        if ch.is_ascii_alphabetic() {
            if ch.is_ascii_uppercase() {
                self.is_english_forced = true;
            }
            let is_word_mode = self.buffer.starts_with('-');
            let max_code_len = if is_word_mode { 12 } else { 4 };
            let code_len = self.buffer.trim_start_matches('-').len();
            let has_real_entry = !self.table.lookup(&self.buffer).is_empty();
            let next_probe = format!("{}{}", self.buffer.to_ascii_lowercase(), ch.to_ascii_lowercase());
            let continues_valid_code = self.table.has_exact_or_prefix(&next_probe);
            // 若目前已達 4 碼以上（或已由聲調數字收尾）且有實際字典候選，而新按下的字母無法構成更長的龍龕 2.0 編碼時：
            // 自動將當前首選單字上屏，並以新字母開啟下一字組字（兼顧 4 碼連打與 5~10 碼全拼部件）！
            let ends_with_tone_digit = self
                .buffer
                .chars()
                .last()
                .map(|c| matches!(c, '1'..='5'))
                .unwrap_or(false);
            if !self.is_english_forced
                && ((code_len >= max_code_len && !continues_valid_code)
                    || (code_len >= 2 && ends_with_tone_digit && !continues_valid_code))
                && !self.buffer.contains('*')
                && !self.buffer.contains('?')
                && has_real_entry
            {
                let prev_committed = self
                    .candidates
                    .get(self.candidate_idx)
                    .or_else(|| self.candidates.first())
                    .cloned()
                    .unwrap_or_default();
                self.record_committed(&prev_committed);
                self.buffer.clear();
                self.buffer.push(ch);
                self.buffer_cursor = self.buffer.len();
                self.update_candidates_with_lexicon(lexicon);
                let mut resp = self.build_compose_response();
                resp.committed_chunk = prev_committed;
                return resp;
            }

            self.buffer.push(ch);
            self.buffer_cursor = self.buffer.len();
            let new_code_len = self.buffer.trim_start_matches('-').len();
            let phrase_probe = format!(
                "-{}",
                self.buffer.trim_start_matches('-').to_ascii_lowercase()
            );
            if new_code_len > 12
                && !self.buffer.contains('*')
                && !self.buffer.contains('?')
                && !self.table.has_exact_or_prefix(&phrase_probe)
            {
                self.is_english_forced = true;
            }
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 8. 其他標點符號
        if self.buffer.is_empty() {
            let s = ch.to_string();
            self.record_committed(&s);
            Self::pass_response(&s)
        } else {
            let committed = self
                .candidates
                .get(self.candidate_idx)
                .or_else(|| self.candidates.first())
                .cloned()
                .unwrap_or_else(|| self.buffer.clone());
            let full_committed = format!("{}{}", committed, ch);
            self.record_committed(&full_committed);
            self.reset();
            Self::commit_response(full_committed)
        }
    }

    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        match key_str {
            "Space" => self.process_key(' '),
            "Tab" | "tab" => self.process_key('\t'),
            "Down" | "down" => {
                if !self.candidates.is_empty() {
                    self.candidate_idx = (self.candidate_idx + 1) % self.candidates.len();
                }
                self.build_compose_response()
            }
            "Up" | "up" => {
                if !self.candidates.is_empty() {
                    self.candidate_idx =
                        (self.candidate_idx + self.candidates.len() - 1) % self.candidates.len();
                }
                self.build_compose_response()
            }
            "PageDown" | "pagedown" | "Next" | "next" => {
                if !self.candidates.is_empty() {
                    self.candidate_idx = (self.candidate_idx + 9) % self.candidates.len();
                }
                self.build_compose_response()
            }
            "PageUp" | "pageup" | "Prior" | "prior" => {
                if !self.candidates.is_empty() {
                    let step = 9 % self.candidates.len();
                    self.candidate_idx =
                        (self.candidate_idx + self.candidates.len() - step) % self.candidates.len();
                }
                self.build_compose_response()
            }
            "Left" | "left" => {
                if self.buffer_cursor > 0 {
                    self.buffer_cursor -= 1;
                }
                self.build_compose_response()
            }
            "Right" | "right" => {
                if self.buffer_cursor < self.buffer.len() {
                    self.buffer_cursor += 1;
                }
                self.build_compose_response()
            }
            "Home" | "home" => {
                self.buffer_cursor = 0;
                self.build_compose_response()
            }
            "End" | "end" => {
                self.buffer_cursor = self.buffer.len();
                self.build_compose_response()
            }
            "Return" | "Enter" => self.process_key('\n'),
            "BackSpace" | "backspace" => self.process_key('\x08'),
            "Shift+BackSpace" | "Shift+backspace" | "Ctrl+BackSpace" | "Ctrl+backspace" => {
                self.reset();
                EngineResponse {
                    action: "update_preedit".to_string(),
                    mode: "longkan".to_string(),
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
            s if s.chars().count() == 1 => {
                let ch = s.chars().next().unwrap();
                self.process_key(ch)
            }
            _ => Self::pass_response(""),
        }
    }

    pub fn update_candidates_with_lexicon(
        &mut self,
        _lexicon: Option<&crate::lexicon::LexiconManager>,
    ) {
        if self.buffer.is_empty() {
            self.candidates.clear();
            self.candidate_idx = 0;
            return;
        }
        if self.is_english_forced {
            self.candidates = vec![self.buffer.clone()];
            self.candidate_idx = 0;
            return;
        }

        let mut entries = self.table.lookup(&self.buffer);
        // WinRAR 君子協定模式：滿 81 日且滿 81,000 字後，單字、雙字詞與前文脈絡消歧 100% 維持完整開放，僅象徵性封存 3 字詞以上簡碼
        if self.grace_degraded {
            let up_to_two_chars: Vec<LongkanEntry> = entries
                .iter()
                .filter(|e| e.text.chars().count() <= 2)
                .cloned()
                .collect();
            if !up_to_two_chars.is_empty() {
                entries = up_to_two_chars;
            }
        }
        if entries.is_empty() {
            self.candidates = vec![self.buffer.clone()];
            self.candidate_idx = 0;
            return;
        }

        // 結合前文脈絡 (TransitionEngine) 對重碼候選進行智慧消歧排序（WinRAR 模式下永久保持啟用）
        // 嚴格確保「精確碼長匹配 (is_exact)」永遠排在「長碼前綴預覽 (prefix_hit)」之前，防止長碼權重反客為主覆蓋精確單字！
        let clean_buf = self.buffer.trim().trim_start_matches('-').to_ascii_lowercase();
        let prev = self.last_committed_word.trim().to_string();
        let mut scored: Vec<(String, bool, f64)> = entries
            .into_iter()
            .map(|e| {
                let is_exact = e.code == clean_buf || e.code == "-";
                let mut score = e.weight as f64;
                if !prev.is_empty() {
                    let bonus = self
                        .transitions
                        .calculate_transition_bonus(&prev, "", &e.text, "");
                    score += bonus * 25.0;
                }
                (e.text, is_exact, score)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| {
                b.2.partial_cmp(&a.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        self.candidates = scored.into_iter().map(|(t, _, _)| t).collect();
        self.candidate_idx = 0;
    }

    fn build_compose_response(&self) -> EngineResponse {
        let preedit = self.make_preedit();
        let active = self
            .candidates
            .get(self.candidate_idx)
            .cloned()
            .unwrap_or_else(|| self.buffer.clone());
        let cursor = if active == self.buffer {
            self.buffer_cursor.min(active.chars().count())
        } else {
            active.chars().count()
        };
        EngineResponse {
            action: "update_preedit".to_string(),
            mode: "longkan".to_string(),
            preedit,
            composed: active.clone(),
            candidates: self.candidates.clone(),
            active_candidate: active,
            candidate_idx: self.candidate_idx,
            cursor_pos: cursor,
            q_press_count: 0,
            committed_chunk: String::new(),
        }
    }

    fn commit_response(text: String) -> EngineResponse {
        let len = text.chars().count();
        EngineResponse {
            action: "commit".to_string(),
            mode: "longkan".to_string(),
            preedit: String::new(),
            composed: String::new(),
            candidates: Vec::new(),
            active_candidate: text.clone(),
            candidate_idx: 0,
            cursor_pos: len,
            q_press_count: 0,
            committed_chunk: text,
        }
    }

    fn pass_response(text: &str) -> EngineResponse {
        EngineResponse {
            action: "pass".to_string(),
            mode: "longkan".to_string(),
            preedit: String::new(),
            composed: String::new(),
            candidates: Vec::new(),
            active_candidate: text.to_string(),
            candidate_idx: 0,
            cursor_pos: 0,
            q_press_count: 0,
            committed_chunk: text.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_seq(engine: &mut LongkanEngine, seq: &str) -> EngineResponse {
        let mut last = LongkanEngine::pass_response("");
        for c in seq.chars() {
            last = engine.process_key(c);
        }
        last
    }

    #[test]
    fn test_longkan_radical_phonetic_and_flagship_kan() {
        let mut engine = LongkanEngine::new();
        // 1. 「鋪」= 金(km) + 甫(fu) -> kmfu (羅馬/台羅基底) 或 jnfx (許氏基底)
        let r_pu = type_seq(&mut engine, "kmfu ");
        assert_eq!(r_pu.committed_chunk, "鋪");

        let r_pu_hsu = type_seq(&mut engine, "jnfx ");
        assert_eq!(r_pu_hsu.committed_chunk, "鋪");

        // 2. 龍龕活招牌「龕」= 合(he/ho/hg) + 龍(lo/lg/ll) -> helo / holg / hgll
        let r_kan1 = type_seq(&mut engine, "helo ");
        assert_eq!(r_kan1.committed_chunk, "龕");

        let r_kan2 = type_seq(&mut engine, "hgll ");
        assert_eq!(r_kan2.committed_chunk, "龕");
    }

    #[test]
    fn test_longkan_visual_pictograph_and_tone_padding() {
        let mut engine = LongkanEngine::new();
        // 1. 「肴」= 乂(x) + 有(eo) + Space/2/3/f -> xeo / xeo2 / xeo3 / xeof
        for code in ["xeo ", "xeo2 ", "xeo3 ", "xeof "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "肴", "Failed on code: {}", code);
        }

        // 2. 「凶」= 乂(x) + 凵(u) + Space/1 -> xu / xu1
        for code in ["xu ", "xu1 ", "ux "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "凶", "Failed on code: {}", code);
        }
    }

    #[test]
    fn test_longkan_component_multiplicity_qu_pin_sen_xie_rui() {
        let mut engine = LongkanEngine::new();
        // 1. 「區」= cpn / cpn3 / cpnf / cko3 (匚c + 口ko×3)
        for code in ["cpn ", "cpn3 ", "cpnf ", "cko3 "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "區", "Failed on code: {}", code);
        }

        // 2. 「品」= ko3 / ko33 / ko3f
        for code in ["ko3 ", "ko33 ", "ko3f "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "品", "Failed on code: {}", code);
        }

        // 3. 「森」= mu3 / mu31, 「林」= mu2 / mu22, 「晶」= ri3 / ri31, 「協」= tli3, 「蕊」= wsm3
        assert_eq!(type_seq(&mut engine, "mu3 ").committed_chunk, "森");
        assert_eq!(type_seq(&mut engine, "mu31 ").committed_chunk, "森");
        assert_eq!(type_seq(&mut engine, "mu2 ").committed_chunk, "林");
        assert_eq!(type_seq(&mut engine, "ri3 ").committed_chunk, "晶");
        assert_eq!(type_seq(&mut engine, "tli3 ").committed_chunk, "協");
        assert_eq!(type_seq(&mut engine, "wsm3 ").committed_chunk, "蕊");
    }

    #[test]
    fn test_longkan_reverse_lookup_coach() {
        let engine = LongkanEngine::new();
        let hint_pu = engine.table.format_char_coach('鋪').unwrap_or_default();
        assert!(hint_pu.contains("KMFU") && hint_pu.contains("金(km)+甫(fu)"));
        let hint_qu = engine.table.format_char_coach('區').unwrap_or_default();
        assert!(hint_qu.contains("匚(c)"));
    }

    #[test]
    fn test_longkan_simplest_component_and_hsu_friendly() {
        let mut engine = LongkanEngine::new();
        // 1. 龍: lnln, liln, yeln; lxl2
        for code in ["lnln ", "liln ", "yeln ", "lxl2 "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "龍", "Failed on 龍 code: {}", code);
        }
        // 2. 龕: holn; hgln
        for code in ["holn ", "hgln "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "龕", "Failed on 龕 code: {}", code);
        }
        // 3. 形: jn3, jnsn; jnse, jncn
        for code in ["jn3 ", "jnsn ", "jnse ", "jncn "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "形", "Failed on 形 code: {}", code);
        }
        // 4. 音: lir4, inin
        for code in ["lir4 ", "inin "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "音", "Failed on 音 code: {}", code);
        }
        // 5. 法: seci, sefa
        for code in ["seci ", "sefa "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "法", "Failed on 法 code: {}", code);
        }
        // 6. 銀: jnin (金+銀), jnen (許氏金jn+許氏銀en), jni2 / jne2 (2+1+調), jngn (金+右旁艮)
        for code in ["jnin ", "jnen ", "jni2 ", "jne2 "] {
            let resp = type_seq(&mut engine, code);
            assert_eq!(resp.committed_chunk, "銀", "Failed on 銀 code: {}", code);
        }
        assert!(
            engine.table.lookup("jngn").iter().any(|e| e.text == "銀"),
            "Expected 銀 in jngn candidates"
        );
    }

    #[test]
    fn test_longkan_word_prefix_and_tab_mode() {
        let mut engine = LongkanEngine::new();
        // 1. Tab 作為詞彙簡碼引導與結束符號（統一 N+2 公式：第一字前兩碼 + 中間字第一碼們 + 最後一字後兩碼）：
        //    - 2字詞（4碼）：`\tceru\t` -> 「輸入」、`\tlnho ` / `\tlnln\t` -> 「龍龕」、`\tjnr4\t` -> 「形音」、`\ttawn\t` -> 「台灣」
        assert_eq!(type_seq(&mut engine, "\tceru\t").committed_chunk, "輸入");
        assert_eq!(type_seq(&mut engine, "\tsuru ").committed_chunk, "輸入");
        assert_eq!(type_seq(&mut engine, "\tlnln\t").committed_chunk, "龍龕");
        assert_eq!(type_seq(&mut engine, "\tjnr4\t").committed_chunk, "形音");
        assert_eq!(type_seq(&mut engine, "\ttawn\t").committed_chunk, "台灣");

        //    - 3字詞（5碼）：`\tcerci\t` / `\tcerfa\t` -> 「輸入法」
        assert_eq!(type_seq(&mut engine, "\tcerci\t").committed_chunk, "輸入法");
        assert_eq!(type_seq(&mut engine, "\tcerfa ").committed_chunk, "輸入法");

        //    - 4字詞（6碼）：`\tlnhjr4\t` / `\tlnhjin\t` -> 「龍龕形音」
        assert_eq!(
            type_seq(&mut engine, "\tlnhjr4\t").committed_chunk,
            "龍龕形音"
        );
        assert_eq!(
            type_seq(&mut engine, "\tlnhjin ").committed_chunk,
            "龍龕形音"
        );

        //    - 5字詞（7碼 N+2 或 6碼封頂）：`\tlnhjlci\t` / `\tlnhjci\t` -> 「龍龕形音法」
        assert_eq!(
            type_seq(&mut engine, "\tlnhjlci\t").committed_chunk,
            "龍龕形音法"
        );
        assert_eq!(
            type_seq(&mut engine, "\tlnhjci\t").committed_chunk,
            "龍龕形音法"
        );

        // 2. 忘記先按 Tab 時，移動文字游標至詞界開頭（Home 或 Left）補按 Tab：
        let _ = type_seq(&mut engine, "cerci");
        let _ = engine.process_key_str("Home");
        let resp_tab = engine.process_key_str("Tab");
        assert_eq!(resp_tab.preedit, "🐉⇥詞:CERCI");
        assert_eq!(resp_tab.active_candidate, "輸入法");
        let resp_commit = engine.process_key_str("Tab");
        assert_eq!(resp_commit.committed_chunk, "輸入法");

        // 3. 單字連打行雲流水（不需按空白或 Tab，第 5 碼自動頂字上屏）與詞尾 Tab 阻斷：
        let _ = type_seq(&mut engine, "lnln");
        let resp_5th = engine.process_key('h');
        assert_eq!(resp_5th.committed_chunk, "龍");
        let resp_2nd_char = type_seq(&mut engine, "oln\t");
        assert_eq!(resp_2nd_char.committed_chunk, "龕");

        // 4. 驗證「吃飽再回來」與「看、問、和、到、別、想、拿、樹、識、聽」按 Enter ('\n') 均確認送出中文漢字：
        assert_eq!(type_seq(&mut engine, "kocc\n").committed_chunk, "吃");
        assert_eq!(type_seq(&mut engine, "kovv\n").committed_chunk, "吃");
        assert_eq!(type_seq(&mut engine, "sibo\n").committed_chunk, "飽");
        assert_eq!(type_seq(&mut engine, "ccbw\n").committed_chunk, "飽");
        assert_eq!(type_seq(&mut engine, "sib3\n").committed_chunk, "飽");
        assert_eq!(type_seq(&mut engine, "mizi\n").committed_chunk, "再");
        assert_eq!(type_seq(&mut engine, "ko2\n").committed_chunk, "回");
        assert_eq!(type_seq(&mut engine, "ko22\n").committed_chunk, "回");
        assert_eq!(type_seq(&mut engine, "rnli\n").committed_chunk, "來");
        assert_eq!(type_seq(&mut engine, "somu\n").committed_chunk, "看");
        assert_eq!(type_seq(&mut engine, "comx\n").committed_chunk, "看");
        assert_eq!(type_seq(&mut engine, "mnko\n").committed_chunk, "問");
        assert_eq!(type_seq(&mut engine, "heko\n").committed_chunk, "和");
        assert_eq!(type_seq(&mut engine, "zido\n").committed_chunk, "到");
        assert_eq!(type_seq(&mut engine, "kodo\n").committed_chunk, "別");
        assert_eq!(type_seq(&mut engine, "xnsn\n").committed_chunk, "想");
        assert_eq!(type_seq(&mut engine, "heso\n").committed_chunk, "拿");
        assert_eq!(type_seq(&mut engine, "mucn\n").committed_chunk, "樹");
        assert_eq!(type_seq(&mut engine, "ynge\n").committed_chunk, "識");
        assert_eq!(type_seq(&mut engine, "ersn\n").committed_chunk, "聽");
        assert_eq!(type_seq(&mut engine, "mayo\n").committed_chunk, "麼");
        assert_eq!(type_seq(&mut engine, "maao\n").committed_chunk, "麼");
        assert_eq!(type_seq(&mut engine, "mame\n").committed_chunk, "麼");
        assert_eq!(type_seq(&mut engine, "bbe\n").committed_chunk, "負");
        assert_eq!(type_seq(&mut engine, "b1be\n").committed_chunk, "負");
        assert_eq!(type_seq(&mut engine, "bbbe\n").committed_chunk, "負");
        assert_eq!(type_seq(&mut engine, "bobe\n").committed_chunk, "負");
        assert_eq!(type_seq(&mut engine, "nbe\n").committed_chunk, "負");
        assert_eq!(type_seq(&mut engine, "nnbe\n").committed_chunk, "負");
        assert_eq!(type_seq(&mut engine, "aoli\n").committed_chunk, "幼");
        assert_eq!(type_seq(&mut engine, "yoli\n").committed_chunk, "幼");
        assert_eq!(type_seq(&mut engine, "jido\n").committed_chunk, "到");
        assert_eq!(type_seq(&mut engine, "znli\n").committed_chunk, "動");
        assert_eq!(type_seq(&mut engine, "jnli\n").committed_chunk, "動");
    }
}
