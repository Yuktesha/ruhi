//! # `telemetry_sync` — 如喜偏好設定中心輕量隱私安全過濾與雲端詞庫同步模組
//!
//! 本模組為 `ruhi-settings` 專屬之純淨化輕量邏輯，徹底解除對 `ruhi-core` 與核心大詞庫的載入依賴：
//! 1. 啟發式個資過濾閘門 (`HeuristicPiiFilter`)：嚴格過濾電話、身分證、電子郵件、信用卡與詳細門牌；
//! 2. Tailcat 社群規則包沙盒守門員 (`import_tailcat_package`)：
//!    - 嚴格攔截單字無上下文權重修改 (`UNIGRAM_WEIGHT`)；
//!    - 阻斷針對核心封閉類虛詞之負向懲罰；
//!    - 規則加權強制夾制於 `[-35.0, +35.0]`；
//!    - 複合詞改寫入個人詞庫 `user_dict.tsv`，徹底淘汰舊版 `lexicon_domain.tsv`；
//! 3. 雲端試算表 Webhook 匿名改字整句回傳與條件式 ETag 低頻更新。

use chrono::Local;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const PRONOUN_CHARS: &str = "我你您他她它咱";
const PARTICLE_CHARS: &str = "的得地了著嗎呢吧啊呀喔哦";
const CONTAINER_SUFFIX_CHARS: &str = "框欄區鈕鍵窗頁表列項";

/// 驗證單一字元是否落於 CJK 統一表意文字區間
#[inline]
pub fn is_cjk_char(ch: char) -> bool {
    let code = ch as u32;
    (0x4E00..=0x9FFF).contains(&code)
        || (0x3400..=0x4DBF).contains(&code)
        || (0xF900..=0xFAFF).contains(&code)
}

/// 驗證是否為合法純 CJK 漢字詞元
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

/// 判別字串是否屬於跨句法短語（不可寫入詞庫）
pub fn is_syntactic_phrase_or_clitic(w: &str) -> bool {
    let chars: Vec<char> = w.trim().chars().collect();
    if chars.len() < 2 {
        return true;
    }
    let allowed_particles = [
        "的士", "的確", "標的", "目的", "有的", "是的", "好的", "真的", "了解", "理解", "了不起",
        "了結", "了得", "了當", "了然", "未了", "知了", "著急", "著名", "著作", "著手", "著重",
        "著陸", "著迷", "著想", "著實", "顯著", "執著", "原著", "名著", "地道", "地步", "地區",
        "地方", "地面", "地球", "地址", "地鐵", "地圖", "地位", "地獄", "地質", "地震", "地鼠",
        "土地", "場地", "質地", "天地", "得到", "得知", "得意", "得力", "得體", "得罪", "得失",
        "心得", "難得", "覺得", "記得", "值得", "獲得", "取得", "贏得", "捨不得", "怪不得", "不得",
        "不見得", "酒吧", "網吧", "貼吧", "吧檯", "吧台", "喇嘛", "幹嘛", "嗎啡", "呢喃",
    ];
    let c0 = chars[0];
    let c1 = chars[1];
    let clast = *chars.last().unwrap_or(&c0);
    if (PARTICLE_CHARS.contains(c0)
        || PARTICLE_CHARS.contains(c1)
        || PARTICLE_CHARS.contains(clast))
        && !allowed_particles.contains(&w)
    {
        return true;
    }
    if chars.len() == 2 {
        if PRONOUN_CHARS.contains(c0)
            && "不沒很都也還就要會能想在再先去來有是把被讓給對跟和向的了過著嗎呢吧".contains(c1)
        {
            return true;
        }
        if "給請讓叫替幫跟對向從把被為由比在的了".contains(c0) && PRONOUN_CHARS.contains(c1) {
            return true;
        }
        if "會都也還先又就才只常總更最很太".contains(c0) && (c1 == '在' || c1 == '再') {
            return true;
        }
        return false;
    }
    let allowed_long = [
        "各階段", "現階段", "下階段", "這時候", "那時候", "全自動", "全方位", "全天候", "更何況",
        "總而言之", "換言之", "簡言之", "反之亦然",
    ];
    if allowed_long.contains(&w) {
        return false;
    }
    if PRONOUN_CHARS.contains(c0)
        && !["我們", "你們", "他們", "她們", "它們", "咱們"]
            .iter()
            .any(|p| w.starts_with(p))
    {
        return true;
    }
    if "每各某本此該這那哪全凡而並且或但卻若雖因隨按依據由從向往對跟比為被把讓叫請給在再又也都還就才只剛正曾已將會要能可很太最更越"
        .contains(c0)
    {
        return true;
    }
    if chars.len() == 3
        && "圈點勾框篩挑複全單候".contains(chars[0])
        && "選取列".contains(chars[1])
        && CONTAINER_SUFFIX_CHARS.contains(chars[2])
    {
        return true;
    }
    false
}

/// 啟發式個資隱私過濾閘門
pub struct HeuristicPiiFilter;

/// 向下相容別名
pub type ZeroPiiPrivacyFilter = HeuristicPiiFilter;

impl HeuristicPiiFilter {
    /// 驗證單一詞元是否通過個資審查
    pub fn is_safe_token(token: &str) -> (bool, &'static str) {
        let s = token.trim();
        if s.is_empty() {
            return (false, "Empty token");
        }
        let char_len = s.chars().count();
        if char_len > 4 {
            return (
                false,
                "Token length > 4 (Potential Proper Noun / Address / Secret)",
            );
        }
        if s.chars().any(|c| c.is_ascii_digit()) {
            return (
                false,
                "Contains digits (Potential Phone / ID / Account Number)",
            );
        }
        if s.chars().any(|c| c.is_ascii_alphabetic() || "@._-:/\\".contains(c)) {
            return (
                false,
                "Contains ASCII/Symbols (Potential Email / Handle / Password)",
            );
        }
        if !is_valid_chinese_token(s) {
            return (false, "Non-CJK characters detected");
        }
        (true, "SAFE_PUBLIC_CONSTRUCT")
    }

    /// 驗證整句對照語料是否通過審查（攔截電話、身分證、電子郵件、信用卡號與詳細門牌地址）
    pub fn is_safe_sentence_pair(
        error_sentence: &str,
        correct_sentence: &str,
    ) -> (bool, &'static str) {
        let e = error_sentence.trim();
        let c = correct_sentence.trim();
        if e.is_empty() || c.is_empty() || e == c {
            return (false, "Empty or identical sentence pair");
        }
        for s in [e, c] {
            if s.contains('@') || s.contains("http://") || s.contains("https://") {
                return (false, "Contains Email or URL");
            }
            let mut consec_digits = 0usize;
            for ch in s.chars() {
                if ch.is_ascii_digit() || ('０'..='９').contains(&ch) {
                    consec_digits += 1;
                    if consec_digits >= 6 {
                        return (false, "Contains >=6 consecutive digits (Phone / ID / Account)");
                    }
                } else {
                    consec_digits = 0;
                }
            }
            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= 10 {
                for w in chars.windows(10) {
                    if w[0].is_ascii_alphabetic()
                        && matches!(w[1], '1' | '2' | '8' | '9')
                        && w[2..].iter().all(|ch| ch.is_ascii_digit())
                    {
                        return (false, "Contains Taiwan National ID pattern");
                    }
                }
            }
            let has_road = s.contains('路') || s.contains('街') || s.contains("大道");
            let has_number = s.contains('號')
                && (s.contains('巷')
                    || s.contains('弄')
                    || s.contains('段')
                    || s.contains('樓'));
            if has_road && has_number {
                return (false, "Contains detailed street address");
            }
        }
        (true, "PASSED_ZERO_PII_SENTENCE")
    }

    /// 淨化二元語法轉移規則
    pub fn sanitize_transition_rule(
        prev: &str,
        next: &str,
        score: f64,
        kind: &str,
    ) -> Option<Value> {
        let (ok1, _) = Self::is_safe_token(prev);
        let (ok2, _) = Self::is_safe_token(next);
        if !ok1 || !ok2 {
            return None;
        }
        let clamped_score = score.clamp(-35.0, 35.0);
        Some(json!({
            "type": "BIGRAM_TRANSITION",
            "prev": prev.trim(),
            "next": next.trim(),
            "recommended_delta": (clamped_score * 10.0).round() / 10.0,
            "kind": kind,
            "privacy_audit": "PASSED_ZERO_PII"
        }))
    }
}

/// 偏好設定中心輕量回饋與同步用戶端
#[derive(Debug, Clone)]
pub struct TelemetrySyncClient {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub revisions_json: PathBuf,
    pub user_tsv: PathBuf,
    pub user_dict_tsv: PathBuf,
    pub patterns_jsonl: PathBuf,
}

impl TelemetrySyncClient {
    /// 建立同步用戶端實體
    pub fn new(root_override: Option<&Path>) -> Self {
        let root = root_override
            .map(Path::to_path_buf)
            .unwrap_or_else(crate::config_sync::resolve_ruhi_root);
        let data_dir = crate::config_sync::resolve_data_dir(&root);
        let revisions_json = data_dir.join("pending_revisions.json");
        let user_tsv = data_dir.join("transitions_user.tsv");
        let user_dict_tsv = root.join("user_dict.tsv");
        let patterns_jsonl = data_dir.join("patterns_user.jsonl");

        Self {
            root,
            data_dir,
            revisions_json,
            user_tsv,
            user_dict_tsv,
            patterns_jsonl,
        }
    }

    /// 讀取本機尚未回傳之改字雙句紀錄
    pub fn load_local_pending_revisions(&self) -> Vec<Value> {
        let mut paths = vec![
            self.data_dir.join("pending_revisions.json"),
            self.revisions_json.clone(),
            self.root.join("scripts").join("data").join("pending_revisions.json"),
            self.root.join("data").join("pending_revisions.json"),
            self.root.join("dist").join("data").join("pending_revisions.json"),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            paths.push(
                PathBuf::from(appdata)
                    .join("Ruhi")
                    .join("data")
                    .join("pending_revisions.json"),
            );
        }
        let mut combined = Vec::new();
        for p in paths {
            if p.is_file() {
                if let Ok(raw) = fs::read_to_string(&p) {
                    if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&raw) {
                        for item in arr {
                            if !combined.contains(&item) {
                                combined.push(item);
                            }
                        }
                    }
                }
            }
        }
        combined
    }

    /// 清空本機待回傳改字紀錄
    pub fn clear_local_pending_revisions(&self) -> usize {
        let count = self.load_local_pending_revisions().len();
        let mut paths = vec![
            self.data_dir.join("pending_revisions.json"),
            self.revisions_json.clone(),
            self.root.join("scripts").join("data").join("pending_revisions.json"),
            self.root.join("data").join("pending_revisions.json"),
            self.root.join("dist").join("data").join("pending_revisions.json"),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            paths.push(
                PathBuf::from(appdata)
                    .join("Ruhi")
                    .join("data")
                    .join("pending_revisions.json"),
            );
        }
        for p in paths {
            if p.is_file() {
                let _ = fs::write(&p, "[]");
            }
            if let Some(parent) = p.parent() {
                let txt = parent.join("user_revision_pairs.txt");
                if txt.is_file() {
                    let _ = fs::write(&txt, "");
                }
            }
        }
        count
    }

    /// 將本機改字紀錄經個資審查後回傳至雲端試算表 Webhook
    pub fn upload_revisions_to_cloud_sheet(
        &self,
        webhook_url: &str,
        zero_pii_enabled: bool,
    ) -> Value {
        let resolved_url = crate::config_sync::resolve_cloud_sheet_webhook_url(webhook_url);
        let url = resolved_url.as_str();
        if url.is_empty() {
            return json!({
                "status": "not_configured",
                "message": "尚未設定雲端試算表 Webhook 網址，已略過"
            });
        }
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return json!({
                "status": "error",
                "message": "雲端資料表 Webhook 網址格式無效（須以 https:// 開頭）"
            });
        }

        let raw_revs = self.load_local_pending_revisions();
        if raw_revs.is_empty() {
            return json!({
                "status": "empty",
                "uploaded_count": 0,
                "filtered_pii_count": 0,
                "message": "目前本機無待回傳之改字紀錄。"
            });
        }

        let mut records_to_send = Vec::new();
        let mut seen_pairs = HashSet::new();
        let mut filtered_pii_count = 0usize;

        for item in &raw_revs {
            let rej = item.get("rejected").and_then(Value::as_str).unwrap_or("").trim();
            let acc = item.get("accepted").and_then(Value::as_str).unwrap_or("").trim();
            let prev_ctx = item.get("prev_context").and_then(Value::as_str).unwrap_or("").trim();
            let next_ctx = item.get("next_context").and_then(Value::as_str).unwrap_or("").trim();

            let err_s_owned = item
                .get("error_sentence")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if !rej.is_empty() && (!prev_ctx.is_empty() || !next_ctx.is_empty()) {
                        format!("{prev_ctx}{rej}{next_ctx}")
                    } else {
                        rej.to_string()
                    }
                });
            let cor_s_owned = item
                .get("correct_sentence")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if !acc.is_empty() && (!prev_ctx.is_empty() || !next_ctx.is_empty()) {
                        format!("{prev_ctx}{acc}{next_ctx}")
                    } else {
                        acc.to_string()
                    }
                });
            let err_s = err_s_owned.as_str();
            let cor_s = cor_s_owned.as_str();
            let bpmf = item.get("bopomofo").and_then(Value::as_str).unwrap_or("").trim();
            let ts = item.get("timestamp").and_then(Value::as_u64).unwrap_or(0);

            if err_s.is_empty() || cor_s.is_empty() || err_s == cor_s {
                continue;
            }
            if zero_pii_enabled {
                let (safe, _) = HeuristicPiiFilter::is_safe_sentence_pair(err_s, cor_s);
                if !safe {
                    filtered_pii_count += 1;
                    continue;
                }
            }
            if !seen_pairs.insert((err_s.to_string(), cor_s.to_string())) {
                continue;
            }
            records_to_send.push(json!({
                "timestamp": ts,
                "error_sentence": err_s,
                "correct_sentence": cor_s,
                "rejected": rej,
                "accepted": acc,
                "bopomofo": bpmf,
                "keyboard_mode": "hsu",
                "app_version": env!("CARGO_PKG_VERSION"),
            }));
        }

        if records_to_send.is_empty() {
            self.clear_local_pending_revisions();
            return json!({
                "status": "filtered_all",
                "uploaded_count": 0,
                "filtered_pii_count": filtered_pii_count,
                "message": format!("本機共 {} 筆紀錄，經個資過濾閘門篩除後無可回傳之一般語句。", raw_revs.len())
            });
        }

        if records_to_send.len() > 20 {
            let start_idx = records_to_send.len() - 20;
            records_to_send = records_to_send.split_off(start_idx);
        }

        let payload = json!({
            "protocol": "RUHI_CLOUD_REVISION_SHEET_V1",
            "app_version": env!("CARGO_PKG_VERSION"),
            "keyboard_mode": "hsu",
            "revisions": records_to_send,
            "records": records_to_send,
        });

        let Ok(resp) = crate::http_client::http_post_json(url, &payload, &[], 25) else {
            return json!({
                "status": "error",
                "message": "無法透過網路傳送資料，請確認連線。"
            });
        };

        let resp_text = resp.text().trim().to_string();
        if resp.status >= 200
            && resp.status < 300
            && (resp_text.contains("\"ok\"") || resp_text.contains("status"))
        {
            let uploaded_cnt = payload["records"].as_array().map(|a| a.len()).unwrap_or(0);
            self.clear_local_pending_revisions();
            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            json!({
                "status": "ok",
                "uploaded_count": uploaded_cnt,
                "filtered_pii_count": filtered_pii_count,
                "timestamp": now_ts,
                "message": format!("✅ 已成功回傳 {} 組改字雙句（過濾 {} 筆敏感資訊），已清空本機暫存！", uploaded_cnt, filtered_pii_count)
            })
        } else {
            json!({
                "status": "error",
                "uploaded_count": 0,
                "filtered_pii_count": filtered_pii_count,
                "message": format!("雲端回傳未完成（回應：{}），本機紀錄安全保留。", if resp_text.is_empty() { "連線逾時" } else { &resp_text })
            })
        }
    }

    /// 透過條件式 HTTP ETag 拉取社群規則庫
    pub fn pull_community_rules_with_etag(
        &mut self,
        feed_url: &str,
        force_refresh: bool,
    ) -> Value {
        let url = if feed_url.trim().is_empty() {
            crate::config_sync::DEFAULT_TAILCAT_FEED_URL
        } else {
            feed_url.trim()
        };
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return json!({
                "status": "error",
                "message": "請提供合法的 HTTPS/HTTP 規則庫網址"
            });
        }

        let state_path = self.data_dir.join("federation_sync_state.json");
        let mut state_map: Value = fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}));

        let cached_etag = if force_refresh {
            String::new()
        } else {
            state_map
                .get("etags")
                .and_then(|m| m.get(url))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };

        let mut req_headers = Vec::new();
        if !cached_etag.is_empty() {
            req_headers.push(("If-None-Match", cached_etag.as_str()));
        }

        let Ok(resp) = crate::http_client::http_get_with_headers(
            url,
            concat!("RuhI-Settings/", env!("CARGO_PKG_VERSION")),
            &req_headers,
            10,
        ) else {
            return json!({
                "status": "error",
                "message": "無法連線至遠端 Tailcat 規則庫"
            });
        };

        if resp.status == 304 {
            return json!({
                "status": "not_modified",
                "http_status": 304,
                "etag": cached_etag,
                "bytes_transferred": 0,
                "current_version": crate::config_sync::CURRENT_ENGINE_VERSION,
                "has_newer_engine_version": false,
                "message": "遠端規則與詞庫已是最新狀態（HTTP 304 Not Modified，零流量負擔）"
            });
        }

        let new_etag = resp.etag.clone();
        let body_part = resp.text();

        let Ok(mut pkg) = serde_json::from_str::<Value>(body_part.trim()) else {
            return json!({
                "status": "error",
                "message": "遠端回應內容非合法之 Tailcat JSON 規則包格式"
            });
        };

        let cur_schema = crate::config_sync::CURRENT_TAILCAT_SCHEMA_VERSION as u64;
        let remote_schema = pkg
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if remote_schema > cur_schema {
            let schema_key = cur_schema.to_string();
            if let Some(compat_url) = pkg
                .get("compat_feeds")
                .and_then(|cf| cf.get(&schema_key))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
            {
                if !compat_url.is_empty() && compat_url != url {
                    if let Ok(compat_resp) = crate::http_client::http_get(&compat_url, 10) {
                        let compat_body = compat_resp.text();
                        if let Ok(compat_pkg) = serde_json::from_str::<Value>(compat_body.trim()) {
                            pkg = compat_pkg;
                        }
                    }
                }
            }
        }

        let mut import_res = self.import_tailcat_package(&pkg);
        if let Some(intake_url) = pkg.get("telemetry_intake_url").and_then(Value::as_str) {
            let trimmed = intake_url.trim();
            if trimmed.starts_with("https://") {
                state_map["telemetry_intake_url"] = json!(trimmed);
            }
        }
        if !new_etag.is_empty() || state_map.get("telemetry_intake_url").is_some() {
            if state_map.get("etags").is_none() {
                state_map["etags"] = json!({});
            }
            if !new_etag.is_empty() {
                state_map["etags"][url] = json!(new_etag);
            }
            state_map["last_sync"] = json!(Local::now().to_rfc3339());
            let _ = fs::write(
                &state_path,
                serde_json::to_string_pretty(&state_map).unwrap_or_default(),
            );
        }

        if let Some(obj) = import_res.as_object_mut() {
            obj.insert("http_status".to_string(), json!(200));
            obj.insert("etag".to_string(), json!(new_etag));
            obj.insert("bytes_transferred".to_string(), json!(body_part.len()));
        }
        import_res
    }

    /// 解析版本號元組
    pub fn parse_semver_tuple(ver: &str) -> (u32, u32, u32) {
        let cleaned = ver.trim().trim_start_matches(['v', 'V']);
        let mut parts = cleaned.split('.').map(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0)
        });
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    }

    /// 比較候選版本號是否新於基準版本號
    pub fn is_semver_greater(candidate: &str, baseline: &str) -> bool {
        Self::parse_semver_tuple(candidate) > Self::parse_semver_tuple(baseline)
    }

    /// 匯入 Tailcat 規則包（含嚴格沙盒守門員檢驗）
    pub fn import_tailcat_package(&mut self, pkg: &Value) -> Value {
        let Some(rules) = pkg.get("rules").and_then(Value::as_array) else {
            return json!({
                "status": "error",
                "message": "Invalid Tailcat package format (missing 'rules' array)",
                "imported_bigrams": 0,
                "imported_compounds": 0,
                "imported_rejected_readings": 0,
                "imported_constructions": 0,
                "skipped_newer_version_rules": 0,
                "blocked_unigram_rules": 0,
                "blocked_unsafe_rules": 0
            });
        };

        let cur_engine_ver = crate::config_sync::CURRENT_ENGINE_VERSION;
        let cur_schema_ver = crate::config_sync::CURRENT_TAILCAT_SCHEMA_VERSION as u64;

        let protected_core_tokens: HashSet<&str> = [
            "的", "得", "地", "了", "在", "再", "不", "沒", "是", "有", "會", "要", "能", "可",
            "把", "被", "讓", "給", "對", "跟", "與", "和", "就", "才", "都", "也", "還", "但",
            "卻", "他", "她", "它", "我", "你", "您", "們",
        ]
        .into_iter()
        .collect();

        let mut valid_bigrams = Vec::new();
        let mut valid_compounds = Vec::new();
        let mut valid_rejected_readings = Vec::new();
        let mut valid_constructions = Vec::new();
        let mut skipped_newer_version = 0usize;
        let mut blocked_unigram = 0usize;
        let mut blocked_unsafe = 0usize;

        for r in rules {
            if let Some(min_v) = r.get("min_engine_version").and_then(Value::as_str) {
                if Self::is_semver_greater(min_v, cur_engine_ver) {
                    skipped_newer_version += 1;
                    continue;
                }
            }
            if let Some(rule_schema) = r.get("schema_version").and_then(Value::as_u64) {
                if rule_schema > cur_schema_ver {
                    skipped_newer_version += 1;
                    continue;
                }
            }

            let r_type = r
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("BIGRAM_TRANSITION");
            if r_type.eq_ignore_ascii_case("UNIGRAM_WEIGHT") {
                blocked_unigram += 1;
                continue;
            }
            if r_type.eq_ignore_ascii_case("COMPOUND_WORD")
                || r_type.eq_ignore_ascii_case("OOV_COMPOUND")
            {
                let word = r
                    .get("word")
                    .or_else(|| r.get("term"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                let (ok, _) = HeuristicPiiFilter::is_safe_token(word);
                if ok
                    && (2..=6).contains(&word.chars().count())
                    && !is_syntactic_phrase_or_clitic(word)
                {
                    valid_compounds.push(r.clone());
                } else {
                    blocked_unsafe += 1;
                }
                continue;
            }
            if r_type.eq_ignore_ascii_case("REJECTED_READING") {
                let word = r.get("word").and_then(Value::as_str).unwrap_or("").trim();
                let reading = r
                    .get("reading")
                    .or_else(|| r.get("bopomofo"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if !word.is_empty() && !reading.is_empty() {
                    valid_rejected_readings.push(r.clone());
                }
                continue;
            }
            if r_type.eq_ignore_ascii_case("PATTERN_CONSTRUCTION") {
                if r.get("pattern").is_some() || r.get("name").is_some() {
                    valid_constructions.push(r.clone());
                }
                continue;
            }
            if !r_type.eq_ignore_ascii_case("BIGRAM_TRANSITION") {
                skipped_newer_version += 1;
                continue;
            }

            let prev = r.get("prev").and_then(Value::as_str).unwrap_or("").trim();
            let next = r.get("next").and_then(Value::as_str).unwrap_or("").trim();
            let delta = r
                .get("recommended_delta")
                .or_else(|| r.get("score"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let kind = r
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("community_tailcat");

            let (ok1, _) = HeuristicPiiFilter::is_safe_token(prev);
            let (ok2, _) = HeuristicPiiFilter::is_safe_token(next);
            if !ok1 || !ok2 || delta.abs() < 1.0 {
                blocked_unsafe += 1;
                continue;
            }
            if delta < 0.0 && protected_core_tokens.contains(next) {
                blocked_unsafe += 1;
                continue;
            }
            let clamped = delta.clamp(-35.0, 35.0);
            valid_bigrams.push(json!({
                "prev": prev,
                "next": next,
                "score": (clamped * 10.0).round() / 10.0,
                "kind": kind,
                "comment": "Tailcat_Community_Verified"
            }));
        }

        let imported_bigrams = self.append_rules_to_tsv(&valid_bigrams);
        let imported_compounds = self.append_words_to_user_dict(&valid_compounds);
        let imported_rejected = self.append_rejected_readings_to_tsv(&valid_rejected_readings);
        let imported_constructions = self.append_patterns_to_jsonl(&valid_constructions);

        let latest_version = pkg
            .get("latest_version")
            .and_then(Value::as_str)
            .unwrap_or(cur_engine_ver);
        let has_newer_version = Self::is_semver_greater(latest_version, cur_engine_ver);
        let release_notes = pkg
            .get("release_notes")
            .and_then(Value::as_str)
            .unwrap_or("");
        let installer_url = pkg
            .get("installer_url")
            .and_then(Value::as_str)
            .unwrap_or("");
        let installer_sha256 = pkg
            .get("installer_sha256")
            .and_then(Value::as_str)
            .unwrap_or("");

        let mut msg = format!(
            "✅ Tailcat 同步完成：新增 {} 個自建詞彙、{} 條二階轉移、{} 條槽位構式、{} 條破音字黑名單。",
            imported_compounds, imported_bigrams, imported_constructions, imported_rejected
        );
        if skipped_newer_version > 0 {
            msg.push_str(&format!(
                "（另有 {} 條新規則需較新版本主程式支援，已自動為您優雅略過）",
                skipped_newer_version
            ));
        }

        json!({
            "status": "ok",
            "current_version": cur_engine_ver,
            "latest_version": latest_version,
            "has_newer_engine_version": has_newer_version,
            "release_notes": release_notes,
            "installer_url": installer_url,
            "installer_sha256": installer_sha256,
            "imported_bigrams": imported_bigrams,
            "imported_compounds": imported_compounds,
            "imported_rejected_readings": imported_rejected,
            "imported_constructions": imported_constructions,
            "skipped_newer_version_rules": skipped_newer_version,
            "blocked_unigram_rules": blocked_unigram,
            "blocked_unsafe_rules": blocked_unsafe,
            "message": msg
        })
    }

    /// 將詞彙寫入個人詞庫 `user_dict.tsv`（不重複寫入）
    fn append_words_to_user_dict(&self, words: &[Value]) -> usize {
        if words.is_empty() {
            return 0;
        }
        let target = &self.user_dict_tsv;
        let existing_raw = fs::read_to_string(target).unwrap_or_else(|_| {
            "# RuhI 使用者自建詞庫 (User Custom Lexicon)\n# 格式規範: WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE\n".to_string()
        });
        let mut existing_words = HashSet::new();
        for line in existing_raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = t.split('\t').collect();
            if !cols.is_empty() {
                existing_words.insert(cols[0].trim().to_string());
            }
        }

        let mut new_lines = Vec::new();
        for w in words {
            let word_str = w
                .get("word")
                .or_else(|| w.get("term"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let bpmf = w
                .get("bopomofo")
                .or_else(|| w.get("reading"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let weight = w
                .get("weight")
                .and_then(Value::as_i64)
                .unwrap_or(750)
                .clamp(500, 950);
            if word_str.is_empty() || existing_words.contains(word_str) {
                continue;
            }
            existing_words.insert(word_str.to_string());
            new_lines.push(format!("{word_str}\t{bpmf}\t{weight}\ttailcat\tCommunity-Verified"));
        }

        if new_lines.is_empty() {
            return 0;
        }

        let mut updated = existing_raw;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        for l in &new_lines {
            updated.push_str(l);
            updated.push('\n');
        }
        let _ = fs::write(target, &updated);
        new_lines.len()
    }

    /// 追加轉移規則至 `transitions_user.tsv`
    fn append_rules_to_tsv(&self, rules: &[Value]) -> usize {
        if rules.is_empty() {
            return 0;
        }
        let target = &self.user_tsv;
        let existing = fs::read_to_string(target).unwrap_or_else(|_| {
            "# RuhI 使用者自適應二階語法轉移表 (prev\tnext\tscore\tkind\tcomment)\n".to_string()
        });
        let mut existing_pairs = HashSet::new();
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = trimmed.split('\t').collect();
            if cols.len() >= 2 {
                existing_pairs.insert((cols[0].trim().to_string(), cols[1].trim().to_string()));
            }
        }

        let mut out = existing;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        let mut added = 0;
        for r in rules {
            let prev = r.get("prev").and_then(Value::as_str).unwrap_or("").trim();
            let next = r.get("next").and_then(Value::as_str).unwrap_or("").trim();
            let score = r.get("score").and_then(Value::as_f64).unwrap_or(26.0);
            let kind = r.get("kind").and_then(Value::as_str).unwrap_or("collocation");
            let cmt = r.get("comment").and_then(Value::as_str).unwrap_or("Tailcat");
            if prev.is_empty() || next.is_empty() {
                continue;
            }
            if !existing_pairs.contains(&(prev.to_string(), next.to_string())) {
                out.push_str(&format!("{prev}\t{next}\t{score:.1}\t{kind}\t{cmt}\n"));
                existing_pairs.insert((prev.to_string(), next.to_string()));
                added += 1;
            }
        }
        if added > 0 {
            let _ = fs::write(target, &out);
        }
        added
    }

    /// 追加破音字黑名單至 `rejected_readings.tsv`
    fn append_rejected_readings_to_tsv(&self, items: &[Value]) -> usize {
        if items.is_empty() {
            return 0;
        }
        let target_path = self.data_dir.join("rejected_readings.tsv");
        let existing_raw = fs::read_to_string(&target_path).unwrap_or_default();
        let mut existing_keys = HashSet::new();
        for line in existing_raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = t.split('\t').collect();
            if cols.len() >= 2 {
                existing_keys.insert((cols[0].trim().to_string(), cols[1].trim().to_string()));
            }
        }

        let mut new_lines = Vec::new();
        for item in items {
            let word = item.get("word").and_then(Value::as_str).unwrap_or("").trim();
            let reading = item
                .get("reading")
                .or_else(|| item.get("bopomofo"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let reason = item
                .get("reason")
                .or_else(|| item.get("comment"))
                .and_then(Value::as_str)
                .unwrap_or("Tailcat_Verified_Rejected_Reading")
                .trim();
            if word.is_empty() || reading.is_empty() {
                continue;
            }
            if existing_keys.insert((word.to_string(), reading.to_string())) {
                new_lines.push(format!("{word}\t{reading}\t{reason}"));
            }
        }

        if new_lines.is_empty() {
            return 0;
        }

        let mut updated = existing_raw;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        for l in &new_lines {
            updated.push_str(l);
            updated.push('\n');
        }
        let _ = fs::write(&target_path, &updated);
        new_lines.len()
    }

    /// 追加槽位構式至 `patterns_user.jsonl`（依 `pattern_id` 去重並夾制加權於 `[-35.0, 35.0]`）
    fn append_patterns_to_jsonl(&self, constructions: &[Value]) -> usize {
        if constructions.is_empty() {
            return 0;
        }
        let target = &self.patterns_jsonl;
        let existing = fs::read_to_string(target).unwrap_or_default();
        let mut seen_ids = HashSet::new();
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                if let Some(pid) = v.get("pattern_id").and_then(Value::as_str) {
                    if !pid.trim().is_empty() {
                        seen_ids.insert(pid.trim().to_string());
                    }
                }
                if let Some(name) = v.get("name").and_then(Value::as_str) {
                    if !name.trim().is_empty() {
                        seen_ids.insert(format!("name:{}", name.trim()));
                    }
                }
            }
        }

        let mut out = existing;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        let mut count = 0;
        for c in constructions {
            let pid = c
                .get("pattern_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let name = c
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if !pid.is_empty() && seen_ids.contains(pid) {
                continue;
            }
            if pid.is_empty() && !name.is_empty() && seen_ids.contains(&format!("name:{name}")) {
                continue;
            }
            let mut sanitized = c.clone();
            if let Some(obj) = sanitized.as_object_mut() {
                if let Some(b) = obj.get("bonus").and_then(Value::as_f64) {
                    obj.insert("bonus".to_string(), json!(b.clamp(-35.0, 35.0)));
                }
                if let Some(p) = obj.get("penalty").and_then(Value::as_f64) {
                    obj.insert("penalty".to_string(), json!(p.clamp(-35.0, 35.0)));
                }
            }
            if let Ok(line) = serde_json::to_string(&sanitized) {
                if !pid.is_empty() {
                    seen_ids.insert(pid.to_string());
                } else if !name.is_empty() {
                    seen_ids.insert(format!("name:{name}"));
                }
                out.push_str(&line);
                out.push('\n');
                count += 1;
            }
        }
        if count > 0 {
            let _ = fs::write(target, &out);
        }
        count
    }

    /// 處理主程式版本升級提示
    pub fn handle_tailcat_version_upgrade(&self, sync_result: &Value, update_mode: &str) -> Value {
        let has_newer = sync_result
            .get("has_newer_engine_version")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !has_newer {
            return json!({
                "upgrade_action": "none",
                "message": "目前已是最新主程式版本。"
            });
        }

        if update_mode == "lexicon_only" {
            return json!({
                "upgrade_action": "skipped_nostalgic_mode",
                "message": "已設定為『僅更新相容詞庫與規則』，已略過主程式版本升級。"
            });
        }

        let latest_ver = sync_result
            .get("latest_version")
            .and_then(Value::as_str)
            .unwrap_or("");
        let cur_ver = crate::config_sync::CURRENT_ENGINE_VERSION;
        let notes = sync_result
            .get("release_notes")
            .and_then(Value::as_str)
            .unwrap_or("包含最新語言學構式與穩定度提升。");
        let installer_url = sync_result
            .get("installer_url")
            .and_then(Value::as_str)
            .unwrap_or("");

        if installer_url.is_empty() || !installer_url.starts_with("https://") {
            return json!({
                "upgrade_action": "invalid_installer_url",
                "message": "遠端清單未提供有效之 HTTPS 安裝包網址。"
            });
        }

        let user_approved = if update_mode == "auto_install" {
            true
        } else {
            #[cfg(target_os = "windows")]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
                };
                let title_w: Vec<u16> = "RuhI 如喜輸入法 — 發現新版本更新 (Tailcat)\0"
                    .encode_utf16()
                    .collect();
                let body = format!(
                    "透過 Tailcat 頻道偵測到如喜輸入法新版本！\n\n• 目前版本：v{}\n• 最新版本：v{}\n• 更新內容：{}\n\n是否同意下載並安裝 v{} 主程式？",
                    cur_ver, latest_ver, notes, latest_ver
                );
                let body_w: Vec<u16> = body.encode_utf16().chain(Some(0)).collect();
                let ans = unsafe {
                    MessageBoxW(
                        std::ptr::null_mut(),
                        body_w.as_ptr(),
                        title_w.as_ptr(),
                        MB_YESNO | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND,
                    )
                };
                ans == IDYES
            }
            #[cfg(not(target_os = "windows"))]
            {
                false
            }
        };

        if !user_approved {
            return json!({
                "upgrade_action": "user_declined",
                "message": "使用者選擇暫不升級主程式，保留已熱更新之相容詞庫與規則。"
            });
        }

        let temp_dir = std::env::temp_dir();
        let installer_path = temp_dir.join(format!("RuhI_Setup_v{latest_ver}.exe"));
        if let Ok(resp) = crate::http_client::http_get(installer_url, 120) {
            if resp.status == 200
                && resp.body.len() > 64 * 1024
                && resp.body.starts_with(b"MZ")
                && fs::write(&installer_path, &resp.body).is_ok()
            {
                let _ = std::process::Command::new(&installer_path)
                    .args(["/SILENT", "/CLOSEAPPLICATIONS"])
                    .spawn();
                return json!({
                    "upgrade_action": "launched_installer",
                    "installer_path": installer_path.to_string_lossy(),
                    "message": format!("✅ 已啟動 RuhI v{} 自動升級程序！", latest_ver)
                });
            }
        }

        json!({
            "upgrade_action": "download_failed",
            "message": "下載安裝包失敗，請前往 GitHub Releases 手動下載。"
        })
    }

    /// 檢查並執行定時回傳與同步排程
    pub fn maybe_run_scheduled_upload(
        &mut self,
        settings: &mut crate::config_sync::RuhiSettings,
    ) -> Option<Value> {
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let interval_secs = match settings.ai_and_privacy.upload_schedule.as_str() {
            "daily" => 86_400u64,
            "weekly" => 604_800u64,
            _ => 86_400u64,
        };

        if settings.ai_and_privacy.tailcat_auto_sync
            && settings.ai_and_privacy.tailcat_update_mode != "manual"
            && now_ts.saturating_sub(settings.ai_and_privacy.last_tailcat_sync_timestamp)
                >= interval_secs
        {
            let feed_url = settings.ai_and_privacy.tailcat_feed_url.clone();
            let update_mode = settings.ai_and_privacy.tailcat_update_mode.clone();
            let sync_res = self.pull_community_rules_with_etag(&feed_url, false);
            if sync_res.get("status").and_then(Value::as_str) == Some("ok")
                || sync_res.get("status").and_then(Value::as_str) == Some("not_modified")
            {
                settings.ai_and_privacy.last_tailcat_sync_timestamp = now_ts;
                let _ = crate::config_sync::save_settings(&self.root, settings);
                let _ = self.handle_tailcat_version_upgrade(&sync_res, &update_mode);
            }
        }

        if !settings.ai_and_privacy.telemetry_consent {
            return None;
        }
        let upload_interval = match settings.ai_and_privacy.upload_schedule.as_str() {
            "daily" => 86_400u64,
            "weekly" => 604_800u64,
            _ => return None,
        };
        if now_ts.saturating_sub(settings.ai_and_privacy.last_upload_timestamp) < upload_interval {
            return None;
        }
        if self.load_local_pending_revisions().is_empty() {
            settings.ai_and_privacy.last_upload_timestamp = now_ts;
            let _ = crate::config_sync::save_settings(&self.root, settings);
            return None;
        }
        let res = self.upload_revisions_to_cloud_sheet(
            &settings.ai_and_privacy.cloud_sheet_webhook_url,
            settings.ai_and_privacy.zero_pii_gateway,
        );
        settings.ai_and_privacy.last_upload_timestamp = now_ts;
        let _ = crate::config_sync::save_settings(&self.root, settings);
        Some(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_pii_filter() {
        assert!(HeuristicPiiFilter::is_safe_token("不懂").0);
        assert!(!HeuristicPiiFilter::is_safe_token("0912345678").0);
        assert!(!HeuristicPiiFilter::is_safe_token("user@example.com").0);
        assert!(!HeuristicPiiFilter::is_safe_token("台北市信義區信義路五段").0);

        let (safe, _) = HeuristicPiiFilter::is_safe_sentence_pair("測試句子", "測試句子修改");
        assert!(safe);

        let (unsafe_pair, _) = HeuristicPiiFilter::is_safe_sentence_pair(
            "我的電話是0912345678",
            "我的電話是0912345678喔",
        );
        assert!(!unsafe_pair);
    }

    #[test]
    fn test_import_tailcat_package_sandbox() {
        let sandbox_dir = std::env::temp_dir().join(format!("ruhi_test_telemetry_{}", std::process::id()));
        let _ = fs::create_dir_all(&sandbox_dir);
        let mut client = TelemetrySyncClient::new(Some(&sandbox_dir));

        let test_pkg = json!({
            "rules": [
                {
                    "type": "UNIGRAM_WEIGHT",
                    "word": "理",
                    "weight": 9999
                },
                {
                    "type": "COMPOUND_WORD",
                    "word": "新架構",
                    "bopomofo": "ㄒㄧㄣ ㄐㄧㄚˋ ㄍㄡˋ",
                    "weight": 780
                },
                {
                    "type": "BIGRAM_TRANSITION",
                    "prev": "歡迎",
                    "next": "新架構",
                    "recommended_delta": 28.0
                }
            ]
        });

        let res = client.import_tailcat_package(&test_pkg);
        assert_eq!(res["status"], "ok");
        assert_eq!(res["blocked_unigram_rules"], 1);
        assert_eq!(res["imported_compounds"], 1);
        assert_eq!(res["imported_bigrams"], 1);

        assert!(client.user_dict_tsv.is_file());
        let dict_content = fs::read_to_string(&client.user_dict_tsv).unwrap_or_default();
        assert!(dict_content.contains("新架構"));

        let _ = fs::remove_dir_all(&sandbox_dir);
    }
}
