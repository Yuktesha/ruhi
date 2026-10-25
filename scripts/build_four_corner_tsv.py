#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 智慧型四角號碼 4×5 數字小鍵盤 碼表與前兩角簡碼矩陣編譯器
================================================================================
本工具負責編譯《智慧型四角號碼輸入法》核心碼表 `four_corner.tsv` 及跨平台 Rime/Fcitx5 方案檔：
1. 解析 Unicode 官方 `Unihan_DictionaryLikeData.txt` (`kFourCornerCode`) 與 `Unihan_Variants.txt`。
2. 結合如喜核心語料庫 (`lexicon_mcbopomofo.tsv`, `lexicon_moe.tsv`) 計算單字與現代漢語詞彙先驗權重。
3. 三軌相容容錯編譯：
   - 標準王雲五四角號碼（4 碼本碼與 5 碼含附角）
   - 台灣正體／異體字雙向對等共享（如 `台` 2360.0 ⇄ `臺` 4010.4）
   - 初學者直覺拆碼寬容（解除「筆畫已用過補 0」之記憶負擔，並支援 `0` 同時涵蓋「頭 `亠`」與「撇／左鉤」）
4. 雙字詞與多字詞「分層四角構詞矩陣」預編譯：
   - 雙字詞（4 碼）：首字前兩角 `1,2` (左上+右上) + 尾字前兩角 `1,2` (左上+右上)（如 `台灣` = `2332`、`電腦` = `1072`、`主權` = `0044`）
   - 三字詞（5 碼）：`首字(1,3) + 中字(1) + 尾字(2,4)`（左緣定錨＋中字首角＋右緣收尾，如 `輸入法` = `50843`、`台灣人` = `26300`）
   - 四字成語（6 碼）：`首字(1,3) + 中一(1) + 中二(1) + 尾字(2,4)`（`1,3[1×n]2,4` 結構，如 `四角號碼` = `622672`）
5. 4×5 數字小鍵盤 `/` 標點符號速選碼（`/1`~`/0`）。
"""

from __future__ import annotations

import argparse
import os
import sys
import urllib.request
import zipfile
from collections import defaultdict
from pathlib import Path
from typing import Dict, List, Set, Tuple

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

UNIHAN_URL = "https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip"

# 台灣正體常見異體字／正俗字雙向共享對照表（確保正體使用者無論依何種字形拆碼皆能命中）
MANUAL_VARIANT_PAIRS: List[Tuple[str, str]] = [
    ("台", "臺"),
    ("裡", "裏"),
    ("線", "綫"),
    ("為", "爲"),
    ("真", "眞"),
    ("群", "羣"),
    ("峰", "峯"),
    ("啟", "啓"),
    ("衛", "衞"),
    ("雞", "鷄"),
    ("體", "体"),
    ("館", "舘"),
    ("麵", "麪"),
    ("床", "牀"),
    ("俗", "俗"),
    ("眾", "衆"),
    ("研", "硏"),
    ("秘", "祕"),
    ("痴", "癡"),
    ("著", "着"),
    ("沉", "沈"),
    ("卻", "却"),
    ("腳", "脚"),
    ("掛", "挂"),
    ("挽", "挽"),
    ("罵", "駡"),
    ("鵝", "鵞"),
    ("略", "畧"),
    ("回", "迴"),
    ("菓", "果"),
    ("計畫", "計劃"),
]

# 4×5 數字小鍵盤 `/` 引導之標點符號速選表
NUMPAD_SYMBOLS: List[Tuple[str, str, int]] = [
    ("/1", "，", 999),
    ("/2", "。", 998),
    ("/3", "、", 997),
    ("/4", "？", 996),
    ("/5", "！", 995),
    ("/6", "：", 994),
    ("/7", "；", 993),
    ("/8", "「」", 992),
    ("/9", "（）", 991),
    ("/0", "……", 990),
]


def ensure_unihan_zip(cache_dir: Path) -> Path:
    cache_dir.mkdir(parents=True, exist_ok=True)
    zip_path = cache_dir / "Unihan.zip"
    if zip_path.exists() and zip_path.stat().st_size > 1_000_000:
        return zip_path
    print(f"[下載] 正在從 Unicode 官方儲存庫獲取 Unihan.zip -> {zip_path} ...")
    urllib.request.urlretrieve(UNIHAN_URL, zip_path)
    return zip_path


def codepoint_to_char(cp_str: str) -> str:
    # 例如 "U+53F0" -> "台"
    if cp_str.startswith("U+"):
        cp_str = cp_str[2:]
    cp_Str_clean = cp_str.split("<")[0].strip()
    return chr(int(cp_Str_clean, 16))


def parse_unihan(zip_path: Path) -> Tuple[Dict[str, List[Tuple[str, str]]], Dict[str, Set[str]]]:
    """
    解析 Unihan.zip 中的 kFourCornerCode 與異體字關聯。
    回傳：
      char_to_codes: { '台': [('2360', '0')], ... }
      variants_map:  { '台': {'臺'}, '臺': {'台'}, ... }
    """
    char_to_codes: Dict[str, List[Tuple[str, str]]] = defaultdict(list)
    variants_map: Dict[str, Set[str]] = defaultdict(set)

    with zipfile.ZipFile(zip_path, "r") as zf:
        with zf.open("Unihan_DictionaryLikeData.txt") as f:
            for raw in f:
                line = raw.decode("utf-8", errors="ignore").strip()
                if not line or line.startswith("#") or "\tkFourCornerCode\t" not in line:
                    continue
                parts = line.split("\t")
                if len(parts) < 3:
                    continue
                ch = codepoint_to_char(parts[0])
                for token in parts[2].split():
                    if "." in token:
                        main4, ext1 = token.split(".", 1)
                    else:
                        main4, ext1 = token[:4], ""
                    if len(main4) == 4 and main4.isdigit():
                        pair = (main4, ext1 if ext1.isdigit() else "")
                        if pair not in char_to_codes[ch]:
                            char_to_codes[ch].append(pair)

        if "Unihan_Variants.txt" in zf.namelist():
            with zf.open("Unihan_Variants.txt") as f:
                for raw in f:
                    line = raw.decode("utf-8", errors="ignore").strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split("\t")
                    if len(parts) < 3:
                        continue
                    field = parts[1]
                    if field not in (
                        "kTraditionalVariant",
                        "kSemanticVariant",
                        "kZVariant",
                        "kSimplifiedVariant",
                    ):
                        continue
                    ch = codepoint_to_char(parts[0])
                    for target_token in parts[2].split():
                        try:
                            target_ch = codepoint_to_char(target_token)
                            if target_ch != ch:
                                variants_map[ch].add(target_ch)
                                variants_map[target_ch].add(ch)
                        except Exception:
                            pass

    # 注入台灣正體手動雙向共享對照
    for a, b in MANUAL_VARIANT_PAIRS:
        if len(a) == 1 and len(b) == 1:
            variants_map[a].add(b)
            variants_map[b].add(a)

    return char_to_codes, variants_map


def load_ruhi_lexicons(
    lexicon_dir: Path,
) -> Tuple[Dict[str, int], Dict[str, int]]:
    """
    從 lexicon_mcbopomofo.tsv 與 lexicon_moe.tsv 載入單字權重與多字詞權重。
    回傳：
      char_weights: { '的': 950, '台': 850, ... }
      word_weights: { '台灣': 781, '電腦': 740, ... }
    """
    char_weights: Dict[str, int] = {}
    word_weights: Dict[str, int] = {}

    mcb_path = lexicon_dir / "lexicon_mcbopomofo.tsv"
    if mcb_path.exists():
        with open(mcb_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) < 3 or parts[2] == "WEIGHT":
                    continue
                word = parts[0].strip()
                if not word or any(c.isascii() for c in word):
                    continue
                try:
                    w = int(round(float(parts[2])))
                except ValueError:
                    continue
                if len(word) == 1:
                    if w > char_weights.get(word, -999999):
                        char_weights[word] = w
                elif 2 <= len(word) <= 4:
                    if w > word_weights.get(word, -999999):
                        word_weights[word] = w

    moe_path = lexicon_dir / "lexicon_moe.tsv"
    if moe_path.exists():
        with open(moe_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) < 3 or parts[2] == "WEIGHT":
                    continue
                word = parts[0].strip()
                if not word or any(c.isascii() for c in word):
                    continue
                try:
                    w = int(round(float(parts[2])))
                except ValueError:
                    continue
                if len(word) == 1:
                    # 若 McBopomofo 未收錄，採用 MOE 權重但給予適度偏移以維持日常字優先
                    scaled = max(120, min(550, w // 2))
                    if word not in char_weights:
                        char_weights[word] = scaled
                elif 2 <= len(word) <= 4:
                    scaled = max(150, min(560, w // 2))
                    if word not in word_weights:
                        word_weights[word] = scaled

    for core_word, cw in [("四角號碼", 640), ("如喜", 590), ("九宮格", 560), ("小鍵盤", 560)]:
        if core_word not in word_weights or word_weights[core_word] < cw:
            word_weights[core_word] = cw

    # 利用雙字詞頻率強化常見單字的基礎活躍度（防止常見字在單字表缺漏）
    char_in_words: Dict[str, int] = defaultdict(int)
    for word, w in word_weights.items():
        if w >= 550:
            for ch in word:
                char_in_words[ch] += 1

    for ch, cnt in char_in_words.items():
        bonus = min(160, cnt * 2)
        if ch in char_weights:
            char_weights[ch] = max(char_weights[ch], 450 + bonus)
        else:
            char_weights[ch] = 420 + bonus

    return char_weights, word_weights


def generate_beginner_intuitive_codes(main4: str) -> List[str]:
    """
    初學者直覺拆碼寬容規則：
    1. 解除王雲五「同一筆畫已經用過則補 0」之記憶負擔：
       - 若右上角 `c[1] == '0'` 且 `c[0] != '0'`，衍生 `c[0] + c[0] + c[2] + c[3]`
       - 若右下角 `c[3] == '0'` 且 `c[2] != '0'`，衍生 `c[0] + c[1] + c[2] + c[2]`
       - 若兩者同時為 `'0'`，同步衍生四角直覺展開碼
    2. 遵循《智慧型四角號碼輸入法》規格書「0 為頭與撇／左鉤 (`0` ⇄ `2`)」雙通設計：
       - 若首碼或末碼為 `2`（撇），允許初學者以 `0` 輸入；若首碼為 `0`（頭），亦容許與鄰近筆畫互查。
    """
    derived: List[str] = []
    c0, c1, c2, c3 = main4[0], main4[1], main4[2], main4[3]

    # 規則 A：筆畫複用直覺展開（不強迫記憶補 0）
    if c1 == "0" and c0 != "0":
        cand = f"{c0}{c0}{c2}{c3}"
        if cand != main4 and cand not in derived:
            derived.append(cand)
    if c3 == "0" and c2 != "0":
        cand = f"{c0}{c1}{c2}{c2}"
        if cand != main4 and cand not in derived:
            derived.append(cand)
    if c1 == "0" and c0 != "0" and c3 == "0" and c2 != "0":
        cand = f"{c0}{c0}{c2}{c2}"
        if cand != main4 and cand not in derived:
            derived.append(cand)

    # 規則 B：規格書定義 `0` 涵蓋「撇與左鉤」(`2` -> `0`)
    if c0 == "2":
        cand = f"0{c1}{c2}{c3}"
        if cand != main4 and cand not in derived:
            derived.append(cand)
    if c2 == "2":
        cand = f"{c0}{c1}0{c3}"
        if cand != main4 and cand not in derived:
            derived.append(cand)

    # 規則 C：三點水 `氵` 左下角為「提筆 (1)」，初學者常直覺視為「點 (3)」(如：灣 3212 -> 3232)
    if c0 == "3" and c2 == "1":
        cand = f"3{c1}3{c3}"
        if cand != main4 and cand not in derived:
            derived.append(cand)
        if c3 == "2":
            # 如「灣 (3212)」右下角「弓」彎鉤，初學者亦常直覺視為折角 7 (3217 / 3237)
            for alt in (f"3{c1}17", f"3{c1}37"):
                if alt != main4 and alt not in derived:
                    derived.append(alt)

    return derived


def compile_four_corner_entries(
    char_to_codes: Dict[str, List[Tuple[str, str]]],
    variants_map: Dict[str, Set[str]],
    char_weights: Dict[str, int],
    word_weights: Dict[str, int],
) -> List[Tuple[str, str, int, str]]:
    """
    編譯全體條目，回傳 `[(code, text, weight, kind), ...]`。
    """
    # (code, text) -> (best_weight, kind)
    best_entries: Dict[Tuple[str, str], Tuple[int, str]] = {}

    def register(code: str, text: str, weight: int, kind: str) -> None:
        key = (code, text)
        if key not in best_entries:
            best_entries[key] = (weight, kind)
        else:
            old_w, old_k = best_entries[key]
            # 優先保留標準 full 或 phrase22 標記，並取較高權重
            priority = {"symbol": 6, "full": 5, "phrase22": 4, "phrase3": 3, "phrase4": 3, "variant": 2, "fuzzy": 1}
            new_k = kind if priority.get(kind, 0) >= priority.get(old_k, 0) else old_k
            best_entries[key] = (max(old_w, weight), new_k)

    # 0. 注入 `/` 標點符號速選表
    for code, sym, w in NUMPAD_SYMBOLS:
        register(code, sym, w, "symbol")

    # 台灣正體雙向共享對照集合（此集合內之正異體字雙向共享彼此的標準四角碼與單字基準活躍度）
    bidirectional_sets: Dict[str, Set[str]] = defaultdict(set)
    for a, b in MANUAL_VARIANT_PAIRS:
        if len(a) == 1 and len(b) == 1:
            bidirectional_sets[a].add(b)
            bidirectional_sets[b].add(a)
            wa = char_weights.get(a, 0)
            wb = char_weights.get(b, 0)
            best_ab = max(wa, wb)
            if best_ab > 0:
                char_weights[a] = max(wa, best_ab - 12)
                char_weights[b] = max(wb, best_ab - 12)

    # 同步擴充含雙向正異體字（台/臺、裡/裏、線/綫、為/爲）之高頻詞彙權重
    for a, b in [("台", "臺"), ("裡", "裏"), ("線", "綫"), ("為", "爲"), ("真", "眞")]:
        extra_words: Dict[str, int] = {}
        for word, w in word_weights.items():
            if w >= 350 and a in word:
                alt = word.replace(a, b)
                if alt not in word_weights or word_weights[alt] < w - 15:
                    extra_words[alt] = w - 15
            elif w >= 350 and b in word:
                alt = word.replace(b, a)
                if alt not in word_weights or word_weights[alt] < w - 15:
                    extra_words[alt] = w - 15
        word_weights.update(extra_words)

    # 1. 建立每個單字的「主四角碼」與「異體共享四角碼」
    # 若某正體字未直接列於 Unihan kFourCornerCode，從其 Unihan 異體字繼承；
    # 若已列於 Unihan，僅對 MANUAL_VARIANT_PAIRS 進行雙向共享（避免 Unihan kSemanticVariant 將「一」與「壹」混同）
    effective_char_codes: Dict[str, List[Tuple[str, str, bool]]] = defaultdict(list)
    all_known_chars = set(char_to_codes.keys()) | set(char_weights.keys())

    for ch in all_known_chars:
        direct = char_to_codes.get(ch, [])
        for m4, ext in direct:
            effective_char_codes[ch].append((m4, ext, False))

        if not direct and ch in variants_map:
            # Unihan 未直接收錄四角碼之正體／罕見字，從異體字補齊
            for var_ch in variants_map[ch]:
                for m4, ext in char_to_codes.get(var_ch, []):
                    if not any(existing_m4 == m4 for existing_m4, _, _ in effective_char_codes[ch]):
                        effective_char_codes[ch].append((m4, ext, True))
        elif ch in bidirectional_sets:
            # 台灣常用正異體字（如 台⇄臺、裡⇄裏、線⇄綫）雙向互通
            for var_ch in bidirectional_sets[ch]:
                for m4, ext in char_to_codes.get(var_ch, []):
                    if not any(existing_m4 == m4 for existing_m4, _, _ in effective_char_codes[ch]):
                        effective_char_codes[ch].append((m4, ext, True))

    # 2. 注入單字標準 4 碼、5 碼（含附角）、異體共享碼與初學者直覺寬容碼
    for ch, code_list in effective_char_codes.items():
        base_w = char_weights.get(ch, 180)
        # 確保正體常用字與罕見字之間有清晰的先驗分層，同時不篡改原始字典權重
        for idx, (main4, ext1, is_variant) in enumerate(code_list):
            rank_penalty = idx * 8
            if not is_variant:
                w4 = max(80, base_w - rank_penalty)
                register(main4, ch, w4, "full")
                if ext1:
                    register(f"{main4}{ext1}", ch, w4 + 35, "full")
                # 對於常用字（base_w >= 400），生成初學者直覺寬容碼（解除補 0 記憶負擔 & 0/2 撇頭互通）
                # 寬容碼權重設於正統單字與正統雙字詞（>=490）之後，避免干擾正統碼第一頁候選（如 0044 主權/辦/辯）
                if base_w >= 400:
                    for fz_code in generate_beginner_intuitive_codes(main4):
                        fz_w = min(485, max(75, w4 - 150))
                        register(fz_code, ch, fz_w, "fuzzy")
            else:
                # 異體雙向共享碼（例如輸入 4010 也能選出「台」，輸入 2360 也能選出「臺」）
                w_var = max(70, base_w - 18 - rank_penalty)
                register(main4, ch, w_var, "variant")
                if ext1:
                    register(f"{main4}{ext1}", ch, w_var + 20, "variant")

    # 3. 建立單字角位索引：
    #    - char_prefixes (1,2 角)：供二字詞 (1,2 + 1,2) 4 碼合成使用
    #    - char_c13      (1,3 角)：供三字詞以上首字左緣定錨 (左上+左下) 使用
    #    - char_c1       (1 角)  ：供三字詞以上中間字首角 (左上) 使用
    #    - char_c24      (2,4 角)：供三字詞以上尾字右緣收尾 (右上+右下) 使用
    char_prefixes: Dict[str, List[Tuple[str, int]]] = defaultdict(list)
    char_c13: Dict[str, List[Tuple[str, int]]] = defaultdict(list)
    char_c1: Dict[str, List[Tuple[str, int]]] = defaultdict(list)
    char_c24: Dict[str, List[Tuple[str, int]]] = defaultdict(list)

    for ch, code_list in effective_char_codes.items():
        seen_p2: Set[str] = set()
        seen_13: Set[str] = set()
        seen_1: Set[str] = set()
        seen_24: Set[str] = set()
        for idx, (main4, _ext1, is_variant) in enumerate(code_list):
            penalty = (15 if is_variant else 0) + idx * 5
            p2 = main4[:2]
            if p2 not in seen_p2:
                seen_p2.add(p2)
                char_prefixes[ch].append((p2, penalty))
            c13 = f"{main4[0]}{main4[2]}"
            if c13 not in seen_13:
                seen_13.add(c13)
                char_c13[ch].append((c13, penalty))
            c1 = main4[0]
            if c1 not in seen_1:
                seen_1.add(c1)
                char_c1[ch].append((c1, penalty))
            c24 = f"{main4[1]}{main4[3]}"
            if c24 not in seen_24:
                seen_24.add(c24)
                char_c24[ch].append((c24, penalty))
            # 最高頻單字若首碼為 2 (撇)，同步支援 0 (撇/左鉤) 前兩角簡碼（降權作為容錯後備，不搶佔正統 0 首碼詞彙）
            if not is_variant and idx == 0 and char_weights.get(ch, 0) >= 620 and p2[0] == "2":
                alt_p2 = "0" + p2[1]
                if alt_p2 not in seen_p2:
                    seen_p2.add(alt_p2)
                    char_prefixes[ch].append((alt_p2, 115))

    # 4. 編譯現代漢語高頻詞彙「分層四角構詞矩陣」
    #    - 二字詞 (4 碼): 首字(1,2) + 尾字(1,2)
    #    - 三字詞 (5 碼): 首字(1,3) + 中字(1) + 尾字(2,4)
    #    - 四字詞 (6 碼): 首字(1,3) + 中一(1) + 中二(1) + 尾字(2,4)
    phrase22_buckets: Dict[str, Dict[str, int]] = defaultdict(dict)
    phrase3_buckets: Dict[str, Dict[str, int]] = defaultdict(dict)
    phrase4_buckets: Dict[str, Dict[str, int]] = defaultdict(dict)

    num_chars = set("一二三四五六七八九十百千萬億兩幾多半")
    glue_tails = set("有來去在太大小上下裏裡中內外前後左右個本台臺堆套支隻條張件次種樣點些")
    lexical_whitelist = {
        "一直", "一起", "一下", "一般", "一定", "一旦", "一致", "一流", "一再", "萬一", "唯一",
        "天下", "天才", "天真", "天氣", "天然", "天堂", "天使", "所有", "擁有", "具有", "沒有",
        "唯有", "只有", "固有", "罕有", "從來", "未來", "原來", "出來", "後來", "起來", "看來",
        "由來", "向來", "外來", "往來", "自來", "如來", "失去", "過去", "離去", "死去", "省去",
        "除去", "平台", "平臺", "電台", "電臺", "舞台", "舞臺", "下台", "下臺", "上台", "上臺",
        "後台", "後臺", "櫃台", "櫃臺", "書本", "課本", "劇本", "成本", "資本", "基本", "根本",
        "日本", "原本", "樣本", "版本", "筆記", "太太", "太陽", "太空", "太子", "太后", "太平洋",
    }

    def is_phonetic_glue_bigram(w_str: str) -> bool:
        if w_str in lexical_whitelist:
            return False
        if w_str[0] in num_chars:
            return True
        if w_str[0] in ("不", "更", "最", "很", "太", "真", "好", "還", "又", "也", "都", "已", "正", "要", "會", "能", "可") and w_str[1] in glue_tails:
            return True
        if w_str[1] in ("有", "來", "去"):
            return True
        return False

    for word, w in word_weights.items():
        wlen = len(word)
        if wlen == 2:
            # 收錄常用雙字詞 (McBopomofo 權重 >= 330，涵蓋前 25% 現代漢語高頻雙字詞)
            if w < 330:
                continue
            c1_p2s = char_prefixes.get(word[0], [])
            c2_p2s = char_prefixes.get(word[1], [])
            if not c1_p2s or not c2_p2s:
                continue
            glue_pen = 165 if is_phonetic_glue_bigram(word) else 0
            for p1, pen1 in c1_p2s[:3]:
                for p2, pen2 in c2_p2s[:3]:
                    code4 = f"{p1}{p2}"
                    # 雙字詞基礎權重給予適度加成，使其在複合詞語境與常用詞輸入時自然浮現
                    eff_w = max(150, w + 45 - pen1 - pen2 - glue_pen)
                    if eff_w > phrase22_buckets[code4].get(word, -999999):
                        phrase22_buckets[code4][word] = eff_w

        elif wlen == 3:
            if w < 300:
                continue
            c1_13s = char_c13.get(word[0], [])
            c2_1s = char_c1.get(word[1], [])
            c3_24s = char_c24.get(word[2], [])
            if not (c1_13s and c2_1s and c3_24s):
                continue
            for p13, pen1 in c1_13s[:2]:
                for m1, pen2 in c2_1s[:2]:
                    for t24, pen3 in c3_24s[:2]:
                        code5 = f"{p13}{m1}{t24}"
                        eff_w = max(160, w + 120 - pen1 - pen2 - pen3)
                        if eff_w > phrase3_buckets[code5].get(word, -999999):
                            phrase3_buckets[code5][word] = eff_w

        elif wlen == 4:
            if w < 260:
                continue
            c1_13s = char_c13.get(word[0], [])
            c2_1s = char_c1.get(word[1], [])
            c3_1s = char_c1.get(word[2], [])
            c4_24s = char_c24.get(word[3], [])
            if not (c1_13s and c2_1s and c3_1s and c4_24s):
                continue
            for p13, pen1 in c1_13s[:2]:
                for m1, pen2 in c2_1s[:1]:
                    for m2, pen3 in c3_1s[:1]:
                        for t24, pen4 in c4_24s[:2]:
                            code6 = f"{p13}{m1}{m2}{t24}"
                            eff_w = max(170, w + 180 - pen1 - pen2 - pen3 - pen4)
                            if eff_w > phrase4_buckets[code6].get(word, -999999):
                                phrase4_buckets[code6][word] = eff_w

    # 每個雙字簡碼桶保留最高頻前 28 詞（支援 3 頁 PgDn 選詞），三字(5碼)／四字(6碼)桶保留前 15 詞
    for code, word_map in phrase22_buckets.items():
        items = sorted(word_map.items(), key=lambda x: (-x[1], x[0]))
        for word, eff_w in items[:28]:
            register(code, word, eff_w, "phrase22")

    for code, word_map in phrase3_buckets.items():
        items = sorted(word_map.items(), key=lambda x: (-x[1], x[0]))
        for word, eff_w in items[:15]:
            register(code, word, eff_w, "phrase3")

    for code, word_map in phrase4_buckets.items():
        items = sorted(word_map.items(), key=lambda x: (-x[1], x[0]))
        for word, eff_w in items[:15]:
            register(code, word, eff_w, "phrase4")

    # 整理並排序最終輸出清單
    final_list: List[Tuple[str, str, int, str]] = []
    for (code, text), (weight, kind) in best_entries.items():
        final_list.append((code, text, weight, kind))

    final_list.sort(key=lambda x: (x[0], -x[2], len(x[1]), x[1]))
    return final_list


def write_tsv(entries: List[Tuple[str, str, int, str]], out_path: Path) -> None:
    out_path.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# 如喜 (RuhI) 智慧型四角號碼輸入法 4×5 數字小鍵盤核心碼表 (Four-Corner Smart IME Table)\n"
        "# 欄位格式：CODE<TAB>TEXT<TAB>WEIGHT<TAB>KIND\n"
        "# KIND 類別：full(標準四角單字), variant(正異體雙向共享), fuzzy(初學者直覺寬容), "
        "phrase22(雙字前兩角簡碼), phrase3(三字詞簡碼), phrase4(四字成語簡碼), symbol(小鍵盤標點)\n"
    )
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(header)
        for code, text, weight, kind in entries:
            f.write(f"{code}\t{text}\t{weight}\t{kind}\n")


def export_rime_fcitx5_schema(entries: List[Tuple[str, str, int, str]], schema_dir: Path) -> None:
    """
    輸出跨平台 Rime / Fcitx5 / Trime (Linux / Android) 獨立可掛載方案檔：
    - four_corner.schema.yaml
    - four_corner.dict.yaml
    """
    schema_dir.mkdir(parents=True, exist_ok=True)
    schema_yaml = """# Rime schema settings
# encoding: utf-8
# 如喜 (RuhI) 智慧型四角號碼輸入法 (4×5 數字小鍵盤專用跨平台方案)

schema:
  schema_id: four_corner
  name: "如喜智慧四角"
  version: "1.0.0"
  author:
    - "RuhI Team (如喜智慧輸入法架構)"
  description: |
    4×5 數字小鍵盤智慧型四角號碼輸入法：
    一橫二垂三點捺，點下帶橫變零頭；叉四插五方塊六，七角八八小是九。
    支援單字 4/5 碼、雙字詞「前兩角 2+2 簡碼」（如 2332=台灣、1072=電腦）、
    正異體雙向共享與 `.` 萬用字元查詢。

switches:
  - name: ascii_mode
    reset: 0
    states: ["四", "英"]
  - name: full_shape
    states: ["半", "全"]
  - name: traditionalization
    reset: 1
    states: ["簡", "繁"]

engine:
  processors:
    - ascii_composer
    - recognizer
    - key_binder
    - speller
    - punctuator
    - selector
    - navigator
    - express_editor
  segmentors:
    - ascii_segmentor
    - matcher
    - abc_segmentor
    - punct_segmentor
    - fallback_segmentor
  translators:
    - punct_translator
    - table_translator
  filters:
    - uniquifier

speller:
  alphabet: "0123456789/"
  initials: "0123456789/"
  max_code_length: 8
  auto_select: false

translator:
  dictionary: four_corner
  enable_charset_filter: false
  enable_sentence: true
  enable_encoder: true
  encode_commit_history: true
  max_phrase_length: 4

key_binder:
  import_preset: default
  bindings:
    - { when: has_menu, accept: KP_Add, send: space }
    - { when: has_menu, accept: plus, send: space }
    - { when: has_menu, accept: KP_Subtract, send: Page_Up }
    - { when: has_menu, accept: KP_Multiply, send: Page_Down }

recognizer:
  import_preset: default
"""
    (schema_dir / "four_corner.schema.yaml").write_text(schema_yaml, encoding="utf-8", newline="\n")

    dict_header = """# Rime dictionary
# encoding: utf-8
# 如喜 (RuhI) 智慧型四角號碼詞庫 (含單字標準碼、正異體共享、初學者容錯與現代漢語前兩角簡碼矩陣)

---
name: four_corner
version: "1.0.0"
sort: by_weight
use_preset_vocabulary: false
columns:
  - text
  - code
  - weight
...
"""
    with open(schema_dir / "four_corner.dict.yaml", "w", encoding="utf-8", newline="\n") as f:
        f.write(dict_header)
        for code, text, weight, _kind in entries:
            f.write(f"{text}\t{code}\t{weight}\n")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="編譯如喜 (RuhI) 智慧型四角號碼輸入法核心碼表與前兩角簡碼矩陣"
    )
    repo_root = Path(__file__).resolve().parent.parent
    parser.add_argument(
        "--unihan-cache",
        type=Path,
        default=repo_root / "_archive" / "_external" / "four_corner_upstream",
        help="Unihan.zip 快取目錄",
    )
    parser.add_argument(
        "--lexicon-dir",
        type=Path,
        default=repo_root / "ruhi_lexicon" / "data",
        help="如喜核心詞庫目錄",
    )
    parser.add_argument(
        "--out-tsv",
        type=Path,
        default=repo_root / "ruhi_lexicon" / "data" / "four_corner.tsv",
        help="輸出之 four_corner.tsv 路徑",
    )
    parser.add_argument(
        "--mirror-tsv",
        type=Path,
        default=repo_root / "scripts" / "data" / "four_corner.tsv",
        help="鏡像輸出之 scripts/data/four_corner.tsv 路徑",
    )
    parser.add_argument(
        "--schema-dir",
        type=Path,
        default=repo_root / "dist" / "four_corner_schema",
        help="跨平台 Rime/Fcitx5 方案檔輸出目錄",
    )
    args = parser.parse_args()

    zip_path = ensure_unihan_zip(args.unihan_cache)
    print(f"[1/4] 解析 Unihan 四角號碼與異體字關聯：{zip_path}")
    char_to_codes, variants_map = parse_unihan(zip_path)
    print(f"      -> 取得 Unihan 標準四角號碼單字：{len(char_to_codes):,} 字，異體字關聯：{len(variants_map):,} 組")

    print(f"[2/4] 載入如喜語料庫單字與現代漢語詞彙權重：{args.lexicon_dir}")
    char_weights, word_weights = load_ruhi_lexicons(args.lexicon_dir)
    print(f"      -> 單字權重：{len(char_weights):,} 字，多字詞權重：{len(word_weights):,} 詞")

    print("[3/4] 編譯三軌相容單字碼與「前兩角簡碼矩陣」...")
    entries = compile_four_corner_entries(char_to_codes, variants_map, char_weights, word_weights)

    kind_counts: Dict[str, int] = defaultdict(int)
    for _c, _t, _w, k in entries:
        kind_counts[k] += 1

    print(f"[4/4] 寫入核心碼表與跨平台方案檔（總計 {len(entries):,} 筆條目）...")
    write_tsv(entries, args.out_tsv)
    if args.mirror_tsv:
        write_tsv(entries, args.mirror_tsv)
    for extra_target in (
        repo_root / "data" / "four_corner.tsv",
        repo_root / "dist" / "data" / "four_corner.tsv",
        repo_root / "dist" / "four_corner_schema" / "four_corner.tsv",
        repo_root / "dist" / "RuhI_FourCorner_Numpad_Win64" / "data" / "four_corner.tsv",
    ):
        if extra_target.parent.exists():
            write_tsv(entries, extra_target)
    export_rime_fcitx5_schema(entries, args.schema_dir)

    print("\n================ 編譯統計摘要 ================")
    for k, cnt in sorted(kind_counts.items(), key=lambda x: -x[1]):
        print(f"  - {k:<10}: {cnt:>8,} 筆")
    print(f"  * 主碼表路徑 : {args.out_tsv} ({args.out_tsv.stat().st_size / 1024:.1f} KB)")
    print(f"  * 跨平台方案 : {args.schema_dir}")

    # 抽樣檢驗指標性測項
    sample_queries = [
        "2360", "4010", "2366", "2332", "4032", "0044", "1072", "4640",
        "50843", "26300", "41300", "30203", "622612", "107164",
    ]
    by_code: Dict[str, List[Tuple[str, int, str]]] = defaultdict(list)
    for c, t, w, k in entries:
        by_code[c].append((t, w, k))
    print("\n================ 指標性編碼抽樣驗證 ================")
    for q in dict.fromkeys(sample_queries):
        top_hits = by_code.get(q, [])[:5]
        formatted = ", ".join(f"{t}({w}:{k})" for t, w, k in top_hits)
        print(f"  [{q}] -> {formatted}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
