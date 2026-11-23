mod client;
mod device_panel;
mod discovery;
#[cfg(not(test))]
mod gg_shortcuts;
mod runtime;

use std::{sync::Arc, time::Duration};

use async_channel::Sender;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{app::Event, features::settings::AppSettings};

pub(crate) use client::PersonalVolumeAction;
pub(crate) use device_panel::device_panel;
pub(crate) use runtime::AppRuntime;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConnectionStatus {
    Connected,
    GgNotRunning,
    SonarDisabled,
    SonarStarting,
    WrongMode,
    ApiChanged,
    CommunicationError,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AudioDevice {
    pub id: String,
    pub name: String,
    pub state: String,
    pub channels: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshot {
    pub status: ConnectionStatus,
    pub message: String,
    pub mode: Option<String>,
    pub devices: Vec<AudioDevice>,
    pub input_devices: Vec<AudioDevice>,
    pub personal_device_id: Option<String>,
    pub personal_device_name: Option<String>,
    pub stream_device_id: Option<String>,
    pub stream_device_name: Option<String>,
    pub mic_device_id: Option<String>,
    pub mic_device_name: Option<String>,
    pub settings: AppSettings,
    pub last_updated_at: u64,
}

impl AppSnapshot {
    fn disconnected(status: ConnectionStatus, message: impl Into<String>, settings: AppSettings) -> Self {
        Self {
            status,
            message: message.into(),
            mode: None,
            devices: vec![],
            input_devices: vec![],
            personal_device_id: None,
            personal_device_name: None,
            stream_device_id: None,
            stream_device_name: None,
            mic_device_id: None,
            mic_device_name: None,
            settings,
            last_updated_at: unix_millis(),
        }
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) enum SonarAction {
    Refresh,
    Toggle,
    Personal(String),
    Stream(String),
    Mic(String),
    Settings(AppSettings),
}

pub(crate) async fn run_sonar(
    runtime: Arc<AppRuntime>,
    mut actions: mpsc::UnboundedReceiver<SonarAction>,
    events: Sender<Event>,
) {
    let mut refresh = tokio::time::interval(Duration::from_secs(2));
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let action = tokio::select! {
            _ = refresh.tick() => SonarAction::Refresh,
            action = actions.recv() => match action {
                Some(action) => action,
                None => break,
            }
        };
        let result = match action {
            SonarAction::Refresh => Ok(runtime.refresh().await),
            SonarAction::Toggle => runtime.toggle().await,
            SonarAction::Personal(device) => runtime.set_output(&device).await,
            SonarAction::Stream(device) => runtime.set_stream_output(&device).await,
            SonarAction::Mic(device) => runtime.set_mic_input(&device).await,
            SonarAction::Settings(settings) => Ok(runtime.apply_settings(settings).await),
        };
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = events.send(Event::Notice(error)).await;
                runtime.refresh().await
            }
        };
        if events.send(Event::Snapshot(Box::new(snapshot))).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ConnectionStatus;
    use crate::features::settings::{AppSettings, ThemePreference};

    #[test]
    fn frontend_contract_uses_expected_json_field_and_status_names() {
        let settings = AppSettings {
            headset_device_id: Some("headset".into()),
            speaker_device_id: Some("speaker".into()),
            shortcut: "Ctrl+Alt+F9".into(),
            autostart: false,
            media_keys_enabled: true,
            theme: ThemePreference::Light,
        };

        assert_eq!(
            serde_json::to_value(settings).unwrap(),
            json!({
                "headsetDeviceId": "headset",
                "speakerDeviceId": "speaker",
                "shortcut": "Ctrl+Alt+F9",
                "autostart": false,
                "mediaKeysEnabled": true,
                "theme": "light"
            })
        );
        assert_eq!(
            serde_json::to_value(ConnectionStatus::CommunicationError).unwrap(),
            json!("communication_error")
        );
    }
}
