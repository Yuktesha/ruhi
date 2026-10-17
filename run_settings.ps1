# RuhI (如喜) 偏好設定中心啟動腳本
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$candidates = @(
    (Join-Path $PSScriptRoot "settings_window.py"),
    (Join-Path $PSScriptRoot "dist\settings_window.py"),
    (Join-Path $PSScriptRoot "scripts\settings_window.py")
)
$target = $null
foreach ($c in $candidates) {
    if (Test-Path $c) {
        $target = (Resolve-Path $c).Path
        break
    }
}

if (-not $target) {
    Write-Host "找不到 settings_window.py！" -ForegroundColor Red
    exit 1
}

$py = if (Test-Path "C:\Python\pythonw.exe") { "C:\Python\pythonw.exe" } else { "pythonw.exe" }

Start-Process -FilePath $py -ArgumentList ('"' + $target + '"') -WorkingDirectory (Split-Path -Parent $target)
