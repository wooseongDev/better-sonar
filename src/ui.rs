use async_channel::Sender;
use gpui::{AppContext, Context, Entity, SharedString, Subscription, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable, StyledExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};

use crate::{
    app::{Action, Event},
    models::{AppSettings, AppSnapshot, ConnectionStatus},
    updates::Update,
};

pub struct ViewModel {
    pub snapshot: AppSnapshot,
    pub notice: Option<String>,
    pub update: Option<Update>,
    pub checking_update: bool,
    pub installing: bool,
    pub downloaded: u64,
    pub total: Option<u64>,
}

impl ViewModel {
    pub fn new(snapshot: AppSnapshot) -> Self {
        Self {
            snapshot,
            notice: None,
            update: None,
            checking_update: false,
            installing: false,
            downloaded: 0,
            total: None,
        }
    }
}

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

pub struct MainView {
    model: Entity<ViewModel>,
    events: Sender<Event>,
    draft: AppSettings,
    shortcut: Entity<InputState>,
    headset: DeviceSelect,
    speaker: DeviceSelect,
    devices: Vec<DeviceChoice>,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    pub fn new(model: Entity<ViewModel>, events: Sender<Event>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let draft = model.read(cx).snapshot.settings.clone();
        let shortcut = cx.new(|cx| InputState::new(window, cx).default_value(draft.shortcut.clone()));
        let headset = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let speaker = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let subscriptions = vec![
            cx.observe_in(&model, window, |view, _, window, cx| {
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
            model,
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
            self.model
                .read(cx)
                .snapshot
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
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.read(cx);
        let snapshot = &model.snapshot;
        let mut draft = self.draft.clone();
        draft.shortcut = self.shortcut.read(cx).value().trim().to_owned();
        let changed = draft != snapshot.settings;
        let connected = snapshot.status == ConnectionStatus::Connected;
        let can_toggle =
            connected && !changed && draft.headset_device_id.is_some() && draft.speaker_device_id.is_some();
        let toggle_label = if snapshot.personal_device_id.is_some()
            && snapshot.personal_device_id == snapshot.settings.headset_device_id
        {
            "스피커로 전환"
        } else {
            "헤드셋으로 전환"
        };
        let theme = cx.theme();
        let title = |text| div().text_lg().font_semibold().child(text);
        let hint = |text: SharedString| div().text_sm().text_color(theme.muted_foreground).child(text);
        let panel = || {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.group_box)
                .w_full()
        };
        let stream_device = snapshot
            .stream_device_name
            .as_deref()
            .or(snapshot.stream_device_id.as_deref().filter(|id| !id.is_empty()))
            .unwrap_or("장치 확인 불가");

        let mut devices =
            panel()
                .child(title("Personal Mix"))
                .child(
                    div().text_xl().font_semibold().child(
                        snapshot
                            .personal_device_name
                            .clone()
                            .unwrap_or_else(|| "장치 확인 불가".into()),
                    ),
                )
                .child(hint(snapshot.message.clone().into()))
                .when(connected, |panel| {
                    panel.child(hint(format!("Stream Mix 출력: {stream_device}").into()))
                })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("toggle")
                                .primary()
                                .label(toggle_label)
                                .disabled(!can_toggle)
                                .on_click(cx.listener(|view, _, _, _| {
                                    let _ = view.events.try_send(Event::Sonar(Action::Toggle));
                                })),
                        )
                        .child(Button::new("refresh").label("Sonar 연결 확인").on_click(cx.listener(
                            |view, _, _, _| {
                                let _ = view.events.try_send(Event::Sonar(Action::Refresh));
                            },
                        ))),
                );
        if connected {
            for (key, label, id) in [
                ("use-headset", "헤드셋 선택", draft.headset_device_id.clone()),
                ("use-speaker", "스피커 선택", draft.speaker_device_id.clone()),
            ] {
                if let Some(id) = id {
                    let available = snapshot
                        .devices
                        .iter()
                        .any(|device| device.id == id && device.state == "active");
                    devices = devices.child(Button::new(key).ghost().label(label).disabled(!available).on_click(
                        cx.listener(move |view, _, _, _| {
                            let _ = view.events.try_send(Event::Sonar(Action::Personal(id.clone())));
                        }),
                    ));
                }
            }
        }

        let settings = panel()
            .child(title("전환할 두 장치 선택"))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(hint("헤드셋".into()))
                            .child(Select::new(&self.headset)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(hint("스피커".into()))
                            .child(Select::new(&self.speaker)),
                    ),
            )
            .child(hint("글로벌 단축키".into()))
            .child(Input::new(&self.shortcut))
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
            );

        let mut updates = panel()
            .child(title("업데이트"))
            .child(hint(format!("현재 버전 {}", env!("CARGO_PKG_VERSION")).into()))
            .child(
                Button::new("check-update")
                    .label(if model.checking_update {
                        "확인 중…"
                    } else {
                        "업데이트 확인"
                    })
                    .disabled(model.checking_update || model.installing)
                    .on_click(cx.listener(|view, _, _, _| {
                        let _ = view.events.try_send(Event::CheckUpdate { manual: true });
                    })),
            );
        if let Some(update) = &model.update {
            let version = update.version.clone();
            updates = updates.child(div().font_semibold().child(format!("새 버전 {version}")));
            if let Some(notes) = &update.notes {
                updates = updates.child(hint(notes.clone().into()));
            }
            updates = updates.child(
                Button::new("install")
                    .primary()
                    .label("다운로드 및 설치")
                    .disabled(model.installing || model.checking_update)
                    .on_click(cx.listener(move |view, _, _, _| {
                        let _ = view.events.try_send(Event::Install(version.clone()));
                    })),
            );
        }
        if model.installing {
            let progress = match model.total {
                Some(total) if total > 0 => format!("다운로드 중 {}%", (model.downloaded * 100 / total).min(100)),
                _ => format!("다운로드 중 {} KiB", model.downloaded / 1024),
            };
            updates = updates.child(hint(progress.into()));
        }

        div()
            .id("main")
            .size_full()
            .overflow_y_scroll()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_5()
                    .child(div().text_2xl().font_semibold().child("Better Sonar"))
                    .child(devices)
                    .child(settings)
                    .child(updates)
                    .children(
                        model
                            .notice
                            .as_ref()
                            .map(|notice| div().p_3().rounded_md().bg(theme.muted).text_sm().child(notice.clone())),
                    )
                    .child(div().h(px(1.))),
            )
    }
}
