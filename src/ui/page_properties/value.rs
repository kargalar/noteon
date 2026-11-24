use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{PageId, PageProperty};
use crate::ui::{Intent, palette::Palette};

use super::date::show_date_editor;
use super::drafts::{read_draft, save_draft};
use super::select::{show_multi_option_editor, show_single_option_editor};
use super::set_value;

#[allow(clippy::too_many_arguments)]
pub(super) fn property_value_editor(
    ui: &mut Ui,
    page: PageId,
    created_at: i64,
    property: &PageProperty,
    available_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    match property.kind.as_str() {
        "checkbox" => {
            let mut checked = property.value == "true";
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Checkbox::without_text(&mut checked))
                    .changed()
                {
                    set_value(intents, page, property.id, checked.to_string());
                }
            });
        }
        "select" | "status" => {
            show_single_option_editor(ui, page, property, busy, palette, intents);
        }
        "multi-select" => {
            show_multi_option_editor(ui, page, property, available_width, busy, palette, intents);
        }
        "date" => {
            show_date_editor(ui, page, property, busy, palette, intents);
        }
        "created_time" | "last_edited_time" => {
            let display = if property.value.trim().is_empty() {
                crate::ui::database::format_date(created_at)
            } else {
                property.value.clone()
            };
            ui.label(
                RichText::new(display)
                    .font(FontId::proportional(13.0))
                    .color(palette.muted),
            );
        }
        _ => {
            show_text_or_number_editor(ui, page, property, available_width, busy, palette, intents);
        }
    }
}

pub(crate) fn layout_property_value(
    ui: &Ui,
    text: &str,
    wrap_width: f32,
    is_number: bool,
    edit_id: egui::Id,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let display = if is_number && !ui.memory(|memory| memory.has_focus(edit_id)) {
        crate::domain::number_format::format_display(text)
    } else {
        text.to_owned()
    };
    let job = egui::text::LayoutJob::simple(display, FontId::proportional(13.0), color, wrap_width);
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn show_text_or_number_editor(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    available_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let value_id = ("property-value", page, property.id);
    let mut value = read_draft(ui, value_id, &property.value);
    if !ui.ctx().memory(|m| m.has_focus(ui.id().with(value_id))) && value != property.value {
        value = property.value.clone();
    }

    let default_id_hint = format!("#{page}");
    let hint: &str = match property.kind.as_str() {
        "number" => "Empty (number)",
        "url" => "https://…",
        "email" => "name@example.com",
        "phone" => "+90…",
        "person" => "Empty (person)",
        "files" => "File path or URL…",
        "place" => "Empty (location)",
        "relation" => "Empty (relation)",
        "rollup" => "Empty (rollup)",
        "formula" => "Empty (formula)",
        "button" => "Button label…",
        "id" => &default_id_hint,
        _ => "Empty",
    };

    let has_url_action = property.kind == "url" && !value.trim().is_empty();
    let edit_w = if has_url_action {
        (available_width - 42.0).max(60.0)
    } else {
        (available_width - 16.0).max(80.0)
    };

    let edit_id = ui.id().with(value_id);
    let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
        layout_property_value(
            ui,
            text,
            wrap_width,
            property.kind == "number",
            edit_id,
            palette.text,
        )
    };
    let response = ui.add_enabled(
        !busy,
        egui::TextEdit::singleline(&mut value)
            .id(edit_id)
            .layouter(&mut layouter)
            .hint_text(hint)
            .font(FontId::proportional(13.0))
            .frame(false)
            .margin(egui::Margin::symmetric(4, 2))
            .desired_width(edit_w),
    );
    let number_submitted = property.kind == "number"
        && (response.has_focus() || response.lost_focus())
        && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if number_submitted && !value.trim().is_empty() {
        if let Some(result) = crate::domain::number_expression::evaluate(&value) {
            value = result;
            ui.ctx().request_repaint();
        } else {
            response.request_focus();
        }
    }
    save_draft(ui, value_id, &value, response.changed() || number_submitted);
    let is_invalid_number = property.kind == "number"
        && !value.trim().is_empty()
        && value.trim().parse::<f64>().map_or(true, |n| !n.is_finite());

    if has_url_action
        && ui
            .small_button(RichText::new("↗").color(palette.accent))
            .on_hover_text("Open URL")
            .clicked()
    {
        let target_url = value.trim();
        let full_url = if target_url.starts_with("http://") || target_url.starts_with("https://") {
            target_url.to_owned()
        } else {
            format!("https://{target_url}")
        };
        intents.push(Intent::OpenUrl(full_url));
    }

    if is_invalid_number {
        ui.label(
            RichText::new("⚠ Invalid number")
                .font(FontId::proportional(11.0))
                .color(palette.danger),
        );
    } else {
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (response.lost_focus() || enter) && value != property.value {
            set_value(intents, page, property.id, value);
        }
    }
}

#[cfg(test)]
mod number_input_tests {
    use crate::worker::Action;

    use super::*;

    #[test]
    fn number_display_formats_only_unfocused_number_properties() {
        let context = egui::Context::default();
        let field = egui::Id::new("number-display-test");
        for (is_number, focused, expected) in [
            (true, false, "40.000,5"),
            (true, true, "40000.5"),
            (false, false, "40000.5"),
        ] {
            context.memory_mut(|memory| {
                if focused {
                    memory.request_focus(field);
                } else {
                    memory.surrender_focus(field);
                }
            });
            let _ = context.run(egui::RawInput::default(), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let galley = layout_property_value(
                        ui,
                        "40000.5",
                        250.0,
                        is_number,
                        field,
                        Color32::WHITE,
                    );
                    assert_eq!(galley.text(), expected);
                });
            });
        }
    }

    #[test]
    fn number_property_calculates_on_enter_and_rejects_invalid_results() {
        for (expression, expected) in [
            ("500+20", Some("520")),
            ("2+3*4", Some("14")),
            ("1/0", None),
        ] {
            let context = egui::Context::default();
            let property = PageProperty {
                id: 7,
                name: "Number".into(),
                kind: "number".into(),
                options: Vec::new(),
                value: String::new(),
            };
            let frame = |events| {
                let mut intents = Vec::new();
                let mut field = egui::Id::NULL;
                let _ = context.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            field = ui.id().with(("property-value", 1_i64, property.id));
                            show_text_or_number_editor(
                                ui,
                                1,
                                &property,
                                250.0,
                                false,
                                Palette::new(false),
                                &mut intents,
                            );
                        });
                    },
                );
                (intents, field)
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
                    Intent::Action(Action::SetPagePropertyValue { value, .. }) => {
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
            let draft = context
                .data(|state| state.get_temp::<String>(field))
                .unwrap();
            assert_eq!(draft, expected.unwrap_or(expression));
            if expected.is_none() {
                assert_eq!(context.memory(|memory| memory.focused()), Some(field));
            }
        }
    }
}
