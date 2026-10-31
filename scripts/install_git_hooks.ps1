# 安裝如喜 (RuhI) Git pre-commit 品質閘門 Hook
# 本腳本無需系統管理員權限。

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$HooksDir = Join-Path $RepoRoot ".git\hooks"
$PreCommitPath = Join-Path $HooksDir "pre-commit"

if (-not (Test-Path $HooksDir)) {
    Write-Error "找不到 .git/hooks 目錄：$HooksDir"
    exit 1
}

$HookLines = @(
    "#!/bin/sh",
    "# 如喜 (RuhI) 自動化品質閘門 pre-commit hook",
    "# 由 scripts/install_git_hooks.ps1 安裝",
    "",
    "STAGED_FILES=`$(git diff --cached --name-only)",
    "if [ -z `"`$STAGED_FILES`" ]; then",
    "    exit 0",
    "fi",
    "",
    "NEEDS_FULL_GATE=0",
    "for FILE in `$STAGED_FILES; do",
    "    case `"`$FILE`" in",
    "        ruhi-core/src/router/*|ruhi-core/src/engine.rs|ruhi-core/src/lexicon/*|ruhi-core/src/fsm/*|ruhi_lexicon/data/*.tsv|ruhi_lexicon/data/*.jsonl)",
    "            NEEDS_FULL_GATE=1",
    "            break",
    "            ;;",
    "    esac",
    "done",
    "",
    "if [ `"`$NEEDS_FULL_GATE`" -eq 1 ]; then",
    "    echo `"[pre-commit] 偵測到評分相關檔案變更，啟動完整品質閘門 (scripts/quality_gate.ps1)...`"",
    "    powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"scripts/quality_gate.ps1`"",
    "    STATUS=`$?",
    "    if [ `$STATUS -ne 0 ]; then",
    "        echo `"[pre-commit] 品質閘門未通過，拒絕提交！`"",
    "        exit `$STATUS",
    "    fi",
    "else",
    "    echo `"[pre-commit] 非評分相關變更，執行 ruhi-core 單元測試 (cargo test --release)...`"",
    "    (cd ruhi-core && cargo test --release)",
    "    STATUS=`$?",
    "    if [ `$STATUS -ne 0 ]; then",
    "        echo `"[pre-commit] 單元測試未通過，拒絕提交！`"",
    "        exit `$STATUS",
    "    fi",
    "fi",
    "",
    "exit 0",
    ""
)

$HookContent = [string]::Join([char]10, $HookLines)
[System.IO.File]::WriteAllText($PreCommitPath, $HookContent, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "[install_git_hooks] 已成功安裝 pre-commit hook 至：$PreCommitPath" -ForegroundColor Green
