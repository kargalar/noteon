mod events;
mod saves;
mod tabs;
mod views;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};
use egui_dock::{DockState, Node, Split};

use crate::config;
use crate::domain::model::{DatabaseData, PageEdit, PageId};
use crate::ui::{
    DockSide, Intent, editor::Editor, palette::Palette, search::Search, sidebar::Sidebar, theme,
};
use crate::worker::{Action, Request, Worker};

use saves::Saves;
use tabs::PageTab;

const MAX_RESIDENT_EDITORS: usize = 12;
const MAX_RESIDENT_DATABASES: usize = 8;

pub struct NoteonApp {
    worker: Option<Worker>,
    data_directory: PathBuf,
    sidebar: Sidebar,
    editors: HashMap<PageId, Editor>,
    editor_lru: VecDeque<PageId>,
    dock: DockState<PageTab>,
    next_tab_instance_id: u64,
    saved_dock_layout: Option<String>,
    search: Search,
    saves: Saves,
    generation: u64,
    selected: Option<PageId>,
    history: Vec<PageId>,
    history_index: usize,
    pending_documents: HashMap<u64, (PageId, Option<usize>)>,
    databases: HashMap<PageId, DatabaseData>,
    database_loading: std::collections::HashSet<PageId>,
    database_lru: VecDeque<PageId>,
    database_property_drafts: HashMap<i64, (String, String)>,
    database_title_drafts: HashMap<PageId, String>,
    database_value_drafts: HashMap<(PageId, i64), String>,
    ready: bool,
    loading: bool,
    busy: bool,
    dark: bool,
    light_background: Color32,
    dark_background: Color32,
    accent_color: Option<Color32>,
    font_scale: f32,
    trash: bool,
    settings_open: bool,
    settings_theme_tab: bool,
    help_open: bool,
    error: Option<String>,
    notice: Option<(String, Instant)>,
    closing: bool,
    allow_close: bool,
}

impl NoteonApp {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        theme::configure(&creation.egui_ctx);
        let directory = config::data_directory();
        let data_directory = directory.as_ref().cloned().unwrap_or_default();
        let worker = directory.and_then(|path| {
            Worker::start(path.join(config::DATABASE_FILE), creation.egui_ctx.clone())
        });
        let error = worker.as_ref().err().map(ToString::to_string);
        Self {
            worker: worker.ok(),
            data_directory,
            sidebar: Sidebar::default(),
            editors: HashMap::new(),
            editor_lru: VecDeque::new(),
            dock: DockState::new(Vec::new()),
            next_tab_instance_id: 1,
            saved_dock_layout: None,
            search: Search::default(),
            saves: Saves::default(),
            generation: 0,
            selected: None,
            history: Vec::new(),
            history_index: 0,
            pending_documents: HashMap::new(),
            databases: HashMap::new(),
            database_loading: std::collections::HashSet::new(),
            database_lru: VecDeque::new(),
            database_property_drafts: HashMap::new(),
            database_title_drafts: HashMap::new(),
            database_value_drafts: HashMap::new(),
            ready: false,
            loading: false,
            busy: false,
            dark: false,
            light_background: Palette::new(false).background,
            dark_background: Palette::new(true).background,
            accent_color: None,
            font_scale: config::DEFAULT_FONT_SCALE,
            trash: false,
            settings_open: false,
            settings_theme_tab: false,
            help_open: false,
            error,
            notice: None,
            closing: false,
            allow_close: false,
        }
    }

    fn send(&mut self, request: Request) -> bool {
        let result = self
            .worker
            .as_ref()
            .ok_or(crate::error::Error::WorkerUnavailable)
            .and_then(|worker| worker.send(request));
        if let Err(error) = result {
            self.error = Some(error.to_string());
            false
        } else {
            true
        }
    }

    fn background_color(&self) -> Color32 {
        if self.dark {
            self.dark_background
        } else {
            self.light_background
        }
    }

    fn accent_color(&self) -> Color32 {
        self.accent_color
            .unwrap_or_else(|| Palette::new(self.dark).accent)
    }

    fn current_palette(&self) -> Palette {
        Palette::new(self.dark).customized(self.background_color(), self.accent_color())
    }

    fn apply_theme(&self, context: &egui::Context) {
        theme::apply(
            context,
            self.dark,
            Some(self.background_color()),
            Some(self.accent_color()),
            self.font_scale,
        );
    }

    fn flush(&mut self, force: bool) {
        if let Some((id, batch)) = self.saves.next_batch(force)
            && !self.send(Request::Save { id, batch })
        {
            self.saves.fail(id);
        }
    }

    fn open_page(&mut self, id: PageId, jump: Option<usize>, add_history: bool) {
        self.open_page_with_mode(id, jump, add_history, false);
    }

    fn open_page_in_tab(&mut self, id: PageId) {
        self.open_page_with_mode(id, None, true, true);
    }

    fn open_page_with_mode(
        &mut self,
        id: PageId,
        jump: Option<usize>,
        add_history: bool,
        new_tab: bool,
    ) {
        if self.busy || self.closing || !self.ready {
            return;
        }
        if self
            .sidebar
            .page(id)
            .is_none_or(|page| page.trashed_by.is_some())
        {
            return;
        }
        self.activate_page_tab(id, new_tab);
        self.open_page_content(id, jump, add_history);
    }

    fn open_page_content(&mut self, id: PageId, jump: Option<usize>, add_history: bool) {
        if self.sidebar.page(id).is_some_and(|page| page.is_database) {
            self.selected = Some(id);
            self.loading = false;
            self.trash = false;
            self.sidebar.reveal(id);
            self.record_navigation(id, add_history);
            return;
        }
        if let Some(editor) = self.editors.get_mut(&id) {
            self.selected = Some(id);
            self.loading = false;
            if jump.is_some() {
                editor.jump_to = jump;
            }
            self.sidebar.reveal(id);
            self.record_navigation(id, add_history);
            self.touch_editor(id);
            return;
        }
        if let Some((_, pending_jump)) = self
            .pending_documents
            .values_mut()
            .find(|(page, _)| *page == id)
        {
            *pending_jump = jump;
            self.selected = Some(id);
            self.loading = true;
            return;
        }
        self.flush(true);
        self.trash = false;
        self.selected = Some(id);
        self.generation += 1;
        self.pending_documents.insert(self.generation, (id, jump));
        self.loading = self.send(Request::Document {
            page: id,
            generation: self.generation,
        });
        if !self.loading {
            self.pending_documents.remove(&self.generation);
        }
        self.sidebar.reveal(id);
        self.record_navigation(id, add_history);
    }

    fn record_navigation(&mut self, id: PageId, add_history: bool) {
        if add_history {
            if !self.history.is_empty() {
                self.history.truncate(self.history_index + 1);
            }
            if self.history.last() != Some(&id) {
                self.history.push(id);
            }
            self.history_index = self.history.len().saturating_sub(1);
        }
        self.send(Request::Setting {
            key: config::SettingKey::LastPage,
            value: id.to_string(),
        });
    }

    fn activate_page_tab(&mut self, id: PageId, new_tab: bool) {
        if !new_tab {
            if self
                .dock
                .find_active_focused()
                .is_some_and(|(_, tab)| tab.page_id == id)
            {
                return;
            }
            let existing_tab = self
                .dock
                .iter_all_tabs()
                .map(|(_, tab)| *tab)
                .find(|tab| tab.page_id == id);
            if let Some(existing_tab) = existing_tab
                && let Some((surface, node, tab)) = self.dock.find_tab(&existing_tab)
            {
                self.dock.set_focused_node_and_surface((surface, node));
                self.dock.set_active_tab((surface, node, tab));
                return;
            }
        }
        let page_tab = PageTab {
            page_id: id,
            instance_id: self.next_tab_instance_id,
        };
        self.next_tab_instance_id = self.next_tab_instance_id.saturating_add(1);
        if !new_tab {
            let replaced = self.dock.find_active_focused().map(|(_, active_tab)| {
                let previous = *active_tab;
                *active_tab = page_tab;
                previous.page_id
            });
            if let Some(previous) = replaced
                && previous != id
                && !self
                    .dock
                    .iter_all_tabs()
                    .any(|(_, tab)| tab.page_id == previous)
            {
                self.editors.remove(&previous);
                self.editor_lru.retain(|page| *page != previous);
                self.pending_documents
                    .retain(|_, (page, _)| *page != previous);
            }
            if replaced.is_some() {
                return;
            }
        }
        self.dock.push_to_focused_leaf(page_tab);
    }

    fn persist_dock_layout(&mut self) {
        let Ok(layout) = serde_json::to_string(&self.dock) else {
            return;
        };
        if self.saved_dock_layout.as_deref() == Some(&layout) {
            return;
        }
        if self.send(Request::Setting {
            key: config::SettingKey::DockLayout,
            value: layout.clone(),
        }) {
            self.saved_dock_layout = Some(layout);
        }
    }

    fn evict_inactive_editors(&mut self, visible_pages: &std::collections::HashSet<PageId>) {
        while self.editors.len() > MAX_RESIDENT_EDITORS {
            let Some(page) = self.editor_lru.pop_front() else {
                break;
            };
            if visible_pages.contains(&page) {
                self.editor_lru.push_back(page);
                if self
                    .editor_lru
                    .iter()
                    .all(|candidate| visible_pages.contains(candidate))
                {
                    break;
                }
            } else {
                self.editors.remove(&page);
            }
        }
    }

    fn touch_editor(&mut self, page: PageId) {
        self.editor_lru.retain(|candidate| *candidate != page);
        self.editor_lru.push_back(page);
    }

    fn evict_inactive_databases(&mut self, visible: &std::collections::HashSet<PageId>) {
        while self.databases.len() > MAX_RESIDENT_DATABASES {
            let Some(page) = self.database_lru.pop_front() else {
                break;
            };
            if visible.contains(&page) {
                self.database_lru.push_back(page);
                if self
                    .database_lru
                    .iter()
                    .all(|candidate| visible.contains(candidate))
                {
                    break;
                }
            } else {
                self.invalidate_database(page);
            }
        }
    }

    fn touch_database(&mut self, page: PageId) {
        self.database_lru.retain(|candidate| *candidate != page);
        self.database_lru.push_back(page);
    }

    fn navigate(&mut self, direction: isize) {
        if self.busy || self.closing {
            return;
        }
        let next = self.history_index as isize + direction;
        if next >= 0 && (next as usize) < self.history.len() {
            self.history_index = next as usize;
            self.open_page(self.history[self.history_index], None, false);
        }
    }

    fn action(&mut self, action: Action) {
        if self.busy || !self.ready || self.closing {
            return;
        }
        if self.saves.failed {
            self.error = Some("Save pending changes before continuing.".into());
            return;
        }
        self.flush(true);
        self.busy = self.send(Request::Action(action));
    }

    fn page_changed(&mut self, page: PageEdit) {
        self.sidebar.update_title(page.id, &page.title, &page.icon);
        if let Some(database) = self.databases.get_mut(&page.id) {
            database.title.clone_from(&page.title);
            database.icon.clone_from(&page.icon);
        }
        for database in self.databases.values_mut() {
            if let Some(row) = database.rows.iter_mut().find(|row| row.page.id == page.id) {
                row.page.title.clone_from(&page.title);
                row.page.icon.clone_from(&page.icon);
                row.page.cover = page.cover;
                row.page.cover_image.clone_from(&page.cover_image);
                row.page.cover_x = page.cover_x;
                row.page.cover_y = page.cover_y;
            }
        }
        self.saves.edit_page(page);
    }

    fn invalidate_database(&mut self, page: PageId) {
        if let Some(database) = self.databases.remove(&page) {
            for row in database.rows {
                self.database_title_drafts.remove(&row.page.id);
                for property in &database.properties {
                    self.database_value_drafts
                        .remove(&(row.page.id, property.id));
                }
            }
            for property in database.properties {
                self.database_property_drafts.remove(&property.id);
            }
        }
        self.database_lru.retain(|candidate| *candidate != page);
        self.database_loading.remove(&page);
    }

    fn handle_intents(&mut self, intents: Vec<Intent>, context: &egui::Context) {
        // Editing and Enter can occur in the same frame: persist the edited prefix first.
        let (edits, actions): (Vec<_>, Vec<_>) = intents
            .into_iter()
            .partition(|intent| matches!(intent, Intent::BlockChanged(_) | Intent::PageChanged(_)));
        for intent in edits.into_iter().chain(actions) {
            match intent {
                Intent::Open(id, jump) => self.open_page(id, jump, true),
                Intent::OpenInTab(id) => self.open_page_in_tab(id),
                Intent::DockPage(id, target_instance, side) => {
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
                            if let Some(row) = database.rows.iter_mut().find(|row| row.page.id == page) {
                                row.page.icon = icon.clone();
                            }
                        }
                    }
                    self.action(action);
                }
                Intent::BlockChanged(block) => {
                    if block.position == 0 && block.kind != crate::domain::model::BlockKind::Divider
                    {
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
                Intent::Help => self.help_open = true,
                Intent::RetrySave => {
                    self.saves.retry();
                    self.error = None;
                    self.flush(true);
                }
                Intent::Back => self.navigate(-1),
                Intent::Forward => self.navigate(1),
            }
        }
    }

    fn dock_page_at_edge(&mut self, id: PageId, target_instance: u64, side: DockSide) {
        if self.busy || self.closing || !self.ready {
            return;
        }
        if self
            .sidebar
            .page(id)
            .is_none_or(|page| page.trashed_by.is_some())
        {
            return;
        }
        let target = self
            .dock
            .iter_all_tabs()
            .map(|(_, tab)| *tab)
            .find(|tab| tab.instance_id == target_instance);
        let Some(target) = target else { return };
        let Some((surface, node, _)) = self.dock.find_tab(&target) else {
            return;
        };
        let page_tab = PageTab {
            page_id: id,
            instance_id: self.next_tab_instance_id,
        };
        self.next_tab_instance_id = self.next_tab_instance_id.saturating_add(1);
        let split = match side {
            DockSide::Left => Split::Left,
            DockSide::Right => Split::Right,
            DockSide::Top => Split::Above,
            DockSide::Bottom => Split::Below,
        };
        self.dock
            .split((surface, node), split, 0.5, Node::leaf_with(vec![page_tab]));
        self.selected = Some(id);
        self.open_page_content(id, None, true);
    }

    fn shortcuts(&mut self, context: &egui::Context) {
        let mut intents = Vec::new();
        let has_selected_blocks = self
            .editors
            .values()
            .any(|editor| !editor.selected_blocks.is_empty());
        let deletable_page = if self.ready
            && !self.busy
            && !self.closing
            && !self.trash
            && !has_selected_blocks
            && !context.wants_keyboard_input()
        {
            self.selected.filter(|id| {
                self.sidebar
                    .page(*id)
                    .is_some_and(|page| page.trashed_by.is_none())
            })
        } else {
            None
        };
        context.input_mut(|input| {
            if input.consume_key(egui::Modifiers::CTRL, egui::Key::K) {
                intents.push(Intent::Search);
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
            if let Some(page) = deletable_page
                && input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            {
                intents.push(Intent::Action(Action::Trash(page)));
            }
        });
        self.handle_intents(intents, context);
    }

    fn close_safely(&mut self, context: &egui::Context) {
        if context.input(|input| input.viewport().close_requested()) && !self.allow_close {
            if !self.saves.is_idle() || self.busy {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.closing = true;
                self.flush(true);
            } else {
                self.allow_close = true;
            }
        }
        if self.closing {
            if self.saves.failed {
                self.closing = false;
            } else if self.saves.is_idle() && !self.busy {
                self.allow_close = true;
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for NoteonApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_events(context);
        self.shortcuts(context);
        if let Some((query, generation)) = self.search.take_request() {
            self.flush(true);
            self.send(Request::Search { query, generation });
        }
        self.flush(false);
        self.close_safely(context);
        self.show(context);
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, time)| time.elapsed() > Duration::from_secs(8))
        {
            self.notice = None;
        }
        // Low-frequency wakeups drive debounce timers; idle frames do no database work.
        context.request_repaint_after(Duration::from_millis(100));
    }
}
