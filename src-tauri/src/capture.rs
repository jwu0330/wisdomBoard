use crate::state::{ManagedState, PanelConfig, PanelType};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWM_THUMBNAIL_PROPERTIES,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, MapWindowPoints, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, WindowFromPoint,
    SM_CXSCREEN, SM_CYSCREEN,
};

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

        let path = std::env::temp_dir().join("wisdomboard_screenshot.bmp");
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

#[tauri::command]
pub fn get_screenshot() -> Result<String, String> {
    capture_screen_to_file()
}

#[tauri::command]
pub fn open_capture_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.close();
    }

    let screenshot_path = capture_screen_to_file()?;

    let scale = app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);

    let (screen_w, screen_h) = unsafe {
        (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    };

    // 轉換為邏輯像素
    let logical_w = screen_w as f64 / scale;
    let logical_h = screen_h as f64 / scale;

    let safe_path_js = serde_json::to_string(&screenshot_path)
        .unwrap_or_else(|_| {
            let escaped = screenshot_path
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            format!("\"{}\"", escaped)
        });

    let url = tauri::WebviewUrl::App("src/overlay.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, "overlay", url)
        .title("WisdomBoard - 框選區域 (按 ESC 或關閉視窗取消)")
        .inner_size(logical_w, logical_h)
        .position(0.0, 0.0)
        .decorations(true)
        .always_on_top(true)
        .skip_taskbar(false)
        .transparent(false)
        .resizable(false)
        .on_page_load(move |wv, payload| {
            if let tauri::webview::PageLoadEvent::Finished = payload.event() {
                let js = format!("window.__SCREENSHOT_PATH__ = {};", safe_path_js);
                let _ = wv.eval(&js);
            }
        });

    match builder.build() {
        Ok(_) => {
            println!("[WisdomBoard] 框選 Overlay 已開啟 ({}x{} logical, scale: {})", logical_w, logical_h, scale);
            Ok(())
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// 擷取螢幕區域並建立面板，同時偵測目標視窗 HWND 以啟用 DWM thumbnail
#[tauri::command]
pub fn capture_region(
    app: AppHandle,
    x: f64, y: f64, width: f64, height: f64,
) -> Result<String, String> {
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

    // 先隱藏 Overlay 以避免 WindowFromPoint 偵測到它
    if let Some(overlay_win) = app.get_webview_window("overlay") {
        let _ = overlay_win.hide();
    }

    // 偵測擷取區域中心的目標視窗
    let center = POINT {
        x: phys_x + phys_w / 2,
        y: phys_y + phys_h / 2,
    };
    let target_hwnd = unsafe { WindowFromPoint(center) };
    let target_hwnd_val = if target_hwnd.0 != 0 {
        Some(target_hwnd.0)
    } else {
        None
    };

    // 計算在目標視窗中的相對座標（用於 DWM source rect）
    let source_rect = if target_hwnd_val.is_some() {
        unsafe {
            let mut pts = [
                POINT { x: phys_x, y: phys_y },
                POINT { x: phys_x + phys_w, y: phys_y + phys_h },
            ];
            // 螢幕座標 → 目標視窗 client 座標
            let _ = MapWindowPoints(HWND(0), target_hwnd, &mut pts);
            Some([pts[0].x, pts[0].y, pts[1].x - pts[0].x, pts[1].y - pts[0].y])
        }
    } else {
        None
    };

    println!(
        "[WisdomBoard] 擷取區域: ({}, {}) {}x{} (scale: {}, target: {:?})",
        phys_x, phys_y, phys_w, phys_h, scale, target_hwnd_val
    );

    let label = crate::panel::next_panel_id();

    let url = tauri::WebviewUrl::App("src/panel.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title("WisdomBoard Capture".to_string())
        .inner_size(phys_w as f64, phys_h as f64)
        .position(phys_x as f64, phys_y as f64)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(false);

    match builder.build() {
        Ok(win) => {
            // 註冊 DWM thumbnail
            if let Some(thwnd) = target_hwnd_val {
                register_dwm_thumbnail(&app, &label, &win, HWND(thwnd), phys_w, phys_h, source_rect.as_ref());
            }

            // 存入 state
            {
                let state = app.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    guard.panels.insert(
                        label.clone(),
                        PanelConfig {
                            label: label.clone(),
                            panel_type: PanelType::Capture,
                            url: None,
                            x: phys_x as f64,
                            y: phys_y as f64,
                            width: phys_w as f64,
                            height: phys_h as f64,
                            mode: "view".into(),
                            zoom: 1.0,
                            target_hwnd: target_hwnd_val,
                            source_rect,
                        },
                    );
                };
            }

            // 監聽視窗事件
            let app_handle = app.clone();
            let panel_label = label.clone();
            win.on_window_event(move |event| {
                match event {
                    tauri::WindowEvent::Destroyed => {
                        cleanup_panel(&app_handle, &panel_label);
                        let _ = app_handle.emit("panel-closed", &panel_label);
                    }
                    tauri::WindowEvent::Moved(pos) => {
                        update_panel_position(&app_handle, &panel_label, pos.x as f64, pos.y as f64);
                    }
                    tauri::WindowEvent::Resized(size) => {
                        update_panel_size(&app_handle, &panel_label, size.width as f64, size.height as f64);
                    }
                    _ => {}
                }
            });

            let _ = app.emit_to(
                "settings",
                "panel-created",
                serde_json::json!({
                    "label": &label,
                    "type": "capture",
                    "x": phys_x, "y": phys_y, "width": phys_w, "height": phys_h
                }),
            );

            crate::persistence::auto_save(&app);
            Ok(label)
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// 公開版本供 panel.rs restore 使用
pub fn register_dwm_thumbnail_pub(
    app: &AppHandle,
    label: &str,
    win: &tauri::WebviewWindow,
    target: HWND,
    panel_w: i32,
    panel_h: i32,
    source_rect: Option<&[i32; 4]>,
) {
    register_dwm_thumbnail(app, label, win, target, panel_w, panel_h, source_rect);
}

fn register_dwm_thumbnail(
    app: &AppHandle,
    label: &str,
    win: &tauri::WebviewWindow,
    target: HWND,
    panel_w: i32,
    panel_h: i32,
    source_rect: Option<&[i32; 4]>,
) {
    // 取得面板視窗的 HWND
    let panel_hwnd = match win.hwnd() {
        Ok(hwnd) => HWND(hwnd.0 as isize),
        Err(_) => return,
    };

    unsafe {
        match DwmRegisterThumbnail(panel_hwnd, target) {
            Ok(thumb_id) => {
                // DWM_TNP flags: RECTDESTINATION=0x01, RECTSOURCE=0x02, VISIBLE=0x08
                let mut props: DWM_THUMBNAIL_PROPERTIES = std::mem::zeroed();
                let mut flags: u32 = 0x01 | 0x08;
                props.fVisible = windows::Win32::Foundation::BOOL(1);
                props.rcDestination = RECT {
                    left: 0,
                    top: 0,
                    right: panel_w,
                    bottom: panel_h,
                };

                if let Some(sr) = source_rect {
                    flags |= 0x02;
                    props.rcSource = RECT {
                        left: sr[0],
                        top: sr[1],
                        right: sr[0] + sr[2],
                        bottom: sr[1] + sr[3],
                    };
                }

                props.dwFlags = flags;

                let _ = DwmUpdateThumbnailProperties(thumb_id, &props);

                let state = app.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    guard.dwm_thumbnails.insert(label.to_string(), thumb_id);
                };
                println!("[WisdomBoard] DWM thumbnail 已註冊: {}", label);
            }
            Err(e) => {
                eprintln!("[WisdomBoard] DWM thumbnail 註冊失敗: {e}");
            }
        }
    }
}

/// 面板關閉時清理 DWM thumbnail 和 state
fn cleanup_panel(app: &AppHandle, label: &str) {
    let state = app.state::<ManagedState>();
    if let Ok(mut guard) = state.lock() {
        guard.panels.remove(label);
        if let Some(thumb_id) = guard.dwm_thumbnails.remove(label) {
            unsafe {
                let _ = DwmUnregisterThumbnail(thumb_id);
            }
            println!("[WisdomBoard] DWM thumbnail 已清除: {}", label);
        }
    }
    crate::persistence::auto_save(app);
}

fn update_panel_position(app: &AppHandle, label: &str, x: f64, y: f64) {
    let state = app.state::<ManagedState>();
    if let Ok(mut guard) = state.lock() {
        if let Some(panel) = guard.panels.get_mut(label) {
            panel.x = x;
            panel.y = y;
        }
    };
    // 延遲儲存（移動事件頻繁，不需每次都寫檔）
}

fn update_panel_size(app: &AppHandle, label: &str, width: f64, height: f64) {
    let state = app.state::<ManagedState>();
    if let Ok(mut guard) = state.lock() {
        if let Some(panel) = guard.panels.get_mut(label) {
            panel.width = width;
            panel.height = height;
        }
        // 更新 DWM thumbnail destination rect
        if let Some(&thumb_id) = guard.dwm_thumbnails.get(label) {
            unsafe {
                let mut props: DWM_THUMBNAIL_PROPERTIES = std::mem::zeroed();
                props.dwFlags = 0x01_u32;
                props.rcDestination = RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                };
                let _ = DwmUpdateThumbnailProperties(thumb_id, &props);
            }
        }
    };
    crate::persistence::auto_save(app);
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
