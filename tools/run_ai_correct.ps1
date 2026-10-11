﻿param(
    [string]$Text = '',
    [string]$Backend = 'auto',
    [switch]$Simulate,
    [switch]$Json,
    [switch]$ApplyOnly
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ToolPy = Join-Path $ScriptDir 'ruhi_ai_correct.py'

$ArgsList = @()
if ($Backend) { $ArgsList += @('--backend', $Backend) }
if ($Json) { $ArgsList += '--json' }
if ($ApplyOnly) { $ArgsList += '--apply-only' }

if ($Simulate) {
    if (-not $Text) { $Text = '專案服務器代碼以經完成，粒刻測試！' }
    $ArgsList += @('--simulate', $Text)
} elseif ($Text) {
    $ArgsList += $Text
}

& python $ToolPy @ArgsList
