use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, ensure};
use async_channel::{Receiver, Sender};
use auto_launch::AutoLaunch;
use global_hotkey::GlobalHotKeyEvent;
use gpui::{App, AppContext, Bounds, Context, Entity, WindowBounds, WindowHandle, WindowOptions, px, size};
use gpui_component::Root;
use semver::Version;
use tokio::{runtime::Handle, sync::mpsc};
use tray_icon::{TrayIconEvent, menu::MenuEvent};

use crate::{
    models::{AppSettings, AppSnapshot},
    settings,
    shortcuts::Hotkeys,
    state::AppRuntime,
    tray::{self, Tray, TrayAction},
    ui::{MainView, ViewModel},
    updates::{Update, Updater},
    windows,
};

pub enum Action {
    Refresh,
    Toggle,
    Personal(String),
    Stream(String),
    Mic(String),
    Settings(AppSettings),
}

pub enum Event {
    Show,
    Hotkey(GlobalHotKeyEvent),
    Menu(String),
    Sonar(Action),
    Save(AppSettings),
    Snapshot(Box<AppSnapshot>),
    Notice(String),
    CheckUpdate {
        manual: bool,
    },
    UpdateChecked {
        manual: bool,
        result: Result<Option<Update>>,
    },
    Install(Version),
    UpdateProgress {
        downloaded: u64,
        total: Option<u64>,
    },
    UpdateFinished(Result<()>),
}

pub struct Desktop {
    events: Sender<Event>,
    actions: mpsc::UnboundedSender<Action>,
    runtime: Arc<AppRuntime>,
    executor: Handle,
    model: Entity<ViewModel>,
    window: Option<WindowHandle<Root>>,
    tray: Tray,
    hotkeys: Hotkeys,
    settings_path: PathBuf,
    settings: AppSettings,
    autostart: AutoLaunch,
    updater: Updater,
}

impl Desktop {
    pub fn start(events: Sender<Event>, receiver: Receiver<Event>, executor: Handle, cx: &mut App) -> Result<()> {
        let executable = std::env::current_exe()?;
        let installation = windows::installer_kind(&executable);
        let settings_path = settings::settings_path(installation.is_some())?;
        let autostart = windows::autostart(&executable, installation.is_some())?;
        let mut initial_settings = settings::load(&settings_path);
        if let Ok(enabled) = autostart.is_enabled() {
            if initial_settings.autostart != enabled {
                initial_settings.autostart = enabled;
                settings::save(&settings_path, &initial_settings)?;
            }
            if enabled {
                autostart.enable()?;
            }
        }

        let runtime = AppRuntime::new(initial_settings.clone())?;
        let initial_snapshot = executor.block_on(runtime.snapshot());
        let model = cx.new(|_| ViewModel::new(initial_snapshot));
        let updater = Updater::new(installation.unwrap_or(windows::InstallerKind::Nsis))?;
        let tray = Tray::new()?;
        let mut hotkeys = Hotkeys::new()?;
        if let Err(message) = hotkeys.set_user(&initial_settings.shortcut) {
            let _ = events.try_send(Event::Notice(message));
        }
        if let Some(message) = hotkeys.set_media(initial_settings.media_keys_enabled) {
            let _ = events.try_send(Event::Notice(message));
        }

        let (actions, requests) = mpsc::unbounded_channel();
        executor.spawn(run_sonar(runtime.clone(), requests, events.clone()));
        executor.spawn(check_updates_periodically(events.clone()));
        let send = events.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |key| {
            let _ = send.try_send(Event::Hotkey(key));
        }));
        let send = events.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = send.try_send(Event::Menu(event.id.0));
        }));
        let send = events.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                let _ = send.try_send(Event::Show);
            }
        }));

        let desktop = cx.new(|_| Self {
            events,
            actions,
            runtime,
            executor,
            model,
            window: None,
            tray,
            hotkeys,
            settings_path,
            settings: initial_settings,
            autostart,
            updater,
        });
        cx.spawn(async move |cx| {
            while let Ok(event) = receiver.recv().await {
                if desktop.update(cx, |desktop, cx| desktop.handle(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        Ok(())
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Show => self.show(cx),
            Event::Hotkey(key) => {
                if self.hotkeys.handle(key, &self.runtime, &self.executor, &self.events) {
                    let _ = self.actions.send(Action::Toggle);
                }
            }
            Event::Menu(id) => match tray::menu_action(&id) {
                Some(TrayAction::Show) => self.show(cx),
                Some(TrayAction::Quit) => cx.quit(),
                Some(TrayAction::Sonar(action)) => {
                    let _ = self.actions.send(action);
                }
                None => {}
            },
            Event::Sonar(action) => {
                let _ = self.actions.send(action);
            }
            Event::Save(settings) => {
                let message = match self.save(settings) {
                    Ok(warning) => warning.unwrap_or_else(|| "설정을 저장했습니다".into()),
                    Err(error) => format!("설정을 저장하지 못했습니다: {error:#}"),
                };
                self.notice(message, cx);
            }
            Event::Snapshot(snapshot) => {
                if let Err(error) = self.tray.update(&snapshot) {
                    eprintln!("트레이를 갱신할 수 없습니다: {error:#}");
                }
                self.model.update(cx, |model, cx| {
                    model.snapshot = *snapshot;
                    cx.notify();
                });
            }
            Event::Notice(message) => self.notice(message, cx),
            Event::CheckUpdate { manual } => self.check_update(manual, cx),
            Event::UpdateChecked { manual, result } => self.model.update(cx, |model, cx| {
                model.checking_update = false;
                match result {
                    Ok(update) => {
                        if let Some(update) = &update {
                            model.notice =
                                Some(format!("Better Sonar {} 업데이트를 사용할 수 있습니다", update.version));
                        } else if manual {
                            model.notice = Some("최신 버전을 사용하고 있습니다".into());
                        }
                        model.update = update;
                    }
                    Err(error) if manual => model.notice = Some(format!("업데이트 확인에 실패했습니다: {error:#}")),
                    Err(_) => {}
                }
                cx.notify();
            }),
            Event::Install(version) => self.install_update(version, cx),
            Event::UpdateProgress { downloaded, total } => self.model.update(cx, |model, cx| {
                model.downloaded = downloaded;
                model.total = total;
                cx.notify();
            }),
            Event::UpdateFinished(Ok(())) => cx.quit(),
            Event::UpdateFinished(Err(error)) => self.model.update(cx, |model, cx| {
                model.installing = false;
                model.notice = Some(format!("업데이트 설치에 실패했습니다: {error:#}"));
                cx.notify();
            }),
        }
    }

    fn show(&mut self, cx: &mut App) {
        if let Some(handle) = self.window
            && handle.update(cx, |_, window, _| windows::show(window)).is_ok()
        {
            return;
        }
        let model = self.model.clone();
        let events = self.events.clone();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(620.), px(760.)),
                cx,
            ))),
            window_min_size: Some(size(px(520.), px(630.))),
            ..WindowOptions::default()
        };
        match cx.open_window(options, |window, cx| {
            window.set_window_title(windows::PRODUCT_NAME);
            window.on_window_should_close(cx, |window, _| {
                windows::hide(window);
                false
            });
            let view = cx.new(|cx| MainView::new(model, events, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            Ok(handle) => {
                self.window = Some(handle);
                cx.activate(true);
            }
            Err(error) => self.notice(format!("창을 열 수 없습니다: {error:#}"), cx),
        }
    }

    fn save(&mut self, settings: AppSettings) -> Result<Option<String>> {
        ensure!(
            settings.headset_device_id.is_none() || settings.headset_device_id != settings.speaker_device_id,
            "헤드셋과 스피커는 서로 다른 장치여야 합니다"
        );
        let old = &self.settings;
        self.hotkeys.set_user(&settings.shortcut).map_err(anyhow::Error::msg)?;
        let autostart_changed = settings.autostart != old.autostart;
        let result = (|| {
            if autostart_changed {
                windows::set_autostart(&self.autostart, settings.autostart)?;
            }
            settings::save(&self.settings_path, &settings)
        })();
        if let Err(error) = result {
            let shortcut = self.hotkeys.set_user(&old.shortcut);
            let startup = if autostart_changed {
                windows::set_autostart(&self.autostart, old.autostart)
            } else {
                Ok(())
            };
            if shortcut.is_err() || startup.is_err() {
                anyhow::bail!("{error:#}. 이전 설정 복구에 실패했습니다. 단축키: {shortcut:?}, 자동 시작: {startup:?}");
            }
            return Err(error);
        }
        let warning = self.hotkeys.set_media(settings.media_keys_enabled);
        self.settings = settings.clone();
        let _ = self.actions.send(Action::Settings(settings));
        Ok(warning)
    }

    fn check_update(&self, manual: bool, cx: &mut App) {
        if self.model.read(cx).checking_update || self.model.read(cx).installing {
            return;
        }
        self.model.update(cx, |model, cx| {
            model.checking_update = true;
            cx.notify();
        });
        let events = self.events.clone();
        let updater = self.updater.clone();
        self.executor.spawn(async move {
            let result = updater.check().await;
            let _ = events.send(Event::UpdateChecked { manual, result }).await;
        });
    }

    fn install_update(&self, version: Version, cx: &mut App) {
        if self.model.read(cx).installing || self.model.read(cx).checking_update {
            return;
        }
        self.model.update(cx, |model, cx| {
            model.installing = true;
            model.downloaded = 0;
            model.total = None;
            cx.notify();
        });
        let events = self.events.clone();
        let updater = self.updater.clone();
        self.executor.spawn(async move {
            let result = updater.install(version, &events).await;
            let _ = events.send(Event::UpdateFinished(result)).await;
        });
    }

    fn notice(&self, message: String, cx: &mut App) {
        eprintln!("{message}");
        self.model.update(cx, |model, cx| {
            model.notice = Some(message);
            cx.notify();
        });
    }
}

async fn run_sonar(runtime: Arc<AppRuntime>, mut actions: mpsc::UnboundedReceiver<Action>, events: Sender<Event>) {
    let mut refresh = tokio::time::interval(Duration::from_secs(2));
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let action = tokio::select! {
            _ = refresh.tick() => Action::Refresh,
            action = actions.recv() => match action {
                Some(action) => action,
                None => break,
            }
        };
        let result = match action {
            Action::Refresh => Ok(runtime.refresh().await),
            Action::Toggle => runtime.toggle().await,
            Action::Personal(device) => runtime.set_output(&device).await,
            Action::Stream(device) => runtime.set_stream_output(&device).await,
            Action::Mic(device) => runtime.set_mic_input(&device).await,
            Action::Settings(settings) => Ok(runtime.apply_settings(settings).await),
        };
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = events.send(Event::Notice(error)).await;
                runtime.refresh().await
            }
        };
        if events.send(Event::Snapshot(Box::new(snapshot))).await.is_err() {
            break;
        }
    }
}

async fn check_updates_periodically(events: Sender<Event>) {
    tokio::time::sleep(Duration::from_secs(10)).await;
    while events.send(Event::CheckUpdate { manual: false }).await.is_ok() {
        tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
    }
}
