# -*- coding: utf-8 -*-
"""
UniversalUI Power & Display Management Engine (power_engine)
------------------------------------------------------------
Provides unified display power management, screen-off with click debounce,
countdown dialogs, and display sleep prevention for long-running batch jobs.
"""

import os
import sys
import time
import ctypes
import threading
import subprocess
import tkinter as tk
from tkinter import ttk

# Re-export safe i18n lookup
def _i18n_text(key, default=""):
    try:
        from _lib import i18n
        res = i18n.t(key)
        if res and res != key:
            return res
    except Exception:
        pass
    return default


def turn_off_screen(lock=False, delay_sec=0.35):
    """
    Turns off physical monitor / display via Win32 DPMS (or platform equivalent).
    
    :param lock: If True, locks the workstation (Win+L).
    :param delay_sec: Brief delay to ensure physical mouse release / up-click 
                      does not immediately wake up the screen.
    """
    if delay_sec > 0:
        time.sleep(delay_sec)
        
    if lock:
        try:
            if sys.platform == "win32":
                ctypes.windll.user32.LockWorkStation()
            elif sys.platform == "darwin":
                subprocess.run(["pmset", "displaysleepnow"], check=False)
            else:
                # Linux / RuhOS: Try standard FreeDesktop loginctl or xdg-screensaver
                for lock_cmd in [
                    ["loginctl", "lock-session"],
                    ["xdg-screensaver", "lock"],
                    ["swaylock"],
                    ["hyprlock"],
                    ["gnome-screensaver-command", "-l"]
                ]:
                    try:
                        subprocess.run(lock_cmd, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                        break
                    except Exception:
                        pass
        except Exception:
            pass
            
    if sys.platform == "win32":
        try:
            HWND_BROADCAST = 0xFFFF
            WM_SYSCOMMAND = 0x0112
            SC_MONITORPOWER = 0xF170
            # 2 = Power off display, 1 = Low power standby, -1 = Power on
            ctypes.windll.user32.SendMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2)
            return True
        except Exception as e:
            print(f"[PowerEngine] Error turning off display: {e}")
            return False
    elif sys.platform == "darwin":
        try:
            subprocess.run(["pmset", "displaysleepnow"], check=False)
            return True
        except Exception:
            return False
    else:
        # Linux / RuhOS: Comprehensive Wayland, X11, D-Bus, and sysfs kernel power-down
        linux_off_methods = [
            # 1. Hyprland (RuhOS modern Wayland compositor)
            ["hyprctl", "dispatch", "dpms", "off"],
            # 2. Sway / Wayfire / wlroots
            ["swaymsg", "output * power off"],
            # 3. Generic Wayland wlopm
            ["wlopm", "--off", "*"],
            # 4. Standard X11 / Xwayland DPMS
            ["xset", "dpms", "force", "off"],
            # 5. GNOME Mutter D-Bus
            ["busctl", "call", "org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig", "SetPowerSaveMode", "i", "3"],
            # 6. KDE Plasma Solid D-Bus
            ["qdbus", "org.kde.Solid.PowerManagement", "/org/kde/Solid/PowerManagement/Actions/BrightnessControl", "setBrightness", "0"]
        ]
        
        success = False
        for cmd in linux_off_methods:
            try:
                res = subprocess.run(cmd, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if res.returncode == 0:
                    success = True
                    break
            except Exception:
                continue
                
        # 7. Sysfs Kernel Direct Backlight Driver (Embedded / Kiosk RuhOS devices)
        if not success:
            try:
                import glob
                for bl in glob.glob("/sys/class/backlight/*/bl_power"):
                    with open(bl, "w") as f:
                        f.write("1\n") # 1 = Power off backlight
                success = True
            except Exception:
                pass
                
        return success


def wake_screen():
    """Wakes the physical display / monitor across Windows, Linux/RuhOS, and macOS."""
    if sys.platform == "win32":
        try:
            ctypes.windll.user32.mouse_event(0x0001, 0, 1, 0, 0)
            ctypes.windll.user32.mouse_event(0x0001, 0, -1, 0, 0)
            ES_CONTINUOUS = 0x80000000
            ES_DISPLAY_REQUIRED = 0x00000002
            ES_SYSTEM_REQUIRED = 0x00000001
            ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)
            return True
        except Exception as e:
            print(f"[PowerEngine] Error waking display: {e}")
            return False
    elif sys.platform == "darwin":
        try:
            subprocess.run(["caffeinate", "-u", "-t", "1"], check=False)
            return True
        except Exception:
            return False
    else:
        # Linux / RuhOS wake
        linux_wake_methods = [
            ["hyprctl", "dispatch", "dpms", "on"],
            ["swaymsg", "output * power on"],
            ["wlopm", "--on", "*"],
            ["xset", "dpms", "force", "on"],
            ["busctl", "call", "org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig", "SetPowerSaveMode", "i", "0"]
        ]
        for cmd in linux_wake_methods:
            try:
                res = subprocess.run(cmd, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if res.returncode == 0:
                    break
            except Exception:
                continue
        try:
            import glob
            for bl in glob.glob("/sys/class/backlight/*/bl_power"):
                with open(bl, "w") as f:
                    f.write("0\n") # 0 = Power on backlight
        except Exception:
            pass
        return True


def prevent_screen_sleep(enable=True):
    """
    Informs OS that a heavy background task (e.g. video render, mixdown, batch export)
    is active and the system/display should not automatically go to sleep.
    """
    if sys.platform == "win32":
        try:
            ES_CONTINUOUS = 0x80000000
            ES_SYSTEM_REQUIRED = 0x00000001
            ES_DISPLAY_REQUIRED = 0x00000002
            if enable:
                ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
            else:
                ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS)
            return True
        except Exception:
            return False
    elif sys.platform == "darwin":
        # macOS caffeinate
        return True
    else:
        # Linux / RuhOS: systemd-inhibit or xdg-screensaver / xset
        try:
            if enable:
                subprocess.run(["xset", "s", "off", "-dpms"], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            else:
                subprocess.run(["xset", "s", "on", "+dpms"], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return True
        except Exception:
            return False


def allow_screen_sleep():
    """Restores standard OS power management and sleep timers."""
    return prevent_screen_sleep(enable=False)


# ----------------------------------------------------------------------
# Universal Countdown Dialog for Batch Apps
# ----------------------------------------------------------------------
class ScreenOffCountdownDialog(tk.Toplevel):
    """
    Universal Dark/Light Modal Dialog that counts down before turning off the screen.
    Can be summoned by any app when a long batch job completes.
    """
    def __init__(self, parent, countdown_sec=30, lock=False, on_complete=None, on_cancel=None, title=None, message=None):
        super().__init__(parent)
        self.parent = parent
        self.countdown_sec = int(countdown_sec)
        self.rem_sec = int(countdown_sec)
        self.lock = lock
        self.on_complete = on_complete
        self.on_cancel = on_cancel
        self.cancelled = False
        self.timer_job = None
        
        # Scale and theme detection
        top = parent.winfo_toplevel() if parent else self
        app = getattr(top, "universal_app_instance", None)
        self.scale_factor = getattr(app, "scale_factor", 1.0)
        self.is_dark = getattr(app, "is_dark", True)
        
        # Title
        win_title = title or _i18n_text("dlg_screen_off_title", "關閉螢幕節能倒數 (Screen Off)")
        self.title(win_title)
        
        s = self.scale_factor
        w = int(460 * s)
        h = int(220 * s)
        self.geometry(f"{w}x{h}")
        self.minsize(int(400 * s), int(190 * s))
        self.resizable(True, True)
        self.attributes("-topmost", True)
        
        # Center relative to parent
        try:
            from _lib.theme_engine import apply_title_bar_theme
            apply_title_bar_theme(self, is_dark=self.is_dark)
        except Exception:
            pass
            
        self._setup_ui(message)
        self._center_window(parent, w, h)
        self.grab_set()
        
        self.bind("<Escape>", lambda e: self.cancel_action())
        self.bind("<Return>", lambda e: self.do_action_now())
        
        self.after(1000, self._tick)

    def _center_window(self, parent, width, height):
        try:
            if parent and parent.winfo_exists() and parent.winfo_viewable():
                rx = parent.winfo_rootx()
                ry = parent.winfo_rooty()
                rw = parent.winfo_width()
                rh = parent.winfo_height()
                x = rx + (rw - width) // 2
                y = ry + (rh - height) // 2
            else:
                x = (self.winfo_screenwidth() - width) // 2
                y = (self.winfo_screenheight() - height) // 2
            self.geometry(f"+{max(10, x)}+{max(10, y)}")
        except Exception:
            pass

    def _setup_ui(self, message):
        s = self.scale_factor
        bg_card = "#1e2227" if self.is_dark else "#f0f2f5"
        fg_title = "#f6ad55" if self.is_dark else "#dd6b20"
        fg_msg = "#cbd5e0" if self.is_dark else "#4a5568"
        fg_cnt = "#00e5ff" if self.is_dark else "#007acc"
        
        self.config(bg=bg_card)
        f = ttk.Frame(self, padding=max(10, int(14 * s)))
        f.pack(fill="both", expand=True)
        
        # Header title
        hdr_txt = "🖥️ " + _i18n_text("dlg_screen_off_header", "工作已完成，即將自動關閉螢幕電源")
        self.lbl_title = tk.Label(
            f, text=hdr_txt, bg=bg_card, fg=fg_title,
            font=("Microsoft JhengHei UI", max(9, int(11 * s)), "bold"),
            wraplength=int(420 * s), justify="center"
        )
        self.lbl_title.pack(anchor="center", pady=(0, 4))
        
        # Detail / Slogan
        msg_txt = message or _i18n_text("dlg_screen_off_desc", "關閉螢幕可大幅節省電力並消除亮光干擾，晃動滑鼠即可喚醒。")
        self.lbl_msg = tk.Label(
            f, text=msg_txt, bg=bg_card, fg=fg_msg,
            font=("Microsoft JhengHei UI", max(8, int(9 * s))),
            wraplength=int(420 * s), justify="center"
        )
        self.lbl_msg.pack(anchor="center", pady=(0, 6))
        
        # Countdown Text
        self.lbl_countdown = tk.Label(
            f, text=self._format_countdown_str(), bg=bg_card, fg=fg_cnt,
            font=("Consolas", max(13, int(16 * s)), "bold"),
            justify="center"
        )
        self.lbl_countdown.pack(anchor="center", pady=(0, 6))
        
        # Progress Bar
        self.prog = ttk.Progressbar(f, maximum=self.countdown_sec, value=self.countdown_sec)
        self.prog.pack(fill="x", pady=(0, 12))
        
        # Buttons Bar
        btn_bar = ttk.Frame(f)
        btn_bar.pack(side="bottom", fill="x")
        btn_bar.columnconfigure(0, weight=1)
        btn_bar.columnconfigure(1, weight=1)
        btn_bar.columnconfigure(2, weight=1)
        
        btn_cancel_txt = _i18n_text("btn_cancel", "✋ 取消定時 (Esc)")
        btn_ext_txt = _i18n_text("btn_extend_5m", "⏱️ 延長 5 分鐘")
        btn_now_txt = _i18n_text("btn_off_now", "⚡ 立即關閉")
        
        ttk.Button(btn_bar, text=btn_cancel_txt, command=self.cancel_action).grid(row=0, column=0, padx=2, sticky="ew")
        ttk.Button(btn_bar, text=btn_ext_txt, command=self.extend_action).grid(row=0, column=1, padx=2, sticky="ew")
        ttk.Button(btn_bar, text=btn_now_txt, command=self.do_action_now).grid(row=0, column=2, padx=2, sticky="ew")

    def _format_countdown_str(self):
        m, s = divmod(self.rem_sec, 60)
        return f"⏳ {m:02d}:{s:02d}"

    def _tick(self):
        if not self.winfo_exists() or self.cancelled:
            return
            
        if self.rem_sec <= 0:
            self.do_action_now()
            return
            
        self.rem_sec -= 1
        self.lbl_countdown.config(text=self._format_countdown_str())
        self.prog.config(value=self.rem_sec)
        self.timer_job = self.after(1000, self._tick)

    def cancel_action(self):
        self.cancelled = True
        if self.timer_job:
            try: self.after_cancel(self.timer_job)
            except Exception: pass
        if callable(self.on_cancel):
            try: self.on_cancel()
            except Exception: pass
        self.destroy()

    def extend_action(self, add_sec=300):
        self.rem_sec += add_sec
        self.countdown_sec += add_sec
        self.prog.config(maximum=self.countdown_sec, value=self.rem_sec)
        self.lbl_countdown.config(text=self._format_countdown_str())

    def do_action_now(self):
        self.cancelled = True
        if self.timer_job:
            try: self.after_cancel(self.timer_job)
            except Exception: pass
        self.destroy()
        
        if callable(self.on_complete):
            try: self.on_complete()
            except Exception: pass
            
        def _worker():
            turn_off_screen(lock=self.lock, delay_sec=0.35)
        threading.Thread(target=_worker, daemon=True).start()


def start_screen_off_timer(parent, countdown_sec=30, lock=False, on_complete=None, on_cancel=None, title=None, message=None):
    """Convenience helper to launch the ScreenOff countdown dialog from any widget/app."""
    return ScreenOffCountdownDialog(
        parent=parent,
        countdown_sec=countdown_sec,
        lock=lock,
        on_complete=on_complete,
        on_cancel=on_cancel,
        title=title,
        message=message
    )
