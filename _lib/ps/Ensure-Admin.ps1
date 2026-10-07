<#
.SYNOPSIS
    ATGprjs 全域共用：自動檢測並提升為系統管理員權限 (UAC Auto-Elevation)
.DESCRIPTION
    任何需要 Administrator 權限之 .ps1 腳本，只需在第一行加入：
        . "C:\ATGprjs\_lib\ps\Ensure-Admin.ps1"
    若當前未具備管理員身分，本模組會自動透過 UAC (RunAs) 重新喚醒原腳本並退出當前非提權工作階段。
    
    編碼標準：強制 UTF-8 with BOM (相容 Windows PowerShell 5.1 & PowerShell 7+)
#>

# 檢驗當前工作階段是否已具備管理員權限
$currentIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal($currentIdentity)

if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    # 精確取得調用方主腳本路徑（透過 $MyInvocation 追蹤點語法引入之父腳本）
    $scriptToElevate = if ($MyInvocation.PSCommandPath) {
        $MyInvocation.PSCommandPath
    } elseif ($PSCommandPath) {
        $PSCommandPath
    } else {
        $MyInvocation.ScriptName
    }

    if (-not $scriptToElevate -or (-not (Test-Path $scriptToElevate))) {
        Write-Warning "【警告】無法自動解析主呼叫腳本路徑，請手動以系統管理員身分開啟 PowerShell 執行。"
        return
    }

    # 組合提權參數，維持相同的執行目錄與傳入引數
    $callerDir = Split-Path -Parent $scriptToElevate
    $argList = "-NoProfile -ExecutionPolicy Bypass -File `"$scriptToElevate`""
    
    if ($args -and $args.Count -gt 0) {
        $escapedArgs = $args | ForEach-Object {
            if ($_ -match '\s') { "`"$_`"" } else { $_ }
        }
        $argList += " " + ($escapedArgs -join " ")
    }

    # 喚起 UAC 授權
    Start-Process powershell.exe -WorkingDirectory $callerDir -ArgumentList $argList -Verb RunAs
    exit
}
