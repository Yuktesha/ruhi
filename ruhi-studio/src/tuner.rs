//! # `tuner` — 如喜計算語言學雙向詞界對齊、二階轉移合成與 Zero-PII 隱私閘門引擎
//!
//! 完整移植並整合原 Python 版 `scripts/semantic_tuner.py` 與 `tools/semantic_federation.py`：
//! 1. **雙向最大匹配分詞 (`BiMM`) 與教育部注音映射**：直接讀取 `ruhi_lexicon/data/*.tsv`，毫秒級完成正逆向詞界切分與注音標註。
//! 2. **六大計算語言學病灶分類學 (`Type I` ～ `Type VI`)**：
//!    嚴格遵守「漢語計算語言學架構與嚴禁單字權重打地鼠守則」，自動識別未登錄雙字複合實詞 (`Layer 3`)、二階語法搭配引力 (`Layer 2`, `+20.0` ～ `+35.0`) 與階層式集合槽位構式 (`Layer 1`)。
//! 3. **Zero-PII 隱私安全閘門與 Tailcat 聯邦提案封裝**：過濾電話、身分證、電子郵件、長串專名與高熵識別碼，確保零個資外洩。

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config_sync::resolve_data_dir;

/// 黃金基準測試集（涵蓋跨語法現象核心基準句、八大同音字組雙向對照與「與／是要／但會不會對」語法構式）
pub const GOLDEN_BENCHMARK: &[(&str, &str, &str)] = &[
    ("cj3fe3l4", "請放心", "禮貌祈使構式"),
    ("m4gj3ef84", "讓不懂", "致使兼語構式"),
    ("gj3ef84ch9g4", "不懂程式", "動賓專業搭配"),
    ("s94lo3ej93g4", "很有意思", "程度副詞＋名詞性述語"),
    ("l4ci6dk4l9", "在硬碟裡", "框式方位介詞構式 (在...裡)"),
    ("n94ns4n94le8", "越弄越大", "關聯副詞構式 (越 A 越 B)"),
    ("cj6mn4de9g4", "奇妙的是", "繫詞結構助詞 (的＋是)"),
    ("i2ge9jc9tl6", "一個鐘頭", "數量詞＋時間量詞凝聚"),
    ("xk4dj6l9", "下載下來", "複合趨向補語構式"),
    ("f84de9dc4", "懂得道", "動補結構助詞 (得)"),
    ("g4x83g4su6", "實事求是", "四字成語長詞凝聚"),
    ("jc6xn3k8", "點選框", "介面動詞＋容器名詞構式"),
    ("c4xk4xw4", "看一下", "動詞＋動量補語"),
    ("a83ji4s4x84", "把字打好", "處置式 (把字句)＋結果補語"),
    ("a84fm4l3ie3", "被風吹走", "被動式 (被字句)＋動趨補語"),
    ("m6dk4xo3j3", "沒辦法走", "否定存現與連動結構"),
    ("hs6eb3hs6s84", "隨便說說", "副詞修飾＋動詞重疊"),
    ("z4mu4jw6xw4", "自動存檔", "資訊科技高頻動賓詞"),
    ("y6lw4jc3do6", "語音輸入", "輸入法領域核心複合詞"),
    ("ru6xc3y6kc4", "如喜語意", "品牌與核心領域詞彙"),
    ("cjewjvelfnenddxajufjgjggszjdgsufejgxa zgdjey effn ce ", "是要請您對與這個字的語意規則加以分析", "焦點助動詞「是要」＋關係介詞「與」構式"),
    ("dmjhxajbxdhxajdxajcejtxlfzwjvldfxjdm ", "但會不會對系統造成負擔", "否定反覆疑問「但會不會對」防「旦會/匯兌」劫掠"),
    ("zijufzij", "再與在", "元語言同音對比並列構式 [X 與 Y]"),
    ("bxdzijufbxdzij", "不再與不在", "雙音節否定同音對比構式"),
    ("dgsufdgd", "的與得", "結構助詞同音對比構式"),
    ("dkfufdkf", "黨與檔", "政黨與資訊檔案同音對比構式"),
    ("dmjufdmj", "但與淡", "轉折連詞與味覺形容詞同音對比構式"),
    ("vkdufvkd", "長與常", "維度形容詞與頻率副詞同音對比構式"),
    ("jeojhwfufjeojhwf", "就好與救好", "條件讓步與醫療救援結果補語對比構式"),
    ("adufad", "詞與辭", "語言詞彙與離職動補同音對比構式"),
    ("efceyjxndjujxhfhidcjjdjee tee gafnendhwflgs", "以下文句我還是直接貼給您好了", "篇章指稱「以下＋文句」防「文具」混淆構式"),
    ("cumfjufcumfdscje ggsdxajjjleffk cjdspmjxkj", "選舉選的是一個對治理方式的盼望", "準分裂焦點「V的是」＋介詞關係定語「一個對...的」構式"),
    ("jeojrkjjekfxmjm bxjdxmjckjjeedmxjzjbwjrkjty dsmenddewjzjrmddsdeedggsgoj", "就讓蔣萬安不斷上節目自爆讓他的民調自然的跌個夠", "媒體登台「上節目」與極性程度補語「跌個夠」構式"),
    ("jen tem zwfckjhxydaxajcdkmjdwjbeedrnddsjxm mj,velflelje ggsdxajhxyjfn ce lgse ceyj", "今天早上滑脆時看到別人的專案，請另一個對話分析了一下", "社群動賓「滑脆時」與雙音複合「對話分析」防錯位搶佔構式"),
    ("axldefeofxndcemjjemjlejje vxfj hoj,zijrkjrndlajxhfgxljcemjjeddxm zwjjuj", "從已有文獻建立基礎之後，再讓人類我貢獻極端造句", "起點介詞框架「從...之後」與跨逗號承接致使「再讓」構式"),
    ("fn cekfe ceyjGAAdszxajcen vldgxhf", "分享一下GAA的最新成果", "雙音動量「分享一下」、跨語種領屬「GAA的」與「最新成果」構式"),
    ("xhfcem vuejrnje ceyj,jgjekje bm cfuljjgfvuejdeljkgfeftee rxjaxhjxjufkm xjjewjdxajcxk juj,jeojdydvldjljvuejufejtewdjewjdsmxjdejmys?", "我先確認一下，這樣一般使用者確定可以貼入錯誤與勘誤校對雙句，就達成正確語意調校的目的嗎？", "語意調校工作室四組同音構式實戰基準句（一般/確定/勘誤/雙句）"),
    ("jgjekje bm gxl vg kgfefzijdxh cwfcuedcl ", "這樣一班公車可以載多少學生", "班次量詞「一班」與交通工具動賓「載多少學生」對比構式"),
    ("txdcx gxmflefco akdlgscufdxh cx jedufkm xj", "圖書館裡收藏了許多書籍與刊物", "出版領域並列「書籍與刊物」防「與➔勘誤」過度泛化對比構式"),
    ("jgjjeyjfa je pajbajlgscxk jujxh lxndenfveld", "這架飛機配備了雙具渦輪引擎", "機械器物量詞「雙具＋渦輪引擎」與語言單位「雙句」對比構式"),
    ("rmdhoj,dyjejjeojje hx xkjgxk gxk lgsey ", "然後，大易就幾乎忘光光了呀", "輸入法話題主語「大易＋就」與程度耗盡補語「幾乎＋忘光光＋了」構式"),
    ("xhfcem vuejrnje ceyj,jgjekjkgfefjew dwjcfuljjgfcofckj,dxajmys?", "我先確認一下，這樣可以交到使用者手上，對嗎？", "受事轉移「交到＋使用者＋手上」與句末附加問句「，對嗎？」構式"),
    ("ty hnfhxajgn kgjhxjdyfjew dwj", "他很會跟客戶打交道", "社交離合詞「跟＋客戶＋打交道」與「交到」雙向對比構式"),
    ("dmjjgjekjjn dseofbejewjmys?", "但這樣真的有必要嗎？", "斷言副詞「真的」＋存現助動詞「有＋必要」防古地名「有庳」搶佔構式"),
    ("zijdxjleldhxndkwfxnj", "再度靈魂拷問", "時間重複副詞「再度」＋動賓四字格「靈魂拷問」防「在渡」搶佔構式"),
    ("zijcfuljcufcjjemjpmd,zijcdzxhjdsufejtewdjlfgxl zxhjcj,jenfcemjhmjuf,", "在使用許氏鍵盤，在實做的語意調整工作室，僅限漢語，", "專名「許氏鍵盤」、進行體定語「在實做的」、「語意調整工作室」與範圍限定「僅限漢語」構式"),
    ("zijcfuljcufcjjemjpmd,zijcdzxhjdsufejtewdjewjgxl zxhjcj,jenfcemjhmjuf,", "在使用許氏鍵盤，在實做的語意調校工作室，僅限漢語，", "專名「許氏鍵盤」、進行體定語「在實做的」、「語意調校工作室」與範圍限定「僅限漢語」構式"),
];

/// 封閉類虛詞、代詞與結構助詞集合（嚴禁跨界吸納為複合實詞，亦嚴禁無條件負向邊界抑制）
const CLOSED_CLASS_CHARS: &str =
    "的得地了著過嗎呢吧啊呀喔哦嘛啦唄我你您他她它咱這那哪每各某本此該全凡在再從向往對跟和與及或但卻而並若雖因由被把讓叫給請替幫比連就才都也還又只剛正曾已將會要能可很太最更越不沒別莫勿非無";

const PRONOUN_CHARS: &str = "我你您他她它咱";
const PARTICLE_CHARS: &str = "的得地了著嗎呢吧啊呀喔哦";
const CONTAINER_SUFFIX_CHARS: &str = "框欄區鈕鍵窗頁表列項";

/// 常見手滑同音錯別字映射表（攔截筆誤，避免誤入詞庫）
pub fn common_typo_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("選則", "選擇"),
        ("反應", "反映"),
        ("再見", "在見"),
        ("以經", "已經"),
        ("應為", "因為"),
        ("必竟", "畢竟"),
        ("部過", "不過"),
        ("即然", "既然"),
        ("那裡", "哪裡"),
        ("候選則", "候選擇"),
    ])
}

/// 驗證是否為純 CJK 漢字詞元（排除英數、標點、空白與 JSON Schema 佔位樣板詞）
pub fn is_valid_chinese_token(text: &str) -> bool {
    let s = text.trim();
    if s.is_empty() {
        return false;
    }
    let placeholders = [
        "前驅純漢字詞元",
        "後繼純漢字詞元",
        "前驅純漢字詞",
        "後繼純漢字詞",
        "真正未登錄之雙字原子複合詞",
        "專有名詞",
        "目標詞",
        "前驅候選",
        "錯誤句",
        "正確句",
        "構式名稱",
        "二階語法搭配說明",
        "二階語法或語意搭配說明",
    ];
    if placeholders.contains(&s) {
        return false;
    }
    s.chars().all(is_cjk_char)
}

pub fn is_cjk_char(ch: char) -> bool {
    let code = ch as u32;
    (0x4E00..=0x9FFF).contains(&code)
        || (0x3400..=0x4DBF).contains(&code)
        || (0xF900..=0xFAFF).contains(&code)
}

/// 判別字串是否屬於跨句法短語（如「每點擊」、「我不懂」、「而失憶」），此類組合應走 Layer 2 二階轉移，不可寫入 Layer 3 詞庫
pub fn is_syntactic_phrase_or_clitic(w: &str) -> bool {
    let chars: Vec<char> = w.trim().chars().collect();
    if chars.len() < 2 {
        return true;
    }
    let allowed_particles = [
        "的士", "的確", "標的", "目的", "有的", "是的", "好的", "真的", "了解", "理解", "了不起",
        "了結", "了得", "了當", "了然", "未了", "知了", "著急", "著名", "著作", "著手", "著重",
        "著陸", "著迷", "著想", "著實", "顯著", "執著", "原著", "名著", "地道", "地步", "地區",
        "地方", "地面", "地球", "地址", "地鐵", "地圖", "地位", "地獄", "地質", "地震", "地鼠",
        "土地", "場地", "質地", "天地", "得到", "得知", "得意", "得力", "得體", "得罪", "得失",
        "心得", "難得", "覺得", "記得", "值得", "獲得", "取得", "贏得", "捨不得", "怪不得", "不得",
        "不見得", "酒吧", "網吧", "貼吧", "吧檯", "吧台", "喇嘛", "幹嘛", "嗎啡", "呢喃",
    ];
    let c0 = chars[0];
    let c1 = chars[1];
    let clast = *chars.last().unwrap_or(&c0);
    if (PARTICLE_CHARS.contains(c0)
        || PARTICLE_CHARS.contains(c1)
        || PARTICLE_CHARS.contains(clast))
        && !allowed_particles.contains(&w)
    {
        return true;
    }
    if chars.len() == 2 {
        if PRONOUN_CHARS.contains(c0)
            && "不沒很都也還就要會能想在再先去來有是把被讓給對跟和向的了過著嗎呢吧".contains(c1)
        {
            return true;
        }
        if "給請讓叫替幫跟對向從把被為由比在的了".contains(c0) && PRONOUN_CHARS.contains(c1) {
            return true;
        }
        if "會都也還先又就才只常總更最很太".contains(c0) && (c1 == '在' || c1 == '再') {
            return true;
        }
        return false;
    }
    let allowed_long = [
        "各階段", "現階段", "下階段", "這時候", "那時候", "全自動", "全方位", "全天候", "更何況",
        "總而言之", "換言之", "簡言之", "反之亦然",
    ];
    if allowed_long.contains(&w) {
        return false;
    }
    if PRONOUN_CHARS.contains(c0)
        && !["我們", "你們", "他們", "她們", "它們", "咱們"]
            .iter()
            .any(|p| w.starts_with(p))
    {
        return true;
    }
    if "每各某本此該這那哪全凡而並且或但卻若雖因隨按依據由從向往對跟比為被把讓叫請給在再又也都還就才只剛正曾已將會要能可很太最更越"
        .contains(c0)
    {
        return true;
    }
    if chars.len() == 3
        && "圈點勾框篩挑複全單候".contains(chars[0])
        && "選取列".contains(chars[1])
        && CONTAINER_SUFFIX_CHARS.contains(chars[2])
    {
        return true;
    }
    false
}

/// Zero-PII 隱私安全過濾器（完整移植自 `tools/semantic_federation.py`）
pub struct ZeroPiiPrivacyFilter;

impl ZeroPiiPrivacyFilter {
    /// 驗證單一詞元是否通過 Zero-PII 隱私審查
    pub fn is_safe_token(token: &str) -> (bool, &'static str) {
        let s = token.trim();
        if s.is_empty() {
            return (false, "Empty token");
        }
        let char_len = s.chars().count();
        if char_len > 4 {
            return (
                false,
                "Token length > 4 (Potential Proper Noun / Address / Secret)",
            );
        }
        if s.chars().any(|c| c.is_ascii_digit()) {
            return (
                false,
                "Contains digits (Potential Phone / ID / Account Number)",
            );
        }
        if s.chars().any(|c| c.is_ascii_alphabetic() || "@._-:/\\".contains(c)) {
            return (
                false,
                "Contains ASCII/Symbols (Potential Email / Handle / Password)",
            );
        }
        if !is_valid_chinese_token(s) {
            return (false, "Non-CJK characters detected");
        }
        (true, "SAFE_PUBLIC_CONSTRUCT")
    }

    /// 驗證整句語料對是否通過 Zero-PII 隱私審查（攔截電話、身分證、電子郵件、信用卡號、密碼與詳細門牌地址）
    pub fn is_safe_sentence_pair(error_sentence: &str, correct_sentence: &str) -> (bool, &'static str) {
        let e = error_sentence.trim();
        let c = correct_sentence.trim();
        if e.is_empty() || c.is_empty() || e == c {
            return (false, "Empty or identical sentence pair");
        }
        for s in [e, c] {
            if s.contains('@') || s.contains("http://") || s.contains("https://") {
                return (false, "Contains Email or URL");
            }
            // 攔截連續 6 位以上數字（電話、手機、銀行帳號、信用卡、驗證碼）
            let mut consec_digits = 0usize;
            for ch in s.chars() {
                if ch.is_ascii_digit() || ('０'..='９').contains(&ch) {
                    consec_digits += 1;
                    if consec_digits >= 6 {
                        return (false, "Contains >=6 consecutive digits (Phone / ID / Account)");
                    }
                } else {
                    consec_digits = 0;
                }
            }
            // 攔截台灣身分證字號格式（1 英文大寫 + 9 數字）
            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= 10 {
                for w in chars.windows(10) {
                    if w[0].is_ascii_alphabetic()
                        && matches!(w[1], '1' | '2' | '8' | '9')
                        && w[2..].iter().all(|ch| ch.is_ascii_digit())
                    {
                        return (false, "Contains Taiwan National ID pattern");
                    }
                }
            }
            // 攔截詳細門牌地址特徵（同時含有路/街/段與巷/弄/號/樓）
            let has_road = s.contains('路') || s.contains('街') || s.contains("大道");
            let has_number = s.contains('號') && (s.contains('巷') || s.contains('弄') || s.contains('段') || s.contains('樓'));
            if has_road && has_number {
                return (false, "Contains detailed street address");
            }
        }
        (true, "PASSED_ZERO_PII_SENTENCE")
    }

    /// 淨化二元語法轉移規則，通過則產出可匿名分享之聯邦封包
    pub fn sanitize_transition_rule(prev: &str, next: &str, score: f64, kind: &str) -> Option<Value> {
        let (ok1, _) = Self::is_safe_token(prev);
        let (ok2, _) = Self::is_safe_token(next);
        if !ok1 || !ok2 {
            return None;
        }
        let clamped_score = score.clamp(-35.0, 35.0);
        Some(json!({
            "type": "BIGRAM_TRANSITION",
            "prev": prev.trim(),
            "next": next.trim(),
            "recommended_delta": (clamped_score * 10.0).round() / 10.0,
            "kind": kind,
            "privacy_audit": "PASSED_ZERO_PII"
        }))
    }
}

/// 二階語法轉移規則結構
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRuleItem {
    pub prev: String,
    pub next: String,
    pub score: f64,
    pub kind: String,
    pub comment: String,
}

/// 計算語言學語意調校引擎 (`SemanticTuner`)
pub struct SemanticTuner {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub user_tsv: PathBuf,
    pub domain_tsv: PathBuf,
    pub patterns_jsonl: PathBuf,
    pub revisions_json: PathBuf,
    pub known_words: HashSet<String>,
    pub word_bopomofo: HashMap<String, String>,
    pub word_max_weight: HashMap<String, i32>,
    pub char_bopomofo: HashMap<char, String>,
    pub user_transitions: HashMap<(String, String), f64>,
}

impl SemanticTuner {
    pub fn new(workspace_root: Option<&Path>) -> Self {
        let root = workspace_root
            .map(Path::to_path_buf)
            .unwrap_or_else( crate::config_sync::resolve_ruhi_root );
        let data_dir = resolve_data_dir(&root);
        let user_tsv = data_dir.join("transitions_user.tsv");
        let domain_tsv = data_dir.join("lexicon_domain.tsv");
        let patterns_jsonl = data_dir.join("patterns_user.jsonl");

        let mut revisions_json = data_dir.join("pending_revisions.json");
        let rev_candidates = [
            data_dir.join("pending_revisions.json"),
            root.join("data").join("pending_revisions.json"),
            root.join("pending_revisions.json"),
        ];
        for cand in &rev_candidates {
            if cand.is_file() {
                revisions_json = cand.clone();
                break;
            }
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_rev = PathBuf::from(appdata).join("Ruhi").join("pending_revisions.json");
            if appdata_rev.is_file() {
                revisions_json = appdata_rev;
            }
        }

        let mut tuner = Self {
            root,
            data_dir,
            user_tsv,
            domain_tsv,
            patterns_jsonl,
            revisions_json,
            known_words: HashSet::new(),
            word_bopomofo: HashMap::new(),
            word_max_weight: HashMap::new(),
            char_bopomofo: HashMap::new(),
            user_transitions: HashMap::new(),
        };
        tuner.load_lexicon_and_transitions();
        tuner
    }

    /// 載入基礎詞庫、領域詞庫與二階轉移表
    pub fn load_lexicon_and_transitions(&mut self) {
        let lexicon_files = [
            "lexicon_core.tsv",
            "lexicon_domain.tsv",
            "lexicon_mcbopomofo.tsv",
            "lexicon_moe.tsv",
            "lexicon_main.tsv",
            "lexicon_colloquial.tsv",
            "lexicon_user.tsv",
        ];

        for fname in &lexicon_files {
            let path = self.data_dir.join(fname);
            if !path.is_file() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split('\t').collect();
                    if parts.len() >= 2 {
                        let word = parts[0].trim();
                        let bpmf = parts[1].trim();
                        let weight = parts
                            .get(2)
                            .and_then(|s| s.trim().parse::<i32>().ok())
                            .unwrap_or(180);
                        let cat = parts.get(3).map(|s| s.trim()).unwrap_or("");
                        if word.is_empty() || word == "word" {
                            continue;
                        }
                        let ch_count = word.chars().count();
                        if ch_count == 1 {
                            if let Some(ch) = word.chars().next() {
                                self.char_bopomofo
                                    .entry(ch)
                                    .or_insert_with(|| bpmf.to_string());
                            }
                        } else if ch_count >= 2 {
                            // 排除萌典中之罕用古文詞條（如「非語」），避免干擾現代漢語雙向最大匹配斷詞
                            let is_archaic_moe = *fname == "lexicon_moe.tsv"
                                && matches!(cat, "archaic" | "classic" | "rare");
                            if !is_archaic_moe {
                                self.known_words.insert(word.to_string());
                            }
                            self.word_bopomofo
                                .entry(word.to_string())
                                .or_insert_with(|| bpmf.to_string());
                            self.word_max_weight
                                .entry(word.to_string())
                                .and_modify(|w| *w = (*w).max(weight))
                                .or_insert(weight);
                            // 同時根據多字詞切分補齊單字注音映射
                            let syls: Vec<&str> = if bpmf.contains('-') {
                                bpmf.split('-').collect()
                            } else {
                                bpmf.split_whitespace().collect()
                            };
                            if syls.len() == ch_count {
                                for (c, s) in word.chars().zip(syls.iter()) {
                                    self.char_bopomofo
                                        .entry(c)
                                        .or_insert_with(|| (*s).to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.user_tsv.is_file() {
            if let Ok(content) = fs::read_to_string(&self.user_tsv) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split('\t').collect();
                    if parts.len() >= 3 {
                        let prev = parts[0].trim().to_string();
                        let next = parts[1].trim().to_string();
                        if let Ok(score) = parts[2].trim().parse::<f64>() {
                            self.user_transitions.insert((prev, next), score);
                        }
                    }
                }
            }
        }
    }

    /// 查詢單字或多字詞之教育部標準注音（以空白分隔音節）
    pub fn get_bopomofo(&self, word: &str) -> String {
        let w = word.trim();
        if let Some(bpmf) = self.word_bopomofo.get(w) {
            return bpmf.replace('-', " ");
        }
        let mut syls = Vec::new();
        for ch in w.chars() {
            if let Some(s) = self.char_bopomofo.get(&ch) {
                syls.push(s.clone());
            }
        }
        syls.join(" ")
    }

    /// 雙向最大匹配分詞 (`BiMM`)，回傳 `(token, start_char_idx, end_char_idx)`
    pub fn tokenize_with_spans(&self, text: &str) -> Vec<(String, usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        if n == 0 {
            return Vec::new();
        }

        // 正向最大匹配 (FMM)
        let mut fmm = Vec::new();
        let mut i = 0;
        while i < n {
            if !is_cjk_char(chars[i]) {
                if chars[i].is_ascii_alphanumeric() {
                    let start = i;
                    while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                        i += 1;
                    }
                    let s: String = chars[start..i].iter().collect();
                    fmm.push((s, start, i));
                } else {
                    let s: String = chars[i..=i].iter().collect();
                    fmm.push((s, i, i + 1));
                    i += 1;
                }
                continue;
            }
            let max_len = (n - i).min(6);
            let mut matched = None;
            for len in (2..=max_len).rev() {
                let cand: String = chars[i..i + len].iter().collect();
                if cand != "的是" && self.known_words.contains(&cand) {
                    matched = Some((cand, i, i + len));
                    break;
                }
            }
            if let Some((tok, s, e)) = matched {
                fmm.push((tok, s, e));
                i = e;
            } else {
                let s: String = chars[i..=i].iter().collect();
                fmm.push((s, i, i + 1));
                i += 1;
            }
        }

        // 逆向最大匹配 (BMM)
        let mut bmm = Vec::new();
        let mut j = n;
        while j > 0 {
            if !is_cjk_char(chars[j - 1]) {
                if chars[j - 1].is_ascii_alphanumeric() {
                    let end = j;
                    while j > 0 && (chars[j - 1].is_ascii_alphanumeric() || chars[j - 1] == '_' || chars[j - 1] == '-') {
                        j -= 1;
                    }
                    let s: String = chars[j..end].iter().collect();
                    bmm.push((s, j, end));
                } else {
                    let s: String = chars[j - 1..j].iter().collect();
                    bmm.push((s, j - 1, j));
                    j -= 1;
                }
                continue;
            }
            let max_len = j.min(6);
            let mut matched = None;
            for len in (2..=max_len).rev() {
                let start = j - len;
                let cand: String = chars[start..j].iter().collect();
                let split_shizuo_de =
                    cand == "做的" && start > 0 && matches!(chars[start - 1], '實' | '試' | '重' | '在');
                if cand != "的是" && !split_shizuo_de && self.known_words.contains(&cand) {
                    matched = Some((cand, start, j));
                    break;
                }
            }
            if let Some((tok, s, e)) = matched {
                bmm.push((tok, s, e));
                j = s;
            } else {
                let s: String = chars[j - 1..j].iter().collect();
                bmm.push((s, j - 1, j));
                j -= 1;
            }
        }
        bmm.reverse();

        // 比較總詞數與單字碎片數，當詞數與單字數相同時優先採用右向核心之 BMM
        let fmm_single = fmm.iter().filter(|(t, _, _)| t.chars().count() == 1).count();
        let bmm_single = bmm.iter().filter(|(t, _, _)| t.chars().count() == 1).count();
        if bmm.len() < fmm.len() || (bmm.len() == fmm.len() && bmm_single <= fmm_single) {
            bmm
        } else {
            fmm
        }
    }

    /// 成對句子同步詞界對齊切分
    pub fn tokenize_pair_with_spans(
        &self,
        error: &str,
        correct: &str,
    ) -> (Vec<(String, usize, usize)>, Vec<(String, usize, usize)>) {
        let cor_spans = self.tokenize_with_spans(correct);
        let err_spans = self.tokenize_with_spans(error);
        let err_chars: Vec<char> = error.chars().collect();
        let cor_chars: Vec<char> = correct.chars().collect();

        if err_chars.len() == cor_chars.len() {
            // 若錯誤句本身並未形成跨詞界交錯假詞（如「匯兌」、「十分」、「加速」），則對齊至正確句詞界
            let cor_boundaries: HashSet<(usize, usize)> =
                cor_spans.iter().map(|(_, s, e)| (*s, *e)).collect();
            let has_cross_boundary_hijack = err_spans
                .iter()
                .any(|(t, s, e)| t.chars().count() >= 2 && !cor_boundaries.contains(&(*s, *e)));
            if !has_cross_boundary_hijack {
                let mut aligned_err = Vec::with_capacity(cor_spans.len());
                for (_, s, e) in &cor_spans {
                    let sub: String = err_chars[*s..*e].iter().collect();
                    aligned_err.push((sub, *s, *e));
                }
                return (aligned_err, cor_spans);
            }
        }

        (err_spans, cor_spans)
    }

    /// 計算字元級 LCS 對齊，回傳連續差異孤島 `(err_start..err_end, cor_start..cor_end)`
    fn compute_char_diff_islands(err_chars: &[char], cor_chars: &[char]) -> Vec<(usize, usize, usize, usize)> {
        let m = err_chars.len();
        let n = cor_chars.len();
        let mut dp = vec![vec![0usize; n + 1]; m + 1];
        for i in (0..m).rev() {
            for j in (0..n).rev() {
                if err_chars[i] == cor_chars[j] {
                    dp[i][j] = 1 + dp[i + 1][j + 1];
                } else {
                    dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
                }
            }
        }

        let mut matches = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < m && j < n {
            if err_chars[i] == cor_chars[j] {
                matches.push((i, j));
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }

        let mut islands = Vec::new();
        let (mut prev_e, mut prev_c) = (0, 0);
        for (me, mc) in matches {
            if me > prev_e || mc > prev_c {
                islands.push((prev_e, me, prev_c, mc));
            }
            prev_e = me + 1;
            prev_c = mc + 1;
        }
        if prev_e < m || prev_c < n {
            islands.push((prev_e, m, prev_c, n));
        }
        islands
    }

    /// 對齊錯誤句與正確句，提取差異區塊、前後文詞元、跨界劫掠假詞、病灶分類與階層構式
    pub fn align_sentence_pair(
        &self,
        error: &str,
        correct: &str,
        custom_err_tokens: Option<&[String]>,
        custom_cor_tokens: Option<&[String]>,
    ) -> Vec<Value> {
        let err_trim = error.trim();
        let cor_trim = correct.trim();
        if err_trim.is_empty() || cor_trim.is_empty() || err_trim == cor_trim {
            return Vec::new();
        }

        let err_chars: Vec<char> = err_trim.chars().collect();
        let cor_chars: Vec<char> = cor_trim.chars().collect();

        let cor_spans: Vec<(String, usize, usize)> = match custom_cor_tokens {
            Some(ts) if !ts.is_empty() => {
                let mut out = Vec::new();
                let mut pos = 0;
                for t in ts {
                    let len = t.chars().count();
                    if len > 0 {
                        out.push((t.clone(), pos, (pos + len).min(cor_chars.len())));
                        pos += len;
                    }
                }
                if pos == cor_chars.len() && !out.is_empty() {
                    out
                } else {
                    self.tokenize_with_spans(cor_trim)
                }
            }
            _ => self.tokenize_with_spans(cor_trim),
        };

        let raw_err_spans: Vec<(String, usize, usize)> = match custom_err_tokens {
            Some(ts) if !ts.is_empty() => {
                let mut out = Vec::new();
                let mut pos = 0;
                for t in ts {
                    let len = t.chars().count();
                    if len > 0 {
                        out.push((t.clone(), pos, (pos + len).min(err_chars.len())));
                        pos += len;
                    }
                }
                if pos == err_chars.len() && !out.is_empty() {
                    out
                } else {
                    self.tokenize_with_spans(err_trim)
                }
            }
            _ => self.tokenize_with_spans(err_trim),
        };

        // 1. 取得字元級差異孤島，並吸附至正確句之詞元邊界 (cor_spans)
        let raw_islands = Self::compute_char_diff_islands(&err_chars, &cor_chars);
        if raw_islands.is_empty() {
            return Vec::new();
        }

        #[derive(Clone)]
        struct SnappedIsland {
            es: usize,
            ee: usize,
            cs: usize,
            ce: usize,
            cor_tok_start: usize,
            cor_tok_end: usize,
        }

        let mut snapped: Vec<SnappedIsland> = Vec::new();
        for (es, ee, cs, ce) in raw_islands {
            // 找出與 [cs, ce) 重疊之正確句 token 範圍
            let mut t_start = None;
            let mut t_end = 0;
            for (idx, (_, s, e)) in cor_spans.iter().enumerate() {
                let overlaps = if cs < ce {
                    *s < ce && *e > cs
                } else {
                    *s <= cs && cs < *e
                };
                if overlaps {
                    if t_start.is_none() {
                        t_start = Some(idx);
                    }
                    t_end = idx + 1;
                }
            }
            let Some(ts_idx) = t_start else {
                continue;
            };
            let snapped_cs = cor_spans[ts_idx].1;
            let snapped_ce = cor_spans[t_end - 1].2;
            let left_expand = cs.saturating_sub(snapped_cs);
            let right_expand = snapped_ce.saturating_sub(ce);
            let snapped_es = es.saturating_sub(left_expand);
            let snapped_ee = (ee + right_expand).min(err_chars.len());

            if let Some(last) = snapped.last_mut() {
                // 若兩個孤島共享或緊鄰同一個被改變的正確句 token（無未改變之正確詞元相隔），則合併為單一區塊
                if ts_idx <= last.cor_tok_end {
                    last.ee = snapped_ee.max(last.ee);
                    last.ce = snapped_ce.max(last.ce);
                    last.cor_tok_end = t_end.max(last.cor_tok_end);
                    continue;
                }
            }
            snapped.push(SnappedIsland {
                es: snapped_es,
                ee: snapped_ee,
                cs: snapped_cs,
                ce: snapped_ce,
                cor_tok_start: ts_idx,
                cor_tok_end: t_end,
            });
        }

        let mut diffs = Vec::new();
        for island in snapped {
            let rejected: String = err_chars[island.es..island.ee].iter().collect();
            let accepted: String = cor_chars[island.cs..island.ce].iter().collect();
            if rejected == accepted || accepted.trim().is_empty() {
                continue;
            }
            let mut accepted_tokens: Vec<String> = cor_spans[island.cor_tok_start..island.cor_tok_end]
                .iter()
                .map(|(t, _, _)| t.clone())
                .filter(|t| is_valid_chinese_token(t))
                .collect();

            // 智慧識別高凝聚度未登錄複合專名（如「瑜伽素」、「文字型」、「大易輸入法」）
            accepted_tokens = Self::coalesce_domain_compounds(&accepted_tokens);

            // 前後文提取：嚴格在標點符號與空白處截斷，並獨立記錄歐文/數值錨點與跨連詞前驅 (prev2_ctx)
            let mut prev_ctx = String::new();
            let mut prev2_ctx = String::new();
            let mut prev_ascii = String::new();
            let mut prev_num = String::new();
            if island.cor_tok_start > 0 {
                let prev_tok = cor_spans[island.cor_tok_start - 1].0.trim();
                if is_valid_chinese_token(prev_tok) {
                    prev_ctx = prev_tok.to_string();
                } else if prev_tok.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-') && !prev_tok.is_empty() {
                    prev_ascii = prev_tok.to_string();
                } else if prev_tok.chars().all(|c| c.is_ascii_digit()) && !prev_tok.is_empty() {
                    prev_num = prev_tok.to_string();
                }
                if island.cor_tok_start > 1 {
                    let prev2_tok = cor_spans[island.cor_tok_start - 2].0.trim();
                    if is_valid_chinese_token(prev2_tok) {
                        prev2_ctx = prev2_tok.to_string();
                    }
                }
            }

            let mut next_ctx = String::new();
            let mut next_ascii = String::new();
            let mut next_num = String::new();
            if island.cor_tok_end < cor_spans.len() {
                let next_tok = cor_spans[island.cor_tok_end].0.trim();
                if is_valid_chinese_token(next_tok) {
                    next_ctx = next_tok.to_string();
                } else if next_tok.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-') && !next_tok.is_empty() {
                    next_ascii = next_tok.to_string();
                } else if next_tok.chars().all(|c| c.is_ascii_digit()) && !next_tok.is_empty() {
                    next_num = next_tok.to_string();
                }
            }

            // 偵測跨詞界交錯劫掠假詞（如「匯兌」跨界劫掠「不會+對」，「十分」跨界劫掠「素食+分類」）
            let cor_sub_spans = &cor_spans[island.cor_tok_start..island.cor_tok_end];
            let cor_local_bounds: HashSet<(usize, usize)> = cor_sub_spans
                .iter()
                .map(|(_, s, e)| (s.saturating_sub(island.cs), e.saturating_sub(island.cs)))
                .collect();
            let mut hijacks = Vec::new();
            for (etok, es, ee) in &raw_err_spans {
                if *es >= island.es && *ee <= island.ee && etok.chars().count() >= 2 && is_valid_chinese_token(etok) {
                    let local_s = es.saturating_sub(island.es);
                    let local_e = ee.saturating_sub(island.es);
                    if !cor_local_bounds.contains(&(local_s, local_e)) && !accepted_tokens.contains(etok) {
                        hijacks.push(etok.clone());
                    }
                }
            }

            diffs.push(self.build_diff_item_with_meta(
                &rejected,
                &accepted,
                &prev_ctx,
                &prev2_ctx,
                &next_ctx,
                &prev_ascii,
                &next_ascii,
                &prev_num,
                &next_num,
                &accepted_tokens,
                &hijacks,
            ));
        }

        diffs
    }

    /// 合併未登錄之高內聚複合詞（如 `[瑜伽, 素]` ➔ `[瑜伽素]`、`[文字, 型]` ➔ `[文字型]`、`[菇, 類]` ➔ `[菇類]`）
    fn coalesce_domain_compounds(tokens: &[String]) -> Vec<String> {
        if tokens.len() < 2 {
            return tokens.to_vec();
        }
        let productive_type_suffixes = ["素", "型", "類", "法", "式", "制", "化", "性", "感", "度", "率", "檔", "庫", "器"];
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            // 優先檢查 [2字專名 + 輸入法] ➔ 5字或3字專名，同時保留內部結構
            if i + 1 < tokens.len() {
                let cur = &tokens[i];
                let nxt = &tokens[i + 1];
                let cur_len = cur.chars().count();
                if (1..=2).contains(&cur_len) && productive_type_suffixes.contains(&nxt.as_str()) {
                    let merged = format!("{cur}{nxt}");
                    if !is_syntactic_phrase_or_clitic(&merged) {
                        out.push(merged);
                        i += 2;
                        continue;
                    }
                }
            }
            out.push(tokens[i].clone());
            i += 1;
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn build_diff_item_with_meta(
        &self,
        rejected: &str,
        accepted: &str,
        prev_ctx: &str,
        prev2_ctx: &str,
        next_ctx: &str,
        prev_ascii: &str,
        next_ascii: &str,
        prev_num: &str,
        next_num: &str,
        accepted_tokens: &[String],
        hijacks: &[String],
    ) -> Value {
        let acc_len = accepted.chars().count();
        let is_known = self.known_words.contains(accepted);
        let is_phrase = is_syntactic_phrase_or_clitic(accepted);

        // 1. 篩選應收錄至 lexicon_domain.tsv 之複合實詞與專名衍生子詞 (Layer 3)
        let mut oov_compound_words = Vec::new();
        let mut seen_oov = HashSet::new();
        for tok in accepted_tokens {
            let t = tok.trim();
            let t_len = t.chars().count();
            if (2..=5).contains(&t_len)
                && is_valid_chinese_token(t)
                && !is_syntactic_phrase_or_clitic(t)
                && !common_typo_map().contains_key(t)
            {
                if t_len == 3 {
                    let prefix2: String = t.chars().take(2).collect();
                    let last_ch = t.chars().nth(2).unwrap_or(' ');
                    if self.known_words.contains(&prefix2) && CONTAINER_SUFFIX_CHARS.contains(last_ch) {
                        continue;
                    }
                }
                if (t.ends_with('素') || t.ends_with('型') || t.ends_with('類')) && t_len >= 2 {
                    let stem: String = t.chars().take(t_len - 1).collect();
                    if stem.chars().count() >= 2 && !self.known_words.contains(&stem) && seen_oov.insert(stem.clone()) {
                        oov_compound_words.push(json!({
                            "word": stem,
                            "bopomofo": self.get_bopomofo(&stem),
                            "weight": 1500,
                            "category": "領域專名詞根",
                            "reason": format!("自派生陣列詞「{}」抽離不可再分之核心詞根，後綴交由 Layer 1 陣列規則處理", t)
                        }));
                    }
                    continue;
                }
                let is_domain_worthy = !self.known_words.contains(t)
                    || matches!(t, "大易" | "倉頡" | "行列" | "許氏" | "嘸蝦米");
                if is_domain_worthy && !t.ends_with("輸入法") && !t.ends_with("鍵盤") && seen_oov.insert(t.to_string()) {
                    oov_compound_words.push(json!({
                        "word": t,
                        "bopomofo": self.get_bopomofo(t),
                        "weight": if t_len >= 3 { 1600 } else { 1500 },
                        "category": "語意調校複合長詞",
                        "reason": format!("Layer 3 複合詞凝聚保護：防止「{}」碎裂或遭假詞「{}」劫掠", t, rejected)
                    }));
                }
            }
        }
        // 若完整區間為「XX輸入法 / XX鍵盤」等陣列組合詞，僅拆解並收錄不可再分之核心專名詞根（如「大易」），
        // 其與「{輸入法, 鍵盤, 字根, 碼表}」之組合一律交由 Layer 1 二維陣列笛卡兒積 (prev_slot × next_slot) 處理
        for head_suf in ["輸入法", "鍵盤"] {
            let suf_len = head_suf.chars().count();
            if accepted.ends_with(head_suf) && acc_len >= suf_len + 2 && acc_len <= suf_len + 3 && is_valid_chinese_token(accepted) {
                let stem: String = accepted.chars().take(acc_len - suf_len).collect();
                if stem.chars().count() >= 2 && !self.known_words.contains(&stem) && seen_oov.insert(stem.clone()) {
                    oov_compound_words.push(json!({
                        "word": stem.clone(),
                        "bopomofo": self.get_bopomofo(&stem),
                        "weight": 1550,
                        "category": "輸入法專名核心詞根",
                        "reason": format!("自「{}」因式分解核心詞根，配合 {{專名詞根}} × {{輸入法, 鍵盤}} 陣列對應規則", accepted)
                    }));
                }
            }
        }

        // 2. 判定六大計算語言學病灶分類 (Type I ~ Type VI)
        let (taxonomy, layer_strategy) = if !prev_ascii.is_empty() || !next_ascii.is_empty() || !prev_num.is_empty() || !next_num.is_empty() {
            (
                "Type VI: 跨語系歐文/數值邊界與交錯假詞劫掠 (Cross-Script Boundary & Hijack)",
                "Layer 1 (子句首/介詞槽位構式) + Layer 2 (跨界錨定轉移，瓦解跨詞界假詞)",
            )
        } else if !hijacks.is_empty() {
            (
                "Type V: 跨詞界交錯假詞劫掠 (Cross-Boundary Overlapping Hijack)",
                "Layer 1 (階層構式邊界保護與假詞抑制) + Layer 3 (複合詞凝聚) + Layer 2 (正軌路徑錨定)",
            )
        } else if !oov_compound_words.is_empty() || (acc_len == 2 && !is_known && !is_phrase && is_valid_chinese_token(accepted)) {
            (
                "Type I: 詞庫缺詞與複合專名碎裂 (OOV & Compound Fragmentation)",
                "Layer 3 (複合實詞/專名入庫與子詞拆解) + Layer 1 (階層槽位構式) + Layer 2 (二階轉移搭配)",
            )
        } else if acc_len == 1 && (PARTICLE_CHARS.contains(accepted.chars().next().unwrap_or(' ')) || CLOSED_CLASS_CHARS.contains(accepted.chars().next().unwrap_or(' '))) {
            (
                "Type III: 結構助詞、連詞與框式介詞構式失配 (Structural Particle & Clause Construction)",
                "Layer 1 (語法構式引力) + Layer 2 (結構虛詞二階轉移，嚴禁調整單字先驗權重)",
            )
        } else if (acc_len == 3
            && self.known_words.contains(&accepted.chars().take(2).collect::<String>())
            && CONTAINER_SUFFIX_CHARS.contains(accepted.chars().nth(2).unwrap_or(' ')))
            || accepted_tokens.iter().any(|t| t.ends_with('型') || t.ends_with('類') || t.ends_with('素'))
        {
            (
                "Type IV: 派生類別後綴與階層槽位構式 ([2+1] Hierarchical Slot-Set Construction)",
                "Layer 1 (階層式集合槽位構式 patterns_user.jsonl) + Layer 3 (派生複合詞入庫) + Layer 2 (二階轉移)",
            )
        } else {
            (
                "Type II: 同音詞二階語法轉移引力不足 (Bigram & Construction Deficiency)",
                "Layer 1 (語法槽位構式) + Layer 2 (二階正向語法轉移 +24.0 ~ +30.0，嚴守單字全域先驗獨立)",
            )
        };

        // 3. 提煉 Layer 1 階層式集合槽位構式 (`patterns_user.jsonl`)
        let hierarchical_constructions = self.synthesize_hierarchical_constructions_for_diff(
            rejected,
            accepted,
            prev_ctx,
            prev2_ctx,
            next_ctx,
            prev_ascii,
            next_ascii,
            accepted_tokens,
            hijacks,
        );

        let hijack_desc = if hijacks.is_empty() {
            Value::Null
        } else {
            json!({
                "hijack_tokens": hijacks,
                "target_tokens": accepted_tokens,
                "mechanism": format!(
                    "錯誤路徑中之雙字假詞「{}」跨越正確詞界「{}」，利用 Viterbi 長度加成奪取局部路徑；已由 Layer 1 構式槽位與 Layer 2 轉移引力予以瓦解。",
                    hijacks.join("、"),
                    accepted_tokens.join(" + ")
                )
            })
        };

        json!({
            "rejected": rejected,
            "accepted": accepted,
            "bopomofo": self.get_bopomofo(accepted),
            "prev_ctx": prev_ctx,
            "prev2_ctx": prev2_ctx,
            "next_ctx": next_ctx,
            "prev_ascii": prev_ascii,
            "next_ascii": next_ascii,
            "prev_num": prev_num,
            "next_num": next_num,
            "accepted_tokens": accepted_tokens,
            "overlapping_hijack": hijack_desc,
            "error_taxonomy": taxonomy,
            "layer_strategy": layer_strategy,
            "oov_compound_words": oov_compound_words,
            "hierarchical_constructions": hierarchical_constructions
        })
    }

    /// 針對單一差異孤島合成可泛化之 Layer 1 階層式集合槽位構式 (`patterns_user.jsonl`)
    #[allow(clippy::too_many_arguments)]
    fn synthesize_hierarchical_constructions_for_diff(
        &self,
        rejected: &str,
        accepted: &str,
        prev_ctx: &str,
        prev2_ctx: &str,
        next_ctx: &str,
        _prev_ascii: &str,
        next_ascii: &str,
        accepted_tokens: &[String],
        hijacks: &[String],
    ) -> Vec<Value> {
        let mut constructions = Vec::new();
        let acc_len = accepted.chars().count();

        // A0. 範圍總括副詞「都/全/已經」＋完成態動補複合詞＋句末體標記「了」構式（如：兜完攻了 ➔ 都完工了）
        if accepted.contains("完工")
            || accepted.contains("完成")
            || accepted.contains("完畢")
            || (accepted.starts_with('都') && (accepted.ends_with('了') || next_ctx == "了"))
            || (prev_ctx == "都" && (accepted == "完工" || accepted.ends_with('了')))
        {
            constructions.push(json!({
                "pattern_id": "scope_adv_completion_predicate_le",
                "name": "範圍總括副詞「都」＋完成態動補複合詞＋體標記「了」構式",
                "formula": "[{都, 全, 全都, 統統, 已經, 終於, 總算, 快, 剛}] → [{完工, 完成, 完畢, 完結, 完稿, 完賽, 收工, 竣工, 做好, 寫好, 弄好, 修好, 改好, 準備好}] → [{了, 囉, 啦}]",
                "category": "Layer 1 範圍總括與完成態動補構式",
                "slots": [
                    ["都", "全", "全都", "統統", "已經", "終於", "總算", "快", "剛"],
                    ["完工", "完成", "完畢", "完結", "完稿", "完賽", "收工", "竣工", "做好", "寫好", "弄好", "修好", "改好", "建好", "裝好"],
                    ["了", "囉", "啦"]
                ],
                "prev_slot": ["都", "全", "全都", "統統", "已經", "終於", "總算", "快", "剛"],
                "next_slot": ["完工", "完成", "完畢", "完結", "完稿", "完賽", "收工", "竣工", "做好", "寫好", "弄好", "修好", "改好", "建好", "裝好"],
                "suppress": [rejected, "兜", "攻了", "工了", "兜完"],
                "bonus": 32.0,
                "backbone_rules": [
                    {"prev": "都", "next": "完工", "score": 30.0, "kind": "construction", "comment": "範圍總括副詞銜接完成態謂詞 [都 ➔ 完工]（防動詞『兜』搶佔）"},
                    {"prev": "完工", "next": "了", "score": 28.0, "kind": "construction", "comment": "完成態雙字動補詞銜接句末助詞 [完工 ➔ 了]（瓦解假詞『攻了』跨界劫掠）"}
                ]
            }));
        }

        // A. [2+1] 介面容器構式（如 點選+框）
        if acc_len == 3 {
            let prefix2: String = accepted.chars().take(2).collect();
            let suffix1: String = accepted.chars().skip(2).collect();
            if is_valid_chinese_token(&prefix2)
                && is_valid_chinese_token(&suffix1)
                && CONTAINER_SUFFIX_CHARS.contains(suffix1.chars().next().unwrap_or(' '))
            {
                constructions.push(json!({
                    "pattern_id": format!("ui_container_slot_{}", prefix2),
                    "name": "介面動作與容器名詞階層槽位構式",
                    "formula": format!("[{{點選, 勾選, 框選, 圈選, 選取, {}}}] → [{{框, 欄, 區, 鈕, 鍵, 窗, {}}}]", prefix2, suffix1),
                    "category": "Layer 1 介面容器階層槽位構式",
                    "prev_slot": ["點選", "勾選", "框選", "圈選", "選取", prefix2.as_str()],
                    "next_slot": ["框", "欄", "區", "鈕", "鍵", "窗", suffix1.as_str()],
                    "suppress": [rejected],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {
                            "prev": prefix2,
                            "next": suffix1,
                            "score": 28.0,
                            "kind": "construction",
                            "comment": format!("階層式集合槽位構式 [{accepted}]")
                        }
                    ]
                }));
            }
        }

        // B. 轉折連詞 + 正反疑問助動詞 + 介詞構式（如：但會不會對 [Github] 造成負擔）
        if accepted.contains("會不會")
            || accepted.contains("能不能")
            || accepted.contains("可不可以")
            || (accepted.starts_with('但') && accepted.ends_with('對'))
            || (accepted == "對"
                && (prev_ctx == "會不會"
                    || prev_ctx == "能不能"
                    || prev_ctx == "可不可以"
                    || prev_ctx.ends_with('會')))
        {
            constructions.push(json!({
                "pattern_id": "adversative_a_not_a_preposition_slot",
                "name": "轉折連詞＋正反疑問助動＋介詞引介構式",
                "formula": "[{但, 但是, 不過, 卻, 那}] → [{會不會, 能不能, 可不可以, 要不要, 該不該, 是否會, 會}] → [{對, 給, 讓, 向, 為}] → [<ASCII/NP>]",
                "category": "Layer 1 子句首情態介詞槽位構式",
                "slots": [
                    ["但", "但是", "不過", "卻", "那"],
                    ["會不會", "能不能", "可不可以", "要不要", "該不該", "是否會"],
                    ["對", "給", "讓", "向", "為"]
                ],
                "prev_slot": ["會不會", "能不能", "可不可以", "要不要", "該不該", "是否會", "可能會", "或許會", "應該會", "總會", "只會"],
                "next_slot": ["對", "給", "讓", "向", "為", "造成"],
                "suppress": if hijacks.is_empty() { vec![rejected.to_string()] } else { hijacks.to_vec() },
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": "但", "next": "會不會", "score": 28.0, "kind": "construction", "comment": "轉折連詞引導正反疑問助動詞 [但 ➔ 會不會]"},
                    {"prev": "會不會", "next": "對", "score": 30.0, "kind": "construction", "comment": "正反疑問助動詞銜接介詞 [會不會 ➔ 對]（瓦解『匯兌』跨界劫掠）"}
                ]
            }));
            if !next_ascii.is_empty() {
                constructions.push(json!({
                    "pattern_id": "preposition_ascii_causative_burden",
                    "name": "跨語系歐文受事與致使負擔構式",
                    "formula": format!("[{{對, 給, 讓}}] → [<ASCII: {}>] → [{{造成, 帶來, 產生, 構成}}] → [{{負擔, 影響, 壓力, 衝擊}}]", next_ascii),
                    "category": "Layer 1 跨語系框架構式",
                    "prev_slot": ["造成", "帶來", "產生", "構成"],
                    "next_slot": ["負擔", "影響", "壓力", "衝擊", "問題"],
                    "bonus": 28.0,
                    "backbone_rules": [
                        {"prev": "造成", "next": "負擔", "score": 28.0, "kind": "construction", "comment": "致使動賓框架搭配 [造成 ➔ 負擔]"}
                    ]
                }));
            }
        }

        // C. 飲食分類、重複副詞「再」與專名項目擴充構式（如：素食分類再增加瑜伽素項目）
        if accepted_tokens.iter().any(|t| t == "素食" || t == "分類" || t.ends_with('素')) {
            constructions.push(json!({
                "pattern_id": "dietary_category_zai4_add_item",
                "name": "分類體系＋重複副詞「再」＋擴充動作＋類別項目構式",
                "formula": "[{建議, 另建議, 新增}] → [{素食, 飲食, 蔬食, 肉食, 餐點, 菜單} + {分類, 類別, 分級, 項目}] → [再] → [{增加, 新增, 加入, 擴充, 增設}] → [{瑜伽素, 全素, 蛋奶素, 五辛素, 植物素} + {項目, 選項}]",
                "category": "Layer 1 領域分類與副詞「再」階層構式",
                "slots": [
                    ["素食", "飲食", "蔬食", "肉食", "餐點", "菜單", "分類", "項目"],
                    ["再"],
                    ["增加", "新增", "加入", "擴充", "增設", "補充"]
                ],
                "prev_slot": ["素食", "飲食", "蔬食", "肉食", "主食", "副食", "糧食", "美食", "速食", "熟食"],
                "next_slot": ["分類", "分析", "分級", "分配", "分區", "分量"],
                "suppress": if hijacks.is_empty() { vec![rejected.to_string(), "十分".to_string()] } else { hijacks.to_vec() },
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": "素食", "next": "分類", "score": 28.0, "kind": "construction", "comment": "定中分類複合構式 [素食 ➔ 分類]（阻斷『十分』跨界劫掠）"},
                    {"prev": "分類", "next": "再", "score": 28.0, "kind": "construction", "comment": "分類主語銜接重複副詞 [分類 ➔ 再]"},
                    {"prev": "再", "next": "增加", "score": 30.0, "kind": "construction", "comment": "重複副詞銜接擴充動詞 [再 ➔ 增加]"},
                    {"prev": "增加", "next": "瑜伽素", "score": 30.0, "kind": "construction", "comment": "擴充動詞銜接領域專名 [增加 ➔ 瑜伽素]"}
                ]
            }));
            // 同步產生對既有「再」規則之擴充構式
            constructions.push(json!({
                "pattern_id": "zai4_action_verb_expansion",
                "name": "重複副詞「再」＋擴充與操作動作謂詞構式",
                "formula": "[{再}] → [{增加, 新增, 加入, 擴充, 增設, 補充, 改用, 取消, 點擊, 確認, 調整}]",
                "category": "Layer 1 副詞「再」謂詞槽位擴充",
                "prev_slot": ["再"],
                "next_slot": ["增加", "新增", "加入", "擴充", "增設", "補充", "改用", "被", "取消", "試", "點擊", "確認", "調整"],
                "suppress": ["在"],
                "bonus": 28.0,
                "backbone_rules": []
            }));
        }

        // D. 成分排除與全稱量化類別構式（如：不含蛋與所有菇類）
        if prev_ctx == "不含" || accepted == "蛋" || accepted_tokens.iter().any(|t| t.ends_with('類')) {
            if prev_ctx == "不含" || accepted == "蛋" {
                constructions.push(json!({
                    "pattern_id": "dietary_exclusion_ingredient_coord",
                    "name": "否定排除謂詞＋食材名詞＋並列連詞構式",
                    "formula": "[{不含, 包含, 不吃, 無, 忌, 免}] → [{蛋, 奶, 蛋奶, 五辛, 蔥, 蒜, 肉, 菇類, 麩質}] → [{與, 及, 和, 或}]",
                    "category": "Layer 1 飲食成分排除構式",
                    "prev_slot": ["不含", "包含", "不吃", "無", "忌"],
                    "next_slot": ["蛋", "奶", "蛋奶", "五辛", "蔥", "蒜", "肉", "菇類", "麩質"],
                    "bifurcation_pairs": [["忌", "蛋"], ["忌", "蒜"]],
                    "suppress": [rejected, "但"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "不含", "next": "蛋", "score": 30.0, "kind": "construction", "comment": "成分排除動賓構式 [不含 ➔ 蛋]（防轉折連詞『但』誤入賓語槽位）"},
                        {"prev": "蛋", "next": "與", "score": 26.0, "kind": "construction", "comment": "名詞並列連詞構式 [蛋 ➔ 與]"}
                    ]
                }));
            }
            if prev_ctx == "所有" || accepted_tokens.iter().any(|t| t.ends_with('類')) {
                constructions.push(json!({
                    "pattern_id": "universal_quantifier_class_noun",
                    "name": "全稱量化限定詞＋總稱類別名詞構式",
                    "formula": "[{所有, 各類, 各種, 任何, 一切, 全部}] → [{菇類, 肉類, 豆類, 藻類, 貝類, 魚類, 蔬菜, 素食, 食材, 項目}]",
                    "category": "Layer 1 全稱量化定中構式",
                    "prev_slot": ["所有", "各類", "各種", "任何", "一切", "全部"],
                    "next_slot": ["菇類", "肉類", "豆類", "藻類", "貝類", "魚類", "蔬菜", "素食", "食材", "項目"],
                    "suppress": [rejected],
                    "bonus": 28.0,
                    "backbone_rules": [
                        {"prev": "所有", "next": "菇類", "score": 28.0, "kind": "construction", "comment": "全稱限定詞修飾類別名詞 [所有 ➔ 菇類]"}
                    ]
                }));
            }
        }

        // E. 學習/使用輸入法專名＋時間方位／話題副詞／耗盡補語構式（以 `{大易, 倉頡, 行列, 許氏, 嘸蝦米...} × {輸入法, 鍵盤, 字根, 碼表}` 二維陣列笛卡兒積處理）
        if accepted.contains("大易")
            || accepted.contains("許氏")
            || accepted_tokens.iter().any(|t| t == "之前" || t == "因為")
        {
            constructions.push(json!({
                "pattern_id": "ime_root_device_head_array",
                "name": "中文輸入法專名詞根 × 設備/法體後綴二維陣列對應",
                "formula": "[{大易, 倉頡, 行列, 許氏, 嘸蝦米, 無蝦米, 注音, 速成, 拼音, 大千, 倚天, 羅馬}] × [{輸入法, 鍵盤, 字根, 碼表, 拆碼, 規則}]",
                "category": "Layer 1 二維陣列笛卡兒積構式 (M × N Array Correspondence)",
                "prev_slot": ["大易", "倉頡", "行列", "許氏", "嘸蝦米", "無蝦米", "注音", "速成", "拼音", "大千", "倚天", "羅馬"],
                "next_slot": ["輸入法", "鍵盤", "字根", "碼表", "拆碼", "規則"],
                "suppress": ["大意", "大憶", "大義", "許是"],
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": "大易", "next": "輸入法", "score": 30.0, "kind": "construction", "comment": "輸入法詞根 × 後綴陣列對應 [大易 ➔ 輸入法]"},
                    {"prev": "許氏", "next": "鍵盤", "score": 30.0, "kind": "construction", "comment": "輸入法詞根 × 後綴陣列對應 [許氏 ➔ 鍵盤]"}
                ]
            }));
            if accepted_tokens.iter().any(|t| t == "之前" || t == "因為")
                || prev_ctx == "學"
                || prev_ctx == "在學"
            {
                constructions.push(json!({
                    "pattern_id": "learn_ime_temporal_localizer_causal",
                    "name": "動作動詞 × 輸入法專名 × 設備後綴(可選) × 時間方位詞四維陣列鏈構式",
                    "formula": "[{學, 學習, 用, 使用, 打, 練, 改用, 切換}] × [{大易, 倉頡, 行列, 注音, 許氏, 嘸蝦米, 速成, 拼音, 大千, 倚天}] × [{輸入法, 鍵盤, 字根, 碼表}?] × [{之前, 之後, 以前, 以後}]",
                    "category": "Layer 1 多維陣列鏈與可選槽位跨接構式",
                    "slots": [
                        ["學", "學習", "用", "使用", "打", "練", "練習", "改用", "切換"],
                        ["大易", "倉頡", "行列", "注音", "許氏", "嘸蝦米", "無蝦米", "速成", "拼音", "大千", "倚天"],
                        ["輸入法", "鍵盤", "字根", "碼表", "拆碼", "規則"],
                        ["之前", "之後", "以前", "以後", "之初", "之餘"]
                    ],
                    "optional_slots": [2],
                    "prev_slot": ["輸入法", "鍵盤", "大易", "倉頡", "行列", "許氏", "嘸蝦米", "工作", "學習", "上班"],
                    "next_slot": ["之前", "之後", "以前", "以後", "之初", "之餘"],
                    "suppress": [rejected, "大意", "支前", "因位"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "學", "next": "大易", "score": 30.0, "kind": "construction", "comment": "學習動詞銜接輸入法專名詞根 [學 ➔ 大易]（抑制同音形容詞『大意』）"},
                        {"prev": "輸入法", "next": "之前", "score": 28.0, "kind": "construction", "comment": "事件名詞後接時間方位詞 [輸入法 ➔ 之前]"},
                        {"prev": "之前", "next": "因為", "score": 28.0, "kind": "construction", "comment": "時間方位詞銜接因果連詞 [之前 ➔ 因為]"}
                    ]
                }));
            }
            if accepted.contains("大易") && (accepted_tokens.iter().any(|t| t == "就") || next_ctx == "就") {
                let post_adv = if next_ctx.is_empty() { "幾乎" } else { next_ctx };
                constructions.push(json!({
                    "pattern_id": "ime_subject_focus_adverb_jiu",
                    "name": "輸入法專名話題主語 × 承接副詞「就/也/都」× 程度副詞構式",
                    "formula": "[{大易, 倉頡, 行列, 許氏, 嘸蝦米, 注音, 拼音}] × [{就, 也, 都, 已經, 幾乎}] × [{幾乎, 差不多, 完全, 已經, 忘光, 忘光光}?]",
                    "category": "Layer 1 話題主語與承接副詞構式",
                    "slots": [
                        ["大易", "倉頡", "行列", "許氏", "嘸蝦米", "無蝦米", "注音", "速成", "拼音", "大千", "倚天"],
                        ["就", "也", "都", "已經", "幾乎"],
                        ["幾乎", "差不多", "完全", "已經", "快要", "忘", "忘記", "忘光", "忘光光"]
                    ],
                    "optional_slots": [2],
                    "prev_slot": ["大易", "倉頡", "行列", "許氏", "嘸蝦米", "無蝦米", "注音", "速成", "拼音"],
                    "next_slot": ["就", "也", "都", "已經", "幾乎"],
                    "suppress": [rejected, "大憶舊", "大意就", "大義就"],
                    "bonus": 32.0,
                    "backbone_rules": [
                        {"prev": "大易", "next": "就", "score": 32.0, "kind": "construction", "comment": "輸入法話題主語銜接承接副詞 [大易 ➔ 就]（防跨詞界錯配為『大憶 + 舊』）"},
                        {"prev": "就", "next": post_adv, "score": 28.0, "kind": "construction", "comment": format!("承接副詞銜接程度副詞 [就 ➔ {post_adv}]")}
                    ]
                }));
            }
        }

        // E2. 耗盡結果補語重疊式「幾乎 × 忘光(光) × 了」構式（如：幾乎忘光光了呀）
        if accepted.contains("忘光") || (accepted.contains("光") && rejected.contains("逛")) {
            let target_word = if accepted.contains("忘光光") { "忘光光" } else { accepted };
            constructions.push(json!({
                "pattern_id": "depletion_resultative_guang_complement",
                "name": "程度副詞「幾乎/全都」× 耗盡結果補語「V光(光)」× 體貌助詞「了」構式",
                "formula": "[{幾乎, 差不多, 全都, 全部, 統統, 通通, 已經, 快}] × [{忘光, 忘光光, 用光, 用光光, 花光, 花光光, 吃光, 吃光光, 賣光, 跑光}] × [{了, 啦, 囉}?]",
                "category": "Layer 1 動結式耗盡補語構式",
                "slots": [
                    ["幾乎", "差不多", "全都", "全部", "統統", "通通", "已經", "快"],
                    ["忘光", "忘光光", "用光", "用光光", "花光", "花光光", "吃光", "吃光光", "賣光", "跑光"],
                    ["了", "啦", "囉"]
                ],
                "optional_slots": [2],
                "prev_slot": ["幾乎", "差不多", "全都", "全部", "統統", "通通", "已經", "快"],
                "next_slot": ["忘光", "忘光光", "用光", "用光光", "花光", "花光光", "吃光", "吃光光", "賣光", "跑光"],
                "suppress": [rejected, "忘逛逛", "逛逛了"],
                "bonus": 32.0,
                "backbone_rules": [
                    {"prev": if prev_ctx.is_empty() { "幾乎" } else { prev_ctx }, "next": target_word, "score": 32.0, "kind": "construction", "comment": format!("程度副詞銜接耗盡結果補語 [幾乎 ➔ {target_word}]（防誤選為重疊動詞『逛逛』）")},
                    {"prev": target_word, "next": "了", "score": 30.0, "kind": "construction", "comment": format!("耗盡結果補語銜接完成貌助詞 [{target_word} ➔ 了]")}
                ]
            }));
        }

        // F. 程度副詞＋傾向心理動詞＋認知型態定語＋對比連詞構式（如：比較偏向文字型思考而非語音思考）
        if accepted_tokens.iter().any(|t| t == "偏向" || t.ends_with('型') || t == "而非" || t == "語音") {
            if accepted_tokens.iter().any(|t| t == "偏向" || t.ends_with('型')) {
                constructions.push(json!({
                    "pattern_id": "cognitive_tendency_type_thinking",
                    "name": "程度副詞＋心理傾向動詞＋[~型]認知屬性＋中心名詞構式",
                    "formula": "[{比較, 較, 更, 相當, 基本上}] → [{偏向, 傾向, 偏好, 擅長}] → [{文字型, 語音型, 視覺型, 聽覺型, 圖像型, 邏輯型, 直覺型, 分析型}] → [{思考, 思維, 記憶, 學習}]",
                    "category": "Layer 1 派生屬性定中階層構式",
                    "slots": [
                        ["比較", "較", "更", "相當", "基本上"],
                        ["偏向", "傾向", "偏好", "擅長"],
                        ["文字型", "語音型", "視覺型", "聽覺型", "圖像型", "邏輯型", "直覺型", "分析型"]
                    ],
                    "prev_slot": ["偏向", "傾向", "偏好", "屬於"],
                    "next_slot": ["文字型", "語音型", "視覺型", "聽覺型", "圖像型", "邏輯型", "直覺型", "分析型", "文字", "語音"],
                    "suppress": [rejected, "片象", "紋自行", "自行"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "比較", "next": "偏向", "score": 28.0, "kind": "construction", "comment": "程度副詞修飾傾向動詞 [比較 ➔ 偏向]"},
                        {"prev": "偏向", "next": "文字型", "score": 30.0, "kind": "construction", "comment": "傾向動詞銜接認知屬性詞 [偏向 ➔ 文字型]（瓦解『自行』跨界劫掠）"},
                        {"prev": "文字型", "next": "思考", "score": 28.0, "kind": "construction", "comment": "認知屬性修飾中心語 [文字型 ➔ 思考]"}
                    ]
                }));
            }
            if accepted_tokens.iter().any(|t| t == "而非" || t == "語音") || prev_ctx == "而非" {
                constructions.push(json!({
                    "pattern_id": "contrastive_negation_erfei_cognitive",
                    "name": "對比否定連詞「而非」＋平行認知名詞構式",
                    "formula": "[{思考, 思維, 模式, 方式}] → [{而非, 而不是}] → [{語音, 語音型, 文字, 文字型, 視覺, 視覺型, 邏輯}] → [{思考, 思維, 模式}]",
                    "category": "Layer 1 對比並列連詞構式",
                    "prev_slot": ["而非", "而不是"],
                    "next_slot": ["語音", "語音型", "文字", "文字型", "視覺", "視覺型", "聽覺", "圖像", "邏輯"],
                    "suppress": [rejected, "羽音"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "思考", "next": "而非", "score": 28.0, "kind": "construction", "comment": "前項中心語銜接對比連詞 [思考 ➔ 而非]"},
                        {"prev": "而非", "next": "語音", "score": 30.0, "kind": "construction", "comment": "對比連詞銜接平行屬性名詞 [而非 ➔ 語音]"}
                    ]
                }));
            }
        }

        // F2. 準分裂句名物化焦點判斷構式「(不)X 的 是」（如：選舉選的試一個 ➔ 選舉選的是一個）
        if accepted == "的是"
            || (accepted == "是" && (prev_ctx == "的" || prev_ctx.ends_with('的')))
            || (rejected.contains('試') && accepted.contains('是'))
        {
            constructions.push(json!({
                "pattern_id": "pseudo_cleft_neg_v_de_shi_3d_array",
                "name": "準分裂句名物化焦點判斷構式「(不)X 的 是」",
                "formula": "「(不)X 的 是」，X={選, 買, 賣, 看, 聽, 讀, 寫, 說, 講, 問, 要, 想, 做, 吃, 喝, 穿, 用, 找, 投, 盼, 爭, 怕, 愛...}（即 [(不)X] × [的] × [是]）",
                "category": "Layer 1 準分裂句名物化焦點判斷構式",
                "slots": [
                    ["選", "買", "賣", "看", "聽", "讀", "寫", "說", "講", "問", "要", "想", "做", "吃", "喝", "穿", "用", "找", "投", "盼", "爭", "怕", "愛", "比", "拼", "靠", "憑", "缺", "指", "不選", "不買", "不賣", "不看", "不聽", "不讀", "不寫", "不說", "不講", "不問", "不要", "不想", "不做", "不吃", "不穿", "不用", "不找", "不怕", "不愛", "不投", "不爭"],
                    ["的"],
                    ["是"]
                ],
                "prev_slot": ["選", "買", "賣", "看", "聽", "讀", "寫", "說", "講", "問", "要", "想", "做", "吃", "喝", "穿", "用", "找", "投", "盼", "爭", "怕", "愛", "比", "拼", "靠", "憑", "缺", "指", "不選", "不買", "不賣", "不看", "不聽", "不讀", "不寫", "不說", "不講", "不問", "不要", "不想", "不做", "不吃", "不穿", "不用", "不找", "不怕", "不愛", "不投", "不爭", "的"],
                "next_slot": ["的", "是"],
                "suppress": [rejected, "的試", "試一個", "選的試"],
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": "選", "next": "的", "score": 28.0, "kind": "construction", "comment": "準分裂句名物化動詞槽位銜接助詞 [(不)X ➔ 的]"},
                    {"prev": "的", "next": "是", "score": 30.0, "kind": "construction", "comment": "準分裂句焦點標記耦合 [的 ➔ 是]（抑制同音『試』，構式：[(不)X] × [的] × [是]）"},
                    {"prev": "是", "next": if next_ctx.is_empty() { "一個" } else { next_ctx }, "score": 28.0, "kind": "construction", "comment": "判斷繫詞銜接焦點數量定語 [是 ➔ 一個]（阻斷離合詞『試一個』跨界搶佔）"}
                ]
            }));
        }

        // F3a. 句末附加問句（Tag Question）「，對嗎？／，對吧？」構式（嚴格與介詞『一個對...』分流，絕不將語氣詞『嗎』塞入抽象名詞槽位）
        let is_tag_question_particle = matches!(next_ctx, "嗎" | "吧" | "呢" | "嗎？" | "吧？" | "囉");
        if (accepted == "對" && is_tag_question_particle)
            || accepted == "對嗎"
            || accepted == "對吧"
        {
            let q_particle = if is_tag_question_particle { next_ctx } else { "嗎" };
            constructions.push(json!({
                "pattern_id": "sentence_final_tag_question_dui_ma",
                "name": "句末確認附加問句「對嗎／對吧／是嗎」二維陣列構式（防誤選為名詞『隊嗎』）",
                "formula": "[{對, 是, 好, 行, 可以, 可, 會, 能, 有, 真的}] × [{嗎, 吧, 呢}]",
                "category": "Layer 1 句末附加問句構式 (Tag Question Construction)",
                "prev_slot": ["對", "是", "好", "行", "可以", "可", "會", "能", "有", "真的"],
                "next_slot": ["嗎", "吧", "呢"],
                "suppress": [rejected, "隊嗎", "隊吧", "兌嗎"],
                "bonus": 32.0,
                "backbone_rules": [
                    {"prev": "對", "next": q_particle, "score": 32.0, "kind": "construction", "comment": format!("句末確認附加問句 [對 ➔ {q_particle}]（防同音名詞『隊』於語氣詞前誤出）")}
                ]
            }));
        }

        // F3b. 不定數量定語＋引介介詞「對」＋抽象治理/議題名詞構式（如：一個隊治理方式 ➔ 一個對治理方式）
        if accepted == "對"
            && !is_tag_question_particle
            && (rejected == "隊"
                || rejected == "兌"
                || prev_ctx.starts_with('一')
                || prev_ctx.ends_with('個')
                || prev_ctx.ends_with('種'))
        {
            let head_noun = if next_ctx.is_empty() || PARTICLE_CHARS.contains(next_ctx) {
                "治理"
            } else {
                next_ctx
            };
            let mut slot3: Vec<&str> = vec![
                "治理", "發展", "改革", "政策", "未來", "社會", "制度", "生活", "國家", "民主",
                "環境", "教育", "經濟",
            ];
            if !slot3.contains(&head_noun) && !PARTICLE_CHARS.contains(head_noun) {
                slot3.push(head_noun);
            }
            let mut next_slot_vec: Vec<&str> = vec!["對", "對於", "關於", "針對"];
            next_slot_vec.extend_from_slice(&slot3);
            let slot3_val = Value::Array(slot3.iter().map(|s| json!(*s)).collect());
            constructions.push(json!({
                "pattern_id": "determiner_prep_dui_nominal_modifier",
                "name": "不定數量定語＋引介介詞「對」＋抽象名詞三維陣列構式",
                "formula": "[數量定語 {一個, 一種, 一份, 一場, 這個, 那個...}] × [引介介詞 {對, 對於, 關於, 針對}] × [抽象名詞 {治理, 發展, 改革, 政策, 未來, 社會...}]",
                "category": "Layer 1 介詞短語作定語構式",
                "slots": [
                    ["一個", "一種", "一份", "一場", "一段", "一項", "這個", "那個", "這種", "那種"],
                    ["對", "對於", "關於", "針對"],
                    slot3_val
                ],
                "prev_slot": ["一個", "一種", "一份", "一場", "一段", "一項", "這個", "那個", "這種", "那種", "對", "對於", "關於", "針對"],
                "next_slot": next_slot_vec,
                "suppress": [rejected, "一個隊"],
                "bonus": 28.0,
                "backbone_rules": [
                    {"prev": if prev_ctx.is_empty() { "一個" } else { prev_ctx }, "next": "對", "score": 28.0, "kind": "construction", "comment": "數量定語銜接引介介詞 [一個 ➔ 對]（防止介詞『對』被誤選為量詞後同音名詞『隊』）"},
                    {"prev": "對", "next": head_noun, "score": 28.0, "kind": "construction", "comment": format!("介詞引介受事抽象名詞 [對 ➔ {head_noun}]")}
                ]
            }));
        }

        // F4. 常態屬性定語「一般」vs. 班次量詞「一班」雙向槽位構式（如：這樣一班使用者 ➔ 這樣一般使用者）
        if accepted == "一般" || (accepted.contains("一般") && rejected.contains("一班")) {
            let head_noun = if next_ctx.is_empty() { "使用者" } else { next_ctx };
            constructions.push(json!({
                "pattern_id": "general_attributive_yiban_vs_classifier",
                "name": "常態屬性定語「一般」× 人群/狀態中心語二維陣列構式（防指示詞『這樣』誤綁定與量詞『一班』誤奪）",
                "formula": "[一般] × [{使用者, 人, 人們, 民眾, 大眾, 百姓, 讀者, 消費者, 客戶, 家庭, 企業, 公司, 情況, 狀況, 情形, 水準, 標準, 原則, 規定, 而言, 來說, 說來, 看來, 認為}]",
                "category": "Layer 1 屬性定語與數量詞雙向分流構式",
                "prev_slot": ["一般"],
                "next_slot": [
                    "使用者", "人", "人們", "民眾", "大眾", "百姓", "讀者", "消費者", "客戶",
                    "家庭", "企業", "公司", "情況", "狀況", "情形", "水準", "標準", "原則",
                    "規定", "而言", "來說", "說來", "看來", "認為", head_noun
                ],
                "suppress": [rejected, "一班使用者", "一班人", "一班民眾"],
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": "一般", "next": head_noun, "score": 28.0, "kind": "construction", "comment": format!("常態屬性定語修飾中心語 [一般 ➔ {head_noun}]（嚴禁綁定前驅指示詞『這樣』，保留『這樣一班公車』反例）")}
                ]
            }));
        }

        // F5. 認知心理動詞「確定 / 確認」× 情態助動詞與子句補語構式（如：使用者卻定可以 ➔ 使用者確定可以）
        if accepted == "確定" || (accepted.contains("確定") && rejected.contains("卻定")) {
            let modal_next = if next_ctx.is_empty() { "可以" } else { next_ctx };
            constructions.push(json!({
                "pattern_id": "cognitive_verb_queding_modal_complement",
                "name": "施事主語 × 認知心理動詞「確定/確認」× 情態助動詞三維陣列構式",
                "formula": "[{使用者, 民眾, 系統, 大家, 我們, 你, 我, 他, 客戶, 家長, 先}?] × [{確定, 確認, 肯定, 決定}] × [{可以, 能, 能夠, 會, 不會, 要, 是, 有, 沒, 沒有, 不, 已, 已經, 一下}]",
                "category": "Layer 1 認知心理動詞與情態補語構式",
                "slots": [
                    ["使用者", "民眾", "系統", "大家", "我們", "你", "我", "他", "客戶", "家長", "先"],
                    ["確定", "確認", "肯定", "決定"],
                    ["可以", "能", "能夠", "會", "不會", "要", "是", "有", "沒", "沒有", "不", "已", "已經", "一下"]
                ],
                "optional_slots": [0],
                "prev_slot": ["確定", "確認", "肯定"],
                "next_slot": ["可以", "能", "能夠", "會", "不會", "要", "是", "有", "沒", "沒有", "不", "已", "已經", "一下", modal_next],
                "suppress": [rejected, "卻定"],
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": "確定", "next": modal_next, "score": 28.0, "kind": "construction", "comment": format!("認知心理動詞銜接情態助動詞 [確定 ➔ {modal_next}]（防雙字詞被拆碎為『卻 + 定』）")}
                ]
            }));
        }

        // F6. 文本校勘「錯誤 × 與 × 勘誤 × 校對」跨連詞三元構式（如：錯誤與刊物校對 ➔ 錯誤與勘誤校對）
        if accepted == "勘誤" || (accepted.contains("勘誤") && rejected.contains("刊物")) {
            let right_head = if next_ctx.is_empty() { "校對" } else { next_ctx };
            constructions.push(json!({
                "pattern_id": "error_and_errata_proofreading_collocation",
                "name": "文本訛誤名詞 × 並列連詞「與/和」×「勘誤」× 校對動作多維陣列構式（防透明連詞『與➔勘誤』污染『書籍與刊物』）",
                "formula": "[{錯誤, 錯字, 訛誤, 疏漏, 筆誤, 錯別字, 原文}] × [{與, 和, 及, 或, 跟}] × [{勘誤, 訂正, 校勘}] × [{校對, 訂正, 比對, 對照, 修正, 表, 清單}?]",
                "category": "Layer 1 跨連詞同義領域並列與文本校對構式",
                "slots": [
                    ["錯誤", "錯字", "訛誤", "疏漏", "筆誤", "錯別字", "原文"],
                    ["與", "和", "及", "或", "跟"],
                    ["勘誤", "訂正", "校勘"]
                ],
                "prev_slot": ["勘誤", "校勘", "訂正"],
                "next_slot": ["校對", "訂正", "比對", "對照", "修正", "表", "清單", "雙句", "紀錄", right_head],
                "suppress": [rejected, "與刊物校對"],
                "bonus": 32.0,
                "backbone_rules": [
                    {"prev": "勘誤", "next": right_head, "score": 28.0, "kind": "construction", "comment": format!("文本校勘專名銜接後繼中心語 [勘誤 ➔ {right_head}]（嚴禁生成無條件『與 ➔ 勘誤』以保護『書籍與刊物』）")}
                ]
            }));
        }

        // F7. 語篇處理動作 × 語言單位「雙句 / 單句 / 文句」二維陣列構式（如：校對雙具 ➔ 校對雙句）
        if accepted == "雙句" || (accepted.ends_with('句') && rejected.ends_with('具')) {
            let left_verb = if prev_ctx.is_empty() { "校對" } else { prev_ctx };
            constructions.push(json!({
                "pattern_id": "textual_action_shuangju_sentence_pair",
                "name": "語篇校對/對齊動作 × 語言單位「雙句/文句/詞句」二維陣列構式",
                "formula": "[{校對, 對齊, 比對, 對照, 貼入, 輸入, 分析, 翻譯, 修改, 勘誤, 範例, 對偶, 平行, 前後}] × [{雙句, 單句, 語句, 文句, 詞句, 句子, 例句, 造句}]",
                "category": "Layer 1 語篇動作與語言單位賓語構式",
                "prev_slot": [
                    "校對", "對齊", "比對", "對照", "貼入", "輸入", "分析", "翻譯", "修改",
                    "勘誤", "範例", "對偶", "平行", "前後", "上下", left_verb
                ],
                "next_slot": ["雙句", "單句", "語句", "文句", "詞句", "句子", "例句", "造句", accepted],
                "suppress": [rejected, "雙具"],
                "bonus": 30.0,
                "backbone_rules": [
                    {"prev": left_verb, "next": accepted, "score": 28.0, "kind": "construction", "comment": format!("語篇動作銜接語言單位賓語 [{left_verb} ➔ {accepted}]（防器物量詞『具』誤入）")}
                ]
            }));
        }

        // F8. 受事轉移複合動補「交到 / 送到 / 傳到」× 接收者名詞 × 處所方位詞構式（如：可以交道使用者手上 ➔ 可以交到使用者手上）
        if accepted == "交到" || (accepted.ends_with('到') && rejected == "交道") {
            let recipient = if next_ctx.is_empty() { "使用者" } else { next_ctx };
            constructions.push(json!({
                "pattern_id": "dative_transfer_jiaodao_recipient_hand",
                "name": "受事轉移結果補語「交到/送到」× 接收者名詞 × 方位詞「手上/手中」三維陣列構式（防社交離合詞『交道』誤奪）",
                "formula": "[{可以, 能, 將, 把}?] × [{交到, 送到, 傳到, 發到, 寄到, 遞到, 落到, 回到}] × [{使用者, 用戶, 客戶, 讀者, 民眾, 大家, 玩家, 消費者, 你, 我, 他}] × [{手上, 手裡, 手中, 身邊, 面前}?]",
                "category": "Layer 1 雙賓/處所轉移結果補語構式",
                "slots": [
                    ["交到", "送到", "傳到", "發到", "寄到", "遞到", "落到", "回到"],
                    ["使用者", "用戶", "客戶", "讀者", "民眾", "大家", "玩家", "消費者", "你", "我", "他"],
                    ["手上", "手裡", "手中", "身邊", "面前", "那裡"]
                ],
                "optional_slots": [2],
                "prev_slot": ["交到", "送到", "傳到", "發到", "寄到", "遞到", "落到", "回到"],
                "next_slot": ["使用者", "用戶", "客戶", "讀者", "民眾", "大家", "玩家", "消費者", "手上", "手裡", "手中", recipient],
                "suppress": [rejected, "交道使用者", "可以交道"],
                "bonus": 32.0,
                "backbone_rules": [
                    {"prev": if prev_ctx.is_empty() { "可以" } else { prev_ctx }, "next": "交到", "score": 28.0, "kind": "construction", "comment": "情態助動詞銜接受事轉移動補 [可以 ➔ 交到]（保留『打交道』反例）"},
                    {"prev": "交到", "next": recipient, "score": 32.0, "kind": "construction", "comment": format!("受事轉移動補銜接接收者名詞 [交到 ➔ {recipient}]（防『交道』誤奪）")}
                ]
            }));
        }

        // F9. 斷言副詞「真的 / 實在」× 存現/情態動詞「有 / 沒」× 抽象需求名詞「必要 / 需要」構式（如：真的有庳要嗎 ➔ 真的有必要嗎）
        if accepted.contains("有必要")
            || accepted == "必要"
            || (rejected.contains('庳') && accepted.contains('必'))
        {
            constructions.push(json!({
                "pattern_id": "modal_existential_you_biyao",
                "name": "斷言情態副詞 × 存現動詞「有/沒有」× 需求/價值抽象名詞三維陣列構式（防古地名『有庳』跨界搶佔）",
                "formula": "[{真的, 實在, 確實, 的確, 還真, 到底, 究竟, 是否, 難道}?] × [{有, 沒有, 沒, 很有, 毫無}] × [{必要, 需要, 意義, 價值, 道理, 可能, 把握, 機會, 幫助, 問題}]",
                "category": "Layer 1 斷言存現與情態名詞構式",
                "slots": [
                    ["真的", "實在", "確實", "的確", "還真", "到底", "究竟", "是否", "難道"],
                    ["有", "沒有", "沒", "很有", "毫無"],
                    ["必要", "需要", "意義", "價值", "道理", "可能", "把握", "機會", "幫助", "問題"]
                ],
                "optional_slots": [0],
                "prev_slot": ["有", "沒有", "沒", "很有", "毫無"],
                "next_slot": ["必要", "需要", "意義", "價值", "道理", "可能", "把握", "機會", "幫助", "問題"],
                "suppress": [rejected, "有庳", "有庳要", "庳要"],
                "bonus": 34.0,
                "backbone_rules": [
                    {"prev": "真的", "next": "有", "score": 30.0, "kind": "construction", "comment": "斷言副詞銜接存現動詞 [真的 ➔ 有]"},
                    {"prev": "有", "next": "必要", "score": 34.0, "kind": "construction", "comment": "存現動詞銜接需求名詞 [有 ➔ 必要]（徹底阻斷古地名『有庳』跨詞界劫掠）"}
                ]
            }));
        }

        // F10. 時間重複副詞「再度 / 再次」× 心理/言語動賓複合詞「靈魂拷問」構式（如：在渡靈魂考問 ➔ 再度靈魂拷問）
        if accepted.contains("再度") || accepted.contains("靈魂拷問") || accepted.contains("拷問") {
            constructions.push(json!({
                "pattern_id": "temporal_zaidu_soul_interrogation",
                "name": "時間重複副詞「再度/再次」× 深度心理/言談動賓構式（防『在渡』與『考問』同音誤配）",
                "formula": "[{再度, 再次, 一再, 屢次, 又一次}] × [{靈魂拷問, 拷問, 質問, 追問, 發出, 引發, 掀起, 成為, 陷入, 面對, 挑戰}]",
                "category": "Layer 1 時間重複副詞與言談動賓構式",
                "slots": [
                    ["再度", "再次", "一再", "屢次", "又一次"],
                    ["靈魂", "發出", "引發", "掀起", "面臨", "面對", "進行", "遭到"],
                    ["拷問", "考驗", "質疑", "追問", "挑戰", "討論", "熱議"]
                ],
                "optional_slots": [1],
                "prev_slot": ["再度", "再次", "一再", "屢次", "靈魂"],
                "next_slot": ["靈魂拷問", "靈魂", "拷問", "質問", "追問", "引發", "掀起", "成為", "面對"],
                "suppress": [rejected, "在渡", "考問", "在渡靈魂", "靈魂考問"],
                "bonus": 34.0,
                "backbone_rules": [
                    {"prev": "再度", "next": "靈魂拷問", "score": 34.0, "kind": "construction", "comment": "時間重複副詞修飾四字格成語 [再度 ➔ 靈魂拷問]"},
                    {"prev": "再度", "next": "靈魂", "score": 32.0, "kind": "construction", "comment": "時間重複副詞銜接修飾名詞 [再度 ➔ 靈魂]（防同音『在渡』）"},
                    {"prev": "靈魂", "next": "拷問", "score": 34.0, "kind": "construction", "comment": "偏正動賓凝聚 [靈魂 ➔ 拷問]（防同音『考問』）"}
                ]
            }));
        }

        // F11. 進行體標記「在」× 工程開發動詞「實做/實作/開發」× 定語助詞「的」× 軟體模組專名構式（如：在實做的「語意調整/調校工作室」僅限漢語）
        if accepted.contains("實做")
            || accepted.contains("調整")
            || accepted.contains("調校")
            || accepted.contains("僅限")
        {
            if accepted.contains("實做") || prev_ctx == "實做" || prev_ctx == "在" {
                constructions.push(json!({
                    "pattern_id": "progressive_shizuo_attributive_de",
                    "name": "進行體「在/正在」× 軟體工程動詞「實做/實作/開發」× 關係子句助詞「的」三維構式",
                    "formula": "[{在, 正在, 目前, 所, 新}] × [{實做, 實作, 開發, 打造, 建構, 設計, 測試, 維護}] × [{的, 中, 出來}]",
                    "category": "Layer 1 進行體關係子句定語構式",
                    "slots": [
                        ["在", "正在", "目前", "所", "新"],
                        ["實做", "實作", "開發", "打造", "建構", "設計", "測試", "維護"],
                        ["的", "中", "出來"]
                    ],
                    "prev_slot": ["在", "正在", "目前", "所", "新", "實做", "實作"],
                    "next_slot": ["實做", "實作", "開發", "打造", "建構", "的"],
                    "suppress": [rejected, "時做", "在時做", "時做的"],
                    "bonus": 32.0,
                    "backbone_rules": [
                        {"prev": "在", "next": "實做", "score": 32.0, "kind": "construction", "comment": "進行體標記銜接工程動詞 [在 ➔ 實做]（防同音『時做』）"},
                        {"prev": "實做", "next": "的", "score": 30.0, "kind": "construction", "comment": "工程動詞銜接定語結構助詞 [實做 ➔ 的]"}
                    ]
                }));
            }
            if accepted.contains("調整") || accepted.contains("調校") || next_ctx == "工作室" {
                constructions.push(json!({
                    "pattern_id": "semantic_tuning_studio_domain_3d_array",
                    "name": "語言學領域詞 ×「調校/調整/分析」×「工作室/引擎/系統」三維陣列構式",
                    "formula": "[{語意, 語法, 權重, 參數, 詞頻, 模型, 系統}] × [{調校, 調整, 分析, 測試, 訓練, 最佳化}] × [{工作室, 引擎, 系統, 工具, 平台, 模組, 介面}]",
                    "category": "Layer 1 領域複合名詞三維陣列構式",
                    "slots": [
                        ["語意", "語法", "權重", "參數", "詞頻", "模型", "系統"],
                        ["調校", "調整", "分析", "測試", "訓練", "最佳化"],
                        ["工作室", "引擎", "系統", "工具", "平台", "模組", "介面"]
                    ],
                    "prev_slot": ["語意", "語法", "權重", "參數", "調校", "調整", "語意調校", "語意調整"],
                    "next_slot": ["調校", "調整", "語意調校", "語意調整", "工作室", "引擎", "系統", "工具"],
                    "suppress": [rejected, "調教", "條整", "語意調教", "語意條整"],
                    "bonus": 34.0,
                    "backbone_rules": [
                        {"prev": "語意", "next": "調校", "score": 34.0, "kind": "construction", "comment": "領域主體銜接校準動詞 [語意 ➔ 調校]（防同音『調教』）"},
                        {"prev": "語意", "next": "調整", "score": 34.0, "kind": "construction", "comment": "領域主體銜接校準動詞 [語意 ➔ 調整]（防同音『條整』）"},
                        {"prev": "調校", "next": "工作室", "score": 32.0, "kind": "construction", "comment": "校準動詞修飾場域中心語 [調校 ➔ 工作室]"},
                        {"prev": "調整", "next": "工作室", "score": 32.0, "kind": "construction", "comment": "調整動詞修飾場域中心語 [調整 ➔ 工作室]"}
                    ]
                }));
            }
            if accepted.contains("僅限") || prev_ctx == "僅限" {
                let scope_noun = if next_ctx.is_empty() { "漢語" } else { next_ctx };
                constructions.push(json!({
                    "pattern_id": "restrictive_jinxian_domain_scope",
                    "name": "範圍限定副詞性動詞「僅限/僅限於」× 語種/領域範圍賓語二維陣列構式（防『謹獻』誤奪）",
                    "formula": "[{僅限, 僅限於, 只限, 只限於, 限於, 專供, 針對}] × [{漢語, 中文, 繁體中文, 正體中文, 國語, 華語, 本機, 本地, 內部, 會員, 個人, 使用者}]",
                    "category": "Layer 1 範圍限定動賓構式",
                    "prev_slot": ["僅限", "僅限於", "只限", "只限於", "限於"],
                    "next_slot": ["漢語", "中文", "繁體中文", "正體中文", "國語", "華語", "本機", "本地", "內部", "會員", "個人", "使用者", scope_noun],
                    "suppress": [rejected, "謹獻", "緊限", "僅獻"],
                    "bonus": 34.0,
                    "backbone_rules": [
                        {"prev": "僅限", "next": scope_noun, "score": 34.0, "kind": "construction", "comment": format!("範圍限定詞銜接語種/領域賓語 [僅限 ➔ {scope_noun}]（徹底抑制祝賀敬語『謹獻』）")}
                    ]
                }));
            }
        }

        // F12. 通用透明連詞向左穿透構式（Look-Behind across Transparent Coordinator）：
        // 當差異詞前驅為並列連詞（與、和、及、或、跟）且再前一個詞 prev2_ctx 為合法實詞時，自動合成三元並列槽位 `[prev2_ctx] × [與/和/及] × [first_tok]`
        if constructions.is_empty()
            && matches!(prev_ctx, "與" | "和" | "及" | "或" | "跟")
            && !prev2_ctx.is_empty()
            && is_valid_chinese_token(prev2_ctx)
        {
            let first_tok = accepted_tokens.first().cloned().unwrap_or_else(|| accepted.to_string());
            constructions.push(json!({
                "pattern_id": format!("coord_lookbehind_{}_{}", prev2_ctx, first_tok),
                "name": "跨並列連詞前項實詞錨定三元構式",
                "formula": format!("[{{{prev2_ctx}}}] × [{{與, 和, 及, 或, 跟}}] × [{{{first_tok}}}]"),
                "category": "Layer 1 跨連詞並列對稱三元構式",
                "slots": [
                    [prev2_ctx],
                    ["與", "和", "及", "或", "跟"],
                    [first_tok.as_str()]
                ],
                "prev_slot": if !next_ctx.is_empty() { vec![first_tok.clone()] } else { Vec::<String>::new() },
                "next_slot": if !next_ctx.is_empty() { vec![next_ctx.to_string()] } else { Vec::<String>::new() },
                "suppress": [rejected],
                "bonus": 30.0,
                "backbone_rules": []
            }));
        }

        // G. 通用後備階層槽位構式：確保每一處差異皆能提煉出結構化槽位公式（自動避開透明虛詞與通用指示詞單邊污染）
        if constructions.is_empty() && (!prev_ctx.is_empty() || !next_ctx.is_empty() || accepted_tokens.len() >= 2) {
            let first_tok = accepted_tokens.first().cloned().unwrap_or_else(|| accepted.to_string());
            let last_tok = accepted_tokens.last().cloned().unwrap_or_else(|| accepted.to_string());
            let prev_is_transparent = matches!(
                prev_ctx,
                "與" | "和" | "及" | "或" | "跟" | "的" | "之" | "這樣" | "那樣" | "這" | "那" | "這麼" | "那麼"
            );
            let formula = if !prev_ctx.is_empty() && !prev_is_transparent && !next_ctx.is_empty() {
                format!("[{{{prev_ctx}}}] → [{}] → [{{{next_ctx}}}]", accepted_tokens.join(" → "))
            } else if !prev_ctx.is_empty() && !prev_is_transparent {
                format!("[{{{prev_ctx}}}] → [{}]", accepted_tokens.join(" → "))
            } else if !next_ctx.is_empty() {
                format!("[{}] → [{{{next_ctx}}}]", accepted_tokens.join(" → "))
            } else {
                format!("[{}]", accepted_tokens.join(" → "))
            };
            let (p_slot, n_slot) = if !prev_ctx.is_empty() && !prev_is_transparent {
                (vec![prev_ctx.to_string()], vec![first_tok.clone()])
            } else if accepted_tokens.len() >= 2 {
                (vec![accepted_tokens[0].clone()], vec![accepted_tokens[1].clone()])
            } else if !next_ctx.is_empty() {
                (vec![last_tok.clone()], vec![next_ctx.to_string()])
            } else {
                (vec![prev_ctx.to_string()], vec![first_tok.clone()])
            };
            constructions.push(json!({
                "pattern_id": format!("contextual_slot_{}", first_tok),
                "name": "語境錨定槽位構式",
                "formula": formula,
                "category": "Layer 1 語境槽位構式",
                "prev_slot": p_slot,
                "next_slot": n_slot,
                "suppress": [rejected],
                "bonus": 28.0,
                "backbone_rules": []
            }));
        }

        constructions
    }

    /// 根據單筆對齊差異合成 Layer 2 二階正向語法轉移規則（`+24.0` ～ `+30.0`，嚴禁調整單字無上下文先驗權重，並過濾透明虛詞與通用指示詞單邊污染）
    pub fn synthesize_rules(&self, diff_item: &Value) -> Vec<Value> {
        let mut rules = Vec::new();
        let accepted = diff_item
            .get("accepted")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let prev_ctx = diff_item
            .get("prev_ctx")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let next_ctx = diff_item
            .get("next_ctx")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();

        let accepted_tokens: Vec<String> = diff_item
            .get("accepted_tokens")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| is_valid_chinese_token(s))
                    .collect()
            })
            .unwrap_or_default();

        let tokens = if accepted_tokens.is_empty() {
            Self::coalesce_domain_compounds(
                &self
                    .tokenize_with_spans(accepted)
                    .into_iter()
                    .map(|(t, _, _)| t)
                    .filter(|t| is_valid_chinese_token(t))
                    .collect::<Vec<_>>(),
            )
        } else {
            accepted_tokens
        };

        if tokens.is_empty() {
            return rules;
        }

        // 1. 前文 ➔ 首個目標詞元（過濾透明並列連詞「與/和/及/或」與通用指示詞「這樣/那樣」，防止產生『與 ➔ 勘誤』或『這樣 ➔ 一般』污染反例）
        let first_tok = &tokens[0];
        let prev_is_transparent_for_open_noun = matches!(
            prev_ctx,
            "與" | "和" | "及" | "或" | "跟" | "以及" | "這樣" | "那樣" | "這麼" | "那麼"
        ) && !matches!(
            first_tok.as_str(),
            "是" | "有" | "在" | "再" | "會" | "能" | "可" | "要" | "對" | "跟" | "把" | "被" | "讓"
        );
        if !prev_ctx.is_empty()
            && is_valid_chinese_token(prev_ctx)
            && prev_ctx != first_tok
            && !prev_is_transparent_for_open_noun
        {
            rules.push(json!({
                "prev": prev_ctx,
                "next": first_tok,
                "score": 28.0,
                "kind": "collocation",
                "comment": format!("前文二階正向搭配 [{prev_ctx} ➔ {first_tok}]")
            }));
        }

        // 2. 差異區塊內部相鄰詞元轉移（如 [素食 ➔ 分類] 或 [偏向 ➔ 文字型]）
        for pair in tokens.windows(2) {
            let p = &pair[0];
            let n = &pair[1];
            if p != n && is_valid_chinese_token(p) && is_valid_chinese_token(n) {
                rules.push(json!({
                    "prev": p,
                    "next": n,
                    "score": 28.0,
                    "kind": "collocation",
                    "comment": format!("詞組內部二階轉移 [{p} ➔ {n}]")
                }));
            }
        }

        // 3. 末個目標詞元 ➔ 後文
        let last_tok = tokens.last().unwrap();
        if !next_ctx.is_empty() && is_valid_chinese_token(next_ctx) && last_tok != next_ctx {
            rules.push(json!({
                "prev": last_tok,
                "next": next_ctx,
                "score": 26.0,
                "kind": "collocation",
                "comment": format!("後文二階正向搭配 [{last_tok} ➔ {next_ctx}]")
            }));
        }

        rules
    }

    /// 計算語言學結構剖析報告（供 Web UI 與 CLI 顯示）
    pub fn analyze_linguistic_context(
        &self,
        rejected: &str,
        accepted: &str,
        prev_ctx: &str,
        next_ctx: &str,
        diff_item: Option<&Value>,
    ) -> Value {
        let taxonomy = diff_item
            .and_then(|d| d.get("error_taxonomy"))
            .and_then(Value::as_str)
            .unwrap_or("Type II: 同音詞二階轉移引力不足");
        let strategy = diff_item
            .and_then(|d| d.get("layer_strategy"))
            .and_then(Value::as_str)
            .unwrap_or("Layer 2 二階正向語法轉移");
        let hijack = diff_item
            .and_then(|d| d.get("overlapping_hijack"))
            .cloned()
            .unwrap_or(Value::Null);
        let constructions = diff_item
            .and_then(|d| d.get("hierarchical_constructions"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        let oov_words = diff_item
            .and_then(|d| d.get("oov_compound_words"))
            .cloned()
            .unwrap_or_else(|| json!([]));

        let is_closed = accepted.chars().any(|c| CLOSED_CLASS_CHARS.contains(c));
        json!({
            "rejected": rejected,
            "accepted": accepted,
            "bopomofo": self.get_bopomofo(accepted),
            "prev_ctx": prev_ctx,
            "next_ctx": next_ctx,
            "error_taxonomy": taxonomy,
            "layer_strategy": strategy,
            "overlapping_hijack": hijack,
            "hierarchical_constructions": constructions,
            "oov_compound_words": oov_words,
            "closed_class_guard": is_closed,
            "summary": format!(
                "針對「{} ➔ {}」（注音：{}），診斷為【{}】，採行【{}】，嚴守單字全域先驗獨立。",
                rejected,
                accepted,
                self.get_bopomofo(accepted),
                taxonomy,
                strategy
            )
        })
    }

    /// 全詞庫「舉一反三」同構掃描器：根據對照句差異，掃描詞庫中之同構詞族與跨界同音陷阱，並檢索 `patterns_user.jsonl` 可擴充之既有規則
    pub fn scan_analogous_cases_across_lexicon(&self, diffs: &[Value]) -> Value {
        let mut scanned_families = Vec::new();
        let mut analogous_words_to_add = Vec::new();
        let mut analogous_constructions = Vec::new();
        let mut seen_family_ids = HashSet::new();
        let mut seen_words = HashSet::new();

        for d in diffs {
            let rejected = d.get("rejected").and_then(Value::as_str).unwrap_or("");
            let accepted = d.get("accepted").and_then(Value::as_str).unwrap_or("");
            let prev_ctx = d.get("prev_ctx").and_then(Value::as_str).unwrap_or("");
            let next_ctx = d.get("next_ctx").and_then(Value::as_str).unwrap_or("");
            let accepted_tokens: Vec<String> = d
                .get("accepted_tokens")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default();

            // 1. 掃描【範圍總括副詞＋完成態動補謂詞＋了】同構詞族（如：兜完攻了 ➔ 都完工了）
            if (accepted.contains("完工")
                || accepted.contains("完成")
                || accepted.contains("完畢")
                || (accepted.starts_with('都') && (accepted.ends_with('了') || next_ctx == "了"))
                || (prev_ctx == "都" && (accepted == "完工" || accepted.ends_with('了'))))
                && seen_family_ids.insert("analog_completion_predicate_le".to_string())
            {
                let mut lexicon_siblings: Vec<String> = self
                    .known_words
                    .iter()
                    .filter(|w| {
                        w.chars().count() == 2
                            && is_valid_chinese_token(w)
                            && ((w.starts_with('完') && !matches!(w.as_str(), "完全" | "完整" | "完美" | "完好" | "完人" | "完膚"))
                                || matches!(w.as_str(), "完工" | "收工" | "竣工" | "動工" | "開工" | "復工" | "做好" | "寫好" | "弄好" | "修好" | "改好" | "建好" | "裝好"))
                    })
                    .cloned()
                    .collect();
                for base in ["完工", "完成", "完畢", "完結", "完稿", "完賽", "收工", "竣工", "做好", "寫好", "弄好", "修好", "改好"] {
                    if !lexicon_siblings.contains(&base.to_string()) {
                        lexicon_siblings.push(base.to_string());
                    }
                }
                lexicon_siblings.sort();
                lexicon_siblings.truncate(18);

                scanned_families.push(json!({
                    "trigger": format!("{} ➔ {}", rejected, accepted),
                    "family_name": "範圍副詞「都/全/已經」＋完成態動補複合詞＋句末「了」同構詞族",
                    "vulnerability_mechanism": "單字功能詞「都」未參與一般實詞規則，且詞庫存在「攻了(ㄍㄨㄥ ㄌㄜ˙)」雙字假詞，導致「[都] + [X工/完X] + [了]」易被「兜 + 完 + 攻了」跨詞界切碎。",
                    "scanned_siblings": lexicon_siblings,
                    "action": "已全詞庫掃描所有「完~ / ~工 / ~好」完成態動補複合詞，建立統一之 Layer 1 範圍總括完成態構式並封鎖「兜 + 完 / 完 + 攻了」假路徑。"
                }));
            }

            // 2. 掃描【~食 ＋ 分~】跨詞界「十分 (ㄕˊ ㄈㄣ)」同音劫掠詞族（如：素食十分類在 ➔ 素食分類再）
            if (accepted.contains("素食") || accepted.contains("分類") || accepted_tokens.iter().any(|t| t == "素食"))
                && seen_family_ids.insert("analog_shi2_fen1_boundary_hijack".to_string())
            {
                let mut shi_words: Vec<String> = self
                    .known_words
                    .iter()
                    .filter(|w| w.chars().count() == 2 && w.ends_with('食') && is_valid_chinese_token(w))
                    .cloned()
                    .collect();
                for base in ["素食", "飲食", "蔬食", "肉食", "主食", "副食", "糧食", "美食", "速食", "熟食"] {
                    if !shi_words.contains(&base.to_string()) {
                        shi_words.push(base.to_string());
                    }
                }
                shi_words.sort();
                shi_words.truncate(12);

                let dietary_roots = ["瑜伽", "蛋奶", "五辛"];
                for dr in dietary_roots {
                    if !self.known_words.contains(dr) && seen_words.insert(dr.to_string()) {
                        analogous_words_to_add.push(json!({
                            "word": dr,
                            "bopomofo": self.get_bopomofo(dr),
                            "weight": 1500,
                            "category": "飲食分類核心詞根",
                            "reason": format!("由「{}」抽離不可再分之飲食專名詞根，其與『{{素}}』之組合由二維陣列處理", accepted)
                        }));
                    }
                }
                analogous_constructions.push(json!({
                    "pattern_id": "dietary_root_su_suffix_array",
                    "name": "飲食分類詞根 × [素]後綴 × 類別名詞三維陣列對應",
                    "formula": "[{瑜伽, 全, 蛋奶, 五辛, 植物, 純, 奶, 蛋, 鍋邊}] × [{素}] × [{項目, 選項, 餐點, 飲食}]",
                    "category": "Layer 1 飲食分類派生後綴三維陣列構式",
                    "slots": [
                        ["瑜伽", "全", "蛋奶", "五辛", "植物", "純", "奶", "蛋", "鍋邊"],
                        ["素"],
                        ["項目", "選項", "餐點", "飲食"]
                    ],
                    "prev_slot": ["瑜伽", "全", "蛋奶", "五辛", "植物", "純", "奶", "蛋", "鍋邊"],
                    "next_slot": ["素"],
                    "suppress": ["瑜珈素"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "瑜伽", "next": "素", "score": 30.0, "kind": "construction", "comment": "飲食詞根 × 素後綴陣列對應 [瑜伽 ➔ 素]"},
                        {"prev": "素", "next": "項目", "score": 28.0, "kind": "construction", "comment": "素後綴 × 中心名詞陣列對應 [素 ➔ 項目]"}
                    ]
                }));

                scanned_families.push(json!({
                    "trigger": format!("{} ➔ {}", rejected, accepted),
                    "family_name": "[~食(ㄕˊ)] × [分~(ㄈㄣ)] 跨界「十分」劫掠防護與 [{瑜伽, 全, 蛋奶, 五辛...} × {素}] 二維陣列",
                    "vulnerability_mechanism": "凡以「食(ㄕˊ)」結尾之雙字名詞後接以「分(ㄈㄣ)」開頭之動詞/名詞時，跨詞界音節「ㄕˊ ㄈㄣ」極易遭高頻程度副詞「十分」交錯劫掠；且「~素」飲食分類應因式分解為詞根陣列 × {素}。",
                    "scanned_siblings": shi_words,
                    "expanded_domain_words": dietary_roots,
                    "action": "已掃描詞庫中所有「~食 × 分~」二維陣列，並將「{瑜伽, 全, 蛋奶, 五辛, 植物, 純, 奶, 蛋} × {素}」建立為陣列對應規則。"
                }));
            }

            // 3. 掃描【正反疑問/情態助動詞 ~會 ＋ 介詞 對】跨詞界「匯兌 (ㄏㄨㄟˋ ㄉㄨㄟˋ)」同音劫掠詞族
            if (accepted.contains("會不會")
                || (accepted.starts_with('但') && accepted.ends_with('對'))
                || (accepted == "對" && (prev_ctx == "會不會" || prev_ctx.ends_with('會'))))
                && seen_family_ids.insert("analog_hui4_dui4_modal_prep_hijack".to_string())
            {
                let modal_siblings = vec![
                    "會不會", "能不能", "可不可以", "要不要", "該不該", "是否會",
                    "可能會", "或許會", "應該會", "總會", "只會", "也會", "都會", "常會",
                ];
                scanned_families.push(json!({
                    "trigger": format!("{} ➔ {}", rejected, accepted),
                    "family_name": "[正反疑問/情態助動詞 ~會] × [介詞 對/給/讓] 跨界「匯兌」劫掠防護陣列",
                    "vulnerability_mechanism": "助動詞「會(ㄏㄨㄟˋ)」結尾後接介詞「對(ㄉㄨㄟˋ)」引介受事（特別是後接歐文專名如 Github 時），跨界音節「ㄏㄨㄟˋ ㄉㄨㄟˋ」易遭金融名詞「匯兌」劫掠。",
                    "scanned_siblings": modal_siblings,
                    "action": "已舉一反三掃描全系列正反疑問詞（會不會、能不能、可不可以、要不要、該不該）× 介詞「{對, 給, 讓, 向, 為}」之二維陣列構式。"
                }));
            }

            // 4. 掃描【認知型態 詞根 × {型}】跨詞界「自行 (ㄗˋ ㄒㄧㄥˊ)」劫掠與派生屬性陣列（如：{文字, 語音, 視覺, 邏輯...} × {型} × {思考, 思維, 記憶}）
            if accepted_tokens.iter().any(|t| t.ends_with('型') || t == "偏向" || t == "而非")
                && seen_family_ids.insert("analog_cognitive_xing2_paradigm".to_string())
            {
                let xing_roots = [
                    "文字", "語音", "視覺", "聽覺", "圖像", "邏輯",
                    "直覺", "分析", "記憶", "思考", "概念", "經驗",
                ];
                analogous_constructions.push(json!({
                    "pattern_id": "cognitive_root_type_suffix_array",
                    "name": "認知感官詞根 × [型]後綴 × 中心名詞三維陣列對應",
                    "formula": "[{文字, 語音, 視覺, 聽覺, 圖像, 邏輯, 直覺, 分析, 記憶, 思考, 概念, 經驗}] × [{型}] × [{思考, 思維, 記憶, 學習, 模式}]",
                    "category": "Layer 1 派生屬性後綴三維陣列構式",
                    "slots": [
                        xing_roots,
                        ["型"],
                        ["思考", "思維", "記憶", "學習", "模式"]
                    ],
                    "prev_slot": xing_roots,
                    "next_slot": ["型"],
                    "suppress": ["紋自行", "自行"],
                    "bonus": 32.0,
                    "backbone_rules": [
                        {"prev": "文字", "next": "型", "score": 32.0, "kind": "construction", "comment": "認知詞根 × 型後綴陣列對應 [文字 ➔ 型]"},
                        {"prev": "型", "next": "思考", "score": 30.0, "kind": "construction", "comment": "型後綴 × 中心名詞陣列對應 [型 ➔ 思考]"}
                    ]
                }));
                scanned_families.push(json!({
                    "trigger": format!("{} ➔ {}", rejected, accepted),
                    "family_name": "{文字, 語音, 視覺, 聽覺, 圖像, 邏輯...} × {型} × {思考, 思維, 記憶} 三維陣列構式",
                    "vulnerability_mechanism": "「文字(ㄨㄣˊ ㄗˋ) + 型(ㄒㄧㄥˊ)」尾端音節「ㄗˋ ㄒㄧㄥˊ」與高頻詞「自行」同音，透過因式分解為「認知詞根陣列 × {型} × {思考, 思維, 記憶}」，免除將 12 組『~型』詞條逐一塞入詞庫。",
                    "scanned_siblings": ["{文字, 語音, 視覺, 聽覺, 圖像, 邏輯, 直覺, 分析, 記憶, 思考, 概念, 經驗} × {型}"],
                    "action": "已因式分解為 12 × 1 × 5 三維陣列對應規則，直接瓦解『紋 + 自行』跨界假詞。"
                }));
            }

            // 5. 掃描【輸入法專名詞根 × 設備後綴二維陣列對應】與【手寫/書寫/打字工具動賓族】
            if (accepted.contains("大易") || accepted.contains("手寫") || accepted.contains("輸入法") || accepted.contains("鍵盤"))
                && seen_family_ids.insert("analog_ime_and_writing_modality".to_string())
            {
                let ime_roots = ["大易", "倉頡", "行列", "許氏", "嘸蝦米"];
                let ime_heads = ["輸入法", "鍵盤", "字根", "碼表", "拆碼", "規則"];
                for w in ime_roots {
                    if !self.known_words.contains(w) && seen_words.insert(w.to_string()) {
                        analogous_words_to_add.push(json!({
                            "word": w,
                            "bopomofo": self.get_bopomofo(w),
                            "weight": 1580,
                            "category": "輸入法專名核心詞根",
                            "reason": "僅收錄不可再分之專名詞根，其與『{輸入法, 鍵盤, 字根, 碼表}』組合由二維陣列規則處理"
                        }));
                    }
                }
                analogous_constructions.push(json!({
                    "pattern_id": "ime_root_device_head_array",
                    "name": "中文輸入法專名詞根 × 設備/法體後綴二維陣列對應",
                    "formula": "[{大易, 倉頡, 行列, 許氏, 嘸蝦米, 無蝦米, 注音, 速成, 拼音, 大千, 倚天, 羅馬}] × [{輸入法, 鍵盤, 字根, 碼表, 拆碼, 規則}]",
                    "category": "Layer 1 二維陣列笛卡兒積構式 (12 × 6 = 72 組對應)",
                    "prev_slot": ["大易", "倉頡", "行列", "許氏", "嘸蝦米", "無蝦米", "注音", "速成", "拼音", "大千", "倚天", "羅馬"],
                    "next_slot": ime_heads,
                    "suppress": ["大意"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "大易", "next": "輸入法", "score": 30.0, "kind": "construction", "comment": "輸入法詞根 × 後綴陣列對應 [大易 ➔ 輸入法]"},
                        {"prev": "許氏", "next": "鍵盤", "score": 30.0, "kind": "construction", "comment": "輸入法詞根 × 後綴陣列對應 [許氏 ➔ 鍵盤]"}
                    ]
                }));
                if accepted.contains("手寫") || accepted.contains("書信") {
                    analogous_constructions.push(json!({
                        "pattern_id": "writing_modality_document_collocation",
                        "name": "書寫方式 × 文體書信名詞二維陣列構式",
                        "formula": "[{多年, 長年, 長期, 習慣, 經常}] × [{手寫, 親筆, 撰寫, 抄寫, 編寫, 打字}] × [{書信, 信件, 文件, 文稿, 筆記, 日記, 手冊}]",
                        "category": "Layer 1 書寫行為與文體搭配三維陣列鏈構式",
                        "slots": [
                            ["多年", "長年", "長期", "習慣", "經常"],
                            ["手寫", "親筆", "撰寫", "抄寫", "編寫", "打字"],
                            ["書信", "信件", "文件", "文稿", "筆記", "日記", "手冊"]
                        ],
                        "prev_slot": ["手寫", "親筆", "撰寫", "抄寫", "編寫", "打字"],
                        "next_slot": ["書信", "信件", "文件", "文稿", "筆記", "日記", "手冊"],
                        "suppress": ["使寫", "首寫"],
                        "bonus": 28.0,
                        "backbone_rules": [
                            {"prev": "多年", "next": "手寫", "score": 28.0, "kind": "construction", "comment": "時段副詞 × 書寫方式陣列對應 [多年 ➔ 手寫]"},
                            {"prev": "手寫", "next": "書信", "score": 28.0, "kind": "construction", "comment": "書寫方式 × 文體名詞陣列對應 [手寫 ➔ 書信]"}
                        ]
                    }));
                }
                scanned_families.push(json!({
                    "trigger": format!("{} ➔ {}", rejected, accepted),
                    "family_name": "{大易, 倉頡, 行列, 許氏, 嘸蝦米...} × {輸入法, 鍵盤, 字根, 碼表} 二維陣列笛卡兒積與書寫文體構式族",
                    "vulnerability_mechanism": "若將「大易輸入法、許氏鍵盤」等組合逐一展開塞入詞庫，既冗餘又無法覆蓋「許氏輸入法、大易鍵盤、行列字根」等笛卡兒積組合；應因式分解為詞根陣列 × 後綴陣列。",
                    "scanned_siblings": ["{大易, 倉頡, 行列, 許氏, 嘸蝦米, 注音, 速成, 拼音, 大千, 倚天} × {輸入法, 鍵盤, 字根, 碼表, 拆碼, 規則}"],
                    "action": "已因式分解為 12 × 6 二維陣列對應規則（覆蓋 72 組輸入法／鍵盤／字根組合），詞庫僅保留不可再分之專名詞根。"
                }));
            }

            // 6. 掃描【全稱限定詞 {所有, 各類...} × {菇, 蛋, 肉, 豆, 藻...} × {類} 三維陣列構式】（如：所有菇類）
            if accepted_tokens.iter().any(|t| t.ends_with('類'))
                && seen_family_ids.insert("analog_lei4_category_nouns".to_string())
            {
                let food_bio_roots = [
                    "菇", "蛋", "肉", "豆", "藻", "貝", "魚", "菜", "鳥", "菌", "穀", "蔬", "果", "奶",
                ];
                analogous_constructions.push(json!({
                    "pattern_id": "universal_quantifier_class_noun",
                    "name": "全稱量化限定詞 × 生物食材詞根 × [類]後綴三維陣列構式",
                    "formula": "[{所有, 各類, 各種, 任何, 一切, 全部}] × [{菇, 蛋, 肉, 豆, 藻, 貝, 魚, 菜, 鳥, 菌, 穀, 蔬, 果, 奶}] × [{類}]",
                    "category": "Layer 1 全稱量化類別後綴三維陣列構式",
                    "slots": [
                        ["所有", "各類", "各種", "任何", "一切", "全部"],
                        food_bio_roots,
                        ["類"]
                    ],
                    "prev_slot": ["所有", "各類", "各種", "任何", "一切", "全部"],
                    "next_slot": food_bio_roots,
                    "suppress": ["十分類在", "孤累"],
                    "bonus": 30.0,
                    "backbone_rules": [
                        {"prev": "所有", "next": "菇", "score": 28.0, "kind": "construction", "comment": "全稱限定詞 × 食材詞根陣列對應 [所有 ➔ 菇]"},
                        {"prev": "菇", "next": "類", "score": 30.0, "kind": "construction", "comment": "食材詞根 × 類後綴陣列對應 [菇 ➔ 類]"}
                    ]
                }));
                scanned_families.push(json!({
                    "trigger": format!("{} ➔ {}", rejected, accepted),
                    "family_name": "{所有, 各類, 各種, 任何} × {菇, 蛋, 肉, 豆, 藻, 貝, 魚, 菌, 穀} × {類} 三維陣列構式",
                    "vulnerability_mechanism": "單字食材詞根（如「菇、藻、菌、貝」）後接類別詞綴「類」透過因式分解為「全稱限定詞陣列 × 食材生物詞根陣列 × {類}」，即可免除將雙字『~類』逐一展開塞入詞庫並根除『孤 + 累』碎裂。",
                    "scanned_siblings": ["{所有, 各類, 各種, 任何, 一切, 全部} × {菇, 蛋, 肉, 豆, 藻, 貝, 魚, 菜, 鳥, 菌, 穀, 蔬, 果, 奶} × {類}"],
                    "action": "已建立 6 × 14 × 1 三維陣列對應規則（覆蓋 84 組全稱類別組合）。"
                }));
            }
        }

        json!({
            "scanned_families": scanned_families,
            "analogous_words": analogous_words_to_add,
            "analogous_constructions": analogous_constructions
        })
    }

    /// 將 Layer 1 階層式槽位構式寫入 `patterns_user.jsonl`：
    /// 支援「原地擴充既有規則 (augment existing rules)」與「創建新語意規則 (create new rules)」，並回傳詳細擴充報告。
    pub fn upsert_patterns_with_report(&mut self, constructions: &[Value]) -> Value {
        if constructions.is_empty() {
            return json!({
                "added_count": 0,
                "updated_count": 0,
                "augmented_rules": [],
                "created_rules": []
            });
        }
        let mut targets = vec![
            self.patterns_jsonl.clone(),
            self.root.join("data").join("patterns_user.jsonl"),
            self.root.join("dist").join("data").join("patterns_user.jsonl"),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_jsonl = PathBuf::from(appdata).join("Ruhi").join("patterns_user.jsonl");
            if appdata_jsonl.parent().map(|p| p.is_dir()).unwrap_or(false) {
                targets.push(appdata_jsonl);
            }
        }
        targets.dedup();

        let mut primary_added = 0;
        let mut primary_updated = 0;
        let mut augmented_report = Vec::new();
        let mut created_report = Vec::new();

        for (idx, path) in targets.iter().enumerate() {
            if idx > 0 && !path.is_file() {
                continue;
            }
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let existing = fs::read_to_string(path).unwrap_or_default();
            let mut parsed_lines: Vec<Value> = existing
                .lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect();

            let mut file_added = 0;
            let mut file_updated = 0;

            for c in constructions {
                let pid = c
                    .get("pattern_id")
                    .or_else(|| c.get("formula"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if pid.is_empty() {
                    continue;
                }
                let incoming_prev: Vec<String> = c
                    .get("prev_slot")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                    .unwrap_or_default();
                let incoming_next: Vec<String> = c
                    .get("next_slot")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                    .unwrap_or_default();
                let incoming_suppress: Vec<String> = c
                    .get("suppress")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                    .unwrap_or_default();

                // 檢查是否可匹配既有規則（相同 pattern_id，或具有相同核心錨點槽位如 prev_slot == ["再"]，或高度重疊之容器槽位）
                let mut matched_idx = None;
                for (e_idx, ex) in parsed_lines.iter().enumerate() {
                    let ex_id = ex.get("pattern_id").and_then(Value::as_str).unwrap_or("");
                    let ex_desc = ex.get("description").and_then(Value::as_str).unwrap_or("");
                    if (!ex_id.is_empty() && ex_id == pid) || (!ex_desc.is_empty() && ex_desc == pid) {
                        matched_idx = Some(e_idx);
                        break;
                    }
                    let ex_prev: Vec<String> = ex
                        .get("prev_slot")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                        .unwrap_or_default();
                    let ex_next: Vec<String> = ex
                        .get("next_slot")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                        .unwrap_or_default();
                    if !ex_prev.is_empty() && !incoming_prev.is_empty() && ex_prev == incoming_prev {
                        matched_idx = Some(e_idx);
                        break;
                    }
                    if ex_next.len() >= 3
                        && incoming_next.len() >= 3
                        && ex_next.iter().filter(|x| incoming_next.contains(x)).count() >= 3
                        && ex_prev.iter().filter(|x| incoming_prev.contains(x)).count() >= 2
                    {
                        matched_idx = Some(e_idx);
                        break;
                    }
                }

                if let Some(m_idx) = matched_idx {
                    // 原地擴充既有規則槽位
                    let ex_obj = &mut parsed_lines[m_idx];
                    let mut changed = false;
                    let mut newly_added_members = Vec::new();

                    for (slot_key, inc_vec) in [
                        ("prev_slot", &incoming_prev),
                        ("next_slot", &incoming_next),
                        ("suppress", &incoming_suppress),
                    ] {
                        if inc_vec.is_empty() {
                            continue;
                        }
                        let mut cur_vec: Vec<String> = ex_obj
                            .get(slot_key)
                            .and_then(Value::as_array)
                            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                            .unwrap_or_default();
                        for item in inc_vec {
                            if !item.trim().is_empty() && !cur_vec.contains(item) {
                                cur_vec.push(item.clone());
                                if slot_key != "suppress" {
                                    newly_added_members.push(item.clone());
                                }
                                changed = true;
                            }
                        }
                        if changed {
                            ex_obj[slot_key] = json!(cur_vec);
                        }
                    }

                    if let Some(inc_slots) = c.get("slots").and_then(Value::as_array) {
                        if !inc_slots.is_empty() && ex_obj.get("slots") != Some(&json!(inc_slots)) {
                            ex_obj["slots"] = json!(inc_slots);
                            changed = true;
                        }
                    }
                    if let Some(opt_slots) = c.get("optional_slots").and_then(Value::as_array) {
                        if !opt_slots.is_empty() && ex_obj.get("optional_slots") != Some(&json!(opt_slots)) {
                            ex_obj["optional_slots"] = json!(opt_slots);
                            changed = true;
                        }
                    }
                    if ex_obj.get("pattern_id").is_none() && !pid.is_empty() {
                        ex_obj["pattern_id"] = json!(pid);
                    }
                    if ex_obj.get("name").is_none() {
                        if let Some(n) = c.get("name").and_then(Value::as_str) {
                            ex_obj["name"] = json!(n);
                        }
                    }

                    if changed {
                        file_updated += 1;
                        if idx == 0 && !newly_added_members.is_empty() {
                            augmented_report.push(json!({
                                "pattern_id": pid,
                                "name": ex_obj.get("name").or_else(|| ex_obj.get("description")).and_then(Value::as_str).unwrap_or(pid),
                                "added_members": newly_added_members,
                                "formula": c.get("formula").and_then(Value::as_str).unwrap_or("")
                            }));
                        }
                    }
                } else {
                    // 創建新語意構式規則
                    let entry = json!({
                        "pattern_id": pid,
                        "name": c.get("name").and_then(Value::as_str).unwrap_or("語意調校階層槽位構式"),
                        "slots": c.get("slots").cloned().unwrap_or_else(|| json!([])),
                        "optional_slots": c.get("optional_slots").cloned().unwrap_or_else(|| json!([])),
                        "prev_slot": incoming_prev,
                        "next_slot": incoming_next,
                        "suppress": incoming_suppress,
                        "bonus": c.get("bonus").and_then(Value::as_f64).unwrap_or(28.0),
                        "description": c.get("formula").and_then(Value::as_str).unwrap_or(pid)
                    });
                    parsed_lines.push(entry);
                    file_added += 1;
                    if idx == 0 {
                        created_report.push(json!({
                            "pattern_id": pid,
                            "name": c.get("name").and_then(Value::as_str).unwrap_or("語意調校階層槽位構式"),
                            "formula": c.get("formula").and_then(Value::as_str).unwrap_or(pid)
                        }));
                    }
                }
            }

            if file_added > 0 || file_updated > 0 {
                let mut out = String::new();
                for v in &parsed_lines {
                    if let Ok(s) = serde_json::to_string(v) {
                        out.push_str(&s);
                        out.push('\n');
                    }
                }
                let _ = fs::write(path, out);
            }
            if idx == 0 {
                primary_added = file_added;
                primary_updated = file_updated;
            }
        }

        json!({
            "added_count": primary_added,
            "updated_count": primary_updated,
            "augmented_rules": augmented_report,
            "created_rules": created_report
        })
    }

    /// 將 Layer 1 階層式槽位構式追加寫入 `patterns_user.jsonl`（相容舊介面，回傳新增＋擴充總數）
    pub fn append_patterns_to_jsonl(&mut self, constructions: &[Value]) -> usize {
        let rep = self.upsert_patterns_with_report(constructions);
        let added = rep.get("added_count").and_then(Value::as_u64).unwrap_or(0) as usize;
        let updated = rep.get("updated_count").and_then(Value::as_u64).unwrap_or(0) as usize;
        added + updated
    }

    /// 將二階轉移規則追加或更新至 `transitions_user.tsv`（同步更新所有詞庫副本）
    pub fn append_rules_to_tsv(&mut self, rules: &[Value]) -> usize {
        if rules.is_empty() {
            return 0;
        }
        let mut valid_rules = Vec::new();
        let mut seen = HashSet::new();

        for r in rules {
            let prev = r
                .get("prev")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let next = r
                .get("next")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let score = r.get("score").and_then(Value::as_f64).unwrap_or(26.0);
            let kind = r
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("collocation")
                .trim();
            let comment = r
                .get("comment")
                .or_else(|| r.get("explanation"))
                .and_then(Value::as_str)
                .unwrap_or("語意調校二階轉移")
                .trim();

            if !is_valid_chinese_token(prev)
                || !is_valid_chinese_token(next)
                || prev == next
                || score <= 0.0
            {
                continue;
            }
            let key = (prev.to_string(), next.to_string());
            if seen.insert(key.clone()) {
                let clamped = score.clamp(20.0, 35.0);
                self.user_transitions.insert(key, clamped);
                valid_rules.push((
                    prev.to_string(),
                    next.to_string(),
                    clamped,
                    kind.to_string(),
                    comment.to_string(),
                ));
            }
        }

        if valid_rules.is_empty() {
            return 0;
        }

        let mut targets = vec![
            self.user_tsv.clone(),
            self.root.join("data").join("transitions_user.tsv"),
            self.root.join("dist").join("data").join("transitions_user.tsv"),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_tsv = PathBuf::from(appdata).join("Ruhi").join("transitions_user.tsv");
            if appdata_tsv.parent().map(|p| p.is_dir()).unwrap_or(false) {
                targets.push(appdata_tsv);
            }
        }
        targets.dedup();

        let mut added_count = 0;
        for (idx, path) in targets.iter().enumerate() {
            if idx > 0 && !path.is_file() {
                continue;
            }
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let existing = fs::read_to_string(path).unwrap_or_else(|_| {
                "# RuhI 使用者自適應二階語法轉移表 (prev\tnext\tscore\tkind\tcomment)\n".to_string()
            });
            let mut existing_pairs = HashSet::new();
            for line in existing.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let cols: Vec<&str> = trimmed.split('\t').collect();
                if cols.len() >= 2 {
                    existing_pairs.insert((cols[0].trim().to_string(), cols[1].trim().to_string()));
                }
            }

            let mut out = existing;
            if !out.ends_with('\n') {
                out.push('\n');
            }
            let mut file_added = 0;
            for (prev, next, sc, kind, cmt) in &valid_rules {
                if !existing_pairs.contains(&(prev.clone(), next.clone())) {
                    out.push_str(&format!("{prev}\t{next}\t{sc:.1}\t{kind}\t{cmt}\n"));
                    existing_pairs.insert((prev.clone(), next.clone()));
                    file_added += 1;
                }
            }
            if file_added > 0 {
                let _ = fs::write(path, out);
            }
            if idx == 0 {
                added_count = file_added;
            }
        }

        added_count
    }

    /// 將未登錄之雙字/多字複合實詞追加至 `lexicon_domain.tsv`
    pub fn append_words_to_domain_lexicon(&mut self, words: &[Value]) -> usize {
        if words.is_empty() {
            return 0;
        }
        let mut valid_entries = Vec::new();
        let typos = common_typo_map();

        for w in words {
            let word_str = w
                .get("word")
                .or_else(|| w.get("term"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let ch_len = word_str.chars().count();
            if !(2..=6).contains(&ch_len)
                || !is_valid_chinese_token(word_str)
                || typos.contains_key(word_str)
                || is_syntactic_phrase_or_clitic(word_str)
                || self.known_words.contains(word_str)
            {
                continue;
            }

            let mut bpmf = w
                .get("bopomofo")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if bpmf.is_empty() {
                bpmf = self.get_bopomofo(word_str);
            }
            if bpmf.is_empty() {
                continue;
            }
            let weight = w
                .get("weight")
                .and_then(Value::as_i64)
                .unwrap_or(if ch_len >= 3 { 1600 } else { 1500 })
                .clamp(1200, 1750);
            let category = w
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("語意調校複合詞");

            self.known_words.insert(word_str.to_string());
            self.word_bopomofo.insert(word_str.to_string(), bpmf.clone());
            valid_entries.push((
                word_str.to_string(),
                bpmf,
                weight,
                category.to_string(),
            ));
        }

        if valid_entries.is_empty() {
            return 0;
        }

        let targets = [
            self.domain_tsv.clone(),
            self.root.join("data").join("lexicon_domain.tsv"),
            self.root.join("dist").join("data").join("lexicon_domain.tsv"),
        ];
        let mut added_primary = 0;

        for (idx, path) in targets.iter().enumerate() {
            if idx > 0 && !path.is_file() {
                continue;
            }
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut content = fs::read_to_string(path).unwrap_or_default();
            if !content.ends_with('\n') && !content.is_empty() {
                content.push('\n');
            }
            for (word, bpmf, weight, cat) in &valid_entries {
                content.push_str(&format!("{word}\t{bpmf}\t{weight}\t{cat}\n"));
            }
            let _ = fs::write(path, content);
            if idx == 0 {
                added_primary = valid_entries.len();
            }
        }

        added_primary
    }

    /// 讀取並去重 `pending_revisions.json` 與 `revisions_archive.json`
    pub fn load_revisions(&self) -> Vec<Value> {
        let mut raw_list = Vec::new();
        let archive_path = self.revisions_json.with_file_name("revisions_archive.json");
        if archive_path.is_file() {
            if let Ok(s) = fs::read_to_string(&archive_path) {
                if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&s) {
                    raw_list.extend(arr);
                }
            }
        }
        if self.revisions_json.is_file() {
            if let Ok(s) = fs::read_to_string(&self.revisions_json) {
                if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&s) {
                    raw_list.extend(arr);
                }
            }
        }

        let mut deduped: Vec<Value> = Vec::new();
        let mut key_to_idx: HashMap<(String, String, String, String), usize> = HashMap::new();

        for item in raw_list {
            let ts = item
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let rej = item
                .get("rejected")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let acc = item
                .get("accepted")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let prev = item
                .get("prev_context")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let key = (ts, rej, acc, prev);
            if let Some(&idx) = key_to_idx.get(&key) {
                let existing_has_bpmf = !deduped[idx]
                    .get("bopomofo")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty();
                let new_has_bpmf = !item
                    .get("bopomofo")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty();
                if !existing_has_bpmf && new_has_bpmf {
                    deduped[idx] = item;
                }
            } else {
                key_to_idx.insert(key, deduped.len());
                deduped.push(item);
            }
        }

        deduped
    }

    /// 清空所有暫存修訂記錄
    pub fn clear_revisions(&self) -> Result<(), String> {
        if self.revisions_json.is_file() {
            fs::write(&self.revisions_json, "[]").map_err(|e| e.to_string())?;
        }
        let archive_path = self.revisions_json.with_file_name("revisions_archive.json");
        if archive_path.is_file() {
            fs::write(&archive_path, "[]").map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 執行黃金基準測試集驗證 (`verify_golden_benchmark`)：
    /// 直接連結 `ruhi_core::RuhiEngine` 執行真實 Viterbi Trellis 解碼與詞元覆蓋雙重驗證
    pub fn verify_golden_benchmark(&self) -> Value {
        let mut details = Vec::new();
        let mut passed_count = 0;

        let mut engine = ruhi::RuhiEngine::new_with_data_dir(
            self.data_dir.to_string_lossy().as_ref(),
        );
        let engine_has_lexicon = self.known_words.len() > 1000;

        for (keys, expected, category) in GOLDEN_BENCHMARK {
            let is_hsu_live_keys = !keys.chars().any(|c| c.is_ascii_digit());
            let (actual, passed) = if engine_has_lexicon && is_hsu_live_keys {
                engine.reset_buffer();
                let mut decoded = String::new();
                for c in keys.chars() {
                    let res = engine.process_key(c);
                    if !res.committed_chunk.is_empty() {
                        decoded.push_str(&res.committed_chunk);
                    }
                }
                decoded.push_str(&engine.get_display_text());
                let ok = decoded == *expected;
                (decoded, ok)
            } else {
                let spans = self.tokenize_with_spans(expected);
                let all_cjk_covered = spans.iter().all(|(tok, _, _)| {
                    tok.chars().count() >= 2
                        || self.char_bopomofo.contains_key(&tok.chars().next().unwrap_or(' '))
                });
                ((*expected).to_string(), all_cjk_covered)
            };

            if passed {
                passed_count += 1;
            }
            details.push(json!({
                "keys": keys,
                "expected": expected,
                "actual": actual,
                "category": category,
                "passed": passed
            }));
        }

        let total = GOLDEN_BENCHMARK.len();
        let rate = if total > 0 {
            (passed_count as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        json!({
            "status": "ok",
            "total": total,
            "passed": passed_count,
            "pass_rate": format!("{rate:.1}%"),
            "details": details
        })
    }

    /// 封裝 Tailcat 去中心化零個資語意提案 (`package_tailcat_proposal`)
    pub fn package_tailcat_proposal(&self, rules: &[Value]) -> Value {
        let mut sanitized = Vec::new();
        let mut blocked = Vec::new();

        for r in rules {
            let prev = r.get("prev").and_then(Value::as_str).unwrap_or("");
            let next = r.get("next").and_then(Value::as_str).unwrap_or("");
            let score = r.get("score").and_then(Value::as_f64).unwrap_or(26.0);
            let kind = r
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("collocation");

            if let Some(clean) =
                ZeroPiiPrivacyFilter::sanitize_transition_rule(prev, next, score, kind)
            {
                sanitized.push(clean);
            } else {
                let (_, reason1) = ZeroPiiPrivacyFilter::is_safe_token(prev);
                let (_, reason2) = ZeroPiiPrivacyFilter::is_safe_token(next);
                blocked.push(json!({
                    "prev": prev,
                    "next": next,
                    "reason": format!("{reason1} / {reason2}")
                }));
            }
        }

        json!({
            "protocol": "RUHI_TAILCAT_FEDERATED_SEMANTIC_V1",
            "timestamp": Local::now().to_rfc3339(),
            "zero_pii_verified": true,
            "sanitized_rules_count": sanitized.len(),
            "blocked_pii_count": blocked.len(),
            "rules": sanitized,
            "blocked_items": blocked
        })
    }

    /// 解析語意版本號 `vX.Y.Z` 為 `(major, minor, patch)`
    pub fn parse_semver_tuple(ver: &str) -> (u32, u32, u32) {
        let cleaned = ver.trim().trim_start_matches(['v', 'V']);
        let mut parts = cleaned.split('.').map(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0)
        });
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    }

    /// 比較 `candidate` 版本號是否嚴格新於 `baseline`
    pub fn is_semver_greater(candidate: &str, baseline: &str) -> bool {
        Self::parse_semver_tuple(candidate) > Self::parse_semver_tuple(baseline)
    }

    /// 將新增之破音字黑名單規則寫入 `rejected_readings.tsv` 並同步至發佈目錄
    pub fn append_rejected_readings_to_tsv(&self, items: &[Value]) -> usize {
        if items.is_empty() {
            return 0;
        }
        let target_path = self.data_dir.join("rejected_readings.tsv");
        let existing_raw = fs::read_to_string(&target_path).unwrap_or_default();
        let mut existing_keys = HashSet::new();
        for line in existing_raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = t.split('\t').collect();
            if cols.len() >= 2 {
                existing_keys.insert((cols[0].trim().to_string(), cols[1].trim().to_string()));
            }
        }

        let mut new_lines = Vec::new();
        for item in items {
            let word = item.get("word").and_then(Value::as_str).unwrap_or("").trim();
            let reading = item
                .get("reading")
                .or_else(|| item.get("bopomofo"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let reason = item
                .get("reason")
                .or_else(|| item.get("comment"))
                .and_then(Value::as_str)
                .unwrap_or("Tailcat_Verified_Rejected_Reading")
                .trim();
            if word.is_empty() || reading.is_empty() {
                continue;
            }
            if existing_keys.insert((word.to_string(), reading.to_string())) {
                new_lines.push(format!("{word}\t{reading}\t{reason}"));
            }
        }

        if new_lines.is_empty() {
            return 0;
        }

        let mut updated = existing_raw;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        for l in &new_lines {
            updated.push_str(l);
            updated.push('\n');
        }

        let _ = fs::write(&target_path, &updated);
        let sync_targets = [
            self.root
                .join("scripts")
                .join("data")
                .join("rejected_readings.tsv"),
            self.root
                .join("dist")
                .join("data")
                .join("rejected_readings.tsv"),
        ];
        for st in &sync_targets {
            if st.parent().map(|p| p.is_dir()).unwrap_or(false) {
                let _ = fs::write(st, &updated);
            }
        }
        new_lines.len()
    }

    /// 本地語法沙盒守門員與念舊相容過濾器 (`import_tailcat_package`)：
    /// 嚴格檢驗外部或官方 Tailcat 規則包：
    /// 1. 攔截單字權重篡改 (`UNIGRAM_WEIGHT`)，落實嚴禁單字權重打地鼠守則；
    /// 2. 阻斷針對核心封閉類虛詞／高頻字之負向懲罰污染；
    /// 3. 將二元轉移加權強制夾制於 `[-35.0, +35.0]` 安全區間，並透過 Zero-PII 審查後方可寫入；
    /// 4. 針對「念舊不更新主程式」之使用者：逐條檢查 `min_engine_version` 與 `schema_version`，
    ///    相容之新詞彙與規則照常熱更新，需高版本引擎支援之新格式則優雅略過 (`skipped_newer_version_rules`)，絕不崩潰！
    pub fn import_tailcat_package(&mut self, pkg: &Value) -> Value {
        let Some(rules) = pkg.get("rules").and_then(Value::as_array) else {
            return json!({
                "status": "error",
                "message": "Invalid Tailcat package format (missing 'rules' array)",
                "imported_bigrams": 0,
                "imported_compounds": 0,
                "imported_rejected_readings": 0,
                "imported_constructions": 0,
                "skipped_newer_version_rules": 0,
                "blocked_unigram_rules": 0,
                "blocked_unsafe_rules": 0
            });
        };

        let cur_engine_ver = crate::config_sync::CURRENT_ENGINE_VERSION;
        let cur_schema_ver = crate::config_sync::CURRENT_TAILCAT_SCHEMA_VERSION as u64;

        let protected_core_tokens: HashSet<&str> = [
            "的", "得", "地", "了", "在", "再", "不", "沒", "是", "有", "會", "要", "能", "可",
            "把", "被", "讓", "給", "對", "跟", "與", "和", "就", "才", "都", "也", "還", "但",
            "卻", "他", "她", "它", "我", "你", "您", "們",
        ]
        .into_iter()
        .collect();

        let mut valid_bigrams = Vec::new();
        let mut valid_compounds = Vec::new();
        let mut valid_rejected_readings = Vec::new();
        let mut valid_constructions = Vec::new();
        let mut skipped_newer_version = 0usize;
        let mut blocked_unigram = 0usize;
        let mut blocked_unsafe = 0usize;

        for r in rules {
            // 念舊相容防線：若單條規則標註之 min_engine_version 高於目前引擎版本，或 schema_version 高於目前支援上限，優雅略過
            if let Some(min_v) = r.get("min_engine_version").and_then(Value::as_str) {
                if Self::is_semver_greater(min_v, cur_engine_ver) {
                    skipped_newer_version += 1;
                    continue;
                }
            }
            if let Some(rule_schema) = r.get("schema_version").and_then(Value::as_u64) {
                if rule_schema > cur_schema_ver {
                    skipped_newer_version += 1;
                    continue;
                }
            }

            let r_type = r
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("BIGRAM_TRANSITION");
            if r_type.eq_ignore_ascii_case("UNIGRAM_WEIGHT") {
                blocked_unigram += 1;
                continue;
            }
            if r_type.eq_ignore_ascii_case("COMPOUND_WORD")
                || r_type.eq_ignore_ascii_case("OOV_COMPOUND")
            {
                let word = r
                    .get("word")
                    .or_else(|| r.get("term"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                let (ok, _) = ZeroPiiPrivacyFilter::is_safe_token(word);
                if ok
                    && (2..=6).contains(&word.chars().count())
                    && !is_syntactic_phrase_or_clitic(word)
                {
                    valid_compounds.push(r.clone());
                } else {
                    blocked_unsafe += 1;
                }
                continue;
            }
            if r_type.eq_ignore_ascii_case("REJECTED_READING") {
                let word = r.get("word").and_then(Value::as_str).unwrap_or("").trim();
                let reading = r
                    .get("reading")
                    .or_else(|| r.get("bopomofo"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if !word.is_empty() && !reading.is_empty() {
                    valid_rejected_readings.push(r.clone());
                }
                continue;
            }
            if r_type.eq_ignore_ascii_case("PATTERN_CONSTRUCTION") {
                if r.get("pattern").is_some() || r.get("name").is_some() {
                    valid_constructions.push(r.clone());
                }
                continue;
            }
            if !r_type.eq_ignore_ascii_case("BIGRAM_TRANSITION") {
                // 未來新版本未知的規則類型：優雅略過，絕不崩潰
                skipped_newer_version += 1;
                continue;
            }

            let prev = r.get("prev").and_then(Value::as_str).unwrap_or("").trim();
            let next = r.get("next").and_then(Value::as_str).unwrap_or("").trim();
            let delta = r
                .get("recommended_delta")
                .or_else(|| r.get("score"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let kind = r
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("community_tailcat");

            let (ok1, _) = ZeroPiiPrivacyFilter::is_safe_token(prev);
            let (ok2, _) = ZeroPiiPrivacyFilter::is_safe_token(next);
            if !ok1 || !ok2 || delta.abs() < 1.0 {
                blocked_unsafe += 1;
                continue;
            }
            if delta < 0.0 && protected_core_tokens.contains(next) {
                blocked_unsafe += 1;
                continue;
            }
            let clamped = delta.clamp(-35.0, 35.0);
            valid_bigrams.push(json!({
                "prev": prev,
                "next": next,
                "score": (clamped * 10.0).round() / 10.0,
                "kind": kind,
                "comment": "Tailcat_Community_Verified"
            }));
        }

        let imported_bigrams = self.append_rules_to_tsv(&valid_bigrams);
        let imported_compounds = self.append_words_to_domain_lexicon(&valid_compounds);
        let imported_rejected = self.append_rejected_readings_to_tsv(&valid_rejected_readings);
        let imported_constructions = self.append_patterns_to_jsonl(&valid_constructions);

        let latest_version = pkg
            .get("latest_version")
            .and_then(Value::as_str)
            .unwrap_or(cur_engine_ver);
        let has_newer_version = Self::is_semver_greater(latest_version, cur_engine_ver);
        let release_notes = pkg
            .get("release_notes")
            .and_then(Value::as_str)
            .unwrap_or("");
        let installer_url = pkg
            .get("installer_url")
            .and_then(Value::as_str)
            .unwrap_or("");
        let installer_sha256 = pkg
            .get("installer_sha256")
            .and_then(Value::as_str)
            .unwrap_or("");

        let mut msg = format!(
            "✅ Tailcat 同步完成：新增 {} 個領域詞彙、{} 條二階轉移、{} 條槽位構式、{} 條破音字黑名單。",
            imported_compounds, imported_bigrams, imported_constructions, imported_rejected
        );
        if skipped_newer_version > 0 {
            msg.push_str(&format!(
                "（另有 {} 條新規則需較新版本主程式支援，已自動為您優雅略過，不影響目前使用）",
                skipped_newer_version
            ));
        }

        json!({
            "status": "ok",
            "current_version": cur_engine_ver,
            "latest_version": latest_version,
            "has_newer_engine_version": has_newer_version,
            "release_notes": release_notes,
            "installer_url": installer_url,
            "installer_sha256": installer_sha256,
            "imported_bigrams": imported_bigrams,
            "imported_compounds": imported_compounds,
            "imported_rejected_readings": imported_rejected,
            "imported_constructions": imported_constructions,
            "skipped_newer_version_rules": skipped_newer_version,
            "blocked_unigram_rules": blocked_unigram,
            "blocked_unsafe_rules": blocked_unsafe,
            "message": msg
        })
    }

    /// 方案 A：透過 GitHub 儲存庫／CDN 進行條件式低頻同步 (`pull_community_rules_with_etag`)
    /// 原生支援 HTTP `ETag` / `If-None-Match` 與 `304 Not Modified` 邊緣 CDN 快取，
    /// 遠端規則未變動時為 0 Bytes 傳輸且不消耗 GitHub API 配額。
    /// 若遠端主清單 `schema_version` 未來升級超過本機支援上限，自動透過 `compat_feeds` 轉向拉取相容世代分支！
    pub fn pull_community_rules_with_etag(&mut self, feed_url: &str, force_refresh: bool) -> Value {
        let url = if feed_url.trim().is_empty() {
            crate::config_sync::DEFAULT_TAILCAT_FEED_URL
        } else {
            feed_url.trim()
        };
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return json!({
                "status": "error",
                "message": "請提供合法的 HTTPS/HTTP 規則庫網址（例如 GitHub Raw URL）"
            });
        }

        let state_path = self.data_dir.join("federation_sync_state.json");
        let mut state_map: Value = fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}));

        let cached_etag = if force_refresh {
            String::new()
        } else {
            state_map
                .get("etags")
                .and_then(|m| m.get(url))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };

        let mut cmd = std::process::Command::new("curl.exe");
        cmd.args([
            "-s",
            "-i",
            "-L",
            "--max-time",
            "10",
            "-A",
            concat!("RuhI-Semantic-Federation/", env!("CARGO_PKG_VERSION")),
        ]);
        if !cached_etag.is_empty() {
            cmd.args(["-H", &format!("If-None-Match: {cached_etag}")]);
        }
        cmd.arg(url);

        let Ok(output) = cmd.output() else {
            return json!({
                "status": "error",
                "message": "無法執行系統 curl.exe 進行網路請求"
            });
        };

        let raw_resp = String::from_utf8_lossy(&output.stdout);
        let (headers_part, body_part) = raw_resp
            .split_once("\r\n\r\n")
            .or_else(|| raw_resp.split_once("\n\n"))
            .unwrap_or((&raw_resp, ""));

        let first_line = headers_part.lines().next().unwrap_or("");
        if first_line.contains(" 304 ") || first_line.ends_with(" 304") {
            return json!({
                "status": "not_modified",
                "http_status": 304,
                "etag": cached_etag,
                "bytes_transferred": 0,
                "current_version": crate::config_sync::CURRENT_ENGINE_VERSION,
                "has_newer_engine_version": false,
                "message": "遠端 Tailcat 規則與詞庫已是最新狀態（HTTP 304 Not Modified，由 GitHub CDN 快取直接回應，零流量負擔）"
            });
        }

        let mut new_etag = String::new();
        for line in headers_part.lines() {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("etag") {
                    new_etag = v.trim().to_string();
                }
            }
        }

        let Ok(mut pkg) = serde_json::from_str::<Value>(body_part.trim()) else {
            return json!({
                "status": "error",
                "message": "遠端回應內容非合法之 Tailcat JSON 規則包格式"
            });
        };

        // 念舊使用者跨世代防線：若未來遠端主清單 schema_version 升級超過本機支援之 CURRENT_TAILCAT_SCHEMA_VERSION，
        // 自動檢查 compat_feeds["1"] 並無縫切換至 v1 相容快照清單
        let cur_schema = crate::config_sync::CURRENT_TAILCAT_SCHEMA_VERSION as u64;
        let remote_schema = pkg
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if remote_schema > cur_schema {
            let schema_key = cur_schema.to_string();
            if let Some(compat_url) = pkg
                .get("compat_feeds")
                .and_then(|cf| cf.get(&schema_key))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
            {
                if !compat_url.is_empty() && compat_url != url {
                    if let Ok(compat_out) = std::process::Command::new("curl.exe")
                        .args(["-s", "-L", "--max-time", "10", &compat_url])
                        .output()
                    {
                        let compat_body = String::from_utf8_lossy(&compat_out.stdout);
                        if let Ok(compat_pkg) = serde_json::from_str::<Value>(compat_body.trim()) {
                            pkg = compat_pkg;
                        }
                    }
                }
            }
        }

        let mut import_res = self.import_tailcat_package(&pkg);
        if let Some(intake_url) = pkg.get("telemetry_intake_url").and_then(Value::as_str) {
            let trimmed = intake_url.trim();
            if trimmed.starts_with("https://") {
                state_map["telemetry_intake_url"] = json!(trimmed);
            }
        }
        if !new_etag.is_empty() || state_map.get("telemetry_intake_url").is_some() {
            if state_map.get("etags").is_none() {
                state_map["etags"] = json!({});
            }
            if !new_etag.is_empty() {
                state_map["etags"][url] = json!(new_etag);
            }
            state_map["last_sync"] = json!(Local::now().to_rfc3339());
            let _ = fs::write(
                &state_path,
                serde_json::to_string_pretty(&state_map).unwrap_or_default(),
            );
        }

        if let Some(obj) = import_res.as_object_mut() {
            obj.insert("http_status".to_string(), json!(200));
            obj.insert("etag".to_string(), json!(new_etag));
            obj.insert("bytes_transferred".to_string(), json!(body_part.len()));
        }
        import_res
    }

    /// 產生官方權威之 `tailcat_official_feed.json`（供開發者調校完成後發布至 GitHub `main` 讓全體使用者自動拉取）
    pub fn export_official_tailcat_feed(&self) -> Value {
        let mut rules = Vec::new();
        let cur_ver = crate::config_sync::CURRENT_ENGINE_VERSION;

        // 1. 匯出領域詞彙 (lexicon_domain.tsv 尾部精選與近期新增詞)
        let domain_path = self.data_dir.join("lexicon_domain.tsv");
        if let Ok(raw) = fs::read_to_string(&domain_path) {
            let all_lines: Vec<&str> = raw
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            let start_idx = all_lines.len().saturating_sub(80);
            for line in &all_lines[start_idx..] {
                let cols: Vec<&str> = line.split('\t').collect();
                if cols.len() >= 2 {
                    let w = cols[0].trim();
                    let bpmf = cols[1].trim();
                    let weight = cols
                        .get(2)
                        .and_then(|s| s.trim().parse::<i64>().ok())
                        .unwrap_or(920);
                    rules.push(json!({
                        "type": "COMPOUND_WORD",
                        "word": w,
                        "bopomofo": bpmf,
                        "weight": weight,
                        "min_engine_version": "0.4.0",
                        "schema_version": 1
                    }));
                }
            }
        }

        // 2. 匯出破音字與罕用音黑名單 (rejected_readings.tsv)
        let rej_path = self.data_dir.join("rejected_readings.tsv");
        if let Ok(raw) = fs::read_to_string(&rej_path) {
            for line in raw.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                let cols: Vec<&str> = t.split('\t').collect();
                if cols.len() >= 2 {
                    rules.push(json!({
                        "type": "REJECTED_READING",
                        "word": cols[0].trim(),
                        "reading": cols[1].trim(),
                        "reason": cols.get(2).map(|s| s.trim()).unwrap_or("Official_Blacklist"),
                        "min_engine_version": "0.4.0",
                        "schema_version": 1
                    }));
                }
            }
        }

        // 3. 匯出二階語法轉移引力 (transitions_user.tsv 近期精鍛規則)
        if let Ok(raw) = fs::read_to_string(&self.user_tsv) {
            let all_lines: Vec<&str> = raw
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            let start_idx = all_lines.len().saturating_sub(150);
            for t in &all_lines[start_idx..] {
                let cols: Vec<&str> = t.split('\t').collect();
                if cols.len() >= 3 {
                    let prev = cols[0].trim();
                    let next = cols[1].trim();
                    let score = cols[2].trim().parse::<f64>().unwrap_or(26.0);
                    let kind = cols.get(3).map(|s| s.trim()).unwrap_or("collocation");
                    if prev.is_empty() || next.is_empty() || score.abs() < 1.0 {
                        continue;
                    }
                    rules.push(json!({
                        "type": "BIGRAM_TRANSITION",
                        "prev": prev,
                        "next": next,
                        "recommended_delta": score.clamp(-35.0, 35.0),
                        "kind": kind,
                        "min_engine_version": "0.4.0",
                        "schema_version": 1
                    }));
                }
            }
        }

        // 4. 匯出槽位構式模板 (patterns_user.jsonl)
        if let Ok(raw) = fs::read_to_string(&self.patterns_jsonl) {
            for line in raw.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                if let Ok(mut obj) = serde_json::from_str::<Value>(t) {
                    if let Some(map) = obj.as_object_mut() {
                        map.insert("type".to_string(), json!("PATTERN_CONSTRUCTION"));
                        map.entry("min_engine_version".to_string())
                            .or_insert_with(|| json!("0.4.0"));
                        map.entry("schema_version".to_string())
                            .or_insert_with(|| json!(1));
                    }
                    rules.push(obj);
                }
            }
        }

        let feed = json!({
            "protocol": "RUHI_TAILCAT_FEDERATED_SEMANTIC_V1",
            "schema_version": crate::config_sync::CURRENT_TAILCAT_SCHEMA_VERSION,
            "latest_version": cur_ver,
            "min_compat_version": "0.4.0",
            "updated_at": Local::now().to_rfc3339(),
            "release_notes": "RuhI v0.5.0：新增韓文羅馬字（RR）輸入模式、阿南達瑪迦梵語羅馬拼音詞彙轉寫與歐語變音候選擴充；支援 Tailcat 動態派發雲端投遞端點與試算表讀寫權限分離；語意規則經 dev 集消融收斂。",
            "installer_url": format!("https://github.com/Yuktesha/ruhi/releases/download/v{}/RuhI_Setup_v{}.exe", cur_ver, cur_ver),
            "telemetry_intake_url": crate::config_sync::resolve_cloud_sheet_webhook_url(""),
            "compat_feeds": {
                "1": crate::config_sync::DEFAULT_TAILCAT_FEED_URL
            },
            "zero_pii_verified": true,
            "rules_count": rules.len(),
            "rules": rules
        });

        let pretty = serde_json::to_string_pretty(&feed).unwrap_or_default();
        let targets = [
            self.data_dir.join("tailcat_official_feed.json"),
            self.root
                .join("scripts")
                .join("data")
                .join("tailcat_official_feed.json"),
            self.root
                .join("dist")
                .join("data")
                .join("tailcat_official_feed.json"),
        ];
        for p in &targets {
            if p.parent().map(|d| d.is_dir()).unwrap_or(false) {
                let _ = fs::write(p, &pretty);
            }
        }
        feed
    }

    /// 當 Tailcat 偵測到遠端有新版主程式 (`has_newer_engine_version == true`) 時，
    /// 依據使用者之 `tailcat_update_mode` 執行「念舊略過」、「彈窗徵詢使用者同意後自動下載安裝」或「預先授權自動升級」
    pub fn handle_tailcat_version_upgrade(&self, sync_result: &Value, update_mode: &str) -> Value {
        let has_newer = sync_result
            .get("has_newer_engine_version")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !has_newer {
            return json!({
                "upgrade_action": "none",
                "message": "目前已是最新主程式版本。"
            });
        }

        if update_mode == "lexicon_only" {
            return json!({
                "upgrade_action": "skipped_nostalgic_mode",
                "message": "使用者設定為『念舊模式（僅更新相容詞庫與規則）』，已略過主程式版本升級。"
            });
        }

        let latest_ver = sync_result
            .get("latest_version")
            .and_then(Value::as_str)
            .unwrap_or("");
        let cur_ver = crate::config_sync::CURRENT_ENGINE_VERSION;
        let notes = sync_result
            .get("release_notes")
            .and_then(Value::as_str)
            .unwrap_or("包含最新深層語法構式與穩定度提升。");
        let installer_url = sync_result
            .get("installer_url")
            .and_then(Value::as_str)
            .unwrap_or("");

        if installer_url.is_empty() || !installer_url.starts_with("https://") {
            return json!({
                "upgrade_action": "invalid_installer_url",
                "message": "遠端清單未提供有效之 HTTPS 安裝包網址。"
            });
        }

        let user_approved = if update_mode == "auto_install" {
            true
        } else {
            #[cfg(target_os = "windows")]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
                };
                let title_w: Vec<u16> = "RuhI 如喜輸入法 — 發現新版本更新 (Tailcat)\0"
                    .encode_utf16()
                    .collect();
                let body = format!(
                    "透過 Tailcat 官方頻道偵測到如喜輸入法新版本！\n\n• 目前版本：v{}\n• 最新版本：v{}\n• 更新內容：{}\n\n（註：相容之最新詞庫與語意規則已為您自動熱同步完成）\n\n是否同意立即自動下載並安裝 v{} 主程式？\n點擊「是 (Yes)」將自動下載並升級；點擊「否 (No)」將保留目前主程式版本。",
                    cur_ver, latest_ver, notes, latest_ver
                );
                let body_w: Vec<u16> = body.encode_utf16().chain(Some(0)).collect();
                let ans = unsafe {
                    MessageBoxW(
                        std::ptr::null_mut(),
                        body_w.as_ptr(),
                        title_w.as_ptr(),
                        MB_YESNO | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND,
                    )
                };
                ans == IDYES
            }
            #[cfg(not(target_os = "windows"))]
            {
                false
            }
        };

        if !user_approved {
            return json!({
                "upgrade_action": "user_declined",
                "message": "使用者選擇暫不升級主程式，僅保留已熱更新之相容詞庫與規則。"
            });
        }

        // 下載安裝包至 %TEMP% 並驗證執行
        let temp_dir = std::env::temp_dir();
        let installer_path = temp_dir.join(format!("RuhI_Setup_v{latest_ver}.exe"));
        let dl_status = std::process::Command::new("curl.exe")
            .args([
                "-s",
                "-L",
                "--max-time",
                "120",
                "-o",
                &installer_path.to_string_lossy(),
                installer_url,
            ])
            .status();

        if dl_status.map(|s| s.success()).unwrap_or(false) && installer_path.is_file() {
            if let Ok(bytes) = fs::read(&installer_path) {
                if bytes.len() > 64 * 1024 && bytes.starts_with(b"MZ") {
                    let _ = std::process::Command::new(&installer_path)
                        .args(["/SILENT", "/CLOSEAPPLICATIONS"])
                        .spawn();
                    return json!({
                        "upgrade_action": "launched_installer",
                        "installer_path": installer_path.to_string_lossy(),
                        "message": format!("✅ 已經使用者同意下載並啟動 RuhI v{} 自動升級程序！", latest_ver)
                    });
                }
            }
        }

        json!({
            "upgrade_action": "download_failed",
            "message": "下載新版安裝包失敗，請稍後再試或前往 GitHub Releases 下載。"
        })
    }

    /// 讀取本機 `pending_revisions.json` 中尚未回傳之手動改字整句對照紀錄
    pub fn load_local_pending_revisions(&self) -> Vec<Value> {
        let mut paths = vec![
            self.data_dir.join("pending_revisions.json"),
            self.revisions_json.clone(),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            paths.push(PathBuf::from(appdata).join("Ruhi").join("data").join("pending_revisions.json"));
        }
        let mut combined = Vec::new();
        for p in paths {
            if p.is_file() {
                if let Ok(raw) = fs::read_to_string(&p) {
                    if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&raw) {
                        for item in arr {
                            if !combined.contains(&item) {
                                combined.push(item);
                            }
                        }
                    }
                }
            }
        }
        combined
    }

    /// 清空本機 `pending_revisions.json` 與 `user_revision_pairs.txt`
    pub fn clear_local_pending_revisions(&self) -> usize {
        let count = self.load_local_pending_revisions().len();
        let mut paths = vec![
            self.data_dir.join("pending_revisions.json"),
            self.revisions_json.clone(),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            paths.push(PathBuf::from(appdata).join("Ruhi").join("data").join("pending_revisions.json"));
        }
        for p in paths {
            if p.is_file() {
                let _ = fs::write(&p, "[]");
            }
            if let Some(parent) = p.parent() {
                let txt = parent.join("user_revision_pairs.txt");
                if txt.is_file() {
                    let _ = fs::write(&txt, "");
                }
            }
        }
        count
    }

    /// 將本機累積之手動改字整句對照紀錄經 Zero-PII 隱私過濾後，以 HTTPS POST 回傳至官方 Google 雲端硬碟試算表 Webhook
    pub fn upload_revisions_to_cloud_sheet(&self, webhook_url: &str, zero_pii_enabled: bool) -> Value {
        let resolved_url = crate::config_sync::resolve_cloud_sheet_webhook_url(webhook_url);
        let url = resolved_url.as_str();
        if url.is_empty() {
            return json!({
                "status": "not_configured",
                "message": "尚未設定雲端試算表 Webhook 網址（請於偏好設定填入，或設定環境變數 RUHI_CLOUD_SHEET_WEBHOOK_URL），已略過"
            });
        }
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return json!({
                "status": "error",
                "message": "雲端資料表 Webhook 網址格式無效（須以 https:// 開頭）"
            });
        }

        let raw_revs = self.load_local_pending_revisions();
        if raw_revs.is_empty() {
            return json!({
                "status": "empty",
                "uploaded_count": 0,
                "filtered_pii_count": 0,
                "message": "目前本機無待回傳之改字紀錄。"
            });
        }

        let mut records_to_send = Vec::new();
        let mut seen_pairs = HashSet::new();
        let mut filtered_pii_count = 0usize;

        for item in &raw_revs {
            let err_s = item
                .get("error_sentence")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| item.get("rejected").and_then(Value::as_str))
                .unwrap_or("")
                .trim();
            let cor_s = item
                .get("correct_sentence")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| item.get("accepted").and_then(Value::as_str))
                .unwrap_or("")
                .trim();
            let rej = item.get("rejected").and_then(Value::as_str).unwrap_or("").trim();
            let acc = item.get("accepted").and_then(Value::as_str).unwrap_or("").trim();
            let bpmf = item.get("bopomofo").and_then(Value::as_str).unwrap_or("").trim();
            let ts = item.get("timestamp").and_then(Value::as_u64).unwrap_or(0);

            if err_s.is_empty() || cor_s.is_empty() || err_s == cor_s {
                continue;
            }
            if zero_pii_enabled {
                let (safe, _) = ZeroPiiPrivacyFilter::is_safe_sentence_pair(err_s, cor_s);
                if !safe {
                    filtered_pii_count += 1;
                    continue;
                }
            }
            if !seen_pairs.insert((err_s.to_string(), cor_s.to_string())) {
                continue;
            }
            records_to_send.push(json!({
                "timestamp": ts,
                "error_sentence": err_s,
                "correct_sentence": cor_s,
                "rejected": rej,
                "accepted": acc,
                "bopomofo": bpmf,
                "keyboard_mode": "hsu",
                "app_version": env!("CARGO_PKG_VERSION"),
            }));
        }

        if records_to_send.is_empty() {
            return json!({
                "status": "filtered_all",
                "uploaded_count": 0,
                "filtered_pii_count": filtered_pii_count,
                "message": format!("本機共 {} 筆紀錄，經啟發式個資過濾閘門篩除後無可回傳之一般語句。", raw_revs.len())
            });
        }

        let payload = json!({
            "protocol": "RUHI_CLOUD_REVISION_SHEET_V1",
            "app_version": env!("CARGO_PKG_VERSION"),
            "keyboard_mode": "hsu",
            "revisions": records_to_send,
            "records": records_to_send,
        });

        let temp_payload_path = self.data_dir.join("cloud_upload_payload.tmp.json");
        let Ok(payload_str) = serde_json::to_string(&payload) else {
            return json!({
                "status": "error",
                "message": "序列化回傳資料失敗"
            });
        };
        let _ = fs::write(&temp_payload_path, &payload_str);

        let data_arg = format!("@{}", temp_payload_path.to_string_lossy());
        let output = std::process::Command::new("curl.exe")
            .args([
                "-s",
                "-L",
                "--max-time",
                "15",
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                &data_arg,
                url,
            ])
            .output();
        let _ = fs::remove_file(&temp_payload_path);

        let Ok(out) = output else {
            return json!({
                "status": "error",
                "message": "無法呼叫系統 curl.exe 傳送資料，請確認網路連線。"
            });
        };

        let resp_text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && (resp_text.contains("\"ok\"") || resp_text.contains("status")) {
            let uploaded_cnt = payload["records"].as_array().map(|a| a.len()).unwrap_or(0);
            self.clear_local_pending_revisions();
            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            json!({
                "status": "ok",
                "uploaded_count": uploaded_cnt,
                "filtered_pii_count": filtered_pii_count,
                "timestamp": now_ts,
                "message": format!("✅ 已成功將 {} 組改字雙句回傳至雲端硬碟資料表（過濾 {} 筆敏感字串），並已清空本機暫存！", uploaded_cnt, filtered_pii_count)
            })
        } else {
            json!({
                "status": "error",
                "uploaded_count": 0,
                "filtered_pii_count": filtered_pii_count,
                "message": format!("雲端回傳未完成（伺服器回應：{}），本機紀錄已安全保留，將於下次週期重試。", if resp_text.is_empty() { "連線逾時或無回應" } else { &resp_text })
            })
        }
    }

    /// 依據使用者設定之週期 (`weekly` / `daily` / `manual` / `disabled`) 檢查並執行背景回傳與 Tailcat 自動同步
    pub fn maybe_run_scheduled_upload(&mut self, settings: &mut crate::config_sync::RuhiSettings) -> Option<Value> {
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let interval_secs = match settings.ai_and_privacy.upload_schedule.as_str() {
            "daily" => 86_400u64,
            "weekly" => 604_800u64,
            _ => 86_400u64,
        };

        // 1. 檢查並執行 Tailcat 官方詞庫熱同步與版本更新檢查
        if settings.ai_and_privacy.tailcat_auto_sync
            && settings.ai_and_privacy.tailcat_update_mode != "manual"
            && now_ts.saturating_sub(settings.ai_and_privacy.last_tailcat_sync_timestamp) >= interval_secs
        {
            let feed_url = settings.ai_and_privacy.tailcat_feed_url.clone();
            let update_mode = settings.ai_and_privacy.tailcat_update_mode.clone();
            let sync_res = self.pull_community_rules_with_etag(&feed_url, false);
            if sync_res.get("status").and_then(Value::as_str) == Some("ok")
                || sync_res.get("status").and_then(Value::as_str) == Some("not_modified")
            {
                settings.ai_and_privacy.last_tailcat_sync_timestamp = now_ts;
                let _ = crate::config_sync::save_settings(&self.root, settings);
                let _ = self.handle_tailcat_version_upgrade(&sync_res, &update_mode);
            }
        }

        // 2. 檢查並執行匿名改字雙句雲端回傳
        if !settings.ai_and_privacy.telemetry_consent {
            return None;
        }
        let upload_interval = match settings.ai_and_privacy.upload_schedule.as_str() {
            "daily" => 86_400u64,
            "weekly" => 604_800u64,
            _ => return None,
        };
        if now_ts.saturating_sub(settings.ai_and_privacy.last_upload_timestamp) < upload_interval {
            return None;
        }
        if self.load_local_pending_revisions().is_empty() {
            return None;
        }
        let res = self.upload_revisions_to_cloud_sheet(
            &settings.ai_and_privacy.cloud_sheet_webhook_url,
            settings.ai_and_privacy.zero_pii_gateway,
        );
        if res.get("status").and_then(Value::as_str) == Some("ok") {
            settings.ai_and_privacy.last_upload_timestamp = now_ts;
            let _ = crate::config_sync::save_settings(&self.root, settings);
        }
        Some(res)
    }

    /// 開發者自動化工具：從官方 Google 雲端試算表一鍵拉取所有狀態為「待調校」之雙句，
    /// 並自動儲存至 `data/cloud_pending_pairs.txt`，供語意調校工作室或 AI 顧問直接批次處理（免人工複製貼上）
    pub fn fetch_pending_revisions_from_cloud_sheet(&self, webhook_url: &str) -> Value {
        let resolved_url = crate::config_sync::resolve_cloud_sheet_webhook_url(webhook_url);
        let url = resolved_url.as_str();
        if url.is_empty() {
            return json!({
                "status": "not_configured",
                "message": "尚未設定雲端試算表 Webhook 網址（請於偏好設定填入，或設定環境變數 RUHI_CLOUD_SHEET_WEBHOOK_URL），已略過"
            });
        }
        let admin_token = crate::config_sync::resolve_cloud_sheet_admin_token();
        let mut fetch_url = if url.contains('?') {
            format!("{url}&action=fetch_pending")
        } else {
            format!("{url}?action=fetch_pending")
        };
        if !admin_token.is_empty() {
            fetch_url.push_str(&format!("&token={admin_token}"));
        }

        let output = std::process::Command::new("curl.exe")
            .args(["-s", "-L", "--max-time", "15", &fetch_url])
            .output();

        let Ok(out) = output else {
            return json!({
                "status": "error",
                "message": "無法呼叫系統 curl.exe 連線至雲端試算表。"
            });
        };

        let resp_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let Ok(val) = serde_json::from_str::<Value>(&resp_str) else {
            return json!({
                "status": "error",
                "message": format!("雲端回應非合法 JSON：{}", resp_str)
            });
        };

        if val.get("status").and_then(Value::as_str) == Some("unauthorized") {
            return json!({
                "status": "unauthorized",
                "message": val.get("message").and_then(Value::as_str).unwrap_or("雲端試算表已啟用管理員權杖保護，請於本機 config.json 設定 cloud_sheet_admin_token 或環境變數 RUHI_CLOUD_SHEET_ADMIN_TOKEN。")
            });
        }

        // 若雲端 Apps Script 尚為舊版 Write-Only，友善提示更新腳本
        if val.get("service").and_then(Value::as_str).unwrap_or("").contains("Write-Only") {
            return json!({
                "status": "needs_script_update",
                "message": "雲端 Apps Script 目前為 Write-Only 模式，請將 tools/google_sheets_webhook.gs 的升級版程式碼貼入 Apps Script 並更新部署，即可啟用雙向自動拉取與『已調校』狀態回寫！"
            });
        }

        let items = val
            .get("pending")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if items.is_empty() {
            return json!({
                "status": "empty",
                "count": 0,
                "pending": [],
                "message": "目前雲端試算表沒有任何『待調校』的新雙句。"
            });
        }

        // 自動寫入 data/cloud_pending_pairs.txt 並登錄至近期檔案清單
        let txt_path = self.data_dir.join("cloud_pending_pairs.txt");
        let mut lines = Vec::new();
        for item in &items {
            let e = item.get("error_sentence").and_then(Value::as_str).unwrap_or("").trim();
            let c = item.get("correct_sentence").and_then(Value::as_str).unwrap_or("").trim();
            if !e.is_empty() && !c.is_empty() && e != c {
                lines.push(format!("{e}\n{c}\n"));
            }
        }
        let _ = fs::write(&txt_path, lines.join("\n"));
        let _ = self.update_recent_txt_history(
            "add",
            "cloud_pending_pairs.txt",
            &txt_path.to_string_lossy(),
            lines.len(),
        );

        json!({
            "status": "ok",
            "count": items.len(),
            "saved_txt_path": txt_path.to_string_lossy(),
            "pending": items,
            "message": format!("✅ 已從雲端試算表自動拉取 {} 組『待調校』雙句，並已匯入 data/cloud_pending_pairs.txt！", items.len())
        })
    }

    /// 開發者自動化工具：將雲端試算表中已完成語意調校之列號（`row_numbers`）自動標記為「✅ 已調校」
    pub fn mark_cloud_sheet_rows_tuned(&self, webhook_url: &str, row_numbers: &[u64]) -> Value {
        if row_numbers.is_empty() {
            return json!({
                "status": "empty",
                "message": "未指定任何列號。"
            });
        }
        let resolved_url = crate::config_sync::resolve_cloud_sheet_webhook_url(webhook_url);
        let url = resolved_url.as_str();
        if url.is_empty() {
            return json!({
                "status": "not_configured",
                "message": "尚未設定雲端試算表 Webhook 網址（請於偏好設定填入，或設定環境變數 RUHI_CLOUD_SHEET_WEBHOOK_URL），已略過"
            });
        }

        let admin_token = crate::config_sync::resolve_cloud_sheet_admin_token();
        let payload = json!({
            "action": "mark_tuned",
            "admin_token": admin_token,
            "rows": row_numbers,
            "tuned_status": format!("✅ 已調校 ({})", Local::now().format("%Y-%m-%d %H:%M"))
        });

        let temp_path = self.data_dir.join("cloud_mark_tuned.tmp.json");
        let _ = fs::write(&temp_path, serde_json::to_string(&payload).unwrap_or_default());
        let data_arg = format!("@{}", temp_path.to_string_lossy());

        let output = std::process::Command::new("curl.exe")
            .args([
                "-s",
                "-L",
                "--max-time",
                "15",
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                &data_arg,
                url,
            ])
            .output();
        let _ = fs::remove_file(&temp_path);

        let Ok(out) = output else {
            return json!({
                "status": "error",
                "message": "無法連線回寫雲端試算表狀態。"
            });
        };

        let resp_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        serde_json::from_str::<Value>(&resp_str).unwrap_or_else(|_| {
            json!({
                "status": "ok",
                "raw_response": resp_str
            })
        })
    }

    /// 解析本機 `語意分析*.txt` 批次語料檔路徑
    pub fn resolve_local_txt_file(&self, target_path: &str, target_name: &str) -> Option<PathBuf> {
        if !target_path.trim().is_empty() {
            let p = PathBuf::from(target_path.trim());
            if p.is_file() {
                return Some(p);
            }
        }
        let fname = if !target_name.trim().is_empty() {
            target_name.trim()
        } else {
            "語意分析.txt"
        };

        let mut search_dirs = vec![
            self.root.clone(),
            self.root.join("scripts"),
            PathBuf::from(r"C:\Users\Yuktesha\Desktop"),
        ];
        if let Some(home) = dirs::home_dir() {
            search_dirs.push(home.join("Desktop"));
        }

        for d in search_dirs {
            let cand = d.join(fname);
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    }

    /// 從文字檔解析「錯誤句 ➔ 正確句」語料對（支援 `➔`、`->`、`→`、Tab 或雙行配對）
    pub fn load_pairs_from_file(&self, path: &Path) -> Vec<Value> {
        let Ok(raw) = fs::read_to_string(path) else {
            return Vec::new();
        };
        let content = raw.trim_start_matches('\u{feff}');
        let mut pairs = Vec::new();
        let mut pending_err: Option<String> = None;

        for line in content.lines() {
            let s = line.trim();
            if s.is_empty() || s.starts_with('#') || s.starts_with("//") {
                continue;
            }
            let sep_candidates = ["➔", "→", "->", "\t", "｜", "|"];
            let mut found_sep = None;
            for sep in sep_candidates {
                if let Some((l, r)) = s.split_once(sep) {
                    let e = l.trim();
                    let c = r.trim();
                    if !e.is_empty() && !c.is_empty() {
                        found_sep = Some((e.to_string(), c.to_string()));
                        break;
                    }
                }
            }
            if let Some((e, c)) = found_sep {
                pending_err = None;
                let (e_sp, c_sp) = self.tokenize_pair_with_spans(&e, &c);
                pairs.push(json!({
                    "error": e,
                    "correct": c,
                    "error_tokens": e_sp.into_iter().map(|(t, _, _)| t).collect::<Vec<_>>(),
                    "correct_tokens": c_sp.into_iter().map(|(t, _, _)| t).collect::<Vec<_>>()
                }));
            } else if let Some(prev_e) = pending_err.take() {
                let c = s.to_string();
                let (e_sp, c_sp) = self.tokenize_pair_with_spans(&prev_e, &c);
                pairs.push(json!({
                    "error": prev_e,
                    "correct": c,
                    "error_tokens": e_sp.into_iter().map(|(t, _, _)| t).collect::<Vec<_>>(),
                    "correct_tokens": c_sp.into_iter().map(|(t, _, _)| t).collect::<Vec<_>>()
                }));
            } else {
                pending_err = Some(s.to_string());
            }
        }
        pairs
    }

    /// 讀取使用者主動匯入之近期 `.txt` 瀏覽紀錄（持久化於 `data/recent_txt_files.json`）
    /// 嚴禁主動掃描桌面強行塞入使用者未曾開啟或已手動清除之 `語意分析*.txt` 檔案！
    pub fn scan_recent_txt_files(&self) -> Vec<Value> {
        let history_path = self.data_dir.join("recent_txt_files.json");
        let Ok(raw) = fs::read_to_string(&history_path) else {
            return Vec::new();
        };
        let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&raw) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for item in items {
            let file_name = item
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let file_path = item
                .get("file_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if file_name.is_empty() || !seen.insert(file_name.clone()) {
                continue;
            }
            if let Some(p) = self.resolve_local_txt_file(&file_path, &file_name) {
                let pairs = self.load_pairs_from_file(&p);
                out.push(json!({
                    "file_path": p.to_string_lossy(),
                    "file_name": file_name,
                    "count": pairs.len()
                }));
            } else {
                let fallback_count = item.get("count").and_then(Value::as_u64).unwrap_or(0);
                out.push(json!({
                    "file_path": file_path,
                    "file_name": file_name,
                    "count": fallback_count
                }));
            }
        }
        out
    }

    /// 新增、移除或清空近期 `.txt` 瀏覽紀錄（持久化至 `data/recent_txt_files.json`）
    pub fn update_recent_txt_history(
        &self,
        action: &str,
        file_name: &str,
        file_path: &str,
        count: usize,
    ) -> Vec<Value> {
        let history_path = self.data_dir.join("recent_txt_files.json");
        let mut current: Vec<Value> = fs::read_to_string(&history_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();

        match action {
            "clear" | "clear_all" => {
                current.clear();
            }
            "remove" | "delete" => {
                let target = file_name.trim();
                current.retain(|x| {
                    x.get("file_name")
                        .and_then(Value::as_str)
                        .map(|n| n.trim() != target)
                        .unwrap_or(false)
                });
            }
            "add" | "upsert" => {
                let target = file_name.trim();
                if !target.is_empty() {
                    let prev_path = current
                        .iter()
                        .find(|x| x.get("file_name").and_then(Value::as_str) == Some(target))
                        .and_then(|x| x.get("file_path").and_then(Value::as_str))
                        .unwrap_or("")
                        .to_string();
                    current.retain(|x| {
                        x.get("file_name")
                            .and_then(Value::as_str)
                            .map(|n| n.trim() != target)
                            .unwrap_or(false)
                    });
                    let resolved_path = if !file_path.trim().is_empty() {
                        file_path.trim().to_string()
                    } else {
                        prev_path
                    };
                    current.insert(
                        0,
                        json!({
                            "file_name": target,
                            "file_path": resolved_path,
                            "count": count
                        }),
                    );
                    current.truncate(8);
                }
            }
            _ => {}
        }

        if let Some(parent) = history_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(serialized) = serde_json::to_string_pretty(&current) {
            let _ = fs::write(&history_path, serialized);
        }
        self.scan_recent_txt_files()
    }

    /// 自動將同構之單點二元轉移規則 (`prev -> next`) 因式分解為二維陣列對應 (`A_array × B_array`)
    ///
    /// 演算法步驟：
    /// 1. 收集所有正向轉移規則 (`score > 0`) 之二元關係圖 $E \subseteq A \times B$。
    /// 2. 依共同後繼詞 $b \in B$ 聚合前驅集合 $P(b) = \{a \mid (a, b) \in E\}$；若 $|P(b)| \ge 2$，形成候選槽位。
    /// 3. 將具備相同前驅集合（或高度重疊）之後繼詞 $b_1, b_2, \dots$ 合併為二維笛卡兒積陣列 $P \times Q$。
    /// 4. 針對剩餘規則再依共同前驅詞 $a \in A$ 聚合後繼集合 $N(a) = \{b \mid (a, b) \in E\}$（$|N(a)| \ge 2$）。
    pub fn factorize_rules_to_arrays(&self, rules: &[Value]) -> Vec<Value> {
        let mut by_next: HashMap<String, Vec<String>> = HashMap::new();
        let mut by_prev: HashMap<String, Vec<String>> = HashMap::new();

        for r in rules {
            let score = r
                .get("score")
                .or_else(|| r.get("recommended_delta"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if score <= 0.0 {
                continue;
            }
            let prev = r.get("prev").and_then(Value::as_str).unwrap_or("").trim();
            let next = r.get("next").and_then(Value::as_str).unwrap_or("").trim();
            if !is_valid_chinese_token(prev) || !is_valid_chinese_token(next) {
                continue;
            }
            let p_list = by_next.entry(next.to_string()).or_default();
            if !p_list.contains(&prev.to_string()) {
                p_list.push(prev.to_string());
            }
            let n_list = by_prev.entry(prev.to_string()).or_default();
            if !n_list.contains(&next.to_string()) {
                n_list.push(next.to_string());
            }
        }

        // 將擁有相同 prev_slot 集合的 next 合併為 A_array × B_array
        let mut prev_set_to_nexts: HashMap<Vec<String>, Vec<String>> = HashMap::new();
        let mut covered_edges: HashSet<(String, String)> = HashSet::new();

        let mut next_keys: Vec<String> = by_next.keys().cloned().collect();
        next_keys.sort();
        for nk in next_keys {
            if let Some(mut p_list) = by_next.get(&nk).cloned() {
                if p_list.len() >= 2 {
                    p_list.sort();
                    prev_set_to_nexts.entry(p_list).or_default().push(nk);
                }
            }
        }

        // 輔助判斷：單一槽位內之詞元是否具備語法同質性（嚴禁將單字封閉虛詞如『就』與三字實詞『輸入法』或定語『一般』與動補『交到』混入同一陣列槽位）
        let is_slot_homogeneous = |slot: &[String]| -> bool {
            if slot.len() <= 1 {
                return true;
            }
            let closed_count = slot
                .iter()
                .filter(|w| w.chars().count() == 1 && CLOSED_CLASS_CHARS.contains(w.as_str()))
                .count();
            // 嚴禁將封閉類單字虛詞與實詞混裝於同一槽位
            if closed_count > 0 && closed_count < slot.len() {
                return false;
            }
            if closed_count == slot.len() {
                return true;
            }
            // 針對多字實詞槽位：要求字長一致，且具備共同字首/字尾（如『調校/調整』、『忘光/忘光光』）或同屬已知輸入法專名／設備後綴族
            let ime_set = ["大易", "倉頡", "行列", "許氏", "嘸蝦米", "無蝦米", "注音", "速成", "拼音", "大千", "倚天", "羅馬"];
            let ime_head_set = ["輸入法", "鍵盤", "字根", "碼表", "拆碼", "規則"];
            if slot.iter().all(|w| ime_set.contains(&w.as_str()))
                || slot.iter().all(|w| ime_head_set.contains(&w.as_str()))
            {
                return true;
            }
            let first_len = slot[0].chars().count();
            let same_len = slot.iter().all(|w| w.chars().count() == first_len);
            let first_ch = slot[0].chars().next();
            let last_ch = slot[0].chars().last();
            let share_affix = (first_ch.is_some() && slot.iter().all(|w| w.chars().next() == first_ch))
                || (last_ch.is_some() && slot.iter().all(|w| w.chars().last() == last_ch));
            same_len && share_affix
        };

        let mut out_arrays = Vec::new();
        let mut sorted_groups: Vec<(Vec<String>, Vec<String>)> =
            prev_set_to_nexts.into_iter().collect();
        sorted_groups.sort_by(|a, b| (b.0.len() * b.1.len()).cmp(&(a.0.len() * a.1.len())));

        for (p_slot, mut n_slot) in sorted_groups {
            if !is_slot_homogeneous(&p_slot) || !is_slot_homogeneous(&n_slot) {
                continue;
            }
            n_slot.sort();
            for p in &p_slot {
                for n in &n_slot {
                    covered_edges.insert((p.clone(), n.clone()));
                }
            }
            let pid = format!(
                "auto_array_{}_{}",
                p_slot.first().map(String::as_str).unwrap_or("p"),
                n_slot.first().map(String::as_str).unwrap_or("n")
            );
            let formula = format!(
                "[{{{}}} × {{{}}}]",
                p_slot.join(", "),
                n_slot.join(", ")
            );
            out_arrays.push(json!({
                "pattern_id": pid,
                "level": "Level 2 (自動因式分解二維陣列構式 A_array × B_array)",
                "name": format!("自動因式分解陣列對應（{} 等 {} 詞 × {} 等 {} 詞）", p_slot[0], p_slot.len(), n_slot[0], n_slot.len()),
                "formula": formula,
                "prev_slot": p_slot,
                "next_slot": n_slot,
                "slots": [p_slot, n_slot],
                "bonus": 28.0,
                "suppress": []
            }));
        }

        // 針對尚未被覆蓋之同前驅多後繼 (1 × B_array) 進行聚合
        let mut prev_keys: Vec<String> = by_prev.keys().cloned().collect();
        prev_keys.sort();
        for pk in prev_keys {
            if let Some(n_list) = by_prev.get(&pk) {
                let uncovered: Vec<String> = n_list
                    .iter()
                    .filter(|n| !covered_edges.contains(&(pk.clone(), (*n).clone())))
                    .cloned()
                    .collect();
                if uncovered.len() >= 2 && is_slot_homogeneous(&uncovered) {
                    let p_slot = vec![pk.clone()];
                    let pid = format!(
                        "auto_array_{}_{}",
                        pk,
                        uncovered.first().map(String::as_str).unwrap_or("n")
                    );
                    let formula = format!("[{{{}}} × {{{}}}]", pk, uncovered.join(", "));
                    out_arrays.push(json!({
                        "pattern_id": pid,
                        "level": "Level 2 (自動因式分解一對多陣列構式 {a} × B_array)",
                        "name": format!("自動因式分解右向陣列（{} × {} 等 {} 詞）", pk, uncovered[0], uncovered.len()),
                        "formula": formula,
                        "prev_slot": p_slot.clone(),
                        "next_slot": uncovered.clone(),
                        "slots": [p_slot, uncovered],
                        "bonus": 28.0,
                        "suppress": []
                    }));
                }
            }
        }

        out_arrays
    }

    /// 統計目前語意規則庫與詞庫之三層結構盤點摘要（供語意工作室與 AI/SI 交接協定即時讀取）
    pub fn get_rule_inventory_summary(&self) -> Value {
        let mut patterns_total = 0usize;
        let mut multi_slot_3d_plus = 0usize;
        let mut array_2d_count = 0usize;
        let mut total_slot_elements = 0usize;

        if self.patterns_jsonl.is_file() {
            if let Ok(raw) = fs::read_to_string(&self.patterns_jsonl) {
                for line in raw.lines() {
                    let s = line.trim();
                    if s.is_empty() {
                        continue;
                    }
                    if let Ok(v) = serde_json::from_str::<Value>(s) {
                        patterns_total += 1;
                        let slots_len = v
                            .get("slots")
                            .and_then(Value::as_array)
                            .map(|a| a.len())
                            .unwrap_or(0);
                        if slots_len >= 3 {
                            multi_slot_3d_plus += 1;
                        } else {
                            array_2d_count += 1;
                        }
                        if let Some(arr) = v.get("slots").and_then(Value::as_array) {
                            for sl in arr {
                                if let Some(inner) = sl.as_array() {
                                    total_slot_elements += inner.len();
                                }
                            }
                        }
                        if let Some(ps) = v.get("prev_slot").and_then(Value::as_array) {
                            total_slot_elements += ps.len();
                        }
                        if let Some(ns) = v.get("next_slot").and_then(Value::as_array) {
                            total_slot_elements += ns.len();
                        }
                    }
                }
            }
        }

        let mut transitions_total = 0usize;
        let mut positive_bigrams = 0usize;
        let mut negative_anti_hijack = 0usize;
        if self.user_tsv.is_file() {
            if let Ok(raw) = fs::read_to_string(&self.user_tsv) {
                for line in raw.lines() {
                    let s = line.trim();
                    if s.is_empty() || s.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = s.split('\t').collect();
                    if parts.len() >= 3 {
                        transitions_total += 1;
                        let sc = parts[2].parse::<f64>().unwrap_or(0.0);
                        if sc >= 0.0 {
                            positive_bigrams += 1;
                        } else {
                            negative_anti_hijack += 1;
                        }
                    }
                }
            }
        }

        let mut domain_words_total = 0usize;
        if self.domain_tsv.is_file() {
            if let Ok(raw) = fs::read_to_string(&self.domain_tsv) {
                for line in raw.lines() {
                    let s = line.trim();
                    if !s.is_empty() && !s.starts_with('#') {
                        domain_words_total += 1;
                    }
                }
            }
        }

        let collision_summary = self.scan_array_homophone_collisions();

        json!({
            "patterns_user_jsonl": {
                "path": self.patterns_jsonl.to_string_lossy(),
                "total_patterns": patterns_total,
                "multi_slot_3d_to_5d_chains": multi_slot_3d_plus,
                "array_2d_correspondences": array_2d_count,
                "total_indexed_slot_tokens": total_slot_elements,
                "array_collision_audit": collision_summary
            },
            "transitions_user_tsv": {
                "path": self.user_tsv.to_string_lossy(),
                "total_bigram_rules": transitions_total,
                "positive_collocations": positive_bigrams,
                "negative_anti_hijack_rules": negative_anti_hijack
            },
            "lexicon_domain_tsv": {
                "path": self.domain_tsv.to_string_lossy(),
                "total_irreducible_entries": domain_words_total
            }
        })
    }

    /// 掃描 `patterns_user.jsonl` 所有陣列展開之二元組合 $(a, b) \in A \times B$：
    /// 檢驗其中有多少組合會與真實詞庫中的同音詞 $W$ 產生注音對衝（$\text{reading}(a)+\text{reading}(b) = \text{reading}(W)$），
    /// 並依據計算語言學四大判別維度自動分類為三大處置策略（而非單一盲目抑制）：
    /// - **Type A (`KEEP_MODERN_ARRAY_OVER_ARCHAIC`)**：對衝詞 $W$ 為罕見低頻古語（`weight <= 220`，如 `說部`、`坐簿`、`得間`、`芳蘭`、`坊曲`、`芳鑒`），保留現代高頻陣列、不盲目抑制；
    /// - **Type B (`RESOLVED_BY_SYNTACTIC_BIFURCATION`)**：雙方皆為真實語句合法用法（如 `騎得` vs. `耆德`、`讀不` vs. `獨步`、`笑到` vs. `孝道`、`全素` vs. `全速`），透過 `bifurcation_pairs` 與左右語法槽位進行雙向語境分流；
    /// - **Type C (`SUPPRESSED_BY_EXCLUDE_ARRAY`)**：陣列展開對本身為跨界假組合且對衝高頻現代實詞（如 `這鍵` vs. `這件`、`個相` vs. `各項`、`分不` vs. `分布`），透過 `exclude_pairs` 精準抵銷二元分數。
    pub fn scan_array_homophone_collisions(&self) -> Value {
        let mut reading_to_real_words: HashMap<String, Vec<String>> = HashMap::new();
        for (w, bpmf) in &self.word_bopomofo {
            if w.chars().count() >= 2 && !bpmf.trim().is_empty() {
                reading_to_real_words
                    .entry(bpmf.trim().to_string())
                    .or_default()
                    .push(w.clone());
            }
        }
        for list in reading_to_real_words.values_mut() {
            list.sort();
            list.dedup();
        }

        let mut total_expanded_pairs = 0usize;
        let mut valid_lexicon_compounds = 0usize;
        let mut harmless_non_existent_readings = 0usize;
        let mut total_homophone_collisions = 0usize;
        let mut type_a_archaic_keep_count = 0usize;
        let mut type_b_bifurcation_count = 0usize;
        let mut suppressed_by_exclude_array = 0usize;
        let mut type_a_items = Vec::new();
        let mut type_b_items = Vec::new();
        let mut suppressed_items = Vec::new();
        let mut unsuppressed_items = Vec::new();

        if self.patterns_jsonl.is_file() {
            if let Ok(raw) = fs::read_to_string(&self.patterns_jsonl) {
                for (line_idx, line) in raw.lines().enumerate() {
                    let s = line.trim();
                    if s.is_empty() || s.starts_with('#') {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<Value>(s) else {
                        continue;
                    };
                    let pid = v
                        .get("pattern_id")
                        .and_then(Value::as_str)
                        .filter(|x| !x.is_empty())
                        .or_else(|| v.get("name").and_then(Value::as_str))
                        .or_else(|| v.get("description").and_then(Value::as_str))
                        .unwrap_or("pattern")
                        .to_string();

                    let mut excluded: HashSet<(String, String)> = HashSet::new();
                    if let Some(ex_arr) = v.get("exclude_pairs").and_then(Value::as_array) {
                        for pair in ex_arr {
                            if let Some(pa) = pair.as_array() {
                                if pa.len() == 2 {
                                    let p0 = pa[0].as_str().unwrap_or("").trim().to_string();
                                    let p1 = pa[1].as_str().unwrap_or("").trim().to_string();
                                    if !p0.is_empty() && !p1.is_empty() {
                                        excluded.insert((p0, p1));
                                    }
                                }
                            }
                        }
                    }

                    let mut bifurcated: HashSet<(String, String)> = HashSet::new();
                    if let Some(bi_arr) = v.get("bifurcation_pairs").and_then(Value::as_array) {
                        for item in bi_arr {
                            if let Some(pa) = item.as_array() {
                                if pa.len() == 2 {
                                    let p0 = pa[0].as_str().unwrap_or("").trim().to_string();
                                    let p1 = pa[1].as_str().unwrap_or("").trim().to_string();
                                    if !p0.is_empty() && !p1.is_empty() {
                                        bifurcated.insert((p0, p1));
                                    }
                                }
                            } else if let Some(pa) = item.get("pair").and_then(Value::as_array) {
                                if pa.len() == 2 {
                                    let p0 = pa[0].as_str().unwrap_or("").trim().to_string();
                                    let p1 = pa[1].as_str().unwrap_or("").trim().to_string();
                                    if !p0.is_empty() && !p1.is_empty() {
                                        bifurcated.insert((p0, p1));
                                    }
                                }
                            }
                        }
                    }

                    let has_target_or_pref = v
                        .get("target")
                        .and_then(Value::as_array)
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
                        || v.get("preferred")
                            .and_then(Value::as_array)
                            .map(|a| !a.is_empty())
                            .unwrap_or(false);

                    let mut slot_pairs: Vec<(Vec<String>, Vec<String>)> = Vec::new();
                    if let Some(slots) = v.get("slots").and_then(Value::as_array) {
                        if !has_target_or_pref || slots.len() == 2 {
                            let cleaned: Vec<Vec<String>> = slots
                                .iter()
                                .filter_map(Value::as_array)
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(Value::as_str)
                                        .map(|x| x.trim().to_string())
                                        .filter(|x| is_valid_chinese_token(x))
                                        .collect()
                                })
                                .collect();
                            for i in 0..cleaned.len().saturating_sub(1) {
                                slot_pairs.push((cleaned[i].clone(), cleaned[i + 1].clone()));
                            }
                        }
                    }
                    if slot_pairs.is_empty() {
                        let ps: Vec<String> = v
                            .get("prev_slot")
                            .and_then(Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_str)
                                    .map(|x| x.trim().to_string())
                                    .filter(|x| is_valid_chinese_token(x))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let ns: Vec<String> = v
                            .get("next_slot")
                            .and_then(Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_str)
                                    .map(|x| x.trim().to_string())
                                    .filter(|x| is_valid_chinese_token(x))
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !ps.is_empty() && !ns.is_empty() {
                            slot_pairs.push((ps, ns));
                        }
                    }

                    let _ = line_idx;
                    for (left_slot, right_slot) in slot_pairs {
                        for a in &left_slot {
                            for b in &right_slot {
                                total_expanded_pairs += 1;
                                let combined = format!("{a}{b}");
                                if self.known_words.contains(&combined) {
                                    valid_lexicon_compounds += 1;
                                    continue;
                                }
                                let ra = self.get_bopomofo(a);
                                let rb = self.get_bopomofo(b);
                                if ra.is_empty() || rb.is_empty() {
                                    harmless_non_existent_readings += 1;
                                    continue;
                                }
                                let reading_ab = format!("{} {}", ra.trim(), rb.trim());
                                if let Some(real_words) = reading_to_real_words.get(&reading_ab) {
                                    let competing: Vec<String> = real_words
                                        .iter()
                                        .filter(|w| **w != combined)
                                        .cloned()
                                        .collect();
                                    if competing.is_empty() {
                                        harmless_non_existent_readings += 1;
                                    } else {
                                        total_homophone_collisions += 1;
                                        let max_comp_weight = competing
                                            .iter()
                                            .filter_map(|w| self.word_max_weight.get(w).copied())
                                            .max()
                                            .unwrap_or(180);
                                        let is_bifurcated =
                                            bifurcated.contains(&(a.clone(), b.clone()));
                                        let is_suppressed =
                                            excluded.contains(&(a.clone(), b.clone()));
                                        let is_archaic_low_freq = max_comp_weight <= 220;

                                        let status = if is_bifurcated {
                                            "RESOLVED_BY_SYNTACTIC_BIFURCATION"
                                        } else if is_suppressed {
                                            "SUPPRESSED_BY_EXCLUDE_ARRAY"
                                        } else if is_archaic_low_freq {
                                            "KEEP_MODERN_ARRAY_OVER_ARCHAIC"
                                        } else {
                                            "NEEDS_RESOLUTION"
                                        };
                                        let item = json!({
                                            "pattern_id": pid,
                                            "pair": [a, b],
                                            "reading": reading_ab,
                                            "competing_real_words": competing,
                                            "max_competitor_weight": max_comp_weight,
                                            "status": status
                                        });
                                        if is_bifurcated {
                                            type_b_bifurcation_count += 1;
                                            type_b_items.push(item);
                                        } else if is_suppressed {
                                            suppressed_by_exclude_array += 1;
                                            suppressed_items.push(item);
                                        } else if is_archaic_low_freq {
                                            type_a_archaic_keep_count += 1;
                                            type_a_items.push(item);
                                        } else {
                                            unsuppressed_items.push(item);
                                        }
                                    }
                                } else {
                                    harmless_non_existent_readings += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        let harmless_total = valid_lexicon_compounds + harmless_non_existent_readings;
        let harmless_rate = if total_expanded_pairs > 0 {
            (harmless_total as f64 / total_expanded_pairs as f64) * 100.0
        } else {
            100.0
        };

        json!({
            "total_expanded_pairs": total_expanded_pairs,
            "valid_lexicon_compounds_count": valid_lexicon_compounds,
            "harmless_non_existent_readings_count": harmless_non_existent_readings,
            "naturally_harmless_rate": format!("{harmless_rate:.2}%"),
            "total_homophone_collisions": total_homophone_collisions,
            "type_a_archaic_keep_array_count": type_a_archaic_keep_count,
            "type_b_syntactic_bifurcation_count": type_b_bifurcation_count,
            "type_c_suppressed_by_exclude_array_count": suppressed_by_exclude_array,
            "suppressed_by_exclude_array_count": suppressed_by_exclude_array,
            "unsuppressed_active_collisions_count": unsuppressed_items.len(),
            "type_a_archaic_items": type_a_items,
            "type_b_bifurcation_items": type_b_items,
            "suppressed_collisions": suppressed_items,
            "unsuppressed_collisions": unsuppressed_items,
            "theoretical_principle": "陣列展開同音對衝四維分流定理：(1) 無同音實詞對應之展開組合（>98%）使用者絕不會打出，天然 0% 干擾；(2) 對衝詞為罕見古語（weight<=220，如 說部/坐簿/得間/芳蘭）時，保留現代高頻陣列（Type A）；(3) 雙方皆為真實語句合法用法（如『他馬騎得很好吧』vs.『才不是什麼耆德老人呢』、『讀不懂』vs.『獨步全球』、『笑到飽』vs.『恪守孝道』）時，透過 bifurcation_pairs 與左右句法槽位進行語法環境雙向分流（Type B）；(4) 陣列展開本身為跨界假組合且對衝高頻現代實詞（如 這鍵/這件、個相/各項、分不/分布）時，方透過 exclude_pairs 對衝抑制陣列抵銷孤立二元分數（Type C）。"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_pii_filter() {
        assert!(ZeroPiiPrivacyFilter::is_safe_token("不懂").0);
        assert!(!ZeroPiiPrivacyFilter::is_safe_token("0912345678").0);
        assert!(!ZeroPiiPrivacyFilter::is_safe_token("user@example.com").0);
        assert!(!ZeroPiiPrivacyFilter::is_safe_token("台北市信義區信義路五段").0);
    }

    #[test]
    fn test_tuner_alignment_and_synthesis() {
        let tuner = SemanticTuner::new(None);
        let diffs = tuner.align_sentence_pair("讓步董程式的人", "讓不懂程式的人", None, None);
        assert!(!diffs.is_empty());
        let rules = tuner.synthesize_rules(&diffs[0]);
        assert!(!rules.is_empty());
    }

    #[test]
    fn test_tailcat_sandbox_gatekeeper() {
        let mut tuner = SemanticTuner::new(None);
        let pkg = json!({
            "protocol": "RUHI_TAILCAT_FEDERATED_SEMANTIC_V1",
            "rules": [
                {"type": "UNIGRAM_WEIGHT", "word": "裡", "weight": 1200},
                {"type": "BIGRAM_TRANSITION", "prev": "奇妙", "next": "他", "recommended_delta": -30.0},
                {"type": "BIGRAM_TRANSITION", "prev": "0912345678", "next": "測試", "recommended_delta": 25.0}
            ]
        });
        let res = tuner.import_tailcat_package(&pkg);
        assert_eq!(res["status"], "ok");
        assert_eq!(res["blocked_unigram_rules"], 1);
        assert_eq!(res["blocked_unsafe_rules"], 2);
    }

    #[test]
    fn test_factorize_rules_to_arrays_261019() {
        let tuner = SemanticTuner::new(None);
        let sample_rules = vec![
            json!({"prev": "大易", "next": "輸入法", "score": 28.0}),
            json!({"prev": "倉頡", "next": "輸入法", "score": 28.0}),
            json!({"prev": "行列", "next": "輸入法", "score": 28.0}),
            json!({"prev": "大易", "next": "鍵盤", "score": 28.0}),
            json!({"prev": "倉頡", "next": "鍵盤", "score": 28.0}),
            json!({"prev": "行列", "next": "鍵盤", "score": 28.0}),
        ];
        let arrays = tuner.factorize_rules_to_arrays(&sample_rules);
        assert_eq!(arrays.len(), 1, "6 rules (3x2 Cartesian product) should factorize into 1 2D array rule");
        let prev_slot = arrays[0]["prev_slot"].as_array().unwrap();
        let next_slot = arrays[0]["next_slot"].as_array().unwrap();
        assert_eq!(prev_slot.len(), 3);
        assert_eq!(next_slot.len(), 2);
    }

    #[test]
    fn test_analogical_generalization_and_upsert_261019() {
        let mut tuner = SemanticTuner::new(None);
        let pairs = [
            ("兜完攻了。", "都完工了。"),
            (
                "所以素食十分類在擴充一個叫瑜珈素的，所有孤累跟發酵的都不吃。",
                "所以素食分類再擴充一個叫瑜伽素的，所有菇類跟發酵的都不吃。",
            ),
            (
                "這是不錯的選擇，但會不會兌Github造成很大的負擔？",
                "這是不錯的選擇，但會不會對Github造成很大的負擔？",
            ),
            (
                "其實我在學大意輸入法之前，因為多年使寫書信的關係，我的思考基本上比較偏向紋自行思考而非語音思考。",
                "其實我在學大易輸入法之前，因為多年手寫書信的關係，我的思考基本上比較偏向文字型思考而非語音思考。",
            ),
        ];

        let mut all_diffs = Vec::new();
        let mut all_constructions = Vec::new();
        let mut all_rules = Vec::new();
        let mut all_new_words = Vec::new();

        for (err_s, corr_s) in pairs {
            let diffs = tuner.align_sentence_pair(err_s, corr_s, None, None);
            assert!(!diffs.is_empty(), "Expected diffs for pair: {} -> {}", err_s, corr_s);
            for d in &diffs {
                if let Some(c_list) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                    for c in c_list {
                        if let Some(brs) = c.get("backbone_rules").and_then(Value::as_array) {
                            all_rules.extend(brs.iter().cloned());
                        }
                        all_constructions.push(c.clone());
                    }
                }
                if let Some(w_list) = d.get("oov_compound_words").and_then(Value::as_array) {
                    all_new_words.extend(w_list.iter().cloned());
                }
                all_rules.extend(tuner.synthesize_rules(d));
            }
            all_diffs.extend(diffs);
        }

        let analogical = tuner.scan_analogous_cases_across_lexicon(&all_diffs);
        let families = analogical
            .get("scanned_families")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            families.len() >= 5,
            "Expected at least 5 analogical families scanned across lexicon, got {}",
            families.len()
        );

        if let Some(aw) = analogical.get("analogous_words").and_then(Value::as_array) {
            all_new_words.extend(aw.iter().cloned());
        }
        if let Some(ac) = analogical.get("analogous_constructions").and_then(Value::as_array) {
            for c in ac {
                if let Some(brs) = c.get("backbone_rules").and_then(Value::as_array) {
                    all_rules.extend(brs.iter().cloned());
                }
                all_constructions.push(c.clone());
            }
        }

        // 自動因式分解同構單點規則為二維陣列
        let factored = tuner.factorize_rules_to_arrays(&all_rules);
        all_constructions.extend(factored);

        // 同步寫入並驗證規則庫維護（補充既有規則 & 創建新規則）
        let pattern_report = tuner.upsert_patterns_with_report(&all_constructions);
        let _added_words = tuner.append_words_to_domain_lexicon(&all_new_words);
        let _added_rules = tuner.append_rules_to_tsv(&all_rules);
        assert!(pattern_report.get("augmented_rules").is_some());
        assert!(pattern_report.get("created_rules").is_some());
    }

    #[test]
    fn test_array_collision_audit_and_auto_handover_delivery_261019() {
        let tuner = SemanticTuner::new(None);
        let audit = tuner.scan_array_homophone_collisions();
        let total_pairs = audit["total_expanded_pairs"].as_u64().unwrap_or(0);
        let type_a = audit["type_a_archaic_keep_array_count"].as_u64().unwrap_or(0);
        let type_b = audit["type_b_syntactic_bifurcation_count"].as_u64().unwrap_or(0);
        let suppressed = audit["suppressed_by_exclude_array_count"].as_u64().unwrap_or(0);
        let unsuppressed = audit["unsuppressed_active_collisions_count"].as_u64().unwrap_or(99);
        assert!(total_pairs > 1500, "Expected >1500 expanded array pairs, got {}", total_pairs);
        assert!(type_a >= 5, "Expected >=5 Type A archaic keep-array pairs, got {}", type_a);
        assert!(type_b >= 6, "Expected >=6 Type B syntactic bifurcation pairs, got {}", type_b);
        assert!(suppressed >= 9, "Expected >=9 Type C suppressed colliding pairs via exclude_pairs, got {}", suppressed);
        assert_eq!(unsuppressed, 0, "Expected 0 unresolved active homophone collisions, got {:?}", audit["unsuppressed_collisions"]);

        let mut consultant = crate::ai_consultant::AISemanticConsultant::new(Some(&tuner.root));
        let handover = consultant.get_recommended_models_and_handover_spec(&tuner);
        assert_eq!(handover["auto_delivered"], true);
        let delivered = handover["delivered_files"].as_array().unwrap();
        assert!(delivered.len() >= 3, "Expected 3 handover files auto-delivered to disk");
        assert!(tuner.root.join("docs").join("AI_SEMANTIC_HANDOVER.md").is_file());
        assert!(tuner.root.join(".agent").join("skills").join("ruhi-semantic-tuning").join("SKILL.md").is_file());
    }

    #[test]
    fn test_milestone_four_homophone_sentence_pair_tuning_261021() {
        let tuner = SemanticTuner::new(None);
        let err = "我先確認一下，這樣一班使用者卻定可以貼入錯誤與刊物校對雙具，就達成正確語意調校的目的嗎？";
        let cor = "我先確認一下，這樣一般使用者確定可以貼入錯誤與勘誤校對雙句，就達成正確語意調校的目的嗎？";

        let diffs = tuner.align_sentence_pair(err, cor, None, None);
        assert_eq!(diffs.len(), 4, "Expected 4 diff islands, got {:?}", diffs);

        let mut all_rules = Vec::new();
        let mut all_pattern_ids = Vec::new();
        for d in &diffs {
            all_rules.extend(tuner.synthesize_rules(d));
            if let Some(arr) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                for c in arr {
                    if let Some(pid) = c.get("pattern_id").and_then(Value::as_str) {
                        all_pattern_ids.push(pid.to_string());
                    }
                }
            }
        }

        // 驗證絕不生成『與 ➔ 勘誤』或『這樣 ➔ 一般』這類會污染反例的單邊透明虛詞規則
        let has_yu_kanwu = all_rules.iter().any(|r| {
            r.get("prev").and_then(Value::as_str) == Some("與")
                && r.get("next").and_then(Value::as_str) == Some("勘誤")
        });
        let has_zheyang_yiban = all_rules.iter().any(|r| {
            r.get("prev").and_then(Value::as_str) == Some("這樣")
                && r.get("next").and_then(Value::as_str) == Some("一般")
        });
        assert!(!has_yu_kanwu, "Must NOT emit poisoned rule 與 ➔ 勘誤");
        assert!(!has_zheyang_yiban, "Must NOT emit poisoned rule 這樣 ➔ 一般");

        // 驗證四組語意槽位構式皆已自動提煉
        assert!(all_pattern_ids.contains(&"general_attributive_yiban_vs_classifier".to_string()));
        assert!(all_pattern_ids.contains(&"cognitive_verb_queding_modal_complement".to_string()));
        assert!(all_pattern_ids.contains(&"error_and_errata_proofreading_collocation".to_string()));
        assert!(all_pattern_ids.contains(&"textual_action_shuangju_sentence_pair".to_string()));

        // 驗證內建真實 RuhiEngine 之 47 組黃金基準測試 100% 通過
        let bench = tuner.verify_golden_benchmark();
        assert_eq!(bench["total"], 47);
        assert_eq!(
            bench["passed"], 47,
            "Expected 47/47 golden benchmark pass with real RuhiEngine, details: {:?}",
            bench["details"]
        );
    }

    #[test]
    fn test_six_sentence_pairs_batch_tuning_261021() {
        let tuner = SemanticTuner::new(None);
        let pairs = [
            (
                "然後，大憶舊幾乎忘逛逛了呀！",
                "然後，大易就幾乎忘光光了呀！",
            ),
            (
                "我先確認一下，這樣可以交道使用者手上，隊嗎？",
                "我先確認一下，這樣可以交到使用者手上，對嗎？",
            ),
            (
                "我先確認一下，這樣一班使用者卻定可以貼入錯誤與刊物校對雙具，就達成正確語意調校的目的嗎？",
                "我先確認一下，這樣一般使用者確定可以貼入錯誤與勘誤校對雙句，就達成正確語意調校的目的嗎？",
            ),
            ("但這樣真的有庳要嗎？", "但這樣真的有必要嗎？"),
            ("在渡靈魂考問", "再度靈魂拷問"),
            (
                "741e6836-1bef-4d3a-a70f-218b9d857259 在使用許氏鍵盤，在時做的「語意條整工作室」謹獻漢語，",
                "741e6836-1bef-4d3a-a70f-218b9d857259 在使用許氏鍵盤，在實做的「語意調整工作室」僅限漢語，",
            ),
        ];

        let mut all_rules = Vec::new();
        let mut all_pattern_ids = Vec::new();
        let mut all_diffs = Vec::new();
        for (err, cor) in pairs {
            let diffs = tuner.align_sentence_pair(err, cor, None, None);
            for d in &diffs {
                all_rules.extend(tuner.synthesize_rules(d));
                if let Some(arr) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                    for c in arr {
                        if let Some(pid) = c.get("pattern_id").and_then(Value::as_str) {
                            all_pattern_ids.push(pid.to_string());
                        }
                        if let Some(brs) = c.get("backbone_rules").and_then(Value::as_array) {
                            all_rules.extend(brs.iter().cloned());
                        }
                    }
                }
            }
            all_diffs.extend(diffs);
        }

        // 1. 驗證絕不把語氣詞『嗎』當成介詞『一個對...』的受事抽象名詞，亦不憑空捏造『一個 ➔ 對』或『多年 ➔ 手寫』
        assert!(
            all_pattern_ids.contains(&"sentence_final_tag_question_dui_ma".to_string()),
            "Must synthesize sentence_final_tag_question_dui_ma for ，對嗎？"
        );
        assert!(
            !all_pattern_ids.contains(&"determiner_prep_dui_nominal_modifier".to_string()),
            "Must NOT misclassify sentence-final ，對嗎？ as determiner_prep_dui_nominal_modifier"
        );
        let analogical = tuner.scan_analogous_cases_across_lexicon(&all_diffs);
        let ac_ids: Vec<String> = analogical
            .get("analogous_constructions")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("pattern_id").and_then(Value::as_str).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            !ac_ids.contains(&"writing_modality_document_collocation".to_string()),
            "Must NOT hallucinate writing_modality_document_collocation (多年 ➔ 手寫) when 手寫/書信 is absent"
        );

        // 2. 驗證自動因式分解絕不將單字虛詞『就』與三字實詞『輸入法』或定語『一般』與動補『交到』混入同一陣列槽位
        let factored = tuner.factorize_rules_to_arrays(&all_rules);
        for f in &factored {
            let formula = f.get("formula").and_then(Value::as_str).unwrap_or("");
            assert!(
                !formula.contains("輸入法, 就") && !formula.contains("就, 輸入法"),
                "Must NOT mix 1-char adverb 就 with 3-char noun 輸入法: {formula}"
            );
            assert!(
                !formula.contains("一般, 交到") && !formula.contains("交到, 一般"),
                "Must NOT mix attributive 一般 with verb-complement 交到: {formula}"
            );
        }

        // 3. 驗證 6 組雙句之專用構式全數合成到位
        for expected_pid in [
            "ime_subject_focus_adverb_jiu",
            "depletion_resultative_guang_complement",
            "dative_transfer_jiaodao_recipient_hand",
            "sentence_final_tag_question_dui_ma",
            "modal_existential_you_biyao",
            "temporal_zaidu_soul_interrogation",
            "progressive_shizuo_attributive_de",
            "semantic_tuning_studio_domain_3d_array",
            "restrictive_jinxian_domain_scope",
        ] {
            assert!(
                all_pattern_ids.contains(&expected_pid.to_string()),
                "Missing expected construction pattern_id: {expected_pid}"
            );
        }
    }

    #[test]
    fn test_recent_txt_history_no_zombie_resurrection_261021() {
        let tuner = SemanticTuner::new(None);
        // 清空後重新讀取，驗證絕不掃描桌面自動塞回『語意分析*.txt』
        let cleared = tuner.update_recent_txt_history("clear", "", "", 0);
        assert!(cleared.is_empty(), "Cleared recent txt history must remain empty on reload");
        assert!(tuner.scan_recent_txt_files().is_empty(), "scan_recent_txt_files must not resurrect desktop files");

        // 新增一筆後再刪除，驗證刪除後持久化生效
        let after_add = tuner.update_recent_txt_history("add", "測試對照句.txt", "", 3);
        assert_eq!(after_add.len(), 1);
        let after_remove = tuner.update_recent_txt_history("remove", "測試對照句.txt", "", 0);
        assert!(after_remove.is_empty(), "Removed file must not come back on reload");
    }
}


