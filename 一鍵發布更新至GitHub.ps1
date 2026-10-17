# ========================================================================
# 如喜 (RuhI) 輸入法 · 一鍵同步更新至 GitHub 輔助工具
# 專為免記憶 Git 指令設計：自動打包、提交變更並推送到 GitHub (Yuktesha/ruhi)
# ========================================================================
param(
    [string]$Message = "chore: update RuhI lexicon and semantic rules"
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Set-Location $PSScriptRoot

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  如喜 (RuhI) · GitHub 雲端同步小幫手" -ForegroundColor Cyan
Write-Host "  目標倉庫: https://github.com/Yuktesha/ruhi" -ForegroundColor DarkGray
Write-Host "========================================================" -ForegroundColor Cyan

$status = git status -s
if (-not $status) {
    Write-Host "目前沒有任何尚未同步的新變更，雲端已是最新狀態！" -ForegroundColor Green
    exit 0
}

Write-Host "偵測到以下變更準備同步至 GitHub：" -ForegroundColor Yellow
git status -s

git add -A
git commit -m $Message
git push origin main

Write-Host ""
Write-Host "✅ 已成功同步推送至 https://github.com/Yuktesha/ruhi ！" -ForegroundColor Green
