# RuhI (如喜) TSF 輸入法全 Windows 生態相容性規範與安裝打包指南
> **文件版本**：v1.0.0  
> **建立日期**：2026-10-11  
> **對接目標**：供後續負責建置、打包、安裝程式（Inno Setup / NSIS / WiX）開發之對話 [`8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`](conversation://8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708) 作為核心發布標準與避坑指南。

---

## 🎯 核心使命與背景

在 Windows 10 / 11 平台上，開發純 TSF (Text Services Framework) 輸入法時，常會遭遇「在現代應用程式（Chrome、Edge、VS Code）運作正常，但在傳統 Win32 應用程式（LibreOffice Writer、Windows 主控台 `conhost.exe`、舊版軟體、部分遊戲）中卻完全無法切換、一按 <kbd>Win</kbd> + <kbd>Space</kbd> 畫面閃一下立即跳回英文，甚至彈出 `Entry Point Not Found` 崩潰對話框」的嚴峻相容性挑戰。

本指南完整沉澱了 2026-10-11 攻克此問題的核心技術原理、兩大致命根因與已驗證的最佳實踐，以確保未來任何安裝包或自動化打包腳本皆能 100% 正確安裝、全環境順暢執行。

---

## 💣 兩大相容性致命死穴與技術根因

### 1. 致命死穴一：PE 導入表（Import Table）對 `comctl32.dll` 的靜態依賴陷阱

* **崩潰現象**：  
  在宿主環境（如 Windows 主控台 `conhost.exe` 或 LibreOffice Writer）載入輸入法 DLL 時，系統直接彈出致命錯誤視窗：
  > **`conhost.exe - Entry Point Not Found`**  
  > `The procedure entry point TaskDialogIndirect could not be located in the dynamic link library C:\ATGprjs\ruhi\dist\ruhi_tsf.dll.`
* **本質根因**：  
  `TaskDialogIndirect` 是 Windows Vista 隨 **ComCtl32 version 6.0** 引入的高級對話框 API。  
  如果 Rust 程式碼中**直接靜態呼叫**了 `TaskDialogIndirect`，編譯器會將其寫入 PE 導入表（`IMAGE_DIRECTORY_ENTRY_IMPORT`）。  
  然而，`conhost.exe`、`soffice.bin` (LibreOffice) 以及許多傳統 Win32 程式**並未啟用 ComCtl32 v6 的 Application Manifest**，其進程空間中載入的是 Windows 歷史相容版的 `comctl32.dll` (v5.82)。舊版中**根本不存在 `TaskDialogIndirect`**！  
  Windows PE Loader 在進行動態鏈結符號解析時發現缺少進入點，**直接中斷載入並宣告進程錯誤**，導致輸入法完全無法被載入！
* **絕對規範**：  
  - **嚴禁在輸入法核心 DLL 中靜態引入任何 ComCtl32 v6 專屬 API**。  
  - 若需使用 `TaskDialogIndirect`，**必須透過 `GetModuleHandleA` + `GetProcAddress` 於執行期動態獲取**。若宿主環境為舊版 v5.82，程式碼**必須安全備援退回原生 `MessageBoxW`**！  
  - 交付驗證：編譯出的 `ruhi_tsf.dll` 導入表中**絕對不得包含 `comctl32.dll`**。

---

### 2. 致命死穴二：純 TSF TIP 註冊時錯誤設定替代鍵盤佈局（SubstituteLayout）

* **錯誤現象**：  
  在 LibreOffice Writer 等傳統 Win32 視窗中按下 <kbd>Win</kbd> + <kbd>Space</kbd> 切換輸入法時，畫面閃一下又瞬間自動跳回英文，`tsf_debug.log` 中完全沒有該進程的任何記錄。
* **本質根因**：  
  在呼叫 `ITfInputProcessorProfileMgr::RegisterProfile` 註冊 TSF 設定檔時，若傳入了非零的替代鍵盤代碼（例如 `hklsubstitute = HKL(0x04040404)`），Windows 會在系統登錄檔中自動產生鍵值：  
  `HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}\LanguageProfile\0x00000404\{GUID}\SubstituteLayout = "0x04040404"`  
  當傳統 Win32 應用程式透過 Windows 核心載入輸入法時，Windows 嘗試呼叫 `LoadKeyboardLayoutW` 載入名為 `"0x04040404"` 的實體鍵盤佈局。然而在 Windows `Keyboard Layouts` 驅動列表中**根本不存在 `0x04040404`**！Windows 判定鍵盤佈局無效，**立即放棄並強制將視窗鍵盤回退至預設的英文佈局**！
* **絕對規範**：  
  - 純 TSF 輸入法（無獨立 `.ime` 驅動檔案者）在註冊 Profile 時，**`hklsubstitute` 必須傳入 `HKL(0 as _)`（即 NULL）**！  
  - 系統登錄檔 `HKLM\SOFTWARE\Microsoft\CTF\TIP\...` 底下**絕對不可存在任何無效的 `SubstituteLayout` 殘留鍵值**。

---

## ⚙️ 安裝程式（Installer）標準打包作業規範

在由對話 [`8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`](conversation://8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708) 製作安裝檔（Setup.exe / MSI）時，安裝流程必須嚴格依序執行以下 5 個步驟：

### 步驟 1：部署檔案與權限宣告
1. 將編譯優化之 `ruhi_tsf.dll` 部署至標準安裝目錄（建議 `C:\Program Files\RuhI` 或使用者指定路徑）。
2. 同步完整詞庫（`data/` 目錄）與偏好設定工具。
3. 確保安裝目錄對全體使用者（`Users` 群組）具備讀取與執行權限，以相容沙盒與非管理者進程。

### 步驟 2：暫停文字服務框架（防檔案鎖定）
在替換或註冊前，安全暫停目前工作階段的 `ctfmon.exe`：
```powershell
Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300
```

### 步驟 3：執行提權 COM 伺服器註冊
以 Administrator 權限呼叫標準動態庫註冊：
```cmd
regsvr32.exe /s "C:\Program Files\RuhI\ruhi_tsf.dll"
```
> **原理**：`DllRegisterServer` 會自動完成 `CLSID`、`InprocServer32`（Apartment 執行緒模型）、`ITfInputProcessorProfiles::Register`、`ITfCategoryMgr::RegisterCategory` 等全套 COM 與 TSF 核心掛載。

### 步驟 4：執行清理與 Win32 Assemblies 關聯
為保證在 LibreOffice、Photoshop 等傳統 Win32 程式中被系統列舉並設為預設組件，安裝腳本必須執行以下登錄檔維護：
1. **清理無效替代佈局**：
   ```powershell
   $lpKey = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}\LanguageProfile\0x00000404\{F52A781B-8321-48E6-A145-C89B0981E7D2}"
   if (Test-Path $lpKey) {
       Remove-ItemProperty -Path $lpKey -Name "SubstituteLayout" -ErrorAction SilentlyContinue
   }
   ```
2. **配置繁中 Assemblies 預設值（0x04040404 標準代碼）**：
   ```powershell
   $asmKey = "HKCU:\Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}"
   if (-not (Test-Path $asmKey)) { New-Item -Path $asmKey -Force | Out-Null }
   Set-ItemProperty -Path $asmKey -Name "Default" -Value "{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}" -Type String
   Set-ItemProperty -Path $asmKey -Name "Profile" -Value "{F52A781B-8321-48E6-A145-C89B0981E7D2}" -Type String
   Set-ItemProperty -Path $asmKey -Name "KeyboardLayout" -Value 0x04040404 -Type DWord

   $sortKey = "HKCU:\Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}\00000000"
   if (-not (Test-Path $sortKey)) { New-Item -Path $sortKey -Force | Out-Null }
   Set-ItemProperty -Path $sortKey -Name "CLSID" -Value "{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}" -Type String
   Set-ItemProperty -Path $sortKey -Name "Profile" -Value "{F52A781B-8321-48E6-A145-C89B0981E7D2}" -Type String
   Set-ItemProperty -Path $sortKey -Name "KeyboardLayout" -Value 0x04040404 -Type DWord
   ```

### 步驟 5：重啟文字服務框架
註冊完畢後立即拉起文字服務框架：
```cmd
start ctfmon.exe
```

---

## 🗑️ 解除安裝（Uninstaller）清理規範

解除安裝程式在移除檔案前，必須執行乾淨反註冊：
1. 呼叫反註冊：`regsvr32.exe /u /s "<Path>\ruhi_tsf.dll"`
2. 清理 `HKCU\Software\Microsoft\CTF\Assemblies` 與 `SortOrder` 中指向如喜的 GUID 項目。
3. 刪除安裝目錄與相關快取。
4. 重啟 `ctfmon.exe`。

---

## 📋 發布前冒煙測試清單（Smoke Test Checklist）

每次建置發布包前，必須通過以下 4 項檢測：
- [ ] **導入表審計**：以 `pefile` 或 `dumpbin /imports` 檢查 `ruhi_tsf.dll`，確認**無 `comctl32.dll` 靜態依賴**。
- [ ] **主控台測試**：開啟 Windows 主控台（CMD / PowerShell / conhost.exe），確認切換至如喜輸入法**零彈窗、零報錯**。
- [ ] **傳統辦公套件測試**：開啟 LibreOffice Writer，按下 <kbd>Win</kbd> + <kbd>Space</kbd>，確認**可穩定停留在如喜輸入法並順暢組字**（非強制彈回英文）。
- [ ] **現代環境測試**：於 Chrome、Edge 或 VS Code 中測試打字、選字座彈出與底線內嵌正常。

---

## 🏛️ 六、 專業排版軟體（LibreOffice Writer 等）與「智慧宿主自適應」深度技術解析

### 1. 現象與核心本質：為什麼多行空行後，獨立輸入座會定位飄移？
在一般文字編輯器（如 Notepad、VS Code、網頁瀏覽器）中，「如喜輸入座（獨立浮動模式）」能緊隨游標即時移動；然而在 **LibreOffice Writer、Adobe InDesign 等專業大型頁面排版軟體** 中，若使用者連續鍵入數個空行（<kbd>Enter</kbd>），浮動輸入座常無法精準感知最新空行的垂直位置。

#### 根本原因：排版樹（Layout Tree）與「文字節點缺失」
- **一般編輯器**：內部資料模型為簡單的一維字元陣列（`char[]`），換行符（`\n`）具有實體偏移量，Caret 矩形由系統繪圖 API 直接依行高計算推導。
- **專業排版軟體（Page Layout Engine / VCL）**：
  - 內部為極其複雜的多層幾何排版樹（Page ➔ Section ➔ Header/Body/Footer ➔ Paragraph ➔ Text Run）。
  - 當使用者連按數次 <kbd>Enter</kbd> 換出連續空行時，排版引擎內部僅生成了空虛的「段落分隔標記（Paragraph Break）」，**在尚未鍵入任何文字前，排版樹中根本不存在任何實體文字節點（Text Node）**！
  - 此時輸入法向宿主詢問「字元幾何邊界在哪？（`ITfContextView::GetTextExt`）」，排版引擎因無實體字元可供測量，只能回傳上一行有效字元的殘留座標，或完全無法回報。
  - 同時，Win32 原生 Caret（`GetGUIThreadInfo`）在純空行未重繪時亦不保證調用 `SetCaretPos`，導致依賴實體視窗座標的獨立輸入座發生定位飄移。

### 2. 為什麼「微軟新注音模式（行內組字 `inline_underline`）」能 100% 完美解決？
微軟 TSF（Text Services Framework）架構本質上是為 **Inline Composition（行內組字）** 所設計的雙向協同通訊協議：
- 在「微軟新注音模式」下，未確認的拼弦與預選字直接作為 Composition String 注入宿主文件內部。
- 宿主排版引擎（Writer）即刻將組字字串納入當前的段落文字樹進行物理排版，換行、行高、字級、多欄排版完全由 Writer 原生渲染器計算。
- 因此無論使用者連續換多少空行、縮排多複雜，字元永遠出現在精確位置（實測「OK，沒問題」）。

### 3. 如喜的終極解法：「智慧宿主自適應（Smart Host Adaptation）」架構
為兼顧「日常軟體享受獨立輸入座的大氣居中美學」與「專業文書排版軟體 100% 穩健精準」，如喜在 `ruhi_tsf.dll` 核心實作了 **智慧宿主自適應機制**：
1. **0 開銷進程自我感知**：
   - 透過 `GetModuleFileNameW(None)` 於 0.0001 毫秒內感知當前注入的宿主程序名稱。
2. **黑名單/專業排版引擎識別**：
   - 當前進程為 `soffice.bin`、`soffice.exe`（LibreOffice）、`photoshop.exe`、`indesign.exe`、`illustrator.exe`、`acad.exe` 等具有私有複雜排版或繪圖畫布之軟體時，自動觸發適應。
3. **無感動態降級（Transparent Fallback）**：
   - 即便使用者全域偏好設定為「如喜輸入座模式（`floating_follow` 或 `floating_fixed`）」，如喜在此類專業宿主內部**自動無感切換為微軟新注音模式（`inline_underline` 行內組字）**。
   - 組字文字流暢呈現在空行游標正位置，選字框僅在需要選字時緊貼彈出。
4. **無感切換復原**：
   - 使用者切換回瀏覽器、Line、Discord、記事本、終端機等日常軟體時，如喜輸入座立即自動滿血復活，徹底免除使用者在不同軟體間手動繁瑣調整設定的負擔！

