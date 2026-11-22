use std::{io::Write, path::Path, process::Command, time::Duration};

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use reqwest::{Client, Url};
use semver::Version;
use serde::Deserialize;

use crate::{app::Event, windows::InstallerKind};

const FEED: &str = "https://github.com/wooseongDev/better-sonar/releases/latest/download/latest.json";
const ASSET_PREFIX: &str = "https://github.com/wooseongDev/better-sonar/releases/download/";
const PUBLIC_KEY: &str = include_str!("../packaging/windows/updater.pub");
const MAX_INSTALLER: u64 = 256 * 1024 * 1024;

impl InstallerKind {
    fn platform(self) -> &'static str {
        match self {
            Self::Nsis => "windows-x86_64-nsis",
            Self::Msi => "windows-x86_64-msi",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Nsis => ".exe",
            Self::Msi => ".msi",
        }
    }

    fn launch(self, path: &Path) -> Result<()> {
        match self {
            Self::Nsis => Command::new(path).args(["/P", "/UPDATE"]).spawn(),
            Self::Msi => {
                let msiexec = std::env::var_os("SystemRoot")
                    .map(std::path::PathBuf::from)
                    .context("Windows 폴더를 찾을 수 없습니다")?
                    .join("System32/msiexec.exe");
                Command::new(msiexec)
                    .arg("/i")
                    .arg(path)
                    .args(["/passive", "/norestart", "AUTOLAUNCHAPP=True"])
                    .spawn()
            }
        }
        .context("설치 프로그램을 시작할 수 없습니다")?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Update {
    pub version: Version,
    pub notes: Option<String>,
    asset: Asset,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    url: String,
    signature: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    notes: Option<String>,
    platforms: std::collections::HashMap<String, Asset>,
}

#[derive(Clone)]
pub struct Updater {
    client: Client,
    installer: InstallerKind,
}

impl Updater {
    pub fn new(installer: InstallerKind) -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("better-sonar/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .https_only(true)
            .build()?;
        Ok(Self { client, installer })
    }

    pub async fn check(&self) -> Result<Option<Update>> {
        let manifest: Manifest = self.client.get(FEED).send().await?.error_for_status()?.json().await?;
        select_update(manifest, self.installer, &Version::parse(env!("CARGO_PKG_VERSION"))?)
    }

    pub async fn install(&self, expected: Version, events: &async_channel::Sender<Event>) -> Result<()> {
        let update = self.check().await?.context("설치할 업데이트가 없습니다")?;
        ensure!(
            update.version == expected,
            "배포 버전이 변경되었습니다. 업데이트를 다시 확인하세요"
        );
        let mut response = self.client.get(&update.asset.url).send().await?.error_for_status()?;
        let total = response.content_length();
        ensure!(
            total.is_none_or(|length| length <= MAX_INSTALLER),
            "설치 파일이 너무 큽니다"
        );
        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                data.len() as u64 + chunk.len() as u64 <= MAX_INSTALLER,
                "설치 파일이 너무 큽니다"
            );
            data.extend_from_slice(&chunk);
            let _ = events
                .send(Event::UpdateProgress {
                    downloaded: data.len() as u64,
                    total,
                })
                .await;
        }
        let key = PublicKey::decode(&String::from_utf8(STANDARD.decode(PUBLIC_KEY.trim())?)?)?;
        let signature = Signature::decode(&String::from_utf8(STANDARD.decode(update.asset.signature)?)?)?;
        verify_installer(self.installer, &data, &key, &signature, &expected)?;

        let mut file = tempfile::Builder::new()
            .prefix("better-sonar-update-")
            .suffix(self.installer.extension())
            .tempfile()?;
        file.write_all(&data)?;
        file.as_file().sync_all()?;
        let path = file.into_temp_path().keep()?;
        if let Err(error) = self.installer.launch(&path) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok(())
    }
}

fn select_update(manifest: Manifest, installer: InstallerKind, current: &Version) -> Result<Option<Update>> {
    let version = Version::parse(manifest.version.trim_start_matches('v'))?;
    if &version <= current {
        return Ok(None);
    }
    let asset = manifest
        .platforms
        .get(installer.platform())
        .context("설치 방식에 맞는 업데이트 파일이 없습니다")?;
    let url = Url::parse(&asset.url)?;
    ensure!(
        asset.url.starts_with(&format!("{ASSET_PREFIX}v{version}/"))
            && url.path().ends_with(installer.extension())
            && url.query().is_none()
            && url.fragment().is_none(),
        "업데이트 경로가 올바르지 않습니다"
    );
    Ok(Some(Update {
        version,
        notes: manifest.notes,
        asset: asset.clone(),
    }))
}

fn verify_installer(
    installer: InstallerKind,
    bytes: &[u8],
    key: &PublicKey,
    signature: &Signature,
    version: &Version,
) -> Result<()> {
    key.verify(bytes, signature, false)
        .context("업데이트 서명이 올바르지 않습니다")?;
    ensure!(
        signature.trusted_comment() == format!("version:{version}"),
        "서명에 기록된 버전이 일치하지 않습니다"
    );
    let expected_format = match installer {
        InstallerKind::Nsis => bytes.starts_with(b"MZ"),
        InstallerKind::Msi => bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
    };
    ensure!(expected_format, "설치 파일 형식이 올바르지 않습니다");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_requires_matching_bytes_version_and_format() {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let public = PublicKey::decode(&pair.pk.to_box().unwrap().to_string()).unwrap();
        let payload = b"MZinstaller";
        let signed = minisign::sign(
            Some(&pair.pk),
            &pair.sk,
            std::io::Cursor::new(payload),
            Some("version:0.0.8"),
            None,
        )
        .unwrap();
        let signature = Signature::decode(&signed.to_string()).unwrap();

        let version = Version::new(0, 0, 8);
        assert!(verify_installer(InstallerKind::Nsis, payload, &public, &signature, &version).is_ok());
        assert!(verify_installer(InstallerKind::Nsis, b"MZmodified", &public, &signature, &version).is_err());
        assert!(verify_installer(InstallerKind::Msi, payload, &public, &signature, &version).is_err());
        assert!(
            verify_installer(
                InstallerKind::Nsis,
                payload,
                &public,
                &signature,
                &Version::new(0, 0, 9)
            )
            .is_err()
        );
    }

    #[test]
    fn msi_update_uses_its_own_asset_and_refuses_a_wrong_path() {
        let mut manifest: Manifest = serde_json::from_value(serde_json::json!({
            "version": "0.0.8",
            "platforms": {
                "windows-x86_64": { "url": format!("{ASSET_PREFIX}v0.0.8/setup.exe"), "signature": "nsis" },
                "windows-x86_64-msi": { "url": format!("{ASSET_PREFIX}v0.0.8/setup.msi"), "signature": "msi" }
            }
        }))
        .unwrap();
        let current = Version::new(0, 0, 7);
        manifest.platforms.get_mut("windows-x86_64-msi").unwrap().url = format!("{ASSET_PREFIX}v0.0.7/setup.msi");
        assert!(select_update(manifest, InstallerKind::Msi, &current).is_err());
    }
}
