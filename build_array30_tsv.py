#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜輸入法 (RuhI) — 行列 30 (Array 30) 標準開源碼表建置工具
===========================================================
從廖明德先生公開授權、葉光哲 (gontera) 整理維護之官方開源儲存庫 (gontera/array30)
取得最新 v2026 標準碼表：
  1. gcin/ar30-regular.cin (標準漢字拆碼、一二級簡碼)
  2. gcin/ar30-big.cin (補充 CJK Ext-A 字集如「㐀」fai，符合 v2026 檢核標準)
  3. array30_spec/ar30hw.txt (一級簡碼與二級簡碼 1~0 定位碼表)
  4. array30_spec/ar30SpecialCodes.txt (398 個標準二鍵特別碼)
  5. array30_spec/ar30symbol.txt (W0~W9 與 HG1~HG8 符號表)

並結合如喜詞庫 (lexicon_mcbopomofo.tsv / lexicon_moe.tsv) 之常用字頻進行權重校準，
輸出至：
  - ruhi_lexicon/data/array30.tsv
  - scripts/data/array30.tsv
"""

import argparse
import os
import sys
import urllib.request

sys.stdout.reconfigure(encoding="utf-8")

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
UPSTREAM_BASE = "https://raw.githubusercontent.com/gontera/array30/master"

URLS = {
    "regular_cin": f"{UPSTREAM_BASE}/gcin/ar30-regular.cin",
    "big_cin": f"{UPSTREAM_BASE}/gcin/ar30-big.cin",
    "hw_txt": f"{UPSTREAM_BASE}/array30_spec/ar30hw.txt",
    "special_txt": f"{UPSTREAM_BASE}/array30_spec/ar30SpecialCodes.txt",
    "symbol_txt": f"{UPSTREAM_BASE}/array30_spec/ar30symbol.txt",
}

VALID_ARRAY30_KEYS = set("abcdefghijklmnopqrstuvwxyz,./;")


def fetch_text(url: str, cache_dir: str, filename: str) -> str:
    os.makedirs(cache_dir, exist_ok=True)
    cache_path = os.path.join(cache_dir, filename)
    if os.path.exists(cache_path) and os.path.getsize(cache_path) > 100:
        with open(cache_path, "r", encoding="utf-8-sig") as f:
            return f.read()
    print(f"  [下載] {url} ...")
    req = urllib.request.Request(url, headers={"User-Agent": "RuhI-Array30-Builder/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        raw = resp.read().decode("utf-8-sig", errors="replace")
    with open(cache_path, "w", encoding="utf-8") as f:
        f.write(raw)
    return raw


def load_ruhi_char_freq(base_dir: str) -> dict[str, int]:
    """從如喜注音詞庫載入單字常用度，用於同碼字的自然排序（不破壞官方優先字）。"""
    char_freq: dict[str, int] = {}
    mcb_path = os.path.join(base_dir, "ruhi_lexicon", "data", "lexicon_mcbopomofo.tsv")
    if os.path.exists(mcb_path):
        with open(mcb_path, "r", encoding="utf-8") as f:
            for line in f:
                parts = line.rstrip("\r\n").split("\t")
                if len(parts) >= 3 and len(parts[0]) == 1:
                    ch = parts[0]
                    try:
                        w = int(float(parts[2]))
                        if w > char_freq.get(ch, -999999):
                            char_freq[ch] = w
                    except ValueError:
                        pass
    return char_freq


def is_cjk_ext_a(ch: str) -> bool:
    if len(ch) != 1:
        return False
    cp = ord(ch)
    return 0x3400 <= cp <= 0x4DBF


def is_bmp_cjk_or_common(ch: str) -> bool:
    if len(ch) != 1:
        return False
    cp = ord(ch)
    return (0x4E00 <= cp <= 0x9FFF) or (0x3400 <= cp <= 0x4DBF) or (0x3000 <= cp <= 0x33FF) or (0xFF00 <= cp <= 0xFFEF)


def parse_ar30hw(text: str) -> tuple[list[tuple[str, str, int, str]], list[tuple[str, str, int, str]]]:
    """解析 ar30hw.txt 一級簡碼與二級簡碼（保留 1..10 槽位順序，忽略 □ 空位）。"""
    short1_entries: list[tuple[str, str, int, str]] = []
    short2_entries: list[tuple[str, str, int, str]] = []

    for raw_line in text.splitlines():
        line = raw_line.split("#")[0].rstrip()
        if not line:
            continue
        # 前面 30 組一級簡碼格式: "A 一到聽現政弄兩而面要"
        # 後面 900 組二級簡碼格式: ",A□□□□□□炳□煩煙" 或 ",A □□□..."
        if len(line) >= 2 and line[0].lower() in VALID_ARRAY30_KEYS and line[1] == " ":
            code = line[0].lower()
            chars_str = line[2:].strip()
            for idx, ch in enumerate(chars_str[:10]):
                if ch != "□" and not ch.isspace():
                    slot = idx + 1  # 1..10 (10 代表鍵盤 0)
                    weight = 1950 - idx * 5
                    short1_entries.append((code, ch, weight, f"short1:{slot}"))
        elif len(line) >= 3 and line[0].lower() in VALID_ARRAY30_KEYS and line[1].lower() in VALID_ARRAY30_KEYS:
            code = line[:2].lower()
            chars_str = line[2:].strip()
            for idx, ch in enumerate(chars_str[:10]):
                if ch != "□" and not ch.isspace():
                    slot = idx + 1
                    weight = 1850 - idx * 5
                    short2_entries.append((code, ch, weight, f"short2:{slot}"))

    return short1_entries, short2_entries


def parse_special_codes(text: str) -> list[tuple[str, str, int, str]]:
    """解析 ar30SpecialCodes.txt 特別碼 (398 字)。"""
    entries: list[tuple[str, str, int, str]] = []
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) >= 2:
            code = parts[0].lower()
            ch = parts[1]
            if all(c in VALID_ARRAY30_KEYS for c in code):
                entries.append((code, ch, 2100, "special"))
    return entries


def parse_symbols(text: str) -> list[tuple[str, str, int, str]]:
    """解析 ar30symbol.txt (w0..w9 與 hg1..hg8 符號表)。"""
    entries: list[tuple[str, str, int, str]] = []
    per_code_count: dict[str, int] = {}
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t") if "\t" in line else line.split()
        if len(parts) >= 2:
            code = parts[0].lower()
            sym = parts[1]
            idx = per_code_count.get(code, 0)
            per_code_count[code] = idx + 1
            weight = max(1000, 2000 - idx * 2)
            entries.append((code, sym, weight, f"symbol:{idx + 1}"))
    return entries


def parse_cin_chardef(cin_text: str, ext_a_only: bool = False) -> list[tuple[str, str]]:
    """解析 .cin 檔案中的 %chardef 區段。"""
    in_chardef = False
    pairs: list[tuple[str, str]] = []
    for raw_line in cin_text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("%chardef"):
            if "begin" in line:
                in_chardef = True
            elif "end" in line:
                in_chardef = False
            continue
        if not in_chardef:
            continue
        parts = line.split()
        if len(parts) >= 2:
            code = parts[0].lower()
            ch = parts[1]
            # 略過純數字 0..9 與符號表 w0..w9 / hg1..hg8 (由 parse_symbols 專門處理)
            if code in {"0", "1", "2", "3", "4", "5", "6", "7", "8", "9"}:
                continue
            if (len(code) == 2 and code.startswith("w") and code[1].isdigit()) or (
                len(code) == 3 and code.startswith("hg") and code[2].isdigit()
            ):
                continue
            if not all(c in VALID_ARRAY30_KEYS for c in code):
                continue
            if ext_a_only and not is_cjk_ext_a(ch):
                continue
            pairs.append((code, ch))
    return pairs


def build_array30_tsv(cache_dir: str, out_paths: list[str]) -> None:
    print("=== 開始建置如喜 (RuhI) 行列 30 標準開源碼表 ===")
    regular_cin = fetch_text(URLS["regular_cin"], cache_dir, "ar30-regular.cin")
    big_cin = fetch_text(URLS["big_cin"], cache_dir, "ar30-big.cin")
    hw_txt = fetch_text(URLS["hw_txt"], cache_dir, "ar30hw.txt")
    special_txt = fetch_text(URLS["special_txt"], cache_dir, "ar30SpecialCodes.txt")
    symbol_txt = fetch_text(URLS["symbol_txt"], cache_dir, "ar30symbol.txt")

    char_freq = load_ruhi_char_freq(BASE_DIR)
    print(f"  已載入如喜單字詞頻參考：{len(char_freq)} 字")

    short1_entries, short2_entries = parse_ar30hw(hw_txt)
    special_entries = parse_special_codes(special_txt)
    symbol_entries = parse_symbols(symbol_txt)

    special_set = {(code, ch) for code, ch, _, _ in special_entries}

    reg_pairs = parse_cin_chardef(regular_cin, ext_a_only=False)
    ext_a_pairs = parse_cin_chardef(big_cin, ext_a_only=True)

    # 合併全碼 (regular + Ext-A)，去除重複
    seen_full: set[tuple[str, str]] = set()
    per_code_rank: dict[str, int] = {}
    full_entries: list[tuple[str, str, int, str]] = []

    for code, ch in reg_pairs + ext_a_pairs:
        if (code, ch) in seen_full:
            continue
        seen_full.add((code, ch))
        # 若該 (code, ch) 已在 special_set 中，仍保留全碼標記或跳過重複（special 已具備更高優先序）
        rank = per_code_rank.get(code, 0)
        per_code_rank[code] = rank + 1

        # 嚴格保持廖明德先生官方 .cin 碼表之重碼字優先順序（如 if -> 1針 2羊 3并）
        # 罕用/簡體字（以 i 結尾且不在如喜正體字頻表）或 Ext-A 字元排於正體常用字之後
        is_rare_i = len(code) >= 3 and code.endswith("i") and ch not in char_freq
        is_ext = is_cjk_ext_a(ch)
        if is_rare_i or is_ext:
            base_w = max(500, 880 - rank * 5)
        else:
            base_w = max(900, 1750 - rank * 12)

        kind = "full"
        if (code, ch) in special_set:
            # 特別碼已在 special_entries 中，不重複輸出同 (code, ch)
            continue
        full_entries.append((code, ch, base_w, kind))

    all_rows: list[tuple[str, str, int, str]] = []
    all_rows.extend(special_entries)
    all_rows.extend(short1_entries)
    all_rows.extend(short2_entries)
    all_rows.extend(full_entries)
    all_rows.extend(symbol_entries)

    print(f"  一級簡碼 (short1) : {len(short1_entries)} 筆")
    print(f"  二級簡碼 (short2) : {len(short2_entries)} 筆")
    print(f"  特別碼   (special): {len(special_entries)} 筆")
    print(f"  標準全碼 (full)   : {len(full_entries)} 筆 (含 CJK Ext-A)")
    print(f"  符號表   (symbol) : {len(symbol_entries)} 筆")
    print(f"  總計輸出條目      : {len(all_rows)} 筆")

    for out_path in out_paths:
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        with open(out_path, "w", encoding="utf-8", newline="\n") as f:
            f.write("# 如喜輸入法 (RuhI) 行列 30 標準開源碼表 (廖明德先生公開授權 / gontera v2026)\n")
            f.write("CODE\tCHAR\tWEIGHT\tKIND\n")
            for code, ch, weight, kind in all_rows:
                f.write(f"{code}\t{ch}\t{weight}\t{kind}\n")
        print(f"  [完成] 已寫入 {out_path} ({os.path.getsize(out_path):,} bytes)")


def main() -> int:
    parser = argparse.ArgumentParser(description="建置如喜輸入法 (RuhI) 行列 30 開源碼表 (array30.tsv)")
    parser.add_argument(
        "--cache-dir",
        default=os.path.join(BASE_DIR, "_external", "array30_upstream"),
        help="上游原始碼表快取目錄",
    )
    args = parser.parse_args()

    out_paths = [
        os.path.join(BASE_DIR, "ruhi_lexicon", "data", "array30.tsv"),
        os.path.join(BASE_DIR, "scripts", "data", "array30.tsv"),
    ]
    build_array30_tsv(args.cache_dir, out_paths)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
