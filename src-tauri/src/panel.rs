use crate::state::{ManagedState, PanelConfig, PanelType};
use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::HWND;

/// 移除 Windows 11 視窗圓角
pub fn set_square_corners(win: &tauri::WebviewWindow) {
    if let Ok(raw) = win.hwnd() {
        let hwnd = HWND(raw.0 as isize);
        unsafe {
            use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE};
            // 直角（不修改 window style，避免 WebView 客戶區錯位）
            let preference: u32 = 1; // DWMWCP_DONOTROUND
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const u32 as *const _,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

static PANEL_COUNT: AtomicU32 = AtomicU32::new(0);

pub fn next_panel_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() % 100000;
    let seq = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    format!("panel-{}-{}", ts, seq)
}

/// 關閉所有面板與 overlay，並儲存狀態
#[tauri::command]
pub fn close_all_panels(app: AppHandle) -> Result<(), String> {
    // 關閉所有 panel- 視窗
    let windows: Vec<_> = app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with("panel-") || label == "overlay")
        .collect();

    for (label, win) in &windows {
        let _ = win.close();
        println!("[WisdomBoard] 關閉視窗: {}", label);
    }

    // 清空 state
    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            guard.panels.clear();
        };
    }

    crate::persistence::auto_save(&app);
    println!("[WisdomBoard] 全部面板已關閉");
    Ok(())
}

#[tauri::command]
pub fn list_panels(app: AppHandle) -> Vec<serde_json::Value> {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with("panel-"))
        .map(|(label, win)| {
            let title = win.title().unwrap_or_default();
            let state = app.state::<ManagedState>();
            let (panel_type, url, mode, zoom) = state
                .lock()
                .ok()
                .and_then(|g| {
                    g.panels.get(&label).map(|p| (
                        if p.panel_type == PanelType::Url { "url" } else { "capture" },
                        p.url.clone(),
                        p.mode.clone(),
                        p.zoom,
                    ))
                })
                .unwrap_or(("capture", None, "locked".to_string(), 1.0));
            serde_json::json!({
                "label": label,
                "title": title,
                "type": panel_type,
                "url": url,
                "mode": mode,
                "zoom": zoom,
            })
        })
        .collect()
}

#[tauri::command]
pub fn create_url_panel(app: AppHandle, url: String) -> Result<String, String> {
    let label = next_panel_id();

    let _parsed: url::Url = url
        .parse()
        .map_err(|e: url::ParseError| format!("網址格式錯誤: {e}"))?;

    // 先 insert 預佔 state
    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            guard.panels.insert(
                label.clone(),
                PanelConfig {
                    label: label.clone(),
                    panel_type: PanelType::Url,
                    url: Some(url.clone()),
                    x: 0.0,
                    y: 0.0,
                    width: 800.0,
                    height: 600.0,
                    mode: "edit".into(),
                    zoom: 1.0,
                    target_hwnd: None,
                    source_rect: None,
                    screenshot_path: None,
                },
            );
        };
    }

    // 在獨立執行緒建立視窗，避免阻塞 command handler
    std::thread::spawn({
        let app = app.clone();
        let label = label.clone();
        let url = url.clone();
        move || {
            let parsed_url: url::Url = match url.parse() {
                Ok(u) => u,
                Err(e) => { println!("[WisdomBoard] URL parse error in thread: {e}"); return; }
            };
            let webview_url = tauri::WebviewUrl::External(parsed_url);
            let builder = tauri::WebviewWindowBuilder::new(&app, &label, webview_url)
                .title(format!("WisdomBoard - {}", url))
                .inner_size(800.0, 600.0)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(false)
                .on_navigation(|_url| true);

            match builder.build() {
                Ok(win) => {
                    set_square_corners(&win);
                    let app_handle = app.clone();
                    let panel_label = label.clone();
                    win.on_window_event(move |event| {
                        match event {
                            tauri::WindowEvent::Destroyed => {
                                {
                                    let state = app_handle.state::<ManagedState>();
                                    if let Ok(mut guard) = state.lock() {
                                        guard.panels.remove(&panel_label);
                                    };
                                }
                                crate::persistence::auto_save(&app_handle);
                                let _ = app_handle.emit("panel-closed", &panel_label);
                            }
                            tauri::WindowEvent::Moved(pos) => {
                                let state = app_handle.state::<ManagedState>();
                                if let Ok(mut guard) = state.lock() {
                                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                                        p.x = pos.x as f64;
                                        p.y = pos.y as f64;
                                    }
                                };
                            }
                            tauri::WindowEvent::Resized(size) => {
                                let state = app_handle.state::<ManagedState>();
                                if let Ok(mut guard) = state.lock() {
                                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                                        p.width = size.width as f64;
                                        p.height = size.height as f64;
                                    }
                                };
                            }
                            _ => {}
                        }
                    });
                    crate::persistence::auto_save(&app);
                    println!("[WisdomBoard] URL 面板 {} 已建立: {}", label, url);
                    let _ = app.emit("panel-created", serde_json::json!({
                        "label": &label, "type": "url", "url": &url
                    }));
                }
                Err(e) => {
                    println!("[WisdomBoard] URL 面板 build() FAILED: {e}");
                    let state = app.state::<ManagedState>();
                    if let Ok(mut guard) = state.lock() {
                        guard.panels.remove(&label);
                    }
                    let _ = app.emit("panel-create-failed", &label);
                }
            }
        }
    });

    println!("[WisdomBoard] URL 面板 {} 建立中: {}", label, url);
    Ok(label)
}

/// 在指定位置和大小建立 URL 面板（從 overlay 框選呼叫）
#[tauri::command]
pub fn create_url_panel_at(
    app: AppHandle, url: String,
    x: f64, y: f64, width: f64, height: f64,
) -> Result<String, String> {
    let label = next_panel_id();

    url.parse::<url::Url>()
        .map_err(|e: url::ParseError| format!("網址格式錯誤: {e}"))?;

    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            guard.panels.insert(
                label.clone(),
                PanelConfig {
                    label: label.clone(),
                    panel_type: PanelType::Url,
                    url: Some(url.clone()),
                    x, y, width, height,
                    mode: "edit".into(),
                    zoom: 1.0,
                    target_hwnd: None,
                    source_rect: None,
                    screenshot_path: None,
                },
            );
        };
    }

    std::thread::spawn({
        let app = app.clone();
        let label = label.clone();
        let url = url.clone();
        move || {
            let parsed_url: url::Url = match url.parse() {
                Ok(u) => u,
                Err(e) => { println!("[WisdomBoard] URL parse error: {e}"); return; }
            };
            let webview_url = tauri::WebviewUrl::External(parsed_url);
            let builder = tauri::WebviewWindowBuilder::new(&app, &label, webview_url)
                .title(format!("WisdomBoard - {}", url))
                .inner_size(width, height)
                .position(x, y)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(false)
                .on_navigation(|_url| true);

            match builder.build() {
                Ok(win) => {
                    set_square_corners(&win);
                    let app_handle = app.clone();
                    let panel_label = label.clone();
                    win.on_window_event(move |event| {
                        match event {
                            tauri::WindowEvent::Destroyed => {
                                {
                                    let state = app_handle.state::<ManagedState>();
                                    if let Ok(mut guard) = state.lock() {
                                        guard.panels.remove(&panel_label);
                                    };
                                }
                                crate::persistence::auto_save(&app_handle);
                                let _ = app_handle.emit("panel-closed", &panel_label);
                            }
                            tauri::WindowEvent::Moved(pos) => {
                                let state = app_handle.state::<ManagedState>();
                                if let Ok(mut guard) = state.lock() {
                                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                                        p.x = pos.x as f64;
                                        p.y = pos.y as f64;
                                    }
                                };
                            }
                            tauri::WindowEvent::Resized(size) => {
                                let state = app_handle.state::<ManagedState>();
                                if let Ok(mut guard) = state.lock() {
                                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                                        p.width = size.width as f64;
                                        p.height = size.height as f64;
                                    }
                                };
                            }
                            _ => {}
                        }
                    });
                    crate::persistence::auto_save(&app);
                    println!("[WisdomBoard] URL 面板 {} 已建立 (框選): {} @ ({},{}) {}x{}",
                        label, url, x, y, width, height);
                    let _ = app.emit("panel-created", serde_json::json!({
                        "label": &label, "type": "url", "url": &url, "mode": "edit"
                    }));
                }
                Err(e) => {
                    println!("[WisdomBoard] URL 面板 build() FAILED: {e}");
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

#[tauri::command]
pub fn create_panel(app: AppHandle) -> Result<String, String> {
    let label = next_panel_id();

    // 先 insert 預佔 state
    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            guard.panels.insert(
                label.clone(),
                PanelConfig {
                    label: label.clone(),
                    panel_type: PanelType::Capture,
                    url: None,
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 300.0,
                    mode: "locked".into(),
                    zoom: 1.0,
                    target_hwnd: None,
                    source_rect: None,
                    screenshot_path: None,
                },
            );
        };
    }

    std::thread::spawn({
        let app = app.clone();
        let label = label.clone();
        move || {
            let url = tauri::WebviewUrl::App("src/panel.html".into());
            let builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
                .title("WisdomBoard Panel".to_string())
                .inner_size(400.0, 300.0)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(false);

            match builder.build() {
                Ok(win) => {
                    set_square_corners(&win);
                    let app_handle = app.clone();
                    let panel_label = label.clone();
                    win.on_window_event(move |event| {
                        match event {
                            tauri::WindowEvent::Destroyed => {
                                {
                                    let state = app_handle.state::<ManagedState>();
                                    if let Ok(mut guard) = state.lock() {
                                        guard.panels.remove(&panel_label);
                                    };
                                }
                                crate::persistence::auto_save(&app_handle);
                                let _ = app_handle.emit("panel-closed", &panel_label);
                            }
                            tauri::WindowEvent::Moved(pos) => {
                                let state = app_handle.state::<ManagedState>();
                                if let Ok(mut guard) = state.lock() {
                                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                                        p.x = pos.x as f64;
                                        p.y = pos.y as f64;
                                    }
                                };
                            }
                            tauri::WindowEvent::Resized(size) => {
                                let state = app_handle.state::<ManagedState>();
                                if let Ok(mut guard) = state.lock() {
                                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                                        p.width = size.width as f64;
                                        p.height = size.height as f64;
                                    }
                                };
                            }
                            _ => {}
                        }
                    });
                    crate::persistence::auto_save(&app);
                    println!("[WisdomBoard] 面板 {} 已建立", label);
                    let _ = app.emit("panel-created", serde_json::json!({
                        "label": &label, "type": "capture", "url": null
                    }));
                }
                Err(e) => {
                    println!("[WisdomBoard] 面板 build() FAILED: {e}");
                    let state = app.state::<ManagedState>();
                    if let Ok(mut guard) = state.lock() {
                        guard.panels.remove(&label);
                    };
                }
            }
        }
    });

    println!("[WisdomBoard] 面板 {} 建立中", label);
    Ok(label)
}

#[tauri::command]
pub fn set_panel_mode(app: AppHandle, label: String, mode: String) -> Result<(), String> {
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let _ = app.emit_to(&label, "mode-changed", &mode);

    // 判斷面板類型
    let is_url = {
        let state = app.state::<ManagedState>();
        state.lock().ok()
            .and_then(|g| g.panels.get(&label).map(|p| p.panel_type == PanelType::Url))
            .unwrap_or(false)
    };

    if mode == "locked" {
        // 鎖定：置底 + 不可調整 + 忽略滑鼠（WS_EX_TRANSPARENT 讓點擊穿透）
        let _ = window.set_always_on_top(false);
        let _ = window.set_resizable(false);
        if let Ok(raw) = window.hwnd() {
            let hwnd = HWND(raw.0 as isize);
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::*;
                let _ = SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongW(hwnd, GWL_EXSTYLE, (ex | WS_EX_TRANSPARENT.0) as i32);
            }
        }
        // URL 面板：移除 drag overlay
        if is_url {
            let _ = window.eval(
                "var d=document.getElementById('wb-drag-overlay'); if(d) d.style.display='none';"
            );
        }
    } else {
        // 編輯：置頂 + 可拖移可調整
        let _ = window.set_always_on_top(true);
        let _ = window.set_resizable(true);
        if let Ok(raw) = window.hwnd() {
            let hwnd = HWND(raw.0 as isize);
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::*;
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongW(hwnd, GWL_EXSTYLE, (ex & !WS_EX_TRANSPARENT.0) as i32);
            }
        }
        let _ = window.show();
        let _ = window.set_focus();

        // URL 面板：注入全屏 drag overlay 讓整個面板可拖移
        if is_url {
            let _ = window.eval(
                "(() => {\
                   var d = document.getElementById('wb-drag-overlay');\
                   if (!d) {\
                     d = document.createElement('div');\
                     d.id = 'wb-drag-overlay';\
                     d.style.cssText = 'position:fixed;inset:0;z-index:99999;cursor:move;-webkit-app-region:drag;background:rgba(137,180,250,0.08);';\
                     document.documentElement.appendChild(d);\
                   } else { d.style.display = 'block'; }\
                 })();"
            );
        }
    }

    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            if let Some(p) = guard.panels.get_mut(&label) {
                p.mode = mode.clone();
            }
        };
    }
    crate::persistence::auto_save(&app);
    Ok(())
}

#[tauri::command]
pub fn set_panel_zoom(app: AppHandle, label: String, zoom: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let js = format!(
        "document.documentElement.style.transform = 'scale({z})'; \
         document.documentElement.style.transformOrigin = 'top left'; \
         document.documentElement.style.width = '{w}%'; \
         document.documentElement.style.height = '{h}%'; \
         document.documentElement.style.overflow = 'hidden';",
        z = zoom,
        w = 100.0 / zoom,
        h = 100.0 / zoom,
    );
    let _ = window.eval(&js);

    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            if let Some(p) = guard.panels.get_mut(&label) {
                p.zoom = zoom;
            }
        };
    }
    crate::persistence::auto_save(&app);
    Ok(())
}

#[tauri::command]
pub fn get_panel_screenshot(app: AppHandle, label: String) -> Option<String> {
    let state = app.state::<ManagedState>();
    let guard = state.lock().ok()?;
    guard.panels.get(&label)?.screenshot_path.clone()
}

#[tauri::command]
pub fn close_panel(app: AppHandle, label: String) -> Result<(), String> {
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    window.close().map_err(|e| format!("{e}"))
}

#[tauri::command]
pub fn focus_panel(app: AppHandle, label: String) -> Result<(), String> {
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let _ = window.show();
    window.set_focus().map_err(|e| format!("{e}"))
}

#[tauri::command]
pub fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let labels: Vec<String> = app.webview_windows()
        .into_keys()
        .filter(|l| l.starts_with("panel-"))
        .collect();
    for label in labels {
        let _ = set_panel_mode(app.clone(), label, mode.clone());
    }
    Ok(())
}

/// 從持久化設定恢復面板（啟動時呼叫）
pub fn restore_panels(app: &AppHandle, configs: Vec<PanelConfig>) {
    for config in configs {
        let result = match config.panel_type {
            PanelType::Url => {
                if let Some(ref url) = config.url {
                    restore_url_panel(app, &config, url)
                } else {
                    continue;
                }
            }
            PanelType::Capture => restore_capture_panel(app, &config),
        };

        match result {
            Ok(label) => {
                println!("[WisdomBoard] 已恢復面板: {}", label);
                // 恢復面板的模式
                if config.mode == "locked" {
                    let a = app.clone();
                    let l = label;
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let _ = set_panel_mode(a, l, "locked".into());
                    });
                }
            }
            Err(e) => eprintln!("[WisdomBoard] 恢復面板失敗: {e}"),
        }
    }
}

fn restore_url_panel(app: &AppHandle, config: &PanelConfig, url: &str) -> Result<String, String> {
    let label = config.label.clone(); // 使用原始 label 而非 next_panel_id()
    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| format!("URL 解析失敗: {e}"))?;
    let webview_url = tauri::WebviewUrl::External(parsed_url);

    let builder = tauri::WebviewWindowBuilder::new(app, &label, webview_url)
        .title(format!("WisdomBoard - {}", url))
        .inner_size(config.width, config.height)
        .position(config.x, config.y)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(false)
        .on_navigation(|_url| true);

    let win = builder.build().map_err(|e| format!("{e}"))?;
    set_square_corners(&win);

    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            guard.panels.insert(label.clone(), PanelConfig {
                label: label.clone(),
                url: Some(url.to_string()),
                ..config.clone()
            });
        };
    }

    let app_handle = app.clone();
    let panel_label = label.clone();
    win.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::Destroyed => {
                {
                    let state = app_handle.state::<ManagedState>();
                    if let Ok(mut guard) = state.lock() {
                        guard.panels.remove(&panel_label);
                    };
                }
                crate::persistence::auto_save(&app_handle);
                let _ = app_handle.emit("panel-closed", &panel_label);
            }
            tauri::WindowEvent::Moved(pos) => {
                let state = app_handle.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                        p.x = pos.x as f64;
                        p.y = pos.y as f64;
                    }
                };
            }
            tauri::WindowEvent::Resized(size) => {
                let state = app_handle.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                        p.width = size.width as f64;
                        p.height = size.height as f64;
                    }
                };
            }
            _ => {}
        }
    });

    Ok(label)
}

fn restore_capture_panel(app: &AppHandle, config: &PanelConfig) -> Result<String, String> {
    let label = config.label.clone(); // 使用原始 label
    let url = tauri::WebviewUrl::App("src/panel.html".into());
    let builder = tauri::WebviewWindowBuilder::new(app, &label, url)
        .title("WisdomBoard Capture".to_string())
        .inner_size(config.width, config.height)
        .position(config.x, config.y)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(false);

    let win = builder.build().map_err(|e| format!("{e}"))?;
    set_square_corners(&win);

    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            guard.panels.insert(label.clone(), PanelConfig {
                label: label.clone(),
                ..config.clone()
            });
        };
    }

    let app_handle = app.clone();
    let panel_label = label.clone();
    win.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::Destroyed => {
                {
                    let state = app_handle.state::<ManagedState>();
                    if let Ok(mut guard) = state.lock() {
                        guard.panels.remove(&panel_label);
                    };
                }
                crate::persistence::auto_save(&app_handle);
                let _ = app_handle.emit("panel-closed", &panel_label);
            }
            tauri::WindowEvent::Moved(pos) => {
                let state = app_handle.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                        p.x = pos.x as f64;
                        p.y = pos.y as f64;
                    }
                };
            }
            tauri::WindowEvent::Resized(size) => {
                let state = app_handle.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    if let Some(p) = guard.panels.get_mut(&panel_label) {
                        p.width = size.width as f64;
                        p.height = size.height as f64;
                    }
                };
            }
            _ => {}
        }
    });

    Ok(label)
}
