use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};
use std::collections::HashMap;

use eframe::egui::{self, FontId, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseRow, PropertyOption};
use crate::ui::page_properties::{option_pill_colors, resolve_option_color};
use crate::worker::Action;

use crate::ui::{Intent, ThemeSection, palette::Palette};

use super::cards::show_cards;
use super::layout::{
    board_max_column_cards_height, resolve_vertical_height, show_vertical_resize_handle,
};

pub(super) const DEFAULT_BOARD_MAX_CARDS_HEIGHT: f32 = 648.0;

#[derive(Clone, Copy)]
struct BoardDragPayload {
    database_id: i64,
    record_id: i64,
}

pub(super) fn show_board(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let status = data
        .properties
        .iter()
        .find(|property| property.kind == "status" || property.kind == "select")
        .cloned();
    let Some(status) = status else {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("No Status or Select property found to group columns.")
                    .color(palette.muted),
            );
            if ui
                .add_enabled(!busy, egui::Button::new("+ Add Status property"))
                .clicked()
            {
                intents.push(Intent::Action(Action::AddDatabaseProperty {
                    database: data.page_id,
                    name: "Status".into(),
                    kind: "status".into(),
                    options: vec![
                        PropertyOption::new("Todo"),
                        PropertyOption::new("In progress"),
                        PropertyOption::new("Done"),
                    ],
                }));
            }
        });
        ui.add_space(6.0);
        show_cards(ui, data, busy, palette, intents, 1);
        return;
    };

    let mut groups = vec!["No selection".to_owned()];
    for option in &status.options {
        if !groups.iter().any(|group| group == &option.name) {
            groups.push(option.name.clone());
        }
    }
    let sorted = &data.rows;
    let selection_id = egui::Id::new(("database-selected-rows", data.page_id));
    let anchor_id = egui::Id::new(("database-anchor-row", data.page_id));
    let mut selected_rows: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    let mut anchor_row: Option<crate::domain::model::PageId> =
        ui.ctx().data(|d| d.get_temp(anchor_id)).flatten();
    let valid_ids: std::collections::HashSet<_> = data.rows.iter().map(|r| r.page.id).collect();
    selected_rows.retain(|id| valid_ids.contains(id));
    if let Some(anchor) = anchor_row
        && !valid_ids.contains(&anchor)
    {
        anchor_row = None;
    }
    let (is_ctrl, is_shift) =
        ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.modifiers.shift));

    if !selected_rows.is_empty() && !ui.ctx().wants_keyboard_input() {
        let delete_pressed = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        });
        if delete_pressed {
            for id in &selected_rows {
                intents.push(Intent::Action(Action::Trash(*id)));
            }
            selected_rows.clear();
            anchor_row = None;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            selected_rows.clear();
            anchor_row = None;
        }
    }

    let board_visible_properties = data.view_settings.board_visible_properties.clone();
    let show_board_desc = data.view_settings.board_show_description;
    let all_properties = data.properties.clone();

    let mut rows_by_group = HashMap::<String, Vec<DatabaseRow>>::new();
    for row in sorted {
        let value = row.values.get(&status.id).map(String::as_str).unwrap_or("");
        let group = if value.is_empty() {
            "No selection"
        } else {
            value
        };
        rows_by_group
            .entry(group.to_owned())
            .or_default()
            .push((*row).clone());
        if !groups.iter().any(|candidate| candidate == group) {
            groups.push(group.to_owned());
        }
    }

    let natural_cards_h = board_max_column_cards_height(data, status.id);
    let (active_cards_h, _has_custom_height) = resolve_vertical_height(
        ui,
        data,
        natural_cards_h,
        DEFAULT_BOARD_MAX_CARDS_HEIGHT,
        96.0,
    );

    let board_col_width = 250.0;
    let total_board_w =
        ((groups.len() + 1) as f32 * (board_col_width + 12.0)).min(ui.available_width().max(320.0));

    egui::ScrollArea::horizontal()
        .id_salt(("database-board-hscroll", data.page_id))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for group in &groups {
                    let target_value = if group == "No selection" {
                        String::new()
                    } else {
                        group.clone()
                    };
                    let rows = rows_by_group.get(group).map_or(&[][..], Vec::as_slice);

                    let col_frame = egui::Frame::new()
                        .fill(palette.code)
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            palette.border.linear_multiply(0.4),
                        ))
                        .corner_radius(16)
                        .inner_margin(16);

                    let col_inner = col_frame.show(ui, |ui| {
                        ui.set_min_width(board_col_width);
                        ui.set_max_width(board_col_width);

                        let mut card_dropped = false;

                        ui.vertical(|ui| {
                            ui.set_min_width(board_col_width);
                            // Column Header
                            ui.horizontal(|ui| {
                                let (pill_fill, pill_text_color) = if group == "No selection" {
                                    (palette.hover, palette.muted)
                                } else {
                                    let opt_col = resolve_option_color(status.option_color(group));
                                    let (bg, _border, text) =
                                        option_pill_colors(palette, "status", group, opt_col);
                                    (bg, text)
                                };
                                let pill_frame = egui::Frame::new()
                                    .fill(pill_fill)
                                    .corner_radius(4)
                                    .inner_margin(egui::Margin::symmetric(8, 3));
                                pill_frame.show(ui, |ui| {
                                    ui.label(
                                        RichText::new(group)
                                            .font(FontId::proportional(12.5))
                                            .color(pill_text_color)
                                            .strong(),
                                    );
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(rows.len().to_string())
                                        .font(FontId::proportional(11.5))
                                        .color(palette.muted),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_enabled(
                                                !busy,
                                                egui::Button::new(
                                                    RichText::new("+")
                                                        .font(FontId::proportional(14.0))
                                                        .color(palette.muted),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_text("Add card to this column")
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(
                                                Action::CreateDatabaseRecordWithProperty {
                                                    database: data.page_id,
                                                    property: status.id,
                                                    value: target_value.clone(),
                                                },
                                            ));
                                        }
                                    },
                                );
                            });

                            ui.add_space(8.0);

                            // Cards vertical list
                            egui::ScrollArea::vertical()
                                .id_salt(("database-board-column", data.page_id, group))
                                .max_height(active_cards_h)
                                .min_scrolled_height(active_cards_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_min_width(board_col_width - 4.0);
                                    ui.set_min_height(active_cards_h);
                                    ui.spacing_mut().item_spacing.y = 8.0;
                                    for row in rows {
                                        let properties: Vec<_> = all_properties
                                            .iter()
                                            .filter(|property| property.id != status.id)
                                            .filter(|property| {
                                                board_visible_properties.as_deref().map_or_else(
                                                    || {
                                                        all_properties
                                                            .iter()
                                                            .filter(|candidate| {
                                                                candidate.id != status.id
                                                            })
                                                            .take(2)
                                                            .any(|visible| {
                                                                visible.id == property.id
                                                            })
                                                    },
                                                    |visible| visible.contains(&property.id),
                                                )
                                            })
                                            .filter_map(|property| {
                                                row.values
                                                    .get(&property.id)
                                                    .filter(|value| !value.is_empty())
                                                    .map(|value| {
                                                        let display_val =
                                                            if property.kind == "multi-select" {
                                                                value
                                                                    .split('|')
                                                                    .filter(|s| !s.is_empty())
                                                                    .collect::<Vec<_>>()
                                                                    .join(", ")
                                                            } else {
                                                                value.clone()
                                                            };
                                                        format!("{}: {display_val}", property.name)
                                                    })
                                            })
                                            .collect();

                                        let show_desc =
                                            show_board_desc && !row.preview.trim().is_empty();
                                        let card_h = 36.0
                                            + if !properties.is_empty() { 20.0 } else { 0.0 }
                                            + if show_desc { 22.0 } else { 0.0 };

                                        let (card_rect, card_resp) = ui.allocate_exact_size(
                                            egui::vec2(board_col_width - 4.0, card_h),
                                            egui::Sense::click_and_drag(),
                                        );
                                        card_resp.dnd_set_drag_payload(BoardDragPayload {
                                            database_id: data.page_id,
                                            record_id: row.page.id,
                                        });

                                        let is_selected = selected_rows.contains(&row.page.id);
                                        let is_hovered = card_resp.hovered();
                                        let card_bg = if is_selected {
                                            palette.accent_soft
                                        } else if is_hovered {
                                            palette.hover
                                        } else {
                                            palette.board_card
                                        };
                                        let stroke_col = if is_selected {
                                            palette.accent
                                        } else if is_hovered {
                                            palette.accent.linear_multiply(0.5)
                                        } else {
                                            palette.border.linear_multiply(0.6)
                                        };
                                        ui.painter().rect_filled(card_rect, 10.0, card_bg);
                                        ui.painter().rect_stroke(
                                            card_rect,
                                            10.0,
                                            egui::Stroke::new(
                                                if is_selected { 2.0 } else { 1.0_f32 },
                                                stroke_col,
                                            ),
                                            egui::StrokeKind::Inside,
                                        );

                                        if card_resp.dragged()
                                            && let Some(pointer) = ui.ctx().pointer_interact_pos()
                                        {
                                            let ghost_rect = egui::Rect::from_min_size(
                                                pointer + egui::vec2(10.0, 10.0),
                                                egui::vec2(200.0, 32.0),
                                            );
                                            let layer = ui.ctx().layer_painter(egui::LayerId::new(
                                                egui::Order::Tooltip,
                                                ui.id().with(("board-drag-ghost", row.page.id)),
                                            ));
                                            layer.rect_filled(ghost_rect, 6.0, palette.sidebar);
                                            layer.rect_stroke(
                                                ghost_rect,
                                                6.0,
                                                egui::Stroke::new(1.5_f32, palette.accent),
                                                egui::StrokeKind::Inside,
                                            );
                                            layer.text(
                                                ghost_rect.left_center() + egui::vec2(10.0, 0.0),
                                                egui::Align2::LEFT_CENTER,
                                                row.page.display_title(),
                                                FontId::proportional(12.5),
                                                palette.text,
                                            );
                                        }

                                        if card_resp.clicked() && !busy {
                                            if is_ctrl {
                                                if selected_rows.contains(&row.page.id) {
                                                    selected_rows.remove(&row.page.id);
                                                } else {
                                                    selected_rows.insert(row.page.id);
                                                    anchor_row = Some(row.page.id);
                                                }
                                            } else if is_shift {
                                                let anchor = anchor_row.unwrap_or(row.page.id);
                                                let p1 = sorted
                                                    .iter()
                                                    .position(|r| r.page.id == anchor)
                                                    .unwrap_or(0);
                                                let p2 = sorted
                                                    .iter()
                                                    .position(|r| r.page.id == row.page.id)
                                                    .unwrap_or(0);
                                                let start = p1.min(p2);
                                                let end = p1.max(p2);
                                                selected_rows.clear();
                                                for r in &sorted[start..=end] {
                                                    selected_rows.insert(r.page.id);
                                                }
                                            } else {
                                                selected_rows.clear();
                                                selected_rows.insert(row.page.id);
                                                anchor_row = Some(row.page.id);
                                                intents.push(Intent::Open(row.page.id, None));
                                            }
                                        }

                                        // Check DnD hover / release on this card
                                        if let Some(dragged) =
                                            card_resp.dnd_hover_payload::<BoardDragPayload>()
                                            && dragged.database_id == data.page_id
                                            && dragged.record_id != row.page.id
                                        {
                                            let pointer_y = ui
                                                .ctx()
                                                .pointer_interact_pos()
                                                .map(|p| p.y)
                                                .unwrap_or(card_rect.center().y);
                                            let after = pointer_y >= card_rect.center().y;
                                            let line_y = if after {
                                                card_rect.bottom() + 3.0
                                            } else {
                                                card_rect.top() - 3.0
                                            };
                                            ui.painter().line_segment(
                                                [
                                                    egui::pos2(card_rect.left() + 4.0, line_y),
                                                    egui::pos2(card_rect.right() - 4.0, line_y),
                                                ],
                                                egui::Stroke::new(2.5_f32, palette.accent),
                                            );
                                            if let Some(released) =
                                                card_resp.dnd_release_payload::<BoardDragPayload>()
                                            {
                                                intents.push(Intent::Action(
                                                    Action::MoveDatabaseRecord {
                                                        record: released.record_id,
                                                        property: status.id,
                                                        value: target_value.clone(),
                                                        target: Some((row.page.id, after)),
                                                    },
                                                ));
                                                card_dropped = true;
                                            }
                                        }

                                        card_resp.persistent_context_menu(|ui| {
                                            if ui.button("Open page").clicked() {
                                                intents.push(Intent::Open(row.page.id, None));
                                                ui.close_menu();
                                            }
                                            if ui.button("🗖  Diyalog olarak aç").clicked() {
                                                intents.push(Intent::OpenInDialog(row.page.id));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("🎨 Temayı düzenle").clicked() {
                                                intents.push(Intent::OpenSettingsTheme(
                                                    ThemeSection::Board,
                                                ));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            ui.label(
                                                RichText::new("Move to")
                                                    .small()
                                                    .color(palette.muted),
                                            );
                                            let current = row
                                                .values
                                                .get(&status.id)
                                                .cloned()
                                                .unwrap_or_default();
                                            if ui
                                                .selectable_label(
                                                    current.is_empty(),
                                                    "No selection",
                                                )
                                                .clicked()
                                                && !current.is_empty()
                                            {
                                                intents.push(Intent::Action(
                                                    Action::SetDatabaseProperty {
                                                        page: row.page.id,
                                                        property: status.id,
                                                        value: String::new(),
                                                    },
                                                ));
                                                ui.close_menu();
                                            }
                                            for option in &status.options {
                                                if ui
                                                    .selectable_label(
                                                        current == option.name,
                                                        &option.name,
                                                    )
                                                    .clicked()
                                                    && current != option.name
                                                {
                                                    intents.push(Intent::Action(
                                                        Action::SetDatabaseProperty {
                                                            page: row.page.id,
                                                            property: status.id,
                                                            value: option.name.clone(),
                                                        },
                                                    ));
                                                    ui.close_menu();
                                                }
                                            }
                                        });

                                        let inner = card_rect.shrink2(egui::vec2(10.0, 8.0));
                                        let mut card_ui = ui.new_child(
                                            egui::UiBuilder::new()
                                                .max_rect(inner)
                                                .layout(egui::Layout::top_down(egui::Align::Min)),
                                        );
                                        card_ui.set_clip_rect(card_rect.intersect(ui.clip_rect()));
                                        card_ui.spacing_mut().item_spacing.y = 3.0;

                                        let icon = if row.page.icon.is_empty() {
                                            "📄"
                                        } else {
                                            &row.page.icon
                                        };
                                        card_ui.horizontal(|ui| {
                                            if let Some(source) =
                                                crate::ui::emoji_image_source(icon)
                                            {
                                                ui.add(
                                                    egui::Image::new(source)
                                                        .fit_to_exact_size(egui::vec2(13.0, 13.0)),
                                                );
                                            } else {
                                                ui.label(
                                                    RichText::new(icon)
                                                        .font(FontId::proportional(13.0))
                                                        .color(palette.text),
                                                );
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(row.page.display_title())
                                                        .font(FontId::proportional(13.0))
                                                        .color(palette.text)
                                                        .strong(),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        if !properties.is_empty() {
                                            card_ui.add(
                                                egui::Label::new(
                                                    RichText::new(properties.join("  ·  "))
                                                        .font(FontId::proportional(11.5))
                                                        .color(palette.muted),
                                                )
                                                .truncate(),
                                            );
                                        }
                                        if show_desc {
                                            card_ui.add(
                                                egui::Label::new(
                                                    RichText::new(row.preview.trim())
                                                        .font(FontId::proportional(11.5))
                                                        .color(palette.muted),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    }
                                });

                            ui.add_space(6.0);
                            let add_card_btn = egui::Button::new(
                                RichText::new("+ New")
                                    .font(FontId::proportional(12.5))
                                    .color(palette.muted),
                            )
                            .frame(false);
                            if ui
                                .add_sized([board_col_width - 8.0, 26.0], add_card_btn)
                                .clicked()
                                && !busy
                            {
                                intents.push(Intent::Action(
                                    Action::CreateDatabaseRecordWithProperty {
                                        database: data.page_id,
                                        property: status.id,
                                        value: target_value.clone(),
                                    },
                                ));
                            }
                        });

                        card_dropped
                    });

                    // Handle dropping on the column background/empty space
                    if !col_inner.inner {
                        let col_drop_resp = ui.interact(
                            col_inner.response.rect,
                            ui.id().with(("board-col-drop", data.page_id, group)),
                            egui::Sense::hover(),
                        );
                        if let Some(hovered_drag) =
                            col_drop_resp.dnd_hover_payload::<BoardDragPayload>()
                            && hovered_drag.database_id == data.page_id
                        {
                            ui.painter().rect_stroke(
                                col_inner.response.rect,
                                8.0,
                                egui::Stroke::new(2.0_f32, palette.accent),
                                egui::StrokeKind::Inside,
                            );
                            if let Some(released) =
                                col_drop_resp.dnd_release_payload::<BoardDragPayload>()
                            {
                                let last_in_col = rows.last().map(|r| (r.page.id, true));
                                intents.push(Intent::Action(Action::MoveDatabaseRecord {
                                    record: released.record_id,
                                    property: status.id,
                                    value: target_value,
                                    target: last_in_col,
                                }));
                            }
                        }
                    }
                }

                // "+ Add group" column button on the right of the board
                ui.persistent_menu_button(
                    RichText::new("+ Group")
                        .font(FontId::proportional(12.5))
                        .color(palette.muted),
                    |ui| {
                        let new_grp_id =
                            egui::Id::new(("board-new-group", data.page_id, status.id));
                        let mut grp_name = ui
                            .ctx()
                            .data(|d| d.get_temp::<String>(new_grp_id))
                            .unwrap_or_default();
                        ui.label(RichText::new("New group name").small().color(palette.muted));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut grp_name)
                                .hint_text("e.g. Review")
                                .desired_width(150.0),
                        );
                        if resp.changed() {
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(new_grp_id, grp_name.clone()));
                        }
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (enter || ui.button("+ Add group").clicked())
                            && !grp_name.trim().is_empty()
                        {
                            let trimmed = grp_name.trim().to_owned();
                            let mut opts = status.options.clone();
                            if !opts.iter().any(|o| o.name == trimmed) {
                                opts.push(PropertyOption::new(&trimmed));
                                intents.push(Intent::Action(Action::SetDatabasePropertyOptions {
                                    property: status.id,
                                    options: opts,
                                }));
                            }
                            ui.ctx().data_mut(|d| d.remove::<String>(new_grp_id));
                            ui.close_menu();
                        }
                    },
                );
            });
        });

    if show_vertical_resize_handle(
        ui,
        data.page_id,
        &mut data.view_settings.table_height,
        total_board_w,
        active_cards_h,
        96.0,
        palette,
    ) {
        intents.push(Intent::Action(Action::SetDatabaseViewSettings {
            page: data.page_id,
            settings: data.view_settings.clone(),
        }));
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(selection_id, selected_rows);
        d.insert_temp(anchor_id, anchor_row);
    });
}
