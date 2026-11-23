use async_channel::Sender;
use gpui::{App, Div, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, StyledExt,
    button::{Button, ButtonVariants},
};

use crate::{
    app::Event,
    features::settings::AppSettings,
    views::{hint, panel, title},
};

use super::{AppSnapshot, ConnectionStatus, SonarAction};

pub(crate) fn device_panel(snapshot: &AppSnapshot, draft: &AppSettings, events: &Sender<Event>, cx: &App) -> Div {
    let changed = draft != &snapshot.settings;
    let connected = snapshot.status == ConnectionStatus::Connected;
    let can_toggle = connected && !changed && draft.headset_device_id.is_some() && draft.speaker_device_id.is_some();
    let toggle_label = if snapshot.personal_device_id.is_some()
        && snapshot.personal_device_id == snapshot.settings.headset_device_id
    {
        "스피커로 전환"
    } else {
        "헤드셋으로 전환"
    };
    let theme = cx.theme();
    let refresh = events.clone();
    let toggle = events.clone();
    let stream_device = snapshot
        .stream_device_name
        .as_deref()
        .or(snapshot.stream_device_id.as_deref().filter(|id| !id.is_empty()))
        .unwrap_or("장치 확인 불가");
    let mut device_actions =
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new("refresh")
                    .label("Sonar 연결 확인")
                    .on_click(move |_, _, _| {
                        let _ = refresh.try_send(Event::Sonar(SonarAction::Refresh));
                    }),
            );
    if connected {
        for (key, label, id) in [
            ("use-headset", "헤드셋 선택", draft.headset_device_id.clone()),
            ("use-speaker", "스피커 선택", draft.speaker_device_id.clone()),
        ] {
            if let Some(id) = id {
                let events = events.clone();
                let available = snapshot
                    .devices
                    .iter()
                    .any(|device| device.id == id && device.state == "active");
                device_actions =
                    device_actions.child(Button::new(key).ghost().label(label).disabled(!available).on_click(
                        move |_, _, _| {
                            let _ = events.try_send(Event::Sonar(SonarAction::Personal(id.clone())));
                        },
                    ));
            }
        }
    }

    panel(cx)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(title("Personal Mix"))
                .when(connected, |row| {
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_1()
                            .rounded_full()
                            .text_xs()
                            .bg(theme.success)
                            .text_color(theme.success_foreground)
                            .child(div().size(px(5.)).rounded_full().bg(theme.success_foreground))
                            .child("Sonar 연결됨"),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(hint(cx, "현재 출력".into()))
                .child(
                    div().text_xl().font_semibold().child(
                        snapshot
                            .personal_device_name
                            .clone()
                            .unwrap_or_else(|| "장치 확인 불가".into()),
                    ),
                ),
        )
        .when(
            !connected || snapshot.settings.has_missing_output(&snapshot.devices),
            |panel| panel.child(hint(cx, snapshot.message.clone().into())),
        )
        .child(
            div()
                .relative()
                .child(
                    Button::new("toggle")
                        .primary()
                        .w_full()
                        .h(px(36.))
                        .justify_start()
                        .label(toggle_label)
                        .disabled(!can_toggle)
                        .on_click(move |_, _, _| {
                            let _ = toggle.try_send(Event::Sonar(SonarAction::Toggle));
                        }),
                )
                .child(
                    div()
                        .absolute()
                        .right_4()
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(
                            Icon::new(IconName::ArrowRight)
                                .with_size(px(16.))
                                .text_color(if can_toggle {
                                    theme.primary_foreground
                                } else {
                                    theme.muted_foreground
                                }),
                        ),
                ),
        )
        .when(connected, |panel| {
            panel.child(
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .pt_2()
                    .text_sm()
                    .child(hint(cx, "Stream Mix 출력".into()))
                    .child(stream_device.to_owned()),
            )
        })
        .child(device_actions)
}
