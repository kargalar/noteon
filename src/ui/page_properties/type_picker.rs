use crate::ui::menus::PersistentMenuUi;
use eframe::egui::{self, Color32, FontId, RichText, Sense, Ui, vec2};

use crate::domain::model::{PROPERTY_TYPE_GROUPS, PROPERTY_TYPES};
use crate::ui::palette::Palette;

use super::appearance::{blend_color, paint_property_icon, property_icon};
use super::drafts::{clear_draft, read_draft, save_draft};

pub fn show_property_type_picker(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + Copy,
    current_kind: Option<&str>,
    busy: bool,
    palette: Palette,
) -> Option<&'static str> {
    let col_w = 148.0;
    let col_gap = 6.0;
    let total_w = col_w * 2.0 + col_gap;
    ui.set_min_width(total_w);
    ui.set_max_width(total_w);

    let search_key = ("prop-type-picker-search", id_salt);
    let mut query = read_draft(ui, search_key, "");

    // "Select type 🔍" search/filter bar
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(4.0);
        let edit_w = if query.is_empty() { 265.0 } else { 248.0 };
        let resp = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut query)
                .hint_text("Select type")
                .font(FontId::proportional(12.5))
                .frame(false)
                .margin(egui::Margin::symmetric(2, 4))
                .desired_width(edit_w),
        );
        save_draft(ui, search_key, &query, resp.changed());

        if query.is_empty() {
            let mag_pos = egui::pos2(resp.rect.left() + 76.0, resp.rect.center().y);
            let mag_rect = egui::Rect::from_center_size(mag_pos, vec2(13.0, 13.0));
            paint_property_icon(
                ui.painter(),
                mag_rect,
                "rollup",
                palette.muted,
                palette.background,
            );
        } else if ui
            .add(
                egui::Button::new(
                    RichText::new("✕")
                        .font(FontId::proportional(11.0))
                        .color(palette.muted),
                )
                .frame(false),
            )
            .on_hover_text("Clear filter")
            .clicked()
        {
            query.clear();
            clear_draft(ui, search_key);
        }
    });

    ui.add_space(2.0);

    let query_lower = query.trim().to_ascii_lowercase();
    let mut selected_kind: Option<&'static str> = None;

    egui::ScrollArea::vertical()
        .id_salt(("prop-type-picker-scroll", id_salt))
        .max_height(380.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(col_gap, 2.0);

            let render_pair =
                |ui: &mut Ui,
                 pair: &[(&'static str, &'static str)],
                 selected_kind: &mut Option<&'static str>| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = col_gap;
                        for &(kind, label) in pair {
                            let is_selected = current_kind == Some(kind);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(col_w, 28.0), Sense::click());
                            let hovered = !busy && resp.hovered();
                            if hovered {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let row_bg = if is_selected {
                                palette.accent_soft
                            } else if hovered {
                                palette.hover
                            } else {
                                Color32::TRANSPARENT
                            };
                            if row_bg != Color32::TRANSPARENT {
                                ui.painter().rect_filled(rect, 5.0, row_bg);
                            }
                            let icon_bg = if row_bg != Color32::TRANSPARENT {
                                row_bg
                            } else {
                                palette.background
                            };
                            let icon_color = if is_selected {
                                palette.accent
                            } else if hovered {
                                palette.text
                            } else {
                                palette.muted
                            };
                            let text_color = if is_selected || hovered {
                                palette.text
                            } else {
                                blend_color(palette.text, palette.muted, 0.15)
                            };
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(rect.left() + 14.0, rect.center().y),
                                vec2(15.0, 15.0),
                            );
                            paint_property_icon(ui.painter(), icon_rect, kind, icon_color, icon_bg);
                            ui.painter().text(
                                egui::pos2(rect.left() + 28.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(13.0),
                                text_color,
                            );
                            if is_selected {
                                ui.painter().text(
                                    egui::pos2(rect.right() - 10.0, rect.center().y),
                                    egui::Align2::CENTER_CENTER,
                                    "✓",
                                    FontId::proportional(12.0),
                                    palette.accent,
                                );
                            }
                            if !busy && resp.clicked() {
                                *selected_kind = Some(kind);
                            }
                        }
                    });
                };

            if query_lower.is_empty() {
                for (group_idx, group) in PROPERTY_TYPE_GROUPS.iter().enumerate() {
                    if group_idx > 0 {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                    }
                    for pair in group.chunks(2) {
                        render_pair(ui, pair, &mut selected_kind);
                    }
                }
            } else {
                let matches: Vec<(&'static str, &'static str)> = PROPERTY_TYPES
                    .into_iter()
                    .filter(|(kind, label)| {
                        label.to_ascii_lowercase().contains(&query_lower)
                            || kind.to_ascii_lowercase().contains(&query_lower)
                    })
                    .collect();
                if matches.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("No matching property type")
                            .font(FontId::proportional(12.0))
                            .color(palette.muted),
                    );
                } else {
                    for pair in matches.chunks(2) {
                        render_pair(ui, pair, &mut selected_kind);
                    }
                }
            }
        });

    if selected_kind.is_some() {
        clear_draft(ui, search_key);
    }

    selected_kind
}

pub fn show_property_type_selector_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + Copy,
    current_kind: &str,
    busy: bool,
    palette: Palette,
) -> Option<&'static str> {
    let mut chosen = None;
    ui.push_id(("prop-type-sel-btn", id_salt), |ui| {
        let icon = property_icon(current_kind);
        let label = type_label(current_kind);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("⇄  Type")
                    .font(FontId::proportional(12.5))
                    .color(palette.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.persistent_menu_button(
                    RichText::new(format!("{icon}  {label}  ▸"))
                        .font(FontId::proportional(12.5))
                        .color(palette.text),
                    |ui| {
                        if let Some(kind) = show_property_type_picker(
                            ui,
                            id_salt,
                            Some(current_kind),
                            busy,
                            palette,
                        ) {
                            if kind != current_kind {
                                chosen = Some(kind);
                            }
                            ui.close_menu();
                        }
                    },
                );
            });
        });
    });
    chosen
}

pub fn type_label(kind: &str) -> &'static str {
    PROPERTY_TYPES
        .iter()
        .find_map(|(candidate, label)| (*candidate == kind).then_some(*label))
        .unwrap_or("Text")
}

pub fn default_name_for_kind(kind: &str) -> &'static str {
    match kind {
        "text" => "Text",
        "number" => "Number",
        "select" => "Select",
        "multi-select" => "Multi-select",
        "status" => "Status",
        "date" => "Date",
        "person" => "Person",
        "files" => "Files & media",
        "checkbox" => "Checkbox",
        "url" => "URL",
        "phone" => "Phone",
        "email" => "Email",
        "relation" => "Relation",
        "rollup" => "Rollup",
        "formula" => "Formula",
        "button" => "Button",
        "id" => "ID",
        "place" => "Place",
        "created_time" => "Created time",
        "last_edited_time" => "Last edited time",
        "created_by" => "Created by",
        "last_edited_by" => "Last edited by",
        _ => "Property",
    }
}
