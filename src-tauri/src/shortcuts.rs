use std::str::FromStr;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub fn apply(app: &AppHandle, value: &str) -> Result<(), String> {
    let shortcut = Shortcut::from_str(value).map_err(|error| format!("단축키 형식이 올바르지 않습니다: {error}"))?;
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| error.to_string())?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|error| format!("다른 앱이 사용 중인 단축키이거나 등록할 수 없습니다: {error}"))
}
