/// RuhI (如喜) Input Method Framework - Rust Core
/// src/western/morphology.rs - 歐文形態學與詞形變化推導規則 (Morphology Engine)

use super::lexicon::WesternLexicon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologicalVariant {
    ExactMatch,
    Plural(String),         // 複數形 (books -> book, cities -> city)
    PastTense(String),      // 過去式 (worked -> work, loved -> love, studied -> study)
    Gerund(String),         // 進行式 (working -> work, writing -> write, running -> run)
    Adverb(String),         // 副詞 (quickly -> quick, easily -> easy)
    Comparative(String),    // 比較級 (faster -> fast)
    Superlative(String),    // 最高級 (fastest -> fast)
    Contraction(String),    // 縮寫 (don't -> do, it's -> it, they're -> they)
    Possessive(String),     // 所有格 (John's -> John)
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct MorphologyEngine;

impl MorphologyEngine {
    pub fn new() -> Self {
        Self
    }

    /// 常見英文縮寫撇號規則表 (Contraction base lookup)
    const CONTRACTIONS: &[(&str, &str)] = &[
        ("don't", "do"),
        ("doesn't", "does"),
        ("didn't", "did"),
        ("can't", "can"),
        ("cannot", "can"),
        ("won't", "will"),
        ("wouldn't", "would"),
        ("shouldn't", "should"),
        ("couldn't", "could"),
        ("isn't", "is"),
        ("aren't", "are"),
        ("wasn't", "was"),
        ("weren't", "were"),
        ("haven't", "have"),
        ("hasn't", "has"),
        ("hadn't", "had"),
        ("it's", "it"),
        ("that's", "that"),
        ("what's", "what"),
        ("who's", "who"),
        ("there's", "there"),
        ("here's", "here"),
        ("let's", "let"),
        ("they're", "they"),
        ("we're", "we"),
        ("you're", "you"),
        ("i'm", "i"),
        ("they've", "they"),
        ("we've", "we"),
        ("you've", "you"),
        ("i've", "i"),
        ("they'll", "they"),
        ("we'll", "we"),
        ("you'll", "you"),
        ("i'll", "i"),
        ("he'll", "he"),
        ("she'll", "she"),
        ("it'll", "it"),
        ("they'd", "they"),
        ("we'd", "we"),
        ("you'd", "you"),
        ("i'd", "i"),
        ("he'd", "he"),
        ("she'd", "she"),
    ];

    /// 分析單字之形態學變化，若為字典內已知詞根的合法屈折形，回傳對應變體與詞根
    pub fn analyze(&self, word: &str, lexicon: &WesternLexicon) -> (MorphologicalVariant, Option<f32>) {
        let w_lower = word.to_ascii_lowercase();

        // 1. 完全命中字典
        if let Some(weight) = lexicon.get_weight(&w_lower) {
            return (MorphologicalVariant::ExactMatch, Some(weight));
        }

        // 2. 縮寫詞與否定詞判定
        for &(contraction, base) in Self::CONTRACTIONS {
            if w_lower == contraction {
                let base_weight = lexicon.get_weight(base).unwrap_or(90.0);
                return (MorphologicalVariant::Contraction(base.to_string()), Some(base_weight * 0.98));
            }
        }

        // 3. 所有格判定 (name's -> name)
        if w_lower.ends_with("'s") && w_lower.len() > 2 {
            let base = &w_lower[..w_lower.len() - 2];
            if let Some(bw) = lexicon.get_weight(base) {
                return (MorphologicalVariant::Possessive(base.to_string()), Some(bw * 0.96));
            }
        }

        // 4. 動名詞/進行式 -ing (working -> work, writing -> write, running -> run)
        if w_lower.ends_with("ing") && w_lower.len() > 4 {
            let stem = &w_lower[..w_lower.len() - 3];
            // 直接加 -ing (work -> working)
            if let Some(bw) = lexicon.get_weight(stem) {
                return (MorphologicalVariant::Gerund(stem.to_string()), Some(bw * 0.95));
            }
            // 去 e 加 -ing (write -> writing, make -> making)
            let with_e = format!("{}e", stem);
            if let Some(bw) = lexicon.get_weight(&with_e) {
                return (MorphologicalVariant::Gerund(with_e), Some(bw * 0.95));
            }
            // 雙寫去重 (running -> run, stopping -> stop)
            if stem.len() >= 3 {
                let chars: Vec<char> = stem.chars().collect();
                if chars[chars.len() - 1] == chars[chars.len() - 2] {
                    let dedupped: String = chars[..chars.len() - 1].iter().collect();
                    if let Some(bw) = lexicon.get_weight(&dedupped) {
                        return (MorphologicalVariant::Gerund(dedupped), Some(bw * 0.95));
                    }
                }
            }
        }

        // 5. 過去式 -ed / -d / -ied (worked -> work, loved -> love, studied -> study)
        if w_lower.ends_with("ed") && w_lower.len() > 3 {
            // -ied -> -y (studied -> study)
            if w_lower.ends_with("ied") && w_lower.len() > 4 {
                let with_y = format!("{}y", &w_lower[..w_lower.len() - 3]);
                if let Some(bw) = lexicon.get_weight(&with_y) {
                    return (MorphologicalVariant::PastTense(with_y), Some(bw * 0.95));
                }
            }
            // 直接加 -ed (work -> worked)
            let stem_ed = &w_lower[..w_lower.len() - 2];
            if let Some(bw) = lexicon.get_weight(stem_ed) {
                return (MorphologicalVariant::PastTense(stem_ed.to_string()), Some(bw * 0.95));
            }
            // 加 -d (love -> loved, move -> moved)
            let stem_d = &w_lower[..w_lower.len() - 1];
            if let Some(bw) = lexicon.get_weight(stem_d) {
                return (MorphologicalVariant::PastTense(stem_d.to_string()), Some(bw * 0.95));
            }
            // 雙寫去重 (stopped -> stop)
            if stem_ed.len() >= 3 {
                let chars: Vec<char> = stem_ed.chars().collect();
                if chars[chars.len() - 1] == chars[chars.len() - 2] {
                    let dedupped: String = chars[..chars.len() - 1].iter().collect();
                    if let Some(bw) = lexicon.get_weight(&dedupped) {
                        return (MorphologicalVariant::PastTense(dedupped), Some(bw * 0.95));
                    }
                }
            }
        }

        // 6. 複數形 -s / -es / -ies (books -> book, boxes -> box, cities -> city)
        if w_lower.ends_with('s') && w_lower.len() > 2 {
            // -ies -> -y (cities -> city)
            if w_lower.ends_with("ies") && w_lower.len() > 4 {
                let with_y = format!("{}y", &w_lower[..w_lower.len() - 3]);
                if let Some(bw) = lexicon.get_weight(&with_y) {
                    return (MorphologicalVariant::Plural(with_y), Some(bw * 0.95));
                }
            }
            // -es (boxes -> box, classes -> class, watches -> watch)
            if w_lower.ends_with("es") && w_lower.len() > 3 {
                let stem_es = &w_lower[..w_lower.len() - 2];
                if let Some(bw) = lexicon.get_weight(stem_es) {
                    return (MorphologicalVariant::Plural(stem_es.to_string()), Some(bw * 0.95));
                }
            }
            // -s (books -> book, samples -> sample)
            let stem_s = &w_lower[..w_lower.len() - 1];
            if let Some(bw) = lexicon.get_weight(stem_s) {
                return (MorphologicalVariant::Plural(stem_s.to_string()), Some(bw * 0.95));
            }
        }

        // 7. 副詞 -ly / -ily (quickly -> quick, happily -> happy)
        if w_lower.ends_with("ly") && w_lower.len() > 3 {
            if w_lower.ends_with("ily") && w_lower.len() > 4 {
                let with_y = format!("{}y", &w_lower[..w_lower.len() - 3]);
                if let Some(bw) = lexicon.get_weight(&with_y) {
                    return (MorphologicalVariant::Adverb(with_y), Some(bw * 0.92));
                }
            }
            let stem_ly = &w_lower[..w_lower.len() - 2];
            if let Some(bw) = lexicon.get_weight(stem_ly) {
                return (MorphologicalVariant::Adverb(stem_ly.to_string()), Some(bw * 0.92));
            }
        }

        // 8. 比較級 -er / 最高級 -est (faster -> fast, fastest -> fast)
        if w_lower.ends_with("er") && w_lower.len() > 3 {
            let stem_er = &w_lower[..w_lower.len() - 2];
            if let Some(bw) = lexicon.get_weight(stem_er) {
                return (MorphologicalVariant::Comparative(stem_er.to_string()), Some(bw * 0.90));
            }
        }
        if w_lower.ends_with("est") && w_lower.len() > 4 {
            let stem_est = &w_lower[..w_lower.len() - 3];
            if let Some(bw) = lexicon.get_weight(stem_est) {
                return (MorphologicalVariant::Superlative(stem_est.to_string()), Some(bw * 0.90));
            }
        }

        (MorphologicalVariant::Unknown, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morphology_variants() {
        let lex = WesternLexicon::new();
        let morph = MorphologyEngine::new();

        // Exact match
        let (v1, w1) = morph.analyze("book", &lex);
        assert_eq!(v1, MorphologicalVariant::ExactMatch);
        assert!(w1.is_some());

        // Plural
        let (v2, w2) = morph.analyze("books", &lex);
        assert_eq!(v2, MorphologicalVariant::Plural("book".to_string()));
        assert!(w2.is_some());

        let (v3, w3) = morph.analyze("samples", &lex);
        assert_eq!(v3, MorphologicalVariant::Plural("sample".to_string()));
        assert!(w3.is_some());

        // Past tense
        let (v4, w4) = morph.analyze("started", &lex);
        assert_eq!(v4, MorphologicalVariant::PastTense("start".to_string()));
        assert!(w4.is_some());

        // Gerund
        let (v5, w5) = morph.analyze("working", &lex);
        assert_eq!(v5, MorphologicalVariant::Gerund("work".to_string()));
        assert!(w5.is_some());

        // Contraction
        let (v6, w6) = morph.analyze("don't", &lex);
        assert_eq!(v6, MorphologicalVariant::Contraction("do".to_string()));
        assert!(w6.is_some());

        let (v7, w7) = morph.analyze("it's", &lex);
        assert_eq!(v7, MorphologicalVariant::Contraction("it".to_string()));
        assert!(w7.is_some());

        // Unknown
        let (v8, w8) = morph.analyze("fyen", &lex);
        assert_eq!(v8, MorphologicalVariant::Unknown);
        assert!(w8.is_none());
    }
}
