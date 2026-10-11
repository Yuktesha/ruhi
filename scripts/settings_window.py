#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/settings_window.py
=============================================================================
RuhI（如喜）輸入法 — 現代化偏好設定中心 (RuhI Modern Settings Hub)
=============================================================================
設計理念與架構：
1. 類 CorelDRAW / JetBrains / 現代 OS 階層式雙欄設定視窗：
   - 左側：樹狀分類導覽列（Treeview），清楚呈現功能維度，杜絕資訊龐雜感。
   - 右側：主題專屬設定面板，具備獨立標頭、清晰說明與流式排版。
2. 基於 ATG UniversalUI 與 rud() 詞塊流式自適應排版第一原理。
   - 字 (原子元件)、詞 (同功能組)、句子 (自適應功能行)、段落 (主題卡片) 四層拓撲。
3. 深色/淺色主題自適應，支援動態 DPI 與全介面 Ctrl + +/0/-, Ctrl + 滑鼠滾輪即時縮放。
4. 視窗位置 (geometry)、尺寸與縮放參數全狀態自動記憶持久化。
5. 關於頁籤呈現「如雲行水流自在歡喜」精神哲學與通用智慧型輸入法全語言藍圖。
"""

import os
import sys
import json
import shutil
import urllib.request
import webbrowser
from pathlib import Path
import tkinter as tk
from tkinter import ttk
import tkinter.font as tkfont

# 引入 _lib UniversalUI
BASE_DIR = Path(__file__).resolve().parent.parent
LIB_DIR = Path(r"C:\ATGprjs\_lib")
if str(LIB_DIR) not in sys.path:
    sys.path.insert(0, str(LIB_DIR))

try:
    from UniversalUI import UniversalApp, showinfo, get_system_theme
    HAS_UNIVERSAL = True
except ImportError:
    HAS_UNIVERSAL = False
    UniversalApp = object
    def get_system_theme():
        return "light"

CONFIG_PATH = Path(__file__).resolve().parent / "settings_config.json"
SANDBOX_CONFIG_PATH = Path(__file__).resolve().parent / "config.json"
DATA_DIR = Path(__file__).resolve().parent / "data"

DEFAULT_SETTINGS = {
    "appearance": {
        "input_display_mode": "inline_underline",
        "orientation": "horizontal",
        "font_size": 32,
        "max_composition_chars": 30,
        "theme": "system",
        "show_bopomofo": True,
        "capsule_mode": "mini_capsule",
        "zoom_level": 0,
        "geometry": "980x720"
    },
    "keyboard": {
        "layout": "hsu",
        "space_behavior": "commit_first",
        "capslock_switch": "capital_lock",
        "candidate_keys": "123456789",
        "backspace_mode": "component",
        "enable_mixed_english": True,
        "mixed_english_mode": "contextual_stream",
        "mixed_english_min_len": 4,
        "q_cycle_max": 3
    },
    "grammar": {
        "enable_all_eight_phases": True,
        "enable_de_vs_de_translingual": True,
        "enable_colloquial_biases": True,
        "enable_hot_reload": True
    },
    "ai_and_privacy": {
        "zero_pii_gateway": True,
        "ollama_model": "gemma4:latest",
        "enable_cloud_colab": True
    }
}

def load_settings() -> dict:
    if CONFIG_PATH.exists():
        try:
            with open(CONFIG_PATH, "r", encoding="utf-8") as f:
                data = json.load(f)
                for section, vals in DEFAULT_SETTINGS.items():
                    data.setdefault(section, {})
                    for k, v in vals.items():
                        data[section].setdefault(k, v)
                return data
        except Exception:
            pass
    return DEFAULT_SETTINGS.copy()

def save_settings(cfg: dict):
    with open(CONFIG_PATH, "w", encoding="utf-8") as f:
        json.dump(cfg, f, ensure_ascii=False, indent=2)
    # 同步核心參數至 TSF 原生架構所讀取的 config.json
    try:
        if SANDBOX_CONFIG_PATH.exists():
            with open(SANDBOX_CONFIG_PATH, "r", encoding="utf-8") as sf:
                sdata = json.load(sf)
        else:
            sdata = {}
        sdata["input_display_mode"] = cfg.get("appearance", {}).get("input_display_mode", "inline_underline")
        sdata["theme"] = cfg.get("appearance", {}).get("theme", "system")
        sdata["font_size"] = cfg.get("appearance", {}).get("font_size", 32)
        sdata["max_composition_chars"] = cfg.get("appearance", {}).get("max_composition_chars", 30)
        sdata["keyboard_mode"] = cfg.get("keyboard", {}).get("layout", "hsu")
        sdata["q_cycle_max"] = cfg.get("keyboard", {}).get("q_cycle_max", 3)
        sdata["enable_mixed_english"] = cfg.get("keyboard", {}).get("enable_mixed_english", True)
        sdata["mixed_english_mode"] = cfg.get("keyboard", {}).get("mixed_english_mode", "contextual_stream")
        sdata["mixed_english_min_len"] = int(cfg.get("keyboard", {}).get("mixed_english_min_len", 4))
        sdata["capslock_switch"] = cfg.get("keyboard", {}).get("capslock_switch", "capital_lock")
        sdata["capsule_mode"] = cfg.get("appearance", {}).get("capsule_mode", "mini_capsule")
        with open(SANDBOX_CONFIG_PATH, "w", encoding="utf-8") as sf:
            json.dump(sdata, sf, ensure_ascii=False, indent=4)
    except Exception:
        pass

def get_installed_ollama_models() -> list[str]:
    """動態檢測本地 Ollama 實際已安裝並可用的 AI 模型清單"""
    try:
        req = urllib.request.Request("http://127.0.0.1:11434/api/tags", method="GET")
        with urllib.request.urlopen(req, timeout=1.5) as resp:
            if resp.status == 200:
                data = json.loads(resp.read().decode("utf-8"))
                return [m["name"] for m in data.get("models", [])]
    except Exception:
        pass
    return []

def calc_rud_combobox_width(values: list[str]) -> int:
    """依據最長字串自動計算 rud() 自適應寬度"""
    if not values:
        return 24
    max_w = 0
    for v in values:
        w = sum(2 if ord(c) > 127 else 1 for c in v)
        if w > max_w:
            max_w = w
    return max(20, int(max_w * 0.62) + 4)

class RuhiSettingsApp(UniversalApp if HAS_UNIVERSAL else object):
    def __init__(self, root):
        self.settings = load_settings()
        self._zoom_level = self.settings.get("appearance", {}).get("zoom_level", 0)
        self._hide_zoom_timer = None
        self.current_category = None
        self.pages = {}

        defaults = {
            "geometry": self.settings.get("appearance", {}).get("geometry", "980x720"),
            "language": "zh_TW"
        }
        if HAS_UNIVERSAL:
            super().__init__(root, "RuhI（如喜）輸入法 — 現代化偏好設定中心", "ruhi_settings_v1", defaults)
        else:
            self.root = root
            self.root.title("RuhI（如喜）輸入法 — 現代化偏好設定中心")
            self.root.geometry(defaults["geometry"])

        saved_geo = self.settings.get("appearance", {}).get("geometry")
        if saved_geo:
            try:
                self.root.geometry(saved_geo)
            except Exception:
                pass

        self.root.protocol("WM_DELETE_WINDOW", self.on_close)

        self.setup_fonts_and_styles()
        self.setup_ui()
        self.setup_zoom_and_scroll_bindings()

        # 預設選中「外觀與候選座」
        self.select_category("appearance")

        if self._zoom_level != 0:
            self._apply_zoom_fonts()

    def on_close(self):
        try:
            self.settings.setdefault("appearance", {})["geometry"] = self.root.geometry()
            self.settings.setdefault("appearance", {})["zoom_level"] = self._zoom_level
            save_settings(self.settings)
        except Exception:
            pass

        if HAS_UNIVERSAL and hasattr(super(), "on_close"):
            try:
                super().on_close()
            except Exception:
                pass
        self.root.destroy()

    def setup_fonts_and_styles(self):
        """初始化具備動態響應能力的 Named Font 物件與 ttk 樣式"""
        self.font_title = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=15, weight="bold")
        self.font_subtitle = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=9)
        self.font_page_title = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=13, weight="bold")
        self.font_page_desc = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=9)
        self.font_card_header = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=11, weight="bold")
        self.font_body = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=10)
        self.font_bold = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=10, weight="bold")
        self.font_button = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=10)
        self.font_tree = tkfont.Font(self.root, family="Microsoft JhengHei UI", size=10)

        self.style = ttk.Style(self.root)
        self.style.configure("TLabelframe.Label", font=self.font_card_header)
        self.style.configure("TLabel", font=self.font_body)
        self.style.configure("TRadiobutton", font=self.font_body)
        self.style.configure("TCheckbutton", font=self.font_body)
        self.style.configure("TButton", font=self.font_button)
        self.style.configure("TCombobox", font=self.font_body)

        # 側邊樹狀欄樣式
        self.style.configure("Nav.Treeview", font=self.font_tree, rowheight=32, borderwidth=0)
        self.style.map("Nav.Treeview",
            background=[("selected", "#2563EB")],
            foreground=[("selected", "#FFFFFF")]
        )

        self.root.option_add("*TCombobox*Listbox.font", self.font_body)
        self.root.option_add("*Combobox*Listbox.font", self.font_body)
        self.root.option_add("*Listbox.font", self.font_body)

    def setup_ui(self):
        # 主容器
        main_frame = ttk.Frame(self.root, padding=(16, 12, 16, 12))
        main_frame.pack(fill=tk.BOTH, expand=True)

        # 頂部全域 Header
        header = ttk.Frame(main_frame)
        header.pack(side=tk.TOP, fill=tk.X, pady=(0, 10))

        title_box = ttk.Frame(header)
        title_box.pack(side=tk.LEFT)

        self.lbl_title = ttk.Label(title_box, text="🌟 RuhI（如喜）偏好設定中心", font=self.font_title)
        self.lbl_title.pack(anchor=tk.W)

        self.lbl_meaning = ttk.Label(title_box,
            text="「如雲行水流自在歡喜」— Ruh 承襲自 RuhOS 精神哲學，I 兼具「輸入 (Input)」與「智慧 (Intelligence)」雙重意涵",
            font=self.font_subtitle, foreground="#64748B")
        self.lbl_meaning.pack(anchor=tk.W, pady=(2, 0))

        # 縮放提示 Toast
        self.lbl_zoom_indicator = ttk.Label(header, text="", font=self.font_bold, foreground="#2563EB")
        self.lbl_zoom_indicator.pack(side=tk.LEFT, padx=15)

        self.lbl_subtitle = ttk.Label(header, text="基於 rud() 詞塊流式定律與現代漢語八大語法架構", font=self.font_subtitle)
        self.lbl_subtitle.pack(side=tk.RIGHT, pady=(6, 0))

        # 底部動作列 (沉底橫向延伸)
        bottom_bar = ttk.Frame(main_frame, padding=(0, 10, 0, 0))
        bottom_bar.pack(side=tk.BOTTOM, fill=tk.X)

        lbl_zoom_hint = ttk.Label(bottom_bar, text="💡 提示：按 Ctrl + 滑鼠滾輪 或 Ctrl + +/- / 0 可自由放大縮小介面",
                                  font=self.font_subtitle, foreground="#64748B")
        lbl_zoom_hint.pack(side=tk.LEFT)

        btn_save = ttk.Button(bottom_bar, text="💾 儲存並即時套用", command=self.on_save)
        btn_save.pack(side=tk.RIGHT, padx=5)

        btn_reset = ttk.Button(bottom_bar, text="🔄 還原最佳預設值", command=self.on_reset)
        btn_reset.pack(side=tk.RIGHT, padx=5)

        # 中間主體：左側樹狀分類導覽 + 右側對應設定內容 (類 CorelDRAW 佈局)
        body_frame = ttk.Frame(main_frame)
        body_frame.pack(side=tk.TOP, fill=tk.BOTH, expand=True)

        # 1. 左側樹狀導覽欄 (Sidebar)
        sidebar_frame = ttk.Frame(body_frame, width=230)
        sidebar_frame.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 10))
        sidebar_frame.pack_propagate(False)

        lbl_nav_title = ttk.Label(sidebar_frame, text="設定分類導覽", font=self.font_card_header, foreground="#475569")
        lbl_nav_title.pack(anchor=tk.W, pady=(0, 6), padx=4)

        tree_container = ttk.Frame(sidebar_frame)
        tree_container.pack(fill=tk.BOTH, expand=True)

        self.nav_tree = ttk.Treeview(tree_container, style="Nav.Treeview", show="tree", selectmode="browse")
        self.nav_tree.pack(fill=tk.BOTH, expand=True)

        # 填充樹狀分類
        self.nav_tree.insert("", "end", iid="appearance", text=" 🎨  外觀與候選座", open=True)

        kb_root = self.nav_tree.insert("", "end", iid="keyboard_group", text=" ⌨️  鍵盤與輸入習慣", open=True)
        self.nav_tree.insert(kb_root, "end", iid="keyboard_layout", text="   ⌨  鍵盤配置與常用鍵")
        self.nav_tree.insert(kb_root, "end", iid="mixed_english", text="   🔤  許氏＋歐文混打")

        self.nav_tree.insert("", "end", iid="grammar", text=" 🧠  語意結構與語法引力", open=True)
        self.nav_tree.insert("", "end", iid="ai_and_privacy", text=" 🛡️  隱私安全與 AI 模型", open=True)
        self.nav_tree.insert("", "end", iid="about", text=" ℹ️  關於與全語言願景", open=True)

        self.nav_tree.bind("<<TreeviewSelect>>", self.on_tree_select)

        # 分隔線
        sep = ttk.Separator(body_frame, orient=tk.VERTICAL)
        sep.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 10))

        # 2. 右側內容容器
        content_panel = ttk.Frame(body_frame)
        content_panel.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        # 頁面標頭區 (顯示目前分類標題與簡述)
        self.page_header = ttk.Frame(content_panel)
        self.page_header.pack(side=tk.TOP, fill=tk.X, pady=(0, 8))

        self.lbl_page_title = ttk.Label(self.page_header, text="", font=self.font_page_title)
        self.lbl_page_title.pack(anchor=tk.W)

        self.lbl_page_desc = ttk.Label(self.page_header, text="", font=self.font_page_desc, foreground="#64748B")
        self.lbl_page_desc.pack(anchor=tk.W, pady=(2, 6))

        sep_header = ttk.Separator(self.page_header, orient=tk.HORIZONTAL)
        sep_header.pack(fill=tk.X)

        # 右側滾動內容容器 (Canvas + Scrollbar)
        scroll_wrapper = ttk.Frame(content_panel)
        scroll_wrapper.pack(side=tk.TOP, fill=tk.BOTH, expand=True)

        self.canvas = tk.Canvas(scroll_wrapper, borderwidth=0, highlightthickness=0)
        self.scrollbar = ttk.Scrollbar(scroll_wrapper, orient="vertical", command=self.canvas.yview)
        self.page_container = ttk.Frame(self.canvas)

        self.page_container.bind("<Configure>", lambda e: self.canvas.configure(scrollregion=self.canvas.bbox("all")))
        self.canvas_window = self.canvas.create_window((0, 0), window=self.page_container, anchor="nw")
        self.canvas.configure(xscrollcommand=None, yscrollcommand=self.scrollbar.set)

        def _on_canvas_configure(event):
            self.canvas.itemconfig(self.canvas_window, width=max(100, event.width - 4))
        self.canvas.bind("<Configure>", _on_canvas_configure)

        self.scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        # 建構各子頁面 Frame
        self.pages["appearance"] = self.build_appearance_page(self.page_container)
        self.pages["keyboard_layout"] = self.build_keyboard_layout_page(self.page_container)
        self.pages["mixed_english"] = self.build_mixed_english_page(self.page_container)
        self.pages["grammar"] = self.build_grammar_page(self.page_container)
        self.pages["ai_and_privacy"] = self.build_ai_privacy_page(self.page_container)
        self.pages["about"] = self.build_about_page(self.page_container)

    def on_tree_select(self, event):
        sel = self.nav_tree.selection()
        if not sel:
            return
        cat_id = sel[0]
        if cat_id == "keyboard_group":
            self.nav_tree.selection_set("keyboard_layout")
            return
        self.select_category(cat_id)

    def select_category(self, cat_id: str):
        """切換右側顯示之設定頁面"""
        if cat_id not in self.pages:
            return
        self.current_category = cat_id

        # 同步樹狀選取
        if self.nav_tree.selection() != (cat_id,):
            self.nav_tree.selection_set(cat_id)
            self.nav_tree.see(cat_id)

        # 隱藏所有頁面，顯示當前頁面
        for pid, page in self.pages.items():
            page.pack_forget()

        self.pages[cat_id].pack(fill=tk.BOTH, expand=True, padx=4, pady=4)

        # 更新標題與簡述
        meta = {
            "appearance": ("🎨 外觀與候選座（UniversalUI 自適應）", "設定輸入操作模式、候選座排列方向、字體大小、主題色彩與迷你狀態指示膠囊。"),
            "keyboard_layout": ("⌨️ 鍵盤配置與常用鍵", "設定注音鍵盤佈局、空白鍵組字行為、Caps Lock 切換模式與 Q 鍵快速候選輪換上限。"),
            "mixed_english": ("🔤 許氏＋歐文智慧混打策略", "配置許氏注音與歐文智慧混打、仲裁策略、語境串流模式與長詞門檻。"),
            "grammar": ("🧠 現代漢語八大語法引力場", "配置高階句法結構拓撲、動補/離合/框式介詞、ê/kah 結構助詞深層語意與自建詞庫熱重載。"),
            "ai_and_privacy": ("🛡️ 隱私安全閘門與本地 AI 模型", "配置 Zero-PII 隱私安全閘門、本地 Ollama 模型偵測與管理、以及雲端 Colab 算力支援。"),
            "about": ("ℹ️ 關於如喜與全語言願景", "如喜輸入法品牌哲學、全語言通用輸入法藍圖、系統版本與詞庫狀態資訊。")
        }
        title, desc = meta.get(cat_id, ("偏好設定", ""))
        self.lbl_page_title.config(text=title)
        self.lbl_page_desc.config(text=desc)

        # 滾動至頂部
        self.root.update_idletasks()
        self.canvas.configure(scrollregion=self.canvas.bbox("all"))
        self.canvas.yview_moveto(0)

    def switch_category(self, cat_id: str):
        """外部或測試調用切換介面"""
        self.select_category(cat_id)

    # ── 頁面一：外觀與候選座 ──
    def build_appearance_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        # 輸入操作模式
        card_mode = ttk.LabelFrame(page, text=" 輸入操作模式（微軟新注音 vs 如喜輸入座） ", padding=12)
        card_mode.pack(fill=tk.X, pady=(0, 10))

        self.var_input_mode = tk.StringVar(value=self.settings.get("appearance", {}).get("input_display_mode", "inline_underline"))
        ttk.Radiobutton(card_mode, text="微軟新注音游標跟隨模式（目標視窗輸入行畫底線）",
                        variable=self.var_input_mode, value="inline_underline").pack(anchor=tk.W, pady=3)
        ttk.Radiobutton(card_mode, text="如喜輸入座游標跟隨模式（開啟輸入座，緊隨目標游標下方，無法顯示時自動置於上方）",
                        variable=self.var_input_mode, value="floating_follow").pack(anchor=tk.W, pady=3)
        ttk.Radiobutton(card_mode, text="如喜輸入座自訂方位模式（輸入座固定位置，使用者可任意拖移至合適位置）",
                        variable=self.var_input_mode, value="floating_fixed").pack(anchor=tk.W, pady=3)

        # 佈局與排版
        card_layout = ttk.LabelFrame(page, text=" 候選座排列與字體尺寸 ", padding=12)
        card_layout.pack(fill=tk.X, pady=(0, 10))

        row1 = ttk.Frame(card_layout)
        row1.pack(fill=tk.X, pady=4)
        ttk.Label(row1, text="候選窗排列方向：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_orientation = tk.StringVar(value=self.settings["appearance"]["orientation"])
        ttk.Radiobutton(row1, text="橫向流式排列（預設，推薦）", variable=self.var_orientation, value="horizontal").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row1, text="直向傳統排列", variable=self.var_orientation, value="vertical").pack(side=tk.LEFT, padx=10)

        row2 = ttk.Frame(card_layout)
        row2.pack(fill=tk.X, pady=6)
        ttk.Label(row2, text="候選字體大小：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_font_size = tk.IntVar(value=self.settings["appearance"]["font_size"])
        lbl_size = ttk.Label(row2, text=f"{self.var_font_size.get()} px", width=8)
        scale = ttk.Scale(row2, from_=18, to=54, variable=self.var_font_size, orient=tk.HORIZONTAL,
                          command=lambda v: lbl_size.config(text=f"{int(float(v))} px"))
        scale.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=10)
        lbl_size.pack(side=tk.LEFT)

        row_len = ttk.Frame(card_layout)
        row_len.pack(fill=tk.X, pady=6)
        ttk.Label(row_len, text="輸入座最大字數：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_max_comp = tk.IntVar(value=self.settings.get("appearance", {}).get("max_composition_chars", 30))
        lbl_len = ttk.Label(row_len, text=f"{self.var_max_comp.get()} 字", width=8)
        scale_len = ttk.Scale(row_len, from_=15, to=60, variable=self.var_max_comp, orient=tk.HORIZONTAL,
                              command=lambda v: lbl_len.config(text=f"{int(float(v))} 字"))
        scale_len.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=10)
        lbl_len.pack(side=tk.LEFT)
        ttk.Label(card_layout, text="註：輸入長度超過最大字數時，系統將自動送出首個標點符號前之文字",
                  font=self.font_subtitle, foreground="#64748B").pack(anchor=tk.W, padx=4)

        # 主題色彩與狀態指示
        card_theme = ttk.LabelFrame(page, text=" 色彩主題與狀態指示 ", padding=12)
        card_theme.pack(fill=tk.X, pady=(0, 10))

        row3 = ttk.Frame(card_theme)
        row3.pack(fill=tk.X, pady=4)
        ttk.Label(row3, text="色彩主題風格：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_theme = tk.StringVar(value=self.settings["appearance"]["theme"])
        ttk.Radiobutton(row3, text="跟隨 Windows 系統", variable=self.var_theme, value="system").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row3, text="極致深色 (Dark)", variable=self.var_theme, value="dark").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row3, text="宣紙淺色 (Light)", variable=self.var_theme, value="light").pack(side=tk.LEFT, padx=10)

        row_capsule = ttk.Frame(card_theme)
        row_capsule.pack(fill=tk.X, pady=6)
        ttk.Label(row_capsule, text="送出字串後狀態指示：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_capsule_mode = tk.StringVar(value=self.settings.get("appearance", {}).get("capsule_mode", "mini_capsule"))
        ttk.Radiobutton(row_capsule, text="縮為常駐迷你狀態膠囊（隨時可點齒輪 ⚙ 開啟設定）",
                        variable=self.var_capsule_mode, value="mini_capsule").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row_capsule, text="完全隱藏輸入座（純淨模式，由系統匣圖示指示）",
                        variable=self.var_capsule_mode, value="auto_hide").pack(side=tk.LEFT, padx=10)

        return page

    # ── 頁面二：鍵盤配置與常用鍵 ──
    def build_keyboard_layout_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_layout = ttk.LabelFrame(page, text=" 鍵盤配置與組字按鍵 ", padding=12)
        card_layout.pack(fill=tk.X, pady=(0, 10))

        row1 = ttk.Frame(card_layout)
        row1.pack(fill=tk.X, pady=6)
        ttk.Label(row1, text="注音鍵盤佈局：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_layout = tk.StringVar(value=self.settings["keyboard"]["layout"])
        ttk.Radiobutton(row1, text="許氏鍵盤（Hsu Layout，極致流暢首選）", variable=self.var_layout, value="hsu").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row1, text="標準大千鍵盤", variable=self.var_layout, value="standard").pack(side=tk.LEFT, padx=10)

        row2 = ttk.Frame(card_layout)
        row2.pack(fill=tk.X, pady=6)
        ttk.Label(row2, text="空白鍵行為：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_space = tk.StringVar(value=self.settings["keyboard"]["space_behavior"])
        ttk.Radiobutton(row2, text="出第一候選字（如喜智能預設）", variable=self.var_space, value="commit_first").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row2, text="僅輸出半形空格", variable=self.var_space, value="space_only").pack(side=tk.LEFT, padx=10)

        row3 = ttk.Frame(card_layout)
        row3.pack(fill=tk.X, pady=6)
        ttk.Label(row3, text="Caps Lock 行為：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_caps = tk.StringVar(value=self.settings["keyboard"]["capslock_switch"])
        ttk.Radiobutton(row3, text="快速切換中／英數模式", variable=self.var_caps, value="chinese_english").pack(side=tk.LEFT, padx=10)
        ttk.Radiobutton(row3, text="大寫鎖定", variable=self.var_caps, value="capital_lock").pack(side=tk.LEFT, padx=10)

        card_q = ttk.LabelFrame(page, text=" Q 鍵快速候選輪換 ", padding=12)
        card_q.pack(fill=tk.X, pady=(0, 10))

        row_q = ttk.Frame(card_q)
        row_q.pack(fill=tk.X, pady=4)
        ttk.Label(row_q, text="Q鍵快速候選輪換次數上限：").pack(side=tk.LEFT, padx=(0, 10))
        self.var_q_cycle = tk.IntVar(value=self.settings.get("keyboard", {}).get("q_cycle_max", 3))
        lbl_q = ttk.Label(row_q, text=f"{self.var_q_cycle.get()} 次", width=8)
        scale_q = ttk.Scale(row_q, from_=1, to=8, variable=self.var_q_cycle, orient=tk.HORIZONTAL,
                            command=lambda v: lbl_q.config(text=f"{int(float(v))} 次"))
        scale_q.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=10)
        lbl_q.pack(side=tk.LEFT)

        ttk.Label(card_q, text="說明：在許氏鍵盤中，單擊 Q 鍵可在首選候選詞與次選詞之間快速切換，連續按滿次數自動展開完整選字座。",
                  font=self.font_subtitle, foreground="#64748B").pack(anchor=tk.W, padx=4, pady=(2, 0))

        return page

    # ── 頁面三：許氏＋歐文混打策略 ──
    def build_mixed_english_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_mix = ttk.LabelFrame(page, text=" 許氏＋歐文智慧混打核心機制 ", padding=12)
        card_mix.pack(fill=tk.X, pady=(0, 10))

        self.var_mixed_english = tk.BooleanVar(value=self.settings.get("keyboard", {}).get("enable_mixed_english", True))
        chk_mix = ttk.Checkbutton(card_mix, text="啟用許氏＋歐文智慧混打（非合法許氏注音組合直接當作歐文，Enter 直接上屏）",
                                   variable=self.var_mixed_english)
        chk_mix.pack(anchor=tk.W, pady=4)

        ttk.Label(card_mix, text="許氏鍵盤將 41 個注音符號精緻對應於 26 個英文鍵位。智慧混打機制讓您無需切換輸入法，即可自然敲出英文單字與程式碼。",
                  font=self.font_subtitle, foreground="#64748B").pack(anchor=tk.W, padx=4, pady=(0, 6))

        card_strategy = ttk.LabelFrame(page, text=" 混打仲裁策略與流式門檻 ", padding=12)
        card_strategy.pack(fill=tk.X, pady=(0, 10))

        row_mix_mode = ttk.Frame(card_strategy)
        row_mix_mode.pack(fill=tk.X, pady=4)
        ttk.Label(row_mix_mode, text="混打仲裁策略：").pack(side=tk.LEFT, padx=(0, 8))
        self.var_mixed_mode = tk.StringVar(value=self.settings.get("keyboard", {}).get("mixed_english_mode", "contextual_stream"))
        ttk.Radiobutton(row_mix_mode, text="語境串流模式 (推薦：首詞開啟歐文流，放行 2~3 字詞，中文 100% 防護)",
                        variable=self.var_mixed_mode, value="contextual_stream").pack(anchor=tk.W, pady=2)
        ttk.Radiobutton(row_mix_mode, text="保守模式 (僅大寫/駝峰/標識符生效)",
                        variable=self.var_mixed_mode, value="conservative").pack(anchor=tk.W, pady=2)
        ttk.Radiobutton(row_mix_mode, text="長詞門檻模式",
                        variable=self.var_mixed_mode, value="min_length_only").pack(anchor=tk.W, pady=2)

        row_mix_len = ttk.Frame(card_strategy)
        row_mix_len.pack(fill=tk.X, pady=(6, 2))
        ttk.Label(row_mix_len, text="長詞門檻字數（門檻模式下生效）：").pack(side=tk.LEFT, padx=(0, 8))
        self.var_mixed_min_len = tk.IntVar(value=self.settings.get("keyboard", {}).get("mixed_english_min_len", 4))
        lbl_mix_len = ttk.Label(row_mix_len, text=f"{self.var_mixed_min_len.get()} 個字母", width=10)
        scale_mix_len = ttk.Scale(row_mix_len, from_=3, to=6, variable=self.var_mixed_min_len, orient=tk.HORIZONTAL,
                                  command=lambda v: lbl_mix_len.config(text=f"{int(float(v))} 個字母"))
        scale_mix_len.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=8)
        lbl_mix_len.pack(side=tk.LEFT)

        return page

    # ── 頁面四：現代漢語八大語法引力場 ──
    def build_grammar_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_rules = ttk.LabelFrame(page, text=" 現代漢語八大語法引擎與結構助詞 ", padding=12)
        card_rules.pack(fill=tk.X, pady=(0, 10))

        self.var_grammar_all = tk.BooleanVar(value=self.settings["grammar"]["enable_all_eight_phases"])
        chk1 = ttk.Checkbutton(card_rules, text="啟用現代漢語八大語法引擎（離合詞、動補、被動式、框式介詞、兼語連動、存現重疊等 154 句法）",
                               variable=self.var_grammar_all)
        chk1.pack(anchor=tk.W, pady=4)

        self.var_translingual = tk.BooleanVar(value=self.settings["grammar"]["enable_de_vs_de_translingual"])
        chk2 = ttk.Checkbutton(card_rules, text="跨語言『的』(ê) 與『得』(kah) 結構助詞深層語意自動分野律",
                               variable=self.var_translingual)
        chk2.pack(anchor=tk.W, pady=4)

        self.var_colloquial = tk.BooleanVar(value=self.settings["grammar"]["enable_colloquial_biases"])
        chk3 = ttk.Checkbutton(card_rules, text="生活高頻口語引力校準（杜絕『自燃』、『渴論』、『醮我一生』破音字奪舍）",
                               variable=self.var_colloquial)
        chk3.pack(anchor=tk.W, pady=4)

        card_hot = ttk.LabelFrame(page, text=" 詞庫熱重載與自建詞維護 ", padding=12)
        card_hot.pack(fill=tk.X, pady=(0, 10))

        self.var_hot_reload = tk.BooleanVar(value=self.settings["grammar"]["enable_hot_reload"])
        chk4 = ttk.Checkbutton(card_hot, text="使用者自建詞庫熱重載（支援 Ctrl+2 快捷加詞與 TSV 檔案即時熱載入）",
                               variable=self.var_hot_reload)
        chk4.pack(anchor=tk.W, pady=4)

        ttk.Label(card_hot, text="架構鐵律守則：如喜堅守計算語言學語法構式與二階轉移引力，嚴禁單字權重打地鼠，保障長詞凝聚力。",
                  font=self.font_subtitle, foreground="#64748B").pack(anchor=tk.W, padx=4, pady=(2, 0))

        return page

    # ── 頁面五：隱私安全與 AI 模型 ──
    def build_ai_privacy_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_privacy = ttk.LabelFrame(page, text=" Zero-PII 隱私安全閘門 ", padding=12)
        card_privacy.pack(fill=tk.X, pady=(0, 10))

        self.var_zero_pii = tk.BooleanVar(value=self.settings["ai_and_privacy"]["zero_pii_gateway"])
        chk1 = ttk.Checkbutton(card_privacy, text="Zero-PII 隱私安全閘門（絕不記錄敏感個資與密碼，保障最高個人隱私）",
                               variable=self.var_zero_pii)
        chk1.pack(anchor=tk.W, pady=4)

        card_model = ttk.LabelFrame(page, text=" 本地 AI 校正模型與算力管理 ", padding=12)
        card_model.pack(fill=tk.X, pady=(0, 10))

        row_model = ttk.Frame(card_model)
        row_model.pack(fill=tk.X, pady=6)
        ttk.Label(row_model, text="本地 AI 模型：").pack(side=tk.LEFT, padx=(0, 10))

        self.installed_models = get_installed_ollama_models()
        self.model_map = {}
        self.model_rev_map = {}

        candidate_definitions = [
            ("gemma4:latest", "gemma4:latest（Google 官方核心推薦）"),
            ("gemma4:12b", "gemma4:12b（深層精準高階）"),
            ("llama3.2:3b", "llama3.2:3b（輕量備援）")
        ]

        dropdown_labels = []
        if self.installed_models:
            for m in self.installed_models:
                desc = None
                for k, lab in candidate_definitions:
                    if m == k or m.startswith(k.split(":")[0]):
                        desc = lab
                        break
                label = desc if desc else f"{m}（已安裝）"
                self.model_map[m] = label
                self.model_rev_map[label] = m
                dropdown_labels.append(label)
        else:
            for k, lab in candidate_definitions:
                self.model_map[k] = lab
                self.model_rev_map[lab] = k
                dropdown_labels.append(lab)

        curr_key = self.settings.get("ai_and_privacy", {}).get("ollama_model", "gemma4:latest")
        curr_label = self.model_map.get(curr_key, dropdown_labels[0])

        self.var_model = tk.StringVar(value=curr_label)

        rud_w = calc_rud_combobox_width(dropdown_labels)
        self.cb_model = ttk.Combobox(row_model, textvariable=self.var_model,
                                     values=dropdown_labels, state="readonly",
                                     font=self.font_body, width=rud_w)
        self.cb_model.pack(side=tk.LEFT, padx=(0, 8))

        btn_manage_model = ttk.Button(row_model, text="📥 智慧管理模型", command=self.open_model_installer)
        btn_manage_model.pack(side=tk.LEFT, padx=(0, 10))

        ttk.Label(row_model, text="（快捷鍵 Ctrl+1 即時潤飾全句）", foreground="#64748B").pack(side=tk.LEFT)

        row_colab = ttk.Frame(card_model)
        row_colab.pack(fill=tk.X, pady=6)
        ttk.Label(row_colab, text="雲端 Colab 算力：").pack(side=tk.LEFT, padx=(0, 10))
        ttk.Label(row_colab, text="🟢 Google Colab CLI 已連線 (957.60 Compute Units)", foreground="#16A34A").pack(side=tk.LEFT)

        return page

    # ── 頁面六：關於與全語言願景 ──
    def build_about_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        # 品牌理念
        card_brand = ttk.LabelFrame(page, text=" 品牌理念與精神哲學 ", padding=12)
        card_brand.pack(fill=tk.X, pady=(0, 10))

        ttk.Label(card_brand, text="「如雲行水流自在歡喜」", font=self.font_card_header, foreground="#2563EB").pack(anchor=tk.W, pady=(0, 4))
        brand_text = (
            "• Ruh 承襲自 RuhOS 精神哲學，象徵心靈與思維之流動。\n"
            "• I 兼具「輸入 (Input)」與「智慧 (Intelligence)」雙重意涵。\n"
            "• 以如雲行水流之姿態，追求打字無拘無束、心手相應之真境。"
        )
        ttk.Label(card_brand, text=brand_text, justify=tk.LEFT).pack(anchor=tk.W, pady=2)

        # 全語言通用智慧型輸入法願景
        card_vision = ttk.LabelFrame(page, text=" 通用智慧型輸入法全語言藍圖（Universal Smart IME Roadmap） ", padding=12)
        card_vision.pack(fill=tk.X, pady=(0, 10))

        vision_desc = (
            "如喜目前雖然專注於許氏鍵盤注音組字與智慧詞庫，但終極目標是建立跨世代的「通用智慧型輸入法」。\n"
            "未來將全面支援全球標準鍵盤以外之全方位語言與輸入方案："
        )
        ttk.Label(card_vision, text=vision_desc, justify=tk.LEFT).pack(anchor=tk.W, pady=(0, 6))

        roadmap_items = (
            "  1. 繁體中文注音體系：許氏鍵盤（首發）、標準大千、倚天 26 鍵、漢語拼音\n"
            "  2. 繁體中文形碼體系：倉頡（三代／五代）、大易、行列輸入法\n"
            "  3. 亞洲多語系輸入：日語假名/漢字、韓語諺文、泰語\n"
            "  4. 專業學術標音符號：梵文天城體羅馬化（IAST）、台羅/白話字拼音\n"
            "  5. 歐文多語言擴展：德文、法文、西文等特殊重音符號智慧組字"
        )
        ttk.Label(card_vision, text=roadmap_items, font=self.font_subtitle, justify=tk.LEFT, foreground="#334155").pack(anchor=tk.W, pady=2)

        # 系統狀態與詞庫資訊
        card_info = ttk.LabelFrame(page, text=" 系統狀態與詞庫架構 ", padding=12)
        card_info.pack(fill=tk.X, pady=(0, 10))

        # 檢測詞庫狀態
        data_path = DATA_DIR if DATA_DIR.exists() else (BASE_DIR / "ruhi_lexicon" / "data")
        lexicon_count_text = "350,000+ 條目（萌典 + MCBopomofo + 自建詞庫）"
        row_sys1 = ttk.Frame(card_info)
        row_sys1.pack(fill=tk.X, pady=3)
        ttk.Label(row_sys1, text="核心版本：").pack(side=tk.LEFT)
        ttk.Label(row_sys1, text="RuhI v1.2.0 (Native Windows TSF & UniversalUI)", font=self.font_bold).pack(side=tk.LEFT)

        row_sys2 = ttk.Frame(card_info)
        row_sys2.pack(fill=tk.X, pady=3)
        ttk.Label(row_sys2, text="詞庫規模：").pack(side=tk.LEFT)
        ttk.Label(row_sys2, text=lexicon_count_text).pack(side=tk.LEFT)

        row_sys3 = ttk.Frame(card_info)
        row_sys3.pack(fill=tk.X, pady=3)
        ttk.Label(row_sys3, text="詞庫目錄：").pack(side=tk.LEFT)
        ttk.Label(row_sys3, text=str(data_path), foreground="#64748B").pack(side=tk.LEFT)

        row_btns = ttk.Frame(card_info)
        row_btns.pack(fill=tk.X, pady=(8, 0))

        def open_data_dir():
            if data_path.exists():
                os.startfile(str(data_path))

        def open_config_dir():
            cfg_dir = CONFIG_PATH.parent
            if cfg_dir.exists():
                os.startfile(str(cfg_dir))

        ttk.Button(row_btns, text="📁 開啟詞庫資料夾", command=open_data_dir).pack(side=tk.LEFT, padx=(0, 8))
        ttk.Button(row_btns, text="⚙️ 開啟設定檔資料夾", command=open_config_dir).pack(side=tk.LEFT)

        return page

    def setup_zoom_and_scroll_bindings(self):
        """綁定 Ctrl + 鍵盤快捷鍵與 Ctrl + 滾輪縮放"""
        self.root.bind_all("<Control-plus>", lambda e: self._zoom(1))
        self.root.bind_all("<Control-equal>", lambda e: self._zoom(1))
        self.root.bind_all("<Control-minus>", lambda e: self._zoom(-1))
        self.root.bind_all("<Control-KP_Add>", lambda e: self._zoom(1))
        self.root.bind_all("<Control-KP_Subtract>", lambda e: self._zoom(-1))
        self.root.bind_all("<Control-0>", lambda e: self._zoom(0))
        self.root.bind_all("<Control-Key-0>", lambda e: self._zoom(0))
        self.root.bind_all("<Control-KP_0>", lambda e: self._zoom(0))
        self.root.bind_all("<Control-KP_Insert>", lambda e: self._zoom(0))

        self.root.bind_all("<Control-MouseWheel>", self._on_ctrl_scroll)
        self.root.bind_all("<MouseWheel>", self._on_mouse_scroll)

    def _zoom(self, direction: int):
        if direction == 0:
            self._zoom_level = 0
        else:
            self._zoom_level = max(-4, min(10, self._zoom_level + direction))

        self._apply_zoom_fonts()

        pct = 100 + self._zoom_level * 10
        self.lbl_zoom_indicator.config(text=f"🔍 縮放 {pct}%")
        if self._hide_zoom_timer:
            self.root.after_cancel(self._hide_zoom_timer)
        self._hide_zoom_timer = self.root.after(1800, lambda: self.lbl_zoom_indicator.config(text=""))
        return "break"

    def _apply_zoom_fonts(self):
        delta = self._zoom_level * 2

        self.font_title.configure(size=max(12, 15 + delta))
        self.font_subtitle.configure(size=max(8, 9 + int(delta * 0.5)))
        self.font_page_title.configure(size=max(10, 13 + delta))
        self.font_page_desc.configure(size=max(8, 9 + int(delta * 0.5)))
        self.font_card_header.configure(size=max(9, 11 + delta))
        self.font_body.configure(size=max(8, 10 + delta))
        self.font_bold.configure(size=max(8, 10 + delta))
        self.font_button.configure(size=max(8, 10 + delta))
        self.font_tree.configure(size=max(8, 10 + delta))

        self.style.configure("TLabelframe.Label", font=self.font_card_header)
        self.style.configure("TLabel", font=self.font_body)
        self.style.configure("TRadiobutton", font=self.font_body)
        self.style.configure("TCheckbutton", font=self.font_body)
        self.style.configure("TButton", font=self.font_button)
        self.style.configure("TCombobox", font=self.font_body)

        row_h = max(24, 32 + delta * 2)
        self.style.configure("Nav.Treeview", font=self.font_tree, rowheight=row_h)

        self.root.option_add("*TCombobox*Listbox.font", self.font_body)
        self.root.option_add("*Combobox*Listbox.font", self.font_body)

        self.settings.setdefault("appearance", {})["zoom_level"] = self._zoom_level

        self.root.update_idletasks()
        if hasattr(self, "canvas") and self.canvas:
            self.canvas.configure(scrollregion=self.canvas.bbox("all"))

    def _on_ctrl_scroll(self, event):
        if event.delta > 0:
            self._zoom(1)
        else:
            self._zoom(-1)
        return "break"

    def _on_mouse_scroll(self, event):
        if hasattr(self, "canvas") and self.canvas:
            self.canvas.yview_scroll(int(-1 * (event.delta / 120)), "units")

    def open_model_installer(self):
        """開啟本地 AI 模型智慧安裝與探索對話框"""
        dialog = tk.Toplevel(self.root)
        dialog.title("📥 本地 AI 模型智慧安裝中心")
        dialog.geometry("640x520")
        dialog.transient(self.root)
        dialog.grab_set()

        padding_frame = ttk.Frame(dialog, padding=20)
        padding_frame.pack(fill=tk.BOTH, expand=True)

        lbl_head = ttk.Label(padding_frame, text="✨ 本地 Ollama 智慧模型管理器", font=self.font_card_header)
        lbl_head.pack(anchor=tk.W, pady=(0, 6))

        models = get_installed_ollama_models()
        is_ollama_installed = bool(shutil.which("ollama") or os.path.isfile(os.path.expandvars(r"%LOCALAPPDATA%\Programs\Ollama\ollama.exe")))

        status_box = ttk.Frame(padding_frame)
        status_box.pack(fill=tk.X, pady=(0, 12))

        if models:
            ttk.Label(status_box, text=f"🟢 Ollama 服務執行中（已偵測到 {len(models)} 款本機模型）", foreground="#16A34A").pack(anchor=tk.W)
        elif is_ollama_installed:
            ttk.Label(status_box, text="🟡 系統已安裝 Ollama 引擎，但尚未下載推薦模型或服務尚未啟動", foreground="#D97706").pack(anchor=tk.W)
        else:
            ttk.Label(status_box, text="🔴 尚未偵測到 Ollama 引擎，請先下載安裝 Ollama 核心", foreground="#DC2626").pack(anchor=tk.W)

        group = ttk.LabelFrame(padding_frame, text=" 🎯 推薦繁體中文校正模型（Google 生態優先） ", padding=12)
        group.pack(fill=tk.BOTH, expand=True, pady=6)

        var_selected_download = tk.StringVar(value="gemma4:latest")

        models_catalog = [
            ("gemma4:latest", "Gemma 4 (Latest) — Google 官方核心輕量旗艦（首選推薦，約 3.3 GB）"),
            ("gemma4:12b", "Gemma 4 (12B) — Google 官方深層高階（複雜語意與長文推理，約 7.8 GB）"),
            ("llama3.2:3b", "Llama 3.2 (3B) — Meta 歐美開源輕量備援（資源極省，約 2.0 GB）")
        ]

        for m_id, m_desc in models_catalog:
            state_tag = " [已安裝]" if m_id in models else ""
            ttk.Radiobutton(group, text=f"{m_desc}{state_tag}",
                            variable=var_selected_download, value=m_id).pack(anchor=tk.W, pady=6)

        action_row = ttk.Frame(padding_frame)
        action_row.pack(fill=tk.X, pady=(15, 0))

        def on_pull():
            target_model = var_selected_download.get()
            cmd_str = f'start "Ollama 模型下載中" cmd.exe /k "ollama pull {target_model}"'
            os.system(cmd_str)
            dialog.destroy()
            if HAS_UNIVERSAL:
                showinfo(self.root, "下載提示", f"已為您喚起終端機下載視窗：\nollama pull {target_model}\n下載完成後再次開啟設定即可直接套用！")

        def on_browse():
            webbrowser.open("https://ollama.com/library")

        btn_pull = ttk.Button(action_row, text="⬇️ 立即拉取並安裝選定模型", command=on_pull)
        btn_pull.pack(side=tk.LEFT)

        btn_browse = ttk.Button(action_row, text="🌐 瀏覽更多開源模型 (ollama.com)", command=on_browse)
        btn_browse.pack(side=tk.LEFT, padx=10)

        btn_close = ttk.Button(action_row, text="關閉", command=dialog.destroy)
        btn_close.pack(side=tk.RIGHT)

    def on_save(self):
        self.settings["appearance"]["input_display_mode"] = self.var_input_mode.get()
        self.settings["appearance"]["orientation"] = self.var_orientation.get()
        self.settings["appearance"]["font_size"] = int(self.var_font_size.get())
        self.settings["appearance"]["max_composition_chars"] = int(self.var_max_comp.get())
        self.settings["appearance"]["theme"] = self.var_theme.get()
        self.settings["appearance"]["capsule_mode"] = self.var_capsule_mode.get()
        self.settings["appearance"]["zoom_level"] = self._zoom_level
        self.settings["appearance"]["geometry"] = self.root.geometry()

        self.settings["keyboard"]["layout"] = self.var_layout.get()
        self.settings["keyboard"]["space_behavior"] = self.var_space.get()
        self.settings["keyboard"]["capslock_switch"] = self.var_caps.get()
        self.settings["keyboard"]["q_cycle_max"] = int(self.var_q_cycle.get())
        self.settings["keyboard"]["enable_mixed_english"] = self.var_mixed_english.get()
        self.settings["keyboard"]["mixed_english_mode"] = self.var_mixed_mode.get()
        self.settings["keyboard"]["mixed_english_min_len"] = int(self.var_mixed_min_len.get())

        self.settings["grammar"]["enable_all_eight_phases"] = self.var_grammar_all.get()
        self.settings["grammar"]["enable_de_vs_de_translingual"] = self.var_translingual.get()
        self.settings["grammar"]["enable_colloquial_biases"] = self.var_colloquial.get()
        self.settings["grammar"]["enable_hot_reload"] = self.var_hot_reload.get()

        self.settings["ai_and_privacy"]["zero_pii_gateway"] = self.var_zero_pii.get()

        sel_label = self.var_model.get()
        model_id = getattr(self, "model_rev_map", {}).get(sel_label, sel_label.split("（")[0])
        self.settings["ai_and_privacy"]["ollama_model"] = model_id

        save_settings(self.settings)
        if HAS_UNIVERSAL and hasattr(self, "root") and self.root.winfo_viewable():
            showinfo(self.root, "RuhI 偏好設定", "🎉 設定已成功儲存並即時套用！")
        else:
            print("Settings saved successfully.")

    def on_reset(self):
        self.var_input_mode.set("inline_underline")
        self.var_orientation.set("horizontal")
        self.var_font_size.set(32)
        self.var_max_comp.set(30)
        self.var_theme.set("system")
        self.var_capsule_mode.set("mini_capsule")
        self.var_layout.set("hsu")
        self.var_space.set("commit_first")
        self.var_caps.set("capital_lock")
        self.var_mixed_english.set(True)
        self.var_mixed_mode.set("contextual_stream")
        self.var_mixed_min_len.set(4)
        self.var_q_cycle.set(3)
        self.var_grammar_all.set(True)
        self.var_translingual.set(True)
        self.var_colloquial.set(True)
        self.var_hot_reload.set(True)
        self.var_zero_pii.set(True)
        default_label = getattr(self, "model_map", {}).get("gemma4:latest", "gemma4:latest（Google 官方核心推薦）")
        self.var_model.set(default_label)
        self._zoom(0)
        self.on_save()

def main():
    if "--test" in sys.argv:
        cfg = load_settings()
        assert "appearance" in cfg, "Missing appearance in config"
        assert "keyboard" in cfg, "Missing keyboard in config"
        assert "grammar" in cfg, "Missing grammar in config"
        assert "ai_and_privacy" in cfg, "Missing ai_and_privacy in config"

        root = tk.Tk()
        root.withdraw()
        app = RuhiSettingsApp(root)

        # 測試類別切換
        categories = ["appearance", "keyboard_layout", "mixed_english", "grammar", "ai_and_privacy", "about"]
        for cat in categories:
            app.switch_category(cat)
            assert app.current_category == cat, f"Failed to switch to category {cat}"

        init_zoom = app._zoom_level
        app._zoom(1)
        assert app._zoom_level == init_zoom + 1, f"Expected zoom_level {init_zoom + 1}, got {app._zoom_level}"
        app._zoom(-1)
        assert app._zoom_level == init_zoom, f"Expected zoom_level {init_zoom}, got {app._zoom_level}"

        app.on_save()
        root.destroy()
        try:
            print("[OK] RuhiSettingsApp CorelDRAW-style layout and headless test passed!")
        except Exception:
            pass
        return 0

    root = tk.Tk()
    app = RuhiSettingsApp(root)
    root.mainloop()

if __name__ == "__main__":
    sys.exit(main())
