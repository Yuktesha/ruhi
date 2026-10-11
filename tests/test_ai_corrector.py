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
如喜 (RuhI) 通用智慧型輸入法 - AI 文字校正中樞與工作目標替換機制自動化測試
tests/test_ai_corrector.py
=============================================================================
全面驗證：
1. 啟發式校正引擎：同音錯別字、高頻筆誤、台灣繁體在地化詞庫、標點全形正規化
2. 多軌 AI 協同與梯隊降級機制 (Gemini ➔ Gemma ➔ Heuristic)
3. 差異與修改原因結構化報告 (TextChange / Diff)
4. 最近送出文字緩衝區 (RecentCommittedTracker) 邊界截取與字數計算
5. 工作目標文字校正控制器 (TargetTextCorrector) 模擬端對端流程
"""

import json
import os
import sys
import unittest
from unittest.mock import MagicMock, patch

sys.stdout.reconfigure(encoding="utf-8")

CURR_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.dirname(CURR_DIR)
if RUHI_ROOT not in sys.path:
    sys.path.insert(0, RUHI_ROOT)

from ruhi_brain.ai_corrector import (
    RuhiAICorrector,
    HeuristicCorrector,
    GeminiCorrector,
    OllamaGemmaCorrector,
    TextCorrectionResult,
    extract_diff_changes,
)
from ruhi_win11.target_corrector import (
    RecentCommittedTracker,
    TargetTextCorrector,
    TargetCorrectionAction,
)


class TestRuhiAICorrector(unittest.TestCase):
    """驗證 AI 文字校正大腦功能"""

    def setUp(self):
        self.heuristic = HeuristicCorrector()
        self.corrector = RuhiAICorrector(backend="heuristic")

    # -------------------------------------------------------------------------
    # 測試 1: 啟發式錯別字與同音誤植修復
    # -------------------------------------------------------------------------
    def test_heuristic_typo_correction(self):
        """驗證常見錯字與同音筆誤能精準校正"""
        test_cases = [
            ("這件事情以經按步就班處理完畢。", "這件事情已經按部就班處理完畢。"),
            ("請粒刻前往現場！", "請立刻前往現場！"),
            ("今天寫程式真的好心類", "今天寫程式真的好心累"),
            ("希望大家再接在勵，更上一層樓！", "希望大家再接再厲，更上一層樓！"),
            ("這各功能應該速效要見效", "這個功能應該速效藥見效"),
            ("那麼，現再來測試看看。", "那麼，現在來測試看看。"),
        ]

        for raw, expected in test_cases:
            res = self.heuristic.correct(raw)
            self.assertTrue(res.has_corrections, f"應該偵測出修訂: {raw}")
            self.assertEqual(res.corrected_text, expected, f"校正結果不符: {raw}")
            self.assertGreater(len(res.changes), 0)

    # -------------------------------------------------------------------------
    # 測試 2: 台灣繁體在地化慣用詞轉換（嚴禁支語）
    # -------------------------------------------------------------------------
    def test_taiwan_localization_terms(self):
        """驗證資訊科技非台灣本土用語轉為標準台灣慣用語"""
        test_cases = [
            ("我們的服務器內存不足", "我們的伺服器記憶體不足"),
            ("這段代碼的算法非常優秀", "這段程式碼的演算法非常優秀"),
            ("項目默認配置了緩存與數組結構", "專案預設配置了快取與陣列結構"),
        ]

        for raw, expected in test_cases:
            res = self.heuristic.correct(raw)
            self.assertTrue(res.has_corrections)
            self.assertEqual(res.corrected_text, expected)
            for ch in res.changes:
                self.assertEqual(ch.category, "localization")

    # -------------------------------------------------------------------------
    # 測試 3: 中文標點符號全形正規化
    # -------------------------------------------------------------------------
    def test_punctuation_normalization(self):
        """驗證中文句子中的半形標點轉為全形"""
        raw = "你好,今天天氣真好!你在寫程式嗎?"
        res = self.heuristic.correct(raw)
        self.assertTrue(res.has_corrections)
        self.assertEqual(res.corrected_text, "你好，今天天氣真好！你在寫程式嗎？")

    # -------------------------------------------------------------------------
    # 測試 4: 完全正確句子不產生誤改 (False Positive Prevention)
    # -------------------------------------------------------------------------
    def test_correct_sentence_no_change(self):
        """驗證完全合法的繁體中文句子不會被無端篡改"""
        raw = "如喜輸入法是專為繁體中文使用者打造的高效能工具。"
        res = self.heuristic.correct(raw)
        self.assertFalse(res.has_corrections)
        self.assertEqual(res.corrected_text, raw)
        self.assertEqual(len(res.changes), 0)

    # -------------------------------------------------------------------------
    # 測試 5: Mock 測試 Google Gemini 遠端 API 回應與容錯
    # -------------------------------------------------------------------------
    @patch("urllib.request.urlopen")
    def test_gemini_corrector_mock(self, mock_urlopen):
        """驗證 Google Gemini API 成功解析與結構化輸出"""
        mock_response = MagicMock()
        mock_response.read.return_value = json.dumps({
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {
                                "text": json.dumps({
                                    "corrected_text": "今天會議已經圓滿結束。",
                                    "has_corrections": True,
                                    "corrections": [
                                        {
                                            "original": "以經",
                                            "replacement": "已經",
                                            "category": "homophone",
                                            "reason": "同音誤植"
                                        }
                                    ]
                                })
                            }
                        ]
                    }
                }
            ]
        }).encode("utf-8")
        mock_urlopen.return_value.__enter__.return_value = mock_response

        gemini = GeminiCorrector(api_key="test_key_fake")
        res = gemini.correct("今天會議以經圓滿結束。")

        self.assertTrue(res.has_corrections)
        self.assertEqual(res.corrected_text, "今天會議已經圓滿結束。")
        self.assertEqual(res.backend_used, "gemini")

    # -------------------------------------------------------------------------
    # 測試 6: 梯隊自動降級機制 (Auto Fallback)
    # -------------------------------------------------------------------------
    def test_auto_fallback_to_heuristic(self):
        """當未配置 Gemini API Key 且本機 Ollama 離線時，自動降級為啟發式引擎"""
        corrector = RuhiAICorrector(
            backend="auto",
            gemini_api_key=None,
            ollama_host="http://127.0.0.1:99999",  # 無效端口必離線
        )
        res = corrector.correct("粒刻出發！")
        self.assertTrue(res.has_corrections)
        self.assertEqual(res.corrected_text, "立刻出發！")
        self.assertEqual(res.backend_used, "heuristic")


class TestTargetTextCorrection(unittest.TestCase):
    """驗證工作目標視窗文字捕獲與替換機制"""

    def setUp(self):
        self.tracker = RecentCommittedTracker(max_chars=100)
        self.corrector = TargetTextCorrector()

    # -------------------------------------------------------------------------
    # 測試 7: 最近送出歷史緩衝區 (RecentCommittedTracker) 截取
    # -------------------------------------------------------------------------
    def test_recent_committed_tracker(self):
        """驗證依據標點符號精確截取最後一句話"""
        self.tracker.append("第一句話已經送出。")
        self.tracker.append("第二句話以經完成了！")

        last_sent, char_len = self.tracker.get_last_sentence()
        self.assertEqual(last_sent, "第二句話以經完成了！")
        self.assertEqual(char_len, len("第二句話以經完成了！"))

    # -------------------------------------------------------------------------
    # 測試 8: 目標視窗文字校正模擬流程
    # -------------------------------------------------------------------------
    def test_target_corrector_simulation(self):
        """模擬工作目標送入字串，觸發校正生成動作報告"""
        self.corrector.record_committed_text("測試代碼的算法以經完成，服務器正常。")

        # 取得最後一句
        last_sent, char_len = self.corrector.recent_tracker.get_last_sentence()
        result = self.corrector.corrector.correct(last_sent)

        self.assertTrue(result.has_corrections)
        self.assertIn("程式碼", result.corrected_text)
        self.assertIn("演算法", result.corrected_text)
        self.assertIn("已經", result.corrected_text)
        self.assertIn("伺服器", result.corrected_text)

        action = TargetCorrectionAction(
            source_type="recent_commit",
            original_text=last_sent,
            corrected_text=result.corrected_text,
            applied=True,
            summary=result.format_summary(),
            result=result,
        )

        self.assertTrue(action.applied)
        self.assertEqual(action.source_type, "recent_commit")


if __name__ == "__main__":
    unittest.main()
