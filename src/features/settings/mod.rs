mod form;
mod storage;
mod theme;

use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use auto_launch::AutoLaunch;

use crate::{app::shortcuts::Hotkeys, windows};

pub(crate) use form::SettingsForm;
pub(crate) use storage::{AppSettings, ThemePreference};
pub(crate) use theme::{appearance_panel, apply as apply_theme};

pub(crate) struct Settings {
    path: PathBuf,
    current: AppSettings,
    autostart: AutoLaunch,
}

impl Settings {
    pub(crate) fn load(executable: &Path, installed: bool) -> Result<Self> {
        let path = storage::settings_path(installed)?;
        let autostart = windows::autostart(executable, installed)?;
        let mut initial_settings = storage::load(&path);
        if let Ok(enabled) = autostart.is_enabled() {
            if initial_settings.autostart != enabled {
                initial_settings.autostart = enabled;
                storage::save(&path, &initial_settings)?;
            }
            if enabled {
                autostart.enable()?;
            }
        }

        Ok(Self {
            path,
            current: initial_settings,
            autostart,
        })
    }

    pub(crate) fn current(&self) -> &AppSettings {
        &self.current
    }

    pub(crate) fn set_theme(&mut self, preference: ThemePreference) -> Result<bool> {
        if self.current.theme == preference {
            return Ok(false);
        }
        let mut next = self.current.clone();
        next.theme = preference;
        storage::save(&self.path, &next)?;
        self.current = next;
        Ok(true)
    }

    pub(crate) fn save(&mut self, mut settings: AppSettings, hotkeys: &mut Hotkeys) -> Result<Option<String>> {
        settings.theme = self.current.theme;
        ensure!(
            settings.headset_device_id.is_none() || settings.headset_device_id != settings.speaker_device_id,
            "헤드셋과 스피커는 서로 다른 장치여야 합니다"
        );
        let old = &self.current;
        hotkeys.set_user(&settings.shortcut).map_err(anyhow::Error::msg)?;
        let autostart_changed = settings.autostart != old.autostart;
        let result = (|| {
            if autostart_changed {
                windows::set_autostart(&self.autostart, settings.autostart)?;
            }
            storage::save(&self.path, &settings)
        })();
        if let Err(error) = result {
            let shortcut = hotkeys.set_user(&old.shortcut);
            let startup = if autostart_changed {
                windows::set_autostart(&self.autostart, old.autostart)
            } else {
                Ok(())
            };
            if shortcut.is_err() || startup.is_err() {
                anyhow::bail!("{error:#}. 이전 설정 복구에 실패했습니다. 단축키: {shortcut:?}, 자동 시작: {startup:?}");
            }
            return Err(error);
        }
        let warning = hotkeys.set_media(settings.media_keys_enabled);
        self.current = settings;
        Ok(warning)
    }
}
