use crate::state::{AppConfig, AppState, ManagedState};
use tauri::{AppHandle, Manager};

fn config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("無法取得 app data 目錄: {e}"))?;
    Ok(dir.join("config.json"))
}

pub fn save(app: &AppHandle, state: &AppState) {
    let config = AppConfig {
        version: 1,
        panels: state.panels.values().cloned().collect(),
        hotkey: state.hotkey.clone(),
        autostart: state.autostart,
    };

    let path = match config_path(app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[WisdomBoard] 儲存設定失敗: {e}");
            return;
        }
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let tmp = path.with_extension("json.tmp");
    match serde_json::to_string_pretty(&config) {
        Ok(json) => {
            if std::fs::write(&tmp, &json).is_ok() {
                if let Err(e) = std::fs::rename(&tmp, &path) {
                    eprintln!("[WisdomBoard] 設定檔替換失敗: {e}");
                    let _ = std::fs::remove_file(&tmp);
                } else {
                    println!("[WisdomBoard] 設定已儲存 ({} 個面板)", config.panels.len());
                }
            } else {
                eprintln!("[WisdomBoard] 寫入臨時設定檔失敗");
            }
        }
        Err(e) => eprintln!("[WisdomBoard] 序列化設定失敗: {e}"),
    }
}

pub fn load(app: &AppHandle) -> Option<AppConfig> {
    let path = config_path(app).ok()?;
    let tmp = path.with_extension("json.tmp");

    // 嘗試主設定檔
    if let Ok(data) = std::fs::read_to_string(&path) {
        match serde_json::from_str::<AppConfig>(&data) {
            Ok(config) => {
                println!(
                    "[WisdomBoard] 已載入設定 ({} 個面板)",
                    config.panels.len()
                );
                return Some(config);
            }
            Err(e) => {
                eprintln!("[WisdomBoard] 設定檔解析失敗: {e}，嘗試備份");
            }
        }
    }

    // 嘗試 tmp 備份
    if let Ok(data) = std::fs::read_to_string(&tmp) {
        match serde_json::from_str::<AppConfig>(&data) {
            Ok(config) => {
                println!(
                    "[WisdomBoard] 已從備份載入設定 ({} 個面板)",
                    config.panels.len()
                );
                return Some(config);
            }
            Err(e) => {
                eprintln!("[WisdomBoard] 備份設定檔解析失敗: {e}");
            }
        }
    }

    None
}

/// 便捷函式：鎖定 state 後自動儲存
pub fn auto_save(app: &AppHandle) {
    let state = app.state::<ManagedState>();
    if let Ok(guard) = state.lock() {
        save(app, &*guard);
    };
}
