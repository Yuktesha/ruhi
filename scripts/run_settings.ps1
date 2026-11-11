# RuhI (如喜) 偏好設定中心啟動腳本 (純 Rust 原生 ruhi-settings.exe)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$rustCandidates = @(
    (Join-Path $PSScriptRoot "ruhi-settings.exe"),
    (Join-Path (Split-Path -Parent $PSScriptRoot) "ruhi-settings.exe"),
    (Join-Path (Split-Path -Parent $PSScriptRoot) "ruhi-settings\target\release\ruhi-settings.exe"),
    "C:\ATGprjs\ruhi\ruhi-settings.exe"
)

foreach ($rc in $rustCandidates) {
    if (Test-Path $rc) {
        $resolved = (Resolve-Path $rc).Path
        Start-Process -FilePath $resolved -WorkingDirectory (Split-Path -Parent $resolved)
        exit 0
    }
}

Write-Host "找不到 ruhi-settings.exe！請先編譯 ruhi-settings。" -ForegroundColor Red
exit 1
