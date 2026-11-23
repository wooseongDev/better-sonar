use async_channel::Sender;
use gpui::{App, Div, div, prelude::*};
use gpui_component::{
    Disableable, StyledExt,
    button::{Button, ButtonVariants},
};

use crate::{
    app::Event,
    views::{hint, panel, title},
};

use super::{UpdateEvent, Updates};

pub(crate) fn update_panel(model: &Updates, events: &Sender<Event>, cx: &App) -> Div {
    let check = events.clone();
    let mut updates = panel(cx)
        .child(title("업데이트"))
        .child(hint(cx, format!("현재 버전 {}", env!("CARGO_PKG_VERSION")).into()))
        .child(
            Button::new("check-update")
                .label(if model.checking_update {
                    "확인 중…"
                } else {
                    "업데이트 확인"
                })
                .disabled(model.checking_update || model.installing)
                .on_click(move |_, _, _| {
                    let _ = check.try_send(Event::Update(UpdateEvent::Check { manual: true }));
                }),
        );
    if let Some(update) = &model.update {
        let version = update.version.clone();
        let install = events.clone();
        updates = updates.child(div().font_semibold().child(format!("새 버전 {version}")));
        if let Some(notes) = &update.notes {
            updates = updates.child(hint(cx, notes.clone().into()));
        }
        updates = updates.child(
            Button::new("install")
                .primary()
                .label("다운로드 및 설치")
                .disabled(model.installing || model.checking_update)
                .on_click(move |_, _, _| {
                    let _ = install.try_send(Event::Update(UpdateEvent::Install(version.clone())));
                }),
        );
    }
    if model.installing {
        let progress = match model.total {
            Some(total) if total > 0 => format!("다운로드 중 {}%", (model.downloaded * 100 / total).min(100)),
            _ => format!("다운로드 중 {} KiB", model.downloaded / 1024),
        };
        updates = updates.child(hint(cx, progress.into()));
    }

    updates
}
