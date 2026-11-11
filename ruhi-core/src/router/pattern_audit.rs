//! # 構式陣列展開同音對衝四維分流審計模組 (`pattern_audit`)
//!
//! 本模組實作多維構式陣列展開時之同音對衝四維分流審計機制。
//! 依據漢語詞彙與語法拓撲結構，當笛卡兒積陣列展開產生雙字或多字組合時，
//! 依循以下四維分流準則進行自動消歧與風險抑制：
//! 1. 天然無害率審計：無同音現代實詞對應之組合，使用者絕不會鍵入，天然具備 0% 干擾性；
//! 2. Type A（古語保留現代陣列）：對衝詞為罕見古語（`weight <= 220`，如「說部」、「坐簿」）時，優先保留現代高頻陣列；
//! 3. Type B（句法雙向鞍點分岔）：兩側皆為真實語句合法用法時，透過 `bifurcation_pairs` 與語境槽位進行雙向分流；
//! 4. Type C（跨界假組合對衝抑制）：陣列展開為跨界假組合且對衝現代高頻實詞時，透過 `exclude_pairs` 抵銷孤立二元分數。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::{json, Value};

/// 驗證單一字元是否落於 CJK 統一表意文字區間
#[inline]
fn is_cjk_char(ch: char) -> bool {
    let code = ch as u32;
    (0x4E00..=0x9FFF).contains(&code)
        || (0x3400..=0x4DBF).contains(&code)
        || (0xF900..=0xFAFF).contains(&code)
}

/// 驗證是否為合法純漢字詞元（排除英數、標點、空白與樣板佔位詞）
pub fn is_valid_chinese_token(text: &str) -> bool {
    let s = text.trim();
    if s.is_empty() {
        return false;
    }
    let placeholders = [
        "前驅純漢字詞元",
        "後繼純漢字詞元",
        "前驅純漢字詞",
        "後繼純漢字詞",
        "真正未登錄之雙字原子複合詞",
        "專有名詞",
        "目標詞",
        "前驅候選",
        "錯誤句",
        "正確句",
        "構式名稱",
        "二階語法搭配說明",
        "二階語法或語意搭配說明",
    ];
    if placeholders.contains(&s) {
        return false;
    }
    s.chars().all(is_cjk_char)
}

/// 定位專案根目錄
pub fn resolve_ruhi_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(parent) = manifest.parent() {
        if parent.join("ruhi_lexicon").is_dir() {
            return parent.to_path_buf();
        }
    }
    PathBuf::from(r"C:\ATGprjs\ruhi")
}

/// 從指定資料目錄掃描多維構式同音對衝四維分流審計
pub fn scan_array_homophone_collisions_from_data_dir(data_dir: &Path) -> Value {
    let mut rejected_set: HashSet<(String, String)> = HashSet::new();
    let rej_path = data_dir.join("rejected_readings.tsv");
    if let Ok(rej_raw) = fs::read_to_string(&rej_path) {
        for line in rej_raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = t.split('\t').collect();
            if cols.len() >= 2 {
                rejected_set.insert((cols[0].trim().to_string(), cols[1].trim().to_string()));
            }
        }
    }

    let lexicon_files = [
        "lexicon_core.tsv",
        "lexicon_domain.tsv",
        "lexicon_mcbopomofo.tsv",
        "lexicon_moe.tsv",
        "lexicon_main.tsv",
        "lexicon_colloquial.tsv",
        "lexicon_user.tsv",
    ];

    let mut char_bopomofo: HashMap<char, String> = HashMap::new();
    let mut word_bopomofo: HashMap<String, String> = HashMap::new();
    let mut word_max_weight: HashMap<String, i32> = HashMap::new();
    let mut known_words: HashSet<String> = HashSet::new();

    for fname in &lexicon_files {
        let path = data_dir.join(fname);
        if !path.is_file() {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split('\t').collect();
                if parts.len() >= 2 {
                    let word = parts[0].trim();
                    let bpmf = parts[1].trim();
                    let weight = parts
                        .get(2)
                        .and_then(|s| s.trim().parse::<i32>().ok())
                        .unwrap_or(180);
                    let cat = parts.get(3).map(|s| s.trim()).unwrap_or("");
                    if word.is_empty() || word == "word" {
                        continue;
                    }
                    if rejected_set.contains(&(word.to_string(), bpmf.to_string())) {
                        continue;
                    }
                    let ch_count = word.chars().count();
                    if ch_count == 1 {
                        if let Some(ch) = word.chars().next() {
                            char_bopomofo.entry(ch).or_insert_with(|| bpmf.to_string());
                        }
                    } else if ch_count >= 2 {
                        let is_archaic_moe = weight < 90
                            || matches!(cat, "archaic" | "classic" | "rare" | "moe_unattested2");
                        if !is_archaic_moe {
                            known_words.insert(word.to_string());
                        }
                        word_bopomofo
                            .entry(word.to_string())
                            .or_insert_with(|| bpmf.to_string());
                        word_max_weight
                            .entry(word.to_string())
                            .and_modify(|w| *w = (*w).max(weight))
                            .or_insert(weight);

                        let syls: Vec<&str> = if bpmf.contains('-') {
                            bpmf.split('-').collect()
                        } else {
                            bpmf.split_whitespace().collect()
                        };
                        if syls.len() == ch_count {
                            for (c, s) in word.chars().zip(syls.iter()) {
                                char_bopomofo.entry(c).or_insert_with(|| (*s).to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let get_bopomofo = |w: &str| -> String {
        let wt = w.trim();
        if let Some(bpmf) = word_bopomofo.get(wt) {
            return bpmf.replace('-', " ");
        }
        let mut syls = Vec::new();
        for ch in wt.chars() {
            if let Some(s) = char_bopomofo.get(&ch) {
                syls.push(s.clone());
            }
        }
        syls.join(" ")
    };

    let mut reading_to_real_words: HashMap<String, Vec<String>> = HashMap::new();
    for (w, bpmf) in &word_bopomofo {
        if w.chars().count() >= 2 && !bpmf.trim().is_empty() {
            reading_to_real_words
                .entry(bpmf.trim().to_string())
                .or_default()
                .push(w.clone());
        }
    }
    for list in reading_to_real_words.values_mut() {
        list.sort();
        list.dedup();
    }

    let mut total_expanded_pairs = 0usize;
    let mut valid_lexicon_compounds = 0usize;
    let mut harmless_non_existent_readings = 0usize;
    let mut total_homophone_collisions = 0usize;
    let mut type_a_archaic_keep_count = 0usize;
    let mut type_b_bifurcation_count = 0usize;
    let mut suppressed_by_exclude_array = 0usize;
    let mut type_a_items = Vec::new();
    let mut type_b_items = Vec::new();
    let mut suppressed_items = Vec::new();
    let mut unsuppressed_items = Vec::new();

    let patterns_path = data_dir.join("patterns_user.jsonl");
    if patterns_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&patterns_path) {
            for line in raw.lines() {
                let s = line.trim();
                if s.is_empty() || s.starts_with('#') {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(s) else {
                    continue;
                };
                let pid = v
                    .get("pattern_id")
                    .and_then(Value::as_str)
                    .filter(|x| !x.is_empty())
                    .or_else(|| v.get("name").and_then(Value::as_str))
                    .or_else(|| v.get("description").and_then(Value::as_str))
                    .unwrap_or("pattern")
                    .to_string();

                let mut excluded: HashSet<(String, String)> = HashSet::new();
                if let Some(ex_arr) = v.get("exclude_pairs").and_then(Value::as_array) {
                    for pair in ex_arr {
                        if let Some(pa) = pair.as_array() {
                            if pa.len() == 2 {
                                let p0 = pa[0].as_str().unwrap_or("").trim().to_string();
                                let p1 = pa[1].as_str().unwrap_or("").trim().to_string();
                                if !p0.is_empty() && !p1.is_empty() {
                                    excluded.insert((p0, p1));
                                }
                            }
                        }
                    }
                }

                let mut bifurcated: HashSet<(String, String)> = HashSet::new();
                if let Some(bi_arr) = v.get("bifurcation_pairs").and_then(Value::as_array) {
                    for item in bi_arr {
                        if let Some(pa) = item.as_array() {
                            if pa.len() == 2 {
                                let p0 = pa[0].as_str().unwrap_or("").trim().to_string();
                                let p1 = pa[1].as_str().unwrap_or("").trim().to_string();
                                if !p0.is_empty() && !p1.is_empty() {
                                    bifurcated.insert((p0, p1));
                                }
                            }
                        } else if let Some(pa) = item.get("pair").and_then(Value::as_array) {
                            if pa.len() == 2 {
                                let p0 = pa[0].as_str().unwrap_or("").trim().to_string();
                                let p1 = pa[1].as_str().unwrap_or("").trim().to_string();
                                if !p0.is_empty() && !p1.is_empty() {
                                    bifurcated.insert((p0, p1));
                                }
                            }
                        }
                    }
                }

                let has_target_or_pref = v
                    .get("target")
                    .and_then(Value::as_array)
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
                    || v.get("preferred")
                        .and_then(Value::as_array)
                        .map(|a| !a.is_empty())
                        .unwrap_or(false);

                let mut slot_pairs: Vec<(Vec<String>, Vec<String>)> = Vec::new();
                if let Some(slots) = v.get("slots").and_then(Value::as_array) {
                    if !has_target_or_pref || slots.len() == 2 {
                        let cleaned: Vec<Vec<String>> = slots
                            .iter()
                            .filter_map(Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_str)
                                    .map(|x| x.trim().to_string())
                                    .filter(|x| is_valid_chinese_token(x))
                                    .collect()
                            })
                            .collect();
                        for i in 0..cleaned.len().saturating_sub(1) {
                            slot_pairs.push((cleaned[i].clone(), cleaned[i + 1].clone()));
                        }
                    }
                }
                if slot_pairs.is_empty() {
                    let ps: Vec<String> = v
                        .get("prev_slot")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .map(|x| x.trim().to_string())
                                .filter(|x| is_valid_chinese_token(x))
                                .collect()
                        })
                        .unwrap_or_default();
                    let ns: Vec<String> = v
                        .get("next_slot")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .map(|x| x.trim().to_string())
                                .filter(|x| is_valid_chinese_token(x))
                                .collect()
                        })
                        .unwrap_or_default();
                    if !ps.is_empty() && !ns.is_empty() {
                        slot_pairs.push((ps, ns));
                    }
                }

                for (left_slot, right_slot) in slot_pairs {
                    for a in &left_slot {
                        for b in &right_slot {
                            total_expanded_pairs += 1;
                            let combined = format!("{a}{b}");
                            if known_words.contains(&combined) {
                                valid_lexicon_compounds += 1;
                                continue;
                            }
                            let ra = get_bopomofo(a);
                            let rb = get_bopomofo(b);
                            if ra.is_empty() || rb.is_empty() {
                                harmless_non_existent_readings += 1;
                                continue;
                            }
                            let reading_ab = format!("{} {}", ra.trim(), rb.trim());
                            if let Some(real_words) = reading_to_real_words.get(&reading_ab) {
                                let competing: Vec<String> = real_words
                                    .iter()
                                    .filter(|w| **w != combined)
                                    .cloned()
                                    .collect();
                                if competing.is_empty() {
                                    harmless_non_existent_readings += 1;
                                } else {
                                    total_homophone_collisions += 1;
                                    let max_comp_weight = competing
                                        .iter()
                                        .filter_map(|w| word_max_weight.get(w).copied())
                                        .max()
                                        .unwrap_or(180);
                                    let is_bifurcated =
                                        bifurcated.contains(&(a.clone(), b.clone()));
                                    let is_suppressed =
                                        excluded.contains(&(a.clone(), b.clone()));
                                    let is_archaic_low_freq = max_comp_weight <= 220;

                                    let status = if is_bifurcated {
                                        "RESOLVED_BY_SYNTACTIC_BIFURCATION"
                                    } else if is_suppressed {
                                        "SUPPRESSED_BY_EXCLUDE_ARRAY"
                                    } else if is_archaic_low_freq {
                                        "KEEP_MODERN_ARRAY_OVER_ARCHAIC"
                                    } else {
                                        "NEEDS_RESOLUTION"
                                    };
                                    let item = json!({
                                        "pattern_id": pid,
                                        "pair": [a, b],
                                        "reading": reading_ab,
                                        "competing_real_words": competing,
                                        "max_competitor_weight": max_comp_weight,
                                        "status": status
                                    });
                                    if is_bifurcated {
                                        type_b_bifurcation_count += 1;
                                        type_b_items.push(item);
                                    } else if is_suppressed {
                                        suppressed_by_exclude_array += 1;
                                        suppressed_items.push(item);
                                    } else if is_archaic_low_freq {
                                        type_a_archaic_keep_count += 1;
                                        type_a_items.push(item);
                                    } else {
                                        unsuppressed_items.push(item);
                                    }
                                }
                            } else {
                                harmless_non_existent_readings += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let harmless_total = valid_lexicon_compounds + harmless_non_existent_readings;
    let harmless_rate = if total_expanded_pairs > 0 {
        (harmless_total as f64 / total_expanded_pairs as f64) * 100.0
    } else {
        100.0
    };

    json!({
        "total_expanded_pairs": total_expanded_pairs,
        "valid_lexicon_compounds_count": valid_lexicon_compounds,
        "harmless_non_existent_readings_count": harmless_non_existent_readings,
        "naturally_harmless_rate": format!("{harmless_rate:.2}%"),
        "total_homophone_collisions": total_homophone_collisions,
        "type_a_archaic_keep_array_count": type_a_archaic_keep_count,
        "type_b_syntactic_bifurcation_count": type_b_bifurcation_count,
        "type_c_suppressed_by_exclude_array_count": suppressed_by_exclude_array,
        "suppressed_by_exclude_array_count": suppressed_by_exclude_array,
        "unsuppressed_active_collisions_count": unsuppressed_items.len(),
        "type_a_archaic_items": type_a_items,
        "type_b_bifurcation_items": type_b_items,
        "suppressed_collisions": suppressed_items,
        "unsuppressed_collisions": unsuppressed_items,
        "theoretical_principle": "陣列展開同音對衝四維分流定理：(1) 無同音實詞對應之展開組合（>98%）使用者絕不會打出，天然 0% 干擾；(2) 對衝詞為罕見古語（weight<=220，如 說部/坐簿/得間/芳蘭）時，保留現代高頻陣列（Type A）；(3) 雙方皆為真實語句合法用法（如『他馬騎得很好吧』vs.『才不是什麼耆德老人呢』、『讀不懂』vs.『獨步全球』、『笑到飽』vs.『恪守孝道』）時，透過 bifurcation_pairs 與左右句法槽位進行語法環境雙向分流（Type B）；(4) 陣列展開本身為跨界假組合且對衝高頻現代實詞（如 這鍵/這件、個相/各項、分不/分布）時，方透過 exclude_pairs 對衝抑制陣列抵銷孤立二元分數（Type C）。"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_collision_audit_and_auto_handover_delivery_261019() {
        let root = resolve_ruhi_root();
        let data_dir = if root.join("ruhi_lexicon").join("data").is_dir() {
            root.join("ruhi_lexicon").join("data")
        } else {
            root.join("data")
        };
        let audit = scan_array_homophone_collisions_from_data_dir(&data_dir);
        let total_pairs = audit["total_expanded_pairs"].as_u64().unwrap_or(0);
        let type_a = audit["type_a_archaic_keep_array_count"].as_u64().unwrap_or(0);
        let type_b = audit["type_b_syntactic_bifurcation_count"].as_u64().unwrap_or(0);
        let suppressed = audit["suppressed_by_exclude_array_count"].as_u64().unwrap_or(0);
        let unsuppressed = audit["unsuppressed_active_collisions_count"].as_u64().unwrap_or(99);

        assert!(total_pairs > 1500, "Expected >1500 expanded array pairs, got {}", total_pairs);
        assert!(type_a >= 5, "Expected >=5 Type A archaic keep-array pairs, got {}", type_a);
        assert!(type_b >= 6, "Expected >=6 Type B syntactic bifurcation pairs, got {}", type_b);
        assert!(suppressed >= 9, "Expected >=9 Type C suppressed colliding pairs via exclude_pairs, got {}", suppressed);
        assert_eq!(unsuppressed, 0, "Expected 0 unresolved active homophone collisions, got {:?}", audit["unsuppressed_collisions"]);
    }

    #[test]
    fn test_array_collision_audit() {
        test_array_collision_audit_and_auto_handover_delivery_261019();
    }
}
