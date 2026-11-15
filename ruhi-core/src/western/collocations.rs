/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/western/collocations.rs - 泛歐文語詞搭配、因式分解句法槽與子句邊界後繼預測模組
/// (Pan-European Collocation, Factorized Syntactic Slot & Clause-Bounded Prediction)

use super::WesternEngine;

#[inline]
fn is_latin_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || (ch.is_alphabetic() && matches!(ch, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}'))
        || matches!(ch, '\u{0300}'..='\u{036F}')
}

#[inline]
fn is_horizontal_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\u{3000}')
}

#[inline]
fn is_internal_word_joiner(ch: char) -> bool {
    matches!(ch, '\'' | '\u{2019}' | '-')
}

/// 由游標前方文本擷取最近 1..=max_words 個歐文單字（小寫、由左至右順序）。
/// 嚴格遵守「零跨越（Zero-Crossing）」原則：一旦遇見逗號、句號、換行、中日韓漢字或標點即刻終止回溯。
pub fn extract_preceding_western_words(prefix_before_cursor: &str, max_words: usize) -> Vec<String> {
    if max_words == 0 || prefix_before_cursor.is_empty() {
        return Vec::new();
    }

    let chars: Vec<char> = prefix_before_cursor.chars().collect();
    let mut i = chars.len();
    let mut rev_words: Vec<String> = Vec::with_capacity(max_words);

    while rev_words.len() < max_words && i > 0 {
        while i > 0 && is_horizontal_space(chars[i - 1]) {
            i -= 1;
        }
        if i == 0 {
            break;
        }
        if !is_latin_word_char(chars[i - 1]) {
            break;
        }

        let end = i;
        while i > 0 {
            let ch = chars[i - 1];
            if is_latin_word_char(ch) {
                i -= 1;
            } else if is_internal_word_joiner(ch) && i >= 2 && is_latin_word_char(chars[i - 2]) {
                i -= 1;
            } else {
                break;
            }
        }

        let token: String = chars[i..end]
            .iter()
            .map(|&c| if c == '\u{2019}' { '\'' } else { c })
            .collect();
        rev_words.push(token.to_lowercase());
    }

    rev_words.reverse();
    rev_words
}

/// 由游標後方文本擷取緊鄰的第一個歐文單字（小寫）。
/// 嚴格遵守零跨越邊界：若跳過水平空白後首個字元非拉丁字母／數字（例如逗號、換行或句號），立即回傳空字串。
pub fn extract_leading_western_word(suffix_after_cursor: &str) -> String {
    if suffix_after_cursor.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = suffix_after_cursor.chars().collect();
    let mut i = 0;
    while i < chars.len() && is_horizontal_space(chars[i]) {
        i += 1;
    }
    if i >= chars.len() || !is_latin_word_char(chars[i]) {
        return String::new();
    }
    let start = i;
    while i < chars.len() {
        let ch = chars[i];
        if is_latin_word_char(ch) {
            i += 1;
        } else if is_internal_word_joiner(ch)
            && i + 1 < chars.len()
            && is_latin_word_char(chars[i + 1])
        {
            i += 1;
        } else {
            break;
        }
    }
    chars[start..i]
        .iter()
        .map(|&c| if c == '\u{2019}' { '\'' } else { c })
        .collect::<String>()
        .to_lowercase()
}

const CLOSED_CLASS_STOPWORDS: &[&str] = &[
    "where", "what", "when", "which", "there", "their", "these", "those", "would", "could",
    "should", "about", "after", "before", "under", "between", "through", "because", "today",
    "thank", "thanks", "please", "with", "from", "that", "this", "have", "been", "were", "will",
    "your", "they", "them", "some", "more", "most", "much", "many", "very", "also", "just",
    "still", "even", "here", "then", "than", "into", "over", "only", "such", "other",
];

/// 由整段前文組字緩衝區擷取實義內容詞（跨子句與逗號保留主題連貫性，過濾純功能詞與 length < 4 短詞）
pub fn extract_earlier_clause_content_words(prefix_before_word: &str) -> Vec<String> {
    let mut out = Vec::new();
    if prefix_before_word.is_empty() {
        return out;
    }
    let chars: Vec<char> = prefix_before_word.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !is_latin_word_char(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len() {
            let ch = chars[i];
            if is_latin_word_char(ch) {
                i += 1;
            } else if is_internal_word_joiner(ch)
                && i + 1 < chars.len()
                && is_latin_word_char(chars[i + 1])
            {
                i += 1;
            } else {
                break;
            }
        }
        let tok: String = chars[start..i]
            .iter()
            .map(|&c| if c == '\u{2019}' { '\'' } else { c })
            .collect::<String>()
            .to_lowercase();
        if tok.chars().count() >= 4
            && tok.chars().all(|c| c.is_alphabetic() || c == '-')
            && !CLOSED_CLASS_STOPWORDS.contains(&tok.as_str())
            && !out.contains(&tok)
        {
            out.push(tok);
        }
    }
    out
}

/// 三連詞語詞搭配表 (w_{t-2}, w_{t-1}, w_t) -> 高機率後繼詞清單
const TRIGRAM_COLLOCATIONS: &[(&str, &str, &str, &[&str])] = &[
    // English - Conversational / Wh-Questions / Progressive / Infinitive / Perfect
    ("where", "are", "you", &["going", "from", "going to", "staying", "now", "heading", "located", "guys"]),
    ("where", "is", "the", &["best", "nearest", "restroom", "station", "file", "problem"]),
    ("where", "do", "you", &["live", "work", "want", "go", "get", "think"]),
    ("where", "did", "you", &["go", "get", "find", "put", "park"]),
    ("where", "have", "you", &["been", "gone", "gotten"]),
    ("what", "are", "you", &["doing", "going to", "looking for", "talking about", "working on", "thinking", "up to"]),
    ("what", "do", "you", &["think", "mean", "want", "need", "recommend", "suggest"]),
    ("when", "are", "you", &["going", "coming", "leaving", "free", "available"]),
    ("why", "are", "you", &["doing", "asking", "saying", "going", "still", "so", "here"]),
    ("who", "are", "you", &["looking for", "talking to", "working with", "going with"]),
    ("how", "are", "you", &["doing", "today", "feeling", "guys"]),
    ("we", "are", "going", &["to", "to see", "to make", "to have", "to need"]),
    ("i", "am", "going", &["to", "to be", "to check", "to send"]),
    ("are", "you", "going", &["to", "to be", "there"]),
    ("have", "you", "ever", &["been", "seen", "tried", "heard", "thought"]),
    ("do", "you", "have", &["any", "a", "time", "the"]),
    ("can", "you", "please", &["help", "check", "send", "let", "confirm", "share"]),
    ("could", "you", "please", &["help", "check", "send", "let", "confirm", "share"]),
    ("i", "have", "already", &["finished", "sent", "checked", "updated", "done"]),
    ("look", "forward", "to", &["seeing", "hearing", "working", "meeting"]),
    ("looking", "forward", "to", &["seeing", "hearing", "working", "meeting", "your"]),
    ("thank", "you", "very", &["much"]),
    ("thank", "you", "so", &["much"]),
    ("thank", "you", "for", &["your", "the", "helping", "sharing", "reaching", "understanding"]),
    ("thanks", "so", "much", &["for"]),
    ("i", "would", "like", &["to"]),
    ("would", "you", "like", &["to", "some", "a"]),
    ("let", "me", "know", &["if", "when", "what", "how"]),
    ("please", "let", "me", &["know"]),
    ("in", "order", "to", &["make", "ensure", "get", "keep", "avoid", "improve"]),
    ("as", "soon", "as", &["possible"]),
    ("as", "well", "as", &["the", "your", "other"]),
    ("by", "the", "way", &["i", "do", "have", "what"]),
    ("on", "the", "other", &["hand"]),
    ("at", "the", "same", &["time"]),
    ("at", "the", "moment", &["i", "we", "there"]),
    ("nice", "to", "meet", &["you"]),
    ("happy", "to", "help", &["with", "you"]),
    ("please", "feel", "free", &["to"]),
    ("feel", "free", "to", &["reach", "ask", "let", "contact", "share"]),
    ("don't", "hesitate", "to", &["ask", "contact", "reach", "let"]),
    ("do", "not", "hesitate", &["to"]),
    ("not", "hesitate", "to", &["ask", "contact", "reach"]),
    ("is", "there", "anything", &["else", "i", "we"]),
    ("i", "look", "forward", &["to"]),
    ("i'm", "looking", "forward", &["to"]),
    ("have", "a", "great", &["day", "weekend", "time", "evening"]),
    ("have", "a", "good", &["day", "weekend", "time", "morning"]),
    ("have", "a", "nice", &["day", "weekend", "evening"]),
    ("according", "to", "the", &["documentation", "specification", "report", "plan"]),
    ("for", "the", "first", &["time"]),
    ("in", "terms", "of", &["performance", "quality", "accuracy", "cost"]),
    ("with", "respect", "to", &["the", "this", "your"]),
    // French
    ("qu'est", "ce", "que", &["c'est", "tu", "vous"]),
    ("où", "allez", "vous", &["aujourd'hui", "maintenant"]),
    ("je", "vous", "remercie", &["de", "pour", "bien"]),
    ("s'il", "vous", "plaît", &["veuillez", "merci", "pouvez-vous"]),
    ("s'il", "vous", "plait", &["veuillez", "merci"]),
    ("comment", "allez", "vous", &["aujourd'hui"]),
    ("j'ai", "besoin", "de", &["votre", "plus", "savoir", "faire"]),
    ("tout", "à", "fait", &["d'accord", "exact", "normal", "compréhensible"]),
    ("il", "y", "a", &["un", "une", "des", "beaucoup", "quelques"]),
    ("n'hésitez", "pas", "à", &["me", "nous", "contacter", "demander"]),
    ("je", "suis", "désolé", &["pour", "de", "mais"]),
    ("merci", "beaucoup", "pour", &["votre", "ce", "cette", "ton"]),
    ("c'est", "à", "dire", &["que", "qu'il"]),
    // German
    ("wie", "geht", "es", &["ihnen", "dir", "euch"]),
    ("vielen", "dank", "für", &["ihre", "deine", "die", "das", "alles"]),
    ("mit", "freundlichen", "grüßen", &["ihr", "ihre"]),
    ("ich", "freue", "mich", &["auf", "sehr", "über", "darauf"]),
    ("es", "tut", "mir", &["leid"]),
    ("ich", "möchte", "gerne", &["eine", "einen", "das", "mit"]),
    ("auf", "jeden", "fall", &["werden", "ist", "kann"]),
    // Spanish
    ("qué", "estás", "haciendo", &["ahora", "hoy"]),
    ("adónde", "vas", "a", &["ir", "comer", "estar"]),
    ("muchas", "gracias", "por", &["tu", "su", "la", "el", "todo", "ayudarme"]),
    ("por", "favor", "dime", &["si", "qué", "cuándo"]),
    ("lo", "siento", "mucho", &["por", "pero", "no"]),
    ("me", "gustaría", "saber", &["si", "cómo", "cuándo", "más"]),
    ("en", "este", "caso", &["podemos", "se", "es"]),
];

/// 二連詞語詞搭配表 (w_{t-1}, w_t) -> 高機率後繼詞清單
const BIGRAM_COLLOCATIONS: &[(&str, &str, &[&str])] = &[
    // English
    ("are", "you", &["going", "sure", "ready", "free", "available", "okay", "doing", "looking", "still", "coming"]),
    ("going", "to", &["see", "be", "make", "get", "do", "have", "take", "check", "use", "start", "work", "go", "show", "share"]),
    ("want", "to", &["see", "make", "get", "know", "go", "do", "be", "have", "check", "try", "share"]),
    ("need", "to", &["make", "get", "be", "do", "check", "know", "update", "fix", "find", "use", "go"]),
    ("have", "to", &["be", "go", "do", "make", "get", "say", "wait", "see", "check"]),
    ("able", "to", &["find", "see", "make", "get", "use", "attend", "access", "help", "resolve"]),
    ("trying", "to", &["find", "make", "get", "understand", "figure", "see", "fix", "use"]),
    ("used", "to", &["be", "have", "work", "live", "do", "go"]),
    ("working", "on", &["the", "a", "this", "it", "improving"]),
    ("looking", "for", &["a", "the", "your", "some", "help"]),
    ("waiting", "for", &["your", "the", "you", "approval", "confirmation"]),
    ("talking", "about", &["the", "this", "how", "what"]),
    ("thinking", "about", &["the", "going", "making", "how", "it"]),
    ("interested", "in", &["learning", "working", "joining", "the", "this"]),
    ("based", "on", &["the", "your", "our", "this"]),
    ("depends", "on", &["the", "how", "what", "whether"]),
    ("can", "you", &["help", "please", "check", "send", "let", "share", "tell", "give", "show"]),
    ("could", "you", &["please", "help", "check", "send", "let", "share", "tell"]),
    ("do", "you", &["have", "know", "want", "think", "need", "like", "see", "mean"]),
    ("did", "you", &["get", "see", "go", "find", "have", "know", "mean", "receive"]),
    ("have", "you", &["been", "seen", "got", "had", "heard", "tried", "finished", "ever"]),
    ("we", "have", &["been", "to", "a", "the", "already", "completed", "updated"]),
    ("they", "are", &["going", "working", "looking", "ready", "available", "not", "still"]),
    ("you", "are", &["welcome", "right", "going", "looking", "very", "ready", "invited"]),
    ("thank", "you", &["very much", "so much", "for", "again", "all"]),
    ("thanks", "for", &["your", "the", "sharing", "reaching", "helping", "waiting"]),
    ("how", "are", &["you", "things", "we", "they"]),
    ("looking", "forward", &["to"]),
    ("look", "forward", &["to"]),
    ("good", "morning", &["everyone", "team", "sir", "madam"]),
    ("good", "afternoon", &["everyone", "team"]),
    ("good", "evening", &["everyone"]),
    ("would", "like", &["to"]),
    ("feel", "free", &["to"]),
    ("let", "me", &["know", "check", "see", "help"]),
    ("according", "to", &["the", "this", "our", "your"]),
    ("due", "to", &["the", "a", "an", "this", "recent"]),
    ("instead", "of", &["the", "using", "having", "doing"]),
    ("because", "of", &["the", "this", "that", "your"]),
    ("in", "addition", &["to"]),
    ("on", "top", &["of"]),
    ("as", "well", &["as"]),
    ("such", "as", &["the", "this", "these", "a"]),
    ("for", "example", &["if", "when", "in", "you", "the"]),
    ("no", "problem", &["at all", "glad", "let"]),
    ("see", "you", &["soon", "tomorrow", "later", "there", "next week"]),
    ("talk", "to", &["you", "you soon", "you later"]),
    ("i", "am", &["writing", "glad", "happy", "sorry", "working", "sure", "looking", "going"]),
    ("i'm", "not", &["sure", "able", "ready", "going"]),
    ("i", "have", &["attached", "updated", "checked", "been", "to", "a"]),
    ("we", "are", &["going", "working", "planning", "happy", "ready", "looking"]),
    ("you", "can", &["use", "find", "see", "check", "try", "also"]),
    ("please", "note", &["that"]),
    ("please", "check", &["the", "if", "your", "this"]),
    ("make", "sure", &["that", "the", "you", "to", "it"]),
    ("kind", "regards", &["thanks"]),
    ("best", "regards", &["thanks"]),
    ("at", "least", &["one", "two", "some", "for"]),
    ("right", "now", &["i", "we", "the", "it"]),
    // French
    ("nous", "allons", &["voir", "faire", "prendre", "commencer", "partir"]),
    ("je", "vais", &["faire", "voir", "essayer", "vérifier", "aller"]),
    ("vous", "pouvez", &["utiliser", "voir", "faire", "vérifier", "nous"]),
    ("merci", "beaucoup", &["pour", "de", "à"]),
    ("merci", "pour", &["votre", "ton", "la", "le", "ce", "cette", "tout"]),
    ("s'il", "vous", &["plaît"]),
    ("s'il", "te", &["plaît"]),
    ("comment", "ça", &["va"]),
    ("bonne", "journée", &["à bientôt", "merci"]),
    ("bonne", "soirée", &["à bientôt", "merci"]),
    ("à", "bientôt", &["merci", "j'espère"]),
    ("bien", "sûr", &["que", "je", "nous", "c'est"]),
    ("je", "suis", &["désolé", "ravi", "prêt", "d'accord", "là"]),
    ("je", "pense", &["que", "qu'il", "qu'on"]),
    ("par", "exemple", &["dans", "pour", "si", "nous"]),
    ("tout", "le", &["monde", "temps"]),
    ("aujourd'hui", "nous", &["allons", "avons", "pouvons"]),
    // German
    ("ich", "möchte", &["gerne", "eine", "einen", "das", "wissen", "fragen"]),
    ("wir", "können", &["das", "gerne", "auch", "zusammen", "machen", "sehen"]),
    ("vielen", "dank", &["für", "im voraus", "nochmals"]),
    ("danke", "schön", &["für", "und"]),
    ("bitte", "schön", &["gerne"]),
    ("guten", "morgen", &["zusammen", "alle"]),
    ("guten", "tag", &["zusammen", "sehr"]),
    ("guten", "abend", &["zusammen"]),
    ("mit", "freundlichen", &["grüßen"]),
    ("liebe", "grüße", &["aus", "und"]),
    ("viele", "grüße", &["aus", "und"]),
    ("ich", "bin", &["mir", "sehr", "gerade", "einverstanden"]),
    ("ich", "habe", &["eine", "einen", "das", "schon", "noch"]),
    ("wir", "haben", &["eine", "das", "ein", "bereits"]),
    ("zum", "beispiel", &["wenn", "für", "in", "können"]),
    ("kein", "problem", &["gerne", "ich"]),
    ("bis", "bald", &["viele grüße"]),
    // Spanish
    ("vamos", "a", &["ver", "hacer", "ir", "empezar", "comer", "hablar"]),
    ("voy", "a", &["hacer", "ver", "ir", "intentar", "revisar", "salir"]),
    ("dónde", "estás", &["ahora", "trabajando", "viviendo", "hoy"]),
    ("muchas", "gracias", &["por", "de", "a"]),
    ("buenos", "días", &["a todos", "cómo estás"]),
    ("buenas", "tardes", &["a todos", "cómo estás"]),
    ("buenas", "noches", &["a todos", "que descanses"]),
    ("por", "favor", &["dime", "ayúdame", "revisa", "confirma"]),
    ("hasta", "luego", &["gracias"]),
    ("hasta", "mañana", &["gracias"]),
    ("cómo", "estás", &["hoy", "todo bien"]),
    ("como", "estas", &["hoy", "todo bien"]),
    ("lo", "siento", &["mucho", "pero", "por"]),
    ("sin", "embargo", &["el", "la", "es", "no"]),
    ("por", "ejemplo", &["si", "en", "para", "cuando"]),
    ("por", "supuesto", &["que", "sí", "podemos"]),
    ("de", "acuerdo", &["con", "entonces", "perfecto"]),
    ("mucho", "más", &["que", "rápido", "claro", "fácil"]),
];

/// 單元詞語詞搭配表 w_t -> 高機率後繼詞清單
const UNIGRAM_COLLOCATIONS: &[(&str, &[&str])] = &[
    // English
    ("where", &["are you", "are", "is", "do you", "can", "to"]),
    ("what", &["are you", "is", "do you", "about", "if", "time"]),
    ("when", &["are you", "you", "is", "will", "can"]),
    ("why", &["are you", "not", "don't", "is", "do"]),
    ("thank", &["you", "you very much", "you so much", "goodness"]),
    ("thanks", &["for", "a lot", "again", "very much", "so much"]),
    ("please", &["let me know", "check", "note", "find", "help", "wait"]),
    ("hello", &["everyone", "there", "world", "team"]),
    ("good", &["morning", "afternoon", "evening", "night", "luck", "idea", "job"]),
    ("happy", &["birthday", "new year", "to help", "to see"]),
    ("best", &["regards", "wishes", "practice", "way"]),
    ("kind", &["regards"]),
    ("sincerely", &["yours"]),
    ("dear", &["sir", "madam", "team", "all", "friend"]),
    ("how", &["are you", "about", "to", "much", "many", "long"]),
    ("would", &["you", "like to", "be", "appreciate"]),
    ("looking", &["forward to", "for", "at", "good"]),
    // French
    ("merci", &["beaucoup", "pour", "à vous", "bien", "d'avance"]),
    ("bonjour", &["à tous", "madame", "monsieur", "comment allez-vous"]),
    ("salut", &["tout le monde", "ça va"]),
    ("au", &["revoir"]),
    ("comment", &["allez-vous", "ça va", "puis-je"]),
    // German
    ("vielen", &["dank"]),
    ("danke", &["schön", "für", "sehr", "dir", "ihnen"]),
    ("guten", &["morgen", "tag", "abend"]),
    ("herzlichen", &["glückwunsch", "dank"]),
    ("mit", &["freundlichen grüßen"]),
    // Spanish
    ("estoy", &["haciendo", "trabajando", "en", "muy", "de acuerdo", "buscando"]),
    ("gracias", &["por", "a todos", "muchas", "de verdad"]),
    ("hola", &["a todos", "cómo estás", "amigo"]),
    ("buenos", &["días"]),
    ("buenas", &["tardes", "noches"]),
    ("muchas", &["gracias"]),
    ("por", &["favor", "ejemplo", "supuesto"]),
];

fn normalize_no_diacritic(word: &str) -> String {
    word.chars()
        .map(WesternEngine::strip_char_diacritic)
        .collect::<String>()
        .to_lowercase()
}

fn push_unique(out: &mut Vec<String>, cand: &str, max_results: usize) {
    if out.len() >= max_results {
        return;
    }
    if !out.iter().any(|s| s == cand) {
        out.push(cand.to_string());
    }
}

/// 依已切出之前導歐文單字列（1..=3 詞）查詢搭配後繼詞（支援完整重音與無重音鍵入比對）
pub fn lookup_continuations_for_words(words: &[String], max_results: usize) -> Vec<String> {
    if words.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let n = words.len();
    let norm: Vec<String> = words.iter().map(|w| normalize_no_diacritic(w)).collect();

    // 1. Trigram 精確匹配與無重音降級匹配
    if n >= 3 {
        let (a, b, c) = (&words[n - 3], &words[n - 2], &words[n - 1]);
        let (na, nb, nc) = (&norm[n - 3], &norm[n - 2], &norm[n - 1]);
        for &(t1, t2, t3, cands) in TRIGRAM_COLLOCATIONS {
            let exact = a == t1 && b == t2 && c == t3;
            let ascii_match = !exact
                && *na == normalize_no_diacritic(t1)
                && *nb == normalize_no_diacritic(t2)
                && *nc == normalize_no_diacritic(t3);
            if exact || ascii_match {
                for &cand in cands {
                    push_unique(&mut out, cand, max_results);
                }
            }
        }
    }

    // 2. Bigram
    if n >= 2 && out.len() < max_results {
        let (b, c) = (&words[n - 2], &words[n - 1]);
        let (nb, nc) = (&norm[n - 2], &norm[n - 1]);
        for &(b1, b2, cands) in BIGRAM_COLLOCATIONS {
            let exact = b == b1 && c == b2;
            let ascii_match =
                !exact && *nb == normalize_no_diacritic(b1) && *nc == normalize_no_diacritic(b2);
            if exact || ascii_match {
                for &cand in cands {
                    push_unique(&mut out, cand, max_results);
                }
            }
        }
    }

    // 3. Unigram
    if out.len() < max_results {
        let c = &words[n - 1];
        let nc = &norm[n - 1];
        for &(u1, cands) in UNIGRAM_COLLOCATIONS {
            let exact = c == u1;
            let ascii_match = !exact && *nc == normalize_no_diacritic(u1);
            if exact || ascii_match {
                for &cand in cands {
                    push_unique(&mut out, cand, max_results);
                }
            }
        }
    }

    out
}

// ============================================================================
// 因式分解正交句法構式分析與跨語系形態評分器
// (Factorized Syntactic Slot Projections & Morphosyntactic Context Analyzer)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum WhKind {
    Where,
    What,
    When,
    Why,
    How,
    Who,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GerundPrepKind {
    ThankSorryFor,
    InsteadOf,
    General,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeterminerKind {
    IndefiniteAn,
    IndefiniteA,
    DefiniteOrPossessive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EuroLangFrame {
    FrenchModalOrFuture,
    FrenchPerfect,
    GermanModalOrFuture,
    SpanishProgressive,
    SpanishModalOrFuture,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClauseFrame {
    WhBeSubject(WhKind),
    BeProgressive,
    InfinitivalTo,
    PrepositionalTo,
    ModalOrDoSupport,
    PerfectHave { locative_wh: bool },
    GerundPrep(GerundPrepKind),
    Determiner(DeterminerKind),
    Intensifier { can_take_much: bool },
    Euro(EuroLangFrame),
    None,
}

#[inline]
fn is_transparent_adverb(w: &str) -> bool {
    matches!(
        w,
        "still"
            | "just"
            | "already"
            | "also"
            | "even"
            | "ever"
            | "never"
            | "not"
            | "now"
            | "currently"
            | "really"
            | "actually"
            | "probably"
            | "definitely"
            | "always"
            | "often"
            | "usually"
            | "only"
            | "please"
    )
}

#[inline]
fn is_be_aux(w: &str) -> bool {
    matches!(
        w,
        "am" | "is" | "are" | "was" | "were" | "isn't" | "aren't" | "wasn't" | "weren't"
    )
}

#[inline]
fn is_be_contraction(w: &str) -> bool {
    matches!(w, "i'm" | "you're" | "we're" | "they're" | "he's" | "she's" | "it's")
}

#[inline]
fn is_subject_pronoun(w: &str) -> bool {
    matches!(
        w,
        "i" | "you" | "we" | "they" | "he" | "she" | "it" | "this" | "that" | "these" | "those"
    )
}

#[inline]
fn is_have_aux(w: &str) -> bool {
    matches!(w, "have" | "has" | "had")
}

#[inline]
fn is_perfect_have_adverb(w: &str) -> bool {
    matches!(
        w,
        "already" | "just" | "ever" | "never" | "not" | "also" | "recently" | "still"
    )
}

#[inline]
fn is_have_contraction(w: &str) -> bool {
    matches!(
        w,
        "i've"
            | "you've"
            | "we've"
            | "they've"
            | "haven't"
            | "hasn't"
            | "hadn't"
            | "should've"
            | "could've"
            | "would've"
            | "must've"
    )
}

#[inline]
fn is_modal_or_do_aux(w: &str) -> bool {
    matches!(
        w,
        "can"
            | "could"
            | "will"
            | "would"
            | "shall"
            | "should"
            | "may"
            | "might"
            | "must"
            | "do"
            | "does"
            | "did"
            | "can't"
            | "cannot"
            | "couldn't"
            | "won't"
            | "wouldn't"
            | "shouldn't"
            | "don't"
            | "doesn't"
            | "didn't"
            | "i'll"
            | "you'll"
            | "we'll"
            | "they'll"
            | "he'll"
            | "she'll"
            | "it'll"
            | "let's"
    )
}

#[inline]
fn is_finite_aux_token(w: &str) -> bool {
    matches!(
        w,
        "am" | "is"
            | "are"
            | "was"
            | "were"
            | "do"
            | "does"
            | "did"
            | "can"
            | "could"
            | "will"
            | "would"
            | "shall"
            | "should"
            | "may"
            | "might"
            | "must"
            | "have"
            | "has"
            | "had"
    )
}

#[inline]
fn is_nominal_ing_word(w: &str) -> bool {
    matches!(
        w,
        "thing"
            | "things"
            | "something"
            | "nothing"
            | "anything"
            | "everything"
            | "king"
            | "ring"
            | "sing"
            | "bring"
            | "spring"
            | "string"
            | "wing"
            | "morning"
            | "evening"
            | "ceiling"
            | "building"
            | "meeting"
            | "wedding"
    )
}

#[inline]
fn is_verbal_ing(w: &str) -> bool {
    w.len() >= 5 && w.ends_with("ing") && !is_nominal_ing_word(w)
}

#[inline]
fn is_regular_ed(w: &str) -> bool {
    w.len() >= 4
        && w.ends_with("ed")
        && !matches!(
            w,
            "red" | "bed" | "fed" | "led" | "shed" | "need" | "feed" | "seed" | "speed" | "bleed"
                | "hundred"
        )
}

#[inline]
fn is_irregular_past_participle(w: &str) -> bool {
    matches!(
        w,
        "been" | "done"
            | "gone"
            | "seen"
            | "taken"
            | "given"
            | "made"
            | "known"
            | "found"
            | "got"
            | "gotten"
            | "had"
            | "said"
            | "told"
            | "written"
            | "sent"
            | "come"
            | "become"
            | "heard"
            | "left"
            | "lost"
            | "met"
            | "paid"
            | "thought"
            | "bought"
            | "brought"
            | "chosen"
            | "spoken"
            | "broken"
            | "driven"
            | "eaten"
            | "fallen"
            | "felt"
            | "held"
            | "kept"
            | "read"
            | "run"
            | "shown"
            | "slept"
            | "stood"
            | "understood"
            | "worn"
    )
}

#[inline]
fn is_irregular_past_or_participle(w: &str) -> bool {
    matches!(
        w,
        "went" | "gone"
            | "saw"
            | "seen"
            | "did"
            | "done"
            | "was"
            | "were"
            | "been"
            | "got"
            | "gotten"
            | "took"
            | "taken"
            | "gave"
            | "given"
            | "made"
            | "knew"
            | "known"
            | "came"
            | "had"
            | "has"
            | "am"
            | "is"
            | "are"
            | "spoke"
            | "spoken"
            | "wrote"
            | "written"
            | "ate"
            | "eaten"
            | "fell"
            | "fallen"
            | "drove"
            | "driven"
            | "flew"
            | "flown"
            | "said"
            | "told"
            | "sent"
            | "found"
            | "bought"
            | "brought"
            | "thought"
            | "left"
            | "lost"
            | "met"
            | "held"
            | "kept"
            | "stood"
    )
}

#[inline]
fn is_bare_non_adj_verb(w: &str) -> bool {
    matches!(
        w,
        "go" | "goes"
            | "went"
            | "get"
            | "gets"
            | "give"
            | "gives"
            | "gave"
            | "make"
            | "makes"
            | "take"
            | "takes"
            | "do"
            | "does"
            | "did"
            | "come"
            | "comes"
            | "came"
            | "see"
            | "sees"
            | "saw"
            | "say"
            | "says"
            | "know"
            | "knows"
            | "think"
            | "thinks"
            | "want"
            | "wants"
            | "need"
            | "needs"
    )
}

#[inline]
fn is_common_base_verb(w: &str) -> bool {
    matches!(
        w,
        "be" | "go"
            | "get"
            | "do"
            | "have"
            | "make"
            | "see"
            | "take"
            | "give"
            | "know"
            | "find"
            | "use"
            | "help"
            | "check"
            | "send"
            | "share"
            | "start"
            | "work"
            | "tell"
            | "say"
            | "come"
            | "think"
            | "want"
            | "need"
            | "like"
            | "mean"
            | "try"
            | "keep"
            | "show"
            | "hear"
            | "hold"
            | "handle"
            | "let"
            | "ask"
            | "confirm"
            | "solve"
            | "save"
            | "set"
            | "stay"
            | "stop"
            | "study"
            | "speak"
            | "support"
            | "feel"
            | "follow"
            | "fix"
            | "finish"
            | "update"
            | "understand"
            | "hope"
            | "create"
            | "change"
            | "call"
            | "continue"
            | "become"
            | "bring"
            | "build"
            | "buy"
            | "add"
            | "apply"
            | "avoid"
            | "allow"
            | "wait"
            | "write"
            | "watch"
            | "walk"
            | "read"
            | "run"
            | "return"
            | "reach"
            | "leave"
            | "learn"
            | "live"
            | "look"
            | "love"
            | "turn"
            | "talk"
            | "travel"
            | "eat"
            | "enjoy"
            | "ensure"
            | "explain"
    )
}

#[inline]
fn is_subject_np_boundary_blocker(w: &str) -> bool {
    is_verbal_ing(w)
        || matches!(
            w,
            "to" | "going"
                | "doing"
                | "been"
                | "done"
                | "in"
                | "on"
                | "at"
                | "for"
                | "from"
                | "with"
                | "by"
                | "of"
                | "and"
                | "or"
                | "if"
                | "that"
                | "who"
                | "which"
        )
}

fn strip_trailing_adverbs(prev_words: &[String]) -> Vec<String> {
    let mut end = prev_words.len();
    while end > 0 && is_transparent_adverb(&prev_words[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        prev_words.to_vec()
    } else {
        prev_words[..end].to_vec()
    }
}

fn parse_wh_kind(w: &str) -> Option<WhKind> {
    match w {
        "where" => Some(WhKind::Where),
        "what" => Some(WhKind::What),
        "when" => Some(WhKind::When),
        "why" => Some(WhKind::Why),
        "how" => Some(WhKind::How),
        "who" => Some(WhKind::Who),
        _ => None,
    }
}

fn is_french_modal_or_future(w: &str) -> bool {
    matches!(
        w,
        "vais"
            | "vas"
            | "va"
            | "allons"
            | "allez"
            | "vont"
            | "peux"
            | "peut"
            | "pouvons"
            | "pouvez"
            | "peuvent"
            | "veux"
            | "veut"
            | "voulons"
            | "voulez"
            | "voudrais"
            | "dois"
            | "doit"
            | "devons"
            | "devez"
            | "faut"
    )
}

fn is_german_modal_or_future(w: &str) -> bool {
    matches!(
        w,
        "kann"
            | "kannst"
            | "können"
            | "konnen"
            | "könnt"
            | "muss"
            | "musst"
            | "müssen"
            | "mussen"
            | "will"
            | "willst"
            | "wollen"
            | "möchte"
            | "mochte"
            | "möchtest"
            | "möchten"
            | "mochten"
            | "soll"
            | "sollst"
            | "sollen"
            | "werde"
            | "wirst"
            | "werden"
    )
}

fn is_spanish_estar(w: &str) -> bool {
    matches!(
        w,
        "estoy" | "estás" | "estas" | "está" | "esta" | "estamos" | "están" | "estan"
    )
}

fn is_spanish_modal_or_ir_a(core: &[String]) -> bool {
    let Some(last) = core.last().map(|s| s.as_str()) else {
        return false;
    };
    if matches!(
        last,
        "puedo"
            | "puedes"
            | "puede"
            | "podemos"
            | "quiero"
            | "quieres"
            | "quiere"
            | "queremos"
            | "necesito"
            | "necesitas"
            | "debo"
            | "debes"
    ) {
        return true;
    }
    last == "a"
        && core.len() >= 2
        && matches!(
            core[core.len() - 2].as_str(),
            "voy" | "vas" | "va" | "vamos" | "van"
        )
}

fn analyze_clause_frame(prev_words: &[String]) -> ClauseFrame {
    if prev_words.is_empty() {
        return ClauseFrame::None;
    }
    let raw_last = prev_words[prev_words.len() - 1].as_str();

    // Exact "to": discriminate prepositional gerund `to` vs infinitival `to` BEFORE adverb stripping
    if raw_last == "to" {
        if prev_words.len() >= 2 {
            let before_to = prev_words[prev_words.len() - 2].as_str();
            let before2 = if prev_words.len() >= 3 {
                prev_words[prev_words.len() - 3].as_str()
            } else {
                ""
            };
            let is_prep_used_to = before_to == "used"
                && (is_be_aux(before2)
                    || is_be_contraction(before2)
                    || matches!(before2, "be" | "been" | "being" | "get" | "gets" | "got" | "getting"));
            if is_prep_used_to
                || matches!(
                    before_to,
                    "forward"
                        | "according"
                        | "due"
                        | "thanks"
                        | "prior"
                        | "compared"
                        | "addition"
                        | "respect"
                        | "close"
                        | "next"
                        | "up"
                        | "listen"
                        | "listening"
                )
            {
                return ClauseFrame::PrepositionalTo;
            }
            if matches!(
                before_to,
                "going"
                    | "want"
                    | "wants"
                    | "wanted"
                    | "need"
                    | "needs"
                    | "needed"
                    | "like"
                    | "likes"
                    | "liked"
                    | "love"
                    | "try"
                    | "tries"
                    | "tried"
                    | "trying"
                    | "plan"
                    | "plans"
                    | "planned"
                    | "planning"
                    | "hope"
                    | "hoping"
                    | "decide"
                    | "decided"
                    | "expect"
                    | "expected"
                    | "able"
                    | "unable"
                    | "ready"
                    | "happy"
                    | "glad"
                    | "sorry"
                    | "easy"
                    | "hard"
                    | "difficult"
                    | "important"
                    | "necessary"
                    | "free"
                    | "hesitate"
                    | "order"
                    | "have"
                    | "has"
                    | "had"
                    | "ought"
                    | "used"
                    | "seem"
                    | "seems"
                    | "begin"
                    | "start"
                    | "started"
                    | "continue"
                    | "forget"
                    | "remember"
                    | "learn"
                    | "ask"
                    | "asked"
                    | "tell"
                    | "told"
                    | "allow"
            ) {
                return ClauseFrame::InfinitivalTo;
            }
        }
        return ClauseFrame::None;
    }

    // Intensifiers (must inspect raw_last so "very", "so", "really" aren't stripped)
    if matches!(
        raw_last,
        "very" | "so" | "really" | "quite" | "too" | "extremely"
    ) {
        return ClauseFrame::Intensifier {
            can_take_much: matches!(raw_last, "very" | "so" | "too"),
        };
    }

    // Determiners (inspect raw_last)
    match raw_last {
        "an" => return ClauseFrame::Determiner(DeterminerKind::IndefiniteAn),
        "a" => return ClauseFrame::Determiner(DeterminerKind::IndefiniteA),
        "the" | "my" | "your" | "our" | "their" | "his" | "her" | "these" | "those" => {
            return ClauseFrame::Determiner(DeterminerKind::DefiniteOrPossessive);
        }
        _ => {}
    }

    // Perfect HAVE with explicit trailing perfect adverb ("have/has/had + already/just/ever/never/not/also/recently")
    if is_perfect_have_adverb(raw_last) && prev_words.len() >= 2 {
        let pre_adv = prev_words[prev_words.len() - 2].as_str();
        if is_have_aux(pre_adv) || is_have_contraction(pre_adv) {
            return ClauseFrame::PerfectHave {
                locative_wh: prev_words.iter().any(|w| w == "where"),
            };
        }
    }

    let core = strip_trailing_adverbs(prev_words);
    if core.is_empty() {
        return ClauseFrame::None;
    }
    let core_last = core[core.len() - 1].as_str();
    let core_prev = if core.len() >= 2 {
        core[core.len() - 2].as_str()
    } else {
        ""
    };

    // Wh + BE + Subject NP (1..=3 subject/determiner tokens)
    let search_start = core.len().saturating_sub(6);
    for wh_idx in search_start..core.len() {
        if let Some(wh) = parse_wh_kind(core[wh_idx].as_str()) {
            if wh_idx + 2 <= core.len() && is_be_aux(core[wh_idx + 1].as_str()) {
                let subj = &core[wh_idx + 2..];
                if (1..=3).contains(&subj.len())
                    && subj.iter().all(|w| !is_subject_np_boundary_blocker(w))
                {
                    return ClauseFrame::WhBeSubject(wh);
                }
            }
        }
    }

    // Gerund-taking prepositions
    if core_last == "for"
        && core.iter().any(|w| {
            matches!(
                w.as_str(),
                "thank" | "thanks" | "sorry" | "apologize" | "waiting" | "looking" | "responsible"
            )
        })
    {
        return ClauseFrame::GerundPrep(GerundPrepKind::ThankSorryFor);
    }
    if core_last == "of" && matches!(core_prev, "instead" | "capable" | "tired" | "afraid" | "proud")
    {
        let kind = if core_prev == "instead" {
            GerundPrepKind::InsteadOf
        } else {
            GerundPrepKind::General
        };
        return ClauseFrame::GerundPrep(kind);
    }
    if matches!(core_last, "without" | "before" | "after" | "by" | "while" | "upon")
        || (core_last == "in" && core_prev == "interested")
        || (core_last == "at" && core_prev == "good")
        || (core_last == "about" && matches!(core_prev, "thinking" | "talking"))
    {
        return ClauseFrame::GerundPrep(GerundPrepKind::General);
    }

    // Perfect HAVE
    if is_have_contraction(core_last) {
        return ClauseFrame::PerfectHave {
            locative_wh: core.iter().any(|w| w == "where"),
        };
    }
    if is_subject_pronoun(core_last) && is_have_aux(core_prev) {
        let pre_have = if core.len() >= 3 {
            core[core.len() - 3].as_str()
        } else {
            ""
        };
        if !matches!(
            pre_have,
            "do" | "does"
                | "did"
                | "can"
                | "could"
                | "will"
                | "would"
                | "should"
                | "may"
                | "might"
                | "must"
                | "to"
        ) {
            return ClauseFrame::PerfectHave {
                locative_wh: core.iter().any(|w| w == "where"),
            };
        }
    }

    // Modal & DO-support (direct auxiliary or inverted "[aux] + [subj_pronoun]")
    if is_modal_or_do_aux(core_last) {
        return ClauseFrame::ModalOrDoSupport;
    }
    if is_subject_pronoun(core_last) && core.len() >= 2 && is_modal_or_do_aux(core_prev) {
        return ClauseFrame::ModalOrDoSupport;
    }

    // Pan-European FR / DE / ES Frames
    if is_french_modal_or_future(core_last) {
        return ClauseFrame::Euro(EuroLangFrame::FrenchModalOrFuture);
    }
    if core_last == "j'ai"
        || (matches!(core_last, "as" | "avons" | "avez") && matches!(core_prev, "tu" | "nous" | "vous"))
    {
        return ClauseFrame::Euro(EuroLangFrame::FrenchPerfect);
    }
    if is_german_modal_or_future(core_last) {
        return ClauseFrame::Euro(EuroLangFrame::GermanModalOrFuture);
    }
    if is_spanish_estar(core_last) {
        return ClauseFrame::Euro(EuroLangFrame::SpanishProgressive);
    }
    if is_spanish_modal_or_ir_a(&core) {
        return ClauseFrame::Euro(EuroLangFrame::SpanishModalOrFuture);
    }

    // Progressive `been`, Polar `[BE] + [Subj]`, and Declarative `[Subj != there/here] + [BE]`
    if core_last == "been" && (is_have_aux(core_prev) || is_have_contraction(core_prev) || is_have_aux(if core.len() >= 3 { core[core.len() - 3].as_str() } else { "" })) {
        return ClauseFrame::BeProgressive;
    }
    if is_be_contraction(core_last) {
        return ClauseFrame::BeProgressive;
    }
    if is_be_aux(core_last)
        && core.len() >= 2
        && !matches!(core_prev, "there" | "here" | "where" | "what" | "when" | "why" | "how" | "who")
    {
        return ClauseFrame::BeProgressive;
    }
    if is_subject_pronoun(core_last) && core.len() == 2 && is_be_aux(core_prev) {
        return ClauseFrame::BeProgressive;
    }

    ClauseFrame::None
}

#[inline]
fn is_vowel_initial_for_an(w: &str) -> bool {
    if matches!(w, "hour" | "hours" | "honest" | "honor" | "honors" | "honour" | "heir") {
        return true;
    }
    if w.starts_with("uni")
        || w.starts_with("use")
        || w.starts_with("eu")
        || matches!(w, "one" | "once")
    {
        return false;
    }
    w.starts_with(['a', 'e', 'i', 'o', 'u'])
}

/// 依句法框架生成無字首（空白後 Tab）與語幹候選延伸詞清單
pub fn lookup_syntactic_frame_continuations(prev_words: &[String], max_results: usize) -> Vec<String> {
    if prev_words.is_empty() || max_results == 0 {
        return Vec::new();
    }
    let list: &[&str] = match analyze_clause_frame(prev_words) {
        ClauseFrame::WhBeSubject(WhKind::Where) => &[
            "going", "from", "going to", "staying", "now", "heading", "located", "working",
            "living", "guys",
        ],
        ClauseFrame::WhBeSubject(WhKind::What) => &[
            "doing", "going to", "looking for", "talking about", "working on", "thinking",
            "up to", "eating",
        ],
        ClauseFrame::WhBeSubject(WhKind::When) => &[
            "going", "coming", "leaving", "arriving", "starting", "free", "available", "ready",
        ],
        ClauseFrame::WhBeSubject(WhKind::Why | WhKind::How | WhKind::Who) => &[
            "doing", "feeling", "going", "looking for", "asking", "saying", "waiting", "still",
        ],
        ClauseFrame::BeProgressive => {
            if prev_words.len() >= 2
                && prev_words[prev_words.len() - 2] == "are"
                && prev_words[prev_words.len() - 1] == "you"
            {
                &["going", "sure", "ready", "doing", "okay", "looking", "guys", "free", "available"]
            } else {
                &["going", "working", "looking", "ready", "glad", "trying", "planning", "available", "done"]
            }
        }
        ClauseFrame::InfinitivalTo => {
            &["see", "do", "make", "get", "be", "have", "go", "know", "check", "work", "start"]
        }
        ClauseFrame::PrepositionalTo => {
            &["seeing", "hearing", "working", "meeting", "the", "your", "this"]
        }
        ClauseFrame::ModalOrDoSupport => {
            &["help", "check", "get", "go", "make", "see", "do", "have", "know", "find", "share"]
        }
        ClauseFrame::PerfectHave { locative_wh: true } => {
            &["been", "gone", "gotten", "done", "seen", "found"]
        }
        ClauseFrame::PerfectHave { locative_wh: false } => {
            &["been", "done", "seen", "already", "got", "finished", "sent", "tried", "heard"]
        }
        ClauseFrame::GerundPrep(GerundPrepKind::ThankSorryFor) => {
            &["your", "the", "sharing", "helping", "waiting", "coming", "reaching", "understanding"]
        }
        ClauseFrame::GerundPrep(GerundPrepKind::InsteadOf) => {
            &["using", "doing", "going", "having", "making", "the"]
        }
        ClauseFrame::GerundPrep(GerundPrepKind::General) => {
            &["using", "going", "making", "doing", "checking", "learning", "working", "the"]
        }
        ClauseFrame::Euro(EuroLangFrame::FrenchModalOrFuture) => {
            &["faire", "voir", "aller", "prendre", "utiliser", "vérifier", "travailler"]
        }
        ClauseFrame::Euro(EuroLangFrame::FrenchPerfect) => {
            &["fait", "vu", "pris", "dit", "eu", "été", "pu", "reçu", "trouvé"]
        }
        ClauseFrame::Euro(EuroLangFrame::GermanModalOrFuture) => {
            &["gerne", "machen", "gehen", "sehen", "wissen", "fragen", "arbeiten", "helfen"]
        }
        ClauseFrame::Euro(EuroLangFrame::SpanishProgressive) => {
            &["haciendo", "trabajando", "yendo", "buscando", "esperando", "hablando", "viendo"]
        }
        ClauseFrame::Euro(EuroLangFrame::SpanishModalOrFuture) => {
            &["hacer", "ver", "ir", "decir", "saber", "hablar", "trabajar", "comer"]
        }
        ClauseFrame::Determiner(_) | ClauseFrame::Intensifier { .. } | ClauseFrame::None => &[],
    };

    list.iter()
        .take(max_results)
        .map(|&s| s.to_string())
        .collect()
}

/// 泛歐文語幹外掛補全清單（涵蓋法／德／西屈折與重音形，補充 ASCII 為主之外掛英語字典）
pub fn lookup_pan_european_prefix_candidates(prev_words: &[String], prefix_lower: &str) -> Vec<String> {
    if prefix_lower.is_empty() {
        return Vec::new();
    }
    let pool: &[&str] = match analyze_clause_frame(prev_words) {
        ClauseFrame::Euro(EuroLangFrame::FrenchModalOrFuture) => &[
            "faire", "voir", "aller", "prendre", "partir", "venir", "savoir", "dire", "parler",
            "travailler", "aider", "envoyer", "vérifier", "utiliser", "être",
        ],
        ClauseFrame::Euro(EuroLangFrame::FrenchPerfect) => &[
            "fait", "vu", "pris", "dit", "eu", "été", "pu", "voulu", "reçu", "envoyé", "trouvé",
            "fini", "compris",
        ],
        ClauseFrame::Euro(EuroLangFrame::GermanModalOrFuture) => &[
            "gerne", "machen", "gehen", "sehen", "kommen", "wissen", "sagen", "helfen", "arbeiten",
            "finden", "geben", "nehmen", "schreiben", "fragen",
        ],
        ClauseFrame::Euro(EuroLangFrame::SpanishProgressive) => &[
            "haciendo", "yendo", "trabajando", "buscando", "esperando", "viendo", "diciendo",
            "pensando", "hablando", "comiendo",
        ],
        ClauseFrame::Euro(EuroLangFrame::SpanishModalOrFuture) => &[
            "hacer", "ver", "ir", "decir", "saber", "hablar", "trabajar", "comer", "salir",
            "venir", "ayudar", "enviar",
        ],
        _ => &[],
    };

    pool.iter()
        .copied()
        .filter(|w| {
            let norm = normalize_no_diacritic(w);
            (w.starts_with(prefix_lower) || norm.starts_with(prefix_lower)) && *w != prefix_lower
        })
        .map(|w| w.to_string())
        .collect()
}

/// 計算單一候選詞在左前子句脈絡 (`prev_words`) 與右鄰詞 (`next_word`) 下之語法匹配加／扣分
pub fn score_candidate_in_clause_context(
    prev_words: &[String],
    candidate_lower: &str,
    next_word: &str,
) -> f32 {
    let mut score = 0.0f32;
    let w = candidate_lower;

    match analyze_clause_frame(prev_words) {
        ClauseFrame::WhBeSubject(wh) => {
            match wh {
                WhKind::Where => {
                    if w == "going" {
                        score += 34.0;
                    } else if matches!(
                        w,
                        "getting"
                            | "heading"
                            | "staying"
                            | "coming"
                            | "living"
                            | "working"
                            | "traveling"
                            | "travelling"
                            | "walking"
                            | "driving"
                            | "flying"
                            | "moving"
                            | "sitting"
                            | "standing"
                            | "running"
                            | "planning"
                            | "leaving"
                            | "arriving"
                            | "visiting"
                            | "waiting"
                            | "looking"
                            | "eating"
                            | "studying"
                            | "sleeping"
                    ) {
                        score += 31.0;
                    } else if is_verbal_ing(w) {
                        score += 30.0;
                    } else if matches!(
                        w,
                        "from" | "now" | "located" | "based" | "guys" | "folks" | "at" | "here"
                            | "there"
                    ) {
                        score += 24.0;
                    } else if matches!(w, "good" | "glad" | "ready" | "free" | "available") {
                        score += 8.0;
                    } else if is_bare_non_adj_verb(w) {
                        score -= 34.0;
                    } else if matches!(
                        w,
                        "god" | "goal" | "graph" | "gpu" | "git" | "github" | "gitlab"
                    ) {
                        score -= 18.0;
                    }
                }
                WhKind::What => {
                    if w == "doing" {
                        score += 34.0;
                    } else if matches!(
                        w,
                        "going"
                            | "looking"
                            | "talking"
                            | "working"
                            | "thinking"
                            | "making"
                            | "trying"
                            | "saying"
                            | "waiting"
                            | "planning"
                            | "reading"
                            | "watching"
                            | "eating"
                            | "using"
                            | "getting"
                            | "having"
                    ) {
                        score += 30.0;
                    } else if is_verbal_ing(w) {
                        score += 32.0;
                    } else if is_bare_non_adj_verb(w) {
                        score -= 34.0;
                    }
                }
                WhKind::When => {
                    if matches!(
                        w,
                        "going"
                            | "coming"
                            | "leaving"
                            | "arriving"
                            | "starting"
                            | "finishing"
                            | "returning"
                            | "flying"
                            | "free"
                            | "available"
                            | "ready"
                            | "back"
                    ) {
                        score += 30.0;
                    } else if is_verbal_ing(w) {
                        score += 28.0;
                    } else if is_bare_non_adj_verb(w) {
                        score -= 32.0;
                    }
                }
                WhKind::Why | WhKind::How | WhKind::Who => {
                    if matches!(
                        w,
                        "doing"
                            | "feeling"
                            | "going"
                            | "getting"
                            | "saying"
                            | "asking"
                            | "looking"
                            | "waiting"
                            | "holding"
                    ) {
                        score += 30.0;
                    } else if is_verbal_ing(w) {
                        score += 32.0;
                    } else if matches!(w, "today" | "guys" | "still" | "so" | "here") {
                        score += 24.0;
                    } else if is_bare_non_adj_verb(w) {
                        score -= 32.0;
                    }
                }
            }
        }
        ClauseFrame::BeProgressive => {
            let after_are_you = prev_words.len() >= 2
                && prev_words[prev_words.len() - 2] == "are"
                && prev_words[prev_words.len() - 1] == "you";
            if w == "going" {
                score += 30.0;
            } else if after_are_you && w == "guys" {
                score += 24.0;
            } else if matches!(
                w,
                "working"
                    | "looking"
                    | "trying"
                    | "planning"
                    | "waiting"
                    | "getting"
                    | "doing"
                    | "making"
                    | "using"
                    | "having"
                    | "coming"
                    | "thinking"
                    | "talking"
                    | "writing"
                    | "running"
                    | "building"
                    | "testing"
                    | "starting"
                    | "hoping"
                    | "wondering"
                    | "sharing"
                    | "helping"
                    | "seeing"
                    | "staying"
                    | "living"
            ) {
                score += 27.0;
            } else if is_verbal_ing(w) {
                score += 28.0;
            } else if matches!(
                w,
                "glad"
                    | "happy"
                    | "ready"
                    | "sure"
                    | "sorry"
                    | "able"
                    | "available"
                    | "free"
                    | "welcome"
                    | "okay"
                    | "ok"
                    | "fine"
                    | "busy"
                    | "tired"
                    | "interested"
                    | "excited"
            ) {
                score += 25.0;
            } else if matches!(w, "done" | "finished" | "based" | "located" | "invited" | "updated" | "fixed")
            {
                score += 26.0;
            } else if matches!(w, "good" | "great" | "right" | "clear" | "important") {
                score += 16.0;
            } else if is_bare_non_adj_verb(w) {
                score -= 32.0;
            }
        }
        ClauseFrame::InfinitivalTo => {
            if is_common_base_verb(w) {
                score += 27.0;
            } else if is_irregular_past_or_participle(w)
                || is_finite_aux_token(w)
                || matches!(w, "i" | "you" | "we" | "they" | "he" | "she" | "it")
            {
                score -= 35.0;
            } else if is_verbal_ing(w) || is_regular_ed(w) {
                score -= 28.0;
            } else if matches!(w, "so" | "some" | "such") {
                score -= 20.0;
            }
        }
        ClauseFrame::PrepositionalTo => {
            if is_verbal_ing(w) {
                score += 25.0;
            } else if matches!(w, "the" | "your" | "our" | "my" | "this" | "a" | "an") {
                score += 24.0;
            } else if is_finite_aux_token(w) {
                score -= 30.0;
            }
        }
        ClauseFrame::ModalOrDoSupport => {
            if is_common_base_verb(w) {
                score += 27.0;
            } else if is_irregular_past_or_participle(w) {
                score -= 35.0;
            } else if is_verbal_ing(w) || is_regular_ed(w) {
                score -= 30.0;
            } else if matches!(w, "does" | "did" | "has" | "had" | "am" | "is" | "are" | "was" | "were")
            {
                score -= 35.0;
            }
        }
        ClauseFrame::PerfectHave { locative_wh } => {
            if w == "been" {
                score += 32.0;
            } else if locative_wh && matches!(w, "gone" | "gotten" | "stayed" | "lived" | "worked")
            {
                score += 35.0;
            } else if is_irregular_past_participle(w) {
                score += 29.0;
            } else if is_regular_ed(w) {
                score += 25.0;
            } else if is_verbal_ing(w) {
                score -= 28.0;
            } else if matches!(
                w,
                "go" | "see" | "do" | "take" | "give" | "write" | "eat" | "know" | "speak"
                    | "drive" | "fly"
            ) {
                score -= 32.0;
            }
        }
        ClauseFrame::GerundPrep(kind) => {
            if kind == GerundPrepKind::ThankSorryFor
                && matches!(
                    w,
                    "sharing"
                        | "helping"
                        | "waiting"
                        | "coming"
                        | "asking"
                        | "understanding"
                        | "reaching"
                        | "sending"
                        | "supporting"
                )
            {
                score += 32.0;
            } else if kind == GerundPrepKind::InsteadOf
                && matches!(w, "using" | "doing" | "going" | "having" | "making")
            {
                score += 31.0;
            } else if is_verbal_ing(w) {
                score += 26.0;
            } else if matches!(w, "your" | "the" | "my" | "our" | "this" | "some") {
                score += 24.0;
            } else if is_finite_aux_token(w) || is_bare_non_adj_verb(w) {
                score -= 26.0;
            } else if is_regular_ed(w) || w == "used" {
                score -= 20.0;
            }
        }
        ClauseFrame::Determiner(det) => {
            if is_finite_aux_token(w)
                || matches!(w, "i" | "you" | "we" | "they" | "he" | "she" | "me" | "them")
            {
                score -= 35.0;
            } else if det == DeterminerKind::IndefiniteAn {
                if is_vowel_initial_for_an(w) {
                    score += 24.0;
                } else {
                    score -= 24.0;
                }
            } else if det == DeterminerKind::IndefiniteA
                && matches!(
                    w,
                    "apple" | "example" | "idea" | "issue" | "option" | "error" | "hour" | "object"
                )
            {
                score -= 18.0;
            }
        }
        ClauseFrame::Intensifier { can_take_much } => {
            if can_take_much && w == "much" {
                score += 32.0;
            } else if matches!(
                w,
                "good" | "great" | "well" | "important" | "useful" | "helpful" | "happy" | "glad"
                    | "nice" | "clear" | "easy" | "hard" | "fast" | "high" | "long" | "interesting"
            ) || (w.len() >= 4 && w.ends_with("ly"))
            {
                score += 30.0;
            } else if is_finite_aux_token(w)
                || matches!(
                    w,
                    "a" | "an" | "the" | "i" | "you" | "we" | "they" | "he" | "she" | "it"
                )
            {
                score -= 30.0;
            }
        }
        ClauseFrame::Euro(euro) => match euro {
            EuroLangFrame::FrenchModalOrFuture => {
                if w.len() >= 4
                    && (w.ends_with("er")
                        || w.ends_with("ir")
                        || w.ends_with("re")
                        || matches!(w, "être" | "etre" | "avoir"))
                {
                    score += 29.0;
                }
            }
            EuroLangFrame::FrenchPerfect => {
                if matches!(
                    w,
                    "fait" | "vu" | "pris" | "dit" | "eu" | "été" | "ete" | "pu" | "voulu" | "reçu"
                        | "recu" | "envoyé" | "trouvé" | "fini" | "compris"
                ) {
                    score += 29.0;
                }
            }
            EuroLangFrame::GermanModalOrFuture => {
                if w == "gerne" {
                    score += 27.0;
                } else if w.len() >= 4 && w.ends_with("en") {
                    score += 29.0;
                }
            }
            EuroLangFrame::SpanishProgressive => {
                if w.ends_with("ando") || w.ends_with("iendo") || w == "yendo" {
                    score += 30.0;
                }
            }
            EuroLangFrame::SpanishModalOrFuture => {
                if w.len() >= 2 && (w.ends_with("ar") || w.ends_with("er") || w.ends_with("ir")) {
                    score += 29.0;
                }
            }
        },
        ClauseFrame::None => {}
    }

    if next_word == "to"
        && matches!(
            w,
            "going"
                | "want"
                | "wanted"
                | "need"
                | "needed"
                | "able"
                | "used"
                | "trying"
                | "have"
                | "has"
                | "had"
                | "ready"
                | "happy"
                | "glad"
                | "according"
                | "due"
                | "thanks"
                | "look"
                | "looking"
                | "forward"
        )
    {
        score += 26.0;
    }

    score
}

/// 空白後（`(cur, cur)` 零長插入點）泛歐文聯想後繼詞查詢（含語詞搭配與因式分解句法框架延伸）
pub fn lookup_associative_after_space(
    prefix_before_cursor: &str,
    max_results: usize,
) -> Vec<String> {
    let words = extract_preceding_western_words(prefix_before_cursor, 6);
    if words.is_empty() || max_results == 0 {
        return Vec::new();
    }
    let mut out = lookup_continuations_for_words(&words, max_results);
    if out.len() < max_results {
        for cand in lookup_syntactic_frame_continuations(&words, max_results) {
            push_unique(&mut out, &cand, max_results);
        }
    }
    out
}

// ============================================================================
// 縮略語—完整詞組雙向定義與括號對偶投影引擎
// (Acronym–Expansion Definitional & Parenthetical Dual Projection Engine)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum SentenceItem {
    Word { raw: String, lower: String },
    OpenParen,
    Colon,
    Equals,
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorStrength {
    Strong,
    Parenthetical,
    WeakCopula,
}

#[derive(Debug, Clone)]
pub struct AcronymSlotContext {
    acro_raw: String,
    acro_lower: String,
    strength: ConnectorStrength,
    typed_tail: Vec<String>,
    has_forward: bool,
    expected_initial: Option<char>,
    is_non_final_modifier_slot: bool,
    has_reverse: bool,
    reverse_words: Vec<(String, String)>,
    reverse_starts_sentence: bool,
}

/// 標準縮略語與展開詞組對照表：(小寫縮略詞, 展開詞序列, 是否需至少兩個大寫字母以避開日常小寫同形詞)
const CANONICAL_ACRONYMS: &[(&str, &[&str], bool)] = &[
    // AI / Machine Learning / NLP / Data Science
    ("ai", &["artificial", "intelligence"], false),
    ("ml", &["machine", "learning"], false),
    ("dl", &["deep", "learning"], false),
    ("nlp", &["natural", "language", "processing"], false),
    ("llm", &["large", "language", "model"], false),
    ("vlm", &["vision", "language", "model"], false),
    ("rag", &["retrieval", "augmented", "generation"], true),
    ("rl", &["reinforcement", "learning"], false),
    ("rlhf", &["reinforcement", "learning", "from", "human", "feedback"], false),
    ("cv", &["computer", "vision"], false),
    ("cv", &["curriculum", "vitae"], false),
    ("cnn", &["convolutional", "neural", "network"], false),
    ("rnn", &["recurrent", "neural", "network"], false),
    ("lstm", &["long", "short", "term", "memory"], false),
    ("gan", &["generative", "adversarial", "network"], false),
    ("vae", &["variational", "autoencoder"], false),
    ("hmm", &["hidden", "markov", "model"], false),
    ("svm", &["support", "vector", "machine"], false),
    ("pca", &["principal", "component", "analysis"], false),
    ("svd", &["singular", "value", "decomposition"], false),
    ("sgd", &["stochastic", "gradient", "descent"], false),
    ("agi", &["artificial", "general", "intelligence"], false),
    ("asi", &["artificial", "superintelligence"], false),
    ("ocr", &["optical", "character", "recognition"], false),
    ("asr", &["automatic", "speech", "recognition"], false),
    ("tts", &["text", "to", "speech"], false),
    ("stt", &["speech", "to", "text"], false),
    ("ner", &["named", "entity", "recognition"], false),
    ("cer", &["character", "error", "rate"], false),
    ("wer", &["word", "error", "rate"], false),
    ("pos", &["part", "of", "speech"], true),
    ("pos", &["point", "of", "sale"], true),
    ("ir", &["information", "retrieval"], true),
    ("ir", &["infrared"], true),
    // Software Engineering / Systems / UI / OS
    ("api", &["application", "programming", "interface"], false),
    ("abi", &["application", "binary", "interface"], false),
    ("ffi", &["foreign", "function", "interface"], false),
    ("ipc", &["interprocess", "communication"], false),
    ("rpc", &["remote", "procedure", "call"], false),
    ("gui", &["graphical", "user", "interface"], false),
    ("cli", &["command", "line", "interface"], false),
    ("tui", &["text", "user", "interface"], false),
    ("ui", &["user", "interface"], false),
    ("ux", &["user", "experience"], false),
    ("sdk", &["software", "development", "kit"], false),
    ("ide", &["integrated", "development", "environment"], false),
    ("ime", &["input", "method", "editor"], false),
    ("tsf", &["text", "services", "framework"], false),
    ("it", &["information", "technology"], true),
    ("os", &["operating", "system"], true),
    ("os", &["open", "source"], true),
    ("db", &["database"], false),
    ("dbms", &["database", "management", "system"], false),
    ("sql", &["structured", "query", "language"], false),
    ("nosql", &["not", "only", "sql"], false),
    ("acid", &["atomicity", "consistency", "isolation", "durability"], true),
    ("orm", &["object", "relational", "mapping"], false),
    ("crud", &["create", "read", "update", "delete"], false),
    ("mvc", &["model", "view", "controller"], false),
    ("mvp", &["minimum", "viable", "product"], false),
    ("mvp", &["most", "valuable", "player"], false),
    ("spa", &["single", "page", "application"], true),
    ("ssr", &["server", "side", "rendering"], false),
    ("csr", &["client", "side", "rendering"], false),
    ("pwa", &["progressive", "web", "app"], false),
    ("dom", &["document", "object", "model"], true),
    ("pr", &["pull", "request"], true),
    ("pr", &["public", "relations"], true),
    ("ci", &["continuous", "integration"], false),
    ("cd", &["continuous", "delivery"], true),
    ("cd", &["compact", "disc"], true),
    ("qa", &["quality", "assurance"], false),
    ("qa", &["question", "answering"], false),
    ("qc", &["quality", "control"], false),
    ("poc", &["proof", "of", "concept"], false),
    ("rfc", &["request", "for", "comments"], false),
    ("adr", &["architecture", "decision", "record"], false),
    ("sop", &["standard", "operating", "procedure"], false),
    // Web / Networking / Security / Formats
    ("http", &["hypertext", "transfer", "protocol"], false),
    ("https", &["hypertext", "transfer", "protocol", "secure"], false),
    ("html", &["hypertext", "markup", "language"], false),
    ("css", &["cascading", "style", "sheets"], false),
    ("json", &["javascript", "object", "notation"], false),
    ("xml", &["extensible", "markup", "language"], false),
    ("yaml", &["yaml", "ain't", "markup", "language"], false),
    ("toml", &["tom's", "obvious", "minimal", "language"], false),
    ("csv", &["comma", "separated", "values"], false),
    ("tsv", &["tab", "separated", "values"], false),
    ("pdf", &["portable", "document", "format"], false),
    ("svg", &["scalable", "vector", "graphics"], false),
    ("png", &["portable", "network", "graphics"], false),
    ("jpeg", &["joint", "photographic", "experts", "group"], false),
    ("utf", &["unicode", "transformation", "format"], false),
    ("ascii", &["american", "standard", "code", "for", "information", "interchange"], false),
    ("bom", &["byte", "order", "mark"], false),
    ("eof", &["end", "of", "file"], false),
    ("eol", &["end", "of", "line"], false),
    ("oom", &["out", "of", "memory"], false),
    ("gc", &["garbage", "collection"], false),
    ("jit", &["just", "in", "time"], false),
    ("aot", &["ahead", "of", "time"], false),
    ("ast", &["abstract", "syntax", "tree"], false),
    ("ssa", &["static", "single", "assignment"], false),
    ("cfg", &["control", "flow", "graph"], false),
    ("dag", &["directed", "acyclic", "graph"], false),
    ("dfa", &["deterministic", "finite", "automaton"], false),
    ("nfa", &["nondeterministic", "finite", "automaton"], false),
    ("fsm", &["finite", "state", "machine"], false),
    ("lru", &["least", "recently", "used"], false),
    ("lfu", &["least", "frequently", "used"], false),
    ("fifo", &["first", "in", "first", "out"], false),
    ("lifo", &["last", "in", "first", "out"], false),
    ("rest", &["representational", "state", "transfer"], true),
    ("soap", &["simple", "object", "access", "protocol"], true),
    ("cors", &["cross", "origin", "resource", "sharing"], false),
    ("csrf", &["cross", "site", "request", "forgery"], false),
    ("xss", &["cross", "site", "scripting"], false),
    ("jwt", &["json", "web", "token"], false),
    ("sso", &["single", "sign", "on"], false),
    ("mfa", &["multi", "factor", "authentication"], false),
    ("rbac", &["role", "based", "access", "control"], false),
    ("lan", &["local", "area", "network"], false),
    ("wan", &["wide", "area", "network"], false),
    ("wlan", &["wireless", "local", "area", "network"], false),
    ("vpn", &["virtual", "private", "network"], false),
    ("dns", &["domain", "name", "system"], false),
    ("dhcp", &["dynamic", "host", "configuration", "protocol"], false),
    ("ip", &["internet", "protocol"], true),
    ("tcp", &["transmission", "control", "protocol"], false),
    ("udp", &["user", "datagram", "protocol"], false),
    ("ssh", &["secure", "shell"], false),
    ("ssl", &["secure", "sockets", "layer"], false),
    ("tls", &["transport", "layer", "security"], false),
    ("ftp", &["file", "transfer", "protocol"], false),
    ("smtp", &["simple", "mail", "transfer", "protocol"], false),
    ("imap", &["internet", "message", "access", "protocol"], false),
    ("bgp", &["border", "gateway", "protocol"], false),
    ("cdn", &["content", "delivery", "network"], false),
    ("nat", &["network", "address", "translation"], true),
    ("url", &["uniform", "resource", "locator"], false),
    ("uri", &["uniform", "resource", "identifier"], false),
    // Hardware / Architecture
    ("cpu", &["central", "processing", "unit"], false),
    ("gpu", &["graphics", "processing", "unit"], false),
    ("tpu", &["tensor", "processing", "unit"], false),
    ("npu", &["neural", "processing", "unit"], false),
    ("fpu", &["floating", "point", "unit"], false),
    ("alu", &["arithmetic", "logic", "unit"], false),
    ("mmu", &["memory", "management", "unit"], false),
    ("tlb", &["translation", "lookaside", "buffer"], false),
    ("ram", &["random", "access", "memory"], true),
    ("rom", &["read", "only", "memory"], true),
    ("dram", &["dynamic", "random", "access", "memory"], false),
    ("sram", &["static", "random", "access", "memory"], false),
    ("vram", &["video", "random", "access", "memory"], false),
    ("nvme", &["non", "volatile", "memory", "express"], false),
    ("ssd", &["solid", "state", "drive"], false),
    ("hdd", &["hard", "disk", "drive"], false),
    ("raid", &["redundant", "array", "of", "independent", "disks"], true),
    ("usb", &["universal", "serial", "bus"], false),
    ("hdmi", &["high", "definition", "multimedia", "interface"], false),
    ("pcie", &["peripheral", "component", "interconnect", "express"], false),
    ("bios", &["basic", "input", "output", "system"], false),
    ("uefi", &["unified", "extensible", "firmware", "interface"], false),
    ("rtos", &["real", "time", "operating", "system"], false),
    ("simd", &["single", "instruction", "multiple", "data"], false),
    ("numa", &["non", "uniform", "memory", "access"], false),
    ("iot", &["internet", "of", "things"], false),
    ("vr", &["virtual", "reality"], false),
    ("ar", &["augmented", "reality"], true),
    ("xr", &["extended", "reality"], false),
    // Business / Organizations / Everyday / Science & Medicine
    ("kpi", &["key", "performance", "indicator"], false),
    ("roi", &["return", "on", "investment"], false),
    ("okr", &["objectives", "and", "key", "results"], false),
    ("sla", &["service", "level", "agreement"], false),
    ("saas", &["software", "as", "a", "service"], false),
    ("paas", &["platform", "as", "a", "service"], false),
    ("iaas", &["infrastructure", "as", "a", "service"], false),
    ("erp", &["enterprise", "resource", "planning"], false),
    ("crm", &["customer", "relationship", "management"], false),
    ("seo", &["search", "engine", "optimization"], false),
    ("cta", &["call", "to", "action"], false),
    ("eta", &["estimated", "time", "of", "arrival"], false),
    ("fyi", &["for", "your", "information"], false),
    ("btw", &["by", "the", "way"], false),
    ("asap", &["as", "soon", "as", "possible"], false),
    ("faq", &["frequently", "asked", "questions"], false),
    ("diy", &["do", "it", "yourself"], false),
    ("tba", &["to", "be", "announced"], false),
    ("tbd", &["to", "be", "determined"], false),
    ("aka", &["also", "known", "as"], false),
    ("ceo", &["chief", "executive", "officer"], false),
    ("cto", &["chief", "technology", "officer"], false),
    ("cfo", &["chief", "financial", "officer"], false),
    ("coo", &["chief", "operating", "officer"], false),
    ("cio", &["chief", "information", "officer"], false),
    ("pm", &["project", "management"], true),
    ("pm", &["product", "manager"], true),
    ("hr", &["human", "resources"], true),
    ("gdp", &["gross", "domestic", "product"], false),
    ("cpi", &["consumer", "price", "index"], false),
    ("who", &["world", "health", "organization"], true),
    ("un", &["united", "nations"], true),
    ("us", &["united", "states"], true),
    ("usa", &["united", "states", "of", "america"], false),
    ("uk", &["united", "kingdom"], false),
    ("eu", &["european", "union"], false),
    ("uae", &["united", "arab", "emirates"], false),
    ("nato", &["north", "atlantic", "treaty", "organization"], false),
    ("nasa", &["national", "aeronautics", "and", "space", "administration"], false),
    ("imf", &["international", "monetary", "fund"], false),
    ("wto", &["world", "trade", "organization"], false),
    ("ieee", &["institute", "of", "electrical", "and", "electronics", "engineers"], false),
    ("acm", &["association", "for", "computing", "machinery"], false),
    ("iso", &["international", "organization", "for", "standardization"], false),
    ("ansi", &["american", "national", "standards", "institute"], false),
    ("stem", &["science", "technology", "engineering", "mathematics"], true),
    ("dna", &["deoxyribonucleic", "acid"], false),
    ("rna", &["ribonucleic", "acid"], false),
    ("mrna", &["messenger", "ribonucleic", "acid"], false),
    ("atp", &["adenosine", "triphosphate"], false),
    ("pcr", &["polymerase", "chain", "reaction"], false),
    ("mri", &["magnetic", "resonance", "imaging"], false),
    ("ct", &["computed", "tomography"], true),
    ("ecg", &["electrocardiogram"], false),
    ("eeg", &["electroencephalogram"], false),
    ("icu", &["intensive", "care", "unit"], false),
    ("bmi", &["body", "mass", "index"], false),
    ("adhd", &["attention", "deficit", "hyperactivity", "disorder"], false),
    ("ptsd", &["post", "traumatic", "stress", "disorder"], false),
    ("ocd", &["obsessive", "compulsive", "disorder"], false),
    ("iq", &["intelligence", "quotient"], false),
    ("eq", &["emotional", "quotient"], false),
    ("phd", &["doctor", "of", "philosophy"], false),
    ("mba", &["master", "of", "business", "administration"], false),
    ("gpa", &["grade", "point", "average"], false),
    ("am", &["amplitude", "modulation"], true),
    ("id", &["identification"], true),
    ("gmt", &["greenwich", "mean", "time"], false),
    ("utc", &["coordinated", "universal", "time"], false),
    ("est", &["eastern", "standard", "time"], true),
    ("mt", &["machine", "translation"], true),
];

#[inline]
fn has_min_two_uppercase(s: &str) -> bool {
    s.chars().filter(|c| c.is_ascii_uppercase()).count() >= 2
}

#[inline]
fn is_all_ascii_uppercase(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase())
}

#[inline]
fn is_title_or_upper_word(s: &str) -> bool {
    s.chars().next().map_or(false, |c| c.is_ascii_uppercase())
}

fn extract_sentence_items(prefix: &str, max_items: usize) -> (Vec<SentenceItem>, bool) {
    if max_items == 0 || prefix.is_empty() {
        return (Vec::new(), true);
    }
    let chars: Vec<char> = prefix.chars().collect();
    let mut i = chars.len();
    let mut rev: Vec<SentenceItem> = Vec::with_capacity(max_items);
    let mut reached_sentence_start = false;

    while rev.len() < max_items && i > 0 {
        while i > 0 && is_horizontal_space(chars[i - 1]) {
            i -= 1;
        }
        if i == 0 {
            reached_sentence_start = true;
            break;
        }
        let ch = chars[i - 1];
        match ch {
            '(' | '（' | '[' => {
                rev.push(SentenceItem::OpenParen);
                i -= 1;
            }
            ':' | '：' => {
                rev.push(SentenceItem::Colon);
                i -= 1;
            }
            '=' => {
                rev.push(SentenceItem::Equals);
                i -= 1;
            }
            ',' | '，' => {
                rev.push(SentenceItem::Comma);
                i -= 1;
            }
            '.' => {
                // 解析如 "A.I."、"U.S." 等無空白相間首字母縮略（需至少兩組 [A-Za-z].）
                let mut j = i;
                let mut letters: Vec<char> = Vec::new();
                while j >= 2 && chars[j - 1] == '.' && chars[j - 2].is_ascii_alphabetic() {
                    let bound_ok = j == 2 || !is_latin_word_char(chars[j - 3]);
                    if !bound_ok {
                        break;
                    }
                    letters.push(chars[j - 2]);
                    j -= 2;
                }
                if letters.len() >= 2 {
                    letters.reverse();
                    let raw: String = letters.into_iter().collect();
                    let lower = raw.to_lowercase();
                    rev.push(SentenceItem::Word { raw, lower });
                    i = j;
                } else {
                    reached_sentence_start = true;
                    break;
                }
            }
            '?' | '!' | ';' | '\n' | '\r' | '。' | '？' | '！' | '；' => {
                reached_sentence_start = true;
                break;
            }
            ')' | '）' | ']' => {
                break;
            }
            c if is_latin_word_char(c) => {
                let end = i;
                while i > 0 {
                    let wch = chars[i - 1];
                    if is_latin_word_char(wch) {
                        i -= 1;
                    } else if is_internal_word_joiner(wch)
                        && i >= 2
                        && is_latin_word_char(chars[i - 2])
                    {
                        i -= 1;
                    } else {
                        break;
                    }
                }
                let raw: String = chars[i..end]
                    .iter()
                    .map(|&c| if c == '\u{2019}' { '\'' } else { c })
                    .collect();
                let lower = raw.to_lowercase();
                rev.push(SentenceItem::Word { raw, lower });
            }
            _ => break,
        }
    }

    if i == 0 {
        reached_sentence_start = true;
    }
    rev.reverse();
    (rev, reached_sentence_start)
}

#[inline]
fn word_lower_at(items: &[SentenceItem], idx: usize) -> Option<&str> {
    match items.get(idx) {
        Some(SentenceItem::Word { lower, .. }) => Some(lower.as_str()),
        _ => None,
    }
}

fn match_forward_connector(
    tail: &[SentenceItem],
    acro_raw: &str,
) -> Option<(usize, ConnectorStrength)> {
    // 4-word Strong: (is|are|was|were, a|an|the, abbreviation|acronym|initialism|shorthand, for|of)
    if tail.len() >= 4 {
        if let (Some(w0), Some(w1), Some(w2), Some(w3)) = (
            word_lower_at(tail, 0),
            word_lower_at(tail, 1),
            word_lower_at(tail, 2),
            word_lower_at(tail, 3),
        ) {
            if matches!(w0, "is" | "are" | "was" | "were")
                && matches!(w1, "a" | "an" | "the")
                && matches!(w2, "abbreviation" | "acronym" | "initialism" | "shorthand")
                && matches!(w3, "for" | "of")
            {
                return Some((4, ConnectorStrength::Strong));
            }
        }
    }

    // 3-item Strong
    if tail.len() >= 3 {
        if matches!(tail[0], SentenceItem::Comma) {
            if matches!(
                (word_lower_at(tail, 1), word_lower_at(tail, 2)),
                (Some("short"), Some("for"))
            ) {
                return Some((3, ConnectorStrength::Strong));
            }
        } else if let (Some(w0), Some(w1), Some(w2)) = (
            word_lower_at(tail, 0),
            word_lower_at(tail, 1),
            word_lower_at(tail, 2),
        ) {
            if matches!(w0, "is" | "are" | "was" | "were")
                && ((w1 == "short" && w2 == "for")
                    || (matches!(w1, "abbreviation" | "acronym" | "initialism" | "shorthand")
                        && matches!(w2, "for" | "of")))
            {
                return Some((3, ConnectorStrength::Strong));
            }
        }
    }

    // 2-item Strong
    if tail.len() >= 2 {
        if matches!(tail[0], SentenceItem::Comma) {
            if has_min_two_uppercase(acro_raw)
                && matches!(word_lower_at(tail, 1), Some("or" | "namely"))
            {
                return Some((2, ConnectorStrength::Strong));
            }
        } else if let (Some(w0), Some(w1)) = (word_lower_at(tail, 0), word_lower_at(tail, 1)) {
            if (matches!(w0, "stands" | "stand" | "stood") && w1 == "for")
                || (matches!(w0, "refers" | "refer" | "referred") && w1 == "to")
                || (w0 == "short" && w1 == "for")
                || (matches!(w0, "abbreviation" | "acronym" | "initialism" | "shorthand")
                    && matches!(w1, "for" | "of"))
            {
                return Some((2, ConnectorStrength::Strong));
            }
        }
    }

    // 1-item Strong / Parenthetical / WeakCopula
    if let Some(first) = tail.first() {
        match first {
            SentenceItem::Word { lower, .. } => {
                if matches!(
                    lower.as_str(),
                    "means"
                        | "mean"
                        | "meant"
                        | "denotes"
                        | "denote"
                        | "represents"
                        | "represent"
                        | "abbreviates"
                        | "signifies"
                        | "equals"
                ) {
                    return Some((1, ConnectorStrength::Strong));
                }
                if is_all_ascii_uppercase(acro_raw)
                    && (2..=6).contains(&acro_raw.chars().count())
                    && matches!(lower.as_str(), "is" | "was")
                {
                    return Some((1, ConnectorStrength::WeakCopula));
                }
            }
            SentenceItem::OpenParen | SentenceItem::Colon | SentenceItem::Equals => {
                if has_min_two_uppercase(acro_raw) {
                    return Some((1, ConnectorStrength::Parenthetical));
                }
            }
            SentenceItem::Comma => {}
        }
    }

    None
}

fn compute_open_expected_initial(
    acro_lower: &str,
    typed_tail: &[String],
) -> (Option<char>, bool) {
    let acro_chars: Vec<char> = acro_lower.chars().collect();
    if !(2..=6).contains(&acro_chars.len()) {
        return (None, false);
    }

    let mut c_idx = 0usize;
    let mut skipped_leading_art = false;
    for (idx, tw) in typed_tail.iter().enumerate() {
        if idx == 0
            && !skipped_leading_art
            && matches!(tw.as_str(), "a" | "an" | "the")
            && !acro_chars[0].eq_ignore_ascii_case(&tw.chars().next().unwrap_or('\0'))
        {
            skipped_leading_art = true;
            continue;
        }
        if c_idx < acro_chars.len() && tw.starts_with(acro_chars[c_idx]) {
            c_idx += 1;
        } else if matches!(
            tw.as_str(),
            "of" | "for" | "the" | "in" | "on" | "to" | "and" | "from" | "with" | "by" | "a" | "an"
        ) {
            continue;
        } else {
            return (None, false);
        }
    }

    if c_idx < acro_chars.len() {
        (Some(acro_chars[c_idx]), c_idx + 1 < acro_chars.len())
    } else {
        (None, false)
    }
}

#[inline]
fn is_potential_acronym_token(raw: &str, lower: &str) -> bool {
    has_min_two_uppercase(raw)
        || CANONICAL_ACRONYMS
            .iter()
            .any(|&(acro, _, require_upper)| !require_upper && acro == lower)
}

/// 解析當前組字游標前之「縮略語—定義／括號雙向投影」語境。
pub fn analyze_acronym_context(prefix_before_word: &str) -> Option<AcronymSlotContext> {
    let (items, reached_sentence_start) = extract_sentence_items(prefix_before_word, 16);
    if items.is_empty() {
        return None;
    }

    // 1. 偵測正向縮略語展開框架（由右至左尋找最近的合法縮略語與定義連接片語）
    let mut forward: Option<(String, String, ConnectorStrength, Vec<String>, Option<char>, bool)> =
        None;
    for acro_idx in (0..items.len()).rev() {
        if let SentenceItem::Word { raw, lower } = &items[acro_idx] {
            let char_len = raw.chars().count();
            if !(2..=8).contains(&char_len)
                || !raw.chars().all(|c| c.is_ascii_alphabetic())
                || !is_potential_acronym_token(raw, lower)
            {
                continue;
            }
            if let Some((conn_len, strength)) = match_forward_connector(&items[acro_idx + 1..], raw)
            {
                let tail_start = acro_idx + 1 + conn_len;
                let tail_slice = &items[tail_start..];
                if tail_slice.len() <= 6
                    && tail_slice
                        .iter()
                        .all(|it| matches!(it, SentenceItem::Word { .. }))
                {
                    let typed_tail: Vec<String> = tail_slice
                        .iter()
                        .filter_map(|it| match it {
                            SentenceItem::Word { lower, .. } => Some(lower.clone()),
                            _ => None,
                        })
                        .collect();
                    let (expected_initial, is_non_final_modifier_slot) = if matches!(
                        strength,
                        ConnectorStrength::Strong | ConnectorStrength::Parenthetical
                    ) && is_all_ascii_uppercase(raw)
                        && (2..=6).contains(&char_len)
                    {
                        compute_open_expected_initial(lower, &typed_tail)
                    } else {
                        (None, false)
                    };
                    forward = Some((
                        raw.clone(),
                        lower.clone(),
                        strength,
                        typed_tail,
                        expected_initial,
                        is_non_final_modifier_slot,
                    ));
                    break;
                }
            }
        }
    }

    // 2. 偵測反向全稱收斂至縮略語框架（如 "artificial intelligence (" / "Responsive User Interface (" / ", abbreviated as "）
    let rev_end_opt: Option<usize> = if matches!(items.last(), Some(SentenceItem::OpenParen)) {
        Some(items.len() - 1)
    } else if items.len() >= 2
        && matches!(
            (
                word_lower_at(&items, items.len() - 2),
                word_lower_at(&items, items.len() - 1)
            ),
            (Some("abbreviated" | "known"), Some("as"))
        )
    {
        let idx = items.len() - 2;
        if idx > 0 && matches!(items[idx - 1], SentenceItem::Comma) {
            Some(idx - 1)
        } else {
            Some(idx)
        }
    } else {
        None
    };

    let mut has_reverse = false;
    let mut reverse_words: Vec<(String, String)> = Vec::new();
    let mut reverse_starts_sentence = false;
    if let Some(rev_end) = rev_end_opt {
        let mut rev_start = rev_end;
        while rev_start > 0 && (rev_end - rev_start) < 6 {
            if matches!(items[rev_start - 1], SentenceItem::Word { .. }) {
                rev_start -= 1;
            } else {
                break;
            }
        }
        if rev_start < rev_end {
            has_reverse = true;
            reverse_starts_sentence = reached_sentence_start && rev_start == 0;
            reverse_words = items[rev_start..rev_end]
                .iter()
                .filter_map(|it| match it {
                    SentenceItem::Word { raw, lower } => Some((raw.clone(), lower.clone())),
                    _ => None,
                })
                .collect();
        }
    }

    if forward.is_none() && !has_reverse {
        return None;
    }

    let (
        acro_raw,
        acro_lower,
        strength,
        typed_tail,
        has_forward,
        expected_initial,
        is_non_final_modifier_slot,
    ) = match forward {
        Some((r, l, st, tt, ei, nf)) => (r, l, st, tt, true, ei, nf),
        None => (
            String::new(),
            String::new(),
            ConnectorStrength::Strong,
            Vec::new(),
            false,
            None,
            false,
        ),
    };

    Some(AcronymSlotContext {
        acro_raw,
        acro_lower,
        strength,
        typed_tail,
        has_forward,
        expected_initial,
        is_non_final_modifier_slot,
        has_reverse,
        reverse_words,
        reverse_starts_sentence,
    })
}

fn format_acronym_word_casing(word: &str, raw_token: &str) -> String {
    let trimmed = raw_token.trim();
    if trimmed.is_empty() {
        return word.to_string();
    }
    let alpha_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count >= 2 && trimmed.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
        return word.to_uppercase();
    }
    if trimmed.chars().next().map_or(false, |c| c.is_uppercase()) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            let mut out = String::new();
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
            return out;
        }
    }
    word.to_string()
}

fn format_acronym_phrase_casing(words: &[&str], raw_token: &str) -> String {
    let trimmed = raw_token.trim();
    let alpha_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    let all_upper = !trimmed.is_empty()
        && alpha_count >= 2
        && trimmed.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
    let title_mode = !trimmed.is_empty() && trimmed.chars().next().map_or(false, |c| c.is_uppercase());

    if all_upper {
        return words.join(" ").to_uppercase();
    }
    if title_mode {
        let titled: Vec<String> = words
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                if i > 0
                    && matches!(
                        w,
                        "of" | "for" | "the" | "in" | "on" | "to" | "and" | "from" | "as" | "a" | "an"
                    )
                {
                    w.to_string()
                } else {
                    format_acronym_word_casing(w, "Title")
                }
            })
            .collect();
        return titled.join(" ");
    }
    words.join(" ")
}

#[inline]
fn is_sentence_initial_closed_class(w_low: &str) -> bool {
    matches!(
        w_low,
        "the" | "a" | "an" | "this" | "that" | "these" | "those" | "in" | "on" | "at" | "for"
            | "with" | "from" | "by" | "when" | "where" | "what" | "why" | "how" | "if" | "while"
            | "since" | "after" | "before" | "we" | "you" | "they" | "he" | "she" | "it"
    )
}

/// 依已解析之縮略語投影語境與當前輸入詞頭（可為空字串）產生候選條目 `(surface, bonus, is_multiword)`。
pub fn lookup_acronym_candidates_with_context(
    ctx: &AcronymSlotContext,
    raw_token: &str,
) -> Vec<(String, f32, bool)> {
    let trimmed = raw_token.trim();
    let t_lower = trimmed.to_lowercase();
    let mut out: Vec<(String, f32, bool)> = Vec::new();
    let mut push_entry = |surface: String, bonus: f32, is_multi: bool| {
        if !surface.is_empty()
            && !out
                .iter()
                .any(|(s, _, m)| *m == is_multi && s.eq_ignore_ascii_case(&surface))
        {
            out.push((surface, bonus, is_multi));
        }
    };

    // 1. 正向展開：縮略語 -> 完整詞組／分槽單字
    if ctx.has_forward {
        for &(acro, expansion, require_upper) in CANONICAL_ACRONYMS {
            if ctx.acro_lower != acro {
                continue;
            }
            if (require_upper
                || matches!(
                    ctx.strength,
                    ConnectorStrength::Parenthetical | ConnectorStrength::WeakCopula
                ))
                && !has_min_two_uppercase(&ctx.acro_raw)
            {
                continue;
            }

            let effective_tail: &[String] = if !ctx.typed_tail.is_empty()
                && matches!(ctx.typed_tail[0].as_str(), "a" | "an" | "the")
                && expansion.first().copied() != Some(ctx.typed_tail[0].as_str())
            {
                &ctx.typed_tail[1..]
            } else {
                &ctx.typed_tail[..]
            };

            let m = effective_tail.len();
            if m >= expansion.len() {
                continue;
            }
            if !effective_tail
                .iter()
                .zip(expansion.iter())
                .all(|(tw, &ew)| tw == ew)
            {
                continue;
            }

            if ctx.strength == ConnectorStrength::WeakCopula && m == 0 && t_lower.chars().count() < 3
            {
                continue;
            }

            let base_bonus = match ctx.strength {
                ConnectorStrength::Strong | ConnectorStrength::Parenthetical => 35.0,
                ConnectorStrength::WeakCopula => 24.0,
            };

            let slot_word = expansion[m];
            if slot_word.starts_with(&t_lower) {
                push_entry(
                    format_acronym_word_casing(slot_word, trimmed),
                    base_bonus,
                    false,
                );
            }
            if m + 1 < expansion.len() {
                let rem_slice = &expansion[m..];
                let rem_low = rem_slice.join(" ");
                if rem_low.starts_with(&t_lower) {
                    push_entry(
                        format_acronym_phrase_casing(rem_slice, trimmed),
                        base_bonus - 3.0,
                        true,
                    );
                }
            }
        }
    }

    // 2. 反向收斂：完整詞組 ("artificial intelligence (" / "Responsive User Interface (") -> 縮略語 ("AI" / "RUI")
    if ctx.has_reverse && !ctx.reverse_words.is_empty() {
        let rlen = ctx.reverse_words.len();
        let last_raw = &ctx.reverse_words[rlen - 1].0;

        for &(acro, expansion, _) in CANONICAL_ACRONYMS {
            let elen = expansion.len();
            if rlen < elen || elen == 0 {
                continue;
            }
            let suffix = &ctx.reverse_words[rlen - elen..];
            let acro_upper = acro.to_ascii_uppercase();
            if last_raw.eq_ignore_ascii_case(&acro_upper) {
                continue;
            }

            let exact_match = suffix
                .iter()
                .zip(expansion.iter())
                .all(|((_, lw), &ew)| lw == ew);
            let mut plural_match = false;
            if !exact_match && elen >= 2 {
                let prefix_ok = suffix[..elen - 1]
                    .iter()
                    .zip(expansion[..elen - 1].iter())
                    .all(|((_, lw), &ew)| lw == ew);
                if prefix_ok {
                    let tail_lw = &suffix[elen - 1].1;
                    let exp_last = expansion[elen - 1];
                    if tail_lw.len() >= 4
                        && tail_lw.ends_with('s')
                        && !exp_last.ends_with('s')
                        && &tail_lw[..tail_lw.len() - 1] == exp_last
                    {
                        plural_match = true;
                    }
                }
            }

            if plural_match {
                let acro_plural = format!("{}s", acro_upper);
                if acro_plural.to_lowercase().starts_with(&t_lower) {
                    push_entry(acro_plural, 35.0, false);
                }
                if acro_upper.to_lowercase().starts_with(&t_lower) {
                    push_entry(acro_upper, 34.0, false);
                }
            } else if exact_match && acro_upper.to_lowercase().starts_with(&t_lower) {
                push_entry(acro_upper, 35.0, false);
            }
        }

        // 動態開放詞彙 TitleCase 首字母合成（例如 "Responsive User Interface (" -> "RUI"）
        let mut cap_start = rlen;
        while cap_start > 0 {
            let (ref raw, ref low) = ctx.reverse_words[cap_start - 1];
            if is_title_or_upper_word(raw) {
                cap_start -= 1;
            } else if cap_start >= 2
                && cap_start < rlen
                && matches!(low.as_str(), "of" | "for" | "and" | "in" | "to")
                && is_title_or_upper_word(&ctx.reverse_words[cap_start - 2].0)
            {
                cap_start -= 1;
            } else {
                break;
            }
        }

        let mut run = &ctx.reverse_words[cap_start..rlen];
        if !run.is_empty()
            && cap_start == 0
            && ctx.reverse_starts_sentence
            && is_sentence_initial_closed_class(&run[0].1)
        {
            run = &run[1..];
        }

        let cap_initials: Vec<char> = run
            .iter()
            .filter(|(raw, _)| is_title_or_upper_word(raw))
            .filter_map(|(raw, _)| {
                let ch = raw.chars().next()?;
                if ch.is_ascii_alphabetic() {
                    Some(ch.to_ascii_uppercase())
                } else {
                    None
                }
            })
            .collect();
        if (2..=5).contains(&cap_initials.len())
            && !run.iter().all(|(raw, _)| has_min_two_uppercase(raw))
        {
            let synth_acro: String = cap_initials.into_iter().collect();
            if synth_acro.to_lowercase().starts_with(&t_lower) {
                push_entry(synth_acro, 32.0, false);
            }
        }
    }

    out
}

/// 開放詞彙首字母對齊與形態句法槽加分（針對未登錄縮略語如 "AL means art" 提升首字母匹配與修飾詞／中心名詞詞性適配）
pub fn score_candidate_with_acronym_frame(
    ctx: &AcronymSlotContext,
    candidate_lower: &str,
) -> f32 {
    let Some(exp_ch) = ctx.expected_initial else {
        return 0.0;
    };
    if !ctx.has_forward
        || !candidate_lower.starts_with(exp_ch)
        || candidate_lower.chars().count() < 3
        || CLOSED_CLASS_STOPWORDS.contains(&candidate_lower)
        || matches!(
            candidate_lower,
            "the" | "and" | "for" | "are" | "was" | "were" | "you" | "his" | "her" | "its" | "our"
        )
    {
        return 0.0;
    }

    let mut bonus = 22.0;
    if ctx.is_non_final_modifier_slot {
        if candidate_lower.ends_with("al")
            || candidate_lower.ends_with("ic")
            || candidate_lower.ends_with("ive")
            || candidate_lower.ends_with("ous")
            || candidate_lower.ends_with("ular")
            || candidate_lower.ends_with("ary")
            || candidate_lower.ends_with("ory")
            || candidate_lower.ends_with("able")
            || candidate_lower.ends_with("ible")
        {
            bonus += 12.0;
        } else if candidate_lower.len() >= 4
            && candidate_lower.ends_with('s')
            && !(candidate_lower.ends_with("ss")
                || candidate_lower.ends_with("is")
                || candidate_lower.ends_with("us")
                || candidate_lower.ends_with("os")
                || candidate_lower.ends_with("ics")
                || candidate_lower.ends_with("ous"))
        {
            bonus -= 14.0;
        }
    } else if candidate_lower.ends_with("tion")
        || candidate_lower.ends_with("sion")
        || candidate_lower.ends_with("ment")
        || candidate_lower.ends_with("ness")
        || candidate_lower.ends_with("ity")
        || candidate_lower.ends_with("ance")
        || candidate_lower.ends_with("ence")
        || candidate_lower.ends_with("er")
        || candidate_lower.ends_with("or")
        || candidate_lower.ends_with("ist")
        || candidate_lower.ends_with("ism")
        || candidate_lower.ends_with("ing")
        || candidate_lower.ends_with("age")
        || candidate_lower.ends_with("ure")
        || candidate_lower.ends_with("tem")
        || candidate_lower.ends_with("rk")
        || candidate_lower.ends_with("ce")
    {
        bonus += 8.0;
    }

    bonus
}

/// 空白或左括號後（零長插入點）縮略語定義與括號後繼候選查詢：多詞完整片語優先於首詞
pub fn lookup_acronym_after_space(prefix_before_cursor: &str, max_results: usize) -> Vec<String> {
    if max_results == 0 || prefix_before_cursor.is_empty() {
        return Vec::new();
    }
    let Some(ctx) = analyze_acronym_context(prefix_before_cursor) else {
        return Vec::new();
    };
    let mut cands = lookup_acronym_candidates_with_context(&ctx, "");
    if cands.is_empty() {
        return Vec::new();
    }
    cands.sort_by(|a, b| {
        b.2.cmp(&a.2).then_with(|| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let mut out = Vec::new();
    for (surface, _, _) in cands {
        if out.len() >= max_results {
            break;
        }
        if !out.iter().any(|s: &String| s.eq_ignore_ascii_case(&surface)) {
            out.push(surface);
        }
    }
    out
}
