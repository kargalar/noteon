pub mod database;
pub mod editor;
pub mod page_properties;
pub mod palette;
pub mod search;
pub mod sidebar;
pub mod text;
pub mod theme;

use crate::domain::model::{Block, PageEdit, PageId};
use crate::worker::Action;
use eframe::egui::{self, Color32, FontId, Response, RichText, Ui, Vec2};

pub enum Intent {
    Open(PageId, Option<usize>),
    OpenInTab(PageId),
    DockPage(PageId, u64, DockSide),
    ClosePageTab(PageId),
    Action(Action),
    BlockChanged(Block),
    PageChanged(PageEdit),
    LoadDatabase(PageId),
    LoadBlocks {
        page: PageId,
        generation: u64,
        start: usize,
    },
    Search,
    ToggleTheme,
    SetTheme(bool),
    SetBackground(egui::Color32),
    SetAccent(egui::Color32),
    SetFontScale(f32),
    Trash,
    Settings,
    Help,
    RetrySave,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
}

pub fn quiet_button(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(color))
            .frame(false)
            .min_size(Vec2::new(28.0, 28.0)),
    )
}

pub fn small_label(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(
        RichText::new(text)
            .font(FontId::proportional(11.0))
            .color(color),
    );
}

pub fn icon_picker_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    current_icon: &str,
    font_size: f32,
    color: Option<Color32>,
) -> Option<String> {
    let mut selected_icon = None;
    let button_text = if current_icon.is_empty() {
        "▤"
    } else {
        current_icon
    };
    let mut text = RichText::new(button_text).size(font_size);
    if let Some(color) = color {
        text = text.color(color);
    }
    ui.menu_button(text, |ui| {
        ui.set_min_width(280.0);
        let search_id = egui::Id::new(("icon-picker-search", &id_salt));
        let mut query = ui
            .ctx()
            .data(|data| data.get_temp::<String>(search_id))
            .unwrap_or_default();
        let search = ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text("Search icons")
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .data_mut(|data| data.insert_temp(search_id, query.clone()));

        let query = query.trim().to_lowercase();
        let matching_icons = crate::config::PAGE_ICONS
            .iter()
            .filter(|(name, icon)| {
                query.is_empty() || name.contains(&query) || icon.contains(query.as_str())
            })
            .collect::<Vec<_>>();
        if matching_icons.is_empty() {
            ui.label("No icons found");
        } else {
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(("icon-picker-grid", &id_salt))
                        .num_columns(8)
                        .spacing([3.0, 3.0])
                        .show(ui, |ui| {
                            for (index, (name, icon)) in matching_icons.iter().enumerate() {
                                if ui
                                    .add_sized(
                                        [30.0, 30.0],
                                        egui::Button::new(RichText::new(*icon).size(19.0)),
                                    )
                                    .on_hover_text(*name)
                                    .clicked()
                                {
                                    selected_icon = Some((*icon).to_string());
                                    ui.ctx().data_mut(|data| {
                                        data.insert_temp(search_id, String::new());
                                    });
                                    ui.close_menu();
                                }
                                if (index + 1) % 8 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                });
        }
        if search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            ui.close_menu();
        }
    });
    selected_icon
}

