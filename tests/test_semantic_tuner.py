# -*- coding: utf-8 -*-
"""
如喜輸入法 - 語意調校工作室雙向詞網感知與安全防護單元測試
(Lexicon-Aware Bidirectional Alignment & Typo Guardrail Tests)
"""

import unittest
import sys
from pathlib import Path

# 加入根目錄至 sys.path
root_dir = Path(__file__).resolve().parent.parent
if str(root_dir) not in sys.path:
    sys.path.insert(0, str(root_dir))

from scripts.semantic_tuner import SemanticTuner

class TestSemanticTunerLexiconAwareness(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tuner = SemanticTuner(root_dir)

    def test_lexicon_loaded(self):
        """驗證詞庫已成功載入且涵蓋基本核心標準詞"""
        self.assertGreater(len(self.tuner.lexicon_words), 100000)
        self.assertIn("模式", self.tuner.lexicon_words)
        self.assertIn("其實", self.tuner.lexicon_words)
        self.assertIn("選擇", self.tuner.lexicon_words)
        self.assertIn("新注音", self.tuner.lexicon_words)
        self.assertIn("不懂", self.tuner.lexicon_words)

    def test_moshilingshi_no_pseudo_words(self):
        """
        核心測試：驗證「模噬臍時 ➔ 模式其實」
        1. 徹底消滅「音模」、「式其實」等非詞怪胎
        2. 正確提取「新注音 ➔ 模式」、「模式 ➔ 其實」、「其實 ➔ 才是」
        3. 正確抑制「模+噬」與「臍+時」
        """
        err = "或許微軟新注音模噬臍時才是相容性最好的選擇"
        cor = "或許微軟新注音模式其實才是相容性最好的選擇"

        diffs = self.tuner.align_sentence_pair(err, cor)
        self.assertEqual(len(diffs), 1)

        d = diffs[0]
        self.assertEqual(d["rejected"], "模噬臍時")
        self.assertEqual(d["accepted"], "模式其實")
        self.assertEqual(d["accepted_tokens"], ["模式", "其實"])
        self.assertEqual(d["prev_ctx"], "新注音")
        self.assertEqual(d["next_ctx"], "才是")

        rules = self.tuner.synthesize_rules(d)
        prev_next_pairs = [(r["prev"], r["next"]) for r in rules]

        # 斷言：合法連鎖引力必須存在
        self.assertIn(("模式", "其實"), prev_next_pairs)
        self.assertIn(("新注音", "模式"), prev_next_pairs)
        self.assertIn(("其實", "才是"), prev_next_pairs)

        # 斷言：怪胎非詞絕對不能存在於任何規則中
        for r in rules:
            self.assertNotEqual(r["prev"], "音模")
            self.assertNotEqual(r["next"], "式其實")
            self.assertNotEqual(r["prev"], "式其實")

    def test_typo_guardrail_xuanze_protection(self):
        """
        核心測試：手滑筆誤防禦
        當使用者在正確句手滑打成「選則」（非字典詞），而原始句為標準詞「選擇」：
        1. 系統自動識別 is_typo = True
        2. 給出清楚的安全警告
        3. 絕對不對常用合法詞「選擇」生成負向抑制規則！
        """
        err = "或許微軟新注音模噬臍時才是相容性最好的選擇"
        cor = "或許微軟新注音模式其實才是相容性最好的選則"

        diffs = self.tuner.align_sentence_pair(err, cor)
        self.assertEqual(len(diffs), 2)

        # 檢查第二個 diff（選擇 ➔ 選則）
        d_xuanze = diffs[1]
        self.assertEqual(d_xuanze["rejected"], "選擇")
        self.assertEqual(d_xuanze["accepted"], "選則")
        self.assertTrue(d_xuanze["is_typo"])
        self.assertIn("安全保護", d_xuanze["warning"])

        rules = self.tuner.synthesize_rules(d_xuanze)
        # 斷言：針對筆誤不生成任何抑制常用詞「選擇」的規則
        self.assertEqual(len(rules), 0)

    def test_causative_budong(self):
        """驗證「讓步董 ➔ 讓不懂」使役構式差分"""
        err = "讓步董程式開發的使用者們"
        cor = "讓不懂程式開發的使用者們"

        diffs = self.tuner.align_sentence_pair(err, cor)
        self.assertEqual(len(diffs), 1)

        d = diffs[0]
        self.assertEqual(d["rejected"], "讓步董")
        self.assertEqual(d["accepted"], "讓不懂")
        self.assertEqual(d["accepted_tokens"], ["讓", "不懂"])

        rules = self.tuner.synthesize_rules(d)
        prev_next_pairs = [(r["prev"], r["next"]) for r in rules]

        self.assertIn(("讓", "不懂"), prev_next_pairs)
        self.assertIn(("不懂", "程式"), prev_next_pairs)
        # 斷言：恪守二階正向引力（二階語意防禦機制），絕對嚴禁對單字組合「步＋董」施加過度抑制（守護「走步董事」等合法詞界搭配）
        self.assertNotIn(("步", "董"), prev_next_pairs)
        for r in rules:
            self.assertGreater(r["score"], 0)

    def test_wanbi_caizhengshi(self):
        """驗證多詞連鎖「完比財政是發布 ➔ 完畢才正式發布」"""
        err = "完比財政是發布"
        cor = "完畢才正式發布"

        diffs = self.tuner.align_sentence_pair(err, cor)
        self.assertEqual(len(diffs), 1)

    def test_overlapping_ambiguity_guozai(self):
        """
        核心測試：驗證「不過在」被「過載」交錯劫掠 (Overlapping Ambiguity)
        1. 正確識別出「過載」橫向搶奪「不過」
        2. 正確合成「不過 ➔ 在 (+26.0)」
        3. 正確合成抑制規則「不 ➔ 過載 (-28.0)」斬斷劫掠
        """
        err = "如喜固定輸入座最大的好處是可以固定輸入行的文字大小，不過載標點符號之前如果太長，就會有卡頓現象。"
        cor = "如喜固定輸入座最大的好處是可以固定輸入行的文字大小，不過在標點符號之前如果太長，就會有卡頓現象。"

        diffs = self.tuner.align_sentence_pair(err, cor)
        self.assertEqual(len(diffs), 1)

        d = diffs[0]
        self.assertEqual(d["rejected"], "載")
        self.assertEqual(d["accepted"], "在")
        self.assertEqual(d["overlapping_hijack"], "過載")
        self.assertEqual(d["isolated_prefix"], "不")

        rules = self.tuner.synthesize_rules(d)
        prev_next_pairs = [(r["prev"], r["next"]) for r in rules]

        # 斷言：正向引力必須存在
        self.assertIn(("不過", "在"), prev_next_pairs)
        self.assertIn(("在", "標點符號"), prev_next_pairs)

        # 斷言：絕不對合法用語「不 ➔ 過載」施加負向抑制，守護「用電安全守則：不過載」
        self.assertNotIn(("不", "過載"), prev_next_pairs)

        meta = self.tuner.analyze_linguistic_context(d["rejected"], d["accepted"], d["prev_ctx"], d["next_ctx"], diff_item=d)
        self.assertIn("交錯型分詞歧義", meta["construction"])
        self.assertIn("過載", meta["safety_note"])
        self.assertIn("用電安全：不過載", meta["safety_note"])

    def test_extreme_sentence_no_negative_mines(self):
        """
        經典反例測試：
        「這時吧檯上跳著艷舞的超模朝著另一邊的走步董事滑了過去。」
        中文無詞界連寫（Scriptio Continua）鐵證：
        1. 「這時」+「吧檯」天然包含「時＋吧」
        2. 「走步」+「董事」天然包含「步＋董」
        驗證：全系統絕對嚴禁產生任何「時 ➔ 吧」或「步 ➔ 董」的過度抑制邊界錯誤 (Boundary Over-Suppression)！
        所有生成的規則分數必須全為正數（二階正向轉移引力場）。
        """
        # 情景一：使用者將「十八個」誤打成「時吧個」
        err1 = "這句野生哲學家所寫的時吧個自我介紹"
        cor1 = "這句野生哲學家所寫的十八個自我介紹"
        diffs1 = self.tuner.align_sentence_pair(err1, cor1)
        rules1 = []
        for d in diffs1:
            rules1.extend(self.tuner.synthesize_rules(d))
        pairs1 = [(r["prev"], r["next"]) for r in rules1]
        self.assertNotIn(("時", "吧"), pairs1)
        self.assertNotIn(("這句", "野"), pairs1)
        # 斷言：絕無任何負向抑制規則（負數分數）
        for r in rules1:
            self.assertGreater(r["score"], 0)
        # 即使十八個 ➔ 自存在，也必須是正向引力（+24.0），絕非負向抑制
        for r in rules1:
            if (r["prev"], r["next"]) == ("十八個", "自"):
                self.assertGreater(r["score"], 0)

        # 情景二：使用者將「不懂」誤打成「步董」
        err2 = "其實他自己也步董"
        cor2 = "其實他自己也不懂"
        diffs2 = self.tuner.align_sentence_pair(err2, cor2)
        rules2 = []
        for d in diffs2:
            rules2.extend(self.tuner.synthesize_rules(d))
        pairs2 = [(r["prev"], r["next"]) for r in rules2]
        self.assertNotIn(("步", "董"), pairs2)
        for r in rules2:
            self.assertGreater(r["score"], 0)

    def test_ai_wizard_and_sublimation(self):
        """驗證 AI 策略引導精靈與修訂日誌 AI 昇華（洗滌打地鼠污染）"""
        from scripts.ai_semantic_consultant import AISemanticConsultant
        import tempfile, shutil

        test_dir = Path(tempfile.mkdtemp())
        test_cfg = test_dir / "config.json"
        test_cfg.write_text("{}", encoding="utf-8")
        try:
            consultant = AISemanticConsultant(test_dir)

            # 1. 測試精靈狀態與策略儲存
            status = consultant.get_ai_wizard_status()
            self.assertIn("wizard_completed", status)
            self.assertIn("ai_strategy", status)

            # 模擬設定 remote_first
            saved = consultant.save_ai_wizard_strategy("remote_first", gemini_model="gemini-2.0-flash")
            self.assertTrue(saved)
            self.assertTrue(consultant.config.get("ai_wizard_completed"))
            self.assertEqual(consultant.config.get("ai_strategy"), "remote_first")

            # 2. 測試修訂日誌 AI 昇華
            sample_records = [
                {
                    "timestamp": 1700000000,
                    "rejected": "的數",
                    "accepted": "地鼠",
                    "prev_context": "打",
                    "next_context": "遊戲",
                    "bopomofo": "ㄉㄧˋ ㄕㄨˇ"
                }
            ]
            sub_res = consultant.sublimate_revisions(sample_records)
            self.assertEqual(sub_res["sublimated_records_count"], 1)
            self.assertGreaterEqual(len(sub_res["sublimated_words"]), 1)
            # 斷言：昇華出來的長詞為「地鼠」或複合詞，且無負向抑制
            for r in sub_res["sublimated_rules"]:
                self.assertGreater(r["score"], 0)

            # 3. 測試詞庫注入長詞凝聚力防護（使用已存在之合法詞「打地鼠」驗證去重與載入，不污染正式詞庫）
            words_added = self.tuner.append_words_to_domain_lexicon([{"word": "打地鼠", "weight": 1600}])
            self.assertIn("打地鼠", self.tuner.lexicon_words)

            # 4. 測試本地備案推薦與免打擾偏好開關
            recs = consultant.check_local_model_recommendations()
            self.assertIn("recommended_models", recs)
            self.assertEqual(recs["recommended_models"][0]["name"], "gemma2:latest")
            self.assertIn("Google 開源生態", recs["recommended_models"][0]["ecosystem"])
            self.assertEqual(recs["recommended_models"][2]["name"], "llama3.2:latest")
            self.assertIn("純綠開源生態", recs["recommended_models"][2]["ecosystem"])

            # 測試設定「不再提醒」
            ok = consultant.set_local_model_prompt_preference(True)
            self.assertTrue(ok)
            self.assertTrue(consultant.config.get("skip_local_model_upgrade_prompt"))

            # 測試反悔恢復
            ok2 = consultant.set_local_model_prompt_preference(False)
            self.assertTrue(ok2)
            self.assertFalse(consultant.config.get("skip_local_model_upgrade_prompt"))
        finally:
            shutil.rmtree(test_dir, ignore_errors=True)

    def test_custom_subscribed_ai_providers(self):
        """驗證使用者自備/自訂訂閱 AI 提供者 (OpenAI, Claude, Custom) 之設定與引導精靈保存"""
        from scripts.ai_semantic_consultant import AISemanticConsultant
        import tempfile, shutil

        test_dir = Path(tempfile.mkdtemp())
        test_cfg = test_dir / "config.json"
        test_cfg.write_text("{}", encoding="utf-8")
        try:
            consultant = AISemanticConsultant(test_dir)

            # 1. 測試儲存 OpenAI 自備 API Key
            consultant.save_ai_settings(
                provider="openai",
                openai_api_key="sk-proj-testkey123456",
                openai_model="gpt-4o-mini"
            )
            self.assertEqual(consultant.get_effective_provider(), "openai")
            self.assertEqual(consultant.config.get("openai_model"), "gpt-4o-mini")

            # 2. 測試儲存 Claude 自備 API Key
            consultant.save_ai_settings(
                provider="claude",
                claude_api_key="sk-ant-testkey654321",
                claude_model="claude-3-5-sonnet-20241022"
            )
            self.assertEqual(consultant.get_effective_provider(), "claude")
            self.assertEqual(consultant.config.get("claude_model"), "claude-3-5-sonnet-20241022")

            # 3. 測試儲存自訂相容端點
            consultant.save_ai_settings(
                provider="custom",
                custom_base_url="https://openrouter.ai/api/v1",
                custom_api_key="sk-or-testkey",
                custom_model="google/gemma-2-9b-it"
            )
            self.assertEqual(consultant.get_effective_provider(), "custom")
            self.assertEqual(consultant.config.get("custom_base_url"), "https://openrouter.ai/api/v1")
            self.assertEqual(consultant.config.get("custom_model"), "google/gemma-2-9b-it")

            # 4. 測試引導精靈方案一保存自備 OpenAI
            ok = consultant.save_ai_wizard_strategy(
                strategy="remote_first",
                remote_provider="openai",
                openai_api_key="sk-proj-wizardkey",
                openai_model="gpt-4o"
            )
            self.assertEqual(consultant.config.get("openai_api_key"), "sk-proj-wizardkey")
            self.assertEqual(consultant.config.get("openai_model"), "gpt-4o")
        finally:
            shutil.rmtree(test_dir, ignore_errors=True)

    def test_parse_and_load_text_file(self):
        """驗證文字檔解析：支援有無空行、上下成對、注釋行、以及箭頭分隔符"""
        sample_text_paired = """
        # 這是一段測試對照檔
        這句事我寫給你看的，不要寫在介面上。
        這句是我寫給你看的，不要寫在介面上。

        直接貼上輸入法組錯的句子
        直接貼上輸入法組織錯誤的句子


        快速填入測試案例按了都沒有結果。
        快速填入測試案例按了都沒有反應。
        """
        pairs = self.tuner.parse_text_to_pairs(sample_text_paired)
        self.assertEqual(len(pairs), 3)
        self.assertEqual(pairs[0]["error"], "這句事我寫給你看的，不要寫在介面上。")
        self.assertEqual(pairs[0]["correct"], "這句是我寫給你看的，不要寫在介面上。")
        self.assertEqual(pairs[1]["error"], "直接貼上輸入法組錯的句子")
        self.assertEqual(pairs[1]["correct"], "直接貼上輸入法組織錯誤的句子")
        self.assertEqual(pairs[2]["error"], "快速填入測試案例按了都沒有結果。")
        self.assertEqual(pairs[2]["correct"], "快速填入測試案例按了都沒有反應。")

        # 驗證單行分隔符
        sample_text_arrow = """
        模噬臍時 ➔ 模式其實
        讓步董 ➔ 讓不懂
        """
        arrow_pairs = self.tuner.parse_text_to_pairs(sample_text_arrow)
        self.assertEqual(len(arrow_pairs), 2)
        self.assertEqual(arrow_pairs[0]["error"], "模噬臍時")
        self.assertEqual(arrow_pairs[0]["correct"], "模式其實")
        self.assertEqual(arrow_pairs[1]["error"], "讓步董")
        self.assertEqual(arrow_pairs[1]["correct"], "讓不懂")

        # 驗證使用者桌面真實檔案 (若存在)
        desktop_file = Path("C:/Users/Yuktesha/Desktop/語意分析.txt")
        if desktop_file.exists():
            desktop_pairs = self.tuner.load_pairs_from_file(desktop_file)
            self.assertGreaterEqual(len(desktop_pairs), 1)
            self.assertTrue(len(desktop_pairs[0]["error"]) > 0)
            self.assertTrue(len(desktop_pairs[0]["correct"]) > 0)

    def test_align_with_custom_tokens(self):
        """驗證使用者手動切分/合併詞界後的對齊演算正確性"""
        err = "目前看起來都再打的數。"
        cor = "目前看起來都在打地鼠。"
        # 情境一：細粒度詞界（[在] + [打地鼠]）
        custom_err = ["目前", "看起來", "都", "再", "打", "的", "數", "。"]
        custom_cor = ["目前", "看起來", "都", "在", "打地鼠", "。"]

        diffs = self.tuner.align_sentence_pair(
            err, cor,
            custom_err_tokens=custom_err,
            custom_cor_tokens=custom_cor
        )
        self.assertEqual(len(diffs), 2)
        # 第一個差分修復介詞選字：再 ➔ 在
        self.assertEqual(diffs[0]["rejected"], "再")
        self.assertEqual(diffs[0]["accepted"], "在")
        self.assertEqual(diffs[0]["prev_ctx"], "都")
        self.assertEqual(diffs[0]["next_ctx"], "打地鼠")
        # 第二個差分修復複合長詞：打的數 ➔ 打地鼠
        self.assertEqual(diffs[1]["rejected"], "打的數")
        self.assertEqual(diffs[1]["accepted"], "打地鼠")
        self.assertEqual(diffs[1]["prev_ctx"], "在")
        # 標點符號不作為二階轉移上下文詞，守護轉移矩陣純淨度
        self.assertEqual(diffs[1]["next_ctx"], "")

        # 情境二：合併長詞詞界（[在打地鼠]）
        merged_err = ["目前", "看起來", "都", "再打的數", "。"]
        merged_cor = ["目前", "看起來", "都", "在打地鼠", "。"]
        diffs_merged = self.tuner.align_sentence_pair(
            err, cor,
            custom_err_tokens=merged_err,
            custom_cor_tokens=merged_cor
        )
        self.assertEqual(len(diffs_merged), 1)
        self.assertEqual(diffs_merged[0]["rejected"], "再打的數")
        self.assertEqual(diffs_merged[0]["accepted"], "在打地鼠")
        self.assertEqual(diffs_merged[0]["prev_ctx"], "都")
        self.assertEqual(diffs_merged[0]["next_ctx"], "")

    def test_no_digit_or_crude_fragment_rules(self):
        """
        核心測試：數詞與曆法日期／貨幣／度量衡／時間單位構式 (Numeric-Unit Classifier Construction)
        1. 驗證「待10越15日 ➔ 待10月15日」與「2026年10越15日 ➔ 2026年10月15日」絕不跨越數字產生「待 ➔ 月」、「年 ➔ 月」、「月 ➔ 日」等偽規則
        2. 驗證自動觸發 Layer 1 `<NUM> ➔ <UNIT_CLASSIFIER>` 專用數詞單位構式
        3. 驗證貨幣（100元）、面積（50坪）、溫度（37度）、重量（500克）、時間（30秒）一體適用
        """
        for err, cor, exp_num, exp_unit in [
            ("待10越15日", "待10月15日", "10", "月"),
            ("2026年10越15日", "2026年10月15日", "10", "月"),
            ("售價100圓", "售價100元", "100", "元"),
            ("室內50瓶", "室內50坪", "50", "坪"),
            ("體溫37渡", "體溫37度", "37", "度"),
            ("重量500課", "重量500克", "500", "克"),
            ("耗時30淼", "耗時30秒", "30", "秒"),
        ]:
            diffs = self.tuner.align_sentence_pair(err, cor)
            self.assertEqual(len(diffs), 1, f"Failed for {err} -> {cor}")
            d = diffs[0]
            self.assertEqual(d["accepted"], exp_unit)
            self.assertEqual(d["prev_num"], exp_num)
            # 嚴禁跨越數字去抓前面的「待」、「年」、「售價」或後面的「日」
            self.assertEqual(d["prev_ctx"], "")
            self.assertEqual(d["next_ctx"], "")

            # 驗證已觸發專用數詞單位構式，且不產生任何字面二元轉移規則（杜絕 待➔月、月➔日）
            constructions = d.get("hierarchical_constructions", [])
            self.assertTrue(
                any("數詞與度量衡" in c.get("name", "") for c in constructions),
                f"Expected Numeric-Unit Classifier Construction for {err} -> {cor}"
            )
            rules = self.tuner.synthesize_rules(d)
            self.assertEqual(rules, [], f"Expected zero literal CJK rules across digits for {err} -> {cor}, got {rules}")

    def test_canonical_spec_and_neuro_symbolic_taxonomy(self):
        """
        核心測試：驗證權威漢語結構語意規範文件 (RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md)、
        注音讀音同音/異調判別、未登錄複合長詞自動凝聚 (Type I) 與六大錯誤分類法 (Type I ~ Type VI)
        """
        from scripts.ai_semantic_consultant import AISemanticConsultant

        consultant = AISemanticConsultant(root_dir)
        # 1. 驗證權威 Markdown 規範文件已載入並注入 System Prompt
        self.assertTrue(len(consultant.spec_content) > 1000)
        self.assertIn("Type I", consultant.spec_content)
        self.assertIn("Type VI", consultant.spec_content)
        sys_inst = consultant.get_canonical_system_instruction(is_batch=True)
        self.assertIn("OOV_COMPOUND_SHATTERED", sys_inst)
        self.assertIn("OVERLAPPING_BRIDGE_HIJACK", sys_inst)

        # 2. 驗證注音讀音與語音關係分類
        self.assertEqual(self.tuner.classify_phonetic_relation("模噬", "模式"), "exact_homophone")
        self.assertEqual(self.tuner.classify_phonetic_relation("再", "在"), "exact_homophone")

        # 3. 驗證 Type I (OOV 複合長詞凝聚)：過去 synthesize_rules 會因「打地鼠」不在舊詞庫而靜默丟棄規則
        err = "目前看起來都再打的數。"
        cor = "目前看起來都在打地鼠。"
        custom_err = ["目前", "看起來", "都", "再", "打", "的", "數", "。"]
        custom_cor = ["目前", "看起來", "都", "在", "打地鼠", "。"]
        diffs = self.tuner.align_sentence_pair(err, cor, custom_err_tokens=custom_err, custom_cor_tokens=custom_cor)
        self.assertEqual(len(diffs), 2)
        self.assertIn("打地鼠", diffs[1]["oov_compound_words"] + (["打地鼠"] if "打地鼠" in self.tuner.lexicon_words else []))

        rules_d0 = self.tuner.synthesize_rules(diffs[0])
        rules_d1 = self.tuner.synthesize_rules(diffs[1])
        pairs_d0 = [(r["prev"], r["next"]) for r in rules_d0]
        pairs_d1 = [(r["prev"], r["next"]) for r in rules_d1]
        self.assertIn(("都", "在"), pairs_d0)
        self.assertIn(("在", "打地鼠"), pairs_d0)
        self.assertIn(("在", "打地鼠"), pairs_d1)

        # 4. 驗證符號計算語言學診斷引擎 (_diagnose_symbolically) 不依賴硬編碼字串，能泛化診斷任意新句子
        sym_diag = consultant._diagnose_symbolically(
            "這套演算法能避免權重失衡覆蓋的問題",
            "這套演算法能避免權重失衡覆蓋的問題",
            symbolic_diffs=[]
        )
        self.assertIn("error_taxonomy", sym_diag["syntactic_analysis"])

        sym_diag2 = consultant._diagnose_symbolically(
            "硬碟理面有許多重要檔案",
            "硬碟裡面有許多重要檔案",
            symbolic_diffs=self.tuner.align_sentence_pair("硬碟理面有許多重要檔案", "硬碟裡面有許多重要檔案")
        )
        self.assertFalse(sym_diag2.get("is_user_typo", False))
        self.assertGreaterEqual(len(sym_diag2["recommended_rules"]), 1)
        for r in sym_diag2["recommended_rules"]:
            self.assertGreater(r["score"], 0)

    def test_dag_dp_tokenization_and_pairwise_alignment(self):
        """
        核心測試：驗證有向無環圖動態規劃 (DAG DP) 分詞、句法邊界防護與雙向對照詞界對齊
        徹底根絕「我步 | 懂」、「我不 | 懂」、「請一 | 句您 | 的專 | 業」、「分階 | 段給 | 我成 | 果」等離譜切分！
        """
        err = "工程專業我步懂，請一句您的專業，分階段給我成果，謝謝。"
        cor = "工程專業我不懂，請依據您的專業，分階段給我成果，謝謝。"

        err_spans, cor_spans = self.tuner.tokenize_pair_with_spans(err, cor)
        err_tokens = [t for t, s, e in err_spans]
        cor_tokens = [t for t, s, e in cor_spans]

        expected_cor = ["工程", "專業", "我", "不懂", "，", "請", "依據", "您", "的", "專業", "，", "分階段", "給", "我", "成果", "，", "謝謝", "。"]
        expected_err = ["工程", "專業", "我", "步懂", "，", "請", "一句", "您", "的", "專業", "，", "分階段", "給", "我", "成果", "，", "謝謝", "。"]

        self.assertEqual(cor_tokens, expected_cor)
        self.assertEqual(err_tokens, expected_err)

        # 斷言：任何機械兩字切塊與代詞黏連偽詞絕不出現
        for bad_tok in ["我步", "我不", "請一", "句您", "的專", "分階", "段給", "我成", "給我"]:
            self.assertNotIn(bad_tok, err_tokens)
            self.assertNotIn(bad_tok, cor_tokens)

        # 驗證對齊與二階語意規則提煉
        diffs = self.tuner.align_sentence_pair(err, cor, custom_err_tokens=err_tokens, custom_cor_tokens=cor_tokens)
        self.assertEqual(len(diffs), 2)
        self.assertEqual(diffs[0]["rejected"], "步懂")
        self.assertEqual(diffs[0]["accepted"], "不懂")
        self.assertEqual(diffs[0]["prev_ctx"], "我")
        self.assertEqual(diffs[1]["rejected"], "一句")
        self.assertEqual(diffs[1]["accepted"], "依據")
        self.assertEqual(diffs[1]["prev_ctx"], "請")
        self.assertEqual(diffs[1]["next_ctx"], "您")

    def test_sublimate_revisions_spec_and_word_boundary_alignment(self):
        """
        核心測試：驗證「🤖 啟動 AI 深度昇華（洗滌打地鼠暫存）」(`sublimate_revisions`)
        1. 絕不採用 3.8 Flash 偷懶的 prev_c[-2:] / next_c[:2] 機械切字（例如「工程專業我」不可切成「業我」）
        2. 依據 DAG DP 精確詞界與 RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md 提煉合法詞與二階正向轉移
        """
        from scripts.ai_semantic_consultant import AISemanticConsultant

        consultant = AISemanticConsultant(root_dir)
        consultant.config["ai_provider"] = "disabled"
        sample_records = [
            {
                "timestamp": 1700000001,
                "rejected": "步董",
                "accepted": "不懂",
                "prev_context": "工程專業我",
                "next_context": "，請依據您的專業",
                "bopomofo": self.tuner.get_bopomofo("不懂"),
                "symbolic_diffs": self.tuner.align_sentence_pair(
                    "工程專業我步董，請依據您的專業",
                    "工程專業我不懂，請依據您的專業"
                )
            }
        ]
        res = consultant.sublimate_revisions(sample_records)
        self.assertEqual(res["sublimated_records_count"], 1)
        self.assertIn("RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md", res["explanation"])

        rule_pairs = [(r["prev"], r["next"]) for r in res["sublimated_rules"]]
        # 斷言：正確提取「我 ➔ 不懂」，絕不出現機械切片產生的「業我 ➔ 不懂」或標點碎片
        self.assertIn(("我", "不懂"), rule_pairs)
        self.assertNotIn(("業我", "不懂"), rule_pairs)
        for r in res["sublimated_rules"]:
            self.assertTrue(self.tuner.is_valid_chinese_token(r["prev"]))
            self.assertTrue(self.tuner.is_valid_chinese_token(r["next"]))
            self.assertGreater(r["score"], 0)

        # 2. 驗證「，每 + [典籍 ➔ 點擊] + 一次」與「[經驗 ➔ 驚艷]」及「[匡 ➔ 框] + 選」：
        #    (a) 絕不可將指代/定語構式「每點擊」捏造為複合詞！
        #    (b) 絕不可將基礎詞典已存在之同音二字詞（點擊、驚艷、全選、依據、程式、形成）重複塞入 sublimated_words 灌 +1450 權重！
        #    (c) 真正的新增構詞（框選、圈選框）應正確收錄且具備完整注音音節！
        #    (d) 正確提煉二階正向轉移「每 ➔ 點擊」與「點擊 ➔ 一次」！
        records_batch = [
            {
                "timestamp": 1791813252,
                "bopomofo": "ㄉㄧㄢˇ",
                "rejected": "典籍",
                "accepted": "點擊",
                "prev_context": "，每",
                "next_context": "一次",
            },
            {
                "timestamp": 1791811618,
                "bopomofo": "ㄐㄧㄥ",
                "rejected": "經驗",
                "accepted": "驚艷",
                "prev_context": "相當",
                "next_context": "。",
            },
            {
                "timestamp": 1791811878,
                "bopomofo": "ㄎㄨㄤ",
                "rejected": "匡",
                "accepted": "框",
                "prev_context": "",
                "next_context": "選",
            },
            {
                "timestamp": 1791812032,
                "bopomofo": "ㄎㄨㄤ",
                "rejected": "匡",
                "accepted": "框",
                "prev_context": "圈選",
                "next_context": "",
            },
        ]
        # 模擬 sublimate_pending_revisions 預分析流程（確保測試時將「框選」視為待昇華之新詞）
        had_kuangxuan = "框選" in self.tuner.lexicon_words
        self.tuner.lexicon_words.discard("框選")
        try:
            for rec in records_batch:
                prev_c = rec.get("prev_context", "")
                next_c = rec.get("next_context", "")
                rej = rec.get("rejected", "")
                acc = rec.get("accepted", "")
                prev_toks = [t for t, _, _ in self.tuner.tokenize_with_spans(prev_c)] if prev_c else []
                next_toks = [t for t, _, _ in self.tuner.tokenize_with_spans(next_c)] if next_c else []
                err_toks = prev_toks + ([rej] if rej else []) + next_toks
                cor_toks = prev_toks + ([acc] if acc else []) + next_toks
                rec["symbolic_diffs"] = self.tuner.align_sentence_pair(
                    f"{prev_c}{rej}{next_c}",
                    f"{prev_c}{acc}{next_c}",
                    custom_err_tokens=err_toks,
                    custom_cor_tokens=cor_toks,
                )

            res2 = consultant.sublimate_revisions(
                records_batch,
                known_lexicon_words=self.tuner.base_lexicon_words - {"框選"},
                pseudo_word_checker=self.tuner.is_syntactic_clitic_pseudo_word,
            )
        finally:
            if had_kuangxuan:
                self.tuner.lexicon_words.add("框選")

        words2 = [w["word"] for w in res2["sublimated_words"]]
        rules2 = [(r["prev"], r["next"]) for r in res2["sublimated_rules"]]
        constructions2 = [c["formula"] for c in res2.get("sublimated_constructions", [])]

        # 斷言：荒謬語法短語「每點擊」、[2+1] 造詞「圈選框」與既有同音詞「點擊」、「驚艷」絕不出現在 sublimated_words
        self.assertNotIn("每點擊", words2)
        self.assertNotIn("圈選框", words2)
        self.assertNotIn("點擊", words2)
        self.assertNotIn("驚艷", words2)
        # 斷言：真正的雙字原子動詞「框選」被正確提煉入庫
        self.assertIn("框選", words2)
        # 斷言：二階正向轉移「每 ➔ 點擊」、「點擊 ➔ 一次」、「相當 ➔ 驚艷」、「圈選 ➔ 框」正確提煉
        self.assertIn(("每", "點擊"), rules2)
        self.assertIn(("點擊", "一次"), rules2)
        self.assertIn(("相當", "驚艷"), rules2)
        self.assertIn(("圈選", "框"), rules2)
        # 斷言：階層式集合槽位構式 `[每] → [點擊] → [({一, 二...} → {次, 回, 下...})]` 與 `[({圈, 點, 勾, 框...} → {選, 取})] → [{框, 欄, 區, 鈕, 鍵}]` 正確合成
        self.assertTrue(any("[每] → [點擊] →" in f for f in constructions2), f"Missing [每]→[點擊] construction in {constructions2}")
        self.assertTrue(any("→ {選, 取})] → [{框, 欄, 區, 鈕, 鍵}]" in f for f in constructions2), f"Missing UI head construction in {constructions2}")

    def test_entity_smart_decomposition(self):
        """驗證中文姓名（吳玟蓉、曹大十、歐陽娜娜）與公司全稱智慧拆分衍生詞與轉移引力"""
        w1, t1 = self.tuner.decompose_entity_subentries("吳玟蓉", "ㄨˊ ㄨㄣˊ ㄖㄨㄥˊ", 1550)
        self.assertEqual([x["word"] for x in w1], ["玟蓉"])
        self.assertEqual(w1[0]["bopomofo"], "ㄨㄣˊ ㄖㄨㄥˊ")
        self.assertIn(("吳", "玟蓉"), [(x["prev"], x["next"]) for x in t1])

        w2, t2 = self.tuner.decompose_entity_subentries("曹大十", "ㄘㄠˊ ㄉㄚˋ ㄕˊ", 1550)
        self.assertEqual([x["word"] for x in w2], ["大十"])
        self.assertEqual(w2[0]["bopomofo"], "ㄉㄚˋ ㄕˊ")
        self.assertIn(("曹", "大十"), [(x["prev"], x["next"]) for x in t2])

        w3, t3 = self.tuner.decompose_entity_subentries("歐陽娜娜", "ㄡ ㄧㄤˊ ㄋㄚˋ ㄋㄚˋ", 1600)
        self.assertEqual([x["word"] for x in w3], ["娜娜"])
        self.assertEqual(w3[0]["bopomofo"], "ㄋㄚˋ ㄋㄚˋ")
        self.assertIn(("歐陽", "娜娜"), [(x["prev"], x["next"]) for x in t3])

        w4, t4 = self.tuner.decompose_entity_subentries(
            "如喜科技有限公司",
            "ㄖㄨˊ ㄒㄧˇ ㄎㄜ ㄐㄧˋ ㄧㄡˇ ㄒㄧㄢˋ ㄍㄨㄥ ㄙ",
            1650,
        )
        words4 = [x["word"] for x in w4]
        self.assertIn("如喜科技", words4)
        self.assertIn("如喜", words4)
        self.assertIn(("如喜", "科技"), [(x["prev"], x["next"]) for x in t4])

    def test_batch_diagnosis_ground_truth_and_anti_placeholder(self):
        """
        驗證批次 AI 診斷（語意分析261015.txt 三組句對）：
        1. 嚴格鎖定每一對照組之原始錯誤句與正確句（1-to-1 Ground-Truth Locking），絕不發生組1錯誤句配組2正確句之錯置。
        2. 徹底攔截小型語言模型回聲照抄之 JSON Schema 樣板字串（如「構式名稱」、「前驅純漢字詞元」、「結合注音音韻與詞庫結構之根本語病因」）。
        3. 正確識別「以鯨魚 ➔ 已經於」之跨詞隙交錯搭橋截斷（Type II）與「藥單脣脂 ➔ 要單純只」之完整合併差分。
        """
        from scripts.ai_semantic_consultant import AISemanticConsultant
        from unittest.mock import patch
        import json

        pairs = [
            {
                "error": "另請教這個部分是否以鯨魚如喜語意調校工作室時現？",
                "correct": "另請教這個部分是否已經於如喜語意調校工作室實現？",
            },
            {
                "error": "藥單脣脂打出注音不是很容易。",
                "correct": "要單純只打出注音不是很容易。",
            },
            {
                "error": "想破頭野想不出要怎麼打。",
                "correct": "想破頭也想不出要怎麼打。",
            },
        ]
        for p in pairs:
            p["symbolic_diffs"] = self.tuner.align_sentence_pair(p["error"], p["correct"])

        # 驗證組 1「以鯨魚 ➔ 已經於」被精確識別為「鯨魚」跨界截斷 Type II
        self.assertEqual(pairs[0]["symbolic_diffs"][0]["overlapping_hijack"], "鯨魚")
        self.assertIn("Type II", pairs[0]["symbolic_diffs"][0]["error_taxonomy"])

        # 驗證組 2「藥單脣脂 ➔ 要單純只」重疊區間已合併為單一完整差分，不分裂為「要單」與「單脣脂」
        self.assertEqual(len(pairs[1]["symbolic_diffs"]), 1)
        self.assertEqual(pairs[1]["symbolic_diffs"][0]["rejected"], "藥單脣脂")
        self.assertEqual(pairs[1]["symbolic_diffs"][0]["accepted"], "要單純只")
        self.assertEqual(pairs[1]["symbolic_diffs"][0]["accepted_tokens"], ["要", "單純", "只"])

        consultant = AISemanticConsultant(root_dir)
        # 模擬小型 2B 模型發生張冠李戴（將組2正確句塞入組1）並照抄 Schema 佔位字串的惡劣回傳
        mocked_bad_llm_json = json.dumps({
            "batch_summary": "本批次問題之計算語言學病灶分佈與治理總評",
            "recommended_lexicon_words": ["建議直接補入詞庫的 2~6 字純漢字複合長詞 (去重匯總)"],
            "recommended_rules": [
                {
                    "prev": "前驅純漢字詞元",
                    "next": "後繼純漢字詞元",
                    "score": 28.0,
                    "kind": "collocation",
                    "explanation": "二階語法搭配說明"
                }
            ],
            "item_diagnostics": [
                {
                    "index": 1,
                    "error": "錯誤句：另請教這個部分是否以鯨魚如喜語意調校工作室時現？",
                    "correct": "正確句：要單純只打出注音不是很容易。",
                    "error_taxonomy": "Type I ~ Type VI 病灶分類",
                    "construction_name": "構式名稱",
                    "root_cause": "結合注音音韻與詞庫結構之根本語病因",
                    "pos_breakdown": "詞性與結構成分分析",
                    "unigram_safety_proof": "單字安全隔離與零過度抑制說明"
                }
            ]
        }, ensure_ascii=False)

        with patch.object(consultant, "get_effective_provider", return_value="local_llama"), \
             patch.object(consultant, "_call_llm", return_value=mocked_bad_llm_json):
            res = consultant.diagnose_sentence_pairs_batch(pairs)

        self.assertTrue(res.get("is_batch"))
        bdata = res["batch_data"]
        items = bdata["item_diagnostics"]

        # 斷言：3 組對照組 100% 完整保留，且每一組的 error 與 correct 均嚴格鎖定原始句子
        self.assertEqual(len(items), 3)
        for i, orig in enumerate(pairs):
            self.assertEqual(items[i]["error"], orig["error"])
            self.assertEqual(items[i]["correct"], orig["correct"])
            self.assertNotEqual(items[i]["construction_name"], "構式名稱")
            self.assertNotIn("結合注音音韻與詞庫結構之根本語病因", items[i]["root_cause"])
            self.assertNotIn("詞性與結構成分分析", items[i]["pos_breakdown"])
            self.assertNotIn("單字安全隔離與零過度抑制說明", items[i]["unigram_safety_proof"])

        # 斷言：樣板佔位詞「前驅純漢字詞元」與「後繼純漢字詞元」徹底被攔截
        rule_pairs = [(r["prev"], r["next"]) for r in bdata["recommended_rules"]]
        self.assertNotIn(("前驅純漢字詞元", "後繼純漢字詞元"), rule_pairs)
        self.assertIn(("是否", "已經"), rule_pairs)
        self.assertIn(("已經", "於"), rule_pairs)
        self.assertIn(("要", "單純"), rule_pairs)
        self.assertIn(("單純", "只"), rule_pairs)
        self.assertIn(("想破頭", "也"), rule_pairs)

    def test_multilingual_western_and_codeswitch_constructions(self):
        """
        驗證跨語言中英混打邊界構式（Code-Switching Boundary Construction）與
        歐文多語系語法同音詞／重音符號消歧構式（Western Grammatical Confusable & Diacritic Construction）：
        1. 中英混打「將aup黨匯出 ➔ 將aup檔匯出」：
           - 識別出 prev_ascii="aup"，生成跨語言中英混打邊界構式
           - 嚴禁跨越 ASCII token "aup" 產生「將 ➔ 檔」偽轉移規則
        2. 歐文多語系語法同音與重音符號消歧：
           - 英語 "They left there bags" ➔ "They left their bags"
           - 法語 "Il va a Paris" ➔ "Il va à Paris"
        """
        import os
        from scripts.ai_semantic_consultant import AISemanticConsultant
        root_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        consultant = AISemanticConsultant(root_dir)

        # 1. 中英混打邊界測試
        err_cs = "請先將aup黨匯出備份"
        cor_cs = "請先將aup檔匯出備份"
        diffs_cs = self.tuner.align_sentence_pair(err_cs, cor_cs)
        self.assertEqual(len(diffs_cs), 1)
        d_cs = diffs_cs[0]
        self.assertEqual(d_cs["rejected"], "黨")
        self.assertEqual(d_cs["accepted"], "檔")
        self.assertEqual(d_cs["prev_ascii"], "aup")
        self.assertEqual(d_cs["prev_ctx"], "")

        rules_cs = self.tuner.synthesize_rules(d_cs)
        cs_pairs = [(r["prev"], r["next"]) for r in rules_cs]
        self.assertNotIn(("將", "檔"), cs_pairs)
        self.assertIn(("檔", "匯出"), cs_pairs)
        self.assertTrue(any("跨語言中英混打" in c.get("name", "") for c in d_cs["hierarchical_constructions"]))

        diag_cs = consultant._diagnose_symbolically(err_cs, cor_cs, diffs_cs)
        self.assertIn("跨語言中英混打", diag_cs["syntactic_analysis"]["construction_name"])

        # 2. 英語同音詞與法語重音符號構式測試
        err_en = "They left there bags"
        cor_en = "They left their bags"
        diffs_en = self.tuner.align_sentence_pair(err_en, cor_en)
        self.assertEqual(len(diffs_en), 1)
        self.assertEqual(diffs_en[0]["rejected"], "there")
        self.assertEqual(diffs_en[0]["accepted"], "their")
        self.assertTrue(any("歐文多語系" in c.get("name", "") for c in diffs_en[0]["hierarchical_constructions"]))
        rules_en = self.tuner.synthesize_rules(diffs_en[0])
        self.assertEqual(len(rules_en), 0)

        err_fr = "Il va a Paris"
        cor_fr = "Il va à Paris"
        diffs_fr = self.tuner.align_sentence_pair(err_fr, cor_fr)
        self.assertEqual(len(diffs_fr), 1)
        self.assertEqual(diffs_fr[0]["rejected"], "a")
        self.assertEqual(diffs_fr[0]["accepted"], "à")
        self.assertTrue(any("歐文多語系" in c.get("name", "") for c in diffs_fr[0]["hierarchical_constructions"]))
        rules_fr = self.tuner.synthesize_rules(diffs_fr[0])
        self.assertEqual(len(rules_fr), 0)
        diag_fr = consultant._diagnose_symbolically(err_fr, cor_fr, diffs_fr)
        self.assertIn("歐文多語系", diag_fr["syntactic_analysis"]["construction_name"])


if __name__ == "__main__":
    unittest.main()

