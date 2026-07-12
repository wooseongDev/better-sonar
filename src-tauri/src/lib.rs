mod commands;
mod discovery;
mod models;
mod settings;
mod shortcuts;
mod sonar_client;
mod state;
mod tray;

use std::{sync::Arc, time::Duration};

use state::AppRuntime;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show_window(app);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = commands::toggle_and_publish(&app).await;
                        });
                    }
                })
                .build(),
        )
        .setup(|app| {
            let app_handle = app.handle().clone();
            let path = settings::settings_path(&app_handle)?;
            let mut initial_settings = settings::load(&path);
            if let Ok(actual_autostart) = app_handle.autolaunch().is_enabled()
                && initial_settings.autostart != actual_autostart
            {
                initial_settings.autostart = actual_autostart;
                let _ = settings::save(&path, &initial_settings);
            }
            let runtime = AppRuntime::new(path, initial_settings.clone())?;
            app.manage(runtime.clone());
            tray::build(&app_handle)?;
            if let Err(error) = shortcuts::apply(&app_handle, &initial_settings.shortcut) {
                let _ = app_handle.emit("sonar-error", error);
            }

            tauri::async_runtime::spawn(background_refresh(app_handle, runtime));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::refresh_state,
            commands::set_output,
            commands::toggle_output,
            commands::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("Better Sonar 실행 중 오류가 발생했습니다");
}

async fn background_refresh(app: tauri::AppHandle, runtime: Arc<AppRuntime>) {
    loop {
        let snapshot = runtime.refresh().await;
        commands::publish(&app, &snapshot).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
