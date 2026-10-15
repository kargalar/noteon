use eframe::egui::{self, RichText, Ui};

use crate::domain::model::{PROPERTY_TYPES, PageId, PageProperty};
use crate::ui::{Intent, palette::Palette, small_label};
use crate::worker::Action;

#[derive(Clone, Copy)]
pub struct PagePropertyTarget {
    pub page: PageId,
    pub widget_scope: Option<u64>,
    pub indent: f32,
}

pub fn show(
    ui: &mut Ui,
    target: PagePropertyTarget,
    created_at: i64,
    properties: &[PageProperty],
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let PagePropertyTarget {
        page,
        widget_scope,
        indent,
    } = target;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.vertical(|ui| {
            ui.set_max_width(440.0);
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
            ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
            ui.push_id(("page-properties", page), |ui| {
                ui.horizontal(|ui| {
                    small_label(ui, "Created", palette.muted);
                    ui.add_sized(
                        [120.0, 20.0],
                        egui::Label::new(crate::ui::database::format_date(created_at)),
                    );
                });

                for property in properties {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        property_row(ui, page, property, busy, palette, intents);
                    });
                }
            });
            add_property_button(ui, page, widget_scope, busy, palette, intents);
        });
    });
    ui.add_space(3.0);
}

fn property_row(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let mut name = read_draft(ui, ("property-name", page, property.id), &property.name);
    let name_response = ui.add_enabled(
        !busy,
        egui::TextEdit::singleline(&mut name)
            .hint_text("Property name")
            .margin(egui::Margin::symmetric(4, 2))
            .desired_width(105.0),
    );
    save_draft(
        ui,
        ("property-name", page, property.id),
        &name,
        name_response.changed(),
    );
    if name_response.lost_focus() && name.trim() != property.name {
        intents.push(Intent::Action(Action::RenamePageProperty {
            page,
            property: property.id,
            name,
        }));
    }

    property_value_editor(ui, page, property, busy, intents);
    ui.menu_button("···", |ui| {
        ui.label("Type");
        for (kind, label) in PROPERTY_TYPES {
            if ui.selectable_label(property.kind == kind, label).clicked() {
                intents.push(Intent::Action(Action::SetPagePropertyType {
                    page,
                    property: property.id,
                    kind: kind.into(),
                }));
                ui.close_menu();
            }
        }
        if matches!(property.kind.as_str(), "select" | "status" | "multi-select") {
            ui.separator();
            let options_id = ("property-options", page, property.id);
            let mut options = read_draft(ui, options_id, &property.options.join(", "));
            ui.label("Options (comma-separated)");
            let response = ui.text_edit_singleline(&mut options);
            save_draft(ui, options_id, &options, response.changed());
            if ui.button("Save options").clicked() {
                intents.push(Intent::Action(Action::SetPagePropertyOptions {
                    page,
                    property: property.id,
                    options: split_options(&options),
                }));
                ui.close_menu();
            }
        }
        ui.separator();
        if ui
            .button(RichText::new("Delete property").color(palette.danger))
            .clicked()
        {
            intents.push(Intent::Action(Action::DeletePageProperty {
                page,
                property: property.id,
            }));
            ui.close_menu();
        }
    });
}

fn property_value_editor(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    busy: bool,
    intents: &mut Vec<Intent>,
) {
    let value_id = ("property-value", page, property.id);
    let mut value = read_draft(ui, value_id, &property.value);
    match property.kind.as_str() {
        "checkbox" => {
            let mut checked = property.value == "true";
            if ui
                .add_enabled(!busy, egui::Checkbox::new(&mut checked, ""))
                .changed()
            {
                set_value(intents, page, property.id, checked.to_string());
            }
        }
        "select" | "status" => {
            ui.add_enabled_ui(!busy, |ui| {
                egui::ComboBox::from_id_salt(("property-value-select", page, property.id))
                    .selected_text(if property.value.is_empty() {
                        "Select"
                    } else {
                        &property.value
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(property.value.is_empty(), "—")
                            .clicked()
                        {
                            set_value(intents, page, property.id, String::new());
                        }
                        for option in &property.options {
                            if ui
                                .selectable_label(property.value == *option, option)
                                .clicked()
                            {
                                set_value(intents, page, property.id, option.clone());
                            }
                        }
                    })
            });
        }
        "multi-select" => {
            let selected: Vec<_> = property
                .value
                .split('|')
                .filter(|item| !item.is_empty())
                .collect();
            let label = if selected.is_empty() {
                "Select".to_owned()
            } else {
                selected.join(", ")
            };
            ui.add_enabled_ui(!busy, |ui| {
                ui.menu_button(label, |ui| {
                    if property.options.is_empty() {
                        ui.label("Add an option first.");
                    }
                    for option in &property.options {
                        let mut checked = selected.contains(&option.as_str());
                        if ui.checkbox(&mut checked, option).changed() {
                            let mut updated: Vec<String> =
                                selected.iter().map(|value| (*value).to_owned()).collect();
                            if checked {
                                if !updated.iter().any(|value| value == option) {
                                    updated.push(option.clone());
                                }
                            } else {
                                updated.retain(|value| value != option);
                            }
                            set_value(intents, page, property.id, updated.join("|"));
                        }
                    }
                })
            });
        }
        _ => {
            let hint = if property.kind == "number" {
                "Number"
            } else if property.kind == "date" {
                "YYYY-AA-GG"
            } else {
                "Value"
            };
            let response = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut value)
                    .hint_text(hint)
                    .margin(egui::Margin::symmetric(4, 2))
                    .desired_width(140.0),
            );
            save_draft(ui, value_id, &value, response.changed());
            if response.lost_focus() && value != property.value {
                set_value(intents, page, property.id, value);
            }
        }
    }
}

fn add_property_button(
    ui: &mut Ui,
    page: PageId,
    widget_scope: Option<u64>,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let widget_scope = widget_scope.unwrap_or_default();
    let form_id = egui::Id::new(("new-page-property-open", page, widget_scope));
    let mut is_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(form_id))
        .unwrap_or(false);
    if ui
        .small_button(RichText::new("+ Property").color(palette.accent))
        .clicked()
    {
        is_open = !is_open;
        ui.ctx().data_mut(|data| data.insert_temp(form_id, is_open));
    }

    if !is_open {
        return;
    }

    let name_id = egui::Id::new(("new-page-property-name", page, widget_scope));
    let kind_id = egui::Id::new(("new-page-property-kind", page, widget_scope));
    let options_id = egui::Id::new(("new-page-property-options", page, widget_scope));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("New property");
            if ui.small_button("Cancel").clicked() {
                ui.ctx().data_mut(|data| data.insert_temp(form_id, false));
            }
        });

        let mut name = ui
            .ctx()
            .data(|data| data.get_temp::<String>(name_id))
            .unwrap_or_default();
        ui.label("Ad");
        if ui.text_edit_singleline(&mut name).changed() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(name_id, name.clone()));
        }

        let mut kind = ui
            .ctx()
            .data(|data| data.get_temp::<String>(kind_id))
            .unwrap_or_else(|| "text".to_owned());
        egui::ComboBox::from_id_salt(("new-property-kind-choice", page, widget_scope))
            .selected_text(type_label(&kind))
            .show_ui(ui, |ui| {
                for (candidate, label) in PROPERTY_TYPES {
                    ui.selectable_value(&mut kind, candidate.to_owned(), label);
                }
            });
        ui.ctx()
            .data_mut(|data| data.insert_temp(kind_id, kind.clone()));

        let mut options = ui
            .ctx()
            .data(|data| data.get_temp::<String>(options_id))
            .unwrap_or_default();
        if matches!(kind.as_str(), "select" | "status" | "multi-select") {
            ui.label("Options (comma-separated)");
            if ui.text_edit_singleline(&mut options).changed() {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(options_id, options.clone()));
            }
        }

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy && !name.trim().is_empty(), egui::Button::new("Add"))
                .clicked()
            {
                intents.push(Intent::Action(Action::AddPageProperty {
                    page,
                    name: name.trim().to_owned(),
                    kind,
                    options: split_options(&options),
                }));
                ui.ctx().data_mut(|data| {
                    data.remove::<String>(name_id);
                    data.remove::<String>(kind_id);
                    data.remove::<String>(options_id);
                    data.insert_temp(form_id, false);
                });
            }
        });
    });
}

fn set_value(intents: &mut Vec<Intent>, page: PageId, property: i64, value: String) {
    intents.push(Intent::Action(Action::SetPagePropertyValue {
        page,
        property,
        value,
    }));
}

fn read_draft(ui: &Ui, key: impl std::hash::Hash, initial: &str) -> String {
    let id = ui.id().with(key);
    ui.ctx()
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| initial.to_owned())
}

fn save_draft(ui: &Ui, key: impl std::hash::Hash, value: &str, changed: bool) {
    if changed {
        let id = ui.id().with(key);
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, value.to_owned()));
    }
}

fn split_options(options: &str) -> Vec<String> {
    options
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect()
}

fn type_label(kind: &str) -> &'static str {
    PROPERTY_TYPES
        .iter()
        .find_map(|(candidate, label)| (*candidate == kind).then_some(*label))
        .unwrap_or("Metin")
}
