#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 通用智慧型輸入法 - 工作目標文字 AI 校正控制器
ruhi_win11.target_corrector — 捕獲與替換已送至工作目標（應用程式）文字的完整機制
=============================================================================
核心特色與架構：
1. 雙向捕獲工作目標文字：
   - 模式 A (反白選取優先)：若使用者在目標視窗中反白了一段文字，自動複製並校正該文字。
   - 模式 B (最近送出追蹤)：若未選取文字，自動回溯輸入法最近送至目標視窗的字串/句子。
2. 無縫安全替換：
   - 反白文字：以校正後文字注入替換，且嚴格還原使用者先前的剪貼簿（避免污染剪貼簿）。
   - 最近送出文字：以精準計算的 Backspace 消除舊字串，隨即注入 AI 校正後的正確文字。
3. 原生 Win32 零依賴實作：
   - 剪貼簿操作採用 Windows 原生 ctypes (user32/kernel32)，不依賴外部三方庫。
"""

import ctypes
import os
import sys
import threading
import time
from collections import deque
from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional, Tuple

# 加入專案路徑
CURRENT_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.dirname(CURRENT_DIR)
for p in [RUHI_ROOT, os.path.dirname(RUHI_ROOT), "c:/ATGprjs", "c:/ATGprjs/RuhOS"]:
    if p not in sys.path:
        sys.path.insert(0, p)

try:
    from ruhi_brain.ai_corrector import RuhiAICorrector, TextCorrectionResult, TextChange
    from ruhi_win11.win32_input_sender import Win32InputSender
except ImportError:
    from ruhi.ruhi_brain.ai_corrector import RuhiAICorrector, TextCorrectionResult, TextChange
    from ruhi.ruhi_win11.win32_input_sender import Win32InputSender


# =============================================================================
# 1. 原生 Win32 剪貼簿安全控制器 (Win32Clipboard)
# =============================================================================

user32 = ctypes.windll.user32
kernel32 = ctypes.windll.kernel32

CF_UNICODETEXT = 13
GMEM_MOVEABLE = 0x0002
GMEM_ZEROINIT = 0x0040

user32.OpenClipboard.argtypes = [ctypes.c_void_p]
user32.OpenClipboard.restype = ctypes.c_bool
user32.CloseClipboard.restype = ctypes.c_bool
user32.EmptyClipboard.restype = ctypes.c_bool
user32.GetClipboardData.argtypes = [ctypes.c_uint]
user32.GetClipboardData.restype = ctypes.c_void_p
user32.SetClipboardData.argtypes = [ctypes.c_uint, ctypes.c_void_p]
user32.SetClipboardData.restype = ctypes.c_void_p

kernel32.GlobalAlloc.argtypes = [ctypes.c_uint, ctypes.c_size_t]
kernel32.GlobalAlloc.restype = ctypes.c_void_p
kernel32.GlobalLock.argtypes = [ctypes.c_void_p]
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalUnlock.argtypes = [ctypes.c_void_p]
kernel32.GlobalUnlock.restype = ctypes.c_bool


class Win32Clipboard:
    """Windows 原生剪貼簿安全存取器"""

    @staticmethod
    def get_text(retries: int = 5, delay: float = 0.02) -> Optional[str]:
        """從系統剪貼簿讀取 Unicode 字串"""
        for _ in range(retries):
            if user32.OpenClipboard(None):
                try:
                    h_clip = user32.GetClipboardData(CF_UNICODETEXT)
                    if not h_clip:
                        return None
                    ptr = kernel32.GlobalLock(h_clip)
                    if not ptr:
                        return None
                    try:
                        text = ctypes.wstring_at(ptr)
                        return text
                    finally:
                        kernel32.GlobalUnlock(h_clip)
                finally:
                    user32.CloseClipboard()
            time.sleep(delay)
        return None

    @staticmethod
    def set_text(text: str, retries: int = 5, delay: float = 0.02) -> bool:
        """將 Unicode 字串寫入系統剪貼簿"""
        encoded = text.encode("utf-16-le") + b"\x00\x00"
        h_mem = kernel32.GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, len(encoded))
        if not h_mem:
            return False

        ptr = kernel32.GlobalLock(h_mem)
        if not ptr:
            return False
        ctypes.memmove(ptr, encoded, len(encoded))
        kernel32.GlobalUnlock(h_mem)

        for _ in range(retries):
            if user32.OpenClipboard(None):
                try:
                    user32.EmptyClipboard()
                    res = user32.SetClipboardData(CF_UNICODETEXT, h_mem)
                    return bool(res)
                finally:
                    user32.CloseClipboard()
            time.sleep(delay)
        return False


# =============================================================================
# 2. 最近送出文字追蹤器 (RecentCommittedTracker)
# =============================================================================

class RecentCommittedTracker:
    """
    輸入法最近已 commit 送至目標視窗的字串追蹤隊列。
    能自動還原最後輸入的一句話、段落或字詞。
    """

    def __init__(self, max_chars: int = 300):
        self.max_chars = max_chars
        self._lock = threading.Lock()
        self._buffer: str = ""

    def append(self, text: str):
        """當文字送入目標視窗時調用記錄"""
        if not text:
            return
        with self._lock:
            self._buffer += text
            if len(self._buffer) > self.max_chars:
                self._buffer = self._buffer[-self.max_chars:]

    def get_last_sentence(self) -> Tuple[str, int]:
        """
        獲取最後一個語意分句及其字元長度。
        依句號、問號、驚嘆號、分號或換行截取最後一段。
        """
        with self._lock:
            buf = self._buffer.strip()
            if not buf:
                return "", 0

            # 逆向尋找最後一個句末標點或換行
            delimiters = "。\n！？!?；;\r"
            last_idx = -1
            for i in range(len(buf) - 2, -1, -1):
                if buf[i] in delimiters:
                    last_idx = i
                    break

            if last_idx != -1:
                target_str = buf[last_idx + 1:].strip()
            else:
                # 若未有句末標點，若長度超過 30 字，取後 30 字，否則取全部
                target_str = buf[-35:] if len(buf) > 35 else buf

            return target_str, len(target_str)

    def get_all(self) -> str:
        with self._lock:
            return self._buffer

    def clear(self):
        with self._lock:
            self._buffer = ""


# =============================================================================
# 3. 工作目標文字 AI 校正控制器 (TargetTextCorrector)
# =============================================================================

@dataclass
class TargetCorrectionAction:
    """校正動作報告"""
    source_type: str  # "selection" | "recent_commit" | "manual"
    original_text: str
    corrected_text: str
    applied: bool
    summary: str
    result: TextCorrectionResult


class TargetTextCorrector:
    """
    工作目標文字校正核心協調器：
    1. 抓取目標視窗中反白文字或輸入法最近送出的文字
    2. 調用 RuhiAICorrector 分析
    3. 自動替換回目標應用程式焦點視窗
    """

    def __init__(self, corrector: Optional[RuhiAICorrector] = None):
        self.corrector = corrector or RuhiAICorrector(backend="auto")
        self.recent_tracker = RecentCommittedTracker()
        self._action_lock = threading.Lock()

    def record_committed_text(self, text: str):
        """向最近文字追蹤器登記新送出之字串"""
        self.recent_tracker.append(text)

    def capture_active_target_text(self) -> Tuple[Optional[str], str, int]:
        """
        嘗試從當前活動的工作目標視窗捕獲待校正文字。
        回傳: (文字內容, 來源類型: "selection" | "recent_commit" | "none", 字元數)
        """
        # 1. 優先嘗試捕獲反白選取文字 (Selection)
        # 備份原有剪貼簿
        old_clipboard = Win32Clipboard.get_text()

        # 發送 Ctrl+C 觸發複製
        Win32InputSender.send_ctrl_c()
        time.sleep(0.06)  # 給予宿主程式微小反應時間

        new_clipboard = Win32Clipboard.get_text()
        
        # 若剪貼簿內容有所更新且不為空，說明使用者目前有反白文字
        if new_clipboard and new_clipboard != old_clipboard:
            # 捕獲成功！原剪貼簿暫存供後續替換完畢後還原
            return new_clipboard.strip(), "selection", len(new_clipboard.strip())

        # 2. 若無反白文字，回退至「最近送出歷史緩衝區 (Recent Commit)」
        recent_sentence, length = self.recent_tracker.get_last_sentence()
        if recent_sentence:
            return recent_sentence, "recent_commit", length

        return None, "none", 0

    def apply_replacement(self, source_type: str, char_count: int, corrected_text: str):
        """
        將校正後的文字精準替換回工作目標視窗
        """
        if not corrected_text:
            return

        if source_type == "selection":
            # 選取文字替換：寫入剪貼簿 ➔ Ctrl+V ➔ 還原
            backup_clip = Win32Clipboard.get_text()
            Win32Clipboard.set_text(corrected_text)
            time.sleep(0.03)
            Win32InputSender.send_ctrl_v()
            time.sleep(0.08)
            # 還原舊剪貼簿
            if backup_clip is not None:
                Win32Clipboard.set_text(backup_clip)

        elif source_type == "recent_commit":
            # 最近送出文字替換：發送 Backspace 消除舊字 ➔ 注入新字
            Win32InputSender.send_backspace(char_count)
            time.sleep(0.04)
            Win32InputSender.send_unicode_string(corrected_text)

    def correct_target_active(
        self,
        auto_apply: bool = True,
        preferred_backend: Optional[str] = None,
        callback: Optional[Callable[[TargetCorrectionAction], None]] = None,
    ) -> Optional[TargetCorrectionAction]:
        """
        執行目標視窗文字校正流程：
        捕獲文字 ➔ AI 校正 ➔ (自動/非自動) 替換回工作目標 ➔ 回傳動作報告
        """
        with self._action_lock:
            text, src_type, char_len = self.capture_active_target_text()
            if not text:
                return None

            # 呼叫 AI 校正
            res = self.corrector.correct(text, preferred_backend=preferred_backend)

            applied = False
            if res.has_corrections and auto_apply:
                self.apply_replacement(src_type, char_len, res.corrected_text)
                applied = True

            action = TargetCorrectionAction(
                source_type=src_type,
                original_text=text,
                corrected_text=res.corrected_text,
                applied=applied,
                summary=res.format_summary(),
                result=res,
            )

            if callback:
                callback(action)

            return action

    def correct_target_async(
        self,
        auto_apply: bool = True,
        preferred_backend: Optional[str] = None,
        callback: Optional[Callable[[TargetCorrectionAction], None]] = None,
    ):
        """非同步執行目標文字校正，絕不阻塞呼叫者執行緒"""
        threading.Thread(
            target=self.correct_target_active,
            args=(auto_apply, preferred_backend, callback),
            daemon=True,
        ).start()


# =============================================================================
# 4. CLI 測試與展示工具
# =============================================================================

def main():
    import argparse

    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")

    parser = argparse.ArgumentParser(
        description="如喜 (RuhI) 工作目標文字 AI 校正控制器 CLI",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--simulate",
        "-s",
        type=str,
        help="模擬已送至工作目標之文字（不進行 Win32 按鍵注入）",
    )
    parser.add_argument(
        "--backend",
        "-b",
        choices=["auto", "gemini", "gemma", "heuristic"],
        default="auto",
        help="指定 AI 校正後端",
    )

    args = parser.parse_args()

    corrector = TargetTextCorrector()

    if args.simulate:
        print("=" * 65)
        print(" 🧪 [模擬工作目標文字校正測試]")
        print("=" * 65)
        print(f" 模擬工作目標已送出文字：{args.simulate}")
        corrector.record_committed_text(args.simulate)
        
        # 模擬呼叫校正
        last_sentence, char_len = corrector.recent_tracker.get_last_sentence()
        res = corrector.corrector.correct(last_sentence, preferred_backend=args.backend)
        
        print(f" 捕獲最近送出片段：{last_sentence} (字元數: {char_len})")
        print(f" AI 校正後文字：{res.corrected_text}")
        print(f" 是否產生修正：{'是' if res.has_corrections else '否'}")
        print("-" * 65)
        print(res.format_summary())
        print("=" * 65)
    else:
        print("=" * 65)
        print(" 🎯 請在 3 秒內切換到任何目標視窗並反白一段文字...")
        print("=" * 65)
        for i in range(3, 0, -1):
            print(f" 倒數 {i} 秒...")
            time.sleep(1.0)
        
        print(" 🚀 正在捕獲並校正文字...")
        action = corrector.correct_target_active(auto_apply=True, preferred_backend=args.backend)
        if action:
            print(f" 來源模式：{action.source_type}")
            print(f" 原文：{action.original_text}")
            print(f" 校正文：{action.corrected_text}")
            print(f" 是否已替換回工作目標：{action.applied}")
            print(action.summary)
        else:
            print(" ⚠️ 未捕獲到任何反白文字或最近送出文字。")


if __name__ == "__main__":
    main()
