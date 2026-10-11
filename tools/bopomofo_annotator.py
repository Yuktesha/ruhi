#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
bopomofo_annotator.py — 本體專名與詞彙全自動精準注音標註器
=============================================================================
支援：
1. 全詞精準比對 (Exact Word Match from MOE & McBopomofo)
2. 最大正向匹配多字子詞拼接 (Max-Forward Subword Segmentation)
3. 梵文與佛學/靈性專名領域特化注音 (Sanskrit Domain Overrides, 如：三摩地 ㄙㄢ ㄇㄛˊ ㄉㄧˋ、阿南達 ㄚ ㄋㄢˊ ㄉㄚˊ)
4. 單字最常用讀音保底拼接 (Primary Char Reading Fallback)
"""

import functools
import os
from typing import Dict, List, Optional, Tuple


class BopomofoAnnotator:
    """
    全自動注音標註器
    """

    # 領域特化專用發音（涵蓋梵文、靈性哲學、特定專有名詞音變、破音字校正）
    DOMAIN_OVERRIDES: Dict[str, str] = {
        "阿南達": "ㄚ ㄋㄢˊ ㄉㄚˊ",
        "阿南達迦耶提": "ㄚ ㄋㄢˊ ㄉㄚˊ ㄐㄧㄚ ㄧㄝˊ ㄊㄧˊ",
        "薄伽梵": "ㄅㄛˊ ㄑㄧㄝˊ ㄈㄢˋ",
        "三摩地": "ㄙㄢ ㄇㄛˊ ㄉㄧˋ",
        "薩達迦": "ㄙㄚˋ ㄉㄚˊ ㄐㄧㄚ",
        "至上意識": "ㄓˋ ㄕㄤˋ ㄧˋ ㄕˋ",
        "那達": "ㄋㄚˊ ㄉㄚˊ",
        "摩訶": "ㄇㄛˊ ㄏㄜ",
        "那些": "ㄋㄚˋ ㄒㄧㄝ",
        "最長": "ㄗㄨㄟˋ ㄔㄤˊ",
        "為父": "ㄨㄟˋ ㄈㄨˋ",
    }

    def __init__(self, data_dir: Optional[str] = None):
        if data_dir is None:
            curr_dir = os.path.dirname(os.path.abspath(__file__))
            data_dir = os.path.join(os.path.dirname(curr_dir), "ruhi_lexicon", "data")
        self.data_dir = data_dir

        self.word_to_bpmf: Dict[str, str] = {}
        self.char_to_bpmf: Dict[str, str] = {}
        self._load_dictionaries()

    def _load_dictionaries(self):
        """載入教育部字典與 McBopomofo 詞庫建立發音字典"""
        # 優先載入教育部字典
        files = [
            os.path.join(self.data_dir, "lexicon_moe.tsv"),
            os.path.join(self.data_dir, "lexicon_mcbopomofo.tsv"),
            os.path.join(self.data_dir, "lexicon_core.tsv"),
        ]

        for filepath in files:
            if not os.path.exists(filepath):
                continue
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    for line in f:
                        line = line.strip()
                        if not line or line.startswith("#") or line.startswith("WORD"):
                            continue
                        parts = line.split("\t")
                        if len(parts) >= 2:
                            word, bpmf = parts[0].strip(), parts[1].strip()
                            if not word or not bpmf:
                                continue

                            # 處理輕聲格式：將 . 轉為正規前綴或維持
                            norm_bpmf = bpmf.replace("˙", "˙").strip()

                            # 單字發音
                            if len(word) == 1:
                                if word not in self.char_to_bpmf:
                                    self.char_to_bpmf[word] = norm_bpmf
                            else:
                                if word not in self.word_to_bpmf:
                                    self.word_to_bpmf[word] = norm_bpmf
            except Exception as e:
                print(f"[BopomofoAnnotator] 載入 {filepath} 失敗: {e}")

        # 覆蓋領域特化發音
        for w, bp in self.DOMAIN_OVERRIDES.items():
            self.word_to_bpmf[w] = bp

    @functools.lru_cache(maxsize=30000)
    def annotate(self, text: str) -> str:
        """
        為任意詞彙或短語標註標準臺灣注音（音節以空格分隔）
        """
        text = text.strip()
        if not text:
            return ""

        # 1. 領域特化優先
        if text in self.DOMAIN_OVERRIDES:
            return self.DOMAIN_OVERRIDES[text]

        # 2. 全詞比對
        if text in self.word_to_bpmf:
            return self.word_to_bpmf[text]

        # 3. 最大正向匹配子詞拼接 (Max-Forward Matching)
        i = 0
        n = len(text)
        result_bpmf: List[str] = []

        while i < n:
            matched = False
            # 優先嘗試最長可能子詞（長度從 min(8, n-i) 往下找至 2）
            for length in range(min(8, n - i), 1, -1):
                sub = text[i : i + length]
                if sub in self.word_to_bpmf:
                    result_bpmf.append(self.word_to_bpmf[sub])
                    i += length
                    matched = True
                    break

            if not matched:
                # 單字比對
                char = text[i]
                c_bpmf = self.char_to_bpmf.get(char, "")
                if not c_bpmf:
                    # 預設未知標記
                    c_bpmf = "?"
                result_bpmf.append(c_bpmf)
                i += 1

        return " ".join(result_bpmf).strip()
