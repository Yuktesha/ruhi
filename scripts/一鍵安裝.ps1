# RuhI (如喜) 通用多語系智慧型輸入法 - 一鍵安裝與註冊腳本
# 符合 Windows 10 / 11 純 TSF 與 Win32 傳統相容性規範 (TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC)
# 強制 UTF-8 with BOM 編碼

# ── 1. 系統管理員權限自我提升檢驗 ──
$libAdmin = Join-Path $PSScriptRoot "_lib\ps\Ensure-Admin.ps1"
if (-not (Test-Path $libAdmin)) { $libAdmin = "C:\ATGprjs\_lib\ps\Ensure-Admin.ps1" }
if (Test-Path $libAdmin) {
    . $libAdmin
} else {
    $currentIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal($currentIdentity)
    if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Host "正在請求系統管理員授權以註冊 TSF 系統服務..." -ForegroundColor Cyan
        $argList = "-NoProfile -ExecutionPolicy Bypass -File `"" + $MyInvocation.MyCommand.Path + "`""
        Start-Process powershell.exe -ArgumentList $argList -Verb RunAs
        exit
    }
}

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在安裝 RuhI (如喜) 通用多語系智慧型輸入法..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

# ── 2. 定位核心檔案 ──
$dllCandidates = @(
    (Join-Path $PSScriptRoot "ruhi_tsf.dll"),
    (Join-Path $PSScriptRoot "dist\ruhi_tsf.dll"),
    (Join-Path $PSScriptRoot "..\dist\ruhi_tsf.dll")
)
$dllPath = $null
foreach ($c in $dllCandidates) {
    if (Test-Path $c) {
        $dllPath = (Resolve-Path $c).Path
        break
    }
}

if (-not $dllPath) {
    Write-Host "`n  【安裝失敗】找不到核心檔案 ruhi_tsf.dll！" -ForegroundColor Red
    Read-Host "請按 Enter 鍵結束"
    exit 1
}

$installDir = Split-Path -Parent $dllPath
Write-Host "  安裝與執行目錄: $installDir" -ForegroundColor DarkGray
Write-Host "  核心模組路徑:   $dllPath" -ForegroundColor DarkGray

# ── 3. 步驟 1：宣告全體使用者與 AppContainer 讀取權限 ──
Write-Host "  [1/5] 配置安全性存取清單 (ACL: ALL_APPLICATION_PACKAGES 及 Users)..." -ForegroundColor Green
try {
    & icacls "$installDir" /grant "*S-1-15-2-1:(OI)(CI)RX" /T /Q 2>$null | Out-Null
    & icacls "$installDir" /grant "Users:(OI)(CI)RX" /T /Q 2>$null | Out-Null
} catch {}

# ── 4. 步驟 2：暫停文字服務框架（防檔案鎖定） ──
Write-Host "  [2/5] 暫停文字服務框架監聽 (ctfmon.exe)..." -ForegroundColor Green
try {
    Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
} catch {}
Start-Sleep -Milliseconds 300

# ── 5. 步驟 3：執行提權 COM 伺服器註冊 ──
Write-Host "  [3/5] 註冊 TSF COM 伺服器..." -ForegroundColor Green
$regsvr = Start-Process -FilePath "regsvr32.exe" -ArgumentList "/s", ('"' + $dllPath + '"') -Wait -PassThru
if ($regsvr.ExitCode -ne 0) {
    Write-Host "`n  【安裝警告】regsvr32 回傳代碼: $($regsvr.ExitCode)" -ForegroundColor Yellow
}

# ── 6. 步驟 4：執行清理與 Win32 Assemblies 關聯 ──
Write-Host "  [4/5] 配置繁中 Assemblies 預設值並清理無效替代佈局..." -ForegroundColor Green
try {
    # 清理無效 SubstituteLayout
    $lpKey = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}\LanguageProfile\0x00000404\{F52A781B-8321-48E6-A145-C89B0981E7D2}"
    if (Test-Path $lpKey) {
        Remove-ItemProperty -Path $lpKey -Name "SubstituteLayout" -ErrorAction SilentlyContinue
    }
} catch {}

try {
    # 配置 HKCU Assemblies 與 SortOrder (0x04040404 標準代碼)
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
} catch {
    Write-Host "  [提示] 設定 Assemblies 時發生例外：$($_.Exception.Message)" -ForegroundColor Yellow
}

# ── 7. 步驟 5：重啟文字服務框架 ──
Write-Host "  [5/5] 喚起文字服務框架 (ctfmon.exe)..." -ForegroundColor Green
Start-Process "ctfmon.exe"

Write-Host ""
Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  【安裝完成！】RuhI (如喜) 已成功安裝並啟用！" -ForegroundColor Green
Write-Host "  ★ 直接按 [Win + 空白鍵] 即可切換至如喜輸入法開始體驗！" -ForegroundColor Yellow
Write-Host "========================================================" -ForegroundColor Cyan
Write-Host ""

# 選擇性安裝本地 AI
$setupAi = Join-Path $PSScriptRoot "安裝本地AI純綠推論引擎.ps1"
if (-not (Test-Path $setupAi)) { $setupAi = Join-Path $installDir "安裝本地AI純綠推論引擎.ps1" }
if (Test-Path $setupAi) {
    Write-Host "【選擇性功能：本地純綠 AI 語意備援引擎】" -ForegroundColor Cyan
    Write-Host "如喜日常打字 100% 由本地 Rust 高速運算，無需 AI 即完整可用。" -ForegroundColor Gray
    Write-Host "若您欲啟用離線 Google Gemma 2 深度句法診斷與長詞萃取：" -ForegroundColor Gray
    $userOpt = Read-Host "是否立即自動安裝純綠 Llama 引擎與 Google Gemma 2？(y/N) [預設 N]"
    if ($userOpt -match '^(y|yes)$') {
        & $setupAi
    }
}

Read-Host "請按 Enter 鍵結束"
