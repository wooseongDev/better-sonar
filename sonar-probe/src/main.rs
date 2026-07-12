use std::{fs, path::PathBuf, thread, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use serde::Deserialize;

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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioDevice {
    friendly_name: String,
    id: String,
    data_flow: String,
    role: String,
    state: String,
    is_vad: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StreamRedirection {
    stream_redirection_id: String,
    device_id: String,
    is_running: bool,
}

fn core_props_path() -> PathBuf {
    std::env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join(r"SteelSeries\GG\coreProps.json")
}

fn redirection(items: &[StreamRedirection], id: &str) -> Result<StreamRedirection> {
    items
        .iter()
        .find(|item| item.stream_redirection_id == id)
        .cloned()
        .with_context(|| format!("streamRedirections에 {id} 항목이 없습니다"))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let switch_to = args
        .windows(2)
        .find(|pair| pair[0] == "--switch-to")
        .map(|pair| pair[1].clone());

    let props: CoreProps =
        serde_json::from_str(&fs::read_to_string(core_props_path()).context("GG coreProps.json을 읽을 수 없습니다")?)
            .context("GG coreProps.json 형식이 예상과 다릅니다")?;

    let discovery = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(3))
        .build()?;
    let apps: SubApps = discovery
        .get(format!("https://{}/subApps", props.gg_encrypted_address))
        .send()?
        .error_for_status()?
        .json()?;
    ensure!(apps.sub_apps.sonar.is_enabled, "Sonar가 비활성화되어 있습니다");
    ensure!(
        apps.sub_apps.sonar.is_ready && apps.sub_apps.sonar.is_running,
        "Sonar가 아직 실행 준비되지 않았습니다"
    );
    let base = apps.sub_apps.sonar.metadata.web_server_address;
    ensure!(!base.is_empty(), "Sonar API 주소가 비어 있습니다");

    let api = Client::builder().timeout(Duration::from_secs(3)).build()?;
    let mode: String = api.get(format!("{base}/mode")).send()?.error_for_status()?.json()?;
    ensure!(
        mode == "stream",
        "현재 모드는 {mode:?}이며 Sonar for Streamers가 아닙니다"
    );

    let devices: Vec<AudioDevice> = api
        .get(format!("{base}/audioDevices"))
        .send()?
        .error_for_status()?
        .json()?;
    let outputs: Vec<_> = devices
        .iter()
        .filter(|d| d.data_flow == "render" && d.role == "none" && !d.is_vad)
        .collect();
    println!("Sonar 연결 성공: mode={mode}, 물리 재생 장치={}개", outputs.len());
    for device in &outputs {
        println!("- {} [{}] state={}", device.friendly_name, device.id, device.state);
    }

    let before: Vec<StreamRedirection> = api
        .get(format!("{base}/streamRedirections"))
        .send()?
        .error_for_status()?
        .json()?;
    let personal_before = redirection(&before, "monitoring")?;
    let stream_before = redirection(&before, "streaming")?;
    println!(
        "Personal Mix: {} (running={})",
        personal_before.device_id, personal_before.is_running
    );
    println!(
        "Stream Mix: {} (running={})",
        stream_before.device_id, stream_before.is_running
    );

    let Some(target) = switch_to else {
        println!("읽기 전용 검증 완료. 실제 왕복 검증은 --switch-to <device-id>로 실행하세요.");
        return Ok(());
    };
    ensure!(
        target != personal_before.device_id,
        "대상 장치가 현재 Personal Mix 장치와 같습니다"
    );
    let target_device = outputs
        .iter()
        .find(|d| d.id == target && d.state == "active")
        .context("대상은 활성 물리 재생 장치여야 합니다")?;

    let put = |id: &str| -> Result<()> {
        let encoded = urlencoding::encode(id);
        api.put(format!("{base}/streamRedirections/monitoring/deviceId/{encoded}"))
            .send()?
            .error_for_status()?;
        Ok(())
    };

    println!("Personal Mix를 {}로 전환합니다.", target_device.friendly_name);
    put(&target)?;
    thread::sleep(Duration::from_millis(250));
    let changed: Vec<StreamRedirection> = api
        .get(format!("{base}/streamRedirections"))
        .send()?
        .error_for_status()?
        .json()?;
    let personal_changed = redirection(&changed, "monitoring")?;
    let stream_changed = redirection(&changed, "streaming")?;
    if personal_changed.device_id != target || stream_changed != stream_before {
        let _ = put(&personal_before.device_id);
        bail!("전환 검증 실패: Personal 또는 Stream Mix 상태가 예상과 다릅니다");
    }

    println!("원래 Personal Mix 장치로 복원합니다.");
    put(&personal_before.device_id)?;
    thread::sleep(Duration::from_millis(250));
    let restored: Vec<StreamRedirection> = api
        .get(format!("{base}/streamRedirections"))
        .send()?
        .error_for_status()?
        .json()?;
    ensure!(
        redirection(&restored, "monitoring")? == personal_before,
        "Personal Mix 원복 검증 실패"
    );
    ensure!(
        redirection(&restored, "streaming")? == stream_before,
        "Stream Mix가 변경되었습니다"
    );
    println!("왕복 전환 및 Stream Mix 불변 검증 성공");
    Ok(())
}
