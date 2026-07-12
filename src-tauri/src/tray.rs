use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

use crate::{commands, models::AppSnapshot, state::AppRuntime};

fn create_menu(app: &AppHandle, snapshot: Option<&AppSnapshot>) -> tauri::Result<Menu<tauri::Wry>> {
    let status_text = snapshot
        .and_then(|value| value.personal_device_name.as_deref())
        .map(|name| format!("현재: {name}"))
        .unwrap_or_else(|| {
            snapshot
                .map(|value| value.message.clone())
                .unwrap_or_else(|| "연결 상태 확인 중…".into())
        });
    let status = MenuItem::with_id(app, "status", status_text, false, None::<&str>)?;
    let toggle_text = snapshot
        .map(|value| {
            if value.personal_device_id == value.settings.headset_device_id {
                "스피커로 전환"
            } else {
                "헤드셋으로 전환"
            }
        })
        .unwrap_or("헤드셋 ↔ 스피커 전환");
    let can_toggle = snapshot.is_some_and(|value| {
        value.status == crate::models::ConnectionStatus::Connected
            && value.settings.headset_device_id.is_some()
            && value.settings.speaker_device_id.is_some()
    });
    let toggle = MenuItem::with_id(app, "toggle", toggle_text, can_toggle, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Better Sonar 열기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&status, &toggle, &separator, &show, &quit])
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = create_menu(app, None)?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("Better Sonar")
        .icon(app.default_window_icon().cloned().expect("앱 아이콘이 필요합니다"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = commands::toggle_and_publish(&app).await;
                });
            }
            "show" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub async fn update(app: &AppHandle) {
    let runtime = app.state::<std::sync::Arc<AppRuntime>>();
    let snapshot = runtime.snapshot().await;
    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(menu) = create_menu(app, Some(&snapshot)) {
            let _ = tray.set_menu(Some(menu));
        }
        let tooltip = match snapshot.personal_device_name {
            Some(name) => format!("Better Sonar · {name}"),
            None => format!("Better Sonar · {}", snapshot.message),
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}
