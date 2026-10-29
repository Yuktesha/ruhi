// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 RuhI Project Contributors
//
//! # `reverse_coach` — 如喜 (RuhI) 通用可擴展跨輸入法反查教練與結構拆解引擎
//!
//! 遵循開放封閉原則 (Open-Closed Principle)，定義統一之 [`ReverseLookupProvider`] 介面：
//! - 任何現有或未來新增之輸入法方案，只需實作 [`ReverseLookupProvider`] 並註冊至 [`UNIVERSAL_REVERSE_PROVIDERS`]，
//!   即可自動與全系統所有輸入法互相反查（N × M 雙向反查），並輸出結構化拆解資訊供互動教學工作台與懸浮提示使用。

use serde::{Deserialize, Serialize};

use crate::array30::{code_to_coords_display, get_key_meta};
use crate::cangjie::table::CangjieTable;
use crate::engine::{EngineMode, RuhiEngine};
use crate::four_corner::digit_stroke_short_name;

/// 鍵盤幾何配置類型（供視覺化工作台渲染對應鍵位動畫）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardGeometry {
    /// 4×5 數字小鍵盤（智慧型四角號碼）
    Numpad4x5,
    /// 30 鍵字根矩陣（行列 30）
    Qwerty30,
    /// 26 鍵英文字母區（許氏雙拼、倉頡、倚天 26 鍵）
    Qwerty26,
    /// 41 鍵標準大千注音排列
    Bopomofo41,
}

/// 單一按鍵拆碼步驟說明
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStepBreakdown {
    /// 實際敲擊之按鍵（如 `"2"`, `"Z"`, `"T"`）
    pub key: String,
    /// 筆形／字根／音韻標籤（如 `"2垂"`, `"18=Z(1v:八/大/雨)"`, `"ㄊ"`）
    pub label: String,
    /// 取碼部位或角色說明（如 `"①左上"`, `"首碼"`, `"聲母"`）
    pub role: String,
}

/// 單一漢字在特定輸入法下之結構化拆解資訊
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharBreakdown {
    pub ch: char,
    pub method_id: String,
    pub method_name: String,
    pub primary_code: String,
    pub alt_codes: Vec<String>,
    pub steps: Vec<KeyStepBreakdown>,
    pub summary_badge: String,
}

/// 單字或多字詞在特定輸入法下之完整反查結果（含詞級簡碼與逐字拆解）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhraseBreakdown {
    pub text: String,
    pub method_id: String,
    pub method_name: String,
    /// 若該輸入法支援多字詞簡碼或連打碼（如四角 `-2332`、許氏聲母簡碼 `tw`），提供最速詞碼
    pub phrase_code: Option<String>,
    /// 詞級編碼構詞規則說明（如 `"二字詞(-首1,2+尾1,2): 台23+灣32"`）
    pub rule_explanation: String,
    /// 逐字拆解清單
    pub chars: Vec<CharBreakdown>,
    /// 供單行提示列顯示之精簡教練字串
    pub coach_line: String,
}

/// 通用跨輸入法反查提供者介面 (Open-Closed Extensible Trait)
pub trait ReverseLookupProvider: Sync {
    /// 輸入法唯一識別碼（如 `"four_corner"`, `"array30"`, `"cangjie"`, `"hsu"`, `"standard"`, `"et26"`）
    fn method_id(&self) -> &'static str;

    /// 輸入法顯示名稱
    fn display_name(&self) -> &'static str;

    /// 單行提示短名（如 `"四角教練"`, `"行列教練"`, `"倉頡教練"`, `"許氏教練"`）
    fn short_coach_title(&self) -> &'static str;

    /// 對應之鍵盤幾何類型
    fn geometry(&self) -> KeyboardGeometry;

    /// 反查單一漢字之結構化拆解
    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown>;

    /// 反查單字或句子之完整結構化拆解
    fn inspect_text(&self, text: &str, engine: &RuhiEngine) -> Option<PhraseBreakdown> {
        let chars_vec: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        if chars_vec.is_empty() {
            return None;
        }
        let mut char_breakdowns = Vec::new();
        let mut summary_parts = Vec::new();
        for &ch in &chars_vec {
            if let Some(cb) = self.inspect_char(ch, engine) {
                if chars_vec.len() == 1 {
                    summary_parts.push(format!("{} ➔ {}", ch, cb.summary_badge));
                } else {
                    summary_parts.push(format!("{}: {}", ch, cb.summary_badge));
                }
                char_breakdowns.push(cb);
            }
        }
        if char_breakdowns.is_empty() {
            return None;
        }
        let joined = summary_parts.join(" ｜ ");
        Some(PhraseBreakdown {
            text: text.to_string(),
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            phrase_code: None,
            rule_explanation: String::new(),
            chars: char_breakdowns,
            coach_line: format!("🎯 {}：{}", self.short_coach_title(), joined),
        })
    }
}

// ============================================================================
// 1. 智慧型四角號碼 (Four-Corner 4×5 Numpad) 反查提供者
// ============================================================================
pub struct FourCornerCoachProvider;

impl ReverseLookupProvider for FourCornerCoachProvider {
    fn method_id(&self) -> &'static str {
        "four_corner"
    }

    fn display_name(&self) -> &'static str {
        "智慧型四角號碼 (4×5 小鍵盤)"
    }

    fn short_coach_title(&self) -> &'static str {
        "四角教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Numpad4x5
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let codes = engine.four_corner.table.char_to_codes.get(&ch)?;
        let primary = codes.first()?.clone();
        let alt_codes: Vec<String> = codes.iter().skip(1).cloned().collect();
        let digits: Vec<char> = primary.chars().collect();
        if digits.len() < 4 {
            return None;
        }
        let roles = ["①左上", "②右上", "③左下", "④右下", "⑤附角"];
        let mut steps = Vec::new();
        for (i, &d) in digits.iter().take(5).enumerate() {
            steps.push(KeyStepBreakdown {
                key: d.to_string(),
                label: format!("{}{}", d, digit_stroke_short_name(d)),
                role: roles.get(i).copied().unwrap_or("角碼").to_string(),
            });
        }
        let summary_badge = engine
            .four_corner
            .table
            .format_char_coach(ch)
            .unwrap_or_else(|| primary.clone());
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: primary,
            alt_codes,
            steps,
            summary_badge,
        })
    }

    fn inspect_text(&self, text: &str, engine: &RuhiEngine) -> Option<PhraseBreakdown> {
        let cjk_chars: Vec<char> = text
            .chars()
            .filter(|&c| engine.four_corner.table.char_to_codes.contains_key(&c))
            .collect();
        if cjk_chars.is_empty() {
            return None;
        }
        let mut char_breakdowns = Vec::new();
        for &ch in &cjk_chars {
            if let Some(cb) = self.inspect_char(ch, engine) {
                char_breakdowns.push(cb);
            }
        }
        let clean_word: String = cjk_chars.iter().collect();
        let coach_detail = engine.four_corner.table.format_text_coach(&clean_word)?;
        let phrase_code = if cjk_chars.len() >= 2 {
            coach_detail
                .split_whitespace()
                .next()
                .map(|s| s.to_string())
        } else {
            char_breakdowns.first().map(|c| c.primary_code.clone())
        };
        let char_summary: Vec<String> = char_breakdowns
            .iter()
            .map(|c| format!("{}({})", c.ch, c.primary_code))
            .collect();
        let coach_line = if cjk_chars.len() == 1 {
            format!("🎯 四角教練：{} ➔ {}", clean_word, coach_detail)
        } else {
            format!(
                "🎯 四角教練：{} ➔ {} ｜ 單字：{}",
                clean_word,
                coach_detail,
                char_summary.join(" · ")
            )
        };
        Some(PhraseBreakdown {
            text: clean_word,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            phrase_code,
            rule_explanation: coach_detail,
            chars: char_breakdowns,
            coach_line,
        })
    }
}

// ============================================================================
// 2. 行列 30 (Array 30) 反查提供者
// ============================================================================
pub struct Array30CoachProvider;

impl ReverseLookupProvider for Array30CoachProvider {
    fn method_id(&self) -> &'static str {
        "array30"
    }

    fn display_name(&self) -> &'static str {
        "行列 30 (筆形座標)"
    }

    fn short_coach_title(&self) -> &'static str {
        "行列教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Qwerty30
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let summary_badge = engine.array30.table.format_char_coach(ch)?;
        let rev = engine.array30.table.reverse_lookup(ch)?;
        let primary = rev
            .full_codes
            .first()
            .cloned()
            .or_else(|| rev.special_code.clone())?;
        let mut alt_codes = Vec::new();
        if let Some(ref sp) = rev.special_code {
            alt_codes.push(format!("特別碼:{}", sp.to_ascii_uppercase()));
        }
        if let Some((ref s1, slot)) = rev.short1 {
            alt_codes.push(format!("一簡:{}{}", s1.to_ascii_uppercase(), slot % 10));
        }
        if let Some((ref s2, slot)) = rev.short2 {
            alt_codes.push(format!("二簡:{}{}", s2.to_ascii_uppercase(), slot % 10));
        }
        let mut steps = Vec::new();
        for (i, k) in primary.chars().enumerate() {
            let label = get_key_meta(k)
                .map(|m| m.coach_label.to_string())
                .unwrap_or_else(|| code_to_coords_display(&k.to_string()));
            steps.push(KeyStepBreakdown {
                key: k.to_ascii_uppercase().to_string(),
                label,
                role: format!("第{}碼", i + 1),
            });
        }
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: primary.to_ascii_uppercase(),
            alt_codes,
            steps,
            summary_badge,
        })
    }

    fn inspect_text(&self, text: &str, engine: &RuhiEngine) -> Option<PhraseBreakdown> {
        let hint = engine.array30.table.format_text_coach(text);
        if hint.is_empty() {
            return None;
        }
        let mut char_breakdowns = Vec::new();
        for ch in text.chars() {
            if let Some(cb) = self.inspect_char(ch, engine) {
                char_breakdowns.push(cb);
            }
        }
        Some(PhraseBreakdown {
            text: text.to_string(),
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            phrase_code: None,
            rule_explanation: hint.clone(),
            chars: char_breakdowns,
            coach_line: format!("🎯 行列教練：{}", hint),
        })
    }
}

// ============================================================================
// 3. 倉頡輸入法 (Cangjie) 反查提供者
// ============================================================================
pub struct CangjieCoachProvider;

impl ReverseLookupProvider for CangjieCoachProvider {
    fn method_id(&self) -> &'static str {
        "cangjie"
    }

    fn display_name(&self) -> &'static str {
        "倉頡輸入法 (哲理字根)"
    }

    fn short_coach_title(&self) -> &'static str {
        "倉頡教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Qwerty26
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let summary_badge = engine.cangjie.table.format_char_coach(ch)?;
        let codes = engine.cangjie.table.char_to_codes.get(&ch)?;
        let mut sorted = codes.clone();
        sorted.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        let primary = sorted.first()?.clone();
        let alt_codes: Vec<String> = sorted
            .iter()
            .skip(1)
            .map(|s| s.to_ascii_uppercase())
            .collect();
        let mut steps = Vec::new();
        for (i, k) in primary.chars().enumerate() {
            let rad = CangjieTable::radical_name(k).unwrap_or("字根");
            steps.push(KeyStepBreakdown {
                key: k.to_ascii_uppercase().to_string(),
                label: format!("{}({})", rad, k.to_ascii_uppercase()),
                role: format!("第{}碼", i + 1),
            });
        }
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: primary.to_ascii_uppercase(),
            alt_codes,
            steps,
            summary_badge,
        })
    }
}

// ============================================================================
// 4. 注音符號與三大音韻鍵盤（許氏雙拼 / 標準大千 / 倚天26鍵）共用轉換工具與提供者
// ============================================================================

/// 查詢單一漢字於詞庫中之首選標準注音（如 `'台'` -> `Some("ㄊㄞˊ")`）
pub fn lookup_char_primary_bopomofo(ch: char, engine: &RuhiEngine) -> Option<String> {
    let s = ch.to_string();
    let ids = engine.lexicon.word_index.get(&s)?;
    let mut best: Option<(u32, &str)> = None;
    for &id in ids {
        if let Some(entry) = engine.lexicon.entries.get(id) {
            if best.map(|(w, _)| entry.weight > w).unwrap_or(true) {
                best = Some((entry.weight, entry.bopomofo.as_str()));
            }
        }
    }
    best.map(|(_, bpm)| bpm.trim().to_string())
}

/// 將單一注音音節（如 `"ㄊㄞˊ"`、`"ㄨㄢ"`）轉譯為許氏雙拼按鍵序列（如 `"tid"`, `"xm "`）
pub fn bopomofo_to_hsu_keys(bpm: &str) -> String {
    let mut clean = bpm.trim().to_string();
    let mut tone_key = " ";
    if clean.starts_with('˙') || clean.ends_with('˙') {
        tone_key = "s";
        clean = clean.replace('˙', "");
    } else if clean.ends_with('ˊ') {
        tone_key = "d";
        clean = clean.replace('ˊ', "");
    } else if clean.ends_with('ˇ') {
        tone_key = "f";
        clean = clean.replace('ˇ', "");
    } else if clean.ends_with('ˋ') {
        tone_key = "j";
        clean = clean.replace('ˋ', "");
    }

    let mut keys = String::new();
    for c in clean.chars() {
        let k = match c {
            'ㄅ' => 'b',
            'ㄆ' => 'p',
            'ㄇ' => 'm',
            'ㄈ' => 'f',
            'ㄉ' => 'd',
            'ㄊ' => 't',
            'ㄋ' => 'n',
            'ㄌ' => 'l',
            'ㄍ' => 'g',
            'ㄎ' => 'k',
            'ㄏ' => 'h',
            'ㄐ' | 'ㄓ' => 'j',
            'ㄑ' | 'ㄔ' => 'v',
            'ㄒ' | 'ㄕ' => 'c',
            'ㄖ' => 'r',
            'ㄗ' => 'z',
            'ㄘ' => 'a',
            'ㄙ' => 's',
            'ㄧ' => 'e',
            'ㄨ' => 'x',
            'ㄩ' => 'u',
            'ㄚ' => 'y',
            'ㄛ' => 'h',
            'ㄜ' => 'g',
            'ㄝ' => 'e',
            'ㄞ' => 'i',
            'ㄟ' => 'a',
            'ㄠ' => 'w',
            'ㄡ' => 'o',
            'ㄢ' => 'm',
            'ㄣ' => 'n',
            'ㄤ' => 'k',
            'ㄥ' | 'ㄦ' => 'l',
            _ => continue,
        };
        keys.push(k);
    }
    if tone_key == " " {
        format!("{}␣", keys.to_ascii_uppercase())
    } else {
        format!("{}{}", keys.to_ascii_uppercase(), tone_key.to_ascii_uppercase())
    }
}

/// 將單一注音音節轉譯為標準大千 41 鍵按鍵序列（如 `"ㄊㄞˊ"` -> `"W96"`）
pub fn bopomofo_to_standard_keys(bpm: &str) -> String {
    let mut clean = bpm.trim().to_string();
    let mut tone_key = "␣";
    if clean.starts_with('˙') || clean.ends_with('˙') {
        tone_key = "7";
        clean = clean.replace('˙', "");
    } else if clean.ends_with('ˊ') {
        tone_key = "6";
        clean = clean.replace('ˊ', "");
    } else if clean.ends_with('ˇ') {
        tone_key = "3";
        clean = clean.replace('ˇ', "");
    } else if clean.ends_with('ˋ') {
        tone_key = "4";
        clean = clean.replace('ˋ', "");
    }

    let mut keys = String::new();
    for c in clean.chars() {
        let k = match c {
            'ㄅ' => '1',
            'ㄆ' => 'Q',
            'ㄇ' => 'A',
            'ㄈ' => 'Z',
            'ㄉ' => '2',
            'ㄊ' => 'W',
            'ㄋ' => 'S',
            'ㄌ' => 'X',
            'ㄍ' => 'E',
            'ㄎ' => 'D',
            'ㄏ' => 'C',
            'ㄐ' => 'R',
            'ㄑ' => 'F',
            'ㄒ' => 'V',
            'ㄓ' => '5',
            'ㄔ' => 'T',
            'ㄕ' => 'G',
            'ㄖ' => 'B',
            'ㄗ' => 'Y',
            'ㄘ' => 'H',
            'ㄙ' => 'N',
            'ㄧ' => 'U',
            'ㄨ' => 'J',
            'ㄩ' => 'M',
            'ㄚ' => '8',
            'ㄛ' => 'I',
            'ㄜ' => 'K',
            'ㄝ' => ',',
            'ㄞ' => '9',
            'ㄟ' => 'O',
            'ㄠ' => 'L',
            'ㄡ' => '.',
            'ㄢ' => '0',
            'ㄣ' => 'P',
            'ㄤ' => ';',
            'ㄥ' => '/',
            'ㄦ' => '-',
            _ => continue,
        };
        keys.push(k);
    }
    format!("{}{}", keys, tone_key)
}

/// 將單一注音音節轉譯為倚天 26 鍵按鍵序列
pub fn bopomofo_to_et26_keys(bpm: &str) -> String {
    let mut clean = bpm.trim().to_string();
    let mut tone_key = "␣";
    if clean.starts_with('˙') || clean.ends_with('˙') {
        tone_key = "Q";
        clean = clean.replace('˙', "");
    } else if clean.ends_with('ˊ') {
        tone_key = "F";
        clean = clean.replace('ˊ', "");
    } else if clean.ends_with('ˇ') {
        tone_key = "J";
        clean = clean.replace('ˇ', "");
    } else if clean.ends_with('ˋ') {
        tone_key = "K";
        clean = clean.replace('ˋ', "");
    }

    let mut keys = String::new();
    for c in clean.chars() {
        let k = match c {
            'ㄅ' => 'B',
            'ㄆ' => 'P',
            'ㄇ' => 'M',
            'ㄈ' => 'F',
            'ㄉ' => 'D',
            'ㄊ' => 'T',
            'ㄋ' => 'N',
            'ㄌ' => 'L',
            'ㄍ' => 'V',
            'ㄎ' => 'K',
            'ㄏ' => 'H',
            'ㄐ' => 'G',
            'ㄑ' => 'C',
            'ㄒ' => 'X',
            'ㄓ' => 'J',
            'ㄔ' => 'Q',
            'ㄕ' => 'W',
            'ㄖ' => 'R',
            'ㄗ' => 'Z',
            'ㄘ' => 'A',
            'ㄙ' => 'S',
            'ㄧ' => 'E',
            'ㄨ' => 'U',
            'ㄩ' => 'I',
            'ㄚ' => 'A',
            'ㄛ' => 'O',
            'ㄜ' => 'R',
            'ㄝ' => 'W',
            'ㄞ' => 'I',
            'ㄟ' => 'S',
            'ㄠ' => 'Z',
            'ㄡ' => 'X',
            'ㄢ' => 'M',
            'ㄣ' => 'N',
            'ㄤ' => 'T',
            'ㄥ' => 'L',
            'ㄦ' => 'H',
            _ => continue,
        };
        keys.push(k);
    }
    format!("{}{}", keys, tone_key)
}

pub struct HsuBopomofoCoachProvider;

impl ReverseLookupProvider for HsuBopomofoCoachProvider {
    fn method_id(&self) -> &'static str {
        "hsu"
    }

    fn display_name(&self) -> &'static str {
        "許氏雙拼注音"
    }

    fn short_coach_title(&self) -> &'static str {
        "許氏教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Qwerty26
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let bpm = lookup_char_primary_bopomofo(ch, engine)?;
        let hsu_keys = bopomofo_to_hsu_keys(&bpm);
        let summary_badge = format!("{} [{}]", hsu_keys, bpm);
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: hsu_keys.clone(),
            alt_codes: vec![bpm.clone()],
            steps: vec![KeyStepBreakdown {
                key: hsu_keys,
                label: bpm,
                role: "許氏雙拼".to_string(),
            }],
            summary_badge,
        })
    }
}

pub struct StandardBopomofoCoachProvider;

impl ReverseLookupProvider for StandardBopomofoCoachProvider {
    fn method_id(&self) -> &'static str {
        "standard"
    }

    fn display_name(&self) -> &'static str {
        "標準大千注音"
    }

    fn short_coach_title(&self) -> &'static str {
        "大千教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Bopomofo41
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let bpm = lookup_char_primary_bopomofo(ch, engine)?;
        let std_keys = bopomofo_to_standard_keys(&bpm);
        let summary_badge = format!("{} [{}]", std_keys, bpm);
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: std_keys.clone(),
            alt_codes: vec![bpm.clone()],
            steps: vec![KeyStepBreakdown {
                key: std_keys,
                label: bpm,
                role: "標準大千".to_string(),
            }],
            summary_badge,
        })
    }
}

pub struct Et26BopomofoCoachProvider;

impl ReverseLookupProvider for Et26BopomofoCoachProvider {
    fn method_id(&self) -> &'static str {
        "et26"
    }

    fn display_name(&self) -> &'static str {
        "倚天 26 鍵注音"
    }

    fn short_coach_title(&self) -> &'static str {
        "倚天教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Qwerty26
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let bpm = lookup_char_primary_bopomofo(ch, engine)?;
        let et_keys = bopomofo_to_et26_keys(&bpm);
        let summary_badge = format!("{} [{}]", et_keys, bpm);
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: et_keys.clone(),
            alt_codes: vec![bpm.clone()],
            steps: vec![KeyStepBreakdown {
                key: et_keys,
                label: bpm,
                role: "倚天26鍵".to_string(),
            }],
            summary_badge,
        })
    }
}

pub struct LongkanCoachProvider;

impl ReverseLookupProvider for LongkanCoachProvider {
    fn method_id(&self) -> &'static str {
        "longkan"
    }

    fn display_name(&self) -> &'static str {
        "龍龕形音 2.0"
    }

    fn short_coach_title(&self) -> &'static str {
        "龍龕教練"
    }

    fn geometry(&self) -> KeyboardGeometry {
        KeyboardGeometry::Qwerty26
    }

    fn inspect_char(&self, ch: char, engine: &RuhiEngine) -> Option<CharBreakdown> {
        let rev = engine.longkan.table.reverse_lookup(ch)?;
        let code_upper = rev.primary_code.to_ascii_uppercase();
        let comp_labels: Vec<&str> = if !rev.breakdown.is_empty() {
            rev.breakdown.split('+').collect()
        } else {
            Vec::new()
        };
        let steps: Vec<KeyStepBreakdown> = code_upper
            .chars()
            .enumerate()
            .map(|(i, k)| KeyStepBreakdown {
                key: k.to_string(),
                label: comp_labels
                    .get(i)
                    .copied()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        if i == 0 && !rev.breakdown.is_empty() {
                            rev.breakdown.clone()
                        } else {
                            k.to_string()
                        }
                    }),
                role: format!("第{}碼", i + 1),
            })
            .collect();
        let summary_badge = engine
            .longkan
            .table
            .format_char_coach(ch)
            .unwrap_or_else(|| {
                if rev.breakdown.is_empty() {
                    code_upper.clone()
                } else {
                    format!("{} [{}]", code_upper, rev.breakdown)
                }
            });
        Some(CharBreakdown {
            ch,
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            primary_code: code_upper,
            alt_codes: rev
                .alt_breakdowns
                .iter()
                .map(|(c, bdesc)| {
                    if bdesc.is_empty() {
                        c.to_ascii_uppercase()
                    } else {
                        format!("{}={}", c.to_ascii_uppercase(), bdesc)
                    }
                })
                .collect(),
            steps,
            summary_badge,
        })
    }

    fn inspect_text(&self, text: &str, engine: &RuhiEngine) -> Option<PhraseBreakdown> {
        let chars_vec: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        if chars_vec.is_empty() {
            return None;
        }
        let mut char_breakdowns = Vec::new();
        let mut summary_parts = Vec::new();
        for &ch in &chars_vec {
            if let Some(cb) = self.inspect_char(ch, engine) {
                if chars_vec.len() == 1 {
                    summary_parts.push(format!("{} ➔ {}", ch, cb.summary_badge));
                } else {
                    summary_parts.push(format!("{}: {}", ch, cb.summary_badge));
                }
                char_breakdowns.push(cb);
            }
        }
        if char_breakdowns.is_empty() {
            return None;
        }
        let joined = summary_parts.join(" ｜ ");
        let (phrase_code, rule_explanation, coach_line) = if char_breakdowns.len() >= 2 {
            let first2: String = char_breakdowns[0]
                .primary_code
                .chars()
                .take(2)
                .collect();
            let mids1: String = char_breakdowns[1..char_breakdowns.len() - 1]
                .iter()
                .filter_map(|c| c.primary_code.chars().next())
                .collect();
            let last_code = &char_breakdowns.last().unwrap().primary_code;
            let last_chars: Vec<char> = last_code.chars().collect();
            let tail2: String = if last_chars.len() >= 2 {
                last_chars[last_chars.len() - 2..].iter().collect()
            } else {
                last_code.clone()
            };
            let p_code = format!("-{}{}{}", first2, mids1, tail2);
            let rule = format!("詞簡碼(Tab/{}): {}", p_code, joined);
            let line = format!("🎯 {}：{} ➔ {} ｜ {}", self.short_coach_title(), text, p_code, joined);
            (Some(p_code), rule, line)
        } else {
            (
                char_breakdowns.first().map(|c| c.primary_code.clone()),
                String::new(),
                format!("🎯 {}：{}", self.short_coach_title(), joined),
            )
        };
        Some(PhraseBreakdown {
            text: text.to_string(),
            method_id: self.method_id().to_string(),
            method_name: self.display_name().to_string(),
            phrase_code,
            rule_explanation,
            chars: char_breakdowns,
            coach_line,
        })
    }
}

// ============================================================================
// 全域輸入法反查提供者註冊表 (Universal Provider Registry)
// 未來新增任何輸入法（如大易、五筆畫、台語羅馬字等）僅需在此陣列追加一項即可全域生效！
// ============================================================================
pub static LONGKAN_PROVIDER: LongkanCoachProvider = LongkanCoachProvider;
pub static FOUR_CORNER_PROVIDER: FourCornerCoachProvider = FourCornerCoachProvider;
pub static ARRAY30_PROVIDER: Array30CoachProvider = Array30CoachProvider;
pub static CANGJIE_PROVIDER: CangjieCoachProvider = CangjieCoachProvider;
pub static HSU_PROVIDER: HsuBopomofoCoachProvider = HsuBopomofoCoachProvider;
pub static STANDARD_PROVIDER: StandardBopomofoCoachProvider = StandardBopomofoCoachProvider;
pub static ET26_PROVIDER: Et26BopomofoCoachProvider = Et26BopomofoCoachProvider;

pub static UNIVERSAL_REVERSE_PROVIDERS: &[&dyn ReverseLookupProvider] = &[
    &LONGKAN_PROVIDER,
    &FOUR_CORNER_PROVIDER,
    &ARRAY30_PROVIDER,
    &CANGJIE_PROVIDER,
    &HSU_PROVIDER,
    &STANDARD_PROVIDER,
    &ET26_PROVIDER,
];

/// 依 `method_id` 取得對應之反查提供者
pub fn get_reverse_provider(method_id: &str) -> Option<&'static dyn ReverseLookupProvider> {
    let norm = RuhiEngine::normalize_keyboard_mode(method_id);
    UNIVERSAL_REVERSE_PROVIDERS
        .iter()
        .copied()
        .find(|p| p.method_id() == norm)
}

/// 針對給定文字，一次取得全體已註冊輸入法之結構化反查結果（供互動工作台與跨輸入法對照器使用）
pub fn inspect_text_across_all_methods(text: &str, engine: &RuhiEngine) -> Vec<PhraseBreakdown> {
    let mut out = Vec::new();
    for provider in UNIVERSAL_REVERSE_PROVIDERS {
        if let Some(pb) = provider.inspect_text(text, engine) {
            out.push(pb);
        }
    }
    out
}

/// 為指定目標輸入法 (`target_method`) 產生當前引擎狀態下之即時反查教練提示字串
pub fn format_universal_coach_for_engine(engine: &RuhiEngine, target_method: &str) -> String {
    let norm_target = RuhiEngine::normalize_keyboard_mode(target_method);
    if norm_target.is_empty() || norm_target == "none" || norm_target == "off" {
        return String::new();
    }
    let cur_mode = RuhiEngine::normalize_keyboard_mode(&engine.keyboard_mode);
    // 若當前輸入法即為反查目標輸入法，或處於英日韓模式，不重複顯示同方案反查
    if cur_mode == norm_target
        || matches!(cur_mode.as_str(), "japanese" | "korean" | "english")
    {
        return String::new();
    }
    let Some(provider) = get_reverse_provider(&norm_target) else {
        return String::new();
    };

    if engine.mode == EngineMode::CandidatePopup && !engine.current_candidates.is_empty() {
        let idx = engine
            .candidate_cycle_idx
            .min(engine.current_candidates.len().saturating_sub(1));
        if let Some(cand) = engine.current_candidates.get(idx) {
            if let Some(pb) = provider.inspect_text(cand, engine) {
                return pb.coach_line;
            }
        }
    }

    if (cur_mode == "longkan" || engine.inline_longkan_active) && engine.longkan.has_buffer() {
        let idx = engine
            .longkan
            .candidate_idx
            .min(engine.longkan.candidates.len().saturating_sub(1));
        if let Some(cand) = engine.longkan.candidates.get(idx) {
            if let Some(pb) = provider.inspect_text(cand, engine) {
                return pb.coach_line;
            }
        }
    }

    if cur_mode == "four_corner" && engine.four_corner.has_buffer() {
        let idx = engine
            .four_corner
            .candidate_idx
            .min(engine.four_corner.candidates.len().saturating_sub(1));
        if let Some(cand) = engine.four_corner.candidates.get(idx) {
            if let Some(pb) = provider.inspect_text(cand, engine) {
                return pb.coach_line;
            }
        }
    }

    if cur_mode == "array30" && engine.array30.has_buffer() {
        let idx = engine
            .array30
            .candidate_idx
            .min(engine.array30.candidates.len().saturating_sub(1));
        if let Some(cand) = engine.array30.candidates.get(idx) {
            if let Some(pb) = provider.inspect_text(cand, engine) {
                return pb.coach_line;
            }
        }
    }

    if cur_mode == "cangjie" && !engine.cangjie.buffer.is_empty() {
        let idx = engine
            .cangjie
            .candidate_idx
            .min(engine.cangjie.candidates.len().saturating_sub(1));
        if let Some(cand) = engine.cangjie.candidates.get(idx) {
            if let Some(pb) = provider.inspect_text(cand, engine) {
                return pb.coach_line;
            }
        }
    }

    if !engine.composed_sentence.is_empty() && !engine.is_latin_mode {
        if let Some(pb) = provider.inspect_text(&engine.composed_sentence, engine) {
            return pb.coach_line;
        }
    }

    if !engine.last_committed_word.is_empty() {
        if let Some(pb) = provider.inspect_text(&engine.last_committed_word, engine) {
            return pb.coach_line;
        }
    }

    String::new()
}
