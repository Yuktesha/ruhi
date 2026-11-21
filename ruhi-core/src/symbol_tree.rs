/// RuhI (如喜) Input Method Framework - Rust Core
/// src/symbol_tree.rs - 9×9×9 多層分類樹符號系統、語意檢索與製表框線駐留連打引擎

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolCategoryMode {
    Normal,
    Wrap,
    Box,
}

#[derive(Debug, Clone)]
pub struct SymbolSubCategory {
    pub name: String,
    pub symbols: Vec<String>,
    pub keywords: Vec<String>,
    pub mode: SymbolCategoryMode,
}

#[derive(Debug, Clone)]
pub struct SymbolMajorCategory {
    pub name: String,
    pub keywords: Vec<String>,
    pub subs: Vec<SymbolSubCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolTabTarget {
    OpenTree {
        major_idx: Option<usize>,
        sub_idx: Option<usize>,
    },
    Candidates(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolTreeStep {
    Stay,
    CommitSymbol { symbol: String, is_wrap_pair: bool },
    InsertStickyBox { symbol: String, is_first: bool },
    DeletePrevStickyBox,
    Exit,
}

const PAIRED_WRAP_SYMBOLS: &[&str] = &[
    "「」", "『』", "“”", "‘’", "〝〞", "（）", "【】", "〔〕", "《》", "〈〉",
    "〖〗", "［］", "｛｝", "︵︶", "︷︸", "︹︺", "︻︼", "︽︾", "︿﹀", "﹁﹂",
    "﹃﹄",
];

const BOX_STYLES: [(&str, [&str; 11]); 3] = [
    (
        "細線",
        ["┌", "┬", "┐", "├", "┼", "┤", "└", "┴", "┘", "─", "│"],
    ),
    (
        "粗線",
        ["┏", "┳", "┓", "┣", "╋", "┫", "┗", "┻", "┛", "━", "┃"],
    ),
    (
        "雙線",
        ["╔", "╦", "╗", "╠", "╬", "╣", "╚", "╩", "╝", "═", "║"],
    ),
];

const BOX_KEY_HINTS: [&str; 11] = [
    "(7)", "(8)", "(9)", "(4)", "(5)", "(6)", "(1)", "(2)", "(3)", "(-/0)", "(|/.)",
];

const SYMBOL_TREE_RELOAD_THROTTLE_MS: u64 = 2000;
const MRU_CAP: usize = 9;
const SESSION_MAP_CAP: usize = 128;

#[derive(Debug, Clone)]
pub struct SymbolTreeStore {
    pub majors: Vec<SymbolMajorCategory>,
    file_path: Option<PathBuf>,
    last_modified: Option<SystemTime>,
    last_check: Option<Instant>,
    all_leaf_symbols: HashSet<String>,
    paired_wrap_symbols: HashSet<String>,
}

impl Default for SymbolTreeStore {
    fn default() -> Self {
        let mut store = Self {
            majors: Vec::new(),
            file_path: None,
            last_modified: None,
            last_check: None,
            all_leaf_symbols: HashSet::new(),
            paired_wrap_symbols: HashSet::new(),
        };
        store.init_builtin_tree();
        store
    }
}

impl SymbolTreeStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn rebuild_symbol_indexes(&mut self) {
        self.all_leaf_symbols.clear();
        self.paired_wrap_symbols.clear();
        for &pair in PAIRED_WRAP_SYMBOLS {
            self.paired_wrap_symbols.insert(pair.to_string());
            self.all_leaf_symbols.insert(pair.to_string());
        }
        for style in &BOX_STYLES {
            for &sym in &style.1 {
                self.all_leaf_symbols.insert(sym.to_string());
            }
        }
        for major in &self.majors {
            for sub in &major.subs {
                for sym in &sub.symbols {
                    self.all_leaf_symbols.insert(sym.clone());
                    if sub.mode == SymbolCategoryMode::Wrap && sym.chars().count() == 2 {
                        self.paired_wrap_symbols.insert(sym.clone());
                    }
                }
            }
        }
    }

    fn parse_keywords(raw: &str) -> Vec<String> {
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn init_builtin_tree(&mut self) {
        let spec: &[(&str, &str, &[(&str, &str, &str, SymbolCategoryMode)])] = &[
            (
                "標點引括",
                "標點,引括,標點符號,punct",
                &[
                    ("成對引號", "「」 『』 “” ‘’ 〝〞", "引號,單引號,雙引號,quote,quotes", SymbolCategoryMode::Wrap),
                    ("成對括號", "（） 【】 〔〕 《》 〈〉 〖〗 ［］ ｛｝", "括號,括弧,方括號,中括號,大括號,書名號,六角括號,bracket,brackets,paren", SymbolCategoryMode::Wrap),
                    ("直排括號", "︵︶ ︷︸ ︹︺ ︻︼ ︽︾ ︿﹀ ﹁﹂ ﹃﹄", "直排括號,直式括號", SymbolCategoryMode::Wrap),
                    ("句讀標點", "， 。 、 ； ： ？ ！ …… ──", "句讀,刪節號,省略號,點點點,破折號,句號,逗號,頓號,分號,冒號,問號,驚嘆號,感嘆號,ellipsis,dash", SymbolCategoryMode::Normal),
                    ("連字間隔", "· ‧ ～ — – ― ﹏ ＿ 〃", "間隔號,音界號,波浪號,連接號,底線", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "幾何核取",
                "幾何,形狀,圖形,geometry,shape",
                &[
                    ("方框核取", "□ ■ ☑ ☒ ▢ ▣ ▫ ▪ ⬜", "方框,正方形,核取,核取方塊,空格框,空心框,實心框,方塊,框,box,square,checkbox", SymbolCategoryMode::Normal),
                    ("圓形環點", "○ ● ◎ ◉ ◌ ◍ ◐ ◑ ◯", "圓圈,圓形,空心圓,實心圓,雙圈,同心圓,圓點,圓,circle,dot", SymbolCategoryMode::Normal),
                    ("三角菱形", "△ ▲ ▽ ▼ ◁ ▷ ◇ ◆ ◈", "三角形,三角,正三角,倒三角,菱形,鑽石,triangle,diamond", SymbolCategoryMode::Normal),
                    ("星號花飾", "☆ ★ ※ ✳ ✴ ✦ ✧ ❀ ❁", "星星,星號,五角星,米字,米字號,重點,參考號,花飾,星,star,asterisk", SymbolCategoryMode::Normal),
                    ("勾叉對錯", "✓ ✔ ✗ ✘ ⭕ ❌ ⊕ ⊖ ⊗", "打勾,勾,勾勾,打叉,叉,叉叉,對錯,正確,錯誤,check,tick,cross", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "箭頭方向",
                "箭頭,方向,指標,arrow,arrows",
                &[
                    ("單線箭頭", "→ ← ↑ ↓ ↗ ↘ ↙ ↖ ↔", "單線箭頭,右箭頭,左箭頭,上箭頭,下箭頭,雙向箭頭,right,left,up,down", SymbolCategoryMode::Normal),
                    ("雙線推論", "⇒ ⇐ ⇑ ⇓ ⇔ ⇖ ⇗ ⇘ ⇙", "雙箭頭,雙線箭頭,推論,蘊含,等價,若且唯若,implies,iff", SymbolCategoryMode::Normal),
                    ("實心指標", "➔ ➜ ➤ ➢ ▶ ◀ ▲ ▼ ☞", "粗箭頭,實心箭頭,手指,播放,pointer", SymbolCategoryMode::Normal),
                    ("迴轉循環", "↺ ↻ ↩ ↪ ⤴ ⤵ ⇄ ⇅ ⇌", "循環,重整,迴轉,對調,來回,可逆,refresh,swap", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "數理邏輯",
                "數學,數理,邏輯,算術,math,logic",
                &[
                    ("四則正負", "＋ － × ÷ ± ∓ ＝ ≠ ≈", "加減乘除,正負,正負號,乘號,除號,不等於,約等於,plus,minus,times,divide,pm,neq,approx", SymbolCategoryMode::Normal),
                    ("大小比較", "＜ ＞ ≤ ≥ ≦ ≧ ≪ ≫ ≡", "小於等於,大於等於,恆等於,遠小於,遠大於,比較,leq,geq,equiv", SymbolCategoryMode::Normal),
                    ("微積根號", "√ ∛ ∞ ∑ ∏ ∫ ∂ ∇ ∝", "根號,開根號,無限大,無窮大,無限,總和,連乘,積分,偏微分,梯度,正比,sqrt,inf,infinity,sum,integral", SymbolCategoryMode::Normal),
                    ("集合邏輯", "∈ ∉ ⊂ ⊆ ∪ ∩ ∅ ∀ ∃", "屬於,不屬於,子集,聯集,交集,空集合,空集,任意,所有,存在,集合,in,subset,union,empty,forall,exists", SymbolCategoryMode::Normal),
                    ("幾何比例", "∵ ∴ ⊥ ∥ ∠ ∟ ‰ ‱ ∶", "因為,所以,垂直,平行,角度,直角,千分比,萬分比,because,therefore,perp,parallel,angle", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "單位貨幣",
                "單位,度量衡,貨幣,金錢,unit,currency",
                &[
                    ("溫度角度", "℃ ℉ ° ′ ″ Å μ Ω ℧", "溫度,攝氏,華氏,度,度數,歐姆,微,埃,deg,degree,celsius,fahrenheit,ohm,micro", SymbolCategoryMode::Normal),
                    ("長度面積", "㎜ ㎝ ㎞ ㎡ ㎢ ㎚ ㎛ 寸 尺", "公釐,毫米,公分,厘米,公里,千米,平方公尺,長度,面積,mm,cm,km,m2", SymbolCategoryMode::Normal),
                    ("重量容積", "㎎ ㎏ ㏄ ㎖ ℓ ㎘ ㎐ ㎑ ㎒", "毫克,公斤,千克,毫升,公升,赫茲,重量,容積,mg,kg,cc,ml,hz", SymbolCategoryMode::Normal),
                    ("世界貨幣", "NT$ ＄ ￥ € £ ¢ ₩ ₹ ฿", "台幣,新台幣,美金,美元,日圓,日幣,歐元,英鎊,韓元,盧比,泰銖,ntd,usd,dollar,yen,euro,pound,won", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "序號圈碼",
                "序號,編號,圈號,number,index",
                &[
                    ("白底圈號", "① ② ③ ④ ⑤ ⑥ ⑦ ⑧ ⑨ ⑩ ⑪ ⑫ ⑬ ⑭ ⑮ ⑯ ⑰ ⑱ ⑲ ⑳", "圈號,圓圈數字,白圈數字,circled", SymbolCategoryMode::Normal),
                    ("黑底圈號", "❶ ❷ ❸ ❹ ❺ ❻ ❼ ❽ ❾ ❿", "黑圈,黑底圈號,實心圈號", SymbolCategoryMode::Normal),
                    ("括弧點號", "⑴ ⑵ ⑶ ⑷ ⑸ ⑹ ⑺ ⑻ ⑼ ⑽ ⒈ ⒉ ⒊ ⒋ ⒌ ⒍ ⒎ ⒏ ⒐ ⒑", "括弧數字,括號數字,點號數字", SymbolCategoryMode::Normal),
                    ("羅馬大寫", "Ⅰ Ⅱ Ⅲ Ⅳ Ⅴ Ⅵ Ⅶ Ⅷ Ⅸ Ⅹ Ⅺ Ⅻ", "羅馬數字,大寫羅馬數字,roman", SymbolCategoryMode::Normal),
                    ("羅馬小寫", "ⅰ ⅱ ⅲ ⅳ ⅴ ⅵ ⅶ ⅷ ⅸ ⅹ ⅺ ⅻ", "小寫羅馬數字", SymbolCategoryMode::Normal),
                    ("中文圈碼", "㈠ ㈡ ㈢ ㈣ ㈤ ㈥ ㈦ ㈧ ㈨ ㈩ 甲 乙 丙 丁 戊 己 庚 辛 壬 癸", "中文圈號,天干,甲乙丙丁", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "製表框線",
                "製表,框線,表格,畫框,畫線,table,boxdraw",
                &[
                    ("細線九宮", "┌ ┬ ┐ ├ ┼ ┤ └ ┴ ┘ ─ │", "細線,細框,單線框", SymbolCategoryMode::Box),
                    ("粗線九宮", "┏ ┳ ┓ ┣ ╋ ┫ ┗ ┻ ┛ ━ ┃", "粗線,粗框", SymbolCategoryMode::Box),
                    ("雙線九宮", "╔ ╦ ╗ ╠ ╬ ╣ ╚ ╩ ╝ ═ ║", "雙線,雙線框", SymbolCategoryMode::Box),
                    ("圓角區塊", "╭ ╮ ╰ ╯ ▀ ▄ █ ▌ ▐ ░ ▒ ▓", "圓角,圓角框,區塊,陰影,網底", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "語文音標",
                "語文,音標,字母",
                &[
                    ("希臘大寫", "Α Β Γ Δ Ε Ζ Η Θ Ι Κ Λ Μ Ν Ξ Ο Π Ρ Σ Τ Υ Φ Χ Ψ Ω", "希臘,希臘字母,大寫希臘,greek,alpha,beta,gamma,delta,omega", SymbolCategoryMode::Normal),
                    ("希臘小寫", "α β γ δ ε ζ η θ ι κ λ μ ν ξ ο π ρ σ τ υ φ χ ψ ω", "小寫希臘,pi,sigma,theta,lambda", SymbolCategoryMode::Normal),
                    ("注音聲母", "ㄅ ㄆ ㄇ ㄈ ㄉ ㄊ ㄋ ㄌ ㄍ ㄎ ㄏ ㄐ ㄑ ㄒ ㄓ ㄔ ㄕ ㄖ ㄗ ㄘ ㄙ", "注音,注音符號,聲母,bopomofo", SymbolCategoryMode::Normal),
                    ("注音韻調", "ㄧ ㄨ ㄩ ㄚ ㄛ ㄜ ㄝ ㄞ ㄟ ㄠ ㄡ ㄢ ㄣ ㄤ ㄥ ㄦ ˊ ˇ ˋ ˙", "韻母,聲調", SymbolCategoryMode::Normal),
                ],
            ),
            (
                "特殊記號",
                "特殊,記號,圖示",
                &[
                    ("文書法務", "§ ¶ © ® ™ ℗ ℠ № ℡", "版權,著作權,商標,註冊商標,章節,段落號,copyright,tm", SymbolCategoryMode::Normal),
                    ("音樂棋牌", "♪ ♫ ♩ ♬ ♭ ♮ ♯ ♠ ♥ ♦ ♣", "音符,音樂,升記號,降記號,撲克,黑桃,紅心,梅花,music", SymbolCategoryMode::Normal),
                    ("天氣天象", "☀ ☁ ☂ ☃ ⚡ ☼ ☽ ☾ ☄", "天氣,太陽,晴天,雲,雨傘,雪人,閃電,月亮,weather,sun,moon", SymbolCategoryMode::Normal),
                    ("哲學心靈", "☸ 🕉 ☯ ✡ ✝ 卍 ☮ ❀ ❖", "法輪,太極,八卦,宗教,哲學,卍,和平,蓮花,om,dharma", SymbolCategoryMode::Normal),
                ],
            ),
        ];

        self.majors.clear();
        for &(m_name, m_kw, subs) in spec {
            let mut sub_vec = Vec::with_capacity(subs.len());
            for &(s_name, syms, s_kw, mode) in subs {
                sub_vec.push(SymbolSubCategory {
                    name: s_name.to_string(),
                    symbols: syms.split_whitespace().map(|x| x.to_string()).collect(),
                    keywords: Self::parse_keywords(s_kw),
                    mode,
                });
            }
            self.majors.push(SymbolMajorCategory {
                name: m_name.to_string(),
                keywords: Self::parse_keywords(m_kw),
                subs: sub_vec,
            });
        }
        self.rebuild_symbol_indexes();
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let p = path.as_ref();
        let file = File::open(p)?;
        let reader = BufReader::new(file);

        let mut majors: Vec<SymbolMajorCategory> = Vec::new();
        let mut major_pos: HashMap<String, usize> = HashMap::new();
        let mut count = 0usize;

        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("PATH") {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            let path_part = parts[0].trim();
            if path_part.is_empty() {
                continue;
            }
            let syms_part = parts.get(1).map(|s| s.trim()).unwrap_or("");
            let kw_part = parts.get(2).map(|s| s.trim()).unwrap_or("");
            let mode_part = parts.get(3).map(|s| s.trim()).unwrap_or("normal");
            let mode = match mode_part.to_ascii_lowercase().as_str() {
                "wrap" => SymbolCategoryMode::Wrap,
                "box" => SymbolCategoryMode::Box,
                _ => SymbolCategoryMode::Normal,
            };

            if let Some((maj_name, sub_name)) = path_part.split_once('/') {
                let maj_name = maj_name.trim();
                let sub_name = sub_name.trim();
                if maj_name.is_empty() || sub_name.is_empty() {
                    continue;
                }
                let maj_idx = if let Some(&idx) = major_pos.get(maj_name) {
                    idx
                } else {
                    let idx = majors.len();
                    majors.push(SymbolMajorCategory {
                        name: maj_name.to_string(),
                        keywords: Vec::new(),
                        subs: Vec::new(),
                    });
                    major_pos.insert(maj_name.to_string(), idx);
                    idx
                };
                let syms: Vec<String> = syms_part
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                let kws = Self::parse_keywords(kw_part);
                if let Some(existing_sub) = majors[maj_idx].subs.iter_mut().find(|s| s.name == sub_name) {
                    existing_sub.symbols = syms;
                    existing_sub.keywords = kws;
                    existing_sub.mode = mode;
                } else {
                    majors[maj_idx].subs.push(SymbolSubCategory {
                        name: sub_name.to_string(),
                        symbols: syms,
                        keywords: kws,
                        mode,
                    });
                }
                count += 1;
            } else {
                let maj_name = path_part;
                let kws = Self::parse_keywords(kw_part);
                if let Some(&idx) = major_pos.get(maj_name) {
                    majors[idx].keywords = kws;
                } else {
                    let idx = majors.len();
                    majors.push(SymbolMajorCategory {
                        name: maj_name.to_string(),
                        keywords: kws,
                        subs: Vec::new(),
                    });
                    major_pos.insert(maj_name.to_string(), idx);
                }
                count += 1;
            }
        }

        if !majors.is_empty() {
            self.majors = majors;
            self.rebuild_symbol_indexes();
        }
        self.file_path = Some(p.to_path_buf());
        if let Ok(meta) = std::fs::metadata(p) {
            self.last_modified = meta.modified().ok();
        }
        self.last_check = Some(Instant::now());
        Ok(count)
    }

    pub fn maybe_reload(&mut self) {
        let Some(ref path) = self.file_path else {
            return;
        };
        let now = Instant::now();
        if let Some(last) = self.last_check {
            if now.duration_since(last) < Duration::from_millis(SYMBOL_TREE_RELOAD_THROTTLE_MS) {
                return;
            }
        }
        self.last_check = Some(now);
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(mtime) = meta.modified() {
                let need_reload = match self.last_modified {
                    Some(prev) => mtime > prev,
                    None => true,
                };
                if need_reload {
                    let p = path.clone();
                    let _ = self.load_tsv(p);
                }
            }
        }
    }

    fn prefer_first(mut cands: Vec<String>, preferred: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(cands.len());
        for &p in preferred {
            if cands.iter().any(|c| c == p) && !out.iter().any(|c| c == p) {
                out.push(p.to_string());
            }
        }
        for c in cands.drain(..) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }

    pub fn lookup_symbol_tab_target(&self, query: &str) -> Option<SymbolTabTarget> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        let low = trimmed.to_ascii_lowercase();

        // 1. 根節點通用召喚詞
        if matches!(low.as_str(), "符號" | "符號表" | "特殊符號" | "symbols" | "symbol") {
            return Some(SymbolTabTarget::OpenTree {
                major_idx: None,
                sub_idx: None,
            });
        }

        // 2. 子分類既有常見多義詞之直達語意優先映射（箭頭／括號／引號／書名號／勾叉／數學符號）
        match low.as_str() {
            "箭頭" | "arrow" | "arrows" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "→", "←", "↑", "↓", "↗", "↘", "↙", "↖", "⇒", "⇔",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "雙箭頭" | "雙線箭頭" | "推論" | "implies" | "iff" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "⇒", "⇔", "⇐", "⇑", "⇓", "⇖", "⇗", "⇘", "⇙",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "括號" | "括弧" | "bracket" | "brackets" | "paren" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "（）", "【】", "〔〕", "《》", "〈〉", "「」", "『』", "〖〗", "［］", "｛｝",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "引號" | "quote" | "quotes" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "「」", "『』", "“”", "‘’", "〝〞",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "書名號" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "《》", "〈〉", "︽︾", "︿﹀",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "打勾" | "勾" | "勾勾" | "check" | "tick" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "✓", "✔", "☑", "⭕", "✗", "✘", "☒", "❌", "⊕",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "打叉" | "叉" | "叉叉" | "cross" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "✗", "✘", "☒", "❌", "✓", "✔", "☑", "⊖", "⊗",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "刪節號" | "省略號" | "點點點" | "ellipsis" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "……", "…", "⋯", "...", "──", "——",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            "破折號" | "dash" => {
                return Some(SymbolTabTarget::Candidates(vec![
                    "──", "——", "—", "─", "–", "―", "……",
                ].into_iter().map(|s| s.to_string()).collect()));
            }
            _ => {}
        }

        // 3. 大類樹幹導覽（標點／幾何／數理／數學／單位／序號／製表／框線／表格／語文）
        for (mi, major) in self.majors.iter().enumerate() {
            if major.name == trimmed
                || major
                    .keywords
                    .iter()
                    .any(|kw| kw.eq_ignore_ascii_case(&low))
            {
                return Some(SymbolTabTarget::OpenTree {
                    major_idx: Some(mi),
                    sub_idx: None,
                });
            }
        }

        // 4. 精確比對各子分類名稱或關鍵詞
        let preferred_map: &[(&str, &[&str])] = &[
            ("上箭頭", &["↑"]),
            ("up", &["↑"]),
            ("下箭頭", &["↓"]),
            ("down", &["↓"]),
            ("左箭頭", &["←"]),
            ("left", &["←"]),
            ("右箭頭", &["→"]),
            ("right", &["→"]),
            ("雙向箭頭", &["↔"]),
            ("單引號", &["「」", "‘’"]),
            ("雙引號", &["『』", "“”", "〝〞"]),
            ("方括號", &["【】", "［］", "〔〕"]),
            ("中括號", &["【】", "［］", "〔〕"]),
            ("大括號", &["｛｝"]),
            ("六角括號", &["〔〕"]),
            ("句號", &["。", "……"]),
            ("逗號", &["，", "、"]),
            ("頓號", &["、", "，"]),
            ("分號", &["；", "："]),
            ("冒號", &["：", "；"]),
            ("問號", &["？", "！"]),
            ("驚嘆號", &["！", "？"]),
            ("感嘆號", &["！", "？"]),
            ("華氏", &["℉", "℃", "°"]),
            ("fahrenheit", &["℉", "℃", "°"]),
            ("攝氏", &["℃", "℉", "°"]),
            ("celsius", &["℃", "℉", "°"]),
            ("度", &["°", "℃", "℉"]),
            ("度數", &["°", "℃", "℉"]),
            ("deg", &["°", "℃", "℉"]),
            ("degree", &["°", "℃", "℉"]),
            ("歐姆", &["Ω", "℧", "μ"]),
            ("ohm", &["Ω", "℧", "μ"]),
            ("正負", &["±", "∓"]),
            ("正負號", &["±", "∓"]),
            ("pm", &["±", "∓"]),
            ("乘號", &["×", "÷"]),
            ("times", &["×", "÷"]),
            ("除號", &["÷", "×"]),
            ("divide", &["÷", "×"]),
            ("不等於", &["≠", "≈", "＝"]),
            ("neq", &["≠", "≈", "＝"]),
            ("約等於", &["≈", "≠", "＝"]),
            ("approx", &["≈", "≠", "＝"]),
            ("小於等於", &["≤", "≦", "≥"]),
            ("leq", &["≤", "≦", "≥"]),
            ("大於等於", &["≥", "≧", "≤"]),
            ("geq", &["≥", "≧", "≤"]),
            ("根號", &["√", "∛"]),
            ("開根號", &["√", "∛"]),
            ("sqrt", &["√", "∛"]),
            ("無限大", &["∞"]),
            ("無窮大", &["∞"]),
            ("無限", &["∞"]),
            ("inf", &["∞"]),
            ("infinity", &["∞"]),
            ("空集合", &["∅", "∈", "∉"]),
            ("空集", &["∅", "∈", "∉"]),
            ("empty", &["∅", "∈", "∉"]),
            ("屬於", &["∈", "∉", "⊂", "⊆"]),
            ("因為", &["∵", "∴"]),
            ("because", &["∵", "∴"]),
            ("所以", &["∴", "∵"]),
            ("therefore", &["∴", "∵"]),
            ("垂直", &["⊥", "∥"]),
            ("perp", &["⊥", "∥"]),
            ("平行", &["∥", "⊥"]),
            ("parallel", &["∥", "⊥"]),
            ("角度", &["∠", "∟"]),
            ("angle", &["∠", "∟"]),
            ("菱形", &["◇", "◆", "◈"]),
            ("diamond", &["◇", "◆", "◈"]),
            ("米字", &["※", "✳", "✴"]),
            ("米字號", &["※", "✳", "✴"]),
            ("asterisk", &["※", "✳", "✴"]),
            ("版權", &["©", "®", "™"]),
            ("著作權", &["©", "®", "™"]),
            ("copyright", &["©", "®", "™"]),
            ("商標", &["™", "®", "©"]),
            ("tm", &["™", "®", "©"]),
            ("太極", &["☯", "☸", "🕉"]),
            ("法輪", &["☸", "🕉", "☯"]),
            ("om", &["🕉", "☸", "☯"]),
        ];

        for (mi, major) in self.majors.iter().enumerate() {
            for (si, sub) in major.subs.iter().enumerate() {
                let matched = sub.name == trimmed
                    || sub
                        .keywords
                        .iter()
                        .any(|kw| kw.eq_ignore_ascii_case(&low));
                if matched {
                    if sub.mode == SymbolCategoryMode::Box {
                        return Some(SymbolTabTarget::OpenTree {
                            major_idx: Some(mi),
                            sub_idx: Some(si),
                        });
                    }
                    let mut cands = sub.symbols.clone();
                    for &(k, prefs) in preferred_map {
                        if k.eq_ignore_ascii_case(&low) {
                            cands = Self::prefer_first(cands, prefs);
                            break;
                        }
                    }
                    return Some(SymbolTabTarget::Candidates(cands));
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone, Default)]
struct SymbolTreeSession {
    major_idx: Option<usize>,
    sub_idx: Option<usize>,
    in_mru: bool,
    sticky_style: usize,
    sticky_drawn_count: usize,
    has_selection_wrap: bool,
    mru: Vec<String>,
}

impl SymbolTreeSession {
    fn reset_active_navigation(&mut self) {
        self.major_idx = None;
        self.sub_idx = None;
        self.in_mru = false;
        self.sticky_style = 0;
        self.sticky_drawn_count = 0;
        self.has_selection_wrap = false;
    }

    fn push_mru(&mut self, sym: &str) {
        let s = sym.trim();
        if s.is_empty() {
            return;
        }
        self.mru.retain(|x| x != s);
        self.mru.insert(0, s.to_string());
        if self.mru.len() > MRU_CAP {
            self.mru.truncate(MRU_CAP);
        }
    }
}

static GLOBAL_SYMBOL_TREE: OnceLock<RwLock<SymbolTreeStore>> = OnceLock::new();
static SYMBOL_TREE_SESSIONS: OnceLock<Mutex<HashMap<usize, SymbolTreeSession>>> = OnceLock::new();

fn global_store() -> &'static RwLock<SymbolTreeStore> {
    GLOBAL_SYMBOL_TREE.get_or_init(|| RwLock::new(SymbolTreeStore::default()))
}

fn sessions_map() -> &'static Mutex<HashMap<usize, SymbolTreeSession>> {
    SYMBOL_TREE_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn configure_tsv_path<P: AsRef<Path>>(path: P) {
    let p = path.as_ref();
    if !p.exists() {
        return;
    }
    if let Ok(mut store) = global_store().write() {
        let should_load = match store.file_path.as_ref() {
            Some(cur) => cur != p,
            None => true,
        };
        if should_load {
            let _ = store.load_tsv(p);
        }
    }
}

fn maybe_reload_global_store() {
    let need_probe = {
        if let Ok(store) = global_store().read() {
            if let Some(last) = store.last_check {
                store.file_path.is_some()
                    && Instant::now().duration_since(last)
                        >= Duration::from_millis(SYMBOL_TREE_RELOAD_THROTTLE_MS)
            } else {
                store.file_path.is_some()
            }
        } else {
            false
        }
    };
    if need_probe {
        if let Ok(mut store) = global_store().write() {
            store.maybe_reload();
        }
    }
}

pub fn is_paired_wrap_symbol(s: &str) -> bool {
    if PAIRED_WRAP_SYMBOLS.contains(&s) {
        return true;
    }
    if let Ok(store) = global_store().read() {
        store.paired_wrap_symbols.contains(s)
    } else {
        false
    }
}

fn is_cjk_ideograph(c: char) -> bool {
    matches!(
        c,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2FA1F}'
    )
}

pub fn is_symbol_candidate(s: &str) -> bool {
    if s.is_empty() || s.chars().all(is_cjk_ideograph) {
        return false;
    }
    if matches!(
        s,
        "……"
            | "…"
            | "⋯"
            | "..."
            | "──"
            | "——"
            | "—"
            | "─"
            | "–"
            | "―"
            | "－"
            | "～"
            | "～～"
            | "〰"
            | "〜"
            | "．"
            | "·"
            | "‧"
            | "•"
            | "﹐"
            | "﹑"
            | "⁇"
            | "⁈"
            | "¿"
            | "‼"
            | "⁉"
            | "¡"
            | "❌"
            | "⭕"
            | "⬜"
            | "⬛"
            | "✓"
            | "✔"
            | "☑"
            | "✗"
            | "✘"
            | "☒"
            | "⊕"
            | "⊖"
            | "⊗"
    ) {
        return true;
    }
    if let Ok(store) = global_store().read() {
        store.all_leaf_symbols.contains(s)
    } else {
        false
    }
}

pub fn lookup_symbol_tab_target(query: &str) -> Option<SymbolTabTarget> {
    maybe_reload_global_store();
    global_store()
        .read()
        .ok()
        .and_then(|s| s.lookup_symbol_tab_target(query))
}

fn with_session_mut<F, R>(engine_key: usize, f: F) -> R
where
    F: FnOnce(&mut SymbolTreeSession) -> R,
{
    let mut guard = sessions_map().lock().unwrap_or_else(|e| e.into_inner());
    if guard.len() >= SESSION_MAP_CAP && !guard.contains_key(&engine_key) {
        guard.retain(|_, s| s.major_idx.is_some() || s.in_mru || !s.mru.is_empty());
        if guard.len() >= SESSION_MAP_CAP {
            guard.clear();
        }
    }
    let session = guard.entry(engine_key).or_default();
    f(session)
}

fn read_session_clone(engine_key: usize) -> SymbolTreeSession {
    sessions_map()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&engine_key)
        .cloned()
        .unwrap_or_default()
}

pub fn clear_active_session(engine_key: usize) {
    if let Ok(mut guard) = sessions_map().lock() {
        if let Some(s) = guard.get_mut(&engine_key) {
            s.reset_active_navigation();
        }
    }
}

pub fn clear_session_and_mru(engine_key: usize) {
    if let Ok(mut guard) = sessions_map().lock() {
        guard.remove(&engine_key);
    }
}

pub fn open_at(
    engine_key: usize,
    major_idx: Option<usize>,
    sub_idx: Option<usize>,
    has_selection_wrap: bool,
) -> (Vec<String>, usize) {
    maybe_reload_global_store();
    let store = global_store().read().unwrap();
    let eff_major = major_idx.filter(|&mi| mi < store.majors.len());
    let eff_sub = match eff_major {
        Some(mi) => sub_idx.filter(|&si| si < store.majors[mi].subs.len()),
        None => None,
    };

    with_session_mut(engine_key, |sess| {
        sess.major_idx = eff_major;
        sess.sub_idx = eff_sub;
        sess.in_mru = false;
        sess.has_selection_wrap = has_selection_wrap;
        sess.sticky_drawn_count = 0;
        if let (Some(mi), Some(si)) = (eff_major, eff_sub) {
            if store.majors[mi].subs[si].mode == SymbolCategoryMode::Box {
                sess.sticky_style = si.min(BOX_STYLES.len() - 1);
            }
        }
        build_candidates_for_session(&store, sess)
    })
}

fn build_sticky_box_candidates(style: usize) -> Vec<String> {
    let syms = &BOX_STYLES[style.min(BOX_STYLES.len() - 1)].1;
    syms.iter()
        .zip(BOX_KEY_HINTS.iter())
        .map(|(sym, hint)| format!("{}{}", sym, hint))
        .collect()
}

fn strip_box_hint(cand: &str) -> String {
    if let Some(pos) = cand.find('(') {
        cand[..pos].to_string()
    } else {
        cand.to_string()
    }
}

fn build_candidates_for_session(
    store: &SymbolTreeStore,
    sess: &SymbolTreeSession,
) -> (Vec<String>, usize) {
    if sess.in_mru {
        return (sess.mru.clone(), 0);
    }
    match (sess.major_idx, sess.sub_idx) {
        (None, _) => {
            let mut cands: Vec<String> = store.majors.iter().map(|m| m.name.clone()).collect();
            if !sess.mru.is_empty() {
                cands.push("最近常用(0)".to_string());
            }
            (cands, 0)
        }
        (Some(mi), None) => {
            if let Some(maj) = store.majors.get(mi) {
                (maj.subs.iter().map(|s| s.name.clone()).collect(), 0)
            } else {
                (Vec::new(), 0)
            }
        }
        (Some(mi), Some(si)) => {
            if let Some(sub) = store.majors.get(mi).and_then(|m| m.subs.get(si)) {
                if sub.mode == SymbolCategoryMode::Box {
                    (build_sticky_box_candidates(sess.sticky_style), 0)
                } else {
                    (sub.symbols.clone(), 0)
                }
            } else {
                (Vec::new(), 0)
            }
        }
    }
}

pub fn is_sticky_box_active(engine_key: usize) -> bool {
    let sess = read_session_clone(engine_key);
    let Ok(store) = global_store().read() else {
        return false;
    };
    if let (Some(mi), Some(si)) = (sess.major_idx, sess.sub_idx) {
        if let Some(sub) = store.majors.get(mi).and_then(|m| m.subs.get(si)) {
            return sub.mode == SymbolCategoryMode::Box;
        }
    }
    false
}

pub fn breadcrumb_badge(engine_key: usize) -> String {
    let sess = read_session_clone(engine_key);
    let Ok(store) = global_store().read() else {
        return "Ω符號樹".to_string();
    };
    if sess.in_mru {
        return "Ω符號▸最近常用".to_string();
    }
    match (sess.major_idx, sess.sub_idx) {
        (None, _) => "Ω符號樹".to_string(),
        (Some(mi), None) => {
            let m_name = store.majors.get(mi).map(|m| m.name.as_str()).unwrap_or("");
            format!("Ω符號▸{}", m_name)
        }
        (Some(mi), Some(si)) => {
            let sub = store.majors.get(mi).and_then(|m| m.subs.get(si));
            if let Some(s) = sub {
                if s.mode == SymbolCategoryMode::Box {
                    let style_name = BOX_STYLES[sess.sticky_style.min(2)].0;
                    format!("📐製表▸{}", style_name)
                } else {
                    format!("Ω{}", s.name)
                }
            } else {
                "Ω符號樹".to_string()
            }
        }
    }
}

pub fn coach_hint(engine_key: usize, quick_key_mode: &str) -> String {
    let sess = read_session_clone(engine_key);
    let Ok(store) = global_store().read() else {
        return String::new();
    };
    if sess.in_mru {
        let key_hint = if quick_key_mode == "asdf" { "a~l/1~9" } else { "1~9" };
        return format!(
            "Ω 符號樹 ▸ 最近常用（按 {} 選取，Backspace 回根目錄，Esc 取消）",
            key_hint
        );
    }
    let key_hint = if quick_key_mode == "asdf" { "a~l/1~9" } else { "1~9" };
    match (sess.major_idx, sess.sub_idx) {
        (None, _) => {
            let wrap_note = if sess.has_selection_wrap {
                "｜選取包裹就緒"
            } else {
                ""
            };
            if let Some(latest) = sess.mru.first() {
                format!(
                    "Ω 符號樹（第1層{}）：按 {} 選大類，0:最近常用({})，Esc 取消",
                    wrap_note, key_hint, latest
                )
            } else {
                format!(
                    "Ω 符號樹（第1層{}）：按 {} 選大類，Tab/方向鍵移動，Esc 取消",
                    wrap_note, key_hint
                )
            }
        }
        (Some(mi), None) => {
            let m_name = store.majors.get(mi).map(|m| m.name.as_str()).unwrap_or("");
            let wrap_note = if sess.has_selection_wrap {
                "｜選取包裹就緒"
            } else {
                ""
            };
            format!(
                "Ω 符號樹 ▸ {}（第2層{}）：按 {} 選子類，Backspace 回上層，Esc 取消",
                m_name, wrap_note, key_hint
            )
        }
        (Some(mi), Some(si)) => {
            let m_name = store.majors.get(mi).map(|m| m.name.as_str()).unwrap_or("");
            let sub = store.majors.get(mi).and_then(|m| m.subs.get(si));
            if let Some(s) = sub {
                if s.mode == SymbolCategoryMode::Box {
                    let style_name = BOX_STYLES[sess.sticky_style.min(2)].0;
                    format!(
                        "📐 製表框線駐留【{}】：7┌ 8┬ 9┐ 4├ 5┼ 6┤ 1└ 2┴ 3┘、-/0 畫─、|/. 畫│（Tab 切換細/粗/雙線，Backspace 退格，Enter/Esc 完成）",
                        style_name
                    )
                } else {
                    let wrap_note = if sess.has_selection_wrap && s.mode == SymbolCategoryMode::Wrap {
                        "（選取後將原地成對包裹）"
                    } else if s.mode == SymbolCategoryMode::Wrap {
                        "（插入後游標自動置中）"
                    } else {
                        ""
                    };
                    format!(
                        "Ω {} ▸ {}{}：按 {} 插入，PgUp/PgDn 翻頁，Backspace 回上層",
                        m_name, s.name, wrap_note, key_hint
                    )
                }
            } else {
                String::new()
            }
        }
    }
}

fn sticky_box_char_for_key(style: usize, key: char) -> Option<&'static str> {
    let syms = &BOX_STYLES[style.min(BOX_STYLES.len() - 1)].1;
    match key {
        '7' => Some(syms[0]),
        '8' => Some(syms[1]),
        '9' => Some(syms[2]),
        '4' => Some(syms[3]),
        '5' => Some(syms[4]),
        '6' => Some(syms[5]),
        '1' => Some(syms[6]),
        '2' => Some(syms[7]),
        '3' => Some(syms[8]),
        '-' | '0' => Some(syms[9]),
        '|' | '.' | '/' => Some(syms[10]),
        _ => None,
    }
}

fn resolve_page_quick_idx(key: char, quick_key_mode: &str, page_sz: usize, cur_page: usize, len: usize) -> Option<usize> {
    let offset = if key.is_ascii_digit() && key != '0' {
        Some((key as usize) - ('1' as usize))
    } else if quick_key_mode == "asdf" {
        crate::engine::RuhiEngine::map_asdf_char_to_idx(key)
    } else {
        None
    };
    offset.and_then(|off| {
        if off < page_sz {
            let target = cur_page * page_sz + off;
            if target < len {
                Some(target)
            } else {
                None
            }
        } else {
            None
        }
    })
}

pub fn step_key(
    engine_key: usize,
    key_str: &str,
    quick_key_mode: &str,
    page_sz: usize,
    current_candidates: &mut Vec<String>,
    candidate_cycle_idx: &mut usize,
) -> SymbolTreeStep {
    maybe_reload_global_store();
    let store = global_store().read().unwrap();
    let page_sz = page_sz.max(1);

    with_session_mut(engine_key, |sess| {
        let in_box_mode = if let (Some(mi), Some(si)) = (sess.major_idx, sess.sub_idx) {
            store
                .majors
                .get(mi)
                .and_then(|m| m.subs.get(si))
                .map_or(false, |s| s.mode == SymbolCategoryMode::Box)
        } else {
            false
        };

        // ── 1. 製表框線駐留連打模式 (Sticky Box-Drawing) ──
        if in_box_mode {
            if matches!(key_str, "Escape" | "escape" | "\x1b" | "Enter" | "Return" | "enter" | "\n" | "\r") {
                sess.reset_active_navigation();
                current_candidates.clear();
                *candidate_cycle_idx = 0;
                return SymbolTreeStep::Exit;
            }
            if matches!(key_str, "BackSpace" | "backspace" | "\x08") {
                if sess.sticky_drawn_count > 0 {
                    sess.sticky_drawn_count -= 1;
                    return SymbolTreeStep::DeletePrevStickyBox;
                }
                sess.sub_idx = None;
                let (cands, idx) = build_candidates_for_session(&store, sess);
                *current_candidates = cands;
                *candidate_cycle_idx = idx;
                return SymbolTreeStep::Stay;
            }
            if matches!(key_str, "Tab" | "tab" | "\t" | "Shift+Tab" | "shift+tab") {
                let reverse = matches!(key_str, "Shift+Tab" | "shift+tab");
                sess.sticky_style = if reverse {
                    (sess.sticky_style + 2) % 3
                } else {
                    (sess.sticky_style + 1) % 3
                };
                if let Some(mi) = sess.major_idx {
                    if let Some(maj) = store.majors.get(mi) {
                        if sess.sticky_style < maj.subs.len()
                            && maj.subs[sess.sticky_style].mode == SymbolCategoryMode::Box
                        {
                            sess.sub_idx = Some(sess.sticky_style);
                        }
                    }
                }
                *current_candidates = build_sticky_box_candidates(sess.sticky_style);
                *candidate_cycle_idx = (*candidate_cycle_idx).min(current_candidates.len().saturating_sub(1));
                return SymbolTreeStep::Stay;
            }
            if matches!(key_str, "Down" | "down" | "Right" | "right") {
                if !current_candidates.is_empty() {
                    *candidate_cycle_idx = (*candidate_cycle_idx + 1) % current_candidates.len();
                }
                return SymbolTreeStep::Stay;
            }
            if matches!(key_str, "Up" | "up" | "Left" | "left") {
                if !current_candidates.is_empty() {
                    *candidate_cycle_idx = if *candidate_cycle_idx == 0 {
                        current_candidates.len() - 1
                    } else {
                        *candidate_cycle_idx - 1
                    };
                }
                return SymbolTreeStep::Stay;
            }
            if matches!(key_str, "PageDown" | "pagedown" | "Next" | "next") {
                if !current_candidates.is_empty() {
                    let cur_p = *candidate_cycle_idx / page_sz;
                    let tot = (current_candidates.len() + page_sz - 1) / page_sz;
                    let np = (cur_p + 1) % tot.max(1);
                    *candidate_cycle_idx = (np * page_sz).min(current_candidates.len() - 1);
                }
                return SymbolTreeStep::Stay;
            }
            if matches!(key_str, "PageUp" | "pageup" | "Prior" | "prior") {
                if !current_candidates.is_empty() {
                    let cur_p = *candidate_cycle_idx / page_sz;
                    let tot = (current_candidates.len() + page_sz - 1) / page_sz;
                    let pp = if cur_p > 0 { cur_p - 1 } else { tot.saturating_sub(1) };
                    *candidate_cycle_idx = (pp * page_sz).min(current_candidates.len() - 1);
                }
                return SymbolTreeStep::Stay;
            }
            if matches!(key_str, "Space" | "space" | " ") {
                if let Some(cand) = current_candidates.get(*candidate_cycle_idx).or_else(|| current_candidates.first()) {
                    let sym = strip_box_hint(cand);
                    let is_first = sess.sticky_drawn_count == 0;
                    sess.sticky_drawn_count += 1;
                    sess.push_mru(&sym);
                    return SymbolTreeStep::InsertStickyBox {
                        symbol: sym,
                        is_first,
                    };
                }
                return SymbolTreeStep::Stay;
            }
            if key_str.chars().count() == 1 {
                let ch = key_str.chars().next().unwrap();
                if let Some(sym) = sticky_box_char_for_key(sess.sticky_style, ch) {
                    let is_first = sess.sticky_drawn_count == 0;
                    sess.sticky_drawn_count += 1;
                    sess.push_mru(sym);
                    return SymbolTreeStep::InsertStickyBox {
                        symbol: sym.to_string(),
                        is_first,
                    };
                }
            }
            return SymbolTreeStep::Stay;
        }

        // ── 2. 一般多層樹狀導覽（第 1 層大類 ／ 第 2 層子類 ／ 第 3 層葉節點 ／ MRU） ──
        if matches!(key_str, "Escape" | "escape" | "\x1b") {
            sess.reset_active_navigation();
            current_candidates.clear();
            *candidate_cycle_idx = 0;
            return SymbolTreeStep::Exit;
        }
        if matches!(key_str, "BackSpace" | "backspace" | "\x08") {
            if sess.in_mru {
                sess.in_mru = false;
                let (cands, idx) = build_candidates_for_session(&store, sess);
                *current_candidates = cands;
                *candidate_cycle_idx = idx;
                return SymbolTreeStep::Stay;
            }
            if sess.sub_idx.is_some() {
                sess.sub_idx = None;
                let (cands, idx) = build_candidates_for_session(&store, sess);
                *current_candidates = cands;
                *candidate_cycle_idx = idx;
                return SymbolTreeStep::Stay;
            }
            if sess.major_idx.is_some() {
                sess.major_idx = None;
                let (cands, idx) = build_candidates_for_session(&store, sess);
                *current_candidates = cands;
                *candidate_cycle_idx = idx;
                return SymbolTreeStep::Stay;
            }
            sess.reset_active_navigation();
            current_candidates.clear();
            *candidate_cycle_idx = 0;
            return SymbolTreeStep::Exit;
        }
        if matches!(key_str, "Down" | "down" | "Right" | "right" | "Tab" | "tab" | "\t") {
            if !current_candidates.is_empty() {
                *candidate_cycle_idx = (*candidate_cycle_idx + 1) % current_candidates.len();
            }
            return SymbolTreeStep::Stay;
        }
        if matches!(key_str, "Up" | "up" | "Left" | "left" | "Shift+Tab" | "shift+tab") {
            if !current_candidates.is_empty() {
                *candidate_cycle_idx = if *candidate_cycle_idx == 0 {
                    current_candidates.len() - 1
                } else {
                    *candidate_cycle_idx - 1
                };
            }
            return SymbolTreeStep::Stay;
        }
        if matches!(key_str, "PageDown" | "pagedown" | "Next" | "next") {
            if !current_candidates.is_empty() {
                let cur_p = *candidate_cycle_idx / page_sz;
                let tot = (current_candidates.len() + page_sz - 1) / page_sz;
                let np = (cur_p + 1) % tot.max(1);
                *candidate_cycle_idx = (np * page_sz).min(current_candidates.len() - 1);
            }
            return SymbolTreeStep::Stay;
        }
        if matches!(key_str, "PageUp" | "pageup" | "Prior" | "prior") {
            if !current_candidates.is_empty() {
                let cur_p = *candidate_cycle_idx / page_sz;
                let tot = (current_candidates.len() + page_sz - 1) / page_sz;
                let pp = if cur_p > 0 { cur_p - 1 } else { tot.saturating_sub(1) };
                *candidate_cycle_idx = (pp * page_sz).min(current_candidates.len() - 1);
            }
            return SymbolTreeStep::Stay;
        }

        // 根節點按 '0' 直接進入最近常用符號 (MRU)
        if !sess.in_mru && sess.major_idx.is_none() && key_str == "0" && !sess.mru.is_empty() {
            sess.in_mru = true;
            let (cands, idx) = build_candidates_for_session(&store, sess);
            *current_candidates = cands;
            *candidate_cycle_idx = idx;
            return SymbolTreeStep::Stay;
        }

        let selected_idx = if matches!(key_str, "Space" | "space" | " " | "Enter" | "Return" | "enter" | "\n" | "\r") {
            if !current_candidates.is_empty() {
                Some((*candidate_cycle_idx).min(current_candidates.len() - 1))
            } else {
                None
            }
        } else if key_str.chars().count() == 1 {
            let ch = key_str.chars().next().unwrap();
            let cur_page = *candidate_cycle_idx / page_sz;
            resolve_page_quick_idx(ch, quick_key_mode, page_sz, cur_page, current_candidates.len())
        } else {
            None
        };

        if let Some(sel) = selected_idx {
            if sess.in_mru {
                if let Some(sym) = current_candidates.get(sel).cloned() {
                    let is_wrap = PAIRED_WRAP_SYMBOLS.contains(&sym.as_str())
                        || store.paired_wrap_symbols.contains(&sym);
                    sess.push_mru(&sym);
                    sess.reset_active_navigation();
                    current_candidates.clear();
                    *candidate_cycle_idx = 0;
                    return SymbolTreeStep::CommitSymbol {
                        symbol: sym,
                        is_wrap_pair: is_wrap,
                    };
                }
                return SymbolTreeStep::Stay;
            }
            match (sess.major_idx, sess.sub_idx) {
                (None, _) => {
                    if sel >= store.majors.len() && !sess.mru.is_empty() {
                        sess.in_mru = true;
                    } else if sel < store.majors.len() {
                        sess.major_idx = Some(sel);
                        sess.sub_idx = None;
                    }
                    let (cands, idx) = build_candidates_for_session(&store, sess);
                    *current_candidates = cands;
                    *candidate_cycle_idx = idx;
                    SymbolTreeStep::Stay
                }
                (Some(mi), None) => {
                    let sub_len = store.majors.get(mi).map(|m| m.subs.len()).unwrap_or(0);
                    if sel < sub_len {
                        sess.sub_idx = Some(sel);
                        if store.majors[mi].subs[sel].mode == SymbolCategoryMode::Box {
                            sess.sticky_style = sel.min(BOX_STYLES.len() - 1);
                            sess.sticky_drawn_count = 0;
                        }
                        let (cands, idx) = build_candidates_for_session(&store, sess);
                        *current_candidates = cands;
                        *candidate_cycle_idx = idx;
                    }
                    SymbolTreeStep::Stay
                }
                (Some(mi), Some(si)) => {
                    if let Some(sym) = current_candidates.get(sel).cloned() {
                        let sub_mode = store
                            .majors
                            .get(mi)
                            .and_then(|m| m.subs.get(si))
                            .map(|s| s.mode)
                            .unwrap_or(SymbolCategoryMode::Normal);
                        let is_wrap = sub_mode == SymbolCategoryMode::Wrap
                            || PAIRED_WRAP_SYMBOLS.contains(&sym.as_str())
                            || store.paired_wrap_symbols.contains(&sym);
                        sess.push_mru(&sym);
                        sess.reset_active_navigation();
                        current_candidates.clear();
                        *candidate_cycle_idx = 0;
                        SymbolTreeStep::CommitSymbol {
                            symbol: sym,
                            is_wrap_pair: is_wrap,
                        }
                    } else {
                        SymbolTreeStep::Stay
                    }
                }
            }
        } else {
            SymbolTreeStep::Stay
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_tree_store_defaults_and_semantic_lookup() {
        let store = SymbolTreeStore::new();
        assert_eq!(store.majors.len(), 9);
        assert_eq!(
            store.lookup_symbol_tab_target("符號"),
            Some(SymbolTabTarget::OpenTree {
                major_idx: None,
                sub_idx: None,
            })
        );
        assert_eq!(
            store.lookup_symbol_tab_target("製表"),
            Some(SymbolTabTarget::OpenTree {
                major_idx: Some(6),
                sub_idx: None,
            })
        );
        match store.lookup_symbol_tab_target("方框") {
            Some(SymbolTabTarget::Candidates(c)) => assert_eq!(c[0], "□"),
            other => panic!("unexpected方框 lookup: {:?}", other),
        }
        match store.lookup_symbol_tab_target("打勾") {
            Some(SymbolTabTarget::Candidates(c)) => {
                assert!(c.contains(&"✓".to_string()));
                assert!(c.contains(&"☑".to_string()));
            }
            other => panic!("unexpected 打勾 lookup: {:?}", other),
        }
        match store.lookup_symbol_tab_target("箭頭") {
            Some(SymbolTabTarget::Candidates(c)) => assert_eq!(c[0], "→"),
            other => panic!("unexpected 箭頭 lookup: {:?}", other),
        }
    }
}
