/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/transitions.rs - 語意轉移矩陣與離合詞語法關聯引擎 (Syntactic Transitions & VO Collocation Engine)
///
/// 三層架構 (Three-Layer Architecture):
///   Layer 1: 語法規則 (Grammar Rules) — 通配符模式，覆蓋所有同類搭配
///   Layer 2: 語意搭配 (Learned Bigrams) — 從文件學習的高頻共現對
///   Layer 3: 離合詞 (VO Separable Verbs) — 跨詞隙動賓綁定

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct VOCollocation {
    pub verb: String,
    pub object: String,
    pub max_distance: usize,
    pub bonus_score: f64,
}

/// 語法規則：前驅模式 × 後繼模式 → 加分
#[derive(Debug, Clone)]
pub struct GrammarRule {
    pub prev_pattern: WordPattern,
    pub curr_pattern: WordPattern,
    pub bonus: f64,
    pub name: String,
}

/// 詞彙匹配模式 (Word matching pattern)
#[derive(Debug, Clone)]
pub enum WordPattern {
    /// 精確匹配一個詞
    Exact(String),
    /// 匹配多個詞之一
    OneOf(Vec<String>),
    /// 匹配任何非功能詞的實詞（動詞/形容詞/名詞）
    /// 簡易判斷：不在功能詞集中 && 長度 <= 2
    ContentWord,
    /// 匹配複合實詞（長度 >= 2 且 <= 4 的實詞，排除孤立單字碎片）
    CompoundContentWord,
    /// 匹配實詞但排除指定名單（例如排除名詞/方位詞接得）
    ContentWordExcept(Vec<String>),
    /// 匹配名詞性成分（實詞或人稱代詞）
    Nominal,
    /// 匹配任何詞
    Any,
}

fn is_rare_impostor(word: &str) -> bool {
    matches!(word, "婑" | "倭" | "䂺" | "婐" | "捰" | "𣇫" | "蟆" | "俛" | "犋" | "不憤")
}

impl WordPattern {
    fn matches(&self, word: &str, function_words: &HashSet<String>) -> bool {
        match self {
            WordPattern::Exact(w) => word == w,
            WordPattern::OneOf(ws) => ws.iter().any(|w| w == word),
            WordPattern::ContentWord => {
                // 實詞 = 不是功能詞、不是標點、長度合理、且非罕見生僻孤立字劫掠者
                !function_words.contains(word) && !is_rare_impostor(word) && word.chars().count() <= 4
                    && !word.is_empty()
            }
            WordPattern::CompoundContentWord => {
                // 複合實詞 = 實詞且長度 >= 2（杜絕孤立單字切碎搭便車）
                !function_words.contains(word) && !is_rare_impostor(word)
                    && word.chars().count() >= 2 && word.chars().count() <= 4
            }
            WordPattern::ContentWordExcept(excludes) => {
                !function_words.contains(word) && !is_rare_impostor(word) && !excludes.iter().any(|ex| ex == word)
                    && word.chars().count() <= 4 && !word.is_empty()
            }
            WordPattern::Nominal => {
                (word.chars().count() >= 2 && !function_words.contains(word) && !is_rare_impostor(word) && word.chars().count() <= 4)
                    || matches!(word, "我" | "你" | "妳" | "他" | "她" | "它" | "牠" | "祂" | "我們" | "你們" | "他們" | "人" | "書" | "車" | "貓" | "狗" | "爸" | "媽" | "兄" | "弟" | "姐" | "妹")
            }
            WordPattern::Any => !word.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransitionEngine {
    /// Layer 1: 語法規則（通配模式）
    pub grammar_rules: Vec<GrammarRule>,
    /// Layer 2: 二元轉移快取: (prev_word, curr_word) -> bonus_score
    pub transitions: HashMap<(String, String), f64>,
    /// Layer 3: 離合動賓搭配群
    pub vo_pairs: Vec<VOCollocation>,
    /// 功能詞集合（用於 ContentWord 模式的排除判斷）
    pub function_words: HashSet<String>,
    file_path: Option<PathBuf>,
    last_modified: Option<SystemTime>,
}

impl Default for TransitionEngine {
    fn default() -> Self {
        let mut engine = Self {
            grammar_rules: Vec::new(),
            transitions: HashMap::new(),
            vo_pairs: Vec::new(),
            function_words: HashSet::new(),
            file_path: None,
            last_modified: None,
        };
        engine.init_builtin_knowledge();
        engine
    }
}

impl TransitionEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 初始化內建的漢語核心語法規則
    fn init_builtin_knowledge(&mut self) {
        // ── 功能詞集合（封閉類，不參與 ContentWord 匹配）──
        let func_words = [
            // 助詞
            "的", "得", "地", "了", "著", "過", "嗎", "吧", "呢", "啊", "吶", "啦", "嘛", "囉", "唄",
            // 介詞
            "在", "從", "到", "對", "把", "被", "讓", "給", "跟", "和", "與", "為",
            // 連詞
            "和", "或", "與", "及", "而", "但", "卻", "雖", "因", "所", "以",
            // 副詞（高頻）
            "不", "沒", "很", "都", "也", "就", "才", "再", "又", "還",
            "已", "正", "將", "會", "能", "可", "要", "該", "應", "未", "曾",
            // 代詞
            "我", "你", "妳", "他", "她", "它", "牠", "祂", "我們", "你們", "他們", "她們", "它們", "牠們",
            "這", "那", "這個", "那個", "什麼", "誰", "哪",
            // 量詞
            "個", "位", "隻", "條", "把", "次", "回",
            // 數詞
            "一", "二", "三", "四", "五", "六", "七", "八", "九", "十",
            "百", "千", "萬", "億",
            // 判斷詞
            "是",
            // 疑問/關係副詞與連詞（封閉類，不作為修飾前綴實詞）
            "怎麼", "怎樣", "如何", "為何", "為什麼", "為甚麼", "何以",
            "因為", "所以", "雖然", "但是", "然而", "如果", "要是", "若是", "假使",
            "究竟", "到底", "難道", "偏偏", "豈", "何必",
        ];
        for w in func_words {
            self.function_words.insert(w.to_string());
        }

        // ── Layer 1: 語法規則 ──

        // 體標記 (Aspect markers): 實詞 + {了/過/著/完/好/掉/起來/出來/下去/下來/上去}
        let aspect_markers = vec![
            "了", "過", "著", "完", "好", "掉",
            "起來", "出來", "下去", "下來", "上去", "上來",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::OneOf(aspect_markers.iter().map(|s| s.to_string()).collect()),
            bonus: 12.0,
            name: "aspect_marker".to_string(),
        });

        // 句末疑問/語氣助詞 (Sentence-final particles): 實詞 + {嗎/吧/呢/啊/啦/嘛/呀/哇/囉/喔/唷/耶}
        let sentence_particles = vec![
            "嗎", "吧", "呢", "啊", "啦", "嘛", "呀", "哇", "囉", "喔", "唷", "耶",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::OneOf(sentence_particles.iter().map(|s| s.to_string()).collect()),
            bonus: 24.0,
            name: "sentence_final_particle".to_string(),
        });

        // 語氣助詞後接實詞懲罰 (Sentence particle followed immediately by ContentWord is ungrammatical)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(sentence_particles.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWord,
            bonus: -40.0,
            name: "anti_particle_before_content".to_string(),
        });

        // 抑制罕見孤立單字「蟆」單獨出現在句中/句尾（非蛤蟆複合詞）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("蟆".to_string()),
            bonus: -30.0,
            name: "anti_isolated_ma2".to_string(),
        });

        // 抑制罕見孤立單字「婑」單獨出現在句中（防「我」被古字典生僻字劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("婑".to_string()),
            bonus: -40.0,
            name: "anti_isolated_wo3_impostor".to_string(),
        });

        // 抑制罕見孤立單字「倭」單獨出現在句中（防「我」被次要破音字劫掠，非倭寇等複合詞）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("倭".to_string()),
            bonus: -40.0,
            name: "anti_isolated_wo1_impostor".to_string(),
        });

        // 抑制罕見孤立單字「篼」（防「都」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("篼".to_string()),
            bonus: -35.0,
            name: "anti_isolated_dou_impostor".to_string(),
        });

        // 抑制孤立「究」（非究竟、研究等複合詞，防「就」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("究".to_string()),
            bonus: -30.0,
            name: "anti_isolated_jiu4_impostor".to_string(),
        });

        // 抑制孤立「研」（非研發、研討、研讀等複合詞，防「研究」被切碎殘留孤立單字）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("研".to_string()),
            bonus: -30.0,
            name: "anti_isolated_yan2_impostor".to_string(),
        });

        // 抑制孤立生僻字「儻」（非倜儻、儻來，防「躺」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("儻".to_string()),
            bonus: -35.0,
            name: "anti_isolated_tang3_impostor".to_string(),
        });

        // 抑制孤立「纖」（非纖維、纖細等複合詞，防「先」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("纖".to_string()),
            bonus: -35.0,
            name: "anti_isolated_xian1_impostor".to_string(),
        });

        // 抑制孤立「萊」（非蓬萊、萊茵等複合詞，防「來」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("萊".to_string()),
            bonus: -35.0,
            name: "anti_isolated_lai2_impostor".to_string(),
        });

        // 抑制孤立「趣」（非興趣、趣味等複合名詞，防「去」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("趣".to_string()),
            bonus: -30.0,
            name: "anti_isolated_qu4_impostor".to_string(),
        });

        // 抑制孤立「慧」（非智慧、聰慧等複合名詞，防「會」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("慧".to_string()),
            bonus: -30.0,
            name: "anti_isolated_hui4_impostor".to_string(),
        });

        // 抑制孤立生僻動詞「揩」（非揩油、揩汗等複合詞，防「開」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("揩".to_string()),
            bonus: -35.0,
            name: "anti_isolated_kai1_impostor".to_string(),
        });

        // 抑制孤立單字「抵」（非抵抗、抵達、抵押等複合詞，防「底」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("抵".to_string()),
            bonus: -30.0,
            name: "anti_isolated_di3_impostor".to_string(),
        });

        // 抑制孤立「諱」（非避諱、諱言等複合詞，防「會」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("諱".to_string()),
            bonus: -35.0,
            name: "anti_isolated_hui4_taboo_impostor".to_string(),
        });

        // 抑制非數詞修飾之孤立「棟」（非一棟、那棟等量詞結構，防「動」被劫掠）
        let quant_num = vec!["一", "兩", "二", "三", "四", "五", "六", "七", "八", "九", "十", "幾", "這", "那", "哪", "每", "整"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(quant_num.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("棟".to_string()),
            bonus: -35.0,
            name: "anti_isolated_dong4_classifier_impostor".to_string(),
        });

        // 分配代詞「每」修飾量詞、時間或動詞構式 (每個、每次、每當、每告一段落)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("每".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 25.0,
            name: "distributive_mei3_modifier".to_string(),
        });

        // 文件/副檔名後置「檔」(如 aup檔、md檔、txt檔、wav檔、影片檔、圖檔)
        let file_exts = vec![
            "md", "MD", "txt", "doc", "pdf", "csv", "json", "xml", "log", "rs", "py",
            "aup", "aup3", "wav", "mp3", "flac", "aac", "m4a", "ogg", "mid",
            "prproj", "aep", "blend", "psd", "ai", "zip", "rar", "7z", "tar",
            "圖", "音", "影片", "文", "封存", "備份"
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(file_exts.clone().into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("檔".to_string()),
            bonus: 35.0,
            name: "file_ext_dang_bonus".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(file_exts.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("黨".to_string()),
            bonus: -45.0,
            name: "anti_file_ext_party_impostor".to_string(),
        });


        // 轉折連詞 + 非關 (但非關、但是非關、卻非關、然而非關)
        let adversative = vec!["但", "但是", "卻", "然而", "不過"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(adversative.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("非關".to_string()),
            bonus: 20.0,
            name: "conjunction_feiguan".to_string(),
        });

        // 非關 + 實詞 (非關組字、非關個人、非關利益、非關命運)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("非關".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 16.0,
            name: "feiguan_content".to_string(),
        });

        // 杜絕軍事飛行員名詞「飛官」誤接「組/組字/組自」等偽動賓搭配
        let zu_words = vec!["組", "組字", "組自"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("飛官".to_string()),
            curr_pattern: WordPattern::OneOf(zu_words.into_iter().map(|s| s.to_string()).collect()),
            bonus: -35.0,
            name: "anti_feiguan_zu".to_string(),
        });

        // 動詞/形容詞 + 過的 (V + 過的): 看過的、學過的、調校過的、走過的
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("過的".to_string()),
            bonus: 20.0,
            name: "verb_guo_de".to_string(),
        });

        // 否定助詞/副詞 + 形成 (無法形成、難以形成、不能形成、不易形成)
        let neg_auxiliaries = vec!["無法", "難以", "不能", "不易", "未曾", "足以", "難以"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(neg_auxiliaries.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("形成".to_string()),
            bonus: 25.0,
            name: "neg_aux_form".to_string(),
        });

        // 動詞「形成」+ 名詞賓語 (形成語意、形成共識、形成風格、形成習慣、形成概念、形成默契)
        let form_objects = vec!["語意", "語句", "共識", "風格", "習慣", "概念", "規範", "默契", "風氣", "對比", "迴圈", "印象"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("形成".to_string()),
            curr_pattern: WordPattern::OneOf(form_objects.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "form_object_collocation".to_string(),
        });

        // 杜絕名詞「行程」誤接名詞語意/語句 (防「行程語意」荒謬劫掠)
        let invalid_travel_collocations = vec!["語意", "語句", "共識", "默契"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("行程".to_string()),
            curr_pattern: WordPattern::OneOf(invalid_travel_collocations.into_iter().map(|s| s.to_string()).collect()),
            bonus: -35.0,
            name: "anti_travel_semantic".to_string(),
        });

        // 杜絕「有益」誤接「意/意的」偽搭配 (防「有意義的」被切碎為「有益意的」)
        let invalid_youyi_collocations = vec!["意", "意的", "義的"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("有益".to_string()),
            curr_pattern: WordPattern::OneOf(invalid_youyi_collocations.into_iter().map(|s| s.to_string()).collect()),
            bonus: -35.0,
            name: "anti_youyi_yi".to_string(),
        });

        // 嚴格禁止 動詞/形容詞 + 過得 (*V + 過得): 杜絕 *看過得、*調校過得、*學過得、*走過得
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("過得".to_string()),
            bonus: -35.0,
            name: "anti_verb_guo_de2".to_string(),
        });

        // 過得 + 補語 (過得好、過得很辛苦、過得快、過得充實)
        let de2_complements = vec![
            "很", "好", "快", "去", "充實", "辛苦", "幸福", "開心", "痛苦", "精彩", "不容易",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("過得".to_string()),
            curr_pattern: WordPattern::OneOf(de2_complements.iter().map(|s| s.to_string()).collect()),
            bonus: 25.0,
            name: "guo_de2_complement".to_string(),
        });

        // 程度補語: 實詞 + 到
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("到".to_string()),
            bonus: 15.0,
            name: "degree_complement".to_string(),
        });

        // 時間子句標記構式: 實詞短語 + 的時候 (停車的時候、開社群的時候、吃飯的時候、小的時候)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("的時候".to_string()),
            bonus: 26.0,
            name: "content_de_shihou".to_string(),
        });

        // 結構助詞「的」: 實詞 + 的
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("的".to_string()),
            bonus: 14.0,
            name: "attributive_de".to_string(),
        });

        // 結構助詞與時間名詞引力: 的 ➔ 時候 (必須置於通用 的 + 實詞 之前！)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("時候".to_string()),
            bonus: 30.0,
            name: "de_shihou".to_string(),
        });

        // 「的」後接實詞: 的 + 實詞（的貓、的背心、的人）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 10.0,
            name: "de_noun".to_string(),
        });

        // 結構助詞「地」規範：嚴格恪守「的／地」二分法，做得很清楚，否則寧可全用「的」
        // 1. 抑制「地 + 名詞/代詞/方位詞」（絕對語法禁忌：嚴禁「*勇敢地人」、「*慢慢地我」）
        let de_forbidden_nominals = vec![
            "人", "我", "你", "妳", "他", "她", "它", "牠", "祂", "貓", "狗", "書", "車", "房", "錢",
            "事", "物", "家", "朋友", "同學", "老師", "孩子", "地方", "東西", "手", "臉", "心", "頭",
            "眼睛", "聲音", "樣子", "時候", "人稱", "人選", "學生", "同志", "同仁", "夥伴", "青年",
            "老者", "長者", "父母", "父親", "母親", "兄弟", "姐妹", "生命", "靈魂", "精神", "道路",
            "世界", "國家", "社會", "公司", "單位", "學校", "城市", "家庭", "團體", "組織", "故事",
            "回憶", "夢想", "希望", "願望", "目標", "責任", "使命", "成果", "作品", "文章", "言語",
            "話語", "面孔", "神情", "態度", "目光", "眼神", "心情", "感受", "思想", "觀點", "看法",
            "裡", "外", "內", "中", "上", "下", "前", "後", "旁",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("地".to_string()),
            curr_pattern: WordPattern::OneOf(de_forbidden_nominals.into_iter().map(|s| s.to_string()).collect()),
            bonus: -40.0,
            name: "anti_de_nominal_shatter".to_string(),
        });

        // 2. 結構助詞「地」正向引力：地 ➔ 動作動詞（勇敢地向前走、慢慢地走、認真地思考、高興地說）
        let de_action_verbs = vec![
            "向前走", "向前", "走", "跑", "跳", "飛", "說", "看", "聽", "做", "想", "寫", "笑", "哭",
            "喊", "叫", "問", "答", "前進", "邁進", "奔跑", "奮鬥", "生活", "工作", "思考", "前行",
            "離去", "離開", "站著", "坐著", "走著", "躺著", "凝視", "端詳", "等待", "尋找",
            "堅持", "流淌", "歌唱", "揮手", "招手", "微笑", "點頭", "搖頭", "出發", "走去", "跑去",
            "跳躍", "飛翔", "漫步", "穿行", "摸索", "攀登", "跋涉", "迎向", "走向", "奔向", "衝向",
            "看著", "笑著", "說著", "聽著", "望著", "念著", "想著",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("地".to_string()),
            curr_pattern: WordPattern::OneOf(de_action_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "de_action_verb".to_string(),
        });

        // 3. 典型形容詞/副詞 ➔ 地（狀態修飾標記）
        let de_adjectival_adverbs = vec![
            "勇敢", "慢慢", "悄悄", "認真", "努力", "高興", "開心", "漸漸", "默默", "偷偷", "靜靜",
            "飛快", "自由", "痛苦", "興奮", "滿意", "緊緊", "深深", "大步", "大聲", "小聲", "清楚",
            "明白", "細心", "耐心", "堅決", "果斷", "熱情", "溫柔", "親切", "迅速", "平靜", "安靜",
            "悠閒", "慌張", "從容", "頑強", "堅定", "刻苦", "勤勞", "嚴肅", "嚴格", "猛烈", "劇烈",
            "瘋狂", "拼命", "死命", "用力", "全力", "盡情", "隨意",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(de_adjectival_adverbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("地".to_string()),
            bonus: 12.0,
            name: "adjective_adverbial_de".to_string(),
        });

        // 結構助詞「得」: 動詞/形容詞 + 得（補語標記）
        // 嚴格排除純名詞與方位詞（如南、北、東、西、男、女、人、貓、狗、書、車、房）接得
        let non_de2_nouns = vec![
            "南", "北", "東", "西", "中", "男", "女", "人", "貓", "狗", "書", "車", "房", "錢",
        ].into_iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(non_de2_nouns),
            curr_pattern: WordPattern::Exact("得".to_string()),
            bonus: 10.0,
            name: "complement_de".to_string(),
        });

        // 趨向補語: 動詞實詞 + {來/去/回/進/出/上/下/開/住}
        // 排除名詞、量詞、時間詞接趨向補語（如位、時、年、天、月、本、張、塊、隻）
        let directional = vec![
            "來", "去", "回", "進", "出", "上", "下", "開", "住",
        ];
        let non_directional_stems = vec![
            "位", "時", "月", "年", "天", "個", "本", "張", "塊", "隻", "條", "間", "棟", "日",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(non_directional_stems.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(directional.iter().map(|s| s.to_string()).collect()),
            bonus: 10.0,
            name: "directional_complement".to_string(),
        });

        // 把字句與被字句 (Disposal & Passive): 把/被 + 名詞性成分(含代詞)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("把".to_string()),
            curr_pattern: WordPattern::Nominal,
            bonus: 12.0,
            name: "ba_construction".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("被".to_string()),
            curr_pattern: WordPattern::Nominal,
            bonus: 12.0,
            name: "bei_construction".to_string(),
        });

        // 處置句與被動句之結果/體態補語約束 (Resultative & Perfective Complements)
        let disposal_results = vec![
            "完了", "破了", "好了", "走了", "掉了", "光了", "濕了", "倒了", "到了",
            "黑了", "開了", "空了", "淨了", "碎了", "傷了", "爛了", "死了",
            "乾淨", "清楚", "看穿", "打破", "解決", "吃光", "看光", "花光",
            "淋濕", "吹倒", "撞倒", "抓走", "帶走", "丟掉",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::OneOf(disposal_results.into_iter().map(|s| s.to_string()).collect()),
            bonus: 16.0,
            name: "disposal_result_complement".to_string(),
        });

        // 數詞 + 量詞（一隻、兩個、三條）
        let numerals = vec![
            "一", "二", "兩", "三", "四", "五", "六", "七", "八", "九", "十",
            "幾", "多", "半", "百", "千", "萬",
        ];
        let classifiers = vec![
            "個", "位", "隻", "條", "把", "次", "回", "種", "件", "份", "句",
            "本", "張", "塊", "間", "棟", "輛", "台", "架", "支", "枝",
            "朵", "匹", "頭", "尾", "顆", "粒", "片", "杯", "碗", "盤",
            "部", "座", "所", "家", "款", "項", "套", "組", "段", "幢",
            "艘", "幅", "首", "尊", "面", "扇", "門", "臺", "冊",
            "堂", "列", "班", "口",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(numerals.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(classifiers.iter().map(|s| s.to_string()).collect()),
            bonus: 15.0,
            name: "numeral_classifier".to_string(),
        });

        // 指示代詞 + 量詞構式 (Demonstrative + Classifier: 這部、這座、這份、那本、這隻、這套)
        let demonstratives = vec!["這", "那"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(demonstratives.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(classifiers.iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "demonstrative_classifier".to_string(),
        });

        // 指示代詞 + 數詞構式 (Demonstrative + Numeral: 這兩、這一、這三、這幾、那些、那兩)
        let demonstratives_num = vec!["這", "那"];
        let numerals_with_xie = vec![
            "一", "二", "兩", "三", "四", "五", "六", "七", "八", "九", "十", "幾", "些", "半",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(demonstratives_num.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(numerals_with_xie.iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "demonstrative_numeral".to_string(),
        });

        // 抑制植物名詞「蔗」偽接數詞/量詞 (防「這兩句」被切碎劫掠為「蔗兩句」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("蔗".to_string()),
            curr_pattern: WordPattern::OneOf(classifiers.iter().chain(numerals_with_xie.iter()).map(|s| s.to_string()).collect()),
            bonus: -45.0,
            name: "anti_zhe_numeral_classifier".to_string(),
        });

        // 副詞「另」+ 動作/狀態動詞 (另開、另起、另外、另有、另行、另闢、另謀)
        let ling_action_verbs = vec!["開", "起", "外", "有", "行", "闢", "謀", "議", "訂", "尋"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("另".to_string()),
            curr_pattern: WordPattern::OneOf(ling_action_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "ling4_action_verb".to_string(),
        });

        // 杜絕「令」誤接「開/起/外/有」 (防「另開」被指令動詞「令」劫掠)
        let ling3_forbidden = vec!["開", "起", "外", "闢", "謀"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("令".to_string()),
            curr_pattern: WordPattern::OneOf(ling3_forbidden.into_iter().map(|s| s.to_string()).collect()),
            bonus: -40.0,
            name: "anti_ling_action".to_string(),
        });

        // 完成時相副詞「已」+ 狀態/動詞 (已安全、已完成、已到達、已結束、已啟用、已停用、已成功、已離線)
        let yi3_verbs = vec!["安全", "完成", "到達", "結束", "啟用", "停用", "成功", "離線", "關閉", "確認", "讀取", "送達"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("已".to_string()),
            curr_pattern: WordPattern::OneOf(yi3_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "yi3_aspect_verb".to_string(),
        });

        // 介詞「以」杜絕偽接狀態動詞「安全/安全卸除」 (防「已安全卸除」被介詞「以」劫掠)
        let yi3_preposition_forbidden = vec!["安全卸除", "安全移除", "安全送達"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("以".to_string()),
            curr_pattern: WordPattern::OneOf(yi3_preposition_forbidden.into_iter().map(|s| s.to_string()).collect()),
            bonus: -40.0,
            name: "anti_yi3_prep_verb".to_string(),
        });

        // 動詞+了 ➔ 一招 (學了一招、練了一招、教了一招、耍了一招、留了一招、走了一招)
        let verb_le_actions = vec!["學了", "練了", "教了", "耍了", "留了", "走了", "試了"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(verb_le_actions.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("一招".to_string()),
            bonus: 35.0,
            name: "verb_le_yi_zhao".to_string(),
        });

        // 杜絕動詞+了接「一朝」 (防「學了一朝」劫掠動量詞)
        let verb_le_actions_anti = vec!["學了", "練了", "教了", "耍了", "留了", "試了"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(verb_le_actions_anti.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("一朝".to_string()),
            bonus: -40.0,
            name: "anti_verb_le_yi_chao".to_string(),
        });

        // 杜絕「以 + 極」偽搭配 (防連詞「以及」被切碎為介詞「以」加形容詞「極」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("以".to_string()),
            curr_pattern: WordPattern::Exact("極".to_string()),
            bonus: -45.0,
            name: "anti_yi_ji2_shatter".to_string(),
        });

        // 抑制名詞/量詞「冊」偽接動作動詞 (防「測試看看」被截斷為「冊試看看」)
        let ce_forbidden_verbs = vec!["試看看", "測試", "看看", "看", "聽", "做", "試"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("冊".to_string()),
            curr_pattern: WordPattern::OneOf(ce_forbidden_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: -45.0,
            name: "anti_ce_action_verb".to_string(),
        });

        // 抑制名詞後綴「性」接句末語氣詞「呀」 (防「非破壞性壓縮」被切碎為「呀縮」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("性".to_string()),
            curr_pattern: WordPattern::Exact("呀".to_string()),
            bonus: -40.0,
            name: "anti_xing_ya".to_string(),
        });

        // 杜絕語氣詞「呀」偽接動詞「縮」 (防「呀縮」荒謬劫掠)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("呀".to_string()),
            curr_pattern: WordPattern::Exact("縮".to_string()),
            bonus: -45.0,
            name: "anti_ya_suo".to_string(),
        });

        // 副詞「再」接動作動詞 (再來、再做、再看、再試、再說、再去、再開)
        let zai4_verbs = vec!["來", "做", "看", "試", "說", "去", "開", "玩", "買", "寫", "聽", "等等"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("再".to_string()),
            curr_pattern: WordPattern::OneOf(zai4_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "zai4_action_verb".to_string(),
        });

        // 杜絕介詞「在」接「來/來做/來看」劫掠副詞「再」 (防「在來」劫掠「再來」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("在".to_string()),
            curr_pattern: WordPattern::Exact("來".to_string()),
            bonus: -25.0,
            name: "anti_zai4_lai2".to_string(),
        });

        // 抑制疑問助詞「嗎」在句中接「上/尚」劫掠時間名詞「馬上」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("嗎".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["上".to_string(), "尚".to_string()]),
            bonus: -50.0,
            name: "anti_ma_shang".to_string(),
        });

        // 杜絕「如 + 和」偽搭配 (防疑問代詞「如何」被切碎為「如和」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("如".to_string()),
            curr_pattern: WordPattern::Exact("和".to_string()),
            bonus: -45.0,
            name: "anti_ru_he_shatter".to_string(),
        });

        // 杜絕形容詞「大」偽接動詞「壓縮」劫掠「壓縮袋」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("大".to_string()),
            curr_pattern: WordPattern::Exact("壓縮".to_string()),
            bonus: -35.0,
            name: "anti_da_yasuo".to_string(),
        });

        // 介詞「在」+ 動作/疊詞成語 (在來來去去、在走走停停、在進進出出)
        let zai4_idioms = vec!["來來去去", "走走停停", "進進出出", "兜兜轉轉", "尋尋覓覓"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("在".to_string()),
            curr_pattern: WordPattern::OneOf(zai4_idioms.into_iter().map(|s| s.to_string()).collect()),
            bonus: 35.0,
            name: "zai4_aabb_idiom".to_string(),
        });

        // 杜絕「人 + 惠」偽搭配 (防「人會」被歷史詞「仁惠」搭橋劫掠)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("人".to_string()),
            curr_pattern: WordPattern::Exact("惠".to_string()),
            bonus: -45.0,
            name: "anti_ren_hui_shatter".to_string(),
        });

        // ── 存現動詞 ➔ 不定量標記「些」(還有些、有些、剩下些、帶有些、留有些) ──
        let existential_verbs = vec!["有", "還有", "剩下", "帶有", "存有", "保留", "具備"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(existential_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("些".to_string()),
            bonus: 26.0,
            name: "existential_verb_xie".to_string(),
        });

        // ── 限定詞/不定量詞「些」➔ 實詞/名詞 (些功能、些問題、些微、些許、些事情、些細節、些建議、些地方) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("些".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 26.0,
            name: "specifier_xie_content_word".to_string(),
        });

        // ── 副詞 ➔ 雙字詞「有些」(還有些、另有些、但有些、卻有些) ──
        let adv_before_youxie = vec!["還", "另", "且", "並", "但", "卻", "也"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(adv_before_youxie.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("有些".to_string()),
            bonus: 24.0,
            name: "adv_before_youxie".to_string(),
        });

        // ── 雙字詞「有些」➔ 複合實詞 (有些功能、有些問題、有些細節、有些建議、有些困難) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("有些".to_string()),
            curr_pattern: WordPattern::CompoundContentWord,
            bonus: 25.0,
            name: "youxie_compound_content".to_string(),
        });

        // ── 抑制罕見孤立詞「歇工」（非現代台灣常用詞，嚴防「些+工.../些+功能」被切碎劫掠）──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("歇工".to_string()),
            bonus: -32.0,
            name: "anti_isolated_xiegong_impostor".to_string(),
        });

        // ── 動作動詞 ➔ 完成體補語「完畢」(處理完畢、準備完畢、確認完畢、測試完畢、安裝完畢、下載完畢、執行完畢、審查完畢、校對完畢) ──
        let wanbi_verbs = vec![
            "處理", "準備", "確認", "測試", "點交", "審查", "檢查", "安裝", "下載", "執行",
            "盤點", "清點", "核對", "校對", "設定", "填寫", "繳納", "回覆", "更新", "修復",
            "搬遷", "整修", "交接", "點收", "製作", "編譯", "建置", "打包", "排版", "列印",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(wanbi_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("完畢".to_string()),
            bonus: 30.0,
            name: "action_verb_wanbi_complement".to_string(),
        });

        // ── 完成/結束體標記 ➔ 先後關聯副詞「才」(完畢才、完成才、結束才、處理完才、做完才、準備好才、考完才、到了才) ──
        let completion_aspect_markers = vec![
            "完畢", "完成", "結束", "做完", "弄完", "處理完", "處理完畢", "準備好", "做好",
            "考完", "看完", "吃完", "到了", "學完", "放學", "畢業", "之後", "過後", "結算",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(completion_aspect_markers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("才".to_string()),
            bonus: 28.0,
            name: "aspect_completion_cai_sequential".to_string(),
        });

        // ── 先後副詞「才」➔ 狀語/動態「正式」(才正式發布、才正式上線、才正式開始、才正式啟用、才正式營業) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("才".to_string()),
            curr_pattern: WordPattern::Exact("正式".to_string()),
            bonus: 26.0,
            name: "cai_zhengshi_adv".to_string(),
        });

        // ── 先後副詞「才」➔ 情態或動作動詞 (才能、才會、才算、才可以、才得以、才開始、才好、才行、才發布、才做) ──
        let cai_action_modal = vec![
            "能", "會", "可以", "算", "算作", "得以", "開始", "著手", "進行", "啟用", "發布", "上線",
            "知道", "明白", "發現", "懂得", "有", "好", "行", "敢", "准", "成",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("才".to_string()),
            curr_pattern: WordPattern::OneOf(cai_action_modal.into_iter().map(|s| s.to_string()).collect()),
            bonus: 25.0,
            name: "cai_modal_or_action".to_string(),
        });

        // ── 狀語「正式」➔ 動作動詞 (正式發布、正式上線、正式開始、正式啟用、正式營運、正式推出、正式營業、正式發表、正式開工、正式實施、正式上架、正式宣告) ──
        let zhengshi_actions = vec![
            "發布", "上線", "開始", "啟用", "運作", "推出", "營業", "營運", "發表", "開工",
            "實施", "上架", "發售", "公布", "宣告", "簽約", "掛牌", "亮相", "啟動", "交機",
            "進駐", "招生", "開學", "成立", "動工", "移交", "授權", "出貨", "交車", "定案",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("正式".to_string()),
            curr_pattern: WordPattern::OneOf(zhengshi_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "zhengshi_action_verb".to_string(),
        });

        // ── 狀語「正式」➔ 複合實詞動詞 (通用泛化: 正式 + 2~4字實詞動詞) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("正式".to_string()),
            curr_pattern: WordPattern::CompoundContentWord,
            bonus: 22.0,
            name: "zhengshi_compound_content_action".to_string(),
        });

        // ── 指示/數量詞 ➔ 定語修飾詞 (這個新的、那個舊的、一個好的、這些新的、那些舊的) ──
        let demonstrative_specifiers = vec!["這個", "那個", "一個", "每個", "這些", "那些", "某個", "另一個", "整個", "全部"];
        let common_attributives = vec!["新的", "舊的", "好的", "壞的", "大的", "小的", "真的", "假的", "其他的", "不同的", "重要的"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(demonstrative_specifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(common_attributives.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "demonstrative_attributive_de".to_string(),
        });

        // ── 指示代詞「這個/那個」➔ 複合實詞 (這個資料夾、這個功能、這個專案、這個系統、這個問題、這個地方) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["這個".to_string(), "那個".to_string()]),
            curr_pattern: WordPattern::CompoundContentWord,
            bonus: 22.0,
            name: "zhege_compound_content".to_string(),
        });

        // ── 定語標記結尾詞「~的」➔ 複合實詞/名詞 (新的資料夾、舊的檔案、好的方法、大的問題、重要的事情) ──
        let de_ending_attributives = vec![
            "新的", "舊的", "好的", "壞的", "大的", "小的", "真的", "假的", "高的", "低的",
            "多的", "少的", "快的", "慢的", "長的", "短的", "遠的", "近的", "美的", "醜的",
            "難的", "易的", "貴的", "便宜的", "重要的", "特定的", "唯一的", "所有的", "一般的",
            "各種的", "不同的", "基本的", "關鍵的", "專門的", "額外的", "內部的", "外部的",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(de_ending_attributives.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::CompoundContentWord,
            bonus: 26.0,
            name: "de_ending_compound_noun".to_string(),
        });

        // 量詞 + 實詞（隻貓、個人、條路、部作品、座城市）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(classifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 16.0,
            name: "classifier_noun".to_string(),
        });

        // ── 數量詞/量詞 + 程度副詞修飾結構 (一句很、一個很、這件非常、那隻太) ──
        let degree_advs = vec!["很", "非常", "特別", "十分", "太", "真", "挺", "極為", "超"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(classifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(degree_advs.iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "classifier_degree_adv".to_string(),
        });

        // ── 程度副詞 + 實詞謂詞/形容詞 (很普通、很好、很大、很小、很美麗) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(degree_advs.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 24.0,
            name: "degree_adv_content_adj".to_string(),
        });

        // 否定: 不/沒 + 實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["不".to_string(), "沒".to_string(), "沒有".to_string()]),
            curr_pattern: WordPattern::ContentWord,
            bonus: 8.0,
            name: "negation".to_string(),
        });

        // 判斷: 是 + 實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("是".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 8.0,
            name: "shi_predicate".to_string(),
        });

        // 疑問/方式副詞 ➔ 動詞/實詞: [怎麼, 怎樣, 如何, 怎, 何以, 為什麼, 為甚麼] ➔ 動詞/實詞 (怎麼做、怎麼辦、怎麼設計、怎麼看、怎麼想、如何處理)
        let interrogative_advs = vec![
            "怎麼", "怎樣", "如何", "怎", "何以", "為什麼", "為甚麼"
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(interrogative_advs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 20.0,
            name: "interrogative_adv_action".to_string(),
        });

        // 指示代名詞短語 ➔ 實詞/名詞: 這樣的 / 那樣的 ➔ [實詞] (這樣的架構、這樣的設計、這樣的系統、這樣的生活、這樣的事情)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["這樣的".to_string(), "那樣的".to_string(), "這般".to_string(), "那般".to_string()]),
            curr_pattern: WordPattern::ContentWord,
            bonus: 24.0,
            name: "zheyangde_nominal".to_string(),
        });

        // 體標記後接名詞/代詞: {了/過/完} + 賓語（吃了飯、看過他、找到了我）
        let aspect_short = vec!["了", "過", "完", "好", "掉"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(aspect_short.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Nominal,
            bonus: 8.0,
            name: "aspect_object".to_string(),
        });

        // 動詞 + 人稱代詞賓語 (看他、找他、聽他、等他、看我、找我、看她)
        let v_transitive = vec![
            "看", "找", "等", "聽", "愛", "想", "問", "叫", "請", "幫", "陪",
            "到", "等到", "看到", "找到", "遇到", "輪到", "碰到", "交給", "送給",
        ];
        let pronouns = vec![
            "我", "你", "妳", "他", "她", "它", "牠", "祂",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(v_transitive.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(pronouns.iter().map(|s| s.to_string()).collect()),
            bonus: 14.0,
            name: "verb_pronoun_object".to_string(),
        });

        // ── 系統化漢語語法構式 (Systematic Chinese Grammar Constructions) ──

        // ── 優先阻斷規則 (Anti-Rules: 必須置於通配規則之前，確保優先攔截) ──

        // 抑制偽單字切分「時後」、「實後」、「什後」（嚴防破壞固有名詞「時候」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["時".to_string(), "實".to_string(), "什".to_string()]),
            curr_pattern: WordPattern::Exact("後".to_string()),
            bonus: -60.0,
            name: "anti_shi_hou4".to_string(),
        });

        // 抑制副詞/繫詞「真是」被切碎為「真事」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("真".to_string()),
            curr_pattern: WordPattern::Exact("事".to_string()),
            bonus: -45.0,
            name: "anti_zhen_shi_shatter".to_string(),
        });

        // 抑制偽構式「很多時」、「多時後」（嚴防破壞「很多時候」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("很".to_string()),
            curr_pattern: WordPattern::Exact("多時".to_string()),
            bonus: -40.0,
            name: "anti_hen_duoshi_shatter".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("多時".to_string()),
            curr_pattern: WordPattern::Exact("後".to_string()),
            bonus: -40.0,
            name: "anti_duoshi_hou_shatter".to_string(),
        });

        // 抑制破音字「大(ㄊㄞˋ)」碎拆接「太」（嚴防「太太」被拆成「大太」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("大".to_string()),
            curr_pattern: WordPattern::Exact("太".to_string()),
            bonus: -45.0,
            name: "anti_da_tai_shatter".to_string(),
        });

        // 抑制連詞「和」碎拆接動詞/介詞「在」或生僻疊字「在在」（嚴防「何在」被拆成「和在」或「和在在」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("和".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["在".to_string(), "在在".to_string()]),
            bonus: -45.0,
            name: "anti_he_zai_shatter".to_string(),
        });

        // 抑制中古宗教罕用詞「祆道」劫掠現代常用動副短語「先到」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("祆道".to_string()),
            bonus: -50.0,
            name: "anti_archaic_xiandao".to_string(),
        });

        // 抑制荒謬偏正「如 + 何在」（嚴防「如何再...」被腰斬碎拆為「如」+「何在」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("如".to_string()),
            curr_pattern: WordPattern::Exact("何在".to_string()),
            bonus: -50.0,
            name: "anti_ru_hezai_shatter".to_string(),
        });

        // 抑制成語生僻詞「世態」接發展等動態動詞（世態僅用於「世態炎涼」，嚴防劫掠「事態發展」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("世態".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "發展".to_string(), "如何".to_string(), "嚴重".to_string(), "擴大".to_string(),
                "演變".to_string(), "走向".to_string(), "變化".to_string(), "失控".to_string(),
            ]),
            bonus: -45.0,
            name: "anti_shitai_development".to_string(),
        });

        // 抑制副詞「再」接名詞「座」（嚴防「再做」被破壞為「再座/在座」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("再".to_string()),
            curr_pattern: WordPattern::Exact("座".to_string()),
            bonus: -45.0,
            name: "anti_zai_zuo_seat".to_string(),
        });

        // 抑制客套詞「承讓」跨詞隙劫掠「過程讓」（杜絕「這一整段過程讓我」被拆為「過 + 承讓 + 我」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("過".to_string()),
            curr_pattern: WordPattern::Exact("承讓".to_string()),
            bonus: -50.0,
            name: "anti_guo_chengrang_shatter".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("承讓".to_string()),
            curr_pattern: WordPattern::Exact("我".to_string()),
            bonus: -45.0,
            name: "anti_chengrang_wo".to_string(),
        });

        // 抑制偽單字切分「處裡」（嚴防破壞「處理」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("處".to_string()),
            curr_pattern: WordPattern::Exact("裡".to_string()),
            bonus: -35.0,
            name: "anti_chu_li_shatter".to_string(),
        });

        // 抑制偽單字切分「覺的」（嚴防破壞「覺得」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("覺".to_string()),
            curr_pattern: WordPattern::Exact("的".to_string()),
            bonus: -25.0,
            name: "anti_jue_de_shatter".to_string(),
        });

        // 抑制偽單字切分「敢謝」（嚴防破壞敬語動詞「感謝」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("敢".to_string()),
            curr_pattern: WordPattern::Exact("謝".to_string()),
            bonus: -35.0,
            name: "anti_gan_xie_shatter".to_string(),
        });

        // 抑制重複口吃「感謝」接同音「謝/瀉/卸/屑/蟹」（嚴防破壞「敢+謝謝」構式）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("感謝".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["謝".to_string(), "瀉".to_string(), "卸".to_string(), "屑".to_string(), "蟹".to_string()]),
            bonus: -40.0,
            name: "anti_ganxie_xie_redundancy".to_string(),
        });

        // 抑制偽詞切分「本來藥」（嚴防破壞「本來要」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("本來".to_string()),
            curr_pattern: WordPattern::Exact("藥".to_string()),
            bonus: -35.0,
            name: "anti_benlai_yao_shatter".to_string(),
        });

        // 抑制偽詞切分「過城中」（嚴防破壞「過程中」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("過".to_string()),
            curr_pattern: WordPattern::Exact("城中".to_string()),
            bonus: -45.0,
            name: "anti_guo_chengzhong_shatter".to_string(),
        });

        // 抑制哲學長尾名詞「我執」偽接單字「接」（杜絕「我執接」碎拆劫掠「我直接」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("我執".to_string()),
            curr_pattern: WordPattern::Exact("接".to_string()),
            bonus: -50.0,
            name: "anti_wozhi_jie_shatter".to_string(),
        });

        // 抑制名詞「案」偽接動量詞（案件不具動作動量屬性，杜絕「案兩下」劫掠「按兩下」）
        let verbal_measures = vec![
            "一下", "兩下", "三下", "幾下", "數下", "一眼", "一次", "一趟", "一回", "一陣", "一番", "一頓", "兩次", "三次",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("案".to_string()),
            curr_pattern: WordPattern::OneOf(verbal_measures.iter().map(|s| s.to_string()).collect()),
            bonus: -45.0,
            name: "anti_an_noun_verbal_measure".to_string(),
        });

        // 抑制採集動詞「採」偽接「坑/雷/地雷」（杜絕「採了兩個坑」劫掠「踩了兩個坑」）
        let cai_targets = vec![
            "坑", "雷", "地雷", "空", "泥巴", "碎", "到底", "剎車", "煞車", "油門", "線", "水",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("採".to_string()),
            curr_pattern: WordPattern::OneOf(cai_targets.iter().map(|s| s.to_string()).collect()),
            bonus: -45.0,
            name: "anti_cai_pick_pit".to_string(),
        });

        // 抑制地質罕用詞「溶蝕」碎拆劫掠「內容時」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("內".to_string()),
            curr_pattern: WordPattern::Exact("溶蝕".to_string()),
            bonus: -50.0,
            name: "anti_nei_rongshi_shatter".to_string(),
        });

        // 抑制偽詞「輸入作」（杜絕破壞「輸入座」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("輸入".to_string()),
            curr_pattern: WordPattern::Exact("作".to_string()),
            bonus: -45.0,
            name: "anti_shuru_zuo_typo".to_string(),
        });

        // 抑制量詞短語「一棟」修飾「輸入座」（杜絕劫掠「移動輸入座」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("一棟".to_string()),
            curr_pattern: WordPattern::Exact("輸入座".to_string()),
            bonus: -50.0,
            name: "anti_yidong_shuruzuo".to_string(),
        });

        // 抑制「輸入座」碎拆接單字「文」（杜絕將「輸入作文」破壞拆碎為「輸入座」+「文」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("輸入座".to_string()),
            curr_pattern: WordPattern::Exact("文".to_string()),
            bonus: -50.0,
            name: "anti_shuruzuo_wen_shatter".to_string(),
        });

        // 抑制單字「文」修飾社會組織名詞「自救會」（杜絕將「文字就會」腰斬拆碎為「文」+「自救會」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("文".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["自救會".to_string(), "自救".to_string()]),
            bonus: -50.0,
            name: "anti_wen_zijiu_shatter".to_string(),
        });

        // 抑制形容詞「需要的」接單字「文」（杜絕破壞「需要的文字」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("需要的".to_string()),
            curr_pattern: WordPattern::Exact("文".to_string()),
            bonus: -35.0,
            name: "anti_xuyaode_wen_shatter".to_string(),
        });



        // 過程/階段等時相名詞 ➔ 方位詞「中」(過程中、階段中、期間中、發展中)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "過程".to_string(), "階段".to_string(), "期間".to_string(), "發展".to_string(),
                "調整".to_string(), "進行".to_string(), "探索".to_string(), "研發".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("中".to_string()),
            bonus: 28.0,
            name: "process_locative_zhong".to_string(),
        });

        // 位移動作動詞 ➔ 動量詞「步」(進/退/走/跑/跨/邁/移/讓 ➔ 一步/兩步/三步/幾步/步)
        let movement_verbs = vec![
            "進", "退", "走", "跑", "跨", "邁", "移", "挪", "讓", "停",
        ];
        let step_measures = vec![
            "一步", "兩步", "三步", "幾步", "百步", "一大步", "一小步", "步",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(movement_verbs.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(step_measures.iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "movement_verb_step".to_string(),
        });

        // 抑制位移動詞接行政單位「部」(退兩部、進三部)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(movement_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(vec!["兩部".to_string(), "三部".to_string(), "一部".to_string(), "部".to_string()]),
            bonus: -35.0,
            name: "anti_movement_verb_bu_department".to_string(),
        });

        // 動作動詞 ➔ 動量詞構式 (Verbal Measure: 按兩下、按一下、點兩下、看一眼、試一次、打一頓、敲一下)
        let action_verbs_with_measure = vec![
            "按", "點", "敲", "打", "看", "查", "碰", "摸", "試", "拉", "推", "動",
            "拍", "捏", "踢", "踩", "跳", "搖", "晃", "轉", "甩", "咬", "吻", "親", "抱",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(action_verbs_with_measure.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(verbal_measures.iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "verb_verbal_measure".to_string(),
        });

        // 人稱代詞 ➔ 方式副詞「直接」(我直接、你直接、他直接、我們直接、你們直接、他們直接、大家直接)
        let pronouns_zhijie = vec![
            "我", "你", "妳", "他", "她", "它", "我們", "你們", "他們", "大家", "誰",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pronouns_zhijie.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("直接".to_string()),
            bonus: 32.0,
            name: "pronoun_zhijie".to_string(),
        });

        // 方式副詞「直接」➔ 動作動詞 (直接按、直接點、直接做、直接看、直接去、直接買、直接選、直接寫、直接說、直接進、直接走)
        let zhijie_actions = vec![
            "按", "點", "做", "看", "去", "買", "選", "寫", "說", "進", "走", "用",
            "拿", "聽", "問", "找", "衝", "飛", "殺", "跳", "到", "開", "關", "換", "投",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("直接".to_string()),
            curr_pattern: WordPattern::OneOf(zhijie_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "zhijie_action".to_string(),
        });

        // 動作動詞「踩」➔ 賓語構式 (踩坑、踩雷、踩地雷、踩空、踩泥巴、踩剎車、踩煞車、踩油門、踩碎)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("踩".to_string()),
            curr_pattern: WordPattern::OneOf(cai_targets.iter().map(|s| s.to_string()).collect()),
            bonus: 35.0,
            name: "cai_object".to_string(),
        });

        // 動詞「踩」➔ 完成體「了」(踩了)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("踩".to_string()),
            curr_pattern: WordPattern::Exact("了".to_string()),
            bonus: 26.0,
            name: "cai_aspect_le".to_string(),
        });

        // 名詞前綴「輸入」➔ 後綴「座」(輸入座)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("輸入".to_string()),
            curr_pattern: WordPattern::Exact("座".to_string()),
            bonus: 35.0,
            name: "shuru_zuo".to_string(),
        });

        // 狀態/動作 ➔ 時間後置副詞「時」(內容時、有內容時、移動時、操作時、點擊時)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "內容".to_string(), "有內容".to_string(), "移動".to_string(), "操作".to_string(),
                "點擊".to_string(), "工作".to_string(), "執行".to_string(), "使用".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("時".to_string()),
            bonus: 32.0,
            name: "condition_time_shi".to_string(),
        });

        // 動作動詞「移動」➔ 受事目標 (移動輸入座、移動滑鼠、移動視窗、移動游標)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("移動".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "輸入座".to_string(), "滑鼠".to_string(), "視窗".to_string(), "游標".to_string(), "位置".to_string(),
            ]),
            bonus: 35.0,
            name: "yidong_target".to_string(),
        });

        // 動賓搭配: 輸入 ➔ 作文
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("輸入".to_string()),
            curr_pattern: WordPattern::Exact("作文".to_string()),
            bonus: 30.0,
            name: "shuru_zuowen".to_string(),
        });

        // 定中修飾構式: [需要, 需要的, 輸入, 輸入的, 顯示, 顯示的] ➔ 文字
        let wenzi_modifiers = vec![
            "需要", "需要的", "輸入", "輸入的", "顯示", "顯示的", "這些", "那些", "所有", "相關",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(wenzi_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("文字".to_string()),
            bonus: 32.0,
            name: "modifier_wenzi".to_string(),
        });

        // 主謂/助動結構: 文字 ➔ 就會
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("文字".to_string()),
            curr_pattern: WordPattern::Exact("就會".to_string()),
            bonus: 32.0,
            name: "wenzi_jiuhui".to_string(),
        });

        // 助動詞 ➔ 狀態評價謂詞: 就會 ➔ [很小, 很大, 很難, 變小, 變大, 很慢, 很快, 很難閱讀]
        let jiuhui_states = vec![
            "很小", "很大", "很難", "變小", "變大", "很慢", "很快", "變慢", "超小",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("就會".to_string()),
            curr_pattern: WordPattern::OneOf(jiuhui_states.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "jiuhui_state_predicate".to_string(),
        });

        // 領域專用搭配: 中英文 ➔ 混打 ➔ 測試
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("中英文".to_string()),
            curr_pattern: WordPattern::Exact("混打".to_string()),
            bonus: 35.0,
            name: "zhongyingwen_hunda".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("混打".to_string()),
            curr_pattern: WordPattern::Exact("測試".to_string()),
            bonus: 35.0,
            name: "hunda_ceshi".to_string(),
        });



        // 時相階段構式: 專案/動作 ➔ 初期 (開發/研發/創業/建置/營運/治療/探索/規劃/籌備 ➔ 初期)
        let phase_activities = vec![
            "開發", "研發", "創業", "建置", "營運", "治療", "探索", "規劃", "籌備", "設計", "研究",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(phase_activities.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("初期".to_string()),
            bonus: 32.0,
            name: "phase_initial_stage".to_string(),
        });

        // 抑制動補結構接孤立代詞「其」(開發出其、想出其、研發出其)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "開發出".to_string(), "想出".to_string(), "設計出".to_string(), "研發出".to_string(), "做出".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("其".to_string()),
            bonus: -40.0,
            name: "anti_verb_result_isolated_qi".to_string(),
        });

        // 抑制時相名詞接代詞（初期他、中期你、後期我，嚴防「開發初期」破壞「開發出其他」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "初期".to_string(), "中期".to_string(), "後期".to_string(), "末期".to_string(), "晚期".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "他".to_string(), "你".to_string(), "我".to_string(), "她".to_string(), "它".to_string(), "牠".to_string(), "誰".to_string(),
            ]),
            bonus: -45.0,
            name: "anti_phase_pronoun".to_string(),
        });

        // 動補動詞 ➔ 其他 (開發出/想出/做出/列出/找出/研發出 ➔ 其他)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "開發出".to_string(), "想出".to_string(), "設計出".to_string(), "研發出".to_string(), "做出".to_string(), "找出".to_string(), "列出".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("其他".to_string()),
            bonus: 30.0,
            name: "verb_result_qita".to_string(),
        });

        // 代詞「其他」➔ 修飾名詞 (其他 ➔ 廠商/人/商品/朋友/公司/方案/團隊/平台)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("其他".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "廠商".to_string(), "商品".to_string(), "朋友".to_string(), "公司".to_string(), "方案".to_string(),
                "團隊".to_string(), "平台".to_string(), "工具".to_string(), "方法".to_string(), "產品".to_string(),
            ]),
            bonus: 28.0,
            name: "qita_noun_modifier".to_string(),
        });

        // 繪畫書寫動量結構: 畫/塗/寫/勾/描 ➔ 一筆/兩筆/三筆/幾筆/筆
        let stroke_verbs = vec!["畫", "塗", "寫", "勾", "描", "添"];
        let stroke_measures = vec!["一筆", "兩筆", "三筆", "幾筆", "筆", "筆畫"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(stroke_verbs.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(stroke_measures.iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "draw_stroke_action".to_string(),
        });

        // 抑制名詞接筆動量詞 (話三筆、圖兩筆)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["話".to_string(), "圖".to_string()]),
            curr_pattern: WordPattern::OneOf(stroke_measures.into_iter().map(|s| s.to_string()).collect()),
            bonus: -40.0,
            name: "anti_noun_stroke_measure".to_string(),
        });

        // 身體姿態動詞 ➔ 時量詞: 躺/睡/歇/坐/站 ➔ 一天/兩天/三天/幾天/半天/一下/一會
        let body_posture_verbs = vec!["躺", "睡", "歇", "坐", "站", "蹲", "跪"];
        let duration_measures = vec!["一天", "兩天", "三天", "幾天", "半天", "一下", "一會", "一會兒", "一整天"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(body_posture_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(duration_measures.into_iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "body_motion_duration".to_string(),
        });

        // 飲食/進出動作動詞 ➔ 動量詞: 吃/喝/吞/咬/吐/嚐 ➔ 一口/兩口/三口/幾口/口
        let ingestion_verbs = vec!["吃", "喝", "吞", "咬", "吐", "嚐"];
        let mouthful_measures = vec!["一口", "兩口", "三口", "幾口", "口"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(ingestion_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(mouthful_measures.iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "ingestion_action_mouthful".to_string(),
        });

        // 抑制名詞接口動量詞 (兔一口、兔兩口)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("兔".to_string()),
            curr_pattern: WordPattern::OneOf(mouthful_measures.into_iter().map(|s| s.to_string()).collect()),
            bonus: -40.0,
            name: "anti_rabbit_mouthful".to_string(),
        });

        // 對稱成語軍事數詞: 傷敵 ➔ 五百/一千/一萬/八百
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("傷敵".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["五百".to_string(), "一千".to_string(), "一萬".to_string(), "八百".to_string()]),
            bonus: 32.0,
            name: "shangdi_number".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["五百".to_string(), "一千".to_string()]),
            curr_pattern: WordPattern::OneOf(vec!["自毀".to_string(), "自損".to_string(), "自殘".to_string()]),
            bonus: 30.0,
            name: "number_zihui".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["自毀".to_string(), "自損".to_string()]),
            curr_pattern: WordPattern::OneOf(vec!["三千".to_string(), "八百".to_string(), "一千".to_string()]),
            bonus: 30.0,
            name: "zihui_number".to_string(),
        });

        // 代詞 ➔ 心理副詞 (我/他/她/眾人/大家/令人/讓人 ➔ 不禁)
        let bujin_subjects = vec!["我", "他", "她", "眾人", "大家", "令人", "讓人", "誰", "我們", "他們"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(bujin_subjects.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("不禁".to_string()),
            bonus: 32.0,
            name: "pronoun_bujin".to_string(),
        });

        // 心理副詞「不禁」➔ 心理/認知/感嘆反應動詞 (不禁 ➔ 想/懷疑/讚嘆/感慨/流淚/思考/微笑/顫抖/搖頭/這麼/自問/納悶/佩服)
        let bujin_verbs = vec![
            "想", "懷疑", "讚嘆", "感慨", "流淚", "思考", "微笑", "顫抖", "搖頭", "這麼",
            "自問", "納悶", "佩服", "感到", "覺得", "感嘆", "落淚", "失笑", "心生",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不禁".to_string()),
            curr_pattern: WordPattern::OneOf(bujin_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "bujin_mental_action".to_string(),
        });

        // 抑制否定副詞短語接金屬名詞 (我不金)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("我不".to_string()),
            curr_pattern: WordPattern::Exact("金".to_string()),
            bonus: -40.0,
            name: "anti_wobu_jin".to_string(),
        });

        // 抑制心理情態副詞接外觀狀態形容詞疊詞 (不禁光閃閃、不禁亮晶晶)
        let appearance_abb_adjs = vec![
            "光閃閃".to_string(), "亮晶晶".to_string(), "紅通通".to_string(), "綠油油".to_string(),
            "白茫茫".to_string(), "黑漆漆".to_string(), "胖嘟嘟".to_string(), "慢吞吞".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不禁".to_string()),
            curr_pattern: WordPattern::OneOf(appearance_abb_adjs),
            bonus: -45.0,
            name: "anti_bujin_appearance_adj".to_string(),
        });

        // 否定副詞 ➔ 成語形容詞 (不/沒/不再 ➔ 金光閃閃)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["不".to_string(), "沒".to_string(), "不再".to_string()]),
            curr_pattern: WordPattern::Exact("金光閃閃".to_string()),
            bonus: 28.0,
            name: "negation_jinguang_shanshan".to_string(),
        });

        // 的 ➔ 人物舞台出場 (的 ➔ 出場)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("出場".to_string()),
            bonus: 26.0,
            name: "de_chuchang".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("出廠".to_string()),
            bonus: -25.0,
            name: "anti_de_chuchang_factory".to_string(),
        });

        // 不禁 ➔ 生理情緒動作 (不禁 ➔ 掉下/流下/落下/掉/流)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不禁".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["掉下".to_string(), "流下".to_string(), "落下".to_string(), "掉".to_string(), "流".to_string()]),
            bonus: 30.0,
            name: "bujin_tear_action".to_string(),
        });

        // 掉下/流下 ➔ 眼淚/淚水
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["掉下".to_string(), "流下".to_string()]),
            curr_pattern: WordPattern::OneOf(vec!["眼淚".to_string(), "淚水".to_string(), "淚".to_string()]),
            bonus: 32.0,
            name: "diaoxia_yanlei".to_string(),
        });

        // 程度/語氣副詞 ➔ 就 (真的/確實/實在/簡直/果然/難怪 ➔ 就)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "真的".to_string(), "確實".to_string(), "實在".to_string(), "簡直".to_string(),
                "果然".to_string(), "難怪".to_string(), "本來".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("就".to_string()),
            bonus: 30.0,
            name: "adverb_chain_zhen_de_jiu".to_string(),
        });

        // 就 ➔ 自然時間事件動詞 (就 ➔ 天亮/天黑/好了/結束/算)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("就".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "天亮".to_string(), "天黑".to_string(), "好了".to_string(), "結束".to_string(),
                "算".to_string(), "來".to_string(), "走".to_string(), "能".to_string(), "會".to_string(),
            ]),
            bonus: 28.0,
            name: "jiu_temporal_event".to_string(),
        });

        // 抑制形容詞「舊」修飾自然時間狀態動詞 (舊天亮、舊天黑)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("舊".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["天亮".to_string(), "天黑".to_string(), "天".to_string()]),
            bonus: -40.0,
            name: "anti_jiu4_adjective_event".to_string(),
        });

        // 學術研究搭配 (相關/深入/學術/語言學/理論 ➔ 研究)
        let research_modifiers = vec![
            "相關", "深入", "學術", "科學", "語言學", "理論", "歷史", "醫學", "文獻",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(research_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("研究".to_string()),
            bonus: 30.0,
            name: "research_topic_modifier".to_string(),
        });

        // 研究/名詞主題 ➔ 助動詞 (研究 ➔ 可以/能/會/有助於)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("研究".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["可以".to_string(), "能".to_string(), "會".to_string(), "指出".to_string(), "顯示".to_string()]),
            bonus: 24.0,
            name: "research_predicate_modal".to_string(),
        });

        // 語氣時間副詞 ➔ 助動詞 (本來/原本/原來/差點/偏偏/剛好 ➔ 要/想/會/能/打算/應該)
        let modal_adverbs = vec!["本來", "原本", "原來", "差點", "偏偏", "剛好", "幸好", "早就"];
        let target_modals = vec!["要", "想", "會", "能", "打算", "應該", "可以"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(modal_adverbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(target_modals.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "adverb_temporal_modal".to_string(),
        });

        // 助動詞「要」➔ 常用動作動詞 (要 ➔ 說/做/去/走/看/買/吃/聽/寫...)
        let yao_actions = vec!["說", "做", "去", "走", "看", "買", "吃", "聽", "寫", "來", "帶", "問", "辦"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("要".to_string()),
            curr_pattern: WordPattern::OneOf(yao_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 22.0,
            name: "yao_action".to_string(),
        });

        // 程度副詞 ➔ 心理情感/敬語動詞 (非常/很/十分/相當/太/萬分/格外 ➔ 感謝/感激/佩服/期待/滿意/喜歡/高興)
        let psych_verbs = vec![
            "感謝", "感激", "佩服", "期待", "滿意", "喜歡", "高興", "珍惜", "在乎",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "非常".to_string(), "很".to_string(), "十分".to_string(), "相當".to_string(),
                "萬分".to_string(), "格外".to_string(), "深深".to_string(), "太".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(psych_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "degree_adv_psych_verb".to_string(),
        });

        // 程度副詞 ➔ 助動詞 (真的/太/很/非常/都/也/更/就/才 ➔ 會/能/可以/想/要)
        let degree_advs = vec!["真的", "太", "很", "非常", "都", "也", "更", "就", "才", "必", "一定", "總"];
        let aux_modals = vec!["會", "能", "可以", "想", "要", "該", "得", "敢"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(degree_advs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(aux_modals.into_iter().map(|s| s.to_string()).collect()),
            bonus: 22.0,
            name: "adverb_modal".to_string(),
        });

        // 助動詞「敢」➔ 動作/社交動詞 (敢 ➔ 謝謝/說/做/承認/面對...)
        let gan_actions = vec![
            "謝謝", "說", "做", "去", "來", "看", "承認", "面對", "承擔", "嘗試", "挑戰", "接受",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("敢".to_string()),
            curr_pattern: WordPattern::OneOf(gan_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "gan_modal_action".to_string(),
        });

        // 助動詞 ➔ 心理/認知/實意動詞 (會/能/可以 ➔ 覺得/感到/認為/處理)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("會".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["覺得".to_string(), "感到".to_string(), "認為".to_string(), "想".to_string(), "處理".to_string()]),
            bonus: 24.0,
            name: "hui_mental_verb".to_string(),
        });

        // 心理認知動詞 ➔ 形容詞/心理狀態 (覺得/感到/認為 ➔ 輕鬆/舒服/快樂/高興/好...)
        let mental_verbs = vec!["覺得", "感到", "認為", "顯得", "變得"];
        let state_adjectives = vec![
            "輕鬆", "舒服", "快樂", "高興", "難過", "滿意", "習慣", "自在",
            "開心", "興奮", "安心", "放心", "疲倦", "累", "痛快", "踏實", "好",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(mental_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(state_adjectives.into_iter().map(|s| s.to_string()).collect()),
            bonus: 24.0,
            name: "mental_verb_adjective".to_string(),
        });

        // 抑制非詞搭配「越底」、「越抵」（嚴防「月底」被拆分劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["底".to_string(), "抵".to_string()]),
            bonus: -35.0,
            name: "anti_yue_di".to_string(),
        });

        // 抑制偽構式「開設群」（嚴防「開 + 社群」被「開設 + 群」劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("開設".to_string()),
            curr_pattern: WordPattern::Exact("群".to_string()),
            bonus: -35.0,
            name: "anti_kaishe_qun".to_string(),
        });

        // 抑制方式副詞接體態補語 (*亂完)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("亂".to_string()),
            curr_pattern: WordPattern::Exact("完".to_string()),
            bonus: -30.0,
            name: "anti_luan_wan".to_string(),
        });

        // 1. 方位後置詞構式: 實詞(非方位詞與時間名詞「時」「實」本身) + {裡/外/內/中/上/下/前/後/旁/之中/裡面/外面/中間/底下/身上/手裡/心裡/心中}
        let locatives = vec![
            "裡", "外", "內", "中", "上", "下", "前", "後", "旁",
            "之中", "裡面", "外面", "中間", "底下", "身上", "手裡", "心裡", "心中",
        ];
        let non_locative_stems = vec![
            "裡", "外", "內", "中", "上", "下", "前", "後", "旁", "時", "實",
        ].into_iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(non_locative_stems),
            curr_pattern: WordPattern::OneOf(locatives.iter().map(|s| s.to_string()).collect()),
            bonus: 18.0,
            name: "postpositional_locative".to_string(),
        });

        // 結果動詞與賓語搭配: 開好 ➔ [社群, 會, 網頁, 軟體, 專案, 門, 車, 房]
        let kaihao_targets = vec!["社群", "會", "網頁", "軟體", "專案", "門", "車", "房"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("開好".to_string()),
            curr_pattern: WordPattern::OneOf(kaihao_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "kaihao_object".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的時候".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 20.0,
            name: "de_shihou_content".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("時候".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 18.0,
            name: "shihou_content".to_string(),
        });

        // 數量/指示/疑問詞 ➔ 時候 (很多時候、有時候、這時候、那時候、什麼時候、有些時候)
        let shihou_modifiers = vec![
            "很多", "有些", "有", "這", "那", "什麼", "甚麼", "幾時", "幾", "平時", "何時",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(shihou_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("時候".to_string()),
            bonus: 28.0,
            name: "shihou_modifier".to_string(),
        });

        // 時間狀語/定語構式 (Temporal topics/specifiers: 未來、過去、現在、目前、今天、明天、昨天、月底...)
        let temporal_topics = vec![
            "未來", "過去", "現在", "目前", "今天", "明天", "昨天", "最近", "這幾天", "月底", "月初", "年底",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(temporal_topics.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 22.0,
            name: "temporal_topic_content".to_string(),
        });

        // 抑制古籍生僻名詞/形容詞「穢亂」在現代語境中劫掠「會亂」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("穢亂".to_string()),
            bonus: -45.0,
            name: "anti_archaic_huiluan".to_string(),
        });

        // 方式副詞構式: 亂 + 動作動詞 (亂搭、亂玩、亂跑、亂搞、亂開、亂買、亂說、亂寫、亂跳、亂晃)
        let luan_actions = vec!["搭", "玩", "跑", "搞", "開", "買", "說", "寫", "花", "丟", "動", "猜", "跳", "晃", "叫", "衝", "飛"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("亂".to_string()),
            curr_pattern: WordPattern::OneOf(luan_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "manner_luan_action".to_string(),
        });

        // 情態助動詞 ➔ 方式副詞 (會 ➔ 亂)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("會".to_string()),
            curr_pattern: WordPattern::Exact("亂".to_string()),
            bonus: 25.0,
            name: "modal_luan".to_string(),
        });

        // 情態助動詞 ➔ 偏正動詞短語 (會 ➔ 亂跳/亂跑/亂動/亂晃/亂飛/亂叫/亂衝/亂搞)
        let luan_compounds = vec!["亂跳", "亂跑", "亂動", "亂晃", "亂飛", "亂叫", "亂衝", "亂搞"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("會".to_string()),
            curr_pattern: WordPattern::OneOf(luan_compounds.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "modal_luan_compounds".to_string(),
        });

        // 主語名詞 ➔ 情態助動詞 (成果/結果/程式/畫面/視窗/游標/文字/數字/系統 ➔ 會)
        let hui_subjects = vec!["成果", "結果", "程式", "畫面", "視窗", "游標", "文字", "數字", "系統"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(hui_subjects.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("會".to_string()),
            bonus: 26.0,
            name: "subject_modal_hui".to_string(),
        });

        // 程度副詞 + 助動詞/心理動詞: 不太 ➔ 需要 / 想要 / 適合 / 清楚
        let bu_tai_targets = vec!["需要", "想要", "適合", "清楚", "容易", "習慣", "明白", "在乎", "介意"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不太".to_string()),
            curr_pattern: WordPattern::OneOf(bu_tai_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "bu_tai_modal_predicate".to_string(),
        });

        // 程度副詞 ➔ 助動詞 (很/非常/相當/比較 ➔ 需要)
        let degree_advs = vec!["很", "非常", "相當", "比較", "極其", "特別"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(degree_advs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("需要".to_string()),
            bonus: 28.0,
            name: "degree_adv_xu_yao".to_string(),
        });

        // 抑制「不太」碎拆並接生僻/虛詞名詞「虛藥」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不太".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["虛".to_string(), "藥".to_string(), "虛藥".to_string()]),
            bonus: -45.0,
            name: "anti_bu_tai_xu_yao".to_string(),
        });

        // 抑制生僻古雅詞「彫瑑」劫掠現代常用成語/形容詞「刁鑽」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("彫瑑".to_string()),
            bonus: -45.0,
            name: "anti_archaic_diaozhuan".to_string(),
        });

        // 醫療動詞 ➔ 證候名詞: 專治 ➔ 氣虛 / 感冒 / 咳嗽 / 胃痛 / 失眠
        let zhuanzhi_targets = vec!["氣虛", "感冒", "咳嗽", "胃痛", "失眠", "痠痛", "頭痛", "過敏"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("專治".to_string()),
            curr_pattern: WordPattern::OneOf(zhuanzhi_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "medical_zhuanzhi_target".to_string(),
        });

        // 氣虛 ➔ 藥到病除 (成語緊接引力)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("氣虛".to_string()),
            curr_pattern: WordPattern::Exact("藥到病除".to_string()),
            bonus: 32.0,
            name: "qixu_yaodaobingchu".to_string(),
        });

        // 醫療 ➔ 要求
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("醫療".to_string()),
            curr_pattern: WordPattern::Exact("要求".to_string()),
            bonus: 26.0,
            name: "yiliao_yaoqiu".to_string(),
        });

        // 並非 ➔ 夢想 / 難事 / 偶然 / 特例
        let bingfei_targets = vec!["夢想", "難事", "偶然", "特例", "壞事", "巧合", "不可能"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("並非".to_string()),
            curr_pattern: WordPattern::OneOf(bingfei_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "bingfei_nominal_predicate".to_string(),
        });

        // 關聯副詞 + 程度/生理/評價狀態: 越是 ➔ 好困難 / 好容易 / 困難 / 容易 / 好睏 / 緊張 / 累 / 急
        let yueshi_states = vec!["好困難", "好容易", "困難", "容易", "複雜", "簡單", "好睏", "緊張", "累", "急", "想睡", "忙", "睏"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越是".to_string()),
            curr_pattern: WordPattern::OneOf(yueshi_states.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "yueshi_state".to_string(),
        });

        // 程度副詞 ➔ 生理狀態: [好, 很, 太, 真] ➔ 睏 (杜絕「好困」)
        let sleep_advs = vec!["好", "很", "太", "真", "超", "真夠"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(sleep_advs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("睏".to_string()),
            bonus: 28.0,
            name: "degree_adv_sleepy".to_string(),
        });

        // 認知動詞 ➔ 程度/評價短語: 覺得 / 感到 / 認為 ➔ 好困難 / 好容易 / 困難 / 容易
        let cognitive_verbs = vec!["覺得", "感到", "認為", "發覺"];
        let cognitive_targets = vec!["好困難", "好容易", "困難", "容易", "複雜", "簡單", "難辦", "奇怪", "納悶", "好玩", "有趣"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(cognitive_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(cognitive_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "cognitive_evaluation_target".to_string(),
        });

        // 介詞短語: 跟別人 ➔ 不同 / 一樣 / 類似 / 無關
        let gen_bieren_predicates = vec!["不同", "一樣", "類似", "無關", "走得近"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("跟別人".to_string()),
            curr_pattern: WordPattern::OneOf(gen_bieren_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "gen_bieren_predicate".to_string(),
        });

        // 抑制「好睏」碎拆並接單字「難」形成「好睏難」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("好睏".to_string()),
            curr_pattern: WordPattern::Exact("難".to_string()),
            bonus: -45.0,
            name: "anti_haokun_nan_shatter".to_string(),
        });

        // 關聯副詞: 越 ➔ 是 (越是構式)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越".to_string()),
            curr_pattern: WordPattern::Exact("是".to_string()),
            bonus: 28.0,
            name: "yue_shi_copula".to_string(),
        });

        // 抑制「越」接孤立動作/視覺單字「視」形成「越視」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越".to_string()),
            curr_pattern: WordPattern::Exact("視".to_string()),
            bonus: -40.0,
            name: "anti_yue_shi_noun".to_string(),
        });

        // 頻率副詞: 他 / 她 / 我們 / 他們 ➔ 總是
        let zongshi_subjects = vec!["他", "她", "我們", "你們", "他們", "大家", "誰", "人"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(zongshi_subjects.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("總是".to_string()),
            bonus: 28.0,
            name: "subject_zongshi".to_string(),
        });

        // 介詞由 ➔ 四字成語主語 (是由 / 由 ➔ 意氣風發 / 經驗豐富 / 技藝高超)
        let you_idiom_modifiers = vec!["意氣風發", "經驗豐富", "技藝高超", "訓練有素"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["是由".to_string(), "由".to_string()]),
            curr_pattern: WordPattern::OneOf(you_idiom_modifiers.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "you_idiom_modifier".to_string(),
        });

        // 哲學界 / 學術界 / 醫學界 / 文藝界 ➔ 的
        let academic_circles = vec!["哲學界", "學術界", "醫學界", "文藝界", "科學界", "教育界", "商業界"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(academic_circles.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("的".to_string()),
            bonus: 26.0,
            name: "academic_circle_de".to_string(),
        });

        // 的 ➔ 心血結晶 / 勞動成果
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["心血結晶".to_string(), "心血".to_string(), "勞動成果".to_string()]),
            bonus: 28.0,
            name: "de_xinxue_jiejing".to_string(),
        });

        // 介詞在 ➔ 演講中 / 會議中 / 課堂中
        let in_occasions = vec!["演講中", "會議中", "課堂中", "生活中", "工作中", "談判中"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("在".to_string()),
            curr_pattern: WordPattern::OneOf(in_occasions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "zai_occasion_zhong".to_string(),
        });

        // 演講中 ➔ 侃侃而談 / 談笑風生
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("演講中".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["侃侃而談".to_string(), "談笑風生".to_string()]),
            bonus: 32.0,
            name: "occasion_kankan_ertan".to_string(),
        });

        // 抑制生僻字詞「衎衎」、「坎坎」劫掠成語「侃侃而談」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::OneOf(vec!["衎衎".to_string(), "坎坎".to_string()]),
            bonus: -40.0,
            name: "anti_kankan_archaic".to_string(),
        });

        // 抑制水果名詞「榴槤」劫掠成語「流連忘返」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("榴槤".to_string()),
            curr_pattern: WordPattern::Exact("忘返".to_string()),
            bonus: -45.0,
            name: "anti_liulian_wangfan".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["令人".to_string(), "讓人".to_string(), "使人".to_string()]),
            curr_pattern: WordPattern::Exact("流連忘返".to_string()),
            bonus: 32.0,
            name: "causative_liulian_wangfan".to_string(),
        });

        // 使役動詞 (讓人/令人/使人) ➔ 生理狀態/情緒感知 (好睏、想睡、難過、失望、開心、興奮、感動、著迷、佩服、流連忘返)
        let causative_feelings = vec![
            "好睏", "想睡", "疲倦", "難過", "失望", "開心", "興奮", "感動", "著迷", "佩服", "流連忘返",
            "震驚", "擔憂", "困惑", "放心", "安心", "驚訝", "驚喜", "滿意", "痛苦", "憤怒", "欣慰", "焦慮",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["令人".to_string(), "讓人".to_string(), "使人".to_string()]),
            curr_pattern: WordPattern::OneOf(causative_feelings.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "causative_feelings".to_string(),
        });

        // 反詰副詞構式: 難道 ➔ [不是, 沒有, 會, 能, 要, 真, 還, 就, 算, 不成, 真的, 難倒]
        let nandao_follow = vec![
            "不是", "沒有", "會", "能", "要", "真", "還", "就", "算", "不成", "真的", "難倒",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("難道".to_string()),
            curr_pattern: WordPattern::OneOf(nandao_follow.into_iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "rhetorical_nandao".to_string(),
        });

        // 領屬人稱代詞構式: [我, 你, 妳, 他, 她, 它, 牠, 我們, 你們, 他們] ➔ 的
        let possessive_pronouns = vec![
            "我", "你", "妳", "他", "她", "它", "牠", "我們", "你們", "他們", "她們", "它們",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(possessive_pronouns.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("的".to_string()),
            bonus: 26.0,
            name: "pronoun_possessive_de".to_string(),
        });

        // 領屬結構 ➔ 抽象/活動/產出名詞: 他的/我的/你的 ➔ 演講/作品/文章/想法/報告/表現
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["他的".to_string(), "我的".to_string(), "你的".to_string(), "他們的".to_string()]),
            curr_pattern: WordPattern::OneOf(vec!["演講".to_string(), "作品".to_string(), "報告".to_string(), "文章".to_string(), "想法".to_string(), "表現".to_string()]),
            bonus: 28.0,
            name: "possessive_noun".to_string(),
        });

        // 嚴防碎拆跨界劫掠: 好困難 ➔ 倒 / 導 (非合法漢語搭配，嚴防「好睏」+「難道」被跨界黏合)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("好困難".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["倒".to_string(), "導".to_string(), "島".to_string(), "蹈".to_string()]),
            bonus: -45.0,
            name: "anti_haokunnan_dao".to_string(),
        });

        // 真是 ➔ 璀璨奪目 / 美麗 / 精彩
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("真是".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["璀璨奪目".to_string(), "璀璨".to_string(), "美麗".to_string(), "精彩".to_string()]),
            bonus: 30.0,
            name: "zhenshi_cuican".to_string(),
        });

        // 反身代詞構式: 自己 + 動作實詞 / 助動詞 + 自己 (自己去、自己開、自己做、自己維護、想自己、不用自己)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("自己".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 22.0,
            name: "ziji_action".to_string(),
        });
        let pre_ziji = vec!["不用", "想", "會", "能", "要", "去", "讓", "由", "請", "都不用", "也不用"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pre_ziji.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("自己".to_string()),
            bonus: 20.0,
            name: "pre_ziji_attraction".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("自己".to_string()),
            curr_pattern: WordPattern::Exact("去".to_string()),
            bonus: 25.0,
            name: "ziji_qu".to_string(),
        });

        // 連動構式延伸: 去 + 目的動詞 (去繳費、去看、去買、去吃、去辦、去玩)
        let qu_actions = vec!["繳費", "買", "吃", "看", "辦", "拿", "做", "找", "玩", "唱", "維護"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("去".to_string()),
            curr_pattern: WordPattern::OneOf(qu_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 25.0,
            name: "qu_action_purpose".to_string(),
        });

        // 網路社群動態動賓構式: 開 ➔ 社群
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("開".to_string()),
            curr_pattern: WordPattern::Exact("社群".to_string()),
            bonus: 26.0,
            name: "kai_shequn".to_string(),
        });

        // 情態助動詞引力: [會, 能, 可以, 要, 該, 想, 只會, 可能會] ➔ 越來越
        let modal_auxiliaries = vec!["會", "能", "可以", "要", "該", "想", "只會", "可能會"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(modal_auxiliaries.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("越來越".to_string()),
            bonus: 26.0,
            name: "modal_yuelaiyue".to_string(),
        });

        // 漸進構式連鎖: 越 ➔ 來 ➔ 越 ➔ 狀態實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越".to_string()),
            curr_pattern: WordPattern::Exact("來".to_string()),
            bonus: 26.0,
            name: "yue_lai".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("來".to_string()),
            curr_pattern: WordPattern::Exact("越".to_string()),
            bonus: 26.0,
            name: "lai_yue".to_string(),
        });
        let yue_states = vec!["快", "好", "多", "大", "遠", "美", "成熟", "便宜", "貴", "難", "容易", "強", "高"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越來越".to_string()),
            curr_pattern: WordPattern::OneOf(yue_states.into_iter().map(|s| s.to_string()).collect()),
            bonus: 25.0,
            name: "yuelaiyue_state".to_string(),
        });

        // 連詞搭配: 但 ➔ 既然
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("但".to_string()),
            curr_pattern: WordPattern::Exact("既然".to_string()),
            bonus: 25.0,
            name: "dan_jiran".to_string(),
        });

        // 副詞搭配: 就 ➔ 先
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("就".to_string()),
            curr_pattern: WordPattern::Exact("先".to_string()),
            bonus: 25.0,
            name: "jiu_xian".to_string(),
        });

        // 生活交易與支付動賓: 匯 ➔ [停車費, 錢, 款, 房租, 學費, 水電費, 車貸]
        let pay_targets = vec!["停車費", "錢", "款", "房租", "學費", "水電費", "車貸"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("匯".to_string()),
            curr_pattern: WordPattern::OneOf(pay_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "hui_payment".to_string(),
        });

        // 否定副詞 + 心理動詞 (不用理會、不用擔心、不用害怕、不用在意)
        let bu_yong_psych = vec!["理會", "擔心", "害怕", "在意", "緊張", "著急"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不用".to_string()),
            curr_pattern: WordPattern::OneOf(bu_yong_psych.into_iter().map(|s| s.to_string()).collect()),
            bonus: 25.0,
            name: "buyong_psych_verb".to_string(),
        });

        // 2. 關聯副詞構式: 越 A 越 B (越好、越快、越弄、越多、越想、越來越...)
        // 語法約束：程度副詞「越」僅修飾度量等級謂詞（形容詞/心理或延展動詞），絕不可任意匹配名詞或生僻孤立字
        let yue_predicates = vec![
            "來", "好", "快", "多", "大", "高", "遠", "長", "深", "難", "貴", "便宜", "強", "美", "新", "早", "近", "冷", "熱",
            "香", "甜", "少", "厚", "薄", "重", "輕", "黑", "白", "小", "想", "看", "弄", "做", "走", "愛", "怕", "在乎",
            "發", "沉", "熟", "成熟", "清楚", "明白", "開心", "傷心", "生氣", "忙", "急", "糟", "慘", "紅", "綠", "藍", "黃",
            "臭", "酸", "苦", "辣", "鹹", "軟", "硬", "乾", "濕", "痛", "累", "煩", "順", "瘋", "狂", "凶", "猛", "準", "穩",
            "狠", "飽", "餓", "瘦", "胖", "大", "小",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("越".to_string()),
            curr_pattern: WordPattern::OneOf(yue_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: 20.0,
            name: "yue_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("越".to_string()),
            bonus: 16.0,
            name: "content_yue".to_string(),
        });

        // 3. 繫詞構式: 的 + 是 (奇妙的是、重要的是、問題是、關鍵是、顯然的是)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("是".to_string()),
            bonus: 12.0,
            name: "de_shi_copula".to_string(),
        });

        // 4. 量詞 + 時間量詞 (一個鐘頭、半個小時、幾分鐘、兩天、三個月)
        let time_nouns = vec![
            "鐘頭", "小時", "分鐘", "天", "年", "週", "周", "秒", "禮拜", "個月",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(classifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(time_nouns.iter().map(|s| s.to_string()).collect()),
            bonus: 22.0,
            name: "classifier_time_noun".to_string(),
        });

        // 5. 指示詞 + 情態副詞 (那會、這會、怎麼會)
        let demonstratives = vec!["那", "這", "哪", "怎麼"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(demonstratives.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("會".to_string()),
            bonus: 16.0,
            name: "demonstrative_hui".to_string(),
        });

        // 6. [ X 中之 X ] 嵌偶構式之結構轉移: 中 ➔ 之、之 ➔ 實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("中".to_string()),
            curr_pattern: WordPattern::Exact("之".to_string()),
            bonus: 14.0,
            name: "zhong_zhi_interfix".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("之".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 12.0,
            name: "zhi_content".to_string(),
        });

        // 7. [ X 上加 X ] 層累構式之結構轉移: 上 ➔ 加、加 ➔ 實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("上".to_string()),
            curr_pattern: WordPattern::Exact("加".to_string()),
            bonus: 14.0,
            name: "shang_jia_interfix".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("加".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 12.0,
            name: "jia_content".to_string(),
        });

        // 8. [ 人上人 ] 與 [ 人中 X ] 熟語結構轉移: 人 ➔ 上 ➔ 人、人 ➔ 中
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("人".to_string()),
            curr_pattern: WordPattern::Exact("上".to_string()),
            bonus: 14.0,
            name: "ren_shang".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("上".to_string()),
            curr_pattern: WordPattern::Exact("人".to_string()),
            bonus: 14.0,
            name: "shang_ren".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("人".to_string()),
            curr_pattern: WordPattern::Exact("中".to_string()),
            bonus: 14.0,
            name: "ren_zhong".to_string(),
        });

        // 9. 可能補語構式 [ 動詞 + 得/不 + 補語 ] (看得懂/看不懂、做得到/做不到、聽得清/聽不清)
        let potential_complements = vec![
            "懂", "見", "清", "到", "起", "下", "住", "著", "完", "好",
            "開", "來", "去", "過", "飽", "慣", "動", "走", "跑", "通",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("得".to_string()),
            curr_pattern: WordPattern::OneOf(potential_complements.iter().map(|s| s.to_string()).collect()),
            bonus: 16.0,
            name: "de_potential_result".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("不".to_string()),
            bonus: 10.0,
            name: "verb_bu_potential".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不".to_string()),
            curr_pattern: WordPattern::OneOf(potential_complements.iter().map(|s| s.to_string()).collect()),
            bonus: 16.0,
            name: "bu_potential_result".to_string(),
        });

        // 10. [ X 來 X 去 ] 動作與空間往復構式 (走來走去、看來看去、想來想去)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("來".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 12.0,
            name: "lai_action".to_string(),
        });

        // 11. [ 大 X 特 X ] 動量擴散構式 (大書特書、大搞特搞、大買特買)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("大".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 10.0,
            name: "da_content".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("特".to_string()),
            bonus: 12.0,
            name: "content_te".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("特".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 12.0,
            name: "te_content".to_string(),
        });

        // 12. 連動構式 (Serial Verb Constructions - SVC): 位移/方式動詞 + 目標動作動詞
        let motion_v1 = vec![
            "去", "來", "回", "上", "出", "進", "坐", "開", "騎", "拿", "帶",
            "跑去", "開車", "坐車", "騎車", "上街", "出門", "回家", "進城", "上樓", "下樓", "回房",
        ];
        let action_v2 = vec![
            "買菜", "吃飯", "睡覺", "上班", "上課", "辦事", "運動", "散步",
            "喝水", "寫字", "看書", "開會", "出遊", "參觀", "工作", "買東西",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(motion_v1.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(action_v2.iter().map(|s| s.to_string()).collect()),
            bonus: 16.0,
            name: "svc_motion_action".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::OneOf(action_v2.into_iter().map(|s| s.to_string()).collect()),
            bonus: 12.0,
            name: "svc_action_v2".to_string(),
        });

        // 13. 雙賓語構式 (Double Object Constructions - DOC): 給予/授予動詞 + 人稱代詞 + 數量名詞
        let ditransitive_verbs = vec![
            "給", "送", "借", "教", "還", "欠", "付", "交代", "交給", "留給", "告訴",
        ];
        let ditransitive_pronouns = vec![
            "我", "你", "妳", "他", "她", "我們", "你們", "他們", "大家",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(ditransitive_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(ditransitive_pronouns.iter().map(|s| s.to_string()).collect()),
            bonus: 16.0,
            name: "ditransitive_pronoun".to_string(),
        });
        let second_objects = vec![
            "一", "兩", "三", "幾", "半", "一個", "一本", "一份", "一筆", "一件", "一塊",
            "很多", "一些", "很多錢",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(ditransitive_pronouns.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(second_objects.into_iter().map(|s| s.to_string()).collect()),
            bonus: 14.0,
            name: "ditransitive_second_object".to_string(),
        });

        // 14. 方式副詞「共同」➔ 動詞 (共同完成、共同努力、共同承擔、共同創造、共同分享、共同研發、共同建立)
        let gongtong_verbs = vec![
            "完成", "努力", "承擔", "創造", "分享", "研發", "建立", "推動", "面對", "奮鬥", "合作", "維護", "探討",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("共同".to_string()),
            curr_pattern: WordPattern::OneOf(gongtong_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "gongtong_verb".to_string(),
        });

        // 動詞「完成」➔ 的 / 了 / 過 (完成的、完成了)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("完成".to_string()),
            curr_pattern: WordPattern::Exact("的".to_string()),
            bonus: 25.0,
            name: "wancheng_de".to_string(),
        });

        // 嚴防碎拆劫掠: 完 + 盛得
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("完".to_string()),
            curr_pattern: WordPattern::Exact("盛得".to_string()),
            bonus: -45.0,
            name: "anti_wan_shengde".to_string(),
        });

        // ── 象／相／像 語意消歧義規則庫 (Xiang Disambiguation Grammar Rules) ──
        // Rule Set A: 像 (Xiàng - 比喻、相似、摹擬)
        // 1. 抑制在相似/比喻語境中誤選「象」或「相」
        let metaphor_left_context = vec![
            "很".to_string(), "太".to_string(), "超".to_string(),
            "越看越".to_string(), "長得".to_string(), "看起來".to_string(), "畫得".to_string(),
            "活脫脫".to_string(), "就像".to_string(), "好".to_string(), "挺".to_string(),
            "不太".to_string(), "多麼".to_string(), "恰似".to_string(), "宛如".to_string(),
            "彷彿".to_string(), "貌似".to_string(), "不".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(metaphor_left_context.clone()),
            curr_pattern: WordPattern::OneOf(vec!["象".to_string(), "相".to_string()]),
            bonus: -40.0,
            name: "anti_xiang_resemblance_confusion".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(metaphor_left_context),
            curr_pattern: WordPattern::Exact("像".to_string()),
            bonus: 28.0,
            name: "xiang_metaphor_attraction".to_string(),
        });

        // 專屬三向消歧義：真 (真像 vs 真相，嚴防「真象」劫掠)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("真".to_string()),
            curr_pattern: WordPattern::Exact("象".to_string()),
            bonus: -45.0,
            name: "anti_zhen_elephant".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("真".to_string()),
            curr_pattern: WordPattern::Exact("相".to_string()),
            bonus: 28.0,
            name: "zhen_xiang_fact".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("真".to_string()),
            curr_pattern: WordPattern::Exact("像".to_string()),
            bonus: 26.0,
            name: "zhen_xiang_metaphor".to_string(),
        });

        // 2. 像 ➔ 句尾比喻後繼標記/結構標記 (像一樣、像似的、像個、像極了、像話)
        let metaphor_right_context = vec![
            "一樣".to_string(), "似的".to_string(), "一般".to_string(), "般".to_string(),
            "個".to_string(), "極了".to_string(), "話".to_string(), "這樣".to_string(), "那樣".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("像".to_string()),
            curr_pattern: WordPattern::OneOf(metaphor_right_context),
            bonus: 28.0,
            name: "xiang_comparative_tail".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("像".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["大白".to_string(), "顯露".to_string(), "畢露".to_string(), "徵".to_string()]),
            bonus: -40.0,
            name: "anti_xiang_fact_tail".to_string(),
        });

        // Rule Set B: 相 (Xiàng/Xiāng - 結構特徵、事實真相、科學相態、交互動詞)
        // 1. 抑制實體本質/容貌修飾接「像」或「象」
        let fact_left_context = vec![
            "實".to_string(), "面".to_string(), "骨".to_string(),
            "法".to_string(), "皮".to_string(), "賣".to_string(), "看".to_string(), "照".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(fact_left_context.clone()),
            curr_pattern: WordPattern::OneOf(vec!["像".to_string(), "象".to_string()]),
            bonus: -40.0,
            name: "anti_fact_feature_confusion".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(fact_left_context),
            curr_pattern: WordPattern::Exact("相".to_string()),
            bonus: 28.0,
            name: "xiang_fact_feature_attraction".to_string(),
        });

        // 2. 相 ➔ 表徵/媒介 (大白、顯露、畢露、片、機、貌、學、術)
        let fact_right_context = vec![
            "大白".to_string(), "顯露".to_string(), "畢露".to_string(), "不露".to_string(),
            "片".to_string(), "機".to_string(), "貌".to_string(), "學".to_string(), "術".to_string(), "鏡".to_string(), "簿".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("相".to_string()),
            curr_pattern: WordPattern::OneOf(fact_right_context),
            bonus: 22.0,
            name: "xiang_fact_feature_tail".to_string(),
        });

        // 真相 ➔ 大白 / 顯露 / 畢露
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("真相".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["大白".to_string(), "顯露".to_string(), "畢露".to_string()]),
            bonus: 32.0,
            name: "zhenxiang_dabai".to_string(),
        });

        // 3. 科學相態前綴 ➔ 相 (氣、液、固、單、多、三、兩、晶 ➔ 相)
        let science_phase_prefixes = vec![
            "氣".to_string(), "液".to_string(), "固".to_string(), "單".to_string(),
            "多".to_string(), "三".to_string(), "兩".to_string(), "晶".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["液".to_string(), "固".to_string(), "晶".to_string()]),
            curr_pattern: WordPattern::OneOf(vec!["象".to_string(), "像".to_string()]),
            bonus: -40.0,
            name: "anti_science_phase_confusion".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(science_phase_prefixes),
            curr_pattern: WordPattern::Exact("相".to_string()),
            bonus: 28.0,
            name: "science_phase_prefix".to_string(),
        });

        // 4. 相 ➔ 科學相態後綴 (態、分、層析、位、變、圖、差、轉移)
        let science_phase_suffixes = vec![
            "態".to_string(), "分".to_string(), "層析".to_string(), "位".to_string(),
            "變".to_string(), "圖".to_string(), "差".to_string(), "轉移".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("相".to_string()),
            curr_pattern: WordPattern::OneOf(science_phase_suffixes),
            bonus: 28.0,
            name: "science_phase_suffix".to_string(),
        });

        // 5. 交互動詞結構: 相 ➔ 動詞 (相處、相伴、相對、相信、相撞、相逢、相輔、相依、相愛、相敬、相提)
        let mutuality_verbs = vec![
            "輔".to_string(), "成".to_string(), "處".to_string(), "伴".to_string(), "對".to_string(),
            "信".to_string(), "撞".to_string(), "逢".to_string(), "比".to_string(), "當".to_string(),
            "識".to_string(), "見".to_string(), "戀".to_string(), "聚".to_string(), "讓".to_string(),
            "救".to_string(), "爭".to_string(), "助".to_string(), "差".to_string(), "稱".to_string(),
            "愛".to_string(), "依".to_string(), "關".to_string(), "隔".to_string(), "反".to_string(),
            "仿".to_string(), "繼".to_string(), "待".to_string(), "扣".to_string(), "通".to_string(),
            "遇".to_string(), "投".to_string(), "結合".to_string(), "提並論".to_string(), "敬如賓".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("相".to_string()),
            curr_pattern: WordPattern::OneOf(mutuality_verbs),
            bonus: 26.0,
            name: "xiang_mutuality_verb".to_string(),
        });

        // Rule Set C: 象 (Xiàng - 宏觀趨勢、抽象概念、自然跡象、大象實體)
        // 1. 抑制宏觀/抽象前綴接「相」或「像」
        let macro_abstract_prefixes = vec![
            "現".to_string(), "天".to_string(), "跡".to_string(), "亂".to_string(),
            "意".to_string(), "心".to_string(), "抽".to_string(), "印".to_string(),
            "形象".to_string(), "氣象".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "現".to_string(), "天".to_string(), "跡".to_string(), "抽".to_string(), "印".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec!["相".to_string(), "像".to_string()]),
            bonus: -40.0,
            name: "anti_macro_abstract_confusion".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(macro_abstract_prefixes),
            curr_pattern: WordPattern::Exact("象".to_string()),
            bonus: 28.0,
            name: "xiang_macro_abstract_attraction".to_string(),
        });

        // 2. 象 ➔ 宏觀/象徵/預報 (徵、牙、學、台、預報、級、化、群)
        let macro_abstract_suffixes = vec![
            "徵".to_string(), "牙".to_string(), "學".to_string(), "台".to_string(),
            "預報".to_string(), "級".to_string(), "化".to_string(), "群".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("象".to_string()),
            curr_pattern: WordPattern::OneOf(macro_abstract_suffixes),
            bonus: 28.0,
            name: "xiang_macro_abstract_tail".to_string(),
        });

        // 3. 宏觀演變構式: 萬 / 氣 ➔ 象 ➔ 更新 / 萬千 / 森羅
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["萬".to_string(), "包羅萬".to_string(), "氣".to_string()]),
            curr_pattern: WordPattern::Exact("象".to_string()),
            bonus: 28.0,
            name: "xiang_macro_evolution_prefix".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("象".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["更新".to_string(), "萬千".to_string(), "森羅".to_string()]),
            bonus: 30.0,
            name: "xiang_macro_evolution_tail".to_string(),
        });

        // Rule Set D: Special Domain Overrides (衝突消解與領域特化)
        // 1. 氣相 vs 氣象: 科學層析語境下強烈鎖定「氣相」
        let science_override_targets = vec![
            "層析".to_string(), "譜".to_string(), "沉積".to_string(), "平衡".to_string(), "反應".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("氣象".to_string()),
            curr_pattern: WordPattern::OneOf(science_override_targets.clone()),
            bonus: -45.0,
            name: "anti_qixiang_science_override".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("氣相".to_string()),
            curr_pattern: WordPattern::OneOf(science_override_targets),
            bonus: 35.0,
            name: "qixiang_science_override".to_string(),
        });

        // 氣象預報正向引力
        let weather_targets = vec![
            "預報".to_string(), "台".to_string(), "局".to_string(), "局長".to_string(),
            "觀測".to_string(), "衛星".to_string(), "站".to_string(), "天氣".to_string(),
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("氣象".to_string()),
            curr_pattern: WordPattern::OneOf(weather_targets),
            bonus: 35.0,
            name: "qixiang_weather_attraction".to_string(),
        });

        // 2. 假相 vs 假象: 佛學典籍語境下覆蓋為「假相」
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["凡所有".to_string(), "虛妄".to_string(), "離幻".to_string()]),
            curr_pattern: WordPattern::Exact("假象".to_string()),
            bonus: -35.0,
            name: "anti_buddhist_jiaxiang_confusion".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["凡所有".to_string(), "虛妄".to_string(), "離幻".to_string(), "照見".to_string(), "色即是空".to_string()]),
            curr_pattern: WordPattern::Exact("假相".to_string()),
            bonus: 35.0,
            name: "buddhist_jiaxiang_override".to_string(),
        });

        // 3. 圖相 vs 圖像: 佛學法相莊嚴語境
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["佛".to_string(), "莊嚴".to_string(), "法相".to_string()]),
            curr_pattern: WordPattern::Exact("圖相".to_string()),
            bonus: 35.0,
            name: "buddhist_tuxiang_override".to_string(),
        });

        // ── 並列連詞構式: [名詞/人稱代詞/實詞] ➔ 和 (ㄏㄢˋ / ㄏㄜˊ) ➔ [名詞/人稱代詞/實詞] ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Nominal,
            curr_pattern: WordPattern::Exact("和".to_string()),
            bonus: 26.0,
            name: "nominal_conjunction_he".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("和".to_string()),
            curr_pattern: WordPattern::Nominal,
            bonus: 24.0,
            name: "conjunction_he_nominal".to_string(),
        });

        // ── 時態事件副句與時間副詞關聯構式 ──
        // 1. [複合謂詞/實詞 (長度>=2)] ➔ 時 (服務時、工作時、開車時、處理時、離開時、吃飯時)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::CompoundContentWord,
            curr_pattern: WordPattern::Exact("時".to_string()),
            bonus: 26.0,
            name: "temporal_event_shi".to_string(),
        });
        // 2. 時 ➔ 就 (服務時就、那時就、當時就、這時就)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("時".to_string()),
            curr_pattern: WordPattern::Exact("就".to_string()),
            bonus: 28.0,
            name: "temporal_shi_adverb_jiu".to_string(),
        });

        // ── 被動語態構式: 被 + 嘲笑/譏笑/批評/排擠 ──
        let passive_disfavour_verbs = vec![
            "譏笑", "嘲笑", "嘲弄", "批評", "排擠", "責備", "拒絕", "懲罰", "欺騙", "欺負", "看輕", "鄙視",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("被".to_string()),
            curr_pattern: WordPattern::OneOf(passive_disfavour_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "passive_disfavour_verb".to_string(),
        });

        // ── 情態副詞「快」+ 動詞 (快要/即將: 快哭了、快瘋了、快到了、快完了、快走) ──
        let kuai_actions = vec![
            "哭", "哭了", "瘋了", "到了", "完了", "死了", "走了", "來了", "下雨", "結束", "崩潰", "跑",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("快".to_string()),
            curr_pattern: WordPattern::OneOf(kuai_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "adverb_kuai_action".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["我".to_string(), "你".to_string(), "他".to_string(), "她".to_string()]),
            curr_pattern: WordPattern::Exact("快".to_string()),
            bonus: 26.0,
            name: "pronoun_adverb_kuai".to_string(),
        });

        // ── 社交與人際動詞 ➔ 親屬/稱謂名詞 (問太太、找太太、陪太太、跟太太、向太太) ──
        let interpersonal_verbs = vec![
            "問", "跟", "找", "陪", "帶", "向", "通知", "告訴", "交代", "囑咐", "請教", "問候",
        ];
        let kinship_titles = vec![
            "太太", "先生", "朋友", "家人", "媽媽", "爸爸", "老師", "同學", "同事", "長官", "客戶",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(interpersonal_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(kinship_titles.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "interpersonal_verb_kinship".to_string(),
        });

        // ── 疑問修辭構式: [名詞/實詞] ➔ 何在 (紙本何在、公理何在、意義何在、誠信何在、尊嚴何在) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("何在".to_string()),
            bonus: 32.0,
            name: "nominal_hezai".to_string(),
        });
        // ── 疑問副詞 ➔ 重複/繼續副詞或行動動詞: 如何 ➔ [再, 做, 作, 去, 處理, 面對, 解決, 看待, 進行, 落實, 因應, 發展] ──
        let ruhe_targets = vec![
            "再", "做", "作", "去", "處理", "面對", "解決", "看待", "進行", "落實", "因應", "發展", "安排", "打算",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("如何".to_string()),
            curr_pattern: WordPattern::OneOf(ruhe_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "ruhe_action".to_string(),
        });

        // ── 重複/接續副詞「再」➔ 動作謂詞: 再 ➔ [做, 作, 說, 看, 想, 議, 談, 去, 來, 辦, 研究, 商量, 決定, 發展, 處理] ──
        let zai_actions = vec![
            "做", "作", "說", "看", "想", "議", "談", "去", "來", "辦", "研究", "商量", "決定", "發展", "處理", "討論",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("再".to_string()),
            curr_pattern: WordPattern::OneOf(zai_actions.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "adverb_zai_action".to_string(),
        });

        // ── 動作謂詞 ➔ 發展: [做, 作, 有, 求, 謀, 促, 推動, 帶動] ➔ 發展 ──
        let fazhan_verbs = vec!["做", "作", "有", "求", "謀", "促", "推動", "帶動", "持續", "健全"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(fazhan_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("發展".to_string()),
            bonus: 26.0,
            name: "action_fazhan".to_string(),
        });

        // ── 情勢名詞 ➔ 動態謂詞: 事態 ➔ [發展, 如何, 嚴重, 擴大, 演變, 走向, 變化, 升級] ──
        let shitai_predicates = vec![
            "發展", "如何", "嚴重", "擴大", "演變", "走向", "變化", "升級", "明朗",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("事態".to_string()),
            curr_pattern: WordPattern::OneOf(shitai_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "shitai_development".to_string(),
        });

        // ── 觀察標記 ➔ 情境焦點: [且看, 靜觀, 觀察, 關注] ➔ 事態 ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["且看".to_string(), "靜觀".to_string(), "觀察".to_string(), "關注".to_string()]),
            curr_pattern: WordPattern::Exact("事態".to_string()),
            bonus: 30.0,
            name: "qiekan_shitai".to_string(),
        });

        // ── 工作流程/階段構式: [今天/案子/專案/會議/討論/工作/事情/進度] ➔ 先到 ➔ [這裡/這/此/為止/一段落] ──
        let procedural_topics = vec![
            "今天", "案子", "專案", "會議", "討論", "工作", "事情", "進度", "行程", "這", "那",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(procedural_topics.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("先到".to_string()),
            bonus: 28.0,
            name: "procedural_xiandao".to_string(),
        });
        let xiandao_destinations = vec![
            "這裡", "這", "此", "為止", "一段落", "學校", "公司", "家", "現場", "底",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("先到".to_string()),
            curr_pattern: WordPattern::OneOf(xiandao_destinations.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "xiandao_destination".to_string(),
        });

        // ── 數量/指示修飾語 ➔ 抽象過程名詞 (這一整段過程、這段過程、整個過程、全部過程) ──
        let guocheng_modifiers = vec![
            "這段", "那段", "整段", "一整段", "這一整段", "那一段", "整個", "全部", "這個", "那個", "完整", "所有",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(guocheng_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("過程".to_string()),
            bonus: 30.0,
            name: "modifier_guocheng".to_string(),
        });

        // ── 名詞事體 ➔ 使役動詞 ➔ 代詞 (過程讓我、事情讓我、經歷讓大家、這讓我) ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("過程".to_string()),
            curr_pattern: WordPattern::Exact("讓".to_string()),
            bonus: 28.0,
            name: "guocheng_rang".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("讓".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["我".to_string(), "我們".to_string(), "大家".to_string(), "人".to_string(), "他".to_string(), "她".to_string()]),
            bonus: 28.0,
            name: "rang_pronoun_causative".to_string(),
        });

        // ── 心理認知回想動態: [我/人/大家] ➔ 回憶 / 回憶起 / 想起 / 記得 ──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["我".to_string(), "我們".to_string(), "大家".to_string(), "令人".to_string(), "讓人".to_string()]),
            curr_pattern: WordPattern::OneOf(vec!["回憶".to_string(), "回憶起".to_string(), "想起".to_string(), "記得".to_string(), "體會到".to_string(), "感受到".to_string()]),
            bonus: 28.0,
            name: "pronoun_mental_recall".to_string(),
        });

        // ── Layer 3: 離合動詞 ──
        let vo_list = [
            ("洗", "澡", 4, 16.0),
            ("煮", "菜", 4, 16.0),
            ("吃", "飯", 4, 15.0),
            ("吃", "晚餐", 4, 16.0),
            ("吃", "早餐", 4, 16.0),
            ("吃", "中餐", 4, 16.0),
            ("吃", "東西", 4, 15.0),
            ("睡", "覺", 4, 18.0),
            ("理", "髮", 3, 16.0),
            ("幫", "忙", 4, 16.0),
            ("生", "氣", 3, 14.0),
            ("散", "步", 3, 15.0),
            ("起", "床", 3, 16.0),
            ("跑", "步", 3, 14.0),
            ("唱", "歌", 3, 15.0),
            ("跳", "舞", 3, 15.0),
            ("游", "泳", 3, 15.0),
            ("聊", "天", 3, 15.0),
            ("開", "會", 3, 14.0),
            ("放", "假", 3, 15.0),
            ("發", "燒", 3, 15.0),
            ("照", "相", 3, 15.0),
            ("畢", "業", 3, 15.0),
            ("道", "歉", 3, 15.0),
            ("敬", "禮", 3, 15.0),
            ("告", "狀", 3, 15.0),
            ("上", "課", 3, 14.0),
            ("下", "課", 3, 14.0),
            ("上", "班", 3, 14.0),
            ("下", "班", 3, 14.0),
            ("見", "面", 3, 15.0),
            ("打", "仗", 3, 14.0),
            ("出", "差", 3, 14.0),
            ("鞠", "躬", 3, 15.0),
            ("拌", "嘴", 3, 14.0),
        ];
        for &(v, o, dist, bonus) in &vo_list {
            self.vo_pairs.push(VOCollocation {
                verb: v.to_string(),
                object: o.to_string(),
                max_distance: dist,
                bonus_score: bonus,
            });
        }
    }

    /// 從 TSV 檔案載入使用者自訂或 Gemma 蒸餾出的動態轉移矩陣
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let p = path.as_ref();
        let file = File::open(p)?;
        let reader = BufReader::new(file);
        let mut count = 0;

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 3 {
                let prev = parts[0].trim();
                let curr = parts[1].trim();
                if let Ok(bonus) = parts[2].trim().parse::<f64>() {
                    self.transitions.insert((prev.to_string(), curr.to_string()), bonus);
                    count += 1;
                }
            }
        }

        self.file_path = Some(p.to_path_buf());
        if let Ok(meta) = std::fs::metadata(p) {
            self.last_modified = meta.modified().ok();
        }

        Ok(count)
    }

    /// 熱重載檢查
    pub fn maybe_reload(&mut self) {
        if let Some(ref path) = self.file_path {
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(mtime) = meta.modified() {
                    let need_reload = match self.last_modified {
                        Some(last) => mtime > last,
                        None => true,
                    };
                    if need_reload {
                        let p = path.clone();
                        let _ = self.load_tsv(p);
                    }
                }
            }
        }
    }

    /// 計算前驅節點轉移至當前節點的語意句法轉移加權分
    ///
    /// 查找順序（優先順序由高到低）:
    ///   1. 精確二元搭配 (Layer 2 exact match)
    ///   2. 語法規則通配 (Layer 1 grammar rules)
    pub fn calculate_transition_bonus(
        &mut self,
        prev_word: &str,
        prev_bopomofo: &str,
        curr_word: &str,
        curr_bopomofo: &str,
    ) -> f64 {
        self.maybe_reload();

        // ── 破音字古音與孤例隔離層 (Archaic & Isolated Polyphonic Isolation Layer) ──
        // 若當前單字或前驅單字使用的是零複詞古音或成語孤例次要音（如 感 ㄏㄢˋ、庫 ㄕㄜˋ、害 ㄏㄜˊ、色 ㄕㄜˋ、樣 ㄒㄧㄤˋ、句 ㄍㄡˋ、不 ㄈㄡ、得 ㄉㄞˇ、日 ㄇㄧˋ 等）
        // 立即施加 -45.0 重度語意懲罰，且徹底阻斷所有 Layer 1 / Layer 2 正向語法引力，防止劫掠合法同音字（如 和、設、何、夠、否、歹、密）
        if crate::router::archaic_table::is_archaic_polyphonic(curr_word, curr_bopomofo)
            || crate::router::archaic_table::is_archaic_polyphonic(prev_word, prev_bopomofo)
        {
            return -45.0;
        }

        let mut bonus = 0.0;

        // 抑制口吃音重疊：「感謝」後接同音「ㄒㄧㄝˋ」單字（嚴防破壞「敢+謝謝」構式）
        if prev_word == "感謝" && curr_bopomofo.starts_with("ㄒㄧㄝ") && curr_word.chars().count() == 1 {
            bonus -= 50.0;
        }

        // Layer 2: 精確二元搭配查表（最高優先 — 使用者自訂或 doc_learning 的精確搭配）
        if let Some(&score) = self.transitions.get(&(prev_word.to_string(), curr_word.to_string())) {
            bonus += score;
        }

        // Layer 1: 語法規則通配匹配
        for rule in &self.grammar_rules {
            if rule.prev_pattern.matches(prev_word, &self.function_words)
                && rule.curr_pattern.matches(curr_word, &self.function_words)
            {
                // ── 讀音錨定檢查 (Phonetic Anchoring Guards) ──
                // 1. 連詞「和」：必須讀 ㄏㄢˋ 或 ㄏㄜˊ（排除 ㄏㄨㄛˋ 和麵、ㄏㄜˋ 附和、ㄏㄨˊ 碰和）
                if rule.name.starts_with("nominal_conjunction_he")
                    && !curr_bopomofo.starts_with("ㄏㄢ")
                    && !curr_bopomofo.starts_with("ㄏㄜˊ")
                {
                    continue;
                }
                if rule.name.starts_with("conjunction_he_nominal")
                    && !prev_bopomofo.starts_with("ㄏㄢ")
                    && !prev_bopomofo.starts_with("ㄏㄜˊ")
                {
                    continue;
                }
                // 2. 方位詞「中」：作方位後置詞時必須讀 ㄓㄨㄥ（排除動詞 ㄓㄨㄥˋ 中毒/中獎）
                if curr_word == "中"
                    && (rule.name.contains("localizer") || rule.name.contains("guo_chengzhong"))
                    && !curr_bopomofo.starts_with("ㄓㄨㄥ")
                {
                    continue;
                }
                if prev_word == "中" && rule.name.contains("localizer") && !prev_bopomofo.starts_with("ㄓㄨㄥ") {
                    continue;
                }

                // 3. 量詞不能直接修飾「狠」、「快」或帶「了」之完成態動詞（如 嚴禁「*句狠」、「*塊哭了」、「*個走了」）
                if rule.name == "classifier_noun"
                    && (curr_word == "狠" || curr_word == "快" || (curr_word.ends_with("了") && curr_word.chars().count() >= 2))
                {
                    continue;
                }

                // 4. 程度副詞修飾單字時，必須為可分級形容詞（嚴禁修飾任意動作動詞或名詞單字，杜絕「*真試」、「*真事」、「*很事」、「*太話」）
                if rule.name == "degree_adv_content_adj" && curr_word.chars().count() == 1 {
                    let gradable_adjectives = [
                        "好", "大", "小", "多", "少", "快", "慢", "長", "短", "高", "低", "美", "難", "新", "舊",
                        "貴", "熱", "冷", "棒", "讚", "累", "忙", "深", "淺", "厚", "薄", "紅", "黑", "白", "早",
                        "晚", "遠", "近", "差", "重", "輕", "硬", "軟", "甜", "苦", "鹹", "香", "痛", "純", "真",
                        "假", "準", "穩", "省", "狂", "狠", "凶", "兇", "酷", "爽", "爛",
                    ];
                    if !gradable_adjectives.contains(&curr_word) {
                        continue;
                    }
                }

                // 5. 時間後置標記「時」不能接在數量詞、指示詞或程度副詞之後（嚴禁「*很多時」、「*一些時」、「*太時」）
                if rule.name == "temporal_event_shi" {
                    let non_temporal_prev = ["很多", "許多", "不少", "一些", "幾個", "無數", "非常", "太", "特別", "這", "那", "的"];
                    if non_temporal_prev.contains(&prev_word) {
                        continue;
                    }
                }

                // 6. 句末疑問助詞「嗎」：作句末語氣助詞時必須讀輕聲 ˙ㄇㄚ 或一聲 ㄇㄚ，絕不可在三聲 ㄇㄚˇ（嗎啡之孤例音）時充當句末助詞劫掠「碼」！
                if rule.name == "sentence_final_particle" && curr_word == "嗎" && curr_bopomofo.contains('ˇ') {
                    continue;
                }

                bonus += rule.bonus;
                break; // 只取第一條匹配的規則（避免多條規則重複加分）
            }
        }

        // ── 形態學動態規則 1: 派生名詞後綴「~庫」加權 (規則+庫, 資料+庫, 代碼+庫, 模型+庫, 資源+庫, 冷凍+庫) ──
        // 限制：prev_word 必須為 2~4 字之實詞，避免破壞固有 2 字詞（如 金庫、倉庫、寶庫、書庫）
        // 關鍵：後綴「~庫」必須讀「ㄎㄨˋ」，絕不可讓罕見破音字「庫 (ㄕㄜˋ)」冒充名詞後綴劫掠！
        if curr_word == "庫" && curr_bopomofo.contains("ㄎㄨ") && !self.function_words.contains(prev_word) && prev_word.chars().count() >= 2 && prev_word.chars().count() <= 4 {
            bonus += 16.0;
        }

        // 抑制罕見孤立單字「庫 (ㄕㄜˋ)」冒充「設/社/射/攝」（教育部詞典古姓氏次要音讀）
        if curr_word == "庫" && curr_bopomofo.contains("ㄕㄜ") {
            bonus -= 35.0;
        }

        // ── 形態學動態規則 2: 派生通用名詞後綴加權 (~化, ~性, ~感, ~度, ~師, ~員, ~器) ──
        let productive_suffixes = ["化", "性", "感", "度", "師", "員", "器"];
        if productive_suffixes.contains(&curr_word) && !self.function_words.contains(prev_word) && prev_word.chars().count() >= 2 && prev_word.chars().count() <= 4 {
            // 關鍵安全防線 1：前驅詞若結尾為「的」或為否定動詞「沒有/不是/不要/不用/完全」，絕不可接名詞後綴（防「*普通的化」、「*沒有性去」）！
            if prev_word.ends_with("的") || prev_word == "沒有" || prev_word == "不是" || prev_word == "不要" || prev_word == "不用" || prev_word == "完全" {
                // 不給予名詞後綴加分
            }
            // 關鍵安全防線 2：後綴「~感」必須讀「ㄍㄢˇ」，絕不允許罕見古音「感 (ㄏㄢˋ)」冒充名詞後綴劫掠連詞「和 (ㄏㄢˋ)」！
            else if curr_word == "感" && !curr_bopomofo.contains("ㄍㄢ") {
                // 不給予名詞後綴加分
            } else {
                bonus += 14.0;
            }
        }

        // 抑制罕見孤立古音單字「感 (ㄏㄢˋ)」冒充「和/漢/汗/翰」（荀子通假古音，現代零複詞）
        if curr_word == "感" && curr_bopomofo.contains("ㄏㄢ") {
            bonus -= 45.0;
        }

        // ── 形態學動態規則 3: 疊字後接修飾標記 (AA+的, AA+地, AABB+的, AABB+地) ──
        if (curr_word == "的" || curr_word == "地") && prev_word.chars().count() >= 2 {
            let chars: Vec<char> = prev_word.chars().collect();
            let is_reduplicated = (chars.len() == 2 && chars[0] == chars[1])
                || (chars.len() == 4 && chars[0] == chars[1] && chars[2] == chars[3]);
            if is_reduplicated {
                bonus += 18.0;
            }
        }

        // ── 形態學動態規則 4: 修飾語接中心名詞「話」（...的話、普通的話、說的話、真心的話）──
        if curr_word == "話" && (prev_word == "的" || prev_word.ends_with("的") || prev_word == "普通") {
            bonus += 26.0;
        }

        // 抑制荒謬偏正接後綴「的 + 化」
        if curr_word == "化" && (prev_word == "的" || prev_word.ends_with("的")) {
            bonus -= 40.0;
        }

        // 抑制被動標記接非動詞名詞「被 + 績效/網室」
        if prev_word == "被" && (curr_word == "績效" || curr_word == "網室") {
            bonus -= 45.0;
        }

        // 抑制動詞接石臼「服務 + 石臼」
        if prev_word == "服務" && curr_word == "石臼" {
            bonus -= 45.0;
        }

        // ── 形態學動態規則 5: 比擬後綴「~般」加權 (如 松鼠般, 密碼般, 神仙般, 英雄般, 教科書般) ──
        if curr_word == "般" && curr_bopomofo.contains("ㄅㄢ") && !self.function_words.contains(prev_word) && prev_word.chars().count() >= 2 && prev_word.chars().count() <= 4 {
            bonus += 26.0;
        }

        // 抑制重複連詞「和 + 與」（兩個相同語法功能之並列連詞疊架）
        if prev_word == "和" && curr_word == "與" {
            bonus -= 45.0;
        }

        // 並列連詞「與 ➔ 後項實詞」構式引力 (與愛, 與死, 與寧靜, 與和平, 與生, 與肉)
        // 關鍵語言學原理：加權必須置於「與 ➔ 後項」，絕不可放在「前項 ➔ 與」！
        // 否則所有結尾為「語」的名詞（如「死語」、「漢語」、「日語」）在獨立輸入時，都會被單鍵「前項 + 與」截斷劫掠！
        if prev_word == "與" && matches!(curr_word, "愛" | "死" | "生" | "寧靜" | "和平" | "義務" | "肉" | "淚" | "仇" | "美") {
            bonus += 35.0;
        }
        if prev_word == "死語" && curr_word == "愛" {
            bonus -= 40.0;
        }

        // 抑制古刑律死因「瘐死」（獄中病死）跨詞隙劫掠「[動詞/名詞] + 與 + 死」
        if (prev_word == "愛" || prev_word == "生" || prev_word == "死" || prev_word == "怕" || prev_word == "恨") && curr_word == "瘐死" {
            bonus -= 50.0;
        }
        if curr_word == "瘐死" && (prev_word == "與" || prev_word == "和") {
            bonus -= 50.0;
        }

        // 抑制割裂「密/蜜 + 嗎」
        if (prev_word == "密" || prev_word == "蜜") && (curr_word == "嗎" || curr_word == "嗎啡") {
            bonus -= 45.0;
        }

        // 抑制古漢代罪犯「赭衣」跨詞隙截斷「使用者一開始」
        if (prev_word == "使用" || prev_word == "使用者") && curr_word == "赭衣" {
            bonus -= 50.0;
        }
        if curr_word == "赭衣" {
            bonus -= 40.0;
        }
        if prev_word == "使用者" && (curr_word == "一開始" || curr_word == "一") {
            bonus += 35.0;
        }

        // 抑制軍事生僻詞「劈刺」劫掠「批次」
        if (prev_word == "支援" || prev_word == "若能" || prev_word == "進行") && curr_word == "劈刺" {
            bonus -= 50.0;
        }
        if curr_word == "劈刺" && curr_word == "轉換" {
            bonus -= 50.0;
        }
        if prev_word == "批次" && curr_word == "轉換" {
            bonus += 35.0;
        }
        if prev_word == "支援" && curr_word == "批次" {
            bonus += 32.0;
        }
        if prev_word == "若能" && curr_word == "支援" {
            bonus += 32.0;
        }
        if prev_word == "若能" && (curr_word == "之源" || curr_word == "之") {
            bonus -= 45.0;
        }

        // 動賓固定搭配「沒有 ➔ 興趣」，防名詞後綴「~性」劫掠
        if prev_word == "沒有" && curr_word == "興趣" {
            bonus += 35.0;
        }
        if (prev_word == "沒有" || prev_word == "毫無") && curr_word == "性" {
            bonus -= 35.0;
        }

        // 時相副詞「已 ➔ 下載」，防「倚下載」
        if prev_word == "已" && curr_word == "下載" {
            bonus += 35.0;
        }
        if prev_word == "倚" && curr_word == "下載" {
            bonus -= 45.0;
        }

        // 指示詞修飾班次/班級「這/那/上/下/哪/某/該 + 一班」
        let ban_demonstratives = ["這", "那", "上", "下", "哪", "某", "該", "每"];
        if ban_demonstratives.contains(&prev_word) && curr_word == "一班" {
            bonus += 35.0;
        }
        if ban_demonstratives.contains(&prev_word) && curr_word == "一般" {
            bonus -= 40.0; // 嚴防「*這一般」劫掠「這一班」！
        }
        if (prev_word == "這一班" || prev_word == "那一班") && curr_word == "所謂" {
            bonus += 30.0;
        }
        if prev_word == "一般" && curr_word == "所謂" {
            bonus += 25.0;
        }
        if prev_word == "所謂" && (curr_word == "精英" || curr_word == "菁英") {
            bonus += 32.0;
        }

        // 科技多重修飾名詞鏈「無損 ➔ 高效 ➔ 壓縮 ➔ 音檔」，杜絕「高校」、「呀縮」、「姻黨」
        if prev_word == "無損" && curr_word == "高效" {
            bonus += 35.0;
        }
        if prev_word == "無損" && curr_word == "壓縮" {
            bonus += 35.0;
        }
        if prev_word == "高效" && curr_word == "壓縮" {
            bonus += 38.0;
        }
        if prev_word == "壓縮" && (curr_word == "音檔" || curr_word == "檔案" || curr_word == "圖檔") {
            bonus += 38.0;
        }
        if (prev_word == "無損" || prev_word == "高效") && curr_word == "高校" {
            bonus -= 45.0; // 杜絕「高校」劫掠「高效」
        }
        if prev_word == "高校" && (curr_word == "壓縮" || curr_word == "音檔") {
            bonus -= 45.0;
        }

        // 核心心理與動賓修飾鏈「正常發揮」、「心累」、「速效藥」、「手動加詞」
        if prev_word == "正常" && curr_word == "發揮" {
            bonus += 35.0;
        }
        if prev_word == "正常" && curr_word == "發灰" {
            bonus -= 50.0; // 杜絕相聲繞口令「發灰」劫掠「正常發揮」
        }
        if (prev_word == "我" || prev_word == "真" || prev_word == "好") && curr_word == "心累" {
            bonus += 32.0;
        }
        if prev_word == "心" && curr_word == "累" {
            bonus += 30.0;
        }
        if prev_word == "心" && curr_word == "類" {
            bonus -= 35.0;
        }
        if prev_word == "速效" && curr_word == "藥" {
            bonus += 35.0;
        }
        if prev_word == "速效" && curr_word == "要" {
            bonus -= 35.0;
        }
        if (prev_word == "不用" || prev_word == "免" || prev_word == "無需") && curr_word == "手動" {
            bonus += 30.0;
        }
        if prev_word == "手動" && curr_word == "加詞" {
            bonus += 35.0;
        }
        if prev_word == "手" && curr_word == "棟" {
            bonus -= 50.0; // 杜絕「手棟」
        }
        if curr_word == "家祠" {
            bonus -= 45.0; // 杜絕古宗族祠堂劫掠「加詞」
        }

        // 軟體專業構式「自建詞庫」與「智慧監測/分析」
        if prev_word == "使用者" && curr_word == "自建" {
            bonus += 35.0;
        }
        if prev_word == "自建" && (curr_word == "詞庫" || curr_word == "檔案" || curr_word == "規則") {
            bonus += 35.0;
        }
        if prev_word == "自薦" && curr_word == "詞庫" {
            bonus -= 50.0; // 杜絕「自薦詞庫」
        }
        if (prev_word == "一個" || prev_word == "這項" || prev_word == "該") && curr_word == "置喙" {
            bonus -= 45.0; // 數量詞不接「置喙」
        }
        if (prev_word == "一個" || prev_word == "這項") && curr_word == "智慧" {
            bonus += 25.0;
        }
        if prev_word == "智慧" && (curr_word == "監測" || curr_word == "分析" || curr_word == "提取" || curr_word == "規則") {
            bonus += 38.0;
        }
        if prev_word == "置喙" && (curr_word == "監測" || curr_word == "分析" || curr_word == "提取") {
            bonus -= 50.0;
        }

        // ── 跨詞隙搭橋碰撞截斷防禦 (Cross-Boundary Phonetic Bridge Anti-Rules) ──
        // 1. 「盲人摸象時」vs「盲人摸橡實」
        if prev_word == "盲人摸" && curr_word == "橡實" {
            bonus -= 50.0;
        }
        if prev_word == "盲人摸象" && curr_word == "實" {
            bonus -= 50.0;
        }
        if prev_word == "盲人摸象" && curr_word == "時" {
            bonus += 32.0;
        }
        if (prev_word == "摸" || prev_word == "敲打") && curr_word == "橡實" {
            bonus += 32.0;
        }
        if prev_word == "橡實" && curr_word == "的" {
            bonus += 26.0;
        }
        if prev_word == "照說" && curr_word == "無法" {
            bonus += 30.0;
        }
        if prev_word == "無法" && curr_word == "相提並論" {
            bonus += 35.0;
        }

        // 2. 「坐一下石頭」vs「坐一下時」防「下石」截斷
        if (prev_word == "一下" || prev_word == "坐一" || prev_word == "坐") && curr_word == "下石" {
            bonus -= 50.0;
        }
        if prev_word == "下石" && (curr_word == "頭" || curr_word == "時") {
            bonus -= 50.0;
        }
        if prev_word == "坐" && curr_word == "一下" {
            bonus += 28.0;
        }
        if prev_word == "坐一下" && curr_word == "石頭" {
            bonus += 30.0;
        }
        if prev_word == "坐一下" && curr_word == "時" {
            bonus += 32.0;
        }
        if prev_word == "一下" && (curr_word == "石頭" || curr_word == "木頭") {
            bonus += 28.0;
        }
        if (prev_word == "石頭" || prev_word == "木頭") && curr_word == "做" {
            bonus += 30.0;
        }
        if prev_word == "做" && curr_word == "的" {
            bonus += 28.0;
        }
        if prev_word == "做的" && curr_word == "椅子" {
            bonus += 32.0;
        }
        if prev_word == "分別" && curr_word == "即使" {
            bonus += 28.0;
        }

        // 3. 「此碑石」vs「悲中之悲」vs「碑中之碑」
        if (prev_word == "悲中之" || prev_word == "悲") && curr_word == "碑石" {
            bonus -= 50.0;
        }
        if prev_word == "此" && curr_word == "碑石" {
            bonus += 30.0;
        }
        if prev_word == "所載" && curr_word == "悲淒" {
            bonus += 30.0;
        }
        if prev_word == "悲淒" && curr_word == "慘事" {
            bonus += 30.0;
        }
        if prev_word == "乃" && curr_word == "悲中之悲" {
            bonus += 35.0;
        }
        if prev_word == "乃" && curr_word == "碑中之碑" {
            bonus -= 35.0;
        }
        if prev_word == "為" && curr_word == "碑中之碑" {
            bonus += 35.0;
        }
        if prev_word == "為" && curr_word == "悲中之悲" {
            bonus -= 35.0;
        }
        if prev_word == "此" && curr_word == "卑位" {
            bonus -= 50.0;
        }
        if prev_word == "卑位" && (curr_word == "碑中之碑" || curr_word == "悲中之悲") {
            bonus -= 50.0;
        }
        if prev_word == "致此碑" && curr_word == "為" {
            bonus += 35.0;
        }
        if prev_word == "此碑" && curr_word == "為" {
            bonus += 35.0;
        }
        if prev_word == "致" && curr_word == "此碑" {
            bonus += 32.0;
        }

        // 4. 「混打看板」防「打看」截斷
        if prev_word == "混" && curr_word == "打看" {
            bonus -= 50.0;
        }
        if prev_word == "打看" && (curr_word == "板" || curr_word == "看板") {
            bonus -= 50.0;
        }
        if prev_word == "混混" && curr_word == "打看" {
            bonus += 32.0;
        }
        if prev_word == "混打" && curr_word == "看板" {
            bonus += 35.0;
        }
        if prev_word == "看板" && curr_word == "上" {
            bonus += 28.0;
        }
        if prev_word == "歐漢" && curr_word == "大亂鬥" {
            bonus += 32.0;
        }

        // 5. 「街巷時鐘」防「橡實」截斷
        if prev_word == "街" && curr_word == "橡實" {
            bonus -= 50.0;
        }
        if prev_word == "橡實" && (curr_word == "鐘" || curr_word == "時鐘") {
            bonus -= 50.0;
        }
        if prev_word == "街巷" && curr_word == "時鐘" {
            bonus += 35.0;
        }
        if prev_word == "時鐘" && curr_word == "底下" {
            bonus += 32.0;
        }
        if prev_word == "前去" && curr_word == "街巷" {
            bonus += 28.0;
        }
        if prev_word == "打算" && curr_word == "備齊" {
            bonus += 30.0;
        }
        if prev_word == "備齊" && curr_word == "一盆" {
            bonus += 30.0;
        }

        // 6. 「很久也」防「九野」搭橋截斷
        if prev_word == "很" && curr_word == "九野" {
            bonus -= 50.0;
        }
        if prev_word == "九野" && (curr_word == "沒" || curr_word == "也") {
            bonus -= 50.0;
        }
        if prev_word == "很久" && curr_word == "也" {
            bonus += 35.0;
        }
        if prev_word == "也" && (curr_word == "沒關係" || curr_word == "沒" || curr_word == "沒有") {
            bonus += 25.0;
        }

        // 7. 「會自動」防「諱字」、「字棟」截斷
        if prev_word == "會" && curr_word == "諱字" {
            bonus -= 50.0;
        }
        if prev_word == "諱字" && (curr_word == "棟" || curr_word == "動") {
            bonus -= 50.0;
        }
        if prev_word == "字" && curr_word == "棟" {
            bonus -= 50.0;
        }
        if prev_word == "機制" && (curr_word == "會" || curr_word == "會自動") {
            bonus += 35.0;
        }
        if prev_word == "會" && (curr_word == "自動" || curr_word == "自動執行") {
            bonus += 35.0;
        }
        if prev_word == "自動" && curr_word == "執行" {
            bonus += 35.0;
        }
        if prev_word == "執行" && (curr_word == "嗎" || curr_word == "呢") {
            bonus += 25.0;
        }

        // 8. 「沒關係」防地名「關西」劫掠
        if prev_word == "沒" && curr_word == "關西" {
            bonus -= 50.0;
        }
        if prev_word == "沒" && curr_word == "關係" {
            bonus += 35.0;
        }
        if prev_word == "關係" && (curr_word == "嗎" || curr_word == "呢" || curr_word == "呀") {
            bonus += 25.0;
        }

        // 9. 「每告一個段落」防「美告」劫掠與斷詞
        if prev_word == "美" && (curr_word == "告" || curr_word == "告一段落" || curr_word == "告一個段落") {
            bonus -= 40.0;
        }
        if prev_word == "每" && (curr_word == "告" || curr_word == "告一段落" || curr_word == "告一個段落") {
            bonus += 35.0;
        }
        if (prev_word == "告一段落" || prev_word == "告一個段落") && curr_word == "就" {
            bonus += 30.0;
        }
        if prev_word == "段落" && curr_word == "就" {
            bonus += 25.0;
        }

        // 10. 文件副檔名後置「檔」防政治「黨」劫掠 (aup, aup3, md, txt, doc, pdf, csv, wav, mp3 等)
        let is_ascii_word = !prev_word.is_empty() && prev_word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let is_political_abbr = prev_word == "kmt" || prev_word == "dpp" || prev_word == "ccp" || prev_word == "gop";
        if (is_ascii_word && !is_political_abbr) || prev_word == "圖" || prev_word == "音" || prev_word == "影片" || prev_word == "封存" {
            if curr_word == "檔" {
                bonus += 38.0;
            } else if curr_word == "黨" {
                bonus -= 45.0;
            }
        }
        if prev_word == "個" && curr_word == "md" {
            bonus += 30.0;
        }
        if prev_word == "寫個" && curr_word == "md" {
            bonus += 35.0;
        }

        // 11. 反詰疑問句末「對吧」杜絕「隊吧」劫掠
        if curr_word == "吧" || curr_word == "ㄅㄚ" {
            if prev_word == "隊" {
                bonus -= 45.0; // 名詞「隊」極罕接語氣詞「吧」
            } else if prev_word == "對" {
                bonus += 35.0; // 反詰確認語氣
            }
        }

        // 12. 狀態與名詞搭配「重疊部份 / 重疊部分」
        if prev_word == "重疊" {
            if curr_word == "部份" || curr_word == "部分" {
                bonus += 40.0;
            } else if curr_word == "不" {
                bonus -= 40.0; // 重疊非正反問動詞，防幾何名詞語意割裂
            }
        }
        // 經典成語語境正向引力（《論語》名句：不憤不啟，不悱不發）
        if (prev_word == "不憤" && curr_word == "不啟") || (prev_word == "不悱" && curr_word == "不發") {
            bonus += 45.0;
        }

        // 13. 遍指代詞「各 clip 間」
        if prev_word == "各" && curr_word == "clip" {
            bonus += 35.0;
        }
        if prev_word == "clip" && curr_word == "間" {
            bonus += 38.0;
        }
        if prev_word == "個" && curr_word == "clip" {
            bonus -= 20.0;
        }

        // 14. 轉折副詞「其實」防單字切碎
        if (prev_word == "其" || prev_word == "期") && curr_word == "時" {
            bonus -= 50.0;
        }

        // 15. 文言偏正定語構式「...之路」(自我毀滅之路、成功之路、希望之路、不歸之路)
        if curr_word == "之路" {
            if prev_word == "自我毀滅" || prev_word == "毀滅" || prev_word == "成功" || prev_word == "希望" || prev_word == "不歸" {
                bonus += 40.0;
            } else if prev_word == "自我" {
                bonus += 25.0;
            }
        }
        if prev_word == "自我毀滅" && curr_word == "支路" {
            bonus -= 45.0; // 交通名詞支路防劫掠
        }

        // 16. 副詞與動態短語「就直接」、「直接揚棄」
        if prev_word == "就" && curr_word == "直接" {
            bonus += 38.0;
        }
        if prev_word == "我們就" && curr_word == "直接" {
            bonus += 42.0;
        }
        if prev_word == "直接" && curr_word == "揚棄" {
            bonus += 40.0;
        }

        // 17. 動賓及物代詞「揚棄它 / 擁有它 / 看到它」防「塌」劫掠
        if curr_word == "它" {
            if prev_word == "揚棄" || prev_word == "放棄" || prev_word == "擁有" || prev_word == "看到" || prev_word == "聽見" {
                bonus += 40.0;
            }
        }
        if curr_word == "塌" {
            if prev_word == "揚棄" || prev_word == "直接" || prev_word == "器" {
                bonus -= 45.0;
            }
        }

        // 18. 科技修飾詞構式「智慧型 / 智慧」+ 科技軟硬體名詞（輸入法、手機、裝置、系統、選字、選字詞）
        // 具備「型」字自然省略相容能力，並驅動「選字/選字詞 ➔ 正確率」評估動態鏈
        if prev_word == "智慧型" || prev_word == "智慧" {
            if matches!(curr_word, "輸入法" | "手機" | "裝置" | "手錶" | "系統" | "選字" | "選字詞" | "電視" | "家居") {
                bonus += 40.0;
            }
        }
        if (prev_word == "選字" || prev_word == "選字詞") && curr_word == "正確率" {
            bonus += 42.0;
        }
        if prev_word == "目前" {
            if curr_word == "主攻" {
                bonus += 42.0;
            } else if curr_word == "主公" {
                bonus -= 45.0; // 現代語境防主公劫掠主攻
            }
        }

        // 19. 典雅成語「殆無疑義」
        if prev_word == "殆" && curr_word == "無疑義" {
            bonus += 42.0;
        }
        if prev_word == "代" && curr_word == "無疑" {
            bonus -= 45.0;
        }

        // 20. 時間與動量「立刻再打一次」
        if prev_word == "立刻" && (curr_word == "再打" || curr_word == "再打一次") {
            bonus += 42.0;
        }
        if prev_word == "再" && curr_word == "打" {
            bonus += 38.0;
        }
        if prev_word == "粒" && curr_word == "刻" {
            bonus -= 45.0;
        }

        // 21. 助動動介與指示結構「將著力於此部分」
        if (prev_word == "本對話" || prev_word == "我們") && curr_word == "將" {
            bonus += 38.0;
        }
        if prev_word == "將" && (curr_word == "著力" || curr_word == "著力於") {
            bonus += 42.0;
        }
        if prev_word == "江" && curr_word == "著力" {
            bonus -= 45.0;
        }
        if prev_word == "此" {
            if curr_word == "部分" || curr_word == "部份" {
                bonus += 40.0;
            } else if curr_word == "不憤" || curr_word == "不忿" {
                bonus -= 45.0; // 指示代詞後接名詞部分，防不憤/不忿劫掠
            }
        }

        // 22. 全稱代詞修飾與防碎詞「任何 + 名詞」
        if prev_word == "任何" {
            bonus += 38.0;
        }
        if prev_word == "有" && curr_word == "任何" {
            bonus += 40.0;
        }

        // 23. 時間副詞「暫時先」與工作定態（正向語義搭配）
        if prev_word == "工作" && curr_word == "暫時" {
            bonus += 38.0;
        }
        if prev_word == "暫時" && curr_word == "先" {
            bonus += 40.0;
        }
        if (prev_word == "這邊" || prev_word == "這裏") && curr_word == "的" {
            bonus += 35.0;
        }

        // 24. 詞庫清零與變化構式 (變得更...)
        if prev_word == "詞庫" && curr_word == "清零" {
            bonus += 38.0;
        }
        if prev_word == "清零" && (curr_word == "後" || curr_word == "後的") {
            bonus += 40.0;
        }
        if prev_word == "有沒有" && curr_word == "變得" {
            bonus += 40.0;
        }
        if prev_word == "變得" && curr_word == "更" {
            bonus += 40.0;
        }
        if prev_word == "更" && curr_word == "行雲流水" {
            bonus += 38.0;
        }
        if prev_word == "行雲流水" && curr_word == "一點" {
            bonus += 35.0;
        }

        // 25. 破音字清查與程度修飾 (不夠徹底)
        if prev_word == "破音字" && curr_word == "清查" {
            bonus += 40.0;
        }
        if prev_word == "清查" && curr_word == "不夠" {
            bonus += 38.0;
        }
        if prev_word == "不夠" && curr_word == "徹底" {
            bonus += 40.0;
        }

        bonus
    }

    /// 跨詞隙動賓離合感知 (Lookback VO Check)
    /// 檢查在前面若干個詞的歷史中，是否存在能與當前詞形成離合詞的動詞
    pub fn calculate_vo_lookback_bonus(&self, prefix_words: &[&str], curr_word: &str) -> f64 {
        let n = prefix_words.len();
        if n == 0 {
            return 0.0;
        }

        let mut best_vo_bonus = 0.0;
        for pair in &self.vo_pairs {
            if pair.object == curr_word {
                let lookback_limit = pair.max_distance.min(n);
                for offset in 1..=lookback_limit {
                    let prev_idx = n - offset;
                    if prefix_words[prev_idx] == pair.verb {
                        if pair.bonus_score > best_vo_bonus {
                            best_vo_bonus = pair.bonus_score;
                        }
                        break;
                    }
                }
            }
        }

        best_vo_bonus
    }
}
