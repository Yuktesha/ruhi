// ruhi-core/src/array30/table.rs — 行列 30 (Array 30) 字根座標映射、一二級簡碼、特別碼、W/HG 符號表與反查教練字典
//
// 依據廖明德先生公開授權之《行列 30 輸入法 Porting 規格書》與 v2026 開源碼表實作：
// 1. 30 個字根鍵位：a-z 及 , . / ; 對應 10 行 (1..9, 0) × 3 列 (^ 上排 1..4、- 中排 5/單筆、v 下排 6..9,0)
// 2. 完整支援：標準全碼 (full)、一級簡碼 (short1)、二級簡碼 (short2)、二鍵特別碼 (special)、W0..W9 與 HG1..HG8 符號表
// 3. 萬用字元查詢：支援 `?`（單一任意鍵）與 `*`（零或多個任意鍵）萬用字元拆碼查詢
// 4. 許氏注音 ➔ 行列 30 即時反查教練：提供漢字之筆形座標（如 18=Z、1-=A）與按鍵拆碼雙向反查

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 行列 30 單一字根鍵位的座標與筆形教學資訊
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Array30KeyMeta {
    /// 對應鍵盤小寫字元 ('a'..='z', ',', '.', '/', ';')
    pub key: char,
    /// 行列座標標示（如 "1^", "1-", "1v", ..., "0v"）
    pub coord: &'static str,
    /// 代表筆形代碼與鍵位對照（如 "18=Z", "1-=A", "98=."）
    pub stroke_badge: &'static str,
    /// 完整筆形座標與代表字根說明（供許氏注音 ➔ 行列 30 教練提示使用）
    pub coach_label: &'static str,
}

/// 30 個標準行列字根鍵位定義表（依第 1 行至第 0 行，上、中、下排排列）
pub const ARRAY30_KEY_METAS: [Array30KeyMeta; 30] = [
    // 第 1 行（首筆 1：一 橫）
    Array30KeyMeta { key: 'q', coord: "1^", stroke_badge: "11=Q", coach_label: "11=Q(1^:厂/亠)" },
    Array30KeyMeta { key: 'a', coord: "1-", stroke_badge: "1-=A", coach_label: "1-=A(1-:一)" },
    Array30KeyMeta { key: 'z', coord: "1v", stroke_badge: "18=Z", coach_label: "18=Z(1v:八/大/雨)" },
    // 第 2 行（首筆 2：└ 逆彎）
    Array30KeyMeta { key: 'w', coord: "2^", stroke_badge: "21=W", coach_label: "21=W(2^:女/巛)" },
    Array30KeyMeta { key: 's', coord: "2-", stroke_badge: "2-=S", coach_label: "2-=S(2-:└/乙)" },
    Array30KeyMeta { key: 'x', coord: "2v", stroke_badge: "28=X", coach_label: "28=X(2v:糸/厶)" },
    // 第 3 行（首筆 3：丨 直）
    Array30KeyMeta { key: 'e', coord: "3^", stroke_badge: "31=E", coach_label: "31=E(3^:門/止/卜)" },
    Array30KeyMeta { key: 'd', coord: "3-", stroke_badge: "3-=D", coach_label: "3-=D(3-:丨/山)" },
    Array30KeyMeta { key: 'c', coord: "3v", stroke_badge: "38=C", coach_label: "38=C(3v:水/小)" },
    // 第 4 行（首筆 4：十 正交）
    Array30KeyMeta { key: 'r', coord: "4^", stroke_badge: "41=R", coach_label: "41=R(4^:土/士/艹)" },
    Array30KeyMeta { key: 'f', coord: "4-", stroke_badge: "4-=F", coach_label: "4-=F(4-:十/廾)" },
    Array30KeyMeta { key: 'v', coord: "4v", stroke_badge: "48=V", coach_label: "48=V(4v:木)" },
    // 第 5 行（首筆 5：┐ 順彎）
    Array30KeyMeta { key: 't', coord: "5^", stroke_badge: "51=T", coach_label: "51=T(5^:阝/尸/己)" },
    Array30KeyMeta { key: 'g', coord: "5-", stroke_badge: "5-=G", coach_label: "5-=G(5-:┐/石/戶)" },
    Array30KeyMeta { key: 'b', coord: "5v", stroke_badge: "58=B", coach_label: "58=B(5v:又/力)" },
    // 第 6 行（首筆 6：丶 點）
    Array30KeyMeta { key: 'y', coord: "6^", stroke_badge: "61=Y", coach_label: "61=Y(6^:言/立/广)" },
    Array30KeyMeta { key: 'h', coord: "6-", stroke_badge: "6-=H", coach_label: "6-=H(6-:丶/方/礻)" },
    Array30KeyMeta { key: 'n', coord: "6v", stroke_badge: "68=N", coach_label: "68=N(6v:心/宀/之)" },
    // 第 7 行（首筆 7：ㄇ 蓋）
    Array30KeyMeta { key: 'u', coord: "7^", stroke_badge: "71=U", coach_label: "71=U(7^:月/皿/冊)" },
    Array30KeyMeta { key: 'j', coord: "7-", stroke_badge: "7-=J", coach_label: "7-=J(7-:ㄇ/目/刀)" },
    Array30KeyMeta { key: 'm', coord: "7v", stroke_badge: "78=M", coach_label: "78=M(7v:貝/夕)" },
    // 第 8 行（首筆 8：八 捺撇）
    Array30KeyMeta { key: 'i', coord: "8^", stroke_badge: "81=I", coach_label: "81=I(8^:金/半/並)" },
    Array30KeyMeta { key: 'k', coord: "8-", stroke_badge: "8-=K", coach_label: "8-=K(8-:人/入/八)" },
    Array30KeyMeta { key: ',', coord: "8v", stroke_badge: "88=,", coach_label: "88=,(8v:火/米)" },
    // 第 9 行（首筆 9：丿 撇）
    Array30KeyMeta { key: 'o', coord: "9^", stroke_badge: "91=O", coach_label: "91=O(9^:手/斤/生)" },
    Array30KeyMeta { key: 'l', coord: "9-", stroke_badge: "9-=L", coach_label: "9-=L(9-:丿/竹/禾)" },
    Array30KeyMeta { key: '.', coord: "9v", stroke_badge: "98=.", coach_label: "98=.(9v:彳/身/犭)" },
    // 第 0 行（首筆 0：口 方框）
    Array30KeyMeta { key: 'p', coord: "0^", stroke_badge: "01=P", coach_label: "01=P(0^:日/田)" },
    Array30KeyMeta { key: ';', coord: "0-", stroke_badge: "0-=;", coach_label: "0-=;(0-:口/囗)" },
    Array30KeyMeta { key: '/', coord: "0v", stroke_badge: "08=/", coach_label: "08=/(0v:四/虫)" },
];

/// 判斷是否為行列 30 的 30 個標準字根鍵位 ('a'..='z', ',', '.', '/', ';')
pub fn is_array30_radical_key(c: char) -> bool {
    let lower = c.to_ascii_lowercase();
    lower.is_ascii_lowercase() || matches!(lower, ',' | '.' | '/' | ';')
}

/// 取得單一按鍵之行列 30 元資料
pub fn get_key_meta(c: char) -> Option<&'static Array30KeyMeta> {
    let lower = c.to_ascii_lowercase();
    ARRAY30_KEY_METAS.iter().find(|m| m.key == lower)
}

/// 將按鍵轉換為行列座標標示（如 'a' -> "1-", 'z' -> "1v", ';' -> "0-"）
pub fn key_to_coord(c: char) -> Option<&'static str> {
    get_key_meta(c).map(|m| m.coord)
}

/// 將行列按鍵序列轉換為標準行列座標顯示字串（如 "zps" -> "1v 0^ 2-"）
/// 支援萬用字元 `?`、`*` 以及 `w0`..`w9`、`hg1`..`hg8` 符號表標示
pub fn code_to_coords_display(code: &str) -> String {
    let lower = code.to_ascii_lowercase();
    if lower.len() == 2 && lower.starts_with('w') && lower.as_bytes()[1].is_ascii_digit() {
        let d = lower.chars().nth(1).unwrap_or('1');
        let cat = match d {
            '1' => "標點",
            '2' => "括號",
            '3' => "一般符號",
            '4' => "數學",
            '5' => "方向",
            '6' => "單位",
            '7' => "圖表",
            '8' => "順序",
            '9' => "希臘",
            '0' => "注音",
            _ => "符號",
        };
        return format!("2^{} [{}]", d, cat);
    }
    if lower.len() == 3 && lower.starts_with("hg") && lower.as_bytes()[2].is_ascii_digit() {
        let d = lower.chars().nth(2).unwrap_or('1');
        return format!("6- 5- {} [擴充符號]", d);
    }

    let mut parts = Vec::with_capacity(lower.len());
    for c in lower.chars() {
        if let Some(coord) = key_to_coord(c) {
            parts.push(coord.to_string());
        } else if c == '?' || c == '*' {
            parts.push(c.to_string());
        } else {
            parts.push(c.to_string());
        }
    }
    parts.join(" ")
}

#[derive(Debug, Clone)]
pub struct Array30Entry {
    pub ch: String,
    pub weight: i32,
    pub is_special: bool,
}

/// 單一漢字的行列 30 反查資訊（供許氏注音 ➔ 行列 30 即時教練使用）
#[derive(Debug, Clone, Default)]
pub struct Array30ReverseEntry {
    /// 標準全碼列表（依常用與簡短度排序，例如「大」-> ["z"], 「行」-> [".aad"]）
    pub full_codes: Vec<String>,
    /// 二鍵特別碼（例如「大」-> Some("ak"), 「是」-> Some("pc")）
    pub special_code: Option<String>,
    /// 一級簡碼與對應數字鍵 1..=10（10 代表按鍵 0，例如「大」-> Some(("z", 2))）
    pub short1: Option<(String, u8)>,
    /// 二級簡碼與對應數字鍵 1..=10（例如「明」-> Some(("pu", 1))）
    pub short2: Option<(String, u8)>,
}

#[derive(Debug, Clone)]
pub struct Array30Table {
    /// 全碼與特別碼表：code -> Vec<Array30Entry> (按 weight 降序排列)
    table: HashMap<String, Vec<Array30Entry>>,
    /// 一級簡碼表：1碼 -> Vec<(slot 1..=10, char)>
    short1_table: HashMap<String, Vec<(u8, String)>>,
    /// 二級簡碼表：2碼 -> Vec<(slot 1..=10, char)>
    short2_table: HashMap<String, Vec<(u8, String)>>,
    /// W0..W9 與 HG1..HG8 符號表：code -> Vec<symbol>
    symbol_table: HashMap<String, Vec<String>>,
    /// 漢字反查索引：char -> Array30ReverseEntry
    reverse_table: HashMap<char, Array30ReverseEntry>,
}

impl Array30Table {
    pub fn new() -> Self {
        let mut t = Self {
            table: HashMap::new(),
            short1_table: HashMap::new(),
            short2_table: HashMap::new(),
            symbol_table: HashMap::new(),
            reverse_table: HashMap::new(),
        };
        t.load_builtin_defaults();
        t
    }

    /// 載入內建核心常用字、一二級簡碼、特別碼與 W 符號表預設值（確保無外部檔案時單元測試與基礎輸入 100% 運作）
    fn load_builtin_defaults(&mut self) {
        // 1. 核心一級簡碼 (short1)
        let builtin_short1: &[(&str, u8, &str)] = &[
            ("a", 1, "一"), ("a", 2, "到"), ("a", 3, "聽"), ("a", 4, "現"), ("a", 5, "政"),
            ("a", 6, "弄"), ("a", 7, "兩"), ("a", 8, "而"), ("a", 9, "面"), ("a", 10, "要"),
            ("b", 1, "又"), ("b", 2, "力"), ("b", 5, "發"), ("b", 7, "通"), ("b", 8, "習"),
            ("c", 1, "小"), ("c", 2, "卜"), ("c", 3, "水"), ("c", 4, "法"), ("c", 5, "決"),
            ("c", 6, "注"), ("c", 7, "當"), ("c", 8, "對"), ("c", 9, "省"), ("c", 10, "常"),
            ("d", 1, "山"), ("d", 2, "片"),
            ("e", 1, "門"), ("e", 2, "止"), ("e", 4, "開"), ("e", 5, "關"),
            ("f", 1, "十"), ("f", 3, "落"), ("f", 4, "著"), ("f", 5, "華"), ("f", 7, "真"),
            ("g", 1, "石"), ("g", 2, "戶"), ("g", 3, "也"), ("g", 4, "那"),
            ("h", 1, "方"), ("h", 5, "良"), ("h", 8, "於"), ("h", 9, "為"),
            ("i", 1, "金"), ("i", 2, "半"), ("i", 5, "食"), ("i", 7, "前"), ("i", 8, "美"), ("i", 9, "道"), ("i", 10, "會"),
            ("j", 1, "目"), ("j", 2, "刀"), ("j", 5, "眼"), ("j", 6, "運"), ("j", 7, "解"),
            ("k", 1, "人"), ("k", 2, "入"), ("k", 3, "八"), ("k", 4, "做"), ("k", 5, "他"),
            ("k", 6, "進"), ("k", 7, "你"), ("k", 8, "坐"), ("k", 9, "作"), ("k", 10, "個"),
            ("l", 1, "竹"), ("l", 2, "看"), ("l", 3, "師"), ("l", 4, "和"), ("l", 5, "第"),
            ("l", 6, "種"), ("l", 7, "向"), ("l", 8, "答"), ("l", 9, "我"), ("l", 10, "的"),
            ("m", 1, "貝"), ("m", 2, "夕"), ("m", 7, "體"), ("m", 9, "然"), ("m", 10, "過"),
            ("n", 1, "之"), ("n", 2, "心"), ("n", 3, "定"), ("n", 4, "麼"), ("n", 5, "字"),
            ("n", 7, "家"), ("n", 8, "應"), ("n", 9, "寫"), ("n", 10, "空"),
            ("o", 1, "手"), ("o", 2, "斤"), ("o", 4, "無"), ("o", 5, "把"), ("o", 6, "接"),
            ("o", 7, "興"), ("o", 8, "推"), ("o", 9, "學"), ("o", 10, "動"),
            ("p", 1, "日"), ("p", 2, "曰"), ("p", 3, "田"), ("p", 4, "時"), ("p", 5, "最"),
            ("p", 6, "是"), ("p", 7, "照"), ("p", 8, "點"), ("p", 9, "易"), ("p", 10, "國"),
            ("q", 1, "工"), ("q", 2, "七"), ("q", 3, "車"), ("q", 5, "事"), ("q", 8, "頭"), ("q", 9, "或"), ("q", 10, "區"),
            ("r", 1, "土"), ("r", 2, "士"), ("r", 4, "起"), ("r", 5, "地"), ("r", 6, "老"), ("r", 8, "報"), ("r", 9, "都"), ("r", 10, "臺"),
            ("s", 1, "乙"), ("s", 3, "收"), ("s", 9, "路"),
            ("t", 1, "隨"), ("t", 2, "民"), ("t", 4, "書"), ("t", 5, "張"),
            ("u", 1, "月"), ("u", 2, "皿"), ("u", 4, "腦"), ("u", 5, "助"),
            ("v", 1, "木"), ("v", 2, "機"), ("v", 3, "極"), ("v", 5, "根"), ("v", 6, "校"), ("v", 7, "想"), ("v", 8, "來"), ("v", 9, "格"), ("v", 10, "查"),
            ("w", 1, "女"),
            ("x", 1, "風"), ("x", 2, "幾"), ("x", 3, "經"), ("x", 4, "結"), ("x", 7, "能"), ("x", 8, "給"), ("x", 9, "總"),
            ("y", 1, "立"), ("y", 2, "言"), ("y", 3, "裡"), ("y", 4, "新"), ("y", 5, "記"), ("y", 6, "該"), ("y", 7, "認"), ("y", 8, "說"), ("y", 9, "話"), ("y", 10, "就"),
            ("z", 1, "不"), ("z", 2, "大"), ("z", 3, "夫"), ("z", 4, "雨"), ("z", 5, "成"), ("z", 6, "在"), ("z", 7, "布"), ("z", 8, "願"), ("z", 9, "原"), ("z", 10, "電"),
            (",", 1, "，"), (",", 2, "火"), (",", 3, "米"), (",", 4, "精"), (",", 5, "燈"), (",", 6, "料"), (",", 9, "類"), (",", 10, "營"),
            (".", 1, "。"), (".", 2, "身"), (".", 3, "行"), (".", 4, "街"), (".", 5, "很"), (".", 6, "往"), (".", 7, "愛"), (".", 8, "從"), (".", 9, "後"), (".", 10, "得"),
            ("/", 1, "四"), ("/", 2, "虫"), ("/", 3, "？"), ("/", 4, "『"), ("/", 5, "』"),
            (";", 1, "口"), (";", 2, "："), (";", 3, "；"), (";", 4, "叫"), (";", 5, "呢"), (";", 6, "嗎"), (";", 8, "別"), (";", 9, "吃"), (";", 10, "號"),
        ];
        for &(code, slot, ch) in builtin_short1 {
            self.insert_short1(code, slot, ch);
        }

        // 2. 核心二級簡碼 (short2)
        let builtin_short2: &[(&str, u8, &str)] = &[
            ("pu", 1, "明"),
            ("w;", 1, "如"),
            ("r;", 8, "喜"),
            ("qi", 7, "輸"),
            ("le", 1, "列"),
        ];
        for &(code, slot, ch) in builtin_short2 {
            self.insert_short2(code, slot, ch);
        }

        // 3. 核心特別碼 (special)
        let builtin_special: &[(&str, &str)] = &[
            ("ak", "大"),
            ("ah", "不"),
            ("dk", "小"),
            ("pc", "是"),
            ("jj", "再"),
            (";;", "個"),
            ("lo", "我"),
            ("dr", "在"),
            ("/;", "喜"),
        ];
        for &(code, ch) in builtin_special {
            self.insert_full_or_special(code, ch, 2100, true);
        }

        // 4. 核心標準全碼 (full)
        let builtin_full: &[(&str, &str, i32)] = &[
            ("a", "一", 1750),
            ("aa", "二", 1750),
            ("aaa", "三", 1750),
            ("b", "又", 1750),
            ("c", "小", 1750),
            ("c", "卜", 1738),
            ("c", "水", 1726),
            ("d", "山", 1750),
            ("e", "門", 1750),
            ("e", "止", 1738),
            ("f", "十", 1750),
            ("g", "石", 1750),
            ("h", "方", 1750),
            ("i", "金", 1750),
            ("i", "半", 1738),
            ("if", "針", 1750),
            ("if", "羊", 1738),
            ("if", "并", 1726),
            ("j", "目", 1750),
            ("j", "刀", 1738),
            ("k", "人", 1750),
            ("k", "入", 1738),
            ("k", "八", 1726),
            ("l", "竹", 1750),
            ("m", "貝", 1750),
            ("n", "之", 1750),
            ("n", "心", 1738),
            ("o", "手", 1750),
            ("p", "日", 1750),
            ("p", "曰", 1738),
            ("p", "田", 1726),
            ("q", "工", 1750),
            ("r", "土", 1750),
            ("r", "士", 1738),
            ("s", "乙", 1750),
            ("t", "的", 1750),
            ("u", "月", 1750),
            ("v", "木", 1750),
            ("w", "女", 1750),
            ("x", "風", 1750),
            ("y", "立", 1750),
            ("y", "言", 1738),
            ("z", "不", 1750),
            ("z", "大", 1738),
            ("z", "夫", 1726),
            ("z", "雨", 1714),
            (",", "，", 1750),
            (".", "。", 1750),
            (";", "口", 1750),
            (";", "：", 1738),
            (";", "；", 1726),
            ("pu", "明", 1750),
            ("w;", "如", 1750),
            ("r;f;", "喜", 1750),
            ("qiue", "輸", 1750),
            ("crx", "法", 1750),
            (".aad", "行", 1750),
            ("aqje", "列", 1750),
            ("lox", "我", 1750),
            ("lplh", "的", 1750),
            ("pac", "是", 1750),
            ("zps", "電", 1750),
            ("fai", "㐀", 1650),
        ];
        for &(code, ch, w) in builtin_full {
            self.insert_full_or_special(code, ch, w, false);
        }

        // 5. 核心 W 鍵符號表 (w0..w9)
        let builtin_symbols: &[(&str, &[&str])] = &[
            ("w1", &["，", "、", "。", "．", "‧", "；", "：", "？", "！", "︰"]),
            ("w2", &["（", "）", "︵", "︶", "｛", "｝", "〔", "〕", "【", "】", "《", "》", "〈", "〉", "「", "」", "『", "』"]),
            ("w3", &["＃", "＆", "＊", "※", "§", "〃", "○", "●", "△", "▲", "☆", "★", "◇", "◆", "□", "■"]),
            ("w4", &["＋", "－", "×", "÷", "±", "√", "＜", "＞", "＝", "≦", "≧", "≠", "∞", "≒", "≡"]),
            ("w5", &["♀", "♂", "⊕", "⊙", "↑", "↓", "←", "→", "↖", "↗", "↙", "↘", "∥", "∣", "／", "＼"]),
            ("w6", &["＄", "￥", "€", "￠", "￡", "％", "＠", "℃", "℉", "㎜", "㎝", "㎞", "㎏", "㎡", "㏄"]),
            ("w7", &["─", "│", "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼", "═", "║", "╔", "╗", "╚", "╝"]),
            ("w8", &["①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧", "⑨", "⑩", "Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ"]),
            ("w9", &["Α", "Β", "Γ", "Δ", "Ε", "Ζ", "Η", "Θ", "Ι", "Κ", "α", "β", "γ", "δ", "ε"]),
            ("w0", &["ㄅ", "ㄆ", "ㄇ", "ㄈ", "ㄉ", "ㄊ", "ㄋ", "ㄌ", "ㄍ", "ㄎ", "ㄏ", "ㄐ", "ㄑ", "ㄒ"]),
        ];
        for &(code, syms) in builtin_symbols {
            for &sym in syms {
                self.insert_symbol(code, sym);
            }
        }
    }

    fn insert_short1(&mut self, code: &str, slot: u8, ch: &str) {
        let key = code.to_ascii_lowercase();
        let list = self.short1_table.entry(key.clone()).or_default();
        if !list.iter().any(|(s, c)| *s == slot && c == ch) {
            list.push((slot, ch.to_string()));
            list.sort_by_key(|(s, _)| *s);
        }
        if ch.chars().count() == 1 {
            if let Some(c) = ch.chars().next() {
                let rev = self.reverse_table.entry(c).or_default();
                if rev.short1.is_none() {
                    rev.short1 = Some((key, slot));
                }
            }
        }
    }

    fn insert_short2(&mut self, code: &str, slot: u8, ch: &str) {
        let key = code.to_ascii_lowercase();
        let list = self.short2_table.entry(key.clone()).or_default();
        if !list.iter().any(|(s, c)| *s == slot && c == ch) {
            list.push((slot, ch.to_string()));
            list.sort_by_key(|(s, _)| *s);
        }
        if ch.chars().count() == 1 {
            if let Some(c) = ch.chars().next() {
                let rev = self.reverse_table.entry(c).or_default();
                if rev.short2.is_none() {
                    rev.short2 = Some((key, slot));
                }
            }
        }
    }

    fn insert_symbol(&mut self, code: &str, sym: &str) {
        let key = code.to_ascii_lowercase();
        let list = self.symbol_table.entry(key).or_default();
        if !list.iter().any(|s| s == sym) {
            list.push(sym.to_string());
        }
    }

    fn insert_full_or_special(&mut self, code: &str, ch: &str, weight: i32, is_special: bool) {
        let key = code.to_ascii_lowercase();
        let list = self.table.entry(key.clone()).or_default();
        if let Some(existing) = list.iter_mut().find(|e| e.ch == ch) {
            if weight > existing.weight {
                existing.weight = weight;
            }
            if is_special {
                existing.is_special = true;
            }
        } else {
            list.push(Array30Entry {
                ch: ch.to_string(),
                weight,
                is_special,
            });
        }

        if ch.chars().count() == 1 {
            if let Some(c) = ch.chars().next() {
                let rev = self.reverse_table.entry(c).or_default();
                if is_special {
                    if rev.special_code.is_none() {
                        rev.special_code = Some(key);
                    }
                } else if !rev.full_codes.contains(&key) {
                    rev.full_codes.push(key);
                }
            }
        }
    }

    /// 從外部 TSV 檔案載入行列 30 碼表 (`CODE\tCHAR\tWEIGHT[\tKIND]`)
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        let file = File::open(path.as_ref()).map_err(|e| format!("無法開啟行列 30 碼表: {}", e))?;
        let reader = BufReader::new(file);
        let mut count = 0usize;

        // 若載入完整外部 TSV，清空舊符號表與簡碼表以採用最新完整規格
        self.table.clear();
        self.short1_table.clear();
        self.short2_table.clear();
        self.symbol_table.clear();
        self.reverse_table.clear();

        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("CODE") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let code = parts[0].trim().to_ascii_lowercase();
                let ch = parts[1].trim();
                let weight: i32 = parts.get(2).and_then(|w| w.trim().parse().ok()).unwrap_or(1000);
                let kind = parts.get(3).map(|k| k.trim()).unwrap_or("full");

                if code.is_empty() || ch.is_empty() {
                    continue;
                }

                if let Some(slot_str) = kind.strip_prefix("short1:") {
                    let slot: u8 = slot_str.parse().unwrap_or(1);
                    self.insert_short1(&code, slot, ch);
                } else if let Some(slot_str) = kind.strip_prefix("short2:") {
                    let slot: u8 = slot_str.parse().unwrap_or(1);
                    self.insert_short2(&code, slot, ch);
                } else if kind.starts_with("symbol")
                    || (code.len() == 2 && code.starts_with('w') && code.as_bytes()[1].is_ascii_digit())
                    || (code.len() == 3 && code.starts_with("hg") && code.as_bytes()[2].is_ascii_digit())
                {
                    self.insert_symbol(&code, ch);
                } else if kind == "special" {
                    self.insert_full_or_special(&code, ch, weight, true);
                } else {
                    self.insert_full_or_special(&code, ch, weight, false);
                }
                count += 1;
            }
        }

        for list in self.table.values_mut() {
            list.sort_by(|a, b| b.weight.cmp(&a.weight));
        }

        // 整理反查表中每個漢字的全碼順序：標準非 i 結尾短碼優先，再依碼長排序
        for rev in self.reverse_table.values_mut() {
            rev.full_codes.sort_by(|a, b| {
                let a_rare = a.len() >= 3 && a.ends_with('i');
                let b_rare = b.len() >= 3 && b.ends_with('i');
                a_rare.cmp(&b_rare).then_with(|| a.len().cmp(&b.len()))
            });
        }

        Ok(count)
    }

    /// 查詢指定字根碼對應的「空白鍵上屏候選字清單」（僅限全碼與特別碼，依權重降序）
    pub fn lookup(&self, code: &str) -> Vec<String> {
        let lower = code.to_ascii_lowercase();
        if let Some(syms) = self.symbol_table.get(&lower) {
            return syms.clone();
        }
        if lower.contains('?') || lower.contains('*') {
            return self.lookup_wildcard(&lower);
        }
        if let Some(entries) = self.table.get(&lower) {
            let mut list: Vec<&Array30Entry> = entries.iter().collect();
            list.sort_by(|a, b| b.weight.cmp(&a.weight));
            list.into_iter().map(|e| e.ch.clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// 查詢一級簡碼清單（依 1..=10 定位槽位排序）
    pub fn lookup_short1(&self, code: &str) -> Vec<String> {
        let lower = code.to_ascii_lowercase();
        self.short1_table
            .get(&lower)
            .map(|list| list.iter().map(|(_, ch)| ch.clone()).collect())
            .unwrap_or_default()
    }

    /// 查詢二級簡碼清單（依 1..=10 定位槽位排序）
    pub fn lookup_short2(&self, code: &str) -> Vec<String> {
        let lower = code.to_ascii_lowercase();
        self.short2_table
            .get(&lower)
            .map(|list| list.iter().map(|(_, ch)| ch.clone()).collect())
            .unwrap_or_default()
    }

    /// 查詢 W0..W9 或 HG1..HG8 符號表
    pub fn lookup_symbols(&self, code: &str) -> Vec<String> {
        let lower = code.to_ascii_lowercase();
        self.symbol_table.get(&lower).cloned().unwrap_or_default()
    }

    /// 組字過程中的綜合即時預覽候選清單：
    /// - 若為 `w0`..`w9` 或 `hg1`..`hg8`：直接回傳符號清單
    /// - 若包含 `?` 或 `*`：回傳萬用字元匹配結果
    /// - 若為 1 碼：將該鍵的「空白鍵首選全碼字（如 `t` -> `的`、`a` -> `一`）」與「一級簡碼 10 字」合併呈現
    /// - 若為 2 碼：將「特別碼/全碼」置頂，後接「二級簡碼」與字首聯想
    /// - 若為 3~5 碼：將「精確全碼」置頂，後接字首聯想
    pub fn lookup_compose_candidates(&self, code: &str, max_results: usize) -> Vec<String> {
        let lower = code.to_ascii_lowercase();
        if let Some(syms) = self.symbol_table.get(&lower) {
            return syms.clone();
        }
        if lower.contains('?') || lower.contains('*') {
            return self.lookup_wildcard(&lower);
        }

        let mut results = Vec::new();

        if lower.len() == 1 {
            // 1. 先放入全碼精確匹配的首字（確保按空白鍵上屏的首字在第一位，例如 a->一, t->的, w->女）
            for ch in self.lookup(&lower) {
                if !results.contains(&ch) {
                    results.push(ch);
                }
            }
            // 2. 放入一級簡碼
            for ch in self.lookup_short1(&lower) {
                if !results.contains(&ch) {
                    results.push(ch);
                }
            }
        } else if lower.len() == 2 {
            // 1. 精確二鍵全碼與特別碼優先（空白鍵直出目標）
            for ch in self.lookup(&lower) {
                if !results.contains(&ch) {
                    results.push(ch);
                }
            }
            // 2. 二級簡碼
            for ch in self.lookup_short2(&lower) {
                if !results.contains(&ch) {
                    results.push(ch);
                }
            }
            // 3. 前綴聯想
            for ch in self.lookup_with_prefix(&lower, max_results) {
                if !results.contains(&ch) {
                    results.push(ch);
                    if results.len() >= max_results {
                        break;
                    }
                }
            }
        } else {
            for ch in self.lookup_with_prefix(&lower, max_results) {
                if !results.contains(&ch) {
                    results.push(ch);
                    if results.len() >= max_results {
                        break;
                    }
                }
            }
        }

        results
    }

    /// 前綴聯想查詢（先精確全碼匹配，再按碼長由短到長、權重由高到低補充）
    pub fn lookup_with_prefix(&self, prefix: &str, max_results: usize) -> Vec<String> {
        let lower = prefix.to_ascii_lowercase();
        let mut results = self.lookup(&lower);
        if results.len() >= max_results {
            results.truncate(max_results);
            return results;
        }

        let mut prefix_matches: Vec<(usize, i32, &str)> = Vec::new();
        for (code, entries) in &self.table {
            if code.len() > lower.len() && code.starts_with(&lower) {
                for e in entries {
                    prefix_matches.push((code.len(), e.weight, e.ch.as_str()));
                }
            }
        }
        prefix_matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));

        for (_, _, ch) in prefix_matches {
            if !results.iter().any(|r| r == ch) {
                results.push(ch.to_string());
                if results.len() >= max_results {
                    break;
                }
            }
        }
        results
    }

    /// 萬用字元查詢：支援 `?`（匹配任意單一字根鍵）與 `*`（匹配零或多個任意字根鍵）
    pub fn lookup_wildcard(&self, pattern: &str) -> Vec<String> {
        let pat = pattern.to_ascii_lowercase();
        let mut matched: Vec<(usize, i32, &str)> = Vec::new();

        for (code, entries) in &self.table {
            if Self::wildcard_match(&pat, code) {
                for e in entries {
                    matched.push((code.len(), e.weight, e.ch.as_str()));
                }
            }
        }

        matched.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));

        let mut results = Vec::new();
        for (_, _, ch) in matched {
            if !results.iter().any(|r| r == ch) {
                results.push(ch.to_string());
                if results.len() >= 50 {
                    break;
                }
            }
        }
        results
    }

    fn wildcard_match(pattern: &str, text: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let t: Vec<char> = text.chars().collect();
        let (m, n) = (p.len(), t.len());
        let mut dp = vec![vec![false; n + 1]; m + 1];
        dp[0][0] = true;

        for i in 1..=m {
            if p[i - 1] == '*' {
                dp[i][0] = dp[i - 1][0];
            }
        }

        for i in 1..=m {
            for j in 1..=n {
                if p[i - 1] == '*' {
                    dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
                } else if p[i - 1] == '?' || p[i - 1] == t[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1];
                }
            }
        }
        dp[m][n]
    }

    /// 多字詞首碼快打 (Phrase Head-Code Lookup)：
    /// 當使用者輸入 2~4 碼且無單字精確全碼時，比對各字行列首碼組成的常用詞
    pub fn lookup_phrase_headcode(
        &self,
        code: &str,
        lexicon: &crate::lexicon::LexiconManager,
    ) -> Vec<String> {
        let lower = code.to_ascii_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        if chars.len() < 2 || chars.len() > 4 {
            return Vec::new();
        }
        if chars.iter().any(|&c| !is_array30_radical_key(c)) {
            return Vec::new();
        }

        let mut matched_phrases: Vec<(String, u32)> = Vec::new();
        for entry in &lexicon.entries {
            let word_chars: Vec<char> = entry.word.chars().collect();
            if word_chars.len() != chars.len() {
                continue;
            }
            let mut all_match = true;
            for (wc, &target_head) in word_chars.iter().zip(chars.iter()) {
                if !self.char_starts_with_key(*wc, target_head) {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                matched_phrases.push((entry.word.clone(), entry.weight));
            }
        }

        matched_phrases.sort_by(|a, b| b.1.cmp(&a.1));
        matched_phrases.dedup_by(|a, b| a.0 == b.0);
        matched_phrases.into_iter().take(5).map(|(w, _)| w).collect()
    }

    fn char_starts_with_key(&self, ch: char, head_key: char) -> bool {
        if let Some(rev) = self.reverse_table.get(&ch) {
            if rev.full_codes.iter().any(|c| c.starts_with(head_key)) {
                return true;
            }
            if let Some(sp) = &rev.special_code {
                if sp.starts_with(head_key) {
                    return true;
                }
            }
            if let Some((s1, _)) = &rev.short1 {
                if s1.starts_with(head_key) {
                    return true;
                }
            }
        }
        false
    }

    /// 反查單一漢字的行列 30 完整編碼結構
    pub fn reverse_lookup(&self, ch: char) -> Option<&Array30ReverseEntry> {
        self.reverse_table.get(&ch)
    }

    /// 為「許氏注音 ➔ 行列 30 即時反查教練模式」格式化單一漢字的筆形座標與拆碼提示
    /// 範例輸出：
    /// - `大`: `大 [18=Z(1v:八/大/雨)] · 特:AK(1- 8-) · 簡1:Z2`
    /// - `行`: `行 [.AAD ➔ 98=.(9v:彳/身/犭) + 1-=A(1-:一) + 1-=A(1-:一) + 3-=D(3-:丨/山)] · 簡1:.3`
    pub fn format_char_coach(&self, ch: char) -> Option<String> {
        let rev = self.reverse_table.get(&ch)?;
        let primary_code = rev
            .full_codes
            .first()
            .or(rev.special_code.as_ref())
            .or_else(|| rev.short1.as_ref().map(|(c, _)| c))
            .or_else(|| rev.short2.as_ref().map(|(c, _)| c))?;

        let upper_code = primary_code.to_ascii_uppercase();
        let stroke_steps: Vec<&str> = primary_code
            .chars()
            .filter_map(|k| get_key_meta(k).map(|m| m.coach_label))
            .collect();

        let breakdown = if stroke_steps.len() == 1 {
            format!("{} [{}]", ch, stroke_steps[0])
        } else {
            format!("{} [{} ➔ {}]", ch, upper_code, stroke_steps.join(" + "))
        };

        let mut extras = Vec::new();
        if let Some(sp) = &rev.special_code {
            if sp != primary_code {
                extras.push(format!("特:{}({})", sp.to_ascii_uppercase(), code_to_coords_display(sp)));
            }
        }
        if let Some((s1, slot)) = &rev.short1 {
            let slot_disp = if *slot == 10 { 0 } else { *slot };
            extras.push(format!("簡1:{}{}", s1.to_ascii_uppercase(), slot_disp));
        } else if let Some((s2, slot)) = &rev.short2 {
            let slot_disp = if *slot == 10 { 0 } else { *slot };
            extras.push(format!("簡2:{}{}", s2.to_ascii_uppercase(), slot_disp));
        }

        if extras.is_empty() {
            Some(breakdown)
        } else {
            Some(format!("{} · {}", breakdown, extras.join(" · ")))
        }
    }

    /// 為單一漢字產生精簡版候選字尾註（適合嵌入選字清單或多字詞教練列，例如 `Z(18=Z·特AK)`）
    pub fn format_char_compact_coach(&self, ch: char) -> Option<String> {
        let rev = self.reverse_table.get(&ch)?;
        let primary_code = rev
            .full_codes
            .first()
            .or(rev.special_code.as_ref())?;
        let badges: Vec<&str> = primary_code
            .chars()
            .filter_map(|k| get_key_meta(k).map(|m| m.stroke_badge))
            .collect();
        let mut s = format!("{}:{}[{}]", ch, primary_code.to_ascii_uppercase(), badges.join(" "));
        if let Some(sp) = &rev.special_code {
            if sp != primary_code {
                s.push_str(&format!("·特{}", sp.to_ascii_uppercase()));
            }
        } else if let Some((s1, slot)) = &rev.short1 {
            let slot_disp = if *slot == 10 { 0 } else { *slot };
            s.push_str(&format!("·簡{}{}", s1.to_ascii_uppercase(), slot_disp));
        }
        Some(s)
    }

    /// 為「許氏注音 ➔ 行列 30 即時反查教練模式」格式化單字或詞句的教練提示：
    /// - 若為單一漢字：顯示完整筆形座標（如 `18=Z(1v:八/大/雨)`）、全碼、特別碼與一二級簡碼
    /// - 若為多字詞句：優先針對最後 1~3 個關鍵漢字顯示完整筆形座標與拆碼對照
    pub fn format_text_coach(&self, text: &str) -> String {
        let cjk_chars: Vec<char> = text
            .chars()
            .filter(|&c| self.reverse_table.contains_key(&c))
            .collect();
        if cjk_chars.is_empty() {
            return String::new();
        }
        if cjk_chars.len() == 1 {
            return self.format_char_coach(cjk_chars[0]).unwrap_or_default();
        }

        // 若為 2 字詞：第一字顯示精簡座標，最後一字（剛打出的字）顯示完整筆形分解與簡碼
        if cjk_chars.len() == 2 {
            let first = self.format_char_compact_coach(cjk_chars[0]).unwrap_or_default();
            let second = self.format_char_coach(cjk_chars[1]).unwrap_or_default();
            return format!("{} ｜ {}", first, second);
        }

        // 3 字以上長句：列出最後 3 字的精簡筆形座標 + 最後 1 字完整拆解
        let tail_start = cjk_chars.len().saturating_sub(3);
        let mut parts = Vec::new();
        for &ch in &cjk_chars[tail_start..cjk_chars.len() - 1] {
            if let Some(compact) = self.format_char_compact_coach(ch) {
                parts.push(compact);
            }
        }
        if let Some(&last_ch) = cjk_chars.last() {
            if let Some(full) = self.format_char_coach(last_ch) {
                parts.push(full);
            }
        }
        parts.join(" ｜ ")
    }
}
