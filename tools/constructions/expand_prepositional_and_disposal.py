#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_prepositional_and_disposal.py
=============================================================================
第六階段：介詞框式結構與處置/被動引力場 (Prepositional Framing & Disposal/Passive)
依據呂叔湘《現代漢語八百詞》介詞篇、劉月華《實用現代漢語語法》處置與被動式。

核心解決痛點：
1. 被動句頂級介詞『被』(ㄅㄟˋ 2000) 權重失位（原 MOE 619 被『背』641 徹底奪舍）。
2. 處置式動詞補語與名詞搭配：
   - 『做好』(1950 vs 作好)
   - 『寫完』(1950 vs 血完)
   - 『打掃乾淨』(1950)
   - 『桌上』(1950 vs 捉上)
   - 『放回』(1900)
   - 『靜下來』(1950 防 心境下來 跨詞誤拼)
3. 被動式深層動賓搭配：
   - 『深深吸引』(1950)
   - 『打敗』(1950)
   - 『淋濕』(1950)
   - 『打動』(1950)
   - 『真誠』(1950 vs 真成)
4. 框式介詞對稱引力（在……之中/之內/之下/之外/之間）：
   - 『時間』(1950 防 時兼)
   - 『之內』(1950)、『之中』(1950)、『之下』(1950)、『之外』(1950)、『之間』(1950)
5. 介引結構（對……而言/來說、從……開始、當……的時候）：
   - 『而言』(1950 防 兒言)
   - 『是個』(1950 防 誓個)
   - 『好消息』(1950)
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

# 1. 介詞、處置式、被動式與框式核心詞庫 (Domain Entries)
PREPOSITION_DOMAIN_ENTRIES = [
    # ── 被動標記核心詞（徹底根除「背」奪舍） ──
    ("被", "ㄅㄟˋ", 2000, "preposition", "被動式核心介詞(防背/倍)"),
    ("被大家", "ㄅㄟˋ ㄉㄚˋ ㄐㄧㄚ", 1950, "phrase", "被動短語"),

    # ── 處置式標記與核心動補短語 ──
    ("把", "ㄅㄚˇ", 2000, "preposition", "處置式核心介詞(防拔/八)"),
    ("做好", "ㄗㄨㄛˋ ㄏㄠˇ", 1950, "verb_complement", "動補短語(做好，防作好)"),
    ("寫完", "ㄒㄧㄝˇ ㄨㄢˊ", 1950, "verb_complement", "動補短語(寫完，防血完)"),
    ("寫", "ㄒㄧㄝˇ", 1950, "verb", "高頻動詞(防血)"),
    ("打掃", "ㄉㄚˇ ㄙㄠˇ", 1950, "verb", "高頻動詞"),
    ("乾淨", "ㄍㄢ ㄐㄧㄥˋ", 1950, "adjective", "高頻形容詞"),
    ("放回", "ㄈㄤˋ ㄏㄨㄟˊ", 1900, "verb_complement", "趨向動補"),
    ("桌上", "ㄓㄨㄛ ㄕㄤˋ", 1950, "noun_locative", "方位名詞(防捉上)"),
    ("靜下來", "ㄐㄧㄥˋ ㄒㄧㄚˋ ㄌㄞˊ", 1950, "verb_complement", "趨向動補(防心境)"),
    ("解決掉", "ㄐㄧㄝˇ ㄐㄩㄝˊ ㄉㄧㄠˋ", 1950, "verb_complement", "動補短語"),
    ("打破了", "ㄉㄚˇ ㄆㄛˋ ˙ㄌㄜ", 1900, "verb_complement", "動補完成體"),

    # ── 被動式動態與受事核心詞 ──
    ("深深", "ㄕㄣ ㄕㄣ", 1950, "adverb", "疊詞副詞"),
    ("吸引", "ㄒㄧ ㄧㄣˇ", 1950, "verb", "高頻動詞"),
    ("打敗", "ㄉㄚˇ ㄅㄞˋ", 1950, "verb", "高頻動詞"),
    ("淋濕", "ㄌㄧㄣˊ ㄕ", 1950, "verb_complement", "動補短語(正體淋濕)"),
    ("打動", "ㄉㄚˇ ㄉㄨㄥˋ", 1950, "verb", "高頻動詞"),
    ("真誠", "ㄓㄣ ㄔㄥˊ", 1950, "adjective", "形容詞(防真成)"),
    ("對手", "ㄉㄨㄟˋ ㄕㄡˇ", 1950, "noun", "名詞"),
    ("超越", "ㄔㄠ ㄩㄝˋ", 1950, "verb", "高頻動詞"),
    ("感動", "ㄍㄢˇ ㄉㄨㄥˋ", 1950, "verb", "高頻動詞"),

    # ── 框式介詞名詞與方位詞（在……之X） ──
    ("時間", "ㄕˊ ㄐㄧㄢ", 1950, "noun_abstract", "高頻抽象名詞(防時兼)"),
    ("之內", "ㄓ ㄋㄟˋ", 1950, "preposition_frame", "框式方位後置詞"),
    ("之中", "ㄓ ㄓㄨㄥ", 1950, "preposition_frame", "框式方位後置詞"),
    ("之下", "ㄓ ㄒㄧㄚˋ", 1950, "preposition_frame", "框式方位後置詞"),
    ("之上", "ㄓ ㄕㄤˋ", 1950, "preposition_frame", "框式方位後置詞"),
    ("之外", "ㄓ ㄨㄞˋ", 1950, "preposition_frame", "框式方位後置詞"),
    ("之間", "ㄓ ㄐㄧㄢ", 1950, "preposition_frame", "框式方位後置詞"),
    ("心中", "ㄒㄧㄣ ㄓㄨㄥ", 1950, "noun_locative", "方位名詞"),
    ("彼此", "ㄅㄧˇ ㄘˇ", 1950, "pronoun", "相互代詞"),

    # ── 介引結構核心詞 ──
    ("而言", "ㄦˊ ㄧㄢˊ", 1950, "idiom_conjunction", "介引連詞(防兒言)"),
    ("是個", "ㄕˋ ㄍㄜˋ", 1950, "phrase", "繫量短語(防誓個)"),
    ("好消息", "ㄏㄠˇ ㄒㄧㄠ ㄒㄧˊ", 1950, "noun_phrase", "正體名詞短語"),
    ("好消息", "ㄏㄠˇ ㄒㄧㄠ ˙ㄒㄧ", 1950, "noun_phrase", "名詞短語(輕聲讀)"),
    ("全力以赴", "ㄑㄩㄢˊ ㄌㄧˋ ㄧˇ ㄈㄨˋ", 2000, "idiom", "高頻成語"),
    ("當", "ㄉㄤ", 1950, "preposition", "時間介詞"),
    ("灑下", "ㄙㄚˇ ㄒㄧㄚˋ", 1900, "verb_complement", "動補短語"),
]

# 2. 介詞框式與處置/被動引力轉移矩陣 (Preposition & Voice Gravity Transitions)
PREPOSITION_TRANSITIONS = [
    # ── 把字句（處置式引力場） ──
    ("把", "事情", 50.0, "disposal_pair", "把事情搭配"),
    ("事情", "做好", 50.0, "disposal_pair", "事情做好搭配"),
    ("把", "作業", 50.0, "disposal_pair", "把作業搭配"),
    ("作業", "寫完", 50.0, "disposal_pair", "作業寫完搭配"),
    ("把", "房間", 50.0, "disposal_pair", "把房間搭配"),
    ("房間", "打掃", 50.0, "disposal_pair", "房間打掃搭配"),
    ("打掃", "乾淨", 50.0, "disposal_pair", "打掃乾淨搭配"),
    ("把", "杯子", 50.0, "disposal_pair", "把杯子搭配"),
    ("杯子", "打破了", 50.0, "disposal_pair", "杯子打破了搭配"),
    ("把", "書", 50.0, "disposal_pair", "把書搭配"),
    ("書", "放回", 50.0, "disposal_pair", "書放回搭配"),
    ("放回", "桌上", 50.0, "disposal_pair", "放回桌上搭配"),
    ("把", "問題", 50.0, "disposal_pair", "把問題搭配"),
    ("問題", "解決掉", 50.0, "disposal_pair", "問題解決掉搭配"),
    ("把", "心", 50.0, "disposal_pair", "把心搭配"),
    ("心", "靜下來", 50.0, "disposal_pair", "心靜下來搭配"),

    # ── 被字句（被動式引力場） ──
    ("被", "大家", 50.0, "passive_pair", "被大家搭配"),
    ("大家", "深深", 48.0, "passive_pair", "大家深深搭配"),
    ("深深", "吸引", 50.0, "passive_pair", "深深吸引搭配"),
    ("被", "困難", 50.0, "passive_pair", "被困難搭配"),
    ("困難", "打敗", 50.0, "passive_pair", "困難打敗搭配"),
    ("被", "雨水", 50.0, "passive_pair", "被雨水搭配"),
    ("雨水", "淋濕", 50.0, "passive_pair", "雨水淋濕搭配"),
    ("被", "熱情", 50.0, "passive_pair", "被熱情搭配"),
    ("熱情", "打動", 50.0, "passive_pair", "熱情打動搭配"),
    ("被", "對手", 50.0, "passive_pair", "被對手搭配"),
    ("對手", "超越", 50.0, "passive_pair", "對手超越搭配"),
    ("被", "真誠", 50.0, "passive_pair", "被真誠搭配"),
    ("真誠", "感動", 50.0, "passive_pair", "真誠感動搭配"),

    # ── 框式介詞（在……之X 引力場） ──
    ("在", "不知不覺", 50.0, "frame_pair", "在不知不覺搭配"),
    ("不知不覺", "之中", 50.0, "frame_pair", "不知不覺之中搭配"),
    ("在", "大家", 48.0, "frame_pair", "在大家搭配"),
    ("共同", "努力", 50.0, "frame_pair", "共同努力搭配"),
    ("努力", "之下", 50.0, "frame_pair", "努力之下搭配"),
    ("在", "有限", 48.0, "frame_pair", "在有限搭配"),
    ("有限的", "時間", 50.0, "frame_pair", "有限的時間搭配"),
    ("時間", "之內", 50.0, "frame_pair", "時間之內搭配"),
    ("在", "情理", 48.0, "frame_pair", "在情理搭配"),
    ("情理", "之外", 50.0, "frame_pair", "情理之外搭配"),
    ("在", "彼此", 48.0, "frame_pair", "在彼此搭配"),
    ("彼此的", "心中", 50.0, "frame_pair", "彼此的心中搭配"),

    # ── 介引結構（對……而言/來說，從……開始） ──
    ("對", "我來說", 50.0, "intro_pair", "對我來說搭配"),
    ("我來說", "非常", 48.0, "intro_pair", "我來說非常搭配"),
    ("對", "大家", 48.0, "intro_pair", "對大家搭配"),
    ("大家", "而言", 50.0, "intro_pair", "大家而言搭配"),
    ("而言", "是個", 50.0, "intro_pair", "而言是個搭配"),
    ("是個", "好消息", 50.0, "intro_pair", "是個好消息搭配"),
    ("從", "現在", 48.0, "intro_pair", "從現在搭配"),
    ("現在", "開始", 50.0, "intro_pair", "現在開始搭配"),
    ("開始", "全力以赴", 50.0, "intro_pair", "開始全力以赴搭配"),
    ("當", "陽光", 48.0, "intro_pair", "當陽光搭配"),
    ("陽光", "灑下", 50.0, "intro_pair", "陽光灑下搭配"),
    ("灑下的", "時候", 50.0, "intro_pair", "灑下的時候搭配"),

    # ── 易混淆詞反向抑制 (Anti-Confusion Penalties) ──
    ("把", "抒", -50.0, "anti_confusion", "防把抒誤拼"),
    ("放回", "捉上", -50.0, "anti_confusion", "防捉上誤拼"),
    ("有限的", "時兼", -50.0, "anti_confusion", "防時兼誤拼"),
    ("大家", "兒言", -50.0, "anti_confusion", "防兒言誤拼"),
    ("而言", "誓個", -50.0, "anti_confusion", "防誓個誤拼"),
    ("心境", "下來", -50.0, "anti_confusion", "防心境下來誤拼"),
    ("被", "真成", -50.0, "anti_confusion", "防真成誤拼"),
]

def main():
    print("🚀 啟動【第六階段：介詞框式結構與處置/被動引力場】擴展...")

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
        for w, bpm, wt, cat, note in PREPOSITION_DOMAIN_ENTRIES:
            if (w, bpm) not in existing_domain_words:
                f.write(f"{w}\t{bpm}\t{wt}\t{cat}\t{note}\n")
                existing_domain_words.add((w, bpm))
                added_domain += 1

    print(f"✨ 成功注入 {added_domain} 條介詞/處置/被動核心詞至 {DOMAIN_PATH}！")

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
        for p_w, c_w, bonus, cat, note in PREPOSITION_TRANSITIONS:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組介詞框式與處置/被動引力轉移矩陣至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
