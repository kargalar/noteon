use eframe::egui::{self, RichText, Stroke, vec2};
use egui_dock::{DockArea, OverlayType, Style as DockStyle, TabViewer};

use crate::config;
use crate::ui::text;
use crate::ui::{DockSide, Intent, palette::Palette, quiet_button, small_label};
use crate::worker::Action;

use super::{NoteonApp, tabs::PageTab};

impl NoteonApp {
    pub(super) fn show(&mut self, context: &egui::Context) {
        let palette = self.current_palette();
        let mut intents = Vec::new();
        show_window_chrome(
            context,
            palette,
            self.selected.and_then(|id| self.sidebar.page(id)),
        );
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(17, 7)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if let Some(error) = &self.error {
                        ui.label(RichText::new(error).small().color(palette.danger));
                        if self.saves.failed && ui.small_button(text::RETRY_SAVE).clicked() {
                            intents.push(Intent::RetrySave);
                        }
                        if ui.small_button("Ã—").clicked() {
                            self.error = None;
                        }
                    } else if let Some((notice, _)) = &self.notice {
                        small_label(ui, notice, palette.accent);
                    } else {
                        let (status, color) = if self.closing {
                            ("Saving changes before closingâ€¦", palette.muted)
                        } else if self.busy {
                            ("Workingâ€¦", palette.muted)
                        } else if self.saves.failed {
                            (
                                "Save failed Â· Your changes are still in memory",
                                palette.danger,
                            )
                        } else if !self.saves.is_idle() {
                            ("Savingâ€¦", palette.muted)
                        } else {
                            (text::LOCAL_SAVED, palette.accent)
                        };
                        small_label(ui, status, color);
                        if self.saves.failed && ui.small_button(text::RETRY_SAVE).clicked() {
                            intents.push(Intent::RetrySave);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let active_count = self
                            .sidebar
                            .pages
                            .iter()
                            .filter(|page| page.trashed_by.is_none())
                            .count();
                        small_label(
                            ui,
                            &format!(
                                "{active_count} pages    Â·    Noteon {}",
                                env!("CARGO_PKG_VERSION")
                            ),
                            palette.muted,
                        );
                    });
                });
            });
        egui::SidePanel::left("sidebar")
            .exact_width(config::SIDEBAR_WIDTH)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(15, 9))
                    .stroke(Stroke::new(1.0, palette.border)),
            )
            .show(context, |ui| {
                ui.add_enabled_ui(self.ready && !self.busy && !self.closing, |ui| {
                    intents.extend(self.sidebar.show(ui, self.selected, self.trash, palette));
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.background).inner_margin(egui::Margin::same(8)))
            .show(context, |ui| {
                if !self.ready {
                    ui.vertical_centered(|ui| {
                        ui.add_space(180.0);
                        if self.error.is_none() {
                            ui.spinner();
                            ui.label("Opening workspaceâ€¦");
                        } else {
                            ui.heading("Could not open workspace");
                            ui.label("Check the error details and make sure the data folder is accessible.");
                            ui.label(self.data_directory.display().to_string());
                        }
                    });
                } else if self.trash {
                    intents.extend(self.show_trash(ui, palette));
                } else {
                    let (visible_pages, visible_databases) = {
                        let mut viewer = PageTabViewer {
                            editors: &mut self.editors,
                            editor_lru: &mut self.editor_lru,
                            sidebar: &self.sidebar,
                            selected: &mut self.selected,
                            pending_documents: &self.pending_documents,
                            databases: &mut self.databases,
                            database_lru: &mut self.database_lru,
                            database_loading: &mut self.database_loading,
                            property_drafts: &mut self.database_property_drafts,
                            title_drafts: &mut self.database_title_drafts,
                            value_drafts: &mut self.database_value_drafts,
                            visible_pages: Default::default(),
                            visible_databases: Default::default(),
                            busy: self.busy || self.closing,
                            palette,
                            intents: &mut intents,
                        };
                        let mut dock_style = DockStyle::from_egui(ui.style());
                        if self.dock.iter_all_tabs().take(2).count() == 1 {
                            dock_style.tab_bar.height = 0.0;
                        }
                        dock_style.overlay.feel.center_drop_coverage = 0.0;
                        dock_style.overlay.feel.window_drop_coverage = 0.0;
                        dock_style.overlay.overlay_type = OverlayType::HighlightedAreas;
                        dock_style.overlay.selection_color = egui::Color32::from_rgba_unmultiplied(
                            palette.accent.r(),
                            palette.accent.g(),
                            palette.accent.b(),
                            190,
                        );
                        DockArea::new(&mut self.dock)
                            .id(egui::Id::new("noteon-page-dock"))
                            .style(dock_style)
                            .show_inside(ui, &mut viewer);
                        (viewer.visible_pages, viewer.visible_databases)
                    };
                    self.evict_inactive_editors(&visible_pages);
                    self.evict_inactive_databases(&visible_databases);
                }
            });
        if self.ready {
            self.persist_dock_layout();
        }
        intents.extend(self.search.show(context, palette));
        intents.extend(self.show_settings(context, palette));
        self.show_help(context, palette);
        self.handle_intents(intents, context);
    }

    fn show_trash(&mut self, ui: &mut egui::Ui, palette: Palette) -> Vec<Intent> {
        let mut intents = Vec::new();
        ui.add_space(35.0);
        ui.heading(RichText::new("Trash").size(31.0));
        ui.add_space(8.0);
        ui.label(RichText::new("Pages and subpages here can be restored.").color(palette.muted));
        ui.add_space(23.0);
        let pages: Vec<_> = self
            .sidebar
            .pages
            .iter()
            .filter(|page| page.trashed_by == Some(page.id))
            .collect();
        if pages.is_empty() {
            ui.label("Your trash is empty.");
        }
        egui::ScrollArea::vertical().show_rows(ui, 56.0, pages.len(), |ui, range| {
            for index in range {
                let page = pages[index];
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(10)
                    .corner_radius(5)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(format!("{}   {}", page.icon, page.display_title()));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(!self.busy, egui::Button::new("Restore"))
                                        .clicked()
                                    {
                                        intents.push(Intent::Action(Action::Restore(page.id)));
                                    }
                                },
                            );
                        });
                    });
            }
        });
        intents
    }

    fn show_settings(&mut self, context: &egui::Context, palette: Palette) -> Vec<Intent> {
        let mut intents = Vec::new();
        let mut settings_open = self.settings_open;
        egui::Window::new("Workspace settings")
            .open(&mut settings_open)
            .resizable(false)
            .collapsible(false)
            .default_width(520.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.settings_theme_tab, "Data").clicked() {
                        self.settings_theme_tab = false;
                    }
                    if ui.selectable_label(self.settings_theme_tab, "Theme").clicked() {
                        self.settings_theme_tab = true;
                    }
                });
                ui.separator();
                ui.add_space(8.0);

                if self.settings_theme_tab {
                    ui.label(RichText::new("Appearance").size(21.0).strong());
                    ui.add_space(8.0);

                    let mut selected_dark = self.dark;
                    let mut theme_changed = false;
                    ui.horizontal(|ui| {
                        ui.label("Theme mode");
                        theme_changed |= ui.radio_value(&mut selected_dark, false, "Light").changed();
                        theme_changed |= ui.radio_value(&mut selected_dark, true, "Dark").changed();
                    });
                    if theme_changed {
                        intents.push(Intent::SetTheme(selected_dark));
                    }

                    let mut background = self.background_color();
                    ui.horizontal(|ui| {
                        ui.label("Background color");
                        if ui.color_edit_button_srgba(&mut background).changed() {
                            intents.push(Intent::SetBackground(background));
                        }
                        ui.label(Palette::to_setting(background));
                    });

                    let mut accent = self.accent_color();
                    ui.horizontal(|ui| {
                        ui.label("Main color");
                        if ui.color_edit_button_srgba(&mut accent).changed() {
                            intents.push(Intent::SetAccent(accent));
                        }
                        ui.label(Palette::to_setting(accent));
                    });

                    let mut font_scale = self.font_scale;
                    ui.horizontal(|ui| {
                        ui.label("Font size");
                        if ui.add(
                            egui::Slider::new(
                                &mut font_scale,
                                config::MIN_FONT_SCALE..=config::MAX_FONT_SCALE,
                            )
                            .step_by(0.05)
                            .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                        ).changed() {
                            intents.push(Intent::SetFontScale(font_scale));
                        }
                    });
                    small_label(
                        ui,
                        "Background color is saved separately for Light and Dark mode.",
                        palette.muted,
                    );
                } else {
                    ui.label(RichText::new("Your data stays on this device.").size(21.0).strong());
                    ui.add_space(9.0);
                    ui.label("No account or internet connection is required. Pages, the search index, and settings are stored in a local SQLite database.");
                    ui.add_space(12.0);
                    small_label(ui, "DATA FOLDER", palette.muted);
                    let path = self.data_directory.display().to_string();
                    ui.label(RichText::new(&path).monospace().small());
                    if quiet_button(ui, "Copy folder path", palette.accent).clicked() {
                        context.copy_text(path);
                    }
                    ui.separator();
                    ui.add_enabled_ui(self.ready && !self.busy, |ui| {
                        if ui.button("Back up workspace…").clicked() {
                            intents.push(Intent::Action(Action::Backup));
                        }
                        if ui.button(text::IMPORT_MARKDOWN).clicked() {
                            intents.push(Intent::Action(Action::Import));
                        }
                    });
                    ui.add_space(8.0);
                    small_label(
                        ui,
                        "To restore a backup, close the app and copy the backup into the data folder as noteon.sqlite. Move the current database and its -wal / -shm files to another folder first.",
                        palette.muted,
                    );
                    ui.separator();
                    let editor = self.selected.and_then(|id| self.editors.get(&id));
                    let rendered_blocks = editor.map_or(0, |editor| editor.rendered_blocks);
                    let cached_blocks = editor.map_or(0, |editor| editor.cache.blocks.len());
                    small_label(
                        ui,
                        &format!(
                            "Visible blocks: {rendered_blocks}   ·   Cached blocks: {cached_blocks}   ·   Cache limit: {} blocks",
                            config::BLOCK_BATCH_SIZE * config::CACHED_BATCHES
                        ),
                        palette.muted,
                    );
                }
            });
        self.settings_open = settings_open;
        intents
    }
    fn show_help(&mut self, context: &egui::Context, palette: Palette) {
        egui::Window::new("Quick guide").open(&mut self.help_open).collapsible(false).resizable(false).default_width(480.0).show(context, |ui| {
            ui.add_space(8.0);
            egui::Grid::new("shortcuts").spacing(vec2(30.0, 12.0)).show(ui, |ui| {
                for (key, description) in [("Ctrl + N", "New page"),("Ctrl + K", "Search all notes"),("Ctrl + S", "Save now / retry"),("Alt + â† / â†’", "Previous / next page"),("Enter", "Split at cursor and add a block"),("Shift + Enter", "New line in the same block"),("Right-click the block menu", "Change type, move, or delete")] {
                    ui.label(RichText::new(key).monospace().color(palette.accent)); ui.label(description); ui.end_row();
                }
            });
            ui.add_space(13.0);
            ui.separator();
            ui.label("Type /h1, /h2, /h3, /todo, /bullet, /number, /quote, /callout, /code, or /divider in an empty block, then press Enter.");
            ui.add_space(8.0);
            small_label(ui, "# + Space, - + Space, [] + Space, and > + Space also change block type. Enter inserts a line break in code blocks.", palette.muted);
        });
    }
}

fn show_window_chrome(
    context: &egui::Context,
    palette: Palette,
    active_page: Option<&crate::domain::model::Page>,
) {
    const HEIGHT: f32 = 38.0;
    const CONTROL_WIDTH: f32 = 46.0;

    egui::TopBottomPanel::top("window-chrome")
        .exact_height(HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(palette.sidebar)
                .stroke(Stroke::new(1.0, palette.border)),
        )
        .show(context, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let (logo_rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::hover());
                ui.painter().rect_filled(logo_rect, 6.0, palette.accent);
                ui.painter().text(
                    logo_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "n",
                    egui::FontId::proportional(18.0),
                    palette.background,
                );
                ui.add_space(8.0);
                ui.label(RichText::new(config::APP_NAME).strong().color(palette.text));

                let drag_width = (ui.available_width() - CONTROL_WIDTH * 3.0 - 12.0).max(0.0);
                let (drag_rect, drag_response) =
                    ui.allocate_exact_size(vec2(drag_width, HEIGHT), egui::Sense::click_and_drag());
                if drag_response.drag_started_by(egui::PointerButton::Primary) {
                    context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_response.double_clicked() {
                    toggle_maximized(context);
                }
                if let Some(page) = active_page {
                    ui.painter().text(
                        drag_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{}  {}", page.icon, page.display_title()),
                        egui::FontId::proportional(12.0),
                        palette.muted,
                    );
                }

                if chrome_button(ui, "×", CONTROL_WIDTH, HEIGHT, palette, true).clicked() {
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let maximized = context.input(|input| input.viewport().maximized.unwrap_or(false));
                if chrome_button(
                    ui,
                    if maximized { "❐" } else { "□" },
                    CONTROL_WIDTH,
                    HEIGHT,
                    palette,
                    false,
                )
                .clicked()
                {
                    toggle_maximized(context);
                }
                if chrome_button(ui, "−", CONTROL_WIDTH, HEIGHT, palette, false).clicked() {
                    context.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            });
        });
}

fn chrome_button(
    ui: &mut egui::Ui,
    icon: &str,
    width: f32,
    height: f32,
    palette: Palette,
    close: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            0.0,
            if close { palette.danger } else { palette.hover },
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(if close { 22.0 } else { 16.0 }),
        if close && response.hovered() {
            palette.background
        } else {
            palette.text
        },
    );
    response
}

fn toggle_maximized(context: &egui::Context) {
    let maximized = context.input(|input| input.viewport().maximized.unwrap_or(false));
    context.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

struct PageTabViewer<'a> {
    editors:
        &'a mut std::collections::HashMap<crate::domain::model::PageId, crate::ui::editor::Editor>,
    editor_lru: &'a mut std::collections::VecDeque<crate::domain::model::PageId>,
    sidebar: &'a crate::ui::sidebar::Sidebar,
    selected: &'a mut Option<crate::domain::model::PageId>,
    pending_documents:
        &'a std::collections::HashMap<u64, (crate::domain::model::PageId, Option<usize>)>,
    databases: &'a mut std::collections::HashMap<
        crate::domain::model::PageId,
        crate::domain::model::DatabaseData,
    >,
    database_lru: &'a mut std::collections::VecDeque<crate::domain::model::PageId>,
    database_loading: &'a mut std::collections::HashSet<crate::domain::model::PageId>,
    property_drafts: &'a mut std::collections::HashMap<i64, (String, String)>,
    title_drafts: &'a mut std::collections::HashMap<crate::domain::model::PageId, String>,
    value_drafts: &'a mut std::collections::HashMap<(crate::domain::model::PageId, i64), String>,
    visible_pages: std::collections::HashSet<crate::domain::model::PageId>,
    visible_databases: std::collections::HashSet<crate::domain::model::PageId>,
    busy: bool,
    palette: Palette,
    intents: &'a mut Vec<Intent>,
}

impl TabViewer for PageTabViewer<'_> {
    type Tab = PageTab;

    fn id(&mut self, page: &mut Self::Tab) -> egui::Id {
        egui::Id::new(("noteon-page-tab", page.instance_id))
    }

    fn title(&mut self, page: &mut Self::Tab) -> egui::WidgetText {
        self.sidebar.page(page.page_id).map_or_else(
            || "Page".into(),
            |page| format!("{} {}", page.icon, page.display_title()).into(),
        )
    }

    fn ui(&mut self, ui: &mut egui::Ui, page: &mut Self::Tab) {
        let page_id = page.page_id;
        let target_instance = page.instance_id;
        let panel_rect = ui.max_rect();
        let drop_response = ui.interact(
            panel_rect,
            ui.id().with(("sidebar-page-drop", page.instance_id)),
            egui::Sense::hover(),
        );
        let hovered_page = drop_response.dnd_hover_payload::<crate::domain::model::PageId>();
        let pointer = ui.ctx().pointer_interact_pos();
        let drop_side = pointer.and_then(|position| dock_drop_side(panel_rect, position));
        if hovered_page.is_some()
            && let (Some(_dragged_page), Some(side)) = (hovered_page, drop_side)
        {
            let preview = dock_preview_rect(panel_rect, side);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                ui.id().with(("sidebar-page-drop-preview", target_instance)),
            ));
            painter.with_clip_rect(panel_rect).rect_filled(
                preview,
                6.0,
                egui::Color32::from_rgba_unmultiplied(
                    self.palette.accent.r(),
                    self.palette.accent.g(),
                    self.palette.accent.b(),
                    110,
                ),
            );
            painter.with_clip_rect(panel_rect).rect_stroke(
                preview,
                6.0,
                Stroke::new(2.0, self.palette.accent),
                egui::StrokeKind::Inside,
            );
            if let Some(released_page) =
                drop_response.dnd_release_payload::<crate::domain::model::PageId>()
            {
                self.intents
                    .push(Intent::DockPage(*released_page, target_instance, side));
            }
        }
        *self.selected = Some(page_id);
        self.visible_pages.insert(page_id);
        self.editor_lru.retain(|candidate| *candidate != page_id);
        self.editor_lru.push_back(page_id);
        let parent_title = self
            .sidebar
            .page(page_id)
            .and_then(|page| page.parent_id)
            .and_then(|parent| self.sidebar.page(parent))
            .map(|parent| parent.display_title().to_owned());

        let record_database_id = self
            .sidebar
            .page(page_id)
            .and_then(|page| page.parent_id)
            .filter(|parent| {
                self.sidebar
                    .page(*parent)
                    .is_some_and(|page| page.is_database)
            });
        if let Some(database_id) = record_database_id {
            self.visible_databases.insert(database_id);
            self.database_lru
                .retain(|candidate| *candidate != database_id);
            self.database_lru.push_back(database_id);
        }
        if let Some(database_id) = record_database_id
            && !self.databases.contains_key(&database_id)
            && !self.database_loading.contains(&database_id)
        {
            self.intents.push(Intent::LoadDatabase(database_id));
        }

        if self
            .sidebar
            .page(page_id)
            .is_some_and(|page| page.is_database)
        {
            self.visible_databases.insert(page_id);
            self.database_lru.retain(|candidate| *candidate != page_id);
            self.database_lru.push_back(page_id);
            if let Some(database) = self.databases.get_mut(&page_id) {
                ui.add_enabled_ui(!self.busy, |ui| {
                    crate::ui::database::show(
                        ui,
                        database,
                        self.busy,
                        self.palette,
                        self.intents,
                        &mut crate::ui::database::Drafts {
                            properties: self.property_drafts,
                            titles: self.title_drafts,
                            values: self.value_drafts,
                        },
                    );
                });
            } else if self.database_loading.contains(&page_id) {
                ui.vertical_centered(|ui| {
                    ui.add_space(130.0);
                    ui.spinner();
                    ui.label("Loading databaseâ€¦");
                });
            } else {
                self.intents.push(Intent::LoadDatabase(page_id));
            }
            return;
        }

        if let Some(editor) = self.editors.get_mut(&page_id)
            && editor
                .document
                .as_ref()
                .is_some_and(|document| document.page.id == page_id)
        {
            ui.add_enabled_ui(!self.busy, |ui| {
                self.intents
                    .extend(editor.show_with_database_context_in_tab(
                        ui,
                        self.palette,
                        parent_title.as_deref(),
                        target_instance,
                        &mut crate::ui::editor::DatabaseContext {
                            record_database_id,
                            databases: self.databases,
                            loading: self.database_loading,
                            property_drafts: self.property_drafts,
                            title_drafts: self.title_drafts,
                            value_drafts: self.value_drafts,
                            visible: &mut self.visible_databases,
                        },
                    ));
            });
        } else if self
            .pending_documents
            .values()
            .any(|(pending_page, _)| *pending_page == page_id)
        {
            ui.vertical_centered(|ui| {
                ui.add_space(130.0);
                ui.spinner();
                ui.label("Opening pageâ€¦");
            });
        } else {
            self.intents.push(Intent::Open(page_id, None));
        }
    }

    fn closeable(&mut self, _page: &mut Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, page: &mut Self::Tab) -> bool {
        self.intents.push(Intent::ClosePageTab(page.page_id));
        true
    }

    fn scroll_bars(&self, _page: &Self::Tab) -> [bool; 2] {
        [false, false]
    }
}

fn dock_drop_side(rect: egui::Rect, pointer: egui::Pos2) -> Option<DockSide> {
    if !rect.contains(pointer) {
        return None;
    }
    let horizontal = (pointer.x - rect.left()) / rect.width().max(1.0);
    let vertical = (pointer.y - rect.top()) / rect.height().max(1.0);
    let candidates = [
        (horizontal, DockSide::Left),
        (1.0 - horizontal, DockSide::Right),
        (vertical, DockSide::Top),
        (1.0 - vertical, DockSide::Bottom),
    ];
    candidates
        .into_iter()
        .filter(|(distance, _)| *distance <= 0.24)
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, side)| side)
}

fn dock_preview_rect(rect: egui::Rect, side: DockSide) -> egui::Rect {
    match side {
        DockSide::Left => {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y))
        }
        DockSide::Right => {
            egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max)
        }
        DockSide::Top => {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.center().y))
        }
        DockSide::Bottom => {
            egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.center().y), rect.max)
        }
    }
}
