use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};

use crate::models::AppSettings;

pub fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join("settings.json"))
}

pub fn load(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &AppSettings) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("설정 폴더를 만들 수 없습니다")?;
    }
    let data = serde_json::to_vec_pretty(settings)?;
    fs::write(path, data).context("설정 파일을 저장할 수 없습니다")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{load, save};
    use crate::models::AppSettings;

    #[test]
    fn settings_round_trip_preserves_device_ids_shortcut_and_autostart() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/settings.json");
        let expected = AppSettings {
            headset_device_id: Some("headset-id".into()),
            speaker_device_id: Some("speaker-id".into()),
            shortcut: "Ctrl+Shift+F10".into(),
            autostart: true,
        };

        save(&path, &expected).unwrap();

        assert_eq!(load(&path), expected);
    }

    #[test]
    fn missing_or_invalid_settings_fall_back_to_defaults() {
        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load(&missing), AppSettings::default());

        let invalid = directory.path().join("invalid.json");
        std::fs::write(&invalid, b"not json").unwrap();
        assert_eq!(load(&invalid), AppSettings::default());
    }
}
