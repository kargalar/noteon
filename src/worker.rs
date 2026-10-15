use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use eframe::egui;

use crate::config::{BLOCK_BATCH_SIZE, SettingKey};
use crate::domain::model::{
    Block, BlockId, BlockKind, DatabaseData, DatabaseView, Document, DropPlacement, Page, PageId,
    SaveBatch, SearchHit,
};
use crate::error::{Error, Result};
use crate::storage::Repository;

#[derive(Debug)]
pub enum Action {
    CreatePage(Option<PageId>),
    Favorite(PageId),
    Trash(PageId),
    Restore(PageId),
    Duplicate(PageId),
    MovePage(PageId, Option<PageId>),
    ReorderPage {
        source: PageId,
        target: PageId,
        after: bool,
    },
    InsertBlock {
        page: PageId,
        position: usize,
        kind: BlockKind,
        content: String,
    },
    ImportMedia {
        page: PageId,
        position: usize,
        source: PathBuf,
    },
    ChooseCoverImage(PageId),
    DeleteBlock {
        page: PageId,
        block: BlockId,
    },
    MoveBlock {
        page: PageId,
        block: BlockId,
        direction: i32,
    },
    ArrangeBlock {
        page: PageId,
        source: BlockId,
        target: BlockId,
        placement: DropPlacement,
    },
    ResizeColumns {
        page: PageId,
        group: BlockId,
        boundary: usize,
        left_width: f32,
        right_width: f32,
    },
    CreateDatabase(Option<PageId>),
    CreateDatabaseBlock {
        page: PageId,
        position: usize,
        replace_block: Option<i64>,
    },
    CreateDatabaseRecord(PageId),
    AddDatabaseProperty {
        database: PageId,
        name: String,
        kind: String,
        options: Vec<String>,
    },
    RenameDatabaseProperty {
        property: i64,
        name: String,
    },
    SetDatabasePropertyType {
        property: i64,
        kind: String,
    },
    SetDatabasePropertyOptions {
        property: i64,
        options: Vec<String>,
    },
    DeleteDatabaseProperty(i64),
    SetDatabaseProperty {
        page: PageId,
        property: i64,
        value: String,
    },
    AddPageProperty {
        page: PageId,
        name: String,
        kind: String,
        options: Vec<String>,
    },
    RenamePageProperty {
        page: PageId,
        property: i64,
        name: String,
    },
    SetPagePropertyType {
        page: PageId,
        property: i64,
        kind: String,
    },
    SetPagePropertyOptions {
        page: PageId,
        property: i64,
        options: Vec<String>,
    },
    DeletePageProperty {
        page: PageId,
        property: i64,
    },
    SetPagePropertyValue {
        page: PageId,
        property: i64,
        value: String,
    },
    SetPagePriority {
        page: PageId,
        priority: u8,
    },
    RenameDatabaseRecord {
        page: PageId,
        title: String,
    },
    SetDatabaseView {
        page: PageId,
        view: DatabaseView,
    },
    SetDatabaseGalleryColumns {
        page: PageId,
        columns: u8,
    },
    SetDatabaseSort {
        page: PageId,
        key: String,
        descending: bool,
    },
    Import,
    Export(PageId),
    Backup,
}

pub enum Request {
    Document {
        page: PageId,
        generation: u64,
    },
    Blocks {
        page: PageId,
        generation: u64,
        start: usize,
    },
    Database(PageId),
    Search {
        query: String,
        generation: u64,
    },
    Save {
        id: u64,
        batch: SaveBatch,
    },
    Action(Action),
    Setting {
        key: SettingKey,
        value: String,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug)]
pub enum FailedOperation {
    Startup,
    Document(u64),
    Blocks(usize, u64),
    Database(PageId),
    Search(u64),
    Save(u64),
    Action,
    Setting,
}

pub enum Event {
    Ready {
        pages: Vec<Page>,
        last_page: Option<PageId>,
        dark: bool,
        light_background: Option<String>,
        dark_background: Option<String>,
        accent_color: Option<String>,
        font_scale: Option<String>,
        dock_layout: Option<String>,
    },
    Document {
        generation: u64,
        document: Document,
    },
    Blocks {
        generation: u64,
        start: usize,
        blocks: Vec<Block>,
    },
    Database(DatabaseData),
    Search {
        generation: u64,
        hits: Vec<SearchHit>,
    },
    Saved(u64),
    ActionDone {
        pages: Vec<Page>,
        document: Option<Document>,
        page_properties: Option<(PageId, Vec<crate::domain::model::PageProperty>)>,
        focus: Option<usize>,
        notice: Option<String>,
        refresh_database: Option<PageId>,
    },
    Failed {
        operation: FailedOperation,
        message: String,
    },
}

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
                match (
                    repository.list_pages(),
                    repository.last_page(),
                    repository.setting(SettingKey::Dark),
                    repository.setting(SettingKey::LightBackground),
                    repository.setting(SettingKey::DarkBackground),
                    repository.setting(SettingKey::AccentColor),
                    repository.setting(SettingKey::FontScale),
                    repository.setting(SettingKey::DockLayout),
                ) {
                    (
                        Ok(pages),
                        Ok(last_page),
                        Ok(dark),
                        Ok(light_background),
                        Ok(dark_background),
                        Ok(accent_color),
                        Ok(font_scale),
                        Ok(dock_layout),
                    ) => publish(Event::Ready {
                        pages,
                        last_page,
                        dark: dark.as_deref() == Some("true"),
                        light_background,
                        dark_background,
                        accent_color,
                        font_scale,
                        dock_layout,
                    }),
                    _ => {
                        publish(Event::Failed {
                            operation: FailedOperation::Startup,
                            message: "Could not read the workspace.".into(),
                        });
                        return;
                    }
                }
                let mut write_failed = false;
                for request in requests {
                    let (operation, result) = match request {
                        Request::Shutdown => break,
                        Request::Document { page, generation } => (
                            FailedOperation::Document(generation),
                            repository.document(page).map(|document| Event::Document {
                                generation,
                                document,
                            }),
                        ),
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
                        Request::Save { id, batch } => {
                            let result = repository.save(&batch);
                            write_failed = result.is_err();
                            (FailedOperation::Save(id), result.map(|()| Event::Saved(id)))
                        }
                        Request::Action(action) => {
                            let result = if write_failed {
                                Err(Error::Invalid(
                                    "Save pending changes before continuing.".into(),
                                ))
                            } else {
                                perform_action(&mut repository, action)
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

fn perform_action(repository: &mut Repository, action: Action) -> Result<Event> {
    let mut focus = None;
    let mut notice = None;
    let mut refresh_database = None;
    let mut refresh_page_properties = None;
    let page = match action {
        Action::CreatePage(parent) => Some(repository.create_page(parent, "")?),
        Action::CreateDatabase(parent) => Some(repository.create_database(parent)?),
        Action::CreateDatabaseBlock {
            page: parent,
            position,
            replace_block,
        } => {
            let database = if let Some(block) = replace_block {
                repository.create_database_block_replacing(parent, block)?
            } else {
                repository.create_database_block(parent, position)?
            };
            refresh_database = Some(database);
            focus = Some(position);
            Some(parent)
        }
        Action::CreateDatabaseRecord(database) => {
            repository.create_database_record(database)?;
            refresh_database = Some(database);
            None
        }
        Action::AddDatabaseProperty {
            database,
            name,
            kind,
            options,
        } => {
            repository.add_database_property(database, &name, &kind, &options)?;
            refresh_database = Some(database);
            None
        }
        Action::RenameDatabaseProperty { property, name } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.rename_database_property(property, &name)?;
            None
        }
        Action::SetDatabasePropertyType { property, kind } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.set_database_property_type(property, &kind)?;
            None
        }
        Action::SetDatabasePropertyOptions { property, options } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.set_database_property_options(property, &options)?;
            None
        }
        Action::DeleteDatabaseProperty(property) => {
            refresh_database = repository.database_property_owner(property)?;
            repository.delete_database_property(property)?;
            None
        }
        Action::SetDatabaseProperty {
            page,
            property,
            value,
        } => {
            refresh_database = repository.page(page)?.parent_id;
            repository.set_database_property(page, property, &value)?;
            None
        }
        Action::AddPageProperty {
            page: property_page,
            name,
            kind,
            options,
        } => {
            repository.add_page_property(property_page, &name, &kind, &options)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::RenamePageProperty {
            page: property_page,
            property,
            name,
        } => {
            repository.rename_page_property(property_page, property, &name)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePropertyType {
            page: property_page,
            property,
            kind,
        } => {
            repository.set_page_property_type(property_page, property, &kind)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePropertyOptions {
            page: property_page,
            property,
            options,
        } => {
            repository.set_page_property_options(property_page, property, &options)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::DeletePageProperty {
            page: property_page,
            property,
        } => {
            repository.delete_page_property(property_page, property)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePropertyValue {
            page: property_page,
            property,
            value,
        } => {
            repository.set_page_property_value(property_page, property, &value)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePriority { page, priority } => {
            refresh_database = repository.page(page)?.parent_id;
            repository.set_page_priority(page, priority)?;
            None
        }
        Action::RenameDatabaseRecord { page, title } => {
            refresh_database = repository.page(page)?.parent_id;
            repository.rename_database_record(page, &title)?;
            None
        }
        Action::SetDatabaseView { page, view } => {
            refresh_database = Some(page);
            repository.set_database_view(page, view)?;
            None
        }
        Action::SetDatabaseGalleryColumns { page, columns } => {
            refresh_database = Some(page);
            repository.set_database_gallery_columns(page, columns)?;
            None
        }
        Action::SetDatabaseSort {
            page,
            key,
            descending,
        } => {
            refresh_database = Some(page);
            repository.set_database_sort(page, &key, descending)?;
            None
        }
        Action::Favorite(id) => {
            repository.toggle_favorite(id)?;
            None
        }
        Action::Trash(id) => {
            repository.trash_page(id)?;
            None
        }
        Action::Restore(id) => {
            repository.restore_page(id)?;
            Some(id)
        }
        Action::Duplicate(id) => Some(repository.duplicate_page(id)?),
        Action::MovePage(id, parent) => {
            repository.move_page(id, parent)?;
            None
        }
        Action::ReorderPage {
            source,
            target,
            after,
        } => {
            repository.reorder_page(source, target, after)?;
            None
        }
        Action::InsertBlock {
            page,
            position,
            kind,
            content,
        } => {
            if content.is_empty() {
                repository.insert_block(page, position, kind)?;
            } else {
                repository.append_blocks(page, position, &[(kind, content, false)])?;
            }
            focus = Some(position);
            Some(page)
        }
        Action::ImportMedia {
            page,
            position,
            source,
        } => {
            let data_directory = crate::config::data_directory()?;
            let kind = repository.import_media_block(page, position, &source, &data_directory)?;
            notice = Some(format!("{} eklendi.", kind.label()));
            focus = Some(position);
            Some(page)
        }
        Action::ChooseCoverImage(page_id) => {
            if let Some(source) = rfd::FileDialog::new()
                .add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"],
                )
                .pick_file()
            {
                let data_directory = crate::config::data_directory()?;
                let cover = repository.import_cover_image(&source, &data_directory)?;
                let mut edit = crate::domain::model::PageEdit::from(&repository.page(page_id)?);
                edit.cover_image = Some(cover);
                edit.cover = 0;
                repository.save(&crate::domain::model::SaveBatch {
                    pages: vec![edit],
                    blocks: Vec::new(),
                })?;
                Some(page_id)
            } else {
                None
            }
        }
        Action::DeleteBlock { page, block } => {
            repository.delete_block(block)?;
            Some(page)
        }
        Action::MoveBlock {
            page,
            block,
            direction,
        } => {
            repository.move_block(block, direction)?;
            Some(page)
        }
        Action::ArrangeBlock {
            page,
            source,
            target,
            placement,
        } => {
            repository.arrange_block(page, source, target, placement)?;
            Some(page)
        }
        Action::ResizeColumns {
            page,
            group,
            boundary,
            left_width,
            right_width,
        } => {
            repository.resize_columns(page, group, boundary, left_width, right_width)?;
            Some(page)
        }
        Action::Import => {
            let path = rfd::FileDialog::new()
                .add_filter("Markdown", &["md", "markdown", "txt"])
                .pick_file();
            if let Some(path) = path {
                Some(repository.import_markdown(&path)?)
            } else {
                None
            }
        }
        Action::Export(id) => {
            let title = repository
                .page(id)?
                .display_title()
                .chars()
                .map(|c| {
                    if c.is_control() || "<>:\"/\\|?*".contains(c) {
                        '_'
                    } else {
                        c
                    }
                })
                .take(120)
                .collect::<String>();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Markdown", &["md"])
                .set_file_name(format!("{title}.md"))
                .save_file()
            {
                repository.export_markdown(id, &path)?;
                notice = Some(format!("Exported to: {}", path.display()));
            }
            None
        }
        Action::Backup => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("SQLite backup", &["sqlite"])
                .set_file_name("noteon-backup.sqlite")
                .save_file()
            {
                repository.backup(&path)?;
                notice = Some(format!("Workspace backed up to: {}", path.display()));
            }
            None
        }
    };
    Ok(Event::ActionDone {
        pages: repository.list_pages()?,
        document: page.map(|id| repository.document(id)).transpose()?,
        page_properties: refresh_page_properties
            .map(|id| {
                repository
                    .page_properties(id)
                    .map(|properties| (id, properties))
            })
            .transpose()?,
        focus,
        notice,
        refresh_database,
    })
}
