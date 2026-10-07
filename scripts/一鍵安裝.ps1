# ── 一行引入 ATGprjs 全域共用提權模組 ──
$libAdmin = Join-Path $PSScriptRoot "_lib\ps\Ensure-Admin.ps1"
if (-not (Test-Path $libAdmin)) { $libAdmin = "C:\ATGprjs\_lib\ps\Ensure-Admin.ps1" }
if (-not (Test-Path $libAdmin)) { $libAdmin = Join-Path $PSScriptRoot "..\..\..\_lib\ps\Ensure-Admin.ps1" }
. $libAdmin

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在安裝 RuhI如喜通用智慧型輸入法（倉頡＋英文零切換）..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

$dllPath = Join-Path $PSScriptRoot "ruhi_tsf.dll"
if (-not (Test-Path $dllPath)) {
    Write-Host "`n  【安裝失敗】找不到核心檔案: $dllPath" -ForegroundColor Red
    Read-Host "請按 Enter 鍵結束"
    exit 1
}

$regsvr = Start-Process -FilePath "regsvr32.exe" -ArgumentList "/s", "`"$dllPath`"" -Wait -PassThru
if ($regsvr.ExitCode -eq 0) {
    Write-Host ""
    Write-Host "  【系統元件註冊成功！】" -ForegroundColor Green
    Write-Host ""
    Write-Host "  請按 [Win + 空白鍵] 切換至「RuhI如喜通用智慧型輸入法」。" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  --------------------------------------------------------" -ForegroundColor DarkGray
    Write-Host "  【提示：若按 Win + 空白鍵 清單中尚未出現如喜輸入法？】" -ForegroundColor Cyan
    Write-Host "  Windows 11 安全架構可能需要您手動將新輸入法打勾啟用。"
    Write-Host "  即將為您自動開啟「Windows 語言與地區設定」頁面："
    Write-Host "    1. 找到「中文(繁體，台灣)」，點擊右側「...」選擇【語言選項】"
    Write-Host "    2. 滾動至【已安裝的鍵盤】，點擊【新增鍵盤】"
    Write-Host "    3. 勾選【RuhI如喜通用智慧型輸入法】即可！"
    Write-Host "  --------------------------------------------------------" -ForegroundColor DarkGray
    Write-Host ""
    Start-Process "ms-settings:regionlanguage"
} else {
    Write-Host ""
    Write-Host "  【安裝失敗】元件註冊回傳代碼: $($regsvr.ExitCode)" -ForegroundColor Red
    Write-Host ""
}

Read-Host "請按 Enter 鍵結束"
