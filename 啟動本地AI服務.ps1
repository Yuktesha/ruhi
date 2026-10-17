# RuhI 本地純綠 AI 推論服務啟動腳本
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$serverCandidates = @()
$cmdCheck = Get-Command 'llama-server.exe' -ErrorAction SilentlyContinue
if ($cmdCheck) {
    $serverCandidates += $cmdCheck.Source
}
$serverCandidates += "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\ggml.llamacpp_Microsoft.Winget.Source_8wekyb3d8bbwe\llama-server.exe"

$wingetMatches = Get-ChildItem -Path "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Filter "*llamacpp*" -Directory -ErrorAction SilentlyContinue
foreach ($dir in $wingetMatches) {
    $cand = Join-Path $dir.FullName 'llama-server.exe'
    if (Test-Path $cand) {
        $serverCandidates += $cand
    }
}

$serverExe = $null
foreach ($c in $serverCandidates) {
    if (Test-Path $c) {
        $serverExe = $c
        break
    }
}

$modelPath = Join-Path $env:LOCALAPPDATA 'RuhI\models\gemma\gemma-2-2b-it-Q4_K_M.gguf'
if (-not (Test-Path $modelPath) -and (Test-Path 'C:\ATGprjs\_models\gemma\gemma-2-2b-it-Q4_K_M.gguf')) {
    $modelPath = 'C:\ATGprjs\_models\gemma\gemma-2-2b-it-Q4_K_M.gguf'
}

if (-not $serverExe) {
    Write-Host '找不到 Llama 引擎 (llama-server.exe)' -ForegroundColor Red
    exit 1
}
if (-not (Test-Path $modelPath)) {
    Write-Host ('找不到模型檔案: ' + $modelPath) -ForegroundColor Red
    exit 1
}

$existing = Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host '本地純綠 AI 服務已在運行中 (http://127.0.0.1:8080)！' -ForegroundColor Green
    exit 0
}

Write-Host '正在喚醒純綠 Llama 引擎與 Google Gemma 2 (連接埠: 8080)...' -ForegroundColor Cyan

$argList = @('-m', $modelPath, '--port', '8080', '--host', '127.0.0.1', '-c', '4096', '-t', '12', '-ngl', '99')
Start-Process -FilePath $serverExe -ArgumentList $argList -WindowStyle Minimized
Start-Sleep -Seconds 2

Write-Host '本地純綠 AI 服務已就緒於 http://127.0.0.1:8080/v1！' -ForegroundColor Green
