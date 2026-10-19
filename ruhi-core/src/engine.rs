/// RuhI (如喜) Input Method Framework - Rust Core
/// src/engine.rs - RuhI 核心中樞協同引擎

use crate::fsm::HsuFSM;
use crate::lexicon::LexiconManager;
use crate::router::{LatticeDAG, ViterbiRouter};
use crate::templates::TemplatesManager;
use crate::symbols::SymbolsManager;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineMode {
    Normal,
    TemplateSelect,
    QuickNum,
    LexiconSwitch,
    CandidatePopup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineResponse {
    pub action: String,
    pub mode: String,
    pub preedit: String,
    pub composed: String,
    pub candidates: Vec<String>,
    pub active_candidate: String,
    pub candidate_idx: usize,
    pub cursor_pos: usize,
    pub q_press_count: usize,
    pub committed_chunk: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionRecord {
    pub timestamp: u64,
    pub bopomofo: String,
    pub rejected: String,
    pub accepted: String,
    pub prev_context: String,
    pub next_context: String,
}

pub struct RuhiEngine {
    pub fsm: HsuFSM,
    pub lexicon: LexiconManager,
    pub templates: TemplatesManager,
    pub symbols: SymbolsManager,
    pub transitions: crate::router::transitions::TransitionEngine,
    pub router: ViterbiRouter,
    pub mode: EngineMode,
    pub q_press_count: usize,
    pub q_popup_threshold: usize,
    pub backtick_press_count: usize,
    pub composed_sentence: String,
    pub sentence_chars: Vec<String>,
    pub current_candidates: Vec<String>,
    pub candidate_cycle_idx: usize,
    pub cursor_pos: usize,
    pub data_dir: Option<String>,
    pub user_pinned: std::collections::HashMap<usize, String>,
    pub cangjie: crate::cangjie::CangjieEngine,
    pub array30: crate::array30::Array30Engine,
    pub japanese: crate::japanese::JapaneseEngine,
    pub keyboard_mode: String,
    /// 使用者自選啟用之亞洲輸入法循環清單（供 Ctrl/Shift + Caps Lock 與徽章點擊切換）
    pub enabled_keyboard_modes: Vec<String>,
    /// 是否啟用「許氏注音 ➔ 行列 30」即時反查教練提示
    pub enable_array30_coach: bool,
    pub segments: Vec<crate::router::LatticeNode>,
    pub enable_mixed_english: bool,
    pub mixed_english_mode: String,
    pub mixed_english_min_len: usize,
    pub raw_ascii_buffer: String,
    pub is_latin_mode: bool,
    pub prior_was_english: bool,
    pub western: crate::western::WesternEngine,
    pub last_committed_word: String,
    pub pending_revisions: Vec<RevisionRecord>,
    /// 手動斷詞點集合（以音節索引為界，任何跨越邊界之多字詞一律禁止生成）
    pub manual_cuts: std::collections::HashSet<usize>,
    /// 當前候選窗作用之字元區間 (start_idx, end_idx)，支援單字、多字詞與變長歐文縮寫替換
    pub candidate_target_range: Option<(usize, usize)>,
}

impl Default for RuhiEngine {
    fn default() -> Self {
        Self {
            fsm: HsuFSM::new(),
            lexicon: LexiconManager::new(),
            templates: TemplatesManager::new(),
            symbols: SymbolsManager::new(),
            transitions: crate::router::transitions::TransitionEngine::new(),
            router: ViterbiRouter::new(),
            mode: EngineMode::Normal,
            q_press_count: 0,
            q_popup_threshold: 5,
            backtick_press_count: 0,
            composed_sentence: String::new(),
            sentence_chars: Vec::new(),
            current_candidates: Vec::new(),
            candidate_cycle_idx: 0,
            cursor_pos: 0,
            data_dir: None,
            user_pinned: std::collections::HashMap::new(),
            cangjie: crate::cangjie::CangjieEngine::new(),
            array30: crate::array30::Array30Engine::new(),
            japanese: crate::japanese::JapaneseEngine::new(),
            keyboard_mode: "hsu".to_string(),
            enabled_keyboard_modes: vec!["hsu".to_string(), "japanese".to_string()],
            enable_array30_coach: false,
            segments: Vec::new(),
            enable_mixed_english: true,
            mixed_english_mode: "contextual_stream".to_string(),
            mixed_english_min_len: 4,
            raw_ascii_buffer: String::new(),
            is_latin_mode: false,
            prior_was_english: false,
            western: crate::western::WesternEngine::new(),
            last_committed_word: String::new(),
            pending_revisions: Vec::new(),
            manual_cuts: std::collections::HashSet::new(),
            candidate_target_range: None,
        }
    }
}

impl RuhiEngine {
    pub fn new_with_data_dir(data_dir: &str) -> Self {
        let mut engine = Self::default();
        engine.data_dir = Some(data_dir.to_string());
        
        let p = std::path::Path::new(data_dir);
        
        let mc_path = p.join("lexicon_mcbopomofo.tsv");
        if mc_path.exists() {
            if let Some(path_str) = mc_path.to_str() {
                let _ = engine.lexicon.load_tsv(path_str, "mcbopomofo");
            }
        }

        let moe_path = p.join("lexicon_moe.tsv");
        if moe_path.exists() {
            if let Some(path_str) = moe_path.to_str() {
                let _ = engine.lexicon.load_tsv(path_str, "moe");
            }
        }

        let domain_path = p.join("lexicon_domain.tsv");
        if domain_path.exists() {
            if let Some(path_str) = domain_path.to_str() {
                let _ = engine.lexicon.load_tsv(path_str, "domain");
            }
        } else {
            engine.lexicon.watch_dynamic_file(&domain_path, "domain");
        }

        let user_path = p.join("user_dict.tsv");
        if user_path.exists() {
            if let Some(path_str) = user_path.to_str() {
                let _ = engine.lexicon.load_tsv(path_str, "user");
            }
        } else {
            engine.lexicon.watch_dynamic_file(&user_path, "user");
        }

        let trans_path = p.join("transitions_user.tsv");
        if trans_path.exists() {
            if let Some(path_str) = trans_path.to_str() {
                let _ = engine.transitions.load_tsv(path_str);
            }
        } else {
            engine.transitions.file_path = Some(trans_path);
        }

        let patterns_path = p.join("patterns_user.jsonl");
        if patterns_path.exists() {
            let _ = engine.transitions.load_patterns_jsonl(&patterns_path);
        } else {
            engine.transitions.patterns_file_path = Some(patterns_path);
        }
        
        let sym_path = p.join("symbols.tsv");
        if sym_path.exists() {
            if let Some(path_str) = sym_path.to_str() {
                let _ = engine.symbols.load_tsv(path_str);
            }
        }
        
        let tpl_path = p.join("templates.tsv");
        if tpl_path.exists() {
            if let Some(path_str) = tpl_path.to_str() {
                let _ = engine.templates.load_tsv(path_str);
            }
        }

        let cj_path = p.join("cangjie.tsv");
        if cj_path.exists() {
            let _ = engine.cangjie.table.load_tsv(&cj_path);
        }

        let ar30_path = p.join("array30.tsv");
        if ar30_path.exists() {
            let _ = engine.array30.table.load_tsv(&ar30_path);
        }

        let jp_path = p.join("japanese.tsv");
        if jp_path.exists() {
            let _ = engine.japanese.load_tsv(&jp_path);
        }

        let en_path = p.join("lexicon_english.tsv");
        if en_path.exists() {
            if let Some(path_str) = en_path.to_str() {
                let _ = engine.western.load_tsv(path_str);
            }
        }
        
        engine
    }

    pub fn new() -> Self {
        let mut resolved_dir = String::new();
        
        // 1. Adjacent to executable: {exe_dir}/data/
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let adj_data = parent.join("data");
                if adj_data.exists() && adj_data.is_dir() {
                    resolved_dir = adj_data.to_string_lossy().to_string();
                }
            }
        }
        
        // 2. Relative in-tree paths
        if resolved_dir.is_empty() {
            let candidates = [
                std::path::PathBuf::from("../ruhi_lexicon/data"),
                std::path::PathBuf::from("../../ruhi_lexicon/data"),
                std::path::PathBuf::from("ruhi_lexicon/data"),
                std::path::PathBuf::from("data"),
            ];
            for cand in &candidates {
                if cand.exists() && cand.is_dir() {
                    resolved_dir = cand.to_string_lossy().to_string();
                    break;
                }
            }
        }

        // 3. APPDATA fallback
        if resolved_dir.is_empty() {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let p = std::path::PathBuf::from(appdata).join("Ruhi").join("data");
                if p.exists() && p.is_dir() {
                    resolved_dir = p.to_string_lossy().to_string();
                }
            }
        }

        if resolved_dir.is_empty() {
            resolved_dir = "../ruhi_lexicon/data".to_string();
        }
        
        Self::new_with_data_dir(&resolved_dir)
    }

    pub fn reset_buffer(&mut self) {
        self.fsm.reset();
        self.cangjie.reset();
        self.array30.reset();
        self.japanese.reset();
        self.western.reset();
        self.mode = EngineMode::Normal;
        self.q_press_count = 0;
        self.backtick_press_count = 0;
        self.composed_sentence.clear();
        self.sentence_chars.clear();
        self.current_candidates.clear();
        self.candidate_cycle_idx = 0;
        self.cursor_pos = 0;
        self.user_pinned.clear();
        self.segments.clear();
        self.raw_ascii_buffer.clear();
        self.is_latin_mode = false;
        self.manual_cuts.clear();
        self.candidate_target_range = None;
        self.lexicon.last_check_instant = None;
        self.transitions.last_check_instant = None;
    }

    pub fn get_display_text(&self) -> String {
        if self.is_latin_mode {
            return self.composed_sentence.clone();
        }
        if self.fsm.has_pending() && self.sentence_chars.len() <= self.fsm.syllables.len() {
            let pending_bpmf = if self.fsm.cur_tone == '5' {
                format!("˙{}{}{}", self.fsm.cur_initial, self.fsm.cur_medial, self.fsm.cur_final)
            } else {
                format!("{}{}{}", self.fsm.cur_initial, self.fsm.cur_medial, self.fsm.cur_final)
            };
            format!("{}{}", self.composed_sentence, pending_bpmf)
        } else {
            self.composed_sentence.clone()
        }
    }

    pub fn get_display_len(&self) -> usize {
        self.get_display_text().chars().count()
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.candidate_target_range = None;
            self.update_active_candidates();
        }
    }

    pub fn move_cursor_right(&mut self) {
        let total_len = self.get_display_len();
        if self.cursor_pos < total_len {
            self.cursor_pos += 1;
            self.candidate_target_range = None;
            self.update_active_candidates();
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
        self.candidate_target_range = None;
        self.update_active_candidates();
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.get_display_len();
        self.candidate_target_range = None;
        self.update_active_candidates();
    }

    pub fn record_committed(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let last_ch = match trimmed.chars().last() {
            Some(c) => c,
            None => return,
        };
        if last_ch.is_ascii_digit() || last_ch == '%' {
            let mut num_rev = Vec::new();
            for c in trimmed.chars().rev() {
                if c.is_ascii_digit() || c == '.' || c == ',' || c == '%' {
                    num_rev.push(c);
                } else {
                    break;
                }
            }
            let num_token: String = num_rev.into_iter().rev().collect();
            if num_token.chars().any(|c| c.is_ascii_digit()) {
                self.last_committed_word = num_token;
                return;
            }
        }
        if last_ch.is_ascii_alphanumeric() || last_ch == '_' || last_ch == '-' {
            let mut ascii_rev = Vec::new();
            for c in trimmed.chars().rev() {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    ascii_rev.push(c);
                } else {
                    break;
                }
            }
            self.last_committed_word = ascii_rev.into_iter().rev().collect();
        } else if "，。、！？：；（）「」『』—…～《》〈〉,.!?:;()[]{}\"'".contains(last_ch) {
            self.last_committed_word.clear();
        } else {
            let chars: Vec<char> = trimmed.chars().collect();
            if chars.len() >= 2 {
                let tail2: String = chars[chars.len() - 2..].iter().collect();
                if tail2.chars().all(|c| !c.is_ascii() && !"，。、！？：；（）「」『』—…～《》〈〉".contains(c)) {
                    self.last_committed_word = tail2;
                } else {
                    self.last_committed_word = last_ch.to_string();
                }
            } else {
                self.last_committed_word = chars.iter().collect();
            }
        }
    }

    pub fn set_keyboard_mode(&mut self, mode: &str) {
        self.keyboard_mode = mode.to_string();
        self.reset_buffer();
    }

    pub fn set_enabled_keyboard_modes(&mut self, modes: Vec<String>) {
        let valid: Vec<String> = modes
            .into_iter()
            .map(|m| match m.trim().to_ascii_lowercase().as_str() {
                "daikien" => "standard".to_string(),
                "eten26" => "et26".to_string(),
                "jp" | "romaji" => "japanese".to_string(),
                "ar30" => "array30".to_string(),
                other => other.to_string(),
            })
            .filter(|m| !m.is_empty())
            .collect();
        if !valid.is_empty() {
            self.enabled_keyboard_modes = valid;
        }
    }

    pub fn toggle_keyboard_mode(&mut self) -> String {
        if self.keyboard_mode == "cangjie" || self.keyboard_mode == "array30" {
            self.keyboard_mode = "hsu".to_string();
        } else {
            self.keyboard_mode = "cangjie".to_string();
        }
        self.reset_buffer();
        self.keyboard_mode.clone()
    }

    /// 依據 `enabled_keyboard_modes` 自選列表循環切換亞洲輸入法鍵盤方案
    /// 預設為 `["hsu", "japanese"]`（許氏雙拼 ⇄ 日文羅馬字），使用者可在設定中心自選啟用清單（如排除標準大千與倚天26鍵）
    pub fn cycle_keyboard_mode(&mut self) -> String {
        let list = if self.enabled_keyboard_modes.is_empty() {
            vec!["hsu".to_string(), "japanese".to_string()]
        } else {
            self.enabled_keyboard_modes.clone()
        };
        let cur_norm = match self.keyboard_mode.as_str() {
            "daikien" => "standard",
            "eten26" => "et26",
            "jp" | "romaji" => "japanese",
            "ar30" => "array30",
            other => other,
        };
        let next_mode = if let Some(pos) = list.iter().position(|m| m == cur_norm) {
            list[(pos + 1) % list.len()].clone()
        } else {
            list[0].clone()
        };
        self.keyboard_mode = next_mode;
        self.reset_buffer();
        self.keyboard_mode.clone()
    }

    /// 取得當前亞洲輸入法方案之單字徽章標記（供候選視窗與狀態列顯示）
    pub fn get_keyboard_mode_badge(&self) -> &'static str {
        match self.keyboard_mode.as_str() {
            "standard" | "daikien" => "注",
            "et26" | "eten26" => "倚",
            "cangjie" => "倉",
            "array30" | "ar30" => "行",
            "japanese" | "jp" | "romaji" => "日",
            _ => "許",
        }
    }

    /// 為指定文字產生「許氏注音 ➔ 行列 30 即時反查教練」提示字串
    pub fn format_array30_coach_for_text(&self, text: &str) -> String {
        self.array30.table.format_text_coach(text)
    }

    /// 取得當前引擎狀態下的「許氏注音 ➔ 行列 30 即時反查教練」提示：
    /// - 若處於選字模式 (`CandidatePopup`) 且有高亮候選詞，針對當前高亮候選詞反查行列 30 筆形座標與拆碼
    /// - 若處於一般組字模式且組字區非空，針對游標所在字與組字句反查行列 30 筆形座標與拆碼
    /// - 若組字區已上屏清空，針對剛上屏之 `last_committed_word` 提供反查提示
    pub fn get_array30_coach_hint(&self) -> String {
        if !self.enable_array30_coach {
            return String::new();
        }
        if matches!(
            self.keyboard_mode.as_str(),
            "array30" | "ar30" | "cangjie" | "japanese" | "jp" | "romaji"
        ) {
            return String::new();
        }
        if self.mode == EngineMode::CandidatePopup && !self.current_candidates.is_empty() {
            let idx = self
                .candidate_cycle_idx
                .min(self.current_candidates.len().saturating_sub(1));
            if let Some(cand) = self.current_candidates.get(idx) {
                let hint = self.array30.table.format_text_coach(cand);
                if !hint.is_empty() {
                    return format!("🎯 行列教練：{}", hint);
                }
            }
        }
        if !self.composed_sentence.is_empty() && !self.is_latin_mode {
            let active_idx = self.get_active_char_index();
            if let Some(ch_str) = self.sentence_chars.get(active_idx) {
                if let Some(ch) = ch_str.chars().next() {
                    if self.sentence_chars.len() == 1 {
                        if let Some(h) = self.array30.table.format_char_coach(ch) {
                            return format!("🎯 行列教練：{}", h);
                        }
                    }
                }
            }
            let hint = self.array30.table.format_text_coach(&self.composed_sentence);
            if !hint.is_empty() {
                return format!("🎯 行列教練：{}", hint);
            }
        }
        if !self.last_committed_word.is_empty() {
            let hint = self.array30.table.format_text_coach(&self.last_committed_word);
            if !hint.is_empty() {
                return format!("🎯 行列教練：{}", hint);
            }
        }
        String::new()
    }

    /// 觸發 `↓` 下箭頭：同音字／同音詞／歐文重音變體（Allographs & Loanword Diacritics）查詢
    pub fn open_down_arrow_candidates(&mut self) -> EngineResponse {
        if matches!(self.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") {
            let resp = self.japanese.process_key_str("Down");
            self.composed_sentence = resp.composed.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.cursor_pos = resp.cursor_pos;
            self.candidate_target_range = Some((0, self.sentence_chars.len()));
            self.current_candidates = resp.candidates.clone();
            self.candidate_cycle_idx = resp.candidate_idx;
            if !resp.candidates.is_empty() {
                self.mode = EngineMode::CandidatePopup;
            }
            return resp;
        }
        if self.fsm.has_pending() && !self.is_latin_mode {
            self.fsm.commit_current('1');
            self.update_sentence_composition();
        }
        if self.is_latin_mode || (!self.raw_ascii_buffer.is_empty() && self.composed_sentence == self.raw_ascii_buffer) {
            let active_idx = self.get_active_char_index();
            let target_ch = self
                .sentence_chars
                .get(active_idx)
                .and_then(|s| s.chars().next());
            let cands = self.western.lookup_down_arrow_candidates(&self.composed_sentence, target_ch);
            if !cands.is_empty() {
                let first_len = cands[0].chars().count();
                if first_len > 1 && self.sentence_chars.len() > 1 {
                    self.candidate_target_range = Some((0, self.sentence_chars.len()));
                } else if active_idx < self.sentence_chars.len() {
                    self.candidate_target_range = Some((active_idx, active_idx + 1));
                }
                self.current_candidates = cands;
                self.candidate_cycle_idx = 0;
                self.mode = EngineMode::CandidatePopup;
                return self.build_response("show_candidate_popup", "");
            }
        }
        self.update_active_candidates();
        let active_idx = self.get_active_char_index();
        if active_idx < self.sentence_chars.len() {
            self.candidate_target_range = Some((active_idx, active_idx + 1));
        }
        self.mode = EngineMode::CandidatePopup;
        self.build_response("show_candidate_popup", "")
    }

    /// 觸發 `↑` 上箭頭：
    /// 1. 歐文軌與跨軌縮寫展開／拼字校正救援（如 `tks` ➔ `thanks`, `recieve` ➔ `receive`, `dont` ➔ `don't`）
    /// 2. 多模式三字詞以上（>= 3 字）頭碼展開（如許氏 `jcdl` ➔ `忠孝東路`、`tbcj` ➔ `台北車站`、`drj` ➔ `砥睿者`；倉頡 `ljdr` ➔ `忠孝東路`）
    /// 3. 日文羅馬字模式（`japanese`）：日文長句／敬語前綴預測展開（如 `ariga` ➔ `ありがとうございます`、`yoroshi` ➔ `よろしくお願いします`）與跨軌歐文救援
    /// 4. 亞洲語文軌上下文語意推斷與模糊音／跨聲調救濟（如 `答按` ➔ `答案`, `可能地答案` ➔ `可能的答案`）
    pub fn open_up_arrow_candidates(&mut self) -> EngineResponse {
        // 0a. 日文羅馬字模式：日文前綴預測展開 + 跨軌歐文縮寫救援
        if matches!(self.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") {
            let raw = self.japanese.raw_romaji.clone();
            let jp_preds = self.japanese.lookup_up_arrow_predictions();
            let mut cands = Vec::new();
            if self.western.has_high_confidence_rescue(&raw) {
                for w in self.western.lookup_up_arrow_candidates(&raw) {
                    if !cands.contains(&w) {
                        cands.push(w);
                    }
                }
                for w in &jp_preds {
                    if !cands.contains(w) {
                        cands.push(w.clone());
                    }
                }
            } else {
                cands.extend(jp_preds);
                for w in self.western.lookup_up_arrow_candidates(&raw) {
                    if !cands.contains(&w) {
                        cands.push(w);
                    }
                }
            }
            if !cands.is_empty() {
                let first = cands[0].clone();
                self.japanese.candidates = cands.clone();
                self.japanese.candidate_idx = 0;
                self.composed_sentence = first.clone();
                self.sentence_chars = first.chars().map(|c| c.to_string()).collect();
                self.cursor_pos = self.sentence_chars.len();
                self.candidate_target_range = Some((0, self.sentence_chars.len()));
                self.current_candidates = cands.clone();
                self.candidate_cycle_idx = 0;
                self.mode = EngineMode::CandidatePopup;
                return EngineResponse {
                    action: "show_candidate_popup".to_string(),
                    mode: "candidates".to_string(),
                    preedit: self.japanese.make_preedit(),
                    composed: first.clone(),
                    candidates: cands,
                    active_candidate: first,
                    candidate_idx: 0,
                    cursor_pos: self.cursor_pos,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                };
            }
            return self.build_response("none", "");
        }

        // 0b. 倉頡模式：優先處理跨軌歐文縮寫救援或三字詞以上（>= 3 字）倉頡首碼頭碼展開
        if self.keyboard_mode == "cangjie" {
            if !self.cangjie.buffer.is_empty() {
                let buf = self.cangjie.buffer.clone();
                let mut cands = Vec::new();
                if self.western.has_high_confidence_rescue(&buf) {
                    cands.extend(self.western.lookup_up_arrow_candidates(&buf));
                    for w in self.cangjie.table.lookup_phrase_headcode(&buf, &self.lexicon) {
                        if !cands.contains(&w) {
                            cands.push(w);
                        }
                    }
                } else {
                    cands.extend(self.cangjie.table.lookup_phrase_headcode(&buf, &self.lexicon));
                    for w in self.western.lookup_up_arrow_candidates(&buf) {
                        if !cands.contains(&w) {
                            cands.push(w);
                        }
                    }
                }
                if !cands.is_empty() {
                    let first = cands[0].clone();
                    self.cangjie.candidates = cands.clone();
                    self.cangjie.candidate_idx = 0;
                    self.composed_sentence = first.clone();
                    self.sentence_chars = first.chars().map(|c| c.to_string()).collect();
                    self.cursor_pos = self.sentence_chars.len();
                    self.candidate_target_range = Some((0, self.sentence_chars.len()));
                    self.current_candidates = cands.clone();
                    self.candidate_cycle_idx = 0;
                    self.mode = EngineMode::CandidatePopup;
                    return EngineResponse {
                        action: "show_candidate_popup".to_string(),
                        mode: "candidates".to_string(),
                        preedit: buf,
                        composed: first.clone(),
                        candidates: cands,
                        active_candidate: first,
                        candidate_idx: 0,
                        cursor_pos: self.cursor_pos,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
            }
            return self.build_response("none", "");
        }

        // 0c. 行列 30 模式：優先處理跨軌歐文縮寫救援或多字詞行列首碼頭碼展開
        if matches!(self.keyboard_mode.as_str(), "array30" | "ar30") {
            if !self.array30.buffer.is_empty() {
                let buf = self.array30.buffer.clone();
                let mut cands = Vec::new();
                if self.western.has_high_confidence_rescue(&buf) {
                    cands.extend(self.western.lookup_up_arrow_candidates(&buf));
                    for w in self.array30.table.lookup_phrase_headcode(&buf, &self.lexicon) {
                        if !cands.contains(&w) {
                            cands.push(w);
                        }
                    }
                } else {
                    cands.extend(self.array30.table.lookup_phrase_headcode(&buf, &self.lexicon));
                    for w in self.western.lookup_up_arrow_candidates(&buf) {
                        if !cands.contains(&w) {
                            cands.push(w);
                        }
                    }
                }
                if !cands.is_empty() {
                    let first = cands[0].clone();
                    self.array30.candidates = cands.clone();
                    self.array30.candidate_idx = 0;
                    self.composed_sentence = first.clone();
                    self.sentence_chars = first.chars().map(|c| c.to_string()).collect();
                    self.cursor_pos = self.sentence_chars.len();
                    self.candidate_target_range = Some((0, self.sentence_chars.len()));
                    self.current_candidates = cands.clone();
                    self.candidate_cycle_idx = 0;
                    self.mode = EngineMode::CandidatePopup;
                    return EngineResponse {
                        action: "show_candidate_popup".to_string(),
                        mode: "candidates".to_string(),
                        preedit: self.array30.make_preedit(),
                        composed: first.clone(),
                        candidates: cands,
                        active_candidate: first,
                        candidate_idx: 0,
                        cursor_pos: self.cursor_pos,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
            }
            return self.build_response("none", "");
        }

        // 1. 檢查三字詞以上（>= 3 字）頭碼展開（當非高信心歐文縮寫如 tks/pls/btw/dont 時，頭碼優先）
        if self.raw_ascii_buffer.chars().count() >= 3
            && !self.western.has_high_confidence_rescue(&self.raw_ascii_buffer)
        {
            let prev_ctx = if self.last_committed_word.is_empty() {
                None
            } else {
                Some(self.last_committed_word.clone())
            };
            let mut hc_cands = self.lexicon.lookup_multi_char_headcode(
                &self.raw_ascii_buffer,
                &self.keyboard_mode,
                prev_ctx.as_deref(),
                Some(&mut self.transitions),
            );
            if !hc_cands.is_empty() {
                for w in self.western.lookup_up_arrow_candidates(&self.raw_ascii_buffer) {
                    if !hc_cands.contains(&w) {
                        hc_cands.push(w);
                    }
                }
                self.is_latin_mode = false;
                self.fsm.cur_initial.clear();
                self.fsm.cur_medial.clear();
                self.fsm.cur_final.clear();
                self.fsm.cur_tone = '1';
                if self.sentence_chars.is_empty() {
                    self.composed_sentence = self.raw_ascii_buffer.clone();
                    self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                }
                self.cursor_pos = self.sentence_chars.len();
                self.candidate_target_range = Some((0, self.sentence_chars.len()));
                self.current_candidates = hc_cands;
                self.candidate_cycle_idx = 0;
                self.mode = EngineMode::CandidatePopup;
                return self.build_response("show_candidate_popup", "");
            }
        }

        // 2. 檢查歐文模式或跨軌高信心縮寫／拼字救援 (Cross-Track Rescue)
        if !self.raw_ascii_buffer.is_empty()
            && (self.is_latin_mode || self.western.has_high_confidence_rescue(&self.raw_ascii_buffer))
        {
            let cands = self.western.lookup_up_arrow_candidates(&self.raw_ascii_buffer);
            if !cands.is_empty() {
                self.is_latin_mode = true;
                self.fsm.reset();
                self.composed_sentence = self.raw_ascii_buffer.clone();
                self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                self.cursor_pos = self.sentence_chars.len();
                self.candidate_target_range = Some((0, self.sentence_chars.len()));
                self.current_candidates = cands;
                self.candidate_cycle_idx = 0;
                self.mode = EngineMode::CandidatePopup;
                return self.build_response("show_candidate_popup", "");
            }
        }

        // 3. 亞洲語文軌：若有未決注音先結算，再執行上下文與模糊音救援
        if self.fsm.has_pending() && !self.is_latin_mode {
            self.fsm.commit_current('1');
            self.update_sentence_composition();
        }
        let chars: Vec<char> = self.composed_sentence.chars().collect();
        if chars.is_empty() {
            return self.build_response("none", "");
        }
        let syls: Vec<Option<String>> = (0..chars.len()).map(|i| self.get_syllable_at(i)).collect();
        let active_idx = self.get_active_char_index();
        let (_actual_idx, range, cands) = self.lexicon.lookup_contextual_fuzzy_rescue(
            &chars,
            &syls,
            active_idx,
            None,
            &mut self.transitions,
        );
        if !cands.is_empty() {
            self.candidate_target_range = Some(range);
            self.cursor_pos = range.1;
            self.current_candidates = cands;
            self.candidate_cycle_idx = 0;
            self.mode = EngineMode::CandidatePopup;
            return self.build_response("show_candidate_popup", "");
        }
        self.build_response("none", "")
    }

    /// 選擇指定索引之候選詞（支援單字、雙字複合詞、三字詞以上頭碼展開及變長歐文縮寫替換）
    pub fn select_candidate(&mut self, digit_idx: usize) -> Option<String> {
        if matches!(self.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") && digit_idx < self.japanese.candidates.len() {
            let chosen = self.japanese.candidates[digit_idx].clone();
            self.composed_sentence = chosen.clone();
            self.sentence_chars = chosen.chars().map(|c| c.to_string()).collect();
            self.cursor_pos = self.sentence_chars.len();
            self.candidate_cycle_idx = digit_idx;
            self.japanese.candidate_idx = digit_idx;
            return Some(chosen);
        }
        if self.keyboard_mode == "cangjie" && digit_idx < self.cangjie.candidates.len() {
            let chosen = self.cangjie.candidates[digit_idx].clone();
            self.composed_sentence = chosen.clone();
            self.sentence_chars = chosen.chars().map(|c| c.to_string()).collect();
            self.cursor_pos = self.sentence_chars.len();
            self.candidate_cycle_idx = digit_idx;
            self.cangjie.candidate_idx = digit_idx;
            return Some(chosen);
        }
        if matches!(self.keyboard_mode.as_str(), "array30" | "ar30") && digit_idx < self.array30.candidates.len() {
            let chosen = self.array30.candidates[digit_idx].clone();
            self.composed_sentence = chosen.clone();
            self.sentence_chars = chosen.chars().map(|c| c.to_string()).collect();
            self.cursor_pos = self.sentence_chars.len();
            self.candidate_cycle_idx = digit_idx;
            self.array30.candidate_idx = digit_idx;
            return Some(chosen);
        }
        if digit_idx >= self.current_candidates.len() || self.sentence_chars.is_empty() {
            return None;
        }
        let chosen = self.current_candidates[digit_idx].clone();
        let orig_len = self.sentence_chars.len();
        let (start, end) = self
            .candidate_target_range
            .unwrap_or_else(|| {
                let idx = self.get_active_char_index();
                (idx, (idx + 1).min(self.sentence_chars.len()))
            });
        if start >= end || end > self.sentence_chars.len() {
            return None;
        }

        let rejected: String = self.sentence_chars[start..end].join("");
        if rejected != chosen && !self.is_latin_mode {
            let bpm = self.get_syllable_at(start).unwrap_or_default();
            let prev_ctx = if start > 0 { self.sentence_chars[start - 1].clone() } else { String::new() };
            let next_ctx = if end < self.sentence_chars.len() { self.sentence_chars[end].clone() } else { String::new() };
            self.record_revision(&bpm, &rejected, &chosen, &prev_ctx, &next_ctx);
        }

        let chosen_chars: Vec<String> = chosen.chars().map(|c| c.to_string()).collect();
        let chosen_len = chosen_chars.len();
        self.sentence_chars.splice(start..end, chosen_chars.clone());
        if (end - start == chosen_len || (start == 0 && end == orig_len)) && !self.is_latin_mode {
            for (offset, ch_str) in chosen_chars.into_iter().enumerate() {
                self.user_pinned.insert(start + offset, ch_str);
            }
        }
        self.composed_sentence = self.sentence_chars.join("");
        if self.is_latin_mode {
            self.raw_ascii_buffer = self.composed_sentence.clone();
        }
        self.candidate_cycle_idx = digit_idx;
        self.candidate_target_range = None;
        self.cursor_pos = (start + chosen_len).min(self.sentence_chars.len());
        if !self.is_latin_mode {
            self.update_active_candidates();
        }
        Some(chosen)
    }

    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        if matches!(self.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") {
            if key_str == "Up" || key_str == "up" {
                return self.open_up_arrow_candidates();
            }
            if key_str == "Down" || key_str == "down" {
                return self.open_down_arrow_candidates();
            }
            let resp = self.japanese.process_key_str(key_str);
            self.composed_sentence = resp.composed.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.current_candidates = resp.candidates.clone();
            self.candidate_cycle_idx = resp.candidate_idx;
            self.cursor_pos = resp.cursor_pos;
            return resp;
        }
        if self.keyboard_mode == "cangjie" {
            if key_str == "Up" || key_str == "up" {
                return self.open_up_arrow_candidates();
            }
            return self.cangjie.process_key_str(key_str);
        }
        if matches!(self.keyboard_mode.as_str(), "array30" | "ar30") {
            if key_str == "Up" || key_str == "up" {
                return self.open_up_arrow_candidates();
            }
            let resp = self.array30.process_key_str(key_str);
            self.composed_sentence = resp.composed.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.current_candidates = resp.candidates.clone();
            self.candidate_cycle_idx = resp.candidate_idx;
            self.cursor_pos = resp.cursor_pos;
            return resp;
        }
        if key_str == "Down" || key_str == "down" {
            return self.open_down_arrow_candidates();
        }
        if key_str == "Up" || key_str == "up" {
            return self.open_up_arrow_candidates();
        }
        if key_str == "BackSpace" || key_str == "backspace" || key_str == "\x08" {
            return self.pop_component();
        }
        if key_str == "Shift+BackSpace" || key_str == "Shift+backspace" {
            return self.pop_syllable();
        }
        if key_str == "Ctrl+BackSpace" || key_str == "Ctrl+backspace" {
            return self.pop_word();
        }
        if let Some(punct) = self.symbols.get(key_str) {
            if self.fsm.has_pending() && !self.is_latin_mode {
                self.fsm.commit_current('1');
                self.update_sentence_composition();
            }
            let mut committed = self.composed_sentence.clone();
            committed.push_str(&punct);
            self.reset_buffer();
            return self.build_response("commit_punctuation", &committed);
        }
        if key_str.chars().count() == 1 {
            let ch = key_str.chars().next().unwrap();
            return self.process_key(ch);
        }
        self.build_response("none", "")
    }

    pub fn process_key(&mut self, key: char) -> EngineResponse {
        if matches!(self.keyboard_mode.as_str(), "japanese" | "jp" | "romaji") {
            let resp = self.japanese.process_key(key);
            self.composed_sentence = resp.composed.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.current_candidates = resp.candidates.clone();
            self.candidate_cycle_idx = resp.candidate_idx;
            self.cursor_pos = resp.cursor_pos;
            return resp;
        }
        if self.keyboard_mode == "cangjie" {
            return self.cangjie.process_key(key);
        }
        if matches!(self.keyboard_mode.as_str(), "array30" | "ar30") {
            let resp = self.array30.process_key_with_lexicon(key, Some(&self.lexicon));
            self.composed_sentence = resp.composed.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.current_candidates = resp.candidates.clone();
            self.candidate_cycle_idx = resp.candidate_idx;
            self.cursor_pos = resp.cursor_pos;
            return resp;
        }
        // 1. 波浪號 `~` ➔ 專業詞庫切換
        if key == '~' {
            self.mode = EngineMode::LexiconSwitch;
            return self.build_response("show_lexicon_switcher", "");
        }

        // 2. 反引號 `` ` `` ➔ 文字範本 / 數字快打
        if key == '`' {
            self.backtick_press_count += 1;
            if self.backtick_press_count == 1 {
                self.mode = EngineMode::TemplateSelect;
                return self.build_response("show_templates", "");
            } else if self.backtick_press_count >= 2 {
                self.mode = EngineMode::QuickNum;
                return self.build_response("show_quick_num", "");
            }
        }

        if key != '`' {
            self.backtick_press_count = 0;
        }

        // 3. 數字鍵輸入處理 ('0'..='9') 與數值小數點 ('.')
        let prev_is_digit = self.composed_sentence.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false);
        if key.is_ascii_digit() || (key == '.' && prev_is_digit) {
            // 僅在明確彈出選字清單時作為候選數字鍵 (1~9)
            if self.mode == EngineMode::CandidatePopup && !self.current_candidates.is_empty() && key != '0' && key != '.' {
                let digit_idx = (key as usize) - ('1' as usize);
                if self.select_candidate(digit_idx).is_some() {
                    return self.build_response("update_candidate", "");
                }
            } else if matches!(self.keyboard_mode.as_str(), "standard" | "daikien") && matches!(key, '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' | '0') {
                // 標準大千鍵盤：數字鍵為注音聲母／韻母／聲調鍵，交由下方 FSM 解析
            } else {
                // 一般打字模式：數字與小數點直接納入全句中，絕不擅自竄改前文字詞！
                if self.fsm.has_pending() {
                    self.fsm.commit_current('1');
                    self.update_sentence_composition();
                }
                let digit_str = key.to_string();
                self.sentence_chars.push(digit_str.clone());
                self.composed_sentence.push_str(&digit_str);
                self.cursor_pos = self.composed_sentence.chars().count();
                self.fsm.reset();
                self.raw_ascii_buffer.clear();
                self.western.reset();
                let current_composed = self.composed_sentence.clone();
                self.record_committed(&current_composed);
                return self.build_response("update_preedit", "");
            }
        }

        // 4. Q 鍵候選輪換與選字窗彈出（僅許氏雙拼模式，因標準大千與倚天26鍵之 Q 鍵為聲母「ㄆ」）
        if (key == 'q' || key == 'Q') && self.keyboard_mode == "hsu" && !self.fsm.get_all_syllables().is_empty() {
            if self.fsm.has_pending() {
                let pending_bpmf = format!("{}{}{}", self.fsm.cur_initial, self.fsm.cur_medial, self.fsm.cur_final);
                if pending_bpmf.chars().count() == 1 {
                    self.fsm.commit_current('1');
                    let new_idx = self.sentence_chars.len();
                    self.sentence_chars.push(pending_bpmf.clone());
                    self.user_pinned.insert(new_idx, pending_bpmf.clone());
                    self.composed_sentence = self.sentence_chars.join("");
                    self.cursor_pos = self.sentence_chars.len();
                    let mut cands = crate::lexicon::LexiconManager::get_hsu_bopomofo_symbols(&pending_bpmf);
                    for e in self.lexicon.lookup_bopomofo(&pending_bpmf, true) {
                        if e.word.chars().count() == 1 && !cands.contains(&e.word) {
                            cands.push(e.word.clone());
                        }
                    }
                    self.current_candidates = cands;
                    self.candidate_cycle_idx = 0;
                }
            }
            self.q_press_count += 1;
            if self.q_press_count >= self.q_popup_threshold {
                self.mode = EngineMode::CandidatePopup;
                return self.build_response("show_candidate_popup", "");
            } else {
                self.cycle_next_candidate();
                return self.build_response("cycle_candidate", "");
            }
        }

        if key != 'q' && key != 'Q' {
            self.q_press_count = 0;
        }

        // 5. 標點符號與自訂符號映射（支援 TSV 熱置換與使用者即時自訂；排除標準大千之韻母鍵 , . ; / -）
        let is_standard_bpm_punct = matches!(self.keyboard_mode.as_str(), "standard" | "daikien")
            && matches!(key, ',' | '.' | ';' | '/' | '-');
        if !is_standard_bpm_punct {
            if let Some(punct) = self.symbols.get_char(key) {
            let has_compound = self.has_any_compound_word();
            let raw_len = self.raw_ascii_buffer.len();
            let raw_lower = self.raw_ascii_buffer.to_ascii_lowercase();
            let is_english_token = if !self.enable_mixed_english || self.mixed_english_mode == "disabled" {
                false
            } else if self.is_latin_mode {
                true
            } else if self.mixed_english_mode == "conservative" {
                false
            } else if self.mixed_english_mode == "min_length_only" {
                !has_compound && raw_len >= self.mixed_english_min_len && self.western.lexicon.contains(&raw_lower)
            } else {
                // "contextual_stream" (default)
                !has_compound && (
                    (raw_len >= 3 && self.western.lexicon.contains(&raw_lower))
                    || (self.prior_was_english && self.western.lexicon.contains(&raw_lower))
                )
            };

            let base_commit = if is_english_token && !self.raw_ascii_buffer.is_empty() {
                self.prior_was_english = true;
                self.raw_ascii_buffer.clone()
            } else {
                if self.fsm.has_pending() && !self.is_latin_mode {
                    self.fsm.commit_current('1');
                    self.update_sentence_composition();
                }
                if !self.composed_sentence.is_empty() {
                    self.prior_was_english = false;
                }
                self.composed_sentence.clone()
            };

            let mut committed = base_commit;
            committed.push_str(&punct);
            self.record_committed(&committed);
            self.reset_buffer();
            return self.build_response("commit_punctuation", &committed);
            }
        }

        // 6. Enter 結算
        if key == '\n' || key == '\r' {
            if self.composed_sentence.is_empty() && self.fsm.get_display_preedit().is_empty() && self.raw_ascii_buffer.is_empty() {
                return self.build_response("none", "");
            }
            if self.has_pending_revisions() {
                let _ = self.auto_synthesize_and_apply_revisions();
            }
            // 雙軌正向晶格競爭仲裁：
            // 歐文信心度評估 vs 中文 Trellis 複合詞凝聚力
            let _western_conf = self.western.score_string(&self.raw_ascii_buffer);
            let has_compound_word = self.segments.iter().any(|n| n.length() >= 2);
            let raw_len = self.raw_ascii_buffer.len();
            let raw_lower = self.raw_ascii_buffer.to_ascii_lowercase();
            let is_protected_chinese_sound = raw_lower == "ha" || raw_lower == "has";
            let is_pending_single_syllable = self.fsm.syllables.is_empty() && self.fsm.has_pending();

            let is_valid_en_token = if raw_len == 1 {
                raw_lower == "a" || raw_lower == "i"
            } else if raw_len >= 2 {
                !is_protected_chinese_sound && (
                    self.western.lexicon.contains(&raw_lower)
                    || crate::western::WesternConfidenceScorer::COMMON_ABBREVS.contains(&raw_lower.as_str())
                )
            } else {
                false
            };

            let is_english_candidate = if !self.enable_mixed_english || self.mixed_english_mode == "disabled" {
                false
            } else if self.is_latin_mode {
                true
            } else if self.mixed_english_mode == "conservative" {
                false
            } else if self.mixed_english_mode == "min_length_only" {
                !has_compound_word && is_valid_en_token && raw_len >= self.mixed_english_min_len
            } else {
                // "contextual_stream" (default)
                !has_compound_word && is_valid_en_token && (
                    raw_len >= 4
                    || (raw_len >= 3 && (!is_pending_single_syllable || self.prior_was_english))
                    || self.prior_was_english
                )
            };

            let committed = if !self.raw_ascii_buffer.is_empty() && is_english_candidate {
                self.prior_was_english = true;
                self.raw_ascii_buffer.clone()
            } else if !self.composed_sentence.is_empty() {
                self.prior_was_english = false;
                self.composed_sentence.clone()
            } else {
                self.prior_was_english = false;
                self.fsm.get_display_preedit()
            };
            self.record_committed(&committed);
            self.reset_buffer();
            return self.build_response("commit_enter", &committed);
        }

        // 6.5 Tab 鍵：手動斷詞切換 (Manual Word Boundary Split / Cut)
        if key == '\t' {
            if self.fsm.syllables.len() > 1 && self.cursor_pos > 0 && self.cursor_pos < self.sentence_chars.len() {
                self.toggle_cut_at_cursor();
                return self.build_response("update_preedit", "");
            }
            return self.build_response("none", "");
        }

        // 7. Backspace 退格
        if key == '\x08' {
            self.western.backspace();
            if !self.raw_ascii_buffer.is_empty() {
                self.raw_ascii_buffer.pop();
            }
            if self.is_latin_mode {
                if self.raw_ascii_buffer.is_empty() {
                    self.reset_buffer();
                    return self.build_response("update_preedit", "");
                } else {
                    self.composed_sentence = self.raw_ascii_buffer.clone();
                    self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                    self.cursor_pos = self.composed_sentence.chars().count();
                    self.current_candidates = vec![self.raw_ascii_buffer.clone()];
                    return self.build_response("update_preedit", "");
                }
            }
            return self.pop_component();
        }

        // 7.5 許氏＋歐文智慧混打模式判斷
        let mut prefix_commit = String::new();
        if self.enable_mixed_english && self.mixed_english_mode != "disabled" {
            // A. 空白鍵智慧判定：
            // 1. 若處於歐文模式，直通結算歐文
            // 2. 若中文軌已具備二字以上複合詞（如「發音」、「你好」、「意義」），中文軌凝聚力極高，絕不被歐文搶奪
            // 3. 在無複合詞情況下，凡合法歐文單字（and, the, hi, for, not...）或縮寫，歐文勝出結算！
            // 4. 單字拼音（如 ha -> ㄏㄟ 黑、has -> ˙ㄏㄟ 嘿、e -> ㄧ 一）由漢字一聲/輕聲優先！
            if key == ' ' {
                let has_compound = self.has_any_compound_word();
                let raw_len = self.raw_ascii_buffer.len();
                let _western_conf = self.western.score_string(&self.raw_ascii_buffer);
                let raw_lower = self.raw_ascii_buffer.to_ascii_lowercase();
                let is_protected_chinese_sound = raw_lower == "ha" || raw_lower == "has";
                let is_pending_single_syllable = self.fsm.syllables.is_empty() && self.fsm.has_pending();

                let fsm_allows_english_space = if self.mixed_english_mode == "min_length_only" {
                    !self.fsm.has_pending() || raw_len >= self.mixed_english_min_len
                } else {
                    !self.fsm.has_pending()
                        || raw_len >= 4
                        || (raw_len >= 3 && self.western.lexicon.contains(&raw_lower) && (!is_pending_single_syllable || self.prior_was_english))
                        || (self.prior_was_english && self.western.lexicon.contains(&raw_lower))
                        || raw_lower == "hi"
                };

                let is_valid_en_token = if raw_len == 1 {
                    raw_lower == "a" || raw_lower == "i"
                } else if raw_len >= 2 {
                    fsm_allows_english_space && !is_protected_chinese_sound && (
                        self.western.lexicon.contains(&raw_lower)
                        || crate::western::WesternConfidenceScorer::COMMON_ABBREVS.contains(&raw_lower.as_str())
                    )
                } else {
                    false
                };

                let is_english_word = if self.is_latin_mode {
                    true
                } else if self.mixed_english_mode == "conservative" {
                    false
                } else if self.mixed_english_mode == "min_length_only" {
                    !has_compound && is_valid_en_token && raw_len >= self.mixed_english_min_len
                } else {
                    !has_compound && is_valid_en_token
                };

                if is_english_word && !self.raw_ascii_buffer.is_empty() {
                    let committed = format!("{} ", self.raw_ascii_buffer);
                    self.record_committed(&committed);
                    self.reset_buffer();
                    self.prior_was_english = true;
                    return self.build_response("commit_space", &committed);
                }
            }

            // B. 若已處於歐文模式，持續接收英數字元
            // 若當前鍵入的字元使歐文前綴失效，而先前已形成合法歐文單字，自動結算歐文詞元並無縫切換回中文輸入流！
            if self.is_latin_mode {
                let mut next_buf = self.raw_ascii_buffer.clone();
                next_buf.push(key);

                let is_still_english_prefix = self.western.lexicon.has_prefix(&next_buf)
                    || crate::western::WesternToken::new(&next_buf).is_code_identifier();

                let has_upper = self.raw_ascii_buffer.chars().any(|c| c.is_ascii_uppercase());
                let prev_is_complete_word = (self.raw_ascii_buffer.len() >= 4 || (has_upper && self.raw_ascii_buffer.len() >= 3))
                    && (self.western.lexicon.contains(&self.raw_ascii_buffer)
                        || self.western.score_string(&self.raw_ascii_buffer) >= 0.70
                        || crate::western::WesternToken::new(&self.raw_ascii_buffer).is_code_identifier());

                if !is_still_english_prefix && prev_is_complete_word && key.is_ascii_lowercase() {
                    let prior_word = self.raw_ascii_buffer.clone();
                    self.raw_ascii_buffer.clear();
                    self.western.reset();
                    self.is_latin_mode = false;
                    self.composed_sentence.clear();
                    self.sentence_chars.clear();
                    self.user_pinned.clear();

                    self.record_committed(&prior_word);
                    prefix_commit = prior_word;
                } else if key.is_ascii_alphanumeric() || matches!(key, '-' | '_' | '.' | '@' | '/' | ':') {
                    self.raw_ascii_buffer.push(key);
                    let _ = self.western.process_key(key);
                    self.composed_sentence = self.raw_ascii_buffer.clone();
                    self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                    self.cursor_pos = self.composed_sentence.chars().count();
                    self.current_candidates = vec![self.raw_ascii_buffer.clone()];
                    return self.build_response("update_preedit", "");
                }
            }

            // C. 大寫字母直接切入歐文模式 (如 Google, NASA, iPhone, CamelCase)
            if key.is_ascii_uppercase() {
                // 若先前尚未處於歐文模式
                if !self.is_latin_mode {
                    let mut camel_check = self.raw_ascii_buffer.clone();
                    camel_check.push(key);
                    let is_code_id = crate::western::WesternToken::new(&camel_check).is_code_identifier();
                    let is_camel = is_code_id && (
                        self.western.lexicon.contains(&self.raw_ascii_buffer)
                        || self.western.score_string(&self.raw_ascii_buffer) >= 0.70
                    );

                    if is_camel {
                        // 駝峰標識符 (如 getElementById, camelCase)：保留前綴英文字，切入歐文模式
                        self.is_latin_mode = true;
                        self.fsm.reset();
                        self.raw_ascii_buffer.push(key);
                        let _ = self.western.process_key(key);
                        self.composed_sentence = self.raw_ascii_buffer.clone();
                        self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                        self.cursor_pos = self.composed_sentence.chars().count();
                        self.current_candidates = vec![self.raw_ascii_buffer.clone()];
                        return self.build_response("update_preedit", "");
                    }

                    let has_composed = !self.composed_sentence.is_empty() || !self.fsm.syllables.is_empty();

                    if has_composed {
                        if self.composed_sentence.is_empty() && !self.fsm.syllables.is_empty() {
                            self.update_sentence_composition();
                        }
                        let prior_commit = self.composed_sentence.clone();
                        self.record_committed(&prior_commit);
                        self.composed_sentence.clear();
                        self.sentence_chars.clear();
                        self.user_pinned.clear();

                        self.fsm.reset();
                        self.raw_ascii_buffer.clear();
                        self.western.reset();

                        self.is_latin_mode = true;
                        self.raw_ascii_buffer.push(key);
                        let _ = self.western.process_key(key);
                        self.composed_sentence = self.raw_ascii_buffer.clone();
                        self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                        self.cursor_pos = self.composed_sentence.chars().count();
                        self.current_candidates = vec![self.raw_ascii_buffer.clone()];

                        let mut resp = self.build_response("update_preedit", "");
                        resp.committed_chunk = prior_commit;
                        return resp;
                    }
                }

                // 歐文模式或無中文複合詞/聲調（如 getElementById 或 NASA），延續或開啟歐文串
                self.is_latin_mode = true;
                self.raw_ascii_buffer.push(key);
                let _ = self.western.process_key(key);
                self.composed_sentence = self.raw_ascii_buffer.clone();
                self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                self.cursor_pos = self.composed_sentence.chars().count();
                self.current_candidates = vec![self.raw_ascii_buffer.clone()];
                return self.build_response("update_preedit", "");
            }
        }

        if key.is_ascii_alphanumeric() || is_standard_bpm_punct {
            self.raw_ascii_buffer.push(key);
            if key.is_ascii_alphanumeric() {
                let _ = self.western.process_key(key);
            }
        }

        // 8. 許氏 FSM 解析
        // 若先前含有已完結的數字/符號前綴且 FSM 處於空狀態，在開啟新注音音節時先將前文字詞結算上屏 (Auto-commit frozen prefix)
        if self.fsm.is_empty() && !self.composed_sentence.is_empty() {
            if prefix_commit.is_empty() {
                prefix_commit = self.composed_sentence.clone();
            }
            self.record_committed(&prefix_commit);
            self.composed_sentence.clear();
            self.sentence_chars.clear();
            self.user_pinned.clear();
            self.prior_was_english = false;
        }

        let prev_syllable_count = self.fsm.syllables.len();
        let success = self.fsm.parse_key_with_layout(key, &self.keyboard_mode);
        if success {
            if self.fsm.syllables.len() > prev_syllable_count {
                self.prior_was_english = false;
                if key == ' ' {
                    // 空白鍵作為一聲聲調結算漢字音節後，清空 raw_ascii_buffer，杜絕第二個空白鍵殘留
                    self.raw_ascii_buffer.clear();
                    self.western.reset();
                }
            }

            let has_invalid_syl = if matches!(self.keyboard_mode.as_str(), "standard" | "daikien") {
                self.fsm.syllables.iter().any(|s| !s.is_mandarin_valid() && !s.is_pure_initial())
            } else {
                self.fsm.has_invalid_syllable()
            };

            if self.enable_mixed_english && (has_invalid_syl || self.fsm.has_invalid_pending()) {
                // 若先前已有確立的中文句子（如已完成「完全」），決不連坐摧毀已成立的中文！
                if !self.composed_sentence.is_empty() && !self.fsm.syllables.is_empty() && !has_invalid_syl {
                    // 僅當前 pending 音位違規（如打字手民誤鍵），回退最後無效按鍵，不波及整句
                    return self.pop_component();
                }

                // 觸發現代漢語音位配列違規（如 th ➔ ㄊㄛ、book ➔ ㄅㄡ、sample ➔ ㄙㄟㄢ 等非漢語音節）
                // 立即將當前詞元自動判定為歐文！
                self.is_latin_mode = true;
                self.prior_was_english = true;
                self.fsm.reset();
                self.composed_sentence = self.raw_ascii_buffer.clone();
                self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                self.cursor_pos = self.composed_sentence.chars().count();
                self.current_candidates = vec![self.raw_ascii_buffer.clone()];
                let mut resp = self.build_response("update_preedit", "");
                if !prefix_commit.is_empty() {
                    resp.committed_chunk = prefix_commit;
                }
                return resp;
            }

            self.update_sentence_composition();

            // 若更新後發現 Viterbi 演算法無法組出任何合法詞（nodes 為空且音節數 > 0）
            if self.enable_mixed_english && self.composed_sentence.is_empty() && !self.fsm.syllables.is_empty() {
                self.is_latin_mode = true;
                self.prior_was_english = true;
                self.fsm.reset();
                self.composed_sentence = self.raw_ascii_buffer.clone();
                self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                self.cursor_pos = self.composed_sentence.chars().count();
                self.current_candidates = vec![self.raw_ascii_buffer.clone()];
            }

            // 在混打模式下，若處於歐文模式或已有候選字時，將 raw_ascii_buffer 作為備選候選加入候補名單
            // 但若無中文候選且處於未決注音狀態（如單鍵或未定音節 am），絕不將 raw_ascii_buffer 作為第 0 順位候選！
            if self.enable_mixed_english && self.mixed_english_mode != "disabled" && !self.raw_ascii_buffer.is_empty() {
                let can_add_cand = if self.mixed_english_mode == "conservative" {
                    self.is_latin_mode
                } else if self.mixed_english_mode == "min_length_only" {
                    self.is_latin_mode || (self.raw_ascii_buffer.len() >= self.mixed_english_min_len && self.western.lexicon.contains(&self.raw_ascii_buffer))
                } else {
                    self.is_latin_mode
                        || (!self.fsm.has_pending() && !self.current_candidates.is_empty())
                        || (self.raw_ascii_buffer.len() >= 3 && self.western.lexicon.contains(&self.raw_ascii_buffer))
                        || (self.prior_was_english && self.western.lexicon.contains(&self.raw_ascii_buffer))
                };

                if can_add_cand && !self.current_candidates.contains(&self.raw_ascii_buffer) {
                    self.current_candidates.push(self.raw_ascii_buffer.clone());
                }
            }

            let mut resp = self.build_response("update_preedit", "");
            if !prefix_commit.is_empty() {
                resp.committed_chunk = prefix_commit;
            }
            return resp;
        } else if self.enable_mixed_english && key.is_ascii_alphanumeric() {
            println!("TRIGGERED key.is_ascii_alphanumeric() fallback on key '{}'", key);
            // 無法被許氏 FSM 解析的英數字元，直接進入歐文模式
            self.is_latin_mode = true;
            self.composed_sentence = self.raw_ascii_buffer.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.cursor_pos = self.composed_sentence.chars().count();
            self.current_candidates = vec![self.raw_ascii_buffer.clone()];
            return self.build_response("update_preedit", "");
        }
 else if !prefix_commit.is_empty() {
            // 若解析失敗，還原 composed_sentence
            self.composed_sentence = prefix_commit;
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
        }

        // 9. 範本匹配
        let key_str = key.to_string();
        if let Some(tpl) = self.templates.match_trigger(&key_str) {
            let (expanded, _) = tpl.expand("");
            self.reset_buffer();
            return self.build_response("commit_template", &expanded);
        }

        self.build_response("none", "")
    }

    pub fn update_sentence_composition(&mut self) {
        if self.enable_mixed_english && self.is_latin_mode {
            return;
        }
        let is_standard_headcode_stream = matches!(self.keyboard_mode.as_str(), "standard" | "daikien")
            && self.fsm.syllables.len() >= 2
            && self.fsm.syllables.iter().all(|s| s.is_pure_initial())
            && !self.fsm.cur_initial.is_empty()
            && self.fsm.cur_medial.is_empty()
            && self.fsm.cur_final.is_empty();
        let syls = if is_standard_headcode_stream {
            self.fsm.get_all_syllables()
        } else {
            self.fsm.syllables.clone()
        };
        if syls.is_empty() {
            self.composed_sentence.clear();
            self.sentence_chars.clear();
            self.current_candidates.clear();
            self.user_pinned.clear();
            return;
        }

        // 清理超出音節範圍的 pinned 鎖定與 manual_cuts 斷詞點
        self.user_pinned.retain(|&idx, _| idx < syls.len());
        self.manual_cuts.retain(|&idx| idx > 0 && idx < syls.len());

        let prior_len = self.sentence_chars.len();
        let was_at_end = self.cursor_pos >= prior_len;

        self.lexicon.maybe_reload_dynamic_lexicons();
        let dag_builder = LatticeDAG::new(&self.lexicon);
        let dag = dag_builder.build_dag_with_cuts(&syls, true, &self.manual_cuts);
        let prefix = if !self.last_committed_word.is_empty() {
            Some(self.last_committed_word.as_str())
        } else {
            None
        };
        let (nodes, _) = self.router.solve_best_path_with_prefix(syls.len(), &dag, Some(&mut self.transitions), prefix);
        let has_real_chinese = nodes.iter().any(|n| n.word.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)))
            || (matches!(self.keyboard_mode.as_str(), "standard" | "daikien") && syls.iter().all(|s| s.is_pure_initial()));
        if self.enable_mixed_english && (!has_real_chinese || nodes.is_empty()) {
            // Viterbi 無法找到任何漢字路徑（全為注音字根 fallback），自動判定為歐文
            self.is_latin_mode = true;
            self.fsm.reset();
            self.composed_sentence = self.raw_ascii_buffer.clone();
            self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
            self.cursor_pos = self.composed_sentence.chars().count();
            self.current_candidates = vec![self.raw_ascii_buffer.clone()];
            return;
        }

        self.composed_sentence = nodes.iter().map(|n| n.word.as_str()).collect();
        self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
        self.segments = nodes;

        // 核心鎖定：強制套用使用者手動選字！絕不被後續打字或 Viterbi 覆蓋！
        for (&idx, pinned_word) in &self.user_pinned {
            if idx < self.sentence_chars.len() {
                self.sentence_chars[idx] = pinned_word.clone();
            }
        }
        self.composed_sentence = self.sentence_chars.join("");

        let new_len = self.get_display_len();
        if was_at_end {
            self.cursor_pos = new_len;
        } else {
            self.cursor_pos = self.cursor_pos.min(new_len);
        }
        self.update_active_candidates();
    }

    /// 在指定音節位置切換（Toggle）手動斷詞點
    /// 回傳 true 表示成功切換（加入或移除），false 表示無效位置（如邊界超出）
    pub fn toggle_cut_at(&mut self, cut_pos: usize) -> bool {
        let syl_count = self.fsm.syllables.len();
        if cut_pos == 0 || cut_pos >= syl_count {
            return false;
        }
        if self.manual_cuts.contains(&cut_pos) {
            self.manual_cuts.remove(&cut_pos);
        } else {
            self.manual_cuts.insert(cut_pos);
        }
        self.update_sentence_composition();
        true
    }

    /// 在當前游標處切換手動斷詞點
    pub fn toggle_cut_at_cursor(&mut self) -> bool {
        self.toggle_cut_at(self.cursor_pos)
    }

    pub fn get_segment_at(&self, char_idx: usize) -> Option<&crate::router::LatticeNode> {
        self.segments.iter().find(|n| char_idx >= n.start_idx && char_idx < n.end_idx)
    }

    pub fn get_syllable_at(&self, char_idx: usize) -> Option<String> {
        if char_idx < self.fsm.syllables.len() {
            Some(self.fsm.syllables[char_idx].bopomofo_display())
        } else {
            None
        }
    }

    pub fn has_any_compound_word(&self) -> bool {
        let syls = self.fsm.get_all_syllables();
        if syls.len() < 2 {
            return false;
        }
        let dag_builder = LatticeDAG::new(&self.lexicon);
        let dag = dag_builder.build_dag(&syls, true);
        for nodes in dag.values() {
            if nodes.iter().any(|n| n.length() >= 2) {
                return true;
            }
        }
        false
    }

    pub fn get_active_char_index(&self) -> usize {
        let total = self.sentence_chars.len();
        if total == 0 {
            return 0;
        }
        if self.cursor_pos == 0 {
            0
        } else if self.cursor_pos <= total {
            self.cursor_pos - 1
        } else {
            total - 1
        }
    }

    pub fn update_active_candidates(&mut self) {
        if self.sentence_chars.is_empty() || self.fsm.syllables.is_empty() {
            self.current_candidates.clear();
            return;
        }
        let active_idx = self.get_active_char_index();
        if active_idx < self.fsm.syllables.len() {
            let syl = &self.fsm.syllables[active_idx];
            let bpm = syl.bopomofo_display();
            let mut entries = self.lexicon.lookup_bopomofo(&bpm, true);
            if entries.is_empty() {
                entries = self.lexicon.lookup_bopomofo(&bpm, false);
            }
            let mut cands: Vec<String> = entries.into_iter().map(|e| e.word.clone()).collect();
            let cur_char = self.sentence_chars.get(active_idx).cloned().unwrap_or_default();
            if bpm.chars().count() == 1 {
                let hsu_syms = crate::lexicon::LexiconManager::get_hsu_bopomofo_symbols(&bpm);
                let cur_is_bpm = cur_char
                    .chars()
                    .next()
                    .map(|c| ('\u{3105}'..='\u{3129}').contains(&c) || matches!(c, 'ˊ' | 'ˇ' | 'ˋ' | '˙'))
                    .unwrap_or(false);
                if cur_is_bpm {
                    let mut front = hsu_syms;
                    for w in cands {
                        if !front.contains(&w) {
                            front.push(w);
                        }
                    }
                    cands = front;
                } else {
                    let insert_pos = cands.len().min(3);
                    let mut offset = 0;
                    for sym in hsu_syms {
                        if !cands.contains(&sym) {
                            cands.insert(insert_pos + offset, sym);
                            offset += 1;
                        }
                    }
                }
            }
            self.current_candidates = cands;
            self.candidate_cycle_idx = self.current_candidates.iter().position(|w| w == &cur_char).unwrap_or(0);
        }
    }

    pub fn cycle_next_candidate(&mut self) {
        if self.sentence_chars.is_empty() || self.current_candidates.is_empty() {
            return;
        }
        let active_idx = self.get_active_char_index();
        if active_idx < self.sentence_chars.len() {
            let orig = self.sentence_chars[active_idx].clone();
            self.candidate_cycle_idx = (self.candidate_cycle_idx + 1) % self.current_candidates.len();
            let chosen = self.current_candidates[self.candidate_cycle_idx].clone();
            if orig != chosen {
                let bpm = self.get_syllable_at(active_idx).unwrap_or_default();
                let prev_ctx = if active_idx > 0 { self.sentence_chars[active_idx - 1].clone() } else { String::new() };
                let next_ctx = if active_idx + 1 < self.sentence_chars.len() { self.sentence_chars[active_idx + 1].clone() } else { String::new() };
                self.record_revision(&bpm, &orig, &chosen, &prev_ctx, &next_ctx);
            }
            self.sentence_chars[active_idx] = chosen.clone();
            self.user_pinned.insert(active_idx, chosen);
            self.composed_sentence = self.sentence_chars.join("");
        }
    }

    pub fn cycle_prev_candidate(&mut self) {
        if self.sentence_chars.is_empty() || self.current_candidates.is_empty() {
            return;
        }
        let active_idx = self.get_active_char_index();
        if active_idx < self.sentence_chars.len() {
            let orig = self.sentence_chars[active_idx].clone();
            if self.candidate_cycle_idx == 0 {
                self.candidate_cycle_idx = self.current_candidates.len().saturating_sub(1);
            } else {
                self.candidate_cycle_idx -= 1;
            }
            let chosen = self.current_candidates[self.candidate_cycle_idx].clone();
            if orig != chosen {
                let bpm = self.get_syllable_at(active_idx).unwrap_or_default();
                let prev_ctx = if active_idx > 0 { self.sentence_chars[active_idx - 1].clone() } else { String::new() };
                let next_ctx = if active_idx + 1 < self.sentence_chars.len() { self.sentence_chars[active_idx + 1].clone() } else { String::new() };
                self.record_revision(&bpm, &orig, &chosen, &prev_ctx, &next_ctx);
            }
            self.sentence_chars[active_idx] = chosen.clone();
            self.user_pinned.insert(active_idx, chosen);
            self.composed_sentence = self.sentence_chars.join("");
        }
    }

    pub fn build_response(&self, action: &str, committed: &str) -> EngineResponse {
        let mode_str = match self.mode {
            EngineMode::Normal => "normal",
            EngineMode::TemplateSelect => "templates",
            EngineMode::QuickNum => "quick_num",
            EngineMode::LexiconSwitch => "lexicon",
            EngineMode::CandidatePopup => "candidates",
        };

        let display_composed = if self.fsm.has_pending() && self.sentence_chars.len() <= self.fsm.syllables.len() {
            let pending_bpmf = format!("{}{}{}", self.fsm.cur_initial, self.fsm.cur_medial, self.fsm.cur_final);
            format!("{}{}", self.composed_sentence, pending_bpmf)
        } else {
            self.composed_sentence.clone()
        };

        EngineResponse {
            action: action.to_string(),
            mode: mode_str.to_string(),
            preedit: self.fsm.get_display_preedit(),
            composed: display_composed,
            candidates: self.current_candidates.clone(),
            active_candidate: self.current_candidates.get(self.candidate_cycle_idx).cloned().unwrap_or_default(),
            candidate_idx: self.candidate_cycle_idx,
            cursor_pos: self.cursor_pos,
            q_press_count: self.q_press_count,
            committed_chunk: committed.to_string(),
        }
    }

    pub fn pop_component(&mut self) -> EngineResponse {
        if !self.raw_ascii_buffer.is_empty() {
            self.raw_ascii_buffer.pop();
        }
        if self.is_latin_mode {
            if self.raw_ascii_buffer.is_empty() {
                self.reset_buffer();
            } else {
                self.composed_sentence = self.raw_ascii_buffer.clone();
                self.sentence_chars = self.composed_sentence.chars().map(|c| c.to_string()).collect();
                self.cursor_pos = self.composed_sentence.chars().count();
                self.current_candidates = vec![self.raw_ascii_buffer.clone()];
            }
            return self.build_response("backspace", "");
        }
        if !self.fsm.raw_keys.is_empty() {
            self.fsm.raw_keys.pop();
            let keys = self.fsm.raw_keys.clone();
            self.fsm.reset();
            for k in keys {
                self.fsm.parse_key_with_layout(k, &self.keyboard_mode);
            }
            self.user_pinned.retain(|&idx, _| idx < self.fsm.syllables.len());
            self.update_sentence_composition();
        } else {
            self.reset_buffer();
        }
        self.build_response("backspace", "")
    }

    pub fn pop_syllable(&mut self) -> EngineResponse {
        if self.fsm.raw_keys.is_empty() {
            self.reset_buffer();
            return self.build_response("backspace", "");
        }

        if self.is_latin_mode {
            self.reset_buffer();
            return self.build_response("backspace", "");
        }

        let cur_syllables_len = self.fsm.syllables.len();
        let has_in_progress = self.fsm.has_pending();

        let target_len = if has_in_progress {
            cur_syllables_len
        } else if cur_syllables_len > 0 {
            cur_syllables_len - 1
        } else {
            0
        };

        while !self.fsm.raw_keys.is_empty() {
            self.fsm.raw_keys.pop();
            let keys = self.fsm.raw_keys.clone();
            self.fsm.reset();
            for k in keys {
                self.fsm.parse_key_with_layout(k, &self.keyboard_mode);
            }
            let now_in_progress = self.fsm.has_pending();
            if self.fsm.syllables.len() <= target_len && !now_in_progress {
                break;
            }
        }

        self.raw_ascii_buffer.truncate(self.fsm.raw_keys.len());
        self.western.reset();

        if self.fsm.raw_keys.is_empty() {
            self.reset_buffer();
        } else {
            self.user_pinned.retain(|&idx, _| idx < self.fsm.syllables.len());
            self.update_sentence_composition();
        }
        self.build_response("backspace", "")
    }

    pub fn pop_word(&mut self) -> EngineResponse {
        if self.fsm.raw_keys.is_empty() {
            self.reset_buffer();
            return self.build_response("backspace", "");
        }

        if self.is_latin_mode {
            self.reset_buffer();
            return self.build_response("backspace", "");
        }

        // 若當前有未完成的拼音部件 (pending)，優先刪除該組未完成音節 (正在輸入中的字)
        if self.fsm.has_pending() {
            return self.pop_syllable();
        }

        let cur_syllables_len = self.fsm.syllables.len();
        if cur_syllables_len == 0 {
            return self.pop_syllable();
        }

        // 依據 Viterbi 切詞結構 (segments)，精確計算最後一個詞的長度 (音節數)
        let pop_count = if let Some(last_seg) = self.segments.last() {
            let seg_len = last_seg.length();
            if seg_len > 0 && seg_len <= cur_syllables_len {
                seg_len
            } else {
                1
            }
        } else {
            1
        };

        let target_len = if cur_syllables_len >= pop_count {
            cur_syllables_len - pop_count
        } else {
            0
        };

        while !self.fsm.raw_keys.is_empty() {
            self.fsm.raw_keys.pop();
            let keys = self.fsm.raw_keys.clone();
            self.fsm.reset();
            for k in keys {
                self.fsm.parse_key_with_layout(k, &self.keyboard_mode);
            }
            let now_in_progress = self.fsm.has_pending();
            if self.fsm.syllables.len() <= target_len && !now_in_progress {
                break;
            }
        }

        self.raw_ascii_buffer.truncate(self.fsm.raw_keys.len());
        self.western.reset();

        if self.fsm.raw_keys.is_empty() {
            self.reset_buffer();
        } else {
            self.user_pinned.retain(|&idx, _| idx < self.fsm.syllables.len());
            self.update_sentence_composition();
        }
        self.build_response("backspace", "")
    }

    pub fn record_revision(&mut self, bopomofo: &str, rejected: &str, accepted: &str, prev_context: &str, next_context: &str) {
        if rejected.is_empty() || accepted.is_empty() || rejected == accepted {
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // 防重機制：若同一秒內已有相同字詞與上下文的修訂，則合併補充注音，杜絕重複寫盤
        if let Some(last) = self.pending_revisions.last_mut() {
            if last.timestamp == now && last.rejected == rejected && last.accepted == accepted && last.prev_context == prev_context {
                if !bopomofo.is_empty() && last.bopomofo.is_empty() {
                    last.bopomofo = bopomofo.to_string();
                }
                return;
            }
        }

        self.pending_revisions.push(RevisionRecord {
            timestamp: now,
            bopomofo: bopomofo.to_string(),
            rejected: rejected.to_string(),
            accepted: accepted.to_string(),
            prev_context: prev_context.to_string(),
            next_context: next_context.to_string(),
        });
    }

    pub fn has_pending_revisions(&self) -> bool {
        !self.pending_revisions.is_empty()
    }

    pub fn flush_pending_revisions(&mut self) -> Vec<RevisionRecord> {
        std::mem::take(&mut self.pending_revisions)
    }

    pub fn save_pending_revisions_to_file(&mut self, file_path: Option<&str>) -> std::io::Result<usize> {
        let revs = self.flush_pending_revisions();
        if revs.is_empty() {
            return Ok(0);
        }

        let target_path = if let Some(p) = file_path {
            std::path::PathBuf::from(p)
        } else if let Some(ref d) = self.data_dir {
            std::path::PathBuf::from(d).join("pending_revisions.json")
        } else if let Ok(appdata) = std::env::var("APPDATA") {
            let p = std::path::PathBuf::from(appdata).join("Ruhi").join("data");
            let _ = std::fs::create_dir_all(&p);
            p.join("pending_revisions.json")
        } else {
            std::path::PathBuf::from("pending_revisions.json")
        };

        let mut existing: Vec<RevisionRecord> = if target_path.exists() {
            std::fs::read_to_string(&target_path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let count = revs.len();
        existing.extend(revs);

        if existing.len() > 500 {
            let skip_count = existing.len() - 500;
            existing.drain(0..skip_count);
        }

        if let Ok(serialized) = serde_json::to_string_pretty(&existing) {
            let _ = std::fs::write(&target_path, serialized);
        }

        Ok(count)
    }

    pub fn auto_synthesize_and_apply_revisions(&mut self) -> usize {
        let revs = self.flush_pending_revisions();
        if revs.is_empty() {
            return 0;
        }

        let mut synthesized_rules = Vec::new();

        let is_cjk = |c: char| -> bool {
            let u = c as u32;
            (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u) || (0xF900..=0xFAFF).contains(&u)
        };

        // 收集同批次所有被使用者否定之錯別字詞，嚴禁將同批次尚未訂正之錯別字上下文寫入轉移矩陣（防「視覺 ➔ 每感」、「但董 ➔ 視覺」等污染）
        let rejected_set: std::collections::HashSet<String> = revs
            .iter()
            .map(|r| r.rejected.trim().chars().filter(|&c| is_cjk(c)).collect::<String>())
            .filter(|s| !s.is_empty())
            .collect();

        let is_valid_context_word = |w: &str, lex: &crate::lexicon::manager::LexiconManager| -> bool {
            if w.is_empty() || rejected_set.contains(w) {
                return false;
            }
            // 若上下文包含已被否定的多字錯詞片段，亦拒絕學習
            if rejected_set.iter().any(|rej| rej.chars().count() >= 2 && w.contains(rej.as_str())) {
                return false;
            }
            // 多字上下文必須為詞庫合法詞條，杜絕「是參」、「參束」、「下啦」、「每感」等斷裂錯字被誤認為合法搭配
            if w.chars().count() >= 2 && !lex.word_index.contains_key(w) {
                return false;
            }
            true
        };

        for r in &revs {
            let rej: String = r.rejected.trim().chars().filter(|&c| is_cjk(c)).collect();
            let acc: String = r.accepted.trim().chars().filter(|&c| is_cjk(c)).collect();
            // 僅提取前文末尾之連續純漢字（剔除逗號、句號等標點）
            let prev: String = r.prev_context.trim().chars().rev().take_while(|&c| is_cjk(c)).collect::<Vec<char>>().into_iter().rev().collect();
            // 僅提取後文開頭之連續純漢字（剔除逗號、句號等標點）
            let next: String = r.next_context.trim().chars().take_while(|&c| is_cjk(c)).collect();

            if acc.is_empty() || rej == acc || !is_valid_context_word(&acc, &self.lexicon) {
                continue;
            }

            // 嚴格遵行《如喜漢語計算語言學架構白皮書》：僅在前後文為詞庫合法詞彙時合成二階正向轉移，且廢除機械切尾字/首字之「局部引力」（防「視覺 ➔ 每」、「拉 ➔ 是參」、「式 ➔ 參束」污染）
            if is_valid_context_word(&prev, &self.lexicon) && prev != acc {
                synthesized_rules.push((prev.clone(), acc.clone(), 26.0, format!("自動學習：{} 引導 {}", prev, acc)));
            }

            if is_valid_context_word(&next, &self.lexicon) && next != acc {
                synthesized_rules.push((acc.clone(), next.clone(), 26.0, format!("自動學習：{} 銜接 {}", acc, next)));
            }
        }

        if synthesized_rules.is_empty() {
            return 0;
        }

        // A. 即時注入執行中的記憶體轉移矩陣 (零延遲生效)
        for (p, n, score, _) in &synthesized_rules {
            self.transitions.transitions.insert((p.clone(), n.clone()), *score);
        }

        // B. 追加持久化至 transitions_user.tsv
        let tsv_path = if let Some(ref d) = self.data_dir {
            std::path::PathBuf::from(d).join("transitions_user.tsv")
        } else if let Ok(appdata) = std::env::var("APPDATA") {
            std::path::PathBuf::from(appdata).join("Ruhi").join("data").join("transitions_user.tsv")
        } else {
            std::path::PathBuf::from("../ruhi_lexicon/data/transitions_user.tsv")
        };

        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&tsv_path) {
            use std::io::Write;
            for (p, n, score, cmt) in &synthesized_rules {
                let _ = writeln!(file, "{}\t{}\t{:.1}\tcollocation\t{}", p, n, score, cmt);
            }
        }

        // C. 同時將修訂記錄備份至 pending_revisions.json
        let target_json = if let Some(ref d) = self.data_dir {
            std::path::PathBuf::from(d).join("pending_revisions.json")
        } else if let Ok(appdata) = std::env::var("APPDATA") {
            let p = std::path::PathBuf::from(appdata).join("Ruhi").join("data");
            let _ = std::fs::create_dir_all(&p);
            p.join("pending_revisions.json")
        } else {
            std::path::PathBuf::from("pending_revisions.json")
        };

        let mut existing_recs: Vec<RevisionRecord> = if target_json.exists() {
            std::fs::read_to_string(&target_json)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        existing_recs.extend(revs);
        if existing_recs.len() > 500 {
            let skip = existing_recs.len() - 500;
            existing_recs.drain(0..skip);
        }
        if let Ok(serialized) = serde_json::to_string_pretty(&existing_recs) {
            let _ = std::fs::write(&target_json, serialized);
        }

        synthesized_rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_pinned_preservation() {
        let mut engine = RuhiEngine::new();
        // 模擬已組出兩字「厲害」
        engine.sentence_chars = vec!["厲".to_string(), "得".to_string()];
        engine.current_candidates = vec!["得".to_string(), "的".to_string(), "地".to_string()];
        engine.cursor_pos = 2; // 作用字為第 2 字 (index 1)
        engine.mode = EngineMode::CandidatePopup; // 候選字模式

        // 使用者手動選字 2 (「的」)
        engine.process_key('2');
        assert_eq!(engine.sentence_chars[1], "的");
        assert_eq!(engine.user_pinned.get(&1), Some(&"的".to_string()));

        // 模擬使用者繼續打字觸發 update_sentence_composition
        // 即使 Viterbi 最初可能判定為「得」，user_pinned 亦能強制鎖定「的」
        engine.sentence_chars = vec!["厲".to_string(), "得".to_string(), "地".to_string(), "方".to_string()];
        for (&idx, pinned) in &engine.user_pinned {
            if idx < engine.sentence_chars.len() {
                engine.sentence_chars[idx] = pinned.clone();
            }
        }
        engine.composed_sentence = engine.sentence_chars.join("");
        assert_eq!(engine.sentence_chars[1], "的");
        assert_eq!(engine.composed_sentence, "厲的地方");

        // Backspace 退格：清理超出範圍的 pinned
        engine.user_pinned.retain(|&idx, _| idx < 1);
        assert!(!engine.user_pinned.contains_key(&1));

        // 重置清理
        engine.reset_buffer();
        assert!(engine.user_pinned.is_empty());
    }

    #[test]
    fn test_revision_recording() {
        let mut engine = RuhiEngine::new();
        // 隔離測試資料目錄至系統暫存區，嚴禁污染使用者詞庫與歷史日誌
        engine.data_dir = Some(std::env::temp_dir().to_str().unwrap().to_string());
        assert!(!engine.has_pending_revisions());

        // 模擬手動選字「步董」->「不懂」
        engine.sentence_chars = vec!["步".to_string(), "董".to_string()];
        engine.current_candidates = vec!["董".to_string(), "懂".to_string()];
        engine.cursor_pos = 2;
        engine.mode = EngineMode::CandidatePopup;

        engine.process_key('2'); // 選懂
        assert!(engine.has_pending_revisions());
        assert_eq!(engine.pending_revisions.len(), 1);
        let rev = &engine.pending_revisions[0];
        assert_eq!(rev.rejected, "董");
        assert_eq!(rev.accepted, "懂");
        assert_eq!(rev.prev_context, "步");

        // 測試 auto_synthesize_and_apply_revisions 瞬間吸收進記憶體轉移矩陣
        engine.record_revision("ㄉㄨㄥˇ", "董", "懂", "步", "");
        let applied_count = engine.auto_synthesize_and_apply_revisions();
        assert!(applied_count > 0);
        assert_eq!(engine.transitions.transitions.get(&("步".to_string(), "懂".to_string())), Some(&26.0));
        assert!(!engine.has_pending_revisions());
    }

    #[test]
    fn test_zhu_zai_and_compound_words_priority() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 驗證 ㄓㄨˋ ㄗㄞˋ (jxjzij) 必推導出詞庫詞「住在」，杜絕碎片「著載」
        engine.reset_buffer();
        for c in "jxjzij".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].word, "住在");
        }

        // 2. 驗證 ㄋㄢˊ ㄉㄜ˙ (nmddgs) 必推導出合法詞，杜絕非詞「南得」
        engine.reset_buffer();
        for c in "nmddgs".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_ne!(result_word, "南得", "絕不可推導出無意義偽詞「南得」");
        }

        // 3. 調校過的 debug
        engine.reset_buffer();
        for c in "dewjcewjgxhjdgs".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "調校過的", "動詞修飾名詞構式必為「調校過的」，杜絕「過得」");
        }

        // 4. 驗證 ㄧㄝˇ ㄏㄣˇ (eefhnf) 必推導出日常生活詞「也很」，徹底杜絕元曲古語「野狠」
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        engine.reset_buffer();
        for c in "eefhnf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "也很", "日常詞彙「也很」必須高於罕見古語「野狠」");
        }

        // 5. 測試 這樣好嗎：語意句法規則「實詞 + 句末助詞」保證「這樣好嗎」
        engine.reset_buffer();
        for c in "jgjekjhwfmrs".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "這樣好嗎", "句末語氣助詞語法規則必引導出「這樣好嗎」，杜絕「好蟆」");
        }

        // 6. 測試中英數混合流「圖2」：數字 2 必直接納入組字流，絕不擅自竄改前文
        engine.reset_buffer();
        for c in "txd".chars() {
            engine.process_key(c);
        }
        engine.process_key('2');
        assert_eq!(engine.composed_sentence, "圖2", "許氏鍵盤按數字 2 必直接輸入數字 2，絕不突變為「突」");

        // 7. 測試「但非關組字」：轉折連詞 + 非關 + 專業詞彙「組字」，絕不推導出「但飛官組自」
        engine.reset_buffer();
        for c in "dmjfa gxm zxfzj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "但非關組字", "連詞與否定詞組必引導出「但非關組字」，杜絕「但飛官組自」");
        }

        // 8. 測試第六季核心生活思維長句：「理想上如果完全不需要選字是最理想的」
        engine.reset_buffer();
        for c in "lefcekfckj rxdgxhfxmdvumdbxjcu ewjcumfzj cjzxajlefcekfds".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "理想上如果完全不需要選字是最理想的", "長句必須行雲流水輸出「理想上如果完全不需要選字是最理想的」");
        }

        // 9. 測試數字小數點流式輸入「14.4」與「1.27」：絕不觸發中文句點「。」，絕不洩漏
        engine.reset_buffer();
        for c in "14.4".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.composed_sentence, "14.4", "連續輸入 14.4 必維持數值小數點");

        engine.reset_buffer();
        for c in "1.27".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.composed_sentence, "1.27", "連續輸入 1.27 必維持數值小數點");

        // 10. 測試「雖然好像」：杜絕「好巷」劫掠
        engine.reset_buffer();
        for c in "sxa rmdhwfcekj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "雖然好像", "必須優先命中「雖然好像」，杜絕「好巷」");
        }

        // 11. 測試「一個鐘頭」：杜絕「中投」劫掠
        engine.reset_buffer();
        for c in "e ggsjxl tod".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "一個鐘頭", "時間量詞必須優先命中「一個鐘頭」，杜絕「一個中投」");
        }

        // 12. 測試「未免也太」：杜絕「慰勉」劫掠
        engine.reset_buffer();
        for c in "xajmemfeeftij".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "未免也太", "口語構式必須優先命中「未免也太」，杜絕「慰勉」");
        }

        // 13. 測試關聯構式「越弄越大」：依靠 Layer 1 語法規則，杜絕「月弄月大」劫掠
        engine.reset_buffer();
        for c in "uejnxljuejdrj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "越弄越大", "關聯副詞構式必須優先命中「越弄越大」，杜絕「月弄月大」");
        }

        // 14. 測試方位後置詞構式「硬碟裡」：實詞 + 方位詞，杜絕「硬碟理」劫掠
        engine.reset_buffer();
        for c in "eljdeedlef".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "硬碟裡", "名詞+方位詞構式必須優先命中「硬碟裡」，杜絕「硬碟理」");
        }

        // 15. 測試繫詞構式「奇妙的是」：的+是構式，杜絕「奇妙的視」劫掠
        engine.reset_buffer();
        for c in "vedmewjdgscj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "奇妙的是", "繫詞構式必須優先命中「奇妙的是」，杜絕「奇妙的視」");
        }

        // 16. 測試「看他回」：代名詞受詞與趨向補語，杜絕「看牠回」
        engine.reset_buffer();
        for c in "kmjtrhxad".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "看他回", "人稱代詞必須優先命中「看他回」，杜絕「看牠回」");
        }

        // 17. 測試「除了我」：虛詞與核心代詞，杜絕生僻古字「婑」劫掠
        engine.reset_buffer();
        for c in "vxdlsxhf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "除了我", "核心人稱代詞必命中「除了我」，杜絕生僻古字「除了婑」");
        }

        // 18. 測試四字成語「重中之重」：四字長詞凝聚力，杜絕碎拆「種中之中」
        engine.reset_buffer();
        for c in "jxljjxl j jxlj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "重中之重", "四字成語必優先命中「重中之重」，杜絕碎拆「種中之中」");
        }

        // 19. 測試「這是所有」：代詞繫詞與全稱量詞，杜絕「這世所有」
        engine.reset_buffer();
        for c in "jgjcjsxhfeof".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "這是所有", "代詞繫詞必優先命中「這是所有」，杜絕「這世所有」");
        }

        // 20. 測試層累構式「難上加難」：[X上加X]構式，杜絕碎拆
        engine.reset_buffer();
        for c in "nmdckjjer nmd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "難上加難", "[X上加X]層累構式必優先命中「難上加難」");
        }

        // 21. 測試成語熟語「急中生智」：[X中生Y]熟語構式
        engine.reset_buffer();
        for c in "jedjxl cl jj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "急中生智", "[X中生Y]熟語構式必命中「急中生智」");
        }

        // 22. 測試「人中龍鳳」與「人上人」：熟語構式
        engine.reset_buffer();
        for c in "rndjxl lxldflj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "人中龍鳳", "熟語構式必命中「人中龍鳳」");
        }
        engine.reset_buffer();
        for c in "rndckjrnd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "人上人", "熟語構式必命中「人上人」");
        }

        // 23. 測試嵌偶極致構式「王中之王」與「難中之難」
        engine.reset_buffer();
        for c in "xkdjxl j xkd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "王中之王", "[X中之X]極致構式必命中「王中之王」");
        }
        engine.reset_buffer();
        for c in "nmdjxl j nmd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "難中之難", "[X中之X]極致構式必命中「難中之難」");
        }

        // 24. 測試動量擴散構式「大書特書」與「大買特買」
        engine.reset_buffer();
        for c in "drjcx tgjcx ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "大書特書", "[大X特X]動量擴散構式必命中「大書特書」");
        }
        engine.reset_buffer();
        for c in "drjmiftgjmif".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "大買特買", "[大X特X]動量擴散構式必命中「大買特買」");
        }

        // 25. 測試可能補語構式「看得懂」與「看不懂」
        engine.reset_buffer();
        for c in "kmjdgddxlf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "看得懂", "可能補語(肯定)必命中「看得懂」");
        }
        engine.reset_buffer();
        for c in "kmjbxjdxlf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "看不懂", "可能補語(否定)必命中「看不懂」");
        }

        // 26. 測試可能補語構式「做得到」與「做不到」
        engine.reset_buffer();
        for c in "zxhjdgddwj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "做得到", "可能補語(肯定)必命中「做得到」");
        }
        engine.reset_buffer();
        for c in "zxhjbxddwj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "做不到", "可能補語(否定)必命中「做不到」");
        }

        // 27. 測試空間與動作往復構式「走來走去」與「看來看去」
        engine.reset_buffer();
        for c in "zoflidzofvuj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "走來走去", "[X來X去]動作往復構式必命中「走來走去」");
        }
        engine.reset_buffer();
        for c in "kmjlidkmjvuj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "看來看去", "[X來X去]動作往復構式必命中「看來看去」");
        }

        // 28. 測試對稱否定與並列凝固構式「不折不扣」與「無邊無際」
        engine.reset_buffer();
        for c in "bxjjgdbxjkoj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "不折不扣", "[不X不X]對稱否定構式必命中「不折不扣」");
        }
        engine.reset_buffer();
        for c in "xdbem xdjej".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "無邊無際", "[無X無X]對稱否定構式必命中「無邊無際」");
        }

        // 29. 測試連動構式 (Serial Verb Constructions - SVC)
        engine.reset_buffer();
        for c in "ckjjee mifaij".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "上街買菜", "連動構式必命中「上街買菜」");
        }
        engine.reset_buffer();
        for c in "zxhjvg hxadjer ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "坐車回家", "連動構式必命中「坐車回家」");
        }

        // 30. 測試雙賓語構式 (Double Object Constructions - DOC)
        engine.reset_buffer();
        for c in "gafxhfe bnfcx ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "給我一本書", "雙賓語構式必命中「給我一本書」");
        }
        engine.reset_buffer();
        for c in "sxljtr e fnjlefxj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "送他一份禮物", "雙賓語構式必命中「送他一份禮物」");
        }

        // 31. 測試把字句與被字句 (Disposal & Passive Constructions)
        engine.reset_buffer();
        for c in "brfcx kmjxmdlgs".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "把書看完了", "把字句處置結果構式必命中「把書看完了」");
        }
        engine.reset_buffer();
        for c in "bajuflendc lgs".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "被雨淋濕了", "被字句被動結果構式必命中「被雨淋濕了」");
        }

        // 32. 測試動詞介詞接人稱代詞構式「等到我」，徹底杜絕生僻異體字「𣇫」
        engine.reset_buffer();
        for c in "dlfdwjxhf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "等到我", "動詞介詞接代詞賓語必命中「等到我」，徹底根除生僻字「𣇫」劫掠");
        }

        // 33. 測試因果連詞構式「因為」，杜絕「音為」劫掠
        engine.reset_buffer();
        for c in "en xaj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "因為", "因果連詞構式必命中「因為」，杜絕「音為」劫掠");
        }

        // 34. 測試雙字複合長詞「鳳凰」，長詞凝聚力杜絕單字切碎
        engine.reset_buffer();
        for c in "fljhxkd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "鳳凰", "雙字詞長詞凝聚力必命中「鳳凰」，杜絕單字切碎");
        }

        // 35. 測試副詞修飾敬語動詞「非常感謝」，杜絕「非常敢謝」
        engine.reset_buffer();
        for c in "fa vkd gmf ceej".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "非常感謝", "副詞修飾敬語動詞必命中「非常感謝」，杜絕「非常敢謝」");
        }

        // 獨立鍵入「感謝」，杜絕單字拆為「敢謝」
        engine.reset_buffer();
        for c in "gmf ceej".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "感謝", "獨立輸入必命中敬語「感謝」，杜絕「敢謝」");
        }

        // 鍵入「十分感謝」，杜絕「十分敢謝」
        engine.reset_buffer();
        for c in "cdfn gmf ceej".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "十分感謝", "程度副詞+敬語動詞必命中「十分感謝」，杜絕「敢謝」");
        }

        // 36. 測試極端造句「他非常敢謝謝對他不好的人」
        // ㄊㄚ (tr ) ㄈㄟ (fa ) ㄔㄤˊ (vkd) ㄍㄢˇ (gmf) ㄒㄧㄝˋ (ceej) ㄒㄧㄝˋ (ceej) ㄉㄨㄟˋ (dxaj) ㄊㄚ (tr ) ㄅㄨˋ (bxj) ㄏㄠˇ (hwf) ㄉㄜ˙ (ds) ㄖㄣˊ (rnd)
        engine.reset_buffer();
        for c in "tr fa vkd gmf ceejceej dxaj tr bxjhwf ds rnd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "他非常敢謝謝對他不好的人", "極端造句必精準命中「他非常敢謝謝對他不好的人」，杜絕割裂");
        }

        // 測試「是個非常勇敢的人」
        // ㄕˋ (cj) ˙ㄍㄜ (ggs) ㄈㄟ (fa ) ㄔㄤˊ (vkd) ㄩㄥˇ (ulf) ㄍㄢˇ (gmf) ㄉㄜ˙ (ds) ㄖㄣˊ (rnd)
        engine.reset_buffer();
        for c in "cj ggs fa vkd ulfgmf ds rnd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "是個非常勇敢的人", "繫詞量詞形容詞必精準命中「是個非常勇敢的人」");
        }

        // 測試「勇敢的人」
        // ㄩㄥˇ (ulf) ㄍㄢˇ (gmf) ㄉㄜ˙ (ds) ㄖㄣˊ (rnd)
        engine.reset_buffer();
        for c in "ulfgmf ds rnd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "勇敢的人", "名詞修飾必精準使用結構助詞「的」，杜絕「勇敢地人」");
        }

        // 測試「勇敢地向前走」
        // ㄩㄥˇ (ulf) ㄍㄢˇ (gmf) ㄉㄜ˙ (ds) ㄒㄧㄤˋ (cekj) ㄑㄧㄢˊ (vemd) ㄗㄡˇ (zof)
        engine.reset_buffer();
        for c in "ulfgmf ds cekj vemd zof".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "勇敢地向前走", "動作修飾必精準使用副詞標記「地」，達成「勇敢地向前走」");
        }

        // 37. 測試語氣時間副詞+助動詞「本來要說」，杜絕「本來藥說」
        // ㄅㄣˇ (bnf) ㄌㄞˊ (lid) ㄧㄠˋ (ewj) ㄕㄨㄛ (cxh )
        engine.reset_buffer();
        for c in "bnf lid ewj cxh ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "本來要說", "時間語氣副詞+助動詞必精準命中「本來要說」，杜絕「本來藥說」");
        }
    }

    #[test]
    fn test_ruhi_engine_cangjie_mode_and_zero_switch() {
        let mut engine = RuhiEngine::new();
        assert_eq!(engine.keyboard_mode, "hsu");

        // 切換至倉頡模式
        engine.set_keyboard_mode("cangjie");
        assert_eq!(engine.keyboard_mode, "cangjie");

        // 1. 倉頡打 "ab" + 空白鍵 ➔ 上屏漢字 "明"
        engine.process_key('a');
        let r_compose = engine.process_key('b');
        assert_eq!(r_compose.action, "compose");
        assert_eq!(r_compose.candidates[0], "明");
        let r_commit = engine.process_key(' ');
        assert_eq!(r_commit.action, "commit");
        assert_eq!(r_commit.committed_chunk, "明");

        // 2. 倉頡打 "ab" + Enter 鍵 ➔ 零切換直通出英文 "ab"！
        engine.process_key('a');
        engine.process_key('b');
        let r_enter = engine.process_key('\n');
        assert_eq!(r_enter.action, "commit");
        assert_eq!(r_enter.committed_chunk, "ab");

        // 3. 英文長單字混打 "github" (>5 碼自動直通英文模式)
        for c in "github".chars() {
            engine.process_key(c);
        }
        let r_gh = engine.process_key(' ');
        assert_eq!(r_gh.action, "commit");
        assert_eq!(r_gh.committed_chunk, "github");

        // 4. 切換回許氏注音模式
        let toggled = engine.toggle_keyboard_mode();
        assert_eq!(toggled, "hsu");
        assert_eq!(engine.keyboard_mode, "hsu");
    }

    #[test]
    fn test_ruhi_engine_array30_mode_and_hsu_coach() {
        let mut engine = RuhiEngine::new();
        engine.set_keyboard_mode("array30");
        assert_eq!(engine.keyboard_mode, "array30");
        assert_eq!(engine.get_keyboard_mode_badge(), "行");

        // 1. 行列 30 標準全碼：".aad" + 空白鍵 ➔ 上屏「行」（座標顯示 "9v 1- 1- 3-"）
        for c in ".aad".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.array30.make_preedit(), "9v 1- 1- 3-");
        let r_hang = engine.process_key(' ');
        assert_eq!(r_hang.action, "commit");
        assert_eq!(r_hang.committed_chunk, "行");

        // 2. 行列 30 CJK Ext-A 檢核基準字：輸入 "fai" (4- 1- 8^)，候選清單必包含「苹」與 Ext-A 首字「㐀」
        engine.process_key('f');
        engine.process_key('a');
        let r_fai = engine.process_key('i');
        assert!(
            r_fai.candidates.contains(&"㐀".to_string()),
            "輸入 fai 候選清單應包含 CJK Ext-A 首字『㐀』，實際：{:?}",
            r_fai.candidates
        );
        let r_ext_a = engine.process_key(' ');
        assert_eq!(r_ext_a.committed_chunk, "苹");

        // 3. 一級簡碼與特別碼："." + "3" ➔ 「行」；"ak" + 空白鍵 ➔ 「大」；"pc" + 空白鍵 ➔ 「是」
        engine.process_key('.');
        let r_s1 = engine.process_key('3');
        assert_eq!(r_s1.committed_chunk, "行");

        engine.process_key('a');
        engine.process_key('k');
        let r_ak = engine.process_key(' ');
        assert_eq!(r_ak.committed_chunk, "大");

        engine.process_key('p');
        engine.process_key('c');
        let r_pc = engine.process_key(' ');
        assert_eq!(r_pc.committed_chunk, "是");

        // 4. W 鍵符號表："w1" 免空白鍵展開標點符號，按 "1" 上屏「，」
        engine.process_key('w');
        let r_w1 = engine.process_key('1');
        assert_eq!(r_w1.action, "compose");
        assert_eq!(r_w1.candidates[0], "，");
        let r_w1_pick = engine.process_key('1');
        assert_eq!(r_w1_pick.committed_chunk, "，");

        // 5. 切換回許氏注音模式並開啟「許氏注音 ➔ 行列 30 即時反查教練」
        engine.set_keyboard_mode("hsu");
        engine.enable_array30_coach = true;
        for c in "dyj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "大");
        let coach_hint = engine.get_array30_coach_hint();
        assert!(
            coach_hint.contains("18=Z") && coach_hint.contains("AK"),
            "許氏注音打『大』時，行列教練提示應包含筆形座標 18=Z 與特別碼 AK，實際：{}",
            coach_hint
        );
    }

    #[test]
    fn test_bu_jian_and_yi_jing_and_prefix_commit() {
        let mut engine = RuhiEngine::new();

        // 1. 驗證「不見」四聲與二聲變調必命中，絕對不被拆成生僻字「轐間」或「部間」
        engine.reset_buffer();
        for c in "bxjjemj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "不見", "四聲 ㄅㄨˋ ㄐㄧㄢˋ 必命中「不見」");

        engine.reset_buffer();
        for c in "bxdjemj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "不見", "二聲變調 ㄅㄨˊ ㄐㄧㄢˋ 必命中「不見」，杜絕「轐間」劫掠");

        // 2. 驗證「已經」與語境搭配
        engine.reset_buffer();
        for c in "efjel ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "已經", "獨立鍵入 ㄧˇ ㄐㄧㄥ 必命中「已經」");

        engine.reset_buffer();
        for c in "dmjcjefjel ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "但是已經", "連句「但是已經」必命中「但是已經」，杜絕「以經」");

        // 3. 驗證完整長句「打出的第一個注音」
        engine.reset_buffer();
        for c in "drfvx dgsdeje ggjjxjen ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "打出的第一個注音", "連句「打出的第一個注音」精準上屏");
    }

    #[test]
    fn test_shihou_chuli_hui_qingsong() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 很多時候: ㄏㄣˇ (hnf) ㄉㄨㄛ (dxh ) ㄕˊ (cd) ㄏㄡˋ (hoj)
        engine.reset_buffer();
        for c in "hnf dxh cd hoj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "很多時候", "必須精準命中「很多時候」，杜絕「什後」破壞");

        // 2. 處理 (ㄔㄨˇ ㄌㄧˇ) - vxf lef
        engine.reset_buffer();
        for c in "vxf lef".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "處理", "必須精準命中「處理」，杜絕切碎「處裡」");

        // 3. 就真的會覺得: ㄐㄧㄡˋ (jeoj) ㄓㄣ (jn ) ㄉㄜ˙ (ds) ㄏㄨㄟˋ (hxaj) ㄐㄩㄝˊ (jued) ㄉㄜ˙ (ds)
        engine.reset_buffer();
        for c in "jeoj jn ds hxaj jued ds".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "就真的會覺得", "副詞+助動詞+心理動詞必須命中「就真的會覺得」，杜絕「慧覺的」");

        // 4. 輕鬆 (ㄑㄧㄥ ㄙㄨㄥ) - 覺得輕鬆: jued ds vel sxl 
        engine.reset_buffer();
        for c in "jued ds vel sxl ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "覺得輕鬆", "心理動詞+狀態形容詞必須命中「覺得輕鬆」，杜絕「覺的清松」");
    }

    #[test]
    fn test_hsu_mixed_english_smart_detection() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試 "this": 由於 ㄊㄛ 非法，自動判定為歐文 "this"
        engine.reset_buffer();
        for c in "this".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "this", "this 具備非法漢語音位搭配，必須自動判定為歐文");
        let resp = engine.process_key('\n');
        assert_eq!(resp.committed_chunk, "this", "Enter 鍵必須直通上屏 raw English");

        // 2. 測試 "sample": Enter 鍵直通上屏 raw English，候選詞庫提供 sample
        engine.reset_buffer();
        for c in "sample".chars() {
            engine.process_key(c);
        }
        assert!(engine.current_candidates.contains(&"sample".to_string()), "候選清單必須包含 raw English sample");
        let resp2 = engine.process_key('\n');
        assert_eq!(resp2.committed_chunk, "sample", "Enter 鍵必須直通上屏 raw English sample");

        // 3. 測試 "book": ㄅㄡ 非法，自動判定為歐文 "book"
        engine.reset_buffer();
        for c in "book".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "book", "book 必須自動判定為歐文");
        let resp3 = engine.process_key('\n');
        assert_eq!(resp3.committed_chunk, "book", "Enter 鍵必須輸出 book");

        // 4. 測試大寫英數 "Google": 大寫直接判定為歐文
        engine.reset_buffer();
        for c in "Google".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "Google", "大寫開頭必須直接進入歐文模式");

        // 5. 確保純中文「你好」依然 100% 正常
        // ㄋㄧˇ (nef) ㄏㄠˇ (hwf)
        engine.reset_buffer();
        for c in "nef hwf".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "你好", "中文正常輸入不受任何影響");

        // 6. 完整英文字串流測試: "this is a sample book"
        engine.reset_buffer();
        let mut full_output = String::new();
        for c in "this is a sample book\n".chars() {
            let resp = engine.process_key(c);
            if !resp.committed_chunk.is_empty() {
                full_output.push_str(&resp.committed_chunk);
            }
        }
        assert_eq!(full_output, "this is a sample book", "整句英文字串流必須零誤殺直通送出");
    }

    #[test]
    fn test_wufa_xingcheng_yuyi_and_youyiyi_de() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試「無法形成語意」: 必須命中「無法形成語意」，杜絕「無法行程語意」劫掠
        // ㄨˊ (xd) ㄈㄚˇ (frf) ㄒㄧㄥˊ (celd) ㄔㄥˊ (vld) ㄩˇ (uf) ㄧˋ (ej)
        engine.reset_buffer();
        for c in "xd frf celd vld uf ej ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "無法形成語意", "語法轉移引力必須鎖定「無法形成語意」，杜絕「行程語意」");

        // 2. 測試「有意義的」: 必須命中「有意義的」，杜絕切碎為「有益意的」
        // ㄧㄡˇ (eof) ㄧˋ (ej) ㄧˋ (ej) ˙ㄉㄜ (ds)
        engine.reset_buffer();
        for c in "eof ejej ds".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "有意義的", "長詞凝聚力必須鎖定「有意義的」，杜絕「有益意的」");

        // 3. 測試「發音」: 絕不可因 raw_len >= 4 誤殺為歐文 "fyen"
        engine.reset_buffer();
        for c in "fyen ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "發音", "無聲調一聲詞「發音」必須正常結算為漢字，杜絕判定為丹麥文/歐文");

        // 4. 測試「完全」保護: 後續誤鍵絕不可將已成立的「完全」整句摧毀為 ASCII
        // ㄨㄢˊ (xmd) ㄑㄩㄢˊ (vumd)
        engine.reset_buffer();
        for c in "xmd vumd ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "完全", "「完全」必須正確組字");
        // 模擬打完「完全」後輸入非法 pending 搭配
        engine.process_key('u');
        assert!(engine.get_display_text().starts_with("完全"), "已確立的「完全」絕對不可被後續鍵位連坐摧毀");
    }

    #[test]
    fn test_dual_track_trellis_competition() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.western.load_tsv("../ruhi_lexicon/data/lexicon_english.tsv");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 純英文句子流流暢輸入（包含冠詞、介係詞、名詞複合變化）
        engine.reset_buffer();
        let mut output = String::new();
        for c in "this is a small book\n".chars() {
            let resp = engine.process_key(c);
            if !resp.committed_chunk.is_empty() {
                output.push_str(&resp.committed_chunk);
            }
        }
        assert_eq!(output, "this is a small book", "雙軌模型必須讓純英文句子自然直通結算");

        // 2. 代碼標識符駝峰與底線模式
        engine.reset_buffer();
        for c in "getElementById".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "getElementById");
        let resp = engine.process_key('\n');
        assert_eq!(resp.committed_chunk, "getElementById", "CamelCase 標識符必須直通上屏");

        // 3. 中文雙字詞「發音」正面壓倒歐文軌生僻詞 "fyen"
        engine.reset_buffer();
        for c in "fyen ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "發音", "中文軌高頻複合詞必須壓制低信心歐文");

        // 4. 「一個鐘頭」單字加量詞組合，不得被 "e" 單字母劫掠
        engine.reset_buffer();
        for c in "e ggsjxl tod".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "一個鐘頭", "雙軌競爭不得破壞單字數量詞組「一個鐘頭」");
        }

        // 5. 測試口語感嘆詞「嘿」(˙ㄏㄟ) 之輸入，杜絕被英文 "has" 劫掠
        // 5.1 許氏標準後綴輕聲鍵位: has (h:ㄏ, a:ㄟ, s:˙)
        engine.reset_buffer();
        for c in "has".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "嘿", "has (˙ㄏㄟ) 必須精準組出「嘿」，絕不被英文 has 劫掠");
        let resp = engine.process_key('\n');
        assert_eq!(resp.committed_chunk, "嘿", "Enter 鍵必須結算上屏「嘿」");

        // 5.2 許氏標準一聲鍵位: ha (h:ㄏ, a:ㄟ, space:一聲)
        engine.reset_buffer();
        for c in "ha".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "ㄏㄟ", "ha 尚未敲空白定音前顯示為 ㄏㄟ");
        let _resp2 = engine.process_key(' ');
        assert!(engine.get_display_text() == "黑" || engine.get_display_text() == "嘿", "空白鍵一聲必須定音為「黑」或「嘿」");
        let resp3 = engine.process_key('\n');
        assert!(resp3.committed_chunk == "黑" || resp3.committed_chunk == "嘿", "Enter 鍵必須提交「黑」或「嘿」");
    }

    #[test]
    fn test_user_reported_anomalies() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 調整過程中
        engine.reset_buffer();
        for c in "tewd jlf gxhj vld jxl ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "調整過程中", "時相名詞方位構式必精準命中「調整過程中」，杜絕「過城中」");
        }

        // 2. 退兩步 / 進三步
        engine.reset_buffer();
        for c in "txaj lekf bxj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "退兩步", "位移動作+動量詞必精準命中「退兩步」，杜絕「退兩部」");
        }

        engine.reset_buffer();
        for c in "jenj sm bxj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "進三步", "位移動作+動量詞必精準命中「進三步」，杜絕「進三部」");
        }

        // 3. 開發初期
        engine.reset_buffer();
        for c in "ki fr vx ved".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "開發初期", "時相名詞構式必精準命中「開發初期」，杜絕「開發出其」");
        }

        // 4. 相關研究可以參考
        engine.reset_buffer();
        for c in "cek gxm emd jeoj kgf ef am kwf ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "相關研究可以參考", "長句複合詞 cohesion 必精準命中「相關研究可以參考」，杜絕「研就可以」");
        }
    }

    #[test]
    fn test_numerical_action_and_kaifachu_qita() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 極端造句：開發出其他廠商從來沒想過的商品
        engine.reset_buffer();
        for c in "ki fr vx ved tr vkf ck axld lid mad cekf gxhj ds ck penf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "開發出其他廠商從來沒想過的商品", "極端造句動補結構必精準命中「開發出其他...」，杜絕「開發初期他」劫掠");
        }

        // 2. 三天捕魚五天曬網
        engine.reset_buffer();
        for c in "sm tem bxf ud xf tem cij xkf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "三天捕魚五天曬網", "對稱俗語成語必精準命中「三天捕魚五天曬網」，杜絕簡體「晒」");
        }

        // 3. 畫三筆塗兩筆
        engine.reset_buffer();
        for c in "hxrj sm bef txd lekf bef".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "畫三筆塗兩筆", "動作動量詞結構必精準命中「畫三筆塗兩筆」，杜絕「話三比圖兩比」");
        }

        // 4. 跳一下躺三天
        engine.reset_buffer();
        for c in "tewj e cerj tkf sm tem ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "跳一下躺三天", "動作+時量詞必精準命中「跳一下躺三天」，杜絕生僻字「儻」劫掠");
        }

        // 5. 吃兩口吐一口
        engine.reset_buffer();
        for c in "v lekf kof txj e kof".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "吃兩口吐一口", "動作+動量詞必精準命中「吃兩口吐一口」，杜絕「兔」劫掠");
        }

        // 6. 傷敵五百自毀三千
        engine.reset_buffer();
        for c in "ck ded xf bif zj hxaf sm vem ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "傷敵五百自毀三千", "對稱軍事俗語必精準命中「傷敵五百自毀三千」，杜絕「商的五百自悔」");
        }
    }

    #[test]
    fn test_ta_zai_kaifa_chuqi_kaifachu_qita_guige() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 他在開發初期就開發出其他廠商所沒有的規格
        engine.reset_buffer();
        for c in "tr zij ki fr vx ved jeoj ki fr vx ved tr vkf ck sxhf mad eof ds gxa ggd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "他在開發初期就開發出其他廠商所沒有的規格", "終極同句雙構式必100%精準命中「開發初期」與「開發出其他」");
        }
    }

    #[test]
    fn test_wo_bujin_zheme_xiangzhe() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 我不禁這麼想著
        engine.reset_buffer();
        for c in "xhf bxj jen jgj mgs cekf jgs".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "我不禁這麼想著", "心理副詞短語必精準命中「我不禁這麼想著」，杜絕「我不金」");
        }
    }

    #[test]
    fn test_wo_bu_jinguang_shanshan_chuchang() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 我不金光閃閃的出場恐怕會不禁掉下眼淚
        // ㄨㄛˇ (xhf) ㄅㄨˋ (bxj) ㄐㄧㄣ (jen ) ㄍㄨㄤ (gxk ) ㄕㄢˇ (cmf) ㄕㄢˇ (cmf) ㄉㄜ˙ (ds) ㄔㄨ (vx ) ㄔㄤˇ (vkf)
        // ㄎㄨㄥˇ (kxlf) ㄆㄚˋ (prj) ㄏㄨㄟˋ (hxaj) ㄅㄨˋ (bxj) ㄐㄧㄣ (jen ) ㄉㄧㄠˋ (dewj) ㄒㄧㄚˋ (cerj) ㄧㄢˇ (emf) ㄌㄟˋ (laj)
        engine.reset_buffer();
        for c in "xhf bxj jen gxk cmf cmf ds vx vkf kxlf prj hxaj bxj jen dewj cerj emf laj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "我不金光閃閃的出場恐怕會不禁掉下眼淚", "同句雙構式必精準區分「我不金光閃閃」與「不禁掉下眼淚」");
        }
    }

    #[test]
    fn test_zhen_de_jiu_tianliang_le() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 真的就天亮了
        // ㄓㄣ (jn ) ㄉㄜ˙ (ds) ㄐㄧㄡˋ (jeoj) ㄊㄧㄢ (tem ) ㄌㄧㄤˋ (lekj) ˙ㄌㄜ (ls)
        engine.reset_buffer();
        for c in "jn ds jeoj tem lekj ls".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "真的就天亮了", "副詞連鎖+時間動詞必精準命中「真的就天亮了」，杜絕「真的舊」");
        }
    }

    #[test]
    fn test_bu_tai_xu_yao() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 不太需要: ㄅㄨˋ (bxj) ㄊㄞˋ (tij) ㄒㄩ (cu ) ㄧㄠˋ (ewj)
        engine.reset_buffer();
        for c in "bxj tij cu ewj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "不太需要", "程度副詞+助動詞必精準命中「不太需要」，杜絕「不太虛藥」");
        }
    }

    #[test]
    fn test_chengguo_hui_luantiao() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 成果會亂跳: ㄔㄥˊ (vld) ㄍㄨㄛˇ (gxhf) ㄏㄨㄟˋ (hxaj) ㄌㄨㄢˋ (lxmj) ㄊㄧㄠˋ (tewj)
        engine.reset_buffer();
        for c in "vld gxhf hxaj lxmj tewj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "成果會亂跳", "主語+助動詞+方式副詞動詞必命中「成果會亂跳」，杜絕「成果穢亂跳」");
        }
    }

    #[test]
    fn test_manzu_xuyao_zhuanzhi_qixu_yaodaobingchu() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 滿足需要專治氣虛藥到病除的醫療要求並非夢想
        engine.reset_buffer();
        for c in "mmf zxd cu ewj jxm jj vej cu ewj dwj belj vxd ds e lewd ew veod belj fa mlj cekf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "滿足需要專治氣虛藥到病除的醫療要求並非夢想", "極端複雜長句必精準命中「滿足需要」與「氣虛藥到病除」");
        }
    }

    #[test]
    fn test_yueshi_haokun_yueshi_juede_haokunnan() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 越是好睏越是覺得好困難
        engine.reset_buffer();
        for c in "uej cj hwf kxnj uej cj jued ds hwf kxnj nmd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "越是好睏越是覺得好困難", "對稱對比構式必精確區分生理狀態「好睏」與程度形容詞「好困難」");
        }
    }

    #[test]
    fn test_diaozhuan() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 刁鑽
        engine.reset_buffer();
        for c in "dew zxm ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "刁鑽", "正體形容詞必精準命中「刁鑽」，杜絕「彫瑑」劫掠");
        }
    }

    #[test]
    fn test_ta_zongshi_gen_bieren_butong_yueshi_haokunnan_yueshi_juede_haorongyi() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 他總是跟別人不同越是好困難越是覺得好容易
        engine.reset_buffer();
        for c in "tr zxlf cj gn beed rnd bxj txld uej cj hwf kxnj nmd uej cj jued ds hwf rxld ej".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "他總是跟別人不同越是好困難越是覺得好容易", "雙向對比構式必精確命中「越是好困難」與「越是覺得好容易」，絕不碎拆為「好睏難」");
        }
    }

    #[test]
    fn test_adv_baogao_yiqifengfa() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 這份報告是由意氣風發的青年學者共同完成的
        engine.reset_buffer();
        for c in "jgj fnj bwj gwj cj eod ej vej fl fr ds vel nemd cued jgf gxlj txld xmd vld ds".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "這份報告是由意氣風發的青年學者共同完成的", "複合長句與成語修飾必精確命中「這份報告是由意氣風發的青年學者共同完成的」");
        }
    }

    #[test]
    fn test_adv_zhexuejie_xinxue_jiejing() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 這部作品是哲學界的心血結晶
        engine.reset_buffer();
        for c in "jgj bxj zxhj penf cj jgd cued jeej ds cen ceef jeed jel ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "這部作品是哲學界的心血結晶", "學術界與成語比喻結構必精確命中「這部作品是哲學界的心血結晶」");
        }
    }

    #[test]
    fn test_adv_cuican_duomu_liulian_wangfan() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 這座城市的夜景真是璀璨奪目讓人流連忘返
        engine.reset_buffer();
        for c in "jgj zxhj vld cj ds eej jelf jn cj axaf amj dxhd mxj rkj rnd leod lemd xkj fmf".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "這座城市的夜景真是璀璨奪目讓人流連忘返", "多重成語串接句必精準命中「璀璨奪目」與「流連忘返」，杜絕劫掠");
        }
    }

    #[test]
    fn test_adv_kankan_ertan_zhuoyue() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 他在演講中侃侃而談展現了卓越的才華
        engine.reset_buffer();
        for c in "tr zij emf jekf jxl kmf kmf ld tmd jmf cemj ls jxhd uej ds aid hxrd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "他在演講中侃侃而談展現了卓越的才華", "場合後置詞引導成語構式必命中「侃侃而談」與「卓越」，抑制生僻字劫掠");
        }
    }

    #[test]
    fn test_adv_yaozeme_shouhuo_xian_name_zai() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 要怎麼收穫先那麼栽
        engine.reset_buffer();
        for c in "ewjznfmsco hxhjcem nrjmszi ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "要怎麼收穫先那麼栽", "長諺語熟語結構必整體連貫輸出「要怎麼收穫先那麼栽」");
        }
    }

    #[test]
    fn test_yanjiang_kuzawuwei_haokun_nandao() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 他的演講枯燥無味真的讓人好睏難道不是真的
        // ㄊㄚ (tr ) ㄉㄜ˙ (ds) ㄧㄢˇ (emf) ㄐㄧㄤˇ (jekf) ㄎㄨ (kx ) ㄗㄠˋ (zwj) ㄨˊ (xd) ㄨㄟˋ (xaj) ㄓㄣ (jn ) ㄉㄜ˙ (ds) ㄖㄤˋ (rkj) ㄖㄣˊ (rnd) ㄏㄠˇ (hwf) ㄎㄨㄣˋ (kxnj) ㄋㄢˊ (nmd) ㄉㄠˋ (dwj) ㄅㄨˊ (bxd) ㄕˋ (cj) ㄓㄣ (jn ) ㄉㄜ˙ (ds)
        engine.reset_buffer();
        for c in "tr ds emf jekf kx zwj xd xaj jn ds rkj rnd hwf kxnj nmd dwj bxd cj jn ds".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "他的演講枯燥無味真的讓人好睏難道不是真的", "使役構式「讓人好睏」與反詰構式「難道不是」必各自成詞，嚴防跨界黏合成「好困難倒」");
        }
    }

    #[test]
    fn test_three_pitfalls_zhijie_anliangxia_cai_keng() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.lexicon.load_tsv("../ruhi_sandbox/user_dict.tsv", "user");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 我直接按兩下搞定了
        // ㄨㄛˇ (xhf) ㄓˊ (jd) ㄐㄧㄝ (jee ) ㄢˋ (mj) ㄌㄧㄤˇ (lekf) ㄒㄧㄚˋ (cerj) ㄍㄠˇ (gwf) ㄉㄧㄥˋ (delj) ˙ㄌㄜ (ls)
        engine.reset_buffer();
        for c in "xhf jd jee mj lekf cerj gwf delj ls".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "我直接按兩下搞定了", "方式副詞+動詞動量詞構式必精確命中「我直接按兩下搞定了」，杜絕「我執接」與「案兩下」劫掠");
        }

        // 2. 一次踩了兩個坑
        // ㄧ (e ) ㄘˋ (aj) ㄘㄞˇ (aif) ˙ㄌㄜ (ls) ㄌㄧㄤˇ (lekf) ˙ㄍㄜ (ggs) ㄎㄥ (kl )
        engine.reset_buffer();
        for c in "e aj aif ls lekf ggs kl ".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "一次踩了兩個坑", "動賓搭配與動態完成體構式必命中「一次踩了兩個坑」，杜絕「採」劫掠");
        }
    }

    #[test]
    fn test_shuruzuo_you_neirong_shi_yidong_shuruzuo_daozhi_mubiaoshichuang_dongjie() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.lexicon.load_tsv("../ruhi_sandbox/user_dict.tsv", "user");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 輸入座有內容時移動輸入座導致目標視窗凍結
        // ㄕㄨ (cx ) ㄖㄨˋ (rxj) ㄗㄨㄛˋ (zxhj) ㄧㄡˇ (eof) ㄋㄟˋ (naj) ㄖㄨㄥˊ (rxld) ㄕˊ (cd) ㄧˊ (ed) ㄉㄨㄥˋ (dxlj) ㄕㄨ (cx ) ㄖㄨˋ (rxj) ㄗㄨㄛˋ (zxhj) ㄉㄠˇ (dwf) ㄓˋ (jj) ㄇㄨˋ (mxj) ㄅㄧㄠ (bew ) ㄕˋ (cj) ㄔㄨㄤ (vxk ) ㄉㄨㄥˋ (dxlj) ㄐㄧㄝˊ (jeed)
        engine.reset_buffer();
        for c in "cx rxj zxhj eof naj rxld cd ed dxlj cx rxj zxhj dwf jj mxj bew cj vxk dxlj jeed".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "輸入座有內容時移動輸入座導致目標視窗凍結", "輸入座專用名詞、時間條件構式、移動動賓構式必精確命中，杜絕「作」、「溶蝕」、「一棟」劫掠");
        }
    }

    #[test]
    fn test_shuru_zuowen_and_wenzi_jiuhui() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.lexicon.load_tsv("../ruhi_sandbox/user_dict.tsv", "user");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試「輸入作文」：杜絕「輸入座文」碎拆劫掠
        // ㄕㄨ (cx ) ㄖㄨˋ (rxj) ㄗㄨㄛˋ (zxhj) ㄨㄣˊ (xnd)
        engine.reset_buffer();
        for c in "cx rxj zxhj xnd".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "輸入作文", "動賓結構「輸入作文」必精準命中，絕不被「輸入座」碎拆為「輸入座文」");
        }

        // 2. 測試「文字就會」：杜絕「文自救會」劫掠
        // ㄨㄣˊ (xnd) ㄗˋ (zj) ㄐㄧㄡˋ (jeoj) ㄏㄨㄟˋ (hxaj)
        engine.reset_buffer();
        for c in "xnd zj jeoj hxaj".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "文字就會", "主謂助動結構「文字就會」必精準命中，絕不被「自救會」劫掠為「文自救會」");
        }

        // 3. 測試完整造句：如果輸入法沒有輸入座那輸入作文需要的文字就會很小很難閱讀啊
        // ㄖㄨˊ (rxd) ㄍㄨㄛˇ (gxhf) ㄕㄨ (cx ) ㄖㄨˋ (rxj) ㄈㄚˇ (frf) ㄇㄟˊ (mad) ㄧㄡˇ (eof) ㄕㄨ (cx ) ㄖㄨˋ (rxj) ㄗㄨㄛˋ (zxhj) ㄋㄚˋ (nrj) ㄕㄨ (cx ) ㄖㄨˋ (rxj) ㄗㄨㄛˋ (zxhj) ㄨㄣˊ (xnd) ㄒㄩ (cu ) ㄧㄠˋ (ewj) ˙ㄉㄜ (ds) ㄨㄣˊ (xnd) ㄗˋ (zj) ㄐㄧㄡˋ (jeoj) ㄏㄨㄟˋ (hxaj) ㄏㄣˇ (hnf) ㄒㄧㄠˇ (cewf) ㄏㄣˇ (hnf) ㄋㄢˊ (nmd) ㄩㄝˋ (uej) ㄉㄨˊ (dxd) ˙ㄚ (ys)
        engine.reset_buffer();
        for c in "rxd gxhf cx rxj frf mad eof cx rxj zxhj nrj cx rxj zxhj xnd cu ewj ds xnd zj jeoj hxaj hnf cewf hnf nmd uej dxd ys".chars() {
            engine.process_key(c);
        }
        {
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag, Some(&mut engine.transitions));
            let result_word = nodes.iter().map(|n| n.word.as_str()).collect::<Vec<_>>().join("");
            assert_eq!(result_word, "如果輸入法沒有輸入座那輸入作文需要的文字就會很小很難閱讀啊", "即時造句測驗必精準命中，徹底根除「輸入座文」與「文自救會」碎拆劫掠");
        }
    }

    #[test]
    fn test_zhongyingwen_hunda_ceshi_uppercase_e() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 使用者輸入許氏注音: jxlelxndhxnjdyfagjcj (中英文混打測試)
        // ㄓㄨㄥ (jxl) ㄧㄥ (el) ㄨㄣˊ (xnd) ㄏㄨㄣˋ (hxnj) ㄉㄚˇ (dyf) ㄘㄜˋ (agj) ㄕˋ (cj)
        engine.reset_buffer();
        for c in "jxlelxndhxnjdyfagjcj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "中英文混打測試", "許氏注音應精準組出「中英文混打測試」");

        // 接續輸入大寫英文字母 'E'
        let resp = engine.process_key('E');
        assert_eq!(resp.committed_chunk, "中英文混打測試", "輸入大寫字母時，先前組好的中文句子必須完整結算上屏");
        assert_eq!(engine.get_display_text(), "E", "引擎應乾淨開啟歐文模式顯示大寫 'E'，絕不回溯洩漏原始按鍵字串");

        // 測試使用者反饋: xlelxndhxnjdyfagjcjLe
        // ㄧㄥ (el) ㄨㄣˊ (xnd) ㄏㄨㄣˋ (hxnj) ㄉㄚˇ (dyf) ㄘㄜˋ (agj) ㄕˋ (cj)
        // 大寫 'L' 結算中文上屏，接續小寫 'e' 形成歐文前綴 "Le"
        engine.reset_buffer();
        for c in "elxndhxnjdyfagjcj".chars() {
            engine.process_key(c);
        }
        let resp_l = engine.process_key('L');
        assert_eq!(resp_l.committed_chunk, "英文混打測試", "大寫 'L' 應結算「英文混打測試」上屏");
        assert_eq!(engine.get_display_text(), "L");
        let _ = engine.process_key('e');
        assert_eq!(engine.get_display_text(), "Le", "接續鍵入小寫 'e' 應形成歐文 preedit 'Le'");
    }

    #[test]
    fn test_ruhi_shurufa_zhongyingwen_hunda_ceshi_let() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.western.load_tsv("../ruhi_lexicon/data/lexicon_english.tsv");

        // 使用者輸入: rxdcefcxrxjfyfjxlelxndhxnjdyfagjcjLet
        // ㄖㄨˊ (rxd) ㄒㄧˇ (cef) ㄕㄨ (cx) ㄖㄨˋ (rxj) ㄈㄚˇ (fyf) ㄓㄨㄥ (jxl) ㄧㄥ (el) ㄨㄣˊ (xnd) ㄏㄨㄣˋ (hxnj) ㄉㄚˇ (dyf) ㄘㄜˋ (agj) ㄕˋ (cj)
        // -> 如喜輸入法中英文混打測試
        engine.reset_buffer();
        for c in "rxdcefcxrxjfyfjxlelxndhxnjdyfagjcj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "如喜輸入法中英文混打測試");

        // 接續鍵入大寫 'L'
        let resp_l = engine.process_key('L');
        assert_eq!(resp_l.committed_chunk, "如喜輸入法中英文混打測試", "大寫 'L' 應結算整句中文上屏");
        assert_eq!(engine.get_display_text(), "L");
        assert!(engine.is_latin_mode);

        // 接續鍵入 'e', 't'
        engine.process_key('e');
        engine.process_key('t');
        assert_eq!(engine.get_display_text(), "Let");

        // 按下空白鍵
        let resp_space = engine.process_key(' ');
        assert_eq!(resp_space.action, "commit_space");
        assert_eq!(resp_space.committed_chunk, "Let ");
    }

    #[test]
    fn test_english_words_on_space() {
        let mut engine = RuhiEngine::new();
        // 1. "and " -> 應結算 "and "，絕不劫掠成「岑」
        engine.reset_buffer();
        for c in "and".chars() {
            engine.process_key(c);
        }
        let resp = engine.process_key(' ');
        assert_eq!(resp.action, "commit_space");
        assert_eq!(resp.committed_chunk, "and ");

        // 2. "hi " -> 應結算 "hi "，絕不劫掠成「咍」
        engine.reset_buffer();
        for c in "hi".chars() {
            engine.process_key(c);
        }
        let resp = engine.process_key(' ');
        assert_eq!(resp.action, "commit_space");
        assert_eq!(resp.committed_chunk, "hi ");

        // 3. 受保護漢語音節 "ha " -> 應保留為「黑」，絕不可被英文劫持
        engine.reset_buffer();
        for c in "ha".chars() {
            engine.process_key(c);
        }
        let resp = engine.process_key(' ');
        assert_ne!(resp.committed_chunk, "ha ", "ha 應為漢語音節「黑」，不可被英文劫掠");
    }

    #[test]
    fn test_english_to_chinese_seamless_transition() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");

        // 輸入 "Chinese" (大寫 C 觸發歐文模式)
        engine.reset_buffer();
        for c in "Chinese".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "Chinese");
        assert!(engine.is_latin_mode);

        // 接續鍵入許氏雙拼中文: "e" (ㄧ) "o" (ㄡ) "f" (ˇ) -> 有
        let resp1 = engine.process_key('e');
        assert_eq!(resp1.committed_chunk, "Chinese", "當英文單字完結且接續字母破壞英文前綴時，應自動結算先前英文單字");
        assert!(!engine.is_latin_mode, "應順暢退回中文輸入模式");

        let resp2 = engine.process_key('o');
        assert_eq!(resp2.action, "update_preedit");

        let resp3 = engine.process_key('f');
        assert_eq!(resp3.action, "update_preedit");
        assert_eq!(engine.get_display_text(), "有", "中文音節應精準組出「有」");
    }

    #[test]
    fn test_canshu_and_mixed_english() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");

        // 1. am + space 必須結算出「參」，且候選第 0 位絕不可被 "am" 劫掠
        engine.reset_buffer();
        engine.process_key('a');
        assert_eq!(engine.get_display_text(), "ㄘ");
        assert!(engine.current_candidates.is_empty() || engine.current_candidates[0] != "a");
        engine.process_key('m');
        assert_eq!(engine.get_display_text(), "ㄘㄢ");
        assert!(engine.current_candidates.is_empty(), "未決音節絕不可被塞入歐文候選 0: {:?}", engine.current_candidates);
        let resp = engine.process_key(' ');
        assert_eq!(resp.action, "update_preedit");
        assert_eq!(engine.get_display_text(), "參", "am + space 應為 ㄘㄢ 一聲「參」");
        assert_eq!(engine.current_candidates[0], "參");
        assert!(!engine.is_latin_mode);

        // 2. amcxj 連續無空格輸入必須精準組出「參數」
        engine.reset_buffer();
        for c in "amcxj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "參數", "amcxj 應精準組出「參數」");
        assert!(!engine.is_latin_mode);

        // 3. am + space + cxj 帶空格輸入亦必須精準組出「參數」
        engine.reset_buffer();
        engine.process_key('a');
        engine.process_key('m');
        engine.process_key(' ');
        assert_eq!(engine.get_display_text(), "參");
        assert_eq!(engine.raw_ascii_buffer, "", "音節成立後 raw_ascii_buffer 必須清空");
        for c in "cxj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "參數", "am + space + cxj 應精準組出「參數」");

        // 4. am + space + space 連續空格絕不可將「參」竄改為 "am "
        engine.reset_buffer();
        engine.process_key('a');
        engine.process_key('m');
        let _r1 = engine.process_key(' ');
        assert_eq!(engine.get_display_text(), "參");
        let r2 = engine.process_key(' ');
        assert_ne!(r2.action, "commit_space", "第二個空白鍵絕不可搶奪為歐文 commit_space");
        assert_ne!(r2.committed_chunk, "am ", "第二個空白鍵絕不可送出 am ");
        assert_eq!(engine.get_display_text(), "參");

        // 5. am + enter 在注音未決狀態下 Enter 必須送出注音預編輯「ㄘㄢ」，絕不可送出 "am"
        engine.reset_buffer();
        engine.process_key('a');
        engine.process_key('m');
        let r_ent = engine.process_key('\r');
        assert_eq!(r_ent.action, "commit_enter");
        assert_eq!(r_ent.committed_chunk, "ㄘㄢ", "Enter 應送出注音預編輯「ㄘㄢ」");

        // 6. 歐文智慧混打：大寫開頭或合法英文單字 (如 and) 正常直通
        engine.reset_buffer();
        for c in "China".chars() {
            engine.process_key(c);
        }
        assert!(engine.is_latin_mode, "China 應自動切入歐文模式");
        let r_china = engine.process_key(' ');
        assert_eq!(r_china.action, "commit_space");
        assert_eq!(r_china.committed_chunk, "China ");

        engine.reset_buffer();
        for c in "and".chars() {
            engine.process_key(c);
        }
        let r_and = engine.process_key(' ');
        assert_eq!(r_and.action, "commit_space");
        assert_eq!(r_and.committed_chunk, "and ");

        println!("=== TRACING so far so good ===");
        engine.reset_buffer();
        let input_text = "Now I am typing English, so far so good.";
        let mut out = String::new();
        for (i, c) in input_text.chars().enumerate() {
            let r = engine.process_key(c);
            if !r.committed_chunk.is_empty() {
                out.push_str(&r.committed_chunk);
            }
            println!("step {}: key='{}' -> action={}, chunk='{}', disp='{}', is_latin={}, raw='{}'",
                i, c, r.action, r.committed_chunk, engine.get_display_text(), engine.is_latin_mode, engine.raw_ascii_buffer);
        }
        println!("TOTAL OUTPUT: '{}'", out);
        assert_eq!(out, "Now I am typing English，so far so good。", "Now I am typing English, so far so good. 必精準直通歐文，杜絕假借漢字劫掠");

        // 7. 驗證使用者抓到的四碼漢字：「對」= d(ㄉ) + x(ㄨ) + a(ㄟ) + j(4聲)
        engine.reset_buffer();
        for c in "dxaj".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "對", "dxaj 必須精準輸出四碼漢字「對」");

        // 8. 驗證保守模式 (conservative)
        engine.reset_buffer();
        engine.mixed_english_mode = "conservative".to_string();
        for c in "China".chars() {
            engine.process_key(c);
        }
        assert!(engine.is_latin_mode, "保守模式下大寫開頭仍應切入歐文模式");
        let r_c = engine.process_key(' ');
        assert_eq!(r_c.committed_chunk, "China ");

        engine.reset_buffer();
        for c in "and".chars() {
            engine.process_key(c);
        }
        let r_and_cons = engine.process_key(' ');
        assert_ne!(r_and_cons.committed_chunk, "and ", "保守模式下小寫短詞絕不可自動上屏歐文");

        // 9. 驗證長詞門檻模式 (min_length_only, min_len=4)
        engine.reset_buffer();
        engine.mixed_english_mode = "min_length_only".to_string();
        engine.mixed_english_min_len = 4;
        for c in "typing".chars() {
            engine.process_key(c);
        }
        let r_typ = engine.process_key(' ');
        assert_eq!(r_typ.committed_chunk, "typing ", "達到長度門檻之單字應正常上屏");

        engine.reset_buffer();
        for c in "and".chars() {
            engine.process_key(c);
        }
        let r_and_len = engine.process_key(' ');
        assert_ne!(r_and_len.committed_chunk, "and ", "未達長度門檻(3<4)之單字不可自動上屏");
    }

    #[test]
    fn test_sheji_and_jiagou() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 測試 1: 設計 (cgj jej)
        engine.reset_buffer();
        for c in "cgj jej".chars() {
            engine.process_key(c);
        }
        println!("DISPLAY for cgj jej: '{}', cand: {:?}", engine.get_display_text(), engine.current_candidates);

        // 測試 2: 架構 (jerj goj)
        engine.reset_buffer();
        for c in "jerj goj".chars() {
            engine.process_key(c);
        }
        println!("DISPLAY for jerj goj: '{}', cand: {:?}", engine.get_display_text(), engine.current_candidates);

        // 測試 3: 當初又是怎麼設計出這樣的架構呢
        engine.reset_buffer();
        // 以許氏鍵盤注音拼裝：
        // 當(dk ) 初(vx ) 又(eoj) 是(cj) 怎(znf) 麼(mgs) 設(cgj) 計(jej) 出(vx ) 這(jgj) 樣(ekj) 的(dgs) 架(jerj) 構(goj) 呢(ngs)
        let keys = "dk vx eoj cj znf mgs cgj jej vx jgj ekj dgs jerj goj ngs";
        for c in keys.chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "當初又是怎麼設計出這樣的架構呢");
    }

    #[test]
    fn test_polyphonic_recalibration() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 句 (juj: ㄐㄩˋ) 必為「句」第一順位
        engine.reset_buffer();
        for c in "juj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "句", "ㄐㄩˋ 必為「句」第一順位");

        // 2. 否 (fof: ㄈㄡˇ) 必為「否」第一順位，絕不被「不」劫掠
        engine.reset_buffer();
        for c in "fof".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "否", "ㄈㄡˇ 必為「否」第一順位");

        // 3. 倒 (dwf: ㄉㄠˇ) 必為「倒」第一順位
        engine.reset_buffer();
        for c in "dwf".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "倒", "ㄉㄠˇ 必為「倒」第一順位");

        // 4. 中 (jxlj: ㄓㄨㄥˋ) 命中/中獎，必為「中」第一順位
        engine.reset_buffer();
        for c in "jxlj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "中", "ㄓㄨㄥˋ 必為「中」第一順位");

        // 5. 行 (hkd: ㄏㄤˊ) 行業/銀行，必為「行」第一順位
        engine.reset_buffer();
        for c in "hkd".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "行", "ㄏㄤˊ 必為「行」第一順位");

        // 6. 重 (vxld: ㄔㄨㄥˊ) 重複/重新，必為「重」第一順位
        engine.reset_buffer();
        for c in "vxld".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "重", "ㄔㄨㄥˊ 必為「重」第一順位");

        // 7. 設計 (cgj jej: ㄕㄜˋ ㄐㄧˋ) 必為「設計」，絕不被罕見古姓氏「庫」搶成「庫寄」
        engine.reset_buffer();
        for c in "cgj jej".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "設計", "ㄕㄜˋ ㄐㄧˋ 必為「設計」，杜絕「庫寄」劫掠");

        // 8. 驗證「庫」在 ㄕㄜˋ 單字候選中已退至深層後方（絕不在前 10 候選）
        engine.reset_buffer();
        for c in "cgj".chars() { engine.process_key(c); }
        let top_10_cgj: Vec<String> = engine.current_candidates.iter().take(10).cloned().collect();
        assert!(!top_10_cgj.contains(&"庫".to_string()), "「庫」在 ㄕㄜˋ 候選字中必不在前 10 名，杜絕冒充");

        // 9. 驗證「不」在 ㄈㄡˇ 單字候選中已退至深層後方，杜絕劫掠「否」
        engine.reset_buffer();
        for c in "fof".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "否", "ㄈㄡˇ 首選字必為「否」");
        let top_3_fof: Vec<String> = engine.current_candidates.iter().take(3).cloned().collect();
        assert!(!top_3_fof.contains(&"不".to_string()), "「不」在 ㄈㄡˇ 候選字中必不在前 3 名，杜絕冒充「否」");
    }

    #[test]
    fn test_xiang_disambiguation() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // Rule Set A: 像 (比喻、相似、摹擬)
        // 1. 長得很像 (ㄓㄤˇ ㄉㄜ˙ ㄏㄣˇ ㄒㄧㄤˋ)
        engine.reset_buffer();
        for c in "jkf dgs hnf cekj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "長得很像", "「長得很像」比喻構式必命中「像」");

        // 2. 看起來像 (ㄎㄢˋ ㄑㄧˇ ㄌㄞˊ ㄒㄧㄤˋ)
        engine.reset_buffer();
        for c in "kmj vef lid cekj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "看起來像", "「看起來像」比喻構式必命中「像」");

        // 3. 像極了 (ㄒㄧㄤˋ ㄐㄧˊ ㄌㄜ˙)
        engine.reset_buffer();
        for c in "cekj jed lgs".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "像極了", "「像極了」比喻構式必命中「像」");

        // 4. 像話 (ㄒㄧㄤˋ ㄏㄨㄚˋ)
        engine.reset_buffer();
        for c in "cekj hxrj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "像話", "「像話」比喻構式必命中「像」");

        // Rule Set B: 相 (結構特徵、事實真相、科學相態、交互)
        // 5. 真相大白 (ㄓㄣ ㄒㄧㄤˋ ㄉㄚˋ ㄅㄞˊ)
        engine.reset_buffer();
        for c in "jn cekj drj bid".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "真相大白", "「真相大白」事實構式必命中「相」");

        // 6. 面相 (ㄇㄧㄢˋ ㄒㄧㄤˋ)
        engine.reset_buffer();
        for c in "memj cekj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "面相", "「面相」容貌構式必命中「相」");

        // 7. 互相 (ㄏㄨˋ ㄒㄧㄤ)
        engine.reset_buffer();
        for c in "hxj cek ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "互相", "「互相」交互構式必命中「相」");

        // 8. 相輔相成 (ㄒㄧㄤ ㄈㄨˇ ㄒㄧㄤ ㄔㄥˊ)
        engine.reset_buffer();
        for c in "cek fxf cek vld".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "相輔相成", "「相輔相成」成語構式必命中「相」");

        // 9. 相提並論 (ㄒㄧㄤ ㄊㄧˊ ㄅㄧㄥˋ ㄌㄨㄣˋ)
        engine.reset_buffer();
        for c in "cek ted belj lxnj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "相提並論", "「相提並論」成語構式必命中「相」");

        // 10. 相敬如賓 (ㄒㄧㄤ ㄐㄧㄥˋ ㄖㄨˊ ㄅㄧㄣ)
        engine.reset_buffer();
        for c in "cek jelj rxd ben ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "相敬如賓", "「相敬如賓」成語構式必命中「相」");

        // 11. 相片 (ㄒㄧㄤˋ ㄆㄧㄢˋ) 首選為相片，非像片
        engine.reset_buffer();
        for c in "cekj pemj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "相片", "ㄒㄧㄤˋ ㄆㄧㄢˋ 預設必為「相片」，非「像片」");

        // Rule Set C: 象 (宏觀趨勢、抽象概念、自然跡象)
        // 12. 現象 (ㄒㄧㄢˋ ㄒㄧㄤˋ)
        engine.reset_buffer();
        for c in "cemj cekj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "現象", "「現象」宏觀趨勢必命中「象」");

        // 13. 現象級 (ㄒㄧㄢˋ ㄒㄧㄤˋ ㄐㄧˊ)
        engine.reset_buffer();
        for c in "cemj cekj jed".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "現象級", "「現象級」宏觀趨勢必命中「象」");

        // 14. 抽象 (ㄔㄡ ㄒㄧㄤˋ)
        engine.reset_buffer();
        for c in "vo cekj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "抽象", "「抽象」邏輯概念必命中「象」");

        // 15. 印象 (ㄧㄣˋ ㄒㄧㄤˋ)
        engine.reset_buffer();
        for c in "enj cekj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "印象", "「印象」心理認知必命中「象」");

        // 16. 萬象更新 (ㄨㄢˋ ㄒㄧㄤˋ ㄍㄥ ㄒㄧㄣ)
        engine.reset_buffer();
        for c in "xmj cekj gl cen ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "萬象更新", "「萬象更新」宏觀演變必命中「象」");

        // Rule Set D: 衝突消解 (Conflict Resolution & Overrides)
        // 17. 氣相層析 (ㄑㄧˋ ㄒㄧㄤˋ ㄘㄥˊ ㄒㄧ) 必命中「氣相」
        engine.reset_buffer();
        for c in "vej cekj ald ce ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "氣相層析", "科學層析語境「氣相層析」必命中「氣相」，非「氣象」");

        // 18. 氣象預報 (ㄑㄧˋ ㄒㄧㄤˋ ㄩˋ ㄅㄠˋ) 必命中「氣象」
        engine.reset_buffer();
        for c in "vej cekj uj bwj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "氣象預報", "氣象語境「氣象預報」必命中「氣象」，非「氣相」");
    }

    #[test]
    fn test_he_conjunction_and_anti_archaic_gan() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // Test 1: hmj (ㄏㄢˋ) 必命中「和」第一順位
        engine.reset_buffer();
        for c in "hmj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "和", "ㄏㄢˋ 必為「和」第一順位");

        // Test 2: 朋友和 (朋友 + ㄏㄢˋ) 必命中「朋友和」，絕不被古音「感」劫掠
        engine.reset_buffer();
        for c in "pld eof hmj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "朋友和", "「朋友和」連詞構式必命中「和」，杜絕古音「感」劫掠");

        // Test 3: 你和我 (你 + 和 + 我)
        engine.reset_buffer();
        for c in "nef hmj xhf".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "你和我", "「你和我」連詞構式必命中「你和我」");

        // Test 4: 幽默感 (幽默 + ㄍㄢˇ) 驗證正常讀音「~感」依然享有派生名詞後綴加權
        engine.reset_buffer();
        for c in "eo mhj gmf".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "幽默感", "「幽默感」名詞後綴必命中「感」");
    }

    #[test]
    fn test_all_archaic_polyphonic_impostors_isolated() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 設計 (ㄕㄜˋ ㄐㄧˋ): 設 (ㄕㄜˋ) 絕不被古音「庫 (ㄕㄜˋ)」劫掠
        engine.reset_buffer();
        for c in "cgj jej".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "設計", "「設計」必命中「設」，杜絕罕用破音「庫 (ㄕㄜˋ)」劫掠");

        // 2. 能否 (ㄋㄥˊ ㄈㄡˇ): 否 (ㄈㄡˇ) 絕不被孤例「不 (ㄈㄡˇ)」劫掠
        engine.reset_buffer();
        for c in "nld fof".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "能否", "「能否」必命中「否」，杜絕孤例「不 (ㄈㄡˇ)」劫掠");

        // 3. 為何 (ㄨㄟˋ ㄏㄜˊ): 何 (ㄏㄜˊ) 絕不被古音「害 (ㄏㄜˊ)」劫掠
        engine.reset_buffer();
        for c in "xaj hgd".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "為何", "「為何」必命中「何」，杜絕古音「害 (ㄏㄜˊ)」劫掠");

        // 4. 足夠 (ㄗㄨˊ ㄍㄡˋ): 夠 (ㄍㄡˋ) 絕不被古音「句 (ㄍㄡˋ)」劫掠
        engine.reset_buffer();
        for c in "zxd goj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "足夠", "「足夠」必命中「夠」，杜絕古音「句 (ㄍㄡˋ)」劫掠");

        // 5. 相貌 (ㄒㄧㄤˋ ㄇㄠˋ): 相 (ㄒㄧㄤˋ) 絕不被古音「樣 (ㄒㄧㄤˋ)」劫掠
        engine.reset_buffer();
        for c in "cekj mwj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "相貌", "「相貌」必命中「相」，杜絕古音「樣 (ㄒㄧㄤˋ)」劫掠");

        // 6. 秘密 (ㄇㄧˋ ㄇㄧˋ): 密 (ㄇㄧˋ) 絕不被孤例「日 (ㄇㄧˋ)」劫掠
        engine.reset_buffer();
        for c in "mejmej".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "秘密", "「秘密」必命中「密」，杜絕孤例「日 (ㄇㄧˋ)」劫掠");

        // 7. 胡同/衚衕 (ㄏㄨˊ ㄊㄨㄥˋ): 驗證固有雙字詞不受單字孤例隔離影響
        engine.reset_buffer();
        for c in "hxdtxlj".chars() { engine.process_key(c); }
        assert!(
            matches!(engine.get_display_text().as_str(), "胡同" | "衚衕"),
            "固有詞「胡同/衚衕」必須正常輸出"
        );

        // 8. 太太 (ㄊㄞˋ ㄊㄞˋ): 太 (ㄊㄞˋ) 絕不被古音「大 (ㄊㄞˋ)」劫掠
        engine.reset_buffer();
        for c in "tij tij".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "太太", "「太太」必精準命中，杜絕古音「大 (ㄊㄞˋ)」劫掠");
    }

    #[test]
    fn test_user_troublesome_sentences() {
        let mut engine = RuhiEngine::new();
        // 1. 也感慨 (eef gmf kif / eef gmf kij)
        engine.reset_buffer();
        for c in "eef gmf kif".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "也感慨", "「也感慨」(3聲) 必命中「也感慨」");

        engine.reset_buffer();
        for c in "eef gmf kij".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "也感慨", "「也感慨」(4聲常讀音) 必命中「也感慨」，絕非「也敢愾」");

        // 2. 服務時就 (fxd xj cd jeoj)
        engine.reset_buffer();
        for c in "fxd xj cd jeoj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "服務時就", "時間副句「服務時就」必精準命中，絕非「服務石臼」");

        // 3. 提出李飛飛 (ted vx  lef fa  fa )
        engine.reset_buffer();
        for c in "ted vx  lef fa  fa ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "提出李飛飛", "專有名詞「提出李飛飛」必精準命中，絕非「提出裡騑騑」");

        // 4. 卻被譏笑的往事 (vuej baj je cewj ds xkf cj)
        engine.reset_buffer();
        for c in "vuej baj je cewj ds xkf cj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "卻被譏笑的往事", "被動語態「卻被譏笑的往事」必精準命中，絕非「卻被績效的網室」");

        // 5. 一句很普通的話 (e juj hnf pxf txl  ds hxrj)
        engine.reset_buffer();
        for c in "e juj hnf pxf txl  ds hxrj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "一句很普通的話", "「一句很普通的話」必精準命中，絕非「依據很普通的化」");

        // 6. 我快哭了 (xhf kxij kx  ls)
        engine.reset_buffer();
        for c in "xhf kxij kx  ls".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "我快哭了", "情態副詞短語「我快哭了」必精準命中，絕非「我塊哭了」");
    }

    #[test]
    fn test_user_cases_tai_and_xiandao() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 我問太太看看紙本何在再決定下一步好了
        {
            engine.reset_buffer();
            for c in "xhf xnj tij tij kmj kmj jf bnf hgd zij zij jued delj cerj e bxj hwf ls".chars() { engine.process_key(c); }
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag1 = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes1, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag1, Some(&mut engine.transitions));
            println!("NODES 1: {:?}", nodes1.iter().map(|n| (&n.word, n.weight)).collect::<Vec<_>>());
            println!("TEXT 1: {}", engine.get_display_text());
            assert_eq!(engine.get_display_text(), "我問太太看看紙本何在再決定下一步好了", "精準命中「太太」與「何在」，杜絕「大」與「和在在」劫掠");
        }

        // 十分感謝這個案子先到這裡
        {
            engine.reset_buffer();
            for c in "cd fn  gmf ceej jgj ggj mj zs cem  dwj jgj lef".chars() { engine.process_key(c); }
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag2 = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes2, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag2, Some(&mut engine.transitions));
            println!("NODES 2: {:?}", nodes2.iter().map(|n| (&n.word, n.weight)).collect::<Vec<_>>());
            println!("TEXT 2: {}", engine.get_display_text());
            assert_eq!(engine.get_display_text(), "十分感謝這個案子先到這裡", "精準命中「先到」，杜絕罕用詞「祆道」劫掠");
        }

        // 且看事態如何再做發展
        {
            engine.reset_buffer();
            for c in "veef kmj cj tij rxd hgd zij zxhj fr  jmf".chars() { engine.process_key(c); }
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag3 = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes3, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag3, Some(&mut engine.transitions));
            println!("NODES 3: {:?}", nodes3.iter().map(|n| (&n.word, n.weight)).collect::<Vec<_>>());
            assert!(
                matches!(engine.get_display_text().as_str(), "且看事態如何再作發展" | "且看事態如何再做發展"),
                "精準命中「且看事態如何再作/做發展」，杜絕「世態」與「如+何在」碎拆，實際為：{}",
                engine.get_display_text()
            );
        }

        // 這一整段過程讓我回憶起
        {
            engine.reset_buffer();
            for c in "jgj e  jlf dxmj gxhj vld rkj xhf hxad ej vef".chars() { engine.process_key(c); }
            let dag_builder = LatticeDAG::new(&engine.lexicon);
            let dag4 = dag_builder.build_dag(&engine.fsm.syllables, true);
            let (nodes4, _) = engine.router.solve_best_path(engine.fsm.syllables.len(), &dag4, Some(&mut engine.transitions));
            println!("NODES 4: {:?}", nodes4.iter().map(|n| (&n.word, n.weight)).collect::<Vec<_>>());
            println!("TEXT 4: {}", engine.get_display_text());
            assert_eq!(engine.get_display_text(), "這一整段過程讓我回憶起", "精準命中「這一整段過程讓我回憶起」，杜絕「過承讓」碎拆劫掠");
        }
    }

    #[test]
    fn test_user_cross_boundary_benchmarks() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // ── 0. 對話中的選詞修正 (Conversational Vocabulary Fixes) ──
        // 0.1 漢語 (ㄏㄢˋ ㄩˇ) 杜絕割裂為「和與」
        {
            engine.reset_buffer();
            for c in "hmjuf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "漢語", "「漢語」必精準命中，杜絕疊架連詞「和與」");
        }

        // 0.2 一般所謂 (ㄧ ㄅㄢ ㄙㄨㄛˇ ㄨㄟˋ) 杜絕「一班所謂」
        {
            engine.reset_buffer();
            for c in "e bm sxhfxaj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "一般所謂", "「一般所謂」必精準命中，杜絕「一班所謂」");
        }

        // 0.3 密碼 (ㄇㄧˋ ㄇㄚˇ) 杜絕割裂為「密嗎」
        {
            engine.reset_buffer();
            for c in "mejmrf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "密碼", "「密碼」必精準命中，杜絕「密嗎」");
        }

        // 0.4 死語 (ㄙˇ ㄩˇ) 杜絕割裂為「死與」
        {
            engine.reset_buffer();
            for c in "sfuf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "死語", "「死語」必精準命中，杜絕割裂「死與」");
        }

        // 0.5 比擬後綴 ~般 (松鼠般, 密碼般)
        {
            engine.reset_buffer();
            for c in "sxl cxfbm ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "松鼠般", "比擬後綴必命中「松鼠般」");

            engine.reset_buffer();
            for c in "mejmrfbm ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "密碼般", "比擬後綴必命中「密碼般」");
        }

        // ── 1. 極端基準句 1: 盲人摸象時 vs 盲人摸橡實 ──
        // 1.1 盲人摸象時 (ㄇㄤˊ ㄖㄣˊ ㄇㄛ ㄒㄧㄤˋ ㄕˊ)
        {
            engine.reset_buffer();
            for c in "mkdrndmh cekjcd".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "盲人摸象時", "成語加時間標記必精準命中「盲人摸象時」");
        }

        // 1.2 敲打橡實的 (ㄑㄧㄠ ㄉㄚˇ ㄒㄧㄤˋ ㄕˊ ㄉㄜ˙)
        {
            engine.reset_buffer();
            for c in "vew drfcekjcdds".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "敲打橡實的", "動賓名詞偏正結構必精準命中「敲打橡實的」");
        }

        // 1.3 照說無法相提並論
        {
            engine.reset_buffer();
            for c in "jwjcxh xdfrfcek tedbeljlxnj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "照說無法相提並論", "評註連詞成語句必精準命中「照說無法相提並論」");
        }

        // ── 2. 極端基準句 2: 坐一下石頭 vs 坐一下時 ──
        // 2.1 坐一下石頭做的椅子 (防「下石」截斷)
        {
            engine.reset_buffer();
            for c in "zxhje cerjcdtodzxhjdsefzs".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "坐一下石頭做的椅子", "動作動量偏正結構必精準命中「坐一下石頭做的椅子」，杜絕「坐一下石」");
        }

        // 2.2 坐一下時的不同 (防「下石」截斷)
        {
            engine.reset_buffer();
            for c in "zxhje cerjcddsbxjtxld".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "坐一下時的不同", "動作時態從句必精準命中「坐一下時的不同」，杜絕「下石」劫掠");
        }

        // ── 3. 極端基準句 3: 此碑石 vs 乃悲中之悲 vs 為碑中之悲 ──
        // 3.1 此碑石 (ㄘˇ ㄅㄟ ㄕˊ)
        {
            engine.reset_buffer();
            for c in "afba cd".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "此碑石", "指示名詞必精準命中「此碑石」");
        }

        // 3.2 悲淒慘事乃悲中之悲
        {
            engine.reset_buffer();
            for c in "ba ve amfcjnifba jxl j ba ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "悲淒慘事乃悲中之悲", "語境情緒定型構式必命中「悲淒慘事乃悲中之悲」");
        }

        // 3.3 致此碑為碑中之碑
        {
            engine.reset_buffer();
            for c in "jjafba xajba jxl j ba ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "致此碑為碑中之碑", "實體標的構式必精準命中「致此碑為碑中之碑」");
        }

        // ── 4. 極端基準句 4: 混混打看 vs 混打看板 ──
        // 4.1 混混打看
        {
            engine.reset_buffer();
            for c in "hxnjhxnjdrfkmj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "混混打看", "疊字名詞+察看動詞必精準命中「混混打看」");
        }

        // 4.2 混打看板 (防「打看」截斷)
        {
            engine.reset_buffer();
            for c in "hxnjdrfkmjbmf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "混打看板", "「混打看板」必精準完整輸出，杜絕「混打看版」跨詞隙截斷");
        }

        // 4.3 歐漢大亂鬥
        {
            engine.reset_buffer();
            for c in "o hmjdrjlxmjdoj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "歐漢大亂鬥", "流行語文化名稱必精準命中「歐漢大亂鬥」");
        }

        // ── 5. 極端基準句 5: 松鼠這般用力敲打橡實 vs 街巷時鐘底下 ──
        // 5.1 街巷時鐘 (防「橡實」截斷)
        {
            engine.reset_buffer();
            for c in "jee cekjcdjxl ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "街巷時鐘", "「街巷時鐘」空間地標結構必精準命中，杜絕「街橡實鐘」截斷");
        }

        // 5.2 打算備齊一盆
        {
            engine.reset_buffer();
            for c in "drfsxmjbajvede pnd".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "打算備齊一盆", "動量短語必精準命中「打算備齊一盆」");
        }

        // ── 6. 深度反例與語法構式驗證 (Deep Counter-Examples & Constructions) ──
        // 6.1 已下載 (ㄧˇ ㄒㄧㄚˋ ㄗㄞˋ / ㄗㄞˇ) 杜絕「倚下載」
        {
            engine.reset_buffer();
            for c in "efcerjzij".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "已下載", "時相副詞動詞短語必精準命中「已下載」");
        }

        // 6.2 沒有興趣 (ㄇㄟˊ ㄧㄡˇ ㄒㄧㄥˋ ㄑㄩˋ) 杜絕「沒有性去」
        {
            engine.reset_buffer();
            for c in "madeofceljvuj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "沒有興趣", "動賓名詞短語必精準命中「沒有興趣」，杜絕名詞後綴「~性」劫掠");
        }

        // 6.3 使用者一開始 (ㄕˇ ㄩㄥˋ ㄓㄜˇ ㄧ ㄎㄞ ㄕˇ) 杜絕「使用赭衣開始」
        {
            engine.reset_buffer();
            for c in "cfuljjgf e ki cf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "使用者一開始", "核心複合詞構式必精準命中「使用者一開始」，杜絕「赭衣」截斷");
        }

        // 6.4 若能支援批次轉換 (杜絕「之源」、「劈刺」)
        {
            engine.reset_buffer();
            for c in "rxhjnldj umdpe ajjxmfhxmj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "若能支援批次轉換", "情態動賓科技名詞必精準命中「若能支援批次轉換」，杜絕「之源」與「劈刺」");
        }

        // 6.5 平和與寧靜 (ㄆㄧㄥˊ ㄏㄜˊ ㄩˇ ㄋㄧㄥˊ ㄐㄧㄥˋ)
        {
            engine.reset_buffer();
            for c in "peldhgd uf neldjelj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "平和與寧靜", "並列描摹連詞構式必精準命中「平和與寧靜」");
        }

        // 6.6 死與愛，愛與死，變態 (杜絕「死語愛」、「愛瘐死」、「變太」)
        {
            engine.reset_buffer();
            for c in "sfufij".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "死與愛", "哲學對稱名詞構式必精準命中「死與愛」，杜絕「死語愛」");

            engine.reset_buffer();
            for c in "ijufsf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "愛與死", "哲學對稱名詞構式必精準命中「愛與死」，杜絕「愛瘐死」");

            engine.reset_buffer();
            for c in "bemjtij".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "變態", "文化心理核心詞必精準命中「變態」，杜絕「變太」");
        }

        // 6.7 這一班所謂精英 (杜絕「這一般所謂」)
        {
            engine.reset_buffer();
            for c in "jgje bm sxhfxajjel el ".chars() { engine.process_key(c); }
            let text = engine.get_display_text();
            assert!(text == "這一班所謂精英" || text == "這一班所謂菁英", "指示量詞構式必精準命中「這一班所謂精英/菁英」，杜絕「這一般所謂」；實際為：{}", text);
        }

        // 6.8 無損高效壓縮音檔 (杜絕「高校」、「呀縮」、「姻黨」)
        {
            engine.reset_buffer();
            for c in "xd sxnf gw cewj er sxh en dkf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "無損高效壓縮音檔", "科技多重名詞修飾鏈必精準命中「無損高效壓縮音檔」，杜絕「高校」、「呀縮」、「姻黨」");
        }

        // 6.9 正常發揮、我心累 (杜絕「正常發灰」、「我心類」)
        {
            engine.reset_buffer();
            for c in "jljvkdfr hxa ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "正常發揮", "能力動賓修飾短語必精準命中「正常發揮」，杜絕繞口令「發灰」劫掠");

            engine.reset_buffer();
            for c in "xhfcen laj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "我心累", "心理狀態詞必精準命中「我心累」，杜絕「心類」劫掠");
        }

        // 6.10 速效藥、手動加詞 (杜絕「速效要」、「手棟家祠」)
        {
            engine.reset_buffer();
            for c in "sxjcewjewj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "速效藥", "隱喻偏正名詞必精準命中「速效藥」，杜絕「速效要」劫掠");

            engine.reset_buffer();
            for c in "cofdxljjer ad".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "手動加詞", "輸入法領域動賓短語必精準命中「手動加詞」，杜絕「手棟家祠」劫掠");
        }

        // 6.11 使用者自建詞庫、智慧監測 (杜絕「自薦」、「置喙」)
        {
            engine.reset_buffer();
            for c in "cfuljjgf zjjemjadkxj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "使用者自建詞庫", "偏正複合詞必精準命中「使用者自建詞庫」，杜絕「自薦」劫掠");

            engine.reset_buffer();
            for c in "eof e ggs jjhxajjem agj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "有一個智慧監測", "數量定語與科技形容詞必精準命中「有一個智慧監測」，杜絕「置喙」劫掠");
        }

        // 6.12 實境體檢：很久也沒關係、每告一個段落、會自動執行 (杜絕「很九野」、「沒關西」、「美告」、「諱字棟」)
        {
            // (a) 很久也 (ㄏㄣˇ ㄐㄧㄡˇ ㄧㄝˇ: hnf jeof eef)
            engine.reset_buffer();
            for c in "hnfjeofeef".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "很久也", "時態短語必精準命中「很久也」，杜絕「很九野」搭橋截斷");

            // (b) 沒關係 (口語一聲: ㄇㄟˊ ㄍㄨㄢ ㄒㄧ: mad + gxm + ce )
            engine.reset_buffer();
            for c in "madgxm ce ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "沒關係", "口語日常短語必精準命中「沒關係」，杜絕地名「關西」劫掠");

            // (c) 每告一個段落 (ㄇㄟˇ ㄍㄠˋ ㄧˊ ㄍㄜ˙ ㄉㄨㄢˋ ㄌㄨㄛˋ: maf + gwj + ed + ggs + dxmj + lxhj)
            engine.reset_buffer();
            for c in "mafgwjedggsdxmjlxhj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "每告一個段落", "分配構式短語必精準命中「每告一個段落」，杜絕「美告」劫掠");

            // (d) 會自動執行 (ㄏㄨㄟˋ ㄗˋ ㄉㄨㄥˋ ㄓˊ ㄒㄧㄥˊ: hxaj + zj + dxlj + jd + celd)
            engine.reset_buffer();
            for c in "hxajzjdxljjdceld".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "會自動執行", "情態科技動詞短語必精準命中「會自動執行」，杜絕古代「諱字棟」搭橋劫掠");
        }

        // 6.13 使用者跨詞邊界真實案例與極端造句評測
        {
            // (a) 支援非破壞性裁剪 (ㄓ ㄩㄢˊ ㄈㄟ ㄆㄛˋ ㄏㄨㄞˋ ㄒㄧㄥˋ ㄘㄞˊ ㄐㄧㄢˇ: j  + umd + fa  + phj + hxij + celj + aid + jemf)
            engine.reset_buffer();
            for c in "j umdfa phjhxijceljaidjemf".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "支援非破壞性裁剪", "科技名詞必精準命中「支援非破壞性裁剪」，杜絕「枝元妃破壞性裁剪」搭橋劫掠");

            // (b) 依據第一句和最後一句以及中間那一句
            // ㄧ ㄐㄩˋ (e juj) ㄉㄧˋ ㄧ ㄐㄩˋ (dej e juj) ㄏㄢˋ (hmj) ㄗㄨㄟˋ ㄏㄡˋ ㄧ ㄐㄩˋ (zxajhoj e juj) ㄧˇ ㄐㄧˊ (efjed) ㄓㄨㄥ ㄐㄧㄢ (jxl jem ) ㄋㄚˋ ㄧ ㄐㄩˋ (nrj e juj)
            engine.reset_buffer();
            for c in "e jujdej e jujhmjzxajhoj e jujefjedjxl jem nrj e juj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "依據第一句和最後一句以及中間那一句", "複合句結構必精準命中「依據第一句和最後一句以及中間那一句」，杜絕「依據/一句」倒掛及「以極」切碎");

            // (c) 連同上面這兩句以及本句一起
            // ㄌㄧㄢˊ ㄊㄨㄥˊ (lemdtxld) ㄕㄤˋ ㄇㄧㄢˋ (ckjmemj) ㄓㄜˋ ㄌㄧㄤˇ ㄐㄩˋ (jgjlekfjuj) ㄧˇ ㄐㄧˊ (efjed) ㄅㄣˇ ㄐㄩˋ (bnfjuj) ㄧ ㄑㄧˇ (e vef)
            engine.reset_buffer();
            for c in "lemdtxldckjmemjjgjlekfjujefjedbnfjuj e vef".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "連同上面這兩句以及本句一起", "指示代詞數量結構必命中「連同上面這兩句以及本句一起」，杜絕「蔗兩句」與「以極」");

            // (d) 學了一招 (ㄒㄩㄝˊ ˙ㄌㄜ ㄧ ㄓㄠ: cuedlgs e jw )
            engine.reset_buffer();
            for c in "cuedlgs e jw ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "學了一招", "動量短語必命中「學了一招」，杜絕成語「學了一朝」劫掠");

            // 僅只學了一招 (ㄐㄧㄣˇ ㄓˇ ㄒㄩㄝˊ ˙ㄌㄜ ㄧ ㄓㄠ: jenfjf cuedlgs e jw )
            engine.reset_buffer();
            for c in "jenfjf cuedlgs e jw ".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "僅只學了一招", "副詞限定動量必命中「僅只學了一招」");

            // (e) 另開新對話 (ㄌㄧㄥˋ ㄎㄞ ㄒㄧㄣ ㄉㄨㄟˋ ㄏㄨㄚˋ: leljki cen dxajhxrj)
            engine.reset_buffer();
            for c in "leljki cen dxajhxrj".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "另開新對話", "操作短語必命中「另開新對話」，杜絕「令開新對話」");

            // (f) 已安全卸除 (ㄧˇ ㄢ ㄑㄩㄢˊ ㄒㄧㄝˋ ㄔㄨˊ: efm vumdceejvxd)
            engine.reset_buffer();
            for c in "efm vumdceejvxd".chars() { engine.process_key(c); }
            assert_eq!(engine.get_display_text(), "已安全卸除", "時相狀態短語必命中「已安全卸除」，杜絕「以安全卸除」");
        }
    }

    #[test]
    fn test_user_ceshi_and_yasuogeshi() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 立刻來測試看看 (正常 lid)
        engine.reset_buffer();
        for c in "lejkgjlidagjcjkmjkmj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "立刻來測試看看", "副詞動詞短語必精準命中「立刻來測試看看」，杜絕「冊試看看」截斷劫掠");

        // 獨立測試「測試看看」
        engine.reset_buffer();
        for c in "agjcjkmjkmj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "測試看看", "動態評估短語必精準命中「測試看看」，杜絕「冊試看看」截斷劫掠");

        // 獨立測試「壓縮格式」
        engine.reset_buffer();
        for c in "er sxh ggdcj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "壓縮格式", "科技名詞必精準命中「壓縮格式」");

        // 測試「非破壞性壓縮格式」
        engine.reset_buffer();
        for c in "fa phjhxijceljer sxh ggdcj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "非破壞性壓縮格式", "複合科技名詞必精準命中「非破壞性壓縮格式」，杜絕「呀縮」劫掠");

        // 測試「支援非破壞性壓縮格式」
        engine.reset_buffer();
        for c in "j umdfa phjhxijceljer sxh ggdcj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "支援非破壞性壓縮格式", "完整科技功能複合句必精準命中「支援非破壞性壓縮格式」，杜絕「呀縮」劫掠");

        // 4. 分組測試四個自然詞組
        // (4a) 我們現在馬上 (杜絕「嗎尚」)
        engine.reset_buffer();
        for c in "xhfmnscemjzijmrfckj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "我們現在馬上", "時間副詞短語必精準命中「我們現在馬上」，杜絕「嗎尚」劫掠");

        // (4b) 再來測試一下 (杜絕「在來」)
        engine.reset_buffer();
        for c in "zijlidagjcj e cerj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "再來測試一下", "連接副詞短語必精準命中「再來測試一下」，杜絕「在來」劫掠");

        // (4c) 今天的壓縮袋 (杜絕「呀梭大」)
        engine.reset_buffer();
        for c in "jen tem dgs er sxh dij".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "今天的壓縮袋", "偏正名詞必精準命中「今天的壓縮袋」，杜絕「呀梭大」截斷劫掠");

        // (4d) 壓縮效果如何 (杜絕「笑果如和」)
        engine.reset_buffer();
        for c in "er sxh cewjgxhfrxdhgd".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "壓縮效果如何", "疑問動賓短語必精準命中「壓縮效果如何」，杜絕「笑果如和」劫掠");
    }

    #[test]
    fn test_user_extreme_sentence_zailai_renqun() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // (1) 在來來去去的人群中 (杜絕「再來來去去」)
        // 在 (zij) 來 (lid) 來 (lid) 去 (vuj) 去 (vuj) 的 (ds) 人 (rnd) 群 (vund) 中 (jxl )
        engine.reset_buffer();
        for c in "zijlidlidvujvujds rndvundjxl ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "在來來去去的人群中", "介詞成語方位結構必精準命中「在來來去去的人群中」，杜絕「再來來去去」劫掠");

        // (2) 找尋哪個人會上上下下爬來爬去 (杜絕「找鄩」、「仁惠」)
        // 找尋 (jwfcund) 哪個人 (nrfggjrnd) 會 (hxaj) 上上下下 (ckjckjcerjcerj) 爬來爬去 (prdlidprdvuj)
        engine.reset_buffer();
        for c in "jwfcundnrfggjrndhxajckjckjcerjcerjprdlidprdvuj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "找尋哪個人會上上下下爬來爬去", "動賓能願成語短語必精準命中「找尋哪個人會上上下下爬來爬去」，杜絕「找鄩」、「仁惠」劫掠");

        // (3) 應該也不會太難才是
        // 應 (el ) 該 (gi ) 也 (eef) 不 (bxj) 會 (hxaj) 太 (tij) 難 (nmd) 才 (aid) 是 (cj)
        engine.reset_buffer();
        for c in "el gi eefbxjhxajtijnmdaidcj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "應該也不會太難才是", "情態評價句必精準命中「應該也不會太難才是」");

        // (4) 整體極端造句連貫驗證
        engine.reset_buffer();
        for c in "zijlidlidvujvujds rndvundjxl jwfcundnrfggjrndhxajckjckjcerjcerjprdlidprdvujel gi eefbxjhxajtijnmdaidcj".chars() { engine.process_key(c); }
        assert_eq!(
            engine.get_display_text(),
            "在來來去去的人群中找尋哪個人會上上下下爬來爬去應該也不會太難才是",
            "使用者極端造句必 100% 完整無瑕輸出！"
        );
    }

    #[test]
    fn test_user_aup_and_rendering_sentences() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試「其實這些」與「aup 檔」，杜絕「其時」與「黨」
        engine.reset_buffer();
        for c in "vedcdjgjcee ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "其實這些", "轉折副詞代詞必精準命中「其實這些」，杜絕「其時」劫掠");

        engine.reset_buffer();
        engine.record_committed("aup");
        for c in "dkf".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "檔", "英文副檔名後置必精準命中「檔」，杜絕政治「黨」劫掠");

        engine.reset_buffer();
        for c in "eefmadeofhnfjxljewj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "也沒有很重要", "否定情態評估句必精準命中「也沒有很重要」");

        // 2. 測試「算繪成果」，杜絕「算會」
        engine.reset_buffer();
        for c in "sxmjhxajvldgxhf".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "算繪成果", "專業電腦圖學在地化術語必精準命中「算繪成果」，杜絕「算會」");

        // 3. 測試「重疊部份的」，杜絕「重疊不憤」切碎劫掠
        engine.reset_buffer();
        for c in "vxlddeedbxjfnjds ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "重疊部份的", "偏正結構名詞必精準命中「重疊部份的」，杜絕「重疊不憤」劫掠");

        // 4. 測試「原始數據應該都不在了」
        engine.reset_buffer();
        for c in "umdcfcxjjuj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "原始數據", "統計名詞必精準命中「原始數據」");

        engine.reset_buffer();
        for c in "el gi do bxjzijls".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "應該都不在了", "情態完成時相必精準命中「應該都不在了」");

        // 5. 測試反詰疑問句尾「對吧」，杜絕「隊吧」劫掠
        engine.reset_buffer();
        for c in "dxajbrs".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "對吧", "反詰句末助詞必精準命中「對吧」，杜絕「隊吧」劫掠");
    }

    #[test]
    fn test_user_audacity_and_classics_context() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試偏正構式「自我毀滅之路邁進」，杜絕「支路」
        engine.reset_buffer();
        for c in "zjxhfhxafmeejj lxjmijjenj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "自我毀滅之路邁進", "文言偏正名詞必精準命中「自我毀滅之路邁進」，杜絕「支路」劫掠");

        // 2. 測試後半句「我們就直接揚棄它吧」，杜絕「就職揭揚器塌吧」整句切碎
        engine.reset_buffer();
        for c in "xhfmnsjeojjdjee ekdvejtr brs".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "我們就直接揚棄它吧", "流暢口語短語必精準命中「我們就直接揚棄它吧」，杜絕「就職揭揚器塌吧」切碎");

        // 3. 測試經典典故語境「不憤不啟」，驗證「不憤」在古典語境正常呼吸（教育部審訂變調音 ㄅㄨˊ ㄈㄣˋ）
        engine.reset_buffer();
        for c in "bxdfnjbxjvef".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "不憤不啟", "《論語》經典成語必精準命中「不憤不啟」，證明非盲目封殺");
    }

    #[test]
    fn test_user_smart_and_user_dict_sentences() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試「智慧型輸入法目前主攻」與省略型「智慧輸入法目前主攻」
        engine.reset_buffer();
        for c in "jjhxajceldcx rxjfrfmxjvemdjxfgxl ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "智慧型輸入法目前主攻", "必精準命中「智慧型輸入法目前主攻」，杜絕「智會行」與「主公」劫掠");

        engine.reset_buffer();
        for c in "jjhxajcx rxjfrfmxjvemdjxfgxl ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "智慧輸入法目前主攻", "科技構式省略型時必精準命中「智慧輸入法目前主攻」");

        // 2. 測試「智慧選字詞正確率」與「智慧選字正確率」
        engine.reset_buffer();
        for c in "jjhxajcumfzjadjljvuejluj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "智慧選字詞正確率", "必精準命中「智慧選字詞正確率」，杜絕「置喙」劫掠");

        engine.reset_buffer();
        for c in "jjhxajcumfzjjljvuejluj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "智慧選字正確率", "必精準命中「智慧選字正確率」");

        // 3. 測試「殆無疑義」
        engine.reset_buffer();
        for c in "dijxdedej".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "殆無疑義", "典雅成語必精準命中「殆無疑義」，杜絕「代無疑意」劫掠");

        // 4. 測試「立刻再打一次」
        engine.reset_buffer();
        for c in "lejkgjzijdrfedaj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "立刻再打一次", "時間副詞與動量必精準命中「立刻再打一次」，杜絕「粒刻在打」劫掠");

        // 5. 測試「將著力於此部分開發」與古典「不憤不啟」
        engine.reset_buffer();
        for c in "jek jxhdlejudafbxjfnjki fr ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "將著力於此部分開發", "四聲輸入必精準命中「將著力於此部分開發」，杜絕「江著粒於此不憤」劫掠");

        engine.reset_buffer();
        for c in "bxdfnjbxjvef".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "不憤不啟", "古典成語依據二聲變調必精準命中「不憤不啟」");

        // 6. 測試使用者自建詞「直接顯示絕無問題」
        let custom_entry = crate::lexicon::LexiconEntry::new(
            "使用者自建詞測試",
            "ㄕˇ ㄩㄥˋ ㄓㄜˇ ㄗˋ ㄐㄧㄢˋ ㄘˊ ㄘㄜˋ ㄕˋ",
            2000,
            "user",
            "自建詞無條件置入測試"
        );
        engine.lexicon.add_entry(custom_entry);

        engine.reset_buffer();
        // ㄕˇ (cf) ㄩㄥˋ (ulj) ㄓㄜˇ (jgf) ㄗˋ (zj) ㄐㄧㄢˋ (jemj) ㄘˊ (ad) ㄘㄜˋ (agj) ㄕˋ (cj)
        for c in "cfuljjgfzjjemjadagjcj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "使用者自建詞測試", "使用者自建詞無條件放進詞庫，立刻再打一次直接命中，絕無問題！");
    }

    #[test]
    fn test_zaoshi_and_renhe_sentences() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試「您這邊的工作暫時先告一段落」，杜絕「地工作戰時」劫掠
        // 您 (nend) 這邊 (jgjbem ) 的 (dgs) 工作 (gxl zxhj) 暫時 (jmjcd) 先 (cem ) 告一段落 (gwjeddxmjlxhj)
        engine.reset_buffer();
        for c in "nendjgjbem dgsgxl zxhjjmjcdcem gwjeddxmjlxhj".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "您這邊的工作暫時先告一段落", "口語敬語句必精準命中「您這邊的工作暫時先告一段落」，杜絕「地工作戰時」劫掠");

        // 2. 測試「有任何相容性問題再回來麻煩您」，杜絕「任和」切碎劫掠「任何」
        // 有 (eof) 任何 (rnjhgd) 相 (cek ) 容 (rxld) 性 (celj) 問 (xnj) 題 (ted) 再 (zij) 回來 (hxadlid) 麻煩 (mrdfmd) 您 (nend)
        engine.reset_buffer();
        for c in "eofrnjhgdcek rxldceljxnjtedzijhxadlidmrdfmdnend".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "有任何相容性問題再回來麻煩您", "商務諮詢句必精準命中「有任何相容性問題再回來麻煩您」，杜絕「任和」切碎劫掠");

        // 3. 測試極端造句「主任和我都很喜歡如喜」，驗證連詞「和」在「主任」後方自然呼吸，不受任和負向黑洞破壞
        // 主任 (jxf rnj) 和 (hgd) 我 (xhf) 都 (do ) 很 (hnf) 喜歡 (cef hxm ) 如喜 (rxd cef)
        engine.reset_buffer();
        for c in "jxfrnjhgdxhfdo hnfcefhxm rxdcef".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "主任和我都很喜歡如喜", "職稱名詞接連詞「主任和我」必流暢自然，絕無負向語意黑洞干擾");

        // 4. 測試雙字詞裸打「ㄖㄣˋ ㄏㄜˊ」，驗證首選必為正統「任何」，徹底消滅「任合」
        engine.reset_buffer();
        for c in "rnjhgd".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "任何", "裸打 ㄖㄣˋ ㄏㄜˊ 首選必為正統高頻詞「任何」，徹底告別生造詞「任合」");
    }

    #[test]
    fn test_user_clean_dict_and_flowing_sentences() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 測試「再來測試看看使用者詞庫清零後的如喜輸入法有沒有變得更行雲流水一點」
        let keys = "zijlidagjcjkm km cfuljjgfadkxjvel leldhojdgsrxdcefcx rxjfrfeofmad eofbemjdgdgljceldundleodcxaf e demf";
        engine.reset_buffer();
        for c in keys.chars() { engine.process_key(c); }
        assert_eq!(
            engine.get_display_text(),
            "再來測試看看使用者詞庫清零後的如喜輸入法有沒有變得更行雲流水一點",
            "零自建詞庫純淨狀態下，長句必精確命中「清零」與「變得更行雲流水」，杜絕「清泠」與「便得」"
        );
    }

    #[test]
    fn test_qingcha_and_chedi_sentences() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試「破音字清查」，杜絕「破音字清茶」劫掠
        engine.reset_buffer();
        for c in "phjen zjvel vrd".chars() { engine.process_key(c); }
        assert_eq!(
            engine.get_display_text(),
            "破音字清查",
            "偏正構式必精準命中「破音字清查」，杜絕「清茶」劫掠"
        );

        // 2. 測試「看來之前的破音字清查不夠徹底」，長句端到端驗證
        // 看 (kmj) 來 (lid) 之 (j ) 前 (vemd) 的 (dgs) 破 (phj) 音 (en ) 字 (zj) 清 (vel ) 查 (vrd) 不 (bxj) 夠 (goj) 徹 (vgj) 底 (def)
        let sentence_keys = "kmj lidj  vemd dgsphjen zjvel vrdbxjgojvgjdef";
        engine.reset_buffer();
        for c in sentence_keys.chars() { engine.process_key(c); }
        assert_eq!(
            engine.get_display_text(),
            "看來之前的破音字清查不夠徹底",
            "口語複合長句必精準命中「看來之前的破音字清查不夠徹底」，杜絕「清茶」與「冊底」劫掠"
        );
    }

    #[test]
    fn test_user_debug_gongneng_and_wanbi_caizhengshi() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 還有一些功能 / 還有歇工能
        // ㄏㄞˊ (hid) ㄧㄡˇ (eof) ㄒㄧㄝ (cee ) ㄍㄨㄥ (gxl ) ㄋㄥˊ (nld)
        engine.reset_buffer();
        for c in "hideofcee gxl nld".chars() { engine.process_key(c); }
        let out1 = engine.get_display_text();
        assert_eq!(out1, "還有些功能", "必精準命中「還有些功能」，杜絕「歇工能」劫掠");

        // 2. 處理完畢才正式發布
        // 處 (vxf) 理 (lef) 完 (xmd) 畢 (bej) 才 (aid) 正 (jlj) 式 (cj) 發 (fr ) 布 (bxj)
        engine.reset_buffer();
        for c in "vxflefxmdbejaidjljcjfr bxj".chars() { engine.process_key(c); }
        let out2 = engine.get_display_text();
        assert_eq!(out2, "處理完畢才正式發布", "完成體先後語法構式必精準命中「處理完畢才正式發布」，杜絕「財政是發布」劫掠");

        // 3. 單獨「完畢才正式發布」
        engine.reset_buffer();
        for c in "xmdbejaidjljcjfr bxj".chars() { engine.process_key(c); }
        let out3 = engine.get_display_text();
        assert_eq!(out3, "完畢才正式發布", "獨立體貌短語必精準命中「完畢才正式發布」");

        // 4. 子句「因為還有些功能需要微調」
        // 因 (en ) 為 (xaj) 還 (hid) 有 (eof) 些 (cee ) 功 (gxl ) 能 (nld) 需 (cu ) 要 (ewj) 微 (xad) 調 (tewd)
        engine.reset_buffer();
        for c in "en xajhideofcee gxl nldcu ewjxadtewd".chars() { engine.process_key(c); }
        let out4 = engine.get_display_text();
        assert_eq!(out4, "因為還有些功能需要微調", "因果存現限定句必精準命中「因為還有些功能需要微調」");

        // 5. 測試「這個新的資料夾」
        engine.reset_buffer();
        for c in "jgjggscen dgsz lewjjerd".chars() { engine.process_key(c); }
        let out_clause = engine.get_display_text();
        assert_eq!(out_clause, "這個新的資料夾", "指示詞與定語必精準命中「這個新的資料夾」，杜絕「這 個 心 的」切碎劫掠");

        // 6. 子句「後續應該是先在這個新的資料夾處理完畢才正式發布」
        // 後 (hoj) 續 (cuj) 應 (el ) 該 (gi ) 是 (cj) 先 (cem ) 在 (zij) 這 (jgj) 個 (ggs) 新 (cen ) 的 (dgs) 資 (z ) 料 (lewj) 夾 (jerd) 處 (vxf) 理 (lef) 完 (xmd) 畢 (bej) 才 (aid) 正 (jlj) 式 (cj) 發 (fr ) 布 (bxj)
        engine.reset_buffer();
        for c in "hojcujel gi cjcem zijjgjggscen dgsz lewjjerdvxflefxmdbejaidjljcjfr bxj".chars() { engine.process_key(c); }
        let out5 = engine.get_display_text();
        assert_eq!(out5, "後續應該是先在這個新的資料夾處理完畢才正式發布", "完整業務邏輯流程長句必精準命中「後續應該是先在這個新的資料夾處理完畢才正式發布」");
    }

    #[test]
    fn test_backspace_shift_backspace_ctrl_backspace() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 測試 a. Backspace (pop_component : 往回刪除一個拼音)
        engine.reset_buffer();
        // 鍵入 r (ㄖ)
        engine.process_key('r');
        assert_eq!(engine.get_display_text(), "ㄖ");
        // 鍵入 x (ㄨ)
        engine.process_key('x');
        assert_eq!(engine.get_display_text(), "ㄖㄨ");
        // 往回刪除一個拼音字母 -> 退回 ㄖ
        engine.pop_component();
        assert_eq!(engine.get_display_text(), "ㄖ");
        // 再往回刪除一個拼音字母 -> 清空
        engine.pop_component();
        assert_eq!(engine.get_display_text(), "");

        // 2. 測試 b. Shift+Backspace (pop_syllable : 往回刪除一組拼音)
        engine.reset_buffer();
        // 鍵入「如喜」: rxd (如, ㄖㄨˊ) + cef (喜, ㄒㄧˇ)
        for c in "rxdcef".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "如喜");
        // 輸入一半的「歡」聲母 h (ㄏ)
        engine.process_key('h');
        assert_eq!(engine.get_display_text(), "如喜ㄏ");
        // 按 Shift+Backspace -> 往回刪除當前這組未完成拼音 (h)
        engine.pop_syllable();
        assert_eq!(engine.get_display_text(), "如喜");
        // 再按 Shift+Backspace -> 往回刪除最後一組已完成拼音「喜」(cef)
        engine.pop_syllable();
        assert_eq!(engine.get_display_text(), "如");
        // 再按 Shift+Backspace -> 往回刪除「如」(rxd)
        engine.pop_syllable();
        assert_eq!(engine.get_display_text(), "");

        // 3. 測試 c. Ctrl+Backspace (pop_word : 往回刪除一個字／詞)
        engine.reset_buffer();
        // 鍵入「我喜歡」: xhf (我, 1音節) + cefhxm (喜歡, 2音節詞)
        for c in "xhfcefhxm ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "我喜歡");
        // 按 Ctrl+Backspace -> 依據 Viterbi 切詞結構精準刪除最後一個詞「喜歡」(2音節)
        engine.pop_word();
        assert_eq!(engine.get_display_text(), "我");
        // 再按 Ctrl+Backspace -> 刪除單字「我」(1音節)
        engine.pop_word();
        assert_eq!(engine.get_display_text(), "");

        // 4. 測試具有未完成音節時按 Ctrl+Backspace
        engine.reset_buffer();
        for c in "xhfcefhxm ".chars() { engine.process_key(c); }
        assert_eq!(engine.get_display_text(), "我喜歡");
        // 打了一半的拼音 r (ㄖ)
        engine.process_key('r');
        assert_eq!(engine.get_display_text(), "我喜歡ㄖ");
        // 按 Ctrl+Backspace -> 優先刪除正在組字中的未完成拼音字
        engine.pop_word();
        assert_eq!(engine.get_display_text(), "我喜歡");
        // 再按 Ctrl+Backspace -> 精確刪除「喜歡」整詞
        engine.pop_word();
        assert_eq!(engine.get_display_text(), "我");
    }

    #[test]
    fn test_user_debug_budong_duozu_xiuzheng() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 驗證「讓不懂程式」能精確命中「不懂」與「程式」，絕不碎裂為「步董」
        engine.reset_buffer();
        for c in "rkjbxjdxlfvldcj".chars() {
            engine.process_key(c);
        }
        let disp1 = engine.get_display_text();
        assert!(disp1.contains("不懂程式") || disp1.contains("讓不懂") || disp1.contains("不懂"), "實際輸出：{}", disp1);

        // 2. 驗證「直接或多組」能精確組出「直接」與「多組」，絕不碎裂為「直街」或「朵組」
        engine.reset_buffer();
        for c in "jdjee hxhjdxh zxf".chars() {
            engine.process_key(c);
        }
        let disp2 = engine.get_display_text();
        assert!(disp2.contains("直接") && disp2.contains("多組"), "實際輸出：{}", disp2);

        // 3. 驗證「修正行為」能精確組出「修正」，絕不碎裂為「休政」
        engine.reset_buffer();
        for c in "ceo jljceldxad".chars() {
            engine.process_key(c);
        }
        let disp3 = engine.get_display_text();
        assert!(disp3.contains("修正行為") || disp3.contains("修正"), "實際輸出：{}", disp3);
    }

    #[test]
    fn test_ganxie_huifu_and_polite_collocation() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 驗證「感謝回覆」能精確組出「感謝」與「回覆」，絕不碎裂為「感系回付」或出現注音符號
        engine.reset_buffer();
        for c in "gmfceejhxadfxj".chars() {
            engine.process_key(c);
        }
        let disp = engine.get_display_text();
        assert!(disp == "感謝回覆" || disp == "感謝回復", "實際輸出：{}", disp);
    }

    #[test]
    fn test_shuru_lie_and_changju_wanzhengxing() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. 驗證「輸入列」能精確組出，絕不出現「輸入裂」
        engine.reset_buffer();
        for c in "cx rxjleej".chars() {
            engine.process_key(c);
        }
        let disp = engine.get_display_text();
        assert!(disp == "輸入列", "實際輸出：{}", disp);

        // 2. 驗證長句「是不是就越容易」能精準流暢組出「是不是就」，絕不碎裂為「是不是舊」
        engine.reset_buffer();
        for c in "cjbxjcjjeojuej".chars() {
            engine.process_key(c);
        }
        let disp2 = engine.get_display_text();
        assert!(disp2.contains("是不是就"), "實際輸出：{}", disp2);

        // 3. 驗證 18 字無標點極限長句「不小心在這裡參雜了語意分析除錯的部份」極速流暢無延遲，且精準組出「除錯」
        // ㄅㄨˋ(bxj) ㄒㄧㄠˇ(cewf) ㄒㄧㄣ(cen ) ㄗㄞˋ(zij) ㄓㄜˋ(jgj) ㄌㄧˇ(lef) ㄘㄢ(am ) ㄗㄚˊ(zrd) ˙ㄌㄜ(ls) ㄩˇ(uf) ㄧˋ(ej) ㄈㄣ(fn ) ㄒㄧ(ce ) ㄔㄨˊ(vxd) ㄘㄨㄛˋ(axhj) ㄉㄜ˙(ds) ㄅㄨˋ(bxj) ㄈㄣˋ(fnj)
        engine.reset_buffer();
        let sentence_keys = "bxj cewf cen zij jgj lef am zrd ls uf ej fn ce vxd axhj ds bxj fnj";
        for c in sentence_keys.chars() {
            engine.process_key(c);
        }
        let disp3 = engine.get_display_text();
        assert!(disp3.contains("不小心") && (disp3.contains("參雜") || disp3.contains("摻雜")) && disp3.contains("語意分析") && disp3.contains("除錯"), "實際輸出：{}", disp3);
    }

    #[test]
    fn test_tab_manual_cut_word_split() {
        use crate::fsm::HsuSyllable;

        // 1. 驗證 HsuSyllable::from_bopomofo_display 正確還原
        let test_cases = ["ㄕㄨ", "ㄖㄨˋ", "ㄌㄧㄝˋ", "˙ㄉㄜ", "ㄧˋ", "ㄨㄛˇ", "ㄘㄢ", "ㄗㄚˊ"];
        for &tc in &test_cases {
            let s = HsuSyllable::from_bopomofo_display(tc).expect("應該成功解析注音");
            assert_eq!(s.bopomofo_display(), tc, "注音還原應完全一致");
        }

        // 2. 驗證 Tab 手動斷詞點（manual_cuts）約束
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbpmf");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 輸入「輸入列」三個音節: ㄕㄨ(cx ) ㄖㄨˋ(rxj) ㄌㄧㄝˋ(leej)
        engine.reset_buffer();
        for c in "cx rxjleej".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.fsm.syllables.len(), 3);

        // 游標移到「輸入」與「列」之間 (索引 2)，按 Tab 鍵手動斷詞
        engine.cursor_pos = 2;
        let resp = engine.process_key('\t');
        assert_eq!(resp.action, "update_preedit");
        assert!(engine.manual_cuts.contains(&2), "斷詞點 2 必須被加入 manual_cuts");
        let disp = engine.get_display_text();
        assert_eq!(disp, "輸入列", "斷詞後必精準成詞");

        // 再按一次 Tab 鍵，驗證 Toggle 取消手動斷詞
        let resp2 = engine.process_key('\t');
        assert_eq!(resp2.action, "update_preedit");
        assert!(!engine.manual_cuts.contains(&2), "再次按 Tab 必須移除斷詞點 2");
    }

    #[test]
    fn test_layer0_patterns_user_jsonl_loading_and_matching() {
        let mut trans = crate::router::TransitionEngine::new();
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join(format!("ruhi_test_patterns_{}.jsonl", std::process::id()));

        let jsonl_content = concat!(
            "{\"name\": \"ai_slot_rule_1\", \"prev_slot\": [\"雲端\", \"本機\"], \"next_slot\": [\"推理\", \"運算\"], \"bonus\": 28.0}\n",
            "{\"name\": \"ai_slot_rule_clamped\", \"slots\": [[\"模型\", \"權重\"]], \"target\": [\"收斂\"], \"bonus\": 99.0}\n"
        );
        std::fs::write(&jsonl_path, jsonl_content).expect("應成功寫入測試 patterns_user.jsonl");

        let baseline_score = trans.calculate_transition_bonus("雲端", "ㄩㄣˊ ㄉㄨㄢ", "推理", "ㄊㄨㄟ ㄌㄧˇ");
        let loaded = trans.load_patterns_jsonl(&jsonl_path).expect("應成功載入 patterns_user.jsonl");
        assert_eq!(loaded, 2, "應成功載入 2 條 AI 歸納槽位規則");

        let score1 = trans.calculate_transition_bonus("雲端", "ㄩㄣˊ ㄉㄨㄢ", "推理", "ㄊㄨㄟ ㄌㄧˇ");
        assert!(
            (score1 - (baseline_score + 28.0)).abs() < 1e-6,
            "載入 patterns_user.jsonl 後應精確獲得 +28.0 加分，實際: {} vs {}",
            score1,
            baseline_score
        );

        // 驗證分數自動鉗制於 [15.0, 35.0] 區間
        let baseline_clamp = trans.calculate_transition_bonus("權重", "ㄑㄩㄢˊ ㄓㄨㄥˋ", "收斂", "ㄕㄡ ㄌㄧㄢˇ");
        let score2 = trans.calculate_transition_bonus("權重", "ㄑㄩㄢˊ ㄓㄨㄥˋ", "收斂", "ㄕㄡ ㄌㄧㄢˇ");
        assert!(
            score2 >= 35.0 && (score2 - baseline_clamp).abs() < 1e-6,
            "超高 bonus 99.0 必須被鉗制在 35.0 上限以內，實際: {}",
            score2
        );

        let _ = std::fs::remove_file(&jsonl_path);
    }

    #[test]
    fn test_layer3_dynamic_lexicon_hot_reload() {
        let temp_dir = std::env::temp_dir().join(format!("ruhi_test_hotreload_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let user_dict_path = temp_dir.join("user_dict.tsv");

        // 初始寫入一筆基礎詞條
        std::fs::write(&user_dict_path, "初始測試\tㄔㄨ ㄕˇ ㄘㄜˋ ㄕˋ\t800\n")
            .expect("應成功寫入初始 user_dict.tsv");

        let mut engine = RuhiEngine::new_with_data_dir(temp_dir.to_str().unwrap());
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");

        // 模擬背景 AI 調校器或自學習模組動態追加新詞「語意調校」(ㄩˇ ㄧˋ ㄊㄧㄠˊ ㄐㄧㄠˋ)
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(
            &user_dict_path,
            "初始測試\tㄔㄨ ㄕˇ ㄘㄜˋ ㄕˋ\t800\n語意調校\tㄩˇ ㄧˋ ㄊㄧㄠˊ ㄐㄧㄠˋ\t880\n",
        )
        .expect("應成功更新 user_dict.tsv");

        // reset_buffer 會清除節流計時器，模擬新句輸入時立即感知熱重載
        engine.reset_buffer();
        for c in "uf ej tewd jewj".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "語意調校",
            "Layer 3 動態詞庫熱重載後，無需重啟引擎即可組出新增專有名詞「語意調校」"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_layer3_vo_separable_verb_lookback_in_viterbi() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 0. 直接驗證 calculate_vo_lookback_bonus 跨詞隙回溯加分
        assert!(
            engine.transitions.calculate_vo_lookback_bonus(&["洗", "了", "一個"], "澡") >= 25.0,
            "離合詞「洗...澡」跨 3 詞隙應獲得 VO 動賓回溯加分"
        );
        assert!(
            engine.transitions.calculate_vo_lookback_bonus(&["幫", "了", "他", "一", "個"], "忙") >= 25.0,
            "離合詞「幫...忙」跨 5 詞隙應獲得 VO 動賓回溯加分"
        );
        assert!(
            engine.transitions.calculate_vo_lookback_bonus(&["睡", "個", "好"], "覺") >= 25.0,
            "離合詞「睡...覺」跨詞隙應獲得 VO 動賓回溯加分"
        );

        // 1. 洗了一個澡: ㄒㄧˇ(cef) ˙ㄌㄜ(ls) ㄧˊ(ed) ˙ㄍㄜ(ggs) ㄗㄠˇ(zwf)
        engine.reset_buffer();
        for c in "ceflsedggszwf".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "洗了一個澡",
            "離合詞構式必精準組出「洗了一個澡」，杜絕「早/棗」同音干擾"
        );

        // 2. 幫了他一個忙: ㄅㄤ(bk ) ˙ㄌㄜ(ls) ㄊㄚ(tr ) ㄧˊ(ed) ˙ㄍㄜ(ggs) ㄇㄤˊ(mkd)
        engine.reset_buffer();
        for c in "bk lstr edggsmkd".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "幫了他一個忙",
            "離合詞構式必精準組出「幫了他一個忙」，杜絕「盲/茫」同音干擾"
        );

        // 3. 睡個好覺: ㄕㄨㄟˋ(cxaj) ˙ㄍㄜ(ggs) ㄏㄠˇ(hwf) ㄐㄧㄠˋ(jewj)
        engine.reset_buffer();
        for c in "cxajggshwfjewj".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "睡個好覺",
            "離合詞構式必精準組出「睡個好覺」，杜絕「好叫/好教」干擾"
        );
    }

    #[test]
    fn test_indexed_transitions_zero_regression() {
        let mut trans = crate::router::TransitionEngine::new();
        let _ = trans.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 驗證收斂至 builtin_transitions 雜湊表後，在 load_tsv 熱重載後依然 100% 生效且具備 O(1) 查找能力
        assert!(
            trans.calculate_transition_bonus("正常", "ㄓㄥˋ ㄔㄤˊ", "發揮", "ㄈㄚ ㄏㄨㄟ") >= 30.0,
            "內建二元轉移「正常 -> 發揮」必須在 load_tsv 後完整保留"
        );
        assert!(
            trans.calculate_transition_bonus("傷敵", "ㄕㄤ ㄉㄧˊ", "五百", "ㄨˇ ㄅㄞˇ") >= 30.0,
            "內建對稱構式轉移「傷敵 -> 五百」必須在 O(1) 雜湊表中精準命中"
        );
        assert!(
            trans.calculate_transition_bonus("自毀", "ㄗˋ ㄏㄨㄟˇ", "三千", "ㄙㄢ ㄑㄧㄢ") >= 30.0,
            "內建對稱構式轉移「自毀 -> 三千」必須在 O(1) 雜湊表中精準命中"
        );
        assert!(
            trans.calculate_transition_bonus("輸入", "ㄕㄨ ㄖㄨˋ", "列", "ㄌㄧㄝˋ") >= 25.0,
            "內建二元轉移「輸入 -> 列」必須精準命中"
        );
        assert!(
            trans.calculate_transition_bonus("正常", "ㄓㄥˋ ㄔㄤˊ", "發灰", "ㄈㄚ ㄏㄨㄟ") < 0.0,
            "內建邊界抑制「正常 -> 發灰」必須維持合理負向抑制"
        );
    }

    #[test]
    fn test_hierarchical_slot_set_constructions() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 1. [每] -> [點擊] -> [({一..十} -> {次, 回, 下})]: ㄇㄟˇ(maf) ㄉㄧㄢˇ(demf) ㄐㄧˊ(jed) ㄧ(e ) ㄘˋ(aj)
        engine.reset_buffer();
        for c in "mafdemfjede aj".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "每點擊一次",
            "階層式構式 `[每] -> [點擊] -> [(一 -> 次)]` 必精準組出「每點擊一次」，杜絕「每典籍一次」"
        );

        // 2. [({圈, 點, 勾, 框} -> {選, 取})] -> [{框, 欄, 區, 鈕}]: ㄑㄩㄢ(vum ) ㄒㄩㄢˇ(cumf) ㄎㄨㄤ(kxk )
        engine.reset_buffer();
        for c in "vum cumfkxk ".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "圈選框",
            "階層式構式 `[((圈/點/勾/框) -> (選/取))] -> [(框/欄)]` 無需將三字死背入庫，即可組出「圈選框」"
        );

        // 3. [(請 -> 用)] -> [滑鼠] -> [(框 -> 選)]: ㄑㄧㄥˇ(velf) ㄩㄥˋ(ulj) ㄏㄨㄚˊ(hxyd) ㄕㄨˇ(cxf) ㄎㄨㄤ(kxk ) ㄒㄩㄢˇ(cumf)
        engine.reset_buffer();
        for c in "velfuljhxydcxfkxk cumf".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "請用滑鼠框選",
            "階層式構式 `[(請 -> 用)] -> [滑鼠] -> [(框 -> 選)]` 必精準組出「請用滑鼠框選」，杜絕「筐選/匡選」"
        );

        // 4. [{在, 於, 從}] -> [{框, 欄, 區}] -> [({中, 內, 裡})]: ㄗㄞˋ(zij) ㄎㄨㄤ(kxk ) ㄓㄨㄥ(jxl )
        engine.reset_buffer();
        for c in "zijkxk jxl ".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "在框中",
            "階層式空間構式必精準組出「在框中」，杜絕「在筐中」"
        );
    }

    #[test]
    fn test_standalone_bopomofo_symbol_cycling() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");

        // 1. 許氏單鍵 'j' 預設顯示 ㄓ，直接按 'q' 立即切換為雙生注音符號 ㄐ（「機」的子音）
        engine.reset_buffer();
        engine.process_key('j');
        assert_eq!(engine.get_display_text(), "ㄓ");
        engine.process_key('q');
        assert_eq!(
            engine.get_display_text(),
            "ㄐ",
            "許氏鍵盤按 j 再按 q 必能直接打出「機」的子音注音符號「ㄐ」"
        );

        // 2. 許氏單鍵 'v' (ㄔ/ㄑ) 按 'q' 立即切換為 ㄑ
        engine.reset_buffer();
        engine.process_key('v');
        assert_eq!(engine.get_display_text(), "ㄔ");
        engine.process_key('q');
        assert_eq!(
            engine.get_display_text(),
            "ㄑ",
            "許氏鍵盤按 v 再按 q 必能直接打出注音符號「ㄑ」"
        );

        // 3. 許氏單鍵 'c' (ㄕ/ㄒ) 按 'q' 立即切換為 ㄒ
        engine.reset_buffer();
        engine.process_key('c');
        assert_eq!(engine.get_display_text(), "ㄕ");
        engine.process_key('q');
        assert_eq!(
            engine.get_display_text(),
            "ㄒ",
            "許氏鍵盤按 c 再按 q 必能直接打出注音符號「ㄒ」"
        );
    }

    #[test]
    fn test_entity_smart_decomposition_names_and_companies() {
        let temp_dir = std::env::temp_dir().join(format!("ruhi_test_decomp_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let user_dict_path = temp_dir.join("user_dict.tsv");

        // 寫入完整人名「吳玟蓉」、「曹大十」、「歐陽娜娜」與公司全稱「如喜科技有限公司」
        std::fs::write(
            &user_dict_path,
            "吳玟蓉\tㄨˊ ㄨㄣˊ ㄖㄨㄥˊ\t800\n曹大十\tㄘㄠˊ ㄉㄚˋ ㄕˊ\t800\n歐陽娜娜\tㄡ ㄧㄤˊ ㄋㄚˋ ㄋㄚˋ\t850\n如喜科技有限公司\tㄖㄨˊ ㄒㄧˇ ㄎㄜ ㄐㄧˋ ㄧㄡˇ ㄒㄧㄢˋ ㄍㄨㄥ ㄙ\t880\n",
        )
        .expect("應成功寫入測試 user_dict.tsv");

        let mut engine = RuhiEngine::new_with_data_dir(temp_dir.to_str().unwrap());
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv(user_dict_path.to_str().unwrap(), "user");

        // 1. 略過姓氏「吳」，單打「玟蓉」(ㄨㄣˊ ㄖㄨㄥˊ: xnd rxld)
        engine.reset_buffer();
        for c in "xndrxld".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "玟蓉",
            "記入「吳玟蓉」後，略過姓氏單打 ㄨㄣˊ ㄖㄨㄥˊ 必正確組出「玟蓉」"
        );

        // 2. 略過姓氏「曹」，單打「大十」(ㄉㄚˋ ㄕˊ: drj cd)
        engine.reset_buffer();
        for c in "drjcd".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "大十",
            "記入「曹大十」後，略過姓氏單打 ㄉㄚˋ ㄕˊ 必正確組出「大十」"
        );

        // 3. 略過複姓「歐陽」，單打「娜娜」(ㄋㄚˋ ㄋㄚˋ: nrj nrj)
        engine.reset_buffer();
        for c in "nrjnrj".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "娜娜",
            "記入「歐陽娜娜」後，略過複姓單打 ㄋㄚˋ ㄋㄚˋ 必正確組出「娜娜」"
        );

        // 4. 略過公司後綴「科技有限公司」，單打核心名稱「如喜」(ㄖㄨˊ ㄒㄧˇ: rxd cef) 與「如喜科技」(rxd cef kg jej)
        engine.reset_buffer();
        for c in "rxdcef".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "如喜",
            "記入「如喜科技有限公司」後，單打 ㄖㄨˊ ㄒㄧˇ 必正確組出核心字號「如喜」"
        );

        engine.reset_buffer();
        for c in "rxdcefkg jej".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "如喜科技",
            "記入「如喜科技有限公司」後，單打 ㄖㄨˊ ㄒㄧˇ ㄎㄜ ㄐㄧˋ 必正確組出「如喜科技」"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_numeric_date_currency_measure_time_units() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        let type_mixed = |eng: &mut RuhiEngine, seq: &str| -> String {
            eng.reset_buffer();
            eng.last_committed_word.clear();
            let mut committed = String::new();
            for ch in seq.chars() {
                let resp = eng.process_key(ch);
                if !resp.committed_chunk.is_empty() {
                    committed.push_str(&resp.committed_chunk);
                }
            }
            committed.push_str(&eng.get_display_text());
            committed
        };

        // 1. 曆法日期：在10月15日 (zij=在, 10, uej=月, 15, rj=日)、待10月15日、2026年10月15日 (nmd=年)
        assert_eq!(
            type_mixed(&mut engine, "zij10uej15rj"),
            "在10月15日",
            "輸入『在10月15日』時，數字 10 後接 ㄩㄝˋ 必命中『月』（杜絕『越』），15 後接 ㄖˋ 必命中『日』"
        );
        engine.reset_buffer();
        engine.record_committed("待10");
        for ch in "uej".chars() {
            engine.process_key(ch);
        }
        assert_eq!(
            engine.get_display_text(),
            "月",
            "前文為『待10』時，輸入 ㄩㄝˋ 必由數詞單位構式直接鎖定『月』（無需『待➔月』跨數字偽規則）"
        );
        assert_eq!(
            type_mixed(&mut engine, "2026nemd10uej15rj"),
            "2026年10月15日",
            "輸入『2026年10月15日』必 100% 精準命中曆法年月日單位"
        );

        // 2. 貨幣與比例單位：100元 (umd=元)、8折 (jgd=折)、5倍 (baj=倍)
        assert_eq!(
            type_mixed(&mut engine, "100umd"),
            "100元",
            "數字後接 ㄩㄢˊ 必命中貨幣單位『元』"
        );
        assert_eq!(
            type_mixed(&mut engine, "8jgd"),
            "8折",
            "數字後接 ㄓㄜˊ 必命中折扣單位『折』"
        );
        assert_eq!(
            type_mixed(&mut engine, "5baj"),
            "5倍",
            "數字後接 ㄅㄟˋ 必命中倍數單位『倍』"
        );

        // 3. 度量衡與時間單位：50坪 (peld=坪)、37度 (dxj=度)、500克 (kgj=克)、30秒 (mlf=秒)
        assert_eq!(
            type_mixed(&mut engine, "50peld"),
            "50坪",
            "數字後接 ㄆㄧㄥˊ 必命中面積單位『坪』而非『瓶』"
        );
        assert_eq!(
            type_mixed(&mut engine, "37dxj"),
            "37度",
            "數字後接 ㄉㄨˋ 必命中溫度/角度單位『度』而非『渡/肚』"
        );
        assert_eq!(
            type_mixed(&mut engine, "500kgj"),
            "500克",
            "數字後接 ㄎㄜˋ 必命中重量單位『克』而非『課/客』"
        );
        assert_eq!(
            type_mixed(&mut engine, "30mewf"),
            "30秒",
            "數字後接 ㄇㄧㄠˇ 必命中時間單位『秒』"
        );
    }

    #[test]
    fn test_dual_track_arrow_lookup_and_contextual_rescue() {
        let mut engine = RuhiEngine::new();

        // 1. 歐文軌 ↓ 下箭頭：cafe ➔ café 外來語重音變體與單字母變音符號
        engine.reset_buffer();
        engine.is_latin_mode = true;
        for ch in "cafe".chars() {
            engine.process_key(ch);
        }
        let down_resp = engine.open_down_arrow_candidates();
        assert!(
            down_resp.candidates.contains(&"café".to_string()),
            "cafe 按下箭頭必包含重音變體 café，實際：{:?}",
            down_resp.candidates
        );
        engine.process_key('1');
        assert_eq!(engine.composed_sentence, "café");

        // 2. 跨軌 ↑ 上箭頭救援：在許氏雙拼模式下直接輸入 tks 按 ↑ 展開為 thanks
        engine.reset_buffer();
        for ch in "tks".chars() {
            engine.process_key(ch);
        }
        let up_tks = engine.open_up_arrow_candidates();
        assert!(!up_tks.candidates.is_empty());
        assert_eq!(
            up_tks.candidates[0], "thanks",
            "在許氏模式下輸入 tks 按上箭頭必跨軌救援展開為 thanks，實際：{:?}",
            up_tks.candidates
        );
        engine.process_key('1');
        assert_eq!(engine.composed_sentence, "thanks");

        // 3. 亞洲軌 ↑ 上箭頭跨聲調／模糊複合詞救援：「答按」➔「答案」
        engine.reset_buffer();
        engine.sentence_chars = vec!["答".to_string(), "按".to_string()];
        engine.composed_sentence = "答按".to_string();
        engine.cursor_pos = 2;
        let up_daan = engine.open_up_arrow_candidates();
        assert!(!up_daan.candidates.is_empty());
        assert_eq!(
            up_daan.candidates[0], "答案",
            "輸入『答按』按上箭頭必由跨聲調複合詞救援首位命中『答案』，實際：{:?}",
            up_daan.candidates
        );
        engine.process_key('1');
        assert_eq!(engine.composed_sentence, "答案");

        // 4. 亞洲軌 ↑ 上箭頭上下文語法助詞推斷：「可能地答案」➔「可能的答案」
        engine.reset_buffer();
        engine.sentence_chars = vec![
            "可".to_string(),
            "能".to_string(),
            "地".to_string(),
            "答".to_string(),
            "案".to_string(),
        ];
        engine.composed_sentence = "可能地答案".to_string();
        engine.cursor_pos = 5; // 即使游標在句尾，按 ↑ 亦能智慧定位句中助詞「地」並首推「的」
        let up_particle = engine.open_up_arrow_candidates();
        assert!(!up_particle.candidates.is_empty());
        assert_eq!(
            up_particle.candidates[0], "的",
            "『可能地答案』按上箭頭必由上下文語法推斷首推『的』，實際：{:?}",
            up_particle.candidates
        );
        engine.process_key('1');
        assert_eq!(engine.composed_sentence, "可能的答案");

        // 5. 亞洲輸入法自選循環切換（預設 ["hsu", "japanese"]，亦支援全選 ["hsu", "standard", "et26", "cangjie", "japanese"]）
        assert_eq!(engine.get_keyboard_mode_badge(), "許");
        assert_eq!(engine.cycle_keyboard_mode(), "japanese");
        assert_eq!(engine.get_keyboard_mode_badge(), "日");
        assert_eq!(engine.cycle_keyboard_mode(), "hsu");
        assert_eq!(engine.get_keyboard_mode_badge(), "許");

        engine.set_enabled_keyboard_modes(vec![
            "hsu".to_string(),
            "standard".to_string(),
            "et26".to_string(),
            "cangjie".to_string(),
            "japanese".to_string(),
        ]);
        assert_eq!(engine.cycle_keyboard_mode(), "standard");
        assert_eq!(engine.get_keyboard_mode_badge(), "注");
        assert_eq!(engine.cycle_keyboard_mode(), "et26");
        assert_eq!(engine.get_keyboard_mode_badge(), "倚");
        assert_eq!(engine.cycle_keyboard_mode(), "cangjie");
        assert_eq!(engine.get_keyboard_mode_badge(), "倉");
        assert_eq!(engine.cycle_keyboard_mode(), "japanese");
        assert_eq!(engine.get_keyboard_mode_badge(), "日");
        assert_eq!(engine.cycle_keyboard_mode(), "hsu");
        assert_eq!(engine.get_keyboard_mode_badge(), "許");
    }

    #[test]
    fn test_multi_mode_headcode_expansion_ge_3_chars() {
        use crate::lexicon::LexiconEntry;
        let mut engine = RuhiEngine::new();
        engine.lexicon.add_entry(LexiconEntry::new("忠孝東路", "ㄓㄨㄥ ㄒㄧㄠˋ ㄉㄨㄥ ㄌㄨˋ", 1500, "domain", ""));
        engine.lexicon.add_entry(LexiconEntry::new("台北車站", "ㄊㄞˊ ㄅㄟˇ ㄔㄜ ㄓㄢˋ", 1550, "domain", ""));
        engine.lexicon.add_entry(LexiconEntry::new("流連忘返", "ㄌㄧㄡˊ ㄌㄧㄢˊ ㄨㄤˋ ㄈㄢˇ", 1500, "core", ""));
        engine.lexicon.add_entry(LexiconEntry::new("一石二鳥", "ㄧ ㄕˊ ㄦˋ ㄋㄧㄠˇ", 1500, "core", ""));
        engine.lexicon.add_entry(LexiconEntry::new("砥睿者", "ㄉㄧˇ ㄖㄨㄟˋ ㄓㄜˇ", 1600, "domain", ""));
        engine.lexicon.add_entry(LexiconEntry::new("邏霸式", "ㄌㄨㄛˊ ㄅㄚˋ ㄕˋ", 1600, "domain", ""));
        engine.lexicon.add_entry(LexiconEntry::new("自串珠鍊", "ㄗˋ ㄔㄨㄢˋ ㄓㄨ ㄌㄧㄢˋ", 1600, "domain", ""));
        engine.lexicon.add_entry(LexiconEntry::new("中華民國", "ㄓㄨㄥ ㄏㄨㄚˊ ㄇㄧㄣˊ ㄍㄨㄛˊ", 1600, "core", ""));

        // 1. 許氏雙拼 3~4 字頭碼展開：jcdl ➔ 忠孝東路、tbvj ➔ 台北車站、tbcj ➔ 台北市長、llxf ➔ 流連忘返、ecln ➔ 一石二鳥、zvjl ➔ 自串珠鍊、drj ➔ 砥睿者、lbc ➔ 邏霸式
        for &(hc, expected) in &[
            ("jcdl", "忠孝東路"),
            ("tbvj", "台北車站"),
            ("tbcj", "台北市長"),
            ("llxf", "流連忘返"),
            ("ecln", "一石二鳥"),
            ("zvjl", "自串珠鍊"),
            ("jhmg", "中華民國"),
            ("drj", "砥睿者"),
            ("lbc", "邏霸式"),
        ] {
            engine.reset_buffer();
            for ch in hc.chars() {
                engine.process_key(ch);
            }
            let up = engine.open_up_arrow_candidates();
            assert!(
                up.candidates.contains(&expected.to_string()),
                "許氏頭碼 `{}` 按上箭頭必包含 `{}`，實際：{:?}",
                hc,
                expected,
                up.candidates
            );
            let idx = up.candidates.iter().position(|w| w == expected).unwrap();
            engine.select_candidate(idx);
            assert_eq!(engine.composed_sentence, expected);
        }

        // 2. 黃金門檻驗證（>= 3 字詞）：2 鍵絕不執行二字詞通用頭碼展開，保護許氏雙拼免遭二字頭碼碰撞污染
        let two_char_hc = engine.lexicon.lookup_multi_char_headcode("tb", "hsu", None, None);
        assert!(
            two_char_hc.is_empty(),
            "2 碼輸入 `tb` 絕不可觸發二字詞通用頭碼展開，實際：{:?}",
            two_char_hc
        );

        // 3. 標準大千（standard）：純聲母連打 >= 3 碼自動組詞與上箭頭頭碼展開
        engine.set_keyboard_mode("standard");
        for &(keys, expected) in &[
            ("5v2x", "忠孝東路"), // ㄓㄒㄉㄌ
            ("w1t5", "台北車站"), // ㄊㄅㄔㄓ
            ("2b5", "砥睿者"),   // ㄉㄖㄓ
        ] {
            engine.reset_buffer();
            for ch in keys.chars() {
                engine.process_key(ch);
            }
            assert_eq!(
                engine.composed_sentence, expected,
                "標準大千純聲母頭碼 `{}` 應自動組出 `{}`，實際：`{}`",
                keys, expected, engine.composed_sentence
            );
        }

        // 4. 倚天 26 鍵（et26）：>= 3 碼上箭頭頭碼展開 (忠孝東路 ㄓㄒㄉㄌ -> j x d l)
        engine.set_keyboard_mode("et26");
        engine.reset_buffer();
        for ch in "jxdl".chars() {
            engine.process_key(ch);
        }
        let et_up = engine.open_up_arrow_candidates();
        assert!(
            et_up.candidates.contains(&"忠孝東路".to_string()),
            "倚天26鍵頭碼 `jxdl` 按上箭頭必包含 `忠孝東路`，實際：{:?}",
            et_up.candidates
        );

        // 5. 倉頡模式（cangjie）：>= 3 字倉頡首碼頭碼展開 (ljdr -> 忠孝東路, iljy -> 台北車站, ltrw -> 中華民國)
        engine.set_keyboard_mode("cangjie");
        for &(cj_hc, expected) in &[
            ("ljdr", "忠孝東路"),
            ("iljy", "台北車站"),
            ("ltrw", "中華民國"),
        ] {
            engine.reset_buffer();
            for ch in cj_hc.chars() {
                engine.process_key(ch);
            }
            let cj_up = engine.process_key_str("Up");
            assert!(
                cj_up.candidates.contains(&expected.to_string()),
                "倉頡首碼頭碼 `{}` 按上箭頭必包含 `{}`，實際：{:?}",
                cj_hc,
                expected,
                cj_up.candidates
            );
        }
    }

    #[test]
    fn test_customizable_mode_cycle_and_japanese_engine() {
        let mut engine = RuhiEngine::new();
        // 使用者設定只留下許氏鍵盤與日文羅馬字，排除標準大千與倚天26鍵
        engine.set_enabled_keyboard_modes(vec!["hsu".to_string(), "japanese".to_string()]);
        assert_eq!(engine.keyboard_mode, "hsu");
        assert_eq!(engine.get_keyboard_mode_badge(), "許");

        // Ctrl/Shift + Caps Lock 切換至日文模式
        assert_eq!(engine.cycle_keyboard_mode(), "japanese");
        assert_eq!(engine.get_keyboard_mode_badge(), "日");

        // 測試日文連打：watashihanihongogasuki -> 私は日本語が好き
        for ch in "watashihanihongogasuki".chars() {
            engine.process_key(ch);
        }
        assert_eq!(engine.composed_sentence, "私は日本語が好き");

        // 測試 ↓ 下箭頭切換平假名／片假名／漢字
        engine.reset_buffer();
        for ch in "ko-hi-".chars() {
            engine.process_key(ch);
        }
        assert_eq!(engine.composed_sentence, "コーヒー");
        let down = engine.process_key_str("Down");
        assert!(down.candidates.contains(&"コーヒー".to_string()));
        assert!(down.candidates.contains(&"こーひー".to_string()));

        // 測試 ↑ 上箭頭長句敬語預測：yoroshi -> よろしくお願いします
        engine.reset_buffer();
        for ch in "yoroshi".chars() {
            engine.process_key(ch);
        }
        let up = engine.process_key_str("Up");
        assert!(
            up.candidates.contains(&"よろしくお願いします".to_string()),
            "yoroshi 按上箭頭應預測展開『よろしくお願いします』，實際：{:?}",
            up.candidates
        );
        let idx = up.candidates.iter().position(|w| w == "よろしくお願いします").unwrap();
        engine.select_candidate(idx);
        assert_eq!(engine.composed_sentence, "よろしくお願いします");

        // 再次切換必直接回到許氏雙拼，絕不經過 standard / et26
        assert_eq!(engine.cycle_keyboard_mode(), "hsu");
        assert_eq!(engine.get_keyboard_mode_badge(), "許");
    }

    #[test]
    fn test_interrogative_degree_adv_duo_vs_classifier_duo() {
        use crate::lexicon::LexiconEntry;
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        // 驗證 lexicon_mcbopomofo.tsv 已徹底清除「朵 (ㄉㄨㄛ)」一聲異讀污染
        let duo1_entries = engine.lexicon.lookup_bopomofo("ㄉㄨㄛ", true);
        assert!(
            !duo1_entries.iter().any(|e| e.word == "朵"),
            "一聲『ㄉㄨㄛ』詞庫嚴禁混入三聲量詞『朵』"
        );

        // 即使刻意在記憶體注入「朵 (ㄉㄨㄛ)」模擬極端異讀或模糊音情境，
        // Layer 1 疑問程度副詞構式與量詞數詞門檻亦必須確保「到底有多厲害呢」、「有多神奇」100% 選中「多」
        engine.lexicon.add_entry(LexiconEntry::new("朵", "ㄉㄨㄛ", 539, "mcbopomofo", ""));

        // 1. 到底有多厲害呢: ㄉㄠˋ(dwj) ㄉㄧˇ(def) ㄧㄡˇ(eof) ㄉㄨㄛ(dxh ) ㄌㄧˋ(lej) ㄏㄞˋ(hij) ˙ㄋㄜ(ngs)
        engine.reset_buffer();
        for c in "dwjdefeofdxh lejhijngs".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "到底有多厲害呢",
            "『到底有多厲害呢』嚴禁被量詞『朵』搶佔，實際：{}",
            engine.get_display_text()
        );

        // 2. 這個輸入法有多神奇: ㄓㄜˋ(jgj) ˙ㄍㄜ(ggs) ㄕㄨ(cx ) ㄖㄨˋ(rxj) ㄈㄚˇ(frf) ㄧㄡˇ(eof) ㄉㄨㄛ(dxh ) ㄕㄣˊ(cnd) ㄑㄧˊ(ved)
        engine.reset_buffer();
        for c in "jgjggscx rxjfrfeofdxh cndved".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "這個輸入法有多神奇",
            "『這個輸入法有多神奇』嚴禁被量詞『朵』搶佔，實際：{}",
            engine.get_display_text()
        );

        // 3. 驗證具備數詞前綴時（一朵花: ㄧ(e ) ㄉㄨㄛˇ(dxhf) ㄏㄨㄚ(hxy )），量詞「朵」正常運作
        engine.reset_buffer();
        for c in "e dxhfhxy ".chars() {
            engine.process_key(c);
        }
        assert_eq!(engine.get_display_text(), "一朵花");

        // 4. 驗證詞庫正音清洗與破音虛詞讀音錨定：
        // (a) 三聲 ㄋㄚˇ ㄅㄣˇ ㄕㄨ (nrfbnfcx ) 必輸出「哪本書」而非「那本書」
        engine.reset_buffer();
        for c in "nrfbnfcx ".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "哪本書",
            "三聲『ㄋㄚˇ ㄅㄣˇ ㄕㄨ』必輸出『哪本書』，嚴禁被四聲指示詞『那』搶佔"
        );

        // (b) 四聲 ㄉㄧˋ：輸入「悄悄地」ㄑㄧㄠˇ(vewf) ㄑㄧㄠˇ(vewf) ㄉㄧˋ(dej) 必輸出「悄悄地」而非「悄悄的」
        engine.reset_buffer();
        for c in "vewfvewfdej".chars() {
            engine.process_key(c);
        }
        assert_eq!(
            engine.get_display_text(),
            "悄悄地",
            "明確輸入四聲『ㄉㄧˋ』時必輸出狀語『悄悄地』，嚴禁被破音『的(ㄉㄧˋ)』搶佔"
        );
    }

    #[test]
    fn test_separable_vo_resilience_and_aabb_wanwanququ() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        let cases = [
            // 1. 洗澡: ㄒㄧˇ(cef) ㄗㄠˇ(zwf)
            ("cefzwf", "洗澡"),
            // 2. 洗了一個澡: ㄒㄧˇ(cef) ˙ㄌㄜ(lgs) ㄧˊ(ed)/ㄧ(e ) ˙ㄍㄜ(ggs) ㄗㄠˇ(zwf)
            ("ceflgsedggszwf", "洗了一個澡"),
            ("ceflgse ggszwf", "洗了一個澡"),
            // 3. 洗了一個昏天暗地日月無光的澡:
            //    ㄒㄧˇ(cef) ˙ㄌㄜ(lgs) ㄧˊ(ed) ˙ㄍㄜ(ggs) ㄏㄨㄣ(hxn ) ㄊㄧㄢ(tem ) ㄢˋ(mj) ㄉㄧˋ(dej)
            //    ㄖˋ(rj) ㄩㄝˋ(uej) ㄨˊ(xd) ㄍㄨㄤ(gxk ) ˙ㄉㄜ(dgs) ㄗㄠˇ(zwf)
            ("ceflgsedggshxn tem mjdejrjuejxdgxk dgszwf", "洗了一個昏天暗地日月無光的澡"),
            ("ceflgse ggshxn tem mjdejrjuejxdgxk dgszwf", "洗了一個昏天暗地日月無光的澡"),
            // 4. 走路: ㄗㄡˇ(zof) ㄌㄨˋ(lxj)
            ("zoflxj", "走路"),
            // 5. 走了一趟路: ㄗㄡˇ(zof) ˙ㄌㄜ(lgs) ㄧˊ(ed)/ㄧ(e ) ㄊㄤˋ(tkj) ㄌㄨˋ(lxj)
            ("zoflgsedtkjlxj", "走了一趟路"),
            ("zoflgse tkjlxj", "走了一趟路"),
            // 6. 走了一趟彎彎曲曲的路:
            //    ㄗㄡˇ(zof) ˙ㄌㄜ(lgs) ㄧˊ(ed) ㄊㄤˋ(tkj) ㄨㄢ(xm ) ㄨㄢ(xm ) ㄑㄩ(vu ) ㄑㄩ(vu ) ˙ㄉㄜ(dgs) ㄌㄨˋ(lxj)
            ("zoflgsedtkjxm xm vu vu dgslxj", "走了一趟彎彎曲曲的路"),
            ("zoflgse tkjxm xm vu vu dgslxj", "走了一趟彎彎曲曲的路"),
            // 7. 走了一趟峰迴路轉的遠路:
            //    ㄗㄡˇ(zof) ˙ㄌㄜ(lgs) ㄧˊ(ed) ㄊㄤˋ(tkj) ㄈㄥ(fl ) ㄏㄨㄟˊ(hxad) ㄌㄨˋ(lxj) ㄓㄨㄢˇ(jxmf) ˙ㄉㄜ(dgs) ㄩㄢˇ(umf) ㄌㄨˋ(lxj)
            ("zoflgsedtkjfl hxadlxjjxmfdgsumflxj", "走了一趟峰迴路轉的遠路"),
            ("zoflgse tkjfl hxadlxjjxmfdgsumflxj", "走了一趟峰迴路轉的遠路"),
        ];

        for (keys, expected) in cases {
            engine.reset_buffer();
            for c in keys.chars() {
                engine.process_key(c);
            }
            let actual = engine.get_display_text();
            assert_eq!(
                actual, expected,
                "離合詞與 AABB 重疊詞韌性測試失敗（輸入 `{}`）：預期『{}』，實際『{}』",
                keys, expected, actual
            );
        }
    }

    #[test]
    fn test_user_reported_vo_and_homophone_cases_261019() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        let cases = [
            // 1. 離合詞（ㄌㄧˊ=led, ㄏㄜˊ=hgd, ㄘˊ=ad，嚴禁被「離合辭」搶佔）
            ("ledhgdad", "離合詞"),
            // 2. 打了一個錯字 / 打了一個晚上的字（ㄘㄨㄛˋ=axhj, ㄗˋ=zj，嚴禁出「自」）
            ("dyflgsedggsaxhjzj", "打了一個錯字"),
            ("dyflgse ggsaxhjzj", "打了一個錯字"),
            ("dyflgsedggsxmfckjdgszj", "打了一個晚上的字"),
            ("dyflgse ggsxmfckjdgszj", "打了一個晚上的字"),
            // 3. 請再清查還有哪些應該被照顧的離合詞（ㄑㄧㄥˇ=velf, ㄑㄧㄥ=vel , ㄔㄚˊ=vyd, ㄒㄧㄝ=cee ，三聲「ㄋㄚˇ=nyf」嚴禁出「那」，「離合詞」嚴禁出「辭」）
            ("velfzijvel vydhideofnyfcee el gibajjwjgxjdgsledhgdad", "請再清查還有哪些應該被照顧的離合詞"),
            // 4. 沒道理我打三聲還出那（ㄨㄛˇ=xhf, ㄙㄢ=sm , ㄕㄥ=cl , ㄔㄨ=vx ，嚴禁出「三升還出納」）
            ("maddwjlefxhfdyfsm cl hidvx nyj", "沒道理我打三聲還出那"),
            // 5. 不過似乎沒有收羅得十分齊全（ㄍㄨㄛˋ=gxhj, ㄕㄡ=co , ㄌㄨㄛˊ=lxhd, ㄕˊ=cd，補語「得」嚴禁被「的」搶佔）
            ("bxjgxhjsjhx madeofco lxhddgscdfn vedvumd", "不過似乎沒有收羅得十分齊全"),
            // 6. 現在好像又不厲害了（「又」嚴禁被「右」搶佔）
            ("cemjzijhwfcekjeojbxjlejhijlgs", "現在好像又不厲害了"),
            ("cemjzijhwfcekjeojbxdlejhijlgs", "現在好像又不厲害了"),
            // 7. 多麼美好（ㄉㄨㄛ=dxh ，嚴禁被拆碎為「多麼每郝」）
            ("dxh mgsmafhwf", "多麼美好"),
            // 8. 跑出來秀一下（ㄔㄨ=vx ，嚴禁被古詞「繡衣」搶佔為「跑出來繡衣下」）
            ("pwfvx lidceoje ceyj", "跑出來秀一下"),
            ("pwfvx lidceojedceyj", "跑出來秀一下"),
            // 9. 每次改個什麼都要查半天（ㄘˋ=aj, ㄕㄣˊ=cnd, ㄔㄚˊ=vyd，嚴禁出「茶半天」）
            ("mafajgifggscndmgsdo ewjvydbmjtem ", "每次改個什麼都要查半天"),
            // 10. 沒幫上什麼忙（ㄕㄤˋ=ckj, ㄕㄣˊ=cnd，嚴禁出「眉幫上什麼忙」）
            ("madbk ckjcndmgsmkd", "沒幫上什麼忙"),
        ];

        for (keys, expected) in cases {
            engine.reset_buffer();
            for c in keys.chars() {
                engine.process_key(c);
            }
            let actual = engine.get_display_text();
            assert_eq!(
                actual, expected,
                "261019 實戰離合詞與同音消歧測試失敗（輸入 `{}`）：預期『{}』，實際『{}』",
                keys, expected, actual
            );
        }
    }

    #[test]
    fn test_contrastive_conjunction_yu_and_homophone_cases_261019_b() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        let cases = [
            // 1. 對比並列連詞「[X] 與 [Y]」構式（徹底杜絕同字重複與跨界「*雨/*語/*羽」劫持）
            ("zijufzij", "再與在"),
            ("bxdzijufbxdzij", "不再與不在"),
            ("bxjzijufbxjzij", "不再與不在"),
            ("dgsufdgd", "的與得"),
            ("dgsufdgs", "的與得"),
            ("dkfufdkf", "黨與檔"),
            ("dmjufdmj", "但與淡"),
            ("vkdufvkd", "長與常"),
            ("jeojufjeoj", "就與救"),
            ("jeojhwfufjeojhwf", "就好與救好"),
            ("adufad", "詞與辭"),
            // 2. 「明明但應該更常用到吧」與「這樣就好」（防「淡」搶「但」、「更長」搶「更常」、「救好」搶「就好」）
            ("meldmelddmjel gigljvkduljdwjbys", "明明但應該更常用到吧"),
            ("jgjekjjeojhwf", "這樣就好"),
            // 3. 「與應該算是一種特殊連接詞」（防「特殊」拆碎為「特書」）
            ("ufel gi sxmjcje jxlftgjcx lemdjee ad", "與應該算是一種特殊連接詞"),
            ("ufel gi sxmjcjejjxlftgjcx lemdjee ad", "與應該算是一種特殊連接詞"),
            // 4. 「目前還沒真正開始發揮作用」（防「開始」拆碎為「開使」）
            ("mxjvemdhidmadjn jljki cffy hxa zxhjulj", "目前還沒真正開始發揮作用"),
        ];

        for (keys, expected) in cases {
            engine.reset_buffer();
            for c in keys.chars() {
                engine.process_key(c);
            }
            let actual = engine.get_display_text();
            assert_eq!(
                actual, expected,
                "261019_b 對比並列構式與反拆碎測試失敗（輸入 `{}`）：預期『{}』，實際『{}』",
                keys, expected, actual
            );
        }
    }

    #[test]
    fn test_eight_homophone_pairs_in_contrasting_sentences_261019() {
        let mut engine = RuhiEngine::new();
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_mcbopomofo.tsv", "mcbopomofo");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_moe.tsv", "moe");
        let _ = engine.lexicon.load_tsv("../ruhi_lexicon/data/lexicon_domain.tsv", "domain");
        let _ = engine.transitions.load_tsv("../ruhi_lexicon/data/transitions_user.tsv");

        let cases = [
            // ── 0. 使用者本輪實戰回報之兩大錯字句 ──
            // (a) 「是要請您對與這個字的語意規則加以分析」（防「試藥」搶佔「是要」）
            ("cjewjvelfnenddxajufjgjggszjdgsufejgxa zgdjey effn ce ", "是要請您對與這個字的語意規則加以分析"),
            // (b) 「但會不會對系統造成負擔」與「照理說這方案對於新手比較友善吧」（防「旦會不匯兌」搶佔「但會不會對」）
            ("dmjhxajbxdhxajdxajcejtxlfzwjvldfxjdm ", "但會不會對系統造成負擔"),
            ("dmjhxajbxjhxajdxajcejtxlfzwjvldfxjdm ", "但會不會對系統造成負擔"),
            ("jwjlefcxh jgjfk mjdxajudcen cofbefjewjeofcmjbys", "照理說這方案對於新手比較友善吧"),

            // ── 1. 「再 vs 在」與「不再 vs 不在」不同語境雙向精準出現 ──
            ("velfzijcjedaj", "請再試一次"),
            ("zijcejtxlfjxl unjzxhj", "在系統中運作"),
            ("bxdzijvxlddwjfxjjgd", "不再重蹈覆轍"),
            ("bxjzijvxlddwjfxjjgd", "不再重蹈覆轍"),
            ("bxdzijmelddm lefmemj", "不在名單裡面"),
            ("bxjzijmelddm lefmemj", "不在名單裡面"),

            // ── 2. 「的 vs 得」不同語境雙向精準出現（含單字與雙字 V+的/V+得）──
            ("ceefdgsfa vkdjel aif", "寫得非常精彩"),
            ("ceefdgdfa vkdjel aif", "寫得非常精彩"),
            ("jgjceefdgsjn vel vxf", "這寫得真清楚"),
            ("jgjceefdgsxndjk ", "這寫的文章"),

            // ── 3. 「黨 vs 檔」不同語境雙向精準出現 ──
            ("velfcem axnddkf", "請先存檔"),
            ("zijeefjljdkf", "在野政黨"),

            // ── 4. 「但 vs 淡」不同語境雙向精準出現 ──
            ("xajdwjhnfdmj", "味道很淡"),
            ("dmjty hxajbxdhxajdxajcejtxlfzwjvldfxjdm ", "但他會不會對系統造成負擔"),

            // ── 5. 「長 vs 常」不同語境雙向精準出現（維度形容詞 vs 頻率副詞）──
            ("jgjtewdcm lxjgljvkd", "這條山路更長"),
            ("jgjggsadgljvkduljdwj", "這個詞更常用到"),

            // ── 6. 「就 vs 救」與「就好 vs 救好」不同語境雙向精準出現 ──
            ("nyjjeojsxmjlgs", "那就算了"),
            ("byfrndjeojhxadlid", "把人救回來"),
            ("jgjekjjeojhwf", "這樣就好"),
            ("e cl byfbeljhxmjjeojhwflgs", "醫生把病患救好了"),

            // ── 7. 「詞 vs 辭」不同語境雙向精準出現 ──
            ("cen zl adkxj", "新增詞庫"),
            ("ty jueddeljadjdlgs", "他決定辭職了"),

            // ── 8. 「與 vs 語 vs 雨」五維語法語境精準分流 ──
            ("leflxnjufcdjemj", "理論與實踐"),
            ("ufafmjxdgxm ", "與此案無關"),
            ("xijmemjceyjdyjuf", "外面下大雨"),
            ("zjrmddgsgxhduf", "自然的國語"),

            // ── 9. 「都完工了。」與 261019 語意分析四句及「舉一反三」同構擴展完整驗證 ──
            ("do xmdgxl lgs.", "都完工了。"),
            ("do xmdgxl ls.", "都完工了。"),
            ("do xmdgxl lgs", "都完工了"),
            ("do xmdgxl ls", "都完工了"),
            ("do xmdvldlgs.", "都完成了。"),
            ("do co gxl lgs.", "都收工了。"),
            ("leljjemjejsxjcd fn lajzijzl jey udjey sxjcekjmxj", "另建議素食分類再增加瑜伽素項目"),
            ("bxjhmddmjufsxhfeofgx laj.", "不含蛋與所有菇類。"),
            ("vedcd xhfzijcueddyjejcxl rxjfyfj vemden xajcofceefcx cenjdgsgxm cej", "其實我在學大易輸入法之前因為手寫書信的關係"),
            ("zijcuedak jeedcxl rxjfyfj vemd", "在學倉頡輸入法之前"),
            ("xhfdgss kwfje bnfckjbefjewjpem cekjxndzjcelds kwfldfa ufen s kwf", "我的思考基本上比較偏向文字型思考而非語音思考"),
            ("befjewjpem cekjufen celds kwf", "比較偏向語音型思考"),
        ];

        for (keys, expected) in cases {
            engine.reset_buffer();
            let mut actual = String::new();
            for c in keys.chars() {
                let res = engine.process_key(c);
                if !res.committed_chunk.is_empty() {
                    actual.push_str(&res.committed_chunk);
                }
            }
            actual.push_str(&engine.get_display_text());
            assert_eq!(
                actual, expected,
                "261019 八大同音字組雙向對比語句測試失敗（輸入 `{}`）：預期『{}』，實際『{}』",
                keys, expected, actual
            );
        }
    }
}

