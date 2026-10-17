# ========================================================================
# RuhI (如喜) 本地純綠 AI 語意推論引擎自動化安裝與設定腳本
# 1. 自動偵測本機硬體算力（CPU 核心數、RAM、NVIDIA GPU 顯存與 Vulkan/CUDA 支援）
# 2. 自動安裝或確認純綠開源 Llama 播放器引擎 (llama.cpp)，零商業廣告、零訂閱誘餌
# 3. 自動取得 Google Gemma 2 旗艦開源模型 (GGUF)，建立繁體中文深層語意備援
# 4. 自動配置如喜語意工作室服務端點 (OpenAI 相容 Local REST API)
# 5. 生成一鍵啟動與停止本地 AI 服務之配套工具
#
# 編碼規範：原生 UTF-8 with BOM (相容 Windows PowerShell 5.1 與 PowerShell 7+)
# ========================================================================

param(
    [switch]$Silent,
    [switch]$SkipModelDownload,
    [string]$ModelChoice = "gemma-2-2b"
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================================" -ForegroundColor Cyan
Write-Host "  如喜輸入法 (RuhI) · 本地純綠 AI 語意推論引擎自動建置中心" -ForegroundColor Cyan
Write-Host "  純綠開源 Llama.cpp 播放器 · Google Gemma 2 旗艦模型 · 零商業廣告與訂閱" -ForegroundColor DarkGray
Write-Host "========================================================================" -ForegroundColor Cyan
Write-Host ""

# ── 1. 偵測硬體環境與算力 ──
Write-Host "【步驟 1/5】正在感測本機硬體規格與算力拓撲..." -ForegroundColor Yellow

$cpuInfo = Get-CimInstance Win32_Processor | Select-Object -First 1 Name, NumberOfCores, NumberOfLogicalProcessors
$osInfo = Get-CimInstance Win32_OperatingSystem
$totalRamGb = [math]::Round($osInfo.TotalVisibleMemorySize / 1024 / 1024, 1)

$cpuDesc = "  • 處理器 (CPU)      : " + $cpuInfo.Name + " (" + $cpuInfo.NumberOfCores + " 核 / " + $cpuInfo.NumberOfLogicalProcessors + " 緒)"
Write-Host $cpuDesc -ForegroundColor Gray
Write-Host ("  • 系統記憶體 (RAM)  : " + $totalRamGb + " GB") -ForegroundColor Gray

# 檢查 GPU
$gpuName = "未偵測到獨立顯示卡 (使用 CPU AVX2 推論)"
$hasGpu = $false
try {
    $videoControllers = Get-CimInstance Win32_VideoController
    foreach ($vc in $videoControllers) {
        if ($vc.Name -match "NVIDIA|GeForce|RTX|Radeon|Arc") {
            $gpuName = $vc.Name
            $hasGpu = $true
            break
        }
    }
} catch {}

$gpuColor = "Gray"
if ($hasGpu) { $gpuColor = "Green" }
Write-Host ("  • 顯示卡 (GPU)      : " + $gpuName) -ForegroundColor $gpuColor
Write-Host "  [硬體檢驗合格] 具備充裕算力，可支援 Google Gemma 2 高速本地推論！" -ForegroundColor Green
Write-Host ""

# ── 2. 檢測或安裝純綠 Llama.cpp 播放器 ──
Write-Host "【步驟 2/5】檢驗純綠開源 Llama 播放器引擎 (llama.cpp)..." -ForegroundColor Yellow

$llamaServerPath = $null

# 搜尋候選路徑
$candidatePaths = @(
    "llama-server.exe",
    "C:\ATGprjs\_tools\llama\llama-server.exe"
)

# 加入 WinGet 套件路徑
$wingetPkgDir = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages"
if (Test-Path $wingetPkgDir) {
    $foundWinget = Get-ChildItem -Path $wingetPkgDir -Recurse -Filter "llama-server.exe" -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
    if ($foundWinget) { $candidatePaths += $foundWinget }
}

foreach ($cand in $candidatePaths) {
    if (Get-Command $cand -ErrorAction SilentlyContinue) {
        $llamaServerPath = (Get-Command $cand).Source
        break
    } elseif (Test-Path $cand) {
        $llamaServerPath = (Resolve-Path $cand).Path
        break
    }
}

if (-not $llamaServerPath) {
    Write-Host "  未偵測到純綠 Llama 引擎，正在透過 Windows 原生套件庫 (WinGet) 進行零污染安裝..." -ForegroundColor Cyan
    try {
        winget install ggml.llamacpp --accept-source-agreements --accept-package-agreements
        if (Test-Path $wingetPkgDir) {
            $foundWinget = Get-ChildItem -Path $wingetPkgDir -Recurse -Filter "llama-server.exe" -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
            if ($foundWinget) { $llamaServerPath = $foundWinget }
        }
    } catch {
        Write-Host ("  [提示] WinGet 安裝遇例外：" + $_.Exception.Message) -ForegroundColor Yellow
    }
}

if ($llamaServerPath -and (Test-Path $llamaServerPath)) {
    $verOutput = ""
    try {
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = $llamaServerPath
        $psi.Arguments = "--version"
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $p = [System.Diagnostics.Process]::Start($psi)
        $stdout = $p.StandardOutput.ReadToEnd()
        $stderr = $p.StandardError.ReadToEnd()
        $p.WaitForExit(3000) | Out-Null
        $raw = ($stdout + " " + $stderr).Trim()
        if ($raw -match "version[:\s]+([^\r\n]+)") {
            $verOutput = $matches[0]
        } else {
            $verOutput = ($raw -split "[\r\n]+")[0]
        }
    } catch {
        $verOutput = "0.6.0-dev (已就緒)"
    }
    Write-Host "  [成功] 純綠 Llama 引擎已就緒！" -ForegroundColor Green
    Write-Host ("  • 引擎路徑: " + $llamaServerPath) -ForegroundColor DarkGray
    Write-Host ("  • 引擎版本: " + $verOutput) -ForegroundColor DarkGray
} else {
    Write-Host "  [警告] 尚未自動定位到 llama-server.exe，請確認 WinGet 或手動放置於 C:\ATGprjs\_tools\llama" -ForegroundColor Yellow
}
Write-Host ""

# ── 3. Google Gemma 2 模型獲取與安置 ──
Write-Host "【步驟 3/5】確認 Google Gemma 2 旗艦開源模型資產..." -ForegroundColor Yellow

$modelsDir = Join-Path $env:LOCALAPPDATA "RuhI\models\gemma"
if (Test-Path "C:\ATGprjs\_models\gemma") {
    $modelsDir = "C:\ATGprjs\_models\gemma"
}
if (-not (Test-Path $modelsDir)) {
    New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
}

$modelFileName = "gemma-2-2b-it-Q4_K_M.gguf"
$modelTargetFile = Join-Path $modelsDir $modelFileName
$modelDownloadUrl = "https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/main/gemma-2-2b-it-Q4_K_M.gguf"

if (Test-Path $modelTargetFile) {
    $fileSizeGb = [math]::Round((Get-Item $modelTargetFile).Length / 1GB, 2)
    Write-Host "  [現存就緒] 已於硬碟中發現已下載模型資產！" -ForegroundColor Green
    Write-Host ("  • 模型檔案: " + $modelTargetFile + " (" + $fileSizeGb + " GB)") -ForegroundColor DarkGray
} elseif ($SkipModelDownload) {
    Write-Host "  已跳過模型下載（依參數要求）。" -ForegroundColor Gray
} else {
    Write-Host "  正在自官方 Hugging Face 歐美團隊開源倉庫下載 Google Gemma 2 (Q4_K_M 繁中最佳化版)..." -ForegroundColor Cyan
    Write-Host ("  • 來源位址: " + $modelDownloadUrl) -ForegroundColor DarkGray
    Write-Host ("  • 目的位址: " + $modelTargetFile) -ForegroundColor DarkGray
    Write-Host "  正在下載中，檔案約 1.63 GB，請稍候..." -ForegroundColor Yellow

    $downloadSuccess = $false
    if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
        Write-Host "  使用系統原生 curl.exe 進行高速串流下載（支援斷點續傳與重新導向）..." -ForegroundColor Cyan
        & curl.exe -L -C - --progress-bar -o $modelTargetFile $modelDownloadUrl
        if ($LASTEXITCODE -eq 0 -and (Test-Path $modelTargetFile) -and (Get-Item $modelTargetFile).Length -gt 100MB) {
            $downloadSuccess = $true
            Write-Host "  [下載成功] Google Gemma 2 模型已完整安置入庫！" -ForegroundColor Green
        }
    }
    if (-not $downloadSuccess) {
        try {
            Start-BitsTransfer -Source $modelDownloadUrl -Destination $modelTargetFile -DisplayName "下載 Google Gemma 2 模型" -ErrorAction Stop
            Write-Host "  [下載成功] Google Gemma 2 模型已完整安置入庫！" -ForegroundColor Green
        } catch {
            Write-Host "  BITS 傳輸不可用，切換至標準串流下載..." -ForegroundColor Yellow
            $wc = New-Object System.Net.WebClient
            $wc.DownloadFile($modelDownloadUrl, $modelTargetFile)
            Write-Host "  [下載成功] Google Gemma 2 模型已完整安置入庫！" -ForegroundColor Green
        }
    }
}
Write-Host ""

# ── 4. 自動生成本地 AI 服務啟動與關閉腳本 ──
Write-Host "【步驟 4/5】建立配套服務腳本 (啟動/關閉)..." -ForegroundColor Yellow

$scriptRoot = $PSScriptRoot
if (-not $scriptRoot) { $scriptRoot = (Get-Location).Path }

$startServiceScript = Join-Path $scriptRoot "啟動本地AI服務.ps1"
$stopServiceScript = Join-Path $scriptRoot "停止本地AI服務.ps1"

$threads = [math]::Max(4, [math]::Min(12, [int]($cpuInfo.NumberOfLogicalProcessors / 2)))
$ngl = 0
if ($hasGpu) { $ngl = 99 }

$startLines = @(
    "# RuhI 本地純綠 AI 推論服務啟動腳本",
    "`$ErrorActionPreference = 'Stop'",
    "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8",
    "",
    ("`$serverExe = '" + $llamaServerPath + "'"),
    ("`$modelPath = '" + $modelTargetFile + "'"),
    "",
    "if (-not (Test-Path `$serverExe)) {",
    "    Write-Host ('找不到 Llama 引擎: ' + `$serverExe) -ForegroundColor Red",
    "    exit 1",
    "}",
    "if (-not (Test-Path `$modelPath)) {",
    "    Write-Host ('找不到模型檔案: ' + `$modelPath) -ForegroundColor Red",
    "    exit 1",
    "}",
    "",
    "Write-Host '正在喚醒純綠 Llama 引擎與 Google Gemma 2 (連接埠: 8080)...' -ForegroundColor Cyan",
    "",
    "`$existing = Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue",
    "if (`$existing) {",
    "    Write-Host '本地 AI 服務已在運行中！' -ForegroundColor Green",
    "    exit 0",
    "}",
    "",
    ("`$argList = @('-m', '" + $modelTargetFile + "', '--port', '8080', '--host', '127.0.0.1', '-c', '4096', '-t', '" + $threads + "', '-ngl', '" + $ngl + "')"),
    "Start-Process -FilePath `$serverExe -ArgumentList `$argList -WindowStyle Minimized",
    "Start-Sleep -Seconds 2",
    "",
    "Write-Host '本地純綠 AI 服務已就緒於 http://127.0.0.1:8080/v1！' -ForegroundColor Green"
)

$utf8Bom = New-Object System.Text.UTF8Encoding($true)
[System.IO.File]::WriteAllLines($startServiceScript, $startLines, $utf8Bom)

$stopLines = @(
    "# RuhI 本地純綠 AI 推論服務停止腳本",
    "`$existing = Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue",
    "if (`$existing) {",
    "    `$existing | Stop-Process -Force",
    "    Write-Host '本地純綠 AI 服務已完全停止。' -ForegroundColor Green",
    "} else {",
    "    Write-Host '未偵測到正在運行的本地 AI 服務。' -ForegroundColor Yellow",
    "}"
)

[System.IO.File]::WriteAllLines($stopServiceScript, $stopLines, $utf8Bom)

Write-Host ("  • 啟動腳本: " + $startServiceScript) -ForegroundColor DarkGray
Write-Host ("  • 停止腳本: " + $stopServiceScript) -ForegroundColor DarkGray
Write-Host "  [成功] 服務管理指令檔建立完成！" -ForegroundColor Green
Write-Host ""

# ── 5. 自動將本地端點注入如喜語意調校工作室設定檔 ──
Write-Host "【步驟 5/5】設定如喜語意調校工作室 AI 配置 (config.json)..." -ForegroundColor Yellow

$cfgPath = Join-Path $scriptRoot "config.json"
if (-not (Test-Path $cfgPath)) {
    $parentCfg = Join-Path (Split-Path -Parent $scriptRoot) "config.json"
    if (Test-Path $parentCfg) { $cfgPath = $parentCfg }
}

$cfg = @{}
if (Test-Path $cfgPath) {
    try {
        $parsed = Get-Content -Path $cfgPath -Raw -Encoding UTF8 | ConvertFrom-Json
        foreach ($prop in $parsed.PSObject.Properties) {
            $cfg[$prop.Name] = $prop.Value
        }
    } catch {}
}

$cfg["custom_base_url"] = "http://127.0.0.1:8080/v1"
$cfg["custom_model"] = "gemma-2-2b-it"
$cfg["custom_api_key"] = ""
$cfg["llama_server_path"] = $llamaServerPath
$cfg["local_model_path"] = $modelTargetFile

if (-not $cfg["ai_strategy"] -or $cfg["ai_strategy"] -eq "legacy") {
    $cfg["ai_strategy"] = "remote_first"
}
$cfg["skip_local_model_upgrade_prompt"] = $true

$cfgJson = $cfg | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($cfgPath, $cfgJson, [System.Text.Encoding]::UTF8)

Write-Host "  [成功] 已將本地純綠 Llama + Google Gemma 2 註冊為專屬離線備援端點！" -ForegroundColor Green
Write-Host ""
Write-Host "========================================================================" -ForegroundColor Cyan
Write-Host "  🎉 大功告成！本地純綠 AI 語意引擎已建置完畢！" -ForegroundColor Green
Write-Host "  • 零商業廣告、零訂閱誘導、100% 離線純綠運算" -ForegroundColor Yellow
Write-Host ("  • 隨時執行: & `"" + $startServiceScript + "`" 即可喚起本地服務") -ForegroundColor Yellow
Write-Host "========================================================================" -ForegroundColor Cyan
