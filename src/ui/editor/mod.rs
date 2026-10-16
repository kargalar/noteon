mod block;
mod cache;
mod header;
pub mod links;
pub(super) mod media;
mod viewport;

use crate::domain::model::{BlockId, DatabaseData, Document, PageId};
use crate::domain::virtual_layout::VirtualLayout;
use crate::ui::{Intent, palette::Palette, text};
use crate::worker::Action;
pub use cache::BlockCache;
use eframe::egui::{Pos2, RichText, Ui};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Editor {
    pub document: Option<Document>,
    pub cache: BlockCache,
    pub layout: VirtualLayout,
    pub generation: u64,
    pub jump_to: Option<usize>,
    pub focus_position: Option<usize>,
    pub rendered_blocks: usize,
    pub visual_rows: Vec<Vec<usize>>,
    pub position_to_row: Vec<usize>,
    pub selected_blocks: HashSet<BlockId>,
    pub anchor_block: Option<BlockId>,
    pub(super) marquee_anchor: Option<Pos2>,
    pub(super) marquee_base_selection: HashSet<BlockId>,
    pub(super) marquee_active: bool,
    pub(super) bulk_menu_pos: Option<Pos2>,
    column_resize_preview: HashMap<(i64, usize), (f32, f32)>,
    title_focus: bool,
    last_width: f32,
    scroll_offset: f32,
    reset_scroll: bool,
}

pub struct DatabaseContext<'a> {
    pub record_database_id: Option<PageId>,
    pub databases: &'a mut HashMap<PageId, DatabaseData>,
    pub loading: &'a mut HashSet<PageId>,
    pub property_drafts: &'a mut HashMap<i64, (String, String)>,
    pub title_drafts: &'a mut HashMap<PageId, String>,
    pub value_drafts: &'a mut HashMap<(PageId, i64), String>,
    pub visible: &'a mut HashSet<PageId>,
}

impl Editor {
    pub fn bind(&mut self, document: Document, generation: u64, focus: Option<usize>) {
        let same_page = self
            .document
            .as_ref()
            .is_some_and(|old| old.page.id == document.page.id);
        if same_page {
            let valid_ids: HashSet<_> = document.outline.iter().map(|stub| stub.id).collect();
            self.selected_blocks.retain(|id| valid_ids.contains(id));
            if let Some(anchor) = self.anchor_block
                && !valid_ids.contains(&anchor)
            {
                self.anchor_block = None;
            }
            if self.selected_blocks.is_empty() {
                self.bulk_menu_pos = None;
            }
        } else {
            self.selected_blocks.clear();
            self.anchor_block = None;
            self.marquee_anchor = None;
            self.marquee_base_selection.clear();
            self.marquee_active = false;
            self.bulk_menu_pos = None;
        }
        self.title_focus = !same_page && document.page.title.is_empty();
        self.document = Some(document);
        self.cache = BlockCache::default();
        self.generation = generation;
        self.focus_position = focus;
        self.jump_to = focus.or(if same_page { None } else { Some(0) });
        self.last_width = 0.0;
        self.reset_scroll = same_page && focus.is_none();
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        breadcrumbs: &[crate::ui::BreadcrumbItem],
    ) -> Vec<Intent> {
        self.show_with_properties(ui, palette, breadcrumbs, None, &mut Default::default())
    }

    pub fn show_with_properties(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        breadcrumbs: &[crate::ui::BreadcrumbItem],
        record_properties: Option<(&crate::domain::model::DatabaseData, i64)>,
        value_drafts: &mut std::collections::HashMap<(i64, i64), String>,
    ) -> Vec<Intent> {
        let mut databases = HashMap::new();
        let mut database_loading = HashSet::new();
        let mut property_drafts = HashMap::new();
        let mut title_drafts = HashMap::new();
        let mut visible_databases = HashSet::new();
        self.show_with_widget_scope(
            ui,
            palette,
            breadcrumbs,
            None,
            &mut DatabaseContext {
                record_database_id: record_properties.map(|(database, _)| database.page_id),
                databases: &mut databases,
                loading: &mut database_loading,
                property_drafts: &mut property_drafts,
                title_drafts: &mut title_drafts,
                value_drafts,
                visible: &mut visible_databases,
            },
        )
    }

    pub fn show_with_properties_in_tab(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        breadcrumbs: &[crate::ui::BreadcrumbItem],
        record_properties: Option<(&crate::domain::model::DatabaseData, i64)>,
        value_drafts: &mut std::collections::HashMap<(i64, i64), String>,
        tab_instance: u64,
    ) -> Vec<Intent> {
        let mut databases = HashMap::new();
        let mut database_loading = HashSet::new();
        let mut property_drafts = HashMap::new();
        let mut title_drafts = HashMap::new();
        let mut visible_databases = HashSet::new();
        self.show_with_widget_scope(
            ui,
            palette,
            breadcrumbs,
            Some(tab_instance),
            &mut DatabaseContext {
                record_database_id: record_properties.map(|(database, _)| database.page_id),
                databases: &mut databases,
                loading: &mut database_loading,
                property_drafts: &mut property_drafts,
                title_drafts: &mut title_drafts,
                value_drafts,
                visible: &mut visible_databases,
            },
        )
    }

    pub fn show_with_database_context_in_tab(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        breadcrumbs: &[crate::ui::BreadcrumbItem],
        tab_instance: u64,
        context: &mut DatabaseContext<'_>,
    ) -> Vec<Intent> {
        self.show_with_widget_scope(ui, palette, breadcrumbs, Some(tab_instance), context)
    }

    fn show_with_widget_scope(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        breadcrumbs: &[crate::ui::BreadcrumbItem],
        widget_scope: Option<u64>,
        context: &mut DatabaseContext<'_>,
    ) -> Vec<Intent> {
        let Some(document) = self.document.as_mut() else {
            return empty_workspace(ui, palette);
        };
        let page_id = document.page.id;
        let record_properties = context
            .record_database_id
            .and_then(|database_id| context.databases.get(&database_id))
            .map(|database| (database, page_id));
        let (canvas, mut intents) = header::show(
            ui,
            document,
            &mut self.title_focus,
            palette,
            breadcrumbs,
            widget_scope,
            header::RecordPropertiesContext {
                record: record_properties,
                property_drafts: context.property_drafts,
                title_drafts: context.title_drafts,
                value_drafts: context.value_drafts,
            },
        );
        intents.extend(self.show_blocks(ui, canvas, palette, context));
        intents
    }
}

fn empty_workspace(ui: &mut Ui, palette: Palette) -> Vec<Intent> {
    let mut intents = Vec::new();
    ui.vertical_centered(|ui| {
        ui.add_space(150.0);
        ui.label(RichText::new(text::EMPTY_TITLE).size(30.0).strong());
        ui.add_space(10.0);
        ui.label(RichText::new(text::EMPTY_DESCRIPTION).color(palette.muted));
        ui.add_space(22.0);
        if ui.button("+ Create a new page").clicked() {
            intents.push(Intent::Action(Action::CreatePage(None)));
        }
    });
    intents
}
