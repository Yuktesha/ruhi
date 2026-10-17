# RuhI 本地純綠 AI 推論服務停止腳本
$existing = Get-Process -Name 'llama-server' -ErrorAction SilentlyContinue
if ($existing) {
    $existing | Stop-Process -Force
    Write-Host '本地純綠 AI 服務已完全停止。' -ForegroundColor Green
} else {
    Write-Host '未偵測到正在運行的本地 AI 服務。' -ForegroundColor Yellow
}
