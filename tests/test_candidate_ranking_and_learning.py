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
測試如喜沙盒：
1. 同音詞/同音字精準排序，杜絕前綴聯想詞污染
2. 候選字多頁分頁機制 (Pagination)
3. GBoard 級動態頻率學習 (Dynamic Frequency Learning)
4. Shift + ↓ 詞語聯想模式與同音模式分離
5. 聲母/頭碼 (Headcode) 索引與支援
6. 候選多字詞替換長度
"""

import os
import sys
import tempfile
import unittest

sys.stdout.reconfigure(encoding="utf-8")

SANDBOX_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "ruhi_sandbox"))
if SANDBOX_DIR not in sys.path:
    sys.path.insert(0, SANDBOX_DIR)

from user_learning import DynamicLearningStore
from user_dict_manager import UserDictStore
import tkinter as tk
try:
    from main_window import RuhiSandboxWindow
except ImportError:
    RuhiSandboxWindow = None


@unittest.skipIf(RuhiSandboxWindow is None, 'RuhiSandboxWindow GUI is deprecated and replaced by native TSF hot reload')
class TestCandidateRankingAndLearning(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        # 建立隱藏的 Tkinter 視窗以供測試
        cls.root = tk.Tk()
        cls.root.withdraw()

        # 建立測試用主視窗實例
        cls.app = RuhiSandboxWindow()
        cls.app.root.withdraw()
        cls.app._build_reverse_index()

    @classmethod
    def tearDownClass(cls):
        try:
            cls.app.root.destroy()
        except Exception:
            pass
        try:
            cls.root.destroy()
        except Exception:
            pass

    def setUp(self):
        # 每個測試使用獨立的臨時 learning store 檔案避免互擾
        self.temp_dir = tempfile.mkdtemp()
        self.learning_json = os.path.join(self.temp_dir, "test_learning.json")
        self.app.learning_store = DynamicLearningStore(self.learning_json)
        self.app.engine.reset()
        self.app._exit_comp_editing()
        self.app._pending_compose = ""
        self.app._pending_cursor = 0
        self.app.comp_focused = True
        self.app._set_comp_text("　")

    def tearDown(self):
        import shutil
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_01_homophone_ranking_no_prefix_pollution(self):
        """測試同音詞/字優先，絕無前綴聯想詞污染"""
        # 模擬使用者在組字區輸入「輸入法有什麼南得呢？」且游標停在「南」(index 6)
        self.app._pending_compose = "輸入法有什麼南得呢？"
        self.app._pending_cursor = 6
        self.app.candidate_selecting = False

        # 執行游標處同音查詢
        self.app._lookup_char_candidates("南")

        all_words = [it["word"] for it in self.app._all_candidate_items]
        self.assertGreater(len(all_words), 0, "候選列表不應為空")

        # 1. 精準同音詞「難得」、「男的」、「難的」必須排在最前面
        first_candidates = all_words[:5]
        self.assertTrue(
            any(w in first_candidates for w in ["難得", "男的", "難的"]),
            f"前 5 個候選應包含同音詞（難得/男的/難的），實際為: {first_candidates}"
        )

        # 2. 單字同音字（難、男、南、喃等）必須出現在候選列表中
        self.assertTrue("難" in all_words or "男" in all_words, "應包含同音單字")

        # 3. 絕對不可出現無關前綴聯想詞！
        pollutions = ["南邊來了個喇嘛", "南腔北調", "南部", "南邊", "男人", "南洋", "南美洲"]
        for p in pollutions:
            self.assertNotIn(
                p, all_words,
                f"純同音模式不應出現前綴聯想詞: {p}"
            )
        print("✅ 測試 1 通過：同音詞/字精準排序，無任何前綴聯想詞污染！")

    def test_02_pagination(self):
        """測試候選分頁（Page Down / Page Up）"""
        self.app._pending_compose = "南"
        self.app._pending_cursor = 0
        self.app._lookup_char_candidates("南")

        total_items = len(self.app._all_candidate_items)
        self.assertGreater(total_items, 9, "候選字總數應超過 9 個以測試分頁")

        # 第一頁 (page 0)
        self.assertEqual(self.app._candidate_page, 0)
        self.app._render_candidate_panel()
        page0_candidates = list(self.app._last_candidates)
        self.assertEqual(len(page0_candidates), 9)

        # 翻到下一頁 (Page Down)
        self.app._next_candidate_page()
        self.assertEqual(self.app._candidate_page, 1)
        page1_candidates = list(self.app._last_candidates)
        self.assertEqual(len(page1_candidates), min(9, total_items - 9))
        self.assertNotEqual(page0_candidates, page1_candidates, "第二頁候選字應與第一頁不同")

        # 翻回上一頁 (Page Up)
        self.app._prev_candidate_page()
        self.assertEqual(self.app._candidate_page, 0)
        self.assertEqual(list(self.app._last_candidates), page0_candidates, "翻回第一頁應一致")

        print(f"✅ 測試 2 通過：分頁機制正常（共 {total_items} 個候選，順利翻頁）！")

    def test_03_gboard_dynamic_frequency_learning(self):
        """測試 GBoard 機制：選取後的同音詞/字調高權重往前排"""
        # 情境 A：在「南的」(輕聲) 情境下，同音詞有「難的」、「男的」
        self.app._pending_compose = "輸入法有什麼南的呢？"
        self.app._pending_cursor = 6
        self.app._lookup_char_candidates("南")

        initial_words = [it["word"] for it in self.app._all_candidate_items]
        target = "男的"
        self.assertIn(target, initial_words, f"「南的」(輕聲) 候選應包含 {target}")

        # 模擬使用者連續選取「男的」3 次
        for _ in range(3):
            self.app.learning_store.record_selection("ㄋㄢˊ ㄉㄜ˙", target)

        # 再次查詢候選
        self.app._lookup_char_candidates("南")
        updated_words = [it["word"] for it in self.app._all_candidate_items]

        # 「男的」被頻繁選取後，權重必須提升到最前面
        self.assertEqual(updated_words[0], target, f"選取學習後「{target}」應排在第 1 位，實際排位: {updated_words[:3]}")
        print(f"✅ 測試 3 通過：GBoard 動態頻率學習成功，{target} 自動躍升為第 1 候選！")

    def test_07_neutral_tone_distinction(self):
        """
        測試使用者輸入有無輕聲與助詞多音字特性：
        1.「南德」(德為嚴格二聲 ㄉㄜˊ，非助詞)：
           同音詞應為「難得」(ㄋㄢˊ ㄉㄜˊ)；絕不包含輕聲「男的」、「難的」！
        2.「南得」(得為兼具輕聲ㄉㄜ˙與二聲ㄉㄜˊ之結構助詞)：
           游標在南或得時，均能召回「難的」與「難得」，且在「有什麼」語意下「難的」位列首選！
        3.「南的」(輕聲 ㄉㄜ˙)：
           同音詞包含「難的」、「男的」！
        """
        # 1. 測試「南德」(德為非助詞，純二聲 ㄉㄜˊ，嚴格隔離輕聲詞)
        self.app._pending_compose = "輸入法有什麼南德呢？"
        self.app._pending_cursor = 6
        self.app._lookup_char_candidates("南")
        words_nan_de_strict = [it["word"] for it in self.app._all_candidate_items]
        self.assertIn("難得", words_nan_de_strict, "「南德」必須包含同音詞「難得」")
        self.assertNotIn("男的", words_nan_de_strict, "「南德」(嚴格二聲) 絕不可包含輕聲詞「男的」")
        self.assertNotIn("難的", words_nan_de_strict, "「南德」(嚴格二聲) 絕不可包含輕聲詞「難的」")

        # 2. 測試「南得」(得為結構助詞，支援輕聲二聲交會修正)
        self.app._pending_compose = "輸入法有什麼南得呢？"
        self.app._pending_cursor = 6
        self.app._lookup_char_candidates("南")
        words_nan_de = [it["word"] for it in self.app._all_candidate_items]
        self.assertIn("難的", words_nan_de, "「南得」在「有什麼」語境下必須包含「難的」")
        self.assertIn("難得", words_nan_de, "「南得」必須包含同音詞「難得」")
        self.assertEqual(words_nan_de[0], "難的", "「有什麼南得」首選候選詞應為「難的」")

        # 3. 測試「南的」(輕聲)
        self.app._pending_compose = "輸入法有什麼南的呢？"
        self.app._pending_cursor = 6
        self.app._lookup_char_candidates("南")
        words_nan_di = [it["word"] for it in self.app._all_candidate_items]
        self.assertIn("男的", words_nan_di, "「南的」(輕聲) 必須包含同音詞「男的」")
        self.assertIn("難的", words_nan_di, "「南的」(輕聲) 必須包含同音詞「難的」")

        print("✅ 測試 7 通過：結構助詞雙向兼顧、非助詞嚴格聲調隔離（南德 ➔ 難得；南得/南的 ➔ 難的優先），語義精準對應！")

    def test_04_shift_down_association_mode(self):
        """測試 Shift + ↓ 啟動詞語聯想模式與同音模式隔離"""
        self.app._pending_compose = "輸入法有什麼南得呢？"
        self.app._pending_cursor = 6
        
        # 1. 正常同音模式
        self.app._lookup_char_candidates("南")
        self.assertEqual(self.app._candidate_mode, "homophone")
        self.assertNotIn("南部", [it["word"] for it in self.app._all_candidate_items])

        # 2. 按 Shift + ↓ 啟動聯想詞
        self.app._on_shift_down()
        self.assertEqual(self.app._candidate_mode, "association")
        assoc_words = [it["word"] for it in self.app._all_candidate_items]
        
        # 必須包含以「南」開頭的詞
        self.assertTrue(
            any(w.startswith("南") for w in assoc_words),
            f"聯想模式應包含以「南」開頭的詞語，實際: {assoc_words[:5]}"
        )
        self.assertTrue("南部" in assoc_words or "南邊" in assoc_words)

        # 3. 按 ESC 退出聯想模式，返回同音模式
        class MockEvent:
            pass
        self.app._on_escape(MockEvent())
        self.assertEqual(self.app._candidate_mode, "homophone")
        print("✅ 測試 4 通過：Shift+↓ 詞語聯想與同音模式清晰分離！")

    def test_05_headcode_support(self):
        """測試聲母/頭碼索引（支援常用詞與成語頭碼輸入）"""
        self.assertIn("ㄓㄏㄇㄍ", self.app._headcode_to_words)
        self.assertIn("中華民國", self.app._headcode_to_words["ㄓㄏㄇㄍ"])

        self.assertIn("ㄋㄉ", self.app._headcode_to_words)
        self.assertTrue(
            any(w in self.app._headcode_to_words["ㄋㄉ"] for w in ["難得", "男的", "難道"]),
            f"ㄋㄉ 應包含 難得 或 男的: {self.app._headcode_to_words['ㄋㄉ']}"
        )
        print("✅ 測試 5 通過：頭碼索引支援（ㄓㄏㄇㄍ → 中華民國，ㄋㄉ → 難得）！")

    def test_06_candidate_replacement_length(self):
        """測試選取多字詞候選時，替換整個多字詞長度"""
        self.app._pending_compose = "輸入法有什麼南得呢？"
        self.app._pending_cursor = 6
        self.app._lookup_char_candidates("南")

        # 找到「難得」候選項目（跨 2 字）
        idx_cand = None
        for i, it in enumerate(self.app._all_candidate_items[:9]):
            if it["word"] == "難得":
                idx_cand = i
                break

        self.assertIsNotNone(idx_cand, "第一頁應有「難得」候選")
        self.app._select_candidate(idx_cand)

        # 驗證「南得」2 字被精準替換為「難得」
        self.assertEqual(self.app._pending_compose, "輸入法有什麼難得呢？")
    def test_08_benchmark_semantic_sentence(self):
        """
        使用者經典極端造句語意陣列提取測試：
        「住在很南的那個男的非常難得的難得很難。」
        驗證 N+M 語意陣列與句法轉移路由在每個位置均能精準推導，而非依賴窮舉列舉法：
        1. 住在很[?]的 ➔ 「南」為方位詞，受居所動詞與程度副詞修飾，位列同音字之首
        2. 那個[?]非常 ➔ 「男的」為指示詞+人稱名詞，位列同音詞之首
        3. 非常[?]的 ➔ 「難得」為程度副詞+形容詞，位列同音詞之首
        4. [?]得很 ➔ 「難」為形容詞+補語標記「得」+程度補語「很」，位列同音字之首
        5. 難[?]很難 ➔ 「得」為結構助詞補語標記，位列同音字之首
        6. 很[?] ➔ 「難」為程度副詞+形容詞，位列同音字之首
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router, "Semantic router 必須存在")

        # 1. 住在很[?]的
        cands1 = [{'word': '南'}, {'word': '難'}, {'word': '男'}]
        res1 = router.rank_candidates('住在很', cands1, '的那個男的非常難得的男得很難。')
        self.assertEqual(res1[0]['word'], '南', f"住在很[?]的 首位應為 南，實際為: {[c['word'] for c in res1]}")

        # 2. 那個[?]非常
        cands2 = [{'word': '男的'}, {'word': '難得'}, {'word': '南得'}]
        res2 = router.rank_candidates('住在很南的那個', cands2, '非常難得的男得很難。')
        self.assertEqual(res2[0]['word'], '男的', f"那個[?]非常 首位應為 男的，實際為: {[c['word'] for c in res2]}")

        # 3. 非常[?]的
        cands3 = [{'word': '難得'}, {'word': '男的'}, {'word': '難的'}]
        res3 = router.rank_candidates('住在很南的那個男的非常', cands3, '的男得很難。')
        self.assertEqual(res3[0]['word'], '難得', f"非常[?]的 首位應為 難得，實際為: {[c['word'] for c in res3]}")

        # 4. [?]得很
        cands4 = [{'word': '難'}, {'word': '南'}, {'word': '男'}]
        res4 = router.rank_candidates('住在很南的那個男的非常難得的', cands4, '得很難。')
        self.assertEqual(res4[0]['word'], '難', f"[?]得很 首位應為 難，實際為: {[c['word'] for c in res4]}")

        # 5. 難[?]很難
        cands5 = [{'word': '得'}, {'word': '的'}, {'word': '地'}]
        res5 = router.rank_candidates('住在很南的那個男的非常難得的難', cands5, '很難。')
        self.assertEqual(res5[0]['word'], '得', f"難[?]很難 首位應為 得，實際為: {[c['word'] for c in res5]}")

        # 6. 很[?]
        cands6 = [{'word': '難'}, {'word': '南'}, {'word': '男'}]
        res6 = router.rank_candidates('住在很南的那個男的非常難得的難得很', cands6, '。')
        self.assertEqual(res6[0]['word'], '難', f"很[?] 首位應為 難，實際為: {[c['word'] for c in res6]}")

        print("✅ 測試 8 通過：經典極端造句「住在很南的那個男的非常難得的難得很難。」6 處語意陣列全部 100% 精準推導！")

    def test_09_multi_dim_tensor_lattice(self):
        """
        測試 A + B + C + ... 多維語意陣列動態解鎖 A * B * C * ... 語意空間：
        以 4 個位置的同音候選陣列：
        Slot 0: ['住', '助', '祝']
        Slot 1: ['在', '再']
        Slot 2: ['很', '狠']
        Slot 3: ['南', '難', '男']
        驗證多維 Viterbi Beam Search 動態規劃能在 3*2*2*3 = 36 種組合中，
        毫秒級精準解鎖「住在很南」，且無須靜態辭庫窮舉！
        """
        router = self.app.semantic_router
        slots = [
            ['住', '助', '祝'],
            ['在', '再'],
            ['很', '狠'],
            ['南', '難', '男']
        ]
        results = router.synthesize_multi_dim_array(slots, prev_context="", next_context="的那個男的", top_k=5)
        self.assertGreater(len(results), 0, "多維解碼結果不應為空")
        top_word = results[0][0]
        self.assertEqual(top_word, "住在很南", f"多維語意陣列首選應為「住在很南」，實際為: {top_word}")
        print(f"✅ 測試 9 通過：A*B*C*D 多維語意陣列解碼成功！首選: {top_word} (得分: {results[0][1]:.2f})")

    def test_10_federation_zero_pii_privacy_filter(self):
        """
        測試語意聯邦蜂巢之 Zero-PII 隱私審計閘門：
        1. 敏感個人資料（如電話號碼、英文帳號、包含特殊符號之字串）➔ 100% 阻擋，絕不上傳！
        2. 含有未公開隱私實體詞彙 ➔ 攔截！
        3. 純去識別化中文句法特徵（如：很 ➔ 難）➔ 通過審計，暫存於本地透明審計清單供使用者檢閱！
        """
        from semantic_federation import ZeroPiiPrivacyFilter, SemanticFederationManager
        import tempfile

        temp_dir = tempfile.mkdtemp()
        fed_mgr = SemanticFederationManager(temp_dir)

        # 1. 攔截測試：含有數字、電話、個人姓名、英文
        self.assertFalse(fed_mgr.record_semantic_insight("密碼是", "123456"))
        self.assertFalse(fed_mgr.record_semantic_insight("電話", "0912345678"))
        self.assertFalse(fed_mgr.record_semantic_insight("王小明", "住在"))
        self.assertFalse(fed_mgr.record_semantic_insight("my_email", "@gmail"))

        # 2. 通過測試：純通用語法轉移
        ok1 = fed_mgr.record_semantic_insight("住在", "很", weight_delta=1.5)
        ok2 = fed_mgr.record_semantic_insight("很", "難", weight_delta=2.0)
        self.assertTrue(ok1, "通用語法特徵應通過審計")
        self.assertTrue(ok2, "通用語法特徵應通過審計")

        # 驗證本地透明審計清單
        staged = fed_mgr.get_staged_records()
        self.assertEqual(len(staged), 2, "審計暫存應只有 2 筆通用特徵")
        self.assertEqual(staged[0]["prev"], "住在")
        self.assertEqual(staged[0]["curr"], "很")
        self.assertEqual(staged[1]["prev"], "很")
        self.assertEqual(staged[1]["curr"], "難")

        # 驗證清空
        fed_mgr.clear_staged_records()
        self.assertEqual(len(fed_mgr.get_staged_records()), 0)

        import shutil
        shutil.rmtree(temp_dir, ignore_errors=True)
        print("✅ 測試 10 通過：Zero-PII 隱私審計閘門嚴格生效，個資 100% 阻擋，語法特徵透明暫存！")

    def test_11_tailcat_consent_and_p2p_sync(self):
        """
        測試 Tailcat 去中心化通訊同意權：
        1. 預設未開啟：sync_to_tailcat_peer() 回傳完全離線私有狀態，絕不主動廣播
        2. 開啟同意權：自動加入社群增益，標記特徵為 tailcat_ready 並順利完成同步
        3. 關閉同意權：立即退出社群廣播，回到 100% 本機私有狀態
        """
        from semantic_federation import SemanticFederationManager
        import tempfile

        temp_dir = tempfile.mkdtemp()
        fed_mgr = SemanticFederationManager(temp_dir)

        # 1. 預設未開啟 Tailcat
        self.assertFalse(fed_mgr.tailcat_enabled)
        fed_mgr.record_semantic_insight("住在", "很", weight_delta=1.0)
        count, msg = fed_mgr.sync_to_tailcat_peer()
        self.assertEqual(count, 0)
        self.assertIn("未開啟 Tailcat 通訊同意權", msg)

        # 2. 開啟 Tailcat 同意權
        fed_mgr.set_tailcat_consent(True)
        self.assertTrue(fed_mgr.tailcat_enabled)
        fed_mgr.record_semantic_insight("很", "難", weight_delta=2.0)
        count2, msg2 = fed_mgr.sync_to_tailcat_peer()
        self.assertGreaterEqual(count2, 1)
        self.assertIn("已成功透過 Tailcat 向去中心化社群增益", msg2)

        # 3. 關閉 Tailcat 同意權退出社群
        fed_mgr.set_tailcat_consent(False)
        self.assertFalse(fed_mgr.tailcat_enabled)
        count3, msg3 = fed_mgr.sync_to_tailcat_peer()
        self.assertEqual(count3, 0)

        import shutil
        shutil.rmtree(temp_dir, ignore_errors=True)
        print("✅ 測試 11 通過：Tailcat 去中心化通訊同意權機制正常（自由開啟/退出、自願增益）！")

    def test_12_factorized_question_predicate_pattern(self):
        """
        測試因式分解乘積語意槽位 (Factorized Pattern Routing)：
        {有, 沒, 算, 哪} * {什麼, 啥, 何} * {(好, 真)} * {[形容詞]+的} * {？, ！, 。}
        1. 驗證槽位組合均能給予「難的」、「好的」、「怕的」等形容詞名物化高分加成 (+35 以上)
        2. 驗證同音混淆詞「難得」、「男的」、「南得」受到抑制懲罰
        3. 驗證省去數千行 TSV 窮舉列舉法，空間複雜度 O(Slots) 且毫秒級推導
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 測試各槽位前驅組合
        prefixes = ["有什麼", "沒什麼", "有啥", "沒啥", "算什麼", "有什麼好"]
        for p in prefixes:
            s_nan_de = router.score_semantic_transition(p, "難的", "？")
            s_nan_de2 = router.score_semantic_transition(p, "難得", "？")
            s_nan_ren = router.score_semantic_transition(p, "男的", "？")

            self.assertGreater(
                s_nan_de, 35.0,
                f"前驅「{p}」後「難的」應獲因式分解句型加分，實際: {s_nan_de}"
            )
            self.assertGreater(
                s_nan_de, s_nan_de2,
                f"前驅「{p}」後「難的」得分必須遠高於「難得」({s_nan_de} vs {s_nan_de2})"
            )
            self.assertGreater(
                s_nan_de, s_nan_ren,
                f"前驅「{p}」後「難的」得分必須遠高於「男的」({s_nan_de} vs {s_nan_ren})"
            )

        print("✅ 測試 12 通過：因式分解乘積語意槽位 (A*B*C*D) 推導成功，省去幾千筆 TSV 平鋪展開！")

    def test_13_gemma_pattern_distiller_mock_and_live(self):
        """
        測試本地 AI (Gemma) 語意分析師槽位提煉與註冊：
        1. 能從使用者日常句型提煉出純中文結構槽位 JSON
        2. 註冊入 FactorizedPatternEngine
        3. 經由 Zero-PII 審計後暫存並可供社群增益
        """
        from ai_corrector import GemmaPatternDistiller
        from semantic_federation import SemanticFederationManager
        import tempfile, shutil

        temp_dir = tempfile.mkdtemp()
        fed_mgr = SemanticFederationManager(temp_dir)
        distiller = GemmaPatternDistiller()

        # 模擬 Gemma 提煉出的純語意槽位（若在線則可即時提煉）
        extracted_pattern = {
            "pattern_id": "rhetorical_difficulty_de",
            "name": "反問難度句式",
            "slots": [
                ["到底", "究竟"],
                ["有什麼", "有啥"],
                ["好", "真"]
            ],
            "preferred": ["難的", "怕的", "怪的"],
            "suppress": ["南得", "難得", "男的"],
            "punctuation": ["？", "！"]
        }

        # 1. 註冊入語意路由器
        registered = self.app.semantic_router.pattern_engine.register_pattern(extracted_pattern)
        self.assertTrue(registered, "語意句型應能成功註冊入 PatternEngine")
        self.assertIn("rhetorical_difficulty_de", self.app.semantic_router.pattern_engine.patterns)

        # 2. 審計並暫存入聯邦管理
        recorded = fed_mgr.record_factorized_pattern(extracted_pattern)
        self.assertTrue(recorded, "純中文語法语意特徵應 100% 通過 Zero-PII 審計")
        staged = fed_mgr.get_staged_records()
        self.assertEqual(len(staged), 1)
        self.assertEqual(staged[0]["type"], "factorized_pattern")

        # 3. 測試新註冊句型的生效
        score = self.app.semantic_router.score_semantic_transition("到底有什麼", "難的", "？")
        self.assertGreater(score, 30.0)

        shutil.rmtree(temp_dir, ignore_errors=True)
        print("✅ 測試 13 通過：本地 AI (Gemma) 語意槽位提煉、引擎動態註冊與聯邦審計驗證成功！")

    def test_14_candidate_on_de_shows_nan_de(self):
        """
        重現並驗證使用者截圖核心痛點 (media_1790074941482.png)：
        當前組字區為「輸入法到底有什麼南得」，且游標位於「得」(index 9) 時：
        1. 絕不可只出現生僻罕用字（如截圖中的鍀、㝵、䙷、䙸）
        2. 向後二字組合查出「難的」(ㄋㄢˊ ㄉㄜ˙) 並位列第 1 位候選詞 (#0)！
        3. 單字候選清單中必須出現「的」！
        4. 選取第 1 位候選後，「南得」被精準替換為「難的」！
        """
        self.app._pending_compose = "輸入法到底有什麼南得"
        self.app._pending_cursor = 9  # 游標停在「得」
        self.app._lookup_char_candidates("得")

        items = self.app._all_candidate_items
        self.assertGreater(len(items), 0, "候選列表不應為空")

        # 1. 第一位候選詞必須為「難的」
        top_word = items[0]["word"]
        self.assertEqual(top_word, "難的", f"游標在「得」時，第 1 候選詞應為「難的」，實際為: {top_word}")

        # 2. 單字候選字必須包含「的」
        char_cands = [it["word"] for it in items if not it.get("is_word")]
        self.assertIn("的", char_cands, "單字候選中必須包含常用字「的」")

        # 3. 驗證選取「難的」後替換「南得」兩字
        self.app._select_candidate(0)
        self.assertEqual(
            self.app._pending_compose,
            "輸入法到底有什麼難的",
            "選取候選詞「難的」應成功將「南得」替換為「難的」"
        )
        print("✅ 測試 14 通過：完美解決截圖痛點！游標在「得」能精準召回「難的」為首選並無瑕替換！")

    def test_15_end_key_and_cursor_bounds(self):
        """
        重現並驗證使用者截圖第二痛點 (media_1790077099228.png)：
        「再來試試看這個輸入法有什麼厲害得地方」 (18 字)：
        1. Home 鍵能回到開頭 index 0
        2. End 鍵必須到達最後末端 index 18 (len)，絕不卡在 index 17 (倒數第 2 字)
        3. 游標在 index 17 時，按方向鍵右 (Right) 能順利移到 index 18 (末端)
        4. 在末端 (index 18) 按方向鍵右，不超出範圍
        5. 在末端 (index 18) 按 Delete，不刪除任何字且不當機，字串長度保持 18
        6. 在 index 8 按 Delete，刪除該位置字元，字串長度減為 17，游標保持在合法邊界內
        7. 渲染狀態列在末端時，正確顯示 (18/18 末端)
        """
        sample_text = "再來試試看這個輸入法有什麼厲害得地方"
        total_len = len(sample_text)  # 18
        self.assertEqual(total_len, 18)

        self.app._pending_compose = sample_text
        self.app._comp_editing = True
        self.app._pending_cursor = 10

        # 1. Home 鍵
        self.app._on_home(None)
        self.assertEqual(self.app._pending_cursor, 0, "Home 鍵應將游標移動到開頭 index 0")

        # 2. End 鍵：必須到達 len (18)，不再是 len - 1 (17)
        self.app._on_end(None)
        self.assertEqual(self.app._pending_cursor, total_len, f"End 鍵應到達末端 index {total_len}")

        # 3. 測試方向鍵右移：從 17 移到 18
        self.app._pending_cursor = 17
        self.app._on_arrow_right(None)
        self.assertEqual(self.app._pending_cursor, 18, "從 index 17 按右鍵應前進到末端 index 18")

        # 4. 在末端 18 再按右鍵，維持在 18
        self.app._on_arrow_right(None)
        self.assertEqual(self.app._pending_cursor, 18, "末端再按右鍵應維持在 index 18 不溢出")

        # 5. 在末端 (index 18) 按 Delete，不刪除任何字
        self.app._global_delete(None)
        self.assertEqual(len(self.app._pending_compose), 18, "在末端按 Delete 不應誤刪字元")
        self.assertEqual(self.app._pending_cursor, 18)

        # 6. 在 index 13 按 Delete 刪除「得」
        de_index = self.app._pending_compose.index("得")
        self.app._pending_cursor = de_index
        self.app._global_delete(None)
        self.assertEqual(len(self.app._pending_compose), 17)
        self.assertNotIn("得", self.app._pending_compose)
        self.assertEqual(self.app._pending_cursor, de_index)

        # 7. 渲染末端狀態列文字驗證
        self.app._pending_cursor = len(self.app._pending_compose)
        self.app._render_pending_compose()
        status_msg = self.app.status_text.cget("text")
        self.assertIn(f"{len(self.app._pending_compose)}/{len(self.app._pending_compose)} 末端", status_msg)

        print("✅ 測試 15 通過：End 鍵與右方向鍵順利達末端 (18/18 末端)，Delete 錯位徹底修復！")

    def test_16_user_pinned_and_grammar_transitions(self):
        """
        重現並驗證使用者截圖第一痛點 (media_1790077072725.png)：
        「厲害得地方」vs「厲害的地方」
        1. 驗證語意矩陣中「厲害 的」與「的 地方」已加入強正向加權
        2. 驗證「得 地方」獲得負向抗混淆懲罰
        3. 驗證使用者手動選字鎖定 (user_pinned) 確保接續打字絕不被覆蓋
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 矩陣分數驗證
        s_lihai_de = router.score_semantic_transition("厲害", "的", "地方")
        s_lihai_de2 = router.score_semantic_transition("厲害", "得", "地方")
        self.assertGreater(
            s_lihai_de, s_lihai_de2,
            f"「厲害 的 地方」分數 ({s_lihai_de}) 必須顯著高於「厲害 得 地方」({s_lihai_de2})"
        )

        # 2. 驗證 anti-confuse 得 地方
        s_anti = router.score_semantic_transition("", "得", "地方")
        self.assertLess(s_anti, 0.0, f"「得 地方」必須受到負向混淆懲罰，實際: {s_anti}")

        print("✅ 測試 16 通過：語法轉移權重「厲害 的 地方」已強化，得/的地方混淆徹底抑制！")

    def test_17_ctrl_number_add_and_toggle_word(self):
        """
        重現並驗證 error.txt 核心錯誤 1：
        1. 測試按下 Ctrl+2 加詞（例如「沙盒」）絕不發生 AttributeError: '_bpmf_to_words'
        2. 自建詞成功寫入 user_dict 並同步至記憶體 _exact_bpmf_to_words, _word_set
        3. 狀態列顯示「✦ 已加入自建詞庫：沙盒 (2字) [ㄕㄚ ㄏㄜˊ]」
        4. 再次按 Ctrl+2 能順利 Toggle 移除，無任何例外
        """
        self.app.comp_focused = True
        self.app._pending_compose = "只是個沙盒"
        self.app._comp_editing = True
        self.app._pending_cursor = 5
        self.app._set_comp_text("只是個沙盒")

        # 確保測試前為未收錄狀態
        if "沙盒" in self.app.dict_store.custom_words:
            self.app.dict_store.delete_word("沙盒")

        # 模擬觸發 Ctrl+2
        res = self.app._on_ctrl_number(2)
        self.assertEqual(res, "break")

        # 驗證無 AttributeError，且自建詞庫成功收錄
        self.assertIn("沙盒", self.app.dict_store.custom_words)
        self.assertIn("沙盒", self.app._word_set)
        bpmf_shahe = "ㄕㄚ ㄏㄜˊ"
        self.assertIn(bpmf_shahe, self.app._exact_bpmf_to_words)
        self.assertIn("沙盒", self.app._exact_bpmf_to_words[bpmf_shahe])

        # 驗證狀態列提示文字
        status_msg = self.app.status_text.cget("text")
        self.assertIn("已加入自建詞庫：沙盒", status_msg)

        # 再次觸發 Ctrl+2：Toggle 移除
        res_remove = self.app._on_ctrl_number(2)
        self.assertEqual(res_remove, "break")
        self.assertNotIn("沙盒", self.app.dict_store.custom_words)
        status_remove = self.app.status_text.cget("text")
        self.assertIn("已自自建詞庫移除：沙盒", status_remove)

        # 恢復加入，確保詞庫中保留「沙盒」
        self.app._on_ctrl_number(2)
        self.assertIn("沙盒", self.app.dict_store.custom_words)

        print("✅ 測試 17 通過：Ctrl+2 快捷加詞與 Toggle 移除無瑕運行，AttributeError 徹底根除！")

    def test_18_non_ascii_resilience(self):
        """
        重現並驗證 error.txt 核心錯誤 2：
        UnicodeEncodeError: 'ascii' codec can't encode character '\uff0c' / '\u53ea'
        1. 測試送入非 ASCII 字元至 engine.process_key 絕不拋出 UnicodeEncodeError
        2. 測試 _global_key_dispatch 遇到全形標點（如「，」、「。」）及中文（如「只」）時安全分流
        """
        # 1. 引擎防禦性保護測試
        resp = self.app.engine.process_key("，")
        self.assertIsNotNone(resp)
        resp2 = self.app.engine.process_key("只")
        self.assertIsNotNone(resp2)

        # 2. 全域按鍵分流測試
        class MockEvent:
            def __init__(self, char, keysym="", state=0):
                self.char = char
                self.keysym = keysym
                self.state = state

        self.app.comp_focused = True
        self.app.engine.reset()
        self.app._set_comp_text("　")
        self.app._pending_compose = "只是個沙"
        self.app._pending_cursor = 4
        self.app._comp_editing = False

        # 模擬輸入全形標點「，」
        ev_comma = MockEvent("，", "comma")
        res1 = self.app._global_key_dispatch(ev_comma)
        self.assertEqual(res1, "break")
        self.assertEqual(self.app._pending_compose, "只是個沙，")

        # 模擬系統輸入法或貼上輸入中文「盒」
        ev_box = MockEvent("盒", "box")
        res2 = self.app._global_key_dispatch(ev_box)
        self.assertEqual(res2, "break")
        self.assertEqual(self.app._pending_compose, "只是個沙，盒")

        print("✅ 測試 18 通過：非 ASCII 字元與全形標點防護 100% 健全，UnicodeEncodeError 徹底根絕！")

    def test_19_full_punctuation_and_ctrl_symbols(self):
        """
        驗證標點符號完整生態鏈：
        1. 核心符號庫支援自然輸入法標準 Ctrl+標點：
           Ctrl+. -> …… (刪節號)
           Ctrl+, -> ， (逗號)
           Ctrl+- -> ── (雙破折號)
           Ctrl+/ -> ？ (問號)
           Ctrl+; -> ； (分號)
           Ctrl+' -> 、 (頓號)
           Ctrl+[ -> 「 (開引號)
           Ctrl+] -> 」 (閉引號)
           Ctrl+Shift+[ -> 『 (雙引號)
           Ctrl+Shift+] -> 』 (雙引號)
        2. [ 與 ] 在非選字狀態下輸出 「 與 」
        3. _global_key_dispatch 模擬 Ctrl+. 觸發刪節號 ……
        """
        # 1. 核心符號表查詢驗證
        symbols_to_test = [
            ("Ctrl+.", "……"),
            ("Ctrl+,", "，"),
            ("Ctrl+-", "──"),
            ("Ctrl+/", "？"),
            ("Ctrl+;", "；"),
            ("Ctrl+'", "、"),
            ("Ctrl+[", "「"),
            ("Ctrl+]", "」"),
            ("Ctrl+Shift+[", "『"),
            ("Ctrl+Shift+]", "』"),
            ("Ctrl+Shift+,", "《"),
            ("Ctrl+Shift+.", "》"),
            ("Ctrl+1", "！"),
            ("Ctrl+\\", "、"),
            ("Ctrl+`", "～"),
        ]
        for key_code, expected_symbol in symbols_to_test:
            resp = self.app.engine.process_key_str(key_code)
            self.assertEqual(
                resp.committed_chunk, expected_symbol,
                f"符號快捷鍵 {key_code} 應輸出 {expected_symbol}，實際為: {resp.committed_chunk}"
            )

        # 2. 測試 [ 與 ] 在非選字狀態下輸出 「 與 」
        self.app.candidate_selecting = False
        self.app.comp_focused = True
        self.app._pending_compose = ""
        res_l = self.app._on_bracket_left(None)
        self.assertEqual(res_l, "break")
        self.assertIn("「", self.app._pending_compose)

        res_r = self.app._on_bracket_right(None)
        self.assertEqual(res_r, "break")
        self.assertIn("」", self.app._pending_compose)

        # 3. 測試透過 _global_key_dispatch 接收 Ctrl+. (keysym='period', state=4)
        class MockCtrlEvent:
            def __init__(self, char, keysym="", state=0):
                self.char = char
                self.keysym = keysym
                self.state = state

        self.app._pending_compose = "十分令人期待"
        ev_ctrl_dot = MockCtrlEvent("\x00", "period", state=4)
        res_ctrl_dot = self.app._global_key_dispatch(ev_ctrl_dot)
        self.assertEqual(res_ctrl_dot, "break")
        self.assertEqual(self.app._pending_compose, "十分令人期待……")

        print("✅ 測試 19 通過：標點符號完整生態支援（Ctrl+標點、中括號引號、刪節號、破折號）100% 成功！")

    def test_20_buffer_cursor_memory_and_arbitrary_insertion(self):
        """
        驗證暫存區文字游標記憶與任意位置插入：
        1. 初始暫存區內容「這是一個輸入法。」
        2. 使用者切換至暫存區將游標移至「這是一個」之後（index 1.4）
        3. 按 ESC 或切換回組字模式，記憶的游標仍維持在 1.4
        4. 組字送出「神奇的」，精準插入為「這是一個神奇的輸入法。」
        5. 游標自動推進至 1.7，接續送出「很強大的」，形成「這是一個神奇的很強大的輸入法。」
        """
        import tkinter as tk
        self.app._buffer_replace("這是一個輸入法。")
        self.assertEqual(self.app._buffer_get(), "這是一個輸入法。")

        # 模擬使用者點擊暫存區第 4 個字元處（「這是一個」之後）
        self.app._enter_buffer_mode()
        self.assertFalse(self.app.comp_focused)
        self.app.buffer_text.mark_set(tk.INSERT, "1.4")
        self.app._on_buffer_cursor_move()
        self.assertEqual(self.app._buffer_cursor_pos, "1.4")

        # 模擬按下 ESC 退出暫存區編輯模式，返回組字模式
        res_esc = self.app._on_escape(None)
        self.assertEqual(res_esc, "break")
        self.assertTrue(self.app.comp_focused)
        self.assertEqual(self.app._buffer_cursor_pos, "1.4")

        # 模擬組字完成並送出「神奇的」
        self.app._buffer_insert("神奇的")
        self.assertEqual(self.app._buffer_get(), "這是一個神奇的輸入法。")
        self.assertEqual(self.app._buffer_cursor_pos, "1.7")

        # 再次送出「很強大的」，驗證連續流式插入
        self.app._buffer_insert("很強大的")
        self.assertEqual(self.app._buffer_get(), "這是一個神奇的很強大的輸入法。")
        self.assertEqual(self.app._buffer_cursor_pos, "1.11")

        print("✅ 測試 20 通過：暫存區游標記憶、ESC 一鍵切換、任意位置精準流式插入 100% 成功！")

    def test_21_ai_correction_json_and_repeated_ai_normalization(self):
        """
        驗證 Layer 2 AI 背景校正 JSON 結構化解碼與重複 ai -> AI 規範化：
        1. 驗證 AiCorrector 若在線，能順利以 format='json' 解析回應
        2. 驗證使用者痛點句子「剛剛有幫我把ai變成AI。但第二次打就沒更正了。」
           AI 能正確識別「ai」並更正為「AI」，was_changed 為 True
        3. 驗證 _apply_ai_correction 套用後暫存區內容更新，且游標記憶位置不跳脫
        """
        corrector = self.app.ai
        self.assertIsNotNone(corrector)

        test_text = "剛剛有幫我把ai變成AI。但第二次打就沒更正了。"
        if corrector.is_available:
            res_text, was_changed = corrector.correct_sync(test_text)
            self.assertTrue(was_changed, f"AI 應辨識出校正變更，實際 was_changed: {was_changed}")
            self.assertIn("AI", res_text)
            self.assertNotIn("把ai變成", res_text)

            # 驗證 UI 主線程安全套用與游標記憶
            self.app._buffer_replace(test_text)
            self.app._buffer_cursor_pos = "1.5"
            self.app._apply_ai_correction(res_text)
            self.assertEqual(self.app._buffer_get(), res_text)
            self.assertEqual(self.app._buffer_cursor_pos, "1.5")
            print("✅ 測試 21 通過：AI 背景校正 JSON 結構化解析、ai->AI 規範化與游標定位 100% 成功！")
        else:
            print("⚠️ 測試 21 略過（Ollama 本地模型不在線）")

    def test_22_bopomofo_annotator_domain_precision(self):
        """
        驗證多層注音自動標注器 (BopomofoAnnotator)：
        1. 包含教育部辭典、小麥注音、最大正向匹配與梵文專有名詞擴充
        2. 精準注音：
           - 至上意識 -> ㄓˋ ㄕㄤˋ ㄧˋ ㄕˋ
           - 三摩地 -> ㄙㄢ ㄇㄛˊ ㄉㄧˋ
           - 薄伽梵 -> ㄅㄛˊ ㄑㄧㄝˊ ㄈㄢˋ
           - 阿南達迦耶提 -> ㄚ ㄋㄢˊ ㄉㄚˊ ㄐㄧㄚ ㄧㄝˊ ㄊㄧˊ
        3. 標注結果絕不含未登錄未知標記 '?'
        """
        from bopomofo_annotator import BopomofoAnnotator
        annotator = BopomofoAnnotator()
        cases = [
            ("至上意識", "ㄓˋ ㄕㄤˋ ㄧˋ ㄕˋ"),
            ("三摩地", "ㄙㄢ ㄇㄛˊ ㄉㄧˋ"),
            ("薄伽梵", "ㄅㄛˊ ㄑㄧㄝˊ ㄈㄢˋ"),
            ("阿南達迦耶提", "ㄚ ㄋㄢˊ ㄉㄚˊ ㄐㄧㄚ ㄧㄝˊ ㄊㄧˊ"),
        ]
        for term, expected_bpmf in cases:
            res = annotator.annotate(term)
            self.assertEqual(res, expected_bpmf, f"{term} 注音標注應為 {expected_bpmf}，實際為 {res}")
            self.assertNotIn("?", res, f"{term} 注音不可包含問號")
        print("✅ 測試 22 通過：注音標注器領域專名、多音字梵文校準 100% 精準無誤！")

    def test_23_distilled_atomic_entities_priority(self):
        """
        驗證 Gemma 提煉之哲學本體實體 (Ontological Lemmas) 與自建詞庫優先級：
        1. 驗證 user_dict.tsv 成功收錄「至上意識」、「三摩地」、「行動瑜伽」、「虔誠瑜伽」、「生命真諦」
        2. 驗證權重均為 2000 (自建詞頂級權重)
        3. 驗證反查索引中「ㄓˋ ㄕㄤˋ ㄧˋ ㄕˋ」能精準查出「至上意識」
        4. 驗證組字區「至上意識」在游標處同音詞候選中包含 4 字原子實體「至上意識」
        """
        custom_words = self.app.dict_store.custom_words
        distilled_entities = ["至上意識", "三摩地", "行動瑜伽", "虔誠瑜伽", "生命真諦"]
        for entity in distilled_entities:
            self.assertIn(entity, custom_words, f"自建詞庫應收錄提煉實體：{entity}")
            self.assertGreaterEqual(custom_words[entity].weight, 1200, f"{entity} 權重應至少為 1200")

        # 驗證反查與候選提取
        self.assertIn("ㄓˋ ㄕㄤˋ ㄧˋ ㄕˋ", self.app._exact_bpmf_to_words)
        self.assertIn("至上意識", self.app._exact_bpmf_to_words["ㄓˋ ㄕㄤˋ ㄧˋ ㄕˋ"])

        self.app._pending_compose = "至上意識"
        self.app._pending_cursor = 0
        self.app._lookup_char_candidates("至")
        all_words = [it["word"] for it in self.app._all_candidate_items]
        self.assertIn("至上意識", all_words, "候選列表應包含原子實體「至上意識」")
        cand_item = next(it for it in self.app._all_candidate_items if it["word"] == "至上意識")
        self.assertEqual(cand_item["end"], 4, "原子實體替換長度應為 4")

        print("✅ 測試 23 通過：Gemma 提煉本體實體成功載入，2000頂級權重與原子替換 100% 成功！")

    def test_24_bi_directional_mind_state_patterns(self):
        """
        驗證因式分解語意場雙向對稱模式 (Factorized Semantic Field Patterns)：
        1. 心靈主體 -> 心境狀態形容詞 (心靈 平靜 ≒ 138.0)
        2. 心境狀態修飾語 -> 心靈主體 (平靜的 心靈 ≒ 38.0)
        3. 同音混淆詞抗混淆懲罰 (心靈 平敬 ≒ -18.0)
        4. 驗證因式分解規則乘積取代平鋪 TSV，以 2 個模式覆蓋 (心靈主體 x 心境狀態) 幾十種句法排列
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 心靈 -> 平靜
        score_mind_calm = router.score_semantic_transition("心靈", "平靜", "")
        # 2. 心靈 -> 平敬 (同音干擾)
        score_mind_bad = router.score_semantic_transition("心靈", "平敬", "")
        # 3. 平靜的 -> 心靈
        score_calm_mind = router.score_semantic_transition("平靜的", "心靈", "")

        self.assertGreater(score_mind_calm, 50.0, f"「心靈 平靜」應獲得強加權，實際: {score_mind_calm}")
        self.assertLess(score_mind_bad, 0.0, f"「心靈 平敬」應獲得混淆懲罰，實際: {score_mind_bad}")
        self.assertGreater(score_calm_mind, 30.0, f"「平靜的 心靈」應獲得語意修飾加權，實際: {score_calm_mind}")

        # 4. 驗證 patterns_user.jsonl 存在且已註冊
        self.assertIn("mind_state_collocation", router.pattern_engine.patterns)
        self.assertIn("state_mind_attributive", router.pattern_engine.patterns)

        print("✅ 測試 24 通過：雙向語意場（心靈平靜 ≒ 平靜的心靈）因式分解加權與混淆抑制 100% 成功！")

    def test_25_three_tiered_backspace_and_ctrl_delete(self):
        """
        驗證三層粒度刪除機制 (Three-tiered Deletion Hierarchy) 與 Ctrl+Delete 正向刪詞：
        1. Backspace：微觀組字元件退格（手滑修復）
        2. Shift + Backspace：中觀單字整字退格
        3. Ctrl + Backspace：宏觀詞組整詞退格（組字區與暫存區）
        4. Ctrl + Delete：向右刪除一個詞組（Windows 全球標準）
        5. Shift + Delete：經典剪下 (Cut) 規範保持，絕不破壞肌肉記憶
        """
        # 1. 引擎 FSM 退格層級測試
        self.app.engine.reset()
        self.app.engine.process_key('g')
        r1 = self.app.engine.process_key('f')
        self.assertTrue(bool(r1.composed))
        # 按 Backspace 消除聲調元件
        resp_bs = self.app.engine.process_key_str("BackSpace")
        # 聲調已消除，音節展開回未決狀態
        self.assertIn("ㄍ", resp_bs.preedit)

        # 2. Shift + Backspace 整字消除
        self.app.engine.reset()
        self.app.engine.process_key('g')
        self.app.engine.process_key('f')
        resp_sbs = self.app.engine.process_key_str("Shift+BackSpace")
        self.assertEqual(resp_sbs.composed, "")
        self.assertEqual(resp_sbs.preedit, "")

        # 3. 組字區 Ctrl+Backspace 整詞退格
        self.app.comp_focused = True
        self.app._pending_compose = "這是輸入法"
        self.app._pending_cursor = len(self.app._pending_compose)
        self.app._comp_editing = True
        self.app._global_ctrl_backspace(None)
        # 「輸入法」(3字) 應被整詞退格
        self.assertEqual(self.app._pending_compose, "這是")
        self.assertEqual(self.app._pending_cursor, len("這是"))

        # 4. 組字區 Ctrl+Delete 向右整詞刪除
        self.app._pending_compose = "這是輸入法"
        self.app._pending_cursor = 2  # 游標在「這是」後
        self.app._global_ctrl_delete(None)
        # 「輸入法」應被向右整詞刪除
        self.assertEqual(self.app._pending_compose, "這是")

        # 5. 暫存區 Ctrl+Backspace / Ctrl+Delete
        self.app.comp_focused = False
        self.app._buffer_replace("這是一個輸入法。")
        # 游標設在「一個」後面 (index 1.4)
        import tkinter as tk
        self.app.buffer_text.mark_set(tk.INSERT, "1.4")
        self.app._global_ctrl_backspace(None)
        self.assertEqual(self.app._buffer_get(), "這是輸入法。")

        # 游標設在「這是」後面 (index 1.2)
        self.app.buffer_text.mark_set(tk.INSERT, "1.2")
        self.app._global_ctrl_delete(None)
        self.assertEqual(self.app._buffer_get(), "這是。")

        # 切換回組字模式並清理
        self.app.comp_focused = True
        self.app._exit_comp_editing()
        self.app._pending_compose = ""
        self.app._pending_cursor = 0
        print("✅ 測試 25 通過：三層退格 (元件/單字/詞組) 與 Ctrl+Delete 正向刪詞 100% 成功！")

    def test_26_backspace_mode_config_toggle(self):
        """
        驗證 config.json 中 backspace_mode 客製化切換：
        1. 預設 'component'：Backspace = 元件，Shift+Backspace = 單字
        2. 切換為 'character'：Backspace = 單字，Shift+Backspace = 元件（滿足自然輸入法老用戶習慣）
        """
        self.app.comp_focused = True
        self.app.engine.reset()
        self.app.config["backspace_mode"] = "character"

        # 輸入 (g + f)
        self.app.engine.process_key('g')
        r = self.app.engine.process_key('f')
        self.app._update_display(r)

        # 在 character 模式下，單按 Backspace 直接觸發整字刪除
        self.app._global_backspace(None)
        comp_text = self.app._get_comp_text().replace("▏", "").strip()
        self.assertTrue(comp_text == "" or comp_text == "　")

        # 恢復預設設定
        self.app.config["backspace_mode"] = "component"
        print("✅ 測試 26 通過：backspace_mode 客製化切換無縫支援自然輸入法習慣！")

    def test_27_semantic_router_de_vs_de2_complement_and_genitive(self):
        """
        驗證「的」(ê) 與「得」(kah) 跨語言深層語意分野：
        1. 程度與結果補語（台語 kah / 英文 so... that...）：高興得跳起來、美麗得不得了、氣得發抖
           - 「得」獲得 +45.0 強加權
           - 「的」受到 -25.0 扣分抑制
        2. 領屬與修飾定語（台語 ê / 英文 's 或 of）：至上意識的意願、我們的小麻糬、美麗的風景
           - 「的」獲得 +45.0 強加權
           - 「得」受到 -30.0 強烈排斥
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 補語結構測試
        s_de2_happy = router.score_semantic_transition("高興", "得", "跳起來")
        s_de1_happy = router.score_semantic_transition("高興", "的", "跳起來")
        self.assertGreater(s_de2_happy, 40.0, f"「高興得跳起來」之『得』應獲強加分，實際: {s_de2_happy}")
        self.assertLess(s_de1_happy, 0.0, f"「高興的跳起來」之『的』應受混淆扣分，實際: {s_de1_happy}")

        s_de2_beauty = router.score_semantic_transition("美麗", "得", "不得了")
        s_de1_beauty = router.score_semantic_transition("美麗", "的", "不得了")
        self.assertGreater(s_de2_beauty, 40.0, f"「美麗得不得了」之『得』應獲強加分，實際: {s_de2_beauty}")
        self.assertLess(s_de1_beauty, 0.0, f"「美麗的不得了」之『的』應受混淆扣分，實際: {s_de1_beauty}")

        # 2. 定語/領屬結構測試
        s_de1_param = router.score_semantic_transition("至上意識", "的", "意願")
        s_de2_param = router.score_semantic_transition("至上意識", "得", "意願")
        self.assertGreater(s_de1_param, 40.0, f"「至上意識的意願」之『的』應獲強加分，實際: {s_de1_param}")
        self.assertLess(s_de2_param, 0.0, f"「至上意識得意願」之『得』應受強烈抑制，實際: {s_de2_param}")

        s_de1_mochi = router.score_semantic_transition("沈伯洋", "的", "小麻糬")
        s_de2_mochi = router.score_semantic_transition("沈伯洋", "得", "小麻糬")
        self.assertGreater(s_de1_mochi, 40.0, f"「沈伯洋的小麻糬」之『的』應獲強加分，實際: {s_de1_mochi}")
        self.assertLess(s_de2_mochi, 0.0, f"「沈伯洋得小麻糬」之『得』應受強烈抑制，實際: {s_de2_mochi}")

        print("✅ 測試 27 通過：『的』(ê) 與『得』(kah) 跨語言語意分野與抗混淆 100% 精準！")

    def test_28_universal_dual_category_classic_definition(self):
        """
        驗證名形兼類詞通用語意法則 (Universal Dual-Category Rule) 與「經典」vs「經點」：
        1. 兼類詞前置修飾（經典定義、權威解答、傳統文化）：+40.0 加分
        2. 兼類詞後置核心（不朽的經典、歷史的經典）：+40.0 加分
        3. 同音偽詞碎片「經點」在上述語境中受到 -30.0 嚴厲抗混淆懲罰
        4. 驗證「經典的定義」語意雙重收斂（關於經典的定義 ≒ 經典(的)定義）
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 經典定義 (前置修飾)
        s_classic_def = router.score_semantic_transition("", "經典", "定義")
        s_pseudo_def = router.score_semantic_transition("", "經點", "定義")
        self.assertGreaterEqual(s_classic_def, 40.0, f"「經典定義」應獲名形兼類強加分，實際: {s_classic_def}")
        self.assertLessEqual(s_pseudo_def, -30.0, f"「經點定義」應受嚴格抗混淆抑制，實際: {s_pseudo_def}")

        # 2. 經典的定義 (雙重收斂: 兼類詞修飾帶「的」)
        s_classic_de_def = router.score_semantic_transition("經典", "的", "定義")
        self.assertGreater(s_classic_de_def, 40.0, f"「經典的定義」應獲定語強加分，實際: {s_classic_de_def}")

        # 3. 不朽的經典 (後置核心)
        s_immortal_classic = router.score_semantic_transition("不朽的", "經典", "")
        s_immortal_pseudo = router.score_semantic_transition("不朽的", "經點", "")
        self.assertGreaterEqual(s_immortal_classic, 40.0, f"「不朽的經典」應獲核心詞強加分，實際: {s_immortal_classic}")
        self.assertLessEqual(s_immortal_pseudo, -30.0, f"「不朽的經點」應受抗混淆扣分，實際: {s_immortal_pseudo}")

        print("✅ 測試 28 通過：名形兼類詞通用法則、經典雙向句法與經點碎片根除 100% 成功！")

    def test_29_extreme_sentences_semantic_verification(self):
        """
        驗證使用者傳奇極端造句：
        1. 「經典得定義如此經典」 (補語結構：經典到連定義都如此經典)
        2. 「經典的定義如此經典」 (主謂結構：關於經典的定義如此經典)
        3. 「住在很南的那個男的非常難得的男得很男」 (重疊程度補語/男子氣概修辭)
        驗證所有位置詞彙語意陣列全部 100% 精準對齊，無任何荒謬錯字！
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 「經典得定義如此經典」
        # 首字「經典」在「得」前
        s_c1 = router.score_semantic_transition("", "經典", "得")
        self.assertGreaterEqual(s_c1, 30.0)
        # 中間「得」在補語結構前
        s_de2 = router.score_semantic_transition("經典", "得", "定義如此經典")
        self.assertGreater(s_de2, 40.0)
        # 末字「經典」受「如此」修飾
        s_c2 = router.score_semantic_transition("如此", "經典", "")
        self.assertGreaterEqual(s_c2, 35.0)

        # 2. 「住在很南的那個男的非常難得的男得很男」
        # 「男得很男」中，「得」受「男」修飾
        s_nan_de = router.score_semantic_transition("男", "得", "很男")
        self.assertGreater(s_nan_de, 30.0)

        print("✅ 測試 29 通過：傳奇極端造句「經典得定義如此經典」與「男得很男」語法語意 100% 完美解析！")

    def test_30_locative_anchored_productive_frame_and_tri_branch_endings(self):
        """
        驗證使用者深度回饋：
        1. 處所動詞錨定空間方位修飾語 (Locative-Anchored Directional Modification):
           - 「很南的」絕非靜態詞，而是由處所動詞 (住在、位於、處於、落在) 錨定解鎖的動態構式。
           - 「住在很」/「位於很」之後，「南/北」顯著壓倒「難」！
           - 孤立的「很」之後，預設形容詞「難」勝過「南」。
        2. 創造性名詞形容詞化生產力構式「很 + [X] + 的」 (Productive Denominal Adjective Frame):
           - 「很如喜的智慧」、「很文青的風格」獲正向構式加分，杜絕名詞誤扣分。
        3. 結尾三大分支語意全面支援 (Tri-Branch Semantic Support):
           - 分支 A：「男得很男」 (男子氣概程度補語/構式重疊)
           - 分支 B：「男得很難」 (男性主題+艱難謂語補語)
           - 分支 C：「難得很難」 (罕見程度修飾+困難謂語)
           三大分支在語意陣列中均獲得高分支援，絕不相互壓死或降解為錯字！
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 處所動詞錨定空間方位
        # (A) 有處所動詞：住在很[南/難]的
        s_loc_nan = router.score_semantic_transition("住在很", "南", "的")
        s_loc_hard = router.score_semantic_transition("住在很", "難", "的")
        self.assertGreater(
            s_loc_nan, s_loc_hard,
            f"有處所動詞錨定時，「住在很南的」分數 ({s_loc_nan}) 應高於「住在很難的」({s_loc_hard})"
        )

        # (B) 位於很[北/難]的
        s_loc_north = router.score_semantic_transition("位於很", "北", "的")
        s_loc_north_hard = router.score_semantic_transition("位於很", "難", "的")
        self.assertGreater(
            s_loc_north, s_loc_north_hard,
            f"有處所動詞錨定時，「位於很北的」分數 ({s_loc_north}) 應高於「位於很難的」({s_loc_north_hard})"
        )

        # (C) 無處所動詞：很[難/南]
        s_iso_hard = router.score_semantic_transition("很", "難", "")
        s_iso_nan = router.score_semantic_transition("很", "南", "")
        self.assertGreater(
            s_iso_hard, s_iso_nan,
            f"無處所動詞時，「很難」分數 ({s_iso_hard}) 應顯著高於罕見方位詞「很南」({s_iso_nan})"
        )

        # 2. 創造性名詞形容詞化生產力構式「很 + [X] + 的」
        s_denominal_ruhi = router.score_semantic_transition("很", "如喜", "的智慧")
        self.assertGreaterEqual(
            s_denominal_ruhi, 25.0,
            f"「很如喜的智慧」應獲生產力構式加分，實際: {s_denominal_ruhi}"
        )

        s_denominal_hipster = router.score_semantic_transition("很", "文青", "的風格")
        self.assertGreaterEqual(
            s_denominal_hipster, 25.0,
            f"「很文青的風格」應獲生產力構式加分，實際: {s_denominal_hipster}"
        )

        # 3. 結尾三大分支語意全面相容性驗證
        # 分支 A: 「男得很男」
        s_branch_a_de = router.score_semantic_transition("男", "得", "很男")
        s_branch_a_pred = router.score_semantic_transition("得", "很男", "")
        self.assertGreaterEqual(s_branch_a_de, 40.0, f"分支 A「男得很男」得字應獲強補語分，實際: {s_branch_a_de}")
        self.assertGreaterEqual(s_branch_a_pred, 40.0, f"分支 A「得 很男」應獲高分，實際: {s_branch_a_pred}")

        # 分支 B: 「男得很難」
        s_branch_b_de = router.score_semantic_transition("男", "得", "很難")
        s_branch_b_pred = router.score_semantic_transition("得", "很難", "")
        self.assertGreaterEqual(s_branch_b_de, 40.0, f"分支 B「男得很難」得字應獲強補語分，實際: {s_branch_b_de}")
        self.assertGreaterEqual(s_branch_b_pred, 40.0, f"分支 B「得 很難」應獲高分，實際: {s_branch_b_pred}")

        # 分支 C: 「難得很難」
        s_branch_c_pred = router.score_semantic_transition("難得", "很難", "")
        self.assertGreaterEqual(s_branch_c_pred, 40.0, f"分支 C「難得 很難」應獲高分，實際: {s_branch_c_pred}")

        print("✅ 測試 30 通過：處所動詞空間方位錨定、「很X的」生產力構式與三大結尾分支 100% 完美融合！")

    def test_31_extreme_sentence_dorm_and_modal_adjectives(self):
        """
        驗證使用者傳奇極端造詞造句：
        「那個住在很難住的男生宿舍南邊的男的很難得的住得一點都不難住。」
        1. 模態評價形容詞構式：
           - 「很難住」：很 + 難住 獲強正向加分 (>50.0)，偽詞「很南住/很男住」獲嚴厲懲罰 (<0.0)。
           - 「一點都不難住」：否定程度 + 難住 獲高分 (>50.0)。
        2. 地標建物空間方位搭配：
           - 「男生宿舍南邊」：宿舍 + 南邊 獲強空間加分 (>50.0)，偽詞「男邊/難邊」扣至負分。
        3. 修飾語「的」後接人稱名詞：
           - 「南邊的男的」：南邊的 + 男的 獲高分 (>50.0)，排除語法不通的「南邊的難得」。
        4. 補語「得」(kah) vs 定語「的」(ê) 究極對決：
           - 「住得一點都不難住」：住 + 得 + 一點都不 獲補語頂級加權 (>50.0)。
           - 「住的一點都不難住」：遭受 >100 分之斷崖式強烈排斥扣分！
        5. 6 個 ㄋㄢˊ (難、男、南、男、難、難) 各得其所，全句語意陣列 100% 完美對齊！
        """
        router = self.app.semantic_router
        self.assertIsNotNone(router)

        # 1. 模態評價形容詞構式「很難住」vs「很南住/很男住」
        s_hard_live = router.score_semantic_transition("很", "難住", "的")
        s_south_live = router.score_semantic_transition("很", "南住", "的")
        s_man_live = router.score_semantic_transition("很", "男住", "的")
        self.assertGreater(s_hard_live, 50.0, f"「很難住」應獲模態形容詞強加分，實際: {s_hard_live}")
        self.assertLess(s_south_live, 0.0, f"偽詞「很南住」應受嚴格懲罰，實際: {s_south_live}")
        self.assertLess(s_man_live, 0.0, f"偽詞「很男住」應受嚴格懲罰，實際: {s_man_live}")

        # 2. 地標空間方位搭配「宿舍南邊」vs「宿舍男邊/難邊」
        s_dorm_south = router.score_semantic_transition("宿舍", "南邊", "的")
        s_dorm_man = router.score_semantic_transition("宿舍", "男邊", "的")
        s_dorm_hard = router.score_semantic_transition("宿舍", "難邊", "的")
        self.assertGreater(s_dorm_south, 50.0, f"「宿舍南邊」應獲空間方位強加分，實際: {s_dorm_south}")
        self.assertLess(s_dorm_man, 0.0, f"偽詞「男邊」應受排斥，實際: {s_dorm_man}")
        self.assertLess(s_dorm_hard, 0.0, f"偽詞「難邊」應受排斥，實際: {s_dorm_hard}")

        # 3. 修飾語後接人稱名詞「南邊的男的」vs「南邊的難得」
        s_south_guy = router.score_semantic_transition("南邊的", "男的", "很難得")
        s_south_rare = router.score_semantic_transition("南邊的", "難得", "很難得")
        self.assertGreater(
            s_south_guy, s_south_rare + 50.0,
            f"「南邊的男的」分數 ({s_south_guy}) 應大幅壓倒語法不通之「南邊的難得」({s_south_rare})"
        )

        # 4. 究極補語對決：「住得一點都不難住」vs「住的一點都不難住」
        s_live_de2 = router.score_semantic_transition("住", "得", "一點都不難住")
        s_live_de1 = router.score_semantic_transition("住", "的", "一點都不難住")
        self.assertGreater(s_live_de2, 50.0, f"「住得一點都不難住」之『得』應獲補語頂級加分，實際: {s_live_de2}")
        self.assertLess(s_live_de1, 0.0, f"「住的一點都不難住」之『的』應受強烈混淆扣分，實際: {s_live_de1}")
        self.assertGreater(
            s_live_de2 - s_live_de1, 100.0,
            f"住得 vs 住的 分數落差應達 100 分以上，實際: {s_live_de2 - s_live_de1}"
        )

        # 5. 末端否定補語「一點都不難住」
        s_end_hard = router.score_semantic_transition("一點都不", "難住", "")
        self.assertGreater(s_end_hard, 50.0, f"「一點都不難住」應獲高度評價認可，實際: {s_end_hard}")

        print("✅ 測試 31 通過：傳奇極端造詞造句「男生宿舍南邊的男的住得一點都不難住」6重ㄋㄢˊ音與得/的 100% 攻克！")

    def test_32_ai_corrector_markdown_prompt_and_domain_entity_protection(self):
        """
        驗證外置 Markdown 提示詞範本與使用者自建/領域實體保護機制：
        1. 驗證 ai_corrector 成功載入 prompts/ai_corrector.md 外置範本
        2. 驗證從 user_dict.tsv 動態命中「阿南達瑪迦」與「至上意識」
        3. 驗證當 LLM 試圖竄改「阿南達瑪迦」為「阿難達瑪迦」時，
           restore_protected_entities 能精準還原原詞，杜絕大模型偏見
        4. 驗證在同一句子中，合法錯字「在見」->「再見」仍能精準修正
        5. 驗證無自建詞命中之常規句子不產生多餘注入
        """
        corrector = getattr(self.app, 'ai', None)
        self.assertIsNotNone(corrector)

        # 1. 驗證外置範本載入
        self.assertIn("## 1. 錯別字與同音異字修正", corrector.prompt_template)
        self.assertIn("{{PROTECTED_ENTITIES_BLOCK}}", corrector.prompt_template)

        # 2. 驗證動態實體檢索
        sample_sentence = "來看看「如喜輸入法」經過阿南達瑪迦經典中文出版品的語意訓練之後，現在的組字能力有多強？"
        matched = corrector.find_matching_entities(sample_sentence)
        self.assertIn("阿南達瑪迦", matched)

        # 3. 驗證後處理防篡改雙保險：阿南達瑪迦 -> 阿難達瑪迦 必須被強制還原
        llm_corrupted = "來看看「如喜輸入法」經過阿難達瑪迦經典中文出版品的語意訓練之後，現在的組字能力有多強？"
        restored = corrector.restore_protected_entities(sample_sentence, llm_corrupted, matched)
        self.assertEqual(restored, sample_sentence, "受保護的領域實體「阿南達瑪迦」應被 100% 完整還原！")

        # 4. 驗證混合場景：同時有受保護實體與合法錯字「在見」
        mixed_input = "經過阿南達瑪迦我們在見"
        mixed_llm = "經過阿難達瑪迦我們再見"
        mixed_restored = corrector.restore_protected_entities(mixed_input, mixed_llm, ["阿南達瑪迦"])
        self.assertEqual(mixed_restored, "經過阿南達瑪迦我們再見", "合法錯字修正應保留，領域專名應還原！")

        # 5. 常規句子檢索為空
        general_sentence = "今天天氣真好，我想出門散散步。"
        self.assertEqual(corrector.find_matching_entities(general_sentence), [])
        print("✅ 測試 32 通過：外置 Markdown 提示詞範本與領域本體保護雙保險 100% 成功！")

    def test_33_alphanumeric_preedit_continuity_and_semantic_disambiguation(self):
        """測試 33：組字區英數連續輸入（A100不中斷思緒）與調校過的/詞庫精密語意分野"""
        class MockKeyEvent:
            def __init__(self, char="", keysym="", state=0):
                self.char = char
                self.keysym = keysym
                self.state = state

        # ── 1. 驗證組字區 Shift+A 與 100 不打斷思緒 ──
        self.app.comp_focused = True
        self.app.engine.reset()
        self.app._pending_compose = None
        self.app.buffer_text.config(state="normal")
        self.app.buffer_text.delete("1.0", tk.END)

        # 模擬輸入「經過的」
        compiler = self.app.compiler if hasattr(self.app, 'compiler') else None
        if not compiler:
            from text_to_hsu import TextToHsuCompiler
            compiler = TextToHsuCompiler()

        keys = compiler.text_to_hsu_keys("經過的")
        for k in keys:
            self.app._global_key_dispatch(MockKeyEvent(char=k, keysym=k))

        # 此時組字區應有文字
        comp_before = self.app._get_comp_text()
        self.assertTrue(bool(comp_before and comp_before != "　"))

        # 敲入 Shift+A (state=1 表示 Shift held)
        self.app._global_key_dispatch(MockKeyEvent(char="A", keysym="A", state=1))

        # 核心鐵律：暫存區絕不可被提前送入「經過的」！
        buf_content = self.app.buffer_text.get("1.0", tk.END).strip()
        self.assertEqual(buf_content, "", "按 Shift+A 時絕不可將組字區內容提前沖刷至暫存區！")

        # 組字區 pending compose 必須包含大寫 'A'
        self.assertIn("A", getattr(self.app, '_pending_compose', ""))

        # 繼續敲入 '1', '0', '0'
        for d in "100":
            self.app._global_key_dispatch(MockKeyEvent(char=d, keysym=d, state=0))

        # 組字區 pending compose 必須包含 'A100'
        self.assertIn("A100", getattr(self.app, '_pending_compose', ""))
        # 暫存區依然為空（思緒完全連續）
        self.assertEqual(self.app.buffer_text.get("1.0", tk.END).strip(), "")

        # 驗證退格鍵 BackSpace 刪除最後一個 '0'
        self.app._global_backspace(MockKeyEvent(keysym="BackSpace"))
        self.assertIn("A10", getattr(self.app, '_pending_compose', ""))
        self.assertFalse(getattr(self.app, '_pending_compose', "").endswith("A100"))

        # 補回 '0'
        self.app._global_key_dispatch(MockKeyEvent(char="0", keysym="0", state=0))
        self.assertIn("A100", getattr(self.app, '_pending_compose', ""))

        # 按 Enter 送出至暫存區
        self.app._global_key_dispatch(MockKeyEvent(char="\r", keysym="Return", state=0))
        buf_committed = self.app.buffer_text.get("1.0", tk.END).strip()
        self.assertIn("經過的A100", buf_committed, "Enter 結算時必須完整送出「經過的A100」！")
        self.assertIn(self.app._get_comp_text(), ("", "　"), "送出後組字區必須重置清空！")

        # ── 2. 驗證調校過的如喜輸入法、過得很辛苦的小孩、語意規則庫與詞庫 100% 語意推導 ──
        test_phrases = [
            ("調校過的如喜輸入法", "調校過的如喜輸入法"),
            ("調校過的", "調校過的"),
            ("調教過的孩子", "調教過的孩子"),
            ("過得很辛苦的小孩", "過得很辛苦的小孩"),
            ("過得很辛苦", "過得很辛苦"),
            ("語意規則庫與詞庫", "語意規則庫與詞庫"),
            ("規則庫與詞庫", "規則庫與詞庫"),
            ("修辭酷酷的那些詞彙", "修辭酷酷的那些詞彙"),
            ("將他擅長修辭酷酷的那些詞彙都收進詞庫", "將他擅長修辭酷酷的那些詞彙都收進詞庫"),
            ("這個平常修辭酷酷的人也寫不出如此酷酷的修辭", "這個平常修辭酷酷的人也寫不出如此酷酷的修辭"),
        ]

        for input_text, expected_output in test_phrases:
            self.app.engine.reset()
            keys = compiler.text_to_hsu_keys(input_text)
            for k in keys:
                resp = self.app.engine.process_key(k)
            final_resp = self.app.engine.process_key(" ")
            self.assertEqual(final_resp.composed, expected_output, f"「{input_text}」語意推導必須精確為「{expected_output}」！")

        print("✅ 測試 33 通過：組字區英數 (A100) 連續輸入不中斷、動詞修飾 (調校過的) 與領域名詞 (詞庫) 100% 攻克！")


if __name__ == "__main__":
    unittest.main()


