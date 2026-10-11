﻿# -*- coding: utf-8 -*-
# RuhI Universal Smart IME - AI 語意分析師背景監控守護啟動腳本
$ErrorActionPreference = 'Stop'

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "  RuhI 如喜通用智慧型輸入法 - AI 語意分析師守護啟動" -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$pyTool = Join-Path $scriptDir "ruhi_semantic_analyst.py"

if (-not (Test-Path $pyTool)) {
    Write-Host "[錯誤] 找不到 AI 語意分析師核心工具 CLI: $pyTool" -ForegroundColor Red
    exit 1
}

Write-Host "正在啟動 AI 語意分析師常駐監控守護 (watch 模式)..." -ForegroundColor Yellow
python $pyTool watch
