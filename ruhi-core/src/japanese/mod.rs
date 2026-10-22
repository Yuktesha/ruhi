/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/japanese/mod.rs - 日文羅馬字（Romaji）平假名／片假名自動機與漢字連文節智慧轉換引擎

use crate::engine::EngineResponse;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct JapaneseEntry {
    pub reading_hira: String,
    pub surface: String,
    pub weight: u32,
}

#[derive(Debug, Clone)]
pub struct JapaneseEngine {
    /// 當前輸入之原始羅馬字緩衝區
    pub raw_romaji: String,
    /// 已轉換之平假名緩衝區（含未決尾碼如單獨 'n' 或子音前綴）
    pub hira_buffer: String,
    /// 尚未構成完整假名之尾端羅馬字（如 "k", "sh", "ch", "ts", "n"）
    pub pending_romaji: String,
    /// 平假名讀音 -> 候選詞列表（依權重降序）
    pub lexicon: HashMap<String, Vec<JapaneseEntry>>,
    /// 學習者音韻骨架索引（無長音、無促音、清濁音歸一 -> 候選詞列表，依權重降序）
    pub skeleton_index: HashMap<String, Vec<JapaneseEntry>>,
    /// 當前候選詞清單
    pub candidates: Vec<String>,
    /// 當前選中候選索引
    pub candidate_idx: usize,
    /// 強制英文直通模式（大寫開頭或識別字）
    pub is_english_forced: bool,
}

impl Default for JapaneseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JapaneseEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            raw_romaji: String::new(),
            hira_buffer: String::new(),
            pending_romaji: String::new(),
            lexicon: HashMap::new(),
            skeleton_index: HashMap::new(),
            candidates: Vec::new(),
            candidate_idx: 0,
            is_english_forced: false,
        };
        engine.load_builtin_lexicon();
        engine
    }

    pub fn reset(&mut self) {
        self.raw_romaji.clear();
        self.hira_buffer.clear();
        self.pending_romaji.clear();
        self.candidates.clear();
        self.candidate_idx = 0;
        self.is_english_forced = false;
    }

    pub fn has_buffer(&self) -> bool {
        !self.raw_romaji.is_empty()
    }

    pub fn add_lexicon_entry(&mut self, reading_hira: &str, surface: &str, weight: u32) {
        let list = self.lexicon.entry(reading_hira.to_string()).or_default();
        if let Some(existing) = list.iter_mut().find(|e| e.surface == surface) {
            if weight > existing.weight {
                existing.weight = weight;
            }
        } else {
            list.push(JapaneseEntry {
                reading_hira: reading_hira.to_string(),
                surface: surface.to_string(),
                weight,
            });
        }
        list.sort_by(|a, b| b.weight.cmp(&a.weight));

        // 同步建立學習者音韻骨架索引（供 2 假名以上詞彙進行即時清濁音、長音、促音容錯轉換）
        if reading_hira.chars().count() >= 2 {
            let skel = Self::learner_phonetic_skeleton(reading_hira);
            if skel.chars().count() >= 2 {
                let s_list = self.skeleton_index.entry(skel).or_default();
                if let Some(existing_s) = s_list.iter_mut().find(|e| e.surface == surface) {
                    if weight > existing_s.weight {
                        existing_s.weight = weight;
                    }
                } else {
                    s_list.push(JapaneseEntry {
                        reading_hira: reading_hira.to_string(),
                        surface: surface.to_string(),
                        weight,
                    });
                }
                s_list.sort_by(|a, b| b.weight.cmp(&a.weight));
            }
        }
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let surface = parts[0].trim();
                let reading = parts[1].trim();
                let weight: u32 = parts.get(2).and_then(|w| w.trim().parse().ok()).unwrap_or(1000);
                self.add_lexicon_entry(reading, surface, weight);
                count += 1;
            }
        }
        Ok(count)
    }

    /// 將平假名字串轉為全形片假名（ひらがな U+3041..U+3096 ➔ カタカナ U+30A1..U+30F6）
    pub fn hira_to_kata(hira: &str) -> String {
        hira.chars()
            .map(|c| {
                let code = c as u32;
                if (0x3041..=0x3096).contains(&code) {
                    std::char::from_u32(code + 0x60).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect()
    }

    /// 將原始羅馬字完整轉換為 (已確定平假名, 未決尾碼)
    pub fn convert_romaji_stream(raw: &str) -> (String, String) {
        let lower = raw.to_ascii_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        let mut hira = String::new();
        let mut i = 0;
        let n = chars.len();

        let is_vowel = |c: char| matches!(c, 'a' | 'i' | 'u' | 'e' | 'o');

        while i < n {
            let rem = n - i;
            let ch = chars[i];

            // 1. 長音符號 '-'
            if ch == '-' {
                hira.push('ー');
                i += 1;
                continue;
            }

            // 2. 明確撥音 "nn" 或 "n'"
            if ch == 'n' && rem >= 2 && (chars[i + 1] == 'n' || chars[i + 1] == '\'') {
                hira.push('ん');
                i += 2;
                continue;
            }

            // 3. 單 'n' 後接非母音、非 'y'、非 'n' 之子音 -> 自動結算為撥音 'ん'
            if ch == 'n' && rem >= 2 {
                let next = chars[i + 1];
                if !is_vowel(next) && next != 'y' && next != 'n' && next != '\'' {
                    hira.push('ん');
                    i += 1;
                    continue;
                }
            }

            // 3b. 赫本式（Hepburn）雙唇音前撥音 'm'（接於 b, p, m 之前，如 shimbun ➔ しんぶん、tempura ➔ てんぷら、gambatte ➔ がんばって、sempai ➔ せんぱい、kampai ➔ かんぱい）
            if ch == 'm' && rem >= 2 && matches!(chars[i + 1], 'b' | 'p' | 'm') {
                hira.push('ん');
                i += 1;
                continue;
            }

            // 4. 促音（雙子音重疊，如 kk, ss, tt, pp, cc, gg, zz, dd, bb, tch）
            if rem >= 3 && chars[i] == 't' && chars[i + 1] == 'c' && chars[i + 2] == 'h' {
                hira.push('っ');
                i += 1;
                continue;
            }
            if rem >= 2
                && chars[i] == chars[i + 1]
                && chars[i].is_ascii_alphabetic()
                && !is_vowel(chars[i])
                && chars[i] != 'n'
                && chars[i] != 'm'
            {
                hira.push('っ');
                i += 1;
                continue;
            }

            // 5. 嘗試匹配 3 碼羅馬字音節（拗音與特殊音節）
            if rem >= 3 {
                let s3: String = chars[i..i + 3].iter().collect();
                if let Some(mapped) = Self::lookup_romaji_table(&s3) {
                    hira.push_str(mapped);
                    i += 3;
                    continue;
                }
            }

            // 6. 嘗試匹配 2 碼羅馬字音節
            if rem >= 2 {
                let s2: String = chars[i..i + 2].iter().collect();
                if let Some(mapped) = Self::lookup_romaji_table(&s2) {
                    hira.push_str(mapped);
                    i += 2;
                    continue;
                }
            }

            // 7. 嘗試匹配 1 碼母音（a, i, u, e, o）
            let s1: String = chars[i..i + 1].iter().collect();
            if let Some(mapped) = Self::lookup_romaji_table(&s1) {
                hira.push_str(mapped);
                i += 1;
                continue;
            }

            // 8. 剩餘無法立即配對之尾端字元（如打字進行中的子音 k, sh, ch, ts, n）
            let pending: String = chars[i..].iter().collect();
            return (hira, pending);
        }

        (hira, String::new())
    }

    /// 當結算（空白鍵／Enter／選字）時，若尾端為單獨 'n'，自動收斂為撥音 'ん'
    pub fn finalized_hira(&self) -> String {
        if self.pending_romaji == "n" {
            format!("{}ん", self.hira_buffer)
        } else if self.pending_romaji.is_empty() {
            self.hira_buffer.clone()
        } else {
            format!("{}{}", self.hira_buffer, self.pending_romaji)
        }
    }

    fn lookup_romaji_table(rom: &str) -> Option<&'static str> {
        match rom {
            // 母音
            "a" => Some("あ"), "i" => Some("い"), "u" => Some("う"), "e" => Some("え"), "o" => Some("お"),
            // か行（含學習者 c 拼法 ca/cu/co）
            "ka" | "ca" => Some("か"), "ki" => Some("き"), "ku" | "cu" => Some("く"), "ke" => Some("け"), "ko" | "co" => Some("こ"),
            "kya" => Some("きゃ"), "kyu" => Some("きゅ"), "kyo" => Some("きょ"),
            "kwa" => Some("くぁ"),
            // さ行
            "sa" => Some("さ"), "shi" | "si" | "ci" => Some("し"), "su" => Some("す"), "se" | "ce" => Some("せ"), "so" => Some("そ"),
            "sha" | "sya" => Some("しゃ"), "shu" | "syu" => Some("しゅ"), "sho" | "syo" => Some("しょ"), "she" => Some("しぇ"),
            // た行
            "ta" => Some("た"), "chi" | "ti" => Some("ち"), "tsu" | "tu" => Some("つ"), "te" => Some("て"), "to" => Some("と"),
            "cha" | "tya" | "cya" => Some("ちゃ"), "chu" | "tyu" | "cyu" => Some("ちゅ"), "cho" | "tyo" | "cyo" => Some("ちょ"), "che" => Some("ちぇ"),
            "tsa" => Some("つぁ"), "tsi" => Some("つぃ"), "tse" => Some("つぇ"), "tso" => Some("つぉ"),
            "thi" => Some("てぃ"), "thu" => Some("とぅ"),
            // な行
            "na" => Some("な"), "ni" => Some("に"), "nu" => Some("ぬ"), "ne" => Some("ね"), "no" => Some("の"),
            "nya" => Some("にゃ"), "nyu" => Some("にゅ"), "nyo" => Some("にょ"),
            // は行
            "ha" => Some("は"), "hi" => Some("ひ"), "fu" | "hu" => Some("ふ"), "he" => Some("へ"), "ho" => Some("ほ"),
            "hya" => Some("ひゃ"), "hyu" => Some("ひゅ"), "hyo" => Some("ひょ"),
            "fa" => Some("ふぁ"), "fi" => Some("ふぃ"), "fe" => Some("ふぇ"), "fo" => Some("ふぉ"),
            // ま行
            "ma" => Some("ま"), "mi" => Some("み"), "mu" => Some("む"), "me" => Some("め"), "mo" => Some("も"),
            "mya" => Some("みゃ"), "myu" => Some("みゅ"), "myo" => Some("みょ"),
            // や行
            "ya" => Some("や"), "yu" => Some("ゆ"), "yo" => Some("よ"), "ye" => Some("いぇ"),
            // ら行
            "ra" | "la" => Some("ら"), "ri" | "li" => Some("り"), "ru" | "lu" => Some("る"), "re" | "le" => Some("れ"), "ro" | "lo" => Some("ろ"),
            "rya" | "lya" => Some("りゃ"), "ryu" | "lyu" => Some("りゅ"), "ryo" | "lyo" => Some("りょ"),
            // わ行
            "wa" => Some("わ"), "wi" => Some("うぃ"), "we" => Some("うぇ"), "wo" => Some("を"),
            // が行（濁音）
            "ga" => Some("が"), "gi" => Some("ぎ"), "gu" => Some("ぐ"), "ge" => Some("げ"), "go" => Some("ご"),
            "gya" => Some("ぎゃ"), "gyu" => Some("ぎゅ"), "gyo" => Some("ぎょ"), "gwa" => Some("ぐぁ"),
            // ざ行（濁音）
            "za" => Some("ざ"), "ji" | "zi" => Some("じ"), "zu" => Some("ず"), "ze" => Some("ぜ"), "zo" => Some("ぞ"),
            "ja" | "jya" | "zya" => Some("じゃ"), "ju" | "jyu" | "zyu" => Some("じゅ"), "jo" | "jyo" | "zyo" => Some("じょ"), "je" => Some("じぇ"),
            // だ行（濁音）
            "da" => Some("だ"), "di" => Some("ぢ"), "du" | "dzu" => Some("づ"), "de" => Some("で"), "do" => Some("ど"),
            "dya" => Some("ぢゃ"), "dyu" => Some("ぢゅ"), "dyo" => Some("ぢょ"),
            "dhi" => Some("でぃ"), "dhu" => Some("どぅ"),
            // ば行（濁音）與 v 行外來語
            "ba" => Some("ば"), "bi" => Some("び"), "bu" => Some("ぶ"), "be" => Some("べ"), "bo" => Some("ぼ"),
            "bya" => Some("びゃ"), "byu" => Some("びゅ"), "byo" => Some("びょ"),
            "va" => Some("ゔぁ"), "vi" => Some("ゔぃ"), "vu" => Some("ゔ"), "ve" => Some("ゔぇ"), "vo" => Some("ゔぉ"),
            // ぱ行（半濁音）
            "pa" => Some("ぱ"), "pi" => Some("ぴ"), "pu" => Some("ぷ"), "pe" => Some("ぺ"), "po" => Some("ぽ"),
            "pya" => Some("ぴゃ"), "pyu" => Some("ぴゅ"), "pyo" => Some("ぴょ"),
            // 小假名 (xa/xi...)
            "xa" => Some("ぁ"), "xi" => Some("ぃ"), "xu" => Some("ぅ"), "xe" => Some("ぇ"), "xo" => Some("ぉ"),
            "xya" => Some("ゃ"), "xyu" => Some("ゅ"), "xyo" => Some("ょ"), "xtu" | "xtsu" | "ltu" | "ltsu" => Some("っ"),
            _ => None,
        }
    }

    /// 嘗試匹配用言（一段動詞、五段動詞、サ變／カ變動詞、い形容詞）活用形與格助詞語境消歧
    /// 回傳 (匹配之平假名長度, 合成後之漢字＋送り仮名表層字串)
    fn match_inflected_predicate(chars: &[char], pos: usize, prev_context: &str) -> Option<(usize, String)> {
        let rem: String = chars[pos..].iter().collect();
        if rem.chars().count() < 2 {
            return None;
        }

        // 1. カ行變格動詞「来る」活用（依長至短排序防止短詞綴截斷，含學習者清濁音容錯）
        const KURU_FORMS: &[(&str, &str)] = &[
            ("きませんでした", "来ませんでした"),
            ("きませんでしだ", "来ませんでした"),
            ("こなかったです", "来なかったです"),
            ("きてください", "来てください"),
            ("きてぐださい", "来てください"),
            ("きています", "来ています"),
            ("こなかった", "来なかった"),
            ("きました", "来ました"),
            ("きましだ", "来ました"),
            ("きません", "来ません"),
            ("きますか", "来ますか"),
            ("きますね", "来ますね"),
            ("きますよ", "来ますよ"),
            ("きたいです", "来たいです"),
            ("きたい", "来たい"),
            ("きます", "来ます"),
            ("こない", "来ない"),
            ("こよう", "来よう"),
            ("くれば", "来れば"),
            ("きて", "来て"),
            ("きた", "来た"),
            ("くる", "来る"),
        ];
        for &(hira_form, surface_form) in KURU_FORMS {
            if rem.starts_with(hira_form) {
                return Some((hira_form.chars().count(), surface_form.to_string()));
            }
        }

        // 2. サ行變格動詞「する」活用（可獨立或接於動名詞如「勉強」「旅行」「練習」之後）
        const SURU_FORMS: &[(&str, &str)] = &[
            ("しませんでした", "しませんでした"),
            ("しませんでしだ", "しませんでした"),
            ("しなかったです", "しなかったです"),
            ("してください", "してください"),
            ("してぐださい", "してください"),
            ("しています", "しています"),
            ("していました", "していました"),
            ("しなかった", "しなかった"),
            ("しました", "しました"),
            ("しましだ", "しました"),
            ("しません", "しません"),
            ("しますか", "しますか"),
            ("しますね", "しますね"),
            ("しますよ", "しますよ"),
            ("したいです", "したいです"),
            ("したい", "したい"),
            ("します", "します"),
            ("しない", "しない"),
            ("しよう", "しよう"),
            ("すれば", "すれば"),
            ("できますか", "できますか"),
            ("できました", "できました"),
            ("できましだ", "できました"),
            ("できません", "できません"),
            ("できる", "できる"),
            ("できます", "できます"),
            ("して", "して"),
            ("した", "した"),
            ("する", "する"),
        ];
        for &(hira_form, surface_form) in SURU_FORMS {
            if rem.starts_with(hira_form) {
                return Some((hira_form.chars().count(), surface_form.to_string()));
            }
        }

        // 3. 一段動詞（語幹 + 活用語尾，依語幹由長至短排序）
        const ICHIDAN_STEMS: &[(&str, &str)] = &[
            ("うけつけ", "受け付け"),
            ("のりかえ", "乗り換え"),
            ("かたづけ", "片付け"),
            ("きをつけ", "気をつけ"),
            ("かんがえ", "考え"),
            ("おしえ", "教え"),
            ("おぼえ", "覚え"),
            ("でかけ", "出かけ"),
            ("つづけ", "続け"),
            ("はじめ", "始め"),
            ("しらべ", "調べ"),
            ("くらべ", "比べ"),
            ("ならべ", "並べ"),
            ("えらべ", "選べ"),
            ("わすれ", "忘れ"),
            ("つかれ", "疲れ"),
            ("わかれ", "別れ"),
            ("うまれ", "生まれ"),
            ("こわれ", "壊れ"),
            ("たおれ", "倒れ"),
            ("おくれ", "遅れ"),
            ("かくれ", "隠れ"),
            ("みつけ", "見つけ"),
            ("つたえ", "伝え"),
            ("むかえ", "迎え"),
            ("こたえ", "答え"),
            ("きこえ", "聞こえ"),
            ("かぞえ", "数え"),
            ("まちがえ", "間違え"),
            ("きがえ", "着替え"),
            ("あつめ", "集め"),
            ("きめ", "決め"),
            ("とめ", "止め"),
            ("やめ", "辞め"),
            ("ほめ", "褒め"),
            ("すて", "捨て"),
            ("たて", "建て"),
            ("そだて", "育て"),
            ("たべ", "食べ"),
            ("おき", "起き"),
            ("あけ", "開け"),
            ("しめ", "閉め"),
            ("つけ", "付け"),
            ("かけ", "掛け"),
            ("まけ", "負け"),
            ("にげ", "逃げ"),
            ("あげ", "上げ"),
            ("さげ", "下げ"),
            ("なげ", "投げ"),
            ("まげ", "曲げ"),
            ("おり", "降り"),
            ("かり", "借り"),
            ("たり", "足り"),
            ("あび", "浴び"),
            ("のび", "伸び"),
            ("おち", "落ち"),
            ("すぎ", "過ぎ"),
            ("みえ", "見え"),
            ("きえ", "消え"),
            ("ふえ", "増え"),
            ("ひえ", "冷え"),
            ("もえ", "燃え"),
            ("はれ", "晴れ"),
            ("ぬれ", "濡れ"),
            ("いれ", "入れ"),
            ("み", "見"),
            ("ね", "寝"),
            ("き", "着"),
            ("で", "出"),
        ];
        const ICHIDAN_ENDINGS: &[(&str, &str)] = &[
            ("ませんでした", "ませんでした"),
            ("ませんでしだ", "ませんでした"),
            ("ていました", "ていました"),
            ("ていましだ", "ていました"),
            ("てください", "てください"),
            ("てぐださい", "てください"),
            ("てはいけない", "てはいけない"),
            ("ています", "ています"),
            ("てみます", "てみます"),
            ("てもいい", "てもいい"),
            ("たいです", "たいです"),
            ("たくない", "たくない"),
            ("たかった", "たかった"),
            ("なかったです", "なかったです"),
            ("なかった", "なかった"),
            ("なければ", "なければ"),
            ("ました", "ました"),
            ("ましだ", "ました"),
            ("ません", "ません"),
            ("ますか", "ますか"),
            ("ますね", "ますね"),
            ("ますよ", "ますよ"),
            ("られる", "られる"),
            ("られます", "られます"),
            ("させる", "させる"),
            ("ます", "ます"),
            ("たい", "たい"),
            ("ない", "ない"),
            ("よう", "よう"),
            ("れば", "れば"),
            ("たら", "たら"),
            ("たり", "たり"),
            ("て", "て"),
            ("た", "た"),
            ("る", "る"),
        ];
        for &(stem_hira, stem_kanji) in ICHIDAN_STEMS {
            if stem_hira == "き"
                && !prev_context.ends_with("服を")
                && !prev_context.ends_with("着物を")
                && !prev_context.ends_with("シャツを")
                && !prev_context.ends_with("コートを")
            {
                continue;
            }
            if rem.starts_with(stem_hira) {
                let after_stem = &rem[stem_hira.len()..];
                for &(ending_hira, ending_norm) in ICHIDAN_ENDINGS {
                    if after_stem.starts_with(ending_hira) {
                        if stem_hira.chars().count() == 1 && ending_hira.chars().count() == 1 && prev_context.is_empty() {
                            continue;
                        }
                        let chosen_kanji = if stem_hira == "み" {
                            if prev_context.ends_with("映画を")
                                || prev_context.ends_with("アニメを")
                                || prev_context.ends_with("テレビを")
                                || prev_context.ends_with("動画を")
                            {
                                "観"
                            } else if prev_context.ends_with("病気を") || prev_context.ends_with("患者を") {
                                "診"
                            } else {
                                stem_kanji
                            }
                        } else {
                            stem_kanji
                        };
                        let total_chars = stem_hira.chars().count() + ending_hira.chars().count();
                        return Some((total_chars, format!("{}{}", chosen_kanji, ending_norm)));
                    }
                }
            }
        }

        // 4. 五段動詞（語幹 + 禮貌連用形 / 音便形 / 否定未然形 / 辭書形）
        // 結構：(語幹平假名, 語幹漢字, 連用i假名, 音便前綴(て/た或で/だ), 音便是否濁化d, 未然a假名, 辭書u假名)
        const GODAN_VERBS: &[(&str, &str, &str, &str, bool, &str, &str)] = &[
            ("がんば", "頑張", "り", "っ", false, "ら", "る"),
            ("てつだ", "手伝", "い", "っ", false, "わ", "う"),
            ("はたら", "働", "き", "い", false, "か", "く"),
            ("はな", "話", "し", "し", false, "さ", "す"),
            ("つか", "使", "い", "っ", false, "わ", "う"),
            ("わか", "分か", "り", "っ", false, "ら", "る"),
            ("おも", "思", "い", "っ", false, "わ", "う"),
            ("かえ", "帰", "り", "っ", false, "ら", "る"),
            ("はし", "走", "り", "っ", false, "ら", "る"),
            ("つく", "作", "り", "っ", false, "ら", "る"),
            ("あそ", "遊", "び", "ん", true, "ば", "ぶ"),
            ("およ", "泳", "ぎ", "い", true, "が", "ぐ"),
            ("ある", "歩", "き", "い", false, "か", "く"),
            ("おわ", "終わ", "り", "っ", false, "ら", "る"),
            ("はじま", "始ま", "り", "っ", false, "ら", "る"),
            ("とま", "泊ま", "り", "っ", false, "ら", "る"),
            ("きま", "決ま", "り", "っ", false, "ら", "る"),
            ("あつま", "集ま", "り", "っ", false, "ら", "る"),
            ("かわ", "変わ", "り", "っ", false, "ら", "る"),
            ("さわ", "触", "り", "っ", false, "ら", "る"),
            ("すわ", "座", "り", "っ", false, "ら", "る"),
            ("おこ", "怒", "り", "っ", false, "ら", "る"),
            ("のこ", "残", "り", "っ", false, "ら", "る"),
            ("のぼ", "登", "り", "っ", false, "ら", "る"),
            ("わた", "渡", "り", "っ", false, "ら", "る"),
            ("まが", "曲が", "り", "っ", false, "ら", "る"),
            ("あが", "上が", "り", "っ", false, "ら", "る"),
            ("さが", "下が", "り", "っ", false, "ら", "る"),
            ("はい", "入", "り", "っ", false, "ら", "る"),
            ("おく", "送", "り", "っ", false, "ら", "る"),
            ("うた", "歌", "い", "っ", false, "わ", "う"),
            ("わら", "笑", "い", "っ", false, "わ", "う"),
            ("あら", "洗", "い", "っ", false, "わ", "う"),
            ("はら", "払", "い", "っ", false, "わ", "う"),
            ("なら", "習", "い", "っ", false, "わ", "う"),
            ("もら", "貰", "い", "っ", false, "わ", "う"),
            ("ちが", "違", "い", "っ", false, "わ", "う"),
            ("むか", "向か", "い", "っ", false, "わ", "う"),
            ("えら", "選", "び", "ん", true, "ば", "ぶ"),
            ("はこ", "運", "び", "ん", true, "ば", "ぶ"),
            ("まな", "学", "び", "ん", true, "ば", "ぶ"),
            ("よろこ", "喜", "び", "ん", true, "ば", "ぶ"),
            ("やす", "休", "み", "ん", true, "ま", "む"),
            ("たの", "頼", "み", "ん", true, "ま", "む"),
            ("すす", "進", "み", "ん", true, "ま", "む"),
            ("いた", "痛", "み", "ん", true, "ま", "む"),
            ("さが", "探", "し", "し", false, "さ", "す"),
            ("なお", "直", "し", "し", false, "さ", "す"),
            ("かえ", "返", "し", "し", false, "さ", "す"),
            ("おもいだ", "思い出", "し", "し", false, "さ", "す"),
            ("とど", "届", "き", "い", false, "か", "く"),
            ("うご", "動", "き", "い", false, "か", "く"),
            ("つづ", "続", "き", "い", false, "か", "く"),
            ("いそ", "急", "ぎ", "い", true, "が", "ぐ"),
            ("ぬ", "脱", "ぎ", "い", true, "が", "ぐ"),
            ("い", "行", "き", "っ", false, "か", "く"),
            ("い", "言", "い", "っ", false, "わ", "う"),
            ("か", "書", "き", "い", false, "か", "く"),
            ("き", "聞", "き", "い", false, "か", "く"),
            ("お", "置", "き", "い", false, "か", "く"),
            ("な", "泣", "き", "い", false, "か", "く"),
            ("さ", "咲", "き", "い", false, "か", "く"),
            ("つ", "着", "き", "い", false, "か", "く"),
            ("ひ", "弾", "き", "い", false, "か", "く"),
            ("よ", "読", "み", "ん", true, "ま", "む"),
            ("の", "飲", "み", "ん", true, "ま", "む"),
            ("す", "住", "み", "ん", true, "ま", "む"),
            ("か", "買", "い", "っ", false, "わ", "う"),
            ("あ", "会", "い", "っ", false, "わ", "う"),
            ("ま", "待", "ち", "っ", false, "た", "つ"),
            ("も", "持", "ち", "っ", false, "た", "つ"),
            ("た", "立", "ち", "っ", false, "た", "つ"),
            ("か", "勝", "ち", "っ", false, "た", "つ"),
            ("し", "知", "り", "っ", false, "ら", "る"),
            ("の", "乗", "り", "っ", false, "ら", "る"),
            ("と", "撮", "り", "っ", false, "ら", "る"),
            ("と", "取", "り", "っ", false, "ら", "る"),
            ("う", "売", "り", "っ", false, "ら", "る"),
            ("き", "切", "り", "っ", false, "ら", "る"),
            ("ふ", "降", "り", "っ", false, "ら", "る"),
            ("よ", "呼", "び", "ん", true, "ば", "ぶ"),
            ("と", "飛", "び", "ん", true, "ば", "ぶ"),
            ("し", "死", "に", "ん", true, "な", "ぬ"),
            ("だ", "出", "し", "し", false, "さ", "す"),
            ("け", "消", "し", "し", false, "さ", "す"),
            ("お", "押", "し", "し", false, "さ", "す"),
            ("か", "貸", "し", "し", false, "さ", "す"),
        ];
        const MASU_SUFFIXES: &[(&str, &str)] = &[
            ("ませんでした", "ませんでした"),
            ("ませんでしだ", "ませんでした"),
            ("ました", "ました"),
            ("ましだ", "ました"),
            ("ません", "ません"),
            ("ますか", "ますか"),
            ("ますね", "ますね"),
            ("ますよ", "ますよ"),
            ("ます", "ます"),
            ("たいです", "たいです"),
            ("たい", "たい"),
        ];
        const TE_SUFFIXES: &[(&str, &str)] = &[
            ("ていました", "ていました"),
            ("ていましだ", "ていました"),
            ("てください", "てください"),
            ("てぐださい", "てください"),
            ("ています", "ています"),
            ("てもいい", "てもいい"),
            ("たら", "たら"),
            ("たり", "たり"),
            ("て", "て"),
            ("た", "た"),
        ];
        const DE_SUFFIXES: &[(&str, &str)] = &[
            ("でいました", "でいました"),
            ("でいましだ", "でいました"),
            ("でください", "でください"),
            ("でぐださい", "でください"),
            ("でいます", "でいます"),
            ("でもいい", "でもいい"),
            ("だら", "だら"),
            ("だり", "だり"),
            ("で", "で"),
            ("だ", "だ"),
        ];
        const NAI_SUFFIXES: &[&str] = &["なかったです", "なかった", "なければ", "ない"];

        for &(stem_h, stem_k, ren_i, onbin, is_voiced_te, mizen_a, dict_u) in GODAN_VERBS {
            // 針對同音語幹依據前驅格助詞語境擇優
            if stem_h == "の" && stem_k == "乗" && !prev_context.ends_with('に') {
                continue;
            }
            if stem_h == "と" && stem_k == "撮" && !prev_context.ends_with("写真を") {
                continue;
            }
            if stem_h == "ふ" && stem_k == "降" && !prev_context.ends_with("雨が") && !prev_context.ends_with("雪が") {
                continue;
            }
            if stem_h == "さ" && stem_k == "咲" && !prev_context.ends_with("花が") && !prev_context.ends_with("桜が") {
                continue;
            }
            if stem_h == "ひ" && stem_k == "弾" && !prev_context.ends_with("ピアノを") && !prev_context.ends_with("ギターを") {
                continue;
            }
            let effective_kanji = if stem_h == "き" && stem_k == "聞" {
                if prev_context.ends_with("音楽を") || prev_context.ends_with("歌を") {
                    "聴"
                } else {
                    "聞"
                }
            } else {
                stem_k
            };

            if rem.starts_with(stem_h) {
                let after_stem = &rem[stem_h.len()..];
                // (a) 連用形 + ます / たい
                if after_stem.starts_with(ren_i) {
                    let after_ren = &after_stem[ren_i.len()..];
                    for &(ms_hira, ms_norm) in MASU_SUFFIXES {
                        if after_ren.starts_with(ms_hira) {
                            let len = stem_h.chars().count() + ren_i.chars().count() + ms_hira.chars().count();
                            return Some((len, format!("{}{}{}", effective_kanji, ren_i, ms_norm)));
                        }
                    }
                }
                // (b) 音便形 + て/た 或 で/だ
                if after_stem.starts_with(onbin) {
                    let after_onbin = &after_stem[onbin.len()..];
                    let suffixes = if is_voiced_te { DE_SUFFIXES } else { TE_SUFFIXES };
                    for &(ts_hira, ts_norm) in suffixes {
                        if after_onbin.starts_with(ts_hira) {
                            let len = stem_h.chars().count() + onbin.chars().count() + ts_hira.chars().count();
                            return Some((len, format!("{}{}{}", effective_kanji, onbin, ts_norm)));
                        }
                    }
                }
                // (c) 未然形 + ない / なかった
                if after_stem.starts_with(mizen_a) {
                    let after_mizen = &after_stem[mizen_a.len()..];
                    for &ns in NAI_SUFFIXES {
                        if after_mizen.starts_with(ns) {
                            let len = stem_h.chars().count() + mizen_a.chars().count() + ns.chars().count();
                            return Some((len, format!("{}{}{}", effective_kanji, mizen_a, ns)));
                        }
                    }
                }
                // (d) 辭書原形
                if after_stem.starts_with(dict_u) && (stem_h.chars().count() >= 2 || !prev_context.is_empty()) {
                    let len = stem_h.chars().count() + dict_u.chars().count();
                    return Some((len, format!("{}{}", effective_kanji, dict_u)));
                }
            }
        }

        // 5. い形容詞活用（語幹 + かった / くない / くて / い）
        const I_ADJ_STEMS: &[(&str, &str)] = &[
            ("あたたか", "暖か"),
            ("すばらし", "素晴らし"),
            ("あたらし", "新し"),
            ("むずかし", "難し"),
            ("おもしろ", "面白"),
            ("いそがし", "忙し"),
            ("うつくし", "美し"),
            ("はずかし", "恥ずかし"),
            ("うらやまし", "羨まし"),
            ("なつかし", "懐かし"),
            ("めずらし", "珍し"),
            ("すずし", "涼し"),
            ("おいし", "美味し"),
            ("かわい", "可愛"),
            ("たのし", "楽し"),
            ("うれし", "嬉し"),
            ("やさし", "優し"),
            ("かなし", "悲し"),
            ("さびし", "寂し"),
            ("きびし", "厳し"),
            ("ただ", "正し"),
            ("くるし", "苦し"),
            ("おおき", "大き"),
            ("ちいさ", "小さ"),
            ("あかる", "明る"),
            ("あぶな", "危な"),
            ("つめた", "冷た"),
            ("すくな", "少な"),
            ("みじか", "短"),
            ("たか", "高"),
            ("やす", "安"),
            ("ひく", "低"),
            ("なが", "長"),
            ("ひろ", "広"),
            ("せま", "狭"),
            ("とお", "遠"),
            ("ちか", "近"),
            ("はや", "早"),
            ("おそ", "遅"),
            ("おお", "多"),
            ("わる", "悪"),
            ("あつ", "暑"),
            ("さむ", "寒"),
            ("くら", "暗"),
            ("おも", "重"),
            ("かる", "軽"),
            ("つよ", "強"),
            ("よわ", "弱"),
            ("ふる", "古"),
            ("わか", "若"),
            ("いた", "痛"),
            ("ねむ", "眠"),
            ("こわ", "怖"),
            ("ほし", "欲し"),
        ];
        const I_ADJ_ENDINGS: &[(&str, &str)] = &[
            ("くありませんでした", "くありませんでした"),
            ("くありませんでしだ", "くありませんでした"),
            ("くありません", "くありません"),
            ("くなかったです", "くなかったです"),
            ("くなかたです", "くなかったです"),
            ("くなかった", "くなかった"),
            ("くなかた", "くなかった"),
            ("くないです", "くないです"),
            ("かったです", "かったです"),
            ("かたです", "かったです"),
            ("くなります", "くなります"),
            ("くてもいい", "くてもいい"),
            ("くない", "くない"),
            ("かった", "かった"),
            ("かた", "かった"),
            ("くなる", "くなる"),
            ("くて", "くて"),
            ("いですか", "いですか"),
            ("いですが", "いですが"),
            ("いです", "いです"),
            ("ですか", "いですか"),
            ("ですが", "いですか"),
            ("です", "いです"),
            ("い", "い"),
        ];
        for &(adj_h, adj_k) in I_ADJ_STEMS {
            if rem.starts_with(adj_h) {
                let after_stem = &rem[adj_h.len()..];
                for &(ending_hira, ending_norm) in I_ADJ_ENDINGS {
                    if after_stem.starts_with(ending_hira) {
                        if adj_h.chars().count() <= 2 && ending_hira == "い" && prev_context.is_empty() && rem.chars().count() > adj_h.chars().count() + 1 {
                            let next_c = rem[adj_h.len() + ending_hira.len()..].chars().next().unwrap_or(' ');
                            if !matches!(next_c, 'で' | 'か' | 'ね' | 'よ' | 'け' | 'の' | 'と' | 'が' | '、' | '。') {
                                continue;
                            }
                        }
                        let len = adj_h.chars().count() + ending_hira.chars().count();
                        return Some((len, format!("{}{}", adj_k, ending_norm)));
                    }
                }
            }
        }

        None
    }

    /// 判斷字元是否為漢字或全形片假名（體言詞尾特徵）
    fn is_kanji_or_kata(ch: char) -> bool {
        let code = ch as u32;
        (0x4E00..=0x9FFF).contains(&code)
            || (0x3400..=0x4DBF).contains(&code)
            || (0x30A1..=0x30FA).contains(&code)
            || ch == 'ー'
    }

    /// 判斷前驅已轉換字串是否為場所、方向或機構體言（供學習者將方向助詞 `へ` 拼為 `e` / `え` 時自動歸一）
    fn ends_with_place_or_direction(prev_out: &str) -> bool {
        const PLACE_SUFFIXES: &[&str] = &[
            "日本", "台湾", "台北", "東京", "大阪", "京都", "北海道", "沖縄", "富士山", "世界",
            "学校", "大学", "会社", "駅", "空港", "ホテル", "コンビニ", "デパート", "スーパー",
            "レストラン", "カフェ", "病院", "銀行", "郵便局", "図書館", "公園", "家", "部屋",
            "国", "町", "店", "海", "山", "川", "橋", "前", "後", "上", "下", "右", "左",
            "東", "西", "南", "北", "ここ", "そこ", "あそこ", "どこ", "こちら", "そちら",
            "あちら", "どちら",
        ];
        PLACE_SUFFIXES.iter().any(|&suf| prev_out.ends_with(suf))
    }

    /// 連文節與用言活用智慧轉換（如「わたしはにほんごをべんきょうします」或學習者拼法「わたしわにほんごおべんきょうします」➔「私は日本語を勉強します」）
    pub fn compose_kanji_sentence(&self, hira: &str) -> String {
        if hira.is_empty() {
            return String::new();
        }
        if let Some(entries) = self.lexicon.get(hira) {
            if let Some(first) = entries.first() {
                return first.surface.clone();
            }
        }
        let chars: Vec<char> = hira.chars().collect();
        let n = chars.len();
        let mut out = String::new();
        let mut i = 0;
        while i < n {
            // 0. 撥音連音自動拆解（Nasal-Vowel Liaison Decomposition）：
            // 當撥音結尾之場所體言（如「にほん」日本、「たいわん」台湾、「こうえん」公園、「としょかん」図書館）後接方向助詞 `e`（へ）時，
            // 羅馬字流 `n` + `e` 會被貪婪合併為「ね」（例如 nihoneikimasu ➔ にほねいきます）；
            // 同理，撥音結尾之受詞體言（如「ほん」本、「ごはん」ご飯、「しゃしん」写真）後接賓格助詞 `o`（を）時，
            // `n` + `o` 會被合併為「の」（例如 honoyomimasu ➔ ほのよみます）。
            // 在此依據後接移動動詞或及物動詞語境，精準還原為「體言 + へ／を」！
            const N_PLACE_LIAISON_STEMS: &[(&str, &str)] = &[
                ("びじゅつか", "美術館"),
                ("はくぶつか", "博物館"),
                ("たいいくか", "体育館"),
                ("としょか", "図書館"),
                ("たいしか", "大使館"),
                ("えいがか", "映画館"),
                ("びょうい", "病院"),
                ("たいわ", "台湾"),
                ("こうえ", "公園"),
                ("りょか", "旅館"),
                ("おんせ", "温泉"),
                ("ふじさ", "富士山"),
                ("にほ", "日本"),
            ];
            let rem_here: String = chars[i..].iter().collect();
            let mut liaison_matched = false;
            for &(stem_no_n, kanji_surf) in N_PLACE_LIAISON_STEMS {
                let target_ne = format!("{}ね", stem_no_n);
                if rem_here.starts_with(&target_ne) {
                    let after_ne = &rem_here[target_ne.len()..];
                    let is_motion_next = after_ne.is_empty()
                        || after_ne.starts_with("い")
                        || after_ne.starts_with("き")
                        || after_ne.starts_with("く")
                        || after_ne.starts_with("こ")
                        || after_ne.starts_with("かえ")
                        || after_ne.starts_with("でかけ")
                        || after_ne.starts_with("むか")
                        || after_ne.starts_with("もど")
                        || after_ne.starts_with("りょこう")
                        || after_ne.starts_with("さんぽ");
                    if is_motion_next {
                        out.push_str(kanji_surf);
                        out.push('へ');
                        i += target_ne.chars().count();
                        liaison_matched = true;
                        break;
                    }
                }
            }
            if liaison_matched {
                continue;
            }

            const N_OBJECT_LIAISON_STEMS: &[(&str, &str)] = &[
                ("あさごは", "朝ご飯"),
                ("ひるごは", "昼ご飯"),
                ("ばんごは", "晩ご飯"),
                ("しんぶ", "新聞"),
                ("らーめ", "ラーメン"),
                ("しゃし", "写真"),
                ("ごは", "ご飯"),
                ("うど", "うどん"),
                ("かば", "鞄"),
                ("ほ", "本"),
                ("ぱ", "パン"),
            ];
            for &(stem_no_n, kanji_surf) in N_OBJECT_LIAISON_STEMS {
                let target_no = format!("{}の", stem_no_n);
                if rem_here.starts_with(&target_no) {
                    let next_pos = i + target_no.chars().count();
                    if next_pos < n {
                        let ctx_with_wo = format!("{}{}を", out, kanji_surf);
                        if Self::match_inflected_predicate(&chars, next_pos, &ctx_with_wo).is_some() {
                            out.push_str(kanji_surf);
                            out.push('を');
                            i = next_pos;
                            liaison_matched = true;
                            break;
                        }
                    }
                }
            }
            if liaison_matched {
                continue;
            }

            // 1. 優先檢查用言（動詞／形容詞）活用形與格助詞語境搭配
            let inflected = Self::match_inflected_predicate(&chars, i, &out);

            // 2. 檢查靜態詞庫最長前綴匹配（含連文節助詞保護與學習者音韻骨架即時容錯）
            let mut lex_len = 0;
            let mut lex_surface = String::new();
            let max_span = (n - i).min(12);
            for span in (2..=max_span).rev() {
                let sub: String = chars[i..i + span].iter().collect();
                if let Some(entries) = self.lexicon.get(&sub) {
                    if let Some(best) = entries.first() {
                        // 連文節助詞邊界保護：當 2 假名詞以助詞（は/わ/が/を/に/へ/で/と/も/の）開頭（如「はな」花、「わく」枠），
                        // 若前方已有實詞，且把首假名當作助詞後緊接位置 `i + 1` 能形成 `>= 2` 假名的合法實詞或用言（如「は + だれ」➔「は誰」），
                        // 則優先保留助詞，絕不吞噬助詞！
                        let first_ch = chars[i];
                        if span == 2
                            && !out.is_empty()
                            && matches!(first_ch, 'は' | 'わ' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の')
                            && i + 1 < n
                        {
                            let max_next = (n - (i + 1)).min(10);
                            let mut next_has_word = false;
                            for next_span in (2..=max_next).rev() {
                                let next_sub: String = chars[i + 1..i + 1 + next_span].iter().collect();
                                if self.lexicon.contains_key(&next_sub) {
                                    next_has_word = true;
                                    break;
                                }
                                if next_span >= 3 {
                                    let next_skel = Self::learner_phonetic_skeleton(&next_sub);
                                    if self.skeleton_index.contains_key(&next_skel) {
                                        next_has_word = true;
                                        break;
                                    }
                                }
                            }
                            if !next_has_word {
                                let p_norm = if first_ch == 'わ' { 'は' } else { first_ch };
                                let ctx_with_p = format!("{}{}", out, p_norm);
                                if Self::match_inflected_predicate(&chars, i + 1, &ctx_with_p).is_some() {
                                    next_has_word = true;
                                }
                            }
                            if next_has_word {
                                continue;
                            }
                        }
                        lex_len = span;
                        lex_surface = best.surface.clone();
                        break;
                    }
                }
            }

            // 2b. 學習者音韻骨架即時容錯匹配與跨長比較（解決清濁音混淆如 あなだ➔貴方、わだし➔私、あしだ➔明日、しこと➔仕事、ともたち➔友達、だいじょぶ➔大丈夫、がこう➔学校、せんせ➔先生，且若 3+ 假名骨架詞比如「あしだ」3 假名比精確 2 假名詞「あし」更長，優先採納 3+ 假名長詞！）
            if lex_len <= 2 {
                // 先檢查常見漏打撥音或長音之高頻學習者發音詞根
                const LEARNER_DIRECT_STEMS: &[(&str, &str)] = &[
                    ("にほご", "日本語"),
                    ("にほんこ", "日本語"),
                    ("にほじん", "日本人"),
                    ("こにちわ", "こんにちは"),
                    ("こにちは", "こんにちは"),
                    ("こばんわ", "こんばんは"),
                    ("こばんは", "こんばんは"),
                    ("ごめなさい", "ごめんなさい"),
                    ("すみませ", "すみません"),
                    ("しんかせん", "新幹線"),
                    ("べんきょ", "勉強"),
                    ("れんしゅ", "練習"),
                    ("りょこ", "旅行"),
                    ("がっこ", "学校"),
                    ("がこう", "学校"),
                    ("がこ", "学校"),
                    ("せんせえ", "先生"),
                    ("せんせ", "先生"),
                    ("だいじょぶ", "大丈夫"),
                    ("たいじょうぶ", "大丈夫"),
                    ("たいじょぶ", "大丈夫"),
                    ("おとうさ", "お父さん"),
                    ("おかあさ", "お母さん"),
                    ("おにいさ", "お兄さん"),
                    ("おねえさ", "お姉さん"),
                ];
                for &(l_hira, l_surf) in LEARNER_DIRECT_STEMS {
                    let l_len = l_hira.chars().count();
                    if l_len > lex_len && rem_here.starts_with(l_hira) {
                        lex_len = l_len;
                        lex_surface = l_surf.to_string();
                        break;
                    }
                }
            }

            if lex_len <= 2 {
                // 若目前已精確命中 2 假名實詞（lex_len == 2，如「はな」花、「えき」駅、「まち」町、「みず」水），
                // 且第 3 假名為格助詞／主題助詞（は/わ/が/を/に/へ/で/と/も/の）並緊接合法詞彙或用言，
                // 則保護該 2 假名實詞與助詞邊界，不進行跨助詞之模糊骨架覆蓋（唯「あしだ」明日、「しこと」仕事等無助詞邊界者仍允許升格）。
                let exact_2char_followed_by_particle = lex_len == 2
                    && i + 2 < n
                    && matches!(
                        chars[i + 2],
                        'は' | 'わ' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の'
                    );
                if !exact_2char_followed_by_particle {
                    let min_skel_span = (lex_len + 1).max(2);
                    for span in (min_skel_span..=max_span).rev() {
                        let sub: String = chars[i..i + span].iter().collect();
                        let first_ch = chars[i];
                        let last_ch = chars[i + span - 1];
                        // 避免把首尾之主題／格助詞或句尾助動詞捲入模糊骨架
                        if !out.is_empty() && matches!(first_ch, 'は' | 'わ' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の') {
                            continue;
                        }
                        let allow_2char_skel = span == 2
                            && matches!(
                                sub.as_str(),
                                "たれ" | "とこ" | "いづ" | "なせ" | "がれ" | "ぎみ" | "ほく" | "ぼぐ" | "いま" | "きょう" | "きょ" | "きの"
                            );
                        if !allow_2char_skel
                            && matches!(
                                last_ch,
                                'は' | 'わ' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の' | 'ね' | 'よ'
                            )
                        {
                            continue;
                        }
                        if sub.ends_with("です")
                            || sub.ends_with("ます")
                            || sub.ends_with("でした")
                            || sub.ends_with("ました")
                            || sub.ends_with("ですが")
                            || sub.ends_with("ですか")
                        {
                            continue;
                        }
                        let sub_skel = Self::learner_phonetic_skeleton(&sub);
                        let skel_char_len = sub_skel.chars().count();
                        // 骨架長度 >= 3（如 あなだ、わだし、ともたち、あしだ、しこと、きの、きょうと），或特定 2 假名疑問／代名詞（だれ/たれ、なに、どこ/とこ、いつ/いづ、なぜ/なせ、かれ/がれ、きみ/ぎみ）
                        if skel_char_len >= 3 || allow_2char_skel {
                            if let Some(entries) = self.skeleton_index.get(&sub_skel) {
                                if let Some(best) = entries.iter().find(|e| {
                                    e.weight >= 1600
                                        && e.reading_hira.chars().count().abs_diff(span) <= 1
                                }) {
                                    lex_len = span;
                                    lex_surface = best.surface.clone();
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // 2c. 疑問句尾清濁音混淆自動歸一（「ですが」➔「ですか」、「ますが」➔「ますか」、「でしだが」➔「でしたか」）：
            // 當句中已出現疑問代名詞（誰、何、どこ、いつ、なぜ、どう、いくら、いくつ、どれ、どの、どちら、どっち、どなた）
            // 或第二人稱／指示主題（貴方は、あなたは、君は、これは、それは、あれは、お名前は、お元気）且位於句尾時，
            // 外國學習者常因母語清濁音感知差異將句尾疑問助詞 ka 打為 ga（如「あなだは誰ですが」），自動歸一為「ですか」！
            if rem_here == "ですが" || rem_here == "ますが" || rem_here == "でしだが" || rem_here == "ましだが" {
                let has_interrogative_context = [
                    "誰", "だれ", "どなた", "何方", "何", "なに", "なん", "どこ", "何処", "いつ", "何時",
                    "なぜ", "何故", "どうして", "どう", "いかが", "いくら", "幾ら", "いくつ", "幾つ",
                    "どれ", "どの", "どちら", "どっち", "どんな", "貴方は", "あなたは", "君は",
                    "お名前", "お元気", "元気",
                ]
                .iter()
                .any(|&kw| out.contains(kw));
                if has_interrogative_context {
                    let norm_tail = match rem_here.as_str() {
                        "ですが" => "ですか",
                        "ますが" => "ますか",
                        "でしだが" => "でしたか",
                        "ましだが" => "ましたか",
                        _ => "ですか",
                    };
                    out.push_str(norm_tail);
                    break;
                }
            }
            if rem_here == "でしだ" {
                out.push_str("でした");
                break;
            }
            if rem_here == "ましだ" {
                out.push_str("ました");
                break;
            }

            // 比較用言活用跨距與靜態詞條跨距：取跨距較長者（若同長則優先用言活用語境結果）
            if let Some((inf_len, inf_surf)) = inflected {
                if inf_len >= lex_len {
                    out.push_str(&inf_surf);
                    i += inf_len;
                    continue;
                }
            }

            if lex_len > 0 {
                out.push_str(&lex_surface);
                i += lex_len;
                continue;
            }

            // 3. 嘗試單假名詞彙（僅限高權重實詞如 私/今/本/車/駅，避免助詞 は/が/を/に 被誤轉為漢字）
            let sub1: String = chars[i..i + 1].iter().collect();
            let is_particle = matches!(
                chars[i],
                'は' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の' | 'か' | 'ね' | 'よ'
            );
            if !is_particle {
                if let Some(entries) = self.lexicon.get(&sub1) {
                    if let Some(best) = entries.first() {
                        if best.weight >= 1450 {
                            out.push_str(&best.surface);
                            i += 1;
                            continue;
                        }
                    }
                }
            }

            // 4. 學習者助詞發音自動歸一（Learner Particle Pronunciation Normalization）：
            // (a) 主題助詞「は」（讀音 wa）：當體言／代名詞／指示詞或格助詞後接未匹配入詞之單假名「わ」時，自動歸一為助詞「は」
            //     （例如 watashiwa ➔ 私は、nihongowa ➔ 日本語は、kyouwa ➔ 今日は、korewa ➔ これは）
            if chars[i] == 'わ' && !out.is_empty() {
                let prev_ch = out.chars().last().unwrap_or(' ');
                if !matches!(prev_ch, 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ゃ' | 'ゅ' | 'ょ' | 'っ' | '、' | '。') {
                    out.push('は');
                    i += 1;
                    continue;
                }
            }

            // (b) 方向助詞「へ」（讀音 e）：當場所／方向體言後接單假名「え」（且位於句尾或後接移動動詞）時，自動歸一為助詞「へ」
            //     （例如 nihoneikimasu ➔ 日本へ行きます、gakkoue ➔ 学校へ、toukyoue ➔ 東京へ）
            if chars[i] == 'え' && Self::ends_with_place_or_direction(&out) {
                let rem_after: String = chars[i + 1..].iter().collect();
                let is_motion_next = rem_after.is_empty()
                    || rem_after.starts_with("い")
                    || rem_after.starts_with("き")
                    || rem_after.starts_with("く")
                    || rem_after.starts_with("こ")
                    || rem_after.starts_with("かえ")
                    || rem_after.starts_with("でかけ")
                    || rem_after.starts_with("むか")
                    || rem_after.starts_with("もど");
                if is_motion_next {
                    out.push('へ');
                    i += 1;
                    continue;
                }
            }

            // (c) 賓格助詞「を」（讀音 o）：當漢字／片假名受詞後接單假名「お」，且緊接用言活用或サ變動名詞時，自動歸一為助詞「を」
            //     （例如 nihongoobenkyoushimasu ➔ 日本語を勉強します、eigaomimasu ➔ 映画を観ます）
            if chars[i] == 'お' && !out.is_empty() && i + 1 < n {
                let prev_ch = out.chars().last().unwrap_or(' ');
                if Self::is_kanji_or_kata(prev_ch) {
                    let ctx_with_wo = format!("{}を", out);
                    let rem_after: String = chars[i + 1..].iter().collect();
                    let next_is_predicate = Self::match_inflected_predicate(&chars, i + 1, &ctx_with_wo).is_some()
                        || [
                            "べんきょう", "べんきょ", "れんしゅう", "れんしゅ", "しつもん", "せつめい",
                            "じゅんび", "よやく", "りょうり", "かいもの", "しごと", "でんわ",
                            "すき", "だいすき", "さんぽ", "りょこう", "りょこ", "うんどう",
                            "しょうかい", "かくにん", "けんきゅう",
                        ]
                        .iter()
                        .any(|&vn| rem_after.starts_with(vn))
                        || [
                            "します", "しました", "しません", "したい", "する", "して", "した",
                            "できます", "できる", "ください",
                        ]
                        .iter()
                        .any(|&sf| rem_after.contains(sf));
                    if next_is_predicate {
                        out.push('を');
                        i += 1;
                        continue;
                    }
                }
            }

            out.push(chars[i]);
            i += 1;
        }
        out
    }

    /// 零基礎日文學習者救援橋接：當不懂日文讀音的使用者輸入常用英文單字或常見羅馬字誤植時，直接提供對應日文常用詞句
    pub fn lookup_english_learner_bridge(raw_lower: &str) -> &'static [&'static str] {
        match raw_lower {
            "hello" | "hi" => &["こんにちは", "どうも"],
            "thanks" | "thank" | "thankyou" | "thx" | "tks" => &[
                "ありがとうございます",
                "ありがとう",
                "どうもありがとうございます",
            ],
            "sorry" | "excuse" | "excuseme" => &["すみません", "ごめんなさい", "失礼します"],
            "morning" | "goodmorning" => &["おはようございます", "おはよう"],
            "evening" | "goodevening" => &["こんばんは"],
            "night" | "goodnight" => &["おやすみなさい", "おやすみ"],
            "bye" | "goodbye" => &["さようなら", "またね", "お疲れ様でした"],
            "please" => &["お願いします", "よろしくお願いします", "ください"],
            "yes" | "ok" | "okay" => &["はい", "大丈夫です", "わかりました"],
            "no" => &["いいえ", "大丈夫です"],
            "delicious" | "yummy" | "good" => &["美味しい", "とても美味しいです", "いいね"],
            "cute" => &["可愛い", "とても可愛いです"],
            "like" | "love" => &["好きです", "大好きです"],
            "water" => &["お水", "水をください"],
            "coffee" => &["コーヒー", "コーヒーをください"],
            "tea" => &["お茶", "お茶をください"],
            "toilet" | "restroom" | "bathroom" => &["トイレはどこですか", "お手洗い"],
            "station" => &["駅", "駅はどこですか"],
            "airport" => &["空港"],
            "hotel" => &["ホテル"],
            "howmuch" | "price" => &["いくらですか", "これはいくらですか"],
            "where" => &["どこですか"],
            "check" | "bill" => &["お会計お願いします"],
            "help" => &["助けてください", "すみません、お願いします"],
            "yourishi" | "yorishi" | "youroshi" | "yorosi" | "yoroshi" | "ようりし" | "よりし" | "ようろし" | "よろし" => &[
                "よろしくお願いします",
                "よろしく",
            ],
            "nihogo" | "にほご" => &["日本語"],
            "otsukare" | "otukare" | "おつかれ" => &["お疲れ様でした", "お疲れ様です", "お疲れ様"],
            "itadaki" | "いただき" => &["いただきます"],
            _ => &[],
        }
    }

    pub fn update_candidates(&mut self) {
        self.candidates.clear();
        self.candidate_idx = 0;
        if self.raw_romaji.is_empty() {
            return;
        }
        if self.is_english_forced {
            self.candidates.push(self.raw_romaji.clone());
            return;
        }
        let lower_raw = self.raw_romaji.to_ascii_lowercase();
        let (hira, pending) = Self::convert_romaji_stream(&self.raw_romaji);
        self.hira_buffer = hira;
        self.pending_romaji = pending;

        let full_hira = self.finalized_hira();
        let full_kata = Self::hira_to_kata(&full_hira);

        // 0. 零基礎英文橋接詞與常見學習者誤植救援（如輸入 hello / thanks / yourishi 直接提供日文候選）
        for &bridged in Self::lookup_english_learner_bridge(&lower_raw) {
            if !self.candidates.contains(&bridged.to_string()) {
                self.candidates.push(bridged.to_string());
            }
        }
        for &bridged in Self::lookup_english_learner_bridge(&full_hira) {
            if !self.candidates.contains(&bridged.to_string()) {
                self.candidates.push(bridged.to_string());
            }
        }

        // 1. 精確命中整詞漢字／片假名候選
        if let Some(entries) = self.lexicon.get(&full_hira) {
            for e in entries {
                if !self.candidates.contains(&e.surface) {
                    self.candidates.push(e.surface.clone());
                }
            }
        } else if full_hira.chars().count() >= 3 {
            // 若整詞屬於學習者音韻骨架（如 あなだ、わだし、だいじょぶ、せんせえ），補入該骨架之全部同音／漢字變體（如 貴方、あなた、貴女）
            let skel = Self::learner_phonetic_skeleton(&full_hira);
            if let Some(entries) = self.skeleton_index.get(&skel) {
                for e in entries {
                    if e.reading_hira.chars().count().abs_diff(full_hira.chars().count()) <= 1
                        && !self.candidates.contains(&e.surface)
                    {
                        self.candidates.push(e.surface.clone());
                    }
                }
            }
        }

        // 2. 連文節組合候選（僅當確實有漢字、片假名或助詞歸一轉換時置頂）
        let composed = self.compose_kanji_sentence(&full_hira);
        if !composed.is_empty()
            && composed != full_hira
            && !self.candidates.contains(&composed)
            && Self::lookup_english_learner_bridge(&lower_raw).is_empty()
            && Self::lookup_english_learner_bridge(&full_hira).is_empty()
        {
            self.candidates.insert(0, composed.clone());
        }
        // 若連文節中包含「貴方」，同步提供現代平假名表記「あなた」之對應連文節作為緊接候選（例如「貴方は誰ですか」與「あなたは誰ですか」）
        if composed.contains("貴方") {
            let anata_kana_variant = composed.replace("貴方", "あなた");
            if !self.candidates.contains(&anata_kana_variant) {
                let insert_pos = self.candidates.len().min(1);
                self.candidates.insert(insert_pos, anata_kana_variant);
            }
        }

        // 若尾端為學習者助詞發音「わ」(wa)，同步提供正統平假名「…は」作為候選
        if full_hira.ends_with('わ') && full_hira.chars().count() >= 2 {
            let mut hira_ha: Vec<char> = full_hira.chars().collect();
            if let Some(last) = hira_ha.last_mut() {
                *last = 'は';
            }
            let hira_ha_str: String = hira_ha.into_iter().collect();
            if let Some(entries) = self.lexicon.get(&hira_ha_str) {
                for e in entries {
                    if !self.candidates.contains(&e.surface) {
                        self.candidates.push(e.surface.clone());
                    }
                }
            }
            if !self.candidates.contains(&hira_ha_str) {
                self.candidates.push(hira_ha_str);
            }
        }

        // 3. 平假名、片假名、原始羅馬字保底候選
        if !full_hira.is_empty() && !self.candidates.contains(&full_hira) {
            self.candidates.push(full_hira);
        }
        if !full_kata.is_empty() && !self.candidates.contains(&full_kata) {
            self.candidates.push(full_kata);
        }
        if !self.candidates.contains(&self.raw_romaji) {
            self.candidates.push(self.raw_romaji.clone());
        }
    }

    /// 依指定羅馬字串查詢 `↓` 下箭頭候選（英日橋接、漢字、連文節、平假名、片假名、羅馬字）
    pub fn lookup_down_candidates_for_romaji(&self, romaji: &str) -> Vec<String> {
        if romaji.is_empty() {
            return Vec::new();
        }
        let lower_raw = romaji.to_ascii_lowercase();
        let (hira, pending) = Self::convert_romaji_stream(romaji);
        let full_hira = if pending == "n" {
            format!("{}ん", hira)
        } else if pending.is_empty() {
            hira
        } else {
            format!("{}{}", hira, pending)
        };
        let full_kata = Self::hira_to_kata(&full_hira);
        let mut cands = Vec::new();
        for &bridged in Self::lookup_english_learner_bridge(&lower_raw) {
            if !cands.contains(&bridged.to_string()) {
                cands.push(bridged.to_string());
            }
        }
        for &bridged in Self::lookup_english_learner_bridge(&full_hira) {
            if !cands.contains(&bridged.to_string()) {
                cands.push(bridged.to_string());
            }
        }
        if let Some(entries) = self.lexicon.get(&full_hira) {
            for e in entries {
                if !cands.contains(&e.surface) {
                    cands.push(e.surface.clone());
                }
            }
        } else if full_hira.chars().count() >= 3 {
            let skel = Self::learner_phonetic_skeleton(&full_hira);
            if let Some(entries) = self.skeleton_index.get(&skel) {
                for e in entries {
                    if e.reading_hira.chars().count().abs_diff(full_hira.chars().count()) <= 1
                        && !cands.contains(&e.surface)
                    {
                        cands.push(e.surface.clone());
                    }
                }
            }
        }
        let composed = self.compose_kanji_sentence(&full_hira);
        if !composed.is_empty()
            && composed != full_hira
            && !cands.contains(&composed)
            && Self::lookup_english_learner_bridge(&lower_raw).is_empty()
            && Self::lookup_english_learner_bridge(&full_hira).is_empty()
        {
            cands.insert(0, composed.clone());
        }
        if composed.contains("貴方") {
            let anata_kana_variant = composed.replace("貴方", "あなた");
            if !cands.contains(&anata_kana_variant) {
                let insert_pos = cands.len().min(1);
                cands.insert(insert_pos, anata_kana_variant);
            }
        }
        if full_hira.ends_with('わ') && full_hira.chars().count() >= 2 {
            let mut hira_ha: Vec<char> = full_hira.chars().collect();
            if let Some(last) = hira_ha.last_mut() {
                *last = 'は';
            }
            let hira_ha_str: String = hira_ha.into_iter().collect();
            if let Some(entries) = self.lexicon.get(&hira_ha_str) {
                for e in entries {
                    if !cands.contains(&e.surface) {
                        cands.push(e.surface.clone());
                    }
                }
            }
            if !cands.contains(&hira_ha_str) {
                cands.push(hira_ha_str);
            }
        }
        if !full_hira.is_empty() && !cands.contains(&full_hira) {
            cands.push(full_hira);
        }
        if !full_kata.is_empty() && !cands.contains(&full_kata) {
            cands.push(full_kata);
        }
        if !cands.contains(&romaji.to_string()) {
            cands.push(romaji.to_string());
        }
        cands
    }

    /// 依畫面已顯示之日文漢字／假名／英文橋接詞反查同音候選與平／片假名變體（支援含助詞之文節如「私は」「橋を」）
    pub fn lookup_down_candidates_for_surface(&self, surface: &str) -> Vec<String> {
        if surface.is_empty() {
            return Vec::new();
        }
        let mut cands = Vec::new();
        let lower_surf = surface.to_ascii_lowercase();
        for &bridged in Self::lookup_english_learner_bridge(&lower_surf) {
            if !cands.contains(&bridged.to_string()) {
                cands.push(bridged.to_string());
            }
        }
        for &bridged in Self::lookup_english_learner_bridge(surface) {
            if !cands.contains(&bridged.to_string()) {
                cands.push(bridged.to_string());
            }
        }
        if let Some(entries) = self.lexicon.get(surface) {
            for e in entries {
                if !cands.contains(&e.surface) {
                    cands.push(e.surface.clone());
                }
            }
        }
        for (reading, entries) in &self.lexicon {
            if entries.iter().any(|e| e.surface == surface) {
                for e in entries {
                    if !cands.contains(&e.surface) {
                        cands.push(e.surface.clone());
                    }
                }
                if !cands.contains(reading) {
                    cands.push(reading.clone());
                }
                let kata = Self::hira_to_kata(reading);
                if !cands.contains(&kata) {
                    cands.push(kata);
                }
            }
        }
        // 若表層字串為「體言 + 格助詞／主題助詞」（如「私は」「橋を」「日本語が」），拆解詞幹反查同音異字與假名候選並保留助詞
        let surf_chars: Vec<char> = surface.chars().collect();
        if surf_chars.len() >= 2 {
            let last_ch = surf_chars[surf_chars.len() - 1];
            if matches!(last_ch, 'は' | 'が' | 'を' | 'に' | 'へ' | 'で' | 'と' | 'も' | 'の') {
                let stem: String = surf_chars[..surf_chars.len() - 1].iter().collect();
                for (reading, entries) in &self.lexicon {
                    if entries.iter().any(|e| e.surface == stem) || reading == &stem {
                        for e in entries {
                            let with_p = format!("{}{}", e.surface, last_ch);
                            if !cands.contains(&with_p) {
                                cands.push(with_p);
                            }
                        }
                        let hira_p = format!("{}{}", reading, last_ch);
                        if !cands.contains(&hira_p) {
                            cands.push(hira_p);
                        }
                        let kata_p = Self::hira_to_kata(&format!("{}{}", reading, last_ch));
                        if !cands.contains(&kata_p) {
                            cands.push(kata_p);
                        }
                    }
                }
            }
        }
        let kata_surf = Self::hira_to_kata(surface);
        if surface.contains("貴方") {
            let anata_variant = surface.replace("貴方", "あなた");
            if !cands.contains(&anata_variant) {
                cands.push(anata_variant);
            }
        }
        if kata_surf != surface && !cands.contains(&kata_surf) {
            cands.push(kata_surf);
        }
        if !cands.is_empty() && !cands.contains(&surface.to_string()) {
            cands.push(surface.to_string());
        }
        cands
    }

    /// 將日文平假名正規化為「無長音、無促音、清濁音歸一」之學習者音韻骨架，用於 `↑` 上方向鍵模糊救援
    fn learner_phonetic_skeleton(hira: &str) -> String {
        let mut skel = String::new();
        let chars: Vec<char> = hira.chars().collect();
        for (idx, &ch) in chars.iter().enumerate() {
            // 忽略促音「っ」與長音符「ー」
            if ch == 'っ' || ch == 'ー' {
                continue;
            }
            // 忽略接在 o 段或 u 段假名後的長音「う」或「お」（如 とうきょう / とおきょお / ときょ 皆歸一為 ときょ）
            if (ch == 'う' || ch == 'お') && idx > 0 {
                let prev = chars[idx - 1];
                if matches!(
                    prev,
                    'お' | 'こ' | 'ご' | 'そ' | 'ぞ' | 'と' | 'ど' | 'の' | 'ほ' | 'ぼ' | 'ぽ'
                        | 'も' | 'よ' | 'ろ' | 'ょ' | 'う' | 'く' | 'ぐ' | 'す' | 'ず' | 'つ'
                        | 'ぬ' | 'ふ' | 'ぶ' | 'ぷ' | 'む' | 'ゆ' | 'る' | 'ゅ'
                ) {
                    continue;
                }
            }
            // 忽略接在 e 段假名後的長音「い」或「え」（如 せんせい / せんせえ / せんせ 皆歸一為 せんせ）
            if (ch == 'い' || ch == 'え') && idx > 0 {
                let prev = chars[idx - 1];
                if matches!(
                    prev,
                    'え' | 'け' | 'げ' | 'せ' | 'ぜ' | 'て' | 'で' | 'ね' | 'へ' | 'べ' | 'ぺ' | 'め' | 'れ'
                ) {
                    continue;
                }
            }
            // 清濁音歸一化（如 だいじょうぶ vs たいじょうぶ）
            let norm_ch = match ch {
                'が' => 'か', 'ぎ' => 'き', 'ぐ' => 'く', 'げ' => 'け', 'ご' => 'こ',
                'ざ' => 'さ', 'じ' => 'し', 'ず' => 'す', 'ぜ' => 'せ', 'ぞ' => 'そ',
                'だ' => 'た', 'ぢ' => 'ち', 'づ' => 'つ', 'で' => 'て', 'ど' => 'と',
                'ば' | 'ぱ' => 'は', 'び' | 'ぴ' => 'ひ', 'ぶ' | 'ぷ' => 'ふ',
                'べ' | 'ぺ' => 'へ', 'ぼ' | 'ぽ' => 'ほ',
                _ => ch,
            };
            skel.push(norm_ch);
        }
        skel
    }

    /// 計算兩組假名序列之編輯距離是否 <= 1（支援單假名替換如 ようりし ➔ よろしく、或漏打撥音如 にほご ➔ にほんご）
    fn is_kana_edit_distance_le_1(a: &[char], b: &[char]) -> bool {
        let la = a.len();
        let lb = b.len();
        if la.abs_diff(lb) > 1 {
            return false;
        }
        if la == lb {
            let diffs = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
            return diffs <= 1;
        }
        let (shorter, longer) = if la < lb { (a, b) } else { (b, a) };
        let mut i = 0;
        let mut j = 0;
        let mut skipped = false;
        while i < shorter.len() && j < longer.len() {
            if shorter[i] == longer[j] {
                i += 1;
                j += 1;
            } else if !skipped {
                skipped = true;
                j += 1;
            } else {
                return false;
            }
        }
        true
    }

    /// `↑` 上箭頭：依指定羅馬字串進行日文前綴預測補全、英日橋接與近音模糊救援
    pub fn lookup_up_predictions_for_romaji(&self, romaji: &str) -> Vec<String> {
        if romaji.is_empty() {
            return Vec::new();
        }
        let mut res = Vec::new();
        let lower_raw = romaji.to_ascii_lowercase();
        for &bridged in Self::lookup_english_learner_bridge(&lower_raw) {
            if !res.contains(&bridged.to_string()) {
                res.push(bridged.to_string());
            }
        }
        let (hira, pending) = Self::convert_romaji_stream(romaji);
        let full_hira = if pending == "n" {
            format!("{}ん", hira)
        } else if pending.is_empty() {
            hira
        } else {
            format!("{}{}", hira, pending)
        };
        for w in self.lookup_up_predictions_for_hira(&full_hira) {
            if !res.contains(&w) {
                res.push(w);
            }
        }
        res
    }

    pub fn lookup_up_predictions_for_hira(&self, full_hira: &str) -> Vec<String> {
        if full_hira.is_empty() {
            return Vec::new();
        }
        let mut res = Vec::new();
        let lower_hira = full_hira.to_ascii_lowercase();
        for &bridged in Self::lookup_english_learner_bridge(&lower_hira) {
            if !res.contains(&bridged.to_string()) {
                res.push(bridged.to_string());
            }
        }
        for &bridged in Self::lookup_english_learner_bridge(full_hira) {
            if !res.contains(&bridged.to_string()) {
                res.push(bridged.to_string());
            }
        }

        let mut matches: Vec<(String, u32, usize)> = Vec::new();
        for (reading, entries) in &self.lexicon {
            if reading.starts_with(full_hira) && reading.chars().count() >= full_hira.chars().count() {
                for e in entries {
                    matches.push((e.surface.clone(), e.weight, reading.chars().count()));
                }
            }
        }
        matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        for (surf, _, _) in matches {
            if !res.contains(&surf) {
                res.push(surf);
            }
            if res.len() >= 12 {
                break;
            }
        }

        // 學習者音韻盲點模糊救援（長音 ou/oo/o 漏打、促音漏打、清濁音混淆、以及單假名近音誤植如 ようりし / にほご）
        let input_skel = Self::learner_phonetic_skeleton(full_hira);
        let input_skel_chars: Vec<char> = input_skel.chars().collect();
        if !input_skel_chars.is_empty() && res.len() < 12 {
            let mut fuzzy_matches: Vec<(String, u32, usize)> = Vec::new();
            for (reading, entries) in &self.lexicon {
                let read_skel = Self::learner_phonetic_skeleton(reading);
                let read_skel_chars: Vec<char> = read_skel.chars().collect();
                let exact_skel = read_skel == input_skel;
                let prefix_skel = input_skel_chars.len() >= 3 && read_skel.starts_with(&input_skel);
                let edit1_skel = input_skel_chars.len() >= 3
                    && read_skel_chars.first() == input_skel_chars.first()
                    && (Self::is_kana_edit_distance_le_1(&input_skel_chars, &read_skel_chars)
                        || (read_skel_chars.len() >= input_skel_chars.len()
                            && Self::is_kana_edit_distance_le_1(
                                &input_skel_chars,
                                &read_skel_chars[..input_skel_chars.len()],
                            )));
                if exact_skel || prefix_skel || edit1_skel {
                    for e in entries {
                        let bonus = if exact_skel {
                            300
                        } else if prefix_skel {
                            200
                        } else {
                            100
                        };
                        fuzzy_matches.push((e.surface.clone(), e.weight + bonus, reading.chars().count()));
                    }
                }
            }
            fuzzy_matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
            for (surf, _, _) in fuzzy_matches {
                if !res.contains(&surf) {
                    res.push(surf);
                }
                if res.len() >= 12 {
                    break;
                }
            }
        }

        let kata = Self::hira_to_kata(full_hira);
        if !res.contains(&full_hira.to_string()) {
            res.push(full_hira.to_string());
        }
        if !res.contains(&kata) {
            res.push(kata);
        }
        res
    }

    /// `↑` 上箭頭：日文前綴預測補全與長詞展開（如輸入 `ariga` + `↑` ➔ `ありがとう`, `ありがとうございます`）
    pub fn lookup_up_arrow_predictions(&self) -> Vec<String> {
        let full_hira = self.finalized_hira();
        self.lookup_up_predictions_for_hira(&full_hira)
    }

    pub fn make_preedit(&self) -> String {
        if self.raw_romaji.is_empty() {
            return String::new();
        }
        if self.is_english_forced {
            return self.raw_romaji.clone();
        }
        format!("{}{}", self.hira_buffer, self.pending_romaji)
    }

    /// Shift + Backspace：刪除最後一個假名音節（含拗音小假名 ゃ/ゅ/ょ/ぁ/ぃ/ぅ/ぇ/ぉ）或未決子音尾碼
    pub fn pop_syllable(&mut self) -> EngineResponse {
        if self.raw_romaji.is_empty() {
            return EngineResponse {
                action: "pass".to_string(),
                mode: "japanese".to_string(),
                preedit: String::new(),
                composed: String::new(),
                candidates: Vec::new(),
                active_candidate: String::new(),
                candidate_idx: 0,
                cursor_pos: 0,
                q_press_count: 0,
                committed_chunk: String::new(),
            };
        }
        if self.is_english_forced {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "japanese".to_string(),
                preedit: String::new(),
                composed: String::new(),
                candidates: Vec::new(),
                active_candidate: String::new(),
                candidate_idx: 0,
                cursor_pos: 0,
                q_press_count: 0,
                committed_chunk: String::new(),
            };
        }
        let (init_hira, init_pend) = Self::convert_romaji_stream(&self.raw_romaji);
        let hira_chars: Vec<char> = init_hira.chars().collect();
        let target_hira_len = if !init_pend.is_empty() {
            hira_chars.len()
        } else if hira_chars.is_empty() {
            0
        } else {
            let last_ch = hira_chars[hira_chars.len() - 1];
            let drop_cnt = if hira_chars.len() >= 2
                && matches!(last_ch, 'ゃ' | 'ゅ' | 'ょ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ')
            {
                2
            } else {
                1
            };
            hira_chars.len().saturating_sub(drop_cnt)
        };
        let target_hira: String = hira_chars[..target_hira_len].iter().collect();
        while !self.raw_romaji.is_empty() {
            self.raw_romaji.pop();
            let (h, p) = Self::convert_romaji_stream(&self.raw_romaji);
            if (p == "n" || p == "m") && format!("{}ん", h) == target_hira {
                self.raw_romaji.pop();
                self.raw_romaji.push_str("nn");
                break;
            }
            if h.chars().count() <= target_hira_len && p.is_empty() {
                break;
            }
        }
        if self.raw_romaji.is_empty() {
            self.reset();
            return EngineResponse {
                action: "reset".to_string(),
                mode: "japanese".to_string(),
                preedit: String::new(),
                composed: String::new(),
                candidates: Vec::new(),
                active_candidate: String::new(),
                candidate_idx: 0,
                cursor_pos: 0,
                q_press_count: 0,
                committed_chunk: String::new(),
            };
        }
        self.update_candidates();
        let preedit = self.make_preedit();
        let active = self.candidates.first().cloned().unwrap_or_default();
        let cur_len = active.chars().count();
        EngineResponse {
            action: "compose".to_string(),
            mode: "japanese".to_string(),
            preedit,
            composed: active.clone(),
            candidates: self.candidates.clone(),
            active_candidate: active,
            candidate_idx: 0,
            cursor_pos: cur_len,
            q_press_count: 0,
            committed_chunk: String::new(),
        }
    }

    /// Ctrl + Backspace：清空當前正在組字中的整組日文語節緩衝區
    pub fn pop_word(&mut self) -> EngineResponse {
        if self.raw_romaji.is_empty() {
            return EngineResponse {
                action: "pass".to_string(),
                mode: "japanese".to_string(),
                preedit: String::new(),
                composed: String::new(),
                candidates: Vec::new(),
                active_candidate: String::new(),
                candidate_idx: 0,
                cursor_pos: 0,
                q_press_count: 0,
                committed_chunk: String::new(),
            };
        }
        self.reset();
        EngineResponse {
            action: "reset".to_string(),
            mode: "japanese".to_string(),
            preedit: String::new(),
            composed: String::new(),
            candidates: Vec::new(),
            active_candidate: String::new(),
            candidate_idx: 0,
            cursor_pos: 0,
            q_press_count: 0,
            committed_chunk: String::new(),
        }
    }

    pub fn process_key(&mut self, ch: char) -> EngineResponse {
        match ch {
            ' ' => {
                if self.raw_romaji.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: " ".to_string(),
                    };
                }
                self.update_candidates();
                let commit_text = self
                    .candidates
                    .get(self.candidate_idx)
                    .cloned()
                    .unwrap_or_else(|| self.finalized_hira());
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: commit_text,
                }
            }
            '\n' | '\r' => {
                if self.raw_romaji.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                self.update_candidates();
                let commit_text = if self.is_english_forced {
                    self.raw_romaji.clone()
                } else {
                    self.candidates
                        .get(self.candidate_idx)
                        .cloned()
                        .unwrap_or_else(|| self.finalized_hira())
                };
                self.reset();
                EngineResponse {
                    action: "commit".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: commit_text,
                }
            }
            '\x08' => {
                if self.raw_romaji.is_empty() {
                    return EngineResponse {
                        action: "pass".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                self.raw_romaji.pop();
                if self.raw_romaji.is_empty() {
                    self.reset();
                    return EngineResponse {
                        action: "reset".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                self.update_candidates();
                let preedit = self.make_preedit();
                let active = self.candidates.first().cloned().unwrap_or_default();
                let cur_len = active.chars().count();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: "japanese".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            '\x1b' => {
                self.reset();
                EngineResponse {
                    action: "reset".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            '1'..='9' if !self.raw_romaji.is_empty() => {
                let idx = (ch as u8 - b'1') as usize;
                if idx < self.candidates.len() {
                    let commit_text = self.candidates[idx].clone();
                    self.reset();
                    EngineResponse {
                        action: "commit".to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: commit_text,
                    }
                } else {
                    let preedit = self.make_preedit();
                    let active = self.candidates.first().cloned().unwrap_or_default();
                    let cur_len = active.chars().count();
                    EngineResponse {
                        action: "compose".to_string(),
                        mode: "japanese".to_string(),
                        preedit,
                        composed: active.clone(),
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: self.candidate_idx,
                        cursor_pos: cur_len,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == '-' || (c == '\'' && !self.raw_romaji.is_empty()) => {
                if c.is_ascii_uppercase() {
                    self.is_english_forced = true;
                }
                self.raw_romaji.push(c);
                self.update_candidates();
                let preedit = self.make_preedit();
                let active = self.candidates.first().cloned().unwrap_or_default();
                let cur_len = active.chars().count();
                EngineResponse {
                    action: "compose".to_string(),
                    mode: "japanese".to_string(),
                    preedit,
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: 0,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            _ => {
                let punct = match ch {
                    ',' => "、",
                    '.' => "。",
                    '?' => "？",
                    '!' => "！",
                    '[' => "「",
                    ']' => "」",
                    '(' => "（",
                    ')' => "）",
                    ':' => "：",
                    ';' => "；",
                    _ => "",
                };
                let act = if !punct.is_empty() { "commit_punctuation" } else { "commit" };
                if self.raw_romaji.is_empty() {
                    let out = if !punct.is_empty() { punct.to_string() } else { ch.to_string() };
                    EngineResponse {
                        action: act.to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: out,
                    }
                } else {
                    self.update_candidates();
                    let mut base = self
                        .candidates
                        .get(self.candidate_idx)
                        .cloned()
                        .unwrap_or_else(|| self.finalized_hira());
                    // 若學習者按下問號 `?`，且句尾將疑問助詞「か」誤打為「が」（如「ですが？」「ますが？」「でしたか？」），自動歸一為「ですか？」「ますか？」
                    if ch == '?' {
                        if base.ends_with("ですが") {
                            base.truncate(base.len() - "が".len());
                            base.push('か');
                        } else if base.ends_with("ますが") {
                            base.truncate(base.len() - "が".len());
                            base.push('か');
                        } else if base.ends_with("でしたか") {
                            // already normalized
                        } else if base.ends_with("でしだが") || base.ends_with("でしたか") {
                            let trim_len = if base.ends_with("でしだが") { "でしだが".len() } else { "でしたか".len() };
                            base.truncate(base.len() - trim_len);
                            base.push_str("でしたか");
                        } else if base.ends_with("ましだが") {
                            base.truncate(base.len() - "ましだが".len());
                            base.push_str("ましたか");
                        }
                    }
                    let tail = if !punct.is_empty() { punct.to_string() } else { ch.to_string() };
                    self.reset();
                    EngineResponse {
                        action: act.to_string(),
                        mode: "japanese".to_string(),
                        preedit: String::new(),
                        composed: String::new(),
                        candidates: Vec::new(),
                        active_candidate: String::new(),
                        candidate_idx: 0,
                        cursor_pos: 0,
                        q_press_count: 0,
                        committed_chunk: format!("{}{}", base, tail),
                    }
                }
            }
        }
    }

    pub fn process_key_str(&mut self, key_str: &str) -> EngineResponse {
        match key_str {
            "Space" => self.process_key(' '),
            "Return" | "Enter" => self.process_key('\n'),
            "BackSpace" | "backspace" | "\x08" => self.process_key('\x08'),
            "Shift+BackSpace" | "Shift+backspace" => self.pop_syllable(),
            "Ctrl+BackSpace" | "Ctrl+backspace" => self.pop_word(),
            "Escape" | "Esc" => self.process_key('\x1b'),
            "Down" | "down" => {
                if !self.raw_romaji.is_empty() {
                    self.update_candidates();
                    let active = self.candidates.first().cloned().unwrap_or_default();
                    let cur_len = active.chars().count();
                    return EngineResponse {
                        action: "show_candidate_popup".to_string(),
                        mode: "candidates".to_string(),
                        preedit: self.make_preedit(),
                        composed: active.clone(),
                        candidates: self.candidates.clone(),
                        active_candidate: active,
                        candidate_idx: self.candidate_idx,
                        cursor_pos: cur_len,
                        q_press_count: 0,
                        committed_chunk: String::new(),
                    };
                }
                EngineResponse {
                    action: "none".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            "Up" | "up" => {
                if !self.raw_romaji.is_empty() {
                    let preds = self.lookup_up_arrow_predictions();
                    if !preds.is_empty() {
                        self.candidates = preds.clone();
                        self.candidate_idx = 0;
                        let first = preds[0].clone();
                        let cur_len = first.chars().count();
                        return EngineResponse {
                            action: "show_candidate_popup".to_string(),
                            mode: "candidates".to_string(),
                            preedit: self.make_preedit(),
                            composed: first.clone(),
                            candidates: preds,
                            active_candidate: first,
                            candidate_idx: 0,
                            cursor_pos: cur_len,
                            q_press_count: 0,
                            committed_chunk: String::new(),
                        };
                    }
                }
                EngineResponse {
                    action: "none".to_string(),
                    mode: "japanese".to_string(),
                    preedit: String::new(),
                    composed: String::new(),
                    candidates: Vec::new(),
                    active_candidate: String::new(),
                    candidate_idx: 0,
                    cursor_pos: 0,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
            s if s.chars().count() == 1 => {
                let ch = s.chars().next().unwrap();
                self.process_key(ch)
            }
            _ => {
                let active = self.candidates.first().cloned().unwrap_or_default();
                let cur_len = active.chars().count();
                EngineResponse {
                    action: "none".to_string(),
                    mode: "japanese".to_string(),
                    preedit: self.make_preedit(),
                    composed: active.clone(),
                    candidates: self.candidates.clone(),
                    active_candidate: active,
                    candidate_idx: self.candidate_idx,
                    cursor_pos: cur_len,
                    q_press_count: 0,
                    committed_chunk: String::new(),
                }
            }
        }
    }

    fn load_builtin_lexicon(&mut self) {
        const BUILTIN: &[(&str, &str, u32)] = &[
            // 常用寒暄與慣用句（含學習者發音拼法 konnichiwa / konbanwa 歸一）
            ("ありがとう", "ありがとう", 1650),
            ("ありがとう", "有難う", 1450),
            ("ありがとうございます", "ありがとうございます", 1700),
            ("こんにちは", "こんにちは", 1650),
            ("こんにちは", "今日は", 1400),
            ("こんにちわ", "こんにちは", 1650),
            ("こんばんは", "こんばんは", 1650),
            ("こんばんは", "今晩は", 1400),
            ("こんばんわ", "こんばんは", 1650),
            ("おはよう", "おはよう", 1650),
            ("おはようございます", "おはようございます", 1700),
            ("すみません", "すみません", 1650),
            ("ごめんなさい", "ごめんなさい", 1600),
            ("よろしく", "よろしく", 1600),
            ("よろしくおねがいします", "よろしくお願いします", 1750),
            ("おねがい", "お願い", 1600),
            ("おねがいします", "お願いします", 1700),
            ("おげんきですか", "お元気ですか", 1650),
            ("げんき", "元気", 1600),
            ("だいじょうぶ", "大丈夫", 1650),
            ("なるほど", "なるほど", 1600),
            ("おつかれさま", "お疲れ様", 1650),
            ("おつかれさまでした", "お疲れ様でした", 1700),
            ("いただきます", "いただきます", 1650),
            ("ごちそうさまでした", "ご馳走様でした", 1650),
            ("はじめまして", "初めまして", 1650),
            ("さようなら", "さようなら", 1600),
            ("おやすみ", "おやすみ", 1600),
            ("おやすみなさい", "おやすみなさい", 1650),
            // 句尾助動詞與常用語氣機能詞（防止長句尾碼誤切）
            ("です", "です", 1680),
            ("でした", "でした", 1680),
            ("ですか", "ですか", 1680),
            ("ですね", "ですね", 1680),
            ("ですよ", "ですよ", 1680),
            ("ください", "ください", 1680),
            // 代名詞、指示詞與人稱
            ("わたし", "私", 1700),
            ("わたしは", "私は", 1720),
            ("ぼく", "僕", 1600),
            ("ぼくは", "僕は", 1620),
            ("おれ", "俺", 1600),
            ("おれは", "俺は", 1620),
            ("あなた", "貴方", 1660),
            ("あなた", "あなた", 1640),
            ("あなたは", "貴方は", 1680),
            ("あなたは", "あなたは", 1660),
            ("きみ", "君", 1600),
            ("きみは", "君は", 1620),
            ("かれ", "彼", 1600),
            ("かれは", "彼は", 1620),
            ("かのじょ", "彼女", 1620),
            ("かのじょは", "彼女は", 1640),
            ("これ", "これ", 1650),
            ("これは", "これは", 1680),
            ("それ", "それ", 1650),
            ("それは", "それは", 1680),
            ("あれ", "あれ", 1650),
            ("あれは", "あれは", 1680),
            ("ここ", "ここ", 1650),
            ("ここは", "ここは", 1680),
            ("そこ", "そこ", 1650),
            ("あそこ", "あそこ", 1650),
            ("どこ", "どこ", 1650),
            ("だれ", "誰", 1650),
            ("なに", "何", 1650),
            ("なん", "何", 1620),
            ("どなた", "どなた", 1640),
            ("いくら", "いくら", 1650),
            ("いくつ", "いくつ", 1640),
            ("みんな", "皆", 1600),
            ("みなさん", "皆さん", 1650),
            ("せんせい", "先生", 1660),
            ("がくせい", "学生", 1640),
            ("ともだち", "友達", 1650),
            ("かぞく", "家族", 1650),
            ("おとうさん", "お父さん", 1650),
            ("おかあさん", "お母さん", 1650),
            ("おにいさん", "お兄さん", 1640),
            ("おねえさん", "お姉さん", 1640),
            ("こども", "子供", 1640),
            ("おとな", "大人", 1640),
            ("せんぱい", "先輩", 1650),
            ("こうはい", "後輩", 1640),
            ("かいしゃいん", "会社員", 1640),
            ("てんいん", "店員", 1640),
            ("いしゃ", "医者", 1640),
            ("ひとり", "一人", 1620),
            ("ふたり", "二人", 1620),
            // 國家、城市、語言與地標
            ("にほん", "日本", 1700),
            ("にっぽん", "日本", 1650),
            ("にほんじん", "日本人", 1680),
            ("にほんご", "日本語", 1700),
            ("たいわん", "台湾", 1700),
            ("たいわんじん", "台湾人", 1700),
            ("たいぺい", "台北", 1650),
            ("とうきょう", "東京", 1700),
            ("とうきょ", "東京", 1640),
            ("ときょう", "東京", 1640),
            ("ときょ", "東京", 1620),
            ("おおさか", "大阪", 1650),
            ("おさか", "大阪", 1620),
            ("きょうと", "京都", 1650),
            ("きょと", "京都", 1620),
            ("ほっかいどう", "北海道", 1650),
            ("ほっかいど", "北海道", 1620),
            ("おきなわ", "沖縄", 1600),
            ("ふじさん", "富士山", 1650),
            ("せかい", "世界", 1650),
            ("えいご", "英語", 1650),
            ("ちゅうごくご", "中国語", 1650),
            ("かんこくご", "韓国語", 1650),
            ("がいこくじん", "外国人", 1620),
            // 時間與日常名詞
            ("いま", "今", 1600),
            ("きょう", "今日", 1650),
            ("あした", "明日", 1650),
            ("きのう", "昨日", 1650),
            ("まいにち", "毎日", 1600),
            ("まいあさ", "毎朝", 1600),
            ("まいばん", "毎晩", 1600),
            ("しゅうまつ", "週末", 1600),
            ("じかん", "時間", 1650),
            ("でんしゃ", "電車", 1650),
            ("しんかんせん", "新幹線", 1700),
            ("ちかてつ", "地下鉄", 1650),
            ("ひこうき", "飛行機", 1650),
            ("じてんしゃ", "自転車", 1620),
            ("くるま", "車", 1600),
            ("えき", "駅", 1600),
            ("くうこう", "空港", 1600),
            ("がっこう", "学校", 1650),
            ("だいがく", "大学", 1650),
            ("かいしゃ", "会社", 1650),
            ("しごと", "仕事", 1650),
            ("でんわ", "電話", 1600),
            ("りょこう", "旅行", 1650),
            ("たべもの", "食べ物", 1650),
            ("のみもの", "飲み物", 1650),
            ("ごはん", "ご飯", 1650),
            ("あさごはん", "朝ご飯", 1650),
            ("ひるごはん", "昼ご飯", 1650),
            ("ばんごはん", "晩ご飯", 1650),
            ("おべんとう", "お弁当", 1620),
            ("おすし", "お寿司", 1650),
            ("すし", "寿司", 1640),
            ("さしみ", "刺身", 1640),
            ("てんぷら", "天ぷら", 1650),
            ("やきにく", "焼肉", 1640),
            ("しんぶん", "新聞", 1650),
            ("えんぴつ", "鉛筆", 1640),
            ("しんぱい", "心配", 1650),
            ("あんしん", "安心", 1640),
            ("しあわせ", "幸せ", 1650),
            ("たんじょうび", "誕生日", 1650),
            ("おたんじょうび", "お誕生日", 1660),
            ("なつやすみ", "夏休み", 1640),
            ("ふゆやすみ", "冬休み", 1640),
            ("おんせん", "温泉", 1650),
            ("りょかん", "旅館", 1640),
            ("じんじゃ", "神社", 1640),
            ("おてら", "お寺", 1640),
            ("おちゃ", "お茶", 1600),
            ("おみず", "お水", 1600),
            ("みず", "水", 1550),
            ("おさけ", "お酒", 1600),
            ("おかね", "お金", 1620),
            ("おみやげ", "お土産", 1620),
            ("おてあらい", "お手洗い", 1620),
            ("おかいけい", "お会計", 1620),
            ("ほん", "本", 1550),
            ("おんがく", "音楽", 1600),
            ("えいが", "映画", 1600),
            ("しゃしん", "写真", 1600),
            ("てんき", "天気", 1600),
            ("こころ", "心", 1600),
            ("へいわ", "平和", 1600),
            ("じゆう", "自由", 1600),
            ("きぼう", "希望", 1600),
            ("みらい", "未来", 1600),
            ("れきし", "歴史", 1600),
            ("ぶんか", "文化", 1600),
            ("かがく", "科学", 1600),
            ("かがく", "化学", 1580),
            ("ぎじゅつ", "技術", 1600),
            ("はし", "橋", 1580),
            ("はし", "箸", 1560),
            ("いえ", "家", 1600),
            ("へや", "部屋", 1600),
            ("みせ", "店", 1600),
            ("くに", "国", 1600),
            ("まち", "町", 1600),
            ("びょういん", "病院", 1620),
            ("ぎんこう", "銀行", 1620),
            ("ゆうびんきょく", "郵便局", 1620),
            ("としょかん", "図書館", 1620),
            ("こうえん", "公園", 1620),
            ("なまえ", "名前", 1620),
            ("ことば", "言葉", 1620),
            ("いみ", "意味", 1620),
            ("てがみ", "手紙", 1600),
            ("べんきょう", "勉強", 1680),
            ("べんきょ", "勉強", 1640),
            ("れんしゅう", "練習", 1650),
            ("れんしゅ", "練習", 1610),
            ("しつもん", "質問", 1650),
            ("せつめい", "説明", 1650),
            ("じゅんび", "準備", 1650),
            ("よやく", "予約", 1650),
            ("りょうり", "料理", 1650),
            ("さんぽ", "散歩", 1620),
            ("かいもの", "買い物", 1650),
            ("びょうき", "病気", 1620),
            ("きっぷ", "切符", 1620),
            ("ちょっと", "ちょっと", 1650),
            ("とても", "とても", 1650),
            ("ほんとう", "本当", 1650),
            ("ほんとうに", "本当に", 1660),
            ("いっしょ", "一緒", 1640),
            ("いっしょに", "一緒に", 1660),
            ("たくさん", "沢山", 1620),
            ("すこし", "少し", 1620),
            ("はる", "春", 1550),
            ("なつ", "夏", 1550),
            ("あき", "秋", 1550),
            ("ふゆ", "冬", 1550),
            ("さくら", "桜", 1600),
            ("かぜ", "風", 1550),
            ("そら", "空", 1550),
            ("うみ", "海", 1550),
            ("やま", "山", 1550),
            ("かわ", "川", 1550),
            ("はな", "花", 1550),
            ("あめ", "雨", 1550),
            ("ゆき", "雪", 1550),
            ("ほし", "星", 1550),
            ("つき", "月", 1550),
            ("ひかり", "光", 1550),
            // 常用形容動詞（な形容詞）、形容詞與動詞
            ("すき", "好き", 1650),
            ("だいすき", "大好き", 1650),
            ("きらい", "嫌い", 1600),
            ("きれい", "綺麗", 1640),
            ("しずか", "静か", 1620),
            ("べんり", "便利", 1640),
            ("ゆうめい", "有名", 1640),
            ("かんたん", "簡単", 1640),
            ("たいへん", "大変", 1640),
            ("しんせつ", "親切", 1620),
            ("じょうず", "上手", 1640),
            ("へた", "下手", 1600),
            ("おいしい", "美味しい", 1650),
            ("かわいい", "可愛い", 1650),
            ("たのしい", "楽しい", 1650),
            ("うれしい", "嬉しい", 1650),
            ("むずかしい", "難しい", 1600),
            ("やさしい", "優しい", 1600),
            ("あたらしい", "新しい", 1600),
            ("おおきい", "大きい", 1600),
            ("ちいさい", "小さい", 1600),
            ("たべる", "食べる", 1600),
            ("のむ", "飲む", 1600),
            ("いく", "行く", 1600),
            ("くる", "来る", 1600),
            ("みる", "見る", 1600),
            ("みる", "観る", 1560),
            ("みる", "診る", 1540),
            ("きく", "聞く", 1600),
            ("きく", "聴く", 1560),
            ("はなす", "話す", 1600),
            ("よむ", "読む", 1600),
            ("かく", "書く", 1600),
            ("わかる", "分かる", 1600),
            ("おもう", "思う", 1600),
            // 常用片假名外來語
            ("こーひー", "コーヒー", 1700),
            ("らーめん", "ラーメン", 1700),
            ("ぱそこん", "パソコン", 1650),
            ("すまほ", "スマホ", 1650),
            ("ほてる", "ホテル", 1650),
            ("こんびに", "コンビニ", 1650),
            ("てれび", "テレビ", 1650),
            ("かめら", "カメラ", 1650),
            ("あにめ", "アニメ", 1650),
            ("げーむ", "ゲーム", 1650),
            ("といれ", "トイレ", 1650),
            ("ばす", "バス", 1640),
            ("たくしー", "タクシー", 1640),
            ("れすとらん", "レストラン", 1640),
            ("かふぇ", "カフェ", 1640),
            ("でぱーと", "デパート", 1640),
            ("すーぱー", "スーパー", 1640),
            ("めーる", "メール", 1640),
            ("にゅーす", "ニュース", 1640),
        ];
        for &(reading, surface, weight) in BUILTIN {
            self.add_lexicon_entry(reading, surface, weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_romaji_to_kana_and_kanji_composition() {
        let mut jp = JapaneseEngine::new();

        // 1. 撥音與促音自動機：nihongo ➔ 日本語 (にほんご)、gakkou ➔ 学校 (がっこう)、shinkansen ➔ 新幹線
        for ch in "nihongo".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.finalized_hira(), "にほんご");
        assert_eq!(jp.candidates[0], "日本語");
        assert!(jp.candidates.contains(&"にほんご".to_string()));
        assert!(jp.candidates.contains(&"ニホンゴ".to_string()));
        let r = jp.process_key(' ');
        assert_eq!(r.committed_chunk, "日本語");

        // 2. 連文節轉換：watashihanihongogasuki ➔ 私は日本語が好き
        for ch in "watashihanihongogasuki".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "私は日本語が好き");
        jp.reset();

        // 2b. 學習者發音拼法 (wa ➔ は, e ➔ へ, o ➔ を)：
        //     打完 watashiwa 尚未按 Space 即自動顯示為「私は」（絕非「私わ」！）
        for ch in "watashiwa".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "私は");
        assert!(jp.candidates.contains(&"わたしは".to_string()));
        jp.reset();

        for ch in "nihongowa".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "日本語は");
        jp.reset();

        for ch in "nihoneikimasu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "日本へ行きます");
        jp.reset();

        for ch in "watashiwanihongogasukidesu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "私は日本語が好きです");
        jp.reset();

        for ch in "watashiwanihongoobenkyoushimasu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "私は日本語を勉強します");
        jp.reset();

        for ch in "watashiwanihongoobenkyoshimasu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "私は日本語を勉強します");
        jp.reset();

        for ch in "ashitatoukyoueikimasu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "明日東京へ行きます");
        jp.reset();

        for ch in "ashitatoukyoeikimasu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "明日東京へ行きます");
        jp.reset();

        for ch in "honoyomimasu".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "本を読みます");
        jp.reset();

        // 3. 外來語長音與片假名：ko-hi- ➔ コーヒー
        for ch in "ko-hi-".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "コーヒー");
        jp.reset();

        // 4. 上箭頭 `↑` 前綴預測補全：ariga + Up ➔ ありがとうございます / ありがとう
        for ch in "ariga".chars() {
            jp.process_key(ch);
        }
        let up_resp = jp.process_key_str("Up");
        assert!(up_resp.candidates.contains(&"ありがとうございます".to_string()));
        assert!(up_resp.candidates.contains(&"ありがとう".to_string()));
    }

    #[test]
    fn test_learner_inflection_context_and_fuzzy_rescue() {
        let jp = JapaneseEngine::new();

        // 1. 用言活用語幹＋送り仮名自動成詞（一段動詞、五段動詞、サ變、い形容詞）
        assert_eq!(
            jp.compose_kanji_sentence("わたしはにほんごをべんきょうします"),
            "私は日本語を勉強します"
        );
        assert_eq!(
            jp.compose_kanji_sentence("きのうがっこうへいきませんでした"),
            "昨日学校へ行きませんでした"
        );
        assert_eq!(
            jp.compose_kanji_sentence("とてもおいしかったです"),
            "とても美味しかったです"
        );
        assert_eq!(
            jp.compose_kanji_sentence("ほんをよんでいます"),
            "本を読んでいます"
        );

        // 2. 格助詞與受詞語境消歧（映画を観る vs 病気を診る、音楽を聴く、写真を撮る、電車に乗る）
        assert_eq!(
            jp.compose_kanji_sentence("えいがをみました"),
            "映画を観ました"
        );
        assert_eq!(
            jp.compose_kanji_sentence("びょうきをみる"),
            "病気を診る"
        );
        assert_eq!(
            jp.compose_kanji_sentence("おんがくをききます"),
            "音楽を聴きます"
        );
        assert_eq!(
            jp.compose_kanji_sentence("しゃしんをとった"),
            "写真を撮った"
        );
        assert_eq!(
            jp.compose_kanji_sentence("でんしゃにのります"),
            "電車に乗ります"
        );

        // 3. 學習者音韻盲點 `↑` 模糊救援（長音漏打 tokyo/gakko/sensee、促音漏打 gakou、清濁混淆 taijoubu）
        let tokyo_rescue = jp.lookup_up_predictions_for_romaji("tokyo");
        assert!(
            tokyo_rescue.contains(&"東京".to_string()),
            "tokyo 漏打長音應透過 ↑ 救回『東京』，實際：{:?}",
            tokyo_rescue
        );
        let gakou_rescue = jp.lookup_up_predictions_for_romaji("gakou");
        assert!(
            gakou_rescue.contains(&"学校".to_string()),
            "gakou 漏打促音應透過 ↑ 救回『学校』，實際：{:?}",
            gakou_rescue
        );
        let sensee_rescue = jp.lookup_up_predictions_for_romaji("sensee");
        assert!(
            sensee_rescue.contains(&"先生".to_string()),
            "sensee 長音混淆應透過 ↑ 救回『先生』，實際：{:?}",
            sensee_rescue
        );
        let taijoubu_rescue = jp.lookup_up_predictions_for_romaji("taijoubu");
        assert!(
            taijoubu_rescue.contains(&"大丈夫".to_string()),
            "taijoubu 清濁音混淆應透過 ↑ 救回『大丈夫』，實際：{:?}",
            taijoubu_rescue
        );

        // 4. 學習者近音誤植與英日橋接救援（yourishi / ようりし ➔ よろしくお願いします、nihogo / にほご ➔ 日本語、thanks ➔ ありがとうございます）
        let yourishi_rescue = jp.lookup_up_predictions_for_romaji("yourishi");
        assert_eq!(yourishi_rescue.first().map(|s| s.as_str()), Some("よろしくお願いします"));
        let yourishi_hira_rescue = jp.lookup_up_predictions_for_hira("ようりし");
        assert_eq!(yourishi_hira_rescue.first().map(|s| s.as_str()), Some("よろしくお願いします"));
        let nihogo_rescue = jp.lookup_up_predictions_for_romaji("nihogo");
        assert!(nihogo_rescue.contains(&"日本語".to_string()));
        let thanks_down = jp.lookup_down_candidates_for_romaji("thanks");
        assert_eq!(thanks_down.first().map(|s| s.as_str()), Some("ありがとうございます"));
        let thanks_surf = jp.lookup_down_candidates_for_surface("thanks");
        assert_eq!(thanks_surf.first().map(|s| s.as_str()), Some("ありがとうございます"));
        let thanks_up = jp.lookup_up_predictions_for_romaji("thanks");
        assert_eq!(thanks_up.first().map(|s| s.as_str()), Some("ありがとうございます"));
    }

    #[test]
    fn test_foreign_learner_voicing_skeleton_and_interrogative_composition() {
        let mut jp = JapaneseEngine::new();
        let _ = jp.load_tsv("../ruhi_lexicon/data/japanese.tsv");

        // 1. 使用者回報案例：「あなだは誰ですが？」(anadawadaredesuga?)
        //    あなだ (anada 清濁音容錯) ➔ 貴方 / あなた
        //    わ (wa 主題助詞歸一) ➔ は
        //    だれ (dare) ➔ 誰
        //    ですが (desuga 疑問句尾清濁音容錯) ➔ ですか
        assert_eq!(
            jp.compose_kanji_sentence("あなだはだれですが"),
            "貴方は誰ですか"
        );
        assert_eq!(
            jp.compose_kanji_sentence("あなだわだれですが"),
            "貴方は誰ですか"
        );

        for ch in "anadawadaredesuga".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "貴方は誰ですか");
        assert!(
            jp.candidates.contains(&"あなたは誰ですか".to_string()),
            "候選清單應同步提供現代平假名表記『あなたは誰ですか』，實際：{:?}",
            jp.candidates
        );
        let q_resp = jp.process_key('?');
        assert_eq!(q_resp.committed_chunk, "貴方は誰ですか？");

        // 2. 其他常見外國學習者清濁音、長音、促音、赫本式 m 撥音連文節自動轉漢字：
        //    wadashiwanihogogasukidesu ➔ 私は日本語が好きです
        assert_eq!(
            jp.compose_kanji_sentence("わだしわにほごがすきです"),
            "私は日本語が好きです"
        );
        //    ashidagakoueikimasu (あしだがこうえいきます) ➔ 明日学校へ行きます
        assert_eq!(
            jp.compose_kanji_sentence("あしだがこうえいきます"),
            "明日学校へ行きます"
        );
        //    korewanandesuga (これはなんですか) ➔ これは何ですか
        assert_eq!(
            jp.compose_kanji_sentence("これわなんですか"),
            "これは何ですか"
        );
        //    赫本式 m 撥音 (shimbun / tempura / gambatte / sempai)
        for ch in "shimbun".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "新聞");
        jp.reset();

        for ch in "tempura".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "天ぷら");
        jp.reset();

        for ch in "gambatte".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "頑張って");
        jp.reset();

        for ch in "sempai".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.candidates[0], "先輩");
        jp.reset();

        // 3. 審核補修驗證：
        // (a) TSF 下方向鍵 `lookup_down_candidates_for_romaji` 與 `lookup_down_candidates_for_surface` 同步支援骨架同音與「あなた」假名變體
        let down_anada = jp.lookup_down_candidates_for_romaji("anada");
        assert!(down_anada.contains(&"貴方".to_string()));
        assert!(down_anada.contains(&"あなた".to_string()));
        let down_sentence = jp.lookup_down_candidates_for_romaji("anadawadaredesuga");
        assert_eq!(down_sentence.first().map(|s| s.as_str()), Some("貴方は誰ですか"));
        assert!(down_sentence.contains(&"あなたは誰ですか".to_string()));
        let surf_sentence = jp.lookup_down_candidates_for_surface("貴方は誰ですか");
        assert!(surf_sentence.contains(&"あなたは誰ですか".to_string()));

        // (b) い形容詞學習者漏打尾音「い」或促音「っ」（oishidesu ➔ 美味しいです、tanoshikatadesu ➔ 楽しかったです）
        assert_eq!(jp.compose_kanji_sentence("おいしです"), "美味しいです");
        assert_eq!(jp.compose_kanji_sentence("たのしかたです"), "楽しかったです");
        assert_eq!(jp.compose_kanji_sentence("むずかしですか"), "難しいですか");

        // (c) 2 假名精確名詞 + 格助詞邊界保護（不遭跨助詞模糊骨架誤吞）
        assert_eq!(jp.compose_kanji_sentence("はながきれいです"), "花が綺麗です");
        assert_eq!(jp.compose_kanji_sentence("えきにいきます"), "駅に行きます");

        // (d) Shift+BackSpace 與 Ctrl+BackSpace 驗證
        jp.reset();
        for ch in "nihongo".chars() {
            jp.process_key(ch);
        }
        assert_eq!(jp.hira_buffer, "にほんご");
        let r_shift = jp.process_key_str("Shift+BackSpace");
        assert_eq!(r_shift.preedit, "にほん");
        let r_ctrl = jp.process_key_str("Ctrl+BackSpace");
        assert_eq!(r_ctrl.action, "reset");
        assert!(!jp.has_buffer());
    }
}

