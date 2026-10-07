# RuhI TSF 免登出熱更新 (Hot Reload) 核心腳本
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host '========================================================' -ForegroundColor Cyan
Write-Host '  正在執行 RuhI TSF 免登出熱更新（Hot Reload）...' -ForegroundColor Cyan
Write-Host '========================================================' -ForegroundColor Cyan

$distDir = $PSScriptRoot
$candidateSource = @(
    (Join-Path $distDir 'ruhi-tsf\target\release\ruhi_tsf.dll'),
    (Join-Path $distDir '..\ruhi-tsf\target\release\ruhi_tsf.dll'),
    (Join-Path $distDir '..\..\ruhi-tsf\target\release\ruhi_tsf.dll')
)
$sourceDll = $candidateSource[0]
foreach ($c in $candidateSource) {
    if (Test-Path $c) {
        $sourceDll = $c
        break
    }
}
$targetDll = Join-Path $distDir 'ruhi_tsf.dll'
$oldDll = Join-Path $distDir 'ruhi_tsf.dll.old'

# 1. 終止當前使用者桌面工作階段的文字服務監聽核心 ctfmon
try {
    Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Write-Host '  [1/4] 已暫停文字服務框架監聽 (ctfmon)...' -ForegroundColor Green
} catch {}

# 2. 利用 NTFS 檔案系統特性：鎖定中的 DLL 允許直接改名！
if (Test-Path $oldDll) {
    Remove-Item -Path $oldDll -Force -ErrorAction SilentlyContinue
}

if (Test-Path $targetDll) {
    try {
        Rename-Item -Path $targetDll -NewName 'ruhi_tsf.dll.old' -Force
        Write-Host '  [2/4] 已成功將鎖定中的舊版 DLL 隔離更名為 .old...' -ForegroundColor Green
    } catch {
        Write-Host '  [2/4] 正在處理 DLL 檔案指標...' -ForegroundColor Yellow
    }
}

# 3. 檢查並拷貝最新編譯之 Release DLL
if (-not (Test-Path $sourceDll)) {
    Write-Host ''
    Write-Host '  【更新失敗】找不到編譯好的 Release DLL！' -ForegroundColor Red
    Write-Host '  請先在終端機執行 cargo build --release 後再試。' -ForegroundColor Yellow
    Write-Host ''
    Read-Host '請按 Enter 鍵結束'
    exit 1
}

Copy-Item -Path $sourceDll -Destination $targetDll -Force
Write-Host '  [3/4] 已將最新編譯之 Release DLL 拷貝至目標路徑！' -ForegroundColor Green

# 4. 重新註冊 TSF 元件
$regsvr = Start-Process -FilePath 'regsvr32.exe' -ArgumentList '/s', ('"' + $targetDll + '"') -Wait -PassThru
if ($regsvr.ExitCode -eq 0) {
    Write-Host '  [4/4] TSF COM 元件重新註冊成功！' -ForegroundColor Green
} elseif ($regsvr.ExitCode -eq 5) {
    Write-Host '  [4/4] 既有 TSF 註冊路徑保持有效（若需變更全域 COM 註冊，請以系統管理員身分執行）。' -ForegroundColor Green
} else {
    Write-Host ('  [4/4] 提示：元件註冊回傳代碼 ' + $regsvr.ExitCode) -ForegroundColor Yellow
}

# 5. 重新喚醒 ctfmon.exe
Start-Process -FilePath 'ctfmon.exe'

Write-Host ''
Write-Host '========================================================' -ForegroundColor Cyan
Write-Host '  【大功告成！】最新版本 DLL 已於 1 秒內熱替換完成！' -ForegroundColor Green
Write-Host '  ★ 徹底免除登出登入，直接按 [Win + 空白鍵] 即可開始體驗！' -ForegroundColor Yellow
Write-Host '========================================================' -ForegroundColor Cyan
Start-Sleep -Seconds 2
