# -*- coding: utf-8 -*-
<#
.SYNOPSIS
    如喜輸入法 (RuhI) · 語意調校工作室快捷進入點
#>
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$script = Join-Path $PSScriptRoot 'scripts\語意除錯調校.ps1'
if (-not (Test-Path $script)) {
    $script = Join-Path $PSScriptRoot 'scripts\語意除錯調教.ps1'
}
& powershell.exe -ExecutionPolicy Bypass -File $script @args
