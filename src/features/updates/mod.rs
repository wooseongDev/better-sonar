mod panel;
mod updater;

use std::time::Duration;

use anyhow::Result;
use async_channel::Sender;
use gpui::Context;
use semver::Version;
use tokio::runtime::Handle;

use crate::{app::Event, windows::InstallerKind};

pub(crate) use panel::update_panel;
use updater::{Update, Updater};

pub(crate) enum UpdateEvent {
    Check {
        manual: bool,
    },
    Checked {
        manual: bool,
        result: Result<Option<Update>>,
    },
    Install(Version),
    Progress {
        downloaded: u64,
        total: Option<u64>,
    },
    Finished(Result<()>),
}

pub(crate) struct Updates {
    updater: Updater,
    update: Option<Update>,
    checking_update: bool,
    installing: bool,
    downloaded: u64,
    total: Option<u64>,
}

impl Updates {
    pub(crate) fn new(installer: InstallerKind) -> Result<Self> {
        Ok(Self {
            updater: Updater::new(installer)?,
            update: None,
            checking_update: false,
            installing: false,
            downloaded: 0,
            total: None,
        })
    }

    pub(crate) fn handle(
        &mut self,
        event: UpdateEvent,
        events: &Sender<Event>,
        executor: &Handle,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let notice = match event {
            UpdateEvent::Check { manual } => {
                if self.checking_update || self.installing {
                    return None;
                }
                self.checking_update = true;
                let events = events.clone();
                let updater = self.updater.clone();
                executor.spawn(async move {
                    let result = updater.check().await;
                    let _ = events
                        .send(Event::Update(UpdateEvent::Checked { manual, result }))
                        .await;
                });
                None
            }
            UpdateEvent::Checked { manual, result } => {
                self.checking_update = false;
                match result {
                    Ok(update) => {
                        let notice = match &update {
                            Some(update) => {
                                Some(format!("Better Sonar {} 업데이트를 사용할 수 있습니다", update.version))
                            }
                            None if manual => Some("최신 버전을 사용하고 있습니다".into()),
                            None => None,
                        };
                        self.update = update;
                        notice
                    }
                    Err(error) if manual => Some(format!("업데이트 확인에 실패했습니다: {error:#}")),
                    Err(_) => None,
                }
            }
            UpdateEvent::Install(version) => {
                if self.installing || self.checking_update {
                    return None;
                }
                self.installing = true;
                self.downloaded = 0;
                self.total = None;
                let events = events.clone();
                let updater = self.updater.clone();
                executor.spawn(async move {
                    let result = updater
                        .install(version, |downloaded, total| {
                            let _ = events.try_send(Event::Update(UpdateEvent::Progress { downloaded, total }));
                        })
                        .await;
                    let _ = events.send(Event::Update(UpdateEvent::Finished(result))).await;
                });
                None
            }
            UpdateEvent::Progress { downloaded, total } => {
                self.downloaded = downloaded;
                self.total = total;
                None
            }
            UpdateEvent::Finished(Ok(())) => {
                cx.quit();
                return None;
            }
            UpdateEvent::Finished(Err(error)) => {
                self.installing = false;
                Some(format!("업데이트 설치에 실패했습니다: {error:#}"))
            }
        };
        cx.notify();
        notice
    }
}

pub(crate) async fn check_periodically(events: Sender<Event>) {
    tokio::time::sleep(Duration::from_secs(10)).await;
    while events
        .send(Event::Update(UpdateEvent::Check { manual: false }))
        .await
        .is_ok()
    {
        tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
    }
}
