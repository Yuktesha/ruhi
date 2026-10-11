"""
Ruhi (如喜) Input Method Framework - Windows 11 核心適配層
ruhi_win11.ruhi_win_overlay - R-UI/UX 自適應無框浮動候選字視窗 (游標跟隨)
"""

import tkinter as tk
from tkinter import ttk
from typing import Dict, List, Optional
from .win32_caret import Win32CaretTracker

class RuhiWinOverlay:
    """Windows 11 游標跟隨無框浮動候選視窗"""
    def __init__(self, root: Optional[tk.Tk] = None):
        self.root = root or tk.Tk()
        self.root.withdraw() # 初始隱藏
        
        # 設定為無邊框且置頂
        self.root.overrideredirect(True)
        self.root.attributes("-topmost", True)
        self.root.config(bg="#1A202C") # 深色主題底色

        self.setup_ui()

    def setup_ui(self):
        self.container = tk.Frame(self.root, bg="#1A202C", highlightthickness=1, highlightbackground="#4A5568")
        self.container.pack(fill="both", expand=True, padx=2, pady=2)

        # 頂部：模式與狀態標籤
        self.lbl_mode = tk.Label(
            self.container, 
            text="[Ruhi 許氏雙拼]", 
            font=("Segoe UI", 9, "bold"), 
            bg="#1A202C", 
            fg="#48BB78"
        )
        self.lbl_mode.pack(anchor="w", padx=6, pady=(4, 0))

        # 中層：注音暫存與即時推導
        self.preedit_frame = tk.Frame(self.container, bg="#1A202C")
        self.preedit_frame.pack(fill="x", padx=6, pady=2)

        self.lbl_preedit = tk.Label(
            self.preedit_frame, 
            text="ㄓㄨㄥ", 
            font=("Microsoft JhengHei", 12), 
            bg="#1A202C", 
            fg="#63B3ED"
        )
        self.lbl_preedit.pack(side="left")

        self.lbl_composed = tk.Label(
            self.preedit_frame, 
            text="【中】", 
            font=("Microsoft JhengHei", 13, "bold"), 
            bg="#1A202C", 
            fg="#F6E05E"
        )
        self.lbl_composed.pack(side="left", padx=6)

        # 底層：1..9 候選字清單條
        self.cand_frame = tk.Frame(self.container, bg="#2D3748")
        self.cand_frame.pack(fill="x", padx=4, pady=(2, 4))

        self.lbl_candidates = tk.Label(
            self.cand_frame,
            text="1.中  2.種  3.重",
            font=("Microsoft JhengHei", 11),
            bg="#2D3748",
            fg="#EDF2F7",
            padx=6,
            pady=3
        )
        self.lbl_candidates.pack(fill="x")

    def update_view(self, state: Dict):
        """根據 RuhiEngine 狀態即時刷新視窗位置與內容"""
        preedit = state.get("preedit", "")
        composed = state.get("composed", "")
        candidates = state.get("candidates", [])
        mode = state.get("mode", "normal")
        q_count = state.get("q_press_count", 0)

        # 若無任何進行中的文字與候選字，隱藏浮動窗
        if not preedit and not composed and not candidates:
            self.root.withdraw()
            return

        # 模式標籤
        mode_str = {
            "normal": f"[Ruhi 許氏] Q:{q_count}/5",
            "templates": "[文字範本模式 `]",
            "quick_num": "[數字快打模式 ``]",
            "lexicon": "[專業詞庫切換 ~]",
            "candidates": "[完整選字窗 Q]",
        }.get(mode, f"[Ruhi | {mode}]")
        self.lbl_mode.config(text=mode_str)

        # 注音與推導
        self.lbl_preedit.config(text=preedit)
        self.lbl_composed.config(text=f"【{composed}】" if composed else "")

        # 候選字清單
        if candidates:
            cand_items = []
            for i, c in enumerate(candidates[:6]):
                cand_items.append(f"{i+1}.{c}")
            self.lbl_candidates.config(text="  ".join(cand_items))
            self.cand_frame.pack(fill="x", padx=4, pady=(2, 4))
        else:
            self.cand_frame.pack_forget()

        # 獲取打字游標坐標並更新視窗位置
        x, y = Win32CaretTracker.get_caret_or_cursor_pos()
        self.root.geometry(f"+{x}+{y}")
        self.root.deiconify()
        self.root.lift()

    def show_correction_toast(self, title: str, text: str, duration_sec: float = 3.5):
        """彈出精美的 AI 目標文字校正通知膠囊"""
        self.lbl_mode.config(text=f"[✨ {title}]", fg="#68D391")
        self.lbl_preedit.config(text="")
        self.lbl_composed.config(text=text, fg="#63B3ED", font=("Microsoft JhengHei", 11, "bold"))
        self.cand_frame.pack_forget()

        x, y = Win32CaretTracker.get_caret_or_cursor_pos()
        self.root.geometry(f"+{x}+{y}")
        self.root.deiconify()
        self.root.lift()

        # 設定定時器隱藏
        def _hide():
            self.root.withdraw()
            # 恢復原本樣式
            self.lbl_mode.config(fg="#48BB78")
            self.lbl_composed.config(fg="#F6E05E", font=("Microsoft JhengHei", 13, "bold"))

        self.root.after(int(duration_sec * 1000), _hide)
