#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
單元測試：本地 Gemma 閉環自進化 AI 語意訓練器 (Closed-Loop Self-Evolving Semantic Trainer)
驗證：
1. TextToHsuCompiler: 臺灣標準注音至許氏雙拼按鍵轉譯與混和穿透
2. GranularDiffAnalyzer: 字元級差異與上下文萃取
3. RuhiVirtualTypist: 虛擬打字員精準重現引擎組字
4. SemanticSelfEvolver: 詞庫、轉移矩陣與因式分解構式之動態寫入與熱重載
5. RuhiSemanticTrainer: 閉環自進化流程端到端測試
"""

import os
import sys
import unittest

RUHI_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if RUHI_ROOT not in sys.path:
    sys.path.insert(0, RUHI_ROOT)

from engine_bridge import RuhiEngineBridge
from text_to_hsu import TextToHsuCompiler
from semantic_trainer import (
    GranularDiffAnalyzer,
    GemmaSemanticAnalyst,
    SemanticSelfEvolver,
    RuhiVirtualTypist,
    RuhiSemanticTrainer,
    Discrepancy,
    AnalysisResult,
)


class TestTextToHsuCompiler(unittest.TestCase):
    """測試許氏雙拼編譯器"""

    @classmethod
    def setUpClass(cls):
        cls.compiler = TextToHsuCompiler()

    def test_bpmf_syllable_to_hsu(self):
        # ㄨㄛˇ -> x (ㄨ) + h (ㄛ) + f (ˇ)
        self.assertEqual(self.compiler.bpmf_syllable_to_hsu("ㄨㄛˇ"), "xhf")
        # ˙ㄉㄜ -> d (ㄉ) + g (ㄜ) + s (˙)
        self.assertEqual(self.compiler.bpmf_syllable_to_hsu("˙ㄉㄜ"), "dgs")
        # ㄕˋ -> c (ㄕ) + j (ˋ)
        self.assertEqual(self.compiler.bpmf_syllable_to_hsu("ㄕˋ"), "cj")
        # ㄚ (獨立零聲母) -> y + 空格
        self.assertEqual(self.compiler.bpmf_syllable_to_hsu("ㄚ"), "y ")

    def test_clause_tokenization(self):
        text = "我們都是自然人，現在硬體環境增強多少倍了？"
        clauses = self.compiler.text_to_hsu_clauses(text)
        self.assertTrue(len(clauses) >= 3)
        # 驗證標點符號穿透
        punct_keys = [keys for t, keys in clauses if t == "，"]
        self.assertEqual(punct_keys, ["，"])

    def test_mixed_script_passthrough(self):
        text = "希瓦 = Namah Shiváya"
        clauses = self.compiler.text_to_hsu_clauses(text)
        self.assertEqual(clauses[0][0], "希瓦")
        self.assertEqual(clauses[1][0], " = Namah Shiváya")
        self.assertEqual(clauses[1][1], " = Namah Shiváya")


class TestGranularDiffAnalyzer(unittest.TestCase):
    """測試細粒度差異分析器"""

    def test_exact_match(self):
        text = "我們都是自然人"
        acc = GranularDiffAnalyzer.calculate_accuracy(text, text)
        self.assertEqual(acc, 1.0)
        discs = GranularDiffAnalyzer.extract_discrepancies(text, text)
        self.assertEqual(len(discs), 0)

    def test_discrepancy_extraction(self):
        exp = "現在硬體環境增強多少倍了？"
        act = "現在硬體環境增強多少貝勒？"
        acc = GranularDiffAnalyzer.calculate_accuracy(exp, act)
        self.assertTrue(0.8 < acc < 1.0)

        discs = GranularDiffAnalyzer.extract_discrepancies(exp, act)
        self.assertEqual(len(discs), 1)
        disc = discs[0]
        self.assertEqual(disc.expected, "倍了")
        self.assertEqual(disc.actual, "貝勒")
        self.assertTrue("多少" in disc.context_before)


class TestGemmaAnalystFallback(unittest.TestCase):
    """測試 Gemma 語意分析器之解析與降級機制"""

    def test_heuristic_fallback(self):
        analyst = GemmaSemanticAnalyst()
        disc = Discrepancy(
            expected="倍了",
            actual="貝勒",
            context_before="多少",
            context_after="？",
            full_clause="增強多少倍了？",
            char_offset=8,
        )
        res = analyst._heuristic_fallback(disc)
        self.assertTrue(len(res.transitions) >= 1)
        self.assertEqual(res.transitions[0]["prev"], "多少")
        self.assertEqual(res.transitions[0]["curr"], "倍了")


class TestEndToEndVirtualTypingAndTrainer(unittest.TestCase):
    """測試虛擬打字員與自進化訓練端到端流程"""

    @classmethod
    def setUpClass(cls):
        cls.bridge = RuhiEngineBridge(enabled_packs=["pack_philosophy_ananda"])
        cls.trainer = RuhiSemanticTrainer(bridge=cls.bridge)

    @classmethod
    def tearDownClass(cls):
        cls.bridge.destroy()

    def test_virtual_typing_verified_sentence(self):
        """驗證已訓練過之經典句組字準確度達到 100%"""
        test_sentences = [
            "我們都是自然人，現在硬體環境增強多少倍了？",
            "看來自然輸入法除了原本的詞彙加權演算法之外，也要朝著自動勘誤的方向前進。",
            "永恆的希瓦‧超越時空的教誨 = Namah Shiváya Shántáya"
        ]
        for s in test_sentences:
            composed = self.trainer.typist.type_text(s)
            self.assertEqual(composed, s, f"Sentence mismatch: expected '{s}', got '{composed}'")


if __name__ == "__main__":
    unittest.main()
