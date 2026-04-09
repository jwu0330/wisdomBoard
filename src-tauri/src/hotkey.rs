use crate::state::ManagedState;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY,
};

const HOTKEY_SNIP: i32 = 1;
/// 自訂訊息：重新註冊快捷鍵
const WM_REREGISTER_HOTKEY: u32 = WM_APP + 1;

/// 啟動快捷鍵監聽執行緒（執行緒 ID 存入 state）
pub fn start_listener(app_handle: AppHandle) {
    let app = app_handle.clone();

    std::thread::spawn(move || {
        let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };

        // 將執行緒 ID 存入 state
        {
            let state = app.state::<ManagedState>();
            if let Ok(mut guard) = state.lock() {
                guard.hotkey_thread_id = Some(thread_id);
            };
        }

        // 從 state 讀取快捷鍵設定
        let (modifiers, vk) = {
            let state = app.state::<ManagedState>();
            let x = match state.lock() {
                Ok(guard) => (guard.hotkey.modifiers, guard.hotkey.vk),
                Err(_) => (0x0001 | 0x0002, 0x53), // 預設 Ctrl+Alt+S
            }; x
        };

        unsafe {
            let mods = HOT_KEY_MODIFIERS(modifiers);
            if RegisterHotKey(HWND(0), HOTKEY_SNIP, mods, vk).is_err() {
                eprintln!("[WisdomBoard] 註冊快捷鍵失敗");
                let state = app.state::<ManagedState>();
                if let Ok(mut guard) = state.lock() {
                    guard.hotkey_thread_id = None;
                };
                return;
            }
            println!("[WisdomBoard] 全域快捷鍵已註冊 (thread {})", thread_id);

            // 追蹤目前成功註冊的快捷鍵，fallback 用
            let mut current_modifiers = modifiers;
            let mut current_vk = vk;

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND(0), 0, 0).as_bool() {
                if msg.message == WM_HOTKEY && msg.wParam == WPARAM(HOTKEY_SNIP as usize) {
                    println!("[WisdomBoard] 快捷鍵觸發，開啟設定視窗");
                    let _ = crate::open_settings(app.clone());
                }

                if msg.message == WM_REREGISTER_HOTKEY {
                    let new_mods = msg.wParam.0 as u32;
                    let new_vk = msg.lParam.0 as u32;
                    let _ = UnregisterHotKey(HWND(0), HOTKEY_SNIP);
                    if RegisterHotKey(HWND(0), HOTKEY_SNIP, HOT_KEY_MODIFIERS(new_mods), new_vk)
                        .is_err()
                    {
                        eprintln!("[WisdomBoard] 重新註冊快捷鍵失敗，恢復上次成功的快捷鍵");
                        let _ = RegisterHotKey(HWND(0), HOTKEY_SNIP, HOT_KEY_MODIFIERS(current_modifiers), current_vk);
                    } else {
                        println!("[WisdomBoard] 快捷鍵已更新");
                        current_modifiers = new_mods;
                        current_vk = new_vk;
                    }
                }
            }

            let _ = UnregisterHotKey(HWND(0), HOTKEY_SNIP);
        }
    });
}

/// Tauri command: 取得目前快捷鍵設定
#[tauri::command]
pub fn get_hotkey_config(
    state: tauri::State<'_, ManagedState>,
) -> Result<crate::state::HotkeyConfig, String> {
    let guard = state.lock().map_err(|e| format!("{e}"))?;
    Ok(guard.hotkey.clone())
}

/// Tauri command: 設定新的全域快捷鍵
#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    state: tauri::State<'_, ManagedState>,
    modifiers: u32,
    vk: u32,
    display_name: String,
) -> Result<(), String> {
    let thread_id = {
        let mut guard = state.lock().map_err(|e| format!("{e}"))?;
        guard.hotkey = crate::state::HotkeyConfig {
            modifiers,
            vk,
            display_name,
        };
        guard.hotkey_thread_id
    };

    // 通知快捷鍵執行緒重新註冊
    if let Some(tid) = thread_id {
        unsafe {
            let _ = PostThreadMessageW(
                tid,
                WM_REREGISTER_HOTKEY,
                WPARAM(modifiers as usize),
                windows::Win32::Foundation::LPARAM(vk as isize),
            );
        }
    }

    crate::persistence::auto_save(&app);
    Ok(())
}
