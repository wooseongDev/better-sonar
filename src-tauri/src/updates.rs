use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    stage: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

fn info(update: &tauri_plugin_updater::Update) -> UpdateInfo {
    UpdateInfo {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    }
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = updater.check().await.map_err(|error| error.to_string())?;
    Ok(update.as_ref().map(info))
}

#[tauri::command]
pub async fn install_update(app: AppHandle, expected_version: String) -> Result<(), String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "설치할 업데이트가 없습니다".to_string())?;

    if update.version != expected_version {
        return Err(format!(
            "확인한 버전({expected_version})과 현재 배포 버전({})이 다릅니다. 다시 확인해 주세요.",
            update.version
        ));
    }

    let progress_app = app.clone();
    let finish_app = app.clone();
    let downloaded = Arc::new(AtomicU64::new(0));
    let _ = app.emit(
        "update-progress",
        UpdateProgress {
            stage: "downloading",
            downloaded: 0,
            total: None,
        },
    );

    let progress_downloaded = downloaded.clone();
    let finish_downloaded = downloaded;
    update
        .download_and_install(
            move |chunk_length, total| {
                let downloaded =
                    progress_downloaded.fetch_add(chunk_length as u64, Ordering::Relaxed) + chunk_length as u64;
                let _ = progress_app.emit(
                    "update-progress",
                    UpdateProgress {
                        stage: "downloading",
                        downloaded,
                        total,
                    },
                );
            },
            move || {
                let downloaded = finish_downloaded.load(Ordering::Relaxed);
                let _ = finish_app.emit(
                    "update-progress",
                    UpdateProgress {
                        stage: "installing",
                        downloaded,
                        total: Some(downloaded),
                    },
                );
            },
        )
        .await
        .map_err(|error| format!("업데이트 검증 또는 설치에 실패했습니다: {error}"))?;

    app.restart();
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::UpdateInfo;

    #[test]
    fn update_info_uses_frontend_field_names() {
        let value = serde_json::to_value(UpdateInfo {
            current_version: "0.0.3".into(),
            version: "0.0.4".into(),
            notes: Some("변경 사항".into()),
            published_at: Some("2026-07-15T00:00:00Z".into()),
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "currentVersion": "0.0.3",
                "version": "0.0.4",
                "notes": "변경 사항",
                "publishedAt": "2026-07-15T00:00:00Z"
            })
        );
    }
}
