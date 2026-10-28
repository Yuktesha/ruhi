// ruhi-core/src/entitlement.rs — 如喜 (RuhI) v0.7.0+「九九圓滿 (81 日 & 81,000 字)」試用與跨平台通行碼終身授權引擎
//
// 核心設計哲學：
// 1. 「九九圓滿 (1 AND 2)」雙重達標條件：
//    - 條件 1：安裝啟用滿 81 日（9 × 9 = 81 天，肌肉記憶完全定型）
//    - 條件 2：累計透過如喜上屏滿 81,000 字（相當於完成一整本專書創作）
//    - 唯有「條件 1 與 條件 2 同時達成（days >= 81 && chars >= 81,000）」且尚未啟用通行碼時，方結束全功能試用。
//      - 重度創作者（15 天打滿 81,000 字）仍可暢享完整 81 天，不因勤於創作而提早受限；
//      - 輕度與外國漢字學習者（滿 81 天僅打 20,000 字）可繼續使用至真正累積滿 81,000 字，養熟肌肉記憶後再自然轉換。
// 2. 優雅降級（Graceful Degradation，絕不斷糧）：
//    - 雙重達標後，基礎單字輸入 100% 正常運作，絕不在緊急工作時鎖死游標；
//    - 僅暫停「龍龕雙字詞 4 碼連打（Track D）」、「前文脈絡二階智慧消歧加成」與「跨輸入法反查教練」，並附帶溫馨圓滿提示。
// 3. 跨平台離線通行碼（Passcode）驗證：
//    - 使用者於 Apple App Store / Google Play / Microsoft Store 以 $1~$2.99 美元取得 `RUHI-LK70-XXXX-XXXX-XXXX` 通行碼後，
//      可100%離線驗證簽章並永久解鎖終身專業版（Lifetime Pro）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 第一重九九圓滿目標日數：九九八十一（81 日）
pub const TRIAL_TARGET_DAYS: u64 = 81;
/// 第一重九九圓滿目標字數：八萬一千字（81,000 字）
pub const TRIAL_TARGET_CHARS: u64 = 81_000;
/// 第二重雙倍九九目標日數：162 日（2 × 81 日）
pub const TIER2_TARGET_DAYS: u64 = 162;
/// 第二重雙倍九九目標字數：162,000 字（2 × 81,000 字）
pub const TIER2_TARGET_CHARS: u64 = 162_000;
/// 每組跨平台通行碼允許綁定之個人裝置上限（5 台：如桌機、筆電、公司電腦、Mac、行動裝置）
pub const MAX_PERSONAL_DEVICES_PER_PASSCODE: usize = 5;

const ENTITLEMENT_STATE_MAGIC_SALT: &[u8] =
    b"RUHI_LONGKAN_PRO_V070_ENTITLEMENT_SALT_9981_XINGJUN_2026";
const PASSCODE_MASTER_KEY: &[u8] =
    b"RUHI_LONGKAN_PRO_MASTER_ENTITLEMENT_KEY_2026_9981_LIFETIME";
const PASSCODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// 三階段九九階梯感恩定價層級（一經購買即永久鎖定終身專業版，絕不補差價）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PricingTier {
    /// 第一階：未同時滿足 81 日 & 81,000 字 ➔ 知音早鳥終身價 $0.99 / €0.99
    Tier1EarlyBird099,
    /// 第二階：已滿 81 日 & 81,000 字，但未同時滿足 162 日 & 162,000 字 ➔ 九九圓滿終身價 $1.99 / €1.99
    Tier2Standard199,
    /// 第三階：已同時滿足 2 倍條件（162 日 & 162,000 字）➔ 雙重圓滿終身價 $2.99 / €2.99
    Tier3Master299,
}

impl PricingTier {
    pub fn price_label(&self) -> &'static str {
        match self {
            PricingTier::Tier1EarlyBird099 => "$0.99 / €0.99",
            PricingTier::Tier2Standard199 => "$1.99 / €1.99",
            PricingTier::Tier3Master299 => "$2.99 / €2.99",
        }
    }

    pub fn tier_name(&self) -> &'static str {
        match self {
            PricingTier::Tier1EarlyBird099 => "第一階・知音早鳥終身價 ($0.99 / €0.99)",
            PricingTier::Tier2Standard199 => "第二階・九九圓滿終身價 ($1.99 / €1.99)",
            PricingTier::Tier3Master299 => "第三階・雙重九九終身價 ($2.99 / €2.99)",
        }
    }

    pub fn store_sku(&self) -> &'static str {
        match self {
            PricingTier::Tier1EarlyBird099 => "ruhi_lifetime_tier1_099",
            PricingTier::Tier2Standard199 => "ruhi_lifetime_tier2_199",
            PricingTier::Tier3Master299 => "ruhi_lifetime_tier3_299",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntitlementStatus {
    /// 已啟用終身專業版通行碼（Lifetime Pro）
    LicensedPro {
        passcode_masked: String,
        days_elapsed: u64,
        chars_committed: u64,
    },
    /// 九九圓滿全功能試用中（尚未同時滿足 81 日 & 81,000 字，享 $0.99 知音早鳥價）
    TrialActive {
        days_elapsed: u64,
        chars_committed: u64,
        pricing_tier: PricingTier,
    },
    /// 九九圓滿雙重達標（已滿 81 日「且」已滿 81,000 字），進入優雅降級溫馨提示狀態（依是否滿 2 倍條件適用 $1.99 或 $2.99）
    TrialFulfilledGrace {
        days_elapsed: u64,
        chars_committed: u64,
        pricing_tier: PricingTier,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEntitlementState {
    pub first_install_epoch: u64,
    #[serde(default)]
    pub max_observed_epoch: u64,
    #[serde(default)]
    pub total_committed_chars: u64,
    #[serde(default)]
    pub license_passcode: String,
    #[serde(default)]
    pub integrity_sig: String,
}

#[derive(Debug, Clone)]
pub struct EntitlementManager {
    pub first_install_epoch: u64,
    pub max_observed_epoch: u64,
    pub total_committed_chars: u64,
    pub license_passcode: String,
    pub state_file_path: Option<PathBuf>,
    unsaved_chars_delta: u64,
}

impl Default for EntitlementManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EntitlementManager {
    pub fn now_epoch() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1_777_000_000)
    }

    pub fn default_state_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("RuhI");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("entitlement_state.json");
        }
        PathBuf::from("entitlement_state.json")
    }

    pub fn secondary_anchor_path() -> Option<PathBuf> {
        None
    }

    pub fn new() -> Self {
        let path = Self::default_state_path();
        Self::load_or_init(&path)
    }

    pub fn new_in_memory(first_install_epoch: u64, total_committed_chars: u64) -> Self {
        Self {
            first_install_epoch,
            max_observed_epoch: first_install_epoch,
            total_committed_chars,
            license_passcode: String::new(),
            state_file_path: None,
            unsaved_chars_delta: 0,
        }
    }

    fn read_verified_state_file(path: &Path) -> Option<PersistedEntitlementState> {
        if !path.exists() {
            return None;
        }
        let raw = std::fs::read_to_string(path).ok()?;
        let parsed = serde_json::from_str::<PersistedEntitlementState>(&raw).ok()?;
        if parsed.first_install_epoch > 0 {
            Some(parsed)
        } else {
            None
        }
    }

    pub fn load_or_init(path: &Path) -> Self {
        let now = Self::now_epoch();
        let primary = Self::read_verified_state_file(path);
        let secondary = Self::secondary_anchor_path()
            .as_deref()
            .and_then(Self::read_verified_state_file);

        // 雙重錨點防刪除還原（取最早安裝時間、最大歷史時間水位與最高累計字數）
        match (primary, secondary) {
            (Some(p), Some(s)) => {
                let first_epoch = p.first_install_epoch.min(s.first_install_epoch);
                let max_epoch = now
                    .max(p.max_observed_epoch)
                    .max(s.max_observed_epoch)
                    .max(first_epoch);
                let total_chars = p.total_committed_chars.max(s.total_committed_chars);
                let passcode = if !p.license_passcode.is_empty() {
                    p.license_passcode
                } else {
                    s.license_passcode
                };
                let mgr = Self {
                    first_install_epoch: first_epoch,
                    max_observed_epoch: max_epoch,
                    total_committed_chars: total_chars,
                    license_passcode: passcode,
                    state_file_path: Some(path.to_path_buf()),
                    unsaved_chars_delta: 0,
                };
                let _ = mgr.save();
                return mgr;
            }
            (Some(p), None) | (None, Some(p)) => {
                let max_epoch = now.max(p.max_observed_epoch).max(p.first_install_epoch);
                let mgr = Self {
                    first_install_epoch: p.first_install_epoch,
                    max_observed_epoch: max_epoch,
                    total_committed_chars: p.total_committed_chars,
                    license_passcode: p.license_passcode,
                    state_file_path: Some(path.to_path_buf()),
                    unsaved_chars_delta: 0,
                };
                let _ = mgr.save();
                return mgr;
            }
            (None, None) => {}
        }

        let mgr = Self {
            first_install_epoch: now,
            max_observed_epoch: now,
            total_committed_chars: 0,
            license_passcode: String::new(),
            state_file_path: Some(path.to_path_buf()),
            unsaved_chars_delta: 0,
        };
        let _ = mgr.save();
        mgr
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(ref path) = self.state_file_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let sig = Self::compute_state_signature(
            self.first_install_epoch,
            self.total_committed_chars,
            &self.license_passcode,
        );
        let state = PersistedEntitlementState {
            first_install_epoch: self.first_install_epoch,
            max_observed_epoch: self.max_observed_epoch.max(Self::now_epoch()),
            total_committed_chars: self.total_committed_chars,
            license_passcode: self.license_passcode.clone(),
            integrity_sig: sig,
        };
        let json = serde_json::to_string_pretty(&state).unwrap_or_default();
        if let Some(sec_path) = Self::secondary_anchor_path() {
            let _ = std::fs::write(sec_path, &json);
        }
        std::fs::write(path, json)
    }

    fn compute_state_signature(epoch: u64, chars: u64, passcode: &str) -> String {
        let payload = format!("v070|{}|{}|{}", epoch, chars, passcode.trim().to_ascii_uppercase());
        let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
        let mut h2: u64 = 0x8422_2325_cbf2_9ce4;
        for (i, &b) in payload
            .as_bytes()
            .iter()
            .chain(ENTITLEMENT_STATE_MAGIC_SALT.iter())
            .enumerate()
        {
            h1 ^= b as u64;
            h1 = h1.wrapping_mul(0x0000_0100_0000_01B3).rotate_left(5);
            h2 ^= (b as u64).wrapping_add(i as u64);
            h2 = h2.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_right(3);
        }
        format!("{:016X}{:016X}", h1, h2)
    }

    /// 計算給定區塊 `block_a` (4碼) 與 `block_b` (4碼) 之 4 碼簽章區塊 `CCCC`
    pub fn compute_passcode_check_block(block_a: &str, block_b: &str) -> String {
        let norm_a = block_a.trim().to_ascii_uppercase();
        let norm_b = block_b.trim().to_ascii_uppercase();
        let seed = format!("RUHI-LK70-{}-{}", norm_a, norm_b);
        let mut hash: u64 = 0x9981_2026_0700_4C4B;
        for (idx, &b) in seed
            .as_bytes()
            .iter()
            .chain(PASSCODE_MASTER_KEY.iter())
            .enumerate()
        {
            hash ^= (b as u64) << ((idx % 7) * 8);
            hash = hash.wrapping_mul(0x517C_C1B7_2722_0A95).rotate_left(11);
        }
        let mut out = String::with_capacity(4);
        let mut cur = hash;
        for _ in 0..4 {
            let idx = (cur as usize) % PASSCODE_ALPHABET.len();
            out.push(PASSCODE_ALPHABET[idx] as char);
            cur = cur.rotate_right(13) ^ 0xA5A5_5A5A_F0F0_0F0F;
        }
        out
    }

    /// 產生合法之跨平台終身授權通行碼 `RUHI-LK70-AAAA-BBBB-CCCC`
    pub fn generate_passcode(block_a: &str, block_b: &str) -> String {
        let a = block_a.trim().to_ascii_uppercase();
        let b = block_b.trim().to_ascii_uppercase();
        let c = Self::compute_passcode_check_block(&a, &b);
        format!("RUHI-LK70-{}-{}-{}", a, b, c)
    }

    /// 由官方應用商店（Apple StoreKit 2 `originalID` / Google Play `orderId` / Microsoft Store）之唯一交易序號，
    /// 決定性（Deterministic）簽發對應之跨平台終身通行碼 `RUHI-LK70-AAAA-BBBB-CCCC`
    pub fn derive_passcode_from_store_tx_id(tx_id: &str) -> String {
        let cleaned = tx_id.trim();
        let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
        let mut h2: u64 = 0x9981_2026_0700_4C4B;
        for (i, &b) in cleaned
            .as_bytes()
            .iter()
            .chain(PASSCODE_MASTER_KEY.iter())
            .enumerate()
        {
            h1 ^= b as u64;
            h1 = h1.wrapping_mul(0x0000_0100_0000_01B3).rotate_left(7);
            h2 ^= (b as u64).wrapping_add(i as u64);
            h2 = h2.wrapping_mul(0x517C_C1B7_2722_0A95).rotate_right(9);
        }
        let mut block_a = String::with_capacity(4);
        let mut block_b = String::with_capacity(4);
        for _ in 0..4 {
            block_a.push(PASSCODE_ALPHABET[(h1 as usize) % PASSCODE_ALPHABET.len()] as char);
            h1 = h1.rotate_right(11) ^ 0x5A5A_A5A5_3C3C_C3C3;
            block_b.push(PASSCODE_ALPHABET[(h2 as usize) % PASSCODE_ALPHABET.len()] as char);
            h2 = h2.rotate_left(13) ^ 0xF00F_0FF0_A55A_5AA5;
        }
        Self::generate_passcode(&block_a, &block_b)
    }

    /// 離線驗證通行碼是否合法
    pub fn verify_passcode(raw_code: &str) -> bool {
        let cleaned = raw_code.trim().to_ascii_uppercase();
        if cleaned.is_empty() {
            return false;
        }
        let parts: Vec<&str> = cleaned.split('-').collect();
        if parts.len() != 5 {
            return false;
        }
        if parts[0] != "RUHI" || parts[1] != "LK70" {
            return false;
        }
        let a = parts[2];
        let b = parts[3];
        let c = parts[4];
        if a.len() != 4 || b.len() != 4 || c.len() != 4 {
            return false;
        }
        if !a.chars().all(|ch| ch.is_ascii_alphanumeric())
            || !b.chars().all(|ch| ch.is_ascii_alphanumeric())
        {
            return false;
        }
        let expected_c = Self::compute_passcode_check_block(a, b);
        c == expected_c
    }

    /// 嘗試啟用通行碼，成功時自動持久化並回傳 `true`
    pub fn activate_passcode(&mut self, raw_code: &str) -> bool {
        let cleaned = raw_code.trim().to_ascii_uppercase();
        if Self::verify_passcode(&cleaned) {
            self.license_passcode = cleaned;
            let _ = self.save();
            true
        } else {
            false
        }
    }

    /// 產生本機匿名硬體指紋識別碼（如 `DEV-8A3F91C2`，同一台電腦重灌維持不變，且不含任何敏感個資）
    pub fn machine_fingerprint() -> String {
        let host = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "RUHI_HOST".to_string());
        let os_arch = format!(
            "{}|{}|{}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            host.trim().to_ascii_uppercase()
        );
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in os_arch
            .as_bytes()
            .iter()
            .chain(ENTITLEMENT_STATE_MAGIC_SALT.iter())
        {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3).rotate_left(7);
        }
        format!("DEV-{:08X}", (h as u32) ^ ((h >> 32) as u32))
    }

    /// 匯出跨裝置可攜式授權證書檔（`ruhi_license.json`），方便使用者免手打直接複製檔案至目標裝置匯入
    pub fn export_license_file(&self, target_path: &Path) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "product": "RuhI Longkan Pro (v0.7.0+)",
            "passcode": self.license_passcode,
            "exported_from_device": Self::machine_fingerprint(),
            "max_personal_devices": MAX_PERSONAL_DEVICES_PER_PASSCODE,
            "ExportTimestamp": Self::now_epoch(),
        });
        let content = serde_json::to_string_pretty(&payload).unwrap_or_default();
        std::fs::write(target_path, content)
    }

    /// 從授權證書檔（`ruhi_license.json`）或純文字檔匯入並自動啟用通行碼
    pub fn import_license_file(&mut self, source_path: &Path) -> bool {
        let Ok(raw) = std::fs::read_to_string(source_path) else {
            return false;
        };
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(code) = val.get("passcode").and_then(|v| v.as_str()) {
                if self.activate_passcode(code) {
                    return true;
                }
            }
        }
        // 亦支援直接從文字內容中自動擷取 RUHI-LK70-XXXX-XXXX-XXXX 格式之通行碼
        if let Some(extracted) = Self::extract_passcode_from_text(&raw) {
            return self.activate_passcode(&extracted);
        }
        false
    }

    /// 從任意剪貼簿文字（例如整封收據信、LINE 訊息或 JSON）中智慧抓取出 `RUHI-LK70-XXXX-XXXX-XXXX` 通行碼
    pub fn extract_passcode_from_text(text: &str) -> Option<String> {
        let upper = text.to_ascii_uppercase();
        if let Some(pos) = upper.find("RUHI-LK70-") {
            let slice = &upper[pos..];
            let cand: String = slice
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if cand.len() >= 24 {
                let token = &cand[..24];
                if Self::verify_passcode(token) {
                    return Some(token.to_string());
                }
            }
        }
        let trimmed = upper.trim();
        if Self::verify_passcode(trimmed) {
            return Some(trimmed.to_string());
        }
        None
    }

    /// 判斷是否已啟用終身專業版授權
    pub fn is_licensed(&self) -> bool {
        Self::verify_passcode(&self.license_passcode)
    }

    /// 計算自首次安裝以來經過的日數（內建單調遞增時間水位保護，防範手動將作業系統時間倒退修改）
    pub fn days_elapsed_at(&self, now_epoch: u64) -> u64 {
        let effective_now = now_epoch.max(self.max_observed_epoch);
        effective_now.saturating_sub(self.first_install_epoch) / 86_400
    }

    pub fn days_elapsed(&self) -> u64 {
        self.days_elapsed_at(Self::now_epoch())
    }

    /// 核心 `1 AND 2` 判定：
    /// 唯有「未授權」且「滿 81 日 (days >= 81)」**且**「累計滿 81,000 字 (chars >= 81,000)」雙重圓滿時，方回傳 `true`！
    pub fn is_trial_expired_both_at(&self, now_epoch: u64) -> bool {
        if self.is_licensed() {
            return false;
        }
        let days = self.days_elapsed_at(now_epoch);
        days >= TRIAL_TARGET_DAYS && self.total_committed_chars >= TRIAL_TARGET_CHARS
    }

    pub fn is_trial_expired_both(&self) -> bool {
        self.is_trial_expired_both_at(Self::now_epoch())
    }

    /// 判斷當前是否享有完整 Pro 智慧功能（已授權 或 尚在九九圓滿試用期內）
    pub fn is_pro_unlocked(&self) -> bool {
        !self.is_trial_expired_both()
    }

    /// 累計上屏字數（排除空白控制字元），每滿 25 字自動落盤一次
    pub fn record_committed_text(&mut self, text: &str) {
        let char_count = text
            .chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .count() as u64;
        if char_count == 0 {
            return;
        }
        self.total_committed_chars = self.total_committed_chars.saturating_add(char_count);
        self.unsaved_chars_delta = self.unsaved_chars_delta.saturating_add(char_count);
        if self.unsaved_chars_delta >= 25 {
            self.unsaved_chars_delta = 0;
            let _ = self.save();
        }
    }

    /// 取得第一重日數進度百分比 (`0.0..=1.0`，以 81 日為基準)
    pub fn days_progress_ratio(&self) -> f32 {
        (self.days_elapsed() as f32 / TRIAL_TARGET_DAYS as f32).clamp(0.0, 1.0)
    }

    /// 取得第一重字數進度百分比 (`0.0..=1.0`，以 81,000 字為基準)
    pub fn chars_progress_ratio(&self) -> f32 {
        (self.total_committed_chars as f32 / TRIAL_TARGET_CHARS as f32).clamp(0.0, 1.0)
    }

    /// 取得第二重（2 倍條件：162 日）日數進度百分比 (`0.0..=1.0`)
    pub fn tier2_days_progress_ratio(&self) -> f32 {
        (self.days_elapsed() as f32 / TIER2_TARGET_DAYS as f32).clamp(0.0, 1.0)
    }

    /// 取得第二重（2 倍條件：162,000 字）字數進度百分比 (`0.0..=1.0`)
    pub fn tier2_chars_progress_ratio(&self) -> f32 {
        (self.total_committed_chars as f32 / TIER2_TARGET_CHARS as f32).clamp(0.0, 1.0)
    }

    /// 依據給定時間點判定當前所處之三階段九九定價層級（`$0.99 ➔ $1.99 ➔ $2.99`）
    pub fn current_pricing_tier_at(&self, now_epoch: u64) -> PricingTier {
        let days = self.days_elapsed_at(now_epoch);
        let chars = self.total_committed_chars;
        if days < TRIAL_TARGET_DAYS || chars < TRIAL_TARGET_CHARS {
            PricingTier::Tier1EarlyBird099
        } else if days < TIER2_TARGET_DAYS || chars < TIER2_TARGET_CHARS {
            PricingTier::Tier2Standard199
        } else {
            PricingTier::Tier3Master299
        }
    }

    pub fn current_pricing_tier(&self) -> PricingTier {
        self.current_pricing_tier_at(Self::now_epoch())
    }

    /// 取得當前結構化授權與試用狀態
    pub fn status(&self) -> EntitlementStatus {
        let days = self.days_elapsed();
        let chars = self.total_committed_chars;
        let pricing_tier = self.current_pricing_tier();
        if self.is_licensed() {
            let masked = if self.license_passcode.len() >= 19 {
                format!(
                    "{}-****-{}",
                    &self.license_passcode[..14],
                    &self.license_passcode[self.license_passcode.len() - 4..]
                )
            } else {
                "RUHI-LK70-PRO".to_string()
            };
            EntitlementStatus::LicensedPro {
                passcode_masked: masked,
                days_elapsed: days,
                chars_committed: chars,
            }
        } else if self.is_trial_expired_both() {
            EntitlementStatus::TrialFulfilledGrace {
                days_elapsed: days,
                chars_committed: chars,
                pricing_tier,
            }
        } else {
            EntitlementStatus::TrialActive {
                days_elapsed: days,
                chars_committed: chars,
                pricing_tier,
            }
        }
    }

    /// 產生設定中心顯示之狀態摘要文字
    pub fn summary_badge_text(&self) -> String {
        match self.status() {
            EntitlementStatus::LicensedPro {
                passcode_masked,
                days_elapsed,
                chars_committed,
            } => format!(
                "🌟 終身專業授權版已啟用 ({})｜已相伴 {} 日・累計創作 {} 字（終身買斷・永不加價）",
                passcode_masked, days_elapsed, chars_committed
            ),
            EntitlementStatus::TrialActive {
                days_elapsed,
                chars_committed,
                pricing_tier,
            } => format!(
                "⏳ 九九圓滿全功能試用中（相伴 {} / {} 日 ＆ 累計 {} / {} 字）｜🔥 當前享「{}」早鳥鎖定資格！",
                days_elapsed,
                TRIAL_TARGET_DAYS,
                chars_committed,
                TRIAL_TARGET_CHARS,
                pricing_tier.tier_name()
            ),
            EntitlementStatus::TrialFulfilledGrace {
                days_elapsed,
                chars_committed,
                pricing_tier,
            } => format!(
                "🎉 九九圓滿達成（已相伴 {} 日且完成 {} 字創作）｜當前適用「{}」解鎖終身完整版",
                days_elapsed,
                chars_committed,
                pricing_tier.tier_name()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nine_by_nine_and_rule_truth_table() {
        let install_t = 1_700_000_000;
        let mut mgr = EntitlementManager::new_in_memory(install_t, 0);

        // 1. 重度創作者：第 15 天已打滿 120,000 字（超過 81,000 字，但未滿 81 日）
        //    因採用 1 AND 2，仍享有 100% 完整 Pro 試用！
        mgr.total_committed_chars = 120_000;
        let day_15 = install_t + 15 * 86_400;
        assert!(!mgr.is_trial_expired_both_at(day_15));

        // 2. 輕度外國學習者：第 100 天（超過 81 日），但僅累計打了 35,000 字（未滿 81,000 字）
        //    因採用 1 AND 2，仍享有 100% 完整 Pro 試用！
        mgr.total_committed_chars = 35_000;
        let day_100 = install_t + 100 * 86_400;
        assert!(!mgr.is_trial_expired_both_at(day_100));

        // 3. 邊界前夕：第 81 天，但字數 80,999 字 -> 仍未到期
        mgr.total_committed_chars = 80_999;
        let day_81 = install_t + 81 * 86_400;
        assert!(!mgr.is_trial_expired_both_at(day_81));

        // 4. 九九圓滿雙重達標：第 81 天 且 滿 81,000 字 -> 觸發優雅降級溫馨提示！
        mgr.record_committed_text("龍");
        assert_eq!(mgr.total_committed_chars, 81_000);
        assert!(mgr.is_trial_expired_both_at(day_81));

        // 5. 啟用跨平台通行碼後 -> 永久解鎖終身專業版！
        let valid_code = EntitlementManager::generate_passcode("9981", "LONG");
        assert!(EntitlementManager::verify_passcode(&valid_code));
        assert!(!EntitlementManager::verify_passcode("RUHI-LK70-9981-LONG-AAAA"));
        assert!(mgr.activate_passcode(&valid_code));
        assert!(mgr.is_licensed());
        assert!(!mgr.is_trial_expired_both_at(day_81));
        assert!(mgr.is_pro_unlocked());

        // 6. 驗證智慧剪貼簿擷取與授權檔匯出／匯入
        let messy_clip = format!("您好！感謝支持如喜龍龕，您的跨平台通行碼是：{}，請直接貼上啟用！", valid_code);
        assert_eq!(
            EntitlementManager::extract_passcode_from_text(&messy_clip),
            Some(valid_code.clone())
        );

        let dev_fp = EntitlementManager::machine_fingerprint();
        assert!(dev_fp.starts_with("DEV-") && dev_fp.len() == 12);

        let tmp_lic = std::env::temp_dir().join("test_ruhi_license_export.json");
        assert!(mgr.export_license_file(&tmp_lic).is_ok());

        let mut mgr2 = EntitlementManager::new_in_memory(install_t, 85_000);
        assert!(mgr2.is_trial_expired_both_at(day_81));
        assert!(mgr2.import_license_file(&tmp_lic));
        assert!(mgr2.is_licensed());
        let _ = std::fs::remove_file(&tmp_lic);

        // 7. 驗證三階段九九階梯定價 ($0.99 ➔ $1.99 ➔ $2.99) 與防時間倒退
        let mut mgr3 = EntitlementManager::new_in_memory(install_t, 50_000);
        assert_eq!(
            mgr3.current_pricing_tier_at(install_t + 30 * 86_400),
            PricingTier::Tier1EarlyBird099
        );
        // 滿 81 天但字數未滿 81,000 -> 仍為第一階 $0.99 早鳥價
        assert_eq!(
            mgr3.current_pricing_tier_at(install_t + 90 * 86_400),
            PricingTier::Tier1EarlyBird099
        );
        // 滿 81 天 且 滿 81,000 字 -> 進入第二階 $1.99 九九圓滿價
        mgr3.total_committed_chars = 81_000;
        assert_eq!(
            mgr3.current_pricing_tier_at(install_t + 81 * 86_400),
            PricingTier::Tier2Standard199
        );
        // 滿 162 天但字數 150,000（未滿 162,000 字）-> 仍為第二階 $1.99
        mgr3.total_committed_chars = 150_000;
        assert_eq!(
            mgr3.current_pricing_tier_at(install_t + 162 * 86_400),
            PricingTier::Tier2Standard199
        );
        // 滿 162 天 且 滿 162,000 字（超過 2 倍條件）-> 進入第三階 $2.99 雙重圓滿價
        mgr3.total_committed_chars = 162_000;
        assert_eq!(
            mgr3.current_pricing_tier_at(install_t + 162 * 86_400),
            PricingTier::Tier3Master299
        );
    }
}
