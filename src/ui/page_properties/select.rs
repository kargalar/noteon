use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Ui, vec2};

use crate::domain::model::{PageId, PageProperty, PropertyOption};
use crate::ui::{Intent, palette::Palette};
use crate::worker::Action;

use super::appearance::{
    option_base_color, option_pill_colors, resolve_option_color, show_color_palette_grid,
};
use super::drafts::{clear_draft, read_draft, save_draft};
use super::set_value;

pub(super) fn show_single_option_editor(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    _busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let is_empty = property.value.is_empty();
    let current_color = resolve_option_color(property.option_color(&property.value));
    let (pill_bg, pill_border, pill_text) = if is_empty {
        (Color32::TRANSPARENT, Color32::TRANSPARENT, palette.muted)
    } else {
        option_pill_colors(palette, &property.kind, &property.value, current_color)
    };
    let display_label = if is_empty {
        "Empty".to_owned()
    } else if property.kind == "status" {
        format!("●  {}", property.value)
    } else {
        property.value.clone()
    };

    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(8.0, 3.0);
        ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_bg;
        ui.style_mut().visuals.widgets.inactive.bg_fill = pill_bg;
        ui.style_mut().visuals.widgets.inactive.bg_stroke = if is_empty {
            Stroke::NONE
        } else {
            Stroke::new(1.0_f32, pill_border)
        };
        ui.persistent_menu_button(
            RichText::new(display_label)
                .font(FontId::proportional(12.5))
                .color(pill_text),
            |ui| {
                ui.set_min_width(215.0);
                if !is_empty
                    && ui
                        .selectable_label(false, RichText::new("—  Clear").color(palette.muted))
                        .clicked()
                {
                    set_value(intents, page, property.id, String::new());
                    ui.close_menu();
                }
                for (idx, option) in property.options.iter().enumerate() {
                    ui.push_id(("single-opt-item", idx), |ui| {
                        let selected = property.value == option.name;
                        let opt_color = resolve_option_color(option.color.as_deref());
                        let (_, _, opt_text_col) =
                            option_pill_colors(palette, &property.kind, &option.name, opt_color);
                        let dot_color =
                            option_base_color(palette, &property.kind, &option.name, opt_color);
                        ui.horizontal(|ui| {
                            ui.persistent_menu_button(
                                RichText::new("●")
                                    .font(FontId::proportional(13.0))
                                    .color(dot_color),
                                |ui| {
                                    show_color_palette_grid(ui, palette, opt_color, |new_color| {
                                        let mut opts = property.options.clone();
                                        if let Some(target) =
                                            opts.iter_mut().find(|o| o.name == option.name)
                                        {
                                            target.color = new_color;
                                            intents.push(Intent::Action(
                                                Action::SetPagePropertyOptions {
                                                    page,
                                                    property: property.id,
                                                    options: opts,
                                                },
                                            ));
                                        }
                                    });
                                },
                            )
                            .response
                            .on_hover_text("Change color");

                            let item_text = if property.kind == "status" {
                                format!("●  {}", option.name)
                            } else {
                                option.name.clone()
                            };
                            if ui
                                .selectable_label(
                                    selected,
                                    RichText::new(item_text).color(opt_text_col),
                                )
                                .clicked()
                            {
                                set_value(intents, page, property.id, option.name.clone());
                                ui.close_menu();
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
                                        .on_hover_text(format!("Delete '{}' option", option.name))
                                        .clicked()
                                    {
                                        intents.push(Intent::Action(
                                            Action::RemovePagePropertyOption {
                                                page,
                                                property: property.id,
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
                let quick_id = ("page-prop-quick-opt", page, property.id);
                let mut new_opt = read_draft(ui, quick_id, "");
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut new_opt)
                            .hint_text("Create option…")
                            .desired_width(140.0),
                    );
                    save_draft(ui, quick_id, &new_opt, resp.changed());
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (enter || ui.small_button("+").clicked()) && !new_opt.trim().is_empty() {
                        let trimmed = new_opt.trim().to_owned();
                        let mut opts = property.options.clone();
                        if !opts.iter().any(|o| o.name == trimmed) {
                            opts.push(PropertyOption::new(&trimmed));
                            intents.push(Intent::Action(Action::SetPagePropertyOptions {
                                page,
                                property: property.id,
                                options: opts,
                            }));
                        }
                        set_value(intents, page, property.id, trimmed);
                        clear_draft(ui, quick_id);
                        ui.close_menu();
                    }
                });
            },
        );
    });
}

pub(super) fn show_multi_option_editor(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    available_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let selected: Vec<String> = property
        .value
        .split('|')
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect();

    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(available_width);
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        ui.style_mut().visuals.widgets.active.bg_stroke = Stroke::NONE;
        ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;

        for (idx, item) in selected.iter().enumerate() {
            ui.push_id(("multi-pill", idx), |ui| {
                let item_color = resolve_option_color(property.option_color(item));
                let (pill_bg, _pill_border, pill_text) =
                    option_pill_colors(palette, "multi-select", item, item_color);
                let pill_btn = egui::Button::new(
                    RichText::new(format!("{item}  ✕"))
                        .font(FontId::proportional(12.0))
                        .color(pill_text),
                )
                .fill(pill_bg)
                .stroke(Stroke::NONE)
                .corner_radius(4);

                let resp = ui.add_enabled(!busy, pill_btn).on_hover_text(format!(
                    "Click to remove '{item}' · Right-click to change color"
                ));
                if resp.clicked() {
                    let mut updated = selected.clone();
                    updated.retain(|candidate| candidate != item);
                    set_value(intents, page, property.id, updated.join("|"));
                }
                resp.persistent_context_menu(|ui| {
                    show_color_palette_grid(ui, palette, item_color, |new_color| {
                        let mut opts = property.options.clone();
                        if let Some(target) = opts.iter_mut().find(|o| o.name == *item) {
                            target.color = new_color;
                            intents.push(Intent::Action(Action::SetPagePropertyOptions {
                                page,
                                property: property.id,
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
            ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            ui.spacing_mut().button_padding = vec2(6.0, 2.0);
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
                        set_value(intents, page, property.id, String::new());
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
                        ui.push_id(("multi-opt-item", idx), |ui| {
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
                                                        Action::SetPagePropertyOptions {
                                                            page,
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

                                if ui.checkbox(&mut checked, &option.name).changed() {
                                    let mut updated = selected.clone();
                                    if checked {
                                        if !updated.contains(&option.name) {
                                            updated.push(option.name.clone());
                                        }
                                    } else {
                                        updated.retain(|value| value != &option.name);
                                    }
                                    set_value(intents, page, property.id, updated.join("|"));
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
                                                Action::RemovePagePropertyOption {
                                                    page,
                                                    property: property.id,
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
                    let quick_id = ("page-prop-multi-quick-opt", page, property.id);
                    let mut new_opt = read_draft(ui, quick_id, "");
                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut new_opt)
                                .hint_text("Create option…")
                                .desired_width(160.0),
                        );
                        save_draft(ui, quick_id, &new_opt, resp.changed());
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (enter || ui.small_button("+").clicked()) && !new_opt.trim().is_empty() {
                            let trimmed = new_opt.trim().to_owned();
                            let mut opts = property.options.clone();
                            if !opts.iter().any(|o| o.name == trimmed) {
                                opts.push(PropertyOption::new(&trimmed));
                                intents.push(Intent::Action(Action::SetPagePropertyOptions {
                                    page,
                                    property: property.id,
                                    options: opts,
                                }));
                            }
                            let mut updated = selected.clone();
                            if !updated.contains(&trimmed) {
                                updated.push(trimmed);
                            }
                            set_value(intents, page, property.id, updated.join("|"));
                            clear_draft(ui, quick_id);
                        }
                    });
                },
            );
        });
    });
}
