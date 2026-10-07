/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/scoring.rs - 歐文正向信心評分器 (Western Confidence Scorer)

use super::fsm::{TokenCasing, WesternToken, WesternTokenKind};
use super::lexicon::WesternLexicon;
use super::morphology::{MorphologicalVariant, MorphologyEngine};

#[derive(Debug, Clone, Default)]
pub struct WesternConfidenceScorer {
    morphology: MorphologyEngine,
}

impl WesternConfidenceScorer {
    pub fn new() -> Self {
        Self {
            morphology: MorphologyEngine::new(),
        }
    }

    /// 已知無母音常見縮寫 (Acronyms without regular vowels or short abbreviations)
    pub const COMMON_ABBREVS: &[&str] = &[
        "html", "css", "sql", "xml", "json", "yaml", "cpu", "gpu", "ssh", "ssl", "tls",
        "tcp", "udp", "dns", "http", "https", "ftp", "sdk", "api", "cli", "gui", "id",
        "db", "ui", "ux", "ip", "url", "uri", "os", "io", "npm", "pip", "git", "tsv",
        "csv", "pdf", "png", "jpg", "svg", "exe", "dll", "app", "cmd", "ps1", "sh",
        "src", "pkg", "lib", "fn", "mut", "pub", "str", "vec", "i32", "u32", "f32",
    ];

    /// 計算單一詞元或輸入緩衝區的「歐文正向信心度」
    /// 回傳值範圍為 [0.0, 1.0]：
    ///   - 0.90 ~ 1.00: 確定性合法歐文（高頻單字、合法屈折變化、標準駝峰代碼標識符）
    ///   - 0.60 ~ 0.89: 合法前綴或中等頻率詞（正在輸入中）
    ///   - 0.15 ~ 0.59: 拼寫邊緣或外來借詞
    ///   - 0.00 ~ 0.14: 漢語注音字母拼弦、子音堆疊噪音、非歐文音位搭配
    pub fn score_token(&self, token: &str, lexicon: &WesternLexicon) -> f64 {
        if token.is_empty() {
            return 0.0;
        }

        let t_clean = token.trim();
        let t_lower = t_clean.to_ascii_lowercase();

        // 0. 單一英文字母除 'a' 與 'i' (大小寫) 之外，英文中絕不存在其他獨立單字！
        // 許氏鍵盤之 e, b, d, f, j 等單鍵在漢字中皆為高頻首字或聲調，絕不可被評為獨立歐文！
        if t_clean.len() == 1 && t_clean.chars().all(|c| c.is_ascii_alphabetic()) {
            if t_lower != "a" && t_lower != "i" {
                return 0.02;
            }
        }

        // 1. 純數字與小數點
        if t_clean.chars().all(|c| c.is_ascii_digit() || c == '.') && t_clean.chars().any(|c| c.is_ascii_digit()) {
            return 0.95;
        }

        // 2. 縮寫詞精確命中
        for &abbr in Self::COMMON_ABBREVS {
            if t_lower == abbr {
                return 0.96;
            }
        }

        // 3. 形態學與字典完全/屈折匹配
        let (variant, weight_opt) = self.morphology.analyze(t_clean, lexicon);
        match variant {
            MorphologicalVariant::ExactMatch => {
                let weight = weight_opt.unwrap_or(50.0);
                // 依權重映射至 0.92 ~ 1.00
                return 0.92 + (weight as f64 / 100.0) * 0.08;
            }
            MorphologicalVariant::Plural(_)
            | MorphologicalVariant::PastTense(_)
            | MorphologicalVariant::Gerund(_)
            | MorphologicalVariant::Contraction(_) => {
                let weight = weight_opt.unwrap_or(50.0);
                return 0.88 + (weight as f64 / 100.0) * 0.08;
            }
            MorphologicalVariant::Adverb(_)
            | MorphologicalVariant::Possessive(_)
            | MorphologicalVariant::Comparative(_)
            | MorphologicalVariant::Superlative(_) => {
                let weight = weight_opt.unwrap_or(50.0);
                return 0.85 + (weight as f64 / 100.0) * 0.08;
            }
            MorphologicalVariant::Unknown => {}
        }

        // 4. 代碼標識符深層拆解判定 (CamelCase, snake_case, kebab-case)
        let token_obj = WesternToken::new(t_clean);
        if token_obj.is_code_identifier() {
            let subwords = self.split_identifier(t_clean, &token_obj.kind);
            if !subwords.is_empty() {
                let mut valid_count = 0;
                for sw in &subwords {
                    let (sw_var, _) = self.morphology.analyze(sw, lexicon);
                    if sw_var != MorphologicalVariant::Unknown || Self::COMMON_ABBREVS.contains(&sw.to_ascii_lowercase().as_str()) {
                        valid_count += 1;
                    }
                }
                let ratio = valid_count as f64 / subwords.len() as f64;
                if ratio >= 0.5 {
                    return 0.90 + ratio * 0.08; // 0.94 ~ 0.98 高信心代碼
                }
                if matches!(token_obj.kind, WesternTokenKind::Identifier(TokenCasing::ScreamingSnakeCase)) {
                    return 0.92;
                }
                return 0.88; // 結構清晰之代碼標識符
            }
        }

        // 5. 前綴匹配判定（使用者正在鍵入單字中）
        if lexicon.has_prefix(&t_lower) {
            let pfx_cnt = lexicon.prefix_count(&t_lower);
            let len = t_lower.len();
            // 越長的合法前綴且分支越多，信心度越高
            let base = 0.50 + (len as f64 * 0.06).min(0.24);
            let pfx_bonus = if pfx_cnt >= 10 { 0.10 } else if pfx_cnt >= 2 { 0.05 } else { 0.0 };
            return base + pfx_bonus; // 0.55 ~ 0.84
        }

        // 6. 英文音位學音律檢驗 (Phonotactics & Vowel ratio)
        // 許氏注音鍵位字串（如 xdf, celd, vld, uf, ej）高度缺乏自然英文母音結構或非合法字根
        let vowels_count = t_lower.chars().filter(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')).count();
        let chars_count = t_lower.chars().filter(|c| c.is_ascii_alphabetic()).count();

        // 異常連續相同子音堆疊 (如 jjj, vvv, qqq, xxx)
        if t_lower.contains("jj") || t_lower.contains("vv") || t_lower.contains("qq") || t_lower.contains("xx") || t_lower.contains("ww") {
            return 0.01;
        }

        // 超過 2 個字元且完全無母音（又不是縮寫）-> 判定為非歐文或注音碎片
        if chars_count >= 3 && vowels_count == 0 {
            return 0.02;
        }

        // 若連在字典中做為任何單字之前綴都無可能，判定為非歐文雜訊 / 漢字拼弦
        0.02
    }

    /// 依命名風格拆解標識符
    fn split_identifier(&self, text: &str, kind: &WesternTokenKind) -> Vec<String> {
        match kind {
            WesternTokenKind::Identifier(TokenCasing::SnakeCase)
            | WesternTokenKind::Identifier(TokenCasing::ScreamingSnakeCase) => {
                text.split('_').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
            }
            WesternTokenKind::Identifier(TokenCasing::KebabCase) => {
                text.split('-').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
            }
            WesternTokenKind::Identifier(TokenCasing::CamelCase)
            | WesternTokenKind::Identifier(TokenCasing::PascalCase) => {
                // 依大小寫邊界拆解 (e.g. getElementById -> ["get", "Element", "By", "Id"])
                let mut parts = Vec::new();
                let mut current = String::new();
                for c in text.chars() {
                    if c.is_ascii_uppercase() && !current.is_empty() {
                        parts.push(current.clone());
                        current.clear();
                    }
                    current.push(c);
                }
                if !current.is_empty() {
                    parts.push(current);
                }
                parts
            }
            _ => vec![text.to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_scoring() {
        let lex = WesternLexicon::new();
        let scorer = WesternConfidenceScorer::new();

        // 1. 高頻單字 -> 信心度 >= 0.95
        assert!(scorer.score_token("the", &lex) >= 0.95);
        assert!(scorer.score_token("book", &lex) >= 0.95);
        assert!(scorer.score_token("sample", &lex) >= 0.95);
        assert!(scorer.score_token("Google", &lex) >= 0.95);

        // 2. 形態學變化 -> 信心度 >= 0.90
        assert!(scorer.score_token("books", &lex) >= 0.90);
        assert!(scorer.score_token("samples", &lex) >= 0.90);
        assert!(scorer.score_token("working", &lex) >= 0.90);
        assert!(scorer.score_token("don't", &lex) >= 0.90);

        // 3. 代碼標識符 -> 信心度 >= 0.90
        assert!(scorer.score_token("getElementById", &lex) >= 0.90);
        assert!(scorer.score_token("max_buffer_size", &lex) >= 0.90);
        assert!(scorer.score_token("VK_RETURN", &lex) >= 0.90);

        // 4. 輸入中合法前綴 -> 0.50 ~ 0.85
        let sam_score = scorer.score_token("sam", &lex);
        assert!(sam_score >= 0.50 && sam_score < 0.90);

        // 5. 許氏注音鍵位字串（純漢字拼弦）-> 信心度 <= 0.05
        assert!(scorer.score_token("xdf", &lex) <= 0.05, "xdf (我) 絕不可被評為英文");
        assert!(scorer.score_token("celd", &lex) <= 0.05, "celd (形) 絕不可被評為英文");
        assert!(scorer.score_token("vld", &lex) <= 0.05, "vld (成) 絕不可被評為英文");
        assert!(scorer.score_token("jjj", &lex) <= 0.02, "連續重複子音絕不可被評為英文");
    }
}
