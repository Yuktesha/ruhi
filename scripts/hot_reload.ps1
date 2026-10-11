# RuhI 熱更新轉發入口 (Forwarder to root hot_reload.ps1)
$ErrorActionPreference = 'Stop'
$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetScript = Join-Path $rootDir "hot_reload.ps1"
if (Test-Path $targetScript) {
    & $targetScript
} else {
    Write-Host "找不到 $targetScript" -ForegroundColor Red
}
