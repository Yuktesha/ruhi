pub mod table;

use std::collections::HashSet;
use std::path::Path;

use crate::engine::EngineResponse;
use crate::lexicon::LexiconManager;
use crate::router::transitions::TransitionEngine;
pub use table::{
    digit_stroke_short_name, neighbor_fuzzy_digits, FourCornerEntry, FourCornerKeyMeta,
    FourCornerTable, FOUR_CORNER_KEY_METAS,
};

/// 如喜 (RuhI) 智慧型四角號碼 4×5 數字小鍵盤輸入引擎
#[derive(Debug, Clone)]
pub struct FourCornerEngine {
    pub table: FourCornerTable,
    /// 當前輸入之數字／符號碼緩衝區（如 `"2360"`, `"2332"`, `"23.0"`, `"/"`）
    pub buffer: String,
    /// 當前即時計算並經語境排序後之候選字詞清單
    pub candidates: Vec<String>,
    /// 當前高亮之候選索引
    pub candidate_idx: usize,
    /// 是否處於數字鍵 (1~0) 選字鎖定模式（由 `/`、`↓`、`PgDn/PgUp` 或 `.` 觸發）
    pub is_selecting_candidate: bool,
    /// 小鍵盤外圍符號鍵配置方案：
    /// - `"blind_closed_loop"`（預設）：全盲打閉環（`-` 詞前導／選單上頁、`+` 字詞分隔／選單下頁、`/` 字詞選單、`1~0` 數字快選、`.` 標點選單、`*` 萬用字元、`Enter` 整句完稿）
    /// - `"direct_punct"`：直出標點模式（`+` 確認首選、`*` 句號、`/` 逗號、`-` 問號、`.` 選字／萬用字元）
    pub keymap_profile: String,
    /// 最近一次上屏之字詞（供雙字／跨詞語境轉移消歧使用）
    pub last_committed_word: String,
}

impl Default for FourCornerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FourCornerEngine {
    pub fn new() -> Self {
        Self {
            table: FourCornerTable::new(),
            buffer: String::new(),
            candidates: Vec::new(),
            candidate_idx: 0,
            is_selecting_candidate: false,
            keymap_profile: "blind_closed_loop".to_string(),
            last_committed_word: String::new(),
        }
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        self.table.load_tsv(path)
    }

    pub fn has_buffer(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.candidates.clear();
        self.candidate_idx = 0;
        self.is_selecting_candidate = false;
    }

    /// 取得組字緩衝區之視覺化預覽字串
    /// 例如輸入 `"2360"`、首選為 `"台"` 時，顯示 `"台 [2360:垂點方頭]"`；
    /// 輸入 `"-2332"`、首選為 `"台灣"` 時，顯示 `"台灣 [➖詞-2332:垂點點垂]"`
    pub fn get_preedit_display(&self) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }
        if self.buffer == "." {
            let cur_page = (self.candidate_idx / 10) + 1;
            let total_pages = ((self.candidates.len() + 9) / 10).max(1);
            return match cur_page {
                1 => format!(
                    "標點選單(第1/{}頁) [.1， .2。 .3、 .4？ .5！ · +下頁]",
                    total_pages
                ),
                2 => format!(
                    "標點選單(第2/{}頁) [1「 2」 3『』 4《》 5【】 · +下頁/-上頁]",
                    total_pages
                ),
                _ => format!(
                    "標點選單(第{}/{}頁) [1〈〉 2［］ 3｛｝ 4＋ 5－ · -上頁]",
                    cur_page, total_pages
                ),
            };
        }
        if self.buffer == "-" {
            if self.is_selecting_candidate {
                let top = self
                    .candidates
                    .get(self.candidate_idx)
                    .map(|s| s.as_str())
                    .unwrap_or("台灣");
                return format!(
                    "{} [➖詞·/選單#{}：按 1~0 快選 · +下頁/-上頁]",
                    top,
                    self.candidate_idx + 1
                );
            }
            return "➖詞 [請輸入4~6碼詞碼，或按 / 選下方常用詞]".to_string();
        }
        let is_word_mode = self.buffer.starts_with('-');
        let code_str = if is_word_mode {
            &self.buffer[1..]
        } else {
            &self.buffer[..]
        };
        let mode_tag = if is_word_mode { "➖詞" } else { "" };
        let stroke_labels: String = code_str.chars().map(digit_stroke_short_name).collect();
        if let Some(top) = self
            .candidates
            .get(self.candidate_idx)
            .or_else(|| self.candidates.first())
        {
            if self.is_selecting_candidate {
                format!(
                    "{} [{}{}·/選單#{}]",
                    top,
                    mode_tag,
                    self.buffer,
                    self.candidate_idx + 1
                )
            } else {
                format!("{} [{}{}:{}]", top, mode_tag, self.buffer, stroke_labels)
            }
        } else {
            format!("[{}{}:{}]", mode_tag, self.buffer, stroke_labels)
        }
    }

    /// 取得候選視窗底部之四角教練提示字串
    pub fn get_live_coach_hint(&self) -> String {
        if self.buffer == "." {
            let cur_page = (self.candidate_idx / 10) + 1;
            let total_pages = ((self.candidates.len() + 9) / 10).max(1);
            return match cur_page {
                1 => format!(
                    "🎯 標點選單（第 1/{} 頁）：按 1(，) 2(。) 3(、) 4(？) 5(！) 6(：) 7(；) 8(「」) 9(（）) 0(……) · 按 + 翻下頁 · 再按 . 直出逗號",
                    total_pages
                ),
                2 => format!(
                    "🎯 標點選單（第 2/{} 頁）：按 1(「) 2(」) 3(『』) 4(《》) 5(【】) 6(——) 7(～) 8(·) 9(（) 0(）) · 按 + 下頁 / - 上頁",
                    total_pages
                ),
                _ => format!(
                    "🎯 標點選單（第 {}/{} 頁）：按 1(〈〉) 2(［］) 3(｛｝) 4(＋) 5(－) 6(×) 7(÷) 8(＝) 9(％) 0(＠) · 按 - 翻上頁",
                    cur_page, total_pages
                ),
            };
        }
        if self.buffer == "-" {
            if self.is_selecting_candidate {
                let cur_page = (self.candidate_idx / 10) + 1;
                let total_pages = (self.candidates.len() + 9) / 10;
                return format!(
                    "🎯 常用詞選單（第 {}/{} 頁）：按 1~0 直接快選 · 按 + 翻下一頁 · 按 - 翻上一頁",
                    cur_page, total_pages
                );
            }
            return "🎯 詞彙模式（-）：直接輸入 4~6 碼詞碼，或按 / 進入選單以 1~0 快選下方常用詞".to_string();
        }
        let page_suffix = if self.candidates.len() > 10 {
            let cur_page = (self.candidate_idx / 10) + 1;
            let total_pages = (self.candidates.len() + 9) / 10;
            if self.is_selecting_candidate {
                format!(
                    "  [第 {}/{} 頁 · 1~0快選 · +下頁 / -上頁]",
                    cur_page, total_pages
                )
            } else {
                format!(
                    "  [第 {}/{} 頁 · 按 / 進入字詞選單(+下頁/-上頁)]",
                    cur_page, total_pages
                )
            }
        } else if self.is_selecting_candidate {
            "  [/字詞選單 · 按 1~0 快選]".to_string()
        } else {
            String::new()
        };
        if let Some(cand) = self
            .candidates
            .get(self.candidate_idx)
            .or_else(|| self.candidates.first())
        {
            if let Some(coach) = self.table.format_text_coach(cand) {
                return format!("🎯 四角教練：{} ➔ {}{}", cand, coach, page_suffix);
            }
        }
        if !self.buffer.is_empty() {
            let code_str = self.buffer.trim_start_matches('-');
            let stroke_labels: Vec<String> = code_str
                .chars()
                .map(|c| format!("{}{}", c, digit_stroke_short_name(c)))
                .collect();
            return format!("🎯 四角拆碼：{}{}", stroke_labels.join(" · "), page_suffix);
        }
        String::new()
    }

    /// 根據當前 `buffer` 與前文語境刷新 `candidates`
    pub fn refresh_candidates(
        &mut self,
        prev_context: Option<&str>,
        mut transitions: Option<&mut TransitionEngine>,
        lexicon: Option<&LexiconManager>,
    ) {
        self.candidates.clear();
        if self.buffer.is_empty() {
            self.candidate_idx = 0;
            return;
        }

        // 1. `.` 標點符號選單模式（支援完整 3 頁共 30 個常用標點與符號，以 `+`/`-` 翻頁、`1~0` 快選）
        if self.buffer == "." {
            self.candidates = FourCornerTable::default_punctuation_menu();
            self.candidate_idx = 0;
            return;
        }

        // 1b. `-` 詞彙前導模式（尚未輸入數字時提供 20 組常用多字詞即時預覽，亦支援按 `/` 後以 `1~0` 快選與 `+`/`-` 翻頁）
        if self.buffer == "-" {
            self.candidates = FourCornerTable::default_word_mode_menu();
            self.candidate_idx = 0;
            return;
        }

        let is_word_mode = self.buffer.starts_with('-');
        let code_str = if is_word_mode {
            self.buffer[1..].to_string()
        } else {
            self.buffer.clone()
        };

        let has_wildcard =
            code_str.contains('*') || code_str.contains('?') || code_str.contains('.');
        let mut raw_entries: Vec<FourCornerEntry>;

        if has_wildcard {
            raw_entries = self.table.lookup_wildcard(&code_str, 48);
            if is_word_mode {
                raw_entries.retain(|e| e.text.chars().count() >= 2);
            } else if raw_entries.iter().any(|e| e.text.chars().count() == 1) {
                raw_entries.retain(|e| e.text.chars().count() == 1);
            }
        } else {
            // 精確匹配
            raw_entries = self.table.lookup_exact(&code_str);

            // 若為 4~6 碼純數字，同步動態檢索 LexiconManager 中之使用者自建詞／領域詞：
            // - 4 碼：二字詞 (首1,2 + 尾1,2)
            // - 5 碼：三字詞 (首1,3 + 中1 + 尾2,4)
            // - 6 碼：四字詞 (首1,3 + 中一1 + 中二1 + 尾2,4)
            if (4..=6).contains(&code_str.len()) && code_str.chars().all(|c| c.is_ascii_digit()) {
                let target_wlen = if code_str.len() == 4 {
                    2
                } else {
                    code_str.len() - 2
                };
                if let Some(lex) = lexicon {
                    for entry in &lex.entries {
                        if (entry.category == "user" || entry.category == "domain")
                            && entry.weight >= 500
                            && entry.word.chars().count() == target_wlen
                            && self.table.word_matches_code(&entry.word, &code_str)
                        {
                            let (bonus, kind) = match target_wlen {
                                2 => (60, "phrase22"),
                                3 => (120, "phrase3"),
                                _ => (180, "phrase4"),
                            };
                            raw_entries.push(FourCornerEntry {
                                code: code_str.clone(),
                                text: entry.word.clone(),
                                weight: (entry.weight as i32) + bonus,
                                kind: kind.to_string(),
                            });
                        }
                    }
                }
            }

            // 單字模式 vs `-` 詞彙前導模式分流：
            // - `is_word_mode == true` (`-XXXX` / `-XXXXX` / `-XXXXXX`)：100% 僅保留多字詞 (`chars().count() >= 2`)，徹底排除單字干擾！
            // - `is_word_mode == false` (`XXXX` 無 `-` 前導)：只要存在單字匹配 (`chars().count() == 1`)，100% 僅保留單字，徹底解決二字詞與單字混雜搶位！
            let has_exact_single = raw_entries.iter().any(|e| e.text.chars().count() == 1);
            if is_word_mode {
                raw_entries.retain(|e| e.text.chars().count() >= 2);
            } else if has_exact_single {
                raw_entries.retain(|e| e.text.chars().count() == 1);
            }

            let min_exact_w = raw_entries.iter().map(|e| e.weight).min().unwrap_or(500);

            // 若輸入 1~3 碼，或 4 碼以上但精確匹配較少時補充前綴預覽（4碼以上時前綴預覽權重不得壓過精確匹配）
            if code_str.len() < 4 || raw_entries.len() < 6 {
                let prefix_hits = self.table.lookup_with_prefix(&code_str, 36);
                let has_any_single = has_exact_single
                    || prefix_hits.iter().any(|e| e.text.chars().count() == 1);
                for mut h in prefix_hits {
                    let word_len = h.text.chars().count();
                    if is_word_mode && word_len < 2 {
                        continue;
                    }
                    if !is_word_mode && has_any_single && word_len != 1 {
                        continue;
                    }
                    if !raw_entries.iter().any(|e| e.text == h.text) {
                        if code_str.len() >= 4 && h.code != code_str {
                            h.weight = h.weight.min(min_exact_w.saturating_sub(20)).max(60);
                        }
                        raw_entries.push(h);
                    }
                }
            }

            // 若為 4 碼以上且精確匹配不足（或供容錯救援），注入動態模糊四角候選（權重不得壓過精確匹配）
            if code_str.len() >= 4 && raw_entries.len() < 12 {
                let fuzzy_hits = self.table.lookup_fuzzy(&code_str, 20);
                for mut h in fuzzy_hits {
                    let word_len = h.text.chars().count();
                    if is_word_mode && word_len < 2 {
                        continue;
                    }
                    if !is_word_mode && has_exact_single && word_len != 1 {
                        continue;
                    }
                    if !raw_entries.iter().any(|e| e.text == h.text) {
                        if !raw_entries.is_empty() {
                            h.weight = h.weight.min(min_exact_w.saturating_sub(30)).max(50);
                        }
                        raw_entries.push(h);
                    }
                }
            }
        }

        // 2. 結合上下文語境轉移引擎 (TransitionEngine) 與詞庫凝聚力重新排序
        let ctx_str = prev_context
            .filter(|s| !s.is_empty())
            .unwrap_or(self.last_committed_word.as_str());

        let mut scored: Vec<(f64, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // 先去重保留最高基礎分
        let mut unique_entries: Vec<FourCornerEntry> = Vec::new();
        for entry in raw_entries {
            if let Some(ex) = unique_entries.iter_mut().find(|e| e.text == entry.text) {
                if entry.weight > ex.weight {
                    ex.weight = entry.weight;
                    ex.kind = entry.kind;
                }
            } else {
                unique_entries.push(entry);
            }
        }

        let prev_tail = ctx_str
            .chars()
            .last()
            .map(|c| c.to_string())
            .unwrap_or_default();

        for entry in unique_entries {
            let mut score = entry.weight as f64;

            // 上下文二元轉移與語法規則加成
            if !ctx_str.is_empty() {
                if let Some(trans) = transitions.as_deref_mut() {
                    let mut bonus = trans.calculate_transition_bonus(ctx_str, "", &entry.text, "");
                    if bonus == 0.0 && !prev_tail.is_empty() && prev_tail != ctx_str {
                        bonus = trans.calculate_transition_bonus(&prev_tail, "", &entry.text, "");
                    }
                    score += bonus * 3.2;
                }
                // 若前文最後一字與當前單字可組成高頻雙字詞，給予長詞凝聚加成
                if let Some(lex) = lexicon {
                    if let Some(last_ch) = ctx_str.chars().last() {
                        if entry.text.chars().count() == 1 {
                            let compound = format!("{}{}", last_ch, entry.text);
                            if let Some(ids) = lex.word_index.get(&compound) {
                                if let Some(&first_id) = ids.first() {
                                    if let Some(lex_e) = lex.entries.get(first_id) {
                                        if lex_e.weight >= 450 {
                                            score += ((lex_e.weight as f64) * 0.42).min(260.0);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            scored.push((score, entry.text));
        }

        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });

        // 3. 若輸入數字長度 == 8（如連續盲打兩組 4 碼長詞串或雙單字串），優先合成雙詞／雙字連打首選句；
        //    若輸入數字長度 == 6，優先呈現原生 6 碼四字成語 (`scored`)，若無四字成語則嘗試 4+2 連打合成
        if code_str.len() == 8 && !has_wildcard {
            for chained in self.solve_multi_word_chain(&code_str, is_word_mode) {
                if seen.insert(chained.clone()) {
                    self.candidates.push(chained);
                }
            }
        }

        for (_s, text) in scored {
            if seen.insert(text.clone()) {
                self.candidates.push(text);
                if self.candidates.len() >= 36 {
                    break;
                }
            }
        }

        if code_str.len() == 6 && !has_wildcard && self.candidates.is_empty() {
            for chained in self.solve_multi_word_chain(&code_str, is_word_mode) {
                if seen.insert(chained.clone()) {
                    self.candidates.push(chained);
                }
            }
        }

        if self.candidate_idx >= self.candidates.len() {
            self.candidate_idx = 0;
        }
    }

    /// 針對 6 碼或 8 碼以上連續輸入（如 `"23321072"` / `"-23321072"` ➔ `"台灣電腦"`）進行分段解碼合成
    fn solve_multi_word_chain(&self, code: &str, is_word_mode: bool) -> Vec<String> {
        let mut results = Vec::new();
        if code.len() == 8 {
            let left_all = self.table.lookup_exact(&code[0..4]);
            let right_all = self.table.lookup_exact(&code[4..8]);
            let left_word = left_all.iter().find(|e| e.text.chars().count() >= 2);
            let right_word = right_all.iter().find(|e| e.text.chars().count() >= 2);
            let left_char = left_all.iter().find(|e| e.text.chars().count() == 1);
            let right_char = right_all.iter().find(|e| e.text.chars().count() == 1);

            if is_word_mode {
                if let (Some(lw), Some(rw)) = (left_word, right_word) {
                    results.push(format!("{}{}", lw.text, rw.text));
                }
                if let (Some(l), Some(r)) = (left_all.first(), right_all.first()) {
                    let s = format!("{}{}", l.text, r.text);
                    if !results.contains(&s) {
                        results.push(s);
                    }
                }
            } else {
                let word_chain = match (left_word, right_word) {
                    (Some(lw), Some(rw)) => Some((lw.weight + rw.weight, format!("{}{}", lw.text, rw.text))),
                    _ => None,
                };
                let char_chain = match (left_char, right_char) {
                    (Some(lc), Some(rc)) => Some((lc.weight + rc.weight, format!("{}{}", lc.text, rc.text))),
                    _ => None,
                };
                // 若首段詞彙權重高於單字（如 2332 台灣 826 > 參 721），或僅有雙詞組合，雙詞連打排第一順位；同時保留雙單字組合供選取
                let word_first = match (left_word, left_char) {
                    (Some(lw), Some(lc)) => lw.weight >= lc.weight,
                    (Some(_), None) => true,
                    _ => false,
                };
                if word_first {
                    if let Some((_, wc)) = word_chain {
                        results.push(wc);
                    }
                    if let Some((_, cc)) = char_chain {
                        if !results.contains(&cc) {
                            results.push(cc);
                        }
                    }
                } else {
                    if let Some((_, cc)) = char_chain {
                        results.push(cc);
                    }
                    if let Some((_, wc)) = word_chain {
                        if !results.contains(&wc) {
                            results.push(wc);
                        }
                    }
                }
                if results.is_empty() {
                    if let (Some(l), Some(r)) = (left_all.first(), right_all.first()) {
                        results.push(format!("{}{}", l.text, r.text));
                    }
                }
            }
        } else if code.len() == 6 {
            // 先檢查是否有原生 6 碼四字成語
            if let Some(exact6) = self
                .table
                .lookup_exact(code)
                .into_iter()
                .find(|e| !is_word_mode || e.text.chars().count() >= 2)
            {
                results.push(exact6.text);
                return results;
            }
            // 否則嘗試 4 + 2（首詞 4 碼 + 次字前兩角首選）
            let left = self.table.lookup_exact(&code[0..4]);
            let right = self.table.lookup_with_prefix(&code[4..6], 4);
            let l_pick = if is_word_mode {
                left.iter().find(|e| e.text.chars().count() >= 2).or_else(|| left.first())
            } else {
                left.iter().find(|e| e.text.chars().count() == 1).or_else(|| left.first())
            };
            if let (Some(l), Some(r)) = (l_pick, right.first()) {
                results.push(format!("{}{}", l.text, r.text));
            }
        }
        results
    }

    /// 無外部語境之標準單鍵處理介面
    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        self.process_key_with_context(ch, None, None, None)
    }

    /// 含語境轉移引擎與詞庫之單鍵處理介面
    pub fn process_key_with_context(
        &mut self,
        ch: char,
        prev_context: Option<&str>,
        mut transitions: Option<&mut TransitionEngine>,
        lexicon: Option<&LexiconManager>,
    ) -> EngineResponse {
        let is_direct_punct = self.keymap_profile == "direct_punct";

        // 1. 退格鍵 BackSpace ('\x08')
        if ch == '\x08' {
            if self.is_selecting_candidate {
                self.is_selecting_candidate = false;
                self.candidate_idx = 0;
                return self.build_compose_response();
            }
            if !self.buffer.is_empty() {
                self.buffer.pop();
                self.candidate_idx = 0;
                self.refresh_candidates(prev_context, transitions.as_deref_mut(), lexicon);
                return self.build_compose_response();
            }
            return Self::none_response();
        }

        // 2. Escape ('\x1b')
        if ch == '\x1b' {
            if self.has_buffer() {
                self.reset();
                return Self::action_response("compose", String::new());
            }
            return Self::none_response();
        }

        // 3. `+` 與 `Enter` 分流：
        //    - 當處於 `/` 字詞選單模式 (`is_selecting_candidate == true`) 或 `.` 標點選單模式 (`buffer == "."`) 時，按 `+` 為「翻下一頁 (PageDown)」！
        //    - 一般組碼狀態下，按 `+` 或空白鍵 (' ') 為「字、詞分隔符號」，將當前首選字／詞收攏至如喜輸入座 (`action = "commit"`)
        //    - Enter ('\n' / '\r')：確認當前候選字／詞並將整句送出至目標視窗 (`action = "commit_enter"`)
        if ch == '\n' || ch == '\r' || ch == ' ' || ch == '+' {
            if ch == '+' && (self.is_selecting_candidate || self.buffer == ".") && !is_direct_punct
            {
                if !self.candidates.is_empty() {
                    let cur_page = self.candidate_idx / 10;
                    let total_pages = (self.candidates.len() + 9) / 10;
                    let next_page = (cur_page + 1) % total_pages.max(1);
                    self.candidate_idx = (next_page * 10).min(self.candidates.len() - 1);
                }
                return self.build_compose_response();
            }
            let action_name = if ch == '\n' || ch == '\r' {
                "commit_enter"
            } else {
                "commit"
            };
            if self.buffer == "-" && !self.is_selecting_candidate {
                self.reset();
                return Self::action_response("compose", String::new());
            }
            if self.has_buffer() {
                let commit_text = self
                    .candidates
                    .get(self.candidate_idx)
                    .cloned()
                    .unwrap_or_else(|| self.buffer.clone());
                self.last_committed_word = commit_text.clone();
                self.reset();
                return Self::action_response(action_name, commit_text);
            }
            return Self::none_response();
        }

        // 4. 小鍵盤除號 (`/`)：字詞選單（1~0 數字鍵快選，`+` 翻下一頁，`-` 翻上一頁）
        //    支援在任何有候選詞之狀態下（包含剛按下 `-` 出現常用詞預覽、或輸入 `-XXXX` / `XXXX` 時）按 `/` 進入選單！
        if ch == '/' {
            if is_direct_punct {
                // 直出標點模式：確認當前候選詞並連帶輸出全形逗號「，」
                let mut out = String::new();
                if self.has_buffer() && self.buffer != "-" && self.buffer != "." {
                    if let Some(top) = self.candidates.get(self.candidate_idx) {
                        out.push_str(top);
                        self.last_committed_word = top.clone();
                    }
                    self.reset();
                }
                out.push('，');
                return Self::action_response("commit_punctuation", out);
            } else {
                // 全盲打閉環模式（預設）：按 `/` 進入字詞選單模式，數字鍵 1~0 快選，`+` 下頁，`-` 上頁
                if self.has_buffer() {
                    if self.buffer == "." {
                        self.is_selecting_candidate = true;
                        return self.build_compose_response();
                    }
                    if !self.candidates.is_empty() {
                        if !self.is_selecting_candidate {
                            self.is_selecting_candidate = true;
                            self.candidate_idx = if self.candidates.len() > 1 { 1 } else { 0 };
                        } else {
                            self.candidate_idx = (self.candidate_idx + 1) % self.candidates.len();
                        }
                    }
                    return self.build_compose_response();
                }
                return Self::none_response();
            }
        }

        // 5. 小鍵盤小數點 (`.`)：標點符號選單前導（`1~0` 快選，`+` 翻下頁，`-` 翻上頁，連按 `..` 直出逗號）
        if ch == '.' {
            if is_direct_punct {
                // 在直出標點模式下，滿 4 碼後按 `.` 可觸發 1~9 選字鎖定
                let cur_code_len = self.buffer.trim_start_matches('-').len();
                if cur_code_len >= 4 {
                    if !self.candidates.is_empty() {
                        self.is_selecting_candidate = !self.is_selecting_candidate;
                        if self.is_selecting_candidate && self.candidates.len() > 1 {
                            self.candidate_idx = 1;
                        }
                    }
                    return self.build_compose_response();
                }
            }
            // - 若已處於 `.` 標點選單模式，再按一次 `.` (`..`) ➔ 直接上屏最常用之全形逗號「，」（或當前頁首選標點）
            if self.buffer == "." {
                let sym = self
                    .candidates
                    .get(self.candidate_idx)
                    .cloned()
                    .unwrap_or_else(|| "，".to_string());
                self.reset();
                return Self::action_response("commit_punctuation", sym);
            }
            // - 若已有字／詞緩衝區，先自動結算當前首選字／詞收攏至輸入座，並立即展開 `.` 標點符號選單！
            let mut prev_committed = String::new();
            if self.has_buffer() {
                if self.buffer != "-" {
                    if let Some(top) = self.candidates.get(self.candidate_idx) {
                        prev_committed = top.clone();
                        self.last_committed_word = top.clone();
                    }
                }
                self.reset();
            }
            self.buffer.push('.');
            self.refresh_candidates(prev_context, transitions.as_deref_mut(), lexicon);
            let mut resp = self.build_compose_response();
            resp.committed_chunk = prev_committed;
            return resp;
        }

        // 6. 小鍵盤減號 (`-`)：
        //    - 在 `/` 字詞選單或 `.` 標點選單中 (`is_selecting_candidate || buffer == "."`)：作為「翻上一頁 (PageUp)」！
        //    - 在一般狀態下：作為「詞」前導符號（進入 `➖詞` 詞彙模式，專打二字詞/三字詞/四字詞）！
        if ch == '-' {
            if is_direct_punct {
                // 直出標點模式：確認當前候選詞並連帶輸出全形問號「？」
                let mut out = String::new();
                if self.has_buffer() && self.buffer != "-" && self.buffer != "." {
                    if let Some(top) = self.candidates.get(self.candidate_idx) {
                        out.push_str(top);
                        self.last_committed_word = top.clone();
                    }
                    self.reset();
                }
                out.push('？');
                return Self::action_response("commit_punctuation", out);
            } else {
                // (a) 若處於 `/` 字詞選單或 `.` 標點選單模式：`-` 翻上一頁 (PageUp)
                if self.is_selecting_candidate || self.buffer == "." {
                    if !self.candidates.is_empty() {
                        let cur_page = self.candidate_idx / 10;
                        let total_pages = (self.candidates.len() + 9) / 10;
                        let prev_page = if cur_page > 0 {
                            cur_page - 1
                        } else {
                            total_pages.saturating_sub(1)
                        };
                        self.candidate_idx = (prev_page * 10).min(self.candidates.len() - 1);
                    }
                    return self.build_compose_response();
                }

                // (b) 若緩衝區為空：啟動 `-` 詞彙前導模式
                if self.buffer.is_empty() {
                    self.buffer.push('-');
                    self.candidate_idx = 0;
                    self.refresh_candidates(prev_context, transitions.as_deref_mut(), lexicon);
                    return self.build_compose_response();
                }

                // (c) 若剛按過 `-` 尚未輸入數字，再按一次 `-` 取消詞彙前導模式回到空白
                if self.buffer == "-" {
                    self.reset();
                    return Self::action_response("compose", String::new());
                }

                // (d) 若剛打到 1~3 碼單字才想到要切換為詞彙模式（或取消詞彙前綴），直接切換 `-` 前導
                let code_len = self.buffer.trim_start_matches('-').len();
                if code_len < 4 && self.buffer != "." {
                    if self.buffer.starts_with('-') {
                        self.buffer = self.buffer[1..].to_string();
                    } else {
                        self.buffer.insert(0, '-');
                    }
                    self.candidate_idx = 0;
                    self.refresh_candidates(prev_context, transitions.as_deref_mut(), lexicon);
                    return self.build_compose_response();
                }

                // (e) 若已輸入滿 4~6 碼字／詞，按 `-` 直接收攏當前字／詞並無縫開啟下一個 `-` 詞彙前導模式！
                let mut prev_committed = String::new();
                if let Some(top) = self.candidates.get(self.candidate_idx) {
                    prev_committed = top.clone();
                    self.last_committed_word = top.clone();
                }
                self.reset();
                self.buffer.push('-');
                let eff_ctx = if !prev_committed.is_empty() {
                    Some(prev_committed.as_str())
                } else {
                    prev_context
                };
                self.refresh_candidates(eff_ctx, transitions.as_deref_mut(), lexicon);
                let mut resp = self.build_compose_response();
                resp.committed_chunk = prev_committed;
                return resp;
            }
        }

        // 7. 小鍵盤乘號 (`*`) 或問號 (`?`)：萬用字元查詢（代表任意 0~9 筆形角碼）
        if ch == '*' || ch == '?' {
            if is_direct_punct && ch == '*' {
                // 直出標點模式：確認當前候選詞並連帶輸出全形句號「。」
                let mut out = String::new();
                if self.has_buffer() && self.buffer != "-" && self.buffer != "." {
                    if let Some(top) = self.candidates.get(self.candidate_idx) {
                        out.push_str(top);
                        self.last_committed_word = top.clone();
                    }
                    self.reset();
                }
                out.push('。');
                return Self::action_response("commit_punctuation", out);
            }
            if self.buffer == "." {
                self.reset();
            }
            let cur_code_len = self.buffer.trim_start_matches('-').len();
            if cur_code_len < 8 {
                self.is_selecting_candidate = false;
                self.buffer.push('*');
                self.candidate_idx = 0;
                self.refresh_candidates(prev_context, transitions.as_deref_mut(), lexicon);
                return self.build_compose_response();
            }
        }

        // 8. 數字鍵 `'0'`..=`'9'`
        if ch.is_ascii_digit() {
            // (a) 若處於 `.` 標點符號選單模式：`.1`~`.0` 立即上屏當前頁對應全形標點
            if self.buffer == "." {
                let page_base = (self.candidate_idx / 10) * 10;
                let offset = if ch == '0' {
                    9
                } else {
                    (ch as usize).saturating_sub('1' as usize)
                };
                let target = page_base + offset;
                let sym = self
                    .candidates
                    .get(target)
                    .cloned()
                    .or_else(|| {
                        let code = format!(".{}", ch);
                        self.table
                            .lookup_exact(&code)
                            .first()
                            .map(|e| e.text.clone())
                    })
                    .unwrap_or_else(|| ch.to_string());
                self.reset();
                return Self::action_response("commit_punctuation", sym);
            }

            // (b) 若處於 `/` 或 `PgDn/PgUp` 觸發之字詞選單模式 (`is_selecting_candidate`)：按 `1`~`9` 直接選定該頁對應候選詞上屏，`0` 選第 10 項
            if self.is_selecting_candidate {
                let page_base = (self.candidate_idx / 10) * 10;
                let offset = if ch == '0' {
                    9
                } else {
                    (ch as usize).saturating_sub('1' as usize)
                };
                let target = page_base + offset;
                if let Some(chosen) = self.candidates.get(target).cloned() {
                    self.last_committed_word = chosen.clone();
                    self.reset();
                    return Self::action_response("commit", chosen);
                }
            }

            // (c) 一般四角號碼累積輸入
            // 若數字碼已達 8 碼，自動先結算當前首選詞上屏，再將新數字納入新緩衝區
            let mut auto_committed = String::new();
            let cur_code_len = self.buffer.trim_start_matches('-').len();
            if cur_code_len >= 8 {
                let was_word_mode = self.buffer.starts_with('-');
                if let Some(top) = self.candidates.get(self.candidate_idx).cloned() {
                    auto_committed = top.clone();
                    self.last_committed_word = top;
                    self.buffer.clear();
                    if was_word_mode {
                        self.buffer.push('-');
                    }
                } else {
                    let raw_digits = self.buffer.trim_start_matches('-').to_string();
                    let left4 = raw_digits[0..4].to_string();
                    if let Some(top4) = self.table.lookup_exact(&left4).first() {
                        auto_committed = top4.text.clone();
                        self.last_committed_word = top4.text.clone();
                    }
                    self.buffer = if was_word_mode {
                        format!("-{}", &raw_digits[4..])
                    } else {
                        raw_digits[4..].to_string()
                    };
                }
            }

            self.buffer.push(ch);
            self.candidate_idx = 0;
            let eff_ctx = if !auto_committed.is_empty() {
                Some(auto_committed.as_str())
            } else {
                prev_context
            };
            self.refresh_candidates(eff_ctx, transitions.as_deref_mut(), lexicon);
            let mut resp = self.build_compose_response();
            resp.committed_chunk = auto_committed;
            return resp;
        }

        Self::none_response()
    }

    /// 字串型按鍵事件處理（支援 `"Down"`, `"Up"`, `"PageDown"`, `"PageUp"`, `"Shift+BackSpace"`, `"Ctrl+BackSpace"`）
    pub fn process_key_str(&mut self, key_name: &str) -> EngineResponse {
        self.process_key_str_with_context(key_name, None, None, None)
    }

    pub fn process_key_str_with_context(
        &mut self,
        key_name: &str,
        prev_context: Option<&str>,
        transitions: Option<&mut TransitionEngine>,
        lexicon: Option<&LexiconManager>,
    ) -> EngineResponse {
        match key_name {
            "BackSpace" => self.process_key_with_context('\x08', prev_context, transitions, lexicon),
            "Shift+BackSpace" | "Ctrl+BackSpace" => {
                if self.has_buffer() {
                    self.reset();
                    Self::action_response("compose", String::new())
                } else {
                    Self::none_response()
                }
            }
            "Escape" => self.process_key_with_context('\x1b', prev_context, transitions, lexicon),
            "Return" | "Enter" | "KP_Enter" => {
                self.process_key_with_context('\n', prev_context, transitions, lexicon)
            }
            "space" | "Space" => {
                self.process_key_with_context(' ', prev_context, transitions, lexicon)
            }
            "KP_Add" | "plus" => {
                self.process_key_with_context('+', prev_context, transitions, lexicon)
            }
            "KP_Multiply" | "asterisk" => {
                self.process_key_with_context('*', prev_context, transitions, lexicon)
            }
            "KP_Divide" | "slash" => {
                self.process_key_with_context('/', prev_context, transitions, lexicon)
            }
            "KP_Subtract" | "minus" => {
                self.process_key_with_context('-', prev_context, transitions, lexicon)
            }
            "KP_Decimal" | "period" => {
                self.process_key_with_context('.', prev_context, transitions, lexicon)
            }
            "Down" => {
                if self.has_buffer() && !self.candidates.is_empty() {
                    self.is_selecting_candidate = true;
                    self.candidate_idx = (self.candidate_idx + 1) % self.candidates.len();
                    self.build_compose_response()
                } else {
                    Self::none_response()
                }
            }
            "Up" => {
                if self.has_buffer() && !self.candidates.is_empty() {
                    self.is_selecting_candidate = true;
                    if self.candidate_idx == 0 {
                        self.candidate_idx = self.candidates.len() - 1;
                    } else {
                        self.candidate_idx -= 1;
                    }
                    self.build_compose_response()
                } else {
                    Self::none_response()
                }
            }
            "Next" | "PageDown" => {
                if self.has_buffer() && !self.candidates.is_empty() {
                    self.is_selecting_candidate = true;
                    let cur_page = self.candidate_idx / 10;
                    let total_pages = (self.candidates.len() + 9) / 10;
                    let next_page = (cur_page + 1) % total_pages.max(1);
                    self.candidate_idx = (next_page * 10).min(self.candidates.len() - 1);
                    self.build_compose_response()
                } else {
                    Self::none_response()
                }
            }
            "Prior" | "PageUp" => {
                if self.has_buffer() && !self.candidates.is_empty() {
                    self.is_selecting_candidate = true;
                    let cur_page = self.candidate_idx / 10;
                    let total_pages = (self.candidates.len() + 9) / 10;
                    let prev_page = if cur_page > 0 {
                        cur_page - 1
                    } else {
                        total_pages.saturating_sub(1)
                    };
                    self.candidate_idx = (prev_page * 10).min(self.candidates.len() - 1);
                    self.build_compose_response()
                } else {
                    Self::none_response()
                }
            }
            _ => {
                if key_name.chars().count() == 1 {
                    let ch = key_name.chars().next().unwrap();
                    self.process_key_with_context(ch, prev_context, transitions, lexicon)
                } else {
                    Self::none_response()
                }
            }
        }
    }

    fn build_compose_response(&self) -> EngineResponse {
        let preedit = self.get_preedit_display();
        let active = self
            .candidates
            .get(self.candidate_idx)
            .cloned()
            .unwrap_or_else(|| preedit.clone());
        let cursor_pos = preedit.chars().count();
        EngineResponse {
            action: "compose".to_string(),
            mode: if self.is_selecting_candidate {
                "candidates".to_string()
            } else {
                "four_corner".to_string()
            },
            preedit: preedit.clone(),
            composed: preedit,
            candidates: self.candidates.clone(),
            active_candidate: active,
            candidate_idx: self.candidate_idx,
            cursor_pos,
            q_press_count: 0,
            committed_chunk: String::new(),
        }
    }

    fn action_response(action: &str, committed_chunk: String) -> EngineResponse {
        EngineResponse {
            action: action.to_string(),
            mode: "four_corner".to_string(),
            preedit: String::new(),
            composed: String::new(),
            candidates: Vec::new(),
            active_candidate: String::new(),
            candidate_idx: 0,
            cursor_pos: 0,
            q_press_count: 0,
            committed_chunk,
        }
    }

    fn none_response() -> EngineResponse {
        Self::action_response("none", String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_four_corner_single_char_and_variant_sharing() {
        let mut eng = FourCornerEngine::new();
        // 輸入 "2360" 應同時命中「台」(首選) 與異體共享「臺」
        for ch in "2360".chars() {
            eng.process_key(ch);
        }
        assert!(eng.has_buffer());
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("台"));
        assert!(eng.candidates.contains(&"臺".to_string()));

        // 按下小鍵盤 '+' 確認上屏
        let resp = eng.process_key('+');
        assert_eq!(resp.action, "commit");
        assert_eq!(resp.committed_chunk, "台");
        assert!(!eng.has_buffer());

        // 輸入正體「臺」之四角碼 "4010" 亦能雙向命中「臺」與「台」
        for ch in "4010".chars() {
            eng.process_key(ch);
        }
        assert!(eng.candidates.contains(&"臺".to_string()));
        assert!(eng.candidates.contains(&"台".to_string()));
    }

    #[test]
    fn test_four_corner_beginner_zero_tolerance_and_fuzzy() {
        let mut eng = FourCornerEngine::new();
        // 初學者不背「筆畫已用過補 0」規則，直接將「台」下方口字拆為雙 6 ("2366")
        for ch in "2366".chars() {
            eng.process_key(ch);
        }
        assert!(
            eng.candidates.contains(&"台".to_string()),
            "初學者免補0碼 2366 應命中「台」，實際候選: {:?}",
            eng.candidates
        );
        eng.reset();

        // 初學者將「如」(4640) 右下口拆為 6 ("4646")
        for ch in "4646".chars() {
            eng.process_key(ch);
        }
        assert!(
            eng.candidates.contains(&"如".to_string()),
            "初學者免補0碼 4646 應命中「如」，實際候選: {:?}",
            eng.candidates
        );
    }

    #[test]
    fn test_four_corner_2corner_phrase_shortcode() {
        let mut eng = FourCornerEngine::new();
        // 雙字詞「前兩角簡碼」：台(23) + 灣(32) = 2332 -> 台灣
        for ch in "2332".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("台灣"));

        let resp = eng.process_key('+');
        assert_eq!(resp.committed_chunk, "台灣");

        // 電(10) + 腦(72) = 1072 -> 電腦
        for ch in "1072".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("電腦"));
    }

    #[test]
    fn test_four_corner_numpad_blind_closed_loop_keys() {
        let mut eng = FourCornerEngine::new();
        // 1. 測試 `/` 鍵進入字詞選單並進入 1~0 數字快選模式
        for ch in "2360".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidate_idx, 0);
        let resp_slash = eng.process_key('/');
        assert_eq!(resp_slash.mode, "candidates");
        assert!(eng.is_selecting_candidate);
        assert_eq!(eng.candidate_idx, 1);

        // 在 `/` 字詞選單模式下按 '2' 直接選取第 2 候選字上屏
        let second_cand = eng.candidates[1].clone();
        let resp_pick = eng.process_key('2');
        assert_eq!(resp_pick.action, "commit");
        assert_eq!(resp_pick.committed_chunk, second_cand);

        // 2. 測試 `-` 鍵作為「詞」前導符號：
        //    (a) 剛按下 `-` 顯示常用詞時，直接按 `/` 進入選單，再按 `2` 即可快選第 2 項「輸入法」（支援 `+` 下頁 / `-` 上頁）
        eng.process_key('-');
        assert_eq!(eng.buffer, "-");
        let resp_minus_slash = eng.process_key('/');
        assert!(eng.is_selecting_candidate);
        assert!(!resp_minus_slash.composed.is_empty());
        eng.process_key('+');
        assert_eq!(eng.candidate_idx, 10);
        eng.process_key('-');
        assert_eq!(eng.candidate_idx, 0);
        let resp_minus_pick = eng.process_key('2');
        assert_eq!(resp_minus_pick.action, "commit");
        assert_eq!(resp_minus_pick.committed_chunk, "輸入法");

        //    (b) 按 `-` 後輸入 4 碼 `-4640` 命中「如喜」，以及獨立九宮格 BackSpace ('\x08') 退格
        eng.process_key('-');
        assert_eq!(eng.buffer, "-");
        for ch in "4640".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.buffer, "-4640");
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("如喜"));
        eng.process_key('\x08');
        assert_eq!(eng.buffer, "-464");

        // 3. 測試 `.` 標點符號選單前導與 `..` 雙擊逗號、`. +` 翻第 2 頁標點
        eng.reset();
        eng.process_key('.');
        let resp_comma = eng.process_key('.');
        assert_eq!(resp_comma.action, "commit_punctuation");
        assert_eq!(resp_comma.committed_chunk, "，");

        eng.process_key('.');
        let resp_period = eng.process_key('2');
        assert_eq!(resp_period.action, "commit_punctuation");
        assert_eq!(resp_period.committed_chunk, "。");

        // 標點選單翻頁測試：`.` -> `+` (翻至第 2 頁) -> `4` (《》)
        eng.process_key('.');
        assert_eq!(eng.candidates.len(), 30);
        eng.process_key('+');
        assert_eq!(eng.candidate_idx, 10);
        let resp_book = eng.process_key('4');
        assert_eq!(resp_book.action, "commit_punctuation");
        assert_eq!(resp_book.committed_chunk, "《》");

        // 標點選單第 3 頁測試：`.` -> `+` -> `+` (翻至第 3 頁) -> `1` (〈〉) 與 `0` (＠)
        eng.process_key('.');
        eng.process_key('+');
        eng.process_key('+');
        assert_eq!(eng.candidate_idx, 20);
        let resp_angle = eng.process_key('1');
        assert_eq!(resp_angle.action, "commit_punctuation");
        assert_eq!(resp_angle.committed_chunk, "〈〉");

        // 4. 測試 `*` 萬用字元查詢 ("23*0" -> 命中 "台" 2360)
        for ch in "23*0".chars() {
            eng.process_key(ch);
        }
        assert!(eng.candidates.contains(&"台".to_string()));
    }

    #[test]
    fn test_four_corner_direct_punct_profile() {
        let mut eng = FourCornerEngine::new();
        eng.keymap_profile = "direct_punct".to_string();

        for ch in "2332".chars() {
            eng.process_key(ch);
        }
        // 在 direct_punct 模式下按 `/` 應直接上屏「台灣，」
        let resp = eng.process_key('/');
        assert_eq!(resp.action, "commit_punctuation");
        assert_eq!(resp.committed_chunk, "台灣，");
    }

    #[test]
    fn test_four_corner_multi_word_chain() {
        let mut eng = FourCornerEngine::new();
        let tsv_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ruhi_lexicon/data/four_corner.tsv");
        if tsv_path.exists() {
            eng.load_tsv(&tsv_path).expect("載入 four_corner.tsv 失敗");
        }
        // 連續輸入 8 碼：-23321072 與 23321072 (台灣 2332 + 電腦 1072 = 台灣電腦)
        for ch in "-23321072".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("台灣電腦"));
        eng.reset();

        for ch in "23321072".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("台灣電腦"));

        // 驗證 lookup_for_reselect 對 `-` 詞前導與單字之精確分流
        let reselect_word = eng.table.lookup_for_reselect("-2332");
        assert!(!reselect_word.is_empty());
        assert_eq!(reselect_word.first().map(|s| s.as_str()), Some("台灣"));
        assert!(reselect_word.iter().all(|w| w.chars().count() >= 2));

        let reselect_char = eng.table.lookup_for_reselect("1040");
        assert!(!reselect_char.is_empty());
        assert!(reselect_char.iter().all(|w| w.chars().count() == 1));
    }

    #[test]
    fn test_four_corner_word_prefix_minus_and_star_plus_minus_pagination() {
        let mut eng = FourCornerEngine::new();
        let tsv_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ruhi_lexicon/data/four_corner.tsv");
        if tsv_path.exists() {
            eng.load_tsv(&tsv_path).expect("載入 four_corner.tsv 失敗");
        }

        // 1. 單字與二字詞徹底分流：
        //    - 無 `-` 前導輸入 "1040"：100% 單字優先（#1 要、#2 干、于、耳、平，不含任何二字詞）
        for ch in "1040".chars() {
            eng.process_key(ch);
        }
        assert!(
            eng.candidates.iter().all(|c| c.chars().count() == 1),
            "無 `-` 前導之 1040 應為純單字候選，實際: {:?}",
            eng.candidates
        );
        assert!(eng.candidates.iter().take(5).any(|s| s == "要"));
        assert!(eng.candidates.iter().take(5).any(|s| s == "干"));
        eng.reset();

        //    - 帶 `-` 詞前導輸入 "-1040"：100% 多字詞，「天才」直接進駐第 1 頁（前 10 名內）！
        for ch in "-1040".chars() {
            eng.process_key(ch);
        }
        assert!(
            eng.candidates.iter().all(|c| c.chars().count() >= 2),
            "帶 `-` 前導之 -1040 應為純多字詞候選，實際: {:?}",
            eng.candidates
        );
        let tc_pos = eng
            .candidates
            .iter()
            .position(|c| c == "天才")
            .expect("-1040 應包含二字詞「天才」");
        assert!(
            tc_pos < 10,
            "「天才」於 -1040 應位於第 1 頁（前 10 名），實際位置: #{}",
            tc_pos + 1
        );
        eng.reset();

        // 2. 帶 `-` 輸入 "-0044"（主 00 + 權 44），「主權」位於第 1 頁
        for ch in "-0044".chars() {
            eng.process_key(ch);
        }
        let zq_pos = eng
            .candidates
            .iter()
            .position(|c| c == "主權")
            .expect("-0044 應包含雙字詞「主權」");
        assert!(
            zq_pos < 10,
            "「主權」應位於 -0044 第 1 頁（前 10 名），實際位置: #{}",
            zq_pos + 1
        );

        // 3. 測試 `/` 進入字詞選單，數字鍵 `1~0` 快選，`+` 翻下一頁，`-` 翻上一頁
        eng.process_key('/');
        assert!(eng.is_selecting_candidate);
        // 在 `/` 選單中按 `+` 翻下一頁 (index 10)
        eng.process_key('+');
        assert_eq!(eng.candidate_idx, 10);
        // 在 `/` 選單中按 `-` 翻回上一頁 (index 0)
        eng.process_key('-');
        assert_eq!(eng.candidate_idx, 0);

        // 在第 1 頁按對應數字鍵直接快選「主權」
        let digit_key = if zq_pos == 9 {
            '0'
        } else {
            char::from_digit((zq_pos + 1) as u32, 10).unwrap()
        };
        let resp = eng.process_key(digit_key);
        assert_eq!(resp.action, "commit");
        assert_eq!(resp.committed_chunk, "主權");
    }

    #[test]
    fn test_four_corner_hybrid_3char_5digit_and_4char_6digit_and_plus_enter_split() {
        let mut eng = FourCornerEngine::new();
        let tsv_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ruhi_lexicon/data/four_corner.tsv");
        if tsv_path.exists() {
            eng.load_tsv(&tsv_path).expect("載入 four_corner.tsv 失敗");
        }

        // 1. 三字詞 `-50843` (首字1,3 + 中字1 + 尾字2,4)：輸(50) + 入(8) + 法(43) = -50843 -> 輸入法
        for ch in "-50843".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("輸入法"));

        // 按下 '+' 作為字／詞分隔符號收攏至輸入座，action 為 "commit"
        let resp_plus = eng.process_key('+');
        assert_eq!(resp_plus.action, "commit");
        assert_eq!(resp_plus.committed_chunk, "輸入法");

        // 當緩衝區已空時，重複按下 '+' 應為無副作用之分界空操作 ("none")，絕不吐出 '+' 號污染句子
        let resp_plus_empty = eng.process_key('+');
        assert_eq!(resp_plus_empty.action, "none");
        assert!(resp_plus_empty.committed_chunk.is_empty());

        // 2. 四字詞 `-622612` (首字1,3 + 中一1 + 中二1 + 尾字2,4)：四(62) + 角(2) + 號(6) + 碼(12) = -622612 -> 四角號碼
        for ch in "-622612".chars() {
            eng.process_key(ch);
        }
        assert_eq!(eng.candidates.first().map(|s| s.as_str()), Some("四角號碼"));

        // 按下 Enter ('\n') 整句送出至目標視窗，action 為 "commit_enter"
        let resp_enter = eng.process_key('\n');
        assert_eq!(resp_enter.action, "commit_enter");
        assert_eq!(resp_enter.committed_chunk, "四角號碼");

        // 3. 驗證四角教練對二字詞、三字詞與四字詞之 `-` 前導與分層取碼解說
        let coach2 = eng.table.format_text_coach("台灣").unwrap_or_default();
        assert!(coach2.contains("-2332") && coach2.contains("-首1,2+尾1,2"));
        let coach3 = eng.table.format_text_coach("輸入法").unwrap_or_default();
        assert!(coach3.contains("-50843") && coach3.contains("-首1,3+中1+尾2,4"));
        let coach4 = eng.table.format_text_coach("四角號碼").unwrap_or_default();
        assert!(coach4.contains("-622612") && coach4.contains("-首1,3+中1×2+尾2,4"));
    }
}
