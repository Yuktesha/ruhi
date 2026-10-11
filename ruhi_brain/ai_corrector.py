#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 通用智慧型輸入法 - 本地/遠端 AI 目標文字校正引擎
ruhi_brain.ai_corrector — 校正已送至工作目標視窗的文字正確性中樞
=============================================================================
核心宗旨與架構原則：
1. 「工作目標文字校正」：
   - 當使用者在目標應用程式（記事本、Word、Chrome、VS Code、LINE 等）中
     已送出 (committed) 文字，或選取已存在的文字時；
   - 藉由本地/遠端 AI 分析並診斷錯別字、同音誤植、贅字、語病與標點瑕疵；
   - 提供結構化校正報告與安全替換資料。
2. 三軌 AI 後端優先序（恪守 ATG 全域技術生態鐵律）：
   - 軌道 1 (優先順位 1)：Google Gemini API (台灣正體在地化首選)
   - 軌道 2 (優先順位 2)：本地 Ollama Gemma (gemma2/gemma3/gemma4，歐美優質開源 SLM，離線隱私優先)
   - 軌道 3 (優先順位 3)：本機啟發式錯字防火牆與語言學規則庫 (Heuristic Fallback，零外部依賴保底)
3. 嚴禁任何中國審查模型 (徹底排除 Qwen、DeepSeek 等)。
4. 恪守台灣正體中文慣用詞與標點規範（嚴禁繁體支語）。
"""

import difflib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass, field
from typing import Any, Callable, Dict, List, Optional, Tuple

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


# =============================================================================
# 1. 啟發式錯字防火牆與台灣在地化詞庫 (Heuristic Knowledge Base)
# =============================================================================

# 高頻錯字與同音混淆詞典 (錯誤/筆誤詞 ➔ 正體正確詞)
HEURISTIC_TYPO_MAP: Dict[str, Tuple[str, str, str]] = {
    # 格式: 誤用詞: (正確詞, 類別, 理由)
    "粒刻": ("立刻", "typo", "常見同音形近筆誤"),
    "按步就班": ("按部就班", "typo", "成語正字為『部』，非『步』"),
    "再接在勵": ("再接再厲", "typo", "成語正字為『再』與『厲』"),
    "再接再勵": ("再接再厲", "typo", "成語正字為『厲』，非『勵』"),
    "再接在厲": ("再接再厲", "typo", "成語正字為『再』與『厲』"),
    "以經": ("已經", "homophone", "同音誤植，正字為『已經』"),
    "以偏蓋全": ("以偏概全", "typo", "成語正字為『概』，非『蓋』"),
    "一偏概全": ("以偏概全", "typo", "成語正字為『以』，非『一』"),
    "變本加利": ("變本加厲", "typo", "成語正字為『厲』，非『利』"),
    "不逕而走": ("不徑而走", "typo", "成語正字為『徑』，非『逕』"),
    "穿流不息": ("川流不息", "typo", "成語正字為『川』，非『穿』"),
    "急燥": ("急躁", "typo", "正字為『躁』，非『燥』"),
    "名列前矛": ("名列前茅", "typo", "成語正字為『茅』，非『矛』"),
    "一愁莫展": ("一籌莫展", "typo", "成語正字為『籌』，非『愁』"),
    "迫不急待": ("迫不及待", "typo", "成語正字為『及』，非『急』"),
    "濫芋充數": ("濫竽充數", "typo", "成語正字為『竽』，非『芋』"),
    "走頭無路": ("走投無路", "typo", "成語正字為『投』，非『頭』"),
    "委屈求全": ("委曲求全", "typo", "成語正字為『曲』，非『屈』"),
    "默守成規": ("墨守成規", "typo", "典故出自墨子，正字為『墨』"),
    "談笑風聲": ("談笑風生", "typo", "成語正字為『生』，非『聲』"),
    "消聲匿跡": ("銷聲匿跡", "typo", "成語正字為『銷』，非『消』"),
    "震憾": ("震撼", "typo", "正字為『撼』，非『憾』"),
    "趨之若騖": ("趨之若鶩", "typo", "成語正字為『鶩』(鴨)，非『騖』(馬奔)"),
    "坐陣": ("坐鎮", "typo", "正字為『鎮』，非『陣』"),
    "甘敗下風": ("甘拜下風", "typo", "正字為『拜』，非『敗』"),
    "名信片": ("明信片", "typo", "正字為『明』，非『名』"),
    "莫明其妙": ("莫名其妙", "typo", "成語正字為『名』，非『明』"),
    "鬼計多端": ("詭計多端", "typo", "成語正字為『詭』，非『鬼』"),
    "絕不認輸": ("決不認輸", "typo", "表示堅定決心正字為『決』"),
    "寒喧": ("寒暄", "typo", "正字為『暄』，非『喧』"),
    "脈膊": ("脈搏", "typo", "正字為『搏』，非『膊』"),
    "針貶": ("針砭", "typo", "正字為『砭』，非『貶』"),
    "蜂湧而至": ("蜂擁而至", "typo", "成語正字為『擁』，非『湧』"),
    "心類": ("心累", "homophone", "同音誤植，正字為『心累』"),
    "手棟": ("手動", "homophone", "同音誤植，正字為『手動』"),
    "笑果如和": ("效果如何", "homophone", "連續同音誤植，應為『效果如何』"),
    "在來測試": ("再來測試", "homophone", "副詞『再來』誤植為介詞『在』"),
    "做為": ("作為", "typo", "作為名詞或介詞時正字為『作為』"),
    "這各": ("這個", "homophone", "同音誤植，正字為『這個』"),
    "那各": ("那個", "homophone", "同音誤植，正字為『那個』"),
    "什模": ("什麼", "homophone", "同音誤植，正字為『什麼』"),
    "應概": ("應該", "homophone", "同音誤植，正字為『應該』"),
    "速效要": ("速效藥", "homophone", "同音誤植，正字為『速效藥』"),
    "呀縮": ("壓縮", "homophone", "同音誤植，正字為『壓縮』"),
    "現再": ("現在", "homophone", "同音誤植，時間副詞正字為『現在』"),
    "現再來": ("現在來", "homophone", "時間副詞與動作搭配，正字為『現在來』"),
}

# 台灣繁體在地化與非在地詞語轉換 (支語/非正體 ➔ 台灣正體慣用語)
HEURISTIC_LOCALIZATION_MAP: Dict[str, Tuple[str, str, str]] = {
    # 誤用詞: (台灣詞, 類別, 理由)
    "服務器": ("伺服器", "localization", "台灣資訊科技標準術語為『伺服器』"),
    "代碼": ("程式碼", "localization", "台灣軟體開發標準術語為『程式碼』"),
    "項目": ("專案", "localization", "軟體工程語境中應使用台灣慣用詞『專案』"),
    "內存": ("記憶體", "localization", "電腦硬體與計算機科學標準術語為『記憶體』"),
    "默認": ("預設", "localization", "資訊科技與日常設定標準術語為『預設』"),
    "算法": ("演算法", "localization", "電腦科學標準術語為『演算法』"),
    "緩存": ("快取", "localization", "計算機快取記憶體標準術語為『快取』"),
    "數組": ("陣列", "localization", "程式語言資料結構標準術語為『陣列』"),
    "字符串": ("字串", "localization", "程式語言資料型別標準術語為『字串』"),
    "接口": ("介面", "localization", "軟體架構與 API 標準術語為『介面』"),
    "宏": ("巨集", "localization", "程式編譯與辦公軟體標準術語為『巨集』"),
    "軟件": ("軟體", "localization", "台灣標準術語為『軟體』"),
    "硬件": ("硬體", "localization", "台灣標準術語為『硬體』"),
    "屏幕": ("螢幕", "localization", "台灣標準術語為『螢幕』"),
    "網絡": ("網路", "localization", "台灣標準術語為『網路』"),
    "視頻": ("影片", "localization", "非電子工程波段語境時，視聽內容台灣慣用『影片』"),
    "打印": ("列印", "localization", "台灣文書輸出標準術語為『列印』"),
}

# 半形標點符號正規化 (半形 ➔ 全形)
PUNCTUATION_NORM_MAP: Dict[str, str] = {
    ",": "，",
    ";": "；",
    ":": "：",
    "?": "？",
    "!": "！",
    "(": "（",
    ")": "）",
}


# =============================================================================
# 2. 資料結構模型 (Data Models)
# =============================================================================

@dataclass
class TextChange:
    """單一文字修訂細節"""
    start: int
    end: int
    original: str
    replacement: str
    category: str  # "typo" | "homophone" | "punctuation" | "localization" | "grammar"
    reason: str
    confidence: float = 0.95

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class TextCorrectionResult:
    """文字校正報告"""
    original_text: str
    corrected_text: str
    has_corrections: bool
    changes: List[TextChange] = field(default_factory=list)
    backend_used: str = "none"  # "gemini" | "gemma" | "heuristic"
    latency_ms: float = 0.0
    error_message: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "original_text": self.original_text,
            "corrected_text": self.corrected_text,
            "has_corrections": self.has_corrections,
            "changes": [c.to_dict() for c in self.changes],
            "backend_used": self.backend_used,
            "latency_ms": self.latency_ms,
            "error_message": self.error_message,
        }

    def format_summary(self) -> str:
        """產出精簡易讀的修訂摘要"""
        if not self.has_corrections:
            return "✅ 文字正確無瑕，未發現錯別字或語病。"
        
        lines = [f"🔍 [AI 校正報告] (後端: {self.backend_used}, 耗時: {self.latency_ms:.1f}ms)"]
        for i, c in enumerate(self.changes, 1):
            cat_label = {
                "typo": "錯別字",
                "homophone": "同音誤植",
                "punctuation": "標點修訂",
                "localization": "台灣在地化",
                "grammar": "語法語病",
            }.get(c.category, c.category)
            lines.append(f"  {i}. [{cat_label}] 「{c.original}」➔「{c.replacement}」({c.reason})")
        return "\n".join(lines)


# =============================================================================
# 3. 差異比對提取器 (Diff & Change Extractor)
# =============================================================================

def extract_diff_changes(original: str, corrected: str, default_reason: str = "AI 語意校正") -> List[TextChange]:
    """比對原文與校正文，提取精確的字元起訖位置與替換區塊"""
    if original == corrected:
        return []

    changes: List[TextChange] = []
    matcher = difflib.SequenceMatcher(None, original, corrected)

    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            continue
        
        orig_sub = original[i1:i2]
        corr_sub = corrected[j1:j2]

        category = "typo"
        reason = default_reason

        # 檢驗是否符合已知啟發式字典
        if orig_sub in HEURISTIC_TYPO_MAP:
            _, cat, r = HEURISTIC_TYPO_MAP[orig_sub]
            category = cat
            reason = r
        elif orig_sub in HEURISTIC_LOCALIZATION_MAP:
            _, cat, r = HEURISTIC_LOCALIZATION_MAP[orig_sub]
            category = cat
            reason = r
        elif len(orig_sub) == 1 and orig_sub in PUNCTUATION_NORM_MAP:
            category = "punctuation"
            reason = "標點符號全形規範化"
        elif tag == "insert":
            category = "grammar"
            reason = "補齊語意缺漏字詞"
        elif tag == "delete":
            category = "grammar"
            reason = "刪除贅字或重複字詞"

        changes.append(
            TextChange(
                start=i1,
                end=i2,
                original=orig_sub,
                replacement=corr_sub,
                category=category,
                reason=reason,
                confidence=0.92,
            )
        )

    return changes


# =============================================================================
# 4. 啟發式校正引擎 (Heuristic Engine - 零外部依賴離線保底)
# =============================================================================

class HeuristicCorrector:
    """純 Python 啟發式規則庫校正器"""

    def correct(self, text: str) -> TextCorrectionResult:
        start_t = time.perf_counter()
        if not text:
            return TextCorrectionResult(text, text, False, [], "heuristic", 0.0)

        # 收集所有潛在的替換候選
        candidates: List[TextChange] = []

        # 1. 錯字與同音混淆詞
        for typo, (corr, cat, reason) in sorted(HEURISTIC_TYPO_MAP.items(), key=lambda x: -len(x[0])):
            idx = 0
            while True:
                found = text.find(typo, idx)
                if found == -1:
                    break
                candidates.append(
                    TextChange(
                        start=found,
                        end=found + len(typo),
                        original=typo,
                        replacement=corr,
                        category=cat,
                        reason=reason,
                        confidence=0.98,
                    )
                )
                idx = found + len(typo)

        # 2. 台灣繁體在地化慣用詞
        for bad_word, (tw_word, cat, reason) in sorted(HEURISTIC_LOCALIZATION_MAP.items(), key=lambda x: -len(x[0])):
            idx = 0
            while True:
                found = text.find(bad_word, idx)
                if found == -1:
                    break
                candidates.append(
                    TextChange(
                        start=found,
                        end=found + len(bad_word),
                        original=bad_word,
                        replacement=tw_word,
                        category=cat,
                        reason=reason,
                        confidence=0.96,
                    )
                )
                idx = found + len(bad_word)

        # 3. 半形標點符號
        for i, ch in enumerate(text):
            if ch in PUNCTUATION_NORM_MAP:
                prev_ch = text[i - 1] if i > 0 else ""
                next_ch = text[i + 1] if i + 1 < len(text) else ""
                if '\u4e00' <= prev_ch <= '\u9fff' or '\u4e00' <= next_ch <= '\u9fff':
                    full_punc = PUNCTUATION_NORM_MAP[ch]
                    candidates.append(
                        TextChange(
                            start=i,
                            end=i + 1,
                            original=ch,
                            replacement=full_punc,
                            category="punctuation",
                            reason="中文字句中標點符號全形正規化",
                            confidence=0.95,
                        )
                    )

        # 排序候選：優先照 start 升序，長度降序
        candidates.sort(key=lambda c: (c.start, -(c.end - c.start)))

        # 貪婪選取不重疊的 spans
        selected_changes: List[TextChange] = []
        last_end = 0
        for c in candidates:
            if c.start >= last_end:
                selected_changes.append(c)
                last_end = c.end

        # 重構 corrected_text
        if not selected_changes:
            latency = (time.perf_counter() - start_t) * 1000.0
            return TextCorrectionResult(
                original_text=text,
                corrected_text=text,
                has_corrections=False,
                changes=[],
                backend_used="heuristic",
                latency_ms=latency,
            )

        pieces: List[str] = []
        cursor = 0
        for c in selected_changes:
            pieces.append(text[cursor:c.start])
            pieces.append(c.replacement)
            cursor = c.end
        pieces.append(text[cursor:])
        corrected_text = "".join(pieces)

        latency = (time.perf_counter() - start_t) * 1000.0
        return TextCorrectionResult(
            original_text=text,
            corrected_text=corrected_text,
            has_corrections=True,
            changes=selected_changes,
            backend_used="heuristic",
            latency_ms=latency,
        )


# =============================================================================
# 5. Google Gemini API 遠端校正引擎 (優先順位 1)
# =============================================================================

class GeminiCorrector:
    """基於 Google Gemini 官方生態的遠端文字校正引擎"""

    SYSTEM_INSTRUCTION = (
        "你是一個極致專業的繁體中文（台灣）語言學家與文字校對專家。\n"
        "你的任務是檢視使用者送入的文字，校正其中的錯別字、同音誤植、語病、倒裝贅字、非全形標點以及支語（轉譯為台灣繁體慣用語）。\n"
        "【絕對規則】：\n"
        "1. 必須嚴格輸出 JSON 格式，不得輸出任何 Markdown 程式碼區塊標記（如 ```json），直接輸出純 JSON 字串。\n"
        "2. JSON 結構如下：\n"
        "{\n"
        '  "corrected_text": "校正後的繁體中文完整文字",\n'
        '  "has_corrections": true 或 false,\n'
        '  "corrections": [\n'
        "    {\n"
        '      "original": "原錯字詞",\n'
        '      "replacement": "校正後字詞",\n'
        '      "category": "typo" | "homophone" | "punctuation" | "localization" | "grammar",\n'
        '      "reason": "簡短繁體中文說明原因"\n'
        "    }\n"
        "  ]\n"
        "}\n"
        "3. 若文字原本就完全正確，corrected_text 請保持原樣，has_corrections 設為 false，corrections 設為空陣列 []。\n"
        "4. 嚴格遵循台灣在地化習慣（伺服器、專案、程式碼、快取、演算法、記憶體等，嚴禁支語）。"
    )

    def __init__(self, api_key: Optional[str] = None, model: str = "gemini-2.5-flash", timeout: float = 6.0):
        self.api_key = api_key or os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
        self.model = model
        self.timeout = timeout

    def is_available(self) -> bool:
        return bool(self.api_key)

    def correct(self, text: str) -> TextCorrectionResult:
        start_t = time.perf_counter()
        if not self.is_available():
            raise RuntimeError("GEMINI_API_KEY 未設定，無法啟用 Gemini 遠端校正")

        url = f"https://generativelanguage.googleapis.com/v1beta/models/{self.model}:generateContent?key={self.api_key}"
        
        prompt = f"請校對並修正以下文字中的任何錯別字或瑕疵：\n{text}"
        payload = {
            "contents": [
                {
                    "parts": [{"text": prompt}]
                }
            ],
            "systemInstruction": {
                "parts": [{"text": self.SYSTEM_INSTRUCTION}]
            },
            "generationConfig": {
                "temperature": 0.1,
                "responseMimeType": "application/json",
            }
        }

        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            headers={"Content-Type": "application/json", "User-Agent": "Ruhi-IME/1.0"}
        )

        with urllib.request.urlopen(req, timeout=self.timeout) as resp:
            body = resp.read().decode("utf-8")
            res_obj = json.loads(body)
            candidates = res_obj.get("candidates", [])
            if not candidates:
                raise ValueError("Gemini API 未回傳候選內容")

            raw_reply = candidates[0].get("content", {}).get("parts", [{}])[0].get("text", "").strip()
            # 清理可能的 markdown 圍欄
            if raw_reply.startswith("```"):
                raw_reply = re.sub(r"^```[a-zA-Z]*\n?", "", raw_reply)
                raw_reply = re.sub(r"\n?```$", "", raw_reply)

            parsed = json.loads(raw_reply)
            corrected_text = parsed.get("corrected_text", text)
            has_corrections = bool(parsed.get("has_corrections", False)) or (corrected_text != text)

            changes = extract_diff_changes(text, corrected_text, default_reason="Gemini 語意智慧校正")
            latency = (time.perf_counter() - start_t) * 1000.0

            return TextCorrectionResult(
                original_text=text,
                corrected_text=corrected_text,
                has_corrections=has_corrections,
                changes=changes,
                backend_used="gemini",
                latency_ms=latency,
            )


# =============================================================================
# 6. 本地 Ollama Gemma 校正引擎 (優先順位 2)
# =============================================================================

class OllamaGemmaCorrector:
    """基於本地 Ollama Gemma 模型的離線文字校正引擎"""

    PROMPT_TEMPLATE = (
        "你是一個極致專業的繁體中文（台灣）語言學家與文字校對專家。\n"
        "請檢視以下使用者文字，校正其中的錯別字、同音誤植、語病、非全形標點以及支語。\n"
        "請直接輸出純 JSON，不要附加任何解釋或 markdown 代碼標籤：\n"
        '{{"corrected_text": "校正後的文字", "has_corrections": true/false}}\n\n'
        "待校對文字：\n{text}\n"
    )

    def __init__(self, host: str = "http://127.0.0.1:11434", model: str = "gemma2:9b", timeout: float = 3.5):
        self.host = host.rstrip('/')
        self.model = model
        self.timeout = timeout
        self._online_checked = False
        self._is_online = False

    def is_available(self) -> bool:
        if self._online_checked:
            return self._is_online
        try:
            req = urllib.request.Request(f"{self.host}/api/tags", headers={"User-Agent": "Ruhi-IME/1.0"})
            with urllib.request.urlopen(req, timeout=0.8) as resp:
                if resp.status == 200:
                    self._is_online = True
        except Exception:
            self._is_online = False
        self._online_checked = True
        return self._is_online

    def correct(self, text: str) -> TextCorrectionResult:
        start_t = time.perf_counter()
        if not self.is_available():
            raise RuntimeError(f"本地 Ollama Gemma 服務不可用 ({self.host})")

        url = f"{self.host}/api/generate"
        payload = {
            "model": self.model,
            "prompt": self.PROMPT_TEMPLATE.format(text=text),
            "stream": False,
            "format": "json",
            "options": {
                "temperature": 0.1,
                "num_predict": 200,
            }
        }

        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            headers={"Content-Type": "application/json", "User-Agent": "Ruhi-IME/1.0"}
        )

        with urllib.request.urlopen(req, timeout=self.timeout) as resp:
            body = resp.read().decode("utf-8")
            res_obj = json.loads(body)
            raw_reply = res_obj.get("response", "").strip()

            parsed = json.loads(raw_reply)
            corrected_text = parsed.get("corrected_text", text)
            has_corrections = bool(parsed.get("has_corrections", False)) or (corrected_text != text)

            changes = extract_diff_changes(text, corrected_text, default_reason="本地 Gemma 語意智慧校正")
            latency = (time.perf_counter() - start_t) * 1000.0

            return TextCorrectionResult(
                original_text=text,
                corrected_text=corrected_text,
                has_corrections=has_corrections,
                changes=changes,
                backend_used="gemma",
                latency_ms=latency,
            )


# =============================================================================
# 7. 統一 AI 校正調度大腦 (RuhiAICorrector)
# =============================================================================

class RuhiAICorrector:
    """
    如喜通用 AI 目標文字校正中樞：
    智慧調度 Gemini (優先 1) ➔ 本地 Gemma (優先 2) ➔ 本機 Heuristic (優先 3)
    """

    def __init__(
        self,
        backend: str = "auto",  # "auto" | "gemini" | "gemma" | "heuristic"
        gemini_api_key: Optional[str] = None,
        gemini_model: str = "gemini-2.5-flash",
        ollama_model: str = "gemma2:9b",
        ollama_host: str = "http://127.0.0.1:11434",
    ):
        self.backend = backend
        self.gemini = GeminiCorrector(api_key=gemini_api_key, model=gemini_model)
        self.gemma = OllamaGemmaCorrector(host=ollama_host, model=ollama_model)
        self.heuristic = HeuristicCorrector()

    def correct(self, text: str, preferred_backend: Optional[str] = None) -> TextCorrectionResult:
        """
        對指定文字執行校正。若遇網路中斷或服務未開啟，自動無感向下級降級。
        """
        mode = preferred_backend or self.backend
        start_t = time.perf_counter()

        # 1. 強制指定單一後端模式
        if mode == "gemini":
            try:
                return self.gemini.correct(text)
            except Exception as e:
                res = self.heuristic.correct(text)
                res.error_message = f"Gemini 呼叫失敗 ({e})，降級至啟發式引擎"
                return res

        elif mode == "gemma":
            try:
                return self.gemma.correct(text)
            except Exception as e:
                res = self.heuristic.correct(text)
                res.error_message = f"Ollama Gemma 呼叫失敗 ({e})，降級至啟發式引擎"
                return res

        elif mode == "heuristic":
            return self.heuristic.correct(text)

        # 2. auto 自動梯隊調度模式：Gemini ➔ Gemma ➔ Heuristic
        # 優先順位 1: Gemini
        if self.gemini.is_available():
            try:
                return self.gemini.correct(text)
            except Exception:
                pass  # 繼續嘗試下一順位

        # 優先順位 2: 本地 Ollama Gemma
        if self.gemma.is_available():
            try:
                return self.gemma.correct(text)
            except Exception:
                pass  # 繼續嘗試下一順位

        # 優先順位 3: 確定性啟發式規則引擎 (保底保證 100% 成功)
        return self.heuristic.correct(text)


# =============================================================================
# 8. Python 一等公民 CLI 命令列工具
# =============================================================================

def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="如喜 (RuhI) AI 目標文字校正 CLI 工具 - 支援 Gemini、本地 Gemma 與離線啟發式引擎",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""範例：
  python -m ruhi_brain.ai_corrector "測試文字以經完成，粒刻送出"
  python -m ruhi_brain.ai_corrector --backend heuristic "服務器崩潰了，代碼需要重構"
  python -m ruhi_brain.ai_corrector --json "按步就班的做為"
""",
    )
    parser.add_argument("text", nargs="?", help="待校正的文字內容")
    parser.add_argument(
        "--backend",
        "-b",
        choices=["auto", "gemini", "gemma", "heuristic"],
        default="auto",
        help="校正引擎後端 (預設: auto 自動梯隊降級)",
    )
    parser.add_argument("--json", "-j", action="store_true", help="以 JSON 格式輸出結構化報告")
    parser.add_argument("--apply", "-a", action="store_true", help="僅輸出最終校正後純文字（適用管線組合）")

    args = parser.parse_args()

    if not args.text:
        # 若未提供參數且非 TTY，嘗試自 stdin 讀取
        if not sys.stdin.isatty():
            args.text = sys.stdin.read().strip()
        else:
            parser.print_help()
            sys.exit(1)

    corrector = RuhiAICorrector(backend=args.backend)
    result = corrector.correct(args.text)

    if args.apply:
        print(result.corrected_text)
    elif args.json:
        print(json.dumps(result.to_dict(), ensure_ascii=False, indent=2))
    else:
        print("=" * 60)
        print(" 🎯 如喜 (RuhI) AI 文字校正中樞")
        print("=" * 60)
        print(f" 原文內容：{result.original_text}")
        print(f" 校正結果：{result.corrected_text}")
        print(f" 是否修正：{'是 (發現瑕疵)' if result.has_corrections else '否 (完全正確)'}")
        print("-" * 60)
        print(result.format_summary())
        print("=" * 60)


if __name__ == "__main__":
    main()
