//! 臺灣正體中文多維度在地化排序與字元檢索核心 (`cjk_sort_engine.rs`)
//!
//! 對應 Python 版 `_lib/cjk_sort_engine.py` 與前端 `cjk_sorter.js`。
//! 遵守 ATG 第二定律：「在地化語意純粹與尊嚴定律 (L10n Semantic Precision & Linguistic Sovereignty Law)」。
//!
//! ### 支援排序維度
//! 1. **教育部標準注音符號序 (`sort_by_zhuyin`)**：`ㄅ ㄆ ㄇ ㄈ ㄉ ㄊ ㄋ ㄌ ㄍ ㄎ ㄏ ㄐ ㄑ ㄒ ㄓ ㄔ ㄕ ㄖ ㄗ ㄘ ㄙ ㄧ ㄨ ㄩ`。
//! 2. **康熙字典 214 部首序 (`sort_by_radical`)**：自「一部」至「龠部」，內建偏旁正規化（如 `氵➔水`、`扌➔手`、`艹➔艸`）。
//! 3. **標準筆畫數序 (`sort_by_stroke`)**：由少至多（或反序）依正體字筆畫數排列。
//! 4. **漢語拼音字母序 (`sort_by_pinyin`)**：A–Z 音序排列。
//! 5. **臺灣閩南語羅馬字（臺羅 Tâi-lô / 白話字 POJ）音序 (`sort_by_taigi`)**：分離聲調附加符號進行基底音序與聲調排序。
//! 6. **書名號與前綴自動清洗 (`clean_title`)**：自動剝除前置《》、「」、『』、括號與編號前綴，鎖定真實核心首字。

/// 教育部標準注音符號排序表（37 符號之首音與介音）
pub const BOPOMOFO_ORDER: [char; 37] = [
    'ㄅ', 'ㄆ', 'ㄇ', 'ㄈ', 'ㄉ', 'ㄊ', 'ㄋ', 'ㄌ', 'ㄍ', 'ㄎ', 'ㄏ', 'ㄐ', 'ㄑ', 'ㄒ', 'ㄓ', 'ㄔ',
    'ㄕ', 'ㄖ', 'ㄗ', 'ㄘ', 'ㄙ', 'ㄚ', 'ㄛ', 'ㄜ', 'ㄝ', 'ㄞ', 'ㄟ', 'ㄠ', 'ㄡ', 'ㄢ', 'ㄣ', 'ㄤ',
    'ㄥ', 'ㄦ', 'ㄧ', 'ㄨ', 'ㄩ',
];

/// 康熙字典標準 214 部首表
pub const KANGXI_RADICALS: [char; 214] = [
    '一', '丨', '丶', '丿', '乙', '亅', '二', '亠', '人', '儿', '入', '八', '冂', '冖', '冫', '几',
    '凵', '刀', '力', '勹', '匕', '匚', '匸', '十', '卜', '卩', '厂', '厶', '又', '口', '囗', '土',
    '士', '夂', '夊', '夕', '大', '女', '子', '宀', '寸', '小', '尢', '尸', '屮', '山', '巛', '工',
    '己', '巾', '干', '幺', '广', '廴', '廾', '弋', '弓', '彐', '彡', '彳', '心', '戈', '戶', '手',
    '支', '攴', '文', '斗', '斤', '方', '无', '日', '曰', '月', '木', '欠', '止', '歹', '殳', '毋',
    '比', '毛', '氏', '气', '水', '火', '爪', '父', '爻', '爿', '片', '牙', '牛', '犬', '玄', '玉',
    '瓜', '瓦', '甘', '生', '用', '田', '疋', '疒', '癶', '白', '皮', '皿', '目', '矛', '矢', '石',
    '示', '禸', '禾', '穴', '立', '竹', '米', '糸', '缶', '网', '羊', '羽', '老', '而', '耒', '耳',
    '聿', '肉', '臣', '自', '至', '臼', '舌', '舛', '舟', '艮', '色', '艸', '虍', '虫', '血', '行',
    '衣', '襾', '見', '角', '言', '谷', '豆', '豕', '豸', '貝', '赤', '走', '足', '身', '車', '辛',
    '辰', '辵', '邑', '酉', '釆', '里', '金', '長', '門', '阜', '隶', '隹', '雨', '靑', '非', '面',
    '革', '韋', '韭', '音', '頁', '風', '飛', '食', '首', '香', '馬', '骨', '高', '髟', '鬥', '鬯',
    '鬲', '鬼', '魚', '鳥', '鹵', '鹿', '麥', '麻', '黃', '黍', '黑', '黹', '黽', '鼎', '鼓', '鼠',
    '鼻', '齊', '齒', '龍', '龜', '龠',
];

/// 單一漢字之音韻、部首與筆畫元資料
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CjkCharInfo {
    pub ch: char,
    pub zhuyin: &'static str,
    pub zhuyin_first: char,
    pub zhuyin_rank: u16,
    pub pinyin: &'static str,
    pub radical: char,
    pub radical_norm: char,
    pub radical_rank: u16,
    pub strokes: u8,
}

/// 將常見變體部首偏旁正規化為標準康熙部首（例如 `氵➔水`、`扌➔手`、`艹➔艸`）。
pub fn normalize_radical(rad: char) -> char {
    match rad {
        '氵' | '氺' => '水',
        '扌' | '龵' => '手',
        '艹' | '䒑' => '艸',
        '亻' | '𠆢' => '人',
        '忄' | '⺗' => '心',
        '犭' => '犬',
        '礻' => '示',
        '衤' => '衣',
        '糹' | '纟' => '糸',
        '钅' => '金',
        '饣' => '食',
        '讠' => '言',
        '辶' => '辵',
        '阝' => '阜',
        '罒' => '网',
        '灬' => '火',
        _ => rad,
    }
}

/// 查詢注音符號在教育部注音表中的排序權重（0..36，非注音回傳 999）。
pub fn bopomofo_rank(c: char) -> u16 {
    BOPOMOFO_ORDER
        .iter()
        .position(|&x| x == c)
        .map(|i| i as u16)
        .unwrap_or(999)
}

/// 查詢部首在康熙 214 部首中的排序權重（0..213，未知回傳 999）。
pub fn radical_rank(rad: char) -> u16 {
    let norm = normalize_radical(rad);
    KANGXI_RADICALS
        .iter()
        .position(|&x| x == norm)
        .map(|i| i as u16)
        .unwrap_or(999)
}

/// 內建高頻正體中文字元檢索表：(字元, 注音, 拼音, 部首, 筆畫數)
const CORE_CJK_TABLE: &[(char, &str, &str, char, u8)] = &[
    ('一', "ㄧ", "yi", '一', 1),
    ('乙', "ㄧˇ", "yi", '乙', 1),
    ('二', "ㄦˋ", "er", '二', 2),
    ('十', "ㄕˊ", "shi", '十', 2),
    ('人', "ㄖㄣˊ", "ren", '人', 2),
    ('入', "ㄖㄨˋ", "ru", '入', 2),
    ('八', "ㄅㄚ", "ba", '八', 2),
    ('刀', "ㄉㄠ", "dao", '刀', 2),
    ('力', "ㄌㄧˋ", "li", '力', 2),
    ('三', "ㄙㄢ", "san", '一', 3),
    ('上', "ㄕㄤˋ", "shang", '一', 3),
    ('下', "ㄒㄧㄚˋ", "xia", '一', 3),
    ('大', "ㄉㄚˋ", "da", '大', 3),
    ('女', "ㄋㄩˇ", "nv", '女', 3),
    ('子', "ㄗˇ", "zi", '子', 3),
    ('小', "ㄒㄧㄠˇ", "xiao", '小', 3),
    ('山', "ㄕㄢ", "shan", '山', 3),
    ('工', "ㄍㄨㄥ", "gong", '工', 3),
    ('口', "ㄎㄡˇ", "kou", '口', 3),
    ('土', "ㄊㄨˇ", "tu", '土', 3),
    ('不', "ㄅㄨˋ", "bu", '一', 4),
    ('中', "ㄓㄨㄥ", "zhong", '丨', 4),
    ('天', "ㄊㄧㄢ", "tian", '大', 4),
    ('太', "ㄊㄞˋ", "tai", '大', 4),
    ('日', "ㄖˋ", "ri", '日', 4),
    ('月', "ㄩㄝˋ", "yue", '月', 4),
    ('木', "ㄇㄨˋ", "mu", '木', 4),
    ('水', "ㄕㄨㄟˇ", "shui", '水', 4),
    ('火', "ㄏㄨㄛˇ", "huo", '火', 4),
    ('心', "ㄒㄧㄣ", "xin", '心', 4),
    ('手', "ㄕㄡˇ", "shou", '手', 4),
    ('文', "ㄨㄣˊ", "wen", '文', 4),
    ('方', "ㄈㄤ", "fang", '方', 4),
    ('比', "ㄅㄧˇ", "bi", '比', 4),
    ('王', "ㄨㄤˊ", "wang", '玉', 4),
    ('包', "ㄅㄠ", "bao", '勹', 5),
    ('北', "ㄅㄟˇ", "bei", '匕', 5),
    ('本', "ㄅㄣˇ", "ben", '木', 5),
    ('白', "ㄅㄞˊ", "bai", '白', 5),
    ('平', "ㄆㄧㄥˊ", "ping", '干', 5),
    ('民', "ㄇㄧㄣˊ", "min", '氏', 5),
    ('目', "ㄇㄨˋ", "mu", '目', 5),
    ('田', "ㄊㄧㄢˊ", "tian", '田', 5),
    ('生', "ㄕㄥ", "sheng", '生', 5),
    ('用', "ㄩㄥˋ", "yong", '用', 5),
    ('立', "ㄌㄧˋ", "li", '立', 5),
    ('石', "ㄕˊ", "shi", '石', 5),
    ('光', "ㄍㄨㄤ", "guang", '儿', 6),
    ('全', "ㄑㄩㄢˊ", "quan", '入', 6),
    ('共', "ㄍㄨㄥˋ", "gong", '八', 6),
    ('合', "ㄏㄜˊ", "he", '口', 6),
    ('同', "ㄊㄨㄥˊ", "tong", '口', 6),
    ('名', "ㄇㄧㄥˊ", "ming", '口', 6),
    ('地', "ㄉㄧˋ", "di", '土', 6),
    ('多', "ㄉㄨㄛ", "duo", '夕', 6),
    ('好', "ㄏㄠˇ", "hao", '女', 6),
    ('字', "ㄗˋ", "zi", '子', 6),
    ('安', "ㄢ", "an", '宀', 6),
    ('年', "ㄋㄧㄢˊ", "nian", '干', 6),
    ('早', "ㄗㄠˇ", "zao", '日', 6),
    ('曲', "ㄑㄩˇ", "qu", '曰', 6),
    ('有', "ㄧㄡˇ", "you", '月', 6),
    ('百', "ㄅㄞˇ", "bai", '白', 6),
    ('竹', "ㄓㄨˊ", "zhu", '竹', 6),
    ('米', "ㄇㄧˇ", "mi", '米', 6),
    ('自', "ㄗˋ", "zi", '自', 6),
    ('行', "ㄒㄧㄥˊ", "xing", '行', 6),
    ('衣', "ㄧ", "yi", '衣', 6),
    ('西', "ㄒㄧ", "xi", '襾', 6),
    ('作', "ㄗㄨㄛˋ", "zuo", '人', 7),
    ('你', "ㄋㄧˇ", "ni", '人', 7),
    ('冷', "ㄌㄥˇ", "leng", '冫', 7),
    ('初', "ㄔㄨ", "chu", '刀', 7),
    ('別', "ㄅㄧㄝˊ", "bie", '刀', 7),
    ('助', "ㄓㄨˋ", "zhu", '力', 7),
    ('吹', "ㄔㄨㄟ", "chui", '口', 7),
    ('告', "ㄍㄠˋ", "gao", '口', 7),
    ('快', "ㄎㄨㄞˋ", "kuai", '心', 7),
    ('我', "ㄨㄛˇ", "wo", '戈', 7),
    ('李', "ㄌㄧˇ", "li", '木', 7),
    ('步', "ㄅㄨˋ", "bu", '止', 7),
    ('每', "ㄇㄟˇ", "mei", '毋', 7),
    ('江', "ㄐㄧㄤ", "jiang", '水', 6),
    ('河', "ㄏㄜˊ", "he", '水', 8),
    ('海', "ㄏㄞˇ", "hai", '水', 10),
    ('洋', "ㄧㄤˊ", "yang", '水', 9),
    ('湖', "ㄏㄨˊ", "hu", '水', 12),
    ('林', "ㄌㄧㄣˊ", "lin", '木', 8),
    ('森', "ㄙㄣ", "sen", '木', 12),
    ('松', "ㄙㄨㄥ", "song", '木', 8),
    ('柏', "ㄅㄛˊ", "bo", '木', 9),
    ('東', "ㄉㄨㄥ", "dong", '木', 8),
    ('果', "ㄍㄨㄛˇ", "guo", '木', 8),
    ('明', "ㄇㄧㄥˊ", "ming", '日', 8),
    ('星', "ㄒㄧㄥ", "xing", '日', 9),
    ('春', "ㄔㄨㄣ", "chun", '日', 9),
    ('夏', "ㄒㄧㄚˋ", "xia", '夂', 10),
    ('秋', "ㄑㄧㄡ", "qiu", '禾', 9),
    ('冬', "ㄉㄨㄥ", "dong", '冫', 5),
    ('風', "ㄈㄥ", "feng", '風', 9),
    ('花', "ㄏㄨㄚ", "hua", '艸', 8),
    ('草', "ㄘㄠˇ", "cao", '艸', 10),
    ('雪', "ㄒㄩㄝˇ", "xue", '雨', 11),
    ('雲', "ㄩㄣˊ", "yun", '雨', 12),
    ('電', "ㄉㄧㄢˋ", "dian", '雨', 13),
    ('雷', "ㄌㄟˊ", "lei", '雨', 13),
    ('音', "ㄧㄣ", "yin", '音', 9),
    ('樂', "ㄩㄝˋ", "yue", '木', 15),
    ('歌', "ㄍㄜ", "ge", '欠', 14),
    ('舞', "ㄨˇ", "wu", '舛', 14),
    ('詩', "ㄕ", "shi", '言', 13),
    ('詞', "ㄘˊ", "ci", '言', 12),
    ('語', "ㄩˇ", "yu", '言', 14),
    ('論', "ㄌㄨㄣˋ", "lun", '言', 15),
    ('金', "ㄐㄧㄣ", "jin", '金', 8),
    ('銀', "ㄧㄣˊ", "yin", '金', 14),
    ('銅', "ㄊㄨㄥˊ", "tong", '金', 14),
    ('鐵', "ㄊㄧㄝˇ", "tie", '金', 21),
    ('臺', "ㄊㄞˊ", "tai", '至', 14),
    ('台', "ㄊㄞˊ", "tai", '口', 5),
    ('灣', "ㄨㄢ", "wan", '水', 25),
    ('華', "ㄏㄨㄚˊ", "hua", '艸', 12),
    ('國', "ㄍㄨㄛˊ", "guo", '囗', 11),
    ('家', "ㄐㄧㄚ", "jia", '宀', 10),
    ('愛', "ㄞˋ", "ai", '心', 13),
    ('靈', "ㄌㄧㄥˊ", "ling", '雨', 24),
    ('魂', "ㄏㄨㄣˊ", "hun", '鬼', 14),
    ('神', "ㄕㄣˊ", "shen", '示', 10),
    ('聖', "ㄕㄥˋ", "sheng", '耳', 13),
    ('智', "ㄓˋ", "zhi", '日', 12),
    ('慧', "ㄏㄨㄟˋ", "hui", '心', 15),
    ('道', "ㄉㄠˋ", "dao", '辵', 13),
    ('德', "ㄉㄜˊ", "de", '彳', 15),
    ('善', "ㄕㄢˋ", "shan", '口', 12),
    ('美', "ㄇㄟˇ", "mei", '羊', 9),
    ('真', "ㄓㄣ", "zhen", '目', 10),
    ('理', "ㄌㄧˇ", "li", '玉', 11),
    ('裡', "ㄌㄧˇ", "li", '衣', 13),
    ('視', "ㄕˋ", "shi", '見', 12),
    ('是', "ㄕˋ", "shi", '日', 9),
    ('越', "ㄩㄝˋ", "yue", '走', 12),
    ('如', "ㄖㄨˊ", "ru", '女', 6),
    ('喜', "ㄒㄧˇ", "xi", '口', 12),
    ('繪', "ㄏㄨㄟˋ", "hui", '糸', 19),
    ('荷', "ㄏㄜˊ", "he", '艸', 11),
    ('晨', "ㄔㄣˊ", "chen", '日', 11),
    ('午', "ㄨˇ", "wu", '十', 4),
    ('晚', "ㄨㄢˇ", "wan", '日', 11),
    ('夜', "ㄧㄝˋ", "ye", '夕', 8),
    ('爸', "ㄅㄚˋ", "ba", '父', 8),
    ('朋', "ㄆㄥˊ", "peng", '月', 8),
    ('友', "ㄧㄡˇ", "you", '又', 4),
    ('媽', "ㄇㄚ", "ma", '女', 13),
    ('清', "ㄑㄧㄥ", "qing", '水', 11),
    ('流', "ㄌㄧㄡˊ", "liu", '水', 10),
    ('龍', "ㄌㄨㄥˊ", "long", '龍', 16),
    ('龜', "ㄍㄨㄟ", "gui", '龜', 16),
    ('龠', "ㄩㄝˋ", "yue", '龠', 17),
];

/// 漢字多維度排序檢索模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum CjkSortMode {
    /// 臺灣教育部標準注音符號順序 (`ㄅㄆㄇㄈ...`)
    #[default]
    Zhuyin,
    /// 康熙字典 214 部首順序 (`一..龠`)
    Radical,
    /// 正體字總筆畫數順序
    Stroke,
    /// 漢語拼音字母順序 (`A..Z`)
    Pinyin,
    /// 臺灣閩南語羅馬字（臺羅 Tâi-lô / 白話字 POJ）順序
    Taigi,
    /// 原始 Unicode 碼位順序
    Unicode,
}

impl CjkSortMode {
    pub fn display_name_tw(self) -> &'static str {
        match self {
            Self::Zhuyin => "注音符號序 (ㄅㄆㄇㄈ)",
            Self::Radical => "康熙 214 部首序 (一～龠)",
            Self::Stroke => "正體總筆畫數序",
            Self::Pinyin => "漢語拼音字母序 (A-Z)",
            Self::Taigi => "臺語羅馬字序 (Tâi-lô / POJ)",
            Self::Unicode => "Unicode 內碼序",
        }
    }
}

/// CJK 中文多維度排序與字元檢索核心引擎
#[derive(Debug, Clone, Copy, Default)]
pub struct CjkSortEngine;

impl CjkSortEngine {
    pub const fn new() -> Self {
        Self
    }

    /// 產生供通用表格 (`UniversalTable`) 進行多模態比較的複合排序鍵 `(主權重, 次權重, 音韻/羅馬字鍵, 核心字串)`。
    pub fn sort_key(
        &self,
        text: &str,
        mode: CjkSortMode,
        clean_prefix: bool,
    ) -> (u16, u16, String, String) {
        let target = if clean_prefix {
            Self::clean_title(text)
        } else {
            text.trim()
        };
        let first_info = target.chars().next().map(Self::get_char_info);
        match mode {
            CjkSortMode::Zhuyin => {
                if let Some(info) = first_info {
                    (
                        info.zhuyin_rank,
                        u16::from(info.strokes),
                        info.zhuyin.to_string(),
                        target.to_string(),
                    )
                } else {
                    (999, 999, String::new(), target.to_string())
                }
            }
            CjkSortMode::Radical => {
                if let Some(info) = first_info {
                    (
                        info.radical_rank,
                        u16::from(info.strokes),
                        info.zhuyin.to_string(),
                        target.to_string(),
                    )
                } else {
                    (999, 999, String::new(), target.to_string())
                }
            }
            CjkSortMode::Stroke => {
                if let Some(info) = first_info {
                    (
                        u16::from(info.strokes),
                        info.zhuyin_rank,
                        info.zhuyin.to_string(),
                        target.to_string(),
                    )
                } else {
                    (999, 999, String::new(), target.to_string())
                }
            }
            CjkSortMode::Pinyin => {
                if let Some(info) = first_info {
                    (
                        0,
                        u16::from(info.strokes),
                        info.pinyin.to_string(),
                        target.to_string(),
                    )
                } else {
                    (999, 999, String::new(), target.to_string())
                }
            }
            CjkSortMode::Taigi => {
                let (base, tones) = normalize_taigi_romanization(target);
                let tone_first = tones.first().copied().unwrap_or(1);
                (0, u16::from(tone_first), base, target.to_string())
            }
            CjkSortMode::Unicode => (0, 0, target.to_string(), text.to_string()),
        }
    }
    /// 去除前置書名號、括號、引號、空白與數字編號，取得真實核心標題字串。
    pub fn clean_title(title: &str) -> &str {
        title
            .trim_start_matches(|c: char| {
                matches!(
                    c,
                    '《' | '〈'
                        | '「'
                        | '『'
                        | '['
                        | ']'
                        | '('
                        | ')'
                        | '（'
                        | '）'
                        | '【'
                        | '】'
                        | '"'
                        | '\''
                        | '.'
                        | '-'
                        | '、'
                        | ':'
                        | '：'
                ) || c.is_whitespace()
                    || c.is_ascii_digit()
            })
            .trim()
    }

    /// 取得單一字元之注音、拼音、康熙部首與筆畫數資訊。
    pub fn get_char_info(ch: char) -> CjkCharInfo {
        if let Some(&(_, zhuyin, pinyin, rad, strokes)) =
            CORE_CJK_TABLE.iter().find(|&&(c, _, _, _, _)| c == ch)
        {
            let z_first = zhuyin.chars().next().unwrap_or(ch);
            let z_rank = bopomofo_rank(z_first);
            let rad_norm = normalize_radical(rad);
            let r_rank = radical_rank(rad_norm);
            return CjkCharInfo {
                ch,
                zhuyin,
                zhuyin_first: z_first,
                zhuyin_rank: z_rank,
                pinyin,
                radical: rad,
                radical_norm: rad_norm,
                radical_rank: r_rank,
                strokes,
            };
        }

        // 若字元本身即為注音符號
        let b_rank = bopomofo_rank(ch);
        if b_rank < 999 {
            return CjkCharInfo {
                ch,
                zhuyin: "",
                zhuyin_first: ch,
                zhuyin_rank: b_rank,
                pinyin: "",
                radical: ch,
                radical_norm: ch,
                radical_rank: 999,
                strokes: 2,
            };
        }

        // 若字元本身即為康熙 214 部首或其變體
        let r_norm = normalize_radical(ch);
        let r_rank = radical_rank(r_norm);
        if r_rank < 999 {
            return CjkCharInfo {
                ch,
                zhuyin: "",
                zhuyin_first: ch,
                zhuyin_rank: 999,
                pinyin: "",
                radical: ch,
                radical_norm: r_norm,
                radical_rank: r_rank,
                strokes: estimate_strokes_by_radical_rank(r_rank),
            };
        }

        CjkCharInfo {
            ch,
            zhuyin: "",
            zhuyin_first: ch,
            zhuyin_rank: 999,
            pinyin: "",
            radical: ch,
            radical_norm: ch,
            radical_rank: 999,
            strokes: 10,
        }
    }

    /// 依臺灣教育部標準注音符號順序（`ㄅㄆㄇㄈ...`）排序。
    pub fn sort_by_zhuyin<T, F>(items: &mut [T], mut key_fn: F, reverse: bool)
    where
        F: FnMut(&T) -> &str,
    {
        items.sort_by(|a, b| {
            let ta = key_fn(a);
            let tb = key_fn(b);
            let ca = Self::clean_title(ta);
            let cb = Self::clean_title(tb);
            let fa = ca.chars().next().map(Self::get_char_info);
            let fb = cb.chars().next().map(Self::get_char_info);

            let ord = match (fa, fb) {
                (Some(ia), Some(ib)) => ia
                    .zhuyin_rank
                    .cmp(&ib.zhuyin_rank)
                    .then_with(|| ia.zhuyin.cmp(ib.zhuyin))
                    .then_with(|| ca.cmp(cb))
                    .then_with(|| ta.cmp(tb)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ta.cmp(tb),
            };
            if reverse {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    /// 依康熙字典 214 部首順序排序。
    pub fn sort_by_radical<T, F>(items: &mut [T], mut key_fn: F, reverse: bool)
    where
        F: FnMut(&T) -> &str,
    {
        items.sort_by(|a, b| {
            let ta = key_fn(a);
            let tb = key_fn(b);
            let ca = Self::clean_title(ta);
            let cb = Self::clean_title(tb);
            let fa = ca.chars().next().map(Self::get_char_info);
            let fb = cb.chars().next().map(Self::get_char_info);

            let ord = match (fa, fb) {
                (Some(ia), Some(ib)) => ia
                    .radical_rank
                    .cmp(&ib.radical_rank)
                    .then_with(|| ia.strokes.cmp(&ib.strokes))
                    .then_with(|| ca.cmp(cb)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ta.cmp(tb),
            };
            if reverse {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    /// 依正體中文字元筆畫數順序排序。
    pub fn sort_by_stroke<T, F>(items: &mut [T], mut key_fn: F, reverse: bool)
    where
        F: FnMut(&T) -> &str,
    {
        items.sort_by(|a, b| {
            let ta = key_fn(a);
            let tb = key_fn(b);
            let ca = Self::clean_title(ta);
            let cb = Self::clean_title(tb);
            let fa = ca.chars().next().map(Self::get_char_info);
            let fb = cb.chars().next().map(Self::get_char_info);

            let ord = match (fa, fb) {
                (Some(ia), Some(ib)) => ia
                    .strokes
                    .cmp(&ib.strokes)
                    .then_with(|| ia.zhuyin_rank.cmp(&ib.zhuyin_rank))
                    .then_with(|| ca.cmp(cb)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ta.cmp(tb),
            };
            if reverse {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    /// 依漢語拼音英文字母順序（A–Z）排序。
    pub fn sort_by_pinyin<T, F>(items: &mut [T], mut key_fn: F, reverse: bool)
    where
        F: FnMut(&T) -> &str,
    {
        items.sort_by(|a, b| {
            let ta = key_fn(a);
            let tb = key_fn(b);
            let ca = Self::clean_title(ta);
            let cb = Self::clean_title(tb);
            let pa = ca
                .chars()
                .next()
                .map(|c| Self::get_char_info(c).pinyin)
                .unwrap_or("");
            let pb = cb
                .chars()
                .next()
                .map(|c| Self::get_char_info(c).pinyin)
                .unwrap_or("");

            let ord = pa.cmp(pb).then_with(|| ca.cmp(cb)).then_with(|| ta.cmp(tb));
            if reverse {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    /// 依臺灣閩南語羅馬字（臺羅 Tâi-lô / 白話字 POJ）基底字母與聲調順序排序。
    pub fn sort_by_taigi<T, F>(items: &mut [T], mut key_fn: F, reverse: bool)
    where
        F: FnMut(&T) -> &str,
    {
        items.sort_by(|a, b| {
            let ta = key_fn(a);
            let tb = key_fn(b);
            let ca = Self::clean_title(ta);
            let cb = Self::clean_title(tb);
            let na = normalize_taigi_romanization(ca);
            let nb = normalize_taigi_romanization(cb);

            let ord = na.cmp(&nb).then_with(|| ca.cmp(cb));
            if reverse {
                ord.reverse()
            } else {
                ord
            }
        });
    }
}

fn estimate_strokes_by_radical_rank(rank: u16) -> u8 {
    match rank {
        0..=5 => 1,
        6..=28 => 2,
        29..=59 => 3,
        60..=93 => 4,
        94..=116 => 5,
        117..=145 => 6,
        146..=165 => 7,
        166..=174 => 8,
        175..=185 => 9,
        186..=193 => 10,
        194..=199 => 11,
        200..=203 => 12,
        204..=207 => 13,
        208..=209 => 14,
        210 => 15,
        211..=212 => 16,
        _ => 17,
    }
}

/// 將臺羅 (Tâi-lô) 或白話字 (POJ) 帶聲調元音拆解為 `(無調基底小寫字串, 聲調權重序列)`。
pub fn normalize_taigi_romanization(input: &str) -> (String, Vec<u8>) {
    let mut base = String::with_capacity(input.len());
    let mut tones = Vec::new();

    for c in input.chars() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        match lower {
            'á' | 'é' | 'í' | 'ó' | 'ú' | 'ḿ' | 'ń' => {
                base.push(strip_taigi_vowel(lower));
                tones.push(2);
            }
            'à' | 'è' | 'ì' | 'ò' | 'ù' | 'ǹ' => {
                base.push(strip_taigi_vowel(lower));
                tones.push(3);
            }
            'â' | 'ê' | 'î' | 'ô' | 'û' => {
                base.push(strip_taigi_vowel(lower));
                tones.push(5);
            }
            'ǎ' | 'ě' | 'ǐ' | 'ǒ' | 'ǔ' => {
                base.push(strip_taigi_vowel(lower));
                tones.push(6);
            }
            'ā' | 'ē' | 'ī' | 'ō' | 'ū' => {
                base.push(strip_taigi_vowel(lower));
                tones.push(7);
            }
            '\u{0301}' => tones.push(2), // Combining acute
            '\u{0300}' => tones.push(3), // Combining grave
            '\u{0302}' => tones.push(5), // Combining circumflex
            '\u{030c}' => tones.push(6), // Combining caron
            '\u{0304}' => tones.push(7), // Combining macron
            '\u{030d}' => tones.push(8), // Combining vertical line above
            '\u{0358}' => {}             // Combining dot above right (o͘)
            _ => {
                base.push(lower);
            }
        }
    }
    (base, tones)
}

fn strip_taigi_vowel(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ǎ' | 'ā' => 'a',
        'é' | 'è' | 'ê' | 'ě' | 'ē' => 'e',
        'í' | 'ì' | 'î' | 'ǐ' | 'ī' => 'i',
        'ó' | 'ò' | 'ô' | 'ǒ' | 'ō' => 'o',
        'ú' | 'ù' | 'û' | 'ǔ' | 'ū' => 'u',
        'ḿ' => 'm',
        'ń' | 'ǹ' => 'n',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_title_strips_book_marks_and_numbers() {
        assert_eq!(
            CjkSortEngine::clean_title("01. 《臺灣音樂史》"),
            "臺灣音樂史》"
        );
        assert_eq!(CjkSortEngine::clean_title("「光明的道路」"), "光明的道路」");
    }

    #[test]
    fn test_sort_by_zhuyin_and_reverse() {
        // 八 (ㄅ) < 民 (ㄇ) < 大 (ㄉ) < 光 (ㄍ) < 臺 (ㄊ, Wait: ㄉ < ㄊ < ㄍ)
        let mut words = vec!["光明", "大地", "八方", "民眾", "臺灣"];
        CjkSortEngine::sort_by_zhuyin(&mut words, |s| s, false);
        // ㄅ(八方) -> ㄇ(民眾) -> ㄉ(大地) -> ㄊ(臺灣) -> ㄍ(光明)
        assert_eq!(words, vec!["八方", "民眾", "大地", "臺灣", "光明"]);

        CjkSortEngine::sort_by_zhuyin(&mut words, |s| s, true);
        assert_eq!(words, vec!["光明", "臺灣", "大地", "民眾", "八方"]);
    }

    #[test]
    fn test_sort_by_radical_and_stroke() {
        // 一部(一, 1畫) < 口部(合, 6畫) < 木部(林, 8畫) < 水部(海, 10畫) < 龍部(龍, 16畫)
        let mut items = vec!["海洋", "巨龍", "森林", "一統", "合作"];
        // Wait: "巨龍" starts with "巨" (not in table), let's use "龍門"
        items[1] = "龍門";
        items[2] = "林木";
        CjkSortEngine::sort_by_radical(&mut items, |s| s, false);
        assert_eq!(items, vec!["一統", "合作", "林木", "海洋", "龍門"]);

        CjkSortEngine::sort_by_stroke(&mut items, |s| s, false);
        assert_eq!(items, vec!["一統", "合作", "林木", "海洋", "龍門"]);
    }

    #[test]
    fn test_sort_by_taigi_romanization() {
        let mut taigi = vec!["Tâi-uân", "Ta̍k-ke", "Án-ne", "Pe̍h-ōe-jī"];
        CjkSortEngine::sort_by_taigi(&mut taigi, |s| s, false);
        assert_eq!(taigi[0], "Án-ne");
        assert_eq!(taigi[1], "Pe̍h-ōe-jī");
    }
}
