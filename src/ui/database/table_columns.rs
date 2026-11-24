use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseProperty, DatabaseRow};
use crate::ui::page_properties::property_icon;
use crate::worker::Action;

use crate::ui::{Intent, palette::Palette};

use super::property_menu::show_property_edit_menu;
use super::property_value::PropertyEditorStyle;
use super::shared::{EMPTY_RECORD_NAME, format_date};
use super::{Drafts, toggle_sort};

pub(super) const TABLE_NAME_WIDTH: f32 = 250.0;

pub(super) const TABLE_CREATED_WIDTH: f32 = 135.0;
pub(super) const TABLE_PROPERTY_WIDTH: f32 = 180.0;
pub(super) const TABLE_ADD_PROP_WIDTH: f32 = 42.0;
pub(super) const TABLE_HEADER_HEIGHT: f32 = 34.0;
pub(super) const TABLE_ROW_HEIGHT: f32 = 36.0;
pub(super) const TABLE_RECORD_ICON_WIDTH: f32 = 24.0;
pub(super) const TABLE_OPEN_PAGE_BUTTON_SIZE: egui::Vec2 = egui::vec2(36.0, 24.0);
pub(super) const TABLE_OPEN_PAGE_BUTTON_MARGIN: f32 = 6.0;
pub(super) const TABLE_NAME_TITLE_GAP: f32 = 4.0;
pub(super) const TABLE_NAME_MIN_WIDTH: f32 = 120.0;
pub(super) const DEFAULT_TABLE_MAX_ROWS_HEIGHT: f32 = 648.0;

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

pub(super) fn auto_fit_name_column(ui: &Ui, rows: &[&DatabaseRow]) -> f32 {
    let header_w = measure_text_width(ui, "Aa  Name ▼", FontId::proportional(12.5)) + 28.0;
    let max_row_w = rows
        .iter()
        .map(|row| {
            let title = if row.page.title.is_empty() {
                EMPTY_RECORD_NAME
            } else {
                &row.page.title
            };
            measure_text_width(ui, title, FontId::proportional(13.0))
                + 58.0
                + TABLE_NAME_TITLE_GAP
                + TABLE_OPEN_PAGE_BUTTON_SIZE.x
                + TABLE_OPEN_PAGE_BUTTON_MARGIN
        })
        .fold(0.0_f32, f32::max);
    header_w.max(max_row_w).clamp(TABLE_NAME_MIN_WIDTH, 600.0)
}

pub(super) fn auto_fit_created_column(ui: &Ui, rows: &[&DatabaseRow]) -> f32 {
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

pub(super) fn auto_fit_property_column(
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

pub(super) struct ColumnResize {
    pub(super) x: f32,
    pub(super) vertical_range: std::ops::Range<f32>,
    pub(super) width: f32,
    pub(super) minimum_width: f32,
}

pub(super) fn handle_column_resize(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    column: ColumnResize,
    palette: Palette,
    auto_fit: impl FnOnce(&Ui) -> f32,
) -> Option<f32> {
    let ColumnResize {
        x,
        vertical_range,
        width: current_width,
        minimum_width,
    } = column;
    let top_y = vertical_range.start;
    let bottom_y = vertical_range.end;
    let total_height = bottom_y - top_y;
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
        let next = (prev + response.drag_delta().x).clamp(minimum_width, 800.0);
        ui.ctx().data_mut(|d| d.insert_temp(drag_state_id, next));
    }

    if response.hovered() || response.dragged() {
        ui.painter().line_segment(
            [egui::pos2(x, top_y), egui::pos2(x, bottom_y)],
            egui::Stroke::new(2.0_f32, palette.accent),
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

pub(super) fn live_column_width(ui: &Ui, id_salt: impl std::hash::Hash, stored_width: f32) -> f32 {
    let id = ui
        .id()
        .with(("table-col-resize", id_salt))
        .with("drag-width");
    ui.ctx()
        .data(|d| d.get_temp::<f32>(id))
        .unwrap_or(stored_width)
}

pub(super) fn show_property_header(
    ui: &mut Ui,
    data: &DatabaseData,
    property: &DatabaseProperty,
    style: PropertyEditorStyle,
    hovered: bool,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let PropertyEditorStyle {
        width: column_width,
        busy,
        palette,
    } = style;
    let rename_id = ui.id().with(("property-header-rename", property.id));
    if let Some(mut name) = ui.ctx().data(|state| state.get_temp::<String>(rename_id)) {
        let response = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut name)
                .id(rename_id)
                .desired_width((column_width - 12.0).max(24.0)),
        );
        let cancel = (response.has_focus() || response.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Escape));
        let save = response.lost_focus()
            || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
        if cancel || save {
            if !cancel && !busy && !name.trim().is_empty() && name.trim() != property.name {
                intents.push(Intent::Action(Action::RenameDatabaseProperty {
                    property: property.id,
                    name: name.trim().to_owned(),
                }));
                drafts.properties.remove(&property.id);
            }
            response.surrender_focus();
            ui.ctx().data_mut(|state| state.remove::<String>(rename_id));
        } else {
            ui.ctx()
                .data_mut(|state| state.insert_temp(rename_id, name));
        }
        return;
    }

    let sort_key = property.id.to_string();
    let is_active_sort = data.sort_key == sort_key;
    let sort_arrow = if is_active_sort {
        if data.sort_descending { " ▼" } else { " ▲" }
    } else {
        ""
    };
    let icon = property_icon(&property.kind);
    let label = format!("{icon}  {}{sort_arrow}", property.name);
    let text_color = if is_active_sort {
        palette.text
    } else {
        palette.muted
    };

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
        .on_hover_text(format!(
            "{} (Left click: sort · Double click: rename · Right click: column options)",
            property.name
        ));

    if resp.double_clicked() && !busy {
        ui.ctx().data_mut(|state| {
            state.insert_temp(rename_id, property.name.clone());
        });
        ui.memory_mut(|memory| memory.request_focus(rename_id));
        ui.ctx().request_repaint();
    } else if resp.clicked() {
        toggle_sort(data, &sort_key, intents);
    }

    resp.persistent_context_menu(|ui| {
        show_property_column_menu(ui, property, busy, palette, intents, drafts);
    });

    if hovered {
        let menu_resp = ui.persistent_menu_button(
            RichText::new("⋯")
                .color(palette.muted)
                .font(FontId::proportional(13.0)),
            |ui| {
                show_property_column_menu(ui, property, busy, palette, intents, drafts);
            },
        );
        menu_resp.response.persistent_context_menu(|ui| {
            show_property_column_menu(ui, property, busy, palette, intents, drafts);
        });
    }
}

fn show_property_column_menu(
    ui: &mut Ui,
    property: &DatabaseProperty,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    ui.persistent_menu_button("Edit property", |ui| {
        show_property_edit_menu(ui, property, busy, palette, intents, drafts);
    });
}

#[cfg(test)]
mod property_header_tests {
    use std::collections::HashMap;

    use crate::domain::model::DatabaseView;

    use super::*;

    #[test]
    fn double_click_renames_property_and_escape_cancels() {
        for cancel in [false, true] {
            let context = egui::Context::default();
            let property = DatabaseProperty {
                id: 7,
                name: "Status".into(),
                kind: "text".into(),
                options: Vec::new(),
            };
            let database = DatabaseData {
                page_id: 100,
                title: "Database".into(),
                icon: String::new(),
                view: DatabaseView::Table,
                gallery_columns: 3,
                view_settings: Default::default(),
                sort_key: "manual".into(),
                sort_descending: false,
                properties: vec![property.clone()],
                rows: Vec::new(),
            };
            let mut properties = HashMap::new();
            let mut titles = HashMap::new();
            let mut values = HashMap::new();
            let mut time = 0.0;
            let mut frame = |events| {
                time += 0.05;
                let mut intents = Vec::new();
                let mut rect = egui::Rect::NOTHING;
                let _ = context.run(
                    egui::RawInput {
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            show_property_header(
                                ui,
                                &database,
                                &property,
                                PropertyEditorStyle {
                                    width: TABLE_PROPERTY_WIDTH,
                                    busy: false,
                                    palette: Palette::new(false),
                                },
                                false,
                                &mut intents,
                                &mut Drafts {
                                    properties: &mut properties,
                                    titles: &mut titles,
                                    values: &mut values,
                                },
                            );
                            rect = ui.min_rect();
                        });
                    },
                );
                (rect, intents)
            };
            let (rect, _) = frame(Vec::new());
            let position = rect.left_top() + egui::vec2(40.0, 15.0);
            for _ in 0..2 {
                for pressed in [true, false] {
                    frame(vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                }
            }
            frame(Vec::new());
            frame(vec![egui::Event::Text(" updated".into())]);
            let (_, intents) = frame(vec![egui::Event::Key {
                key: if cancel {
                    egui::Key::Escape
                } else {
                    egui::Key::Enter
                },
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            let names: Vec<_> = intents
                .iter()
                .filter_map(|intent| match intent {
                    Intent::Action(Action::RenameDatabaseProperty { property: 7, name }) => {
                        Some(name.as_str())
                    }
                    _ => None,
                })
                .collect();
            if cancel {
                assert!(names.is_empty());
            } else {
                assert_eq!(names.len(), 1);
                assert!(names[0].contains("updated"));
            }
        }
    }
}
