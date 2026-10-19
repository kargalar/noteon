use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use eframe::egui;

use crate::config::{BLOCK_BATCH_SIZE, SettingKey};
use crate::domain::model::{
    Block, BlockId, BlockKind, BulkBlockStyleOp, DatabaseData, DatabaseView, DatabaseViewSettings,
    Document, DropPlacement, Page, PageId, PageSnapshot, PropertyOption, SaveBatch, SearchHit, TextStyleRange,
};
use crate::error::{Error, Result};
use crate::storage::Repository;

#[derive(Debug)]
pub enum Action {
    CreatePage(Option<PageId>),
    CreateSubpageBlock {
        page: PageId,
        position: usize,
        replace_block: Option<BlockId>,
    },
    Favorite(PageId),
    Trash(PageId),
    TrashPages(Vec<PageId>),
    DeletePermanently(PageId),
    DeletePagesPermanently(Vec<PageId>),
    EmptyTrash,
    Restore(PageId),
    RestorePages(Vec<PageId>),
    UndoPage(PageId),
    RedoPage(PageId),
    RestorePageHistory {
        page: PageId,
        entry_id: u64,
    },
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
        text_styles: Vec<TextStyleRange>,
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
    DeleteBlocks {
        page: PageId,
        blocks: Vec<BlockId>,
    },
    SetBlocksKind {
        page: PageId,
        blocks: Vec<BlockId>,
        kind: BlockKind,
    },
    SetBlocksChecked {
        page: PageId,
        blocks: Vec<BlockId>,
        checked: bool,
    },
    FormatBlocks {
        page: PageId,
        blocks: Vec<BlockId>,
        op: BulkBlockStyleOp,
    },
    MoveBlocks {
        page: PageId,
        blocks: Vec<BlockId>,
        direction: i32,
    },
    DuplicateBlocks {
        page: PageId,
        blocks: Vec<BlockId>,
    },
    GroupBlocksInColumns {
        page: PageId,
        blocks: Vec<BlockId>,
    },
    UngroupBlockColumns {
        page: PageId,
        blocks: Vec<BlockId>,
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
    ArrangeBlocks {
        page: PageId,
        sources: Vec<BlockId>,
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
    CreateDatabaseRecordWithProperty {
        database: PageId,
        property: i64,
        value: String,
    },
    MoveDatabaseRecord {
        record: PageId,
        property: i64,
        value: String,
        target: Option<(PageId, bool)>,
    },
    AddDatabaseProperty {
        database: PageId,
        name: String,
        kind: String,
        options: Vec<PropertyOption>,
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
        options: Vec<PropertyOption>,
    },
    RenameDatabasePropertyOption {
        property: i64,
        old_option: String,
        new_option: String,
    },
    RemoveDatabasePropertyOption {
        property: i64,
        option: String,
    },
    MoveDatabaseProperty {
        property: i64,
        direction: i32,
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
        options: Vec<PropertyOption>,
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
        options: Vec<PropertyOption>,
    },
    RenamePagePropertyOption {
        page: PageId,
        property: i64,
        old_option: String,
        new_option: String,
    },
    RemovePagePropertyOption {
        page: PageId,
        property: i64,
        option: String,
    },
    MovePageProperty {
        page: PageId,
        property: i64,
        direction: i32,
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
    SetDatabaseRecordIcon {
        page: PageId,
        icon: String,
    },
    SetDatabaseView {
        page: PageId,
        view: DatabaseView,
    },
    SetDatabaseGalleryColumns {
        page: PageId,
        columns: u8,
    },
    SetDatabaseViewSettings {
        page: PageId,
        settings: DatabaseViewSettings,
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
        sidebar_pinned: Option<String>,
        custom_theme: Option<String>,
    },
    Document {
        generation: u64,
        document: Document,
        history: Vec<crate::domain::model::PageHistorySummary>,
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
    Saved {
        id: u64,
        histories: HashMap<PageId, Vec<crate::domain::model::PageHistorySummary>>,
    },
    ActionDone {
        pages: Vec<Page>,
        document: Option<Document>,
        page_properties: Option<(PageId, Vec<crate::domain::model::PageProperty>)>,
        focus: Option<usize>,
        focus_at_end: bool,
        notice: Option<String>,
        refresh_database: Option<PageId>,
        created_record_document: Option<Document>,
        history: Option<Vec<crate::domain::model::PageHistorySummary>>,
    },
    PageUndoEmpty {
        page_id: PageId,
    },
    PageRedoEmpty {
        page_id: PageId,
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
                    repository.setting(SettingKey::SidebarPinned),
                    repository.setting(SettingKey::CustomTheme),
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
                        Ok(sidebar_pinned),
                        Ok(custom_theme),
                    ) => publish(Event::Ready {
                        pages,
                        last_page,
                        dark: dark.as_deref() == Some("true"),
                        light_background,
                        dark_background,
                        accent_color,
                        font_scale,
                        dock_layout,
                        sidebar_pinned,
                        custom_theme,
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
                let mut page_histories: HashMap<PageId, PageHistory> = HashMap::new();
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
                        Request::Save { id, batch } => {
                            let touched_pages: HashSet<PageId> = batch
                                .pages
                                .iter()
                                .map(|p| p.id)
                                .chain(batch.blocks.iter().map(|b| b.page_id))
                                .collect();
                            for &page_id in &touched_pages {
                                let history = page_histories.entry(page_id).or_default();
                                let should_push = match history.last_save_time {
                                    Some(time) => time.elapsed() > std::time::Duration::from_millis(1500),
                                    None => true,
                                };
                                if let Ok(current_snap) = repository.page_snapshot(page_id) {
                                    let is_discrete_change = batch.pages.iter().any(|p| p.id == page_id)
                                        || batch.blocks.iter().any(|b| {
                                            current_snap
                                                .blocks
                                                .iter()
                                                .any(|cb| cb.id == b.id && (cb.checked != b.checked || cb.kind != b.kind))
                                        });
                                    if should_push || is_discrete_change {
                                        let desc = if is_discrete_change
                                            && batch.blocks.iter().any(|b| {
                                                current_snap
                                                    .blocks
                                                    .iter()
                                                    .any(|cb| cb.id == b.id && cb.checked != b.checked)
                                            })
                                        {
                                            "Görev durumu değiştirildi"
                                        } else if batch.pages.iter().any(|p| p.id == page_id) {
                                            "Başlık düzenlendi"
                                        } else {
                                            "Metin düzenlendi"
                                        };
                                        history.push_undo(current_snap, desc);
                                    }
                                }
                                history.last_save_time = Some(std::time::Instant::now());
                            }
                            let result = repository.save(&batch);
                            write_failed = result.is_err();
                            let updated_histories: HashMap<PageId, Vec<crate::domain::model::PageHistorySummary>> = touched_pages
                                .into_iter()
                                .map(|p| (p, page_histories.get(&p).map(|h| h.summaries()).unwrap_or_default()))
                                .collect();
                            (FailedOperation::Save(id), result.map(|()| Event::Saved { id, histories: updated_histories }))
                        }
                        Request::Action(action) => {
                            let result = if write_failed {
                                Err(Error::Invalid(
                                    "Save pending changes before continuing.".into(),
                                ))
                            } else {
                                perform_action(&mut repository, &mut page_histories, action)
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

const MAX_PAGE_HISTORY: usize = 10;

#[derive(Clone, Debug)]
struct PageHistoryEntry {
    id: u64,
    description: String,
    timestamp: i64,
    snapshot: PageSnapshot,
}

#[derive(Default)]
struct PageHistory {
    undo_stack: Vec<PageHistoryEntry>,
    redo_stack: Vec<PageHistoryEntry>,
    next_id: u64,
    last_save_time: Option<std::time::Instant>,
}

impl PageHistory {
    fn push_undo(&mut self, snapshot: PageSnapshot, description: impl Into<String>) {
        if let Some(top) = self.undo_stack.last() {
            if top.snapshot == snapshot {
                return;
            }
        }
        self.next_id += 1;
        let now = chrono::Local::now().timestamp();
        self.undo_stack.push(PageHistoryEntry {
            id: self.next_id,
            description: description.into(),
            timestamp: now,
            snapshot,
        });
        while self.undo_stack.len() > MAX_PAGE_HISTORY {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn summaries(&self) -> Vec<crate::domain::model::PageHistorySummary> {
        self.undo_stack
            .iter()
            .rev()
            .map(|e| crate::domain::model::PageHistorySummary {
                id: e.id,
                description: e.description.clone(),
                timestamp: e.timestamp,
            })
            .collect()
    }
}

fn perform_action(
    repository: &mut Repository,
    page_histories: &mut HashMap<PageId, PageHistory>,
    action: Action,
) -> Result<Event> {
    let mut focus = None;
    let mut focus_at_end = false;
    let mut notice = None;
    let mut refresh_database = None;
    let mut refresh_page_properties = None;
    let mut created_record = None;

    let action_info: Option<(PageId, &'static str)> = match &action {
        Action::InsertBlock { page, .. } => Some((*page, "Blok eklendi")),
        Action::CreateSubpageBlock { page, .. } => Some((*page, "Alt sayfa eklendi")),
        Action::ImportMedia { page, .. } => Some((*page, "Medya eklendi")),
        Action::DeleteBlock { page, .. } | Action::DeleteBlocks { page, .. } => Some((*page, "Blok silindi")),
        Action::SetBlocksKind { page, .. } => Some((*page, "Blok türü değiştirildi")),
        Action::SetBlocksChecked { page, .. } => Some((*page, "Görev durumu değiştirildi")),
        Action::FormatBlocks { page, .. } => Some((*page, "Biçimlendirme uygulandı")),
        Action::MoveBlocks { page, .. } | Action::MoveBlock { page, .. } | Action::ArrangeBlock { page, .. } | Action::ArrangeBlocks { page, .. } => Some((*page, "Blok taşındı")),
        Action::DuplicateBlocks { page, .. } => Some((*page, "Bloklar çoğaltıldı")),
        Action::GroupBlocksInColumns { page, .. } | Action::UngroupBlockColumns { page, .. } | Action::ResizeColumns { page, .. } => Some((*page, "Sütunlar düzenlendi")),
        Action::CreateDatabaseBlock { page, .. } => Some((*page, "Veritabanı eklendi")),
        Action::AddPageProperty { page, .. }
        | Action::RenamePageProperty { page, .. }
        | Action::SetPagePropertyType { page, .. }
        | Action::SetPagePropertyOptions { page, .. }
        | Action::DeletePageProperty { page, .. }
        | Action::SetPagePropertyValue { page, .. } => Some((*page, "Özellik düzenlendi")),
        _ => None,
    };
    if let Some((page_id, desc)) = action_info {
        let history = page_histories.entry(page_id).or_default();
        history.last_save_time = None;
        if let Ok(snapshot) = repository.page_snapshot(page_id) {
            history.push_undo(snapshot, desc);
        }
    }

    let page = match action {
        Action::CreatePage(parent) => Some(repository.create_page(parent, "")?),
        Action::CreateSubpageBlock {
            page: parent,
            position,
            replace_block,
        } => {
            let subpage_id = if let Some(block) = replace_block {
                repository.create_subpage_block_replacing(parent, block)?
            } else {
                repository.create_subpage_block(parent, position)?
            };
            Some(subpage_id)
        }
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
            let record = repository.create_database_record(database)?;
            refresh_database = Some(database);
            created_record = Some(record);
            None
        }
        Action::CreateDatabaseRecordWithProperty {
            database,
            property,
            value,
        } => {
            let record = repository.create_database_record_with_property(database, Some((property, &value)))?;
            refresh_database = Some(database);
            created_record = Some(record);
            None
        }
        Action::MoveDatabaseRecord {
            record,
            property,
            value,
            target,
        } => {
            refresh_database = repository.move_database_record(record, property, &value, target)?;
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
        Action::RenameDatabasePropertyOption {
            property,
            old_option,
            new_option,
        } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.rename_database_property_option(property, &old_option, &new_option)?;
            None
        }
        Action::RemoveDatabasePropertyOption { property, option } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.remove_database_property_option(property, &option)?;
            None
        }
        Action::MoveDatabaseProperty {
            property,
            direction,
        } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.move_database_property(property, direction)?;
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
        Action::RenamePagePropertyOption {
            page: property_page,
            property,
            old_option,
            new_option,
        } => {
            repository.rename_page_property_option(
                property_page,
                property,
                &old_option,
                &new_option,
            )?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::RemovePagePropertyOption {
            page: property_page,
            property,
            option,
        } => {
            repository.remove_page_property_option(property_page, property, &option)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::MovePageProperty {
            page: property_page,
            property,
            direction,
        } => {
            repository.move_page_property(property_page, property, direction)?;
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
        Action::SetDatabaseRecordIcon { page, icon } => {
            let page_info = repository.page(page)?;
            refresh_database = if page_info.is_database {
                None
            } else {
                page_info.parent_id
            };
            repository.set_page_icon(page, &icon)?;
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
        Action::SetDatabaseViewSettings { page, settings } => {
            refresh_database = Some(page);
            repository.set_database_view_settings(page, &settings)?;
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
            let parent = repository.page(id).ok().and_then(|p| p.parent_id);
            repository.trash_pages(&[id])?;
            if let Some(parent_id) = parent
                && repository.page(parent_id).is_ok_and(|p| p.is_database)
            {
                refresh_database = Some(parent_id);
            }
            None
        }
        Action::TrashPages(ids) => {
            let parent = ids
                .first()
                .copied()
                .and_then(|id| repository.page(id).ok())
                .and_then(|p| p.parent_id);
            repository.trash_pages(&ids)?;
            if let Some(parent_id) = parent
                && repository.page(parent_id).is_ok_and(|p| p.is_database)
            {
                refresh_database = Some(parent_id);
            }
            None
        }
        Action::DeletePermanently(id) => {
            repository.delete_pages_permanently(&[id])?;
            None
        }
        Action::DeletePagesPermanently(ids) => {
            repository.delete_pages_permanently(&ids)?;
            None
        }
        Action::EmptyTrash => {
            repository.empty_trash()?;
            None
        }
        Action::Restore(id) => {
            repository.restore_pages(&[id])?;
            Some(id)
        }
        Action::RestorePages(ids) => {
            repository.restore_pages(&ids)?;
            ids.last().copied()
        }
        Action::UndoPage(page_id) => {
            let history = page_histories.entry(page_id).or_default();
            if let Some(prev) = history.undo_stack.pop() {
                if let Ok(current) = repository.page_snapshot(page_id) {
                    history.next_id += 1;
                    let now = chrono::Local::now().timestamp();
                    history.redo_stack.push(PageHistoryEntry {
                        id: history.next_id,
                        description: prev.description.clone(),
                        timestamp: now,
                        snapshot: current,
                    });
                    while history.redo_stack.len() > MAX_PAGE_HISTORY {
                        history.redo_stack.remove(0);
                    }
                }
                history.last_save_time = None;
                let _ = repository.restore_page_snapshot(&prev.snapshot)?;
                focus = Some(0);
                Some(page_id)
            } else {
                return Ok(Event::PageUndoEmpty { page_id });
            }
        }
        Action::RedoPage(page_id) => {
            let history = page_histories.entry(page_id).or_default();
            if let Some(next) = history.redo_stack.pop() {
                if let Ok(current) = repository.page_snapshot(page_id) {
                    history.next_id += 1;
                    let now = chrono::Local::now().timestamp();
                    history.undo_stack.push(PageHistoryEntry {
                        id: history.next_id,
                        description: next.description.clone(),
                        timestamp: now,
                        snapshot: current,
                    });
                    while history.undo_stack.len() > MAX_PAGE_HISTORY {
                        history.undo_stack.remove(0);
                    }
                }
                history.last_save_time = None;
                let _ = repository.restore_page_snapshot(&next.snapshot)?;
                focus = Some(0);
                Some(page_id)
            } else {
                return Ok(Event::PageRedoEmpty { page_id });
            }
        }
        Action::RestorePageHistory { page: page_id, entry_id } => {
            let history = page_histories.entry(page_id).or_default();
            if let Some(pos) = history.undo_stack.iter().position(|e| e.id == entry_id) {
                let entry = history.undo_stack[pos].clone();
                if let Ok(current) = repository.page_snapshot(page_id) {
                    history.next_id += 1;
                    let now = chrono::Local::now().timestamp();
                    history.redo_stack.push(PageHistoryEntry {
                        id: history.next_id,
                        description: "Geri yükleme öncesi".into(),
                        timestamp: now,
                        snapshot: current,
                    });
                    while history.redo_stack.len() > MAX_PAGE_HISTORY {
                        history.redo_stack.remove(0);
                    }
                }
                history.last_save_time = None;
                let _ = repository.restore_page_snapshot(&entry.snapshot)?;
                notice = Some("Geçmiş sürüm geri yüklendi.".into());
                focus = Some(0);
                Some(page_id)
            } else {
                None
            }
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
            text_styles,
        } => {
            if content.is_empty() {
                repository.insert_block(page, position, kind)?;
            } else {
                repository.append_blocks_with_styles(
                    page,
                    position,
                    &[(kind, content, false, text_styles)],
                )?;
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
            let pos = repository.block_position(block).ok().flatten();
            repository.delete_block(block)?;
            focus = pos.map(|p| p.saturating_sub(1));
            focus_at_end = true;
            Some(page)
        }
        Action::DeleteBlocks { page, blocks } => {
            let first_pos = blocks
                .first()
                .and_then(|id| repository.block_position(*id).ok().flatten());
            repository.delete_blocks(page, &blocks)?;
            focus = first_pos.map(|p| p.saturating_sub(1));
            focus_at_end = true;
            Some(page)
        }
        Action::SetBlocksKind { page, blocks, kind } => {
            repository.set_blocks_kind(page, &blocks, kind)?;
            Some(page)
        }
        Action::SetBlocksChecked {
            page,
            blocks,
            checked,
        } => {
            repository.set_blocks_checked(page, &blocks, checked)?;
            Some(page)
        }
        Action::FormatBlocks { page, blocks, op } => {
            repository.format_blocks(page, &blocks, op)?;
            Some(page)
        }
        Action::MoveBlocks {
            page,
            blocks,
            direction,
        } => {
            repository.move_blocks(page, &blocks, direction)?;
            Some(page)
        }
        Action::DuplicateBlocks { page, blocks } => {
            repository.duplicate_blocks(page, &blocks)?;
            Some(page)
        }
        Action::GroupBlocksInColumns { page, blocks } => {
            repository.group_blocks_in_columns(page, &blocks)?;
            Some(page)
        }
        Action::UngroupBlockColumns { page, blocks } => {
            repository.ungroup_block_columns(page, &blocks)?;
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
        Action::ArrangeBlocks {
            page,
            sources,
            target,
            placement,
        } => {
            repository.arrange_blocks(page, &sources, target, placement)?;
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
    let created_record_document = created_record
        .map(|id| {
            let _ = repository.ensure_subpage_blocks(id);
            repository.document(id)
        })
        .transpose()?;
    Ok(Event::ActionDone {
        pages: repository.list_pages()?,
        document: page
            .map(|id| {
                let _ = repository.ensure_subpage_blocks(id);
                repository.document(id)
            })
            .transpose()?,
        page_properties: refresh_page_properties
            .map(|id| {
                repository
                    .page_properties(id)
                    .map(|properties| (id, properties))
            })
            .transpose()?,
        focus,
        focus_at_end,
        notice,
        refresh_database,
        created_record_document,
        history: page.and_then(|id| page_histories.get(&id)).map(|h| h.summaries()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_history_caps_at_maximum_entries_and_clears_redo_on_push() {
        let mut history = PageHistory::default();
        for i in 0..15 {
            let snapshot = PageSnapshot {
                page_id: 1,
                title: format!("Title {i}"),
                icon: "📄".into(),
                blocks: Vec::new(),
            };
            history.push_undo(snapshot, format!("Change {i}"));
        }

        assert_eq!(history.undo_stack.len(), MAX_PAGE_HISTORY);
        assert_eq!(history.undo_stack.len(), 10);
        assert_eq!(history.undo_stack.last().unwrap().description, "Change 14");
        assert_eq!(history.undo_stack.first().unwrap().description, "Change 5");

        let summaries = history.summaries();
        assert_eq!(summaries.len(), 10);
        assert_eq!(summaries[0].description, "Change 14");
        assert_eq!(summaries[9].description, "Change 5");
    }

    #[test]
    fn page_history_ignores_duplicate_snapshots() {
        let mut history = PageHistory::default();
        let snapshot = PageSnapshot {
            page_id: 1,
            title: "Same Title".into(),
            icon: "📄".into(),
            blocks: Vec::new(),
        };
        history.push_undo(snapshot.clone(), "Change 1");
        history.push_undo(snapshot, "Change 2");
        assert_eq!(history.undo_stack.len(), 1);
        assert_eq!(history.undo_stack[0].description, "Change 1");
    }
}
