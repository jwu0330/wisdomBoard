# WisdomBoard 開發說明文件

> **版本：** v0.2.0-dev  
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

- **桌面嵌入**：將網頁內容嵌入桌面壁紙與圖示之間的 WorkerW 圖層
- **佈告欄**（開發中）：即時裁切任意視窗的特定區域，產生可互動的釘選面板
- **系統匣控制**：透過右下角系統匣圖示進行操作
- **開機自啟動**：Windows 開機時自動啟動

### 核心價值

讓生產力工具像桌布一樣自然存在於桌面，無需切換視窗即可檢視和操作。

---

## 2. 技術架構

```
┌─────────────────────────────────────────────────┐
│                   使用者桌面                       │
│  ┌──────────────┐  ┌──────────────┐              │
│  │  桌面圖示層    │  │  佈告欄面板    │ ← 最前端顯示  │
│  └──────────────┘  └──────────────┘              │
│  ┌──────────────────────────────────┐            │
│  │  WisdomBoard 主視窗 (WorkerW 層)  │ ← 桌布之上   │
│  │  └─ WebView2 (GitHub Projects)   │            │
│  └──────────────────────────────────┘            │
│  ┌──────────────────────────────────┐            │
│  │  桌面壁紙                         │            │
│  └──────────────────────────────────┘            │
└─────────────────────────────────────────────────┘
```

### 技術棧

| 層級 | 技術 | 用途 |
|------|------|------|
| 前端 | TypeScript + Vite + HTML/CSS | WebView 介面與佈告欄 UI |
| 後端 | Rust + Tauri v2 | 桌面整合、Windows API 操作 |
| 系統 API | `windows` crate (0.52) | WorkerW 掛載、DWM Thumbnail、全域快捷鍵 |
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
│   ├── main.ts                    # 前端進入點
│   ├── index.html                 # HTML 主頁（含 iframe）
│   ├── styles.css                 # 全域樣式
│   └── assets/                    # 靜態資源
├── src-tauri/                     # Rust 後端
│   ├── src/
│   │   ├── main.rs                # 程式進入點（呼叫 lib::run）
│   │   └── lib.rs                 # 核心邏輯：桌面掛載 + 系統匣
│   ├── Cargo.toml                 # Rust 依賴管理
│   ├── tauri.conf.json            # Tauri 應用設定
│   ├── capabilities/              # 權限定義
│   │   ├── default.json           # 主視窗權限
│   │   └── desktop.json           # 桌面功能權限（autostart）
│   └── icons/                     # 應用程式圖示
├── package.json                   # Node.js 依賴
├── tsconfig.json                  # TypeScript 設定
├── vite.config.ts                 # Vite 打包設定
├── SPECIFICATION.md               # 專案規格書
└── DEVELOPMENT.md                 # 本文件
```

---

## 4. 程式碼邏輯說明

### 4.1 桌面掛載流程 (`src-tauri/src/lib.rs`)

這是整個應用的核心「黑魔法」邏輯：

```
Step 1: FindWindowW("Progman")
        → 找到桌面的 Progman 視窗

Step 2: SendMessage(Progman, 0x052C)
        → 強迫 Windows 生成新的 WorkerW 圖層

Step 3: EnumWindows → enum_windows_proc
        → 遍歷所有視窗，找到包含 SHELLDLL_DefView 的 WorkerW
        → 取得其「下一個」WorkerW 作為目標掛載點

Step 4: SetParent(Tauri_HWND, WorkerW_HWND)
        → 將 Tauri 視窗設為 WorkerW 的子視窗
        → 效果：視窗顯示在桌面圖示之下、壁紙之上
```

**關鍵全域變數：**
- `WORKERW_HWND: AtomicIsize` — 儲存找到的目標 WorkerW 句柄

**錯誤處理策略（v0.2.0 改善）：**
- 所有 `unwrap()` 已替換為 `match` + `eprintln!` 錯誤日誌
- `pin_to_desktop()` 回傳 `bool` 表示成功/失敗
- 找不到 WorkerW 時會回退掛載到 Progman

### 4.2 系統匣 (System Tray)

在 `run()` 函式的 `.setup()` 中建立：

- **重整畫面**：呼叫 `window.eval("window.location.reload()")` 重新載入 WebView
- **離開**：呼叫 `app.exit(0)` 結束程式

### 4.3 開機自啟動

使用 `tauri-plugin-autostart`，在 `.setup()` 中自動啟用：
```rust
let autostart_manager = app.autolaunch();
if !autostart_manager.is_enabled().unwrap_or(false) {
    autostart_manager.enable()?;
}
```

### 4.4 前端 (`src/index.html`)

目前為單純的全螢幕 iframe，嵌入 GitHub Projects 看板：
```html
<iframe src="https://github.com/users/jwu0330/projects/2/views/3"></iframe>
```

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
2. 畫面進入「選取模式」— 全螢幕透明 Overlay
3. 點選目標視窗
4. 拖拉框選要截錄的區域
5. 產生一個「釘選面板」小視窗
6. 面板即時顯示目標區域的內容（DWM Thumbnail）
7. 可在面板中直接操作目標視窗（輸入轉發）
```

### 7.3 三種操作模式

| 模式 | 行為 | 觸發方式 |
|------|------|----------|
| **鎖定模式**（預設）| 面板固定不動，不可關閉/移動 | 預設狀態 |
| **編輯模式** | 可移動、縮放、關閉面板 | 系統匣/快捷鍵 |
| **控制輸入模式** | 點擊/鍵盤轉發到目標視窗 | 系統匣/快捷鍵 |

### 7.4 技術實作

| 模組 | Windows API | 說明 |
|------|-------------|------|
| 即時畫面 | `DwmRegisterThumbnail` | DWM 即時縮圖，非截圖 |
| 輸入轉發 | `SendInput` / `PostMessage` | 座標映射後模擬輸入 |
| 全域快捷鍵 | `RegisterHotKey` | 可自訂的全域快捷鍵 |
| 面板持久化 | JSON 設定檔 | 儲存/恢復面板配置 |

### 7.5 開發階段

| 階段 | 內容 | 狀態 |
|------|------|------|
| Step 0 | 穩定性修復 | ✅ 完成 |
| Step 1 | 全域快捷鍵 + 選取 Overlay | 待開發 |
| Step 2 | DWM Thumbnail 即時面板 | 待開發 |
| Step 3 | 三模式切換 + 工具列 UI | 待開發 |
| Step 4 | 設定持久化 + 開機恢復 | 待開發 |
| Step 5 | 自訂快捷鍵 UI | 待開發 |

---

## 8. 已知問題與修復紀錄

### v0.2.0-dev 修復項目

| 問題 | 修復方式 | 檔案 |
|------|----------|------|
| `window.hwnd().unwrap()` 可能 panic | 改為 `match` + 錯誤日誌 | `lib.rs:31` |
| `get_webview_window("main").unwrap()` | 改為 `if let Some()` | `lib.rs:88` |
| `greet` 指令殘留未使用 | 移除 greet 函式與 invoke_handler | `lib.rs` |
| autostart 插件載入但未啟用 | 加入 `autostart_manager.enable()` | `lib.rs:84` |
| `EnumWindows` 前未重置全域變數 | 呼叫前重置 `WORKERW_HWND` 為 0 | `lib.rs:54` |
| `SetParent` 無錯誤處理 | 比較 `HWND(0)` 判斷失敗 | `lib.rs:72` |
| `SetParent` 誤用 `match Ok/Err` | 修正為比較回傳值（見踩坑紀錄） | `lib.rs:70-75` |

---

## 9. 注意事項

### 安全性
- `tauri.conf.json` 中 `csp: null`（CSP 停用），這是為了允許 iframe 載入外部網站
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

### 10.1 windows crate API 回傳型態不一致

**問題：** `windows` crate 0.52 中，不同 Win32 函式的回傳型態不統一：

| 函式 | 回傳型態 | 錯誤檢查方式 |
|------|----------|-------------|
| `FindWindowW` | `HWND` | `== HWND(0)` 表示找不到 |
| `FindWindowExW` | `HWND` | `== HWND(0)` 表示找不到 |
| `SendMessageTimeoutW` | `LRESULT` | `== LRESULT(0)` 表示逾時/失敗 |
| `SetParent` | `HWND` | `== HWND(0)` 表示失敗 |
| `EnumWindows` | `Result<()>` | 標準 `Result` 錯誤處理 |

**教訓：** 不能假設所有 Win32 函式都回傳 `Result`。在使用 `match Ok/Err` 之前，必須先到 `~/.cargo/registry/src/` 中確認該函式的實際簽名：
```bash
grep "pub unsafe fn 函式名稱" ~/.cargo/registry/src/index.crates.io-*/windows-0.52.0/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs
```

### 10.2 本機無法編譯 Rust（缺少 Windows SDK）

**問題：** 本機有 VS 2022 Community 和 Rust toolchain，但未安裝 Windows SDK，導致 `rust-lld` 找不到 `kernel32.lib`。

**解決方式：** 所有建置透過 GitHub Actions 完成（`windows-latest` runner 已內建完整 SDK）。

**如需本機編譯：** 透過 Visual Studio Installer → 修改 → 個別元件 → 勾選「Windows 11 SDK」。

### 10.3 修改程式碼後務必驗證 API 簽名

**流程：** 修改 Rust 程式碼 → 確認所有使用的 API 簽名 → 提交 → 推送 → 等 CI/CD 結果

**驗證方法：**
```bash
# 查看某個 crate 中函式的實際簽名
grep "pub unsafe fn 函式名" ~/.cargo/registry/src/index.crates.io-*/crate-name-version/src/**/*.rs

# 查看 trait 定義
grep -A5 "pub trait TraitName" ~/.cargo/registry/src/index.crates.io-*/crate-name-version/src/**/*.rs
```
