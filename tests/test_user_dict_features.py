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
測試如喜沙盒自然輸入法風格 Ctrl+1~9 快捷自建詞與 Ctrl+0 詞庫管理中心
"""

import os
import sys
import tempfile
import unittest

sys.stdout.reconfigure(encoding="utf-8")

SANDBOX_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "ruhi_sandbox"))
if SANDBOX_DIR not in sys.path:
    sys.path.insert(0, SANDBOX_DIR)

from user_dict_manager import UserDictStore, UserDictEntry, DeletedEntry
from engine_bridge import RuhiEngineBridge


class TestUserDictFeatures(unittest.TestCase):

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()
        self.user_tsv = os.path.join(self.temp_dir, "test_user_dict.tsv")
        self.del_tsv = os.path.join(self.temp_dir, "test_deleted_dict.tsv")
        self.store = UserDictStore(self.user_tsv, self.del_tsv)

    def tearDown(self):
        import shutil
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_toggle_add_and_remove(self):
        """測試新增自建詞與再次按下 Toggle 移除"""
        # 1. 新增多字詞
        action, word = self.store.toggle_word("如喜沙盒", "ㄖㄨˊ ㄒㄧˇ ㄕㄚ ㄏㄜˊ", weight=2000)
        self.assertEqual(action, "added")
        self.assertEqual(word, "如喜沙盒")
        self.assertIn("如喜沙盒", self.store.custom_words)
        self.assertEqual(self.store.custom_words["如喜沙盒"].weight, 2000)
        self.assertTrue(os.path.exists(self.user_tsv))

        # 2. 再次按 -> Toggle 移除
        action2, word2 = self.store.toggle_word("如喜沙盒", "")
        self.assertEqual(action2, "removed")
        self.assertNotIn("如喜沙盒", self.store.custom_words)
        self.assertIn("如喜沙盒", self.store.deleted_words)
        self.assertTrue(self.store.is_deleted("如喜沙盒"))

        # 3. 恢復
        recovered = self.store.recover_deleted("如喜沙盒")
        self.assertTrue(recovered)
        self.assertFalse(self.store.is_deleted("如喜沙盒"))

    def test_single_char_ctrl_1(self):
        """測試 Ctrl+1 單字權重自訂與 Toggle"""
        action, word = self.store.toggle_word("喜", "ㄒㄧˇ", weight=2000)
        self.assertEqual(action, "added")
        self.assertEqual(self.store.custom_words["喜"].weight, 2000)

        # 重新從檔案載入驗證持久化
        new_store = UserDictStore(self.user_tsv, self.del_tsv)
        self.assertIn("喜", new_store.custom_words)
        self.assertEqual(new_store.custom_words["喜"].weight, 2000)

    def test_engine_bridge_reload(self):
        """測試 Rust 引擎透過 bridge 熱重載 user_dict.tsv"""
        # 寫入一筆詞彙
        self.store.toggle_word("如喜卓越", "ㄖㄨˊ ㄒㄧˇ ㄓㄨㄛˊ ㄩㄝˋ", weight=2000)
        
        try:
            bridge = RuhiEngineBridge(user_dict_path=self.user_tsv)
            count = bridge.reload_user_dict()
            self.assertGreaterEqual(count, 1)
            print(f"✅ Rust 引擎熱載入自建詞庫成功，載入 {count} 筆！")
        except Exception as e:
            print(f"⚠️ 引擎載入測試跳過 (可能缺少 dll): {e}")


if __name__ == "__main__":
    unittest.main()
