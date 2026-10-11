"""
Ruhi (如喜) Input Method Framework - Windows 11 全域接管輸入法服務 (精準緩衝區判定版)
ruhi_win11.ruhi_win11_service - Win32 全域按鍵攔截與 pynput 直通注入中樞
"""

import os
import sys
import time
import queue
import ctypes
import tkinter as tk
from pynput import keyboard
from typing import Optional

# 加入專案路徑
current_dir = os.path.dirname(os.path.abspath(__file__))
ruhi_root = os.path.dirname(current_dir)
ruhos_root = os.path.dirname(ruhi_root)

for p in [ruhi_root, ruhos_root, "c:/ATGprjs", "c:/ATGprjs/RuhOS", "C:/ATGprjs/ruhi"]:
    if p not in sys.path:
        sys.path.insert(0, p)

try:
    from ruhi.ruhi_brain.engine import RuhiEngine, RuhiEngineMode
    from ruhi.ruhi_win11.win32_input_sender import Win32InputSender
    from ruhi.ruhi_win11.ruhi_win_overlay import RuhiWinOverlay
    from ruhi.ruhi_win11.target_corrector import TargetTextCorrector
except ImportError:
    from ruhi_brain.engine import RuhiEngine, RuhiEngineMode
    from ruhi_win11.win32_input_sender import Win32InputSender
    from ruhi_win11.ruhi_win_overlay import RuhiWinOverlay
    from ruhi_win11.target_corrector import TargetTextCorrector

# Win32 常數
WM_KEYDOWN = 0x0100
WM_KEYUP = 0x0101
WM_SYSKEYDOWN = 0x0104

VK_SHIFT = 0x10
VK_LSHIFT = 0xA0
VK_RSHIFT = 0xA1
VK_SPACE = 0x20
VK_RETURN = 0x0D
VK_BACK = 0x08
VK_ESCAPE = 0x1B
VK_OEM_3 = 0xC0 # ` / ~
VK_OEM_COMMA = 0xBC # ,
VK_OEM_PERIOD = 0xBE # .
VK_OEM_2 = 0xBF # / or ?
VK_OEM_1 = 0xBA # ; or :
VK_OEM_4 = 0xDB # [ or {
VK_OEM_6 = 0xDD # ] or }

LLKHF_INJECTED = 0x00000001

class RuhiWin11Service:
    """Windows 11 全域輸入法守護進程"""
    def __init__(self):
        self.engine = RuhiEngine()
        self.root = tk.Tk()
        self.overlay = RuhiWinOverlay(self.root)
        self.target_corrector = TargetTextCorrector()
        
        self.is_chinese_mode = True  # 中文 (許氏) / 英文 模式
        self.listener: Optional[keyboard.Listener] = None
        self.event_queue = queue.Queue()

    def _has_active_buffer(self) -> bool:
        """精確檢查 Ruhi 當前是否有未結算的注音、整句或候選字"""
        return bool(
            self.engine.fsm.has_pending() or
            self.engine.fsm.syllables or
            self.engine.composed_sentence or
            self.engine.current_candidates or
            self.engine.mode != RuhiEngineMode.NORMAL
        )

    def start(self):
        print("=" * 68)
        print(" 🚀 Ruhi (如喜) Windows 11 全域輸入法服務已啟動！")
        print("=" * 68)
        print("  • 模式：許氏鍵盤雙拼 (16 萬詞庫 + 本機 Gemma AI)")
        print("  • 支援全域視窗：LINE, Word, Chrome, 記事本, VSCode...")
        print("  • 【Shift】鍵：快速切換 中文[許氏] / 英文 模式")
        print("  • 【Q】鍵：原地輪換候選字 (連續 5 下展開選字窗)")
        print("  • 【`】(反引號)：按 1 下文字範本 | 按 2 下數字快打")
        print("  • 【~】(Shift+`)：專業詞庫切換")
        print("  • 【Ctrl+Alt+C】：一鍵 AI 語意校正工作目標文字 (選取或最後一句)")
        print("=" * 68)
        print(" [狀態] 當前模式：【中文 (許氏雙拼)】，請在任何視窗直接敲鍵盤！\n")

        # 啟動 pynput 鍵盤監聽器
        self.listener = keyboard.Listener(
            win32_event_filter=self._win32_event_filter,
            suppress=False
        )
        self.listener.start()

        # 啟動 Tkinter 佇列處理
        self._check_queue()
        try:
            self.root.mainloop()
        except KeyboardInterrupt:
            print("\n👋 Ruhi 輸入法服務正在安全退出...")
        finally:
            if self.listener:
                self.listener.stop()

    def _win32_event_filter(self, msg, data):
        """Win32 底層鍵盤事件過濾器"""
        # 忽略由 SendInput 產生的注入按鍵，直接放行
        if data.flags & LLKHF_INJECTED:
            return True

        # 只處理 KeyDown / SysKeyDown
        if msg not in (WM_KEYDOWN, WM_SYSKEYDOWN):
            # 若為 KeyUp 且處於打字處理中，攔截 KeyUp 防止殘留
            if self.is_chinese_mode and self._is_typing_key(data.vkCode):
                if self.listener:
                    self.listener.suppress_event()
                return False
            return True

        vk = data.vkCode

        # 0. 處理全域熱鍵 Ctrl + Alt + C (一鍵 AI 校正工作目標文字)
        if vk == 0x43 and self._is_ctrl_down() and self._is_alt_down():
            self.event_queue.put(("ai_correct_target", None))
            if self.listener:
                self.listener.suppress_event()
            return False

        # 1. 處理 Shift 鍵 (中/英切換)
        if vk in (VK_SHIFT, VK_LSHIFT, VK_RSHIFT):
            if not self._has_active_buffer():
                self.is_chinese_mode = not self.is_chinese_mode
                mode_str = "【中文 (許氏雙拼)】" if self.is_chinese_mode else "【純英文】"
                print(f" 🔄 [模式切換] ➔ {mode_str}")
                self.event_queue.put(("mode_change", None))
            return True

        # 若目前為純英文模式，100% 全部放行
        if not self.is_chinese_mode:
            return True

        # 2. 處理 Esc 鍵 (清空緩衝區)
        if vk == VK_ESCAPE:
            if self._has_active_buffer():
                self.event_queue.put(("reset", None))
                if self.listener:
                    self.listener.suppress_event()
                return False
            return True

        # 3. 虛擬鍵碼轉換為字元
        key_char = self._vk_to_char(vk)
        if key_char is None:
            return True

        # 4. 檢查是否有進行中的文字緩衝區
        has_buffer = self._has_active_buffer()

        # 若目前完全無字：
        if not has_buffer:
            # 只有在遇到許氏鍵 (A-Z) 或標點/反引號時才攔截
            if key_char.isalpha() or key_char in ('`', '~', ',', '.', '?', '!', ';', '[', ']'):
                pass
            else:
                return True # 放行一般獨立空白、Enter、數字

        # 5. 主動壓制 (Suppress) 此按鍵，防止英文字母或 Enter 漏進目標視窗！
        if self.listener:
            self.listener.suppress_event()

        self.event_queue.put(("key", key_char))
        return False

    def _is_typing_key(self, vk: int) -> bool:
        if 0x41 <= vk <= 0x5A: # A-Z
            return True
        if self._has_active_buffer() and vk in (VK_SPACE, VK_RETURN, VK_BACK):
            return True
        return False

    def _check_queue(self):
        """在 Tkinter 主執行緒中處理按鍵與畫面刷新"""
        while not self.event_queue.empty():
            evt_type, val = self.event_queue.get_nowait()
            if evt_type == "reset":
                self.engine.reset_buffer()
                self.overlay.update_view(self.engine._build_state_response())
            elif evt_type == "mode_change":
                self.engine.reset_buffer()
                self.overlay.update_view(self.engine._build_state_response())
            elif evt_type == "ai_correct_target":
                print(" 🔍 [AI 目標文字校正] 收到觸發請求，正在分析工作目標文字...")
                def _on_done(action):
                    if action and action.result.has_corrections:
                        msg = f"已校正：{action.original_text} ➔ {action.corrected_text}"
                        print(f" ✨ [AI 校正完成] {msg}")
                        self.root.after(0, lambda: self.overlay.show_correction_toast("AI 校正完成", msg))
                    elif action:
                        print(" ✅ [AI 校正檢查] 文字完全正確，無須修改。")
                        self.root.after(0, lambda: self.overlay.show_correction_toast("AI 檢查通過", "文字正確無瑕"))
                    else:
                        print(" ⚠️ [AI 校正] 未捕獲到任何可校正的文字。")
                self.target_corrector.correct_target_async(auto_apply=True, callback=_on_done)
            elif evt_type == "key":
                res = self.engine.process_key(val)
                # 若有確定文字產出，直接調用 SendInput 打入作用中視窗
                if res["action"].startswith("commit"):
                    committed = res["committed_chunk"]
                    # 登記至最近送出歷史隊列
                    self.target_corrector.record_committed_text(committed)
                    self.overlay.update_view(self.engine._build_state_response())
                    Win32InputSender.send_unicode_string(committed)
                else:
                    self.overlay.update_view(res)

        # 每 10ms 輪詢一次
        self.root.after(10, self._check_queue)

    def _vk_to_char(self, vk: int) -> Optional[str]:
        if 0x41 <= vk <= 0x5A:
            return chr(vk).lower()
        if 0x30 <= vk <= 0x39:
            return chr(vk)
        if vk == VK_SPACE:
            return " "
        if vk == VK_RETURN:
            return "\n"
        if vk == VK_BACK:
            return "\b"
        if vk == VK_OEM_3:
            return "~" if self._is_shift_down() else "`"
        if vk == VK_OEM_COMMA:
            return ","
        if vk == VK_OEM_PERIOD:
            return "."
        if vk == VK_OEM_2:
            return "?" if self._is_shift_down() else "/"
        if vk == VK_OEM_1:
            return ":" if self._is_shift_down() else ";"
        if vk == VK_OEM_4:
            return "{" if self._is_shift_down() else "["
        if vk == VK_OEM_6:
            return "}" if self._is_shift_down() else "]"
        return None

    def _is_shift_down(self) -> bool:
        user32 = ctypes.windll.user32
        return (user32.GetAsyncKeyState(VK_SHIFT) & 0x8000) != 0

    def _is_ctrl_down(self) -> bool:
        user32 = ctypes.windll.user32
        return (user32.GetAsyncKeyState(0x11) & 0x8000) != 0

    def _is_alt_down(self) -> bool:
        user32 = ctypes.windll.user32
        return (user32.GetAsyncKeyState(0x12) & 0x8000) != 0

def main():
    service = RuhiWin11Service()
    service.start()

if __name__ == "__main__":
    main()
