#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
text_to_hsu.py — 文章/詞句轉許氏鍵盤雙拼編碼器 (Text-to-Hsu Key Sequence Compiler)
=============================================================================
功能：
1. 結合 BopomofoAnnotator，將任意中文段落自動標註為標準臺灣注音。
2. 依照許氏鍵盤 (Hsu 26-key Layout) 雙拼規則，將注音音節精準編譯為按鍵字串序列。
3. 支援標點符號、數字、英文字元之切分與穿透。
"""

import functools
import re
from typing import List, Tuple, Optional
try:
    from bopomofo_annotator import BopomofoAnnotator
except ImportError:
    from ruhi_sandbox.bopomofo_annotator import BopomofoAnnotator


class TextToHsuCompiler:
    """
    文字轉許氏鍵盤碼編譯器
    """

    # 許氏聲母映射 (Initials)
    INITIAL_MAP = {
        'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
        'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
        'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
        'ㄐ': 'j', 'ㄑ': 'v', 'ㄒ': 'c',
        'ㄓ': 'j', 'ㄔ': 'v', 'ㄕ': 'c', 'ㄖ': 'r',
        'ㄗ': 'z', 'ㄘ': 'a', 'ㄙ': 's'
    }

    # 許氏介母映射 (Medials)
    MEDIAL_MAP = {
        'ㄧ': 'e',
        'ㄨ': 'x',
        'ㄩ': 'u'
    }

    # 許氏韻母映射 (Finals)
    FINAL_MAP = {
        'ㄚ': 'r',
        'ㄛ': 'h',
        'ㄜ': 'g',
        'ㄝ': 'e',
        'ㄞ': 'i',
        'ㄟ': 'a',
        'ㄠ': 'w',
        'ㄡ': 'o',
        'ㄢ': 'm',
        'ㄣ': 'n',
        'ㄤ': 'k',
        'ㄥ': 'l',
        'ㄦ': 'l'
    }

    # 聲調映射 (Tones)
    TONE_MAP = {
        '1': ' ',  # 一聲陰平 (Space)
        '2': 'd',  # 二聲陽平 (ˊ)
        '3': 'f',  # 三聲上聲 (ˇ)
        '4': 'j',  # 四聲去聲 (ˋ)
        '5': 's',  # 五聲輕聲 (˙)
    }

    # 中文與常用標點符號集合
    PUNCTUATIONS = set("，。！？、；：…—「」『』（）《》〈〉“”‘’()[]{},.!?:;\"'-")

    def __init__(self, data_dir: Optional[str] = None):
        self.annotator = BopomofoAnnotator(data_dir=data_dir)

    @functools.lru_cache(maxsize=5000)
    def bpmf_syllable_to_hsu(self, syllable: str) -> str:
        """
        將單一注音音節（如 ㄒㄧㄢˋ、ㄓㄨˋ、˙ㄉㄜ、ㄨㄛˇ、ㄚ）轉換為許氏鍵盤鍵碼
        """
        if not syllable:
            return ""

        s = syllable.strip()
        tone = '1'

        # 1. 提取聲調
        if s.startswith('˙'):
            tone = '5'
            s = s[1:]
        elif s.endswith('˙'):
            tone = '5'
            s = s[:-1]
        elif s.endswith('ˊ'):
            tone = '2'
            s = s[:-1]
        elif s.endswith('ˇ'):
            tone = '3'
            s = s[:-1]
        elif s.endswith('ˋ'):
            tone = '4'
            s = s[:-1]

        initial = ""
        medial = ""
        final = ""

        # 2. 匹配聲母
        if s and s[0] in self.INITIAL_MAP:
            initial = s[0]
            s = s[1:]

        # 3. 匹配介母
        if s and s[0] in self.MEDIAL_MAP:
            medial = s[0]
            s = s[1:]

        # 4. 匹配韻母
        if s:
            final = s

        # 5. 編譯為許氏鍵碼
        keys = []
        if initial:
            keys.append(self.INITIAL_MAP[initial])

        if medial:
            keys.append(self.MEDIAL_MAP[medial])

        if final:
            # 零聲母、零介母之獨立「ㄚ」使用 'y'，其餘使用 'r'
            if not initial and not medial and final == 'ㄚ':
                keys.append('y')
            elif final in self.FINAL_MAP:
                keys.append(self.FINAL_MAP[final])

        # 6. 追加聲調鍵
        keys.append(self.TONE_MAP.get(tone, ' '))
        return "".join(keys)

    @functools.lru_cache(maxsize=20000)
    def hanzi_to_hsu_keys(self, token: str) -> str:
        """純中文字串直接自快取轉譯為許氏鍵碼序列"""
        bpmf_str = self.annotator.annotate(token)
        syllables = bpmf_str.split()
        return "".join(self.bpmf_syllable_to_hsu(s) for s in syllables)

    def text_to_hsu_clauses(self, text: str) -> List[Tuple[str, str]]:
        """
        將文章分句並轉換為 (原文字句, 許氏鍵碼序列) 的子句清單。
        標點符號會作為子句切割點，利於模擬實際輸入法鍵入與 Commit 行為。
        """
        clauses: List[Tuple[str, str]] = []
        if not text:
            return clauses

        # 切割為 中文字串群 與 非中文字串群 (包含標點、英數、空白、符號)
        tokens = re.findall(r'([\u4e00-\u9fa5]+|[^\u4e00-\u9fa5]+)', text)
        for token in tokens:
            if not token:
                continue

            if re.match(r'^[\u4e00-\u9fa5]+$', token):
                # 純中文：標註注音並轉換為許氏鍵碼（極速快取）
                keys = self.hanzi_to_hsu_keys(token)
                clauses.append((token, keys))
            else:
                # 非中文 (英數、標點、符號、空白)：直接穿透
                clauses.append((token, token))

        return clauses



    def text_to_hsu_keys(self, text: str) -> str:
        """將整段文字無縫轉為連續許氏鍵碼"""
        clauses = self.text_to_hsu_clauses(text)
        return "".join(keys for _, keys in clauses if not all(c in self.PUNCTUATIONS for c in keys))


if __name__ == "__main__":
    compiler = TextToHsuCompiler()
    test_text = "我們都是自然人，現在硬體環境增強多少倍了？"
    res = compiler.text_to_hsu_clauses(test_text)
    for clause, keys in res:
        print(f"[{clause}] -> Keys: {keys}")
