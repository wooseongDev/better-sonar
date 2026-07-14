use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
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
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub state: String,
    pub channels: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub headset_device_id: Option<String>,
    pub speaker_device_id: Option<String>,
    pub shortcut: String,
    pub autostart: bool,
    #[serde(default = "default_media_keys_enabled")]
    pub media_keys_enabled: bool,
}

const fn default_media_keys_enabled() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            headset_device_id: None,
            speaker_device_id: None,
            shortcut: "Ctrl+Alt+F9".into(),
            autostart: false,
            media_keys_enabled: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub status: ConnectionStatus,
    pub message: String,
    pub mode: Option<String>,
    pub devices: Vec<AudioDevice>,
    pub personal_device_id: Option<String>,
    pub personal_device_name: Option<String>,
    pub stream_device_id: Option<String>,
    pub settings: AppSettings,
    pub last_updated_at: u64,
}

impl AppSnapshot {
    pub fn disconnected(status: ConnectionStatus, message: impl Into<String>, settings: AppSettings) -> Self {
        Self {
            status,
            message: message.into(),
            mode: None,
            devices: vec![],
            personal_device_id: None,
            personal_device_name: None,
            stream_device_id: None,
            settings,
            last_updated_at: unix_millis(),
        }
    }
}

pub fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{AppSettings, ConnectionStatus};

    #[test]
    fn frontend_contract_uses_expected_json_field_and_status_names() {
        let settings = AppSettings {
            headset_device_id: Some("headset".into()),
            speaker_device_id: Some("speaker".into()),
            shortcut: "Ctrl+Alt+F9".into(),
            autostart: false,
            media_keys_enabled: true,
        };

        assert_eq!(
            serde_json::to_value(settings).unwrap(),
            json!({
                "headsetDeviceId": "headset",
                "speakerDeviceId": "speaker",
                "shortcut": "Ctrl+Alt+F9",
                "autostart": false,
                "mediaKeysEnabled": true
            })
        );
        assert_eq!(
            serde_json::to_value(ConnectionStatus::CommunicationError).unwrap(),
            json!("communication_error")
        );
    }
}
