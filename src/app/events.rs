use std::collections::HashSet;

use crate::ui::palette::Palette;
use crate::worker::{Event, FailedOperation};
use eframe::egui;

use super::NoteonApp;

impl NoteonApp {
    pub(super) fn receive_events(&mut self, context: &egui::Context) {
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|worker| worker.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Ready {
                    mut pages,
                    last_page,
                    dark,
                    light_background,
                    dark_background,
                    accent_color,
                    font_scale,
                    dock_layout,
                    sidebar_pinned,
                    custom_theme,
                } => {
                    for page in &mut pages {
                        self.saves.overlay_page(page);
                    }
                    self.sidebar.set_pages(pages);
                    if let Some(pinned) = sidebar_pinned {
                        self.sidebar_pinned = pinned.as_str() != "false";
                    }
                    self.dark = dark;
                    if let Some(color) = light_background.as_deref().and_then(Palette::from_setting)
                    {
                        self.light_background = color;
                    }
                    if let Some(color) = dark_background.as_deref().and_then(Palette::from_setting)
                    {
                        self.dark_background = color;
                    }
                    self.accent_color = accent_color.as_deref().and_then(Palette::from_setting);
                    self.font_scale = font_scale
                        .and_then(|value| value.parse().ok())
                        .filter(|value: &f32| value.is_finite())
                        .unwrap_or(crate::config::DEFAULT_FONT_SCALE)
                        .clamp(crate::config::MIN_FONT_SCALE, crate::config::MAX_FONT_SCALE);
                    if let Some(raw) = custom_theme {
                        if let Ok(config) = serde_json::from_str(&raw) {
                            self.custom_theme = config;
                        }
                    }
                    self.apply_theme(context);
                    if let Some(layout) = dock_layout {
                        let valid_pages: HashSet<_> = self
                            .sidebar
                            .pages
                            .iter()
                            .filter(|page| page.trashed_by.is_none())
                            .map(|page| page.id)
                            .collect();
                        if let Some((dock, next_tab_instance_id)) =
                            super::tabs::restore_layout(&layout, &valid_pages)
                        {
                            self.dock = dock;
                            self.next_tab_instance_id = next_tab_instance_id;
                            self.saved_dock_layout = serde_json::to_string(&self.dock).ok();
                        }
                    }
                    self.ready = true;
                    if let Some(id) = last_page.or_else(|| {
                        self.sidebar
                            .pages
                            .iter()
                            .find(|page| page.trashed_by.is_none())
                            .map(|page| page.id)
                    }) {
                        self.open_page(id, None, true);
                    }
                }
                Event::Document {
                    generation,
                    mut document,
                    history,
                } => {
                    if let Some((page_id, jump)) = self.pending_documents.remove(&generation)
                        && page_id == document.page.id
                    {
                        if self.selected == Some(page_id) {
                            self.loading = false;
                        }
                        self.sidebar.reveal(page_id);
                        self.saves.overlay_page(&mut document.page);
                        let is_pending_dialog = self.pending_dialog == Some(page_id);
                        let is_selected_without_jump =
                            self.selected == Some(page_id) && jump.is_none();
                        let editor = self.editors.entry(page_id).or_default();
                        editor.bind(document, generation, jump);
                        editor.history = history;
                        if is_pending_dialog || is_selected_without_jump {
                            editor.title_focus = true;
                        }
                        self.touch_editor(page_id);
                        if is_pending_dialog {
                            self.pending_dialog = None;
                            self.page_dialog = Some(page_id);
                        }
                    }
                }
                Event::Blocks {
                    generation,
                    start,
                    mut blocks,
                } => {
                    if let Some(editor) = self
                        .editors
                        .values_mut()
                        .find(|editor| editor.generation == generation)
                    {
                        self.saves.overlay_blocks(&mut blocks);
                        editor.cache.insert(start, blocks);
                        if editor.find.open {
                            editor
                                .find
                                .update_matches(editor.document.as_ref(), &editor.cache.blocks);
                        }
                    }
                }
                Event::Database(mut data) => {
                    for row in &mut data.rows {
                        self.saves.overlay_page(&mut row.page);
                    }
                    super::sync_database_title_drafts(
                        self.databases.get(&data.page_id),
                        &data,
                        &mut self.database_title_drafts,
                    );
                    super::sync_database_value_drafts(
                        self.databases.get(&data.page_id),
                        &data,
                        &mut self.database_value_drafts,
                    );
                    self.database_loading.remove(&data.page_id);
                    self.touch_database(data.page_id);
                    self.databases.insert(data.page_id, data);
                }
                Event::Search { generation, hits } if generation == self.search.generation => {
                    self.search.hits = hits;
                    self.search.loading = false;
                }
                Event::Saved { id, histories } => {
                    self.saves.acknowledge(id);
                    for (page_id, history) in histories {
                        if let Some(editor) = self.editors.get_mut(&page_id) {
                            editor.history = history;
                        }
                    }
                }
                Event::ActionDone {
                    mut pages,
                    document,
                    preserve_view,
                    page_properties,
                    focus,
                    focus_at_end,
                    notice,
                    refresh_database,
                    created_record_document,
                    focus_created_record_name,
                    history,
                    page_transfer,
                } => {
                    self.busy = false;
                    if let Some(update) = page_transfer {
                        crate::ui::page_transfer::finish_cut(context, &update.moved_pages);
                        for database in update.databases {
                            self.refresh_database(database);
                        }
                        for document in update.documents {
                            if let Some(editor) = self.editors.get_mut(&document.page.id) {
                                self.generation += 1;
                                editor.bind(document, self.generation, None);
                            }
                        }
                    }
                    for page in &mut pages {
                        self.saves.overlay_page(page);
                    }
                    self.sidebar.set_pages(pages);
                    self.databases.retain(|id, _| {
                        self.sidebar
                            .page(*id)
                            .is_some_and(|page| page.is_database && page.trashed_by.is_none())
                    });
                    self.database_lru
                        .retain(|id| self.databases.contains_key(id));
                    if let Some(database) = refresh_database {
                        self.refresh_database(database);
                    }
                    if let Some((page_id, properties)) = page_properties
                        && let Some(document) = self
                            .editors
                            .get_mut(&page_id)
                            .and_then(|editor| editor.document.as_mut())
                    {
                        document.properties = properties;
                    }
                    if let Some(mut document) = document {
                        let id = document.page.id;
                        let in_dialog = self.page_dialog == Some(id);
                        if !in_dialog && !preserve_view {
                            if self.selected != Some(id) {
                                self.history.truncate(self.history_index + 1);
                                self.history.push(id);
                                self.history_index = self.history.len() - 1;
                            }
                            self.activate_page_tab(id, false);
                            self.selected = Some(id);
                            self.trash = false;
                            self.sidebar.reveal(id);
                            self.record_navigation(id, false);
                        }
                        self.generation += 1;
                        if self.selected == Some(id) {
                            self.loading = false;
                        }
                        self.saves.overlay_page(&mut document.page);
                        self.editors.entry(id).or_default().bind_with_focus(
                            document,
                            self.generation,
                            focus,
                            focus_at_end,
                        );
                        if let Some(hist) = history {
                            if let Some(editor) = self.editors.get_mut(&id) {
                                editor.history = hist;
                            }
                        }
                        self.touch_editor(id);
                    } else if let Some(mut doc) = created_record_document {
                        let id = doc.page.id;
                        let db_id = refresh_database.or(doc.page.parent_id);
                        let open_mode = if focus_created_record_name {
                            crate::domain::model::NewRecordOpenMode::FocusName
                        } else {
                            db_id
                                .and_then(|db| self.databases.get(&db))
                                .map(|db| db.view_settings.new_record_open_mode)
                                .unwrap_or_default()
                        };
                        match open_mode {
                            crate::domain::model::NewRecordOpenMode::FocusName => {
                                if let Some(db) = db_id {
                                    context.data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new(("pending-table-focus-record", db)),
                                            id,
                                        );
                                    });
                                }
                            }
                            crate::domain::model::NewRecordOpenMode::Dialog => {
                                self.generation += 1;
                                self.saves.overlay_page(&mut doc.page);
                                let mut editor = crate::ui::editor::Editor::default();
                                editor.bind(doc, self.generation, None);
                                editor.title_focus = true;
                                if let Some(hist) = history {
                                    editor.history = hist;
                                }
                                self.editors.insert(id, editor);
                                self.touch_editor(id);
                                self.page_dialog = Some(id);
                            }
                            crate::domain::model::NewRecordOpenMode::FullPage => {
                                self.page_dialog = None;
                                if self.selected != Some(id) {
                                    self.history.truncate(self.history_index + 1);
                                    self.history.push(id);
                                    self.history_index = self.history.len() - 1;
                                }
                                self.activate_page_tab(id, false);
                                self.selected = Some(id);
                                self.trash = false;
                                self.sidebar.reveal(id);
                                self.record_navigation(id, false);
                                self.generation += 1;
                                self.loading = false;
                                self.saves.overlay_page(&mut doc.page);
                                let mut editor = crate::ui::editor::Editor::default();
                                editor.bind(doc, self.generation, None);
                                editor.title_focus = true;
                                if let Some(hist) = history {
                                    editor.history = hist;
                                }
                                self.editors.insert(id, editor);
                                self.touch_editor(id);
                            }
                        }
                    } else {
                        if let Some(hist) = history {
                            if let Some(page) = self.selected {
                                if let Some(editor) = self.editors.get_mut(&page) {
                                    editor.history = hist;
                                }
                            }
                        }
                        for editor in self.editors.values_mut() {
                            if let Some(document) = editor.document.as_mut()
                                && let Some(page) = self.sidebar.page(document.page.id)
                            {
                                document.page.favorite = page.favorite;
                                document.page.parent_id = page.parent_id;
                                document.page.trashed_by = page.trashed_by;
                                document.page.priority = page.priority;
                            }
                        }
                        if self.selected.is_some_and(|id| {
                            self.sidebar
                                .page(id)
                                .is_none_or(|page| page.trashed_by.is_some())
                        }) {
                            self.selected = None;
                            self.loading = false;
                        }
                        let valid_pages: HashSet<_> = self
                            .sidebar
                            .pages
                            .iter()
                            .filter(|page| page.trashed_by.is_none())
                            .map(|page| page.id)
                            .collect();
                        self.dock = self.dock.filter_map_tabs(|tab| {
                            valid_pages.contains(&tab.page_id).then_some(*tab)
                        });
                    }
                    if let Some(message) = notice {
                        self.notice = Some((message, std::time::Instant::now()));
                    }
                    if let Some(page) = self.selected
                        && self.sidebar.page(page).is_some_and(|page| page.is_database)
                    {
                        self.invalidate_database(page);
                    }
                }
                Event::Failed { operation, message } => {
                    match operation {
                        FailedOperation::Save(id) => {
                            self.saves.fail(id);
                            self.closing = false;
                        }
                        FailedOperation::Action => self.busy = false,
                        FailedOperation::Document(generation) => {
                            if let Some((page, _)) = self.pending_documents.remove(&generation)
                                && self.selected == Some(page)
                            {
                                self.loading = false;
                            }
                        }
                        FailedOperation::Blocks(start, generation) => {
                            if let Some(editor) = self
                                .editors
                                .values_mut()
                                .find(|editor| editor.generation == generation)
                            {
                                editor.cache.pending.remove(&start);
                            }
                        }
                        FailedOperation::Database(page) => {
                            self.database_loading.remove(&page);
                        }
                        FailedOperation::Search(generation)
                            if generation == self.search.generation =>
                        {
                            self.search.loading = false
                        }
                        _ => {}
                    }
                    self.error = Some(message);
                }
                Event::UndoEmpty
                | Event::RedoEmpty
                | Event::PageUndoEmpty { page_id: _ }
                | Event::PageRedoEmpty { page_id: _ } => {
                    self.busy = false;
                }
                _ => {}
            }
        }
    }
}
