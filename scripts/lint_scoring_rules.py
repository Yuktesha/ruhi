#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）選字規則靜態檢查工具 (lint_scoring_rules.py)

檢查選字／構式評分規則是否符合開發規範：

1. 分數絕對值上限：Rust 規則（`constructions.rs`、`transitions.rs`、`onomastics.rs`、
   `geo_hierarchy.rs`、`ruhi-core/src/western/*.rs`）、`.jsonl`（`score`/`bonus`）、
   `.tsv`（第三欄分數）之數值絕對值不得超過 `--max-score`（預設 35）。
2. 不得以句子片段為語境條件：在 Rust 語境比對行（`prev_word` / `prev_clean` /
   `prev2_word` / `prev3_word` / `rw` / `next_word` / `nw` / `last_word`）中，不得出現
   3 個漢字以上的字串常值；在 `.tsv` 前兩欄中，不得出現 4 個漢字以上的詞組。

預設只檢查相對於 `--base` 的新增行（`git diff -U0`）；傳入 `--full` 時改為全檔掃描
（跳過 `#` 註解行、`//` 註解與 Rust `#[cfg(test)]` 測試區塊）。可另傳入位置參數指定檔案。

用法：
    python scripts/lint_scoring_rules.py                       # 檢查工作目錄相對 HEAD 的新增行
    python scripts/lint_scoring_rules.py --base main           # 相對 main
    python scripts/lint_scoring_rules.py --staged              # 只檢查暫存區（供 pre-commit 使用）
    python scripts/lint_scoring_rules.py --full                # 全檔掃描（全部規則檔）
    python scripts/lint_scoring_rules.py --full path1 path2    # 全檔掃描指定檔案

結束碼：0 = 通過；1 = 有違規。
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

BASE_SCORING_PATHS = [
    "ruhi-core/src/router/constructions.rs",
    "ruhi-core/src/router/transitions.rs",
    "ruhi-core/src/router/onomastics.rs",
    "ruhi-core/src/router/geo_hierarchy.rs",
    "ruhi_lexicon/data/transitions_user.tsv",
    "ruhi_lexicon/data/patterns_user.jsonl",
]

RE_BONUS_ASSIGN = re.compile(r"\b(?:bonus|score)\s*[+-]=\s*([^\n;]+)")
RE_FLOAT_LITERAL = re.compile(r"(?<![A-Za-z0-9_.\[])([0-9]+(?:\.[0-9]+)?)(?![A-Za-z0-9_\]])")
RE_STRUCT_SCORE = re.compile(r"\b(?:bonus|score)\s*:\s*(-?[0-9]+(?:\.[0-9]+)?)")
RE_APPLY_SCORE = re.compile(r"\bapply_rule_score\w*\s*\([^,]+,\s*(-?[0-9]+(?:\.[0-9]+)?)\s*\)")
RE_SCORE_FIELD = re.compile(r"\"?(?:score|bonus)\"?\s*[:=]\s*(-?[0-9]+(?:\.[0-9]+)?)")
RE_CTX_LINE = re.compile(
    r"\b(prev_word|prev_clean|prev2_word|prev3_word|rw|next_word|nw|last_word)\b"
)
RE_LONG_CJK = re.compile(r"\"([\u3400-\u9fff\uf900-\ufaff]{3,})\"")

GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
RESET = "\033[0m"


def resolve_scoring_paths(explicit_paths: list[str]) -> list[str]:
    if explicit_paths:
        out: list[str] = []
        for raw in explicit_paths:
            p = Path(raw)
            if not p.is_absolute():
                p = (ROOT / raw).resolve()
            else:
                p = p.resolve()
            try:
                rel = p.relative_to(ROOT).as_posix()
            except ValueError:
                rel = p.as_posix()
            if rel not in out:
                out.append(rel)
        return out

    western_dir = ROOT / "ruhi-core/src/western"
    western_paths = (
        sorted(p.relative_to(ROOT).as_posix() for p in western_dir.glob("*.rs"))
        if western_dir.is_dir()
        else []
    )
    return BASE_SCORING_PATHS[:4] + western_paths + BASE_SCORING_PATHS[4:]


def strip_rust_line_comment(line: str) -> str:
    """移除字串常值之外的 `//` 單行註解。"""
    in_str = False
    escape = False
    i = 0
    n = len(line)
    while i < n:
        ch = line[i]
        if escape:
            escape = False
        elif ch == "\\" and in_str:
            escape = True
        elif ch == '"':
            in_str = not in_str
        elif not in_str and ch == "/" and i + 1 < n and line[i + 1] == "/":
            return line[:i]
        i += 1
    return line


def compute_rust_test_lines(file_path: Path) -> set[int]:
    """找出 Rust 檔案中位於 `#[cfg(test)]` 區塊內之 1-based 行號。"""
    if not file_path.is_file():
        return set()
    lines = file_path.read_text(encoding="utf-8", errors="replace").splitlines()
    test_lines: set[int] = set()
    pending_cfg_test = False
    in_test_block = False
    brace_depth = 0
    saw_open_brace = False

    for idx, line in enumerate(lines, 1):
        code = strip_rust_line_comment(line)
        stripped = code.strip()
        if not in_test_block:
            if "#[cfg(test)]" in stripped:
                pending_cfg_test = True
                test_lines.add(idx)
                continue
            if pending_cfg_test:
                test_lines.add(idx)
                opens = code.count("{")
                closes = code.count("}")
                if opens > 0:
                    in_test_block = True
                    saw_open_brace = True
                    brace_depth = opens - closes
                    if brace_depth <= 0:
                        in_test_block = False
                        pending_cfg_test = False
                        saw_open_brace = False
                elif ";" in stripped:
                    pending_cfg_test = False
        else:
            test_lines.add(idx)
            opens = code.count("{")
            closes = code.count("}")
            if opens > 0:
                saw_open_brace = True
            brace_depth += opens - closes
            if saw_open_brace and brace_depth <= 0:
                in_test_block = False
                pending_cfg_test = False
                saw_open_brace = False
    return test_lines


def git_diff(base: str | None, staged: bool, target: str | None, paths: list[str]) -> str:
    cmd = ["git", "-C", str(ROOT), "diff", "-U0"]
    if staged:
        cmd.append("--cached")
    elif base:
        cmd.append(base)
        if target:
            cmd.append(target)
    cmd += ["--", *paths]
    return subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace").stdout


def parse_added(diff: str):
    """回傳 (檔案, 新檔行號, 內容, 同區塊已刪除之詞組對集合) 之新增行清單。"""
    cur_file = None
    new_ln = 0
    removed_tsv_pairs: set[tuple[str, str]] = set()
    for line in diff.splitlines():
        if line.startswith("+++ "):
            cur_file = line[6:] if line.startswith("+++ b/") else None
            removed_tsv_pairs = set()
            continue
        if line.startswith("@@"):
            m = re.search(r"\+(\d+)", line)
            new_ln = int(m.group(1)) if m else 0
            removed_tsv_pairs = set()
            continue
        if cur_file is None:
            continue
        if line.startswith("-") and not line.startswith("---"):
            if cur_file.endswith(".tsv"):
                cols = line[1:].split("\t")
                if len(cols) >= 2:
                    removed_tsv_pairs.add((cols[0], cols[1]))
            continue
        if line.startswith("+") and not line.startswith("+++"):
            yield cur_file, new_ln, line[1:], removed_tsv_pairs if cur_file.endswith(".tsv") else None
            new_ln += 1


def iter_full_lines(paths: list[str]):
    """讀取指定路徑清單之全檔每一行，回傳 (相對路徑, 行號, 內容, None)。"""
    for rel in paths:
        p = Path(rel) if Path(rel).is_absolute() else (ROOT / rel)
        if not p.is_file():
            continue
        lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
        for ln, text in enumerate(lines, 1):
            yield rel, ln, text, None


def extract_rust_scores(code: str) -> list[float]:
    scores: list[float] = []
    m_assign = RE_BONUS_ASSIGN.search(code)
    if m_assign:
        rhs = re.sub(r"\.(?:clamp|min|max)\([^)]*\)", "", m_assign.group(1))
        scores.extend(float(x) for x in RE_FLOAT_LITERAL.findall(rhs))
    scores.extend(abs(float(x)) for x in RE_STRUCT_SCORE.findall(code))
    scores.extend(abs(float(x)) for x in RE_APPLY_SCORE.findall(code))
    return scores


def check(lines_iter, max_score: float):
    violations = []
    test_lines_cache: dict[str, set[int]] = {}

    for f, ln, text, untouched_tsv_pairs in lines_iter:
        stripped = text.strip()
        if not stripped:
            continue
        if f.endswith(".tsv"):
            if stripped.startswith("#"):
                continue
            cols = text.split("\t")
            if len(cols) >= 3:
                try:
                    v = abs(float(cols[2]))
                    if v > max_score:
                        violations.append((f, ln, f"分數 {cols[2]} 超過上限 ±{max_score:g}", text))
                except ValueError:
                    pass
            # 若 git diff 中僅修改既有轉移條目的分數而未更動前兩欄詞組，不將舊有詞組誤報為新增違規
            phrase_unchanged = (
                untouched_tsv_pairs is not None
                and len(cols) >= 2
                and (cols[0], cols[1]) in untouched_tsv_pairs
            )
            if not phrase_unchanged:
                for c in cols[:2]:
                    if len(re.findall(r"[\u3400-\u9fff]", c)) >= 4:
                        violations.append((f, ln, f"語境欄位「{c}」為 4 字以上片段", text))
            continue

        if f.endswith(".jsonl"):
            if stripped.startswith("#"):
                continue
            for x in RE_SCORE_FIELD.findall(text):
                v = abs(float(x))
                if v > max_score:
                    violations.append((f, ln, f"分數 {v:g} 超過上限 ±{max_score:g}", stripped))
            continue

        if f.endswith(".rs"):
            if f not in test_lines_cache:
                p = Path(f) if Path(f).is_absolute() else (ROOT / f)
                test_lines_cache[f] = compute_rust_test_lines(p)
            if ln in test_lines_cache[f]:
                continue
            code = strip_rust_line_comment(text)
            if not code.strip():
                continue
            for v in extract_rust_scores(code):
                if v > max_score:
                    violations.append((f, ln, f"分數 {v:g} 超過上限 ±{max_score:g}", stripped))
            if RE_CTX_LINE.search(code):
                for frag in RE_LONG_CJK.findall(code):
                    violations.append((f, ln, f"語境條件使用 3 字以上片段「{frag}」", stripped))
    return violations


def main() -> int:
    ap = argparse.ArgumentParser(description="如喜選字規則靜態檢查（分數上限與句子片段條件）")
    ap.add_argument("--base", default="HEAD", help="比較基準（預設 HEAD）")
    ap.add_argument("--target", default=None, help="比較目標 commit／分支（省略時為工作目錄）")
    ap.add_argument("--staged", action="store_true", help="只檢查暫存區")
    ap.add_argument("--full", action="store_true", help="全檔掃描（而非只看 git diff 新增行）")
    ap.add_argument("--max-score", type=float, default=35.0, help="分數絕對值上限（預設 35）")
    ap.add_argument("--limit", type=int, default=30, help="最多列出幾筆違規（預設 30）")
    ap.add_argument("paths", nargs="*", help="可選：指定受檢檔案路徑（預設檢查全體規則與歐文模組）")
    args = ap.parse_args()

    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")

    target_paths = resolve_scoring_paths(args.paths)
    if args.full:
        lines_to_check = list(iter_full_lines(target_paths))
        label = "檢查全檔行數"
    else:
        lines_to_check = list(parse_added(git_diff(args.base, args.staged, args.target, target_paths)))
        label = "檢查新增行數"

    violations = check(lines_to_check, args.max_score)
    print(f"{label}：{len(lines_to_check)}")
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
