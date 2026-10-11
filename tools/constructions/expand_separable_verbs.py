#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_separable_verbs.py
=============================================================================
第一階段：漢語高頻動賓離合詞拓撲擴展引擎
依據呂叔湘《現代漢語八百詞》與劉月華《實用現代漢語語法》之離合詞插入律。

解決痛點：
1. 動量短語搶字車禍：『上一下 ➔ 上衣下』、『洗一下 ➔ 洗衣下』、『睡一下 ➔ 睡衣下』。
2. 離合插入態單字碎裂：『睡個覺 ➔ 稅個教』、『洗個澡 ➔ 喜個早』、『吃個飯 ➔ 吃個範』。
3. 超長跨度修飾尾部退化：『洗個［……］的澡 ➔ 的早』、『睡個［……］的覺 ➔ 的教』。
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

# 1. 80 組台灣生活高頻動賓離合詞核心庫 (Verb-Object Separable Roots)
# (動詞, 賓語, 動詞注音, 賓語注音, 類別)
SEPARABLE_ROOTS = [
    # 生活起居
    ("洗", "澡", "ㄒㄧˇ", "ㄗㄠˇ", "daily"),
    ("睡", "覺", "ㄕㄨㄟˋ", "ㄐㄧㄠˋ", "daily"),
    ("吃", "飯", "ㄔ", "ㄈㄢˋ", "daily"),
    ("上", "廁所", "ㄕㄤˋ", "ㄘㄜˋ ㄙㄨㄛˇ", "daily"),
    ("起", "床", "ㄑㄧˇ", "ㄔㄨㄤˊ", "daily"),
    ("刷", "牙", "ㄕㄨㄚ", "ㄧㄚˊ", "daily"),
    ("洗", "臉", "ㄒㄧˇ", "ㄌㄧㄢˇ", "daily"),
    ("理", "髮", "ㄌㄧˇ", "ㄈㄚˇ", "daily"),
    ("剪", "髮", "ㄐㄧㄢˇ", "ㄈㄚˇ", "daily"),
    ("泡", "澡", "ㄆㄠˋ", "ㄗㄠˇ", "daily"),
    ("做", "夢", "ㄗㄨㄛˋ", "ㄇㄥˋ", "daily"),
    ("流", "汗", "ㄌㄧㄡˊ", "ㄏㄢˋ", "daily"),
    ("嘆", "氣", "ㄊㄢˋ", "ㄑㄧˋ", "daily"),
    ("抽", "煙", "ㄔㄡ", "ㄧㄢ", "daily"),
    ("抽", "菸", "ㄔㄡ", "ㄧㄢ", "daily"),
    ("喝", "酒", "ㄏㄜ", "ㄐㄧㄡˇ", "daily"),
    ("泡", "茶", "ㄆㄠˋ", "ㄔㄚˊ", "daily"),
    ("煮", "飯", "ㄓㄨˇ", "ㄈㄢˋ", "daily"),

    # 社交人際與情感
    ("聊", "天", "ㄌㄧㄠˊ", "ㄊㄧㄢ", "social"),
    ("講", "話", "ㄐㄧㄤˇ", "ㄏㄨㄚˋ", "social"),
    ("說", "話", "ㄕㄨㄛ", "ㄏㄨㄚˋ", "social"),
    ("開", "玩笑", "ㄎㄞ", "ㄨㄢˊ ㄒㄧㄠˋ", "social"),
    ("請", "客", "ㄑㄧㄥˇ", "ㄎㄜˋ", "social"),
    ("幫", "忙", "ㄅㄤ", "ㄇㄤˊ", "social"),
    ("見", "面", "ㄐㄧㄢˋ", "ㄇㄧㄢˋ", "social"),
    ("打", "招呼", "ㄉㄚˇ", "ㄓㄠ ㄏㄨ", "social"),
    ("握", "手", "ㄨㄛˋ", "ㄕㄡˇ", "social"),
    ("鞠", "躬", "ㄐㄩˊ", "ㄍㄨㄥ", "social"),
    ("生", "氣", "ㄕㄥ", "ㄑㄧˋ", "social"),
    ("發", "脾氣", "ㄈㄚ", "ㄆㄧˊ ㄑㄧˋ", "social"),
    ("吵", "架", "ㄔㄠˇ", "ㄐㄧㄚˋ", "social"),
    ("打", "架", "ㄉㄚˇ", "ㄐㄧㄚˋ", "social"),
    ("道", "歉", "ㄉㄠˋ", "ㄑㄧㄢˇ", "social"),

    # 休閒娛樂與運動
    ("跑", "步", "ㄆㄠˇ", "ㄅㄨˋ", "leisure"),
    ("散", "步", "ㄙㄢˋ", "ㄅㄨˋ", "leisure"),
    ("唱", "歌", "ㄔㄤˋ", "ㄍㄜ", "leisure"),
    ("跳", "舞", "ㄊㄧㄠˋ", "ㄨˇ", "leisure"),
    ("游", "泳", "ㄧㄡˊ", "ㄩㄥˇ", "leisure"),
    ("打", "球", "ㄉㄚˇ", "ㄑㄧㄡˊ", "leisure"),
    ("爬", "山", "ㄆㄚˊ", "ㄕㄢ", "leisure"),
    ("逛", "街", "ㄍㄨㄤˋ", "ㄐㄧㄝ", "leisure"),
    ("釣", "魚", "ㄉㄧㄠˋ", "ㄩˊ", "leisure"),
    ("拍", "照", "ㄆㄞ", "ㄓㄠˋ", "leisure"),
    ("照", "相", "ㄓㄠˋ", "ㄒㄧㄤˋ", "leisure"),

    # 職場、就學與醫療公共
    ("請", "假", "ㄑㄧㄥˇ", "ㄐㄧㄚˋ", "work"),
    ("出", "差", "ㄔㄨ", "ㄔㄞ", "work"),
    ("開", "會", "ㄎㄞ", "ㄏㄨㄟˋ", "work"),
    ("打", "卡", "ㄉㄚˇ", "ㄎㄚˇ", "work"),
    ("上", "班", "ㄕㄤˋ", "ㄅㄢ", "work"),
    ("下", "班", "ㄒㄧㄚˋ", "ㄅㄢ", "work"),
    ("加", "班", "ㄐㄧㄚ", "ㄅㄢ", "work"),
    ("罷", "工", "ㄅㄚˋ", "ㄍㄨㄥ", "work"),
    ("辭", "職", "ㄘˊ", "ㄓˊ", "work"),
    ("考", "試", "ㄎㄠˇ", "ㄕˋ", "work"),
    ("排", "隊", "ㄆㄞˊ", "ㄉㄨㄟˋ", "public"),
    ("看", "病", "ㄎㄢˋ", "ㄅㄧㄥˋ", "public"),
    ("打", "針", "ㄉㄚˇ", "ㄓㄣ", "public"),
    ("吃", "藥", "ㄔ", "ㄧㄠˋ", "public"),
    ("住", "院", "ㄓㄨˋ", "ㄩㄢˋ", "public"),
    ("結", "帳", "ㄐㄧㄝˊ", "ㄓㄤˋ", "public"),
    ("買", "單", "ㄇㄞˇ", "ㄉㄢ", "public"),
    ("簽", "名", "ㄑㄧㄢ", "ㄇㄧㄥˊ", "public"),
    ("打", "仗", "ㄉㄚˇ", "ㄓㄤˋ", "public"),
]

# 2. 高頻動態動量短語（徹底封殺名詞搶字，如上衣下、洗衣下、睡衣下）
VERB_MOMENTUM_PHRASES = [
    ("上一下", "ㄕㄤˋ ㄧ ㄒㄧㄚˋ", 1850, "抑制上衣搶字"),
    ("洗一下", "ㄒㄧˇ ㄧ ㄒㄧㄚˋ", 1850, "抑制洗衣搶字"),
    ("睡一下", "ㄕㄨㄟˋ ㄧ ㄒㄧㄚˋ", 1850, "抑制睡衣搶字"),
    ("吃一下", "ㄔ ㄧ ㄒㄧㄚˋ", 1850, "常用動量短語"),
    ("看一下", "ㄎㄢˋ ㄧ ㄒㄧㄚˋ", 1850, "常用動量短語"),
    ("等一下", "ㄉㄥˇ ㄧ ㄒㄧㄚˋ", 1900, "超高頻動量短語"),
    ("走一下", "ㄗㄡˇ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("跑一下", "ㄆㄠˇ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("坐一下", "ㄗㄨㄛˋ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("聽一下", "ㄊㄧㄥ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("問一下", "ㄨㄣˋ ㄧ ㄒㄧㄚˋ", 1850, "常用動量短語"),
    ("聊一下", "ㄌㄧㄠˊ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("想一下", "ㄒㄧㄤˇ ㄧ ㄒㄧㄚˋ", 1850, "常用動量短語"),
    ("查一下", "ㄔㄚˊ ㄧ ㄒㄧㄚˋ", 1850, "常用動量短語"),
    ("試一下", "ㄕˋ ㄧ ㄒㄧㄚˋ", 1850, "常用動量短語"),
    ("幫一下", "ㄅㄤ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("碰一下", "ㄆㄥˋ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("轉一下", "ㄓㄨㄢˇ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("推一下", "ㄊㄨㄟ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
    ("拉一下", "ㄌㄚ ㄧ ㄒㄧㄚˋ", 1800, "常用動量短語"),
]

def generate_separable_entries():
    domain_entries = []
    transitions = []

    # 1. 登記動量動詞短語
    for word, bpmf, wt, note in VERB_MOMENTUM_PHRASES:
        domain_entries.append((word, bpmf, wt, "verb_momentum", note))

    # 2. 登記動詞離合前綴 (V + 個)
    unique_prefixes = set()
    for v, o, v_bpm, o_bpm, cat in SEPARABLE_ROOTS:
        prefix = f"{v}個"
        if prefix not in unique_prefixes:
            # 支援輕聲與四聲雙注音
            domain_entries.append((prefix, f"{v_bpm} ˙ㄍㄜ", 1700, "separable_prefix", f"{v}離合前綴(輕聲)"))
            domain_entries.append((prefix, f"{v_bpm} ㄍㄜˋ", 1700, "separable_prefix", f"{v}離合前綴(四聲)"))
            unique_prefixes.add(prefix)

    # 3. 登記完整離合式：V個O、V一下O、V個好O
    for v, o, v_bpm, o_bpm, cat in SEPARABLE_ROOTS:
        # V個O (三字或四字短語)
        v_ge_o = f"{v}個{o}"
        domain_entries.append((v_ge_o, f"{v_bpm} ˙ㄍㄜ {o_bpm}", 1650, "separable_phrase", f"{v}{o}離合式(輕聲)"))
        domain_entries.append((v_ge_o, f"{v_bpm} ㄍㄜˋ {o_bpm}", 1650, "separable_phrase", f"{v}{o}離合式(四聲)"))

        # V一下O
        v_yixia_o = f"{v}一下{o}"
        domain_entries.append((v_yixia_o, f"{v_bpm} ㄧ ㄒㄧㄚˋ {o_bpm}", 1650, "separable_phrase", f"{v}{o}動量離合式"))

        # 特殊高頻修飾型：睡個好覺、洗個好澡、洗個熱水澡、吃個飽飯、開個小會
        if (v, o) == ("睡", "覺"):
            domain_entries.append(("睡個好覺", "ㄕㄨㄟˋ ˙ㄍㄜ ㄏㄠˇ ㄐㄧㄠˋ", 1700, "separable_phrase", "睡覺修飾離合"))
            domain_entries.append(("睡個好覺", "ㄕㄨㄟˋ ㄍㄜˋ ㄏㄠˇ ㄐㄧㄠˋ", 1700, "separable_phrase", "睡覺修飾離合"))
            domain_entries.append(("睡個大覺", "ㄕㄨㄟˋ ˙ㄍㄜ ㄉㄚˋ ㄐㄧㄠˋ", 1600, "separable_phrase", "睡覺修飾離合"))
            domain_entries.append(("睡個大覺", "ㄕㄨㄟˋ ㄍㄜˋ ㄉㄚˋ ㄐㄧㄠˋ", 1600, "separable_phrase", "睡覺修飾離合"))
        elif (v, o) == ("洗", "澡"):
            domain_entries.append(("洗個好澡", "ㄒㄧˇ ˙ㄍㄜ ㄏㄠˇ ㄗㄠˇ", 1700, "separable_phrase", "洗澡修飾離合"))
            domain_entries.append(("洗個好澡", "ㄒㄧˇ ㄍㄜˋ ㄏㄠˇ ㄗㄠˇ", 1700, "separable_phrase", "洗澡修飾離合"))
            domain_entries.append(("洗個熱水澡", "ㄒㄧˇ ˙ㄍㄜ ㄖㄜˋ ㄕㄨㄟˇ ㄗㄠˇ", 1700, "separable_phrase", "洗澡修飾離合"))
            domain_entries.append(("洗個熱水澡", "ㄒㄧˇ ㄍㄜˋ ㄖㄜˋ ㄕㄨㄟˇ ㄗㄠˇ", 1700, "separable_phrase", "洗澡修飾離合"))
            domain_entries.append(("洗個痛快澡", "ㄒㄧˇ ˙ㄍㄜ ㄊㄨㄥˋ ㄎㄨㄞˋ ㄗㄠˇ", 1600, "separable_phrase", "洗澡修飾離合"))
        elif (v, o) == ("吃", "飯"):
            domain_entries.append(("吃個飽飯", "ㄔ ˙ㄍㄜ ㄅㄠˇ ㄈㄢˋ", 1650, "separable_phrase", "吃飯修飾離合"))
            domain_entries.append(("吃個飽飯", "ㄔ ㄍㄜˋ ㄅㄠˇ ㄈㄢˋ", 1650, "separable_phrase", "吃飯修飾離合"))
            domain_entries.append(("吃個好飯", "ㄔ ˙ㄍㄜ ㄏㄠˇ ㄈㄢˋ", 1650, "separable_phrase", "吃飯修飾離合"))
            domain_entries.append(("吃頓飯", "ㄔ ㄉㄨㄣˋ ㄈㄢˋ", 1900, "separable_phrase", "吃飯動量離合"))
            domain_entries.append(("吃頓", "ㄔ ㄉㄨㄣˋ", 1850, "separable_prefix", "吃頓離合前綴"))
            domain_entries.append(("頓飯", "ㄉㄨㄣˋ ㄈㄢˋ", 1850, "separable_noun", "量賓短語"))
            domain_entries.append(("吃一頓飯", "ㄔ ㄧ ㄉㄨㄣˋ ㄈㄢˋ", 1900, "separable_phrase", "吃飯動量離合"))
            domain_entries.append(("吃一頓", "ㄔ ㄧ ㄉㄨㄣˋ", 1850, "separable_phrase", "吃一頓動量"))
            domain_entries.append(("一頓飯", "ㄧ ㄉㄨㄣˋ ㄈㄢˋ", 1850, "separable_phrase", "一頓飯量賓"))
            domain_entries.append(("好好吃", "ㄏㄠˇ ㄏㄠˇ ㄔ", 1850, "verb_phrase", "副詞+動詞"))
            transitions.append(("好好", "吃頓飯", 48.0, "separable_frame", "好好吃頓飯搭配"))
            transitions.append(("好好", "吃", 45.0, "separable_frame", "好好吃搭配"))
            transitions.append(("吃", "頓飯", 48.0, "separable_frame", "吃頓飯搭配"))
            transitions.append(("吃頓", "飯", 48.0, "separable_frame", "吃頓飯搭配"))
        elif (v, o) == ("開", "會"):
            domain_entries.append(("開個小會", "ㄎㄞ ˙ㄍㄜ ㄒㄧㄠˇ ㄏㄨㄟˋ", 1600, "separable_phrase", "開會修飾離合"))
            domain_entries.append(("開個短會", "ㄎㄞ ˙ㄍㄜ ㄉㄨㄢˇ ㄏㄨㄟˋ", 1600, "separable_phrase", "開會修飾離合"))

        # 4. 建立遠程動賓框定與跨詞轉移 (Transitions)
        # 4a. 前綴轉移：洗個 -> 澡, 睡個 -> 覺, 吃個 -> 飯, 開個 -> 會, 吵個 -> 架, 打個 -> 仗
        transitions.append((f"{v}個", o, 40.0, "separable_frame", f"{v}個...{o}遠程框架"))
        # 4b. 尾部修飾轉移：的 -> 澡, 的 -> 覺, 的 -> 架, 的 -> 仗
        if o in ["澡", "覺", "架", "仗", "氣"]:
            transitions.append(("的", o, 36.0, "separable_suffix", f"修飾語後接{o}"))

    return domain_entries, transitions

def main():
    print("🚀 啟動【第一階段：漢語高頻動賓離合詞拓撲擴展引擎】...")

    domain_entries, transitions = generate_separable_entries()

    # 1. 讀取現有 lexicon_domain.tsv，去重追加
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

    print(f"✨ 成功注入 {added_domain} 條高階動量短語與離合詞式至 {DOMAIN_PATH}！")

    # 2. 讀取現有 transitions_user.tsv，去重追加
    existing_transitions = set()
    if TRANSITIONS_PATH.exists():
        with open(TRANSITIONS_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2 and not line.startswith("#"):
                    existing_transitions.add((p[0].strip(), p[1].strip()))

    added_trans = 0
    with open(TRANSITIONS_PATH, "a", encoding="utf-8") as f:
        for p_w, c_w, bonus, cat, note in transitions:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組動賓遠程框架轉移規則至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
