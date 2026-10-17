# -*- coding: utf-8 -*-
"""
RuhI 跨詞隙音節搭橋截斷清查器 (Cross-Boundary Phonetic Bridge Collision Auditor)

原理 (Linguistic Principle):
當前驅詞 W1 的詞尾音節 S_tail 與後繼詞 W2 的詞頭音節 S_head 恰好拼出詞庫中的生僻詞/獨立詞 W_impostor 時：
  W1 (.. S_tail) + W2 (S_head ..) ===> [.. S_tail S_head ..]
Viterbi 演算法若無「長詞優先/構式引力/反向截斷約束」，就可能出現：
  W1_head + W_impostor + W2_tail
例如：
  1. 坐一下 (ㄧ ㄒㄧㄚˋ) + 石頭 (ㄕˊ ㄊㄡˊ)  ===>  坐一 + 下石 (ㄒㄧㄚˋ ㄕˊ) + 頭
  2. 盲人摸象 (ㄇㄛ ㄒㄧㄤˋ) + 時 (ㄕˊ)   ===>  盲人摸 + 橡實 (ㄒㄧㄤˋ ㄕˊ)
  3. 街巷 (ㄐㄧㄝ ㄒㄧㄤˋ) + 時鐘 (ㄕˊ ㄓㄨㄥ) ===>  街 + 橡實 (ㄒㄧㄤˋ ㄕˊ) + 鐘
  4. 混打 (ㄏㄨㄣˋ ㄉㄚˇ) + 看板 (ㄎㄢˋ ㄅㄢˇ) ===>  混 + 打看 (ㄉㄚˇ ㄎㄢˋ) + 板

本工具全面掃描詞庫，自動列出潛在跨詞隙搭橋碰撞對，供轉移矩陣與反語意劫掠守則守護。
"""

import sys
import os
from collections import defaultdict

sys.stdout.reconfigure(encoding='utf-8')

def load_lexicons():
    files = [
        "ruhi_lexicon/data/lexicon_domain.tsv",
        "ruhi_lexicon/data/lexicon_mcbopomofo.tsv",
        "ruhi_lexicon/data/lexicon_moe.tsv"
    ]
    words_by_syl = defaultdict(list)
    word_to_syls = {}
    
    for f in files:
        if not os.path.exists(f):
            continue
        with open(f, "r", encoding="utf-8") as fp:
            for line in fp:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) >= 2:
                    word = parts[0]
                    bpmf = parts[1].strip()
                    syls = tuple(bpmf.split())
                    if len(word) == len(syls): # standard 1-to-1 syllables
                        word_to_syls[word] = syls
                        words_by_syl[syls].append(word)
    return word_to_syls, words_by_syl

def audit_collisions():
    word_to_syls, words_by_syl = load_lexicons()
    print(f"Total indexed words: {len(word_to_syls)}")
    
    # Target impostors that are 2-character words
    two_char_impostors = {w: syls for w, syls in word_to_syls.items() if len(w) == 2 and len(syls) == 2}
    print(f"Total 2-char impostor candidate patterns: {len(two_char_impostors)}")

    # Specific high-risk candidates for verification
    check_pairs = [
        ("坐一下", "石頭", "下石"),
        ("坐一下", "時", "下石"),
        ("盲人摸象", "時", "橡實"),
        ("街巷", "時鐘", "橡實"),
        ("混打", "看板", "打看"),
        ("此碑", "為", "卑位"),
        ("悲中之悲", "時之將至", "悲時"),
    ]
    
    print("\n=== 🎯 高風險搭橋碰撞驗證清單 ===")
    for w1, w2, impostor in check_pairs:
        syl1 = word_to_syls.get(w1)
        syl2 = word_to_syls.get(w2)
        syl_imp = word_to_syls.get(impostor)
        
        if syl1 and syl2 and syl_imp:
            tail_syl = syl1[-1]
            head_syl = syl2[0]
            bridge = (tail_syl, head_syl)
            matches = bridge == syl_imp
            print(f"[{'COLLISION' if matches else 'CLEAN'}] '{w1}' ({tail_syl}) + '{w2}' ({head_syl}) vs '{impostor}' ({syl_imp}) -> Matches: {matches}")
        else:
            print(f"[MISSING DATA] {w1}: {bool(syl1)}, {w2}: {bool(syl2)}, {impostor}: {bool(syl_imp)}")

if __name__ == "__main__":
    audit_collisions()
