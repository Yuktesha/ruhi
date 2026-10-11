#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
semantic_trainer.py — 本地 Gemma 閉環自進化語意訓練器 (Closed-Loop Self-Evolving AI Semantic Trainer)
=============================================================================
使用者原始設計藍圖（源自 [LINE]我們都是自然人 🌝.txt）：
「 A文章轉許氏鍵盤碼B -> 如喜依照B鍵盤碼吐出組字詞句段落文章C -> diff A vs C ->
   Gemma分析diff -> Gemma提取語意規則 -> Gemma調整詞庫與語意規則 」

五大核心模組：
1. RuhiVirtualTypist:
   - 透過 TextToHsuCompiler 將文章 A 編譯為許氏鍵碼 B
   - 模擬虛擬打字員逐鍵餵入 RuhiEngineBridge (ruhi-core.dll)，產出組字結果 C
2. GranularDiffAnalyzer:
   - 使用 difflib 進行微觀對比，精確鎖定所有失真字詞及前後語境 (Context)
3. GemmaSemanticAnalyst:
   - 連線本地 Ollama gemma4:latest 進行專家級計算語言學診斷
   - 結構化輸出實體詞 (new_words)、轉移加權/抑制 (transitions) 與因式分解構式 (patterns)
4. SemanticSelfEvolver:
   - 自動更新 user_dict.tsv、transitions_user.tsv 與 patterns_user.jsonl
   - 即時熱重載 (Hot Reload) 引擎，無需重啟
5. RuhiSemanticTrainer:
   - 協調整個閉環訓練流程，輸出前後精確度比較與差異消解報告
"""

import argparse
import difflib
import glob
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from datetime import date
from typing import Any, Dict, List, Optional, Tuple

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except Exception:
    pass

# 相對/絕對匯入相容
CURR_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.dirname(CURR_DIR)
if RUHI_ROOT not in sys.path:
    sys.path.insert(0, RUHI_ROOT)
if CURR_DIR not in sys.path:
    sys.path.insert(0, CURR_DIR)

from bopomofo_annotator import BopomofoAnnotator
from engine_bridge import RuhiEngineBridge
from text_to_hsu import TextToHsuCompiler

OLLAMA_API_URL = "http://127.0.0.1:11434/api/generate"


# =============================================================================
# 1. 資料模型 (Data Models)
# =============================================================================

@dataclass
class Discrepancy:
    """單一失真/差異記錄"""
    expected: str
    actual: str
    context_before: str
    context_after: str
    full_clause: str
    char_offset: int


@dataclass
class AnalysisResult:
    """Gemma 語意診斷結果"""
    analysis: str = ""
    new_words: List[Dict[str, Any]] = field(default_factory=list)
    transitions: List[Dict[str, Any]] = field(default_factory=list)
    patterns: List[Dict[str, Any]] = field(default_factory=list)


@dataclass
class TrainingReport:
    """訓練循環報告"""
    source_text: str
    original_composed: str
    final_composed: str
    initial_accuracy: float
    final_accuracy: float
    discrepancies_before: int
    discrepancies_after: int
    evolved_words: int
    evolved_transitions: int
    evolved_patterns: int
    details: List[str] = field(default_factory=list)


# =============================================================================
# 2. 虛擬打字員 (RuhiVirtualTypist)
# =============================================================================

class RuhiVirtualTypist:
    """
    虛擬打字員：將文本轉譯為許氏雙拼按鍵流，逐鍵餵入引擎並收集組字成果
    """

    def __init__(self, bridge: RuhiEngineBridge, compiler: Optional[TextToHsuCompiler] = None):
        self.bridge = bridge
        self.compiler = compiler or TextToHsuCompiler(data_dir=bridge._data_dir)

    def type_clause(self, clause_text: str, clause_keys: str) -> str:
        """鍵入單一子句並取得引擎組字結果"""
        if clause_text == clause_keys or all(ch in self.compiler.PUNCTUATIONS or ch.isspace() for ch in clause_keys):
            return clause_keys

        self.bridge.reset()
        last_resp = None
        for key in clause_keys:
            last_resp = self.bridge.process_key(key)

        if last_resp and last_resp.composed:
            return last_resp.composed
        return clause_text

    def type_text(self, text: str) -> str:
        """
        鍵入完整文章段落，精確重現輸入法在標點切割與組字緩衝區的行為
        """
        clauses = self.compiler.text_to_hsu_clauses(text)
        composed_chunks: List[str] = []

        for c_text, c_keys in clauses:
            chunk = self.type_clause(c_text, c_keys)
            composed_chunks.append(chunk)

        return "".join(composed_chunks)


# =============================================================================
# 3. 細粒度差異分析器 (GranularDiffAnalyzer)
# =============================================================================

class GranularDiffAnalyzer:
    """
    細粒度差異分析器：比對期望文本 A 與引擎組字 C，萃取所有失真字詞及其語境
    """

    @staticmethod
    def calculate_accuracy(expected: str, actual: str) -> float:
        """計算字元級準確度 (0.0 ~ 1.0)"""
        if not expected and not actual:
            return 1.0
        matcher = difflib.SequenceMatcher(None, expected, actual)
        return matcher.ratio()

    @staticmethod
    def extract_discrepancies(expected: str, actual: str) -> List[Discrepancy]:
        """
        使用 difflib SequenceMatcher 找出所有非匹配區間，並提取左右上下文
        """
        discrepancies: List[Discrepancy] = []
        matcher = difflib.SequenceMatcher(None, expected, actual)
        opcodes = matcher.get_opcodes()

        for tag, i1, i2, j1, j2 in opcodes:
            if tag in ("replace", "delete", "insert"):
                exp_sub = expected[i1:i2]
                act_sub = actual[j1:j2]

                ctx_start = max(0, i1 - 8)
                ctx_end = min(len(expected), i2 + 8)
                before = expected[ctx_start:i1]
                after = expected[i2:ctx_end]

                clause_start = max(0, i1 - 18)
                clause_end = min(len(expected), i2 + 18)
                full_clause = expected[clause_start:clause_end]

                discrepancies.append(Discrepancy(
                    expected=exp_sub,
                    actual=act_sub,
                    context_before=before,
                    context_after=after,
                    full_clause=full_clause,
                    char_offset=i1,
                ))

        return discrepancies


# =============================================================================
# 4. Gemma 語意診斷專家 (GemmaSemanticAnalyst)
# =============================================================================

class GemmaSemanticAnalyst:
    """
    Gemma 語意診斷師：透過專家提示詞分析差異成因，提煉詞庫擴充、轉移加權與語意場構式
    """

    GEMMA_PROMPT_TEMPLATE = """你是一位精通漢語計算語言學、Viterbi 格點最佳化與如喜智慧輸入法語意分析的專家。
輸入法引擎在透過許氏雙拼鍵入時，發生了選字失真。以下是實際比對結果：

- 完整語境：「{full_clause}」
- 期望輸出 (Expected)：「{expected}」{compound_exp_hint}
- 引擎選字 (Actual)  ：「{actual}」{compound_act_hint}
- 前置詞 (Context Before)：「{context_before}」
- 後續詞 (Context After) ：「{context_after}」

請深入診斷失真成因（例如靜態詞庫有長詞干擾、轉移概率不足、或為特定語意構式），並提煉具體規則予以修正：
特別注意：
1. 若失真看似單字（如『漢』vs『撼』），但與相鄰字結合成詞後實為專有名詞或複合詞（例如『震漢』被靜態詞『震撼』壓過），請務必將正確的完整複合詞（例如『震漢』）加入 "new_words"，並在 "transitions" 中加入負向抑制（例如前驅詞到錯誤詞設為 -40.0，如『震 -> 撼 (-40.0)』或『前身 -> 震撼 (-40.0)』）！
2. "new_words": 若期望詞為未收錄的專有名詞或高頻複合詞，加入詞庫 (權重 1500~2000)。
3. "transitions": Bigram 轉移矩陣加權 (正向加強 20.0~35.0；同音干擾詞負向抑制 -30.0~-50.0)。
4. "patterns": 若涉及語法槽位、疑問句式、兼類詞或程度補語等泛化結構，提供因式分解槽位。

請嚴格輸出符合以下結構的 JSON（請勿包含 Markdown 標籤之外的多餘文字）：
{{
  "analysis": "簡要分析成因與語言學機理",
  "new_words": [
    {{"word": "期望詞彙", "weight": 1800}}
  ],
  "transitions": [
    {{"prev": "前驅詞", "curr": "期望詞", "weight": 25.0, "type": "collocation"}},
    {{"prev": "前驅詞", "curr": "干擾詞", "weight": -35.0, "type": "anti_confusion"}}
  ],
  "patterns": [
    {{
      "pattern_id": "唯一識別碼",
      "name": "中文構式名稱",
      "slots": [["槽位A詞表"], ["槽位B詞表"]],
      "preferred": ["偏好詞"],
      "suppress": ["抑制詞"]
    }}
  ]
}}
"""

    def __init__(self, model: str = "gemma4:latest"):
        self.model = model
        self._ollama_proc = None

    def _ensure_ollama(self) -> bool:
        """確保 Ollama 在線"""
        try:
            req = urllib.request.Request("http://127.0.0.1:11434/api/tags", method="GET")
            with urllib.request.urlopen(req, timeout=2) as resp:
                if resp.status == 200:
                    return True
        except Exception:
            pass

        import shutil
        import subprocess
        ollama_path = shutil.which("ollama") or r"C:\Users\Yuktesha\AppData\Local\Programs\Ollama\ollama.exe"
        if not os.path.exists(ollama_path):
            return False

        CREATE_NO_WINDOW = 0x08000000
        self._ollama_proc = subprocess.Popen(
            [ollama_path, "serve"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=CREATE_NO_WINDOW,
        )
        for _ in range(12):
            try:
                with urllib.request.urlopen("http://127.0.0.1:11434/api/tags", timeout=2) as resp:
                    if resp.status == 200:
                        return True
            except Exception:
                time.sleep(1)
        return False

    def analyze_discrepancy(self, disc: Discrepancy) -> AnalysisResult:
        """呼叫本地 Gemma 分析單一失真項目"""
        self._ensure_ollama()
        compound_exp_hint = ""
        compound_act_hint = ""
        if len(disc.expected) == 1 and disc.context_before:
            cand = disc.context_before[-1] + disc.expected
            compound_exp_hint = f"（相鄰複合詞可能為：「{cand}」）"
        if len(disc.actual) == 1 and disc.context_before:
            cand_act = disc.context_before[-1] + disc.actual
            compound_act_hint = f"（相鄰實際組詞為：「{cand_act}」）"

        prompt = self.GEMMA_PROMPT_TEMPLATE.format(
            full_clause=disc.full_clause,
            expected=disc.expected,
            actual=disc.actual,
            context_before=disc.context_before,
            context_after=disc.context_after,
            compound_exp_hint=compound_exp_hint,
            compound_act_hint=compound_act_hint,
        )

        payload = json.dumps({
            "model": self.model,
            "prompt": prompt,
            "stream": False,
            "format": "json",
            "options": {"temperature": 0.1, "num_predict": 900}
        }).encode("utf-8")

        req = urllib.request.Request(
            OLLAMA_API_URL,
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST"
        )

        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                data = json.loads(resp.read().decode("utf-8"))
                raw_response = data.get("response", "").strip()
                return self._parse_json_response(raw_response)
        except Exception as e:
            print(f"[GemmaSemanticAnalyst] 連線 Ollama 失敗 ({e})，啟動智慧啟發式降級模式")
            return self._heuristic_fallback(disc)

    def _parse_json_response(self, raw_text: str) -> AnalysisResult:
        """從 Gemma 回應文本中解析 JSON"""
        try:
            parsed = json.loads(raw_text)
            return AnalysisResult(
                analysis=parsed.get("analysis", ""),
                new_words=parsed.get("new_words", []),
                transitions=parsed.get("transitions", []),
                patterns=parsed.get("patterns", []),
            )
        except json.JSONDecodeError:
            match = re.search(r"\{.*\}", raw_text, re.DOTALL)
            if match:
                try:
                    parsed = json.loads(match.group(0))
                    return AnalysisResult(
                        analysis=parsed.get("analysis", ""),
                        new_words=parsed.get("new_words", []),
                        transitions=parsed.get("transitions", []),
                        patterns=parsed.get("patterns", []),
                    )
                except Exception:
                    pass
        return AnalysisResult(analysis="JSON 解析失敗")

    def _heuristic_fallback(self, disc: Discrepancy) -> AnalysisResult:
        """
        本機無 LLM 或連線逾時時的智慧啟發式保底分析
        """
        transitions = []
        new_words = []

        if disc.context_before and disc.expected:
            last_word = disc.context_before[-2:]
            transitions.append({
                "prev": last_word,
                "curr": disc.expected,
                "weight": 25.0,
                "type": "collocation"
            })
            if disc.actual:
                transitions.append({
                    "prev": last_word,
                    "curr": disc.actual,
                    "weight": -35.0,
                    "type": "anti_confusion"
                })

        if len(disc.expected) >= 2:
            new_words.append({
                "word": disc.expected,
                "weight": 1800
            })

        return AnalysisResult(
            analysis=f"啟發式規則生成：強化 '{disc.context_before}' -> '{disc.expected}'，抑制 '{disc.actual}'",
            new_words=new_words,
            transitions=transitions,
            patterns=[],
        )


# =============================================================================
# 5. 語意知識自進化器 (SemanticSelfEvolver)
# =============================================================================

class SemanticSelfEvolver:
    """
    語意自進化器：將提煉出的語意規則即時寫入字典、轉移矩陣與構式庫，並觸發引擎熱重載
    """

    def __init__(
        self,
        bridge: RuhiEngineBridge,
        user_dict_path: Optional[str] = None,
        transitions_path: Optional[str] = None,
        patterns_path: Optional[str] = None,
    ):
        self.bridge = bridge
        self.annotator = BopomofoAnnotator(data_dir=bridge._data_dir)

        self.user_dict_path = user_dict_path or bridge._user_dict_path
        self.transitions_path = (
            transitions_path or os.path.join(bridge._data_dir, "transitions_user.tsv")
        )
        self.patterns_path = (
            patterns_path or os.path.join(bridge._data_dir, "patterns_user.jsonl")
        )

    def evolve(self, analysis_result: AnalysisResult) -> Tuple[int, int, int]:
        """
        寫入所有規則並熱重載
        Returns:
            (新增詞彙數, 新增轉移數, 新增構式數)
        """
        w_count = self._add_words(analysis_result.new_words)
        t_count = self._add_transitions(analysis_result.transitions)
        p_count = self._add_patterns(analysis_result.patterns)

        # 觸發熱重載
        if w_count > 0:
            self.bridge.reload_user_dict(self.user_dict_path)
        if t_count > 0:
            self.bridge.reload_transitions(self.transitions_path)

        return w_count, t_count, p_count

    def _add_words(self, words: List[Dict[str, Any]]) -> int:
        if not words:
            return 0

        existing: Set[str] = set()
        if os.path.exists(self.user_dict_path):
            with open(self.user_dict_path, "r", encoding="utf-8") as f:
                for line in f:
                    parts = line.strip().split("\t")
                    if parts and not line.startswith("#"):
                        existing.add(parts[0].strip())

        new_entries = []
        for w_item in words:
            word = w_item.get("word", "").strip()
            if not word or len(word) < 2 or word in existing:
                continue

            weight = int(w_item.get("weight", 1800))
            bpmf = w_item.get("bpmf") or self.annotator.annotate(word)
            if not bpmf or "?" in bpmf:
                continue

            new_entries.append(f"{word}\t{bpmf}\t{weight}\tAI_evolved\tGemma evolved")
            existing.add(word)

        if new_entries:
            with open(self.user_dict_path, "a", encoding="utf-8") as f:
                for entry in new_entries:
                    f.write(entry + "\n")
            print(f"[SemanticSelfEvolver] 新增使用者自建詞庫 {len(new_entries)} 筆")

        return len(new_entries)

    def _add_transitions(self, transitions: List[Dict[str, Any]]) -> int:
        if not transitions:
            return 0

        today_str = date.today().isoformat()
        new_lines = []

        for item in transitions:
            prev = item.get("prev", "").strip()
            curr = item.get("curr", "").strip()
            if not prev or not curr:
                continue

            weight = float(item.get("weight", 20.0))
            t_type = item.get("type", "collocation")
            new_lines.append(f"{prev}\t{curr}\t{weight:.1f}\t{t_type}\t{today_str}\n")

        if new_lines:
            with open(self.transitions_path, "a", encoding="utf-8") as f:
                for line in new_lines:
                    f.write(line)
            print(f"[SemanticSelfEvolver] 追加轉移矩陣 {len(new_lines)} 筆")

        return len(new_lines)

    def _add_patterns(self, patterns: List[Dict[str, Any]]) -> int:
        if not patterns:
            return 0

        existing_ids = set()
        if os.path.exists(self.patterns_path):
            with open(self.patterns_path, "r", encoding="utf-8") as f:
                for line in f:
                    if line.strip():
                        try:
                            obj = json.loads(line)
                            if "pattern_id" in obj:
                                existing_ids.add(obj["pattern_id"])
                        except Exception:
                            pass

        added = 0
        with open(self.patterns_path, "a", encoding="utf-8") as f:
            for p in patterns:
                pid = p.get("pattern_id")
                if not pid or pid in existing_ids:
                    continue
                # 補充預設分數
                if "bonus" not in p:
                    p["bonus"] = 35.0
                if "penalty" not in p:
                    p["penalty"] = -18.0
                if "punctuation" not in p:
                    p["punctuation"] = ["，", "。", "！", "？"]

                f.write(json.dumps(p, ensure_ascii=False) + "\n")
                existing_ids.add(pid)
                added += 1

        if added > 0:
            print(f"[SemanticSelfEvolver] 新增因式分解構式 {added} 筆至 patterns_user.jsonl")
        return added


# =============================================================================
# 6. 訓練協調器 (RuhiSemanticTrainer)
# =============================================================================

class RuhiSemanticTrainer:
    """
    語意自進化訓練協調器：
    執行完整閉環訓練：
    Text A -> Keys B -> Composed C -> Diff A vs C -> Gemma -> Evolve -> Re-type C' -> Report
    """

    def __init__(
        self,
        bridge: Optional[RuhiEngineBridge] = None,
        model: str = "gemma4:latest",
    ):
        self.bridge = bridge or RuhiEngineBridge()
        self.typist = RuhiVirtualTypist(self.bridge)
        self.analyzer = GranularDiffAnalyzer()
        self.analyst = GemmaSemanticAnalyst(model=model)
        self.evolver = SemanticSelfEvolver(self.bridge)

    def train_text(self, text: str, max_iterations: int = 2) -> TrainingReport:
        """
        針對特定文字段落進行自進化閉環訓練
        """
        text = text.strip()
        print(f"\n=======================================================")
        print(f"🚀 開始語意自進化訓練：長度 {len(text)} 字")
        print(f"=======================================================")

        # 1. 初始虛擬打字 (Iteration 0)
        c0 = self.typist.type_text(text)
        acc0 = self.analyzer.calculate_accuracy(text, c0)
        discs0 = self.analyzer.extract_discrepancies(text, c0)

        print(f"[Iter 0] 初始組字結果：{c0}")
        print(f"[Iter 0] 初始準確度  ：{acc0 * 100:.2f}% | 失真數：{len(discs0)}")

        if not discs0 or acc0 >= 0.9999:
            print("🎉 初始組字完全吻合！無需調整規則。")
            return TrainingReport(
                source_text=text,
                original_composed=c0,
                final_composed=c0,
                initial_accuracy=acc0,
                final_accuracy=acc0,
                discrepancies_before=0,
                discrepancies_after=0,
                evolved_words=0,
                evolved_transitions=0,
                evolved_patterns=0,
            )

        current_composed = c0
        current_acc = acc0
        current_discs = discs0
        total_w, total_t, total_p = 0, 0, 0
        details = []

        for iter_num in range(1, max_iterations + 1):
            print(f"\n--- 進入訓練迭代第 {iter_num} 輪 (剩餘失真 {len(current_discs)} 處) ---")
            round_w, round_t, round_p = 0, 0, 0

            # 依序診斷每一處失真
            for idx, disc in enumerate(current_discs, 1):
                print(f"\n  [失真 {idx}/{len(current_discs)}]")
                print(f"  語境：「{disc.full_clause}」")
                print(f"  期望：'{disc.expected}' vs 實際：'{disc.actual}'")

                # Gemma 語意診斷
                analysis = self.analyst.analyze_discrepancy(disc)
                print(f"  🤖 Gemma 診斷：{analysis.analysis}")

                # 寫入知識庫並熱重載
                w_count, t_count, p_count = self.evolver.evolve(analysis)
                round_w += w_count
                round_t += t_count
                round_p += p_count

                details.append(
                    f"Iter {iter_num} | '{disc.expected}' vs '{disc.actual}': "
                    f"+{w_count} words, +{t_count} trans, +{p_count} patterns"
                )

                # 顯卡散熱間歇保護
                time.sleep(3.0)

            total_w += round_w
            total_t += round_t
            total_p += round_p

            # 重新鍵入驗證 (Re-type)
            current_composed = self.typist.type_text(text)
            current_acc = self.analyzer.calculate_accuracy(text, current_composed)
            current_discs = self.analyzer.extract_discrepancies(text, current_composed)

            print(f"\n[Iter {iter_num}] 重測組字結果：{current_composed}")
            print(f"[Iter {iter_num}] 最新準確度  ：{current_acc * 100:.2f}% | 剩餘失真：{len(current_discs)}")

            if not current_discs or current_acc >= 0.9999:
                print(f"✨ 恭喜！在第 {iter_num} 輪訓練後，所有失真均已徹底消除！")
                break

        print("\n=======================================================")
        print(f"📊 訓練總結報告：")
        print(f"   初始準確度：{acc0 * 100:.2f}% -> 最終準確度：{current_acc * 100:.2f}%")
        print(f"   失真數消減：{len(discs0)} 處 -> {len(current_discs)} 處")
        print(f"   累計進化：+{total_w} 詞彙, +{total_t} 轉移, +{total_p} 構式")
        print("=======================================================\n")

        return TrainingReport(
            source_text=text,
            original_composed=c0,
            final_composed=current_composed,
            initial_accuracy=acc0,
            final_accuracy=current_acc,
            discrepancies_before=len(discs0),
            discrepancies_after=len(current_discs),
            evolved_words=total_w,
            evolved_transitions=total_t,
            evolved_patterns=total_p,
            details=details,
        )

    def train_file(self, filepath: str, max_lines: int = 10) -> List[TrainingReport]:
        """讀取檔案進行批次訓練"""
        reports = []
        if not os.path.exists(filepath):
            print(f"檔案不存在：{filepath}")
            return reports

        with open(filepath, "r", encoding="utf-8") as f:
            lines = [line.strip() for line in f if line.strip() and not line.startswith("#")]

        print(f"開始處理檔案：{filepath}，共 {len(lines)} 行有效文本（選取前 {max_lines} 行）")
        for line in lines[:max_lines]:
            if len(line) < 4:
                continue
            rep = self.train_text(line)
            reports.append(rep)

        return reports


# =============================================================================
# 7. 命令列入口 (CLI Entry)
# =============================================================================

def main():
    parser = argparse.ArgumentParser(description="如喜閉環自進化 AI 語意訓練器")
    parser.add_argument("--text", type=str, help="直接指定單一段落進行訓練")
    parser.add_argument("--file", type=str, help="指定文本檔案進行訓練")
    parser.add_argument("--corpus", type=str, help="指定清洗語料庫目錄進行抽樣訓練")
    parser.add_argument("--model", type=str, default="gemma4:latest", help="Ollama 模型名稱")
    parser.add_argument("--max-iter", type=int, default=2, help="單一段落最大迭代次數")

    args = parser.parse_args()

    trainer = RuhiSemanticTrainer(model=args.model)

    if args.text:
        trainer.train_text(args.text, max_iterations=args.max_iter)
    elif args.file:
        trainer.train_file(args.file)
    elif args.corpus:
        files = glob.glob(os.path.join(args.corpus, "*.txt"))
        print(f"在語料庫目錄 {args.corpus} 發現 {len(files)} 個檔案，開始抽樣訓練...")
        for fpath in files[:3]:
            trainer.train_file(fpath, max_lines=3)
    else:
        # 預設示範測試：使用使用者於 LINE 提及的經典情境
        sample = "現在硬體環境增強多少倍了？"
        trainer.train_text(sample, max_iterations=args.max_iter)

    trainer.bridge.destroy()


if __name__ == "__main__":
    main()
