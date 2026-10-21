// ruhi-core/src/array30/mod.rs — 如喜輸入法「行列 30 (Array 30)」零切換智慧引擎
//
// 核心設計特色：
// 1. 遵循廖明德先生公開授權之《行列 30 輸入法 Porting 規格書》(v2026)：
//    - 30 個字根鍵位：a-z 與 , . / ; 對應 1^ 1- 1v ... 0^ 0- 0v 座標顯示
//    - 一級簡碼 (1 碼 + 數字鍵 1..0) 與二級簡碼 (2 碼 + 數字鍵 1..0) 即時預覽與快選
//    - 二鍵特別碼 (398 個高頻字如 ak=大、pc=是、lo=我) 空白鍵直出
//    - W 鍵符號表 (w0..w9) 與 HG 擴充符號表 (hg1..hg8) 免空白鍵即時展開
//    - 萬用字元查詢：組字中支援 `?`（單一任意鍵）與 `*`（零或多個任意鍵）模糊查字
// 2. 結合如喜 Viterbi 語意詞網與 TransitionEngine：
//    - 同碼重碼字（如 if -> 針/羊/并、ae -> 正/耳）依據前文語境智慧消歧
//    - 支援多字詞首碼快打與如喜輸入座長句連打緩衝區無縫整合
// 3. 零切換中英混打 (Zero-Switch English Passthrough)：
//    - 按空白鍵上屏漢字；按 Enter 鍵直接輸出原始英文字母
//    - 超過 5 碼純英文字母或含大寫字母時自動轉為英文直通模式
// 4. 許氏注音 ➔ 行列 30 即時反查教練：
//    - 提供 `format_char_coach` 與 `format_text_coach`，即時顯示漢字之行列筆形座標（如 18=Z）與拆碼步驟

pub mod table;

pub use self::table::{
    code_to_coords_display, get_key_meta, is_array30_radical_key, key_to_coord, Array30Entry,
    Array30KeyMeta, Array30ReverseEntry, Array30Table, ARRAY30_KEY_METAS,
};
use crate::engine::EngineResponse;
use crate::router::transitions::TransitionEngine;
use std::path::Path;

/// 行列 30 標準最大碼長為 4 碼，加上罕用/異體字尾碼 `i` (8^) 最長為 5 碼
const MAX_ARRAY30_CODE_LEN: usize = 5;

#[derive(Debug, Clone)]
pub struct Array30Engine {
    pub table: Array30Table,
    pub transitions: TransitionEngine,
    /// 當前輸入的原始按鍵序列（如 "zps", "w1", "a?d"）
    pub buffer: String,
    /// 是否已進入英文直通模式（例如超過 5 碼純英文或首字母大寫）
    pub is_english_forced: bool,
    /// 當前即時預覽的候選字清單
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
    pub last_committed_word: String,
}

impl Default for Array30Engine {
    fn default() -> Self {
        Self {
            table: Array30Table::new(),
            transitions: TransitionEngine::new(),
            buffer: String::new(),
            is_english_forced: false,
            candidates: Vec::new(),
            candidate_idx: 0,
            last_committed_word: String::new(),
        }
    }
}

impl Array30Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_tsv(path: &Path) -> Self {
        let mut engine = Self::default();
        let _ = engine.table.load_tsv(path);
        engine
    }

    /// 重置行列 30 組字緩衝區
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.is_english_forced = false;
        self.candidates.clear();
        self.candidate_idx = 0;
    }

    pub fn has_buffer(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// 記錄已上屏之字詞或數字脈絡，供行列重碼字進行上下文智慧消歧
    pub fn record_committed(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
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

    /// 判斷當前緩衝區是否處於 W0..W9 或 HG1..HG8 符號選單模式
    pub fn is_symbol_menu_active(&self) -> bool {
        let lower = self.buffer.to_ascii_lowercase();
        (lower.len() == 2 && lower.starts_with('w') && lower.as_bytes()[1].is_ascii_digit())
            || (lower.len() == 3 && lower.starts_with("hg") && lower.as_bytes()[2].is_ascii_digit())
    }

    /// 取得當前組字區的顯示文字（行列座標如 "1v 0^ 2-" 或英文直通字串）
    pub fn make_preedit(&self) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }
        if self.is_english_forced {
            return self.buffer.clone();
        }
        code_to_coords_display(&self.buffer)
    }

    /// 處理單一按鍵輸入（無外部詞庫版本）
    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        self.process_key_with_lexicon(ch, None)
    }

    /// 處理單一按鍵輸入（結合如喜上下文語境與詞庫進行重碼字消歧與多字詞首碼展開）
    pub fn process_key_with_lexicon(
        &mut self,
        ch: char,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) -> EngineResponse {
        // 1. Backspace ('\x08')：退格刪除一碼
        if ch == '\x08' {
            if self.buffer.is_empty() {
                return Self::pass_response("");
            }
            self.buffer.pop();
            if self.buffer.is_empty() {
                self.reset();
                return EngineResponse {
                    action: "reset".to_string(),
                    mode: "array30".to_string(),
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

        // 2. Escape ('\x1b')：清空組字緩衝區
        if ch == '\x1b' {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "array30".to_string(),
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

        // 3. Enter ('\n' 或 '\r')：零切換英文直通，直接輸出原始按鍵字串
        if ch == '\n' || ch == '\r' {
            if self.buffer.is_empty() {
                self.last_committed_word.clear();
                return Self::pass_response("");
            }
            let raw = self.buffer.clone();
            self.record_committed(&raw);
            self.reset();
            return Self::commit_response(raw);
        }

        // 4. Space (' ')：確認組字，輸出最佳行列候選字（或英文直通單字）
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

            let committed = self.resolve_space_commit(lexicon);
            self.record_committed(&committed);
            self.reset();
            return Self::commit_response(committed);
        }

        // 5. 數字鍵 '0'..'9'：
        //    (a) 若當前 buffer 為 "w" 或 "hg"，按數字鍵立即展開對應的符號表 (w0..w9 / hg1..hg8)！
        //    (b) 若當前已有候選字（含一級簡碼、二級簡碼、重碼字、符號清單），按 1..9, 0 (0 代表第 10 個) 直接選字上屏！
        if ch.is_ascii_digit() {
            if self.buffer.is_empty() {
                let s = ch.to_string();
                self.record_committed(&s);
                return Self::pass_response(&s);
            }
            if !self.is_english_forced {
                let lower_buf = self.buffer.to_ascii_lowercase();
                if lower_buf == "w"
                    || (lower_buf == "hg" && matches!(ch, '1' | '2' | '8'))
                {
                    self.buffer.push(ch);
                    self.update_candidates_with_lexicon(lexicon);
                    return self.build_compose_response();
                }

                // 選字索引：'1'..'9' -> 0..8, '0' -> 9 (第 10 個候選字)
                let idx = if ch == '0' {
                    9
                } else {
                    (ch as usize) - ('1' as usize)
                };

                // 若為 1 碼或 2 碼，優先檢查一級/二級簡碼對應槽位（遵循 ar30hw.txt 1..0 定位規則）
                if lower_buf.len() == 1 {
                    let s1_list = self.table.lookup_short1(&lower_buf);
                    if idx < s1_list.len() {
                        let chosen = s1_list[idx].clone();
                        self.record_committed(&chosen);
                        self.reset();
                        return Self::commit_response(chosen);
                    }
                } else if lower_buf.len() == 2 && !self.is_symbol_menu_active() {
                    let s2_list = self.table.lookup_short2(&lower_buf);
                    if idx < s2_list.len() {
                        let chosen = s2_list[idx].clone();
                        self.record_committed(&chosen);
                        self.reset();
                        return Self::commit_response(chosen);
                    }
                }

                if idx < self.candidates.len() {
                    let chosen = self.candidates[idx].clone();
                    self.record_committed(&chosen);
                    self.reset();
                    return Self::commit_response(chosen);
                }
                return self.build_compose_response();
            }

            // 若為英文直通模式，將數字併入英文緩衝區
            self.buffer.push(ch);
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 6. 萬用字元 `?` 與 `*`（僅在緩衝區非空且非英文直通模式時作為行列萬用查詢鍵）
        if (ch == '?' || ch == '*') && !self.buffer.is_empty() && !self.is_english_forced {
            self.buffer.push(ch);
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 7. 行列 30 標準字根鍵 ('a'..'z', ',', '.', '/', ';') 或大寫英文字母 ('A'..'Z')
        if is_array30_radical_key(ch) || ch.is_ascii_uppercase() {
            if ch.is_ascii_uppercase() {
                self.is_english_forced = true;
            }
            self.buffer.push(ch);
            self.update_candidates_with_lexicon(lexicon);
            return self.build_compose_response();
        }

        // 8. 其他按鍵（底線、連字號或外部標點符號）
        if self.buffer.is_empty() {
            let s = ch.to_string();
            self.record_committed(&s);
            Self::pass_response(&s)
        } else if ch == '_' || ch == '-' {
            self.is_english_forced = true;
            self.buffer.push(ch);
            self.update_candidates_with_lexicon(lexicon);
            self.build_compose_response()
        } else {
            let committed = if self.is_english_forced {
                self.buffer.clone()
            } else {
                self.resolve_space_commit(lexicon)
            };
            let full_committed = format!("{}{}", committed, ch);
            self.record_committed(&full_committed);
            self.reset();
            Self::commit_response(full_committed)
        }
    }

    /// 支援具名鍵位字串
    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        match key_str {
            "Space" => self.process_key(' '),
            "Return" | "Enter" => self.process_key('\n'),
            "BackSpace" | "backspace" => self.process_key('\x08'),
            "Shift+BackSpace" | "Shift+backspace" | "Ctrl+BackSpace" | "Ctrl+backspace" => {
                self.reset();
                EngineResponse {
                    action: "update_preedit".to_string(),
                    mode: "array30".to_string(),
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
            _ => {
                let preedit = self.make_preedit();
                let active = self.candidates.first().cloned().unwrap_or_default();
                EngineResponse {
                    action: "pass".to_string(),
                    mode: "array30".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: self.candidate_idx,
                    cursor_pos: self.buffer.len(),
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
        }
    }

    /// 解析空白鍵上屏目標：
    /// 依據行列 30 Porting 規格，按空白鍵時優先查詢該碼的「標準全碼/特別碼 (`table.lookup`)」，
    /// 並結合上下文語境對同碼重碼字排序；若無全碼/特別碼匹配，則退回採用 `candidates[0]`。
    pub fn resolve_space_commit(
        &mut self,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) -> String {
        let lower = self.buffer.to_ascii_lowercase();
        let mut exact_matches = self.table.lookup(&lower);

        if exact_matches.is_empty() {
            if let Some(lex) = lexicon {
                exact_matches = self.table.lookup_phrase_headcode(&lower, lex);
            }
        }

        if !exact_matches.is_empty() {
            self.rank_candidates_by_context(&mut exact_matches, lexicon);
            return exact_matches.remove(0);
        }

        self.candidates
            .first()
            .cloned()
            .unwrap_or_else(|| self.buffer.clone())
    }

    /// 重新計算組字狀態、英文直通判定與候選字排序
    pub fn update_candidates_with_lexicon(
        &mut self,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) {
        self.candidates.clear();
        self.candidate_idx = 0;

        if self.buffer.is_empty() {
            return;
        }

        let has_uppercase = self.buffer.chars().any(|c| c.is_ascii_uppercase());
        let has_array_punct_or_wildcard = self
            .buffer
            .chars()
            .any(|c| matches!(c, ',' | '.' | '/' | ';' | '?' | '*'));

        // 若含大寫字母，或超過 5 碼且全為純英文字母（不含 , . / ; ? *），自動轉為英文直通模式
        if has_uppercase
            || self.is_english_forced
            || (self.buffer.len() > MAX_ARRAY30_CODE_LEN && !has_array_punct_or_wildcard)
        {
            self.is_english_forced = true;
            self.candidates = vec![self.buffer.clone()];
            return;
        }

        let lower = self.buffer.to_ascii_lowercase();

        // 若為 W0..W9 或 HG1..HG8 符號表，直接載入符號候選清單
        if self.is_symbol_menu_active() {
            self.is_english_forced = false;
            self.candidates = self.table.lookup_symbols(&lower);
            return;
        }

        // 若含 ? 或 * 萬用字元，執行萬用字元查詢
        if lower.contains('?') || lower.contains('*') {
            self.is_english_forced = false;
            self.candidates = self.table.lookup_wildcard(&lower);
            return;
        }

        // 1 碼時：優先顯示一級簡碼 1..10 供數字鍵快選，並附加全碼字（如 t -> 的）
        if lower.len() == 1 {
            self.is_english_forced = false;
            let mut cands = self.table.lookup_short1(&lower);
            for exact in self.table.lookup(&lower) {
                if !cands.contains(&exact) {
                    cands.push(exact);
                }
            }
            if !has_array_punct_or_wildcard && !cands.contains(&self.buffer) {
                cands.push(self.buffer.clone());
            }
            self.candidates = cands;
            return;
        }

        // 2 碼時：優先維持二級簡碼 1..10 槽位對齊供數字鍵快選，並整合二鍵全碼/特別碼與前綴聯想
        if lower.len() == 2 {
            let mut exact_cands = self.table.lookup(&lower);
            self.rank_candidates_by_context(&mut exact_cands, lexicon);
            let s2 = self.table.lookup_short2(&lower);
            let mut cands = if !s2.is_empty() {
                let mut list = s2;
                for exact in exact_cands {
                    if !list.contains(&exact) {
                        list.push(exact);
                    }
                }
                list
            } else {
                exact_cands
            };
            if let Some(lex) = lexicon {
                for phrase in self.table.lookup_phrase_headcode(&lower, lex) {
                    if !cands.contains(&phrase) {
                        cands.push(phrase);
                    }
                }
            }
            for pref in self.table.lookup_with_prefix(&lower, 12) {
                if !cands.contains(&pref) {
                    cands.push(pref);
                }
            }
            if !cands.is_empty() {
                self.is_english_forced = false;
                if !has_array_punct_or_wildcard && !cands.contains(&self.buffer) {
                    cands.push(self.buffer.clone());
                }
                self.candidates = cands;
            } else {
                self.candidates = vec![self.buffer.clone()];
            }
            return;
        }

        // 3~5 碼：精確全碼（結合語境排序）+ 多字詞首碼 + 前綴聯想
        let mut exact_cands = self.table.lookup(&lower);
        self.rank_candidates_by_context(&mut exact_cands, lexicon);

        let mut cands = exact_cands;
        if cands.is_empty() {
            if let Some(lex) = lexicon {
                for phrase in self.table.lookup_phrase_headcode(&lower, lex) {
                    if !cands.contains(&phrase) {
                        cands.push(phrase);
                    }
                }
            }
        }
        for pref in self.table.lookup_with_prefix(&lower, 12) {
            if !cands.contains(&pref) {
                cands.push(pref);
            }
        }

        if cands.is_empty() {
            self.candidates = vec![self.buffer.clone()];
        } else {
            self.is_english_forced = false;
            if !has_array_punct_or_wildcard && !cands.contains(&self.buffer) {
                cands.push(self.buffer.clone());
            }
            self.candidates = cands;
        }
    }

    /// 根據上文語境 (`last_committed_word`) 對同碼重碼候選字進行智慧消歧排序
    fn rank_candidates_by_context(
        &mut self,
        candidates: &mut Vec<String>,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) {
        if candidates.len() <= 1 || self.last_committed_word.is_empty() {
            return;
        }
        let prev = self.last_committed_word.as_str();
        let prev_tail = prev
            .chars()
            .last()
            .map(|c| c.to_string())
            .unwrap_or_default();
        if prev_tail.is_empty() {
            return;
        }

        let mut scored: Vec<(String, f64, usize)> = candidates
            .iter()
            .enumerate()
            .map(|(idx, cand)| {
                // 強烈保護廖明德先生官方首選字位階（非強語境搭配不隨意翻轉第 1 字）
                let base_rank_score = -(idx as f64) * 35.0;
                let mut trans_bonus =
                    self.transitions.calculate_transition_bonus(prev, "", cand, "");
                if trans_bonus == 0.0 && !prev_tail.is_empty() && prev_tail != prev {
                    trans_bonus = self
                        .transitions
                        .calculate_transition_bonus(&prev_tail, "", cand, "");
                }

                let mut lex_bonus = 0.0;
                if let Some(lex) = lexicon {
                    let cand_word = format!("{}{}", prev, cand);
                    let tail_word = format!("{}{}", prev_tail, cand);
                    if lex.word_index.contains_key(&cand_word)
                        || (!prev_tail.is_empty() && lex.word_index.contains_key(&tail_word))
                    {
                        lex_bonus = 42.0;
                    }
                }

                (cand.clone(), base_rank_score + trans_bonus + lex_bonus, idx)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.cmp(&b.2))
        });
        *candidates = scored.into_iter().map(|(c, _, _)| c).collect();
    }

    fn build_compose_response(&self) -> EngineResponse {
        let preedit = self.make_preedit();
        let lower = self.buffer.to_ascii_lowercase();
        let active = if self.is_english_forced {
            self.buffer.clone()
        } else if self.is_symbol_menu_active() {
            preedit.clone()
        } else if let Some(exact) = self.table.lookup(&lower).into_iter().next() {
            exact
        } else {
            self.candidates
                .first()
                .cloned()
                .unwrap_or_else(|| self.buffer.clone())
        };
        EngineResponse {
            action: "compose".to_string(),
            mode: if self.is_english_forced {
                "array30_en".to_string()
            } else {
                "array30".to_string()
            },
            preedit,
            composed: active.clone(),
            candidates: self.candidates.clone(),
            active_candidate: active,
            candidate_idx: self.candidate_idx,
            cursor_pos: self.buffer.chars().count(),
            q_press_count: 0,
            committed_chunk: String::new(),
        }
    }

    fn commit_response(text: String) -> EngineResponse {
        EngineResponse {
            action: "commit".to_string(),
            mode: "array30".to_string(),
            preedit: String::new(),
            composed: String::new(),
            candidates: Vec::new(),
            active_candidate: String::new(),
            candidate_idx: 0,
            cursor_pos: 0,
            q_press_count: 0,
            committed_chunk: text,
        }
    }

    fn pass_response(text: &str) -> EngineResponse {
        EngineResponse {
            action: "pass".to_string(),
            mode: "array30".to_string(),
            preedit: String::new(),
            composed: String::new(),
            candidates: Vec::new(),
            active_candidate: String::new(),
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

    #[test]
    fn test_array30_basic_coords_and_full_codes() {
        let mut engine = Array30Engine::new();

        // 1. 輸入 "z" -> 顯示座標 "1v"，一級簡碼第 1 字為「不」、第 2 字為「大」
        let r1 = engine.process_key('z');
        assert_eq!(r1.action, "compose");
        assert_eq!(r1.preedit, "1v");
        assert_eq!(r1.candidates[0], "不");
        assert_eq!(r1.candidates[1], "大");

        // 按空白鍵上屏全碼首選「不」
        let r_commit = engine.process_key(' ');
        assert_eq!(r_commit.action, "commit");
        assert_eq!(r_commit.committed_chunk, "不");

        // 2. 輸入 ".aad" + Space -> 「行」 (座標 "9v 1- 1- 3-")
        for c in ".aad".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.make_preedit(), "9v 1- 1- 3-");
        let r_hang = engine.process_key(' ');
        assert_eq!(r_hang.action, "commit");
        assert_eq!(r_hang.committed_chunk, "行");
    }

    #[test]
    fn test_array30_shortcodes_and_special_codes() {
        let mut engine = Array30Engine::new();

        // 1. 一級簡碼：按 '.' (9v) 再按數字鍵 '3' -> 直接上屏「行」！
        engine.process_key('.');
        let r_short1 = engine.process_key('3');
        assert_eq!(r_short1.action, "commit");
        assert_eq!(r_short1.committed_chunk, "行");

        // 2. 特別碼：按 "ak" + 空白鍵 -> 直接上屏「大」；"pc" + 空白鍵 -> 「是」
        engine.process_key('a');
        engine.process_key('k');
        let r_sp_da = engine.process_key(' ');
        assert_eq!(r_sp_da.committed_chunk, "大");

        engine.process_key('p');
        engine.process_key('c');
        let r_sp_shi = engine.process_key(' ');
        assert_eq!(r_sp_shi.committed_chunk, "是");
    }

    #[test]
    fn test_array30_w_symbol_table_and_wildcards() {
        let mut engine = Array30Engine::new();

        // 1. W 鍵符號表：按 'w' 再按 '1'（免按空白鍵），立即展開標點符號表
        engine.process_key('w');
        let r_w1 = engine.process_key('1');
        assert_eq!(r_w1.action, "compose");
        assert!(r_w1.preedit.contains("2^1"));
        assert_eq!(r_w1.candidates[0], "，");
        assert_eq!(r_w1.candidates[1], "、");
        assert_eq!(r_w1.candidates[2], "。");

        // 再按數字鍵 '3' -> 上屏第 3 個符號「。」
        let r_sym = engine.process_key('3');
        assert_eq!(r_sym.action, "commit");
        assert_eq!(r_sym.committed_chunk, "。");

        // 2. 萬用字元查詢：輸入 ".?ad" 或 ".*d" 可查出「行」(.aad)
        for c in ".?ad".chars() {
            engine.process_key(c);
        }
        assert!(
            engine.candidates.contains(&"行".to_string()),
            "萬用字元 .?ad 必須能查出『行』(.aad)，實際候選：{:?}",
            engine.candidates
        );
        engine.reset();

        for c in "z*s".chars() {
            engine.process_key(c);
        }
        assert!(
            engine.candidates.contains(&"電".to_string()),
            "萬用字元 z*s 必須能查出『電』(zps)，實際候選：{:?}",
            engine.candidates
        );
    }

    #[test]
    fn test_array30_reverse_lookup_coach() {
        let engine = Array30Engine::new();

        // 驗證「大」的教練提示包含筆形座標 18=Z 與特別碼 AK、一級簡碼 Z2
        let coach_da = engine
            .table
            .format_char_coach('大')
            .expect("應能反查『大』");
        assert!(
            coach_da.contains("18=Z"),
            "『大』的反查教練提示應包含筆形座標 18=Z，實際：{}",
            coach_da
        );
        assert!(
            coach_da.contains("AK") && coach_da.contains("Z2"),
            "『大』的反查教練提示應包含特別碼 AK 與一級簡碼 Z2，實際：{}",
            coach_da
        );

        // 驗證「行」的教練提示包含 .AAD 與 98=.
        let coach_hang = engine
            .table
            .format_char_coach('行')
            .expect("應能反查『行』");
        assert!(
            coach_hang.contains(".AAD") && coach_hang.contains("98=."),
            "『行』的反查教練提示應包含 .AAD 與 98=.，實際：{}",
            coach_hang
        );
    }
}
