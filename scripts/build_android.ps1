# RuhI (如喜) Android 外接鍵盤版一鍵編譯與部署腳本
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$IncludeX86_64,
    [switch]$Install,
    [switch]$SkipRust
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$RepoRoot = Split-Path -Parent $PSScriptRoot
$JniDir = Join-Path $RepoRoot "ruhi-android-jni"
$AndroidDir = Join-Path $RepoRoot "ruhi-android"
$SrcDataDir = Join-Path $RepoRoot "ruhi_lexicon\data"
$AssetsDataDir = Join-Path $AndroidDir "app\src\main\assets\ruhi_data"
$JniLibsDir = Join-Path $AndroidDir "app\src\main\jniLibs"

# 1. 偵測 Android SDK 與 NDK
$sdkCandidates = @(
    $env:ANDROID_HOME,
    $env:ANDROID_SDK_ROOT,
    (Join-Path $env:LOCALAPPDATA "Android\Sdk"),
    "C:\Android\Sdk"
) | Where-Object { $_ -and (Test-Path $_) }

if (-not $sdkCandidates -or $sdkCandidates.Count -eq 0) {
    throw "找不到 Android SDK！請確認 ANDROID_HOME 或 %LOCALAPPDATA%\Android\Sdk。"
}
$SdkDir = ($sdkCandidates | Select-Object -First 1)
$env:ANDROID_HOME = $SdkDir
$env:ANDROID_SDK_ROOT = $SdkDir

$ndkBase = Join-Path $SdkDir "ndk"
if (-not (Test-Path $ndkBase)) {
    throw "找不到 Android NDK 目錄：$ndkBase"
}
$NdkDir = Get-ChildItem -Path $ndkBase -Directory | Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $NdkDir) {
    throw "未在 $ndkBase 發現已安裝之 NDK 版本。"
}
$env:ANDROID_NDK_HOME = $NdkDir
Write-Host "[Android SDK] $SdkDir" -ForegroundColor Cyan
Write-Host "[Android NDK] $NdkDir" -ForegroundColor Cyan

# 2. 同步精簡核心詞庫與梵語包（排除龍龕、倉頡、行列、四角、日韓）
$curatedFiles = @(
    "lexicon_core.tsv",
    "lexicon_english.tsv",
    "transitions_user.tsv",
    "patterns_user.jsonl",
    "feature_weights.tsv",
    "symbols.tsv",
    "symbol_tree.tsv",
    "templates.tsv",
    "packs\pack_western_sanskrit.tsv"
)

New-Item -ItemType Directory -Force -Path $AssetsDataDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $AssetsDataDir "packs") | Out-Null

$hashLines = New-Object System.Collections.Generic.List[string]
foreach ($rel in $curatedFiles) {
    $src = Join-Path $SrcDataDir $rel
    if (-not (Test-Path $src)) {
        throw "找不到必要資料檔：$src"
    }
    $dst = Join-Path $AssetsDataDir $rel
    Copy-Item -Path $src -Destination $dst -Force
    $h = (Get-FileHash -Path $dst -Algorithm SHA256).Hash
    $hashLines.Add("$rel=$h")
}

$joinedHashes = [System.Text.Encoding]::UTF8.GetBytes(($hashLines -join "`n"))
$sha256 = [System.Security.Cryptography.SHA256]::Create()
try {
    $digestBytes = $sha256.ComputeHash($joinedHashes)
    $manifestHash = ([System.BitConverter]::ToString($digestBytes)).Replace("-", "").ToLowerInvariant()
} finally {
    $sha256.Dispose()
}
$manifestPath = Join-Path $AssetsDataDir "manifest_version.txt"
[System.IO.File]::WriteAllText($manifestPath, $manifestHash + "`n", (New-Object System.Text.UTF8Encoding($false)))
Write-Host "[Assets] 已同步 9 份精選資料檔，版本雜湊：$manifestHash" -ForegroundColor Green

# 3. 編譯原生 ARM64 (及可選 x86_64) libruhi_android_jni.so（含 16 KB 分頁對齊）
$abis = @("arm64-v8a")
if ($IncludeX86_64) {
    $abis += "x86_64"
}

if (-not $SkipRust) {
    $prevRustFlags = $env:RUSTFLAGS
    $pageFlag = "-C link-arg=-Wl,-z,max-page-size=16384"
    if ([string]::IsNullOrWhiteSpace($prevRustFlags)) {
        $env:RUSTFLAGS = $pageFlag
    } else {
        $env:RUSTFLAGS = "$prevRustFlags $pageFlag"
    }
    try {
        New-Item -ItemType Directory -Force -Path $JniLibsDir | Out-Null
        $ndkArgs = @()
        foreach ($abi in $abis) {
            $ndkArgs += "-t"
            $ndkArgs += $abi
        }
        $ndkArgs += @(
            "-P", "29",
            "-o", $JniLibsDir,
            "build", "--release",
            "--manifest-path", (Join-Path $JniDir "Cargo.toml")
        )
        Write-Host "[Rust NDK] 開始編譯 ($($abis -join ", "))..." -ForegroundColor Cyan
        Push-Location $JniDir
        try {
            & cargo ndk @ndkArgs
            if ($LASTEXITCODE -ne 0) {
                throw "cargo ndk 編譯失敗 (ExitCode=$LASTEXITCODE)"
            }
        } finally {
            Pop-Location
        }
    } finally {
        $env:RUSTFLAGS = $prevRustFlags
    }
}

# ruhi-core 同時宣告 cdylib 產出之冗餘 libruhi.so 不需隨 APK 打包（已靜態連結至 libruhi_android_jni.so）
Get-ChildItem -Path $JniLibsDir -Recurse -Filter "libruhi.so" -ErrorAction SilentlyContinue | Remove-Item -Force

foreach ($abi in $abis) {
    $soPath = Join-Path $JniLibsDir "$abi\libruhi_android_jni.so"
    if (Test-Path $soPath) {
        $mb = [math]::Round((Get-Item $soPath).Length / 1MB, 2)
        Write-Host "[Native SO] $abi -> $soPath ($mb MB)" -ForegroundColor Green
    }
}

# 4. 建置 Android APK
$gradleTask = if ($Release) { "assembleRelease" } else { "assembleDebug" }
$abiProp = "-PruhiAbis=" + ($abis -join ",")
$gradlew = Join-Path $AndroidDir "gradlew.bat"

Push-Location $AndroidDir
try {
    Write-Host "[Gradle] 執行 $gradleTask ($abiProp)..." -ForegroundColor Cyan
    & $gradlew $gradleTask $abiProp
    if ($LASTEXITCODE -ne 0) {
        throw "Gradle $gradleTask 建置失敗 (ExitCode=$LASTEXITCODE)"
    }
} finally {
    Pop-Location
}

$apkSubPath = if ($Release) {
    "app\build\outputs\apk\release\app-release-unsigned.apk"
} else {
    "app\build\outputs\apk\debug\app-debug.apk"
}
$apkPath = Join-Path $AndroidDir $apkSubPath
if ($Release -and -not (Test-Path $apkPath)) {
    $signedCandidate = Join-Path $AndroidDir "app\build\outputs\apk\release\app-release.apk"
    if (Test-Path $signedCandidate) {
        $apkPath = $signedCandidate
    }
}

if (-not (Test-Path $apkPath)) {
    throw "找不到產出之 APK 檔案：$apkPath"
}

$apkMb = [math]::Round((Get-Item $apkPath).Length / 1MB, 2)
Write-Host "[APK 完成] $apkPath ($apkMb MB)" -ForegroundColor Green

# 5. 若指定 -Install，透過 adb 覆蓋安裝至已連線 Android 裝置
if ($Install) {
    $adb = Join-Path $SdkDir "platform-tools\adb.exe"
    if (-not (Test-Path $adb)) {
        throw "找不到 adb.exe：$adb"
    }
    Write-Host "[ADB] 正在安裝 APK 至實機..." -ForegroundColor Cyan
    & $adb install -r $apkPath
    if ($LASTEXITCODE -ne 0) {
        throw "adb install -r 失敗 (ExitCode=$LASTEXITCODE)"
    }
    Write-Host "[ADB] 安裝完成！" -ForegroundColor Green
}
