/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/mod.rs - 純歐文鍵盤輸入引擎 (Western Input Engine)

pub mod collocations;
pub mod fsm;
pub mod lexicon;
pub mod morphology;
pub mod phonetics;
pub mod sanskrit_am;
pub mod scoring;

pub use fsm::{FsmAction, TokenCasing, WesternFSM, WesternToken, WesternTokenKind};
pub use lexicon::WesternLexicon;
pub use morphology::{MorphologicalVariant, MorphologyEngine};
pub use scoring::WesternConfidenceScorer;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

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
struct WesternSessionMemory {
    recent_words: VecDeque<(String, String)>,
    recent_bigrams: VecDeque<(String, String)>,
}

fn western_session_store() -> &'static Mutex<HashMap<usize, WesternSessionMemory>> {
    static STORE: OnceLock<Mutex<HashMap<usize, WesternSessionMemory>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone)]
pub struct WesternEngine {
    pub fsm: WesternFSM,
    pub lexicon: Arc<WesternLexicon>,
    pub scorer: WesternConfidenceScorer,
}

#[derive(Debug, Clone)]
struct WesternLangState {
    enabled_languages: Vec<String>,
    data_dir: Option<std::path::PathBuf>,
    baseline_lexicon: Option<Arc<WesternLexicon>>,
    user_inserted_words: Vec<(String, u32, f32)>,
}

impl Default for WesternLangState {
    fn default() -> Self {
        Self {
            enabled_languages: vec!["en".to_string()],
            data_dir: None,
            baseline_lexicon: None,
            user_inserted_words: Vec::new(),
        }
    }
}

type LangStoreEntry = (std::sync::Weak<WesternLexicon>, WesternLangState);
type TsvDirEntry = (std::sync::Weak<WesternLexicon>, std::path::PathBuf);

fn western_lang_store() -> &'static Mutex<HashMap<usize, LangStoreEntry>> {
    static LANG_STORE: OnceLock<Mutex<HashMap<usize, LangStoreEntry>>> = OnceLock::new();
    LANG_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn western_tsv_dir_store() -> &'static Mutex<HashMap<usize, TsvDirEntry>> {
    static DIR_STORE: OnceLock<Mutex<HashMap<usize, TsvDirEntry>>> = OnceLock::new();
    DIR_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn normalize_western_languages(langs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in langs {
        let norm = match raw.trim().to_lowercase().as_str() {
            "" => continue,
            "english" => "en".to_string(),
            "french" | "français" | "francais" => "fr".to_string(),
            "german" | "deutsch" => "de".to_string(),
            "spanish" | "español" | "espanol" => "es".to_string(),
            "sanskrit" | "ananda_marga" | "am_sanskrit" => "sa_am".to_string(),
            other => other.to_string(),
        };
        if !out.contains(&norm) {
            out.push(norm);
        }
    }
    if out.is_empty() {
        out.push("en".to_string());
    }
    out
}

impl Default for WesternEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WesternEngine {
    pub fn new() -> Self {
        Self {
            fsm: WesternFSM::new(),
            lexicon: WesternLexicon::shared_default(),
            scorer: WesternConfidenceScorer::new(),
        }
    }

    pub fn new_with_lexicon(lexicon: WesternLexicon) -> Self {
        Self {
            fsm: WesternFSM::new(),
            lexicon: Arc::new(lexicon),
            scorer: WesternConfidenceScorer::new(),
        }
    }

    pub fn new_with_shared_lexicon(lexicon: Arc<WesternLexicon>) -> Self {
        Self {
            fsm: WesternFSM::new(),
            lexicon,
            scorer: WesternConfidenceScorer::new(),
        }
    }

    #[inline]
    fn session_key(&self) -> usize {
        self as *const Self as usize
    }

    fn snapshot_session_memory(&self) -> WesternSessionMemory {
        let key = self.session_key();
        western_session_store()
            .lock()
            .ok()
            .and_then(|map| map.get(&key).cloned())
            .unwrap_or_default()
    }

    /// 取得當前引擎實例之同子句雙詞記憶快照（主要供測試驗證）。
    pub fn session_recent_bigrams(&self) -> Vec<(String, String)> {
        self.snapshot_session_memory()
            .recent_bigrams
            .into_iter()
            .collect()
    }

    /// 清除當前引擎實例對應之靜態會話記憶。
    pub fn clear_session_memory(&mut self) {
        let key = self.session_key();
        if let Ok(mut map) = western_session_store().lock() {
            map.remove(&key);
        }
    }

    /// 記錄使用者已上屏歐文單字與同子句鄰接雙詞搭配（上限 96 筆，不隨單次組字緩衝重置清除，並採靜態旁表以嚴格維持結構體二進位佈局相容性）。
    pub fn record_committed_text(&mut self, text: &str) {
        const SESSION_CTX_CAP: usize = 96;
        let is_western_letter = |c: char| -> bool {
            matches!(
                c,
                'A'..='Z' | 'a'..='z' | '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}'
            )
        };
        let is_clause_break = |c: char| -> bool {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\u{201C}'
                    | '\u{201D}' | '\n' | '\r' | '\u{FF0C}' | '\u{3002}' | '\u{3001}' | '\u{FF1B}'
                    | '\u{FF1A}' | '\u{FF01}' | '\u{FF1F}' | '\u{FF08}' | '\u{FF09}' | '\u{300C}'
                    | '\u{300D}'
            ) || (!c.is_whitespace()
                && !is_western_letter(c)
                && c != '\''
                && c != '\u{2019}'
                && c != '-')
        };

        let mut parsed_clauses: Vec<Vec<String>> = Vec::new();
        for clause in text.split(is_clause_break) {
            let chars: Vec<char> = clause.chars().collect();
            let mut clause_words: Vec<String> = Vec::new();
            let mut i = 0usize;
            while i < chars.len() {
                if is_western_letter(chars[i]) {
                    let mut tok = String::new();
                    while i < chars.len() {
                        let ch = chars[i];
                        if is_western_letter(ch) {
                            tok.push(ch);
                            i += 1;
                        } else if (ch == '\'' || ch == '\u{2019}')
                            && i + 1 < chars.len()
                            && is_western_letter(chars[i + 1])
                        {
                            tok.push('\'');
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    if !tok.is_empty() {
                        clause_words.push(tok);
                    }
                } else {
                    i += 1;
                }
            }
            if !clause_words.is_empty() {
                parsed_clauses.push(clause_words);
            }
        }

        if parsed_clauses.is_empty() {
            return;
        }
        let key = self.session_key();
        let Ok(mut store) = western_session_store().lock() else {
            return;
        };
        if !store.contains_key(&key) && store.len() >= 64 {
            if let Some(evict_key) = store.keys().next().copied() {
                store.remove(&evict_key);
            }
        }
        let mem = store.entry(key).or_default();

        for clause_words in parsed_clauses {
            for w in &clause_words {
                if w.chars().count() >= 2 {
                    let w_low = w.to_lowercase();
                    if let Some(pos) = mem.recent_words.iter().position(|(low, _)| low == &w_low) {
                        mem.recent_words.remove(pos);
                    }
                    mem.recent_words.push_back((w_low, w.clone()));
                    while mem.recent_words.len() > SESSION_CTX_CAP {
                        mem.recent_words.pop_front();
                    }
                }
            }

            for pair in clause_words.windows(2) {
                let p_low = pair[0].to_lowercase();
                let c_orig = &pair[1];
                let c_low = c_orig.to_lowercase();
                if let Some(pos) = mem
                    .recent_bigrams
                    .iter()
                    .position(|(p, c)| p == &p_low && c.to_lowercase() == c_low)
                {
                    mem.recent_bigrams.remove(pos);
                }
                mem.recent_bigrams.push_back((p_low, c_orig.clone()));
                while mem.recent_bigrams.len() > SESSION_CTX_CAP {
                    mem.recent_bigrams.pop_front();
                }
            }
        }
    }

    /// 設定當前引擎實例啟用之歐語與外掛語言包清單（支援 `"en"`, `"fr"`, `"de"`, `"es"`, `"sa_am"`）。
    /// 預設僅啟用 `["en"]`（`"sa_am"` 預設為關閉 OFF）。
    pub fn set_enabled_languages(&mut self, langs: &[String]) {
        let target_langs = normalize_western_languages(langs);
        let want_sa = target_langs.iter().any(|l| l == "sa_am");
        let old_ptr = Arc::as_ptr(&self.lexicon) as usize;

        let mut prev_state = if let Ok(mut store) = western_lang_store().lock() {
            store.retain(|_, (weak, _)| weak.strong_count() > 0);
            store
                .get(&old_ptr)
                .filter(|(weak, _)| weak.strong_count() > 0)
                .map(|(_, st)| st.clone())
        } else {
            None
        }
        .unwrap_or_default();

        if prev_state.data_dir.is_none() {
            if let Ok(mut dirs) = western_tsv_dir_store().lock() {
                dirs.retain(|_, (weak, _)| weak.strong_count() > 0);
                if let Some((weak, dir)) = dirs.get(&old_ptr) {
                    if weak.strong_count() > 0 {
                        prev_state.data_dir = Some(dir.clone());
                    }
                }
            }
        }

        let had_sa = prev_state.enabled_languages.iter().any(|l| l == "sa_am");

        if Arc::ptr_eq(&self.lexicon, &WesternLexicon::shared_default())
            || Arc::strong_count(&self.lexicon) > 1
        {
            if prev_state.baseline_lexicon.is_none() && !had_sa {
                prev_state.baseline_lexicon = Some(self.lexicon.clone());
            }
            let _ = Arc::make_mut(&mut self.lexicon);
        } else if prev_state.baseline_lexicon.is_none() && !had_sa {
            prev_state.baseline_lexicon = Some(self.lexicon.clone());
        }

        if want_sa && !had_sa {
            if prev_state.baseline_lexicon.is_none() {
                prev_state.baseline_lexicon = Some(self.lexicon.clone());
            }
            if let Some(ref dir) = prev_state.data_dir {
                let pack_path = dir.join("packs").join("pack_western_sanskrit.tsv");
                if pack_path.exists() {
                    let _ = Arc::make_mut(&mut self.lexicon).load_tsv(&pack_path);
                }
            }
        } else if !want_sa && had_sa {
            if let Some(ref base_arc) = prev_state.baseline_lexicon {
                let mut restored = (**base_arc).clone();
                for (w, freq, weight) in &prev_state.user_inserted_words {
                    restored.insert_word(w, *freq, *weight);
                }
                self.lexicon = Arc::new(restored);
            }
        }

        prev_state.enabled_languages = target_langs;
        let new_ptr = Arc::as_ptr(&self.lexicon) as usize;
        if old_ptr != new_ptr {
            if let Ok(mut sess) = western_session_store().lock() {
                if let Some(mem) = sess.remove(&old_ptr) {
                    sess.insert(new_ptr, mem);
                }
            }
        }
        if let Ok(mut store) = western_lang_store().lock() {
            if old_ptr != new_ptr {
                store.remove(&old_ptr);
            }
            store.insert(new_ptr, (Arc::downgrade(&self.lexicon), prev_state));
        }
    }

    /// 取得當前引擎實例啟用之歐語與外掛語言包清單（預設為 `["en"]`）。
    pub fn enabled_languages(&self) -> Vec<String> {
        let ptr = Arc::as_ptr(&self.lexicon) as usize;
        if let Ok(store) = western_lang_store().lock() {
            if let Some((weak, st)) = store.get(&ptr) {
                if weak.strong_count() > 0 {
                    return st.enabled_languages.clone();
                }
            }
        }
        vec!["en".to_string()]
    }

    /// 檢查特定語言或領域包是否於當前引擎實例啟用（`sa_am` 預設為 false）。
    pub fn is_language_enabled(&self, lang: &str) -> bool {
        let target = lang.trim().to_lowercase();
        let ptr = Arc::as_ptr(&self.lexicon) as usize;
        if let Ok(store) = western_lang_store().lock() {
            if let Some((weak, st)) = store.get(&ptr) {
                if weak.strong_count() > 0 {
                    return st.enabled_languages.iter().any(|l| l == &target);
                }
            }
        }
        target == "en"
    }

    /// 寫入單字至當前引擎詞庫（寫入時自動 Copy-on-Write，不影響共用唯讀字典單例）
    pub fn insert_word(&mut self, word: &str, freq: u32, weight: f32) {
        let old_ptr = Arc::as_ptr(&self.lexicon) as usize;
        Arc::make_mut(&mut self.lexicon).insert_word(word, freq, weight);
        let new_ptr = Arc::as_ptr(&self.lexicon) as usize;
        if let Ok(mut store) = western_lang_store().lock() {
            if let Some((_, mut st)) = if old_ptr != new_ptr {
                store.remove(&old_ptr)
            } else {
                store.get(&new_ptr).cloned()
            } {
                st.user_inserted_words.push((word.to_string(), freq, weight));
                store.insert(new_ptr, (Arc::downgrade(&self.lexicon), st));
            }
        }
        if old_ptr != new_ptr {
            if let Ok(mut sess) = western_session_store().lock() {
                if let Some(mem) = sess.remove(&old_ptr) {
                    sess.insert(new_ptr, mem);
                }
            }
        }
    }

    /// 從 TSV 檔案載入完整歐文詞庫（優先共享系統級唯讀 TSV 快取單例）
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(0);
        }
        let old_ptr = Arc::as_ptr(&self.lexicon) as usize;
        let prev_langs = western_lang_store()
            .lock()
            .ok()
            .and_then(|s| s.get(&old_ptr).filter(|(w, _)| w.strong_count() > 0).map(|(_, st)| st.enabled_languages.clone()));

        let count = if Arc::ptr_eq(&self.lexicon, &WesternLexicon::shared_default()) {
            let (shared, count) = WesternLexicon::shared_from_tsv_with_count(p)?;
            self.lexicon = shared;
            count
        } else {
            Arc::make_mut(&mut self.lexicon).load_tsv(p)?
        };

        if let Some(parent_dir) = p.parent() {
            let new_ptr = Arc::as_ptr(&self.lexicon) as usize;
            if let Ok(mut dirs) = western_tsv_dir_store().lock() {
                dirs.retain(|_, (weak, _)| weak.strong_count() > 0);
                dirs.insert(new_ptr, (Arc::downgrade(&self.lexicon), parent_dir.to_path_buf()));
            }
            if let Some(langs) = prev_langs {
                if old_ptr != new_ptr {
                    if let Ok(mut store) = western_lang_store().lock() {
                        store.remove(&old_ptr);
                    }
                }
                self.set_enabled_languages(&langs);
            }
        }
        Ok(count)
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

    /// 常見歐文外來語與重音標準整詞對照表（供 ↓ 下箭頭查詢）
    pub const LOANWORD_DIACRITICS: &[(&str, &[&str])] = &[
        ("cafe", &["café", "cafés", "cafe"]),
        ("cafes", &["cafés", "café", "cafes"]),
        ("resume", &["résumé", "resumé", "resume"]),
        ("resumes", &["résumés", "resumés", "resumes"]),
        ("naive", &["naïve", "naive"]),
        ("naivete", &["naïveté", "naivete"]),
        ("facade", &["façade", "facade"]),
        ("cliche", &["cliché", "cliche"]),
        ("cliches", &["clichés", "cliches"]),
        ("fiance", &["fiancé", "fiancée", "fiance"]),
        ("fiancee", &["fiancée", "fiancé", "fiancee"]),
        ("touche", &["touché", "touche"]),
        ("voila", &["voilà", "voila"]),
        ("deja", &["déjà", "déjà vu", "deja"]),
        ("entree", &["entrée", "entree"]),
        ("protege", &["protégé", "protege"]),
        ("soiree", &["soirée", "soiree"]),
        ("melee", &["mêlée", "melee"]),
        ("creme", &["crème", "crème brûlée", "creme"]),
        ("brulee", &["brûlée", "brulee"]),
        ("jalapeno", &["jalapeño", "jalapeno"]),
        ("pinata", &["piñata", "pinata"]),
        ("senor", &["señor", "senor"]),
        ("senorita", &["señorita", "senorita"]),
        ("nino", &["niño", "nino"]),
        ("nina", &["niña", "nina"]),
        ("espana", &["España", "españa", "espana"]),
        ("espanol", &["español", "Español", "espanol"]),
        ("malaga", &["Málaga", "Malaga"]),
        ("bogota", &["Bogotá", "Bogota"]),
        ("cancun", &["Cancún", "Cancun"]),
        ("uber", &["über", "Uber", "uber"]),
        ("strasse", &["Straße", "Strasse", "straße"]),
        ("doppelganger", &["doppelgänger", "doppelganger"]),
        ("fuhrer", &["Führer", "fuhrer"]),
        ("schon", &["schön", "schon"]),
        ("zurich", &["Zürich", "Zurich"]),
        ("munchen", &["München", "Munich"]),
        ("koln", &["Köln", "Cologne"]),
        ("dusseldorf", &["Düsseldorf", "Dusseldorf"]),
        ("nurnberg", &["Nürnberg", "Nuremberg"]),
        ("gottingen", &["Göttingen", "Gottingen"]),
        ("tubingen", &["Tübingen", "Tubingen"]),
        ("expose", &["exposé", "expose"]),
        ("matinee", &["matinée", "matinee"]),
        ("puree", &["purée", "puree"]),
        ("saute", &["sauté", "sautéed", "saute"]),
        ("coupe", &["coupé", "coupe"]),
        ("decor", &["décor", "decor"]),
        ("detente", &["détente", "detente"]),
        ("elan", &["élan", "elan"]),
        ("emigre", &["émigré", "emigre"]),
        ("epee", &["épée", "epee"]),
        ("fete", &["fête", "fete"]),
        ("flambe", &["flambé", "flambe"]),
        ("frappe", &["frappé", "frappe"]),
        ("garcon", &["garçon", "garcon"]),
        ("lecon", &["leçon", "lecon"]),
        ("macrame", &["macramé", "macrame"]),
        ("nee", &["née", "nee"]),
        ("outre", &["outré", "outre"]),
        ("passe", &["passé", "passe"]),
        ("pate", &["pâté", "pate"]),
        ("risque", &["risqué", "risque"]),
        ("rose", &["rosé", "rose"]),
        ("souffle", &["soufflé", "souffle"]),
        ("francais", &["français", "Français", "francais"]),
        ("chateau", &["château", "chateau"]),
        ("gateau", &["gâteau", "gateau"]),
        ("hotel", &["hôtel", "hotel"]),
        ("role", &["rôle", "role"]),
        ("cote", &["côte", "côté", "cote"]),
        ("tete", &["tête", "tête-à-tête", "tete"]),
        ("etre", &["être", "etre"]),
        ("tres", &["très", "tres"]),
        ("apres", &["après", "apres"]),
        ("noel", &["Noël", "noël", "noel"]),
        ("coeur", &["cœur", "coeur"]),
        ("oeuvre", &["œuvre", "oeuvre"]),
        ("hors", &["hors d'œuvre", "hors"]),
        ("malmo", &["Malmö", "Malmo"]),
        ("goteborg", &["Göteborg", "Gothenburg"]),
        ("angstrom", &["ångström", "Ångström", "angstrom"]),
        ("smorgasbord", &["smörgåsbord", "smorgasbord"]),
        ("sao", &["São", "São Paulo", "sao"]),
        ("coracao", &["coração", "coracao"]),
        ("joao", &["João", "Joao"]),
        ("dvorak", &["Dvořák", "Dvorak"]),
        ("lodz", &["Łódź", "Lodz"]),
        ("krakow", &["Kraków", "Krakow"]),
        ("wroclaw", &["Wrocław", "Wroclaw"]),
        ("gdansk", &["Gdańsk", "Gdansk"]),
        ("erdogan", &["Erdoğan", "Erdogan"]),
        ("turkiye", &["Türkiye", "Turkiye"]),
        ("pho", &["phở", "Phở", "pho"]),
        ("banh", &["bánh", "bánh mì", "banh"]),
        ("nguyen", &["Nguyễn", "Nguyen"]),
        // 擴充法語高頻重音詞
        ("francaise", &["française", "Française", "francaise"]),
        ("ecole", &["école", "ecole"]),
        ("etude", &["étude", "etude"]),
        ("desole", &["désolé", "désolée", "desole"]),
        ("desolee", &["désolée", "désolé", "desolee"]),
        ("plait", &["plaît", "plait"]),
        ("bientot", &["bientôt", "bientot"]),
        ("dejeuner", &["déjeuner", "dejeuner"]),
        ("theatre", &["théâtre", "theatre"]),
        ("fenetre", &["fenêtre", "fenetre"]),
        ("foret", &["forêt", "foret"]),
        ("hopital", &["hôpital", "hopital"]),
        ("premiere", &["première", "premiere"]),
        ("lumiere", &["lumière", "lumiere"]),
        ("reverie", &["rêverie", "reverie"]),
        ("evenement", &["événement", "evenement"]),
        ("repondre", &["répondre", "repondre"]),
        ("boite", &["boîte", "boite"]),
        ("maitre", &["maître", "maitre"]),
        ("systeme", &["système", "systeme"]),
        ("probleme", &["problème", "probleme"]),
        ("numero", &["numéro", "numero"]),
        ("aout", &["août", "aout"]),
        // 擴充德語高頻變音詞
        ("fur", &["für", "fur"]),
        ("grussen", &["grüßen", "Grüßen", "grussen"]),
        ("grusse", &["Grüße", "grüße", "grusse"]),
        ("gruss", &["Gruß", "gruß", "gruss"]),
        ("offentlich", &["öffentlich", "offentlich"]),
        ("moglich", &["möglich", "moglich"]),
        ("konnen", &["können", "konnen"]),
        ("mussen", &["müssen", "mussen"]),
        ("mochte", &["möchte", "mochte"]),
        ("mochten", &["möchten", "mochten"]),
        ("wunsche", &["Wünsche", "wünsche", "wunsche"]),
        ("glucklich", &["glücklich", "glucklich"]),
        ("gluckwunsch", &["Glückwunsch", "gluckwunsch"]),
        ("fruhstuck", &["Frühstück", "fruhstuck"]),
        ("buro", &["Büro", "buro"]),
        ("gross", &["groß", "gross"]),
        ("weiss", &["weiß", "weiss"]),
        ("heisst", &["heißt", "heisst"]),
        ("fussen", &["Füssen", "Fussen"]),
        ("osterreich", &["Österreich", "Osterreich"]),
        ("naturlich", &["natürlich", "naturlich"]),
        ("spater", &["später", "spater"]),
        ("fruher", &["früher", "fruher"]),
        // 擴充西班牙語高頻重音與 Ñ 詞
        ("dias", &["días", "dias"]),
        ("manana", &["mañana", "manana"]),
        ("anos", &["años", "anos"]),
        ("ano", &["año", "ano"]),
        ("sueno", &["sueño", "sueno"]),
        ("suenos", &["sueños", "suenos"]),
        ("corazon", &["corazón", "corazon"]),
        ("cancion", &["canción", "cancion"]),
        ("nacion", &["nación", "nacion"]),
        ("informacion", &["información", "informacion"]),
        ("educacion", &["educación", "educacion"]),
        ("atencion", &["atención", "atencion"]),
        ("perdon", &["perdón", "perdon"]),
        ("adios", &["adiós", "adios"]),
        ("despues", &["después", "despues"]),
        ("tambien", &["también", "tambien"]),
        ("facil", &["fácil", "facil"]),
        ("dificil", &["difícil", "dificil"]),
        ("rapido", &["rápido", "rapido"]),
        ("telefono", &["teléfono", "telefono"]),
        ("musica", &["música", "musica"]),
        ("medico", &["médico", "medico"]),
        ("bebe", &["bebé", "bebe"]),
        ("arbol", &["árbol", "arbol"]),
        ("lapiz", &["lápiz", "lapiz"]),
        ("ingles", &["inglés", "Inglés", "ingles"]),
        ("frances", &["francés", "frances"]),
        ("aleman", &["alemán", "aleman"]),
        ("japon", &["Japón", "japon"]),
        ("mexico", &["México", "Mexico"]),
        ("peru", &["Perú", "Peru"]),
    ];

    /// 高頻歐文縮寫展開表（供 ↑ 上箭頭智慧展開）
    pub const ABBREV_EXPANSIONS: &[(&str, &[&str])] = &[
        ("tks", &["thanks", "thank you", "thank"]),
        ("thx", &["thanks", "thank you", "thank"]),
        ("ty", &["thank you", "thanks"]),
        ("pls", &["please"]),
        ("plz", &["please"]),
        ("btw", &["by the way"]),
        ("idk", &["I don't know"]),
        ("imo", &["in my opinion"]),
        ("imho", &["in my humble opinion"]),
        ("asap", &["as soon as possible", "ASAP"]),
        ("fyi", &["for your information", "FYI"]),
        ("afaik", &["as far as I know"]),
        ("tbh", &["to be honest"]),
        ("brb", &["be right back"]),
        ("np", &["no problem"]),
        ("nvm", &["never mind"]),
        ("omg", &["oh my god", "OMG"]),
        ("aka", &["also known as", "a.k.a."]),
        ("bc", &["because"]),
        ("bcz", &["because"]),
        ("cuz", &["because"]),
        ("w/", &["with"]),
        ("w/o", &["without"]),
        ("wo", &["without"]),
        ("msg", &["message", "messages"]),
        ("info", &["information", "info"]),
        ("cfg", &["configuration", "config"]),
        ("conf", &["configuration", "conference", "config"]),
        ("doc", &["document", "documentation", "documents"]),
        ("docs", &["documents", "documentation"]),
        ("repo", &["repository", "repositories"]),
        ("env", &["environment", "environments"]),
        ("impl", &["implementation", "implement"]),
        ("init", &["initialize", "initialization"]),
        ("param", &["parameter", "parameters"]),
        ("params", &["parameters", "parameter"]),
        ("arg", &["argument", "arguments"]),
        ("args", &["arguments", "argument"]),
        ("fn", &["function", "functions"]),
        ("func", &["function", "functions"]),
        ("var", &["variable", "variables"]),
        ("vars", &["variables", "variable"]),
        ("val", &["value", "values"]),
        ("vals", &["values", "value"]),
        ("idx", &["index", "indices"]),
        ("len", &["length"]),
        ("cnt", &["count", "counter"]),
        ("buf", &["buffer", "buffers"]),
        ("str", &["string", "strings"]),
        ("num", &["number", "numbers"]),
        ("err", &["error", "errors"]),
        ("req", &["request", "requirement"]),
        ("resp", &["response", "responses"]),
        ("res", &["result", "response", "resource"]),
        ("db", &["database", "databases"]),
        ("srv", &["server", "service"]),
        ("svr", &["server", "service"]),
        ("dev", &["development", "developer"]),
        ("prod", &["production", "product"]),
        ("perf", &["performance"]),
        ("auth", &["authentication", "authorization"]),
        ("sync", &["synchronize", "synchronization", "synchronous"]),
        ("async", &["asynchronous"]),
        ("temp", &["temporary", "temperature", "template"]),
        ("tmp", &["temporary", "template"]),
        ("prev", &["previous"]),
        ("curr", &["current"]),
        ("cur", &["current", "cursor"]),
        ("max", &["maximum"]),
        ("min", &["minimum"]),
        ("avg", &["average"]),
        ("std", &["standard"]),
        ("pkg", &["package", "packages"]),
        ("lib", &["library", "libraries"]),
        ("dir", &["directory", "directories"]),
        ("src", &["source"]),
        ("dst", &["destination"]),
        ("dest", &["destination"]),
        ("opt", &["option", "options", "optional"]),
        ("opts", &["options", "optional"]),
        ("ppl", &["people"]),
        ("tmr", &["tomorrow"]),
        ("tmw", &["tomorrow"]),
        ("ytd", &["yesterday"]),
        ("govt", &["government"]),
        ("dept", &["department"]),
        ("mgmt", &["management"]),
        ("intl", &["international"]),
        ("tech", &["technology", "technical"]),
        ("cmptr", &["computer", "computers"]),
        ("kybrd", &["keyboard"]),
        ("prgm", &["program", "programming"]),
    ];

    /// 跨歐語與梵語多語言致意／感謝語義橋接表（供歐文模式下輸入 `tks`/`thanks`/`hello` 等按 `↓` 展開）
    pub const MULTILINGUAL_EU_GREETINGS: &[(&str, &[&str])] = &[
        (
            "tks",
            &[
                "Thanks",
                "Thank you",
                "Merci",
                "Danke",
                "Gracias",
                "Grazie",
                "Obrigado",
                "Dhanyava\u{0301}d",
            ],
        ),
        (
            "thx",
            &[
                "Thanks",
                "Thank you",
                "Merci",
                "Danke",
                "Gracias",
                "Grazie",
                "Obrigado",
                "Dhanyava\u{0301}d",
            ],
        ),
        (
            "thanks",
            &[
                "Thanks",
                "Thank you",
                "Merci",
                "Danke",
                "Gracias",
                "Grazie",
                "Obrigado",
                "Dhanyava\u{0301}d",
            ],
        ),
        (
            "thankyou",
            &[
                "Thank you",
                "Thanks",
                "Merci beaucoup",
                "Vielen Dank",
                "Muchas gracias",
                "Grazie mille",
                "Muito obrigado",
                "Dhanyava\u{0301}d",
            ],
        ),
        (
            "hi",
            &["Hi", "Hello", "Bonjour", "Hallo", "Hola", "Ciao", "Ola\u{0301}", "Namaste"],
        ),
        (
            "hello",
            &["Hello", "Bonjour", "Hallo", "Hola", "Ciao", "Ola\u{0301}", "Namaste"],
        ),
        (
            "sorry",
            &[
                "Sorry",
                "De\u{0301}sole\u{0301}",
                "Entschuldigung",
                "Lo siento",
                "Mi dispiace",
                "Desculpe",
            ],
        ),
        (
            "pls",
            &["Please", "S'il vous plai\u{0302}t", "Bitte", "Por favor", "Per favore"],
        ),
        (
            "please",
            &["Please", "S'il vous plai\u{0302}t", "Bitte", "Por favor", "Per favore"],
        ),
        (
            "bye",
            &["Bye", "Goodbye", "Au revoir", "Tschu\u{0308}ss", "Adio\u{0301}s", "Arrivederci", "Tchau"],
        ),
    ];

    /// 中文輸入模式（許氏／大千／漢語／倉頡／行列）下之簡易英語語義橋接（不混打前提下按 `↓`／`↑` 映射為中文常用語）
    pub const CHINESE_SEMANTIC_BRIDGE: &[(&str, &[&str])] = &[
        ("tks", &["謝謝", "感謝", "非常感謝", "多謝"]),
        ("thx", &["謝謝", "感謝", "非常感謝", "多謝"]),
        ("ty", &["謝謝", "感謝", "非常感謝"]),
        ("thanks", &["謝謝", "感謝", "非常感謝", "多謝"]),
        ("thankyou", &["非常感謝", "謝謝你", "感謝您", "謝謝"]),
        ("hi", &["你好", "嗨", "您好"]),
        ("hello", &["你好", "您好", "哈囉"]),
        ("sorry", &["對不起", "抱歉", "不好意思"]),
        ("sry", &["抱歉", "對不起", "不好意思"]),
        ("pls", &["請", "拜託", "麻煩"]),
        ("plz", &["請", "拜託", "麻煩"]),
        ("please", &["請", "拜託", "麻煩您"]),
        ("ok", &["好的", "沒問題", "收到"]),
        ("yes", &["是的", "沒錯", "好的"]),
        ("no", &["不是", "不用", "沒有"]),
        ("bye", &["再見", "拜拜", "下次見"]),
        ("good", &["太好了", "很棒", "讚"]),
        ("help", &["幫忙", "協助", "救命"]),
    ];

    /// 查詢中文模式下簡易英語語義橋接候選詞
    pub fn lookup_chinese_semantic_bridge(raw_ascii: &str) -> Vec<String> {
        let lower = raw_ascii.trim().to_lowercase();
        if lower.len() < 2 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for &(key, targets) in Self::CHINESE_SEMANTIC_BRIDGE {
            if key == lower {
                for &t in targets {
                    let s = t.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    /// 無撇號縮寫還原表（供 ↑ 上箭頭補回撇號）
    pub const CONTRACTION_RESCUE: &[(&str, &[&str])] = &[
        ("dont", &["don't", "do not"]),
        ("doesnt", &["doesn't", "does not"]),
        ("didnt", &["didn't", "did not"]),
        ("cant", &["can't", "cannot"]),
        ("wont", &["won't", "will not"]),
        ("wouldnt", &["wouldn't", "would not"]),
        ("shouldnt", &["shouldn't", "should not"]),
        ("couldnt", &["couldn't", "could not"]),
        ("isnt", &["isn't", "is not"]),
        ("arent", &["aren't", "are not"]),
        ("wasnt", &["wasn't", "was not"]),
        ("werent", &["weren't", "were not"]),
        ("havent", &["haven't", "have not"]),
        ("hasnt", &["hasn't", "has not"]),
        ("hadnt", &["hadn't", "had not"]),
        ("im", &["I'm", "I am"]),
        ("ive", &["I've", "I have"]),
        ("id", &["I'd", "ID", "id"]),
        ("ill", &["I'll", "ill"]),
        ("youre", &["you're", "you are"]),
        ("youve", &["you've", "you have"]),
        ("youd", &["you'd", "you would"]),
        ("youll", &["you'll", "you will"]),
        ("theyre", &["they're", "they are"]),
        ("theyve", &["they've", "they have"]),
        ("theyd", &["they'd", "they would"]),
        ("theyll", &["they'll", "they will"]),
        ("weve", &["we've", "we have"]),
        ("wed", &["we'd", "we would"]),
        ("thats", &["that's", "that is"]),
        ("whats", &["what's", "what is"]),
        ("whos", &["who's", "who is"]),
        ("theres", &["there's", "there is"]),
        ("heres", &["here's", "here is"]),
        ("lets", &["let's", "lets"]),
    ];

    /// 常見拼字錯誤直達修正表（供 ↑ 上箭頭即時修正）
    pub const COMMON_MISSPELLINGS: &[(&str, &[&str])] = &[
        ("teh", &["the"]),
        ("recieve", &["receive", "received", "receiving"]),
        ("seperate", &["separate", "separated"]),
        ("occured", &["occurred", "occurring"]),
        ("definately", &["definitely"]),
        ("definitly", &["definitely"]),
        ("accomodate", &["accommodate", "accommodation"]),
        ("wierd", &["weird"]),
        ("untill", &["until"]),
        ("becuase", &["because"]),
        ("beacuse", &["because"]),
        ("freind", &["friend", "friends"]),
        ("wich", &["which"]),
        ("thier", &["their"]),
        ("beleive", &["believe", "believed"]),
        ("acheive", &["achieve", "achieved"]),
        ("enviroment", &["environment", "environmental"]),
        ("goverment", &["government"]),
        ("occurence", &["occurrence"]),
        ("neccessary", &["necessary"]),
        ("necesary", &["necessary"]),
        ("tommorow", &["tomorrow"]),
        ("tomorow", &["tomorrow"]),
        ("begining", &["beginning"]),
        ("calender", &["calendar"]),
        ("collegue", &["colleague"]),
        ("concious", &["conscious"]),
        ("disapear", &["disappear"]),
        ("embarass", &["embarrass"]),
        ("existance", &["existence"]),
        ("foriegn", &["foreign"]),
        ("fourty", &["forty"]),
        ("gramer", &["grammar"]),
        ("grammer", &["grammar"]),
        ("immediatly", &["immediately"]),
        ("independant", &["independent"]),
        ("knowlege", &["knowledge"]),
        ("maintainance", &["maintenance"]),
        ("noticable", &["noticeable"]),
        ("priviledge", &["privilege"]),
        ("realy", &["really"]),
        ("rember", &["remember"]),
        ("remeber", &["remember"]),
        ("succesful", &["successful"]),
        ("successfull", &["successful"]),
        ("suprise", &["surprise"]),
        ("therefor", &["therefore"]),
        ("truely", &["truly"]),
        ("unfortunatly", &["unfortunately"]),
        ("langauge", &["language"]),
        ("algoritm", &["algorithm"]),
        ("algorythm", &["algorithm"]),
        ("architechture", &["architecture"]),
        ("performence", &["performance"]),
        ("paramter", &["parameter"]),
        ("funtion", &["function"]),
        ("funciton", &["function"]),
        ("retun", &["return"]),
        ("reutrn", &["return"]),
        ("widnow", &["window"]),
        ("sytem", &["system"]),
        ("sytsem", &["system"]),
        // 擴充無歧義高頻歐文錯拼直達修正
        ("recieved", &["received"]),
        ("recieving", &["receiving"]),
        ("adress", &["address"]),
        ("addres", &["address"]),
        ("buisness", &["business"]),
        ("busines", &["business"]),
        ("comming", &["coming"]),
        ("commitee", &["committee"]),
        ("completly", &["completely"]),
        ("definate", &["definite"]),
        ("develope", &["develop"]),
        ("diffrent", &["different"]),
        ("diferent", &["different"]),
        ("documnet", &["document"]),
        ("exampel", &["example"]),
        ("familar", &["familiar"]),
        ("finaly", &["finally"]),
        ("foward", &["forward"]),
        ("happend", &["happened"]),
        ("intrest", &["interest"]),
        ("libary", &["library"]),
        ("listner", &["listener"]),
        ("mispell", &["misspell"]),
        ("mispelled", &["misspelled"]),
        ("occuring", &["occurring"]),
        ("orginal", &["original"]),
        ("persue", &["pursue"]),
        ("posible", &["possible"]),
        ("possiblity", &["possibility"]),
        ("prefered", &["preferred"]),
        ("probly", &["probably"]),
        ("probaly", &["probably"]),
        ("progam", &["program"]),
        ("progamming", &["programming"]),
        ("queston", &["question"]),
        ("refered", &["referred"]),
        ("relase", &["release"]),
        ("relevent", &["relevant"]),
        ("requre", &["require"]),
        ("requrie", &["require"]),
        ("respone", &["response"]),
        ("resposne", &["response"]),
        ("restaraunt", &["restaurant"]),
        ("serach", &["search"]),
        ("servie", &["service"]),
        ("similiar", &["similar"]),
        ("speach", &["speech"]),
        ("strenght", &["strength"]),
        ("succint", &["succinct"]),
        ("sucess", &["success"]),
        ("tempalte", &["template"]),
        ("temprature", &["temperature"]),
        ("throught", &["through", "thought"]),
        ("togehter", &["together"]),
        ("togther", &["together"]),
        ("translaton", &["translation"]),
        ("upate", &["update"]),
        ("usefull", &["useful"]),
        ("usally", &["usually"]),
        ("verson", &["version"]),
        ("writting", &["writing"]),
        ("yestarday", &["yesterday"]),
        ("beacoup", &["beaucoup"]),
        ("bonjuor", &["bonjour"]),
        ("gracais", &["gracias"]),
        ("grcias", &["gracias"]),
        ("vieln", &["vielen"]),
        // 跨歐語與 L2 高頻無歧義拼寫錯誤救援
        ("pronounciation", &["pronunciation"]),
        ("pronounsiation", &["pronunciation"]),
        ("accomodation", &["accommodation"]),
        ("acomodation", &["accommodation"]),
        ("acomodashun", &["accommodation"]),
        ("psycology", &["psychology"]),
        ("psikology", &["psychology"]),
        ("phylosophy", &["philosophy"]),
        ("knolege", &["knowledge"]),
        ("rythm", &["rhythm"]),
        ("appearence", &["appearance"]),
        ("basicly", &["basically"]),
        ("physicly", &["physically"]),
        ("unecessary", &["unnecessary"]),
        ("millenium", &["millennium"]),
        ("questionaire", &["questionnaire"]),
        ("recomend", &["recommend"]),
        ("enviromental", &["environmental"]),
        ("entshuldigung", &["Entschuldigung"]),
    ];

    /// 將變音歐文字元還原為對應之基礎 ASCII 字母（支援在已變音字元上再次按 ↓ 循環選字）
    pub fn strip_char_diacritic(ch: char) -> char {
        match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'æ' | 'ª' => 'a',
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'Ǎ' | 'Æ' => 'A',
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'œ' | 'ə' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'Œ' | 'Ə' => 'E',
            'í' | 'ì' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ǐ' | 'ı' | 'ĳ' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'Ǐ' | 'İ' | 'Ĳ' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ǒ' | 'º' => 'o',
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'Ǒ' => 'O',
            'ú' | 'ù' | 'û' | 'ü' | 'ũ' | 'ů' | 'ū' | 'ŭ' | 'ű' | 'ų' | 'ǔ' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' => 'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ũ' | 'Ů' | 'Ū' | 'Ŭ' | 'Ű' | 'Ų' | 'Ǔ' => 'U',
            'ý' | 'ÿ' | 'ŷ' | 'ȳ' => 'y',
            'Ý' | 'Ÿ' | 'Ŷ' | 'Ȳ' => 'Y',
            'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => 'c',
            'Ç' | 'Ć' | 'Č' | 'Ĉ' | 'Ċ' => 'C',
            'ñ' | 'ń' | 'ň' | 'ņ' | 'ŋ' | 'ṅ' | 'ṇ' => 'n',
            'Ñ' | 'Ń' | 'Ň' | 'Ņ' | 'Ŋ' | 'Ṅ' | 'Ṇ' => 'N',
            'ß' | 'ś' | 'š' | 'ş' | 'ș' | 'ŝ' | 'ṣ' => 's',
            'ẞ' | 'Ś' | 'Š' | 'Ş' | 'Ș' | 'Ŝ' | 'Ṣ' => 'S',
            'ź' | 'ż' | 'ž' => 'z',
            'Ź' | 'Ż' | 'Ž' => 'Z',
            'ł' | 'ĺ' | 'ľ' | 'ļ' | 'ŀ' => 'l',
            'Ł' | 'Ĺ' | 'Ľ' | 'Ļ' | 'Ŀ' => 'L',
            'ð' | 'ď' | 'đ' | 'ḍ' => 'd',
            'Ð' | 'Ď' | 'Đ' | 'Ḍ' => 'D',
            'þ' | 'ť' | 'ţ' | 'ț' | 'ṭ' => 't',
            'Þ' | 'Ť' | 'Ţ' | 'Ț' | 'Ṭ' => 'T',
            'ŕ' | 'ř' | 'ŗ' | 'ṛ' => 'r',
            'Ŕ' | 'Ř' | 'Ŗ' | 'Ṛ' => 'R',
            'ğ' | 'ĝ' | 'ġ' | 'ģ' => 'g',
            'Ğ' | 'Ĝ' | 'Ġ' | 'Ģ' => 'G',
            'ĥ' | 'ħ' | 'ḥ' => 'h',
            'Ĥ' | 'Ħ' | 'Ḥ' => 'H',
            'ĵ' => 'j',
            'Ĵ' => 'J',
            'ķ' | 'ĸ' => 'k',
            'Ķ' => 'K',
            'ṁ' | 'ṃ' => 'm',
            'Ṁ' | 'Ṃ' => 'M',
            'ŵ' | 'ẁ' | 'ẃ' | 'ẅ' => 'w',
            'Ŵ' | 'Ẁ' | 'Ẃ' | 'Ẅ' => 'W',
            other => other,
        }
    }

    /// 查詢單一歐文字母之變音同位字集（法、德、西、葡、北歐、東歐、羅馬拼音與梵文轉寫 IAST）
    pub fn lookup_char_diacritics(ch: char) -> Vec<String> {
        let base = Self::strip_char_diacritic(ch);
        let list: &[&str] = match base {
            'a' => &["á", "à", "â", "ä", "ã", "å", "ā", "ă", "ą", "ǎ", "æ", "ª", "a"],
            'A' => &["Á", "À", "Â", "Ä", "Ã", "Å", "Ā", "Ă", "Ą", "Ǎ", "Æ", "A"],
            'e' => &["é", "è", "ê", "ë", "ē", "ĕ", "ė", "ę", "ě", "œ", "ə", "e"],
            'E' => &["É", "È", "Ê", "Ë", "Ē", "Ĕ", "Ė", "Ę", "Ě", "Œ", "Ə", "E"],
            'i' => &["í", "ì", "î", "ï", "ĩ", "ī", "ĭ", "į", "ǐ", "ı", "ĳ", "i"],
            'I' => &["Í", "Ì", "Î", "Ï", "Ĩ", "Ī", "Ĭ", "Į", "Ǐ", "İ", "Ĳ", "I"],
            'o' => &["ó", "ò", "ô", "ö", "õ", "ø", "ō", "ŏ", "ő", "ǒ", "œ", "º", "o"],
            'O' => &["Ó", "Ò", "Ô", "Ö", "Õ", "Ø", "Ō", "Ŏ", "Ő", "Ǒ", "Œ", "O"],
            'u' => &["ú", "ù", "û", "ü", "ũ", "ů", "ū", "ŭ", "ű", "ų", "ǔ", "ǖ", "ǘ", "ǚ", "ǜ", "u"],
            'U' => &["Ú", "Ù", "Û", "Ü", "Ũ", "Ů", "Ū", "Ŭ", "Ű", "Ų", "Ǔ", "U"],
            'y' => &["ý", "ÿ", "ŷ", "ȳ", "y"],
            'Y' => &["Ý", "Ÿ", "Ŷ", "Ȳ", "Y"],
            'c' => &["ç", "ć", "č", "ĉ", "ċ", "©", "¢", "c"],
            'C' => &["Ç", "Ć", "Č", "Ĉ", "Ċ", "©", "C"],
            'n' => &["ñ", "ń", "n\u{0301}", "n\u{032d}", "ň", "ņ", "ŋ", "ṅ", "ṇ", "n"],
            'N' => &["Ñ", "Ń", "N\u{0301}", "N\u{032d}", "Ň", "Ņ", "Ŋ", "Ṅ", "Ṇ", "N"],
            's' => &["ß", "ś", "s\u{0301}", "š", "ş", "ș", "ŝ", "ṣ", "§", "s"],
            'S' => &["ẞ", "Ś", "S\u{0301}", "Š", "Ş", "Ș", "Ŝ", "Ṣ", "§", "S"],
            'z' => &["ź", "ż", "ž", "z"],
            'Z' => &["Ź", "Ż", "Ž", "Z"],
            'l' => &["ł", "ĺ", "l\u{0301}", "ľ", "ļ", "ŀ", "ḷ", "l"],
            'L' => &["Ł", "Ĺ", "L\u{0301}", "Ľ", "Ļ", "Ŀ", "Ḷ", "L"],
            'd' => &["d\u{0301}", "ḍ", "ð", "ď", "đ", "d"],
            'D' => &["D\u{0301}", "Ḍ", "Ð", "Ď", "Đ", "D"],
            't' => &["t\u{0301}", "ṭ", "þ", "ť", "ţ", "ț", "™", "t"],
            'T' => &["T\u{0301}", "Ṭ", "Þ", "Ť", "Ţ", "Ț", "™", "T"],
            'r' => &["r\u{0301}", "ṛ", "ṝ", "ŕ", "ř", "ŗ", "®", "r"],
            'R' => &["R\u{0301}", "Ṛ", "Ṝ", "Ŕ", "Ř", "Ŗ", "®", "R"],
            'g' => &["ğ", "ĝ", "ġ", "ģ", "g"],
            'G' => &["Ğ", "Ĝ", "Ġ", "Ģ", "G"],
            'h' => &["ḥ", "ĥ", "ħ", "h"],
            'H' => &["Ḥ", "Ĥ", "Ħ", "H"],
            'j' => &["ĵ", "j"],
            'J' => &["Ĵ", "J"],
            'k' => &["ķ", "ĸ", "k"],
            'K' => &["Ķ", "K"],
            'm' => &["m\u{0301}", "ṁ", "ṃ", "µ", "m"],
            'M' => &["M\u{0301}", "Ṁ", "Ṃ", "M"],
            'w' => &["ŵ", "ẁ", "ẃ", "ẅ", "w"],
            'W' => &["Ŵ", "Ẁ", "Ẃ", "Ẅ", "W"],
            _ => &[],
        };
        list.iter().map(|&s| s.to_string()).collect()
    }

    /// 根據原始輸入字串的大小寫形態（LowerCase / TitleCase / UpperCase）套用至候選詞
    pub fn apply_casing_pattern(source: &str, candidate: &str) -> String {
        let src_chars: Vec<char> = source.chars().collect();
        if src_chars.is_empty() || candidate.is_empty() {
            return candidate.to_string();
        }
        let all_upper = src_chars.len() > 1 && src_chars.iter().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
        if all_upper {
            return candidate.to_uppercase();
        }
        let first_upper = src_chars[0].is_uppercase()
            && src_chars[1..].iter().filter(|c| c.is_alphabetic()).all(|c| c.is_lowercase());
        if first_upper {
            let mut c_chars = candidate.chars();
            if let Some(first) = c_chars.next() {
                let first_up: String = first.to_uppercase().collect();
                return format!("{}{}", first_up, c_chars.as_str());
            }
        }
        candidate.to_string()
    }

    /// 歐語軌 `↓` 下箭頭：同位變音字母（Diacritics）與外來語重音標準整詞查詢
    pub fn lookup_down_arrow_candidates(&self, token: &str, target_char: Option<char>) -> Vec<String> {
        self.lookup_word_down_candidates_at_offset(token, None, target_char)
    }

    /// 歐語軌 `↓` 下箭頭（支援指定單字內游標字元偏移 `focus_offset`）：
    /// 無論游標停在單字詞尾 (`cafe|`)、詞中 (`ca|fe`) 或詞首 (`|cafe`)，
    /// 皆優先回傳整詞變音候選，並針對游標所在字母與詞內母音合成整詞變音變體，絕不以單一字母覆寫整詞！
    pub fn lookup_word_down_candidates_at_offset(
        &self,
        token: &str,
        focus_offset: Option<usize>,
        target_char: Option<char>,
    ) -> Vec<String> {
        let trimmed = token.trim();
        let mut results: Vec<String> = Vec::new();

        if trimmed.is_empty() {
            if let Some(ch) = target_char {
                return Self::lookup_char_diacritics(ch);
            }
            return results;
        }

        let word_chars: Vec<char> = trimmed.chars().collect();
        // 1. 單一字元：直接回傳該字母之完整變音同位字集
        if word_chars.len() == 1 {
            return Self::lookup_char_diacritics(word_chars[0]);
        }

        // 2. 多字元單字：優先檢查外來語重音整詞表（如 cafe -> café, resume -> résumé, naive -> naïve, strasse -> Straße）
        let normalized_ascii: String = word_chars
            .iter()
            .copied()
            .map(Self::strip_char_diacritic)
            .collect::<String>()
            .to_lowercase();
        for &(key, variants) in Self::LOANWORD_DIACRITICS {
            if key == normalized_ascii {
                for &v in variants {
                    let cased = Self::apply_casing_pattern(trimmed, v);
                    if !results.contains(&cased) {
                        results.push(cased);
                    }
                }
                break;
            }
        }

        // 2b. 阿南達瑪迦梵語羅馬拼音（Ananda Marga Roman Saṁskṛta）與 IAST 整詞轉寫查詢（僅於明確啟用 `sa_am` 語言包時生效）
        //     例如 ananda -> ánanda / Ánanda / ānanda, krsna / krishna -> krśńa / Krśńa / kṛṣṇa,
        //     samskrta / sanskrit -> saḿskrta, jinana / jnana -> jiṋána, dharana -> dhárańá
        if self.is_language_enabled("sa_am") {
            for am_cand in sanskrit_am::lookup_am_sanskrit_word(trimmed) {
                if !results.contains(&am_cand) {
                    results.push(am_cand);
                }
            }
        }

        // 2c. 泛歐與（啟用 `sa_am` 時含梵文）多語系常用溝通語意橋接（如 tks / TKS / thanks + ↓）
        let include_sa_am = self.is_language_enabled("sa_am");
        for &(key, greetings) in Self::MULTILINGUAL_EU_GREETINGS {
            if key == normalized_ascii {
                for &g in greetings {
                    if !include_sa_am && g == "Dhanyava\u{0301}d" {
                        continue;
                    }
                    let s = g.to_string();
                    if !results.contains(&s) {
                        results.push(s);
                    }
                }
                break;
            }
        }

        // 3. 針對游標所在位置字母（或詞內母音／可變音子音）合成「整詞變音變體」
        //    例如在 cafe 詞尾或 'e' 上按 ↓ 產生 café, cafè, cafê, cafë；在 'a' 上按 ↓ 產生 cáfe, càfe, câfe
        let primary_idx = focus_offset
            .filter(|&idx| idx < word_chars.len())
            .or_else(|| {
                if let Some(tc) = target_char {
                    word_chars.iter().rposition(|&c| c == tc)
                } else {
                    None
                }
            })
            .unwrap_or(word_chars.len() - 1);

        let mut candidate_indices = vec![primary_idx];
        for idx in (0..word_chars.len()).rev() {
            if idx != primary_idx {
                let base = Self::strip_char_diacritic(word_chars[idx]).to_ascii_lowercase();
                if matches!(base, 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'c' | 'n' | 's' | 'z' | 'l' | 't' | 'd' | 'm' | 'r') {
                    candidate_indices.push(idx);
                }
            }
        }

        for (order, &idx) in candidate_indices.iter().enumerate() {
            let ch = word_chars[idx];
            let diacritics = Self::lookup_char_diacritics(ch);
            let limit = if order == 0 { 8 } else { 4 };
            for d in diacritics.into_iter().take(limit) {
                if d == ch.to_string() || d.chars().any(|dc| matches!(dc, '©' | '¢' | '§' | '™' | '®' | 'µ' | 'ª' | 'º')) {
                    continue;
                }
                let mut mutated = String::new();
                for (i, &orig_c) in word_chars.iter().enumerate() {
                    if i == idx {
                        mutated.push_str(&d);
                    } else {
                        mutated.push(orig_c);
                    }
                }
                if !results.contains(&mutated) {
                    results.push(mutated);
                }
            }
        }

        // 4. 補充常見無撇號縮寫、拼字修正與縮寫展開（讓使用者在詞尾按 ↓ 也能直接獲得智慧整詞修正）
        for &(raw, rescues) in Self::CONTRACTION_RESCUE {
            if normalized_ascii == raw {
                for &r in rescues {
                    let cased = Self::apply_casing_pattern(trimmed, r);
                    if !results.contains(&cased) {
                        results.push(cased);
                    }
                }
                break;
            }
        }
        for &(mis, fixes) in Self::COMMON_MISSPELLINGS {
            if normalized_ascii == mis {
                for &f in fixes {
                    let cased = Self::apply_casing_pattern(trimmed, f);
                    if !results.contains(&cased) {
                        results.push(cased);
                    }
                }
                break;
            }
        }
        for &(abbr, expansions) in Self::ABBREV_EXPANSIONS {
            if normalized_ascii == abbr {
                for &exp in expansions {
                    let cased = Self::apply_casing_pattern(trimmed, exp);
                    if !results.contains(&cased) {
                        results.push(cased);
                    }
                }
                break;
            }
        }

        // 5. 補充大小寫形態變體（TitleCase / UPPERCASE / lowercase）
        let lower = trimmed.to_lowercase();
        let title = Self::apply_casing_pattern("Title", &lower);
        let upper = trimmed.to_uppercase();
        for v in [title, upper, lower] {
            if !v.is_empty() && !results.contains(&v) {
                results.push(v);
            }
        }

        results
    }

    /// 收集單詞級明確拼寫救援與固定擴展（縮寫、問候、省略撇號、固定錯拼、外來語變音與梵語轉寫）。
    fn collect_up_arrow_exact_rescues(&self, token: &str) -> Vec<String> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let t_lower = trimmed.to_lowercase();
        let mut results: Vec<String> = Vec::new();

        let push_unique = |results: &mut Vec<String>, cand: &str| {
            let cased = Self::apply_casing_pattern(trimmed, cand);
            if !cased.is_empty() && !results.contains(&cased) {
                results.push(cased);
            }
        };

        // 1. 高頻縮寫展開 (如 tks -> thanks, thank you; pls -> please; btw -> by the way)
        for &(abbr, expansions) in Self::ABBREV_EXPANSIONS {
            if t_lower == abbr {
                for &exp in expansions {
                    push_unique(&mut results, exp);
                }
                break;
            }
        }

        // 1b. 泛歐與（啟用 `sa_am` 時含梵文）多語系常用溝通語意橋接
        let include_sa_am = self.is_language_enabled("sa_am");
        for &(key, greetings) in Self::MULTILINGUAL_EU_GREETINGS {
            if t_lower == key {
                for &g in greetings {
                    if !include_sa_am && g == "Dhanyava\u{0301}d" {
                        continue;
                    }
                    let s = g.to_string();
                    if !results.contains(&s) {
                        results.push(s);
                    }
                }
                break;
            }
        }

        // 2. 無撇號縮寫還原 (如 dont -> don't, cant -> can't, im -> I'm)
        for &(raw, rescues) in Self::CONTRACTION_RESCUE {
            if t_lower == raw {
                for &r in rescues {
                    push_unique(&mut results, r);
                }
                break;
            }
        }

        // 3. 常見拼字錯誤直達修正 (如 recieve -> receive, teh -> the, seperate -> separate)
        for &(mis, fixes) in Self::COMMON_MISSPELLINGS {
            if t_lower == mis {
                for &f in fixes {
                    push_unique(&mut results, f);
                }
                break;
            }
        }

        // 4. 外來語重音整詞亦納入智慧校正候選 (如 cafe -> café, naive -> naïve)
        for &(key, variants) in Self::LOANWORD_DIACRITICS {
            if t_lower == key {
                for &v in variants {
                    push_unique(&mut results, v);
                }
                break;
            }
        }

        // 4b. 阿南達瑪迦梵語經典名相與片語展開（僅於明確啟用 `sa_am` 語言包時生效）
        if self.is_language_enabled("sa_am") {
            for ph in sanskrit_am::lookup_am_sanskrit_phrases(trimmed) {
                if !results.contains(&ph) {
                    results.push(ph);
                }
            }
            for am_w in sanskrit_am::lookup_am_sanskrit_word(trimmed) {
                if !results.contains(&am_w) {
                    results.push(am_w);
                }
            }
        }

        // 5. 前綴智慧補全與拼寫救援依真實前綴狀態排序：
        //    若 `t_lower` 為詞庫合法前綴（如 "progr"、"algor"、"deve"），優先顯示依頻率／權重排序之前綴補全，
        //    避免被 Damerau-Levenshtein 或骨架模糊匹配搶佔首位；
        //    若非合法前綴，則先進行骨架／編輯距離錯字救援，再補充前綴匹配。
        let _ = self.lexicon.has_prefix(&t_lower);
        results
    }

    /// 依 TokenCasing 原型調整單字候選的大小寫形態。
    fn apply_candidate_casing(cand: &str, raw_token: &str) -> String {
        if cand.contains(' ') {
            return cand.to_string();
        }
        let alpha_count = raw_token.chars().filter(|c| c.is_alphabetic()).count();
        if alpha_count >= 2 && raw_token.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
            return cand.to_uppercase();
        }
        if raw_token.chars().next().map_or(false, |c| c.is_uppercase()) {
            let mut chars = cand.chars();
            if let Some(first) = chars.next() {
                if first.is_lowercase() {
                    let mut out = String::new();
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                    return out;
                }
            }
        }
        cand.to_string()
    }

    /// 歐語軌 `↑` 上箭頭／`Tab` 智慧拼字校正與前綴補全（無上下文快速入口）。
    pub fn lookup_up_arrow_candidates(&self, token: &str) -> Vec<String> {
        self.lookup_contextual_up_arrow_candidates("", token, "")
    }

    /// 歐語軌語境感知聯想與補全：整合雙向混淆消歧、N-gram 搭配詞、正交文法構式、近期會話記憶、泛歐變音詞與全詞典計分前綴檢索。
    pub fn lookup_contextual_up_arrow_candidates(
        &self,
        prefix_before_word: &str,
        token: &str,
        next_context: &str,
    ) -> Vec<String> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let t_lower = trimmed.to_lowercase();
        let mut results: Vec<String> = Vec::new();
        let session_mem = self.snapshot_session_memory();

        let after_space = prefix_before_word
            .chars()
            .last()
            .map_or(false, |c| matches!(c, ' ' | '\t' | '\u{3000}'));
        let after_acronym_boundary = after_space
            || prefix_before_word
                .chars()
                .last()
                .map_or(false, |c| matches!(c, '(' | '（' | '[' | ':' | '='));
        let acro_ctx = if after_acronym_boundary {
            collocations::analyze_acronym_context(prefix_before_word)
        } else {
            None
        };
        let acro_cands = acro_ctx
            .as_ref()
            .map(|ctx| collocations::lookup_acronym_candidates_with_context(ctx, trimmed))
            .unwrap_or_default();
        let prev_words: Vec<String> = if after_space {
            collocations::extract_preceding_western_words(prefix_before_word, 6)
        } else {
            Vec::new()
        };
        let next_word = collocations::extract_leading_western_word(next_context);
        let earlier_content_words =
            collocations::extract_earlier_clause_content_words(prefix_before_word);
        let prev_last = prev_words.last().map(|s| s.as_str()).unwrap_or("");
        let has_context = !prev_words.is_empty()
            || !next_word.is_empty()
            || !earlier_content_words.is_empty()
            || acro_ctx.is_some();

        // 1. 高分數雙向語境易混詞消歧（如 "their are" -> "there"，"more then" -> "than"）
        if !prev_last.is_empty() || !next_word.is_empty() {
            for (cand, score, _rule) in
                self.lookup_contextual_confusables(prev_last, trimmed, &next_word)
            {
                if score >= 95.0 && !results.contains(&cand) {
                    results.push(cand);
                }
            }
        }

        // 2. 明確單詞級拼寫救援與縮寫／梵語／外來語變音擴展（若當前存在縮略語定義上下文則延後至語境排序後插入）
        if acro_cands.is_empty() {
            for w in self.collect_up_arrow_exact_rescues(trimmed) {
                if !results.contains(&w) {
                    results.push(w);
                }
            }
        }

        // 計算單詞於當前子句與會話記憶中之額外語境加分
        let compute_context_bonus = |word_lower: &str| -> f32 {
            let mut bonus = if has_context {
                let mut sc = collocations::score_candidate_in_clause_context(
                    &prev_words,
                    word_lower,
                    &next_word,
                );
                if let Some(ref actx) = acro_ctx {
                    sc += collocations::score_candidate_with_acronym_frame(actx, word_lower);
                }
                sc
            } else {
                0.0
            };

            if !prev_last.is_empty() {
                if let Some((idx, _)) = session_mem
                    .recent_bigrams
                    .iter()
                    .rev()
                    .enumerate()
                    .find(|(_, (p, c))| p == prev_last && c.to_lowercase() == word_lower)
                {
                    bonus += 35.0 - (idx as f32 * 0.4).min(15.0);
                }
            }

            if let Some((idx, _)) = session_mem
                .recent_words
                .iter()
                .rev()
                .enumerate()
                .find(|(_, (w_low, _))| w_low == word_lower)
            {
                bonus += 26.0 - (idx as f32 * 0.25).min(12.0);
            }

            if word_lower.chars().count() >= 4
                && earlier_content_words.iter().any(|cw| cw == word_lower)
            {
                bonus += 14.0;
            }

            if prev_last == "please"
                && matches!(
                    word_lower,
                    "write" | "check" | "send" | "help" | "read" | "tell" | "give" | "share"
                        | "find" | "wait" | "note" | "confirm" | "make" | "take" | "let"
                )
            {
                bonus += 14.0;
            }

            bonus
        };

        // 3. 彙整搭配詞、文法構式、近期會話記憶、泛歐變音前綴與全詞典計分前綴候選
        let mut best_prefix: HashMap<String, (String, f32)> = HashMap::new();
        let mut multiword_matches: Vec<(String, f32)> = Vec::new();
        let mut upsert_pref = |key_low: String, surface: String, score: f32| {
            if let Some(entry) = best_prefix.get_mut(&key_low) {
                if score > entry.1 {
                    *entry = (surface, score);
                }
            } else {
                best_prefix.insert(key_low, (surface, score));
            }
        };

        if after_space && !prev_words.is_empty() {
            let mut assoc_pool: Vec<(String, f32)> = Vec::new();
            for (rank, c) in collocations::lookup_continuations_for_words(&prev_words, 24)
                .into_iter()
                .enumerate()
            {
                assoc_pool.push((c, 55.0 - (rank as f32 * 1.5)));
            }
            for (rank, c) in collocations::lookup_syntactic_frame_continuations(&prev_words, 24)
                .into_iter()
                .enumerate()
            {
                assoc_pool.push((c, 50.0 - (rank as f32 * 1.2)));
            }
            for (idx, (p_low, c_orig)) in session_mem.recent_bigrams.iter().rev().enumerate() {
                if p_low == prev_last {
                    assoc_pool.push((c_orig.clone(), 72.0 - (idx as f32 * 0.5).min(20.0)));
                }
            }

            for (cand, rank_bonus) in assoc_pool {
                let c_low = cand.to_lowercase();
                if c_low.starts_with(&t_lower) {
                    if cand.contains(' ') {
                        let sc = rank_bonus + compute_context_bonus(&c_low);
                        if !multiword_matches
                            .iter()
                            .any(|(m, _)| m.eq_ignore_ascii_case(&cand))
                        {
                            multiword_matches.push((cand, sc));
                        }
                    } else {
                        let base_w = self.lexicon.get_weight(&c_low).unwrap_or(52.0);
                        let total = base_w + rank_bonus + compute_context_bonus(&c_low);
                        upsert_pref(
                            c_low,
                            Self::apply_candidate_casing(&cand, trimmed),
                            total,
                        );
                    }
                }
            }
        }

        for (surface, bonus, is_multiword) in &acro_cands {
            if *is_multiword {
                if !multiword_matches
                    .iter()
                    .any(|(m, _)| m.eq_ignore_ascii_case(surface))
                {
                    multiword_matches.push((surface.clone(), 50.0 + *bonus));
                }
            } else {
                let c_low = surface.to_lowercase();
                let base_w = self.lexicon.get_weight(&c_low).unwrap_or(58.0).max(66.0);
                let total = base_w + (*bonus * 2.0) + compute_context_bonus(&c_low);
                upsert_pref(c_low, surface.clone(), total);
            }
        }

        for (idx, (w_low, w_orig)) in session_mem.recent_words.iter().rev().enumerate() {
            if w_low.starts_with(&t_lower) && w_low != &t_lower {
                let base_w = self.lexicon.get_weight(w_low).unwrap_or(58.0);
                let recency_bonus = 46.0 - (idx as f32 * 0.5).min(20.0);
                let total = base_w + recency_bonus + compute_context_bonus(w_low);
                let surface = if trimmed
                    .chars()
                    .all(|c| !c.is_alphabetic() || c.is_lowercase())
                {
                    w_orig.clone()
                } else {
                    Self::apply_candidate_casing(w_orig, trimmed)
                };
                upsert_pref(w_low.clone(), surface, total);
            }
        }

        for cand in collocations::lookup_pan_european_prefix_candidates(&prev_words, &t_lower) {
            let c_low = cand.to_lowercase();
            let base_w = self.lexicon.get_weight(&c_low).unwrap_or(54.0);
            let total = base_w + 18.0 + compute_context_bonus(&c_low);
            upsert_pref(
                c_low,
                Self::apply_candidate_casing(&cand, trimmed),
                total,
            );
        }

        for w in self
            .lexicon
            .lookup_prefix_scored(&t_lower, 24, |w_low, _base_w| compute_context_bonus(w_low))
        {
            let w_low = w.to_lowercase();
            let base_w = self.lexicon.get_weight(&w_low).unwrap_or(45.0);
            let total = base_w + compute_context_bonus(&w_low);
            upsert_pref(w_low, Self::apply_candidate_casing(&w, trimmed), total);
        }

        let mut sorted_pref: Vec<(String, f32)> = best_prefix.into_values().collect();
        sorted_pref.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        sorted_pref.truncate(24);

        multiword_matches.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut phonetic_pool: HashMap<String, (String, f32)> = HashMap::new();
        for (w, eff_w, phon_bonus, _best_dist) in
            phonetics::lookup_phonetic_candidates(&self.lexicon, trimmed, 18)
        {
            let w_low = w.to_lowercase();
            let surface = if w_low == "entschuldigung"
                && trimmed
                    .chars()
                    .all(|c| !c.is_alphabetic() || c.is_lowercase())
            {
                "Entschuldigung".to_string()
            } else {
                Self::apply_candidate_casing(&w, trimmed)
            };
            let sc = eff_w + phon_bonus + compute_context_bonus(&w_low);
            if let Some(entry) = phonetic_pool.get_mut(&w_low) {
                if sc > entry.1 {
                    *entry = (surface, sc);
                }
            } else {
                phonetic_pool.insert(w_low, (surface, sc));
            }
        }
        let has_non_initial_vowel = t_lower
            .chars()
            .skip(1)
            .any(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y'));
        for w in self.lexicon.lookup_skeleton_and_fuzzy(&t_lower, 8) {
            let w_low = w.to_lowercase();
            if !phonetic_pool.contains_key(&w_low) {
                let base_w = self.lexicon.get_weight(&w_low).unwrap_or(40.0);
                let skel_bonus = if has_non_initial_vowel && w_low.len() >= t_lower.len() + 2 {
                    -18.0
                } else {
                    18.0
                };
                let sc = base_w + skel_bonus + compute_context_bonus(&w_low);
                phonetic_pool.insert(w_low, (Self::apply_candidate_casing(&w, trimmed), sc));
            }
        }
        let mut fuzzy_scored: Vec<(String, f32)> = phonetic_pool.into_values().collect();
        fuzzy_scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let push_pref_and_multi = |res: &mut Vec<String>| {
            let mut inserted_multi = false;
            for (surface, _) in &sorted_pref {
                if !res.iter().any(|e| e.eq_ignore_ascii_case(surface)) {
                    res.push(surface.clone());
                }
                if !inserted_multi {
                    inserted_multi = true;
                    for (mw, _) in multiword_matches.iter().take(4) {
                        if !res.iter().any(|e| e.eq_ignore_ascii_case(mw)) {
                            res.push(mw.clone());
                        }
                    }
                }
            }
            if !inserted_multi {
                for (mw, _) in multiword_matches.iter().take(4) {
                    if !res.iter().any(|e| e.eq_ignore_ascii_case(mw)) {
                        res.push(mw.clone());
                    }
                }
            }
        };

        let push_deferred_exact_rescues = |res: &mut Vec<String>| {
            if !acro_cands.is_empty() {
                for w in self.collect_up_arrow_exact_rescues(trimmed) {
                    if !res.contains(&w) {
                        res.push(w);
                    }
                }
            }
        };

        let top_orth_base_w = sorted_pref
            .first()
            .and_then(|(s, _)| self.lexicon.get_weight(&s.to_lowercase()))
            .unwrap_or(0.0);
        let strong_orth_prefix = !acro_cands.is_empty()
            || t_lower.chars().count() < 3
            || self.lexicon.prefix_count(&t_lower) >= 5
            || top_orth_base_w >= 40.0;

        if strong_orth_prefix && (self.lexicon.has_prefix(&t_lower) || !sorted_pref.is_empty()) {
            push_pref_and_multi(&mut results);
            push_deferred_exact_rescues(&mut results);
            for (fw, _) in fuzzy_scored {
                if !results.iter().any(|e| e.eq_ignore_ascii_case(&fw)) {
                    results.push(fw);
                }
            }
        } else if !sorted_pref.is_empty() {
            // 弱前綴雜訊（如 rait->raito、rong->rongpur）：保留正字法前綴 +36.0 忠實度加分後與音韻近似統一競技排序
            let mut merged_pool: HashMap<String, (String, f32)> = HashMap::new();
            for (surface, sc) in &sorted_pref {
                let k = surface.to_lowercase();
                let boosted = *sc + 36.0;
                if let Some(entry) = merged_pool.get_mut(&k) {
                    if boosted > entry.1 {
                        *entry = (surface.clone(), boosted);
                    }
                } else {
                    merged_pool.insert(k, (surface.clone(), boosted));
                }
            }
            for (surface, sc) in fuzzy_scored {
                let k = surface.to_lowercase();
                if let Some(entry) = merged_pool.get_mut(&k) {
                    if sc > entry.1 {
                        *entry = (surface, sc);
                    }
                } else {
                    merged_pool.insert(k, (surface, sc));
                }
            }
            let mut merged_list: Vec<(String, f32)> = merged_pool.into_values().collect();
            merged_list.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
            for (surface, _) in merged_list {
                if !results.iter().any(|e| e.eq_ignore_ascii_case(&surface)) {
                    results.push(surface);
                }
            }
            for (mw, _) in multiword_matches.iter().take(4) {
                if !results.iter().any(|e| e.eq_ignore_ascii_case(mw)) {
                    results.push(mw.clone());
                }
            }
            push_deferred_exact_rescues(&mut results);
        } else {
            for (fw, _) in fuzzy_scored {
                if !results.iter().any(|e| e.eq_ignore_ascii_case(&fw)) {
                    results.push(fw);
                }
            }
            push_pref_and_multi(&mut results);
            push_deferred_exact_rescues(&mut results);
        }

        // 7. 保留原始輸入字串於末位作為保底選項
        if !results.contains(&trimmed.to_string()) {
            results.push(trimmed.to_string());
        }

        results
    }

    /// 判斷原始 ASCII 按鍵序列是否具備高信心之歐文跨軌救援價值（例如在許氏或倉頡誤打 tks、pls、btw、recieve 等）
    pub fn has_high_confidence_rescue(&self, raw_ascii: &str) -> bool {
        let t_lower = raw_ascii.trim().to_lowercase();
        if t_lower.len() < 2 {
            return false;
        }
        if Self::ABBREV_EXPANSIONS.iter().any(|&(k, _)| k == t_lower) {
            return true;
        }
        if Self::MULTILINGUAL_EU_GREETINGS.iter().any(|&(k, _)| k == t_lower) {
            return true;
        }
        if Self::CHINESE_SEMANTIC_BRIDGE.iter().any(|&(k, _)| k == t_lower) {
            return true;
        }
        if Self::CONTRACTION_RESCUE.iter().any(|&(k, _)| k == t_lower) {
            return true;
        }
        if Self::COMMON_MISSPELLINGS.iter().any(|&(k, _)| k == t_lower) {
            return true;
        }
        if Self::LOANWORD_DIACRITICS.iter().any(|&(k, _)| k == t_lower) {
            return true;
        }
        if self.is_language_enabled("sa_am") {
            if sanskrit_am::AM_ABBREV_AND_HEADCODES.binary_search_by_key(&t_lower.as_str(), |&(k, _)| k).is_ok() {
                return true;
            }
            if sanskrit_am::AM_PHRASE_EXPANSIONS.binary_search_by_key(&t_lower.as_str(), |&(k, _)| k).is_ok() {
                return true;
            }
            if sanskrit_am::AM_SANSKRIT_LEXICON.binary_search_by_key(&t_lower.as_str(), |&(k, _)| k).is_ok() {
                return true;
            }
        }
        self.lexicon.contains(&t_lower)
    }

    /// 歐語多語系（英／法／西／德）語法同音異形詞與單音節重音變音詞之雙向語境消歧排序
    /// 同時保留原始輸入的大小寫形態（LowerCase / TitleCase / UPPERCASE）
    pub fn lookup_contextual_confusables(
        &self,
        prev_token: &str,
        token: &str,
        next_token: &str,
    ) -> Vec<(String, f64, &'static str)> {
        let raw_ranked = self.scorer.rank_contextual_confusables(prev_token, token, next_token);
        raw_ranked
            .into_iter()
            .map(|(cand, score, rule)| (Self::apply_casing_pattern(token, &cand), score, rule))
            .collect()
    }

    /// 泛歐文空白後或左括號後（`(cur, cur)`）搭配後繼詞與縮略語展開聯想查詢（支援英／法／德／西 N-gram、正交文法構式與近期會話雙詞記憶）
    pub fn lookup_associative_continuations(
        &self,
        prefix_before_cursor: &str,
        max_results: usize,
    ) -> Vec<String> {
        if max_results == 0
            || !prefix_before_cursor
                .chars()
                .last()
                .map_or(false, |c| matches!(c, ' ' | '\t' | '\u{3000}' | '('))
        {
            return Vec::new();
        }
        let mut merged: Vec<String> = Vec::new();
        let mut push_assoc = |cand: String| {
            if !cand.is_empty() && !merged.iter().any(|e| e.eq_ignore_ascii_case(&cand)) {
                merged.push(cand);
            }
        };
        for c in collocations::lookup_acronym_after_space(prefix_before_cursor, max_results) {
            push_assoc(c);
        }
        if prefix_before_cursor.ends_with('(') {
            merged.truncate(max_results);
            return merged;
        }
        let prev_words = collocations::extract_preceding_western_words(prefix_before_cursor, 4);
        if prev_words.is_empty() {
            merged.truncate(max_results);
            return merged;
        }
        for c in collocations::lookup_continuations_for_words(&prev_words, max_results) {
            push_assoc(c);
        }
        if let Some(last_w) = prev_words.last() {
            let session_mem = self.snapshot_session_memory();
            for (p_low, c_orig) in session_mem.recent_bigrams.iter().rev() {
                if p_low == last_w {
                    push_assoc(c_orig.clone());
                }
            }
        }
        for c in collocations::lookup_syntactic_frame_continuations(&prev_words, max_results) {
            push_assoc(c);
        }
        merged.truncate(max_results);
        merged
    }

    /// 無歧義非詞縮寫還原對照表（供空白鍵自動勘誤 Tier 1a 即時套用；明確排除 cant/wont/ill/id/well/were/wed/lets/its 等實詞）
    const UNAMBIGUOUS_SPACE_CONTRACTIONS: &[(&str, &str)] = &[
        ("dont", "don't"),
        ("doesnt", "doesn't"),
        ("didnt", "didn't"),
        ("isnt", "isn't"),
        ("arent", "aren't"),
        ("wasnt", "wasn't"),
        ("werent", "weren't"),
        ("havent", "haven't"),
        ("hasnt", "hasn't"),
        ("hadnt", "hadn't"),
        ("wouldnt", "wouldn't"),
        ("couldnt", "couldn't"),
        ("shouldnt", "shouldn't"),
        ("mustnt", "mustn't"),
        ("neednt", "needn't"),
        ("im", "I'm"),
        ("ive", "I've"),
        ("youre", "you're"),
        ("youve", "you've"),
        ("youll", "you'll"),
        ("youd", "you'd"),
        ("theyre", "they're"),
        ("theyve", "they've"),
        ("theyll", "they'll"),
        ("theyd", "they'd"),
        ("hes", "he's"),
        ("shes", "she's"),
        ("thats", "that's"),
        ("theres", "there's"),
        ("heres", "here's"),
        ("whos", "who's"),
        ("whats", "what's"),
        ("wheres", "where's"),
        ("whens", "when's"),
        ("whys", "why's"),
        ("hows", "how's"),
    ];

    /// 具真實詞義之歧義字串（絕對禁止於空白鍵自動改寫，僅於 Tab / ↑ 候選列提供選擇）
    const AMBIGUOUS_REAL_WORDS: &[&str] = &[
        "cant", "wont", "ill", "id", "well", "were", "wed", "lets", "its", "fur", "sur", "mas",
        "tu", "a", "ou",
    ];

    /// 擷取 `prefix_before_token` 尾端緊鄰空白前之最後一個原始單字切片與尾端空白區間，
    /// 並驗證該單字左側未緊貼程式碼或路徑分隔符號（如 `.`、`_`、`/`、`\`、`:`、`@`）。
    fn trailing_prev_word_info(prefix_before_token: &str) -> Option<(&str, &str, usize)> {
        let last_ch = prefix_before_token.chars().last()?;
        if !matches!(last_ch, ' ' | '\t' | '\u{3000}') {
            return None;
        }

        let bytes = prefix_before_token;
        let mut word_end_byte = bytes.len();
        for (idx, ch) in bytes.char_indices().rev() {
            if matches!(ch, ' ' | '\t' | '\u{3000}') {
                word_end_byte = idx;
            } else {
                break;
            }
        }
        if word_end_byte == 0 {
            return None;
        }

        let head = &bytes[..word_end_byte];
        let tail_last = head.chars().last()?;
        if !(tail_last.is_ascii_alphanumeric()
            || matches!(tail_last, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}'))
        {
            return None;
        }

        let mut word_start_byte = 0;
        let chars_with_idx: Vec<(usize, char)> = head.char_indices().collect();
        let mut k = chars_with_idx.len();
        while k > 0 {
            let (b_idx, ch) = chars_with_idx[k - 1];
            let is_lat = ch.is_ascii_alphanumeric()
                || matches!(ch, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}');
            if is_lat {
                word_start_byte = b_idx;
                k -= 1;
            } else if matches!(ch, '\'' | '\u{2019}' | '-') && k >= 2 {
                let prev_ch = chars_with_idx[k - 2].1;
                if prev_ch.is_ascii_alphanumeric()
                    || matches!(prev_ch, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}')
                {
                    word_start_byte = b_idx;
                    k -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if word_start_byte > 0 {
            let left_ch = bytes[..word_start_byte].chars().last()?;
            if left_ch.is_alphanumeric()
                || matches!(left_ch, '.' | '_' | '/' | '\\' | ':' | '@' | '#' | '$' | '-')
            {
                return None;
            }
        }

        let prev_orig = &bytes[word_start_byte..word_end_byte];
        let space_between = &bytes[word_end_byte..];
        let span_chars = bytes[word_start_byte..].chars().count();
        Some((prev_orig, space_between, span_chars))
    }

    /// 評估歐文輸入於按下空白鍵時是否觸發高確定性非破壞自動勘誤 (Tier 1a + 封閉語法 Tier 1b)。
    /// 嚴格保護程式碼駝峰／蛇形標識符、網址路徑、已知技術縮寫、全大寫詞及歧義實詞。
    pub fn evaluate_space_autocorrect(
        &self,
        prefix_before_token: &str,
        token: &str,
    ) -> Option<WesternSpaceAutocorrect> {
        if token.is_empty() || token.trim() != token {
            return None;
        }
        // 僅對純 ASCII 英文字母詞元生效（排除數字、底線、點、斜線、已含變音符號或撇號之詞）
        if !token.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        // 保護混合大小寫程式碼標識符 (如 getElementById, myFunc, iOS) 與 len>=2 全大寫縮寫 (TEH, SQL)
        let mut chars_it = token.chars();
        let _first = chars_it.next()?;
        let rest: Vec<char> = chars_it.collect();
        if !rest.is_empty() && token.chars().all(|c| c.is_ascii_uppercase()) {
            return None;
        }
        if rest.iter().any(|c| c.is_ascii_uppercase()) {
            return None;
        }
        // 保護緊接於代碼／路徑／屬性運算子之後的詞元 (如 foo.teh, /var/teh, my_thier)
        if let Some(prev_sigil) = prefix_before_token.chars().last() {
            if matches!(prev_sigil, '.' | '_' | '/' | '\\' | ':' | '@' | '#' | '$' | '-') {
                return None;
            }
        }

        let t_lower = token.to_lowercase();
        if Self::is_common_abbrev(&t_lower) {
            return None;
        }

        let tok_char_len = token.chars().count();
        let prev_words = collocations::extract_preceding_western_words(prefix_before_token, 3);
        let prev_info = Self::trailing_prev_word_info(prefix_before_token);

        // Tier 1b：高確定性封閉多詞語法／變音消歧構式（僅於相同水平子句內觸發）
        if let Some((prev_orig, space_between, prev_span_chars)) = prev_info {
            let prev_lower = prev_orig.to_lowercase();
            let span_2 = prev_span_chars + tok_char_len;
            let orig_span = format!("{}{}{}", prev_orig, space_between, token);

            // 1. 英文單數第一人稱 "i am" -> "I am"
            if prev_orig == "i" && t_lower == "am" {
                let rep = format!("I{}{}", space_between, token);
                if rep != orig_span {
                    return Some(WesternSpaceAutocorrect {
                        replace_chars: span_2,
                        replacement: rep,
                        reason: "tier1b:i_am",
                    });
                }
            }

            // 2. 英文存在句 "their are/is/was/were" -> "there ..."
            if prev_lower == "their"
                && matches!(t_lower.as_str(), "are" | "is" | "was" | "were")
            {
                let fixed_there = Self::apply_casing_pattern(prev_orig, "there");
                let rep = format!("{}{}{}", fixed_there, space_between, token);
                if rep != orig_span {
                    return Some(WesternSpaceAutocorrect {
                        replace_chars: span_2,
                        replacement: rep,
                        reason: "tier1b:their_existential",
                    });
                }
            }

            // 3. 英文比較級 + "then" -> "than"
            if t_lower == "then"
                && matches!(
                    prev_lower.as_str(),
                    "more"
                        | "less"
                        | "better"
                        | "worse"
                        | "bigger"
                        | "smaller"
                        | "faster"
                        | "slower"
                        | "higher"
                        | "lower"
                        | "older"
                        | "younger"
                        | "easier"
                        | "harder"
                        | "rather"
                        | "other"
                )
            {
                let rep = Self::apply_casing_pattern(token, "than");
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: rep,
                    reason: "tier1b:comparative_than",
                });
            }

            // 4. 法語 "bien sur" -> "bien sûr"
            if prev_lower == "bien" && t_lower == "sur" {
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: Self::apply_casing_pattern(token, "sûr"),
                    reason: "tier1b:fr_bien_sur",
                });
            }

            // 5. 法語 "s'il vous/te plait" -> "plaît"
            if t_lower == "plait"
                && prev_words.len() >= 2
                && (prev_words[prev_words.len() - 2] == "s'il"
                    || prev_words[prev_words.len() - 2] == "s’il")
                && matches!(prev_words[prev_words.len() - 1].as_str(), "vous" | "te")
            {
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: Self::apply_casing_pattern(token, "plaît"),
                    reason: "tier1b:fr_sil_plait",
                });
            }

            // 6. 法語 "c'est a dire" -> "c'est à dire"
            if t_lower == "dire"
                && prev_words.len() >= 2
                && (prev_words[prev_words.len() - 2] == "c'est"
                    || prev_words[prev_words.len() - 2] == "c’est")
                && prev_lower == "a"
            {
                let fixed_a = Self::apply_casing_pattern(prev_orig, "à");
                let rep = format!("{}{}{}", fixed_a, space_between, token);
                if rep != orig_span {
                    return Some(WesternSpaceAutocorrect {
                        replace_chars: span_2,
                        replacement: rep,
                        reason: "tier1b:fr_cest_a_dire",
                    });
                }
            }

            // 7. 西班牙語 "buenos dias" -> "días"
            if prev_lower == "buenos" && t_lower == "dias" {
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: Self::apply_casing_pattern(token, "días"),
                    reason: "tier1b:es_buenos_dias",
                });
            }

            // 8. 西班牙語 "mucho/nada/poco mas" -> "más"
            if matches!(prev_lower.as_str(), "mucho" | "nada" | "poco") && t_lower == "mas" {
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: Self::apply_casing_pattern(token, "más"),
                    reason: "tier1b:es_mas",
                });
            }

            // 9. 西班牙語第二人稱動詞前綴 "tu eres/tienes/estas/puedes/sabes/quieres" -> "tú ..."
            if prev_lower == "tu"
                && matches!(
                    t_lower.as_str(),
                    "eres" | "tienes" | "estas" | "puedes" | "sabes" | "quieres"
                )
            {
                let fixed_tu = Self::apply_casing_pattern(prev_orig, "tú");
                let fixed_verb = if t_lower == "estas" {
                    Self::apply_casing_pattern(token, "estás")
                } else {
                    token.to_string()
                };
                let rep = format!("{}{}{}", fixed_tu, space_between, fixed_verb);
                if rep != orig_span {
                    return Some(WesternSpaceAutocorrect {
                        replace_chars: span_2,
                        replacement: rep,
                        reason: "tier1b:es_tu_verb",
                    });
                }
            }

            // 10. 德語書信敬語 "mit freundlichen grussen" -> "Grüßen"
            if t_lower == "grussen"
                && prev_words.len() >= 2
                && prev_words[prev_words.len() - 2] == "mit"
                && prev_words[prev_words.len() - 1] == "freundlichen"
            {
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: "Grüßen".to_string(),
                    reason: "tier1b:de_grussen",
                });
            }

            // 11. 德語介系詞片語 "fur mich/dich/uns/euch/sie" -> "für ..."
            if prev_lower == "fur"
                && matches!(t_lower.as_str(), "mich" | "dich" | "uns" | "euch" | "sie")
            {
                let fixed_fur = Self::apply_casing_pattern(prev_orig, "für");
                let rep = format!("{}{}{}", fixed_fur, space_between, token);
                if rep != orig_span {
                    return Some(WesternSpaceAutocorrect {
                        replace_chars: span_2,
                        replacement: rep,
                        reason: "tier1b:de_fur_pronoun",
                    });
                }
            }
        }

        // Tier 1a：無歧義錯拼與無撇號非詞縮寫還原（不得蓋過詞庫已有合法詞彙及歧義實詞）
        if Self::AMBIGUOUS_REAL_WORDS.contains(&t_lower.as_str()) || self.lexicon.contains(&t_lower) {
            return None;
        }

        for &(raw, fix) in Self::UNAMBIGUOUS_SPACE_CONTRACTIONS {
            if t_lower == raw {
                if raw == "im"
                    && matches!(
                        prev_words.last().map(|s| s.as_str()),
                        Some(
                            "ich"
                                | "bin"
                                | "bist"
                                | "ist"
                                | "sind"
                                | "war"
                                | "waren"
                                | "in"
                                | "und"
                                | "auch"
                                | "noch"
                        )
                    )
                {
                    return None;
                }
                let rep = if raw == "im" || raw == "ive" {
                    fix.to_string()
                } else {
                    Self::apply_casing_pattern(token, fix)
                };
                return Some(WesternSpaceAutocorrect {
                    replace_chars: tok_char_len,
                    replacement: rep,
                    reason: "tier1a:contraction",
                });
            }
        }

        for &(mis, fixes) in Self::COMMON_MISSPELLINGS {
            if t_lower == mis {
                if let Some(&first_fix) = fixes.first() {
                    if first_fix != t_lower {
                        return Some(WesternSpaceAutocorrect {
                            replace_chars: tok_char_len,
                            replacement: Self::apply_casing_pattern(token, first_fix),
                            reason: "tier1a:misspelling",
                        });
                    }
                }
                break;
            }
        }

        None
    }

    #[inline]
    fn is_common_abbrev(lower: &str) -> bool {
        WesternConfidenceScorer::COMMON_ABBREVS.contains(&lower)
    }
}

/// 空白鍵自動勘誤結果（由 `WesternEngine::evaluate_space_autocorrect` 產出，供 TSF 與 Core 共用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WesternSpaceAutocorrect {
    pub replace_chars: usize,
    pub replacement: String,
    pub reason: &'static str,
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

    #[test]
    fn test_western_down_arrow_diacritics_and_loanwords() {
        let eng = WesternEngine::new();

        // 1. 單字母 'a' 按 ↓ 應回傳 á, à, â, ä 等變音同位字
        let a_cands = eng.lookup_down_arrow_candidates("a", Some('a'));
        assert!(a_cands.len() >= 6);
        assert_eq!(a_cands[0], "á");
        assert!(a_cands.contains(&"à".to_string()));
        assert!(a_cands.contains(&"ä".to_string()));
        assert!(a_cands.contains(&"ā".to_string()));

        // 2. 大寫 'E' 按 ↓ 應回傳 É, È, Ê, Ë 等大寫變音字
        let e_cands = eng.lookup_down_arrow_candidates("E", Some('E'));
        assert_eq!(e_cands[0], "É");
        assert!(e_cands.contains(&"È".to_string()));

        // 3. 外來語 "cafe" 與 "resume" 按 ↓ 應優先回傳標準重音整詞
        let cafe_cands = eng.lookup_down_arrow_candidates("cafe", None);
        assert_eq!(cafe_cands[0], "café");

        let resume_cands = eng.lookup_down_arrow_candidates("resume", None);
        assert_eq!(resume_cands[0], "résumé");

        let title_cafe = eng.lookup_down_arrow_candidates("Cafe", None);
        assert_eq!(title_cafe[0], "Café");
    }

    #[test]
    fn test_western_up_arrow_expansion_and_spell_rescue() {
        let eng = WesternEngine::new();

        // 1. 輸入 tks 按 ↑ 必得 thanks 與 thank you
        let tks_cands = eng.lookup_up_arrow_candidates("tks");
        assert_eq!(tks_cands[0], "thanks");
        assert!(tks_cands.contains(&"thank you".to_string()));

        // 2. 輸入 pls 按 ↑ 必得 please
        let pls_cands = eng.lookup_up_arrow_candidates("pls");
        assert_eq!(pls_cands[0], "please");

        // 3. 輸入錯字 recieve / teh 按 ↑ 必得 receive / the
        let rec_cands = eng.lookup_up_arrow_candidates("recieve");
        assert_eq!(rec_cands[0], "receive");

        let teh_cands = eng.lookup_up_arrow_candidates("teh");
        assert_eq!(teh_cands[0], "the");

        // 4. 輸入無撇號縮寫 dont 按 ↑ 必得 don't
        let dont_cands = eng.lookup_up_arrow_candidates("dont");
        assert_eq!(dont_cands[0], "don't");
    }

    #[test]
    fn test_ananda_marga_roman_sanskrit_support() {
        let mut eng = WesternEngine::new();
        eng.set_enabled_languages(&["en".to_string(), "sa_am".to_string()]);

        // 1. 輸入 ananda 按 ↓ 必得 ánanda 與 Ánanda 及 IAST ānanda
        let ananda_cands = eng.lookup_down_arrow_candidates("ananda", None);
        assert!(ananda_cands.contains(&"a\u{0301}nanda".to_string()), "ananda cands: {:?}", ananda_cands);
        assert!(ananda_cands.contains(&"A\u{0301}nanda".to_string()));
        assert!(ananda_cands.contains(&"ānanda".to_string()));

        // 2. 輸入 krsna 或常見拼法 krishna 按 ↓ 必得 krśńa / Krśńa
        let krsna_cands = eng.lookup_down_arrow_candidates("krsna", None);
        assert!(krsna_cands.contains(&"krs\u{0301}n\u{0301}a".to_string()), "krsna cands: {:?}", krsna_cands);
        let krishna_cands = eng.lookup_down_arrow_candidates("krishna", None);
        assert!(krishna_cands.contains(&"krs\u{0301}n\u{0301}a".to_string()), "krishna cands: {:?}", krishna_cands);

        // 3. 輸入 jinana 或標準拼法 jnana 按 ↓ 必得含底抑揚符 U+032D 之 jiṋána
        let jnana_cands = eng.lookup_down_arrow_candidates("jnana", None);
        assert!(jnana_cands.contains(&"jin\u{032d}a\u{0301}na".to_string()), "jnana cands: {:?}", jnana_cands);

        // 4. 輸入 samskrta 或 sanskrit 按 ↓ 必得 saḿskrta
        let skt_cands = eng.lookup_down_arrow_candidates("sanskrit", None);
        assert!(skt_cands.contains(&"sam\u{0301}skrta".to_string()), "sanskrit cands: {:?}", skt_cands);

        // 5. 輸入 ananda / baba 按 ↑ 必得 Ánanda Vacanámrtam、Ánanda Sútram 與 Bábá náma kevalam / Bábá nám kevalam 等正統名相片語
        let up_ananda = eng.lookup_up_arrow_candidates("ananda");
        assert!(up_ananda.contains(&"A\u{0301}nanda Vacana\u{0301}mrtam".to_string()), "up_ananda: {:?}", up_ananda);
        assert!(up_ananda.contains(&"A\u{0301}nanda Su\u{0301}tram".to_string()));
        let up_baba = eng.lookup_up_arrow_candidates("baba");
        assert_eq!(up_baba[0], "Ba\u{0301}ba\u{0301} na\u{0301}ma kevalam");
        assert!(up_baba.contains(&"Ba\u{0301}ba\u{0301} na\u{0301}m kevalam".to_string()));

        // 6. 測試常用縮寫與子音頭碼展開：bnk -> Bábá Náma Kevalam，ksn -> Krśńa
        let up_bnk = eng.lookup_up_arrow_candidates("bnk");
        assert_eq!(up_bnk[0], "Ba\u{0301}ba\u{0301} Na\u{0301}ma Kevalam");
        assert!(up_bnk.contains(&"Ba\u{0301}ba\u{0301} na\u{0301}ma kevalam".to_string()));
        let down_bnk = eng.lookup_down_arrow_candidates("bnk", None);
        assert_eq!(down_bnk[0], "Ba\u{0301}ba\u{0301} Na\u{0301}ma Kevalam");

        let up_ksn = eng.lookup_up_arrow_candidates("ksn");
        assert_eq!(up_ksn[0], "Krs\u{0301}n\u{0301}a");
        let down_ksn = eng.lookup_down_arrow_candidates("ksn", None);
        assert_eq!(down_ksn[0], "Krs\u{0301}n\u{0301}a");

        // 7. 歐文模式下 tks / TKS / thanks 按 ↓ 列出支援的歐洲與梵語各語言致意詞
        let down_tks = eng.lookup_down_arrow_candidates("TKS", None);
        assert!(down_tks.contains(&"Thanks".to_string()), "down_tks: {:?}", down_tks);
        assert!(down_tks.contains(&"Merci".to_string()));
        assert!(down_tks.contains(&"Danke".to_string()));
        assert!(down_tks.contains(&"Gracias".to_string()));
        assert!(down_tks.contains(&"Grazie".to_string()));
        assert!(down_tks.contains(&"Obrigado".to_string()));
        assert!(down_tks.contains(&"Dhanyava\u{0301}d".to_string()));

        // 8. 中文語義橋接表查詢
        let zh_tks = WesternEngine::lookup_chinese_semantic_bridge("tks");
        assert_eq!(zh_tks[0], "謝謝");
        assert!(zh_tks.contains(&"感謝".to_string()));
    }

    #[test]
    fn test_western_shared_lexicon_arc_pointer_equality() {
        let mut e1 = WesternEngine::new();
        let e2 = WesternEngine::new();
        assert!(
            Arc::ptr_eq(&e1.lexicon, &e2.lexicon),
            "多個 WesternEngine 實例必須 O(1) 共享相同之 Arc<WesternLexicon> 唯讀字典單例"
        );

        e1.insert_word("xylophonecustom", 120, 88.0);
        assert!(
            !Arc::ptr_eq(&e1.lexicon, &e2.lexicon),
            "寫入單字時應執行 Copy-on-Write，與全域單例分離"
        );
        assert!(e1.lexicon.contains("xylophonecustom"));
        assert!(!e2.lexicon.contains("xylophonecustom"));
    }

    #[test]
    fn test_western_pillar1_prefix_vs_fuzzy_and_diacritic_completion() {
        let eng = WesternEngine::new();

        // 合法前綴補全必須優先於 Damerau-Levenshtein 或骨架模糊猜測
        let progr = eng.lookup_up_arrow_candidates("progr");
        assert_eq!(progr[0], "program", "progr 前綴應優先命中 program: {:?}", progr);

        let algor = eng.lookup_up_arrow_candidates("algor");
        assert_eq!(algor[0], "algorithm", "algor 前綴應優先命中 algorithm: {:?}", algor);

        let deve = eng.lookup_up_arrow_candidates("deve");
        assert_eq!(deve[0], "development", "deve 前綴應優先命中 development: {:?}", deve);

        // 英／法／德／西外來語重音還原與大小寫保形 (TokenCasing)
        assert_eq!(eng.lookup_up_arrow_candidates("cafe")[0], "café");
        assert_eq!(eng.lookup_up_arrow_candidates("Cafe")[0], "Café");
        assert_eq!(eng.lookup_up_arrow_candidates("desole")[0], "désolé");
        assert_eq!(eng.lookup_up_arrow_candidates("grussen")[0], "grüßen");
        assert_eq!(eng.lookup_up_arrow_candidates("manana")[0], "mañana");
    }

    #[test]
    fn test_western_pillar2_pan_european_collocations_and_clause_isolation() {
        let eng = WesternEngine::new();

        let en_tri = eng.lookup_associative_continuations("thank you very ", 6);
        assert!(en_tri.contains(&"much".to_string()), "en_tri: {:?}", en_tri);

        let en_bi = eng.lookup_associative_continuations("looking forward ", 6);
        assert!(en_bi.contains(&"to".to_string()), "en_bi: {:?}", en_bi);

        let fr = eng.lookup_associative_continuations("merci beaucoup ", 6);
        assert!(fr.contains(&"pour".to_string()), "fr: {:?}", fr);

        let de = eng.lookup_associative_continuations("vielen dank ", 6);
        assert!(de.contains(&"für".to_string()), "de: {:?}", de);

        let es = eng.lookup_associative_continuations("muchas gracias ", 6);
        assert!(es.contains(&"por".to_string()), "es: {:?}", es);

        // 零跨越子句隔離檢查（逗號、換行、全形標點後不得跨區聯想）
        assert!(eng.lookup_associative_continuations("thank you, ", 6).is_empty());
        assert!(eng.lookup_associative_continuations("thank you\n", 6).is_empty());
        assert!(eng.lookup_associative_continuations("thank you，", 6).is_empty());
    }

    #[test]
    fn test_western_pillar3_space_autocorrect_and_false_positive_immunity() {
        let eng = WesternEngine::new();

        // Tier 1a 無歧義拼誤與縮寫（保留字首大寫）
        assert_eq!(
            eng.evaluate_space_autocorrect("", "teh").map(|r| r.replacement),
            Some("the".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("", "Recieve").map(|r| r.replacement),
            Some("Receive".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("", "dont").map(|r| r.replacement),
            Some("don't".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("", "im").map(|r| r.replacement),
            Some("I'm".to_string())
        );

        // Tier 1b 封閉多詞語法／變音消歧構式
        let iam = eng.evaluate_space_autocorrect("i ", "am").unwrap();
        assert_eq!((iam.replace_chars, iam.replacement), (4, "I am".to_string()));
        assert!(eng.evaluate_space_autocorrect("I ", "am").is_none());

        let their_are = eng.evaluate_space_autocorrect("their ", "are").unwrap();
        assert_eq!((their_are.replace_chars, their_are.replacement), (9, "there are".to_string()));

        assert_eq!(
            eng.evaluate_space_autocorrect("more ", "then").map(|r| r.replacement),
            Some("than".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("bien ", "sur").map(|r| r.replacement),
            Some("sûr".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("s'il vous ", "plait").map(|r| r.replacement),
            Some("plaît".to_string())
        );
        let cest = eng.evaluate_space_autocorrect("c'est a ", "dire").unwrap();
        assert_eq!((cest.replace_chars, cest.replacement), (6, "à dire".to_string()));

        let tu_eres = eng.evaluate_space_autocorrect("tu ", "eres").unwrap();
        assert_eq!((tu_eres.replace_chars, tu_eres.replacement), (7, "tú eres".to_string()));
        assert_eq!(
            eng.evaluate_space_autocorrect("buenos ", "dias").map(|r| r.replacement),
            Some("días".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("mucho ", "mas").map(|r| r.replacement),
            Some("más".to_string())
        );
        assert_eq!(
            eng.evaluate_space_autocorrect("mit freundlichen ", "grussen").map(|r| r.replacement),
            Some("Grüßen".to_string())
        );
        let fur_mich = eng.evaluate_space_autocorrect("fur ", "mich").unwrap();
        assert_eq!((fur_mich.replace_chars, fur_mich.replacement), (8, "für mich".to_string()));

        // 零誤改免疫清單：代碼、路徑、全大寫縮寫、歧義實詞、換行跨越、德語 im
        for safe_tok in [
            "getElementById",
            "my_var",
            "SQL",
            "TEH",
            "api",
            "json",
            "v2",
            "cant",
            "wont",
            "ill",
            "id",
            "well",
            "were",
            "wed",
            "lets",
            "its",
        ] {
            assert!(
                eng.evaluate_space_autocorrect("", safe_tok).is_none(),
                "不可誤改保護詞元: {}",
                safe_tok
            );
        }
        assert!(eng.evaluate_space_autocorrect("foo.", "teh").is_none());
        assert!(eng.evaluate_space_autocorrect("/var/", "teh").is_none());
        assert!(eng.evaluate_space_autocorrect("the ", "fur").is_none());
        assert!(eng.evaluate_space_autocorrect("tu ", "libro").is_none());
        assert!(eng.evaluate_space_autocorrect("their\n", "are").is_none());
        assert!(eng.evaluate_space_autocorrect("ich bin ", "im").is_none());
    }

    #[test]
    fn test_western_contextual_syntactic_and_session_prediction_261115() {
        let mut eng_embedded = WesternEngine::new();

        // 1. 內建核心詞庫測試：Where are you g -> going（進行式勝過 get/go/great/good）
        let cands_where = eng_embedded.lookup_contextual_up_arrow_candidates("Where are you ", "g", "");
        assert_eq!(
            cands_where.first().map(|s| s.as_str()),
            Some("going"),
            "embedded Where are you g: {:?}",
            cands_where
        );

        // 2. 非逐字 N-gram 泛化文法構式：Where is your brother g -> going / getting
        let cands_gen =
            eng_embedded.lookup_contextual_up_arrow_candidates("Where is your brother ", "g", "");
        assert_eq!(
            cands_gen.first().map(|s| s.as_str()),
            Some("going"),
            "generalized progressive Where is your brother g: {:?}",
            cands_gen
        );

        // 3. 不定詞結構：Today, we are going to s -> see 排於首位，勝過 so / some / said
        let cands_inf =
            eng_embedded.lookup_contextual_up_arrow_candidates("Today, we are going to ", "s", "");
        assert_eq!(
            cands_inf.first().map(|s| s.as_str()),
            Some("see"),
            "Today, we are going to s: {:?}",
            cands_inf
        );

        // 4. 完成式：Where have you g -> gone（克服 given 靜態高權重干擾）
        let cands_perf = eng_embedded.lookup_contextual_up_arrow_candidates("Where have you ", "g", "");
        assert_eq!(
            cands_perf.first().map(|s| s.as_str()),
            Some("gone"),
            "Where have you g: {:?}",
            cands_perf
        );

        // 5. 雙向右文消歧與副詞穿透
        let cands_right = eng_embedded.lookup_contextual_up_arrow_candidates("", "their", " are");
        assert_eq!(cands_right.first().map(|s| s.as_str()), Some("there"), "their are: {:?}", cands_right);

        // 6. 多語種正交構式：法語 j'ai f -> fait / fini；西班牙語／德語搭配
        let cands_fr = eng_embedded.lookup_contextual_up_arrow_candidates("j'ai ", "f", "");
        assert!(
            cands_fr.iter().take(4).any(|w| w == "fait" || w == "fini"),
            "French PerfectHave j'ai f: {:?}",
            cands_fr
        );

        // 7. 會話長期記憶與跨子句隔離（逗號左側不連成雙詞搭配）
        eng_embedded.record_committed_text("Yesterday, we deploy Antigravity microservices.");
        let cands_session_word =
            eng_embedded.lookup_contextual_up_arrow_candidates("Please start ", "Antig", "");
        assert_eq!(
            cands_session_word.first().map(|s| s.as_str()),
            Some("Antigravity"),
            "session proper noun: {:?}",
            cands_session_word
        );
        let cands_session_bi =
            eng_embedded.lookup_contextual_up_arrow_candidates("We deploy ", "m", "");
        assert_eq!(
            cands_session_bi.first().map(|s| s.as_str()),
            Some("microservices"),
            "session bigram deploy m: {:?}",
            cands_session_bi
        );
        let assoc_deploy = eng_embedded.lookup_associative_continuations("We should deploy ", 8);
        assert!(
            assoc_deploy.iter().any(|w| w == "Antigravity"),
            "session assoc after deploy: {:?}",
            assoc_deploy
        );
        let assoc_product =
            eng_embedded.lookup_associative_continuations("We should deploy Antigravity ", 8);
        assert!(
            assoc_product.iter().any(|w| w == "microservices"),
            "session assoc after Antigravity: {:?}",
            assoc_product
        );
        assert_eq!(
            std::mem::size_of::<WesternEngine>(),
            std::mem::size_of::<(WesternFSM, Arc<WesternLexicon>, WesternConfidenceScorer)>(),
            "WesternEngine 必須維持既有三欄位二進位佈局以利免登出熱切換安全運作"
        );
        let recent_bi = eng_embedded.session_recent_bigrams();
        assert!(
            !recent_bi
                .iter()
                .any(|(p, c)| p == "yesterday" && c.eq_ignore_ascii_case("we")),
            "逗號不應跨子句生成 yesterday->we bigram: {:?}",
            recent_bi
        );
        eng_embedded.clear_session_memory();

        // 8. 若磁碟存在完整 37,615 詞 data/lexicon_english.tsv，同步實測完整詞庫排序
        let tsv_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lexicon_english.tsv");
        if std::path::Path::new(tsv_path).exists() {
            let (full_lex, _) = WesternLexicon::shared_from_tsv_with_count(tsv_path)
                .expect("lexicon_english.tsv 應可載入");
            let eng_full = WesternEngine::new_with_shared_lexicon(full_lex);

            let full_where = eng_full.lookup_contextual_up_arrow_candidates("Where are you ", "g", "");
            assert_eq!(
                full_where.first().map(|s| s.as_str()),
                Some("going"),
                "full TSV Where are you g: {:?}",
                full_where
            );

            let full_inf =
                eng_full.lookup_contextual_up_arrow_candidates("Today, we are going to ", "s", "");
            assert_eq!(
                full_inf.first().map(|s| s.as_str()),
                Some("see"),
                "full TSV Today, we are going to s: {:?}",
                full_inf
            );

            let full_perf = eng_full.lookup_contextual_up_arrow_candidates("Where have you ", "g", "");
            assert_eq!(
                full_perf.first().map(|s| s.as_str()),
                Some("gone"),
                "full TSV Where have you g: {:?}",
                full_perf
            );
        }
    }

    #[test]
    fn test_western_acronym_definitional_and_parenthetical_projection_261115() {
        let eng_embedded = WesternEngine::new();

        // 1. 核心雙向縮略語定義展開與大小寫保持（"AI means art" / "AI means Art"）
        let cands_ai_emb = eng_embedded.lookup_contextual_up_arrow_candidates(
            "Where are you going to eat your dinner? AI means ",
            "art",
            "",
        );
        assert_eq!(
            cands_ai_emb.first().map(|s| s.as_str()),
            Some("artificial"),
            "embedded AI means art: {:?}",
            cands_ai_emb
        );
        assert_eq!(
            cands_ai_emb.get(1).map(|s| s.as_str()),
            Some("artificial intelligence"),
            "embedded AI means art multiword: {:?}",
            cands_ai_emb
        );

        let cands_ai_title = eng_embedded.lookup_contextual_up_arrow_candidates("AI means ", "Art", "");
        assert_eq!(
            cands_ai_title.first().map(|s| s.as_str()),
            Some("Artificial"),
            "TitleCase AI means Art: {:?}",
            cands_ai_title
        );
        assert_eq!(
            cands_ai_title.get(1).map(|s| s.as_str()),
            Some("Artificial Intelligence"),
            "TitleCase AI means Art multiword: {:?}",
            cands_ai_title
        );

        // 2. 分槽連續接續（空白後零長插入 + 第二槽詞首補完 + A.I. 帶點縮寫）
        let assoc_ai = eng_embedded.lookup_associative_continuations("AI means ", 8);
        assert_eq!(
            assoc_ai.first().map(|s| s.as_str()),
            Some("artificial intelligence"),
            "after space AI means: {:?}",
            assoc_ai
        );
        let assoc_ai_slot2 = eng_embedded.lookup_associative_continuations("AI means artificial ", 8);
        assert_eq!(
            assoc_ai_slot2.first().map(|s| s.as_str()),
            Some("intelligence"),
            "after space AI means artificial: {:?}",
            assoc_ai_slot2
        );
        let cands_ai_slot2 =
            eng_embedded.lookup_contextual_up_arrow_candidates("AI means artificial ", "i", "");
        assert_eq!(
            cands_ai_slot2.first().map(|s| s.as_str()),
            Some("intelligence"),
            "AI means artificial i: {:?}",
            cands_ai_slot2
        );
        let cands_dotted =
            eng_embedded.lookup_contextual_up_arrow_candidates("A.I. means ", "art", "");
        assert_eq!(
            cands_dotted.first().map(|s| s.as_str()),
            Some("artificial"),
            "A.I. means art: {:?}",
            cands_dotted
        );

        // 3. 多元定義片語（stands for / refers to / is short for）
        let cands_nlp =
            eng_embedded.lookup_contextual_up_arrow_candidates("NLP stands for ", "nat", "");
        assert_eq!(
            cands_nlp.first().map(|s| s.as_str()),
            Some("natural"),
            "NLP stands for nat: {:?}",
            cands_nlp
        );
        assert_eq!(
            cands_nlp.get(1).map(|s| s.as_str()),
            Some("natural language processing"),
            "NLP stands for nat multiword: {:?}",
            cands_nlp
        );
        let cands_gui =
            eng_embedded.lookup_contextual_up_arrow_candidates("GUI refers to ", "gr", "");
        assert_eq!(
            cands_gui.first().map(|s| s.as_str()),
            Some("graphical"),
            "GUI refers to gr: {:?}",
            cands_gui
        );
        let cands_tsf =
            eng_embedded.lookup_contextual_up_arrow_candidates("TSF is short for ", "t", "");
        assert_eq!(
            cands_tsf.first().map(|s| s.as_str()),
            Some("text"),
            "TSF is short for t: {:?}",
            cands_tsf
        );

        // 4. 小寫同形詞免疫防護（it/who 小寫不誤觸發，大寫 IT/WHO 精準觸發）
        let cands_lower_it =
            eng_embedded.lookup_contextual_up_arrow_candidates("it means ", "inf", "");
        assert!(
            !cands_lower_it
                .iter()
                .any(|c| c.eq_ignore_ascii_case("information technology")),
            "lowercase 'it means inf' must not trigger IT expansion: {:?}",
            cands_lower_it
        );
        let cands_upper_it =
            eng_embedded.lookup_contextual_up_arrow_candidates("IT means ", "inf", "");
        assert_eq!(
            cands_upper_it.first().map(|s| s.as_str()),
            Some("information"),
            "uppercase IT means inf: {:?}",
            cands_upper_it
        );
        assert_eq!(
            cands_upper_it.get(1).map(|s| s.as_str()),
            Some("information technology"),
            "uppercase IT means inf multiword: {:?}",
            cands_upper_it
        );

        let cands_lower_who =
            eng_embedded.lookup_contextual_up_arrow_candidates("who stands for ", "w", "");
        assert!(
            !cands_lower_who
                .iter()
                .any(|c| c.eq_ignore_ascii_case("world health organization")),
            "lowercase 'who stands for w' must not trigger WHO expansion: {:?}",
            cands_lower_who
        );
        let cands_upper_who =
            eng_embedded.lookup_contextual_up_arrow_candidates("WHO stands for ", "w", "");
        assert_eq!(
            cands_upper_who.first().map(|s| s.as_str()),
            Some("world"),
            "uppercase WHO stands for w: {:?}",
            cands_upper_who
        );
        assert_eq!(
            cands_upper_who.get(1).map(|s| s.as_str()),
            Some("world health organization"),
            "uppercase WHO stands for w multiword: {:?}",
            cands_upper_who
        );

        // 5. 括號正向與反向對偶（含開放詞彙 TitleCase 動態縮略合成）
        let assoc_llm = eng_embedded.lookup_associative_continuations("LLM (", 8);
        assert_eq!(
            assoc_llm.first().map(|s| s.as_str()),
            Some("large language model"),
            "LLM (: {:?}",
            assoc_llm
        );
        let cands_llm = eng_embedded.lookup_contextual_up_arrow_candidates("LLM (", "l", "");
        assert_eq!(
            cands_llm.first().map(|s| s.as_str()),
            Some("large"),
            "LLM (l: {:?}",
            cands_llm
        );
        let assoc_rev =
            eng_embedded.lookup_associative_continuations("artificial intelligence (", 8);
        assert_eq!(
            assoc_rev.first().map(|s| s.as_str()),
            Some("AI"),
            "artificial intelligence (: {:?}",
            assoc_rev
        );
        let cands_rev =
            eng_embedded.lookup_contextual_up_arrow_candidates("artificial intelligence (", "a", "");
        assert_eq!(
            cands_rev.first().map(|s| s.as_str()),
            Some("AI"),
            "artificial intelligence (a: {:?}",
            cands_rev
        );
        let cands_dyn = eng_embedded.lookup_contextual_up_arrow_candidates(
            "Responsive User Interface (",
            "r",
            "",
        );
        assert_eq!(
            cands_dyn.first().map(|s| s.as_str()),
            Some("RUI"),
            "Responsive User Interface (r: {:?}",
            cands_dyn
        );

        // 6. 完整 37,615 詞 data/lexicon_english.tsv 實測（克服 artists=49.77, artha=46.33 靜態高權重遮蔽＋未登錄 AL 形態槽預測）
        let tsv_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lexicon_english.tsv");
        if std::path::Path::new(tsv_path).exists() {
            let (full_lex, _) = WesternLexicon::shared_from_tsv_with_count(tsv_path)
                .expect("lexicon_english.tsv 應可載入");
            let eng_full = WesternEngine::new_with_shared_lexicon(full_lex);

            let full_ai = eng_full.lookup_contextual_up_arrow_candidates(
                "Where are you going to eat your dinner? AI means ",
                "art",
                "",
            );
            assert_eq!(
                full_ai.first().map(|s| s.as_str()),
                Some("artificial"),
                "full TSV exact screenshot case AI means art: {:?}",
                full_ai
            );
            assert_eq!(
                full_ai.get(1).map(|s| s.as_str()),
                Some("artificial intelligence"),
                "full TSV exact screenshot case AI means art multiword: {:?}",
                full_ai
            );

            let full_al = eng_full.lookup_contextual_up_arrow_candidates("AL means ", "art", "");
            let pos_artistic = full_al.iter().position(|w| w == "artistic");
            let pos_artificial = full_al.iter().position(|w| w == "artificial");
            let pos_artists = full_al.iter().position(|w| w == "artists");
            assert!(
                matches!((pos_artistic, pos_artificial), (Some(_), Some(_))),
                "full TSV AL means art should surface adjectival modifiers: {:?}",
                full_al
            );
            if let (Some(p_adj), Some(p_art), Some(p_pl)) =
                (pos_artistic, pos_artificial, pos_artists)
            {
                assert!(
                    p_adj < p_pl && p_art < p_pl,
                    "full TSV AL means art adjectivals ({}, {}) should rank before plural artists ({}): {:?}",
                    p_adj,
                    p_art,
                    p_pl,
                    full_al
                );
            }
        }
    }

    #[test]
    fn test_western_l2_phonetic_approximation_and_contextual_rescue_261115() {
        assert_eq!(
            std::mem::size_of::<WesternEngine>(),
            std::mem::size_of::<(WesternFSM, Arc<WesternLexicon>, WesternConfidenceScorer)>(),
            "WesternEngine 必須維持 3 欄位二進位佈局不變以確保熱重載記憶體相容"
        );

        let verify_engine = |eng: &WesternEngine, label: &str| {
            // 1. 整詞泛歐語拼音近似校正（涵蓋不發音字首、希臘／羅曼詞根、音節綴合併與雙寫脫落）
            let whole_word_cases: &[(&str, &str)] = &[
                ("foto", "photo"),
                ("fotograf", "photograph"),
                ("filosofi", "philosophy"),
                ("sikoloji", "psychology"),
                ("psikology", "psychology"),
                ("nolej", "knowledge"),
                ("knolege", "knowledge"),
                ("rong", "wrong"),
                ("saiens", "science"),
                ("skejul", "schedule"),
                ("restorant", "restaurant"),
                ("nesesari", "necessary"),
                ("pronounsiation", "pronunciation"),
                ("acomodashun", "accommodation"),
                ("foren", "foreign"),
                ("gramer", "grammar"),
                ("ritm", "rhythm"),
                ("algoritm", "algorithm"),
                ("arkitekcher", "architecture"),
                ("ailand", "island"),
                ("kasel", "castle"),
                ("resit", "receipt"),
                ("kemistri", "chemistry"),
                ("karakter", "character"),
                ("jeneral", "general"),
            ];
            for &(query, expected) in whole_word_cases {
                let cands = eng.lookup_up_arrow_candidates(query);
                assert_eq!(
                    cands.first().map(|s| s.as_str()),
                    Some(expected),
                    "[{}] 整詞拼音近似 `{}` 應以 `{}` 居首位，實際: {:?}",
                    label,
                    query,
                    expected,
                    cands
                );
            }
            let koleg_cands = eng.lookup_up_arrow_candidates("koleg");
            assert!(
                matches!(koleg_cands.first().map(|s| s.as_str()), Some("college") | Some("colleague")),
                "[{}] `koleg` 首位應為 college 或 colleague，實際: {:?}",
                label,
                koleg_cands
            );

            // 2. 拼音近似前綴補全（僅記起近似開頭發音）
            let prefix_cases: &[(&str, &str)] = &[
                ("filos", "philosophy"),
                ("fotog", "photograph"),
                ("sikol", "psychology"),
                ("arkit", "architecture"),
                ("pronuns", "pronunciation"),
                ("neses", "necessary"),
            ];
            for &(query, expected) in prefix_cases {
                let cands = eng.lookup_up_arrow_candidates(query);
                assert_eq!(
                    cands.first().map(|s| s.as_str()),
                    Some(expected),
                    "[{}] 拼音近似前綴 `{}` 應補全 `{}` 居首，實際: {:?}",
                    label,
                    query,
                    expected,
                    cands
                );
            }

            // 3. 亞洲母語負遷移 (L <-> R) 與跨歐語拼音近似（法／西／德）
            let l2_eu_cases: &[(&str, &str)] = &[
                ("fliend", "friend"),
                ("pararell", "parallel"),
                ("bonjur", "bonjour"),
                ("mersi", "merci"),
                ("boku", "beaucoup"),
                ("grasias", "gracias"),
                ("entshuldigung", "Entschuldigung"),
            ];
            for &(query, expected) in l2_eu_cases {
                let cands = eng.lookup_up_arrow_candidates(query);
                assert_eq!(
                    cands.first().map(|s| s.as_str()),
                    Some(expected),
                    "[{}] 跨語／負遷移近似 `{}` 應以 `{}` 居首，實際: {:?}",
                    label,
                    query,
                    expected,
                    cands
                );
            }

            // 4. 子句語法框架同音消歧
            let write_inf = eng.lookup_contextual_up_arrow_candidates("I want to ", "rite", "");
            assert_eq!(
                write_inf.first().map(|s| s.as_str()),
                Some("write"),
                "[{}] 不定詞後 `rite` 應排 write 第一: {:?}",
                label,
                write_inf
            );
            let right_pred = eng.lookup_contextual_up_arrow_candidates("You are ", "rite", "");
            assert_eq!(
                right_pred.first().map(|s| s.as_str()),
                Some("right"),
                "[{}] Be 動詞後 `rite` 應排 right 第一: {:?}",
                label,
                right_pred
            );
            let write_imp = eng.lookup_contextual_up_arrow_candidates("Please ", "rait", "");
            assert_eq!(
                write_imp.first().map(|s| s.as_str()),
                Some("write"),
                "[{}] 祈使句 Please 後 `rait` 應排 write 第一: {:?}",
                label,
                write_imp
            );
            let photo_det = eng.lookup_contextual_up_arrow_candidates("take a ", "foto", "");
            assert_eq!(
                photo_det.first().map(|s| s.as_str()),
                Some("photo"),
                "[{}] take a 後 `foto` 應排 photo 第一: {:?}",
                label,
                photo_det
            );
            let night_good = eng.lookup_contextual_up_arrow_candidates("good ", "nait", "");
            assert_eq!(
                night_good.first().map(|s| s.as_str()),
                Some("night"),
                "[{}] good 後 `nait` 應排 night 第一: {:?}",
                label,
                night_good
            );

            // 5. 既有功能零退化保護
            let going_ctx = eng.lookup_contextual_up_arrow_candidates("Where are you ", "g", "");
            assert_eq!(
                going_ctx.first().map(|s| s.as_str()),
                Some("going"),
                "[{}] 既有前綴語境 Where are you g 不可退化: {:?}",
                label,
                going_ctx
            );
            let ai_ctx = eng.lookup_contextual_up_arrow_candidates("AI means ", "art", "");
            assert_eq!(
                ai_ctx.first().map(|s| s.as_str()),
                Some("artificial"),
                "[{}] 既有縮略語定義 AI means art 不可退化: {:?}",
                label,
                ai_ctx
            );
            assert!(
                ai_ctx.iter().any(|s| s == "artificial intelligence"),
                "[{}] 既有縮略語多詞展開不可遺失: {:?}",
                label,
                ai_ctx
            );
        };

        let eng_embedded = WesternEngine::new();
        verify_engine(&eng_embedded, "embedded");

        let tsv_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lexicon_english.tsv");
        if std::path::Path::new(tsv_path).exists() {
            let (full_lex, _) = WesternLexicon::shared_from_tsv_with_count(tsv_path)
                .expect("lexicon_english.tsv 應可載入");
            let eng_full = WesternEngine::new_with_shared_lexicon(full_lex);
            verify_engine(&eng_full, "full_tsv");
        }
    }

    #[test]
    fn test_sanskrit_pack_default_off_isolation_and_toggle() {
        // 1. 預設僅啟用 ["en"]，sa_am 預設關閉
        let mut eng = WesternEngine::new();
        assert_eq!(eng.enabled_languages(), vec!["en".to_string()]);
        assert!(!eng.is_language_enabled("sa_am"));

        // 2. 預設關閉下，bnk / ananda / krsna 不應滲入阿南達瑪迦詞彙，TKS 不應含 Dhanyavád
        let down_bnk_off = eng.lookup_down_arrow_candidates("bnk", None);
        assert!(
            !down_bnk_off.iter().any(|s| s == "Ba\u{0301}ba\u{0301} Na\u{0301}ma Kevalam"),
            "預設關閉 sa_am 時 bnk 不應觸發阿南達瑪迦縮寫展開: {:?}",
            down_bnk_off
        );
        let down_ananda_off = eng.lookup_down_arrow_candidates("ananda", None);
        assert!(
            !down_ananda_off.iter().any(|s| s == "a\u{0301}nanda" || s == "ānanda"),
            "預設關閉 sa_am 時 ananda 不應觸發梵文專用轉寫: {:?}",
            down_ananda_off
        );
        let down_tks_off = eng.lookup_down_arrow_candidates("TKS", None);
        assert!(down_tks_off.contains(&"Thanks".to_string()));
        assert!(
            !down_tks_off.iter().any(|s| s == "Dhanyava\u{0301}d"),
            "預設關閉 sa_am 時 TKS 不應包含 Dhanyavád: {:?}",
            down_tks_off
        );
        assert!(!eng.has_high_confidence_rescue("bnk"));
        assert!(!eng.has_high_confidence_rescue("parama"));

        // 3. 載入主英文詞庫與動態切換 sa_am 開關（ON 載入 pack_western_sanskrit.tsv，OFF 即刻移除）
        let tsv_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lexicon_english.tsv");
        if std::path::Path::new(tsv_path).exists() {
            eng.load_tsv(tsv_path).expect("應可載入乾淨英文主詞庫");
            assert!(
                !eng.lexicon.contains("parama"),
                "主英文詞庫去傾後不應包含 parama"
            );

            eng.set_enabled_languages(&["en".to_string(), "sa_am".to_string()]);
            assert!(eng.is_language_enabled("sa_am"));
            assert!(eng.lexicon.contains("parama"), "啟用 sa_am 後應載入 pack_western_sanskrit.tsv");
            let down_tks_on = eng.lookup_down_arrow_candidates("TKS", None);
            assert!(down_tks_on.contains(&"Dhanyava\u{0301}d".to_string()));
            assert!(eng.has_high_confidence_rescue("bnk"));

            // 切回僅英文：應立即卸載梵文資料包
            eng.set_enabled_languages(&["en".to_string()]);
            assert!(!eng.is_language_enabled("sa_am"));
            assert!(!eng.lexicon.contains("parama"));
            let down_tks_off_again = eng.lookup_down_arrow_candidates("TKS", None);
            assert!(!down_tks_off_again.iter().any(|s| s == "Dhanyava\u{0301}d"));
        }
    }

    #[test]
    fn test_english_lexicon_l2_vs_corpus_parama_deskew() {
        let tsv_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/lexicon_english.tsv");
        if !std::path::Path::new(tsv_path).exists() {
            return;
        }
        let mut eng = WesternEngine::new();
        eng.load_tsv(tsv_path).expect("應可載入 lexicon_english.tsv");

        // parama / dharma 不應再傾壓一般英文語境
        assert!(!eng.lexicon.contains("parama"));
        assert!(!eng.lexicon.contains("krsna"));

        // 日常 L2 高頻詞應穩居 CEFR 權重優勢且可自常見拼誤正確救援
        let hello_w = eng.lexicon.get_weight("hello").unwrap_or(0.0);
        let psychic_w = eng.lexicon.get_weight("psychic").unwrap_or(0.0);
        let cosmic_w = eng.lexicon.get_weight("cosmic").unwrap_or(0.0);
        assert!(
            hello_w > psychic_w && hello_w > cosmic_w,
            "日常高頻詞 hello ({}) 必須高於 psychic ({}) 與 cosmic ({})",
            hello_w,
            psychic_w,
            cosmic_w
        );

        let pairs = [
            ("coffe", "coffee"),
            ("reciept", "receipt"),
            ("resturant", "restaurant"),
            ("scedule", "schedule"),
            ("libary", "library"),
            ("coleague", "colleague"),
            ("helo", "hello"),
        ];
        for (typo, expected) in pairs {
            let cands = eng.lookup_up_arrow_candidates(typo);
            assert!(
                cands.iter().take(3).any(|c| c.eq_ignore_ascii_case(expected)),
                "L2 拼寫救援 '{}' 應在前 3 名內給出 '{}', 實際為 {:?}",
                typo,
                expected,
                cands
            );
        }
    }

    #[test]
    fn test_phonetic_cache_generation_invalidation() {
        let mut eng = WesternEngine::new();
        // 先暖機建立音韻快取（此時不包含虛構詞彙 xqzblap / xqzblop）
        let before = eng.lookup_up_arrow_candidates("xqzblp");
        assert!(!before.iter().any(|w| w == "xqzblap" || w == "xqzblop"));

        // 動態插入詞彙應透過 generation 遞增立即使音韻快取失效重建
        eng.insert_word("xqzblap", 100, 50.0);
        eng.insert_word("xqzblop", 100, 50.0);
        let after_insert = eng.lookup_up_arrow_candidates("xqzblp");
        assert!(
            after_insert.iter().any(|w| w == "xqzblap"),
            "動態插入後音韻快取必須立即命中新詞 xqzblap: {:?}",
            after_insert
        );

        // 同詞典長度覆寫權重（lexicon.len() 不變）亦須觸發 generation 刷新快取排序
        let prev_len = eng.lexicon.len();
        eng.insert_word("xqzblop", 900, 99.0);
        assert_eq!(eng.lexicon.len(), prev_len, "覆寫既有詞不應改變詞典總詞數");
        let after_overwrite = eng.lookup_up_arrow_candidates("xqzblp");
        let pos_op = after_overwrite.iter().position(|w| w == "xqzblop");
        let pos_ap = after_overwrite.iter().position(|w| w == "xqzblap");
        assert!(
            matches!((pos_op, pos_ap), (Some(op), Some(ap)) if op < ap),
            "同長度覆寫權重後，高權重 xqzblop 應立即領先 xqzblap: {:?}",
            after_overwrite
        );
    }

    #[test]
    fn test_eu_and_sanskrit_diacritics_fsm_and_scoring() {
        let eng = WesternEngine::new();
        let scorer = scoring::WesternConfidenceScorer::new();
        // 1. 泛歐變音符號與組合音調標記不可被切斷為符號
        for token in ["café", "naïve", "résumé", "Straße", "España", "über", "François", "a\u{0301}nanda", "jin\u{032d}a\u{0301}na", "Ba\u{0301}ba\u{0301}"] {
            assert!(
                matches!(fsm::WesternToken::classify(token), fsm::WesternTokenKind::Word(_)),
                "變音字詞 '{}' 必須識別為完整 WesternTokenKind::Word",
                token
            );
            let s = scorer.score_token(token, &eng.lexicon);
            assert!(
                s >= 0.75,
                "變音字詞 '{}' 評分應為正常泛歐詞水準 (>= 0.75), 實際為 {}",
                token,
                s
            );
        }

        // 2. WesternFSM 連續按鍵輸入帶組合音標與 Latin-Extended 字元不得提早中斷 Token
        let mut machine = fsm::WesternFSM::new();
        for ch in "café".chars() {
            let committed = machine.process_char(ch);
            assert_eq!(committed, fsm::FsmAction::AppendPreedit, "輸入 'café' 途中應持續累積 preedit");
        }
        let committed_end = machine.process_char(' ');
        assert_eq!(committed_end, fsm::FsmAction::CommitToken("café ".to_string()));

        let mut skt_machine = fsm::WesternFSM::new();
        for ch in "jin\u{032d}a\u{0301}na".chars() {
            let committed = skt_machine.process_char(ch);
            assert_eq!(committed, fsm::FsmAction::AppendPreedit, "輸入 'jiṋána' 組合變音字元時應持續累積 preedit");
        }
        assert_eq!(
            skt_machine.process_char(' '),
            fsm::FsmAction::CommitToken("jin\u{032d}a\u{0301}na ".to_string())
        );
    }
}
