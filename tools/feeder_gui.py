#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) ✦ 人工餵料器 (Corpus Feeder GUI)
=============================================================================
設計理念：
1. 極簡 m3u8 純文字清單哲學：一行一個路徑，可手動註解與自由編輯
2. 全原生 Windows 檔案/資料夾/捷徑 (.lnk) 拖放支援 (tkinterdnd2)
3. 整合 RuhMark (如標) 深度清洗 ➔ 產生 corpus_cleaned/*.cleaned.txt 副本
4. 整合本地 AI 錯字防火牆 ➔ 提煉大十文風高頻 Bigram 轉移熱寫入 transitions_user.tsv
5. 嚴格貫徹 ATG App Standards & _lib 寶藏庫：
   - Ctrl + + / - / 0 介面無級縮放 (Dynamic UI Scaling)
   - 🌙 深色主題 / ☀️ 淺色宣紙主題 一鍵切換
   - Windows 10/11 DWM 沉浸式標題列色彩渲染 (apply_title_bar_theme)
"""

import os
import sys
import threading
import time
import subprocess
import tkinter as tk
from tkinter import ttk, filedialog, messagebox
from tkinterdnd2 import DND_FILES, TkinterDnD

# Windows 高 DPI 感知與 UIPI 拖放過濾器解鎖
try:
    import ctypes
    ctypes.windll.shcore.SetProcessDpiAwareness(1)
    # 解鎖 UIPI 跨權限拖放阻擋 (WM_DROPFILES, WM_COPYDATA, WM_COPYGLOBALDATA)
    MSGFLT_ADD = 1
    for msg_id in (0x0233, 0x004A, 0x0049):
        try:
            ctypes.windll.user32.ChangeWindowMessageFilter(msg_id, MSGFLT_ADD)
        except Exception:
            pass
except Exception:
    pass

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_DIR = os.path.abspath(os.path.join(BASE_DIR, ".."))
CORPUS_FEEDER_DIR = os.path.join(RUHI_DIR, "corpus_feeder")
CORPUS_CLEANED_DIR = os.path.join(RUHI_DIR, "corpus_cleaned")
DEFAULT_PLAYLIST_PATH = os.path.join(CORPUS_FEEDER_DIR, "feeder_playlist.ruhi_list")

# 引入共用 _lib 寶庫
LIB_DIR = os.path.abspath("C:/ATGprjs/_lib")
if LIB_DIR not in sys.path:
    sys.path.insert(0, LIB_DIR)

try:
    from theme_engine import apply_title_bar_theme, get_system_theme
except Exception:
    def apply_title_bar_theme(w, dark=True): pass
    def get_system_theme(): return "dark"

sys.path.insert(0, os.path.join(RUHI_DIR, "tools"))
from ruhi_doc_learner import (
    resolve_windows_shortcut,
    clean_corpus_with_ruhmark,
    RuhiPersonalLearner,
    RUHMARK_AVAILABLE
)

# 典雅主題雙調色盤 (UniversalUI 規格)
PALETTES = {
    "light": {
        "is_dark": False,
        "bg_main": "#F7F5F0",
        "bg_card": "#FFFFFF",
        "ink_primary": "#1C1D21",
        "ink_secondary": "#4A4D57",
        "ink_muted": "#8E929D",
        "accent_cinnabar": "#C23A3A",
        "accent_teal": "#1F6F78",
        "accent_gold": "#B8860B",
        "border": "#DDD7CD",
        "dropzone_bg": "#FAF8F5",
        "dropzone_fg": "#B8860B",
        "dropzone_border": "#B8860B",
        "list_bg": "#FFFFFF",
        "list_fg": "#1C1D21",
        "list_select": "#EAE5DC"
    },
    "dark": {
        "is_dark": True,
        "bg_main": "#1E1F22",
        "bg_card": "#2B2D31",
        "ink_primary": "#E3E5E8",
        "ink_secondary": "#B5BAC1",
        "ink_muted": "#80848E",
        "accent_cinnabar": "#E05252",
        "accent_teal": "#2E959E",
        "accent_gold": "#E5B54F",
        "border": "#3B3E45",
        "dropzone_bg": "#25272B",
        "dropzone_fg": "#E5B54F",
        "dropzone_border": "#E5B54F",
        "list_bg": "#232428",
        "list_fg": "#DBDEE1",
        "list_select": "#3A3C42"
    }
}

class CorpusFeederApp:
    def __init__(self, root):
        self.root = root
        self.root.title("如喜 ✦ 人工餵料器 (RuhI Corpus Feeder)")
        self.root.geometry("860x700")
        self.root.minsize(740, 580)

        # 預設主題：自動跟隨系統或預設深色
        sys_theme = get_system_theme()
        self.current_theme_key = sys_theme if sys_theme in PALETTES else "dark"
        self.scale_factor = 1.0 # 縮放倍率 (預設 1.0)

        self.playlist_path = DEFAULT_PLAYLIST_PATH
        self.playlist_items = []
        self.is_running = False

        self.setup_ui()
        self.bind_shortcuts()
        self.apply_theme()
        self.load_playlist_file(self.playlist_path)

    def setup_ui(self):
        # 1. 頂部標題列與主題/縮放開關
        self.top_frame = tk.Frame(self.root)
        self.top_frame.pack(fill=tk.X, pady=12, padx=20)

        self.title_lbl = tk.Label(
            self.top_frame,
            text="如喜 ✦ 人工餵料器",
            font=("Microsoft JhengHei UI", int(18 * self.scale_factor), "bold")
        )
        self.title_lbl.pack(side=tk.LEFT)

        self.subtitle_lbl = tk.Label(
            self.top_frame,
            text="m3u 播放清單哲學 ✦ 整合 RuhMark 深度清洗與錯字除魅",
            font=("Microsoft JhengHei UI", int(10 * self.scale_factor))
        )
        self.subtitle_lbl.pack(side=tk.LEFT, padx=14, pady=(6, 0))

        # 主題切換鈕與縮放指示
        self.top_ctrl_frame = tk.Frame(self.top_frame)
        self.top_ctrl_frame.pack(side=tk.RIGHT)

        self.lbl_zoom = tk.Label(
            self.top_ctrl_frame,
            text="100%",
            font=("Consolas", int(9 * self.scale_factor))
        )
        self.lbl_zoom.pack(side=tk.LEFT, padx=6)

        self.btn_theme_toggle = tk.Button(
            self.top_ctrl_frame,
            text="🌙 深色" if self.current_theme_key == "light" else "☀️ 淺色",
            font=("Microsoft JhengHei UI", int(9 * self.scale_factor)),
            relief=tk.GROOVE,
            command=self.toggle_theme
        )
        self.btn_theme_toggle.pack(side=tk.LEFT, padx=4)

        # 2. 核心拖曳放置區 (Drop Zone)
        self.drop_frame = tk.Frame(self.root)
        self.drop_frame.pack(fill=tk.X, padx=20, pady=6)

        self.drop_box = tk.Label(
            self.drop_frame,
            text="📂 將文章檔案、寫作資料夾或捷徑 (.lnk) 拖曳至此放開\n（或點擊此處瀏覽加入語料）",
            font=("Microsoft JhengHei UI", int(12 * self.scale_factor)),
            relief=tk.RIDGE,
            bd=2,
            pady=int(24 * self.scale_factor),
            cursor="hand2"
        )
        self.drop_box.pack(fill=tk.X)

        self.drop_box.drop_target_register(DND_FILES)
        self.drop_box.dnd_bind('<<Drop>>', self.on_files_dropped)
        self.drop_box.bind("<Button-1>", lambda e: self.browse_and_add())

        # 3. 語料播放清單 (Playlist) 工具列
        self.list_tool_frame = tk.Frame(self.root)
        self.list_tool_frame.pack(fill=tk.X, padx=20, pady=(10, 4))

        self.list_title = tk.Label(
            self.list_tool_frame,
            text="📜 語料來源清單 (m3u8 格式)：",
            font=("Microsoft JhengHei UI", int(11 * self.scale_factor), "bold")
        )
        self.list_title.pack(side=tk.LEFT)

        # 紅色精粹過濾開關
        self.var_red_only = tk.BooleanVar(value=True)
        self.chk_red_only = tk.Checkbutton(
            self.list_tool_frame,
            text="🔴 僅提煉親筆標註之紅色精粹 (過濾 Google 機翻)",
            variable=self.var_red_only,
            font=("Microsoft JhengHei UI", int(9 * self.scale_factor)),
            relief=tk.FLAT
        )
        self.chk_red_only.pack(side=tk.LEFT, padx=12)

        self.btn_open_notepad = tk.Button(
            self.list_tool_frame,
            text="📝 記事本手動編輯",
            font=("Microsoft JhengHei UI", int(9 * self.scale_factor)),
            relief=tk.GROOVE,
            command=self.open_playlist_in_notepad
        )
        self.btn_open_notepad.pack(side=tk.RIGHT, padx=4)

        self.btn_reload = tk.Button(
            self.list_tool_frame,
            text="🔄 重新整理",
            font=("Microsoft JhengHei UI", int(9 * self.scale_factor)),
            relief=tk.GROOVE,
            command=lambda: self.load_playlist_file(self.playlist_path)
        )
        self.btn_reload.pack(side=tk.RIGHT, padx=4)

        self.btn_save = tk.Button(
            self.list_tool_frame,
            text="💾 另存清單",
            font=("Microsoft JhengHei UI", int(9 * self.scale_factor)),
            relief=tk.GROOVE,
            command=self.save_playlist_as
        )
        self.btn_save.pack(side=tk.RIGHT, padx=4)

        self.btn_clear = tk.Button(
            self.list_tool_frame,
            text="🗑️ 清空清單",
            font=("Microsoft JhengHei UI", int(9 * self.scale_factor)),
            relief=tk.GROOVE,
            command=self.clear_playlist
        )
        self.btn_clear.pack(side=tk.RIGHT, padx=4)

        # 4. 播放清單 Listbox 與捲軸
        self.list_container = tk.Frame(self.root)
        self.list_container.pack(fill=tk.BOTH, expand=True, padx=20)

        self.scrollbar = tk.Scrollbar(self.list_container)
        self.scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        self.listbox = tk.Listbox(
            self.list_container,
            font=("Consolas", int(10 * self.scale_factor)),
            bd=1,
            relief=tk.SOLID,
            yscrollcommand=self.scrollbar.set
        )
        self.listbox.pack(fill=tk.BOTH, expand=True)
        self.scrollbar.config(command=self.listbox.yview)

        self.listbox.bind("<Delete>", lambda e: self.remove_selected_item())

        # 全域與各核心區域全面註冊拖放支援
        for target_widget in (self.drop_box, self.listbox, self.root):
            try:
                target_widget.drop_target_register(DND_FILES)
                target_widget.dnd_bind('<<Drop>>', self.on_files_dropped)
                target_widget.dnd_bind('<<DropEnter>>', lambda e: self.on_drop_enter())
                target_widget.dnd_bind('<<DropLeave>>', lambda e: self.on_drop_leave())
            except Exception as e:
                print("DND 註冊提示:", e)

        # 5. 進度與狀態卡片
        self.status_frame = tk.Frame(self.root)
        self.status_frame.pack(fill=tk.X, padx=20, pady=8)

        self.status_lbl = tk.Label(
            self.status_frame,
            text="就緒。請拖放語料至上方，或點擊開始執行餵料。",
            font=("Microsoft JhengHei UI", int(10 * self.scale_factor)),
            anchor="w"
        )
        self.status_lbl.pack(fill=tk.X, pady=(0, 4))

        self.progress_bar = ttk.Progressbar(self.status_frame, mode='determinate')
        self.progress_bar.pack(fill=tk.X)

        # 6. 底部大按鈕 (Call To Action)
        self.bottom_frame = tk.Frame(self.root)
        self.bottom_frame.pack(fill=tk.X, padx=20, pady=14)

        self.btn_start = tk.Button(
            self.bottom_frame,
            text="⚡ 開始深度餵料學習 (RuhMark 清洗 ➔ 錯字除魅 ➔ 熱寫入)",
            font=("Microsoft JhengHei UI", int(13 * self.scale_factor), "bold"),
            relief=tk.FLAT,
            pady=int(10 * self.scale_factor),
            cursor="hand2",
            command=self.start_feeding_thread
        )
        self.btn_start.pack(fill=tk.X)

    # --------------------------------------------------------------------------
    # 快捷鍵綁定 (Ctrl + +/0/- 縮放)
    # --------------------------------------------------------------------------
    def bind_shortcuts(self):
        # 放大
        self.root.bind("<Control-plus>", lambda e: self.zoom_in())
        self.root.bind("<Control-equal>", lambda e: self.zoom_in())
        self.root.bind("<Control-KP_Add>", lambda e: self.zoom_in())
        # 縮小
        self.root.bind("<Control-minus>", lambda e: self.zoom_out())
        self.root.bind("<Control-KP_Subtract>", lambda e: self.zoom_out())
        # 重置 100%
        self.root.bind("<Control-0>", lambda e: self.zoom_reset())
        self.root.bind("<Control-KP_0>", lambda e: self.zoom_reset())

    def zoom_in(self):
        if self.scale_factor < 2.0:
            self.scale_factor = round(self.scale_factor + 0.1, 1)
            self.apply_scaling()

    def zoom_out(self):
        if self.scale_factor > 0.8:
            self.scale_factor = round(self.scale_factor - 0.1, 1)
            self.apply_scaling()

    def zoom_reset(self):
        self.scale_factor = 1.0
        self.apply_scaling()

    def apply_scaling(self):
        s = self.scale_factor
        self.lbl_zoom.config(text=f"{int(s * 100)}%")

        self.title_lbl.config(font=("Microsoft JhengHei UI", int(18 * s), "bold"))
        self.subtitle_lbl.config(font=("Microsoft JhengHei UI", int(10 * s)))
        self.btn_theme_toggle.config(font=("Microsoft JhengHei UI", int(9 * s)))
        self.lbl_zoom.config(font=("Consolas", int(9 * s)))

        self.drop_box.config(font=("Microsoft JhengHei UI", int(12 * s)), pady=int(24 * s))
        self.list_title.config(font=("Microsoft JhengHei UI", int(11 * s), "bold"))
        self.chk_red_only.config(font=("Microsoft JhengHei UI", int(9 * s)))
        self.btn_open_notepad.config(font=("Microsoft JhengHei UI", int(9 * s)))
        self.btn_save.config(font=("Microsoft JhengHei UI", int(9 * s)))
        self.btn_clear.config(font=("Microsoft JhengHei UI", int(9 * s)))

        self.listbox.config(font=("Consolas", int(10 * s)))
        self.status_lbl.config(font=("Microsoft JhengHei UI", int(10 * s)))
        self.btn_start.config(font=("Microsoft JhengHei UI", int(13 * s), "bold"), pady=int(10 * s))

    # --------------------------------------------------------------------------
    # 主題切換與 DWM 暗黑標題列
    # --------------------------------------------------------------------------
    def toggle_theme(self):
        self.current_theme_key = "dark" if self.current_theme_key == "light" else "light"
        self.apply_theme()

    def apply_theme(self):
        p = PALETTES[self.current_theme_key]
        is_dark = p["is_dark"]

        # 1. 設置 Windows DWM 標題列深淺色
        apply_title_bar_theme(self.root, dark=is_dark)

        # 2. 全域背景
        self.root.configure(bg=p["bg_main"])
        self.top_frame.configure(bg=p["bg_main"])
        self.top_ctrl_frame.configure(bg=p["bg_main"])
        self.drop_frame.configure(bg=p["bg_main"])
        self.list_tool_frame.configure(bg=p["bg_main"])
        self.list_container.configure(bg=p["bg_main"])
        self.status_frame.configure(bg=p["bg_main"])
        self.bottom_frame.configure(bg=p["bg_main"])

        # 3. 標題與標籤
        self.title_lbl.configure(bg=p["bg_main"], fg=p["ink_primary"])
        self.subtitle_lbl.configure(bg=p["bg_main"], fg=p["ink_muted"])
        self.lbl_zoom.configure(bg=p["bg_main"], fg=p["ink_secondary"])
        self.list_title.configure(bg=p["bg_main"], fg=p["ink_primary"])
        self.chk_red_only.configure(
            bg=p["bg_main"],
            fg=p["accent_cinnabar"],
            selectcolor="#2B2D31" if is_dark else "#FFFFFF",
            activebackground=p["bg_main"],
            activeforeground=p["accent_cinnabar"]
        )
        self.status_lbl.configure(bg=p["bg_main"], fg=p["ink_secondary"])

        # 4. 拖放卡片
        self.drop_box.configure(
            bg=p["dropzone_bg"],
            fg=p["dropzone_fg"],
            highlightbackground=p["dropzone_border"],
            highlightcolor=p["dropzone_border"]
        )

        # 5. 清單列表
        self.listbox.configure(
            bg=p["list_bg"],
            fg=p["list_fg"],
            selectbackground=p["list_select"],
            selectforeground=p["ink_primary"]
        )

        # 6. 工具按鈕
        btn_bg = "#2B2D31" if is_dark else "#FFFFFF"
        btn_fg = p["ink_secondary"]
        self.btn_theme_toggle.configure(
            text="☀️ 淺色" if is_dark else "🌙 深色",
            bg=btn_bg,
            fg=btn_fg
        )
        self.btn_open_notepad.configure(bg=btn_bg, fg=btn_fg)
        self.btn_reload.configure(bg=btn_bg, fg=btn_fg)
        self.btn_save.configure(bg=btn_bg, fg=btn_fg)
        self.btn_clear.configure(bg=btn_bg, fg=p["accent_cinnabar"])

        # 7. 底部啟動大按鈕
        self.btn_start.configure(
            bg=p["accent_cinnabar"],
            fg="#FFFFFF",
            activebackground="#AA2F2F" if not is_dark else "#C83B3B",
            activeforeground="#FFFFFF"
        )

    # --------------------------------------------------------------------------
    # 拖曳與檔案加入
    # --------------------------------------------------------------------------
    def on_drop_enter(self):
        p = PALETTES[self.current_theme_key]
        self.drop_box.configure(
            bg="#3A382A" if p["is_dark"] else "#FBF5E6",
            highlightthickness=3,
            highlightbackground=p["accent_cinnabar"]
        )

    def on_drop_leave(self):
        self.apply_theme()

    def on_files_dropped(self, event):
        self.apply_theme()
        paths = self.parse_dropped_paths(event.data)
        added_count = 0
        for p in paths:
            clean_p = os.path.normpath(p.strip())
            if clean_p and clean_p not in self.playlist_items:
                self.playlist_items.append(clean_p)
                added_count += 1
        
        if added_count > 0:
            self.refresh_listbox()
            self.save_playlist_file(self.playlist_path)
            self.status_lbl.config(text=f"已成功加入 {added_count} 筆語料來源！")
        else:
            self.status_lbl.config(text="拖放的項目已在清單中或無效。")

    def parse_dropped_paths(self, data_str):
        # 1. 優先使用 Tk 原生 splitlist (完美處理 Windows 帶空格、花括號與中文路徑)
        paths = []
        try:
            raw_list = self.root.tk.splitlist(data_str)
            for item in raw_list:
                item_str = str(item).strip('{}').strip()
                if os.path.exists(item_str):
                    paths.append(item_str)
        except Exception:
            pass

        # 2. 保底正則解析
        if not paths:
            pattern = r'\{([^}]+)\}|(\S+)'
            matches = re.findall(pattern, data_str)
            for m in matches:
                p = m[0] if m[0] else m[1]
                p = p.strip()
                if os.path.exists(p):
                    paths.append(p)
        return paths

    def browse_and_add(self):
        choice = messagebox.askquestion("選擇類型", "要加入「資料夾」嗎？\n點「是」選擇資料夾，點「否」選擇檔案。", icon='question')
        if choice == 'yes':
            d = filedialog.askdirectory(title="選擇寫作資料夾")
            if d:
                norm_d = os.path.normpath(d)
                if norm_d not in self.playlist_items:
                    self.playlist_items.append(norm_d)
                    self.refresh_listbox()
                    self.save_playlist_file(self.playlist_path)
        else:
            files = filedialog.askopenfilenames(
                title="選擇文章檔案或捷徑",
                filetypes=[("語料檔案與捷徑", "*.txt *.md *.markdown *.json *.csv *.lnk"), ("所有檔案", "*.*")]
            )
            if files:
                for f in files:
                    norm_f = os.path.normpath(f)
                    if norm_f not in self.playlist_items:
                        self.playlist_items.append(norm_f)
                self.refresh_listbox()
                self.save_playlist_file(self.playlist_path)

    def remove_selected_item(self):
        sel = self.listbox.curselection()
        if sel:
            idx = sel[0]
            del self.playlist_items[idx]
            self.refresh_listbox()
            self.save_playlist_file(self.playlist_path)

    def clear_playlist(self):
        if self.playlist_items and messagebox.askyesno("確認清空", "確定要清空目前的語料播放清單嗎？"):
            self.playlist_items = []
            self.refresh_listbox()
            self.save_playlist_file(self.playlist_path)

    def refresh_listbox(self):
        self.listbox.delete(0, tk.END)
        for i, item in enumerate(self.playlist_items, 1):
            tag = " [捷徑]" if item.lower().endswith(".lnk") else (" [目錄]" if os.path.isdir(item) else " [檔案]")
            self.listbox.insert(tk.END, f"{i:2d}. {tag} {item}")

    # --------------------------------------------------------------------------
    # m3u8 風格純文字播放清單儲存與讀取
    # --------------------------------------------------------------------------
    def load_playlist_file(self, path):
        if not os.path.exists(path):
            return
        self.playlist_items = []
        try:
            with open(path, "r", encoding="utf-8") as f:
                for line in f:
                    clean = line.strip()
                    if clean and not clean.startswith("#"):
                        if clean not in self.playlist_items:
                            self.playlist_items.append(clean)
            self.refresh_listbox()
        except Exception as e:
            print("讀取清單失敗", e)

    def save_playlist_file(self, path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        try:
            with open(path, "w", encoding="utf-8") as f:
                f.write("# ==============================================================================\n")
                f.write("# 如喜人工餵料清單 (RuhI Corpus Playlist - m3u8 format)\n")
                f.write("# 每一行代表一個檔案、資料夾或捷徑 (.lnk)，可在記事本中隨意編輯！\n")
                f.write("# ==============================================================================\n\n")
                for item in self.playlist_items:
                    f.write(f"{item}\n")
        except Exception as e:
            print("儲存清單失敗", e)

    def save_playlist_as(self):
        path = filedialog.asksaveasfilename(
            title="另存語料清單",
            defaultextension=".ruhi_list",
            filetypes=[("如喜語料清單", "*.ruhi_list"), ("文字檔", "*.txt"), ("所有檔案", "*.*")]
        )
        if path:
            self.save_playlist_file(path)
            self.playlist_path = path
            messagebox.showinfo("儲存成功", f"清單已成功儲存至：\n{path}")

    def open_playlist_in_notepad(self):
        self.save_playlist_file(self.playlist_path)
        subprocess.Popen(["notepad.exe", self.playlist_path])

    # --------------------------------------------------------------------------
    # 核心管線執行 (RuhMark 清洗 ➔ 錯字除魅 ➔ 熱寫入)
    # --------------------------------------------------------------------------
    def start_feeding_thread(self):
        if self.is_running:
            return
        if not self.playlist_items:
            messagebox.showwarning("清單為空", "請先拖放或加入至少一個檔案、資料夾或捷徑！")
            return

        self.is_running = True
        self.btn_start.config(state=tk.DISABLED, text="⏳ 正在深度餵料學習中...")
        threading.Thread(target=self.run_feeding_pipeline, daemon=True).start()

    def run_feeding_pipeline(self):
        try:
            self.status_lbl.config(text="🔍 正在穿透解析所有捷徑與目錄檔案...")
            self.progress_bar['value'] = 10

            expanded_files = []
            supported_exts = {'.txt', '.md', '.markdown', '.docx', '.odt', '.json', '.csv'}

            for raw_item in self.playlist_items:
                if not os.path.exists(raw_item):
                    continue

                if raw_item.lower().endswith(".lnk"):
                    target = resolve_windows_shortcut(raw_item)
                    if target and os.path.exists(target):
                        if os.path.isfile(target):
                            if os.path.splitext(target)[1].lower() in supported_exts:
                                expanded_files.append((target, f"捷徑 ➔ {os.path.basename(raw_item)}"))
                        elif os.path.isdir(target):
                            for r, _, files in os.walk(target):
                                for f in files:
                                    if os.path.splitext(f)[1].lower() in supported_exts:
                                        expanded_files.append((os.path.join(r, f), f"捷徑目錄 [{os.path.basename(raw_item)}] ➔ {f}"))
                elif os.path.isdir(raw_item):
                    for r, _, files in os.walk(raw_item):
                        for f in files:
                            if os.path.splitext(f)[1].lower() in supported_exts:
                                expanded_files.append((os.path.join(r, f), f"目錄檔案 ➔ {f}"))
                elif os.path.isfile(raw_item):
                    if os.path.splitext(raw_item)[1].lower() in supported_exts:
                        expanded_files.append((raw_item, f"本體檔案 ➔ {os.path.basename(raw_item)}"))

            if not expanded_files:
                self.root.after(0, lambda: messagebox.showwarning("查無語料", "在清單指定的位置中未找到受支援的文字檔案！"))
                return

            self.status_lbl.config(text=f"⚡ 呼叫 RuhMark 深度清洗 {len(expanded_files)} 份文稿 (紅色親筆模式)...")
            self.progress_bar['value'] = 30

            # 2. 調用 RuhMark 深度清洗 (傳入 red_only 設置與進度回調)
            is_red_only = self.var_red_only.get()
            def on_clean_progress(curr, total, name):
                pct = 20 + int(50 * (curr / total))
                self.root.after(0, lambda c=curr, t=total, p=pct, n=name: [
                    self.progress_bar.config(value=p),
                    self.status_lbl.config(text=f"⚡ RuhMark 清洗中 ({c}/{t}): {n[:35]}")
                ])

            cleaned_records = clean_corpus_with_ruhmark(
                expanded_files,
                output_dir=CORPUS_CLEANED_DIR,
                red_only=is_red_only,
                progress_cb=on_clean_progress
            )

            all_sentences = []
            for rec in cleaned_records:
                all_sentences.extend(rec["sentences"])

            self.status_lbl.config(text=f"🛡️  本地 AI 錯字防火牆掃描與 Bigram 語意萃取 (共 {len(all_sentences)} 句)...")
            self.progress_bar['value'] = 70

            learner = RuhiPersonalLearner()
            report = learner.audit_and_learn(all_sentences)

            self.status_lbl.config(text="🏛️ 正在安全寫入如喜神經轉移矩陣 (transitions_user.tsv)...")
            self.progress_bar['value'] = 90

            added_count = learner.commit_transitions(report, min_freq=2)

            self.progress_bar['value'] = 100
            self.status_lbl.config(text="🎉 深度餵料大功告成！如喜已成功吸收大謙前輩經典與大十精粹語法！")

            typo_info = f"攔截到 {len(report['typos'])} 處可能混淆字詞" if report['typos'] else "文筆極其嚴謹，0 錯字！"
            msg = (
                f"🎉 如喜人工餵料圓滿完成！\n\n"
                f"• 原始語料來源：{len(expanded_files)} 份文件（含捷徑穿透）\n"
                f"• RuhMark 清洗副本：已歸檔至 corpus_cleaned/\n"
                f"• 總分析語意句數：{report['total_sentences']} 句\n"
                f"• 本地錯字防火牆：{typo_info}\n"
                f"• 新增專屬轉移矩陣：{added_count} 組高頻搭配\n\n"
                f"如喜輸入法已即刻熱重載，打字時立刻享受人機合一的流暢手感！"
            )
            self.root.after(0, lambda: messagebox.showinfo("餵料完成", msg))

        except Exception as e:
            self.root.after(0, lambda: messagebox.showerror("執行錯誤", f"餵料過程發生異常：\n{e}"))
        finally:
            self.is_running = False
            self.root.after(0, lambda: self.btn_start.config(
                state=tk.NORMAL,
                text="⚡ 開始深度餵料學習 (RuhMark 清洗 ➔ 錯字除魅 ➔ 熱寫入)"
            ))

def main():
    root = TkinterDnD.Tk()
    app = CorpusFeederApp(root)
    root.mainloop()

if __name__ == '__main__':
    main()
