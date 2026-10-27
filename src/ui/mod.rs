use crate::ui::menus::PersistentMenuUi;
pub mod assistant_panel;
mod block_clipboard;
pub mod database;
pub mod editor;
pub mod menus;
pub mod page_properties;
pub mod page_transfer;
pub mod palette;
pub mod search;
mod selection;
pub mod sidebar;
pub mod text;
pub mod theme;

use crate::domain::model::{Block, PageEdit, PageId};
use crate::worker::Action;
use eframe::egui::{self, Color32, FontId, Response, RichText, Ui, Vec2};

pub enum Intent {
    Open(PageId, Option<usize>),
    OpenTitle(String),
    OpenInTab(PageId),
    OpenInDialog(PageId),
    ClosePageDialog,
    OpenUrl(String),
    OpenFileExplorer(String),
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
    ToggleAssistant,
    FindInPage,
    ToggleSidebarPin,
    ToggleTheme,
    SetTheme(bool),
    SetBackground(egui::Color32),
    SetAccent(egui::Color32),
    SetFontScale(f32),
    Trash,
    Settings,
    OpenSettingsTheme(ThemeSection),
    SetCustomThemeColor {
        key: CustomColorKey,
        value: Option<egui::Color32>,
    },
    ResetCustomThemeSection(ThemeSection),
    ResetAllCustomTheme,
    Help,
    RetrySave,
    Back,
    Forward,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeSection {
    General,
    Sidebar,
    Editor,
    Header,
    Gallery,
    Board,
    Table,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomColorKey {
    Sidebar,
    Text,
    Muted,
    Border,
    AccentSoft,
    Hover,
    Code,
    GalleryCard,
    GalleryBorder,
    BoardCard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    pub id: PageId,
    pub title: String,
    pub icon: String,
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

use crate::ui::palette::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconTab {
    Color,
    Monochrome,
}

pub fn show_icon_picker_content(
    ui: &mut Ui,
    id_salt: &impl std::hash::Hash,
    current_icon: &str,
    selected_icon: &mut Option<String>,
) {
    ui.set_min_width(280.0);

    let tab_id = egui::Id::new(("icon-picker-tab", id_salt));
    let mut active_tab = ui
        .ctx()
        .data(|data| data.get_temp::<IconTab>(tab_id))
        .unwrap_or(IconTab::Color);

    ui.horizontal(|ui| {
        if ui
            .selectable_label(active_tab == IconTab::Color, "Renkli")
            .clicked()
        {
            active_tab = IconTab::Color;
            ui.ctx()
                .data_mut(|data| data.insert_temp(tab_id, active_tab));
        }
        if ui
            .selectable_label(active_tab == IconTab::Monochrome, "Renksiz")
            .clicked()
        {
            active_tab = IconTab::Monochrome;
            ui.ctx()
                .data_mut(|data| data.insert_temp(tab_id, active_tab));
        }
    });

    ui.add_space(4.0);

    let search_id = egui::Id::new(("icon-picker-search", id_salt));
    let mut query = ui
        .ctx()
        .data(|data| data.get_temp::<String>(search_id))
        .unwrap_or_default();
    let search = ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text("İkon ara…")
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(search_id, query.clone()));

    let query_clean = query.trim().to_lowercase();
    let icon_list = match active_tab {
        IconTab::Color => crate::config::COLOR_ICONS,
        IconTab::Monochrome => crate::config::MONOCHROME_ICONS,
    };

    let matching_icons = icon_list
        .iter()
        .filter(|(name, icon)| {
            query_clean.is_empty()
                || name.contains(&query_clean)
                || icon.contains(query_clean.as_str())
        })
        .collect::<Vec<_>>();

    if matching_icons.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("İkon bulunamadı").color(ui.visuals().weak_text_color()));
    } else {
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new(("icon-picker-grid", id_salt))
                    .num_columns(8)
                    .spacing([3.0, 3.0])
                    .show(ui, |ui| {
                        for (index, (name, icon)) in matching_icons.iter().enumerate() {
                            let button_resp = if let Some(source) = emoji_image_source(icon) {
                                let img = egui::Image::new(source)
                                    .fit_to_exact_size(egui::vec2(22.0, 22.0));
                                ui.add_sized([30.0, 30.0], egui::Button::image(img))
                            } else {
                                let icon_font_size = match active_tab {
                                    IconTab::Color => 19.0,
                                    IconTab::Monochrome => 17.0,
                                };
                                ui.add_sized(
                                    [30.0, 30.0],
                                    egui::Button::new(RichText::new(*icon).size(icon_font_size)),
                                )
                            };
                            if button_resp.on_hover_text(*name).clicked() {
                                *selected_icon = Some((*icon).to_string());
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

    if !current_icon.is_empty() {
        ui.add_space(4.0);
        ui.separator();
        if ui
            .button(
                RichText::new("🗑  İkonu kaldır")
                    .size(12.0)
                    .color(ui.visuals().error_fg_color),
            )
            .clicked()
        {
            *selected_icon = Some(String::new());
            ui.ctx().data_mut(|data| {
                data.insert_temp(search_id, String::new());
            });
            ui.close_menu();
        }
    }

    if search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        ui.close_menu();
    }
}

pub fn emoji_image_source(emoji: &str) -> Option<egui::ImageSource<'static>> {
    let clean = emoji.trim();
    if clean.is_empty() {
        return None;
    }
    if let Some(svg) = twemoji_assets::svg::SvgTwemojiAsset::from_emoji(clean) {
        return Some(egui::ImageSource::Bytes {
            uri: format!("bytes://twemoji/{clean}.svg").into(),
            bytes: egui::load::Bytes::Static(svg.as_bytes()),
        });
    }
    let without_vs = clean.replace('\u{fe0f}', "");
    if let Some(svg) = twemoji_assets::svg::SvgTwemojiAsset::from_emoji(&without_vs) {
        return Some(egui::ImageSource::Bytes {
            uri: format!("bytes://twemoji/{without_vs}.svg").into(),
            bytes: egui::load::Bytes::Static(svg.as_bytes()),
        });
    }
    let with_vs = format!("{clean}\u{fe0f}");
    if let Some(svg) = twemoji_assets::svg::SvgTwemojiAsset::from_emoji(&with_vs) {
        return Some(egui::ImageSource::Bytes {
            uri: format!("bytes://twemoji/{clean}_vs.svg").into(),
            bytes: egui::load::Bytes::Static(svg.as_bytes()),
        });
    }
    None
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
    if let Some(source) = emoji_image_source(button_text) {
        let img = egui::Image::new(source).fit_to_exact_size(egui::vec2(font_size, font_size));
        ui.persistent_menu_image_button(img, |ui| {
            show_icon_picker_content(ui, &id_salt, current_icon, &mut selected_icon);
        });
    } else {
        let mut text = RichText::new(button_text).size(font_size);
        if !crate::config::is_color_icon(button_text) {
            if let Some(color) = color {
                text = text.color(color);
            }
        }
        ui.persistent_menu_button(text, |ui| {
            show_icon_picker_content(ui, &id_salt, current_icon, &mut selected_icon);
        });
    }
    selected_icon
}

pub fn add_icon_picker_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    palette: Palette,
) -> (Option<String>, bool, bool) {
    let mut selected_icon = None;
    let text = RichText::new("☺  İkon ekle")
        .size(12.5)
        .color(palette.muted);

    let menu_resp = ui.persistent_menu_button(text, |ui| {
        show_icon_picker_content(ui, &id_salt, "", &mut selected_icon);
    });
    let is_open = menu_resp.inner.is_some();
    let hovered = menu_resp.response.hovered();
    (selected_icon, is_open, hovered)
}
