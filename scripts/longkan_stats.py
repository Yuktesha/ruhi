#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026 A Solid GUI Studio (龜鋼A工作室) / RuhI Contributors
# Licensed under the Apache License, Version 2.0 (see LICENSE in repository root).
"""
longkan_stats.py — 龍龕形音輸入法 (Longkan Morpho-Phonetic IME) 碼表實證統計工具
================================================================================
直接讀取 `ruhi_lexicon/data/longkan.tsv` 計算並輸出可重現之統計指標：
1. 總資料行數（不含註解）、相異編碼數、收錄單字數、收錄多字詞數（依詞長 2、3、4、5+ 分列）
2. 每字平均編碼數與分布（最小值、中位數 P50、P90、P99、最大值）
3. 單字編碼唯一率（依相異單字編碼桶計算之不加權唯一率，以及依單字先驗權重加權之唯一率）
4. 二字詞與多字詞編碼唯一率（不加權與詞頻加權各一）
5. 單字與詞彙編碼夏農資訊熵（Shannon Entropy H(C)，明確定義機率模型）
6. 前 3,000 高權重常用單字之首選可達率（至少有一組編碼排在第一候選之字數與比例）
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path
from typing import Dict, List, Set, Tuple

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass


def compute_longkan_stats(tsv_path: Path) -> Dict[str, object]:
    total_rows = 0
    all_codes: Set[str] = set()
    char_codes: Dict[str, Set[str]] = defaultdict(set)
    word_codes_by_len: Dict[int, Dict[str, Set[str]]] = defaultdict(lambda: defaultdict(set))

    # code -> list of (text, weight, char_len)
    code_to_char_entries: Dict[str, List[Tuple[str, int]]] = defaultdict(list)
    code_to_word_entries: Dict[str, List[Tuple[str, int, int]]] = defaultdict(list)

    char_max_weight: Dict[str, int] = defaultdict(int)
    word_max_weight: Dict[str, int] = defaultdict(int)

    with open(tsv_path, "r", encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            parts = s.split("\t")
            if len(parts) < 3:
                continue
            code = parts[0].strip().lower()
            text = parts[1].strip()
            try:
                weight = int(parts[2].strip())
            except ValueError:
                weight = 100
            if not code or not text:
                continue

            total_rows += 1
            all_codes.add(code)
            ch_len = len(text)
            if ch_len == 1:
                char_codes[text].add(code)
                code_to_char_entries[code].append((text, weight))
                if weight > char_max_weight[text]:
                    char_max_weight[text] = weight
            else:
                bucket_len = min(ch_len, 5)
                word_codes_by_len[bucket_len][text].add(code)
                code_to_word_entries[code].append((text, weight, ch_len))
                if weight > word_max_weight[text]:
                    word_max_weight[text] = weight

    # Sort entries inside each code bucket deterministically: (-weight, text)
    for c, lst in code_to_char_entries.items():
        lst.sort(key=lambda x: (-x[1], x[0]))
    for c, lst in code_to_word_entries.items():
        lst.sort(key=lambda x: (-x[1], x[0]))

    num_chars = len(char_codes)
    num_words_total = sum(len(d) for d in word_codes_by_len.values())
    words_by_len_counts = {
        str(k) if k < 5 else "5+": len(word_codes_by_len[k])
        for k in sorted(word_codes_by_len.keys())
    }

    # Per-character code count distribution
    codes_per_char = sorted(len(cs) for cs in char_codes.values())
    avg_codes_per_char = sum(codes_per_char) / max(1, len(codes_per_char))
    p50_codes = codes_per_char[len(codes_per_char) // 2] if codes_per_char else 0
    p90_codes = codes_per_char[int(len(codes_per_char) * 0.90)] if codes_per_char else 0
    p99_codes = codes_per_char[int(len(codes_per_char) * 0.99)] if codes_per_char else 0
    max_codes = codes_per_char[-1] if codes_per_char else 0
    min_codes = codes_per_char[0] if codes_per_char else 0

    # Single-char code bucket uniqueness (unweighted & weight-weighted)
    char_bucket_sizes = [len(lst) for lst in code_to_char_entries.values()]
    num_char_codes = len(code_to_char_entries)
    unique_char_codes = sum(1 for sz in char_bucket_sizes if sz == 1)
    char_uniqueness_unweighted = (unique_char_codes / max(1, num_char_codes)) * 100.0
    avg_char_candidates = sum(char_bucket_sizes) / max(1, num_char_codes)
    max_char_bucket_code, max_char_bucket_list = max(
        code_to_char_entries.items(), key=lambda kv: len(kv[1])
    )

    # Weight-weighted single-char uniqueness:
    # For each (code, char, weight) entry, weighted by weight, proportion where bucket size == 1
    total_char_entry_weight = sum(w for lst in code_to_char_entries.values() for _, w in lst)
    unique_char_entry_weight = sum(
        lst[0][1] for lst in code_to_char_entries.values() if len(lst) == 1
    )
    char_uniqueness_weighted = (unique_char_entry_weight / max(1, total_char_entry_weight)) * 100.0

    # Word code bucket uniqueness (overall & 2-char words specifically)
    num_word_codes = len(code_to_word_entries)
    unique_word_codes = sum(1 for lst in code_to_word_entries.values() if len(lst) == 1)
    word_uniqueness_unweighted = (unique_word_codes / max(1, num_word_codes)) * 100.0

    total_word_entry_weight = sum(w for lst in code_to_word_entries.values() for _, w, _ in lst)
    unique_word_entry_weight = sum(
        lst[0][1] for lst in code_to_word_entries.values() if len(lst) == 1
    )
    word_uniqueness_weighted = (unique_word_entry_weight / max(1, total_word_entry_weight)) * 100.0

    # 2-char word uniqueness specifically
    code_to_w2: Dict[str, List[Tuple[str, int]]] = defaultdict(list)
    for c, lst in code_to_word_entries.items():
        w2_items = [(t, w) for (t, w, clen) in lst if clen == 2]
        if w2_items:
            code_to_w2[c] = w2_items
    num_w2_codes = len(code_to_w2)
    unique_w2_codes = sum(1 for lst in code_to_w2.values() if len(lst) == 1)
    w2_uniqueness_unweighted = (unique_w2_codes / max(1, num_w2_codes)) * 100.0
    total_w2_wt = sum(w for lst in code_to_w2.values() for _, w in lst)
    unique_w2_wt = sum(lst[0][1] for lst in code_to_w2.values() if len(lst) == 1)
    w2_uniqueness_weighted = (unique_w2_wt / max(1, total_w2_wt)) * 100.0

    # Shannon Entropy H(C) of single-character primary codes:
    # Definition A (Uniform over distinct single-character codes): log2(num_char_codes)
    # Definition B (Weight-proportional over single-char code buckets):
    #   P(c) = (top candidate weight in bucket c) / sum_{c'} (top candidate weight in bucket c')
    bucket_top_weights = [lst[0][1] for lst in code_to_char_entries.values() if lst]
    sum_top_weights = sum(bucket_top_weights)
    shannon_entropy_weighted_bits = -sum(
        (w / sum_top_weights) * math.log2(w / sum_top_weights)
        for w in bucket_top_weights
        if w > 0
    )
    shannon_entropy_uniform_bits = math.log2(max(1, num_char_codes))

    # Top 3,000 highest-weight characters that rank #1 in at least one code bucket
    first_place_chars = {lst[0][0] for lst in code_to_char_entries.values() if lst}
    top3000_chars = [
        ch for ch, _ in sorted(char_max_weight.items(), key=lambda kv: (-kv[1], kv[0]))[:3000]
    ]
    top3000_first_hit = sum(1 for ch in top3000_chars if ch in first_place_chars)

    return {
        "generated_date": str(date.today()),
        "tsv_file": str(tsv_path.name),
        "total_rows": total_rows,
        "distinct_codes_total": len(all_codes),
        "distinct_single_chars": num_chars,
        "distinct_words_total": num_words_total,
        "words_by_length": words_by_len_counts,
        "codes_per_char_distribution": {
            "mean": round(avg_codes_per_char, 2),
            "min": min_codes,
            "p50_median": p50_codes,
            "p90": p90_codes,
            "p99": p99_codes,
            "max": max_codes,
        },
        "single_char_code_stats": {
            "distinct_char_codes": num_char_codes,
            "unique_char_codes": unique_char_codes,
            "uniqueness_rate_unweighted_pct": round(char_uniqueness_unweighted, 2),
            "uniqueness_rate_weighted_pct": round(char_uniqueness_weighted, 2),
            "avg_candidates_per_code": round(avg_char_candidates, 2),
            "max_collision_code": max_char_bucket_code,
            "max_collision_count": len(max_char_bucket_list),
        },
        "word_code_stats": {
            "distinct_word_codes_all": num_word_codes,
            "uniqueness_rate_all_unweighted_pct": round(word_uniqueness_unweighted, 2),
            "uniqueness_rate_all_weighted_pct": round(word_uniqueness_weighted, 2),
            "distinct_2char_word_codes": num_w2_codes,
            "uniqueness_rate_2char_unweighted_pct": round(w2_uniqueness_unweighted, 2),
            "uniqueness_rate_2char_weighted_pct": round(w2_uniqueness_weighted, 2),
        },
        "entropy_bits": {
            "definition_weighted": "H(C) = -sum P(c) log2 P(c), where P(c) is proportional to the top candidate weight of single-character code bucket c",
            "single_char_code_entropy_weighted_bits": round(shannon_entropy_weighted_bits, 2),
            "single_char_code_entropy_uniform_bits": round(shannon_entropy_uniform_bits, 2),
        },
        "top_3000_chars_first_choice_reachability": {
            "top_n": len(top3000_chars),
            "reachable_as_first_choice": top3000_first_hit,
            "reachability_pct": round(top3000_first_hit / max(1, len(top3000_chars)) * 100.0, 2),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="計算並輸出 longkan.tsv 實證統計指標")
    parser.add_argument(
        "--tsv",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "ruhi_lexicon" / "data" / "longkan.tsv",
        help="longkan.tsv 檔案路徑",
    )
    parser.add_argument("--json", action="store_true", help="以 JSON 格式輸出")
    args = parser.parse_args()

    stats = compute_longkan_stats(args.tsv)
    if args.json:
        print(json.dumps(stats, ensure_ascii=False, indent=2))
        return 0

    print("==================================================================")
    print(f"  如喜・龍龕形音輸入法碼表統計報告 ({stats['generated_date']})")
    print("==================================================================")
    print(f"  碼表檔案           : {stats['tsv_file']}")
    print(f"  總資料行數         : {stats['total_rows']:,} 行")
    print(f"  相異編碼總數       : {stats['distinct_codes_total']:,} 組")
    print(f"  收錄單字總數       : {stats['distinct_single_chars']:,} 字")
    print(f"  收錄多字詞總數     : {stats['distinct_words_total']:,} 詞 ({stats['words_by_length']})")
    dist = stats["codes_per_char_distribution"]
    print(
        f"  每字編碼數分布     : 平均 {dist['mean']} 組 "
        f"(最小 {dist['min']} / 中位數 {dist['p50_median']} / P90 {dist['p90']} / P99 {dist['p99']} / 最多 {dist['max']} 組)"
    )
    c_st = stats["single_char_code_stats"]
    print(
        f"  單字編碼唯一率     : 不加權 {c_st['uniqueness_rate_unweighted_pct']}% "
        f"｜ 權重加權 {c_st['uniqueness_rate_weighted_pct']}% "
        f"(共 {c_st['distinct_char_codes']:,} 組單字碼，平均每碼 {c_st['avg_candidates_per_code']} 個候選；"
        f"最擁擠編碼 `{c_st['max_collision_code']}` 有 {c_st['max_collision_count']} 個候選)"
    )
    w_st = stats["word_code_stats"]
    print(
        f"  二字詞編碼唯一率   : 不加權 {w_st['uniqueness_rate_2char_unweighted_pct']}% "
        f"｜ 權重加權 {w_st['uniqueness_rate_2char_weighted_pct']}% "
        f"(共 {w_st['distinct_2char_word_codes']:,} 組二字詞碼)"
    )
    print(
        f"  全多字詞編碼唯一率 : 不加權 {w_st['uniqueness_rate_all_unweighted_pct']}% "
        f"｜ 權重加權 {w_st['uniqueness_rate_all_weighted_pct']}% "
        f"(共 {w_st['distinct_word_codes_all']:,} 組多字詞碼)"
    )
    ent = stats["entropy_bits"]
    print(
        f"  單字編碼資訊熵     : 權重加權 {ent['single_char_code_entropy_weighted_bits']} bits "
        f"｜ 均勻分布 {ent['single_char_code_entropy_uniform_bits']} bits"
    )
    print(f"    (定義: {ent['definition_weighted']})")
    r3k = stats["top_3000_chars_first_choice_reachability"]
    print(
        f"  前 3,000 常用字首選: {r3k['reachable_as_first_choice']:,} / {r3k['top_n']:,} 字 "
        f"({r3k['reachability_pct']}%)"
    )
    print("==================================================================")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
