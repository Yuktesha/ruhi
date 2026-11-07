#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）選字規則靜態檢查工具 (lint_scoring_rules.py)

檢查「新增的」選字規則是否符合交接文件 §4.3 與修訂版交接提示詞「一、3」之兩項可機械檢查條件：

1. 分數絕對值上限：新增之 `bonus += / -= N`、`score: N`、TSV 第三欄分數，其絕對值不得超過 --max-score（預設 35）。
2. 不得以句子片段為語境條件：在 `prev_word` / `prev_clean` / `prev2_word` / `rw` / `next` 等語境比對行中，
   不得出現 3 個漢字以上的字串常值（例如 `金銀紙原`、`木塊內壁`）。

只檢查相對於 --base 的新增行（git diff -U0），既有規則不在此檢查範圍內。

用法：
    python scripts/lint_scoring_rules.py                       # 檢查工作目錄相對 HEAD 的新增行
    python scripts/lint_scoring_rules.py --base main           # 相對 main
    python scripts/lint_scoring_rules.py --staged              # 只檢查暫存區（供 pre-commit 使用）

結束碼：0 = 通過；1 = 有違規。
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SCORING_PATHS = [
    "ruhi-core/src/router/constructions.rs",
    "ruhi-core/src/router/transitions.rs",
    "ruhi_lexicon/data/transitions_user.tsv",
    "ruhi_lexicon/data/patterns_user.jsonl",
]

RE_BONUS = re.compile(r"\bbonus\s*[+-]=\s*([0-9]+(?:\.[0-9]+)?)")
RE_SCORE_FIELD = re.compile(r"\"?(?:score|bonus)\"?\s*[:=]\s*(-?[0-9]+(?:\.[0-9]+)?)")
RE_CTX_LINE = re.compile(r"\b(prev_word|prev_clean|prev2_word|prev3_word|rw|next_word|nw)\b")
RE_LONG_CJK = re.compile(r"\"([\u3400-\u9fff\uf900-\ufaff]{3,})\"")

GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
RESET = "\033[0m"


def git_diff(base: str | None, staged: bool, target: str | None = None) -> str:
    cmd = ["git", "-C", str(ROOT), "diff", "-U0"]
    if staged:
        cmd.append("--cached")
    elif base:
        cmd.append(base)
        if target:
            cmd.append(target)
    cmd += ["--", *SCORING_PATHS]
    return subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace").stdout


def parse_added(diff: str):
    """回傳 (檔案, 新檔行號, 內容) 之新增行清單。"""
    cur_file = None
    new_ln = 0
    for line in diff.splitlines():
        if line.startswith("+++ "):
            cur_file = line[6:] if line.startswith("+++ b/") else None
            continue
        if line.startswith("@@"):
            m = re.search(r"\+(\d+)", line)
            new_ln = int(m.group(1)) if m else 0
            continue
        if cur_file is None:
            continue
        if line.startswith("+") and not line.startswith("+++"):
            yield cur_file, new_ln, line[1:]
            new_ln += 1


def check(added, max_score: float):
    violations = []
    for f, ln, text in added:
        if f.endswith(".tsv"):
            cols = text.split("\t")
            if len(cols) >= 3:
                try:
                    v = abs(float(cols[2]))
                    if v > max_score:
                        violations.append((f, ln, f"分數 {cols[2]} 超過上限 ±{max_score:g}", text))
                except ValueError:
                    pass
            for c in cols[:2]:
                if len(re.findall(r"[\u3400-\u9fff]", c)) >= 4:
                    violations.append((f, ln, f"語境欄位「{c}」為 4 字以上片段", text))
            continue
        scores = [float(x) for x in RE_BONUS.findall(text)]
        if f.endswith(".jsonl"):
            scores += [abs(float(x)) for x in RE_SCORE_FIELD.findall(text)]
        for v in scores:
            if v > max_score:
                violations.append((f, ln, f"分數 {v:g} 超過上限 ±{max_score:g}", text.strip()))
        if f.endswith(".rs") and RE_CTX_LINE.search(text):
            for frag in RE_LONG_CJK.findall(text):
                violations.append((f, ln, f"語境條件使用 3 字以上片段「{frag}」", text.strip()))
    return violations


def main() -> int:
    ap = argparse.ArgumentParser(description="如喜選字規則靜態檢查（分數上限與句子片段條件）")
    ap.add_argument("--base", default="HEAD", help="比較基準（預設 HEAD）")
    ap.add_argument("--target", default=None, help="比較目標 commit／分支（省略時為工作目錄）")
    ap.add_argument("--staged", action="store_true", help="只檢查暫存區")
    ap.add_argument("--max-score", type=float, default=35.0, help="分數絕對值上限（預設 35）")
    ap.add_argument("--limit", type=int, default=30, help="最多列出幾筆違規（預設 30）")
    args = ap.parse_args()

    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")

    added = list(parse_added(git_diff(args.base, args.staged, args.target)))
    violations = check(added, args.max_score)
    print(f"檢查新增行數：{len(added)}")
    if not violations:
        print(f"{GREEN}[通過]{RESET} 未發現違規。")
        return 0
    print(f"{RED}[未通過]{RESET} 共 {len(violations)} 筆違規：")
    for f, ln, reason, text in violations[: args.limit]:
        print(f"  {YELLOW}{f}:{ln}{RESET}  {reason}")
        print(f"      {text[:120]}")
    if len(violations) > args.limit:
        print(f"  …其餘 {len(violations) - args.limit} 筆省略（可用 --limit 調整）")
    return 1


if __name__ == "__main__":
    sys.exit(main())
