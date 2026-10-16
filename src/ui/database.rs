use std::collections::HashMap;

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{
    DatabaseData, DatabaseProperty, DatabaseRow, DatabaseView, PROPERTY_TYPES,
};
use crate::worker::Action;

use super::{Intent, palette::Palette};

const TABLE_NAME_WIDTH: f32 = 250.0;
const TABLE_CREATED_WIDTH: f32 = 135.0;
const TABLE_PROPERTY_WIDTH: f32 = 180.0;
const TABLE_ADD_PROP_WIDTH: f32 = 42.0;
const TABLE_HEADER_HEIGHT: f32 = 34.0;
const TABLE_ROW_HEIGHT: f32 = 36.0;

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
    parent_title: Option<&str>,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    show_inner(
        ui,
        data,
        page_icon,
        parent_title,
        busy,
        palette,
        intents,
        drafts,
        true,
    );
}

pub fn show_embedded(
    ui: &mut Ui,
    data: &mut DatabaseData,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    show_inner(ui, data, None, None, false, palette, intents, drafts, false);
}

fn show_inner(
    ui: &mut Ui,
    data: &mut DatabaseData,
    page_icon: Option<&str>,
    parent_title: Option<&str>,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
    full_page: bool,
) {
    ui.vertical(|ui| {
        if full_page {
            ui.add_space(20.0);
            if let Some(parent) = parent_title {
                ui.horizontal(|ui| {
                    ui.add_space(44.0);
                    ui.label(
                        RichText::new(format!("{parent}  /  {}", data.title))
                            .font(FontId::proportional(12.0))
                            .color(palette.muted),
                    );
                });
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.add_space(44.0);
                let icon = page_icon.unwrap_or("🗃️");
                ui.label(RichText::new(icon).size(34.0));
                ui.add_space(10.0);
                let previous_title = data.title.clone();
                let title_response = ui.add_enabled(
                    !busy,
                    egui::TextEdit::singleline(&mut data.title)
                        .font(FontId::proportional(30.0))
                        .hint_text("Untitled Database")
                        .frame(false)
                        .margin(egui::Margin::ZERO)
                        .desired_width(ui.available_width() - 88.0),
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
                ui.label(RichText::new("🗃️").size(16.0));
                ui.label(
                    RichText::new(&data.title)
                        .font(FontId::proportional(16.0))
                        .strong(),
                );
            });
            ui.add_space(6.0);
        }

        ui.horizontal(|ui| {
            if full_page {
                ui.add_space(44.0);
            }
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

                    if data.view == DatabaseView::Gallery {
                        egui::ComboBox::from_id_salt(("gallery-columns", data.page_id))
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
                                        intents.push(Intent::Action(Action::SetDatabaseGalleryColumns {
                                            page: data.page_id,
                                            columns,
                                        }));
                                    }
                                }
                            });
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
            if full_page {
                ui.add_space(44.0);
            }
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
    ui.menu_button("View options", |ui| match view {
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
}

pub fn show_record_properties(
    ui: &mut Ui,
    data: &DatabaseData,
    target: RecordPropertiesTarget,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    value_drafts: &mut std::collections::HashMap<(i64, i64), String>,
) {
    let RecordPropertiesTarget { page_id, indent } = target;
    let Some(row) = data.rows.iter().find(|row| row.page.id == page_id) else {
        return;
    };
    if data.properties.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.vertical(|ui| {
            ui.set_max_width(440.0);
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
            ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
            ui.label(
                RichText::new("PROPERTIES")
                    .small()
                    .strong()
                    .color(palette.muted),
            );
            for property in &data.properties {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_sized([105.0, 20.0], egui::Label::new(&property.name));
                    property_editor(ui, row, property, 260.0, busy, palette, intents, value_drafts);
                });
            }
        });
    });
    ui.add_space(3.0);
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
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, TABLE_ROW_HEIGHT, rows.len(), |ui, visible| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    for row in &rows[visible] {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                            table_cell(
                                ui,
                                name_w,
                                TABLE_ROW_HEIGHT,
                                palette,
                                false,
                                true,
                                |ui, hovered| {
                                    let icon = if row.page.icon.is_empty() {
                                        "📄"
                                    } else {
                                        &row.page.icon
                                    };
                                    ui.label(RichText::new(icon).size(13.0));

                                    let title = drafts
                                        .titles
                                        .entry(row.page.id)
                                        .or_insert_with(|| row.page.title.clone());
                                    let width_avail = (if hovered {
                                        name_w - 64.0
                                    } else {
                                        name_w - 42.0
                                    })
                                    .max(24.0);
                                    let response = ui.add_enabled(
                                        !busy,
                                        egui::TextEdit::singleline(title)
                                            .hint_text("Untitled")
                                            .desired_width(width_avail)
                                            .font(FontId::proportional(13.0))
                                            .frame(false)
                                            .margin(egui::Margin::ZERO),
                                    );
                                    if response.lost_focus() && title != &row.page.title {
                                        intents.push(Intent::Action(Action::RenameDatabaseRecord {
                                            page: row.page.id,
                                            title: title.clone(),
                                        }));
                                    }
                                    if response.double_clicked() {
                                        intents.push(Intent::Open(row.page.id, None));
                                    }
                                    if hovered {
                                        let open_btn = egui::Button::new(
                                            RichText::new("↗")
                                                .color(palette.muted)
                                                .font(FontId::proportional(12.0)),
                                        )
                                        .frame(false);
                                        if ui.add(open_btn).on_hover_text("Open page").clicked() {
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
                                    |ui, _| {
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
                        |_ui, _| {},
                    );
                }
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
            let mut settings_changed = false;
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
    add_contents: impl FnOnce(&mut Ui, bool),
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let hovered = response.hovered();
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
    let hovered = response.hovered();
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

fn show_property_edit_menu(
    ui: &mut Ui,
    property: &DatabaseProperty,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    ui.set_min_width(230.0);
    let draft = drafts
        .properties
        .entry(property.id)
        .or_insert_with(|| (property.name.clone(), String::new()));

    ui.label(RichText::new("Property name").small().color(palette.muted));
    let name_resp = ui.add_enabled(
        !busy,
        egui::TextEdit::singleline(&mut draft.0).desired_width(210.0),
    );
    let enter_pressed = name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if (name_resp.lost_focus() || enter_pressed)
        && !draft.0.trim().is_empty()
        && draft.0.trim() != property.name
    {
        intents.push(Intent::Action(Action::RenameDatabaseProperty {
            property: property.id,
            name: draft.0.trim().to_owned(),
        }));
    }

    ui.add_space(6.0);
    ui.label(RichText::new("Type").small().color(palette.muted));
    egui::ComboBox::from_id_salt(("property-kind", property.id))
        .width(210.0)
        .selected_text(property_types_label(&property.kind))
        .show_ui(ui, |ui| {
            for (kind, label) in PROPERTY_TYPES {
                if ui.selectable_label(property.kind == kind, label).clicked()
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
                        let opt_draft_id =
                            egui::Id::new(("prop-option-edit", property.id, opt_idx, option));
                        let mut opt_text = ui
                            .ctx()
                            .data(|d| d.get_temp::<String>(opt_draft_id))
                            .unwrap_or_else(|| option.clone());
                        ui.horizontal(|ui| {
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
                            if commit && !opt_text.trim().is_empty() && opt_text.trim() != option {
                                intents.push(Intent::Action(
                                    Action::RenameDatabasePropertyOption {
                                        property: property.id,
                                        old_option: option.clone(),
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
                                        option: option.clone(),
                                    },
                                ));
                            }
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
                    if !updated.iter().any(|existing| existing == part) {
                        updated.push(part.to_owned());
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
    if ui
        .button(RichText::new("Delete property").color(palette.danger))
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
            let display_text = if value.is_empty() { "—" } else { &value };
            let text_color = if value.is_empty() {
                palette.muted
            } else {
                palette.text
            };
            let pill_fill = if value.is_empty() {
                Color32::TRANSPARENT
            } else {
                palette.accent_soft
            };
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = if value.is_empty() {
                    egui::Stroke::NONE
                } else {
                    egui::Stroke::new(1.0, palette.accent.linear_multiply(0.3))
                };
                ui.menu_button(
                    RichText::new(display_text)
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                    |ui| {
                        ui.set_min_width(170.0);
                        if ui
                            .selectable_label(value.is_empty(), "None")
                            .clicked()
                            && !value.is_empty()
                        {
                            intents.push(action(String::new()));
                            ui.close_menu();
                        }
                        for option in &property.options {
                            if ui.selectable_label(value == *option, option).clicked()
                                && value != *option
                            {
                                intents.push(action(option.clone()));
                                ui.close_menu();
                            }
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
                                if !opts.contains(&trimmed) {
                                    opts.push(trimmed.clone());
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
                .add_enabled(!busy, egui::Checkbox::without_text(&mut checked))
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
            let text_color = if selected.is_empty() {
                palette.muted
            } else {
                palette.text
            };
            let pill_fill = if selected.is_empty() {
                Color32::TRANSPARENT
            } else {
                palette.accent_soft
            };
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = if selected.is_empty() {
                    egui::Stroke::NONE
                } else {
                    egui::Stroke::new(1.0, palette.accent.linear_multiply(0.3))
                };
                ui.menu_button(
                    RichText::new(selected_label)
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                    |ui| {
                        ui.set_min_width(175.0);
                        if property.options.is_empty() {
                            ui.label(
                                RichText::new("No options yet.")
                                    .small()
                                    .italics()
                                    .color(palette.muted),
                            );
                        }
                        for option in &property.options {
                            let mut enabled = selected.contains(option);
                            if ui.checkbox(&mut enabled, option).changed() {
                                let mut next = selected.clone();
                                if enabled && !next.contains(option) {
                                    next.push(option.clone());
                                } else if !enabled {
                                    next.retain(|item| item != option);
                                }
                                intents.push(action(next.join("|")));
                            }
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
                                if !opts.contains(&trimmed) {
                                    opts.push(trimmed.clone());
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
                    .hint_text(if property.kind == "date" {
                        "YYYY-MM-DD"
                    } else {
                        ""
                    }),
            );
            if text.lost_focus() && edited != &value {
                intents.push(action(edited.clone()));
            }
        }
    }
}

fn add_property_menu(ui: &mut Ui, data: &DatabaseData, intents: &mut Vec<Intent>) {
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
    data: &DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    show_cards(
        ui,
        data,
        busy,
        palette,
        intents,
        usize::from(data.gallery_columns),
    );
}

fn show_cards(
    ui: &mut Ui,
    data: &DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    columns: usize,
) {
    let rows = sorted_rows(data);
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
    let card_height: f32 = (cover_h + title_h + created_h + desc_h + prop_h + 24.0).max(88.0);
    let row_gap = 12.0;
    let col_gap = 12.0;
    let row_height = card_height + row_gap;
    let total_width = (ui.available_width() - 12.0).max(160.0);
    let card_width =
        ((total_width - col_gap * columns.saturating_sub(1) as f32) / columns as f32).max(110.0);

    let scroll_max_h = ui.available_height().max(240.0);
    egui::ScrollArea::vertical()
        .id_salt(("database-gallery-scroll", data.page_id))
        .max_height(scroll_max_h)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, row_count + 1, |ui, visible| {
            for row_index in visible {
                if row_index == row_count {
                    ui.add_space(4.0);
                    let new_btn = egui::Button::new(
                        RichText::new("+ New record")
                            .font(FontId::proportional(13.0))
                            .color(palette.muted),
                    )
                    .fill(palette.sidebar)
                    .stroke(egui::Stroke::new(1.0, palette.border.linear_multiply(0.5)))
                    .corner_radius(6);
                    if ui
                        .add_sized([total_width, 36.0], new_btn)
                        .clicked()
                        && !busy
                    {
                        intents.push(Intent::Action(Action::CreateDatabaseRecord(data.page_id)));
                    }
                    continue;
                }

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = col_gap;
                    for row in rows.iter().skip(row_index * columns).take(columns) {
                        let (card_rect, card_resp) = ui.allocate_exact_size(
                            egui::vec2(card_width, card_height),
                            egui::Sense::click(),
                        );
                        let hovered = card_resp.hovered();
                        let bg = if hovered {
                            palette.hover
                        } else {
                            palette.sidebar
                        };
                        let border_color = if hovered {
                            palette.accent.linear_multiply(0.6)
                        } else {
                            palette.border.linear_multiply(0.55)
                        };
                        ui.painter().rect_filled(card_rect, 8.0, bg);
                        ui.painter().rect_stroke(
                            card_rect,
                            8.0,
                            egui::Stroke::new(1.0, border_color),
                            egui::StrokeKind::Inside,
                        );

                        if card_resp.clicked() && !busy {
                            intents.push(Intent::Open(row.page.id, None));
                        }
                        card_resp.on_hover_cursor(egui::CursorIcon::PointingHand);

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
                            if let Some(path) = row.page.cover_image.as_deref() {
                                let focus = egui::vec2(row.page.cover_x, row.page.cover_y);
                                crate::ui::editor::media::paint_cover(
                                    &mut card_ui,
                                    std::path::Path::new(path),
                                    image_rect,
                                    focus,
                                );
                            } else {
                                card_ui.painter().rect_filled(image_rect, 6.0, palette.code);
                                let icon = if row.page.icon.is_empty() {
                                    "📄"
                                } else {
                                    &row.page.icon
                                };
                                card_ui.painter().text(
                                    image_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    icon,
                                    FontId::proportional(26.0),
                                    palette.muted,
                                );
                            }
                        }

                        if show_title {
                            let icon = if row.page.icon.is_empty() {
                                "📄"
                            } else {
                                &row.page.icon
                            };
                            let title = row.page.display_title();
                            card_ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{icon}  {title}"))
                                        .font(FontId::proportional(14.0))
                                        .color(palette.text)
                                        .strong(),
                                )
                                .truncate(),
                            );
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
                                visible_properties.map_or_else(
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
                ui.add_space(row_gap);
            }
        });
}

fn show_board(
    ui: &mut Ui,
    data: &DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let status = data
        .properties
        .iter()
        .find(|property| property.kind == "status" || property.kind == "select");
    let Some(status) = status else {
        ui.label("Add a Status or Select property to use the board view.");
        show_cards(ui, data, busy, palette, intents, 1);
        return;
    };
    let mut groups = vec!["No selection".to_owned()];
    for option in &status.options {
        if !groups.iter().any(|group| group == option) {
            groups.push(option.clone());
        }
    }
    let sorted = sorted_rows(data);
    let board_visible_properties = data.view_settings.board_visible_properties.as_deref();
    let mut rows_by_group = HashMap::<String, Vec<&DatabaseRow>>::new();
    for row in &sorted {
        let value = row.values.get(&status.id).map(String::as_str).unwrap_or("");
        let group = if value.is_empty() {
            "No selection"
        } else {
            value
        };
        rows_by_group.entry(group.to_owned()).or_default().push(row);
        if !groups.iter().any(|candidate| candidate == group) {
            groups.push(group.to_owned());
        }
    }

    let board_col_width = 250.0;
    let board_height = ui.available_height().max(320.0);

    egui::ScrollArea::horizontal()
        .id_salt(("database-board-hscroll", data.page_id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for group in groups {
                    let target_value = if group == "No selection" {
                        String::new()
                    } else {
                        group.clone()
                    };
                    let rows = rows_by_group.get(&group).map_or(&[][..], Vec::as_slice);

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
                            // Column Header
                            ui.horizontal(|ui| {
                                let pill_fill = if group == "No selection" {
                                    palette.hover
                                } else {
                                    palette.accent_soft
                                };
                                let pill_frame = egui::Frame::new()
                                    .fill(pill_fill)
                                    .corner_radius(4)
                                    .inner_margin(egui::Margin::symmetric(8, 3));
                                pill_frame.show(ui, |ui| {
                                    ui.label(
                                        RichText::new(&group)
                                            .font(FontId::proportional(12.5))
                                            .color(if group == "No selection" {
                                                palette.muted
                                            } else {
                                                palette.text
                                            })
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
                                .id_salt(("database-board-column", data.page_id, &group))
                                .max_height((board_height - 90.0).max(160.0))
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 8.0;
                                    for row in rows {
                                        let properties: Vec<_> = data
                                            .properties
                                            .iter()
                                            .filter(|property| property.id != status.id)
                                            .filter(|property| {
                                                board_visible_properties.map_or_else(
                                                    || {
                                                        data.properties
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

                                        let show_desc = data.view_settings.board_show_description
                                            && !row.preview.trim().is_empty();
                                        let card_h = 36.0
                                            + if !properties.is_empty() { 20.0 } else { 0.0 }
                                            + if show_desc { 22.0 } else { 0.0 };

                                        let (card_rect, card_resp) = ui.allocate_exact_size(
                                            egui::vec2(board_col_width - 4.0, card_h),
                                            egui::Sense::click_and_drag(),
                                        );
                                        card_resp.dnd_set_drag_payload(row.page.id);

                                        let is_hovered = card_resp.hovered();
                                        let card_bg = if is_hovered {
                                            palette.hover
                                        } else {
                                            palette.sidebar
                                        };
                                        let stroke_col = if is_hovered {
                                            palette.accent.linear_multiply(0.5)
                                        } else {
                                            palette.border.linear_multiply(0.6)
                                        };
                                        ui.painter().rect_filled(card_rect, 6.0, card_bg);
                                        ui.painter().rect_stroke(
                                            card_rect,
                                            6.0,
                                            egui::Stroke::new(1.0, stroke_col),
                                            egui::StrokeKind::Inside,
                                        );

                                        if card_resp.clicked() && !busy {
                                            intents.push(Intent::Open(row.page.id, None));
                                        }

                                        // Check DnD hover / release on this card
                                        if let Some(dragged_id) =
                                            card_resp.dnd_hover_payload::<crate::domain::model::PageId>()
                                            && *dragged_id != row.page.id
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
                                            if let Some(released_id) = card_resp
                                                .dnd_release_payload::<crate::domain::model::PageId>()
                                            {
                                                intents.push(Intent::Action(
                                                    Action::MoveDatabaseRecord {
                                                        record: *released_id,
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
                                                    .selectable_label(current == *option, option)
                                                    .clicked()
                                                    && current != *option
                                                {
                                                    intents.push(Intent::Action(
                                                        Action::SetDatabaseProperty {
                                                            page: row.page.id,
                                                            property: status.id,
                                                            value: option.clone(),
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
                                        card_ui.add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{icon}  {}",
                                                    row.page.display_title()
                                                ))
                                                .font(FontId::proportional(13.0))
                                                .color(palette.text)
                                                .strong(),
                                            )
                                            .truncate(),
                                        );
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
                            ui.id().with(("board-col-drop", data.page_id, &group)),
                            egui::Sense::hover(),
                        );
                        if col_drop_resp
                            .dnd_hover_payload::<crate::domain::model::PageId>()
                            .is_some()
                        {
                            ui.painter().rect_stroke(
                                col_inner.response.rect,
                                8.0,
                                egui::Stroke::new(2.0, palette.accent),
                                egui::StrokeKind::Inside,
                            );
                            if let Some(released_id) = col_drop_resp
                                .dnd_release_payload::<crate::domain::model::PageId>()
                            {
                                let last_in_col = rows.last().map(|r| (r.page.id, true));
                                intents.push(Intent::Action(Action::MoveDatabaseRecord {
                                    record: *released_id,
                                    property: status.id,
                                    value: target_value,
                                    target: last_in_col,
                                }));
                            }
                        }
                    }
                }
            });
        });
}

pub(crate) fn format_date(timestamp: i64) -> String {
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
    use super::format_date_at;
    use chrono::{Local, NaiveDate, TimeZone};

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
