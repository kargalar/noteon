mod board;
mod cards;
mod layout;
mod property_menu;
mod property_value;
mod record_properties;
mod shared;
mod table;
mod table_columns;
mod table_filter;
mod table_footer;
mod table_navigation;
mod view_options;

use crate::ui::menus::PersistentMenuUi;

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseView};
use crate::worker::Action;

use super::{Intent, palette::Palette, primary_button};

use board::show_board;
use cards::show_gallery;
pub use layout::desired_embedded_height;
pub use property_menu::{add_property_menu, show_property_edit_menu};
pub use record_properties::{
    RecordPropertiesTarget, show_record_properties, show_record_property_rows,
};
pub(crate) use shared::format_date;
use table::{delete_selected_table_rows, show_table};
use table_filter::show_table_find_bar;
use view_options::{show_view_options, sort_key_label};

pub struct Drafts<'a> {
    pub properties: &'a mut std::collections::HashMap<i64, (String, String)>,
    pub titles: &'a mut std::collections::HashMap<i64, String>,
    pub values: &'a mut std::collections::HashMap<(i64, i64), String>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the public database renderer API"
)]
pub fn show(
    ui: &mut Ui,
    data: &mut DatabaseData,
    page_icon: Option<&str>,
    breadcrumbs: &[crate::ui::BreadcrumbItem],
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    egui::ScrollArea::vertical()
        .id_salt(("database-full-page-scroll", data.page_id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_inner(
                ui,
                data,
                DatabaseDisplay::Page {
                    icon: page_icon,
                    breadcrumbs,
                    busy,
                },
                palette,
                intents,
                drafts,
            );
        });
}

pub fn show_embedded(
    ui: &mut Ui,
    data: &mut DatabaseData,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    show_inner(
        ui,
        data,
        DatabaseDisplay::Embedded,
        palette,
        intents,
        drafts,
    );
}

enum DatabaseDisplay<'a> {
    Page {
        icon: Option<&'a str>,
        breadcrumbs: &'a [crate::ui::BreadcrumbItem],
        busy: bool,
    },
    Embedded,
}

fn show_inner(
    ui: &mut Ui,
    data: &mut DatabaseData,
    display: DatabaseDisplay<'_>,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let (page_icon, breadcrumbs, busy) = match display {
        DatabaseDisplay::Page {
            icon,
            breadcrumbs,
            busy,
        } => (icon, breadcrumbs, busy),
        DatabaseDisplay::Embedded => (None, &[][..], false),
    };
    let full_page = matches!(display, DatabaseDisplay::Page { .. });
    let selection_scope = ui.id().with(("database-row-selection", data.page_id));
    if crate::ui::selection::select_all(ui, selection_scope, true) {
        let rows: std::collections::HashSet<_> = data.rows.iter().map(|row| row.page.id).collect();
        ui.ctx().data_mut(|state| {
            state.insert_temp(
                egui::Id::new(("database-selected-rows", data.page_id)),
                rows,
            );
            state.insert_temp(
                egui::Id::new(("database-anchor-row", data.page_id)),
                data.rows.first().map(|row| row.page.id),
            );
        });
    }
    delete_selected_table_rows(ui, data, intents);
    let selected: std::collections::HashSet<i64> = ui
        .ctx()
        .data(|state| state.get_temp(egui::Id::new(("database-selected-rows", data.page_id))))
        .unwrap_or_default();
    if !selected.is_empty()
        && crate::ui::selection::is_active(ui, selection_scope)
        && !ui.ctx().wants_keyboard_input()
        && let Some(cut) = crate::ui::page_transfer::clipboard_shortcut(ui)
    {
        let rows: Vec<_> = data
            .rows
            .iter()
            .filter(|row| selected.contains(&row.page.id))
            .collect();
        crate::ui::page_transfer::copy_pages(
            ui.ctx(),
            rows.iter().map(|row| row.page.id).collect(),
            &rows
                .iter()
                .map(|row| row.page.display_title().to_owned())
                .collect::<Vec<_>>(),
            cut,
        );
    }
    if !ui.ctx().wants_keyboard_input()
        && crate::ui::selection::is_active(ui, selection_scope)
        && let Some(action) = crate::ui::page_transfer::take_paste(ui, data.page_id, None, None)
    {
        intents.push(Intent::Action(action));
    }
    let content = ui.vertical(|ui| {
        if full_page {
            ui.add_space(20.0);
            if !breadcrumbs.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(44.0);
                    for item in breadcrumbs {
                        let label = if !item.icon.is_empty() {
                            format!("{}  {}", item.icon, item.title)
                        } else {
                            item.title.clone()
                        };
                        let resp = ui.add(
                            egui::Button::new(
                                RichText::new(label)
                                    .font(FontId::proportional(12.0))
                                    .color(palette.muted),
                            )
                            .frame(false),
                        );
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            intents.push(Intent::Open(item.id, None));
                        } else if resp.middle_clicked() {
                            intents.push(Intent::OpenInTab(item.id));
                        }
                        ui.label(
                            RichText::new("/")
                                .font(FontId::proportional(12.0))
                                .color(palette.muted),
                        );
                    }
                    ui.label(
                        RichText::new(&data.title)
                            .font(FontId::proportional(12.0))
                            .color(palette.muted),
                    );
                });
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.add_space(44.0);
                let current_icon = if !data.icon.is_empty() {
                    &data.icon
                } else {
                    page_icon.unwrap_or("▦")
                };
                if let Some(new_icon) = crate::ui::icon_picker_button(
                    ui,
                    ("database-header-icon", data.page_id),
                    current_icon,
                    34.0,
                    Some(palette.accent),
                ) {
                    data.icon = new_icon.clone();
                    intents.push(Intent::Action(Action::SetDatabaseRecordIcon {
                        page: data.page_id,
                        icon: new_icon,
                    }));
                }
                ui.add_space(10.0);
                let title_width = (ui.available_width() - 88.0).max(120.0);
                show_title(ui, data, busy, 30.0, title_width, intents);
            });
            ui.add_space(16.0);
        } else {
            ui.horizontal(|ui| {
                let current_icon = if !data.icon.is_empty() {
                    &data.icon
                } else {
                    "▦"
                };
                if let Some(new_icon) = crate::ui::icon_picker_button(
                    ui,
                    ("database-embedded-icon", data.page_id),
                    current_icon,
                    16.0,
                    Some(palette.accent),
                ) {
                    data.icon = new_icon.clone();
                    intents.push(Intent::Action(Action::SetDatabaseRecordIcon {
                        page: data.page_id,
                        icon: new_icon,
                    }));
                }
                let button_width = 28.0;
                let title_width =
                    (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(0.0);
                show_title(ui, data, busy, 16.0, title_width, intents);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [button_width, 24.0],
                            egui::Button::new(RichText::new("⛶").color(palette.muted)).frame(false),
                        )
                        .on_hover_text("Tam ekranda aç")
                        .clicked()
                    {
                        intents.push(Intent::Open(data.page_id, None));
                    }
                });
            });
            ui.add_space(6.0);
        }

        let horizontal_margin = if full_page { 44 } else { 0 };
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(horizontal_margin, 0))
            .show(ui, |ui| {
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
                                egui::Stroke::new(1.0_f32, palette.accent.linear_multiply(0.4))
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

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let new_btn = primary_button("+ New record", palette);
                            let new_clicked = ui.add_enabled(!busy, new_btn).clicked();
                            if new_clicked {
                                intents.push(Intent::Action(Action::CreateDatabaseRecord(
                                    data.page_id,
                                )));
                            }

                            if data.view != DatabaseView::Table {
                                ui.persistent_menu_button(
                                    RichText::new("+ Property")
                                        .font(FontId::proportional(13.0))
                                        .color(palette.muted),
                                    |ui| {
                                        add_property_menu(ui, data, intents);
                                    },
                                );
                            }

                            show_view_options(ui, data, intents);

                            let sort_name = sort_key_label(data);
                            let direction_arrow = if data.sort_key == "manual" {
                                ""
                            } else if data.sort_descending {
                                " ▼"
                            } else {
                                " ▲"
                            };
                            ui.persistent_menu_button(
                                RichText::new(format!("⇅ Sort: {sort_name}{direction_arrow}"))
                                    .font(FontId::proportional(12.5))
                                    .color(palette.muted),
                                |ui| {
                                    ui.label(RichText::new("Sort by").small().color(palette.muted));
                                    for (label, key) in [
                                        ("Custom", "manual"),
                                        ("Name", "title"),
                                        ("Created", "created_at"),
                                    ] {
                                        let is_selected = data.sort_key == key;
                                        if ui.selectable_label(is_selected, label).clicked() {
                                            if key == "manual" {
                                                intents.push(Intent::Action(
                                                    Action::SetDatabaseSort {
                                                        page: data.page_id,
                                                        key: "manual".into(),
                                                        descending: false,
                                                    },
                                                ));
                                            } else {
                                                toggle_sort(data, key, intents);
                                            }
                                        }
                                    }
                                    for property in &data.properties {
                                        let key = property.id.to_string();
                                        let is_selected = data.sort_key == key;
                                        if ui
                                            .selectable_label(is_selected, &property.name)
                                            .clicked()
                                        {
                                            toggle_sort(data, &key, intents);
                                        }
                                    }
                                    if data.sort_key != "manual" {
                                        ui.separator();
                                        ui.label(
                                            RichText::new("Direction").small().color(palette.muted),
                                        );
                                        if ui
                                            .selectable_label(
                                                !data.sort_descending,
                                                "Ascending (▲)",
                                            )
                                            .clicked()
                                            && data.sort_descending
                                        {
                                            intents.push(Intent::Action(Action::SetDatabaseSort {
                                                page: data.page_id,
                                                key: data.sort_key.clone(),
                                                descending: false,
                                            }));
                                        }
                                        if ui
                                            .selectable_label(
                                                data.sort_descending,
                                                "Descending (▼)",
                                            )
                                            .clicked()
                                            && !data.sort_descending
                                        {
                                            intents.push(Intent::Action(Action::SetDatabaseSort {
                                                page: data.page_id,
                                                key: data.sort_key.clone(),
                                                descending: true,
                                            }));
                                        }
                                    }
                                },
                            );
                        });
                    });

                    if data.view == DatabaseView::Table {
                        show_table_find_bar(ui, data, palette);
                    }

                    ui.add_space(8.0);
                    let divider_stroke =
                        egui::Stroke::new(1.0_f32, palette.border.linear_multiply(0.4));
                    let avail_w = ui.available_width();
                    let (divider_rect, _) =
                        ui.allocate_exact_size(egui::vec2(avail_w, 1.0), egui::Sense::hover());
                    ui.painter().line_segment(
                        [divider_rect.left_top(), divider_rect.right_top()],
                        divider_stroke,
                    );
                    ui.add_space(8.0);

                    match data.view {
                        DatabaseView::Table => show_table(ui, data, busy, palette, intents, drafts),
                        DatabaseView::Gallery => show_gallery(ui, data, busy, palette, intents),
                        DatabaseView::Board => show_board(ui, data, busy, palette, intents),
                    }
                });
            });
    });
    if let Some(payload) =
        egui::DragAndDrop::payload::<crate::ui::page_transfer::PageDragPayload>(ui.ctx())
        && !payload
            .pages
            .iter()
            .all(|id| data.rows.iter().any(|row| row.page.id == *id))
    {
        crate::ui::page_transfer::drop_target(
            ui,
            content.response.rect,
            Some(data.page_id),
            None,
            palette,
            intents,
        );
    }
}

fn show_title(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    font_size: f32,
    width: f32,
    intents: &mut Vec<Intent>,
) {
    let title_id = egui::Id::new(("database-header-title", data.page_id));
    let original_id = title_id.with("original");
    let previous_title = data.title.clone();
    let response = ui.add_enabled(
        !busy,
        egui::TextEdit::singleline(&mut data.title)
            .id(title_id)
            .font(FontId::proportional(font_size))
            .hint_text("Untitled Database")
            .frame(false)
            .margin(egui::Margin::ZERO)
            .desired_width(width),
    );
    if response.changed() {
        ui.ctx().data_mut(|state| {
            state.get_temp_mut_or_insert_with(original_id, || previous_title);
        });
    }
    if response.lost_focus()
        && let Some(original) = ui
            .ctx()
            .data_mut(|state| state.remove_temp::<String>(original_id))
        && original != data.title
    {
        intents.push(Intent::Action(Action::RenameDatabaseRecord {
            page: data.page_id,
            title: data.title.clone(),
        }));
    }
}

fn toggle_sort(data: &DatabaseData, key: &str, intents: &mut Vec<Intent>) {
    intents.push(Intent::Action(Action::SetDatabaseSort {
        page: data.page_id,
        key: key.into(),
        descending: data.sort_key == key && !data.sort_descending,
    }));
}
