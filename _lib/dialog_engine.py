# -*- coding: utf-8 -*-
"""
==============================================================================
ATG DIALOG & MODAL ENGINE (現代化對話框與通知引擎)
==============================================================================
提供沉浸式深淺色主題對話框、非阻塞懸浮 Toast 通知、
以及開箱即用的訊息對話盒包裝函式。
"""

import os
import sys
import tkinter as tk
from tkinter import ttk
from _lib.theme_engine import apply_title_bar_theme


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
    高質感主題自適應對話框 (Universal MessageBox)
    """
    def __init__(self, parent=None, title="訊息", message="", icon="info", buttons="ok", default_btn="ok", scale_factor=1.0):
        super().__init__(parent)
        self.result = None
        self.withdraw()
        
        is_dark = True
        if parent:
            is_dark = getattr(parent, 'is_dark', True)
            if not hasattr(parent, 'is_dark'):
                try:
                    bg = parent.cget("bg")
                    is_dark = bg.lower() not in ("#f3f3f3", "#f0f0f0", "white", "systembuttonface")
                except Exception:
                    is_dark = True
                    
        bg_main = "#1e1e1e" if is_dark else "#fdfdfd"
        fg_main = "#cccccc" if is_dark else "#222222"
        border_col = "#3f3f46" if is_dark else "#d1d5db"
        
        self.title(title)
        self.configure(bg=bg_main)
        self.resizable(False, False)
        
        try:
            self.transient(parent)
            self.grab_set()
        except Exception:
            pass
            
        apply_title_bar_theme(self, dark=is_dark)
        
        s = scale_factor or 1.0
        font_msg = ("Microsoft JhengHei UI", max(9, int(10 * s)))
        
        container = tk.Frame(self, bg=bg_main, padx=max(16, int(24 * s)), pady=max(16, int(20 * s)))
        container.pack(fill="both", expand=True)
        
        content_f = tk.Frame(container, bg=bg_main)
        content_f.pack(fill="both", expand=True, pady=(0, max(12, int(18 * s))))
        
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
        
        text_f = tk.Frame(content_f, bg=bg_main)
        text_f.pack(side="left", fill="both", expand=True)
        
        lbl_msg = tk.Label(
            text_f, text=message, bg=bg_main, fg=fg_main,
            font=font_msg, justify="left", anchor="w",
            wraplength=max(320, int(460 * s))
        )
        lbl_msg.pack(fill="both", expand=True)
        
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
            self.bind("<Return>", lambda e: _on_action(True))
            self.bind("<Escape>", lambda e: _on_action(None))
            if default_btn == 'no': b_no.focus_set()
            elif default_btn == 'cancel': b_can.focus_set()
            else: b_yes.focus_set()
            
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
            self.bind("<Return>", lambda e: _on_action(True))
            self.bind("<Escape>", lambda e: _on_action(False))
            if default_btn == 'no': b_no.focus_set()
            else: b_yes.focus_set()
            
        elif buttons == "okcancel":
            b_ok = ttk.Button(btn_f, text=txt_ok, command=lambda: _on_action(True), width=btn_w)
            b_can = ttk.Button(btn_f, text=txt_can, command=lambda: _on_action(False), width=btn_w)
            b_can.pack(side="right", padx=(max(4, int(6 * s)), 0))
            b_ok.pack(side="right")
            self._buttons['ok'] = b_ok
            self._buttons['cancel'] = b_can
            self.bind("<Return>", lambda e: _on_action(True))
            self.bind("<Escape>", lambda e: _on_action(False))
            if default_btn == 'cancel': b_can.focus_set()
            else: b_ok.focus_set()
            
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
        self.deiconify()

    def show(self):
        self.wait_window(self)
        return self.result


def showinfo(title="提示", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="info", buttons="ok", **kwargs)
    return dlg.show()

def showwarning(title="警告", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="warning", buttons="ok", **kwargs)
    return dlg.show()

def showerror(title="錯誤", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="error", buttons="ok", **kwargs)
    return dlg.show()

def askyesno(title="確認", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="question", buttons="yesno", **kwargs)
    return dlg.show()

def askyesnocancel(title="確認", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="question", buttons="yesnocancel", **kwargs)
    return dlg.show()

def askokcancel(title="確認", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="question", buttons="okcancel", **kwargs)
    return dlg.show()

def askretrycancel(title="重試", message="", parent=None, **kwargs):
    dlg = UniversalMessageBox(parent=parent, title=title, message=message, icon="warning", buttons="retrycancel", **kwargs)
    return dlg.show()

def askquestion(title="確認", message="", parent=None, **kwargs):
    res = askyesno(title=title, message=message, parent=parent, **kwargs)
    return 'yes' if res else 'no'


class UniversalToast(tk.Toplevel):
    """
    非阻塞懸浮 Toast 通知視窗 (Non-blocking Floating Toast)
    """
    def __init__(self, parent, message, level="info", duration=3000):
        super().__init__(parent)
        self.overrideredirect(True)
        self.attributes("-topmost", True)
        self.attributes("-alpha", 0.0)
        
        colors = {
            "info": ("#007acc", "white"),
            "success": ("#28a745", "white"),
            "warning": ("#ffc107", "black"),
            "error": ("#dc3545", "white")
        }
        bg, fg = colors.get(level, colors["info"])
        self.configure(bg=bg)
        
        icons = {"info": "ℹ️", "success": "✅", "warning": "⚠️", "error": "❌"}
        icon = icons.get(level, "🔔")
        
        lbl = tk.Label(self, text=f"{icon} {message}", bg=bg, fg=fg, 
                       padx=20, pady=10, font=("Microsoft JhengHei UI", 10, "bold"))
        lbl.pack()
        self.update_idletasks()
        
        try:
            px = parent.winfo_rootx()
            py = parent.winfo_rooty()
            pw = parent.winfo_width()
            ph = parent.winfo_height()
            w = self.winfo_width()
            h = self.winfo_height()
            x = px + pw - w - 20
            y = py + ph - h - 20
            self.geometry(f"+{x}+{y}")
        except Exception:
            self.geometry("+100+100")
            
        self._fade_in()
        self.after(duration, self._fade_out)

    def _fade_in(self):
        alpha = self.attributes("-alpha")
        if alpha < 0.95:
            self.attributes("-alpha", alpha + 0.1)
            self.after(20, self._fade_in)

    def _fade_out(self):
        alpha = self.attributes("-alpha")
        if alpha > 0.0:
            self.attributes("-alpha", alpha - 0.1)
            self.after(20, self._fade_out)
        else:
            self.destroy()


def show_toast(parent, message, level="info", duration=3000):
    """彈出 Toast 通知"""
    return UniversalToast(parent, message, level=level, duration=duration)
# ==============================================================================
# 🏛️ UNIVERSAL DIALOG STANDARD BASE CLASS (標準通用彈出視窗基底)
# ==============================================================================
class UniversalDialog(tk.Toplevel):
    """
    ATG 全域統一對話框標準基底類別 (Standard Universal Dialog Base)
    
    [ 核心職責 (Core Responsibilities) ]
    1. 🎨 主題與色彩繼承：自動繼承母視窗深淺色、DPI 縮放比例、原生暗色標題列。
    2. 🌐 多語系自動連動 (Dynamic i18n)：支援 bind_i18n 與 retranslate_ui 生命週期。
    3. 🧠 自適應預設值直通 (PresetEngine Direct Integration)：開箱即用常用歷史記錄與動態按鈕。
    4. 📐 rud() 標準按鈕底欄：自動等比伸展、防截字破版、預設綁定 Enter/Esc 快速鍵。
    5. 💾 幾何位置記憶與置中：首次開啟智慧置中於母視窗，關閉自動持久化保存尺寸。
    6. 📄 CJK 自適應折行 (Smart Wrap)：視窗縮放自動重算折行與兩端對齊。
    """
    def __init__(
        self,
        parent,
        title_key_or_str="dlg_title",
        state_id=None,
        default_size=(500, 400),
        minsize=(380, 260),
        resizable=(True, True),
        is_modal=True,
        **kwargs
    ):
        self.title_key = title_key_or_str
        self.is_modal = is_modal
        self.dialog_state_id = state_id
        
        super().__init__(parent, **kwargs)
        
        # 1. Resolve localized title
        initial_title = _i18n_text(title_key_or_str, default=title_key_or_str)
        self.title(initial_title)
        
        # 2. Inherit theme and scaling from parent
        top = parent.winfo_toplevel() if (parent and hasattr(parent, 'winfo_toplevel')) else parent
        self.scale_factor = getattr(top, 'scale_factor', 1.0)
        self.is_dark = getattr(top, 'is_dark', True)
        # Safely resolve app_config (avoiding Tkinter widget .config() method)
        cfg = getattr(top, "universal_app_config", None)
        if cfg is None:
            parent_cfg = getattr(parent, "config", None)
            if parent_cfg is not None and not callable(parent_cfg) and hasattr(parent_cfg, "get"):
                cfg = parent_cfg
            elif hasattr(parent, "app_config"):
                cfg = getattr(parent, "app_config")
        self.app_config = cfg
        
        s = self.scale_factor
        self.bg_main = "#1e222b" if self.is_dark else "#f8f9fa"
        self.fg_main = "#e2e8f0" if self.is_dark else "#2d3748"
        self.bg_card = "#252b36" if self.is_dark else "#ffffff"
        self.accent_color = "#00e5ff" if self.is_dark else "#0078d4"
        
        # Win32 transient ownership
        if top and top != self:
            try: self.transient(top)
            except Exception: pass
            
        try: self.configure(bg=self.bg_main)
        except Exception: pass
        
        # Apply dark/light title bar
        self.after(10, lambda: apply_title_bar_theme(self, self.is_dark))
        
        self.resizable(resizable[0], resizable[1])
        min_w = int(minsize[0] * s)
        min_h = int(minsize[1] * s)
        self.minsize(min_w, min_h)
        
        # 3. Geometry & Centering
        has_saved_geo = False
        if state_id and self.app_config:
            saved_geo = self.app_config.get(f"window_{state_id}")
            if saved_geo:
                try:
                    self.geometry(saved_geo)
                    has_saved_geo = True
                except Exception:
                    pass
                
        if not has_saved_geo:
            w = int(default_size[0] * s)
            h = int(default_size[1] * s)
            if top and top.winfo_viewable():
                px = top.winfo_rootx()
                py = top.winfo_rooty()
                pw = top.winfo_width()
                ph = top.winfo_height()
                x = px + max(0, (pw - w) // 2)
                y = py + max(0, (ph - h) // 2)
                self.geometry(f"{w}x{h}+{x}+{y}")
            else:
                sw = self.winfo_screenwidth()
                sh = self.winfo_screenheight()
                x = max(0, (sw - w) // 2)
                y = max(0, (sh - h) // 2)
                self.geometry(f"{w}x{h}+{x}+{y}")
                
        # 4. Preset Engine Direct Access
        try:
            from _lib.preset_engine import get_preset_engine
            self.preset_engine = get_preset_engine(self.app_config)
        except Exception:
            self.preset_engine = None
            
        # 5. Dynamic i18n & RWD Wrap Tracking
        self._i18n_bindings = [] # [(widget, key, prop, fmt_kwargs)]
        self._wrap_labels = []   # [(widget, raw_text_or_key)]
        
        # 6. Bind Configure for Smart Text-Wrap
        self.bind("<Configure>", self._on_dialog_configure)
        
        # 7. Zoom Shortcuts
        self.bind("<Control-plus>", self._on_zoom_in)
        self.bind("<Control-equal>", self._on_zoom_in)
        self.bind("<Control-minus>", self._on_zoom_out)
        self.bind("<Control-0>", self._on_zoom_reset)
        
        # 8. Close protocol
        self.protocol("WM_DELETE_WINDOW", self.on_close)
        
        # 9. Modal Grab if requested
        if self.is_modal:
            try: self.grab_set()
            except Exception: pass

    def bind_i18n_text(self, widget, key, prop="text", **fmt_kwargs):
        """Binds a widget property to an i18n key for dynamic live re-translation."""
        initial_val = _i18n_text(key, default=key)
        if fmt_kwargs:
            try: initial_val = initial_val.format(**fmt_kwargs)
            except Exception: pass
            
        try:
            widget.config(**{prop: initial_val})
        except Exception:
            pass
        self._i18n_bindings.append((widget, key, prop, fmt_kwargs))

    def register_wrap_label(self, widget, text_or_key=""):
        """Registers a label for automatic responsive CJK text wrapping on resize."""
        self._wrap_labels.append((widget, text_or_key))

    def retranslate_ui(self):
        """
        Refreshes all bound i18n texts and updates the window title.
        Subclasses can override to perform additional custom re-translations.
        """
        # 1. Update Title
        new_title = _i18n_text(self.title_key, default=self.title_key)
        self.title(new_title)
        
        # 2. Update all bound widgets
        for item in self._i18n_bindings:
            try:
                widget, key, prop, kwargs = item
                if widget.winfo_exists():
                    val = _i18n_text(key, default=key)
                    if kwargs:
                        val = val.format(**kwargs)
                    widget.config(**{prop: val})
            except Exception:
                pass

    def create_button_bar(
        self,
        parent,
        ok_cmd=None,
        cancel_cmd=None,
        ok_key="btn_ok",
        cancel_key="btn_cancel",
        extra_buttons=None,
        default_enter="ok"
    ):
        """
        Creates a rud() compliant, responsive bottom button bar.
        - Automatically handles equal-width scaling or side alignment.
        - Automatically binds Enter / Escape shortcuts.
        - Uses i18n keys for standard labels.
        """
        s = getattr(self, 'scale_factor', 1.0)
        btn_bar = ttk.Frame(parent)
        btn_bar.pack(side="bottom", fill="x", pady=(int(10 * s), 0))
        
        # Left Extra Buttons
        if extra_buttons:
            for btn_cfg in extra_buttons:
                txt = _i18n_text(btn_cfg.get("key", ""), default=btn_cfg.get("text", ""))
                cmd = btn_cfg.get("command")
                b = ttk.Button(btn_bar, text=txt, command=cmd)
                b.pack(side="left", padx=(0, int(4 * s)))
                
        # Right Actions (Cancel / OK)
        if cancel_cmd or cancel_key:
            c_cmd = cancel_cmd or self.on_close
            c_txt = _i18n_text(cancel_key, default="取消")
            b_can = ttk.Button(btn_bar, text=c_txt, command=c_cmd)
            b_can.pack(side="right", padx=(int(6 * s), 0))
            self.bind("<Escape>", lambda e: c_cmd())
            
        if ok_cmd or ok_key:
            o_cmd = ok_cmd or self.on_close
            o_txt = _i18n_text(ok_key, default="確定")
            style_name = "Accent.TButton" if "Accent.TButton" in ttk.Style().theme_names() else "TButton"
            b_ok = ttk.Button(btn_bar, text=o_txt, command=o_cmd, style=style_name)
            b_ok.pack(side="right")
            if default_enter == "ok":
                self.bind("<Return>", lambda e: o_cmd())
                
        return btn_bar

    def _on_dialog_configure(self, event):
        if event.widget == self:
            s = getattr(self, 'scale_factor', 1.0)
            calc_wrap = max(200, event.width - int(48 * s))
            for item in self._wrap_labels:
                try:
                    if isinstance(item, tuple):
                        lbl, raw_text_or_key = item
                        if lbl.winfo_exists():
                            txt = _i18n_text(raw_text_or_key, default=raw_text_or_key) if raw_text_or_key else lbl.cget("text")
                            try:
                                from _lib.responsive_engine import cjk_smart_wrap
                                wrapped = cjk_smart_wrap(txt, max_width_px=calc_wrap)
                                lbl.config(text=wrapped)
                            except Exception:
                                lbl.config(wraplength=calc_wrap)
                    elif hasattr(item, 'winfo_exists') and item.winfo_exists():
                        item.config(wraplength=calc_wrap)
                except Exception:
                    pass

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
        if self.dialog_state_id and self.app_config and hasattr(self.app_config, "set"):
            try:
                self.app_config.set(f"window_{self.dialog_state_id}", self.geometry())
                if hasattr(self.app_config, "save"):
                    self.app_config.save()
            except Exception: pass
        self.destroy()


def reset_all_dialog_geometries(config_store) -> int:
    """
    Clears all saved custom dialog geometries (window_* keys) from ConfigStore,
    restoring all dialogs to their default auto-adaptive centering relative to the main window.
    """
    if not config_store:
        return 0
    cleared = 0
    target_dict = getattr(config_store, 'data', {})
    if not isinstance(target_dict, dict) and hasattr(config_store, 'get_all'):
        target_dict = config_store.get_all()
        
    keys_to_del = [k for k in list(target_dict.keys()) if str(k).startswith("window_") and str(k) not in ("window_main", "window_geometry")]
    for k in keys_to_del:
        try:
            if hasattr(config_store, "delete"):
                config_store.delete(k)
            elif hasattr(config_store, "data") and k in config_store.data:
                del config_store.data[k]
            cleared += 1
        except Exception:
            pass
            
    if hasattr(config_store, "save"):
        try: config_store.save()
        except Exception: pass
    return cleared
