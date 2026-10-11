#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/smart_composer.py
=============================================================================
如喜靈犀流式補全引擎 (RuhI Smart Compose & Head-Code Engine)
=============================================================================
功能：
1. 頭碼極速躍遷 (Head-code Rapid Leap)：
   支援聲母/頭碼縮寫即時反查（例如：ㄉㄖㄓ ➔ 砥睿者、ㄌㄅㄕ ➔ 邏霸式、ㄗㄔㄓㄌ ➔ 自串珠鍊）。
2. 靈犀流式接續建議 (Smart Compose Streaming Suggestion)：
   依據 31,000+ 條雙向語意引力轉移矩陣，預測接續最自然的 4~12 字高階片語，
   於組字區以淺灰色浮現，支援 Tab 鍵一秒採納。
=============================================================================
"""

import os
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except Exception:
    pass


class SmartComposer:
    """靈犀流式補全與頭碼預測引擎"""

    def __init__(self, data_dir: Optional[str] = None):
        if data_dir is None:
            curr = Path(__file__).resolve().parent
            data_dir = str(curr.parent / "ruhi_lexicon" / "data")
        self.data_dir = Path(data_dir)

        # 轉移矩陣圖：prev -> [(weight, next_token), ...] 按權重降序
        self.forward_graph: Dict[str, List[Tuple[float, str]]] = {}
        # 頭碼索引：headcode -> [word, ...]
        self.headcode_map: Dict[str, List[str]] = {}
        # 詞庫反查
        self.domain_words: Dict[str, float] = {}

        self._load_data()

    def _load_data(self):
        """載入領域辭庫與語意轉移矩陣"""
        trans_file = self.data_dir / "transitions_user.tsv"
        domain_file = self.data_dir / "lexicon_domain.tsv"
        moe_file = self.data_dir / "lexicon_moe.tsv"

        # 1. 載入轉移矩陣
        if trans_file.exists():
            with open(trans_file, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split("\t")
                    if len(parts) >= 3:
                        prev = parts[0]
                        curr = parts[1]
                        try:
                            weight = float(parts[2])
                        except ValueError:
                            continue
                        # 僅收錄正向高置信度引力 (>= 75.0)
                        if weight >= 75.0:
                            if prev not in self.forward_graph:
                                self.forward_graph[prev] = []
                            self.forward_graph[prev].append((weight, curr))

            # 依權重排序
            for k in self.forward_graph:
                self.forward_graph[k].sort(key=lambda x: -x[0])

        # 2. 載入領域與教育部詞庫以建構頭碼索引
        def extract_initials(bpmf_str: str) -> str:
            initials = []
            for syll in bpmf_str.split():
                s = syll.strip()
                if not s:
                    continue
                c0 = s[0]
                if c0 in "ㄅㄆㄇㄈㄉㄊㄋㄌㄍㄎㄏㄐㄑㄒㄓㄔㄕㄖㄗㄘㄙ":
                    initials.append(c0)
            return "".join(initials)

        files_to_index = [domain_file, moe_file]
        for fpath in files_to_index:
            if not fpath.exists():
                continue
            with open(fpath, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split("\t")
                    if len(parts) >= 2:
                        word = parts[0]
                        bpmf = parts[1]
                        wt = float(parts[2]) if len(parts) >= 3 and parts[2].replace(".", "", 1).isdigit() else 500.0
                        self.domain_words[word] = max(self.domain_words.get(word, 0.0), wt)

                        # 建構頭碼
                        hc = extract_initials(bpmf)
                        if len(hc) >= 2:
                            if hc not in self.headcode_map:
                                self.headcode_map[hc] = []
                            if word not in self.headcode_map[hc]:
                                self.headcode_map[hc].append(word)

    HSU_INITIAL_MAP = {
        'b': 'ㄅ', 'p': 'ㄆ', 'm': 'ㄇ', 'f': 'ㄈ',
        'd': 'ㄉ', 't': 'ㄊ', 'n': 'ㄋ', 'l': 'ㄌ',
        'g': 'ㄍ', 'k': 'ㄎ', 'h': 'ㄏ',
        'j': 'ㄓ', 'v': 'ㄔ', 'c': 'ㄕ',
        'r': 'ㄖ', 'z': 'ㄗ', 'a': 'ㄘ', 's': 'ㄙ',
    }

    def query_headcode(self, headcode: str, top_k: int = 8) -> List[str]:
        """
        查詢頭碼縮寫候選詞（支援注音聲母縮寫或許氏英文字母縮寫）
        例如：'ㄉㄖㄓ' 或 'drj' -> ['砥睿者']
             'ㄌㄅㄕ' 或 'lbc' -> ['邏霸式']
             'ㄗㄔㄓㄌ' 或 'zvjl' -> ['自串珠鍊']
        """
        hc = headcode.strip()
        # 若傳入為許氏英文字母，自動映射轉換為注音聲母
        if hc.isascii() and hc.isalpha():
            hc = "".join(self.HSU_INITIAL_MAP.get(c.lower(), "") for c in hc)

        candidates = self.headcode_map.get(hc, [])
        # 依詞頻與長度排序
        sorted_cands = sorted(candidates, key=lambda w: self.domain_words.get(w, 0.0), reverse=True)
        return sorted_cands[:top_k]

    def predict_continuation(self, text_context: str, max_words: int = 3, max_chars: int = 15) -> str:
        """
        依據當前輸入上下文，沿轉移矩陣路徑預測最自然的接續補全片語 (Smart Compose)
        """
        if not text_context:
            return ""

        suggestion_parts = []
        curr_ctx = text_context[-4:]  # 最長取末尾 4 字

        for _ in range(max_words):
            next_token = None
            # 優先嘗試 4字、3字、2字 前綴匹配
            for l in range(min(len(curr_ctx), 4), 1, -1):
                sub = curr_ctx[-l:]
                if sub in self.forward_graph and self.forward_graph[sub]:
                    # 取最高權重之後續詞
                    cand = self.forward_graph[sub][0][1]
                    # 避免環形重複
                    if cand not in suggestion_parts and cand != sub:
                        next_token = cand
                        break

            if not next_token:
                break

            suggestion_parts.append(next_token)
            curr_ctx = next_token
            if sum(len(p) for p in suggestion_parts) >= max_chars:
                break

        full_suggestion = "".join(suggestion_parts)
        return full_suggestion


if __name__ == "__main__":
    composer = SmartComposer()
    print("=== 頭碼測試 ===")
    for hc in ["ㄉㄖㄓ", "ㄌㄅㄕ", "ㄗㄔㄓㄌ", "ㄓㄏㄇㄍ", "ㄎㄒㄓㄕ"]:
        print(f"頭碼 [{hc}]: {composer.query_headcode(hc)}")

    print("\n=== Smart Compose 接續測試 ===")
    contexts = [
        "作為團隊的砥睿者必須",
        "以建構出無懈可擊的",
        "謹此為父",
        "這款頂級金萱茶經",
        "傳統木作工法以精巧的"
    ]
    for ctx in contexts:
        pred = composer.predict_continuation(ctx)
        print(f"輸入: 「{ctx}」 ➔ 建議接續: 「{pred}」")
