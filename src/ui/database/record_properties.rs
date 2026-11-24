use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseProperty, DatabaseRow, PropertyOption};
use crate::ui::page_properties::{
    option_base_color, option_pill_colors, property_icon, resolve_option_color,
    show_color_palette_grid,
};
use crate::worker::Action;

use crate::ui::{Intent, palette::Palette};

use super::Drafts;
use super::property_menu::show_property_edit_menu;
use super::property_value::{PropertyEditorStyle, property_editor};

pub struct RecordPropertiesTarget {
    pub page_id: i64,
    pub indent: f32,
    pub max_width: f32,
}

fn render_record_multi_select_chips(
    ui: &mut Ui,
    row: &DatabaseRow,
    property: &DatabaseProperty,
    style: PropertyEditorStyle,
    intents: &mut Vec<Intent>,
) {
    let PropertyEditorStyle {
        width: available_width,
        busy,
        palette,
    } = style;
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

                let resp = ui.add_enabled(!busy, pill_btn).on_hover_text(format!(
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
                resp.persistent_context_menu(|ui| {
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
            ui.persistent_menu_button(
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
                            let dot_color =
                                option_base_color(palette, "multi-select", &option.name, opt_color);
                            ui.horizontal(|ui| {
                                ui.persistent_menu_button(
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
                                                if let Some(target) =
                                                    opts.iter_mut().find(|o| o.name == option.name)
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
                    let quick_id = egui::Id::new(("db-prop-multi-quick-opt", record_id, prop_id));
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
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(quick_id, new_opt.clone()));
                        }
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.small_button("+").clicked() || enter) && !new_opt.trim().is_empty() {
                            let trimmed = new_opt.trim().to_owned();
                            let mut opts = property.options.clone();
                            if !opts.iter().any(|o| o.name == trimmed) {
                                opts.push(PropertyOption::new(&trimmed));
                                intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                    property: prop_id,
                                    options: opts,
                                }));
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

#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the public record properties API"
)]
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

                                            let menu_resp = ui.persistent_menu_button(
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
                                                    crate::ui::page_properties::type_label(&property.kind)
                                                ))
                                                .persistent_context_menu(|ui| {
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
                                        let style = PropertyEditorStyle { width: value_w - 12.0, busy, palette };
                                        if property.kind == "multi-select" {
                                            render_record_multi_select_chips(
                                                ui,
                                                row,
                                                property,
                                                style,
                                                intents,
                                            );
                                        } else {
                                            property_editor(
                                                ui,
                                                row,
                                                property,
                                                style,
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
