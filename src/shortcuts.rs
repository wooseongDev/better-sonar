use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey as Shortcut},
};

use crate::{app::Event, sonar_client::PersonalVolumeAction, state::AppRuntime};
fn parse_user(value: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(value).map_err(|error| format!("단축키 형식이 올바르지 않습니다: {error}"))?;
    if is_media_key(shortcut.key) {
        return Err("음량 미디어 키는 Personal Mix 음량 제어용으로 예약되어 있습니다".into());
    }
    Ok(shortcut)
}

const fn is_media_key(key: Code) -> bool {
    matches!(key, Code::AudioVolumeMute | Code::AudioVolumeDown | Code::AudioVolumeUp)
}

fn media_action(key: Code) -> Option<PersonalVolumeAction> {
    match key {
        Code::AudioVolumeMute => Some(PersonalVolumeAction::ToggleMute),
        Code::AudioVolumeDown => Some(PersonalVolumeAction::StepDown),
        Code::AudioVolumeUp => Some(PersonalVolumeAction::StepUp),
        _ => None,
    }
}

#[derive(Default)]
pub struct MediaKeyRepeatState {
    active: Mutex<HashMap<PersonalVolumeAction, Arc<AtomicBool>>>,
}

impl MediaKeyRepeatState {
    fn start(&self, action: PersonalVolumeAction) -> Option<Arc<AtomicBool>> {
        let mut active = self.active.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.contains_key(&action) {
            return None;
        }
        let held = Arc::new(AtomicBool::new(true));
        active.insert(action, held.clone());
        Some(held)
    }

    fn stop(&self, action: PersonalVolumeAction) {
        let mut active = self.active.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(held) = active.remove(&action) {
            held.store(false, Ordering::Release);
        }
    }
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    user: Option<Shortcut>,
    media: HashMap<u32, Shortcut>,
    repeats: MediaKeyRepeatState,
}

impl Hotkeys {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            user: None,
            media: HashMap::new(),
            repeats: MediaKeyRepeatState::default(),
        })
    }

    pub fn set_user(&mut self, value: &str) -> Result<(), String> {
        let next = parse_user(value)?;
        if self.user == Some(next) {
            return Ok(());
        }

        self.manager
            .register(next)
            .map_err(|error| format!("단축키를 등록할 수 없습니다: {error}"))?;
        if let Some(old) = self.user
            && let Err(error) = self.manager.unregister(old)
        {
            let _ = self.manager.unregister(next);
            return Err(format!("이전 단축키를 해제할 수 없습니다: {error}"));
        }

        self.user = Some(next);
        Ok(())
    }

    pub fn set_media(&mut self, enabled: bool) -> Option<String> {
        let mut errors = Vec::new();
        for code in [Code::AudioVolumeMute, Code::AudioVolumeDown, Code::AudioVolumeUp] {
            let key = Shortcut::new(None, code);
            let id = key.id();
            match (enabled, self.media.contains_key(&id)) {
                (true, false) => match self.manager.register(key) {
                    Ok(()) => {
                        self.media.insert(id, key);
                    }
                    Err(error) => errors.push(format!("{code:?}: {error}")),
                },
                (false, true) => match self.manager.unregister(key) {
                    Ok(()) => {
                        self.media.remove(&id);
                        if let Some(action) = media_action(code) {
                            self.repeats.stop(action);
                        }
                    }
                    Err(error) => errors.push(format!("{code:?}: {error}")),
                },
                _ => {}
            }
        }
        (!errors.is_empty()).then(|| format!("일부 미디어 키 설정을 적용할 수 없습니다: {}", errors.join(", ")))
    }

    pub fn handle(
        &self,
        event: GlobalHotKeyEvent,
        runtime: &Arc<AppRuntime>,
        executor: &tokio::runtime::Handle,
        events: &async_channel::Sender<Event>,
    ) -> bool {
        if self.user.is_some_and(|key| key.id() == event.id) {
            return event.state() == HotKeyState::Pressed;
        }
        let Some(action) = self.media.get(&event.id).and_then(|key| media_action(key.key)) else {
            return false;
        };

        if event.state() == HotKeyState::Released {
            self.repeats.stop(action);
            return false;
        }
        let Some(held) = self.repeats.start(action) else {
            return false;
        };
        let runtime = runtime.clone();
        let events = events.clone();
        executor.spawn(async move {
            if let Err(error) = runtime.control_personal_volume(action).await {
                let _ = events.send(Event::Notice(error)).await;
                return;
            }
            if action == PersonalVolumeAction::ToggleMute {
                return;
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
            while held.load(Ordering::Acquire) {
                if let Err(error) = runtime.control_personal_volume(action).await {
                    let _ = events.send(Event::Notice(error)).await;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(90)).await;
            }
        });
        false
    }
}
#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::{MediaKeyRepeatState, PersonalVolumeAction, is_media_key, media_action, parse_user};
    use global_hotkey::hotkey::Code;

    #[test]
    fn recognizes_only_windows_volume_media_keys() {
        assert!(is_media_key(Code::AudioVolumeMute));
        assert!(is_media_key(Code::AudioVolumeDown));
        assert!(is_media_key(Code::AudioVolumeUp));
        assert!(!is_media_key(Code::F10));
        assert!(!is_media_key(Code::F11));
        assert!(!is_media_key(Code::F12));
    }

    #[test]
    fn user_shortcuts_cannot_replace_reserved_media_keys() {
        assert!(parse_user("Ctrl+Alt+F9").is_ok());
        assert!(parse_user("AudioVolumeMute").is_err());
        assert!(parse_user("AudioVolumeDown").is_err());
        assert!(parse_user("AudioVolumeUp").is_err());
    }

    #[test]
    fn routes_each_media_key_to_a_distinct_volume_action() {
        assert_eq!(
            media_action(Code::AudioVolumeMute),
            Some(PersonalVolumeAction::ToggleMute)
        );
        assert_eq!(
            media_action(Code::AudioVolumeDown),
            Some(PersonalVolumeAction::StepDown)
        );
        assert_eq!(media_action(Code::AudioVolumeUp), Some(PersonalVolumeAction::StepUp));
        assert_eq!(media_action(Code::F10), None);
        assert_eq!(media_action(Code::F11), None);
        assert_eq!(media_action(Code::F12), None);
    }

    #[test]
    fn repeat_state_deduplicates_press_and_stops_on_release() {
        let repeats = MediaKeyRepeatState::default();
        let held = repeats.start(PersonalVolumeAction::StepUp).unwrap();

        assert!(held.load(Ordering::Acquire));
        assert!(repeats.start(PersonalVolumeAction::StepUp).is_none());
        assert!(repeats.start(PersonalVolumeAction::StepDown).is_some());

        repeats.stop(PersonalVolumeAction::StepUp);
        assert!(!held.load(Ordering::Acquire));
        assert!(repeats.start(PersonalVolumeAction::StepUp).is_some());
    }
}
