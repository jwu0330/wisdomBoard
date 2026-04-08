use std::sync::atomic::{AtomicIsize, Ordering};
use tauri::{Manager, Runtime, WebviewWindow};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
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

/// 將 Tauri 視窗掛載到桌面 WorkerW 圖層（圖示之下、桌布之上）
/// 回傳 true 表示成功，false 表示失敗
pub fn pin_to_desktop<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    let tauri_hwnd = match window.hwnd() {
        Ok(hwnd) => HWND(hwnd.0 as isize),
        Err(e) => {
            eprintln!("[WisdomBoard] 無法取得視窗句柄: {e}");
            return false;
        }
    };

    unsafe {
        // 1. 找到 Progman
        let progman = FindWindowW(w!("Progman"), PCWSTR::null());
        if progman == HWND(0) {
            eprintln!("[WisdomBoard] 找不到 Progman 視窗");
            return false;
        }

        // 2. 發送 0x052C 訊息給 Progman，強迫生成 WorkerW 圖層
        // SendMessageTimeoutW 回傳 LRESULT（非 Result），0 表示逾時或失敗
        let send_result = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            None,
        );
        if send_result == LRESULT(0) {
            eprintln!("[WisdomBoard] 發送 0x052C 訊息逾時或失敗，仍嘗試繼續");
        }

        // 3. 遍歷所有的 WorkerW 以尋找剛剛生成的目標層級
        WORKERW_HWND.store(0, Ordering::SeqCst);
        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));

        // 4. 掛載 Tauri 視窗到找到的 WorkerW 下，如果沒找到就退回掛在 Progman
        let worker_isize = WORKERW_HWND.load(Ordering::SeqCst);
        let target_hwnd = if worker_isize != 0 {
            HWND(worker_isize)
        } else {
            eprintln!("[WisdomBoard] 找不到 WorkerW，退回掛載到 Progman");
            progman
        };

        // SetParent 回傳 HWND（非 Result），HWND(0) 表示失敗
        let prev_parent = SetParent(tauri_hwnd, target_hwnd);
        if prev_parent == HWND(0) {
            eprintln!("[WisdomBoard] SetParent 失敗");
            return false;
        }

        println!("[WisdomBoard] 成功掛載到桌面圖層");
        true
    }
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

            // 在啟動時將主視窗掛載到桌面底層
            if let Some(main_window) = app.get_webview_window("main") {
                pin_to_desktop(&main_window);
            } else {
                eprintln!("[WisdomBoard] 找不到 main 視窗，跳過桌面掛載");
            }

            // 建立系統匣選單 (System Tray)
            let refresh_i =
                MenuItem::with_id(app, "refresh", "重整畫面", true, None::<&str>)?;
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
