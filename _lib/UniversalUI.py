
# ==============================================================================
# UNIVERSAL UI LIBRARY v1.3
# ==============================================================================

"""
[ 開發樣板與撰寫規範 (UniversalUI Coding Guidelines & Template) ]

1. 【主程式架構規範 (App Architecture)】
   - 所有應用程式強烈建議繼承 UniversalApp。
   - 視窗關閉前務必呼叫 super().on_close() 儲存幾何尺寸與設定檔。

2. 【彈出視窗與對話框規範 (Modal Dialog & MessageBox Rules)】
   - ⚠️ 禁止直接呼叫 OS 原生白底的 tkinter.messagebox (Win32 API 會破壞深色主題、字體縮放與整體美感)。
   - ✅ 一律使用 UniversalUI 提供的通用對話框：
     - app.askyesno(title, message) / UniversalUI.askyesno(...)
     - app.showinfo(title, message) / UniversalUI.showinfo(...)
     - app.showwarning(title, message) / UniversalUI.showwarning(...)
     - app.showerror(title, message) / UniversalUI.showerror(...)
     - app.askokcancel(title, message) / UniversalUI.askokcancel(...)
   - 彈出視窗均具備：
     * 自動跟隨父視窗深色/淺色主題與 DWM 沉浸式暗色標題列 (apply_title_bar_theme)。
     * 自動跟隨 UniversalUI 的縮放比例 (scale_factor) 與動態字體。
     * 自動相對於父視窗居中定位 (Parent-Centered)。
     * 支援鍵盤快捷鍵（Enter 確認、Esc 取消、Y/N 快速鍵）。

3. 【多語言國際化規範 (i18n & Multi-Language Rules)】
   - ✅ 介面文字（按鈕、選單、標籤、提示訊息）一律使用 `i18n.t("key")` 或 `_("key")` 包裹。
   - ✅ 在應用程式目錄下建立 `_lang/languages.tsv` 試算表對照矩陣。
   - ✅ 於 `__init__` 初始化 `i18n.init(os.path.join(curr_dir, "_lang"))`。
   - ✅ 主選單與偏好設定一律使用 `i18n.get_available_languages()` 動態掛載語系。

[ 快速繼承樣板 (Quick Start Template) ]
import os
import tkinter as tk
from tkinter import ttk
from _lib.UniversalUI import UniversalApp, askyesno, showinfo
from _lib import i18n

curr_dir = os.path.dirname(os.path.abspath(__file__))

class MyApp(UniversalApp):
    def __init__(self, root):
        defaults = {
            "geometry": "800x600",
            "language": "zh_TW",
            "my_setting": True
        }
        super().__init__(root, "My New App", "my_app_v1", defaults)
        
        # 初始化多語言引擎
        i18n.init(os.path.join(curr_dir, "_lang"), default_lang="zh_TW")
        i18n.set_language(self.config.get("language", "zh_TW"))
        
        self.setup_ui()
        
    def setup_ui(self):
        # 使用 i18n.t("key") 來建構介面...
        self.btn_save = ttk.Button(self.root, text=i18n.t("btn_save"), command=self.save)
        self.btn_save.pack(padx=20, pady=20)
        
    def save(self):
        self.show_toast(i18n.t("msg_saved"), level="success")
        
    def on_close(self):
        # 覆寫 on_close 儲存參數並關閉
        super().on_close()

def main():
    root = tk.Tk()
    app = MyApp(root)
    root.mainloop()

if __name__ == "__main__":
    main()
"""

import tkinter as tk
from tkinter import ttk, messagebox, font, filedialog
import json
import os
import time
import re
import datetime
import shutil
import glob
import sys
import copy
import ctypes
import winreg
from pathlib import Path


from pathlib import Path


# Re-export from submodules
from _lib.theme_engine import (
    get_system_theme,
    get_system_accent_color,
    get_palette,
    SystemThemeWatcher,
    apply_title_bar_theme,
    hex_to_rgb,
    rgb_to_hex,
    adjust_hsl,
    PALETTES
)
from _lib.responsive_engine import (
    ResponsiveContainer,
    ElasticSpacer,
    AutoScrollbar,
    calc_treeview_width,
    ResponsiveWrapManager,
    TwoTierOption
)
from _lib.table_engine import (
    DraggableTreeHelper,
    UniversalTreeview
)
from _lib.dialog_engine import (
    UniversalMessageBox,
    UniversalToast,
    UniversalDialog,
    showinfo,
    showwarning,
    showerror,
    askyesno,
    askyesnocancel,
    askokcancel,
    askretrycancel,
    askquestion,
    show_toast
)
from _lib.titlebar_engine import ModernTitleBar
from _lib.power_engine import (
    turn_off_screen,
    wake_screen,
    prevent_screen_sleep,
    allow_screen_sleep,
    start_screen_off_timer,
    ScreenOffCountdownDialog
)


# ==============================================================================
# UNIVERSAL MESSAGEBOX & MODAL DIALOGS
# ==============================================================================

def _i18n_text(key, default=""):
    try:
        from _lib import i18n
        res = i18n.t(key)
        if res and res != key:
            return res
    except Exception:
        pass
    return default

class UniversalMessageBox(tk.Toplevel):
    """
    Universal Dark/Light Themed Modal Dialog for Tkinter / UniversalApp.
    Provides responsive typography, font scaling, DWM titlebar dark mode,
    parent-centered positioning, vector emoji icons, and keyboard shortcuts.
    """
    def __init__(
        self,
        parent=None,
        title="提示",
        message="",
        icon="info",        # 'info', 'question', 'warning', 'error'
        buttons="ok",       # 'ok', 'yesno', 'okcancel', 'retrycancel'
        default_btn=None,   # 'yes', 'no', 'ok', 'cancel', 'retry'
        scale_factor=None
    ):
        super().__init__(parent)
        self.parent = parent
        self.result = None
        
        # 1. Determine theme and scaling from parent / UniversalApp
        is_dark = True
        s = 1.0
        if parent:
            top = parent.winfo_toplevel() if hasattr(parent, 'winfo_toplevel') else parent
            is_dark = getattr(top, 'is_dark', True)
            s = getattr(top, 'scale_factor', 1.0)
            
        if scale_factor is not None:
            s = scale_factor
            
        self.scale_factor = s
        self.is_dark = is_dark
        
        # Colors matching UniversalUI theme
        if is_dark:
            bg_main = "#1e2227"
            fg_main = "#abb2bf"
            btn_bg = "#2c313a"
        else:
            bg_main = "#f3f3f3"
            fg_main = "#222222"
            btn_bg = "#e1e4e8"
            
        self.title(title)
        self.configure(bg=bg_main)
        self.resizable(False, False)
        
        # Transient & Modal setup
        if parent:
            self.transient(parent)
            
        apply_title_bar_theme(self, is_dark)
        
        # Fonts
        base_size = 9
        font_msg = ("Microsoft JhengHei UI", max(8, int(base_size * s)))
        
        # Main Layout
        pad_x = max(14, int(20 * s))
        pad_y = max(12, int(18 * s))
        
        container = tk.Frame(self, bg=bg_main, padx=pad_x, pady=pad_y)
        container.pack(fill="both", expand=True)
        
        # Content Row (Icon Left, Text Right)
        content_f = tk.Frame(container, bg=bg_main)
        content_f.pack(fill="both", expand=True, pady=(0, max(10, int(16 * s))))
        
        # Icon Map (Rich Visual Icons with distinct theme colors)
        icon_data = {
            "info": ("ℹ️", "#61afef"),
            "question": ("❓", "#98c379"),
            "warning": ("⚠️", "#e5c07b"),
            "error": ("❌", "#e06c75"),
        }
        ico_char, ico_color = icon_data.get(icon, ("ℹ️", "#61afef"))
        
        lbl_icon = tk.Label(
            content_f, text=ico_char, bg=bg_main, fg=ico_color,
            font=("Segoe UI Emoji", max(18, int(26 * s))),
            anchor="n"
        )
        lbl_icon.pack(side="left", anchor="n", padx=(0, max(12, int(16 * s))))
        
        # Text Block with smart word wrapping
        text_f = tk.Frame(content_f, bg=bg_main)
        text_f.pack(side="left", fill="both", expand=True)
        
        lbl_msg = tk.Label(
            text_f, text=message, bg=bg_main, fg=fg_main,
            font=font_msg, justify="left", anchor="w",
            wraplength=max(320, int(460 * s))
        )
        lbl_msg.pack(fill="both", expand=True)
        
        # Button Row
        btn_f = tk.Frame(container, bg=bg_main)
        btn_f.pack(fill="x", side="bottom")
        
        self._buttons = {}
        
        def _on_action(val):
            self.result = val
            self.destroy()
            
        btn_w = max(7, int(8 * s))
        
        txt_yes = _i18n_text("msg_yes", "是 (Y)")
        txt_no = _i18n_text("msg_no", "否 (N)")
        txt_can = _i18n_text("msg_cancel", "取消 (Esc)")
        txt_ok = _i18n_text("msg_ok", "確定 (Enter)")
        txt_ret = _i18n_text("msg_retry", "重試 (R)")
        
        if buttons == "yesnocancel":
            b_yes = ttk.Button(btn_f, text=txt_yes, command=lambda: _on_action(True), width=btn_w)
            b_no = ttk.Button(btn_f, text=txt_no, command=lambda: _on_action(False), width=btn_w)
            b_can = ttk.Button(btn_f, text=txt_can, command=lambda: _on_action(None), width=btn_w)
            b_can.pack(side="right", padx=(max(4, int(6 * s)), 0))
            b_no.pack(side="right", padx=(max(4, int(6 * s)), 0))
            b_yes.pack(side="right")
            self._buttons['yes'] = b_yes
            self._buttons['no'] = b_no
            self._buttons['cancel'] = b_can
            self.bind("<y>", lambda e: _on_action(True))
            self.bind("<Y>", lambda e: _on_action(True))
            self.bind("<n>", lambda e: _on_action(False))
            self.bind("<N>", lambda e: _on_action(False))
            self.bind("<Escape>", lambda e: _on_action(None))
            if default_btn == 'no':
                b_no.focus_set()
                self.bind("<Return>", lambda e: _on_action(False))
            elif default_btn == 'cancel':
                b_can.focus_set()
                self.bind("<Return>", lambda e: _on_action(None))
            else:
                b_yes.focus_set()
                self.bind("<Return>", lambda e: _on_action(True))
                
        elif buttons == "yesno":
            b_yes = ttk.Button(btn_f, text=txt_yes, command=lambda: _on_action(True), width=btn_w)
            b_no = ttk.Button(btn_f, text=txt_no, command=lambda: _on_action(False), width=btn_w)
            b_no.pack(side="right", padx=(max(4, int(6 * s)), 0))
            b_yes.pack(side="right")
            self._buttons['yes'] = b_yes
            self._buttons['no'] = b_no
            self.bind("<y>", lambda e: _on_action(True))
            self.bind("<Y>", lambda e: _on_action(True))
            self.bind("<n>", lambda e: _on_action(False))
            self.bind("<N>", lambda e: _on_action(False))
            self.bind("<Escape>", lambda e: _on_action(False))
            if default_btn == 'no':
                b_no.focus_set()
                self.bind("<Return>", lambda e: _on_action(False))
            else:
                b_yes.focus_set()
                self.bind("<Return>", lambda e: _on_action(True))
                
        elif buttons == "okcancel":
            b_ok = ttk.Button(btn_f, text=txt_ok, command=lambda: _on_action(True), width=btn_w)
            b_can = ttk.Button(btn_f, text=txt_can, command=lambda: _on_action(False), width=btn_w)
            b_can.pack(side="right", padx=(max(4, int(6 * s)), 0))
            b_ok.pack(side="right")
            self._buttons['ok'] = b_ok
            self._buttons['cancel'] = b_can
            self.bind("<Return>", lambda e: _on_action(True))
            self.bind("<Escape>", lambda e: _on_action(False))
            if default_btn == 'cancel':
                b_can.focus_set()
            else:
                b_ok.focus_set()
                
        elif buttons == "retrycancel":
            b_ret = ttk.Button(btn_f, text=txt_ret, command=lambda: _on_action(True), width=btn_w)
            b_can = ttk.Button(btn_f, text=txt_can, command=lambda: _on_action(False), width=btn_w)
            b_can.pack(side="right", padx=(max(4, int(6 * s)), 0))
            b_ret.pack(side="right")
            self._buttons['retry'] = b_ret
            self._buttons['cancel'] = b_can
            self.bind("<r>", lambda e: _on_action(True))
            self.bind("<R>", lambda e: _on_action(True))
            self.bind("<Return>", lambda e: _on_action(True))
            self.bind("<Escape>", lambda e: _on_action(False))
            b_ret.focus_set()
            
        else: # 'ok'
            b_ok = ttk.Button(btn_f, text=txt_ok, command=lambda: _on_action(True), width=btn_w)
            b_ok.pack(side="right")
            self._buttons['ok'] = b_ok
            self.bind("<Return>", lambda e: _on_action(True))
            self.bind("<Escape>", lambda e: _on_action(True))
            b_ok.focus_set()
            
        # Center dialog over parent or screen
        self.update_idletasks()
        w = self.winfo_reqwidth()
        h = self.winfo_reqheight()
        
        if parent and parent.winfo_viewable():
            pw = parent.winfo_width()
            ph = parent.winfo_height()
            px = parent.winfo_rootx()
            py = parent.winfo_rooty()
            x = px + max(0, (pw - w) // 2)
            y = py + max(0, (ph - h) // 2)
        else:
            sw = self.winfo_screenwidth()
            sh = self.winfo_screenheight()
            x = max(0, (sw - w) // 2)
            y = max(0, (sh - h) // 2)
            
        self.geometry(f"+{x}+{y}")
        self.grab_set()
        
    def show(self):
        self.wait_window(self)
        return self.result


def showinfo(title="提示", message="", parent=None, **kwargs):
    """Universal styled Info MessageBox."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="info", buttons="ok", **kwargs)
    return dlg.show()

def showwarning(title="警告", message="", parent=None, **kwargs):
    """Universal styled Warning MessageBox."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="warning", buttons="ok", **kwargs)
    return dlg.show()

def showerror(title="錯誤", message="", parent=None, **kwargs):
    """Universal styled Error MessageBox."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="error", buttons="ok", **kwargs)
    return dlg.show()

def askyesno(title="確認", message="", parent=None, **kwargs):
    """Universal styled Yes/No Confirmation Dialog."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="question", buttons="yesno", **kwargs)
    return dlg.show()

def askyesnocancel(title="確認", message="", parent=None, **kwargs):
    """Universal styled Yes/No/Cancel Confirmation Dialog."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="question", buttons="yesnocancel", **kwargs)
    return dlg.show()

def askokcancel(title="確認", message="", parent=None, **kwargs):
    """Universal styled OK/Cancel Confirmation Dialog."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="question", buttons="okcancel", **kwargs)
    return dlg.show()

def askretrycancel(title="重試", message="", parent=None, **kwargs):
    """Universal styled Retry/Cancel Dialog."""
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="warning", buttons="retrycancel", **kwargs)
    return dlg.show()

def askquestion(title="確認", message="", parent=None, **kwargs):
    """Universal styled Question Dialog returning 'yes' or 'no'."""
    res = askyesno(title=title, message=message, parent=parent, **kwargs)
    return 'yes' if res else 'no'


class AppConfig:
    def __init__(self, app_name, signature, defaults=None):
        self.app_name = app_name
        self.signature = signature
        # Dynamic Name: [AppName]_cfg.json
        # Dynamic Name: [AppName]_cfg.json
        # Logic:
        # 1. Check legacy: [ScriptDir]/[AppName]_cfg.json
        # 2. Check new:    [ScriptDir]/_cfg/[AppName]_cfg.json
        self.script_dir = os.path.dirname(os.path.abspath(sys.argv[0]))
        self.legacy_path = os.path.join(self.script_dir, f"{app_name}_cfg.json")
        
        self.cfg_dir = os.path.join(self.script_dir, "_cfg")
        self.new_path = os.path.join(self.cfg_dir, f"{app_name}_cfg.json")
        
        # Decide which one to use
        if os.path.exists(self.legacy_path):
            self.filename = self.legacy_path
        else:
            self.filename = self.new_path
        
        self.defaults = defaults or {}
        # Embed signature in defaults
        self.defaults["config_signature"] = self.signature
        
        self.data = self.load()

    def load(self):
        # 1. Try Find Exact Match
        if os.path.exists(self.filename):
            return self._read_file(self.filename)
            
        # 2. Smart Scan (If missing)
        return self._scan_and_adopt()

    def _read_file(self, path):
        data = self.defaults.copy()
        try:
            with open(path, 'r') as f:
                saved = json.load(f)
                # Verify Signature if present in saved file
                if saved.get("config_signature") == self.signature:
                    data.update(saved)
                else:
                    # Generic fallback or migration could happen here
                    # For now, if no signature, we assume it's valid legacy or just load it
                    data.update(saved)
        except: pass
        # Ensure signature is set in memory for next save
        data["config_signature"] = self.signature
        return data

    def _scan_and_adopt(self):
        # Look for *_cfg.json files
        candidates = []
        for file in glob.glob("*_cfg.json"):
            if file == self.filename: continue
            try:
                with open(file, 'r') as f:
                    dat = json.load(f)
                    if dat.get("config_signature") == self.signature:
                        candidates.append(file)
            except: pass
            
        if not candidates:
            return self.defaults.copy()
            
        # Found candidates! Ask User.
        return self._ask_user_to_adopt(candidates)

    def _ask_user_to_adopt(self, candidates):
        # We need a root window to show dialog, but AppConfig might run before main UI?
        # UniversalApp creates AppConfig. We can use a temporary hidden root or defer?
        # Simplest: Use a temp hidden root for the dialog.
        
        root = tk.Tk()
        root.withdraw()
        
        # Format msg
        msg = f"Configuration file '{self.filename}' not found.\n\n"
        msg += f"However, I found {len(candidates)} compatible configuration(s) from previous versions or renamed copies:\n\n"
        for c in candidates[:5]: msg += f" - {c}\n"
        if len(candidates) > 5: msg += " ... and more\n"
        msg += "\nWould you like to import settings from the most recent one?"
        
        # Sort by modification time (newest first)
        candidates.sort(key=lambda x: os.path.getmtime(x), reverse=True)
        best = candidates[0]
        
        # Using AskYesNo
        adopt = messagebox.askyesno("Smart Config Recovery", msg, parent=root)
        root.destroy()
        
        if adopt:
            # Copy content
            data = self._read_file(best)
            # Save immediately to new name
            self.data = data
            self.save()
            return data
        else:
            return self.defaults.copy()

    def save(self):
        self.data["config_signature"] = self.signature
        try:
            # If using new path, ensure directory exists
            if self.filename == self.new_path:
                if not os.path.exists(self.cfg_dir):
                    os.makedirs(self.cfg_dir)

            with open(self.filename, 'w') as f:
                json.dump(self.data, f, indent=4)
        except: pass

    def get(self, key, default=None):
        return self.data.get(key, default)

    def set(self, key, value):
        self.data[key] = value

    def restore_factory_defaults(self):
        self.data = self.defaults.copy()
        self.save()

    def get_profiles_dir(self):
        p_dir = os.path.join(self.cfg_dir, "profiles")
        if not os.path.exists(p_dir):
            try: os.makedirs(p_dir)
            except: pass
        return p_dir

    def save_profile(self, profile_name):
        p_dir = self.get_profiles_dir()
        path = os.path.join(p_dir, f"{profile_name}.json")
        try:
            with open(path, 'w') as f:
                json.dump(self.data, f, indent=4)
            return True
        except:
            return False

    def load_profile(self, profile_name):
        p_dir = self.get_profiles_dir()
        path = os.path.join(p_dir, f"{profile_name}.json")
        if os.path.exists(path):
            self.data = self._read_file(path)
            self.save()
            return True
        return False

    def list_profiles(self):
        p_dir = self.get_profiles_dir()
        profiles = []
        if os.path.exists(p_dir):
            for file in glob.glob(os.path.join(p_dir, "*.json")):
                profiles.append(os.path.splitext(os.path.basename(file))[0])
        return sorted(profiles)

BaseConfig = AppConfig


class UniversalToplevel(tk.Toplevel):
    """
    A auto-themed Toplevel window that inherits the Antigravity theme.
    """
    def __init__(self, parent, title=None, state_id=None, **kwargs):
        self.state_id = state_id or kwargs.pop("state_id", None)
        super().__init__(parent, **kwargs)
        if title: self.title(title)
        
        # 0. Inherit theme and scaling from parent
        top = parent.winfo_toplevel() if (parent and hasattr(parent, 'winfo_toplevel')) else parent
        self.scale_factor = getattr(top, 'scale_factor', 1.0)
        self.is_dark = getattr(top, 'is_dark', True)
        
        # 0.1 Establish Win32 Window Ownership (transient) to prevent Alt+Tab crashes
        if top and top != self:
            try:
                self.transient(top)
            except Exception:
                pass
        
        # 1. Apply Background from Parent (or default)
        try:
            bg = parent.cget("bg")
            self.configure(bg=bg)
        except:
            self.configure(bg="#1e1e1e")
            
        # 2. Apply Title Bar Theme
        is_dark = self.is_dark
        try:
             if self.cget("bg").lower() in ("#f0f0f0", "white", "systembuttonface"):
                 is_dark = False
        except: pass
        
        # Apply immediate
        self.after(10, lambda: apply_title_bar_theme(self, is_dark))

        # 3. State Persistence (Geometry)
        self.app_config = getattr(top, "universal_app_config", None)
        
        if self.state_id and self.app_config:
            # Restore
            geo = self.app_config.get(f"window_{self.state_id}")
            if geo:
                try: self.geometry(geo)
                except: pass
        
        # 4. Zoom & Scaling Keyboard Shortcuts (Ctrl + + / - / 0)
        self.bind("<Control-plus>", self._on_zoom_in)
        self.bind("<Control-equal>", self._on_zoom_in)
        self.bind("<Control-minus>", self._on_zoom_out)
        self.bind("<Control-0>", self._on_zoom_reset)
        self.bind("<Control-KP_Add>", self._on_zoom_in)
        self.bind("<Control-KP_Subtract>", self._on_zoom_out)
        self.bind("<Control-KP_0>", self._on_zoom_reset)
                
        self.protocol("WM_DELETE_WINDOW", self.on_close)

    def _on_zoom_in(self, event=None):
        top = self.master.winfo_toplevel() if hasattr(self, 'master') and self.master else self
        app = getattr(top, 'universal_app_instance', None)
        if app and hasattr(app, 'increase_scale'):
            app.increase_scale(event)
            self.scale_factor = getattr(app, 'scale_factor', self.scale_factor)

    def _on_zoom_out(self, event=None):
        top = self.master.winfo_toplevel() if hasattr(self, 'master') and self.master else self
        app = getattr(top, 'universal_app_instance', None)
        if app and hasattr(app, 'decrease_scale'):
            app.decrease_scale(event)
            self.scale_factor = getattr(app, 'scale_factor', self.scale_factor)

    def _on_zoom_reset(self, event=None):
        top = self.master.winfo_toplevel() if hasattr(self, 'master') and self.master else self
        app = getattr(top, 'universal_app_instance', None)
        if app and hasattr(app, 'reset_scale'):
            app.reset_scale(event)
            self.scale_factor = 1.0

    def on_close(self):
        if self.state_id and self.app_config:
            # Save
            self.app_config.set(f"window_{self.state_id}", self.geometry())
            self.app_config.save()
        self.destroy()

class UniversalApp:
    def __init__(self, root, title, app_signature, defaults=None):
        self.root = root
        self.title = title
        self.root.title(title)
        
        # Determine App Name from script name (safe against renaming)
        # sys.argv[0] -> "FilmStitch_Pro.py" -> "FilmStitch_Pro"
        script_name = os.path.splitext(os.path.basename(sys.argv[0]))[0]
        
        # Win Protocol
        self.root.protocol("WM_DELETE_WINDOW", self.on_close)
        
        # Config (Smart)
        self.config = AppConfig(script_name, app_signature, defaults)
        
        # Expose config and instance to root for children (UniversalToplevel) to access
        self.root.universal_app_config = self.config
        self.root.universal_app_instance = self

        
        # State
        self.base_font_size = 11
        self.scale_factor = self.config.get("ui_scale", 1.0)
        
        # Initialize Named Fonts (These update automatically!)
        # Use Microsoft JhengHei UI for better CJK coverage on Windows, avoiding weird fallbacks
        self.font_std = font.Font(family="Microsoft JhengHei UI", size=self.base_font_size)
        self.font_bold = font.Font(family="Microsoft JhengHei UI", size=self.base_font_size, weight="bold")
        # Keep font_mono object for reference, though we override Text widgets with tuple for fallback
        self.font_mono = font.Font(family="Consolas", size=self.base_font_size-2)
        self.font_h1 = font.Font(family="Microsoft JhengHei UI", size=int(self.base_font_size*1.6), weight="bold")
        try:
            self.font_menu = font.nametofont("TkMenuFont")
            self.font_menu.configure(family="Microsoft JhengHei UI", size=self.base_font_size)
        except Exception:
            self.font_menu = self.font_std
        
        self.setup_theme()
        
        # Geometry Restore
        geo = self.config.get("geometry")
        if geo:
            try: self.root.geometry(geo)
            except: pass
            
        # Maximize Restore
        if self.config.get("maximized", False):
            try: self.root.state('zoomed')
            except: pass
            
        # Bind Scaling
        self.root.bind("<Control-plus>", self.increase_scale)
        self.root.bind("<Control-equal>", self.increase_scale)
        self.root.bind("<Control-minus>", self.decrease_scale)
        self.root.bind("<Control-0>", self.reset_scale)
        self.root.bind("<Control-t>", self.toggle_theme)
        self.root.bind("<Control-T>", self.toggle_theme)

    def toggle_theme(self, event=None):
        """Toggles between Dark and Light mode."""
        new_mode = "light" if getattr(self, 'is_dark', True) else "dark"

            
        self.config.set("theme", new_mode)
        self.setup_theme()
        
        # Show a Toast to notify user
        mode_label = new_mode.capitalize()
        if new_mode == "system":
            mode_label = f"System ({get_system_theme().capitalize()})"
        self.show_toast(f"Theme switched to {mode_label}", level="info")

    def open_settings(self):
        """Opens the generic Universal Settings Dialog."""
        UniversalSettingsDialog(self.root, self.config, self.reload_settings)

    def reload_settings(self):
        """Override this in your app to apply settings when they change."""
        self.show_toast("設定已儲存", level="success")

    def _on_system_theme_changed(self, is_dark, accent_color):
        """Callback triggered when Windows changes Dark/Light mode or Accent Color."""
        if self.config.get("theme", "system") == "system":
            self.setup_theme()
            if hasattr(self, 'reload_theme'):
                self.reload_theme()

    def setup_theme(self):
        # 1. Determine Mode
        mode = self.config.get("theme", "system")
        if mode == "system":
            effective_theme = get_system_theme()
        else:
            effective_theme = mode
            
        self.is_dark = (effective_theme == "dark")
        
        # 2. Get Dynamic Palette with Windows Accent Color
        palette = get_palette(is_dark=self.is_dark, follow_system_accent=True)
        
        # 3. Apply Title Bar Theme (DWM)
        self.root.after(10, lambda: apply_title_bar_theme(self.root, self.is_dark))
        
        # 4. Start Live Windows System Theme Watcher
        if not hasattr(self, '_sys_theme_watcher'):
            self._sys_theme_watcher = SystemThemeWatcher(self.root, self._on_system_theme_changed)

        style = ttk.Style()
        style.theme_use('clam')
        
        BG_MAIN = palette["bg_main"]
        BG_SEC = palette["bg_surface"]
        BG_INPUT = palette["bg_input"]
        FG_MAIN = palette["fg_main"]
        FG_SEC = palette["fg_muted"]
        ACCENT = palette["accent"]
        BORDER = palette["border"]
        
        if self.is_dark:
            self._console_bg = "black"
            self._console_fg = "#00ff00"
            self._header_fg = palette["fg_bright"]
            self._input_fg = palette["fg_bright"]
            self._tree_active = palette["bg_hover"]
            self._tree_sel = palette["highlight"]
        else:
            self._console_bg = "#ffffff"
            self._console_fg = "#333333"
            self._header_fg = palette["fg_bright"]
            self._input_fg = palette["fg_bright"]
            self._tree_active = palette["bg_hover"]
            self._tree_sel = palette["highlight"]
            
        self.root.configure(bg=BG_MAIN)
        
        # Base
        style.configure(".", background=BG_MAIN, foreground=FG_MAIN, borderwidth=0, font=self.font_std)
        
        # Widgets
        style.configure("TLabel", background=BG_MAIN, foreground=FG_MAIN, font=self.font_std)
        style.configure("Header.TLabel", font=self.font_h1, foreground=self._header_fg)
        
        style.configure("TButton", background=BG_INPUT, foreground=self._input_fg, borderwidth=0, relief="flat", anchor="center", font=self.font_std, padding=[6, 4])
        style.map("TButton", background=[('active', ACCENT), ('pressed', '#005fb8')], foreground=[('disabled', FG_SEC), ('active', 'white')])
        
        # Transport Button Style: Exact same handsome height, padding and crisp font
        style.configure("Transport.TButton", font=self.font_std, background=BG_INPUT, foreground=self._input_fg, borderwidth=0, relief="flat", anchor="center", padding=[6, 4])
        style.map("Transport.TButton", background=[('active', ACCENT), ('pressed', '#005fb8')], foreground=[('disabled', FG_SEC), ('active', 'white')])
        style.configure("Compact.TButton", font=self.font_std, padding=[3, 2])
        
        # Entry & Inputs (Dark/Light Invariant Readability)
        style.configure("TEntry", fieldbackground=BG_INPUT, foreground=self._input_fg, insertcolor=self._input_fg, borderwidth=1, bordercolor=BORDER, relief="flat", font=self.font_std, padding=[4, 2])
        style.map("TEntry", 
                  fieldbackground=[('readonly', BG_INPUT), ('disabled', BG_MAIN)],
                  foreground=[('disabled', FG_SEC)],
                  bordercolor=[('focus', ACCENT)], 
                  lightcolor=[('focus', ACCENT)], 
                  darkcolor=[('focus', ACCENT)])
        
        # Combobox
        style.configure("TCombobox", fieldbackground=BG_INPUT, background=BG_INPUT, foreground=self._input_fg, arrowcolor=self._input_fg, borderwidth=0, font=self.font_std, padding=[4, 2])
        style.map("TCombobox", 
                  fieldbackground=[('readonly', 'active', BG_INPUT), ('readonly', BG_INPUT), ('disabled', BG_MAIN)],
                  selectbackground=[('readonly', 'active', BG_INPUT), ('!readonly', 'active', BG_INPUT)],
                  selectforeground=[('readonly', self._input_fg), ('!readonly', self._input_fg)],
                  background=[('active', BG_INPUT), ('pressed', BG_INPUT)], 
                  foreground=[('readonly', 'active', self._input_fg), ('active', self._input_fg), ('disabled', FG_SEC)],
                  arrowcolor=[('active', self._input_fg), ('disabled', FG_SEC)])
        
        # Spinbox
        style.configure("TSpinbox", fieldbackground=BG_INPUT, background=BG_INPUT, foreground=self._input_fg, arrowcolor=self._input_fg, borderwidth=0, font=self.font_std)
        style.map("TSpinbox", 
                  fieldbackground=[('readonly', 'active', BG_INPUT), ('readonly', BG_INPUT), ('disabled', BG_MAIN)],
                  selectbackground=[('readonly', 'active', BG_INPUT), ('!readonly', 'active', BG_INPUT)],
                  selectforeground=[('readonly', self._input_fg), ('!readonly', self._input_fg)],
                  background=[('active', BG_INPUT)],
                  foreground=[('active', self._input_fg), ('disabled', FG_SEC)],
                  arrowcolor=[('active', self._input_fg), ('disabled', FG_SEC)])

        style.configure("Horizontal.TProgressbar", background=ACCENT, troughcolor=BG_INPUT, borderwidth=0)
        
        # Checkbutton / Radiobutton
        style.configure("TCheckbutton", background=BG_MAIN, foreground=FG_MAIN, font=self.font_std)
        style.map("TCheckbutton", indicatorbackground=[('selected', ACCENT), ('active', BG_INPUT)], background=[('active', BG_MAIN)], indicatorcolor=[('selected', ACCENT)])
        
        style.configure("TRadiobutton", background=BG_MAIN, foreground=FG_MAIN, font=self.font_std)
        style.map("TRadiobutton", indicatorbackground=[('selected', ACCENT), ('active', BG_INPUT)], background=[('active', BG_MAIN)], indicatorcolor=[('selected', ACCENT)])
        
        style.configure("TLabelframe", background=BG_MAIN, foreground=FG_MAIN, bordercolor=BORDER, borderwidth=1)
        style.configure("TLabelframe.Label", background=BG_MAIN, foreground=ACCENT, font=self.font_bold)

        # Treeview
        style.configure("Treeview.Heading", background=BG_INPUT, foreground=FG_MAIN, relief="flat", font=self.font_bold)
        style.map("Treeview.Heading", background=[('active', self._tree_active)])
        style.configure("Treeview", background=BG_SEC, foreground=FG_MAIN, fieldbackground=BG_SEC, borderwidth=0, font=self.font_std)
        style.map("Treeview", background=[('selected', self._tree_sel)], foreground=[])
        
        # Notebook
        style.configure('TNotebook', background=BG_MAIN, borderwidth=0)
        style.configure('TNotebook.Tab', background=BG_SEC, foreground=FG_MAIN, padding=[10, 5], borderwidth=0)
        style.map('TNotebook.Tab', background=[('selected', ACCENT), ('active', BG_INPUT)], foreground=[('selected', 'white')])
        
        style.configure("TPanedwindow", background=BG_MAIN)
        sash_bg = "#2b303c" if self.is_dark else "#e2e8f0"
        style.configure("Sash", background=sash_bg, handlepad=0, handlesize=0, sashthickness=3, sashpad=0)
        style.map("Sash", background=[('active', ACCENT), ('hover', ACCENT)])
        
        # Standard TK & TTK Popdown Options for Combobox Dropdown Listbox
        list_bg = "#21252b" if self.is_dark else "#ffffff"
        list_fg = "#abb2bf" if self.is_dark else "#222222"
        list_sel_bg = "#3a3f4b" if self.is_dark else "#007acc"
        self.root.option_add('*TCombobox*Listbox.background', list_bg)
        self.root.option_add('*TCombobox*Listbox.foreground', list_fg)
        self.root.option_add('*TCombobox*Listbox.selectBackground', list_sel_bg)
        self.root.option_add('*TCombobox*Listbox.selectForeground', '#ffffff')
        self.root.option_add('*TCombobox*Listbox.font', self.font_std)
        self.root.option_add('*ComboboxListbox*background', list_bg)
        self.root.option_add('*ComboboxListbox*foreground', list_fg)
        self.root.option_add('*ComboboxListbox*selectBackground', list_sel_bg)
        self.root.option_add('*ComboboxListbox*selectForeground', '#ffffff')
        self.root.option_add('*ComboboxPopdown*background', BG_MAIN)
        self.root.option_add('*Listbox.background', list_bg)
        self.root.option_add('*Listbox.foreground', list_fg)
        self.root.option_add('*Listbox.selectBackground', list_sel_bg)
        self.root.option_add('*Listbox.selectForeground', '#ffffff')
        
        try:
            self.root.bind_class('TCombobox', '<ButtonPress-1>', self._style_combobox_popdown, add="+")
            self.root.bind_class('TCombobox', '<Down>', self._style_combobox_popdown, add="+")
        except Exception:
            pass
        
        # Apply Logic
        self.apply_scaling()

    def _style_combobox_popdown(self, event=None):
        """Ensures the internal Tkinter popdown Listbox of ttk.Combobox matches the theme."""
        try:
            widget = event.widget if event else None
            if widget and hasattr(widget, 'tk'):
                popdown = widget.tk.eval(f'ttk::combobox::PopdownWindow {widget}')
                if popdown:
                    listbox = f'{popdown}.f.l'
                    is_dark = getattr(self, 'is_dark', True)
                    f_std = getattr(self, 'font_std', None)
                    cfg_args = ['-bg', bg, '-fg', fg, 
                                '-selectbackground', sel_bg, '-selectforeground', sel_fg, 
                                '-relief', 'flat', '-bd', '1']
                    if f_std:
                        cfg_args.extend(['-font', f_std])
                    widget.tk.call(listbox, 'configure', *cfg_args)
        except Exception:
            pass

    def apply_scaling(self):
        s = self.scale_factor
        base = self.base_font_size
        
        # Update Named Fonts -> Auto updates TTK widgets
        self.font_std.configure(size=int(base * s))
        self.font_bold.configure(size=int(base * s))
        # Keep font_mono object for reference, though we override Text widgets with tuple for fallback
        self.font_mono.configure(size=int((base-2) * s))
        self.font_h1.configure(size=int((base*1.6) * s))

        # Update TkMenuFont for native and dropdown menus
        try:
            mf = font.nametofont("TkMenuFont")
            mf.configure(size=int(base * s), family="Microsoft JhengHei UI")
        except Exception:
            pass
        
        # Calculate Row Height
        rh = self.font_std.metrics("linespace") + 6
        ttk.Style().configure("Treeview", rowheight=rh)
        
        # Standard TK Options
        self.root.option_add("*Font", self.font_std)
        self.root.option_add("*Menu.font", self.font_std)
        self.root.option_add("*Menubutton.font", self.font_std)
        self.root.option_add("*TCombobox*Listbox.font", self.font_std)
        self.root.option_add("*Combobox*Listbox.font", self.font_std)
        self.root.option_add("*Listbox.font", self.font_std)

        # Refresh root menubar if present
        try:
            m_name = self.root.cget('menu')
            if m_name:
                self.root.config(menu=m_name)
        except Exception:
            pass
        
        # Recursive fix for TK Text widgets
        self.update_tk_widgets(self.root)
        
        self.config.set("ui_scale", self.scale_factor)
        
    def update_tk_widgets(self, widget):
        if isinstance(widget, tk.Text):
            s = int((self.base_font_size-2) * self.scale_factor)
            c_bg = getattr(self, '_console_bg', 'black')
            c_fg = getattr(self, '_console_fg', '#00ff00')
            widget.configure(font=("Consolas", s), bg=c_bg, fg=c_fg, insertbackground=c_fg)
        elif isinstance(widget, tk.Label): # Fallback for old tk.Labels
            # If it looks like a header (manual check), use H1? Hard to detect.
            # Best to use ttk.Label in app code. 
            pass
            
        for child in widget.winfo_children():
            self.update_tk_widgets(child)

    def increase_scale(self, event=None):
        self.scale_factor += 0.1
        self.apply_scaling()

    def decrease_scale(self, event=None):
        if self.scale_factor > 0.5:
            self.scale_factor -= 0.1
            self.apply_scaling()

    def reset_scale(self, event=None):
        self.scale_factor = 1.0
        self.apply_scaling()

    def create_paned_ui(self, parent, list_weight=3, log_weight=1):
        """
        Creates a standard vertically PanedWindow with two frames (List and Log).
        Returns: (paned_window, list_frame, log_frame)
        """
        paned = ttk.PanedWindow(parent, orient=tk.VERTICAL)
        paned.pack(fill="both", expand=True, padx=10, pady=5)
        
        # Pane 1: List Area
        list_frame = ttk.Frame(paned)
        paned.add(list_frame, weight=list_weight)
        
        # Pane 2: Log/Status Area
        log_frame = ttk.Frame(paned)
        paned.add(log_frame, weight=log_weight)
        
        # Restore Sash Position if saved
        sash = self.config.get('sash_pos')
        if sash:
            self.root.after(150, lambda: self._apply_sash_pos(paned, sash))
            
        def on_sash_release(event):
            try:
                pos = paned.sashpos(0)
                if pos and pos > 50:
                    self.config.set('sash_pos', pos)
            except: pass
        paned.bind("<ButtonRelease-1>", on_sash_release, add='+')
        self.main_paned = paned
        
        return paned, list_frame, log_frame

    def _apply_sash_pos(self, paned, sash):
        try:
            paned.sashpos(0, int(sash))
        except: pass
        
    def create_console_log(self, parent, height=5, title="即時日誌 (Live Console)", show_toolbar=True):
        """
        Creates a standard 'Console-like' Text widget with Scrollbar, Export & Clear tools.
        Returns: The Text widget.
        """
        outer_frame = ttk.Frame(parent)
        outer_frame.pack(fill="both", expand=True)
        
        if show_toolbar:
            hdr = ttk.Frame(outer_frame)
            hdr.pack(fill="x", padx=2, pady=(1, 4))
            
            try:
                from _lib import i18n
                init_title = i18n.t("console_title") if i18n.t("console_title") != "console_title" else title
                btn_clear_text = i18n.t("console_clear") if i18n.t("console_clear") != "console_clear" else "📋 清空日誌"
                btn_export_text = i18n.t("console_export") if i18n.t("console_export") != "console_export" else "💾 匯出日誌"
            except Exception:
                init_title = title
                btn_clear_text = "📋 清空日誌"
                btn_export_text = "💾 匯出日誌"
                
            self.console_title_lbl = ttk.Label(hdr, text=init_title, font=self.font_bold)
            self.console_title_lbl.pack(side="left")
            
            def clear_log():
                if askyesno("清空日誌", "確定要清空當前控制台日誌內容嗎？", parent=self.root):
                    txt.delete("1.0", tk.END)
                    self.log_to_widget(txt, "控制台日誌已清空。")
                    
            def export_log():
                content = txt.get("1.0", tk.END).strip()
                if not content:
                    self.show_toast("日誌目前為空，無可匯出內容。", level="warning")
                    return
                
                app_tag = getattr(self, 'app_signature', 'Log')
                default_name = f"{app_tag}_Log_{time.strftime('%Y%m%d_%H%M%S')}.txt"
                f = filedialog.asksaveasfilename(
                    title="匯出控制台日誌",
                    initialfile=default_name,
                    defaultextension=".txt",
                    filetypes=[("Text Log Files", "*.txt"), ("All Files", "*.*")],
                    parent=self.root
                )
                if f:
                    try:
                        with open(f, "w", encoding="utf-8") as fo:
                            fo.write(content + "\n")
                        self.show_toast(f"日誌已成功匯出至：{os.path.basename(f)}", level="success")
                    except Exception as e:
                        showerror("匯出失敗", f"無法寫入日誌檔案：{e}", parent=self.root)
                        
            self.btn_clear_log = ttk.Button(hdr, text=btn_clear_text, command=clear_log)
            self.btn_clear_log.pack(side="right", padx=(4, 0))
            self.btn_export_log = ttk.Button(hdr, text=btn_export_text, command=export_log)
            self.btn_export_log.pack(side="right")
        
        frame = ttk.Frame(outer_frame)
        frame.pack(fill="both", expand=True)
        
        # Scrollbar
        sb = ttk.Scrollbar(frame, orient="vertical")
        sb.pack(side="right", fill="y")
        
        s = int((self.base_font_size-2) * self.scale_factor)
        c_bg = getattr(self, '_console_bg', 'black')
        c_fg = getattr(self, '_console_fg', '#00ff00')
        txt = tk.Text(frame, height=height, bg=c_bg, fg=c_fg, 
                      insertbackground=c_fg, bd=0, 
                      font=("Consolas", s),
                      yscrollcommand=sb.set)
        txt.pack(side="left", fill="both", expand=True)
        
        sb.config(command=txt.yview)
        
        txt.export_log = export_log if show_toolbar else None
        txt.clear_log = clear_log if show_toolbar else None
        
        return txt

    def log_to_widget(self, widget, message):
        ts = time.strftime("[%H:%M:%S]")
        widget.insert(tk.END, f"{ts} {message}\n")
        widget.see(tk.END)

    def on_close(self):
        is_zoomed = (self.root.state() == 'zoomed')
        self.config.set("maximized", is_zoomed)
        
        if not is_zoomed:
            self.config.set("geometry", self.root.geometry())
            
        try:
            if hasattr(self, 'main_paned'):
                pos = self.main_paned.sashpos(0)
                if pos and pos > 50:
                    self.config.set('sash_pos', pos)
            if hasattr(self, 'bottom_paned'):
                pos_b = self.bottom_paned.sashpos(0)
                if pos_b and pos_b > 50:
                    self.config.set('sash_pos_bottom', pos_b)
        except: pass
        
        self.save_bound_vars()
        self.config.save()
        self.root.destroy()


    def bind_config_var(self, key, tk_var):
        if not hasattr(self, '_bound_vars'):
            self._bound_vars = {}
        
        # Load initial value
        val = self.config.get(key)
        if val is not None:
            try: tk_var.set(val)
            except: pass
            
        self._bound_vars[key] = tk_var

    def sync_ui_from_config(self):
        if not hasattr(self, '_bound_vars'): return
        for key, var in self._bound_vars.items():
            val = self.config.get(key)
            if val is not None:
                try: var.set(val)
                except: pass

    def save_bound_vars(self):
        if not hasattr(self, '_bound_vars'): return
        for key, var in self._bound_vars.items():
            self.config.set(key, var.get())
        self.config.save()

    def show_toast(self, message, level="info", duration=3000):
        """Shows a modern non-blocking notification."""
        UniversalToast(self.root, message, level, duration)

    def showinfo(self, title="提示", message="", **kwargs):
        """Universal styled Info MessageBox parented to this app."""
        return showinfo(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def showwarning(self, title="警告", message="", **kwargs):
        """Universal styled Warning MessageBox parented to this app."""
        return showwarning(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def showerror(self, title="錯誤", message="", **kwargs):
        """Universal styled Error MessageBox parented to this app."""
        return showerror(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def askyesno(self, title="確認", message="", **kwargs):
        """Universal styled Yes/No Confirmation Dialog parented to this app."""
        return askyesno(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def askyesnocancel(self, title="確認", message="", **kwargs):
        """Universal styled Yes/No/Cancel Confirmation Dialog parented to this app."""
        return askyesnocancel(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def askokcancel(self, title="確認", message="", **kwargs):
        """Universal styled OK/Cancel Confirmation Dialog parented to this app."""
        return askokcancel(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def askretrycancel(self, title="重試", message="", **kwargs):
        """Universal styled Retry/Cancel Dialog parented to this app."""
        return askretrycancel(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def askquestion(self, title="確認", message="", **kwargs):
        """Universal styled Question Dialog parented to this app."""
        return askquestion(title=title, message=message, parent=self.root, scale_factor=self.scale_factor, **kwargs)

    def turn_off_screen(self, lock=False, delay_sec=0.35):
        """Turns off physical monitor/display via Win32 DPMS with 0.35s smart click debounce."""
        return turn_off_screen(lock=lock, delay_sec=delay_sec)

    def start_screen_off_timer(self, countdown_sec=30, lock=False, message=None, on_complete=None, on_cancel=None):
        """Pops up the standard UniversalUI ScreenOff countdown dialog."""
        return start_screen_off_timer(
            parent=self.root,
            countdown_sec=countdown_sec,
            lock=lock,
            message=message,
            on_complete=on_complete,
            on_cancel=on_cancel
        )

    def prevent_screen_sleep(self, enable=True):
        """Informs OS that long-running batch job is active to prevent automatic OS sleep."""
        return prevent_screen_sleep(enable=enable)

    def allow_screen_sleep(self):
        """Restores normal OS sleep behaviour."""
        return allow_screen_sleep()


def get_ffmpeg_exe():
    cwd_path = Path.cwd() / "ffmpeg.exe"
    if cwd_path.exists(): return str(cwd_path)
    path_exe = shutil.which("ffmpeg")
    if path_exe: return path_exe
    return None

def get_ffprobe_exe():
    cwd_path = Path.cwd() / "ffprobe.exe"
    if cwd_path.exists(): return str(cwd_path)
    ff = get_ffmpeg_exe()
    if ff and "ffmpeg" in str(ff).lower():
        probe = str(ff).replace("ffmpeg", "ffprobe")
        if os.path.exists(probe): return probe
    return shutil.which("ffprobe")

class DraggableTreeHelper:
    """
    Helper to enable 'Lego-like' visual drag-and-drop reordering/merging for ttk.Treeview.
    """
    def __init__(self, tree, on_drop_callback=None):
        self.tree = tree
        self.root = tree.winfo_toplevel()
        self.on_drop_callback = on_drop_callback
        
        # State
        self._drag_data = {"items": [], "y": 0, "ghost": None, "last_target_id": None, "auto_scroll_id": None}
        
        # Configure drop_target style tag based on current theme
        try:
            self.tree.tag_configure("drop_target", background="#007acc", foreground="white")
        except:
            pass
            
        # Bindings
        self.tree.bind("<ButtonPress-1>", self.on_drag_start, add='+')
        self.tree.bind("<B1-Motion>", self.on_drag_motion, add='+')
        self.tree.bind("<ButtonRelease-1>", self.on_drag_release, add='+')

    def on_drag_start(self, event):
        # Only record coordinate, let Tkinter handle default selection first
        item = self.tree.identify_row(event.y)
        if item:
            self._drag_data["start_item"] = item
            self._drag_data["start_y"] = event.y
            self._drag_data["start_x"] = event.x
            self._drag_data["active"] = False
            self._drag_data["last_target_id"] = None
            
            # Check modifiers
            is_ctrl = (event.state & 0x0004) != 0
            is_shift = (event.state & 0x0001) != 0
            
            if item in self.tree.selection() and not is_ctrl and not is_shift:
                # If clicking on an already selected item, save the full selection and prevent default click
                self._drag_data["items"] = list(self.tree.selection())
                return "break"
            else:
                self._drag_data["items"] = []

    def on_drag_motion(self, event):
        start_item = self._drag_data.get("start_item")
        if not start_item: return
        
        # Check if dragging threshold is met
        if not self._drag_data.get("active"):
            dx = abs(event.x - self._drag_data["start_x"])
            dy = abs(event.y - self._drag_data["start_y"])
            if dx > 5 or dy > 5:
                self._drag_data["active"] = True
                
                # Get current selection
                if not self._drag_data.get("items"):
                    selected = list(self.tree.selection())
                    if start_item not in selected:
                        self.tree.selection_set(start_item)
                        selected = [start_item]
                    self._drag_data["items"] = selected
                else:
                    selected = self._drag_data["items"]
                
                # Create ghost
                count = len(selected)
                if count == 1:
                    vals = self.tree.item(start_item, "values")
                    txt = " | ".join(str(v) for v in vals if v)
                    if len(txt) > 50: txt = txt[:47] + "..."
                else:
                    txt = f"📦 已選取 {count} 個項目"
                
                ghost = tk.Toplevel(self.root)
                ghost.overrideredirect(True)
                ghost.attributes("-alpha", 0.7)
                ghost.attributes("-topmost", True)
                
                bg = "#4a90e2"
                fg = "white"
                
                lbl = tk.Label(ghost, text=txt, bg=bg, fg=fg, padx=10, pady=5, relief="solid", borderwidth=1)
                lbl.pack()
                
                self._drag_data["ghost"] = ghost
                self.tree.configure(cursor="hand2")

        if self._drag_data.get("active"):
            # Move Ghost
            ghost = self._drag_data["ghost"]
            if ghost:
                ghost.geometry(f"+{event.x_root + 15}+{event.y_root + 10}")
                
            target_id = self.tree.identify_row(event.y)
            
            # Clear old drop target tags
            for item in self.tree.get_children():
                tags = list(self.tree.item(item, "tags") or [])
                if "drop_target" in tags:
                    tags.remove("drop_target")
                    self.tree.item(item, tags=tags)
                    
            if target_id and target_id not in self._drag_data["items"]:
                self._drag_data["last_target_id"] = target_id
                tags = list(self.tree.item(target_id, "tags") or [])
                if "drop_target" not in tags:
                    tags.append("drop_target")
                    self.tree.item(target_id, tags=tags)

            # Auto-scroll logic
            self._drag_data["last_y"] = event.y
            tree_h = self.tree.winfo_height()
            margin = 30
            if event.y < margin:
                self._start_auto_scroll(-1)
            elif event.y > tree_h - margin:
                self._start_auto_scroll(1)
            else:
                self._stop_auto_scroll()

    def on_drag_release(self, event):
        # Clear drop target tags
        for item in self.tree.get_children():
            tags = list(self.tree.item(item, "tags") or [])
            if "drop_target" in tags:
                tags.remove("drop_target")
                self.tree.item(item, tags=tags)

        if self._drag_data.get("ghost"):
            self._drag_data["ghost"].destroy()
            self._drag_data["ghost"] = None
        self.tree.configure(cursor="")
        
        self._stop_auto_scroll()
        
        if not self._drag_data.get("active"):
            start_item = self._drag_data.get("start_item")
            if start_item and self._drag_data.get("items"):
                self.tree.selection_set(start_item)
            self._drag_data = {"items": [], "y": 0, "ghost": None, "last_target_id": None, "auto_scroll_id": None}
            return
            
        source_ids = self._drag_data["items"]
        target_id = self._drag_data.get("last_target_id")
        
        self._drag_data = {"items": [], "y": 0, "ghost": None, "last_target_id": None, "auto_scroll_id": None}
        
        if target_id and target_id not in source_ids:
            if self.on_drop_callback:
                self.on_drop_callback(source_ids, target_id)

    def _start_auto_scroll(self, direction):
        if self._drag_data.get("auto_scroll_dir") == direction:
            return
        self._drag_data["auto_scroll_dir"] = direction
        self._auto_scroll_loop()

    def _stop_auto_scroll(self):
        self._drag_data["auto_scroll_dir"] = 0
        if self._drag_data.get("auto_scroll_id"):
            self.root.after_cancel(self._drag_data["auto_scroll_id"])
            self._drag_data["auto_scroll_id"] = None

    def _auto_scroll_loop(self):
        direction = self._drag_data.get("auto_scroll_dir", 0)
        if direction == 0:
            return
        
        self.tree.yview_scroll(direction, "units")
        
        # Update target_id since items have moved
        last_y = self._drag_data.get("last_y")
        if last_y is not None:
            target_id = self.tree.identify_row(last_y)
            for item in self.tree.get_children():
                tags = list(self.tree.item(item, "tags") or [])
                if "drop_target" in tags:
                    tags.remove("drop_target")
                    self.tree.item(item, tags=tags)
                    
            if target_id and target_id not in self._drag_data.get("items", []):
                self._drag_data["last_target_id"] = target_id
                tags = list(self.tree.item(target_id, "tags") or [])
                if "drop_target" not in tags:
                    tags.append("drop_target")
                    self.tree.item(target_id, tags=tags)
                    
        self._drag_data["auto_scroll_id"] = self.root.after(50, self._auto_scroll_loop)

class UndoManager:
    """
    Generic Undo/Redo Manager.
    Maintains a history stack of states.
    """
    def __init__(self, callback_restore, limit=32):
        self.callback_restore = callback_restore
        self.limit = limit
        self.history = []
        self.redo_stack = []
        
    def snapshot(self, state):
        # Deep copy the state to ensure isolation
        try:
            # Using deepcopy is safer involves mutable objects (lists/dicts)
            frozen = copy.deepcopy(state)
            self.history.append(frozen)
            
            # Enforce Limit
            if len(self.history) > self.limit:
                self.history.pop(0)
                
            # Clear redo stack on new branch
            self.redo_stack.clear()
        except Exception as e:
            print(f"UndoManager Snapshot Error: {e}")

    def undo(self, event=None):
        if len(self.history) < 2:
            return # Nothing to undo (need at least current state + 1 prev)
            
        # 1. Pop current state and push to redo
        current = self.history.pop()
        self.redo_stack.append(current)
        
        # 2. Peek previous state (now last in history)
        prev = self.history[-1]
        
        # 3. Restore
        # Note: pass a COPY to the app, so if app mutates it, it doesn't corrupt history record
        self.callback_restore(copy.deepcopy(prev))

    def redo(self, event=None):
        if not self.redo_stack:
            return
            
        # 1. Pop from redo
        next_state = self.redo_stack.pop()
        
        # 2. Push back to history
        self.history.append(next_state)
        
        # 3. Restore
        self.callback_restore(copy.deepcopy(next_state))


class UniversalSettingsDialog:
    """
    Auto-generated Settings Dialog based on AppConfig data.
    """
    def __init__(self, parent, config, on_save_callback=None):
        self.config = config
        self.on_save_callback = on_save_callback
        
        self.win = tk.Toplevel(parent)
        self.win.title("Configuration")
        
        # Restore Geometry if exists
        geo = self.config.get("_settings_geometry")
        if geo:
            try: self.win.geometry(geo)
            except: self.win.geometry("400x500")
        else:
            self.win.geometry("400x500")
            
        self.win.transient(parent)
        self.win.grab_set()
        
        # Style: Match Parent Theme
        bg_color = parent.cget("bg")
        self.win.configure(bg=bg_color)
        
        # Infer Dark Mode from BG color (approximate)
        # Default Windows Light is #f0f0f0 or SystemButtonFace
        is_dark = True
        try:
            # If strictly native light, bg might be named color or #f0f0f0
            if bg_color.lower() in ("#f0f0f0", "systembuttonface", "white"):
                is_dark = False
        except: pass
        
        # Apply Title Bar
        self.win.after(10, lambda: apply_title_bar_theme(self.win, is_dark))
        
        # Bind Close
        self.win.protocol("WM_DELETE_WINDOW", self.on_close)
        
        # Buttons (Pack FIRST to ensure they stay at bottom)
        btn_frame = ttk.Frame(self.win)
        btn_frame.pack(side="bottom", fill="x", pady=10)
        ttk.Button(btn_frame, text="Save", command=self.save).pack(side="right", padx=10)
        ttk.Button(btn_frame, text="Cancel", command=self.on_close).pack(side="right")
        
        # Scrollable Canvas
        # Match canvas bg to theme, remove highlight border
        self.canvas = tk.Canvas(self.win, bg=bg_color, highlightthickness=0)
        self.scrollbar = ttk.Scrollbar(self.win, orient="vertical", command=self.canvas.yview)
        
        # Frame inside canvas
        self.scrollable_frame = ttk.Frame(self.canvas)

        self.scrollable_frame.bind(
            "<Configure>",
            lambda e: self.canvas.configure(scrollregion=self.canvas.bbox("all"))
        )

        self.canvas_window = self.canvas.create_window((0, 0), window=self.scrollable_frame, anchor="nw")
        self.canvas.configure(yscrollcommand=self.scrollbar.set)
        
        # Handle Canvas Resize to resize the inner frame
        self.canvas.bind("<Configure>", self.on_canvas_configure)

        self.scrollbar.pack(side="right", fill="y")
        self.canvas.pack(side="left", fill="both", expand=True)
        
        self.vars = {}
        self.build_ui()

    def on_canvas_configure(self, event):
        # Force inner frame to match canvas width
        self.canvas.itemconfig(self.canvas_window, width=event.width)


    def build_ui(self):
        # Filter keys: ignore _*, ignore signature
        keys = sorted([k for k in self.config.data.keys() if not k.startswith("_") and k != "config_signature"])
        
        r = 0
        for k in keys:
            val = self.config.data[k]
            v_type = type(val)
            
            lbl = ttk.Label(self.scrollable_frame, text=k + ":")
            lbl.grid(row=r, column=0, sticky="w", padx=10, pady=5)
            
            if v_type == bool:
                var = tk.BooleanVar(value=val)
                ent = ttk.Checkbutton(self.scrollable_frame, variable=var)
                ent.grid(row=r, column=1, sticky="w", padx=10, pady=5)
                self.vars[k] = (var, bool)
            else:
                var = tk.StringVar(value=str(val))
                ent = ttk.Entry(self.scrollable_frame, textvariable=var, width=30)
                ent.grid(row=r, column=1, sticky="w", padx=10, pady=5)
                # Helper for int/float detection
                target_type = int if v_type == int else (float if v_type == float else str)
                self.vars[k] = (var, target_type)
            
            r += 1

    def save_geometry(self):
        self.config.data["_settings_geometry"] = self.win.geometry()

    def on_close(self):
        self.save_geometry()
        # Save config silently to persist geometry even if values weren't saved?
        # Typically yes, window position preference is separate from apply.
        self.config.save() 
        self.win.destroy()

    def save(self):
        self.save_geometry()
        
        for k, (var, t_type) in self.vars.items():
            val = var.get()
            try:
                if t_type == bool:
                    self.config.data[k] = bool(val)
                elif t_type == int:
                    self.config.data[k] = int(val)
                elif t_type == float:
                    self.config.data[k] = float(val)
                else:
                    self.config.data[k] = str(val)
            except:
                # Type conversion failed, keep old or set as str? 
                # Better safe than sorry, ignore error or warn.
                print(f"Failed to convert {k} to {t_type}")
                pass
                
        # Save to disk and close
        self.config.save()
        if self.on_save_callback:
            self.on_save_callback()
        self.on_close()


class ResponsiveFrame(ttk.Frame):
    """
    A Frame that automatically switches its layout from horizontal (row) to vertical (stack)
    when its width falls below a certain threshold.
    """
    def __init__(self, parent, threshold=400, padding=5, **kwargs):
        super().__init__(parent, **kwargs)
        self.threshold = threshold
        self.padding = padding
        self._is_stacked = False
        self._widgets = []
        
        self.bind("<Configure>", self._on_resize)

    def add_widget(self, widget_class, **kwargs):
        """Creates and adds a widget to this frame."""
        w = widget_class(self, **kwargs)
        self._widgets.append(w)
        self._re_layout()
        return w

    def add_existing(self, widget):
        """Adds an existing widget to managed list."""
        widget.master = self # Warning: master change in TK is tricky
        self._widgets.append(widget)
        self._re_layout()

    def _on_resize(self, event):
        should_stack = event.width < self.threshold
        if should_stack != self._is_stacked:
            self._is_stacked = should_stack
            self._re_layout()

    def _re_layout(self):
        for w in self._widgets:
            w.pack_forget()
        
        for w in self._widgets:
            if self._is_stacked:
                w.pack(side="top", fill="x", padx=self.padding, pady=self.padding)
            else:
                w.pack(side="left", fill="both", expand=True, padx=self.padding, pady=self.padding)


class UniversalToast(tk.Toplevel):
    """
    A non-blocking 'Toast' notification window.
    """
    def __init__(self, parent, message, level="info", duration=3000):
        super().__init__(parent)
        self.overrideredirect(True)
        self.attributes("-topmost", True)
        self.attributes("-alpha", 0.0) # Start invisible for fade-in
        
        # Colors based on level
        colors = {
            "info": ("#007acc", "white"),
            "success": ("#28a745", "white"),
            "warning": ("#ffc107", "black"),
            "error": ("#dc3545", "white")
        }
        bg, fg = colors.get(level, colors["info"])
        
        self.configure(bg=bg)
        
        # Icon (Simulated with emoji for now)
        icons = {"info": "ℹ️", "success": "✅", "warning": "⚠️", "error": "❌"}
        icon = icons.get(level, "🔔")
        
        lbl = tk.Label(self, text=f"{icon} {message}", bg=bg, fg=fg, 
                       padx=20, pady=10, font=("Microsoft JhengHei UI", 10, "bold"))
        lbl.pack()
        
        self.update_idletasks()
        
        # Position: Bottom Right of parent or screen
        try:
            px = parent.winfo_rootx()
            py = parent.winfo_rooty()
            pw = parent.winfo_width()
            ph = parent.winfo_height()
            
            x = px + pw - self.winfo_width() - 20
            y = py + ph - self.winfo_height() - 20
        except:
            # Fallback to screen bottom right
            sw = self.winfo_screenwidth()
            sh = self.winfo_screenheight()
            x = sw - self.winfo_width() - 20
            y = sh - self.winfo_height() - 60
            
        self.geometry(f"+{x}+{y}")
        
        # Fade In
        self._fade_in()
        
        # Auto Close
        self.after(duration, self._fade_out)

    def _fade_in(self):
        alpha = self.attributes("-alpha")
        if alpha < 0.95:
            self.attributes("-alpha", alpha + 0.1)
            self.after(20, self._fade_in)

    def _fade_out(self):
        alpha = self.attributes("-alpha")
        if alpha > 0.05:
            self.attributes("-alpha", alpha - 0.1)
            self.after(20, self._fade_out)
        else:
            self.destroy()
                
class UniversalTreeview(ttk.Treeview):
    """
    A pre-configured Treeview with:
    - Auto-sorting columns
    - Drag & Drop reordering (optional)
    - Alternating row colors
    """
    def __init__(self, parent, columns, draggable=True, **kwargs):
        super().__init__(parent, columns=columns, show="headings", **kwargs)
        
        self.columns = columns
        self.draggable = draggable
        self._current_sort_col = "#" if "#" in columns else None
        self._current_sort_desc = False
        
        # Setup Columns
        for col in columns:
            init_txt = f"{col} ▲" if col == "#" else col
            self.heading(col, text=init_txt, command=lambda c=col: self.sort_by(c))
            self.column(col, width=100) # Default width, can be overridden
            
        # Alternating Colors (Try to get from style or hardcode decent dark defaults)
        self.tag_configure('odd', background='#252526')
        self.tag_configure('even', background='#1e1e1e')

        # Drag & Drop
        if self.draggable:
            self.drag_helper = DraggableTreeHelper(self, self.on_drag_drop_complete)

        # Select All binding
        self.bind("<Control-a>", self.select_all)
        self.bind("<Control-A>", self.select_all)
        
        # Navigation bindings
        self.bind("<Control-Home>", self.move_to_home)
        self.bind("<Control-End>", self.move_to_end)
        self.bind("<Control-Shift-Home>", self.extend_to_home)
        self.bind("<Control-Shift-End>", self.extend_to_end)
        
        # F2 Inline Rename Binding & Click Column Tracking
        self.bind("<F2>", self._start_inline_edit)
        self.bind("<Button-1>", self._on_tree_click_track_col, add="+")
        self.bind("<ButtonPress>", lambda e: self._hide_hint_tooltip(), add="+")
        self.bind("<Motion>", self._on_tree_motion_hint, add="+")
        self.bind("<Leave>", lambda e: self._hide_hint_tooltip(), add="+")

    def _on_tree_motion_hint(self, event):
        sel = self.selection()
        if len(sel) <= 1:
            self._hide_hint_tooltip()
            return
            
        if getattr(self, '_hint_timer', None):
            try: self.after_cancel(self._hint_timer)
            except: pass
            self._hint_timer = None
        
        # Responsive 250ms hover delay matching timeline canvas tooltip
        self._hint_timer = self.after(250, lambda: self._show_batch_hint_tooltip(event.x_root, event.y_root))

    def _show_batch_hint_tooltip(self, x_root, y_root):
        sel = self.selection()
        if len(sel) <= 1: return
        
        top = self.winfo_toplevel()
        is_dark = getattr(top, 'is_dark', True)
        s = getattr(top, 'scale_factor', 1.0)
        
        bg = "#1e2227" if is_dark else "#ffffff"
        fg = "#61afef" if is_dark else "#005a9e"
        txt_fg = "#abb2bf" if is_dark else "#333333"
        font_title = ("Microsoft JhengHei UI", max(9, int(9.5 * s)), "bold")
        font_body = ("Microsoft JhengHei UI", max(8, int(8.5 * s)))
        
        if getattr(self, '_hint_win', None) and self._hint_win.winfo_exists():
            try: self._hint_win.destroy()
            except: pass
            
        self._hint_win = tk.Toplevel(top)
        self._hint_win.overrideredirect(True)
        self._hint_win.attributes("-topmost", True)
        try: self._hint_win.attributes("-alpha", 0.94)
        except: pass
        self._hint_win.configure(bg="#3a3f4b", bd=1)
        
        card = tk.Frame(self._hint_win, bg=bg, padx=max(8, int(10 * s)), pady=max(6, int(8 * s)))
        card.pack(fill="both", expand=True)
        
        tk.Label(card, text=f"💡 批次編輯技巧 (已選取 {len(sel)} 首曲目)", bg=bg, fg=fg, font=font_title, anchor="w").pack(fill="x", pady=(0, 2))
        hint_text = (
            "• 按 F2 或 Alt+點擊欄位：批次修改選取曲目\n"
            "• 編輯中按 Tab / Shift+Tab：切換前/後欄位\n"
            "• 支援自動流水號：[##1] 或 (#01)（多位數補零）\n"
            "• 排程標籤支援：P1T1, P1E1, T1, E1, M, MG1O1\n"
            "• 支援方向鍵在文字中自由移動游標與選取"
        )
        tk.Label(card, text=hint_text, bg=bg, fg=txt_fg, font=font_body, justify="left", anchor="w").pack(fill="x")
        
        self._hint_win.geometry(f"+{x_root + 15}+{y_root + 15}")
        
        # Auto-hide bindings when user clicks, leaves, or switches windows
        self._hint_win.bind("<Button-1>", lambda e: self._hide_hint_tooltip())
        top.bind("<FocusOut>", lambda e: self._hide_hint_tooltip(), add="+")
        top.bind("<Unmap>", lambda e: self._hide_hint_tooltip(), add="+")

    def _hide_hint_tooltip(self):
        if getattr(self, '_hint_timer', None):
            try: self.after_cancel(self._hint_timer)
            except: pass
            self._hint_timer = None
        if getattr(self, '_hint_win', None) and self._hint_win.winfo_exists():
            try: self._hint_win.destroy()
            except: pass
            self._hint_win = None

    def _on_tree_click_track_col(self, event):
        col = self.identify_column(event.x)
        if col:
            try:
                col_idx = int(col.replace('#', '')) - 1
                vis = self.get_visible_columns()
                if 0 <= col_idx < len(vis):
                    self._last_clicked_col = vis[col_idx]
            except: pass

    def enable_inline_editing(self, default_col="檔案名稱", on_rename_callback=None, editable_columns=None):
        """
        Enables in-place cell editing on F2 key press, Alt+Click, and Double-Click across editable columns.
        - Normal Left Click: 100% standard OS selection behavior.
        - Alt + Left Click: Directly batch-edits clicked column across all selected rows WITHOUT altering selection!
        - Mouse Hover: Tracks hovered cell for F2 quick targeting.
        """
        self.inline_edit_col = default_col
        self.on_rename_callback = on_rename_callback
        self.editable_columns = set(editable_columns) if editable_columns is not None else None
        self._hover_col = None
        self._hover_row_id = None
        
        self.bind("<Motion>", self._on_tree_motion, add="+")
        self.bind("<Alt-Button-1>", self._on_alt_click_inline_edit, add="+")
        self.bind("<Alt-1>", self._on_alt_click_inline_edit, add="+")
        self.bind("<Double-1>", self._on_double_click_inline_edit, add="+")
        self.bind("<F2>", self._start_inline_edit)

    def _on_tree_motion(self, event):
        row_id = self.identify_row(event.y)
        col_id = self.identify_column(event.x)
        if row_id and col_id:
            try:
                col_idx = int(col_id.replace('#', '')) - 1
                vis = self.get_visible_columns()
                if 0 <= col_idx < len(vis):
                    self._hover_col = vis[col_idx]
                    self._hover_row_id = row_id
                    return
            except: pass
        self._hover_col = None
        self._hover_row_id = None

    def _on_alt_click_inline_edit(self, event):
        """Alt + Click: Immediately enters edit mode for clicked column without resetting multi-row selection."""
        row_id = self.identify_row(event.y)
        col_id = self.identify_column(event.x)
        if not row_id or not col_id: return "break"
        
        try:
            col_idx = int(col_id.replace('#', '')) - 1
            vis = self.get_visible_columns()
            if 0 <= col_idx < len(vis):
                clicked_col = vis[col_idx]
                self._last_clicked_col = clicked_col
                
                if self.editable_columns is not None and clicked_col not in self.editable_columns:
                    return "break"
                
                curr_sel = list(self.selection())
                if not curr_sel:
                    self.selection_set([row_id])
                elif row_id in curr_sel:
                    self.focus(row_id)
                    
                self._start_inline_edit(event)
                return "break"
        except Exception:
            pass
        return "break"

    def _on_double_click_inline_edit(self, event):
        row_id = self.identify_row(event.y)
        col_id = self.identify_column(event.x)
        if not row_id or not col_id: return
        
        try:
            col_idx = int(col_id.replace('#', '')) - 1
            vis = self.get_visible_columns()
            if 0 <= col_idx < len(vis):
                clicked_col = vis[col_idx]
                self._last_clicked_col = clicked_col
                
                # If editable_columns specified, check if this column can be edited
                if self.editable_columns is not None and clicked_col not in self.editable_columns:
                    return
                
                curr_sel = list(self.selection())
                if row_id not in curr_sel:
                    self.selection_set([row_id])
                self.focus(row_id)
                self._start_inline_edit(event)
                return "break"
        except Exception:
            pass

    def _start_inline_edit(self, event=None):
        sel = list(self.selection())
        if not sel: return "break"
        focus_id = self.focus()
        target_iid = focus_id if focus_id in sel else sel[0]
        
        vis = self.get_visible_columns()
        
        # Priority 1: If hovered over an editable column when F2 was pressed, use hover column!
        col_to_edit = None
        if event is None and getattr(self, '_hover_col', None):
            h_col = self._hover_col
            if h_col in vis and (self.editable_columns is None or h_col in self.editable_columns):
                col_to_edit = h_col

        # Priority 2: Last clicked column
        if not col_to_edit:
            col_to_edit = getattr(self, '_last_clicked_col', None)

        # Priority 3: Fallback default
        if not col_to_edit or col_to_edit not in vis:
            col_to_edit = getattr(self, 'inline_edit_col', '檔案名稱')
        if col_to_edit not in vis:
            col_to_edit = vis[1] if len(vis) > 1 else vis[0]
            
        # Check editable columns whitelist on F2 press
        if getattr(self, 'editable_columns', None) is not None:
            if col_to_edit not in self.editable_columns:
                fallback = getattr(self, 'inline_edit_col', None)
                if fallback and fallback in vis and fallback in self.editable_columns:
                    col_to_edit = fallback
                else:
                    editables_vis = [c for c in vis if c in self.editable_columns]
                    if editables_vis:
                        col_to_edit = editables_vis[0]
                    else:
                        return "break"
            
        bbox = self.bbox(target_iid, column=col_to_edit)
        if not bbox:
            self.see(target_iid)
            self.update_idletasks()
            bbox = self.bbox(target_iid, column=col_to_edit)
            if not bbox: return "break"
            
        x, y, w, h = bbox
        old_val = str(self.set(target_iid, col_to_edit))
        
        top = self.winfo_toplevel()
        is_dark = getattr(top, 'is_dark', True)
        s = getattr(top, 'scale_factor', 1.0)
        bg = "#1e2227" if is_dark else "#ffffff"
        fg = "#ffffff" if is_dark else "#000000"
        entry_font = ("Microsoft JhengHei UI", max(9, int(9.5 * s)))
        
        entry = tk.Entry(self, bg=bg, fg=fg, insertbackground=fg, bd=1, relief="solid",
                         font=entry_font)
        entry.insert(0, old_val)
        
        # Explorer smart selection: highlight base name without extension
        if "." in old_val and not old_val.startswith("."):
            base_len = old_val.rfind(".")
            entry.select_range(0, base_len)
            entry.icursor(base_len)
        else:
            entry.select_range(0, tk.END)
            
        entry.place(x=x, y=y, width=max(w, int(180 * s)), height=h)
        entry.focus_set()
        
        def commit(ev=None):
            if not entry.winfo_exists(): return
            new_val = entry.get().strip()
            entry.destroy()
            if new_val and new_val != old_val:
                if getattr(self, 'on_rename_callback', None):
                    res = self.on_rename_callback(sel, target_iid, col_to_edit, old_val, new_val)
                    if res is False:
                        return
                    if isinstance(res, dict):
                        for item_id, item_val in res.items():
                            self.set(item_id, col_to_edit, item_val)
                        return
                self.set(target_iid, col_to_edit, new_val)
                
        def cancel(ev=None):
            if entry.winfo_exists():
                entry.destroy()

        def next_column(direction=1):
            if not entry.winfo_exists(): return
            new_val = entry.get().strip()
            entry.destroy()
            if new_val and new_val != old_val:
                if getattr(self, 'on_rename_callback', None):
                    res = self.on_rename_callback(sel, target_iid, col_to_edit, old_val, new_val)
                    if isinstance(res, dict):
                        for item_id, item_val in res.items():
                            self.set(item_id, col_to_edit, item_val)
                    elif res is not False:
                        self.set(target_iid, col_to_edit, new_val)
                else:
                    self.set(target_iid, col_to_edit, new_val)
            
            # Find next editable column in visible columns
            editables = getattr(self, 'editable_columns', None)
            vis_cols = self.get_visible_columns()
            if editables is not None:
                vis_cols = [c for c in vis_cols if c in editables]
            if not vis_cols: return
            
            cur_i = vis_cols.index(col_to_edit) if col_to_edit in vis_cols else 0
            next_i = (cur_i + direction) % len(vis_cols)
            self._last_clicked_col = vis_cols[next_i]
            self.after(20, lambda: self._start_inline_edit(None))
            
        entry.bind("<Return>", commit)
        entry.bind("<Escape>", cancel)
        entry.bind("<FocusOut>", commit)
        entry.bind("<Tab>", lambda e: (next_column(1), "break")[1])
        entry.bind("<Shift-Tab>", lambda e: (next_column(-1), "break")[1])
        entry.bind("<ISO_Left_Tab>", lambda e: (next_column(-1), "break")[1])
        
        return "break"

    def select_all(self, event=None):
        """Select all items in the treeview."""
        self.selection_add(self.get_children())
        return "break"

    def move_to_home(self, event=None):
        children = self.get_children()
        if children:
            self.selection_set(children[0])
            self.focus(children[0])
            self.see(children[0])
        return "break"

    def move_to_end(self, event=None):
        children = self.get_children()
        if children:
            self.selection_set(children[-1])
            self.focus(children[-1])
            self.see(children[-1])
        return "break"

    def extend_to_home(self, event=None):
        children = self.get_children()
        current = self.focus()
        if not children or not current:
            return "break"
        idx = children.index(current)
        self.selection_add(children[0:idx+1])
        self.see(children[0])
        return "break"

    def extend_to_end(self, event=None):
        children = self.get_children()
        current = self.focus()
        if not children or not current:
            return "break"
        idx = children.index(current)
        self.selection_add(children[idx:])
        self.see(children[-1])
        return "break"

        
    def sort_by(self, col, descending=None):
        """
        Sort tree contents when a column header is clicked.
        Uses column header sort flag state and shows Windows Explorer-style sort arrow indicators (▲ / ▼).
        """
        children = self.get_children('')
        if not children: return
        
        # Check and toggle sort direction flag
        if descending is None:
            if getattr(self, '_current_sort_col', None) == col:
                descending = not getattr(self, '_current_sort_desc', False)
            else:
                descending = False
                
        self._current_sort_col = col
        self._current_sort_desc = descending
        
        data = [(self.set(child, col), child) for child in children]
        
        def extract_key(val):
            if not val or str(val).strip() in ("---", ""):
                return (2, "")
            val_str = str(val).strip()
            
            # Numeric / Index (# column)
            try:
                if val_str.isdigit():
                    return (0, int(val_str))
                return (0, float(val_str))
            except ValueError: pass
            
            # File size ("12.34 MB", "500 KB")
            v_upper = val_str.upper()
            if "MB" in v_upper:
                try: return (0, float(v_upper.replace("MB", "").strip()) * 1024 * 1024)
                except: pass
            if "KB" in v_upper:
                try: return (0, float(v_upper.replace("KB", "").strip()) * 1024)
                except: pass
            if "BYTES" in v_upper:
                try: return (0, float(v_upper.replace("BYTES", "").strip()))
                except: pass
                
            # Time format "HH:MM:SS" or "MM:SS"
            if ":" in val_str:
                parts = val_str.split(":")
                try:
                    sec = 0
                    for p in parts:
                        sec = sec * 60 + float(p)
                    return (0, sec)
                except: pass
                
            # Bitrate ("320 kbps")
            if "KBPS" in v_upper:
                try: return (0, float(v_upper.replace("KBPS", "").strip()))
                except: pass
                
            return (1, val_str.lower())
            
        try:
            data.sort(key=lambda t: extract_key(t[0]), reverse=descending)
        except Exception:
            data.sort(key=lambda t: str(t[0]).lower(), reverse=descending)
            
        for index, (val, child) in enumerate(data):
            self.move(child, '', index)
            
        # Update column header sort indicator arrows (▲ / ▼)
        for c in self.columns:
            if c == col:
                arrow = " ▼" if descending else " ▲"
                self.heading(c, text=f"{c}{arrow}")
            else:
                self.heading(c, text=c)
                
        self.refresh_stripes()
        
        if getattr(self, 'on_sort_callback', None):
            self.on_sort_callback(col, descending)

    def on_drag_drop_complete(self, source_ids, target_id):
        """Called by DraggableTreeHelper when a drop happens."""
        # Default behavior: Move source to index of target
        if not source_ids or target_id in source_ids: return
        
        try:
            # We want to drop insert "before" the target usually
            target_index = self.index(target_id)
            for i, sid in enumerate(source_ids):
                self.move(sid, '', target_index + i)
            self.refresh_stripes()
        except: pass
        
    def refresh_stripes(self):
        for i, item in enumerate(self.get_children()):
            stripe_tag = 'even' if i % 2 == 0 else 'odd'
            curr_tags = list(self.item(item, 'tags') or [])
            other_tags = [t for t in curr_tags if t not in ('even', 'odd', 'evenrow', 'oddrow')]
            self.item(item, tags=tuple([stripe_tag] + other_tags))

    def get_all_items(self):
        """Returns list of children IIDs."""
        return self.get_children()

    def auto_fit_columns(self, max_width=800, padding=25):
        """Automatically adjust column widths based on contents and headers."""
        self.auto_fit_all_columns(max_width=max_width, padding=padding)

    def auto_fit_column(self, col, max_width=800, min_width=40, padding=25):
        """Adjusts a single column width to optimal size."""
        import tkinter.font as tkfont
        try:
            font_name = self.tk.call("ttk::style", "lookup", "Treeview", "-font")
            font_obj = tkfont.nametofont(font_name) if font_name else tkfont.nametofont("TkDefaultFont")
        except:
            font_obj = tkfont.nametofont("TkDefaultFont")

        max_w = font_obj.measure(str(col)) + padding
        for item in self.get_children():
            try:
                val = self.set(item, col)
                w = font_obj.measure(str(val)) + padding
                if w > max_w:
                    max_w = w
            except: pass
            
        self.column(col, width=max_w)
        if getattr(self, 'on_column_width_changed', None):
            try: self.on_column_width_changed()
            except: pass

    def auto_fit_all_columns(self, max_width=800, min_width=40, padding=25):
        """Adjusts all visible columns to optimal size."""
        visible = self.get_visible_columns()
        for col in visible:
            self.auto_fit_column(col, max_width=max_width, min_width=min_width, padding=padding)
        if getattr(self, 'on_column_width_changed', None):
            try: self.on_column_width_changed()
            except: pass

    def get_column_widths(self):
        """Returns dict of {col_name: current_width_in_px} for all defined columns."""
        widths = {}
        for c in self.columns:
            try:
                w = self.column(c, 'width')
                if w: widths[c] = int(w)
            except: pass
        return widths

    def set_column_widths(self, widths_dict):
        """Restores saved column widths from a dict."""
        if not isinstance(widths_dict, dict): return
        for c, w in widths_dict.items():
            if c in self.columns:
                try:
                    self.column(c, width=int(w))
                except: pass

    def get_visible_columns(self):
        disp = self.cget("displaycolumns")
        if not disp or disp == "#all" or disp == ("#all",):
            return list(self.columns)
        return list(disp)

    def set_visible_columns(self, visible_cols):
        valid = [c for c in visible_cols if c in self.columns]
        if not valid:
            valid = list(self.columns)
        self.configure(displaycolumns=valid)

    def enable_header_context_menu(self, on_change_callback=None):
        """Enables Windows Explorer-style header right-click menu."""
        self.header_change_callback = on_change_callback
        self.bind("<Button-3>", self._show_header_context_menu, add="+")

    def _show_header_context_menu(self, event):
        clicked_col = self.identify_column(event.x)
        
        # Resolve column name if clicked
        target_col = None
        if clicked_col:
            try:
                col_idx = int(clicked_col.replace("#", "")) - 1
                vis = self.get_visible_columns()
                if 0 <= col_idx < len(vis):
                    target_col = vis[col_idx]
            except: pass

        # Determine theme colors from UniversalApp or parent
        top = self.winfo_toplevel()
        is_dark = getattr(top, 'is_dark', True)
        if not hasattr(top, 'is_dark'):
            try:
                bg = top.cget("bg")
                is_dark = bg.lower() not in ("#f3f3f3", "#f0f0f0", "white", "systembuttonface")
            except:
                is_dark = True
                
        if is_dark:
            menu_bg = "#252526"
            menu_fg = "#cccccc"
            active_bg = "#007acc"
            active_fg = "#ffffff"
            sel_color = "#007acc"
        else:
            menu_bg = "#fdfdfd"
            menu_fg = "#222222"
            active_bg = "#007acc"
            active_fg = "#ffffff"
            sel_color = "#007acc"

        s = getattr(top, 'scale_factor', 1.0)
        menu_font = ("Microsoft JhengHei UI", max(9, int(9.5 * s)))
        
        menu = tk.Menu(
            self,
            tearoff=0,
            bg=menu_bg,
            fg=menu_fg,
            activebackground=active_bg,
            activeforeground=active_fg,
            selectcolor=sel_color,
            bd=1,
            relief="solid",
            font=menu_font
        )
        
        lbl_fit_col = _i18n_text("tree_menu_fit_col", "調整「{col}」至最適大小(S)").replace("{col}", str(target_col or ""))
        lbl_fit_all = _i18n_text("tree_menu_fit_all", "調整所有欄位至最適大小(A)")
        
        if target_col:
            menu.add_command(
                label=lbl_fit_col,
                command=lambda c=target_col: self.auto_fit_column(c)
            )
        menu.add_command(
            label=lbl_fit_all,
            command=self.auto_fit_all_columns
        )
        menu.add_separator()
        
        visible_list = self.get_visible_columns()
        visible_set = set(visible_list)
        
        self._col_vars = {}
        for col in self.columns:
            var = tk.BooleanVar(value=(col in visible_set))
            self._col_vars[col] = var
            
            def make_cmd(c_name, v):
                def cmd():
                    curr = list(self.get_visible_columns())
                    if v.get():
                        if c_name not in curr:
                            # Insert in natural column order
                            curr = [c for c in self.columns if c in curr or c == c_name]
                    else:
                        if len(curr) > 1 and c_name in curr:
                            curr.remove(c_name)
                        else:
                            v.set(True) # Force stay on if only 1 column left
                    self.set_visible_columns(curr)
                    if getattr(self, 'header_change_callback', None):
                        self.header_change_callback(curr)
                return cmd
                
            heading_txt = self.heading(col, "text") or col
            menu.add_checkbutton(label=heading_txt, variable=var, command=make_cmd(col, var))
            
        try:
            menu.tk_popup(event.x_root, event.y_root)
        finally:
            menu.grab_release()

    def show_column_selector(self):
        """Pops up the column selector context menu at treeview position."""
        class _FakeEvent:
            pass
        e = _FakeEvent()
        try:
            e.x = max(10, self.winfo_width() // 2)
            e.y = 10
            e.x_root = self.winfo_rootx() + e.x
            e.y_root = self.winfo_rooty() + e.y
        except Exception:
            e.x = 100
            e.y = 10
            e.x_root = 200
            e.y_root = 200
        self._show_header_context_menu(e)



class ProfileManagerUI(ttk.LabelFrame):
    def __init__(self, parent, config, on_reload=None):
        super().__init__(parent, text="個人化參數組合 (Profiles)", padding=15)
        self.config = config
        self.on_reload = on_reload
        
        btn_row1 = ttk.Frame(self)
        btn_row1.pack(fill="x", pady=(0, 5))
        ttk.Button(btn_row1, text="💾 儲存目前的參數組合", command=self._save_profile).pack(side="left", fill="x", expand=True, padx=(0, 5))
        ttk.Button(btn_row1, text="📂 載入參數組合", command=self._load_profile).pack(side="left", fill="x", expand=True)
        
        btn_row2 = ttk.Frame(self)
        btn_row2.pack(fill="x", pady=(5, 0))
        ttk.Button(btn_row2, text="🔄 恢復原廠預設值", command=self._restore_defaults).pack(fill="x")
        
    def _save_profile(self):
        from tkinter import simpledialog
        name = simpledialog.askstring("儲存參數組合", "請輸入參數組合名稱：\n(例如：DJ_Mix_Mode, Podcast_Mode)", parent=self)
        if not name or not name.strip(): return
        name = name.strip()
        
        if self.config.save_profile(name):
            showinfo("成功", f"已成功儲存參數組合：{name}", parent=self)
        else:
            showerror("錯誤", "儲存失敗，請檢查權限或檔名是否合法。", parent=self)
            
    def _load_profile(self):
        profiles = self.config.list_profiles()
        if not profiles:
            showinfo("提示", "目前沒有已儲存的參數組合。", parent=self)
            return
            
        top = tk.Toplevel(self)
        top.title("載入參數組合")
        top.geometry("300x400")
        top.transient(self.winfo_toplevel())
        top.grab_set()
        
        apply_title_bar_theme(top, True)
        top.configure(bg="#1e1e1e")
        
        ttk.Label(top, text="請選擇要載入的參數組合：", foreground="#cccccc", background="#1e1e1e").pack(pady=10)
        
        lb = tk.Listbox(top, bg="#2d2d2d", fg="white", selectbackground="#007acc")
        lb.pack(fill="both", expand=True, padx=15, pady=5)
        for p in profiles: lb.insert(tk.END, p)
        
        def on_select():
            sel = lb.curselection()
            if not sel: return
            p_name = lb.get(sel[0])
            if askyesno("確認", f"確定要載入參數組合 '{p_name}' 嗎？\n當前未儲存的設定將被覆寫。", parent=top):
                if self.config.load_profile(p_name):
                    if self.on_reload: self.on_reload()
                    showinfo("成功", "已載入參數組合。", parent=self)
                    top.destroy()
        
        btn_frame = ttk.Frame(top)
        btn_frame.pack(fill="x", pady=10, padx=15)
        ttk.Button(btn_frame, text="載入", command=on_select).pack(side="right", padx=(5,0))
        ttk.Button(btn_frame, text="取消", command=top.destroy).pack(side="right")
        
    def _restore_defaults(self):
        if askyesno("警告", "確定要恢復原廠預設值嗎？\n所有參數將被重置為初始狀態。", parent=self):
            self.config.restore_factory_defaults()
            if self.on_reload: self.on_reload()
            showinfo("成功", "已恢復原廠預設值。", parent=self)
