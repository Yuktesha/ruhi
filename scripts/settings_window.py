#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ruhi_sandbox/settings_window.py
=============================================================================
RuhI（如喜）輸入法 — 現代化偏好設定中心 (RuhI Modern Settings Hub)
=============================================================================
設計理念與架構：
1. 類 CorelDRAW / JetBrains / 現代 OS 階層式雙欄設定視窗：
   - 左側：樹狀分類導覽列（Treeview），可任意拖拉調整寬度，並依字型與縮放動態自適應寬度，絕不截斷字元。
   - 右側：主題專屬設定面板，具備獨立標頭、清晰說明與流式排版。
2. 遵循 rud() 空間接管第一原理與條列項目條件折行：
   - 杜絕超長單行文字溢出與截斷。
   - 選項採用「主標題 + 結構化次說明」雙層條列結構，說明文字支援動態響應式折行 (Responsive Wrapping)。
   - 標頭、群組標題與多選項群組採用流式分層佈局，任何解析度下永不碰撞、永不截斷。
3. 深度深淺色自適應與主題切換：
   - 預設完全跟隨 Windows 系統主題（若系統為暗黑模式即刻呈現極致深色）。
   - 點選「跟隨系統／深色／淺色」時即時全介面響應切換，無需重啟。
   - 支援 Windows DWM 沉浸式深色標題列與個人化強調色 (Accent Color)。
   - 背景即時監聽 Windows 主題變更 (SystemThemeWatcher)。
4. 全介面 Ctrl + +/0/-, Ctrl + 滑鼠滾輪即時縮放，視窗尺寸與側邊欄寬度狀態持久化。
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

# 引入 _lib UniversalUI 與 theme_engine
_HERE = Path(__file__).resolve().parent
BASE_DIR = _HERE if (_HERE / "ruhi-core").exists() or (_HERE / "data").exists() else _HERE.parent
for _cand_lib in [
    _HERE / "_lib",
    _HERE,
    _HERE.parent / "_lib",
    _HERE.parent,
    Path(r"C:\ATGprjs\_lib"),
]:
    if _cand_lib.exists() and str(_cand_lib) not in sys.path:
        sys.path.insert(0, str(_cand_lib))

try:
    from UniversalUI import UniversalApp, showinfo
    HAS_UNIVERSAL = True
except ImportError:
    HAS_UNIVERSAL = False
    UniversalApp = object

try:
    from theme_engine import (
        get_system_theme,
        get_system_accent_color,
        get_palette,
        apply_title_bar_theme,
        SystemThemeWatcher,
        ThemeAuditor
    )
    HAS_THEME_ENGINE = True
except ImportError:
    HAS_THEME_ENGINE = False
    def get_system_theme():
        return "light"
    def get_system_accent_color(default="#007acc"):
        return default
    def get_palette(is_dark=None, follow_system_accent=True):
        if is_dark:
            return {
                "name": "Dark", "is_dark": True,
                "bg_main": "#1e1e1e", "bg_surface": "#252526", "bg_input": "#2d2d30",
                "bg_header": "#252526", "bg_hover": "#2c313a", "bg_active": "#3a3f4b",
                "fg_main": "#d4d4d4", "fg_bright": "#ffffff", "fg_muted": "#9da5b4",
                "accent": "#0078d4", "accent_hover": "#1084d8", "border": "#3e3e42",
                "stripe_alt": "#222222", "highlight": "#094771",
                "menu_bg": "#252526", "menu_fg": "#d4d4d4",
                "menu_active_bg": "#2c313a", "menu_active_fg": "#ffffff"
            }
        else:
            return {
                "name": "Light", "is_dark": False,
                "bg_main": "#f8f9fa", "bg_surface": "#ffffff", "bg_input": "#ffffff",
                "bg_header": "#e9ecef", "bg_hover": "#e2e6ea", "bg_active": "#dae0e5",
                "fg_main": "#212529", "fg_bright": "#000000", "fg_muted": "#6c757d",
                "accent": "#0d6efd", "accent_hover": "#0b5ed7", "border": "#dee2e6",
                "stripe_alt": "#f1f3f5", "highlight": "#cfe2ff",
                "menu_bg": "#ffffff", "menu_fg": "#212529",
                "menu_active_bg": "#e9ecef", "menu_active_fg": "#000000"
            }
    def apply_title_bar_theme(window, dark=True):
        pass
    class SystemThemeWatcher:
        def __init__(self, root, callback):
            pass
        def stop(self):
            pass
    class ThemeAuditor:
        @staticmethod
        def audit_and_apply(root, is_dark=True, palette=None):
            pass

CONFIG_PATH = Path(__file__).resolve().parent / "settings_config.json"
SANDBOX_CONFIG_PATH = Path(__file__).resolve().parent / "config.json"
DATA_DIR = Path(__file__).resolve().parent / "data"

DEFAULT_SETTINGS = {
    "appearance": {
        "input_display_mode": "smart_auto",
        "orientation": "horizontal",
        "font_size": 32,
        "max_composition_chars": 30,
        "theme": "system",
        "show_bopomofo": True,
        "capsule_mode": "mini_capsule",
        "zoom_level": 0,
        "geometry": "980x720",
        "sidebar_width": 260
    },
    "keyboard": {
        "layout": "hsu",
        "enabled_keyboard_modes": ["hsu", "japanese"],
        "space_behavior": "commit_first",
        "capslock_switch": "capital_lock",
        "candidate_keys": "123456789",
        "backspace_mode": "component",
        "enable_mixed_english": True,
        "mixed_english_mode": "contextual_stream",
        "mixed_english_min_len": 4,
        "punctuation_commit_mode": "A",
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
        sdata["enabled_keyboard_modes"] = cfg.get("keyboard", {}).get("enabled_keyboard_modes", ["hsu", "japanese"])
        sdata["punctuation_commit_mode"] = cfg.get("keyboard", {}).get("punctuation_commit_mode", "A")
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
    if not values:
        return 24
    max_w = 0
    for v in values:
        w = sum(2 if ord(c) > 127 else 1 for c in v)
        if w > max_w:
            max_w = w
    return max(20, int(max_w * 0.62) + 4)

class AutoScrollbar(ttk.Scrollbar):
    def set(self, lo, hi):
        if float(lo) <= 0.0 and float(hi) >= 1.0:
            self.pack_forget()
        else:
            if not self.winfo_ismapped():
                self.pack(side=tk.BOTTOM, fill=tk.X)
        super().set(lo, hi)

class RuhiSettingsApp(UniversalApp if HAS_UNIVERSAL else object):
    def __init__(self, root):
        self.settings = load_settings()
        self._zoom_level = self.settings.get("appearance", {}).get("zoom_level", 0)
        self._hide_zoom_timer = None
        self.current_category = None
        self.pages = {}
        self.is_dark = False
        self.current_palette = {}
        self.sys_theme_watcher = None
        self._wrap_labels = []

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

        # 套用設定儲存之色彩主題（或自適應 Windows 系統深淺色）
        initial_theme = self.settings.get("appearance", {}).get("theme", "system")
        self.apply_theme(initial_theme)

        # 啟動系統布景主題即時監聽器
        self.sys_theme_watcher = SystemThemeWatcher(self.root, self._on_system_theme_changed)

        # 套用儲存之側邊欄寬度與自適應寬度
        self.root.after_idle(self.update_treeview_width)

        if self._zoom_level != 0:
            self._apply_zoom_fonts()

    def on_close(self):
        try:
            self.settings.setdefault("appearance", {})["geometry"] = self.root.geometry()
            self.settings.setdefault("appearance", {})["zoom_level"] = self._zoom_level
            if hasattr(self, "paned") and self.paned:
                try:
                    sash_w = self.paned.sashpos(0)
                    if sash_w > 80:
                        self.settings.setdefault("appearance", {})["sidebar_width"] = sash_w
                except Exception:
                    pass
            save_settings(self.settings)
        except Exception:
            pass

        if self.sys_theme_watcher:
            try:
                self.sys_theme_watcher.stop()
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
        self.style.theme_use('clam')

    def register_wrap_label(self, label: ttk.Label):
        """註冊需要動態自適應條件折行之說明標籤"""
        self._wrap_labels.append(label)

    def create_item_subtext(self, parent, text: str) -> ttk.Label:
        """輔助工廠：建立符合 rud() 縮排規格之說明標籤，支援動態條件折行"""
        lbl = ttk.Label(parent, text=text, font=self.font_subtitle)
        lbl.pack(anchor=tk.W, padx=(24, 0), pady=(0, 6))
        self.register_wrap_label(lbl)
        return lbl

    def _on_content_resize(self, event):
        """當右側內容面板寬度變更時，即時重算所有條列項目說明文字的折行寬度 (rud 動態自適應)"""
        available_w = max(260, event.width - 48)
        for lbl in self._wrap_labels:
            try:
                lbl.configure(wraplength=available_w)
            except Exception:
                pass

    def apply_theme(self, theme_mode: str = None):
        """核心動態主題引擎：全介面深度套用深色／淺色主題與 Windows 系統強調色"""
        if theme_mode is None:
            theme_mode = self.var_theme.get() if hasattr(self, "var_theme") else self.settings.get("appearance", {}).get("theme", "system")

        if theme_mode == "system":
            is_dark = (get_system_theme() == "dark")
        else:
            is_dark = (theme_mode == "dark")

        self.is_dark = is_dark
        p = get_palette(is_dark=is_dark, follow_system_accent=True)
        self.current_palette = p

        # 1. Windows DWM 沉浸式深色標題列
        apply_title_bar_theme(self.root, dark=is_dark)

        # 2. 視窗根背景
        self.root.configure(bg=p["bg_main"])

        # 3. ttk 全域元件樣式套用
        self.style.configure(".", background=p["bg_main"], foreground=p["fg_main"], font=self.font_body)
        self.style.configure("TFrame", background=p["bg_main"])
        self.style.configure("Sidebar.TFrame", background=p["bg_surface"])
        self.style.configure("Content.TFrame", background=p["bg_main"])

        self.style.configure("TLabel", background=p["bg_main"], foreground=p["fg_main"], font=self.font_body)
        self.style.configure("TLabelframe", background=p["bg_main"], bordercolor=p["border"], borderwidth=1)
        self.style.configure("TLabelframe.Label", background=p["bg_main"], foreground=p["accent"], font=self.font_card_header)

        self.style.configure("TRadiobutton", background=p["bg_main"], foreground=p["fg_main"], font=self.font_body)
        self.style.map("TRadiobutton",
            background=[('active', p["bg_main"])],
            foreground=[('active', p["fg_bright"])],
            indicatorbackground=[('selected', p["accent"]), ('!selected', p["bg_input"])],
            indicatorcolor=[('selected', p["accent"])]
        )

        self.style.configure("TCheckbutton", background=p["bg_main"], foreground=p["fg_main"], font=self.font_body)
        self.style.map("TCheckbutton",
            background=[('active', p["bg_main"])],
            foreground=[('active', p["fg_bright"])],
            indicatorbackground=[('selected', p["accent"]), ('!selected', p["bg_input"])],
            indicatorcolor=[('selected', p["accent"])]
        )

        self.style.configure("TButton", background=p["bg_input"], foreground=p["fg_bright"],
                             bordercolor=p["border"], borderwidth=1, font=self.font_button, padding=[6, 4])
        self.style.map("TButton",
            background=[('pressed', p["accent_hover"]), ('active', p["accent"])],
            foreground=[('active', '#ffffff'), ('pressed', '#ffffff')]
        )

        delta = self._zoom_level * 2
        row_h = max(24, int(32 + delta * 1.5))
        self.style.configure("Nav.Treeview",
            background=p["bg_surface"],
            foreground=p["fg_main"],
            fieldfieldbackground=p["bg_surface"],
            fieldbackground=p["bg_surface"],
            borderwidth=0,
            font=self.font_tree,
            rowheight=row_h
        )
        self.style.map("Nav.Treeview",
            background=[("selected", p["accent"])],
            foreground=[("selected", "#ffffff")]
        )

        self.style.configure("TCombobox", background=p["bg_input"], fieldbackground=p["bg_input"],
                             foreground=p["fg_bright"], arrowcolor=p["fg_bright"], font=self.font_body)
        self.style.map("TCombobox",
            fieldbackground=[('readonly', p["bg_input"])],
            selectbackground=[('readonly', p["accent"])],
            selectforeground=[('readonly', '#ffffff')]
        )

        self.style.configure("TScale", background=p["bg_main"], troughcolor=p["bg_input"])
        self.style.configure("TScrollbar", background=p["bg_input"], troughcolor=p["bg_main"], borderwidth=0)
        self.style.configure("TSeparator", background=p["border"])
        self.style.configure("TPanedwindow", background=p["bg_main"])
        self.style.configure("Sash", sashthickness=6, background=p["border"])

        # 4. 個別重點元件顏色重新渲染
        if hasattr(self, "canvas") and self.canvas:
            self.canvas.configure(bg=p["bg_main"])
        if hasattr(self, "page_container") and self.page_container:
            self.page_container.configure(style="TFrame")

        if hasattr(self, "lbl_title"):
            self.lbl_title.configure(foreground=p["fg_bright"])
            self.lbl_meaning.configure(foreground=p["fg_muted"])
            self.lbl_subtitle.configure(foreground=p["fg_muted"])
            self.lbl_zoom_indicator.configure(foreground=p["accent"])
            self.lbl_zoom_hint.configure(foreground=p["fg_muted"])
            self.lbl_page_title.configure(foreground=p["fg_bright"])
            self.lbl_page_desc.configure(foreground=p["fg_muted"])
            self.lbl_nav_title.configure(foreground=p["accent"])
            self.lbl_nav_hint.configure(foreground=p["fg_muted"])

        if hasattr(self, "lbl_brand_quote"):
            self.lbl_brand_quote.configure(foreground=p["accent"])
        if hasattr(self, "lbl_vision_roadmap"):
            self.lbl_vision_roadmap.configure(foreground=p["fg_main"])
        if hasattr(self, "lbl_sys_path"):
            self.lbl_sys_path.configure(foreground=p["fg_muted"])

        # 針對所有註冊之折行標籤更新前景色為柔和次要色 (fg_muted)
        for lbl in self._wrap_labels:
            try:
                lbl.configure(foreground=p["fg_muted"])
            except Exception:
                pass

        self.root.option_add("*TCombobox*Listbox.background", p["bg_surface"])
        self.root.option_add("*TCombobox*Listbox.foreground", p["fg_main"])
        self.root.option_add("*TCombobox*Listbox.selectBackground", p["accent"])
        self.root.option_add("*TCombobox*Listbox.selectForeground", "#ffffff")
        self.root.option_add("*Combobox*Listbox.background", p["bg_surface"])
        self.root.option_add("*Combobox*Listbox.foreground", p["fg_main"])
        self.root.option_add("*Combobox*Listbox.selectBackground", p["accent"])
        self.root.option_add("*Combobox*Listbox.selectForeground", "#ffffff")

        ThemeAuditor.audit_and_apply(self.root, is_dark=is_dark, palette=p)

    def on_theme_radio_changed(self):
        mode = self.var_theme.get()
        self.settings.setdefault("appearance", {})["theme"] = mode
        self.apply_theme(mode)

    def _on_system_theme_changed(self, is_dark, accent_color):
        if hasattr(self, "var_theme") and self.var_theme.get() == "system":
            self.apply_theme("system")

    def calc_treeview_width(self) -> int:
        all_texts = [
            " 🎨  外觀與候選座",
            " ⌨️  鍵盤與輸入習慣",
            "   ⌨  鍵盤配置與常用鍵",
            "   🔤  許氏＋歐文混打",
            " 🧠  語意結構與語法引力",
            " 🛡️  隱私安全與 AI 模型",
            " ℹ️  關於與全語言願景"
        ]
        max_text_w = 0
        for t in all_texts:
            w = self.font_tree.measure(t)
            if w > max_text_w:
                max_text_w = w
        return max(240, max_text_w + 56)

    def update_treeview_width(self):
        calc_w = self.calc_treeview_width()
        saved_w = self.settings.get("appearance", {}).get("sidebar_width")
        target_w = max(calc_w, saved_w) if (saved_w and isinstance(saved_w, (int, float))) else calc_w

        if hasattr(self, "nav_tree"):
            self.nav_tree.column("#0", width=target_w, minwidth=180, stretch=True)

        if hasattr(self, "paned") and self.paned:
            try:
                self.root.update_idletasks()
                self.paned.sashpos(0, target_w)
            except Exception:
                pass

    def setup_ui(self):
        main_frame = ttk.Frame(self.root, padding=(16, 12, 16, 12))
        main_frame.pack(fill=tk.BOTH, expand=True)

        # 頂部全域 Header (遵循 rud() 兩層式流動佈局，防止單行擠壓碰撞)
        header = ttk.Frame(main_frame)
        header.pack(side=tk.TOP, fill=tk.X, pady=(0, 8))

        row_header_top = ttk.Frame(header)
        row_header_top.pack(fill=tk.X)

        self.lbl_title = ttk.Label(row_header_top, text="🌟 RuhI（如喜）偏好設定中心", font=self.font_title)
        self.lbl_title.pack(side=tk.LEFT)

        self.lbl_zoom_indicator = ttk.Label(row_header_top, text="", font=self.font_bold)
        self.lbl_zoom_indicator.pack(side=tk.LEFT, padx=12)

        self.lbl_subtitle = ttk.Label(row_header_top, text="基於 rud() 詞塊流式定律與八大語法架構", font=self.font_subtitle)
        self.lbl_subtitle.pack(side=tk.RIGHT)

        row_header_sub = ttk.Frame(header)
        row_header_sub.pack(fill=tk.X, pady=(2, 0))

        self.lbl_meaning = ttk.Label(row_header_sub,
            text="「如雲行水流自在歡喜」— Ruh 承襲自 RuhOS 精神哲學，I 兼具「輸入 (Input)」與「智慧 (Intelligence)」雙重意涵",
            font=self.font_subtitle)
        self.lbl_meaning.pack(anchor=tk.W)

        # 底部動作列 (沉底橫向延伸)
        bottom_bar = ttk.Frame(main_frame, padding=(0, 10, 0, 0))
        bottom_bar.pack(side=tk.BOTTOM, fill=tk.X)

        self.lbl_zoom_hint = ttk.Label(bottom_bar, text="💡 提示：按 Ctrl + 滑鼠滾輪 或 Ctrl + +/- / 0 可自由放大縮小介面",
                                       font=self.font_subtitle)
        self.lbl_zoom_hint.pack(side=tk.LEFT)

        btn_save = ttk.Button(bottom_bar, text="💾 儲存並即時套用", command=self.on_save)
        btn_save.pack(side=tk.RIGHT, padx=5)

        btn_reset = ttk.Button(bottom_bar, text="🔄 還原最佳預設值", command=self.on_reset)
        btn_reset.pack(side=tk.RIGHT, padx=5)

        # 中間主體：採用 ttk.PanedWindow 實現可任意調整、自適應寬度的雙欄佈局 (類 CorelDRAW)
        body_frame = ttk.Frame(main_frame)
        body_frame.pack(side=tk.TOP, fill=tk.BOTH, expand=True)

        self.paned = ttk.PanedWindow(body_frame, orient=tk.HORIZONTAL)
        self.paned.pack(fill=tk.BOTH, expand=True)

        # 1. 左側樹狀導覽欄 (Sidebar Frame in PanedWindow)
        calc_w = self.calc_treeview_width()
        saved_w = self.settings.get("appearance", {}).get("sidebar_width")
        initial_sidebar_w = max(calc_w, saved_w) if (saved_w and isinstance(saved_w, (int, float))) else calc_w

        self.sidebar_frame = ttk.Frame(self.paned, width=initial_sidebar_w)
        self.paned.add(self.sidebar_frame, weight=0)

        # 標題精簡化，搭配獨立提示行，杜絕過長截斷
        self.lbl_nav_title = ttk.Label(self.sidebar_frame, text="設定分類導覽", font=self.font_card_header)
        self.lbl_nav_title.pack(anchor=tk.W, pady=(0, 1), padx=4)

        self.lbl_nav_hint = ttk.Label(self.sidebar_frame, text="↔ 可拖曳右緣分割線調整欄寬", font=self.font_subtitle)
        self.lbl_nav_hint.pack(anchor=tk.W, pady=(0, 6), padx=4)

        tree_container = ttk.Frame(self.sidebar_frame)
        tree_container.pack(fill=tk.BOTH, expand=True)

        self.nav_tree = ttk.Treeview(tree_container, style="Nav.Treeview", show="tree", selectmode="browse")
        xscroll = AutoScrollbar(tree_container, orient=tk.HORIZONTAL, command=self.nav_tree.xview)
        self.nav_tree.configure(xscrollcommand=xscroll.set)
        self.nav_tree.pack(side=tk.TOP, fill=tk.BOTH, expand=True)

        self.nav_tree.insert("", "end", iid="appearance", text=" 🎨  外觀與候選座", open=True)

        kb_root = self.nav_tree.insert("", "end", iid="keyboard_group", text=" ⌨️  鍵盤與輸入習慣", open=True)
        self.nav_tree.insert(kb_root, "end", iid="keyboard_layout", text="   ⌨  鍵盤配置與常用鍵")
        self.nav_tree.insert(kb_root, "end", iid="mixed_english", text="   🔤  許氏＋歐文混打")

        self.nav_tree.insert("", "end", iid="grammar", text=" 🧠  語意結構與語法引力", open=True)
        self.nav_tree.insert("", "end", iid="ai_and_privacy", text=" 🛡️  隱私安全與 AI 模型", open=True)
        self.nav_tree.insert("", "end", iid="about", text=" ℹ️  關於與全語言願景", open=True)

        self.nav_tree.bind("<<TreeviewSelect>>", self.on_tree_select)

        # 2. 右側內容容器 (Content Panel in PanedWindow)
        content_panel = ttk.Frame(self.paned, padding=(8, 0, 0, 0))
        self.paned.add(content_panel, weight=1)

        self.page_header = ttk.Frame(content_panel)
        self.page_header.pack(side=tk.TOP, fill=tk.X, pady=(0, 6))

        self.lbl_page_title = ttk.Label(self.page_header, text="", font=self.font_page_title)
        self.lbl_page_title.pack(anchor=tk.W)

        self.lbl_page_desc = ttk.Label(self.page_header, text="", font=self.font_page_desc)
        self.lbl_page_desc.pack(anchor=tk.W, pady=(2, 6))
        self.register_wrap_label(self.lbl_page_desc)

        sep_header = ttk.Separator(self.page_header, orient=tk.HORIZONTAL)
        sep_header.pack(fill=tk.X)

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
            self._on_content_resize(event)
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
        if cat_id not in self.pages:
            return
        self.current_category = cat_id

        if self.nav_tree.selection() != (cat_id,):
            self.nav_tree.selection_set(cat_id)
            self.nav_tree.see(cat_id)

        for pid, page in self.pages.items():
            page.pack_forget()

        self.pages[cat_id].pack(fill=tk.BOTH, expand=True, padx=4, pady=4)

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

        self.root.update_idletasks()
        self.canvas.configure(scrollregion=self.canvas.bbox("all"))
        self.canvas.yview_moveto(0)

    def switch_category(self, cat_id: str):
        self.select_category(cat_id)

    # ── 頁面一：外觀與候選座 ──
    def build_appearance_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        # 輸入操作模式（結構化條列：主選項 + 縮排說明）
        card_mode = ttk.LabelFrame(page, text=" 輸入操作模式（微軟新注音 vs 如喜輸入座） ", padding=12)
        card_mode.pack(fill=tk.X, pady=(0, 10))

        self.var_input_mode = tk.StringVar(value=self.settings.get("appearance", {}).get("input_display_mode", "smart_auto"))

        ttk.Radiobutton(card_mode, text="自動判斷 APP 切換最佳輸入模式（預設推薦）", variable=self.var_input_mode, value="smart_auto").pack(anchor=tk.W)
        self.create_item_subtext(card_mode, "日常軟體（記事本、瀏覽器、通訊軟體）優先使用獨立如喜輸入座；遇專業排版軟體（LibreOffice Writer 等）或傳統主控台 (CMD) 時，自動無感切換為微軟新注音模式，兼顧美觀與極致相容！")

        ttk.Radiobutton(card_mode, text="如喜輸入座：游標跟隨（自然經典風格・ㄅ）", variable=self.var_input_mode, value="floating_follow").pack(anchor=tk.W)
        self.create_item_subtext(card_mode, "所有應用程式皆以獨立懸浮輸入座緊隨游標下方；若遇到未向系統回報空行幾何資訊之跨平台排版軟體，可能出現空行位置落後現象。")

        ttk.Radiobutton(card_mode, text="如喜輸入座：螢幕固定（自然經典風格・ㄆ）", variable=self.var_input_mode, value="floating_fixed").pack(anchor=tk.W)
        self.create_item_subtext(card_mode, "輸入座保持在固定螢幕座標，使用者可隨時用滑鼠自由拖曳至螢幕任意最佳位置，完全不受任何 APP 游標更新異常影響。")

        ttk.Radiobutton(card_mode, text="微軟新注音游標跟隨模式（極致相容）", variable=self.var_input_mode, value="inline_underline").pack(anchor=tk.W)
        self.create_item_subtext(card_mode, "100% 遵從 Windows TSF 原生行內組字規範，直接於目標視窗之文字輸入游標處繪製底線與拼弦，選字時彈出微軟新注音風格輕巧候選浮窗，相容性最高、排版工作首選。")

        # 候選座排列與字體尺寸
        card_layout = ttk.LabelFrame(page, text=" 候選座排列與字體尺寸 ", padding=12)
        card_layout.pack(fill=tk.X, pady=(0, 10))

        # 排列方向（Prompt + 結構化條列）
        ttk.Label(card_layout, text="候選窗排列方向：", font=self.font_bold).pack(anchor=tk.W, pady=(2, 4))
        self.var_orientation = tk.StringVar(value=self.settings["appearance"]["orientation"])
        ttk.Radiobutton(card_layout, text="橫向流式排列（預設推薦，視線左右平移最自然）", variable=self.var_orientation, value="horizontal").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="直向傳統排列（經典直書自上而下列出）", variable=self.var_orientation, value="vertical").pack(anchor=tk.W, padx=(12, 0), pady=2)

        # 候選字體大小
        row2 = ttk.Frame(card_layout)
        row2.pack(fill=tk.X, pady=(8, 4))
        ttk.Label(row2, text="候選字體大小：").pack(side=tk.LEFT, padx=(0, 8))
        self.var_font_size = tk.IntVar(value=self.settings["appearance"]["font_size"])
        lbl_size = ttk.Label(row2, text=f"{self.var_font_size.get()} px", width=8)
        scale = ttk.Scale(row2, from_=18, to=54, variable=self.var_font_size, orient=tk.HORIZONTAL,
                          command=lambda v: lbl_size.config(text=f"{int(float(v))} px"))
        scale.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=8)
        lbl_size.pack(side=tk.LEFT)

        # 輸入座最大字數
        row_len = ttk.Frame(card_layout)
        row_len.pack(fill=tk.X, pady=(6, 2))
        ttk.Label(row_len, text="輸入座最大字數：").pack(side=tk.LEFT, padx=(0, 8))
        self.var_max_comp = tk.IntVar(value=self.settings.get("appearance", {}).get("max_composition_chars", 30))
        lbl_len = ttk.Label(row_len, text=f"{self.var_max_comp.get()} 字", width=8)
        scale_len = ttk.Scale(row_len, from_=15, to=60, variable=self.var_max_comp, orient=tk.HORIZONTAL,
                              command=lambda v: lbl_len.config(text=f"{int(float(v))} 字"))
        scale_len.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=8)
        lbl_len.pack(side=tk.LEFT)

        self.create_item_subtext(card_layout, "註：輸入長度超過最大字數時，系統將自動送出首個標點符號前之文字。")

        # 色彩主題與狀態指示
        card_theme = ttk.LabelFrame(page, text=" 色彩主題與狀態指示 ", padding=12)
        card_theme.pack(fill=tk.X, pady=(0, 10))

        ttk.Label(card_theme, text="色彩主題風格：", font=self.font_bold).pack(anchor=tk.W, pady=(2, 4))
        self.var_theme = tk.StringVar(value=self.settings["appearance"]["theme"])
        ttk.Radiobutton(card_theme, text="跟隨 Windows 系統（自動即時匹配深淺色與系統個人化強調色）",
                        variable=self.var_theme, value="system", command=self.on_theme_radio_changed).pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_theme, text="極致深色 (Dark)",
                        variable=self.var_theme, value="dark", command=self.on_theme_radio_changed).pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_theme, text="宣紙淺色 (Light)",
                        variable=self.var_theme, value="light", command=self.on_theme_radio_changed).pack(anchor=tk.W, padx=(12, 0), pady=2)

        ttk.Label(card_theme, text="送出字串後狀態指示行為：", font=self.font_bold).pack(anchor=tk.W, pady=(8, 4))
        self.var_capsule_mode = tk.StringVar(value=self.settings.get("appearance", {}).get("capsule_mode", "mini_capsule"))
        ttk.Radiobutton(card_theme, text="縮為常駐迷你狀態膠囊（隨時可點齒輪 ⚙ 開啟設定，狀態清晰）",
                        variable=self.var_capsule_mode, value="mini_capsule").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_theme, text="完全隱藏輸入座（純淨極簡模式，由工作列系統匣圖示指示狀態）",
                        variable=self.var_capsule_mode, value="auto_hide").pack(anchor=tk.W, padx=(12, 0), pady=2)

        return page

    # ── 頁面二：鍵盤配置與常用鍵 ──
    def build_keyboard_layout_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_layout = ttk.LabelFrame(page, text=" 鍵盤配置與常用鍵設定 ", padding=12)
        card_layout.pack(fill=tk.X, pady=(0, 10))

        ttk.Label(card_layout, text="當前預設亞洲輸入模式：", font=self.font_bold).pack(anchor=tk.W, pady=(2, 4))
        self.var_layout = tk.StringVar(value=self.settings["keyboard"]["layout"])
        ttk.Radiobutton(card_layout, text="許氏雙拼注音（Hsu Layout，盲打極致流暢首選 - 許）",
                        variable=self.var_layout, value="hsu").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="日文羅馬字輸入（Japanese Romaji，五十音／漢字／片假名輕量轉換 - 日）",
                        variable=self.var_layout, value="japanese").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="倉頡輸入法（Cangjie，支援 Z 鍵萬用字元查詢 - 倉）",
                        variable=self.var_layout, value="cangjie").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="標準大千注音（Standard Daikien - 注）",
                        variable=self.var_layout, value="standard").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="倚天 26 鍵注音（ETen 26-Key - 倚）",
                        variable=self.var_layout, value="et26").pack(anchor=tk.W, padx=(12, 0), pady=2)

        ttk.Label(card_layout, text="參與快速切換迴圈之亞洲模式（Ctrl / Shift + Caps Lock ＆ 狀態標籤選單）：", font=self.font_bold).pack(anchor=tk.W, pady=(10, 4))
        self.create_item_subtext(card_layout, "勾選您日常使用的亞洲輸入模式。未勾選的注音或形碼模式將完全排除於快捷鍵循環與選單之外（例如僅勾選「許氏雙拼」與「日文羅馬字」，即可一鍵在許／日之間極速往返）。")

        enabled_list = self.settings["keyboard"].get("enabled_keyboard_modes", ["hsu", "japanese"])
        self.vars_enabled_modes = {}
        mode_options = [
            ("hsu", "許氏雙拼注音 (許)"),
            ("japanese", "日文羅馬字輸入 (日)"),
            ("cangjie", "倉頡輸入法 (倉)"),
            ("standard", "標準大千注音 (注)"),
            ("et26", "倚天 26 鍵注音 (倚)"),
        ]
        chk_frame = ttk.Frame(card_layout)
        chk_frame.pack(fill=tk.X, padx=(12, 0), pady=(2, 6))
        for m_code, m_label in mode_options:
            bv = tk.BooleanVar(value=(m_code in enabled_list))
            self.vars_enabled_modes[m_code] = bv
            ttk.Checkbutton(chk_frame, text=m_label, variable=bv).pack(anchor=tk.W, pady=1)

        ttk.Label(card_layout, text="空白鍵行為：", font=self.font_bold).pack(anchor=tk.W, pady=(8, 4))
        self.var_space = tk.StringVar(value=self.settings["keyboard"]["space_behavior"])
        ttk.Radiobutton(card_layout, text="出第一候選字（如喜智慧預設，流暢選詞）",
                        variable=self.var_space, value="commit_first").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="僅輸出半形空格",
                        variable=self.var_space, value="space_only").pack(anchor=tk.W, padx=(12, 0), pady=2)

        ttk.Label(card_layout, text="Caps Lock 行為：", font=self.font_bold).pack(anchor=tk.W, pady=(8, 4))
        self.var_caps = tk.StringVar(value=self.settings["keyboard"]["capslock_switch"])
        ttk.Radiobutton(card_layout, text="快速切換中／英數模式",
                        variable=self.var_caps, value="chinese_english").pack(anchor=tk.W, padx=(12, 0), pady=2)
        ttk.Radiobutton(card_layout, text="大寫鎖定 (Capital Lock)",
                        variable=self.var_caps, value="capital_lock").pack(anchor=tk.W, padx=(12, 0), pady=2)

        # 標點符號結算行為（Prompt + 結構化條列說明）
        card_punct = ttk.LabelFrame(page, text=" 標點符號結算行為（微軟新注音 vs 自然長句風格） ", padding=12)
        card_punct.pack(fill=tk.X, pady=(0, 10))

        ttk.Label(card_punct, text="標點鍵入送出模式：", font=self.font_bold).pack(anchor=tk.W, pady=(2, 4))
        self.var_punct_mode = tk.StringVar(value=self.settings.get("keyboard", {}).get("punctuation_commit_mode", "A"))

        ttk.Radiobutton(card_punct, text="經典即時送出（微軟新注音風格・預設推薦）",
                        variable=self.var_punct_mode, value="A").pack(anchor=tk.W, padx=(12, 0), pady=2)
        self.create_item_subtext(card_punct, "鍵入任何標點（「，」、「。」、「、」等），整句連同標點立刻結算送出上屏。系統負擔最小、相容性最高、各類 APP 零排版延遲。")

        ttk.Radiobutton(card_punct, text="智慧句末送出（自然長句折衷風格）",
                        variable=self.var_punct_mode, value="B").pack(anchor=tk.W, padx=(12, 0), pady=2)
        self.create_item_subtext(card_punct, "逗號「，」、頓號「、」保留在輸入座維持長句；打到句末標點（「。」、「？」、「！」等）自動結算送出。兼顧長句打字連續性與段落自然閉合。")

        ttk.Radiobutton(card_punct, text="全手動長句模式（純 Enter 送出）",
                        variable=self.var_punct_mode, value="C").pack(anchor=tk.W, padx=(12, 0), pady=2)
        self.create_item_subtext(card_punct, "任何標點（包括句號）完全保留在輸入座內，絕不自動送出，一律由使用者確認無誤後自行按下 Enter 鍵統一結算。適合全文通盤檢視校對。")

        card_q = ttk.LabelFrame(page, text=" Q 鍵快速候選輪換 ", padding=12)
        card_q.pack(fill=tk.X, pady=(0, 10))

        row_q = ttk.Frame(card_q)
        row_q.pack(fill=tk.X, pady=4)
        ttk.Label(row_q, text="Q鍵快速候選輪換次數上限：").pack(side=tk.LEFT, padx=(0, 8))
        self.var_q_cycle = tk.IntVar(value=self.settings.get("keyboard", {}).get("q_cycle_max", 3))
        lbl_q = ttk.Label(row_q, text=f"{self.var_q_cycle.get()} 次", width=8)
        scale_q = ttk.Scale(row_q, from_=1, to=8, variable=self.var_q_cycle, orient=tk.HORIZONTAL,
                            command=lambda v: lbl_q.config(text=f"{int(float(v))} 次"))
        scale_q.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=8)
        lbl_q.pack(side=tk.LEFT)

        self.create_item_subtext(card_q, "說明：在許氏鍵盤中，單擊 Q 鍵可在首選詞與次選詞間快速切換，連續按滿指定次數自動展開完整選字座。")

        return page

    # ── 頁面三：許氏＋歐文混打策略 ──
    def build_mixed_english_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_mix = ttk.LabelFrame(page, text=" 許氏＋歐文智慧混打核心機制 ", padding=12)
        card_mix.pack(fill=tk.X, pady=(0, 10))

        self.var_mixed_english = tk.BooleanVar(value=self.settings.get("keyboard", {}).get("enable_mixed_english", True))
        ttk.Checkbutton(card_mix, text="啟用許氏＋歐文智慧混打", variable=self.var_mixed_english).pack(anchor=tk.W, pady=2)
        self.create_item_subtext(card_mix, "非合法許氏注音組合直接當作歐文，Enter 直接上屏；無需切換輸入法即可自然敲出英文單字與程式碼。")

        card_strategy = ttk.LabelFrame(page, text=" 混打仲裁策略與門檻 ", padding=12)
        card_strategy.pack(fill=tk.X, pady=(0, 10))

        ttk.Label(card_strategy, text="混打仲裁策略：", font=self.font_bold).pack(anchor=tk.W, pady=(2, 4))
        self.var_mixed_mode = tk.StringVar(value=self.settings.get("keyboard", {}).get("mixed_english_mode", "contextual_stream"))

        ttk.Radiobutton(card_strategy, text="語境串流模式（推薦）", variable=self.var_mixed_mode, value="contextual_stream").pack(anchor=tk.W, padx=(12, 0))
        self.create_item_subtext(card_strategy, "首詞確認為歐文時開啟串流，放行後續 2~3 字詞；中文模式享有 100% 語境防護，杜絕非預期覆寫。")

        ttk.Radiobutton(card_strategy, text="保守模式", variable=self.var_mixed_mode, value="conservative").pack(anchor=tk.W, padx=(12, 0))
        self.create_item_subtext(card_strategy, "僅在大寫字母、駝峰命名 (CamelCase) 或特殊標識符時自動放行歐文。")

        ttk.Radiobutton(card_strategy, text="長詞門檻模式", variable=self.var_mixed_mode, value="min_length_only").pack(anchor=tk.W, padx=(12, 0))
        self.create_item_subtext(card_strategy, "連續未匹配字元長度達到門檻長度時，方強制轉為歐文串流。")

        row_mix_len = ttk.Frame(card_strategy)
        row_mix_len.pack(fill=tk.X, pady=(8, 2))
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
        ttk.Checkbutton(card_rules, text="啟用現代漢語八大語法引擎", variable=self.var_grammar_all).pack(anchor=tk.W, pady=2)
        self.create_item_subtext(card_rules, "涵蓋離合詞、動補結構、被動式、框式介詞、兼語連動、存現重疊等 154 種句法拓撲引力。")

        self.var_translingual = tk.BooleanVar(value=self.settings["grammar"]["enable_de_vs_de_translingual"])
        ttk.Checkbutton(card_rules, text="跨語言結構助詞深層語意分野律", variable=self.var_translingual).pack(anchor=tk.W, pady=2)
        self.create_item_subtext(card_rules, "自動精準辨析閩客臺華跨語言『的』(ê) 與『得』(kah) 結構助詞的深層語法功能。")

        self.var_colloquial = tk.BooleanVar(value=self.settings["grammar"]["enable_colloquial_biases"])
        ttk.Checkbutton(card_rules, text="生活高頻口語引力校準", variable=self.var_colloquial).pack(anchor=tk.W, pady=2)
        self.create_item_subtext(card_rules, "校正破音字先驗分佈，杜絕『自燃』、『渴論』、『醮我一生』等罕見假同音字語意劫掠。")

        card_hot = ttk.LabelFrame(page, text=" 詞庫主權、熱重載與語意調校工作室 ", padding=12)
        card_hot.pack(fill=tk.X, pady=(0, 10))

        self.var_hot_reload = tk.BooleanVar(value=self.settings["grammar"]["enable_hot_reload"])
        ttk.Checkbutton(card_hot, text="使用者自建詞庫熱重載", variable=self.var_hot_reload).pack(anchor=tk.W, pady=2)
        self.create_item_subtext(card_hot, "支援 Ctrl+2 快捷加詞與 TSV 檔案即時熱載入，修改後無須重啟輸入法。")

        self.create_item_subtext(card_hot, "架構鐵律守則：如喜堅守計算語言學語法構式與二階轉移引力，嚴禁單字權重打地鼠，再也不必望穿秋水等候一個可能永遠不會進步的官方詞庫。")

        row_tuner_btn = ttk.Frame(card_hot)
        row_tuner_btn.pack(fill=tk.X, pady=(8, 2))
        ttk.Button(row_tuner_btn, text="🛠️ 啟動語意調校工作室 (Ctrl+Alt+R)", command=self.open_semantic_tuner).pack(side=tk.LEFT, padx=(0, 8))

        return page

    # ── 頁面五：隱私安全與 AI 模型 ──
    def build_ai_privacy_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_privacy = ttk.LabelFrame(page, text=" Zero-PII 隱私安全閘門 ", padding=12)
        card_privacy.pack(fill=tk.X, pady=(0, 10))

        self.var_zero_pii = tk.BooleanVar(value=self.settings["ai_and_privacy"]["zero_pii_gateway"])
        ttk.Checkbutton(card_privacy, text="啟用 Zero-PII 隱私安全閘門", variable=self.var_zero_pii).pack(anchor=tk.W, pady=2)
        self.create_item_subtext(card_privacy, "絕不記錄個人敏感個資、身分資訊與密碼欄位輸入，保障本機最高隱私安全。")

        card_model = ttk.LabelFrame(page, text=" 本地 AI 校正模型與算力管理 ", padding=12)
        card_model.pack(fill=tk.X, pady=(0, 10))

        ttk.Label(card_model, text="本地 AI 模型選擇：", font=self.font_bold).pack(anchor=tk.W, pady=(2, 4))

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

        row_model = ttk.Frame(card_model)
        row_model.pack(fill=tk.X, pady=4)

        rud_w = calc_rud_combobox_width(dropdown_labels)
        self.cb_model = ttk.Combobox(row_model, textvariable=self.var_model,
                                     values=dropdown_labels, state="readonly",
                                     font=self.font_body, width=rud_w)
        self.cb_model.pack(side=tk.LEFT, padx=(0, 8))

        btn_manage_model = ttk.Button(row_model, text="📥 智慧管理模型", command=self.open_model_installer)
        btn_manage_model.pack(side=tk.LEFT, padx=(0, 10))

        self.create_item_subtext(card_model, "快捷提示：打字時按下 Ctrl+Alt+R 可隨時喚醒語意調校工作室，透過雙向詞網感知與 AI 顧問自動提煉二階語法引力。")

        row_tuner_ai = ttk.Frame(card_model)
        row_tuner_ai.pack(fill=tk.X, pady=(6, 2))
        ttk.Button(row_tuner_ai, text="🛠️ 開啟語意調校工作室與 AI 顧問", command=self.open_semantic_tuner).pack(side=tk.LEFT)

        return page

    # ── 頁面六：關於與全語言願景 ──
    def build_about_page(self, parent) -> ttk.Frame:
        page = ttk.Frame(parent)

        card_brand = ttk.LabelFrame(page, text=" 品牌理念與精神哲學 ", padding=12)
        card_brand.pack(fill=tk.X, pady=(0, 10))

        self.lbl_brand_quote = ttk.Label(card_brand, text="「如雲行水流自在歡喜」", font=self.font_card_header)
        self.lbl_brand_quote.pack(anchor=tk.W, pady=(0, 4))

        brand_text = (
            "• Ruh 承襲自 RuhOS 精神哲學，象徵心靈與思維之流動。\n"
            "• I 兼具「輸入 (Input)」與「智慧 (Intelligence)」雙重意涵。\n"
            "• 以如雲行水流之姿態，追求打字無拘無束、心手相應之真境。"
        )
        self.create_item_subtext(card_brand, brand_text)

        card_vision = ttk.LabelFrame(page, text=" 通用智慧型輸入法全語言藍圖（Universal Smart IME Roadmap） ", padding=12)
        card_vision.pack(fill=tk.X, pady=(0, 10))

        vision_desc = (
            "如喜目前雖然專注於許氏鍵盤注音組字與智慧詞庫，但終極目標是建立跨世代的「通用智慧型輸入法」。\n"
            "未來將全面支援全球標準鍵盤以外之全方位語言與輸入方案："
        )
        self.create_item_subtext(card_vision, vision_desc)

        roadmap_items = (
            "  1. 繁體中文注音體系：許氏鍵盤（首發）、標準大千、倚天 26 鍵、漢語拼音\n"
            "  2. 繁體中文形碼體系：倉頡（三代／五代）、大易、行列輸入法\n"
            "  3. 亞洲多語系輸入：日語假名/漢字、韓語諺文、泰語\n"
            "  4. 專業學術標音符號：梵文天城體羅馬化（IAST）、台羅/白話字拼音\n"
            "  5. 歐文多語言擴展：德文、法文、西文等特殊重音符號智慧組字"
        )
        self.lbl_vision_roadmap = ttk.Label(card_vision, text=roadmap_items, font=self.font_subtitle, justify=tk.LEFT)
        self.lbl_vision_roadmap.pack(anchor=tk.W, pady=2)
        self.register_wrap_label(self.lbl_vision_roadmap)

        card_info = ttk.LabelFrame(page, text=" 系統狀態與詞庫架構 ", padding=12)
        card_info.pack(fill=tk.X, pady=(0, 10))

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
        self.lbl_sys_path = ttk.Label(row_sys3, text=str(data_path))
        self.lbl_sys_path.pack(side=tk.LEFT)

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
        ttk.Button(row_btns, text="⚙️ 開啟設定檔資料夾", command=open_config_dir).pack(side=tk.LEFT, padx=(0, 8))
        ttk.Button(row_btns, text="🛠️ 語意調校工作室", command=self.open_semantic_tuner).pack(side=tk.LEFT)

        return page

    def open_semantic_tuner(self):
        import subprocess
        here = Path(__file__).resolve().parent
        ps1_candidates = [
            here / "語意調校工作室.ps1",
            here / "scripts" / "語意除錯調校.ps1",
            here.parent / "語意調校工作室.ps1",
            here.parent / "scripts" / "語意除錯調校.ps1",
        ]
        for ps1 in ps1_candidates:
            if ps1.exists():
                subprocess.Popen(
                    ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(ps1)],
                    cwd=str(ps1.parent)
                )
                return
        py_candidates = [
            here / "scripts" / "semantic_tuner.py",
            here / "semantic_tuner.py",
            here.parent / "scripts" / "semantic_tuner.py",
        ]
        for py in py_candidates:
            if py.exists():
                subprocess.Popen(
                    [sys.executable, str(py), "--web"],
                    cwd=str(py.parent)
                )
                return

    def setup_zoom_and_scroll_bindings(self):
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

        row_h = max(24, int(32 + delta * 1.5))
        self.style.configure("Nav.Treeview", font=self.font_tree, rowheight=row_h)

        self.settings.setdefault("appearance", {})["zoom_level"] = self._zoom_level
        self.update_treeview_width()

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
        dialog = tk.Toplevel(self.root)
        dialog.title("📥 本地 AI 模型智慧安裝中心")
        dialog.geometry("660x540")
        dialog.transient(self.root)
        dialog.grab_set()

        p = self.current_palette
        apply_title_bar_theme(dialog, dark=self.is_dark)
        dialog.configure(bg=p["bg_main"])

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

        ThemeAuditor.audit_and_apply(dialog, is_dark=self.is_dark, palette=p)

    def on_save(self):
        self.settings["appearance"]["input_display_mode"] = self.var_input_mode.get()
        self.settings["appearance"]["orientation"] = self.var_orientation.get()
        self.settings["appearance"]["font_size"] = int(self.var_font_size.get())
        self.settings["appearance"]["max_composition_chars"] = int(self.var_max_comp.get())
        self.settings["appearance"]["theme"] = self.var_theme.get()
        self.settings["appearance"]["capsule_mode"] = self.var_capsule_mode.get()
        self.settings["appearance"]["zoom_level"] = self._zoom_level
        self.settings["appearance"]["geometry"] = self.root.geometry()
        if hasattr(self, "paned") and self.paned:
            try:
                sash_w = self.paned.sashpos(0)
                if sash_w > 80:
                    self.settings["appearance"]["sidebar_width"] = sash_w
            except Exception:
                pass

        self.settings["keyboard"]["layout"] = self.var_layout.get()
        selected_modes = [
            m_code for m_code, bv in getattr(self, "vars_enabled_modes", {}).items() if bv.get()
        ]
        if not selected_modes:
            selected_modes = [self.var_layout.get()]
        elif self.var_layout.get() not in selected_modes:
            selected_modes.insert(0, self.var_layout.get())
        self.settings["keyboard"]["enabled_keyboard_modes"] = selected_modes
        self.settings["keyboard"]["space_behavior"] = self.var_space.get()
        self.settings["keyboard"]["capslock_switch"] = self.var_caps.get()
        self.settings["keyboard"]["punctuation_commit_mode"] = self.var_punct_mode.get()
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
        for m_code, bv in getattr(self, "vars_enabled_modes", {}).items():
            bv.set(m_code in ("hsu", "japanese"))
        self.var_space.set("commit_first")
        self.var_caps.set("capital_lock")
        self.var_punct_mode.set("A")
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
        self.apply_theme("system")
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

        # 測試側欄寬度自適應
        calc_w = app.calc_treeview_width()
        assert calc_w >= 240, f"Calculated treeview width too small: {calc_w}"

        # 測試動態折行機制
        assert len(app._wrap_labels) >= 10, f"Expected wrap labels registered, got {len(app._wrap_labels)}"

        # 測試主題切換
        app.apply_theme("dark")
        assert app.is_dark is True, "Failed to apply dark theme"
        app.apply_theme("light")
        assert app.is_dark is False, "Failed to apply light theme"
        app.apply_theme("system")

        init_zoom = app._zoom_level
        app._zoom(1)
        assert app._zoom_level == init_zoom + 1, f"Expected zoom_level {init_zoom + 1}, got {app._zoom_level}"
        app._zoom(-1)
        assert app._zoom_level == init_zoom, f"Expected zoom_level {init_zoom}, got {app._zoom_level}"

        app.on_save()
        root.destroy()
        try:
            print("[OK] RuhiSettingsApp rud() conditional wrap and two-tier item layout test passed!")
        except Exception:
            pass
        return 0

    root = tk.Tk()
    app = RuhiSettingsApp(root)
    root.mainloop()

if __name__ == "__main__":
    sys.exit(main())
