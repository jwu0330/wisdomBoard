use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PanelType {
    Url,
    Capture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelConfig {
    pub label: String,
    pub panel_type: PanelType,
    pub url: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub mode: String,
    pub zoom: f64,
    /// 擷取面板的目標視窗 HWND（用於 DWM thumbnail 和輸入轉發）
    pub target_hwnd: Option<isize>,
    /// 目標視窗中的擷取區域 [x, y, w, h]（物理像素）
    pub source_rect: Option<[i32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifiers: u32,
    pub vk: u32,
    pub display_name: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        // Ctrl+Alt+S
        Self {
            modifiers: 0x0001 | 0x0002, // MOD_ALT | MOD_CONTROL
            vk: 0x53,                    // 'S'
            display_name: "Ctrl+Alt+S".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub panels: Vec<PanelConfig>,
    pub hotkey: HotkeyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            panels: Vec::new(),
            hotkey: HotkeyConfig::default(),
        }
    }
}

pub struct AppState {
    pub panels: HashMap<String, PanelConfig>,
    pub hotkey: HotkeyConfig,
    /// DWM thumbnail handles: panel label -> thumbnail id
    pub dwm_thumbnails: HashMap<String, isize>,
    /// 快捷鍵監聽執行緒 ID（用於 PostThreadMessage）
    pub hotkey_thread_id: Option<u32>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            panels: HashMap::new(),
            hotkey: HotkeyConfig::default(),
            dwm_thumbnails: HashMap::new(),
            hotkey_thread_id: None,
        }
    }
}

/// Tauri managed state wrapper
pub type ManagedState = Mutex<AppState>;
