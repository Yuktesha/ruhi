use ruhi::engine::{EngineMode, EngineResponse};
use ruhi::router::geo_hierarchy::CjkGeoHierarchy;
use ruhi::router::MorphemeOntology;
use ruhi::RuhiEngine;
use serde::{Deserialize, Serialize};

fn default_western_langs() -> Vec<String> {
    vec!["en".to_string(), "sa_am".to_string()]
}

fn default_halfwidth_punct() -> String {
    "fullwidth".to_string()
}

fn default_quick_key() -> String {
    "auto".to_string()
}

fn default_tab_mode() -> String {
    "associative".to_string()
}

fn default_tilde_mode() -> String {
    "punct".to_string()
}

fn default_initial_mode() -> String {
    "hsu".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AndroidConfig {
    #[serde(default = "default_western_langs")]
    pub western_languages: Vec<String>,
    #[serde(default = "default_halfwidth_punct")]
    pub halfwidth_punct_mode: String,
    #[serde(default = "default_quick_key")]
    pub candidate_quick_key: String,
    #[serde(default = "default_tab_mode")]
    pub tab_key_mode: String,
    #[serde(default = "default_tilde_mode")]
    pub tilde_toggle_mode: String,
    #[serde(default)]
    pub shift_space_toggle_fullwidth: bool,
    #[serde(default)]
    pub enable_mixed_english: bool,
    #[serde(default = "default_initial_mode")]
    pub initial_keyboard_mode: String,
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            western_languages: default_western_langs(),
            halfwidth_punct_mode: default_halfwidth_punct(),
            candidate_quick_key: default_quick_key(),
            tab_key_mode: default_tab_mode(),
            tilde_toggle_mode: default_tilde_mode(),
            shift_space_toggle_fullwidth: false,
            enable_mixed_english: false,
            initial_keyboard_mode: default_initial_mode(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidEngineSnapshot {
    pub action: String,
    pub ui_mode: String,
    pub keyboard_mode: String,
    pub mode_badge: String,
    pub fullwidth_mode: bool,
    pub has_buffer: bool,
    pub show_candidates: bool,
    pub quick_select_active: bool,
    pub quick_key_mode: String,
    pub preedit: String,
    pub composed: String,
    pub cursor_pos: usize,
    pub selection_start: i32,
    pub selection_end: i32,
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
    pub page_size: usize,
    pub committed_chunk: String,
}

pub struct AndroidEngineSession {
    pub engine: RuhiEngine,
}

impl AndroidEngineSession {
    pub fn new(data_dir: &str, config: AndroidConfig) -> Self {
        let mut engine = RuhiEngine::new_with_data_dir(data_dir);
        engine.set_enabled_keyboard_modes(vec!["hsu".to_string(), "english".to_string()]);

        let init_mode = if RuhiEngine::normalize_keyboard_mode(&config.initial_keyboard_mode) == "english" {
            "english"
        } else {
            "hsu"
        };
        engine.set_keyboard_mode(init_mode);
        engine.last_keyboard_mode = if init_mode == "english" {
            "hsu".to_string()
        } else {
            "english".to_string()
        };

        let mut session = Self { engine };
        session.apply_config_internal(config, false);

        // 在背景初始化階段主動預熱全域 OnceLock 索引，避免首字輸入卡頓
        let _ = CjkGeoHierarchy::global();
        let _ = MorphemeOntology::global();

        session
    }

    pub fn from_config_json(data_dir: &str, config_json: &str) -> Self {
        let cfg = serde_json::from_str::<AndroidConfig>(config_json).unwrap_or_default();
        Self::new(data_dir, cfg)
    }

    fn apply_config_internal(&mut self, config: AndroidConfig, _preserve_mode: bool) {
        let langs = if config.western_languages.is_empty() {
            default_western_langs()
        } else {
            config.western_languages
        };
        self.engine.set_western_languages(&langs);
        self.engine.halfwidth_punct_mode = match config.halfwidth_punct_mode.trim().to_ascii_lowercase().as_str() {
            "consistent" => "consistent".to_string(),
            _ => "fullwidth".to_string(),
        };
        self.engine.candidate_quick_key = match config.candidate_quick_key.trim().to_ascii_lowercase().as_str() {
            "1234" | "123456789" => "1234".to_string(),
            "asdf" | "asdfghjkl" => "asdf".to_string(),
            _ => "auto".to_string(),
        };
        self.engine.tab_key_mode = match config.tab_key_mode.trim().to_ascii_lowercase().as_str() {
            "boundary_cut" => "boundary_cut".to_string(),
            _ => "associative".to_string(),
        };
        self.engine.tilde_toggle_mode = if config.tilde_toggle_mode.trim().is_empty() {
            default_tilde_mode()
        } else {
            config.tilde_toggle_mode.trim().to_string()
        };
        self.engine.shift_space_toggle_fullwidth = config.shift_space_toggle_fullwidth;
        self.engine.enable_mixed_english = config.enable_mixed_english;
        // 僅掛載許氏與歐文，不將輔助切換引導至未打包之龍龕
        self.engine.backtick_inline_mode = "user_abbrev".to_string();
    }

    pub fn apply_config(&mut self, config: AndroidConfig) -> AndroidEngineSnapshot {
        self.apply_config_internal(config, true);
        let resp = self.engine.build_response("apply_config", "");
        self.snapshot_from_response(&resp)
    }

    pub fn has_buffer(&self) -> bool {
        !self.engine.composed_sentence.is_empty()
            || self.engine.fsm.has_pending()
            || self.engine.inline_longkan_active
            || self.engine.mode != EngineMode::Normal
            || self.engine.is_backslash_escape_pending()
    }

    fn effective_quick_key_mode_str(&self) -> String {
        match self.engine.effective_candidate_quick_key() {
            "1234" | "1234_10" => "1234".to_string(),
            _ => "asdf".to_string(),
        }
    }

    fn is_popup_candidate_modal_active(&self) -> bool {
        self.engine.mode == EngineMode::CandidatePopup
            && !self.engine.inline_longkan_active
            && !self.engine.is_shift_down_longkan_popup()
            && !self.engine.current_candidates.is_empty()
    }

    pub fn process_key_str(&mut self, key_str: &str) -> AndroidEngineSnapshot {
        if self.is_popup_candidate_modal_active() {
            if matches!(key_str, "Escape" | "escape") {
                self.engine.mode = EngineMode::Normal;
                self.engine.candidate_target_range = None;
                self.engine.candidate_cycle_idx = 0;
                if !self.engine.is_english_keyboard_mode() && !self.engine.is_latin_mode {
                    self.engine.update_active_candidates();
                }
                let resp = self.engine.build_response("close_candidates", "");
                return self.snapshot_from_response(&resp);
            }
            if matches!(
                key_str,
                "Space" | "space" | " " | "Enter" | "Return" | "enter" | "return"
            ) {
                let idx = self.engine.candidate_cycle_idx;
                if self.engine.select_candidate(idx).is_some() {
                    self.engine.mode = EngineMode::Normal;
                    let resp = self.engine.build_response("update_candidate", "");
                    return self.snapshot_from_response(&resp);
                }
            }
            if key_str.chars().count() == 1 {
                let ch = key_str.chars().next().unwrap();
                if let Some(target_idx) = self.engine.resolve_menu_quick_select_idx(ch, true) {
                    if self.engine.select_candidate(target_idx).is_some() {
                        self.engine.mode = EngineMode::Normal;
                        let resp = self.engine.build_response("update_candidate", "");
                        return self.snapshot_from_response(&resp);
                    }
                }
            }
            let is_nav = matches!(
                key_str,
                "Up" | "up"
                    | "Down"
                    | "down"
                    | "Left"
                    | "left"
                    | "Right"
                    | "right"
                    | "Tab"
                    | "tab"
                    | "Shift+Tab"
                    | "Shift+tab"
                    | "shift+tab"
                    | "PageUp"
                    | "pageup"
                    | "Prior"
                    | "prior"
                    | "PageDown"
                    | "pagedown"
                    | "Next"
                    | "next"
            );
            if !is_nav {
                // 非導航鍵且非快選鍵：關閉候選窗並交由常規打字流程處理
                self.engine.mode = EngineMode::Normal;
                self.engine.candidate_target_range = None;
            }
        }

        let resp = self.engine.process_key_str(key_str);
        self.snapshot_from_response(&resp)
    }

    pub fn flush_pending_text(&mut self) -> String {
        if matches!(self.engine.mode, EngineMode::Normal | EngineMode::CandidatePopup)
            && !self.engine.inline_longkan_active
            && !self.engine.is_english_keyboard_mode()
            && self.engine.fsm.has_pending()
            && !self.engine.is_latin_mode
        {
            self.engine.fsm.commit_current('1');
            self.engine.update_sentence_composition();
        }
        let committed = self.engine.composed_sentence.clone();
        if !committed.is_empty() {
            self.engine.record_committed(&committed);
        }
        self.engine.reset_buffer();
        committed
    }

    pub fn toggle_mode(&mut self) -> AndroidEngineSnapshot {
        let committed = self.flush_pending_text();
        let _ = self.engine.toggle_last_two_keyboard_modes();
        let resp = self.engine.build_response("switch_keyboard_mode", &committed);
        self.snapshot_from_response(&resp)
    }

    pub fn toggle_fullwidth(&mut self) -> AndroidEngineSnapshot {
        self.engine.fullwidth_mode = !self.engine.fullwidth_mode;
        let resp = self.engine.build_response("toggle_fullwidth_mode", "");
        self.snapshot_from_response(&resp)
    }

    pub fn select_candidate(&mut self, index: usize) -> AndroidEngineSnapshot {
        if self.engine.inline_longkan_active
            || matches!(
                self.engine.mode,
                EngineMode::QuickNum
                    | EngineMode::TemplateSelect
                    | EngineMode::LexiconSwitch
                    | EngineMode::SymbolTree
            )
        {
            if index < self.engine.current_candidates.len() {
                self.engine.candidate_cycle_idx = index;
                let resp = self.engine.process_key('\n');
                return self.snapshot_from_response(&resp);
            }
            let resp = self.engine.build_response("none", "");
            return self.snapshot_from_response(&resp);
        }

        if self.engine.select_candidate(index).is_some() {
            self.engine.mode = EngineMode::Normal;
            let resp = self.engine.build_response("update_candidate", "");
            self.snapshot_from_response(&resp)
        } else {
            let resp = self.engine.build_response("none", "");
            self.snapshot_from_response(&resp)
        }
    }

    pub fn commit_all(&mut self) -> AndroidEngineSnapshot {
        let committed = self.flush_pending_text();
        let action = if committed.is_empty() {
            "none"
        } else {
            "commit_enter"
        };
        let resp = self.engine.build_response(action, &committed);
        self.snapshot_from_response(&resp)
    }

    pub fn reset(&mut self) -> AndroidEngineSnapshot {
        self.engine.reset_buffer();
        let resp = self.engine.build_response("reset", "");
        self.snapshot_from_response(&resp)
    }

    pub fn get_state(&self) -> AndroidEngineSnapshot {
        let resp = self.engine.build_response("state", "");
        self.snapshot_from_response(&resp)
    }

    fn snapshot_from_response(&self, resp: &EngineResponse) -> AndroidEngineSnapshot {
        let kb_norm = RuhiEngine::normalize_keyboard_mode(&self.engine.keyboard_mode);
        let badge = self.engine.get_keyboard_mode_badge().to_string();
        let (sel_start, sel_end) = match self.engine.get_selection_range() {
            Some((s, e)) => (s as i32, e as i32),
            None => (-1, -1),
        };

        let show_candidates = (self.engine.mode != EngineMode::Normal
            || self.engine.inline_longkan_active
            || resp.action == "cycle_candidate")
            && !resp.candidates.is_empty();

        let quick_select_active = match self.engine.mode {
            EngineMode::CandidatePopup | EngineMode::QuickNum | EngineMode::LexiconSwitch | EngineMode::SymbolTree => {
                !resp.candidates.is_empty()
            }
            EngineMode::TemplateSelect => self.engine.template_cand_selecting && !resp.candidates.is_empty(),
            EngineMode::Normal => {
                self.engine.inline_longkan_active
                    && self.engine.inline_cand_selecting
                    && !resp.candidates.is_empty()
            }
        };

        let candidates = if show_candidates {
            resp.candidates.clone()
        } else {
            Vec::new()
        };

        AndroidEngineSnapshot {
            action: resp.action.clone(),
            ui_mode: resp.mode.clone(),
            keyboard_mode: kb_norm,
            mode_badge: badge,
            fullwidth_mode: self.engine.fullwidth_mode,
            has_buffer: self.has_buffer(),
            show_candidates,
            quick_select_active,
            quick_key_mode: self.effective_quick_key_mode_str(),
            preedit: resp.preedit.clone(),
            composed: resp.composed.clone(),
            cursor_pos: resp.cursor_pos,
            selection_start: sel_start,
            selection_end: sel_end,
            candidates,
            candidate_idx: resp.candidate_idx,
            page_size: self.engine.candidate_page_size(),
            committed_chunk: resp.committed_chunk.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_data_dir() -> Option<String> {
        let candidates = [
            PathBuf::from("../ruhi_lexicon/data"),
            PathBuf::from("ruhi_lexicon/data"),
            PathBuf::from("../data"),
            PathBuf::from("data"),
        ];
        for c in candidates {
            if c.join("lexicon_core.tsv").exists() && c.join("lexicon_english.tsv").exists() {
                return Some(c.to_string_lossy().to_string());
            }
        }
        None
    }

    fn make_session() -> AndroidEngineSession {
        let dir = repo_data_dir().expect("repo ruhi_lexicon/data directory must exist for tests");
        AndroidEngineSession::new(&dir, AndroidConfig::default())
    }

    fn type_chars(sess: &mut AndroidEngineSession, s: &str) -> AndroidEngineSnapshot {
        let mut last = sess.get_state();
        for ch in s.chars() {
            let key = match ch {
                ' ' => "Space".to_string(),
                '\n' => "Enter".to_string(),
                _ => ch.to_string(),
            };
            last = sess.process_key_str(&key);
        }
        last
    }

    #[test]
    fn test_hsu_sentence_decoding_and_commit() {
        let mut sess = make_session();
        assert_eq!(sess.get_state().keyboard_mode, "hsu");
        assert_eq!(sess.get_state().mode_badge, "許");

        // 許氏輸入：「如喜」(r x d = ㄖㄨˊ, x e f = ㄒㄧˇ) 與「輸入」(c x = ㄕㄨ, r x j = ㄖㄨˋ)
        let s1 = type_chars(&mut sess, "cxrxj");
        assert!(s1.has_buffer);
        assert_eq!(s1.composed, "輸入");
        assert!(!s1.show_candidates, "一般打字狀態下不展開候選視窗列");

        let committed = sess.process_key_str("Enter");
        assert_eq!(committed.committed_chunk, "輸入");
        assert!(!committed.has_buffer);
    }

    #[test]
    fn test_hsu_single_char_candidate_popup_escape_and_asdf_select() {
        let mut sess = make_session();
        // 打「ㄕˋ」(許氏 c j)
        let s1 = type_chars(&mut sess, "cj");
        assert!(s1.has_buffer);
        let orig_len = s1.composed.chars().count();
        assert_eq!(orig_len, 1);

        // 按 Down 開啟單字候選窗
        let pop = sess.process_key_str("Down");
        assert_eq!(pop.ui_mode, "candidates");
        assert!(pop.show_candidates);
        assert!(pop.quick_select_active);
        assert_eq!(pop.quick_key_mode, "asdf");
        assert!(pop.candidates.len() >= 2);
        let second_cand = pop.candidates[1].clone();

        // 按 Escape 僅關閉候選窗，絕不清空整句
        let esc = sess.process_key_str("Escape");
        assert_eq!(esc.ui_mode, "normal");
        assert!(!esc.show_candidates);
        assert!(esc.has_buffer);
        assert_eq!(esc.composed.chars().count(), 1);

        // 再度按 Down 開啟候選窗並按 's'（asdf 第 2 項）直接選取
        let pop2 = sess.process_key_str("Down");
        assert_eq!(pop2.ui_mode, "candidates");
        let picked = sess.process_key_str("s");
        assert_eq!(picked.ui_mode, "normal");
        assert!(!picked.show_candidates);
        assert_eq!(picked.composed, second_cand);

        // 也支援在候選窗按 Space 確認高亮項
        let pop3 = sess.process_key_str("Down");
        assert_eq!(pop3.ui_mode, "candidates");
        let _nav = sess.process_key_str("Down");
        let confirmed = sess.process_key_str("Space");
        assert_eq!(confirmed.ui_mode, "normal");
        assert!(!confirmed.show_candidates);
    }

    #[test]
    fn test_mode_toggle_flushes_pending_composition_without_loss() {
        let mut sess = make_session();
        // 測試有已成字＋未完成音節時切換中／歐，應先沖洗上屏
        let s1 = type_chars(&mut sess, "cxr"); // 「輸」＋未結算之 ㄖ
        assert!(s1.has_buffer);

        let toggled = sess.toggle_mode();
        assert_eq!(toggled.keyboard_mode, "english");
        assert_eq!(toggled.mode_badge, "歐");
        assert!(!toggled.has_buffer);
        assert!(!toggled.committed_chunk.is_empty(), "切換時必須先送出未上屏文字，絕不丟字");

        let back = sess.toggle_mode();
        assert_eq!(back.keyboard_mode, "hsu");
        assert_eq!(back.mode_badge, "許");
    }

    #[test]
    fn test_english_autocorrect_rollback_and_tab_completion() {
        let mut sess = make_session();
        sess.toggle_mode();
        assert_eq!(sess.get_state().keyboard_mode, "english");

        // 1. 空白非破壞性勘誤 teh -> the ，按一次 BackSpace 還原 teh
        let ac = type_chars(&mut sess, "teh ");
        assert_eq!(ac.composed, "the ");
        let rb = sess.process_key_str("BackSpace");
        assert_eq!(rb.composed, "teh");
        sess.reset();

        // 2. 子句語境 Tab 補全：Where are you g + Tab -> going 居前並支援 asdf 快選
        let _ = type_chars(&mut sess, "Where are you g");
        let tab = sess.process_key_str("Tab");
        assert_eq!(tab.ui_mode, "candidates");
        assert!(tab.show_candidates);
        assert!(tab.candidates.iter().any(|c| c == "going"));
        assert_eq!(tab.quick_key_mode, "asdf", "歐文候選窗應統一保留許氏 asdfghjkl 習慣");

        // 若選擇的是 candidate[0] 並確認，能完成替換
        let going_idx = tab.candidates.iter().position(|c| c == "going").unwrap();
        let chosen = sess.select_candidate(going_idx);
        assert_eq!(chosen.ui_mode, "normal");
        assert!(chosen.composed.ends_with("going"));
    }

    #[test]
    fn test_english_sa_am_sanskrit_pack_enabled_by_default() {
        let mut sess = make_session();
        sess.toggle_mode();

        let _ = type_chars(&mut sess, "ananda");
        let down = sess.process_key_str("Down");
        assert_eq!(down.ui_mode, "candidates");
        assert!(down.show_candidates);
        assert!(
            down.candidates.iter().any(|c| c.contains('a') && c != "ananda"),
            "預設啟用 sa_am 時，ananda 按下方向鍵應提供阿南達瑪迦變音候選：{:?}",
            down.candidates
        );

        // 按 'a'（asdf 第一候選）或 's'（第二候選）應直接選詞而非誤打字母
        let picked = sess.process_key_str("a");
        assert_eq!(picked.ui_mode, "normal");
        assert!(!picked.show_candidates);
        assert_eq!(picked.composed, down.candidates[0]);
    }

    #[test]
    fn test_quick_num_mode_and_commit() {
        let mut sess = make_session();
        let q1 = sess.process_key_str("~");
        // tilde_toggle_mode 預設為 "punct" 輸出全形波浪號；但雙波浪進入數字快打需確認行為
        // 若 tilde_toggle_mode 為 "punct"，~ 會輸出「～」，這是使用者預設；切換為 "longkan" 才支援 ~~
        assert_eq!(q1.committed_chunk, "～");

        let mut cfg = AndroidConfig::default();
        cfg.tilde_toggle_mode = "longkan".to_string();
        sess.apply_config(cfg);
        let _ = sess.process_key_str("~");
        let qn = sess.process_key_str("~");
        assert_eq!(qn.ui_mode, "quick_num");
        let typed = type_chars(&mut sess, "123");
        assert_eq!(typed.ui_mode, "quick_num");
        assert!(typed.candidates.iter().any(|c| c == "一百二十三"));
        let idx = typed.candidates.iter().position(|c| c == "一百二十三").unwrap();
        let res = sess.select_candidate(idx);
        assert_eq!(res.committed_chunk, "一百二十三");
    }
}
