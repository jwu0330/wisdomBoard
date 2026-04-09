use crate::state::{ManagedState, PanelConfig, PanelType};
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{AppHandle, Emitter, Manager};

static PANEL_COUNT: AtomicI32 = AtomicI32::new(0);

pub fn next_panel_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32;
    let seq = PANEL_COUNT.fetch_add(1, Ordering::SeqCst);
    format!("panel-{}-{}", ts % 100000, seq)
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
                .unwrap_or(("capture", None, "view".to_string(), 1.0));
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

    let target_url = url.clone();
    let webview_url = tauri::WebviewUrl::App("src/webpanel.html".into());
    let navigated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let safe_url_js = serde_json::to_string(&target_url)
        .unwrap_or_else(|_| {
            let escaped = target_url
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            format!("\"{}\"", escaped)
        });

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
                    mode: "view".into(),
                    zoom: 1.0,
                    target_hwnd: None,
                    source_rect: None,
                },
            );
        };
    }

    let builder = tauri::WebviewWindowBuilder::new(&app, &label, webview_url)
        .title(format!("WisdomBoard - {}", url))
        .inner_size(800.0, 600.0)
        .decorations(true)
        .always_on_top(false)
        .skip_taskbar(false)
        .transparent(false)
        .on_navigation(|_url| true)
        .on_page_load({
            let navigated = navigated.clone();
            move |wv, payload| {
                if let tauri::webview::PageLoadEvent::Finished = payload.event() {
                    if !navigated.swap(true, Ordering::SeqCst) {
                        let js = format!("window.location.href = {};", safe_url_js);
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
            Ok(label)
        }
        Err(e) => {
            // 建立失敗，移除預佔
            let state = app.state::<ManagedState>();
            if let Ok(mut guard) = state.lock() {
                guard.panels.remove(&label);
            }
            Err(format!("{e}"))
        }
    }
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
                    mode: "view".into(),
                    zoom: 1.0,
                    target_hwnd: None,
                    source_rect: None,
                },
            );
        };
    }

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
            let app_handle = app.clone();
            let panel_label = label.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Destroyed = event {
                    {
                        let state = app_handle.state::<ManagedState>();
                        if let Ok(mut guard) = state.lock() {
                            guard.panels.remove(&panel_label);
                        };
                    }
                    crate::persistence::auto_save(&app_handle);
                    let _ = app_handle.emit("panel-closed", &panel_label);
                }
            });

            crate::persistence::auto_save(&app);
            println!("[WisdomBoard] 面板 {} 已建立", label);
            Ok(label)
        }
        Err(e) => {
            // 建立失敗，移除預佔
            let state = app.state::<ManagedState>();
            if let Ok(mut guard) = state.lock() {
                guard.panels.remove(&label);
            }
            Err(format!("{e}"))
        }
    }
}

#[tauri::command]
pub fn set_panel_mode(app: AppHandle, label: String, mode: String) -> Result<(), String> {
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;
    let _ = app.emit_to(&label, "mode-changed", &mode);
    let _ = window.set_resizable(mode == "resize");

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
    for (label, window) in app.webview_windows() {
        if label.starts_with("panel-") {
            let _ = window.emit("mode-changed", &mode);
        }
    }

    {
        let state = app.state::<ManagedState>();
        if let Ok(mut guard) = state.lock() {
            // 只更新仍存在的面板
            let existing_labels: Vec<String> = app.webview_windows()
                .into_keys()
                .filter(|l| l.starts_with("panel-"))
                .collect();
            for label in &existing_labels {
                if let Some(p) = guard.panels.get_mut(label) {
                    p.mode = mode.clone();
                }
            }
        };
    }
    crate::persistence::auto_save(&app);
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
            Ok(label) => println!("[WisdomBoard] 已恢復面板: {}", label),
            Err(e) => eprintln!("[WisdomBoard] 恢復面板失敗: {e}"),
        }
    }
}

fn restore_url_panel(app: &AppHandle, config: &PanelConfig, url: &str) -> Result<String, String> {
    let label = config.label.clone(); // 使用原始 label 而非 next_panel_id()
    let target_url = url.to_string();
    let webview_url = tauri::WebviewUrl::App("src/webpanel.html".into());
    let navigated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let safe_url_js = serde_json::to_string(&target_url)
        .unwrap_or_else(|_| {
            let escaped = target_url
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            format!("\"{}\"", escaped)
        });

    let builder = tauri::WebviewWindowBuilder::new(app, &label, webview_url)
        .title(format!("WisdomBoard - {}", url))
        .inner_size(config.width, config.height)
        .position(config.x, config.y)
        .decorations(true)
        .always_on_top(false)
        .skip_taskbar(false)
        .transparent(false)
        .on_navigation(|_url| true)
        .on_page_load({
            let navigated = navigated.clone();
            move |wv, payload| {
                if let tauri::webview::PageLoadEvent::Finished = payload.event() {
                    if !navigated.swap(true, Ordering::SeqCst) {
                        let js = format!("window.location.href = {};", safe_url_js);
                        let _ = wv.eval(&js);
                    }
                }
            }
        });

    let win = builder.build().map_err(|e| format!("{e}"))?;

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

    // 嘗試重新註冊 DWM thumbnail（目標視窗可能已關閉）
    if let Some(thwnd) = config.target_hwnd {
        use windows::Win32::Foundation::HWND;
        let target = HWND(thwnd);
        // 檢查目標視窗是否仍存在
        let is_valid = unsafe {
            windows::Win32::UI::WindowsAndMessaging::IsWindow(target).as_bool()
        };
        if is_valid {
            crate::capture::register_dwm_thumbnail_pub(
                app, &label, &win, target,
                config.width as i32, config.height as i32,
                config.source_rect.as_ref(),
            );
        }
    }

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
                        if let Some(thumb_id) = guard.dwm_thumbnails.remove(&panel_label) {
                            unsafe {
                                let _ = windows::Win32::Graphics::Dwm::DwmUnregisterThumbnail(thumb_id);
                            }
                        }
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
                    if let Some(&thumb_id) = guard.dwm_thumbnails.get(&panel_label) {
                        unsafe {
                            let mut props: windows::Win32::Graphics::Dwm::DWM_THUMBNAIL_PROPERTIES = std::mem::zeroed();
                            // DWM_TNP_RECTDESTINATION = 0x01
                            props.dwFlags = 0x01_u32;
                            props.rcDestination = windows::Win32::Foundation::RECT {
                                left: 0, top: 0,
                                right: size.width as i32,
                                bottom: size.height as i32,
                            };
                            let _ = windows::Win32::Graphics::Dwm::DwmUpdateThumbnailProperties(thumb_id, &props);
                        }
                    }
                };
            }
            _ => {}
        }
    });

    Ok(label)
}
