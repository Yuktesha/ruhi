#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_existential_and_reduplication.py
=============================================================================
第八階段：存現句與動形重疊生動構式 (Existential & Reduplication Topology)
依據陸儉明《現代漢語語法研究教程》與劉月華《實用現代漢語語法》存現與重疊論。

核心解決痛點：
1. 空間存現句量詞與動詞音頻語意劫掠：
   - 『一幅畫』(2000 vs 衣服化)、『一幅』(1950)
   - 『朵朵白雲』(2000 vs 趓趓白雲)、『朵朵』(1950)
   - 『幾行字』(1950 vs 己航自)、『幾行』(1900)
   - 『擺滿了』(1950 vs 百滿了)、『擺滿』(1950)
   - 『種著』(1950 vs 重著)、『許多花』(1950 vs 許多化)
2. 生動重疊構式防冷僻語意劫掠：
   - 『想了一想』(2000 vs 想了一餉)
   - 『漂漂亮亮』(2000 vs 票票悢悢)
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

# 1. 存現句與重疊生動構式領域詞 (Domain Entries)
EXISTENTIAL_DOMAIN_ENTRIES = [
    # ── 存現句量詞、客體與動詞補語 ──
    ("一幅畫", "ㄧ ㄈㄨˊ ㄏㄨㄚˋ", 2000, "noun_phrase", "量名短語(防衣服化)"),
    ("一幅", "ㄧ ㄈㄨˊ", 1950, "classifier_phrase", "量詞短語"),
    ("朵朵", "ㄉㄨㄛˇ ㄉㄨㄛˇ", 1950, "reduplication_measure", "疊字量詞(防趓趓)"),
    ("朵朵白雲", "ㄉㄨㄛˇ ㄉㄨㄛˇ ㄅㄞˊ ㄩㄣˊ", 2000, "noun_phrase", "偏正名詞短語"),
    ("幾行字", "ㄐㄧˇ ㄏㄤˊ ㄗˋ", 1950, "noun_phrase", "數量名短語(防己航自)"),
    ("幾行", "ㄐㄧˇ ㄏㄤˊ", 1900, "classifier_phrase", "數量短語"),
    ("擺滿了", "ㄅㄞˇ ㄇㄢˇ ˙ㄌㄜ", 1950, "verb_complement", "動補完成體(防百滿了)"),
    ("擺滿", "ㄅㄞˇ ㄇㄢˇ", 1950, "verb_complement", "動補短語"),
    ("種著", "ㄓㄨㄥˋ ˙ㄓㄜ", 1950, "verb_aspect", "存現動態助詞(防重著)"),
    ("許多花", "ㄒㄩˇ ㄉㄨㄛ ㄏㄨㄚ", 1950, "noun_phrase", "數量名短語(防許多化)"),

    # ── 生動重疊詞 ──
    ("想了一想", "ㄒㄧㄤˇ ˙ㄌㄜ ㄧ ㄒㄧㄤˇ", 2000, "verb_reduplication", "動詞重疊(防想了一餉)"),
    ("漂漂亮亮", "ㄆㄧㄠˋ ㄆㄧㄠˋ ㄌㄧㄤˋ ㄌㄧㄤˋ", 2000, "adj_reduplication", "AABB疊詞(防票票悢悢)"),
]

# 2. 存現句與重疊引力轉移矩陣 (Existential & Reduplication Transitions)
EXISTENTIAL_TRANSITIONS = [
    # ── 存現句引力鏈 ──
    ("牆上", "掛著", 50.0, "existential_chain", "牆上掛著搭配"),
    ("掛著", "一幅畫", 50.0, "existential_chain", "掛著一幅畫搭配"),
    ("掛著", "一幅", 50.0, "existential_chain", "掛著一幅搭配"),
    ("一幅", "畫", 50.0, "existential_chain", "一幅畫搭配"),
    ("空中", "飄著", 50.0, "existential_chain", "空中飄著搭配"),
    ("飄著", "朵朵白雲", 50.0, "existential_chain", "飄著朵朵白雲搭配"),
    ("飄著", "朵朵", 50.0, "existential_chain", "飄著朵朵搭配"),
    ("朵朵", "白雲", 50.0, "existential_chain", "朵朵白雲搭配"),
    ("黑板上", "寫著", 50.0, "existential_chain", "黑板上寫著搭配"),
    ("寫著", "幾行字", 50.0, "existential_chain", "寫著幾行字搭配"),
    ("寫著", "幾行", 50.0, "existential_chain", "寫著幾行搭配"),
    ("幾行", "字", 50.0, "existential_chain", "幾行字搭配"),
    ("桌上", "擺滿了", 50.0, "existential_chain", "桌上擺滿了搭配"),
    ("擺滿了", "水果", 50.0, "existential_chain", "擺滿了水果搭配"),
    ("院子裡", "種著", 50.0, "existential_chain", "院子裡種著搭配"),
    ("種著", "許多花", 50.0, "existential_chain", "種著許多花搭配"),

    # ── 重疊構式引力鏈 ──
    ("想了", "一想", 50.0, "reduplication_chain", "想了一想搭配"),

    # ── 反向抑制 ──
    ("掛著", "衣服化", -50.0, "anti_confusion", "防衣服化誤拼"),
    ("掛著", "衣服", -40.0, "anti_confusion", "防掛著衣服誤拼"),
    ("飄著", "趓趓", -50.0, "anti_confusion", "防趓趓誤拼"),
    ("寫著", "己航", -50.0, "anti_confusion", "防己航誤拼"),
    ("桌上", "百滿了", -50.0, "anti_confusion", "防百滿了誤拼"),
    ("院子裡", "重著", -50.0, "anti_confusion", "防重著誤拼"),
    ("想了一", "餉", -50.0, "anti_confusion", "防想了一餉誤拼"),
    ("票票", "悢悢", -50.0, "anti_confusion", "防票票悢悢誤拼"),
]

def main():
    print("🚀 啟動【第八階段：存現句與動形重疊生動構式】擴展...")

    # 寫入 lexicon_domain.tsv
    existing_domain_words = set()
    if DOMAIN_PATH.exists():
        with open(DOMAIN_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2:
                    existing_domain_words.add((p[0].strip(), p[1].strip()))

    added_domain = 0
    with open(DOMAIN_PATH, "a", encoding="utf-8") as f:
        for w, bpm, wt, cat, note in EXISTENTIAL_DOMAIN_ENTRIES:
            if (w, bpm) not in existing_domain_words:
                f.write(f"{w}\t{bpm}\t{wt}\t{cat}\t{note}\n")
                existing_domain_words.add((w, bpm))
                added_domain += 1

    print(f"✨ 成功注入 {added_domain} 條存現與重疊核心詞至 {DOMAIN_PATH}！")

    # 寫入 transitions_user.tsv
    existing_transitions = set()
    if TRANSITIONS_PATH.exists():
        with open(TRANSITIONS_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2 and not line.startswith("#"):
                    existing_transitions.add((p[0].strip(), p[1].strip()))

    added_trans = 0
    with open(TRANSITIONS_PATH, "a", encoding="utf-8") as f:
        for p_w, c_w, bonus, cat, note in EXISTENTIAL_TRANSITIONS:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組存現與重疊引力轉移矩陣至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
