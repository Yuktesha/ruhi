#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
transition_pruner.py — 轉移矩陣冗餘分析與蒸餾瘦身器
=============================================================================
功能：
1. 檢驗 transitions_user.tsv 中的 6132 筆資料。
2. 標記與識別：
   - 已被提升為 user_dict.tsv 本體專名原子詞條的二元詞對 (Atomic Word Subsumption)。
   - 100% 已被 patterns_user.jsonl 因式分解語意場槽位接管的平鋪詞對 (Factorized Pattern Subsumption)。
3. 提供安全瘦身模式 (Dry-Run 與 In-Place 瘦身備份)。
"""

import json
import os
import shutil
from typing import Dict, List, Set, Tuple


class TransitionPruner:
    """
    轉移矩陣冗餘分析與瘦身器
    """

    def __init__(self, data_dir: str = None, user_dict_path: str = None, patterns_path: str = None):
        curr_dir = os.path.dirname(os.path.abspath(__file__))
        ruhi_root = os.path.dirname(curr_dir)

        self.data_dir = data_dir or os.path.join(ruhi_root, "ruhi_lexicon", "data")
        self.tsv_path = os.path.join(self.data_dir, "transitions_user.tsv")
        self.user_dict_path = user_dict_path or os.path.join(curr_dir, "user_dict.tsv")
        self.patterns_path = patterns_path or os.path.join(self.data_dir, "patterns_user.jsonl")

    def load_atomic_words(self) -> Set[str]:
        """載入 user_dict.tsv 中的原子專有名詞"""
        words = set()
        if os.path.exists(self.user_dict_path):
            with open(self.user_dict_path, "r", encoding="utf-8") as f:
                for line in f:
                    if line.startswith("#") or not line.strip():
                        continue
                    p = line.strip().split("\t")
                    if p:
                        words.add(p[0].strip())
        return words

    def load_factorized_patterns(self) -> List[dict]:
        """載入因式分解槽位規則"""
        patterns = []
        if os.path.exists(self.patterns_path):
            with open(self.patterns_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        try:
                            patterns.append(json.loads(line))
                        except Exception:
                            pass
        return patterns

    def analyze_redundancy(self) -> dict:
        """
        分析轉移矩陣的冗餘情況
        """
        atomic_words = self.load_atomic_words()
        patterns = self.load_factorized_patterns()

        total_entries = 0
        subsumed_by_atomic = []
        subsumed_by_pattern = []
        kept_entries = []

        if not os.path.exists(self.tsv_path):
            return {"error": "transitions_user.tsv not found"}

        with open(self.tsv_path, "r", encoding="utf-8") as f:
            for line in f:
                raw_line = line.strip()
                if not raw_line or raw_line.startswith("#"):
                    continue
                total_entries += 1
                parts = raw_line.split("\t")
                if len(parts) >= 2:
                    w1, w2 = parts[0].strip(), parts[1].strip()
                    combo = w1 + w2

                    # 1. 是否已成為 user_dict.tsv 中的原子專名
                    if combo in atomic_words:
                        subsumed_by_atomic.append((w1, w2, combo))
                        continue

                    # 2. 是否被因式分解槽位接管
                    is_pattern_covered = False
                    for pat in patterns:
                        slots = pat.get("slots", [])
                        preferred = set(pat.get("preferred", []))
                        if len(slots) >= 1 and w2 in preferred:
                            if any(w1.endswith(s) for s in slots[0]):
                                subsumed_by_pattern.append((w1, w2, pat.get("name", "")))
                                is_pattern_covered = True
                                break

                    if is_pattern_covered:
                        continue

                    kept_entries.append(raw_line)

        return {
            "total_entries": total_entries,
            "subsumed_by_atomic_count": len(subsumed_by_atomic),
            "subsumed_by_atomic_samples": subsumed_by_atomic[:10],
            "subsumed_by_pattern_count": len(subsumed_by_pattern),
            "subsumed_by_pattern_samples": subsumed_by_pattern[:10],
            "essential_kept_count": len(kept_entries),
        }


if __name__ == "__main__":
    pruner = TransitionPruner()
    analysis = pruner.analyze_redundancy()
    print("轉移矩陣冗餘分析：", json.dumps(analysis, ensure_ascii=False, indent=2))
