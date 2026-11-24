mod assistant;
mod database_cache;
mod events;
mod intents;
mod navigation;
mod saves;
mod tabs;
mod views;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};
use egui_dock::DockState;

use crate::config;
use crate::domain::model::{DatabaseData, PageId};
use crate::ui::{editor::Editor, palette::Palette, search::Search, sidebar::Sidebar, theme};
use crate::worker::{Request, Worker};

use saves::Saves;
use tabs::PageTab;

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
    assistant: crate::assistant::Assistant,
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
    sidebar_pinned: bool,
    trash: bool,
    trashed_pages_history: Vec<Vec<PageId>>,
    trashed_pages_redo: Vec<Vec<PageId>>,
    settings_open: bool,
    settings_theme_tab: bool,
    settings_theme_highlight: Option<(crate::ui::ThemeSection, Instant)>,
    custom_theme: crate::ui::palette::ThemeConfig,
    help_open: bool,
    page_dialog: Option<PageId>,
    pending_dialog: Option<PageId>,
    error: Option<String>,
    notice: Option<(String, Instant)>,
    closing: bool,
    allow_close: bool,
}

impl NoteonApp {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&creation.egui_ctx);
        theme::configure(&creation.egui_ctx);
        let directory = config::data_directory();
        let data_directory = directory.as_ref().cloned().unwrap_or_default();
        let assistant = crate::assistant::Assistant::load(&data_directory);
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
            assistant,
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
            dark: true,
            light_background: Palette::new(false).background,
            dark_background: Palette::new(true).background,
            accent_color: None,
            font_scale: config::DEFAULT_FONT_SCALE,
            sidebar_pinned: true,
            trash: false,
            trashed_pages_history: Vec::new(),
            trashed_pages_redo: Vec::new(),
            settings_open: false,
            settings_theme_tab: false,
            settings_theme_highlight: None,
            custom_theme: crate::ui::palette::ThemeConfig::default(),
            help_open: false,
            page_dialog: None,
            pending_dialog: None,
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

    fn active_custom_theme(&self) -> &crate::ui::palette::CustomTheme {
        if self.dark {
            &self.custom_theme.dark
        } else {
            &self.custom_theme.light
        }
    }

    fn active_custom_theme_mut(&mut self) -> &mut crate::ui::palette::CustomTheme {
        if self.dark {
            &mut self.custom_theme.dark
        } else {
            &mut self.custom_theme.light
        }
    }

    fn current_palette(&self) -> Palette {
        Palette::new(self.dark).customized(
            self.background_color(),
            self.accent_color(),
            self.active_custom_theme(),
        )
    }

    fn apply_theme(&self, context: &egui::Context) {
        theme::apply(
            context,
            self.dark,
            Some(self.background_color()),
            Some(self.accent_color()),
            self.font_scale,
            self.active_custom_theme(),
        );
    }

    fn flush(&mut self, force: bool) {
        if let Some((id, batch)) = self.saves.next_batch(force)
            && !self.send(Request::Save { id, batch })
        {
            self.saves.fail(id);
        }
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

    fn request_searches(&mut self) {
        if let Some((query, generation)) = self.search.take_request() {
            self.flush(true);
            if !self.send(Request::Search { query, generation }) {
                self.search.fail();
            }
        }
        let page_searches: Vec<_> = self
            .editors
            .iter_mut()
            .filter_map(|(&page, editor)| {
                let (revision, query) = editor.find.take_request()?;
                Some((page, editor.generation, revision, query))
            })
            .collect();
        if !page_searches.is_empty() {
            self.flush(true);
        }
        for (page, generation, revision, query) in page_searches {
            if (self.saves.failed
                || !self.send(Request::FindInPage {
                    page,
                    generation,
                    revision,
                    query,
                }))
                && let Some(editor) = self.editors.get_mut(&page)
            {
                editor.find.fail(revision);
            }
        }
    }
}

impl eframe::App for NoteonApp {
    #[cfg(windows)]
    fn raw_input_hook(&mut self, context: &egui::Context, input: &mut egui::RawInput) {
        use mouse_position::mouse_position::Mouse;

        if input.hovered_files.is_empty() && input.dropped_files.is_empty() {
            return;
        }
        // The Windows file-drop event carries a path but no cursor coordinates.
        let Mouse::Position { x, y } = Mouse::get_mouse_position() else {
            return;
        };
        let viewport = input.viewport();
        if let Some(window) = viewport.inner_rect {
            let scale = viewport.native_pixels_per_point.map_or_else(
                || context.pixels_per_point(),
                |native| native * context.zoom_factor(),
            );
            let position = egui::pos2(x as f32, y as f32) / scale;
            input
                .events
                .push(egui::Event::PointerMoved(position - window.min.to_vec2()));
        }
        if !input.hovered_files.is_empty() {
            context.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }

    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        theme::refresh_motion(context);
        self.receive_events(context);
        self.poll_assistant();
        self.shortcuts(context);
        self.request_searches();
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
