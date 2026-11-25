// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/topic_cache.rs — 對話／文件工作階段主題詞突發快取 (`Topic Burstiness Cache`)
//
// 僅於記憶體中記錄單一文件工作階段內最近送出之 2–4 字實詞頻次（滑動視窗 128 詞，換段即清空），
// 提供輕量且有界之主題詞復現加權，嚴格維持 `RuhiEngine` / `TransitionEngine` 二進位佈局不變，
// 且絕不改動全域詞庫或靜態單字先驗權重。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock, RwLock};

use super::lattice::LatticeNode;

const MAX_TRACKED_KEYS: usize = 64;
const MAX_WINDOW_TOKENS: usize = 128;

const TOPIC_HIT1_BONUS: f64 = 3.0;
const TOPIC_HIT2_BONUS: f64 = 4.5;
const TOPIC_HIT3_BONUS: f64 = 5.5;
const TOPIC_SENTENCE_CAP: f64 = 12.0;

const TOPIC_STOPWORDS: &[&str] = &[
    "我們", "你們", "他們", "她們", "它們", "牠們", "這個", "那個", "一個", "這些", "那些",
    "可以", "已經", "因為", "所以", "但是", "如果", "雖然", "不是", "沒有", "這樣", "那樣",
    "什麼", "怎麼", "這裡", "那裡", "非常", "應該", "必須", "可能", "現在", "今天", "明天",
    "昨天", "時候", "時間", "問題", "大家", "自己", "知道", "覺得", "認為", "希望", "就是",
    "還是", "或者", "以及", "而且", "不過", "只要", "只有", "一定", "一直", "一起", "一樣",
    "一些", "一下", "一般", "其中", "其實", "其他", "之後", "之前", "以後", "以前", "開始",
    "進行", "使用", "需要", "透過", "關於", "對於", "由於", "因此", "然而", "於是", "甚至",
    "並且", "同時", "目前", "最近", "正在", "曾經", "比較", "真的", "確實", "完全", "直接",
    "只是", "也是", "都是", "地方", "東西", "事情", "部分", "部份", "方面", "方式", "方法",
    "狀況", "情況", "結果", "過程",
];

#[derive(Default)]
struct DocumentTopicCache {
    window: VecDeque<String>,
    counts: HashMap<String, u16>,
    snapshot: Arc<HashMap<String, u16>>,
}

impl DocumentTopicCache {
    fn record_tokens(&mut self, tokens: &[String]) {
        let mut changed = false;
        for tok in tokens {
            if !is_valid_topic_token(tok) {
                continue;
            }
            if self.window.len() >= MAX_WINDOW_TOKENS {
                if let Some(old) = self.window.pop_front() {
                    if let Some(cnt) = self.counts.get_mut(&old) {
                        if *cnt <= 1 {
                            self.counts.remove(&old);
                        } else {
                            *cnt -= 1;
                        }
                    }
                }
            }
            self.window.push_back(tok.clone());
            let entry = self.counts.entry(tok.clone()).or_insert(0);
            *entry = entry.saturating_add(1);
            changed = true;
        }
        if changed {
            self.snapshot = Arc::new(self.counts.clone());
        }
    }

    fn clear(&mut self) {
        self.window.clear();
        self.counts.clear();
        self.snapshot = Arc::new(HashMap::new());
    }
}

static STORE: OnceLock<RwLock<HashMap<usize, DocumentTopicCache>>> = OnceLock::new();

fn store() -> &'static RwLock<HashMap<usize, DocumentTopicCache>> {
    STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[derive(Debug, Clone)]
pub struct TopicCacheSnapshot {
    counts: Arc<HashMap<String, u16>>,
}

impl TopicCacheSnapshot {
    /// 計算單一候選詞彙之主題詞復現加權（2–4 字實詞未命中回傳 `0.0`，否則依次數回傳 `3.0 / 4.5 / 5.5`）
    #[inline]
    pub fn score_word(&self, word: &str) -> f64 {
        if word.len() < 6 {
            return 0.0;
        }
        match self.counts.get(word).copied() {
            None | Some(0) => 0.0,
            Some(1) => TOPIC_HIT1_BONUS,
            Some(2) => TOPIC_HIT2_BONUS,
            Some(_) => TOPIC_HIT3_BONUS,
        }
    }

    /// 計算整條候選路徑之主題詞復現總分（句內同詞僅採計一次，限幅於 `[0.0, 12.0]`）
    pub fn score_sentence(&self, nodes: &[LatticeNode]) -> f64 {
        let mut seen: Vec<&str> = Vec::with_capacity(nodes.len().min(8));
        let mut total = 0.0;
        for n in nodes {
            let w = n.word.as_str();
            if w.len() < 6 || seen.contains(&w) {
                continue;
            }
            let b = self.score_word(w);
            if b > 0.0 {
                seen.push(w);
                total += b;
                if total >= TOPIC_SENTENCE_CAP {
                    return TOPIC_SENTENCE_CAP;
                }
            }
        }
        total.clamp(0.0, TOPIC_SENTENCE_CAP)
    }
}

/// 判斷字詞是否符合文件主題快取收納標準（2–4 個基本區漢字，且不在高頻功能停用詞表）
#[inline]
pub fn is_valid_topic_token(word: &str) -> bool {
    let mut count = 0usize;
    for ch in word.chars() {
        if !('\u{4E00}'..='\u{9FFF}').contains(&ch) {
            return false;
        }
        count += 1;
        if count > 4 {
            return false;
        }
    }
    count >= 2 && !TOPIC_STOPWORDS.contains(&word)
}

pub fn record_tokens_for_key(key: usize, tokens: &[String]) {
    if tokens.is_empty() {
        return;
    }
    if let Ok(mut map) = store().write() {
        if !map.contains_key(&key) && map.len() >= MAX_TRACKED_KEYS {
            map.clear();
        }
        let entry = map.entry(key).or_default();
        entry.record_tokens(tokens);
    }
}

pub fn clear_for_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(entry) = map.get_mut(&key) {
            entry.clear();
        }
    }
}

pub fn remove_key(key: usize) {
    if let Ok(mut map) = store().write() {
        map.remove(&key);
    }
}

pub fn snapshot_if_non_empty(key: usize) -> Option<TopicCacheSnapshot> {
    let map = store().read().ok()?;
    let entry = map.get(&key)?;
    if entry.counts.is_empty() {
        return None;
    }
    Some(TopicCacheSnapshot {
        counts: Arc::clone(&entry.snapshot),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(words: &[&str]) -> Vec<LatticeNode> {
        let mut pos = 0usize;
        words
            .iter()
            .map(|&w| {
                let len = w.chars().count().max(1);
                let n = LatticeNode::new(pos, pos + len, w, 400, "");
                pos += len;
                n
            })
            .collect()
    }

    #[test]
    fn test_topic_cache_stopword_saturation_eviction_and_cap() {
        let key = 0xCAFE_BABE_0001usize;
        remove_key(key);

        assert!(!is_valid_topic_token("我們"));
        assert!(!is_valid_topic_token("這"));
        assert!(!is_valid_topic_token("五個字以上詞"));
        assert!(!is_valid_topic_token("AB組"));
        assert!(is_valid_topic_token("量子電腦"));
        assert!(is_valid_topic_token("異步"));

        record_tokens_for_key(
            key,
            &[
                "我們".to_string(),
                "量子電腦".to_string(),
                "的".to_string(),
            ],
        );
        let s1 = snapshot_if_non_empty(key).expect("snapshot should exist");
        assert_eq!(s1.score_word("我們"), 0.0);
        assert_eq!(s1.score_word("量子電腦"), 3.0);

        record_tokens_for_key(key, &["量子電腦".to_string()]);
        let s2 = snapshot_if_non_empty(key).unwrap();
        assert_eq!(s2.score_word("量子電腦"), 4.5);

        record_tokens_for_key(key, &["量子電腦".to_string(), "半導體".to_string(), "光刻機".to_string()]);
        let s3 = snapshot_if_non_empty(key).unwrap();
        assert_eq!(s3.score_word("量子電腦"), 5.5);

        // 句內重複詞僅採計一次，且整句上限為 12.0
        let dup_nodes = mk(&["量子電腦", "搭配", "量子電腦"]);
        assert!((s3.score_sentence(&dup_nodes) - 5.5).abs() < 1e-6);

        let multi_nodes = mk(&["量子電腦", "量子電腦", "半導體", "光刻機"]);
        // 5.5 + 3.0 + 3.0 = 11.5 (deduped "量子電腦")
        assert!((s3.score_sentence(&multi_nodes) - 11.5).abs() < 1e-6);

        // 推入 128 筆其他詞後，「量子電腦」應滑出視窗
        let filler: Vec<String> = (0..MAX_WINDOW_TOKENS)
            .map(|i| {
                let c1 = char::from_u32(0x4E00 + (i as u32 % 40)).unwrap();
                let c2 = char::from_u32(0x4E40 + (i as u32 / 40)).unwrap();
                format!("{c1}{c2}")
            })
            .collect();
        record_tokens_for_key(key, &filler);
        let s_evicted = snapshot_if_non_empty(key).unwrap();
        assert_eq!(s_evicted.score_word("量子電腦"), 0.0);

        clear_for_key(key);
        assert!(snapshot_if_non_empty(key).is_none());
        remove_key(key);
    }
}
