# -*- coding: utf-8 -*-
"""
RuhI (如喜) 輸入法 — 純 Rust 原生設定中心轉發器 (Native Rust Studio Forwarder)
==============================================================================
自 v0.3.0 起，偏好設定中心與語意調校工作室已 100% 遷移至純 Rust 原生執行檔 `ruhi-studio.exe`。
本檔案僅作為向下相容轉發入口：若任何舊版工作階段或外部腳本呼叫 `settings_window.py`，
將立即以無主控台視窗模式 (CREATE_NO_WINDOW) 喚起 `ruhi-studio.exe` 並即刻退出。
"""

import os
import sys
import subprocess
from pathlib import Path


def main() -> int:
    here = Path(__file__).resolve().parent
    candidates = [
        here / "ruhi-studio.exe",
        here.parent / "ruhi-studio.exe",
        here / "ruhi-studio" / "target" / "release" / "ruhi-studio.exe",
        here.parent / "ruhi-studio" / "target" / "release" / "ruhi-studio.exe",
        here / "dist" / "ruhi-studio.exe",
        here.parent / "dist" / "ruhi-studio.exe",
        Path(r"C:\ATGprjs\ruhi\ruhi-studio.exe"),
        Path(r"C:\ATGprjs\ruhi\ruhi-studio\target\release\ruhi-studio.exe"),
    ]

    for exe in candidates:
        if exe.is_file():
            flags = getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000) if sys.platform == "win32" else 0
            subprocess.Popen(
                [str(exe)] + sys.argv[1:],
                cwd=str(exe.parent),
                creationflags=flags,
            )
            return 0

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
