# -*- coding: utf-8 -*-
"""
audit_homophones.py - 全音節同音字權重倒掛排查系統
比對 phrase.occ 台灣客觀語料詞頻與 RuhI 現有權重，揪出所有先驗倒掛異常。
"""
import os
import sys
from collections import defaultdict

sys.stdout.reconfigure(encoding='utf-8')

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
DATA_DIR = os.path.join(BASE_DIR, "data")
OCC_PATH = os.path.join(BASE_DIR, "..", "_external", "McBopomofo", "Source", "Data", "phrase.occ")

def load_phrase_occ():
    occ = {}
    if not os.path.exists(OCC_PATH):
        print(f"Warning: {OCC_PATH} does not exist!")
        return occ
    with open(OCC_PATH, "r", encoding="utf-8") as f:
        for line in f:
            parts = line.strip().split()
            if len(parts) >= 2:
                occ[parts[0]] = int(parts[1])
    return occ

def normalize_bpm(bpm: str) -> str:
    # 如喜內部輕聲標準化：結尾 ˙ 轉開頭 ˙
    if bpm.endswith("˙"):
        return "˙" + bpm[:-1]
    return bpm

def load_all_unigrams():
    # syllable -> {char: max_weight}
    syllables = defaultdict(dict)
    
    files = [
        ("lexicon_mcbopomofo.tsv", "mc"),
        ("lexicon_moe.tsv", "moe"),
        ("lexicon_domain.tsv", "dom")
    ]
    
    for fname, src in files:
        fpath = os.path.join(DATA_DIR, fname)
        if not os.path.exists(fpath):
            continue
        with open(fpath, "r", encoding="utf-8") as f:
            for line in f:
                parts = line.strip().split("\t")
                if len(parts) >= 3 and parts[2].isdigit():
                    word = parts[0].strip()
                    bpm = parts[1].strip()
                    weight = int(parts[2])
                    if len(word) == 1 and " " not in bpm:
                        norm = normalize_bpm(bpm)
                        if word not in syllables[norm] or weight > syllables[norm][word]:
                            syllables[norm][word] = weight
                            
    return syllables

def main():
    occ = load_phrase_occ()
    print(f"Loaded {len(occ)} words from phrase.occ")
    
    syllables = load_all_unigrams()
    print(f"Loaded {len(syllables)} unique 1-char syllables from RuhI lexicons")
    
    inversions = []
    
    for syl, char_weights in syllables.items():
        if len(char_weights) <= 1:
            continue
            
        # 依現行 weight 排序
        by_weight = sorted(char_weights.items(), key=lambda x: x[1], reverse=True)
        # 依客觀 occ 排序
        by_occ = sorted(char_weights.keys(), key=lambda c: occ.get(c, 0), reverse=True)
        
        top_weight_char, top_weight_val = by_weight[0]
        top_occ_char = by_occ[0]
        
        top_weight_occ = occ.get(top_weight_char, 0)
        top_occ_val = occ.get(top_occ_char, 0)
        top_occ_weight = char_weights.get(top_occ_char, 0)
        
        # 判定倒掛條件：只要客觀語料第一名 != 現行權重第一名，且客觀頻率 > 100
        if top_occ_val >= 100 and top_occ_char != top_weight_char:
            ratio = top_occ_val / max(1, top_weight_occ)
            inversions.append({
                "syllable": syl,
                "top_weight_char": top_weight_char,
                "top_weight_val": top_weight_val,
                "top_weight_occ": top_weight_occ,
                "top_occ_char": top_occ_char,
                "top_occ_val": top_occ_val,
                "top_occ_weight": top_occ_weight,
                "ratio": ratio,
                "all_candidates": by_weight[:6]
            })

    # 依客觀第一名的詞頻降冪排序（越常用卻被倒掛的排越前）
    inversions.sort(key=lambda x: x["top_occ_val"], reverse=True)
    
    print("\n" + "="*80)
    print(f"【RuhI 全音節同音字權重倒掛排查報告】發現 {len(inversions)} 個音節存在嚴重先驗倒掛：")
    print("="*80)
    
    for idx, inv in enumerate(inversions[:40], 1):
        syl = inv["syllable"]
        twc, twv, two = inv["top_weight_char"], inv["top_weight_val"], inv["top_weight_occ"]
        toc, tov, tow = inv["top_occ_char"], inv["top_occ_val"], inv["top_occ_weight"]
        cand_str = " ".join([f"{w}:{wt}" for w, wt in inv["all_candidates"]])
        
        print(f"{idx:2d}. 音節 [{syl}]:")
        print(f"    - 客觀語料首位: 「{toc}」(語料出現 {tov} 次，但現行權重僅 {tow})")
        print(f"    - 現行權重首位: 「{twc}」(現行權重 {twv}，但語料僅出現 {two} 次，相差 {inv['ratio']:.1f} 倍)")
        print(f"    - 目前候選排序: {cand_str}")
        print()

if __name__ == "__main__":
    main()
