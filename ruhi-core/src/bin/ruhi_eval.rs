//! # `ruhi_eval` — 如喜輸入法 (RuhI) 大規模語料庫自動評估、消融實驗與結構化感知機校準工具
//!
//! 實作規範依據：`docs/RUHI_EVALUATION_AND_CALIBRATION_HANDOVER.md`
//! 支援指令：
//! - `cargo run --release --bin ruhi_eval -- --build-corpus`
//! - `cargo run --release --bin ruhi_eval -- --set dev --out eval_reports/dev_baseline.json`
//! - `cargo run --release --bin ruhi_eval -- --set dev --baseline eval_reports/dev_baseline.json`
//! - `cargo run --release --bin ruhi_eval -- --set test --out eval_reports/test_report.json`
//! - `cargo run --release --bin ruhi_eval -- --set dev --ablation --out eval_reports/ablation_report.json`
//! - `cargo run --release --bin ruhi_eval -- --set dev --calibrate --out eval_reports/calibration_report.json`

use ruhi::fsm::{HsuFSM, HsuSyllable};
use ruhi::lexicon::LexiconManager;
use ruhi::router::LatticeDAG;
use ruhi::RuhiEngine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 語料庫單筆紀錄 (`id\tsource_category\ttext`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusItem {
    pub id: String,
    pub category: String,
    pub text: String,
}

/// 單句編譯後之注音與許氏按鍵結構
#[derive(Debug, Clone)]
pub struct CompiledSentence {
    pub item: CorpusItem,
    pub syllables: Vec<HsuSyllable>,
    pub hsu_keys: String,
    pub has_polyphonic_single_char: bool,
}

/// 單句評估結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceEvalResult {
    pub id: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gold: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pred: Option<String>,
    pub char_len: usize,
    pub char_errors: usize,
    pub is_exact_match: bool,
    pub has_polyphonic_single_char: bool,
    pub is_gold_reachable: bool,
    pub latency_us: u64,
}

/// 分類別評估指標
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CategoryMetrics {
    pub sentences: usize,
    pub chars: usize,
    pub char_errors: usize,
    pub cer: f64,
    pub cer_percent: String,
    pub exact_match: usize,
    pub sentence_accuracy: f64,
    pub sentence_accuracy_percent: String,
}

/// 完整評估報告結構
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub dataset: String,
    pub total_sentences: usize,
    pub total_chars: usize,
    pub total_char_errors: usize,
    pub cer: f64,
    pub cer_percent: String,
    pub exact_match_sentences: usize,
    pub sentence_accuracy: f64,
    pub sentence_accuracy_percent: String,
    pub polyphonic_ambiguity_audit: Value,
    pub unreachable_gold_audit: Value,
    pub latency_us: Value,
    pub per_category: BTreeMap<String, CategoryMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcnemar_comparison: Option<Value>,
    pub sentence_results: Vec<SentenceEvalResult>,
}

/// 高歧義單字破音字表（當在句子中以未消歧之孤立單字出現時，標記為具上游 Text-to-Bopomofo 破音歧義）
const HIGH_AMBIGUITY_SINGLE_POLYPHONES: &str =
    "行長重還傳調彈種處數惡樂朝少將相為得地著了應當看難便更省率盛乾假背奇載轉差度露切落興號識稱給模曲會合解間好要見發正分過空教單";

/// 常見單字在孤立語境下之預設現代最常用讀音（避免字典順序偶發選中罕用破音）
fn preferred_single_char_bopomofo(ch: char) -> Option<&'static str> {
    match ch {
        '的' => Some("˙ㄉㄜ"),
        '地' => Some("ㄉㄧˋ"),
        '得' => Some("ㄉㄜˊ"),
        '了' => Some("˙ㄌㄜ"),
        '著' => Some("˙ㄓㄜ"),
        '過' => Some("ㄍㄨㄛˋ"),
        '嗎' => Some("˙ㄇㄚ"),
        '呢' => Some("˙ㄋㄜ"),
        '吧' => Some("˙ㄅㄚ"),
        '啊' => Some("˙ㄚ"),
        '呀' => Some("˙ㄧㄚ"),
        '為' => Some("ㄨㄟˋ"),
        '與' => Some("ㄩˇ"),
        '和' => Some("ㄏㄢˋ"),
        '還' => Some("ㄏㄞˊ"),
        '行' => Some("ㄒㄧㄥˊ"),
        '長' => Some("ㄔㄤˊ"),
        '重' => Some("ㄓㄨㄥˋ"),
        '種' => Some("ㄓㄨㄥˇ"),
        '處' => Some("ㄔㄨˋ"),
        '數' => Some("ㄕㄨˋ"),
        '樂' => Some("ㄌㄜˋ"),
        '少' => Some("ㄕㄠˇ"),
        '將' => Some("ㄐㄧㄤ"),
        '相' => Some("ㄒㄧㄤ"),
        '應' => Some("ㄧㄥ"),
        '當' => Some("ㄉㄤ"),
        '看' => Some("ㄎㄢˋ"),
        '難' => Some("ㄋㄢˊ"),
        '便' => Some("ㄅㄧㄢˋ"),
        '更' => Some("ㄍㄥˋ"),
        '省' => Some("ㄕㄥˇ"),
        '率' => Some("ㄌㄩˋ"),
        '盛' => Some("ㄕㄥˋ"),
        '乾' => Some("ㄍㄢ"),
        '假' => Some("ㄐㄧㄚˇ"),
        '背' => Some("ㄅㄟˋ"),
        '奇' => Some("ㄑㄧˊ"),
        '載' => Some("ㄗㄞˋ"),
        '轉' => Some("ㄓㄨㄢˇ"),
        '差' => Some("ㄔㄚ"),
        '度' => Some("ㄉㄨˋ"),
        '切' => Some("ㄑㄧㄝ"),
        '落' => Some("ㄌㄨㄛˋ"),
        '興' => Some("ㄒㄧㄥ"),
        '號' => Some("ㄏㄠˋ"),
        '識' => Some("ㄕˋ"),
        '稱' => Some("ㄔㄥ"),
        '給' => Some("ㄍㄟˇ"),
        '模' => Some("ㄇㄛˊ"),
        '曲' => Some("ㄑㄩˇ"),
        '會' => Some("ㄏㄨㄟˋ"),
        '合' => Some("ㄏㄜˊ"),
        '解' => Some("ㄐㄧㄝˇ"),
        '間' => Some("ㄐㄧㄢ"),
        '好' => Some("ㄏㄠˇ"),
        '要' => Some("ㄧㄠˋ"),
        '見' => Some("ㄐㄧㄢˋ"),
        '發' => Some("ㄈㄚ"),
        '正' => Some("ㄓㄥˋ"),
        '分' => Some("ㄈㄣ"),
        '空' => Some("ㄎㄨㄥ"),
        '教' => Some("ㄐㄧㄠ"),
        '單' => Some("ㄉㄢ"),
        '都' => Some("ㄉㄡ"),
        '只' => Some("ㄓˇ"),
        '那' => Some("ㄋㄚˋ"),
        '這' => Some("ㄓㄜˋ"),
        '誰' => Some("ㄕㄟˊ"),
        '什' => Some("ㄕㄣˊ"),
        '麼' => Some("˙ㄇㄜ"),
        '個' => Some("˙ㄍㄜ"),
        _ => None,
    }
}

/// 定位 `ruhi` 專案根目錄
pub fn resolve_ruhi_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(parent) = manifest.parent() {
        if parent.join("ruhi_lexicon").is_dir() {
            return parent.to_path_buf();
        }
    }
    PathBuf::from(r"C:\ATGprjs\ruhi")
}

/// 將單一 `HsuSyllable` 逆向轉換為許氏鍵盤按鍵序列
pub fn syllable_to_hsu_keys(syl: &HsuSyllable) -> Option<String> {
    let mut out = String::with_capacity(4);

    if syl.initial.is_empty() && syl.medial.is_empty() {
        // 零聲母無介母音節（如 ㄦ、ㄜ、ㄛ、ㄢ、ㄣ、ㄚ、ㄞ、ㄠ、ㄡ）
        let k = match syl.final_part.as_str() {
            "ㄦ" => 'l',
            "ㄜ" => 'g',
            "ㄛ" => 'h',
            "ㄢ" => 'm',
            "ㄣ" => 'n',
            "ㄚ" => 'y',
            "ㄞ" => 'i',
            "ㄠ" => 'w',
            "ㄡ" => 'o',
            "ㄝ" => 'e',
            "ㄟ" => 'a',
            "ㄤ" => 'k',
            "ㄥ" => 'l',
            _ => return None,
        };
        out.push(k);
    } else {
        if !syl.initial.is_empty() {
            let ik = match syl.initial.as_str() {
                "ㄅ" => 'b',
                "ㄆ" => 'p',
                "ㄇ" => 'm',
                "ㄈ" => 'f',
                "ㄉ" => 'd',
                "ㄊ" => 't',
                "ㄋ" => 'n',
                "ㄌ" => 'l',
                "ㄍ" => 'g',
                "ㄎ" => 'k',
                "ㄏ" => 'h',
                "ㄐ" | "ㄓ" => 'j',
                "ㄑ" | "ㄔ" => 'v',
                "ㄒ" | "ㄕ" => 'c',
                "ㄖ" => 'r',
                "ㄗ" => 'z',
                "ㄘ" => 'a',
                "ㄙ" => 's',
                _ => return None,
            };
            out.push(ik);
        }
        if !syl.medial.is_empty() {
            let mk = match syl.medial.as_str() {
                "ㄧ" => 'e',
                "ㄨ" => 'x',
                "ㄩ" => 'u',
                _ => return None,
            };
            out.push(mk);
        }
        if !syl.final_part.is_empty() {
            let fk = match syl.final_part.as_str() {
                "ㄚ" => 'y',
                "ㄛ" => 'h',
                "ㄜ" => 'g',
                "ㄝ" => 'e',
                "ㄞ" => 'i',
                "ㄟ" => 'a',
                "ㄠ" => 'w',
                "ㄡ" => 'o',
                "ㄢ" => 'm',
                "ㄣ" => 'n',
                "ㄤ" => 'k',
                "ㄥ" | "ㄦ" => 'l',
                _ => return None,
            };
            out.push(fk);
        }
    }

    let tk = match syl.tone {
        '1' => ' ',
        '2' => 'd',
        '3' => 'f',
        '4' => 'j',
        '5' => 's',
        _ => ' ',
    };
    out.push(tk);
    Some(out)
}

/// 建立詞級與單字標準注音索引（供 Text-to-Bopomofo 轉換與單字破音歧義檢測）
pub struct PhoneticCompiler {
    pub word_to_bpm: HashMap<String, Vec<String>>,
    pub char_to_bpm: HashMap<char, String>,
    pub polyphonic_chars: HashSet<char>,
}

impl PhoneticCompiler {
    pub fn from_lexicon(lexicon: &LexiconManager) -> Self {
        let mut word_best: HashMap<String, (u32, Vec<String>)> = HashMap::new();
        let mut char_best: HashMap<char, (u32, String)> = HashMap::new();
        let mut char_readings: HashMap<char, HashSet<String>> = HashMap::new();

        for entry in &lexicon.entries {
            if entry.category == "archaic" || entry.category == "classic" || entry.category == "rare" {
                continue;
            }
            let w = entry.word.trim();
            let ch_count = w.chars().count();
            if ch_count == 0 {
                continue;
            }
            let norm = ruhi::lexicon::LexiconEntry::normalize_bpm(&entry.bopomofo);
            let syls: Vec<String> = norm.split_whitespace().map(|s| s.to_string()).collect();
            if syls.len() != ch_count {
                continue;
            }
            // 確認每個音節皆可轉為 HsuSyllable
            if !syls.iter().all(|s| {
                HsuSyllable::from_bopomofo_display(s)
                    .and_then(|hs| syllable_to_hsu_keys(&hs))
                    .is_some()
            }) {
                continue;
            }

            if ch_count == 1 {
                let ch = w.chars().next().unwrap();
                if entry.weight >= 850 {
                    char_readings
                        .entry(ch)
                        .or_default()
                        .insert(syls[0].clone());
                }
                let replace = match char_best.get(&ch) {
                    Some((best_w, _)) => entry.weight > *best_w,
                    None => true,
                };
                if replace {
                    char_best.insert(ch, (entry.weight, syls[0].clone()));
                }
            } else if ch_count <= 8 {
                let replace = match word_best.get(w) {
                    Some((best_w, _)) => entry.weight > *best_w,
                    None => true,
                };
                if replace {
                    word_best.insert(w.to_string(), (entry.weight, syls));
                }
            }
        }

        let mut char_to_bpm: HashMap<char, String> = char_best
            .into_iter()
            .map(|(k, (_, v))| (k, v))
            .collect();
        for ch in HIGH_AMBIGUITY_SINGLE_POLYPHONES.chars() {
            if let Some(pref) = preferred_single_char_bopomofo(ch) {
                char_to_bpm.insert(ch, pref.to_string());
            }
        }
        for (ch, pref) in [
            ('的', "˙ㄉㄜ"),
            ('了', "˙ㄌㄜ"),
            ('著', "˙ㄓㄜ"),
            ('嗎', "˙ㄇㄚ"),
            ('呢', "˙ㄋㄜ"),
            ('吧', "˙ㄅㄚ"),
            ('個', "˙ㄍㄜ"),
        ] {
            char_to_bpm.insert(ch, pref.to_string());
        }

        let mut polyphonic_chars: HashSet<char> = HIGH_AMBIGUITY_SINGLE_POLYPHONES.chars().collect();
        for (ch, rset) in char_readings {
            if rset.len() >= 2 {
                polyphonic_chars.insert(ch);
            }
        }

        let word_to_bpm = word_best.into_iter().map(|(k, (_, v))| (k, v)).collect();

        Self {
            word_to_bpm,
            char_to_bpm,
            polyphonic_chars,
        }
    }

    /// 將目標句子轉換為注音音節序列與許氏按鍵串，並驗證 `HsuFSM` 100% 無損往返
    pub fn compile(&self, item: &CorpusItem) -> Option<CompiledSentence> {
        let chars: Vec<char> = item.text.chars().collect();
        let n = chars.len();
        if n == 0 {
            return None;
        }

        let mut syllables = Vec::with_capacity(n);
        let mut hsu_keys = String::with_capacity(n * 3);
        let mut has_polyphonic_single_char = false;

        let mut i = 0;
        while i < n {
            let max_len = (n - i).min(6);
            let mut matched: Option<(usize, &[String])> = None;
            for len in (2..=max_len).rev() {
                let cand: String = chars[i..i + len].iter().collect();
                if cand == "的是" {
                    continue;
                }
                if let Some(syls) = self.word_to_bpm.get(&cand) {
                    matched = Some((len, syls.as_slice()));
                    break;
                }
            }

            if let Some((len, syls)) = matched {
                for s in syls {
                    let hs = HsuSyllable::from_bopomofo_display(s)?;
                    let k = syllable_to_hsu_keys(&hs)?;
                    hsu_keys.push_str(&k);
                    syllables.push(hs);
                }
                i += len;
            } else {
                let ch = chars[i];
                if self.polyphonic_chars.contains(&ch)
                    && !matches!(ch, '的' | '了' | '嗎' | '呢' | '吧' | '個' | '這' | '那' | '是' | '有' | '在' | '不' | '我' | '你' | '他' | '她' | '它')
                {
                    has_polyphonic_single_char = true;
                }
                let bpm = preferred_single_char_bopomofo(ch)
                    .map(|s| s.to_string())
                    .or_else(|| self.char_to_bpm.get(&ch).cloned())?;
                let hs = HsuSyllable::from_bopomofo_display(&bpm)?;
                let k = syllable_to_hsu_keys(&hs)?;
                hsu_keys.push_str(&k);
                syllables.push(hs);
                i += 1;
            }
        }

        if syllables.len() != n {
            return None;
        }

        // 驗證 HsuFSM 逆向往返一致性（確保按鍵序列透過 HsuFSM 解出之音節與預期 100% 相同）
        let mut fsm = HsuFSM::new();
        for ch in hsu_keys.chars() {
            fsm.parse_hsu_key(ch);
        }
        let decoded_syls = fsm.get_all_syllables();
        if decoded_syls.len() != syllables.len() {
            return None;
        }
        for (a, b) in decoded_syls.iter().zip(syllables.iter()) {
            if a.bopomofo_display() != b.bopomofo_display() {
                return None;
            }
        }

        Some(CompiledSentence {
            item: item.clone(),
            syllables: decoded_syls,
            hsu_keys,
            has_polyphonic_single_char,
        })
    }
}

/// 計算字元級 Levenshtein 編輯距離
pub fn char_levenshtein(a: &str, b: &str) -> usize {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let m = ac.len();
    let n = bc.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if ac[i - 1] == bc[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// 互補誤差函數 $\text{erfc}(x)$ 近似（Abramowitz & Stegun 7.1.26，最大誤差 $< 1.5 \times 10^{-7}$），
/// 用於計算卡方自由度 1 之 McNemar 檢定雙尾 $p\text{-value} = \text{erfc}(\sqrt{\chi^2 / 2})$
pub fn erfc_approx(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * z);
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let poly = (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t;
    let res = poly * (-z * z).exp();
    if x >= 0.0 {
        res
    } else {
        2.0 - res
    }
}

/// 平行評估編譯後之語料集
pub fn evaluate_compiled_dataset(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    dataset_name: &str,
    hide_text: bool,
) -> EvalReport {
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    let mut all_results: Vec<SentenceEvalResult> = Vec::with_capacity(compiled.len());

    if chunk_size == 0 {
        return EvalReport {
            dataset: dataset_name.to_string(),
            total_sentences: 0,
            total_chars: 0,
            total_char_errors: 0,
            cer: 0.0,
            cer_percent: "0.00%".to_string(),
            exact_match_sentences: 0,
            sentence_accuracy: 0.0,
            sentence_accuracy_percent: "0.00%".to_string(),
            polyphonic_ambiguity_audit: json!({}),
            unreachable_gold_audit: json!({}),
            latency_us: json!({}),
            per_category: BTreeMap::new(),
            mcnemar_comparison: None,
            sentence_results: Vec::new(),
        };
    }

    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size) {
            let mut worker_engine = base_engine.clone();
            handles.push(s.spawn(move || {
                let mut local_out = Vec::with_capacity(chunk.len());
                let dag_builder = LatticeDAG::new(&worker_engine.lexicon);
                for cs in chunk {
                    let t0 = Instant::now();
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let (nodes, _) = worker_engine.router.solve_best_path(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker_engine.transitions),
                    );
                    let elapsed_us = t0.elapsed().as_micros() as u64;
                    let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                    let gold = &cs.item.text;
                    let char_len = gold.chars().count();
                    let char_errors = char_levenshtein(&pred, gold);
                    let is_exact_match = char_errors == 0;

                    let target_chars: Vec<char> = gold.chars().collect();
                    let is_gold_reachable = if is_exact_match {
                        true
                    } else {
                        worker_engine
                            .router
                            .solve_forced_alignment(
                                cs.syllables.len(),
                                &dag,
                                Some(&mut worker_engine.transitions),
                                &target_chars,
                            )
                            .is_some()
                    };

                    local_out.push(SentenceEvalResult {
                        id: cs.item.id.clone(),
                        category: cs.item.category.clone(),
                        gold: if hide_text { None } else { Some(gold.clone()) },
                        pred: if hide_text { None } else { Some(pred) },
                        char_len,
                        char_errors,
                        is_exact_match,
                        has_polyphonic_single_char: cs.has_polyphonic_single_char,
                        is_gold_reachable,
                        latency_us: elapsed_us,
                    });
                }
                local_out
            }));
        }
        for h in handles {
            if let Ok(part) = h.join() {
                all_results.extend(part);
            }
        }
    });

    let total_sentences = all_results.len();
    let total_chars: usize = all_results.iter().map(|r| r.char_len).sum();
    let total_char_errors: usize = all_results.iter().map(|r| r.char_errors).sum();
    let exact_match_sentences = all_results.iter().filter(|r| r.is_exact_match).count();

    let cer = if total_chars > 0 {
        total_char_errors as f64 / total_chars as f64
    } else {
        0.0
    };
    let sentence_accuracy = if total_sentences > 0 {
        exact_match_sentences as f64 / total_sentences as f64
    } else {
        0.0
    };

    // 破音字歧義統計
    let mut ambig_sentences = 0usize;
    let mut clean_sentences = 0usize;
    let mut clean_chars = 0usize;
    let mut clean_char_errors = 0usize;
    let mut clean_exact = 0usize;

    // 不可達黃金路徑統計
    let mut unreachable_count = 0usize;
    let mut reachable_chars = 0usize;
    let mut reachable_char_errors = 0usize;
    let mut reachable_sentences = 0usize;
    let mut reachable_exact = 0usize;

    let mut per_cat: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut latencies: Vec<u64> = Vec::with_capacity(total_sentences);

    for r in &all_results {
        latencies.push(r.latency_us);
        if r.has_polyphonic_single_char {
            ambig_sentences += 1;
        } else {
            clean_sentences += 1;
            clean_chars += r.char_len;
            clean_char_errors += r.char_errors;
            if r.is_exact_match {
                clean_exact += 1;
            }
        }

        if !r.is_gold_reachable {
            unreachable_count += 1;
        } else {
            reachable_sentences += 1;
            reachable_chars += r.char_len;
            reachable_char_errors += r.char_errors;
            if r.is_exact_match {
                reachable_exact += 1;
            }
        }

        let entry = per_cat.entry(r.category.clone()).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += r.char_len;
        entry.2 += r.char_errors;
        if r.is_exact_match {
            entry.3 += 1;
        }
    }

    latencies.sort_unstable();
    let p50 = latencies.get(latencies.len() / 2).copied().unwrap_or(0);
    let p95 = latencies
        .get((latencies.len() * 95) / 100)
        .copied()
        .unwrap_or(0);
    let p99 = latencies
        .get((latencies.len() * 99) / 100)
        .copied()
        .unwrap_or(0);
    let mean_us = if !latencies.is_empty() {
        latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
    } else {
        0.0
    };

    let mut per_category = BTreeMap::new();
    for (cat, (sents, chs, errs, ex)) in per_cat {
        let c_cer = if chs > 0 { errs as f64 / chs as f64 } else { 0.0 };
        let c_ser = if sents > 0 {
            ex as f64 / sents as f64
        } else {
            0.0
        };
        per_category.insert(
            cat,
            CategoryMetrics {
                sentences: sents,
                chars: chs,
                char_errors: errs,
                cer: c_cer,
                cer_percent: format!("{:.2}%", c_cer * 100.0),
                exact_match: ex,
                sentence_accuracy: c_ser,
                sentence_accuracy_percent: format!("{:.2}%", c_ser * 100.0),
            },
        );
    }

    let clean_cer = if clean_chars > 0 {
        clean_char_errors as f64 / clean_chars as f64
    } else {
        0.0
    };
    let clean_ser = if clean_sentences > 0 {
        clean_exact as f64 / clean_sentences as f64
    } else {
        0.0
    };
    let reach_cer = if reachable_chars > 0 {
        reachable_char_errors as f64 / reachable_chars as f64
    } else {
        0.0
    };
    let reach_ser = if reachable_sentences > 0 {
        reachable_exact as f64 / reachable_sentences as f64
    } else {
        0.0
    };

    EvalReport {
        dataset: dataset_name.to_string(),
        total_sentences,
        total_chars,
        total_char_errors,
        cer,
        cer_percent: format!("{:.2}%", cer * 100.0),
        exact_match_sentences,
        sentence_accuracy,
        sentence_accuracy_percent: format!("{:.2}%", sentence_accuracy * 100.0),
        polyphonic_ambiguity_audit: json!({
            "ambiguous_sentences_count": ambig_sentences,
            "ambiguous_sentences_ratio": format!("{:.2}%", if total_sentences > 0 { (ambig_sentences as f64 / total_sentences as f64) * 100.0 } else { 0.0 }),
            "unambiguous_sentences_count": clean_sentences,
            "cer_excluding_polyphonic_ambiguity": clean_cer,
            "cer_excluding_polyphonic_ambiguity_percent": format!("{:.2}%", clean_cer * 100.0),
            "sentence_accuracy_excluding_polyphonic_ambiguity": clean_ser,
            "sentence_accuracy_excluding_polyphonic_ambiguity_percent": format!("{:.2}%", clean_ser * 100.0)
        }),
        unreachable_gold_audit: json!({
            "unreachable_sentences_count": unreachable_count,
            "unreachable_sentences_ratio": format!("{:.2}%", if total_sentences > 0 { (unreachable_count as f64 / total_sentences as f64) * 100.0 } else { 0.0 }),
            "reachable_sentences_count": reachable_sentences,
            "cer_on_reachable_gold": reach_cer,
            "cer_on_reachable_gold_percent": format!("{:.2}%", reach_cer * 100.0),
            "sentence_accuracy_on_reachable_gold_percent": format!("{:.2}%", reach_ser * 100.0)
        }),
        latency_us: json!({
            "p50_us": p50,
            "p95_us": p95,
            "p99_us": p99,
            "mean_us": (mean_us * 10.0).round() / 10.0,
            "p50_ms": format!("{:.3} ms", p50 as f64 / 1000.0),
            "p95_ms": format!("{:.3} ms", p95 as f64 / 1000.0),
            "p99_ms": format!("{:.3} ms", p99 as f64 / 1000.0)
        }),
        per_category,
        mcnemar_comparison: None,
        sentence_results: all_results,
    }
}

/// 執行 McNemar 統計檢定與回歸比較（嚴守 `test` 集零句子內容輸出）
pub fn compare_with_baseline(
    baseline: &EvalReport,
    current: &EvalReport,
    hide_text: bool,
) -> Value {
    let base_map: HashMap<&str, &SentenceEvalResult> = baseline
        .sentence_results
        .iter()
        .map(|r| (r.id.as_str(), r))
        .collect();

    let mut b_fixed = 0usize; // 基準錯 ➔ 新版完全正確
    let mut c_broken = 0usize; // 基準完全正確 ➔ 新版錯
    let mut char_improved = 0usize; // 新版字元錯誤數 < 基準字元錯誤數
    let mut char_degraded = 0usize; // 新版字元錯誤數 > 基準字元錯誤數
    let mut fixed_examples = Vec::new();
    let mut broken_examples = Vec::new();

    for cur in &current.sentence_results {
        if let Some(old) = base_map.get(cur.id.as_str()) {
            if !old.is_exact_match && cur.is_exact_match {
                b_fixed += 1;
                if !hide_text && fixed_examples.len() < 40 {
                    fixed_examples.push(json!({
                        "id": cur.id,
                        "category": cur.category,
                        "gold": cur.gold,
                        "baseline_pred": old.pred,
                        "new_pred": cur.pred
                    }));
                }
            } else if old.is_exact_match && !cur.is_exact_match {
                c_broken += 1;
                if !hide_text && broken_examples.len() < 40 {
                    broken_examples.push(json!({
                        "id": cur.id,
                        "category": cur.category,
                        "gold": cur.gold,
                        "baseline_pred": old.pred,
                        "new_pred": cur.pred
                    }));
                }
            }
            if cur.char_errors < old.char_errors {
                char_improved += 1;
            } else if cur.char_errors > old.char_errors {
                char_degraded += 1;
            }
        }
    }

    let discord_sum = b_fixed + c_broken;
    let chi_square = if discord_sum > 0 {
        let diff = (b_fixed as f64 - c_broken as f64).abs();
        let num = (diff - 1.0).max(0.0);
        (num * num) / (discord_sum as f64)
    } else {
        0.0
    };
    let p_value = if chi_square > 0.0 {
        erfc_approx((chi_square / 2.0).sqrt())
    } else {
        1.0
    };
    let significant_improvement = b_fixed > c_broken && p_value < 0.05;

    let mut out = json!({
        "baseline_cer": baseline.cer,
        "new_cer": current.cer,
        "delta_cer": current.cer - baseline.cer,
        "delta_cer_percentage_points": format!("{:+.3}%", (current.cer - baseline.cer) * 100.0),
        "baseline_sentence_accuracy": baseline.sentence_accuracy,
        "new_sentence_accuracy": current.sentence_accuracy,
        "delta_sentence_accuracy_percentage_points": format!("{:+.2}%", (current.sentence_accuracy - baseline.sentence_accuracy) * 100.0),
        "mcnemar_b_fixed_sentences": b_fixed,
        "mcnemar_c_broken_sentences": c_broken,
        "char_error_improved_sentences": char_improved,
        "char_error_degraded_sentences": char_degraded,
        "mcnemar_chi_square": (chi_square * 10000.0).round() / 10000.0,
        "mcnemar_p_value": p_value,
        "statistically_significant_improvement": significant_improvement
    });

    if !hide_text {
        out["fixed_examples"] = json!(fixed_examples);
        out["broken_examples"] = json!(broken_examples);
    }

    out
}

/// 載入 TSV 語料集 (`id\tsource_category\ttext`)
pub fn load_corpus_tsv(path: &Path) -> Vec<CorpusItem> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut items = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if idx == 0 && (trimmed.starts_with("id\t") || trimmed.starts_with("ID\t")) {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() >= 3 {
            items.push(CorpusItem {
                id: parts[0].trim().to_string(),
                category: parts[1].trim().to_string(),
                text: parts[2].trim().to_string(),
            });
        }
    }
    items
}

/// 執行規則庫消融實驗 (`--ablation`)，精準量化每一個規則族與單條規則在 `dev.tsv` 上的觸發次數、觸發正確率與 $\Delta \text{CER}$
pub fn run_ablation_study(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    prune_harmful: bool,
    root: &Path,
) -> Value {
    println!(
        "[Ablation] 正在掃描 {} 句開發集 (dev.tsv) 之全圖格規則觸發軌跡與最佳路徑歸因...",
        compiled.len()
    );
    let dag_builder = LatticeDAG::new(&base_engine.lexicon);

    let mut total_chars = 0usize;
    let mut base_total_errors = 0usize;
    let mut base_errors_per_sent: Vec<usize> = Vec::with_capacity(compiled.len());
    let mut dags: Vec<HashMap<usize, Vec<ruhi::router::LatticeNode>>> =
        Vec::with_capacity(compiled.len());

    // 紀錄每條規則在哪些句子之 Lattice 中曾被評估 (lattice_touched_sents)
    let mut lattice_touched_sents: HashMap<String, Vec<usize>> = HashMap::new();
    let mut family_touched_sents: HashMap<String, Vec<usize>> = HashMap::new();

    // 紀錄每條規則在 1-Best 最佳路徑上之觸發次數與正確句數
    let mut best_path_trigger_count: HashMap<String, usize> = HashMap::new();
    let mut best_path_correct_count: HashMap<String, usize> = HashMap::new();
    let mut family_best_trigger_count: HashMap<String, usize> = HashMap::new();
    let mut family_best_correct_count: HashMap<String, usize> = HashMap::new();

    let mut trace_engine = base_engine.clone();
    for (s_idx, cs) in compiled.iter().enumerate() {
        let dag = dag_builder.build_dag(&cs.syllables, true);
        trace_engine.transitions.active_trace = Some(Vec::new());
        let (best_nodes, _) = trace_engine.router.solve_best_path(
            cs.syllables.len(),
            &dag,
            Some(&mut trace_engine.transitions),
        );
        let lattice_events = trace_engine.transitions.active_trace.take().unwrap_or_default();

        let mut seen_rules_in_sent = HashSet::new();
        let mut seen_fams_in_sent = HashSet::new();
        for (rname, fam, _) in lattice_events {
            if seen_rules_in_sent.insert(rname.clone()) {
                lattice_touched_sents.entry(rname).or_default().push(s_idx);
            }
            if seen_fams_in_sent.insert(fam.clone()) {
                family_touched_sents.entry(fam).or_default().push(s_idx);
            }
        }

        let pred: String = best_nodes.iter().map(|n| n.word.as_str()).collect();
        let ch_len = cs.item.text.chars().count();
        let errs = char_levenshtein(&pred, &cs.item.text);
        total_chars += ch_len;
        base_total_errors += errs;
        base_errors_per_sent.push(errs);

        let (_, best_rules) = trace_engine.router.extract_path_features_and_triggers(
            &best_nodes,
            &dag,
            &mut trace_engine.transitions,
        );
        let mut seen_best_fams = HashSet::new();
        for rname in best_rules {
            *best_path_trigger_count.entry(rname.clone()).or_insert(0) += 1;
            if errs == 0 {
                *best_path_correct_count.entry(rname.clone()).or_insert(0) += 1;
            }
            let fam =
                ruhi::router::transitions::TransitionEngine::classify_rule_family(&rname).to_string();
            if seen_best_fams.insert(fam.clone()) {
                *family_best_trigger_count.entry(fam.clone()).or_insert(0) += 1;
                if errs == 0 {
                    *family_best_correct_count.entry(fam).or_insert(0) += 1;
                }
            }
        }
        dags.push(dag);
    }

    let base_cer = if total_chars > 0 {
        base_total_errors as f64 / total_chars as f64
    } else {
        0.0
    };

    // 1. 規則族 (Rule Families) 消融實驗
    let families = [
        "user_bigram_pos",
        "user_bigram_neg",
        "builtin_bigram",
        "vo_separable_lookback",
        "construction_hierarchical_matrix",
        "family_user_pattern",
        "family_vo_complement",
        "family_classifier_numeric",
        "family_spatial_localizer",
        "family_aspect_temporal",
        "family_conjunction_discourse",
        "family_morph_phonetic_guard",
        "family_core_syntax",
    ];

    let mut family_reports = Vec::new();
    for &fam in &families {
        let touched = family_touched_sents.get(fam).cloned().unwrap_or_default();
        let mut diff_errors: isize = 0;
        let mut helped_sents = 0usize; // 停用後變差（表示該規則族有正面貢獻）
        let mut hurt_sents = 0usize; // 停用後變好（表示該規則族造成負面干擾）

        if !touched.is_empty() {
            let mut test_trans = base_engine.transitions.clone();
            test_trans.disabled_rules.insert(fam.to_string());
            for &s_idx in &touched {
                let cs = &compiled[s_idx];
                let (nodes, _) = base_engine.router.solve_best_path(
                    cs.syllables.len(),
                    &dags[s_idx],
                    Some(&mut test_trans),
                );
                let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                let ablated_errs = char_levenshtein(&pred, &cs.item.text);
                let orig_errs = base_errors_per_sent[s_idx];
                diff_errors += (ablated_errs as isize) - (orig_errs as isize);
                if ablated_errs > orig_errs {
                    helped_sents += 1;
                } else if ablated_errs < orig_errs {
                    hurt_sents += 1;
                }
            }
        }

        let ablated_total_errs = (base_total_errors as isize + diff_errors).max(0) as usize;
        let ablated_cer = ablated_total_errs as f64 / total_chars.max(1) as f64;
        let delta_cer = ablated_cer - base_cer;
        let trig_cnt = family_best_trigger_count.get(fam).copied().unwrap_or(0);
        let corr_cnt = family_best_correct_count.get(fam).copied().unwrap_or(0);
        let acc = if trig_cnt > 0 {
            (corr_cnt as f64 / trig_cnt as f64) * 100.0
        } else {
            0.0
        };

        family_reports.push(json!({
            "family": fam,
            "lattice_matched_sentences": touched.len(),
            "best_path_trigger_sentences": trig_cnt,
            "when_triggered_exact_accuracy": format!("{:.2}%", acc),
            "ablated_cer": ablated_cer,
            "delta_cer": delta_cer,
            "delta_char_errors_when_disabled": diff_errors,
            "helped_sentences_count": helped_sents,
            "hurt_sentences_count": hurt_sents,
            "classification": if diff_errors > 0 {
                "POSITIVE_CONTRIBUTOR"
            } else if diff_errors < 0 {
                "NEGATIVE_INTERFERENCE"
            } else {
                "DEAD_OR_REDUNDANT"
            }
        }));
    }

    // 2. 單條規則 (Individual Rules) 消融實驗
    // 收集所有已知規則名稱（含 grammar_rules, user_trigram_patterns, 以及所有曾於 Lattice 觸發之規則）
    let mut all_rule_names: HashSet<String> = HashSet::new();
    for r in &base_engine.transitions.grammar_rules {
        all_rule_names.insert(r.name.clone());
    }
    for t in &base_engine.transitions.user_trigram_patterns {
        all_rule_names.insert(format!("trigram_{}", t.name));
    }
    for k in lattice_touched_sents.keys() {
        all_rule_names.insert(k.clone());
    }

    let mut sorted_rules: Vec<String> = all_rule_names.into_iter().collect();
    sorted_rules.sort();

    let mut positive_rules = Vec::new();
    let mut negative_rules = Vec::new();
    let mut dead_or_zero_rules = Vec::new();
    let mut harmful_user_bigrams: Vec<(String, String)> = Vec::new();

    for rname in &sorted_rules {
        let touched = lattice_touched_sents.get(rname).cloned().unwrap_or_default();
        let mut diff_errors: isize = 0;
        let mut helped_sents = 0usize;
        let mut hurt_sents = 0usize;

        if !touched.is_empty() {
            let mut test_trans = base_engine.transitions.clone();
            test_trans.disabled_rules.insert(rname.clone());
            for &s_idx in &touched {
                let cs = &compiled[s_idx];
                let (nodes, _) = base_engine.router.solve_best_path(
                    cs.syllables.len(),
                    &dags[s_idx],
                    Some(&mut test_trans),
                );
                let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                let ablated_errs = char_levenshtein(&pred, &cs.item.text);
                let orig_errs = base_errors_per_sent[s_idx];
                diff_errors += (ablated_errs as isize) - (orig_errs as isize);
                if ablated_errs > orig_errs {
                    helped_sents += 1;
                } else if ablated_errs < orig_errs {
                    hurt_sents += 1;
                }
            }
        }

        let ablated_total_errs = (base_total_errors as isize + diff_errors).max(0) as usize;
        let ablated_cer = ablated_total_errs as f64 / total_chars.max(1) as f64;
        let delta_cer = ablated_cer - base_cer;
        let trig_cnt = best_path_trigger_count.get(rname).copied().unwrap_or(0);
        let corr_cnt = best_path_correct_count.get(rname).copied().unwrap_or(0);
        let acc = if trig_cnt > 0 {
            (corr_cnt as f64 / trig_cnt as f64) * 100.0
        } else {
            0.0
        };
        let fam = ruhi::router::transitions::TransitionEngine::classify_rule_family(rname);

        let entry = json!({
            "rule_id": rname,
            "family": fam,
            "lattice_matched_sentences": touched.len(),
            "best_path_trigger_count": trig_cnt,
            "when_triggered_accuracy": format!("{:.2}%", acc),
            "delta_cer": delta_cer,
            "delta_char_errors_when_disabled": diff_errors,
            "helped_sentences_count": helped_sents,
            "hurt_sentences_count": hurt_sents,
            "classification": if diff_errors > 0 {
                "POSITIVE_CONTRIBUTOR"
            } else if diff_errors < 0 {
                "NEGATIVE_INTERFERENCE"
            } else {
                "DEAD_OR_REDUNDANT"
            }
        });

        if diff_errors > 0 {
            positive_rules.push((diff_errors, entry));
        } else if diff_errors < 0 {
            if let Some(rest) = rname
                .strip_prefix("user_bigram_pos:")
                .or_else(|| rname.strip_prefix("user_bigram_neg:"))
            {
                if let Some((p, n)) = rest.split_once("->") {
                    harmful_user_bigrams.push((p.to_string(), n.to_string()));
                }
            }
            negative_rules.push((diff_errors, entry));
        } else {
            dead_or_zero_rules.push(entry);
        }
    }

    positive_rules.sort_by(|a, b| b.0.cmp(&a.0));
    negative_rules.sort_by(|a, b| a.0.cmp(&b.0));

    let mut pruned_bigram_count = 0usize;
    if prune_harmful && !harmful_user_bigrams.is_empty() {
        let harmful_set: HashSet<(String, String)> = harmful_user_bigrams.into_iter().collect();
        let tsv_paths = [
            root.join("ruhi_lexicon").join("data").join("transitions_user.tsv"),
            root.join("data").join("transitions_user.tsv"),
            root.join("dist").join("data").join("transitions_user.tsv"),
        ];
        for (idx, p) in tsv_paths.iter().enumerate() {
            if !p.is_file() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(p) {
                let mut kept = Vec::new();
                let mut removed = 0usize;
                for line in content.lines() {
                    let t = line.trim();
                    if t.is_empty() || t.starts_with('#') {
                        kept.push(line.to_string());
                        continue;
                    }
                    let cols: Vec<&str> = t.split('\t').collect();
                    if cols.len() >= 2
                        && harmful_set.contains(&(cols[0].trim().to_string(), cols[1].trim().to_string()))
                    {
                        removed += 1;
                        continue;
                    }
                    kept.push(line.to_string());
                }
                if removed > 0 {
                    let mut out_str = kept.join("\n");
                    out_str.push('\n');
                    let _ = fs::write(p, out_str);
                    if idx == 0 {
                        pruned_bigram_count = removed;
                    }
                }
            }
        }
    }

    let pos_vec: Vec<Value> = positive_rules.into_iter().map(|(_, v)| v).collect();
    let neg_vec: Vec<Value> = negative_rules.into_iter().map(|(_, v)| v).collect();

    json!({
        "dataset": "dev",
        "baseline_cer": base_cer,
        "baseline_cer_percent": format!("{:.2}%", base_cer * 100.0),
        "total_evaluated_rules": sorted_rules.len(),
        "summary": {
            "positive_contributor_rules_count": pos_vec.len(),
            "negative_interference_rules_count": neg_vec.len(),
            "dead_or_zero_impact_rules_count": dead_or_zero_rules.len(),
            "pruned_harmful_user_bigrams_count": pruned_bigram_count
        },
        "family_ablation": family_reports,
        "positive_contributor_rules": pos_vec,
        "negative_interference_rules": neg_vec,
        "dead_or_zero_impact_rules_sample": dead_or_zero_rules.into_iter().take(150).collect::<Vec<_>>()
    })
}

/// 執行平均結構化感知機 (Averaged Structured Perceptron) 權重校準 (`--calibrate`)
pub fn run_perceptron_calibration(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    root: &Path,
) -> Value {
    println!(
        "[Calibration] 正在對 {} 句開發集執行強制對齊 (Forced Alignment) 與特徵擷取...",
        compiled.len()
    );
    let dag_builder = LatticeDAG::new(&base_engine.lexicon);

    let feature_keys = [
        "freq_ln",
        "multi_char_len",
        "cohesion_len3",
        "cohesion_len4",
        "coord_prior",
        "user_bigram_pos",
        "user_bigram_neg",
        "builtin_bigram",
        "vo_separable_lookback",
        "construction_hierarchical_matrix",
        "family_user_pattern",
        "family_vo_complement",
        "family_classifier_numeric",
        "family_spatial_localizer",
        "family_aspect_temporal",
        "family_conjunction_discourse",
        "family_morph_phonetic_guard",
        "family_core_syntax",
    ];

    let mut dags = Vec::with_capacity(compiled.len());
    let mut gold_paths: Vec<Option<(Vec<ruhi::router::LatticeNode>, HashMap<String, f64>)>> =
        Vec::with_capacity(compiled.len());
    let mut unreachable_count = 0usize;
    let mut total_chars = 0usize;
    let mut init_errors = 0usize;

    let mut work_engine = base_engine.clone();
    work_engine.transitions.feature_weights.clear();

    for cs in compiled {
        let dag = dag_builder.build_dag(&cs.syllables, true);
        let (pred_nodes, _) = work_engine.router.solve_best_path(
            cs.syllables.len(),
            &dag,
            Some(&mut work_engine.transitions),
        );
        let pred: String = pred_nodes.iter().map(|n| n.word.as_str()).collect();
        let ch_len = cs.item.text.chars().count();
        let errs = char_levenshtein(&pred, &cs.item.text);
        total_chars += ch_len;
        init_errors += errs;

        let target_chars: Vec<char> = cs.item.text.chars().collect();
        if let Some((g_nodes, _)) = work_engine.router.solve_forced_alignment(
            cs.syllables.len(),
            &dag,
            Some(&mut work_engine.transitions),
            &target_chars,
        ) {
            let (g_feats, _) = work_engine.router.extract_path_features_and_triggers(
                &g_nodes,
                &dag,
                &mut work_engine.transitions,
            );
            gold_paths.push(Some((g_nodes, g_feats)));
        } else {
            unreachable_count += 1;
            gold_paths.push(None);
        }
        dags.push(dag);
    }

    let init_cer = init_errors as f64 / total_chars.max(1) as f64;
    println!(
        "[Calibration] 初始 CER = {:.2}% ({}/{} 字錯誤)，黃金路徑可達率 = {:.2}% ({}/{} 句)",
        init_cer * 100.0,
        init_errors,
        total_chars,
        ((compiled.len() - unreachable_count) as f64 / compiled.len().max(1) as f64) * 100.0,
        compiled.len() - unreachable_count,
        compiled.len()
    );

    let mut w: HashMap<String, f64> = feature_keys.iter().map(|&k| (k.to_string(), 1.0)).collect();
    let mut w_sum: HashMap<String, f64> =
        feature_keys.iter().map(|&k| (k.to_string(), 0.0)).collect();
    let mut total_steps = 0f64;

    let mut best_weights = w.clone();
    let mut best_errors = init_errors;
    let mut epoch_history = Vec::new();

    let epochs = 5;
    for ep in 1..=epochs {
        let lr = 0.00004 / (ep as f64).sqrt();
        let mut updates_in_epoch = 0usize;

        for (idx, cs) in compiled.iter().enumerate() {
            let Some((_, ref g_feats)) = gold_paths[idx] else {
                continue;
            };
            work_engine.transitions.feature_weights = w.clone();
            let (pred_nodes, _) = work_engine.router.solve_best_path(
                cs.syllables.len(),
                &dags[idx],
                Some(&mut work_engine.transitions),
            );
            let pred_str: String = pred_nodes.iter().map(|n| n.word.as_str()).collect();
            if pred_str != cs.item.text {
                let (p_feats, _) = work_engine.router.extract_path_features_and_triggers(
                    &pred_nodes,
                    &dags[idx],
                    &mut work_engine.transitions,
                );
                for &k in &feature_keys {
                    let gv = g_feats.get(k).copied().unwrap_or(0.0);
                    let pv = p_feats.get(k).copied().unwrap_or(0.0);
                    let diff = (gv - pv).clamp(-25.0, 25.0);
                    if diff.abs() > 1e-9 {
                        let cur_w = w.get(k).copied().unwrap_or(1.0);
                        // L2 正則化錨定於 1.0，保護雙字詞凝聚力與既有 107 組單元測試不退化
                        let (lo, hi) = match k {
                            "multi_char_len" | "cohesion_len3" | "cohesion_len4" | "coord_prior" => (0.98, 1.05),
                            "family_core_syntax" | "family_morph_phonetic_guard" | "construction_hierarchical_matrix" => (0.94, 1.06),
                            _ => (0.88, 1.12),
                        };
                        let next_w = (cur_w + lr * diff - lr * 0.25 * (cur_w - 1.0)).clamp(lo, hi);
                        w.insert(k.to_string(), next_w);
                    }
                }
                updates_in_epoch += 1;
            }
            total_steps += 1.0;
            for &k in &feature_keys {
                *w_sum.entry(k.to_string()).or_insert(0.0) += w.get(k).copied().unwrap_or(1.0);
            }
        }

        let avg_w: HashMap<String, f64> = feature_keys
            .iter()
            .map(|&k| {
                let v = if total_steps > 0.0 {
                    w_sum.get(k).copied().unwrap_or(total_steps) / total_steps
                } else {
                    1.0
                };
                (k.to_string(), (v * 10000.0).round() / 10000.0)
            })
            .collect();

        // 評估當前平均權重於 dev.tsv 之 CER（直接使用預先建構之 dags 加速）
        let mut ep_errors = 0usize;
        let mut ep_exact = 0usize;
        work_engine.transitions.feature_weights = avg_w.clone();
        for (idx, cs) in compiled.iter().enumerate() {
            let (p_nodes, _) = work_engine.router.solve_best_path(
                cs.syllables.len(),
                &dags[idx],
                Some(&mut work_engine.transitions),
            );
            let pred_s: String = p_nodes.iter().map(|n| n.word.as_str()).collect();
            let e = char_levenshtein(&pred_s, &cs.item.text);
            ep_errors += e;
            if e == 0 {
                ep_exact += 1;
            }
        }
        let ep_cer = ep_errors as f64 / total_chars.max(1) as f64;
        let ep_ser = ep_exact as f64 / compiled.len().max(1) as f64;
        println!(
            "[Calibration] Epoch {}/{}: updates={}, dev CER={:.2}% ({} errors), SER={:.2}%",
            ep,
            epochs,
            updates_in_epoch,
            ep_cer * 100.0,
            ep_errors,
            ep_ser * 100.0
        );
        epoch_history.push(json!({
            "epoch": ep,
            "updates": updates_in_epoch,
            "dev_cer": ep_cer,
            "dev_cer_percent": format!("{:.2}%", ep_cer * 100.0),
            "dev_char_errors": ep_errors,
            "dev_sentence_accuracy_percent": format!("{:.2}%", ep_ser * 100.0),
            "averaged_weights": avg_w
        }));

        if ep_errors < best_errors {
            best_errors = ep_errors;
            best_weights = avg_w;
        }
    }

    // 寫出最佳權重至 feature_weights.tsv
    let mut tsv_lines = vec![
        "# RuhI 線性評估模型特徵與規則族權重表 (Averaged Structured Perceptron Calibrated)".to_string(),
        "# 格式: feature_or_family_name\tweight_multiplier".to_string(),
    ];
    for &k in &feature_keys {
        let val = best_weights.get(k).copied().unwrap_or(1.0);
        tsv_lines.push(format!("{k}\t{val:.4}"));
    }
    let tsv_content = tsv_lines.join("\n") + "\n";

    let target_paths = [
        root.join("ruhi_lexicon").join("data").join("feature_weights.tsv"),
        root.join("data").join("feature_weights.tsv"),
        root.join("dist").join("data").join("feature_weights.tsv"),
    ];
    for p in &target_paths {
        if let Some(parent) = p.parent() {
            if parent.is_dir() {
                let _ = fs::write(p, &tsv_content);
            }
        }
    }

    let best_cer = best_errors as f64 / total_chars.max(1) as f64;
    json!({
        "status": "ok",
        "initial_dev_cer": init_cer,
        "initial_dev_cer_percent": format!("{:.2}%", init_cer * 100.0),
        "calibrated_dev_cer": best_cer,
        "calibrated_dev_cer_percent": format!("{:.2}%", best_cer * 100.0),
        "delta_dev_cer": best_cer - init_cer,
        "unreachable_gold_sentences": unreachable_count,
        "unreachable_gold_ratio": format!("{:.2}%", (unreachable_count as f64 / compiled.len().max(1) as f64) * 100.0),
        "best_weights": best_weights,
        "epoch_history": epoch_history
    })
}

/// 確定性偽亂數產生器 (XorShift64)，用於固定種子 (`seed = 20261021`) 語料洗牌與 70/30 切分
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x853c49e6748fea9b } else { seed },
        }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    fn shuffle<T>(&mut self, slice: &mut [T]) {
        let n = slice.len();
        for i in (1..n).rev() {
            let j = (self.next_u64() as usize) % (i + 1);
            slice.swap(i, j);
        }
    }
}

fn is_pure_cjk_clause(s: &str) -> bool {
    let ch_count = s.chars().count();
    if !(7..=28).contains(&ch_count) {
        return false;
    }
    s.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 排除含外文音譯、宗教專名、罕用字或中國用語之子句，確保 100% 台灣現代正體通用語境
fn is_clean_taiwan_modern_clause(s: &str) -> bool {
    let banned_substrings = [
        "帕拉", "樸拉", "阿難", "吠陀", "梵語", "梵文", "上師", "靈修", "至上意識", "至上本體",
        "靜坐", "冥想", "業力", "那羅延", "克里希那", "卡皮拉", "婆羅門", "蘇菲", "唯識",
        "加爾各答", "帕特納", "帕特那", "瓦拉納西", "喜悅甘露", "朿縛", "衪", "祂",
        "信息", "軟件", "硬件", "內存", "服務器", "數據庫", "網絡", "屏幕", "視頻",
    ];
    for b in &banned_substrings {
        if s.contains(b) {
            return false;
        }
    }
    // 排除連續三個相同字元
    let chars: Vec<char> = s.chars().collect();
    for w in chars.windows(3) {
        if w[0] == w[1] && w[1] == w[2] {
            return false;
        }
    }
    true
}

/// 建置並封存 5,200 句台灣繁體中文分層抽樣評估語料庫 (`ruhi_lexicon/eval/dev.tsv` & `test.tsv`)
pub fn build_and_seal_evaluation_corpus(root: &Path, engine: &RuhiEngine) -> Value {
    println!("[CorpusBuilder] 正在建置台灣繁體中文 5,200 句分層抽樣語料庫...");
    let compiler = PhoneticCompiler::from_lexicon(&engine.lexicon);

    // 1. 收集所有已存在於 engine.rs 與 tuner.rs 之測試句（嚴格排除，杜絕評估分數虛高）
    let mut excluded_sentences: HashSet<String> = HashSet::new();
    for src_path in [
        root.join("ruhi-core").join("src").join("engine.rs"),
        root.join("ruhi-studio").join("src").join("tuner.rs"),
    ] {
        if let Ok(src) = fs::read_to_string(&src_path) {
            let mut cur = String::new();
            for ch in src.chars() {
                if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                    cur.push(ch);
                } else {
                    if cur.chars().count() >= 4 {
                        excluded_sentences.insert(cur.clone());
                    }
                    cur.clear();
                }
            }
        }
    }

    // 收集詞庫單一詞條集合与四字成語集合
    let mut lexicon_single_entries: HashSet<String> = HashSet::new();
    let mut moe_four_char_idioms: Vec<String> = Vec::new();
    for e in &engine.lexicon.entries {
        let w = e.word.trim();
        lexicon_single_entries.insert(w.to_string());
        if w.chars().count() == 4
            && e.category != "archaic"
            && w.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        {
            moe_four_char_idioms.push(w.to_string());
        }
    }
    moe_four_char_idioms.sort();
    moe_four_char_idioms.dedup();
    let moe_idiom_set: HashSet<String> = moe_four_char_idioms.into_iter().collect();

    let mut seen_clauses: HashSet<String> = HashSet::new();
    let mut cat_buckets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for cat in [
        "daily_dialogue",
        "tech_doc",
        "literary_narrative",
        "civic_public",
        "moe_idiom_context",
    ] {
        cat_buckets.insert(cat.to_string(), Vec::new());
    }

    let is_usable_clause = |cl: &str,
                            seen: &mut HashSet<String>,
                            excluded: &HashSet<String>,
                            lex_entries: &HashSet<String>,
                            comp: &PhoneticCompiler|
     -> bool {
        if !is_pure_cjk_clause(cl) || !is_clean_taiwan_modern_clause(cl) {
            return false;
        }
        if lex_entries.contains(cl) || excluded.contains(cl) {
            return false;
        }
        if excluded.iter().any(|ex| ex.len() >= 12 && (ex.contains(cl) || cl.contains(ex.as_str()))) {
            return false;
        }
        if !seen.insert(cl.to_string()) {
            return false;
        }
        let dummy = CorpusItem {
            id: String::new(),
            category: String::new(),
            text: cl.to_string(),
        };
        comp.compile(&dummy).is_some()
    };

    // A. 掃描工作區內所有 Markdown 技術文件與 i18n 介面字串提取 `tech_doc`
    let atg_root = root.parent().unwrap_or(root);
    let mut md_files = Vec::new();
    fn collect_md_and_tsv(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 4 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "target" | "node_modules" | "_external" | "_backup" | "dist" | "eval"
                )
            {
                continue;
            }
            if p.is_dir() {
                collect_md_and_tsv(&p, out, depth + 1);
            } else if name.ends_with(".md") || name.contains("i18n") {
                out.push(p);
            }
        }
    }
    collect_md_and_tsv(atg_root, &mut md_files, 0);
    md_files.sort();

    for p in &md_files {
        if let Ok(txt) = fs::read_to_string(p) {
            let mut cur = String::new();
            for ch in txt.chars() {
                if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                    cur.push(ch);
                } else {
                    if cur.chars().count() >= 7 {
                        let cl = cur.clone();
                        if is_usable_clause(
                            &cl,
                            &mut seen_clauses,
                            &excluded_sentences,
                            &lexicon_single_entries,
                            &compiler,
                        ) {
                            cat_buckets.get_mut("tech_doc").unwrap().push(cl);
                        }
                    }
                    cur.clear();
                }
            }
        }
    }

    // B. 掃描大型平行語料庫，依詞彙與構式特徵分流至五大領域
    let corpus_files = [
        atg_root
            .join("RuhOS")
            .join("corpus")
            .join("bilingual_parallel")
            .join("baba_bilingual_parallel.tsv"),
        atg_root
            .join("RuhOS")
            .join("tools")
            .join("tranStyler")
            .join("profiles")
            .join("default_dashi")
            .join("corpus_triplets.tsv"),
    ];

    for cp in &corpus_files {
        if !cp.is_file() {
            continue;
        }
        if let Ok(txt) = fs::read_to_string(cp) {
            for line in txt.lines() {
                let mut cur = String::new();
                for ch in line.chars() {
                    if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                        cur.push(ch);
                    } else {
                        if (7..=26).contains(&cur.chars().count()) {
                            let cl = cur.clone();
                            let chars: Vec<char> = cl.chars().collect();
                            let has_moe_idiom = chars.windows(4).any(|w| {
                                let s: String = w.iter().collect();
                                moe_idiom_set.contains(&s)
                            });

                            let target_cat = if has_moe_idiom
                                && cat_buckets["moe_idiom_context"].len() < 1600
                            {
                                Some("moe_idiom_context")
                            } else if (cl.contains("我們")
                                || cl.contains("你們")
                                || cl.contains("大家")
                                || cl.contains("覺得")
                                || cl.contains("告訴")
                                || cl.contains("高興")
                                || cl.contains("喜歡")
                                || cl.contains("朋友")
                                || cl.contains("回家")
                                || cl.contains("吃飯")
                                || cl.contains("說話")
                                || cl.contains("休息")
                                || cl.contains("幫忙")
                                || cl.contains("一起")
                                || cl.contains("怎麼")
                                || cl.contains("為什麼"))
                                && cat_buckets["daily_dialogue"].len() < 1600
                            {
                                Some("daily_dialogue")
                            } else if (cl.contains("社會")
                                || cl.contains("制度")
                                || cl.contains("教育")
                                || cl.contains("經濟")
                                || cl.contains("政府")
                                || cl.contains("國家")
                                || cl.contains("法律")
                                || cl.contains("政策")
                                || cl.contains("發展")
                                || cl.contains("文化")
                                || cl.contains("公共")
                                || cl.contains("責任")
                                || cl.contains("公民")
                                || cl.contains("資源"))
                                && cat_buckets["civic_public"].len() < 1600
                            {
                                Some("civic_public")
                            } else if (cl.contains("科學")
                                || cl.contains("技術")
                                || cl.contains("系統")
                                || cl.contains("結構")
                                || cl.contains("方法")
                                || cl.contains("研究")
                                || cl.contains("實驗")
                                || cl.contains("物理")
                                || cl.contains("分析")
                                || cl.contains("邏輯")
                                || cl.contains("原理"))
                                && cat_buckets["tech_doc"].len() < 1600
                            {
                                Some("tech_doc")
                            } else if cat_buckets["literary_narrative"].len() < 1600 {
                                Some("literary_narrative")
                            } else {
                                None
                            };

                            if let Some(cat) = target_cat {
                                if is_usable_clause(
                                    &cl,
                                    &mut seen_clauses,
                                    &excluded_sentences,
                                    &lexicon_single_entries,
                                    &compiler,
                                ) {
                                    cat_buckets.get_mut(cat).unwrap().push(cl);
                                }
                            }
                        }
                        cur.clear();
                    }
                }
            }
        }
    }

    let per_cat_target = 1040usize; // 5 * 1040 = 5,200 句（每類各佔 20.0%，嚴格 <= 35%）
    let mut rng = XorShift64::new(20261021);
    let mut dev_items = Vec::new();
    let mut test_items = Vec::new();
    let mut cat_summary = BTreeMap::new();

    for (cat, list) in &mut cat_buckets {
        rng.shuffle(list);
        let selected: Vec<String> = list.iter().take(per_cat_target).cloned().collect();
        let dev_count = (selected.len() * 7) / 10;
        let test_count = selected.len() - dev_count;
        for (i, text) in selected.into_iter().enumerate() {
            if i < dev_count {
                dev_items.push(CorpusItem {
                    id: format!("dev_{}_{:04}", cat, i + 1),
                    category: cat.clone(),
                    text,
                });
            } else {
                test_items.push(CorpusItem {
                    id: format!("test_{}_{:04}", cat, i - dev_count + 1),
                    category: cat.clone(),
                    text,
                });
            }
        }
        cat_summary.insert(
            cat.clone(),
            json!({
                "total": dev_count + test_count,
                "dev": dev_count,
                "test": test_count,
                "share_percent": format!("{:.1}%", ((dev_count + test_count) as f64 / (per_cat_target * 5) as f64) * 100.0)
            }),
        );
    }

    let eval_dir = root.join("ruhi_lexicon").join("eval");
    let _ = fs::create_dir_all(&eval_dir);

    let mut dev_tsv = String::from("id\tsource_category\ttext\n");
    for item in &dev_items {
        dev_tsv.push_str(&format!("{}\t{}\t{}\n", item.id, item.category, item.text));
    }
    let mut test_tsv = String::from("id\tsource_category\ttext\n");
    for item in &test_items {
        test_tsv.push_str(&format!("{}\t{}\t{}\n", item.id, item.category, item.text));
    }

    let dev_path = eval_dir.join("dev.tsv");
    let test_path = eval_dir.join("test.tsv");
    fs::write(&dev_path, dev_tsv).expect("Failed to write dev.tsv");
    fs::write(&test_path, test_tsv).expect("Failed to write test.tsv");

    // README.md 由人工維護（如實記載語料來源與不公開原因），重建語料時不覆寫

    json!({
        "status": "ok",
        "total_sentences": dev_items.len() + test_items.len(),
        "dev_sentences": dev_items.len(),
        "test_sentences": test_items.len(),
        "categories": cat_summary,
        "dev_path": dev_path.to_string_lossy(),
        "test_path": test_path.to_string_lossy()
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = resolve_ruhi_root();
    let data_dir = root.join("ruhi_lexicon").join("data");
    let engine = RuhiEngine::new_with_data_dir(&data_dir.to_string_lossy());

    if args.iter().any(|a| a == "--build-corpus") {
        let res = build_and_seal_evaluation_corpus(&root, &engine);
        println!("{}", serde_json::to_string_pretty(&res).unwrap());
        return;
    }

    let mut set_name = "dev".to_string();
    let mut out_path: Option<PathBuf> = None;
    let mut baseline_path: Option<PathBuf> = None;
    let mut do_ablation = false;
    let mut prune_harmful = false;
    let mut do_calibrate = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--set" if i + 1 < args.len() => {
                set_name = args[i + 1].clone();
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                out_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--baseline" if i + 1 < args.len() => {
                baseline_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--ablation" => {
                do_ablation = true;
                i += 1;
            }
            "--prune-harmful" => {
                prune_harmful = true;
                i += 1;
            }
            "--calibrate" => {
                do_calibrate = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let corpus_path = root
        .join("ruhi_lexicon")
        .join("eval")
        .join(format!("{}.tsv", set_name));
    if !corpus_path.is_file() {
        eprintln!(
            "錯誤：找不到語料檔 {:?}，請先執行 `--build-corpus`。",
            corpus_path
        );
        std::process::exit(1);
    }

    let raw_items = load_corpus_tsv(&corpus_path);
    let compiler = PhoneticCompiler::from_lexicon(&engine.lexicon);
    let compiled: Vec<CompiledSentence> = raw_items
        .iter()
        .filter_map(|item| compiler.compile(item))
        .collect();

    if do_ablation {
        if set_name == "test" {
            eprintln!("錯誤：嚴禁在盲測集 (test.tsv) 上執行規則消融分析！");
            std::process::exit(1);
        }
        let rep = run_ablation_study(&engine, &compiled, prune_harmful, &root);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dataset": rep["dataset"],
                "baseline_cer_percent": rep["baseline_cer_percent"],
                "total_evaluated_rules": rep["total_evaluated_rules"],
                "summary": rep["summary"],
                "family_ablation": rep["family_ablation"],
                "top_positive_rules": rep["positive_contributor_rules"].as_array().map(|a| a.iter().take(15).cloned().collect::<Vec<_>>()),
                "negative_interference_rules": rep["negative_interference_rules"]
            }))
            .unwrap()
        );
        return;
    }

    if do_calibrate {
        if set_name == "test" {
            eprintln!("錯誤：嚴禁在盲測集 (test.tsv) 上執行權重校準！");
            std::process::exit(1);
        }
        let rep = run_perceptron_calibration(&engine, &compiled, &root);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        return;
    }

    // 評估語料不公開：輸出報告預設不含句子原文；僅 dev 集可加 `--with-text` 於本機做錯誤分析（test 集一律遮蔽）
    let with_text = args.iter().any(|a| a == "--with-text");
    let hide_text = set_name == "test" || !with_text;
    let mut report = evaluate_compiled_dataset(&engine, &compiled, &set_name, hide_text);

    if let Some(ref bp) = baseline_path {
        if let Ok(bs_str) = fs::read_to_string(bp) {
            if let Ok(base_rep) = serde_json::from_str::<EvalReport>(&bs_str) {
                let cmp = compare_with_baseline(&base_rep, &report, hide_text);
                report.mcnemar_comparison = Some(cmp);
            }
        }
    }

    if let Some(ref p) = out_path {
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(p, serde_json::to_string_pretty(&report).unwrap());
    }

    // 終端機輸出摘要（不含逐句明細，確保終端機乾淨且盲測集絕對保密）
    let summary = json!({
        "dataset": report.dataset,
        "total_sentences": report.total_sentences,
        "total_chars": report.total_chars,
        "total_char_errors": report.total_char_errors,
        "cer": report.cer,
        "cer_percent": report.cer_percent,
        "exact_match_sentences": report.exact_match_sentences,
        "sentence_accuracy": report.sentence_accuracy,
        "sentence_accuracy_percent": report.sentence_accuracy_percent,
        "polyphonic_ambiguity_audit": report.polyphonic_ambiguity_audit,
        "unreachable_gold_audit": report.unreachable_gold_audit,
        "latency_us": report.latency_us,
        "per_category": report.per_category,
        "mcnemar_comparison": report.mcnemar_comparison
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}
