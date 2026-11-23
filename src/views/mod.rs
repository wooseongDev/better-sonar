mod main;

use gpui::{App, Div, SharedString, div, prelude::*, px};
use gpui_component::{ActiveTheme, StyledExt};

pub(crate) use main::MainView;

pub(crate) fn panel(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded(px(9.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .w_full()
}

pub(crate) fn title(text: &'static str) -> Div {
    div().text_lg().font_semibold().child(text)
}

pub(crate) fn hint(cx: &App, text: SharedString) -> Div {
    div().text_sm().text_color(cx.theme().muted_foreground).child(text)
}
