#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod discovery;
#[cfg(not(test))]
mod gg_shortcuts;
mod models;
mod settings;
mod shortcuts;
mod sonar_client;
mod state;
mod tray;
mod ui;
mod updates;
mod windows;

fn main() -> anyhow::Result<()> {
    let (events, receiver) = async_channel::unbounded();
    let show = !windows::is_autostart();
    let Some(_instance) = windows::Instance::acquire(events.clone(), show)? else {
        return Ok(());
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    if show {
        events.try_send(app::Event::Show)?;
    }
    let executor = runtime.handle().clone();
    gpui::Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            if let Err(error) = app::Desktop::start(events, receiver, executor, cx) {
                eprintln!("Better Sonar를 시작할 수 없습니다: {error:#}");
                cx.quit();
            }
        });
    Ok(())
}
