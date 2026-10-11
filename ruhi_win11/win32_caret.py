"""
Ruhi (如喜) Input Method Framework - Windows 11 核心適配層
ruhi_win11.win32_caret - 原生 Win32 游標與 Caret 座標定位器
"""

import ctypes
from ctypes import wintypes
from typing import Tuple

class GUITHREADINFO(ctypes.Structure):
    _fields_ = [
        ("cbSize", wintypes.DWORD),
        ("flags", wintypes.DWORD),
        ("hwndActive", wintypes.HWND),
        ("hwndFocus", wintypes.HWND),
        ("hwndCapture", wintypes.HWND),
        ("hwndMenuOwner", wintypes.HWND),
        ("hwndMoveSize", wintypes.HWND),
        ("hwndCaret", wintypes.HWND),
        ("rcCaret", wintypes.RECT)
    ]

class POINT(ctypes.Structure):
    _fields_ = [("x", wintypes.LONG), ("y", wintypes.LONG)]

class Win32CaretTracker:
    """即時追蹤當前焦點視窗與輸入游標 (Caret) 螢幕坐標"""

    @staticmethod
    def get_caret_or_cursor_pos() -> Tuple[int, int]:
        """
        優先嘗試取得文字 Caret 螢幕坐標；若應用程式未回報 Caret，則退回滑鼠游標坐標
        返回: (x, y) 螢幕像素坐標
        """
        user32 = ctypes.windll.user32
        gui_info = GUITHREADINFO()
        gui_info.cbSize = ctypes.sizeof(GUITHREADINFO)

        # 獲取前景視窗執行緒的 Caret 資訊
        if user32.GetGUIThreadInfo(0, ctypes.byref(gui_info)):
            if gui_info.hwndCaret and gui_info.rcCaret.left != 0:
                pt = POINT(gui_info.rcCaret.left, gui_info.rcCaret.bottom)
                user32.ClientToScreen(gui_info.hwndCaret, ctypes.byref(pt))
                return pt.x, pt.y + 4

        # 退回滑鼠或螢幕右下角安全位置
        pt = POINT()
        user32.GetCursorPos(ctypes.byref(pt))
        return pt.x + 10, pt.y + 20
