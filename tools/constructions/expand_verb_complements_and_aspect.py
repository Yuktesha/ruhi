#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_verb_complements_and_aspect.py
=============================================================================
第四階段：動補結構與時態體標記引擎 (Verb-Complement & Aspectual Marker Rules)
依據呂叔湘《現代漢語八百詞》動補篇、劉月華《實用現代漢語語法》補語章節。

涵蓋四大核心語法模組：
1. 可能補語 (Potential Complements: V得C / V不C)
   看得懂/看不懂、看得到/看不到、聽得懂/聽不懂、買得起/買不起、
   吃得飽/吃不飽、吃得下/吃不下、睡得著/睡不著、受得了/受不了、
   記得住/記不住、走得動/走不動、堅持得住/堅持不住。
2. 複合趨向補語 (Directional Complements: V起來、V下去、V出來、V進去)
   站起來、坐下來、拿出來、拿進去、堅持下去、走下去、活下去、想起來。
3. 動量與時量補語拓撲 (Action-Measure & Time-Measure Patterns: V了+時段+O)
   睡了八個小時覺、住了整整三年、等了半個小時、看了好幾遍、去了好幾次。
4. 動態體標記拓撲 (Aspectual Markers: 著, 了, 過)
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

# 1. 可能補語列表 (Verb + 得/不 + Complement)
POTENTIAL_COMPLEMENT_PAIRS = [
    # (動詞, 補語, 動詞注音, 補語注音)
    ("看", "懂", "ㄎㄢˋ", "ㄉㄨㄥˇ"),
    ("看", "到", "ㄎㄢˋ", "ㄉㄠˋ"),
    ("看", "見", "ㄎㄢˋ", "ㄐㄧㄢˋ"),
    ("看", "清", "ㄎㄢˋ", "ㄑㄧㄥ"),
    ("看", "起", "ㄎㄢˋ", "ㄑㄧˇ"),
    ("聽", "懂", "ㄊㄧㄥ", "ㄉㄨㄥˇ"),
    ("聽", "到", "ㄊㄧㄥ", "ㄉㄠˋ"),
    ("聽", "見", "ㄊㄧㄥ", "ㄐㄧㄢˋ"),
    ("聽", "清", "ㄊㄧㄥ", "ㄑㄧㄥ"),
    ("買", "起", "ㄇㄞˇ", "ㄑㄧˇ"),
    ("買", "到", "ㄇㄞˇ", "ㄉㄠˋ"),
    ("做", "完", "ㄗㄨㄛˋ", "ㄨㄢˊ"),
    ("做", "好", "ㄗㄨㄛˋ", "ㄏㄠˇ"),
    ("做", "到", "ㄗㄨㄛˋ", "ㄉㄠˋ"),
    ("吃", "飽", "ㄔ", "ㄅㄠˇ"),
    ("吃", "下", "ㄔ", "ㄒㄧㄚˋ"),
    ("吃", "完", "ㄔ", "ㄨㄢˊ"),
    ("睡", "著", "ㄕㄨㄟˋ", "ㄓㄠˊ"),
    ("受", "了", "ㄕㄡˋ", "ㄌㄧㄠˇ"),
    ("記", "住", "ㄐㄧˋ", "ㄓㄨˋ"),
    ("記", "得", "ㄐㄧˋ", "ㄉㄜˊ"),
    ("忘", "了", "ㄨㄤˋ", "ㄌㄧㄠˇ"),
    ("走", "動", "ㄗㄡˇ", "ㄉㄨㄥˋ"),
    ("說", "清", "ㄕㄨㄛ", "ㄑㄧㄥ"),
    ("想", "通", "ㄒㄧㄤˇ", "ㄊㄨㄥ"),
    ("拿", "動", "ㄋㄚˊ", "ㄉㄨㄥˋ"),
    ("顧", "了", "ㄍㄨˋ", "ㄌㄧㄠˇ"),
    ("追", "上", "ㄓㄨㄟ", "ㄕㄤˋ"),
    ("配", "上", "ㄆㄟˋ", "ㄕㄤˋ"),
    ("合", "來", "ㄏㄜˊ", "ㄌㄞˊ"),
]

# 2. 趨向補語列表 (Directional Complements)
DIRECTIONAL_VERBS = [
    ("站起來", "ㄓㄢˋ ㄑㄧˇ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("坐下來", "ㄗㄨㄛˋ ㄒㄧㄚˋ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("拿出來", "ㄋㄚˊ ㄔㄨ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("拿進去", "ㄋㄚˊ ㄐㄧㄣˋ ㄑㄩˋ", 1850, "verb_directional", "趨向動詞"),
    ("跑過去", "ㄆㄠˇ ㄍㄨㄛˋ ㄑㄩˋ", 1850, "verb_directional", "趨向動詞"),
    ("走下去", "ㄗㄡˇ ㄒㄧㄚˋ ㄑㄩˋ", 1850, "verb_directional", "趨向動詞"),
    ("活下去", "ㄏㄨㄛˊ ㄒㄧㄚˋ ㄑㄩˋ", 1850, "verb_directional", "趨向動詞"),
    ("堅持下去", "ㄐㄧㄢ ㄔˊ ㄒㄧㄚˋ ㄑㄩˋ", 1900, "verb_directional", "趨向動詞"),
    ("想起來", "ㄒㄧㄤˇ ㄑㄧˇ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("想出來", "ㄒㄧㄤˇ ㄔㄨ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("做下去", "ㄗㄨㄛˋ ㄒㄧㄚˋ ㄑㄩˋ", 1850, "verb_directional", "趨向動詞"),
    ("說出來", "ㄕㄨㄛ ㄔㄨ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("看出來", "ㄎㄢˋ ㄔㄨ ㄌㄞˊ", 1850, "verb_directional", "趨向動詞"),
    ("傳承下去", "ㄔㄨㄢˊ ㄔㄥˊ ㄒㄧㄚˋ ㄑㄩˋ", 1900, "verb_directional", "趨向動詞"),
]

# 3. 動量與時量補語結構 (Action-Measure & Time-Measure Phrases)
MEASURE_COMPLEMENT_ENTRIES = [
    # 睡了八個小時覺 系列
    ("睡了八個小時覺", "ㄕㄨㄟˋ ˙ㄌㄜ ㄅㄚ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ ㄐㄧㄠˋ", 2000, "measure_complement", "時量補語離合短語(輕聲)"),
    ("睡了八個小時覺", "ㄕㄨㄟˋ ˙ㄌㄜ ㄅㄚ ㄍㄜˋ ㄒㄧㄠˇ ㄕˊ ㄐㄧㄠˋ", 2000, "measure_complement", "時量補語離合短語(四聲)"),
    ("睡了八個小時", "ㄕㄨㄟˋ ˙ㄌㄜ ㄅㄚ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ", 1900, "measure_complement", "時量補語短語"),
    ("睡了八個小時", "ㄕㄨㄟˋ ˙ㄌㄜ ㄅㄚ ㄍㄜˋ ㄒㄧㄠˇ ㄕˊ", 1900, "measure_complement", "時量補語短語"),
    ("八個小時覺", "ㄅㄚ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ ㄐㄧㄠˋ", 1850, "measure_complement", "時量賓語"),
    ("八個小時覺", "ㄅㄚ ㄍㄜˋ ㄒㄧㄠˇ ㄕˊ ㄐㄧㄠˋ", 1850, "measure_complement", "時量賓語"),
    ("八個小時", "ㄅㄚ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ", 1900, "measure_complement", "時量名詞"),
    ("八個小時", "ㄅㄚ ㄍㄜˋ ㄒㄧㄠˇ ㄕˊ", 1900, "measure_complement", "時量名詞"),
    ("睡了一整天覺", "ㄕㄨㄟˋ ˙ㄌㄜ ㄧ ㄓㄥˇ ㄊㄧㄢ ㄐㄧㄠˋ", 2000, "measure_complement", "時量補語短語"),
    ("一整天覺", "ㄧ ㄓㄥˇ ㄊㄧㄢ ㄐㄧㄠˋ", 1850, "measure_complement", "時量賓語"),

    # 住了整整三年 系列
    ("住了整整三年", "ㄓㄨˋ ˙ㄌㄜ ㄓㄥˇ ㄓㄥˇ ㄙㄢ ㄋㄧㄢˊ", 2000, "measure_complement", "時量補語短語"),
    ("整整三年", "ㄓㄥˇ ㄓㄥˇ ㄙㄢ ㄋㄧㄢˊ", 1900, "measure_complement", "時量名詞短語"),
    ("住了", "ㄓㄨˋ ˙ㄌㄜ", 1800, "aspect", "完成體動詞"),

    # 等了半個小時 系列
    ("等了半個小時", "ㄉㄥˇ ˙ㄌㄜ ㄅㄢˋ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ", 2000, "measure_complement", "時量補語短語(輕聲)"),
    ("等了半個小時", "ㄉㄥˇ ˙ㄌㄜ ㄅㄢˋ ㄍㄜˋ ㄒㄧㄠˇ ㄕˊ", 2000, "measure_complement", "時量補語短語(四聲)"),
    ("半個小時", "ㄅㄢˋ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ", 1900, "measure_complement", "時量名詞短語"),
    ("半個小時", "ㄅㄢˋ ㄍㄜˋ ㄒㄧㄠˇ ㄕˊ", 1900, "measure_complement", "時量名詞短語"),
    ("等了", "ㄉㄥˇ ˙ㄌㄜ", 1800, "aspect", "完成體動詞"),

    # 看了好幾遍 / 去了好幾次 系列
    ("看了好幾遍", "ㄎㄢˋ ˙ㄌㄜ ㄏㄠˇ ㄐㄧˇ ㄅㄧㄢˋ", 2000, "measure_complement", "動量補語短語"),
    ("好幾遍", "ㄏㄠˇ ㄐㄧˇ ㄅㄧㄢˋ", 1850, "measure_complement", "動量名詞短語"),
    ("去了好幾次", "ㄑㄩˋ ˙ㄌㄜ ㄏㄠˇ ㄐㄧˇ ㄘˋ", 2000, "measure_complement", "動量補語短語"),
    ("好幾次", "ㄏㄠˇ ㄐㄧˇ ㄘˋ", 1850, "measure_complement", "動量名詞短語"),
    ("看了", "ㄎㄢˋ ˙ㄌㄜ", 1800, "aspect", "完成體動詞"),
    ("去了", "ㄑㄩˋ ˙ㄌㄜ", 1800, "aspect", "完成體動詞"),
]

# 4. 語法結構轉移矩陣 (Transitions)
PHASE_4_TRANSITIONS = [
    # 時量補語框架
    ("睡了", "八個小時", 45.0, "time_measure", "時量動賓框架"),
    ("八個小時", "覺", 48.0, "time_measure", "時量動賓框架"),
    ("睡了", "一整天", 45.0, "time_measure", "時量動賓框架"),
    ("一整天", "覺", 48.0, "time_measure", "時量動賓框架"),
    ("住了", "整整三年", 48.0, "time_measure", "時量補語搭配"),
    ("等了", "半個小時", 48.0, "time_measure", "時量補語搭配"),
    ("看了", "好幾遍", 48.0, "time_measure", "動量補語搭配"),
    ("去了", "好幾次", 48.0, "time_measure", "動量補語搭配"),

    # 趨向補語框架
    ("堅持", "下去", 48.0, "dir_complement", "趨向補語搭配"),
    ("傳承", "下去", 48.0, "dir_complement", "趨向補語搭配"),
    ("活", "下去", 45.0, "dir_complement", "趨向補語搭配"),
    ("走", "下去", 45.0, "dir_complement", "趨向補語搭配"),
    ("站", "起來", 45.0, "dir_complement", "趨向補語搭配"),
    ("坐", "下來", 45.0, "dir_complement", "趨向補語搭配"),
    ("拿", "出來", 45.0, "dir_complement", "趨向補語搭配"),
    ("拿", "進去", 45.0, "dir_complement", "趨向補語搭配"),
]

def generate_potential_entries():
    entries = []
    for v, c, v_bpm, c_bpm in POTENTIAL_COMPLEMENT_PAIRS:
        # V得C (肯定式: 支援 ˙ㄉㄜ 與 ㄉㄜˊ 雙注音)
        pos = f"{v}得{c}"
        entries.append((pos, f"{v_bpm} ˙ㄉㄜ {c_bpm}", 1950, "potential_complement", f"{v}{c}可能補語(肯定輕聲)"))
        entries.append((pos, f"{v_bpm} ㄉㄜˊ {c_bpm}", 1950, "potential_complement", f"{v}{c}可能補語(肯定陽平)"))

        # V不C (否定式: ㄅㄨˋ 或 ㄅㄨˊ 變調)
        neg = f"{v}不{c}"
        entries.append((neg, f"{v_bpm} ㄅㄨˋ {c_bpm}", 1950, "potential_complement", f"{v}{c}可能補語(否定去聲)"))
        entries.append((neg, f"{v_bpm} ㄅㄨˊ {c_bpm}", 1950, "potential_complement", f"{v}{c}可能補語(否定變調陽平)"))

    return entries

def main():
    print("🚀 啟動【第四階段：動補結構與時態體標記引擎】...")

    domain_entries = []
    # 1. 產生可能補語條目
    domain_entries.extend(generate_potential_entries())
    # 2. 加入趨向補語條目
    domain_entries.extend(DIRECTIONAL_VERBS)
    # 3. 加入時量/動量補語條目
    domain_entries.extend(MEASURE_COMPLEMENT_ENTRIES)

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
        for w, bpm, wt, cat, note in domain_entries:
            if (w, bpm) not in existing_domain_words:
                f.write(f"{w}\t{bpm}\t{wt}\t{cat}\t{note}\n")
                existing_domain_words.add((w, bpm))
                added_domain += 1

    print(f"✨ 成功注入 {added_domain} 條動補結構與時態體條目至 {DOMAIN_PATH}！")

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
        for p_w, c_w, bonus, cat, note in PHASE_4_TRANSITIONS:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組動補語法轉移規則至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
