# WisdomBoard 開發說明文件

> **版本：** v0.3.0
> **最後更新：** 2026-04-08
> **維護者：** jwu0330

---

## 目錄

1. [專案概述](#1-專案概述)
2. [技術架構](#2-技術架構)
3. [目錄結構](#3-目錄結構)
4. [程式碼邏輯說明](#4-程式碼邏輯說明)
5. [CI/CD 流程](#5-cicd-流程)
6. [開發與建置](#6-開發與建置)
7. [佈告欄功能規劃](#7-佈告欄功能規劃)
8. [已知問題與修復紀錄](#8-已知問題與修復紀錄)
9. [注意事項](#9-注意事項)

---

## 1. 專案概述

**WisdomBoard（靈魂桌面．智匯看板）** 是一款 Windows 桌面應用程式，核心功能為：

- **面板釘選**：將網頁 URL 或螢幕擷取區域以 always-on-top 面板形式釘選於桌面
- **框選擷取**：全域快捷鍵觸發螢幕截圖式 Overlay，拖拉框選產生面板
- **系統匣控制**：透過右下角系統匣圖示進行操作
- **開機自啟動**：Windows 開機時自動啟動

### 核心價值

讓生產力工具像桌布一樣自然存在於桌面，無需切換視窗即可檢視和操作。

### 桌面嵌入（規劃中，尚未實作）

SPECIFICATION.md 描述的 WorkerW 桌面嵌入功能（F02）目前尚未實作。當前所有面板使用 `always_on_top` 方式釘選。

---

## 2. 技術架構

```
┌─────────────────────────────────────────────────┐
│                   使用者桌面                       │
│  ┌──────────────┐  ┌──────────────┐              │
│  │  系統匣圖示    │  │  面板視窗      │ ← always on top│
│  └──────────────┘  └──────────────┘              │
│  ┌──────────────────────────────────┐            │
│  │  WisdomBoard 主視窗（隱藏）        │            │
│  └──────────────────────────────────┘            │
└─────────────────────────────────────────────────┘
```

### 技術棧

| 層級 | 技術 | 用途 |
|------|------|------|
| 前端 | TypeScript + Vite + HTML/CSS | 設定介面與面板 UI |
| 後端 | Rust + Tauri v2 | 系統整合、Windows API 操作 |
| 系統 API | `windows` crate (0.52) | 螢幕截圖、全域快捷鍵 |
| 外掛 | `tauri-plugin-autostart` | 開機自啟動 |
| CI/CD | GitHub Actions | 自動建置 Windows EXE |

---

## 3. 目錄結構

```
wisdomboard/
├── .github/
│   └── workflows/
│       └── build.yml              # CI/CD：自動建置 Windows EXE
├── src/                           # 前端程式碼
│   ├── main.ts                    # 前端進入點（主視窗隱藏，無邏輯）
│   ├── index.html                 # 主視窗 HTML（隱藏，無內容）
│   ├── settings.html              # 設定視窗（面板管理 UI）
│   ├── panel.html                 # 面板視窗（含模式切換工具列）
│   ├── overlay.html               # 框選 Overlay（截圖背景 + 拖拉框選）
│   ├── webpanel.html              # URL 面板（iframe 載入，備用方案）
│   └── styles.css                 # 全域樣式（白底極簡風格）
├── src-tauri/                     # Rust 後端
│   ├── src/
│   │   ├── main.rs                # 程式進入點（呼叫 lib::run）
│   │   ├── lib.rs                 # 入口：插件/匣/命令註冊
│   │   ├── state.rs               # 共用狀態與資料模型
│   │   ├── panel.rs               # 面板 CRUD 命令
│   │   ├── capture.rs             # 截圖、Overlay、DWM thumbnail
│   │   ├── hotkey.rs              # 快捷鍵監聽與自訂
│   │   ├── persistence.rs         # JSON 設定檔讀寫
│   │   └── input.rs               # 輸入轉發（PostMessage）
│   ├── Cargo.toml                 # Rust 依賴管理
│   ├── tauri.conf.json            # Tauri 應用設定
│   ├── capabilities/              # 權限定義
│   │   ├── default.json           # 主視窗權限
│   │   └── desktop.json           # 桌面功能權限（autostart）
│   └── icons/                     # 應用程式圖示
├── index.html                     # Vite 入口點（主視窗，隱藏）
├── package.json                   # Node.js 依賴
├── tsconfig.json                  # TypeScript 設定
├── vite.config.ts                 # Vite 打包設定
├── SPECIFICATION.md               # 專案規格書
└── DEVELOPMENT.md                 # 本文件
```

---

## 4. 程式碼邏輯說明

### 4.1 全域快捷鍵 (`lib.rs`)

使用 `RegisterHotKey` 註冊 Ctrl+Alt+S，在獨立執行緒中以 `GetMessageW` 迴圈監聽：

```rust
RegisterHotKey(HWND(0), HOTKEY_SNIP, MOD_ALT | MOD_CONTROL, 0x53); // 'S'
```

觸發後開啟設定視窗（`open_settings`）。

### 4.2 系統匣 (System Tray)

在 `run()` 函式的 `.setup()` 中建立，提供兩個選項：

- **設定 (Ctrl+Alt+S)**：開啟設定視窗
- **離開**：呼叫 `app.exit(0)` 結束程式

### 4.3 開機自啟動

使用 `tauri-plugin-autostart`，在 `.setup()` 中自動啟用：
```rust
let autostart_manager = app.autolaunch();
if !autostart_manager.is_enabled().unwrap_or(false) {
    autostart_manager.enable()?;
}
```

### 4.4 面板系統

面板分兩種：
- **URL 面板**：`create_url_panel` 使用 `WebviewUrl::External` 直接載入外部網頁
- **擷取面板**：`capture_region` 在框選位置建立空白 panel.html 面板

### 4.5 四種操作模式

| 模式 | 行為 |
|------|------|
| **觀看 (view)** | 面板可拖拉移動，不可調整大小 |
| **操作 (interact)** | 可與面板內容互動 |
| **調整 (resize)** | 可拖拉調整面板大小 |
| **鎖定 (locked)** | 面板固定不動，不可關閉/移動 |

settings.html 的 pill 按鈕直接設定模式；panel.html 的工具列支援「再按鎖定 = 解鎖」的 toggle UX。

### 4.6 螢幕截圖與框選 Overlay

1. `open_capture_overlay` 先呼叫 `capture_screen_to_file()` 截取全螢幕 BMP
2. 建立全螢幕不透明 Overlay 視窗，以截圖作為背景（模擬透明效果）
3. 使用者拖拉框選區域，前端傳送 CSS 像素座標
4. `capture_region` 將座標乘以 DPI 縮放因子轉為物理像素，在該位置建立面板

---

## 5. CI/CD 流程

### 5.1 建置流程

**本專案不在本機編譯**，透過 GitHub Actions 在雲端建置：

```
git push origin main
    ↓
GitHub Actions 觸發 (build.yml)
    ↓
windows-latest Runner
    ├── Checkout 程式碼
    ├── Setup Node.js 24
    ├── Install Rust (MSVC target)
    ├── Setup MSVC Developer Prompt
    ├── Rust Cache（加速二次建置）
    ├── npm install
    └── npm run tauri build
    ↓
產出物：WisdomBoard-Portable (wisdomboard.exe)
    ↓
可從 GitHub Actions Artifacts 下載
```

### 5.2 觸發方式

| 方式 | 說明 |
|------|------|
| `git push origin main` | 推送到 main 分支自動觸發 |
| GitHub 網頁 → Actions → Run workflow | 手動觸發（workflow_dispatch）|

### 5.3 快捷操作指令

```bash
# 一鍵推送並觸發 CI/CD
git add -A && git commit -m "描述" && git push origin main

# 查看 CI/CD 執行狀態
gh run list --limit 5

# 查看最新一次執行的詳細日誌
gh run view --log

# 下載最新建置產物
gh run download -n WisdomBoard-Portable

# 手動觸發建置（不需推送程式碼）
gh workflow run build.yml
```

### 5.4 加速建置技巧

- **Rust Cache**：已設定 `Swatinem/rust-cache`，二次建置可節省約 5-10 分鐘
- **Node Cache**：可加入 npm cache 進一步加速
- **快速模式**：使用 `--no-bundle` 跳過安裝包建立，只產生 EXE

---

## 6. 開發與建置

### 6.1 環境需求

開發本機（僅需編輯程式碼，不需本機編譯）：
- Git
- Node.js 24+
- GitHub CLI (`gh`)

完整本機編譯（可選）：
- 以上全部
- Rust toolchain (stable, MSVC target)
- Visual Studio 2022 + Windows 11 SDK
- MSVC Build Tools

### 6.2 日常開發流程

```bash
# 1. 修改程式碼

# 2. 提交變更
git add <files>
git commit -m "feat: 新功能描述"

# 3. 推送觸發 CI/CD
git push origin main

# 4. 等待建置完成（約 5-15 分鐘）
gh run watch

# 5. 下載建置產物
gh run download -n WisdomBoard-Portable

# 6. 測試執行
./WisdomBoard-Portable/wisdomboard.exe
```

---

## 7. 佈告欄功能規劃

### 7.1 功能概述

類似 PowerToys Crop and Lock 的進階版，可即時裁切任意視窗的特定區域，產生可互動的獨立面板。

### 7.2 操作流程

```
1. 使用者按下快捷鍵（預設 Ctrl+Alt+S）
2. 畫面進入「選取模式」— 全螢幕截圖式 Overlay
3. 拖拉框選要擷取的區域
4. 產生一個「釘選面板」小視窗
5. （未來）面板即時顯示目標區域的內容（DWM Thumbnail）
6. （未來）可在面板中直接操作目標視窗（輸入轉發）
```

### 7.3 四種操作模式

| 模式 | 行為 | 觸發方式 |
|------|------|----------|
| **觀看模式**（預設）| 面板可拖拉移動 | 預設狀態 |
| **操作模式** | 可與面板內容互動 | 設定視窗 pill / 面板工具列 |
| **調整模式** | 可拖拉調整大小 | 設定視窗 pill / 面板工具列 |
| **鎖定模式** | 面板固定不動，不可關閉 | 設定視窗 pill / 面板工具列 |

### 7.4 技術實作

| 模組 | 狀態 | 說明 |
|------|------|------|
| 全域快捷鍵 | ✅ 已實作 | `RegisterHotKey` Ctrl+Alt+S |
| 截圖式 Overlay | ✅ 已實作 | GDI 截圖 + 全螢幕 Overlay + 拖拉框選 |
| 面板管理 UI | ✅ 已實作 | settings.html 面板列表 + 模式/縮放控制 |
| URL 面板 | ✅ 已實作 | `WebviewUrl::External` 直接載入 |
| DWM Thumbnail | ✅ 已實作 | `DwmRegisterThumbnail` 即時縮圖，自動偵測目標視窗 |
| 輸入轉發 | ✅ 已實作 | 操作模式下 `PostMessageW` 座標映射轉發 |
| 面板持久化 | ✅ 已實作 | JSON 設定檔自動儲存/恢復面板配置 |
| 自訂快捷鍵 | ✅ 已實作 | 設定視窗 UI 設定 + `PostThreadMessage` 動態註冊 |

---

## 8. 已知問題與修復紀錄

### v0.2.0-dev 修復項目

| 問題 | 修復方式 |
|------|----------|
| `window.hwnd().unwrap()` 可能 panic | 改為 `match` + 錯誤日誌 |
| `get_webview_window("main").unwrap()` | 改為 `if let Some()` |
| `greet` 指令殘留未使用 | 移除 greet 函式與 invoke_handler |
| autostart 插件載入但未啟用 | 加入 `autostart_manager.enable()` |
| 版本號不一致 (0.1.0 / 0.2.0) | 統一為 0.2.0 |
| base64 / urlencoding 依賴未使用 | 移除無用 crate |
| URL 面板先載入 webpanel.html 再 navigate() | 改用 `WebviewUrl::External` 直接載入 |
| 面板找不到時靜默成功 | 改為回傳錯誤 |
| panel.html toggle 邏輯與 settings 不一致 | setMode 改為直接設定，toggle 只在 click handler |
| set_mode 廣播到所有視窗 | 改為只對 panel-* 視窗發送 |
| 截圖 BMP 上下顛倒 | biHeight 改為負值（top-down BMP） |
| overlay 框選座標未考慮 DPI | capture_region 乘以 scale factor |
| styles.css 深色主題與設計不符 | 改為白底黑字極簡風格 |
| Cargo.toml description/authors 為模板值 | 更新為專案實際資訊 |
| capabilities 只授權 main 視窗 | 擴展到 settings/overlay/panel-* |
| 根 index.html 為 Tauri 模板 | 替換為最小化頁面 |
| README.md 為模板內容 | 重寫為專案說明 |

---

## 9. 注意事項

### 安全性
- `tauri.conf.json` 中 `csp: null`（CSP 停用），這是為了允許載入外部網站
- 未來應限縮 CSP 至僅允許特定來源

### 建置相關
- 本機不需安裝 Windows SDK，所有建置透過 GitHub Actions 完成
- Rust Cache 大幅加速二次建置，但首次建置需約 15 分鐘
- 路徑中的中文字元（「新增資料夾」）可能在某些工具中造成問題

### 版本控管
- 所有變更必須透過 Git 提交並推送到 GitHub
- 每次功能完成後進行一次 commit
- commit message 格式：`feat:` / `fix:` / `ci:` / `docs:` 開頭

---

## 10. 踩坑紀錄與經驗教訓

### 10.1 全螢幕透明 Overlay 無法實現

**問題：** `transparent: true` + `fullscreen: true` → Tauri 在 Windows 上直接閃退。

**替代方案（已實作）：** 使用 GDI 截取螢幕截圖，顯示在不透明全螢幕視窗上，模擬透明效果。

### 10.2 windows crate API 回傳型態不一致

**問題：** `windows` crate 0.52 中，不同 Win32 函式的回傳型態不統一：

| 函式 | 回傳型態 | 錯誤檢查方式 |
|------|----------|-------------|
| `FindWindowW` | `HWND` | `== HWND(0)` 表示找不到 |
| `SetParent` | `HWND` | `== HWND(0)` 表示失敗 |
| `EnumWindows` | `Result<()>` | 標準 `Result` 錯誤處理 |

**教訓：** 不能假設所有 Win32 函式都回傳 `Result`，必須先確認實際簽名。

### 10.3 本機無法編譯 Rust（缺少 Windows SDK）

**解決方式：** 所有建置透過 GitHub Actions 完成（`windows-latest` runner 已內建完整 SDK）。

如需本機編譯：透過 Visual Studio Installer → 修改 → 個別元件 → 勾選「Windows 11 SDK」。

### 10.4 SetParent 後視窗消失（Windows 11）

**根本原因：** Windows 11 的 `SetParent` 會將子視窗的尺寸重置或隱藏。

**解決方式：** 在 `SetParent` 之後立即呼叫 `MoveWindow` + `ShowWindow`。
