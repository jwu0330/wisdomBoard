use crate::state::{ManagedState, PanelConfig, PanelType};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetForegroundWindow, GetWindowTextW,
    SM_CXSCREEN, SM_CYSCREEN,
};

/// 嘗試從前景視窗取得瀏覽器 URL（透過 UI Automation）
fn detect_browser_url() -> Option<String> {
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, CoCreateInstance, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::*;

        let fg = GetForegroundWindow();
        if fg.0 == 0 { return None; }

        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(fg, &mut title_buf);
        if len == 0 { return None; }
        let title = String::from_utf16_lossy(&title_buf[..len as usize]);
        println!("[WisdomBoard] 前景視窗標題: {}", title);

        // CoInitializeEx 回傳：
        //   Ok(())  = S_OK 或 S_FALSE（成功或已初始化），需配對呼叫 CoUninitialize
        //   Err     = 模式衝突（RPC_E_CHANGED_MODE），不呼叫 CoUninitialize
        // COM 初始化計數是每執行緒的引用計數，S_FALSE 也需要配對的 CoUninitialize。
        let com_initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        // 若 COM 初始化失敗（模式衝突），仍嘗試呼叫 UIAutomation（可能已由主執行緒初始化）
        let result = (|| -> Option<String> {
            let uia: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL) {
                Ok(u) => u,
                Err(e) => { println!("[WisdomBoard] UIAutomation 初始化失敗: {e}"); return None; }
            };

            let root = match uia.ElementFromHandle(fg) {
                Ok(el) => el,
                Err(e) => { println!("[WisdomBoard] ElementFromHandle 失敗: {e}"); return None; }
            };

            let true_cond = match uia.CreateTrueCondition() {
                Ok(c) => c,
                Err(_) => return None,
            };

            let elements = match root.FindAll(TreeScope_Descendants, &true_cond) {
                Ok(els) => els,
                Err(_) => return None,
            };

            let count = elements.Length().unwrap_or(0);
            let mut found_url: Option<String> = None;

            for i in 0..count {
                let el = match elements.GetElement(i) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let ct = el.CurrentControlType().unwrap_or_default();
                if ct != UIA_EditControlTypeId { continue; }

                let pattern: IUIAutomationValuePattern = match el.GetCurrentPatternAs(UIA_ValuePatternId) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let val = match pattern.CurrentValue() {
                    Ok(v) => v.to_string(),
                    Err(_) => continue,
                };

                let candidate = if val.starts_with("http://") || val.starts_with("https://") {
                    val.clone()
                } else if val.contains('.') || val.starts_with("localhost") {
                    format!("https://{}", val)
                } else {
                    continue;
                };
                if candidate.parse::<url::Url>().is_ok() {
                    println!("[WisdomBoard] 偵測到 URL: {}", candidate);
                    found_url = Some(candidate);
                    break;
                }
            }

            if found_url.is_none() {
                println!("[WisdomBoard] 未偵測到 URL (檢查了 {} 個元素)", count);
            }
            found_url
        })();

        if com_initialized {
            CoUninitialize();
        }
        result
    }
}

/// 擷取全螢幕截圖並存為 BMP 暫存檔
pub fn capture_screen_to_file() -> Result<String, String> {
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);

        let screen_dc = GetDC(HWND(0));
        if screen_dc.is_invalid() {
            return Err("GetDC 失敗".into());
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, bmp);
        if old.0 as isize == -1 {
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err("SelectObject 失敗".into());
        }

        let blt_result = BitBlt(mem_dc, 0, 0, w, h, screen_dc, 0, 0, SRCCOPY);
        if let Err(e) = blt_result {
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err(format!("BitBlt 失敗: {e}"));
        }

        let row_bytes = ((w as u32 * 3 + 3) & !3) as usize;
        let img_size = row_bytes * h as usize;
        let mut pixels = vec![0u8; img_size];

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 24,
                biCompression: BI_RGB.0 as u32,
                biSizeImage: img_size as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let scan_lines = GetDIBits(
            mem_dc, bmp, 0, h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi, DIB_RGB_COLORS,
        );
        if scan_lines == 0 {
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err("GetDIBits 失敗".into());
        }

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(HWND(0), screen_dc);

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = std::env::temp_dir().join(format!("wisdomboard_screenshot_{}.bmp", ts));
        let file_size = 54 + img_size;
        let mut f = std::fs::File::create(&path).map_err(|e| format!("建立檔案失敗: {e}"))?;
        use std::io::Write;
        f.write_all(b"BM").map_err(|e| format!("{e}"))?;
        f.write_all(&(file_size as u32).to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&0u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&54u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&40u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&w.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&(-h).to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&1u16.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&24u16.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&0u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&(img_size as u32).to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&[0u8; 16]).map_err(|e| format!("{e}"))?;
        f.write_all(&pixels).map_err(|e| format!("{e}"))?;

        let path_str = path.to_string_lossy().to_string();
        println!("[WisdomBoard] 螢幕截圖已存到: {} ({}x{}, {} bytes)", path_str, w, h, file_size);
        Ok(path_str)
    }
}

/// 直接用 GDI 擷取螢幕指定區域並存為 BMP 暫存檔
pub fn capture_region_to_file(x: i32, y: i32, w: i32, h: i32, label: &str) -> Result<String, String> {
    if w <= 0 || h <= 0 {
        return Err(format!("無效的擷取尺寸: {}x{}", w, h));
    }
    unsafe {
        let screen_dc = GetDC(HWND(0));
        if screen_dc.is_invalid() {
            return Err("GetDC 失敗".into());
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, bmp);
        if old.0 as isize == -1 {
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err("SelectObject 失敗".into());
        }

        let blt_result = BitBlt(mem_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY);
        if let Err(e) = blt_result {
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err(format!("BitBlt 失敗: {e}"));
        }

        let row_bytes = ((w as u32 * 3 + 3) & !3) as usize;
        let img_size = row_bytes * h as usize;
        let mut pixels = vec![0u8; img_size];

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 24,
                biCompression: BI_RGB.0 as u32,
                biSizeImage: img_size as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let scan_lines = GetDIBits(
            mem_dc, bmp, 0, h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi, DIB_RGB_COLORS,
        );
        if scan_lines == 0 {
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err("GetDIBits 失敗".into());
        }

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(HWND(0), screen_dc);

        let safe_label: String = label.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let filename = format!("wisdomboard_panel_{}.bmp", safe_label);
        let path = std::env::temp_dir().join(&filename);
        let file_size = 54 + img_size;
        let mut f = std::fs::File::create(&path).map_err(|e| format!("建立檔案失敗: {e}"))?;
        use std::io::Write;
        f.write_all(b"BM").map_err(|e| format!("{e}"))?;
        f.write_all(&(file_size as u32).to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&0u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&54u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&40u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&w.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&(-h).to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&1u16.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&24u16.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&0u32.to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&(img_size as u32).to_le_bytes()).map_err(|e| format!("{e}"))?;
        f.write_all(&[0u8; 16]).map_err(|e| format!("{e}"))?;
        f.write_all(&pixels).map_err(|e| format!("{e}"))?;

        let path_str = path.to_string_lossy().to_string();
        println!("[WisdomBoard] 區域截圖已存到: {} ({}x{} @ {},{}, {} bytes)", path_str, w, h, x, y, file_size);
        Ok(path_str)
    }
}

#[tauri::command]
pub fn get_screenshot(app: AppHandle) -> Result<String, String> {
    println!("[WisdomBoard] get_screenshot invoked");
    // 回傳 open_capture_overlay 預先截好的截圖路徑
    let state = app.state::<crate::state::ManagedState>();
    let path = state.lock()
        .map_err(|e| format!("state lock 失敗: {e}"))?
        .screenshot_path
        .clone()
        .ok_or_else(|| "尚未有截圖".to_string())?;
    println!("[WisdomBoard] get_screenshot OK: {}", path);
    Ok(path)
}

/// 取得截圖的 base64 data URL（解決 release build asset:// 路徑問題）
#[tauri::command]
pub fn get_screenshot_base64(app: AppHandle) -> Result<String, String> {
    let state = app.state::<crate::state::ManagedState>();
    let path = state.lock()
        .map_err(|e| format!("{e}"))?
        .screenshot_path
        .clone()
        .ok_or_else(|| "尚未有截圖".to_string())?;

    let data = std::fs::read(&path).map_err(|e| format!("讀取截圖失敗: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Ok(format!("data:image/bmp;base64,{}", b64))
}

/// 取得面板截圖的 base64 data URL
#[tauri::command]
pub fn get_panel_screenshot_base64(app: AppHandle, label: String) -> Result<String, String> {
    let state = app.state::<crate::state::ManagedState>();
    let guard = state.lock().map_err(|e| format!("{e}"))?;
    let path = guard.panels.get(&label)
        .and_then(|p| p.screenshot_path.clone())
        .ok_or_else(|| "無截圖".to_string())?;
    drop(guard);

    let data = std::fs::read(&path).map_err(|e| format!("讀取截圖失敗: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Ok(format!("data:image/bmp;base64,{}", b64))
}

/// 取得截圖前偵測到的瀏覽器 URL
#[tauri::command]
pub fn get_detected_url(app: AppHandle) -> Option<String> {
    let state = app.state::<crate::state::ManagedState>();
    let guard = state.lock().ok()?;
    guard.detected_url.clone()
}

/// overlay 關閉或 build 失敗時，恢復所有面板並重新套用 locked 模式
fn restore_panels_after_overlay(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label != "overlay" && label != "main" {
            let _ = w.show();
        }
    }
    let locked_labels: Vec<String> = {
        let state = app.state::<crate::state::ManagedState>();
        let result = match state.lock() {
            Ok(g) => g.panels.iter()
                .filter(|(_, cfg)| cfg.mode == "locked")
                .map(|(l, _)| l.clone())
                .collect(),
            Err(_) => vec![],
        };
        result
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        for l in locked_labels {
            let _ = crate::panel::set_panel_mode(app.clone(), l, "locked".into());
        }
    });
}

#[tauri::command]
pub fn open_capture_overlay(app: AppHandle) -> Result<(), String> {
    // 整個流程在獨立執行緒執行，避免阻塞 command handler
    std::thread::spawn(move || {
        if let Some(win) = app.get_webview_window("overlay") {
            let _ = win.close();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let detected_url = detect_browser_url();
        {
            let state = app.state::<crate::state::ManagedState>();
            if let Ok(mut guard) = state.lock() {
                guard.detected_url = detected_url;
                // 清理上一次的全域截圖暫存檔
                if let Some(old_path) = guard.screenshot_path.take() {
                    let _ = std::fs::remove_file(&old_path);
                }
            };
        }

        // 截圖前隱藏所有 WisdomBoard 視窗，確保截到真正的桌面內容
        let all_wins: Vec<_> = app.webview_windows().into_iter().collect();
        for (_, win) in &all_wins {
            if let Ok(raw) = win.hwnd() {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::*;
                    let hwnd = HWND(raw.0 as isize);
                    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                    if ex & WS_EX_TRANSPARENT.0 != 0 {
                        SetWindowLongW(hwnd, GWL_EXSTYLE, (ex & !WS_EX_TRANSPARENT.0) as i32);
                    }
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            let _ = win.hide();
        }
        // 等待視窗完全隱藏（包含 DWM 動畫）
        std::thread::sleep(std::time::Duration::from_millis(600));

        let screenshot_path = match capture_screen_to_file() {
            Ok(p) => p,
            Err(e) => {
                println!("[WisdomBoard] 截圖失敗: {e}");
                restore_panels_after_overlay(&app);
                return;
            }
        };

        {
            let state = app.state::<crate::state::ManagedState>();
            if let Ok(mut guard) = state.lock() {
                guard.screenshot_path = Some(screenshot_path.clone());
            };
        }

        let scale = app.primary_monitor()
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or(1.0);

        let (screen_w, screen_h) = unsafe {
            (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
        };

        let logical_w = screen_w as f64 / scale;
        let logical_h = screen_h as f64 / scale;

        println!("[WisdomBoard] 建立 overlay 視窗: {}x{} (scale={})", logical_w, logical_h, scale);

        let url = tauri::WebviewUrl::App("src/overlay.html".into());
        let builder = tauri::WebviewWindowBuilder::new(&app, "overlay", url)
            .title("WisdomBoard - 框選區域 (按 ESC 或關閉視窗取消)")
            .inner_size(logical_w, logical_h)
            .position(0.0, 0.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .transparent(false)
            .resizable(false);

        println!("[WisdomBoard] overlay builder 準備 build()...");
        match builder.build() {
            Ok(win) => {
                println!("[WisdomBoard] 框選 Overlay build() 成功");
                let _ = win.set_size(tauri::LogicalSize::new(logical_w, logical_h));
                let _ = win.set_position(tauri::LogicalPosition::new(0.0, 0.0));
                let _ = win.show();
                let _ = win.set_focus();

                let app_close = app.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::Destroyed = event {
                        restore_panels_after_overlay(&app_close);
                    }
                });
            }
            Err(e) => {
                println!("[WisdomBoard] overlay build() FAILED: {e}");
                restore_panels_after_overlay(&app);
            }
        }
    });

    Ok(())
}

/// 擷取螢幕區域並建立面板
#[tauri::command]
pub fn capture_region(
    app: AppHandle,
    x: f64, y: f64, width: f64, height: f64,
) -> Result<String, String> {
    if width <= 0.0 || height <= 0.0 {
        return Err(format!("無效的擷取尺寸: {}x{}", width, height));
    }
    // 使用主螢幕 scale_factor
    let scale = app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);

    let phys_x = (x * scale) as i32;
    let phys_y = (y * scale) as i32;
    let phys_w = (width * scale) as i32;
    let phys_h = (height * scale) as i32;

    // 隱藏 Overlay
    if let Some(overlay_win) = app.get_webview_window("overlay") {
        let _ = overlay_win.hide();
    }

    let label = crate::panel::next_panel_id();
    let logical_x = x;
    let logical_y = y;
    let logical_w = width;
    let logical_h = height;

    // 截圖與建視窗移到獨立執行緒，避免阻塞 command handler
    std::thread::spawn({
        let app = app.clone();
        let label = label.clone();
        move || {
            // 等待一幀讓 overlay 真正消失
            std::thread::sleep(std::time::Duration::from_millis(100));

            println!(
                "[WisdomBoard] 擷取區域: ({}, {}) {}x{} (scale: {})",
                phys_x, phys_y, phys_w, phys_h, scale
            );

            let screenshot_path = match capture_region_to_file(phys_x, phys_y, phys_w, phys_h, &label) {
                Ok(p) => p,
                Err(e) => {
                    println!("[WisdomBoard] 區域截圖失敗: {e}");
                    return;
                }
            };

            {
                let state = app.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    guard.panels.insert(
                        label.clone(),
                        PanelConfig {
                            label: label.clone(),
                            panel_type: PanelType::Capture,
                            url: None,
                            x: logical_x,
                            y: logical_y,
                            width: logical_w,
                            height: logical_h,
                            mode: "locked".into(),
                            zoom: 1.0,
                            target_hwnd: None,
                            source_rect: None,
                            screenshot_path: Some(screenshot_path.clone()),
                        },
                    );
                };
            }

            let url = tauri::WebviewUrl::App("src/panel.html".into());
            let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
                .title("WisdomBoard Capture".to_string())
                .inner_size(logical_w, logical_h)
                .position(logical_x, logical_y)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(false);

            match builder.build() {
                Ok(win) => {
                    crate::panel::set_square_corners(&win);
                    {
                        let a = app.clone();
                        let l = label.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            let _ = crate::panel::set_panel_mode(a, l, "locked".into());
                        });
                    }
                    let app_handle = app.clone();
                    let panel_label = label.clone();
                    win.on_window_event(move |event| {
                        crate::panel::handle_panel_event(&app_handle, &panel_label, event);
                    });
                    let _ = app.emit("panel-created",
                        serde_json::json!({
                            "label": &label, "type": "capture", "mode": "locked",
                            "x": logical_x, "y": logical_y,
                            "width": logical_w, "height": logical_h,
                            "screenshot_path": &screenshot_path,
                        }),
                    );
                    crate::persistence::auto_save(&app);
                }
                Err(e) => {
                    println!("[WisdomBoard] 擷取面板 build() FAILED: {e}");
                    let state = app.state::<ManagedState>();
                    if let Ok(mut guard) = state.lock() {
                        guard.panels.remove(&label);
                    };
                }
            }
        }
    });

    Ok(label)
}


/// Debug 測試
#[tauri::command]
pub fn run_debug_tests() -> Result<String, String> {
    let mut report = String::new();

    let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    report.push_str(&format!("1. 螢幕尺寸: {}x{}\n", sw, sh));

    match capture_screen_to_file() {
        Ok(path) => {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            report.push_str(&format!("2. 截圖存檔: OK\n   路徑: {}\n   大小: {} bytes\n", path, size));
        }
        Err(e) => report.push_str(&format!("2. 截圖存檔: FAIL: {}\n", e)),
    }

    match "https://cli.github.com/".parse::<url::Url>() {
        Ok(u) => report.push_str(&format!("3. URL 解析: OK ({})\n", u)),
        Err(e) => report.push_str(&format!("3. URL 解析: FAIL: {}\n", e)),
    }

    report.push_str(&format!("4. Temp 目錄: {}\n", std::env::temp_dir().display()));

    Ok(report)
}
