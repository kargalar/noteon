use std::collections::HashMap;

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{
    DEFAULT_DATABASE_STATUS_OPTIONS, DatabaseData, DatabaseProperty, DatabaseRow, DatabaseView,
    PROPERTY_TYPES,
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

                        ui.menu_button(
                            RichText::new("+ Property")
                                .font(FontId::proportional(13.0))
                                .color(palette.muted),
                            |ui| {
                                add_property_menu(ui, data, intents);
                            },
                        );

                        show_view_options(ui, data, intents);

                        let sort_name = sort_key_label(data);
                        let direction_arrow = if data.sort_descending { "▼" } else { "▲" };
                        ui.menu_button(
                            RichText::new(format!("⇅ Sort: {sort_name} {direction_arrow}"))
                                .font(FontId::proportional(12.5))
                                .color(palette.muted),
                            |ui| {
                                ui.label(RichText::new("Sort by").small().color(palette.muted));
                                for (label, key) in [("Name", "title"), ("Created", "created_at")] {
                                    let is_selected = data.sort_key == key;
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        toggle_sort(data, key, intents);
                                    }
                                }
                                for property in &data.properties {
                                    let key = property.id.to_string();
                                    let is_selected = data.sort_key == key;
                                    if ui.selectable_label(is_selected, &property.name).clicked() {
                                        toggle_sort(data, &key, intents);
                                    }
                                }
                                ui.separator();
                                ui.label(RichText::new("Direction").small().color(palette.muted));
                                if ui.selectable_label(!data.sort_descending, "Ascending (▲)").clicked()
                                    && data.sort_descending
                                {
                                    intents.push(Intent::Action(Action::SetDatabaseSort {
                                        page: data.page_id,
                                        key: data.sort_key.clone(),
                                        descending: false,
                                    }));
                                }
                                if ui.selectable_label(data.sort_descending, "Descending (▼)").clicked()
                                    && !data.sort_descending
                                {
                                    intents.push(Intent::Action(Action::SetDatabaseSort {
                                        page: data.page_id,
                                        key: data.sort_key.clone(),
                                        descending: true,
                                    }));
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
                &board_properties,
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
                properties,
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
                    property_editor(ui, row, property, busy, palette, intents, value_drafts);
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

fn show_table(
    ui: &mut Ui,
    data: &DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let rows = sorted_rows(data);
    let visible_properties: Vec<_> = data
        .properties
        .iter()
        .filter(|property| {
            data.view_settings
                .table_visible_properties
                .as_ref()
                .is_none_or(|visible| visible.contains(&property.id))
        })
        .collect();
    let show_created_at = data.view_settings.table_show_created_at;
    let table_width = TABLE_NAME_WIDTH
        + if show_created_at {
            TABLE_CREATED_WIDTH
        } else {
            0.0
        }
        + TABLE_PROPERTY_WIDTH * visible_properties.len() as f32
        + TABLE_ADD_PROP_WIDTH;

    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.set_min_width(table_width);

            // 1. Table Header Row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_cell(
                    ui,
                    TABLE_NAME_WIDTH,
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
                        let resp = ui.add(
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
                        TABLE_CREATED_WIDTH,
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
                            let text_color = if is_active_sort { palette.text } else { palette.muted };
                            let resp = ui.add(
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

                for property in &visible_properties {
                    table_cell(
                        ui,
                        TABLE_PROPERTY_WIDTH,
                        TABLE_HEADER_HEIGHT,
                        palette,
                        true,
                        false,
                        |ui, hovered| {
                            show_property_header(
                                ui, data, property, busy, palette, hovered, intents, drafts,
                            );
                        },
                    );
                }

                table_cell(
                    ui,
                    TABLE_ADD_PROP_WIDTH,
                    TABLE_HEADER_HEIGHT,
                    palette,
                    true,
                    false,
                    |ui, _hovered| {
                        ui.menu_button(
                            RichText::new("+").size(15.0).color(palette.muted),
                            |ui| {
                                add_property_menu(ui, data, intents);
                            },
                        );
                    },
                );
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
                                TABLE_NAME_WIDTH,
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
                                    let width_avail = if hovered {
                                        TABLE_NAME_WIDTH - 64.0
                                    } else {
                                        TABLE_NAME_WIDTH - 42.0
                                    };
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
                                    TABLE_CREATED_WIDTH,
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

                            for property in &visible_properties {
                                table_cell(
                                    ui,
                                    TABLE_PROPERTY_WIDTH,
                                    TABLE_ROW_HEIGHT,
                                    palette,
                                    false,
                                    false,
                                    |ui, _| {
                                        property_editor(
                                            ui,
                                            row,
                                            property,
                                            busy,
                                            palette,
                                            intents,
                                            drafts.values,
                                        );
                                    },
                                );
                            }

                            table_cell(
                                ui,
                                TABLE_ADD_PROP_WIDTH,
                                TABLE_ROW_HEIGHT,
                                palette,
                                false,
                                false,
                                |_ui, _| {},
                            );
                        });
                    }
                });

            // 3. "+ New" Bottom Row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_cell(
                    ui,
                    TABLE_NAME_WIDTH,
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
                        let resp = ui.add(
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
                        TABLE_CREATED_WIDTH,
                        TABLE_ROW_HEIGHT,
                        palette,
                        false,
                        false,
                        |_ui, _| {},
                    );
                }
                for _ in &visible_properties {
                    table_cell(
                        ui,
                        TABLE_PROPERTY_WIDTH,
                        TABLE_ROW_HEIGHT,
                        palette,
                        false,
                        false,
                        |_ui, _| {},
                    );
                }
                table_cell(
                    ui,
                    TABLE_ADD_PROP_WIDTH,
                    TABLE_ROW_HEIGHT,
                    palette,
                    false,
                    false,
                    |_ui, _| {},
                );
            });

            // 4. Table Calculation / Footer
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_footer_cell(ui, TABLE_NAME_WIDTH, palette, true, |ui| {
                    ui.label(
                        RichText::new(format!("Count {}", rows.len()))
                            .font(FontId::proportional(11.0))
                            .color(palette.muted),
                    );
                });
                if show_created_at {
                    table_footer_cell(ui, TABLE_CREATED_WIDTH, palette, false, |_ui| {});
                }
                for _ in &visible_properties {
                    table_footer_cell(ui, TABLE_PROPERTY_WIDTH, palette, false, |_ui| {});
                }
                table_footer_cell(ui, TABLE_ADD_PROP_WIDTH, palette, false, |_ui| {});
            });
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

fn show_property_header(
    ui: &mut Ui,
    data: &DatabaseData,
    property: &DatabaseProperty,
    busy: bool,
    palette: Palette,
    hovered: bool,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let draft = drafts
        .properties
        .entry(property.id)
        .or_insert_with(|| (property.name.clone(), property.options.join(", ")));
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
        TABLE_PROPERTY_WIDTH - 40.0
    } else {
        TABLE_PROPERTY_WIDTH - 16.0
    };

    let resp = ui.add_sized(
        [text_width, TABLE_HEADER_HEIGHT],
        egui::Button::new(
            RichText::new(label)
                .font(FontId::proportional(12.5))
                .color(text_color)
                .strong(),
        )
        .frame(false),
    )
    .on_hover_text(&property.name);

    if resp.clicked() {
        toggle_sort(data, &sort_key, intents);
    }

    if hovered {
        ui.menu_button(
            RichText::new("⋯")
                .color(palette.muted)
                .font(FontId::proportional(13.0)),
            |ui| {
                ui.label(RichText::new("Property name").small().color(palette.muted));
                let name_changed = ui
                    .add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut draft.0).desired_width(180.0),
                    )
                    .lost_focus();
                if name_changed && draft.0 != property.name {
                    intents.push(Intent::Action(Action::RenameDatabaseProperty {
                        property: property.id,
                        name: draft.0.clone(),
                    }));
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Type").small().color(palette.muted));
                egui::ComboBox::from_id_salt(("property-kind", property.id))
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
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Options (comma-separated)")
                            .small()
                            .color(palette.muted),
                    );
                    ui.text_edit_singleline(&mut draft.1);
                    if ui.button("Save options").clicked() {
                        intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                            property: property.id,
                            options: draft
                                .1
                                .split(',')
                                .map(str::trim)
                                .filter(|option| !option.is_empty())
                                .map(str::to_owned)
                                .collect(),
                        }));
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui
                    .button(RichText::new("Delete property").color(palette.danger))
                    .clicked()
                {
                    intents.push(Intent::Action(Action::DeleteDatabaseProperty(property.id)));
                    ui.close_menu();
                }
            },
        );
    }
}

fn property_types_label(kind: &str) -> &'static str {
    PROPERTY_TYPES
        .iter()
        .find(|(key, _)| *key == kind)
        .map_or("Metin", |(_, label)| label)
}

fn property_editor(
    ui: &mut Ui,
    row: &DatabaseRow,
    property: &DatabaseProperty,
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
            let draft = value_drafts
                .entry((row.page.id, property.id))
                .or_insert_with(|| value.clone());
            let mut selected: Vec<String> = draft
                .split('|')
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect();
            let selected_label = if selected.is_empty() {
                "—".to_owned()
            } else {
                let joined = selected.join(", ");
                if joined.chars().count() > 20 {
                    format!("{}…", joined.chars().take(19).collect::<String>())
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
                        for option in &property.options {
                            let mut enabled = selected.contains(option);
                            if ui.checkbox(&mut enabled, option).changed() {
                                if enabled && !selected.contains(option) {
                                    selected.push(option.clone());
                                }
                                if !enabled {
                                    selected.retain(|item| item != option);
                                }
                                *draft = selected.join("|");
                            }
                        }
                        if ui.button("Uygula").clicked() {
                            intents.push(action(draft.clone()));
                            ui.close_menu();
                        }
                    },
                );
            });
        }
        _ => {
            let edited = value_drafts
                .entry((row.page.id, property.id))
                .or_insert(value.clone());
            let text = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(edited)
                    .desired_width(TABLE_PROPERTY_WIDTH - 20.0)
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
                options: if matches!(kind, "select" | "status" | "multi-select") {
                    DEFAULT_DATABASE_STATUS_OPTIONS
                        .iter()
                        .map(|option| (*option).into())
                        .collect()
                } else {
                    Vec::new()
                },
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
    let row_height: f32 = (if show_cover { 160.0 } else { 0.0 })
        + if show_title { 28.0 } else { 0.0 }
        + if show_created_at { 20.0 } else { 0.0 }
        + if show_description { 38.0 } else { 0.0 }
        + if property_count > 0 { 20.0 } else { 0.0 }
        + 24.0;
    let row_height = row_height.max(70.0);
    let gap = ui.spacing().item_spacing.x;
    let card_frame = egui::Frame::new()
        .fill(palette.sidebar)
        .stroke(egui::Stroke::new(1.0, palette.border.linear_multiply(0.5)))
        .corner_radius(8)
        .inner_margin(10);
    let frame_margin = card_frame.total_margin();
    let horizontal_frame_margin = frame_margin.left + frame_margin.right;
    let card_width = ((ui.available_width().max(1.0) - gap * columns.saturating_sub(1) as f32)
        / columns as f32
        - horizontal_frame_margin)
        .max(1.0);
    let card_height = (row_height - frame_margin.top - frame_margin.bottom).max(1.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, row_count, |ui, visible| {
            for row_index in visible {
                ui.horizontal(|ui| {
                    for row in rows.iter().skip(row_index * columns).take(columns) {
                        card_frame.show(ui, |ui| {
                            ui.set_min_width(card_width);
                            ui.set_max_width(card_width);
                            ui.set_min_height(card_height);

                            let content_width = (card_width - 16.0).max(1.0);
                            if show_cover {
                                let image_size = egui::vec2(content_width, 160.0);
                                let (image_rect, response) =
                                    ui.allocate_exact_size(image_size, egui::Sense::click());
                                if let Some(path) = row.page.cover_image.as_deref() {
                                    let focus = egui::vec2(row.page.cover_x, row.page.cover_y);
                                    crate::ui::editor::media::paint_cover(
                                        ui,
                                        std::path::Path::new(path),
                                        image_rect,
                                        focus,
                                    );
                                } else {
                                    ui.painter().rect_filled(image_rect, 4.0, palette.code);
                                }
                                if response.clicked() && !busy {
                                    intents.push(Intent::Open(row.page.id, None));
                                }
                            }
                            if show_title {
                                let title = if row.page.title.is_empty() {
                                    "Untitled page"
                                } else {
                                    &row.page.title
                                };
                                if ui
                                    .add_sized(
                                        [content_width, 22.0],
                                        egui::Label::new(RichText::new(title).heading())
                                            .truncate()
                                            .sense(egui::Sense::click()),
                                    )
                                    .clicked()
                                    && !busy
                                {
                                    intents.push(Intent::Open(row.page.id, None));
                                }
                            }
                            if show_created_at {
                                ui.add_sized(
                                    [content_width, 16.0],
                                    egui::Label::new(
                                        RichText::new(format_date(row.page.created_at))
                                            .small()
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }
                            if show_description && !row.preview.trim().is_empty() {
                                ui.add_sized(
                                    [content_width, 36.0],
                                    egui::Label::new(
                                        RichText::new(&row.preview).small().color(palette.muted),
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
                                        .map(|value| format!("{}: {value}", property.name))
                                })
                                .collect();
                            if !summaries.is_empty() {
                                ui.add_sized(
                                    [content_width, 18.0],
                                    egui::Label::new(
                                        RichText::new(summaries.join(" / "))
                                            .small()
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                    }
                });
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
    let mut groups = if status.options.is_empty() {
        vec!["No selection".to_owned()]
    } else {
        status.options.clone()
    };
    if !groups.iter().any(|group| group == "No selection") {
        groups.push("No selection".into());
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
    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for group in groups {
                egui::Frame::new()
                    .fill(palette.code)
                    .corner_radius(8)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_min_width(230.0);
                        let rows = rows_by_group.get(&group).map_or(&[][..], Vec::as_slice);
                        ui.horizontal(|ui| {
                            let pill_frame = egui::Frame::new()
                                .fill(palette.accent_soft)
                                .corner_radius(4)
                                .inner_margin(egui::Margin::symmetric(6, 2));
                            pill_frame.show(ui, |ui| {
                                ui.label(
                                    RichText::new(&group)
                                        .font(FontId::proportional(12.5))
                                        .color(palette.text)
                                        .strong(),
                                );
                            });
                            ui.label(
                                RichText::new(rows.len().to_string())
                                    .font(FontId::proportional(11.0))
                                    .color(palette.muted),
                            );
                        });
                        ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt(("database-board-column", data.page_id, &group))
                        .max_height(ui.available_height())
                        .show_rows(ui, 62.0, rows.len(), |ui, visible| {
                            for row in &rows[visible] {
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !busy,
                                            egui::Button::new(row.page.display_title()),
                                        )
                                        .clicked()
                                    {
                                        intents.push(Intent::Open(row.page.id, None));
                                    }
                                    let current =
                                        row.values.get(&status.id).cloned().unwrap_or_default();
                                    egui::ComboBox::from_id_salt(("board-status", row.page.id))
                                        .selected_text(if current.is_empty() {
                                            "No selection"
                                        } else {
                                            &current
                                        })
                                        .show_ui(ui, |ui| {
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
                                                }
                                            }
                                        });
                                });
                                let properties: Vec<_> = data
                                    .properties
                                    .iter()
                                    .filter(|property| property.id != status.id)
                                    .filter(|property| {
                                        board_visible_properties.map_or_else(
                                            || {
                                                data.properties
                                                    .iter()
                                                    .filter(|candidate| candidate.id != status.id)
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
                                            .map(|value| format!("{}: {value}", property.name))
                                    })
                                    .collect();
                                if !properties.is_empty() {
                                    ui.label(
                                        RichText::new(properties.join("  ·  "))
                                            .small()
                                            .color(palette.muted),
                                    );
                                }
                                if data.view_settings.board_show_description
                                    && !row.preview.trim().is_empty()
                                {
                                    ui.label(
                                        RichText::new(&row.preview).small().color(palette.muted),
                                    );
                                }
                            }
                        });
                });
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
