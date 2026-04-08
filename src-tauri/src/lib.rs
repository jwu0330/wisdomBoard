mod capture;
mod hotkey;
mod input;
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
            // 自動啟動
            let autostart_manager = app.autolaunch();
            if !autostart_manager.is_enabled().unwrap_or(false) {
                if let Err(e) = autostart_manager.enable() {
                    eprintln!("[WisdomBoard] 自啟動設定失敗: {e}");
                }
            }

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
            let settings_i =
                MenuItem::with_id(app, "settings", "設定 (Ctrl+Alt+S)", true, None::<&str>)?;
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

            println!("[WisdomBoard] v0.3.0 已啟動");
            println!("[WisdomBoard] 按快捷鍵開啟設定視窗");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            panel::create_panel,
            panel::create_url_panel,
            panel::close_panel,
            panel::focus_panel,
            panel::set_mode,
            panel::set_panel_mode,
            panel::set_panel_zoom,
            panel::list_panels,
            capture::open_capture_overlay,
            capture::capture_region,
            capture::get_screenshot,
            capture::run_debug_tests,
            hotkey::get_hotkey_config,
            hotkey::set_hotkey,
            input::forward_input,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
