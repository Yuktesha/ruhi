// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/cache_lm.rs — 局部上下文線上學習快取語言模型 (`Contextual Cache LM`)
//
// 僅記錄具左右語境錨定之三元 (`left_ch, word, right_ch`) 與二元 (`left_ch, word` / `word, right_ch`)
// 個人選擇強化與誤選短期負向懲罰，絕不改動全域無上下文單字先驗權重；採用唯讀快照共享與時間衰減機制，
// 且以外部 `OnceLock` 旁表索引 `TransitionEngine` 位址，嚴格維持 `RuhiEngine` / `RuhiTextService` 二進位佈局不變。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::lattice::LatticeNode;

/// 正向個人選擇強化半衰期：30 天
pub const HALF_LIFE_POS_SECS: f64 = 30.0 * 86_400.0;
/// 被拒錯誤路徑短期負向懲罰半衰期：7 天
pub const HALF_LIFE_NEG_SECS: f64 = 7.0 * 86_400.0;

const TRIGRAM_POS_DELTA: f64 = 10.0;
const TRIGRAM_POS_CAP: f64 = 16.0;
const TRIGRAM_NEG_DELTA: f64 = -8.0;
const TRIGRAM_NEG_CAP: f64 = -14.0;

const SPAN_TRIGRAM_POS_DELTA: f64 = 6.0;
const SPAN_BIGRAM_POS_DELTA: f64 = 2.5;

const BIGRAM_POS_DELTA: f64 = 4.0;
const BIGRAM_POS_CAP: f64 = 8.0;
const BIGRAM_NEG_DELTA: f64 = -3.0;
const BIGRAM_NEG_CAP: f64 = -6.0;

const EDGE_TOTAL_CLAMP: f64 = 18.0;
const MAX_ENTRIES_PER_TABLE: usize = 2048;
const PRUNE_MIN_ABS_STRENGTH: f64 = 0.25;
const RELOAD_THROTTLE_SECS: u64 = 2;
const COMPETITIVE_RIGHT_MIN_WEIGHT: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CacheEntry {
    pub strength: f32,
    pub last_ts: u64,
    pub hits: u32,
}

#[derive(Debug, Clone, Default)]
struct CacheLmTables {
    trigrams: HashMap<(String, String, String), CacheEntry>,
    left_bigrams: HashMap<(String, String), CacheEntry>,
    right_bigrams: HashMap<(String, String), CacheEntry>,
    known_words: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CacheLmSnapshot {
    tables: Arc<CacheLmTables>,
    now_secs: u64,
}

#[derive(Debug, Default)]
pub struct ContextualCacheLm {
    tables: CacheLmTables,
    snapshot: Arc<CacheLmTables>,
    file_path: Option<PathBuf>,
    last_mtime: Option<SystemTime>,
    last_check: Option<Instant>,
    dirty: bool,
}

static CACHE_LM_STORE: OnceLock<RwLock<HashMap<usize, ContextualCacheLm>>> = OnceLock::new();

#[inline]
fn store() -> &'static RwLock<HashMap<usize, ContextualCacheLm>> {
    CACHE_LM_STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[inline]
pub fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[inline]
fn is_cjk_char(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

#[inline]
pub fn last_cjk_char(s: &str) -> String {
    s.chars()
        .rev()
        .find(|&c| is_cjk_char(c))
        .map(|c| c.to_string())
        .unwrap_or_default()
}

#[inline]
pub fn first_cjk_char(s: &str) -> String {
    s.chars()
        .find(|&c| is_cjk_char(c))
        .map(|c| c.to_string())
        .unwrap_or_default()
}

#[inline]
pub fn is_valid_cjk_word(s: &str) -> bool {
    let mut cnt = 0usize;
    for c in s.chars() {
        if !is_cjk_char(c) {
            return false;
        }
        cnt += 1;
        if cnt > 4 {
            return false;
        }
    }
    cnt >= 1
}

/// 計算帶時間指數衰減之目前強化／懲罰分值
#[inline]
pub fn decayed_strength(entry: &CacheEntry, now_secs: u64) -> f64 {
    let dt = now_secs.saturating_sub(entry.last_ts) as f64;
    let half_life = if entry.strength >= 0.0 {
        HALF_LIFE_POS_SECS
    } else {
        HALF_LIFE_NEG_SECS
    };
    (entry.strength as f64) * (0.5_f64).powf(dt / half_life)
}

fn apply_delta<K>(
    map: &mut HashMap<K, CacheEntry>,
    key: K,
    delta: f64,
    pos_cap: f64,
    neg_cap: f64,
    now_secs: u64,
) where
    K: Eq + std::hash::Hash,
{
    let entry = map.entry(key).or_insert(CacheEntry {
        strength: 0.0,
        last_ts: now_secs,
        hits: 0,
    });
    let cur = decayed_strength(entry, now_secs);
    let updated = (cur + delta).clamp(neg_cap, pos_cap) as f32;
    entry.strength = updated;
    entry.last_ts = now_secs;
    entry.hits = entry.hits.saturating_add(1);
}

fn prune_table<K, F>(map: &mut HashMap<K, CacheEntry>, now_secs: u64, key_of: F)
where
    K: Eq + std::hash::Hash + Clone,
    F: Fn(&K) -> String,
{
    let _ = &key_of;
    if map.len() <= MAX_ENTRIES_PER_TABLE {
        return;
    }
    map.retain(|_, v| decayed_strength(v, now_secs).abs() >= PRUNE_MIN_ABS_STRENGTH);
    if map.len() <= MAX_ENTRIES_PER_TABLE {
        return;
    }
    let mut ranked: Vec<(K, f64)> = map
        .iter()
        .map(|(k, v)| (k.clone(), decayed_strength(v, now_secs).abs()))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(MAX_ENTRIES_PER_TABLE);
    let keep: HashMap<K, CacheEntry> = ranked
        .into_iter()
        .filter_map(|(k, _)| map.get(&k).copied().map(|v| (k, v)))
        .collect();
    *map = keep;
}

impl ContextualCacheLm {
    fn rebuild_known_words_and_snapshot(&mut self, now_secs: u64) {
        prune_table(&mut self.tables.trigrams, now_secs, |k| k.1.clone());
        prune_table(&mut self.tables.left_bigrams, now_secs, |k| k.1.clone());
        prune_table(&mut self.tables.right_bigrams, now_secs, |k| k.0.clone());

        self.tables.known_words.clear();
        for (_, w, _) in self.tables.trigrams.keys() {
            self.tables.known_words.insert(w.clone());
        }
        for (_, w) in self.tables.left_bigrams.keys() {
            self.tables.known_words.insert(w.clone());
        }
        for (w, _) in self.tables.right_bigrams.keys() {
            self.tables.known_words.insert(w.clone());
        }
        self.snapshot = Arc::new(self.tables.clone());
    }

    fn record_pair_with_deltas(
        &mut self,
        left_ch: &str,
        word: &str,
        right_ch: &str,
        tri_delta: f64,
        bi_delta: f64,
        now_secs: u64,
    ) {
        if !is_valid_cjk_word(word) || (left_ch.is_empty() && right_ch.is_empty()) {
            return;
        }
        apply_delta(
            &mut self.tables.trigrams,
            (left_ch.to_string(), word.to_string(), right_ch.to_string()),
            tri_delta,
            TRIGRAM_POS_CAP,
            TRIGRAM_NEG_CAP,
            now_secs,
        );
        if !left_ch.is_empty() {
            apply_delta(
                &mut self.tables.left_bigrams,
                (left_ch.to_string(), word.to_string()),
                bi_delta,
                BIGRAM_POS_CAP,
                BIGRAM_NEG_CAP,
                now_secs,
            );
        }
        if !right_ch.is_empty() {
            apply_delta(
                &mut self.tables.right_bigrams,
                (word.to_string(), right_ch.to_string()),
                bi_delta,
                BIGRAM_POS_CAP,
                BIGRAM_NEG_CAP,
                now_secs,
            );
        }
    }

    fn record_correction(
        &mut self,
        prev_ctx: &str,
        rejected: &str,
        accepted: &str,
        next_ctx: &str,
        now_secs: u64,
    ) {
        if rejected == accepted || !is_valid_cjk_word(rejected) || !is_valid_cjk_word(accepted) {
            return;
        }
        let left_ch = last_cjk_char(prev_ctx);
        let right_ch = first_cjk_char(next_ctx);
        if left_ch.is_empty() && right_ch.is_empty() {
            return;
        }
        self.record_pair_with_deltas(
            &left_ch,
            accepted,
            &right_ch,
            TRIGRAM_POS_DELTA,
            BIGRAM_POS_DELTA,
            now_secs,
        );
        self.record_pair_with_deltas(
            &left_ch,
            rejected,
            &right_ch,
            TRIGRAM_NEG_DELTA,
            BIGRAM_NEG_DELTA,
            now_secs,
        );
        self.dirty = true;
        self.rebuild_known_words_and_snapshot(now_secs);
    }

    fn record_sentence_span_diffs(
        &mut self,
        raw_sentence: &str,
        cor_sentence: &str,
        now_secs: u64,
    ) {
        let raw_chars: Vec<char> = raw_sentence.chars().collect();
        let cor_chars: Vec<char> = cor_sentence.chars().collect();
        if raw_chars.len() != cor_chars.len() || raw_chars == cor_chars {
            return;
        }

        let n = raw_chars.len();
        let mut i = 0usize;
        let mut mutated = false;
        while i < n {
            if raw_chars[i] == cor_chars[i] {
                i += 1;
                continue;
            }
            let s = i;
            while i < n && raw_chars[i] != cor_chars[i] {
                i += 1;
            }
            let e = i;
            let span_len = e - s;
            // 單字差異已由 select_candidate 透過 record_correction 處理，此處專責多字邊界切換 (len 2..=4)
            if !(2..=4).contains(&span_len) {
                continue;
            }
            let raw_span: String = raw_chars[s..e].iter().collect();
            let cor_span: String = cor_chars[s..e].iter().collect();
            if !is_valid_cjk_word(&raw_span) || !is_valid_cjk_word(&cor_span) {
                continue;
            }
            let left_ch = if s > 0 && is_cjk_char(cor_chars[s - 1]) {
                cor_chars[s - 1].to_string()
            } else {
                String::new()
            };
            let right_ch = if e < n && is_cjk_char(cor_chars[e]) {
                cor_chars[e].to_string()
            } else {
                String::new()
            };
            if left_ch.is_empty() && right_ch.is_empty() {
                continue;
            }

            self.record_pair_with_deltas(
                &left_ch,
                &cor_span,
                &right_ch,
                TRIGRAM_POS_DELTA,
                BIGRAM_POS_DELTA,
                now_secs,
            );
            self.record_pair_with_deltas(
                &left_ch,
                &raw_span,
                &right_ch,
                TRIGRAM_NEG_DELTA,
                BIGRAM_NEG_DELTA,
                now_secs,
            );

            // 若正確區段採單字切分（如「舊址」->「就 + 只」），補齊內部字元步驟轉移支持
            for k in s..e {
                let w = cor_chars[k].to_string();
                let lk = if k > 0 && is_cjk_char(cor_chars[k - 1]) {
                    cor_chars[k - 1].to_string()
                } else {
                    String::new()
                };
                let rk = if k + 1 < n && is_cjk_char(cor_chars[k + 1]) {
                    cor_chars[k + 1].to_string()
                } else {
                    String::new()
                };
                self.record_pair_with_deltas(
                    &lk,
                    &w,
                    &rk,
                    SPAN_TRIGRAM_POS_DELTA,
                    SPAN_BIGRAM_POS_DELTA,
                    now_secs,
                );
            }
            mutated = true;
        }

        if mutated {
            self.dirty = true;
            self.rebuild_known_words_and_snapshot(now_secs);
        }
    }

    fn load_from_path(&mut self, path: &Path) {
        self.file_path = Some(path.to_path_buf());
        self.last_check = Some(Instant::now());
        let content = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                self.last_mtime = None;
                return;
            }
        };
        let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
        self.tables = CacheLmTables::default();
        for line in content.lines() {
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() != 7 {
                continue;
            }
            let kind = parts[0];
            let left = parts[1];
            let word = parts[2];
            let right = parts[3];
            let Ok(strength) = parts[4].parse::<f32>() else {
                continue;
            };
            let Ok(last_ts) = parts[5].parse::<u64>() else {
                continue;
            };
            let Ok(hits) = parts[6].parse::<u32>() else {
                continue;
            };
            if !strength.is_finite() || !is_valid_cjk_word(word) {
                continue;
            }
            if (!left.is_empty() && !is_valid_cjk_word(left))
                || (!right.is_empty() && !is_valid_cjk_word(right))
            {
                continue;
            }
            let entry = CacheEntry {
                strength,
                last_ts,
                hits,
            };
            match kind {
                "T" if !left.is_empty() || !right.is_empty() => {
                    self.tables.trigrams.insert(
                        (left.to_string(), word.to_string(), right.to_string()),
                        entry,
                    );
                }
                "L" if !left.is_empty() => {
                    self.tables
                        .left_bigrams
                        .insert((left.to_string(), word.to_string()), entry);
                }
                "R" if !right.is_empty() => {
                    self.tables
                        .right_bigrams
                        .insert((word.to_string(), right.to_string()), entry);
                }
                _ => {}
            }
        }
        self.last_mtime = mtime;
        self.dirty = false;
        self.rebuild_known_words_and_snapshot(current_unix_secs());
    }

    fn flush_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(ref path) = self.file_path else {
            return;
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && fs::create_dir_all(parent).is_err() {
                return;
            }
        }

        let mut buf = String::with_capacity(2048);
        buf.push_str("# ruhi user_cache_lm v1\n");
        buf.push_str("# kind\tleft\tword\tright\tstrength\tlast_ts\thits\n");

        let mut tri_rows: Vec<_> = self.tables.trigrams.iter().collect();
        tri_rows.sort_by(|a, b| a.0.cmp(b.0));
        for ((l, w, r), e) in tri_rows {
            buf.push_str(&format!(
                "T\t{}\t{}\t{}\t{:.4}\t{}\t{}\n",
                l, w, r, e.strength, e.last_ts, e.hits
            ));
        }

        let mut lb_rows: Vec<_> = self.tables.left_bigrams.iter().collect();
        lb_rows.sort_by(|a, b| a.0.cmp(b.0));
        for ((l, w), e) in lb_rows {
            buf.push_str(&format!(
                "L\t{}\t{}\t\t{:.4}\t{}\t{}\n",
                l, w, e.strength, e.last_ts, e.hits
            ));
        }

        let mut rb_rows: Vec<_> = self.tables.right_bigrams.iter().collect();
        rb_rows.sort_by(|a, b| a.0.cmp(b.0));
        for ((w, r), e) in rb_rows {
            buf.push_str(&format!(
                "R\t\t{}\t{}\t{:.4}\t{}\t{}\n",
                w, r, e.strength, e.last_ts, e.hits
            ));
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("user_cache_lm.tsv");
        let tmp_path = path.with_file_name(format!("{}.tmp.{}", file_name, std::process::id()));
        if fs::write(&tmp_path, buf.as_bytes()).is_err() {
            let _ = fs::remove_file(&tmp_path);
            return;
        }
        if fs::rename(&tmp_path, path).is_err() {
            if fs::copy(&tmp_path, path).is_err() {
                let _ = fs::remove_file(&tmp_path);
                return;
            }
            let _ = fs::remove_file(&tmp_path);
        }
        self.last_mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
        self.last_check = Some(Instant::now());
        self.dirty = false;
    }

    fn needs_reload_check(&self) -> bool {
        !self.dirty
            && self.file_path.is_some()
            && self
                .last_check
                .map_or(true, |t| t.elapsed() >= Duration::from_secs(RELOAD_THROTTLE_SECS))
    }

    fn maybe_reload_if_newer(&mut self) {
        if !self.needs_reload_check() {
            return;
        }
        self.last_check = Some(Instant::now());
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let cur_mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
        if cur_mtime.is_some() && cur_mtime != self.last_mtime {
            self.load_from_path(&path);
        }
    }
}

impl CacheLmSnapshot {
    /// 於單一 Viterbi 轉移邊上計算語境化 Cache LM 強化／懲罰總分，夾限至 `[-18.0, +18.0]`
    pub fn score_edge(
        &self,
        prev_word: &str,
        curr_word: &str,
        end_idx: usize,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        if !self.tables.known_words.contains(curr_word) {
            return 0.0;
        }
        let left_ch = last_cjk_char(prev_word);
        let mut score = 0.0;

        if !left_ch.is_empty() {
            if let Some(entry) = self
                .tables
                .left_bigrams
                .get(&(left_ch.clone(), curr_word.to_string()))
            {
                score += decayed_strength(entry, self.now_secs);
            }
        }

        // 收集右鄰音節邊界上具競爭力之首漢字（去重），另於句尾納入空字串錨點
        let mut cand_rights: Vec<String> = Vec::new();
        if let Some(right_nodes) = dag.get(&end_idx) {
            for m in right_nodes {
                if m.weight < COMPETITIVE_RIGHT_MIN_WEIGHT {
                    continue;
                }
                let rc = first_cjk_char(&m.word);
                if !rc.is_empty() && !cand_rights.iter().any(|x| x == &rc) {
                    cand_rights.push(rc);
                }
            }
        }
        if end_idx == n_syllables && !cand_rights.iter().any(|x| x.is_empty()) {
            cand_rights.push(String::new());
        }

        let mut best_tri: Option<f64> = None;
        let mut best_rb: Option<f64> = None;
        for rc in &cand_rights {
            if !left_ch.is_empty() || !rc.is_empty() {
                if let Some(entry) = self.tables.trigrams.get(&(
                    left_ch.clone(),
                    curr_word.to_string(),
                    rc.clone(),
                )) {
                    let val = decayed_strength(entry, self.now_secs);
                    if best_tri.map_or(true, |b| val.abs() > b.abs()) {
                        best_tri = Some(val);
                    }
                }
            }
            if !rc.is_empty() {
                if let Some(entry) = self
                    .tables
                    .right_bigrams
                    .get(&(curr_word.to_string(), rc.clone()))
                {
                    let val = decayed_strength(entry, self.now_secs);
                    if best_rb.map_or(true, |b| val.abs() > b.abs()) {
                        best_rb = Some(val);
                    }
                }
            }
        }

        if let Some(t) = best_tri {
            score += t;
        }
        if let Some(rb) = best_rb {
            score += rb;
        }

        score.clamp(-EDGE_TOTAL_CLAMP, EDGE_TOTAL_CLAMP)
    }
}

pub fn bind_path_for_key(key: usize, path: &Path) {
    if let Ok(mut map) = store().write() {
        let entry = map.entry(key).or_default();
        entry.load_from_path(path);
    }
}

pub fn flush_if_dirty_for_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(entry) = map.get_mut(&key) {
            entry.flush_if_dirty();
        }
    }
}

pub fn clear_for_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(entry) = map.get_mut(&key) {
            entry.tables = CacheLmTables::default();
            entry.snapshot = Arc::new(CacheLmTables::default());
            entry.dirty = true;
        }
    }
}

pub fn remove_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(mut entry) = map.remove(&key) {
            entry.flush_if_dirty();
        }
    }
}

pub fn record_correction_for_key(
    key: usize,
    prev_ctx: &str,
    rejected: &str,
    accepted: &str,
    next_ctx: &str,
    now_secs: u64,
) {
    if let Ok(mut map) = store().write() {
        let entry = map.entry(key).or_default();
        entry.record_correction(prev_ctx, rejected, accepted, next_ctx, now_secs);
    }
}

pub fn record_sentence_span_diffs_for_key(
    key: usize,
    raw_sentence: &str,
    cor_sentence: &str,
    now_secs: u64,
) {
    if let Ok(mut map) = store().write() {
        let entry = map.entry(key).or_default();
        entry.record_sentence_span_diffs(raw_sentence, cor_sentence, now_secs);
    }
}

pub fn snapshot_if_non_empty(key: usize) -> Option<CacheLmSnapshot> {
    let needs_reload = {
        let map = store().read().ok()?;
        let entry = map.get(&key)?;
        if !entry.needs_reload_check() {
            if entry.tables.known_words.is_empty() {
                return None;
            }
            return Some(CacheLmSnapshot {
                tables: Arc::clone(&entry.snapshot),
                now_secs: current_unix_secs(),
            });
        }
        true
    };
    if needs_reload {
        if let Ok(mut map) = store().write() {
            if let Some(entry) = map.get_mut(&key) {
                entry.maybe_reload_if_newer();
                if entry.tables.known_words.is_empty() {
                    return None;
                }
                return Some(CacheLmSnapshot {
                    tables: Arc::clone(&entry.snapshot),
                    now_secs: current_unix_secs(),
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_decay_and_preference_reversal() {
        let pos = CacheEntry {
            strength: 10.0,
            last_ts: 1_000,
            hits: 1,
        };
        let after_30d = decayed_strength(&pos, 1_000 + 30 * 86_400);
        assert!((after_30d - 5.0).abs() < 1e-6, "after 30d: {}", after_30d);

        let neg = CacheEntry {
            strength: -8.0,
            last_ts: 1_000,
            hits: 1,
        };
        let after_7d = decayed_strength(&neg, 1_000 + 7 * 86_400);
        assert!((after_7d + 4.0).abs() < 1e-6, "after 7d: {}", after_7d);

        let key = 0xDEAD_BEEF_0001usize;
        remove_key(key);
        record_correction_for_key(key, "請", "著", "卓", "參", 10_000);
        let snap1 = snapshot_if_non_empty(key).expect("snapshot should exist");
        let e_zhuo = snap1
            .tables
            .trigrams
            .get(&("請".to_string(), "卓".to_string(), "參".to_string()))
            .unwrap();
        assert!(e_zhuo.strength > 0.0);

        // 反悔覆寫：改回「著」應迅速壓低「卓」並轉正「著」
        record_correction_for_key(key, "請", "卓", "著", "參", 10_005);
        record_correction_for_key(key, "請", "卓", "著", "參", 10_010);
        let snap2 = snapshot_if_non_empty(key).unwrap();
        let e_zhuo2 = snap2
            .tables
            .trigrams
            .get(&("請".to_string(), "卓".to_string(), "參".to_string()))
            .unwrap();
        let e_zhe2 = snap2
            .tables
            .trigrams
            .get(&("請".to_string(), "著".to_string(), "參".to_string()))
            .unwrap();
        assert!(e_zhuo2.strength < 0.0, "expected negative, got {}", e_zhuo2.strength);
        assert!(e_zhe2.strength > 0.0, "expected positive, got {}", e_zhe2.strength);
        remove_key(key);
    }

    #[test]
    fn test_strict_context_only_and_capacity_eviction() {
        let key = 0xDEAD_BEEF_0002usize;
        remove_key(key);

        // 無左右語境錨定時一律拒絕寫入，防止退化成無差別全域單字調權重
        record_correction_for_key(key, "", "緊", "謹", "", 1_000);
        assert!(snapshot_if_non_empty(key).is_none());

        // 灌入超過 2048 筆資料應自動修剪至上限內
        for idx in 0..2200u64 {
            let left = char::from_u32(0x4E00 + (idx % 40) as u32).unwrap().to_string();
            let right = char::from_u32(0x4E50 + (idx / 40) as u32).unwrap().to_string();
            record_correction_for_key(key, &left, "事", "是", &right, 2_000 + idx);
        }
        let snap = snapshot_if_non_empty(key).unwrap();
        assert!(snap.tables.trigrams.len() <= MAX_ENTRIES_PER_TABLE);
        remove_key(key);
    }

    #[test]
    fn test_atomic_tsv_save_and_reload_roundtrip() {
        let key_w = 0xDEAD_BEEF_0003usize;
        let key_r = 0xDEAD_BEEF_0004usize;
        remove_key(key_w);
        remove_key(key_r);

        let tmp = std::env::temp_dir().join(format!(
            "ruhi_cache_lm_test_{}.tsv",
            std::process::id()
        ));
        let _ = fs::remove_file(&tmp);

        bind_path_for_key(key_w, &tmp);
        record_correction_for_key(key_w, "", "緊", "謹", "請", current_unix_secs());
        record_correction_for_key(key_w, "請", "著", "卓", "參", current_unix_secs());
        flush_if_dirty_for_key(key_w);
        assert!(tmp.exists());

        bind_path_for_key(key_r, &tmp);
        let snap = snapshot_if_non_empty(key_r).expect("reloaded snapshot must exist");
        assert!(snap.tables.known_words.contains("謹"));
        assert!(snap.tables.known_words.contains("卓"));

        remove_key(key_w);
        remove_key(key_r);
        let _ = fs::remove_file(&tmp);
    }
}
