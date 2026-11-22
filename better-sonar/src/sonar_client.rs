use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{discovery::discover_sonar, models::AudioDevice};

#[cfg(any(windows, test))]
const MASTER_VOLUME_STEP: f32 = 0.05;
#[cfg(any(windows, test))]
const VOLUME_EPSILON: f32 = 0.001;

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PersonalVolumeAction {
    ToggleMute,
    StepDown,
    StepUp,
}

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

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize)]
struct RawVolumeState {
    volume: f32,
    muted: bool,
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
struct RawStreamMasters {
    stream: RawStreamMasterTargets,
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
struct RawStreamMasterTargets {
    monitoring: RawVolumeState,
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
struct RawStreamerVolumeSettings {
    masters: RawStreamMasters,
}

#[derive(Clone, Debug)]
pub struct SonarState {
    pub mode: String,
    pub devices: Vec<AudioDevice>,
    pub input_devices: Vec<AudioDevice>,
    pub personal_device_id: String,
    pub stream_device_id: String,
    pub mic_device_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirectionTarget {
    Personal,
    Stream,
    Mic,
}

impl RedirectionTarget {
    fn id(self) -> &'static str {
        match self {
            Self::Personal => "monitoring",
            Self::Stream => "streaming",
            Self::Mic => "mic",
        }
    }

    fn data_flow(self) -> &'static str {
        match self {
            Self::Personal | Self::Stream => "render",
            Self::Mic => "capture",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Personal => "Personal Mix",
            Self::Stream => "Stream Mix",
            Self::Mic => "마이크 입력",
        }
    }
}

pub struct SonarClient {
    http: Client,
    #[cfg(all(windows, not(test)))]
    gg_shortcuts: crate::gg_shortcuts::GgShortcutClient,
    #[cfg(test)]
    fixed_base: Option<String>,
}

impl SonarClient {
    pub fn new() -> Result<Self, SonarError> {
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(3)).build()?,
            #[cfg(all(windows, not(test)))]
            gg_shortcuts: crate::gg_shortcuts::GgShortcutClient::new(),
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
            #[cfg(all(windows, not(test)))]
            gg_shortcuts: crate::gg_shortcuts::GgShortcutClient::new(),
            fixed_base: Some(base),
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: String) -> Result<T, SonarError> {
        let response = self.http.get(url).send().await?;
        if matches!(response.status().as_u16(), 404 | 405) {
            return Err(SonarError::ApiChanged(format!(
                "Sonar가 필요한 조회 경로를 지원하지 않습니다 ({})",
                response.status()
            )));
        }
        let response = response.error_for_status()?;
        response
            .json()
            .await
            .map_err(|_| SonarError::ApiChanged("Sonar 응답 형식이 예상과 다릅니다".into()))
    }

    async fn redirections(&self, base: &str) -> Result<Vec<RawRedirection>, SonarError> {
        self.get_json(format!("{base}/streamRedirections")).await
    }

    #[cfg(any(windows, test))]
    async fn streamer_volume_settings(&self, base: &str) -> Result<RawStreamerVolumeSettings, SonarError> {
        self.get_json(format!("{base}/volumeSettings/streamer")).await
    }

    #[cfg(any(windows, test))]
    async fn ensure_streamer_mode(&self, base: &str) -> Result<(), SonarError> {
        let mode: String = self.get_json(format!("{base}/mode")).await?;
        if mode == "stream" {
            Ok(())
        } else {
            Err(SonarError::WrongMode)
        }
    }

    #[cfg(test)]
    async fn put_internal_api(&self, url: String) -> Result<(), SonarError> {
        let response = self.http.put(&url).send().await?;
        if response.status().is_success() {
            return Ok(());
        }
        if matches!(response.status().as_u16(), 404 | 405) {
            return Err(SonarError::ApiChanged(format!(
                "Sonar가 Master - Personal 변경 경로를 지원하지 않습니다 ({})",
                response.status()
            )));
        }
        response.error_for_status()?;
        Ok(())
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
            .iter()
            .filter(|d| d.data_flow == "render" && d.role == "none" && !d.is_vad)
            .map(|d| AudioDevice {
                id: d.id.clone(),
                name: d.friendly_name.clone(),
                state: d.state.clone(),
                channels: d.channels,
            })
            .collect();
        let input_devices = raw_devices
            .iter()
            .filter(|d| d.data_flow == "capture" && d.role == "none" && !d.is_vad)
            .map(|d| AudioDevice {
                id: d.id.clone(),
                name: d.friendly_name.clone(),
                state: d.state.clone(),
                channels: d.channels,
            })
            .collect();
        let redirections = self.redirections(&base).await?;
        let personal = Self::find_redirection(&redirections, "monitoring")?;
        let stream = Self::find_redirection(&redirections, "streaming")?;
        let mic = Self::find_redirection(&redirections, "mic")?;
        if !personal.is_running {
            return Err(SonarError::Verification("Personal Mix가 실행 중이 아닙니다".into()));
        }
        Ok(SonarState {
            mode,
            devices,
            input_devices,
            personal_device_id: personal.device_id.clone(),
            stream_device_id: stream.device_id.clone(),
            mic_device_id: mic.device_id.clone(),
        })
    }

    pub async fn set_personal_output(&self, device_id: &str) -> Result<(), SonarError> {
        self.set_redirection(RedirectionTarget::Personal, device_id).await
    }

    pub async fn set_stream_output(&self, device_id: &str) -> Result<(), SonarError> {
        self.set_redirection(RedirectionTarget::Stream, device_id).await
    }

    pub async fn set_mic_input(&self, device_id: &str) -> Result<(), SonarError> {
        self.set_redirection(RedirectionTarget::Mic, device_id).await
    }

    async fn set_redirection(&self, target: RedirectionTarget, device_id: &str) -> Result<(), SonarError> {
        let base = self.base().await?;
        let mode: String = self.get_json(format!("{base}/mode")).await?;
        if mode != "stream" {
            return Err(SonarError::WrongMode);
        }
        let devices: Vec<RawAudioDevice> = self.get_json(format!("{base}/audioDevices")).await?;
        if !devices.iter().any(|d| {
            d.id == device_id
                && d.data_flow == target.data_flow()
                && d.role == "none"
                && !d.is_vad
                && d.state == "active"
        }) {
            return Err(SonarError::DeviceUnavailable);
        }

        let before = self.redirections(&base).await?;
        for id in ["monitoring", "streaming", "mic"] {
            Self::find_redirection(&before, id)?;
        }
        let encoded = urlencoding::encode(device_id);
        self.http
            .put(format!("{base}/streamRedirections/{}/deviceId/{encoded}", target.id()))
            .send()
            .await?
            .error_for_status()?;

        tokio::time::sleep(Duration::from_millis(180)).await;
        let after = self.redirections(&base).await?;
        let target_after = Self::find_redirection(&after, target.id())?;
        if target_after.device_id != device_id {
            return Err(SonarError::Verification(format!(
                "{}이(가) 요청한 장치로 변경되지 않았습니다",
                target.label()
            )));
        }

        for id in ["monitoring", "streaming", "mic"] {
            if id == target.id() {
                continue;
            }
            let before_value = serde_json::to_value(Self::find_redirection(&before, id)?)
                .map_err(|error| SonarError::ApiChanged(error.to_string()))?;
            let after_value = serde_json::to_value(Self::find_redirection(&after, id)?)
                .map_err(|error| SonarError::ApiChanged(error.to_string()))?;
            if before_value != after_value {
                return Err(SonarError::Verification(format!(
                    "{} 변경 중 {id} 리디렉션이 함께 변경되었습니다",
                    target.label()
                )));
            }
        }
        Ok(())
    }

    #[cfg(any(windows, test))]
    pub async fn control_personal_volume(&self, action: PersonalVolumeAction) -> Result<(), SonarError> {
        let base = self.base().await?;
        self.ensure_streamer_mode(&base).await?;
        let before = self.streamer_volume_settings(&base).await?.masters.stream.monitoring;
        if !before.volume.is_finite() || !(0.0..=1.0).contains(&before.volume) {
            return Err(SonarError::ApiChanged(
                "Master - Personal 음량 값이 예상 범위를 벗어났습니다".into(),
            ));
        }

        let (path, expected) = match action {
            PersonalVolumeAction::ToggleMute => (
                format!(
                    "{base}/volumeSettings/streamer/monitoring/master/isMuted/{}",
                    !before.muted
                ),
                RawVolumeState {
                    volume: before.volume,
                    muted: !before.muted,
                },
            ),
            PersonalVolumeAction::StepDown | PersonalVolumeAction::StepUp => {
                let delta = if action == PersonalVolumeAction::StepUp {
                    MASTER_VOLUME_STEP
                } else {
                    -MASTER_VOLUME_STEP
                };
                let next = (before.volume + delta).clamp(0.0, 1.0);
                (
                    format!("{base}/volumeSettings/streamer/monitoring/master/volume/{next}"),
                    RawVolumeState {
                        volume: next,
                        muted: before.muted,
                    },
                )
            }
        };

        #[cfg(all(windows, not(test)))]
        self.gg_shortcuts.trigger(action).await?;
        #[cfg(any(not(windows), test))]
        self.put_internal_api(path).await?;
        #[cfg(all(windows, not(test)))]
        let _ = path;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let after = self.streamer_volume_settings(&base).await?.masters.stream.monitoring;
        let applied = match action {
            PersonalVolumeAction::ToggleMute => after.muted == expected.muted,
            PersonalVolumeAction::StepDown | PersonalVolumeAction::StepUp => {
                (after.volume - expected.volume).abs() <= VOLUME_EPSILON
            }
        };
        if !applied {
            return Err(SonarError::Verification(
                "Sonar Master - Personal 값이 요청대로 변경되지 않았습니다".into(),
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

    use super::{PersonalVolumeAction, SonarClient, SonarError};

    const HEADSET_ID: &str = "{0.0.0.00000000}.{headset}";
    const SPEAKER_ID: &str = "{0.0.0.00000000}.{speaker}";
    const STREAM_ID: &str = "{0.0.0.00000000}.{stream}";
    const MIC_ID: &str = "{0.0.1.00000000}.{microphone}";
    const SECOND_MIC_ID: &str = "{0.0.1.00000000}.{second-microphone}";

    #[derive(Clone)]
    struct MockState {
        inner: Arc<Mutex<MockInner>>,
    }

    struct MockInner {
        mode: String,
        personal_id: String,
        stream_id: String,
        mic_id: String,
        mutate_stream_on_put: bool,
        ignore_put: bool,
        put_count: usize,
        personal_volume: f32,
        personal_muted: bool,
        streaming_volume: f32,
        volume_put_count: usize,
        ignore_volume_put: bool,
        reject_volume_put: bool,
    }

    impl Default for MockInner {
        fn default() -> Self {
            Self {
                mode: "stream".into(),
                personal_id: HEADSET_ID.into(),
                stream_id: STREAM_ID.into(),
                mic_id: MIC_ID.into(),
                mutate_stream_on_put: false,
                ignore_put: false,
                put_count: 0,
                personal_volume: 0.6,
                personal_muted: false,
                streaming_volume: 0.8,
                volume_put_count: 0,
                ignore_volume_put: false,
                reject_volume_put: false,
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
                "id": MIC_ID,
                "dataFlow": "capture",
                "role": "none",
                "channels": 1,
                "state": "active",
                "isVad": false
            },
            {
                "friendlyName": "Second microphone",
                "id": SECOND_MIC_ID,
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
            },
            {
                "streamRedirectionId": "mic",
                "deviceId": inner.mic_id,
                "status": [],
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

    async fn set_streaming(State(state): State<MockState>, Path(device_id): Path<String>) -> StatusCode {
        let mut inner = state.inner.lock().await;
        inner.put_count += 1;
        if !inner.ignore_put {
            inner.stream_id = device_id;
        }
        StatusCode::OK
    }

    async fn set_mic(State(state): State<MockState>, Path(device_id): Path<String>) -> StatusCode {
        let mut inner = state.inner.lock().await;
        inner.put_count += 1;
        if !inner.ignore_put {
            inner.mic_id = device_id;
        }
        StatusCode::OK
    }

    async fn volume_settings(State(state): State<MockState>) -> Json<Value> {
        let inner = state.inner.lock().await;
        Json(json!({
            "masters": {
                "stream": {
                    "streaming": { "volume": inner.streaming_volume, "muted": false },
                    "monitoring": { "volume": inner.personal_volume, "muted": inner.personal_muted }
                },
                "classic": { "volume": 1.0, "muted": false }
            },
            "devices": {}
        }))
    }

    async fn set_master_volume(State(state): State<MockState>, Path(volume): Path<f32>) -> StatusCode {
        let mut inner = state.inner.lock().await;
        inner.volume_put_count += 1;
        if inner.reject_volume_put {
            return StatusCode::NOT_FOUND;
        }
        if !inner.ignore_volume_put {
            inner.personal_volume = volume;
        }
        StatusCode::OK
    }

    async fn set_master_mute(State(state): State<MockState>, Path(muted): Path<bool>) -> StatusCode {
        let mut inner = state.inner.lock().await;
        inner.volume_put_count += 1;
        if inner.reject_volume_put {
            return StatusCode::NOT_FOUND;
        }
        if !inner.ignore_volume_put {
            inner.personal_muted = muted;
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
            .route(
                "/streamRedirections/streaming/deviceId/{*device_id}",
                put(set_streaming),
            )
            .route("/streamRedirections/mic/deviceId/{*device_id}", put(set_mic))
            .route("/volumeSettings/streamer", get(volume_settings))
            .route(
                "/volumeSettings/streamer/monitoring/master/volume/{volume}",
                put(set_master_volume),
            )
            .route(
                "/volumeSettings/streamer/monitoring/master/isMuted/{muted}",
                put(set_master_mute),
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
        assert_eq!(state.mic_device_id, MIC_ID);
        assert_eq!(state.devices.len(), 3);
        assert_eq!(state.input_devices.len(), 2);
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

        assert!(matches!(error, SonarError::Verification(message) if message.contains("streaming")));
    }

    #[tokio::test]
    async fn set_stream_output_changes_only_streaming_redirection() {
        let (client, state) = server(MockInner::default()).await;

        client.set_stream_output(SPEAKER_ID).await.unwrap();

        let inner = state.inner.lock().await;
        assert_eq!(inner.personal_id, HEADSET_ID);
        assert_eq!(inner.stream_id, SPEAKER_ID);
        assert_eq!(inner.mic_id, MIC_ID);
        assert_eq!(inner.put_count, 1);
    }

    #[tokio::test]
    async fn set_mic_input_changes_only_mic_redirection() {
        let (client, state) = server(MockInner::default()).await;

        client.set_mic_input(SECOND_MIC_ID).await.unwrap();

        let inner = state.inner.lock().await;
        assert_eq!(inner.personal_id, HEADSET_ID);
        assert_eq!(inner.stream_id, STREAM_ID);
        assert_eq!(inner.mic_id, SECOND_MIC_ID);
        assert_eq!(inner.put_count, 1);
    }

    #[tokio::test]
    async fn set_mic_input_rejects_render_device() {
        let (client, state) = server(MockInner::default()).await;

        let error = client.set_mic_input(SPEAKER_ID).await.unwrap_err();

        assert!(matches!(error, SonarError::DeviceUnavailable));
        assert_eq!(state.inner.lock().await.put_count, 0);
    }

    #[tokio::test]
    async fn personal_master_volume_uses_sonar_step_and_preserves_stream_master() {
        let (client, state) = server(MockInner::default()).await;

        client
            .control_personal_volume(PersonalVolumeAction::StepUp)
            .await
            .unwrap();
        client
            .control_personal_volume(PersonalVolumeAction::StepDown)
            .await
            .unwrap();

        let inner = state.inner.lock().await;
        assert!((inner.personal_volume - 0.6).abs() < 0.001);
        assert!((inner.streaming_volume - 0.8).abs() < f32::EPSILON);
        assert_eq!(inner.volume_put_count, 2);
    }

    #[tokio::test]
    async fn personal_master_mute_toggles_sonar_monitoring_master() {
        let (client, state) = server(MockInner::default()).await;

        client
            .control_personal_volume(PersonalVolumeAction::ToggleMute)
            .await
            .unwrap();

        let inner = state.inner.lock().await;
        assert!(inner.personal_muted);
        assert_eq!(inner.volume_put_count, 1);
    }

    #[tokio::test]
    async fn personal_master_control_detects_when_sonar_ignores_the_request() {
        let inner = MockInner {
            ignore_volume_put: true,
            ..MockInner::default()
        };
        let (client, state) = server(inner).await;

        let error = client
            .control_personal_volume(PersonalVolumeAction::StepUp)
            .await
            .unwrap_err();

        assert!(matches!(error, SonarError::Verification(message) if message.contains("변경되지 않았습니다")));
        assert_eq!(state.inner.lock().await.volume_put_count, 1);
    }

    #[tokio::test]
    async fn personal_master_control_rejects_classic_mode_without_writing() {
        let inner = MockInner {
            mode: "classic".into(),
            ..MockInner::default()
        };
        let (client, state) = server(inner).await;

        let error = client
            .control_personal_volume(PersonalVolumeAction::StepUp)
            .await
            .unwrap_err();

        assert!(matches!(error, SonarError::WrongMode));
        assert_eq!(state.inner.lock().await.volume_put_count, 0);
    }

    #[tokio::test]
    async fn personal_master_control_reports_internal_api_changes() {
        let inner = MockInner {
            reject_volume_put: true,
            ..MockInner::default()
        };
        let (client, state) = server(inner).await;

        let error = client
            .control_personal_volume(PersonalVolumeAction::StepUp)
            .await
            .unwrap_err();

        assert!(matches!(error, SonarError::ApiChanged(message) if message.contains("변경 경로")));
        assert_eq!(state.inner.lock().await.volume_put_count, 1);
    }
}
