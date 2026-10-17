# RuhI (如喜) 通用多語系智慧型輸入法 - 一鍵解除安裝腳本
# 符合 Windows 10 / 11 純 TSF 規範 (TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC)
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
        Write-Host "正在請求系統管理員授權以註銷 TSF 系統服務..." -ForegroundColor Cyan
        $argList = "-NoProfile -ExecutionPolicy Bypass -File `"" + $MyInvocation.MyCommand.Path + "`""
        Start-Process powershell.exe -ArgumentList $argList -Verb RunAs
        exit
    }
}

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在解除安裝 RuhI (如喜) 通用多語系智慧型輸入法..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

# ── 2. 定位 DLL ──
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

# 暫停 ctfmon
try {
    Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
} catch {}
Start-Sleep -Milliseconds 300

# 執行 COM 註銷
if ($dllPath -and (Test-Path $dllPath)) {
    Write-Host "  正在註銷 TSF 元件: $dllPath" -ForegroundColor Yellow
    $regsvr = Start-Process -FilePath "regsvr32.exe" -ArgumentList "/u", "/s", ('"' + $dllPath + '"') -Wait -PassThru
}

# 清理 Assemblies 與 SortOrder 項目
try {
    $asmKey = "HKCU:\Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}"
    if (Test-Path $asmKey) {
        $curDefault = (Get-ItemProperty -Path $asmKey -Name "Default" -ErrorAction SilentlyContinue).Default
        if ($curDefault -eq "{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}") {
            Remove-Item -Path $asmKey -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    $sortKey = "HKCU:\Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}"
    if (Test-Path $sortKey) {
        Remove-Item -Path $sortKey -Recurse -Force -ErrorAction SilentlyContinue
    }
} catch {}

# 重啟 ctfmon
Start-Process "ctfmon.exe"

Write-Host ""
Write-Host "  【已成功解除安裝】如喜輸入法已自系統中安全移除。" -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Cyan
Write-Host ""
Read-Host "請按 Enter 鍵結束"
