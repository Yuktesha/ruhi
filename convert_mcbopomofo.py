#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
McBopomofo → RuhI 詞庫轉換器
Converts McBopomofo BPMFMappings.txt + BPMFBase.txt + phrase.occ
into RuhI's lexicon TSV format: WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE
"""

import os
import sys
import math

sys.stdout.reconfigure(encoding='utf-8')

# === 路徑 ===
BASE = os.path.dirname(os.path.abspath(__file__))
MCBPMF_DIR = os.path.join(BASE, "_external", "McBopomofo", "Source", "Data")
OUTPUT = os.path.join(BASE, "ruhi_lexicon", "data", "lexicon_mcbopomofo.tsv")

# === 1. 載入詞頻 (phrase.occ) ===
print("載入 phrase.occ ...")
freq = {}
with open(os.path.join(MCBPMF_DIR, "phrase.occ"), "r", encoding="utf-8") as f:
    for line in f:
        parts = line.strip().split()
        if len(parts) >= 2:
            word = parts[0]
            try:
                count = int(parts[1])
                freq[word] = count
            except ValueError:
                pass

print(f"  詞頻載入：{len(freq)} 筆")

# === 2. 載入多字詞注音對照 (BPMFMappings.txt) ===
print("載入 BPMFMappings.txt ...")
mappings = {}  # word -> list of bopomofo strings
with open(os.path.join(MCBPMF_DIR, "BPMFMappings.txt"), "r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) >= 3:
            word = parts[0]
            # 注音是 parts[1:] 中的空格分隔注音
            bpmf = " ".join(parts[1:])
            if word not in mappings:
                mappings[word] = []
            if bpmf not in mappings[word]:
                mappings[word].append(bpmf)

print(f"  多字詞載入：{len(mappings)} 筆（含多音字變體）")

# === 3. 載入單字注音對照 (BPMFBase.txt) ===
print("載入 BPMFBase.txt ...")
single_chars = {}  # char -> list of bopomofo
with open(os.path.join(MCBPMF_DIR, "BPMFBase.txt"), "r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) >= 2:
            char = parts[0]
            bpmf = parts[1]
            if char not in single_chars:
                single_chars[char] = []
            if bpmf not in single_chars[char]:
                single_chars[char].append(bpmf)

print(f"  單字載入：{len(single_chars)} 筆")

# === 4. 計算權重 ===
# McBopomofo 用 ln(P) 機率，RuhI 用整數權重 0-1000
# 策略：用 phrase.occ 的出現次數做對數縮放到 1-1000 範圍
max_freq = max(freq.values()) if freq else 1
min_weight = 1
max_weight = 1000

def calc_weight(word, count=None):
    """從出現次數計算 RuhI 權重"""
    if count is None:
        count = freq.get(word, 0)
    if count <= 0:
        return 100  # 有注音但無詞頻的詞，給基礎權重
    # log scale: log(count) / log(max_freq) * 900 + 100
    log_ratio = math.log(count + 1) / math.log(max_freq + 1)
    return min(max_weight, max(min_weight, int(log_ratio * 900 + 100)))

# === 4.5 載入台灣教育部萌典基準 (lexicon_moe.tsv) 作為正音與破音主次排序權威 ===
MOE_PATH = os.path.join(BASE, "ruhi_lexicon", "data", "lexicon_moe.tsv")
moe_chars = {}
if os.path.exists(MOE_PATH):
    with open(MOE_PATH, "r", encoding="utf-8") as mf:
        for line in mf:
            if not line.strip() or line.startswith("#") or line.startswith("WORD\t"):
                continue
            parts = line.rstrip("\r\n").split("\t")
            if len(parts) >= 3 and parts[2].lstrip("-").isdigit():
                mw, mbpm, mwt = parts[0], parts[1].strip(), int(parts[2])
                if mbpm.endswith("˙"):
                    mbpm = "˙" + mbpm[:-1]
                if len(mw) == 1:
                    moe_chars.setdefault(mw, {})[mbpm] = max(mwt, moe_chars.get(mw, {}).get(mbpm, 0))

def norm_neutral(bpm):
    bpm = bpm.strip()
    return ("˙" + bpm[:-1]) if bpm.endswith("˙") else bpm

# === 5. 輸出 TSV ===
print(f"輸出至 {OUTPUT} ...")
os.makedirs(os.path.dirname(OUTPUT), exist_ok=True)
total = 0
seen = set()  # 去重用

with open(OUTPUT, "w", encoding="utf-8", newline="\n") as out:
    out.write("WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE\n")

    # 5a. 多字詞 (高優先)
    for word, bpmf_list in mappings.items():
        occ = freq.get(word, 0)
        w = calc_weight(word, occ)
        for i, bpmf in enumerate(bpmf_list):
            key = f"{word}\t{bpmf}"
            if key in seen:
                continue
            seen.add(key)
            adj_w = max(1, w - i * 50)
            note = f"occ={occ}" if occ > 0 else ""
            out.write(f"{word}\t{bpmf}\t{adj_w}\tmcbpmf\t{note}\n")
            total += 1

    # 5b. 單字：以教育部 lexicon_moe.tsv 過濾非標準異讀，並依 MOE 主次讀音高低決定權重與上限
    for char, bpmf_list in single_chars.items():
        occ = freq.get(char, 0)
        w = calc_weight(char, occ)
        moe_d = moe_chars.get(char)
        if moe_d:
            valid_bpmfs = [
                b for b in bpmf_list
                if norm_neutral(b) in moe_d or (char, b) in (("麼", "ㄇㄛ˙"), ("兒", "ㄦ"))
            ]
            valid_bpmfs.sort(key=lambda b: moe_d.get(norm_neutral(b), 300), reverse=True)
            max_moe = max(moe_d.values())
        else:
            valid_bpmfs = bpmf_list
            max_moe = 0

        for i, bpmf in enumerate(valid_bpmfs):
            key = f"{char}\t{bpmf}"
            if key in seen:
                continue
            seen.add(key)
            adj_w = max(1, w - i * 30)
            if moe_d:
                moe_wt = moe_d.get(norm_neutral(bpmf), adj_w)
                if moe_wt <= 285 and max_moe >= 640 and adj_w > moe_wt:
                    adj_w = moe_wt
            note = f"occ={occ}" if occ > 0 else "base"
            out.write(f"{char}\t{bpmf}\t{adj_w}\tmcbpmf_char\t{note}\n")
            total += 1

print(f"\n✅ 完成！共輸出 {total} 筆。")
print(f"  輸出檔案：{OUTPUT}")
print(f"  檔案大小：{os.path.getsize(OUTPUT):,} bytes")

# === 統計 ===
with_freq = sum(1 for w in mappings if w in freq)
print(f"\n📊 統計：")
print(f"  多字詞有詞頻的：{with_freq}/{len(mappings)} ({with_freq*100//len(mappings)}%)")
print(f"  phrase.occ 總筆數：{len(freq)}")
print(f"  最高詞頻：{max_freq}")
