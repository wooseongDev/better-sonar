use async_channel::Sender;
use gpui::{App, AppContext, Context, Entity, SharedString, Subscription, Window, div, prelude::*, px};
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};

use crate::{
    app::Event,
    features::sonar::AppSnapshot,
    views::{hint, panel, title},
};

use super::AppSettings;

#[derive(Clone, PartialEq, Eq)]
struct DeviceChoice {
    id: Option<String>,
    name: SharedString,
}

impl SelectItem for DeviceChoice {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

type DeviceSelect = Entity<SelectState<Vec<DeviceChoice>>>;

pub(crate) struct SettingsForm {
    snapshot: Entity<AppSnapshot>,
    events: Sender<Event>,
    draft: AppSettings,
    shortcut: Entity<InputState>,
    headset: DeviceSelect,
    speaker: DeviceSelect,
    devices: Vec<DeviceChoice>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsForm {
    pub(crate) fn new(
        snapshot: Entity<AppSnapshot>,
        events: Sender<Event>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = snapshot.read(cx).settings.clone();
        let shortcut = cx.new(|cx| InputState::new(window, cx).default_value(draft.shortcut.clone()));
        let headset = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let speaker = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let subscriptions = vec![
            cx.observe_in(&snapshot, window, |view, _, window, cx| {
                view.sync_devices(window, cx);
                cx.notify();
            }),
            cx.observe(&shortcut, |_, _, cx| cx.notify()),
            cx.subscribe(&headset, |view, _, event, cx| {
                let SelectEvent::Confirm(value) = event;
                view.draft.headset_device_id = value.clone().flatten();
                cx.notify();
            }),
            cx.subscribe(&speaker, |view, _, event, cx| {
                let SelectEvent::Confirm(value) = event;
                view.draft.speaker_device_id = value.clone().flatten();
                cx.notify();
            }),
        ];
        let mut view = Self {
            snapshot,
            events,
            draft,
            shortcut,
            headset,
            speaker,
            devices: Vec::new(),
            _subscriptions: subscriptions,
        };
        view.sync_devices(window, cx);
        view
    }

    fn sync_devices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut devices = vec![DeviceChoice {
            id: None,
            name: "선택 안 함".into(),
        }];
        devices.extend(
            self.snapshot
                .read(cx)
                .devices
                .iter()
                .filter(|device| device.state == "active")
                .map(|device| DeviceChoice {
                    id: Some(device.id.clone()),
                    name: device.name.clone().into(),
                }),
        );
        for id in [&self.draft.headset_device_id, &self.draft.speaker_device_id]
            .into_iter()
            .flatten()
        {
            if !devices.iter().any(|device| device.id.as_ref() == Some(id)) {
                devices.push(DeviceChoice {
                    id: Some(id.clone()),
                    name: "연결이 끊긴 저장 장치".into(),
                });
            }
        }
        if devices == self.devices {
            return;
        }
        for (select, id) in [
            (&self.headset, &self.draft.headset_device_id),
            (&self.speaker, &self.draft.speaker_device_id),
        ] {
            select.update(cx, |select, cx| {
                select.set_items(devices.clone(), window, cx);
                select.set_selected_value(id, window, cx);
                cx.notify();
            });
        }
        self.devices = devices;
    }

    pub(crate) fn draft(&self, cx: &App) -> AppSettings {
        let mut draft = self.draft.clone();
        draft.shortcut = self.shortcut.read(cx).value().trim().to_owned();
        draft.theme = self.snapshot.read(cx).settings.theme;
        draft
    }
}

impl Render for SettingsForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.draft(cx);
        let changed = draft != self.snapshot.read(cx).settings;
        panel(cx)
            .child(title("장치 전환 설정"))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(hint(cx, "헤드셋".into()))
                            .child(Select::new(&self.headset)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(hint(cx, "스피커".into()))
                            .child(Select::new(&self.speaker)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(hint(cx, "글로벌 단축키".into()))
                    .child(div().w(px(208.)).child(Input::new(&self.shortcut))),
            )
            .child(
                Checkbox::new("autostart")
                    .label("Windows 시작 시 자동 실행")
                    .checked(draft.autostart)
                    .on_click(cx.listener(|view, checked, _, cx| {
                        view.draft.autostart = *checked;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("media")
                    .label("Fn 미디어 키 액션")
                    .checked(draft.media_keys_enabled)
                    .on_click(cx.listener(|view, checked, _, cx| {
                        view.draft.media_keys_enabled = *checked;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("save")
                    .primary()
                    .label("설정 저장")
                    .disabled(!changed)
                    .on_click(cx.listener(move |view, _, _, _| {
                        let _ = view.events.try_send(Event::Save(draft.clone()));
                    })),
            )
    }
}
