# 如喜 (RuhI) 強制品質閘門腳本 (Quality Gate)
# 依序執行：cargo test --release (ruhi-core) -> cargo build --release (ruhi-tsf) -> ruhi_eval --gate
# 任一步失敗即中止，並以非 0 狀態碼結束。本腳本無需系統管理員權限。

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$RuhiCoreDir = Join-Path $RepoRoot "ruhi-core"
$RuhiTsfDir = Join-Path $RepoRoot "ruhi-tsf"
$BaselinePath = Join-Path $RepoRoot "eval_reports\dev_gate_baseline.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "[Quality Gate 1/3] 執行 ruhi-core 單元測試 (cargo test --release)..." -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Push-Location $RuhiCoreDir
try {
    & cargo test --release
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[Quality Gate 失敗] ruhi-core 單元測試未通過 (ExitCode=$LASTEXITCODE)。" -ForegroundColor Red
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "[Quality Gate 2/3] 建置 ruhi-tsf Release 版本 (cargo build --release)..." -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Push-Location $RuhiTsfDir
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[Quality Gate 失敗] ruhi-tsf 建置失敗 (ExitCode=$LASTEXITCODE)。" -ForegroundColor Red
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "[Quality Gate 3/3] 執行開發集品質閘門評測 (ruhi_eval --gate)..." -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Push-Location $RuhiCoreDir
try {
    & cargo run --release --bin ruhi_eval -- --gate --baseline $BaselinePath
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[Quality Gate 失敗] 開發集評測未通過品質閘門基準 (ExitCode=$LASTEXITCODE)。" -ForegroundColor Red
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Write-Host "============================================================" -ForegroundColor Green
Write-Host "[Quality Gate 通過] 所有測試、TSF 建置與開發集 CER 品質閘門皆通過！" -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
exit 0
