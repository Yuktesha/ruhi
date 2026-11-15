#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
由 Unihan 官方資料表 (Unihan_IRGSources.txt) 建置 BMP 漢字康熙字典筆畫二進位查表 (kangxi_strokes_bmp.bin)。

涵蓋範圍：U+4E00..=U+9FFF（共 20,992 個漢字，每個漢字 1 byte，O(1) 查詢）。
康熙部首折算規則（對齊傳統熊崎氏姓名學／康熙字典規範）：
- 氵(水部 #85) = 4 畫、艹(艸部 #140) = 6 畫、辶(辵部 #162) = 7 畫
- 左耳旁(阜部 #170) = 8 畫、右耳旁(邑部 #163) = 7 畫
- 王字旁(玉部 #96) = 5 畫（獨立「王」字於 Unihan 標記為 96.-1 -> 4 畫，「玉」字為 96.0 -> 5 畫）
- 忄(心部 #61) = 4 畫、扌(手部 #64) = 4 畫、礻(示部 #113) = 5 畫、衤(衣部 #145) = 6 畫、月(肉部 #130) = 6 畫
- 數字專名筆畫對齊姓名學慣例：一(1)〜十(10)、百(6)、千(3)、萬(15)、廿(20)、卅(30)。
"""
from __future__ import annotations

import zipfile
from pathlib import Path

# 214 個康熙部首標準筆畫數（索引 1..=214）
KANGXI_RAD_STROKES = [
    0,
    # 1..6 (1 畫)
    1, 1, 1, 1, 1, 1,
    # 7..29 (2 畫)
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    # 30..60 (3 畫)
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    # 61..94 (4 畫)
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    # 95..117 (5 畫)
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    # 118..146 (6 畫)
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    # 147..166 (7 畫)
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    # 167..175 (8 畫)
    8, 8, 8, 8, 8, 8, 8, 8, 8,
    # 176..186 (9 畫)
    9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
    # 187..194 (10 畫)
    10, 10, 10, 10, 10, 10, 10, 10,
    # 195..200 (11 畫)
    11, 11, 11, 11, 11, 11,
    # 201..204 (12 畫)
    12, 12, 12, 12,
    # 205..208 (13 畫)
    13, 13, 13, 13,
    # 209..210 (14 畫)
    14, 14,
    # 211 (15 畫)
    15,
    # 212..213 (16 畫)
    16, 16,
    # 214 (17 畫)
    17,
]
assert len(KANGXI_RAD_STROKES) == 215

NAMING_OVERRIDES = {
    "一": 1, "二": 2, "三": 3, "四": 4, "五": 5,
    "六": 6, "七": 7, "八": 8, "九": 9, "十": 10,
    "百": 6, "千": 3, "萬": 15, "廿": 20, "卅": 30,
}


def main() -> None:
    repo_root = Path(__file__).resolve().parent.parent
    unihan_zip = repo_root / "_archive" / "_external" / "four_corner_upstream" / "Unihan.zip"
    out_path = repo_root / "ruhi_lexicon" / "data" / "kangxi_strokes_bmp.bin"

    rs_map: dict[int, int] = {}
    ts_map: dict[int, int] = {}

    with zipfile.ZipFile(unihan_zip) as zf:
        for line in zf.read("Unihan_IRGSources.txt").decode("utf-8").splitlines():
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3 or not parts[0].startswith("U+"):
                continue
            cp = int(parts[0][2:], 16)
            if 0x4E00 <= cp <= 0x9FFF:
                if parts[1] == "kRSUnicode":
                    tok = parts[2].split()[0]
                    rad_part, res_part = tok.split(".")
                    rad_id = int(rad_part.rstrip("'"))
                    res = int(res_part)
                    rs_map[cp] = KANGXI_RAD_STROKES[rad_id] + res
                elif parts[1] == "kTotalStrokes":
                    vals = [int(x) for x in parts[2].split()]
                    ts_map[cp] = vals[-1]

    out = bytearray(0x9FFF - 0x4E00 + 1)
    for cp in range(0x4E00, 0x9FFF + 1):
        ch = chr(cp)
        if ch in NAMING_OVERRIDES:
            st = NAMING_OVERRIDES[ch]
        else:
            st = max(rs_map.get(cp, 0), ts_map.get(cp, 0))
        out[cp - 0x4E00] = min(max(st, 1), 64)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(bytes(out))
    print(f"[OK] Wrote {out_path} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
