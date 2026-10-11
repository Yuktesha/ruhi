#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 通用智慧型輸入法 - AI 目標文字校正管理工具 (CLI 一等公民)
tools/ruhi_ai_correct.py
=============================================================================
功能說明：
1. 本地/遠端 AI 目標文字校正診斷與測試。
2. 支援 Google Gemini (優先 1)、本地 Ollama Gemma (優先 2)、啟發式錯字防火牆 (優先 3)。
3. 支援工作目標文字捕獲模擬與批次檔案校對。
"""

import argparse
import json
import os
import sys
import time

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

CURR_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.dirname(CURR_DIR)
for p in [RUHI_ROOT, os.path.dirname(RUHI_ROOT), "c:/ATGprjs", "c:/ATGprjs/RuhOS"]:
    if p not in sys.path:
        sys.path.insert(0, p)

from ruhi_brain.ai_corrector import RuhiAICorrector, TextCorrectionResult
from ruhi_win11.target_corrector import TargetTextCorrector


def run_single_correction(text: str, backend: str, json_output: bool, apply_only: bool):
    corrector = RuhiAICorrector(backend=backend)
    res = corrector.correct(text)

    if apply_only:
        print(res.corrected_text)
        return

    if json_output:
        print(json.dumps(res.to_dict(), ensure_ascii=False, indent=2))
        return

    print("=" * 68)
    print(" 🚀 如喜 (RuhI) AI 工作目標文字校正診斷報告")
    print("=" * 68)
    print(f" 待測原文：{res.original_text}")
    print(f" 校正結果：{res.corrected_text}")
    print(f" 瑕疵修訂：{'✅ 發現並修復瑕疵' if res.has_corrections else '🌟 原文正確無瑕'}")
    print(f" 調度後端：{res.backend_used} (耗時: {res.latency_ms:.1f}ms)")
    if res.error_message:
        print(f" 狀態備註：{res.error_message}")
    print("-" * 68)
    print(res.format_summary())
    print("=" * 68)


def run_target_simulation(text: str, backend: str):
    print("=" * 68)
    print(" 🧪 [模擬工作目標視窗文字捕獲與校正]")
    print("=" * 68)
    print(f" 1. 模擬輸入法送出至目標視窗文字：\n    「{text}」")
    
    target_corrector = TargetTextCorrector()
    target_corrector.record_committed_text(text)

    last_sent, char_len = target_corrector.recent_tracker.get_last_sentence()
    print(f" 2. 目標視窗自動提取最近送出片段：\n    「{last_sent}」 (字元數: {char_len})")

    res = target_corrector.corrector.correct(last_sent, preferred_backend=backend)
    print(f" 3. AI 語意診斷結果 (後端: {res.backend_used})：\n    「{res.corrected_text}」")
    print(" 4. 模擬替換動作：")
    if res.has_corrections:
        print(f"    ➔ 發送 {char_len} 次 Backspace 消除舊文字")
        print(f"    ➔ 注入正確文字：「{res.corrected_text}」")
    else:
        print("    ➔ 文字無須改動，工作目標保持現狀。")
    print("-" * 68)
    print(res.format_summary())
    print("=" * 68)


def main():
    parser = argparse.ArgumentParser(
        description="如喜 (RuhI) AI 目標文字校正管理工具",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""範例用法：
  python tools/ruhi_ai_correct.py "測試句子以經完成，服務器崩潰了"
  python tools/ruhi_ai_correct.py --simulate "按步就班的做為"
  python tools/ruhi_ai_correct.py --backend heuristic "粒刻前往"
  python tools/ruhi_ai_correct.py --json "代碼緩存"
""",
    )
    parser.add_argument("text", nargs="?", help="待校正文字內容")
    parser.add_argument(
        "--simulate",
        "-s",
        type=str,
        help="以指定文字模擬工作目標視窗已 commit 送出的文字流程",
    )
    parser.add_argument(
        "--backend",
        "-b",
        choices=["auto", "gemini", "gemma", "heuristic"],
        default="auto",
        help="指定 AI 校正後端 (預設: auto 自動梯隊降級)",
    )
    parser.add_argument("--json", "-j", action="store_true", help="輸出結構化 JSON 報告")
    parser.add_argument("--apply-only", "-a", action="store_true", help="僅輸出校正後純文字")

    args = parser.parse_args()

    if args.simulate:
        run_target_simulation(args.simulate, args.backend)
    elif args.text:
        run_single_correction(args.text, args.backend, args.json, args.apply_only)
    elif not sys.stdin.isatty():
        stdin_text = sys.stdin.read().strip()
        if stdin_text:
            run_single_correction(stdin_text, args.backend, args.json, args.apply_only)
        else:
            parser.print_help()
    else:
        # 預設自檢展示
        print("💡 未提供文字參數，執行預設自檢示範案例：\n")
        demo_text = "專案的服務器代碼以經按步就班完成，請粒刻進行測試！"
        run_single_correction(demo_text, args.backend, args.json, args.apply_only)


if __name__ == "__main__":
    main()
