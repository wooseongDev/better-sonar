use std::str::FromStr;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

#[cfg(any(windows, test))]
use crate::sonar_client::PersonalVolumeAction;

#[cfg(windows)]
use std::time::Duration;
#[cfg(any(windows, test))]
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[cfg(windows)]
use tauri::{Emitter, Manager};

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

pub fn register_user(app: &AppHandle, value: &str) -> Result<(), String> {
    let shortcut = parse_user(value)?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|error| format!("다른 앱이 사용 중인 단축키이거나 등록할 수 없습니다: {error}"))
}

pub fn replace_user(app: &AppHandle, old_value: &str, new_value: &str) -> Result<(), String> {
    let old = parse_user(old_value)?;
    let new = parse_user(new_value)?;
    if old == new {
        return Ok(());
    }

    app.global_shortcut()
        .register(new)
        .map_err(|error| format!("다른 앱이 사용 중인 단축키이거나 등록할 수 없습니다: {error}"))?;
    if let Err(error) = app.global_shortcut().unregister(old) {
        let _ = app.global_shortcut().unregister(new);
        return Err(format!("이전 단축키 등록을 해제하지 못했습니다: {error}"));
    }
    Ok(())
}

pub fn handle_event(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    #[cfg(not(windows))]
    let _ = shortcut;
    #[cfg(windows)]
    if let Some(action) = media_action(shortcut.key) {
        handle_media_event(app, action, event.state());
        return;
    }

    if event.state() == ShortcutState::Pressed {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::commands::toggle_and_publish(&app).await;
        });
    }
}

#[cfg(any(windows, test))]
fn media_action(key: Code) -> Option<PersonalVolumeAction> {
    match key {
        Code::AudioVolumeMute => Some(PersonalVolumeAction::ToggleMute),
        Code::AudioVolumeDown => Some(PersonalVolumeAction::StepDown),
        Code::AudioVolumeUp => Some(PersonalVolumeAction::StepUp),
        _ => None,
    }
}

#[cfg(windows)]
const MEDIA_SHORTCUTS: [(Code, &str); 3] = [
    (Code::AudioVolumeMute, "AudioVolumeMute"),
    (Code::AudioVolumeDown, "AudioVolumeDown"),
    (Code::AudioVolumeUp, "AudioVolumeUp"),
];

#[cfg(windows)]
pub fn register_media_keys(app: &AppHandle) -> Result<(), String> {
    let mut registered = Vec::new();
    for (key, name) in MEDIA_SHORTCUTS {
        let shortcut = Shortcut::new(None, key);
        match app.global_shortcut().register(shortcut) {
            Ok(()) => {
                registered.push(shortcut);
                eprintln!("[media-key] {name} 전역 등록 완료");
            }
            Err(error) => {
                for registered_shortcut in registered {
                    let _ = app.global_shortcut().unregister(registered_shortcut);
                }
                return Err(format!(
                    "{name} 미디어 키를 등록하지 못했습니다. 다른 앱과의 글로벌 키 충돌일 수 있습니다: {error}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn unregister_media_keys(app: &AppHandle) -> Result<(), String> {
    let mut unregistered = Vec::new();
    for (key, name) in MEDIA_SHORTCUTS {
        let shortcut = Shortcut::new(None, key);
        if let Err(error) = app.global_shortcut().unregister(shortcut) {
            for unregistered_shortcut in unregistered {
                let _ = app.global_shortcut().register(unregistered_shortcut);
            }
            return Err(format!("{name} 미디어 키 등록을 해제하지 못했습니다: {error}"));
        }
        unregistered.push(shortcut);
        eprintln!("[media-key] {name} 전역 등록 해제");
    }
    Ok(())
}

#[cfg(windows)]
pub fn replace_media_keys(app: &AppHandle, old_enabled: bool, new_enabled: bool) -> Result<(), String> {
    if old_enabled == new_enabled {
        return Ok(());
    }
    if new_enabled {
        register_media_keys(app)
    } else {
        unregister_media_keys(app)
    }
}

#[cfg(any(windows, test))]
#[derive(Default)]
pub struct MediaKeyRepeatState {
    active: Mutex<HashMap<PersonalVolumeAction, Arc<AtomicBool>>>,
}

#[cfg(any(windows, test))]
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

#[cfg(windows)]
fn handle_media_event(app: &AppHandle, action: PersonalVolumeAction, state: ShortcutState) {
    eprintln!("[media-key] {action:?} {state:?} 수신");
    let repeats = app.state::<MediaKeyRepeatState>();
    if state == ShortcutState::Released {
        repeats.stop(action);
        return;
    }

    if action == PersonalVolumeAction::ToggleMute {
        spawn_volume_action(app.clone(), action);
        return;
    }

    let Some(held) = repeats.start(action) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if !run_volume_action(&app, action).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(350)).await;
        while held.load(Ordering::Acquire) {
            if !run_volume_action(&app, action).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(90)).await;
        }
    });
}

#[cfg(windows)]
fn spawn_volume_action(app: AppHandle, action: PersonalVolumeAction) {
    tauri::async_runtime::spawn(async move {
        run_volume_action(&app, action).await;
    });
}

#[cfg(windows)]
async fn run_volume_action(app: &AppHandle, action: PersonalVolumeAction) -> bool {
    let runtime = app.state::<Arc<crate::state::AppRuntime>>();
    match runtime.control_personal_volume(action).await {
        Ok(()) => true,
        Err(error) => {
            let message = format!("Personal Mix 음량을 조절하지 못했습니다: {error}");
            eprintln!("[media-key] {message}");
            let _ = app.emit("sonar-error", message);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::{MediaKeyRepeatState, PersonalVolumeAction, is_media_key, media_action, parse_user};
    use tauri_plugin_global_shortcut::Code;

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
