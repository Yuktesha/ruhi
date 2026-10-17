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

    /// 歐文多語系（英／法／西／德）上下文語法同音詞與重音符消歧器
    /// (Contextual Homophone, Confusable & Diacritic Disambiguator)
    /// 地位等同於華語的「的／得／地」、「在／再」二階構式消歧：
    /// 依據前驅詞 (`prev_token`) 與後繼詞 (`next_token`) 推導最適合的語法詞形。
    pub fn rank_contextual_confusables(
        &self,
        prev_token: &str,
        token: &str,
        next_token: &str,
    ) -> Vec<(String, f64, &'static str)> {
        let p = prev_token.trim().to_ascii_lowercase();
        let t = token.trim().to_ascii_lowercase();
        let n = next_token.trim().to_ascii_lowercase();
        let mut results: Vec<(String, f64, &'static str)> = Vec::new();

        match t.as_str() {
            // 1. 英文三元同音構式：their / there / they're
            "their" | "there" | "they're" | "theyre" | "thier" => {
                let is_existential_or_locative = matches!(
                    n.as_str(),
                    "is" | "are" | "was" | "were" | "has" | "have" | "will" | "can" | "could"
                        | "should" | "would" | "may" | "might" | "seems" | "appears"
                ) || (n.is_empty() && matches!(
                    p.as_str(),
                    "in" | "over" | "up" | "down" | "out" | "go" | "went" | "stay" | "from" | "right"
                ));
                let is_contraction_they_are = (n.len() > 4 && n.ends_with("ing"))
                    || matches!(
                        n.as_str(),
                        "a" | "an" | "the" | "not" | "so" | "very" | "too" | "quite" | "still"
                            | "already" | "always" | "never" | "right" | "wrong" | "ready"
                            | "here" | "there" | "happy" | "sure" | "able" | "welcome"
                    );
                if is_existential_or_locative {
                    results.push(("there".to_string(), 96.0, "Existential/Locative Construction [there + be / prep + there]"));
                    results.push(("their".to_string(), 70.0, "Possessive Determiner"));
                    results.push(("they're".to_string(), 68.0, "Subject-Copula Contraction"));
                } else if is_contraction_they_are {
                    results.push(("they're".to_string(), 96.0, "Subject-Copula Contraction [they're + V-ing/Adj/Art]"));
                    results.push(("their".to_string(), 70.0, "Possessive Determiner"));
                    results.push(("there".to_string(), 68.0, "Existential/Locative Adverb"));
                } else {
                    results.push(("their".to_string(), 95.0, "Possessive Determiner Construction [their + Noun]"));
                    results.push(("there".to_string(), 72.0, "Existential/Locative Adverb"));
                    results.push(("they're".to_string(), 70.0, "Subject-Copula Contraction"));
                }
            }

            // 2. 英文雙元同音構式：your / you're
            "your" | "you're" | "youre" => {
                let is_contraction_you_are = (n.len() > 4 && n.ends_with("ing"))
                    || matches!(
                        n.as_str(),
                        "welcome" | "right" | "wrong" | "sure" | "ready" | "not" | "so" | "very"
                            | "too" | "quite" | "still" | "already" | "always" | "never" | "a"
                            | "an" | "the" | "here" | "there" | "invited" | "done" | "good" | "great"
                    );
                if is_contraction_you_are {
                    results.push(("you're".to_string(), 96.0, "Subject-Copula Contraction [you're + Adj/V-ing/Art]"));
                    results.push(("your".to_string(), 70.0, "Possessive Determiner"));
                } else {
                    results.push(("your".to_string(), 95.0, "Possessive Determiner Construction [your + Noun]"));
                    results.push(("you're".to_string(), 70.0, "Subject-Copula Contraction"));
                }
            }

            // 3. 英文雙元同音構式：its / it's
            "its" | "it's" | "its'" => {
                let is_contraction_it_is = (n.len() > 4 && n.ends_with("ing"))
                    || matches!(
                        n.as_str(),
                        "a" | "an" | "the" | "not" | "so" | "very" | "too" | "quite" | "still"
                            | "already" | "always" | "never" | "been" | "gone" | "done" | "time"
                            | "ok" | "okay" | "fine" | "great" | "good" | "bad" | "hard" | "easy"
                            | "important" | "necessary" | "clear" | "true" | "false" | "only" | "just"
                    );
                if is_contraction_it_is {
                    results.push(("it's".to_string(), 96.0, "Pronoun-Copula Contraction [it's + Art/Adj/Adv/V-ing]"));
                    results.push(("its".to_string(), 70.0, "Possessive Determiner"));
                } else {
                    results.push(("its".to_string(), 95.0, "Possessive Determiner Construction [its + Noun/own]"));
                    results.push(("it's".to_string(), 70.0, "Pronoun-Copula Contraction"));
                }
            }

            // 4. 英文比較級連詞 vs 時間副詞：than / then
            "than" | "then" => {
                let is_comparative_prev = matches!(
                    p.as_str(),
                    "more" | "less" | "better" | "worse" | "faster" | "slower" | "higher" | "lower"
                        | "larger" | "smaller" | "bigger" | "greater" | "older" | "younger"
                        | "easier" | "harder" | "sooner" | "later" | "rather" | "other" | "different"
                ) || (p.len() > 4
                    && p.ends_with("er")
                    && !matches!(p.as_str(), "under" | "over" | "after" | "never" | "ever" | "water" | "paper" | "order" | "power" | "number" | "member" | "server" | "folder" | "buffer" | "router"));
                if is_comparative_prev {
                    results.push(("than".to_string(), 96.0, "Comparative Subordinator Construction [Comparative + than]"));
                    results.push(("then".to_string(), 68.0, "Temporal Adverb"));
                } else {
                    results.push(("then".to_string(), 94.0, "Temporal Sequence Adverb Construction [then]"));
                    results.push(("than".to_string(), 72.0, "Comparative Subordinator"));
                }
            }

            // 5. 英文及物動詞 vs 名詞：affect / effect
            "affect" | "effect" => {
                let is_verb_ctx = matches!(
                    p.as_str(),
                    "will" | "would" | "can" | "could" | "may" | "might" | "should" | "must"
                        | "does" | "did" | "do" | "not" | "don't" | "doesn't" | "didn't" | "won't"
                        | "to" | "directly" | "adversely" | "negatively" | "positively" | "significantly"
                );
                let is_noun_ctx = matches!(
                    p.as_str(),
                    "the" | "an" | "this" | "that" | "no" | "any" | "some" | "side" | "positive"
                        | "negative" | "ripple" | "net" | "direct" | "in" | "into" | "take" | "takes" | "took"
                );
                if is_verb_ctx && !is_noun_ctx {
                    results.push(("affect".to_string(), 96.0, "Transitive Verb Construction [Modal/Aux/to + affect]"));
                    results.push(("effect".to_string(), 70.0, "Noun"));
                } else if is_noun_ctx {
                    results.push(("effect".to_string(), 96.0, "Nominal Head Construction [Det/Adj/Prep + effect]"));
                    results.push(("affect".to_string(), 70.0, "Verb"));
                } else {
                    results.push((t.clone(), 85.0, "Lexical Default"));
                }
            }

            // 6. 英文介詞/不定式標記 vs 程度副詞：to / too
            "to" | "too" => {
                let is_degree_too = matches!(
                    n.as_str(),
                    "much" | "many" | "late" | "early" | "long" | "short" | "far" | "fast" | "slow"
                        | "high" | "low" | "big" | "small" | "hard" | "easy" | "old" | "young"
                        | "bad" | "good" | "expensive" | "difficult" | "complex" | "tired" | "busy"
                ) || (p == "me" && n.is_empty());
                if is_degree_too {
                    results.push(("too".to_string(), 96.0, "Degree/Additive Adverb Construction [too + Adj/Quantifier]"));
                    results.push(("to".to_string(), 70.0, "Preposition / Infinitive Marker"));
                } else {
                    results.push(("to".to_string(), 94.0, "Preposition / Infinitive Marker [to + V/NP]"));
                    results.push(("too".to_string(), 70.0, "Degree Adverb"));
                }
            }

            // 7. 法文語法重音消歧：a (助動詞 avoir) vs à (介詞)
            "a" | "à" => {
                let is_french_prep_a = matches!(
                    n.as_str(),
                    "la" | "le" | "les" | "un" | "une" | "paris" | "temps" | "partir" | "côté"
                        | "cote" | "cause" | "travers" | "nouveau" | "bientôt" | "bientot" | "demain"
                        | "tout" | "tous" | "toutes"
                ) || matches!(p.as_str(), "jusqu'" | "grâce" | "grace" | "quant");
                let is_french_aux_a = matches!(p.as_str(), "il" | "elle" | "on" | "qui");
                if is_french_prep_a {
                    results.push(("à".to_string(), 96.0, "French Preposition Construction [à + la/le/NP]"));
                    results.push(("a".to_string(), 70.0, "Auxiliary / Article"));
                } else if is_french_aux_a {
                    results.push(("a".to_string(), 95.0, "French Auxiliary Verb Construction [il/elle/on + a]"));
                    results.push(("à".to_string(), 70.0, "French Preposition"));
                }
            }

            // 8. 法文語法重音消歧：ou (或) vs où (哪裡/何時)
            "ou" | "où" => {
                let is_french_where = matches!(
                    p.as_str(),
                    "là" | "la" | "par" | "dans" | "moment" | "époque" | "epoque" | "cas" | "pays"
                        | "ville" | "jour" | "endroit" | "monde"
                ) || matches!(n.as_str(), "es" | "est" | "sont" | "vas" | "allez" | "il" | "elle" | "tu" | "vous");
                if is_french_where {
                    results.push(("où".to_string(), 96.0, "French Relative/Interrogative Locative Pronoun [où]"));
                    results.push(("ou".to_string(), 72.0, "French Disjunctive Conjunction"));
                } else {
                    results.push(("ou".to_string(), 92.0, "French Disjunctive Conjunction [A ou B]"));
                    results.push(("où".to_string(), 75.0, "French Locative Pronoun"));
                }
            }

            // 9. 法文語法重音消歧：sur (在...上) vs sûr (確定的，如 bien sûr)
            "sur" | "sûr" => {
                if matches!(p.as_str(), "bien" | "tout" | "très" | "tres" | "plus" | "moins" | "être" | "etre" | "est" | "suis") {
                    results.push(("sûr".to_string(), 96.0, "French Predicative Adjective Construction [bien/très + sûr]"));
                    results.push(("sur".to_string(), 70.0, "French Preposition"));
                } else {
                    results.push(("sur".to_string(), 94.0, "French Preposition [sur + Det/NP]"));
                    results.push(("sûr".to_string(), 70.0, "French Adjective"));
                }
            }

            // 10. 西班牙文語法重音消歧：tu (你的) vs tú (你)、el (定冠詞) vs él (他)、mas (但) vs más (更)
            "tu" | "tú" => {
                if matches!(
                    n.as_str(),
                    "eres" | "estás" | "estas" | "tienes" | "puedes" | "quieres" | "sabes" | "vas" | "haces" | "dices"
                ) {
                    results.push(("tú".to_string(), 96.0, "Spanish Subject Pronoun Construction [tú + Verb]"));
                    results.push(("tu".to_string(), 70.0, "Spanish Possessive Determiner"));
                } else {
                    results.push(("tu".to_string(), 94.0, "Spanish Possessive Determiner [tu + Noun]"));
                    results.push(("tú".to_string(), 72.0, "Spanish Subject Pronoun"));
                }
            }
            "el" | "él" => {
                if matches!(
                    p.as_str(),
                    "para" | "por" | "con" | "sin" | "contra" | "sobre" | "entre" | "hacia" | "según" | "segun"
                ) {
                    results.push(("él".to_string(), 96.0, "Spanish Prepositional Pronoun Construction [Prep + él]"));
                    results.push(("el".to_string(), 70.0, "Spanish Definite Article"));
                } else {
                    results.push(("el".to_string(), 94.0, "Spanish Definite Article [el + Noun]"));
                    results.push(("él".to_string(), 70.0, "Spanish Personal Pronoun"));
                }
            }
            "mas" | "más" => {
                if matches!(
                    n.as_str(),
                    "tarde" | "bien" | "grande" | "importante" | "rápido" | "rapido" | "fácil"
                        | "facil" | "allá" | "alla" | "menos" | "tiempo" | "gente" | "dinero"
                        | "veces" | "que" | "de"
                ) || matches!(p.as_str(), "mucho" | "nada" | "algo" | "poco" | "uno" | "vez")
                {
                    results.push(("más".to_string(), 96.0, "Spanish Comparative/Quantitative Adverb [más + Adj/Adv/que]"));
                    results.push(("mas".to_string(), 68.0, "Spanish Adversative Conjunction"));
                } else {
                    results.push(("más".to_string(), 90.0, "Spanish Quantitative Adverb"));
                    results.push(("mas".to_string(), 75.0, "Spanish Conjunction"));
                }
            }
            _ => {}
        }

        results
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

    #[test]
    fn test_western_contextual_confusables_disambiguation() {
        let scorer = WesternConfidenceScorer::new();

        // 1. 英文 their / there / they're
        let r_there = scorer.rank_contextual_confusables("", "their", "are");
        assert_eq!(r_there[0].0, "there", "後接 are 必消歧為存現構式 there");

        let r_theyre = scorer.rank_contextual_confusables("", "there", "working");
        assert_eq!(r_theyre[0].0, "they're", "後接進行分詞 working 必消歧為縮寫構式 they're");

        let r_their = scorer.rank_contextual_confusables("in", "they're", "house");
        assert_eq!(r_their[0].0, "their", "後接名詞 house 必消歧為所有格限定詞 their");

        // 2. 英文 your / you're 與 its / it's
        let r_youre = scorer.rank_contextual_confusables("", "your", "welcome");
        assert_eq!(r_youre[0].0, "you're");

        let r_its_contr = scorer.rank_contextual_confusables("", "its", "important");
        assert_eq!(r_its_contr[0].0, "it's");

        let r_its_poss = scorer.rank_contextual_confusables("in", "it's", "own");
        assert_eq!(r_its_poss[0].0, "its");

        // 3. 英文 than / then 與 affect / effect 與 to / too
        let r_than = scorer.rank_contextual_confusables("faster", "then", "before");
        assert_eq!(r_than[0].0, "than");

        let r_affect = scorer.rank_contextual_confusables("will", "effect", "the");
        assert_eq!(r_affect[0].0, "affect");

        let r_effect = scorer.rank_contextual_confusables("positive", "affect", "on");
        assert_eq!(r_effect[0].0, "effect");

        let r_too = scorer.rank_contextual_confusables("is", "to", "late");
        assert_eq!(r_too[0].0, "too");

        // 4. 法文 a / à, ou / où, sur / sûr 與西班牙文 tu / tú, el / él, mas / más
        let r_fr_a = scorer.rank_contextual_confusables("aller", "a", "paris");
        assert_eq!(r_fr_a[0].0, "à");

        let r_fr_ou = scorer.rank_contextual_confusables("moment", "ou", "il");
        assert_eq!(r_fr_ou[0].0, "où");

        let r_fr_sur = scorer.rank_contextual_confusables("bien", "sur", "");
        assert_eq!(r_fr_sur[0].0, "sûr");

        let r_es_tu = scorer.rank_contextual_confusables("", "tu", "eres");
        assert_eq!(r_es_tu[0].0, "tú");

        let r_es_el = scorer.rank_contextual_confusables("para", "el", "");
        assert_eq!(r_es_el[0].0, "él");

        let r_es_mas = scorer.rank_contextual_confusables("mucho", "mas", "importante");
        assert_eq!(r_es_mas[0].0, "más");
    }
}
