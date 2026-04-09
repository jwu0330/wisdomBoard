# WisdomBoard 完全清除腳本
# 用法：powershell -ExecutionPolicy Bypass -File uninstall.ps1

$ErrorActionPreference = "SilentlyContinue"

Write-Host "=== WisdomBoard 完全清除 ===" -ForegroundColor Cyan
Write-Host ""

# 1. 關閉程式
Write-Host "[1/4] 關閉程式..." -ForegroundColor Yellow
taskkill /f /im wisdomboard.exe 2>$null
Start-Sleep -Seconds 1

# 2. 刪除安裝目錄
Write-Host "[2/4] 刪除程式檔案..." -ForegroundColor Yellow
$paths = @(
    "$env:LOCALAPPDATA\WisdomBoard",
    "$env:LOCALAPPDATA\Programs\wisdomboard"
)
foreach ($p in $paths) {
    if (Test-Path $p) {
        Remove-Item $p -Recurse -Force
        Write-Host "  已刪除: $p" -ForegroundColor Gray
    }
}

# 3. 刪除設定和暫存
Write-Host "[3/4] 刪除設定與暫存..." -ForegroundColor Yellow
$configDir = "$env:APPDATA\com.jwu0330.wisdomboard"
if (Test-Path $configDir) {
    Remove-Item $configDir -Recurse -Force
    Write-Host "  已刪除: $configDir" -ForegroundColor Gray
}
$deleted = (Get-ChildItem "$env:TEMP\wisdomboard_*.bmp" -ErrorAction SilentlyContinue).Count
Remove-Item "$env:TEMP\wisdomboard_*.bmp" -Force -ErrorAction SilentlyContinue
if ($deleted -gt 0) {
    Write-Host "  已刪除 $deleted 個暫存截圖" -ForegroundColor Gray
}

# 4. 移除自動啟動
Write-Host "[4/4] 移除開機自啟動..." -ForegroundColor Yellow
$startupPaths = @(
    "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\wisdomboard*",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
)
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\wisdomboard*" -Force -ErrorAction SilentlyContinue
$regKey = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -ErrorAction SilentlyContinue
if ($regKey.PSObject.Properties.Name -contains "wisdomboard") {
    Remove-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "wisdomboard" -ErrorAction SilentlyContinue
    Write-Host "  已移除登錄檔自啟動項目" -ForegroundColor Gray
}

Write-Host ""
Write-Host "=== 清除完成！===" -ForegroundColor Green
Write-Host ""
Read-Host "按 Enter 關閉此視窗"
