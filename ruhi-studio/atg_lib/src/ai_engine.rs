//! RuhOS 通用本地 AI 熱插拔子系統 —— 純綠免安裝版 `llama.cpp` 架構 (`ai_engine.rs`)
//!
//! 徹底揚棄夾帶商業訂閱誘導與背景常駐服務的 Ollama，全面採用 **純綠免安裝版 `llama.cpp` (`llama-server` / `.gguf`)**。
//!
//! ### 核心設計原則
//! 1. **純綠可攜、零訂閱誘導、零背景殘留**：
//!    直接掃描本地資料夾中的 `.gguf` 模型權重檔與綠色版 `llama-server` 執行檔，或連線至本機純淨 `llama-server`（預設 `http://127.0.0.1:8080`）。
//!    支援由應用程式按需啟動子程序 (`spawn_green_server`)，並在退出時自動終止、完整釋放顯示卡視訊記憶體 (VRAM)。
//! 2. **Google 技術生態絕對優先**：
//!    於模型自動遴選時，第一優先選擇 Google `Gemma`（Gemma 3 / Gemma 4）與 `PaliGemma` 系列 `.gguf` 模型，
//!    第二順位為歐美開源模型（Llama、Mistral），排除具審查偏見之來源。
//! 3. **本地顯示卡硬體感知 (`probe_hardware_gpu`)**：
//!    透過 `nvidia-smi` 即時偵測本地 GPU 型號、總視訊記憶體與可用視訊記憶體，據以計算建議載入層數 (`-ngl`)。
//! 4. **100% 離線優雅降級**：
//!    採用純 Rust 標準庫 `std::net::TcpStream` 實作 `/health`、`/v1/models` 與 `/v1/chat/completions` 通訊，
//!    無任何外部重型網路套件依賴；當無模型或服務未啟動時，立即回退至 `AIStatus::Offline` 純演算法模式。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// 本地 AI 子系統運作狀態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AIStatus {
    /// 離線（未啟動 `llama-server`，進入純演算法備援模式）
    Offline,
    /// 正在載入 `.gguf` 模型或連線探測中
    Connecting,
    /// 本地純綠 `llama-server` 在線就緒
    Online,
    /// 模型正在執行推論運算中
    Busy,
}

/// 本地 GGUF 模型能力分類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelCapability {
    /// 視覺多模態理解 (搭配 `mmproj-*.gguf` 之 Gemma 3/4、PaliGemma、LLaVA)
    Vision,
    /// 一般文字推理與對話
    Chat,
    /// 程式碼生成與分析
    Code,
    /// 向量嵌入 (`--embedding`)
    Embed,
}

/// 本地 `.gguf` 模型資訊與能力評級
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelInfo {
    /// 模型識別名稱或檔名（如 `"gemma-3-12b-it-Q4_K_M.gguf"`）
    pub name: String,
    /// 本地 `.gguf` 實體檔案路徑（若由本地目錄掃描取得）
    pub gguf_path: Option<PathBuf>,
    /// 多模態視覺投影檔路徑 (`mmproj-*.gguf`)
    pub mmproj_path: Option<PathBuf>,
    /// 模型檔案大小（位元組）
    pub size_bytes: u64,
    /// 參數量標記（如 `"12B"`、`"8B"`、`"4B"`）
    pub parameter_size: String,
    /// 量化等級標記（如 `"Q4_K_M"`、`"Q8_0"`）
    pub quantization: String,
    /// 支援之能力清單
    pub capabilities: Vec<ModelCapability>,
    /// 是否正由 `llama-server` 載入駐留於視訊記憶體中
    pub is_active_in_vram: bool,
}

impl LocalModelInfo {
    /// 模型檔案大小（GB）
    pub fn size_gb(&self) -> f64 {
        (self.size_bytes as f64) / (1024.0 * 1024.0 * 1024.0)
    }

    /// 是否具備視覺多模態能力
    pub fn has_vision(&self) -> bool {
        self.capabilities.contains(&ModelCapability::Vision)
    }
}

/// 本地顯示卡硬體與視訊記憶體資訊
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuHardwareInfo {
    pub gpu_name: String,
    pub vram_total_mb: u64,
    pub vram_free_mb: u64,
    pub has_nvidia: bool,
}

/// AI 查詢回應結構
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIQueryResult {
    pub ok: bool,
    pub model: String,
    pub text: String,
    pub data: Option<Value>,
    pub elapsed_sec: f64,
    pub error: Option<String>,
}

/// RuhOS 純綠版 `llama.cpp` 本地 AI 引擎中樞
pub struct RuhAIEngine {
    /// `llama-server` HTTP 端點（預設 `http://127.0.0.1:8080`）
    pub endpoint: String,
    /// 純綠版 `llama-server.exe` 執行檔搜尋路徑清單
    pub llama_bin_dirs: Vec<PathBuf>,
    /// 本地 `.gguf` 模型權重檔搜尋目錄清單
    pub model_dirs: Vec<PathBuf>,
    pub status: AIStatus,
    pub active_vlm_model: Option<String>,
    pub active_chat_model: Option<String>,
    pub available_models: HashMap<String, LocalModelInfo>,
    pub gpu_info: GpuHardwareInfo,
    last_probe: Option<Instant>,
    managed_server: Option<Child>,
}

impl Default for RuhAIEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuhAIEngine {
    /// 建立純綠版 `llama.cpp` AI 引擎（預設連接 `RUH_LLAMA_ENDPOINT` 或 `http://127.0.0.1:8080`）。
    pub fn new() -> Self {
        let endpoint = std::env::var("RUH_LLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
            .trim_end_matches('/')
            .to_string();

        let mut llama_bin_dirs = Vec::new();
        if let Ok(custom_bin) = std::env::var("RUH_LLAMA_DIR") {
            llama_bin_dirs.push(PathBuf::from(custom_bin));
        }
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            llama_bin_dirs.push(
                PathBuf::from(&local_app)
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Packages")
                    .join("ggml.llamacpp_Microsoft.Winget.Source_8wekyb3d8bbwe"),
            );
        }
        llama_bin_dirs.push(PathBuf::from(r"C:\ATGprjs\_llama"));
        llama_bin_dirs.push(PathBuf::from("./_llama"));
        llama_bin_dirs.push(PathBuf::from("./bin"));

        let mut model_dirs = Vec::new();
        if let Ok(custom_models) = std::env::var("RUH_MODELS_DIR") {
            model_dirs.push(PathBuf::from(custom_models));
        }
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            model_dirs.push(PathBuf::from(&local_app).join("RuhI").join("models").join("gemma"));
        }
        model_dirs.push(PathBuf::from(r"C:\ATGprjs\_models\gemma"));
        model_dirs.push(PathBuf::from(r"C:\ATGprjs\_models"));
        model_dirs.push(PathBuf::from(r"C:\ATGprjs\_llama\models"));
        model_dirs.push(PathBuf::from("./models"));

        Self {
            endpoint,
            llama_bin_dirs,
            model_dirs,
            status: AIStatus::Offline,
            active_vlm_model: None,
            active_chat_model: None,
            available_models: HashMap::new(),
            gpu_info: GpuHardwareInfo {
                gpu_name: "Unknown".to_string(),
                ..Default::default()
            },
            last_probe: None,
            managed_server: None,
        }
    }

    /// 探測本機 NVIDIA GPU 與視訊記憶體容量。
    pub fn probe_hardware_gpu(&mut self) -> &GpuHardwareInfo {
        if let Ok(output) = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total,memory.free",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = stdout.lines().next() {
                    let parts: Vec<&str> = first_line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 3 {
                        self.gpu_info = GpuHardwareInfo {
                            gpu_name: parts[0].to_string(),
                            vram_total_mb: parts[1].parse().unwrap_or(0),
                            vram_free_mb: parts[2].parse().unwrap_or(0),
                            has_nvidia: true,
                        };
                    }
                }
            }
        }
        &self.gpu_info
    }

    /// 掃描本地綠色模型目錄中之 `.gguf` 檔案，自動解析參數量、量化等級與多模態投影檔 (`mmproj`)。
    pub fn scan_local_gguf_models(&mut self) {
        let mut gguf_files: Vec<PathBuf> = Vec::new();
        let mut mmproj_files: Vec<PathBuf> = Vec::new();

        for dir in &self.model_dirs {
            if !dir.exists() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if !fname.to_ascii_lowercase().ends_with(".gguf") {
                        continue;
                    }
                    if fname.to_ascii_lowercase().contains("mmproj") {
                        mmproj_files.push(path);
                    } else {
                        gguf_files.push(path);
                    }
                }
            }
        }

        for path in gguf_files {
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let mut info = parse_gguf_filename_metadata(fname, Some(path.clone()), size_bytes);

            // 若同目錄存在對應之 mmproj 視覺投影檔，自動賦予 Vision 能力
            let stem_prefix = fname.split('-').next().unwrap_or("").to_ascii_lowercase();
            if let Some(mm) = mmproj_files.iter().find(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase().contains(&stem_prefix))
                    .unwrap_or(false)
            }) {
                info.mmproj_path = Some(mm.clone());
                if !info.capabilities.contains(&ModelCapability::Vision) {
                    info.capabilities.push(ModelCapability::Vision);
                }
            }

            self.available_models.insert(fname.to_string(), info);
        }
    }

    /// 尋找本地純綠版 `llama-server`（Windows 下為 `llama-server.exe`）執行檔路徑。
    pub fn find_green_llama_server_bin(&self) -> Option<PathBuf> {
        let bin_name = if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        for dir in &self.llama_bin_dirs {
            let candidate = dir.join(bin_name);
            if candidate.exists() && candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// 按需啟動純綠版 `llama-server` 子程序載入指定 `.gguf` 模型（無背景常駐污染，隨應用程式退出自動回收）。
    pub fn spawn_green_server(
        &mut self,
        gguf_path: impl AsRef<Path>,
        port: u16,
        gpu_layers: i32,
        ctx_size: u32,
    ) -> Result<(), String> {
        self.stop_managed_server();

        let bin = self
            .find_green_llama_server_bin()
            .ok_or_else(|| "找不到純綠版 llama-server 執行檔".to_string())?;
        let model_path = gguf_path.as_ref();
        if !model_path.exists() {
            return Err(format!("找不到 GGUF 模型檔案: {}", model_path.display()));
        }

        let mut cmd = Command::new(bin);
        cmd.arg("-m")
            .arg(model_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-ngl")
            .arg(gpu_layers.to_string())
            .arg("-c")
            .arg(ctx_size.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| format!("啟動純綠版 llama-server 失敗: {}", e))?;
        self.managed_server = Some(child);
        self.endpoint = format!("http://127.0.0.1:{}", port);
        self.status = AIStatus::Connecting;
        Ok(())
    }

    /// 終止由本引擎啟動之純綠版 `llama-server` 子程序並立即釋放 VRAM。
    pub fn stop_managed_server(&mut self) {
        if let Some(mut child) = self.managed_server.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// 即時探測本地純綠 `llama-server` 連線狀態（`/health` 與 `/v1/models`）及本地 `.gguf` 模型清單。
    pub fn refresh(&mut self, force: bool) -> AIStatus {
        if !force {
            if let Some(last) = self.last_probe {
                if last.elapsed() < Duration::from_secs(3) {
                    return self.status;
                }
            }
        }
        self.last_probe = Some(Instant::now());
        self.probe_hardware_gpu();
        self.scan_local_gguf_models();

        // 1. 探測 llama-server /health 端點
        let Ok(health_body) = http_get(&self.endpoint, "/health", Duration::from_millis(600))
        else {
            self.status = AIStatus::Offline;
            self.select_best_models();
            return self.status;
        };

        if let Ok(h_json) = serde_json::from_str::<Value>(&health_body) {
            let st_str = h_json.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if st_str == "loading model" {
                self.status = AIStatus::Connecting;
                return self.status;
            }
        }

        // 2. 探測 llama-server /v1/models 取得目前駐留 VRAM 的活躍模型
        if let Ok(models_body) = http_get(&self.endpoint, "/v1/models", Duration::from_millis(600))
        {
            if let Ok(m_json) = serde_json::from_str::<Value>(&models_body) {
                if let Some(arr) = m_json.get("data").and_then(|v| v.as_array()) {
                    for item in arr {
                        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                            let fname = Path::new(id)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or(id);
                            let entry = self
                                .available_models
                                .entry(fname.to_string())
                                .or_insert_with(|| parse_gguf_filename_metadata(fname, None, 0));
                            entry.is_active_in_vram = true;
                        }
                    }
                }
            }
        }

        self.status = AIStatus::Online;
        self.select_best_models();
        self.status
    }

    fn select_best_models(&mut self) {
        let mut vlm_candidates = Vec::new();
        let mut chat_candidates = Vec::new();
        let mut active_vram = std::collections::HashSet::new();

        for (name, info) in &self.available_models {
            if info.is_active_in_vram {
                active_vram.insert(name.clone());
            }
            if info.has_vision() {
                vlm_candidates.push(name.clone());
            }
            chat_candidates.push(name.clone());
        }

        self.active_vlm_model = select_preferred_model(&vlm_candidates, &active_vram);
        self.active_chat_model = self
            .active_vlm_model
            .clone()
            .or_else(|| select_preferred_model(&chat_candidates, &active_vram));
    }

    /// 檢查本地純綠 `llama-server` 是否就緒可用。
    pub fn is_available(&mut self) -> bool {
        if self.status != AIStatus::Online {
            self.refresh(false);
        }
        self.status == AIStatus::Online
    }

    /// 取得用於介面狀態列顯示之精簡狀態摘要標籤與語意色碼。
    pub fn get_status_badge(&self) -> (String, &'static str) {
        match self.status {
            AIStatus::Offline => (
                "⚪ 純綠 Llama: 離線 (純演算法模式)".to_string(),
                "#718096",
            ),
            AIStatus::Connecting => (
                "🟡 純綠 Llama: 正在載入 GGUF 模型...".to_string(),
                "#ecc94b",
            ),
            AIStatus::Busy => {
                let m = self.active_chat_model.as_deref().unwrap_or("GGUF");
                (format!("🟡 純綠 Llama 推論中: {}...", m), "#ecc94b")
            }
            AIStatus::Online => {
                let m = self
                    .active_vlm_model
                    .as_deref()
                    .or(self.active_chat_model.as_deref())
                    .unwrap_or("llama-server");
                let vram_tag = if self.gpu_info.has_nvidia {
                    format!(" ({}GB VRAM)", self.gpu_info.vram_total_mb / 1024)
                } else {
                    String::new()
                };
                (format!("🟢 純綠 Llama 就緒: {}{}", m, vram_tag), "#48bb78")
            }
        }
    }

    /// 透過純綠 `llama-server` 原生 `/v1/chat/completions` 執行文字推理與對話查詢。
    pub fn chat_query(
        &mut self,
        prompt: &str,
        system_prompt: &str,
        model_override: Option<&str>,
        as_json: bool,
        timeout: Duration,
    ) -> AIQueryResult {
        let model = model_override
            .map(|s| s.to_string())
            .or_else(|| self.active_chat_model.clone())
            .unwrap_or_else(|| "default".to_string());

        let start = Instant::now();
        self.status = AIStatus::Busy;

        let mut messages = Vec::new();
        let combined_user_content = if !system_prompt.is_empty() {
            format!("{system_prompt}\n\n{prompt}")
        } else {
            prompt.to_string()
        };
        messages.push(serde_json::json!({
            "role": "user",
            "content": combined_user_content
        }));

        let mut payload = serde_json::json!({
            "model": model,
            "messages": messages,
            "temperature": 0.2,
            "stream": false
        });
        if as_json {
            payload["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        let resp = http_post(
            &self.endpoint,
            "/v1/chat/completions",
            &payload.to_string(),
            timeout,
        );
        self.status = AIStatus::Online;
        let elapsed_sec = start.elapsed().as_secs_f64();

        match resp {
            Ok(raw_resp) => {
                if let Ok(json_resp) = serde_json::from_str::<Value>(&raw_resp) {
                    let raw_text = json_resp
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| c0.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let parsed_data = if as_json {
                        serde_json::from_str::<Value>(&raw_text).ok()
                    } else {
                        None
                    };
                    AIQueryResult {
                        ok: true,
                        model,
                        text: raw_text,
                        data: parsed_data,
                        elapsed_sec,
                        error: None,
                    }
                } else {
                    AIQueryResult {
                        ok: false,
                        model,
                        text: String::new(),
                        data: None,
                        elapsed_sec,
                        error: Some("llama-server 回應 JSON 解析失敗".to_string()),
                    }
                }
            }
            Err(e) => {
                self.status = AIStatus::Offline;
                AIQueryResult {
                    ok: false,
                    model,
                    text: String::new(),
                    data: None,
                    elapsed_sec,
                    error: Some(e),
                }
            }
        }
    }

    /// 透過純綠 `llama-server` `/v1/chat/completions` 執行多模態視覺查詢（輸入 Base64 PNG/JPEG 與提示詞）。
    pub fn vision_query(
        &mut self,
        image_base64: &str,
        prompt: &str,
        system_prompt: &str,
        as_json: bool,
        timeout: Duration,
    ) -> AIQueryResult {
        let model = self
            .active_vlm_model
            .clone()
            .or_else(|| self.active_chat_model.clone())
            .unwrap_or_else(|| "default".to_string());

        let start = Instant::now();
        self.status = AIStatus::Busy;

        let data_uri = if image_base64.starts_with("data:image/") {
            image_base64.to_string()
        } else {
            format!("data:image/png;base64,{}", image_base64)
        };

        let mut messages = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system_prompt
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": data_uri } },
                { "type": "text", "text": prompt }
            ]
        }));

        let mut payload = serde_json::json!({
            "model": model,
            "messages": messages,
            "temperature": 0.1,
            "stream": false
        });
        if as_json {
            payload["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        let resp = http_post(
            &self.endpoint,
            "/v1/chat/completions",
            &payload.to_string(),
            timeout,
        );
        self.status = AIStatus::Online;
        let elapsed_sec = start.elapsed().as_secs_f64();

        match resp {
            Ok(raw_resp) => {
                if let Ok(json_resp) = serde_json::from_str::<Value>(&raw_resp) {
                    let raw_text = json_resp
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| c0.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let parsed_data = if as_json {
                        serde_json::from_str::<Value>(&raw_text).ok()
                    } else {
                        None
                    };
                    AIQueryResult {
                        ok: true,
                        model,
                        text: raw_text,
                        data: parsed_data,
                        elapsed_sec,
                        error: None,
                    }
                } else {
                    AIQueryResult {
                        ok: false,
                        model,
                        text: String::new(),
                        data: None,
                        elapsed_sec,
                        error: Some("llama-server 視覺查詢回應解析失敗".to_string()),
                    }
                }
            }
            Err(e) => {
                self.status = AIStatus::Offline;
                AIQueryResult {
                    ok: false,
                    model,
                    text: String::new(),
                    data: None,
                    elapsed_sec,
                    error: Some(e),
                }
            }
        }
    }
}

impl Drop for RuhAIEngine {
    fn drop(&mut self) {
        self.stop_managed_server();
    }
}

/// 從 `.gguf` 檔案名稱解析模型參數量、量化格式與能力標籤。
pub fn parse_gguf_filename_metadata(
    filename: &str,
    gguf_path: Option<PathBuf>,
    size_bytes: u64,
) -> LocalModelInfo {
    let lower = filename.to_ascii_lowercase();
    let mut caps = vec![ModelCapability::Chat];
    if ["gemma-3", "gemma-4", "gemma3", "gemma4", "paligemma", "llava", "vision"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        caps.push(ModelCapability::Vision);
    }
    if ["code", "coder", "starcoder"].iter().any(|kw| lower.contains(kw)) {
        caps.push(ModelCapability::Code);
    }

    let mut param_size = String::new();
    let mut quant = String::new();
    for part in filename.trim_end_matches(".gguf").split(['-', '_']) {
        let up = part.to_ascii_uppercase();
        if param_size.is_empty()
            && up.ends_with('B')
            && up[..up.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
        {
            param_size = up.clone();
        }
        if quant.is_empty()
            && (up.starts_with("Q4")
                || up.starts_with("Q5")
                || up.starts_with("Q6")
                || up.starts_with("Q8")
                || up.starts_with("IQ4")
                || up == "F16"
                || up == "BF16")
        {
            quant = up;
        }
    }

    LocalModelInfo {
        name: filename.to_string(),
        gguf_path,
        mmproj_path: None,
        size_bytes,
        parameter_size: param_size,
        quantization: quant,
        capabilities: caps,
        is_active_in_vram: false,
    }
}

/// 依 ATG 技術來源優先序遴選最佳模型：
/// 1. 已駐留顯存之 Google Gemma / PaliGemma 模型
/// 2. 已駐留顯存之歐美開源模型 (Llama / Mistral)
/// 3. 本地目錄中之 Google Gemma / PaliGemma `.gguf`
/// 4. 本地目錄中之 Llama / Mistral `.gguf`
pub fn select_preferred_model(
    candidates: &[String],
    active_vram: &std::collections::HashSet<String>,
) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    if let Some(m) = candidates
        .iter()
        .find(|m| active_vram.contains(*m) && m.to_ascii_lowercase().contains("gemma"))
    {
        return Some(m.clone());
    }
    if let Some(m) = candidates.iter().find(|m| active_vram.contains(*m)) {
        return Some(m.clone());
    }
    if let Some(m) = candidates
        .iter()
        .find(|m| m.to_ascii_lowercase().contains("gemma"))
    {
        return Some(m.clone());
    }
    if let Some(m) = candidates
        .iter()
        .find(|m| m.to_ascii_lowercase().contains("llama") || m.to_ascii_lowercase().contains("mistral"))
    {
        return Some(m.clone());
    }
    candidates.first().cloned()
}

fn parse_host_port(endpoint: &str) -> Result<(String, SocketAddr), String> {
    let stripped = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    let host_port = stripped.split('/').next().unwrap_or("127.0.0.1:8080");
    let addr_str = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{}:80", host_port)
    };
    let addr = addr_str
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "無法解析 llama-server 主機位址".to_string())?;
    Ok((host_port.to_string(), addr))
}

fn http_get(endpoint: &str, path: &str, timeout: Duration) -> Result<String, String> {
    let (host, addr) = parse_host_port(endpoint)?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: RuhOS-GreenLlama/1.0\r\nConnection: close\r\n\r\n",
        path, host
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut resp = String::new();
    stream.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    extract_http_body(&resp)
}

fn http_post(
    endpoint: &str,
    path: &str,
    json_body: &str,
    timeout: Duration,
) -> Result<String, String> {
    let (host, addr) = parse_host_port(endpoint)?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: RuhOS-GreenLlama/1.0\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        json_body.len(),
        json_body
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut resp = String::new();
    stream.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    extract_http_body(&resp)
}

fn extract_http_body(raw_http: &str) -> Result<String, String> {
    let Some((header, body)) = raw_http.split_once("\r\n\r\n") else {
        return Err("無效的 HTTP 回應格式".to_string());
    };
    let first_line = header.lines().next().unwrap_or("");
    if !first_line.contains("200") {
        return Err(format!("HTTP 狀態非 200: {}", first_line));
    }
    Ok(body.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_green_llama_offline_fallback_and_gguf_metadata_parser() {
        let mut engine = RuhAIEngine::new();
        engine.endpoint = "http://127.0.0.1:59998".to_string();
        let st = engine.refresh(true);
        assert_eq!(st, AIStatus::Offline);
        let (badge, _) = engine.get_status_badge();
        assert!(badge.contains("純綠 Llama"));

        let meta = parse_gguf_filename_metadata(
            "gemma-3-12b-it-Q4_K_M.gguf",
            None,
            7_500_000_000,
        );
        assert_eq!(meta.parameter_size, "12B");
        assert_eq!(meta.quantization, "Q4");
        assert!(meta.has_vision());

        let candidates = vec![
            "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf".to_string(),
            "gemma-3-12b-it-Q4_K_M.gguf".to_string(),
            "Mistral-7B-Instruct-v0.3-Q5_K_M.gguf".to_string(),
        ];
        let active = std::collections::HashSet::new();
        assert_eq!(
            select_preferred_model(&candidates, &active),
            Some("gemma-3-12b-it-Q4_K_M.gguf".to_string())
        );
    }
}
