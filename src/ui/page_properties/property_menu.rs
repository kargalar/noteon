use crate::ui::menus::PersistentMenuUi;
use eframe::egui::{self, FontId, RichText, Ui, vec2};

use crate::domain::model::{DEFAULT_DATABASE_STATUS_OPTIONS, PageId, PageProperty, PropertyOption};
use crate::ui::{Intent, palette::Palette};
use crate::worker::Action;

use super::appearance::{
    option_base_color, property_icon, resolve_option_color, show_color_palette_grid,
};
use super::drafts::{clear_draft, read_draft, save_draft};
use super::set_value;
use super::type_picker::{
    default_name_for_kind, show_property_type_picker, show_property_type_selector_button,
};

#[allow(clippy::too_many_arguments)]
pub fn show_property_edit_panel(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    index: usize,
    total: usize,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    ui.set_min_width(250.0);
    ui.spacing_mut().item_spacing = vec2(6.0, 5.0);

    ui.label(
        RichText::new("EDIT PROPERTY")
            .font(FontId::proportional(10.5))
            .strong()
            .color(palette.muted),
    );

    // 1. Property Name Editor
    let draft_key = ("property-name-edit", page, property.id);
    let mut name = read_draft(ui, draft_key, &property.name);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(property_icon(&property.kind))
                .font(FontId::proportional(13.0))
                .color(palette.muted),
        );
        let name_changed = name.trim() != property.name && !name.trim().is_empty();
        let input_w = if name_changed { 175.0 } else { 215.0 };
        let resp = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut name)
                .hint_text("Property name")
                .desired_width(input_w),
        );
        save_draft(ui, draft_key, &name, resp.changed());
        let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let apply_clicked = name_changed
            && ui
                .small_button(RichText::new("✓").color(palette.accent).strong())
                .on_hover_text("Save name")
                .clicked();
        if (resp.lost_focus() || enter_pressed || apply_clicked) && name_changed {
            intents.push(Intent::Action(Action::RenamePageProperty {
                page,
                property: property.id,
                name: name.trim().to_owned(),
            }));
            clear_draft(ui, draft_key);
        }
    });

    ui.add_space(2.0);

    // 2. Property Type Selector
    if let Some(new_kind) = show_property_type_selector_button(
        ui,
        ("page-prop-kind-picker", page, property.id),
        &property.kind,
        busy,
        palette,
    ) {
        intents.push(Intent::Action(Action::SetPagePropertyType {
            page,
            property: property.id,
            kind: new_kind.into(),
        }));
    }

    // 3. Options Manager for Select / Status / Multi-select
    if matches!(property.kind.as_str(), "select" | "status" | "multi-select") {
        ui.separator();
        ui.label(
            RichText::new("Options")
                .font(FontId::proportional(11.5))
                .color(palette.muted),
        );
        if property.options.is_empty() {
            ui.label(
                RichText::new("No options yet. Add one below.")
                    .small()
                    .italics()
                    .color(palette.muted),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt(("page-prop-options-scroll", page, property.id))
                .max_height(175.0)
                .show(ui, |ui| {
                    for (idx, opt) in property.options.iter().enumerate() {
                        ui.push_id(("page-prop-opt-row", idx), |ui| {
                            let opt_id = ("page-prop-opt-edit", page, property.id, idx, &opt.name);
                            let mut edited_opt = read_draft(ui, opt_id, &opt.name);
                            let opt_color = resolve_option_color(opt.color.as_deref());
                            let dot_color =
                                option_base_color(palette, &property.kind, &opt.name, opt_color);
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
                                                if let Some(target) = reordered.get_mut(idx) {
                                                    target.color = new_color;
                                                    intents.push(Intent::Action(
                                                        Action::SetPagePropertyOptions {
                                                            page,
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
                                    egui::TextEdit::singleline(&mut edited_opt)
                                        .desired_width(135.0),
                                );
                                save_draft(ui, opt_id, &edited_opt, resp.changed());
                                let commit = resp.lost_focus()
                                    || (resp.has_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if commit
                                    && !edited_opt.trim().is_empty()
                                    && edited_opt.trim() != opt.name
                                {
                                    intents.push(Intent::Action(
                                        Action::RenamePagePropertyOption {
                                            page,
                                            property: property.id,
                                            old_option: opt.name.clone(),
                                            new_option: edited_opt.trim().to_owned(),
                                        },
                                    ));
                                    clear_draft(ui, opt_id);
                                }
                                if idx > 0
                                    && ui
                                        .small_button("↑")
                                        .on_hover_text("Move option up")
                                        .clicked()
                                {
                                    let mut reordered = property.options.clone();
                                    reordered.swap(idx, idx - 1);
                                    intents.push(Intent::Action(Action::SetPagePropertyOptions {
                                        page,
                                        property: property.id,
                                        options: reordered,
                                    }));
                                }
                                if idx + 1 < property.options.len()
                                    && ui
                                        .small_button("↓")
                                        .on_hover_text("Move option down")
                                        .clicked()
                                {
                                    let mut reordered = property.options.clone();
                                    reordered.swap(idx, idx + 1);
                                    intents.push(Intent::Action(Action::SetPagePropertyOptions {
                                        page,
                                        property: property.id,
                                        options: reordered,
                                    }));
                                }
                                if ui
                                    .small_button(RichText::new("✕").color(palette.danger))
                                    .on_hover_text("Delete option")
                                    .clicked()
                                {
                                    intents.push(Intent::Action(
                                        Action::RemovePagePropertyOption {
                                            page,
                                            property: property.id,
                                            option: opt.name.clone(),
                                        },
                                    ));
                                }
                            });
                        });
                    }
                });
        }

        let new_opt_id = ("page-prop-new-opt", page, property.id);
        let mut new_opt = read_draft(ui, new_opt_id, "");
        ui.horizontal(|ui| {
            let resp = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut new_opt)
                    .hint_text("New option…")
                    .desired_width(175.0),
            );
            save_draft(ui, new_opt_id, &new_opt, resp.changed());
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add_clicked = ui
                .add_enabled(
                    !busy && !new_opt.trim().is_empty(),
                    egui::Button::new("+ Add"),
                )
                .clicked();
            if (enter || add_clicked) && !new_opt.trim().is_empty() {
                let mut updated = property.options.clone();
                for part in split_options(&new_opt) {
                    if !updated.iter().any(|o| o.name == part) {
                        updated.push(PropertyOption::new(part));
                    }
                }
                intents.push(Intent::Action(Action::SetPagePropertyOptions {
                    page,
                    property: property.id,
                    options: updated,
                }));
                clear_draft(ui, new_opt_id);
            }
        });
    }

    // 4. Reorder & Property Actions
    ui.separator();
    if total > 1 {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy && index > 0, egui::Button::new("↑ Move up"))
                .clicked()
            {
                intents.push(Intent::Action(Action::MovePageProperty {
                    page,
                    property: property.id,
                    direction: -1,
                }));
            }
            if ui
                .add_enabled(!busy && index + 1 < total, egui::Button::new("↓ Move down"))
                .clicked()
            {
                intents.push(Intent::Action(Action::MovePageProperty {
                    page,
                    property: property.id,
                    direction: 1,
                }));
            }
        });
    }

    let has_value = if property.kind == "checkbox" {
        property.value == "true"
    } else {
        !property.value.is_empty()
    };
    if has_value
        && ui
            .button(RichText::new("⌫  Clear value").color(palette.muted))
            .clicked()
    {
        let cleared = if property.kind == "checkbox" {
            "false".to_owned()
        } else {
            String::new()
        };
        set_value(intents, page, property.id, cleared);
        clear_draft(ui, ("property-value", page, property.id));
        ui.close_menu();
    }

    if ui
        .button(RichText::new("🗑  Delete property").color(palette.danger))
        .clicked()
    {
        intents.push(Intent::Action(Action::DeletePageProperty {
            page,
            property: property.id,
        }));
        ui.close_menu();
    }
}

pub fn show_add_property_panel(
    ui: &mut Ui,
    page: PageId,
    scope_id: u64,
    existing_properties: &[PageProperty],
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    ui.spacing_mut().item_spacing = vec2(6.0, 4.0);

    let name_id = ("new-page-property-name", page, scope_id);
    let mut name = read_draft(ui, name_id, "");
    let name_resp = ui.add_enabled(
        !busy,
        egui::TextEdit::singleline(&mut name)
            .hint_text("Type a property name…")
            .desired_width(296.0),
    );
    save_draft(ui, name_id, &name, name_resp.changed());

    let enter_pressed = name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if enter_pressed && !name.trim().is_empty() {
        let final_name = crate::ui::property_names::unique_name(
            existing_properties
                .iter()
                .map(|property| property.name.as_str()),
            name.trim(),
        );
        intents.push(Intent::Action(Action::AddPageProperty {
            page,
            name: final_name,
            kind: "text".into(),
            options: Vec::new(),
        }));
        clear_draft(ui, name_id);
        ui.close_menu();
        return;
    }

    ui.separator();

    if let Some(kind) = show_property_type_picker(
        ui,
        ("add-page-prop-type-picker", page, scope_id),
        None,
        busy,
        palette,
    ) {
        let base_name = if name.trim().is_empty() {
            default_name_for_kind(kind)
        } else {
            name.trim()
        };
        let final_name = crate::ui::property_names::unique_name(
            existing_properties
                .iter()
                .map(|property| property.name.as_str()),
            base_name,
        );
        let options = if kind == "status" {
            DEFAULT_DATABASE_STATUS_OPTIONS
                .iter()
                .map(|s| PropertyOption::new(*s))
                .collect()
        } else {
            Vec::new()
        };
        intents.push(Intent::Action(Action::AddPageProperty {
            page,
            name: final_name,
            kind: kind.into(),
            options,
        }));
        clear_draft(ui, name_id);
        ui.close_menu();
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
