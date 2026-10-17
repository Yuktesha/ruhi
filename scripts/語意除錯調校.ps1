<#
.SYNOPSIS
    如喜輸入法 (RuhI) - 語意除錯與自適應調校工作室進入點 (具備環境自檢與一鍵修復)
.DESCRIPTION
    啟動如喜輸入法語意調校工具，支援：
    1. 環境條件自動偵測：智慧探測 Python 3，過濾微軟商店假替身
    2. 遇缺智慧引導：無 Python 時提供 Windows winget 一鍵自動靜默安裝
    3. 現代化 Web HTML 親和視覺介面 (預設自動彈出瀏覽器)
    4. 雙句差分比對 (手動輸入錯誤句與更正句調校)
    5. 一鍵自適應同步 (讀取輸入座修訂記錄，化一切於無形)
    6. 黃金語料防回歸驗證 (Zero-Regression Golden Check)
#>
param(
    [string]$ErrorSentence = '',
    [string]$CorrectSentence = '',
    [switch]$AutoAlign,
    [switch]$Verify,
    [switch]$Tailcat,
    [switch]$Cli,
    [switch]$Web
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$Host.UI.RawUI.WindowTitle = '如喜輸入法語意調校工作室 (Ruhi Semantic Tuning Studio)'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$TunerScript = Join-Path $ScriptDir 'semantic_tuner.py'

# ── 1. 智慧探測可用之 Python 3 執行環境 ──
function Test-PythonExe([string]$exePath) {
    if (-not $exePath) { return $false }
    try {
        $pinfo = New-Object System.Diagnostics.ProcessStartInfo
        $pinfo.FileName = $exePath
        $pinfo.Arguments = '-c "import sys; print(sys.version_info.major)"'
        $pinfo.RedirectStandardOutput = $true
        $pinfo.RedirectStandardError = $true
        $pinfo.UseShellExecute = $false
        $pinfo.CreateNoWindow = $true

        $proc = [System.Diagnostics.Process]::Start($pinfo)
        $output = $proc.StandardOutput.ReadToEnd().Trim()
        $proc.WaitForExit(3000)

        if ($proc.ExitCode -eq 0 -and $output -eq '3') {
            return $true
        }
    } catch {
        return $false
    }
    return $false
}

$ValidPython = $null

# 優先順序：py -> python -> 常見安裝路徑
$Candidates = @('py', 'python')

# 掃描常見本機目錄
$LocalApp = [System.Environment]::GetFolderPath('LocalApplicationData')
if ($LocalApp) {
    $PyDirs = Get-ChildItem -Path "$LocalApp\Programs\Python" -Filter "Python3*" -ErrorAction SilentlyContinue
    foreach ($d in $PyDirs) {
        $Candidates += (Join-Path $d.FullName 'python.exe')
    }
}
$SystemDrives = @('C:\Python312\python.exe', 'C:\Python311\python.exe', 'C:\Python310\python.exe', 'C:\Python39\python.exe', 'C:\Program Files\Python311\python.exe', 'C:\Program Files\Python312\python.exe')
foreach ($sd in $SystemDrives) {
    if (Test-Path $sd) {
        $Candidates += $sd
    }
}

foreach ($cand in $Candidates) {
    if (Test-PythonExe $cand) {
        $ValidPython = $cand
        break
    }
}

# ── 2. 遇缺智慧引導機制 (遇缺不閃退，親和引導初次安裝) ──
if (-not $ValidPython) {
    Write-Host ''
    Write-Host '========================================================================' -ForegroundColor Cyan
    Write-Host '  如喜輸入法 (RuhI) · 語意調校工作室 - 系統環境健全度診斷' -ForegroundColor Yellow
    Write-Host '========================================================================' -ForegroundColor Cyan
    Write-Host ''
    Write-Host '【診斷報告】偵測到目前 Windows 系統尚未安裝 Python 3 執行環境。' -ForegroundColor Red
    Write-Host '語意調校工作室之後端語言學分析模組需要 Python 3 作為輕量支援。' -ForegroundColor Gray
    Write-Host ''
    Write-Host '為您提供以下一鍵解決方案：' -ForegroundColor White
    Write-Host '  [1] 一鍵自動安裝 (透過 Windows 內建 winget 自動下載安裝 Python 3.11)' -ForegroundColor Green
    Write-Host '  [2] 打開瀏覽器前往 Python 官方下載網站手動安裝' -ForegroundColor Cyan
    Write-Host '  [0] 離開' -ForegroundColor Gray
    Write-Host ''

    $choice = Read-Host '請輸入選項 [1 / 2 / 0]'
    if ($choice -eq '1') {
        if (Get-Command winget -ErrorAction SilentlyContinue) {
            Write-Host '正在透過 Windows 內建套件管理員 (winget) 安裝 Python 3.11...' -ForegroundColor Cyan
            Write-Host '此過程完全自動，請稍候約 1~2 分鐘...' -ForegroundColor Gray
            & winget install Python.Python.3.11 --silent --accept-package-agreements --accept-source-agreements
            
            # 刷新 PATH
            $env:Path = [System.Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [System.Environment]::GetEnvironmentVariable('Path', 'User')
            
            # 重新檢查
            if (Test-PythonExe 'python') {
                $ValidPython = 'python'
                Write-Host '✅ Python 安裝成功！即將啟動語意調校工作室...' -ForegroundColor Green
                Start-Sleep -Seconds 2
            } else {
                Write-Host '安裝已觸發，請在安裝完成後重新啟動本腳本！' -ForegroundColor Yellow
                Read-Host '按 Enter 鍵結束...'
                exit
            }
        } else {
            Write-Host '您的系統未搭載 winget，正在為您打開官方下載頁面...' -ForegroundColor Yellow
            Start-Process 'https://www.python.org/downloads/'
            Read-Host '請下載安裝 Python 並勾選 "Add Python to PATH"，安裝完成後按 Enter 結束...'
            exit
        }
    } elseif ($choice -eq '2') {
        Write-Host '正在為您打開 Python 官方網站...' -ForegroundColor Cyan
        Start-Process 'https://www.python.org/downloads/'
        Write-Host '提示：安裝時請務必勾選「Add python.exe to PATH」！' -ForegroundColor Yellow
        Read-Host '安裝完成後按 Enter 結束...'
        exit
    } else {
        exit
    }
}

# ── 2.5 智慧偵測並連帶喚醒本機純綠 AI 引擎 (零手動負擔) ──
$LocalAiScript = Join-Path $ScriptDir '啟動本地AI服務.ps1'
if (Test-Path $LocalAiScript) {
    $runningLlama = Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue
    if (-not $runningLlama) {
        Write-Host '偵測到本機純綠 AI 引擎 (Llama + Gemma 2) 尚未運行，正在自動連帶喚醒...' -ForegroundColor Cyan
        try {
            & $LocalAiScript
        } catch {
            Write-Host "本機 AI 引擎啟動提示：$_" -ForegroundColor DarkGray
        }
    }
}

# ── 3. 準備啟動參數並喚醒工作室 ──
$ArgsList = @($TunerScript)
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
} elseif ($Cli) {
    $ArgsList += '--cli'
} else {
    $ArgsList += '--web'
}

try {
    & $ValidPython @ArgsList
} catch {
    Write-Host "執行過程發生錯誤：$_" -ForegroundColor Red
    Read-Host '按 Enter 鍵關閉視窗...'
}
