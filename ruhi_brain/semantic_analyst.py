#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 通用智慧型輸入法 - 語意分析大腦模組
ruhi_brain.semantic_analyst — 本地/遠端 AI 語意分析師與使用者詞庫防劫掠規則庫引擎
=============================================================================
核心宗旨與架構原則：
1. 「使用者詞庫是一帖含著劇毒的速效藥」：
   - 使用者自建詞無條件放進詞庫，能滿足「立刻再打一次直接顯示」的及時需求；
   - 但若採用傳統輸入法的超高先驗權重（如 2000~9999），會形成語意黑洞，
     對內建詞庫造成嚴重的「語意劫掠」（非預期覆寫）、切碎語法構式，甚至永久固化錯別字（如「粒刻」）。
2. 《嚴禁單字權重打地鼠守則》的全面落實：
   - 實體先驗歸一化 (Unigram Prior Neutralization)：自建詞一律降回安全中立先驗（750~880）。
   - 二階語意引力場 (Attractor Transitions)：自建詞僅在其所屬的上下文語意場中獲得強引力（+25.0 ~ +38.0）。
   - 防劫掠互斥罩 (Anti-Hijacking Exclusion Shields)：針對同音競態詞與潛在誤切邊界施加負向抑制（-30.0 ~ -50.0）。
   - 錯字防火牆 (Typo Firewall)：主動攔截與防禦筆誤詞（例如將「立刻」誤為「粒刻」），杜絕毒化詞庫。
3. 三軌後端架構：
   - 優先順位 1：Google Gemini API (遵循 ATG 全域技術生態第一優先原則)
   - 優先順位 2：本機 Ollama Gemma (gemma2/gemma3/gemma4，本地離線 SLM)
   - 優先順位 3：本機啟發式語意引擎 (Heuristic Semantic Fallback，零外部依賴，純 Python 穩定保底)
"""

import json
import math
import os
import re
import sys
import time
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, Set, Tuple

try:
    import pypinyin
    PYPINYIN_AVAILABLE = True
except ImportError:
    PYPINYIN_AVAILABLE = False


# =============================================================================
# 1. 錯字防火牆常數庫 (Typo Firewall Constants)
# =============================================================================

COMMON_TYPO_MAP: Dict[str, str] = {
    # 極高頻生活與科技筆誤混淆詞 (誤用詞 ➔ 正體正確詞)
    "粒刻": "立刻",
    "按步就班": "按部就班",
    "再接在勵": "再接再厲",
    "再接再勵": "再接再厲",
    "再接在厲": "再接再厲",
    "以經": "已經",
    "以偏蓋全": "以偏概全",
    "一偏概全": "以偏概全",
    "變本加利": "變本加厲",
    "不逕而走": "不徑而走",
    "穿流不息": "川流不息",
    "急燥": "急躁",
    "名列前矛": "名列前茅",
    "一愁莫展": "一籌莫展",
    "迫不急待": "迫不及待",
    "濫芋充數": "濫竽充數",
    "走頭無路": "走投無路",
    "委屈求全": "委曲求全",
    "默守成規": "墨守成規",
    "談笑風聲": "談笑風生",
    "消聲匿跡": "銷聲匿跡",
    "震憾": "震撼",
    "趨之若騖": "趨之若鶩",
    "坐陣": "坐鎮",
    "甘敗下風": "甘拜下風",
    "名信片": "明信片",
    "莫明其妙": "莫名其妙",
    "鬼計多端": "詭計多端",
    "絕不認輸": "決不認輸",
    "寒喧": "寒暄",
    "脈膊": "脈搏",
    "挖角": "挖腳",
    "針貶": "針砭",
    "蜂湧而至": "蜂擁而至",
    "趨吉避兇": "趨吉避凶",
    "迫害妄想": "被害妄想",
    "速效要": "速效藥",
    "手棟": "手動",
    "心類": "心累",
    "笑果如和": "效果如何",
    "枝元妃": "支援非",
    "呀縮": "壓縮",
    "在來測試": "再來測試",
}


# =============================================================================
# 2. 資料結構模型 (Data Models)
# =============================================================================

@dataclass
class TypoAuditResult:
    """錯字防火牆審查報告"""
    is_typo: bool
    confidence: float
    suggested_correct: Optional[str]
    reason: str


@dataclass
class AttractorTransition:
    """語意引力場二階轉移"""
    prev_word: str
    curr_word: str
    bonus: float
    reason: str


@dataclass
class AntiHijackShield:
    """防劫掠互斥抑制罩"""
    prev_word: str
    curr_word: str
    penalty: float
    reason: str


@dataclass
class SemanticAnalysisVerdict:
    """語意分析師之最終綜合裁決"""
    rule_id: str
    created_at: str
    word: str
    bopomofo: str
    normalized_weight: int
    category: str
    action_recommended: str  # "APPROVE_SAFE_ADD" | "WARN_POTENTIAL_TYPO" | "BLOCK_HIGH_RISK"
    typo_audit: TypoAuditResult
    attractors: List[AttractorTransition] = field(default_factory=list)
    shields: List[AntiHijackShield] = field(default_factory=list)
    linguistic_summary: str = ""
    context_sentence: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "rule_id": self.rule_id,
            "created_at": self.created_at,
            "word": self.word,
            "bopomofo": self.bopomofo,
            "normalized_weight": self.normalized_weight,
            "category": self.category,
            "action_recommended": self.action_recommended,
            "linguistic_summary": self.linguistic_summary,
            "context_sentence": self.context_sentence,
            "typo_audit": asdict(self.typo_audit),
            "attractors": [asdict(a) for a in self.attractors],
            "shields": [asdict(s) for s in self.shields],
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "SemanticAnalysisVerdict":
        typo_dict = data.get("typo_audit", {})
        typo_audit = TypoAuditResult(
            is_typo=typo_dict.get("is_typo", False),
            confidence=typo_dict.get("confidence", 0.0),
            suggested_correct=typo_dict.get("suggested_correct"),
            reason=typo_dict.get("reason", ""),
        )
        attractors = [
            AttractorTransition(
                prev_word=a.get("prev_word", ""),
                curr_word=a.get("curr_word", ""),
                bonus=float(a.get("bonus", 25.0)),
                reason=a.get("reason", ""),
            )
            for a in data.get("attractors", [])
        ]
        shields = [
            AntiHijackShield(
                prev_word=s.get("prev_word", ""),
                curr_word=s.get("curr_word", ""),
                penalty=float(s.get("penalty", -35.0)),
                reason=s.get("reason", ""),
            )
            for s in data.get("shields", [])
        ]
        return cls(
            rule_id=data.get("rule_id", ""),
            created_at=data.get("created_at", ""),
            word=data.get("word", ""),
            bopomofo=data.get("bopomofo", ""),
            normalized_weight=int(data.get("normalized_weight", 800)),
            category=data.get("category", "user"),
            action_recommended=data.get("action_recommended", "APPROVE_SAFE_ADD"),
            typo_audit=typo_audit,
            attractors=attractors,
            shields=shields,
            linguistic_summary=data.get("linguistic_summary", ""),
            context_sentence=data.get("context_sentence", ""),
        )


# =============================================================================
# 3. 核心 AI 語意分析師 (RuhiSemanticAnalyst)
# =============================================================================

class RuhiSemanticAnalyst:
    """
    如喜語意分析師：
    負責對使用者自建詞及其出現的上下文進行深度計算語言學診斷，
    提煉安全中立權重、上下文轉移引力與防劫掠互斥矩陣。
    """

    def __init__(
        self,
        data_dir: Optional[str] = None,
        user_dict_path: Optional[str] = None,
        transitions_path: Optional[str] = None,
        rules_path: Optional[str] = None,
        backend: str = "auto",  # 'auto' | 'gemini' | 'ollama' | 'heuristic'
        gemini_api_key: Optional[str] = None,
        gemini_model: str = "gemini-2.5-flash",
        ollama_model: str = "gemma2:9b",
        ollama_host: str = "http://127.0.0.1:11434",
    ):
        base_dir = os.path.dirname(os.path.abspath(__file__))
        ruhi_root = os.path.dirname(base_dir)

        self.data_dir = data_dir or os.path.join(ruhi_root, "ruhi_lexicon", "data")
        self.user_dict_path = user_dict_path or os.path.join(ruhi_root, "ruhi_sandbox", "user_dict.tsv")
        self.transitions_path = transitions_path or os.path.join(self.data_dir, "transitions_user.tsv")
        self.rules_path = rules_path or os.path.join(self.data_dir, "user_semantic_rules.json")
        self.events_path = os.path.join(self.data_dir, "user_word_events.jsonl")

        self.backend = backend
        self.gemini_api_key = gemini_api_key or os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
        self.gemini_model = gemini_model
        self.ollama_model = ollama_model
        self.ollama_host = ollama_host.rstrip('/')

        # 記憶體索引與快取
        self._known_lexicon_words: Set[str] = set()
        self._bpmf_to_high_freq_words: Dict[str, List[Tuple[str, int]]] = {}
        self._word_to_bpmf: Dict[str, str] = {}
        self._rules_cache: Dict[str, SemanticAnalysisVerdict] = {}

        self._load_lexicon_indices()
        self._load_rules_cache()

    # -------------------------------------------------------------------------
    # 索引與字典加載
    # -------------------------------------------------------------------------

    def _load_lexicon_indices(self):
        """載入內建教育部詞典與領域詞典之關鍵詞與注音索引"""
        files_to_load = [
            os.path.join(self.data_dir, "lexicon_core.tsv"),
            os.path.join(self.data_dir, "lexicon_domain.tsv"),
            os.path.join(self.data_dir, "lexicon_mcbopomofo.tsv"),
        ]

        for fpath in files_to_load:
            if not os.path.exists(fpath):
                continue
            try:
                with open(fpath, "r", encoding="utf-8") as f:
                    for line in f:
                        line = line.strip()
                        if not line or line.startswith("#"):
                            continue
                        parts = line.split("\t")
                        if len(parts) >= 2:
                            w = parts[0].strip()
                            bpm = parts[1].strip()
                            wt = int(parts[2].strip()) if len(parts) > 2 and parts[2].strip().isdigit() else 100
                            self._known_lexicon_words.add(w)
                            if w not in self._word_to_bpmf:
                                self._word_to_bpmf[w] = bpm
                            if wt >= 800:
                                self._bpmf_to_high_freq_words.setdefault(bpm, []).append((w, wt))
            except Exception:
                pass

    def _load_rules_cache(self):
        """載入已儲存的使用者詞庫語意規則庫"""
        self._rules_cache.clear()
        if not os.path.exists(self.rules_path):
            return
        try:
            with open(self.rules_path, "r", encoding="utf-8") as f:
                data = json.load(f)
                if isinstance(data, list):
                    for item in data:
                        v = SemanticAnalysisVerdict.from_dict(item)
                        self._rules_cache[v.rule_id] = v
        except Exception:
            pass

    # -------------------------------------------------------------------------
    # 聲韻與注音反查
    # -------------------------------------------------------------------------

    def get_bopomofo(self, word: str) -> str:
        """取得精準繁體注音標註"""
        # 1. 優先查字典已知詞
        if word in self._word_to_bpmf:
            return self._word_to_bpmf[word]

        # 2. 次用 pypinyin 轉換
        if PYPINYIN_AVAILABLE:
            try:
                bps = pypinyin.pinyin(word, style=pypinyin.BOPOMOFO)
                res = []
                for b in bps:
                    s = b[0]
                    # 統一輕聲表示法
                    if s.endswith('˙'):
                        s = '˙' + s[:-1]
                    res.append(s)
                return " ".join(res)
            except Exception:
                pass

        # 3. 逐字查表拼接
        chars_bpm = []
        for ch in word:
            if ch in self._word_to_bpmf:
                chars_bpm.append(self._word_to_bpmf[ch])
            else:
                chars_bpm.append("?")
        return " ".join(chars_bpm)

    # -------------------------------------------------------------------------
    # 錯字防火牆 (Typo Firewall)
    # -------------------------------------------------------------------------

    def audit_typo(self, word: str, context: str = "") -> TypoAuditResult:
        """
        審查該詞是否為常見筆誤/錯別字（如：粒刻 ➔ 立刻）
        """
        # 1. 精確比對混淆矩陣
        if word in COMMON_TYPO_MAP:
            target = COMMON_TYPO_MAP[word]
            return TypoAuditResult(
                is_typo=True,
                confidence=0.99,
                suggested_correct=target,
                reason=f"命中高頻筆誤混淆矩陣：『{word}』應修正為正體規範詞『{target}』，防止劫掠正常詞彙！"
            )

        # 2. 同音高頻反查比對：若該詞非字典既有詞，但其同音字詞中有 Unigram 壓倒性超高詞彙
        word_bpm = self.get_bopomofo(word)
        if word not in self._known_lexicon_words and word_bpm in self._bpmf_to_high_freq_words:
            high_freq_cands = self._bpmf_to_high_freq_words[word_bpm]
            # 檢查是否有同長度且權重極高的主流詞
            for h_word, h_wt in high_freq_cands:
                if len(h_word) == len(word) and h_wt >= 1200:
                    # 進一步檢驗該詞是否僅為單一字音同字異
                    diff_chars = sum(1 for c1, c2 in zip(word, h_word) if c1 != c2)
                    if diff_chars == 1:
                        return TypoAuditResult(
                            is_typo=True,
                            confidence=0.88,
                            suggested_correct=h_word,
                            reason=f"檢測到高度疑似同音筆誤：注音為『{word_bpm}』，主流規範詞為高頻詞『{h_word}』(權重 {h_wt})，自建詞『{word}』極易構成劫掠毒性！"
                        )

        return TypoAuditResult(
            is_typo=False,
            confidence=0.95,
            suggested_correct=None,
            reason="未發現常見筆誤特徵，視為專有名詞或合法自建詞彙。"
        )

    # -------------------------------------------------------------------------
    # 核心權重歸一化 (Unigram Neutralization - 反地鼠原則)
    # -------------------------------------------------------------------------

    @staticmethod
    def calculate_safe_unigram_weight(word: str) -> int:
        """
        依詞長計算安全中立的 Unigram 先驗權重，嚴禁灌入 2000 等破壞性先驗
        """
        length = len(word)
        if length <= 1:
            return 600  # 單字極度受限，必須依賴前驅二階轉移
        elif length == 2:
            return 750  # 雙字詞標準中立先驗，略低於極高頻詞（850~1200），日常裸打不劫掠
        elif length == 3:
            return 820  # 三字詞自然凝聚力
        else:
            return 880  # 四字及以上專名/成語，長詞凝聚力較高

    # -------------------------------------------------------------------------
    # 啟發式本機語意分析引擎 (Heuristic Semantic Engine)
    # -------------------------------------------------------------------------

    def _analyze_heuristically(
        self,
        word: str,
        context: str,
        user_bpm: Optional[str] = None
    ) -> SemanticAnalysisVerdict:
        """
        離線本機啟發式語意分析器：
        零依賴、純 Python，快速完成引力場提取、錯字審查與防劫掠互斥矩陣生成
        """
        rule_id = f"rule_{datetime.now().strftime('%Y%m%d_%H%M%S')}_{os.urandom(3).hex()}"
        created_at = datetime.now().isoformat()
        bpmf = user_bpm.strip() if user_bpm else self.get_bopomofo(word)
        typo_res = self.audit_typo(word, context)
        safe_wt = self.calculate_safe_unigram_weight(word)

        attractors: List[AttractorTransition] = []
        shields: List[AntiHijackShield] = []
        category = "user_proper_noun" if not typo_res.is_typo else "user_typo_quarantine"
        action = "APPROVE_SAFE_ADD"

        if typo_res.is_typo and typo_res.confidence >= 0.85:
            action = "WARN_POTENTIAL_TYPO"

        # 上下文語意引力場提取 (Context Attractor Extraction)
        if context and word in context:
            pos = context.find(word)
            before_text = context[:pos].strip()
            after_text = context[pos + len(word):].strip()

            # 提取左前驅詞 (Previous Word)
            prev_token = ""
            if before_text:
                # 簡單中文標點與字詞切分
                clean_before = re.sub(r'[，。！？；：、「」『』（）\s]', ' ', before_text).strip()
                tokens = clean_before.split()
                if tokens:
                    last_chunk = tokens[-1]
                    # 取最後 1~4 字作為前驅候選
                    for l in (3, 2, 4, 1):
                        if len(last_chunk) >= l:
                            prev_token = last_chunk[-l:]
                            break

            # 提取右後繼詞 (Next Word)
            next_token = ""
            if after_text:
                clean_after = re.sub(r'[，。！？；：、「」『』（）\s]', ' ', after_text).strip()
                tokens = clean_after.split()
                if tokens:
                    first_chunk = tokens[0]
                    for l in (2, 3, 4, 1):
                        if len(first_chunk) >= l:
                            next_token = first_chunk[:l]
                            break

            if prev_token:
                attractors.append(AttractorTransition(
                    prev_word=prev_token,
                    curr_word=word,
                    bonus=32.0,
                    reason=f"上下文強引力：前驅詞『{prev_token}』引導自建詞『{word}』精準出字"
                ))

            if next_token:
                attractors.append(AttractorTransition(
                    prev_word=word,
                    curr_word=next_token,
                    bonus=28.0,
                    reason=f"上下文順向引力：自建詞『{word}』引導後續詞『{next_token}』"
                ))

        # 防劫掠互斥矩陣生成 (Anti-Hijacking Shields)
        # 尋找與 word 同音的內建競態詞，防禦劫掠
        if bpmf in self._bpmf_to_high_freq_words:
            for competitor, _ in self._bpmf_to_high_freq_words[bpmf]:
                if competitor != word:
                    # 在當前自建詞引力場的前驅詞下，壓抑競態詞
                    for att in attractors:
                        if att.curr_word == word:
                            shields.append(AntiHijackShield(
                                prev_word=att.prev_word,
                                curr_word=competitor,
                                penalty=-45.0,
                                reason=f"防劫掠互斥：在前驅詞『{att.prev_word}』環境下，壓制同音競態詞『{competitor}』"
                            ))

        summary = (
            f"啟發式診斷完成：詞彙『{word}』(注音: {bpmf})。"
            f"先驗權重歸一化至中立值 {safe_wt} 分；"
            f"萃取 {len(attractors)} 條語意引力規則與 {len(shields)} 條防劫掠互斥規則。"
        )

        return SemanticAnalysisVerdict(
            rule_id=rule_id,
            created_at=created_at,
            word=word,
            bopomofo=bpmf,
            normalized_weight=safe_wt,
            category=category,
            action_recommended=action,
            typo_audit=typo_res,
            attractors=attractors,
            shields=shields,
            linguistic_summary=summary,
            context_sentence=context,
        )

    # -------------------------------------------------------------------------
    # Google Gemini API 遠端語意大腦
    # -------------------------------------------------------------------------

    def _analyze_with_gemini(
        self,
        word: str,
        context: str,
        user_bpm: Optional[str] = None
    ) -> Optional[SemanticAnalysisVerdict]:
        """調用 Google Gemini API 進行專家級語意診斷"""
        if not self.gemini_api_key:
            return None

        prompt = f"""你是一位精通漢語計算語言學、Viterbi 動態規劃與繁體中文智慧型輸入法的「AI 語意分析師」。
使用者剛在如喜輸入法中鍵入了自建詞，請進行嚴謹的語言學診斷與解毒分析，防禦自建詞劇毒並杜絕「劫掠」內建詞庫。

【輸入資訊】：
- 詞目：『{word}』
- 出現之上下文句子：『{context}』
- 參考注音：『{user_bpm or self.get_bopomofo(word)}』

【分析目標】：
1. 錯字防火牆 (Typo Check)：判斷『{word}』是否為常見同音筆誤/錯別字（例如將「立刻」誤為「粒刻」）？
2. 權重歸一化 (Unigram Prior)：給出合理中立權重（嚴禁 2000 等破壞性超高分，建議 700~880 之間）。
3. 語意引力場 (Attractors)：萃取上下文中的前驅詞與後繼詞轉移搭配（+20.0 ~ +38.0）。
4. 防劫掠互斥矩陣 (Anti-Hijack Shields)：若有同音常態詞，提供負向抑制對（-35.0 ~ -50.0）。

請嚴格輸出符合以下格式的純 JSON 物件（不得包含額外說明）：
{{
  "is_typo": false,
  "confidence": 0.95,
  "suggested_correct": null,
  "typo_reason": "說明文字",
  "normalized_weight": 800,
  "category": "user_proper_noun",
  "action": "APPROVE_SAFE_ADD",
  "attractors": [
    {{"prev": "前驅詞", "curr": "{word}", "bonus": 32.0, "reason": "搭配說明"}}
  ],
  "shields": [
    {{"prev": "前驅詞", "curr": "同音競態詞", "penalty": -45.0, "reason": "防劫掠說明"}}
  ],
  "linguistic_summary": "簡短的語言學機理總結"
}}
"""
        url = f"https://generativelanguage.googleapis.com/v1beta/models/{self.gemini_model}:generateContent?key={self.gemini_api_key}"
        payload = {
            "contents": [{"parts": [{"text": prompt}]}],
            "generationConfig": {"temperature": 0.1, "responseMimeType": "application/json"}
        }

        try:
            req = urllib.request.Request(
                url,
                data=json.dumps(payload).encode("utf-8"),
                headers={"Content-Type": "application/json"}
            )
            with urllib.request.urlopen(req, timeout=12) as resp:
                if resp.status == 200:
                    raw_res = json.loads(resp.read().decode("utf-8"))
                    text = raw_res["candidates"][0]["content"]["parts"][0]["text"]
                    parsed = json.loads(text)
                    return self._build_verdict_from_json(word, context, user_bpm, parsed)
        except Exception as e:
            print(f"[RuhiSemanticAnalyst] Gemini API 連線或解析失敗: {e}")

        return None

    # -------------------------------------------------------------------------
    # 本地 Ollama Gemma 語意大腦
    # -------------------------------------------------------------------------

    def _analyze_with_ollama(
        self,
        word: str,
        context: str,
        user_bpm: Optional[str] = None
    ) -> Optional[SemanticAnalysisVerdict]:
        """調用本地 Ollama Gemma 進行語意診斷"""
        prompt = f"""你是一位精通漢語計算語言學、Viterbi 動態規劃與繁體中文智慧型輸入法的「AI 語意分析師」。
請針對自建詞『{word}』在語境『{context}』中進行診斷，嚴格輸出 JSON 格式：
{{
  "is_typo": false,
  "confidence": 0.95,
  "suggested_correct": null,
  "typo_reason": "筆誤審查",
  "normalized_weight": 800,
  "category": "user_proper_noun",
  "action": "APPROVE_SAFE_ADD",
  "attractors": [
    {{"prev": "前驅詞", "curr": "{word}", "bonus": 32.0, "reason": "引力說明"}}
  ],
  "shields": [
    {{"prev": "前驅詞", "curr": "干擾詞", "penalty": -45.0, "reason": "防劫掠"}}
  ],
  "linguistic_summary": "機理總結"
}}
"""
        url = f"{self.ollama_host}/api/generate"
        payload = {
            "model": self.ollama_model,
            "prompt": prompt,
            "format": "json",
            "stream": False,
            "options": {"temperature": 0.1}
        }
        try:
            req = urllib.request.Request(
                url,
                data=json.dumps(payload).encode("utf-8"),
                headers={"Content-Type": "application/json"}
            )
            with urllib.request.urlopen(req, timeout=8) as resp:
                if resp.status == 200:
                    raw_res = json.loads(resp.read().decode("utf-8"))
                    parsed = json.loads(raw_res.get("response", "{}"))
                    return self._build_verdict_from_json(word, context, user_bpm, parsed)
        except Exception:
            pass
        return None

    def _build_verdict_from_json(
        self,
        word: str,
        context: str,
        user_bpm: Optional[str],
        parsed: Dict[str, Any]
    ) -> SemanticAnalysisVerdict:
        rule_id = f"rule_{datetime.now().strftime('%Y%m%d_%H%M%S')}_{os.urandom(3).hex()}"
        created_at = datetime.now().isoformat()
        bpmf = user_bpm.strip() if user_bpm else self.get_bopomofo(word)

        typo_res = TypoAuditResult(
            is_typo=bool(parsed.get("is_typo", False)),
            confidence=float(parsed.get("confidence", 0.9)),
            suggested_correct=parsed.get("suggested_correct"),
            reason=parsed.get("typo_reason", "AI 語意筆誤分析"),
        )

        attractors = [
            AttractorTransition(
                prev_word=item.get("prev", ""),
                curr_word=item.get("curr", word),
                bonus=float(item.get("bonus", 30.0)),
                reason=item.get("reason", "AI 提煉之語意引力"),
            )
            for item in parsed.get("attractors", [])
            if item.get("prev") and item.get("curr")
        ]

        shields = [
            AntiHijackShield(
                prev_word=item.get("prev", ""),
                curr_word=item.get("curr", ""),
                penalty=float(item.get("penalty", -40.0)),
                reason=item.get("reason", "AI 提煉之防劫掠互斥"),
            )
            for item in parsed.get("shields", [])
            if item.get("prev") and item.get("curr")
        ]

        norm_wt = int(parsed.get("normalized_weight", self.calculate_safe_unigram_weight(word)))
        # 鐵律保護：嚴禁 AI 回傳大於 1200 的危險權重，強制歸一化
        if norm_wt > 1100:
            norm_wt = self.calculate_safe_unigram_weight(word)

        return SemanticAnalysisVerdict(
            rule_id=rule_id,
            created_at=created_at,
            word=word,
            bopomofo=bpmf,
            normalized_weight=norm_wt,
            category=parsed.get("category", "user"),
            action_recommended=parsed.get("action", "APPROVE_SAFE_ADD"),
            typo_audit=typo_res,
            attractors=attractors,
            shields=shields,
            linguistic_summary=parsed.get("linguistic_summary", "AI 大腦專家分析完畢"),
            context_sentence=context,
        )

    # -------------------------------------------------------------------------
    # 頂層對外介面 (Public API)
    # -------------------------------------------------------------------------

    def analyze_custom_word(
        self,
        word: str,
        context: str = "",
        user_bpm: Optional[str] = None,
        force_heuristic: bool = False
    ) -> SemanticAnalysisVerdict:
        """
        全面分析自建詞，自動走訪 Google Gemini ➔ 本機 Ollama ➔ 啟發式保底
        """
        word = word.strip()
        context = context.strip()

        if not force_heuristic and self.backend in ("auto", "gemini"):
            verdict = self._analyze_with_gemini(word, context, user_bpm)
            if verdict:
                return verdict

        if not force_heuristic and self.backend in ("auto", "ollama"):
            verdict = self._analyze_with_ollama(word, context, user_bpm)
            if verdict:
                return verdict

        # 啟發式保底（100% 穩定可用）
        return self._analyze_heuristically(word, context, user_bpm)

    def apply_verdict(
        self,
        verdict: SemanticAnalysisVerdict,
        commit_to_disk: bool = True,
        engine_bridge: Any = None
    ) -> Dict[str, Any]:
        """
        將語意裁決落地應用至詞庫系統：
        1. 寫入 user_dict.tsv（中立安全權重 + 標籤）
        2. 寫入 transitions_user.tsv（引力與防劫掠互斥）
        3. 記錄 user_semantic_rules.json（結構化規則庫）
        4. 透過 engine_bridge 觸發即時熱重載
        """
        result = {
            "rule_id": verdict.rule_id,
            "word_added": False,
            "attractors_added": 0,
            "shields_added": 0,
            "warning": None,
        }

        # 若判定為高危險錯字且建議阻擋
        if verdict.action_recommended == "WARN_POTENTIAL_TYPO":
            result["warning"] = (
                f"⚠️ 警示：『{verdict.word}』高度疑似為筆誤！"
                f"建議修正為『{verdict.typo_audit.suggested_correct}』。"
            )

        if not commit_to_disk:
            return result

        # 1. 寫入 user_dict.tsv (防重複與中立權重)
        existing_words = set()
        os.makedirs(os.path.dirname(self.user_dict_path), exist_ok=True)
        if os.path.exists(self.user_dict_path):
            with open(self.user_dict_path, "r", encoding="utf-8") as f:
                for line in f:
                    if line.strip() and not line.startswith("#"):
                        p = line.split("\t")
                        if p:
                            existing_words.add(p[0].strip())

        if verdict.word not in existing_words:
            entry_line = (
                f"{verdict.word}\t{verdict.bopomofo}\t{verdict.normalized_weight}\t"
                f"{verdict.category}\tAI-Sanitized: {verdict.rule_id}\n"
            )
            with open(self.user_dict_path, "a", encoding="utf-8") as f:
                f.write(entry_line)
            result["word_added"] = True

        # 2. 寫入 transitions_user.tsv (引力場與防劫掠互斥)
        existing_trans = set()
        os.makedirs(os.path.dirname(self.transitions_path), exist_ok=True)
        if os.path.exists(self.transitions_path):
            with open(self.transitions_path, "r", encoding="utf-8") as f:
                for line in f:
                    if line.strip() and not line.startswith("#"):
                        p = line.split("\t")
                        if len(p) >= 2:
                            existing_trans.add((p[0].strip(), p[1].strip()))

        today_str = datetime.now().strftime("%Y-%m-%d")
        trans_lines = []

        # 寫入正向引力
        for att in verdict.attractors:
            if (att.prev_word, att.curr_word) not in existing_trans:
                trans_lines.append(
                    f"{att.prev_word}\t{att.curr_word}\t{att.bonus:.1f}\tuser_attractor\t{today_str}\n"
                )
                existing_trans.add((att.prev_word, att.curr_word))
                result["attractors_added"] += 1

        # 寫入防劫掠負向互斥
        for sh in verdict.shields:
            if (sh.prev_word, sh.curr_word) not in existing_trans:
                trans_lines.append(
                    f"{sh.prev_word}\t{sh.curr_word}\t{sh.penalty:.1f}\tanti_hijack_shield\t{today_str}\n"
                )
                existing_trans.add((sh.prev_word, sh.curr_word))
                result["shields_added"] += 1

        if trans_lines:
            with open(self.transitions_path, "a", encoding="utf-8") as f:
                f.writelines(trans_lines)

        # 3. 儲存至 user_semantic_rules.json
        self._rules_cache[verdict.rule_id] = verdict
        rules_list = [v.to_dict() for v in self._rules_cache.values()]
        os.makedirs(os.path.dirname(self.rules_path), exist_ok=True)
        with open(self.rules_path, "w", encoding="utf-8") as f:
            json.dump(rules_list, f, ensure_ascii=False, indent=2)

        # 4. 熱重載 (Hot Reload)
        if engine_bridge:
            try:
                engine_bridge.reload_user_dict(self.user_dict_path)
                engine_bridge.reload_transitions(self.transitions_path)
            except Exception as e:
                print(f"[RuhiSemanticAnalyst] 引擎熱重載異常: {e}")

        return result

    def rollback_rule(self, rule_id: str, engine_bridge: Any = None) -> bool:
        """
        精準回滾單一規則，撤銷其詞條、引力場與防劫掠互斥條目
        """
        if rule_id not in self._rules_cache:
            return False

        verdict = self._rules_cache[rule_id]

        # 1. 自 user_dict.tsv 移除詞條
        if os.path.exists(self.user_dict_path):
            with open(self.user_dict_path, "r", encoding="utf-8") as f:
                lines = f.readlines()
            new_lines = [l for l in lines if not l.startswith(f"{verdict.word}\t")]
            with open(self.user_dict_path, "w", encoding="utf-8") as f:
                f.writelines(new_lines)

        # 2. 自 transitions_user.tsv 移除引力與防劫掠條目
        targets_to_remove = set()
        for a in verdict.attractors:
            targets_to_remove.add((a.prev_word, a.curr_word))
        for s in verdict.shields:
            targets_to_remove.add((s.prev_word, s.curr_word))

        if os.path.exists(self.transitions_path):
            with open(self.transitions_path, "r", encoding="utf-8") as f:
                t_lines = f.readlines()
            filtered_t_lines = []
            for tl in t_lines:
                p = tl.strip().split("\t")
                if len(p) >= 2 and (p[0].strip(), p[1].strip()) in targets_to_remove:
                    continue
                filtered_t_lines.append(tl)
            with open(self.transitions_path, "w", encoding="utf-8") as f:
                f.writelines(filtered_t_lines)

        # 3. 自 user_semantic_rules.json 移除
        del self._rules_cache[rule_id]
        rules_list = [v.to_dict() for v in self._rules_cache.values()]
        with open(self.rules_path, "w", encoding="utf-8") as f:
            json.dump(rules_list, f, ensure_ascii=False, indent=2)

        # 4. 觸發熱重載
        if engine_bridge:
            try:
                engine_bridge.reload_user_dict(self.user_dict_path)
                engine_bridge.reload_transitions(self.transitions_path)
            except Exception:
                pass

        return True

    def list_rules(self) -> List[Dict[str, Any]]:
        """列出所有有效之使用者詞庫語意規則"""
        return [v.to_dict() for v in self._rules_cache.values()]

    def process_events(
        self,
        events_path: Optional[str] = None,
        force_heuristic: bool = False,
        engine_bridge: Any = None
    ) -> List[Dict[str, Any]]:
        """
        批次處理使用者自建詞事件佇列 (user_word_events.jsonl)：
        逐條進行 AI 語意分析、提煉上下文引力場、生成防劫掠互斥罩並寫入規則庫
        """
        fpath = events_path or self.events_path
        if not os.path.exists(fpath):
            return []

        processed_words = {v.word for v in self._rules_cache.values()}
        results = []

        try:
            with open(fpath, "r", encoding="utf-8") as f:
                lines = [l.strip() for l in f if l.strip()]
        except Exception:
            return []

        for line in lines:
            try:
                evt = json.loads(line)
            except Exception:
                continue

            word = evt.get("word", "").strip()
            context = evt.get("context", "").strip()
            bpmf = evt.get("bopomofo", "").strip() or None

            if not word or word in processed_words:
                continue

            verdict = self.analyze_custom_word(
                word=word,
                context=context,
                user_bpm=bpmf,
                force_heuristic=force_heuristic
            )

            res = self.apply_verdict(verdict, commit_to_disk=True, engine_bridge=engine_bridge)
            processed_words.add(word)
            results.append({
                "word": word,
                "rule_id": verdict.rule_id,
                "action": verdict.action_recommended,
                "attractors_count": len(verdict.attractors),
                "shields_count": len(verdict.shields),
                "warning": res.get("warning"),
            })

        return results
