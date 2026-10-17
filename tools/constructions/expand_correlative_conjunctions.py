#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_correlative_conjunctions.py
=============================================================================
第五階段：複句關聯詞拓撲與語意配對引力場 (Correlative Conjunction Topology)
依據呂叔湘《現代漢語八百詞》關聯詞篇、劉月華《實用現代漢語語法》複句關係論。

核心解決痛點：
1. 超高頻關聯詞破音與字頻失位：
   - 『不但』：補齊變調音讀『ㄅㄨˊ ㄉㄢˋ』（杜絕被『不淡』語意劫掠）。
   - 『而且』：補齊『ㄦˊ ㄑㄧㄝˇ』頂級連詞（杜絕碎成『兒且』）。
   - 『我也/你也/他也』：補齊代詞副詞複合體（杜絕『我野』）。
2. 高頻語境名詞加權校準：
   - 『過程』(1850 vs 過成)、『學歷』(1850 vs 學力)、『行程』(1850 vs 形成)、『一搏』(1800 奮力一搏 vs 一薄)。
3. 六大複句語意配對引力場 (Correlative Paired Gravity Transitions)：
   - 因果複句：因為 ➔ 所以、由於 ➔ 因此、既然 ➔ 就。
   - 轉折複句：雖然 ➔ 但是/可是/不過、儘管 ➔ 還是。
   - 假設條件：只要 ➔ 就、只有 ➔ 才、如果 ➔ 就、要是 ➔ 就、除非 ➔ 否則/不然。
   - 遞進讓步：不但 ➔ 而且、不僅 ➔ 而且/甚至、即使 ➔ 也、哪怕 ➔ 也、即便 ➔ 也。
   - 選擇複句：與其 ➔ 不如、寧可 ➔ 也不、寧願 ➔ 也不。
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

# 1. 關聯詞與高頻語法詞補充 (Domain Entries)
CONJUNCTION_DOMAIN_ENTRIES = [
    # 遞進與轉折連詞
    ("而且", "ㄦˊ ㄑㄧㄝˇ", 2000, "conjunction", "超高頻遞進連詞"),
    ("不但", "ㄅㄨˊ ㄉㄢˋ", 2000, "conjunction", "超高頻遞進連詞(變調二聲)"),
    ("不但", "ㄅㄨˋ ㄉㄢˋ", 2000, "conjunction", "超高頻遞進連詞(本調四聲)"),
    ("不但如此", "ㄅㄨˊ ㄉㄢˋ ㄖㄨˊ ㄘˇ", 1900, "conjunction", "遞進轉折短語"),
    ("不但如此", "ㄅㄨˋ ㄉㄢˋ ㄖㄨˊ ㄘˇ", 1900, "conjunction", "遞進轉折短語"),
    ("不僅", "ㄅㄨˋ ㄐㄧㄣˇ", 1950, "conjunction", "遞進連詞"),
    ("不論", "ㄅㄨˊ ㄌㄨㄣˋ", 1900, "conjunction", "條件連詞(變調)"),
    ("不論", "ㄅㄨˋ ㄌㄨㄣˋ", 1900, "conjunction", "條件連詞"),
    ("即便", "ㄐㄧˊ ㄅㄧㄢˋ", 1900, "conjunction", "讓步連詞"),
    ("即便", "ㄐㄧˋ ㄅㄧㄢˋ", 1900, "conjunction", "讓步連詞(去聲讀)"),
    ("哪怕", "ㄋㄚˇ ㄆㄚˋ", 1900, "conjunction", "讓步連詞(上聲)"),
    ("哪怕", "ㄋㄚˋ ㄆㄚˋ", 1900, "conjunction", "讓步連詞(去聲)"),

    # 代詞 + 副詞/助動詞高頻塊 (Pronoun + Adverb Collocations)
    ("我也", "ㄨㄛˇ ㄧㄝˇ", 1950, "pronoun_adverb", "代詞副詞複合體"),
    ("你也", "ㄋㄧˇ ㄧㄝˇ", 1950, "pronoun_adverb", "代詞副詞複合體"),
    ("他也", "ㄊㄚ ㄧㄝˇ", 1950, "pronoun_adverb", "代詞副詞複合體"),
    ("我也要", "ㄨㄛˇ ㄧㄝˇ ㄧㄠˋ", 1900, "phrase", "代詞副詞短語"),
    ("我也會", "ㄨㄛˇ ㄧㄝˇ ㄏㄨㄟˋ", 1900, "phrase", "代詞副詞短語"),
    ("我也想", "ㄨㄛˇ ㄧㄝˇ ㄒㄧㄤˇ", 1900, "phrase", "代詞副詞短語"),
    ("我也能", "ㄨㄛˇ ㄧㄝˇ ㄋㄥˊ", 1900, "phrase", "代詞副詞短語"),
    ("我也是", "ㄨㄛˇ ㄧㄝˇ ㄕˋ", 1900, "phrase", "代詞副詞短語"),

    # 高頻易混淆核心名詞與成語補強
    ("過程", "ㄍㄨㄛˋ ㄔㄥˊ", 1950, "noun_abstract", "高頻抽象名詞(防過成)"),
    ("學歷", "ㄒㄩㄝˊ ㄌㄧˋ", 1900, "noun_abstract", "教育背景名詞(防學力)"),
    ("行程", "ㄒㄧㄥˊ ㄔㄥˊ", 1900, "noun_abstract", "旅遊交通名詞(防形成)"),
    ("一搏", "ㄧ ㄅㄛˊ", 1850, "verb_object", "奮力一搏(防一薄)"),
    ("奮力一搏", "ㄈㄣˋ ㄌㄧˋ ㄧ ㄅㄛˊ", 2000, "idiom", "成語"),
    ("取消了", "ㄑㄩˇ ㄒㄧㄠ ˙ㄌㄜ", 1850, "verb_aspect", "完成體動詞"),

    # 語法修辭防撞核心詞
    ("困難", "ㄎㄨㄣˋ ㄋㄢˊ", 1950, "noun_abstract", "高頻抽象名詞(二聲讀音)"),
    ("困難", "ㄎㄨㄣˋ ˙ㄋㄢ", 1950, "noun_abstract", "高頻抽象名詞(輕聲讀音)"),
    ("自己", "ㄗˋ ㄐㄧˇ", 2000, "pronoun", "高頻人稱代詞(防自給)"),
    ("放棄", "ㄈㄤˋ ㄑㄧˋ", 1950, "verb", "高頻動詞(防放氣)"),
    ("絕不", "ㄐㄩㄝˊ ㄅㄨˋ", 1950, "adverb", "高頻否定副詞(防覺不)"),
    ("寧可", "ㄋㄧㄥˋ ㄎㄜˇ", 1900, "conjunction", "選擇連詞(四聲)"),
    ("寧可", "ㄋㄧㄥˊ ㄎㄜˇ", 1900, "conjunction", "選擇連詞(二聲)"),
    ("便宜", "ㄆㄧㄢˊ ㄧˊ", 1900, "adjective", "形容詞(便宜)"),
    ("便宜", "ㄆㄧㄢˊ ˙ㄧ", 1900, "adjective", "形容詞(輕聲)"),
    ("佔便宜", "ㄓㄢˋ ㄆㄧㄢˊ ㄧˊ", 1950, "phrase", "動賓短語"),
    ("也不", "ㄧㄝˇ ㄅㄨˋ", 1900, "adverb", "副詞短語(防野不)"),
    ("就", "ㄐㄧㄡˋ", 1950, "adverb", "高頻副詞(防舊)"),
    ("重重", "ㄔㄨㄥˊ ㄔㄨㄥˊ", 1900, "adjective", "疊詞形容詞"),
    ("來了", "ㄌㄞˊ ˙ㄌㄜ", 1900, "verb_aspect", "完成體動詞(防碎字)"),
    ("好好", "ㄏㄠˇ ㄏㄠˇ", 1950, "adverb", "高頻副詞(好好，防舊好)"),
    ("就好好", "ㄐㄧㄡˋ ㄏㄠˇ ㄏㄠˇ", 1900, "phrase", "副詞短語"),
    ("放鬆", "ㄈㄤˋ ㄙㄨㄥ", 1950, "verb", "高頻動詞"),
    ("一下", "ㄧ ㄒㄧㄚˋ", 1950, "measure", "高頻時量詞"),
]

# 2. 複句對稱引力轉移矩陣 (Correlative Paired Gravity Transitions)
CORRELATIVE_TRANSITIONS = [
    # ── 遞進與讓步 ──
    ("不但", "而且", 50.0, "correlative_pair", "不但...而且配對"),
    ("不僅", "而且", 50.0, "correlative_pair", "不僅...而且配對"),
    ("不僅", "甚至", 48.0, "correlative_pair", "不僅...甚至配對"),
    ("即使", "也", 48.0, "correlative_pair", "即使...也配對"),
    ("哪怕", "也", 48.0, "correlative_pair", "哪怕...也配對"),
    ("即便", "也", 48.0, "correlative_pair", "即便...也配對"),
    ("下雨", "我也", 45.0, "correlative_pair", "下雨...我也搭配"),

    # ── 因果與轉折 ──
    ("因為", "所以", 50.0, "correlative_pair", "因為...所以配對"),
    ("由於", "因此", 50.0, "correlative_pair", "由於...因此配對"),
    ("既然", "就", 48.0, "correlative_pair", "既然...就配對"),
    ("雖然", "但是", 50.0, "correlative_pair", "雖然...但是配對"),
    ("雖然", "可是", 48.0, "correlative_pair", "雖然...可是配對"),
    ("雖然", "不過", 48.0, "correlative_pair", "雖然...不過配對"),
    ("儘管", "還是", 48.0, "correlative_pair", "儘管...還是配對"),

    # ── 條件與假設 ──
    ("只要", "就", 50.0, "correlative_pair", "只要...就配對"),
    ("只有", "才", 50.0, "correlative_pair", "只有...才配對"),
    ("如果", "就", 48.0, "correlative_pair", "如果...就配對"),
    ("要是", "就", 48.0, "correlative_pair", "要是...就配對"),
    ("除非", "否則", 50.0, "correlative_pair", "除非...否則配對"),
    ("除非", "不然", 48.0, "correlative_pair", "除非...不然配對"),

    # ── 選擇 ──
    ("與其", "不如", 50.0, "correlative_pair", "與其...不如配對"),
    ("寧可", "也不", 48.0, "correlative_pair", "寧可...也不配對"),
    ("寧願", "也不", 48.0, "correlative_pair", "寧願...也不配對"),

    # ── 語境短語搭配 ──
    ("取消", "行程", 48.0, "collocation", "取消行程搭配"),
    ("取消了", "行程", 50.0, "collocation", "取消了行程搭配"),
    ("奮力", "一搏", 48.0, "collocation", "奮力一搏搭配"),
    ("結果", "很甜美", 45.0, "collocation", "結果很甜美搭配"),
    ("學歷", "很高", 45.0, "collocation", "學歷很高搭配"),
    ("雖然", "過程", 48.0, "collocation", "雖然...過程搭配"),
    ("過程", "很辛苦", 48.0, "collocation", "過程很辛苦搭配"),
    ("既然", "來了", 48.0, "collocation", "既然來了搭配"),
    ("來了", "就", 50.0, "collocation", "來了就搭配"),
    ("了", "就", 48.0, "collocation", "了就搭配"),
    ("就", "好好", 50.0, "collocation", "就好好搭配"),
    ("好好", "放鬆", 50.0, "collocation", "好好放鬆搭配"),
    ("放鬆", "一下", 50.0, "collocation", "放鬆一下搭配"),
    ("來了", "舊好", -50.0, "anti_confusion", "防舊好誤匹"),
    ("了", "舊好", -50.0, "anti_confusion", "防舊好誤匹"),
    ("哪怕", "困難", 48.0, "collocation", "哪怕困難搭配"),
    ("困難", "重重", 50.0, "collocation", "困難重重搭配"),
    ("重重", "我也", 48.0, "collocation", "重重我也搭配"),
    ("我也", "絕不", 48.0, "collocation", "我也絕不搭配"),
    ("絕不", "放棄", 50.0, "collocation", "絕不放棄搭配"),
    ("否則", "我", 45.0, "collocation", "否則我搭配"),
    ("我", "絕不", 48.0, "collocation", "我絕不搭配"),
    ("絕不", "相信", 50.0, "collocation", "絕不相信搭配"),
    ("寧可", "自己", 48.0, "collocation", "寧可自己搭配"),
    ("自己", "吃虧", 50.0, "collocation", "自己吃虧搭配"),
    ("吃虧", "也不", 50.0, "collocation", "吃虧也不搭配"),
    ("也不", "佔便宜", 50.0, "collocation", "也不佔便宜搭配"),
    ("也不", "佔", 48.0, "collocation", "也不佔搭配"),
    ("佔人", "便宜", 50.0, "collocation", "佔人便宜搭配"),
]

def main():
    print("🚀 啟動【第五階段：複句關聯詞拓撲與語意配對引力場】擴展...")

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
        for w, bpm, wt, cat, note in CONJUNCTION_DOMAIN_ENTRIES:
            if (w, bpm) not in existing_domain_words:
                f.write(f"{w}\t{bpm}\t{wt}\t{cat}\t{note}\n")
                existing_domain_words.add((w, bpm))
                added_domain += 1

    print(f"✨ 成功注入 {added_domain} 條高階關聯詞與語意防撞核心詞至 {DOMAIN_PATH}！")

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
        for p_w, c_w, bonus, cat, note in CORRELATIVE_TRANSITIONS:
            if (p_w, c_w) not in existing_transitions:
                f.write(f"{p_w}\t{c_w}\t{bonus:.1f}\t{cat}\t{note}\n")
                existing_transitions.add((p_w, c_w))
                added_trans += 1

    print(f"✨ 成功注入 {added_trans} 組複句對稱引力轉移規則至 {TRANSITIONS_PATH}！")

if __name__ == "__main__":
    main()
