# -*- coding: utf-8 -*-

import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
tests/test_pluggable_packs_and_clean_user_dict.py
驗證：
1. 使用者自建詞庫 user_dict.tsv 純淨為空 (Clean Zero-Default)
2. 通用詞句（如喜輸入法、沙盒、任何、暫時先告一段落）純憑內建詞庫即可 100% 命中
3. 專業外掛詞庫包 (pack_philosophy_ananda) 支援動態發現與手動/自動裝卸
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "ruhi_sandbox")))
from engine_bridge import RuhiEngineBridge

class TestPluggablePacksAndCleanUserDict(unittest.TestCase):
    def setUp(self):
        self.bridge = RuhiEngineBridge()

    def test_01_user_dict_is_clean_empty(self):
        """驗證發布狀態下 user_dict.tsv 為純淨空檔 (0 條殘留)"""
        user_dict_path = self.bridge._user_dict_path
        self.assertTrue(os.path.exists(user_dict_path), f"找不到 {user_dict_path}")
        with open(user_dict_path, "r", encoding="utf-8") as f:
            lines = [l.strip() for l in f if l.strip() and not l.startswith("#")]
        self.assertEqual(len(lines), 0, f"發布狀態下 user_dict 必須為 0 筆詞條，但發現了 {len(lines)} 筆！")

    def test_02_core_flow_without_any_user_dict(self):
        """驗證使用者不需要任何自建詞庫，核心通用詞彙與語句 100% 行雲流水打到底"""
        # 測試「任何」裸打首選 (rnj hgd)
        self.bridge.reset()
        for c in "rnjhgd":
            resp = self.bridge.process_key(c)
        self.assertEqual(resp.composed, "任何", "零自建詞庫下，裸打 ㄖㄣˋ ㄏㄜˊ 組句必為正統「任何」，絕無「任合」生造詞")

        # 測試吸收至 domain 的現代專名「如喜輸入法」
        self.bridge.reset()
        # 如 (rxd) 喜 (cef) 輸 (cx ) 入 (rxj) 法 (frf)
        for c in "rxdcefcx rxjfrf":
            resp = self.bridge.process_key(c)
        self.assertEqual(resp.composed, "如喜輸入法", "零自建詞庫下，現代領域專名「如喜輸入法」必精準命中")

        # 測試吸收至 domain 的「沙盒」
        self.bridge.reset()
        # 沙 (cr ) 盒 (hgd)
        for c in "cr hgd":
            resp = self.bridge.process_key(c)
        self.assertEqual(resp.composed, "沙盒", "零自建詞庫下，「沙盒」必精準命中")

    def test_03_pluggable_packs_discovery_and_mounting(self):
        """驗證專業/語境選配外掛詞庫支援探索與動態裝卸"""
        available_packs = self.bridge.list_available_packs()
        self.assertIn("pack_philosophy_ananda", available_packs, "packs 目錄中必須探索到 pack_philosophy_ananda")

        # 動態裝載外掛包
        loaded_count = self.bridge.load_pack("pack_philosophy_ananda")
        self.assertGreater(loaded_count, 700, f"成功掛載外掛包應 > 700 筆，實際載入 {loaded_count} 筆")

        # 掛載後可正常打出該哲學專用語（例如「三摩地」ㄙㄢ ㄇㄛˊ ㄉㄧˋ: ㄙ(s) ㄢ(m) ( ) + ㄇ(m) ㄛ(h) ˊ(d) + ㄉ(d) ㄧ(e) ˋ(j) ➔ "sm mhddej"）
        self.bridge.reset()
        for c in "sm mhddej":
            resp = self.bridge.process_key(c)
        self.assertEqual(resp.composed, "三摩地", "外掛包裝載後，專業哲學詞彙「三摩地」必流暢命中")

if __name__ == "__main__":
    unittest.main()
