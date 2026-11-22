use anyhow::Result;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::{app::Action, models::AppSnapshot};
const PERSONAL_PREFIX: &str = "personal-device:";
const STREAM_PREFIX: &str = "stream-device:";
const MIC_PREFIX: &str = "mic-device:";

#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuDevice {
    id: String,
    name: String,
    checked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuSignature {
    status_text: String,
    toggle_text: String,
    can_toggle: bool,
    connected: bool,
    personal_devices: Vec<MenuDevice>,
    stream_devices: Vec<MenuDevice>,
    mic_devices: Vec<MenuDevice>,
}

impl MenuSignature {
    fn from_snapshot(snapshot: Option<&AppSnapshot>) -> Self {
        let status_text = snapshot
            .and_then(|value| value.personal_device_name.as_deref())
            .map(|name| format!("현재: {name}"))
            .unwrap_or_else(|| {
                snapshot
                    .map(|value| value.message.clone())
                    .unwrap_or_else(|| "연결 상태 확인 중…".into())
            });
        let toggle_text = snapshot
            .map(|value| {
                if value.personal_device_id == value.settings.headset_device_id {
                    "스피커로 전환"
                } else {
                    "헤드셋으로 전환"
                }
            })
            .unwrap_or("헤드셋 ↔ 스피커 전환")
            .into();
        let can_toggle = snapshot.is_some_and(|value| {
            value.status == crate::models::ConnectionStatus::Connected
                && value.settings.headset_device_id.is_some()
                && value.settings.speaker_device_id.is_some()
        });
        let connected = snapshot.is_some_and(|value| value.status == crate::models::ConnectionStatus::Connected);
        let menu_devices = |devices: &[crate::models::AudioDevice], current_device_id: Option<&str>| {
            devices
                .iter()
                .filter(|device| device.state == "active")
                .map(|device| MenuDevice {
                    id: device.id.clone(),
                    name: device.name.clone(),
                    checked: current_device_id == Some(device.id.as_str()),
                })
                .collect()
        };
        let output_devices = snapshot.map(|value| value.devices.as_slice()).unwrap_or_default();

        Self {
            status_text,
            toggle_text,
            can_toggle,
            connected,
            personal_devices: menu_devices(
                output_devices,
                snapshot.and_then(|value| value.personal_device_id.as_deref()),
            ),
            stream_devices: menu_devices(
                output_devices,
                snapshot.and_then(|value| value.stream_device_id.as_deref()),
            ),
            mic_devices: menu_devices(
                snapshot.map(|value| value.input_devices.as_slice()).unwrap_or_default(),
                snapshot.and_then(|value| value.mic_device_id.as_deref()),
            ),
        }
    }
}

pub enum TrayAction {
    Show,
    Quit,
    Sonar(Action),
}

pub fn menu_action(id: &str) -> Option<TrayAction> {
    match id {
        "show" => return Some(TrayAction::Show),
        "quit" => return Some(TrayAction::Quit),
        "toggle" => return Some(TrayAction::Sonar(Action::Toggle)),
        _ => {}
    }
    let (prefix, encoded) = id.split_once(':')?;
    let device = urlencoding::decode(encoded).ok()?.into_owned();
    let action = match prefix {
        "personal-device" => Action::Personal(device),
        "stream-device" => Action::Stream(device),
        "mic-device" => Action::Mic(device),
        _ => return None,
    };
    Some(TrayAction::Sonar(action))
}

fn device_submenu(label: &str, prefix: &str, devices: &[MenuDevice], connected: bool) -> Result<Submenu> {
    let group = Submenu::new(label, connected && !devices.is_empty());
    if devices.is_empty() {
        let label = if connected {
            "사용 가능한 장치 없음"
        } else {
            "Sonar 연결 확인 중…"
        };
        group.append(&MenuItem::new(label, false, None))?;
    }
    for device in devices {
        group.append(&CheckMenuItem::with_id(
            format!("{prefix}{}", urlencoding::encode(&device.id)),
            &device.name,
            connected,
            device.checked,
            None,
        ))?;
    }
    Ok(group)
}

fn create_menu(signature: &MenuSignature) -> Result<Menu> {
    let status = MenuItem::new(&signature.status_text, false, None);
    let toggle = MenuItem::with_id("toggle", &signature.toggle_text, signature.can_toggle, None);
    let personal = device_submenu(
        "개인 믹스",
        PERSONAL_PREFIX,
        &signature.personal_devices,
        signature.connected,
    )?;
    let stream = device_submenu(
        "스트림 믹스",
        STREAM_PREFIX,
        &signature.stream_devices,
        signature.connected,
    )?;
    let mic = device_submenu("마이크 입력", MIC_PREFIX, &signature.mic_devices, signature.connected)?;
    let show = MenuItem::with_id("show", "Better Sonar 열기", true, None);
    let quit = MenuItem::with_id("quit", "종료", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status as &dyn IsMenuItem,
        &toggle,
        &PredefinedMenuItem::separator(),
        &personal,
        &stream,
        &mic,
        &PredefinedMenuItem::separator(),
        &show,
        &quit,
    ])?;
    Ok(menu)
}

pub struct Tray {
    icon: TrayIcon,
    signature: MenuSignature,
}

impl Tray {
    pub fn new() -> Result<Self> {
        let signature = MenuSignature::from_snapshot(None);
        let icon = image::load_from_memory(include_bytes!("../assets/icon.png"))?
            .resize(32, 32, image::imageops::FilterType::Lanczos3)
            .into_rgba8();
        let (width, height) = icon.dimensions();
        let icon = TrayIconBuilder::new()
            .with_tooltip("Better Sonar")
            .with_icon(Icon::from_rgba(icon.into_raw(), width, height)?)
            .with_menu(Box::new(create_menu(&signature)?))
            .with_menu_on_left_click(false)
            .build()?;
        Ok(Self { icon, signature })
    }

    pub fn update(&mut self, snapshot: &AppSnapshot) -> Result<()> {
        let signature = MenuSignature::from_snapshot(Some(snapshot));
        if self.signature != signature {
            self.icon.set_menu(Some(Box::new(create_menu(&signature)?)));
            self.icon
                .set_tooltip(Some(format!("Better Sonar · {}", signature.status_text)))?;
            self.signature = signature;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use crate::models::{AppSettings, AppSnapshot, AudioDevice, ConnectionStatus};

    use super::MenuSignature;

    fn snapshot() -> AppSnapshot {
        AppSnapshot {
            status: ConnectionStatus::Connected,
            message: "연결됨".into(),
            mode: Some("stream".into()),
            devices: vec![AudioDevice {
                id: "speaker".into(),
                name: "Speakers".into(),
                state: "active".into(),
                channels: 2,
            }],
            input_devices: vec![AudioDevice {
                id: "microphone".into(),
                name: "Microphone".into(),
                state: "active".into(),
                channels: 1,
            }],
            personal_device_id: Some("speaker".into()),
            personal_device_name: Some("Speakers".into()),
            stream_device_id: Some("speaker".into()),
            stream_device_name: Some("Speakers".into()),
            mic_device_id: Some("microphone".into()),
            mic_device_name: Some("Microphone".into()),
            settings: AppSettings::default(),
            last_updated_at: 1,
        }
    }

    #[test]
    fn periodic_timestamp_changes_do_not_change_the_menu_signature() {
        let first = snapshot();
        let mut refreshed = first.clone();
        refreshed.last_updated_at = 2;

        assert_eq!(
            MenuSignature::from_snapshot(Some(&first)),
            MenuSignature::from_snapshot(Some(&refreshed))
        );
    }

    #[test]
    fn selected_device_changes_do_change_the_menu_signature() {
        let first = snapshot();
        let mut changed = first.clone();
        changed.personal_device_id = Some("other".into());

        assert_ne!(
            MenuSignature::from_snapshot(Some(&first)),
            MenuSignature::from_snapshot(Some(&changed))
        );
    }
}
