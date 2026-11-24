use std::time::Instant;

use eframe::egui::{self};

use crate::config;
use crate::domain::model::{PageEdit, PageId};
use crate::ui::{Intent, palette::Palette};
use crate::worker::{Action, Request};

use super::NoteonApp;
use super::database_cache::sync_database_page;

impl NoteonApp {
    pub(super) fn action(&mut self, action: Action) {
        self.action_in_batch(action, false);
    }

    pub(super) fn action_in_batch(&mut self, action: Action, allow_if_busy: bool) {
        let is_value_edit = matches!(&action, Action::SetDatabaseProperty { .. });
        if (!allow_if_busy && self.busy && !is_value_edit) || !self.ready || self.closing {
            return;
        }
        if self.saves.failed {
            self.error = Some("Save pending changes before continuing.".into());
            return;
        }
        self.flush(true);
        if self.saves.failed {
            return;
        }
        match &action {
            Action::Trash(id) => {
                if self.page_dialog == Some(*id) {
                    self.page_dialog = None;
                }
                self.trashed_pages_history.push(vec![*id]);
                self.trashed_pages_redo.clear();
            }
            Action::TrashPages(ids) => {
                if let Some(dialog_id) = self.page_dialog
                    && ids.contains(&dialog_id)
                {
                    self.page_dialog = None;
                }
                if !ids.is_empty() {
                    self.trashed_pages_history.push(ids.clone());
                    self.trashed_pages_redo.clear();
                }
            }
            Action::DeletePermanently(id) => {
                if self.selected == Some(*id) {
                    self.selected = None;
                }
                if self.page_dialog == Some(*id) {
                    self.page_dialog = None;
                }
                self.editors.remove(id);
                self.editor_lru.retain(|p| p != id);
                for group in &mut self.trashed_pages_history {
                    group.retain(|gid| gid != id);
                }
                self.trashed_pages_history.retain(|group| !group.is_empty());
                for group in &mut self.trashed_pages_redo {
                    group.retain(|gid| gid != id);
                }
                self.trashed_pages_redo.retain(|group| !group.is_empty());
            }
            Action::DeletePagesPermanently(ids) => {
                for id in ids {
                    if self.selected == Some(*id) {
                        self.selected = None;
                    }
                    if self.page_dialog == Some(*id) {
                        self.page_dialog = None;
                    }
                    self.editors.remove(id);
                    self.editor_lru.retain(|p| p != id);
                    for group in &mut self.trashed_pages_history {
                        group.retain(|gid| gid != id);
                    }
                    for group in &mut self.trashed_pages_redo {
                        group.retain(|gid| gid != id);
                    }
                }
                self.trashed_pages_history.retain(|group| !group.is_empty());
                self.trashed_pages_redo.retain(|group| !group.is_empty());
            }
            Action::EmptyTrash => {
                let trashed: Vec<PageId> = self
                    .sidebar
                    .pages
                    .iter()
                    .filter(|p| p.trashed_by.is_some())
                    .map(|p| p.id)
                    .collect();
                for id in &trashed {
                    if self.selected == Some(*id) {
                        self.selected = None;
                    }
                    if self.page_dialog == Some(*id) {
                        self.page_dialog = None;
                    }
                    self.editors.remove(id);
                    self.editor_lru.retain(|p| p != id);
                }
                self.trashed_pages_history.clear();
                self.trashed_pages_redo.clear();
            }
            Action::Restore(id) => {
                for group in &mut self.trashed_pages_history {
                    group.retain(|gid| gid != id);
                }
                self.trashed_pages_history.retain(|group| !group.is_empty());
            }
            Action::RestorePages(ids) => {
                for id in ids {
                    for group in &mut self.trashed_pages_history {
                        group.retain(|gid| gid != id);
                    }
                }
                self.trashed_pages_history.retain(|group| !group.is_empty());
            }
            _ => {}
        }
        let sent = self.send(Request::Action(action));
        if !is_value_edit {
            self.busy = sent;
        }
    }

    fn page_changed(&mut self, page: PageEdit) {
        let previous_page = self
            .sidebar
            .page(page.id)
            .map(|previous| (previous.parent_id, previous.title.clone()));
        self.sidebar.update_page(&page);
        sync_database_page(&mut self.databases, &mut self.database_title_drafts, &page);
        if let Some((Some(parent_id), old_title)) = previous_page
            && old_title != page.title
            && let Some(parent_editor) = self.editors.get_mut(&parent_id)
        {
            parent_editor.find.refresh();
            let old_target = if old_title.trim().is_empty() {
                config::UNTITLED
            } else {
                old_title.trim()
            };
            let new_target = if page.title.trim().is_empty() {
                config::UNTITLED
            } else {
                page.title.trim()
            };
            for block in parent_editor.cache.blocks.values_mut() {
                let target = block.content.trim().strip_prefix('\u{2197}').map(str::trim);
                if block.kind == crate::domain::model::BlockKind::Text
                    && target.is_some_and(|target| {
                        target == old_target || (old_title.trim().is_empty() && target.is_empty())
                    })
                {
                    block.content = format!("\u{2197} {new_target}");
                    parent_editor.cache.needs_measure.insert(block.position);
                    self.saves.edit_block(block.clone());
                }
            }
        }
        if let Some(editor) = self.editors.get_mut(&page.id) {
            editor.find.refresh();
        }
        self.saves.edit_page(page);
    }

    pub(super) fn handle_intents(&mut self, intents: Vec<Intent>, context: &egui::Context) {
        let mut dispatched_action_in_batch = false;
        // Editing and Enter can occur in the same frame: persist the edited prefix first.
        let (edits, actions): (Vec<_>, Vec<_>) = intents
            .into_iter()
            .partition(|intent| matches!(intent, Intent::BlockChanged(_) | Intent::PageChanged(_)));
        for intent in edits.into_iter().chain(actions) {
            match intent {
                Intent::Open(id, jump) => {
                    self.pending_dialog = None;
                    self.page_dialog = None;
                    self.open_page(id, jump, true);
                }
                Intent::OpenTitle(title) => {
                    if let Some(id) = self.sidebar.find_by_title(&title, self.selected) {
                        self.pending_dialog = None;
                        self.page_dialog = None;
                        self.open_page(id, None, true);
                    }
                }
                Intent::OpenInTab(id) => {
                    self.pending_dialog = None;
                    self.page_dialog = None;
                    self.open_page_in_tab(id);
                }
                Intent::OpenInDialog(id) => self.open_page_in_dialog(id),
                Intent::ClosePageDialog => {
                    self.pending_dialog = None;
                    self.page_dialog = None;
                }
                Intent::OpenUrl(url) => {
                    let url = crate::ui::editor::links::normalize_web_url(&url);
                    context.open_url(egui::OpenUrl::new_tab(url));
                }
                Intent::OpenFileExplorer(path) => {
                    crate::ui::editor::links::open_in_file_explorer(&path);
                }
                Intent::DockPage(id, target_instance, side) => {
                    self.page_dialog = None;
                    self.dock_page_at_edge(id, target_instance, side);
                }
                Intent::LoadDatabase(page) if self.database_loading.insert(page) => {
                    if !self.send(Request::Database(page)) {
                        self.database_loading.remove(&page);
                    }
                }
                Intent::LoadDatabase(_) => {}
                Intent::ClosePageTab(id) => {
                    let page_still_open =
                        self.dock.iter_all_tabs().any(|(_, tab)| tab.page_id == id);
                    if !page_still_open {
                        self.editors.remove(&id);
                        self.editor_lru.retain(|page| *page != id);
                        self.pending_documents.retain(|_, (page, _)| *page != id);
                    }
                    if self.selected == Some(id) && !page_still_open {
                        self.selected = None;
                        self.loading = false;
                    }
                }
                Intent::Action(action) => {
                    if let Action::SetDatabaseRecordIcon { page, ref icon } = action {
                        let current_title = self
                            .sidebar
                            .page(page)
                            .map(|p| p.title.clone())
                            .unwrap_or_default();
                        self.sidebar.update_title(page, &current_title, icon);
                        if let Some(database) = self.databases.get_mut(&page) {
                            database.icon = icon.clone();
                        }
                        for database in self.databases.values_mut() {
                            if let Some(row) =
                                database.rows.iter_mut().find(|row| row.page.id == page)
                            {
                                row.page.icon = icon.clone();
                            }
                        }
                    }
                    self.action_in_batch(action, dispatched_action_in_batch);
                    dispatched_action_in_batch = true;
                }
                Intent::BlockChanged(block) => {
                    if let Some(editor) = self.editors.get_mut(&block.page_id) {
                        editor.find.refresh();
                    }
                    if block.position == 0 && block.kind == crate::domain::model::BlockKind::Text {
                        for database in self.databases.values_mut() {
                            if let Some(row) = database
                                .rows
                                .iter_mut()
                                .find(|row| row.page.id == block.page_id)
                            {
                                row.preview.clone_from(&block.content);
                            }
                        }
                    }
                    self.saves.edit_block(block);
                }
                Intent::PageChanged(page) => self.page_changed(page),
                Intent::LoadBlocks {
                    page,
                    generation,
                    start,
                } => {
                    if !self.send(Request::Blocks {
                        page,
                        generation,
                        start,
                    }) && let Some(editor) = self.editors.get_mut(&page)
                    {
                        editor.cache.pending.remove(&start);
                    }
                }
                Intent::Search => self.search.open(),
                Intent::ToggleAssistant => self.assistant.toggle(),
                Intent::FindInPage => {
                    let target_id = self.page_dialog.or(self.selected);
                    if let Some(page_id) = target_id {
                        if self.databases.get(&page_id).is_some_and(|database| {
                            database.view == crate::domain::model::DatabaseView::Table
                        }) {
                            context.data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::new(("database-table-find-open", page_id)),
                                    true,
                                );
                                data.insert_temp(
                                    egui::Id::new(("database-table-find-focus", page_id)),
                                    true,
                                );
                            });
                        } else if let Some(editor) = self.editors.get_mut(&page_id) {
                            editor.find.open();
                        }
                    }
                }
                Intent::ToggleSidebarPin => {
                    self.sidebar_pinned = !self.sidebar_pinned;
                    if self.sidebar_pinned {
                        context.data_mut(|d| {
                            d.insert_temp(egui::Id::new("sidebar_autohide_open"), false);
                        });
                    } else {
                        context.data_mut(|d| {
                            d.insert_temp(egui::Id::new("sidebar_autohide_open"), true);
                        });
                    }
                    self.send(Request::Setting {
                        key: config::SettingKey::SidebarPinned,
                        value: self.sidebar_pinned.to_string(),
                    });
                }
                Intent::ToggleTheme => {
                    self.dark = !self.dark;
                    self.apply_theme(context);
                    self.send(Request::Setting {
                        key: config::SettingKey::Dark,
                        value: self.dark.to_string(),
                    });
                }
                Intent::SetTheme(dark) if self.dark != dark => {
                    self.dark = dark;
                    self.apply_theme(context);
                    self.send(Request::Setting {
                        key: config::SettingKey::Dark,
                        value: dark.to_string(),
                    });
                }
                Intent::SetTheme(_) => {}
                Intent::SetBackground(color) if color != self.background_color() => {
                    if self.dark {
                        self.dark_background = color;
                    } else {
                        self.light_background = color;
                    }
                    self.apply_theme(context);
                    self.send(Request::Setting {
                        key: if self.dark {
                            config::SettingKey::DarkBackground
                        } else {
                            config::SettingKey::LightBackground
                        },
                        value: Palette::to_setting(color),
                    });
                }
                Intent::SetBackground(_) => {}
                Intent::SetAccent(color) if self.accent_color() != color => {
                    self.accent_color = Some(color);
                    self.apply_theme(context);
                    self.send(Request::Setting {
                        key: config::SettingKey::AccentColor,
                        value: Palette::to_setting(color),
                    });
                }
                Intent::SetAccent(_) => {}
                Intent::SetFontScale(scale) if (self.font_scale - scale).abs() > f32::EPSILON => {
                    self.font_scale = scale.clamp(config::MIN_FONT_SCALE, config::MAX_FONT_SCALE);
                    self.apply_theme(context);
                    self.send(Request::Setting {
                        key: config::SettingKey::FontScale,
                        value: self.font_scale.to_string(),
                    });
                }
                Intent::SetFontScale(_) => {}
                Intent::Trash => {
                    self.flush(true);
                    self.trash = true;
                }
                Intent::Settings => self.settings_open = true,
                Intent::OpenSettingsTheme(section) => {
                    self.settings_open = true;
                    self.settings_theme_tab = true;
                    self.settings_theme_highlight = Some((section, Instant::now()));
                }
                Intent::SetCustomThemeColor { key, value } => {
                    let hex = value.map(Palette::to_setting);
                    {
                        let theme = self.active_custom_theme_mut();
                        match key {
                            crate::ui::CustomColorKey::Sidebar => theme.sidebar = hex,
                            crate::ui::CustomColorKey::Text => theme.text = hex,
                            crate::ui::CustomColorKey::Muted => theme.muted = hex,
                            crate::ui::CustomColorKey::Border => theme.border = hex,
                            crate::ui::CustomColorKey::AccentSoft => theme.accent_soft = hex,
                            crate::ui::CustomColorKey::Hover => theme.hover = hex,
                            crate::ui::CustomColorKey::Code => theme.code = hex,
                            crate::ui::CustomColorKey::GalleryCard => theme.gallery_card = hex,
                            crate::ui::CustomColorKey::GalleryBorder => theme.gallery_border = hex,
                            crate::ui::CustomColorKey::BoardCard => theme.board_card = hex,
                        }
                    }
                    self.apply_theme(context);
                    if let Ok(value) = serde_json::to_string(&self.custom_theme) {
                        self.send(Request::Setting {
                            key: config::SettingKey::CustomTheme,
                            value,
                        });
                    }
                }
                Intent::ResetCustomThemeSection(section) => {
                    {
                        let theme = self.active_custom_theme_mut();
                        match section {
                            crate::ui::ThemeSection::General => {
                                theme.border = None;
                                theme.accent_soft = None;
                                theme.hover = None;
                            }
                            crate::ui::ThemeSection::Sidebar => {
                                theme.sidebar = None;
                            }
                            crate::ui::ThemeSection::Editor => {
                                theme.text = None;
                                theme.muted = None;
                                theme.code = None;
                            }
                            crate::ui::ThemeSection::Header => {
                                theme.text = None;
                                theme.muted = None;
                            }
                            crate::ui::ThemeSection::Gallery => {
                                theme.gallery_card = None;
                                theme.gallery_border = None;
                            }
                            crate::ui::ThemeSection::Board => {
                                theme.board_card = None;
                            }
                            crate::ui::ThemeSection::Table => {
                                theme.border = None;
                                theme.hover = None;
                            }
                        }
                    }
                    self.apply_theme(context);
                    if let Ok(value) = serde_json::to_string(&self.custom_theme) {
                        self.send(Request::Setting {
                            key: config::SettingKey::CustomTheme,
                            value,
                        });
                    }
                }
                Intent::ResetAllCustomTheme => {
                    *self.active_custom_theme_mut() = crate::ui::palette::CustomTheme::default();
                    self.apply_theme(context);
                    if let Ok(value) = serde_json::to_string(&self.custom_theme) {
                        self.send(Request::Setting {
                            key: config::SettingKey::CustomTheme,
                            value,
                        });
                    }
                }
                Intent::Help => self.help_open = true,
                Intent::RetrySave => {
                    if self.saves.failed {
                        for editor in self.editors.values_mut() {
                            editor.find.refresh();
                        }
                    }
                    self.saves.retry();
                    self.error = None;
                    self.flush(true);
                }
                Intent::Back => self.navigate(-1),
                Intent::Forward => self.navigate(1),
                Intent::Undo => self.action(Action::Undo),
                Intent::Redo => self.action(Action::Redo),
            }
        }
    }

    pub(super) fn shortcuts(&mut self, context: &egui::Context) {
        let mut intents = Vec::new();
        let has_selected_blocks = self
            .editors
            .values()
            .any(|editor| !editor.selected_blocks.is_empty());
        let has_selected_database_rows = self.selected.is_some_and(|page_id| {
            let selection_id = egui::Id::new(("database-selected-rows", page_id));
            context
                .data(|d| d.get_temp::<std::collections::HashSet<PageId>>(selection_id))
                .is_some_and(|rows| !rows.is_empty())
        });
        let wants_keyboard = context.wants_keyboard_input();
        let deletable_pages: Vec<PageId> = if self.ready
            && !self.busy
            && !self.closing
            && !self.trash
            && !has_selected_blocks
            && !has_selected_database_rows
            && !wants_keyboard
            && self.page_dialog.is_none()
            && !self.sidebar.selected_pages.is_empty()
        {
            self.sidebar
                .selected_pages
                .iter()
                .copied()
                .filter(|id| {
                    self.sidebar
                        .page(*id)
                        .is_some_and(|page| page.trashed_by.is_none())
                })
                .collect()
        } else {
            Vec::new()
        };
        context.input_mut(|input| {
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::K) {
                intents.push(Intent::Search);
            }
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::J) {
                intents.push(Intent::ToggleAssistant);
            }
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::F)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::F)
            {
                intents.push(Intent::FindInPage);
            }
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::B) {
                intents.push(Intent::ToggleSidebarPin);
            }
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::N) {
                intents.push(Intent::Action(Action::CreatePage(None)));
            }
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::S) {
                intents.push(Intent::RetrySave);
            }
            if input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft) {
                intents.push(Intent::Back);
            }
            if input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight) {
                intents.push(Intent::Forward);
            }
            if !wants_keyboard {
                let redo = input
                    .consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
                    || input.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::Z,
                    )
                    || input.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
                let undo = input.consume_key(egui::Modifiers::CTRL, egui::Key::Z)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
                if redo {
                    intents.push(Intent::Redo);
                } else if undo {
                    intents.push(Intent::Undo);
                }
            }
            if !deletable_pages.is_empty()
                && input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            {
                if deletable_pages.len() == 1 {
                    intents.push(Intent::Action(Action::Trash(deletable_pages[0])));
                } else {
                    intents.push(Intent::Action(Action::TrashPages(deletable_pages)));
                }
            }
        });
        if !self.sidebar.selected_pages.is_empty()
            && intents.iter().any(|i| {
                matches!(
                    i,
                    Intent::Action(Action::Trash(_)) | Intent::Action(Action::TrashPages(_))
                )
            })
        {
            self.sidebar.selected_pages.clear();
            self.sidebar.anchor_page = None;
            self.sidebar.anchor_row_index = None;
        }
        self.handle_intents(intents, context);
    }
}
