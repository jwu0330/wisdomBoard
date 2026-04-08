// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

use std::sync::atomic::{AtomicIsize, Ordering};
use tauri::{Manager, Runtime, WebviewWindow};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, SendMessageTimeoutW, SetParent, SMTO_NORMAL,
};

static WORKERW_HWND: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, _: LPARAM) -> BOOL {
    let p = FindWindowExW(hwnd, HWND(0), w!("SHELLDLL_DefView"), PCWSTR::null());
    if p != HWND(0) {
        // 找到有 SHELLDLL_DefView 的 WorkerW 的下一個 WorkerW 就是我們要的桌面圖層
        let workerw = FindWindowExW(HWND(0), hwnd, w!("WorkerW"), PCWSTR::null());
        if workerw != HWND(0) {
            WORKERW_HWND.store(workerw.0 as isize, Ordering::SeqCst);
        }
    }
    BOOL(1) // 繼續遍歷
}

pub fn pin_to_desktop<R: Runtime>(window: &WebviewWindow<R>) {
    let tauri_hwnd = window.hwnd().unwrap().0 as isize;
    let tauri_hwnd = HWND(tauri_hwnd);

    unsafe {
        // 1. 找到 Progman
        let progman = FindWindowW(w!("Progman"), PCWSTR::null());

        // 2. 發送 0x052C 訊息給 Progman，強迫生成 WorkerW 圖層
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            None,
        );

        // 3. 遍歷所有的 WorkerW 以尋找剛剛生成的目標層級
        EnumWindows(Some(enum_windows_proc), LPARAM(0));

        // 4. 掛載 Tauri 視窗到找到的 WorkerW 下，如果沒找到就退回掛在 Progman
        let worker_isize = WORKERW_HWND.load(Ordering::SeqCst);
        let target_hwnd = if worker_isize != 0 {
            HWND(worker_isize)
        } else {
            progman
        };

        SetParent(tauri_hwnd, target_hwnd);
    }
}

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![])))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 在啟動時將主視窗掛載到桌面底層
            let main_window = app.get_webview_window("main").unwrap();
            pin_to_desktop(&main_window);

            // 建立系統匣選單 (System Tray)
            let refresh_i = MenuItem::with_id(app, "refresh", "重整畫面", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "離開", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_i, &quit_i])?;

            let mut tray_builder = TrayIconBuilder::new().menu(&menu);
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "refresh" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.eval("window.location.reload();");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
