# -*- coding: utf-8 -*-
"""
如喜（RuhI）倉頡＋英文零切換智慧混打 - Windows 獨立發行包組裝工具
"""
import os
import shutil
import zipfile

def package_cangjie():
    base_dir = r"C:\ATGprjs\ruhi"
    dist_dir = os.path.join(base_dir, "dist", "Ruhi_Cangjie_Win")
    os.makedirs(dist_dir, exist_ok=True)
    data_dir = os.path.join(dist_dir, "data")
    os.makedirs(data_dir, exist_ok=True)

    # 1. 複製 ruhi_tsf.dll
    tsf_src = os.path.join(base_dir, "ruhi-tsf", "target", "release", "ruhi_tsf.dll")
    if os.path.exists(tsf_src):
        shutil.copy2(tsf_src, os.path.join(dist_dir, "ruhi_tsf.dll"))
        print("[OK] Copied ruhi_tsf.dll")
    else:
        print("[FAIL] ruhi_tsf.dll not found")

    # 2. 複製 ruhi_cangjie_tester.exe
    tester_src = os.path.join(base_dir, "ruhi-core", "target", "release", "ruhi_cangjie_tester.exe")
    if os.path.exists(tester_src):
        shutil.copy2(tester_src, os.path.join(dist_dir, "ruhi_cangjie_tester.exe"))
        print("[OK] Copied ruhi_cangjie_tester.exe")
    else:
        print("[FAIL] ruhi_cangjie_tester.exe not found")

    # 3. 複製 data/cangjie.tsv 與 data/symbols.tsv
    cj_src = os.path.join(base_dir, "ruhi_lexicon", "data", "cangjie.tsv")
    if os.path.exists(cj_src):
        shutil.copy2(cj_src, os.path.join(data_dir, "cangjie.tsv"))
        print("[OK] Copied data/cangjie.tsv")

    sym_src = os.path.join(base_dir, "ruhi_lexicon", "data", "symbols.tsv")
    if os.path.exists(sym_src):
        shutil.copy2(sym_src, os.path.join(data_dir, "symbols.tsv"))
        print("[OK] Copied data/symbols.tsv")

    # 3.5 複製核心原始碼至 src_developer 目錄（供大飛與 AI 協作擴充使用）
    src_dev_dir = os.path.join(dist_dir, "src_developer")
    os.makedirs(src_dev_dir, exist_ok=True)
    cj_code_src = os.path.join(base_dir, "ruhi-core", "src", "cangjie")
    cj_code_dst = os.path.join(src_dev_dir, "cangjie")
    if os.path.exists(cj_code_dst):
        shutil.rmtree(cj_code_dst)
    if os.path.exists(cj_code_src):
        shutil.copytree(cj_code_src, cj_code_dst)
        print("[OK] Copied src_developer/cangjie/")

    tester_code_src = os.path.join(base_dir, "ruhi-core", "src", "bin", "cangjie_tester.rs")
    if os.path.exists(tester_code_src):
        shutil.copy2(tester_code_src, os.path.join(src_dev_dir, "cangjie_tester.rs"))
        print("[OK] Copied src_developer/cangjie_tester.rs")

    cargo_src = os.path.join(base_dir, "ruhi-core", "Cargo.toml")
    if os.path.exists(cargo_src):
        shutil.copy2(cargo_src, os.path.join(src_dev_dir, "Cargo.toml"))
        print("[OK] Copied src_developer/Cargo.toml")

    # 4. 寫入預設倉頡模式之 config.json
    config_path = os.path.join(dist_dir, "config.json")
    with open(config_path, "w", encoding="utf-8") as f:
        f.write('{\n')
        f.write('    "keyboard_mode": "cangjie",\n')
        f.write('    "candidate_quick_key": "asdf",\n')
        f.write('    "half_full_width": "half",\n')
        f.write('    "theme": "dark",\n')
        f.write('    "font_size": 32,\n')
        f.write('    "cjk_font": "Microsoft JhengHei UI",\n')
        f.write('    "punctuation_commit_mode": "B",\n')
        f.write('    "input_display_mode": "floating_follow"\n')
        f.write('}\n')
    print("[OK] Generated config.json")

    # 5. 產生 一鍵安裝.bat (帶 BOM UTF-8)
    install_bat = os.path.join(dist_dir, "一鍵安裝(以系統管理員身分執行).bat")
    with open(install_bat, "w", encoding="utf-8-sig") as f:
        f.write('@echo off\r\n')
        f.write('chcp 65001 >nul\r\n')
        f.write('cd /d "%~dp0"\r\n')
        f.write('echo ========================================================\r\n')
        f.write('echo   正在安裝 RuhI如喜通用智慧型輸入法（倉頡＋英文零切換）...\r\n')
        f.write('echo ========================================================\r\n')
        f.write('regsvr32.exe /s "%~dp0ruhi_tsf.dll"\r\n')
        f.write('if %ERRORLEVEL% EQU 0 (\r\n')
        f.write('    echo.\r\n')
        f.write('    echo   【系統元件註冊成功！】\r\n')
        f.write('    echo.\r\n')
        f.write('    echo   請按 [Win + 空白鍵] 切換至「RuhI如喜通用智慧型輸入法」。\r\n')
        f.write('    echo.\r\n')
        f.write('    echo   --------------------------------------------------------\r\n')
        f.write('    echo   【提示：若按 Win + 空白鍵 清單中尚未出現如喜輸入法？】\r\n')
        f.write('    echo   Windows 11 安全架構可能需要您手動將新輸入法打勾啟用。\r\n')
        f.write('    echo   即將為您自動開啟「Windows 語言與地區設定」頁面：\r\n')
        f.write('    echo     1. 找到「中文(繁體，台灣)」，點擊右側「...」選擇【語言選項】\r\n')
        f.write('    echo     2. 滾動至【已安裝的鍵盤】，點擊【新增鍵盤】\r\n')
        f.write('    echo     3. 勾選【RuhI如喜通用智慧型輸入法】即可！\r\n')
        f.write('    echo   --------------------------------------------------------\r\n')
        f.write('    echo.\r\n')
        f.write('    start ms-settings:regionlanguage\r\n')
        f.write(') else (\r\n')
        f.write('    echo.\r\n')
        f.write('    echo   【安裝失敗】請確認是否已「按右鍵 - 以系統管理員身分執行」。\r\n')
        f.write('    echo.\r\n')
        f.write(')\r\n')
        f.write('pause\r\n')
    print("[OK] Generated install bat")

    # 6. 產生 一鍵解除安裝.bat (帶 BOM UTF-8)
    uninstall_bat = os.path.join(dist_dir, "一鍵解除安裝(以系統管理員身分執行).bat")
    with open(uninstall_bat, "w", encoding="utf-8-sig") as f:
        f.write('@echo off\r\n')
        f.write('chcp 65001 >nul\r\n')
        f.write('cd /d "%~dp0"\r\n')
        f.write('echo ========================================================\r\n')
        f.write('echo   正在解除安裝 RuhI如喜通用智慧型輸入法...\r\n')
        f.write('echo ========================================================\r\n')
        f.write('regsvr32.exe /u /s "%~dp0ruhi_tsf.dll"\r\n')
        f.write('echo.\r\n')
        f.write('echo   【已成功解除安裝】已自系統中移除註冊。\r\n')
        f.write('echo.\r\n')
        f.write('pause\r\n')
    print("[OK] Generated uninstall bat")

    # 7. 產生 免安裝即時測試.bat (雙擊直接啟動獨立測試器)
    tester_bat = os.path.join(dist_dir, "免安裝即時測試(雙擊啟動).bat")
    with open(tester_bat, "w", encoding="utf-8-sig") as f:
        f.write('@echo off\r\n')
        f.write('chcp 65001 >nul\r\n')
        f.write('cd /d "%~dp0"\r\n')
        f.write('ruhi_cangjie_tester.exe\r\n')
        f.write('pause\r\n')
    print("[OK] Generated tester bat")

    # 8. 產生 說明文件 README_測試指南.txt (帶 BOM UTF-8)
    readme_path = os.path.join(dist_dir, "README_測試指南.txt")
    with open(readme_path, "w", encoding="utf-8-sig") as f:
        f.write("============================================================\r\n")
        f.write("  RuhI如喜通用智慧型輸入法 - 倉頡＋英文零切換智慧混打測試指南\r\n")
        f.write("============================================================\r\n\r\n")
        f.write("敬致 自然人社群 大飛 兄與測試夥伴：\r\n\r\n")
        f.write("感謝您寶貴的第一手反饋！本次更新重點修復：\r\n")
        f.write("1. 【全面補齊三代倉頡 (CJ3) 常用字碼】：\r\n")
        f.write("   - 「鍵」(cnkq)、「鼠」(hxvyv)、「睡」(buhjm)、「賊」(bcij) 等全面直出！\r\n")
        f.write("   - 達成「三代＋五代雙向完全相容」，微軟倉頡老手完全不必改變擊鍵習慣。\r\n")
        f.write("2. 【名稱統一】：輸入法在 Windows 語言列正式定名為「RuhI如喜通用智慧型輸入法」。\r\n")
        f.write("3. 【Windows 11 切換指引】：執行一鍵安裝時自動引導進入語言設定頁面。\r\n\r\n")
        f.write("------------------------------------------------------------\r\n")
        f.write("【兩種測試方式】\r\n\r\n")
        f.write("方式一：免安裝即時測試（推薦先跑這個快速驗證字碼與手感）\r\n")
        f.write("  1. 雙擊「免安裝即時測試(雙擊啟動).bat」或「ruhi_cangjie_tester.exe」。\r\n")
        f.write("  2. 在終端機中直接鍵入字母與特殊操作體驗！\r\n\r\n")
        f.write("方式二：Windows 系統級安裝（在整個系統所有軟體中使用）\r\n")
        f.write("  1. 對「一鍵安裝(以系統管理員身分執行).bat」按右鍵，選擇【以系統管理員身分執行】。\r\n")
        f.write("  2. 若按 [Win + 空白鍵] 清單中已出現「RuhI如喜通用智慧型輸入法」，直接選中即可！\r\n")
        f.write("  3. 若清單中尚未出現（Windows 11 安全特性），腳本會自動彈出「語言設定」頁面：\r\n")
        f.write("     ➔ 點擊 中文(台灣) 旁的「...」➔【語言選項】➔【新增鍵盤】➔ 勾選【RuhI如喜通用智慧型輸入法】。\r\n")
        f.write("  4. 即可在 Notepad、Word、Chrome、Line 等任何程式中暢快混打！\r\n")
        f.write("  5. 測試完畢若想移除，按右鍵執行「一鍵解除安裝」即可，乾淨無殘留。\r\n\r\n")
        f.write("------------------------------------------------------------\r\n")
        f.write("【核心手感與鍵位邏輯】\r\n\r\n")
        f.write("1. 倉頡漢字出字：\r\n")
        f.write("   - 輸入倉頡碼（如 cnkq）➔ 按【空白鍵 Space】➔ 立即上屏漢字「鍵」。\r\n")
        f.write("   - 預編輯區會同時顯示倉頡英文字母與字根提示（如 cnkq [金弓大手]）。\r\n\r\n")
        f.write("2. 零切換英文直通（核心亮點！）：\r\n")
        f.write("   - 輸入字母後（如 ab、git、cd、test）➔ 直接按【Enter 鍵】！\r\n")
        f.write("   - 系統立即原樣上屏原始英文，完全免按 Shift 切換中英文模式！\r\n\r\n")
        f.write("3. 智慧英文自動識別：\r\n")
        f.write("   - 超過 5 碼單字（如 commit、github、system）：因倉頡碼長上限為 5，\r\n")
        f.write("     系統自動判定為英文模式，按 Space 或 Enter 皆直接上屏英文單字！\r\n")
        f.write("   - 大寫字母開頭（如 Git、API、Windows）：自動判定為英文，直接上屏。\r\n")
        f.write("   - 底線變數名（如 user_id、my_data）：自動連貫上屏，寫程式零負擔。\r\n\r\n")
        f.write("4. 選字與退格：\r\n")
        f.write("   - 數字鍵 1~9：可直接挑選候選窗內的重碼字。\r\n")
        f.write("   - Backspace 退格：逐碼修改。\r\n")
        f.write("   - Esc 鍵：清空當前組字。\r\n\r\n")
        f.write("歡迎隨時提出手感反饋與改進建議，感謝您的熱心協助！\r\n")
    # 8.5 產生 開發者與AI協作指南 src_developer/README_AI協作指南.txt (帶 BOM UTF-8)
    dev_readme_path = os.path.join(src_dev_dir, "README_AI協作指南.txt")
    with open(dev_readme_path, "w", encoding="utf-8-sig") as f:
        f.write("============================================================\r\n")
        f.write("  如喜（RuhI）倉頡＋歐文智慧混打 - 開發者與 AI 協作手冊\r\n")
        f.write("============================================================\r\n\r\n")
        f.write("致 大飛 兄：\r\n\r\n")
        f.write("這份源碼包包含了如喜「倉頡＋英文零切換智慧混打引擎」最核心的 Rust 實現！\r\n")
        f.write("您完全不需要是 Rust 專家，現在只要將這些代碼交給任何您慣用的 AI（Claude、ChatGPT 或 Gemini），\r\n")
        f.write("AI 就能基於這個完整可運行的底座，為您量身打造出您心中夢想的「倉頡＋歐文（德法西義）混打輸入法」！\r\n\r\n")
        f.write("------------------------------------------------------------\r\n")
        f.write("【目錄結構說明】\r\n\r\n")
        f.write("1. cangjie/mod.rs\r\n")
        f.write("   - 核心狀態機：負責管理緩衝區 (Buffer)、24 字根提示、候選字清單。\r\n")
        f.write("   - 零切換判定：超過 5 碼自動轉英文、大寫直通、Enter 直通原始英文、Space 上屏漢字。\r\n\r\n")
        f.write("2. cangjie/table.rs\r\n")
        f.write("   - 碼表管理：以 HashMap 實現超高速字碼查詢，支援三代與五代容錯與權重排序。\r\n\r\n")
        f.write("3. cangjie_tester.rs\r\n")
        f.write("   - 獨立命令列測試器：讓您在終端機中敲指令即時觀察出字與狀態轉變。\r\n\r\n")
        f.write("4. Cargo.toml\r\n")
        f.write("   - Rust 專案配置檔。\r\n\r\n")
        f.write("------------------------------------------------------------\r\n")
        f.write("【您可以直接複製給 AI 的指令範本（Prompt Template）】\r\n\r\n")
        f.write("範本一（加入歐文特殊字母混打）：\r\n")
        f.write("「這是我目前使用的倉頡＋英文混打輸入法 Rust 核心代碼（見附檔 cangjie/mod.rs）。\r\n")
        f.write(" 我想請你幫我擴充支援『歐文特殊字母（如德文 ä, ö, ü, ß、法文 é, è, ê, ç 等）』。\r\n")
        f.write(" 請幫我設計一套既不干擾倉頡 24 字根，又能直覺打出歐文字母的擊鍵規則，\r\n")
        f.write(" 並修改 CangjieEngine 程式碼實現它！」\r\n\r\n")
        f.write("範本二（自訂專屬符號或快捷鍵）：\r\n")
        f.write("「請參考 cangjie/mod.rs，我想增加自訂快捷鍵與標點符號對應，\r\n")
        f.write(" 例如輸入特定的前綴碼時能展開常用的數學符號或程式碼片段，請幫我修改實現。」\r\n\r\n")
        f.write("------------------------------------------------------------\r\n")
        f.write("【本地編譯與測試方式（若您有安裝 Rust 環境）】\r\n\r\n")
        f.write("  cargo run --bin ruhi_cangjie_tester\r\n\r\n")
        f.write("祝您與 AI 協作愉快，一同探索輸入法的無限可能！🌿\r\n")
    print("[OK] Generated README_AI_guide.txt")

    # 9. 打包為 ZIP 壓縮檔
    zip_output = os.path.join(base_dir, "dist", "Ruhi_Cangjie_Win.zip")
    with zipfile.ZipFile(zip_output, "w", zipfile.ZIP_DEFLATED) as zf:
        for root, dirs, files in os.walk(dist_dir):
            for file in files:
                full_path = os.path.join(root, file)
                rel_path = os.path.relpath(full_path, os.path.dirname(dist_dir))
                zf.write(full_path, rel_path)
    
    zip_size = os.path.getsize(zip_output)
    print(f"\n[SUCCESS] Package created: {zip_output} ({zip_size} bytes)")

if __name__ == "__main__":
    package_cangjie()
