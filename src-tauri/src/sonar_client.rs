use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{discovery::discover_sonar, models::AudioDevice};

#[derive(Debug, Error)]
pub enum SonarError {
    #[error("SteelSeries GG가 실행 중이 아닙니다")]
    GgNotRunning,
    #[error("SteelSeries GG 설정에서 Sonar가 꺼져 있습니다")]
    SonarDisabled,
    #[error("Sonar가 시작 중입니다. 잠시 후 자동으로 다시 연결합니다")]
    SonarStarting,
    #[error("현재 Sonar for Streamers 모드가 아닙니다")]
    WrongMode,
    #[error("선택한 장치를 현재 Sonar에서 찾을 수 없습니다")]
    DeviceUnavailable,
    #[error("Sonar API가 변경되었을 수 있습니다: {0}")]
    ApiChanged(String),
    #[error("Sonar와 통신할 수 없습니다: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("출력 전환 검증에 실패했습니다: {0}")]
    Verification(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawAudioDevice {
    friendly_name: String,
    id: String,
    data_flow: String,
    role: String,
    channels: u32,
    state: String,
    is_vad: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawRedirection {
    stream_redirection_id: String,
    device_id: String,
    is_running: bool,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct SonarState {
    pub mode: String,
    pub devices: Vec<AudioDevice>,
    pub personal_device_id: String,
    pub stream_device_id: String,
}

pub struct SonarClient {
    http: Client,
    #[cfg(test)]
    fixed_base: Option<String>,
}

impl SonarClient {
    pub fn new() -> Result<Self, SonarError> {
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(3)).build()?,
            #[cfg(test)]
            fixed_base: None,
        })
    }

    async fn base(&self) -> Result<String, SonarError> {
        #[cfg(test)]
        if let Some(base) = &self.fixed_base {
            return Ok(base.clone());
        }
        discover_sonar().await
    }

    #[cfg(test)]
    fn with_base(base: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("테스트 HTTP 클라이언트를 생성해야 합니다"),
            fixed_base: Some(base),
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: String) -> Result<T, SonarError> {
        let response = self.http.get(url).send().await?.error_for_status()?;
        response
            .json()
            .await
            .map_err(|_| SonarError::ApiChanged("Sonar 응답 형식이 예상과 다릅니다".into()))
    }

    async fn redirections(&self, base: &str) -> Result<Vec<RawRedirection>, SonarError> {
        self.get_json(format!("{base}/streamRedirections")).await
    }

    fn find_redirection<'a>(items: &'a [RawRedirection], id: &str) -> Result<&'a RawRedirection, SonarError> {
        items
            .iter()
            .find(|item| item.stream_redirection_id == id)
            .ok_or_else(|| SonarError::ApiChanged(format!("streamRedirections에 {id} 항목이 없습니다")))
    }

    pub async fn state(&self) -> Result<SonarState, SonarError> {
        let base = self.base().await?;
        let mode: String = self.get_json(format!("{base}/mode")).await?;
        if mode != "stream" {
            return Err(SonarError::WrongMode);
        }
        let raw_devices: Vec<RawAudioDevice> = self.get_json(format!("{base}/audioDevices")).await?;
        let devices = raw_devices
            .into_iter()
            .filter(|d| d.data_flow == "render" && d.role == "none" && !d.is_vad)
            .map(|d| AudioDevice {
                id: d.id,
                name: d.friendly_name,
                state: d.state,
                channels: d.channels,
            })
            .collect();
        let redirections = self.redirections(&base).await?;
        let personal = Self::find_redirection(&redirections, "monitoring")?;
        let stream = Self::find_redirection(&redirections, "streaming")?;
        if !personal.is_running {
            return Err(SonarError::Verification("Personal Mix가 실행 중이 아닙니다".into()));
        }
        Ok(SonarState {
            mode,
            devices,
            personal_device_id: personal.device_id.clone(),
            stream_device_id: stream.device_id.clone(),
        })
    }

    pub async fn set_personal_output(&self, device_id: &str) -> Result<(), SonarError> {
        let base = self.base().await?;
        let mode: String = self.get_json(format!("{base}/mode")).await?;
        if mode != "stream" {
            return Err(SonarError::WrongMode);
        }
        let devices: Vec<RawAudioDevice> = self.get_json(format!("{base}/audioDevices")).await?;
        if !devices.iter().any(|d| {
            d.id == device_id && d.data_flow == "render" && d.role == "none" && !d.is_vad && d.state == "active"
        }) {
            return Err(SonarError::DeviceUnavailable);
        }

        let before = self.redirections(&base).await?;
        let stream_before = Self::find_redirection(&before, "streaming")?;
        let stream_fingerprint =
            serde_json::to_value(stream_before).map_err(|error| SonarError::ApiChanged(error.to_string()))?;
        let encoded = urlencoding::encode(device_id);
        self.http
            .put(format!("{base}/streamRedirections/monitoring/deviceId/{encoded}"))
            .send()
            .await?
            .error_for_status()?;

        tokio::time::sleep(Duration::from_millis(180)).await;
        let after = self.redirections(&base).await?;
        let personal_after = Self::find_redirection(&after, "monitoring")?;
        let stream_after = Self::find_redirection(&after, "streaming")?;
        if personal_after.device_id != device_id {
            return Err(SonarError::Verification(
                "Personal Mix가 요청한 장치로 변경되지 않았습니다".into(),
            ));
        }
        let after_fingerprint =
            serde_json::to_value(stream_after).map_err(|error| SonarError::ApiChanged(error.to_string()))?;
        if stream_fingerprint != after_fingerprint {
            return Err(SonarError::Verification(
                "Stream Mix가 함께 변경되어 작업을 중단했습니다".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Json, Router,
        extract::{Path, State},
        http::StatusCode,
        routing::{get, put},
    };
    use serde_json::{Value, json};
    use tokio::{net::TcpListener, sync::Mutex};

    use super::{SonarClient, SonarError};

    const HEADSET_ID: &str = "{0.0.0.00000000}.{headset}";
    const SPEAKER_ID: &str = "{0.0.0.00000000}.{speaker}";
    const STREAM_ID: &str = "{0.0.0.00000000}.{stream}";

    #[derive(Clone)]
    struct MockState {
        inner: Arc<Mutex<MockInner>>,
    }

    struct MockInner {
        mode: String,
        personal_id: String,
        stream_id: String,
        mutate_stream_on_put: bool,
        ignore_put: bool,
        put_count: usize,
    }

    impl Default for MockInner {
        fn default() -> Self {
            Self {
                mode: "stream".into(),
                personal_id: HEADSET_ID.into(),
                stream_id: STREAM_ID.into(),
                mutate_stream_on_put: false,
                ignore_put: false,
                put_count: 0,
            }
        }
    }

    fn devices() -> Value {
        json!([
            {
                "friendlyName": "Headphones",
                "id": HEADSET_ID,
                "dataFlow": "render",
                "role": "none",
                "channels": 2,
                "state": "active",
                "isVad": false
            },
            {
                "friendlyName": "Speakers",
                "id": SPEAKER_ID,
                "dataFlow": "render",
                "role": "none",
                "channels": 2,
                "state": "active",
                "isVad": false
            },
            {
                "friendlyName": "Disconnected output",
                "id": "inactive",
                "dataFlow": "render",
                "role": "none",
                "channels": 2,
                "state": "disabled",
                "isVad": false
            },
            {
                "friendlyName": "Sonar Gaming",
                "id": "vad",
                "dataFlow": "render",
                "role": "game",
                "channels": 8,
                "state": "active",
                "isVad": true
            },
            {
                "friendlyName": "Microphone",
                "id": "capture",
                "dataFlow": "capture",
                "role": "none",
                "channels": 1,
                "state": "active",
                "isVad": false
            }
        ])
    }

    async fn mode(State(state): State<MockState>) -> Json<Value> {
        Json(json!(state.inner.lock().await.mode))
    }

    async fn audio_devices() -> Json<Value> {
        Json(devices())
    }

    async fn redirections(State(state): State<MockState>) -> Json<Value> {
        let inner = state.inner.lock().await;
        Json(json!([
            {
                "streamRedirectionId": "streaming",
                "deviceId": inner.stream_id,
                "status": [{"role": "game", "isEnabled": true}],
                "isRunning": true
            },
            {
                "streamRedirectionId": "monitoring",
                "deviceId": inner.personal_id,
                "status": [{"role": "game", "isEnabled": true}],
                "isRunning": true
            }
        ]))
    }

    async fn set_monitoring(State(state): State<MockState>, Path(device_id): Path<String>) -> StatusCode {
        let mut inner = state.inner.lock().await;
        inner.put_count += 1;
        if !inner.ignore_put {
            inner.personal_id = device_id;
        }
        if inner.mutate_stream_on_put {
            inner.stream_id = "stream-was-modified".into();
        }
        StatusCode::OK
    }

    async fn server(inner: MockInner) -> (SonarClient, MockState) {
        let state = MockState {
            inner: Arc::new(Mutex::new(inner)),
        };
        let app = Router::new()
            .route("/mode", get(mode))
            .route("/audioDevices", get(audio_devices))
            .route("/streamRedirections", get(redirections))
            .route(
                "/streamRedirections/monitoring/deviceId/{*device_id}",
                put(set_monitoring),
            )
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (SonarClient::with_base(format!("http://{address}")), state)
    }

    #[tokio::test]
    async fn state_returns_only_physical_render_devices_and_current_mixes() {
        let (client, _) = server(MockInner::default()).await;

        let state = client.state().await.unwrap();

        assert_eq!(state.mode, "stream");
        assert_eq!(state.personal_device_id, HEADSET_ID);
        assert_eq!(state.stream_device_id, STREAM_ID);
        assert_eq!(state.devices.len(), 3);
        assert_eq!(state.devices[0].name, "Headphones");
        assert_eq!(state.devices[1].name, "Speakers");
        assert_eq!(state.devices[2].state, "disabled");
    }

    #[tokio::test]
    async fn state_rejects_classic_mode() {
        let inner = MockInner {
            mode: "classic".into(),
            ..MockInner::default()
        };
        let (client, _) = server(inner).await;

        assert!(matches!(client.state().await, Err(SonarError::WrongMode)));
    }

    #[tokio::test]
    async fn set_output_changes_monitoring_and_preserves_streaming() {
        let (client, state) = server(MockInner::default()).await;

        client.set_personal_output(SPEAKER_ID).await.unwrap();

        let inner = state.inner.lock().await;
        assert_eq!(inner.personal_id, SPEAKER_ID);
        assert_eq!(inner.stream_id, STREAM_ID);
        assert_eq!(inner.put_count, 1);
    }

    #[tokio::test]
    async fn set_output_rejects_unavailable_device_without_put() {
        let (client, state) = server(MockInner::default()).await;

        let error = client.set_personal_output("inactive").await.unwrap_err();

        assert!(matches!(error, SonarError::DeviceUnavailable));
        assert_eq!(state.inner.lock().await.put_count, 0);
    }

    #[tokio::test]
    async fn set_output_detects_when_personal_mix_did_not_change() {
        let inner = MockInner {
            ignore_put: true,
            ..MockInner::default()
        };
        let (client, _) = server(inner).await;

        let error = client.set_personal_output(SPEAKER_ID).await.unwrap_err();

        assert!(matches!(error, SonarError::Verification(message) if message.contains("변경되지 않았습니다")));
    }

    #[tokio::test]
    async fn set_output_detects_any_stream_mix_mutation() {
        let inner = MockInner {
            mutate_stream_on_put: true,
            ..MockInner::default()
        };
        let (client, _) = server(inner).await;

        let error = client.set_personal_output(SPEAKER_ID).await.unwrap_err();

        assert!(matches!(error, SonarError::Verification(message) if message.contains("Stream Mix")));
    }
}
