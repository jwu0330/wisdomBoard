use crate::state::ManagedState;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_RBUTTONDOWN, WM_RBUTTONUP,
};

fn make_lparam(x: i32, y: i32) -> LPARAM {
    LPARAM(((y & 0xFFFF) << 16 | (x & 0xFFFF)) as isize)
}

/// 將面板座標映射到目標視窗的 client 座標
fn map_coordinates(
    panel_x: f64,
    panel_y: f64,
    panel_w: f64,
    panel_h: f64,
    source_rect: &[i32; 4],
) -> (i32, i32) {
    let ratio_x = panel_x / panel_w;
    let ratio_y = panel_y / panel_h;
    let target_x = source_rect[0] + (ratio_x * source_rect[2] as f64) as i32;
    let target_y = source_rect[1] + (ratio_y * source_rect[3] as f64) as i32;
    (target_x, target_y)
}

/// Tauri command: 轉發滑鼠/鍵盤事件到目標視窗
#[tauri::command]
pub fn forward_input(
    app: AppHandle,
    label: String,
    event_type: String,
    x: f64,
    y: f64,
    panel_width: f64,
    panel_height: f64,
    key_code: Option<u32>,
) -> Result<(), String> {
    let state = app.state::<ManagedState>();
    let guard = state.lock().map_err(|e| format!("{e}"))?;

    let panel = guard
        .panels
        .get(&label)
        .ok_or_else(|| format!("找不到面板: {}", label))?;

    if panel.mode != "interact" {
        return Ok(()); // 非操作模式，忽略
    }

    let target_hwnd = panel
        .target_hwnd
        .ok_or("此面板無目標視窗")?;
    let source_rect = panel
        .source_rect
        .as_ref()
        .ok_or("此面板無來源區域資訊")?;

    let (tx, ty) = map_coordinates(x, y, panel_width, panel_height, source_rect);
    let hwnd = HWND(target_hwnd);

    unsafe {
        let result = match event_type.as_str() {
            "mousemove" => PostMessageW(hwnd, WM_MOUSEMOVE, WPARAM(0), make_lparam(tx, ty)),
            "mousedown" => PostMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(0x0001), make_lparam(tx, ty)),
            "mouseup" => PostMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), make_lparam(tx, ty)),
            "contextmenu" => PostMessageW(hwnd, WM_RBUTTONDOWN, WPARAM(0x0002), make_lparam(tx, ty)),
            "contextmenuup" => PostMessageW(hwnd, WM_RBUTTONUP, WPARAM(0), make_lparam(tx, ty)),
            "keydown" => {
                let vk = key_code.unwrap_or(0);
                PostMessageW(hwnd, WM_KEYDOWN, WPARAM(vk as usize), LPARAM(0))
            }
            "keyup" => {
                let vk = key_code.unwrap_or(0);
                PostMessageW(hwnd, WM_KEYUP, WPARAM(vk as usize), LPARAM(0))
            }
            _ => return Ok(()),
        };

        if let Err(e) = result {
            eprintln!("[WisdomBoard] 輸入轉發失敗: {e}");
        }
    }

    Ok(())
}
