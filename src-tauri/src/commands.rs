use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    models::{AppSettings, AppSnapshot},
    shortcuts,
    state::AppRuntime,
    tray,
};

pub async fn publish(app: &AppHandle, snapshot: &AppSnapshot) {
    let _ = app.emit("sonar-state", snapshot);
    tray::update(app).await;
}

pub async fn toggle_and_publish(app: &AppHandle) -> Result<AppSnapshot, String> {
    let runtime = app.state::<Arc<AppRuntime>>();
    let result = runtime.toggle().await;
    match &result {
        Ok(snapshot) => publish(app, snapshot).await,
        Err(message) => {
            let _ = app.emit("sonar-error", message);
            let snapshot = runtime.refresh().await;
            publish(app, &snapshot).await;
        }
    }
    result
}

#[tauri::command]
pub async fn get_snapshot(runtime: State<'_, Arc<AppRuntime>>) -> Result<AppSnapshot, String> {
    Ok(runtime.snapshot().await)
}

#[tauri::command]
pub async fn refresh_state(app: AppHandle, runtime: State<'_, Arc<AppRuntime>>) -> Result<AppSnapshot, String> {
    let snapshot = runtime.refresh().await;
    publish(&app, &snapshot).await;
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_output(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    device_id: String,
) -> Result<AppSnapshot, String> {
    let snapshot = runtime.set_output(&device_id).await?;
    publish(&app, &snapshot).await;
    Ok(snapshot)
}

#[tauri::command]
pub async fn toggle_output(app: AppHandle) -> Result<AppSnapshot, String> {
    toggle_and_publish(&app).await
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    settings: AppSettings,
) -> Result<AppSnapshot, String> {
    let old = runtime.snapshot().await.settings;
    if old.shortcut != settings.shortcut
        && let Err(error) = shortcuts::apply(&app, &settings.shortcut)
    {
        let _ = shortcuts::apply(&app, &old.shortcut);
        return Err(error);
    }
    if old.autostart != settings.autostart {
        let autostart_result = if settings.autostart {
            app.autolaunch().enable().map_err(|error| error.to_string())
        } else {
            app.autolaunch().disable().map_err(|error| error.to_string())
        };
        if let Err(error) = autostart_result {
            let _ = shortcuts::apply(&app, &old.shortcut);
            return Err(error);
        }
    }
    match runtime.save_settings(settings).await {
        Ok(snapshot) => {
            publish(&app, &snapshot).await;
            Ok(snapshot)
        }
        Err(error) => {
            let _ = shortcuts::apply(&app, &old.shortcut);
            if old.autostart {
                let _ = app.autolaunch().enable();
            } else {
                let _ = app.autolaunch().disable();
            }
            Err(error)
        }
    }
}
