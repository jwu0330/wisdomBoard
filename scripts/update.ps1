# WisdomBoard 一鍵更新腳本
# 用法：右鍵 → 以 PowerShell 執行，或在終端機輸入：
#   powershell -ExecutionPolicy Bypass -File update.ps1

$ErrorActionPreference = "Stop"
$repo = "jwu0330/wisdomBoard"
$installDir = "$env:LOCALAPPDATA\WisdomBoard"
$exeName = "wisdomboard.exe"

Write-Host "=== WisdomBoard 更新工具 ===" -ForegroundColor Cyan
Write-Host ""

# 1. 關閉舊版
Write-Host "[1/4] 關閉舊版程式..." -ForegroundColor Yellow
taskkill /f /im $exeName 2>$null
Start-Sleep -Seconds 1

# 2. 建立安裝目錄
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

# 3. 下載最新版
Write-Host "[2/4] 下載最新版本..." -ForegroundColor Yellow
$apiUrl = "https://api.github.com/repos/$repo/releases/tags/latest"
try {
    $release = Invoke-RestMethod -Uri $apiUrl
    $asset = $release.assets | Where-Object { $_.name -like "*Portable.exe" } | Select-Object -First 1
    if (-not $asset) {
        Write-Host "找不到 Portable EXE，嘗試下載第一個 .exe..." -ForegroundColor Yellow
        $asset = $release.assets | Where-Object { $_.name -like "*.exe" } | Select-Object -First 1
    }
    if (-not $asset) {
        throw "Release 中找不到可下載的 EXE 檔案"
    }
    $downloadUrl = $asset.browser_download_url
    $fileName = $asset.name
    Write-Host "  下載: $fileName" -ForegroundColor Gray
    Invoke-WebRequest -Uri $downloadUrl -OutFile "$installDir\$exeName"
    Write-Host "  完成！" -ForegroundColor Green
} catch {
    Write-Host "下載失敗: $_" -ForegroundColor Red
    Write-Host "請確認 GitHub Release 存在: https://github.com/$repo/releases" -ForegroundColor Yellow
    Read-Host "按 Enter 結束"
    exit 1
}

# 4. 清理暫存
Write-Host "[3/4] 清理暫存檔案..." -ForegroundColor Yellow
Remove-Item "$env:TEMP\wisdomboard_*.bmp" -Force -ErrorAction SilentlyContinue
$configDir = "$env:APPDATA\com.jwu0330.wisdomboard"
if (Test-Path "$configDir\config.json") {
    Write-Host "  保留設定檔: $configDir\config.json" -ForegroundColor Gray
}

# 5. 啟動
Write-Host "[4/4] 啟動 WisdomBoard..." -ForegroundColor Yellow
Start-Process "$installDir\$exeName"

Write-Host ""
Write-Host "=== 更新完成！===" -ForegroundColor Green
Write-Host "安裝位置: $installDir\$exeName" -ForegroundColor Gray
Write-Host "按 Ctrl+Alt+S 開啟設定" -ForegroundColor Gray
Write-Host ""
Read-Host "按 Enter 關閉此視窗"
