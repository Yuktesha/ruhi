#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/expand_invertible_idioms.py
=============================================================================
對稱並列倒裝成語（互文、對偶、主謂/動賓互文）系統性自動補全

漢語四字成語中存在高度嚴謹的對稱互文結構：
- [名₁ + 形/動₁] + [名₂ + 形/動₂]：天經地義 <-> 地義天經、兄友弟恭 <-> 弟恭兄友、父慈子孝 <-> 子孝父慈
- [動₁ + 賓₁] + [動₂ + 賓₂]：驚天動地 <-> 動地驚天、移山倒海 <-> 倒海移山
- [定₁ + 名₁] + [定₂ + 名₂]：良辰美景 <-> 美景良辰、奇花異草 <-> 異草奇花
- [名₁ + 名₂] + [名₃ + 名₄]：山盟海誓 <-> 海誓山盟、琴棋書畫 <-> 詩詞歌賦

傳統辭典常因編纂習慣僅收錄單向，導致輸入法在處理氣韻倒裝作文時嚴重碎詞。
本腳本分析現有詞庫，自動補全其倒裝形式，並賦予標準成語權重（1200）。
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
MOE_PATH = LEXICON_DIR / "lexicon_moe.tsv"
DOMAIN_PATH = LEXICON_DIR / "lexicon_domain.tsv"
TRANSITIONS_PATH = LEXICON_DIR / "transitions_user.tsv"

ANTITHETICAL_PAIRS = [
    ('天', '地'), ('父', '子'), ('兄', '弟'), ('山', '水'), ('山', '川'),
    ('風', '雨'), ('日', '月'), ('春', '秋'), ('生', '死'), ('陰', '陽'),
    ('前', '後'), ('左', '右'), ('東', '西'), ('南', '北'), ('始', '終'),
    ('內', '外'), ('首', '尾'), ('頭', '尾'), ('眉', '目'), ('心', '意'),
    ('身', '心'), ('魂', '魄'), ('德', '才'), ('功', '過'), ('智', '勇'),
    ('忠', '義'), ('悲', '喜'), ('苦', '樂'), ('榮', '辱'), ('恩', '怨'),
    ('吉', '凶'), ('禍', '福'), ('善', '惡'), ('虛', '實'), ('動', '靜'),
    ('剛', '柔'), ('貧', '富'), ('尊', '卑'), ('清', '濁'), ('古', '今'),
    ('明', '暗'), ('真', '假'), ('深', '淺'), ('輕', '重'), ('長', '短'),
    ('大', '小'), ('高', '低'), ('遠', '近'), ('粗', '細'), ('損', '益'),
    ('出', '入'), ('進', '退'), ('升', '降'), ('安', '危'), ('存', '亡'),
    ('家', '戶'), ('經', '緯'), ('綱', '領'), ('律', '度'), ('言', '行'),
    ('花', '草'), ('珠', '寶'), ('金', '玉'), ('鳥', '獸'), ('草', '木')
]

def main():
    print("📖 正在掃描並提煉對稱並列倒裝成語...")

    moe_entries = {}
    with open(MOE_PATH, "r", encoding="utf-8") as f:
        for line in f:
            p = line.strip().split("\t")
            if len(p) >= 2:
                moe_entries[p[0]] = p[1]

    domain_entries = {}
    if DOMAIN_PATH.exists():
        with open(DOMAIN_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2:
                    domain_entries[p[0]] = p[1]

    all_vocab = set(moe_entries.keys()) | set(domain_entries.keys())

    inverted_to_add = []
    transitions_to_add = []

    for w, bpmf in moe_entries.items():
        if len(w) == 4:
            c1, c2, c3, c4 = w[0], w[1], w[2], w[3]
            syls = bpmf.split()
            if len(syls) != 4:
                continue
            for p1, p2 in ANTITHETICAL_PAIRS:
                if (c1 == p1 and c3 == p2) or (c1 == p2 and c3 == p1):
                    inv = w[2:] + w[:2]
                    if inv not in all_vocab:
                        inv_bpmf = f"{syls[2]} {syls[3]} {syls[0]} {syls[1]}"
                        inverted_to_add.append((inv, inv_bpmf, 1200, "idiom_inverted", f"{w}對稱倒裝成語"))
                        all_vocab.add(inv)
                        # 為前後半二字詞建立銜接雙向轉移
                        part1, part2 = w[:2], w[2:]
                        transitions_to_add.append((part2, part1, 350, "inverted_idiom", f"{inv}跨詞轉移"))

    print(f"✨ 成功提煉 {len(inverted_to_add)} 條對稱倒裝成語！")
    print(f"✨ 同步生成 {len(transitions_to_add)} 組跨詞轉移規則！")

    # 1. 寫入 lexicon_domain.tsv
    with open(DOMAIN_PATH, "a", encoding="utf-8") as f:
        for w, bpmf, wt, cat, note in inverted_to_add:
            f.write(f"{w}\t{bpmf}\t{wt}\t{cat}\t{note}\n")
    print(f"✅ 已成功將 {len(inverted_to_add)} 條倒裝成語寫入 {DOMAIN_PATH}")

    # 2. 寫入 transitions_user.tsv
    existing_trans = set()
    if TRANSITIONS_PATH.exists():
        with open(TRANSITIONS_PATH, "r", encoding="utf-8") as f:
            for line in f:
                p = line.strip().split("\t")
                if len(p) >= 2:
                    existing_trans.add((p[0], p[1]))

    new_trans_count = 0
    with open(TRANSITIONS_PATH, "a", encoding="utf-8") as f:
        for p, c, b, cat, note in transitions_to_add:
            if (p, c) not in existing_trans:
                f.write(f"{p}\t{c}\t{b}\t{cat}\t{note}\n")
                existing_trans.add((p, c))
                new_trans_count += 1
    print(f"✅ 已成功將 {new_trans_count} 條倒裝成語轉移規則注入 {TRANSITIONS_PATH}")

    # 測試代表性範例
    test_samples = ["地義天經", "弟恭兄友", "子孝父慈", "地設天造", "戶曉家喻", "地轉天旋", "地暗天昏"]
    print("\n🔍 抽樣驗證已注入成語：")
    for s in test_samples:
        status = "✅ 存在" if s in all_vocab else "❌ 缺失"
        print(f"  {s}: {status}")

if __name__ == "__main__":
    main()
