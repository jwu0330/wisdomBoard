mod capture;
mod hotkey;
mod panel;
mod persistence;
mod state;

use state::ManagedState;
use std::sync::Mutex;
use tauri::Manager;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt;

/// 開啟或聚焦設定視窗（供 hotkey 模組呼叫）
pub fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
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
        Ok(_) => Ok(()),
        Err(e) => Err(format!("{e}")),
    }
}

#[tauri::command]
fn get_autostart(state: tauri::State<'_, ManagedState>) -> bool {
    state.lock().map(|g| g.autostart).unwrap_or(true)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, state: tauri::State<'_, ManagedState>, enabled: bool) -> Result<(), String> {
    {
        let mut guard = state.lock().map_err(|e| format!("{e}"))?;
        guard.autostart = enabled;
    }
    let autostart_manager = app.autolaunch();
    if enabled {
        autostart_manager.enable().map_err(|e| format!("{e}"))?;
    } else {
        autostart_manager.disable().map_err(|e| format!("{e}"))?;
    }
    crate::persistence::auto_save(&app);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(state::AppState::default()) as ManagedState)
        .setup(|app| {
            // 隱藏主視窗
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.hide();
            }

            // 載入持久化設定
            let saved_config = persistence::load(&app.handle());
            if let Some(config) = &saved_config {
                let state = app.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    guard.hotkey = config.hotkey.clone();
                    guard.autostart = config.autostart;
                };
            }

            // 依設定決定是否自動開機
            let should_autostart = {
                let state = app.state::<ManagedState>();
                state.lock().map(|g| g.autostart).unwrap_or(true)
            };
            let autostart_manager = app.autolaunch();
            if should_autostart {
                if !autostart_manager.is_enabled().unwrap_or(false) {
                    if let Err(e) = autostart_manager.enable() {
                        eprintln!("[WisdomBoard] 自啟動設定失敗: {e}");
                    }
                }
            } else {
                if autostart_manager.is_enabled().unwrap_or(false) {
                    if let Err(e) = autostart_manager.disable() {
                        eprintln!("[WisdomBoard] 停用自啟動失敗: {e}");
                    }
                }
            }

            // 啟動快捷鍵監聽
            hotkey::start_listener(app.handle().clone());

            // 恢復面板
            if let Some(config) = saved_config {
                if !config.panels.is_empty() {
                    let handle = app.handle().clone();
                    // 延遲恢復，確保視窗系統就緒
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        panel::restore_panels(&handle, config.panels);
                    });
                }
            }

            // 系統匣
            let hotkey_display = {
                let state = app.state::<ManagedState>();
                state.lock()
                    .map(|g| g.hotkey.display_name.clone())
                    .unwrap_or_else(|_| "Ctrl+Alt+S".into())
            };
            let settings_i =
                MenuItem::with_id(app, "settings", format!("設定 ({})", hotkey_display), true, None::<&str>)?;
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

            println!("[WisdomBoard] v{} 已啟動", env!("CARGO_PKG_VERSION"));
            println!("[WisdomBoard] 按快捷鍵開啟設定視窗");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            panel::create_panel,
            panel::create_url_panel,
            panel::create_url_panel_at,
            panel::close_panel,
            panel::close_all_panels,
            panel::set_mode,
            panel::set_panel_mode,
            panel::set_panel_zoom,
            panel::list_panels,
            panel::get_panel_screenshot,
            capture::open_capture_overlay,
            capture::capture_region,
            capture::get_screenshot,
            capture::get_detected_url,
            capture::run_debug_tests,
            hotkey::get_hotkey_config,
            hotkey::set_hotkey,
            get_autostart,
            set_autostart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    // 只有非明確退出（如最後一個視窗關閉）才攔截，保持 tray 常駐
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => {
                    // 終止快捷鍵執行緒
                    let state = app.state::<ManagedState>();
                    if let Ok(guard) = state.lock() {
                        if let Some(tid) = guard.hotkey_thread_id {
                            unsafe {
                                use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                                use windows::Win32::Foundation::{WPARAM, LPARAM};
                                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
                            }
                        }
                    };
                }
                _ => {}
            }
        });
}
