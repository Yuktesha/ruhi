# RuhI (如喜) 偏好設定中心啟動腳本 (優先啟動原生 Rust 版 ruhi-studio.exe)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$rustCandidates = @(
    (Join-Path $PSScriptRoot "ruhi-studio.exe"),
    (Join-Path $PSScriptRoot "ruhi-studio\target\release\ruhi-studio.exe"),
    (Join-Path $PSScriptRoot "dist\ruhi-studio.exe"),
    (Join-Path $PSScriptRoot "scripts\ruhi-studio.exe"),
    "C:\ATGprjs\ruhi\ruhi-studio.exe"
)

foreach ($rc in $rustCandidates) {
    if (Test-Path $rc) {
        $resolved = (Resolve-Path $rc).Path
        Start-Process -FilePath $resolved -WorkingDirectory (Split-Path -Parent $resolved)
        exit 0
    }
}

$pyCandidates = @(
    (Join-Path $PSScriptRoot "settings_window.py"),
    (Join-Path $PSScriptRoot "dist\settings_window.py"),
    (Join-Path $PSScriptRoot "scripts\settings_window.py")
)
$target = $null
foreach ($c in $pyCandidates) {
    if (Test-Path $c) {
        $target = (Resolve-Path $c).Path
        break
    }
}

if (-not $target) {
    Write-Host "找不到 ruhi-studio.exe 或 settings_window.py！" -ForegroundColor Red
    exit 1
}

$py = if (Test-Path "C:\Python\pythonw.exe") { "C:\Python\pythonw.exe" } else { "pythonw.exe" }
Start-Process -FilePath $py -ArgumentList ('"' + $target + '"') -WorkingDirectory (Split-Path -Parent $target)
