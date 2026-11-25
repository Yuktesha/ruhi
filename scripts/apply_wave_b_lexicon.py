#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）Wave B 現代台灣詞彙與技術詞庫擴充／消融套用工具 (apply_wave_b_lexicon.py)

用途：
1. 套用經過台灣國語讀音人工覆核之現代台灣生活與資訊科學專業詞表 (`CURATED_MODERN_TW_TECH`)；
2. 依消融測試參數 `--max-auto N` 併入語料庫探勘之單音高內聚 OOV 詞條 (`auto_monophonic.tsv`)；
3. 依 `--include-underweighted` 套用低權重高頻實詞校準 (`underweighted_audit.tsv` + `CURATED_UNDERWEIGHTED_LIFT`)；
4. 嚴格遵守 3 欄格式 (`WORD\tBOPOMOFO\tWEIGHT`)、`60..=1000` 權重上限、連讀變調正規化 (`canonicalize_sandhi`)、
   `rejected_readings.tsv` 排重黑名單、絕不更動單字權重 (`len == 1`)，並同步至執行期鏡像。
"""
from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import sys

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT_DIR = SCRIPT_DIR.parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from purify_core_lexicon import canonicalize_sandhi, normalize_bpm  # noqa: E402

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

CORE_LEXICON = ROOT_DIR / "ruhi_lexicon" / "data" / "lexicon_core.tsv"
REJECTED_PATH = ROOT_DIR / "ruhi_lexicon" / "data" / "rejected_readings.tsv"
DEFAULT_MINED_DIR = ROOT_DIR / "eval_reports" / "local" / "oov_wave_b_v2"
DEFAULT_BACKUP = DEFAULT_MINED_DIR / "lexicon_core.tsv.bak"

MIRRORS_CORE = [
    ROOT_DIR / "data" / "lexicon_core.tsv",
]
MIRRORS_EXTRA = [
    ROOT_DIR / "dist" / "data" / "lexicon_core.tsv",
    ROOT_DIR / "scripts" / "data" / "lexicon_core.tsv",
    ROOT_DIR / "ruhi-android" / "app" / "src" / "main" / "assets" / "ruhi_data" / "lexicon_core.tsv",
]

# 經台灣國語標準讀音核對之現代資訊科學與台灣日常高頻複合詞（嚴禁句法片語與中國大陸網通黑話）
CURATED_MODERN_TW_TECH: list[tuple[str, str, int]] = [
    # 資訊科學 / AI / 半導體
    ("算力", "ㄙㄨㄢˋ ㄌㄧˋ", 340),
    ("字級", "ㄗˋ ㄐㄧˊ", 320),
    ("語源", "ㄩˇ ㄩㄢˊ", 320),
    ("值守", "ㄓˊ ㄕㄡˇ", 300),
    ("壓測", "ㄧㄚ ㄘㄜˋ", 320),
    ("光刻機", "ㄍㄨㄤ ㄎㄜˋ ㄐㄧ", 340),
    ("光刻機", "ㄍㄨㄤ ㄎㄜ ㄐㄧ", 310),
    ("神經網路", "ㄕㄣˊ ㄐㄧㄥ ㄨㄤˇ ㄌㄨˋ", 360),
    ("詞彙表", "ㄘˊ ㄏㄨㄟˋ ㄅㄧㄠˇ", 330),
    ("模型訓練", "ㄇㄛˊ ㄒㄧㄥˊ ㄒㄩㄣˋ ㄌㄧㄢˋ", 340),
    ("重排序", "ㄔㄨㄥˊ ㄆㄞˊ ㄒㄩˋ", 330),
    ("執行緒", "ㄓˊ ㄒㄧㄥˊ ㄒㄩˋ", 360),
    ("非同步", "ㄈㄟ ㄊㄨㄥˊ ㄅㄨˋ", 350),
    ("同步化", "ㄊㄨㄥˊ ㄅㄨˋ ㄏㄨㄚˋ", 320),
    ("函式庫", "ㄏㄢˊ ㄕˋ ㄎㄨˋ", 360),
    ("函式呼叫", "ㄏㄢˊ ㄕˋ ㄏㄨ ㄐㄧㄠˋ", 320),
    ("函式名稱", "ㄏㄢˊ ㄕˋ ㄇㄧㄥˊ ㄔㄥ", 320),
    ("容器化", "ㄖㄨㄥˊ ㄑㄧˋ ㄏㄨㄚˋ", 330),
    ("微服務", "ㄨㄟˊ ㄈㄨˊ ㄨˋ", 340),
    ("邊緣運算", "ㄅㄧㄢ ㄩㄢˊ ㄩㄣˋ ㄙㄨㄢˋ", 340),
    ("區塊鏈", "ㄑㄩ ㄎㄨㄞˋ ㄌㄧㄢˋ", 380),
    ("元宇宙", "ㄩㄢˊ ㄩˇ ㄓㄡˋ", 360),
    ("無人機", "ㄨˊ ㄖㄣˊ ㄐㄧ", 380),
    ("自駕車", "ㄗˋ ㄐㄧㄚˋ ㄔㄜ", 360),
    ("自駕", "ㄗˋ ㄐㄧㄚˋ", 320),
    ("比特幣", "ㄅㄧˇ ㄊㄜˋ ㄅㄧˋ", 360),
    ("域名", "ㄩˋ ㄇㄧㄥˊ", 320),
    ("標註", "ㄅㄧㄠ ㄓㄨˋ", 330),
    ("解決方案", "ㄐㄧㄝˇ ㄐㄩㄝˊ ㄈㄤ ㄢˋ", 360),
    ("基礎設施", "ㄐㄧ ㄔㄨˇ ㄕㄜˋ ㄕ", 360),
    ("基礎建設", "ㄐㄧ ㄔㄨˇ ㄐㄧㄢˋ ㄕㄜˋ", 360),
    ("生態系統", "ㄕㄥ ㄊㄞˋ ㄒㄧˋ ㄊㄨㄥˇ", 350),
    ("數據分析", "ㄕㄨˋ ㄐㄩˋ ㄈㄣ ㄒㄧ", 350),
    ("多項式", "ㄉㄨㄛ ㄒㄧㄤˋ ㄕˋ", 340),
    ("多層次", "ㄉㄨㄛ ㄘㄥˊ ㄘˋ", 330),
    ("代入", "ㄉㄞˋ ㄖㄨˋ", 320),
    ("隔詞", "ㄍㄜˊ ㄘˊ", 260),
    # 現代台灣生活 / 數位金融 / 公共政策
    ("臉書", "ㄌㄧㄢˇ ㄕㄨ", 400),
    ("粉專", "ㄈㄣˇ ㄓㄨㄢ", 340),
    ("限動", "ㄒㄧㄢˋ ㄉㄨㄥˋ", 330),
    ("業配", "ㄧㄝˋ ㄆㄟˋ", 350),
    ("推播", "ㄊㄨㄟ ㄅㄛˋ", 340),
    ("私訊", "ㄙ ㄒㄩㄣˋ", 360),
    ("小編", "ㄒㄧㄠˇ ㄅㄧㄢ", 350),
    ("免運", "ㄇㄧㄢˇ ㄩㄣˋ", 350),
    ("網銀", "ㄨㄤˇ ㄧㄣˊ", 340),
    ("行動支付", "ㄒㄧㄥˊ ㄉㄨㄥˋ ㄓ ㄈㄨˋ", 380),
    ("一卡通", "ㄧˋ ㄎㄚˇ ㄊㄨㄥ", 360),
    ("社群媒體", "ㄕㄜˋ ㄑㄩㄣˊ ㄇㄟˊ ㄊㄧˇ", 380),
    ("社交媒體", "ㄕㄜˋ ㄐㄧㄠ ㄇㄟˊ ㄊㄧˇ", 360),
    ("充電樁", "ㄔㄨㄥ ㄉㄧㄢˋ ㄓㄨㄤ", 360),
    ("碳排", "ㄊㄢˋ ㄆㄞˊ", 360),
    ("淨零", "ㄐㄧㄥˋ ㄌㄧㄥˊ", 360),
    ("低碳", "ㄉㄧ ㄊㄢˋ", 350),
    ("綠能", "ㄌㄩˋ ㄋㄥˊ", 380),
    ("風電", "ㄈㄥ ㄉㄧㄢˋ", 350),
    ("儲能", "ㄔㄨˇ ㄋㄥˊ", 360),
    ("環評", "ㄏㄨㄢˊ ㄆㄧㄥˊ", 360),
    ("氣候變遷", "ㄑㄧˋ ㄏㄡˋ ㄅㄧㄢˋ ㄑㄧㄢ", 380),
    ("空氣品質", "ㄎㄨㄥ ㄑㄧˋ ㄆㄧㄣˇ ㄓˊ", 370),
    ("長照", "ㄔㄤˊ ㄓㄠˋ", 380),
    ("社宅", "ㄕㄜˋ ㄓㄞˊ", 360),
    ("公宅", "ㄍㄨㄥ ㄓㄞˊ", 340),
    ("少子化", "ㄕㄠˇ ㄗˇ ㄏㄨㄚˋ", 380),
    ("新住民", "ㄒㄧㄣ ㄓㄨˋ ㄇㄧㄣˊ", 380),
    ("移工", "ㄧˊ ㄍㄨㄥ", 380),
    ("偏鄉", "ㄆㄧㄢ ㄒㄧㄤ", 380),
    ("實名制", "ㄕˊ ㄇㄧㄥˊ ㄓˋ", 370),
    ("跨領域", "ㄎㄨㄚˋ ㄌㄧㄥˇ ㄩˋ", 360),
    ("跨部門", "ㄎㄨㄚˋ ㄅㄨˋ ㄇㄣˊ", 350),
    ("非營利", "ㄈㄟ ㄧㄥˊ ㄌㄧˋ", 350),
    ("民眾黨", "ㄇㄧㄣˊ ㄓㄨㄥˋ ㄉㄤˇ", 380),
    ("召委", "ㄓㄠˋ ㄨㄟˇ", 360),
    ("朝野協商", "ㄔㄠˊ ㄧㄝˇ ㄒㄧㄝˊ ㄕㄤ", 360),
    ("憲法法庭", "ㄒㄧㄢˋ ㄈㄚˇ ㄈㄚˇ ㄊㄧㄥˊ", 380),
    ("外役監", "ㄨㄞˋ ㄧˋ ㄐㄧㄢ", 360),
    ("主計總處", "ㄓㄨˇ ㄐㄧˋ ㄗㄨㄥˇ ㄔㄨˇ", 360),
    ("特別預算", "ㄊㄜˋ ㄅㄧㄝˊ ㄩˋ ㄙㄨㄢˋ", 350),
    ("附帶決議", "ㄈㄨˋ ㄉㄞˋ ㄐㄩㄝˊ ㄧˋ", 340),
    ("揭弊", "ㄐㄧㄝ ㄅㄧˋ", 350),
    ("性騷", "ㄒㄧㄥˋ ㄙㄠ", 350),
    ("性侵", "ㄒㄧㄥˋ ㄑㄧㄣ", 360),
    ("退撫", "ㄊㄨㄟˋ ㄈㄨˇ", 350),
    ("裁罰", "ㄘㄞˊ ㄈㄚˊ", 350),
    ("醫療機構", "ㄧ ㄌㄧㄠˊ ㄐㄧ ㄍㄡˋ", 350),
    ("公共交通", "ㄍㄨㄥ ㄍㄨㄥˋ ㄐㄧㄠ ㄊㄨㄥ", 350),
    ("客群", "ㄎㄜˋ ㄑㄩㄣˊ", 350),
    ("受眾", "ㄕㄡˋ ㄓㄨㄥˋ", 350),
    ("入住", "ㄖㄨˋ ㄓㄨˋ", 350),
    ("高端", "ㄍㄠ ㄉㄨㄢ", 350),
    ("瑜珈", "ㄩˊ ㄐㄧㄚ", 360),
    ("狗狗", "ㄍㄡˇ ㄍㄡˇ", 340),
    ("幸福感", "ㄒㄧㄥˋ ㄈㄨˊ ㄍㄢˇ", 340),
    ("貿易戰", "ㄇㄠˋ ㄧˋ ㄓㄢˋ", 360),
    ("完整版", "ㄨㄢˊ ㄓㄥˇ ㄅㄢˇ", 340),
]

CURATED_UNDERWEIGHTED_LIFT: dict[tuple[str, str], int] = {
    ("全域", "ㄑㄩㄢˊ ㄩˋ"): 320,
    ("先驗", "ㄒㄧㄢ ㄧㄢˋ"): 320,
    ("異義", "ㄧˋ ㄧˋ"): 280,
    ("前綴", "ㄑㄧㄢˊ ㄓㄨㄟˋ"): 300,
    ("後綴", "ㄏㄡˋ ㄓㄨㄟˋ"): 300,
    ("多工", "ㄉㄨㄛ ㄍㄨㄥ"): 300,
    ("直譯器", "ㄓˊ ㄧˋ ㄑㄧˋ"): 300,
    ("博弈", "ㄅㄛˊ ㄧˋ"): 300,
    ("外送", "ㄨㄞˋ ㄙㄨㄥˋ"): 340,
    ("手搖飲", "ㄕㄡˇ ㄧㄠˊ ㄧㄣˇ"): 340,
}

# 排除少量未完全過濾之兩岸慣用語變體、句法殘片與單調弱詞
EXPLICIT_SKIP_WORDS: frozenset[str] = frozenset({
    "擁堵", "運營", "西蘭花", "黨黨團", "邊形", "斯坦", "克斯",
    "改主決議", "另有規定", "發性", "三種", "年增", "手作", "量大", "道是",
    "桐花",
})


def load_rejected() -> tuple[set[str], set[tuple[str, str]]]:
    rej_words: set[str] = set()
    rej_pairs: set[tuple[str, str]] = set()
    if REJECTED_PATH.is_file():
        for line in REJECTED_PATH.read_text(encoding="utf-8").splitlines():
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            p = s.split("\t")
            if len(p) >= 2:
                w = p[0].strip()
                b = p[1].strip()
                if b == "*":
                    rej_words.add(w)
                else:
                    rej_pairs.add((w, normalize_bpm(b)))
    return rej_words, rej_pairs


def sync_mirrors(src: Path, include_extra: bool) -> None:
    targets = list(MIRRORS_CORE) + (list(MIRRORS_EXTRA) if include_extra else [])
    for dst in targets:
        if dst.parent.is_dir():
            shutil.copy2(src, dst)


def main() -> int:
    parser = argparse.ArgumentParser(description="如喜 Wave B 詞庫擴充與消融套用工具")
    parser.add_argument("--mined-dir", type=Path, default=DEFAULT_MINED_DIR)
    parser.add_argument("--backup", type=Path, default=DEFAULT_BACKUP)
    parser.add_argument("--restore-backup", action="store_true")
    parser.add_argument("--max-auto", type=int, default=0)
    parser.add_argument("--include-underweighted", action="store_true")
    parser.add_argument("--sync-all", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    args.backup.parent.mkdir(parents=True, exist_ok=True)
    if args.restore_backup:
        if not args.backup.is_file():
            raise FileNotFoundError(f"找不到備份檔：{args.backup}")
        shutil.copy2(args.backup, CORE_LEXICON)
        sync_mirrors(CORE_LEXICON, include_extra=args.sync_all)
        print(f"[restore] 已從 {args.backup} 還原並同步詞庫")
        return 0

    if not args.backup.is_file():
        shutil.copy2(CORE_LEXICON, args.backup)
        print(f"[backup] 已建立基準備份：{args.backup}")

    rej_words, rej_pairs = load_rejected()
    base_lines = args.backup.read_text(encoding="utf-8").splitlines()

    header_lines: list[str] = []
    entries: list[tuple[str, str, int]] = []
    seen_pairs: set[tuple[str, str]] = set()
    existing_words: set[str] = set()

    for line in base_lines:
        s = line.strip()
        if not s or s.startswith("#"):
            if not entries:
                header_lines.append(line)
            continue
        p = s.split("\t")
        if len(p) != 3:
            raise ValueError(f"基礎詞庫格式異常：{line!r}")
        w, b, wt = p[0], normalize_bpm(p[1]), int(p[2])
        entries.append((w, b, wt))
        seen_pairs.add((w, b))
        existing_words.add(w)

    # 1. 準備低權重校準表（嚴格限制 len >= 2，絕不更動單字先驗權重）
    lift_map: dict[tuple[str, str], int] = {}
    if args.include_underweighted:
        under_tsv = args.mined_dir / "underweighted_audit.tsv"
        if under_tsv.is_file():
            for line in under_tsv.read_text(encoding="utf-8").splitlines():
                s = line.strip()
                if not s or s.startswith("#"):
                    continue
                p = s.split("\t")
                w = p[0].strip()
                if w in EXPLICIT_SKIP_WORDS:
                    continue
                b = normalize_bpm(canonicalize_sandhi(w, p[1].strip()))
                sug_wt = max(60, min(1000, int(p[3])))
                if len(w) >= 2 and (w, b) in seen_pairs:
                    lift_map[(w, b)] = max(lift_map.get((w, b), 0), sug_wt)
        for (w, raw_b), sug_wt in CURATED_UNDERWEIGHTED_LIFT.items():
            b = normalize_bpm(canonicalize_sandhi(w, raw_b))
            if len(w) >= 2 and (w, b) in seen_pairs:
                lift_map[(w, b)] = max(lift_map.get((w, b), 0), sug_wt)

    lifted_count = 0
    for idx, (w, b, wt) in enumerate(entries):
        if len(w) >= 2 and (w, b) in lift_map and lift_map[(w, b)] > wt:
            entries[idx] = (w, b, lift_map[(w, b)])
            lifted_count += 1

    # 2. 加入人工覆核現代台灣／技術詞表
    added_curated = 0
    for w, raw_b, wt in CURATED_MODERN_TW_TECH:
        assert len(w) >= 2, f"嚴禁修改或新增單字詞條：{w}"
        assert 60 <= wt <= 1000
        b = normalize_bpm(canonicalize_sandhi(w, raw_b))
        assert len(w) == len(b.split()), f"注音音節數不符：{w} -> {b}"
        if w in rej_words or (w, b) in rej_pairs or (w, b) in seen_pairs:
            continue
        entries.append((w, b, wt))
        seen_pairs.add((w, b))
        existing_words.add(w)
        added_curated += 1

    # 3. 依 --max-auto 加入高內聚單音 OOV 詞條
    added_auto = 0
    if args.max_auto > 0:
        mono_tsv = args.mined_dir / "auto_monophonic.tsv"
        for line in mono_tsv.read_text(encoding="utf-8").splitlines():
            if added_auto >= args.max_auto:
                break
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            p = s.split("\t")
            w = p[0].strip()
            if w in EXPLICIT_SKIP_WORDS:
                continue
            raw_b = p[1].strip()
            wt = max(60, min(1000, int(p[2])))
            b = normalize_bpm(canonicalize_sandhi(w, raw_b))
            if len(w) < 2 or len(w) != len(b.split()):
                continue
            if w in rej_words or (w, b) in rej_pairs or (w, b) in seen_pairs or w in existing_words:
                continue
            entries.append((w, b, wt))
            seen_pairs.add((w, b))
            existing_words.add(w)
            added_auto += 1

    entries.sort(key=lambda r: (r[0], -r[2], r[1]))
    print(
        f"[apply] curated_added={added_curated} | auto_added={added_auto} | "
        f"underweighted_lifted={lifted_count} | total_entries={len(entries)}"
    )
    if args.dry_run:
        return 0

    with CORE_LEXICON.open("w", encoding="utf-8", newline="\n") as f:
        for h in header_lines:
            f.write(h + "\n")
        for w, b, wt in entries:
            f.write(f"{w}\t{b}\t{wt}\n")

    sync_mirrors(CORE_LEXICON, include_extra=args.sync_all)
    print(f"[sync] 已更新並同步詞庫至執行期鏡像 (sync_all={args.sync_all})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
