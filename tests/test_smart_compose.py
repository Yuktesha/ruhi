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
tests/test_smart_compose.py
靈犀流式補全與頭碼盲打單元測試 (Unit Tests for Smart Compose & Head-code)
"""

import sys
import unittest
from pathlib import Path

# Add project root to sys.path
PROJECT_ROOT = Path(__file__).resolve().parent.parent
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from smart_composer import SmartComposer


class TestSmartComposer(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.composer = SmartComposer()

    def test_headcode_queries(self):
        """測試聲母頭碼縮寫查詢"""
        # 1. 砥睿者 (ㄉ ㄖ ㄓ / Hsu: drj)
        cands_drz = self.composer.query_headcode("ㄉㄖㄓ")
        self.assertIn("砥睿者", cands_drz, "頭碼 ㄉㄖㄓ 應命中 砥睿者")
        cands_drj = self.composer.query_headcode("drj")
        self.assertIn("砥睿者", cands_drj, "許氏頭碼 drj 應命中 砥睿者")

        # 2. 邏霸式 (ㄌ ㄅ ㄕ / Hsu: lbc)
        cands_lbs = self.composer.query_headcode("ㄌㄅㄕ")
        self.assertIn("邏霸式", cands_lbs, "頭碼 ㄌㄅㄕ 應命中 邏霸式")
        cands_lbc = self.composer.query_headcode("lbc")
        self.assertIn("邏霸式", cands_lbc, "許氏頭碼 lbc 應命中 邏霸式")

        # 3. 自串珠鍊 (ㄗ ㄔ ㄓ ㄌ / Hsu: zvjl)
        cands_zczl = self.composer.query_headcode("ㄗㄔㄓㄌ")
        self.assertIn("自串珠鍊", cands_zczl, "頭碼 ㄗㄔㄓㄌ 應命中 自串珠鍊")
        cands_zvjl = self.composer.query_headcode("zvjl")
        self.assertIn("自串珠鍊", cands_zvjl, "許氏頭碼 zvjl 應命中 自串珠鍊")

    def test_smart_compose_prediction(self):
        """測試流式接續建議 (Smart Compose Continuation)"""
        # 測試「謹此為父」的接續預測
        pred = self.composer.predict_continuation("謹此為父")
        self.assertTrue(len(pred) > 0, "應有預測接續詞")
        self.assertTrue("獻上" in pred or "自串珠鍊" in pred, f"預測結果應包含自然接續，實際得到: {pred}")

    def test_empty_and_unknown_context(self):
        """測試邊界條件與未知上下文"""
        self.assertEqual(self.composer.predict_continuation(""), "")
        cands_unknown = self.composer.query_headcode("XYZ")
        self.assertEqual(cands_unknown, [])


if __name__ == "__main__":
    unittest.main()
