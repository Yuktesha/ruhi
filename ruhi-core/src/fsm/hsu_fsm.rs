/// RuhI (如喜) Input Method Framework - Rust Core
/// src/fsm/hsu_fsm.rs - 許氏鍵盤雙拼精密有限狀態機

use super::hsu_table::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsuSyllable {
    pub initial: String,
    pub medial: String,
    pub final_part: String,
    pub tone: char,
}

impl HsuSyllable {
    pub fn new(initial: &str, medial: &str, final_part: &str, tone: char) -> Self {
        Self {
            initial: initial.to_string(),
            medial: medial.to_string(),
            final_part: final_part.to_string(),
            tone,
        }
    }

    pub fn bopomofo_display(&self) -> String {
        let tone_mark = get_tone_mark(self.tone);
        if self.tone == '5' {
            format!("˙{}{}{}", self.initial, self.medial, self.final_part)
        } else {
            format!("{}{}{}{}", self.initial, self.medial, self.final_part, tone_mark)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.initial.is_empty() && self.medial.is_empty() && self.final_part.is_empty()
    }

    /// 判斷是否為未帶介音、韻母且一聲之純聲母音節（供標準大千／頭碼輸入模式判定）
    pub fn is_pure_initial(&self) -> bool {
        !self.initial.is_empty() && self.medial.is_empty() && self.final_part.is_empty() && self.tone == '1'
    }

    /// 檢驗該音節是否為符合現代漢語規範之合法基礎音節（嚴格抵禦如「ㄊㄛ」、「ㄅㄡ」等非漢語音節劫掠）
    pub fn is_mandarin_valid(&self) -> bool {
        let base = format!("{}{}{}", self.initial, self.medial, self.final_part);
        if base.is_empty() {
            return false;
        }
        crate::fsm::mandarin_syllables::is_valid_mandarin_syllable(&base)
    }

    /// 從標準注音顯示字串（如「ㄕㄨ」、「ㄖㄨˋ」、「ㄌㄧㄝˋ」、「˙ㄉㄜ」）還原解析出 HsuSyllable
    pub fn from_bopomofo_display(bpm: &str) -> Option<Self> {
        let trimmed = bpm.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut tone = '1';
        let mut text = trimmed;

        if text.starts_with('˙') {
            tone = '5';
            text = &text['˙'.len_utf8()..];
        } else if text.ends_with('ˊ') {
            tone = '2';
            text = &text[..text.len() - 'ˊ'.len_utf8()];
        } else if text.ends_with('ˇ') {
            tone = '3';
            text = &text[..text.len() - 'ˇ'.len_utf8()];
        } else if text.ends_with('ˋ') {
            tone = '4';
            text = &text[..text.len() - 'ˋ'.len_utf8()];
        }

        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return None;
        }

        let is_initial = |c: char| matches!(c, 'ㄅ'..='ㄙ');
        let is_medial = |c: char| matches!(c, 'ㄧ' | 'ㄨ' | 'ㄩ');

        let mut init = String::new();
        let mut med = String::new();
        let mut fin = String::new();

        let mut idx = 0;
        if idx < chars.len() && is_initial(chars[idx]) {
            init.push(chars[idx]);
            idx += 1;
        }
        if idx < chars.len() && is_medial(chars[idx]) {
            if idx + 1 < chars.len() {
                med.push(chars[idx]);
                idx += 1;
            } else if init.is_empty() {
                med.push(chars[idx]);
                idx += 1;
            } else {
                // 具有聲母且後面無其他韻母，若為 ㄧ/ㄨ/ㄩ，許氏視為介母
                med.push(chars[idx]);
                idx += 1;
            }
        }
        while idx < chars.len() {
            fin.push(chars[idx]);
            idx += 1;
        }

        Some(Self {
            initial: init,
            medial: med,
            final_part: fin,
            tone,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct HsuFSM {
    pub raw_keys: Vec<char>,
    pub syllables: Vec<HsuSyllable>,
    pub cur_initial: String,
    pub cur_medial: String,
    pub cur_final: String,
    pub cur_tone: char,
    pub is_standard_layout: bool,
}

impl HsuFSM {
    pub fn new() -> Self {
        Self {
            cur_tone: '1',
            ..Default::default()
        }
    }

    pub fn reset(&mut self) {
        self.raw_keys.clear();
        self.syllables.clear();
        self.cur_initial.clear();
        self.cur_medial.clear();
        self.cur_final.clear();
        self.cur_tone = '1';
        self.is_standard_layout = false;
    }

    pub fn has_pending(&self) -> bool {
        !self.cur_initial.is_empty() || !self.cur_medial.is_empty() || !self.cur_final.is_empty() || self.cur_tone != '1'
    }

    pub fn is_empty(&self) -> bool {
        self.syllables.is_empty() && !self.has_pending()
    }

    /// 檢驗已提交音節序列中是否存在任何非漢語音節
    pub fn has_invalid_syllable(&self) -> bool {
        self.syllables.iter().any(|s| !s.is_mandarin_valid())
    }

    fn get_normalized_pending_base(&self) -> String {
        let mut init = self.cur_initial.clone();
        let med = &self.cur_medial;
        let fin = &self.cur_final;

        if med == "ㄧ" || med == "ㄩ" {
            if init == "ㄓ" || init == "ㄍ" {
                init = "ㄐ".to_string();
            } else if init == "ㄔ" {
                init = "ㄑ".to_string();
            } else if init == "ㄕ" {
                init = "ㄒ".to_string();
            }
        }
        format!("{}{}{}", init, med, fin)
    }

    /// 檢驗當前未決組字結構是否已構成非法漢語音位搭配（如 ㄊ+ㄛ、ㄅ+ㄡ、ㄡ+ㄡ 等）
    pub fn has_invalid_pending(&self) -> bool {
        if !self.has_pending() {
            return false;
        }
        if !self.cur_final.is_empty() {
            let base = self.get_normalized_pending_base();
            !crate::fsm::mandarin_syllables::is_valid_mandarin_syllable(&base)
        } else {
            false
        }
    }

    fn find_init(k: char) -> Option<&'static str> {
        INIT_KEYS.iter().find(|&&(c, _)| c == k).map(|&(_, v)| v)
    }

    fn find_med(k: char) -> Option<&'static str> {
        MED_KEYS.iter().find(|&&(c, _)| c == k).map(|&(_, v)| v)
    }

    fn find_final(k: char) -> Option<&'static str> {
        FINAL_KEYS.iter().find(|&&(c, _)| c == k).map(|&(_, v)| v)
    }

    fn find_tone(k: char) -> Option<char> {
        TONE_KEYS.iter().find(|&&(c, _)| c == k).map(|&(_, t)| t)
    }

    pub fn parse_key_with_layout(&mut self, key: char, layout: &str) -> bool {
        self.is_standard_layout = matches!(layout, "standard" | "daikien");
        match layout {
            "standard" | "daikien" => self.parse_standard_key(key),
            "et26" | "eten26" => self.parse_et26_key(key),
            _ => self.parse_hsu_key(key),
        }
    }

    pub fn parse_standard_key(&mut self, key: char) -> bool {
        self.is_standard_layout = true;
        let k = key.to_ascii_lowercase();
        let tone = match k {
            ' ' => Some('1'),
            '6' => Some('2'),
            '3' => Some('3'),
            '4' => Some('4'),
            '7' => Some('5'),
            _ => None,
        };
        if let Some(t) = tone {
            if self.has_pending() {
                self.commit_current(t);
                self.raw_keys.push(k);
                return true;
            }
            return false;
        }
        let init = match k {
            '1' => Some("ㄅ"), 'q' => Some("ㄆ"), 'a' => Some("ㄇ"), 'z' => Some("ㄈ"),
            '2' => Some("ㄉ"), 'w' => Some("ㄊ"), 's' => Some("ㄋ"), 'x' => Some("ㄌ"),
            'e' => Some("ㄍ"), 'd' => Some("ㄎ"), 'c' => Some("ㄏ"),
            'r' => Some("ㄐ"), 'f' => Some("ㄑ"), 'v' => Some("ㄒ"),
            '5' => Some("ㄓ"), 't' => Some("ㄔ"), 'g' => Some("ㄕ"), 'b' => Some("ㄖ"),
            'y' => Some("ㄗ"), 'h' => Some("ㄘ"), 'n' => Some("ㄙ"),
            _ => None,
        };
        let med = match k {
            'u' => Some("ㄧ"), 'j' => Some("ㄨ"), 'm' => Some("ㄩ"),
            _ => None,
        };
        let fin = match k {
            '8' => Some("ㄚ"), 'i' => Some("ㄛ"), 'k' => Some("ㄜ"), ',' => Some("ㄝ"),
            '9' => Some("ㄞ"), 'o' => Some("ㄟ"), 'l' => Some("ㄠ"), '.' => Some("ㄡ"),
            '0' => Some("ㄢ"), 'p' => Some("ㄣ"), ';' => Some("ㄤ"), '/' => Some("ㄥ"),
            '-' => Some("ㄦ"),
            _ => None,
        };
        if let Some(iv) = init {
            if !self.cur_initial.is_empty() || !self.cur_medial.is_empty() || !self.cur_final.is_empty() {
                self.commit_current('1');
            }
            self.cur_initial = iv.to_string();
            self.raw_keys.push(k);
            return true;
        }
        if let Some(mv) = med {
            if !self.cur_medial.is_empty() || !self.cur_final.is_empty() {
                self.commit_current('1');
            }
            self.cur_medial = mv.to_string();
            self.raw_keys.push(k);
            return true;
        }
        if let Some(fv) = fin {
            if !self.cur_final.is_empty() || (fv == "ㄦ" && (!self.cur_initial.is_empty() || !self.cur_medial.is_empty())) {
                self.commit_current('1');
            }
            self.cur_final = fv.to_string();
            self.raw_keys.push(k);
            return true;
        }
        false
    }

    pub fn parse_et26_key(&mut self, key: char) -> bool {
        let k = key.to_ascii_lowercase();
        if self.has_pending() {
            let tone = match k {
                ' ' => Some('1'),
                'f' if !self.cur_final.is_empty() || !self.cur_medial.is_empty() => Some('2'),
                'd' if !self.cur_final.is_empty() || !self.cur_medial.is_empty() => Some('3'),
                'j' if !self.cur_final.is_empty() || !self.cur_medial.is_empty() => Some('4'),
                'k' if !self.cur_final.is_empty() || !self.cur_medial.is_empty() => Some('5'),
                _ => None,
            };
            if let Some(t) = tone {
                self.commit_current(t);
                self.raw_keys.push(k);
                return true;
            }
        }
        if !self.has_pending() {
            let init = match k {
                'b' => Some("ㄅ"), 'p' => Some("ㄆ"), 'm' => Some("ㄇ"), 'f' => Some("ㄈ"),
                'd' => Some("ㄉ"), 't' => Some("ㄊ"), 'n' => Some("ㄋ"), 'l' => Some("ㄌ"),
                'v' => Some("ㄍ"), 'k' => Some("ㄎ"), 'h' => Some("ㄏ"),
                'g' => Some("ㄐ"), 'c' => Some("ㄑ"), 'x' => Some("ㄒ"),
                'j' => Some("ㄓ"), 'q' => Some("ㄔ"), 'w' => Some("ㄕ"), 'r' => Some("ㄖ"),
                'z' => Some("ㄗ"), 'a' => Some("ㄘ"), 's' => Some("ㄙ"),
                _ => None,
            };
            if let Some(iv) = init {
                self.cur_initial = iv.to_string();
                self.raw_keys.push(k);
                return true;
            }
        }
        let med = match k {
            'e' => Some("ㄧ"), 'u' => Some("ㄨ"), 'i' if self.cur_medial.is_empty() => Some("ㄩ"),
            _ => None,
        };
        if let Some(mv) = med {
            if self.cur_final.is_empty() {
                self.cur_medial = mv.to_string();
                self.raw_keys.push(k);
                return true;
            }
        }
        let fin = match k {
            'a' => Some("ㄚ"), 'o' => Some("ㄛ"), 'r' => Some("ㄜ"), 'w' => Some("ㄝ"),
            'i' => Some("ㄞ"), 'q' => Some("ㄟ"), 'z' => Some("ㄠ"), 'x' => Some("ㄡ"),
            'm' => Some("ㄢ"), 'n' => Some("ㄣ"), 't' => Some("ㄤ"), 'l' => Some("ㄥ"),
            'h' => Some("ㄦ"),
            _ => None,
        };
        if let Some(fv) = fin {
            self.cur_final = fv.to_string();
            self.raw_keys.push(k);
            return true;
        }
        false
    }

    pub fn parse_hsu_key(&mut self, key: char) -> bool {
        let k = key.to_ascii_lowercase();

        // 1. 聲調結算 (當已具備聲母/介音/韻母任一項時)
        if self.has_pending() {
            if k == ' ' {
                self.commit_current('1');
                self.raw_keys.push(k);
                return true;
            }

            if let Some(t) = Self::find_tone(k) {
                if !self.cur_final.is_empty()
                    || ((!self.cur_medial.is_empty() || !self.cur_initial.is_empty())
                        && matches!(k, 'd' | 'f' | 'a' | 's' | 'j')
                        && Self::find_final(k).is_none())
                {
                    self.commit_current(t);
                    self.raw_keys.push(k);
                    return true;
                }
            }
        }

        if k == ' ' {
            return false;
        }

        // 2. 狀態 A：空音節開始
        if !self.has_pending() {
            // 支援經典「前綴輕聲」：若以 '.' 或 '7' 開頭，預置當前音節聲調為輕聲 (tone 5)
            if k == '.' || k == '7' {
                self.cur_tone = '5';
                self.raw_keys.push(k);
                return true;
            }
            if let Some(init) = Self::find_init(k) {
                self.cur_initial = init.to_string();
                self.raw_keys.push(k);
                return true;
            }
            if let Some(med) = Self::find_med(k) {
                self.cur_medial = med.to_string();
                self.raw_keys.push(k);
                return true;
            }
            if let Some(fin) = Self::find_final(k) {
                self.cur_final = fin.to_string();
                self.raw_keys.push(k);
                return true;
            }
            return false;
        }

        // 3. 狀態 B：已有聲母，尚未有介音與韻母
        if !self.cur_initial.is_empty() && self.cur_medial.is_empty() && self.cur_final.is_empty() {
            if let Some(med) = Self::find_med(k) {
                self.cur_medial = med.to_string();
                self.raw_keys.push(k);
                return true;
            }
            if let Some(fin) = Self::find_final(k) {
                self.cur_final = fin.to_string();
                self.raw_keys.push(k);
                return true;
            }
            if let Some(t) = Self::find_tone(k) {
                self.commit_current(t);
                self.raw_keys.push(k);
                return true;
            }
            if let Some(init) = Self::find_init(k) {
                self.commit_current('1');
                self.cur_initial = init.to_string();
                self.raw_keys.push(k);
                return true;
            }
        }

        // 4. 狀態 C：已有介音，尚未有韻母
        if !self.cur_medial.is_empty() && self.cur_final.is_empty() {
            if let Some(fin) = Self::find_final(k) {
                self.cur_final = fin.to_string();
                self.raw_keys.push(k);
                return true;
            }
            if let Some(t) = Self::find_tone(k) {
                self.commit_current(t);
                self.raw_keys.push(k);
                return true;
            }
            if let Some(init) = Self::find_init(k) {
                self.commit_current('1');
                self.cur_initial = init.to_string();
                self.raw_keys.push(k);
                return true;
            }
            if let Some(med) = Self::find_med(k) {
                if self.cur_medial == med {
                    // 連按相同介音鍵（如 u+u 或退格後重按 u），冪等維持當前介音並吸收按鍵，嚴防 u/x 洩漏至目標視窗
                    return true;
                }
                self.commit_current('1');
                self.cur_medial = med.to_string();
                self.raw_keys.push(k);
                return true;
            }
        }

        // 5. 狀態 D：已有韻母
        if !self.cur_final.is_empty() {
            if let Some(t) = Self::find_tone(k) {
                self.commit_current(t);
                self.raw_keys.push(k);
                return true;
            }

            // 特殊雙義鍵位回退修復：
            // 當前韻母若是由兼任聲母「ㄖ」之 'r' 鍵填入的「ㄚ」，且介音為「ㄨ」
            // 接續鍵入的鍵位若為介音「ㄨ」(x) 或「ㄧ」(e)，則 'r' 必為後字聲母「ㄖ」（如 cx + rxj ➔ ㄕㄨ ㄖㄨˋ 輸入，嚴防誤成 刷物）
            if self.cur_final == "ㄚ" && self.raw_keys.last() == Some(&'r') && self.cur_medial == "ㄨ" && (k == 'x' || k == 'e') {
                self.cur_final.clear();
                self.commit_current('1'); // 結算前字「ㄕㄨ」
                self.cur_initial = "ㄖ".to_string(); // 'r' 歸位為後字聲母「ㄖ」
                if k == 'x' {
                    self.cur_medial = "ㄨ".to_string();
                } else {
                    self.cur_medial = "ㄧ".to_string();
                }
                self.raw_keys.push(k);
                return true;
            }

            self.commit_current('1');
            if let Some(init) = Self::find_init(k) {
                self.cur_initial = init.to_string();
            } else if let Some(med) = Self::find_med(k) {
                self.cur_medial = med.to_string();
            } else if let Some(fin) = Self::find_final(k) {
                self.cur_final = fin.to_string();
            }
            self.raw_keys.push(k);
            return true;
        }

        false
    }

    fn normalize_syllable(mut init: String, med: String, mut fin: String, tone: char) -> HsuSyllable {
        // 1. 輕聲特殊簡拼補全：ds -> ㄉㄜ˙, ms -> ㄇㄜ˙, ls -> ㄌㄜ˙, ns -> ㄋㄜ˙, js -> ㄓㄜ˙, bs -> ㄅㄚ˙
        if tone == '5' && med.is_empty() && fin.is_empty() {
            match init.as_str() {
                "ㄉ" => fin = "ㄜ".to_string(),
                "ㄇ" => fin = "ㄜ".to_string(),
                "ㄌ" => fin = "ㄜ".to_string(),
                "ㄋ" => fin = "ㄜ".to_string(),
                "ㄓ" => fin = "ㄜ".to_string(),
                "ㄅ" => fin = "ㄚ".to_string(),
                _ => {}
            }
        } else if med.is_empty() && fin.is_empty() {
            // 2. 非輕聲且無介音/韻母時，處理非獨立聲母的自轉韻母（許氏雙用鍵位）：
            // ㄌ -> ㄦ (ld->ㄦˊ, lf->ㄦˇ, lj->ㄦˋ, l ->ㄦ)
            // ㄍ -> ㄜ (gd->ㄜˊ, gf->ㄜˇ, gj->ㄜˋ, g ->ㄜ)
            // ㄏ -> ㄛ (hd->ㄛˊ, hf->ㄛˇ, hj->ㄛˋ, h ->ㄛ)
            // ㄇ -> ㄢ (md->ㄢˊ, mf->ㄢˇ, mj->ㄢˋ, m ->ㄢ)
            // ㄋ -> ㄣ (nd->ㄣˊ, nf->ㄣˇ, nj->ㄣˋ, n ->ㄣ)
            match init.as_str() {
                "ㄌ" => { init.clear(); fin = "ㄦ".to_string(); }
                "ㄍ" => { init.clear(); fin = "ㄜ".to_string(); }
                "ㄏ" => { init.clear(); fin = "ㄛ".to_string(); }
                "ㄇ" => { init.clear(); fin = "ㄢ".to_string(); }
                "ㄋ" => { init.clear(); fin = "ㄣ".to_string(); }
                _ => {}
            }
        }

        // 3. 許氏自動轉換：ㄓ/ㄔ/ㄕ/ㄍ 遇 ㄧ/ㄩ 轉 ㄐ/ㄑ/ㄒ/ㄐ
        if med == "ㄧ" || med == "ㄩ" {
            if init == "ㄓ" || init == "ㄍ" {
                init = "ㄐ".to_string();
            } else if init == "ㄔ" {
                init = "ㄑ".to_string();
            } else if init == "ㄕ" {
                init = "ㄒ".to_string();
            }
        }

        HsuSyllable::new(&init, &med, &fin, tone)
    }

    pub fn commit_current(&mut self, tone: char) {
        let final_tone = if self.cur_tone != '1' { self.cur_tone } else { tone };
        let syl = if self.is_standard_layout {
            HsuSyllable::new(
                &self.cur_initial,
                &self.cur_medial,
                &self.cur_final,
                final_tone,
            )
        } else {
            Self::normalize_syllable(
                self.cur_initial.clone(),
                self.cur_medial.clone(),
                self.cur_final.clone(),
                final_tone,
            )
        };
        self.syllables.push(syl);
        self.cur_initial.clear();
        self.cur_medial.clear();
        self.cur_final.clear();
        self.cur_tone = '1';
    }

    pub fn get_display_preedit(&self) -> String {
        let mut parts: Vec<String> = self.syllables.iter().map(|s| s.bopomofo_display()).collect();
        let pending = if self.cur_tone == '5' {
            format!("˙{}{}{}", self.cur_initial, self.cur_medial, self.cur_final)
        } else {
            format!("{}{}{}", self.cur_initial, self.cur_medial, self.cur_final)
        };
        if !pending.is_empty() {
            parts.push(pending);
        }
        parts.join(" ")
    }

    pub fn get_all_syllables(&self) -> Vec<HsuSyllable> {
        let mut res = self.syllables.clone();
        if self.has_pending() {
            let syl = if self.is_standard_layout {
                HsuSyllable::new(
                    &self.cur_initial,
                    &self.cur_medial,
                    &self.cur_final,
                    self.cur_tone,
                )
            } else {
                Self::normalize_syllable(
                    self.cur_initial.clone(),
                    self.cur_medial.clone(),
                    self.cur_final.clone(),
                    self.cur_tone,
                )
            };
            res.push(syl);
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(fsm: &mut HsuFSM, s: &str) {
        for ch in s.chars() {
            fsm.parse_hsu_key(ch);
        }
    }

    #[test]
    fn test_wo_lai_shi_shi_kan() {
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "xhflidcjcjkmj");
        let syls = fsm.get_all_syllables();
        let display: Vec<String> = syls.iter().map(|s| s.bopomofo_display()).collect();
        assert_eq!(display, vec!["ㄨㄛˇ", "ㄌㄞˊ", "ㄕˋ", "ㄕˋ", "ㄎㄢˋ"]);
    }

    #[test]
    fn test_standalone_initials_and_finals() {
        // c j -> ㄕˋ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "cj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄕˋ");

        // v j -> ㄔˋ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "vj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄔˋ");

        // j j -> ㄓˋ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "jj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄓˋ");

        // m j -> ㄢˋ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "mj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄢˋ");

        // n j -> ㄣˋ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "nj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄣˋ");

        // l d -> ㄦˊ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "ld");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄦˊ");

        // l s -> ㄌㄜ˙
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "ls");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "˙ㄌㄜ");

        // g j -> ㄜˋ
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "gj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄜˋ");

        // c e r j -> ㄒㄧㄚˋ (ㄕ meeting ㄧ becomes ㄒ)
        let mut fsm = HsuFSM::new();
        type_str(&mut fsm, "cerj");
        assert_eq!(fsm.get_all_syllables()[0].bopomofo_display(), "ㄒㄧㄚˋ");
    }
}

