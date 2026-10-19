# RuhI 通用智慧型輸入法：全自動免登出熱更新 (Hot Reload & Deploy)
# 符合 ATG 全域守則：原生 UTF-8 BOM、動態路徑解耦、try-finally 守護 ctfmon
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在執行 RuhI TSF 全自動免登出熱更新 (Hot Reload)..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

# 1. 動態推導專案根目錄
$scriptDir = $PSScriptRoot
$rootDir = $scriptDir
if (-not (Test-Path (Join-Path $rootDir "ruhi-tsf"))) {
    if (Test-Path (Join-Path $scriptDir "..\ruhi-tsf")) {
        $rootDir = (Resolve-Path (Join-Path $scriptDir "..")).Path
    }
}

Write-Host "  專案根目錄: $rootDir" -ForegroundColor DarkGray

# 2. 尋找最新編譯之 Release DLL
$sourceDllCandidates = @(
    (Join-Path $rootDir "ruhi-tsf\target\release\ruhi_tsf.dll"),
    (Join-Path $rootDir "target\release\ruhi_tsf.dll")
)

$sourceDll = $null
foreach ($cand in $sourceDllCandidates) {
    if (Test-Path $cand) {
        $sourceDll = (Resolve-Path $cand).Path
        break
    }
}

if (-not $sourceDll) {
    Write-Host ""
    Write-Host "  【錯誤】找不到編譯好的 Release DLL！" -ForegroundColor Red
    Write-Host "  請先在終端機執行: cargo build --release --manifest-path ruhi-tsf/Cargo.toml" -ForegroundColor Yellow
    Write-Host ""
    Read-Host "請按 Enter 鍵結束"
    exit 1
}

$sourceTime = (Get-Item $sourceDll).LastWriteTime
Write-Host "  [1/4] 找到最新 Release DLL: $sourceDll" -ForegroundColor Green
Write-Host "        建置時間: $sourceTime" -ForegroundColor DarkGray

# 3. 動態偵測 Windows 註冊表中目前實際綁定載入的 DLL 目標路徑
$regTarget = $null
try {
    $regVal = Get-ItemPropertyValue -Path "Registry::HKEY_CLASSES_ROOT\CLSID\{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}\InprocServer32" -Name "(default)" -ErrorAction SilentlyContinue
    if ($regVal -and (Test-Path (Split-Path -Parent $regVal))) {
        $regTarget = $regVal
    }
} catch {}

# 定義所有必須同步熱替換的目標
$targetPaths = [System.Collections.Generic.List[string]]::new()
if ($regTarget) {
    $targetPaths.Add($regTarget)
}
# 預設發布目錄與根目錄
$defaultDist = Join-Path $rootDir "dist\ruhi_tsf.dll"
$defaultRoot = Join-Path $rootDir "ruhi_tsf.dll"

if (-not $targetPaths.Contains($defaultDist)) { $targetPaths.Add($defaultDist) }
if (-not $targetPaths.Contains($defaultRoot)) { $targetPaths.Add($defaultRoot) }

Write-Host "  [2/4] 目標部署路徑 ($($targetPaths.Count) 處):" -ForegroundColor Green
foreach ($t in $targetPaths) {
    Write-Host "        -> $t" -ForegroundColor DarkGray
}

$updatedCount = 0

# 4. 執行核心熱替換程序（以 try-finally 鐵律絕對守護 ctfmon 生命週期）
try {
    # (a) 暫停目前桌面工作階段的文字服務監聽核心 ctfmon
    try {
        Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Write-Host "  [3/4] 已暫停文字服務框架監聽 (ctfmon)..." -ForegroundColor Green
    } catch {}
    Start-Sleep -Milliseconds 200

    # (b) 逐一執行安全隔離替換
    foreach ($target in $targetPaths) {
        $parent = Split-Path -Parent $target
        if (-not (Test-Path $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }

        if (Test-Path $target) {
            # 清理過期未被鎖定的 .old 殘留檔
            $leafName = Split-Path -Leaf $target
            Get-ChildItem -Path $parent -Filter "$leafName*.old" -ErrorAction SilentlyContinue | ForEach-Object {
                Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue
            }

            # 利用 NTFS 允許重命名鎖定中檔案的特性，加上時間戳避免碰撞
            $timestamp = Get-Date -Format 'yyyyMMdd_HHmmss'
            $renamed = $false
            $candidateOldNames = @(
                "$target.old",
                "$target.$timestamp.old"
            )

            foreach ($oldName in $candidateOldNames) {
                try {
                    Rename-Item -Path $target -NewName (Split-Path -Leaf $oldName) -Force -ErrorAction Stop
                    $renamed = $true
                    break
                } catch {}
            }

            if (-not $renamed) {
                Write-Host "        [略過] 檔案被完全鎖定且無法更名: $target" -ForegroundColor Yellow
                continue
            }
        }

        try {
            Copy-Item -Path $sourceDll -Destination $target -Force -ErrorAction Stop
            Write-Host "        [替換成功] $target" -ForegroundColor Green
            $updatedCount++
        } catch {
            Write-Host "        [警告] 拷貝失敗: $target ($($_.Exception.Message))" -ForegroundColor Yellow
        }

        # (c) 同步完整詞庫與偏好設定至該目標目錄，保證絕不出現殘缺字典！
        $targetDataDir = Join-Path $parent "data"
        $sourceDataDir = Join-Path $rootDir "ruhi_lexicon\data"
        if (Test-Path $sourceDataDir) {
            if (-not (Test-Path $targetDataDir)) {
                New-Item -ItemType Directory -Path $targetDataDir -Force | Out-Null
            }
            # 同步所有 tsv / jsonl 詞庫與碼表
            Get-ChildItem -Path $sourceDataDir -Recurse -File -Include "*.tsv", "*.jsonl", "*.json" -ErrorAction SilentlyContinue | ForEach-Object {
                $relPart = $_.FullName.Substring($sourceDataDir.Length).TrimStart('\', '/')
                $destSubFile = Join-Path $targetDataDir $relPart
                $destSubParent = Split-Path -Parent $destSubFile
                if (-not (Test-Path $destSubParent)) { New-Item -ItemType Directory -Path $destSubParent -Force | Out-Null }
                if ((-not (Test-Path $destSubFile)) -or ($_.LastWriteTime -gt (Get-Item $destSubFile).LastWriteTime)) {
                    Copy-Item $_.FullName -Destination $destSubFile -Force -ErrorAction SilentlyContinue
                }
            }
        }

        # 同步純 Rust 偏好設定與語意調校執行檔 (ruhi-studio.exe) 與設定檔至該目標目錄
        $studioSrcCandidates = @(
            (Join-Path $rootDir "ruhi-studio\target\release\ruhi-studio.exe"),
            (Join-Path $rootDir "ruhi-studio.exe")
        )
        foreach ($sc in $studioSrcCandidates) {
            if (Test-Path $sc) {
                $dstStudio = Join-Path $parent "ruhi-studio.exe"
                if ($sc -ne $dstStudio) {
                    Copy-Item $sc -Destination $dstStudio -Force -ErrorAction SilentlyContinue
                }
                break
            }
        }
        foreach ($sf in @("settings_window.py", "settings_config.json", "config.json")) {
            $srcF = Join-Path $rootDir $sf
            if (Test-Path $srcF) {
                $dstF = Join-Path $parent $sf
                if ($srcF -ne $dstF) {
                    Copy-Item $srcF -Destination $dstF -Force -ErrorAction SilentlyContinue
                }
            }
        }
    }

    # (d) 同步單一真實詞庫來源 (ruhi_lexicon\data) 至專案內所有執行期與測試用 data 目錄
    $canonicalDataDir = Join-Path $rootDir "ruhi_lexicon\data"
    $workspaceDataTargets = @(
        (Join-Path $rootDir "data"),
        (Join-Path $rootDir "scripts\data"),
        (Join-Path $rootDir "dist\data"),
        (Join-Path $rootDir "dist\RuhI_v0.3.0_Windows_x64\data")
    )
    if (Test-Path $canonicalDataDir) {
        foreach ($wTarget in $workspaceDataTargets) {
            if (Test-Path $wTarget) {
                Get-ChildItem -Path $canonicalDataDir -File -Include "*.tsv", "*.jsonl", "*.json" -ErrorAction SilentlyContinue | ForEach-Object {
                    $destWFile = Join-Path $wTarget $_.Name
                    if ((-not (Test-Path $destWFile)) -or ($_.LastWriteTime -gt (Get-Item $destWFile).LastWriteTime)) {
                        Copy-Item $_.FullName -Destination $destWFile -Force -ErrorAction SilentlyContinue
                    }
                }
            }
        }
    }
} finally {
    # 鐵律：無論熱替換成功或失敗，100% 必須保證重啟文字服務框架
    Start-Process "ctfmon.exe"
    Write-Host "  [4/4] 已安全重啟文字服務框架監聽 (ctfmon)！" -ForegroundColor Green
}

Write-Host ""
Write-Host "========================================================" -ForegroundColor Cyan
if ($updatedCount -gt 0) {
    Write-Host "  【大功告成！】最新版本 DLL 與完整詞庫已成功熱替換完成！" -ForegroundColor Green
    Write-Host "  ★ 徹底免除登出登入，直接按 [Win + 空白鍵] 即可開始體驗！" -ForegroundColor Yellow
} else {
    Write-Host "  【提示】未更新任何目標，請檢查權限或建置輸出。" -ForegroundColor Yellow
}
Write-Host "========================================================" -ForegroundColor Cyan
Start-Sleep -Seconds 1
