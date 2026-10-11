#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
semantic_router.py — 語意陣列與句法轉移路由引擎 (Semantic Array & Syntactic Transition Engine)
=============================================================================
核心原理：
1. 拒絕死板窮舉 (Anti-Enumeration)：由 N 個音節候選 + M 個音節候選組成語意陣列，
   在 O(N + M) 空間複雜度下，經由句法語法規則 (Layer 1)、大數據轉移矩陣 (Layer 2)
   與離合詞語意關聯 (Layer 3)，動態提取最佳上下文語意，自然產出 N * M 的精準語意結果。
2. 經典案例驗證：
   「住在很南的那個男的非常難得的難得很難。」
   - 住在很[南]的 ➔ 方位詞語意加分
   - 那個[男的]非常 ➔ 指示詞+人稱名詞加分
   - 非常[難得]的 ➔ 程度副詞+形容詞加分
   - [難]得很 ➔ 形容詞+補語標記「得」+程度補語「很」加分
   - 很[難] ➔ 程度副詞+形容詞加分
"""

import json
import os
import time
from typing import Dict, List, Optional, Set, Tuple


class FactorizedGrammarPattern:
    """
    因式分解語意句型槽位 (Factorized Semantic Grammar Pattern)
    以 A * B * C * ... 乘積語意槽位，避免在 TSV 中展開幾千筆窮舉 Bigram。
    例如：
    Slots = [
      ["有", "沒", "算", "哪", "差", "憑"],      # Slot 0: 存在/否定/反問動詞
      ["什麼", "啥", "何", "甚麼", "哪樣"],       # Slot 1: 疑問詞
      ["好", "真", "那麼", "這麼", "太", "可"]    # Slot 2: 程度/語氣助詞 (可選)
    ]
    Targets: [形容詞]+的 (難的、好的、怕的、怪的、笑的、大不了的...) 或 形容詞原形
    Suppress: 同音混淆詞 (南得、難得、男的、南德)
    Punctuation: [？, ！, 。, 嗎, 呢, 啦]
    """

    def __init__(
        self,
        pattern_id: str,
        name: str,
        slots: List[List[str]],
        optional_slots: Optional[Set[int]] = None,
        preferred_targets: Optional[List[str]] = None,
        suppress_targets: Optional[List[str]] = None,
        punctuation: Optional[List[str]] = None,
        bonus: float = 38.0,
        penalty: float = -18.0
    ):
        self.pattern_id = pattern_id
        self.name = name
        self.slots = slots
        self.optional_slots = optional_slots if optional_slots is not None else {2}
        self.preferred_targets = set(preferred_targets or [])
        self.suppress_targets = set(suppress_targets or [])
        self.punctuation = set(punctuation or ["？", "?", "！", "!", "。", "嗎", "呢", "啦", "呀", "哩"])
        self.bonus = bonus
        self.penalty = penalty

    def matches_context(self, prev_w: str) -> bool:
        """檢驗 prev_w 是否符合槽位語法組合 (A * B * C ...)"""
        if not prev_w or not self.slots:
            return False

        slot0 = self.slots[0]
        # 若僅有 1 個槽位，或 slot 1 為可選槽位時，支援直接匹配 slot0
        if len(self.slots) == 1 or (1 in self.optional_slots):
            for s0 in slot0:
                if prev_w.endswith(s0):
                    return True

        if len(self.slots) < 2:
            return False

        slot1 = self.slots[1]
        has_slot2 = len(self.slots) > 2
        slot2 = self.slots[2] if has_slot2 else []

        for s0 in slot0:
            for s1 in slot1:
                prefix = f"{s0}{s1}"
                if prev_w.endswith(prefix):
                    return True
                if has_slot2:
                    for s2 in slot2:
                        if prev_w.endswith(f"{prefix}{s2}"):
                            return True
        return False

    def evaluate(self, prev_w: str, cand: str, next_w: str, adjectives_ref: Set[str]) -> float:
        if not self.matches_context(prev_w):
            return 0.0

        score = 0.0
        # 1. 優先排除干擾混淆詞 (如：南得、難得、男的、南德)
        if cand in self.suppress_targets:
            return self.penalty

        # 2. 目標詞判定：形容詞名物化 ([ADJ]+的) 或在偏好清單中，或單音節形容詞實詞
        is_adjective_de = (
            cand.endswith("的") and (
                cand[:-1] in adjectives_ref or
                cand in self.preferred_targets or
                cand[:-1] in {"難", "好", "怪", "怕", "美", "累", "痛", "大不了", "稀罕", "要緊", "多", "少", "大", "小", "笑", "氣"}
            )
        )
        single_adj = (len(cand) == 1 and cand in adjectives_ref and cand != "得")

        if cand in self.preferred_targets or is_adjective_de or single_adj:
            score += self.bonus
            # 若緊接標點符號或疑問終止助詞，給予閉合加分
            if next_w and any(next_w.startswith(p) for p in self.punctuation):
                score += 15.0

        return score


class FactorizedPatternEngine:
    """
    高效能因式分解句型路由器 (Factorized Pattern Router Engine)
    取代平鋪直敘的 TSV 大檔案，支援毫秒級 O(1) 槽位比對、JSONL 緊湊儲存、
    與本地 AI (Gemma) 提煉法則動態註冊。
    """

    def __init__(self, data_dir: str, adjectives_ref: Set[str]):
        self.data_dir = data_dir
        self.adjectives_ref = adjectives_ref
        self.jsonl_path = os.path.join(data_dir, "patterns_user.jsonl")
        self.patterns: Dict[str, FactorizedGrammarPattern] = {}
        self._init_default_patterns()
        self.load_patterns()

    def _init_default_patterns(self):
        # 預設句型 1：疑問存在句 + 形容詞名物化「的」
        # {有, 沒, 算, 哪, 差, 憑} * {什麼, 啥, 何, 甚麼, 哪樣} * {(好, 真, 那麼, 這麼, 太, 可)} * {[形容詞]+的} * {？, ！, 。}
        pat1 = FactorizedGrammarPattern(
            pattern_id="rhetorical_question_predicate_de",
            name="疑問存在句+形容詞名物化",
            slots=[
                ["有", "沒", "算", "哪", "差", "憑"],
                ["什麼", "啥", "何", "甚麼", "哪樣"],
                ["好", "真", "那麼", "這麼", "太", "可"]
            ],
            optional_slots={2},
            preferred_targets=[
                "難的", "好的", "壞的", "怕的", "怪的", "笑的", "氣的", "痛的", "累的",
                "遠的", "貴的", "便宜的", "大不了的", "稀罕的", "要緊的", "難", "好", "多", "少", "大", "小"
            ],
            suppress_targets=["南得", "難得", "男的", "南德", "喃得", "湳得"],
            punctuation=["？", "?", "！", "!", "。", "嗎", "呢", "啦", "呀", "哩"],
            bonus=38.0,
            penalty=-18.0
        )
        self.patterns[pat1.pattern_id] = pat1

    def load_patterns(self):
        if not os.path.exists(self.jsonl_path):
            self.save_patterns()
            return
        try:
            with open(self.jsonl_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    data = json.loads(line)
                    pat = FactorizedGrammarPattern(
                        pattern_id=data["pattern_id"],
                        name=data.get("name", data["pattern_id"]),
                        slots=data["slots"],
                        optional_slots=set(data.get("optional_slots", [2])),
                        preferred_targets=data.get("preferred", []),
                        suppress_targets=data.get("suppress", []),
                        punctuation=data.get("punctuation", ["？", "！", "。"]),
                        bonus=float(data.get("bonus", 38.0)),
                        penalty=float(data.get("penalty", -18.0))
                    )
                    self.patterns[pat.pattern_id] = pat
        except Exception:
            pass

    def save_patterns(self):
        try:
            with open(self.jsonl_path, "w", encoding="utf-8") as f:
                for pat in self.patterns.values():
                    record = {
                        "pattern_id": pat.pattern_id,
                        "name": pat.name,
                        "slots": pat.slots,
                        "optional_slots": list(pat.optional_slots),
                        "preferred": list(pat.preferred_targets),
                        "suppress": list(pat.suppress_targets),
                        "punctuation": list(pat.punctuation),
                        "bonus": pat.bonus,
                        "penalty": pat.penalty
                    }
                    f.write(json.dumps(record, ensure_ascii=False) + "\n")
        except Exception:
            pass

    def register_pattern(self, pattern_dict: dict) -> bool:
        """從 Gemma 提煉或社群聯邦接收新語意法則並註冊"""
        try:
            pat_id = pattern_dict.get("pattern_id") or f"pat_{int(time.time())}"
            pat = FactorizedGrammarPattern(
                pattern_id=pat_id,
                name=pattern_dict.get("name", pat_id),
                slots=pattern_dict.get("slots", []),
                optional_slots=set(pattern_dict.get("optional_slots", [2])),
                preferred_targets=pattern_dict.get("preferred", []),
                suppress_targets=pattern_dict.get("suppress", []),
                punctuation=pattern_dict.get("punctuation", ["？", "！", "。"]),
                bonus=float(pattern_dict.get("bonus", 38.0)),
                penalty=float(pattern_dict.get("penalty", -18.0))
            )
            self.patterns[pat_id] = pat
            self.save_patterns()
            return True
        except Exception:
            return False

    def score(self, prev_w: str, cand: str, next_w: str) -> float:
        total = 0.0
        for pat in self.patterns.values():
            total += pat.evaluate(prev_w, cand, next_w, self.adjectives_ref)
        return total


class SemanticTransitionRouter:
    """
    純 Python 端語意陣列與上下文句法轉移路由器
    """

    def __init__(self, data_dir: Optional[str] = None):
        if data_dir is None:
            base_dir = os.path.dirname(os.path.abspath(__file__))
            data_dir = os.path.join(os.path.dirname(base_dir), "ruhi_lexicon", "data")
        self.data_dir = data_dir

        # Layer 2: 轉移矩陣 (prev, curr) -> bonus
        self.transitions: Dict[Tuple[str, str], float] = {}

        # 封閉類功能詞集合與詞性語意陣列
        self._init_semantic_lexicon()
        self._load_transitions()

        # 因式分解句型路由器 (Factorized Pattern Engine)
        self.pattern_engine = FactorizedPatternEngine(self.data_dir, self.ADJECTIVES)

    def _init_semantic_lexicon(self):
        """初始化語法特徵陣列 (Layer 1 規則庫)"""
        # 程度副詞
        self.DEGREE_ADVERBS: Set[str] = {
            "很", "非常", "太", "最", "更", "極", "真", "挺", "蠻", "好",
            "格外", "相當", "十分", "甚", "分外", "特別", "越發"
        }
        # 指示代詞與量詞
        self.DEMONSTRATIVES: Set[str] = {
            "這", "那", "這個", "那個", "這些", "那些", "某", "某個", "此", "該"
        }
        self.CLASSIFIERS: Set[str] = {
            "個", "位", "隻", "條", "把", "次", "回", "種", "件", "份", "張",
            "塊", "本", "輛", "台", "架", "部", "首", "名"
        }
        # 方位與地理實詞
        self.DIRECTION_WORDS: Set[str] = {
            "南", "北", "東", "西", "中", "上", "下", "左", "右", "前", "後",
            "裡", "外", "南部", "北部", "東部", "西部", "南方", "北方",
            "南邊", "北邊", "東邊", "西邊", "旁邊", "對面", "後面", "前面", "上方", "下方"
        }
        # 地標與建築物名詞 (Landmarks & Buildings)
        self.BUILDING_NOUNS: Set[str] = {
            "宿舍", "男生宿舍", "女生宿舍", "學校", "大樓", "大廈", "車站", "家",
            "村", "門", "公寓", "校區", "球場", "建築", "房間", "套房", "雅房", "男宿", "女宿"
        }
        # 模態/評價動詞詞根 (配合「難/好」生成形容詞：難住、難吃、難看、好住、好吃、好看)
        self.MODAL_PREFIX_VERBS: Set[str] = {
            "住", "吃", "看", "喝", "聽", "開", "讀", "寫", "做", "受", "懂",
            "走", "買", "賣", "認", "記", "找", "修", "教", "學", "過", "用", "穿", "戴"
        }
        # 處所／居所／空間方位錨定動詞 (Locative & Spatial Predicate Anchors)
        self.LOCATIVE_VERBS: Set[str] = {
            "住", "住在", "居", "居於", "在", "位於", "處於", "座落於", "座落",
            "落在", "站在", "立於", "置於", "去", "來", "到", "走向", "前往", "飛往",
            "朝向", "面向", "偏向"
        }
        # 人稱與稱謂名詞
        self.PERSON_NOUNS: Set[str] = {
            "男", "男的", "男人", "男生", "男子", "女", "女的", "女人", "女生",
            "女子", "人", "朋友", "老師", "學生", "同志", "傢伙", "老兄", "少年"
        }
        # 常見形容詞 (可受程度副詞修飾、可帶補語)
        self.ADJECTIVES: Set[str] = {
            "難", "難得", "好", "快", "慢", "多", "少", "大", "小", "美", "累",
            "熱", "冷", "忙", "貴", "便宜", "高", "低", "長", "短", "新", "舊"
        }
        # 疑問代詞與詢問功能詞 (如：什麼、怎麼、哪、誰、何、多麼)
        self.QUESTION_PRONOUNS: Set[str] = {
            "什麼", "有什麼", "沒什麼", "怎麼", "如何", "哪樣", "多麼"
        }
        # 否定詞
        self.NEGATIONS: Set[str] = {"不", "沒", "沒有", "別", "莫", "休"}

        # 名形兼類實詞集 (Noun-Adjective Multi-Category Words)
        self.NOUN_ADJ_DUAL_CATEGORY: Set[str] = {
            "經典", "權威", "傳統", "規範", "科學", "標準", "基本", "專業",
            "極端", "熱門", "理想", "關鍵", "核心", "現代", "典型"
        }
        # 兼類詞前置修飾之後接名詞/概念
        self.DUAL_POST_NOUNS: Set[str] = {
            "定義", "著作", "案例", "之作", "語錄", "範例", "教材", "理論",
            "方法", "解答", "文化", "觀念", "流程", "標準", "態度", "問題", "作品", "狀態"
        }
        # 兼類詞後置核心之前驅修飾語
        self.DUAL_PRE_MODIFIERS: Set[str] = {
            "的", "不朽的", "永恆的", "偉大的", "傳世的", "歷久彌新的", "跨時代的", "嚴格的", "現代的"
        }
        # 程度與結果補語關鍵詞 (台語 kah / 英文 so... that... 補語結構)
        self.COMPLEMENT_MARKERS: Set[str] = {
            "不得了", "要命", "不行", "要死", "極了", "很", "跳起來", "發抖", "飛快",
            "合不攏嘴", "說不出話", "喘不過氣", "清清楚楚", "乾乾淨淨", "團團轉", "落花流水",
            "如此", "這麼", "那麼", "這般", "那般", "定義如此",
            "一點都", "一點也不", "絲毫都", "絲毫也不", "舒服", "習慣", "安穩", "難受"
        }
        # 常帶程度補語之形容詞與動詞
        self.PRE_DE2_WORDS: Set[str] = {
            "高興", "美麗", "開心", "好看", "氣", "笑", "跑", "走", "看", "聽",
            "做", "寫", "打", "說", "吃", "喝", "玩", "累", "痛", "熱", "冷",
            "急", "忙", "快", "慢", "好", "多", "少", "大", "小", "經典",
            "住", "活", "過", "睡", "穿", "戴", "用", "學", "教"
        }
        # 常見領屬主體名詞與代詞 (台語 ê / 英文 's / of 定語結構)
        self.PRE_DE_NOUNS: Set[str] = {
            "我", "你", "他", "她", "它", "我們", "你們", "他們", "大家",
            "至上意識", "心靈", "靈魂", "沈伯洋", "老師", "學生", "朋友", "國家",
            "學校", "世界", "歷史", "時代", "社會", "宇宙"
        }
        # 常見核心名詞 (受「的」修飾)
        self.POST_DE_NOUNS: Set[str] = {
            "意願", "心態", "想法", "未來", "希望", "眼睛", "朋友", "地方",
            "事物", "內容", "功能", "速度", "能力", "小麻糬", "聲音", "生活", "故事",
            "男生宿舍", "女生宿舍", "宿舍", "男生", "女生", "房子", "房間"
        }

    def _load_transitions(self):
        """載入 transitions_user.tsv 轉移矩陣 (Layer 2)"""
        tsv_path = os.path.join(self.data_dir, "transitions_user.tsv")
        if not os.path.exists(tsv_path):
            return

        try:
            with open(tsv_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split("\t")
                    if len(parts) >= 3:
                        prev_w = parts[0].strip()
                        curr_w = parts[1].strip()
                        try:
                            score = float(parts[2].strip())
                            self.transitions[(prev_w, curr_w)] = score
                        except ValueError:
                            pass
            try:
                print(f"[semantic_router] 語意轉移矩陣載入完成：{len(self.transitions)} 筆")
            except Exception:
                pass
        except Exception as e:
            try:
                print(f"[semantic_router] 載入轉移矩陣失敗: {e}")
            except Exception:
                pass

    def score_semantic_transition(self, prev_w: str, cand: str, next_w: str) -> float:
        """
        計算候選詞在上下文 (prev_w, next_w) 中的語意轉移總分
        """
        score = 0.0

        # ── 0. Layer 0: 因式分解乘積語意槽位 (Factorized Pattern Engine) ──
        if hasattr(self, 'pattern_engine') and self.pattern_engine:
            score += self.pattern_engine.score(prev_w, cand, next_w)

        # ── 1. Layer 2: 轉移矩陣精準 Bigram 比對 ──
        if prev_w:
            score += self.transitions.get((prev_w, cand), 0.0) * 4.0
        if next_w:
            score += self.transitions.get((cand, next_w), 0.0) * 4.0

        # ── 2. Layer 1: 句法語法陣列推導 (Syntactic Array Rules) ──

        # A. 前驅語意搭配 (Left context)
        if prev_w:
            # (1) 程度副詞 + 實詞修飾
            if prev_w in self.DEGREE_ADVERBS or any(prev_w.endswith(adv) for adv in self.DEGREE_ADVERBS):
                has_locative_anchor = any(v in prev_w for v in self.LOCATIVE_VERBS)

                # A. 處所動詞解鎖方位詞的程度修飾：[處所動詞] + [很/非常] + [南/北/東/西]
                if cand in self.DIRECTION_WORDS or cand.startswith("南") or cand.startswith("北"):
                    if has_locative_anchor:
                        score += 35.0  # 處所動詞錨定空間方位修飾語（住在很南、位於很北）
                    else:
                        # 無處所動詞時，「很南」、「很北」極為罕見，避免壓過「很難」
                        score += 5.0
                elif cand in self.ADJECTIVES or cand == "難得":
                    score += 28.0

                # B. 創造性名詞形容詞化生產力構式：很 + [名詞/專名/概念] + 的/名詞
                # （如：很如喜的智慧、很文青的風格、很男的氣質、男得很男）
                elif cand in self.PERSON_NOUNS or cand in self.PRE_DE_NOUNS or cand in {"如喜", "文青", "台灣", "藝術", "個性", "哲學"}:
                    is_denominal_frame = bool(
                        (next_w and (next_w.startswith("的") or next_w in self.POST_DE_NOUNS or next_w in self.DUAL_POST_NOUNS or next_w in ("智慧", "樣子", "風格", "表現", "味道", "氣質", "個性", "特質"))) or
                        prev_w.endswith("得很") or prev_w == "得很" or
                        (cand in {"男", "女"} and (next_w.startswith("得") or next_w.startswith("很") or prev_w.endswith("得很")))
                    )
                    if is_denominal_frame:
                        score += 28.0
                    else:
                        score -= 8.0

            # (2) 指示詞/量詞/修飾語「的」 + 人稱名詞（如：那個男的、南邊的男的、這隻貓、這位老師）
            if prev_w in self.DEMONSTRATIVES or prev_w in self.CLASSIFIERS or prev_w.endswith("個") or (prev_w.endswith("的") and (len(cand) > 1 or cand == "人")):
                if cand in self.PERSON_NOUNS or cand in ("男的", "女的", "男生", "女生"):
                    score += 32.0
                elif cand in self.DIRECTION_WORDS or cand in self.ADJECTIVES:
                    # 「那個南」、「南邊的難得」不符句法
                    score -= 18.0
            elif prev_w.endswith("的") and len(cand) == 1 and cand in ("男", "女"):
                # 單字「的男」、「的女」非獨立名詞碎片，扣分
                score -= 20.0

            # (3) 居所動詞 + 方位詞（如：住在很南、走到南邊、在南方）
            if any(v in prev_w for v in self.LOCATIVE_VERBS):
                if cand in self.DIRECTION_WORDS or cand.startswith("南") or cand.startswith("北"):
                    score += 22.0

            # (4) 否定詞 + 實詞 (不難、不南、沒好)
            if prev_w in self.NEGATIONS:
                if cand in self.ADJECTIVES:
                    score += 20.0

            # (5) 疑問/存在句型：(有)什麼/怎麼 + 形容詞/形容詞短語（如：有什麼難的、什麼難、怎麼難）
            if any(q in prev_w for q in self.QUESTION_PRONOUNS) or prev_w.endswith("什麼"):
                if cand in ("難的", "難", "好", "多", "大"):
                    score += 30.0
                elif cand == "難得":
                    # 「有什麼難得」在漢語中遠少於「有什麼難的/有什麼難」
                    score -= 10.0

            # (6) 罕見/頻率副詞「難得」後接形容詞謂語（如：難得很難、難得很好）
            if prev_w.endswith("難得") or prev_w in ("難得", "非常難得"):
                if cand in ("很難", "很好", "很珍貴", "很難得", "很男"):
                    score += 32.0

            # (7) 補語標記「得」後接程度補語（如：男得很難、男得很男、住得一點都不難住）
            if prev_w.endswith("得") and cand in ("很男", "很難", "很好", "很美", "很帥", "一點都不難住", "一點都不", "一點也不"):
                score += 35.0

            # (8) 建築物/地標 + 方位詞（如：男生宿舍南邊、學校對面、車站旁邊）
            if any(prev_w.endswith(b) for b in self.BUILDING_NOUNS):
                if cand in self.DIRECTION_WORDS or cand in ("南邊", "北邊", "東邊", "西邊", "旁邊", "對面", "後面", "前面"):
                    score += 32.0

        # B. 後繼語意搭配 (Right context)
        if next_w:
            # (1) 程度補語結構：形容詞 + 得很 (如：難得很、好得很、多得很)
            if next_w.startswith("得很") or next_w == "很":
                if cand == "得":
                    # 當前字本身就是結構助詞「得」
                    score += 35.0
                elif cand in self.ADJECTIVES:
                    score += 30.0

            # (2) 補語標記：實詞 + 得
            if next_w.startswith("得") and not next_w.startswith("得到"):
                if cand in self.ADJECTIVES or cand in self.PRE_DE2_WORDS or cand in self.NOUN_ADJ_DUAL_CATEGORY or cand in ("男", "女", "住"):
                    score += 35.0

            # (3) 定語助詞：實詞 + 的 (如：很南的、男的、難得的、南邊的、很難住的)
            if next_w.startswith("的"):
                if cand in self.DIRECTION_WORDS or cand in self.ADJECTIVES or cand.endswith("難住") or cand.endswith("好住"):
                    score += 22.0
                elif cand in {"男", "女"}:
                    score += 25.0  # 「男的」、「女的」成詞高頻

            # (4) 罕見/頻率副詞「難得」後接形容詞謂語（如：難得 + 很難）
            if next_w.startswith("很難") or next_w.startswith("很好"):
                if cand == "難得":
                    score += 32.0

        # ── 3. 名形兼類詞通用語意法則 (Universal Dual-Category Rule) ──
        if cand in self.NOUN_ADJ_DUAL_CATEGORY:
            # A. 前置修飾位 (Attributive / Adverbial): 經典(的)定義、經典案例
            if next_w:
                if next_w in self.DUAL_POST_NOUNS or next_w.startswith("的") or next_w in self.ADJECTIVES:
                    score += 40.0
            # B. 後置核心位 (Head Noun): 不朽的經典、歷史的經典、如此經典
            if prev_w:
                if prev_w.endswith("的") or prev_w in self.DUAL_PRE_MODIFIERS or prev_w in ("如此", "非常", "特別", "極其", "多麼"):
                    score += 40.0

        # 抗混淆：排除「經點」等無語意拼湊碎片
        if cand == "經點":
            if (next_w and (next_w in self.DUAL_POST_NOUNS or next_w.startswith("的"))) or \
               (prev_w and (prev_w.endswith("的") or prev_w in ("如此", "非常", "特別"))):
                score -= 30.0

        # ── 4. 補語「得」(kah) vs 定語「的」(ê) 跨語言語意分野 ──
        if cand == "得":
            is_complement_ctx = bool(next_w and (
                any(next_w.startswith(m) for m in self.COMPLEMENT_MARKERS) or
                next_w.startswith("很") or "如此" in next_w or "不得了" in next_w or
                "一點都" in next_w or "一點也不" in next_w or
                next_w in ("跳起來", "不行", "要命")
            ))
            # (1) 前為形容詞/動詞或形容詞化實詞，後接程度補語 (如：高興得跳起來、美麗得不得了、經典得定義如此經典、男得很男、住得一點都不難住)
            if prev_w and (prev_w in self.PRE_DE2_WORDS or any(prev_w.endswith(w) for w in self.PRE_DE2_WORDS) or prev_w in self.ADJECTIVES or prev_w in self.PERSON_NOUNS or prev_w in self.NOUN_ADJ_DUAL_CATEGORY):
                if is_complement_ctx:
                    score += 48.0
                elif next_w:
                    score += 25.0
            # (2) 若前驅是典型名詞領屬主體且後繼非補語，不該用「得」
            if prev_w and (prev_w in self.PRE_DE_NOUNS or (prev_w in self.PERSON_NOUNS and prev_w not in ("男", "女"))):
                if not is_complement_ctx:
                    score -= 30.0

        elif cand == "的":
            # (1) 領屬/定語結構：名詞/代詞/修飾語 + 的 + 名詞 (如：至上意識的意願、我們的小麻糬、很難住的男生宿舍、南邊的男的)
            if prev_w and (prev_w in self.PRE_DE_NOUNS or prev_w in self.PERSON_NOUNS or prev_w in self.ADJECTIVES or prev_w in self.NOUN_ADJ_DUAL_CATEGORY or prev_w in self.DIRECTION_WORDS or any(prev_w.endswith(m) for m in ("難住", "好住", "南邊", "北邊"))):
                if next_w and (next_w in self.POST_DE_NOUNS or next_w in self.DUAL_POST_NOUNS or any(next_w.startswith(b) for b in self.BUILDING_NOUNS) or next_w in self.PERSON_NOUNS or next_w in ("男的", "女生", "男生")):
                    score += 45.0
                else:
                    score += 20.0
            # (2) 若前驅為動詞/形容詞且後接強程度補語 (如：高興得跳起來、美麗得不得了、住得一點都不難住)，排斥「的」
            if prev_w and (prev_w in self.PRE_DE2_WORDS or any(prev_w.endswith(w) for w in self.PRE_DE2_WORDS) or prev_w in ("高興", "美麗", "氣", "笑", "痛", "累", "住")):
                if next_w and (any(next_w.startswith(m) for m in self.COMPLEMENT_MARKERS) or "一點都" in next_w or "不得了" in next_w or "跳起來" in next_w):
                    score -= 35.0

        # ── 5. 模態評價形容詞構式：{難, 好} + [動詞]（如：難住、好住、難吃、難看、好開） ──
        is_modal_eval_adj = (
            len(cand) == 2 and cand[0] in ("難", "好") and cand[1] in self.MODAL_PREFIX_VERBS
        )
        if is_modal_eval_adj:
            # 前接程度副詞或否定詞（很難住、一點都不難住、不難吃、太難開）
            if prev_w and (
                prev_w in self.DEGREE_ADVERBS or any(prev_w.endswith(adv) for adv in self.DEGREE_ADVERBS) or
                prev_w in self.NEGATIONS or any(prev_w.endswith(neg) for neg in ("不", "沒", "一點都不", "一點也不", "毫無"))
            ):
                score += 35.0
            # 後接定語助詞「的」（很難住的男生宿舍）
            if next_w and next_w.startswith("的"):
                score += 25.0

        # 抗混淆：同音偽詞（南住、男住、男邊、難邊、南生、難生）
        if (len(cand) == 2 and cand[0] in ("南", "男") and cand[1] in self.MODAL_PREFIX_VERBS) or cand in ("男邊", "難邊", "南生", "難生"):
            score -= 40.0

        return score

    def synthesize_semantic_pairs(
        self,
        chars_pos0: List[str],
        chars_pos1: List[str],
        prev_context: str,
        next_context: str,
        top_k: int = 10
    ) -> List[Tuple[str, float]]:
        """
        以 N 個首字候選 + M 個次字候選，利用語意陣列與句法轉移，
        在 O(N + M) 候選空間下動態合成出最優的 N * M 雙字詞候選與語意得分。
        避免以靜態 TSV 窮舉列舉法！
        """
        scored_pairs: List[Tuple[str, float]] = []
        prev_w = prev_context[-4:] if prev_context else ""
        next_w = next_context[:4] if next_context else ""

        for c0 in chars_pos0[:8]:
            for c1 in chars_pos1[:8]:
                pair = f"{c0}{c1}"
                # 計算 pair 內部語意相容性 + 上下文轉移
                inner_score = self.transitions.get((c0, c1), 0.0) * 5.0
                ctx_score = self.score_semantic_transition(prev_w, pair, next_w)
                # 加上單字轉移
                c0_ctx = self.score_semantic_transition(prev_w, c0, c1)
                c1_ctx = self.score_semantic_transition(c0, c1, next_w)
                total = inner_score + ctx_score + (c0_ctx * 0.5) + (c1_ctx * 0.5)
                scored_pairs.append((pair, total))

        scored_pairs.sort(key=lambda x: x[1], reverse=True)
        return scored_pairs[:top_k]

    def synthesize_multi_dim_array(
        self,
        char_slots: List[List[str]],
        prev_context: str,
        next_context: str,
        top_k: int = 10
    ) -> List[Tuple[str, float]]:
        """
        多維語意陣列解碼器 (Multi-Dimensional Semantic Tensor Lattice Search):
        輸入 L 個位置的同音候選陣列 [A, B, C, ...]，總候選空間為 A * B * C * ...
        在 O(A + B + C + ...) 儲存下，藉由動態規劃 (Viterbi) 毫秒級找出全域最優路徑。
        """
        if not char_slots:
            return []
        if len(char_slots) == 1:
            return [(c, self.score_semantic_transition(prev_context[-4:], c, next_context[:4])) for c in char_slots[0][:top_k]]
        if len(char_slots) == 2:
            return self.synthesize_semantic_pairs(char_slots[0], char_slots[1], prev_context, next_context, top_k)

        # 多維 Viterbi Beam Search (保留前 12 條最佳路徑分支)
        prev_w = prev_context[-4:] if prev_context else ""
        next_w = next_context[:4] if next_context else ""

        # dp 路徑: List of (word_so_far, cumulative_score, last_char)
        beam_paths: List[Tuple[str, float, str]] = []
        for c0 in char_slots[0][:6]:
            s0 = self.score_semantic_transition(prev_w, c0, "")
            beam_paths.append((c0, s0, c0))

        for slot_idx in range(1, len(char_slots)):
            curr_slot = char_slots[slot_idx][:6]
            is_last = (slot_idx == len(char_slots) - 1)
            new_paths: List[Tuple[str, float, str]] = []

            for path_word, path_score, last_char in beam_paths:
                for c in curr_slot:
                    pair_score = self.transitions.get((last_char, c), 0.0) * 2.0
                    # 支援長跨度前驅轉移 (如「住在」->「南」、「住在」->「很南」)
                    if len(path_word) >= 2:
                        pair_score += self.transitions.get((path_word[-2:], c), 0.0) * 2.0
                    for v in sorted(self.LOCATIVE_VERBS, key=len, reverse=True):
                        if v in path_word:
                            if (v, c) in self.transitions:
                                pair_score += self.transitions[(v, c)] * 2.0
                            if (v, last_char + c) in self.transitions:
                                pair_score += self.transitions[(v, last_char + c)] * 2.0
                            if c in self.DIRECTION_WORDS or c.startswith("南") or c.startswith("北"):
                                pair_score += 15.0
                            break

                    step_ctx = self.score_semantic_transition(path_word[-4:], c, next_w if is_last else "")
                    new_word = path_word + c
                    total = path_score + pair_score + step_ctx
                    new_paths.append((new_word, total, c))

            new_paths.sort(key=lambda x: x[1], reverse=True)
            beam_paths = new_paths[:12]

        # 針對最終完整詞，再整體評估與前後上下文的適配度
        final_results = []
        for path_word, score, _ in beam_paths:
            whole_score = self.score_semantic_transition(prev_w, path_word, next_w)
            final_results.append((path_word, score + whole_score))

        final_results.sort(key=lambda x: x[1], reverse=True)
        return final_results[:top_k]

    def rank_candidates(
        self,
        prev_context: str,
        candidates: List[dict],
        next_context: str,
        learning_store=None
    ) -> List[dict]:
        """
        以語意陣列與動態學習對候選項目進行綜合加權重排
        """
        if not candidates:
            return []

        # 提取最近的前驅詞（最多取最後 4 字作為上下文參考）
        prev_w = prev_context[-4:] if prev_context else ""
        next_w = next_context[:4] if next_context else ""

        def eval_item(it: dict) -> Tuple[float, int, float]:
            word = it.get("word", "")
            bpmf = it.get("bpmf", "")
            base_wt = float(it.get("weight", 100))

            # 1. 語意陣列上下文轉移分數
            sem_score = self.score_semantic_transition(prev_w, word, next_w)

            # 2. GBoard 動態頻率次數
            hits = 0
            last_used = 0.0
            if learning_store:
                hits = learning_store.get_hits(bpmf, word)
                key = learning_store._make_key(bpmf, word)
                last_used = learning_store.history.get(key, {}).get("last_used", 0.0)

            # 總綜合評分：語意轉移權重優先 (大幅壓過靜態詞頻)，動態學習加乘
            composite_score = sem_score * 100.0 + (hits * 300.0) + (base_wt * 0.1)
            return (composite_score, hits, last_used)

        return sorted(candidates, key=eval_item, reverse=True)
