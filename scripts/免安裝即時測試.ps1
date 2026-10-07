# ── 免安裝即時測試 (主控台模式) ──
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$testerExe = Join-Path $PSScriptRoot "ruhi_cangjie_tester.exe"
if (Test-Path $testerExe) {
    & $testerExe
} else {
    Write-Host "找不到測試程式: $testerExe" -ForegroundColor Red
}

Read-Host "`n請按 Enter 鍵結束"
