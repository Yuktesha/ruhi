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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_homophone_char_errors: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcnemar_comparison: Option<Value>,
    pub sentence_results: Vec<SentenceEvalResult>,
}

/// 品質閘門基準彙總檔 (`eval_reports/dev_gate_baseline.json`)，不含任何句子原文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateBaselineSummary {
    pub commit: String,
    pub dataset: String,
    pub total_sentences: usize,
    pub total_chars: usize,
    pub total_char_errors: usize,
    pub cer: f64,
    pub cer_percent: String,
    pub exact_match_sentences: usize,
    pub sentence_accuracy: f64,
    pub sentence_accuracy_percent: String,
    pub per_category: BTreeMap<String, CategoryMetrics>,
}

/// 本機逐句正誤紀錄 (`eval_reports/local/*.json`)，僅記錄句子編號與正誤，不含原文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSentenceRecord {
    pub id: String,
    pub is_exact_match: bool,
    pub char_errors: usize,
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
            if (entry.word.chars().count() >= 2 && entry.weight < 90)
                || entry.category == "archaic"
                || entry.category == "classic"
                || entry.category == "rare"
                || entry.category == "moe_unattested2"
            {
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
    let mut char_confusion_counts: HashMap<(char, char), usize> = HashMap::new();

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
            top_homophone_char_errors: Vec::new(),
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
                let mut local_confusions: HashMap<(char, char), usize> = HashMap::new();
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

                    if !is_exact_match && pred.chars().count() == char_len {
                        for (p_ch, g_ch) in pred.chars().zip(gold.chars()) {
                            if p_ch != g_ch {
                                *local_confusions.entry((p_ch, g_ch)).or_insert(0) += 1;
                            }
                        }
                    }

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
                (local_out, local_confusions)
            }));
        }
        for h in handles {
            if let Ok((part, part_conf)) = h.join() {
                all_results.extend(part);
                for (pair, cnt) in part_conf {
                    *char_confusion_counts.entry(pair).or_insert(0) += cnt;
                }
            }
        }
    });

    let mut sorted_confusions: Vec<((char, char), usize)> =
        char_confusion_counts.into_iter().collect();
    sorted_confusions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top_homophone_char_errors: Vec<Value> = sorted_confusions
        .into_iter()
        .take(20)
        .map(|((wrong_ch, gold_ch), count)| {
            json!({
                "wrong_char": wrong_ch.to_string(),
                "gold_char": gold_ch.to_string(),
                "count": count
            })
        })
        .collect();

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
        top_homophone_char_errors,
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
        "construction_topological_core",
        "construction_trigram_slot",
        "geo_hierarchy",
        "onomastics",
        "construction_ctx31_bifurcation",
        "construction_boundary_guard",
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

const DERIVED_FAMILIES: [&str; 31] = [
    // Stratum 1: Glued Pronoun / Modal / Adverb / Neg-Copula Bigrams
    "pref2_pronoun",
    "pref2_modal_copula",
    "pref2_adverb",
    "suf2_neg_cop_exist",
    // Stratum 2: Particle-Attached Bigrams
    "suf2_de_struct",
    "suf2_de_comp",
    "suf2_di",
    "suf2_le",
    "suf2_zhe",
    "suf2_guo",
    "suf2_modal_particle",
    // Stratum 3: Negation / Existence / Demonstrative / Quantifier Prefixed Bigrams
    "pref2_bu",
    "pref2_mei",
    "pref2_you",
    "pref2_zai",
    "pref2_dem_quant",
    // Stratum 4: Verb-Complement / Directional Bigrams
    "suf2_dao",
    "suf2_chu",
    "suf2_lai_qu",
    // Stratum 5: Transparent 3-Char Function-Word Phrases
    "tri_xx_de",
    "tri_xx_aspect",
    "tri_x_de_y",
    "tri_x_le_y",
    "tri_bu_xx",
    "tri_mei_xx",
    "tri_you_xx",
    "tri_x_bu_y",
    "tri_x_de2_y",
    "tri_x_yi_y",
    // Stratum 6: Legacy High-Weight Multi-Char Outliers
    "outlier_wt_ge_800",
    "outlier_wt_ge_800_derived",
];

/// Classify a lexicon word (char len >= 2) into overlapping structural/derived families.
fn classify_derived_families(word: &str, max_weight: u32) -> Vec<&'static str> {
    let chars: Vec<char> = word.chars().collect();
    let len = chars.len();
    if len < 2 {
        return Vec::new();
    }

    let mut fams = Vec::new();
    if len == 2 {
        if matches!(chars[0], '我' | '你' | '妳' | '他' | '她' | '它' | '牠' | '祂')
            && chars[1] != '們'
        {
            fams.push("pref2_pronoun");
        }
        if matches!(chars[0], '要' | '會' | '能' | '可' | '該' | '是') {
            fams.push("pref2_modal_copula");
        }
        if matches!(
            chars[0],
            '很' | '太' | '最' | '更' | '都' | '也' | '就' | '才' | '再' | '又' | '還'
        ) {
            fams.push("pref2_adverb");
        }
        if matches!(chars[1], '不' | '沒' | '是' | '在' | '有') {
            fams.push("suf2_neg_cop_exist");
        }
        if chars[1] == '的' {
            fams.push("suf2_de_struct");
        }
        if chars[1] == '得' {
            fams.push("suf2_de_comp");
        }
        if chars[1] == '地' {
            fams.push("suf2_di");
        }
        if chars[1] == '了' {
            fams.push("suf2_le");
        }
        if chars[1] == '著' {
            fams.push("suf2_zhe");
        }
        if chars[1] == '過' {
            fams.push("suf2_guo");
        }
        if matches!(chars[1], '嗎' | '呢' | '吧' | '啊' | '呀' | '嘛' | '啦') {
            fams.push("suf2_modal_particle");
        }
        if chars[0] == '不' {
            fams.push("pref2_bu");
        }
        if chars[0] == '沒' {
            fams.push("pref2_mei");
        }
        if chars[0] == '有' {
            fams.push("pref2_you");
        }
        if chars[0] == '在' {
            fams.push("pref2_zai");
        }
        if matches!(chars[0], '這' | '那' | '哪' | '每' | '各' | '某' | '其' | '所') {
            fams.push("pref2_dem_quant");
        }
        if chars[1] == '到' {
            fams.push("suf2_dao");
        }
        if chars[1] == '出' {
            fams.push("suf2_chu");
        }
        if matches!(chars[1], '來' | '去') {
            fams.push("suf2_lai_qu");
        }
    } else if len == 3 {
        if chars[2] == '的' {
            fams.push("tri_xx_de");
        }
        if matches!(chars[2], '得' | '了' | '著' | '過') {
            fams.push("tri_xx_aspect");
        }
        if chars[1] == '的' {
            fams.push("tri_x_de_y");
        }
        if chars[1] == '了' {
            fams.push("tri_x_le_y");
        }
        if chars[0] == '不' {
            fams.push("tri_bu_xx");
        }
        if chars[0] == '沒' {
            fams.push("tri_mei_xx");
        }
        if chars[0] == '有' {
            fams.push("tri_you_xx");
        }
        if chars[1] == '不' {
            fams.push("tri_x_bu_y");
        }
        if chars[1] == '得' {
            fams.push("tri_x_de2_y");
        }
        if chars[1] == '一' {
            fams.push("tri_x_yi_y");
        }
    }

    let is_structural_derived = !fams.is_empty();
    if max_weight >= 800 {
        fams.push("outlier_wt_ge_800");
        if is_structural_derived {
            fams.push("outlier_wt_ge_800_derived");
        }
    }
    fams
}

struct RemovedWordSetMetrics {
    delta_char_errors: isize,
    ablated_cer: f64,
    delta_cer_pp: f64,
    fixed_sents: usize,
    broken_sents: usize,
    char_improved_sents: usize,
    char_degraded_sents: usize,
    mcnemar_chi_square: f64,
    mcnemar_p_value: f64,
    per_category_delta_char_errors: BTreeMap<String, isize>,
}

/// Filter multi-char nodes matching `remove_set` from a single sentence's lattice DAG while keeping all length-1 nodes intact.
fn filter_dag_excluding(
    dag: &HashMap<usize, Vec<ruhi::router::LatticeNode>>,
    remove_set: &HashSet<String>,
) -> HashMap<usize, Vec<ruhi::router::LatticeNode>> {
    let mut out = HashMap::with_capacity(dag.len());
    for (&pos, nodes) in dag {
        let filtered: Vec<ruhi::router::LatticeNode> = nodes
            .iter()
            .filter(|n| n.length() == 1 || !remove_set.contains(&n.word))
            .cloned()
            .collect();
        out.insert(pos, filtered);
    }
    out
}

/// Compute McNemar continuity-corrected chi-square and two-sided p-value.
fn compute_mcnemar(fixed_sents: usize, broken_sents: usize) -> (f64, f64) {
    let discord = fixed_sents + broken_sents;
    if discord == 0 {
        return (0.0, 1.0);
    }
    let diff = (fixed_sents as f64 - broken_sents as f64).abs();
    let num = (diff - 1.0).max(0.0);
    let chi_square = (num * num) / (discord as f64);
    let p_value = if chi_square > 0.0 {
        erfc_approx((chi_square / 2.0).sqrt())
    } else {
        1.0
    };
    (chi_square, p_value)
}

/// Evaluate removal of `remove_set` across only the affected `touched_sents` in parallel.
fn eval_removed_word_set(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    dags: &[HashMap<usize, Vec<ruhi::router::LatticeNode>>],
    base_errors: &[usize],
    base_exact: &[bool],
    total_chars: usize,
    base_total_errors: usize,
    base_cer: f64,
    remove_set: &HashSet<String>,
    touched_sents: &[usize],
) -> RemovedWordSetMetrics {
    if touched_sents.is_empty() || remove_set.is_empty() {
        return RemovedWordSetMetrics {
            delta_char_errors: 0,
            ablated_cer: base_cer,
            delta_cer_pp: 0.0,
            fixed_sents: 0,
            broken_sents: 0,
            char_improved_sents: 0,
            char_degraded_sents: 0,
            mcnemar_chi_square: 0.0,
            mcnemar_p_value: 1.0,
            per_category_delta_char_errors: BTreeMap::new(),
        };
    }

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);

    let eval_chunk = |chunk: &[usize],
                      trans: &mut ruhi::router::transitions::TransitionEngine|
     -> (isize, usize, usize, usize, usize, BTreeMap<String, isize>) {
        let mut diff_errs = 0isize;
        let mut fixed = 0usize;
        let mut broken = 0usize;
        let mut improved = 0usize;
        let mut degraded = 0usize;
        let mut cat_diff: BTreeMap<String, isize> = BTreeMap::new();

        for &s_idx in chunk {
            let cs = &compiled[s_idx];
            let f_dag = filter_dag_excluding(&dags[s_idx], remove_set);
            let (nodes, _) =
                base_engine
                    .router
                    .solve_best_path(cs.syllables.len(), &f_dag, Some(trans));
            let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
            let ablated_errs = char_levenshtein(&pred, &cs.item.text);
            let orig_errs = base_errors[s_idx];
            let d = (ablated_errs as isize) - (orig_errs as isize);
            if d != 0 {
                diff_errs += d;
                *cat_diff.entry(cs.item.category.clone()).or_insert(0) += d;
            }
            if !base_exact[s_idx] && ablated_errs == 0 {
                fixed += 1;
            } else if base_exact[s_idx] && ablated_errs > 0 {
                broken += 1;
            }
            if ablated_errs < orig_errs {
                improved += 1;
            } else if ablated_errs > orig_errs {
                degraded += 1;
            }
        }
        (diff_errs, fixed, broken, improved, degraded, cat_diff)
    };

    let (delta_char_errors, fixed_sents, broken_sents, char_improved_sents, char_degraded_sents, per_cat) =
        if touched_sents.len() <= 32 || n_threads <= 1 {
            let mut worker_trans = base_engine.transitions.clone();
            eval_chunk(touched_sents, &mut worker_trans)
        } else {
            let chunk_size = (touched_sents.len() + n_threads - 1) / n_threads.max(1);
            let parts: Vec<_> = std::thread::scope(|s| {
                let mut handles = Vec::new();
                for chunk in touched_sents.chunks(chunk_size) {
                    let mut worker_trans = base_engine.transitions.clone();
                    handles.push(s.spawn(move || eval_chunk(chunk, &mut worker_trans)));
                }
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            let mut delta = 0isize;
            let mut fixed = 0usize;
            let mut broken = 0usize;
            let mut improved = 0usize;
            let mut degraded = 0usize;
            let mut cat_diff: BTreeMap<String, isize> = BTreeMap::new();
            for (d, fx, brk, imp, deg, cd) in parts {
                delta += d;
                fixed += fx;
                broken += brk;
                improved += imp;
                degraded += deg;
                for (k, v) in cd {
                    *cat_diff.entry(k).or_insert(0) += v;
                }
            }
            (delta, fixed, broken, improved, degraded, cat_diff)
        };

    let ablated_total_errs = (base_total_errors as isize + delta_char_errors).max(0) as usize;
    let ablated_cer = ablated_total_errs as f64 / total_chars.max(1) as f64;
    let delta_cer_pp = (ablated_cer - base_cer) * 100.0;
    let (mcnemar_chi_square, mcnemar_p_value) = compute_mcnemar(fixed_sents, broken_sents);

    RemovedWordSetMetrics {
        delta_char_errors,
        ablated_cer,
        delta_cer_pp,
        fixed_sents,
        broken_sents,
        char_improved_sents,
        char_degraded_sents,
        mcnemar_chi_square,
        mcnemar_p_value,
        per_category_delta_char_errors: per_cat,
    }
}

fn family_verdict(m: &RemovedWordSetMetrics) -> &'static str {
    if m.delta_char_errors < 0 && m.broken_sents <= m.fixed_sents {
        "NET_BENEFICIAL_TO_REMOVE"
    } else if m.delta_char_errors == 0 && m.fixed_sents == m.broken_sents {
        "NEUTRAL_PRUNABLE"
    } else if m.delta_char_errors <= 10
        || (m.fixed_sents > 0 && m.broken_sents > 0 && m.fixed_sents * 2 >= m.broken_sents)
    {
        "MIXED_SELECTIVE"
    } else {
        "KEEP_OR_FACTORIZE_FIRST"
    }
}

/// 執行衍生詞庫系統性盤查、家族與單詞級離線因果消融暨 `multi_char_bonus` 靈敏度實驗 (`--lexicon-ablation`)
pub fn run_lexicon_ablation_study(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    root: &Path,
) -> Value {
    let t_start = Instant::now();
    println!(
        "[LexiconAblation] 正在掃描 lexicon_core.tsv 與 {} 句開發集 (dev.tsv) 之全晶格結構...",
        compiled.len()
    );

    // Layer 1a: Read lexicon_core.tsv for census
    let lex_path = root.join("ruhi_lexicon").join("data").join("lexicon_core.tsv");
    let lex_content = fs::read_to_string(&lex_path).expect("無法讀取 ruhi_lexicon/data/lexicon_core.tsv");
    let mut lex_max_weight: HashMap<String, u32> = HashMap::new();
    for line in lex_content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut cols = t.split('\t');
        let Some(w) = cols.next() else { continue };
        let Some(_bpm) = cols.next() else { continue };
        let Some(wt_str) = cols.next() else { continue };
        let Ok(wt) = wt_str.trim().parse::<u32>() else { continue };
        let entry = lex_max_weight.entry(w.to_string()).or_insert(0);
        if wt > *entry {
            *entry = wt;
        }
    }

    let mut fam_lex_total: HashMap<&'static str, usize> = HashMap::new();
    let mut fam_lex_active: HashMap<&'static str, usize> = HashMap::new();
    let mut derived_union_all_words = 0usize;
    let mut derived_union_active_words = 0usize;
    let mut outlier_800_all_words = 0usize;

    for (w, &wt) in &lex_max_weight {
        if w.chars().count() < 2 {
            continue;
        }
        let fams = classify_derived_families(w, wt);
        let mut has_1_to_29 = false;
        for &f in &fams {
            *fam_lex_total.entry(f).or_insert(0) += 1;
            if wt >= 90 {
                *fam_lex_active.entry(f).or_insert(0) += 1;
            }
            if f != "outlier_wt_ge_800" && f != "outlier_wt_ge_800_derived" {
                has_1_to_29 = true;
            }
        }
        if has_1_to_29 {
            derived_union_all_words += 1;
            if wt >= 90 {
                derived_union_active_words += 1;
            }
        }
        if wt >= 800 {
            outlier_800_all_words += 1;
        }
    }

    // Layer 1b: Build cached DAGs & compute baseline predictions in parallel
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    let per_sent_base: Vec<(
        HashMap<usize, Vec<ruhi::router::LatticeNode>>,
        usize,
        bool,
        usize,
        Vec<String>,
        Vec<String>,
    )> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size) {
            let dag_builder = LatticeDAG::new(&base_engine.lexicon);
            let mut worker_trans = base_engine.transitions.clone();
            handles.push(s.spawn(move || {
                let mut out = Vec::with_capacity(chunk.len());
                for cs in chunk {
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let (best_nodes, _) = base_engine.router.solve_best_path(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker_trans),
                    );
                    let pred: String = best_nodes.iter().map(|n| n.word.as_str()).collect();
                    let ch_len = cs.item.text.chars().count();
                    let errs = char_levenshtein(&pred, &cs.item.text);

                    let mut multi_in_dag: HashSet<String> = HashSet::new();
                    for nodes in dag.values() {
                        for n in nodes {
                            if n.length() >= 2 && n.word.chars().count() >= 2 {
                                multi_in_dag.insert(n.word.clone());
                            }
                        }
                    }
                    let best_multi: Vec<String> = best_nodes
                        .iter()
                        .filter(|n| n.length() >= 2 && n.word.chars().count() >= 2)
                        .map(|n| n.word.clone())
                        .collect();
                    out.push((
                        dag,
                        errs,
                        errs == 0,
                        ch_len,
                        multi_in_dag.into_iter().collect(),
                        best_multi,
                    ));
                }
                out
            }));
        }
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });

    let mut dags = Vec::with_capacity(compiled.len());
    let mut base_errors = Vec::with_capacity(compiled.len());
    let mut base_exact = Vec::with_capacity(compiled.len());
    let mut total_chars = 0usize;
    let mut base_total_errors = 0usize;
    let mut base_exact_count = 0usize;
    let mut base_per_cat: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();

    let mut word_to_touched_sents: HashMap<String, Vec<usize>> = HashMap::new();
    let mut word_best_hits: HashMap<String, usize> = HashMap::new();
    let mut word_error_hits: HashMap<String, usize> = HashMap::new();

    for (s_idx, (dag, errs, is_exact, ch_len, dag_words, best_multi)) in
        per_sent_base.into_iter().enumerate()
    {
        total_chars += ch_len;
        base_total_errors += errs;
        if is_exact {
            base_exact_count += 1;
        }
        let ce = base_per_cat
            .entry(compiled[s_idx].item.category.clone())
            .or_insert((0, 0, 0, 0));
        ce.0 += 1;
        ce.1 += ch_len;
        ce.2 += errs;
        if is_exact {
            ce.3 += 1;
        }

        for w in dag_words {
            word_to_touched_sents.entry(w).or_default().push(s_idx);
        }
        let mut seen_best = HashSet::new();
        for w in best_multi {
            if seen_best.insert(w.clone()) {
                *word_best_hits.entry(w.clone()).or_insert(0) += 1;
                if errs > 0 {
                    *word_error_hits.entry(w).or_insert(0) += 1;
                }
            }
        }
        dags.push(dag);
        base_errors.push(errs);
        base_exact.push(is_exact);
    }

    let base_cer = base_total_errors as f64 / total_chars.max(1) as f64;
    let base_ser = base_exact_count as f64 / compiled.len().max(1) as f64;

    // Index families over touched multi-char words in dev DAGs
    let mut family_to_words: HashMap<&'static str, HashSet<String>> = HashMap::new();
    let mut family_to_touched_set: HashMap<&'static str, HashSet<usize>> = HashMap::new();
    let mut word_families_cache: HashMap<String, Vec<&'static str>> = HashMap::new();

    for (w, sents) in &word_to_touched_sents {
        let wt = lex_max_weight.get(w).copied().unwrap_or(0);
        let fams = classify_derived_families(w, wt);
        if !fams.is_empty() {
            for &f in &fams {
                family_to_words.entry(f).or_default().insert(w.clone());
                let st = family_to_touched_set.entry(f).or_default();
                for &si in sents {
                    st.insert(si);
                }
            }
            word_families_cache.insert(w.clone(), fams);
        }
    }

    println!(
        "[LexiconAblation] 基準 CER {:.4}% ({} 字錯 / {} 句精準), 已觸及 {} 個多字詞與 {} 個衍生詞",
        base_cer * 100.0,
        base_total_errors,
        base_exact_count,
        word_to_touched_sents.len(),
        word_families_cache.len()
    );

    // Layer 2: Macro Family Ablation (all 31 families)
    let mut family_ablation = Vec::with_capacity(DERIVED_FAMILIES.len());
    for &fam in &DERIVED_FAMILIES {
        let remove_words = family_to_words.get(fam).cloned().unwrap_or_default();
        let mut touched_sents: Vec<usize> = family_to_touched_set
            .get(fam)
            .map(|st| st.iter().copied().collect())
            .unwrap_or_default();
        touched_sents.sort_unstable();

        let metrics = eval_removed_word_set(
            base_engine,
            compiled,
            &dags,
            &base_errors,
            &base_exact,
            total_chars,
            base_total_errors,
            base_cer,
            &remove_words,
            &touched_sents,
        );
        let verdict = family_verdict(&metrics);
        family_ablation.push(json!({
            "family": fam,
            "lexicon_total_words": fam_lex_total.get(fam).copied().unwrap_or(0),
            "lexicon_active_wt_ge_90": fam_lex_active.get(fam).copied().unwrap_or(0),
            "dev_touched_words": remove_words.len(),
            "dev_touched_sentences": touched_sents.len(),
            "delta_char_errors": metrics.delta_char_errors,
            "ablated_cer": round4_f64(metrics.ablated_cer * 100.0),
            "ablated_cer_percent": format!("{:.4}%", metrics.ablated_cer * 100.0),
            "delta_cer_percentage_points": format!("{:+.4}%", metrics.delta_cer_pp),
            "fixed_sentences": metrics.fixed_sents,
            "broken_sentences": metrics.broken_sents,
            "char_improved_sentences": metrics.char_improved_sents,
            "char_degraded_sentences": metrics.char_degraded_sents,
            "mcnemar_chi_square": round4_f64(metrics.mcnemar_chi_square),
            "mcnemar_p_value": metrics.mcnemar_p_value,
            "per_category_delta_char_errors": metrics.per_category_delta_char_errors,
            "verdict": verdict
        }));
    }

    // Layer 3: Micro Word-Level Leave-One-Out (LOO) Causal Attribution
    #[derive(Clone)]
    struct LooCandidate {
        word: String,
        char_len: usize,
        weight: u32,
        is_derived_1_29: bool,
        families: Vec<&'static str>,
        touched_sents: Vec<usize>,
        best_hits: usize,
        error_hits: usize,
    }

    #[derive(Clone, serde::Serialize)]
    struct LooResult {
        word: String,
        char_len: usize,
        lexicon_weight: u32,
        is_derived: bool,
        families: Vec<&'static str>,
        touched_sents: usize,
        best_path_hits: usize,
        error_path_hits: usize,
        delta_char_errors: isize,
        fixed_sents: usize,
        broken_sents: usize,
        char_improved_sents: usize,
        char_degraded_sents: usize,
    }

    let mut loo_candidates: Vec<LooCandidate> = Vec::new();
    for (w, sents) in &word_to_touched_sents {
        let wt = lex_max_weight.get(w).copied().unwrap_or(0);
        let fams = word_families_cache.get(w).cloned().unwrap_or_default();
        let best_hits = word_best_hits.get(w).copied().unwrap_or(0);
        let error_hits = word_error_hits.get(w).copied().unwrap_or(0);
        let is_derived_1_29 = fams
            .iter()
            .any(|&f| f != "outlier_wt_ge_800" && f != "outlier_wt_ge_800_derived");
        let include = !fams.is_empty() || error_hits >= 1;
        if include {
            loo_candidates.push(LooCandidate {
                word: w.clone(),
                char_len: w.chars().count(),
                weight: wt,
                is_derived_1_29,
                families: fams,
                touched_sents: sents.clone(),
                best_hits,
                error_hits,
            });
        }
    }
    loo_candidates.sort_by(|a, b| a.word.cmp(&b.word));
    println!(
        "[LexiconAblation] 正在平行執行 {} 個候選詞之單詞級 Leave-One-Out 因果驗證...",
        loo_candidates.len()
    );

    let loo_chunk = (loo_candidates.len() + n_threads - 1) / n_threads.max(1);
    let dags_ref = dags.as_slice();
    let base_errors_ref = base_errors.as_slice();
    let base_exact_ref = base_exact.as_slice();
    let loo_results: Vec<LooResult> = if loo_candidates.is_empty() {
        Vec::new()
    } else {
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for chunk in loo_candidates.chunks(loo_chunk) {
                let mut worker_trans = base_engine.transitions.clone();
                handles.push(s.spawn(move || {
                    let mut out = Vec::with_capacity(chunk.len());
                    for cand in chunk {
                        let mut d_errs = 0isize;
                        let mut fixed = 0usize;
                        let mut broken = 0usize;
                        let mut imp = 0usize;
                        let mut deg = 0usize;
                        for &s_idx in &cand.touched_sents {
                            let cs = &compiled[s_idx];
                            let mut f_dag = HashMap::with_capacity(dags_ref[s_idx].len());
                            for (&pos, nodes) in &dags_ref[s_idx] {
                                let kept: Vec<ruhi::router::LatticeNode> = nodes
                                    .iter()
                                    .filter(|n| n.length() == 1 || n.word != cand.word)
                                    .cloned()
                                    .collect();
                                f_dag.insert(pos, kept);
                            }
                            let (nodes, _) = base_engine.router.solve_best_path(
                                cs.syllables.len(),
                                &f_dag,
                                Some(&mut worker_trans),
                            );
                            let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                            let ab_e = char_levenshtein(&pred, &cs.item.text);
                            let orig_e = base_errors_ref[s_idx];
                            d_errs += (ab_e as isize) - (orig_e as isize);
                            if !base_exact_ref[s_idx] && ab_e == 0 {
                                fixed += 1;
                            } else if base_exact_ref[s_idx] && ab_e > 0 {
                                broken += 1;
                            }
                            if ab_e < orig_e {
                                imp += 1;
                            } else if ab_e > orig_e {
                                deg += 1;
                            }
                        }
                        out.push(LooResult {
                            word: cand.word.clone(),
                            char_len: cand.char_len,
                            lexicon_weight: cand.weight,
                            is_derived: cand.is_derived_1_29,
                            families: cand.families.clone(),
                            touched_sents: cand.touched_sents.len(),
                            best_path_hits: cand.best_hits,
                            error_path_hits: cand.error_hits,
                            delta_char_errors: d_errs,
                            fixed_sents: fixed,
                            broken_sents: broken,
                            char_improved_sents: imp,
                            char_degraded_sents: deg,
                        });
                    }
                    out
                }));
            }
            handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
        })
    };

    // Aggregate LOO statistics
    let mut derived_harmful_count = 0usize;
    let mut derived_strict_harmful_count = 0usize;
    let mut derived_neutral_count = 0usize;
    let mut derived_helpful_count = 0usize;

    let mut derived_harmful: Vec<&LooResult> = Vec::new();
    let mut derived_helpful: Vec<&LooResult> = Vec::new();
    let mut non_derived_harmful: Vec<&LooResult> = Vec::new();

    for r in &loo_results {
        if r.is_derived {
            if r.delta_char_errors < 0 {
                derived_harmful_count += 1;
                if r.broken_sents == 0 {
                    derived_strict_harmful_count += 1;
                }
                derived_harmful.push(r);
            } else if r.delta_char_errors > 0 {
                derived_helpful_count += 1;
                derived_helpful.push(r);
            } else {
                derived_neutral_count += 1;
            }
        } else if r.delta_char_errors < 0 {
            non_derived_harmful.push(r);
        }
    }

    derived_harmful.sort_by(|a, b| {
        a.delta_char_errors
            .cmp(&b.delta_char_errors)
            .then_with(|| a.broken_sents.cmp(&b.broken_sents))
            .then_with(|| b.fixed_sents.cmp(&a.fixed_sents))
            .then_with(|| a.word.cmp(&b.word))
    });
    derived_helpful.sort_by(|a, b| {
        b.delta_char_errors
            .cmp(&a.delta_char_errors)
            .then_with(|| b.broken_sents.cmp(&a.broken_sents))
            .then_with(|| a.word.cmp(&b.word))
    });
    non_derived_harmful.sort_by(|a, b| {
        a.delta_char_errors
            .cmp(&b.delta_char_errors)
            .then_with(|| a.broken_sents.cmp(&b.broken_sents))
            .then_with(|| b.fixed_sents.cmp(&a.fixed_sents))
            .then_with(|| a.word.cmp(&b.word))
    });

    // Per-family LOO breakdown across all 31 families
    let mut per_family_loo = Vec::with_capacity(DERIVED_FAMILIES.len());
    for &fam in &DERIVED_FAMILIES {
        let mut mems: Vec<&LooResult> = loo_results
            .iter()
            .filter(|r| r.families.contains(&fam))
            .collect();
        let mut h_cnt = 0usize;
        let mut k_cnt = 0usize;
        let mut n_cnt = 0usize;
        let mut sum_harm_d = 0isize;
        let mut sum_help_d = 0isize;
        for &m in &mems {
            if m.delta_char_errors < 0 {
                h_cnt += 1;
                sum_harm_d += m.delta_char_errors;
            } else if m.delta_char_errors > 0 {
                k_cnt += 1;
                sum_help_d += m.delta_char_errors;
            } else {
                n_cnt += 1;
            }
        }
        mems.sort_by(|a, b| a.delta_char_errors.cmp(&b.delta_char_errors));
        let top5_harm: Vec<Value> = mems
            .iter()
            .filter(|m| m.delta_char_errors < 0)
            .take(5)
            .map(|m| json!({"word": m.word, "weight": m.lexicon_weight, "delta_char_errors": m.delta_char_errors, "fixed": m.fixed_sents, "broken": m.broken_sents}))
            .collect();
        mems.sort_by(|a, b| b.delta_char_errors.cmp(&a.delta_char_errors));
        let top5_help: Vec<Value> = mems
            .iter()
            .filter(|m| m.delta_char_errors > 0)
            .take(5)
            .map(|m| json!({"word": m.word, "weight": m.lexicon_weight, "delta_char_errors": m.delta_char_errors, "fixed": m.fixed_sents, "broken": m.broken_sents}))
            .collect();

        per_family_loo.push(json!({
            "family": fam,
            "harmful_word_count": h_cnt,
            "helpful_word_count": k_cnt,
            "neutral_word_count": n_cnt,
            "sum_harmful_delta_errors": sum_harm_d,
            "sum_helpful_delta_errors": sum_help_d,
            "top_5_harmful_words": top5_harm,
            "top_5_helpful_words": top5_help,
        }));
    }

    // Layer 4: Joint Prune Simulations
    let build_joint_sim = |name: &str, words: HashSet<String>| -> Value {
        let mut tset: HashSet<usize> = HashSet::new();
        for w in &words {
            if let Some(sents) = word_to_touched_sents.get(w) {
                for &si in sents {
                    tset.insert(si);
                }
            }
        }
        let mut tvec: Vec<usize> = tset.into_iter().collect();
        tvec.sort_unstable();
        let m = eval_removed_word_set(
            base_engine,
            compiled,
            &dags,
            &base_errors,
            &base_exact,
            total_chars,
            base_total_errors,
            base_cer,
            &words,
            &tvec,
        );
        json!({
            "simulation_name": name,
            "removed_word_count": words.len(),
            "touched_sentences": tvec.len(),
            "delta_char_errors": m.delta_char_errors,
            "ablated_cer_percent": format!("{:.4}%", m.ablated_cer * 100.0),
            "delta_cer_percentage_points": format!("{:+.4}%", m.delta_cer_pp),
            "exact_match_sentences": (base_exact_count as isize + m.fixed_sents as isize - m.broken_sents as isize),
            "fixed_sentences": m.fixed_sents,
            "broken_sentences": m.broken_sents,
            "char_improved_sentences": m.char_improved_sents,
            "char_degraded_sentences": m.char_degraded_sents,
            "mcnemar_chi_square": round4_f64(m.mcnemar_chi_square),
            "mcnemar_p_value": m.mcnemar_p_value,
            "per_category_delta_char_errors": m.per_category_delta_char_errors,
        })
    };

    let s_strict_harm_der: HashSet<String> = loo_results
        .iter()
        .filter(|r| r.is_derived && r.delta_char_errors < 0 && r.broken_sents == 0)
        .map(|r| r.word.clone())
        .collect();
    let s_all_harm_der: HashSet<String> = loo_results
        .iter()
        .filter(|r| r.is_derived && r.delta_char_errors < 0)
        .map(|r| r.word.clone())
        .collect();
    let s_strict_harm_all: HashSet<String> = loo_results
        .iter()
        .filter(|r| r.delta_char_errors < 0 && r.broken_sents == 0)
        .map(|r| r.word.clone())
        .collect();
    let s_all_harm_all: HashSet<String> = loo_results
        .iter()
        .filter(|r| r.delta_char_errors < 0)
        .map(|r| r.word.clone())
        .collect();
    let s_neutral_der: HashSet<String> = loo_results
        .iter()
        .filter(|r| r.is_derived && r.delta_char_errors == 0 && r.touched_sents > 0)
        .map(|r| r.word.clone())
        .collect();

    let joint_simulations = vec![
        build_joint_sim("strict_harmful_derived", s_strict_harm_der),
        build_joint_sim("all_harmful_derived", s_all_harm_der),
        build_joint_sim("strict_harmful_all", s_strict_harm_all),
        build_joint_sim("all_harmful_all", s_all_harm_all),
        build_joint_sim("neutral_derived_touched", s_neutral_der),
    ];

    // Layer 5: multi_char_bonus Sensitivity Curve
    let bonus_values = [28.0_f64, 24.0, 20.0, 16.0, 12.0, 8.0, 4.0, 0.0];
    let mut bonus_curve = Vec::with_capacity(bonus_values.len());
    for &bval in &bonus_values {
        let mut test_router = base_engine.router.clone();
        test_router.multi_char_bonus = bval;

        let part_res: Vec<Vec<(usize, bool, usize, String)>> = std::thread::scope(|s| {
            let mut handles = Vec::new();
            for (c_chunk, d_chunk) in compiled.chunks(chunk_size).zip(dags.chunks(chunk_size)) {
                let mut worker_trans = base_engine.transitions.clone();
                let r_ref = &test_router;
                handles.push(s.spawn(move || {
                    let mut local = Vec::with_capacity(c_chunk.len());
                    for (cs, dag) in c_chunk.iter().zip(d_chunk.iter()) {
                        let (nodes, _) = r_ref.solve_best_path(
                            cs.syllables.len(),
                            dag,
                            Some(&mut worker_trans),
                        );
                        let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                        let e = char_levenshtein(&pred, &cs.item.text);
                        local.push((
                            e,
                            e == 0,
                            cs.item.text.chars().count(),
                            cs.item.category.clone(),
                        ));
                    }
                    local
                }));
            }
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let mut tot_e = 0usize;
        let mut tot_ex = 0usize;
        let mut cat_m: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
        for part in part_res {
            for (e, ex, clen, cat) in part {
                tot_e += e;
                if ex {
                    tot_ex += 1;
                }
                let ce = cat_m.entry(cat).or_insert((0, 0, 0, 0));
                ce.0 += 1;
                ce.1 += clen;
                ce.2 += e;
                if ex {
                    ce.3 += 1;
                }
            }
        }
        let cer_v = tot_e as f64 / total_chars.max(1) as f64;
        let ser_v = tot_ex as f64 / compiled.len().max(1) as f64;
        let mut per_cat_json = serde_json::Map::new();
        for (cat, (sents, clen, ce, cex)) in cat_m {
            let ccer = ce as f64 / clen.max(1) as f64;
            per_cat_json.insert(
                cat,
                json!({
                    "cer": round4_f64(ccer),
                    "cer_percent": format!("{:.2}%", ccer * 100.0),
                    "char_errors": ce,
                    "exact_match": cex,
                    "sentences": sents
                }),
            );
        }
        bonus_curve.push(json!({
            "multi_char_bonus": bval,
            "total_char_errors": tot_e,
            "delta_char_errors_vs_28": (tot_e as isize) - (base_total_errors as isize),
            "cer": round4_f64(cer_v),
            "cer_percent": format!("{:.2}%", cer_v * 100.0),
            "exact_match_sentences": tot_ex,
            "sentence_accuracy_percent": format!("{:.2}%", ser_v * 100.0),
            "per_category": per_cat_json,
        }));
    }

    let elapsed_secs = t_start.elapsed().as_secs_f64();
    let mut base_cat_json = serde_json::Map::new();
    for (cat, (sents, clen, ce, cex)) in base_per_cat {
        let ccer = ce as f64 / clen.max(1) as f64;
        base_cat_json.insert(
            cat,
            json!({
                "sentences": sents,
                "chars": clen,
                "char_errors": ce,
                "cer": round4_f64(ccer),
                "cer_percent": format!("{:.2}%", ccer * 100.0),
                "exact_match": cex,
            }),
        );
    }
    let baseline_json = json!({
        "dataset": "dev",
        "total_sentences": compiled.len(),
        "total_chars": total_chars,
        "total_char_errors": base_total_errors,
        "cer": round4_f64(base_cer),
        "cer_percent": format!("{:.2}%", base_cer * 100.0),
        "exact_match_sentences": base_exact_count,
        "sentence_accuracy_percent": format!("{:.2}%", base_ser * 100.0),
        "per_category": base_cat_json,
    });
    let census_summary = json!({
        "lexicon_unique_words": lex_max_weight.len(),
        "derived_families_1_to_29_total_words": derived_union_all_words,
        "derived_families_1_to_29_active_wt_ge_90": derived_union_active_words,
        "outlier_wt_ge_800_words": outlier_800_all_words,
        "dev_touched_multi_char_words": word_to_touched_sents.len(),
        "dev_touched_classified_words": word_families_cache.len(),
        "loo_candidates_evaluated": loo_results.len(),
    });
    let loo_totals = json!({
        "evaluated_candidates": loo_results.len(),
        "derived_harmful_count": derived_harmful_count,
        "derived_strict_harmful_count": derived_strict_harmful_count,
        "derived_neutral_count": derived_neutral_count,
        "derived_helpful_count": derived_helpful_count,
        "non_derived_harmful_count": non_derived_harmful.len(),
    });

    let top_60_helpful: Vec<&&LooResult> = derived_helpful.iter().take(60).collect();
    let top_100_non_derived_harmful: Vec<&&LooResult> = non_derived_harmful.iter().take(100).collect();

    let compact_family_table: Vec<Value> = family_ablation
        .iter()
        .map(|f| {
            json!({
                "family": f["family"],
                "lex_active": f["lexicon_active_wt_ge_90"],
                "touched_words": f["dev_touched_words"],
                "touched_sents": f["dev_touched_sentences"],
                "delta_errs": f["delta_char_errors"],
                "fixed": f["fixed_sentences"],
                "broken": f["broken_sentences"],
                "verdict": f["verdict"],
            })
        })
        .collect();

    let stdout_summary = json!({
        "elapsed_seconds": (elapsed_secs * 100.0).round() / 100.0,
        "baseline": baseline_json,
        "census": census_summary,
        "family_ablation_summary": compact_family_table,
        "loo_totals": loo_totals,
        "joint_simulations": joint_simulations,
        "multi_char_bonus_curve": bonus_curve,
    });

    json!({
        "elapsed_seconds": (elapsed_secs * 100.0).round() / 100.0,
        "baseline": baseline_json,
        "census": census_summary,
        "family_ablation": family_ablation,
        "per_family_loo": per_family_loo,
        "loo_totals": loo_totals,
        "all_harmful_derived_words": derived_harmful,
        "top_60_helpful_derived_words": top_60_helpful,
        "all_harmful_non_derived_words": top_100_non_derived_harmful,
        "joint_simulations": joint_simulations,
        "multi_char_bonus_curve": bonus_curve,
        "stdout_summary": stdout_summary,
    })
}

/// 執行跨策略控制對照統計 (`--strategy-bench`)：比較 S0..S6 各方案於現代白話、5-Fold 跨折泛化、文言文與新創實戰之利弊得失
pub fn run_strategy_benchmark(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    compiler: &PhoneticCompiler,
    root: &Path,
) -> Value {
    let t_start = Instant::now();
    println!(
        "[StrategyBench] 正在建立基準晶格與診斷探針（dev {} 句 + 文言文 20 句 + 新創/實戰 15 句）...",
        compiled.len()
    );

    let classical_texts = [
        "知之為知之不知為不知是知也",
        "道可道非常道名可名非常名",
        "先天下之憂而憂後天下之樂而樂",
        "古之學者必有師",
        "師者所以傳道受業解惑也",
        "不以物喜不以己悲",
        "醉翁之意不在酒在乎山水之間也",
        "予獨愛蓮之出淤泥而不染",
        "三人行必有我師焉",
        "溫故而知新可以為師矣",
        "生於憂患而死於安樂",
        "天將降大任於斯人也",
        "臣本布衣躬耕於南陽",
        "己所不欲勿施於人",
        "君子和而不同小人同而不和",
        "水能載舟亦能覆舟",
        "民為貴社稷次之君為輕",
        "千里之行始於足下",
        "工欲善其事必先利其器",
        "欲窮千里目更上一層樓",
    ];
    let novel_texts = [
        "一個有辦法打文言文的輸入法",
        "文言文輸入不出錯",
        "不過是該漢字的常見連結",
        "漢字系統的原型",
        "先跑一下統計以確定利弊得失",
        "每一個漢字本身就是一個語意本體",
        "憑感覺的事情",
        "平均值的計算",
        "令人不捨的行為",
        "台灣省的時代",
        "他樂得清閒",
        "我也很禮貌的回答",
        "量子計算機與神經形態晶片",
        "跨域語素因式分解拓撲模型",
        "低延遲本地端解碼引擎",
    ];

    let compile_probe = |texts: &[&str], cat: &str| -> Vec<CompiledSentence> {
        texts
            .iter()
            .enumerate()
            .filter_map(|(idx, &txt)| {
                let item = CorpusItem {
                    id: format!("{}_{:02}", cat, idx + 1),
                    category: cat.to_string(),
                    text: txt.to_string(),
                };
                compiler.compile(&item)
            })
            .collect()
    };

    let classical_compiled = compile_probe(&classical_texts, "classical");
    let novel_compiled = compile_probe(&novel_texts, "novel");

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    // Build dev DAGs + baseline in parallel
    let base_dev_parts: Vec<(
        HashMap<usize, Vec<ruhi::router::LatticeNode>>,
        usize,
        bool,
        usize,
        Vec<String>,
        Vec<String>,
    )> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size) {
            let dag_builder = LatticeDAG::new(&base_engine.lexicon);
            let mut worker_trans = base_engine.transitions.clone();
            handles.push(s.spawn(move || {
                let mut out = Vec::with_capacity(chunk.len());
                for cs in chunk {
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let (best_nodes, _) = base_engine.router.solve_best_path(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker_trans),
                    );
                    let pred: String = best_nodes.iter().map(|n| n.word.as_str()).collect();
                    let ch_len = cs.item.text.chars().count();
                    let errs = char_levenshtein(&pred, &cs.item.text);

                    let mut multi_in_dag: HashSet<String> = HashSet::new();
                    for nodes in dag.values() {
                        for n in nodes {
                            if n.length() >= 2 && n.word.chars().count() >= 2 {
                                multi_in_dag.insert(n.word.clone());
                            }
                        }
                    }
                    let best_multi: Vec<String> = best_nodes
                        .iter()
                        .filter(|n| n.length() >= 2 && n.word.chars().count() >= 2)
                        .map(|n| n.word.clone())
                        .collect();
                    out.push((
                        dag,
                        errs,
                        errs == 0,
                        ch_len,
                        multi_in_dag.into_iter().collect(),
                        best_multi,
                    ));
                }
                out
            }));
        }
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });

    let mut dags = Vec::with_capacity(compiled.len());
    let mut base_errors = Vec::with_capacity(compiled.len());
    let mut base_exact = Vec::with_capacity(compiled.len());
    let mut total_chars = 0usize;
    let mut base_total_errors = 0usize;
    let mut base_exact_count = 0usize;
    let mut word_to_touched_sents: HashMap<String, Vec<usize>> = HashMap::new();
    let mut word_error_hits: HashMap<String, usize> = HashMap::new();

    for (s_idx, (dag, errs, is_exact, ch_len, dag_words, best_multi)) in
        base_dev_parts.into_iter().enumerate()
    {
        total_chars += ch_len;
        base_total_errors += errs;
        if is_exact {
            base_exact_count += 1;
        }
        for w in dag_words {
            word_to_touched_sents.entry(w).or_default().push(s_idx);
        }
        let mut seen_best = HashSet::new();
        for w in best_multi {
            if seen_best.insert(w.clone()) && errs > 0 {
                *word_error_hits.entry(w).or_insert(0) += 1;
            }
        }
        dags.push(dag);
        base_errors.push(errs);
        base_exact.push(is_exact);
    }
    let base_cer = base_total_errors as f64 / total_chars.max(1) as f64;

    // Build probe DAGs once
    let dag_builder_probe = LatticeDAG::new(&base_engine.lexicon);
    let classical_dags: Vec<_> = classical_compiled
        .iter()
        .map(|cs| dag_builder_probe.build_dag(&cs.syllables, true))
        .collect();
    let novel_dags: Vec<_> = novel_compiled
        .iter()
        .map(|cs| dag_builder_probe.build_dag(&cs.syllables, true))
        .collect();

    // Load lexicon_core.tsv unique word max weights to construct S1 / S1c structural sets
    let lex_path = root.join("ruhi_lexicon").join("data").join("lexicon_core.tsv");
    let lex_content = fs::read_to_string(&lex_path).expect("無法讀取 lexicon_core.tsv");
    let mut lex_max_weight: HashMap<String, u32> = HashMap::new();
    for line in lex_content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut cols = t.split('\t');
        let Some(w) = cols.next() else { continue };
        let Some(_bpm) = cols.next() else { continue };
        let Some(wt_str) = cols.next() else { continue };
        let Ok(wt) = wt_str.trim().parse::<u32>() else { continue };
        let e = lex_max_weight.entry(w.to_string()).or_insert(0);
        if wt > *e {
            *e = wt;
        }
    }

    let atomic_suf2_de_nouns: HashSet<&str> = ["目的", "標的", "別的", "有的", "是的"]
        .into_iter()
        .collect();
    let truncated_frag_words: HashSet<&str> = ["沒辦", "沒關", "沒想"].into_iter().collect();

    let mut all_derived_1_29: HashSet<String> = HashSet::new();
    let mut transparent_derived_narrow: HashSet<String> = HashSet::new();
    let mut s1a_remove_set: HashSet<String> = HashSet::new();

    for (w, &wt) in &lex_max_weight {
        if w.chars().count() < 2 {
            continue;
        }
        let fams = classify_derived_families(w, wt);
        if fams
            .iter()
            .any(|&f| f != "outlier_wt_ge_800" && f != "outlier_wt_ge_800_derived")
        {
            all_derived_1_29.insert(w.clone());
        }
        let is_s1a = fams.iter().any(|&f| {
            matches!(
                f,
                "outlier_wt_ge_800_derived"
                    | "suf2_modal_particle"
                    | "tri_x_de2_y"
                    | "tri_x_le_y"
                    | "tri_xx_de"
                    | "tri_xx_aspect"
            )
        });
        if is_s1a {
            s1a_remove_set.insert(w.clone());
        }
        let is_narrow = is_s1a
            || truncated_frag_words.contains(w.as_str())
            || fams.contains(&"pref2_pronoun")
            || (fams.contains(&"suf2_de_struct") && !atomic_suf2_de_nouns.contains(w.as_str()));
        if is_narrow {
            transparent_derived_narrow.insert(w.clone());
        }
    }
    let s1b_remove_set = transparent_derived_narrow.clone();
    let all_derived_arc = std::sync::Arc::new(all_derived_1_29);
    let narrow_derived_arc = std::sync::Arc::new(transparent_derived_narrow);

    let dags_ref = dags.as_slice();
    let base_errors_ref = base_errors.as_slice();
    let base_exact_ref = base_exact.as_slice();

    // Helper: evaluate a strategy across dev.tsv + Classical-20 + Novel-15
    let filter_dag_strat = |dag: &HashMap<usize, Vec<ruhi::router::LatticeNode>>,
                            n_syls: usize,
                            rm: Option<&HashSet<String>>,
                            min_l2: Option<u32>|
     -> HashMap<usize, Vec<ruhi::router::LatticeNode>> {
        let mut out = HashMap::with_capacity(dag.len());
        for (&pos, nodes) in dag {
            let kept: Vec<ruhi::router::LatticeNode> = nodes
                .iter()
                .filter(|n| {
                    if n.length() == 1 {
                        return true;
                    }
                    if let Some(rset) = rm {
                        if rset.contains(&n.word) {
                            return false;
                        }
                    }
                    if let Some(mw) = min_l2 {
                        if n_syls >= 3 && n.length() == 2 && n.weight < mw {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();
            out.insert(pos, kept);
        }
        out
    };

    let eval_probe_set = |router: &ruhi::router::ViterbiRouter,
                          probes: &[CompiledSentence],
                          p_dags: &[HashMap<usize, Vec<ruhi::router::LatticeNode>>],
                          rm: Option<&HashSet<String>>,
                          min_l2: Option<u32>|
     -> (usize, usize, usize, f64, Vec<Value>) {
        let mut trans = base_engine.transitions.clone();
        let mut tot_ch = 0usize;
        let mut tot_err = 0usize;
        let mut exact = 0usize;
        let mut preds = Vec::with_capacity(probes.len());
        for (cs, dag) in probes.iter().zip(p_dags.iter()) {
            let (nodes, _) = if rm.is_some() || min_l2.is_some() {
                let fd = filter_dag_strat(dag, cs.syllables.len(), rm, min_l2);
                router.solve_best_path(cs.syllables.len(), &fd, Some(&mut trans))
            } else {
                router.solve_best_path(cs.syllables.len(), dag, Some(&mut trans))
            };
            let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
            let ch_len = cs.item.text.chars().count();
            let errs = char_levenshtein(&pred, &cs.item.text);
            tot_ch += ch_len;
            tot_err += errs;
            if errs == 0 {
                exact += 1;
            }
            preds.push(json!({
                "id": cs.item.id,
                "gold": cs.item.text,
                "pred": pred,
                "char_errors": errs,
            }));
        }
        let cer = tot_err as f64 / tot_ch.max(1) as f64;
        (tot_err, tot_ch, exact, cer, preds)
    };

    let eval_strategy_full = |name: &str,
                              group: &str,
                              router: &ruhi::router::ViterbiRouter,
                              rm: Option<&HashSet<String>>,
                              min_l2: Option<u32>,
                              include_probe_details: bool|
     -> Value {
        let part_res: Vec<Vec<(usize, bool, bool, bool, isize, String)>> = std::thread::scope(|s| {
            let mut handles = Vec::new();
            for (chunk_idx, c_chunk) in compiled.chunks(chunk_size).enumerate() {
                let offset = chunk_idx * chunk_size;
                let mut worker_trans = base_engine.transitions.clone();
                handles.push(s.spawn(move || {
                    let mut local = Vec::with_capacity(c_chunk.len());
                    for (local_i, cs) in c_chunk.iter().enumerate() {
                        let s_idx = offset + local_i;
                        let orig_dag = &dags_ref[s_idx];
                        let (nodes, _) = if rm.is_some() || min_l2.is_some() {
                            let fd = filter_dag_strat(orig_dag, cs.syllables.len(), rm, min_l2);
                            router.solve_best_path(cs.syllables.len(), &fd, Some(&mut worker_trans))
                        } else {
                            router.solve_best_path(
                                cs.syllables.len(),
                                orig_dag,
                                Some(&mut worker_trans),
                            )
                        };
                        let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                        let errs = char_levenshtein(&pred, &cs.item.text);
                        let orig_e = base_errors_ref[s_idx];
                        let orig_ex = base_exact_ref[s_idx];
                        let fixed = !orig_ex && errs == 0;
                        let broken = orig_ex && errs > 0;
                        let d_err = (errs as isize) - (orig_e as isize);
                        local.push((
                            errs,
                            errs == 0,
                            fixed,
                            broken,
                            d_err,
                            cs.item.category.clone(),
                        ));
                    }
                    local
                }));
            }
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let mut tot_e = 0usize;
        let mut tot_ex = 0usize;
        let mut fixed = 0usize;
        let mut broken = 0usize;
        let mut cat_stats: BTreeMap<String, (usize, isize)> = BTreeMap::new();
        for part in part_res {
            for (e, is_ex, fx, brk, d_e, cat) in part {
                tot_e += e;
                if is_ex {
                    tot_ex += 1;
                }
                if fx {
                    fixed += 1;
                }
                if brk {
                    broken += 1;
                }
                let ce = cat_stats.entry(cat).or_insert((0, 0));
                ce.0 += e;
                ce.1 += d_e;
            }
        }
        let delta_err = (tot_e as isize) - (base_total_errors as isize);
        let cer = tot_e as f64 / total_chars.max(1) as f64;
        let delta_cer_pp = (cer - base_cer) * 100.0;
        let (_, mcnemar_p) = compute_mcnemar(fixed, broken);

        let mut per_cat_delta = BTreeMap::new();
        for (cat, (_ce, cd)) in cat_stats {
            per_cat_delta.insert(cat, cd);
        }

        let (cl_err, cl_ch, cl_ex, cl_cer, cl_preds) =
            eval_probe_set(router, &classical_compiled, &classical_dags, rm, min_l2);
        let (nv_err, nv_ch, nv_ex, nv_cer, nv_preds) =
            eval_probe_set(router, &novel_compiled, &novel_dags, rm, min_l2);

        let mut obj = json!({
            "strategy": name,
            "group": group,
            "dev_char_errors": tot_e,
            "dev_delta_char_errors": delta_err,
            "dev_cer": round4_f64(cer),
            "dev_cer_percent": format!("{:.4}%", cer * 100.0),
            "dev_delta_cer_pp": format!("{:+.4}%", delta_cer_pp),
            "dev_exact_match": tot_ex,
            "dev_fixed_sents": fixed,
            "dev_broken_sents": broken,
            "dev_mcnemar_p_value": mcnemar_p,
            "dev_per_category_delta": per_cat_delta,
            "classical_total_chars": cl_ch,
            "classical_char_errors": cl_err,
            "classical_exact": format!("{}/{}", cl_ex, classical_compiled.len()),
            "classical_exact_count": cl_ex,
            "classical_cer_percent": format!("{:.2}%", cl_cer * 100.0),
            "novel_total_chars": nv_ch,
            "novel_char_errors": nv_err,
            "novel_exact": format!("{}/{}", nv_ex, novel_compiled.len()),
            "novel_exact_count": nv_ex,
            "novel_cer_percent": format!("{:.2}%", nv_cer * 100.0),
        });
        if include_probe_details {
            obj["classical_predictions"] = json!(cl_preds);
            obj["novel_predictions"] = json!(nv_preds);
        }
        obj
    };

    let mut strategies_table: Vec<Value> = Vec::new();

    // S0: Baseline (平權重 +28.0 參照基準，停用路線 C 文言單字模組)
    println!("[StrategyBench] 評估 S0 (Baseline) & S1 (規則式衍生詞裁剪/降級)...");
    let mut s0_router = base_engine.router.clone();
    s0_router.bonus_policy = None;
    s0_router.classical_mode = ruhi::router::ClassicalRegisterMode::ModernOnly;
    strategies_table.push(eval_strategy_full(
        "S0_baseline",
        "S0",
        &s0_router,
        None,
        None,
        true,
    ));

    // S1a & S1b: Hard rule-based derived pruning in n>=3
    strategies_table.push(eval_strategy_full(
        "S1a_prune_3char_and_wt800_derived",
        "S1a",
        &s0_router,
        Some(&s1a_remove_set),
        None,
        false,
    ));
    strategies_table.push(eval_strategy_full(
        "S1b_prune_transparent_derived_narrow",
        "S1b",
        &s0_router,
        Some(&s1b_remove_set),
        None,
        true,
    ));

    // S1c: Tiered derived multi_char_bonus (narrow set & all derived 1..29)
    let mut best_s1c_narrow_bonus = 20.0_f64;
    let mut best_s1c_narrow_errs = usize::MAX;
    for &dbonus in &[20.0_f64, 14.0, 10.0, 6.0, 0.0] {
        let mut r = s0_router.clone();
        r.bonus_policy = Some(ruhi::router::MultiCharBonusPolicy {
            derived_words: Some(narrow_derived_arc.clone()),
            derived_bonus: dbonus,
            ..Default::default()
        });
        let res = eval_strategy_full(
            &format!("S1c_narrow_derived_bonus_{}", dbonus as i32),
            "S1c_narrow",
            &r,
            None,
            None,
            false,
        );
        let errs = res["dev_char_errors"].as_u64().unwrap() as usize;
        if errs < best_s1c_narrow_errs {
            best_s1c_narrow_errs = errs;
            best_s1c_narrow_bonus = dbonus;
        }
        strategies_table.push(res);
    }
    for &dbonus in &[22.0_f64, 18.0, 14.0] {
        let mut r = s0_router.clone();
        r.bonus_policy = Some(ruhi::router::MultiCharBonusPolicy {
            derived_words: Some(all_derived_arc.clone()),
            derived_bonus: dbonus,
            ..Default::default()
        });
        strategies_table.push(eval_strategy_full(
            &format!("S1c_all_derived_bonus_{}", dbonus as i32),
            "S1c_all",
            &r,
            None,
            None,
            false,
        ));
    }

    // S2a: Hard low-weight 2-char pruning threshold in n >= 3
    println!("[StrategyBench] 評估 S2a (低權重雙字詞硬剪枝) & S2b (平滑權重梯度)...");
    for &thresh in &[120_u32, 150, 180, 220, 260, 300, 350] {
        strategies_table.push(eval_strategy_full(
            &format!("S2a_prune_len2_lt_{}", thresh),
            "S2a",
            &s0_router,
            None,
            Some(thresh),
            false,
        ));
    }

    // S2b: Soft weight-modulated multi_char_bonus ramp
    let ramp_grid: [(u32, u32, f64); 9] = [
        (100, 300, 14.0),
        (100, 300, 18.0),
        (100, 300, 22.0),
        (100, 350, 12.0),
        (100, 350, 16.0),
        (100, 350, 20.0),
        (150, 400, 14.0),
        (150, 400, 18.0),
        (180, 450, 16.0),
    ];
    let mut best_s2b = ramp_grid[0];
    let mut best_s2b_errs = usize::MAX;
    let mut best_s2b_idx = 0usize;
    for &(lo, hi, min_b) in &ramp_grid {
        let mut r = s0_router.clone();
        r.bonus_policy = Some(ruhi::router::MultiCharBonusPolicy {
            weight_ramp_low: Some(lo),
            weight_ramp_high: Some(hi),
            weight_ramp_min_bonus: min_b,
            ..Default::default()
        });
        let res = eval_strategy_full(
            &format!("S2b_ramp_{}_{}_min{}", lo, hi, min_b as i32),
            "S2b",
            &r,
            None,
            None,
            false,
        );
        let errs = res["dev_char_errors"].as_u64().unwrap() as usize;
        if errs < best_s2b_errs {
            best_s2b_errs = errs;
            best_s2b = (lo, hi, min_b);
            best_s2b_idx = strategies_table.len();
        }
        strategies_table.push(res);
    }
    // Re-run best S2b with probe details for inspection
    {
        let (lo, hi, min_b) = best_s2b;
        let mut r = s0_router.clone();
        r.bonus_policy = Some(ruhi::router::MultiCharBonusPolicy {
            weight_ramp_low: Some(lo),
            weight_ramp_high: Some(hi),
            weight_ramp_min_bonus: min_b,
            ..Default::default()
        });
        strategies_table[best_s2b_idx] = eval_strategy_full(
            &format!("S2b_ramp_{}_{}_min{}", lo, hi, min_b as i32),
            "S2b",
            &r,
            None,
            None,
            true,
        );
    }

    // S5: Combined best non-overfitting structural policy (best S2b ramp + best S1c narrow bonus)
    println!(
        "[StrategyBench] 評估 S5 (組合 S2b({}, {}, {}) + S1c_narrow({}))...",
        best_s2b.0, best_s2b.1, best_s2b.2, best_s1c_narrow_bonus
    );
    {
        let mut r = s0_router.clone();
        r.bonus_policy = Some(ruhi::router::MultiCharBonusPolicy {
            weight_ramp_low: Some(best_s2b.0),
            weight_ramp_high: Some(best_s2b.1),
            weight_ramp_min_bonus: best_s2b.2,
            use_narrow_derived_tier: false,
            derived_words: Some(narrow_derived_arc.clone()),
            derived_bonus: best_s1c_narrow_bonus,
        });
        strategies_table.push(eval_strategy_full(
            &format!(
                "S5_combo_ramp_{}_{}_min{}_plus_narrow{}",
                best_s2b.0, best_s2b.1, best_s2b.2 as i32, best_s1c_narrow_bonus as i32
            ),
            "S5",
            &r,
            None,
            None,
            true,
        ));
    }

    // S3: 5-Fold Stratified CV Out-Of-Fold Generalization Audit of Per-Word LOO Pruning
    println!("[StrategyBench] 正在執行 S3 5-Fold 跨折交叉驗證（檢驗逐詞刪詞之真實泛化率 vs 過擬合率）...");
    let mut cat_counters: BTreeMap<String, usize> = BTreeMap::new();
    let mut sent_fold: Vec<usize> = Vec::with_capacity(compiled.len());
    for cs in compiled {
        let cnt = cat_counters.entry(cs.item.category.clone()).or_insert(0);
        sent_fold.push(*cnt % 5);
        *cnt += 1;
    }

    struct CvCandidate {
        word: String,
        is_derived_1_29: bool,
        touched_sents: Vec<usize>,
    }
    struct CvWordTrace {
        word: String,
        is_derived: bool,
        fold_touched: [usize; 5],
        nonzero_outcomes: Vec<(usize, isize, bool, bool)>,
    }

    let mut cv_candidates: Vec<CvCandidate> = Vec::new();
    for (w, sents) in &word_to_touched_sents {
        let wt = lex_max_weight.get(w).copied().unwrap_or(0);
        let fams = classify_derived_families(w, wt);
        let error_hits = word_error_hits.get(w).copied().unwrap_or(0);
        let is_derived_1_29 = fams
            .iter()
            .any(|&f| f != "outlier_wt_ge_800" && f != "outlier_wt_ge_800_derived");
        if !fams.is_empty() || error_hits >= 1 {
            cv_candidates.push(CvCandidate {
                word: w.clone(),
                is_derived_1_29,
                touched_sents: sents.clone(),
            });
        }
    }
    cv_candidates.sort_by(|a, b| a.word.cmp(&b.word));

    let cv_chunk = (cv_candidates.len() + n_threads - 1) / n_threads.max(1);
    let sent_fold_ref = sent_fold.as_slice();
    let cv_traces: Vec<CvWordTrace> = if cv_candidates.is_empty() {
        Vec::new()
    } else {
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for chunk in cv_candidates.chunks(cv_chunk) {
                let mut worker_trans = base_engine.transitions.clone();
                handles.push(s.spawn(move || {
                    let mut out = Vec::with_capacity(chunk.len());
                    for cand in chunk {
                        let mut fold_touched = [0usize; 5];
                        let mut nonzero_outcomes = Vec::new();
                        for &s_idx in &cand.touched_sents {
                            fold_touched[sent_fold_ref[s_idx]] += 1;
                            let cs = &compiled[s_idx];
                            let mut f_dag = HashMap::with_capacity(dags_ref[s_idx].len());
                            for (&pos, nodes) in &dags_ref[s_idx] {
                                let kept: Vec<ruhi::router::LatticeNode> = nodes
                                    .iter()
                                    .filter(|n| n.length() == 1 || n.word != cand.word)
                                    .cloned()
                                    .collect();
                                f_dag.insert(pos, kept);
                            }
                            let (nodes, _) = base_engine.router.solve_best_path(
                                cs.syllables.len(),
                                &f_dag,
                                Some(&mut worker_trans),
                            );
                            let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                            let ab_e = char_levenshtein(&pred, &cs.item.text);
                            let orig_e = base_errors_ref[s_idx];
                            let d = (ab_e as isize) - (orig_e as isize);
                            let fx = !base_exact_ref[s_idx] && ab_e == 0;
                            let brk = base_exact_ref[s_idx] && ab_e > 0;
                            if d != 0 || fx || brk {
                                nonzero_outcomes.push((s_idx, d, fx, brk));
                            }
                        }
                        out.push(CvWordTrace {
                            word: cand.word.clone(),
                            is_derived: cand.is_derived_1_29,
                            fold_touched,
                            nonzero_outcomes,
                        });
                    }
                    out
                }));
            }
            handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
        })
    };

    #[derive(Clone, Copy)]
    enum CvRule {
        StrictHarmfulDerived,
        StrictHarmfulAll,
        StrictHarmfulAllLeMinus2,
        NeutralDerived,
    }
    let cv_rules = [
        ("S3a_cv_strict_harmful_derived", CvRule::StrictHarmfulDerived),
        ("S3b_cv_strict_harmful_all", CvRule::StrictHarmfulAll),
        ("S3c_cv_strict_harmful_all_le_m2", CvRule::StrictHarmfulAllLeMinus2),
        ("S3d_cv_neutral_derived", CvRule::NeutralDerived),
    ];

    let matches_cv_rule = |rule: CvRule, tr: &CvWordTrace, excluded_fold: Option<usize>| -> bool {
        if matches!(rule, CvRule::StrictHarmfulDerived | CvRule::NeutralDerived) && !tr.is_derived {
            return false;
        }
        let mut d_sum = 0isize;
        let mut brk_sum = 0usize;
        for &(s_idx, d, _fx, brk) in &tr.nonzero_outcomes {
            if excluded_fold.is_some_and(|kf| sent_fold_ref[s_idx] == kf) {
                continue;
            }
            d_sum += d;
            if brk {
                brk_sum += 1;
            }
        }
        let touched_sum: usize = (0..5)
            .filter(|&f| excluded_fold != Some(f))
            .map(|f| tr.fold_touched[f])
            .sum();
        match rule {
            CvRule::StrictHarmfulDerived | CvRule::StrictHarmfulAll => d_sum < 0 && brk_sum == 0,
            CvRule::StrictHarmfulAllLeMinus2 => d_sum <= -2 && brk_sum == 0,
            CvRule::NeutralDerived => d_sum == 0 && touched_sum > 0,
        }
    };

    let mut cv_results: Vec<Value> = Vec::with_capacity(cv_rules.len());
    for &(rname, rule) in &cv_rules {
        // Full in-sample evaluation for exact baseline comparison
        let full_words: HashSet<String> = cv_traces
            .iter()
            .filter(|tr| matches_cv_rule(rule, tr, None))
            .map(|tr| tr.word.clone())
            .collect();
        let mut full_touched_set: HashSet<usize> = HashSet::new();
        for w in &full_words {
            if let Some(sents) = word_to_touched_sents.get(w) {
                for &si in sents {
                    full_touched_set.insert(si);
                }
            }
        }
        let mut full_touched_vec: Vec<usize> = full_touched_set.into_iter().collect();
        full_touched_vec.sort_unstable();
        let in_sample = eval_removed_word_set(
            base_engine,
            compiled,
            &dags,
            &base_errors,
            &base_exact,
            total_chars,
            base_total_errors,
            base_cer,
            &full_words,
            &full_touched_vec,
        );

        // 5-Fold Out-Of-Fold evaluation
        let mut fold_word_counts = Vec::with_capacity(5);
        let mut oof_delta = 0isize;
        let mut oof_fixed = 0usize;
        let mut oof_broken = 0usize;
        let mut oof_imp = 0usize;
        let mut oof_deg = 0usize;
        let mut oof_cat_delta: BTreeMap<String, isize> = BTreeMap::new();

        for k in 0..5 {
            let fold_words: HashSet<String> = cv_traces
                .iter()
                .filter(|tr| matches_cv_rule(rule, tr, Some(k)))
                .map(|tr| tr.word.clone())
                .collect();
            fold_word_counts.push(fold_words.len());

            let mut heldout_touched: HashSet<usize> = HashSet::new();
            for w in &fold_words {
                if let Some(sents) = word_to_touched_sents.get(w) {
                    for &si in sents {
                        if sent_fold[si] == k {
                            heldout_touched.insert(si);
                        }
                    }
                }
            }
            let mut heldout_vec: Vec<usize> = heldout_touched.into_iter().collect();
            heldout_vec.sort_unstable();

            let fm = eval_removed_word_set(
                base_engine,
                compiled,
                &dags,
                &base_errors,
                &base_exact,
                total_chars,
                base_total_errors,
                base_cer,
                &fold_words,
                &heldout_vec,
            );
            oof_delta += fm.delta_char_errors;
            oof_fixed += fm.fixed_sents;
            oof_broken += fm.broken_sents;
            oof_imp += fm.char_improved_sents;
            oof_deg += fm.char_degraded_sents;
            for (cat, cd) in fm.per_category_delta_char_errors {
                *oof_cat_delta.entry(cat).or_insert(0) += cd;
            }
        }

        let avg_words = fold_word_counts.iter().sum::<usize>() as f64 / 5.0;
        let oof_errs = (base_total_errors as isize + oof_delta).max(0) as usize;
        let oof_cer = oof_errs as f64 / total_chars.max(1) as f64;
        let oof_cer_pp = (oof_cer - base_cer) * 100.0;
        let (_, oof_p) = compute_mcnemar(oof_fixed, oof_broken);
        let retention = if in_sample.delta_char_errors != 0 {
            Some(round4_f64(
                oof_delta as f64 / in_sample.delta_char_errors as f64,
            ))
        } else {
            None
        };

        // Also check Probe sets using the full-dev selected words
        let (cl_err, _cl_ch, cl_ex, cl_cer, _) =
            eval_probe_set(&base_engine.router, &classical_compiled, &classical_dags, Some(&full_words), None);
        let (nv_err, _nv_ch, nv_ex, nv_cer, _) =
            eval_probe_set(&base_engine.router, &novel_compiled, &novel_dags, Some(&full_words), None);

        cv_results.push(json!({
            "strategy": rname,
            "full_dev_selected_words": full_words.len(),
            "avg_selected_words_per_fold": (avg_words * 10.0).round() / 10.0,
            "in_sample_delta_char_errors": in_sample.delta_char_errors,
            "in_sample_cer_percent": format!("{:.4}%", in_sample.ablated_cer * 100.0),
            "in_sample_fixed_sents": in_sample.fixed_sents,
            "in_sample_broken_sents": in_sample.broken_sents,
            "oof_delta_char_errors": oof_delta,
            "oof_cer_percent": format!("{:.4}%", oof_cer * 100.0),
            "oof_delta_cer_pp": format!("{:+.4}%", oof_cer_pp),
            "oof_exact_match": (base_exact_count as isize + oof_fixed as isize - oof_broken as isize),
            "oof_fixed_sents": oof_fixed,
            "oof_broken_sents": oof_broken,
            "oof_char_improved_sents": oof_imp,
            "oof_char_degraded_sents": oof_deg,
            "oof_mcnemar_p_value": oof_p,
            "generalization_retention_ratio": retention,
            "oof_per_category_delta": oof_cat_delta,
            "full_dev_set_classical_cer_percent": format!("{:.2}%", cl_cer * 100.0),
            "full_dev_set_classical_errors": cl_err,
            "full_dev_set_classical_exact": format!("{}/{}", cl_ex, classical_compiled.len()),
            "full_dev_set_novel_cer_percent": format!("{:.2}%", nv_cer * 100.0),
            "full_dev_set_novel_errors": nv_err,
            "full_dev_set_novel_exact": format!("{}/{}", nv_ex, novel_compiled.len()),
        }));
    }

    // S6: Classical-Mode preview (sweep multi_char_bonus on Classical-20 & Novel-15 only)
    let mut classical_mode_preview = Vec::new();
    for &bval in &[28.0_f64, 20.0, 14.0, 10.0, 6.0, 0.0] {
        let mut r = s0_router.clone();
        r.multi_char_bonus = bval;
        let include_preds = matches!(bval as i32, 28 | 10 | 0);
        let (cl_err, cl_ch, cl_ex, cl_cer, cl_preds) =
            eval_probe_set(&r, &classical_compiled, &classical_dags, None, None);
        let (nv_err, nv_ch, nv_ex, nv_cer, nv_preds) =
            eval_probe_set(&r, &novel_compiled, &novel_dags, None, None);
        let mut row = json!({
            "multi_char_bonus": bval,
            "classical_chars": cl_ch,
            "classical_errors": cl_err,
            "classical_exact": format!("{}/{}", cl_ex, classical_compiled.len()),
            "classical_cer_percent": format!("{:.2}%", cl_cer * 100.0),
            "novel_chars": nv_ch,
            "novel_errors": nv_err,
            "novel_exact": format!("{}/{}", nv_ex, novel_compiled.len()),
            "novel_cer_percent": format!("{:.2}%", nv_cer * 100.0),
        });
        if include_preds {
            row["classical_predictions"] = json!(cl_preds);
            row["novel_predictions"] = json!(nv_preds);
        }
        classical_mode_preview.push(row);
    }

    let elapsed_secs = (t_start.elapsed().as_secs_f64() * 100.0).round() / 100.0;
    let compact_strategies: Vec<Value> = strategies_table
        .iter()
        .map(|s| {
            json!({
                "strategy": s["strategy"],
                "group": s["group"],
                "dev_delta": s["dev_delta_char_errors"],
                "dev_cer": s["dev_cer_percent"],
                "dev_exact": s["dev_exact_match"],
                "fix": s["dev_fixed_sents"],
                "brk": s["dev_broken_sents"],
                "p": s["dev_mcnemar_p_value"],
                "cl_cer": s["classical_cer_percent"],
                "cl_exact": s["classical_exact"],
                "nv_cer": s["novel_cer_percent"],
                "nv_exact": s["novel_exact"],
            })
        })
        .collect();

    let stdout_summary = json!({
        "elapsed_seconds": elapsed_secs,
        "strategies_compact": compact_strategies,
        "s3_five_fold_cv": cv_results,
        "s6_classical_preview_compact": classical_mode_preview.iter().map(|c| json!({
            "multi_char_bonus": c["multi_char_bonus"],
            "classical_errors": c["classical_errors"],
            "classical_exact": c["classical_exact"],
            "classical_cer": c["classical_cer_percent"],
            "novel_errors": c["novel_errors"],
            "novel_exact": c["novel_exact"],
            "novel_cer": c["novel_cer_percent"],
        })).collect::<Vec<_>>(),
    });

    json!({
        "elapsed_seconds": elapsed_secs,
        "s1a_word_count": s1a_remove_set.len(),
        "s1b_word_count": s1b_remove_set.len(),
        "best_s2b": {"lo": best_s2b.0, "hi": best_s2b.1, "min_bonus": best_s2b.2},
        "best_s1c_narrow_bonus": best_s1c_narrow_bonus,
        "strategies": strategies_table,
        "s3_five_fold_cv": cv_results,
        "s6_classical_mode_preview": classical_mode_preview,
        "stdout_summary": stdout_summary,
    })
}

/// 路線 C 文言單字語境原型評估 (`--classical-bench`)：
/// 於 S5 生產底盤之上，比較 `ModernOnly`、`Auto`、`ClassicalExplicit` 三種模式在
/// `dev.tsv`、`Classical-20`、`Classical-10 Unseen Holdout` 與 `Novel-15` 之實證表現。
pub fn run_classical_benchmark(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    compiler: &PhoneticCompiler,
    _root: &Path,
) -> Value {
    use ruhi::router::{ClassicalRegisterMode, ClassicalRegisterProfile};

    let t_start = Instant::now();
    let classical_20_texts = [
        "知之為知之不知為不知是知也",
        "道可道非常道名可名非常名",
        "先天下之憂而憂後天下之樂而樂",
        "古之學者必有師",
        "師者所以傳道受業解惑也",
        "不以物喜不以己悲",
        "醉翁之意不在酒在乎山水之間也",
        "予獨愛蓮之出淤泥而不染",
        "三人行必有我師焉",
        "溫故而知新可以為師矣",
        "生於憂患而死於安樂",
        "天將降大任於斯人也",
        "臣本布衣躬耕於南陽",
        "己所不欲勿施於人",
        "君子和而不同小人同而不和",
        "水能載舟亦能覆舟",
        "民為貴社稷次之君為輕",
        "千里之行始於足下",
        "工欲善其事必先利其器",
        "欲窮千里目更上一層樓",
    ];
    let classical_10_unseen_texts = [
        "學而不思則罔思而不學則殆",
        "青出於藍而勝於藍",
        "仁者見仁智者見智",
        "君子坦蕩蕩小人長戚戚",
        "敏而好學不恥下問",
        "業精於勤荒於嬉",
        "行成於思毀於隨",
        "言必信行必果",
        "失之毫釐謬以千里",
        "天下興亡匹夫有責",
    ];
    let novel_15_texts = [
        "一個有辦法打文言文的輸入法",
        "文言文輸入不出錯",
        "不過是該漢字的常見連結",
        "漢字系統的原型",
        "先跑一下統計以確定利弊得失",
        "每一個漢字本身就是一個語意本體",
        "憑感覺的事情",
        "平均值的計算",
        "令人不捨的行為",
        "台灣省的時代",
        "他樂得清閒",
        "我也很禮貌的回答",
        "量子計算機與神經形態晶片",
        "跨域語素因式分解拓撲模型",
        "低延遲本地端解碼引擎",
    ];

    let compile_probe = |texts: &[&str], cat: &str| -> Vec<CompiledSentence> {
        texts
            .iter()
            .enumerate()
            .filter_map(|(idx, &txt)| {
                let item = CorpusItem {
                    id: format!("{}_{:02}", cat, idx + 1),
                    category: cat.to_string(),
                    text: txt.to_string(),
                };
                compiler.compile(&item)
            })
            .collect()
    };

    let cl20 = compile_probe(&classical_20_texts, "classical_20");
    let cl10 = compile_probe(&classical_10_unseen_texts, "classical_10_unseen");
    let nv15 = compile_probe(&novel_15_texts, "novel_15");

    let build_dags = |items: &[CompiledSentence]| -> Vec<HashMap<usize, Vec<ruhi::router::LatticeNode>>> {
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(16);
        let chunk_size = (items.len() + n_threads - 1) / n_threads.max(1);
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for chunk in items.chunks(chunk_size.max(1)) {
                let dag_builder = LatticeDAG::new(&base_engine.lexicon);
                handles.push(s.spawn(move || {
                    chunk
                        .iter()
                        .map(|cs| dag_builder.build_dag(&cs.syllables, true))
                        .collect::<Vec<_>>()
                }));
            }
            handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
        })
    };

    println!(
        "[ClassicalBench] 正在建置詞網格（dev {} 句 + Classical-20 {} 句 + Unseen-10 {} 句 + Novel-15 {} 句）...",
        compiled.len(),
        cl20.len(),
        cl10.len(),
        nv15.len()
    );
    let dev_dags = build_dags(compiled);
    let cl20_dags = build_dags(&cl20);
    let cl10_dags = build_dags(&cl10);
    let nv15_dags = build_dags(&nv15);

    let eval_dev_mode = |mode: ClassicalRegisterMode| -> Value {
        let mut router = base_engine.router.clone();
        router.classical_mode = mode;
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(16);
        let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

        let parts: Vec<(String, usize, usize, bool, String)> = std::thread::scope(|s| {
            let mut handles = Vec::new();
            for (c_idx, chunk) in compiled.chunks(chunk_size.max(1)).enumerate() {
                let start = c_idx * chunk_size.max(1);
                let dag_slice = &dev_dags[start..start + chunk.len()];
                let router_ref = &router;
                let mut trans = base_engine.transitions.clone();
                handles.push(s.spawn(move || {
                    let mut out = Vec::with_capacity(chunk.len());
                    for (cs, dag) in chunk.iter().zip(dag_slice.iter()) {
                        let (nodes, _) =
                            router_ref.solve_best_path(cs.syllables.len(), dag, Some(&mut trans));
                        let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                        let ch_len = cs.item.text.chars().count();
                        let errs = char_levenshtein(&pred, &cs.item.text);
                        let auto_trig =
                            ClassicalRegisterProfile::detect(ClassicalRegisterMode::Auto, cs.syllables.len(), dag)
                                .active;
                        out.push((
                            cs.item.category.clone(),
                            ch_len,
                            errs,
                            auto_trig,
                            cs.item.id.clone(),
                        ));
                    }
                    out
                }));
            }
            handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
        });

        let mut total_chars = 0usize;
        let mut total_errors = 0usize;
        let mut exact_match = 0usize;
        let mut auto_triggered_ids: Vec<String> = Vec::new();
        let mut by_cat: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for (cat, ch, errs, trig, id) in parts {
            total_chars += ch;
            total_errors += errs;
            if errs == 0 {
                exact_match += 1;
            }
            if trig {
                auto_triggered_ids.push(id);
            }
            let e = by_cat.entry(cat).or_insert((0, 0));
            e.0 += ch;
            e.1 += errs;
        }
        let cer = total_errors as f64 / total_chars.max(1) as f64;
        let ser = exact_match as f64 / compiled.len().max(1) as f64;
        let trig_cnt = auto_triggered_ids.len();
        let trig_rate = trig_cnt as f64 / compiled.len().max(1) as f64;
        let mut per_category = BTreeMap::new();
        for (cat, (ch, err)) in by_cat {
            let c_cer = err as f64 / ch.max(1) as f64;
            per_category.insert(
                cat,
                json!({
                    "total_chars": ch,
                    "char_errors": err,
                    "cer": c_cer,
                    "cer_percent": format!("{:.2}%", c_cer * 100.0),
                }),
            );
        }
        json!({
            "total_sentences": compiled.len(),
            "total_chars": total_chars,
            "char_errors": total_errors,
            "cer": cer,
            "cer_percent": format!("{:.2}%", cer * 100.0),
            "exact_match": exact_match,
            "sentence_accuracy_percent": format!("{:.2}%", ser * 100.0),
            "auto_triggered_count": trig_cnt,
            "auto_triggered_rate_percent": format!("{:.3}%", trig_rate * 100.0),
            "auto_triggered_ids": auto_triggered_ids,
            "per_category": per_category,
        })
    };

    let eval_probe = |mode: ClassicalRegisterMode,
                      items: &[CompiledSentence],
                      dags: &[HashMap<usize, Vec<ruhi::router::LatticeNode>>]|
     -> Value {
        let mut router = base_engine.router.clone();
        router.classical_mode = mode;
        let mut trans = base_engine.transitions.clone();
        let mut tot_ch = 0usize;
        let mut tot_err = 0usize;
        let mut exact = 0usize;
        let mut auto_trig_cnt = 0usize;
        let mut preds = Vec::with_capacity(items.len());

        for (cs, dag) in items.iter().zip(dags.iter()) {
            let (nodes, _) = router.solve_best_path(cs.syllables.len(), dag, Some(&mut trans));
            let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
            let ch_len = cs.item.text.chars().count();
            let errs = char_levenshtein(&pred, &cs.item.text);
            let prof_auto =
                ClassicalRegisterProfile::detect(ClassicalRegisterMode::Auto, cs.syllables.len(), dag);
            tot_ch += ch_len;
            tot_err += errs;
            if errs == 0 {
                exact += 1;
            }
            if prof_auto.active {
                auto_trig_cnt += 1;
            }
            preds.push(json!({
                "id": cs.item.id,
                "gold": cs.item.text,
                "pred": pred,
                "char_errors": errs,
                "auto_triggered": prof_auto.active,
                "auto_score": prof_auto.classical_score,
            }));
        }
        let cer = tot_err as f64 / tot_ch.max(1) as f64;
        json!({
            "total_sentences": items.len(),
            "total_chars": tot_ch,
            "char_errors": tot_err,
            "cer": cer,
            "cer_percent": format!("{:.2}%", cer * 100.0),
            "exact_match": format!("{}/{}", exact, items.len()),
            "exact_count": exact,
            "auto_triggered_count": auto_trig_cnt,
            "predictions": preds,
        })
    };

    let modes = [
        ("ModernOnly", ClassicalRegisterMode::ModernOnly),
        ("Auto", ClassicalRegisterMode::Auto),
        ("ClassicalExplicit", ClassicalRegisterMode::ClassicalExplicit),
    ];

    let mut modern_dev_errors = 0isize;
    let mut mode_reports = serde_json::Map::new();
    let mut compact_rows = Vec::new();

    for (label, mode) in modes {
        println!("[ClassicalBench] 評估模式 {} ...", label);
        let mut dev_res = eval_dev_mode(mode);
        let cur_dev_err = dev_res["char_errors"].as_u64().unwrap_or(0) as isize;
        if label == "ModernOnly" {
            modern_dev_errors = cur_dev_err;
        }
        let delta_dev = cur_dev_err - modern_dev_errors;
        dev_res["delta_char_errors_vs_modern"] = json!(delta_dev);

        let r20 = eval_probe(mode, &cl20, &cl20_dags);
        let r10 = eval_probe(mode, &cl10, &cl10_dags);
        let r15 = eval_probe(mode, &nv15, &nv15_dags);

        compact_rows.push(json!({
            "mode": label,
            "dev_errors": cur_dev_err,
            "dev_delta_vs_modern": delta_dev,
            "dev_cer": dev_res["cer_percent"],
            "dev_exact": dev_res["exact_match"],
            "dev_auto_trig": dev_res["auto_triggered_count"],
            "dev_auto_rate": dev_res["auto_triggered_rate_percent"],
            "cl20_errors": r20["char_errors"],
            "cl20_cer": r20["cer_percent"],
            "cl20_exact": r20["exact_match"],
            "cl20_trig": r20["auto_triggered_count"],
            "cl10_unseen_errors": r10["char_errors"],
            "cl10_unseen_cer": r10["cer_percent"],
            "cl10_unseen_exact": r10["exact_match"],
            "cl10_unseen_trig": r10["auto_triggered_count"],
            "novel15_errors": r15["char_errors"],
            "novel15_cer": r15["cer_percent"],
            "novel15_exact": r15["exact_match"],
            "novel15_trig": r15["auto_triggered_count"],
        }));

        mode_reports.insert(
            label.to_string(),
            json!({
                "dev": dev_res,
                "classical_20": r20,
                "classical_10_unseen": r10,
                "novel_15": r15,
            }),
        );
    }

    let elapsed_secs = (t_start.elapsed().as_secs_f64() * 100.0).round() / 100.0;
    let stdout_summary = json!({
        "elapsed_seconds": elapsed_secs,
        "modes_compact": compact_rows,
    });

    json!({
        "elapsed_seconds": elapsed_secs,
        "modes": mode_reports,
        "stdout_summary": stdout_summary,
    })
}

#[inline]
fn default_feature_weight(k: &str) -> f64 {
    if k.starts_with("ngram_") || k == "coord_he_huo_prior" {
        0.0
    } else {
        1.0
    }
}

/// 線性特徵安全邊界（Trust-Region Bounds，保護構式雙字詞與既有單元測試不退化）
fn feature_bounds(k: &str) -> (f64, f64) {
    match k {
        "coord_he_huo_prior" => (0.0, 0.20),
        "ngram_word_bi" => (0.0, 0.35),
        "ngram_word_tri" => (0.0, 0.35),
        "ngram_char_bi" => (0.0, 0.10),
        "ngram_char_tri" => (0.0, 0.30),
        "multi_char_len" | "cohesion_len3" | "cohesion_len4" => (0.98, 1.05),
        "freq_ln_char1" => (1.02, 1.0845),
        "freq_ln_compound" => (1.0845, 1.15),
        "construction_topological_core"
        | "construction_boundary_guard"
        | "construction_ctx31_bifurcation"
        | "construction_trigram_slot"
        | "geo_hierarchy" => (0.9437, 1.08),
        "family_core_syntax" | "family_morph_phonetic_guard" => (0.92, 1.08),
        _ => (0.90, 1.10),
    }
}

/// 關鍵金絲雀語句驗證（保證回歸單元測試 100% 綠燈）
fn check_canary_sentences(engine: &RuhiEngine, cand_w: &HashMap<String, f64>) -> Result<(), String> {
    let mut worker = engine.clone();
    worker.transitions.feature_weights = cand_w.clone();
    let run_case = |w: &mut RuhiEngine, keys: &str, expected: &str, trim_out: bool| -> Result<(), String> {
        w.reset_buffer();
        w.last_committed_word.clear();
        w.active_geo_context.clear();
        let mut out = String::new();
        for c in keys.chars() {
            let r = w.process_key(c);
            if !r.committed_chunk.is_empty() {
                out.push_str(&r.committed_chunk);
            }
        }
        out.push_str(&w.get_display_text());
        let actual_cmp = if trim_out { out.trim() } else { out.as_str() };
        if actual_cmp == expected {
            Ok(())
        } else {
            Err(format!("expected={} actual={}", expected, actual_cmp))
        }
    };

    let hsu_canaries = [
        ("eef gmf kif", "也感慨", false),
        ("eef gmf kij", "也感慨", false),
        ("z lewjvxflefdscdfn txhfdkj", "資料處理得十分妥當", false),
        ("hidzijceo gifvldcjmyf", "還在修改程式碼", false),
        ("cm ckjdshojcuefhidzijcerjggsmadxmdmadlewf", "山上的厚雪還在下個沒完沒了", false),
        ("nyflefcu ewjbk mkdjeojvujnyflef", "哪裡需要幫忙就去哪裡", false),
        ("je lxldcjjxl jljvu hgdcemjlefe lendhgde lxjcdsm hwj", "基隆市中正區和憲里一鄰和一路十三號", false),
        ("tidbafcjdrjm vu hxrdcl lef 11 lendjxl cewj dxl lxj sj dxmj 295 hwj ", "台北市大安區華聲里11鄰忠孝東路四段295號", false),
        ("ty zijagjcjcen gxl nld", "他在測試新功能", true),
        ("ty dscen zxhjpenf", "他的新作品", true),
        ("xd xnd rxld ", "吳玟蓉", true),
        ("jeoflxldcn cxafbxjven jo jee 37Khwj", "九龍深水埗欽州街37K號", true),
        ("has", "嘿", true),
    ];
    for &(keys, expected, trim_out) in &hsu_canaries {
        run_case(&mut worker, keys, expected, trim_out)?;
    }

    worker.set_keyboard_mode("standard");
    let std_canaries = [
        ("t03qup35jp31o4g;4vu04", "產品準備上線"),
        ("up3fu3ru62845j4u4", "引起極大注意"),
        ("ji3u3ru/ qu/60 cjo6ru8 xk7", "我已經平安回家了"),
    ];
    for &(keys, expected) in &std_canaries {
        run_case(&mut worker, keys, expected, false)?;
    }
    worker.set_keyboard_mode("hsu");
    Ok(())
}

/// 平行快速評估候選特徵權重映射
fn eval_candidate_weights(
    base_engine: &RuhiEngine,
    cand_w: &HashMap<String, f64>,
    compiled: &[CompiledSentence],
    dags: &[HashMap<usize, Vec<ruhi::router::LatticeNode>>],
    init_exact_flags: &[bool],
) -> (usize, usize, usize, usize) {
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);
    if chunk_size == 0 {
        return (0, 0, 0, 0);
    }

    let thread_results: Vec<(usize, usize, usize, usize)> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for (c_chunk, (d_chunk, init_chunk)) in compiled
            .chunks(chunk_size)
            .zip(dags.chunks(chunk_size).zip(init_exact_flags.chunks(chunk_size)))
        {
            let mut worker = base_engine.clone();
            worker.transitions.feature_weights = cand_w.clone();
            handles.push(s.spawn(move || {
                let mut errs = 0usize;
                let mut exact = 0usize;
                let mut fixed = 0usize;
                let mut broken = 0usize;
                for i in 0..c_chunk.len() {
                    let cs = &c_chunk[i];
                    let dag = &d_chunk[i];
                    let init_ex = init_chunk[i];
                    let (nodes, _) = worker.router.solve_best_path(
                        cs.syllables.len(),
                        dag,
                        Some(&mut worker.transitions),
                    );
                    let pred: String = nodes.iter().map(|n| n.word.as_str()).collect();
                    let e = char_levenshtein(&pred, &cs.item.text);
                    errs += e;
                    if e == 0 {
                        exact += 1;
                        if !init_ex {
                            fixed += 1;
                        }
                    } else if init_ex {
                        broken += 1;
                    }
                }
                (errs, exact, fixed, broken)
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut tot_errs = 0;
    let mut tot_exact = 0;
    let mut tot_fixed = 0;
    let mut tot_broken = 0;
    for (e, ex, fix, brk) in thread_results {
        tot_errs += e;
        tot_exact += ex;
        tot_fixed += fix;
        tot_broken += brk;
    }
    if let Err(reason) = check_canary_sentences(base_engine, cand_w) {
        let wb = cand_w.get("ngram_word_bi").copied().unwrap_or(0.0);
        let cb = cand_w.get("ngram_char_bi").copied().unwrap_or(0.0);
        let ct = cand_w.get("ngram_char_tri").copied().unwrap_or(0.0);
        println!(
            "  [CanaryVeto] ct={:.2} wb={:.2} cb={:.2} | errs={} ex={} fix={} brk={} | {}",
            ct, wb, cb, tot_errs, tot_exact, tot_fixed, tot_broken, reason
        );
        tot_broken += 99999;
    }
    (tot_errs, tot_exact, tot_fixed, tot_broken)
}

/// Stage 0 座標網格暖啟動單軸掃描輔助函式
#[allow(clippy::too_many_arguments)]
fn stage0_sweep_feature(
    pass_label: &str,
    k: &str,
    grid: &[f64],
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    dags: &[HashMap<usize, Vec<ruhi::router::LatticeNode>>],
    init_exact_flags: &[bool],
    best_weights: &mut HashMap<String, f64>,
    best_errors: &mut usize,
    best_exact: &mut usize,
    stage0_steps: &mut Vec<Value>,
) {
    let (lo, hi) = feature_bounds(k);
    let base_val = best_weights
        .get(k)
        .copied()
        .unwrap_or(default_feature_weight(k));
    for &raw_cand in grid {
        let cand_val = ((raw_cand.clamp(lo, hi)) * 10000.0).round() / 10000.0;
        if (cand_val - base_val).abs() < 1e-6 {
            continue;
        }
        let mut cand_w = best_weights.clone();
        cand_w.insert(k.to_string(), cand_val);
        let (c_errors, c_exact, c_fixed, c_broken) =
            eval_candidate_weights(base_engine, &cand_w, compiled, dags, init_exact_flags);
        if (c_errors < *best_errors || (c_errors == *best_errors && c_exact > *best_exact))
            && c_broken <= c_fixed
        {
            let cur_base = best_weights
                .get(k)
                .copied()
                .unwrap_or(default_feature_weight(k));
            println!(
                "  [Stage0:{}] {}: {:.4} -> {:.4} (錯誤數 {} -> {}, 正確率 {} -> {}, fix={}, brk={})",
                pass_label, k, cur_base, cand_val, *best_errors, c_errors, *best_exact, c_exact, c_fixed, c_broken
            );
            stage0_steps.push(json!({
                "pass": pass_label,
                "feature": k,
                "old_val": cur_base,
                "new_val": cand_val,
                "old_errors": *best_errors,
                "new_errors": c_errors,
                "old_exact": *best_exact,
                "new_exact": c_exact,
                "fixed": c_fixed,
                "broken": c_broken,
            }));
            *best_errors = c_errors;
            *best_exact = c_exact;
            best_weights.insert(k.to_string(), cand_val);
        }
    }
}

/// 執行平均結構化感知機 (Averaged Structured Perceptron) 權重校準 (`--calibrate`)
pub fn run_perceptron_calibration(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    root: &Path,
    ngram_only: bool,
) -> Value {
    println!(
        "[Calibration] 正在對 {} 句開發集執行強制對齊 (Forced Alignment) 與特徵擷取...",
        compiled.len()
    );

    let feature_keys = [
        "freq_ln_char1",
        "freq_ln_compound",
        "multi_char_len",
        "cohesion_len3",
        "cohesion_len4",
        "coord_prior",
        "coord_he_huo_prior",
        "user_bigram_pos",
        "user_bigram_neg",
        "builtin_bigram",
        "vo_separable_lookback",
        "construction_topological_core",
        "construction_trigram_slot",
        "geo_hierarchy",
        "construction_ctx31_bifurcation",
        "construction_boundary_guard",
        "family_user_pattern",
        "family_vo_complement",
        "family_classifier_numeric",
        "family_spatial_localizer",
        "family_aspect_temporal",
        "family_conjunction_discourse",
        "family_morph_phonetic_guard",
        "family_core_syntax",
        "ngram_word_bi",
        "ngram_word_tri",
        "ngram_char_bi",
        "ngram_char_tri",
    ];

    // 代數恆等溫啟動 (Warm-Start Algebraic Identity Initialization)
    let base_fw = &base_engine.transitions.feature_weights;
    let w0: HashMap<String, f64> = feature_keys
        .iter()
        .map(|&k| {
            let val = match k {
                "freq_ln_char1" | "freq_ln_compound" => base_fw
                    .get(k)
                    .or_else(|| base_fw.get("freq_ln"))
                    .copied()
                    .unwrap_or(1.0),
                "construction_topological_core"
                | "construction_trigram_slot"
                | "geo_hierarchy"
                | "construction_ctx31_bifurcation"
                | "construction_boundary_guard" => base_fw
                    .get(k)
                    .or_else(|| base_fw.get("construction_hierarchical_matrix"))
                    .copied()
                    .unwrap_or(1.0),
                _ if k.starts_with("ngram_") || k == "coord_he_huo_prior" => {
                    base_fw.get(k).copied().unwrap_or(0.0)
                }
                _ => base_fw.get(k).copied().unwrap_or(1.0),
            };
            (k.to_string(), val)
        })
        .collect();

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    struct PrebuildItem {
        dag: HashMap<usize, Vec<ruhi::router::LatticeNode>>,
        gold_path: Option<(Vec<ruhi::router::LatticeNode>, HashMap<String, f64>)>,
        init_exact: bool,
        init_errors: usize,
        char_len: usize,
    }

    let prebuild_results: Vec<Vec<PrebuildItem>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size) {
            let mut worker = base_engine.clone();
            worker.transitions.feature_weights = w0.clone();
            handles.push(s.spawn(move || {
                let mut chunk_out = Vec::with_capacity(chunk.len());
                let dag_builder = LatticeDAG::new(&worker.lexicon);
                for cs in chunk {
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let (pred_nodes, _) = worker.router.solve_best_path(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker.transitions),
                    );
                    let pred: String = pred_nodes.iter().map(|n| n.word.as_str()).collect();
                    let ch_len = cs.item.text.chars().count();
                    let errs = char_levenshtein(&pred, &cs.item.text);
                    let is_exact = errs == 0;

                    let target_chars: Vec<char> = cs.item.text.chars().collect();
                    let gold_path = if let Some((g_nodes, _)) = worker.router.solve_forced_alignment(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker.transitions),
                        &target_chars,
                    ) {
                        let (g_feats, _) = worker.router.extract_path_features_and_triggers(
                            &g_nodes,
                            &dag,
                            &mut worker.transitions,
                        );
                        Some((g_nodes, g_feats))
                    } else {
                        None
                    };

                    chunk_out.push(PrebuildItem {
                        dag,
                        gold_path,
                        init_exact: is_exact,
                        init_errors: errs,
                        char_len: ch_len,
                    });
                }
                chunk_out
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut dags = Vec::with_capacity(compiled.len());
    let mut gold_paths = Vec::with_capacity(compiled.len());
    let mut init_exact_flags = Vec::with_capacity(compiled.len());
    let mut init_errors = 0usize;
    let mut total_chars = 0usize;
    let mut init_exact_count = 0usize;
    let mut unreachable_count = 0usize;

    for chunk_res in prebuild_results {
        for item in chunk_res {
            if item.gold_path.is_none() {
                unreachable_count += 1;
            }
            if item.init_exact {
                init_exact_count += 1;
            }
            init_errors += item.init_errors;
            total_chars += item.char_len;
            init_exact_flags.push(item.init_exact);
            dags.push(item.dag);
            gold_paths.push(item.gold_path);
        }
    }

    let init_cer = init_errors as f64 / total_chars.max(1) as f64;
    let init_ser = init_exact_count as f64 / compiled.len().max(1) as f64;
    println!(
        "[Calibration] 初始 CER = {:.2}% ({}/{} 字錯誤)，完全匹配 = {:.2}% ({}/{} 句)，黃金路徑可達率 = {:.2}% ({}/{} 句)",
        init_cer * 100.0,
        init_errors,
        total_chars,
        init_ser * 100.0,
        init_exact_count,
        compiled.len(),
        ((compiled.len() - unreachable_count) as f64 / compiled.len().max(1) as f64) * 100.0,
        compiled.len() - unreachable_count,
        compiled.len()
    );

    let mut best_weights = w0.clone();
    let mut best_errors = init_errors;
    let mut best_exact = init_exact_count;
    let mut stage0_steps = Vec::new();

    if let Some(ref lm_arc) = base_engine.transitions.ngram_lm {
        let has_wt = lm_arc.has_word_trigrams();
        println!("[Calibration] 進入 Stage 0: 統計 N-gram 特徵多尺度網格暖啟動...");
        stage0_sweep_feature(
            "0a",
            "ngram_char_tri",
            &[0.0, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30],
            base_engine,
            compiled,
            &dags,
            &init_exact_flags,
            &mut best_weights,
            &mut best_errors,
            &mut best_exact,
            &mut stage0_steps,
        );
        stage0_sweep_feature(
            "0b",
            "ngram_word_bi",
            &[0.0, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35],
            base_engine,
            compiled,
            &dags,
            &init_exact_flags,
            &mut best_weights,
            &mut best_errors,
            &mut best_exact,
            &mut stage0_steps,
        );
        if has_wt {
            stage0_sweep_feature(
                "0b-wt",
                "ngram_word_tri",
                &[0.0, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35],
                base_engine,
                compiled,
                &dags,
                &init_exact_flags,
                &mut best_weights,
                &mut best_errors,
                &mut best_exact,
                &mut stage0_steps,
            );
        }
        stage0_sweep_feature(
            "0c",
            "ngram_char_bi",
            &[0.0, 0.02, 0.05, 0.10],
            base_engine,
            compiled,
            &dags,
            &init_exact_flags,
            &mut best_weights,
            &mut best_errors,
            &mut best_exact,
            &mut stage0_steps,
        );
        if !ngram_only {
            stage0_sweep_feature(
                "0d",
                "multi_char_len",
                &[0.98, 0.995, 1.0113, 1.03],
                base_engine,
                compiled,
                &dags,
                &init_exact_flags,
                &mut best_weights,
                &mut best_errors,
                &mut best_exact,
                &mut stage0_steps,
            );
        }

        let refine_passes: &[(&str, &[f64])] = if ngram_only {
            &[
                ("0e-p1", &[-0.08, -0.04, -0.02, 0.02, 0.04, 0.08]),
                ("0e-p2", &[-0.02, -0.01, 0.01, 0.02]),
            ]
        } else {
            &[("0e", &[-0.25, -0.1, -0.05, 0.05, 0.1, 0.25])]
        };
        let refine_keys: &[&str] = if has_wt {
            &["ngram_char_tri", "ngram_word_bi", "ngram_word_tri", "ngram_char_bi"]
        } else {
            &["ngram_char_tri", "ngram_word_bi", "ngram_char_bi"]
        };
        for &(pass_tag, offsets) in refine_passes {
            for &k in refine_keys {
                let cur = best_weights.get(k).copied().unwrap_or(0.0);
                let grid: Vec<f64> = offsets.iter().map(|&d| cur + d).collect();
                stage0_sweep_feature(
                    pass_tag,
                    k,
                    &grid,
                    base_engine,
                    compiled,
                    &dags,
                    &init_exact_flags,
                    &mut best_weights,
                    &mut best_errors,
                    &mut best_exact,
                    &mut stage0_steps,
                );
            }
        }
    }

    // Stage A: 10 輪洗牌損失增強 AdaGrad 平均結構化感知機
    let w0_stage_a = best_weights.clone();
    let mut w = w0_stage_a.clone();
    let mut w_sum: HashMap<String, f64> =
        feature_keys.iter().map(|&k| (k.to_string(), 0.0)).collect();
    let mut grad_sq: HashMap<String, f64> =
        feature_keys.iter().map(|&k| (k.to_string(), 1.0)).collect();
    let mut total_steps = 0f64;

    let mut epoch_history = Vec::new();
    let mut coordinate_polish_steps = Vec::new();

    if !ngram_only {
        let mut work_engine = base_engine.clone();

        let epochs = 10;
        for ep in 1..=epochs {
            let mut rng = XorShift64::new(20261111 + ep as u64);
            let mut indices: Vec<usize> = (0..compiled.len()).collect();
            rng.shuffle(&mut indices);

            let eta = 0.012 / (ep as f64).sqrt();
            let mut updates_in_epoch = 0usize;

            for &idx in &indices {
                let Some((_, ref g_feats)) = gold_paths[idx] else {
                    continue;
                };
                work_engine.transitions.feature_weights = w.clone();
                let cs = &compiled[idx];
                let (pred_nodes, _) = work_engine.router.solve_best_path(
                    cs.syllables.len(),
                    &dags[idx],
                    Some(&mut work_engine.transitions),
                );
                let pred_str: String = pred_nodes.iter().map(|n| n.word.as_str()).collect();
                if pred_str != cs.item.text {
                    let err_scale = (char_levenshtein(&pred_str, &cs.item.text) as f64).sqrt().min(3.0);
                    let (p_feats, _) = work_engine.router.extract_path_features_and_triggers(
                        &pred_nodes,
                        &dags[idx],
                        &mut work_engine.transitions,
                    );
                    for &k in &feature_keys {
                        let gv = g_feats.get(k).copied().unwrap_or(0.0);
                        let pv = p_feats.get(k).copied().unwrap_or(0.0);
                        let diff = (gv - pv).clamp(-40.0, 40.0);
                        if diff.abs() > 1e-9 {
                            let eta_k = if k.starts_with("ngram_") {
                                0.06 / (ep as f64).sqrt()
                            } else {
                                eta
                            };
                            let g = (diff / 15.0) * err_scale;
                            let gsq = grad_sq.entry(k.to_string()).or_insert(1.0);
                            *gsq += g * g;
                            let def_w = default_feature_weight(k);
                            let cur_w = w.get(k).copied().unwrap_or(def_w);
                            let w_init = w0_stage_a.get(k).copied().unwrap_or(def_w);
                            let (lo, hi) = feature_bounds(k);
                            let next_w = (cur_w + eta_k * g / gsq.sqrt() - 0.02 * eta_k * (cur_w - w_init)).clamp(lo, hi);
                            w.insert(k.to_string(), next_w);
                        }
                    }
                    updates_in_epoch += 1;
                }
                total_steps += 1.0;
                for &k in &feature_keys {
                    *w_sum.entry(k.to_string()).or_insert(0.0) += w.get(k).copied().unwrap_or(default_feature_weight(k));
                }
            }

            let avg_w: HashMap<String, f64> = feature_keys
                .iter()
                .map(|&k| {
                    let v = if total_steps > 0.0 {
                        w_sum.get(k).copied().unwrap_or(0.0) / total_steps
                    } else {
                        w0_stage_a.get(k).copied().unwrap_or(default_feature_weight(k))
                    };
                    (k.to_string(), (v * 10000.0).round() / 10000.0)
                })
                .collect();

            let (ep_errors, ep_exact, fixed_vs_init, broken_vs_init) = eval_candidate_weights(
                base_engine,
                &avg_w,
                compiled,
                &dags,
                &init_exact_flags,
            );

            let ep_cer = ep_errors as f64 / total_chars.max(1) as f64;
            let ep_ser = ep_exact as f64 / compiled.len().max(1) as f64;
            println!(
                "[Calibration] Epoch {}/{}: updates={}, dev CER={:.2}% ({} errors, fixed={}, broken={}), SER={:.2}%",
                ep,
                epochs,
                updates_in_epoch,
                ep_cer * 100.0,
                ep_errors,
                fixed_vs_init,
                broken_vs_init,
                ep_ser * 100.0
            );

            epoch_history.push(json!({
                "epoch": ep,
                "updates": updates_in_epoch,
                "dev_cer": ep_cer,
                "dev_cer_percent": format!("{:.2}%", ep_cer * 100.0),
                "dev_char_errors": ep_errors,
                "dev_exact_matches": ep_exact,
                "fixed_vs_init": fixed_vs_init,
                "broken_vs_init": broken_vs_init,
                "dev_sentence_accuracy_percent": format!("{:.2}%", ep_ser * 100.0),
                "averaged_weights": avg_w
            }));

            if (ep_errors < best_errors || (ep_errors == best_errors && ep_exact > best_exact))
                && broken_vs_init <= fixed_vs_init
            {
                best_errors = ep_errors;
                best_exact = ep_exact;
                best_weights = avg_w;
            }
        }

        // Stage B: 雙輪特徵座標軸精細拋光 (Coordinate Line-Search Refinement)
        println!(
            "[Calibration] 進入 Stage B: 雙輪特徵座標軸精細拋光 (Coordinate Line-Search Refinement)，基準錯誤數: {}",
            best_errors
        );
        let deltas = [-0.03, -0.015, -0.006, 0.006, 0.015, 0.03];

        for pass in 1..=2 {
            println!("[Calibration] Coordinate Polish Pass {}/2...", pass);
            for &k in &feature_keys {
                let base_val = best_weights
                    .get(k)
                    .copied()
                    .unwrap_or(default_feature_weight(k));
                let (lo, hi) = feature_bounds(k);
                let deltas_k: &[f64] = if k.starts_with("ngram_") {
                    &[-0.25, -0.10, -0.04, 0.04, 0.10, 0.25]
                } else {
                    &deltas
                };

                for &d in deltas_k {
                    let cand_val = ((base_val + d).clamp(lo, hi) * 10000.0).round() / 10000.0;
                    if (cand_val - base_val).abs() < 1e-6 {
                        continue;
                    }
                    let mut cand_w = best_weights.clone();
                    cand_w.insert(k.to_string(), cand_val);

                    let (c_errors, c_exact, c_fixed, c_broken) = eval_candidate_weights(
                        base_engine,
                        &cand_w,
                        compiled,
                        &dags,
                        &init_exact_flags,
                    );

                    if (c_errors < best_errors || (c_errors == best_errors && c_exact > best_exact))
                        && c_broken <= c_fixed
                    {
                        println!(
                            "  [Polish] 特徵 {}: {:.4} -> {:.4} (錯誤數 {} -> {}, 正確率 {} -> {}, fix={}, brk={})",
                            k, base_val, cand_val, best_errors, c_errors, best_exact, c_exact, c_fixed, c_broken
                        );
                        coordinate_polish_steps.push(json!({
                            "pass": pass,
                            "feature": k,
                            "old_val": base_val,
                            "new_val": cand_val,
                            "old_errors": best_errors,
                            "new_errors": c_errors,
                            "old_exact": best_exact,
                            "new_exact": c_exact,
                            "fixed": c_fixed,
                            "broken": c_broken
                        }));
                        best_errors = c_errors;
                        best_exact = c_exact;
                        best_weights.insert(k.to_string(), cand_val);
                    }
                }
            }
        }
    } else {
        println!(
            "[Calibration] --ngram-only 模式：鎖定 24 項既有規則權重，完成 N-gram 三權重優化（錯誤數 {}）",
            best_errors
        );
    }

    // 寫出最佳權重至 feature_weights.tsv
    let mut tsv_lines = vec![
        "# RuhI 線性評估模型特徵與規則族權重表 (Averaged Structured Perceptron Calibrated)".to_string(),
        "# 格式: feature_or_family_name\tweight_multiplier".to_string(),
    ];
    let f_c1 = best_weights.get("freq_ln_char1").copied().unwrap_or(1.0);
    let f_cmp = best_weights.get("freq_ln_compound").copied().unwrap_or(1.0);
    let f_legacy = (0.5 * (f_c1 + f_cmp) * 10000.0).round() / 10000.0;
    tsv_lines.push(format!("freq_ln\t{f_legacy:.4}"));

    for &k in &feature_keys {
        let val = best_weights
            .get(k)
            .copied()
            .unwrap_or(default_feature_weight(k));
        tsv_lines.push(format!("{k}\t{val:.4}"));
    }

    let c_topo = best_weights.get("construction_topological_core").copied().unwrap_or(1.0);
    tsv_lines.push(format!("construction_hierarchical_matrix\t{c_topo:.4}"));

    let tsv_content = tsv_lines.join("\n") + "\n";

    let target_paths = [
        root.join("ruhi_lexicon").join("data").join("feature_weights.tsv"),
        root.join("data").join("feature_weights.tsv"),
        root.join("dist").join("data").join("feature_weights.tsv"),
        root.join("scripts").join("data").join("feature_weights.tsv"),
        root.join("dist").join("RuhI_v0.7.0_Windows_x64").join("data").join("feature_weights.tsv"),
    ];
    for p in &target_paths {
        if let Some(parent) = p.parent() {
            if parent.is_dir() {
                let _ = fs::write(p, &tsv_content);
            }
        }
    }

    let best_cer = best_errors as f64 / total_chars.max(1) as f64;
    let best_ser = best_exact as f64 / compiled.len().max(1) as f64;
    json!({
        "status": "ok",
        "initial_dev_cer": init_cer,
        "initial_dev_cer_percent": format!("{:.2}%", init_cer * 100.0),
        "initial_dev_char_errors": init_errors,
        "initial_dev_sentence_accuracy_percent": format!("{:.2}%", init_ser * 100.0),
        "calibrated_dev_cer": best_cer,
        "calibrated_dev_cer_percent": format!("{:.2}%", best_cer * 100.0),
        "calibrated_dev_char_errors": best_errors,
        "calibrated_dev_sentence_accuracy_percent": format!("{:.2}%", best_ser * 100.0),
        "delta_dev_cer": best_cer - init_cer,
        "delta_char_errors": (best_errors as isize) - (init_errors as isize),
        "unreachable_gold_sentences": unreachable_count,
        "unreachable_gold_ratio": format!("{:.2}%", (unreachable_count as f64 / compiled.len().max(1) as f64) * 100.0),
        "best_weights": best_weights,
        "stage0_ngram_warmstart_steps": stage0_steps,
        "epoch_history": epoch_history,
        "coordinate_polish_steps": coordinate_polish_steps
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

#[inline]
fn round4_f64(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn summarize_latency_us(samples: &mut [f64]) -> serde_json::Value {
    if samples.is_empty() {
        return json!({
            "mean_us": 0.0,
            "p50_us": 0.0,
            "p95_us": 0.0,
            "p99_us": 0.0,
            "max_us": 0.0,
            "mean_ms": 0.0,
            "p99_ms": 0.0
        });
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = samples.len();
    let mean = samples.iter().sum::<f64>() / (n as f64);
    let p50 = samples[n / 2];
    let p95 = samples[((n as f64) * 0.95) as usize];
    let p99 = samples[((n as f64) * 0.99) as usize];
    let max_v = samples[n - 1];
    json!({
        "mean_us": round4_f64(mean),
        "p50_us": round4_f64(p50),
        "p95_us": round4_f64(p95),
        "p99_us": round4_f64(p99),
        "max_us": round4_f64(max_v),
        "mean_ms": round4_f64(mean / 1000.0),
        "p99_ms": round4_f64(p99 / 1000.0)
    })
}

fn evaluate_longkan_mode(
    engine: &mut RuhiEngine,
    items: &[CorpusItem],
    set_name: &str,
) -> serde_json::Value {
    let t_init = Instant::now();
    engine.longkan.table.ensure_loaded();
    let load_ms = t_init.elapsed().as_secs_f64() * 1000.0;

    fn run_pass(
        engine: &mut RuhiEngine,
        items: &[CorpusItem],
        use_transition_bonus: bool,
    ) -> serde_json::Value {
        engine.longkan.use_transition_bonus = use_transition_bonus;
        let mut total_chars = 0usize;
        let mut covered_chars = 0usize;
        let mut top1_hits = 0usize;
        let mut top5_hits = 0usize;
        let mut top9_hits = 0usize;
        let mut reachable_hits = 0usize;
        let mut rank_sum = 0usize;
        let mut total_keystrokes = 0usize;
        let mut query_latencies_us: Vec<f64> = Vec::new();

        for item in items {
            let mut prev_committed: Option<String> = None;
            for ch in item.text.chars() {
                if ch.is_whitespace()
                    || ('\u{3000}'..='\u{303F}').contains(&ch)
                    || ('\u{FF00}'..='\u{FFEF}').contains(&ch)
                {
                    continue;
                }
                let s = ch.to_string();
                total_chars += 1;
                let primary_opt = engine
                    .longkan
                    .table
                    .reverse_lookup_char(ch)
                    .map(|info| info.primary_code);
                let Some(code) = primary_opt else {
                    total_keystrokes += 5;
                    prev_committed = Some(s);
                    continue;
                };
                if code.is_empty() {
                    total_keystrokes += 5;
                    prev_committed = Some(s);
                    continue;
                }
                covered_chars += 1;

                let t_q = Instant::now();
                engine.longkan.reset();
                engine.longkan.last_committed_word = prev_committed.clone().unwrap_or_default();
                engine.longkan.buffer = code.clone();
                engine.longkan.update_candidates_with_context(
                    Some(&engine.lexicon),
                    Some(&mut engine.transitions),
                );
                let elapsed_us = t_q.elapsed().as_secs_f64() * 1_000_000.0;
                query_latencies_us.push(elapsed_us);

                let rank_opt = engine
                    .longkan
                    .candidates
                    .iter()
                    .position(|c| c == &s)
                    .map(|idx| idx + 1);

                if let Some(rank) = rank_opt {
                    reachable_hits += 1;
                    rank_sum += rank;
                    if rank == 1 {
                        top1_hits += 1;
                        total_keystrokes += code.len() + 1;
                    } else if rank <= 5 {
                        top5_hits += 1;
                        top9_hits += 1;
                        total_keystrokes += code.len() + 1;
                    } else if rank <= 9 {
                        top9_hits += 1;
                        total_keystrokes += code.len() + 1;
                    } else {
                        let page_turns = (rank - 1) / 9;
                        total_keystrokes += code.len() + page_turns + 1;
                    }
                } else {
                    rank_sum += 20;
                    total_keystrokes += code.len() + 3;
                }
                prev_committed = Some(s);
            }
        }

        let denom = covered_chars.max(1) as f64;
        let kspc = if total_chars > 0 {
            total_keystrokes as f64 / total_chars as f64
        } else {
            0.0
        };
        let top1_rate = top1_hits as f64 / denom;
        let top5_rate = (top1_hits + top5_hits) as f64 / denom;
        let top9_rate = (top1_hits + top9_hits) as f64 / denom;
        let avg_rank = rank_sum as f64 / denom;

        json!({
            "use_transition_bonus": use_transition_bonus,
            "total_chars": total_chars,
            "covered_chars": covered_chars,
            "coverage_percent": round4_f64((covered_chars as f64 / total_chars.max(1) as f64) * 100.0),
            "top1_hits": top1_hits,
            "top1_hit_rate_percent": round4_f64(top1_rate * 100.0),
            "top5_hit_rate_percent": round4_f64(top5_rate * 100.0),
            "top9_hit_rate_percent": round4_f64(top9_rate * 100.0),
            "reachable_rate_percent": round4_f64((reachable_hits as f64 / denom) * 100.0),
            "avg_candidate_rank": round4_f64(avg_rank),
            "total_keystrokes": total_keystrokes,
            "kspc": round4_f64(kspc),
            "query_latency_us": summarize_latency_us(&mut query_latencies_us),
        })
    }

    let without_transitions = run_pass(engine, items, false);
    let with_transitions = run_pass(engine, items, true);
    engine.longkan.use_transition_bonus = false;

    let top1_diff = with_transitions["top1_hits"].as_i64().unwrap_or(0)
        - without_transitions["top1_hits"].as_i64().unwrap_or(0);
    let rank_diff = with_transitions["avg_candidate_rank"].as_f64().unwrap_or(0.0)
        - without_transitions["avg_candidate_rank"].as_f64().unwrap_or(0.0);

    json!({
        "mode": "longkan",
        "dataset": set_name,
        "total_sentences": items.len(),
        "first_load_ms": round4_f64(load_ms),
        "mmap_bytes": engine.longkan.table.mmap_bytes(),
        "baseline_without_transition_bonus": without_transitions,
        "with_transition_bonus": with_transitions,
        "transition_engine_net_diff": {
            "top1_hits_delta": top1_diff,
            "avg_candidate_rank_delta": round4_f64(rank_diff),
        }
    })
}

fn run_nbest_oracle_benchmark(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    set_name: &str,
    top_k: usize,
    diverse_nbest: bool,
) -> Value {
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    struct OracleRec {
        chars: usize,
        top1_errors: usize,
        oracle_errors: usize,
        top1_exact: bool,
        oracle_exact: bool,
        pron_err_top1: bool,
        pron_err_oracle_hit: bool,
        latency_us: f64,
    }

    let chunks: Vec<Vec<OracleRec>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size.max(1)) {
            let mut worker = base_engine.clone();
            handles.push(s.spawn(move || {
                let mut out = Vec::with_capacity(chunk.len());
                let dag_builder = LatticeDAG::new(&worker.lexicon);
                for cs in chunk {
                    let t0 = Instant::now();
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let nbest = if diverse_nbest {
                        let astar_k = if cs.syllables.len() >= 15 { 16 } else { 12 };
                        let max_k = top_k.max(if cs.syllables.len() >= 15 { 40 } else { 28 });
                        worker.router.solve_diverse_nbest_paths_with_prefix(
                            cs.syllables.len(),
                            &dag,
                            Some(&mut worker.transitions),
                            None,
                            astar_k,
                            max_k,
                        )
                    } else {
                        worker.router.solve_nbest_paths(
                            cs.syllables.len(),
                            &dag,
                            Some(&mut worker.transitions),
                            top_k,
                        )
                    };
                    let (top1_nodes, _) = worker.router.solve_best_path(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker.transitions),
                    );
                    let elapsed_us = t0.elapsed().as_secs_f64() * 1_000_000.0;
                    let gold = &cs.item.text;
                    let chars = gold.chars().count();
                    let top1_pred: String = top1_nodes.iter().map(|n| n.word.as_str()).collect();
                    let top1_errors = char_levenshtein(&top1_pred, gold);
                    let top1_exact = top1_errors == 0;

                    let mut oracle_errors = top1_errors;
                    let mut oracle_exact = top1_exact;
                    for (cand_nodes, _) in &nbest {
                        let cand_str: String = cand_nodes.iter().map(|n| n.word.as_str()).collect();
                        let e = char_levenshtein(&cand_str, gold);
                        if e < oracle_errors {
                            oracle_errors = e;
                        }
                        if e == 0 {
                            oracle_exact = true;
                        }
                    }

                    let mut pron_err_top1 = false;
                    if !top1_exact && top1_pred.chars().count() == chars {
                        pron_err_top1 = top1_pred.chars().zip(gold.chars()).any(|(p, g)| {
                            p != g && matches!(p, '他' | '她' | '它' | '牠') && matches!(g, '他' | '她' | '它' | '牠')
                        });
                    }
                    let pron_err_oracle_hit = if pron_err_top1 {
                        nbest.iter().any(|(cand_nodes, _)| {
                            let s: String = cand_nodes.iter().map(|n| n.word.as_str()).collect();
                            char_levenshtein(&s, gold) < top1_errors
                        })
                    } else {
                        false
                    };

                    out.push(OracleRec {
                        chars,
                        top1_errors,
                        oracle_errors,
                        top1_exact,
                        oracle_exact,
                        pron_err_top1,
                        pron_err_oracle_hit,
                        latency_us: elapsed_us,
                    });
                }
                out
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut total_chars = 0usize;
    let mut top1_err_sum = 0usize;
    let mut oracle_err_sum = 0usize;
    let mut top1_exact_cnt = 0usize;
    let mut oracle_exact_cnt = 0usize;
    let mut err_sentences = 0usize;
    let mut oracle_recovered_exact = 0usize;
    let mut pron_err_cnt = 0usize;
    let mut pron_oracle_hit_cnt = 0usize;
    let mut latencies = Vec::with_capacity(compiled.len());

    #[derive(Default, Clone, Copy)]
    struct BucketAgg {
        sentences: usize,
        chars: usize,
        top1_err: usize,
        oracle_err: usize,
        top1_exact: usize,
        oracle_exact: usize,
    }
    let bucket_defs: [(&str, usize, usize); 4] = [
        ("1-7", 1, 7),
        ("8-14", 8, 14),
        ("15-24", 15, 24),
        ("25+", 25, usize::MAX),
    ];
    let mut bucket_aggs = [BucketAgg::default(); 4];

    for part in chunks {
        for r in part {
            total_chars += r.chars;
            top1_err_sum += r.top1_errors;
            oracle_err_sum += r.oracle_errors;
            if r.top1_exact {
                top1_exact_cnt += 1;
            } else {
                err_sentences += 1;
                if r.oracle_exact {
                    oracle_recovered_exact += 1;
                }
            }
            if r.oracle_exact {
                oracle_exact_cnt += 1;
            }
            if r.pron_err_top1 {
                pron_err_cnt += 1;
                if r.pron_err_oracle_hit {
                    pron_oracle_hit_cnt += 1;
                }
            }
            latencies.push(r.latency_us);

            for (bi, &(_, lo, hi)) in bucket_defs.iter().enumerate() {
                if r.chars >= lo && r.chars <= hi {
                    let b = &mut bucket_aggs[bi];
                    b.sentences += 1;
                    b.chars += r.chars;
                    b.top1_err += r.top1_errors;
                    b.oracle_err += r.oracle_errors;
                    if r.top1_exact {
                        b.top1_exact += 1;
                    }
                    if r.oracle_exact {
                        b.oracle_exact += 1;
                    }
                    break;
                }
            }
        }
    }

    let mut by_length_bucket = serde_json::Map::new();
    for (bi, &(label, _, _)) in bucket_defs.iter().enumerate() {
        let b = bucket_aggs[bi];
        by_length_bucket.insert(
            label.to_string(),
            json!({
                "sentences": b.sentences,
                "chars": b.chars,
                "top1_char_errors": b.top1_err,
                "top1_cer_percent": format!("{:.2}%", (b.top1_err as f64 / b.chars.max(1) as f64) * 100.0),
                "oracle_char_errors": b.oracle_err,
                "oracle_cer_percent": format!("{:.2}%", (b.oracle_err as f64 / b.chars.max(1) as f64) * 100.0),
                "top1_exact_match": b.top1_exact,
                "top1_exact_percent": format!("{:.2}%", (b.top1_exact as f64 / b.sentences.max(1) as f64) * 100.0),
                "oracle_exact_match": b.oracle_exact,
                "oracle_exact_percent": format!("{:.2}%", (b.oracle_exact as f64 / b.sentences.max(1) as f64) * 100.0),
            }),
        );
    }

    let n_sent = compiled.len().max(1);
    json!({
        "dataset": set_name,
        "top_k": top_k,
        "diverse_nbest": diverse_nbest,
        "total_sentences": compiled.len(),
        "total_chars": total_chars,
        "top1_char_errors": top1_err_sum,
        "top1_cer_percent": format!("{:.2}%", (top1_err_sum as f64 / total_chars.max(1) as f64) * 100.0),
        "top1_exact_match": top1_exact_cnt,
        "top1_sentence_accuracy_percent": format!("{:.2}%", (top1_exact_cnt as f64 / n_sent as f64) * 100.0),
        "oracle_char_errors": oracle_err_sum,
        "oracle_cer_percent": format!("{:.2}%", (oracle_err_sum as f64 / total_chars.max(1) as f64) * 100.0),
        "oracle_exact_match": oracle_exact_cnt,
        "oracle_sentence_accuracy_percent": format!("{:.2}%", (oracle_exact_cnt as f64 / n_sent as f64) * 100.0),
        "erroneous_top1_sentences": err_sentences,
        "oracle_exact_recall_on_top1_errors_percent": format!("{:.2}%", (oracle_recovered_exact as f64 / err_sentences.max(1) as f64) * 100.0),
        "pronoun_error_sentences": pron_err_cnt,
        "pronoun_oracle_improved_sentences": pron_oracle_hit_cnt,
        "pronoun_oracle_recall_percent": format!("{:.2}%", (pron_oracle_hit_cnt as f64 / pron_err_cnt.max(1) as f64) * 100.0),
        "by_length_bucket": by_length_bucket,
        "nbest_latency_us": summarize_latency_us(&mut latencies),
    })
}

/// 匯出 dev 開發集 Top-K N-Best 候選與結構／神經特徵至 JSONL，供 v1.0 神經重排序 5 折交叉驗證使用
fn dump_nbest_jsonl(
    base_engine: &RuhiEngine,
    compiled: &[CompiledSentence],
    top_k: usize,
    diverse_nbest: bool,
    neural_model: Option<&std::sync::Arc<ruhi::router::rlnm::RlnmModel>>,
    out_path: &Path,
) -> Value {
    use ruhi::router::classical_register::ClassicalRegisterProfile;
    use ruhi::router::discourse_reranker::{
        dag_has_third_person_pronoun, score_discourse_sentence, RERANK_SCORE_MARGIN,
    };
    use std::io::{BufWriter, Write};

    struct CandDumpRow {
        text: String,
        base_score: f64,
        word_count: usize,
        single_char_count: usize,
        min_node_weight: u32,
        avg_node_weight: f64,
        discourse_delta: f64,
    }

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    let chunks: Vec<Vec<(String, usize, usize, bool, bool)>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size.max(1)) {
            let mut worker = base_engine.clone();
            let model_ref = neural_model;
            handles.push(s.spawn(move || {
                let mut kv_cache = model_ref.map(|m| m.alloc_kv_cache());
                let mut out = Vec::with_capacity(chunk.len());
                let dag_builder = LatticeDAG::new(&worker.lexicon);
                for cs in chunk {
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let nbest = if diverse_nbest {
                        let astar_k = if cs.syllables.len() >= 15 { 16 } else { 12 };
                        let max_k = top_k.max(if cs.syllables.len() >= 15 { 40 } else { 28 });
                        worker.router.solve_diverse_nbest_paths_with_prefix(
                            cs.syllables.len(),
                            &dag,
                            Some(&mut worker.transitions),
                            None,
                            astar_k,
                            max_k,
                        )
                    } else {
                        worker.router.solve_nbest_paths(
                            cs.syllables.len(),
                            &dag,
                            Some(&mut worker.transitions),
                            top_k,
                        )
                    };
                    let (top1_nodes, _) = worker.router.solve_best_path(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker.transitions),
                    );
                    let top1_pred: String = top1_nodes.iter().map(|n| n.word.as_str()).collect();
                    let gold = &cs.item.text;
                    let chars = gold.chars().count();
                    let top1_errors = char_levenshtein(&top1_pred, gold);

                    let classical_active = ClassicalRegisterProfile::detect(
                        worker.router.classical_mode,
                        cs.syllables.len(),
                        &dag,
                    )
                    .active;
                    let apply_discourse = cs.syllables.len() >= 4
                        && !classical_active
                        && !worker.transitions.is_rule_disabled("discourse_reranker")
                        && dag_has_third_person_pronoun(&dag);

                    let raw_top1_sc = nbest.first().map(|(_, sc)| *sc).unwrap_or(0.0);
                    let mut cands: Vec<CandDumpRow> = nbest
                        .iter()
                        .map(|(nodes, base_sc)| {
                            let txt: String = nodes.iter().map(|n| n.word.as_str()).collect();
                            let d_delta =
                                if apply_discourse && (raw_top1_sc - *base_sc) <= RERANK_SCORE_MARGIN {
                                    score_discourse_sentence(nodes)
                                } else {
                                    0.0
                                };
                            let adj = *base_sc + d_delta;
                            let word_count = nodes.len();
                            let single_char_count = nodes
                                .iter()
                                .filter(|n| n.word.chars().count() == 1)
                                .count();
                            let min_node_weight =
                                nodes.iter().map(|n| n.weight).min().unwrap_or(0);
                            let avg_node_weight = if word_count == 0 {
                                0.0
                            } else {
                                nodes.iter().map(|n| n.weight as f64).sum::<f64>()
                                    / (word_count as f64)
                            };
                            CandDumpRow {
                                text: txt,
                                base_score: adj,
                                word_count,
                                single_char_count,
                                min_node_weight,
                                avg_node_weight,
                                discourse_delta: d_delta,
                            }
                        })
                        .collect();
                    cands.sort_by(|a, b| {
                        b.base_score
                            .partial_cmp(&a.base_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let neural_pairs: Option<Vec<(f32, f32)>> =
                        if let (Some(m), Some(cache)) = (model_ref, kv_cache.as_mut()) {
                            let refs: Vec<&str> = cands.iter().map(|c| c.text.as_str()).collect();
                            Some(m.score_candidates_trie_detailed("", &refs, cache, None))
                        } else {
                            None
                        };

                    let mut oracle_errors = top1_errors;
                    let mut oracle_exact = top1_errors == 0;
                    let cand_json: Vec<Value> = cands
                        .iter()
                        .enumerate()
                        .map(|(idx, c)| {
                            let e = char_levenshtein(&c.text, gold);
                            if e < oracle_errors {
                                oracle_errors = e;
                            }
                            if e == 0 {
                                oracle_exact = true;
                            }
                            let mut obj = serde_json::Map::new();
                            obj.insert("text".to_string(), json!(c.text));
                            obj.insert("base_score".to_string(), json!(c.base_score));
                            obj.insert("word_count".to_string(), json!(c.word_count));
                            obj.insert("single_char_count".to_string(), json!(c.single_char_count));
                            obj.insert("min_node_weight".to_string(), json!(c.min_node_weight));
                            obj.insert("avg_node_weight".to_string(), json!(c.avg_node_weight));
                            obj.insert("discourse_delta".to_string(), json!(c.discourse_delta));
                            if let Some(ref pairs) = neural_pairs {
                                if let Some(&(nlp, min_nlp)) = pairs.get(idx) {
                                    obj.insert("neural_logp".to_string(), json!(nlp));
                                    obj.insert("neural_min_char_logp".to_string(), json!(min_nlp));
                                }
                            }
                            Value::Object(obj)
                        })
                        .collect();

                    let line_val = json!({
                        "id": cs.item.id,
                        "category": cs.item.category,
                        "gold": gold,
                        "top1": top1_pred,
                        "candidates": cand_json,
                    });
                    out.push((
                        serde_json::to_string(&line_val).unwrap(),
                        chars,
                        top1_errors,
                        top1_errors == 0,
                        oracle_exact,
                    ));
                }
                out
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    if let Some(parent) = out_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = fs::File::create(out_path)
        .unwrap_or_else(|e| panic!("無法建立 N-Best 輸出檔 {}: {}", out_path.display(), e));
    let mut writer = BufWriter::new(file);

    let mut total_sentences = 0usize;
    let mut total_chars = 0usize;
    let mut top1_err_sum = 0usize;
    let mut top1_exact_cnt = 0usize;
    let mut oracle_exact_cnt = 0usize;

    for part in chunks {
        for (line, chars, t_err, t_exact, o_exact) in part {
            writeln!(writer, "{}", line).unwrap();
            total_sentences += 1;
            total_chars += chars;
            top1_err_sum += t_err;
            if t_exact {
                top1_exact_cnt += 1;
            }
            if o_exact {
                oracle_exact_cnt += 1;
            }
        }
    }
    writer.flush().unwrap();

    json!({
        "dataset": "dev",
        "out_path": out_path.display().to_string(),
        "top_k": top_k,
        "total_sentences": total_sentences,
        "total_chars": total_chars,
        "top1_char_errors": top1_err_sum,
        "top1_cer_percent": format!("{:.2}%", (top1_err_sum as f64 / total_chars.max(1) as f64) * 100.0),
        "top1_exact_match": top1_exact_cnt,
        "oracle_exact_match": oracle_exact_cnt,
    })
}

/// 自已篩除評測重疊之純淨語料庫匯出真實 Viterbi 多樣性困難負樣本與邊際候選（供 Wave C Listwise 重排微調）
fn dump_train_corpus_nbest(
    base_engine: &RuhiEngine,
    corpus_txt_path: &Path,
    out_jsonl_path: &Path,
    max_sents: usize,
) -> Value {
    use ruhi::router::classical_register::ClassicalRegisterProfile;
    use ruhi::router::discourse_reranker::{
        dag_has_third_person_pronoun, score_discourse_sentence, RERANK_SCORE_MARGIN,
    };
    use ruhi::router::neural_reranker::structural_fusion_bonus;
    use std::io::{BufRead, BufReader, BufWriter, Write};

    let t0 = Instant::now();
    let compiler = PhoneticCompiler::from_lexicon(&base_engine.lexicon);

    let file = fs::File::open(corpus_txt_path)
        .unwrap_or_else(|e| panic!("無法開啟訓練語料檔 {}: {}", corpus_txt_path.display(), e));
    let reader = BufReader::new(file);

    let mut compiled: Vec<CompiledSentence> = Vec::new();
    let mut scanned_lines = 0usize;
    let mut skipped_polyphonic = 0usize;

    for (idx, line_res) in reader.lines().enumerate() {
        if compiled.len() >= max_sents {
            break;
        }
        let Ok(raw) = line_res else { continue };
        let text = raw.trim();
        let clen = text.chars().count();
        if !(6..=32).contains(&clen) || !text.chars().all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)) {
            continue;
        }
        scanned_lines += 1;
        let item = CorpusItem {
            id: format!("train_{idx}"),
            category: "corpus".to_string(),
            text: text.to_string(),
        };
        if let Some(cs) = compiler.compile(&item) {
            if cs.has_polyphonic_single_char {
                skipped_polyphonic += 1;
                continue;
            }
            compiled.push(cs);
        }
    }
    let compiled_clean = compiled.len();

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    let chunk_size = (compiled.len() + n_threads - 1) / n_threads.max(1);

    let chunks: Vec<Vec<(String, bool, bool)>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in compiled.chunks(chunk_size.max(1)) {
            let mut worker = base_engine.clone();
            handles.push(s.spawn(move || {
                let mut out: Vec<(String, bool, bool)> = Vec::new();
                let dag_builder = LatticeDAG::new(&worker.lexicon);
                for cs in chunk {
                    let dag = dag_builder.build_dag(&cs.syllables, true);
                    let astar_k = if cs.syllables.len() >= 15 { 16 } else { 12 };
                    let max_k = if cs.syllables.len() >= 15 { 32 } else { 24 };
                    let nbest = worker.router.solve_diverse_nbest_paths_with_prefix(
                        cs.syllables.len(),
                        &dag,
                        Some(&mut worker.transitions),
                        None,
                        astar_k,
                        max_k,
                    );
                    if nbest.len() < 2 {
                        continue;
                    }

                    let classical_active = ClassicalRegisterProfile::detect(
                        worker.router.classical_mode,
                        cs.syllables.len(),
                        &dag,
                    )
                    .active;
                    let apply_discourse = cs.syllables.len() >= 4
                        && !classical_active
                        && !worker.transitions.is_rule_disabled("discourse_reranker")
                        && dag_has_third_person_pronoun(&dag);
                    let raw_top1_sc = nbest[0].1;

                    let mut dedup: HashMap<String, f64> = HashMap::with_capacity(nbest.len());
                    for (nodes, base_sc) in &nbest {
                        let txt: String = nodes.iter().map(|n| n.word.as_str()).collect();
                        let d_delta = if apply_discourse && (raw_top1_sc - *base_sc) <= RERANK_SCORE_MARGIN {
                            score_discourse_sentence(nodes)
                        } else {
                            0.0
                        };
                        let non_neural = *base_sc + d_delta + structural_fusion_bonus(nodes, d_delta, 6.0);
                        match dedup.get_mut(&txt) {
                            Some(prev) => {
                                if non_neural > *prev {
                                    *prev = non_neural;
                                }
                            }
                            None => {
                                dedup.insert(txt, non_neural);
                            }
                        }
                    }

                    if dedup.len() < 2 {
                        continue;
                    }
                    let mut cands: Vec<(String, f64)> = dedup.into_iter().collect();
                    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                    let gold = &cs.item.text;
                    let Some(full_gold_idx) = cands.iter().position(|(t, _)| t == gold) else {
                        continue;
                    };
                    let top_sc = cands[0].1;
                    let gold_not_top1 = full_gold_idx != 0;
                    let thin_margin_top1 = !gold_not_top1 && (top_sc - cands[1].1) <= 15.0;
                    if !gold_not_top1 && !thin_margin_top1 {
                        continue;
                    }

                    // 保留最多 12 名近距離競爭候選（容許誤差視窗 25.0），並保證納入正確解答
                    let mut selected: Vec<(String, f64)> = Vec::with_capacity(12);
                    for (txt, sc) in &cands {
                        if selected.len() >= 12 {
                            break;
                        }
                        if selected.len() >= 2 && top_sc - *sc > 25.0 {
                            break;
                        }
                        selected.push((txt.clone(), *sc));
                    }
                    if !selected.iter().any(|(t, _)| t == gold) {
                        if selected.len() >= 12 {
                            selected.pop();
                        }
                        selected.push((gold.clone(), cands[full_gold_idx].1));
                    }
                    selected.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let Some(final_gold_idx) = selected.iter().position(|(t, _)| t == gold) else {
                        continue;
                    };
                    if selected.len() < 2 {
                        continue;
                    }

                    let row_json = json!({
                        "gold_idx": final_gold_idx,
                        "cands": selected.iter().map(|(t, sc)| json!([t, ((*sc as f32) * 1000.0).round() / 1000.0])).collect::<Vec<_>>(),
                    });
                    out.push((row_json.to_string(), gold_not_top1, thin_margin_top1));
                }
                out
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    if let Some(parent) = out_jsonl_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let out_f = fs::File::create(out_jsonl_path)
        .unwrap_or_else(|e| panic!("無法建立訓練 N-Best 輸出檔 {}: {}", out_jsonl_path.display(), e));
    let mut writer = BufWriter::new(out_f);

    let mut kept_hard_or_thin = 0usize;
    let mut gold_not_top1_cnt = 0usize;
    let mut thin_margin_cnt = 0usize;
    for part in chunks {
        for (line, is_hard, is_thin) in part {
            writeln!(writer, "{}", line).unwrap();
            kept_hard_or_thin += 1;
            if is_hard {
                gold_not_top1_cnt += 1;
            }
            if is_thin {
                thin_margin_cnt += 1;
            }
        }
    }
    writer.flush().unwrap();

    json!({
        "corpus_txt": corpus_txt_path.display().to_string(),
        "out_jsonl": out_jsonl_path.display().to_string(),
        "scanned_lines": scanned_lines,
        "skipped_polyphonic": skipped_polyphonic,
        "compiled_clean": compiled_clean,
        "kept_hard_or_thin": kept_hard_or_thin,
        "gold_not_top1": gold_not_top1_cnt,
        "thin_margin_top1": thin_margin_cnt,
        "elapsed_secs": ((t0.elapsed().as_secs_f64()) * 100.0).round() / 100.0,
    })
}

fn run_longkan_benchmark(root: &Path) -> serde_json::Value {
    let data_dir = root.join("ruhi_lexicon").join("data");
    let t0 = Instant::now();
    let engine = RuhiEngine::new_with_data_dir(&data_dir.to_string_lossy());
    let init_unloaded_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let is_loaded_initially = engine.longkan.table.is_loaded();

    let t_load = Instant::now();
    engine.longkan.table.ensure_loaded();
    let first_load_ms = t_load.elapsed().as_secs_f64() * 1000.0;
    let mmap_bytes = engine.longkan.table.mmap_bytes();

    let test_codes = ["j", "co", "koko", "s", "mu", "xnsn", "bdyd", "cocn", "j*n", "c?c*"];
    let mut per_code_us = serde_json::Map::new();
    for code in test_codes {
        let mut samples = Vec::with_capacity(200);
        for _ in 0..200 {
            let t_q = Instant::now();
            let cands = engine.longkan.table.lookup(code);
            std::hint::black_box(&cands);
            samples.push(t_q.elapsed().as_secs_f64() * 1_000_000.0);
        }
        let stats = summarize_latency_us(&mut samples);
        per_code_us.insert(code.to_string(), stats);
    }

    json!({
        "engine_init_without_longkan_ms": round4_f64(init_unloaded_ms),
        "longkan_loaded_on_init": is_loaded_initially,
        "longkan_first_load_ms": round4_f64(first_load_ms),
        "longkan_mmap_bytes": mmap_bytes,
        "longkan_mmap_mb": round4_f64(mmap_bytes as f64 / (1024.0 * 1024.0)),
        "per_keystroke_query_latency": per_code_us,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = resolve_ruhi_root();
    let data_dir = root.join("ruhi_lexicon").join("data");

    if args.iter().any(|a| a == "--bench-longkan") {
        let res = run_longkan_benchmark(&root);
        println!("{}", serde_json::to_string_pretty(&res).unwrap());
        return;
    }

    let mut engine = RuhiEngine::new_with_data_dir(&data_dir.to_string_lossy());
    if let Some(pos) = args.iter().position(|a| a == "--ngram-bin") {
        if let Some(p) = args.get(pos + 1) {
            engine
                .transitions
                .load_ngram_lm_bin(p)
                .unwrap_or_else(|e| panic!("無法載入 --ngram-bin {}: {}", p, e));
        }
    }

    if args.iter().any(|a| a == "--build-corpus") {
        let res = build_and_seal_evaluation_corpus(&root, &engine);
        println!("{}", serde_json::to_string_pretty(&res).unwrap());
        return;
    }

    let mut set_name = "dev".to_string();
    let mut mode_name = "bopomofo".to_string();
    let mut out_path: Option<PathBuf> = None;
    let mut baseline_path: Option<PathBuf> = None;
    let mut do_ablation = false;
    let mut do_lexicon_ablation = false;
    let mut do_strategy_bench = false;
    let mut do_classical_bench = false;
    let mut prune_harmful = false;
    let mut do_calibrate = false;
    let mut ngram_only = false;
    let mut do_gate = false;
    let mut init_gate_baseline = false;
    let mut do_nbest_oracle = false;
    let mut diverse_nbest = false;
    let mut top_k: usize = 16;
    let mut dump_nbest_path: Option<PathBuf> = None;
    let mut dump_train_nbest: Option<(PathBuf, PathBuf)> = None;
    let mut max_train_sents: usize = 300_000;
    let mut neural_rerank_path: Option<PathBuf> = None;
    let mut neural_lambda: f64 = 4.5;
    let mut neural_margin: Option<f64> = None;
    let mut neural_max_trie_nodes: Option<usize> = None;
    let mut confirm_final_blind_test = false;
    let mut commit_override: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--set" if i + 1 < args.len() => {
                set_name = args[i + 1].clone();
                i += 2;
            }
            "--mode" if i + 1 < args.len() => {
                mode_name = args[i + 1].clone();
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
            "--commit" if i + 1 < args.len() => {
                commit_override = Some(args[i + 1].clone());
                i += 2;
            }
            "--ngram-bin" if i + 1 < args.len() => {
                i += 2;
            }
            "--gate" => {
                do_gate = true;
                i += 1;
            }
            "--init-gate-baseline" | "--write-gate-baseline" => {
                init_gate_baseline = true;
                i += 1;
            }
            "--ablation" => {
                do_ablation = true;
                i += 1;
            }
            "--lexicon-ablation" => {
                do_lexicon_ablation = true;
                i += 1;
            }
            "--strategy-bench" => {
                do_strategy_bench = true;
                i += 1;
            }
            "--classical-bench" => {
                do_classical_bench = true;
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
            "--ngram-only" => {
                ngram_only = true;
                i += 1;
            }
            "--nbest-oracle" => {
                do_nbest_oracle = true;
                i += 1;
            }
            "--diverse-nbest" => {
                diverse_nbest = true;
                i += 1;
            }
            "--top-k" if i + 1 < args.len() => {
                top_k = args[i + 1].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("錯誤：無效的 --top-k 數值 `{}`", args[i + 1]);
                    std::process::exit(1);
                }).clamp(1, 128);
                i += 2;
            }
            "--dump-nbest" if i + 1 < args.len() => {
                dump_nbest_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--dump-train-nbest" if i + 2 < args.len() => {
                dump_train_nbest = Some((PathBuf::from(&args[i + 1]), PathBuf::from(&args[i + 2])));
                i += 3;
            }
            "--max-sents" if i + 1 < args.len() => {
                max_train_sents = args[i + 1].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("錯誤：無效的 --max-sents 數值 `{}`", args[i + 1]);
                    std::process::exit(1);
                });
                i += 2;
            }
            "--neural-rerank" if i + 1 < args.len() => {
                neural_rerank_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--neural-lambda" if i + 1 < args.len() => {
                neural_lambda = args[i + 1].parse::<f64>().unwrap_or_else(|_| {
                    eprintln!("錯誤：無效的 --neural-lambda 數值 `{}`", args[i + 1]);
                    std::process::exit(1);
                });
                i += 2;
            }
            "--neural-margin" if i + 1 < args.len() => {
                let v = args[i + 1].parse::<f64>().unwrap_or_else(|_| {
                    eprintln!("錯誤：無效的 --neural-margin 數值 `{}`", args[i + 1]);
                    std::process::exit(1);
                });
                neural_margin = Some(v);
                i += 2;
            }
            "--neural-max-trie-nodes" if i + 1 < args.len() => {
                let v = args[i + 1].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("錯誤：無效的 --neural-max-trie-nodes 數值 `{}`", args[i + 1]);
                    std::process::exit(1);
                });
                neural_max_trie_nodes = Some(v);
                i += 2;
            }
            "--confirm-final-blind-test" => {
                confirm_final_blind_test = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if let Some((ref corpus_txt, ref out_jsonl)) = dump_train_nbest {
        let rep = dump_train_corpus_nbest(&engine, corpus_txt, out_jsonl, max_train_sents);
        println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        return;
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

    if mode_name == "longkan" {
        let rep = evaluate_longkan_mode(&mut engine, &raw_items, &set_name);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        return;
    }
    let compiler = PhoneticCompiler::from_lexicon(&engine.lexicon);
    let compiled: Vec<CompiledSentence> = raw_items
        .iter()
        .filter_map(|item| compiler.compile(item))
        .collect();

    if do_ablation {
        if set_name == "test" || set_name == "typing_tw" {
            eprintln!("錯誤：嚴禁在盲測集 ({}.tsv) 上執行規則消融分析！", set_name);
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

    if do_lexicon_ablation {
        if set_name == "test" || set_name == "typing_tw" {
            eprintln!("錯誤：嚴禁在盲測集 ({}.tsv) 上執行衍生詞庫消融分析！", set_name);
            std::process::exit(1);
        }
        let rep = run_lexicon_ablation_study(&engine, &compiled, &root);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep["stdout_summary"]).unwrap());
        return;
    }

    if do_strategy_bench {
        if set_name == "test" || set_name == "typing_tw" {
            eprintln!("錯誤：嚴禁在盲測集 ({}.tsv) 上執行策略對照統計！", set_name);
            std::process::exit(1);
        }
        let rep = run_strategy_benchmark(&engine, &compiled, &compiler, &root);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep["stdout_summary"]).unwrap());
        return;
    }

    if do_classical_bench {
        if set_name == "test" || set_name == "typing_tw" {
            eprintln!("錯誤：嚴禁在盲測集 ({}.tsv) 上執行路線 C 文言對照評估！", set_name);
            std::process::exit(1);
        }
        let rep = run_classical_benchmark(&engine, &compiled, &compiler, &root);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep["stdout_summary"]).unwrap());
        return;
    }

    if do_calibrate {
        if set_name == "test" || set_name == "typing_tw" {
            eprintln!("錯誤：嚴禁在盲測集 ({}.tsv) 上執行權重校準！", set_name);
            std::process::exit(1);
        }
        let rep = run_perceptron_calibration(&engine, &compiled, &root, ngram_only);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        return;
    }

    if do_nbest_oracle {
        if set_name == "test" || set_name == "typing_tw" {
            eprintln!("錯誤：嚴禁在盲測集 ({}.tsv) 上執行 N-Best Oracle 分析！", set_name);
            std::process::exit(1);
        }
        let rep = run_nbest_oracle_benchmark(&engine, &compiled, &set_name, top_k, diverse_nbest);
        if let Some(ref p) = out_path {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(p, serde_json::to_string_pretty(&rep).unwrap());
        }
        println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        return;
    }

    if let Some(ref dump_path) = dump_nbest_path {
        if set_name != "dev" {
            eprintln!(
                "錯誤：--dump-nbest 僅允許搭配 `--set dev` 使用，嚴禁匯出 `{}.tsv` 原文！",
                set_name
            );
            std::process::exit(1);
        }
        let dump_model = neural_rerank_path.as_ref().map(|model_path| {
            std::sync::Arc::new(ruhi::router::rlnm::RlnmModel::open(model_path).unwrap_or_else(|e| {
                eprintln!("無法載入 RLNM0001 模型 `{}`: {}", model_path.display(), e);
                std::process::exit(1);
            }))
        });
        let rep = dump_nbest_jsonl(&engine, &compiled, top_k, diverse_nbest, dump_model.as_ref(), dump_path);
        println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        return;
    }

    if let Some(ref model_path) = neural_rerank_path {
        if set_name != "dev" && !confirm_final_blind_test {
            eprintln!(
                "錯誤：--neural-rerank 僅允許搭配 `--set dev` 使用，嚴禁對 `{}.tsv` 執行實驗性重排評估！（定案後單次盲測須加 `--confirm-final-blind-test`）",
                set_name
            );
            std::process::exit(1);
        }
        let model = ruhi::router::rlnm::RlnmModel::open(model_path).unwrap_or_else(|e| {
            eprintln!("無法載入 RLNM0001 模型 `{}`: {}", model_path.display(), e);
            std::process::exit(1);
        });
        ruhi::router::neural_reranker::set_in_process_model(
            Some(std::sync::Arc::new(model)),
            neural_lambda,
        );
        ruhi::router::neural_reranker::set_tuning_overrides(
            neural_margin,
            neural_max_trie_nodes,
        );
    }

    // 評估語料不公開：輸出報告預設不含句子原文；僅 dev 集可加 `--with-text` 於本機做錯誤分析（test 與 typing_tw 集一律遮蔽）
    let with_text = args.iter().any(|a| a == "--with-text");
    let hide_text =
        set_name == "test" || set_name == "typing_tw" || !with_text || do_gate || init_gate_baseline;
    let mut report = evaluate_compiled_dataset(&engine, &compiled, &set_name, hide_text);

    if init_gate_baseline {
        let target_bp = baseline_path
            .clone()
            .or_else(|| out_path.clone())
            .unwrap_or_else(|| root.join("eval_reports").join("dev_gate_baseline.json"));
        let commit_hash = commit_override.unwrap_or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .current_dir(&root)
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "dd8ea03".to_string())
        });
        let gate_summary = GateBaselineSummary {
            commit: commit_hash,
            dataset: report.dataset.clone(),
            total_sentences: report.total_sentences,
            total_chars: report.total_chars,
            total_char_errors: report.total_char_errors,
            cer: report.cer,
            cer_percent: report.cer_percent.clone(),
            exact_match_sentences: report.exact_match_sentences,
            sentence_accuracy: report.sentence_accuracy,
            sentence_accuracy_percent: report.sentence_accuracy_percent.clone(),
            per_category: report.per_category.clone(),
        };
        if let Some(parent) = target_bp.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&target_bp, serde_json::to_string_pretty(&gate_summary).unwrap())
            .expect("無法寫入 dev_gate_baseline.json");

        let local_dir = root.join("eval_reports").join("local");
        let _ = fs::create_dir_all(&local_dir);
        let sent_records: Vec<GateSentenceRecord> = report
            .sentence_results
            .iter()
            .map(|r| GateSentenceRecord {
                id: r.id.clone(),
                is_exact_match: r.is_exact_match,
                char_errors: r.char_errors,
            })
            .collect();
        let local_base_path = local_dir.join("dev_gate_baseline_sentences.json");
        fs::write(
            &local_base_path,
            serde_json::to_string_pretty(&sent_records).unwrap(),
        )
        .expect("無法寫入 eval_reports/local/dev_gate_baseline_sentences.json");

        println!("{}", serde_json::to_string_pretty(&gate_summary).unwrap());
        return;
    }

    if do_gate {
        let target_bp = baseline_path
            .clone()
            .unwrap_or_else(|| root.join("eval_reports").join("dev_gate_baseline.json"));
        let Ok(bs_str) = fs::read_to_string(&target_bp) else {
            eprintln!("錯誤：找不到品質閘門基準檔 {:?}", target_bp);
            std::process::exit(1);
        };
        let Ok(base_sum) = serde_json::from_str::<GateBaselineSummary>(&bs_str) else {
            eprintln!("錯誤：無法解析品質閘門基準檔 {:?}", target_bp);
            std::process::exit(1);
        };

        let local_dir = root.join("eval_reports").join("local");
        let _ = fs::create_dir_all(&local_dir);
        let cur_records: Vec<GateSentenceRecord> = report
            .sentence_results
            .iter()
            .map(|r| GateSentenceRecord {
                id: r.id.clone(),
                is_exact_match: r.is_exact_match,
                char_errors: r.char_errors,
            })
            .collect();
        let _ = fs::write(
            local_dir.join("dev_gate_latest_sentences.json"),
            serde_json::to_string_pretty(&cur_records).unwrap(),
        );

        let local_base_path = local_dir.join("dev_gate_baseline_sentences.json");
        let base_records: Vec<GateSentenceRecord> = fs::read_to_string(&local_base_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let base_map: HashMap<&str, &GateSentenceRecord> =
            base_records.iter().map(|r| (r.id.as_str(), r)).collect();

        let mut fixed_ids = Vec::new();
        let mut broken_ids = Vec::new();
        let mut char_improved_ids = Vec::new();
        let mut char_degraded_ids = Vec::new();

        for cur in &cur_records {
            if let Some(old) = base_map.get(cur.id.as_str()) {
                if !old.is_exact_match && cur.is_exact_match {
                    fixed_ids.push(cur.id.clone());
                } else if old.is_exact_match && !cur.is_exact_match {
                    broken_ids.push(cur.id.clone());
                }
                if cur.char_errors < old.char_errors {
                    char_improved_ids.push(cur.id.clone());
                } else if cur.char_errors > old.char_errors {
                    char_degraded_ids.push(cur.id.clone());
                }
            }
        }
        fixed_ids.sort();
        broken_ids.sort();
        char_improved_ids.sort();
        char_degraded_ids.sort();

        let b_fixed = fixed_ids.len();
        let c_broken = broken_ids.len();
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

        let delta_cer_pp = (report.cer - base_sum.cer) * 100.0;
        let delta_ser_pp = (report.sentence_accuracy - base_sum.sentence_accuracy) * 100.0;

        let mut failure_reasons = Vec::new();
        if delta_cer_pp > 0.05 + 1e-9 {
            failure_reasons.push(format!(
                "整體 CER 上升 {:.4} 個百分點（超過上限 +0.05 個百分點）",
                delta_cer_pp
            ));
        }
        if delta_ser_pp < -0.30 - 1e-9 {
            failure_reasons.push(format!(
                "整句正確率下降 {:.4} 個百分點（超過上限 -0.30 個百分點）",
                -delta_ser_pp
            ));
        }

        let mut per_cat_diff = serde_json::Map::new();
        for (cat, cur_m) in &report.per_category {
            if let Some(base_m) = base_sum.per_category.get(cat) {
                let cat_delta_cer_pp = (cur_m.cer - base_m.cer) * 100.0;
                let exceeded = cat_delta_cer_pp > 0.30 + 1e-9;
                if exceeded {
                    failure_reasons.push(format!(
                        "類別 `{}` CER 上升 {:.4} 個百分點（超過上限 +0.30 個百分點）",
                        cat, cat_delta_cer_pp
                    ));
                }
                per_cat_diff.insert(
                    cat.clone(),
                    json!({
                        "baseline_cer": base_m.cer,
                        "baseline_cer_percent": base_m.cer_percent,
                        "current_cer": cur_m.cer,
                        "current_cer_percent": cur_m.cer_percent,
                        "delta_cer_percentage_points": format!("{:+.4}%", cat_delta_cer_pp),
                        "baseline_char_errors": base_m.char_errors,
                        "current_char_errors": cur_m.char_errors,
                        "delta_char_errors": (cur_m.char_errors as isize) - (base_m.char_errors as isize),
                        "exceeded_threshold": exceeded
                    }),
                );
            }
        }

        if c_broken > b_fixed {
            failure_reasons.push(format!(
                "弄壞的句數 ({}) 多於修好的句數 ({})",
                c_broken, b_fixed
            ));
        }

        let passed = failure_reasons.is_empty();
        let gate_out = json!({
            "gate_passed": passed,
            "verdict": if passed { "PASS" } else { "FAIL" },
            "failure_reasons": failure_reasons,
            "baseline": {
                "commit": base_sum.commit,
                "dataset": base_sum.dataset,
                "total_sentences": base_sum.total_sentences,
                "total_chars": base_sum.total_chars,
                "total_char_errors": base_sum.total_char_errors,
                "cer": base_sum.cer,
                "cer_percent": base_sum.cer_percent,
                "exact_match_sentences": base_sum.exact_match_sentences,
                "sentence_accuracy": base_sum.sentence_accuracy,
                "sentence_accuracy_percent": base_sum.sentence_accuracy_percent
            },
            "current": {
                "dataset": report.dataset,
                "total_sentences": report.total_sentences,
                "total_chars": report.total_chars,
                "total_char_errors": report.total_char_errors,
                "cer": report.cer,
                "cer_percent": report.cer_percent,
                "exact_match_sentences": report.exact_match_sentences,
                "sentence_accuracy": report.sentence_accuracy,
                "sentence_accuracy_percent": report.sentence_accuracy_percent
            },
            "diff": {
                "delta_cer": report.cer - base_sum.cer,
                "delta_cer_percentage_points": format!("{:+.4}%", delta_cer_pp),
                "delta_char_errors": (report.total_char_errors as isize) - (base_sum.total_char_errors as isize),
                "delta_sentence_accuracy": report.sentence_accuracy - base_sum.sentence_accuracy,
                "delta_sentence_accuracy_percentage_points": format!("{:+.4}%", delta_ser_pp),
                "delta_exact_match_sentences": (report.exact_match_sentences as isize) - (base_sum.exact_match_sentences as isize)
            },
            "per_category_diff": per_cat_diff,
            "sentence_changes": {
                "fixed_count": b_fixed,
                "broken_count": c_broken,
                "fixed_sentence_ids": fixed_ids,
                "broken_sentence_ids": broken_ids,
                "char_error_improved_count": char_improved_ids.len(),
                "char_error_degraded_count": char_degraded_ids.len(),
                "char_error_improved_ids": char_improved_ids,
                "char_error_degraded_ids": char_degraded_ids,
                "mcnemar_chi_square": round4_f64(chi_square),
                "mcnemar_p_value": p_value
            }
        });

        println!("{}", serde_json::to_string_pretty(&gate_out).unwrap());
        if !passed {
            std::process::exit(1);
        }
        return;
    }

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
        if set_name == "typing_tw" {
            let blind_summary = json!({
                "dataset": report.dataset,
                "total_sentences": report.total_sentences,
                "total_chars": report.total_chars,
                "total_char_errors": report.total_char_errors,
                "cer": report.cer,
                "cer_percent": report.cer_percent,
                "exact_match_sentences": report.exact_match_sentences,
                "sentence_accuracy": report.sentence_accuracy,
                "sentence_accuracy_percent": report.sentence_accuracy_percent,
                "per_category": report.per_category,
                "top_20_homophone_char_errors": report.top_homophone_char_errors
            });
            let _ = fs::write(p, serde_json::to_string_pretty(&blind_summary).unwrap());
        } else {
            let _ = fs::write(p, serde_json::to_string_pretty(&report).unwrap());
        }
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
        "top_20_homophone_char_errors": report.top_homophone_char_errors,
        "mcnemar_comparison": report.mcnemar_comparison
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}
