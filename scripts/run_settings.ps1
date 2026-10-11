# RuhI 偏好設定中心啟動腳本
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$distScript = "C:\ATGprjs\ruhi\dist\settings_window.py"
$scriptsScript = "C:\ATGprjs\ruhi\scripts\settings_window.py"

$target = if (Test-Path $distScript) { $distScript } else { $scriptsScript }

$py = if (Test-Path "C:\Python\pythonw.exe") { "C:\Python\pythonw.exe" } else { "pythonw.exe" }

Start-Process -FilePath $py -ArgumentList "`"$target`"" -WorkingDirectory (Split-Path -Parent $target)
