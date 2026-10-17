# -*- coding: utf-8 -*-
"""
==============================================================================
ATG THEME & COLOR ENGINE (主題與色彩調色引擎)
==============================================================================
Developer Guidelines & Theme Compliance Specifications (主題開發規範):
1. No Naked Win32 Menubars (拒絕裸露白邊系統選單):
   - Windows Tkinter `root.config(menu=menubar)` produces a non-customizable
     white system menubar (`COLOR_MENUBAR`) even in dark mode.
   - Always use `ThemedMenuBar` for desktop applications to ensure 100%
     pixel-perfect dark/light theme consistency across Windows, macOS, and Linux.
2. Recursive Theme Traversal (階層式主題遍歷):
   - Use `apply_theme_to_all_widgets(root, is_dark)` or `ThemeAuditor` to enforce
     consistent background, foreground, selection, and active colors across
     all standard Tk widgets (Menu, Entry, Text, Canvas, Listbox, Combobox popdowns).
3. Windows DWM Immersive Dark Titlebar (DWM 標題列深色渲染):
   - Native OS titlebars must always be synced with app theme using
     `apply_title_bar_theme(window, dark=True)`.
4. Standard Tokenized Palettes (標準化語意色彩代碼):
   - All components must reference semantic color tokens from `PALETTES`
     (bg_main, bg_surface, bg_input, bg_header, bg_hover, fg_main, fg_bright,
     fg_muted, accent, accent_hover, border, stripe_alt, highlight).
"""

import sys
import ctypes
import colorsys
import tkinter as tk
from tkinter import ttk

try:
    import winreg
except ImportError:
    winreg = None


def get_system_theme():
    """Detects Windows System Theme ('dark' or 'light')."""
    if sys.platform == "win32" and winreg is not None:
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize") as key:
                value, _ = winreg.QueryValueEx(key, "AppsUseLightTheme")
                return "light" if value == 1 else "dark"
        except Exception:
            pass
    return "dark"


def get_system_accent_color(default="#007acc"):
    """
    Extracts the Windows 10/11 System Accent Color (個人化強調色) directly from DWM registry.
    Returns standard 6-char hex string, e.g. '#0078d7'.
    """
    if sys.platform == "win32" and winreg is not None:
        # Method 1: Check HKCU\Software\Microsoft\Windows\DWM -> AccentColor (ABGR)
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\DWM") as key:
                accent_val, _ = winreg.QueryValueEx(key, "AccentColor")
                r = accent_val & 0xFF
                g = (accent_val >> 8) & 0xFF
                b = (accent_val >> 16) & 0xFF
                return f"#{r:02x}{g:02x}{b:02x}"
        except Exception:
            pass
        # Method 2: Check HKCU\Software\Microsoft\Windows\DWM -> ColorizationColor (AARRGGBB)
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\DWM") as key:
                col_val, _ = winreg.QueryValueEx(key, "ColorizationColor")
                r = (col_val >> 16) & 0xFF
                g = (col_val >> 8) & 0xFF
                b = col_val & 0xFF
                return f"#{r:02x}{g:02x}{b:02x}"
        except Exception:
            pass
    return default


def apply_title_bar_theme(window, dark=True):
    """Applies Windows DWM immersive dark/light mode to native title bar (64-bit safe, re-entrancy safe)."""
    if not (ctypes and sys.platform == "win32"):
        return
    try:
        if not (window and hasattr(window, 'winfo_exists') and window.winfo_exists()):
            return
        user32 = ctypes.windll.user32
        dwmapi = ctypes.windll.dwmapi
        
        # 尋找 Win32 頂層視窗 Handle (GA_ROOT = 2)
        GA_ROOT = 2
        user32.GetAncestor.argtypes = [ctypes.c_void_p, ctypes.c_uint]
        user32.GetAncestor.restype = ctypes.c_void_p
        hwnd = user32.GetAncestor(window.winfo_id(), GA_ROOT)
        if not hwnd:
            user32.GetParent.argtypes = [ctypes.c_void_p]
            user32.GetParent.restype = ctypes.c_void_p
            hwnd = user32.GetParent(window.winfo_id()) or window.winfo_id()
            
        dwmapi.DwmSetWindowAttribute.argtypes = [
            ctypes.c_void_p, ctypes.c_uint, ctypes.c_void_p, ctypes.c_uint
        ]
        dwmapi.DwmSetWindowAttribute.restype = ctypes.c_int
        
        # 20 is DWMWA_USE_IMMERSIVE_DARK_MODE in Windows 10/11
        # 19 was the older Windows 10 1809 attribute
        val = ctypes.c_int(1 if dark else 0)
        res = dwmapi.DwmSetWindowAttribute(hwnd, 20, ctypes.byref(val), ctypes.sizeof(val))
        if res != 0:
            dwmapi.DwmSetWindowAttribute(hwnd, 19, ctypes.byref(val), ctypes.sizeof(val))

        # Windows 11 Native Caption Color (35) and Text Color (36)
        DWMWA_CAPTION_COLOR = 35
        DWMWA_TEXT_COLOR = 36
        if dark:
            caption_color = ctypes.c_uint32(0x002e1e1e)
            text_color = ctypes.c_uint32(0x00f4d6cd)
        else:
            caption_color = ctypes.c_uint32(0x00faf8f6)
            text_color = ctypes.c_uint32(0x002f2924)

        dwmapi.DwmSetWindowAttribute(
            hwnd, DWMWA_CAPTION_COLOR,
            ctypes.byref(caption_color), ctypes.sizeof(caption_color)
        )
        dwmapi.DwmSetWindowAttribute(
            hwnd, DWMWA_TEXT_COLOR,
            ctypes.byref(text_color), ctypes.sizeof(text_color)
        )
    except Exception:
        pass


def hex_to_rgb(hex_color):
    hex_color = hex_color.lstrip('#')
    if len(hex_color) == 3:
        hex_color = ''.join([c * 2 for c in hex_color])
    return tuple(int(hex_color[i:i+2], 16) for i in (0, 2, 4))


def rgb_to_hex(rgb):
    r = max(0, min(255, int(rgb[0])))
    g = max(0, min(255, int(rgb[1])))
    b = max(0, min(255, int(rgb[2])))
    return f"#{r:02x}{g:02x}{b:02x}"


def adjust_hsl(hex_color, hue_shift=0.0, sat_mult=1.0, light_mult=1.0):
    """Photoshop-like HSL tone adjustment."""
    try:
        r, g, b = [x / 255.0 for x in hex_to_rgb(hex_color)]
        h, l, s = colorsys.rgb_to_hls(r, g, b)
        h = (h + hue_shift) % 1.0
        s = max(0.0, min(1.0, s * sat_mult))
        l = max(0.0, min(1.0, l * light_mult))
        new_r, new_g, new_b = colorsys.hls_to_rgb(h, l, s)
        return rgb_to_hex((new_r * 255, new_g * 255, new_b * 255))
    except Exception:
        return hex_color


PALETTES = {
    "dark_standard": {
        "name": "🌙 標準暗黑 (Studio Dark)",
        "is_dark": True,
        "bg_main": "#1e1e1e",
        "bg_surface": "#21252b",
        "bg_input": "#282c34",
        "bg_header": "#252526",
        "bg_hover": "#2c313a",
        "bg_active": "#3a3f4b",
        "fg_main": "#abb2bf",
        "fg_bright": "#ffffff",
        "fg_muted": "#5c6370",
        "accent": "#007acc",
        "accent_hover": "#1c97ea",
        "border": "#3f3f46",
        "stripe_alt": "#222222",
        "highlight": "#094771",
        "menu_bg": "#21252b",
        "menu_fg": "#abb2bf",
        "menu_active_bg": "#2c313a",
        "menu_active_fg": "#ffffff",
    },
    "light_clean": {
        "name": "☀️ 純粹簡約淺色 (Clean Light)",
        "is_dark": False,
        "bg_main": "#f8f9fa",
        "bg_surface": "#ffffff",
        "bg_input": "#ffffff",
        "bg_header": "#e9ecef",
        "bg_hover": "#e2e6ea",
        "bg_active": "#dae0e5",
        "fg_main": "#212529",
        "fg_bright": "#000000",
        "fg_muted": "#6c757d",
        "accent": "#0d6efd",
        "accent_hover": "#0b5ed7",
        "border": "#dee2e6",
        "stripe_alt": "#f1f3f5",
        "highlight": "#cfe2ff",
        "menu_bg": "#ffffff",
        "menu_fg": "#212529",
        "menu_active_bg": "#e9ecef",
        "menu_active_fg": "#000000",
    }
}


def get_palette(is_dark=None, follow_system_accent=True):
    """
    Returns the active color palette.
    - If is_dark is None: automatically follows Windows System Theme (AppsUseLightTheme).
    - If follow_system_accent is True: automatically integrates Windows System Accent Color.
    """
    if is_dark is None:
        is_dark = (get_system_theme() == "dark")
        
    base = dict(PALETTES["dark_standard"] if is_dark else PALETTES["light_clean"])
    
    if follow_system_accent:
        sys_accent = get_system_accent_color()
        if sys_accent and sys_accent != base["accent"]:
            base["accent"] = sys_accent
            base["accent_hover"] = adjust_hsl(sys_accent, light_mult=1.15 if is_dark else 0.9)
            base["highlight"] = adjust_hsl(sys_accent, light_mult=0.35 if is_dark else 1.6, sat_mult=0.75)
            
    return base


class SystemThemeWatcher:
    """
    即時系統布景主題監聽器 (Live Windows System Theme Watcher):
    - 自動在背景監聽 Windows 系統的深淺色切換（AppsUseLightTheme）與強調色（AccentColor）變更。
    - 當使用者在 Windows 設定中切換外觀時，零延遲觸發回呼函數，無縫自動切換整套應用程式主題。
    """
    def __init__(self, root, on_theme_change_callback, check_interval_ms=1200):
        self.root = root
        self.callback = on_theme_change_callback
        self.check_interval_ms = check_interval_ms
        self._last_theme = get_system_theme()
        self._last_accent = get_system_accent_color()
        self._timer = None
        self._start_polling()
        
    def _start_polling(self):
        self._check()
        
    def _check(self):
        try:
            curr_theme = get_system_theme()
            curr_accent = get_system_accent_color()
            if curr_theme != self._last_theme or curr_accent != self._last_accent:
                self._last_theme = curr_theme
                self._last_accent = curr_accent
                if callable(self.callback):
                    self.callback(curr_theme == "dark", curr_accent)
        except Exception:
            pass
        finally:
            if self.root and self.root.winfo_exists():
                self._timer = self.root.after(self.check_interval_ms, self._check)
                
    def stop(self):
        if self._timer and self.root and self.root.winfo_exists():
            try:
                self.root.after_cancel(self._timer)
            except Exception:
                pass
            self._timer = None


class ThemedMenuBar(tk.Frame):
    """
    自訂深淺色沉浸式水平選單列 (Themed Menu Bar):
    - 完全杜絕 Windows Win32 系統選單在深色模式下的刺眼白色橫條 (COLOR_MENUBAR)。
    - 提供流暢的滑鼠懸停 (Hover)、點擊展開 (Popup)、以及選單間流暢平移切換。
    - 支援 Alt+Key 快捷鍵導覽與純淨字型呈現。
    """
    def __init__(self, parent, is_dark=True, font=("Segoe UI", 9), **kwargs):
        self.is_dark = is_dark
        self.palette = get_palette(is_dark)
        self.font = font
        
        super().__init__(parent, bg=self.palette["bg_surface"], **kwargs)
        
        self._menus = []
        self._buttons = []
        self._active_menu_idx = None
        self._is_menu_open = False
        
        # Bottom subtle border
        self._sep = tk.Frame(self, bg=self.palette["border"], height=1)
        self._sep.pack(side="bottom", fill="x")
        
        self.bind("<Button-1>", lambda e: self._close_active_menu())
        
    def _find_hotkey_underline_index(self, display_text):
        """
        Finds the exact character index of the hotkey accelerator inside display_text.
        e.g., '  檔案 (F)  ' -> finds 'F' inside '(F)' -> index 6.
        e.g., '  語言 (Language)  ' -> finds 'L' inside '(Language)' -> index 6.
        e.g., '  File (&F)  ' -> index of 'F' -> index 8.
        """
        import re
        # Case 1: Match standard CJK hotkey pattern '(X)' or '(X...)' where X is ascii letter
        m = re.search(r'\(([A-Za-z])', display_text)
        if m:
            return m.start(1)
        # Case 2: Match '&X' ampersand mnemonic
        m = re.search(r'&([A-Za-z])', display_text)
        if m:
            return m.start(1)
        return -1

    def add_cascade(self, label, menu, underline=None):
        """Adds a top-level cascade menu button to the menu bar (Clean typography, no underlines)."""
        idx = len(self._menus)
        self._menus.append((label, menu))
        
        bg_normal = self.palette["bg_surface"]
        fg_normal = self.palette["fg_main"]
        bg_hover = self.palette["bg_hover"]
        
        display_text = f"  {label}  "
        btn = tk.Label(
            self,
            text=display_text,
            font=self.font,
            bg=bg_normal,
            fg=fg_normal,
            cursor="arrow",
            padx=2,
            pady=3
        )
        btn.pack(side="left", fill="y", pady=(0, 1))
        
        def _on_enter(event):
            btn.config(bg=bg_hover, fg=self.palette["fg_bright"])
            if self._is_menu_open and self._active_menu_idx != idx:
                self._popup_menu(idx, btn, menu)
                
        def _on_leave(event):
            if not (self._is_menu_open and self._active_menu_idx == idx):
                btn.config(bg=bg_normal, fg=fg_normal)
                
        def _on_click(event):
            if self._is_menu_open and self._active_menu_idx == idx:
                self._close_active_menu()
            else:
                self._popup_menu(idx, btn, menu)
                
        btn.bind("<Enter>", _on_enter)
        btn.bind("<Leave>", _on_leave)
        btn.bind("<Button-1>", _on_click)
        
        self._buttons.append(btn)
        return btn

    def _reset_all_buttons(self, except_idx=None):
        bg_normal = self.palette["bg_surface"]
        fg_normal = self.palette["fg_main"]
        for i, b in enumerate(self._buttons):
            if i != except_idx:
                try:
                    b.config(bg=bg_normal, fg=fg_normal)
                except Exception:
                    pass

    def _popup_menu(self, idx, btn, menu):
        self._reset_all_buttons(except_idx=idx)
        self._active_menu_idx = idx
        self._is_menu_open = True
        btn.config(bg=self.palette["bg_active"], fg=self.palette["fg_bright"])
        
        x = btn.winfo_rootx()
        y = btn.winfo_rooty() + btn.winfo_height()
        
        def _on_menu_unpost():
            self._is_menu_open = False
            self._active_menu_idx = None
            self._reset_all_buttons()
                
        menu.bind("<Unmap>", lambda e: _on_menu_unpost(), add="+")
        try:
            menu.tk_popup(x, y)
        except Exception:
            _on_menu_unpost()

    def _close_active_menu(self):
        self._is_menu_open = False
        self._active_menu_idx = None
        self._reset_all_buttons()

    def update_theme(self, is_dark=True):
        self.is_dark = is_dark
        self.palette = get_palette(is_dark)
        self.config(bg=self.palette["bg_surface"])
        self._sep.config(bg=self.palette["border"])
        for btn in self._buttons:
            btn.config(bg=self.palette["bg_surface"], fg=self.palette["fg_main"])

    def clear(self):
        for btn in self._buttons:
            try: btn.destroy()
            except Exception: pass
        self._buttons.clear()
        self._menus.clear()
        self._active_menu_idx = None
        self._is_menu_open = False


class ThemeAuditor:
    """
    全域主題審查與自動樣式一致性套用器 (Theme Auditor & Universal Style Applicator):
    自動遞迴遍歷整個 Widget 樹，強制修正未套用深/淺色主題的元件。
    """
    @staticmethod
    def audit_and_apply(root_widget, is_dark=True, palette=None):
        if palette is None:
            palette = get_palette(is_dark)
            
        # 1. Apply Tk Option Database for Combobox & Listboxes
        root = root_widget.winfo_toplevel()
        try:
            root.option_add('*TCombobox*Listbox.background', palette["bg_surface"])
            root.option_add('*TCombobox*Listbox.foreground', palette["fg_main"])
            root.option_add('*TCombobox*Listbox.selectBackground', palette["bg_hover"])
            root.option_add('*TCombobox*Listbox.selectForeground', palette["fg_bright"])
            root.option_add('*ComboboxListbox*background', palette["bg_surface"])
            root.option_add('*ComboboxListbox*foreground', palette["fg_main"])
            root.option_add('*ComboboxListbox*selectBackground', palette["bg_hover"])
            root.option_add('*ComboboxListbox*selectForeground', palette["fg_bright"])
            root.option_add('*Listbox.background', palette["bg_surface"])
            root.option_add('*Listbox.foreground', palette["fg_main"])
            root.option_add('*Listbox.selectBackground', palette["bg_hover"])
            root.option_add('*Listbox.selectForeground', palette["fg_bright"])
        except Exception:
            pass
            
        # 2. Apply Title Bar DWM Dark Mode
        apply_title_bar_theme(root, dark=is_dark)
        
        # 3. Recursive Widget Traversal
        def _walk(w):
            w_class = w.winfo_class()
            try:
                if isinstance(w, tk.Menu):
                    w.config(
                        bg=palette["menu_bg"],
                        fg=palette["menu_fg"],
                        activebackground=palette["menu_active_bg"],
                        activeforeground=palette["menu_active_fg"],
                        selectcolor=palette["accent"],
                        bd=1,
                        relief="flat"
                    )
                elif isinstance(w, (tk.Entry, tk.Text)):
                    w.config(
                        bg=palette["bg_input"],
                        fg=palette["fg_main"],
                        insertbackground=palette["fg_bright"],
                        selectbackground=palette["highlight"],
                        selectforeground=palette["fg_bright"]
                    )
                elif isinstance(w, tk.Listbox):
                    w.config(
                        bg=palette["bg_surface"],
                        fg=palette["fg_main"],
                        selectbackground=palette["bg_hover"],
                        selectforeground=palette["fg_bright"]
                    )
            except Exception:
                pass
                
            for child in w.winfo_children():
                _walk(child)
                
        try:
            _walk(root_widget)
        except Exception:
            pass


def apply_theme_to_all_widgets(root, is_dark=True, palette=None):
    """Universal Helper: Applies theme consistently across entire widget hierarchy."""
    ThemeAuditor.audit_and_apply(root, is_dark=is_dark, palette=palette)

