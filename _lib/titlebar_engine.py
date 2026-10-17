# -*- coding: utf-8 -*-
"""
UniversalUI Modern Titlebar Engine (Chrome / Windows 11 Notepad Style)
----------------------------------------------------------------------
Provides a unified, ultra-efficient top titlebar where tabs, menu,
and window controls live on a single consolidated row.
Removes redundant OS caption & menu bar layers, freeing maximum vertical space.
"""

import os
import sys
import tkinter as tk
from tkinter import ttk

try:
    import ctypes
    from ctypes import wintypes
except Exception:
    ctypes = None

class FramelessWindowHelper:
    """
    Manages custom borderless window behavior for Tkinter:
    1. Taskbar presence (WS_EX_APPWINDOW) and Alt+Tab support.
    2. 8-direction smooth border resize grips (Top, Bottom, Left, Right, 4 Corners).
    3. Workarea-aware Maximize / Restore (preserves Windows taskbar).
    """
    def __init__(self, root, titlebar, min_w=600, min_h=400, on_close=None):
        self.root = root
        self.titlebar = titlebar
        self.min_w = min_w
        self.min_h = min_h
        self.on_close_cb = on_close or root.destroy
        self.is_maximized = False
        self.is_minimized = False
        self.prev_geometry = None
        self._hwnd = None  # Cached Win32 HWND
        
        # 1. Enable borderless
        self.root.overrideredirect(True)
        
        # 2. Hook into Windows Taskbar
        self.setup_taskbar_presence()
        
        # 3. Add 8-direction resize grips
        self.setup_resize_grips()
        
        # 4. Bind <Map> event to handle restore from minimized state
        self.root.bind("<Map>", self._on_map_restore, add="+")
        
    def setup_taskbar_presence(self):
        if not ctypes or sys.platform != "win32":
            return
        try:
            GWL_EXSTYLE = -20
            GWL_STYLE = -16
            WS_EX_APPWINDOW = 0x00040000
            WS_EX_WINDOWEDGE = 0x00000100
            WS_EX_TOOLWINDOW = 0x00000080
            WS_THICKFRAME = 0x00040000
            WS_MINIMIZEBOX = 0x00020000
            WS_MAXIMIZEBOX = 0x00010000
            WS_SYSMENU = 0x00080000
            
            hwnd = ctypes.windll.user32.GetParent(self.root.winfo_id())
            if not hwnd:
                hwnd = self.root.winfo_id()
            self._hwnd = hwnd  # Cache for minimize/restore reuse
                
            # 1. Register Win32 Styles for Taskbar & System Boundary Query
            style = ctypes.windll.user32.GetWindowLongW(hwnd, GWL_STYLE)
            style = style | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU
            ctypes.windll.user32.SetWindowLongW(hwnd, GWL_STYLE, style)
            
            ex_style = ctypes.windll.user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            ex_style = (ex_style & ~WS_EX_TOOLWINDOW) | WS_EX_APPWINDOW
            ctypes.windll.user32.SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style)
            
            # 2. Extend DWM Frame into Client Area (Enables native DWM Drop Shadow & ShareX bounds)
            class MARGINS(ctypes.Structure):
                _fields_ = [('cxLeftWidth', ctypes.c_int), ('cxRightWidth', ctypes.c_int),
                            ('cyTopHeight', ctypes.c_int), ('cyBottomHeight', ctypes.c_int)]
            margins = MARGINS(1, 1, 1, 1)
            ctypes.windll.dwmapi.DwmExtendFrameIntoClientArea(hwnd, ctypes.byref(margins))
            
            # 3. Windows 11 Native Rounded Corners (DWMWA_WINDOW_CORNER_PREFERENCE)
            try:
                DWMWA_WINDOW_CORNER_PREFERENCE = 33
                DWMWCP_ROUND = 2
                ctypes.windll.dwmapi.DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    ctypes.byref(ctypes.c_int(DWMWCP_ROUND)),
                    ctypes.sizeof(ctypes.c_int)
                )
            except Exception:
                pass
                
            # 4. Notify Windows DWM of Frame Changed so ShareX & OS get exact bounds
            SWP_FRAMECHANGED = 0x0020
            SWP_NOMOVE = 0x0002
            SWP_NOSIZE = 0x0001
            SWP_NOZORDER = 0x0004
            ctypes.windll.user32.SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER)
            
            self.root.withdraw()
            self.root.after(10, self.root.deiconify)
        except Exception:
            pass

    def setup_resize_grips(self):
        """Creates 8 border grips for fluid mouse resize in borderless mode."""
        grip_color = "#181a1f"
        grip_thickness = 4
        corner_size = 8
        
        self.grips = {}
        
        # Top, Bottom, Left, Right
        self.grips['t'] = tk.Frame(self.root, bg=grip_color, cursor="size_ns", height=grip_thickness)
        self.grips['b'] = tk.Frame(self.root, bg=grip_color, cursor="size_ns", height=grip_thickness)
        self.grips['l'] = tk.Frame(self.root, bg=grip_color, cursor="size_we", width=grip_thickness)
        self.grips['r'] = tk.Frame(self.root, bg=grip_color, cursor="size_we", width=grip_thickness)
        
        # 4 Corners
        self.grips['tl'] = tk.Frame(self.root, bg=grip_color, cursor="size_nw_se", width=corner_size, height=corner_size)
        self.grips['tr'] = tk.Frame(self.root, bg=grip_color, cursor="size_ne_sw", width=corner_size, height=corner_size)
        self.grips['bl'] = tk.Frame(self.root, bg=grip_color, cursor="size_ne_sw", width=corner_size, height=corner_size)
        self.grips['br'] = tk.Frame(self.root, bg=grip_color, cursor="size_nw_se", width=corner_size, height=corner_size)
        
        for name, grip in self.grips.items():
            grip.bind("<ButtonPress-1>", lambda e, d=name: self._on_resize_start(e, d))
            grip.bind("<B1-Motion>", self._on_resize_motion)
            
        self.root.bind("<Configure>", self._update_grip_positions, add="+")

    def _update_grip_positions(self, event=None):
        if self.is_maximized:
            for g in self.grips.values():
                g.place_forget()
            return
            
        w = self.root.winfo_width()
        h = self.root.winfo_height()
        if w < 10 or h < 10: return
        
        gt = 4
        cs = 8
        
        self.grips['t'].place(x=cs, y=0, width=w - 2 * cs, height=gt)
        self.grips['b'].place(x=cs, y=h - gt, width=w - 2 * cs, height=gt)
        self.grips['l'].place(x=0, y=cs, width=gt, height=h - 2 * cs)
        self.grips['r'].place(x=w - gt, y=cs, width=gt, height=h - 2 * cs)
        
        self.grips['tl'].place(x=0, y=0, width=cs, height=cs)
        self.grips['tr'].place(x=w - cs, y=0, width=cs, height=cs)
        self.grips['bl'].place(x=0, y=h - cs, width=cs, height=cs)
        self.grips['br'].place(x=w - cs, y=h - cs, width=cs, height=cs)

    def _on_resize_start(self, event, direction):
        self._resize_dir = direction
        self._start_x = event.x_root
        self._start_y = event.y_root
        self._start_win_x = self.root.winfo_x()
        self._start_win_y = self.root.winfo_y()
        self._start_win_w = self.root.winfo_width()
        self._start_win_h = self.root.winfo_height()

    def _on_resize_motion(self, event):
        dx = event.x_root - self._start_x
        dy = event.y_root - self._start_y
        
        x = self._start_win_x
        y = self._start_win_y
        w = self._start_win_w
        h = self._start_win_h
        
        if 'r' in self._resize_dir:
            w = max(self.min_w, self._start_win_w + dx)
        if 'l' in self._resize_dir:
            new_w = self._start_win_w - dx
            if new_w >= self.min_w:
                w = new_w
                x = self._start_win_x + dx
        if 'b' in self._resize_dir:
            h = max(self.min_h, self._start_win_h + dy)
        if 't' in self._resize_dir:
            new_h = self._start_win_h - dy
            if new_h >= self.min_h:
                h = new_h
                y = self._start_win_y + dy
                
        self.root.geometry(f"{int(w)}x{int(h)}+{int(x)}+{int(y)}")

    def minimize(self):
        """
        Properly minimize an overrideredirect(True) window using Win32 ShowWindow.
        Tkinter's root.iconify() is ILLEGAL on overrideredirect windows and will
        throw TclError or corrupt the window manager state. This method bypasses
        Tkinter entirely and uses the Win32 API directly.
        """
        if ctypes and sys.platform == "win32" and self._hwnd:
            try:
                SW_MINIMIZE = 6
                ctypes.windll.user32.ShowWindow(self._hwnd, SW_MINIMIZE)
                self.is_minimized = True
            except Exception:
                pass
        else:
            # Fallback for non-Windows: withdraw instead
            try:
                self.root.withdraw()
                self.is_minimized = True
            except Exception:
                pass

    def _on_map_restore(self, event=None):
        """
        Handles clean window restoration when the user Alt+Tabs back or clicks
        the taskbar icon. Resets the minimized flag and triggers a Win32
        geometry refresh to ensure the window redraws correctly without destroying HWND.
        """
        if event is not None and getattr(event, 'widget', None) != self.root:
            return
        if not self.is_minimized:
            return
        self.is_minimized = False
        
        def _safe_dwm_refresh():
            try:
                if ctypes and sys.platform == "win32" and self._hwnd:
                    SWP_NOMOVE = 0x0002
                    SWP_NOSIZE = 0x0001
                    SWP_NOZORDER = 0x0004
                    SWP_FRAMECHANGED = 0x0020
                    SWP_SHOWWINDOW = 0x0040
                    ctypes.windll.user32.SetWindowPos(
                        self._hwnd, 0, 0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW
                    )
                self._update_grip_positions()
            except Exception:
                pass
                
        try:
            self.root.after_idle(_safe_dwm_refresh)
        except Exception:
            _safe_dwm_refresh()

    def _get_frame_border_thickness(self):
        """
        Returns (border_x, border_y) — the invisible frame padding that
        WS_THICKFRAME adds around our overrideredirect window.
        Chrome / Notepad compensate for this by expanding the window rect
        outward by these amounts when maximized, pushing the invisible
        borders off-screen so the visible client area fills the work area exactly.
        """
        if not ctypes or sys.platform != "win32":
            return (0, 0)
        try:
            SM_CXSIZEFRAME = 32       # Horizontal resize border thickness
            SM_CYSIZEFRAME = 33       # Vertical resize border thickness
            SM_CXPADDEDBORDERWIDTH = 92  # DWM padding added on each side
            
            border_x = (ctypes.windll.user32.GetSystemMetrics(SM_CXSIZEFRAME)
                        + ctypes.windll.user32.GetSystemMetrics(SM_CXPADDEDBORDERWIDTH))
            border_y = (ctypes.windll.user32.GetSystemMetrics(SM_CYSIZEFRAME)
                        + ctypes.windll.user32.GetSystemMetrics(SM_CXPADDEDBORDERWIDTH))
            return (border_x, border_y)
        except Exception:
            return (8, 8)  # Safe fallback for typical 100% DPI

    def toggle_maximize(self):
        """Toggles between maximized state (within taskbar work area) and restored geometry with 100% Win32 precision."""
        try:
            hwnd = (ctypes.windll.user32.GetParent(self.root.winfo_id()) or self.root.winfo_id()) if ctypes else None
            
            if not self.is_maximized:
                # Save exact current window rectangle before maximizing
                if ctypes and sys.platform == "win32" and hwnd:
                    rect = wintypes.RECT()
                    ctypes.windll.user32.GetWindowRect(hwnd, ctypes.byref(rect))
                    self.prev_rect = (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
                else:
                    self.prev_geometry = self.root.geometry()

                # Get monitor work area (accurately detecting which monitor window is on, excluding Windows taskbar)
                if ctypes and sys.platform == "win32" and hwnd:
                    class RECT(ctypes.Structure):
                        _fields_ = [('left', ctypes.c_long), ('top', ctypes.c_long),
                                    ('right', ctypes.c_long), ('bottom', ctypes.c_long)]

                    class MONITORINFO(ctypes.Structure):
                        _fields_ = [('cbSize', ctypes.c_ulong),
                                    ('rcMonitor', RECT),
                                    ('rcWork', RECT),
                                    ('dwFlags', ctypes.c_ulong)]

                    MONITOR_DEFAULTTONEAREST = 2
                    hmonitor = ctypes.windll.user32.MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
                    mi = MONITORINFO()
                    mi.cbSize = ctypes.sizeof(MONITORINFO)
                    ctypes.windll.user32.GetMonitorInfoW(hmonitor, ctypes.byref(mi))
                    
                    work_x = mi.rcWork.left
                    work_y = mi.rcWork.top
                    work_w = mi.rcWork.right - mi.rcWork.left
                    work_h = mi.rcWork.bottom - mi.rcWork.top
                    
                    # Compensate for the invisible border added by WS_THICKFRAME:
                    # Expand the window rect outward by the border thickness so
                    # the invisible borders are pushed off-screen and the visible
                    # client area fills the work area edge-to-edge.
                    bx, by = self._get_frame_border_thickness()
                    
                    SWP_NOZORDER = 0x0004
                    SWP_FRAMECHANGED = 0x0020
                    ctypes.windll.user32.SetWindowPos(
                        hwnd, 0,
                        work_x - bx,          # Push left border off-screen
                        work_y - by,          # Push top border off-screen
                        work_w + 2 * bx,      # Expand width to cover both side borders
                        work_h + 2 * by,      # Expand height to cover top & bottom borders
                        SWP_NOZORDER | SWP_FRAMECHANGED
                    )
                else:
                    sw = self.root.winfo_screenwidth()
                    sh = self.root.winfo_screenheight()
                    self.root.geometry(f"{sw}x{sh - 40}+0+0")

                self.is_maximized = True
                if hasattr(self.titlebar, 'btn_max'):
                    self.titlebar.btn_max.config(text="🗗")
            else:
                if ctypes and sys.platform == "win32" and hwnd and hasattr(self, 'prev_rect') and self.prev_rect:
                    x, y, w, h = self.prev_rect
                    SWP_NOZORDER = 0x0004
                    SWP_FRAMECHANGED = 0x0020
                    ctypes.windll.user32.SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOZORDER | SWP_FRAMECHANGED)
                elif self.prev_geometry:
                    self.root.geometry(self.prev_geometry)
                    
                self.is_maximized = False
                if hasattr(self.titlebar, 'btn_max'):
                    self.titlebar.btn_max.config(text="🗖")
                    
            self._update_grip_positions()
        except Exception:
            pass


class ModernTitleBar(tk.Frame):
    """
    Unified Chrome / Notepad / Explorer Style Top Title Bar.
    Layout:
    [ Icon + Menu ▾ ] | [ Tab 1 ✕ ] [ Tab 2 ✕ ] [ ➕ ] ---- [ Drag Area / Title ] ---- [ 🗕 ] [ 🗖 ] [ ✕ ]
    """
    def __init__(self, parent, app, title="DJ Nyte 2.0", on_close_callback=None, show_controls=True, enable_frameless=False, show_menu_button=False, **kwargs):
        self.app = app
        self.root = app.root if hasattr(app, 'root') else parent
        self.is_dark = getattr(app, 'is_dark', True)
        self.on_close_callback = on_close_callback or (lambda: self.root.destroy())
        self.enable_frameless = enable_frameless
        self.show_controls = show_controls and enable_frameless
        self.show_menu_button = show_menu_button
        
        bg_color = kwargs.pop('bg', "#181a1f" if self.is_dark else "#e4e7eb")
        super().__init__(parent, bg=bg_color, **kwargs)
        
        self.s = getattr(app, 'scale_factor', 1.0)
        self._drag_start_x = 0
        self._drag_start_y = 0
        
        self._init_ui(title)
        if enable_frameless:
            self._bind_drag_and_actions()
        
        # Setup frameless helper if enabled
        if enable_frameless:
            self.frameless_helper = FramelessWindowHelper(self.root, self, min_w=int(600*self.s), min_h=int(450*self.s), on_close=self.on_close_callback)
        else:
            self.frameless_helper = None

    def _init_ui(self, title):
        s = self.s
        is_dark = self.is_dark
        bar_bg = "#181a1f" if is_dark else "#e4e7eb"
        text_fg = "#abb2bf" if is_dark else "#333333"
        accent_fg = "#00e5ff" if is_dark else "#007acc"
        
        # 1. Left Side: App Icon & Master Menu Button (Only if show_menu_button is True)
        if self.show_menu_button:
            self.left_box = tk.Frame(self, bg=bar_bg)
            self.left_box.pack(side="left", fill="y", padx=(int(6 * s), 2))
            
            self.lbl_icon = tk.Label(
                self.left_box,
                text="🎛️",
                bg=bar_bg,
                fg=accent_fg,
                font=("Segoe UI Emoji", max(10, int(12 * s))),
                cursor="hand2"
            )
            self.lbl_icon.pack(side="left", padx=(0, 2))
            
            self.btn_menu = tk.Label(
                self.left_box,
                text="選單 ▾",
                bg=bar_bg,
                fg=text_fg,
                font=("Microsoft JhengHei UI", max(8, int(9.5 * s)), "bold"),
                cursor="hand2",
                padx=int(6 * s),
                pady=int(3 * s)
            )
            self.btn_menu.pack(side="left", padx=(0, 4))
            
            self.btn_menu.bind("<Enter>", lambda e: self.btn_menu.config(bg="#2c313a" if self.is_dark else "#d0d4dc"))
            self.btn_menu.bind("<Leave>", lambda e: self.btn_menu.config(bg=bar_bg))
            self.btn_menu.bind("<Button-1>", self._popup_app_menu)
            self.lbl_icon.bind("<Button-1>", self._popup_app_menu)
            
            # Vertical Separator
            self.sep1 = tk.Frame(self, bg="#2d3139" if is_dark else "#c0c4cc", width=1)
            self.sep1.pack(side="left", fill="y", pady=int(6 * s), padx=int(4 * s))
        else:
            self.left_box = None
            self.btn_menu = None
            self.lbl_icon = None
            self.sep1 = None
        
        # 2. Window Controls on Far Right (🗕 🗖 ✕)
        if self.show_controls:
            self.ctrl_box = tk.Frame(self, bg=bar_bg)
            self.ctrl_box.pack(side="right", fill="y")
            
            font_ctrl = ("Segoe UI", max(8, int(10 * s)))
            
            # Close button (✕)
            self.btn_close = tk.Label(self.ctrl_box, text="✕", bg=bar_bg, fg=text_fg, font=font_ctrl, width=4, cursor="hand2")
            self.btn_close.pack(side="right", fill="y")
            self.btn_close.bind("<Enter>", lambda e: self.btn_close.config(bg="#e81123", fg="#ffffff"))
            self.btn_close.bind("<Leave>", lambda e: self.btn_close.config(bg=bar_bg, fg=text_fg))
            self.btn_close.bind("<Button-1>", lambda e: self.on_close_callback())
            
            # Maximize / Restore button (🗖 / 🗗)
            self.btn_max = tk.Label(self.ctrl_box, text="🗖", bg=bar_bg, fg=text_fg, font=font_ctrl, width=4, cursor="hand2")
            self.btn_max.pack(side="right", fill="y")
            self.btn_max.bind("<Enter>", lambda e: self.btn_max.config(bg="#2c313a" if self.is_dark else "#d0d4dc"))
            self.btn_max.bind("<Leave>", lambda e: self.btn_max.config(bg=bar_bg))
            self.btn_max.bind("<Button-1>", lambda e: self.toggle_maximize())
            
            # Minimize button (🗕)
            self.btn_min = tk.Label(self.ctrl_box, text="🗕", bg=bar_bg, fg=text_fg, font=font_ctrl, width=4, cursor="hand2")
            self.btn_min.pack(side="right", fill="y")
            self.btn_min.bind("<Enter>", lambda e: self.btn_min.config(bg="#2c313a" if self.is_dark else "#d0d4dc"))
            self.btn_min.bind("<Leave>", lambda e: self.btn_min.config(bg=bar_bg))
            self.btn_min.bind("<Button-1>", lambda e: self._do_minimize())

        # 3. Center Chrome-Style Tab Bar Container
        self.tab_container = tk.Frame(self, bg=bar_bg)
        self.tab_container.pack(side="left", fill="y", padx=2)
        
        # 4. Drag & Title Area (Spans remaining middle-right space)
        self.drag_area = tk.Frame(self, bg=bar_bg)
        self.drag_area.pack(side="left", fill="both", expand=True)
        
        self.lbl_title = tk.Label(
            self.drag_area,
            text=title,
            bg=bar_bg,
            fg="#5c6370" if is_dark else "#888888",
            font=("Microsoft JhengHei UI", max(7, int(8.5 * s)))
        )
        self.lbl_title.pack(side="right", padx=10, fill="y")

    def _bind_drag_and_actions(self):
        """Binds mouse drag on empty titlebar spaces to move window, and double click to maximize."""
        for w in (self, self.drag_area, self.lbl_title):
            w.bind("<ButtonPress-1>", self._on_drag_start)
            w.bind("<B1-Motion>", self._on_drag_motion)
            w.bind("<Double-Button-1>", lambda e: self.toggle_maximize())

    def _on_drag_start(self, event):
        self._drag_start_x = event.x_root - self.root.winfo_x()
        self._drag_start_y = event.y_root - self.root.winfo_y()

    def _on_drag_motion(self, event):
        if self.frameless_helper and self.frameless_helper.is_maximized:
            self.frameless_helper.toggle_maximize()
            self._drag_start_x = self.root.winfo_width() // 2
            self._drag_start_y = event.y
            
        new_x = event.x_root - self._drag_start_x
        new_y = event.y_root - self._drag_start_y
        self.root.geometry(f"+{new_x}+{new_y}")

    def toggle_maximize(self):
        if self.frameless_helper:
            self.frameless_helper.toggle_maximize()
        else:
            try:
                if self.root.state() == 'zoomed':
                    self.root.state('normal')
                else:
                    self.root.state('zoomed')
            except Exception:
                pass

    def _do_minimize(self):
        """Minimizes the window using Win32 API (safe for overrideredirect windows)."""
        if self.frameless_helper:
            self.frameless_helper.minimize()
        else:
            try:
                self.root.iconify()
            except Exception:
                pass

    def set_title(self, text):
        """Updates the subtle title text in the titlebar."""
        if hasattr(self, 'lbl_title'):
            self.lbl_title.config(text=text)

    def _popup_app_menu(self, event):
        """Pops up the application master menu below the menu button."""
        if hasattr(self.app, 'menubar') and self.app.menubar:
            try:
                x = self.btn_menu.winfo_rootx()
                y = self.btn_menu.winfo_rooty() + self.btn_menu.winfo_height()
                self.app.menubar.tk_popup(x, y)
            except Exception:
                pass

    def update_theme(self, is_dark):
        """Updates titlebar colors when theme changes."""
        self.is_dark = is_dark
        bar_bg = "#181a1f" if is_dark else "#e4e7eb"
        text_fg = "#abb2bf" if is_dark else "#333333"
        accent_fg = "#00e5ff" if is_dark else "#007acc"
        
        self.config(bg=bar_bg)
        self.left_box.config(bg=bar_bg)
        self.lbl_icon.config(bg=bar_bg, fg=accent_fg)
        self.btn_menu.config(bg=bar_bg, fg=text_fg)
        self.sep1.config(bg="#2d3139" if is_dark else "#c0c4cc")
        self.tab_container.config(bg=bar_bg)
        self.drag_area.config(bg=bar_bg)
        self.lbl_title.config(bg=bar_bg, fg="#5c6370" if is_dark else "#888888")
        
        if hasattr(self, 'ctrl_box'):
            self.ctrl_box.config(bg=bar_bg)
            self.btn_min.config(bg=bar_bg, fg=text_fg)
            self.btn_max.config(bg=bar_bg, fg=text_fg)
            self.btn_close.config(bg=bar_bg, fg=text_fg)
