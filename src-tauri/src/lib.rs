mod commands;
mod discovery;
#[cfg(all(windows, not(test)))]
mod gg_shortcuts;
mod models;
mod settings;
mod shortcuts;
mod sonar_client;
mod state;
mod tray;
mod updates;

use std::{ffi::OsStr, sync::Arc, time::Duration};

use state::AppRuntime;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

const AUTOSTART_ARG: &str = "--autostart";

fn has_autostart_arg(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> bool {
    args.into_iter().any(|arg| arg.as_ref() == OsStr::new(AUTOSTART_ARG))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show_window(app);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().arg(AUTOSTART_ARG).build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(shortcuts::handle_event)
                .build(),
        )
        .setup(|app| {
            let app_handle = app.handle().clone();
            let path = settings::settings_path(&app_handle)?;
            let mut initial_settings = settings::load(&path);
            if let Ok(actual_autostart) = app_handle.autolaunch().is_enabled() {
                if actual_autostart {
                    // 이전 버전에서 인자 없이 등록된 자동 실행 항목도 새 형식으로 갱신한다.
                    let _ = app_handle.autolaunch().enable();
                }
                if initial_settings.autostart != actual_autostart {
                    initial_settings.autostart = actual_autostart;
                    let _ = settings::save(&path, &initial_settings);
                }
            }
            let runtime = AppRuntime::new(path, initial_settings.clone())?;
            app.manage(runtime.clone());
            #[cfg(windows)]
            app.manage(shortcuts::MediaKeyRepeatState::default());
            tray::build(&app_handle)?;
            if !has_autostart_arg(std::env::args_os()) {
                tray::show_window(&app_handle);
            }
            if let Err(error) = shortcuts::register_user(&app_handle, &initial_settings.shortcut) {
                let _ = app_handle.emit("sonar-error", error);
            }
            #[cfg(windows)]
            if initial_settings.media_keys_enabled
                && let Err(error) = shortcuts::register_media_keys(&app_handle)
            {
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
            updates::check_for_update,
            updates::install_update,
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

#[cfg(test)]
mod tests {
    use super::has_autostart_arg;

    #[test]
    fn detects_autostart_launch_argument() {
        assert!(has_autostart_arg(["better-sonar", "--autostart"]));
        assert!(!has_autostart_arg(["better-sonar"]));
        assert!(!has_autostart_arg(["better-sonar", "--other"]));
    }
}
