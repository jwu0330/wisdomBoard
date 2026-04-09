# WisdomBoard 執行 / 部署流程與疑難排解

> 版本：v0.3.0｜最後更新：2026-04-09

---

## 一、執行與部署流程

### 1.1 開發模式啟動

**完整步驟（含常見障礙排除）：**

#### 步驟 1：確認 port 1420 沒有被佔用

```bash
netstat -ano | findstr :1420
```

若無輸出，代表 port 乾淨，直接跳到步驟 3。

若有輸出，記下 PID（最右欄），繼續步驟 2。

#### 步驟 2：清掉佔用 port 的殘留進程

```bash
# 確認是什麼進程
powershell -Command "Get-Process -Id <PID>"

# 嘗試殺掉
taskkill /PID <PID> /F

# 同時確認 wisdomboard.exe 本身也沒在跑
powershell -Command "Get-Process -Name wisdomboard -ErrorAction SilentlyContinue"
powershell -Command "Stop-Process -Name wisdomboard -Force -ErrorAction SilentlyContinue"
```

> **注意：** 若出現「存取被拒 (os error 5)」，表示進程在 Session 0，無法從一般使用者殺掉。
> 需要手動到 **工作管理員 → 詳細資料 → 找到 wisdomboard.exe 或 node.exe → 結束工作**。

再次確認 port 已清空：

```bash
netstat -ano | findstr :1420
# 應無輸出，或只剩 TIME_WAIT 狀態（可忽略）
```

#### 步驟 3：啟動

```bash
cd wisdomboard
RUSTC_WRAPPER="" npx tauri dev
```

> `RUSTC_WRAPPER=""` 是為了繞過 `sccache` 未安裝的問題。
> 若你的環境沒有設定 `RUSTC_WRAPPER`，直接執行 `npx tauri dev` 即可。

#### 步驟 4：確認啟動成功

等待約 30-60 秒（首次編譯較久），看到以下輸出代表成功：

```
[WisdomBoard] 全域快捷鍵已註冊 (thread XXXXX)
[WisdomBoard] v0.3.0 已啟動
[WisdomBoard] 按快捷鍵開啟設定視窗
```

此時應用程式以系統匣圖示常駐，按 **Ctrl+Alt+S** 開啟設定視窗。

### 1.2 正式建置（本機）

```bash
npx tauri build
```

產出位置：`src-tauri/target/release/bundle/`（NSIS + MSI 安裝包）。

### 1.3 正式建置（CI/CD）

```bash
git push origin main         # 推送即觸發 GitHub Actions
gh run watch                 # 追蹤建置進度
gh run download -n WisdomBoard-Portable   # 下載產出
```

### 1.4 部署到使用者機器

1. 執行 NSIS 安裝包，或直接使用 Portable EXE
2. 首次執行時會自動註冊開機自啟動（可在設定視窗關閉）
3. 設定檔儲存於 `%APPDATA%/com.jwu0330.wisdomboard/config.json`

---

## 二、可能遇到的狀況

### 2.1 編譯階段

---

#### 狀況：`could not execute process 'sccache'`

**原因：** 環境變數 `RUSTC_WRAPPER=sccache` 存在，但 sccache 未安裝。

**解法（擇一）：**

| 方式 | 指令 |
|---|---|
| 安裝 sccache | `cargo install sccache` |
| 本次 session 繞過 | `export RUSTC_WRAPPER=""` (bash) 或 `$env:RUSTC_WRAPPER=""` (PowerShell) |
| 永久移除 | 系統設定 → 環境變數 → 刪除 `RUSTC_WRAPPER` |

---

#### 狀況：`unresolved import MapWindowPoints`

**原因：** 在 `windows` crate 0.52 中，`MapWindowPoints` 屬於 `Win32::Graphics::Gdi` 模組，不在 `Win32::UI::WindowsAndMessaging`。

**解法：**

```rust
// 錯誤
use windows::Win32::UI::WindowsAndMessaging::MapWindowPoints;

// 正確
use windows::Win32::Graphics::Gdi::MapWindowPoints;
```

---

#### 狀況：`expected isize, found *mut c_void`（HWND 型別不符）

**原因：** Tauri 的 `win.hwnd()` 回傳的 HWND 內部是 `*mut c_void`，而 `windows` crate 的 HWND 內部是 `isize`。

**解法：**

```rust
// 錯誤
HWND(hwnd.0)

// 正確
HWND(hwnd.0 as isize)
```

---

#### 狀況：`state does not live long enough`（E0597，大量出現）

**原因：** `app.state::<ManagedState>()` 產生的暫時值在 block 結尾時被 drop，但 `MutexGuard` 的 destructor 仍持有借用。

**解法：** 在 `if let Ok(guard) = state.lock() { ... }` 尾部加分號，讓 guard 在 state 之前 drop：

```rust
// 錯誤 — guard 的 destructor 在 state drop 之後才跑
if let Ok(mut guard) = state.lock() {
    guard.panels.remove(&label);
}

// 正確 — 分號讓 guard 的暫時值提前 drop
if let Ok(mut guard) = state.lock() {
    guard.panels.remove(&label);
};
```

此問題出現在 `capture.rs`、`panel.rs`、`hotkey.rs`、`persistence.rs`、`lib.rs` 多處，全部需要加分號。

---

#### 狀況：`reference to field of packed struct is unaligned`（DWM_THUMBNAIL_PROPERTIES）

**原因：** `DWM_THUMBNAIL_PROPERTIES` 是 1-byte aligned 的 packed struct，取 `&mut props.dwFlags` 會產生 UB。

**解法：**

```rust
// 錯誤
std::ptr::write(&mut props.dwFlags as *mut _ as *mut u32, flags);

// 正確（方法 A：直接賦值）
props.dwFlags = flags;

// 正確（方法 B：使用 addr_of_mut）
std::ptr::write_unaligned(std::ptr::addr_of_mut!(props.dwFlags) as *mut u32, flags);
```

---

### 2.2 開發服務啟動階段

---

#### 狀況：`Port 1420 is already in use`

**原因：** 上次 `tauri dev` 沒正常結束，Vite 的 node 進程殘留佔用 port。

**解法：**

```bash
# 步驟 1：找出佔用 port 的 PID
netstat -ano | findstr :1420

# 步驟 2：確認進程身分
powershell -Command "Get-Process -Id <PID>"

# 步驟 3：殺掉
taskkill /PID <PID> /F
```

**注意：** 如果進程在 Session 0（服務層級），普通使用者無法殺掉，需要：
- 開啟「以系統管理員身分執行」的終端機再執行 `taskkill`
- 或開啟工作管理員 → 詳細資料 → 找到 PID → 結束工作

---

#### 狀況：殭屍 node 進程會跟隨 port 設定變化

**原因：** 殘留的 Vite dev server 仍在 watch `vite.config.ts`，你改 port 它會自動 reload 到新 port。

**解法：** 不要試圖改 port 繞過，直接用管理員權限殺掉殭屍進程。

**預防方式：**
- 關閉 `tauri dev` 時使用 **Ctrl+C**，等完全結束後再關終端
- 不要直接關閉終端視窗（會留下殭屍進程）

---

### 2.3 執行期問題

---

#### 狀況：截圖 Overlay 無法顯示螢幕截圖（白畫面或黑畫面）

**原因：** Tauri v2 的 `convertFileSrc()` 走 `asset://` 協議，需要設定 scope 才能讀取本機檔案。

**解法：** 確認 `tauri.conf.json` 包含 asset protocol 設定：

```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'unsafe-inline'; img-src 'self' asset: https:; style-src 'self' 'unsafe-inline'",
      "assetProtocol": {
        "enable": true,
        "scope": ["$TEMP/**", "$APPDATA/**"]
      }
    }
  }
}
```

關鍵：
- `img-src` 必須包含 `asset:` 才能顯示本機圖片
- `scope` 必須包含 `$TEMP/**`，因為截圖存在 `%TEMP%/wisdomboard_screenshot.bmp`

---

#### 狀況：DWM Thumbnail 顯示有 8px 偏移

**原因：** `rcDestination.top` 被硬編碼為 8。

**解法：** 改為 0：

```rust
props.rcDestination = RECT {
    left: 0,
    top: 0,      // 修正：原本為 8
    right: panel_w,
    bottom: panel_h,
};
```

---

#### 狀況：框選擷取偵測到 Overlay 自身，而非目標視窗

**原因：** `capture_region` 呼叫 `WindowFromPoint` 時，Overlay 仍在最上層顯示，所以偵測到的是 Overlay 而不是底下的視窗。

**解法：** 在偵測目標視窗之前先隱藏 Overlay：

```rust
if let Some(overlay_win) = app.get_webview_window("overlay") {
    let _ = overlay_win.hide();
}
// 然後再 WindowFromPoint(...)
```

---

#### 狀況：高 DPI 螢幕下框選座標偏移

**原因：** scale factor 原本從 Overlay 視窗取得，但 Overlay 關閉後會 fallback 到 1.0。

**解法：** 改用 `primary_monitor()` 取得穩定的 scale factor：

```rust
// 錯誤 — overlay 可能已被關閉
let scale = app.get_webview_window("overlay")
    .and_then(|w| w.scale_factor().ok())
    .unwrap_or(1.0);

// 正確
let scale = app.primary_monitor()
    .ok()
    .flatten()
    .map(|m| m.scale_factor())
    .unwrap_or(1.0);
```

---

#### 狀況：設定視窗的「關閉 / 模式切換 / 縮放」操作到錯誤的面板

**原因：** 前端用陣列 index（`doClose(0)`, `doMode(1, 'view')`）對應面板，但在 async 操作中，其他面板被關閉後 index 會位移。

**解法：** 改用面板的 `label`（唯一識別）：

```javascript
// 錯誤 — async 環境下 index 可能已過期
onclick="doClose(${i})"

// 正確 — label 是穩定的唯一識別
onclick="doClose(${JSON.stringify(p.label)})"
```

---

### 2.4 CI/CD 相關

---

#### 狀況：GitHub Actions 下載 job logs 回傳 403

**原因：** 使用的 GitHub token 沒有 repo admin 權限。

**解法：** 確認 workflow token 具有 `actions:read` scope，或改用具有足夠權限的 PAT。

---

## 三、持久化資料位置

| 資料 | 路徑 |
|---|---|
| 應用設定 | `%APPDATA%/com.jwu0330.wisdomboard/config.json` |
| 截圖暫存 | `%TEMP%/wisdomboard_screenshot.bmp` |
| 自動啟動登錄 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

---

## 四、環境設定參考

### `~/.cargo/config.toml`（全域 Rust 設定）

```toml
[build]
jobs = 6

[target.x86_64-pc-windows-msvc]
linker = "rust-lld"
```

### 需要注意的環境變數

| 變數 | 說明 | 建議 |
|---|---|---|
| `RUSTC_WRAPPER` | 如果設為 `sccache` 但未安裝會導致編譯失敗 | 安裝 sccache 或移除此變數 |
