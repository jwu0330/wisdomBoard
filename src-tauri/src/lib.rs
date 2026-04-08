use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{Manager, AppHandle, Emitter};
use url;
use windows::Win32::Foundation::{HWND, WPARAM};
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
    // 如果設定視窗已存在，聚焦它
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    // 建立新的設定視窗（一般可拖拉視窗）
    let url = tauri::WebviewUrl::App("src/settings.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, "settings", url)
        .title("WisdomBoard 設定")
        .inner_size(480.0, 560.0)
        .decorations(true)    // 標準視窗框架
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

/// 建立一個 URL 面板（直接載入外部網頁）
#[tauri::command]
fn create_url_panel(app: AppHandle, url: String) -> Result<String, String> {
    let id = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    let label = format!("panel-{}", id);

    // 直接使用 External URL 載入外部網頁
    let parsed: url::Url = url.parse().map_err(|e: url::ParseError| format!("網址格式錯誤: {e}"))?;
    let webview_url = tauri::WebviewUrl::External(parsed);

    let builder = tauri::WebviewWindowBuilder::new(&app, &label, webview_url)
        .title(format!("WisdomBoard Panel {}", id))
        .inner_size(480.0, 360.0)
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
    if let Some(window) = app.get_webview_window(&label) {
        let _ = app.emit_to(&label, "mode-changed", &mode);
        let _ = window.set_resizable(mode == "resize");
        println!("[WisdomBoard] 面板 {} 模式: {}", label, mode);
    }
    Ok(())
}

/// 開啟框選設定（在設定視窗中操作，不使用全螢幕 Overlay）
#[tauri::command]
fn open_capture_overlay(app: AppHandle) -> Result<(), String> {
    // 暫時方案：直接在選取位置建立一個可調整的面板
    // 全螢幕透明 Overlay 在 Tauri + Windows 上有相容性問題（見 DEVELOPMENT.md 10.0）
    let id = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    let label = format!("panel-{}", id);

    let url = tauri::WebviewUrl::App("src/panel.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(format!("WisdomBoard {}", id))
        .inner_size(400.0, 300.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true);

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
                "type": "capture"
            }));

            println!("[WisdomBoard] 面板 {} 已建立（手動調整位置和大小）", label);
            Ok(())
        }
        Err(e) => Err(format!("{e}"))
    }
}

/// 擷取螢幕區域並建立面板
#[tauri::command]
fn capture_region(app: AppHandle, x: i32, y: i32, width: i32, height: i32) -> Result<String, String> {
    println!("[WisdomBoard] 擷取區域: ({}, {}) {}x{}", x, y, width, height);

    let id = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    let label = format!("panel-{}", id);

    // 建立面板視窗，顯示在擷取位置
    let url = tauri::WebviewUrl::App("src/panel.html".into());
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(format!("WisdomBoard Capture {}", id))
        .inner_size(width as f64, height as f64)
        .position(x as f64, y as f64)
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

            // 通知設定視窗新面板已建立
            let _ = app.emit("panel-created", serde_json::json!({
                "label": &label,
                "type": "capture",
                "x": x, "y": y, "width": width, "height": height
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
    if let Some(window) = app.get_webview_window(&label) {
        // 使用 CSS zoom 縮放整個頁面內容（保持 RWD 不變）
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
    }
    Ok(())
}

/// 關閉指定面板
#[tauri::command]
fn close_panel(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// 聚焦指定面板
#[tauri::command]
fn focus_panel(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        window.set_focus().map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// 設定所有面板的操作模式
#[tauri::command]
fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    println!("[WisdomBoard] 切換模式: {}", mode);
    // 廣播模式切換事件到所有面板
    let _ = app.emit("mode-changed", &mode);
    Ok(())
}

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 啟用開機自啟動
            let autostart_manager = app.autolaunch();
            if !autostart_manager.is_enabled().unwrap_or(false) {
                if let Err(e) = autostart_manager.enable() {
                    eprintln!("[WisdomBoard] 自啟動設定失敗: {e}");
                }
            }

            // 隱藏主視窗（僅作為系統匣應用）
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.hide();
            }

            // 啟動全域快捷鍵監聽
            start_hotkey_listener(app.handle().clone());

            // 建立系統匣選單
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
        .invoke_handler(tauri::generate_handler![create_panel, create_url_panel, close_panel, focus_panel, set_mode, set_panel_mode, set_panel_zoom, open_capture_overlay, capture_region])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // 防止所有視窗關閉時程式結束（系統匣常駐）
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
