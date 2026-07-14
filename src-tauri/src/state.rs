use std::{path::PathBuf, sync::Arc};

use tokio::sync::{Mutex, RwLock};

use crate::{
    models::{AppSettings, AppSnapshot, ConnectionStatus, unix_millis},
    settings,
    sonar_client::{SonarClient, SonarError},
};

pub struct AppRuntime {
    client: SonarClient,
    settings_path: PathBuf,
    settings: RwLock<AppSettings>,
    snapshot: RwLock<AppSnapshot>,
    operation: Mutex<()>,
}

impl AppRuntime {
    pub fn new(settings_path: PathBuf, initial_settings: AppSettings) -> Result<Arc<Self>, SonarError> {
        let snapshot = AppSnapshot::disconnected(
            ConnectionStatus::SonarStarting,
            "Sonar 연결을 확인하고 있습니다",
            initial_settings.clone(),
        );
        Ok(Arc::new(Self {
            client: SonarClient::new()?,
            settings_path,
            settings: RwLock::new(initial_settings),
            snapshot: RwLock::new(snapshot),
            operation: Mutex::new(()),
        }))
    }

    pub async fn snapshot(&self) -> AppSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn refresh(&self) -> AppSnapshot {
        let settings = self.settings.read().await.clone();
        let next = match self.client.state().await {
            Ok(state) => {
                let current_name = state
                    .devices
                    .iter()
                    .find(|device| device.id == state.personal_device_id)
                    .map(|device| device.name.clone());
                let missing_selection = [settings.headset_device_id.as_ref(), settings.speaker_device_id.as_ref()]
                    .into_iter()
                    .flatten()
                    .find(|id| {
                        !state
                            .devices
                            .iter()
                            .any(|device| &device.id == *id && device.state == "active")
                    });
                let message = if missing_selection.is_some() {
                    "저장된 장치 중 하나가 연결되어 있지 않습니다"
                } else {
                    "Sonar for Streamers에 연결되었습니다"
                };
                AppSnapshot {
                    status: ConnectionStatus::Connected,
                    message: message.into(),
                    mode: Some(state.mode),
                    devices: state.devices,
                    personal_device_id: Some(state.personal_device_id),
                    personal_device_name: current_name,
                    stream_device_id: Some(state.stream_device_id),
                    settings,
                    last_updated_at: unix_millis(),
                }
            }
            Err(error) => snapshot_from_error(error, settings),
        };
        *self.snapshot.write().await = next.clone();
        next
    }

    pub async fn set_output(&self, device_id: &str) -> Result<AppSnapshot, String> {
        let _guard = self.operation.lock().await;
        self.client
            .set_personal_output(device_id)
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = self.refresh().await;
        if snapshot.personal_device_id.as_deref() != Some(device_id) {
            return Err("전환 후 Personal Mix 상태를 확인하지 못했습니다".into());
        }
        Ok(snapshot)
    }

    pub async fn toggle(&self) -> Result<AppSnapshot, String> {
        let current = self.refresh().await;
        if current.status != ConnectionStatus::Connected {
            return Err(current.message);
        }
        let settings = self.settings.read().await.clone();
        let headset = settings.headset_device_id.ok_or("헤드셋 장치를 먼저 선택하세요")?;
        let speaker = settings.speaker_device_id.ok_or("스피커 장치를 먼저 선택하세요")?;
        if headset == speaker {
            return Err("헤드셋과 스피커는 서로 다른 장치여야 합니다".into());
        }
        let target = if current.personal_device_id.as_deref() == Some(headset.as_str()) {
            speaker
        } else {
            headset
        };
        self.set_output(&target).await
    }

    pub async fn save_settings(&self, value: AppSettings) -> Result<AppSnapshot, String> {
        if value.headset_device_id.is_some() && value.headset_device_id == value.speaker_device_id {
            return Err("헤드셋과 스피커는 서로 다른 장치여야 합니다".into());
        }
        settings::save(&self.settings_path, &value).map_err(|error| error.to_string())?;
        *self.settings.write().await = value;
        Ok(self.refresh().await)
    }

    #[cfg(windows)]
    pub async fn control_personal_volume(
        &self,
        action: crate::sonar_client::PersonalVolumeAction,
    ) -> Result<(), String> {
        let _guard = self.operation.lock().await;
        self.client
            .control_personal_volume(action)
            .await
            .map_err(|error| error.to_string())
    }
}

fn snapshot_from_error(error: SonarError, settings: AppSettings) -> AppSnapshot {
    let status = match &error {
        SonarError::GgNotRunning => ConnectionStatus::GgNotRunning,
        SonarError::SonarDisabled => ConnectionStatus::SonarDisabled,
        SonarError::SonarStarting => ConnectionStatus::SonarStarting,
        SonarError::WrongMode => ConnectionStatus::WrongMode,
        SonarError::ApiChanged(_) => ConnectionStatus::ApiChanged,
        SonarError::DeviceUnavailable | SonarError::Transport(_) | SonarError::Verification(_) => {
            ConnectionStatus::CommunicationError
        }
    };
    AppSnapshot::disconnected(status, error.to_string(), settings)
}

#[cfg(test)]
mod tests {
    use crate::{
        models::{AppSettings, ConnectionStatus},
        sonar_client::SonarError,
    };

    use super::snapshot_from_error;

    #[test]
    fn discovery_errors_keep_their_user_facing_status() {
        let cases = [
            (SonarError::GgNotRunning, ConnectionStatus::GgNotRunning),
            (SonarError::SonarDisabled, ConnectionStatus::SonarDisabled),
            (SonarError::SonarStarting, ConnectionStatus::SonarStarting),
            (SonarError::WrongMode, ConnectionStatus::WrongMode),
            (SonarError::ApiChanged("changed".into()), ConnectionStatus::ApiChanged),
            (SonarError::DeviceUnavailable, ConnectionStatus::CommunicationError),
            (
                SonarError::Verification("failed".into()),
                ConnectionStatus::CommunicationError,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(snapshot_from_error(error, AppSettings::default()).status, expected);
        }
    }
}
