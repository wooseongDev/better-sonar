use async_channel::Sender;
use gpui::{AppContext, Context, Entity, Subscription, Window, div, prelude::*, px};
use gpui_component::ActiveTheme;

use crate::{
    app::Event,
    features::{
        settings::{SettingsForm, ThemePreference, appearance_panel, apply_theme},
        sonar::{AppSnapshot, device_panel},
        updates::{Updates, update_panel},
    },
};

pub(crate) struct MainView {
    snapshot: Entity<AppSnapshot>,
    settings: Entity<SettingsForm>,
    updates: Entity<Updates>,
    notice: Entity<Option<String>>,
    events: Sender<Event>,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    pub(crate) fn new(
        snapshot: Entity<AppSnapshot>,
        updates: Entity<Updates>,
        notice: Entity<Option<String>>,
        events: Sender<Event>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = cx.new(|cx| SettingsForm::new(snapshot.clone(), events.clone(), window, cx));
        let subscriptions = vec![
            cx.observe(&snapshot, |_, _, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&updates, |_, _, cx| cx.notify()),
            cx.observe(&notice, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |view, window, cx| {
                if view.snapshot.read(cx).settings.theme == ThemePreference::System {
                    apply_theme(ThemePreference::System, window, cx);
                }
            }),
        ];
        Self {
            snapshot,
            settings,
            updates,
            notice,
            events,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let snapshot = self.snapshot.read(cx);
        let draft = self.settings.read(cx).draft(cx);
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
                    .child(device_panel(snapshot, &draft, &self.events, cx))
                    .child(self.settings.clone())
                    .child(appearance_panel(snapshot.settings.theme, &self.events, cx))
                    .child(update_panel(self.updates.read(cx), &self.events, cx))
                    .children(
                        self.notice
                            .read(cx)
                            .as_ref()
                            .map(|notice| div().p_3().rounded_md().bg(theme.muted).text_sm().child(notice.clone())),
                    )
                    .child(div().h(px(1.))),
            )
    }
}
