use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::SettingKey;
use crate::domain::model::{
    Block, BlockId, BlockKind, BulkBlockStyleOp, DatabaseData, DatabaseView, DatabaseViewSettings,
    Document, DropPlacement, Page, PageId, PropertyOption, SaveBatch, SearchHit, TextStyleRange,
};
use crate::domain::search::PageFindMatch;

#[derive(Debug)]
pub enum MediaSource {
    File(PathBuf),
    Image(image::RgbaImage),
}

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
    Undo,
    Redo,
    UndoPage(PageId),
    RedoPage(PageId),
    RestorePageHistory {
        page: PageId,
        entry_id: u64,
    },
    Duplicate(PageId),
    MovePage(PageId, Option<PageId>),
    TransferPages {
        pages: Vec<PageId>,
        parent: Option<PageId>,
        position: Option<usize>,
        replace_block: Option<BlockId>,
        copy: bool,
    },
    ReorderPage {
        source: PageId,
        target: PageId,
        after: bool,
    },
    InsertBlock {
        column_anchor: Option<BlockId>,
        page: PageId,
        position: usize,
        kind: BlockKind,
        content: String,
        text_styles: Vec<TextStyleRange>,
    },
    InsertBlocks {
        column_anchor: Option<BlockId>,
        page: PageId,
        position: usize,
        blocks: Vec<crate::domain::model::StyledBlockContent>,
    },
    PasteBlocks {
        page: PageId,
        position: usize,
        replace_block: Option<BlockId>,
        blocks: Vec<Block>,
    },
    ImportMedia {
        page: PageId,
        position: usize,
        column_anchor: Option<BlockId>,
        source: MediaSource,
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
    CreateDatabaseRecordInline(PageId),
    ReorderDatabaseRecords {
        database: PageId,
        records: Vec<PageId>,
        target: PageId,
        after: bool,
    },
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
    Reload,
    RestoreSnapshot(PathBuf),
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
    FindInPage {
        page: PageId,
        generation: u64,
        revision: u64,
        query: String,
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
    FindInPage { generation: u64, revision: u64 },
    Save(u64),
    Action,
    Setting,
}

#[allow(
    clippy::large_enum_variant,
    reason = "Preserve the worker event payload types"
)]
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
    PageFind {
        generation: u64,
        revision: u64,
        matches: Vec<PageFindMatch>,
    },
    Saved {
        id: u64,
        histories: HashMap<PageId, Vec<crate::domain::model::PageHistorySummary>>,
    },
    ActionDone {
        pages: Vec<Page>,
        document: Option<Document>,
        preserve_view: bool,
        page_properties: Option<(PageId, Vec<crate::domain::model::PageProperty>)>,
        focus: Option<usize>,
        focus_at_end: bool,
        notice: Option<String>,
        refresh_database: Option<PageId>,
        created_record_document: Option<Document>,
        focus_created_record_name: bool,
        history: Option<Vec<crate::domain::model::PageHistorySummary>>,
        page_transfer: Option<Box<PageTransferUpdate>>,
    },
    PageUndoEmpty {
        page_id: PageId,
    },
    PageRedoEmpty {
        page_id: PageId,
    },
    UndoEmpty,
    RedoEmpty,
    Failed {
        operation: FailedOperation,
        message: String,
    },
}

#[derive(Debug)]
pub struct PageTransferUpdate {
    pub moved_pages: Vec<PageId>,
    pub databases: Vec<PageId>,
    pub documents: Vec<Document>,
}
