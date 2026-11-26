// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/variant_prefs.rs — 異體雙寫自動記憶與個人正寫偏好模組
//
// 針對臺灣正體中文常見之無歧義異體雙寫詞組（例如「台北／臺北」、「台灣／臺灣」、「公布／公佈」等）
// 以及具左右語境與注音錨定之單人名／姓氏選字偏好（例如「劉／柳」），提供：
// 1. 三種偏好預設模式：`general`（一般自動學習）、`official_tw`（中華民國公文書正寫偏好）、`custom`（完全手動自訂）。
// 2. 以 `OnceLock<RwLock<HashMap<usize, VariantPrefsState>>>` 旁表索引 `TransitionEngine::cache_lm_key()`，
//    嚴格維持 `RuhiEngine` / `TransitionEngine` / `RuhiTextService` 二進位佈局不變。
// 3. `variant_prefs.json` 人類可讀格式化持久化，並具備受保護安裝目錄（如 `Program Files`）自動回退至
//    `%APPDATA%/Ruhi/variant_prefs.json` 之寫入能力。
// 4. Viterbi 解碼加權與神經重排序後保護（`apply_post_neural_protection`），確保已記憶之雙寫或人名選擇
//    不致遭神經語言模型覆寫。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::lattice::LatticeNode;

const DOUBLETS_BONUS: f64 = 24.0;
const NAME_OVERRIDE_BONUS: f64 = 24.0;
const RELOAD_THROTTLE_SECS: u64 = 2;
const COMPETITIVE_RIGHT_MIN_WEIGHT: u32 = 80;
const MAX_NAME_OVERRIDES: usize = 512;
const MAX_TRACKED_KEYS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VariantPreset {
    #[default]
    General,
    OfficialTw,
    Custom,
}

impl VariantPreset {
    pub fn from_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "official_tw" | "official" | "tw_official" => Self::OfficialTw,
            "custom" | "manual" => Self::Custom,
            _ => Self::General,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::OfficialTw => "official_tw",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DoubletDef {
    pub a: &'static str,
    pub b: &'static str,
    pub official_tw: &'static str,
}

/// 臺灣常見無歧義異體雙寫詞組對照表（依字數降冪或等長排列，方便最長區段優先比對）
pub const DOUBLETS: &[DoubletDef] = &[
    DoubletDef { a: "台北市", b: "臺北市", official_tw: "臺北市" },
    DoubletDef { a: "台中市", b: "臺中市", official_tw: "臺中市" },
    DoubletDef { a: "台南市", b: "臺南市", official_tw: "臺南市" },
    DoubletDef { a: "台東縣", b: "臺東縣", official_tw: "臺東縣" },
    DoubletDef { a: "台灣省", b: "臺灣省", official_tw: "臺灣省" },
    DoubletDef { a: "台北", b: "臺北", official_tw: "臺北" },
    DoubletDef { a: "台灣", b: "臺灣", official_tw: "臺灣" },
    DoubletDef { a: "台中", b: "臺中", official_tw: "臺中" },
    DoubletDef { a: "台南", b: "臺南", official_tw: "臺南" },
    DoubletDef { a: "台東", b: "臺東", official_tw: "臺東" },
    DoubletDef { a: "台大", b: "臺大", official_tw: "臺大" },
    DoubletDef { a: "台語", b: "臺語", official_tw: "臺語" },
    DoubletDef { a: "了解", b: "瞭解", official_tw: "瞭解" },
    DoubletDef { a: "部分", b: "部份", official_tw: "部分" },
    DoubletDef { a: "裡面", b: "裏面", official_tw: "裡面" },
    DoubletDef { a: "發布", b: "發佈", official_tw: "發佈" },
    DoubletDef { a: "公布", b: "公佈", official_tw: "公佈" },
    DoubletDef { a: "宣布", b: "宣佈", official_tw: "宣佈" },
    DoubletDef { a: "佈局", b: "布局", official_tw: "布局" },
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VariantPairRecord {
    pub preferred: String,
    pub count: u32,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NameOverrideRecord {
    pub preferred: String,
    pub rejected: String,
    pub count: u32,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VariantPrefsFile {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_preset_str")]
    preset: String,
    #[serde(default)]
    pairs: BTreeMap<String, VariantPairRecord>,
    #[serde(default)]
    surname_or_name_overrides: BTreeMap<String, NameOverrideRecord>,
}

fn default_true() -> bool {
    true
}

fn default_preset_str() -> String {
    "general".to_string()
}

#[derive(Debug, Clone, Copy)]
struct RuntimeConfig {
    configured: bool,
    enabled: bool,
    preset: VariantPreset,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            configured: false,
            enabled: true,
            preset: VariantPreset::General,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct VariantPrefsTables {
    pairs: BTreeMap<String, VariantPairRecord>,
    surname_or_name_overrides: BTreeMap<String, NameOverrideRecord>,
    name_override_words: HashSet<String>,
}

#[derive(Debug, Default)]
struct VariantPrefsState {
    tables: VariantPrefsTables,
    snapshot: Arc<VariantPrefsTables>,
    file_path: Option<PathBuf>,
    last_mtime: Option<SystemTime>,
    last_check: Option<Instant>,
    dirty: bool,
    file_enabled: Option<bool>,
    file_preset: Option<VariantPreset>,
    key_override_enabled: Option<bool>,
    key_override_preset: Option<VariantPreset>,
}

#[derive(Debug, Clone)]
pub struct VariantPrefsSnapshot {
    enabled: bool,
    preset: VariantPreset,
    tables: Arc<VariantPrefsTables>,
}

static STORE: OnceLock<RwLock<HashMap<usize, VariantPrefsState>>> = OnceLock::new();
static RUNTIME_CFG: OnceLock<RwLock<RuntimeConfig>> = OnceLock::new();

#[inline]
fn store() -> &'static RwLock<HashMap<usize, VariantPrefsState>> {
    STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[inline]
fn runtime_cfg() -> &'static RwLock<RuntimeConfig> {
    RUNTIME_CFG.get_or_init(|| RwLock::new(RuntimeConfig::default()))
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
fn last_cjk_char(s: &str) -> String {
    s.chars()
        .rev()
        .find(|&c| is_cjk_char(c))
        .map(|c| c.to_string())
        .unwrap_or_default()
}

#[inline]
fn first_cjk_char(s: &str) -> String {
    s.chars()
        .find(|&c| is_cjk_char(c))
        .map(|c| c.to_string())
        .unwrap_or_default()
}

#[inline]
fn is_valid_cjk_len(s: &str, min_len: usize, max_len: usize) -> bool {
    let mut cnt = 0usize;
    for c in s.chars() {
        if !is_cjk_char(c) {
            return false;
        }
        cnt += 1;
        if cnt > max_len {
            return false;
        }
    }
    cnt >= min_len
}

#[inline]
pub fn canonical_key(a: &str, b: &str) -> String {
    format!("{a}|{b}")
}

#[inline]
fn is_surname_str(s: &str) -> bool {
    !s.is_empty()
        && (super::onomastics::TIER_A_SURNAMES.contains(s)
            || super::onomastics::COMPOUND_SURNAMES.contains(&s))
}

#[inline]
fn is_graphic_variant_char_pair(w1: &str, w2: &str) -> bool {
    const CHAR_VARIANTS: &[(&str, &str)] = &[
        ("台", "臺"), ("裡", "裏"), ("為", "爲"), ("著", "着"), ("真", "眞"),
        ("強", "强"), ("教", "敎"), ("溫", "温"), ("黃", "黄"), ("衛", "衞"),
        ("峰", "峯"), ("群", "羣"), ("啟", "啓"), ("悅", "悦"), ("戶", "户"),
    ];
    CHAR_VARIANTS
        .iter()
        .any(|&(a, b)| (w1 == a && w2 == b) || (w1 == b && w2 == a))
}

#[inline]
fn is_onomastic_or_variant_context(left_ch: &str, rejected: &str, accepted: &str) -> bool {
    if is_surname_str(left_ch) {
        return true;
    }
    if rejected.chars().count() == 1 && accepted.chars().count() == 1 {
        if (is_surname_str(rejected) && is_surname_str(accepted))
            || is_graphic_variant_char_pair(rejected, accepted)
        {
            return true;
        }
    }
    false
}

#[inline]
fn find_doublet(word: &str) -> Option<(String, &'static DoubletDef)> {
    // 內建雙寫表目前全為 2–3 字漢字（UTF-8 位元組長度 6..=9），快速排除非目標詞
    if !(6..=9).contains(&word.len()) {
        return None;
    }
    for def in DOUBLETS {
        if word == def.a || word == def.b {
            return Some((canonical_key(def.a, def.b), def));
        }
    }
    None
}

#[inline]
fn find_doublet_pair(w1: &str, w2: &str) -> Option<(String, &'static DoubletDef)> {
    if w1 == w2 || !(6..=9).contains(&w1.len()) || w1.len() != w2.len() {
        return None;
    }
    for def in DOUBLETS {
        if (w1 == def.a && w2 == def.b) || (w1 == def.b && w2 == def.a) {
            return Some((canonical_key(def.a, def.b), def));
        }
    }
    None
}

fn is_under_program_files(path: &Path) -> bool {
    let low = path.to_string_lossy().to_ascii_lowercase();
    low.contains("\\program files\\") || low.contains("\\program files (x86)\\")
}

fn appdata_fallback_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .filter(|s| !s.is_empty())
        .map(|base| PathBuf::from(base).join("Ruhi").join("variant_prefs.json"))
}

fn effective_config_for_state(state: Option<&VariantPrefsState>, global: RuntimeConfig) -> (bool, VariantPreset) {
    if let Some(st) = state {
        let enabled = st.key_override_enabled.unwrap_or_else(|| {
            if global.configured {
                global.enabled
            } else {
                st.file_enabled.unwrap_or(global.enabled)
            }
        });
        let preset = st.key_override_preset.unwrap_or_else(|| {
            if global.configured {
                global.preset
            } else {
                st.file_preset.unwrap_or(global.preset)
            }
        });
        (enabled, preset)
    } else {
        (global.enabled, global.preset)
    }
}

fn read_runtime_cfg() -> RuntimeConfig {
    runtime_cfg()
        .read()
        .map(|g| *g)
        .unwrap_or_default()
}

pub fn set_runtime_config(enabled: bool, preset: &str) {
    if let Ok(mut g) = runtime_cfg().write() {
        g.configured = true;
        g.enabled = enabled;
        g.preset = VariantPreset::from_str(preset);
    }
}

pub fn get_runtime_config() -> (bool, VariantPreset) {
    let g = read_runtime_cfg();
    (g.enabled, g.preset)
}

pub fn set_key_config(key: usize, enabled: bool, preset: VariantPreset) {
    if let Ok(mut map) = store().write() {
        ensure_capacity(&mut map, key);
        let st = map.entry(key).or_default();
        st.key_override_enabled = Some(enabled);
        st.key_override_preset = Some(preset);
    }
}

pub fn clear_key_config(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(st) = map.get_mut(&key) {
            st.key_override_enabled = None;
            st.key_override_preset = None;
        }
    }
}

fn ensure_capacity(map: &mut HashMap<usize, VariantPrefsState>, keep_key: usize) {
    if map.contains_key(&keep_key) || map.len() < MAX_TRACKED_KEYS {
        return;
    }
    map.retain(|&k, st| k == keep_key || st.file_path.is_some() || !st.tables.pairs.is_empty());
    if map.len() >= MAX_TRACKED_KEYS {
        if let Some(evict_k) = map.keys().copied().find(|&k| k != keep_key) {
            map.remove(&evict_k);
        }
    }
}

impl VariantPrefsState {
    fn rebuild_snapshot(&mut self) {
        if self.tables.surname_or_name_overrides.len() > MAX_NAME_OVERRIDES {
            let mut entries: Vec<(String, u64)> = self
                .tables
                .surname_or_name_overrides
                .iter()
                .map(|(k, v)| (k.clone(), v.updated_at))
                .collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            entries.truncate(MAX_NAME_OVERRIDES);
            let keep: HashSet<String> = entries.into_iter().map(|(k, _)| k).collect();
            self.tables
                .surname_or_name_overrides
                .retain(|k, _| keep.contains(k));
        }

        self.tables.name_override_words.clear();
        for rec in self.tables.surname_or_name_overrides.values() {
            self.tables.name_override_words.insert(rec.preferred.clone());
            self.tables.name_override_words.insert(rec.rejected.clone());
        }
        self.snapshot = Arc::new(self.tables.clone());
    }

    fn load_from_path(&mut self, path: &Path) {
        let fallback = appdata_fallback_path();
        let chosen_path = if is_under_program_files(path) {
            if let Some(ref fb) = fallback {
                if fb.is_file() || !path.is_file() {
                    fb.clone()
                } else {
                    path.to_path_buf()
                }
            } else {
                path.to_path_buf()
            }
        } else if !path.is_file() {
            if let Some(ref fb) = fallback {
                if fb.is_file() {
                    fb.clone()
                } else {
                    path.to_path_buf()
                }
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        self.file_path = Some(chosen_path.clone());
        self.last_check = Some(Instant::now());

        let content = match fs::read_to_string(&chosen_path) {
            Ok(s) => s,
            Err(_) => {
                self.last_mtime = None;
                return;
            }
        };
        let parsed: VariantPrefsFile = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        self.last_mtime = fs::metadata(&chosen_path).and_then(|m| m.modified()).ok();
        self.file_enabled = Some(parsed.enabled);
        self.file_preset = Some(VariantPreset::from_str(&parsed.preset));

        self.tables = VariantPrefsTables::default();
        for (k, rec) in parsed.pairs {
            if let Some((canon, def)) = find_doublet(&rec.preferred) {
                if canon == k && (rec.preferred == def.a || rec.preferred == def.b) {
                    self.tables.pairs.insert(k, rec);
                }
            }
        }
        for (k, rec) in parsed.surname_or_name_overrides {
            let parts: Vec<&str> = k.split('|').collect();
            if parts.len() != 3 {
                continue;
            }
            let (left, bpm, right) = (parts[0], parts[1], parts[2]);
            if bpm.trim().is_empty() || (left.is_empty() && right.is_empty()) {
                continue;
            }
            if (!left.is_empty() && !is_valid_cjk_len(left, 1, 1))
                || (!right.is_empty() && !is_valid_cjk_len(right, 1, 1))
            {
                continue;
            }
            let plen = rec.preferred.chars().count();
            if rec.preferred != rec.rejected
                && (1..=3).contains(&plen)
                && rec.rejected.chars().count() == plen
                && is_valid_cjk_len(&rec.preferred, 1, 3)
                && is_valid_cjk_len(&rec.rejected, 1, 3)
            {
                self.tables.surname_or_name_overrides.insert(k, rec);
            }
        }
        self.dirty = false;
        self.rebuild_snapshot();
    }

    fn write_json_atomic(target: &Path, json: &str) -> bool {
        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() && fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        let file_name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("variant_prefs.json");
        let tmp_path = target.with_file_name(format!("{}.tmp.{}", file_name, std::process::id()));
        if fs::write(&tmp_path, json.as_bytes()).is_err() {
            let _ = fs::remove_file(&tmp_path);
            return fs::write(target, json.as_bytes()).is_ok();
        }
        if fs::rename(&tmp_path, target).is_err() {
            let copied = fs::copy(&tmp_path, target).is_ok();
            let _ = fs::remove_file(&tmp_path);
            return copied;
        }
        true
    }

    fn flush_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(ref path) = self.file_path.clone() else {
            return;
        };

        let global = read_runtime_cfg();
        let (enabled, preset) = effective_config_for_state(Some(self), global);
        let file_obj = VariantPrefsFile {
            enabled,
            preset: preset.as_str().to_string(),
            pairs: self.tables.pairs.clone(),
            surname_or_name_overrides: self.tables.surname_or_name_overrides.clone(),
        };
        let Ok(json) = serde_json::to_string_pretty(&file_obj) else {
            return;
        };

        let mut final_path = path.clone();
        let wrote = if is_under_program_files(path) {
            false
        } else {
            Self::write_json_atomic(path, &json)
        };

        if !wrote {
            if let Some(fb) = appdata_fallback_path() {
                if Self::write_json_atomic(&fb, &json) {
                    final_path = fb;
                    self.file_path = Some(final_path.clone());
                } else {
                    return;
                }
            } else {
                return;
            }
        }

        self.last_mtime = fs::metadata(&final_path).and_then(|m| m.modified()).ok();
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

    fn record_doublet(&mut self, canon_key: String, preferred: &str, now_secs: u64) {
        let entry = self
            .tables
            .pairs
            .entry(canon_key)
            .or_insert_with(|| VariantPairRecord {
                preferred: preferred.to_string(),
                count: 0,
                updated_at: now_secs,
            });
        entry.preferred = preferred.to_string();
        entry.count = entry.count.saturating_add(1);
        entry.updated_at = now_secs;
        self.dirty = true;
        self.rebuild_snapshot();
    }

    fn record_correction(
        &mut self,
        bopomofo: &str,
        prev_ctx: &str,
        rejected: &str,
        accepted: &str,
        next_ctx: &str,
        now_secs: u64,
    ) {
        let global = read_runtime_cfg();
        let (enabled, preset) = effective_config_for_state(Some(self), global);
        if !enabled || preset == VariantPreset::Custom || rejected == accepted {
            return;
        }

        // 1. 直接比對多字異體雙寫詞組（例如：「台北」->「臺北」）
        if let Some((canon_key, _)) = find_doublet_pair(rejected, accepted) {
            self.record_doublet(canon_key, accepted, now_secs);
            self.flush_if_dirty();
            return;
        }

        // 2. 結合相鄰單字語境比對雙寫（例如：使用者於「台北市」中僅修改首字「台」->「臺」，next_ctx="北"）
        let left_ch = last_cjk_char(prev_ctx);
        let right_ch = first_cjk_char(next_ctx);
        if !right_ch.is_empty() {
            let rej_r = format!("{rejected}{right_ch}");
            let acc_r = format!("{accepted}{right_ch}");
            if let Some((canon_key, _)) = find_doublet_pair(&rej_r, &acc_r) {
                self.record_doublet(canon_key, &acc_r, now_secs);
                self.flush_if_dirty();
                return;
            }
        }
        if !left_ch.is_empty() {
            let rej_l = format!("{left_ch}{rejected}");
            let acc_l = format!("{left_ch}{accepted}");
            if let Some((canon_key, _)) = find_doublet_pair(&rej_l, &acc_l) {
                self.record_doublet(canon_key, &acc_l, now_secs);
                self.flush_if_dirty();
                return;
            }
        }

        // 3. 具注音與至少一側語境錨定之 1–3 字人名／姓氏選字或異體單字偏好（例如：「柳」->「劉」、「陳家」->「陳嘉」）
        //    僅針對姓氏後綴人名、姓氏異寫對、或單字異體圖形對自動學習，避免將一般文法／語音誤判混入本表
        let bpm = bopomofo.trim();
        let acc_len = accepted.chars().count();
        if bpm.is_empty()
            || (left_ch.is_empty() && right_ch.is_empty())
            || !(1..=3).contains(&acc_len)
            || rejected.chars().count() != acc_len
            || !is_valid_cjk_len(rejected, 1, 3)
            || !is_valid_cjk_len(accepted, 1, 3)
            || !is_onomastic_or_variant_context(&left_ch, rejected, accepted)
        {
            return;
        }

        let ov_key = format!("{left_ch}|{bpm}|{right_ch}");
        let entry = self
            .tables
            .surname_or_name_overrides
            .entry(ov_key)
            .or_insert_with(|| NameOverrideRecord {
                preferred: accepted.to_string(),
                rejected: rejected.to_string(),
                count: 0,
                updated_at: now_secs,
            });
        entry.preferred = accepted.to_string();
        entry.rejected = rejected.to_string();
        entry.count = entry.count.saturating_add(1);
        entry.updated_at = now_secs;
        self.dirty = true;
        self.rebuild_snapshot();
        self.flush_if_dirty();
    }

    fn record_sentence_span_diffs(
        &mut self,
        raw_sentence: &str,
        cor_sentence: &str,
        now_secs: u64,
    ) {
        let global = read_runtime_cfg();
        let (enabled, preset) = effective_config_for_state(Some(self), global);
        if !enabled || preset == VariantPreset::Custom {
            return;
        }

        let raw_chars: Vec<char> = raw_sentence.chars().collect();
        let cor_chars: Vec<char> = cor_sentence.chars().collect();
        if raw_chars.len() != cor_chars.len() || raw_chars == cor_chars {
            return;
        }

        let n = raw_chars.len();
        let mut mutated = false;
        let mut i = 0usize;
        while i < n {
            if raw_chars[i] == cor_chars[i] {
                i += 1;
                continue;
            }
            let mut matched_end: Option<usize> = None;
            'doublet_loop: for def in DOUBLETS {
                let len = def.a.chars().count();
                let min_start = i.saturating_sub(len - 1);
                let max_start = i.min(n.saturating_sub(len));
                for start in min_start..=max_start {
                    let end = start + len;
                    let raw_sub: String = raw_chars[start..end].iter().collect();
                    let cor_sub: String = cor_chars[start..end].iter().collect();
                    if (raw_sub == def.a && cor_sub == def.b)
                        || (raw_sub == def.b && cor_sub == def.a)
                    {
                        self.record_doublet(canonical_key(def.a, def.b), &cor_sub, now_secs);
                        mutated = true;
                        matched_end = Some(end);
                        break 'doublet_loop;
                    }
                }
            }
            i = matched_end.unwrap_or(i + 1);
        }

        if mutated {
            self.flush_if_dirty();
        }
    }
}

impl VariantPrefsSnapshot {
    #[inline]
    pub fn is_active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        self.preset == VariantPreset::OfficialTw
            || !self.tables.pairs.is_empty()
            || !self.tables.surname_or_name_overrides.is_empty()
    }

    #[inline]
    fn preferred_for_doublet(&self, canon_key: &str, def: &DoubletDef) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        if let Some(rec) = self.tables.pairs.get(canon_key) {
            if rec.preferred == def.a || rec.preferred == def.b {
                return Some(rec.preferred.as_str());
            }
        }
        match self.preset {
            VariantPreset::OfficialTw => Some(def.official_tw),
            VariantPreset::General | VariantPreset::Custom => None,
        }
    }

    #[inline]
    pub fn score_word(&self, word: &str) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let Some((canon_key, def)) = find_doublet(word) else {
            return 0.0;
        };
        let Some(pref) = self.preferred_for_doublet(&canon_key, def) else {
            return 0.0;
        };
        if word == pref {
            DOUBLETS_BONUS
        } else {
            -DOUBLETS_BONUS
        }
    }

    pub fn score_name_override_edge(
        &self,
        prev_word: &str,
        node: &LatticeNode,
        end_idx: usize,
        _n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        if !self.enabled
            || self.tables.surname_or_name_overrides.is_empty()
            || !self.tables.name_override_words.contains(&node.word)
        {
            return 0.0;
        }

        let left_ch = last_cjk_char(prev_word);
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

        let mut best_delta = 0.0f64;
        for (ov_key, rec) in &self.tables.surname_or_name_overrides {
            if node.word != rec.preferred && node.word != rec.rejected {
                continue;
            }
            let mut parts = ov_key.split('|');
            let (Some(k_left), Some(k_bpm), Some(k_right)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if parts.next().is_some() || k_bpm != node.bopomofo {
                continue;
            }
            if !k_left.is_empty() && k_left != left_ch {
                continue;
            }
            if !k_right.is_empty() && !cand_rights.iter().any(|r| r == k_right) {
                continue;
            }
            let delta = if node.word == rec.preferred {
                NAME_OVERRIDE_BONUS
            } else {
                -NAME_OVERRIDE_BONUS
            };
            if delta.abs() > best_delta.abs() {
                best_delta = delta;
            }
        }
        best_delta
    }

    fn find_name_override_replacement(
        &self,
        nodes: &[LatticeNode],
        idx: usize,
    ) -> Option<&str> {
        if self.tables.surname_or_name_overrides.is_empty()
            || !self.tables.name_override_words.contains(&nodes[idx].word)
        {
            return None;
        }
        let left_ch = if idx > 0 {
            last_cjk_char(&nodes[idx - 1].word)
        } else {
            String::new()
        };
        let right_ch = if idx + 1 < nodes.len() {
            first_cjk_char(&nodes[idx + 1].word)
        } else {
            String::new()
        };
        for (ov_key, rec) in &self.tables.surname_or_name_overrides {
            if nodes[idx].word != rec.rejected {
                continue;
            }
            let mut parts = ov_key.split('|');
            let (Some(k_left), Some(k_bpm), Some(k_right)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if parts.next().is_some() || k_bpm != nodes[idx].bopomofo {
                continue;
            }
            if (!k_left.is_empty() && k_left != left_ch)
                || (!k_right.is_empty() && k_right != right_ch)
            {
                continue;
            }
            return Some(rec.preferred.as_str());
        }
        None
    }

    pub fn apply_post_neural_protection_nodes(
        &self,
        mut nodes: Vec<LatticeNode>,
        nbest: &[(Vec<LatticeNode>, f64)],
    ) -> Vec<LatticeNode> {
        if !self.enabled || nodes.is_empty() {
            return nodes;
        }

        let mut target_words: Vec<String> = Vec::with_capacity(nodes.len());
        let mut any_violation = false;
        for idx in 0..nodes.len() {
            let cur_word = nodes[idx].word.as_str();
            if let Some((canon_key, def)) = find_doublet(cur_word) {
                if let Some(pref) = self.preferred_for_doublet(&canon_key, def) {
                    if cur_word != pref {
                        any_violation = true;
                        target_words.push(pref.to_string());
                        continue;
                    }
                }
            }
            if let Some(pref_name) = self.find_name_override_replacement(&nodes, idx) {
                any_violation = true;
                target_words.push(pref_name.to_string());
                continue;
            }
            target_words.push(cur_word.to_string());
        }

        if !any_violation {
            return nodes;
        }

        let target_sentence: String = target_words.iter().map(|s| s.as_str()).collect();
        let mut best_nbest_idx: Option<usize> = None;
        let mut best_nbest_score = f64::NEG_INFINITY;
        for (idx, (cand_nodes, base_sc)) in nbest.iter().enumerate() {
            let cand_sentence: String = cand_nodes.iter().map(|n| n.word.as_str()).collect();
            if cand_sentence == target_sentence && *base_sc > best_nbest_score {
                best_nbest_score = *base_sc;
                best_nbest_idx = Some(idx);
            }
        }

        if let Some(idx) = best_nbest_idx {
            return nbest[idx].0.clone();
        }

        for (node, tw) in nodes.iter_mut().zip(target_words.into_iter()) {
            if node.word != tw {
                node.word = tw;
            }
        }
        nodes
    }
}

pub fn snapshot_if_active(key: usize) -> Option<VariantPrefsSnapshot> {
    let global = read_runtime_cfg();
    let needs_reload = {
        let map = store().read().ok()?;
        if let Some(st) = map.get(&key) {
            if !st.needs_reload_check() {
                let (enabled, preset) = effective_config_for_state(Some(st), global);
                let snap = VariantPrefsSnapshot {
                    enabled,
                    preset,
                    tables: Arc::clone(&st.snapshot),
                };
                return if snap.is_active() { Some(snap) } else { None };
            }
            true
        } else {
            let snap = VariantPrefsSnapshot {
                enabled: global.enabled,
                preset: global.preset,
                tables: Arc::new(VariantPrefsTables::default()),
            };
            return if snap.is_active() { Some(snap) } else { None };
        }
    };

    if needs_reload {
        if let Ok(mut map) = store().write() {
            if let Some(st) = map.get_mut(&key) {
                st.maybe_reload_if_newer();
                let (enabled, preset) = effective_config_for_state(Some(st), global);
                let snap = VariantPrefsSnapshot {
                    enabled,
                    preset,
                    tables: Arc::clone(&st.snapshot),
                };
                return if snap.is_active() { Some(snap) } else { None };
            }
        }
    }
    None
}

pub fn score_word_for_key(key: usize, word: &str) -> f64 {
    snapshot_if_active(key).map_or(0.0, |snap| snap.score_word(word))
}

pub fn apply_post_neural_protection(
    key: usize,
    nodes: Vec<LatticeNode>,
    nbest: &[(Vec<LatticeNode>, f64)],
) -> Vec<LatticeNode> {
    match snapshot_if_active(key) {
        Some(snap) => snap.apply_post_neural_protection_nodes(nodes, nbest),
        None => nodes,
    }
}

pub fn bind_path_for_key(key: usize, path: &Path) {
    if let Ok(mut map) = store().write() {
        ensure_capacity(&mut map, key);
        let st = map.entry(key).or_default();
        st.load_from_path(path);
    }
}

pub fn is_key_bound(key: usize) -> bool {
    store()
        .read()
        .ok()
        .and_then(|map| map.get(&key).map(|st| st.file_path.is_some()))
        .unwrap_or(false)
}

pub fn flush_if_dirty_for_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(st) = map.get_mut(&key) {
            st.flush_if_dirty();
        }
    }
}

pub fn clear_for_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(st) = map.get_mut(&key) {
            st.tables = VariantPrefsTables::default();
            st.snapshot = Arc::new(VariantPrefsTables::default());
            st.dirty = true;
            st.flush_if_dirty();
        }
    }
}

pub fn remove_key(key: usize) {
    if let Ok(mut map) = store().write() {
        if let Some(mut st) = map.remove(&key) {
            st.flush_if_dirty();
        }
    }
}

pub fn record_correction_for_key(
    key: usize,
    bopomofo: &str,
    prev_ctx: &str,
    rejected: &str,
    accepted: &str,
    next_ctx: &str,
    now_secs: u64,
) {
    if let Ok(mut map) = store().write() {
        ensure_capacity(&mut map, key);
        let st = map.entry(key).or_default();
        st.record_correction(bopomofo, prev_ctx, rejected, accepted, next_ctx, now_secs);
    }
}

pub fn record_sentence_span_diffs_for_key(
    key: usize,
    raw_s: &str,
    cor_s: &str,
    now_secs: u64,
) {
    if let Ok(mut map) = store().write() {
        ensure_capacity(&mut map, key);
        let st = map.entry(key).or_default();
        st.record_sentence_span_diffs(raw_s, cor_s, now_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_nodes(words: &[&str]) -> Vec<LatticeNode> {
        let mut pos = 0usize;
        words
            .iter()
            .map(|&w| {
                let len = w.chars().count().max(1);
                let n = LatticeNode::new(pos, pos + len, w, 300, "");
                pos += len;
                n
            })
            .collect()
    }

    #[test]
    fn test_general_baseline_returns_zero_when_empty() {
        let key = 0xDEAD_1001usize;
        remove_key(key);
        set_key_config(key, true, VariantPreset::General);

        assert!(snapshot_if_active(key).is_none());
        assert_eq!(score_word_for_key(key, "台北"), 0.0);
        assert_eq!(score_word_for_key(key, "臺北"), 0.0);
        assert_eq!(score_word_for_key(key, "公佈"), 0.0);
        remove_key(key);
    }

    #[test]
    fn test_official_tw_preset_and_user_override_precedence() {
        let key = 0xDEAD_1002usize;
        remove_key(key);
        set_key_config(key, true, VariantPreset::OfficialTw);

        assert_eq!(score_word_for_key(key, "臺北"), 24.0);
        assert_eq!(score_word_for_key(key, "台北"), -24.0);
        assert_eq!(score_word_for_key(key, "臺灣"), 24.0);
        assert_eq!(score_word_for_key(key, "公佈"), 24.0);
        assert_eq!(score_word_for_key(key, "公布"), -24.0);
        assert_eq!(score_word_for_key(key, "瞭解"), 24.0);
        assert_eq!(score_word_for_key(key, "部分"), 24.0);

        // 若使用者手動將「公佈」改回「公布」，個人 pairs 應覆寫預設公文書偏好
        record_correction_for_key(key, "", "將", "公佈", "公布", "", 1_700_000_010);
        assert_eq!(score_word_for_key(key, "公布"), 24.0);
        assert_eq!(score_word_for_key(key, "公佈"), -24.0);
        // 其餘未覆寫者仍維持 OfficialTw
        assert_eq!(score_word_for_key(key, "臺北"), 24.0);
        remove_key(key);
    }

    #[test]
    fn test_learning_persistence_roundtrip_and_disable_toggle() {
        let key1 = 0xDEAD_1003usize;
        let key2 = 0xDEAD_1004usize;
        remove_key(key1);
        remove_key(key2);

        let dir = std::env::temp_dir().join(format!("ruhi_variant_prefs_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("variant_prefs.json");

        bind_path_for_key(key1, &json_path);
        set_key_config(key1, true, VariantPreset::General);

        // 以單字選字（於「台北市」首字改為「臺」）亦須自動辨識「台北|臺北」
        record_correction_for_key(key1, "ㄊㄞˊ", "在", "台", "臺", "北", 1_700_000_100);
        assert!(json_path.is_file(), "variant_prefs.json must be flushed to disk");
        assert_eq!(score_word_for_key(key1, "臺北"), 24.0);
        assert_eq!(score_word_for_key(key1, "台北"), -24.0);

        // 停用開關時應立刻回歸 0.0
        set_key_config(key1, false, VariantPreset::General);
        assert_eq!(score_word_for_key(key1, "臺北"), 0.0);
        assert_eq!(score_word_for_key(key1, "台北"), 0.0);

        // 重新載入驗證持久化資料
        bind_path_for_key(key2, &json_path);
        set_key_config(key2, true, VariantPreset::General);
        assert_eq!(score_word_for_key(key2, "臺北"), 24.0);
        assert_eq!(score_word_for_key(key2, "台北"), -24.0);

        remove_key(key1);
        remove_key(key2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_post_neural_protection_and_surname_override() {
        let key = 0xDEAD_1005usize;
        remove_key(key);
        set_key_config(key, true, VariantPreset::General);

        record_correction_for_key(key, "", "去", "台北", "臺北", "", 1_700_000_200);
        record_correction_for_key(key, "ㄌㄧㄡˊ", "", "柳", "劉", "先", 1_700_000_205);

        let neural_chosen = mk_nodes(&["我", "去", "台北"]);
        let nbest_alt = mk_nodes(&["我", "去", "臺北"]);
        let protected = apply_post_neural_protection(
            key,
            neural_chosen,
            &[(nbest_alt.clone(), 42.0)],
        );
        let text: String = protected.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(text, "我去臺北");

        // 即使 nbest 中無完全匹配，亦須直接就地改寫節點字詞同時保留其餘詞彙
        let neural_only = mk_nodes(&["我", "住", "台北"]);
        let rewritten = apply_post_neural_protection(key, neural_only, &[]);
        let text2: String = rewritten.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(text2, "我住臺北");

        // 人名／姓氏保護僅於相符注音＋左右語境時生效
        let mut bad_name = vec![
            LatticeNode::new(0, 1, "柳", 300, "ㄌㄧㄡˊ"),
            LatticeNode::new(1, 3, "先生", 400, "ㄒㄧㄢ ㄕㄥ"),
        ];
        bad_name = apply_post_neural_protection(key, bad_name, &[]);
        assert_eq!(bad_name[0].word, "劉");

        remove_key(key);
    }

    #[test]
    fn test_sentence_span_diff_learning() {
        let key = 0xDEAD_1006usize;
        remove_key(key);
        set_key_config(key, true, VariantPreset::General);

        record_sentence_span_diffs_for_key(key, "我們在台灣生活", "我們在臺灣生活", 1_700_000_300);
        assert_eq!(score_word_for_key(key, "臺灣"), 24.0);
        assert_eq!(score_word_for_key(key, "台灣"), -24.0);

        remove_key(key);
    }
}
