use std::collections::HashMap;

use eframe::egui::{self, text::{CCursor, CCursorRange}, Color32, FontId, RichText, Ui};

use crate::domain::model::{
    DatabaseData, DatabaseProperty, DatabaseRow, DatabaseView, PROPERTY_TYPES, PropertyOption,
};
use crate::ui::page_properties::{
    option_base_color, option_pill_colors, resolve_option_color, show_color_palette_grid,
};
use crate::worker::Action;

use super::{Intent, ThemeSection, palette::Palette};

const TABLE_NAME_WIDTH: f32 = 250.0;
const TABLE_CREATED_WIDTH: f32 = 135.0;
const TABLE_PROPERTY_WIDTH: f32 = 180.0;
const TABLE_ADD_PROP_WIDTH: f32 = 42.0;
const TABLE_HEADER_HEIGHT: f32 = 34.0;
const TABLE_ROW_HEIGHT: f32 = 36.0;
const DEFAULT_TABLE_MAX_ROWS_HEIGHT: f32 = 648.0;
const DEFAULT_GALLERY_MAX_HEIGHT: f32 = 760.0;
const DEFAULT_BOARD_MAX_CARDS_HEIGHT: f32 = 648.0;

#[derive(Clone, Copy)]
struct BoardDragPayload {
    database_id: i64,
    record_id: i64,
}

fn resolve_vertical_height(
    ui: &Ui,
    data: &DatabaseData,
    natural_h: f32,
    default_max_h: f32,
    min_h: f32,
) -> (f32, bool) {
    let default_h = natural_h.max(min_h).min(default_max_h);
    let stored_h = data.view_settings.table_height.unwrap_or(default_h);
    let live_v_id = egui::Id::new(("table-v-resize-live", data.page_id));
    let live_h = ui.ctx().data(|d| d.get_temp::<f32>(live_v_id));
    let has_custom = data.view_settings.table_height.is_some() || live_h.is_some();
    let active_h = live_h.unwrap_or(stored_h).clamp(min_h, 2400.0);
    (active_h, has_custom)
}

fn show_vertical_resize_handle(
    ui: &mut Ui,
    data: &mut DatabaseData,
    width: f32,
    active_h: f32,
    min_h: f32,
    palette: Palette,
) -> bool {
    let live_v_id = egui::Id::new(("table-v-resize-live", data.page_id));
    let handle_w = width.max(160.0);
    let (v_handle_rect, v_handle_resp) = ui.allocate_exact_size(
        egui::vec2(handle_w, 12.0),
        egui::Sense::click_and_drag(),
    );
    let v_handle_resp = v_handle_resp
        .on_hover_cursor(egui::CursorIcon::ResizeVertical)
        .on_hover_text("Drag to resize height · Double-click to auto-fit");

    let grip_color = if v_handle_resp.hovered() || v_handle_resp.dragged() {
        palette.accent
    } else {
        palette.border.linear_multiply(0.8)
    };
    if v_handle_resp.hovered() || v_handle_resp.dragged() {
        ui.painter().line_segment(
            [
                egui::pos2(v_handle_rect.left(), v_handle_rect.center().y),
                egui::pos2(v_handle_rect.right(), v_handle_rect.center().y),
            ],
            egui::Stroke::new(2.0, palette.accent),
        );
    }
    ui.painter().rect_filled(
        egui::Rect::from_center_size(v_handle_rect.center(), egui::vec2(40.0, 4.0)),
        2.0,
        grip_color,
    );

    let mut settings_changed = false;
    if v_handle_resp.double_clicked() {
        ui.ctx().data_mut(|d| d.remove::<f32>(live_v_id));
        data.view_settings.table_height = None;
        settings_changed = true;
    } else {
        if v_handle_resp.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(live_v_id, active_h));
        }
        if v_handle_resp.dragged() {
            let prev = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(live_v_id))
                .unwrap_or(active_h);
            let next = (prev + v_handle_resp.drag_delta().y).clamp(min_h, 2400.0);
            ui.ctx().data_mut(|d| d.insert_temp(live_v_id, next));
        }
        if v_handle_resp.drag_stopped() {
            let final_h = ui.ctx().data(|d| d.get_temp::<f32>(live_v_id));
            ui.ctx().data_mut(|d| d.remove::<f32>(live_v_id));
            if let Some(final_h) = final_h {
                data.view_settings.table_height = Some(final_h);
                settings_changed = true;
            }
        }
    }
    settings_changed
}

fn gallery_card_metrics(data: &DatabaseData, gallery: bool) -> (f32, f32) {
    let settings = &data.view_settings;
    let show_cover = gallery && settings.gallery_show_cover;
    let show_title = !gallery || settings.gallery_show_title;
    let show_created_at = if gallery {
        settings.gallery_show_created_at
    } else {
        true
    };
    let show_description = gallery && settings.gallery_show_description;
    let visible_properties = if gallery {
        settings.gallery_visible_properties.as_deref()
    } else {
        None
    };
    let property_count =
        visible_properties.map_or_else(|| data.properties.len().min(2), |visible| visible.len());
    let cover_h: f32 = if show_cover { 130.0 } else { 0.0 };
    let title_h: f32 = if show_title { 28.0 } else { 0.0 };
    let created_h: f32 = if show_created_at { 20.0 } else { 0.0 };
    let desc_h: f32 = if show_description { 38.0 } else { 0.0 };
    let prop_h: f32 = if property_count > 0 { 24.0 } else { 0.0 };
    let card_height: f32 = (cover_h + title_h + created_h + desc_h + prop_h + 24.0).max(96.0);
    let row_gap = 12.0;
    (card_height, row_gap)
}

fn board_max_column_cards_height(data: &DatabaseData, status_id: i64) -> f32 {
    let mut heights = HashMap::<String, f32>::new();
    let mut counts = HashMap::<String, usize>::new();
    let board_visible_properties = data.view_settings.board_visible_properties.as_deref();
    let has_other_props = data
        .properties
        .iter()
        .filter(|p| p.id != status_id)
        .any(|p| {
            board_visible_properties.map_or_else(
                || {
                    data.properties
                        .iter()
                        .filter(|c| c.id != status_id)
                        .take(2)
                        .any(|v| v.id == p.id)
                },
                |visible| visible.contains(&p.id),
            )
        });
    for row in &data.rows {
        let val = row.values.get(&status_id).map(String::as_str).unwrap_or("");
        let group = if val.is_empty() { "No selection" } else { val };
        let show_desc =
            data.view_settings.board_show_description && !row.preview.trim().is_empty();
        let card_h = 36.0
            + if has_other_props { 20.0 } else { 0.0 }
            + if show_desc { 22.0 } else { 0.0 };
        *heights.entry(group.to_owned()).or_insert(0.0) += card_h;
        *counts.entry(group.to_owned()).or_insert(0) += 1;
    }
    heights
        .into_iter()
        .map(|(g, h)| {
            let c = counts.get(&g).copied().unwrap_or(0);
            h + c.saturating_sub(1) as f32 * 8.0 + 8.0
        })
        .fold(96.0_f32, f32::max)
}

pub fn desired_embedded_height(ui: &Ui, data: &DatabaseData) -> f32 {
    let chrome_height = if data.title.is_empty() { 64.0 } else { 90.0 };
    match data.view {
        DatabaseView::Table => {
            let natural_rows_h = (data.rows.len() as f32 * TABLE_ROW_HEIGHT).max(TABLE_ROW_HEIGHT);
            let (rows_h, _) = resolve_vertical_height(
                ui,
                data,
                natural_rows_h,
                DEFAULT_TABLE_MAX_ROWS_HEIGHT,
                TABLE_ROW_HEIGHT,
            );
            let table_total = TABLE_HEADER_HEIGHT + rows_h + TABLE_ROW_HEIGHT + 24.0 + 24.0;
            (chrome_height + table_total).clamp(180.0, 2600.0)
        }
        DatabaseView::Gallery => {
            let cols = usize::from(data.gallery_columns).max(1);
            let row_count = data.rows.len().div_ceil(cols);
            let (card_h, row_gap) = gallery_card_metrics(data, true);
            let natural_h = if row_count == 0 {
                card_h
            } else {
                row_count as f32 * card_h + row_count.saturating_sub(1) as f32 * row_gap
            };
            let (active_h, _) = resolve_vertical_height(
                ui,
                data,
                natural_h,
                DEFAULT_GALLERY_MAX_HEIGHT,
                card_h,
            );
            let gallery_total = active_h + 40.0 + 24.0;
            (chrome_height + gallery_total).clamp(200.0, 2600.0)
        }
        DatabaseView::Board => {
            let status_id = data
                .properties
                .iter()
                .find(|p| p.kind == "status" || p.kind == "select")
                .map(|p| p.id);
            if let Some(status_id) = status_id {
                let natural_cards_h = board_max_column_cards_height(data, status_id);
                let (active_cards_h, _) = resolve_vertical_height(
                    ui,
                    data,
                    natural_cards_h,
                    DEFAULT_BOARD_MAX_CARDS_HEIGHT,
                    96.0,
                );
                let board_total = active_cards_h + 104.0 + 24.0;
                (chrome_height + board_total).clamp(240.0, 2600.0)
            } else {
                let (card_h, row_gap) = gallery_card_metrics(data, false);
                let row_count = data.rows.len();
                let natural_h = if row_count == 0 {
                    card_h
                } else {
                    row_count as f32 * card_h + row_count.saturating_sub(1) as f32 * row_gap
                };
                let (active_h, _) = resolve_vertical_height(
                    ui,
                    data,
                    natural_h,
                    DEFAULT_GALLERY_MAX_HEIGHT,
                    card_h,
                );
                (chrome_height + active_h + 88.0).clamp(220.0, 2600.0)
            }
        }
    }
}

fn property_icon(kind: &str) -> &'static str {
    match kind {
        "text" => "Aa",
        "number" => "#",
        "date" => "📅",
        "status" => "○",
        "select" => "🏷",
        "multi-select" => "☰",
        "checkbox" => "☑",
        _ => "•",
    }
}

pub struct Drafts<'a> {
    pub properties: &'a mut std::collections::HashMap<i64, (String, String)>,
    pub titles: &'a mut std::collections::HashMap<i64, String>,
    pub values: &'a mut std::collections::HashMap<(i64, i64), String>,
}

pub fn show(
    ui: &mut Ui,
    data: &mut DatabaseData,
    page_icon: Option<&str>,
    breadcrumbs: &[crate::ui::BreadcrumbItem],
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    egui::ScrollArea::vertical()
        .id_salt(("database-full-page-scroll", data.page_id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_inner(
                ui,
                data,
                page_icon,
                breadcrumbs,
                busy,
                palette,
                intents,
                drafts,
                true,
            );
        });
}

pub fn show_embedded(
    ui: &mut Ui,
    data: &mut DatabaseData,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    show_inner(ui, data, None, &[], false, palette, intents, drafts, false);
}

fn show_inner(
    ui: &mut Ui,
    data: &mut DatabaseData,
    page_icon: Option<&str>,
    breadcrumbs: &[crate::ui::BreadcrumbItem],
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
    full_page: bool,
) {
    ui.vertical(|ui| {
        if full_page {
            ui.add_space(20.0);
            if !breadcrumbs.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(44.0);
                    for item in breadcrumbs {
                        let label = if !item.icon.is_empty() {
                            format!("{}  {}", item.icon, item.title)
                        } else {
                            item.title.clone()
                        };
                        let resp = ui.add(
                            egui::Button::new(
                                RichText::new(label)
                                    .font(FontId::proportional(12.0))
                                    .color(palette.muted),
                            )
                            .frame(false),
                        );
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            intents.push(Intent::Open(item.id, None));
                        } else if resp.middle_clicked() {
                            intents.push(Intent::OpenInTab(item.id));
                        }
                        ui.label(
                            RichText::new("/")
                                .font(FontId::proportional(12.0))
                                .color(palette.muted),
                        );
                    }
                    ui.label(
                        RichText::new(&data.title)
                            .font(FontId::proportional(12.0))
                            .color(palette.muted),
                    );
                });
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.add_space(44.0);
                let current_icon = if !data.icon.is_empty() {
                    &data.icon
                } else if let Some(icon) = page_icon {
                    icon
                } else {
                    "▦"
                };
                if let Some(new_icon) = crate::ui::icon_picker_button(
                    ui,
                    ("database-header-icon", data.page_id),
                    current_icon,
                    34.0,
                    Some(palette.accent),
                ) {
                    data.icon = new_icon.clone();
                    intents.push(Intent::Action(Action::SetDatabaseRecordIcon {
                        page: data.page_id,
                        icon: new_icon,
                    }));
                }
                ui.add_space(10.0);
                let previous_title = data.title.clone();
                let title_response = ui.add_enabled(
                    !busy,
                    egui::TextEdit::singleline(&mut data.title)
                        .font(FontId::proportional(30.0))
                        .hint_text("Untitled Database")
                        .frame(false)
                        .margin(egui::Margin::ZERO)
                        .desired_width((ui.available_width() - 88.0).max(120.0)),
                );
                if title_response.lost_focus() && previous_title != data.title {
                    intents.push(Intent::Action(Action::RenameDatabaseRecord {
                        page: data.page_id,
                        title: data.title.clone(),
                    }));
                }
            });
            ui.add_space(16.0);
        } else if !data.title.is_empty() {
            ui.horizontal(|ui| {
                let current_icon = if !data.icon.is_empty() {
                    &data.icon
                } else {
                    "▦"
                };
                if let Some(new_icon) = crate::ui::icon_picker_button(
                    ui,
                    ("database-embedded-icon", data.page_id),
                    current_icon,
                    16.0,
                    Some(palette.accent),
                ) {
                    data.icon = new_icon.clone();
                    intents.push(Intent::Action(Action::SetDatabaseRecordIcon {
                        page: data.page_id,
                        icon: new_icon,
                    }));
                }
                ui.label(
                    RichText::new(&data.title)
                        .font(FontId::proportional(16.0))
                        .strong(),
                );
            });
            ui.add_space(6.0);
        }

        let horizontal_margin = if full_page { 44 } else { 0 };
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(horizontal_margin, 0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                    for view in [
                        DatabaseView::Table,
                        DatabaseView::Board,
                        DatabaseView::Gallery,
                    ] {
                        let selected = data.view == view;
                        let icon = match view {
                            DatabaseView::Table => "⊞",
                            DatabaseView::Board => "☷",
                            DatabaseView::Gallery => "🗂",
                        };
                        let tab_text = format!("{icon}  {}", view.label());
                        let fill = if selected {
                            palette.accent_soft
                        } else {
                            Color32::TRANSPARENT
                        };
                        let stroke = if selected {
                            egui::Stroke::new(1.0, palette.accent.linear_multiply(0.4))
                        } else {
                            egui::Stroke::NONE
                        };
                        let text_color = if selected {
                            palette.text
                        } else {
                            palette.muted
                        };
                        let button = egui::Button::new(
                            RichText::new(tab_text)
                                .font(FontId::proportional(13.0))
                                .color(text_color)
                                .strong(),
                        )
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(5);
                        if ui.add(button).clicked() && !selected {
                            intents.push(Intent::Action(Action::SetDatabaseView {
                                page: data.page_id,
                                view,
                            }));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let new_btn = egui::Button::new(
                            RichText::new("+ New record")
                                .font(FontId::proportional(13.0))
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(palette.accent)
                        .corner_radius(5);
                        if ui.add_enabled(!busy, new_btn).clicked() {
                            intents.push(Intent::Action(Action::CreateDatabaseRecord(data.page_id)));
                        }

                        if data.view != DatabaseView::Table {
                            ui.menu_button(
                                RichText::new("+ Property")
                                    .font(FontId::proportional(13.0))
                                    .color(palette.muted),
                                |ui| {
                                    add_property_menu(ui, data, intents);
                                },
                            );
                        }

                        show_view_options(ui, data, intents);

                        let sort_name = sort_key_label(data);
                        let direction_arrow = if data.sort_key == "manual" {
                            ""
                        } else if data.sort_descending {
                            " ▼"
                        } else {
                            " ▲"
                        };
                        ui.menu_button(
                            RichText::new(format!("⇅ Sort: {sort_name}{direction_arrow}"))
                                .font(FontId::proportional(12.5))
                                .color(palette.muted),
                            |ui| {
                                ui.label(RichText::new("Sort by").small().color(palette.muted));
                                for (label, key) in [
                                    ("Manual order", "manual"),
                                    ("Name", "title"),
                                    ("Created", "created_at"),
                                ] {
                                    let is_selected = data.sort_key == key;
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        if key == "manual" {
                                            intents.push(Intent::Action(Action::SetDatabaseSort {
                                                page: data.page_id,
                                                key: "manual".into(),
                                                descending: false,
                                            }));
                                        } else {
                                            toggle_sort(data, key, intents);
                                        }
                                    }
                                }
                                for property in &data.properties {
                                    let key = property.id.to_string();
                                    let is_selected = data.sort_key == key;
                                    if ui.selectable_label(is_selected, &property.name).clicked() {
                                        toggle_sort(data, &key, intents);
                                    }
                                }
                                if data.sort_key != "manual" {
                                    ui.separator();
                                    ui.label(RichText::new("Direction").small().color(palette.muted));
                                    if ui
                                        .selectable_label(!data.sort_descending, "Ascending (▲)")
                                        .clicked()
                                        && data.sort_descending
                                    {
                                        intents.push(Intent::Action(Action::SetDatabaseSort {
                                            page: data.page_id,
                                            key: data.sort_key.clone(),
                                            descending: false,
                                        }));
                                    }
                                    if ui
                                        .selectable_label(data.sort_descending, "Descending (▼)")
                                        .clicked()
                                        && !data.sort_descending
                                    {
                                        intents.push(Intent::Action(Action::SetDatabaseSort {
                                            page: data.page_id,
                                            key: data.sort_key.clone(),
                                            descending: true,
                                        }));
                                    }
                                }
                            },
                        );
                    });
                });

                ui.add_space(8.0);
                let divider_stroke = egui::Stroke::new(1.0, palette.border.linear_multiply(0.4));
                let avail_w = ui.available_width();
                let (divider_rect, _) = ui.allocate_exact_size(egui::vec2(avail_w, 1.0), egui::Sense::hover());
                ui.painter().line_segment([divider_rect.left_top(), divider_rect.right_top()], divider_stroke);
                ui.add_space(8.0);

                    match data.view {
                        DatabaseView::Table => show_table(ui, data, busy, palette, intents, drafts),
                        DatabaseView::Gallery => show_gallery(ui, data, busy, palette, intents),
                        DatabaseView::Board => show_board(ui, data, busy, palette, intents),
                    }
                });
            });
    });
}

fn sort_key_label(data: &DatabaseData) -> String {
    match data.sort_key.as_str() {
        "manual" => "Manual".to_owned(),
        "title" => "Name".to_owned(),
        "created_at" => "Created".to_owned(),
        key => key
            .parse::<i64>()
            .ok()
            .and_then(|id| data.properties.iter().find(|property| property.id == id))
            .map_or_else(|| "Property".to_owned(), |property| property.name.clone()),
    }
}

fn show_view_options(ui: &mut Ui, data: &mut DatabaseData, intents: &mut Vec<Intent>) {
    let mut changed = false;
    let properties = &data.properties;
    let board_properties: Vec<_> = properties
        .iter()
        .filter(|property| property.kind != "status" && property.kind != "select")
        .cloned()
        .collect();
    let view = data.view;
    let settings = &mut data.view_settings;
    ui.menu_button("View options", |ui| {
        ui.set_min_width(220.0);
        match view {
            DatabaseView::Table => {
                changed |= ui
                    .checkbox(&mut settings.table_show_created_at, "Show created date")
                    .changed();
                ui.separator();
                ui.label("Visible properties");
                changed |= property_visibility_options(
                    ui,
                    properties,
                    &mut settings.table_visible_properties,
                    usize::MAX,
                );
            }
            DatabaseView::Gallery => {
                ui.horizontal(|ui| {
                    ui.label("Items per row");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt(("gallery-columns-options", data.page_id))
                            .selected_text(format!("{} items/row", data.gallery_columns))
                            .show_ui(ui, |ui| {
                                for columns in 1..=6 {
                                    if ui
                                        .selectable_label(
                                            data.gallery_columns == columns,
                                            format!("{columns} items/row"),
                                        )
                                        .clicked()
                                        && data.gallery_columns != columns
                                    {
                                        data.gallery_columns = columns;
                                        intents.push(Intent::Action(Action::SetDatabaseGalleryColumns {
                                            page: data.page_id,
                                            columns,
                                        }));
                                    }
                                }
                            });
                    });
                });
                ui.separator();
                changed |= ui
                    .checkbox(&mut settings.gallery_show_cover, "Show cover image")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.gallery_show_title, "Show title")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.gallery_show_description, "Show description")
                    .changed();
                changed |= ui
                    .checkbox(&mut settings.gallery_show_created_at, "Show created date")
                    .changed();
                ui.separator();
                ui.label("Visible properties");
                changed |= property_visibility_options(
                    ui,
                    properties,
                    &mut settings.gallery_visible_properties,
                    2,
                );
            }
            DatabaseView::Board => {
                changed |= ui
                    .checkbox(&mut settings.board_show_description, "Show description")
                    .changed();
                ui.separator();
                ui.label("Visible properties");
                changed |= property_visibility_options(
                    ui,
                    &board_properties,
                    &mut settings.board_visible_properties,
                    2,
                );
            }
        }
        ui.separator();
        let current_section = match view {
            DatabaseView::Gallery => ThemeSection::Gallery,
            DatabaseView::Board => ThemeSection::Board,
            DatabaseView::Table => ThemeSection::Table,
        };
        if ui.button("🎨 Temayı düzenle").clicked() {
            intents.push(Intent::OpenSettingsTheme(current_section));
            ui.close_menu();
        }
    });
    if changed {
        intents.push(Intent::Action(Action::SetDatabaseViewSettings {
            page: data.page_id,
            settings: data.view_settings.clone(),
        }));
    }
}

fn property_visibility_options(
    ui: &mut Ui,
    properties: &[DatabaseProperty],
    visible_properties: &mut Option<Vec<i64>>,
    default_count: usize,
) -> bool {
    let defaults = || -> Vec<i64> {
        if default_count == usize::MAX {
            properties.iter().map(|property| property.id).collect()
        } else {
            properties
                .iter()
                .take(default_count)
                .map(|property| property.id)
                .collect()
        }
    };
    let mut changed = false;
    for property in properties {
        let is_visible = visible_properties.as_ref().map_or_else(
            || defaults().contains(&property.id),
            |ids| ids.contains(&property.id),
        );
        let mut checked = is_visible;
        if ui.checkbox(&mut checked, &property.name).changed() {
            let visible = visible_properties.get_or_insert_with(defaults);
            if checked {
                if !visible.contains(&property.id) {
                    visible.push(property.id);
                }
            } else {
                visible.retain(|id| *id != property.id);
            }
            changed = true;
        }
    }
    changed
}

pub struct RecordPropertiesTarget {
    pub page_id: i64,
    pub indent: f32,
    pub max_width: f32,
}

fn render_record_multi_select_chips(
    ui: &mut Ui,
    row: &DatabaseRow,
    property: &DatabaseProperty,
    available_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let value = row.values.get(&property.id).cloned().unwrap_or_default();
    let selected: Vec<String> = value
        .split('|')
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect();

    let record_id = row.page.id;
    let prop_id = property.id;

    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(available_width);
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.style_mut().visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        ui.style_mut().visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        ui.style_mut().visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;

        for (idx, item) in selected.iter().enumerate() {
            ui.push_id(("rec-multi-pill", idx), |ui| {
                let item_color = resolve_option_color(property.option_color(item));
                let (pill_bg, _pill_border, pill_text) =
                    option_pill_colors(palette, "multi-select", item, item_color);
                let pill_btn = egui::Button::new(
                    RichText::new(format!("{item}  ✕"))
                        .font(FontId::proportional(12.0))
                        .color(pill_text),
                )
                .fill(pill_bg)
                .stroke(egui::Stroke::NONE)
                .corner_radius(4);

                let resp = ui
                    .add_enabled(!busy, pill_btn)
                    .on_hover_text(format!(
                        "Click to remove '{item}' · Right-click to change color"
                    ));
                if resp.clicked() {
                    let mut updated = selected.clone();
                    updated.retain(|candidate| candidate != item);
                    intents.push(Intent::Action(Action::SetDatabaseProperty {
                        page: record_id,
                        property: prop_id,
                        value: updated.join("|"),
                    }));
                }
                resp.context_menu(|ui| {
                    show_color_palette_grid(ui, palette, item_color, |new_color| {
                        let mut opts = property.options.clone();
                        if let Some(target) = opts.iter_mut().find(|o| o.name == *item) {
                            target.color = new_color;
                            intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                property: prop_id,
                                options: opts,
                            }));
                        }
                    });
                });
            });
        }

        let trigger_label = if selected.is_empty() {
            "+ Add option"
        } else {
            "+ Add"
        };
        ui.scope(|ui| {
            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
            ui.menu_button(
                RichText::new(trigger_label)
                    .font(FontId::proportional(12.0))
                    .color(palette.muted),
                |ui| {
                    ui.set_min_width(230.0);
                    if !selected.is_empty()
                        && ui
                            .button(RichText::new("Clear all").small().color(palette.muted))
                            .clicked()
                    {
                        intents.push(Intent::Action(Action::SetDatabaseProperty {
                            page: record_id,
                            property: prop_id,
                            value: String::new(),
                        }));
                        ui.close_menu();
                    }
                    if property.options.is_empty() {
                        ui.label(
                            RichText::new("No options yet. Create one below.")
                                .small()
                                .italics()
                                .color(palette.muted),
                        );
                    }
                    for (idx, option) in property.options.iter().enumerate() {
                        ui.push_id(("rec-multi-opt-item", idx), |ui| {
                            let mut checked = selected.contains(&option.name);
                            let opt_color = resolve_option_color(option.color.as_deref());
                            let dot_color = option_base_color(
                                palette,
                                "multi-select",
                                &option.name,
                                opt_color,
                            );
                            ui.horizontal(|ui| {
                                ui.menu_button(
                                    RichText::new("●")
                                        .font(FontId::proportional(13.0))
                                        .color(dot_color),
                                    |ui| {
                                        show_color_palette_grid(
                                            ui,
                                            palette,
                                            opt_color,
                                            |new_color| {
                                                let mut opts = property.options.clone();
                                                if let Some(target) = opts
                                                    .iter_mut()
                                                    .find(|o| o.name == option.name)
                                                {
                                                    target.color = new_color;
                                                    intents.push(Intent::Action(
                                                        Action::SetDatabasePropertyOptions {
                                                            property: prop_id,
                                                            options: opts,
                                                        },
                                                    ));
                                                }
                                            },
                                        );
                                    },
                                )
                                .response
                                .on_hover_text("Change color");

                                if ui.checkbox(&mut checked, &option.name).changed() {
                                    let mut updated = selected.clone();
                                    if checked {
                                        if !updated.contains(&option.name) {
                                            updated.push(option.name.clone());
                                        }
                                    } else {
                                        updated.retain(|value| value != &option.name);
                                    }
                                    intents.push(Intent::Action(Action::SetDatabaseProperty {
                                        page: record_id,
                                        property: prop_id,
                                        value: updated.join("|"),
                                    }));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("✕")
                                                        .font(FontId::proportional(11.0))
                                                        .color(palette.danger),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_text(format!(
                                                "Delete '{}' option",
                                                option.name
                                            ))
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(
                                                Action::RemoveDatabasePropertyOption {
                                                    property: prop_id,
                                                    option: option.name.clone(),
                                                },
                                            ));
                                        }
                                    },
                                );
                            });
                        });
                    }
                    ui.separator();
                    let quick_id =
                        egui::Id::new(("db-prop-multi-quick-opt", record_id, prop_id));
                    let mut new_opt = ui
                        .ctx()
                        .data(|d| d.get_temp::<String>(quick_id))
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut new_opt)
                                .hint_text("Create option…")
                                .desired_width(145.0),
                        );
                        if resp.changed() {
                            ui.ctx().data_mut(|d| d.insert_temp(quick_id, new_opt.clone()));
                        }
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.small_button("+").clicked() || enter)
                            && !new_opt.trim().is_empty()
                        {
                            let trimmed = new_opt.trim().to_owned();
                            let mut opts = property.options.clone();
                            if !opts.iter().any(|o| o.name == trimmed) {
                                opts.push(PropertyOption::new(&trimmed));
                                intents.push(Intent::Action(
                                    Action::SetDatabasePropertyOptions {
                                        property: prop_id,
                                        options: opts,
                                    },
                                ));
                            }
                            let mut updated = selected.clone();
                            if !updated.contains(&trimmed) {
                                updated.push(trimmed);
                            }
                            intents.push(Intent::Action(Action::SetDatabaseProperty {
                                page: record_id,
                                property: prop_id,
                                value: updated.join("|"),
                            }));
                            new_opt.clear();
                            ui.ctx().data_mut(|d| d.remove::<String>(quick_id));
                        }
                    });
                },
            );
        });
    });
}

pub fn show_record_property_rows(
    ui: &mut Ui,
    data: &DatabaseData,
    page_id: i64,
    section_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let Some(row) = data.rows.iter().find(|row| row.page.id == page_id) else {
        return;
    };
    let label_w = crate::ui::page_properties::LABEL_COLUMN_WIDTH;
    let value_w = (section_width - label_w - 28.0).max(120.0);

    for property in &data.properties {
        let row_id = ui.id().with(("rec-prop-row", page_id, property.id));
        let is_hovered = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(row_id))
            .unwrap_or(false);
        let row_bg = if is_hovered {
            palette.hover.linear_multiply(0.55)
        } else {
            Color32::TRANSPARENT
        };

        let frame_resp = egui::Frame::new()
            .fill(Color32::TRANSPARENT)
            .inner_margin(egui::Margin::symmetric(0, 1))
            .show(ui, |ui| {
                ui.set_min_width(section_width - 12.0);
                ui.push_id(("rec-prop-row-scope", page_id, property.id), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        // Left container: Record Property Header
                        egui::Frame::new()
                            .fill(row_bg)
                            .corner_radius(5)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                let icon = property_icon(&property.kind);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(label_w - 12.0, 24.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.scope(|ui| {
                                            ui.style_mut().visuals.widgets.inactive.weak_bg_fill =
                                                Color32::TRANSPARENT;
                                            ui.style_mut().visuals.widgets.inactive.bg_fill =
                                                Color32::TRANSPARENT;
                                            ui.style_mut().visuals.widgets.inactive.bg_stroke =
                                                egui::Stroke::NONE;
                                            ui.spacing_mut().button_padding = egui::vec2(4.0, 3.0);

                                            let menu_resp = ui.menu_button(
                                                RichText::new(format!("{icon}   {}", property.name))
                                                    .font(FontId::proportional(13.0))
                                                    .color(if is_hovered {
                                                        palette.text
                                                    } else {
                                                        palette.muted
                                                    }),
                                                |ui| {
                                                    show_property_edit_menu(
                                                        ui, property, busy, palette, intents,
                                                        drafts,
                                                    );
                                                },
                                            );
                                            menu_resp
                                                .response
                                                .on_hover_text(format!(
                                                    "{} ({}) · Click or right-click to edit property",
                                                    property.name,
                                                    property_types_label(&property.kind)
                                                ))
                                                .context_menu(|ui| {
                                                    show_property_edit_menu(
                                                        ui, property, busy, palette, intents,
                                                        drafts,
                                                    );
                                                });
                                        });
                                    },
                                );
                            });

                        // Right container: Record Property Value
                        egui::Frame::new()
                            .fill(row_bg)
                            .corner_radius(5)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(value_w - 12.0, 24.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        if property.kind == "multi-select" {
                                            render_record_multi_select_chips(
                                                ui,
                                                row,
                                                property,
                                                value_w - 12.0,
                                                busy,
                                                palette,
                                                intents,
                                            );
                                        } else {
                                            property_editor(
                                                ui,
                                                row,
                                                property,
                                                value_w - 12.0,
                                                busy,
                                                palette,
                                                intents,
                                                drafts.values,
                                            );
                                        }
                                    },
                                );
                            });
                    });
                });
            });

        let hovered_now = ui.rect_contains_pointer(frame_resp.response.rect);
        if hovered_now != is_hovered {
            ui.ctx().request_repaint();
        }
        ui.ctx().data_mut(|d| d.insert_temp(row_id, hovered_now));
    }
}

#[allow(dead_code)]
pub fn show_record_properties(
    ui: &mut Ui,
    data: &DatabaseData,
    target: RecordPropertiesTarget,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let RecordPropertiesTarget {
        page_id,
        indent,
        max_width,
    } = target;
    let section_width = max_width.clamp(300.0, 680.0);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.vertical(|ui| {
            ui.set_max_width(section_width);
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
            show_record_property_rows(
                ui,
                data,
                page_id,
                section_width,
                busy,
                palette,
                intents,
                drafts,
            );
        });
    });
}

fn sorted_rows(data: &DatabaseData) -> Vec<&DatabaseRow> {
    data.rows.iter().collect()
}

fn toggle_sort(data: &DatabaseData, key: &str, intents: &mut Vec<Intent>) {
    intents.push(Intent::Action(Action::SetDatabaseSort {
        page: data.page_id,
        key: key.into(),
        descending: data.sort_key == key && !data.sort_descending,
    }));
}

fn measure_text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
            .size()
            .x
    })
}

fn auto_fit_name_column(ui: &Ui, rows: &[&DatabaseRow]) -> f32 {
    let header_w = measure_text_width(ui, "Aa  Name ▼", FontId::proportional(12.5)) + 28.0;
    let max_row_w = rows
        .iter()
        .map(|row| {
            let title = if row.page.title.is_empty() {
                "Untitled"
            } else {
                &row.page.title
            };
            measure_text_width(ui, title, FontId::proportional(13.0)) + 64.0
        })
        .fold(0.0_f32, f32::max);
    header_w.max(max_row_w).clamp(90.0, 600.0)
}

fn auto_fit_created_column(ui: &Ui, rows: &[&DatabaseRow]) -> f32 {
    let header_w = measure_text_width(ui, "🕒  Created ▼", FontId::proportional(12.5)) + 28.0;
    let max_row_w = rows
        .iter()
        .map(|row| {
            let text = format_date(row.page.created_at);
            measure_text_width(ui, &text, FontId::proportional(12.0)) + 24.0
        })
        .fold(0.0_f32, f32::max);
    header_w.max(max_row_w).clamp(80.0, 400.0)
}

fn auto_fit_property_column(
    ui: &Ui,
    property: &DatabaseProperty,
    rows: &[&DatabaseRow],
) -> f32 {
    let icon = property_icon(&property.kind);
    let header_label = format!("{icon}  {} ▼", property.name);
    let header_w = measure_text_width(ui, &header_label, FontId::proportional(12.5)) + 44.0;
    let max_row_w = rows
        .iter()
        .map(|row| {
            let val = row
                .values
                .get(&property.id)
                .map(String::as_str)
                .unwrap_or("");
            match property.kind.as_str() {
                "checkbox" => 44.0,
                "multi-select" => {
                    let joined = val
                        .split('|')
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(", ");
                    if joined.is_empty() {
                        40.0
                    } else {
                        measure_text_width(ui, &joined, FontId::proportional(12.0)) + 36.0
                    }
                }
                "select" | "status" => {
                    if val.is_empty() {
                        40.0
                    } else {
                        measure_text_width(ui, val, FontId::proportional(12.0)) + 36.0
                    }
                }
                _ => {
                    if val.is_empty() {
                        40.0
                    } else {
                        measure_text_width(ui, val, FontId::proportional(13.0)) + 24.0
                    }
                }
            }
        })
        .fold(0.0_f32, f32::max);
    header_w.max(max_row_w).clamp(64.0, 600.0)
}

fn handle_column_resize(
    ui: &mut Ui,
    x: f32,
    top_y: f32,
    total_height: f32,
    id_salt: impl std::hash::Hash + Copy,
    current_width: f32,
    min_width: f32,
    palette: Palette,
    auto_fit: impl FnOnce(&Ui) -> f32,
) -> Option<f32> {
    let handle_rect = egui::Rect::from_center_size(
        egui::pos2(x, top_y + total_height * 0.5),
        egui::vec2(8.0, total_height),
    );
    let id = ui.id().with(("table-col-resize", id_salt));
    let response = ui
        .interact(handle_rect, id, egui::Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let drag_state_id = id.with("drag-width");

    if response.double_clicked() {
        ui.ctx().data_mut(|d| d.remove::<f32>(drag_state_id));
        return Some(auto_fit(ui));
    }

    if response.drag_started() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(drag_state_id, current_width));
    }

    if response.dragged() {
        let prev = ui
            .ctx()
            .data(|d| d.get_temp::<f32>(drag_state_id))
            .unwrap_or(current_width);
        let next = (prev + response.drag_delta().x).clamp(min_width, 800.0);
        ui.ctx().data_mut(|d| d.insert_temp(drag_state_id, next));
    }

    if response.hovered() || response.dragged() {
        ui.painter().line_segment(
            [
                egui::pos2(x, top_y),
                egui::pos2(x, top_y + total_height),
            ],
            egui::Stroke::new(2.0, palette.accent),
        );
    }

    if response.drag_stopped() {
        let final_w = ui.ctx().data(|d| d.get_temp::<f32>(drag_state_id));
        ui.ctx().data_mut(|d| d.remove::<f32>(drag_state_id));
        if let Some(final_w) = final_w {
            return Some(final_w);
        }
    }
    None
}

fn live_column_width(
    ui: &Ui,
    id_salt: impl std::hash::Hash,
    stored_width: f32,
) -> f32 {
    let id = ui.id().with(("table-col-resize", id_salt)).with("drag-width");
    ui.ctx()
        .data(|d| d.get_temp::<f32>(id))
        .unwrap_or(stored_width)
}

fn show_table(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let rows: Vec<DatabaseRow> = data.rows.clone();
    let row_refs: Vec<&DatabaseRow> = rows.iter().collect();
    let visible_properties: Vec<DatabaseProperty> = data
        .properties
        .iter()
        .filter(|property| {
            data.view_settings
                .table_visible_properties
                .as_ref()
                .is_none_or(|visible| visible.contains(&property.id))
        })
        .cloned()
        .collect();
    let show_created_at = data.view_settings.table_show_created_at;

    let base_name_w = data
        .view_settings
        .table_name_width
        .unwrap_or(TABLE_NAME_WIDTH);
    let base_created_w = data
        .view_settings
        .table_created_width
        .unwrap_or(TABLE_CREATED_WIDTH);

    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let name_w = live_column_width(ui, ("name", data.page_id), base_name_w);
            let created_w = live_column_width(ui, ("created", data.page_id), base_created_w);
            let prop_widths: Vec<f32> = visible_properties
                .iter()
                .map(|prop| {
                    let stored = data
                        .view_settings
                        .table_column_widths
                        .get(&prop.id)
                        .copied()
                        .unwrap_or(TABLE_PROPERTY_WIDTH);
                    live_column_width(ui, ("prop", data.page_id, prop.id), stored)
                })
                .collect();

            let table_width = name_w
                + if show_created_at { created_w } else { 0.0 }
                + prop_widths.iter().sum::<f32>()
                + TABLE_ADD_PROP_WIDTH;

            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.set_min_width(table_width);

            let table_top = ui.cursor().min.y;
            let table_left = ui.cursor().min.x;

            // 1. Table Header Row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_cell(
                    ui,
                    name_w,
                    TABLE_HEADER_HEIGHT,
                    palette,
                    true,
                    true,
                    false,
                    |ui, _hovered| {
                        let is_active_sort = data.sort_key == "title";
                        let sort_arrow = if is_active_sort {
                            if data.sort_descending { " ▼" } else { " ▲" }
                        } else {
                            ""
                        };
                        let header_label = format!("Aa  Name{sort_arrow}");
                        let text_color = if is_active_sort { palette.text } else { palette.muted };
                        let resp = ui.add_sized(
                            [ui.available_width(), TABLE_HEADER_HEIGHT],
                            egui::Button::new(
                                RichText::new(header_label)
                                    .font(FontId::proportional(12.5))
                                    .color(text_color)
                                    .strong(),
                            )
                            .frame(false),
                        );
                        if resp.clicked() {
                            toggle_sort(data, "title", intents);
                        }
                    },
                );

                if show_created_at {
                    table_cell(
                        ui,
                        created_w,
                        TABLE_HEADER_HEIGHT,
                        palette,
                        true,
                        false,
                        false,
                        |ui, _hovered| {
                            let is_active_sort = data.sort_key == "created_at";
                            let sort_arrow = if is_active_sort {
                                if data.sort_descending { " ▼" } else { " ▲" }
                            } else {
                                ""
                            };
                            let header_label = format!("🕒  Created{sort_arrow}");
                            let text_color =
                                if is_active_sort { palette.text } else { palette.muted };
                            let resp = ui.add_sized(
                                [ui.available_width(), TABLE_HEADER_HEIGHT],
                                egui::Button::new(
                                    RichText::new(header_label)
                                        .font(FontId::proportional(12.5))
                                        .color(text_color)
                                        .strong(),
                                )
                                .frame(false),
                            );
                            if resp.clicked() {
                                toggle_sort(data, "created_at", intents);
                            }
                        },
                    );
                }

                for (idx, property) in visible_properties.iter().enumerate() {
                    let col_w = prop_widths[idx];
                    table_cell(
                        ui,
                        col_w,
                        TABLE_HEADER_HEIGHT,
                        palette,
                        true,
                        false,
                        false,
                        |ui, hovered| {
                            show_property_header(
                                ui, data, property, col_w, busy, palette, hovered, intents, drafts,
                            );
                        },
                    );
                }

                // Hover-only "+ Property" cell on the right of the header
                let (add_rect, add_resp) = ui.allocate_exact_size(
                    egui::vec2(TABLE_ADD_PROP_WIDTH, TABLE_HEADER_HEIGHT),
                    egui::Sense::hover(),
                );
                let popup_open_id = ui.id().with(("add-prop-popup-open", data.page_id));
                let was_open = ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(popup_open_id))
                    .unwrap_or(false);
                let show_add = add_resp.hovered()
                    || was_open
                    || ui.rect_contains_pointer(add_rect.expand2(egui::vec2(18.0, 12.0)));
                let stroke = egui::Stroke::new(1.0, table_border_color(palette));
                ui.painter()
                    .line_segment([add_rect.left_top(), add_rect.left_bottom()], stroke);
                if show_add {
                    let fill = if add_resp.hovered() {
                        palette.hover
                    } else {
                        palette.background
                    };
                    ui.painter().rect_filled(add_rect, 0.0, fill);
                    ui.painter()
                        .line_segment([add_rect.left_top(), add_rect.right_top()], stroke);
                    ui.painter()
                        .line_segment([add_rect.left_bottom(), add_rect.right_bottom()], stroke);
                    ui.painter()
                        .line_segment([add_rect.right_top(), add_rect.right_bottom()], stroke);

                    let mut child_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(add_rect)
                            .layout(egui::Layout::centered_and_justified(
                                egui::Direction::LeftToRight,
                            )),
                    );
                    let menu_resp = child_ui.menu_button(
                        RichText::new("+").size(16.0).color(palette.muted).strong(),
                        |ui| {
                            ui.ctx().data_mut(|d| d.insert_temp(popup_open_id, true));
                            add_property_menu(ui, data, intents);
                        },
                    );
                    if menu_resp.inner.is_none() {
                        ui.ctx().data_mut(|d| d.insert_temp(popup_open_id, false));
                    }
                } else {
                    ui.ctx().data_mut(|d| d.insert_temp(popup_open_id, false));
                }
            });

            // 2. Data Rows
            let selection_id = egui::Id::new(("database-selected-rows", data.page_id));
            let anchor_id = egui::Id::new(("database-anchor-row", data.page_id));
            let mut selected_rows: std::collections::HashSet<crate::domain::model::PageId> = ui
                .ctx()
                .data(|d| d.get_temp(selection_id))
                .unwrap_or_default();
            let mut anchor_row: Option<crate::domain::model::PageId> = ui
                .ctx()
                .data(|d| d.get_temp(anchor_id))
                .flatten();

            let valid_ids: std::collections::HashSet<_> = rows.iter().map(|r| r.page.id).collect();
            selected_rows.retain(|id| valid_ids.contains(id));
            if let Some(anchor) = anchor_row
                && !valid_ids.contains(&anchor)
            {
                anchor_row = None;
            }

            let (is_ctrl, is_shift) =
                ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.modifiers.shift));
            let primary_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
            let secondary_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
            let primary_down = ui.input(|i| i.pointer.primary_down());
            let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
            let primary_released = ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
            let pointer_pos = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));

            let marquee_anchor_id = egui::Id::new(("database-table-marquee-anchor", data.page_id));
            let marquee_active_id = egui::Id::new(("database-table-marquee-active", data.page_id));
            let marquee_base_id = egui::Id::new(("database-table-marquee-base", data.page_id));

            let mut marquee_anchor: Option<egui::Pos2> = ui.ctx().data(|d| d.get_temp(marquee_anchor_id)).flatten();
            let mut marquee_active: bool = ui.ctx().data(|d| d.get_temp(marquee_active_id)).unwrap_or(false);
            let mut marquee_base: std::collections::HashSet<crate::domain::model::PageId> = ui
                .ctx()
                .data(|d| d.get_temp(marquee_base_id))
                .unwrap_or_default();

            if !selected_rows.is_empty() && !ui.ctx().wants_keyboard_input() {
                let delete_pressed = ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                        || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
                });
                if delete_pressed {
                    for id in &selected_rows {
                        intents.push(Intent::Action(Action::Trash(*id)));
                    }
                    selected_rows.clear();
                    anchor_row = None;
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                    selected_rows.clear();
                    anchor_row = None;
                }
            }

            let natural_rows_h = (rows.len() as f32 * TABLE_ROW_HEIGHT).max(TABLE_ROW_HEIGHT);
            let (active_rows_h, has_custom_height) = resolve_vertical_height(
                ui,
                data,
                natural_rows_h,
                DEFAULT_TABLE_MAX_ROWS_HEIGHT,
                TABLE_ROW_HEIGHT,
            );

            let rows_start_cursor = ui.cursor().min;
            let rows_area = egui::Rect::from_min_size(
                rows_start_cursor,
                egui::vec2(table_width.max(ui.available_width()), active_rows_h),
            );

            if primary_pressed && let Some(pos) = pointer_pos && rows_area.contains(pos) {
                marquee_anchor = Some(pos);
                marquee_active = false;
                marquee_base = if is_ctrl { selected_rows.clone() } else { std::collections::HashSet::new() };
            }

            if primary_down && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos) {
                let delta = (pos - anchor).length();
                if !marquee_active && delta > 6.0 {
                    marquee_active = true;
                    if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                        ui.ctx().memory_mut(|m| m.surrender_focus(focused));
                    }
                }
            }

            let selection_rect = if marquee_active && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos) {
                let rect = egui::Rect::from_two_pos(anchor, pos);
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new(("table-marquee-painter", data.page_id)),
                ));
                painter.rect_filled(
                    rect,
                    2.0,
                    Color32::from_rgba_unmultiplied(
                        palette.accent.r(),
                        palette.accent.g(),
                        palette.accent.b(),
                        30,
                    ),
                );
                painter.rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.2, palette.accent),
                    egui::StrokeKind::Inside,
                );
                let edge_zone = 28.0;
                if pos.y < rows_area.top() + edge_zone {
                    let speed = ((rows_area.top() + edge_zone - pos.y) * 0.45).clamp(2.0, 18.0);
                    ui.scroll_with_delta(egui::vec2(0.0, speed));
                } else if pos.y > rows_area.bottom() - edge_zone {
                    let speed = ((pos.y - (rows_area.bottom() - edge_zone)) * 0.45).clamp(2.0, 18.0);
                    ui.scroll_with_delta(egui::vec2(0.0, -speed));
                }
                ui.ctx().request_repaint();
                Some(rect)
            } else {
                None
            };

            egui::ScrollArea::vertical()
                .id_salt(("table-rows-vscroll", data.page_id))
                .max_height(active_rows_h)
                .min_scrolled_height(active_rows_h)
                .auto_shrink([false, !has_custom_height])
                .show_rows(ui, TABLE_ROW_HEIGHT, rows.len(), |ui, visible| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    for row in &rows[visible] {
                        let row_id = row.page.id;
                        let row_cursor = ui.cursor().min;
                        let row_rect = egui::Rect::from_min_size(
                            row_cursor,
                            egui::vec2(table_width, TABLE_ROW_HEIGHT),
                        );
                        if let Some(sel_rect) = selection_rect {
                            if sel_rect.intersects(row_rect) {
                                selected_rows.insert(row_id);
                            } else if !marquee_base.contains(&row_id) {
                                selected_rows.remove(&row_id);
                            }
                        }
                        let is_row_selected = selected_rows.contains(&row_id);
                        let row_hovered = ui.rect_contains_pointer(row_rect);

                        if primary_clicked && !marquee_active && let Some(pos) = pointer_pos && row_rect.contains(pos) {
                            if is_ctrl {
                                if selected_rows.contains(&row_id) {
                                    selected_rows.remove(&row_id);
                                } else {
                                    selected_rows.insert(row_id);
                                    anchor_row = Some(row_id);
                                }
                                if let Some(f) = ui.ctx().memory(|m| m.focused()) {
                                    ui.ctx().memory_mut(|m| m.surrender_focus(f));
                                }
                            } else if is_shift {
                                let anchor = anchor_row.unwrap_or(row_id);
                                let p1 = rows.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                                let p2 = rows.iter().position(|r| r.page.id == row_id).unwrap_or(0);
                                let start = p1.min(p2);
                                let end = p1.max(p2);
                                selected_rows.clear();
                                for r in &rows[start..=end] {
                                    selected_rows.insert(r.page.id);
                                }
                                if let Some(f) = ui.ctx().memory(|m| m.focused()) {
                                    ui.ctx().memory_mut(|m| m.surrender_focus(f));
                                }
                            }
                        }
                        if secondary_clicked && !marquee_active && let Some(pos) = pointer_pos && row_rect.contains(pos) {
                            if !selected_rows.contains(&row_id) {
                                selected_rows.clear();
                                selected_rows.insert(row_id);
                                anchor_row = Some(row_id);
                            }
                        }

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                            table_cell(
                                ui,
                                name_w,
                                TABLE_ROW_HEIGHT,
                                palette,
                                false,
                                true,
                                is_row_selected,
                                |ui, cell_hovered| {
                                    let hovered = row_hovered || cell_hovered;
                                    let name_cell_rect = egui::Rect::from_min_size(
                                        row_cursor,
                                        egui::vec2(name_w, TABLE_ROW_HEIGHT),
                                    );

                                    let mut sel_box_hit: Option<egui::Rect> = None;
                                    if hovered || is_row_selected {
                                        let sel_box_rect = egui::Rect::from_center_size(
                                            ui.cursor().min + egui::vec2(8.0, 10.0),
                                            egui::vec2(14.0, 14.0),
                                        );
                                        sel_box_hit = Some(sel_box_rect.expand(2.0));
                                        let sel_resp = ui.interact(
                                            sel_box_rect,
                                            ui.id().with(("row-sel", row.page.id)),
                                            egui::Sense::click(),
                                        );
                                        let check_stroke = egui::Stroke::new(
                                            1.0,
                                            if is_row_selected { palette.accent } else { palette.muted },
                                        );
                                        ui.painter().rect_stroke(sel_box_rect, 3.0, check_stroke, egui::StrokeKind::Inside);
                                        if is_row_selected {
                                            ui.painter().rect_filled(sel_box_rect.shrink(2.5), 2.0, palette.accent);
                                        }
                                        if sel_resp.clicked() && !marquee_active {
                                            if is_ctrl {
                                                if selected_rows.contains(&row_id) {
                                                    selected_rows.remove(&row_id);
                                                } else {
                                                    selected_rows.insert(row_id);
                                                    anchor_row = Some(row_id);
                                                }
                                            } else if is_shift {
                                                let anchor = anchor_row.unwrap_or(row_id);
                                                let p1 = rows.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                                                let p2 = rows.iter().position(|r| r.page.id == row_id).unwrap_or(0);
                                                let start = p1.min(p2);
                                                let end = p1.max(p2);
                                                selected_rows.clear();
                                                for r in &rows[start..=end] {
                                                    selected_rows.insert(r.page.id);
                                                }
                                            } else if is_row_selected && selected_rows.len() == 1 {
                                                selected_rows.clear();
                                                anchor_row = None;
                                            } else {
                                                selected_rows.clear();
                                                selected_rows.insert(row_id);
                                                anchor_row = Some(row_id);
                                            }
                                        }
                                        ui.add_space(18.0);
                                    }

                                    let icon_start_x = ui.cursor().min.x;
                                    let icon = if row.page.icon.is_empty() {
                                        "📄"
                                    } else {
                                        &row.page.icon
                                    };
                                    if let Some(new_icon) = crate::ui::icon_picker_button(
                                        ui,
                                        ("table-row-icon", row.page.id),
                                        icon,
                                        13.0,
                                        None,
                                    ) {
                                        intents.push(Intent::Action(Action::SetDatabaseRecordIcon {
                                            page: row.page.id,
                                            icon: new_icon,
                                        }));
                                    }
                                    let icon_hit_rect = egui::Rect::from_min_max(
                                        egui::pos2(icon_start_x, name_cell_rect.top()),
                                        egui::pos2(ui.cursor().min.x + 2.0, name_cell_rect.bottom()),
                                    );

                                    let title = drafts
                                        .titles
                                        .entry(row.page.id)
                                        .or_insert_with(|| row.page.title.clone());
                                    let width_avail = (if hovered {
                                        name_w - 82.0
                                    } else {
                                        name_w - 42.0
                                    })
                                    .max(24.0);

                                    let display_text = if title.is_empty() { "Untitled" } else { title.as_str() };
                                    let text_width = ui.fonts(|f| {
                                        f.layout_no_wrap(
                                            display_text.to_string(),
                                            FontId::proportional(13.0),
                                            Color32::WHITE,
                                        )
                                        .rect
                                        .width()
                                    });
                                    let title_edit_w = (text_width + 8.0).clamp(24.0, width_avail);

                                    let response = ui.add_enabled(
                                        !busy,
                                        egui::TextEdit::singleline(title)
                                            .hint_text("Untitled")
                                            .desired_width(title_edit_w)
                                            .font(FontId::proportional(13.0))
                                            .frame(false)
                                            .margin(egui::Margin::ZERO),
                                    );

                                    if response.gained_focus() {
                                        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), response.id).unwrap_or_default();
                                        let char_count = title.chars().count();
                                        let range = CCursorRange::two(
                                            CCursor::new(0),
                                            CCursor::new(char_count),
                                        );
                                        state.cursor.set_char_range(Some(range));
                                        state.store(ui.ctx(), response.id);
                                        ui.ctx().request_repaint();
                                    }

                                    if response.lost_focus() && title != &row.page.title {
                                        intents.push(Intent::Action(Action::RenameDatabaseRecord {
                                            page: row.page.id,
                                            title: title.clone(),
                                        }));
                                    }

                                    response.context_menu(|ui| {
                                        if selected_rows.contains(&row.page.id) && selected_rows.len() > 1 {
                                            let count = selected_rows.len();
                                            ui.label(RichText::new(format!("{count} records selected")).small().color(palette.muted));
                                            ui.separator();
                                            if ui.button(format!("Duplicate {count} records")).clicked() {
                                                for id in &selected_rows {
                                                    intents.push(Intent::Action(Action::Duplicate(*id)));
                                                }
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button(RichText::new(format!("Move {count} records to trash")).color(palette.danger)).clicked() {
                                                for id in &selected_rows {
                                                    intents.push(Intent::Action(Action::Trash(*id)));
                                                }
                                                ui.close_menu();
                                            }
                                        } else {
                                            if ui.button("↗  Open page").clicked() {
                                                intents.push(Intent::Open(row.page.id, None));
                                                ui.close_menu();
                                            }
                                            if ui.button("🗖  Diyalog olarak aç").clicked() {
                                                intents.push(Intent::OpenInDialog(row.page.id));
                                                ui.close_menu();
                                            }
                                            if ui.button("Duplicate").clicked() {
                                                intents.push(Intent::Action(Action::Duplicate(row.page.id)));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button(RichText::new("Move to trash").color(palette.danger)).clicked() {
                                                intents.push(Intent::Action(Action::Trash(row.page.id)));
                                                ui.close_menu();
                                            }
                                        }
                                        ui.separator();
                                        if ui.button("🎨 Temayı düzenle").clicked() {
                                            intents.push(Intent::OpenSettingsTheme(ThemeSection::Table));
                                            ui.close_menu();
                                        }
                                    });

                                    let mut open_btn_hit: Option<egui::Rect> = None;
                                    if hovered {
                                        let btn_rect = egui::Rect::from_center_size(
                                            egui::pos2(name_cell_rect.right() - 14.0, name_cell_rect.center().y),
                                            egui::vec2(20.0, 20.0),
                                        );
                                        open_btn_hit = Some(btn_rect.expand(2.0));
                                        let open_resp = ui.put(
                                            btn_rect,
                                            egui::Button::new(
                                                RichText::new("↗")
                                                    .color(palette.text)
                                                    .font(FontId::proportional(12.5)),
                                            )
                                            .frame(false),
                                        );
                                        if open_resp.on_hover_text("Open page").clicked() {
                                            intents.push(Intent::Open(row.page.id, None));
                                        }
                                    }

                                    let is_interactive = pointer_pos.map_or(false, |pos| {
                                        sel_box_hit.map_or(false, |r| r.contains(pos))
                                            || icon_hit_rect.contains(pos)
                                            || response.rect.contains(pos)
                                            || open_btn_hit.map_or(false, |r| r.contains(pos))
                                    });

                                    let in_empty_space = pointer_pos.map_or(false, |pos| name_cell_rect.contains(pos)) && !is_interactive;

                                    if in_empty_space && !marquee_active {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        if primary_clicked
                                            && !is_ctrl
                                            && !is_shift
                                            && ui.ctx().dragged_id().is_none()
                                        {
                                            intents.push(Intent::Open(row.page.id, None));
                                        }
                                    }
                                },
                            );

                            if show_created_at {
                                table_cell(
                                    ui,
                                    created_w,
                                    TABLE_ROW_HEIGHT,
                                    palette,
                                    false,
                                    false,
                                    is_row_selected,
                                    |ui, _| {
                                        ui.label(
                                            RichText::new(format_date(row.page.created_at))
                                                .font(FontId::proportional(12.0))
                                                .color(palette.muted),
                                        );
                                    },
                                );
                            }

                            for (idx, property) in visible_properties.iter().enumerate() {
                                let col_w = prop_widths[idx];
                                table_cell(
                                    ui,
                                    col_w,
                                    TABLE_ROW_HEIGHT,
                                    palette,
                                    false,
                                    false,
                                    is_row_selected,
                                    |ui, _| {
                                        ui.push_id(("table-prop-cell", row.page.id, property.id), |ui| {
                                            property_editor(
                                                ui,
                                                row,
                                                property,
                                                col_w,
                                                busy,
                                                palette,
                                                intents,
                                                drafts.values,
                                            );
                                        });
                                    },
                                );
                            }
                        });
                    }
                });

            // 3. "+ New" Bottom Row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_cell(
                    ui,
                    name_w,
                    TABLE_ROW_HEIGHT,
                    palette,
                    false,
                    true,
                    false,
                    |ui, hovered| {
                        let text_color = if hovered {
                            palette.accent
                        } else {
                            palette.muted
                        };
                        let resp = ui.add_sized(
                            [ui.available_width(), TABLE_ROW_HEIGHT],
                            egui::Button::new(
                                RichText::new("+ New")
                                    .font(FontId::proportional(13.0))
                                    .color(text_color),
                            )
                            .frame(false),
                        );
                        if resp.clicked() && !busy {
                            intents.push(Intent::Action(Action::CreateDatabaseRecord(data.page_id)));
                        }
                    },
                );

                if show_created_at {
                    table_cell(
                        ui,
                        created_w,
                        TABLE_ROW_HEIGHT,
                        palette,
                        false,
                        false,
                        false,
                        |_ui, _| {},
                    );
                }
                for col_w in &prop_widths {
                    table_cell(
                        ui,
                        *col_w,
                        TABLE_ROW_HEIGHT,
                        palette,
                        false,
                        false,
                        false,
                        |_ui, _| {},
                    );
                }
            });

            if primary_clicked
                && !is_ctrl
                && !is_shift
                && !marquee_active
                && let Some(pos) = pointer_pos
                && rows_area.contains(pos)
                && pos.y > rows_start_cursor.y + (rows.len() as f32 * TABLE_ROW_HEIGHT)
            {
                selected_rows.clear();
                anchor_row = None;
            }

            if primary_released {
                marquee_anchor = None;
                marquee_active = false;
                marquee_base.clear();
            }

            if !selected_rows.is_empty() {
                let count = selected_rows.len();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{count} record{} selected", if count == 1 { "" } else { "s" }))
                            .strong()
                            .color(palette.accent),
                    );
                    if ui.button("Duplicate").clicked() {
                        for id in &selected_rows {
                            intents.push(Intent::Action(Action::Duplicate(*id)));
                        }
                    }
                    if ui.button(RichText::new("Delete").color(palette.danger)).clicked() {
                        for id in &selected_rows {
                            intents.push(Intent::Action(Action::Trash(*id)));
                        }
                        selected_rows.clear();
                        anchor_row = None;
                    }
                    if ui.button("Clear selection").clicked() {
                        selected_rows.clear();
                        anchor_row = None;
                    }
                });
            }

            ui.ctx().data_mut(|d| {
                d.insert_temp(marquee_anchor_id, marquee_anchor);
                d.insert_temp(marquee_active_id, marquee_active);
                d.insert_temp(marquee_base_id, marquee_base);
                d.insert_temp(selection_id, selected_rows.clone());
                d.insert_temp(anchor_id, anchor_row);
            });

            // 4. Table Calculation / Footer
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_footer_cell(ui, name_w, palette, true, |ui| {
                    ui.label(
                        RichText::new(format!("Count {}", rows.len()))
                            .font(FontId::proportional(11.0))
                            .color(palette.muted),
                    );
                });
                if show_created_at {
                    table_footer_cell(ui, created_w, palette, false, |_ui| {});
                }
                for col_w in &prop_widths {
                    table_footer_cell(ui, *col_w, palette, false, |_ui| {});
                }
            });

            let table_bottom = ui.cursor().min.y;
            let total_h = (table_bottom - table_top).max(TABLE_HEADER_HEIGHT);
            let data_columns_width = name_w
                + if show_created_at { created_w } else { 0.0 }
                + prop_widths.iter().sum::<f32>();
            let mut settings_changed = show_vertical_resize_handle(
                ui,
                data,
                data_columns_width,
                active_rows_h,
                TABLE_ROW_HEIGHT,
                palette,
            );

            let mut boundary_x = table_left + name_w;

            if let Some(new_w) = handle_column_resize(
                ui,
                boundary_x,
                table_top,
                total_h,
                ("name", data.page_id),
                name_w,
                90.0,
                palette,
                |ui| auto_fit_name_column(ui, &row_refs),
            ) {
                data.view_settings.table_name_width = Some(new_w);
                settings_changed = true;
            }

            if show_created_at {
                boundary_x += created_w;
                if let Some(new_w) = handle_column_resize(
                    ui,
                    boundary_x,
                    table_top,
                    total_h,
                    ("created", data.page_id),
                    created_w,
                    80.0,
                    palette,
                    |ui| auto_fit_created_column(ui, &row_refs),
                ) {
                    data.view_settings.table_created_width = Some(new_w);
                    settings_changed = true;
                }
            }

            for (idx, prop) in visible_properties.iter().enumerate() {
                let col_w = prop_widths[idx];
                boundary_x += col_w;
                if let Some(new_w) = handle_column_resize(
                    ui,
                    boundary_x,
                    table_top,
                    total_h,
                    ("prop", data.page_id, prop.id),
                    col_w,
                    64.0,
                    palette,
                    |ui| auto_fit_property_column(ui, prop, &row_refs),
                ) {
                    data.view_settings.table_column_widths.insert(prop.id, new_w);
                    settings_changed = true;
                }
            }

            if settings_changed {
                intents.push(Intent::Action(Action::SetDatabaseViewSettings {
                    page: data.page_id,
                    settings: data.view_settings.clone(),
                }));
            }
        });
}

fn table_border_color(palette: Palette) -> Color32 {
    let blend = |border_val: u8, text_val: u8| -> u8 {
        (f32::from(border_val) * 0.82 + f32::from(text_val) * 0.18).round() as u8
    };
    Color32::from_rgb(
        blend(palette.border.r(), palette.muted.r()),
        blend(palette.border.g(), palette.muted.g()),
        blend(palette.border.b(), palette.muted.b()),
    )
}

fn table_cell(
    ui: &mut Ui,
    width: f32,
    height: f32,
    palette: Palette,
    _is_header: bool,
    _is_first_col: bool,
    is_selected: bool,
    add_contents: impl FnOnce(&mut Ui, bool),
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    let fill = if is_selected {
        palette.accent_soft
    } else if hovered {
        palette.hover
    } else {
        palette.background
    };
    ui.painter().rect_filled(rect, 0.0, fill);

    let stroke = egui::Stroke::new(1.0, table_border_color(palette));
    ui.painter().line_segment([rect.left_top(), rect.right_top()], stroke);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    ui.painter().line_segment([rect.left_top(), rect.left_bottom()], stroke);
    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], stroke);

    if is_selected && _is_first_col {
        ui.painter().line_segment(
            [rect.left_top(), rect.left_bottom()],
            egui::Stroke::new(3.0, palette.accent),
        );
    }

    let content_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 4.0, rect.bottom()),
    );
    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    add_contents(&mut child_ui, hovered);
}

fn table_footer_cell(
    ui: &mut Ui,
    width: f32,
    palette: Palette,
    _is_first_col: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::hover());
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    let fill = if hovered {
        palette.hover
    } else {
        palette.background
    };
    ui.painter().rect_filled(rect, 0.0, fill);

    let stroke = egui::Stroke::new(1.0, table_border_color(palette));
    ui.painter().line_segment([rect.left_top(), rect.right_top()], stroke);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    ui.painter().line_segment([rect.left_top(), rect.left_bottom()], stroke);
    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], stroke);

    let content_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 4.0, rect.bottom()),
    );
    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    add_contents(&mut child_ui);
}

pub fn show_property_edit_menu(
    ui: &mut Ui,
    property: &DatabaseProperty,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    ui.set_min_width(245.0);
    let draft = drafts
        .properties
        .entry(property.id)
        .or_insert_with(|| (property.name.clone(), String::new()));

    ui.label(
        RichText::new("EDIT PROPERTY")
            .font(FontId::proportional(10.5))
            .strong()
            .color(palette.muted),
    );
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(property_icon(&property.kind))
                .font(FontId::proportional(13.0))
                .color(palette.muted),
        );
        let name_changed = !draft.0.trim().is_empty() && draft.0.trim() != property.name;
        let input_w = if name_changed { 170.0 } else { 205.0 };
        let name_resp = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut draft.0).desired_width(input_w),
        );
        let enter_pressed = name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let apply_clicked = name_changed
            && ui
                .small_button(RichText::new("✓").color(palette.accent).strong())
                .on_hover_text("Save name")
                .clicked();
        if (name_resp.lost_focus() || enter_pressed || apply_clicked) && name_changed {
            intents.push(Intent::Action(Action::RenameDatabaseProperty {
                property: property.id,
                name: draft.0.trim().to_owned(),
            }));
        }
    });

    ui.add_space(6.0);
    ui.label(RichText::new("Property type").small().color(palette.muted));
    egui::ComboBox::from_id_salt(("property-kind", property.id))
        .width(230.0)
        .selected_text(format!(
            "{}   {}",
            property_icon(&property.kind),
            property_types_label(&property.kind)
        ))
        .show_ui(ui, |ui| {
            for (kind, label) in PROPERTY_TYPES {
                let item_label = format!("{}   {label}", property_icon(kind));
                if ui.selectable_label(property.kind == kind, item_label).clicked()
                    && property.kind != kind
                {
                    intents.push(Intent::Action(Action::SetDatabasePropertyType {
                        property: property.id,
                        kind: kind.into(),
                    }));
                }
            }
        });

    if matches!(property.kind.as_str(), "select" | "status" | "multi-select") {
        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("Options").small().color(palette.muted));

        if property.options.is_empty() {
            ui.label(
                RichText::new("No options yet. Add one below.")
                    .small()
                    .italics()
                    .color(palette.muted),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt(("prop-options-scroll", property.id))
                .max_height(180.0)
                .show(ui, |ui| {
                    for (opt_idx, option) in property.options.iter().enumerate() {
                        ui.push_id(("prop-edit-opt-row", property.id, opt_idx, &option.name), |ui| {
                            let opt_draft_id =
                                egui::Id::new(("prop-option-edit", property.id, opt_idx, &option.name));
                            let mut opt_text = ui
                                .ctx()
                                .data(|d| d.get_temp::<String>(opt_draft_id))
                                .unwrap_or_else(|| option.name.clone());
                            let opt_color = resolve_option_color(option.color.as_deref());
                            let dot_color = option_base_color(palette, &property.kind, &option.name, opt_color);
                            ui.horizontal(|ui| {
                                ui.menu_button(
                                    RichText::new("●")
                                        .font(FontId::proportional(14.0))
                                        .color(dot_color),
                                    |ui| {
                                        show_color_palette_grid(ui, palette, opt_color, |new_color| {
                                            let mut reordered = property.options.clone();
                                            if let Some(target) = reordered.get_mut(opt_idx) {
                                                target.color = new_color;
                                                intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                                    property: property.id,
                                                    options: reordered,
                                                }));
                                            }
                                        });
                                    },
                                )
                                .response
                                .on_hover_text("Change color");

                                let resp = ui.add_enabled(
                                    !busy,
                                    egui::TextEdit::singleline(&mut opt_text).desired_width(145.0),
                                );
                                if resp.changed() {
                                    ui.ctx()
                                        .data_mut(|d| d.insert_temp(opt_draft_id, opt_text.clone()));
                                }
                                let commit = resp.lost_focus()
                                    || (resp.has_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if commit && !opt_text.trim().is_empty() && opt_text.trim() != option.name {
                                    intents.push(Intent::Action(
                                        Action::RenameDatabasePropertyOption {
                                            property: property.id,
                                            old_option: option.name.clone(),
                                            new_option: opt_text.trim().to_owned(),
                                        },
                                    ));
                                    ui.ctx().data_mut(|d| d.remove::<String>(opt_draft_id));
                                }
                                if opt_idx > 0
                                    && ui
                                        .small_button("↑")
                                        .on_hover_text("Move up")
                                        .clicked()
                                {
                                    let mut reordered = property.options.clone();
                                    reordered.swap(opt_idx, opt_idx - 1);
                                    intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                        property: property.id,
                                        options: reordered,
                                    }));
                                }
                                if opt_idx + 1 < property.options.len()
                                    && ui
                                        .small_button("↓")
                                        .on_hover_text("Move down")
                                        .clicked()
                                {
                                    let mut reordered = property.options.clone();
                                    reordered.swap(opt_idx, opt_idx + 1);
                                    intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                        property: property.id,
                                        options: reordered,
                                    }));
                                }
                                if ui
                                    .small_button(RichText::new("✕").color(palette.danger))
                                    .on_hover_text("Remove option")
                                    .clicked()
                                {
                                    intents.push(Intent::Action(
                                        Action::RemoveDatabasePropertyOption {
                                            property: property.id,
                                            option: option.name.clone(),
                                        },
                                    ));
                                }
                            });
                        });
                    }
                });
        }

        ui.add_space(4.0);
        let new_opt_id = egui::Id::new(("prop-new-option", property.id));
        let mut new_opt = ui
            .ctx()
            .data(|d| d.get_temp::<String>(new_opt_id))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            let new_resp = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut new_opt)
                    .hint_text("New option…")
                    .desired_width(155.0),
            );
            if new_resp.changed() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(new_opt_id, new_opt.clone()));
            }
            let enter_add =
                new_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add_clicked = ui
                .add_enabled(!busy && !new_opt.trim().is_empty(), egui::Button::new("+ Add"))
                .clicked();
            if (enter_add || add_clicked) && !new_opt.trim().is_empty() {
                let mut updated = property.options.clone();
                for part in new_opt.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if !updated.iter().any(|existing| existing.name == part) {
                        updated.push(PropertyOption::new(part));
                    }
                }
                intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                    property: property.id,
                    options: updated,
                }));
                ui.ctx().data_mut(|d| d.remove::<String>(new_opt_id));
            }
        });
    }

    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy, egui::Button::new("↑ Move up"))
            .clicked()
        {
            intents.push(Intent::Action(Action::MoveDatabaseProperty {
                property: property.id,
                direction: -1,
            }));
        }
        if ui
            .add_enabled(!busy, egui::Button::new("↓ Move down"))
            .clicked()
        {
            intents.push(Intent::Action(Action::MoveDatabaseProperty {
                property: property.id,
                direction: 1,
            }));
        }
    });
    if ui
        .button(RichText::new("🗑  Delete property").color(palette.danger))
        .clicked()
    {
        intents.push(Intent::Action(Action::DeleteDatabaseProperty(property.id)));
        ui.close_menu();
    }
}

fn show_property_header(
    ui: &mut Ui,
    data: &DatabaseData,
    property: &DatabaseProperty,
    column_width: f32,
    busy: bool,
    palette: Palette,
    hovered: bool,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let sort_key = property.id.to_string();
    let is_active_sort = data.sort_key == sort_key;
    let sort_arrow = if is_active_sort {
        if data.sort_descending { " ▼" } else { " ▲" }
    } else {
        ""
    };
    let icon = property_icon(&property.kind);
    let label = format!("{icon}  {}{sort_arrow}", property.name);
    let text_color = if is_active_sort { palette.text } else { palette.muted };

    let text_width = if hovered {
        (column_width - 38.0).max(24.0)
    } else {
        (column_width - 12.0).max(24.0)
    };

    let resp = ui
        .add_sized(
            [text_width, TABLE_HEADER_HEIGHT],
            egui::Button::new(
                RichText::new(label)
                    .font(FontId::proportional(12.5))
                    .color(text_color)
                    .strong(),
            )
            .frame(false),
        )
        .on_hover_text(format!("{} (Left click: sort · Right click: edit)", property.name));

    if resp.clicked() {
        toggle_sort(data, &sort_key, intents);
    }

    resp.context_menu(|ui| {
        show_property_edit_menu(ui, property, busy, palette, intents, drafts);
    });

    if hovered {
        let menu_resp = ui.menu_button(
            RichText::new("⋯")
                .color(palette.muted)
                .font(FontId::proportional(13.0)),
            |ui| {
                show_property_edit_menu(ui, property, busy, palette, intents, drafts);
            },
        );
        menu_resp.response.context_menu(|ui| {
            show_property_edit_menu(ui, property, busy, palette, intents, drafts);
        });
    }
}

fn property_types_label(kind: &str) -> &'static str {
    PROPERTY_TYPES
        .iter()
        .find(|(key, _)| *key == kind)
        .map_or("Text", |(_, label)| label)
}

fn property_editor(
    ui: &mut Ui,
    row: &DatabaseRow,
    property: &DatabaseProperty,
    column_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    value_drafts: &mut std::collections::HashMap<(i64, i64), String>,
) {
    let value = row.values.get(&property.id).cloned().unwrap_or_default();
    let action = |value: String| {
        Intent::Action(Action::SetDatabaseProperty {
            page: row.page.id,
            property: property.id,
            value,
        })
    };
    match property.kind.as_str() {
        "select" | "status" => {
            let (pill_fill, text_color, pill_stroke) = if value.is_empty() {
                (Color32::TRANSPARENT, palette.muted, egui::Stroke::NONE)
            } else {
                let opt_col = resolve_option_color(property.option_color(&value));
                let (bg, border, text) =
                    option_pill_colors(palette, &property.kind, &value, opt_col);
                (bg, text, egui::Stroke::new(1.0, border))
            };
            let display_text = if value.is_empty() { "—" } else { &value };
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = pill_stroke;
                ui.menu_button(
                    RichText::new(display_text)
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                    |ui| {
                        ui.set_min_width(190.0);
                        if ui
                            .selectable_label(value.is_empty(), "None")
                            .clicked()
                            && !value.is_empty()
                        {
                            intents.push(action(String::new()));
                            ui.close_menu();
                        }
                        for (opt_idx, option) in property.options.iter().enumerate() {
                            ui.push_id(("cell-sel-opt", row.page.id, property.id, opt_idx, &option.name), |ui| {
                                let opt_color = resolve_option_color(option.color.as_deref());
                                let dot_color = option_base_color(palette, &property.kind, &option.name, opt_color);
                                ui.horizontal(|ui| {
                                    ui.menu_button(
                                        RichText::new("●")
                                            .font(FontId::proportional(13.0))
                                            .color(dot_color),
                                        |ui| {
                                            show_color_palette_grid(ui, palette, opt_color, |new_color| {
                                                let mut opts = property.options.clone();
                                                if let Some(target) = opts.iter_mut().find(|o| o.name == option.name) {
                                                    target.color = new_color;
                                                    intents.push(Intent::Action(
                                                        Action::SetDatabasePropertyOptions {
                                                            property: property.id,
                                                            options: opts,
                                                        },
                                                    ));
                                                }
                                            });
                                        },
                                    )
                                    .response
                                    .on_hover_text("Change color");

                                    if ui.selectable_label(value == option.name, &option.name).clicked()
                                        && value != option.name
                                    {
                                        intents.push(action(option.name.clone()));
                                        ui.close_menu();
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui
                                            .small_button(RichText::new("✕").size(10.5).color(palette.muted))
                                            .on_hover_text("Delete option from property")
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(
                                                Action::RemoveDatabasePropertyOption {
                                                    property: property.id,
                                                    option: option.name.clone(),
                                                },
                                            ));
                                        }
                                    });
                                });
                            });
                        }
                        ui.separator();
                        let quick_opt_id =
                            egui::Id::new(("cell-new-opt", row.page.id, property.id));
                        let mut new_opt = ui
                            .ctx()
                            .data(|d| d.get_temp::<String>(quick_opt_id))
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut new_opt)
                                    .hint_text("New option…")
                                    .desired_width(115.0),
                            );
                            if resp.changed() {
                                ui.ctx()
                                    .data_mut(|d| d.insert_temp(quick_opt_id, new_opt.clone()));
                            }
                            let enter =
                                resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (enter || ui.small_button("+").clicked())
                                && !new_opt.trim().is_empty()
                            {
                                let trimmed = new_opt.trim().to_owned();
                                let mut opts = property.options.clone();
                                if !opts.iter().any(|o| o.name == trimmed) {
                                    opts.push(PropertyOption::new(&trimmed));
                                    intents.push(Intent::Action(
                                        Action::SetDatabasePropertyOptions {
                                            property: property.id,
                                            options: opts,
                                        },
                                    ));
                                }
                                intents.push(action(trimmed));
                                ui.ctx().data_mut(|d| d.remove::<String>(quick_opt_id));
                                ui.close_menu();
                            }
                        });
                    },
                );
            });
        }
        "checkbox" => {
            let mut checked = value == "true";
            if ui
                .add(egui::Checkbox::without_text(&mut checked))
                .changed()
            {
                intents.push(action(checked.to_string()));
            }
        }
        "multi-select" => {
            let selected: Vec<String> = value
                .split('|')
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect();
            let max_chars = ((column_width - 28.0) / 7.0).max(6.0) as usize;
            let selected_label = if selected.is_empty() {
                "—".to_owned()
            } else {
                let joined = selected.join(", ");
                if joined.chars().count() > max_chars {
                    format!(
                        "{}…",
                        joined
                            .chars()
                            .take(max_chars.saturating_sub(1))
                            .collect::<String>()
                    )
                } else {
                    joined
                }
            };
            let (pill_fill, text_color, pill_stroke) = if selected.is_empty() {
                (Color32::TRANSPARENT, palette.muted, egui::Stroke::NONE)
            } else if selected.len() == 1 {
                let opt_col = resolve_option_color(property.option_color(&selected[0]));
                let (bg, _border, text) =
                    option_pill_colors(palette, "multi-select", &selected[0], opt_col);
                (bg, text, egui::Stroke::NONE)
            } else {
                (
                    palette.accent_soft,
                    if palette.is_dark() { Color32::WHITE } else { palette.text },
                    egui::Stroke::NONE,
                )
            };
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = pill_stroke;
                ui.menu_button(
                    RichText::new(selected_label)
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                    |ui| {
                        ui.set_min_width(195.0);
                        if property.options.is_empty() {
                            ui.label(
                                RichText::new("No options yet.")
                                    .small()
                                    .italics()
                                    .color(palette.muted),
                            );
                        }
                        for (opt_idx, option) in property.options.iter().enumerate() {
                            ui.push_id(("cell-multi-opt", row.page.id, property.id, opt_idx, &option.name), |ui| {
                                let mut enabled = selected.contains(&option.name);
                                let opt_color = resolve_option_color(option.color.as_deref());
                                let dot_color = option_base_color(palette, "multi-select", &option.name, opt_color);
                                ui.horizontal(|ui| {
                                    ui.menu_button(
                                        RichText::new("●")
                                            .font(FontId::proportional(13.0))
                                            .color(dot_color),
                                        |ui| {
                                            show_color_palette_grid(ui, palette, opt_color, |new_color| {
                                                let mut opts = property.options.clone();
                                                if let Some(target) = opts.iter_mut().find(|o| o.name == option.name) {
                                                    target.color = new_color;
                                                    intents.push(Intent::Action(
                                                        Action::SetDatabasePropertyOptions {
                                                            property: property.id,
                                                            options: opts,
                                                        },
                                                    ));
                                                }
                                            });
                                        },
                                    )
                                    .response
                                    .on_hover_text("Change color");

                                    if ui.checkbox(&mut enabled, &option.name).changed() {
                                        let mut next = selected.clone();
                                        if enabled && !next.contains(&option.name) {
                                            next.push(option.name.clone());
                                        } else if !enabled {
                                            next.retain(|item| item != &option.name);
                                        }
                                        intents.push(action(next.join("|")));
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui
                                            .small_button(RichText::new("✕").size(10.5).color(palette.muted))
                                            .on_hover_text("Delete option from property")
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(
                                                Action::RemoveDatabasePropertyOption {
                                                    property: property.id,
                                                    option: option.name.clone(),
                                                },
                                            ));
                                        }
                                    });
                                });
                            });
                        }
                        ui.separator();
                        let quick_opt_id =
                            egui::Id::new(("cell-multi-new-opt", row.page.id, property.id));
                        let mut new_opt = ui
                            .ctx()
                            .data(|d| d.get_temp::<String>(quick_opt_id))
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut new_opt)
                                    .hint_text("New option…")
                                    .desired_width(120.0),
                            );
                            if resp.changed() {
                                ui.ctx()
                                    .data_mut(|d| d.insert_temp(quick_opt_id, new_opt.clone()));
                            }
                            let enter =
                                resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (enter || ui.small_button("+").clicked())
                                && !new_opt.trim().is_empty()
                            {
                                let trimmed = new_opt.trim().to_owned();
                                let mut opts = property.options.clone();
                                if !opts.iter().any(|o| o.name == trimmed) {
                                    opts.push(PropertyOption::new(&trimmed));
                                    intents.push(Intent::Action(
                                        Action::SetDatabasePropertyOptions {
                                            property: property.id,
                                            options: opts,
                                        },
                                    ));
                                }
                                let mut next = selected.clone();
                                if !next.contains(&trimmed) {
                                    next.push(trimmed);
                                }
                                intents.push(action(next.join("|")));
                                ui.ctx().data_mut(|d| d.remove::<String>(quick_opt_id));
                            }
                        });
                    },
                );
            });
        }
        "date" => {
            let is_empty = value.trim().is_empty();
            let display_label = if is_empty {
                "📅  Empty".to_owned()
            } else {
                format!(
                    "📅  {}",
                    crate::ui::page_properties::format_iso_date_pretty(value.trim())
                )
            };
            let label_color = if is_empty {
                palette.muted.gamma_multiply(0.6)
            } else {
                palette.text
            };
            ui.scope(|ui| {
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.menu_button(
                    RichText::new(display_label)
                        .font(FontId::proportional(12.5))
                        .color(label_color),
                    |ui| {
                        if let Some(chosen) =
                            crate::ui::page_properties::show_date_picker_popup(
                                ui,
                                ("db-cell-cal", row.page.id, property.id),
                                &value,
                                palette,
                            )
                        {
                            value_drafts.remove(&(row.page.id, property.id));
                            intents.push(action(chosen));
                            ui.close_menu();
                        }
                    },
                )
                .response
                .on_hover_text("Select date from calendar");
            });
        }
        _ => {
            let edited = value_drafts
                .entry((row.page.id, property.id))
                .or_insert_with(|| value.clone());
            let text = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(edited)
                    .desired_width((column_width - 16.0).max(24.0))
                    .font(FontId::proportional(13.0))
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .hint_text(if property.kind == "number" {
                        "Empty"
                    } else {
                        "Empty"
                    }),
            );
            let enter = text.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (text.lost_focus() || enter) && edited != &value {
                intents.push(action(edited.clone()));
            }
        }
    }
}

pub fn add_property_menu(ui: &mut Ui, data: &DatabaseData, intents: &mut Vec<Intent>) {
    let name_id = egui::Id::new(("new-database-property-name", data.page_id));
    let mut name = ui
        .ctx()
        .data(|data| data.get_temp::<String>(name_id))
        .unwrap_or_else(|| format!("Property {}", data.properties.len() + 1));
    ui.label(RichText::new("Property name").small());
    if ui.text_edit_singleline(&mut name).changed() {
        ui.ctx()
            .data_mut(|data| data.insert_temp(name_id, name.clone()));
    }
    ui.separator();
    ui.label(RichText::new("Type").small());
    for (kind, label) in PROPERTY_TYPES {
        let icon = property_icon(kind);
        if ui.button(format!("{icon}  {label}")).clicked() {
            intents.push(Intent::Action(Action::AddDatabaseProperty {
                database: data.page_id,
                name: if name.trim().is_empty() {
                    format!("Property {}", data.properties.len() + 1)
                } else {
                    name.trim().to_owned()
                },
                kind: kind.into(),
                options: Vec::new(),
            }));
            ui.ctx().data_mut(|data| data.remove::<String>(name_id));
            ui.close_menu();
        }
    }
}

fn show_gallery(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let cols = usize::from(data.gallery_columns);
    show_cards(ui, data, busy, palette, intents, cols);
}

fn show_cards(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    columns: usize,
) {
    let rows = sorted_rows(data);
    let selection_id = egui::Id::new(("database-selected-rows", data.page_id));
    let anchor_id = egui::Id::new(("database-anchor-row", data.page_id));
    let mut selected_rows: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    let mut anchor_row: Option<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(anchor_id))
        .flatten();
    let valid_ids: std::collections::HashSet<_> = rows.iter().map(|r| r.page.id).collect();
    selected_rows.retain(|id| valid_ids.contains(id));
    if let Some(anchor) = anchor_row
        && !valid_ids.contains(&anchor)
    {
        anchor_row = None;
    }
    let (is_ctrl, is_shift) =
        ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.modifiers.shift));
    let primary_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
    let secondary_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    let primary_released = ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
    let pointer_pos = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));

    let marquee_anchor_id = egui::Id::new(("database-cards-marquee-anchor", data.page_id));
    let marquee_active_id = egui::Id::new(("database-cards-marquee-active", data.page_id));
    let marquee_base_id = egui::Id::new(("database-cards-marquee-base", data.page_id));

    let mut marquee_anchor: Option<egui::Pos2> = ui.ctx().data(|d| d.get_temp(marquee_anchor_id)).flatten();
    let mut marquee_active: bool = ui.ctx().data(|d| d.get_temp(marquee_active_id)).unwrap_or(false);
    let mut marquee_base: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(marquee_base_id))
        .unwrap_or_default();

    if !selected_rows.is_empty() && !ui.ctx().wants_keyboard_input() {
        let delete_pressed = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        });
        if delete_pressed {
            for id in &selected_rows {
                intents.push(Intent::Action(Action::Trash(*id)));
            }
            selected_rows.clear();
            anchor_row = None;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            selected_rows.clear();
            anchor_row = None;
        }
    }

    let columns = columns.max(1);
    let row_count = rows.len().div_ceil(columns);
    let gallery = data.view == DatabaseView::Gallery;
    let settings = &data.view_settings;
    let show_cover = gallery && settings.gallery_show_cover;
    let show_title = !gallery || settings.gallery_show_title;
    let show_created_at = if gallery {
        settings.gallery_show_created_at
    } else {
        true
    };
    let show_description = gallery && settings.gallery_show_description;
    let visible_properties: Option<Vec<i64>> = if gallery {
        settings.gallery_visible_properties.clone()
    } else {
        None
    };
    let (card_height, row_gap) = gallery_card_metrics(data, gallery);
    let col_gap = 12.0;
    let total_width = (ui.available_width() - 12.0).max(200.0);
    let card_width =
        ((total_width - col_gap * columns.saturating_sub(1) as f32) / columns as f32).max(110.0);

    let natural_h = if row_count == 0 {
        card_height
    } else {
        row_count as f32 * card_height + row_count.saturating_sub(1) as f32 * row_gap
    };
    let (active_h, has_custom_height) = resolve_vertical_height(
        ui,
        data,
        natural_h,
        DEFAULT_GALLERY_MAX_HEIGHT,
        card_height,
    );

    let cards_start_cursor = ui.cursor().min;
    let cards_area = egui::Rect::from_min_size(
        cards_start_cursor,
        egui::vec2(total_width, active_h),
    );

    if primary_pressed && let Some(pos) = pointer_pos && cards_area.contains(pos) {
        marquee_anchor = Some(pos);
        marquee_active = false;
        marquee_base = if is_ctrl { selected_rows.clone() } else { std::collections::HashSet::new() };
    }

    if primary_down && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos) {
        let delta = (pos - anchor).length();
        if !marquee_active && delta > 6.0 {
            marquee_active = true;
            if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                ui.ctx().memory_mut(|m| m.surrender_focus(focused));
            }
        }
    }

    let selection_rect = if marquee_active && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos) {
        let rect = egui::Rect::from_two_pos(anchor, pos);
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new(("cards-marquee-painter", data.page_id)),
        ));
        painter.rect_filled(
            rect,
            2.0,
            Color32::from_rgba_unmultiplied(
                palette.accent.r(),
                palette.accent.g(),
                palette.accent.b(),
                30,
            ),
        );
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.2, palette.accent),
            egui::StrokeKind::Inside,
        );
        let edge_zone = 28.0;
        if pos.y < cards_area.top() + edge_zone {
            let speed = ((cards_area.top() + edge_zone - pos.y) * 0.45).clamp(2.0, 18.0);
            ui.scroll_with_delta(egui::vec2(0.0, speed));
        } else if pos.y > cards_area.bottom() - edge_zone {
            let speed = ((pos.y - (cards_area.bottom() - edge_zone)) * 0.45).clamp(2.0, 18.0);
            ui.scroll_with_delta(egui::vec2(0.0, -speed));
        }
        ui.ctx().request_repaint();
        Some(rect)
    } else {
        None
    };

    if row_count == 0 {
        let (empty_rect, _) =
            ui.allocate_exact_size(egui::vec2(total_width, active_h), egui::Sense::hover());
        ui.painter().rect_stroke(
            empty_rect,
            8.0,
            egui::Stroke::new(1.0, palette.border.linear_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            empty_rect.center(),
            egui::Align2::CENTER_CENTER,
            "No records yet. Click \"+ New record\" below to add one.",
            FontId::proportional(13.0),
            palette.muted,
        );
    } else {
        egui::ScrollArea::vertical()
            .id_salt(("database-gallery-scroll", data.page_id))
            .max_height(active_h)
            .min_scrolled_height(active_h)
            .auto_shrink([false, !has_custom_height])
            .show_rows(ui, card_height, row_count, |ui, visible| {
                ui.spacing_mut().item_spacing.y = row_gap;
                for row_index in visible {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = col_gap;
                        for row in rows.iter().skip(row_index * columns).take(columns) {
                            let (card_rect, card_resp) = ui.allocate_exact_size(
                                egui::vec2(card_width, card_height),
                                egui::Sense::click(),
                            );
                            if let Some(sel_rect) = selection_rect {
                                if sel_rect.intersects(card_rect) {
                                    selected_rows.insert(row.page.id);
                                } else if !marquee_base.contains(&row.page.id) {
                                    selected_rows.remove(&row.page.id);
                                }
                            }
                            let is_selected = selected_rows.contains(&row.page.id);
                            let hovered = card_resp.hovered();
                            let bg = if is_selected {
                                palette.accent_soft
                            } else if hovered {
                                palette.hover
                            } else {
                                palette.gallery_card
                            };
                            let border_color = if is_selected {
                                palette.accent
                            } else if hovered {
                                palette.accent.linear_multiply(0.6)
                            } else {
                                palette.gallery_border
                            };
                            ui.painter().rect_filled(card_rect, 8.0, bg);
                            ui.painter().rect_stroke(
                                card_rect,
                                8.0,
                                egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, border_color),
                                egui::StrokeKind::Inside,
                            );

                            card_resp.context_menu(|ui| {
                                if ui.button("↗  Sayfayı aç").clicked() {
                                    intents.push(Intent::Open(row.page.id, None));
                                    ui.close_menu();
                                }
                                if ui.button("🗖  Diyalog olarak aç").clicked() {
                                    intents.push(Intent::OpenInDialog(row.page.id));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button(RichText::new("🗑  Sil").color(palette.danger)).clicked() {
                                    intents.push(Intent::Action(Action::Trash(row.page.id)));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("🎨 Temayı düzenle").clicked() {
                                    intents.push(Intent::OpenSettingsTheme(ThemeSection::Gallery));
                                    ui.close_menu();
                                }
                            });

                            if secondary_clicked && !marquee_active && let Some(pos) = pointer_pos && card_rect.contains(pos) {
                                if !selected_rows.contains(&row.page.id) {
                                    selected_rows.clear();
                                    selected_rows.insert(row.page.id);
                                    anchor_row = Some(row.page.id);
                                }
                            }

                            if card_resp.double_clicked() && !busy {
                                intents.push(Intent::Open(row.page.id, None));
                            } else if card_resp.clicked() && !busy && !marquee_active {
                                if is_ctrl {
                                    if selected_rows.contains(&row.page.id) {
                                        selected_rows.remove(&row.page.id);
                                    } else {
                                        selected_rows.insert(row.page.id);
                                        anchor_row = Some(row.page.id);
                                    }
                                } else if is_shift {
                                    let anchor = anchor_row.unwrap_or(row.page.id);
                                    let p1 = rows.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                                    let p2 = rows.iter().position(|r| r.page.id == row.page.id).unwrap_or(0);
                                    let start = p1.min(p2);
                                    let end = p1.max(p2);
                                    selected_rows.clear();
                                    for r in &rows[start..=end] {
                                        selected_rows.insert(r.page.id);
                                    }
                                } else {
                                    selected_rows.clear();
                                    selected_rows.insert(row.page.id);
                                    anchor_row = Some(row.page.id);
                                }
                            }
                            card_resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                            if hovered || is_selected {
                                let sel_box_rect = egui::Rect::from_center_size(
                                    card_rect.left_top() + egui::vec2(16.0, 16.0),
                                    egui::vec2(16.0, 16.0),
                                );
                                let sel_resp = ui.interact(
                                    sel_box_rect,
                                    ui.id().with(("card-sel", row.page.id)),
                                    egui::Sense::click(),
                                );
                                let check_stroke = egui::Stroke::new(
                                    1.0,
                                    if is_selected { palette.accent } else { palette.muted },
                                );
                                ui.painter().rect_filled(sel_box_rect, 3.0, palette.sidebar);
                                ui.painter().rect_stroke(sel_box_rect, 3.0, check_stroke, egui::StrokeKind::Inside);
                                if is_selected {
                                    ui.painter().rect_filled(sel_box_rect.shrink(3.0), 2.0, palette.accent);
                                }
                                if sel_resp.clicked() && !marquee_active {
                                    if is_ctrl {
                                        if selected_rows.contains(&row.page.id) {
                                            selected_rows.remove(&row.page.id);
                                        } else {
                                            selected_rows.insert(row.page.id);
                                            anchor_row = Some(row.page.id);
                                        }
                                    } else if is_shift {
                                        let anchor = anchor_row.unwrap_or(row.page.id);
                                        let p1 = rows.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                                        let p2 = rows.iter().position(|r| r.page.id == row.page.id).unwrap_or(0);
                                        let start = p1.min(p2);
                                        let end = p1.max(p2);
                                        selected_rows.clear();
                                        for r in &rows[start..=end] {
                                            selected_rows.insert(r.page.id);
                                        }
                                    } else if is_selected && selected_rows.len() == 1 {
                                        selected_rows.clear();
                                        anchor_row = None;
                                    } else {
                                        selected_rows.clear();
                                        selected_rows.insert(row.page.id);
                                        anchor_row = Some(row.page.id);
                                    }
                                }
                            }

                            if hovered {
                                let open_box_rect = egui::Rect::from_center_size(
                                    card_rect.right_top() + egui::vec2(-28.0, 16.0),
                                    egui::vec2(44.0, 18.0),
                                );
                                let open_resp = ui.interact(
                                    open_box_rect,
                                    ui.id().with(("card-open", row.page.id)),
                                    egui::Sense::click(),
                                );
                                let bg = if open_resp.hovered() { palette.accent } else { palette.sidebar };
                                let fg = if open_resp.hovered() { Color32::WHITE } else { palette.muted };
                                ui.painter().rect_filled(open_box_rect, 3.0, bg);
                                ui.painter().rect_stroke(open_box_rect, 3.0, egui::Stroke::new(1.0, palette.border), egui::StrokeKind::Inside);
                                ui.painter().text(
                                    open_box_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "OPEN ↗",
                                    FontId::proportional(10.0),
                                    fg,
                                );
                                if open_resp.clicked() && !marquee_active {
                                    intents.push(Intent::Open(row.page.id, None));
                                }
                            }

                            let inner_rect = card_rect.shrink(10.0);
                            let mut card_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(inner_rect)
                                    .layout(egui::Layout::top_down(egui::Align::Min)),
                            );
                            card_ui.set_clip_rect(card_rect.intersect(ui.clip_rect()));
                            card_ui.spacing_mut().item_spacing.y = 4.0;

                            if show_cover {
                                let image_rect = egui::Rect::from_min_size(
                                    card_ui.cursor().min,
                                    egui::vec2(inner_rect.width(), 120.0),
                                );
                                card_ui.allocate_exact_size(
                                    egui::vec2(inner_rect.width(), 120.0),
                                    egui::Sense::hover(),
                                );
                                card_ui.painter().rect_filled(image_rect, 6.0, palette.code);
                                let mut painted = false;
                                if let Some(path) = row.page.cover_image.as_deref() {
                                    let focus = egui::vec2(row.page.cover_x, row.page.cover_y);
                                    painted = crate::ui::editor::media::paint_cover(
                                        &mut card_ui,
                                        std::path::Path::new(path),
                                        image_rect,
                                        focus,
                                    )
                                    .is_some();
                                }
                                if !painted {
                                    let icon = if row.page.icon.is_empty() {
                                        "📄"
                                    } else {
                                        &row.page.icon
                                    };
                                    if let Some(source) = crate::ui::emoji_image_source(icon) {
                                        let icon_rect = egui::Rect::from_center_size(
                                            image_rect.center(),
                                            egui::vec2(26.0, 26.0),
                                        );
                                        egui::Image::new(source).paint_at(&card_ui, icon_rect);
                                    } else {
                                        card_ui.painter().text(
                                            image_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            icon,
                                            FontId::proportional(26.0),
                                            palette.muted,
                                        );
                                    }
                                }
                            }


                            if show_title {
                                let icon = if row.page.icon.is_empty() {
                                    "📄"
                                } else {
                                    &row.page.icon
                                };
                                let title = row.page.display_title();
                                card_ui.horizontal(|ui| {
                                    if let Some(source) = crate::ui::emoji_image_source(icon) {
                                        ui.add(egui::Image::new(source).fit_to_exact_size(egui::vec2(14.0, 14.0)));
                                    } else {
                                        ui.label(RichText::new(icon).font(FontId::proportional(14.0)).color(palette.text));
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(title)
                                                .font(FontId::proportional(14.0))
                                                .color(palette.text)
                                                .strong(),
                                        )
                                        .truncate(),
                                    );
                                });
                            }

                            if show_created_at {
                                card_ui.add(
                                    egui::Label::new(
                                        RichText::new(format_date(row.page.created_at))
                                            .font(FontId::proportional(11.5))
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }

                            if show_description && !row.preview.trim().is_empty() {
                                card_ui.add(
                                    egui::Label::new(
                                        RichText::new(row.preview.trim())
                                            .font(FontId::proportional(12.0))
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }

                            let summaries: Vec<_> = data
                                .properties
                                .iter()
                                .filter(|property| {
                                    visible_properties.as_deref().map_or_else(
                                        || {
                                            data.properties
                                                .iter()
                                                .take(2)
                                                .any(|visible| visible.id == property.id)
                                        },
                                        |visible| visible.contains(&property.id),
                                    )
                                })
                                .filter_map(|property| {
                                    row.values
                                        .get(&property.id)
                                        .filter(|value| !value.is_empty())
                                        .map(|value| {
                                            let display_val = if property.kind == "multi-select" {
                                                value
                                                    .split('|')
                                                    .filter(|s| !s.is_empty())
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            } else {
                                                value.clone()
                                            };
                                            format!("{}: {display_val}", property.name)
                                        })
                                })
                                .collect();
                            if !summaries.is_empty() {
                                card_ui.add(
                                    egui::Label::new(
                                        RichText::new(summaries.join("  ·  "))
                                            .font(FontId::proportional(11.5))
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }
                        }
                    });
                }
            });
    }

    if primary_clicked
        && !is_ctrl
        && !is_shift
        && !marquee_active
        && let Some(pos) = pointer_pos
        && cards_area.contains(pos)
        && pos.y > cards_start_cursor.y + natural_h
    {
        selected_rows.clear();
        anchor_row = None;
    }

    if primary_released {
        marquee_anchor = None;
        marquee_active = false;
        marquee_base.clear();
    }

    if !selected_rows.is_empty() {
        let count = selected_rows.len();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{count} record{} selected", if count == 1 { "" } else { "s" }))
                    .strong()
                    .color(palette.accent),
            );
            if ui.button("Duplicate").clicked() {
                for id in &selected_rows {
                    intents.push(Intent::Action(Action::Duplicate(*id)));
                }
            }
            if ui.button(RichText::new("Delete").color(palette.danger)).clicked() {
                for id in &selected_rows {
                    intents.push(Intent::Action(Action::Trash(*id)));
                }
                selected_rows.clear();
                anchor_row = None;
            }
            if ui.button("Clear selection").clicked() {
                selected_rows.clear();
                anchor_row = None;
            }
        });
    }

    ui.add_space(6.0);
    let new_btn = egui::Button::new(
        RichText::new("+ New record")
            .font(FontId::proportional(13.0))
            .color(palette.muted),
    )
    .fill(palette.sidebar)
    .stroke(egui::Stroke::new(1.0, palette.border.linear_multiply(0.5)))
    .corner_radius(6);
    if ui.add_sized([total_width, 34.0], new_btn).clicked() && !busy {
        intents.push(Intent::Action(Action::CreateDatabaseRecord(data.page_id)));
    }

    if show_vertical_resize_handle(ui, data, total_width, active_h, card_height, palette) {
        intents.push(Intent::Action(Action::SetDatabaseViewSettings {
            page: data.page_id,
            settings: data.view_settings.clone(),
        }));
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(marquee_anchor_id, marquee_anchor);
        d.insert_temp(marquee_active_id, marquee_active);
        d.insert_temp(marquee_base_id, marquee_base);
        d.insert_temp(selection_id, selected_rows);
        d.insert_temp(anchor_id, anchor_row);
    });
}

fn show_board(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let status = data
        .properties
        .iter()
        .find(|property| property.kind == "status" || property.kind == "select")
        .cloned();
    let Some(status) = status else {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("No Status or Select property found to group columns.")
                    .color(palette.muted),
            );
            if ui
                .add_enabled(!busy, egui::Button::new("+ Add Status property"))
                .clicked()
            {
                intents.push(Intent::Action(Action::AddDatabaseProperty {
                    database: data.page_id,
                    name: "Status".into(),
                    kind: "status".into(),
                    options: vec![
                        PropertyOption::new("Todo"),
                        PropertyOption::new("In progress"),
                        PropertyOption::new("Done"),
                    ],
                }));
            }
        });
        ui.add_space(6.0);
        show_cards(ui, data, busy, palette, intents, 1);
        return;
    };

    let mut groups = vec!["No selection".to_owned()];
    for option in &status.options {
        if !groups.iter().any(|group| group == &option.name) {
            groups.push(option.name.clone());
        }
    }
    let sorted = sorted_rows(data);
    let selection_id = egui::Id::new(("database-selected-rows", data.page_id));
    let anchor_id = egui::Id::new(("database-anchor-row", data.page_id));
    let mut selected_rows: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    let mut anchor_row: Option<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(anchor_id))
        .flatten();
    let valid_ids: std::collections::HashSet<_> = data.rows.iter().map(|r| r.page.id).collect();
    selected_rows.retain(|id| valid_ids.contains(id));
    if let Some(anchor) = anchor_row
        && !valid_ids.contains(&anchor)
    {
        anchor_row = None;
    }
    let (is_ctrl, is_shift) =
        ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.modifiers.shift));

    if !selected_rows.is_empty() && !ui.ctx().wants_keyboard_input() {
        let delete_pressed = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        });
        if delete_pressed {
            for id in &selected_rows {
                intents.push(Intent::Action(Action::Trash(*id)));
            }
            selected_rows.clear();
            anchor_row = None;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            selected_rows.clear();
            anchor_row = None;
        }
    }

    let board_visible_properties = data.view_settings.board_visible_properties.clone();
    let show_board_desc = data.view_settings.board_show_description;
    let all_properties = data.properties.clone();

    let mut rows_by_group = HashMap::<String, Vec<DatabaseRow>>::new();
    for row in &sorted {
        let value = row.values.get(&status.id).map(String::as_str).unwrap_or("");
        let group = if value.is_empty() {
            "No selection"
        } else {
            value
        };
        rows_by_group
            .entry(group.to_owned())
            .or_default()
            .push((*row).clone());
        if !groups.iter().any(|candidate| candidate == group) {
            groups.push(group.to_owned());
        }
    }

    let natural_cards_h = board_max_column_cards_height(data, status.id);
    let (active_cards_h, _has_custom_height) = resolve_vertical_height(
        ui,
        data,
        natural_cards_h,
        DEFAULT_BOARD_MAX_CARDS_HEIGHT,
        96.0,
    );

    let board_col_width = 250.0;
    let total_board_w = ((groups.len() + 1) as f32 * (board_col_width + 12.0))
        .min(ui.available_width().max(320.0));

    egui::ScrollArea::horizontal()
        .id_salt(("database-board-hscroll", data.page_id))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for group in &groups {
                    let target_value = if group == "No selection" {
                        String::new()
                    } else {
                        group.clone()
                    };
                    let rows = rows_by_group.get(group).map_or(&[][..], Vec::as_slice);

                    let col_frame = egui::Frame::new()
                        .fill(palette.code)
                        .stroke(egui::Stroke::new(1.0, palette.border.linear_multiply(0.4)))
                        .corner_radius(8)
                        .inner_margin(10);

                    let col_inner = col_frame.show(ui, |ui| {
                        ui.set_min_width(board_col_width);
                        ui.set_max_width(board_col_width);

                        let mut card_dropped = false;

                        ui.vertical(|ui| {
                            ui.set_min_width(board_col_width);
                            // Column Header
                            ui.horizontal(|ui| {
                                let (pill_fill, pill_text_color) = if group == "No selection" {
                                    (palette.hover, palette.muted)
                                } else {
                                    let opt_col = resolve_option_color(status.option_color(group));
                                    let (bg, _border, text) =
                                        option_pill_colors(palette, "status", group, opt_col);
                                    (bg, text)
                                };
                                let pill_frame = egui::Frame::new()
                                    .fill(pill_fill)
                                    .corner_radius(4)
                                    .inner_margin(egui::Margin::symmetric(8, 3));
                                pill_frame.show(ui, |ui| {
                                    ui.label(
                                        RichText::new(group)
                                            .font(FontId::proportional(12.5))
                                            .color(pill_text_color)
                                            .strong(),
                                    );
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(rows.len().to_string())
                                        .font(FontId::proportional(11.5))
                                        .color(palette.muted),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_enabled(
                                                !busy,
                                                egui::Button::new(
                                                    RichText::new("+")
                                                        .font(FontId::proportional(14.0))
                                                        .color(palette.muted),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_text("Add card to this column")
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(
                                                Action::CreateDatabaseRecordWithProperty {
                                                    database: data.page_id,
                                                    property: status.id,
                                                    value: target_value.clone(),
                                                },
                                            ));
                                        }
                                    },
                                );
                            });

                            ui.add_space(8.0);

                            // Cards vertical list
                            egui::ScrollArea::vertical()
                                .id_salt(("database-board-column", data.page_id, group))
                                .max_height(active_cards_h)
                                .min_scrolled_height(active_cards_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_min_width(board_col_width - 4.0);
                                    ui.set_min_height(active_cards_h);
                                    ui.spacing_mut().item_spacing.y = 8.0;
                                    for row in rows {
                                        let properties: Vec<_> = all_properties
                                            .iter()
                                            .filter(|property| property.id != status.id)
                                            .filter(|property| {
                                                board_visible_properties.as_deref().map_or_else(
                                                    || {
                                                        all_properties
                                                            .iter()
                                                            .filter(|candidate| {
                                                                candidate.id != status.id
                                                            })
                                                            .take(2)
                                                            .any(|visible| {
                                                                visible.id == property.id
                                                            })
                                                    },
                                                    |visible| visible.contains(&property.id),
                                                )
                                            })
                                            .filter_map(|property| {
                                                row.values
                                                    .get(&property.id)
                                                    .filter(|value| !value.is_empty())
                                                    .map(|value| {
                                                        let display_val =
                                                            if property.kind == "multi-select" {
                                                                value
                                                                    .split('|')
                                                                    .filter(|s| !s.is_empty())
                                                                    .collect::<Vec<_>>()
                                                                    .join(", ")
                                                            } else {
                                                                value.clone()
                                                            };
                                                        format!("{}: {display_val}", property.name)
                                                    })
                                            })
                                            .collect();

                                        let show_desc =
                                            show_board_desc && !row.preview.trim().is_empty();
                                        let card_h = 36.0
                                            + if !properties.is_empty() { 20.0 } else { 0.0 }
                                            + if show_desc { 22.0 } else { 0.0 };

                                        let (card_rect, card_resp) = ui.allocate_exact_size(
                                            egui::vec2(board_col_width - 4.0, card_h),
                                            egui::Sense::click_and_drag(),
                                        );
                                        card_resp.dnd_set_drag_payload(BoardDragPayload {
                                            database_id: data.page_id,
                                            record_id: row.page.id,
                                        });

                                        let is_selected = selected_rows.contains(&row.page.id);
                                        let is_hovered = card_resp.hovered();
                                        let card_bg = if is_selected {
                                            palette.accent_soft
                                        } else if is_hovered {
                                            palette.hover
                                        } else {
                                            palette.board_card
                                        };
                                        let stroke_col = if is_selected {
                                            palette.accent
                                        } else if is_hovered {
                                            palette.accent.linear_multiply(0.5)
                                        } else {
                                            palette.border.linear_multiply(0.6)
                                        };
                                        ui.painter().rect_filled(card_rect, 6.0, card_bg);
                                        ui.painter().rect_stroke(
                                            card_rect,
                                            6.0,
                                            egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, stroke_col),
                                            egui::StrokeKind::Inside,
                                        );

                                        if card_resp.dragged()
                                            && let Some(pointer) = ui.ctx().pointer_interact_pos()
                                        {
                                            let ghost_rect = egui::Rect::from_min_size(
                                                pointer + egui::vec2(10.0, 10.0),
                                                egui::vec2(200.0, 32.0),
                                            );
                                            let layer = ui.ctx().layer_painter(egui::LayerId::new(
                                                egui::Order::Tooltip,
                                                ui.id().with(("board-drag-ghost", row.page.id)),
                                            ));
                                            layer.rect_filled(ghost_rect, 6.0, palette.sidebar);
                                            layer.rect_stroke(
                                                ghost_rect,
                                                6.0,
                                                egui::Stroke::new(1.5, palette.accent),
                                                egui::StrokeKind::Inside,
                                            );
                                            layer.text(
                                                ghost_rect.left_center() + egui::vec2(10.0, 0.0),
                                                egui::Align2::LEFT_CENTER,
                                                row.page.display_title(),
                                                FontId::proportional(12.5),
                                                palette.text,
                                            );
                                        }

                                        if card_resp.clicked() && !busy {
                                            if is_ctrl {
                                                if selected_rows.contains(&row.page.id) {
                                                    selected_rows.remove(&row.page.id);
                                                } else {
                                                    selected_rows.insert(row.page.id);
                                                    anchor_row = Some(row.page.id);
                                                }
                                            } else if is_shift {
                                                let anchor = anchor_row.unwrap_or(row.page.id);
                                                let p1 = sorted.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                                                let p2 = sorted.iter().position(|r| r.page.id == row.page.id).unwrap_or(0);
                                                let start = p1.min(p2);
                                                let end = p1.max(p2);
                                                selected_rows.clear();
                                                for r in &sorted[start..=end] {
                                                    selected_rows.insert(r.page.id);
                                                }
                                            } else {
                                                selected_rows.clear();
                                                selected_rows.insert(row.page.id);
                                                anchor_row = Some(row.page.id);
                                                intents.push(Intent::Open(row.page.id, None));
                                            }
                                        }

                                        // Check DnD hover / release on this card
                                        if let Some(dragged) =
                                            card_resp.dnd_hover_payload::<BoardDragPayload>()
                                            && dragged.database_id == data.page_id
                                            && dragged.record_id != row.page.id
                                        {
                                            let pointer_y = ui
                                                .ctx()
                                                .pointer_interact_pos()
                                                .map(|p| p.y)
                                                .unwrap_or(card_rect.center().y);
                                            let after = pointer_y >= card_rect.center().y;
                                            let line_y = if after {
                                                card_rect.bottom() + 3.0
                                            } else {
                                                card_rect.top() - 3.0
                                            };
                                            ui.painter().line_segment(
                                                [
                                                    egui::pos2(card_rect.left() + 4.0, line_y),
                                                    egui::pos2(card_rect.right() - 4.0, line_y),
                                                ],
                                                egui::Stroke::new(2.5, palette.accent),
                                            );
                                            if let Some(released) =
                                                card_resp.dnd_release_payload::<BoardDragPayload>()
                                            {
                                                intents.push(Intent::Action(
                                                    Action::MoveDatabaseRecord {
                                                        record: released.record_id,
                                                        property: status.id,
                                                        value: target_value.clone(),
                                                        target: Some((row.page.id, after)),
                                                    },
                                                ));
                                                card_dropped = true;
                                            }
                                        }

                                        card_resp.context_menu(|ui| {
                                            if ui.button("Open page").clicked() {
                                                intents.push(Intent::Open(row.page.id, None));
                                                ui.close_menu();
                                            }
                                            if ui.button("🗖  Diyalog olarak aç").clicked() {
                                                intents.push(Intent::OpenInDialog(row.page.id));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("🎨 Temayı düzenle").clicked() {
                                                intents.push(Intent::OpenSettingsTheme(ThemeSection::Board));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            ui.label(
                                                RichText::new("Move to")
                                                    .small()
                                                    .color(palette.muted),
                                            );
                                            let current = row
                                                .values
                                                .get(&status.id)
                                                .cloned()
                                                .unwrap_or_default();
                                            if ui
                                                .selectable_label(
                                                    current.is_empty(),
                                                    "No selection",
                                                )
                                                .clicked()
                                                && !current.is_empty()
                                            {
                                                intents.push(Intent::Action(
                                                    Action::SetDatabaseProperty {
                                                        page: row.page.id,
                                                        property: status.id,
                                                        value: String::new(),
                                                    },
                                                ));
                                                ui.close_menu();
                                            }
                                            for option in &status.options {
                                                if ui
                                                    .selectable_label(current == option.name, &option.name)
                                                    .clicked()
                                                    && current != option.name
                                                {
                                                    intents.push(Intent::Action(
                                                        Action::SetDatabaseProperty {
                                                            page: row.page.id,
                                                            property: status.id,
                                                            value: option.name.clone(),
                                                        },
                                                    ));
                                                    ui.close_menu();
                                                }
                                            }
                                        });

                                        let inner = card_rect.shrink2(egui::vec2(10.0, 8.0));
                                        let mut card_ui = ui.new_child(
                                            egui::UiBuilder::new()
                                                .max_rect(inner)
                                                .layout(egui::Layout::top_down(egui::Align::Min)),
                                        );
                                        card_ui.set_clip_rect(card_rect.intersect(ui.clip_rect()));
                                        card_ui.spacing_mut().item_spacing.y = 3.0;

                                        let icon = if row.page.icon.is_empty() {
                                            "📄"
                                        } else {
                                            &row.page.icon
                                        };
                                        card_ui.horizontal(|ui| {
                                            if let Some(source) = crate::ui::emoji_image_source(icon) {
                                                ui.add(egui::Image::new(source).fit_to_exact_size(egui::vec2(13.0, 13.0)));
                                            } else {
                                                ui.label(RichText::new(icon).font(FontId::proportional(13.0)).color(palette.text));
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(row.page.display_title())
                                                        .font(FontId::proportional(13.0))
                                                        .color(palette.text)
                                                        .strong(),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        if !properties.is_empty() {
                                            card_ui.add(
                                                egui::Label::new(
                                                    RichText::new(properties.join("  ·  "))
                                                        .font(FontId::proportional(11.5))
                                                        .color(palette.muted),
                                                )
                                                .truncate(),
                                            );
                                        }
                                        if show_desc {
                                            card_ui.add(
                                                egui::Label::new(
                                                    RichText::new(row.preview.trim())
                                                        .font(FontId::proportional(11.5))
                                                        .color(palette.muted),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    }
                                });

                            ui.add_space(6.0);
                            let add_card_btn = egui::Button::new(
                                RichText::new("+ New")
                                    .font(FontId::proportional(12.5))
                                    .color(palette.muted),
                            )
                            .frame(false);
                            if ui
                                .add_sized([board_col_width - 8.0, 26.0], add_card_btn)
                                .clicked()
                                && !busy
                            {
                                intents.push(Intent::Action(
                                    Action::CreateDatabaseRecordWithProperty {
                                        database: data.page_id,
                                        property: status.id,
                                        value: target_value.clone(),
                                    },
                                ));
                            }
                        });

                        card_dropped
                    });

                    // Handle dropping on the column background/empty space
                    if !col_inner.inner {
                        let col_drop_resp = ui.interact(
                            col_inner.response.rect,
                            ui.id().with(("board-col-drop", data.page_id, group)),
                            egui::Sense::hover(),
                        );
                        if let Some(hovered_drag) =
                            col_drop_resp.dnd_hover_payload::<BoardDragPayload>()
                            && hovered_drag.database_id == data.page_id
                        {
                            ui.painter().rect_stroke(
                                col_inner.response.rect,
                                8.0,
                                egui::Stroke::new(2.0, palette.accent),
                                egui::StrokeKind::Inside,
                            );
                            if let Some(released) =
                                col_drop_resp.dnd_release_payload::<BoardDragPayload>()
                            {
                                let last_in_col = rows.last().map(|r| (r.page.id, true));
                                intents.push(Intent::Action(Action::MoveDatabaseRecord {
                                    record: released.record_id,
                                    property: status.id,
                                    value: target_value,
                                    target: last_in_col,
                                }));
                            }
                        }
                    }
                }

                // "+ Add group" column button on the right of the board
                ui.menu_button(
                    RichText::new("+ Group")
                        .font(FontId::proportional(12.5))
                        .color(palette.muted),
                    |ui| {
                        let new_grp_id = egui::Id::new(("board-new-group", data.page_id, status.id));
                        let mut grp_name = ui
                            .ctx()
                            .data(|d| d.get_temp::<String>(new_grp_id))
                            .unwrap_or_default();
                        ui.label(RichText::new("New group name").small().color(palette.muted));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut grp_name)
                                .hint_text("e.g. Review")
                                .desired_width(150.0),
                        );
                        if resp.changed() {
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(new_grp_id, grp_name.clone()));
                        }
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (enter || ui.button("+ Add group").clicked())
                            && !grp_name.trim().is_empty()
                        {
                            let trimmed = grp_name.trim().to_owned();
                            let mut opts = status.options.clone();
                            if !opts.iter().any(|o| o.name == trimmed) {
                                opts.push(PropertyOption::new(&trimmed));
                                intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                    property: status.id,
                                    options: opts,
                                }));
                            }
                            ui.ctx().data_mut(|d| d.remove::<String>(new_grp_id));
                            ui.close_menu();
                        }
                    },
                );
            });
        });

    if show_vertical_resize_handle(ui, data, total_board_w, active_cards_h, 96.0, palette) {
        intents.push(Intent::Action(Action::SetDatabaseViewSettings {
            page: data.page_id,
            settings: data.view_settings.clone(),
        }));
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(selection_id, selected_rows);
        d.insert_temp(anchor_id, anchor_row);
    });
}

pub(crate) fn format_date(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "—".to_owned();
    }
    let Some(date_time) =
        chrono::DateTime::from_timestamp(timestamp, 0).map(|utc| utc.with_timezone(&chrono::Local))
    else {
        return "—".to_owned();
    };
    format_date_at(date_time, chrono::Local::now().date_naive())
}

fn format_date_at(date_time: chrono::DateTime<chrono::Local>, today: chrono::NaiveDate) -> String {
    use chrono::{Datelike, Timelike};

    let month = match date_time.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => unreachable!(),
    };

    if date_time.date_naive() == today {
        format!(
            "{} {month}, {:02}:{:02}",
            date_time.day(),
            date_time.hour(),
            date_time.minute()
        )
    } else if date_time.year() == today.year() {
        format!("{} {month}", date_time.day())
    } else {
        format!("{} {month} {}", date_time.day(), date_time.year())
    }
}

#[cfg(test)]
mod date_format_tests {
    use super::{format_date, format_date_at};
    use chrono::{Local, NaiveDate, TimeZone};

    #[test]
    fn zero_or_negative_timestamp_returns_dash() {
        assert_eq!(format_date(0), "—");
        assert_eq!(format_date(-1), "—");
    }

    #[test]
    fn date_format_hides_year_for_this_year_and_time_for_other_days() {
        let today = NaiveDate::from_ymd_opt(2025, 10, 15).unwrap();
        let yesterday = Local.with_ymd_and_hms(2025, 10, 14, 17, 42, 31).unwrap();
        let previous_year = Local.with_ymd_and_hms(2024, 10, 15, 17, 42, 31).unwrap();

        assert_eq!(format_date_at(yesterday, today), "14 October");
        assert_eq!(format_date_at(previous_year, today), "15 October 2024");
    }

    #[test]
    fn date_format_shows_time_only_for_today() {
        let today = NaiveDate::from_ymd_opt(2025, 10, 15).unwrap();
        let same_day = Local.with_ymd_and_hms(2025, 10, 15, 9, 7, 53).unwrap();

        assert_eq!(format_date_at(same_day, today), "15 October, 09:07");
    }
}
