#Requires -Version 5.1
param(
    [string]$ModelPath = "",
    [string]$PipeName = "\\.\pipe\RuhI_NeuralReranker_v1",
    [int]$Threads = 4,
    [switch]$Bench
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot

$ExeCandidates = @(
    (Join-Path $RepoRoot "ruhi-core\target\release\ruhi_reranker.exe"),
    (Join-Path $RepoRoot "dist\ruhi_reranker.exe"),
    (Join-Path $RepoRoot "ruhi_reranker.exe")
)
$ExePath = $ExeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $ExePath) {
    Write-Error "找不到 ruhi_reranker.exe，請先執行 cargo build --release --manifest-path ruhi-core\Cargo.toml --bin ruhi_reranker"
    exit 1
}

if ([string]::IsNullOrWhiteSpace($ModelPath)) {
    $ModelCandidates = @(
        (Join-Path $RepoRoot "ruhi_lexicon\data\rlnm_zh_tw.bin"),
        (Join-Path $RepoRoot "data\rlnm_zh_tw.bin"),
        (Join-Path $RepoRoot "dist\data\rlnm_zh_tw.bin")
    )
    $ModelPath = $ModelCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if ([string]::IsNullOrWhiteSpace($ModelPath) -or -not (Test-Path $ModelPath)) {
    Write-Error "找不到模型檔案 rlnm_zh_tw.bin，請透過 -ModelPath 指定路徑。"
    exit 1
}

$ArgsList = @("--model", $ModelPath, "--pipe", $PipeName, "--threads", $Threads.ToString())
if ($Bench) {
    $ArgsList += "--bench"
}

Write-Host ("啟動如喜神經語言模型重排服務：{0}" -f $ExePath)
Write-Host ("模型：{0}" -f $ModelPath)
Write-Host ("管線：{0} (執行緒：{1})" -f $PipeName, $Threads)
& $ExePath @ArgsList
exit $LASTEXITCODE
