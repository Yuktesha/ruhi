#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 通用智慧型輸入法 - 全域 AI 目標文字校正守護核心工具
tools/ruhi_corrector_daemon.py
=============================================================================
功能：
1. 註冊 Win32 / pynput 全域熱鍵【Ctrl + Alt + C】。
2. 當使用者在任何應用程式中反白選取一段文字並按下熱鍵時：
   - 自動捕獲選取文字
   - 呼叫本地/遠端 AI 校正引擎（Gemini ➔ Gemma ➔ 啟發式防火牆）
   - 精準將修正後的文字替換回目標視窗
   - 完整還原使用者原有的系統剪貼簿
"""

import os
import sys
import time
import threading
from pynput import keyboard

# 設定路徑
CURRENT_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.dirname(CURRENT_DIR)
for p in [RUHI_ROOT, os.path.dirname(RUHI_ROOT), "c:/ATGprjs", "c:/ATGprjs/RuhOS"]:
    if p not in sys.path:
        sys.path.insert(0, p)

from ruhi_brain.ai_corrector import RuhiAICorrector
from ruhi_win11.target_corrector import TargetTextCorrector

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def on_hotkey_triggered(corrector: TargetTextCorrector):
    print("\n⚡ [全域熱鍵 Ctrl+Alt+C 觸發] 正在捕獲並校正工作目標文字...")
    try:
        action = corrector.correct_target_active(auto_apply=True)
        if action:
            if action.applied:
                print(f"  ✅ [校正成功替換] 原文：「{action.original_text}」 ➔ 修正文：「{action.corrected_text}」")
                print(f"     調度後端: {action.result.backend_used} (耗時: {action.result.latency_ms:.1f}ms)")
            else:
                print(f"  🌟 [原文正確無瑕] 「{action.original_text}」無須修訂。")
        else:
            print("  ⚠️ [未捕獲文字] 請先反白選取欲校正之文字，再按 Ctrl+Alt+C。")
    except Exception as e:
        print(f"  ❌ 校正過程發生異常: {e}")


def main():
    print("=" * 68)
    print(" 🚀 如喜 (RuhI) 全域 AI 目標文字校正守護核心工具已啟動！")
    print("=" * 68)
    print("  • 監聽熱鍵：【Ctrl + Alt + C】")
    print("  • 使用方法：在任何應用程式（記事本、Word、LINE、瀏覽器等）中，")
    print("              只要「反白選取」一段文字，按下【Ctrl + Alt + C】，")
    print("              AI 即會自動分析瑕疵並直接替換回目標文字，且不污染剪貼簿！")
    print("  • 支援後端：Google Gemini API ➔ 本機 Ollama Gemma ➔ 啟發式錯字防火牆")
    print("  • 結束請按：Ctrl + C 關閉此主控台視窗")
    print("=" * 68 + "\n")

    corrector = TargetTextCorrector()

    # 註冊 GlobalHotKeys
    hotkey = keyboard.GlobalHotKeys({
        '<ctrl>+<alt>+c': lambda: threading.Thread(target=on_hotkey_triggered, args=(corrector,), daemon=True).start(),
        '<ctrl>+<alt>+C': lambda: threading.Thread(target=on_hotkey_triggered, args=(corrector,), daemon=True).start(),
    })

    hotkey.start()
    try:
        while True:
            time.sleep(1.0)
    except KeyboardInterrupt:
        print("\n👋 全域 AI 校正守護工具已安全停止。")
    finally:
        hotkey.stop()


if __name__ == "__main__":
    main()
