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

    /// 常見歐文外來語與重音標準整詞對照表（供 ↓ 下箭頭查詢）
    pub const LOANWORD_DIACRITICS: &[(&str, &[&str])] = &[
        ("cafe", &["café", "cafés", "cafe"]),
        ("cafes", &["cafés", "café", "cafes"]),
        ("resume", &["résumé", "resumé", "resume"]),
        ("resumes", &["résumés", "resumés", "resumes"]),
        ("naive", &["naïve", "naive"]),
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
        ("uber", &["über", "Uber", "uber"]),
        ("doppelganger", &["doppelgänger", "doppelganger"]),
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
        ("macrame", &["macramé", "macrame"]),
        ("nee", &["née", "nee"]),
        ("outre", &["outré", "outre"]),
        ("passe", &["passé", "passe"]),
        ("pate", &["pâté", "pate"]),
        ("risque", &["risqué", "risque"]),
        ("rose", &["rosé", "rose"]),
        ("souffle", &["soufflé", "souffle"]),
        ("francais", &["français", "Français", "francais"]),
        ("espanol", &["español", "Español", "espanol"]),
        ("zurich", &["Zürich", "Zurich"]),
        ("munchen", &["München", "Munich"]),
        ("koln", &["Köln", "Cologne"]),
        ("malmo", &["Malmö", "Malmo"]),
        ("goteborg", &["Göteborg", "Gothenburg"]),
        ("sao", &["São", "São Paulo", "sao"]),
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
            'n' => &["ñ", "ń", "ň", "ņ", "ŋ", "ṅ", "ṇ", "n"],
            'N' => &["Ñ", "Ń", "Ň", "Ņ", "Ŋ", "Ṅ", "Ṇ", "N"],
            's' => &["ß", "ś", "š", "ş", "ș", "ŝ", "ṣ", "§", "s"],
            'S' => &["ẞ", "Ś", "Š", "Ş", "Ș", "Ŝ", "Ṣ", "§", "S"],
            'z' => &["ź", "ż", "ž", "z"],
            'Z' => &["Ź", "Ż", "Ž", "Z"],
            'l' => &["ł", "ĺ", "ľ", "ļ", "ŀ", "l"],
            'L' => &["Ł", "Ĺ", "Ľ", "Ļ", "Ŀ", "L"],
            'd' => &["ð", "ď", "đ", "ḍ", "d"],
            'D' => &["Ð", "Ď", "Đ", "Ḍ", "D"],
            't' => &["þ", "ť", "ţ", "ț", "ṭ", "™", "t"],
            'T' => &["Þ", "Ť", "Ţ", "Ț", "Ṭ", "™", "T"],
            'r' => &["ŕ", "ř", "ŗ", "ṛ", "®", "r"],
            'R' => &["Ŕ", "Ř", "Ŗ", "Ṛ", "®", "R"],
            'g' => &["ğ", "ĝ", "ġ", "ģ", "g"],
            'G' => &["Ğ", "Ĝ", "Ġ", "Ģ", "G"],
            'h' => &["ĥ", "ħ", "ḥ", "h"],
            'H' => &["Ĥ", "Ħ", "Ḥ", "H"],
            'j' => &["ĵ", "j"],
            'J' => &["Ĵ", "J"],
            'k' => &["ķ", "ĸ", "k"],
            'K' => &["Ķ", "K"],
            'm' => &["ṁ", "ṃ", "µ", "m"],
            'M' => &["Ṁ", "Ṃ", "M"],
            'w' => &["ŵ", "ẁ", "ẃ", "ẅ", "w"],
            'W' => &["Ŵ", "Ẁ", "Ẃ", "Ẅ", "W"],
            _ => &[],
        };
        list.iter().map(|&s| s.to_string()).collect()
    }

    /// 根據原始輸入字串的大小寫形態（LowerCase / TitleCase / UpperCase）套用至候選詞
    fn apply_casing_pattern(source: &str, candidate: &str) -> String {
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
        let trimmed = token.trim();
        let mut results: Vec<String> = Vec::new();

        if trimmed.is_empty() {
            if let Some(ch) = target_char {
                return Self::lookup_char_diacritics(ch);
            }
            return results;
        }

        // 1. 單一字元：直接回傳該字母之完整變音同位字集
        if trimmed.chars().count() == 1 {
            let ch = trimmed.chars().next().unwrap();
            return Self::lookup_char_diacritics(ch);
        }

        // 2. 多字元單字：優先檢查外來語重音整詞表（如 cafe -> café, resume -> résumé, naive -> naïve）
        let normalized_ascii: String = trimmed.chars().map(Self::strip_char_diacritic).collect::<String>().to_ascii_lowercase();
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

        // 3. 若指定了游標所在字母（或取單字末尾母音/字母），提供該字母之單字元變音候選以供局部替換
        let focus_ch = target_char.or_else(|| trimmed.chars().last());
        if let Some(ch) = focus_ch {
            for d in Self::lookup_char_diacritics(ch) {
                if !results.contains(&d) {
                    results.push(d);
                }
            }
        }

        // 4. 補充大小寫形態變體（TitleCase / UPPERCASE / lowercase）
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

    /// 歐語軌 `↑` 上箭頭：智慧拼字校正、縮寫展開（tks -> thanks）、無撇號還原、子音骨架與前綴補全
    pub fn lookup_up_arrow_candidates(&self, token: &str) -> Vec<String> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let t_lower = trimmed.to_ascii_lowercase();
        let mut results: Vec<String> = Vec::new();

        let mut push_unique = |cand: &str| {
            let cased = Self::apply_casing_pattern(trimmed, cand);
            if !cased.is_empty() && !results.contains(&cased) {
                results.push(cased);
            }
        };

        // 1. 高頻縮寫展開 (如 tks -> thanks, thank you; pls -> please; btw -> by the way)
        for &(abbr, expansions) in Self::ABBREV_EXPANSIONS {
            if t_lower == abbr {
                for &exp in expansions {
                    push_unique(exp);
                }
                break;
            }
        }

        // 2. 無撇號縮寫還原 (如 dont -> don't, cant -> can't, im -> I'm)
        for &(raw, rescues) in Self::CONTRACTION_RESCUE {
            if t_lower == raw {
                for &r in rescues {
                    push_unique(r);
                }
                break;
            }
        }

        // 3. 常見拼字錯誤直達修正 (如 recieve -> receive, teh -> the, seperate -> separate)
        for &(mis, fixes) in Self::COMMON_MISSPELLINGS {
            if t_lower == mis {
                for &f in fixes {
                    push_unique(f);
                }
                break;
            }
        }

        // 4. 外來語重音整詞亦納入智慧校正候選 (如 cafe -> café, naive -> naïve)
        for &(key, variants) in Self::LOANWORD_DIACRITICS {
            if t_lower == key {
                for &v in variants {
                    push_unique(v);
                }
                break;
            }
        }

        // 5. 子音骨架匹配與 Damerau-Levenshtein 動態編輯距離校正
        for w in self.lexicon.lookup_skeleton_and_fuzzy(&t_lower, 8) {
            push_unique(&w);
        }

        // 6. 前綴智慧補全 (Prefix Completion)
        for w in self.lexicon.lookup_prefix(&t_lower, 8) {
            push_unique(&w);
        }

        // 7. 保留原始輸入字串於末位作為保底選項
        if !results.contains(&trimmed.to_string()) {
            results.push(trimmed.to_string());
        }

        results
    }

    /// 判斷原始 ASCII 按鍵序列是否具備高信心之歐文跨軌救援價值（例如在許氏或倉頡誤打 tks、pls、btw、recieve 等）
    pub fn has_high_confidence_rescue(&self, raw_ascii: &str) -> bool {
        let t_lower = raw_ascii.trim().to_ascii_lowercase();
        if t_lower.len() < 2 {
            return false;
        }
        if Self::ABBREV_EXPANSIONS.iter().any(|&(k, _)| k == t_lower) {
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
}
