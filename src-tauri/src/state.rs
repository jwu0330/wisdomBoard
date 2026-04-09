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
    /// 保留：未來 DWM Thumbnail 功能的目標視窗 HWND
    #[serde(skip)]
    pub target_hwnd: Option<isize>,
    /// 保留：未來 DWM Thumbnail 功能的來源區域 [x, y, w, h]
    #[serde(skip)]
    pub source_rect: Option<[i32; 4]>,
    /// 擷取面板的截圖 BMP 檔案路徑（僅執行期暫存，不持久化）
    #[serde(skip_serializing, default)]
    pub screenshot_path: Option<String>,
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
    #[serde(default = "default_version")]
    pub version: u32,
    pub panels: Vec<PanelConfig>,
    pub hotkey: HotkeyConfig,
    #[serde(default = "default_autostart")]
    pub autostart: bool,
}

fn default_version() -> u32 { 1 }
fn default_autostart() -> bool { true }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            panels: Vec::new(),
            hotkey: HotkeyConfig::default(),
            autostart: true,
        }
    }
}

pub struct AppState {
    pub panels: HashMap<String, PanelConfig>,
    pub hotkey: HotkeyConfig,
    /// 快捷鍵監聽執行緒 ID（用於 PostThreadMessage）
    pub hotkey_thread_id: Option<u32>,
    pub autostart: bool,
    /// 框選 overlay 用的截圖路徑（在 overlay 開啟前截好，overlay JS 讀取此值）
    pub screenshot_path: Option<String>,
    /// 截圖前偵測到的前景視窗 URL（瀏覽器網址列）
    pub detected_url: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            panels: HashMap::new(),
            hotkey: HotkeyConfig::default(),
            hotkey_thread_id: None,
            autostart: true,
            screenshot_path: None,
            detected_url: None,
        }
    }
}

/// Tauri managed state wrapper
pub type ManagedState = Mutex<AppState>;
