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
