//! 公用自適應預設值與使用者習慣記憶核心引擎 (`preset_engine.rs`)
//!
//! 對應 Python 版 `_lib/preset_engine.py`。
//!
//! ### 核心演算法（頻率加權 ＋ 近期衰減模型）
//! $$\text{Score}(v) = \text{Count}(v) \times W_{\text{freq}} + \frac{W_{\text{recent}}}{1.0 + (\Delta t_{\text{hours}} / 24.0)}$$
//!
//! - 當使用者頻繁輸入或選取自訂數值（如鬧鐘時間 `06:45`、倒數時長 `45` 分鐘、取樣率 `48000`）時，
//!   該數值的綜合評分將自動超越原廠預設值，動態晉升至快捷按鈕列。
//! - 支援圖釘鎖定 (`is_pinned`)、一鍵恢復原廠預設值，並可與 [`crate::config_store::AppConfig`] 無縫同步持久化。

use crate::config_store::AppConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

fn current_unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// 單一自適應預設值候選項目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptivePresetItem {
    /// 預設值字串鍵或數值表示（例如 `"06:45"`、`"45"`）
    pub value: String,
    /// 顯示於快捷膠囊按鈕上的標籤（例如 `"⏰ 06:45"`、 `"+45 分鐘"`）
    pub label: String,
    /// 是否為原廠預設項目
    pub is_default: bool,
    /// 是否由使用者釘選鎖定（釘選項目永遠優先排在最前）
    pub is_pinned: bool,
    /// 累計使用次數
    pub count: u32,
    /// 最後一次使用之 Unix 時間戳（秒）
    pub last_used: f64,
    /// 附加元資料
    #[serde(default)]
    pub extra: HashMap<String, Value>,
}

impl AdaptivePresetItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>, is_default: bool) -> Self {
        let v = value.into();
        let l = label.into();
        Self {
            label: if l.is_empty() { v.clone() } else { l },
            value: v,
            is_default,
            is_pinned: false,
            count: 1,
            last_used: current_unix_seconds(),
            extra: HashMap::new(),
        }
    }
}

/// 單一業務領域（如 `"alarm_time"`、`"timer_duration"`）之預設值管理模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptivePresetDomain {
    pub domain_key: String,
    pub max_slots: usize,
    pub recency_weight: f64,
    pub frequency_weight: f64,
    pub default_items: Vec<AdaptivePresetItem>,
    pub history: HashMap<String, AdaptivePresetItem>,
}

impl AdaptivePresetDomain {
    pub fn new(
        domain_key: impl Into<String>,
        default_items: Vec<(String, String)>,
        max_slots: usize,
    ) -> Self {
        let defs = default_items
            .into_iter()
            .map(|(val, lbl)| AdaptivePresetItem::new(val, lbl, true))
            .collect();
        Self {
            domain_key: domain_key.into(),
            max_slots: max_slots.max(1),
            recency_weight: 2.0,
            frequency_weight: 1.0,
            default_items: defs,
            history: HashMap::new(),
        }
    }

    /// 記錄一次使用者選取或自訂輸入，提升該項目的頻率與近期權重。
    pub fn record_usage(&mut self, value: &str, label: Option<&str>) {
        self.record_usage_at(value, label, current_unix_seconds());
    }

    /// 指定時間戳記錄使用次數（便於確定性單元測試）。
    pub fn record_usage_at(&mut self, value: &str, label: Option<&str>, now: f64) {
        if let Some(item) = self.history.get_mut(value) {
            item.count = item.count.saturating_add(1);
            item.last_used = now;
            if let Some(lbl) = label {
                if !lbl.is_empty() {
                    item.label = lbl.to_string();
                }
            }
        } else {
            let matching_def = self.default_items.iter().find(|d| d.value == value);
            let is_def = matching_def.is_some();
            let is_pinned = matching_def.map(|d| d.is_pinned).unwrap_or(false);
            let resolved_label = label
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| matching_def.map(|d| d.label.clone()))
                .unwrap_or_else(|| value.to_string());

            let mut item = AdaptivePresetItem::new(value, resolved_label, is_def);
            item.is_pinned = is_pinned;
            item.last_used = now;
            self.history.insert(value.to_string(), item);
        }
    }

    /// 計算單一項目的綜合評分。
    pub fn compute_score(&self, item: &AdaptivePresetItem, now: f64) -> f64 {
        if item.is_pinned {
            return 999_999.0 + (item.count as f64);
        }
        let hours_ago = ((now - item.last_used) / 3600.0).max(0.0);
        let recency_factor = self.recency_weight / (1.0 + (hours_ago / 24.0));
        let freq_factor = (item.count as f64) * self.frequency_weight;
        freq_factor + recency_factor
    }

    /// 取得當前分數最高的前 `max_slots` 個預設值項目。
    pub fn get_active_presets(&self) -> Vec<AdaptivePresetItem> {
        self.get_active_presets_at(current_unix_seconds())
    }

    /// 於指定時間戳取得前 `max_slots` 個預設值項目。
    pub fn get_active_presets_at(&self, now: f64) -> Vec<AdaptivePresetItem> {
        let mut candidates: Vec<AdaptivePresetItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for (k, item) in &self.history {
            candidates.push(item.clone());
            seen.insert(k.clone());
        }
        for d in &self.default_items {
            if !seen.contains(&d.value) {
                candidates.push(d.clone());
                seen.insert(d.value.clone());
            }
        }

        candidates.sort_by(|a, b| {
            let sa = self.compute_score(a, now);
            let sb = self.compute_score(b, now);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(self.max_slots);
        candidates
    }

    /// 釘選或取消釘選某個預設值項目。
    pub fn pin_preset(&mut self, value: &str, pinned: bool) {
        if let Some(item) = self.history.get_mut(value) {
            item.is_pinned = pinned;
            return;
        }
        if let Some(def) = self.default_items.iter_mut().find(|d| d.value == value) {
            def.is_pinned = pinned;
            return;
        }
        self.record_usage(value, None);
        if let Some(item) = self.history.get_mut(value) {
            item.is_pinned = pinned;
        }
    }

    /// 清除所有歷史記錄，回歸原廠預設值。
    pub fn reset_to_defaults(&mut self) {
        self.history.clear();
    }
}

/// 跨領域通用自適應預設值引擎
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PresetEngine {
    pub namespace: String,
    pub domains: HashMap<String, AdaptivePresetDomain>,
}

impl PresetEngine {
    pub fn new() -> Self {
        Self {
            namespace: "adaptive_presets".to_string(),
            domains: HashMap::new(),
        }
    }

    /// 註冊一個預設值領域（若已存在則保留其歷史記錄並更新預設項）。
    pub fn register_domain(
        &mut self,
        domain_key: &str,
        default_items: Vec<(String, String)>,
        max_slots: usize,
    ) -> &mut AdaptivePresetDomain {
        self.domains
            .entry(domain_key.to_string())
            .or_insert_with(|| AdaptivePresetDomain::new(domain_key, default_items, max_slots))
    }

    /// 記錄使用者在指定領域的選擇。
    pub fn record_usage(&mut self, domain_key: &str, value: &str, label: Option<&str>) {
        if let Some(dom) = self.domains.get_mut(domain_key) {
            dom.record_usage(value, label);
        }
    }

    /// 取得指定領域的前 `max_slots` 個活躍預設值。
    pub fn get_active_presets(&self, domain_key: &str) -> Vec<AdaptivePresetItem> {
        self.domains
            .get(domain_key)
            .map(|d| d.get_active_presets())
            .unwrap_or_default()
    }

    /// 將指定領域重設為原廠預設值。
    pub fn reset_domain(&mut self, domain_key: &str) {
        if let Some(dom) = self.domains.get_mut(domain_key) {
            dom.reset_to_defaults();
        }
    }

    /// 釘選或取消釘選指定領域中的某個項目。
    pub fn pin_preset(&mut self, domain_key: &str, value: &str, pinned: bool) {
        if let Some(dom) = self.domains.get_mut(domain_key) {
            dom.pin_preset(value, pinned);
        }
    }

    /// 從 [`AppConfig`] 載入已保存之所有領域歷史記錄。
    pub fn load_from_config(&mut self, config: &AppConfig) {
        if let Some(saved_map) = config.get::<HashMap<String, AdaptivePresetDomain>>(&self.namespace)
        {
            for (k, saved_dom) in saved_map {
                if let Some(existing) = self.domains.get_mut(&k) {
                    existing.max_slots = saved_dom.max_slots;
                    existing.history = saved_dom.history;
                } else {
                    self.domains.insert(k, saved_dom);
                }
            }
        }
    }

    /// 將所有領域之自適應歷史寫入 [`AppConfig`] 並持久化存檔。
    pub fn save_to_config(&self, config: &mut AppConfig) {
        config.set(&self.namespace, &self.domains);
        let _ = config.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_preset_promotion_and_pinning() {
        let mut engine = PresetEngine::new();
        engine.register_domain(
            "sleep_minutes",
            vec![
                ("15".to_string(), "15 分鐘".to_string()),
                ("30".to_string(), "30 分鐘".to_string()),
                ("60".to_string(), "60 分鐘".to_string()),
            ],
            3,
        );

        let initial = engine.get_active_presets("sleep_minutes");
        assert_eq!(initial.len(), 3);

        // 使用者連續 3 次自訂 "45" 分鐘，分數應超越僅使用 1 次的原廠預設值
        engine.record_usage("sleep_minutes", "45", Some("45 分鐘"));
        engine.record_usage("sleep_minutes", "45", Some("45 分鐘"));
        engine.record_usage("sleep_minutes", "45", Some("45 分鐘"));

        let updated = engine.get_active_presets("sleep_minutes");
        assert_eq!(updated[0].value, "45");
        assert_eq!(updated[0].label, "45 分鐘");

        // 釘選 "90" 分鐘應直接排到第一順位
        if let Some(dom) = engine.domains.get_mut("sleep_minutes") {
            dom.pin_preset("90", true);
        }
        let after_pin = engine.get_active_presets("sleep_minutes");
        assert_eq!(after_pin[0].value, "90");
        assert!(after_pin[0].is_pinned);

        // 重設回原廠預設值
        engine.reset_domain("sleep_minutes");
        let after_reset = engine.get_active_presets("sleep_minutes");
        assert!(after_reset.iter().all(|it| it.value != "45"));
    }
}
