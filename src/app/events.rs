use std::collections::HashSet;

use crate::ui::palette::Palette;
use crate::worker::{Action, Event, FailedOperation};
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
                } => {
                    for page in &mut pages {
                        self.saves.overlay_page(page);
                    }
                    self.sidebar.set_pages(pages);
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
                } => {
                    if let Some((page_id, jump)) = self.pending_documents.remove(&generation)
                        && page_id == document.page.id
                    {
                        if self.selected == Some(page_id) {
                            self.loading = false;
                        }
                        self.sidebar.reveal(page_id);
                        self.saves.overlay_page(&mut document.page);
                        self.editors
                            .entry(page_id)
                            .or_default()
                            .bind(document, generation, jump);
                        self.touch_editor(page_id);
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
                    }
                }
                Event::Database(mut data) => {
                    for row in &mut data.rows {
                        self.saves.overlay_page(&mut row.page);
                    }
                    self.database_loading.remove(&data.page_id);
                    self.touch_database(data.page_id);
                    self.databases.insert(data.page_id, data);
                }
                Event::Search { generation, hits } if generation == self.search.generation => {
                    self.search.hits = hits;
                    self.search.loading = false;
                }
                Event::Saved(id) => self.saves.acknowledge(id),
                Event::ActionDone {
                    mut pages,
                    document,
                    page_properties,
                    focus,
                    notice,
                    refresh_database,
                } => {
                    self.busy = false;
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
                        self.invalidate_database(database);
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
                        if self.selected != Some(id) {
                            self.history.truncate(self.history_index + 1);
                            self.history.push(id);
                            self.history_index = self.history.len() - 1;
                        }
                        self.activate_page_tab(id, false);
                        self.selected = Some(id);
                        self.trash = false;
                        self.generation += 1;
                        self.loading = false;
                        self.saves.overlay_page(&mut document.page);
                        self.editors
                            .entry(id)
                            .or_default()
                            .bind(document, self.generation, focus);
                        self.touch_editor(id);
                        self.sidebar.reveal(id);
                        self.record_navigation(id, false);
                    } else {
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
                Event::PageUndoEmpty { page_id: _ } => {
                    self.busy = false;
                    if let Some(trashed_ids) = self.trashed_pages_history.pop() {
                        self.trashed_pages_redo.push(trashed_ids.clone());
                        self.action(Action::RestorePages(trashed_ids));
                    }
                }
                Event::PageRedoEmpty { page_id: _ } => {
                    self.busy = false;
                    if let Some(trashed_ids) = self.trashed_pages_redo.pop() {
                        self.trashed_pages_history.push(trashed_ids.clone());
                        self.action(Action::TrashPages(trashed_ids));
                    }
                }
                _ => {}
            }
        }
    }
}
