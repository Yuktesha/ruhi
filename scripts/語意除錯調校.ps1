<#
.SYNOPSIS
    如喜輸入法 (RuhI) - 語意除錯與自適應調校工作室進入點 (Rust 原生執行檔優先)
.DESCRIPTION
    啟動如喜輸入法語意調校工具 (ruhi-studio.exe)，支援：
    1. 零外部依賴原生 Rust 引擎 (ruhi-studio.exe)，免安裝 Python 即可秒開
    2. 原生 egui 桌面模式 (-NativeGui) 與現代化 Web HTML 視覺介面 (-Web)
    3. 雙句差分比對 (-ErrorSentence / -CorrectSentence)
    4. 一鍵自適應修訂昇華 (-AutoAlign)
    5. 20 組黃金基準驗證 (-Verify) 與 Zero-PII Tailcat 提案 (-Tailcat)
#>
param(
    [string]$ErrorSentence = '',
    [string]$CorrectSentence = '',
    [switch]$AutoAlign,
    [switch]$Verify,
    [switch]$Tailcat,
    [switch]$Cli,
    [switch]$Web,
    [switch]$NativeGui
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$Host.UI.RawUI.WindowTitle = '如喜輸入法語意調校工作室 (Ruhi Semantic Tuning Studio - Rust Native)'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir

# ── 1. 智慧偵測並連帶喚醒本機純綠 AI 引擎 ──
$LocalAiScript = Join-Path $ScriptDir '啟動本地AI服務.ps1'
if (Test-Path $LocalAiScript) {
    $runningLlama = Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue
    if (-not $runningLlama) {
        try {
            & $LocalAiScript
        } catch {
            Write-Host "本機 AI 引擎啟動提示：$_" -ForegroundColor DarkGray
        }
    }
}

# ── 2. 優先探測原生 Rust 執行檔 ruhi-studio.exe ──
$RustCandidates = @(
    (Join-Path $RootDir 'ruhi-studio.exe'),
    (Join-Path $ScriptDir 'ruhi-studio.exe'),
    (Join-Path $RootDir 'ruhi-studio\target\release\ruhi-studio.exe'),
    (Join-Path $RootDir 'dist\ruhi-studio.exe')
)

$RustExe = $null
foreach ($rc in $RustCandidates) {
    if (Test-Path $rc) {
        $RustExe = (Resolve-Path $rc).Path
        break
    }
}

$ArgsList = @()
if ($AutoAlign) {
    $ArgsList += '--auto-align'
} elseif ($Verify) {
    $ArgsList += '--verify'
} elseif ($ErrorSentence -ne '' -and $CorrectSentence -ne '') {
    $ArgsList += '--error'
    $ArgsList += $ErrorSentence
    $ArgsList += '--correct'
    $ArgsList += $CorrectSentence
    if ($Tailcat) {
        $ArgsList += '--tailcat'
    }
} elseif ($Tailcat) {
    $ArgsList += '--tailcat'
} elseif ($Cli) {
    $ArgsList += '--cli'
} elseif ($NativeGui) {
    $ArgsList += '--tuner'
} else {
    $ArgsList += '--web'
}

if ($RustExe) {
    & $RustExe @ArgsList
    exit $LASTEXITCODE
}

# ── 3. 備援：若無編譯執行檔則回退至 Python 腳本 ──
$TunerScript = Join-Path $ScriptDir 'semantic_tuner.py'
$PyArgs = @($TunerScript) + $ArgsList
& python @PyArgs
