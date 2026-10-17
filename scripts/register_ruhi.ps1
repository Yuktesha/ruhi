. "C:\ATGprjs\_lib\ps\Ensure-Admin.ps1"

# RuhI 輸入法全域 COM 註冊路徑歸正腳本
# 符合 ATG 全域守則：原生 UTF-8 BOM、乾淨純 TSF 規範、徹底支援 LibreOffice 等 Win32 程式
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

# 清除任何殘留的無效 SubstituteLayout（純 TSF TIP 絕不可包含無效鍵盤代碼）
try {
    $lpKey = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}\LanguageProfile\0x00000404\{F52A781B-8321-48E6-A145-C89B0981E7D2}"
    if (Test-Path $lpKey) {
        Remove-ItemProperty -Path $lpKey -Name "SubstituteLayout" -ErrorAction SilentlyContinue
    }
} catch {}

# 確保 HKCU Assemblies 與 SortOrder 關聯正確，徹底支援 LibreOffice / Photoshop 等 Win32 傳統桌面應用程式
try {
    $asmKey = "HKCU:\Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}"
    if (-not (Test-Path $asmKey)) { New-Item -Path $asmKey -Force | Out-Null }
    Set-ItemProperty -Path $asmKey -Name "Default" -Value "{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}" -Type String
    Set-ItemProperty -Path $asmKey -Name "Profile" -Value "{F52A781B-8321-48E6-A145-C89B0981E7D2}" -Type String
    Set-ItemProperty -Path $asmKey -Name "KeyboardLayout" -Value 0x04040404 -Type DWord

    $sortKey = "HKCU:\Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}\00000000"
    if (-not (Test-Path $sortKey)) { New-Item -Path $sortKey -Force | Out-Null }
    Set-ItemProperty -Path $sortKey -Name "CLSID" -Value "{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}" -Type String
    Set-ItemProperty -Path $sortKey -Name "Profile" -Value "{F52A781B-8321-48E6-A145-C89B0981E7D2}" -Type String
    Set-ItemProperty -Path $sortKey -Name "KeyboardLayout" -Value 0x04040404 -Type DWord

    Write-Host "  [成功] 已將如喜輸入法設為 0x0404 預設 Assembly 與有效鍵盤佈局 (0x04040404)！" -ForegroundColor Green
} catch {
    Write-Host "  [提示] 設定 Assemblies 時發生例外：$($_.Exception.Message)" -ForegroundColor Yellow
}

# 喚起 ctfmon
Start-Process "ctfmon.exe"
Write-Host "  [成功] 文字服務框架 (ctfmon) 已重新啟動！" -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  RuhI 已正式將註冊路徑歸入 C:\ATGprjs\ruhi！" -ForegroundColor Green
Write-Host "========================================================" -ForegroundColor Cyan
