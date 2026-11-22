use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::models::AppSettings;
use anyhow::{Context, Result};

pub fn settings_path(installed: bool) -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("Windows 사용자 설정 폴더를 찾을 수 없습니다")?;
    let folder = if installed {
        "dev.wooseong.better-sonar"
    } else {
        "dev.wooseong.better-sonar.dev"
    };
    Ok(PathBuf::from(appdata).join(folder).join("settings.json"))
}

pub fn load(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &AppSettings) -> Result<()> {
    let parent = path.parent().context("설정 폴더가 없습니다")?;
    fs::create_dir_all(parent).context("설정 폴더를 만들 수 없습니다")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(file.as_file_mut(), settings)?;
    file.persist(path).context("설정 파일을 저장할 수 없습니다")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{load, save};
    use crate::models::{AppSettings, ThemePreference};

    #[test]
    fn settings_round_trip_preserves_device_ids_shortcut_and_autostart() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested/settings.json");
        let expected = AppSettings {
            headset_device_id: Some("headset-id".into()),
            speaker_device_id: Some("speaker-id".into()),
            shortcut: "Ctrl+Shift+F10".into(),
            autostart: true,
            media_keys_enabled: false,
            theme: ThemePreference::Dark,
        };

        save(&path, &AppSettings::default()).unwrap();
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

    #[test]
    fn legacy_settings_enable_media_keys_and_follow_system_theme_by_default() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{
                "headsetDeviceId": null,
                "speakerDeviceId": null,
                "shortcut": "Ctrl+Shift+F8",
                "autostart": false
            }"#,
        )
        .unwrap();

        let settings = load(&path);
        assert!(settings.media_keys_enabled);
        assert_eq!(settings.shortcut, "Ctrl+Shift+F8");
        assert_eq!(settings.theme, ThemePreference::System);
    }
}
