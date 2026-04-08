use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{Manager, AppHandle, Emitter};
use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    MOD_ALT, MOD_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, GetSystemMetrics, MSG, SM_CXSCREEN, SM_CYSCREEN, WM_HOTKEY,
};

const HOTKEY_SNIP: i32 = 1;
static PANEL_COUNT: AtomicI32 = AtomicI32::new(0);

/// 在背景執行緒中監聽全域快捷鍵
fn start_hotkey_listener(app_handle: AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            let modifiers = HOT_KEY_MODIFIERS(MOD_ALT.0 | MOD_CONTROL.0);
            let result = RegisterHotKey(HWND(0), HOTKEY_SNIP, modifiers, 0x53); // 'S'
            if result.is_err() {
                eprintln!("[WisdomBoard] 註冊快捷鍵 Ctrl+Alt+S 失敗");
                return;
            }
            println!("[WisdomBoard] 全域快捷鍵 Ctrl+Alt+S 已註冊");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND(0), 0, 0).as_bool() {
                if msg.message == WM_HOTKEY && msg.wParam == WPARAM(HOTKEY_SNIP as usize) {
                    println!("[WisdomBoard] Ctrl+Alt+S 觸發，開啟設定視窗");
                    let _ = open_settings(app_handle.clone());
                }
            }

            let _ = UnregisterHotKey(HWND(0), HOTKEY_SNIP);
        }
    });
}

/// 開啟或聚焦設定視窗
fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let url = tauri::WebviewUrl::App("src/settings.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, "settings", url)
        .title("WisdomBoard 設定")
        .inner_size(480.0, 560.0)
        .decorations(true)
        .always_on_top(true)
        .resizable(true)
        .center();

    match builder.build() {
        Ok(_) => {
            println!("[WisdomBoard] 設定視窗已開啟");
            Ok(())
        }
        Err(e) => {
            eprintln!("[WisdomBoard] 開啟設定視窗失敗: {e}");
            Err(format!("{e}"))
        }
    }
}

/// 建立一個 URL 面板（載入本地頁面後重導向到外部 URL）
#[tauri::command]
fn create_url_panel(app: AppHandle, url: String) -> Result<String, String> {
    let id = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    let label = format!("panel-{}", id);

    // 驗證 URL 格式
    let _parsed: url::Url = url.parse()
        .map_err(|e: url::ParseError| format!("網址格式錯誤: {e}"))?;

    // 先載入本地頁面，再透過 JS 重導向（External URL 在 dev 模式不渲染）
    let target_url = url.clone();
    let webview_url = tauri::WebviewUrl::App("src/webpanel.html".into());
    let navigated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, webview_url)
        .title(format!("WisdomBoard - {}", url))
        .inner_size(800.0, 600.0)
        .decorations(true)       // 保留視窗框架，確保使用者可以關閉
        .always_on_top(false)    // 不強制置頂，避免擋住其他視窗
        .skip_taskbar(false)
        .transparent(false)
        .on_navigation(|_url| true)  // 允許所有導航（包括重導向到外部 URL）
        .on_page_load({
            let navigated = navigated.clone();
            move |wv, payload| {
                if let tauri::webview::PageLoadEvent::Finished = payload.event() {
                    // 只在第一次載入時重導向
                    if !navigated.swap(true, Ordering::SeqCst) {
                        println!("[WisdomBoard] 頁面載入完成，重導向到: {}", target_url);
                        let js = format!("window.location.href = '{}';",
                            target_url.replace('\'', "\\'"));
                        let _ = wv.eval(&js);
                    }
                }
            }
        });

    match builder.build() {
        Ok(win) => {
            let app_handle = app.clone();
            let panel_label = label.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Destroyed = event {
                    let _ = app_handle.emit("panel-closed", &panel_label);
                }
            });
            println!("[WisdomBoard] URL 面板 {} 已建立: {}", label, url);
            Ok(label)
        }
        Err(e) => {
            eprintln!("[WisdomBoard] 建立面板失敗: {e}");
            Err(format!("{e}"))
        }
    }
}

/// 建立一個空白面板
#[tauri::command]
fn create_panel(app: AppHandle) -> Result<String, String> {
    let id = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    let label = format!("panel-{}", id);

    let url = tauri::WebviewUrl::App("src/panel.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(format!("WisdomBoard Panel {}", id))
        .inner_size(400.0, 300.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(false);

    match builder.build() {
        Ok(win) => {
            let app_handle = app.clone();
            let panel_label = label.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Destroyed = event {
                    let _ = app_handle.emit("panel-closed", &panel_label);
                }
            });
            println!("[WisdomBoard] 面板 {} 已建立", label);
            Ok(label)
        }
        Err(e) => {
            eprintln!("[WisdomBoard] 建立面板失敗: {e}");
            Err(format!("{e}"))
        }
    }
}

/// 設定個別面板的操作模式
#[tauri::command]
fn set_panel_mode(app: AppHandle, label: String, mode: String) -> Result<(), String> {
    let window = app.get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let _ = app.emit_to(&label, "mode-changed", &mode);
    let _ = window.set_resizable(mode == "resize");
    println!("[WisdomBoard] 面板 {} 模式: {}", label, mode);
    Ok(())
}

/// 擷取螢幕截圖，存到 temp 檔案，回傳檔案路徑
fn capture_screen_to_file() -> Result<String, String> {
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);

        let screen_dc = GetDC(HWND(0));
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, bmp);

        BitBlt(mem_dc, 0, 0, w, h, screen_dc, 0, 0, SRCCOPY)
            .map_err(|e| format!("BitBlt 失敗: {e}"))?;

        let row_bytes = ((w as u32 * 3 + 3) & !3) as usize;
        let img_size = row_bytes * h as usize;
        let mut pixels = vec![0u8; img_size];

        // 使用負的 biHeight 產生 top-down BMP（避免圖片上下顛倒）
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

        GetDIBits(
            mem_dc, bmp, 0, h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi, DIB_RGB_COLORS,
        );

        // 存到 temp 檔案（top-down BMP 在檔案中仍需正的 biHeight）
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

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(HWND(0), screen_dc);

        let path_str = path.to_string_lossy().to_string();
        println!("[WisdomBoard] 螢幕截圖已存到: {} ({}x{}, {} bytes)", path_str, w, h, file_size);
        Ok(path_str)
    }
}

/// 回傳截圖檔案路徑（供前端使用 convertFileSrc 讀取）
#[tauri::command]
fn get_screenshot() -> Result<String, String> {
    capture_screen_to_file()
}

/// 開啟全螢幕框選 Overlay（截圖式，不依賴透明視窗）
#[tauri::command]
fn open_capture_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    // 先截圖（在開啟 overlay 之前，這樣截圖不會包含 overlay 本身）
    let screenshot_path = capture_screen_to_file()?;

    let (screen_w, screen_h) = unsafe {
        (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    };

    let url = tauri::WebviewUrl::App("src/overlay.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, "overlay", url)
        .title("WisdomBoard - 框選區域 (按 ESC 或關閉視窗取消)")
        .inner_size(screen_w as f64, screen_h as f64)
        .position(0.0, 0.0)
        .decorations(true)       // 保留標題列，確保使用者可以關閉
        .always_on_top(true)
        .skip_taskbar(false)
        .transparent(false)
        .resizable(false)
        .on_page_load(move |wv, payload| {
            if let tauri::webview::PageLoadEvent::Finished = payload.event() {
                let js = format!(
                    "window.__SCREENSHOT_PATH__ = '{}';",
                    screenshot_path.replace('\\', "\\\\"),
                );
                let _ = wv.eval(&js);
            }
        });

    match builder.build() {
        Ok(_) => {
            println!("[WisdomBoard] 框選 Overlay 已開啟 ({}x{})", screen_w, screen_h);
            Ok(())
        }
        Err(e) => {
            eprintln!("[WisdomBoard] 開啟 Overlay 失敗: {e}");
            Err(format!("{e}"))
        }
    }
}

/// 擷取螢幕區域並建立面板
/// 注意：overlay 傳來的座標是 CSS 像素，需乘以 DPI 縮放因子轉為物理像素
#[tauri::command]
fn capture_region(app: AppHandle, x: f64, y: f64, width: f64, height: f64) -> Result<String, String> {
    // 取得 overlay 的 DPI 縮放因子
    let scale = app.get_webview_window("overlay")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);

    let phys_x = (x * scale) as i32;
    let phys_y = (y * scale) as i32;
    let phys_w = (width * scale) as i32;
    let phys_h = (height * scale) as i32;

    println!("[WisdomBoard] 擷取區域: ({}, {}) {}x{} (scale: {})", phys_x, phys_y, phys_w, phys_h, scale);

    let id = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    let label = format!("panel-{}", id);

    let url = tauri::WebviewUrl::App("src/panel.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(format!("WisdomBoard Capture {}", id))
        .inner_size(phys_w as f64, phys_h as f64)
        .position(phys_x as f64, phys_y as f64)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(false);

    match builder.build() {
        Ok(win) => {
            let app_handle = app.clone();
            let panel_label = label.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Destroyed = event {
                    let _ = app_handle.emit("panel-closed", &panel_label);
                }
            });

            let _ = app.emit("panel-created", serde_json::json!({
                "label": &label,
                "type": "capture",
                "x": phys_x, "y": phys_y, "width": phys_w, "height": phys_h
            }));

            println!("[WisdomBoard] 擷取面板 {} 已建立", label);
            Ok(label)
        }
        Err(e) => Err(format!("{e}"))
    }
}

/// 設定面板的縮放比例
#[tauri::command]
fn set_panel_zoom(app: AppHandle, label: String, zoom: f64) -> Result<(), String> {
    let window = app.get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let js = format!(
        "document.documentElement.style.transform = 'scale({z})'; \
         document.documentElement.style.transformOrigin = 'top left'; \
         document.documentElement.style.width = '{w}%'; \
         document.documentElement.style.height = '{h}%';",
        z = zoom,
        w = 100.0 / zoom,
        h = 100.0 / zoom,
    );
    let _ = window.eval(&js);
    println!("[WisdomBoard] 面板 {} 縮放: {}%", label, (zoom * 100.0) as i32);
    Ok(())
}

/// 關閉指定面板
#[tauri::command]
fn close_panel(app: AppHandle, label: String) -> Result<(), String> {
    let window = app.get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    window.close().map_err(|e| format!("{e}"))
}

/// 聚焦指定面板
#[tauri::command]
fn focus_panel(app: AppHandle, label: String) -> Result<(), String> {
    let window = app.get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let _ = window.show();
    window.set_focus().map_err(|e| format!("{e}"))
}

/// 設定所有面板的操作模式（只廣播到面板視窗）
#[tauri::command]
fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    println!("[WisdomBoard] 切換模式: {}", mode);
    // 遍歷所有面板視窗發送事件，避免廣播到 settings 等非面板視窗
    for (label, window) in app.webview_windows() {
        if label.starts_with("panel-") {
            let _ = window.emit("mode-changed", &mode);
        }
    }
    Ok(())
}

/// Debug 測試：回傳各子系統狀態
#[tauri::command]
fn run_debug_tests() -> Result<String, String> {
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

    println!("[WisdomBoard] === DEBUG REPORT ===\n{}", report);
    Ok(report)
}

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // MacosLauncher 為 tauri-plugin-autostart 的必要參數，在 Windows 上會被忽略
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let autostart_manager = app.autolaunch();
            if !autostart_manager.is_enabled().unwrap_or(false) {
                if let Err(e) = autostart_manager.enable() {
                    eprintln!("[WisdomBoard] 自啟動設定失敗: {e}");
                }
            }

            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.hide();
            }

            start_hotkey_listener(app.handle().clone());

            let settings_i = MenuItem::with_id(app, "settings", "設定 (Ctrl+Alt+S)", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "離開", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &quit_i])?;

            let mut tray_builder = TrayIconBuilder::new().menu(&menu);
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "settings" => {
                        let _ = open_settings(app.clone());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            println!("[WisdomBoard] 系統匣應用已啟動");
            println!("[WisdomBoard] 按 Ctrl+Alt+S 開啟設定視窗");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![create_panel, create_url_panel, close_panel, focus_panel, set_mode, set_panel_mode, set_panel_zoom, open_capture_overlay, capture_region, get_screenshot, run_debug_tests])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
