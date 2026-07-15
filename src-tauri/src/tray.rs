use tauri::{
    AppHandle, Manager,
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
};

use crate::{
    commands,
    models::{AppSnapshot, AudioDevice, ConnectionStatus},
    state::AppRuntime,
};

const PERSONAL_PREFIX: &str = "personal-device:";
const STREAM_PREFIX: &str = "stream-device:";
const MIC_PREFIX: &str = "mic-device:";

fn device_submenu(
    app: &AppHandle,
    label: &str,
    prefix: &str,
    devices: &[AudioDevice],
    current_device_id: Option<&str>,
    connected: bool,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let active_devices = devices
        .iter()
        .filter(|device| device.state == "active")
        .collect::<Vec<_>>();
    let items = active_devices
        .iter()
        .map(|device| {
            CheckMenuItem::with_id(
                app,
                format!("{prefix}{}", urlencoding::encode(&device.id)),
                &device.name,
                connected,
                current_device_id == Some(device.id.as_str()),
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;

    if items.is_empty() {
        let empty = MenuItem::with_id(
            app,
            format!("{prefix}empty"),
            if connected {
                "사용 가능한 장치 없음"
            } else {
                "Sonar 연결 확인 중…"
            },
            false,
            None::<&str>,
        )?;
        return Submenu::with_items(app, label, false, &[&empty]);
    }

    let item_refs = items
        .iter()
        .map(|item| item as &dyn IsMenuItem<tauri::Wry>)
        .collect::<Vec<_>>();
    Submenu::with_items(app, label, connected, &item_refs)
}

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
    let connected = snapshot.is_some_and(|value| value.status == ConnectionStatus::Connected);
    let personal = device_submenu(
        app,
        "개인 믹스",
        PERSONAL_PREFIX,
        snapshot.map(|value| value.devices.as_slice()).unwrap_or_default(),
        snapshot.and_then(|value| value.personal_device_id.as_deref()),
        connected,
    )?;
    let stream = device_submenu(
        app,
        "스트림 믹스",
        STREAM_PREFIX,
        snapshot.map(|value| value.devices.as_slice()).unwrap_or_default(),
        snapshot.and_then(|value| value.stream_device_id.as_deref()),
        connected,
    )?;
    let mic = device_submenu(
        app,
        "마이크 입력",
        MIC_PREFIX,
        snapshot.map(|value| value.input_devices.as_slice()).unwrap_or_default(),
        snapshot.and_then(|value| value.mic_device_id.as_deref()),
        connected,
    )?;
    let show = MenuItem::with_id(app, "show", "Better Sonar 열기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let device_separator = PredefinedMenuItem::separator(app)?;
    let app_separator = PredefinedMenuItem::separator(app)?;
    Menu::with_items(
        app,
        &[
            &status,
            &toggle,
            &device_separator,
            &personal,
            &stream,
            &mic,
            &app_separator,
            &show,
            &quit,
        ],
    )
}

fn device_id_from_event(event_id: &str, prefix: &str) -> Option<String> {
    let encoded = event_id.strip_prefix(prefix)?;
    urlencoding::decode(encoded).ok().map(|value| value.into_owned())
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = create_menu(app, None)?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("Better Sonar")
        .icon(app.default_window_icon().cloned().expect("앱 아이콘이 필요합니다"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let event_id = event.id.as_ref();
            match event_id {
                "toggle" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = commands::toggle_and_publish(&app).await;
                    });
                }
                "show" => show_window(app),
                "quit" => app.exit(0),
                _ => {
                    let operation = if let Some(device_id) = device_id_from_event(event_id, PERSONAL_PREFIX) {
                        Some((device_id, PERSONAL_PREFIX))
                    } else if let Some(device_id) = device_id_from_event(event_id, STREAM_PREFIX) {
                        Some((device_id, STREAM_PREFIX))
                    } else {
                        device_id_from_event(event_id, MIC_PREFIX).map(|device_id| (device_id, MIC_PREFIX))
                    };
                    let Some((device_id, prefix)) = operation else {
                        return;
                    };
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match prefix {
                            PERSONAL_PREFIX => {
                                let _ = commands::set_personal_output_and_publish(&app, &device_id).await;
                            }
                            STREAM_PREFIX => {
                                let _ = commands::set_stream_output_and_publish(&app, &device_id).await;
                            }
                            MIC_PREFIX => {
                                let _ = commands::set_mic_input_and_publish(&app, &device_id).await;
                            }
                            _ => {}
                        }
                    });
                }
            }
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
