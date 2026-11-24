use crate::ui::menus::PersistentMenuUi;

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{DatabaseProperty, DatabaseRow, PropertyOption};
use crate::ui::page_properties::{
    option_base_color, option_pill_colors, resolve_option_color, show_color_palette_grid,
};
use crate::worker::Action;

use crate::ui::{Intent, palette::Palette};

use super::shared::format_date;

#[derive(Clone, Copy)]
pub(super) struct PropertyEditorStyle {
    pub(super) width: f32,
    pub(super) busy: bool,
    pub(super) palette: Palette,
}

pub(super) fn property_editor(
    ui: &mut Ui,
    row: &DatabaseRow,
    property: &DatabaseProperty,
    style: PropertyEditorStyle,
    intents: &mut Vec<Intent>,
    value_drafts: &mut std::collections::HashMap<(i64, i64), String>,
) -> egui::Response {
    let PropertyEditorStyle {
        width: column_width,
        busy,
        palette,
    } = style;
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
                (bg, text, egui::Stroke::new(1.0_f32, border))
            };
            let display_text = if value.is_empty() { "—" } else { &value };
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = pill_stroke;
                ui.persistent_menu_button(
                    RichText::new(display_text)
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                    |ui| {
                        ui.set_min_width(190.0);
                        if ui.selectable_label(value.is_empty(), "None").clicked()
                            && !value.is_empty()
                        {
                            intents.push(action(String::new()));
                            ui.close_menu();
                        }
                        for (opt_idx, option) in property.options.iter().enumerate() {
                            ui.push_id(
                                (
                                    "cell-sel-opt",
                                    row.page.id,
                                    property.id,
                                    opt_idx,
                                    &option.name,
                                ),
                                |ui| {
                                    let opt_color = resolve_option_color(option.color.as_deref());
                                    let dot_color = option_base_color(
                                        palette,
                                        &property.kind,
                                        &option.name,
                                        opt_color,
                                    );
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
                                                        if let Some(target) = opts
                                                            .iter_mut()
                                                            .find(|o| o.name == option.name)
                                                        {
                                                            target.color = new_color;
                                                            intents.push(Intent::Action(
                                                        Action::SetDatabasePropertyOptions {
                                                            property: property.id,
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

                                        if ui
                                            .selectable_label(value == option.name, &option.name)
                                            .clicked()
                                            && value != option.name
                                        {
                                            intents.push(action(option.name.clone()));
                                            ui.close_menu();
                                        }

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button(
                                                        RichText::new("✕")
                                                            .size(10.5)
                                                            .color(palette.muted),
                                                    )
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
                                            },
                                        );
                                    });
                                },
                            );
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
                )
                .response
            })
            .inner
        }
        "checkbox" => {
            let mut checked = value == "true";
            let response = ui.add(egui::Checkbox::without_text(&mut checked));
            if response.changed() {
                intents.push(action(checked.to_string()));
            }
            response
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
                    if palette.is_dark() {
                        Color32::WHITE
                    } else {
                        palette.text
                    },
                    egui::Stroke::NONE,
                )
            };
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_fill = pill_fill;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = pill_stroke;
                ui.persistent_menu_button(
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
                            ui.push_id(
                                (
                                    "cell-multi-opt",
                                    row.page.id,
                                    property.id,
                                    opt_idx,
                                    &option.name,
                                ),
                                |ui| {
                                    let mut enabled = selected.contains(&option.name);
                                    let opt_color = resolve_option_color(option.color.as_deref());
                                    let dot_color = option_base_color(
                                        palette,
                                        "multi-select",
                                        &option.name,
                                        opt_color,
                                    );
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
                                                        if let Some(target) = opts
                                                            .iter_mut()
                                                            .find(|o| o.name == option.name)
                                                        {
                                                            target.color = new_color;
                                                            intents.push(Intent::Action(
                                                        Action::SetDatabasePropertyOptions {
                                                            property: property.id,
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

                                        if ui.checkbox(&mut enabled, &option.name).changed() {
                                            let mut next = selected.clone();
                                            if enabled && !next.contains(&option.name) {
                                                next.push(option.name.clone());
                                            } else if !enabled {
                                                next.retain(|item| item != &option.name);
                                            }
                                            intents.push(action(next.join("|")));
                                        }

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button(
                                                        RichText::new("✕")
                                                            .size(10.5)
                                                            .color(palette.muted),
                                                    )
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
                                            },
                                        );
                                    });
                                },
                            );
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
                )
                .response
            })
            .inner
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
                ui.persistent_menu_button(
                    RichText::new(display_label)
                        .font(FontId::proportional(12.5))
                        .color(label_color),
                    |ui| {
                        if let Some(chosen) = crate::ui::page_properties::show_date_picker_popup(
                            ui,
                            ("db-cell-cal", row.page.id, property.id),
                            &value,
                            palette,
                        ) {
                            value_drafts.remove(&(row.page.id, property.id));
                            intents.push(action(chosen));
                            ui.close_menu();
                        }
                    },
                )
                .response
                .on_hover_text("Select date from calendar")
            })
            .inner
        }
        "created_time" => {
            let display = if value.trim().is_empty() {
                format_date(row.page.created_at)
            } else {
                value
            };
            ui.label(
                RichText::new(display)
                    .font(FontId::proportional(12.5))
                    .color(palette.muted),
            )
        }
        "last_edited_time" => {
            let ts = row.page.updated_at.max(row.page.created_at);
            let display = if value.trim().is_empty() {
                format_date(ts)
            } else {
                value
            };
            ui.label(
                RichText::new(display)
                    .font(FontId::proportional(12.5))
                    .color(palette.muted),
            )
        }
        _ => {
            let edited = value_drafts
                .entry((row.page.id, property.id))
                .or_insert_with(|| value.clone());
            let default_id_hint = format!("#{}", row.page.id);
            let hint_text: &str = match property.kind.as_str() {
                "url" => "https://…",
                "email" => "name@example.com",
                "phone" => "+90…",
                "id" => &default_id_hint,
                _ => "Empty",
            };
            let has_url_action = property.kind == "url" && !edited.trim().is_empty();
            let edit_w = if has_url_action {
                (column_width - 36.0).max(24.0)
            } else {
                (column_width - 16.0).max(24.0)
            };
            let edit_id = egui::Id::new(("database-property-value", row.page.id, property.id));
            let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
                crate::ui::page_properties::layout_property_value(
                    ui,
                    text,
                    wrap_width,
                    property.kind == "number",
                    edit_id,
                    palette.text,
                )
            };
            let text = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(edited)
                    .id(edit_id)
                    .layouter(&mut layouter)
                    .desired_width(edit_w)
                    .font(FontId::proportional(13.0))
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .hint_text(hint_text),
            );
            let number_submitted = property.kind == "number"
                && (text.has_focus() || text.lost_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if number_submitted && !edited.trim().is_empty() {
                let Some(result) = crate::domain::number_expression::evaluate(edited) else {
                    text.request_focus();
                    return text;
                };
                *edited = result;
                ui.ctx().request_repaint();
            }
            if has_url_action
                && ui
                    .small_button(RichText::new("↗").color(palette.accent))
                    .on_hover_text("Open URL")
                    .clicked()
            {
                let target_url = edited.trim();
                let full_url =
                    if target_url.starts_with("http://") || target_url.starts_with("https://") {
                        target_url.to_owned()
                    } else {
                        format!("https://{target_url}")
                    };
                intents.push(Intent::OpenUrl(full_url));
            }
            if (text.lost_focus() || number_submitted) && edited != &value {
                intents.push(action(edited.clone()));
            }
            text
        }
    }
}

#[cfg(test)]
mod property_value_tests {
    use std::collections::HashMap;

    use eframe::egui::text::{CCursor, CCursorRange};

    use crate::ui::database::table_columns::TABLE_PROPERTY_WIDTH;

    use super::*;

    #[test]
    fn number_cell_calculates_on_enter_and_keeps_invalid_input_editable() {
        for (expression, expected) in [
            ("500+20", Some("520")),
            ("500-20", Some("480")),
            ("500*20", Some("10000")),
            ("500/20", Some("25")),
            ("500/0", None),
            ("500+", None),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut repository =
                crate::storage::Repository::open(&directory.path().join("test.sqlite")).unwrap();
            let database = repository.create_database(None).unwrap();
            let page = repository.create_database_record(database).unwrap();
            repository
                .add_database_property(database, "Number", "number", &[])
                .unwrap();
            let data = repository.database(database).unwrap();
            let property = data
                .properties
                .iter()
                .find(|property| property.kind == "number")
                .unwrap();
            let row = &data.rows[0];
            let context = egui::Context::default();
            let mut drafts = HashMap::new();
            let mut edit_id = egui::Id::NULL;
            let mut frame = |events| {
                let mut intents = Vec::new();
                let _ = context.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            edit_id = egui::Id::new(("database-property-value", page, property.id));
                            property_editor(
                                ui,
                                row,
                                property,
                                PropertyEditorStyle {
                                    width: TABLE_PROPERTY_WIDTH,
                                    busy: false,
                                    palette: Palette::new(false),
                                },
                                &mut intents,
                                &mut drafts,
                            );
                        });
                    },
                );
                (intents, edit_id)
            };
            let (_, field) = frame(Vec::new());
            context.memory_mut(|memory| memory.request_focus(field));
            frame(Vec::new());
            assert!(
                frame(vec![egui::Event::Text(expression.into())])
                    .0
                    .is_empty()
            );
            let (intents, _) = frame(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            let saved: Vec<_> = intents
                .iter()
                .filter_map(|intent| match intent {
                    Intent::Action(Action::SetDatabaseProperty { value, .. }) => {
                        Some(value.as_str())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                saved,
                expected.into_iter().collect::<Vec<_>>(),
                "{expression}"
            );
            if expected.is_none() {
                assert_eq!(context.memory(|memory| memory.focused()), Some(field));
            }
            assert_eq!(drafts[&(page, property.id)], expected.unwrap_or(expression));
        }
    }

    #[test]
    fn cell_edits_save_on_blur_without_reverting_or_stealing_the_next_fields_focus() {
        for kind in ["text", "number", "email", "url"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("test.sqlite");
            let mut repository = crate::storage::Repository::open(&path).unwrap();
            let database = repository.create_database(None).unwrap();
            let page = repository.create_database_record(database).unwrap();
            repository
                .add_database_property(database, "Value", kind, &[])
                .unwrap();
            let data = repository.database(database).unwrap();
            let property = data
                .properties
                .iter()
                .find(|property| property.kind == kind)
                .unwrap();
            let original = if kind == "number" { "1" } else { "old" };
            repository
                .set_database_property(page, property.id, original)
                .unwrap();
            let mut row = repository.database(database).unwrap().rows[0].clone();
            let context = egui::Context::default();
            let mut drafts = HashMap::new();
            let mut next_value = String::new();
            let next_id = egui::Id::new("next-field");
            let mut frame = |row: &DatabaseRow, events| {
                let mut intents = Vec::new();
                let mut next_rect = egui::Rect::NOTHING;
                let _ = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400.0, 120.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            property_editor(
                                ui,
                                row,
                                property,
                                PropertyEditorStyle {
                                    width: TABLE_PROPERTY_WIDTH,
                                    busy: false,
                                    palette: Palette::new(false),
                                },
                                &mut intents,
                                &mut drafts,
                            );
                            next_rect = ui
                                .add(egui::TextEdit::singleline(&mut next_value).id(next_id))
                                .rect;
                        });
                    },
                );
                (intents, next_rect)
            };
            frame(&row, Vec::new());
            let position = egui::pos2(25.0, 16.0);
            for pressed in [true, false] {
                frame(
                    &row,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            let focused = context
                .memory(|memory| memory.focused())
                .expect("cell must be focused");
            let mut state = egui::text_edit::TextEditState::load(&context, focused).unwrap();
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(0),
                CCursor::new(original.chars().count()),
            )));
            state.store(&context, focused);
            let value = if kind == "number" { "5" } else { "edited" };
            for character in value.chars() {
                let (intents, _) = frame(&row, vec![egui::Event::Text(character.to_string())]);
                assert!(
                    intents.is_empty(),
                    "typing must not save or refresh the table"
                );
                assert_eq!(context.memory(|memory| memory.focused()), Some(focused));
            }
            let (_, next_rect) = frame(&row, Vec::new());
            let position = next_rect.center();
            let mut saved = Vec::new();
            for pressed in [true, false] {
                let (intents, _) = frame(
                    &row,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                saved.extend(intents);
            }
            assert_eq!(saved.len(), 1);
            let Intent::Action(Action::SetDatabaseProperty {
                page: saved_page,
                property: saved_property,
                value: saved_value,
            }) = &saved[0]
            else {
                panic!("leaving the cell must save its draft");
            };
            assert_eq!(*saved_page, page);
            assert_eq!(*saved_property, property.id);
            assert_eq!(saved_value, value);
            assert_eq!(context.memory(|memory| memory.focused()), Some(next_id));
            repository
                .set_database_property(*saved_page, *saved_property, saved_value)
                .unwrap();
            row = repository.database(database).unwrap().rows[0].clone();
            let (intents, _) = frame(&row, Vec::new());
            assert!(intents.is_empty());
            assert_eq!(context.memory(|memory| memory.focused()), Some(next_id));
            assert_eq!(drafts[&(page, property.id)], value);
            drop(repository);
            let reopened = crate::storage::Repository::open(&path).unwrap();
            assert_eq!(
                reopened.database(database).unwrap().rows[0].values[&property.id],
                value
            );
        }
    }
}
