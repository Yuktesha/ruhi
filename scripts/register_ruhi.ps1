. "C:\ATGprjs\_lib\ps\Ensure-Admin.ps1"

# RuhI 輸入法全域 COM 註冊路徑歸正腳本
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在將 RuhI (如喜) COM 註冊路徑正式歸入 C:\ATGprjs\ruhi..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

$distDll = "C:\ATGprjs\ruhi\dist\ruhi_tsf.dll"
if (-not (Test-Path $distDll)) {
    $src = "C:\ATGprjs\ruhi\ruhi-tsf\target\release\ruhi_tsf.dll"
    if (Test-Path $src) {
        $parent = Split-Path -Parent $distDll
        if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
        Copy-Item $src $distDll -Force
    } else {
        Write-Host "  [錯誤] 找不到發布版 DLL，請先執行 cargo build --release！" -ForegroundColor Red
        exit 1
    }
}

# 終止 ctfmon
try {
    Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
} catch {}
Start-Sleep -Milliseconds 300

# 執行 COM 註冊，自動將系統 InProcServer32 綁定至 C:\ATGprjs\ruhi\dist\ruhi_tsf.dll
$regsvr = Start-Process -FilePath "regsvr32.exe" -ArgumentList "/s", ('"' + $distDll + '"') -Wait -PassThru
if ($regsvr.ExitCode -eq 0) {
    Write-Host "  [成功] TSF COM 元件已成功註冊至: $distDll" -ForegroundColor Green
} else {
    Write-Host "  [警告] 註冊程序回傳代碼: $($regsvr.ExitCode)" -ForegroundColor Yellow
}

# 喚起 ctfmon
Start-Process "ctfmon.exe"
Write-Host "  [成功] 文字服務框架 (ctfmon) 已重新啟動！" -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  RuhI 已正式將註冊路徑歸入 C:\ATGprjs\ruhi！" -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Cyan
