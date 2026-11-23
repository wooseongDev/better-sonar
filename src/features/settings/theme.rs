use async_channel::Sender;
use gpui::{App, Div, Window, div, prelude::*, rgb};
use gpui_component::{
    Theme, ThemeMode,
    button::{Button, ButtonVariants},
};

use crate::{
    app::Event,
    views::{panel, title},
    windows,
};

use super::ThemePreference;

pub(crate) fn apply(preference: ThemePreference, window: &mut Window, cx: &mut App) {
    let mode = match preference {
        ThemePreference::System => ThemeMode::from(window.appearance()),
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
    };
    Theme::change(mode, None, cx);
    let theme = Theme::global_mut(cx);
    let palette = if mode.is_dark() { DARK } else { LIGHT };
    let colors = &mut theme.colors;
    colors.background = rgb(palette.background).into();
    colors.foreground = rgb(palette.foreground).into();
    colors.group_box = rgb(palette.panel).into();
    colors.group_box_foreground = colors.foreground;
    colors.border = rgb(palette.border).into();
    colors.input = rgb(palette.input_border).into();
    colors.muted = colors.group_box;
    colors.muted_foreground = rgb(palette.muted_foreground).into();
    colors.primary = rgb(palette.primary).into();
    colors.primary_foreground = rgb(palette.primary_foreground).into();
    colors.primary_hover = rgb(palette.primary_hover).into();
    colors.primary_active = rgb(palette.primary_active).into();
    colors.secondary = colors.background;
    colors.secondary_foreground = colors.foreground;
    colors.secondary_hover = rgb(palette.item_hover).into();
    colors.secondary_active = colors.secondary_hover;
    colors.accent = colors.secondary_hover;
    colors.accent_foreground = colors.foreground;
    colors.ring = rgb(palette.ring).into();
    colors.selection = colors.ring.alpha(0.3);
    colors.popover = colors.group_box;
    colors.popover_foreground = colors.foreground;
    colors.list = colors.popover;
    colors.list_hover = colors.accent;
    colors.list_active = colors.accent;
    colors.list_active_border = colors.ring;
    colors.success = rgb(palette.success).into();
    colors.success_foreground = rgb(palette.success_foreground).into();
    windows::set_titlebar_colors(window, palette.background, palette.foreground);
    window.refresh();
}

struct Palette {
    background: u32,
    panel: u32,
    foreground: u32,
    muted_foreground: u32,
    border: u32,
    input_border: u32,
    primary: u32,
    primary_foreground: u32,
    primary_hover: u32,
    primary_active: u32,
    item_hover: u32,
    ring: u32,
    success: u32,
    success_foreground: u32,
}

const LIGHT: Palette = Palette {
    background: 0xfafbf9,
    panel: 0xeff1ed,
    foreground: 0x202220,
    muted_foreground: 0x565b55,
    border: 0xd4d9d1,
    input_border: 0x83897f,
    primary: 0x2b2d2b,
    primary_foreground: 0xfafbf9,
    primary_hover: 0x40453f,
    primary_active: 0x161816,
    item_hover: 0xdfe4dd,
    ring: 0x565b55,
    success: 0xdce9dd,
    success_foreground: 0x365b40,
};

const DARK: Palette = Palette {
    background: 0x101110,
    panel: 0x202220,
    foreground: 0xe8ebe6,
    muted_foreground: 0xa9ada6,
    border: 0x353935,
    input_border: 0x656a63,
    primary: 0xe8ebe6,
    primary_foreground: 0x101110,
    primary_hover: 0xffffff,
    primary_active: 0xcdd1ca,
    item_hover: 0x353935,
    ring: 0x969992,
    success: 0x283a2d,
    success_foreground: 0xaad1b0,
};

pub(crate) fn appearance_panel(current: ThemePreference, events: &Sender<Event>, cx: &App) -> Div {
    panel(cx).child(title("테마")).child(
        div().flex().flex_wrap().gap_2().children(
            [
                ("theme-system", "시스템 설정 따르기", ThemePreference::System),
                ("theme-light", "라이트", ThemePreference::Light),
                ("theme-dark", "다크", ThemePreference::Dark),
            ]
            .map(|(id, label, preference)| {
                let events = events.clone();
                Button::new(id)
                    .when(current == preference, |button| button.primary())
                    .label(label)
                    .on_click(move |_, _, _| {
                        let _ = events.try_send(Event::SetTheme(preference));
                    })
            }),
        ),
    )
}
