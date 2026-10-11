
import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
Ruhi (如喜) Input Method Framework - 核心功能自動化測試單元
"""

import os
import sys
import unittest

# 將專案根目錄加入路徑
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

from ruhi.ruhi_layout.hsu_fsm import HsuFSM
from ruhi.ruhi_lexicon.templates_manager import TemplatesManager
from ruhi.ruhi_lexicon.lexicon_manager import LexiconManager
from ruhi.ruhi_brain.engine import RuhiEngine, RuhiEngineMode

class TestRuhiFramework(unittest.TestCase):
    def setUp(self):
        self.engine = RuhiEngine()

    def test_01_hsu_fsm_syllable_parsing(self):
        """測試許氏鍵盤雙拼狀態機音節拼合"""
        fsm = HsuFSM()
        
        # 測試輸入「中」(ㄓ ㄨ ㄥ 1聲) -> J (ㄓ) + X (ㄨ) + L (ㄥ) + Space (1聲)
        # 或 J + X + L + 空白
        keys = ['j', 'x', 'l', ' ']
        for k in keys:
            fsm.parse_hsu_key(k)
        
        syls = fsm.syllables
        self.assertEqual(len(syls), 1)
        self.assertEqual(syls[0].bopomofo_display, "ㄓㄨㄥ")

    def test_02_template_macros_and_tsv(self):
        """測試 TSV 文字範本載入與巨集替換"""
        tpl_mgr = self.engine.templates
        item = tpl_mgr.match("addr")
        self.assertIsNotNone(item)
        expanded, _ = item.expand()
        self.assertIn("重慶南路", expanded)

        # 測試日期巨集
        today_item = tpl_mgr.match("today")
        self.assertIsNotNone(today_item)
        today_str, _ = today_item.expand()
        self.assertIn("年", today_str)
        self.assertIn("月", today_str)

        # 測試換行與排版控制
        letter_item = tpl_mgr.match("letter")
        self.assertIsNotNone(letter_item)
        letter_str, cur_pos = letter_item.expand()
        self.assertIn("\n", letter_str)
        self.assertIn("\t", letter_str)
        self.assertTrue(cur_pos >= 0)

    def test_03_q_key_cycling_and_popup(self):
        """測試 Q 鍵原地輪播與第 5 下彈出候選字清單窗"""
        engine = self.engine
        
        # 模擬輸入「我」(ㄨ ㄛ ˇ) -> X (ㄨ) + H (ㄛ) + F (3聲)
        for k in ['x', 'h', 'f']:
            engine.process_key(k)

        self.assertEqual(engine.composed_sentence, "我")
        
        # 連續按 4 次 Q 鍵 -> 輪播候選字
        for i in range(1, 5):
            res = engine.process_key('q')
            self.assertEqual(res["action"], "cycle_candidate")
            self.assertEqual(res["q_press_count"], i)
            self.assertEqual(engine.mode, RuhiEngineMode.NORMAL)

        # 第 5 次按 Q 鍵 -> 觸發完整候選清單彈出
        res = engine.process_key('q')
        self.assertEqual(res["action"], "show_candidate_popup")
        self.assertEqual(engine.mode, RuhiEngineMode.CANDIDATE_POPUP)

    def test_04_prefix_triggers(self):
        """測試反引號與波浪號引導鍵"""
        engine = self.engine
        
        # 按一下 ` -> 帶出文字範本
        res = engine.process_key('`')
        self.assertEqual(res["action"], "show_templates")
        self.assertEqual(engine.mode, RuhiEngineMode.TEMPLATE_SELECT)

        # 再按一下 ` -> 帶出數字快打
        res = engine.process_key('`')
        self.assertEqual(res["action"], "show_quick_num")
        self.assertEqual(engine.mode, RuhiEngineMode.QUICK_NUM)

        # 按 ~ (波浪號) -> 帶出專業詞庫切換
        res = engine.process_key('~')
        self.assertEqual(res["action"], "show_lexicon_switcher")
        self.assertEqual(engine.mode, RuhiEngineMode.LEXICON_SWITCH)

    def test_05_long_phrase_viterbi_segmentation(self):
        """測試 Viterbi 長詞優先斷詞 (例如 中華民國)"""
        engine = self.engine
        engine.reset_buffer()
        
        # 模擬連續打「中華民國」
        # ㄓㄨㄥ(jxl ) + ㄏㄨㄚˊ(hxrd) + ㄇㄧㄣˊ(memd) + ㄍㄨㄛˊ(gxhd)
        input_keys = [
            'j', 'x', 'l', ' ',
            'h', 'x', 'r', 'd',
            'm', 'e', 'n', 'd',
            'g', 'x', 'h', 'd'
        ]
        for k in input_keys:
            engine.process_key(k)

        self.assertEqual(engine.composed_sentence, "中華民國")

if __name__ == "__main__":
    unittest.main()
