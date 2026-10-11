"""
Ruhi (如喜) Input Method Framework
ruhi_brain.ai_provider - 本地熱插拔 AI 大腦中樞 (Gemma / Ollama / Local SLM)
"""

import json
import urllib.request
import urllib.error
import threading
from typing import List, Dict, Optional, Tuple, Callable

class RuhiAIProvider:
    """本地 AI 提供者抽象基底"""
    def is_available(self) -> bool:
        return False

    def disambiguate(self, candidates: List[str], left_ctx: str = "", right_ctx: str = "") -> Optional[str]:
        return None

    def auto_correct(self, raw_text: str) -> Optional[str]:
        return None


class OllamaGemmaProvider(RuhiAIProvider):
    """基於本地 Ollama Gemma 引擎的非同步語意大腦"""
    def __init__(self, host: str = "http://127.0.0.1:11434", model: str = "gemma2:9b", timeout: float = 1.5):
        self.host = host.rstrip('/')
        self.model = model
        self.timeout = timeout
        self._is_online = False
        self._check_online()

    def _check_online(self):
        """檢查本地 Ollama 是否在線"""
        try:
            req = urllib.request.Request(f"{self.host}/api/tags", headers={"User-Agent": "Ruhi-IME/1.0"})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                if resp.status == 200:
                    self._is_online = True
                    return True
        except Exception:
            self._is_online = False
        return False

    def is_available(self) -> bool:
        return self._is_online or self._check_online()

    def query_gemma_async(self, prompt: str, callback: Callable[[str], None], system_prompt: str = ""):
        """非同步呼叫 Gemma，結果透過 callback 回傳，絕不阻塞主打字執行緒"""
        def _worker():
            try:
                url = f"{self.host}/api/generate"
                payload = {
                    "model": self.model,
                    "prompt": prompt,
                    "stream": False,
                    "options": {
                        "temperature": 0.1,
                        "top_p": 0.9,
                        "num_predict": 30,
                    }
                }
                if system_prompt:
                    payload["system"] = system_prompt

                data = json.dumps(payload).encode('utf-8')
                req = urllib.request.Request(
                    url,
                    data=data,
                    headers={"Content-Type": "application/json", "User-Agent": "Ruhi-IME/1.0"}
                )
                with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                    if resp.status == 200:
                        res = json.loads(resp.read().decode('utf-8'))
                        reply = res.get("response", "").strip()
                        callback(reply)
            except Exception:
                pass

        threading.Thread(target=_worker, daemon=True).start()

    def disambiguate(self, candidates: List[str], left_ctx: str = "", right_ctx: str = "") -> Optional[str]:
        """
        同步/極速評估同音詞消歧義：在多個候選詞中挑選上下文最自然者
        """
        if not self.is_available() or not candidates or len(candidates) <= 1:
            return None

        prompt = (
            f"請依據前後文語境，在候選詞中挑選最正確的一個繁體中文字詞，直接輸出該字詞，不得輸出其他任何文字：\n"
            f"前文：{left_ctx}\n"
            f"候選詞：{', '.join(candidates[:5])}\n"
            f"後文：{right_ctx}\n"
            f"最佳選擇："
        )
        try:
            url = f"{self.host}/api/generate"
            payload = {
                "model": self.model,
                "prompt": prompt,
                "stream": False,
                "options": {
                    "temperature": 0.0,
                    "num_predict": 10,
                }
            }
            data = json.dumps(payload).encode('utf-8')
            req = urllib.request.Request(
                url,
                data=data,
                headers={"Content-Type": "application/json", "User-Agent": "Ruhi-IME/1.0"}
            )
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                if resp.status == 200:
                    res = json.loads(resp.read().decode('utf-8'))
                    reply = res.get("response", "").strip()
                    for cand in candidates:
                        if cand in reply:
                            return cand
        except Exception:
            return None
        return None
