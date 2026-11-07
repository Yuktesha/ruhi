param(
    [switch]$Web
)
$exe = Join-Path $PSScriptRoot 'ruhi-studio.exe'
if ($Web) {
    Start-Process -FilePath $exe -ArgumentList '--web'
} else {
    Start-Process -FilePath $exe -ArgumentList '--tuner-app'
}
