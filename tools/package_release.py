# -*- coding: utf-8 -*-
"""
RuhI (如喜) 輸入法 - GitHub 發布版自動化封裝工具 (Release Packager)
================================================================
依據 TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC.md 規格：
1. 驗證 ruhi_tsf.dll 之 PE 導入表（嚴格排除 comctl32.dll 與 TaskDialogIndirect 靜態依賴）
2. 匯總最新編譯產物 (ruhi_tsf.dll, ruhi.dll)
3. 封裝完整官方詞庫 (data/)、偏好設定中心、一鍵安裝/解除安裝腳本 (UTF-8 BOM)
4. 產生標準壓縮檔 RuhI_v<version>_Windows_x64.zip 與 SHA256 校驗碼
"""

import os
import sys
import shutil
import hashlib
import zipfile
from pathlib import Path

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

def calculate_sha256(filepath: Path) -> str:
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()

def verify_pe_imports(dll_path: Path):
    print(f"[*] 正在執行 PE 導入表審計: {dll_path}...")
    try:
        import pefile
    except ImportError:
        print("[!] 尚未安裝 pefile 模組，嘗試安裝...")
        import subprocess
        subprocess.check_call([sys.executable, "-m", "pip", "install", "pefile"])
        import pefile

    pe = pefile.PE(str(dll_path))
    imported_dlls = [entry.dll.decode("utf-8", "ignore").lower() for entry in getattr(pe, "DIRECTORY_ENTRY_IMPORT", [])]

    # 規範 1：導入表中絕對不得包含 comctl32.dll
    if "comctl32.dll" in imported_dlls:
        raise RuntimeError("【嚴重違規】PE 導入表包含 comctl32.dll 靜態依賴！會導致傳統 Win32 程式（如 conhost.exe、LibreOffice）載入失敗！")

    # 規範 2：絕對不得靜態導入 TaskDialogIndirect
    imported_funcs = []
    for entry in getattr(pe, "DIRECTORY_ENTRY_IMPORT", []):
        for imp in entry.imports:
            if imp.name:
                imported_funcs.append(imp.name.decode("utf-8", "ignore"))

    if any("taskdialog" in f.lower() for f in imported_funcs):
        raise RuntimeError("【嚴重違規】PE 導入表包含 TaskDialog 靜態進入點！必須於執行期動態獲取！")

    print("  [OK] PE 導入表審計通過！零 comctl32.dll 靜態依賴，安全相容全 Windows 生態。")

def main():
    root = Path(__file__).resolve().parent.parent
    version = "0.2.0"
    pkg_name = f"RuhI_v{version}_Windows_x64"
    dist_dir = root / "dist"
    release_dir = dist_dir / pkg_name

    print("========================================================")
    print(f"  正在打包 RuhI (如喜) 通用多語系智慧型輸入法 v{version}...")
    print("========================================================")

    # 1. 來源路徑檢查
    tsf_dll = root / "ruhi-tsf" / "target" / "release" / "ruhi_tsf.dll"
    core_dll = root / "ruhi-core" / "target" / "release" / "ruhi.dll"

    if not tsf_dll.exists():
        raise FileNotFoundError(f"找不到發布版 DLL: {tsf_dll}，請先執行 cargo build --release！")
    if not core_dll.exists():
        raise FileNotFoundError(f"找不到發布版 DLL: {core_dll}，請先執行 cargo build --release！")

    # 2. PE 導入表審核
    verify_pe_imports(tsf_dll)

    # 3. 準備發布暫存目錄
    if release_dir.exists():
        shutil.rmtree(release_dir)
    release_dir.mkdir(parents=True, exist_ok=True)

    # 複製核心二進位檔
    shutil.copy2(tsf_dll, release_dir / "ruhi_tsf.dll")
    shutil.copy2(core_dll, release_dir / "ruhi.dll")

    # 複製詞庫與語意轉移檔
    release_data = release_dir / "data"
    release_data.mkdir(parents=True, exist_ok=True)
    src_data = root / "ruhi_lexicon" / "data"
    for item in src_data.glob("*"):
        if item.is_file():
            shutil.copy2(item, release_data / item.name)
        elif item.is_dir():
            shutil.copytree(item, release_data / item.name, dirs_exist_ok=True)

    # 複製偏好設定與設定檔
    for f in ["settings_window.py", "settings_config.json", "config.json", "user_dict.tsv"]:
        src_f = dist_dir / f if (dist_dir / f).exists() else (root / f)
        if src_f.exists():
            shutil.copy2(src_f, release_dir / f)

    # 複製安裝與啟動腳本
    for script_name in ["一鍵安裝.ps1", "一鍵解除安裝.ps1", "run_settings.ps1", "hot_reload.ps1"]:
        src_s = dist_dir / script_name if (dist_dir / script_name).exists() else (root / "scripts" / script_name)
        if src_s.exists():
            shutil.copy2(src_s, release_dir / script_name)

    # 複製文件
    for doc in ["LICENSE", "README.md"]:
        if (root / doc).exists():
            shutil.copy2(root / doc, release_dir / doc)

    docs_dir = release_dir / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)
    for doc_name in [
        "TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC.md",
        "RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md",
        "RUHI_MULTILINGUAL_EXPANSION_GUIDE.md",
        "RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md",
        "RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md"
    ]:
        src_doc = root / "docs" / doc_name
        if src_doc.exists():
            shutil.copy2(src_doc, docs_dir / doc_name)

    print(f"[*] 已完成檔案封裝至暫存目錄: {release_dir}")

    # 4. 建立 ZIP 封裝檔
    zip_path = dist_dir / f"{pkg_name}.zip"
    if zip_path.exists():
        zip_path.unlink()

    print(f"[*] 正在建立發布壓縮檔: {zip_path}...")
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        for file_path in release_dir.rglob("*"):
            if file_path.is_file():
                arcname = file_path.relative_to(dist_dir)
                zf.write(file_path, arcname)

    # 5. 計算 SHA256 校驗碼
    sha256_val = calculate_sha256(zip_path)
    sha256_file = dist_dir / f"{pkg_name}.zip.sha256"
    with open(sha256_file, "w", encoding="utf-8") as f:
        f.write(f"{sha256_val} *{zip_path.name}\n")

    zip_size_mb = zip_path.stat().st_size / (1024 * 1024)
    print("========================================================")
    print("  【打包成功！】發布包與校驗檔已產出：")
    print(f"  壓縮檔案: {zip_path} ({zip_size_mb:.2f} MB)")
    print(f"  SHA256:   {sha256_val}")
    print(f"  校驗檔案: {sha256_file}")
    print("========================================================")

if __name__ == "__main__":
    main()
