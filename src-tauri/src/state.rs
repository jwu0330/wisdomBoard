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
    /// 擷取面板的截圖 BMP 檔案路徑（僅執行期暫存，不持久化）
    #[serde(skip_serializing, default)]
    pub screenshot_path: Option<String>,

    // === 螢幕綁定欄位(規格書 §5.4、§9.2)===
    // 全部用 Option + serde default,讓舊版 config.json 可直接讀取。
    /// 面板歸屬螢幕的硬體指紋,用於重啟後重新定位
    #[serde(default)]
    pub monitor_fingerprint: Option<String>,
    /// 面板左上角在歸屬螢幕內的比例位置 x (0.0~1.0)
    #[serde(default)]
    pub monitor_relative_x: Option<f64>,
    /// 面板左上角在歸屬螢幕內的比例位置 y (0.0~1.0)
    #[serde(default)]
    pub monitor_relative_y: Option<f64>,
    /// 面板是否因螢幕斷開而被遷移(規格書 §5.3)
    #[serde(default)]
    pub is_migrated: bool,
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

fn default_version() -> u32 { 2 }
fn default_autostart() -> bool { true }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            // version 2 新增螢幕綁定欄位(monitor_fingerprint 等)。
            // 舊版 v1 config.json 可無痛讀取,欄位會被 serde default 填為 None/false。
            version: 2,
            panels: Vec::new(),
            hotkey: HotkeyConfig::default(),
            autostart: true,
        }
    }
}

pub struct AppState {
    pub panels: HashMap<String, PanelConfig>,
    pub hotkey: HotkeyConfig,
    /// 快捷鍵監聯執行緒 ID（用於 PostThreadMessage）
    pub hotkey_thread_id: Option<u32>,
    pub autostart: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            panels: HashMap::new(),
            hotkey: HotkeyConfig::default(),
            hotkey_thread_id: None,
            autostart: true,
        }
    }
}

/// Overlay 暫態資料（偵測到的 URL + overlay 原點），獨立鎖以免跟高頻面板事件競爭
#[derive(Default)]
pub struct OverlayState {
    /// 截圖前偵測到的前景視窗 URL（瀏覽器網址列）
    pub detected_url: Option<String>,
    /// Overlay 視窗左上角的邏輯座標（主螢幕 scale 為基準的虛擬桌面座標）
    /// 供前端校正框選座標（多螢幕時 overlay 原點不在 (0,0)）
    pub overlay_origin_x: f64,
    pub overlay_origin_y: f64,
}

/// Tauri managed state wrappers
pub type ManagedState = Mutex<AppState>;
pub type ManagedOverlayState = Mutex<OverlayState>;
