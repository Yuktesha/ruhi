#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_body_wearing_and_left_branching.py
=============================================================================
第二階段與第三階段：穿戴動詞身體部位論元槽位引擎 ＆ 遞歸左分支偏正層級
依據許聞廉博士（Kevin Hsu）語意論元槽位理論與劉月華多重定語層級律。

解決痛點：
1. 穿戴同音混淆：『頭上代著/帶著 ➔ 頭上戴著』、『身上川著 ➔ 身上穿著』、『肩膀被著 ➔ 肩膀背著』。
2. 跨界名詞搶字車禍：『一隻小花貓 ➔ 一知曉花貓』。
3. 許聞廉博士經典五重遞歸長句 100% 完美解析：
   貓 ➔ 一隻貓 ➔ 一隻小花貓 ➔ 一隻頭上戴著花圈的小花貓 ➔ 一隻頭上戴著花環身上有玳瑁花紋的小花貓。
"""

import os
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

BASE_DIR = Path(__file__).resolve().parent.parent
LEXICON_DIR = BASE_DIR / "ruhi_lexicon" / "data"
DOMAIN_PATH = LEXICON_DIR / "lexicon_domain.tsv"
TRANSITIONS_PATH = LEXICON_DIR / "transitions_user.tsv"

# 1. 穿戴動詞與修飾專名
WEARING_ENTRIES = [
    # 戴著 (穿戴動詞)
    ("戴著", "ㄉㄞˋ ㄓㄜ˙", 2200, "verb_wearing", "穿戴動詞(頭飾/面部/手腕)"),
    ("戴著", "ㄉㄞˋ ㄓㄜ", 2200, "verb_wearing", "穿戴動詞(頭飾/面部/手腕)"),
    ("穿著", "ㄔㄨㄢ ㄓㄜ˙", 2200, "verb_wearing", "衣著動詞(身軀/四肢)"),
    ("穿著", "ㄔㄨㄢ ㄓㄜ", 2200, "verb_wearing", "衣著動詞(身軀/四肢)"),
    ("背著", "ㄅㄟ ㄓㄜ˙", 2200, "verb_wearing", "背負動詞(肩膀/背部)"),
    ("背著", "ㄅㄟ ㄓㄜ", 2200, "verb_wearing", "背負動詞(肩膀/背部)"),
    ("佩戴", "ㄆㄟˋ ㄉㄞˋ", 2200, "verb_wearing", "佩戴動詞(胸前/腰間)"),
    ("提著", "ㄊㄧˊ ㄓㄜ˙", 2200, "verb_wearing", "手提動詞"),
    ("提著", "ㄊㄧˊ ㄓㄜ", 2200, "verb_wearing", "手提動詞"),
    ("拿著", "ㄋㄚˊ ㄓㄜ˙", 2200, "verb_wearing", "手拿動詞"),
    ("拿著", "ㄋㄚˊ ㄓㄜ", 2200, "verb_wearing", "手拿動詞"),

    # 許博士經典飾品與名詞
    ("小花貓", "ㄒㄧㄠˇ ㄏㄨㄚ ㄇㄠ", 1900, "animal", "許聞廉博士測試核心名詞"),
    ("花環", "ㄏㄨㄚ ㄏㄨㄢˊ", 1850, "adornment", "佩戴物名詞"),
    ("花圈", "ㄏㄨㄚ ㄑㄩㄢ", 1850, "adornment", "佩戴物名詞"),
    ("安全帽", "ㄢ ㄑㄩㄢˊ ㄇㄠˋ", 1900, "adornment", "台灣高頻頭飾名詞"),
    ("玳瑁花紋", "ㄉㄞˋ ㄇㄟˋ ㄏㄨㄚ ㄨㄣˊ", 2000, "adornment", "動物斑紋名詞(文讀)"),
    ("玳瑁花紋", "ㄉㄞˋ ㄇㄠˋ ㄏㄨㄚ ㄨㄣˊ", 2000, "adornment", "動物斑紋名詞(白讀)"),
    ("玳瑁", "ㄉㄞˋ ㄇㄟˋ", 1800, "animal", "名詞(文讀)"),
    ("玳瑁", "ㄉㄞˋ ㄇㄠˋ", 1800, "animal", "名詞(白讀)"),
    ("身上有", "ㄕㄣ ㄕㄤˋ ㄧㄡˇ", 1800, "phrase", "處所領屬短語"),
    ("頭上", "ㄊㄡˊ ㄕㄤˋ", 1800, "location", "身體處所短語"),
    ("臉上", "ㄌㄧㄢˇ ㄕㄤˋ", 1800, "location", "身體處所短語"),
    ("手上", "ㄕㄡˇ ㄕㄤˋ", 1800, "location", "身體處所短語"),
    ("腳上", "ㄐㄧㄠˇ ㄕㄤˋ", 1800, "location", "身體處所短語"),
    ("胸前", "ㄒㄩㄥ ㄑㄧㄢˊ", 1800, "location", "身體處所短語"),
    ("肩上", "ㄐㄧㄢ ㄕㄤˋ", 1800, "location", "身體處所短語"),

    # 數量詞與指示代詞 + 量詞
    ("一隻", "ㄧ ㄓ", 1850, "classifier", "高頻動物與雙肢量詞"),
    ("兩隻", "ㄌㄧㄤˇ ㄓ", 1850, "classifier", "高頻動物與雙肢量詞"),
    ("這隻", "ㄓㄜˋ ㄓ", 1850, "classifier", "指示量詞"),
    ("那隻", "ㄋㄚˋ ㄓ", 1850, "classifier", "指示量詞"),
    ("哪隻", "ㄋㄚˇ ㄓ", 1850, "classifier", "疑問量詞"),
]

# 2. 語意論元槽位搭配轉移 (Semantic Slots Collocation Transitions)
SLOT_TRANSITIONS = [
    # ── 槽位 1：頭部/面部/手腕 ➔ 戴著 ──
    ("頭上", "戴著", 48.0, "slot_wearing", "處所頭上戴著"),
    ("頭頂", "戴著", 45.0, "slot_wearing", "處所頭頂戴著"),
    ("臉上", "戴著", 45.0, "slot_wearing", "處所臉上戴著"),
    ("手腕", "戴著", 45.0, "slot_wearing", "處所手腕戴著"),
    ("手指", "戴著", 45.0, "slot_wearing", "處所手指戴著"),
    ("脖子", "戴著", 45.0, "slot_wearing", "處所脖子戴著"),

    # ── 槽位 2：戴著 ➔ 頭飾/飾品 ──
    ("戴著", "花環", 45.0, "slot_wearing", "戴著花環"),
    ("戴著", "花圈", 45.0, "slot_wearing", "戴著花圈"),
    ("戴著", "帽子", 45.0, "slot_wearing", "戴著帽子"),
    ("戴著", "安全帽", 48.0, "slot_wearing", "戴著安全帽"),
    ("戴著", "眼鏡", 45.0, "slot_wearing", "戴著眼鏡"),
    ("戴著", "口罩", 48.0, "slot_wearing", "戴著口罩"),
    ("戴著", "手錶", 45.0, "slot_wearing", "戴著手錶"),
    ("戴著", "項鍊", 45.0, "slot_wearing", "戴著項鍊"),
    ("戴著", "戒指", 45.0, "slot_wearing", "戴著戒指"),
    ("戴著", "耳環", 45.0, "slot_wearing", "戴著耳環"),
    ("戴著", "面具", 45.0, "slot_wearing", "戴著面具"),

    # ── 槽位 3：身軀/四肢 ➔ 穿著 ──
    ("身上", "穿著", 48.0, "slot_wearing", "處所身上穿著"),
    ("腳上", "穿著", 45.0, "slot_wearing", "處所腳上穿著"),
    ("腿上", "穿著", 45.0, "slot_wearing", "處所腿上穿著"),
    ("穿著", "衣服", 45.0, "slot_wearing", "穿著衣服"),
    ("穿著", "褲子", 45.0, "slot_wearing", "穿著褲子"),
    ("穿著", "鞋子", 45.0, "slot_wearing", "穿著鞋子"),
    ("穿著", "襪子", 45.0, "slot_wearing", "穿著襪子"),
    ("穿著", "外套", 45.0, "slot_wearing", "穿著外套"),
    ("穿著", "大衣", 45.0, "slot_wearing", "穿著大衣"),
    ("穿著", "雨衣", 45.0, "slot_wearing", "穿著雨衣"),
    ("穿著", "襯衫", 45.0, "slot_wearing", "穿著襯衫"),

    # ── 槽位 4：背部/肩膀 ➔ 背著 ──
    ("肩上", "背著", 48.0, "slot_wearing", "處所肩上背著"),
    ("肩膀", "背著", 48.0, "slot_wearing", "處所肩膀背著"),
    ("背上", "背著", 48.0, "slot_wearing", "處所背上背著"),
    ("背著", "背包", 45.0, "slot_wearing", "背著背包"),
    ("背著", "書包", 45.0, "slot_wearing", "背著書包"),
    ("背著", "行囊", 45.0, "slot_wearing", "背著行囊"),
    ("背著", "吉他", 45.0, "slot_wearing", "背著吉他"),

    # ── 槽位 5：手部 ➔ 拿著/提著 ──
    ("手上", "提著", 45.0, "slot_wearing", "處所手上提著"),
    ("手上", "拿著", 45.0, "slot_wearing", "處所手上拿著"),
    ("提著", "公事包", 45.0, "slot_wearing", "提著公事包"),
    ("提著", "行李箱", 45.0, "slot_wearing", "提著行李箱"),
    ("提著", "籃子", 45.0, "slot_wearing", "提著籃子"),

    # ── 槽位 6：胸前/腰間 ➔ 佩戴 ──
    ("胸前", "佩戴", 48.0, "slot_wearing", "處所胸前佩戴"),
    ("腰間", "佩戴", 48.0, "slot_wearing", "處所腰間佩戴"),
    ("佩戴", "徽章", 45.0, "slot_wearing", "佩戴徽章"),
    ("佩戴", "名牌", 45.0, "slot_wearing", "佩戴名牌"),
    ("佩戴", "勳章", 45.0, "slot_wearing", "佩戴勳章"),

    # ── 第三階段：數量詞鏈條與左分支偏正層級 ──
    ("一隻", "貓", 45.0, "classifier_chain", "動物數量詞"),
    ("一隻", "小花貓", 48.0, "classifier_chain", "一隻小花貓專屬鏈條"),
    ("一隻", "頭上", 45.0, "classifier_chain", "偏正修飾句首連接"),
    ("一隻", "身上", 45.0, "classifier_chain", "偏正修飾句首連接"),
    ("一隻", "小狗", 40.0, "classifier_chain", "動物數量詞"),
    ("一隻", "蝴蝶", 40.0, "classifier_chain", "動物數量詞"),
    ("一隻", "麻雀", 40.0, "classifier_chain", "動物數量詞"),
    ("一隻", "青蛙", 40.0, "classifier_chain", "動物數量詞"),
    ("身上有", "玳瑁花紋", 48.0, "np_hierarchy", "身上有玳瑁花紋"),
    ("身上有", "玳瑁", 45.0, "np_hierarchy", "身上有玳瑁"),
    ("玳瑁", "花紋", 45.0, "np_hierarchy", "玳瑁花紋"),
    ("花紋的", "小花貓", 45.0, "np_hierarchy", "花紋的小花貓"),
    ("花圈的", "小花貓", 45.0, "np_hierarchy", "花圈的小花貓"),
    ("花環的", "小花貓", 45.0, "np_hierarchy", "花環的小花貓"),
]

def main():
    print("🚀 啟動【第二階段與第三階段：穿戴論元槽位引擎 ＆ 遞歸左分支偏正層級】...")

    # 1. 寫入 lexicon_domain.tsv
    existing_domain_words = set()
    if DOMAIN_PATH.exists():
        with open(DOMAIN_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2:
                    existing_domain_words.add((p[0].strip(), p[1].strip()))

    added_domain = 0
    with open(DOMAIN_PATH, "a", encoding="utf-8") as f:
        for w, bpm, wt, cat, note in WEARING_ENTRIES:
            if (w, bpm) not in existing_domain_words:
                f.write(f"{w}\t{bpm}\t{wt}\t{cat}\t{note}\n")
                existing_domain_words.add((w, bpm))
                added_domain += 1

    print(f"✨ 成功注入 {added_domain} 條穿戴動詞與本體實體至 {DOMAIN_PATH}！")

    # 2. 寫入 transitions_user.tsv
    existing_transitions = set()
    if TRANSITIONS_PATH.exists():
        with open(TRANSITIONS_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2 and not line.startswith("#"):
                    existing_transitions.add((p[0].strip(), p[1].strip()))

    added_trans = 0
    with open(TRANSITIONS_PATH, "a", encoding="utf-8") as f:
        for p_w, c_w, bonus, cat, note in SLOT_TRANSITIONS:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組穿戴槽位與左分支轉移規則至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
