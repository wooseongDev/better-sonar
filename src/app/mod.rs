pub(crate) mod shortcuts;
mod tray;

use std::sync::Arc;

use anyhow::Result;
use async_channel::{Receiver, Sender};
use global_hotkey::GlobalHotKeyEvent;
use gpui::{App, AppContext, Bounds, Context, Entity, WindowBounds, WindowHandle, WindowOptions, px, size};
use gpui_component::Root;
use tokio::{runtime::Handle, sync::mpsc};
use tray_icon::{TrayIconEvent, menu::MenuEvent};

use crate::{
    features::{
        settings::{AppSettings, Settings, ThemePreference, apply_theme},
        sonar::{AppRuntime, AppSnapshot, SonarAction, run_sonar},
        updates::{UpdateEvent, Updates, check_periodically},
    },
    views::MainView,
    windows,
};

use self::{
    shortcuts::Hotkeys,
    tray::{Tray, TrayAction},
};

pub(crate) enum Event {
    Show,
    Hotkey(GlobalHotKeyEvent),
    Menu(String),
    Sonar(SonarAction),
    Save(AppSettings),
    SetTheme(ThemePreference),
    Snapshot(Box<AppSnapshot>),
    Notice(String),
    Update(UpdateEvent),
}

pub(crate) struct Desktop {
    events: Sender<Event>,
    actions: mpsc::UnboundedSender<SonarAction>,
    runtime: Arc<AppRuntime>,
    executor: Handle,
    snapshot: Entity<AppSnapshot>,
    notice: Entity<Option<String>>,
    updates: Entity<Updates>,
    window: Option<WindowHandle<Root>>,
    tray: Tray,
    hotkeys: Hotkeys,
    settings: Settings,
}

impl Desktop {
    pub fn start(events: Sender<Event>, receiver: Receiver<Event>, executor: Handle, cx: &mut App) -> Result<()> {
        let executable = std::env::current_exe()?;
        let installation = windows::installer_kind(&executable);
        let settings = Settings::load(&executable, installation.is_some())?;
        let runtime = AppRuntime::new(settings.current().clone())?;
        let initial_snapshot = executor.block_on(runtime.snapshot());
        let snapshot = cx.new(|_| initial_snapshot);
        let notice = cx.new(|_| None);
        let updates = Updates::new(installation.unwrap_or(windows::InstallerKind::Nsis))?;
        let updates = cx.new(|_| updates);
        let tray = Tray::new()?;
        let mut hotkeys = Hotkeys::new()?;
        if let Err(message) = hotkeys.set_user(&settings.current().shortcut) {
            let _ = events.try_send(Event::Notice(message));
        }
        if let Some(message) = hotkeys.set_media(settings.current().media_keys_enabled) {
            let _ = events.try_send(Event::Notice(message));
        }

        let (actions, requests) = mpsc::unbounded_channel();
        executor.spawn(run_sonar(runtime.clone(), requests, events.clone()));
        executor.spawn(check_periodically(events.clone()));
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
            snapshot,
            notice,
            updates,
            window: None,
            tray,
            hotkeys,
            settings,
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
                    let _ = self.actions.send(SonarAction::Toggle);
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
                let message = match self.settings.save(settings, &mut self.hotkeys) {
                    Ok(warning) => {
                        let _ = self
                            .actions
                            .send(SonarAction::Settings(self.settings.current().clone()));
                        warning.unwrap_or_else(|| "설정을 저장했습니다".into())
                    }
                    Err(error) => format!("설정을 저장하지 못했습니다: {error:#}"),
                };
                self.notice(message, cx);
            }
            Event::SetTheme(preference) => {
                if let Err(error) = self.set_theme(preference, cx) {
                    self.notice(format!("테마를 저장하지 못했습니다: {error:#}"), cx);
                }
            }
            Event::Snapshot(mut snapshot) => {
                snapshot.settings.theme = self.settings.current().theme;
                if let Err(error) = self.tray.update(&snapshot) {
                    eprintln!("트레이를 갱신할 수 없습니다: {error:#}");
                }
                self.snapshot.update(cx, |current, cx| {
                    *current = *snapshot;
                    cx.notify();
                });
            }
            Event::Notice(message) => self.notice(message, cx),
            Event::Update(event) => {
                let notice = self.updates.update(cx, |updates, cx| {
                    updates.handle(event, &self.events, &self.executor, cx)
                });
                if let Some(notice) = notice {
                    self.set_notice(notice, cx);
                }
            }
        }
    }

    fn show(&mut self, cx: &mut App) {
        if let Some(handle) = self.window
            && handle.update(cx, |_, window, _| windows::show(window)).is_ok()
        {
            return;
        }
        let snapshot = self.snapshot.clone();
        let updates = self.updates.clone();
        let notice = self.notice.clone();
        let events = self.events.clone();
        let preference = self.settings.current().theme;
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
            apply_theme(preference, window, cx);
            window.on_window_should_close(cx, |window, _| {
                windows::hide(window);
                false
            });
            let view = cx.new(|cx| MainView::new(snapshot, updates, notice, events, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            Ok(handle) => {
                self.window = Some(handle);
                cx.activate(true);
            }
            Err(error) => self.notice(format!("창을 열 수 없습니다: {error:#}"), cx),
        }
    }

    fn set_theme(&mut self, preference: ThemePreference, cx: &mut App) -> Result<()> {
        if !self.settings.set_theme(preference)? {
            return Ok(());
        }
        self.snapshot.update(cx, |snapshot, cx| {
            snapshot.settings.theme = preference;
            cx.notify();
        });
        if let Some(handle) = self.window {
            let _ = handle.update(cx, |_, window, cx| apply_theme(preference, window, cx));
        }
        Ok(())
    }

    fn notice(&self, message: String, cx: &mut App) {
        eprintln!("{message}");
        self.set_notice(message, cx);
    }

    fn set_notice(&self, message: String, cx: &mut App) {
        self.notice.update(cx, |notice, cx| {
            *notice = Some(message);
            cx.notify();
        });
    }
}
