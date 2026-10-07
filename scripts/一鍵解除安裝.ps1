# ── 一行引入 ATGprjs 全域共用提權模組 ──
$libAdmin = Join-Path $PSScriptRoot "_lib\ps\Ensure-Admin.ps1"
if (-not (Test-Path $libAdmin)) { $libAdmin = "C:\ATGprjs\_lib\ps\Ensure-Admin.ps1" }
if (-not (Test-Path $libAdmin)) { $libAdmin = Join-Path $PSScriptRoot "..\..\..\_lib\ps\Ensure-Admin.ps1" }
. $libAdmin

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在解除安裝 RuhI如喜通用智慧型輸入法..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

$dllPath = Join-Path $PSScriptRoot "ruhi_tsf.dll"
if (Test-Path $dllPath) {
    $regsvr = Start-Process -FilePath "regsvr32.exe" -ArgumentList "/u", "/s", "`"$dllPath`"" -Wait -PassThru
    Write-Host ""
    Write-Host "  【已成功解除安裝】已自系統中移除註冊。" -ForegroundColor Green
    Write-Host ""
} else {
    Write-Host ""
    Write-Host "  找不到 $dllPath，略過 COM 註銷。" -ForegroundColor Yellow
    Write-Host ""
}

Read-Host "請按 Enter 鍵結束"
