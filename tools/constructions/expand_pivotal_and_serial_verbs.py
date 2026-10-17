#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_pivotal_and_serial_verbs.py
=============================================================================
第七階段：兼語式與連動結構引擎 (Pivotal & Serial Verb Constructions)
依據趙元任《中國話的文法》與呂叔湘《現代漢語八百詞》兼語式與連動結構論。

核心解決痛點：
1. 兼語致使動詞音頻語意劫掠：
   - 『叫』(1950 vs 教)、『叫他』(1950)
   - 『使人』(1950 vs 矢人)、『使』(2000)
   - 『催他』(1950 vs 摧他)、『催』(1950)
   - 『進來』(1950 vs 近來)、『快走』(1950 vs 塊走)
2. 連動結構工具、處所與目的動補：
   - 『借書』(1950 vs 戒書)、『圖書館 ➔ 借書』
   - 『找朋友』(1950 vs 爪朋友)、『台北 ➔ 找朋友』
   - 『台北』(2000)、『坐高鐵』(1950 vs 作高鐵)、『回台北』(1950)
   - 『拿筆』(1950 vs 拿比)、『拿筆 ➔ 寫字』
   - 『閉著』(1950 vs 壁著)、『閉著眼睛』(1950)、『聽音樂』(1950)
   - 『站著』(1950 vs 戰著)、『等公車』(1950 vs 等功車)、『公車』(1950)
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

# 1. 兼語式與連動結構領域核心詞 (Domain Entries)
PIVOTAL_SERIAL_DOMAIN_ENTRIES = [
    # ── 兼語致使動詞與短語 ──
    ("叫他", "ㄐㄧㄠˋ ㄊㄚ", 1950, "pivotal_phrase", "兼語短語(防教他)"),
    ("叫", "ㄐㄧㄠˋ", 1950, "verb_pivotal", "兼語致使動詞(防教/窖)"),
    ("快來", "ㄎㄨㄞˋ ㄌㄞˊ", 1950, "verb_complement", "動詞短語"),
    ("讓他", "ㄖㄤˋ ㄊㄚ", 1950, "pivotal_phrase", "兼語短語"),
    ("進來", "ㄐㄧㄣˋ ㄌㄞˊ", 1950, "verb_direction", "趨向動詞(防近來)"),
    ("使人", "ㄕˇ ㄖㄣˊ", 1950, "pivotal_phrase", "兼語短語(防矢人)"),
    ("使", "ㄕˇ", 2000, "verb_causative", "致使動詞(防矢/始)"),
    ("催他", "ㄘㄨㄟ ㄊㄚ", 1950, "pivotal_phrase", "兼語短語(防摧他)"),
    ("催", "ㄘㄨㄟ", 1950, "verb_causative", "致使動詞(防摧)"),
    ("快走", "ㄎㄨㄞˋ ㄗㄡˇ", 1950, "verb_phrase", "動詞短語(防塊走)"),

    # ── 連動結構動詞、工具與目的名詞 ──
    ("借書", "ㄐㄧㄝˋ ㄕㄨ", 1950, "verb_object", "動賓短語(防戒書)"),
    ("找朋友", "ㄓㄠˇ ㄆㄥˊ ㄧㄡˇ", 1950, "verb_object", "動賓短語(防爪朋友)"),
    ("台北", "ㄊㄞˊ ㄅㄟˇ", 2000, "proper_noun_tw", "台灣核心地名(正體台北)"),
    ("坐高鐵", "ㄗㄨㄛˋ ㄍㄠ ㄊㄧㄝˇ", 1950, "verb_object", "連動方式短語(防作高鐵)"),
    ("高鐵", "ㄍㄠ ㄊㄧㄝˇ", 1950, "noun_tw", "台灣高鐵專名"),
    ("回台北", "ㄏㄨㄟˊ ㄊㄞˊ ㄅㄟˇ", 1950, "verb_object", "連動目的短語"),
    ("拿筆", "ㄋㄚˊ ㄅㄧˇ", 1950, "verb_object", "連動工具短語(防拿比)"),
    ("寫字", "ㄒㄧㄝˇ ㄗˋ", 1950, "verb_object", "動賓短語(防血字)"),
    ("閉著", "ㄅㄧˋ ˙ㄓㄜ", 1950, "verb_aspect", "伴隨狀態動態(防壁著)"),
    ("閉著眼睛", "ㄅㄧˋ ˙ㄓㄜ ㄧㄢˇ ㄐㄧㄥ", 2000, "phrase", "伴隨動態短語"),
    ("聽音樂", "ㄊㄧㄥ ㄧㄣ ㄩㄝˋ", 1950, "verb_object", "連動目的短語"),
    ("站著", "ㄓㄢˋ ˙ㄓㄜ", 1950, "verb_aspect", "伴隨狀態動態(防戰著)"),
    ("公車", "ㄍㄨㄥ ㄔㄜ", 1950, "noun_tw", "大眾交通名詞(防功車)"),
    ("等公車", "ㄉㄥˇ ㄍㄨㄥ ㄔㄜ", 1950, "verb_object", "連動目的短語"),
]

# 2. 兼語式與連動結構引力轉移矩陣 (Pivotal & Serial Transitions)
PIVOTAL_SERIAL_TRANSITIONS = [
    # ── 兼語式引力鏈 ──
    ("叫他", "快來", 50.0, "pivotal_chain", "叫他快來搭配"),
    ("讓他", "進來", 50.0, "pivotal_chain", "讓他進來搭配"),
    ("使人", "感動", 50.0, "pivotal_chain", "使人感動搭配"),
    ("催他", "快走", 50.0, "pivotal_chain", "催他快走搭配"),
    ("教他", "快來", -50.0, "anti_confusion", "防教他快來誤拼"),
    ("讓他", "近來", -50.0, "anti_confusion", "防近來誤拼"),
    ("矢人", "感動", -50.0, "anti_confusion", "防矢人誤拼"),
    ("摧他", "快走", -50.0, "anti_confusion", "防摧他誤拼"),

    # ── 連動結構引力鏈 ──
    ("圖書館", "借書", 50.0, "serial_chain", "圖書館借書搭配"),
    ("去", "圖書館", 48.0, "serial_chain", "去圖書館搭配"),
    ("來", "台北", 50.0, "serial_chain", "來台北搭配"),
    ("台北", "找朋友", 50.0, "serial_chain", "台北找朋友搭配"),
    ("坐", "高鐵", 50.0, "serial_chain", "坐高鐵搭配"),
    ("坐高鐵", "回台北", 50.0, "serial_chain", "坐高鐵回台北搭配"),
    ("高鐵", "回台北", 48.0, "serial_chain", "高鐵回台北搭配"),
    ("拿筆", "寫字", 50.0, "serial_chain", "拿筆寫字搭配"),
    ("拿", "筆", 48.0, "serial_chain", "拿筆搭配"),
    ("閉著", "眼睛", 50.0, "serial_chain", "閉著眼睛搭配"),
    ("眼睛", "聽音樂", 48.0, "serial_chain", "眼睛聽音樂搭配"),
    ("閉著眼睛", "聽音樂", 50.0, "serial_chain", "閉著眼睛聽音樂搭配"),
    ("站著", "等公車", 50.0, "serial_chain", "站著等公車搭配"),
    ("等", "公車", 50.0, "serial_chain", "等公車搭配"),

    # ── 反向抑制 ──
    ("圖書館", "戒書", -50.0, "anti_confusion", "防戒書誤拼"),
    ("台北", "爪朋友", -50.0, "anti_confusion", "防爪朋友誤拼"),
    ("作", "高鐵", -50.0, "anti_confusion", "防作高鐵誤拼"),
    ("拿", "比", -45.0, "anti_confusion", "防拿比誤拼"),
    ("壁著", "眼睛", -50.0, "anti_confusion", "防壁著誤拼"),
    ("戰著", "等公車", -50.0, "anti_confusion", "防戰著誤拼"),
    ("等", "功車", -50.0, "anti_confusion", "防功車誤拼"),
]

def main():
    print("🚀 啟動【第七階段：兼語式與連動結構引擎】擴展...")

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
        for w, bpm, wt, cat, note in PIVOTAL_SERIAL_DOMAIN_ENTRIES:
            if (w, bpm) not in existing_domain_words:
                f.write(f"{w}\t{bpm}\t{wt}\t{cat}\t{note}\n")
                existing_domain_words.add((w, bpm))
                added_domain += 1

    print(f"✨ 成功注入 {added_domain} 條兼語與連動核心詞至 {DOMAIN_PATH}！")

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
        for p_w, c_w, bonus, cat, note in PIVOTAL_SERIAL_TRANSITIONS:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組兼語與連動引力轉移矩陣至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
