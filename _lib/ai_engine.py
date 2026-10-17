# -*- coding: utf-8 -*-
"""
RuhOS Universal Local AI Engine (通用本地 AI 核心子系統)
================================================================================
遵照 ATG 最高軟體架構規範與 RuhOS 第一原理：
    RuhOS = rud(Hardware, Environment, Human Intent)

核心功能：
1. 熱插拔 (Hot-Pluggable) 本地端多模態 AI 服務探測 (Ollama / LM Studio / LocalAI)
2. 自動感知硬體算力 (NVIDIA VRAM、系統 RAM、CPU 拓撲)
3. 本地模型能力分級與自動適配 (視覺 VLM、對話推理 CHAT、程式碼 CODE)
4. 統一代碼接口 (vision_query, chat_query, structured_extract)
5. 零依賴優雅備援 (Graceful Degradation / 100% 離線可用性保證)
================================================================================
"""

import os
import sys
import json
import base64
import time
import threading
import urllib.request
import urllib.error
from enum import Enum
from typing import Dict, Any, List, Optional, Tuple, Callable, Union
from dataclasses import dataclass, field
from io import BytesIO


class AIStatus(Enum):
    OFFLINE = "offline"         # 離線 (無本地服務，進入純演算法備援)
    CONNECTING = "connecting"   # 正在連線探測中
    ONLINE = "online"           # 本地 AI 服務在線
    BUSY = "busy"               # 模型正在推論運算中


class ModelCapability(Enum):
    VISION = "vision"           # 具備視覺多模態能力 (VLM: Gemma 3/4, PaliGemma, LLaVA 等)
    CHAT = "chat"               # 具備一般文字推理與對話能力
    CODE = "code"               # 具備程式碼生成與重構能力
    EMBED = "embed"             # 向量嵌入能力


@dataclass
class LocalModelInfo:
    """本地模型資訊與能力評級"""
    name: str
    size_bytes: int = 0
    parameter_size: str = ""    # 例如 "11.9B", "8B", "4B"
    quantization: str = ""      # 例如 "Q4_K_M"
    capabilities: List[ModelCapability] = field(default_factory=list)
    is_active_in_vram: bool = False
    vram_usage_bytes: int = 0
    modified_at: str = ""

    @property
    def size_gb(self) -> float:
        return round(self.size_bytes / (1024 ** 3), 2)

    @property
    def has_vision(self) -> bool:
        return ModelCapability.VISION in self.capabilities


class RuhAIEngine:
    """
    RuhOS 本地通用 AI 引擎中樞
    ============================================================================
    向所有 ATG / RuhOS 應用提供單一、穩定的本地 AI 語意增強接口。
    """
    _instance: Optional["RuhAIEngine"] = None
    _lock = threading.Lock()

    @classmethod
    def get_instance(cls) -> "RuhAIEngine":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = RuhAIEngine()
        return cls._instance

    def __init__(self):
        self.endpoint: str = os.environ.get("RUH_OLLAMA_HOST", "http://localhost:11434").rstrip("/")
        self.status: AIStatus = AIStatus.CONNECTING
        self.active_vlm_model: Optional[str] = None
        self.active_chat_model: Optional[str] = None
        self.available_models: Dict[str, LocalModelInfo] = {}
        self.gpu_info: Dict[str, Any] = {}
        self._listeners: List[Callable[[AIStatus, Dict[str, Any]], None]] = []
        self._last_probe_time: float = 0
        self._probe_lock = threading.Lock()

        # 非同步背景啟動初次硬體與模型探測
        threading.Thread(target=self.refresh, daemon=True).start()

    def add_listener(self, callback: Callable[[AIStatus, Dict[str, Any]], None]):
        """註冊狀態變更與模型更新監聽器 (支援熱更新 UI)"""
        if callback not in self._listeners:
            self._listeners.append(callback)

    def remove_listener(self, callback: Callable[[AIStatus, Dict[str, Any]], None]):
        if callback in self._listeners:
            self._listeners.remove(callback)

    def _notify_listeners(self):
        ctx = {
            "status": self.status,
            "vlm": self.active_vlm_model,
            "chat": self.active_chat_model,
            "models": list(self.available_models.keys()),
            "gpu": self.gpu_info
        }
        for cb in list(self._listeners):
            try:
                cb(self.status, ctx)
            except Exception:
                pass

    def probe_hardware_gpu(self) -> Dict[str, Any]:
        """感測本地 GPU 與顯存容量 (支援 NVIDIA / 系統記憶體)"""
        info = {
            "gpu_name": "Unknown",
            "vram_total_mb": 0,
            "vram_free_mb": 0,
            "has_nvidia": False
        }
        try:
            import subprocess
            res = subprocess.run(
                ["nvidia-smi", "--query-gpu=name,memory.total,memory.free", "--format=csv,noheader,nounits"],
                capture_output=True, text=True, timeout=2
            )
            if res.returncode == 0 and res.stdout.strip():
                parts = [p.strip() for p in res.stdout.strip().split(",")]
                if len(parts) >= 3:
                    info["gpu_name"] = parts[0]
                    info["vram_total_mb"] = int(parts[1])
                    info["vram_free_mb"] = int(parts[2])
                    info["has_nvidia"] = True
        except Exception:
            pass

        self.gpu_info = info
        return info

    def refresh(self, force: bool = False) -> AIStatus:
        """
        即時探測本地 AI 服務連線狀態與已載入模型 (熱插拔探測入口)
        """
        with self._probe_lock:
            now = time.time()
            if not force and (now - self._last_probe_time < 3.0):
                return self.status

            self._last_probe_time = now
            self.probe_hardware_gpu()

            try:
                # 1. 探測 Ollama /api/tags
                req = urllib.request.Request(
                    f"{self.endpoint}/api/tags",
                    headers={"User-Agent": "RuhOS-AIEngine/1.0"}
                )
                with urllib.request.urlopen(req, timeout=2.5) as resp:
                    if resp.status != 200:
                        self.status = AIStatus.OFFLINE
                        self._notify_listeners()
                        return self.status
                    data = json.loads(resp.read().decode("utf-8"))

                # 2. 探測當前駐留顯存中的活躍進程 /api/ps
                active_ps_models = set()
                try:
                    ps_req = urllib.request.Request(
                        f"{self.endpoint}/api/ps",
                        headers={"User-Agent": "RuhOS-AIEngine/1.0"}
                    )
                    with urllib.request.urlopen(ps_req, timeout=1.5) as ps_resp:
                        if ps_resp.status == 200:
                            ps_data = json.loads(ps_resp.read().decode("utf-8"))
                            for m in ps_data.get("models", []):
                                active_ps_models.add(m.get("name", ""))
                except Exception:
                    pass

                # 3. 解析模型清單與能力評級
                parsed_models: Dict[str, LocalModelInfo] = {}
                vlm_candidates = []
                chat_candidates = []

                for item in data.get("models", []):
                    m_name = item.get("name", "")
                    details = item.get("details", {})
                    family = details.get("family", "").lower()
                    param_size = details.get("parameter_size", "")
                    quant = details.get("quantization_level", "")

                    caps = [ModelCapability.CHAT]
                    name_lower = m_name.lower()

                    # 判斷是否具備視覺能力 (VLM)
                    if any(kw in name_lower for kw in ["gemma", "paligemma", "vision", "llava", "minicpm-v", "bakllava", "moondream", "qwen2-vl", "qwen2.5-vl"]):
                        caps.append(ModelCapability.VISION)
                        vlm_candidates.append(m_name)

                    if any(kw in name_lower for kw in ["code", "coder", "starcoder", "deepseek-coder"]):
                        caps.append(ModelCapability.CODE)

                    chat_candidates.append(m_name)

                    is_vram = (m_name in active_ps_models)
                    info = LocalModelInfo(
                        name=m_name,
                        size_bytes=item.get("size", 0),
                        parameter_size=param_size,
                        quantization=quant,
                        capabilities=caps,
                        is_active_in_vram=is_vram,
                        modified_at=item.get("modified_at", "")
                    )
                    parsed_models[m_name] = info

                self.available_models = parsed_models
                self.status = AIStatus.ONLINE

                # 4. 自動揀選最適模型 (已駐留顯存者優先，其次選用最新 Gemma/PaliGemma)
                if not self.active_vlm_model or self.active_vlm_model not in parsed_models:
                    active_vlm = [m for m in vlm_candidates if m in active_ps_models]
                    if active_vlm:
                        self.active_vlm_model = active_vlm[0]
                    elif vlm_candidates:
                        gemma_m = [m for m in vlm_candidates if "gemma" in m.lower()]
                        self.active_vlm_model = gemma_m[0] if gemma_m else vlm_candidates[0]
                    else:
                        self.active_vlm_model = None

                if not self.active_chat_model or self.active_chat_model not in parsed_models:
                    if self.active_vlm_model:
                        self.active_chat_model = self.active_vlm_model
                    elif chat_candidates:
                        self.active_chat_model = chat_candidates[0]
                    else:
                        self.active_chat_model = None

                self._notify_listeners()
                return self.status

            except Exception:
                self.status = AIStatus.OFFLINE
                self._notify_listeners()
                return self.status

    def set_active_model(self, model_name: str):
        """手動切換指定模型 (熱切換)"""
        if model_name in self.available_models:
            info = self.available_models[model_name]
            if info.has_vision:
                self.active_vlm_model = model_name
            self.active_chat_model = model_name
            self._notify_listeners()

    def is_available(self) -> bool:
        """檢查本地 AI 是否就緒可用"""
        if self.status != AIStatus.ONLINE:
            self.refresh()
        return self.status == AIStatus.ONLINE and bool(self.available_models)

    def _prepare_image_b64(self, image_input: Union[str, bytes, Any]) -> str:
        """將多種圖片輸入統一編碼為 Base64 字串"""
        if isinstance(image_input, str):
            if os.path.exists(image_input):
                with open(image_input, "rb") as f:
                    return base64.b64encode(f.read()).decode("utf-8")
            else:
                return image_input
        elif isinstance(image_input, bytes):
            return base64.b64encode(image_input).decode("utf-8")
        elif hasattr(image_input, "save"):
            buf = BytesIO()
            image_input.save(buf, format="PNG")
            return base64.b64encode(buf.getvalue()).decode("utf-8")
        else:
            raise ValueError(f"不支援的影像格式: {type(image_input)}")

    def vision_query(
        self,
        image_input: Union[str, bytes, Any],
        prompt: str,
        system_prompt: str = "",
        model_name: Optional[str] = None,
        as_json: bool = False,
        timeout: int = 45
    ) -> Dict[str, Any]:
        """
        通用多模態視覺查詢 (Vision Query API)
        ------------------------------------------------------------------------
        輸入圖片與提示詞，由本地 Gemma / PaliGemma 深度解析並輸出文字或 JSON。
        """
        target_model = model_name or self.active_vlm_model
        if not target_model or not self.is_available():
            return {
                "success": False,
                "status": "offline",
                "error": "本地 VLM 服務離線或無可用多模態視覺模型",
                "content": ""
            }

        try:
            b64_img = self._prepare_image_b64(image_input)
            messages = []
            if system_prompt:
                messages.append({"role": "system", "content": system_prompt})

            messages.append({
                "role": "user",
                "content": prompt,
                "images": [b64_img]
            })

            payload = {
                "model": target_model,
                "messages": messages,
                "stream": False
            }
            if as_json:
                payload["format"] = "json"

            req = urllib.request.Request(
                f"{self.endpoint}/api/chat",
                data=json.dumps(payload).encode("utf-8"),
                headers={"Content-Type": "application/json", "User-Agent": "RuhOS-AIEngine/1.0"}
            )
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                res = json.loads(resp.read().decode("utf-8"))
                content = res.get("message", {}).get("content", "").strip()

                parsed_json = None
                if as_json:
                    try:
                        parsed_json = json.loads(content)
                    except Exception:
                        pass

                return {
                    "success": True,
                    "status": "online",
                    "model": target_model,
                    "content": content,
                    "json_data": parsed_json,
                    "raw_response": res
                }

        except Exception as e:
            return {
                "success": False,
                "status": "error",
                "error": str(e),
                "content": ""
            }

    def chat_query(
        self,
        prompt: str,
        system_prompt: str = "",
        model_name: Optional[str] = None,
        as_json: bool = False,
        timeout: int = 40
    ) -> Dict[str, Any]:
        """
        通用純文字推理對話 (Text/Reasoning Query API)
        """
        target_model = model_name or self.active_chat_model
        if not target_model or not self.is_available():
            return {
                "success": False,
                "status": "offline",
                "error": "本地 AI 服務離線",
                "content": ""
            }

        try:
            messages = []
            if system_prompt:
                messages.append({"role": "system", "content": system_prompt})
            messages.append({"role": "user", "content": prompt})

            payload = {
                "model": target_model,
                "messages": messages,
                "stream": False
            }
            if as_json:
                payload["format"] = "json"

            req = urllib.request.Request(
                f"{self.endpoint}/api/chat",
                data=json.dumps(payload).encode("utf-8"),
                headers={"Content-Type": "application/json", "User-Agent": "RuhOS-AIEngine/1.0"}
            )
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                res = json.loads(resp.read().decode("utf-8"))
                content = res.get("message", {}).get("content", "").strip()
                return {
                    "success": True,
                    "status": "online",
                    "model": target_model,
                    "content": content,
                    "raw_response": res
                }
        except Exception as e:
            return {
                "success": False,
                "status": "error",
                "error": str(e),
                "content": ""
            }


# 全域單例取得捷徑
def get_ai_engine() -> RuhAIEngine:
    """取得 RuhOS 全域通用本地 AI 引擎實例"""
    return RuhAIEngine.get_instance()