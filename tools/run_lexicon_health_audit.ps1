﻿# PowerShell 腳本：如喜 (RuhI) 詞庫健康檢查啟動器
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Set-Location -Path (Split-Path -Parent )

Write-Host '🚀 啟動如喜詞庫 Phase 1 全面靜態健康檢查器...' -ForegroundColor Cyan
python tools/audit_lexicon_health.py --export-report docs/LEXICON_HEALTH_REPORT_PHASE1.md
