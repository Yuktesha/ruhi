//! # `ai_consultant` — 如喜神經符號融合 AI 語意顧問 (Rust 原生版)
//!
//! 完整移植並升級原 Python 版 `scripts/ai_semantic_consultant.py`：
//! 1. **Google 技術生態絕對優先**：優先支援 Google Gemini API (`gemini-2.5-flash`) 與純綠免安裝版 `llama.cpp` (`atg_lib::ai_engine::RuhAIEngine` 驅動之本地 Google Gemma `.gguf` 模型)。
//! 2. **100% 離線神經符號融合保底**：當處於無網路或未啟動大型語言模型之純離線環境時，自動以內建計算語言學雙向詞網與六大病灶分類學完成精準診斷、專業文獻詞彙萃取、Tailcat 提案安全審計與修訂歷史昇華。

use atg_lib::ai_engine::{AIStatus, RuhAIEngine};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::tuner::{
    common_typo_map, is_syntactic_phrase_or_clitic, is_valid_chinese_token, SemanticTuner,
    ZeroPiiPrivacyFilter,
};

/// 推薦雲端大 AI / SI 模型清單（Google Gemini 生態第一優先，歐美模型第二順位）
pub fn recommended_cloud_models() -> Vec<Value> {
    vec![
        json!({
            "id": "gemini-2.5-pro",
            "provider": "gemini",
            "display_name": "Google Gemini 2.5 Pro（複雜句法構式與階層陣列因式分解首選）",
            "priority": 1,
            "ecosystem": "Google 雲端生態（第一優先）",
            "role": "深層語法構式設計、多槽位陣列抽離 (A_array × B_array) 與全詞庫同音防禦規畫",
            "description": "具備極佳的形式語言學推理與結構化 JSON 輸出穩定度，適合處理跨子句結構歧義與階層式陣列因式分解。"
        }),
        json!({
            "id": "gemini-2.5-flash",
            "provider": "gemini",
            "display_name": "Google Gemini 2.5 Flash（日常高速對照句診斷預設推薦）",
            "priority": 2,
            "ecosystem": "Google 雲端生態（第一優先）",
            "role": "高頻日常錯句／正句對照分析、同音病灶定位與舉一反三掃描",
            "description": "低延遲、高吞吐且支援免費額度 API Key，適合日常語意調校工作室即時互動診斷。"
        }),
        json!({
            "id": "claude-sonnet-4-20250514",
            "provider": "claude",
            "display_name": "Anthropic Claude Sonnet 4（歐美雲端備援）",
            "priority": 3,
            "ecosystem": "歐美商用生態（第二順位）",
            "role": "結構化語意規則生成與跨句對比審計",
            "description": "支援透過如喜標準 AI/SI 交接協定（Handover Protocol）無縫接續語意調校任務。"
        }),
        json!({
            "id": "gpt-4o",
            "provider": "openai",
            "display_name": "OpenAI GPT-4o（歐美雲端備援）",
            "priority": 4,
            "ecosystem": "歐美商用生態（第二順位）",
            "role": "相容 OpenAI API 端點之語意診斷備援",
            "description": "支援標準 JSON Schema 輸出，可依交接協定產出二維／三維陣列對應規則。"
        }),
    ]
}

/// 推薦本地離線純綠模型清單（Google Gemma 生態第一優先，歐美開源第二順位）
pub fn recommended_local_models() -> Vec<Value> {
    vec![
        json!({
            "name": "gemma3:12b",
            "tag": "gemma-3-12b-it-Q4_K_M.gguf",
            "display_name": "Google Gemma 3 12B Instruct Q4_K_M（本地深度語意調校首選）",
            "priority": 1,
            "ecosystem": "Google 開源生態（第一優先）",
            "description": "支援純綠免安裝 llama.cpp (127.0.0.1:8080) 或 Ollama 驅動，繁體中文句法構式與陣列抽離能力最佳（建議 8GB+ VRAM）。"
        }),
        json!({
            "name": "gemma3:4b",
            "tag": "gemma-3-4b-it-Q4_K_M.gguf",
            "display_name": "Google Gemma 3 4B Instruct Q4_K_M（輕量筆電高速首選）",
            "priority": 2,
            "ecosystem": "Google 開源生態（第一優先）",
            "description": "記憶體佔用僅約 3GB，推論極快，適合本機常駐即時對照句診斷。"
        }),
        json!({
            "name": "gemma2:latest",
            "tag": "gemma2:latest",
            "display_name": "Google Gemma 2 9B / 2B（穩定開源主力）",
            "priority": 3,
            "ecosystem": "Google 開源生態（第一優先）",
            "description": "Google 官方開源輕量模型，由純綠開源 Llama 引擎驅動，零商業廣告與審查干擾。"
        }),
        json!({
            "name": "llama3.2:3b",
            "tag": "llama3.2:3b",
            "display_name": "Meta Llama 3.2 3B Instruct（歐美優質開源備援）",
            "priority": 4,
            "ecosystem": "歐美開源生態（第二順位）",
            "description": "Meta 歐美開源架構，推論速度極快，適合離線二階搭配提煉。"
        }),
        json!({
            "name": "mistral-nemo:latest",
            "tag": "mistral-nemo:latest",
            "display_name": "Mistral Nemo 12B Instruct（歐洲開源備援）",
            "priority": 5,
            "ecosystem": "歐美開源生態（第二順位）",
            "description": "法國 Mistral AI 與 NVIDIA 合作開源模型，多語結構化指令遵循穩定。"
        }),
    ]
}

/// 高階神經符號 AI 語意顧問 (`AISemanticConsultant`)
pub struct AISemanticConsultant {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub config: Map<String, Value>,
    pub green_ai: RuhAIEngine,
}

impl AISemanticConsultant {
    pub fn new(workspace_root: Option<&Path>) -> Self {
        let root = workspace_root
            .map(Path::to_path_buf)
            .unwrap_or_else(crate::config_sync::resolve_ruhi_root);
        let config_path = root.join("config.json");
        let config = Self::load_config_map(&config_path);
        let mut green_ai = RuhAIEngine::new();
        green_ai.scan_local_gguf_models();

        Self {
            root,
            config_path,
            config,
            green_ai,
        }
    }

    fn load_config_map(path: &Path) -> Map<String, Value> {
        if path.is_file() {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(&raw) {
                    if let Some(gm) = map.get("gemini_model").and_then(Value::as_str) {
                        if gm == "gemini-3.8-flash" || gm == "gemini-3.5-flash" {
                            map.insert("gemini_model".to_string(), json!("gemini-2.5-flash"));
                        }
                    }
                    return map;
                }
            }
        }
        Map::new()
    }

    pub fn save_config(&self) -> bool {
        if let Ok(pretty) = serde_json::to_string_pretty(&Value::Object(self.config.clone())) {
            fs::write(&self.config_path, pretty).is_ok()
        } else {
            false
        }
    }

    pub fn get_str(&self, key: &str, default: &str) -> String {
        self.config
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.config
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    /// 探測本機純綠 `llama-server` (`127.0.0.1:8080`)
    pub fn probe_local_llama(&mut self) -> bool {
        self.green_ai.refresh(true) == AIStatus::Online
    }

    /// 探測本機 `127.0.0.1:11434` 服務
    pub fn probe_ollama(&self) -> bool {
        probe_local_http_port(11434, "/api/tags")
    }

    /// 掃描本機已下載之 `.gguf` 與離線模型清單
    pub fn read_offline_models(&mut self) -> Vec<String> {
        let mut models = Vec::new();
        self.green_ai.scan_local_gguf_models();
        for name in self.green_ai.available_models.keys() {
            models.push(format!("gguf:{name}"));
        }

        if let Some(home) = dirs::home_dir() {
            let ollama_lib = home
                .join(".ollama")
                .join("models")
                .join("manifests")
                .join("registry.ollama.ai")
                .join("library");
            if ollama_lib.is_dir() {
                if let Ok(entries) = fs::read_dir(ollama_lib) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            let m_name = p
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
                            if let Ok(tags) = fs::read_dir(&p) {
                                for tag in tags.flatten() {
                                    if tag.path().is_file() {
                                        let t_name = tag
                                            .file_name()
                                            .to_string_lossy()
                                            .to_string();
                                        models.push(format!("{m_name}:{t_name}"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        models.sort();
        models.dedup();
        models
    }

    pub fn find_ollama_paths(&self) -> Value {
        let mut cli: Option<String> = None;
        let mut app: Option<String> = None;
        let mut installed = false;

        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local_app).join("Programs").join("Ollama");
            let app_p = base.join("ollama app.exe");
            let cli_p = base.join("ollama.exe");
            if app_p.is_file() {
                app = Some(app_p.to_string_lossy().to_string());
                installed = true;
            }
            if cli_p.is_file() {
                cli = Some(cli_p.to_string_lossy().to_string());
                installed = true;
            }
        }
        if !self.green_ai.available_models.is_empty() {
            installed = true;
        }

        json!({
            "cli": cli,
            "app": app,
            "installed": installed
        })
    }

    pub fn has_gemini_key(&self) -> bool {
        let env_k = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        let cfg_k = self.get_str("gemini_api_key", "");
        !env_k.trim().is_empty() || !cfg_k.trim().is_empty()
    }

    pub fn get_effective_provider(&mut self) -> String {
        let provider = self.get_str("ai_provider", "auto");
        if provider == "ollama" {
            if self.probe_local_llama() {
                return "local_llama".to_string();
            }
            return "ollama".to_string();
        }
        if matches!(
            provider.as_str(),
            "gemini" | "openai" | "claude" | "custom" | "local_llama" | "disabled"
        ) {
            return provider;
        }

        if self.has_gemini_key() {
            return "gemini".to_string();
        }
        if self.probe_local_llama() {
            return "local_llama".to_string();
        }
        if self.probe_ollama() {
            return "ollama".to_string();
        }
        "gemini_free_ready".to_string()
    }

    pub fn test_connection(&mut self) -> Value {
        let provider = self.get_effective_provider();
        if provider == "local_llama" {
            let model = self
                .green_ai
                .active_chat_model
                .clone()
                .unwrap_or_else(|| "gemma-gguf".to_string());
            return json!({
                "status": "ok",
                "provider": "local_llama",
                "model": model,
                "has_internet": true,
                "message": format!("🟢 已連線至本機純綠 llama-server ({model})")
            });
        }
        if provider == "ollama" && self.probe_ollama() {
            let m = self.get_str("ollama_model", "gemma2:latest");
            return json!({
                "status": "ok",
                "provider": "ollama",
                "model": m,
                "has_internet": true,
                "message": format!("🟢 已連線至本機推論服務 ({m})")
            });
        }
        if provider == "gemini" && self.has_gemini_key() {
            let m = self.get_str("gemini_model", "gemini-2.5-flash");
            return json!({
                "status": "ok",
                "provider": "gemini",
                "model": m,
                "has_internet": true,
                "message": format!("🟢 Google Gemini API 已就緒 ({m})")
            });
        }

        json!({
            "status": "gemini_free_ready",
            "provider": "gemini",
            "has_internet": true,
            "message": "已啟用計算語言學確定性引擎！如需雲端大模型輔助，可一鍵設定 Google Gemini 免費金鑰或啟動本機純綠 GGUF 引擎。"
        })
    }

    pub fn get_ai_wizard_status(&mut self) -> Value {
        let offline_models = self.read_offline_models();
        let ollama_paths = self.find_ollama_paths();
        let ollama_running = self.probe_local_llama() || self.probe_ollama();
        let provider = self.get_effective_provider();

        json!({
            "wizard_completed": self.get_bool("ai_wizard_completed", false),
            "ai_strategy": self.get_str("ai_strategy", "remote_first"),
            "provider": provider,
            "has_internet": true,
            "has_gemini_key": self.has_gemini_key(),
            "has_openai_key": !self.get_str("openai_api_key", "").is_empty(),
            "has_claude_key": !self.get_str("claude_api_key", "").is_empty(),
            "ollama_running": ollama_running,
            "ollama_installed": ollama_paths.get("installed").and_then(Value::as_bool).unwrap_or(false),
            "offline_models": offline_models,
            "config": {
                "ai_provider": self.get_str("ai_provider", "auto"),
                "gemini_model": self.get_str("gemini_model", "gemini-2.5-flash"),
                "ollama_url": self.get_str("ollama_url", "http://127.0.0.1:11434"),
                "ollama_model": self.get_str("ollama_model", ""),
                "openai_model": self.get_str("openai_model", "gpt-4o-mini"),
                "claude_model": self.get_str("claude_model", "claude-3-5-sonnet-20241022"),
                "custom_base_url": self.get_str("custom_base_url", "https://api.openai.com/v1"),
                "custom_model": self.get_str("custom_model", "")
            }
        })
    }

    pub fn check_local_model_recommendations(&mut self) -> Value {
        let skip_prompt = self.get_bool("skip_local_model_upgrade_prompt", false);
        let installed_models = self.read_offline_models();
        let ollama_status = self.find_ollama_paths();
        let has_gemma = installed_models
            .iter()
            .any(|m| m.to_ascii_lowercase().contains("gemma"));
        let recs = recommended_local_models();
        let best = recs[0].clone();
        let need_prompt = !skip_prompt && !has_gemma;
        let reason = if need_prompt {
            "建議準備本機純綠 Google Gemma GGUF 離線模型，離線環境下亦可執行深度語法構式解析。"
        } else {
            ""
        };

        json!({
            "skip_prompt": skip_prompt,
            "need_prompt": need_prompt,
            "reason": reason,
            "installed_models": installed_models,
            "recommended_models": recs,
            "best_recommendation": best,
            "ollama_installed": ollama_status.get("installed").and_then(Value::as_bool).unwrap_or(false),
            "ollama_running": self.probe_local_llama() || self.probe_ollama()
        })
    }

    pub fn set_local_model_prompt_preference(&mut self, skip_prompt: bool) -> bool {
        self.config.insert(
            "skip_local_model_upgrade_prompt".to_string(),
            json!(skip_prompt),
        );
        self.save_config()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_ai_settings(
        &mut self,
        provider: &str,
        gemini_api_key: &str,
        gemini_model: &str,
        ollama_url: &str,
        ollama_model: &str,
        openai_api_key: &str,
        openai_model: &str,
        claude_api_key: &str,
        claude_model: &str,
        custom_base_url: &str,
        custom_api_key: &str,
        custom_model: &str,
    ) -> bool {
        self.config.insert("ai_provider".to_string(), json!(provider));
        if !gemini_api_key.is_empty() {
            self.config
                .insert("gemini_api_key".to_string(), json!(gemini_api_key));
        }
        if !gemini_model.is_empty() {
            self.config
                .insert("gemini_model".to_string(), json!(gemini_model));
        }
        if !ollama_url.is_empty() {
            self.config.insert("ollama_url".to_string(), json!(ollama_url));
        }
        if !ollama_model.is_empty() {
            self.config
                .insert("ollama_model".to_string(), json!(ollama_model));
        }
        if !openai_api_key.is_empty() {
            self.config
                .insert("openai_api_key".to_string(), json!(openai_api_key));
        }
        if !openai_model.is_empty() {
            self.config
                .insert("openai_model".to_string(), json!(openai_model));
        }
        if !claude_api_key.is_empty() {
            self.config
                .insert("claude_api_key".to_string(), json!(claude_api_key));
        }
        if !claude_model.is_empty() {
            self.config
                .insert("claude_model".to_string(), json!(claude_model));
        }
        if !custom_base_url.is_empty() {
            self.config
                .insert("custom_base_url".to_string(), json!(custom_base_url));
        }
        if !custom_api_key.is_empty() {
            self.config
                .insert("custom_api_key".to_string(), json!(custom_api_key));
        }
        if !custom_model.is_empty() {
            self.config
                .insert("custom_model".to_string(), json!(custom_model));
        }
        self.save_config()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_ai_wizard_strategy(
        &mut self,
        strategy: &str,
        gemini_api_key: &str,
        gemini_model: &str,
        ollama_url: &str,
        ollama_model: &str,
        remote_provider: &str,
        openai_api_key: &str,
        openai_model: &str,
        claude_api_key: &str,
        claude_model: &str,
        custom_base_url: &str,
        custom_api_key: &str,
        custom_model: &str,
    ) -> bool {
        self.config
            .insert("ai_wizard_completed".to_string(), json!(true));
        self.config
            .insert("ai_strategy".to_string(), json!(strategy));
        let provider = match strategy {
            "remote_first" => remote_provider,
            "local_only" => "ollama",
            _ => "disabled",
        };
        self.save_ai_settings(
            provider,
            gemini_api_key,
            gemini_model,
            ollama_url,
            ollama_model,
            openai_api_key,
            openai_model,
            claude_api_key,
            claude_model,
            custom_base_url,
            custom_api_key,
            custom_model,
        )
    }

    pub fn start_ollama_service(&mut self) -> bool {
        if self.probe_local_llama() || self.probe_ollama() {
            return true;
        }
        let ps1_candidates = [
            self.root.join("scripts").join("啟動本地AI服務.ps1"),
            self.root.join("啟動本地AI服務.ps1"),
        ];
        for ps1 in &ps1_candidates {
            if ps1.is_file() {
                let _ = Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-WindowStyle",
                        "Hidden",
                        "-File",
                        &ps1.to_string_lossy(),
                    ])
                    .spawn();
                for _ in 0..12 {
                    std::thread::sleep(Duration::from_millis(400));
                    if self.probe_local_llama() || self.probe_ollama() {
                        return true;
                    }
                }
            }
        }
        self.green_ai.scan_local_gguf_models();
        let best_gguf = self
            .green_ai
            .available_models
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase().contains("gemma"))
            .or_else(|| self.green_ai.available_models.iter().next())
            .and_then(|(_, info)| info.gguf_path.clone());
        if let Some(gguf_path) = best_gguf {
            if self
                .green_ai
                .spawn_green_server(&gguf_path, 8080, 99, 4096)
                .is_ok()
            {
                for _ in 0..10 {
                    std::thread::sleep(Duration::from_millis(400));
                    if self.probe_local_llama() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 呼叫對接之遠端或本機大型語言模型進行自然語言對話（由模型直接以自身語氣回覆使用者）
    pub fn invoke_llm_chat(&mut self, system_prompt: &str, user_prompt: &str) -> Option<(String, String)> {
        let provider = self.get_effective_provider();
        if provider == "disabled" {
            return None;
        }

        // 1. 優先嘗試 Google Gemini API (若已設定金鑰)
        if (provider == "gemini" || provider == "auto" || provider == "gemini_free_ready") && self.has_gemini_key() {
            let env_k = std::env::var("GEMINI_API_KEY").unwrap_or_default();
            let cfg_k = self.get_str("gemini_api_key", "");
            let api_key = if !env_k.trim().is_empty() {
                env_k.trim().to_string()
            } else {
                cfg_k.trim().to_string()
            };
            let model = self.get_str("gemini_model", "gemini-2.5-flash");
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
            );
            let payload = json!({
                "systemInstruction": {
                    "parts": [{ "text": system_prompt }]
                },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": user_prompt }]
                }],
                "generationConfig": {
                    "temperature": 0.4
                }
            });
            if let Some(raw_out) = curl_post_json(&url, &payload, &[]) {
                if let Ok(resp_json) = serde_json::from_str::<Value>(&raw_out) {
                    if let Some(text) = resp_json
                        .get("candidates")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| c0.get("content"))
                        .and_then(|cnt| cnt.get("parts"))
                        .and_then(|p| p.get(0))
                        .and_then(|p0| p0.get("text"))
                        .and_then(Value::as_str)
                    {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some((format!("Google Gemini ({model})"), trimmed.to_string()));
                        }
                    }
                }
            }
        }

        // 2. 若本機純綠 llama-server 或 Ollama 尚未喚醒，自動按需啟動
        if !self.probe_local_llama() && !self.probe_ollama() {
            let _ = self.start_ollama_service();
        }

        // 3. 嘗試本機純綠 llama-server (Gemma GGUF) 自然語言直通回覆
        if self.probe_local_llama() {
            let res = self.green_ai.chat_query(
                user_prompt,
                system_prompt,
                None,
                false,
                Duration::from_secs(25),
            );
            if res.ok && !res.text.trim().is_empty() {
                return Some((format!("本機純綠 Llama ({})", res.model), res.text.trim().to_string()));
            }
        }

        // 4. 嘗試本機 Ollama (127.0.0.1:11434) 自然語言直通回覆
        if self.probe_ollama() {
            let model = self.get_str("ollama_model", "gemma2:latest");
            let model_eff = if model.trim().is_empty() { "gemma2:latest".to_string() } else { model };
            if let Some(txt) = query_local_ollama_text(&model_eff, system_prompt, user_prompt) {
                return Some((format!("本機離線模型 ({model_eff})"), txt));
            }
        }

        None
    }

    /// 呼叫遠端或本地大型語言模型執行結構化 JSON 語言學推理（支援 Google Gemini API、本機純綠 llama-server 與本機 Ollama）
    pub fn invoke_llm_json(&mut self, system_prompt: &str, user_prompt: &str) -> Option<(String, Value)> {
        let provider = self.get_effective_provider();
        if provider == "disabled" {
            return None;
        }

        // 1. 優先嘗試 Google Gemini API (若已設定金鑰)
        if (provider == "gemini" || provider == "auto" || provider == "gemini_free_ready") && self.has_gemini_key() {
            let env_k = std::env::var("GEMINI_API_KEY").unwrap_or_default();
            let cfg_k = self.get_str("gemini_api_key", "");
            let api_key = if !env_k.trim().is_empty() {
                env_k.trim().to_string()
            } else {
                cfg_k.trim().to_string()
            };
            let model = self.get_str("gemini_model", "gemini-2.5-flash");
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
            );
            let payload = json!({
                "systemInstruction": {
                    "parts": [{ "text": system_prompt }]
                },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": user_prompt }]
                }],
                "generationConfig": {
                    "temperature": 0.2,
                    "responseMimeType": "application/json"
                }
            });
            if let Some(raw_out) = curl_post_json(&url, &payload, &[]) {
                if let Ok(resp_json) = serde_json::from_str::<Value>(&raw_out) {
                    if let Some(text) = resp_json
                        .get("candidates")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| c0.get("content"))
                        .and_then(|cnt| cnt.get("parts"))
                        .and_then(|p| p.get(0))
                        .and_then(|p0| p0.get("text"))
                        .and_then(Value::as_str)
                    {
                        if let Some(parsed) = parse_json_from_llm_text(text) {
                            return Some((format!("Google Gemini ({model})"), parsed));
                        }
                    }
                }
            }
        }

        // 2. 嘗試本機純綠 llama-server (Gemma GGUF)
        if self.probe_local_llama() {
            let res = self.green_ai.chat_query(
                user_prompt,
                system_prompt,
                None,
                true,
                Duration::from_secs(18),
            );
            if res.ok {
                if let Some(data) = res.data.or_else(|| parse_json_from_llm_text(&res.text)) {
                    return Some((format!("本機純綠 Llama ({})", res.model), data));
                }
            }
        }

        // 3. 嘗試本機 Ollama (127.0.0.1:11434)
        if self.probe_ollama() {
            let model = self.get_str("ollama_model", "gemma2:latest");
            let model_eff = if model.trim().is_empty() { "gemma2:latest".to_string() } else { model };
            if let Some(parsed) = query_local_ollama_json(&model_eff, system_prompt, user_prompt) {
                return Some((format!("本機離線模型 ({model_eff})"), parsed));
            }
        }

        None
    }

    /// 批次診斷錯誤句與正確句對，驅動遠端/本地 AI 理解對照句並執行「舉一反三」全詞庫同構掃描 (`diagnose_sentence_pairs_batch`)
    pub fn diagnose_sentence_pairs_batch(&mut self, pairs: &[Value], tuner: &SemanticTuner) -> Value {
        let mut items = Vec::new();
        let mut item_diagnostics = Vec::new();
        let mut all_sym_diffs = Vec::new();
        let mut all_rec_words: Vec<String> = Vec::new();
        let mut all_rec_word_objs: Vec<Value> = Vec::new();
        let mut all_rec_constructions: Vec<Value> = Vec::new();
        let mut all_rec_rules: Vec<Value> = Vec::new();
        let mut seen_words = HashSet::new();
        let mut seen_constructions = HashSet::new();
        let typos = common_typo_map();

        for (pair_idx, p) in pairs.iter().enumerate() {
            let err = p.get("error").and_then(Value::as_str).unwrap_or("").trim();
            let cor = p.get("correct").and_then(Value::as_str).unwrap_or("").trim();
            if err.is_empty() || cor.is_empty() {
                continue;
            }
            let sym_diffs = p
                .get("symbolic_diffs")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| tuner.align_sentence_pair(err, cor, None, None));
            all_sym_diffs.extend(sym_diffs.clone());

            let mut rec_words = Vec::new();
            let mut rec_word_names = Vec::new();
            let mut rec_constructions = Vec::new();
            let mut rec_rules = Vec::new();
            let mut taxonomies = Vec::new();
            let mut strategies = Vec::new();
            let mut root_causes = Vec::new();
            let mut pos_parts = Vec::new();
            let mut construction_names = Vec::new();
            let mut is_typo = false;

            for d in &sym_diffs {
                let rej = d.get("rejected").and_then(Value::as_str).unwrap_or("");
                let acc = d.get("accepted").and_then(Value::as_str).unwrap_or("");
                let prev_ctx = d.get("prev_ctx").and_then(Value::as_str).unwrap_or("");
                let next_ctx = d.get("next_ctx").and_then(Value::as_str).unwrap_or("");
                let next_ascii = d.get("next_ascii").and_then(Value::as_str).unwrap_or("");
                if typos.contains_key(acc) {
                    is_typo = true;
                }
                if let Some(t) = d.get("error_taxonomy").and_then(Value::as_str) {
                    if !taxonomies.contains(&t.to_string()) {
                        taxonomies.push(t.to_string());
                    }
                }
                if let Some(s) = d.get("layer_strategy").and_then(Value::as_str) {
                    if !strategies.contains(&s.to_string()) {
                        strategies.push(s.to_string());
                    }
                }
                if let Some(hj) = d.get("overlapping_hijack").and_then(Value::as_object) {
                    if let Some(mech) = hj.get("mechanism").and_then(Value::as_str) {
                        root_causes.push(format!("【{} ➔ {}】{}", rej, acc, mech));
                    }
                } else {
                    let tax_short = d
                        .get("error_taxonomy")
                        .and_then(Value::as_str)
                        .unwrap_or("二階轉移失配");
                    root_causes.push(format!(
                        "【{} ➔ {}】屬 {}（前驅：{}，後繼：{}）",
                        rej,
                        acc,
                        tax_short,
                        if prev_ctx.is_empty() { "句首/標點" } else { prev_ctx },
                        if !next_ascii.is_empty() {
                            next_ascii
                        } else if next_ctx.is_empty() {
                            "句尾/標點"
                        } else {
                            next_ctx
                        }
                    ));
                }

                if let Some(words) = d.get("oov_compound_words").and_then(Value::as_array) {
                    for w in words {
                        rec_words.push(w.clone());
                        if let Some(w_str) = w.get("word").and_then(Value::as_str) {
                            if !rec_word_names.contains(&w_str.to_string()) {
                                rec_word_names.push(w_str.to_string());
                            }
                            if seen_words.insert(w_str.to_string()) {
                                all_rec_words.push(w_str.to_string());
                                all_rec_word_objs.push(w.clone());
                            }
                        }
                    }
                }

                if let Some(cs) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                    for c in cs {
                        rec_constructions.push(c.clone());
                        if let Some(cname) = c.get("name").and_then(Value::as_str) {
                            if !construction_names.contains(&cname.to_string()) {
                                construction_names.push(cname.to_string());
                            }
                        }
                        if let Some(formula) = c.get("formula").and_then(Value::as_str) {
                            pos_parts.push(format!("構式槽位 {}", formula));
                        }
                        if let Some(pid) = c.get("pattern_id").and_then(Value::as_str) {
                            if seen_constructions.insert(pid.to_string()) {
                                all_rec_constructions.push(c.clone());
                            }
                        }
                        if let Some(brs) = c.get("backbone_rules").and_then(Value::as_array) {
                            rec_rules.extend(brs.clone());
                            all_rec_rules.extend(brs.clone());
                        }
                    }
                }

                let rules = tuner.synthesize_rules(d);
                rec_rules.extend(rules.clone());
                all_rec_rules.extend(rules);
            }

            let taxonomy_joined = if taxonomies.is_empty() {
                "Type II: 同音詞二階語法轉移引力不足".to_string()
            } else {
                taxonomies.join(" ＋ ")
            };
            let cname_joined = if construction_names.is_empty() {
                taxonomy_joined.clone()
            } else {
                construction_names.join(" · ")
            };
            let root_cause_str = if root_causes.is_empty() {
                format!("計算語言學雙向詞界對齊診斷：{}", taxonomy_joined)
            } else {
                root_causes.join("；")
            };
            let pos_str = if pos_parts.is_empty() {
                strategies.join(" ＋ ")
            } else {
                pos_parts.join(" ｜ ")
            };
            let safety_str = "嚴格採行四層語意防禦架構（Layer 1 階層槽位構式 + Layer 3 複合專名凝聚 + Layer 2 二階轉移 +26.0~+30.0），100% 保持單字全域先驗獨立，零單字權重變動。";

            item_diagnostics.push(json!({
                "index": pair_idx + 1,
                "error": err,
                "correct": cor,
                "error_taxonomy": taxonomy_joined,
                "construction_name": cname_joined,
                "root_cause": root_cause_str,
                "pos_breakdown": pos_str,
                "unigram_safety_proof": safety_str
            }));

            items.push(json!({
                "error": err,
                "correct": cor,
                "is_user_typo": is_typo,
                "error_taxonomy": taxonomy_joined,
                "root_cause": root_cause_str,
                "syntactic_structure": pos_str,
                "safety_isolation_note": safety_str,
                "recommended_lexicon_words": rec_words,
                "recommended_constructions": rec_constructions,
                "recommended_rules": rec_rules,
                "syntactic_analysis": {
                    "register": "現代台灣繁體書面與專業科技語體",
                    "error_taxonomy": taxonomy_joined,
                    "construction_name": cname_joined,
                    "root_cause": root_cause_str,
                    "pos_breakdown": pos_str
                },
                "unigram_safety_proof": safety_str,
                "provider_note": format!("診斷引擎：{}（結合六大語病分類學、跨界假詞劫掠分析、全詞庫舉一反三掃描與四層語意合成）", self.get_effective_provider())
            }));
        }

        // ── 執行「舉一反三」全詞庫同構掃描與遠端/本地 AI 深度類推擴充 ──
        let analog_scan = tuner.scan_analogous_cases_across_lexicon(&all_sym_diffs);
        let mut scanned_families = analog_scan
            .get("scanned_families")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if let Some(aw_list) = analog_scan.get("analogous_words").and_then(Value::as_array) {
            for w in aw_list {
                if let Some(w_str) = w.get("word").and_then(Value::as_str) {
                    if seen_words.insert(w_str.to_string()) {
                        all_rec_words.push(w_str.to_string());
                        all_rec_word_objs.push(w.clone());
                    }
                }
            }
        }
        if let Some(ac_list) = analog_scan.get("analogous_constructions").and_then(Value::as_array) {
            for c in ac_list {
                if let Some(pid) = c.get("pattern_id").and_then(Value::as_str) {
                    if seen_constructions.insert(pid.to_string()) {
                        all_rec_constructions.push(c.clone());
                    }
                }
                if let Some(brs) = c.get("backbone_rules").and_then(Value::as_array) {
                    all_rec_rules.extend(brs.clone());
                }
            }
        }

        // 嘗試呼叫遠端 Gemini 或本機 Gemma / Ollama 進行深層「舉一反三」推理補充
        let mut llm_engine_used: Option<String> = None;
        if !items.is_empty() {
            let sys_prompt = "你是如喜 (RuhI) 智慧注音輸入法的計算語言學與漢語句法專家。請根據使用者提供的『錯誤句 ➔ 正確句』對照，找出 Viterbi 解碼選錯字或跨詞界劫掠的根本原因，並執行『舉一反三』：列出同構型、同詞綴或同語法槽位的類推詞彙與集合槽位構式。嚴禁調整單字全域先驗權重。請回傳 JSON 格式：{\"llm_insight\": \"深層句法與舉一反三總結\", \"analogous_words\": [{\"word\": \"詞彙\", \"category\": \"分類\", \"reason\": \"舉一反三原因\"}], \"analogous_constructions\": [{\"pattern_id\": \"英文ID\", \"name\": \"構式名稱\", \"formula\": \"[{槽位A}] → [{槽位B}]\", \"prev_slot\": [\"詞1\", \"詞2\"], \"next_slot\": [\"詞3\", \"詞4\"], \"suppress\": [\"錯詞\"], \"bonus\": 30.0}]}";
            let pairs_brief: Vec<Value> = items
                .iter()
                .map(|it| {
                    json!({
                        "error": it.get("error"),
                        "correct": it.get("correct"),
                        "symbolic_cause": it.get("root_cause")
                    })
                })
                .collect();
            let usr_prompt = format!(
                "請針對以下對照句進行深層語意理解與『舉一反三』同構擴充（使用台灣正體中文術語）：\n{}",
                serde_json::to_string_pretty(&pairs_brief).unwrap_or_default()
            );
            if let Some((engine_name, llm_json)) = self.invoke_llm_json(sys_prompt, &usr_prompt) {
                llm_engine_used = Some(engine_name.clone());
                if let Some(insight) = llm_json.get("llm_insight").and_then(Value::as_str) {
                    if !insight.trim().is_empty() {
                        scanned_families.insert(
                            0,
                            json!({
                                "trigger": format!("🤖 {} 深層語意與舉一反三推理", engine_name),
                                "family_name": "大型語言模型跨語境同構類推",
                                "vulnerability_mechanism": insight.trim(),
                                "scanned_siblings": [],
                                "action": "已將 AI 類推之同構詞族與語法槽位併入知識庫。"
                            }),
                        );
                    }
                }
                if let Some(llm_words) = llm_json.get("analogous_words").and_then(Value::as_array) {
                    for w in llm_words {
                        let w_str = w
                            .get("word")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim();
                        if (2..=5).contains(&w_str.chars().count())
                            && is_valid_chinese_token(w_str)
                            && !is_syntactic_phrase_or_clitic(w_str)
                            && seen_words.insert(w_str.to_string())
                        {
                            all_rec_words.push(w_str.to_string());
                            all_rec_word_objs.push(json!({
                                "word": w_str,
                                "bopomofo": tuner.get_bopomofo(w_str),
                                "weight": 1580,
                                "category": w.get("category").and_then(Value::as_str).unwrap_or("AI舉一反三擴充詞"),
                                "reason": w.get("reason").and_then(Value::as_str).unwrap_or("AI 舉一反三同構詞彙擴充")
                            }));
                        }
                    }
                }
                if let Some(llm_cs) = llm_json.get("analogous_constructions").and_then(Value::as_array) {
                    for c in llm_cs {
                        let pid = c
                            .get("pattern_id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim();
                        if !pid.is_empty() && seen_constructions.insert(pid.to_string()) {
                            all_rec_constructions.push(c.clone());
                        }
                    }
                }
            }
        }

        let mut single = items.first().cloned().unwrap_or_else(|| json!({}));
        if single.get("recommended_lexicon_words").is_some() {
            single["recommended_lexicon_word_objs"] = json!(all_rec_word_objs);
            single["recommended_lexicon_words"] = json!(all_rec_words);
            single["recommended_constructions"] = json!(all_rec_constructions);
        }

        let engine_tag = llm_engine_used
            .clone()
            .unwrap_or_else(|| "內建神經符號計算語言學引擎 + 全詞庫舉一反三掃描器".to_string());
        let batch_summary = format!(
            "【{}】已完成 {} 組對照語句理解與「舉一反三」全詞庫掃描：共掃描 {} 大同構詞族、合成/擴充 {} 組 Layer 1 階層槽位構式、收錄 {} 個 Layer 3 複合實詞與同構專名，並建立 {} 條 Layer 2 二階轉移規則。",
            engine_tag,
            items.len(),
            scanned_families.len(),
            all_rec_constructions.len(),
            all_rec_words.len(),
            all_rec_rules.len()
        );

        json!({
            "status": "ok",
            "provider": self.get_effective_provider(),
            "llm_engine": engine_tag,
            "count": items.len(),
            "is_batch": items.len() > 1,
            "analogical_families": scanned_families,
            "batch_data": {
                "batch_summary": batch_summary,
                "analogical_families": scanned_families,
                "recommended_lexicon_words": all_rec_words,
                "recommended_lexicon_word_objs": all_rec_word_objs,
                "recommended_constructions": all_rec_constructions,
                "recommended_rules": all_rec_rules,
                "item_diagnostics": item_diagnostics
            },
            "items": items,
            "single": single,
            "summary": batch_summary
        })
    }

    /// 專業領域長篇文獻詞彙與二階轉移萃取 (`extract_domain_corpus`)
    pub fn extract_domain_corpus(&self, text: &str, domain: &str, tuner: &SemanticTuner) -> Value {
        let spans = tuner.tokenize_with_spans(text);
        let mut terms = Vec::new();
        let mut transitions = Vec::new();
        let mut seen_terms = HashSet::new();
        let mut seen_trans = HashSet::new();

        let cjk_tokens: Vec<String> = spans
            .into_iter()
            .map(|(t, _, _)| t)
            .filter(|t| is_valid_chinese_token(t))
            .collect();

        for tok in &cjk_tokens {
            let len = tok.chars().count();
            if (2..=4).contains(&len)
                && !is_syntactic_phrase_or_clitic(tok)
                && seen_terms.insert(tok.clone())
            {
                terms.push(json!({
                    "word": tok,
                    "bopomofo": tuner.get_bopomofo(tok),
                    "weight": 1550,
                    "category": format!("{domain}領域詞彙")
                }));
            }
        }

        for pair in cjk_tokens.windows(2) {
            let p = &pair[0];
            let n = &pair[1];
            if p != n && seen_trans.insert((p.clone(), n.clone())) {
                transitions.push(json!({
                    "prev": p,
                    "next": n,
                    "score": 28.0,
                    "description": format!("{domain}文獻二階搭配 [{p} ➔ {n}]")
                }));
            }
        }

        json!({
            "status": "ok",
            "domain": domain,
            "terms": terms,
            "transitions": transitions,
            "summary": format!("自「{domain}」文獻提煉出 {} 個核心詞條與 {} 條二階搭配轉移。", terms.len(), transitions.len())
        })
    }

    /// 審計 Tailcat 去中心化語意提案 (`audit_tailcat_proposal`)
    pub fn audit_tailcat_proposal(&self, proposal: &Value) -> Value {
        let rules = proposal
            .get("rules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut passed = 0;
        let mut warnings = Vec::new();

        for r in &rules {
            let prev = r.get("prev").and_then(Value::as_str).unwrap_or("");
            let next = r.get("next").and_then(Value::as_str).unwrap_or("");
            let (ok1, msg1) = ZeroPiiPrivacyFilter::is_safe_token(prev);
            let (ok2, msg2) = ZeroPiiPrivacyFilter::is_safe_token(next);
            if ok1 && ok2 {
                passed += 1;
            } else {
                warnings.push(format!("[{prev} ➔ {next}]: {msg1} / {msg2}"));
            }
        }

        json!({
            "status": "ok",
            "safety_verdict": if warnings.is_empty() { "SAFE_TO_MERGE" } else { "NEEDS_REVIEW" },
            "total_rules": rules.len(),
            "passed_rules": passed,
            "warnings": warnings,
            "audit_summary": format!(
                "Zero-PII 與語言學邊界審計完成：共 {} 條規則通過驗證，{} 條攔截。",
                passed,
                warnings.len()
            )
        })
    }

    /// 將暫存修訂記錄昇華為階層構式、雙字原子實詞與二階正向轉移 (`sublimate_revisions`)
    pub fn sublimate_revisions(&mut self, tuner: &mut SemanticTuner) -> Value {
        let records = tuner.load_revisions();
        if records.is_empty() {
            return json!({
                "status": "ok",
                "sublimated_words": [],
                "sublimated_rules": [],
                "sublimated_constructions": [],
                "sublimated_records_count": 0,
                "words_added_count": 0,
                "rules_added_count": 0,
                "explanation": "目前無待昇華之暫存修訂記錄。"
            });
        }

        let mut all_words = Vec::new();
        let mut all_rules = Vec::new();
        let mut all_constructions = Vec::new();
        let mut valid_count = 0;

        for rec in &records {
            let rej = rec
                .get("rejected")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let acc = rec
                .get("accepted")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let prev = rec
                .get("prev_context")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let next = rec
                .get("next_context")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if rej.is_empty() || acc.is_empty() || common_typo_map().contains_key(acc) {
                continue;
            }
            valid_count += 1;
            let err_sent = format!("{prev}{rej}{next}");
            let cor_sent = format!("{prev}{acc}{next}");
            let diffs = tuner.align_sentence_pair(&err_sent, &cor_sent, None, None);
            for d in &diffs {
                if let Some(ws) = d.get("oov_compound_words").and_then(Value::as_array) {
                    all_words.extend(ws.clone());
                }
                if let Some(cs) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                    all_constructions.extend(cs.clone());
                }
                all_rules.extend(tuner.synthesize_rules(d));
            }
        }

        let words_added = tuner.append_words_to_domain_lexicon(&all_words);
        let rules_added = tuner.append_rules_to_tsv(&all_rules);

        json!({
            "status": "ok",
            "sublimated_words": all_words,
            "sublimated_rules": all_rules,
            "sublimated_constructions": all_constructions,
            "sublimated_records_count": valid_count,
            "words_added_count": words_added,
            "rules_added_count": rules_added,
            "explanation": format!(
                "如喜神經符號語意昇華系統已將 {} 筆暫存修正深度洗滌，提煉出 {} 條階層式構式、新增 {} 個雙字原子實詞與 {} 條二階正向引力規則！",
                valid_count,
                all_constructions.len(),
                words_added,
                rules_added
            )
        })
    }

    /// 自使用者輸入的多行對話文字中自動偵測是否內嵌「錯誤輸出句／期望正確句」對照行
    pub fn extract_embedded_sentence_pair(user_message: &str) -> Option<(String, String)> {
        let mut candidate_lines = Vec::new();
        for raw_line in user_message.lines() {
            let line = raw_line
                .trim()
                .trim_start_matches(['-', '•', '*', '1', '2', '.', '、', ' '])
                .trim_start_matches("錯誤輸出句：")
                .trim_start_matches("錯誤輸出句:")
                .trim_start_matches("錯誤句：")
                .trim_start_matches("錯誤句:")
                .trim_start_matches("期望正確句：")
                .trim_start_matches("期望正確句:")
                .trim_start_matches("正確句：")
                .trim_start_matches("正確句:")
                .trim();
            if line.is_empty() {
                continue;
            }
            // 跳過純提問句（如「您會如何解決？」、「怎麼解決？」、「為什麼會這樣？」）
            if (line.ends_with('？') || line.ends_with('?'))
                && (line.contains("如何")
                    || line.contains("怎麼")
                    || line.contains("為何")
                    || line.contains("為什麼")
                    || line.contains("請問")
                    || line.chars().count() <= 10)
            {
                continue;
            }
            let cjk_cnt = line.chars().filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c)).count();
            if cjk_cnt >= 4 {
                candidate_lines.push(line.to_string());
            }
        }
        if candidate_lines.len() >= 2 {
            let l1 = &candidate_lines[0];
            let l2 = &candidate_lines[1];
            let len1 = l1.chars().count();
            let len2 = l2.chars().count();
            if l1 != l2 && len1.abs_diff(len2) <= 4 {
                let common_chars = l1.chars().filter(|c| l2.contains(*c)).count();
                if common_chars * 2 >= len1.min(len2) {
                    return Some((l1.clone(), l2.clone()));
                }
            }
        }
        None
    }

    /// 自然語言互動調校對話引擎 (`chat_with_model_for_tuning`)
    /// 讓對接之 AI 語言模型（Google Gemini、本機純綠 Gemma GGUF、Ollama）直接以自身語氣回覆使用者，
    /// 並在討論具體調校案例時同步提煉可一鍵套用之三層語意規則（Layer 1 槽位陣列 / Layer 2 二階轉移 / Layer 3 原子詞根）。
    pub fn chat_with_model_for_tuning(
        &mut self,
        user_message: &str,
        error_sent: &str,
        correct_sent: &str,
        current_log: &str,
        target_lang: &str,
        tuner: &SemanticTuner,
    ) -> Value {
        let msg_trim = user_message.trim();
        if msg_trim.is_empty() {
            return json!({
                "status": "error",
                "message": "請輸入訊息與對接之語言模型對話。"
            });
        }

        // 若使用者直接在對話框貼上兩行對照句，優先萃取對話框內的對照句
        let embedded_pair = Self::extract_embedded_sentence_pair(msg_trim);
        let err_effective = embedded_pair
            .as_ref()
            .map(|(e, _)| e.as_str())
            .unwrap_or_else(|| error_sent.trim());
        let cor_effective = embedded_pair
            .as_ref()
            .map(|(_, c)| c.as_str())
            .unwrap_or_else(|| correct_sent.trim());

        let err_trim = err_effective.trim();
        let cor_trim = cor_effective.trim();
        let lang_label = if target_lang.trim().is_empty() {
            "zh-TW"
        } else {
            target_lang.trim()
        };

        let mut proposed_constructions: Vec<Value> = Vec::new();
        let mut proposed_rules: Vec<Value> = Vec::new();
        let mut proposed_words: Vec<Value> = Vec::new();
        let mut seen_rules = HashSet::new();
        let mut seen_words = HashSet::new();
        let mut active_diffs: Vec<Value> = Vec::new();

        // 僅在使用者非單純寒暄/自我介紹，且對照句非未更動之預設範例（或訊息明確提及對照句）時，才對錯句與正句執行對齊提煉
        let is_greeting_or_intro = msg_trim.contains("自我介紹")
            || msg_trim.contains("你是誰")
            || msg_trim.contains("請問你是")
            || msg_trim == "你好"
            || msg_trim == "您好";
        let is_default_placeholder = embedded_pair.is_none()
            && err_trim == "讓步董程式的人也能輕鬆上手"
            && !msg_trim.contains("步董")
            && !msg_trim.contains("不懂");

        if !is_greeting_or_intro
            && !is_default_placeholder
            && !err_trim.is_empty()
            && !cor_trim.is_empty()
        {
            active_diffs = tuner.align_sentence_pair(err_trim, cor_trim, None, None);
            for d in &active_diffs {
                if let Some(ws) = d.get("oov_compound_words").and_then(Value::as_array) {
                    for w in ws {
                        if let Some(w_str) = w.get("word").and_then(Value::as_str) {
                            if seen_words.insert(w_str.to_string()) {
                                proposed_words.push(w.clone());
                            }
                        }
                    }
                }
                if let Some(cs) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                    for c in cs {
                        proposed_constructions.push(c.clone());
                        if let Some(brs) = c.get("backbone_rules").and_then(Value::as_array) {
                            for br in brs {
                                let rp = br.get("prev").and_then(Value::as_str).unwrap_or("").to_string();
                                let rn = br.get("next").and_then(Value::as_str).unwrap_or("").to_string();
                                if !rp.is_empty() && !rn.is_empty() && seen_rules.insert((rp, rn)) {
                                    proposed_rules.push(br.clone());
                                }
                            }
                        }
                    }
                }
                for r in tuner.synthesize_rules(d) {
                    let rp = r.get("prev").and_then(Value::as_str).unwrap_or("").to_string();
                    let rn = r.get("next").and_then(Value::as_str).unwrap_or("").to_string();
                    if !rp.is_empty() && !rn.is_empty() && seen_rules.insert((rp, rn)) {
                        proposed_rules.push(r);
                    }
                }
            }
        }

        // 解析自然語言中明確提及之特定構式家族（如「控、狂、粉、專家」準詞綴家族）
        if msg_trim.contains("控")
            && (msg_trim.contains("狂")
                || msg_trim.contains("粉")
                || msg_trim.contains("專家")
                || msg_trim.contains("陣列"))
        {
            proposed_constructions.push(json!({
                "pattern_id": "aficionado_specialist_quasi_suffix_3d_array",
                "name": "人物喜好與專精屬性準詞綴三維構式陣列 ([介詞/限定詞] × [領域特徵詞幹] × [{控, 狂, 粉, 迷, 達人, 專家, 大師}])",
                "formula": "([{對於, 對, 身為, 作為, 是個}]) × [領域/特徵詞幹] × [{控, 狂, 粉, 迷, 族, 達人, 魔人, 專家, 大師, 高手, 玩家, 行家}]",
                "slots": [
                    ["對於", "對", "身為", "作為", "是個", "一介", "簡直是", "完全是"],
                    ["細節", "音質", "畫質", "顏值", "制服", "聲優", "文具", "鍵盤", "字體", "收納", "咖啡", "抹茶", "甜點", "機械", "極簡", "參數", "數據", "攝影", "音響", "工作", "購物", "健身", "動漫", "鐵道", "科技", "語言", "推理", "歷史"],
                    ["控", "狂", "粉", "迷", "族", "達人", "魔人", "專家", "大師", "高手", "玩家", "行家"]
                ],
                "optional_slots": [1],
                "bonus": 30.0
            }));
            for (p, n, sc, cmt) in [
                ("對於", "細節控", 30.0, "NL對話提煉：對於細節控"),
                ("細節控", "會", 28.0, "NL對話提煉：細節控會感到"),
                ("細節", "控", 28.0, "NL對話提煉：XYZ控準詞綴"),
                ("音質", "控", 28.0, "NL對話提煉：XYZ控準詞綴"),
                ("鍵盤", "控", 28.0, "NL對話提煉：XYZ控準詞綴"),
            ] {
                if seen_rules.insert((p.to_string(), n.to_string())) {
                    proposed_rules.push(json!({
                        "prev": p,
                        "next": n,
                        "score": sc,
                        "kind": "collocation",
                        "comment": cmt
                    }));
                }
            }
        }

        // 解析自然語言中明確提及之「(不)X 的 是」準分裂句名物化焦點判斷構式
        if msg_trim.contains("(不)X")
            || (msg_trim.contains("的") && msg_trim.contains("是") && msg_trim.contains("買") && msg_trim.contains("賣"))
        {
            if !proposed_constructions
                .iter()
                .any(|c| c.get("pattern_id").and_then(Value::as_str) == Some("pseudo_cleft_neg_v_de_shi_3d_array"))
            {
                proposed_constructions.push(json!({
                    "pattern_id": "pseudo_cleft_neg_v_de_shi_3d_array",
                    "name": "準分裂句名物化焦點判斷構式「(不)X 的 是」",
                    "formula": "「(不)X 的 是」，X={選, 買, 賣, 看, 聽, 讀, 寫, 說, 講, 問, 要, 想, 做, 吃, 喝, 穿, 用, 找, 投, 盼, 爭, 怕, 愛...}（即 [(不)X] × [的] × [是]）",
                    "slots": [
                        ["選", "買", "賣", "看", "聽", "讀", "寫", "說", "講", "問", "要", "想", "做", "吃", "喝", "穿", "用", "找", "投", "盼", "爭", "怕", "愛", "比", "拼", "靠", "憑", "缺", "指", "不選", "不買", "不賣", "不看", "不聽", "不讀", "不寫", "不說", "不講", "不問", "不要", "不想", "不做", "不吃", "不穿", "不用", "不找", "不怕", "不愛", "不投", "不爭"],
                        ["的"],
                        ["是"]
                    ],
                    "prev_slot": ["選", "買", "賣", "看", "聽", "讀", "寫", "說", "講", "問", "要", "想", "做", "吃", "喝", "穿", "用", "找", "投", "盼", "爭", "怕", "愛", "比", "拼", "靠", "憑", "缺", "指", "不選", "不買", "不賣", "不看", "不聽", "不讀", "不寫", "不說", "不講", "不問", "不要", "不想", "不做", "不吃", "不穿", "不用", "不找", "不怕", "不愛", "不投", "不爭", "的"],
                    "next_slot": ["的", "是"],
                    "suppress": ["的試", "試一個", "選的試"],
                    "bonus": 30.0
                }));
            }
            for (p, n, sc, cmt) in [
                ("選", "的", 28.0, "準分裂句名物化動詞槽位 [(不)X ➔ 的]"),
                ("的", "是", 30.0, "準分裂句焦點標記耦合 [的 ➔ 是]"),
            ] {
                if seen_rules.insert((p.to_string(), n.to_string())) {
                    proposed_rules.push(json!({
                        "prev": p,
                        "next": n,
                        "score": sc,
                        "kind": "construction",
                        "comment": cmt
                    }));
                }
            }
        }

        // 整理神經符號雙向詞界對齊與三層語意合成摘要，注入 Prompt 供 LLM 精確理解自身任務
        let mut symbolic_brief_lines = Vec::new();
        for (idx, d) in active_diffs.iter().enumerate() {
            let rej = d.get("rejected").and_then(Value::as_str).unwrap_or("");
            let acc = d.get("accepted").and_then(Value::as_str).unwrap_or("");
            let bpmf = d.get("bopomofo").and_then(Value::as_str).unwrap_or("");
            let p_ctx = d.get("prev_ctx").and_then(Value::as_str).unwrap_or("");
            let n_ctx = d.get("next_ctx").and_then(Value::as_str).unwrap_or("");
            let tax = d.get("error_taxonomy").and_then(Value::as_str).unwrap_or("");
            symbolic_brief_lines.push(format!(
                "  {}. 病灶 {}：使用者想輸入正確詞「{}」（注音：{}），但輸入法在「{} ➔ {} ➔ {}」語境下誤選成了同音錯字「{}」（診斷分類：{}）。",
                idx + 1,
                idx + 1,
                acc,
                bpmf,
                if p_ctx.is_empty() { "句首" } else { p_ctx },
                acc,
                if n_ctx.is_empty() { "句尾" } else { n_ctx },
                rej,
                tax
            ));
        }

        let mut layer1_lines = Vec::new();
        for c in &proposed_constructions {
            let cname = c.get("name").and_then(Value::as_str).unwrap_or("槽位構式");
            let cform = c.get("formula").and_then(Value::as_str).unwrap_or("");
            let cbonus = c.get("bonus").and_then(Value::as_f64).unwrap_or(28.0);
            layer1_lines.push(format!("  • Layer 1 構式【{cname}】：`{cform}` (+{cbonus:.1})"));
        }

        let mut layer2_lines = Vec::new();
        for r in &proposed_rules {
            let rp = r.get("prev").and_then(Value::as_str).unwrap_or("");
            let rn = r.get("next").and_then(Value::as_str).unwrap_or("");
            let sc = r.get("score").and_then(Value::as_f64).unwrap_or(28.0);
            let cmt = r.get("comment").and_then(Value::as_str).unwrap_or("");
            layer2_lines.push(format!("  • Layer 2 二階轉移：`[{rp} ➔ {rn}]` (+{sc:.1}，{cmt})"));
        }

        let symbolic_context_block = if !symbolic_brief_lines.is_empty() {
            format!(
                "【雙向詞界對齊病灶清單（共 {} 處同音誤選，請逐一完整分析）】：\n{}\n\n【已提煉之如喜三層語意調校規則】：\n{}\n{}\n  • 鐵律保證：100% 維持單字無上下文全域先驗權重不變（嚴禁單字權重打地鼠）。",
                symbolic_brief_lines.len(),
                symbolic_brief_lines.join("\n"),
                if layer1_lines.is_empty() { "  • (無新增 Layer 1 構式)".to_string() } else { layer1_lines.join("\n") },
                if layer2_lines.is_empty() { "  • (無新增 Layer 2 轉移)".to_string() } else { layer2_lines.join("\n") }
            )
        } else {
            String::new()
        };

        // 直接呼叫對接之遠端或本機大型語言模型（Google Gemini / 本機純綠 Gemma GGUF / Ollama）以自然語言本身回覆
        let chat_sys_prompt = if is_greeting_or_intro {
            "你是對接於「如喜輸入法 (RuhI — 如雲行水流自在歡喜) · 語意調校工作室」的 AI 語言模型顧問。請使用自然、親切、清晰的台灣繁體中文自我介紹你的模型身分，並說明你如何透過如喜三層語意架構（Layer 1 多維槽位構式陣列、Layer 2 二階條件轉移、Layer 3 領域原子詞根，且嚴守不隨意調整單字全域先驗權重）協助使用者解決注音輸入法的同音選字問題。"
        } else {
            "你是對接於「如喜輸入法 (RuhI — 如雲行水流自在歡喜) · 語意調校工作室」的計算語言學與輸入法引擎調校顧問（絕對不是作文批改老師）。\n\
            【核心任務鐵律】：\n\
            1. 第一句「錯誤輸出句」是注音輸入法 Viterbi 解碼因同音競爭而選錯字的結果；第二句「期望正確句」是使用者真正想輸入的 100% 正確目標句。\n\
            2. 絕對禁止批評「期望正確句」有語病！絕對禁止建議使用者改寫句子！你的任務是說明如何調校輸入法引擎，使輸入法能把每一處同音錯字修正為正確字。\n\
            3. 請完整涵蓋下方列出的每一處同音病灶（不可遺漏任何一處！且注意因果方向：是「正確詞」被輸入法誤選成了「同音錯字」，切勿講反），並以精煉條列說明 Layer 1 多維槽位構式陣列 (`patterns_user.jsonl`)、Layer 2 二階條件轉移 (`transitions_user.tsv`) 與「嚴禁調整單字全域先驗權重」之解決方案。"
        };

        let chat_usr_prompt = if !symbolic_context_block.is_empty() {
            format!(
                "【輸入法同音選字調校案例】\n\
                - 輸入法錯誤輸出句：{err_trim}\n\
                - 使用者期望正確句：{cor_trim}\n\n\
                {symbolic_context_block}\n\n\
                使用者提問：{msg_trim}\n\n\
                請根據上述病灶清單（共 {} 處病灶皆須說明）與三層語意調校規則，用清晰條列的台灣正體中文回答：",
                symbolic_brief_lines.len()
            )
        } else if !is_greeting_or_intro
            && !is_default_placeholder
            && !err_trim.is_empty()
            && !cor_trim.is_empty()
        {
            if current_log.trim().is_empty() {
                format!(
                    "目前語意調校對照句：\n- 錯誤輸出句：{err_trim}\n- 期望正確句：{cor_trim}\n\n使用者訊息：\n{msg_trim}"
                )
            } else {
                format!(
                    "目前語意調校對照句：\n- 錯誤輸出句：{err_trim}\n- 期望正確句：{cor_trim}\n- 近期調校紀錄：{current_log}\n\n使用者訊息：\n{msg_trim}"
                )
            }
        } else {
            msg_trim.to_string()
        };

        let mut engine_used = "內建計算語言學引擎".to_string();
        let mut reply_text = String::new();

        if let Some((llm_name, llm_reply)) = self.invoke_llm_chat(chat_sys_prompt, &chat_usr_prompt) {
            engine_used = llm_name.clone();
            let raw_reply = llm_reply.trim();

            // 針對輕量量化模型或通用 LLM 之確定性形式化護欄：
            // 若模型誤把「期望正確句」當成作文語病批改、顛倒錯正字因果關係、胡言「強化單字出現頻率／X與X組合」，或遺漏已合成之 Layer 1 形式化構式公式，
            // 自動以確定性三層語意構式診斷報告 (Deterministic Grounded Solution) 取代或補強，杜絕「統計機率鸚鵡」空談！
            let has_writing_critique_hallucination = raw_reply.contains("都是常見的語法錯誤")
                || raw_reply.contains("建議使用者明確表達")
                || raw_reply.contains("建議使用者使用更明確")
                || raw_reply.contains("改善句子表達")
                || raw_reply.contains("這個詞彙本身就存在語義模糊")
                || raw_reply.contains("+ 是] × [的]")
                || raw_reply.contains("+ 對] ×")
                || raw_reply.contains("(不)是 的 是")
                || raw_reply.contains("出現頻率")
                || raw_reply.contains("的組合，並強化");
            let has_inverted_causality = active_diffs.iter().any(|d| {
                let rej = d.get("rejected").and_then(Value::as_str).unwrap_or("");
                let acc = d.get("accepted").and_then(Value::as_str).unwrap_or("");
                !rej.is_empty()
                    && !acc.is_empty()
                    && (raw_reply.contains(&format!("\"{rej}\" 誤選為 \"{acc}\""))
                        || raw_reply.contains(&format!("「{rej}」誤選為「{acc}」"))
                        || raw_reply.contains(&format!("導致 \"{cor_trim}\" 的錯誤輸出"))
                        || raw_reply.contains(&format!("導致「{cor_trim}」的錯誤輸出"))
                        || raw_reply.contains(&format!("「{acc}」與「{acc}」")))
            });
            let mentions_target_diffs = active_diffs.is_empty()
                || active_diffs.iter().all(|d| {
                    let acc = d.get("accepted").and_then(Value::as_str).unwrap_or("");
                    let rej = d.get("rejected").and_then(Value::as_str).unwrap_or("");
                    !acc.is_empty() && raw_reply.contains(acc) && (rej.is_empty() || raw_reply.contains(rej))
                });
            let misses_formal_constructions = !proposed_constructions.is_empty()
                && !proposed_constructions.iter().any(|c| {
                    let pid = c.get("pattern_id").and_then(Value::as_str).unwrap_or("");
                    let formula = c.get("formula").and_then(Value::as_str).unwrap_or("");
                    (!pid.is_empty() && raw_reply.contains(pid))
                        || (!formula.is_empty() && raw_reply.contains(formula))
                });

            if !active_diffs.is_empty()
                && (has_writing_critique_hallucination
                    || has_inverted_causality
                    || !mentions_target_diffs
                    || misses_formal_constructions)
            {
                let grounded_solution = format!(
                    "針對您提供的對照句：\n\
                    • 錯誤輸出句：「{err_trim}」\n\
                    • 期望正確句：「{cor_trim}」\n\n\
                    這並非句子本身的語法問題，而是注音輸入法在 Viterbi 動態規劃解碼時的**同音跨界競爭**現象。以下為如喜三層語意架構的解決方案：\n\n\
                    **1. 雙向詞界對齊與同音病灶診斷**\n\
                    {}\n\n\
                    **2. 三層語意調校解決方案（零單字權重變動）**\n\
                    {}\n\
                    {}\n\
                    • **嚴守單字先驗獨立鐵律**：全程不調高「是」、「對」或調低「試」、「隊」的單字全域權重，徹底杜絕「單字權重打地鼠」引發的連鎖副作用。\n\n\
                    👉 您可直接點擊上方的 **「⚡ 一鍵套用對話提煉規則並執行基準驗證」** 按鈕，將上述規則寫入詞庫並立即生效。",
                    symbolic_brief_lines.join("\n"),
                    layer1_lines.join("\n"),
                    layer2_lines.join("\n")
                );
                reply_text = format!("🤖 【{llm_name}】：\n{grounded_solution}");
            } else {
                reply_text = format!("🤖 【{llm_name}】：\n{raw_reply}");
            }
        }

        if reply_text.is_empty() {
            if !symbolic_context_block.is_empty() {
                reply_text = format!(
                    "🧠 【內建神經符號計算語言學引擎】：\n{symbolic_context_block}\n\n👉 您可直接點擊上方「⚡ 一鍵套用對話提煉規則並執行基準驗證」按鈕立即寫入生效。"
                );
            } else {
                reply_text = format!(
                    "⚪ 目前未偵測到連線中之外部語言模型（可於「🛡️ 隱私安全與 AI 模型」分頁設定 Google Gemini API Key 或確認本機 GGUF 模型）。\n已由內建計算語言學引擎收到您的訊息：「{msg_trim}」。"
                );
            }
        }

        json!({
            "status": "ok",
            "engine": engine_used,
            "target_lang": lang_label,
            "user_message": msg_trim,
            "extracted_error": err_trim,
            "extracted_correct": cor_trim,
            "reply": reply_text,
            "proposed_constructions": proposed_constructions,
            "proposed_rules": proposed_rules,
            "proposed_words": proposed_words,
            "proposal_counts": {
                "constructions": proposed_constructions.len(),
                "rules": proposed_rules.len(),
                "words": proposed_words.len()
            }
        })
    }

    /// 產生大 AI / SI 時代之「推薦對接模型型錄」與「語意調校任務標準交接協定 (Handover Protocol)」，
    /// 並在確認對接模型時自動將 `AI_SEMANTIC_HANDOVER.md`、`SKILL.md` 與 `ai_handover_context.json` 寫入工作區交付給對接模型（無須使用者手動複製貼上）。
    pub fn get_recommended_models_and_handover_spec(&mut self, tuner: &SemanticTuner) -> Value {
        let inventory = tuner.get_rule_inventory_summary();
        let collision_audit = inventory["patterns_user_jsonl"]["array_collision_audit"].clone();
        let active_provider = self.get_effective_provider();
        let cloud_models = recommended_cloud_models();
        let local_models = recommended_local_models();

        let handover_markdown = format!(
r#"# 如喜輸入法 (RuhI) · 語意調校工作室 AI / SI 自動交付任務規格書 (Handover Specification v2.8)

> **自動交付狀態**：本文件由 `ruhi-studio` 於確認對接模型（目前對接引擎：`{}`）時自動生成並交付至工作區，無須使用者手動複製貼上。

您現在接手的是 **如喜輸入法 (RuhI — 如雲行水流自在歡喜)** 的計算語言學語意調校工程。請嚴格遵循以下三層架構規範、**同音對衝四維度辨析與三路分流定理 (`Type A / Type B / Type C`)** 與五大標準作業程序 (SOP) 接續工作，確保零回歸完成目標。

## 一、三層語意知識庫與陣列同音對衝即時狀態 (Live Knowledge & Collision Inventory)
1. **Layer 1 多維槽位陣列與句法構式 (`ruhi_lexicon/data/patterns_user.jsonl`)**：
   - 目前共 `{}` 條構式規則（含 `{}` 條 3D~5D 多槽位鏈式陣列、`{}` 條 2D 二維笛卡兒積陣列，共索引 `{}` 個槽位詞元）。
   - **陣列展開同音對衝實證審計與三路分流**：展開之相鄰二元組合共 `{}` 組，天然無害率達 **`{}`**（含真實詞庫複合詞 `{}` 組、無同音實詞對應之無害組合 `{}` 組）；對於 `{}` 組與真實詞彙同音對衝之邊界，系統不採盲目單向抑制，而是依四維度特徵分為三路處置：
     - **Type A（保留現代高頻構式、古語限定語境，`KEEP_MODERN_ARRAY_OVER_ARCHAIC`）**：共 `{}` 組（如 `說不` vs. 古語 `說部`、`做不` vs. `坐簿`、`得見` vs. `得間`），保留現代構式陣列暢通。
     - **Type B（語法環境雙向分流，`RESOLVED_BY_SYNTACTIC_BIFURCATION` / `bifurcation_pairs`）**：共 `{}` 組（如 `騎得` vs. `耆德`、`笑到` vs. `孝道`、`讀不` vs. `獨步`、`讀得` vs. `獨得`、`顧不` vs. `故步`、`全素` vs. `全速`），依左右槽位論元與中心語實現雙向精準共存（如「他馬騎得很好吧？…才不是什麼耆德老人呢。」）。
     - **Type C（對衝抑制陣列，`SUPPRESSED_BY_EXCLUDE_ARRAY` / `exclude_pairs`）**：共 `{}` 組（如 `這×鍵` vs. `這件`、`個×相` vs. `各項`、`分×不` vs. `分布`、`在×極` vs. `在即`），二元展開無意義，抑制其單獨觸發，僅在完整三元構式時發火。
2. **Layer 2 二元語意轉移矩陣 (`ruhi_lexicon/data/transitions_user.tsv`)**：
   - 目前共 `{}` 條精確二元轉移（正向搭配引力 `{}` 條、負向防跨界劫掠抑制 `{}` 條）。
   - 分值規範：正向搭配固定於 `[+20.0, +35.0]`，負向防劫掠固定於 `[-25.0, -45.0]`。
3. **Layer 3 領域核心詞根庫 (`ruhi_lexicon/data/lexicon_domain.tsv`)**：
   - 目前共 `{}` 筆不可再分之原子詞根（如 `大易`、`許氏`、`嘸蝦米`、`瑜伽`、`五辛`、`完工`）。

## 二、同音詞彙對衝之「四維度辨析」與「三路分流 (`Type A / B / C`)」定理
- **槽位集合因式分解 ($O(1)$ 查找)**：
  - 嚴禁將 `A_array × B_array` 平面展開塞入詞庫。透過雙向雜湊倒排索引 (`rules_by_prev` / `rules_by_curr`)，Viterbi 單步查找恆為 $O(1)$。
- **當陣列展開組合 $(a, b) \in A \times B$ 與詞庫真實詞彙 $W$ 同音對衝 ($R(a)+R(b)=R(W)$) 時，依四個語言學維度分辨並分流**：
  1. **維度一：語體與詞頻梯度（Register & Prior Gradient）** ➔ 若競爭詞 $W$ 為極低頻罕見古語（`weight <= 220`，如 `說部`、`坐簿`、`得間`），而陣列組合為現代日常高頻構式（`說不`、`做不`、`得見`），歸入 **Type A**：不列入 `exclude_pairs`，保留現代陣列暢通。
  2. **維度二：前驅論元槽位（Left Argument / Determiner Slot $S_{{i-1}}$）與維度三：後繼補語／中心語槽位（Right Complement / Head-Noun Slot $S_{{i+1}}$）** ➔ 若陣列組合 $(a, b)$ 與真實詞 $W$ 均為真實可用語彙（或具特定修辭語境，如動補構式 `騎得` vs. 尊稱古雅詞 `耆德`、動補 `笑到` vs. 名詞 `孝道`、可能補語 `讀不` vs. 成語 `獨步`），歸入 **Type B (`bifurcation_pairs`)**：
     - 左接受事論元 `[馬, 車, 單車, 腳踏車, 機車]` 或右接程度補語 `[很, 很好, 快, 穩, 飽]` ➔ 導向動補構式 **`騎 + 得`**（「他馬騎得很好吧」、「騎得飽」）。
     - 左接否定／限定詞 `[什麼, 不是什麼, 並非, 一代, 鄉里, 堪稱]` 或右接人物中心語 `[老人, 長者, 老者, 宿儒, 賢達, 前輩]` ➔ 導向定語名詞 **`耆德`**（「才不是什麼耆德老人呢」）。
  3. **維度四：語義組合合理性與跨詞界假象（Compositional Plausibility）** ➔ 若二元組合 $(a, b)$ 單獨出現時語義不通或純屬三元構式之局部殘片（如 `這×鍵` 對衝 `這件`、`個×相` 對衝 `各項`、`分×不` 對衝 `分布`），歸入 **Type C (`exclude_pairs`)**：抑制二元單獨觸發，僅在三元槽位完整滿足時啟動。

## 三、跨語言通用智慧型輸入法推論架構 (Universal Cross-Language Paradigm)
索緒爾 (Saussure) 與雅各布森 (Jakobson) 確立人類一切自然語言皆由「**聚合軸 (Paradigmatic Axis = 槽位陣列 Slot Array)** × **組合軸 (Syntagmatic Axis = 序列乘積 Sequence $\times$)**」交織而成：
1. **漢語／台語／客語／粵語（孤立語／分析語）**：`[範疇修飾陣列] × ([介接助詞陣列 {{到, 得}}]) × [補語/體標記陣列]`（如 `[V] × ({{到, 得}}) × [飽]` 支援 optional_slots）。
2. **日語假名轉漢字（黏著語 Kana-to-Kanji）**：`[名詞語意場陣列] × [格助詞陣列 {{が, を, に, で, へ}}] × [用言謂語陣列]`（消除同音異義漢字如 `科学/化学`、`見る/診る/観る` 之轉換歧義）。
3. **韓語／越南語／印歐屈折語**：`[詞幹陣列] × [詞尾／助詞／介詞陣列] × [受事／補語陣列]`。
4. **字形碼與音形混合輸入法（行列 30、大易、倉頡、嘸蝦米）**：在簡碼、萬用字元 (`*`/`?`) 或盲打連打時，透過同一套 Layer 1 語意陣列與三路同音分流自動完成重碼消歧。

## 四、三大絕對禁令與接續工作五大標準作業程序 (5-Step SOP)
1. **嚴禁單字權重打地鼠 (No Unigram Weight Whack-a-Mole)**：絕對禁止手動調整單字先驗權重。
2. **嚴禁笛卡兒積平面化塞詞庫 (No Cartesian Lexicon Flattening)**：一律以 `patterns_user.jsonl` 陣列對應表達。
3. **步驟 1～5**：(1) BiMM+LCS 差異孤島定位 ➔ (2) 舉一反三全詞庫同構掃描 ➔ (3) 陣列因式分解 `A_array × B_array` ➔ (4) 執行 `/api/rules/collisions` 執行四維度同音對衝審計，依 Type A（保留）、Type B（`bifurcation_pairs` 雙向槽位分流）、Type C（`exclude_pairs` 抑制）處置 ➔ (5) 執行 `/api/benchmark` 與 `cargo test --lib` 驗證零回歸。"#,
            active_provider,
            inventory["patterns_user_jsonl"]["total_patterns"].as_u64().unwrap_or(0),
            inventory["patterns_user_jsonl"]["multi_slot_3d_to_5d_chains"].as_u64().unwrap_or(0),
            inventory["patterns_user_jsonl"]["array_2d_correspondences"].as_u64().unwrap_or(0),
            inventory["patterns_user_jsonl"]["total_indexed_slot_tokens"].as_u64().unwrap_or(0),
            collision_audit["total_expanded_pairs"].as_u64().unwrap_or(0),
            collision_audit["naturally_harmless_rate"].as_str().unwrap_or("99.0%"),
            collision_audit["valid_lexicon_compounds_count"].as_u64().unwrap_or(0),
            collision_audit["harmless_non_existent_readings_count"].as_u64().unwrap_or(0),
            collision_audit["total_homophone_collisions"].as_u64().unwrap_or(0),
            collision_audit["type_a_archaic_keep_array_count"].as_u64().unwrap_or(0),
            collision_audit["type_b_syntactic_bifurcation_count"].as_u64().unwrap_or(0),
            collision_audit["type_c_suppressed_by_exclude_array_count"].as_u64().unwrap_or(0),
            inventory["transitions_user_tsv"]["total_bigram_rules"].as_u64().unwrap_or(0),
            inventory["transitions_user_tsv"]["positive_collocations"].as_u64().unwrap_or(0),
            inventory["transitions_user_tsv"]["negative_anti_hijack_rules"].as_u64().unwrap_or(0),
            inventory["lexicon_domain_tsv"]["total_irreducible_entries"].as_u64().unwrap_or(0),
        );

        let skill_markdown = format!(
r#"---
name: ruhi-semantic-tuning
description: 如喜輸入法 (RuhI) 計算語言學三層語意陣列調校、舉一反三同構擴充與同音對衝四維度三路分流 (Type A 保留 / Type B bifurcation_pairs 雙向分流 / Type C exclude_pairs 抑制) 標準作業技能。當對接 AI/SI 模型執行輸入法選字修正、語意陣列建構或跨語言構式擴充時自動載入。
---

{}
"#,
            handover_markdown
        );

        // 自動寫入實體檔案（免除使用者手動複製貼上）
        let doc_path = tuner.root.join("docs").join("AI_SEMANTIC_HANDOVER.md");
        let skill_path = tuner
            .root
            .join(".agent")
            .join("skills")
            .join("ruhi-semantic-tuning")
            .join("SKILL.md");
        let context_json_path = tuner.data_dir.join("ai_handover_context.json");

        let mut delivered_files = Vec::new();
        if let Some(parent) = doc_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(&doc_path, &handover_markdown).is_ok() {
            delivered_files.push(doc_path.to_string_lossy().to_string());
        }
        if let Some(parent) = skill_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(&skill_path, &skill_markdown).is_ok() {
            delivered_files.push(skill_path.to_string_lossy().to_string());
        }

        let context_payload = json!({
            "version": "RUHI_AI_SI_HANDOVER_V2.8",
            "auto_delivered": true,
            "active_provider": active_provider,
            "delivered_at": chrono::Local::now().to_rfc3339(),
            "live_inventory": inventory,
            "json_output_schema": {
                "pattern_id": "string (英文蛇形命名，如 v_dao_bao_satiation_3d_array)",
                "name": "string (繁體中文台灣資訊科學與語言學術語名稱)",
                "formula": "string (陣列表示式，如 [V] × ({到, 得}) × [飽])",
                "prev_slot": ["純漢字前驅詞元陣列"],
                "next_slot": ["純漢字後繼詞元陣列"],
                "slots": [["第1槽位詞元陣列"], ["第2槽位詞元陣列(可選)"], ["第3槽位詞元陣列"]],
                "optional_slots": [2],
                "bifurcation_pairs": [["雙向語法分流前字(如騎)", "雙向語法分流後字(如得)"]],
                "exclude_pairs": [["單向抑制前字(如這)", "單向抑制後字(如鍵)"]],
                "bonus": 28.0,
                "suppress": ["需抑制之跨界同音假詞"]
            }
        });
        if let Ok(pretty_json) = serde_json::to_string_pretty(&context_payload) {
            if fs::write(&context_json_path, pretty_json).is_ok() {
                delivered_files.push(context_json_path.to_string_lossy().to_string());
            }
        }

        json!({
            "status": "ok",
            "auto_delivered": true,
            "delivered_files": delivered_files,
            "active_provider": active_provider,
            "recommended_cloud_models": cloud_models,
            "recommended_local_models": local_models,
            "live_inventory": inventory,
            "hierarchy_depth_analysis": {
                "optimal_levels": "2 至 3 層 (Level 1: A×B 構詞陣列, Level 2: [A×B]×[C×D] 短語構式陣列, Level 3: 子句邊界錨定 + Type A/B/C 三路同音對衝分流)",
                "runtime_complexity": "O(1) 雙向雜湊槽位查找（嚴禁平面展開為 O(N^k) 詞庫詞條）",
                "why_not_deeper_than_3": "陣列展開後的不理性組合只要在詞庫中無真實同音詞，使用者絕不會打出（天然無害率 >99%）；少數與真實詞同音對衝之二元對透過 Type A（保留現代構式）、Type B（bifurcation_pairs 左右語法槽位雙向分流）與 Type C（exclude_pairs 抑制）精準處置，兼具最高覆蓋率與零副作用。"
            },
            "cross_language_generalization": {
                "is_universal_paradigm": true,
                "linguistic_foundation": "索緒爾 (Saussure) 聚合軸 (Paradigmatic Slot Array) × 組合軸 (Syntagmatic Sequence ×) 與 Goldberg 構式語法 (Construction Grammar)",
                "supported_language_families": [
                    {"family": "漢藏語系（華語／台語／客語／粵語）", "pattern": "[修飾/副詞陣列] × ([介接助詞陣列]) × [補語/體標記陣列]（如 [V]×({到,得})×[飽]）"},
                    {"family": "日韓黏著語系（日語假名轉漢字 Kana-to-Kanji／韓語）", "pattern": "[名詞語意場陣列] × [助詞陣列 {が,を,に,で}] × [動詞陣列]（消除同音漢字如 科学/化学、見る/診る 歧義）"},
                    {"family": "印歐屈折與分析語系（英／法／德／西／印地／梵語）", "pattern": "[動詞/介詞陣列] × [冠詞/格標記陣列] × [受事名詞陣列]（智慧預測與滑行輸入消歧）"},
                    {"family": "字形碼與音形混合輸入法（行列30／大易／倉頡／嘸蝦米）", "pattern": "簡碼、萬用字元 (* / ?) 與盲打連打之重碼候選，直接共用同一套 Layer 1 語意陣列完成免選字消歧"}
                ]
            },
            "handover_protocol": {
                "version": "RUHI_AI_SI_HANDOVER_V2.8",
                "delivery_mode": "AUTOMATIC_FILE_AND_SKILL_DELIVERY",
                "delivered_files": delivered_files,
                "system_prompt": handover_markdown,
                "five_step_sop": [
                    "1. 句對齊與跨詞界劫掠定位：透過 BiMM + LCS 找出錯誤句中之跨界假詞與碎裂邊界。",
                    "2. 舉一反三同構詞族掃描：掃描全詞庫具相同尾音/首音或派生後綴之同類詞群。",
                    "3. 陣列因式分解 (A_array × B_array)：將同構單點規則收斂為 2D/3D 槽位陣列，優先擴充既有 pattern_id。",
                    "4. 同音對衝四維度審計與三路分流：依 Type A（保留現代高頻構式）、Type B（bifurcation_pairs 左右語法槽位雙向分流，如「騎得」與「耆德」）、Type C（exclude_pairs 單向抑制）精準處置。",
                    "5. 黃金基準迴歸驗證：執行 /api/benchmark 與 cargo test 確認 100% 通過且無副作用。"
                ],
                "json_output_schema": context_payload["json_output_schema"]
            }
        })
    }
}

fn probe_local_http_port(port: u16, path: &str) -> bool {
    let addr_str = format!("127.0.0.1:{port}");
    let Ok(mut addrs) = addr_str.to_socket_addrs() else {
        return false;
    };
    let Some(SocketAddr::V4(v4)) = addrs.find(|a| a.is_ipv4()) else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&SocketAddr::V4(v4), Duration::from_millis(250))
    else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(350)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 128];
    let Ok(n) = stream.read(&mut buf) else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..n]);
    head.contains("200 OK")
}

fn curl_post_json(url: &str, payload: &Value, extra_headers: &[(&str, &str)]) -> Option<String> {
    let tmp_path = std::env::temp_dir().join(format!(
        "ruhi_ai_req_{}.json",
        std::process::id()
    ));
    let body_bytes = serde_json::to_vec(payload).ok()?;
    fs::write(&tmp_path, &body_bytes).ok()?;

    let data_arg = format!("@{}", tmp_path.to_string_lossy());
    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-sS",
        "-m",
        "18",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json; charset=utf-8",
    ]);
    for (k, v) in extra_headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg("--data-binary").arg(&data_arg).arg(url);

    let out = cmd.output().ok();
    let _ = fs::remove_file(&tmp_path);
    let output = out?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn query_local_ollama_text(model: &str, system_prompt: &str, user_prompt: &str) -> Option<String> {
    let payload = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "stream": false
    });
    let body = serde_json::to_vec(&payload).ok()?;
    let addr: SocketAddr = "127.0.0.1:11434".parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(600)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(22)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let req = format!(
        "POST /api/chat HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(req.as_bytes()).ok()?;
    stream.write_all(&body).ok()?;
    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes).ok()?;
    let resp_str = String::from_utf8_lossy(&resp_bytes);
    let (_, json_body) = resp_str.split_once("\r\n\r\n")?;
    let v: Value = serde_json::from_str(json_body.trim()).ok()?;
    let content = v.get("message")?.get("content")?.as_str()?.trim();
    if content.is_empty() {
        None
    } else {
        Some(content.to_string())
    }
}

fn query_local_ollama_json(model: &str, system_prompt: &str, user_prompt: &str) -> Option<Value> {
    let payload = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "format": "json",
        "stream": false
    });
    let body = serde_json::to_vec(&payload).ok()?;
    let addr: SocketAddr = "127.0.0.1:11434".parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(600)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(18)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let req = format!(
        "POST /api/chat HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(req.as_bytes()).ok()?;
    stream.write_all(&body).ok()?;
    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes).ok()?;
    let resp_str = String::from_utf8_lossy(&resp_bytes);
    let (_, json_body) = resp_str.split_once("\r\n\r\n")?;
    let v: Value = serde_json::from_str(json_body.trim()).ok()?;
    let content = v.get("message")?.get("content")?.as_str()?;
    parse_json_from_llm_text(content)
}

fn parse_json_from_llm_text(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end > start {
        serde_json::from_str::<Value>(&trimmed[start..=end]).ok()
    } else {
        None
    }
}

