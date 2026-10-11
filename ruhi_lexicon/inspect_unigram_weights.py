# -*- coding: utf-8 -*-
"""
Inspect Homophone Unigram Weights & Inversions
"""
import os
import sys
from collections import defaultdict

sys.stdout.reconfigure(encoding='utf-8')

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
DATA_DIR = os.path.join(BASE_DIR, "data")

# Standard Ministry of Education (MOE) Top Frequency Common Characters (Sample of top tier)
# In Taiwan, MOE 常用國字標準字體表 has 4,808 common characters.
# We can load MOE frequency or compute character frequency.

def load_lexicons():
    entries = [] # (word, bpm, weight, source)
    
    # 1. mcbopomofo
    mc_path = os.path.join(DATA_DIR, "lexicon_mcbopomofo.tsv")
    if os.path.exists(mc_path):
        with open(mc_path, "r", encoding="utf-8") as f:
            for line in f:
                parts = line.strip().split("\t")
                if len(parts) >= 3 and parts[2].isdigit():
                    entries.append((parts[0], parts[1], int(parts[2]), "mcbopomofo"))

    # 2. moe
    moe_path = os.path.join(DATA_DIR, "lexicon_moe.tsv")
    if os.path.exists(moe_path):
        with open(moe_path, "r", encoding="utf-8") as f:
            for line in f:
                parts = line.strip().split("\t")
                if len(parts) >= 3 and parts[2].isdigit():
                    entries.append((parts[0], parts[1], int(parts[2]), "moe"))

    # 3. domain
    dom_path = os.path.join(DATA_DIR, "lexicon_domain.tsv")
    if os.path.exists(dom_path):
        with open(dom_path, "r", encoding="utf-8") as f:
            for line in f:
                parts = line.strip().split("\t")
                if len(parts) >= 3 and parts[2].isdigit():
                    entries.append((parts[0], parts[1], int(parts[2]), "domain"))

    return entries

def main():
    entries = load_lexicons()
    print(f"Total entries loaded: {len(entries)}")

    # Group by syllable for 1-char words
    syllables = defaultdict(dict) # bpm -> {word: max_weight}
    for word, bpm, weight, src in entries:
        if len(word) == 1 and " " not in bpm:
            # Normalize light tone: if ends with ˙ -> starts with ˙
            clean_bpm = bpm
            if clean_bpm.endswith("˙"):
                clean_bpm = "˙" + clean_bpm[:-1]
            if word not in syllables[clean_bpm] or weight > syllables[clean_bpm][word]:
                syllables[clean_bpm][word] = weight

    print(f"Total unique 1-char syllables: {len(syllables)}")

    # Check key frequently-confused syllables
    test_syllables = [
        "ㄕˋ", "ㄌㄧˇ", "ㄗㄞˋ", "ㄉㄜ˙", "˙ㄉㄜ", "˙ㄏㄟ", "ㄏㄟ",
        "ㄗㄨㄛˋ", "ㄧˇ", "ㄒㄩ", "ㄐㄧˊ", "ㄐㄩㄝˊ", "ㄆㄧㄢ", "ㄧˋ",
        "ㄩㄝˋ", "ㄍㄨㄛˇ", "ㄉㄠˋ", "ㄎㄜˇ", "ㄕˊ", "ㄏㄨㄟˋ"
    ]

    for syl in test_syllables:
        if syl in syllables:
            ranked = sorted(syllables[syl].items(), key=lambda x: x[1], reverse=True)
            chars_str = " ".join([f"{w}({wt})" for w, wt in ranked[:10]])
            print(f"[{syl}] (total {len(ranked)}): {chars_str}")
        else:
            print(f"[{syl}]: NOT FOUND IN LEXICON!")

if __name__ == "__main__":
    main()
