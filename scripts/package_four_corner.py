#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 智慧型四角號碼 4×5 數字小鍵盤 獨立解耦打包工具
================================================================================
本工具負責將「如喜智慧型四角號碼輸入法」打包為兩套可獨立散佈之發行套件：
1. `dist/RuhI_FourCorner_Numpad_Win64.zip`
   - Windows 10/11 (x64) 4×5 數字小鍵盤獨立版（預設開箱即為 `"four_corner"` 模式，
     內含 `ruhi_tsf.dll`、`ruhi-studio.exe`、`four_corner.tsv`、語境轉移矩陣及 UTF-8 BOM `.ps1` 安裝腳本）。
2. `dist/RuhI_FourCorner_CrossPlatform_Schema.zip`
   - 跨平台 (Linux Fcitx5 / IBus Rime、Android Trime / Fcitx5-Android、macOS Squirrel)
     標準方案與「前兩角簡碼矩陣」詞庫包。
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass


INSTALL_PS1_CONTENT = """. "C:\\ATGprjs\\_lib\\ps\\Ensure-Admin.ps1"
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  如喜 (RuhI) 智慧型四角號碼輸入法 (4×5 數字小鍵盤) 安裝程序" -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

$SrcDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$DllPath = Join-Path $SrcDir "ruhi_tsf.dll"

if (-not (Test-Path $DllPath)) {
    Write-Host "[錯誤] 找不到 ruhi_tsf.dll：$DllPath" -ForegroundColor Red
    exit 1
}

Write-Host "[1/2] 正在向 Windows TSF 框架註冊智慧型四角號碼引擎..." -ForegroundColor Yellow
& regsvr32.exe /s "$DllPath"
if ($LASTEXITCODE -ne 0) {
    Write-Host "[錯誤] regsvr32 註冊失敗 (代碼: $LASTEXITCODE)" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "[2/2] 註冊完成！預設模式已設定為「四 (智慧型四角號碼 4×5 小鍵盤)」。" -ForegroundColor Green
Write-Host "      - 單字輸入：取左上、右上、左下、右下 4 碼（如 2360 = 台、3212 = 灣）"
Write-Host "      - 二字詞簡碼（4碼）：首字(1,2) + 尾字(1,2)（如 2332 = 台灣、0044 = 主權、1072 = 電腦）"
Write-Host "      - 三／四字詞簡碼（5/6碼）：首字(1,3) + 中字(1)×n + 尾字(2,4)（如 50843 = 輸入法、622612 = 四角號碼）"
Write-Host "      - 小鍵盤全盲打閉環：+ 字詞分界收攏至輸入座、Enter 整句完稿送出、* 進入選字模式、/ 標點面板、- 退格、. 萬用字元"
"""

UNINSTALL_PS1_CONTENT = """. "C:\\ATGprjs\\_lib\\ps\\Ensure-Admin.ps1"
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$SrcDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$DllPath = Join-Path $SrcDir "ruhi_tsf.dll"

if (Test-Path $DllPath) {
    Write-Host "正在從 Windows TSF 解除註冊 ruhi_tsf.dll ..." -ForegroundColor Yellow
    & regsvr32.exe /u /s "$DllPath"
    Write-Host "已成功解除安裝如喜智慧型四角號碼輸入法。" -ForegroundColor Green
} else {
    Write-Host "找不到 ruhi_tsf.dll，無須解除註冊。" -ForegroundColor Yellow
}
"""

SWITCH_KEYMAP_PS1_CONTENT = """param(
    [ValidateSet("blind_closed_loop", "direct_punct")]
    [string]$Profile = "blind_closed_loop"
)
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$SrcDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$CfgPath = Join-Path $SrcDir "config.json"

if (-not (Test-Path $CfgPath)) {
    Write-Host "[錯誤] 找不到 config.json：$CfgPath" -ForegroundColor Red
    exit 1
}

$JsonRaw = Get-Content -Path $CfgPath -Raw -Encoding UTF8
$CfgObj = $JsonRaw | ConvertFrom-Json
$CfgObj | Add-Member -MemberType NoteProperty -Name "four_corner_keymap" -Value $Profile -Force
$OutJson = $CfgObj | ConvertTo-Json -Depth 10
[System.IO.File]::WriteAllText($CfgPath, $OutJson, (New-Object System.Text.UTF8Encoding($false)))

Write-Host "[完成] 已將 4×5 數字小鍵盤外圍符號鍵行為切換為：$Profile" -ForegroundColor Green
"""


def verify_ps1_syntax(ps1_path: Path) -> None:
    cmd = [
        "powershell.exe",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        f"& {{ Get-Command -Syntax '{ps1_path}' }}",
    ]
    res = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if res.returncode != 0:
        raise RuntimeError(f"PowerShell 5.1 語法自檢失敗 ({ps1_path}):\n{res.stderr}\n{res.stdout}")


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    dist_dir = repo_root / "dist"
    staging_win = dist_dir / "RuhI_FourCorner_Numpad_Win64"
    staging_schema = dist_dir / "four_corner_schema"

    # 1. 確保 four_corner.tsv 與跨平台方案已由編譯器產出
    fc_tsv = repo_root / "ruhi_lexicon" / "data" / "four_corner.tsv"
    if not fc_tsv.exists():
        subprocess.run([sys.executable, str(repo_root / "scripts" / "build_four_corner_tsv.py")], check=True)

    if staging_win.exists():
        shutil.rmtree(staging_win)
    staging_win.mkdir(parents=True, exist_ok=True)
    (staging_win / "data").mkdir(parents=True, exist_ok=True)

    # 2. 複製核心執行檔與 DLL
    dll_candidates = [
        repo_root / "ruhi-tsf" / "target" / "release" / "ruhi_tsf.dll",
        repo_root / "scripts" / "ruhi_tsf.dll",
        repo_root / "dist" / "ruhi_tsf.dll",
    ]
    dll_src = next((p for p in dll_candidates if p.exists()), None)
    if dll_src:
        shutil.copy2(dll_src, staging_win / "ruhi_tsf.dll")
        # 同步更新 scripts/ruhi_tsf.dll 與 dist/ruhi_tsf.dll
        for target_dll in [repo_root / "scripts" / "ruhi_tsf.dll", repo_root / "dist" / "ruhi_tsf.dll"]:
            try:
                if target_dll.parent.exists():
                    shutil.copy2(dll_src, target_dll)
            except PermissionError:
                pass

    studio_candidates = [
        repo_root / "ruhi-studio" / "target" / "release" / "ruhi-studio.exe",
        repo_root / "dist" / "ruhi-studio.exe",
    ]
    studio_src = next((p for p in studio_candidates if p.exists()), None)
    if studio_src:
        shutil.copy2(studio_src, staging_win / "ruhi-studio.exe")

    # 3. 複製四角號碼核心碼表與語境轉移資料檔
    data_files = [
        "four_corner.tsv",
        "transitions_user.tsv",
        "symbols.tsv",
        "lexicon_mcbopomofo.tsv",
    ]
    for df in data_files:
        src_f = repo_root / "ruhi_lexicon" / "data" / df
        if src_f.exists():
            shutil.copy2(src_f, staging_win / "data" / df)

    # 4. 產生預設以 `four_corner` 為核心之 config.json 與 settings_config.json
    standalone_config = {
        "keyboard_mode": "four_corner",
        "last_keyboard_mode": "english",
        "enabled_keyboard_modes": ["four_corner", "hsu"],
        "four_corner_keymap": "blind_closed_loop",
        "candidate_quick_key": "1234",
        "punctuation_commit_mode": "A",
        "input_display_mode": "smart_auto",
        "orientation": "horizontal",
        "font_size": 32,
        "candidate_font_size": 24,
        "desk_opacity": 95,
        "max_composition_chars": 30,
        "theme": "system",
        "ui_lang": "auto",
        "capslock_switch": "english_direct",
        "enable_mixed_english": True,
        "mixed_english_mode": "contextual_stream",
        "mixed_english_min_len": 4,
    }
    (staging_win / "config.json").write_text(
        json.dumps(standalone_config, ensure_ascii=False, indent=2),
        encoding="utf-8",
        newline="\n",
    )

    # 5. 寫入帶 UTF-8 BOM 之 PowerShell 腳本並強制執行語法自檢
    ps1_files = [
        (staging_win / "安裝_如喜智慧四角輸入法.ps1", INSTALL_PS1_CONTENT),
        (staging_win / "解除安裝_如喜智慧四角輸入法.ps1", UNINSTALL_PS1_CONTENT),
        (staging_win / "切換小鍵盤按鍵配置.ps1", SWITCH_KEYMAP_PS1_CONTENT),
    ]
    for ps1_path, content in ps1_files:
        ps1_path.write_text(content, encoding="utf-8-sig", newline="\r\n")
        verify_ps1_syntax(ps1_path)
        print(f"[語法自檢通過] {ps1_path.name} (UTF-8 with BOM)")

    # 6. 複製規格說明文件
    spec_doc = repo_root / "docs" / "RUHI_FOUR_CORNER_NUMPAD_SPEC.md"
    if spec_doc.exists():
        shutil.copy2(spec_doc, staging_win / "README_智慧型四角號碼輸入法.md")
        shutil.copy2(spec_doc, staging_schema / "README_智慧型四角號碼輸入法.md")

    # 7. 壓縮產生兩套 ZIP 發行包
    win_zip = dist_dir / "RuhI_FourCorner_Numpad_Win64.zip"
    with zipfile.ZipFile(win_zip, "w", zipfile.ZIP_DEFLATED) as zf:
        for file_path in sorted(staging_win.rglob("*")):
            if file_path.is_file():
                zf.write(file_path, file_path.relative_to(staging_win))

    schema_zip = dist_dir / "RuhI_FourCorner_CrossPlatform_Schema.zip"
    shutil.copy2(fc_tsv, staging_schema / "four_corner.tsv")
    with zipfile.ZipFile(schema_zip, "w", zipfile.ZIP_DEFLATED) as zf:
        for file_path in sorted(staging_schema.rglob("*")):
            if file_path.is_file():
                zf.write(file_path, file_path.relative_to(staging_schema))

    print("\n================ 獨立打包完成 ================")
    print(f"  1. Windows 64-bit 獨立版 : {win_zip} ({win_zip.stat().st_size / (1024*1024):.2f} MB)")
    print(f"  2. 跨平台方案與詞庫包    : {schema_zip} ({schema_zip.stat().st_size / (1024*1024):.2f} MB)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
