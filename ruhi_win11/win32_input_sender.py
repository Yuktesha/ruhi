"""
Ruhi (如喜) Input Method Framework - Windows 11 核心適配層
ruhi_win11.win32_input_sender - 原生 Unicode 文字注入器 (pynput + Win32 雙引擎)
"""

from pynput.keyboard import Controller, Key
import time

_controller = Controller()

class Win32InputSender:
    """將 Unicode 漢字與按鍵送入當前作用視窗"""

    @staticmethod
    def send_unicode_string(text: str):
        """將完整的 Unicode 字串（漢字、標點、Emoji）直接注入焦點視窗"""
        if not text:
            return
        # 使用 pynput 經過全球千錘百鍊的 Unicode 注入引擎
        _controller.type(text)

    @staticmethod
    def send_backspace(count: int = 1):
        """發送 Backspace 鍵"""
        for _ in range(count):
            _controller.press(Key.backspace)
            _controller.release(Key.backspace)

    @staticmethod
    def send_enter():
        """發送 Enter 鍵"""
        _controller.press(Key.enter)
        _controller.release(Key.enter)

    @staticmethod
    def send_ctrl_c():
        """發送 Ctrl+C (複製)"""
        with _controller.pressed(Key.ctrl):
            _controller.press('c')
            _controller.release('c')

    @staticmethod
    def send_ctrl_v():
        """發送 Ctrl+V (貼上)"""
        with _controller.pressed(Key.ctrl):
            _controller.press('v')
            _controller.release('v')

    @staticmethod
    def send_ctrl_z():
        """發送 Ctrl+Z (復原)"""
        with _controller.pressed(Key.ctrl):
            _controller.press('z')
            _controller.release('z')

    @staticmethod
    def send_shift_left(count: int = 1):
        """發送 Shift+Left (向左反白選取文字)"""
        with _controller.pressed(Key.shift):
            for _ in range(count):
                _controller.press(Key.left)
                _controller.release(Key.left)
