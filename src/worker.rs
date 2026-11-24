mod actions;
mod history;
mod protocol;
mod saves;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use eframe::egui;

use crate::config::{BLOCK_BATCH_SIZE, SettingKey};
use crate::error::{Error, Result};
use crate::storage::Repository;

use actions::perform_action;
use history::{GlobalHistory, PageHistory};
pub use protocol::{Action, Event, FailedOperation, MediaSource, PageTransferUpdate, Request};
use saves::PendingSaves;

pub struct Worker {
    sender: Sender<Request>,
    pub events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(path: PathBuf, context: egui::Context) -> Result<Self> {
        let (sender, requests) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("noteon-storage".into())
            .spawn(move || {
                let publish = |event| {
                    let _ = events.send(event);
                    context.request_repaint();
                };
                let mut repository = match Repository::open(&path).and_then(|mut repository| {
                    repository.seed_welcome()?;
                    Ok(repository)
                }) {
                    Ok(repository) => repository,
                    Err(error) => {
                        publish(Event::Failed {
                            operation: FailedOperation::Startup,
                            message: error.to_string(),
                        });
                        return;
                    }
                };
                match workspace_ready(&repository) {
                    Ok(event) => publish(event),
                    Err(_) => {
                        publish(Event::Failed {
                            operation: FailedOperation::Startup,
                            message: "Could not read the workspace.".into(),
                        });
                        return;
                    }
                }
                let mut saves = PendingSaves::default();
                let mut page_histories: HashMap<_, PageHistory> = HashMap::new();
                let mut global_history = GlobalHistory::default();
                for request in requests {
                    let (operation, result) = match request {
                        Request::Shutdown => break,
                        Request::Document { page, generation } => {
                            let _ = repository.ensure_subpage_blocks(page);
                            let history = page_histories
                                .get(&page)
                                .map(|h| h.summaries())
                                .unwrap_or_default();
                            (
                                FailedOperation::Document(generation),
                                repository.document(page).map(|document| Event::Document {
                                    generation,
                                    document,
                                    history,
                                }),
                            )
                        }
                        Request::Blocks {
                            page,
                            generation,
                            start,
                        } => (
                            FailedOperation::Blocks(start, generation),
                            repository
                                .blocks(page, start, BLOCK_BATCH_SIZE)
                                .map(|blocks| Event::Blocks {
                                    generation,
                                    start,
                                    blocks,
                                }),
                        ),
                        Request::Database(page) => (
                            FailedOperation::Database(page),
                            repository.database(page).map(Event::Database),
                        ),
                        Request::Search { query, generation } => (
                            FailedOperation::Search(generation),
                            repository
                                .search(&query)
                                .map(|hits| Event::Search { generation, hits }),
                        ),
                        Request::FindInPage {
                            page,
                            generation,
                            revision,
                            query,
                        } => {
                            let result = if !saves.has_failed_changes() {
                                repository.find_in_page(page, &query).map(|matches| {
                                    Event::PageFind {
                                        generation,
                                        revision,
                                        matches,
                                    }
                                })
                            } else {
                                Err(Error::Invalid(
                                    "Save pending changes before searching.".into(),
                                ))
                            };
                            (
                                FailedOperation::FindInPage {
                                    generation,
                                    revision,
                                },
                                result,
                            )
                        }
                        Request::Save { id, batch } => (
                            FailedOperation::Save(id),
                            saves.save(
                                &mut repository,
                                &mut page_histories,
                                &mut global_history,
                                id,
                                &batch,
                            ),
                        ),
                        Request::Action(action) => {
                            let result = if saves.has_failed_changes() {
                                Err(Error::Invalid(
                                    "Save pending changes before continuing.".into(),
                                ))
                            } else {
                                perform_action(
                                    &mut repository,
                                    &mut page_histories,
                                    &mut global_history,
                                    action,
                                )
                            };
                            (FailedOperation::Action, result)
                        }
                        Request::Setting { key, value } => {
                            if let Err(error) = repository.set_setting(key, &value) {
                                publish(Event::Failed {
                                    operation: FailedOperation::Setting,
                                    message: error.to_string(),
                                });
                            }
                            continue;
                        }
                    };
                    publish(match result {
                        Ok(event) => event,
                        Err(error) => Event::Failed {
                            operation,
                            message: error.to_string(),
                        },
                    });
                }
            })?;
        Ok(Self {
            sender,
            events: receiver,
            thread: Some(thread),
        })
    }

    pub fn send(&self, request: Request) -> Result<()> {
        self.sender
            .send(request)
            .map_err(|_| Error::WorkerUnavailable)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.sender.send(Request::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn workspace_ready(repository: &Repository) -> Result<Event> {
    Ok(Event::Ready {
        pages: repository.list_pages()?,
        last_page: repository.last_page()?,
        dark: repository.setting(SettingKey::Dark)?.as_deref() != Some("false"),
        light_background: repository.setting(SettingKey::LightBackground)?,
        dark_background: repository.setting(SettingKey::DarkBackground)?,
        accent_color: repository.setting(SettingKey::AccentColor)?,
        font_scale: repository.setting(SettingKey::FontScale)?,
        dock_layout: repository.setting(SettingKey::DockLayout)?,
        sidebar_pinned: repository.setting(SettingKey::SidebarPinned)?,
        custom_theme: repository.setting(SettingKey::CustomTheme)?,
    })
}

#[cfg(test)]
mod tests;
