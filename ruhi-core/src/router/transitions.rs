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
    /// 匹配數詞與數值詞元（阿拉伯數字 0-9、小數 3.5、中文數詞 一~十/百/千/萬/億/兆/零/〇/兩/半/幾/數/多，以及 第/約/近/共/計/滿/逾 + 數詞）
    Numeric,
    /// 匹配任何詞
    Any,
}

fn is_rare_impostor(word: &str) -> bool {
    matches!(word, "婑" | "倭" | "䂺" | "婐" | "捰" | "𣇫" | "蟆" | "俛" | "犋" | "不憤")
}

impl WordPattern {
    pub fn is_numeric_token(word: &str) -> bool {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            return false;
        }
        // 排除雖由數詞字組成但實為副詞/連詞/熟語之非數量詞元
        if matches!(
            trimmed,
            "萬一" | "一一" | "一再" | "一旦" | "一直" | "一概" | "一向" | "一同" | "一齊"
                | "一併" | "一味" | "一舉" | "一體" | "一般" | "一樣" | "一致" | "一點"
                | "一些" | "一下" | "一會" | "十分" | "萬分" | "千萬" | "萬萬" | "百般"
                | "千方" | "四處" | "八方" | "九州" | "三思" | "二話" | "多少"
        ) {
            return false;
        }
        // 1. 阿拉伯數字、小數、百分比或帶千分位數值（如 "10", "2026", "3.5", "1,000", "50%"）
        if trimmed
            .chars()
            .last()
            .map(|c| c.is_ascii_digit() || c == '%')
            .unwrap_or(false)
            && trimmed.chars().any(|c| c.is_ascii_digit())
        {
            return true;
        }
        // 2. 中文數詞與概數/序數詞元（可帶「第、約、近、共、計、滿、逾、達、超」前綴）
        let mut chars = trimmed.chars().peekable();
        if let Some(&first) = chars.peek() {
            if matches!(first, '第' | '約' | '近' | '共' | '計' | '滿' | '逾' | '達' | '超') {
                chars.next();
            }
        }
        let rest: String = chars.collect();
        if rest.is_empty() {
            return false;
        }
        // 單字「多」、「數」或「點」若單獨出現多為形容詞/動詞/名詞，但「十多」、「二十多」、「數十」、「數百」、「三點五」為合法數詞
        if matches!(rest.as_str(), "多" | "數" | "點" | ".") {
            return false;
        }
        rest.chars().all(|c| {
            matches!(
                c,
                '零' | '〇' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九'
                    | '十' | '百' | '千' | '萬' | '億' | '兆' | '兩' | '半' | '幾' | '數'
                    | '多' | '廿' | '卅' | '.' | '點'
            )
        })
    }

    pub(crate) fn matches(&self, word: &str, function_words: &HashSet<String>) -> bool {
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
                (word.chars().count() >= 2
                    && !function_words.contains(word)
                    && !is_rare_impostor(word)
                    && !word.ends_with('了')
                    && !word.ends_with('過')
                    && !word.ends_with('著')
                    && word.chars().count() <= 4)
                    || matches!(word, "我" | "你" | "妳" | "他" | "她" | "它" | "牠" | "祂" | "我們" | "你們" | "他們" | "人" | "書" | "車" | "貓" | "狗" | "爸" | "媽" | "兄" | "弟" | "姐" | "妹")
            }
            WordPattern::Numeric => Self::is_numeric_token(word),
            WordPattern::Any => !word.is_empty(),
        }
    }
}

/// 階層式三元集合槽位構式：[Slot0] → [Slot1] → [Slot2]
#[derive(Debug, Clone)]
pub struct TrigramSlotPattern {
    pub slot0: HashSet<String>,
    pub slot1: HashSet<String>,
    pub slot2: HashSet<String>,
    pub bonus: f64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TransitionEngine {
    /// Layer 1: 語法規則（通配模式）
    pub grammar_rules: Vec<GrammarRule>,
    /// 內建語法規則數量（用於 patterns_user.jsonl 熱重載時保留內建規則、截斷外掛規則）
    pub builtin_grammar_rule_count: usize,
    /// Layer 1 雙向倒排索引（加速 40~80 倍，杜絕長句 Trellis 計算卡頓）
    /// 索引前驅為 Exact / OneOf 的規則：prev_word -> [rule_index, ...]
    pub rules_by_prev: HashMap<String, Vec<usize>>,
    /// 索引前驅為通配符、但後繼為 Exact / OneOf 的規則：curr_word -> [rule_index, ...]
    pub rules_by_curr: HashMap<String, Vec<usize>>,
    /// 兩端皆為通配符的兜底規則（極罕見）：[rule_index, ...]
    pub wildcard_rules: Vec<usize>,
    /// 內建精確二元轉移雜湊表: (prev_word, curr_word) -> bonus_score (O(1) 結構化收斂，不會被 load_tsv 清空)
    pub builtin_transitions: HashMap<(String, String), f64>,
    /// Layer 2: 使用者自訂二元轉移快取: (prev_word, curr_word) -> bonus_score
    pub transitions: HashMap<(String, String), f64>,
    /// Layer 3: 離合動賓搭配群
    pub vo_pairs: Vec<VOCollocation>,
    /// 動態載入之三元階層槽位構式群 (來自 patterns_user.jsonl)
    pub user_trigram_patterns: Vec<TrigramSlotPattern>,
    /// 對衝抑制陣列集合 (Type C exclude_pairs)：於 O(1) 查找時直接攔截與真實詞同音對衝之二元陣列展開對
    pub excluded_pairs: HashSet<(String, String)>,
    /// 功能詞集合（用於 ContentWord 模式的排除判斷）
    pub function_words: HashSet<String>,
    /// 已停用之規則名稱或前綴集合（支援 `RUHI_DISABLE_RULES` 環境變數與 `ruhi_eval --ablation` 消融分析）
    pub disabled_rules: HashSet<String>,
    /// 線性模型特徵與規則族權重倍數表（載入自 `feature_weights.tsv`，預設為 1.0）
    pub feature_weights: HashMap<String, f64>,
    /// 最佳路徑／強制對齊路徑評估時之觸發追蹤緩衝區 `(rule_name, family_name, raw_score)`
    pub active_trace: Option<Vec<(String, String, f64)>>,
    pub file_path: Option<PathBuf>,
    pub last_modified: Option<SystemTime>,
    pub patterns_file_path: Option<PathBuf>,
    pub patterns_last_modified: Option<SystemTime>,
    pub last_check_instant: Option<std::time::Instant>,
}

impl Default for TransitionEngine {
    fn default() -> Self {
        let mut disabled_rules = HashSet::new();
        if let Ok(env_disabled) = std::env::var("RUHI_DISABLE_RULES") {
            for item in env_disabled.split([',', ';']) {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    disabled_rules.insert(trimmed.to_string());
                }
            }
        }

        let mut engine = Self {
            grammar_rules: Vec::new(),
            builtin_grammar_rule_count: 0,
            rules_by_prev: HashMap::new(),
            rules_by_curr: HashMap::new(),
            wildcard_rules: Vec::new(),
            builtin_transitions: HashMap::new(),
            transitions: HashMap::new(),
            vo_pairs: Vec::new(),
            user_trigram_patterns: Vec::new(),
            excluded_pairs: HashSet::new(),
            function_words: HashSet::new(),
            disabled_rules,
            feature_weights: HashMap::new(),
            active_trace: None,
            file_path: None,
            last_modified: None,
            patterns_file_path: None,
            patterns_last_modified: None,
            last_check_instant: None,
        };
        engine.init_builtin_knowledge();
        engine
    }
}

impl TransitionEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 初始化內建的漢語核心語法規則（模組化分層載入）
    fn init_builtin_knowledge(&mut self) {
        self.init_function_words();
        self.init_core_syntax_rules();
        self.init_collocation_and_anti_shatter_rules();
        self.init_construction_and_xiang_rules();
        self.init_builtin_vo_pairs();
        self.init_builtin_exact_bigrams();

        self.builtin_grammar_rule_count = self.grammar_rules.len();
        self.rebuild_rule_index();
    }

    /// 初始化封閉類功能詞集合（不參與 ContentWord 匹配）
    pub(crate) fn init_function_words(&mut self) {
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
            "這", "那", "這個", "那個", "什麼", "甚麼", "誰", "哪", "每",
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
    }

    /// 初始化 Layer 1 核心句法結構規則（數詞量詞、體標記、語氣助詞、結構助詞 的/地/得、處置/被動句、構詞模具）
    pub(crate) fn init_core_syntax_rules(&mut self) {
        // 0. 數詞與度量衡／曆法日期／時間／貨幣／計量單位構式 (Numeric-Unit Classifier Construction):
        //    [<NUM: 阿拉伯數字 / 小數 / 中文數詞>] ➔ [日期曆法 / 時間時段 / 貨幣金融 / 度量衡 / 通用計量單位]
        //    一體適用所有數值後接單位（如：2026年10月15日、100元、50坪、37度、500公克、8折、5倍），免除個別上下文單字追蹤。
        let numeric_unit_classifiers = vec![
            // (a) 曆法與日期單位 (Date & Calendar Units)
            "年", "月", "日", "號", "世紀", "年代", "週", "星期", "禮拜", "季", "旬", "載",
            "年度", "月份", "年份",
            // (b) 時間與時段單位 (Time & Duration Units)
            "秒", "秒鐘", "分", "分鐘", "時", "小時", "鐘頭", "點", "點鐘", "點半", "刻", "刻鐘",
            "天", "個月", "歲", "週歲", "週年", "周年", "輩子", "代", "世",
            // (c) 貨幣、金融與比例單位 (Currency, Finance & Ratio Units)
            "元", "塊", "塊錢", "角", "毛", "錢", "萬", "億", "兆", "萬元", "億元", "兆元",
            "台幣", "新台幣", "美元", "美金", "日圓", "日元", "歐元", "英鎊", "港幣", "人民幣",
            "韓元", "法郎", "盧比", "披索", "泰銖", "折", "成", "趴", "倍", "番",
            // (d) 度量衡單位：長度、重量、面積、體積、溫熱、電磁、物理、數位資訊 (Weights & Measures)
            "公尺", "米", "公分", "厘米", "公釐", "毫米", "微米", "奈米", "公里", "千米", "里",
            "海里", "浬", "尺", "寸", "丈", "英尺", "呎", "英寸", "吋", "碼", "光年",
            "公克", "克", "公斤", "千克", "公噸", "噸", "毫克", "微克", "斤", "台斤", "市斤",
            "兩", "磅", "盎司", "克拉",
            "坪", "公頃", "公畝", "甲", "平方公尺", "平方米", "平方公里", "平方公分", "平方英尺",
            "公升", "升", "毫升", "立方公尺", "立方米", "立方公分", "加侖", "品脫", "斗",
            "度", "攝氏", "華氏", "瓦", "千瓦", "瓩", "伏", "伏特", "安", "安培", "歐姆",
            "赫", "赫茲", "焦耳", "卡", "大卡", "卡路里", "分貝", "帕", "大氣壓", "馬力",
            "流明", "燭光", "轉", "幀", "位元", "位元組", "像素", "字", "字元",
            // (e) 通用個體、容器、篇章與集合計量單位 (General Classifiers)
            "行", "列", "頁", "篇", "章", "節", "段", "條", "款", "項", "筆", "欄", "格",
            "個", "位", "名", "人", "次", "回", "下", "遍", "趟", "場", "屆", "期", "聲",
            "級", "等", "層", "樓", "室", "房", "廳", "衛", "戶", "家", "所", "處", "棟",
            "座", "間", "台", "部", "輛", "架", "艘", "班", "支", "枝", "根", "張", "片",
            "顆", "粒", "滴", "杯", "碗", "盤", "皿", "瓶", "罐", "盒", "包", "袋", "箱",
            "籃", "桶", "本", "冊", "卷", "集", "首", "幅", "件", "套", "組", "批", "雙",
            "對", "副", "串", "束", "朵", "棵", "株", "隻", "頭", "匹", "尾", "群", "種",
            "類", "樣", "份",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Numeric,
            curr_pattern: WordPattern::OneOf(
                numeric_unit_classifiers.into_iter().map(|s| s.to_string()).collect(),
            ),
            bonus: 26.0,
            name: "numeric_unit_classifier".to_string(),
        });

        // 典型單音節及物/動作動詞 + 體標記或人稱代詞賓語（如：幫了、幫他、洗了、吃了、看了）
        // 置於通用 ContentWord 體標記之前，使真正具備動賓/體貌能力的動詞優先於同音黏著名詞語素（如「幫了」優於「邦了」）
        let core_action_verbs = vec![
            "幫", "洗", "睡", "吃", "喝", "看", "聽", "說", "讀", "寫", "走", "跑", "跳", "買", "賣",
            "做", "弄", "搞", "打", "拿", "帶", "送", "找", "問", "教", "救", "追", "推",
            "拉", "抱", "穿", "脫", "換", "搬", "寄", "付", "借", "欠", "騙", "罵", "誇", "陪",
        ];
        let verb_post_elements = vec![
            "了", "著", "他", "她", "它", "我", "你", "妳", "我們", "你們", "他們",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(core_action_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(verb_post_elements.into_iter().map(|s| s.to_string()).collect()),
            bonus: 18.0,
            name: "action_verb_aspect_or_pronoun".to_string(),
        });

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

        // 句末疑問/語氣助詞 (Sentence-final particles):
        // 注意：「對、是、會、能、可、要、該、在、有、沒、不」等位於 function_words 中，若僅對 ContentWord 加分，
        // 會導致句末附加問句「，對嗎？ / ，是嗎？ / ，會嗎？」被排除，反而讓同音名詞「隊嗎 / 試嗎 / 惠嗎」獲得 +24.0 非預期搶佔！
        let sentence_particles = vec![
            "嗎", "吧", "呢", "啊", "啦", "嘛", "呀", "哇", "囉", "喔", "唷", "耶",
        ];
        let tag_question_predicates = vec![
            "對", "是", "會", "能", "可以", "可", "要", "該", "在", "有", "沒", "沒有", "不",
            "好", "行", "真", "真的", "對不對", "是不是", "好不好", "行不行", "可不可以",
            "會不會", "能不能", "要不要", "該不該",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(tag_question_predicates.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(sentence_particles.iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "affirmative_or_modal_sentence_final_particle".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("隊".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "嗎".to_string(), "吧".to_string(), "呢".to_string(), "呀".to_string(), "喔".to_string(), "啦".to_string(),
            ]),
            bonus: -30.0,
            name: "anti_dui4_team_before_tag_particle".to_string(),
        });
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

        // 抑制孤立姓氏「郝」（非郝柏村等專名或「姓郝/老郝/小郝」，防「好」被姓氏劫掠）
        let surname_prefixes = vec!["姓", "老", "小"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(surname_prefixes.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("郝".to_string()),
            bonus: -35.0,
            name: "anti_isolated_surname_hao3".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "".to_string(), "的".to_string(), "得".to_string(), "地".to_string(),
                "了".to_string(), "很".to_string(), "太".to_string(), "最".to_string(),
                "更".to_string(), "不".to_string(), "沒".to_string(), "多".to_string(),
                "多麼".to_string(), "那麼".to_string(), "這麼".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("郝".to_string()),
            bonus: -35.0,
            name: "anti_isolated_surname_hao3_after_func".to_string(),
        });

        // 抑制孤立單字「眉」（非眉毛、眉頭、眉目、蛾眉等複合詞，防句首或句中「沒」被劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Any,
            curr_pattern: WordPattern::Exact("眉".to_string()),
            bonus: -30.0,
            name: "anti_isolated_mei2_eyebrow".to_string(),
        });

        // 抑制非數詞修飾之孤立「棟」（非一棟、那棟等量詞結構，防「動」被劫掠）
        let quant_num = vec!["一", "兩", "二", "三", "四", "五", "六", "七", "八", "九", "十", "幾", "這", "那", "哪", "每", "整"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(quant_num.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("棟".to_string()),
            bonus: -35.0,
            name: "anti_isolated_dong4_classifier_impostor".to_string(),
        });

        // 分配代詞「每」修飾量詞、時間詞或數詞構式 (每個、每次、每天、每年、每當、每逢、每一、每兩...)
        // 嚴格限定後接合法量詞/時間詞/數詞，防止「每」無差別加分一般名詞/形容詞而把「美好/美感/美化」切碎為「每郝/每感/每畫」
        let mei3_valid_targets = vec![
            "個", "次", "回", "天", "日", "年", "月", "週", "晚", "夜", "早", "晨",
            "人", "位", "名", "家", "戶", "件", "種", "樣", "類", "條", "項", "筆", "頁", "行",
            "段", "章", "節", "篇", "集", "本", "冊", "部", "台", "輛", "班", "站", "場", "屆",
            "期", "局", "輪", "趟", "遍", "下", "步", "處", "棟", "座", "間", "層", "樓",
            "碗", "杯", "盤", "瓶", "包", "盒", "袋", "箱", "雙", "對", "套", "組", "批",
            "朵", "顆", "粒", "滴", "隻", "頭", "匹", "棵", "株", "支", "根", "張", "片",
            "塊", "元", "分", "秒", "時", "小時", "分鐘", "星期", "禮拜", "學期", "階段", "世代",
            "當", "逢", "遇", "到", "隔", "過", "一", "兩", "二", "三", "四", "五", "六", "七", "八", "九", "十", "百", "千",
            "告一段落",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("每".to_string()),
            curr_pattern: WordPattern::OneOf(mei3_valid_targets.into_iter().map(|s| s.to_string()).collect()),
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

        // 準分裂句焦點標記構式: 實詞短語 + 的是 (選舉選的是、他要的是、我說的是、反應的是)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWord,
            curr_pattern: WordPattern::Exact("的是".to_string()),
            bonus: 26.0,
            name: "content_de_shi_copula".to_string(),
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

        // 結構助詞與繫詞引力: 的 ➔ 是 (必須置於通用 的 + 實詞 之前！)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("是".to_string()),
            bonus: 26.0,
            name: "de_shi_copula".to_string(),
        });

        // 抑制結構助詞「的」後誤接單字動詞「試」（必須置於通用 的 + 實詞 之前，杜絕「選的試一個」搶佔「選的是一個」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("試".to_string()),
            bonus: -35.0,
            name: "anti_de_shi4_try".to_string(),
        });

        // 「的」後接實詞: 的 + 實詞（的貓、的背心、的人）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 10.0,
            name: "de_noun".to_string(),
        });

        // ══════════════════════════════════════════════════════════════════
        // 🌟 漢語構詞模具引力網：舉一反三的高產偏正構式矩陣 (Schema Matrix)
        // ══════════════════════════════════════════════════════════════════

        // 1. 介面互動與軟體元件構式 (UI & Interaction Components)
        // 前綴(動作/功能) + 後綴(載體/容器/控制項): 搜尋框、編輯列、導覽欄、對話窗、設定頁...
        let ui_action_prefixes = vec![
            "輸入", "輸出", "搜尋", "尋找", "選取", "選擇", "編輯", "瀏覽",
            "顯示", "對話", "確認", "設定", "播放", "導覽", "提示", "預覽",
            "切換", "下載", "上傳", "控制", "排版", "組字", "候選",
        ];
        let ui_carrier_suffixes = vec![
            "框", "欄", "列", "行", "窗", "區", "頁", "座", "鈕", "鍵",
            "條", "表", "幕", "板", "層", "台", "埠", "端", "項", "值",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(ui_action_prefixes.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(ui_carrier_suffixes.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "ui_interaction_schema".to_string(),
        });

        // 2. 資料、運算與系統架構構式 (Data, Compute & System Structures)
        // 實體名詞 + 形態/結構/容器後綴: 資料流、封包段、參數組、快取區、字串值、狀態碼...
        let data_entity_prefixes = vec![
            "資料", "數據", "字串", "字元", "位元", "封包", "參數", "快取",
            "記憶體", "網路", "訊息", "指令", "位址", "節點", "金鑰", "權杖",
            "狀態", "權限", "程式碼", "程式", "模組", "封裝",
        ];
        let data_carrier_suffixes = vec![
            "庫", "流", "表", "集", "組", "段", "區", "值", "源", "點",
            "鏈", "列", "陣", "碼", "量", "階", "體", "樹", "圖", "檔",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(data_entity_prefixes.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(data_carrier_suffixes.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "data_structure_schema".to_string(),
        });

        // 3. 度量、屬性與統計指標構式 (Metrics & Attributes)
        // 度量維度 + 量化邊界後綴: 品質指標、效能比、頻率線、容量值、權重數、門檻值...
        let metric_dimension_prefixes = vec![
            "品質", "效能", "容量", "頻率", "速度", "成本", "風險", "價值",
            "壓力", "溫度", "時間", "空間", "比例", "極限", "權重", "精度",
            "維度", "門檻", "負載", "耗損", "頻寬",
        ];
        let metric_carrier_suffixes = vec![
            "值", "率", "比", "量", "度", "數", "階", "級", "線", "點",
            "幅", "界", "額", "差", "項", "標",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(metric_dimension_prefixes.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(metric_carrier_suffixes.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "metric_attribute_schema".to_string(),
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

        // 結構助詞「得」: 單音節動詞/形容詞或特定雙音節謂詞 + 得（補語標記）
        let valid_de2_stems = vec![
            "看", "聽", "讀", "寫", "說", "講", "走", "跑", "跳", "飛", "做", "弄", "搞", "打",
            "吃", "喝", "睡", "穿", "買", "賣", "學", "教", "唱", "畫", "洗", "擦", "修", "改",
            "算", "記", "認", "猜", "想", "找", "拿", "搬", "追", "逃", "忍", "受", "忘", "合",
            "配", "顧", "分", "談", "比", "長", "變", "顯", "過", "活", "玩", "笑", "哭", "鬧",
            "忙", "累", "氣", "急", "嚇", "樂", "美", "帥", "快", "慢", "高", "低", "好", "壞",
            "難", "男", "女", "台", "神", "文青", "收羅", "整理", "表現", "準備", "處理", "解釋",
            "說明", "形容", "描述", "控制", "掌握", "發揮", "維持", "保持", "保護", "照顧", "打扮",
            "收拾", "打掃", "安排", "設計", "規劃", "計算", "分析", "研究", "觀察", "配合", "發展",
        ].into_iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(valid_de2_stems),
            curr_pattern: WordPattern::Exact("得".to_string()),
            bonus: 10.0,
            name: "complement_de".to_string(),
        });

        // 趨向補語: 單音節動作/位移動詞 + {來/去/回/進/出/上/下/開/住}
        let directional = vec![
            "來", "去", "回", "進", "出", "上", "下", "開", "住",
        ];
        let directional_verb_stems = vec![
            "走", "跑", "跳", "飛", "爬", "滾", "衝", "闖", "搬", "拿", "帶", "送", "寄", "拉",
            "推", "拖", "抬", "舉", "放", "丟", "扔", "掉", "落", "跌", "站", "坐", "躺", "趴",
            "蹲", "看", "望", "想", "說", "講", "喊", "叫", "哭", "笑", "吃", "喝", "吞", "吐",
            "咬", "穿", "脫", "戴", "掛", "貼", "寫", "畫", "記", "算", "查", "找", "買", "賣",
            "租", "借", "還", "開", "關", "鎖", "綁", "抓", "捉", "握", "抱", "扶", "靠", "頂",
            "撐", "擋", "蓋", "遮", "藏", "躲", "逃", "追", "趕", "引", "領", "導", "生", "長",
            "活", "醒", "睡", "進", "出", "回", "過", "上", "下", "起",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(directional_verb_stems.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(directional.iter().map(|s| s.to_string()).collect()),
            bonus: 10.0,
            name: "directional_complement".to_string(),
        });

        // 把字句與被字句 (Disposal & Passive): 把/被 + 名詞性成分(含代詞)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("把".to_string()),
            curr_pattern: WordPattern::Nominal,
            bonus: 20.0,
            name: "ba_construction".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("被".to_string()),
            curr_pattern: WordPattern::Nominal,
            bonus: 20.0,
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
    }

    /// 初始化動量、時相、心理動詞搭配與防碎拆阻斷規則 (Collocations & Anti-Shatter Rules)
    pub(crate) fn init_collocation_and_anti_shatter_rules(&mut self) {
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

        // 副詞「另」+ 動作/狀態動詞 (另開、另起、另外、另有、另行、另闢、另謀、另建議、另加、另增、另設、另列、另附)
        let ling_action_verbs = vec![
            "開", "起", "外", "有", "行", "闢", "謀", "議", "訂", "尋",
            "建議", "加", "增", "設", "列", "附", "見",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("另".to_string()),
            curr_pattern: WordPattern::OneOf(ling_action_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "ling4_action_verb".to_string(),
        });

        // 杜絕「令」誤接「開/起/外/有」 (防「另開」被指令動詞「令」劫掠)
        let ling3_forbidden = vec!["開", "起", "外", "闢", "謀", "建議"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("令".to_string()),
            curr_pattern: WordPattern::OneOf(ling3_forbidden.into_iter().map(|s| s.to_string()).collect()),
            bonus: -40.0,
            name: "anti_ling_action".to_string(),
        });

        // 完成時相副詞「已」+ 狀態/動詞 (已安全、已完成、已到達、已結束、已啟用、已停用、已成功、已離線)
        let yi3_verbs = vec!["安全", "完成", "到達", "結束", "啟用", "停用", "成功", "離線", "關閉", "確認", "讀取", "送達", "完工"];
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

        // 祈使/情態/先後條件詞後接重複副詞「再」（如：請再清查、請再確認、還要再、明天再、下次再）
        let pre_zai4_modals = vec![
            "請", "煩請", "麻煩", "還要", "想要", "歡迎", "務必", "不妨", "然後", "接著", "之後", "以後",
            "重新", "回頭", "順便", "另外", "同時", "立刻", "馬上", "趕快",
            "趕緊", "明天", "改天", "下次", "下回", "稍後", "等一下", "待會", "晚點", "過後", "完",
            "完了", "完畢", "好了", "別", "不要", "不用", "不必", "不需要",
            "分類", "項目", "清單", "列表", "選單", "欄位", "問題", "何在",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pre_zai4_modals.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("再".to_string()),
            bonus: 28.0,
            name: "modal_imperative_zai4_again".to_string(),
        });

        // 主語代詞與進行體/時間狀態副詞後接進行體標記「在」（如：我在想、你在看、他在做、還在想、正在討論、到底在忙什麼）
        let pre_zai4_progressive_subjects = vec![
            "我", "你", "妳", "他", "她", "它", "我們", "你們", "他們", "大家", "別人", "有人", "誰",
            "正", "正在", "還", "都", "一直", "總", "總是", "依然", "仍然", "剛才", "剛剛", "當時",
            "現在", "目前", "到底", "究竟", "其實", "只是",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pre_zai4_progressive_subjects.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("在".to_string()),
            bonus: 24.0,
            name: "subject_progressive_zai4_at".to_string(),
        });

        // 固有重複/嘗試/擴充/決策類謂詞接於「再」之後（再來、再去、再試、再清查、再決定、再試試、再看看、再想想、再增加、再新增、再改用）
        let zai4_inherent_iterative_verbs = vec![
            "來", "去", "試", "清查", "決定", "試試", "看看", "想想", "問問", "聊聊", "等等",
            "增加", "新增", "加入", "擴充", "補充", "改用", "重試",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("再".to_string()),
            curr_pattern: WordPattern::OneOf(zai4_inherent_iterative_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 32.0,
            name: "zai4_action_verb".to_string(),
        });

        // 雙向共容之一般動作與認知動詞（想、看、聽、說、做、查、問、找、寫、討論、研究、處理...）：
        // 賦予「在 ➔ V」與「再 ➔ V」完全對稱之基礎語法銜接分 (+20.0)，由前驅語法槽位（主語進行體 vs 助動/祈使/先後）與後繼動量補語決定「在 vs 再」！
        let dual_aspect_zai4_verbs = vec![
            "想", "思考", "考慮", "尋思", "琢磨", "懷疑", "猶豫", "擔心", "煩惱", "觀察", "研究", "分析",
            "看", "聽", "說", "講", "問", "做", "作", "弄", "搞", "忙", "等", "等待", "找", "尋找",
            "玩", "笑", "哭", "睡", "吃", "喝", "讀", "寫", "學", "學習", "討論", "商量", "開會",
            "工作", "運動", "測試", "檢查", "確認", "檢視", "執行", "運作", "運行", "處理", "發展",
            "改", "修", "加", "補", "設定", "調整", "修改", "切換", "開", "買", "查", "談", "議", "辦", "打",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["在".to_string(), "再".to_string()]),
            curr_pattern: WordPattern::OneOf(dual_aspect_zai4_verbs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 20.0,
            name: "dual_aspect_zai_verb_base".to_string(),
        });

        // 程度讓步副詞「再」+ 可分級單字形容詞（再好、再大、再高、再多、再快、再難、再遠、再久、再強、再貴、再忙、再累、再怎麼）
        let zai4_degree_adjs = vec![
            "好", "大", "小", "高", "低", "多", "少", "快", "慢", "難", "遠", "近", "久", "強", "貴", "忙", "累", "苦", "痛", "怎麼",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("再".to_string()),
            curr_pattern: WordPattern::OneOf(zai4_degree_adjs.iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "zai4_degree_concession_adj".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("在".to_string()),
            curr_pattern: WordPattern::OneOf(zai4_degree_adjs.into_iter().map(|s| s.to_string()).collect()),
            bonus: -28.0,
            name: "anti_zai4_prep_before_degree_adj".to_string(),
        });

        // 杜絕介詞「在」接「來/來做/來看/試試/看看/想想/決定」劫掠副詞「再」 (防「在來」劫掠「再來」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("在".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "來".to_string(), "試試".to_string(), "看看".to_string(), "想想".to_string(),
                "決定".to_string(), "增加".to_string(), "新增".to_string(), "擴充".to_string(),
                "改用".to_string(), "清查".to_string(),
            ]),
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

        // ── 存現動詞 ➔ 不定量標記「些」(還有些、有些、剩下些、帶有些、留有些；後接實詞由 constructions.rs 三元驗證前驅含「有/這/那/哪/一/好」後觸發) ──
        let existential_verbs = vec!["有", "還有", "剩下", "帶有", "存有", "保留", "具備"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(existential_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("些".to_string()),
            bonus: 26.0,
            name: "existential_verb_xie".to_string(),
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

        // ── 動作動詞 ➔ 完成體補語「完畢」(處理完畢、準備完畢、確認完畢、測試完畢、安裝完畢、下載完畢、執行完畢、審查完畢、校對完畢、調校完畢) ──
        let wanbi_verbs = vec![
            "處理", "準備", "確認", "測試", "點交", "審查", "檢查", "安裝", "下載", "執行",
            "盤點", "清點", "核對", "校對", "設定", "填寫", "繳納", "回覆", "更新", "修復",
            "搬遷", "整修", "交接", "點收", "製作", "編譯", "建置", "打包", "排版", "列印",
            "調校", "校準", "訓練", "部署", "同步", "轉換", "匯入", "匯出", "載入", "註冊",
            "升級", "釋出", "開發", "調整", "修改",
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

        // ── 情態語氣副詞「真的/實在/確實/的確」➔ 存在/判斷/情態動詞（真的有、真的沒有、真的是、真的會、真的要；必須置於 de_ending_compound_noun 之前！）──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "真的".to_string(), "實在".to_string(), "確實".to_string(), "的確".to_string(), "還真".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "有".to_string(), "沒".to_string(), "沒有".to_string(), "是".to_string(), "不是".to_string(),
                "會".to_string(), "不會".to_string(), "能".to_string(), "不能".to_string(), "可以".to_string(),
                "要".to_string(), "不要".to_string(), "想".to_string(), "很".to_string(), "太".to_string(), "好".to_string(),
            ]),
            bonus: 28.0,
            name: "modal_adverb_zhende_before_verb_or_modal".to_string(),
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

        // ── 數量詞/指示量詞複合詞 + 實詞 / 程度副詞修飾結構 ──
        // 僅針對無同音實詞衝突之「數詞/指示詞+個/種」複合詞與單字「隻」，後接雙字以上複合實詞給予引力，防止單字 ContentWord 拆碎雙字名詞
        let degree_advs = vec!["很", "非常", "特別", "十分", "太", "真", "挺", "極為", "超", "多", "多麼"];
        let safe_num_cls_compounds: Vec<String> = vec![
            "一個", "兩個", "三個", "這個", "那個", "哪個", "每個", "某個", "幾個", "半個",
            "一種", "這種", "那種", "哪種", "隻",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
        // 消融實測（dev 集）：「數詞/指示詞+個/種 → 任意雙字實詞」通配加分僅改善 4 句、損害 70 句（同音雙字詞被錯誤吸附），
        // 故移除通配引力，僅保留無歧義之「隻 → 貓」與「數量詞 → 程度副詞」搭配。
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("隻".to_string()),
            curr_pattern: WordPattern::Exact("貓".to_string()),
            bonus: 18.0,
            name: "classifier_noun".to_string(),
        });

        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(safe_num_cls_compounds),
            curr_pattern: WordPattern::OneOf(degree_advs.iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "classifier_degree_adv".to_string(),
        });

        // ── 感嘆/疑問程度副詞「多」+ 核心狀態形容詞 (有多厲害、有多神奇、多好、多大) ──
        let duo_gradable_adjs = vec![
            "厲害", "神奇", "好", "大", "高", "快", "遠", "久", "難", "美", "棒", "讚", "強", "深", "重", "貴", "美妙", "精彩", "漂亮", "誇張", "離譜", "嚴重", "辛苦", "開心", "高興",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("多".to_string()),
            curr_pattern: WordPattern::OneOf(duo_gradable_adjs.into_iter().map(|s| s.to_string()).collect()),
            bonus: 24.0,
            name: "duo_degree_gradable_adj".to_string(),
        });

        // ── 專職程度副詞 + 實詞謂詞/形容詞 (很普通、很好、很大；排除單字「多/真/挺/超」無條件匹配一般名詞) ──
        let strict_degree_advs = vec!["很", "非常", "特別", "十分", "太", "極為", "多麼"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(strict_degree_advs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 14.0,
            name: "degree_adv_content_adj".to_string(),
        });

        // 否定: 不/沒 + 實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["不".to_string(), "沒".to_string(), "沒有".to_string()]),
            curr_pattern: WordPattern::ContentWord,
            bonus: 8.0,
            name: "negation".to_string(),
        });

        // 繫詞「是 / 的是」➔ 不定數量名結構（是一個、是一種、是一件、是一場、是一份...；必須置於通用 是 + 實詞 之前！）
        let indefinite_num_classifiers = vec![
            "一個", "一種", "一件", "一位", "一項", "一場", "一段", "一套", "一部", "一座",
            "一門", "一雙", "一對", "一筆", "一份", "一條", "一張", "一本", "一隻", "一顆",
            "一粒", "一棟", "一間", "一輛", "一台", "一架", "一班", "一支", "一片", "一杯",
            "一碗", "一盤", "一瓶", "一盒", "一包", "一袋", "一箱", "一束", "一朵", "一棵",
            "一株", "一頭", "一匹", "一群", "一類", "一樣", "這", "那", "這個", "那個",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["是".to_string(), "的是".to_string()]),
            curr_pattern: WordPattern::OneOf(indefinite_num_classifiers.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "copula_shi_indefinite_np".to_string(),
        });

        // 判斷: 是 + 實詞
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("是".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 8.0,
            name: "shi_predicate".to_string(),
        });

        // 疑問/方式副詞 ➔ 情態助動詞/否定/核心副詞/判斷詞: [怎麼, 怎樣, 如何, 怎, 何以, 為什麼, 為甚麼, 為何, 難道, 到底, 究竟] ➔ [會, 能, 可以, 要, 應該, 該, 得, 不, 沒, 沒有, 還, 都, 也, 就, 才, 又, 再, 在, 是, 有, 這麼, 那麼, 如此]
        // （必須置於 interrogative_adv_action 之前，因為「會、能、要、不、沒、是、有」等位於 function_words 中，若僅對 ContentWord 加分，會導致「為什麼 + ㄏㄨㄟˋ」將功能詞「會」排除、反而對同音實詞「惠」加 20 分造成「為什麼惠」的非預期搶佔！）
        let interrogative_advs_modal = vec![
            "怎麼", "怎樣", "如何", "怎", "何以", "為什麼", "為甚麼", "為何", "難道", "到底", "究竟",
        ];
        let modal_or_func_after_interrogative = vec![
            "會", "能", "能夠", "可以", "可", "要", "想", "應該", "該", "得", "敢", "肯", "願", "願意",
            "不", "沒", "沒有", "不會", "不能", "不要", "不用", "不必", "不是", "還", "都", "也", "就",
            "才", "又", "再", "在", "是", "有", "這麼", "那麼", "如此", "這樣", "那樣",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(interrogative_advs_modal.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(modal_or_func_after_interrogative.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "interrogative_adv_modal_or_neg".to_string(),
        });

        // 疑問/方式副詞 ➔ 動詞/實詞: [怎麼, 怎樣, 如何, 怎, 何以, 為什麼, 為甚麼] ➔ 動詞/實詞 (怎麼做、怎麼辦、怎麼設計、怎麼看、怎麼想、如何處理)
        let interrogative_advs = vec![
            "怎麼", "怎樣", "如何", "怎", "何以", "為什麼", "為甚麼"
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(interrogative_advs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::ContentWordExcept(vec![
                "惠".to_string(), "匯".to_string(), "繪".to_string(), "慧".to_string(), "賄".to_string(), "晦".to_string(), "穢".to_string(), "".to_string(),
            ]),
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

        // 抑制偽詞切分「本來藥 / 好像藥」（嚴防破壞「本來要 / 好像要」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "本來".to_string(),
                "好像".to_string(),
                "似乎".to_string(),
                "原本".to_string(),
                "原來".to_string(),
            ]),
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

        // 否定副詞 ➔ 成語形容詞 (不/沒/不再/我不/他不/她不 ➔ 金光閃閃)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "不".to_string(), "沒".to_string(), "不再".to_string(),
                "我不".to_string(), "你".to_string(), "他不".to_string(), "她不".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("金光閃閃".to_string()),
            bonus: 36.0,
            name: "negation_jinguang_shanshan".to_string(),
        });
        // 人稱代詞/介詞代詞 ➔ 否定狀態詞「不好 / 不對 / 不錯 / 不行」（如：對他不好的人，防「部好」同音劫掠）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "我".to_string(), "你".to_string(), "妳".to_string(), "他".to_string(), "她".to_string(),
                "對他".to_string(), "對我".to_string(), "對你".to_string(), "對她".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "不好".to_string(), "不對".to_string(), "不錯".to_string(), "不行".to_string(),
            ]),
            bonus: 26.0,
            name: "pronoun_negation_adjective".to_string(),
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

        // 語氣時間副詞 ➔ 助動詞 (本來/原本/原來/差點/偏偏/剛好/好像/似乎 ➔ 要/想/會/能/打算/應該)
        let modal_adverbs = vec![
            "本來", "原本", "原來", "差點", "偏偏", "剛好", "幸好", "早就", "好像", "似乎",
        ];
        let target_modals = vec!["要", "想", "會", "能", "打算", "應該", "可以"];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(modal_adverbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(target_modals.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "adverb_temporal_modal".to_string(),
        });

        // 助動詞「要」➔ 常用動作與氣象動詞 (要 ➔ 說/做/去/走/看/買/吃/聽/寫/下雨...)
        let yao_actions = vec![
            "說", "做", "去", "走", "看", "買", "吃", "聽", "寫", "來", "帶", "問", "辦", "下雨", "下雪", "下", "開始", "出發",
        ];
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

        // ── 敬語/致謝動詞 ➔ 動賓客體/反饋名詞 (感謝/多謝/謝謝/萬分感謝/非常感謝 ➔ 回覆/回復/協助/支持/指導/解釋/提醒/分享/幫忙/配合/賜教/建議/耐心/告知/實作/開發/修正/調校) ──
        let gratitude_verbs = vec!["感謝", "多謝", "謝謝", "萬分感謝", "非常感謝", "十分感謝"];
        let gratitude_objects = vec![
            "回覆", "回復", "協助", "支持", "指導", "解釋", "提醒", "分享", "幫忙", "配合",
            "賜教", "建議", "解答", "耐心", "告知", "包容", "體諒", "關照", "照顧", "招待",
            "反饋", "回饋", "提供", "指教", "實作", "開發", "修正", "修復", "調整", "調校",
            "更新", "升級", "設計", "測試", "維護", "貢獻", "參與", "付出",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(gratitude_verbs.clone().into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(gratitude_objects.into_iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "gratitude_verb_object".to_string(),
        });
        // 抑制敬語/致謝與工程動作動詞後誤接數詞「十」再接量詞「座」劫掠「實作 (ㄕˊ ㄗㄨㄛˋ)」
        let shizuo_left_verbs = vec![
            "感謝", "多謝", "謝謝", "萬分感謝", "非常感謝", "十分感謝",
            "完成", "開始", "進行", "負責", "協助", "實際", "具體", "完整",
            "系統", "程式", "程式碼", "軟體", "功能", "介面", "演算法", "架構", "模組", "引擎",
            "已經", "已", "並", "及", "與", "的", "直接", "重新", "自行", "手動", "自動",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(shizuo_left_verbs.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("實作".to_string()),
            bonus: 28.0,
            name: "verb_or_modifier_before_shizuo".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(gratitude_verbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("十".to_string()),
            bonus: -28.0,
            name: "anti_gratitude_verb_before_numeral_shi_shatter_shizuo".to_string(),
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
    }

    /// 初始化方位詞構式、關聯副詞、補語構式與「象/相/像」消歧義規則 (Constructions & Xiang Disambiguation)
    pub(crate) fn init_construction_and_xiang_rules(&mut self) {
        let classifiers = vec![
            "個", "位", "隻", "條", "把", "次", "回", "種", "件", "份", "句",
            "本", "張", "塊", "間", "棟", "輛", "台", "架", "支", "枝",
            "朵", "匹", "頭", "尾", "顆", "粒", "片", "杯", "碗", "盤",
            "部", "座", "所", "家", "款", "項", "套", "組", "段", "幢",
            "艘", "幅", "首", "尊", "面", "扇", "門", "臺", "冊",
            "堂", "列", "班", "口",
        ];

        // 1. 方位後置詞構式: 實詞(非方位詞與時間名詞「時」「實」本身) + {裡/外/內/中/上/下/前/後/旁/之中/裡面/外面/中間/底下/身上/手裡/心裡/心中}
        let locatives = vec![
            "裡", "外", "內", "中", "上", "下", "前", "後", "旁",
            "之中", "裡面", "外面", "中間", "底下", "身上", "手裡", "心裡", "心中",
        ];
        let non_locative_stems = vec![
            "裡", "外", "內", "中", "上", "下", "前", "後", "旁", "時", "實", "全",
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

        // 3. 繫詞與分裂焦點構式: 的 + 是 (奇妙的是、重要的是、選的是、說的是、看的是、要的是、指的是)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("是".to_string()),
            bonus: 26.0,
            name: "de_shi_copula".to_string(),
        });
        let cleft_de_prefixes = vec![
            "選的", "說的", "看的", "想的", "要的", "做的", "講的", "問的", "求的", "談的",
            "買的", "吃的", "玩的", "穿的", "開的", "學的", "教的", "考的", "比的", "拼的",
            "靠的", "憑的", "缺的", "怕的", "愛的", "指的", "聽的", "寫的", "用的", "找的",
            "投的", "盼的", "爭的", "護的", "守的", "救的",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(cleft_de_prefixes.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("是".to_string()),
            bonus: 28.0,
            name: "cleft_focus_de_shi_copula".to_string(),
        });
        // 抑制結構助詞「的」後誤接單字動詞「試」（杜絕「選的試一個」劫掠「選的是一個」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("的".to_string()),
            curr_pattern: WordPattern::Exact("試".to_string()),
            bonus: -35.0,
            name: "anti_de_shi4_try".to_string(),
        });
        // 繫詞「是 / 的是」➔ 不定數量名結構（是一個、是一種、是一件、是一場、是一份...）
        let indefinite_num_classifiers = vec![
            "一個", "一種", "一件", "一位", "一項", "一場", "一段", "一套", "一部", "一座",
            "一門", "一雙", "一對", "一筆", "一份", "一條", "一張", "一本", "一隻", "一顆",
            "一粒", "一棟", "一間", "一輛", "一台", "一架", "一班", "一支", "一片", "一杯",
            "一碗", "一盤", "一瓶", "一盒", "一包", "一袋", "一箱", "一束", "一朵", "一棵",
            "一株", "一頭", "一匹", "一群", "一類", "一樣", "這", "那", "這個", "那個",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["是".to_string(), "的是".to_string()]),
            curr_pattern: WordPattern::OneOf(indefinite_num_classifiers.into_iter().map(|s| s.to_string()).collect()),
            bonus: 24.0,
            name: "copula_shi_indefinite_np".to_string(),
        });

        // 3b. 介詞短語作定語構式：[數量詞/指示詞] ➔ 對/對於/關於/針對 ➔ [複合實詞]（如：一個對治理方式的盼望、一種對未來的期許）
        let np_determiners_before_prep = vec![
            "一個", "一種", "一份", "一場", "一段", "一項", "這個", "那個", "這種", "那種",
            "個", "種", "份", "些", "這些", "那些",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(np_determiners_before_prep.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(vec![
                "對".to_string(), "對於".to_string(), "關於".to_string(), "針對".to_string(),
            ]),
            bonus: 24.0,
            name: "determiner_preposition_modifier".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "對".to_string(), "對於".to_string(), "關於".to_string(), "針對".to_string(),
            ]),
            curr_pattern: WordPattern::CompoundContentWord,
            bonus: 24.0,
            name: "preposition_dui_compound_object".to_string(),
        });
        // 抑制量詞「個／一個」直接誤接單字名詞「隊」（漢語說「一個隊伍／一個團隊／一支隊伍」，杜絕「*一個隊治理」劫掠「一個對治理」）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["個".to_string(), "一個".to_string()]),
            curr_pattern: WordPattern::Exact("隊".to_string()),
            bonus: -35.0,
            name: "anti_ge_bare_dui4_team".to_string(),
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

        // 9. 可能補語與程度狀態補語構式 [ 動詞 + 得/不 + 補語 ] (看得懂/看不懂、做得到/做不到、收羅得十分齊全、說得很清楚)
        let potential_complements = vec![
            "懂", "見", "清", "到", "起", "下", "住", "著", "完", "好",
            "開", "來", "去", "過", "飽", "慣", "動", "走", "跑", "通",
            "出", "上", "成", "了", "掉", "進", "回", "準", "穩",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("得".to_string()),
            curr_pattern: WordPattern::OneOf(potential_complements.iter().map(|s| s.to_string()).collect()),
            bonus: 18.0,
            name: "de_potential_result".to_string(),
        });
        // 程度補語與狀態補語構式：[ 動詞 + 得 + 專屬多字狀態補語 ]（如：收羅得十分齊全、講得頭頭是道；一般程度副詞交由 constructions.rs 三元構式七驗證前驅為合法動詞後觸發）
        let de2_degree_and_state_complements = vec![
            "十分", "齊全", "透徹", "過癮", "痛快", "不可開交", "團團轉", "頭頭是道", "有聲有色", "淋漓盡致",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("得".to_string()),
            curr_pattern: WordPattern::OneOf(de2_degree_and_state_complements.into_iter().map(|s| s.to_string()).collect()),
            bonus: 24.0,
            name: "de2_degree_or_state_complement".to_string(),
        });
        // 副詞後接否定詞構式：[ 又/也/都/還/卻/就/才/偏偏/竟然/居然/明明 + 不/沒/沒有/不是/不會... ]（如：現在好像又不厲害了、也不要、還沒有）
        let pre_negation_adverbs = vec![
            "又", "也", "都", "還", "卻", "就", "才", "偏偏", "竟然", "居然", "明明",
        ];
        let negation_words = vec![
            "不", "沒", "沒有", "不是", "不會", "不能", "不要", "不用", "不必", "不可", "不再", "不太", "不夠", "不如", "不曾", "未必", "未曾",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pre_negation_adverbs.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(negation_words.into_iter().map(|s| s.to_string()).collect()),
            bonus: 16.0,
            name: "adverb_before_negation".to_string(),
        });
        // 情態/時間詞後接重複副詞「又」（如：現在好像又不厲害了、怎麼又、居然又）
        let modal_temporal_before_you4 = vec![
            "好像", "似乎", "現在", "如今", "今天", "昨天", "最近", "剛才", "剛剛", "後來", "最後",
            "怎麼", "為何", "為什麼", "居然", "竟然", "明明", "偏偏", "果然", "終究", "結果", "反倒", "反而",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(modal_temporal_before_you4.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("又".to_string()),
            bonus: 26.0,
            name: "modal_temporal_you4".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::ContentWordExcept(vec!["右".to_string(), "左".to_string(), "幼".to_string(), "釉".to_string(), "柚".to_string()]),
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

        // 10. [ X 來 X 去 ] 動作與空間往復構式 (走來走去、看來看去、想來想去) 已移至 constructions.rs 四元對稱構式驗證，避免單字「來+實詞」無條件拆詞。

        // 11. [ 大 X 特 X ] 動量擴散構式 (大書特書、大搞特搞、大買特買；後兩項由 constructions.rs 驗證前驅含「大」且前後動作 X 同一後觸發，避免無條件「實詞+特+實詞」拆碎「一種特殊」)
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("大".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 10.0,
            name: "da_content".to_string(),
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

        // 5. 交互動詞結構: 相 ➔ 多字動詞補語 (相結合、相提並論、相敬如賓、相輔相成；雙字詞如相處/相信/相對已由主詞庫承載，避免單字「相+單字」無條件拆詞)
        let mutuality_verbs = vec![
            "結合".to_string(), "提並論".to_string(), "敬如賓".to_string(), "輔相成".to_string(),
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

        // ── 並列連詞構式: [名詞/人稱代詞/實詞] ➔ 和 (台灣口語 ㄏㄢˋ) ➔ [名詞/人稱代詞/實詞] ──
        // （讀音為 ㄏㄜˊ 時極易與「配合/符合/結合/合作/合理/核心/如何/任何」等雙字詞首尾同音碰撞，故 ㄏㄜˊ 交由 constructions.rs 三元並列構式在無跨界雙字詞時觸發）
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Nominal,
            curr_pattern: WordPattern::Exact("和".to_string()),
            bonus: 24.0,
            name: "nominal_conjunction_he".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("和".to_string()),
            curr_pattern: WordPattern::Nominal,
            bonus: 22.0,
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

        // ── 動詞「查」與時量/動量/結果補語構式（防「查半天/查一下/都要查」被名詞「茶」搶佔）──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "要".to_string(), "都要".to_string(), "還要".to_string(), "先".to_string(),
                "再".to_string(), "去".to_string(), "來".to_string(), "去".to_string(),
                "檢".to_string(), "調".to_string(), "清".to_string(), "追".to_string(),
                "反".to_string(), "備".to_string(), "搜".to_string(), "探".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("查".to_string()),
            bonus: 26.0,
            name: "modal_or_prefix_cha2_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("查".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "半天".to_string(), "一下".to_string(), "一陣子".to_string(), "好久".to_string(),
                "很久".to_string(), "清楚".to_string(), "出來".to_string(), "到".to_string(),
                "看".to_string(), "閱".to_string(), "詢".to_string(), "證".to_string(),
                "明".to_string(), "出".to_string(), "完".to_string(), "過".to_string(), "了".to_string(),
            ]),
            bonus: 28.0,
            name: "cha2_verb_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("茶".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "半天".to_string(), "一下".to_string(), "清楚".to_string(), "出來".to_string(),
                "到".to_string(), "完".to_string(), "過".to_string(),
            ]),
            bonus: -35.0,
            name: "anti_tea_before_verbal_complement".to_string(),
        });

        // ── 否定副詞「沒」後接動詞構式（防「沒幫上什麼忙」句首「沒」被「眉」搶佔）──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("沒".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "幫".to_string(), "幫上".to_string(), "幫到".to_string(), "幫忙".to_string(),
                "想".to_string(), "想到".to_string(), "看".to_string(), "看到".to_string(),
                "聽".to_string(), "聽到".to_string(), "找".to_string(), "找到".to_string(),
                "說".to_string(), "做".to_string(), "寫".to_string(), "打".to_string(),
                "吃".to_string(), "喝".to_string(), "睡".to_string(), "去".to_string(),
                "來".to_string(), "拿".to_string(), "帶".to_string(), "買".to_string(),
            ]),
            bonus: 26.0,
            name: "mei2_negation_action_verb".to_string(),
        });

        // ── 趨向補語後接「秀」與動量「一下」構式（如：跑出來秀一下，防被古詞「繡衣」搶佔）──
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "出來".to_string(), "起來".to_string(), "上來".to_string(), "過來".to_string(), "來".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec!["秀".to_string(), "秀一下".to_string()]),
            bonus: 26.0,
            name: "directional_xiu4_show".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("秀".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["一下".to_string(), "出".to_string(), "出來".to_string(), "給".to_string()]),
            bonus: 28.0,
            name: "xiu4_show_complement".to_string(),
        });

        // ══════════════════════════════════════════════════════════════════
        // ── 八大同音字組與「與／是要／但會不會對」全語境雙向句法分流規則 ──
        // ══════════════════════════════════════════════════════════════════

        // 1. 「但 (轉折連詞) vs 淡 (狀態形容詞) vs 旦 (古詞/時間詞素)」雙向分流：
        let dan4_clause_followers = vec![
            "會", "不會", "會不會", "要", "不要", "要不要", "能", "不能", "能不能",
            "是", "不是", "是不是", "可", "可以", "可能", "應該", "必須", "得",
            "還", "還是", "卻", "也", "都", "只", "仍", "仍然", "依舊", "依然",
            "又", "更", "最", "很", "太", "真", "實", "其實", "事實上", "實際上",
            "如果", "若", "要是", "假如", "倘若", "當", "在", "從", "對", "對於",
            "關於", "至於", "因為", "由於", "雖然", "儘管", "即使", "就算", "無論",
            "不管", "只要", "只有", "一旦", "萬一", "最後", "最終", "後來", "現在",
            "目前", "如今", "今天", "有時", "往往", "通常", "一般", "並", "不", "沒", "沒有",
            "我", "你", "您", "他", "她", "它", "我們", "你們", "他們", "大家", "人", "人們",
            "這", "那", "這個", "那個", "這些", "那些", "這項", "那項", "此", "其",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("但".to_string()),
            curr_pattern: WordPattern::OneOf(dan4_clause_followers.iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "adversative_dan4_clause_lead".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["旦".to_string(), "淡".to_string(), "彈".to_string(), "氮".to_string()]),
            curr_pattern: WordPattern::OneOf(dan4_clause_followers.into_iter().map(|s| s.to_string()).collect()),
            bonus: -28.0,
            name: "anti_impostor_dan4_clause_lead".to_string(),
        });
        let dan4_adj_modifiers = vec![
            "很", "太", "更", "最", "真", "超", "挺", "蠻", "頗", "極", "十分", "非常",
            "相當", "特別", "格外", "有點", "稍微", "略", "偏", "過於", "越來越",
            "看", "變", "沖", "調", "清", "平", "冷", "暗", "慘", "寡", "鹹", "濃",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(dan4_adj_modifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("淡".to_string()),
            bonus: 28.0,
            name: "degree_or_verb_dan4_light_taste".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(dan4_adj_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(vec!["但".to_string(), "旦".to_string()]),
            bonus: -32.0,
            name: "anti_conjunction_dan4_after_degree".to_string(),
        });

        // 2. 「是要 (焦點助動複合詞) vs 試藥」與「但會不會對 vs 匯兌」語法錨定：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("是".to_string()),
            curr_pattern: WordPattern::Exact("要".to_string()),
            bonus: 24.0,
            name: "copula_modal_shi_yao".to_string(),
        });
        let shiyao_valid_followers = vec![
            "請", "請您", "請你", "讓", "把", "將", "對", "對於", "跟", "與", "和", "向",
            "給", "替", "為", "用", "以", "從", "在", "去", "來", "做", "看", "說", "問",
            "講", "寫", "查", "找", "買", "吃", "喝", "玩", "學", "教", "幫", "救", "處",
            "處理", "分析", "討論", "解決", "研究", "檢查", "確認", "測試", "怎", "怎麼", "如何",
            "我", "你", "您", "他", "她", "我們", "你們", "他們", "大家",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("是要".to_string()),
            curr_pattern: WordPattern::OneOf(shiyao_valid_followers.iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "shiyao_focus_modal_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("是要".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 18.0,
            name: "shiyao_content_word".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("試藥".to_string()),
            curr_pattern: WordPattern::OneOf(shiyao_valid_followers.into_iter().map(|s| s.to_string()).collect()),
            bonus: -38.0,
            name: "anti_shiyao_drug_trial_before_clause".to_string(),
        });

        let modal_prep_leads = vec![
            "會", "不會", "會不會", "能", "不能", "能不能", "要", "不要", "要不要",
            "是", "不是", "是不是", "是要", "應該", "不該", "可能", "可以", "不可",
            "總會", "常會", "更常", "很常", "往往", "容易", "難免", "勢必", "必將",
        ];
        let prep_targets = vec![
            "對", "對於", "向", "往", "把", "將", "被", "讓", "給", "跟", "與", "和", "為", "替", "請",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(modal_prep_leads.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(prep_targets.into_iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "modal_auxiliary_preposition_bridge".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "對".to_string(), "對於".to_string(),
            ]),
            curr_pattern: WordPattern::Nominal,
            bonus: 18.0,
            name: "preposition_dui4_nominal_object".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "不".to_string(), "會不".to_string(), "不會".to_string(), "但".to_string(), "這".to_string(), "那".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("匯兌".to_string()),
            bonus: -38.0,
            name: "anti_huidui_forex_impostor".to_string(),
        });

        // 3. 「與 (連詞/介詞) vs 雨 (氣象名詞) vs 語 (語言詞素)」句法結構分流：
        let yu3_prep_leads = vec![
            "請", "請您", "請你", "並", "且", "並且", "而且", "更", "常與", "多", "先",
            "能", "要", "會", "應該", "必須", "可以", "將", "把", "對", "對於", "不", "未",
            "曾", "已", "正", "正在", "樂於", "善於", "勇於", "敢於", "羞於", "屑於",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(yu3_prep_leads.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("與".to_string()),
            bonus: 20.0,
            name: "adverb_or_modal_preposition_yu3".to_string(),
        });
        let yu3_np_determiners = vec![
            "這", "這個", "這項", "這類", "這些", "這份", "這套", "這條", "這段", "這種",
            "那", "那個", "那項", "那類", "那些", "那份", "那套", "那條", "那段", "那種",
            "此", "其", "其他", "其餘", "各", "各個", "各種", "各類", "每", "每個", "某",
            "我", "你", "您", "他", "她", "它", "我們", "你們", "他們", "大家", "自己", "他人", "別人",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("與".to_string()),
            curr_pattern: WordPattern::OneOf(yu3_np_determiners.iter().map(|s| s.to_string()).collect()),
            bonus: 24.0,
            name: "yu3_conjunction_or_prep_determiner".to_string(),
        });
        let yu3_coord_nominals = vec![
            "實踐", "實務", "理論", "應用", "分析", "設計", "開發", "測試", "架構", "標準",
            "規範", "制度", "政策", "方法", "技術", "科學", "藝術", "文化", "歷史", "社會",
            "自然", "環境", "生活", "工作", "學習", "家庭", "朋友", "同事", "團隊", "客戶",
            "使用者", "系統", "硬體", "軟體", "資料", "檔案", "詞庫", "語意", "語法", "規則",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("與".to_string()),
            curr_pattern: WordPattern::OneOf(yu3_coord_nominals.into_iter().map(|s| s.to_string()).collect()),
            bonus: 22.0,
            name: "yu3_conjunction_nominal".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["雨".to_string(), "語".to_string(), "羽".to_string(), "宇".to_string()]),
            curr_pattern: WordPattern::OneOf(yu3_np_determiners.into_iter().map(|s| s.to_string()).collect()),
            bonus: -28.0,
            name: "anti_rain_or_lang_before_determiner".to_string(),
        });
        let weather_rain_prefixes = vec![
            "下", "淋", "冒", "避", "躲", "防", "擋", "遮", "暴", "豪", "陣", "酸", "梅", "春", "秋", "雷", "風", "被",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(weather_rain_prefixes.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("雨".to_string()),
            bonus: 28.0,
            name: "weather_verb_or_modifier_rain".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(weather_rain_prefixes.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(vec!["與".to_string(), "語".to_string(), "羽".to_string()]),
            bonus: -28.0,
            name: "anti_yu3_conjunction_after_weather".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("雨".to_string()),
            curr_pattern: WordPattern::OneOf(vec!["淋".to_string(), "淋濕".to_string(), "淋透".to_string(), "沖".to_string(), "打".to_string()]),
            bonus: 28.0,
            name: "rain_weather_action".to_string(),
        });

        // 4. 「不在 (否定存在動詞/介詞) vs 不再 (否定重複時制副詞)」雙向分流：
        let buzai_locative_objects = vec![
            "家", "家裡", "現場", "場", "這", "這裡", "這邊", "那", "那裡", "那邊", "此",
            "線", "線上", "乎", "意", "少數", "話下", "名單", "範圍", "討論", "考量",
            "服務", "位置", "座位", "辦公室", "公司", "學校", "教室", "房間", "房裡",
            "車上", "身上", "手邊", "身邊", "眼裡", "心上", "國內", "國外", "台灣", "台北",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不在".to_string()),
            curr_pattern: WordPattern::OneOf(buzai_locative_objects.iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "buzai_locative_existence".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不再".to_string()),
            curr_pattern: WordPattern::OneOf(buzai_locative_objects.into_iter().map(|s| s.to_string()).collect()),
            bonus: -26.0,
            name: "anti_buzai_adverb_before_locative".to_string(),
        });
        let buzai_action_predicates = vec![
            "是", "有", "去", "來", "說", "做", "看", "想", "問", "哭", "笑", "等", "等待",
            "使用", "出現", "發生", "重蹈", "聯絡", "見面", "猶豫", "懷疑", "堅持", "依賴",
            "更新", "支援", "提供", "採用", "受限", "造成", "浪費", "迷惘", "害怕", "孤單",
            "難過", "生氣", "悲傷", "擔心", "害怕", "沉默", "妥協", "退縮", "回頭", "理會",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不再".to_string()),
            curr_pattern: WordPattern::OneOf(buzai_action_predicates.iter().map(|s| s.to_string()).collect()),
            bonus: 26.0,
            name: "buzai_cessative_adverb_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不在".to_string()),
            curr_pattern: WordPattern::OneOf(buzai_action_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: -24.0,
            name: "anti_buzai_locative_before_predicate".to_string(),
        });

        // 5. 「檔 (資訊檔案/檔次) vs 黨 (政黨組織)」雙向分流：
        let file_dang4_modifiers = vec![
            "存", "讀", "開", "關", "建", "刪", "轉", "傳", "寫", "存成", "轉成", "開啟",
            "讀取", "儲存", "刪除", "備份", "設定", "文字", "圖片", "音訊", "視訊", "影片",
            "音樂", "聲音", "專案", "原始", "執行", "壓縮", "日誌", "紀錄", "測試", "範例",
            "字典", "詞庫", "模型", "權重", "文", "圖", "音", "來源", "目標", "輸出", "輸入",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(file_dang4_modifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("檔".to_string()),
            bonus: 28.0,
            name: "file_operation_or_type_dang4".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(file_dang4_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("黨".to_string()),
            bonus: -30.0,
            name: "anti_party_dang3_after_file_modifier".to_string(),
        });
        let political_dang3_modifiers = vec![
            "政", "執政", "在野", "反對", "最大", "多數", "少數", "民主", "共和", "國民",
            "民進", "共產", "社會", "勞工", "自由", "保守", "綠", "藍", "白", "橘", "紅",
            "入", "退", "創", "組", "建", "全", "各", "跨", "本", "貴", "同", "死", "政客",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(political_dang3_modifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("黨".to_string()),
            bonus: 28.0,
            name: "political_party_dang3".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(political_dang3_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("檔".to_string()),
            bonus: -30.0,
            name: "anti_file_dang4_after_political_modifier".to_string(),
        });
        let zaiye_political_heads = vec![
            "政黨", "黨派", "黨團", "陣營", "聯盟", "人士", "立委", "議員", "勢力", "時", "期間",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("在野".to_string()),
            curr_pattern: WordPattern::OneOf(zaiye_political_heads.iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "zaiye_before_political_nominal".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("再也".to_string()),
            curr_pattern: WordPattern::OneOf(zaiye_political_heads.into_iter().map(|s| s.to_string()).collect()),
            bonus: -35.0,
            name: "anti_zaiye_npi_before_political_nominal".to_string(),
        });

        // 6. 「詞 (語言詞彙/詩詞) vs 辭 (辭職/推辭/修辭)」雙向分流：
        let ci2_lexical_modifiers = vec![
            "新", "舊", "單", "複", "雙字", "三字", "四字", "長", "短", "名", "動", "形容",
            "副", "介", "連", "助", "嘆", "量", "代", "數", "實", "虛", "同音", "多音",
            "破音", "近義", "同義", "反義", "生僻", "冷僻", "罕見", "常用", "高頻", "低頻",
            "關鍵", "敏感", "專有", "專業", "外來", "方言", "主", "謂", "賓", "定", "狀",
            "補", "語", "字", "組", "造", "選", "拆", "分", "斷", "歌", "詩", "宋", "唐",
            "填", "台", "臺", "講", "說", "微", "一", "個", "這", "那", "每個", "這個", "那個",
            "類", "延伸", "衍生", "同類", "各類", "這類", "那類", "假",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(ci2_lexical_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("詞".to_string()),
            bonus: 26.0,
            name: "lexical_or_poetic_ci2".to_string(),
        });
        let ci2_lexical_heads = vec![
            "庫", "典", "彙", "語", "義", "意", "性", "類", "組", "條", "頻", "根", "綴",
            "尾", "頭", "素", "格", "序", "句", "曲", "牌", "人", "章", "藻", "鋒",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("詞".to_string()),
            curr_pattern: WordPattern::OneOf(ci2_lexical_heads.iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "ci2_lexical_compound_head".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("辭".to_string()),
            curr_pattern: WordPattern::OneOf(ci2_lexical_heads.into_iter().map(|s| s.to_string()).collect()),
            bonus: -28.0,
            name: "anti_resign_ci2_before_lexical_head".to_string(),
        });
        let ci2_resign_complements = vec![
            "職", "去", "呈", "行", "別", "世", "退", "官", "謝", "工", "掉", "了",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("辭".to_string()),
            curr_pattern: WordPattern::OneOf(ci2_resign_complements.iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "resign_or_depart_ci2_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("詞".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "職".to_string(), "呈".to_string(), "別".to_string(), "世".to_string(), "官".to_string(),
            ]),
            bonus: -30.0,
            name: "anti_lexical_ci2_before_resign_complement".to_string(),
        });

        // 7. 指示量詞複合詞 (`這個/那個/每個/哪個/幾個/一個/兩個`) ➔ 「字」vs「自」消歧：
        let det_ge_modifiers = vec![
            "個", "這個", "那個", "每個", "哪個", "幾個", "一個", "兩個", "三個", "半個", "整", "整個",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(det_ge_modifiers.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("字".to_string()),
            bonus: 28.0,
            name: "det_classifier_ge_zi4_character".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(det_ge_modifiers.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("自".to_string()),
            bonus: -32.0,
            name: "anti_det_classifier_ge_zi4_self".to_string(),
        });

        // 8. 施事主語「醫生 (ㄧ ㄕㄥ)」vs 動量短語「一聲」句法位置消歧：
        let doctor_agent_predicates = vec![
            "把", "將", "對", "跟", "向", "幫", "替", "為", "給", "說", "看", "問", "講",
            "建議", "表示", "指出", "發現", "認為", "檢查", "診斷", "治療", "開藥", "搶救", "救",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("醫生".to_string()),
            curr_pattern: WordPattern::OneOf(doctor_agent_predicates.iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "agent_subject_doctor_yisheng".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("一聲".to_string()),
            curr_pattern: WordPattern::OneOf(doctor_agent_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: -32.0,
            name: "anti_verbal_classifier_yisheng_as_agent".to_string(),
        });

        // 9. 指示代詞／公文限定詞 (`此/本/該/全/各/專/草/提/報/備/結/定`) ➔ 「案」vs「岸/暗/按」消歧：
        let case_an4_determiners = vec![
            "此", "本", "該", "全", "各", "這", "那", "專", "草", "提", "報", "備", "結", "定", "破",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(case_an4_determiners.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("案".to_string()),
            bonus: 26.0,
            name: "determiner_case_an4".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("此岸".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "無關".to_string(), "有關".to_string(), "沒有".to_string(), "處理".to_string(), "進度".to_string(),
            ]),
            bonus: -35.0,
            name: "anti_buddhist_cian4_in_case_context".to_string(),
        });

        // 10. 總括範圍副詞「都 (ㄉㄡ)」vs 名詞/動詞「兜」與完成動補結構「都完工了」消歧：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("兜".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "完".to_string(), "完工".to_string(), "完成".to_string(), "完了".to_string(),
                "了".to_string(), "過".to_string(), "好".to_string(),
            ]),
            bonus: -35.0,
            name: "anti_dou1_pocket_before_aspect".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "完".to_string(), "都".to_string(), "已".to_string(), "已經".to_string(),
                "快".to_string(), "剛".to_string(), "終於".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("攻了".to_string()),
            bonus: -45.0,
            name: "anti_wan_gongle_impostor".to_string(),
        });
        let dou1_completion_predicates = vec![
            "完工", "完成", "完畢", "完了", "完", "做好", "弄好", "修好", "寫好", "準備好",
            "結束", "好了", "可以", "行", "有", "沒有", "沒", "是", "在", "會", "要", "能",
            "很", "太", "已經", "還",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("都".to_string()),
            curr_pattern: WordPattern::OneOf(dou1_completion_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "scope_adv_dou1_completion_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("都".to_string()),
            curr_pattern: WordPattern::CompoundContentWord,
            bonus: 16.0,
            name: "scope_adv_dou1_compound_content".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "完工".to_string(), "完成".to_string(), "完畢".to_string(), "結束".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("了".to_string()),
            bonus: 26.0,
            name: "completion_compound_aspect_le".to_string(),
        });

        // 11. 飲食分類、成分排除與全稱量化構式（另建議素食分類再增加瑜伽素項目：不含蛋與所有菇類）：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "建議".to_string(), "另建議".to_string(), "新增".to_string(), "提供".to_string(), "選擇".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "素食".to_string(), "分類".to_string(), "項目".to_string(), "選項".to_string(),
            ]),
            bonus: 28.0,
            name: "suggest_dietary_category".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "素食".to_string(), "飲食".to_string(), "餐點".to_string(), "菜單".to_string(), "商品".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "分類".to_string(), "項目".to_string(), "類別".to_string(), "選項".to_string(),
            ]),
            bonus: 28.0,
            name: "dietary_item_category_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "增加".to_string(), "新增".to_string(), "加入".to_string(), "增設".to_string(), "提供".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "瑜伽素".to_string(), "瑜伽".to_string(), "全素".to_string(), "蛋奶素".to_string(),
                "五辛素".to_string(), "純素".to_string(), "素食".to_string(),
            ]),
            bonus: 30.0,
            name: "add_dietary_type_object".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "瑜伽素".to_string(), "全素".to_string(), "蛋奶素".to_string(), "五辛素".to_string(), "純素".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "項目".to_string(), "分類".to_string(), "餐點".to_string(), "選項".to_string(), "料理".to_string(),
            ]),
            bonus: 28.0,
            name: "dietary_type_item_head".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "不含".to_string(), "包含".to_string(), "不吃".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec!["但".to_string(), "旦".to_string()]),
            bonus: -45.0,
            name: "anti_dietary_exclusion_dan4".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "不含".to_string(), "包含".to_string(), "含".to_string(), "無".to_string(), "沒有".to_string(), "不吃".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "蛋".to_string(), "奶".to_string(), "蛋奶".to_string(), "五辛".to_string(),
                "蔥".to_string(), "蒜".to_string(), "肉".to_string(), "菇類".to_string(),
            ]),
            bonus: 30.0,
            name: "dietary_exclusion_ingredient".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "所有".to_string(), "各類".to_string(), "各種".to_string(), "任何".to_string(), "一切".to_string(), "全部".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "菇類".to_string(), "肉類".to_string(), "素食".to_string(), "項目".to_string(),
                "食材".to_string(), "蔬菜".to_string(), "豆類".to_string(),
            ]),
            bonus: 28.0,
            name: "universal_quantifier_category_noun".to_string(),
        });

        // 12. 輸入法學習與時間方位因果構式（其實我在學大易輸入法之前因為手寫書信的關係）：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "學".to_string(), "學習".to_string(), "用".to_string(), "使用".to_string(),
                "打".to_string(), "練".to_string(), "練習".to_string(), "改用".to_string(), "切換".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("大意".to_string()),
            bonus: -40.0,
            name: "anti_learn_dayi_careless".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "學".to_string(), "學習".to_string(), "用".to_string(), "使用".to_string(),
                "打".to_string(), "練".to_string(), "練習".to_string(), "改用".to_string(),
                "切換".to_string(), "熟悉".to_string(), "忘記".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "大易".to_string(), "倉頡".to_string(), "行列".to_string(), "注音".to_string(),
                "許氏".to_string(), "嘸蝦米".to_string(), "無蝦米".to_string(), "速成".to_string(),
                "拼音".to_string(), "大千".to_string(), "倚天".to_string(), "羅馬".to_string(),
            ]),
            bonus: 30.0,
            name: "learn_use_ime_name".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "大易".to_string(), "倉頡".to_string(), "行列".to_string(), "注音".to_string(),
                "許氏".to_string(), "嘸蝦米".to_string(), "無蝦米".to_string(), "速成".to_string(),
                "拼音".to_string(), "大千".to_string(), "倚天".to_string(), "羅馬".to_string(),
                "智慧型".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "輸入法".to_string(), "鍵盤".to_string(), "字根".to_string(),
                "碼表".to_string(), "拆碼".to_string(), "規則".to_string(),
            ]),
            bonus: 28.0,
            name: "ime_name_shurufa_head".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "輸入法".to_string(), "鍵盤".to_string(), "大易".to_string(), "倉頡".to_string(),
                "行列".to_string(), "許氏".to_string(), "嘸蝦米".to_string(), "無蝦米".to_string(),
                "注音".to_string(), "工作".to_string(), "學習".to_string(), "離開".to_string(),
                "開始".to_string(), "上班".to_string(), "畢業".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "之前".to_string(), "之後".to_string(), "以前".to_string(), "以後".to_string(),
                "之初".to_string(), "之餘".to_string(),
            ]),
            bonus: 28.0,
            name: "event_noun_temporal_localizer".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "之前".to_string(), "之後".to_string(), "以前".to_string(), "以後".to_string(),
                "當時".to_string(), "後來".to_string(), "其實".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "因為".to_string(), "由於".to_string(), "所以".to_string(), "都是".to_string(), "還是".to_string(),
            ]),
            bonus: 28.0,
            name: "temporal_localizer_causal_conj".to_string(),
        });

        // 13. 認知思考型態與對比否定構式（我的思考基本上比較偏向文字型思考而非語音思考）：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "比較".to_string(), "較".to_string(), "更".to_string(), "相當".to_string(),
                "非常".to_string(), "明顯".to_string(), "總是".to_string(), "往往".to_string(), "基本上".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "偏向".to_string(), "傾向".to_string(), "偏好".to_string(), "擅長".to_string(), "習慣".to_string(),
            ]),
            bonus: 28.0,
            name: "degree_adv_tendency_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "偏向".to_string(), "傾向".to_string(), "偏好".to_string(), "屬於".to_string(), "採用".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "文字型".to_string(), "語音型".to_string(), "視覺型".to_string(), "圖像型".to_string(),
                "邏輯型".to_string(), "文字".to_string(), "語音".to_string(), "視覺".to_string(), "圖像".to_string(),
                "聽覺".to_string(), "邏輯".to_string(), "直覺".to_string(), "分析".to_string(), "記憶".to_string(),
                "思考".to_string(), "概念".to_string(), "經驗".to_string(),
            ]),
            bonus: 30.0,
            name: "tendency_verb_cognitive_style".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "偏向".to_string(), "傾向".to_string(), "偏好".to_string(), "屬於".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("紋".to_string()),
            bonus: -40.0,
            name: "anti_tendency_verb_wen".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("紋".to_string()),
            curr_pattern: WordPattern::Exact("自行".to_string()),
            bonus: -45.0,
            name: "anti_wen_zixing".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "文字".to_string(), "語音".to_string(), "視覺".to_string(), "聽覺".to_string(),
                "圖像".to_string(), "邏輯".to_string(), "直覺".to_string(), "分析".to_string(),
                "記憶".to_string(), "思考".to_string(), "概念".to_string(), "經驗".to_string(),
                "智慧".to_string(), "學習".to_string(), "觸覺".to_string(), "空間".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("型".to_string()),
            bonus: 32.0,
            name: "cognitive_root_type_suffix_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "型".to_string(), "文字型".to_string(), "語音型".to_string(), "視覺型".to_string(),
                "圖像型".to_string(), "邏輯型".to_string(), "文字".to_string(), "語音".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "思考".to_string(), "思維".to_string(), "記憶".to_string(), "學習".to_string(),
                "模式".to_string(), "輸入法".to_string(), "人物".to_string(), "人才".to_string(),
            ]),
            bonus: 30.0,
            name: "cognitive_style_head_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "思考".to_string(), "思維".to_string(), "方式".to_string(), "模式".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "而非".to_string(), "而不是".to_string(),
            ]),
            bonus: 28.0,
            name: "cognitive_noun_rather_than".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "而非".to_string(), "而不是".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "語音".to_string(), "語音型".to_string(), "文字".to_string(), "文字型".to_string(),
                "視覺".to_string(), "單純".to_string(),
            ]),
            bonus: 30.0,
            name: "rather_than_contrast_target".to_string(),
        });

        // 14. 派生後綴陣列構式：[飲食類別詞根 × {素}] 與 [食材生物詞根 × {類}]
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "增加".to_string(), "新增".to_string(), "加入".to_string(), "擴充".to_string(),
                "增設".to_string(), "補充".to_string(), "提供".to_string(), "吃".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "瑜伽".to_string(), "五辛".to_string(), "蛋奶".to_string(), "植物".to_string(),
                "全".to_string(), "純".to_string(), "奶".to_string(), "蛋".to_string(), "鍋邊".to_string(),
            ]),
            bonus: 30.0,
            name: "add_verb_dietary_root_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "瑜伽".to_string(), "五辛".to_string(), "蛋奶".to_string(), "植物".to_string(),
                "全".to_string(), "純".to_string(), "奶".to_string(), "蛋".to_string(),
                "鍋邊".to_string(), "方便".to_string(), "早".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("素".to_string()),
            bonus: 32.0,
            name: "dietary_root_su_suffix_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("素".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "項目".to_string(), "選項".to_string(), "餐點".to_string(), "飲食".to_string(),
                "分類".to_string(), "料理".to_string(), "者".to_string(),
            ]),
            bonus: 30.0,
            name: "su_suffix_item_head_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "所有".to_string(), "各類".to_string(), "各種".to_string(), "任何".to_string(),
                "一切".to_string(), "全部".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "菇".to_string(), "蛋".to_string(), "肉".to_string(), "豆".to_string(),
                "藻".to_string(), "貝".to_string(), "魚".to_string(), "菜".to_string(),
                "鳥".to_string(), "菌".to_string(), "穀".to_string(), "蔬".to_string(),
                "果".to_string(), "奶".to_string(), "蝦".to_string(), "蟹".to_string(),
            ]),
            bonus: 28.0,
            name: "universal_quantifier_food_root_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "菇".to_string(), "蛋".to_string(), "肉".to_string(), "豆".to_string(),
                "藻".to_string(), "貝".to_string(), "魚".to_string(), "菜".to_string(),
                "鳥".to_string(), "菌".to_string(), "穀".to_string(), "蔬".to_string(),
                "果".to_string(), "奶".to_string(), "蝦".to_string(), "蟹".to_string(),
                "人".to_string(), "鳥".to_string(), "蟲".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("類".to_string()),
            bonus: 30.0,
            name: "food_bio_root_lei_suffix_array".to_string(),
        });

        // 15. 可能補語三維陣列構式：{核心動詞} × {得, 不} × {結果/趨向補語}（取代 lexicon_domain.tsv 120 筆平面化詞條）
        let pot_verbs: Vec<String> = [
            "看", "聽", "讀", "寫", "說", "算", "想", "記", "認", "猜", "弄", "搞", "分",
            "做", "受", "吃", "合", "忘", "拿", "睡", "買", "走", "追", "配", "顧", "找",
            "學", "用", "穿", "搬", "喝", "忍", "逃", "比", "猜", "算", "談",
            "分辨", "區分", "判斷", "解釋", "說明", "分析", "處理", "表達", "描述",
        ].iter().map(|s| s.to_string()).collect();
        let pot_results: Vec<String> = [
            "懂", "到", "見", "清", "起", "出", "完", "好", "下", "了", "飽", "來", "通",
            "動", "著", "住", "上", "過", "開", "掉", "得", "進", "回", "準", "穩",
        ].iter().map(|s| s.to_string()).collect();
        let pot_neg_compounds: Vec<String> = [
            "不懂", "不到", "不見", "不清", "不起", "不完", "不好", "不下", "不了", "不飽",
            "不來", "不通", "不動", "不著", "不住", "不上", "不過", "不開", "不掉", "不得",
        ].iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pot_verbs.clone()),
            curr_pattern: WordPattern::OneOf(vec!["得".to_string(), "不".to_string()]),
            bonus: 28.0,
            name: "potential_verb_de_bu_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["得".to_string(), "不".to_string()]),
            curr_pattern: WordPattern::OneOf(pot_results),
            bonus: 26.0,
            name: "potential_de_bu_result_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pot_verbs.clone()),
            curr_pattern: WordPattern::OneOf(pot_neg_compounds),
            bonus: 32.0,
            name: "potential_verb_neg_compound_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(pot_verbs),
            curr_pattern: WordPattern::OneOf(vec![
                "步道".to_string(), "佈道".to_string(), "布道".to_string(), "不道".to_string(),
            ]),
            bonus: -45.0,
            name: "anti_potential_verb_budao_impostor".to_string(),
        });

        // 16. 動量與離合詞中綴三維陣列構式：{核心動詞} × {一下, 個, 一頓, 一趟, 一個, 頓} × {離合賓語/修飾語}（取代 lexicon_domain.tsv 338 筆平面化詞條）
        let vo_momentum_verbs: Vec<String> = [
            "看", "聽", "讀", "寫", "說", "講", "聊", "問", "查", "試", "想", "等", "坐",
            "走", "跑", "跳", "吃", "喝", "睡", "洗", "刷", "泡", "理", "剪", "做", "煮",
            "抽", "流", "嘆", "開", "請", "幫", "見", "打", "握", "鞠", "生", "發", "吵",
            "道", "散", "唱", "游", "爬", "逛", "釣", "拍", "照", "出", "上", "下", "加",
            "罷", "辭", "考", "排", "住", "結", "買", "簽", "秀", "碰", "轉", "推", "拉",
            "改", "用", "測試",
        ].iter().map(|s| s.to_string()).collect();
        let vo_infixes: Vec<String> = [
            "一下", "個", "一頓", "一趟", "一個", "一場", "一番", "頓",
        ].iter().map(|s| s.to_string()).collect();
        let vo_objects: Vec<String> = [
            "澡", "覺", "飯", "廁所", "床", "牙", "臉", "髮", "夢", "汗", "氣", "煙", "菸",
            "酒", "茶", "天", "話", "玩笑", "客", "忙", "面", "招呼", "手", "躬", "脾氣",
            "架", "歉", "步", "歌", "舞", "泳", "球", "山", "街", "魚", "照", "相", "假",
            "差", "會", "卡", "班", "工", "職", "試", "隊", "病", "針", "藥", "院", "帳",
            "單", "名", "仗", "石頭", "時", "時的", "什麼",
        ].iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vo_momentum_verbs),
            curr_pattern: WordPattern::OneOf(vo_infixes.clone()),
            bonus: 28.0,
            name: "vo_verb_momentum_infix_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vo_infixes.clone()),
            curr_pattern: WordPattern::OneOf(vo_objects),
            bonus: 26.0,
            name: "vo_infix_object_head_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vo_infixes),
            curr_pattern: WordPattern::OneOf(vec![
                "好".to_string(), "熱水".to_string(), "痛快".to_string(), "大".to_string(),
                "飽".to_string(), "小".to_string(), "短".to_string(),
            ]),
            bonus: 26.0,
            name: "vo_infix_modifier_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "好".to_string(), "熱水".to_string(), "痛快".to_string(), "大".to_string(),
                "飽".to_string(), "小".to_string(), "短".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "澡".to_string(), "覺".to_string(), "飯".to_string(), "會".to_string(),
            ]),
            bonus: 28.0,
            name: "vo_modifier_object_head_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["個".to_string(), "一個".to_string()]),
            curr_pattern: WordPattern::Exact("好叫".to_string()),
            bonus: -45.0,
            name: "anti_ge_haojiao_impostor".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "跑出來".to_string(), "出來".to_string(), "出來秀".to_string(), "來".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("繡衣".to_string()),
            bonus: -45.0,
            name: "anti_xiuyi_impostor_after_chulai".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("繡衣".to_string()),
            curr_pattern: WordPattern::Exact("下".to_string()),
            bonus: -45.0,
            name: "anti_xiuyi_xia_impostor".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("一".to_string()),
            curr_pattern: WordPattern::Exact("下石".to_string()),
            bonus: -45.0,
            name: "anti_yi_xiashi_impostor".to_string(),
        });
        // 對衝抑制陣列（Suppression Array）：抵銷展開後與真實詞彙（分布、何不、是個、各部、各項、各式）同音對衝之二元組合
        for (p, n) in [
            ("分", "不"),
            ("分", "不起"),
            ("合", "不"),
            ("試", "個"),
            ("個", "步"),
            ("個", "相"),
            ("個", "試"),
        ] {
            self.excluded_pairs.insert((p.to_string(), n.to_string()));
        }

        // 17. 極量與飽足結果補語陣列構式：[V × ({到, 得}?) × {飽}]（涵蓋 V+到+飽、V+得+飽、V+飽）
        let satiation_v_dao_de_compounds: Vec<String> = [
            "看到", "吃到", "喝到", "聽到", "玩到", "睡到", "坐到", "唱到", "騎到", "用到",
            "讀到", "買到", "逛到", "泡到", "聊到", "笑到", "氣到", "學到", "寫到", "打到",
            "充到", "領到", "拿到", "爽到", "賺到",
            "看得", "吃得", "喝得", "聽得", "玩得", "睡得", "坐得", "唱得", "騎得", "用得",
            "讀得", "買得",
        ].iter().map(|s| s.to_string()).collect();
        let satiation_verbs_single: Vec<String> = [
            "看", "吃", "喝", "聽", "玩", "睡", "坐", "唱", "騎", "用", "讀", "買", "逛",
            "泡", "聊", "笑", "氣", "學", "寫", "打", "充", "領", "拿", "灌", "餵", "塞",
        ].iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(satiation_v_dao_de_compounds.clone()),
            curr_pattern: WordPattern::Exact("飽".to_string()),
            bonus: 34.0,
            name: "satiation_v_dao_de_bao_compound_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(satiation_v_dao_de_compounds),
            curr_pattern: WordPattern::OneOf(vec![
                "保".to_string(), "寶".to_string(), "堡".to_string(), "葆".to_string(),
            ]),
            bonus: -35.0,
            name: "anti_satiation_v_dao_de_bao_impostor".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(satiation_verbs_single.clone()),
            curr_pattern: WordPattern::OneOf(vec!["到".to_string(), "得".to_string()]),
            bonus: 26.0,
            name: "satiation_verb_dao_de_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(satiation_verbs_single),
            curr_pattern: WordPattern::Exact("飽".to_string()),
            bonus: 30.0,
            name: "satiation_verb_bao_direct_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("飽".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "了".to_string(), "沒".to_string(), "嗎".to_string(), "足".to_string(),
            ]),
            bonus: 28.0,
            name: "satiation_bao_aspect_particle_array".to_string(),
        });

        // 18. 同音詞彙對衝之「語法環境雙向分流 (Syntactic Slot Bifurcation)」規則組：
        //     當陣列展開對 (a, b) 與詞庫同音詞 W 皆為真實語句合法用法時（如「他馬騎得很好吧」vs.「才不是什麼耆德老人呢」），
        //     絕不單向盲目抑制，而是依據左向論元／限定語槽位與右向補語／中心名詞槽位進行雙向精準分流。

        // (18a) 「騎 + 得」vs.「耆德」雙向語境分流：
        let mount_vehicles: Vec<String> = [
            "馬", "車", "單車", "腳踏車", "機車", "摩托車", "鐵馬", "重機", "駿馬", "白馬",
        ].iter().map(|s| s.to_string()).collect();
        let qide_determiners: Vec<String> = [
            "什麼", "不是什麼", "並非", "一代", "鄉里", "堪稱", "尊為", "這位", "那位",
        ].iter().map(|s| s.to_string()).collect();
        let qide_elder_nouns: Vec<String> = [
            "老人", "長者", "老者", "老人家", "宿儒", "碩望", "賢達", "前輩", "耆老", "風範",
        ].iter().map(|s| s.to_string()).collect();
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(mount_vehicles.clone()),
            curr_pattern: WordPattern::Exact("騎".to_string()),
            bonus: 30.0,
            name: "bifurcation_mount_patient_qi2_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(mount_vehicles),
            curr_pattern: WordPattern::OneOf(vec!["耆德".to_string(), "其".to_string(), "奇".to_string(), "棋".to_string()]),
            bonus: -40.0,
            name: "anti_archaic_qide_after_mount_vehicle".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("耆德".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "很".to_string(), "很好".to_string(), "很快".to_string(), "很穩".to_string(),
                "非常".to_string(), "太".to_string(), "真".to_string(), "超".to_string(),
                "好".to_string(), "快".to_string(), "穩".to_string(), "順".to_string(),
                "飽".to_string(), "不錯".to_string(),
            ]),
            bonus: -42.0,
            name: "anti_archaic_qide_before_degree_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(qide_determiners),
            curr_pattern: WordPattern::Exact("耆德".to_string()),
            bonus: 34.0,
            name: "bifurcation_determiner_archaic_qide".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("耆德".to_string()),
            curr_pattern: WordPattern::OneOf(qide_elder_nouns.clone()),
            bonus: 35.0,
            name: "bifurcation_archaic_qide_elder_head".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("得".to_string()),
            curr_pattern: WordPattern::OneOf(qide_elder_nouns),
            bonus: -38.0,
            name: "anti_complement_de_before_elder_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["什麼".to_string(), "不是什麼".to_string()]),
            curr_pattern: WordPattern::Exact("騎".to_string()),
            bonus: -32.0,
            name: "anti_shenme_qi2_before_elder".to_string(),
        });

        // (18b) 「對不對／雖然根本只是個年輕人／才不是什麼」長句結構凝聚與防跨界假詞：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("對".to_string()),
            curr_pattern: WordPattern::Exact("不對".to_string()),
            bonus: 32.0,
            name: "a_not_a_dui_budui".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("對簿".to_string()),
            curr_pattern: WordPattern::Exact("對".to_string()),
            bonus: -45.0,
            name: "anti_duibu_dui_impostor".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("雖然".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "根本".to_string(), "只是".to_string(), "還是".to_string(), "已經".to_string(),
            ]),
            bonus: 28.0,
            name: "suiran_genben_collocation".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("根本".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "只是".to_string(), "不是".to_string(), "沒".to_string(), "沒有".to_string(),
                "不".to_string(), "不會".to_string(), "不能".to_string(), "就是".to_string(),
            ]),
            bonus: 28.0,
            name: "genben_zhishi_negation".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("根本".to_string()),
            curr_pattern: WordPattern::Exact("指示".to_string()),
            bonus: -38.0,
            name: "anti_genben_zhishi_directive".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("只是".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "個".to_string(), "一個".to_string(), "一名".to_string(), "一位".to_string(),
            ]),
            bonus: 28.0,
            name: "zhishi_ge_indefinite_classifier".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["個".to_string(), "一個".to_string()]),
            curr_pattern: WordPattern::OneOf(vec![
                "年輕人".to_string(), "老人".to_string(), "孩子".to_string(), "小孩".to_string(),
                "新手".to_string(), "外行人".to_string(), "普通人".to_string(), "一般人".to_string(),
            ]),
            bonus: 28.0,
            name: "ge_human_role_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("才".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "不是".to_string(), "不會".to_string(), "不要".to_string(), "不能".to_string(),
            ]),
            bonus: 28.0,
            name: "cai2_bushi_emphatic_negation".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不是".to_string()),
            curr_pattern: WordPattern::Exact("什麼".to_string()),
            bonus: 28.0,
            name: "bushi_shenme_rhetorical_negation".to_string(),
        });

        // (18c) 其他雙向同音構式分流（獨步 vs 讀不、獨得 vs 讀得、走步 vs 走不、故步 vs 顧不、孝道 vs 笑到、全速 vs 全素）：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("獨步".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "全球".to_string(), "天下".to_string(), "武林".to_string(), "業界".to_string(),
                "亞洲".to_string(), "世界".to_string(), "古今".to_string(), "當代".to_string(),
            ]),
            bonus: 34.0,
            name: "bifurcation_dubu_domain_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("獨步".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "懂".to_string(), "完".to_string(), "通".to_string(), "下".to_string(), "出".to_string(),
            ]),
            bonus: -38.0,
            name: "anti_dubu_before_potential_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("獨得".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "頭獎".to_string(), "大獎".to_string(), "首獎".to_string(), "獎金".to_string(),
                "冠軍".to_string(), "金牌".to_string(), "第一".to_string(),
            ]),
            bonus: 34.0,
            name: "bifurcation_dude_prize_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("獨得".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "懂".to_string(), "完".to_string(), "好".to_string(), "快".to_string(), "飽".to_string(), "很".to_string(),
            ]),
            bonus: -38.0,
            name: "anti_dude_before_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("故步".to_string()),
            curr_pattern: WordPattern::Exact("自封".to_string()),
            bonus: 35.0,
            name: "bifurcation_gubu_zifeng".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("故步".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "上".to_string(), "得".to_string(), "了".to_string(), "到".to_string(),
            ]),
            bonus: -38.0,
            name: "anti_gubu_before_potential_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("孝道".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "飽".to_string(), "肚子痛".to_string(), "流淚".to_string(), "飆淚".to_string(), "翻".to_string(),
            ]),
            bonus: -40.0,
            name: "anti_xiaodao_before_satiation_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "因為".to_string(), "由於".to_string(), "他".to_string(), "一向".to_string(),
                "始終".to_string(), "堅持".to_string(), "嚴格".to_string(), "總是".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "恪守".to_string(), "謹守".to_string(), "奉行".to_string(), "秉持".to_string(), "固守".to_string(),
            ]),
            bonus: 34.0,
            name: "causal_subject_normative_verb_keshou".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "盡".to_string(), "恪守".to_string(), "謹守".to_string(), "奉行".to_string(),
                "秉持".to_string(), "發揚".to_string(), "弘揚".to_string(), "講求".to_string(),
                "傳統".to_string(), "實踐".to_string(), "違背".to_string(), "固守".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "孝道".to_string(), "本分".to_string(), "原則".to_string(), "禮教".to_string(),
                "規矩".to_string(), "道德".to_string(), "承諾".to_string(),
            ]),
            bonus: 34.0,
            name: "bifurcation_verb_xiaodao_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "孝道".to_string(), "原則".to_string(), "傳統".to_string(), "本分".to_string(),
                "禮教".to_string(), "道德".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "飽受".to_string(), "備受".to_string(), "深受".to_string(), "遭受".to_string(), "蒙受".to_string(),
            ]),
            bonus: 34.0,
            name: "ethics_noun_experiencer_verb_baoshou".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "飽受".to_string(), "備受".to_string(), "深受".to_string(), "遭受".to_string(),
                "蒙受".to_string(), "飽經".to_string(), "飽嘗".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "抨擊".to_string(), "批評".to_string(), "折磨".to_string(), "煎熬".to_string(),
                "委屈".to_string(), "苦難".to_string(), "質疑".to_string(), "責難".to_string(),
                "非議".to_string(), "嘲笑".to_string(), "譏諷".to_string(), "壓力".to_string(), "爭議".to_string(),
            ]),
            bonus: 32.0,
            name: "baoshou_adversity_noun_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "根本".to_string(), "簡直".to_string(), "完全".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "被".to_string(), "讓人".to_string(), "被人".to_string(),
            ]),
            bonus: 28.0,
            name: "emphatic_adv_passive_bei".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "被".to_string(), "讓人".to_string(), "被人".to_string(), "給人".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "笑".to_string(), "罵".to_string(), "電".to_string(), "酸".to_string(),
                "虧".to_string(), "嗆".to_string(), "虐".to_string(), "揍".to_string(), "打".to_string(),
            ]),
            bonus: 28.0,
            name: "passive_bei_action_verb_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "被".to_string(), "讓人".to_string(), "被人".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("孝道".to_string()),
            bonus: -42.0,
            name: "anti_passive_bei_xiaodao_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "這".to_string(), "那".to_string(), "難道".to_string(), "到底".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "還有".to_string(), "有沒有".to_string(), "哪有".to_string(),
            ]),
            bonus: 28.0,
            name: "rhetorical_dem_haiyou".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "還有".to_string(), "沒有".to_string(), "沒".to_string(), "喪盡".to_string(), "泯滅".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "天良".to_string(), "天理".to_string(), "良心".to_string(), "王法".to_string(), "人性".to_string(),
            ]),
            bonus: 32.0,
            name: "haiyou_tianliang_conscience_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("全速".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "前進".to_string(), "衝刺".to_string(), "行駛".to_string(), "運轉".to_string(),
                "推進".to_string(), "奔馳".to_string(), "飛馳".to_string(),
            ]),
            bonus: 34.0,
            name: "bifurcation_quansu_motion_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("全速".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "項目".to_string(), "選項".to_string(), "餐點".to_string(), "飲食".to_string(),
                "料理".to_string(), "便當".to_string(), "餐廳".to_string(),
            ]),
            bonus: -40.0,
            name: "anti_quansu_before_dietary_noun".to_string(),
        });

        // 助動介詞構式「應與」vs 語言名詞「英語」語境消歧（如「應與設定同步」、「應與國際接軌」、「應與事實相符」）
        let yingyu_relational_targets = vec![
            "設定", "系統", "標準", "規範", "國際", "實際", "事實", "現實",
            "原文", "前文", "原著", "合約", "規定", "法律", "法規", "時代",
            "世界", "環境", "硬體", "軟體", "伺服器", "資料庫", "對方", "他人",
            "客戶", "使用者", "團隊", "部門", "家長", "學校", "社會", "自然",
            "其", "此", "之",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("應與".to_string()),
            curr_pattern: WordPattern::OneOf(yingyu_relational_targets.iter().map(|s| s.to_string()).collect()),
            bonus: 35.0,
            name: "yingyu_modal_preposition_target".to_string(),
        });
        let relational_predicates = vec![
            "同步", "一致", "相符", "相同", "配合", "接軌", "對齊", "結合", "相容", "連動",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(yingyu_relational_targets.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(relational_predicates.into_iter().map(|s| s.to_string()).collect()),
            bonus: 28.0,
            name: "relational_target_sync_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("英語".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "設定".to_string(), "系統".to_string(), "規範".to_string(), "實際".to_string(),
                "事實".to_string(), "現實".to_string(), "合約".to_string(), "法規".to_string(),
                "硬體".to_string(), "伺服器".to_string(), "資料庫".to_string(), "同步".to_string(),
                "相符".to_string(), "一致".to_string(), "接軌".to_string(),
            ]),
            bonus: -35.0,
            name: "anti_english_before_sync_target".to_string(),
        });
        let language_action_verbs = vec![
            "說", "講", "學", "學習", "教", "懂", "會", "聽", "看", "寫", "翻譯", "精通", "用", "使用",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(language_action_verbs.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(vec![
                "英語".to_string(), "英文".to_string(), "日語".to_string(), "日文".to_string(),
                "法語".to_string(), "德語".to_string(), "韓語".to_string(), "國語".to_string(),
                "台語".to_string(), "母語".to_string(), "外語".to_string(),
            ]),
            bonus: 30.0,
            name: "language_action_verb_language_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "說".to_string(), "講".to_string(), "學".to_string(), "學習".to_string(),
                "教".to_string(), "聽".to_string(), "寫".to_string(), "精通".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("應與".to_string()),
            bonus: -30.0,
            name: "anti_yingyu_after_language_verb".to_string(),
        });

        // 28. 全稱限定詞構式（各種 / 各類 / 各項 / 各個 / 各式 ➔ 實詞名詞）與防碎裂約束（嚴禁拆碎為「個種」、「間粒」）：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "各種".to_string(), "各類".to_string(), "各項".to_string(),
                "各個".to_string(), "各式".to_string(), "各家".to_string(),
                "各界".to_string(), "各國".to_string(), "各地".to_string(),
            ]),
            curr_pattern: WordPattern::ContentWord,
            bonus: 24.0,
            name: "universal_determiner_content_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "基本上".to_string(), "處理".to_string(), "解決".to_string(),
                "面對".to_string(), "包含".to_string(), "涵蓋".to_string(),
                "培養".to_string(), "應對".to_string(), "支援".to_string(),
                "提供".to_string(), "針對".to_string(), "對於".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "各種".to_string(), "各類".to_string(), "各項".to_string(), "各個".to_string(),
            ]),
            bonus: 26.0,
            name: "adverb_or_verb_before_universal_determiner".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("個".to_string()),
            curr_pattern: WordPattern::Exact("種".to_string()),
            bonus: -40.0,
            name: "anti_shatter_ge_zhong_fake_compound".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("間".to_string()),
            curr_pattern: WordPattern::Exact("粒".to_string()),
            bonus: -45.0,
            name: "anti_shatter_jian_li_fake_compound".to_string(),
        });

        // 29. 「建立」動賓構式與「假詞彙 / 延伸類詞」語言學構式：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "需要".to_string(), "不需要".to_string(), "無需".to_string(), "毋須".to_string(),
                "不必".to_string(), "必須".to_string(), "應該".to_string(), "可以".to_string(),
                "能夠".to_string(), "能".to_string(), "要".to_string(), "會".to_string(),
                "先".to_string(), "再".to_string(), "重新".to_string(), "開始".to_string(),
                "逐步".to_string(), "共同".to_string(), "協助".to_string(), "透過".to_string(),
                "藉由".to_string(), "自行".to_string(), "手動".to_string(), "自動".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("建立".to_string()),
            bonus: 28.0,
            name: "modal_or_adverb_before_jianli".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("建立".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "一堆".to_string(), "一個".to_string(), "一套".to_string(), "一系列".to_string(),
                "一些".to_string(), "許多".to_string(), "各種".to_string(), "假詞彙".to_string(),
                "詞彙".to_string(), "詞庫".to_string(), "規則".to_string(), "系統".to_string(),
                "模型".to_string(), "架構".to_string(), "制度".to_string(), "關係".to_string(),
                "連線".to_string(), "索引".to_string(), "檔案".to_string(), "資料".to_string(),
                "標準".to_string(), "基礎".to_string(), "信心".to_string(), "共識".to_string(),
                "起".to_string(), "好".to_string(), "在".to_string(), "於".to_string(),
            ]),
            bonus: 26.0,
            name: "jianli_verb_object_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "一堆".to_string(), "一些".to_string(), "許多".to_string(), "很多".to_string(),
                "不少".to_string(), "幾個".to_string(), "這種".to_string(), "那種".to_string(),
                "這些".to_string(), "那些".to_string(), "所謂".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "假".to_string(), "新".to_string(), "舊".to_string(), "假詞彙".to_string(), "詞彙".to_string(),
            ]),
            bonus: 24.0,
            name: "quantifier_before_modifier_jia_cihui".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("假".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "詞彙".to_string(), "詞".to_string(), "訊息".to_string(), "新聞".to_string(),
                "資料".to_string(), "設".to_string(), "如".to_string(), "使".to_string(),
                "裝".to_string(), "冒".to_string(), "象".to_string(), "借".to_string(),
            ]),
            bonus: 28.0,
            name: "modifier_jia_before_lexical_target".to_string(),
        });

        // 30. 判斷感嘆程度構式（「真是太好用了」vs. 嚴禁「真 + 事態」錯誤拼湊）：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "真是".to_string(), "實在是".to_string(), "真的是".to_string(),
                "簡直是".to_string(), "算是".to_string(), "也是".to_string(),
                "還是".to_string(), "倒也是".to_string(), "那真是".to_string(), "這真是".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "太".to_string(), "很".to_string(), "非常".to_string(), "超".to_string(),
                "超級".to_string(), "十分".to_string(), "相當".to_string(), "特別".to_string(),
                "夠".to_string(), "挺".to_string(), "蠻".to_string(), "真".to_string(),
                "好".to_string(), "棒".to_string(), "讚".to_string(), "妙".to_string(),
            ]),
            bonus: 30.0,
            name: "copula_exclamatory_before_degree_adv".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "真".to_string(), "太".to_string(), "很".to_string(), "非常".to_string(),
                "超".to_string(), "挺".to_string(), "蠻".to_string(), "最".to_string(), "更".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec!["事態".to_string(), "世態".to_string()]),
            bonus: -40.0,
            name: "anti_degree_adv_before_abstract_noun_shitai".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["事態".to_string(), "世態".to_string()]),
            curr_pattern: WordPattern::OneOf(vec![
                "好用".to_string(), "好".to_string(), "棒".to_string(), "讚".to_string(),
                "快".to_string(), "慢".to_string(), "高".to_string(), "低".to_string(),
                "大".to_string(), "小".to_string(), "多".to_string(), "少".to_string(),
                "美".to_string(), "貴".to_string(),
            ]),
            bonus: -40.0,
            name: "anti_shitai_before_gradable_adj".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "事態".to_string(), "局勢".to_string(), "情勢".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "出現".to_string(), "發生".to_string(), "嚴重".to_string(), "發展".to_string(),
                "擴大".to_string(), "惡化".to_string(), "平息".to_string(), "緊急".to_string(),
                "控制".to_string(), "明朗".to_string(), "失控".to_string(), "變化".to_string(),
                "演變".to_string(), "炎涼".to_string(),
            ]),
            bonus: 28.0,
            name: "shitai_subject_before_event_or_state_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "太".to_string(), "很".to_string(), "非常".to_string(),
                "超".to_string(), "挺".to_string(), "蠻".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "出現".to_string(), "發生".to_string(), "產生".to_string(),
                "進行".to_string(), "開始".to_string(), "結束".to_string(),
            ]),
            bonus: -30.0,
            name: "anti_degree_adv_before_inchoative_event_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "好用".to_string(), "太高".to_string(), "太低".to_string(), "太快".to_string(),
                "太慢".to_string(), "太好".to_string(), "太棒".to_string(), "太貴".to_string(),
                "太難".to_string(), "太誇張".to_string(), "太離譜".to_string(), "太方便".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("了".to_string()),
            bonus: 26.0,
            name: "degree_adj_clause_final_le".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "調整".to_string(), "設定".to_string(), "修改".to_string(),
                "改變".to_string(), "切換".to_string(), "設計".to_string(), "功能".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "真是".to_string(), "實在是".to_string(), "真的是".to_string(),
                "太".to_string(), "很".to_string(), "非常".to_string(),
            ]),
            bonus: 24.0,
            name: "topic_nominal_before_exclamatory_predicate".to_string(),
        });

        // 31. 「權重 (ㄑㄩㄢˊ ㄓㄨㄥˋ)」vs「全中」資訊科學語境搭配，以及「英語人才應與世界接軌」、「課題延伸類詞」構式：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "出現的".to_string(), "單字的".to_string(), "詞彙的".to_string(),
                "模型的".to_string(), "節點的".to_string(), "語意的".to_string(),
                "的".to_string(), "出現".to_string(), "單字".to_string(),
                "詞頻".to_string(), "模型".to_string(), "調整".to_string(),
                "設定".to_string(), "提高".to_string(), "降低".to_string(),
                "計算".to_string(), "分配".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("權重".to_string()),
            bonus: 28.0,
            name: "modifier_or_verb_before_quanzhong".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("權重".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "看來".to_string(), "看起來".to_string(), "太高".to_string(), "過高".to_string(),
                "太低".to_string(), "過低".to_string(), "調整".to_string(), "設定".to_string(),
                "分配".to_string(), "計算".to_string(), "失衡".to_string(), "比例".to_string(),
                "上限".to_string(), "下限".to_string(), "值".to_string(), "高".to_string(), "低".to_string(),
            ]),
            bonus: 28.0,
            name: "quanzhong_before_predicate_or_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec!["看來".to_string(), "看起來".to_string()]),
            curr_pattern: WordPattern::OneOf(vec![
                "太".to_string(), "很".to_string(), "非常".to_string(), "已經".to_string(),
                "還是".to_string(), "是".to_string(), "有".to_string(), "相當".to_string(),
                "太高".to_string(), "太低".to_string(), "不錯".to_string(),
            ]),
            bonus: 26.0,
            name: "kanlai_before_degree_predicate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "各種".to_string(), "各類".to_string(), "優秀".to_string(),
                "專業".to_string(), "培養".to_string(), "招募".to_string(),
                "國際".to_string(), "台灣".to_string(), "本土".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("英語".to_string()),
            bonus: 26.0,
            name: "modifier_before_english_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "英語".to_string(), "英文".to_string(), "日語".to_string(), "日文".to_string(), "外語".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "人才".to_string(), "能力".to_string(), "教學".to_string(), "教育".to_string(),
                "課程".to_string(), "環境".to_string(), "檢定".to_string(), "程度".to_string(),
                "老師".to_string(), "學習".to_string(), "溝通".to_string(), "會話".to_string(),
            ]),
            bonus: 30.0,
            name: "language_noun_before_compound_head".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("應與".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "人才".to_string(), "教學".to_string(), "課程".to_string(),
                "檢定".to_string(), "老師".to_string(), "會話".to_string(),
            ]),
            bonus: -35.0,
            name: "anti_yingyu_before_language_head_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "人才".to_string(), "系統".to_string(), "台灣".to_string(),
                "企業".to_string(), "教育".to_string(), "政策".to_string(),
                "規格".to_string(), "設計".to_string(), "標準".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("應與".to_string()),
            bonus: 28.0,
            name: "subject_noun_before_yingyu_modal_prep".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "課題".to_string(), "問題".to_string(), "概念".to_string(),
                "語意".to_string(), "規則".to_string(), "主題".to_string(), "話題".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("延伸".to_string()),
            bonus: 28.0,
            name: "topic_noun_before_yanshen".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("延伸".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "類詞".to_string(), "詞彙".to_string(), "詞".to_string(),
                "問題".to_string(), "討論".to_string(), "應用".to_string(),
                "思考".to_string(), "出".to_string(), "到".to_string(), "至".to_string(),
            ]),
            bonus: 26.0,
            name: "yanshen_before_nominal_or_complement".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "能夠".to_string(), "能".to_string(), "可以".to_string(),
                "都能".to_string(), "都可以".to_string(), "都能夠".to_string(),
                "皆能".to_string(), "均能".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "分辨".to_string(), "區分".to_string(), "解決".to_string(),
                "處理".to_string(), "判斷".to_string(), "完成".to_string(), "達成".to_string(),
            ]),
            bonus: 28.0,
            name: "modal_can_before_cognitive_resolution_verb".to_string(),
        });

        // 32. 人物偏好／狂熱／專精準詞綴家族二維陣列構式：`[領域/事物/屬性詞根 XYZ] × [{控, 狂, 粉, 迷, 族, 達人, 魔人, 專家, 大師, 高手, 玩家, 行家}]`
        let aficionado_stems = vec![
            "細節", "設定", "數據", "資料", "音質", "畫質", "顏值", "外貌", "制服", "聲優",
            "大叔", "蘿莉", "御姐", "貓", "狗", "毛孩", "手錶", "機械", "鍵盤", "文具",
            "字體", "排版", "收納", "清潔", "完美", "裝備", "器材", "耳機", "相機", "攝影",
            "咖啡", "抹茶", "甜點", "巧克力", "起司", "草莓", "芋頭", "香菜", "火鍋", "拉麵",
            "鐵道", "火車", "飛機", "軍事", "歷史", "推理", "科幻", "動漫", "遊戲", "電玩",
            "電影", "音樂", "古典", "搖滾", "爵士", "購物", "工作", "健身", "跑步", "單車",
            "露營", "旅遊", "植物", "多肉", "古著", "球鞋", "香水", "保養", "成分", "效率",
            "速度", "性能", "參數", "程式碼", "架構", "美學", "極簡", "復古", "色彩", "對稱",
            "秩序", "各種", "各類",
        ];
        let aficionado_suffixes = vec![
            "控", "狂", "粉", "迷", "族", "達人", "魔人", "專家", "大師", "高手", "玩家", "行家",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(aficionado_stems.iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(aficionado_suffixes.iter().map(|s| s.to_string()).collect()),
            bonus: 30.0,
            name: "aficionado_specialist_quasi_suffix_array".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(aficionado_stems.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::Exact("空".to_string()),
            bonus: -32.0,
            name: "anti_kong_empty_after_aficionado_stem".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "對於".to_string(), "對".to_string(), "身為".to_string(), "作為".to_string(),
                "是個".to_string(), "簡直是".to_string(), "根本是".to_string(), "各種".to_string(),
                "各類".to_string(), "許多".to_string(), "這種".to_string(), "那種".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "細節".to_string(), "細節控".to_string(), "音質控".to_string(), "畫質控".to_string(),
                "顏值控".to_string(), "制服控".to_string(), "聲優控".to_string(), "文具控".to_string(),
                "鍵盤控".to_string(), "字體控".to_string(), "收納控".to_string(), "咖啡控".to_string(),
                "抹茶控".to_string(), "甜點控".to_string(), "草莓控".to_string(), "芋頭控".to_string(),
                "香菜控".to_string(), "機械控".to_string(), "極簡控".to_string(), "參數控".to_string(),
                "數據控".to_string(),
            ]),
            bonus: 28.0,
            name: "preposition_or_copula_before_aficionado_noun".to_string(),
        });
        let aficionado_heads_and_compounds = vec![
            "控", "狂", "粉", "迷", "族", "達人", "魔人", "專家", "大師", "高手", "玩家", "行家",
            "細節控", "音質控", "畫質控", "顏值控", "制服控", "聲優控", "文具控", "鍵盤控",
            "字體控", "收納控", "咖啡控", "抹茶控", "甜點控", "草莓控", "芋頭控", "香菜控",
            "機械控", "極簡控", "參數控", "數據控",
        ];
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(aficionado_heads_and_compounds.into_iter().map(|s| s.to_string()).collect()),
            curr_pattern: WordPattern::OneOf(vec![
                "會".to_string(), "都會".to_string(), "一定".to_string(), "肯定".to_string(),
                "絕對".to_string(), "最".to_string(), "感到".to_string(), "覺得".to_string(),
                "看到".to_string(), "來說".to_string(), "而言".to_string(), "是".to_string(),
                "的".to_string(), "們".to_string(),
            ]),
            bonus: 26.0,
            name: "aficionado_noun_before_predicate_or_particle".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "感到".to_string(), "覺得".to_string(), "感覺".to_string(),
                "令人".to_string(), "讓人".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "無比".to_string(), "十分".to_string(), "非常".to_string(), "格外".to_string(),
                "相當".to_string(), "極為".to_string(), "舒適".to_string(), "滿意".to_string(),
                "療癒".to_string(), "安心".to_string(), "放心".to_string(), "驚艷".to_string(),
            ]),
            bonus: 28.0,
            name: "psych_verb_before_wubi_degree_or_state".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("無比".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "舒適".to_string(), "輕鬆".to_string(), "流暢".to_string(), "順暢".to_string(),
                "便利".to_string(), "方便".to_string(), "自豪".to_string(), "驕傲".to_string(),
                "興奮".to_string(), "開心".to_string(), "高興".to_string(), "欣慰".to_string(),
                "滿足".to_string(), "重要".to_string(), "清晰".to_string(),
            ]),
            bonus: 28.0,
            name: "wubi_degree_before_state_adj".to_string(),
        });

        // 33. 「調校完畢 vs 調教完比」、「不斷升級 vs 不段」、「在開發文件中也寫下這段註記」與語意調校工作室語境構式：
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "語意".to_string(), "模型".to_string(), "參數".to_string(), "權重".to_string(),
                "系統".to_string(), "引擎".to_string(), "音質".to_string(), "色彩".to_string(),
                "螢幕".to_string(), "演算法".to_string(), "經過".to_string(), "完成".to_string(),
                "自動".to_string(), "手動".to_string(), "進行".to_string(), "持續".to_string(),
                "就".to_string(), "才".to_string(), "已".to_string(), "已經".to_string(),
                "提高".to_string(), "大幅提高".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "調校".to_string(), "調整".to_string(),
            ]),
            bonus: 30.0,
            name: "technical_context_before_tiaojiao_calibrate".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "調校".to_string(), "調整".to_string(), "語意調校".to_string(), "語意調整".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "完畢".to_string(), "完成".to_string(), "結果".to_string(), "工作室".to_string(),
                "成功".to_string(), "成功率".to_string(), "過程".to_string(), "參數".to_string(),
                "權重".to_string(), "模型".to_string(), "系統".to_string(), "引擎".to_string(),
                "後".to_string(), "中".to_string(), "出".to_string(),
            ]),
            bonus: 30.0,
            name: "tiaojiao_calibrate_before_complement_or_object".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("調教".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "完畢".to_string(), "工作室".to_string(), "成功率".to_string(), "結果".to_string(),
                "參數".to_string(), "權重".to_string(), "模型".to_string(), "引擎".to_string(),
            ]),
            bonus: -28.0,
            name: "anti_tiaojiao_discipline_in_technical_calibration".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("完".to_string()),
            curr_pattern: WordPattern::Exact("比".to_string()),
            bonus: -45.0,
            name: "anti_wan_bi_shatter_impostor".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "隨著".to_string(), "隨".to_string(), "隨着".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "各種".to_string(), "各類".to_string(), "模型".to_string(), "系統".to_string(),
                "技術".to_string(), "時代".to_string(), "時間".to_string(), "版本".to_string(),
            ]),
            bonus: 28.0,
            name: "suizhe_before_nominal_subject".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "模型".to_string(), "系統".to_string(), "技術".to_string(), "版本".to_string(),
                "功能".to_string(), "詞庫".to_string(), "演算法".to_string(), "架構".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "不斷".to_string(), "持續".to_string(), "進步".to_string(), "升級".to_string(),
                "釋出".to_string(), "更新".to_string(), "演進".to_string(),
            ]),
            bonus: 28.0,
            name: "system_model_subject_before_buduan_or_evolution_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不斷".to_string()),
            curr_pattern: WordPattern::ContentWord,
            bonus: 24.0,
            name: "buduan_continuous_adverb_content_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("不".to_string()),
            curr_pattern: WordPattern::Exact("段".to_string()),
            bonus: -45.0,
            name: "anti_bu_duan_classifier_shatter".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "或許".to_string(), "也許".to_string(), "可能".to_string(), "大概".to_string(),
                "同時".to_string(), "並且".to_string(), "而且".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("在".to_string()),
            bonus: 26.0,
            name: "epistemic_adverb_before_preposition_zai".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "開發".to_string(), "設計".to_string(), "技術".to_string(), "系統".to_string(),
                "專案".to_string(), "說明".to_string(), "規格".to_string(), "官方".to_string(),
                "測試".to_string(), "原始".to_string(), "相關".to_string(), "這份".to_string(),
                "那份".to_string(), "讀取".to_string(), "儲存".to_string(), "寫入".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("文件".to_string()),
            bonus: 28.0,
            name: "modifier_or_verb_before_wenjian_document".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "中".to_string(), "裡".to_string(), "內".to_string(), "上".to_string(),
                "下".to_string(), "裡面".to_string(), "之中".to_string(), "之內".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "也".to_string(), "都".to_string(), "就".to_string(), "才".to_string(),
                "還".to_string(), "又".to_string(), "常".to_string(), "會".to_string(),
                "能".to_string(), "可以".to_string(), "要".to_string(), "已".to_string(),
                "已經".to_string(), "正".to_string(), "正在".to_string(), "將".to_string(),
                "不".to_string(), "沒".to_string(), "沒有".to_string(), "有".to_string(),
                "寫".to_string(), "寫下".to_string(), "加入".to_string(), "包含".to_string(),
            ]),
            bonus: 25.0,
            name: "postpositional_locative_before_adverb_or_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "文件".to_string(), "開發".to_string(), "系統".to_string(), "過程".to_string(),
                "文章".to_string(), "報告".to_string(), "程式".to_string(), "架構".to_string(),
                "設定".to_string(), "面板".to_string(), "文".to_string(), "間".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("中野".to_string()),
            bonus: -45.0,
            name: "anti_nakano_place_name_after_document_or_container".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "也".to_string(), "都".to_string(), "就".to_string(), "才".to_string(),
                "還".to_string(), "又".to_string(), "並".to_string(), "先".to_string(), "再".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "寫下".to_string(), "記下".to_string(), "留下".to_string(), "寫入".to_string(),
                "加入".to_string(), "補上".to_string(), "紀錄".to_string(), "記錄".to_string(),
                "增加".to_string(),
            ]),
            bonus: 28.0,
            name: "adverb_before_write_or_record_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "寫下".to_string(), "記下".to_string(), "留下".to_string(), "補上".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "這段".to_string(), "那段".to_string(), "一段".to_string(),
                "這".to_string(), "那".to_string(), "註記".to_string(), "說明".to_string(),
                "文字".to_string(), "紀錄".to_string(), "話".to_string(),
            ]),
            bonus: 26.0,
            name: "write_verb_before_demonstrative_duan_or_note".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "這段".to_string(), "那段".to_string(), "一段".to_string(), "段".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "註記".to_string(), "文字".to_string(), "說明".to_string(), "時間".to_string(),
                "話".to_string(), "落".to_string(), "程式碼".to_string(), "歷史".to_string(),
                "記憶".to_string(), "日子".to_string(), "期間".to_string(), "距離".to_string(),
                "路".to_string(), "期間".to_string(),
            ]),
            bonus: 28.0,
            name: "duan_classifier_before_discourse_or_time_noun".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "部分的".to_string(), "這個".to_string(), "那個".to_string(), "中的".to_string(),
                "裡的".to_string(), "的".to_string(), "語言".to_string(), "自然語言".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "數值框".to_string(), "輸入框".to_string(), "對話框".to_string(),
                "搜尋框".to_string(), "文字框".to_string(),
            ]),
            bonus: 28.0,
            name: "modifier_before_ui_box_compound".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "新的".to_string(), "任何".to_string(), "不同".to_string(), "各種".to_string(),
                "其他".to_string(), "多".to_string(), "跨".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("語種".to_string()),
            bonus: 28.0,
            name: "modifier_before_yuzhong_language_type".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("語種".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "輸入法".to_string(), "模型".to_string(), "詞庫".to_string(),
                "支援".to_string(), "切換".to_string(), "翻譯".to_string(), "的".to_string(),
            ]),
            bonus: 30.0,
            name: "yuzhong_before_ime_or_model_head".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "跟".to_string(), "與".to_string(), "和".to_string(), "所".to_string(), "已".to_string(),
            ]),
            curr_pattern: WordPattern::Exact("對接".to_string()),
            bonus: 26.0,
            name: "prep_before_duijie_verb".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "對接的".to_string(), "的".to_string(), "跟".to_string(), "與".to_string(), "對接".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "語言模型".to_string(), "自然語言".to_string(), "大語言模型".to_string(), "模型".to_string(),
            ]),
            bonus: 28.0,
            name: "modifier_before_language_model".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::OneOf(vec![
                "語言模型".to_string(), "大語言模型".to_string(), "模型".to_string(),
            ]),
            curr_pattern: WordPattern::OneOf(vec![
                "對話".to_string(), "互動".to_string(), "溝通".to_string(),
                "調校".to_string(), "連線".to_string(), "對接".to_string(),
            ]),
            bonus: 28.0,
            name: "language_model_before_dialogue_or_action".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("進步".to_string()),
            curr_pattern: WordPattern::Exact("而".to_string()),
            bonus: 26.0,
            name: "jinbu_before_conjunction_er".to_string(),
        });
        self.grammar_rules.push(GrammarRule {
            prev_pattern: WordPattern::Exact("而".to_string()),
            curr_pattern: WordPattern::OneOf(vec![
                "大幅".to_string(), "顯著".to_string(), "有效".to_string(),
                "進一步".to_string(), "自動".to_string(), "直接".to_string(),
            ]),
            bonus: 28.0,
            name: "conjunction_er_before_degree_adverb_dafu".to_string(),
        });
    }

    /// 重建語法規則雙向倒排索引（加速 40~80 倍，O(1) 精準規則定位）
    pub fn rebuild_rule_index(&mut self) {
        self.rules_by_prev.clear();
        self.rules_by_curr.clear();
        self.wildcard_rules.clear();

        for (idx, rule) in self.grammar_rules.iter().enumerate() {
            let mut indexed_by_prev = false;
            match &rule.prev_pattern {
                WordPattern::Exact(w) => {
                    self.rules_by_prev.entry(w.clone()).or_default().push(idx);
                    indexed_by_prev = true;
                }
                WordPattern::OneOf(words) => {
                    for w in words {
                        self.rules_by_prev.entry(w.clone()).or_default().push(idx);
                    }
                    indexed_by_prev = true;
                }
                _ => {}
            }

            if !indexed_by_prev {
                // 前驅為通配符，嘗試按後繼建立索引
                let mut indexed_by_curr = false;
                match &rule.curr_pattern {
                    WordPattern::Exact(w) => {
                        self.rules_by_curr.entry(w.clone()).or_default().push(idx);
                        indexed_by_curr = true;
                    }
                    WordPattern::OneOf(words) => {
                        for w in words {
                            self.rules_by_curr.entry(w.clone()).or_default().push(idx);
                        }
                        indexed_by_curr = true;
                    }
                    _ => {}
                }

                if !indexed_by_curr {
                    // 兩端皆為通配符（極罕見）
                    self.wildcard_rules.push(idx);
                }
            }
        }
    }

    fn is_valid_cjk_token(s: &str) -> bool {
        let trimmed = s.trim();
        !trimmed.is_empty()
            && trimmed.chars().all(|c| {
                ('\u{4e00}'..='\u{9fff}').contains(&c)
                    || ('\u{3400}'..='\u{4dbf}').contains(&c)
                    || ('\u{f900}'..='\u{faff}').contains(&c)
            })
    }

    /// 從 JSONL 檔案載入 Layer 0 因式分解語意槽位規則（patterns_user.jsonl），並納入雙向倒排索引
    pub fn load_patterns_jsonl<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        fn default_pattern_bonus() -> f64 {
            28.0
        }

        #[derive(serde::Deserialize)]
        struct UserSlotPattern {
            #[serde(default)]
            prev_slot: Vec<String>,
            #[serde(default)]
            next_slot: Vec<String>,
            #[serde(default)]
            slots: Vec<Vec<String>>,
            #[serde(default)]
            optional_slots: Vec<usize>,
            #[serde(default)]
            target: Vec<String>,
            #[serde(default)]
            preferred: Vec<String>,
            #[serde(default)]
            exclude_pairs: Vec<Vec<String>>,
            #[serde(default)]
            #[allow(dead_code)]
            bifurcation_pairs: Vec<serde_json::Value>,
            #[serde(default = "default_pattern_bonus")]
            bonus: f64,
            #[serde(default)]
            name: String,
            #[serde(default)]
            pattern_id: String,
            #[serde(default)]
            description: String,
        }

        let p = path.as_ref();
        let file = File::open(p)?;
        let reader = BufReader::new(file);

        // 截斷先前載入之外掛槽位規則，100% 保留內建語法規則
        if self.builtin_grammar_rule_count > 0 && self.grammar_rules.len() > self.builtin_grammar_rule_count {
            self.grammar_rules.truncate(self.builtin_grammar_rule_count);
        }
        self.user_trigram_patterns.clear();

        let mut count = 0;
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Ok(pat) = serde_json::from_str::<UserSlotPattern>(trimmed) {
                if pat.bonus <= 0.0 {
                    continue;
                }
                let safe_bonus = pat.bonus.clamp(15.0, 35.0);
                let label = if !pat.pattern_id.is_empty() {
                    pat.pattern_id.clone()
                } else if !pat.name.is_empty() {
                    pat.name.clone()
                } else if !pat.description.is_empty() {
                    pat.description.clone()
                } else {
                    format!("slot_{}", count)
                };

                // 載入對衝抑制陣列（exclude_pairs）：當陣列展開之特定二元對 (p, n) 與真實詞彙同音對衝時，於 O(1) 查找直接攔截該二元展開
                for pair in &pat.exclude_pairs {
                    if pair.len() == 2
                        && Self::is_valid_cjk_token(&pair[0])
                        && Self::is_valid_cjk_token(&pair[1])
                    {
                        self.excluded_pairs.insert((pair[0].trim().to_string(), pair[1].trim().to_string()));
                    }
                }

                let mut chain_registered_first_pair = false;
                if pat.slots.len() >= 3 {
                    for w in 0..=pat.slots.len() - 3 {
                        let s0: HashSet<String> = pat.slots[w]
                            .iter()
                            .map(|word| word.trim().to_string())
                            .filter(|word| Self::is_valid_cjk_token(word))
                            .collect();
                        let s1: HashSet<String> = pat.slots[w + 1]
                            .iter()
                            .map(|word| word.trim().to_string())
                            .filter(|word| Self::is_valid_cjk_token(word))
                            .collect();
                        let s2: HashSet<String> = pat.slots[w + 2]
                            .iter()
                            .map(|word| word.trim().to_string())
                            .filter(|word| Self::is_valid_cjk_token(word))
                            .collect();
                        if !s0.is_empty() && !s1.is_empty() && !s2.is_empty() {
                            self.user_trigram_patterns.push(TrigramSlotPattern {
                                slot0: s0,
                                slot1: s1,
                                slot2: s2,
                                bonus: safe_bonus,
                                name: label.clone(),
                            });
                            if w == 0 {
                                count += 1;
                            }
                        }
                    }
                    // 若為無 target/preferred 之多維槽位鏈（如 [A_arr, B_arr, C_arr, D_arr]），
                    // 自動將相鄰槽位對 (slots[i] × slots[i+1]) 及可選槽位跨接 (slots[i] × slots[i+2]) 註冊為 O(1) 陣列規則
                    if pat.target.is_empty() && pat.preferred.is_empty() {
                        let cleaned_slots: Vec<Vec<String>> = pat
                            .slots
                            .iter()
                            .map(|sl| {
                                sl.iter()
                                    .map(|w| w.trim().to_string())
                                    .filter(|w| Self::is_valid_cjk_token(w))
                                    .collect()
                            })
                            .collect();
                        for i in 0..cleaned_slots.len().saturating_sub(1) {
                            let left = &cleaned_slots[i];
                            let right = &cleaned_slots[i + 1];
                            let left_is_pure_coordinator_or_particle = left.iter().all(|w| {
                                matches!(
                                    w.as_str(),
                                    "與" | "和" | "及" | "或" | "跟" | "的" | "之" | "得" | "到"
                                )
                            });
                            if !left.is_empty() && !right.is_empty() && !left_is_pure_coordinator_or_particle {
                                if i == 0 && pat.prev_slot == pat.slots[0] && pat.next_slot == pat.slots[1] {
                                    chain_registered_first_pair = true;
                                }
                                self.grammar_rules.push(GrammarRule {
                                    prev_pattern: if left.len() == 1 {
                                        WordPattern::Exact(left[0].clone())
                                    } else {
                                        WordPattern::OneOf(left.clone())
                                    },
                                    curr_pattern: if right.len() == 1 {
                                        WordPattern::Exact(right[0].clone())
                                    } else {
                                        WordPattern::OneOf(right.clone())
                                    },
                                    bonus: safe_bonus,
                                    name: format!("user_chain:{}:{}->{}", label, i, i + 1),
                                });
                            }
                            if pat.optional_slots.contains(&(i + 1)) && i + 2 < cleaned_slots.len() {
                                let skip_right = &cleaned_slots[i + 2];
                                if !left.is_empty() && !skip_right.is_empty() {
                                    self.grammar_rules.push(GrammarRule {
                                        prev_pattern: if left.len() == 1 {
                                            WordPattern::Exact(left[0].clone())
                                        } else {
                                            WordPattern::OneOf(left.clone())
                                        },
                                        curr_pattern: if skip_right.len() == 1 {
                                            WordPattern::Exact(skip_right[0].clone())
                                        } else {
                                            WordPattern::OneOf(skip_right.clone())
                                        },
                                        bonus: safe_bonus,
                                        name: format!("user_chain_opt:{}:{}->{}", label, i, i + 2),
                                    });
                                }
                            }
                        }
                    }
                    // 三槽位以上之純鏈式規則已由上方相鄰槽位 (i -> i+1) 完整展開，禁止再用首尾 prev_slot × next_slot 註冊跨槽二元捷徑！
                    if pat.target.is_empty() && pat.preferred.is_empty() {
                        continue;
                    }
                    if pat.prev_slot.is_empty() || pat.next_slot.is_empty() || chain_registered_first_pair {
                        continue;
                    }
                }

                let eff_target = if !pat.target.is_empty() {
                    pat.target
                } else {
                    pat.preferred
                };

                let (raw_prev, raw_next) = if !pat.prev_slot.is_empty() && !pat.next_slot.is_empty() {
                    (pat.prev_slot, pat.next_slot)
                } else if pat.slots.len() == 1 && !eff_target.is_empty() {
                    (pat.slots[0].clone(), eff_target)
                } else if pat.slots.len() == 2 && !eff_target.is_empty() {
                    // 雙槽位笛卡兒展開為前驅組合 + 單槽位後驅
                    let mut combined = Vec::new();
                    for s0 in &pat.slots[0] {
                        for s1 in &pat.slots[1] {
                            combined.push(format!("{}{}", s0.trim(), s1.trim()));
                        }
                    }
                    for s1 in &pat.slots[1] {
                        combined.push(s1.trim().to_string());
                    }
                    (combined, eff_target)
                } else if pat.slots.len() == 2 && eff_target.is_empty() {
                    (pat.slots[0].clone(), pat.slots[1].clone())
                } else {
                    continue;
                };

                if raw_prev.is_empty() || raw_next.is_empty() {
                    continue;
                }
                if !raw_prev.iter().all(|w| Self::is_valid_cjk_token(w))
                    || !raw_next.iter().all(|w| Self::is_valid_cjk_token(w))
                {
                    continue;
                }

                let prev_clean: Vec<String> = raw_prev.into_iter().map(|w| w.trim().to_string()).collect();
                let next_clean: Vec<String> = raw_next.into_iter().map(|w| w.trim().to_string()).collect();

                let prev_pattern = if prev_clean.len() == 1 {
                    WordPattern::Exact(prev_clean.into_iter().next().unwrap())
                } else {
                    WordPattern::OneOf(prev_clean)
                };
                let curr_pattern = if next_clean.len() == 1 {
                    WordPattern::Exact(next_clean.into_iter().next().unwrap())
                } else {
                    WordPattern::OneOf(next_clean)
                };

                self.grammar_rules.push(GrammarRule {
                    prev_pattern,
                    curr_pattern,
                    bonus: safe_bonus,
                    name: format!("user_pattern:{}", label),
                });
                count += 1;
            }
        }

        self.patterns_file_path = Some(p.to_path_buf());
        if let Ok(meta) = std::fs::metadata(p) {
            self.patterns_last_modified = meta.modified().ok();
        }

        self.rebuild_rule_index();
        Ok(count)
    }

    /// 從 TSV 檔案載入使用者自訂或 Gemma 蒸餾出的動態轉移矩陣
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let p = path.as_ref();
        let file = File::open(p)?;
        let reader = BufReader::new(file);
        self.transitions.clear();
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

    /// 熱重載檢查（內建 2000ms 節流機制，同時檢查 transitions_user.tsv 與 patterns_user.jsonl）
    pub fn maybe_reload(&mut self) {
        if self.file_path.is_none() && self.patterns_file_path.is_none() {
            return;
        }
        let now = std::time::Instant::now();
        if let Some(last_check) = self.last_check_instant {
            if now.duration_since(last_check).as_millis() < 2000 {
                return;
            }
        }
        self.last_check_instant = Some(now);

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

        if let Some(ref path) = self.patterns_file_path {
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(mtime) = meta.modified() {
                    let need_reload = match self.patterns_last_modified {
                        Some(last) => mtime > last,
                        None => true,
                    };
                    if need_reload {
                        let p = path.clone();
                        let _ = self.load_patterns_jsonl(p);
                    }
                }
            }
        }
    }

    /// 將單一規則名稱歸類至對應之特徵規則族（供平均結構化感知器共用權重校準與消融分組）
    pub fn classify_rule_family(rule_name: &str) -> &'static str {
        if rule_name == "user_bigram" || rule_name.starts_with("user_bigram_pos") {
            "user_bigram_pos"
        } else if rule_name.starts_with("user_bigram_neg") {
            "user_bigram_neg"
        } else if rule_name == "builtin_bigram" || rule_name.starts_with("builtin_bigram:") {
            "builtin_bigram"
        } else if rule_name == "vo_separable_lookback" {
            "vo_separable_lookback"
        } else if rule_name == "construction_hierarchical_matrix" || rule_name.starts_with("trigram_") {
            "construction_hierarchical_matrix"
        } else if rule_name.starts_with("user_chain:")
            || rule_name.starts_with("user_chain_opt:")
            || rule_name.starts_with("user_pattern:")
        {
            "family_user_pattern"
        } else if rule_name.starts_with("vo_")
            || rule_name.starts_with("potential_")
            || rule_name.starts_with("directional_")
            || rule_name.contains("complement")
        {
            "family_vo_complement"
        } else if rule_name.contains("classifier")
            || rule_name.contains("numeric")
            || rule_name.contains("measure")
            || rule_name.contains("quant")
        {
            "family_classifier_numeric"
        } else if rule_name.contains("localizer")
            || rule_name.contains("locative")
            || rule_name.contains("spatial")
        {
            "family_spatial_localizer"
        } else if rule_name.contains("aspect")
            || rule_name.contains("tense")
            || rule_name.contains("temporal")
        {
            "family_aspect_temporal"
        } else if rule_name.contains("conjunction")
            || rule_name.contains("correlative")
            || rule_name.contains("cleft")
        {
            "family_conjunction_discourse"
        } else if rule_name.starts_with("guard_")
            || rule_name.starts_with("morph_")
            || rule_name.contains("penalty")
            || rule_name.contains("anti_")
        {
            "family_morph_phonetic_guard"
        } else {
            "family_core_syntax"
        }
    }

    #[inline]
    pub fn is_rule_disabled(&self, rule_name: &str) -> bool {
        if self.disabled_rules.is_empty() {
            return false;
        }
        if self.disabled_rules.contains(rule_name) {
            return true;
        }
        let fam = Self::classify_rule_family(rule_name);
        if self.disabled_rules.contains(fam) {
            return true;
        }
        self.disabled_rules
            .iter()
            .any(|pat| !pat.is_empty() && rule_name.starts_with(pat.as_str()))
    }

    #[inline]
    pub fn apply_rule_score(&mut self, rule_name: &str, raw_score: f64) -> f64 {
        if raw_score == 0.0 || self.is_rule_disabled(rule_name) {
            return 0.0;
        }
        let fam = Self::classify_rule_family(rule_name);
        let w = self
            .feature_weights
            .get(rule_name)
            .or_else(|| self.feature_weights.get(fam))
            .copied()
            .unwrap_or(1.0);
        if let Some(ref mut trace) = self.active_trace {
            trace.push((rule_name.to_string(), fam.to_string(), raw_score));
        }
        raw_score * w
    }

    /// 從 TSV 載入線性模型特徵與規則族權重 (`feature_weights.tsv`)
    pub fn load_feature_weights_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let key = parts[0].trim();
                if let Ok(val) = parts[1].trim().parse::<f64>() {
                    self.feature_weights.insert(key.to_string(), val);
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// 計算前驅節點轉移至當前節點的語意句法轉移加權分
    ///
    /// 查找順序（優先順序由高到低）:
    ///   1. 精確二元搭配 (Layer 2 exact match: builtin_transitions + user transitions)
    ///   2. 語法規則通配與槽位陣列 (Layer 1 & Layer 0 indexed grammar rules)
    pub fn calculate_transition_bonus(
        &mut self,
        prev_word: &str,
        prev_bopomofo: &str,
        curr_word: &str,
        curr_bopomofo: &str,
    ) -> f64 {
        self.maybe_reload();

        // ── 破音字古音與孤例隔離層 (Archaic & Isolated Polyphonic Isolation Layer) ──
        if crate::router::archaic_table::is_archaic_polyphonic(curr_word, curr_bopomofo)
            || crate::router::archaic_table::is_archaic_polyphonic(prev_word, prev_bopomofo)
        {
            return self.apply_rule_score("guard_archaic_polyphonic", -45.0);
        }

        let mut bonus = 0.0;

        // 抑制口吃音重疊：「感謝」後接同音「ㄒㄧㄝˋ」單字（嚴防破壞「敢+謝謝」構式）
        if prev_word == "感謝" && curr_bopomofo.starts_with("ㄒㄧㄝ") && curr_word.chars().count() == 1 {
            bonus += self.apply_rule_score("guard_ganxie_stutter", -50.0);
        }

        // Layer 2: 精確二元搭配 O(1) 雜湊查表（使用者自訂 transitions + 內建結構化收斂 builtin_transitions）
        let pair_key = (prev_word.to_string(), curr_word.to_string());
        if let Some(&score) = self.transitions.get(&pair_key) {
            if self.active_trace.is_some() || !self.disabled_rules.is_empty() {
                let rid = if score >= 0.0 {
                    format!("user_bigram_pos:{}->{}", prev_word, curr_word)
                } else {
                    format!("user_bigram_neg:{}->{}", prev_word, curr_word)
                };
                bonus += self.apply_rule_score(&rid, score);
            } else if score >= 0.0 {
                bonus += self.apply_rule_score("user_bigram_pos", score);
            } else {
                bonus += self.apply_rule_score("user_bigram_neg", score);
            }
        }
        if let Some(&score) = self.builtin_transitions.get(&pair_key) {
            if self.active_trace.is_some() || !self.disabled_rules.is_empty() {
                let rid = format!("builtin_bigram:{}->{}", prev_word, curr_word);
                bonus += self.apply_rule_score(&rid, score);
            } else {
                bonus += self.apply_rule_score("builtin_bigram", score);
            }
        }

        // Layer 1 & Layer 0: 語法規則索引匹配（雙向倒排索引，O(1) 精準定位）
        let mut candidate_indices: Vec<usize> = Vec::new();
        if let Some(list) = self.rules_by_prev.get(prev_word) {
            candidate_indices.extend_from_slice(list);
        }
        if let Some(list) = self.rules_by_curr.get(curr_word) {
            candidate_indices.extend_from_slice(list);
        }
        if !self.wildcard_rules.is_empty() {
            candidate_indices.extend_from_slice(&self.wildcard_rules);
        }

        // 依照規則原始宣告順序檢查（保持第一條命中的最高優先度特性）
        candidate_indices.sort_unstable();
        candidate_indices.dedup();

        let mut matched_rule: Option<(String, f64)> = None;

        for &rule_idx in &candidate_indices {
            if self.is_rule_disabled(&self.grammar_rules[rule_idx].name) {
                continue;
            }
            let rule = &self.grammar_rules[rule_idx];
            if rule.prev_pattern.matches(prev_word, &self.function_words)
                && rule.curr_pattern.matches(curr_word, &self.function_words)
            {
                if rule.bonus > 0.0
                    && (rule.name.starts_with("user_chain:")
                        || rule.name.starts_with("user_pattern:")
                        || rule.name.starts_with("potential_")
                        || rule.name.starts_with("vo_"))
                    && self
                        .excluded_pairs
                        .contains(&(prev_word.to_string(), curr_word.to_string()))
                {
                    continue;
                }
                // ── 讀音錨定檢查 (Phonetic Anchoring Guards) ──
                // 0. 通用破音虛詞「形音義一體」讀音門檻（防止次要破音如 的(ㄉㄧˋ/ㄉㄧˊ)、會(ㄎㄨㄞˋ/ㄏㄨㄟˇ)、要(ㄧㄠ)、沒(ㄇㄛˋ)、更(ㄍㄥ/ㄐㄧㄥ)、被(ㄆㄧ)、從(ㄘㄨㄥ/ㄗㄨㄥˋ)、與(ㄩˋ)、幾(ㄐㄧ)、句(ㄍㄡ)、那(ㄋㄚˇ)、都(ㄉㄨ)、還(ㄏㄨㄢˊ)、種(ㄓㄨㄥˋ) 冒領語法構式加分）
                if rule.bonus > 0.0 {
                    let is_bad_polyphone = |w: &str, bpm: &str, rname: &str| -> bool {
                        if bpm.is_empty() {
                            return false;
                        }
                        match w {
                            "的" => bpm.starts_with("ㄉㄧ"),
                            "會" => bpm.starts_with("ㄎㄨㄞ") || bpm.starts_with("ㄏㄨㄟˇ") || bpm.starts_with("ㄍㄨㄟ"),
                            "要" => bpm == "ㄧㄠ",
                            "沒" => bpm.starts_with("ㄇㄛ"),
                            "不" => !bpm.starts_with("ㄅㄨ"),
                            "更" => bpm == "ㄍㄥ" || bpm.starts_with("ㄐㄧㄥ"),
                            "被" => bpm.starts_with("ㄆㄧ"),
                            "把" => bpm.starts_with("ㄅㄚˋ") || bpm.starts_with("ㄅㄞ"),
                            "給" => bpm.starts_with("ㄐㄧ"),
                            "從" => bpm == "ㄘㄨㄥ" || bpm.starts_with("ㄗㄨㄥ"),
                            "與" => bpm.starts_with("ㄩˋ") || bpm.starts_with("ㄩˊ"),
                            "和" => bpm.starts_with("ㄏㄜˋ") || bpm.starts_with("ㄏㄨㄛ") || bpm.starts_with("ㄏㄨˊ"),
                            "幾" => bpm == "ㄐㄧ",
                            "句" => bpm.starts_with("ㄍㄡ"),
                            "那" => bpm.starts_with("ㄋㄚˇ") || bpm.starts_with("ㄋㄟˇ") || bpm.starts_with("ㄋㄨㄛ"),
                            "都" => bpm == "ㄉㄨ",
                            "還" => bpm.starts_with("ㄏㄨㄢ") || bpm.starts_with("ㄒㄩㄢ"),
                            "只" => bpm == "ㄓ",
                            "得" => bpm.starts_with("ㄉㄞ"),
                            "種" => bpm.starts_with("ㄓㄨㄥˋ") && rname.contains("classifier"),
                            "間" => bpm.starts_with("ㄐㄧㄢˋ") && rname.contains("classifier"),
                            "處" => bpm.starts_with("ㄔㄨˇ") && (rname.contains("classifier") || rname.contains("localizer")),
                            "著" => (bpm.starts_with("ㄓㄨ") || bpm.starts_with("ㄓㄠ")) && rname.contains("aspect"),
                            "了" => bpm.starts_with("ㄌㄧㄠ") && rname.contains("aspect"),
                            "分" => bpm.starts_with("ㄈㄣˋ") && (rname.contains("potential") || rname.contains("vo_") || rname.contains("user_")),
                            _ => false,
                        }
                    };
                    if is_bad_polyphone(prev_word, prev_bopomofo, &rule.name)
                        || is_bad_polyphone(curr_word, curr_bopomofo, &rule.name)
                    {
                        continue;
                    }
                }

                // 1. 連詞「和」：二元無條件規則僅開放台灣口語連詞讀音 ㄏㄢˋ（讀 ㄏㄜˊ 時由 constructions.rs 三元並列構式驗證前後皆為名詞且不拆碎雙字詞後給分）
                if rule.name.starts_with("nominal_conjunction_he")
                    && !curr_bopomofo.starts_with("ㄏㄢ")
                {
                    continue;
                }
                if rule.name.starts_with("conjunction_he_nominal")
                    && !prev_bopomofo.starts_with("ㄏㄢ")
                {
                    continue;
                }
                // 2. 方位詞「中」：作方位後置詞時必須讀一聲 ㄓㄨㄥ（排除四聲動詞 ㄓㄨㄥˋ 中毒/中獎/全中）
                if curr_word == "中"
                    && (rule.name.contains("localizer")
                        || rule.name.contains("locative")
                        || rule.name.contains("guo_chengzhong"))
                    && !curr_bopomofo.is_empty()
                    && curr_bopomofo != "ㄓㄨㄥ"
                {
                    continue;
                }
                if prev_word == "中"
                    && (rule.name.contains("localizer") || rule.name.contains("locative"))
                    && !prev_bopomofo.is_empty()
                    && prev_bopomofo != "ㄓㄨㄥ"
                {
                    continue;
                }

                // 3. 量詞不能直接修飾「狠」、「快」、帶「了」之完成態動詞、其他純量詞或數詞開頭量化詞（徹底杜絕「個種」、「間粒一堆」量詞連環自激）
                if rule.name == "classifier_noun" {
                    let curr_is_bare_classifier_or_num = matches!(
                        curr_word,
                        "個" | "位" | "隻" | "條" | "把" | "次" | "回" | "種" | "件" | "份"
                            | "句" | "本" | "張" | "塊" | "間" | "棟" | "輛" | "台" | "架"
                            | "支" | "枝" | "朵" | "匹" | "頭" | "尾" | "顆" | "粒" | "片"
                            | "杯" | "碗" | "盤" | "部" | "座" | "所" | "家" | "款" | "項"
                            | "套" | "組" | "段" | "幢" | "艘" | "幅" | "首" | "尊" | "面"
                            | "扇" | "門" | "臺" | "冊" | "堂" | "列" | "班" | "口" | "堆"
                            | "批" | "群" | "雙" | "對" | "副"
                    ) || WordPattern::is_numeric_token(curr_word)
                        || curr_word.starts_with([
                            '一', '二', '兩', '三', '四', '五', '六', '七', '八', '九', '十',
                            '幾', '半', '這', '那', '哪', '每', '各',
                        ]);
                    if curr_is_bare_classifier_or_num
                        || curr_word == "狠"
                        || curr_word == "快"
                        || (curr_word.ends_with("了") && curr_word.chars().count() >= 2)
                        || (prev_word == "回"
                            && matches!(
                                curr_word,
                                "付" | "覆" | "走" | "來" | "去" | "看" | "買" | "賣" | "說" | "寫"
                            ))
                        || (prev_word == "班"
                            && !matches!(
                                curr_word,
                                "公車" | "客運" | "列車" | "火車" | "捷運" | "高鐵" | "班機"
                                    | "飛機" | "船" | "渡輪" | "校車" | "專車" | "車" | "學生"
                                    | "同學" | "人馬" | "士兵" | "戰士" | "幹部" | "兵"
                            ))
                    {
                        continue;
                    }
                }

                // 4. 程度副詞修飾單字時，必須為可分級形容詞
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

                // 5. 時間後置標記「時」不能接在數量詞、指示詞或程度副詞之後
                if rule.name == "temporal_event_shi" {
                    let non_temporal_prev = [
                        "很多", "許多", "不少", "一些", "幾個", "無數", "非常", "太", "特別", "這", "那", "的",
                    ];
                    if non_temporal_prev.contains(&prev_word) {
                        continue;
                    }
                }

                // 6. 句末疑問助詞「嗎」：必須讀輕聲 ˙ㄇㄚ 或一聲 ㄇㄚ，排除三聲 ㄇㄚˇ（嗎啡）
                if rule.name == "sentence_final_particle" && curr_word == "嗎" && curr_bopomofo.contains('ˇ') {
                    continue;
                }

                // 7. 數詞與單位量詞構式 (numeric_unit_classifier) 之破音字讀音錨定與熟語保護：
                if rule.name == "numeric_unit_classifier" {
                    // 中文數詞（非阿拉伯數字）後接單字單位：消融實測改善 0 句、損害 8 句，一律交由 numeral_classifier (+15.0) 處理，
                    // +26.0 數值單位錨定僅保留給阿拉伯數字（如 2026年、100元）與多字單位（如 公里、小時）
                    if !prev_word.chars().any(|c| c.is_ascii_digit()) && curr_word.chars().count() == 1 {
                        continue;
                    }
                    // 現代漢語中，位數詞「百、千、萬、億、兆」不可在無係數前綴（如 一千、數千、上萬）時單獨直接修飾個體量詞（如 *千筆、*百個、*萬支）；
                    // 攔截裸位數詞 + 個體量詞之單字加分，嚴防「千+筆」(+26.0) 拆碎並搶佔既有雙字核心詞「鉛筆 (ㄑㄧㄢ ㄅㄧˇ)」！
                    if matches!(prev_word, "百" | "千" | "萬" | "億" | "兆")
                        && matches!(
                            curr_word,
                            "筆" | "個" | "支" | "枝" | "根" | "張" | "本" | "冊" | "件" | "套"
                                | "雙" | "對" | "隻" | "條" | "顆" | "粒" | "杯" | "碗" | "盤"
                                | "瓶" | "罐" | "盒" | "包" | "袋" | "箱" | "台" | "部" | "輛"
                                | "架" | "艘" | "座" | "間" | "棟" | "分"
                        )
                    {
                        continue;
                    }
                    // 中文數詞（一、二、三、四...）後接單字「升」不套用容量單位加分（防「三聲」被「三升」搶佔；阿拉伯數字「3升」或雙字「公升/毫升」仍正常加分）
                    if curr_word == "升" && !prev_word.chars().any(|c| c.is_ascii_digit()) {
                        continue;
                    }
                    if !curr_bopomofo.is_empty() {
                        let polyphonic_unit_invalid = match curr_word {
                            "度" => !curr_bopomofo.starts_with("ㄉㄨˋ"),
                            "行" => !curr_bopomofo.starts_with("ㄏㄤˊ"),
                            "處" => !curr_bopomofo.starts_with("ㄔㄨˋ"),
                            "間" => curr_bopomofo != "ㄐㄧㄢ",
                            "種" => !curr_bopomofo.starts_with("ㄓㄨㄥˇ"),
                            "轉" => !curr_bopomofo.starts_with("ㄓㄨㄢˋ"),
                            "卷" => !curr_bopomofo.starts_with("ㄐㄩㄢˋ"),
                            "載" => !curr_bopomofo.starts_with("ㄗㄞˇ"),
                            "兩" => !curr_bopomofo.starts_with("ㄌㄧㄤˇ"),
                            "折" => !curr_bopomofo.starts_with("ㄓㄜˊ"),
                            "成" => !curr_bopomofo.starts_with("ㄔㄥˊ"),
                            "分" => curr_bopomofo != "ㄈㄣ",
                            "斗" => !curr_bopomofo.starts_with("ㄉㄡˇ"),
                            "石" => !curr_bopomofo.starts_with("ㄉㄢˋ") && !curr_bopomofo.starts_with("ㄕˊ"),
                            _ => false,
                        };
                        if polyphonic_unit_invalid {
                            continue;
                        }
                    }
                    // 若前驅為明確的阿拉伯數字（如 2026年、10月、15日、100元、50坪、37度），額外強化數值單位錨定力
                    let mut extra = 0.0;
                    if prev_word.chars().any(|c| c.is_ascii_digit()) {
                        extra += 12.0;
                    }
                    if matches!(curr_word, "克" | "坪") {
                        extra += 6.0;
                    }
                    matched_rule = Some((rule.name.clone(), rule.bonus + extra));
                    break;
                }

                matched_rule = Some((rule.name.clone(), rule.bonus));
                break; // 只取第一條匹配的規則（避免多條規則重複加分）
            }
        }

        if let Some((rname, rscore)) = matched_rule {
            bonus += self.apply_rule_score(&rname, rscore);
        }

        // ── 形態學與音韻特徵動態守門規則 (Morphological & Phonetic Guards) ──
        // 1. 派生名詞後綴「~庫 (ㄎㄨˋ)」加權與古姓氏「庫 (ㄕㄜˋ)」隔離
        if curr_word == "庫" {
            if curr_bopomofo.contains("ㄎㄨ")
                && !self.function_words.contains(prev_word)
                && !WordPattern::is_numeric_token(prev_word)
                && prev_word.chars().count() >= 2
                && prev_word.chars().count() <= 4
            {
                bonus += self.apply_rule_score("morph_suffix_ku", 16.0);
            } else if curr_bopomofo.contains("ㄕㄜ") {
                bonus += self.apply_rule_score("guard_ku_she_surname", -35.0);
            }
        }

        // 2. 派生通用名詞後綴：僅保留古音隔離守門（移除無條件 +14.0 加權，防止單字「感/度/化/性/師/員/器」拆碎後接雙字詞如「感到、失去、原來、氣氛、幸福、變化」）
        if curr_word == "感" && curr_bopomofo.contains("ㄏㄢ") {
            bonus += self.apply_rule_score("guard_gan_han_polyphone", -35.0);
        }
        // 單字連詞「和」僅讀 ㄏㄜˊ 或 ㄏㄢˋ，破音讀 ㄏㄨㄛˋ/ㄏㄨㄛˊ/ㄏㄨˊ/ㄏㄜˋ 時嚴禁單獨成詞搶佔選擇連詞「或 (ㄏㄨㄛˋ)」或名詞「貨/禍/獲」
        if curr_word == "和"
            && (curr_bopomofo.starts_with("ㄏㄨㄛ")
                || curr_bopomofo.starts_with("ㄏㄨˊ")
                || curr_bopomofo.starts_with("ㄏㄜˋ"))
        {
            bonus += self.apply_rule_score("guard_he_huo_polyphone", -35.0);
        }
        if prev_word == "和"
            && (prev_bopomofo.starts_with("ㄏㄨㄛ")
                || prev_bopomofo.starts_with("ㄏㄨˊ")
                || prev_bopomofo.starts_with("ㄏㄜˋ"))
        {
            bonus += self.apply_rule_score("guard_he_huo_polyphone", -35.0);
        }

        // 3. 疊字後接修飾標記 (AA+的, AA+地, AABB+的, AABB+地)
        if (curr_word == "的" || curr_word == "地") && prev_word.chars().count() >= 2 {
            let chars: Vec<char> = prev_word.chars().collect();
            let is_reduplicated = (chars.len() == 2 && chars[0] == chars[1])
                || (chars.len() == 4 && chars[0] == chars[1] && chars[2] == chars[3]);
            if is_reduplicated {
                bonus += self.apply_rule_score("morph_reduplicated_de", 18.0);
            }
        }

        // 4. 修飾語接中心名詞「話」與防「的+化」
        if curr_word == "話" && (prev_word == "的" || prev_word.ends_with("的") || prev_word == "普通") {
            bonus += self.apply_rule_score("morph_modifier_hua", 26.0);
        }
        if curr_word == "化" && (prev_word == "的" || prev_word.ends_with("的")) {
            bonus += self.apply_rule_score("guard_de_hua_suffix", -35.0);
        }

        // 5. 比擬後綴「~般 (ㄅㄢ)」加權
        if curr_word == "般"
            && curr_bopomofo.contains("ㄅㄢ")
            && !self.function_words.contains(prev_word)
            && prev_word.chars().count() >= 2
            && prev_word.chars().count() <= 4
        {
            bonus += self.apply_rule_score("morph_simile_ban", 26.0);
        }

        // 6. 跨語系副檔名與媒體類別後置「檔」防政治「黨」劫掠
        if curr_word == "檔" || curr_word == "黨" {
            let is_ascii_word = !prev_word.is_empty()
                && prev_word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            let is_political_abbr = matches!(prev_word, "kmt" | "dpp" | "ccp" | "gop");
            if (is_ascii_word && !is_political_abbr) || matches!(prev_word, "圖" | "音" | "影片" | "封存") {
                if curr_word == "檔" {
                    bonus += self.apply_rule_score("morph_media_file_dang", 35.0);
                } else {
                    bonus += self.apply_rule_score("guard_media_file_dang_party", -35.0);
                }
            }
        }

        // 7. 全稱限定詞「任何」後接名詞／實詞雙向語法構式（排除標點與純助詞），以及三聲「ㄋㄚˇ」嚴禁出指代詞「那」
        if prev_word == "任何" && !curr_word.is_empty() && !matches!(curr_word, "的" | "了" | "嗎" | "呢" | "吧" | "啊") {
            bonus += self.apply_rule_score("morph_renhe_nominal", 32.0);
        }
        if (curr_word == "那" || curr_word.starts_with('那')) && curr_bopomofo.starts_with("ㄋㄚˇ") {
            bonus += self.apply_rule_score("guard_na3_tone", -35.0);
        }

        bonus
    }
}
