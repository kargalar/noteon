use crate::ui::menus::PersistentMenuUi;

use eframe::egui::{self, FontId, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseProperty, PropertyOption};
use crate::ui::page_properties::{
    option_base_color, property_icon, resolve_option_color, show_color_palette_grid,
};
use crate::worker::Action;

use crate::ui::{Intent, palette::Palette};

use super::Drafts;

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

    ui.add_space(4.0);
    if let Some(new_kind) = crate::ui::page_properties::show_property_type_selector_button(
        ui,
        ("db-prop-kind-picker", property.id),
        &property.kind,
        busy,
        palette,
    ) {
        intents.push(Intent::Action(Action::SetDatabasePropertyType {
            property: property.id,
            kind: new_kind.into(),
        }));
    }

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
                        ui.push_id(
                            ("prop-edit-opt-row", property.id, opt_idx, &option.name),
                            |ui| {
                                let opt_draft_id = egui::Id::new((
                                    "prop-option-edit",
                                    property.id,
                                    opt_idx,
                                    &option.name,
                                ));
                                let mut opt_text = ui
                                    .ctx()
                                    .data(|d| d.get_temp::<String>(opt_draft_id))
                                    .unwrap_or_else(|| option.name.clone());
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
                                            .font(FontId::proportional(14.0))
                                            .color(dot_color),
                                        |ui| {
                                            show_color_palette_grid(
                                                ui,
                                                palette,
                                                opt_color,
                                                |new_color| {
                                                    let mut reordered = property.options.clone();
                                                    if let Some(target) = reordered.get_mut(opt_idx)
                                                    {
                                                        target.color = new_color;
                                                        intents.push(Intent::Action(
                                                            Action::SetDatabasePropertyOptions {
                                                                property: property.id,
                                                                options: reordered,
                                                            },
                                                        ));
                                                    }
                                                },
                                            );
                                        },
                                    )
                                    .response
                                    .on_hover_text("Change color");

                                    let resp = ui.add_enabled(
                                        !busy,
                                        egui::TextEdit::singleline(&mut opt_text)
                                            .desired_width(145.0),
                                    );
                                    if resp.changed() {
                                        ui.ctx().data_mut(|d| {
                                            d.insert_temp(opt_draft_id, opt_text.clone())
                                        });
                                    }
                                    let commit = resp.lost_focus()
                                        || (resp.has_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                    if commit
                                        && !opt_text.trim().is_empty()
                                        && opt_text.trim() != option.name
                                    {
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
                                        && ui.small_button("↑").on_hover_text("Move up").clicked()
                                    {
                                        let mut reordered = property.options.clone();
                                        reordered.swap(opt_idx, opt_idx - 1);
                                        intents.push(Intent::Action(
                                            Action::SetDatabasePropertyOptions {
                                                property: property.id,
                                                options: reordered,
                                            },
                                        ));
                                    }
                                    if opt_idx + 1 < property.options.len()
                                        && ui.small_button("↓").on_hover_text("Move down").clicked()
                                    {
                                        let mut reordered = property.options.clone();
                                        reordered.swap(opt_idx, opt_idx + 1);
                                        intents.push(Intent::Action(
                                            Action::SetDatabasePropertyOptions {
                                                property: property.id,
                                                options: reordered,
                                            },
                                        ));
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
                            },
                        );
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
            let enter_add = new_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add_clicked = ui
                .add_enabled(
                    !busy && !new_opt.trim().is_empty(),
                    egui::Button::new("+ Add"),
                )
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

pub fn add_property_menu(ui: &mut Ui, data: &DatabaseData, intents: &mut Vec<Intent>) {
    let mut palette = Palette::new(ui.visuals().dark_mode);
    palette.background = ui.visuals().window_fill;
    palette.text = ui.visuals().text_color();
    palette.hover = ui.visuals().widgets.hovered.bg_fill;
    palette.accent_soft = ui.visuals().selection.bg_fill;
    palette.accent = ui.visuals().selection.stroke.color;

    let name_id = egui::Id::new(("new-database-property-name", data.page_id));
    let mut name = ui
        .ctx()
        .data(|d| d.get_temp::<String>(name_id))
        .unwrap_or_default();

    let name_resp = ui.add(
        egui::TextEdit::singleline(&mut name)
            .hint_text("Type a property name…")
            .desired_width(296.0),
    );
    if name_resp.changed() {
        ui.ctx().data_mut(|d| d.insert_temp(name_id, name.clone()));
    }

    let enter_pressed = name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if enter_pressed && !name.trim().is_empty() {
        let final_name = crate::ui::property_names::unique_name(
            data.properties
                .iter()
                .map(|property| property.name.as_str()),
            name.trim(),
        );
        intents.push(Intent::Action(Action::AddDatabaseProperty {
            database: data.page_id,
            name: final_name,
            kind: "text".into(),
            options: Vec::new(),
        }));
        ui.ctx().data_mut(|d| d.remove::<String>(name_id));
        ui.close_menu();
        return;
    }

    ui.separator();

    if let Some(kind) = crate::ui::page_properties::show_property_type_picker(
        ui,
        ("add-db-prop-type-picker", data.page_id),
        None,
        false,
        palette,
    ) {
        let base_name = if name.trim().is_empty() {
            crate::ui::page_properties::default_name_for_kind(kind)
        } else {
            name.trim()
        };
        let final_name = crate::ui::property_names::unique_name(
            data.properties
                .iter()
                .map(|property| property.name.as_str()),
            base_name,
        );
        let options = if kind == "status" {
            crate::domain::model::DEFAULT_DATABASE_STATUS_OPTIONS
                .iter()
                .map(|s| PropertyOption::new(*s))
                .collect()
        } else {
            Vec::new()
        };
        intents.push(Intent::Action(Action::AddDatabaseProperty {
            database: data.page_id,
            name: final_name,
            kind: kind.into(),
            options,
        }));
        ui.ctx().data_mut(|d| d.remove::<String>(name_id));
        ui.close_menu();
    }
}
