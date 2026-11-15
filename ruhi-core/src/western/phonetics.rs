/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/phonetics.rs - 表驅動泛歐語音韻近似編碼與前綴補全引擎 (Pan-European Phonetics)
///
/// 採宣告式規格結構（`PhoneticProfile` / `G2pClusterRule` / `PhoneticConfusionPair`）設計，
/// 支援非母語學習者以聽覺近似拼音進行整詞勘誤、前綴補全與跨歐語容錯檢索。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use super::lexicon::WesternLexicon;
use super::WesternEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G2pPosition {
    Onset,
    WordFinal,
    Anywhere,
}

#[derive(Debug, Clone, Copy)]
pub struct G2pClusterRule {
    pub grapheme: &'static str,
    pub pos: G2pPosition,
    pub primary: &'static str,
    pub alt: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct PhoneticConfusionPair {
    pub a: char,
    pub b: char,
    pub cost: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct PhoneticProfile {
    pub name: &'static str,
    pub silent_h_prefixes: &'static [&'static str],
    pub cluster_rules: &'static [G2pClusterRule],
    pub confusions: &'static [PhoneticConfusionPair],
}

/// 泛歐語（英／法／德／西及希臘拉丁學術詞根）預設宣告式音韻規格表
pub const DEFAULT_PAN_EUROPEAN_PROFILE: PhoneticProfile = PhoneticProfile {
    name: "pan_european_l2",
    silent_h_prefixes: &["honest", "honor", "honour", "hour", "heir"],
    cluster_rules: &[
        // 字首靜音群與特殊起始組合（依長度降序排列）
        G2pClusterRule { grapheme: "aisl", pos: G2pPosition::Onset, primary: "YL", alt: None },
        G2pClusterRule { grapheme: "isl", pos: G2pPosition::Onset, primary: "YL", alt: None },
        G2pClusterRule { grapheme: "who", pos: G2pPosition::Onset, primary: "H", alt: None },
        G2pClusterRule { grapheme: "wh", pos: G2pPosition::Onset, primary: "W", alt: None },
        G2pClusterRule { grapheme: "kn", pos: G2pPosition::Onset, primary: "N", alt: None },
        G2pClusterRule { grapheme: "gn", pos: G2pPosition::Onset, primary: "N", alt: None },
        G2pClusterRule { grapheme: "pn", pos: G2pPosition::Onset, primary: "N", alt: None },
        G2pClusterRule { grapheme: "mn", pos: G2pPosition::Onset, primary: "N", alt: None },
        G2pClusterRule { grapheme: "wr", pos: G2pPosition::Onset, primary: "R", alt: None },
        G2pClusterRule { grapheme: "rh", pos: G2pPosition::Onset, primary: "R", alt: None },
        G2pClusterRule { grapheme: "ps", pos: G2pPosition::Onset, primary: "S", alt: None },
        G2pClusterRule { grapheme: "pt", pos: G2pPosition::Onset, primary: "T", alt: None },
        // 詞尾靜音與歐語特定尾綴
        G2pClusterRule { grapheme: "coup", pos: G2pPosition::WordFinal, primary: "KU", alt: None },
        G2pClusterRule { grapheme: "mb", pos: G2pPosition::WordFinal, primary: "M", alt: None },
        G2pClusterRule { grapheme: "mn", pos: G2pPosition::WordFinal, primary: "M", alt: None },
        // 詞中／詞尾常見音節與拼寫群組（5 字元 -> 2 字元長匹配優先）
        G2pClusterRule { grapheme: "cious", pos: G2pPosition::Anywhere, primary: "XES", alt: None },
        G2pClusterRule { grapheme: "tious", pos: G2pPosition::Anywhere, primary: "XES", alt: None },
        G2pClusterRule { grapheme: "xious", pos: G2pPosition::Anywhere, primary: "XES", alt: None },
        G2pClusterRule { grapheme: "sious", pos: G2pPosition::Anywhere, primary: "XES", alt: None },
        G2pClusterRule { grapheme: "tcher", pos: G2pPosition::Anywhere, primary: "XER", alt: None },
        G2pClusterRule { grapheme: "ceipt", pos: G2pPosition::Anywhere, primary: "SIT", alt: None },
        G2pClusterRule { grapheme: "tion", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "sion", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "cion", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "shun", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "zhun", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "cean", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "cian", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "tian", pos: G2pPosition::Anywhere, primary: "XEN", alt: None },
        G2pClusterRule { grapheme: "shus", pos: G2pPosition::Anywhere, primary: "XES", alt: None },
        G2pClusterRule { grapheme: "ture", pos: G2pPosition::Anywhere, primary: "XER", alt: None },
        G2pClusterRule { grapheme: "cher", pos: G2pPosition::Anywhere, primary: "XER", alt: None },
        G2pClusterRule { grapheme: "sure", pos: G2pPosition::Anywhere, primary: "JER", alt: None },
        G2pClusterRule { grapheme: "zure", pos: G2pPosition::Anywhere, primary: "JER", alt: None },
        G2pClusterRule { grapheme: "ough", pos: G2pPosition::Anywhere, primary: "AF", alt: Some("O") },
        G2pClusterRule { grapheme: "augh", pos: G2pPosition::Anywhere, primary: "AF", alt: Some("O") },
        G2pClusterRule { grapheme: "sten", pos: G2pPosition::Anywhere, primary: "SEN", alt: None },
        G2pClusterRule { grapheme: "stl", pos: G2pPosition::Anywhere, primary: "SEL", alt: None },
        G2pClusterRule { grapheme: "dge", pos: G2pPosition::Anywhere, primary: "J", alt: None },
        G2pClusterRule { grapheme: "igh", pos: G2pPosition::Anywhere, primary: "Y", alt: None },
        G2pClusterRule { grapheme: "eau", pos: G2pPosition::Anywhere, primary: "O", alt: None },
        G2pClusterRule { grapheme: "sch", pos: G2pPosition::Anywhere, primary: "SK", alt: Some("X") },
        G2pClusterRule { grapheme: "ph", pos: G2pPosition::Anywhere, primary: "F", alt: None },
        G2pClusterRule { grapheme: "sh", pos: G2pPosition::Anywhere, primary: "X", alt: None },
        G2pClusterRule { grapheme: "ch", pos: G2pPosition::Anywhere, primary: "X", alt: Some("K") },
        G2pClusterRule { grapheme: "th", pos: G2pPosition::Anywhere, primary: "T", alt: None },
        G2pClusterRule { grapheme: "ck", pos: G2pPosition::Anywhere, primary: "K", alt: None },
        G2pClusterRule { grapheme: "cq", pos: G2pPosition::Anywhere, primary: "K", alt: None },
        G2pClusterRule { grapheme: "qu", pos: G2pPosition::Anywhere, primary: "K", alt: None },
    ],
    confusions: &[
        PhoneticConfusionPair { a: 'L', b: 'R', cost: 0.35 },
        PhoneticConfusionPair { a: 'V', b: 'W', cost: 0.35 },
        PhoneticConfusionPair { a: 'F', b: 'V', cost: 0.35 },
        PhoneticConfusionPair { a: 'G', b: 'J', cost: 0.35 },
        PhoneticConfusionPair { a: 'S', b: 'X', cost: 0.35 },
        PhoneticConfusionPair { a: 'P', b: 'B', cost: 0.35 },
        PhoneticConfusionPair { a: 'T', b: 'D', cost: 0.35 },
        PhoneticConfusionPair { a: 'K', b: 'G', cost: 0.35 },
        PhoneticConfusionPair { a: 'M', b: 'N', cost: 0.35 },
    ],
};

/// 現代日常高頻 CEFR A1–B2 詞彙之音韻檢索保底權重（彌補文學語料庫中現代詞權重偏低之結構偏差）
pub fn modern_l2_floor_weight(word_lower: &str) -> f32 {
    match word_lower {
        "photo" => 78.0,
        "photograph" => 74.0,
        "photography" => 72.0,
        "photographer" => 70.0,
        "phone" => 82.0,
        "schedule" => 78.0,
        "restaurant" => 78.0,
        "receipt" => 72.0,
        "castle" => 70.0,
        "knight" => 68.0,
        "island" => 74.0,
        "foreign" => 76.0,
        "grammar" => 76.0,
        "rhythm" => 72.0,
        "pronunciation" => 74.0,
        "accommodation" => 72.0,
        "chemistry" => 74.0,
        "chemist" => 70.0,
        "architect" => 74.0,
        "architecture" => 78.0,
        "colleague" => 76.0,
        "college" => 78.0,
        "university" => 80.0,
        "hospital" => 78.0,
        "doctor" => 80.0,
        "medicine" => 76.0,
        "science" => 82.0,
        "physical" => 76.0,
        "physics" => 74.0,
        "psychology" => 76.0,
        "psychological" => 72.0,
        "philosophy" => 76.0,
        "philosophical" => 72.0,
        "phenomenon" => 72.0,
        "character" => 78.0,
        "general" => 80.0,
        "genius" => 72.0,
        "parallel" => 72.0,
        "enough" => 82.0,
        "laugh" => 76.0,
        "listen" => 80.0,
        "honest" => 76.0,
        "honor" => 74.0,
        "hour" => 82.0,
        "wrong" => 84.0,
        "write" => 85.0,
        "right" => 88.0,
        "night" => 86.0,
        "knowledge" => 84.0,
        "necessary" => 84.0,
        "definitely" => 84.0,
        "environment" => 86.0,
        "algorithm" => 86.0,
        "friend" => 86.0,
        "artificial" => 78.0,
        "intelligence" => 78.0,
        "europe" => 76.0,
        "bonjour" => 78.0,
        "merci" => 78.0,
        "beaucoup" => 76.0,
        "gracias" => 78.0,
        "entschuldigung" => 74.0,
        _ => 0.0,
    }
}

#[inline]
fn is_vowel_byte(b: u8) -> bool {
    matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

#[inline]
fn is_phon_vowel(c: char) -> bool {
    matches!(c, 'A' | 'E' | 'I' | 'O' | 'U' | 'Y')
}

fn normalize_ascii_letters(word: &str) -> Option<String> {
    let mut out = String::with_capacity(word.len());
    for ch in word.trim().chars() {
        let stripped = WesternEngine::strip_char_diacritic(ch).to_ascii_lowercase();
        if stripped.is_ascii_lowercase() {
            out.push(stripped);
        } else {
            return None;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[derive(Clone, Copy)]
struct EncodeFlags {
    ch_as_k: bool,
    g_hard: bool,
    gh_silent: bool,
    sch_sh: bool,
    du_j: bool,
    ai_as_y: bool,
}

fn encode_single_variant(s: &str, profile: &PhoneticProfile, flags: EncodeFlags) -> (String, String) {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n + 2);
    let mut i = 0usize;

    // 1. 字首靜音群與特殊起始音位處理
    let mut handled_onset = false;
    for rule in profile.cluster_rules {
        if rule.pos == G2pPosition::Onset && s.starts_with(rule.grapheme) {
            out.push_str(rule.primary);
            i = rule.grapheme.len();
            handled_onset = true;
            break;
        }
    }
    if !handled_onset {
        if profile.silent_h_prefixes.iter().any(|&p| s.starts_with(p)) {
            i = 1;
        } else if bytes[0] == b'x' {
            out.push('Z');
            i = 1;
        }
    }

    // 2. 主掃描迴圈
    while i < n {
        let rem = &s[i..];

        // 2a. 詞尾定型規則
        let mut matched_cluster = false;
        for rule in profile.cluster_rules {
            if rule.pos == G2pPosition::WordFinal && rem == rule.grapheme {
                out.push_str(rule.primary);
                i += rule.grapheme.len();
                matched_cluster = true;
                break;
            }
        }
        if matched_cluster {
            continue;
        }

        // 2b. 長匹配優先群組規則
        for rule in profile.cluster_rules {
            if rule.pos == G2pPosition::Anywhere && rem.starts_with(rule.grapheme) {
                let chosen = match rule.grapheme {
                    "ough" | "augh" if flags.gh_silent => rule.alt.unwrap_or(rule.primary),
                    "sch" if flags.sch_sh => rule.alt.unwrap_or(rule.primary),
                    "ch" if flags.ch_as_k => rule.alt.unwrap_or(rule.primary),
                    _ => rule.primary,
                };
                out.push_str(chosen);
                i += rule.grapheme.len();
                matched_cluster = true;
                break;
            }
        }
        if matched_cluster {
            continue;
        }

        // 2c. 顎化 du (如 schedule -> skejul)
        if flags.du_j && rem.len() >= 3 && rem.starts_with("du") {
            let third = rem.as_bytes()[2];
            if matches!(third, b'l' | b'a' | b'e' | b'i' | b'o') {
                out.push_str("JU");
                i += 2;
                continue;
            }
        }

        // 2d. 條件式子音組合 (sc+e/i/y, gh, 詞中/尾 gn)
        if rem.len() >= 3 && rem.starts_with("sc") && matches!(rem.as_bytes()[2], b'e' | b'i' | b'y') {
            out.push('S');
            i += 2;
            continue;
        }
        if rem.starts_with("gh") {
            if i == 0 {
                out.push('G');
            }
            i += 2;
            continue;
        }
        if i > 0
            && rem.starts_with("gn")
            && matches!(rem, "gn" | "gns" | "gned" | "gning" | "gner")
        {
            out.push('N');
            i += 2;
            continue;
        }

        // 2e. 母音二合字母等價群
        if rem.starts_with("ai") || rem.starts_with("ay") || rem.starts_with("ei") || rem.starts_with("ey") {
            if (rem.starts_with("ai") && flags.ai_as_y)
                || (rem.len() >= 3 && matches!(rem.as_bytes()[2], b'a' | b'e' | b'o' | b'u'))
            {
                out.push('Y');
            } else {
                out.push('E');
            }
            i += 2;
            continue;
        }
        if rem.starts_with("ee") || rem.starts_with("ea") || rem.starts_with("ie") {
            if rem.starts_with("ie") && s.starts_with("scien") {
                out.push_str("YE");
            } else {
                out.push('I');
            }
            i += 2;
            continue;
        }
        if rem.starts_with("oa") || rem.starts_with("ow") || rem.starts_with("au") || rem.starts_with("aw") {
            out.push('O');
            i += 2;
            continue;
        }
        if rem.starts_with("oo") || rem.starts_with("ou") || rem.starts_with("ew") || rem.starts_with("eu") {
            out.push('U');
            i += 2;
            continue;
        }

        // 2f. 單字母母音與 magic-e
        let b = bytes[i];
        let nxt = if i + 1 < n { Some(bytes[i + 1]) } else { None };
        if is_vowel_byte(b) {
            if b == b'e' && i == n - 1 && n >= 4 {
                i += 1;
                continue;
            }
            let is_magic_e = i + 2 == n - 1
                && bytes[n - 1] == b'e'
                && nxt.map_or(false, |nc| !is_vowel_byte(nc));
            match b {
                b'i' | b'y' => {
                    if i == 0 && b == b'y' && nxt.map_or(false, |nc| matches!(nc, b'a' | b'e' | b'i' | b'o' | b'u')) {
                        out.push('J');
                    } else if is_magic_e || (b == b'y' && i > 0 && i + 1 < n) {
                        out.push('Y');
                    } else {
                        out.push('I');
                    }
                }
                b'a' => out.push(if is_magic_e { 'E' } else { 'A' }),
                b'e' => out.push('E'),
                b'o' => out.push('O'),
                b'u' => out.push('U'),
                _ => {}
            }
            i += 1;
            continue;
        }

        // 2g. 單子音對應
        match b {
            b'w' => {
                if nxt.map_or(false, is_vowel_byte) {
                    out.push('W');
                }
            }
            b'h' => {
                if nxt.map_or(false, is_vowel_byte) {
                    out.push('H');
                }
            }
            b'c' => {
                if nxt.map_or(false, |nc| matches!(nc, b'e' | b'i' | b'y')) {
                    out.push('S');
                } else {
                    out.push('K');
                }
            }
            b'g' => {
                if nxt.map_or(false, |nc| matches!(nc, b'e' | b'i' | b'y')) {
                    out.push(if flags.g_hard { 'G' } else { 'J' });
                } else {
                    out.push('G');
                }
            }
            b'k' | b'q' => out.push('K'),
            b'x' => out.push_str("KS"),
            b'z' => out.push('S'),
            b'j' => out.push('J'),
            b'b' | b'f' | b'p' | b'v' | b'd' | b't' | b'l' | b'r' | b'm' | b'n' | b's' => {
                out.push((b as char).to_ascii_uppercase());
            }
            _ => {}
        }
        i += 1;
    }

    // 3. 去重相鄰相同音素構成 `pronunciation_signature`，再提煉子音骨架 `consonant_skeleton`
    let mut pron = String::with_capacity(out.len());
    let mut last_p: Option<char> = None;
    for ch in out.chars() {
        if Some(ch) != last_p {
            pron.push(ch);
            last_p = Some(ch);
        }
    }

    let mut skel = String::with_capacity(pron.len());
    let mut last_s: Option<char> = None;
    for (idx, ch) in pron.chars().enumerate() {
        if is_phon_vowel(ch) {
            if idx == 0 {
                skel.push('A');
                last_s = Some('A');
            }
        } else if Some(ch) != last_s {
            skel.push(ch);
            last_s = Some(ch);
        }
    }

    (skel, pron)
}

#[inline]
fn has_soft_g_candidate(s: &str) -> bool {
    let bytes = s.as_bytes();
    for idx in 0..bytes.len().saturating_sub(1) {
        if bytes[idx] == b'g' && matches!(bytes[idx + 1], b'e' | b'i' | b'y') {
            return true;
        }
    }
    false
}

/// 將任意泛歐文單字或近似拼音輸入編碼為 `(consonant_skeleton, pronunciation_signature)` 多變體清單
pub fn encode_phonetic_forms(word: &str) -> Vec<(String, String)> {
    encode_phonetic_forms_with_profile(word, &DEFAULT_PAN_EUROPEAN_PROFILE)
}

pub fn encode_phonetic_forms_with_profile(word: &str, profile: &PhoneticProfile) -> Vec<(String, String)> {
    let Some(s) = normalize_ascii_letters(word) else {
        return Vec::new();
    };
    let mut forms: Vec<(String, String)> = Vec::with_capacity(4);
    let ch_opts: &[bool] = if s.contains("ch") { &[false, true] } else { &[false] };
    let g_opts: &[bool] = if has_soft_g_candidate(&s) { &[false, true] } else { &[false] };
    let gh_opts: &[bool] = if s.contains("gh") { &[false, true] } else { &[false] };
    let sch_opts: &[bool] = if s.contains("sch") { &[false, true] } else { &[false] };
    let du_opts: &[bool] = if s.contains("du") { &[false, true] } else { &[false] };
    let ai_opts: &[bool] = if s.contains("ai") { &[false, true] } else { &[false] };

    for &ch_as_k in ch_opts {
        for &g_hard in g_opts {
            for &gh_silent in gh_opts {
                for &sch_sh in sch_opts {
                    for &du_j in du_opts {
                        for &ai_as_y in ai_opts {
                            let pair = encode_single_variant(
                                &s,
                                profile,
                                EncodeFlags {
                                    ch_as_k,
                                    g_hard,
                                    gh_silent,
                                    sch_sh,
                                    du_j,
                                    ai_as_y,
                                },
                            );
                            if !pair.0.is_empty() && !forms.contains(&pair) {
                                forms.push(pair);
                            }
                        }
                    }
                }
            }
        }
    }
    forms
}

#[inline]
fn substitution_cost(ca: char, cb: char, profile: &PhoneticProfile) -> f32 {
    if ca == cb {
        return 0.0;
    }
    let a_vow = is_phon_vowel(ca);
    let b_vow = is_phon_vowel(cb);
    if a_vow && b_vow {
        if matches!((ca, cb), ('I', 'Y') | ('Y', 'I')) {
            return 0.15;
        }
        if matches!(
            (ca, cb),
            ('E', 'Y')
                | ('Y', 'E')
                | ('E', 'I')
                | ('I', 'E')
                | ('O', 'U')
                | ('U', 'O')
                | ('A', 'O')
                | ('O', 'A')
                | ('A', 'E')
                | ('E', 'A')
        ) {
            return 0.35;
        }
        return 0.55;
    }
    for pair in profile.confusions {
        if (pair.a == ca && pair.b == cb) || (pair.b == ca && pair.a == cb) {
            return pair.cost;
        }
    }
    1.10
}

/// 計算兩音韻簽章字串間之加權音韻編輯距離（含母音等價與 L2 易混音素群）
pub fn pronunciation_distance(a: &str, b: &str) -> f32 {
    pronunciation_distance_with_profile(a, b, &DEFAULT_PAN_EUROPEAN_PROFILE)
}

pub fn pronunciation_distance_with_profile(a: &str, b: &str, profile: &PhoneticProfile) -> f32 {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let m = ab.len();
    let n = bb.len();
    if m == 0 {
        return (n as f32) * 0.85;
    }
    if n == 0 {
        return (m as f32) * 0.85;
    }

    let mut prev: Vec<f32> = Vec::with_capacity(n + 1);
    let mut curr: Vec<f32> = vec![0.0; n + 1];
    prev.push(0.0);
    for j in 0..n {
        let cb = bb[j] as char;
        let step = if is_phon_vowel(cb) { 0.45 } else { 0.85 };
        prev.push(prev[j] + step);
    }

    for i in 1..=m {
        let ca = ab[i - 1] as char;
        let del_cost = if is_phon_vowel(ca) { 0.45 } else { 0.95 };
        curr[0] = prev[0] + del_cost;
        for j in 1..=n {
            let cb = bb[j - 1] as char;
            let ins_cost = if is_phon_vowel(cb) { 0.45 } else { 0.95 };
            let sub = substitution_cost(ca, cb, profile);
            curr[j] = (prev[j] + del_cost)
                .min(curr[j - 1] + ins_cost)
                .min(prev[j - 1] + sub);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

#[derive(Debug)]
struct PhoneticEntry {
    word: String,
    eff_weight: f32,
    prons: Vec<String>,
}

/// 唯讀泛歐語音韻倒排與前綴索引（獨立快取於靜態側表，不改變 `WesternLexicon` 結構體佈局）
#[derive(Debug)]
pub struct WesternPhoneticIndex {
    entries: Vec<PhoneticEntry>,
    exact_buckets: HashMap<String, Vec<u32>>,
    prefix_buckets: HashMap<String, Vec<u32>>,
}

impl WesternPhoneticIndex {
    pub fn build(lexicon: &WesternLexicon) -> Self {
        let mut entries: Vec<PhoneticEntry> = Vec::new();
        let mut exact_buckets: HashMap<String, Vec<u32>> = HashMap::new();
        let mut prefix_buckets: HashMap<String, Vec<u32>> = HashMap::new();

        for (w, _freq, weight) in lexicon.iter_words() {
            if w.len() < 3 || !w.chars().all(|c| c.is_alphabetic()) {
                continue;
            }
            let forms = encode_phonetic_forms(w);
            if forms.is_empty() {
                continue;
            }
            let w_low = w.to_ascii_lowercase();
            let eff_weight = weight.max(modern_l2_floor_weight(&w_low));
            let idx = entries.len() as u32;

            let mut prons: Vec<String> = Vec::with_capacity(forms.len());
            let mut seen_sk: Vec<String> = Vec::with_capacity(forms.len());
            let mut seen_pf: Vec<String> = Vec::with_capacity(8);

            for (sk, pr) in forms {
                if !prons.contains(&pr) {
                    prons.push(pr);
                }
                if !seen_sk.contains(&sk) {
                    seen_sk.push(sk.clone());
                    exact_buckets.entry(sk.clone()).or_default().push(idx);
                }
                let max_plen = sk.len().min(6);
                for plen in 2..=max_plen {
                    let pf = sk[..plen].to_string();
                    if !seen_pf.contains(&pf) {
                        seen_pf.push(pf.clone());
                        prefix_buckets.entry(pf).or_default().push(idx);
                    }
                }
            }

            entries.push(PhoneticEntry {
                word: w_low,
                eff_weight,
                prons,
            });
        }

        // 各前綴桶與精確桶依有效權重降序排列，並將前綴桶截斷為前 64 筆以確保常數時間檢索
        for list in exact_buckets.values_mut() {
            list.sort_by(|&a, &b| {
                entries[b as usize]
                    .eff_weight
                    .partial_cmp(&entries[a as usize].eff_weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        for list in prefix_buckets.values_mut() {
            list.sort_by(|&a, &b| {
                entries[b as usize]
                    .eff_weight
                    .partial_cmp(&entries[a as usize].eff_weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if list.len() > 64 {
                list.truncate(64);
            }
        }

        Self {
            entries,
            exact_buckets,
            prefix_buckets,
        }
    }
}

type IndexCacheEntry = (Weak<WesternLexicon>, Arc<WesternPhoneticIndex>);

fn get_or_build_index(lexicon: &Arc<WesternLexicon>) -> Arc<WesternPhoneticIndex> {
    static CACHE: OnceLock<Mutex<HashMap<(usize, usize), IndexCacheEntry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (Arc::as_ptr(lexicon) as usize, lexicon.len());

    if let Ok(guard) = cache.lock() {
        if let Some((weak, idx)) = guard.get(&key) {
            if weak.upgrade().map_or(false, |u| Arc::ptr_eq(&u, lexicon)) {
                return idx.clone();
            }
        }
    }

    let built = Arc::new(WesternPhoneticIndex::build(lexicon));
    if let Ok(mut guard) = cache.lock() {
        if guard.len() >= 8 {
            guard.retain(|_, (weak, _)| weak.strong_count() > 0);
        }
        guard.insert(key, (Arc::downgrade(lexicon), built.clone()));
    }
    built
}

#[inline]
fn is_confusable_skeleton_char(a: u8, b: u8) -> bool {
    matches!(
        (a, b),
        (b'L', b'R')
            | (b'R', b'L')
            | (b'V', b'W')
            | (b'W', b'V')
            | (b'F', b'V')
            | (b'V', b'F')
            | (b'G', b'J')
            | (b'J', b'G')
            | (b'S', b'X')
            | (b'X', b'S')
            | (b'K', b'G')
            | (b'G', b'K')
            | (b'T', b'D')
            | (b'D', b'T')
            | (b'P', b'B')
            | (b'B', b'P')
            | (b'M', b'N')
            | (b'N', b'M')
    )
}

/// 查詢泛歐語音韻近似與前綴補全候選：
/// 回傳 `(word, effective_weight, phonetic_bonus, best_pronunciation_distance)`，依綜合分數降序排列。
pub fn lookup_phonetic_candidates(
    lexicon: &Arc<WesternLexicon>,
    query: &str,
    limit: usize,
) -> Vec<(String, f32, f32, f32)> {
    if limit == 0 {
        return Vec::new();
    }
    let Some(q_norm) = normalize_ascii_letters(query) else {
        return Vec::new();
    };
    let q_char_len = q_norm.chars().count();
    if q_char_len < 3 {
        return Vec::new();
    }

    let q_forms = encode_phonetic_forms(&q_norm);
    if q_forms.is_empty() {
        return Vec::new();
    }

    let index = get_or_build_index(lexicon);
    let mut cand_min_sk_pen: HashMap<u32, f32> = HashMap::new();
    let mut record_cand = |idx: u32, sk_pen: f32| {
        if let Some(prev) = cand_min_sk_pen.get_mut(&idx) {
            if sk_pen < *prev {
                *prev = sk_pen;
            }
        } else {
            cand_min_sk_pen.insert(idx, sk_pen);
        }
    };

    const SKEL_ALPHA: &[u8] = b"ABDFGHJKLMNPRSTVWX";

    let mut q_skels: Vec<String> = Vec::with_capacity(q_forms.len());
    let mut q_prons: Vec<String> = Vec::with_capacity(q_forms.len());
    for (sk, pr) in &q_forms {
        if !q_skels.contains(sk) {
            q_skels.push(sk.clone());
        }
        if !q_prons.contains(pr) {
            q_prons.push(pr.clone());
        }
    }

    for sk in &q_skels {
        // (a) 精確子音骨架桶
        if let Some(list) = index.exact_buckets.get(sk.as_str()) {
            for &idx in list {
                record_cand(idx, 0.0);
            }
        }

        let sb = sk.as_bytes();
        // (b) 1-edit 子音骨架鄰居
        if sb.len() >= 3 {
            let mut buf = String::with_capacity(sb.len() + 1);
            // 刪除單字元
            for del_i in 0..sb.len() {
                buf.clear();
                for (k, &ch) in sb.iter().enumerate() {
                    if k != del_i {
                        buf.push(ch as char);
                    }
                }
                if let Some(list) = index.exact_buckets.get(buf.as_str()) {
                    for &idx in list {
                        record_cand(idx, 1.0);
                    }
                }
            }
            // 單字元替換
            for sub_i in 0..sb.len() {
                for &alt in SKEL_ALPHA {
                    if alt == sb[sub_i] {
                        continue;
                    }
                    buf.clear();
                    for (k, &ch) in sb.iter().enumerate() {
                        buf.push((if k == sub_i { alt } else { ch }) as char);
                    }
                    if let Some(list) = index.exact_buckets.get(buf.as_str()) {
                        for &idx in list {
                            record_cand(idx, 1.0);
                        }
                    }
                }
            }
            // 插入單字元
            for ins_i in 0..=sb.len() {
                for &ins in SKEL_ALPHA {
                    buf.clear();
                    for &ch in &sb[..ins_i] {
                        buf.push(ch as char);
                    }
                    buf.push(ins as char);
                    for &ch in &sb[ins_i..] {
                        buf.push(ch as char);
                    }
                    if let Some(list) = index.exact_buckets.get(buf.as_str()) {
                        for &idx in list {
                            record_cand(idx, 1.0);
                        }
                    }
                }
            }
            // 相鄰互換
            for tr_i in 0..sb.len() - 1 {
                if sb[tr_i] == sb[tr_i + 1] {
                    continue;
                }
                buf.clear();
                for k in 0..sb.len() {
                    let ch = if k == tr_i {
                        sb[tr_i + 1]
                    } else if k == tr_i + 1 {
                        sb[tr_i]
                    } else {
                        sb[k]
                    };
                    buf.push(ch as char);
                }
                if let Some(list) = index.exact_buckets.get(buf.as_str()) {
                    for &idx in list {
                        record_cand(idx, 1.0);
                    }
                }
            }
        } else if sb.len() == 2 {
            // 二子音短詞（如 foto -> FT, rait -> RT）：展開易混子音替換鄰居 (如 FT <-> PT/VT, RT <-> LT)
            let mut buf = [0u8; 2];
            for pos in 0..2 {
                for &alt in SKEL_ALPHA {
                    if is_confusable_skeleton_char(sb[pos], alt) {
                        buf[0] = if pos == 0 { alt } else { sb[0] };
                        buf[1] = if pos == 1 { alt } else { sb[1] };
                        if let Ok(nb) = std::str::from_utf8(&buf) {
                            if let Some(list) = index.exact_buckets.get(nb) {
                                for &idx in list {
                                    record_cand(idx, 0.6);
                                }
                            }
                        }
                    }
                }
            }
        }

        // (c) 音韻前綴補全 (query 長度 >= 4 且骨架長度 >= 2)
        if q_char_len >= 4 && sk.len() >= 2 {
            let pf_key = &sk[..sk.len().min(6)];
            if let Some(list) = index.prefix_buckets.get(pf_key) {
                for &idx in list {
                    let entry = &index.entries[idx as usize];
                    if entry.word.len() > q_norm.len() {
                        record_cand(idx, 0.15);
                    }
                }
            }
        }
    }

    let mut results: Vec<(String, f32, f32, f32)> = Vec::new();
    for (idx, sk_pen) in cand_min_sk_pen {
        let entry = &index.entries[idx as usize];
        if entry.word == q_norm {
            continue;
        }
        let mut best_dist = f32::MAX;
        for qp in &q_prons {
            for wp in &entry.prons {
                let d_full = pronunciation_distance(qp, wp);
                if d_full < best_dist {
                    best_dist = d_full;
                }
                if q_char_len >= 4
                    && entry.word.len() >= q_norm.len() + 2
                    && wp.len() > qp.len()
                {
                    let cut1 = qp.len();
                    let cut2 = (qp.len() + 1).min(wp.len());
                    for cut in [cut1, cut2] {
                        let d_pf = pronunciation_distance(qp, &wp[..cut])
                            + 0.18
                            + (wp.len() - cut) as f32 * 0.035;
                        if d_pf < best_dist {
                            best_dist = d_pf;
                        }
                    }
                }
            }
        }
        if best_dist <= 2.2 {
            let phon_bonus = 24.0 - best_dist * 26.0 - sk_pen * 8.0;
            results.push((entry.word.clone(), entry.eff_weight, phon_bonus, best_dist));
        }
    }

    results.sort_by(|a, b| {
        let ta = a.1 + a.2;
        let tb = b.1 + b.2;
        tb.partial_cmp(&ta)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.0.len().cmp(&b.0.len()))
            .then_with(|| a.0.cmp(&b.0))
    });
    results.truncate(limit);
    results
}
