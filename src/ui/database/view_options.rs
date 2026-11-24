use crate::ui::menus::PersistentMenuUi;

use eframe::egui::{self, Ui};

use crate::domain::model::{DatabaseData, DatabaseProperty, DatabaseView, NewRecordOpenMode};
use crate::worker::Action;

use crate::ui::{Intent, ThemeSection};

pub(super) fn sort_key_label(data: &DatabaseData) -> String {
    match data.sort_key.as_str() {
        "manual" => "Custom".to_owned(),
        "title" => "Name".to_owned(),
        "created_at" => "Created".to_owned(),
        key => key
            .parse::<i64>()
            .ok()
            .and_then(|id| data.properties.iter().find(|property| property.id == id))
            .map_or_else(|| "Property".to_owned(), |property| property.name.clone()),
    }
}

pub(super) fn show_view_options(ui: &mut Ui, data: &mut DatabaseData, intents: &mut Vec<Intent>) {
    let mut changed = false;
    let properties = &data.properties;
    let board_properties: Vec<_> = properties
        .iter()
        .filter(|property| property.kind != "status" && property.kind != "select")
        .cloned()
        .collect();
    let view = data.view;
    let settings = &mut data.view_settings;
    ui.persistent_menu_button("View options", |ui| {
        ui.set_min_width(240.0);
        ui.horizontal(|ui| {
            ui.label("Yeni öğe açılışı");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt(("new-record-open-mode", data.page_id))
                    .selected_text(settings.new_record_open_mode.label())
                    .show_ui(ui, |ui| {
                        for mode in NewRecordOpenMode::ALL {
                            if ui
                                .selectable_label(
                                    settings.new_record_open_mode == mode,
                                    mode.label(),
                                )
                                .clicked()
                                && settings.new_record_open_mode != mode
                            {
                                settings.new_record_open_mode = mode;
                                changed = true;
                            }
                        }
                    });
            });
        });
        ui.separator();
        match view {
            DatabaseView::Table => {
                changed |= ui
                    .checkbox(&mut settings.table_show_created_at, "Show created date")
                    .changed();
                ui.separator();
                ui.label("Visible properties");
                changed |= property_visibility_options(
                    ui,
                    properties,
                    &mut settings.table_visible_properties,
                    usize::MAX,
                );
            }
            DatabaseView::Gallery => {
                ui.horizontal(|ui| {
                    ui.label("Items per row");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt(("gallery-columns-options", data.page_id))
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
                                        data.gallery_columns = columns;
                                        intents.push(Intent::Action(
                                            Action::SetDatabaseGalleryColumns {
                                                page: data.page_id,
                                                columns,
                                            },
                                        ));
                                    }
                                }
                            });
                    });
                });
                ui.separator();
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
                    &board_properties,
                    &mut settings.board_visible_properties,
                    2,
                );
            }
        }
        ui.separator();
        let current_section = match view {
            DatabaseView::Gallery => ThemeSection::Gallery,
            DatabaseView::Board => ThemeSection::Board,
            DatabaseView::Table => ThemeSection::Table,
        };
        if ui.button("🎨 Temayı düzenle").clicked() {
            intents.push(Intent::OpenSettingsTheme(current_section));
            ui.close_menu();
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
