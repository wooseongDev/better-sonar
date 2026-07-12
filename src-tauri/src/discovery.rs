use std::{
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use reqwest::Client;
use serde::Deserialize;

use crate::sonar_client::SonarError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoreProps {
    gg_encrypted_address: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubApps {
    sub_apps: Apps,
}

#[derive(Debug, Deserialize)]
struct Apps {
    sonar: SonarApp,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SonarApp {
    is_enabled: bool,
    is_ready: bool,
    is_running: bool,
    metadata: Metadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    web_server_address: String,
}

fn core_props_candidates() -> Vec<PathBuf> {
    let root = std::env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    vec![
        root.join(r"SteelSeries\GG\coreProps.json"),
        root.join(r"SteelSeries\SteelSeries Engine 3\coreProps.json"),
    ]
}

pub async fn discover_sonar() -> Result<String, SonarError> {
    let path = core_props_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or(SonarError::GgNotRunning)?;
    let raw = fs::read_to_string(path).map_err(|_| SonarError::GgNotRunning)?;
    let props: CoreProps =
        serde_json::from_str(&raw).map_err(|_| SonarError::ApiChanged("GG 연결 정보 형식이 예상과 다릅니다".into()))?;
    let gg_address: SocketAddr = props
        .gg_encrypted_address
        .parse()
        .map_err(|_| SonarError::ApiChanged("GG 연결 주소가 올바르지 않습니다".into()))?;
    if !gg_address.ip().is_loopback() {
        return Err(SonarError::ApiChanged(
            "안전을 위해 로컬이 아닌 GG 연결 주소는 거부했습니다".into(),
        ));
    }

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(SonarError::Transport)?;
    let response = client
        .get(format!("https://{gg_address}/subApps"))
        .send()
        .await
        .map_err(|_| SonarError::GgNotRunning)?
        .error_for_status()
        .map_err(|_| SonarError::GgNotRunning)?;
    let apps: SubApps = response
        .json()
        .await
        .map_err(|_| SonarError::ApiChanged("GG subApps 응답 형식이 예상과 다릅니다".into()))?;
    let sonar = apps.sub_apps.sonar;
    if !sonar.is_enabled {
        return Err(SonarError::SonarDisabled);
    }
    if !sonar.is_ready || !sonar.is_running || sonar.metadata.web_server_address.is_empty() {
        return Err(SonarError::SonarStarting);
    }
    let base = sonar.metadata.web_server_address.trim_end_matches('/').to_string();
    let parsed =
        reqwest::Url::parse(&base).map_err(|_| SonarError::ApiChanged("Sonar API 주소가 올바르지 않습니다".into()))?;
    let is_loopback = parsed.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    });
    if parsed.scheme() != "http" || !is_loopback {
        return Err(SonarError::ApiChanged(
            "안전을 위해 로컬 HTTP Sonar API 주소만 허용합니다".into(),
        ));
    }
    Ok(base)
}
