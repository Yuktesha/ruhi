use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 四角號碼碼表單筆條目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FourCornerEntry {
    /// 四角編碼（如 `"2360"`, `"23600"`, `"2332"`, `"/1"`）
    pub code: String,
    /// 對應字詞或符號（如 `"台"`, `"台灣"`, `"，"`）
    pub text: String,
    /// 先驗權重
    pub weight: i32,
    /// 條目類型：`"full"` | `"variant"` | `"fuzzy"` | `"phrase22"` | `"phrase3"` | `"phrase4"` | `"symbol"`
    pub kind: String,
}

/// 四角號碼 0~9 筆形定義與教學口訣結構
#[derive(Debug, Clone)]
pub struct FourCornerKeyMeta {
    pub digit: char,
    pub stroke_name: &'static str,
    pub radical_examples: &'static str,
    pub mnemonic: &'static str,
}

/// 王雲五／現代化智慧四角號碼 0~9 筆形口訣對照表
pub const FOUR_CORNER_KEY_METAS: &[FourCornerKeyMeta] = &[
    FourCornerKeyMeta {
        digit: '0',
        stroke_name: "頭／撇鉤",
        radical_examples: "亠、言、主、广（兼容初學撇／左鉤）",
        mnemonic: "點下帶橫變零頭",
    },
    FourCornerKeyMeta {
        digit: '1',
        stroke_name: "橫",
        radical_examples: "一、乚、天、土（橫、挑、右鉤）",
        mnemonic: "一橫",
    },
    FourCornerKeyMeta {
        digit: '2',
        stroke_name: "垂",
        radical_examples: "丨、丿、亅、千、順（豎、撇、左鉤）",
        mnemonic: "二垂",
    },
    FourCornerKeyMeta {
        digit: '3',
        stroke_name: "點",
        radical_examples: "丶、乀、之、衣（點、捺）",
        mnemonic: "三點捺",
    },
    FourCornerKeyMeta {
        digit: '4',
        stroke_name: "叉",
        radical_examples: "十、乂、艹、大、對（兩筆相交）",
        mnemonic: "叉四",
    },
    FourCornerKeyMeta {
        digit: '5',
        stroke_name: "插",
        radical_examples: "扌、丰、申、史（一筆穿過兩筆以上）",
        mnemonic: "插五",
    },
    FourCornerKeyMeta {
        digit: '6',
        stroke_name: "方",
        radical_examples: "口、囗、目、四、國（四邊齊封閉方框）",
        mnemonic: "方塊六",
    },
    FourCornerKeyMeta {
        digit: '7',
        stroke_name: "角",
        radical_examples: "﹁、厂、匕、尸、羽（橫豎折轉之轉角）",
        mnemonic: "七角",
    },
    FourCornerKeyMeta {
        digit: '8',
        stroke_name: "八",
        radical_examples: "八、丷、人、入、分（八字形相對且不相交）",
        mnemonic: "八八",
    },
    FourCornerKeyMeta {
        digit: '9',
        stroke_name: "小",
        radical_examples: "小、忄、⺌、尖、慕（中豎帶左右兩點）",
        mnemonic: "小是九",
    },
];

/// 取得單一數字碼對應之簡稱（供組字預覽與字根教練顯示）
pub fn digit_stroke_short_name(d: char) -> &'static str {
    match d {
        '0' => "頭",
        '1' => "橫",
        '2' => "垂",
        '3' => "點",
        '4' => "叉",
        '5' => "插",
        '6' => "方",
        '7' => "角",
        '8' => "八",
        '9' => "小",
        '*' | '?' => "？",
        '.' => "符",
        '/' => "選",
        _ => "",
    }
}

/// 鄰近易混淆筆畫矩陣（供動態模糊四角救援查詢使用）
pub fn neighbor_fuzzy_digits(d: char) -> &'static [char] {
    match d {
        '0' => &['2', '1', '3', '6'],
        '1' => &['7', '4', '0'],
        '2' => &['0', '7', '3'],
        '3' => &['0', '8', '9', '2'],
        '4' => &['5', '1'],
        '5' => &['4', '1'],
        '6' => &['7', '0'],
        '7' => &['1', '2', '6'],
        '8' => &['9', '3'],
        '9' => &['8', '3'],
        _ => &[],
    }
}

/// 四角號碼核心查表與前兩角簡碼引擎資料結構
#[derive(Debug, Clone)]
pub struct FourCornerTable {
    pub code_to_entries: HashMap<String, Vec<FourCornerEntry>>,
    pub char_to_codes: HashMap<char, Vec<String>>,
    pub char_to_prefix2: HashMap<char, Vec<String>>,
}

impl Default for FourCornerTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FourCornerTable {
    pub fn new() -> Self {
        let mut table = Self {
            code_to_entries: HashMap::new(),
            char_to_codes: HashMap::new(),
            char_to_prefix2: HashMap::new(),
        };
        table.load_builtin_defaults();
        table
    }

    /// 內建基礎保底碼表（確保無外部 TSV 時單元測試與核心詞彙仍可完整運作）
    pub fn load_builtin_defaults(&mut self) {
        let defaults: &[(&str, &str, i32, &str)] = &[
            // 數字小鍵盤 `.` 標點符號選單前導表
            (".1", "，", 999, "symbol"),
            (".2", "。", 998, "symbol"),
            (".3", "、", 997, "symbol"),
            (".4", "？", 996, "symbol"),
            (".5", "！", 995, "symbol"),
            (".6", "：", 994, "symbol"),
            (".7", "；", 993, "symbol"),
            (".8", "「」", 992, "symbol"),
            (".9", "（）", 991, "symbol"),
            (".0", "……", 990, "symbol"),
            // 指標性單字（含標準碼、5碼附角、正異體共享、初學者免補0容錯）
            ("2360", "台", 835, "full"),
            ("23600", "台", 870, "full"),
            ("2360", "臺", 797, "variant"),
            ("2366", "台", 807, "fuzzy"),
            ("0360", "台", 790, "fuzzy"),
            ("4010", "臺", 823, "full"),
            ("40104", "臺", 858, "full"),
            ("4010", "台", 809, "variant"),
            ("4010", "查", 717, "full"),
            ("4010", "士", 715, "full"),
            ("3212", "灣", 750, "full"),
            ("32127", "灣", 785, "full"),
            ("1071", "電", 810, "full"),
            ("10716", "電", 845, "full"),
            ("7226", "腦", 740, "full"),
            ("72263", "腦", 775, "full"),
            ("4640", "如", 811, "full"),
            ("46400", "如", 846, "full"),
            ("4646", "如", 783, "fuzzy"),
            ("4060", "喜", 690, "full"),
            ("40601", "喜", 725, "full"),
            ("8640", "知", 820, "full"),
            ("3830", "道", 830, "full"),
            ("2762", "的", 960, "full"),
            ("6080", "是", 940, "full"),
            ("4021", "在", 920, "full"),
            ("2355", "我", 910, "full"),
            ("6015", "國", 890, "full"),
            ("8000", "人", 905, "full"),
            ("5000", "中", 895, "full"),
            ("4003", "大", 885, "full"),
            ("9000", "小", 875, "full"),
            ("1000", "一", 930, "full"),
            ("1010", "二", 860, "full"),
            ("1010", "三", 855, "full"),
            ("6021", "四", 850, "full"),
            ("2722", "角", 780, "full"),
            ("6121", "號", 770, "full"),
            ("1162", "碼", 765, "full"),
            ("1762", "碼", 740, "fuzzy"),
            ("5802", "輸", 755, "full"),
            ("8000", "入", 825, "full"),
            ("3413", "法", 840, "full"),
            ("8660", "智", 745, "full"),
            ("5533", "慧", 735, "full"),
            ("3402", "為", 880, "full"),
            ("2420", "什", 815, "full"),
            ("0023", "麼", 820, "full"),
            ("7750", "舉", 740, "full"),
            ("1022", "兩", 805, "full"),
            ("2624", "得", 850, "full"),
            ("0010", "主", 865, "full"),
            ("4491", "權", 790, "full"),
            // 雙字詞「前兩角簡碼矩陣 (首1,2 + 尾1,2 = 4碼)」
            ("2332", "台灣", 826, "phrase22"),
            ("2332", "臺灣", 791, "phrase22"),
            ("4032", "臺灣", 811, "phrase22"),
            ("4032", "台灣", 806, "phrase22"),
            ("0044", "主權", 545, "phrase22"),
            ("6030", "國家", 808, "phrase22"),
            ("1072", "電腦", 716, "phrase22"),
            ("4640", "如喜", 635, "phrase22"),
            ("8638", "知道", 780, "phrase22"),
            ("5880", "輸入", 740, "phrase22"),
            ("8655", "智慧", 730, "phrase22"),
            ("6027", "四角", 725, "phrase22"),
            ("6111", "號碼", 720, "phrase22"),
            ("2360", "我國", 668, "phrase22"),
            ("2360", "我是", 654, "phrase22"),
            // 三字詞（5碼：首字1,3 + 中字1 + 尾字2,4）與四字成語（6碼：首字1,3 + 中一1 + 中二1 + 尾字2,4）
            ("50843", "輸入法", 790, "phrase3"),
            ("26300", "台灣人", 770, "phrase3"),
            ("41300", "臺灣人", 755, "phrase3"),
            ("30203", "為什麼", 773, "phrase3"),
            ("622612", "四角號碼", 820, "phrase4"),
            ("622672", "四角號碼", 785, "phrase4"),
            ("107164", "一舉兩得", 760, "phrase4"),
        ];

        for &(code, text, weight, kind) in defaults {
            self.insert_entry(code, text, weight, kind);
        }
        self.finalize_buckets();
    }

    fn insert_entry(&mut self, code: &str, text: &str, weight: i32, kind: &str) {
        let bucket = self.code_to_entries.entry(code.to_string()).or_default();
        if let Some(existing) = bucket.iter_mut().find(|e| e.text == text) {
            if weight > existing.weight {
                existing.weight = weight;
            }
            if kind == "full" || (kind == "phrase22" && existing.kind != "full") {
                existing.kind = kind.to_string();
            }
        } else {
            bucket.push(FourCornerEntry {
                code: code.to_string(),
                text: text.to_string(),
                weight,
                kind: kind.to_string(),
            });
        }

        let char_count = text.chars().count();
        if char_count == 1 && (kind == "full" || kind == "variant") {
            if let Some(ch) = text.chars().next() {
                if code.len() >= 4 && code.chars().all(|c| c.is_ascii_digit()) {
                    let codes = self.char_to_codes.entry(ch).or_default();
                    if kind == "full" {
                        if !codes.contains(&code.to_string()) {
                            codes.insert(0, code.to_string());
                        }
                    } else if !codes.contains(&code.to_string()) {
                        codes.push(code.to_string());
                    }

                    let p2 = code[..2].to_string();
                    let prefixes = self.char_to_prefix2.entry(ch).or_default();
                    if !prefixes.contains(&p2) {
                        prefixes.push(p2);
                    }
                }
            }
        }
    }

    fn finalize_buckets(&mut self) {
        for bucket in self.code_to_entries.values_mut() {
            bucket.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.text.cmp(&b.text)));
        }
    }

    /// 載入外部 `four_corner.tsv` 碼表
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        let file = File::open(path.as_ref()).map_err(|e| format!("無法開啟四角號碼碼表: {}", e))?;
        let reader = BufReader::new(file);
        let mut count = 0usize;

        // 清空舊表以確保外部 TSV 權重調校（包含降權與排序更新）100% 覆蓋生效
        self.code_to_entries.clear();
        self.char_to_codes.clear();
        self.char_to_prefix2.clear();

        for line_res in reader.lines() {
            let line = line_res.map_err(|e| format!("讀取碼表行失敗: {}", e))?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() < 2 {
                continue;
            }
            let code = parts[0].trim();
            let text = parts[1].trim();
            if code.is_empty() || text.is_empty() {
                continue;
            }
            let weight: i32 = parts
                .get(2)
                .and_then(|s| s.trim().parse::<i32>().ok())
                .unwrap_or(500);
            let kind = parts.get(3).map(|s| s.trim()).unwrap_or("full");

            self.insert_entry(code, text, weight, kind);
            count += 1;
        }

        self.finalize_buckets();
        Ok(count)
    }

    /// 精確查表
    pub fn lookup_exact(&self, code: &str) -> Vec<FourCornerEntry> {
        self.code_to_entries.get(code).cloned().unwrap_or_default()
    }

    /// 取得 `.` 標點符號選單之完整 3 頁 30 組常用全形標點與成對括號／符號
    pub fn default_punctuation_menu() -> Vec<String> {
        vec![
            // 第 1 頁 (1~0)：核心高頻句讀與引號括號
            "，".to_string(),
            "。".to_string(),
            "、".to_string(),
            "？".to_string(),
            "！".to_string(),
            "：".to_string(),
            "；".to_string(),
            "「」".to_string(),
            "（）".to_string(),
            "……".to_string(),
            // 第 2 頁 (+ 翻下頁後 1~0)：單邊引號、書名號、方括號、破折號與連接號
            "「".to_string(),
            "」".to_string(),
            "『』".to_string(),
            "《》".to_string(),
            "【】".to_string(),
            "——".to_string(),
            "～".to_string(),
            "·".to_string(),
            "（".to_string(),
            "）".to_string(),
            // 第 3 頁 (+ 再翻下頁後 1~0)：單書名號、中大括號與常用數學／商務符號
            "〈〉".to_string(),
            "［］".to_string(),
            "｛｝".to_string(),
            "＋".to_string(),
            "－".to_string(),
            "×".to_string(),
            "÷".to_string(),
            "＝".to_string(),
            "％".to_string(),
            "＠".to_string(),
        ]
    }

    /// 取得 `-` 詞彙前導模式（尚未輸入數字時）之 20 組高頻多字詞預覽清單
    pub fn default_word_mode_menu() -> Vec<String> {
        vec![
            // 第 1 頁 (1~0)
            "台灣".to_string(),
            "輸入法".to_string(),
            "四角號碼".to_string(),
            "主權".to_string(),
            "電腦".to_string(),
            "天才".to_string(),
            "為什麼".to_string(),
            "如喜".to_string(),
            "我們".to_string(),
            "自己".to_string(),
            // 第 2 頁 (+ 翻下頁後 1~0)
            "國家".to_string(),
            "知道".to_string(),
            "智慧".to_string(),
            "時間".to_string(),
            "可以".to_string(),
            "現在".to_string(),
            "這個".to_string(),
            "因為".to_string(),
            "所以".to_string(),
            "一舉兩得".to_string(),
        ]
    }

    /// 供 TSF 輸入座對已收攏之字／詞區塊按 `/` 或 `↓` 重新展開候選清單時調用：
    /// 自動識別 `-` 詞前導模式、`.` 標點模式與萬用字元，並嚴格執行「單字 vs. 多字詞」分流過濾。
    pub fn lookup_for_reselect(&self, raw_code: &str) -> Vec<String> {
        let trimmed = raw_code.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        if trimmed == "." {
            return Self::default_punctuation_menu();
        }
        if trimmed == "-" {
            return Self::default_word_mode_menu();
        }

        let is_word_mode = trimmed.starts_with('-');
        let code_str = trimmed.trim_start_matches('-');
        if code_str.is_empty() {
            return Vec::new();
        }

        let has_wildcard =
            code_str.contains('*') || code_str.contains('?') || code_str.contains('.');
        let mut raw_entries = if has_wildcard {
            self.lookup_wildcard(code_str, 48)
        } else {
            self.lookup_exact(code_str)
        };

        let has_exact_single = raw_entries.iter().any(|e| e.text.chars().count() == 1);
        if is_word_mode {
            raw_entries.retain(|e| e.text.chars().count() >= 2);
        } else if has_exact_single {
            raw_entries.retain(|e| e.text.chars().count() == 1);
        }

        if !has_wildcard && code_str.len() >= 4 && raw_entries.len() < 12 {
            let fuzzy_hits = self.lookup_fuzzy(code_str, 20);
            for h in fuzzy_hits {
                let word_len = h.text.chars().count();
                if is_word_mode && word_len < 2 {
                    continue;
                }
                if !is_word_mode && has_exact_single && word_len != 1 {
                    continue;
                }
                if !raw_entries.iter().any(|e| e.text == h.text) {
                    raw_entries.push(h);
                }
            }
        }

        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for e in raw_entries {
            if seen.insert(e.text.clone()) {
                out.push(e.text);
                if out.len() >= 36 {
                    break;
                }
            }
        }
        out
    }

    /// 前綴聯想查詢（當輸入 1~3 碼或 4 碼欲補附角時提供即時高頻預覽）
    pub fn lookup_with_prefix(&self, prefix: &str, limit: usize) -> Vec<FourCornerEntry> {
        if prefix.is_empty() {
            return Vec::new();
        }
        let mut exact = self.lookup_exact(prefix);
        let mut seen: HashSet<String> = exact.iter().map(|e| e.text.clone()).collect();

        let mut prefix_hits: Vec<FourCornerEntry> = Vec::new();
        for (k, bucket) in &self.code_to_entries {
            if k.len() > prefix.len() && k.starts_with(prefix) {
                // 取每個桶前 6 高頻項以兼顧單字與多字詞前綴預覽
                for entry in bucket.iter().take(6) {
                    prefix_hits.push(entry.clone());
                }
            }
        }

        prefix_hits.sort_by(|a, b| {
            b.weight
                .cmp(&a.weight)
                .then_with(|| a.code.len().cmp(&b.code.len()))
        });

        for hit in prefix_hits {
            if exact.len() >= limit {
                break;
            }
            if seen.insert(hit.text.clone()) {
                exact.push(hit);
            }
        }
        exact.truncate(limit);
        exact
    }

    /// 萬用字元查詢（支援 `*` 或 `?` 代表任意一角號碼）
    pub fn lookup_wildcard(&self, pattern: &str, limit: usize) -> Vec<FourCornerEntry> {
        if pattern.is_empty() {
            return Vec::new();
        }
        let pat_chars: Vec<char> = pattern.chars().collect();
        let pat_len = pat_chars.len();

        let mut matches: Vec<FourCornerEntry> = Vec::new();
        for (code, bucket) in &self.code_to_entries {
            let code_chars: Vec<char> = code.chars().collect();
            // 支援等長匹配，或當 pattern 為 4 碼時同步匹配 5 碼含附角條目
            if code_chars.len() != pat_len && !(pat_len == 4 && code_chars.len() == 5) {
                continue;
            }
            let is_match = pat_chars.iter().enumerate().all(|(i, &pc)| {
                pc == '*' || pc == '?' || pc == '.' || code_chars.get(i).copied() == Some(pc)
            });
            if is_match {
                for entry in bucket.iter().take(5) {
                    matches.push(entry.clone());
                }
            }
        }

        matches.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.code.cmp(&b.code)));

        let mut deduped: Vec<FourCornerEntry> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for m in matches {
            if seen.insert(m.text.clone()) {
                deduped.push(m);
                if deduped.len() >= limit {
                    break;
                }
            }
        }
        deduped
    }

    /// 動態模糊四角救援查詢（1-Hamming 鄰近易混淆筆畫與免補 0 容錯）
    pub fn lookup_fuzzy(&self, code: &str, limit: usize) -> Vec<FourCornerEntry> {
        if code.len() < 4 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Vec::new();
        }
        let chars: Vec<char> = code.chars().collect();
        let mut cand_codes: Vec<(String, i32)> = Vec::new();

        // 1. 若使用者未補 0（例如打了重複角碼），嘗試將右上角或右下角替換為 '0'
        if chars.len() >= 4 {
            if chars[1] != '0' {
                let mut c = chars.clone();
                c[1] = '0';
                cand_codes.push((c.into_iter().collect(), 20));
            }
            if chars[3] != '0' {
                let mut c = chars.clone();
                c[3] = '0';
                cand_codes.push((c.into_iter().collect(), 18));
            }
        }

        // 2. 對每個角位套用鄰近易混淆筆畫替換 (1-Hamming distance，支援 4~6 碼單字與多字詞)
        for pos in 0..chars.len().min(6) {
            for &alt_d in neighbor_fuzzy_digits(chars[pos]) {
                let mut c = chars.clone();
                c[pos] = alt_d;
                let alt_code: String = c.into_iter().collect();
                // 末兩角的模糊容錯懲罰低於首兩角
                let penalty = if pos >= 2 { 35 } else { 50 };
                cand_codes.push((alt_code, penalty));
            }
        }

        // 3. 左右角順序誤調救援（如左上/右上顛倒，或左下/右下顛倒）
        if chars.len() == 4 {
            if chars[0] != chars[1] {
                cand_codes.push((
                    format!("{}{}{}{}", chars[1], chars[0], chars[2], chars[3]),
                    45,
                ));
            }
            if chars[2] != chars[3] {
                cand_codes.push((
                    format!("{}{}{}{}", chars[0], chars[1], chars[3], chars[2]),
                    40,
                ));
            }
        }

        let mut fuzzy_hits: Vec<FourCornerEntry> = Vec::new();
        for (alt_code, penalty) in cand_codes {
            if let Some(bucket) = self.code_to_entries.get(&alt_code) {
                for entry in bucket.iter().take(4) {
                    let mut cloned = entry.clone();
                    cloned.weight = (cloned.weight - penalty).max(50);
                    cloned.kind = "fuzzy".to_string();
                    fuzzy_hits.push(cloned);
                }
            }
        }

        fuzzy_hits.sort_by(|a, b| b.weight.cmp(&a.weight));

        let mut result: Vec<FourCornerEntry> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for hit in fuzzy_hits {
            if seen.insert(hit.text.clone()) {
                result.push(hit);
                if result.len() >= limit {
                    break;
                }
            }
        }
        result
    }

    /// 檢查單一字元是否具備指定之 2 碼前兩角前綴
    pub fn char_matches_prefix2(&self, ch: char, p2: &str) -> bool {
        self.char_to_prefix2
            .get(&ch)
            .map(|list| list.iter().any(|x| x == p2))
            .unwrap_or(false)
    }

    /// 檢查單一字元之主四角碼是否匹配指定角位組合（如 1,3 角、1 角、2,4 角）
    pub fn char_matches_corners(&self, ch: char, i1: usize, i2: Option<usize>, target: &str) -> bool {
        let Some(codes) = self.char_to_codes.get(&ch) else {
            return false;
        };
        let t_chars: Vec<char> = target.chars().collect();
        for c in codes {
            let dc: Vec<char> = c.chars().collect();
            if dc.len() < 4 {
                continue;
            }
            if let Some(idx2) = i2 {
                if t_chars.len() == 2 && dc.get(i1) == Some(&t_chars[0]) && dc.get(idx2) == Some(&t_chars[1]) {
                    return true;
                }
            } else if t_chars.len() == 1 && dc.get(i1) == Some(&t_chars[0]) {
                return true;
            }
        }
        false
    }

    /// 檢查多字詞是否符合分層四角構詞規則：
    /// - 二字詞 (4 碼): `首字(1,2) + 尾字(1,2)`
    /// - 三字詞以上 (N+2 碼): `首字(1,3) + [中間各字(1) × (N-2)] + 尾字(2,4)`
    pub fn word_matches_code(&self, word: &str, code: &str) -> bool {
        let wchars: Vec<char> = word.chars().collect();
        let wlen = wchars.len();
        if wlen == 2 && code.len() == 4 {
            return self.char_matches_prefix2(wchars[0], &code[0..2])
                && self.char_matches_prefix2(wchars[1], &code[2..4]);
        }
        if wlen >= 3 && code.len() == wlen + 2 {
            if !self.char_matches_corners(wchars[0], 0, Some(2), &code[0..2]) {
                return false;
            }
            for i in 1..(wlen - 1) {
                let slice = &code[(i + 1)..(i + 2)];
                if !self.char_matches_corners(wchars[i], 0, None, slice) {
                    return false;
                }
            }
            let tail_slice = &code[wlen..(wlen + 2)];
            return self.char_matches_corners(wchars[wlen - 1], 1, Some(3), tail_slice);
        }
        false
    }

    /// 取得單字或詞彙的四角拆碼教練提示字串
    /// 例如：
    /// - 單字 `台` -> `Some("2360 [2垂·3點·6方·0頭]")`
    /// - 二字詞 `台灣` -> `Some("-2332 [二字詞(-首1,2+尾1,2): 台23+灣32]")`
    /// - 三字詞 `輸入法` -> `Some("-50843 [三字詞(-首1,3+中1+尾2,4): 輸50+入8+法43]")`
    /// - 四字詞 `四角號碼` -> `Some("-622612 [四字詞(-首1,3+中1×2+尾2,4): 四62+角2+號6+碼12]")`
    pub fn format_text_coach(&self, text: &str) -> Option<String> {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() == 1 {
            self.format_char_coach(chars[0])
        } else if chars.len() == 2 {
            let c1 = chars[0];
            let c2 = chars[1];
            let p1 = self.char_to_prefix2.get(&c1)?.first()?;
            let p2 = self.char_to_prefix2.get(&c2)?.first()?;
            Some(format!(
                "-{}{} [二字詞(-首1,2+尾1,2): {}{}+{}{}]",
                p1, p2, c1, p1, c2, p2
            ))
        } else if chars.len() >= 3 {
            let n = chars.len();
            let first_code: Vec<char> = self.char_to_codes.get(&chars[0])?.first()?.chars().collect();
            let last_code: Vec<char> = self.char_to_codes.get(&chars[n - 1])?.first()?.chars().collect();
            if first_code.len() < 4 || last_code.len() < 4 {
                return None;
            }
            let p13 = format!("{}{}", first_code[0], first_code[2]);
            let t24 = format!("{}{}", last_code[1], last_code[3]);
            let mut full_code = p13.clone();
            let mut breakdown_parts = vec![format!("{}{}", chars[0], p13)];
            for &ch_mid in &chars[1..(n - 1)] {
                let mid_code: Vec<char> = self.char_to_codes.get(&ch_mid)?.first()?.chars().collect();
                let m1 = *mid_code.first()?;
                full_code.push(m1);
                breakdown_parts.push(format!("{}{}", ch_mid, m1));
            }
            full_code.push_str(&t24);
            breakdown_parts.push(format!("{}{}", chars[n - 1], t24));
            let rule_label = if n == 3 {
                "三字詞(-首1,3+中1+尾2,4)".to_string()
            } else {
                format!("{}字詞(-首1,3+中1×{}+尾2,4)", n, n - 2)
            };
            Some(format!(
                "-{} [{}: {}]",
                full_code,
                rule_label,
                breakdown_parts.join("+")
            ))
        } else {
            None
        }
    }

    /// 取得單一漢字之四角號碼與四角筆形拆解說明
    pub fn format_char_coach(&self, ch: char) -> Option<String> {
        let codes = self.char_to_codes.get(&ch)?;
        let primary = codes.first()?;
        let digits: Vec<char> = primary.chars().collect();
        if digits.len() < 4 {
            return None;
        }
        let breakdown = format!(
            "{}{}·{}{}·{}{}·{}{}",
            digits[0],
            digit_stroke_short_name(digits[0]),
            digits[1],
            digit_stroke_short_name(digits[1]),
            digits[2],
            digit_stroke_short_name(digits[2]),
            digits[3],
            digit_stroke_short_name(digits[3]),
        );
        if digits.len() >= 5 {
            Some(format!(
                "{}.{} [{} + 附{}{}]",
                &primary[..4],
                digits[4],
                breakdown,
                digits[4],
                digit_stroke_short_name(digits[4])
            ))
        } else {
            Some(format!("{} [{}]", primary, breakdown))
        }
    }
}
