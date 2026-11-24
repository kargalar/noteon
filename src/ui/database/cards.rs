use crate::ui::menus::PersistentContextMenu;

use eframe::egui::{self, Color32, FontId, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseView};
use crate::worker::Action;

use crate::ui::{Intent, ThemeSection, palette::Palette, primary_button};

use super::layout::{
    DEFAULT_GALLERY_MAX_HEIGHT, gallery_card_metrics, resolve_vertical_height,
    show_vertical_resize_handle,
};
use super::shared::format_date;

pub(super) fn show_gallery(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let cols = usize::from(data.gallery_columns);
    show_cards(ui, data, busy, palette, intents, cols);
}

pub(super) fn show_cards(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    columns: usize,
) {
    let rows = &data.rows;
    let selection_id = egui::Id::new(("database-selected-rows", data.page_id));
    let anchor_id = egui::Id::new(("database-anchor-row", data.page_id));
    let mut selected_rows: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    let mut anchor_row: Option<crate::domain::model::PageId> =
        ui.ctx().data(|d| d.get_temp(anchor_id)).flatten();
    let valid_ids: std::collections::HashSet<_> = rows.iter().map(|r| r.page.id).collect();
    selected_rows.retain(|id| valid_ids.contains(id));
    if let Some(anchor) = anchor_row
        && !valid_ids.contains(&anchor)
    {
        anchor_row = None;
    }
    let (is_ctrl, is_shift) =
        ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.modifiers.shift));
    let primary_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
    let secondary_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    let primary_released = ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
    let pointer_pos = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));

    let marquee_anchor_id = egui::Id::new(("database-cards-marquee-anchor", data.page_id));
    let marquee_active_id = egui::Id::new(("database-cards-marquee-active", data.page_id));
    let marquee_base_id = egui::Id::new(("database-cards-marquee-base", data.page_id));

    let mut marquee_anchor: Option<egui::Pos2> =
        ui.ctx().data(|d| d.get_temp(marquee_anchor_id)).flatten();
    let mut marquee_active: bool = ui
        .ctx()
        .data(|d| d.get_temp(marquee_active_id))
        .unwrap_or(false);
    let mut marquee_base: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|d| d.get_temp(marquee_base_id))
        .unwrap_or_default();

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

    let columns = columns.max(1);
    let row_count = rows.len().div_ceil(columns);
    let gallery = data.view == DatabaseView::Gallery;
    let settings = &data.view_settings;
    let show_cover = gallery && settings.gallery_show_cover;
    let show_title = !gallery || settings.gallery_show_title;
    let show_created_at = if gallery {
        settings.gallery_show_created_at
    } else {
        true
    };
    let show_description = gallery && settings.gallery_show_description;
    let visible_properties: Option<Vec<i64>> = if gallery {
        settings.gallery_visible_properties.clone()
    } else {
        None
    };
    let (card_height, row_gap) = gallery_card_metrics(data, gallery);
    let col_gap = 12.0;
    let total_width = (ui.available_width() - 12.0).max(200.0);
    let card_width =
        ((total_width - col_gap * columns.saturating_sub(1) as f32) / columns as f32).max(110.0);

    let natural_h = if row_count == 0 {
        card_height
    } else {
        row_count as f32 * card_height + row_count.saturating_sub(1) as f32 * row_gap
    };
    let (active_h, has_custom_height) =
        resolve_vertical_height(ui, data, natural_h, DEFAULT_GALLERY_MAX_HEIGHT, card_height);

    let cards_start_cursor = ui.cursor().min;
    let cards_area =
        egui::Rect::from_min_size(cards_start_cursor, egui::vec2(total_width, active_h));

    if primary_pressed
        && let Some(pos) = pointer_pos
        && cards_area.contains(pos)
    {
        marquee_anchor = Some(pos);
        marquee_active = false;
        marquee_base = if is_ctrl {
            selected_rows.clone()
        } else {
            std::collections::HashSet::new()
        };
    }

    if primary_down && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos) {
        let delta = (pos - anchor).length();
        if !marquee_active && delta > 6.0 {
            marquee_active = true;
            if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                ui.ctx().memory_mut(|m| m.surrender_focus(focused));
            }
        }
    }

    let selection_rect =
        if marquee_active && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos) {
            let rect = egui::Rect::from_two_pos(anchor, pos);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(("cards-marquee-painter", data.page_id)),
            ));
            painter.rect_filled(
                rect,
                2.0,
                Color32::from_rgba_unmultiplied(
                    palette.accent.r(),
                    palette.accent.g(),
                    palette.accent.b(),
                    30,
                ),
            );
            painter.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.2_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
            let edge_zone = 28.0;
            if pos.y < cards_area.top() + edge_zone {
                let speed = ((cards_area.top() + edge_zone - pos.y) * 0.45).clamp(2.0, 18.0);
                ui.scroll_with_delta(egui::vec2(0.0, speed));
            } else if pos.y > cards_area.bottom() - edge_zone {
                let speed = ((pos.y - (cards_area.bottom() - edge_zone)) * 0.45).clamp(2.0, 18.0);
                ui.scroll_with_delta(egui::vec2(0.0, -speed));
            }
            ui.ctx().request_repaint();
            Some(rect)
        } else {
            None
        };

    if row_count == 0 {
        let (empty_rect, _) =
            ui.allocate_exact_size(egui::vec2(total_width, active_h), egui::Sense::hover());
        ui.painter().rect_stroke(
            empty_rect,
            8.0,
            egui::Stroke::new(1.0_f32, palette.border.linear_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            empty_rect.center(),
            egui::Align2::CENTER_CENTER,
            "No records yet. Click \"+ New record\" below to add one.",
            FontId::proportional(13.0),
            palette.muted,
        );
    } else {
        egui::ScrollArea::vertical()
            .id_salt(("database-gallery-scroll", data.page_id))
            .max_height(active_h)
            .min_scrolled_height(active_h)
            .auto_shrink([false, !has_custom_height])
            .show_rows(ui, card_height, row_count, |ui, visible| {
                ui.spacing_mut().item_spacing.y = row_gap;
                for row_index in visible {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = col_gap;
                        for row in rows.iter().skip(row_index * columns).take(columns) {
                            let (card_rect, card_resp) = ui.allocate_exact_size(
                                egui::vec2(card_width, card_height),
                                egui::Sense::click(),
                            );
                            if let Some(sel_rect) = selection_rect {
                                if sel_rect.intersects(card_rect) {
                                    selected_rows.insert(row.page.id);
                                } else if !marquee_base.contains(&row.page.id) {
                                    selected_rows.remove(&row.page.id);
                                }
                            }
                            let is_selected = selected_rows.contains(&row.page.id);
                            let hovered = card_resp.hovered();
                            let bg = if is_selected {
                                palette.accent_soft
                            } else if hovered {
                                palette.hover
                            } else {
                                palette.gallery_card
                            };
                            let border_color = if is_selected {
                                palette.accent
                            } else if hovered {
                                palette.accent.linear_multiply(0.6)
                            } else {
                                palette.gallery_border
                            };
                            ui.painter().rect_filled(card_rect, 12.0, bg);
                            ui.painter().rect_stroke(
                                card_rect,
                                12.0,
                                egui::Stroke::new(
                                    if is_selected { 2.0 } else { 1.0_f32 },
                                    border_color,
                                ),
                                egui::StrokeKind::Inside,
                            );

                            card_resp.persistent_context_menu(|ui| {
                                if ui.button("↗  Sayfayı aç").clicked() {
                                    intents.push(Intent::Open(row.page.id, None));
                                    ui.close_menu();
                                }
                                if ui.button("🗖  Diyalog olarak aç").clicked() {
                                    intents.push(Intent::OpenInDialog(row.page.id));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui
                                    .button(RichText::new("🗑  Sil").color(palette.danger))
                                    .clicked()
                                {
                                    intents.push(Intent::Action(Action::Trash(row.page.id)));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("🎨 Temayı düzenle").clicked() {
                                    intents.push(Intent::OpenSettingsTheme(ThemeSection::Gallery));
                                    ui.close_menu();
                                }
                            });

                            if secondary_clicked
                                && !marquee_active
                                && let Some(pos) = pointer_pos
                                && card_rect.contains(pos)
                                && !selected_rows.contains(&row.page.id)
                            {
                                selected_rows.clear();
                                selected_rows.insert(row.page.id);
                                anchor_row = Some(row.page.id);
                            }

                            if card_resp.double_clicked() && !busy {
                                intents.push(Intent::Open(row.page.id, None));
                            } else if card_resp.clicked() && !busy && !marquee_active {
                                if is_ctrl {
                                    if selected_rows.contains(&row.page.id) {
                                        selected_rows.remove(&row.page.id);
                                    } else {
                                        selected_rows.insert(row.page.id);
                                        anchor_row = Some(row.page.id);
                                    }
                                } else if is_shift {
                                    let anchor = anchor_row.unwrap_or(row.page.id);
                                    let p1 =
                                        rows.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                                    let p2 = rows
                                        .iter()
                                        .position(|r| r.page.id == row.page.id)
                                        .unwrap_or(0);
                                    let start = p1.min(p2);
                                    let end = p1.max(p2);
                                    selected_rows.clear();
                                    for r in &rows[start..=end] {
                                        selected_rows.insert(r.page.id);
                                    }
                                } else {
                                    selected_rows.clear();
                                    selected_rows.insert(row.page.id);
                                    anchor_row = Some(row.page.id);
                                }
                            }
                            card_resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                            if hovered || is_selected {
                                let sel_box_rect = egui::Rect::from_center_size(
                                    card_rect.left_top() + egui::vec2(16.0, 16.0),
                                    egui::vec2(16.0, 16.0),
                                );
                                let sel_resp = ui.interact(
                                    sel_box_rect,
                                    ui.id().with(("card-sel", row.page.id)),
                                    egui::Sense::click(),
                                );
                                let check_stroke = egui::Stroke::new(
                                    1.0_f32,
                                    if is_selected {
                                        palette.accent
                                    } else {
                                        palette.muted
                                    },
                                );
                                ui.painter().rect_filled(sel_box_rect, 3.0, palette.sidebar);
                                ui.painter().rect_stroke(
                                    sel_box_rect,
                                    3.0,
                                    check_stroke,
                                    egui::StrokeKind::Inside,
                                );
                                if is_selected {
                                    ui.painter().rect_filled(
                                        sel_box_rect.shrink(3.0),
                                        2.0,
                                        palette.accent,
                                    );
                                }
                                if sel_resp.clicked() && !marquee_active {
                                    if is_ctrl {
                                        if selected_rows.contains(&row.page.id) {
                                            selected_rows.remove(&row.page.id);
                                        } else {
                                            selected_rows.insert(row.page.id);
                                            anchor_row = Some(row.page.id);
                                        }
                                    } else if is_shift {
                                        let anchor = anchor_row.unwrap_or(row.page.id);
                                        let p1 = rows
                                            .iter()
                                            .position(|r| r.page.id == anchor)
                                            .unwrap_or(0);
                                        let p2 = rows
                                            .iter()
                                            .position(|r| r.page.id == row.page.id)
                                            .unwrap_or(0);
                                        let start = p1.min(p2);
                                        let end = p1.max(p2);
                                        selected_rows.clear();
                                        for r in &rows[start..=end] {
                                            selected_rows.insert(r.page.id);
                                        }
                                    } else if is_selected && selected_rows.len() == 1 {
                                        selected_rows.clear();
                                        anchor_row = None;
                                    } else {
                                        selected_rows.clear();
                                        selected_rows.insert(row.page.id);
                                        anchor_row = Some(row.page.id);
                                    }
                                }
                            }

                            if hovered {
                                let open_box_rect = egui::Rect::from_center_size(
                                    card_rect.right_top() + egui::vec2(-28.0, 16.0),
                                    egui::vec2(44.0, 18.0),
                                );
                                let open_resp = ui.interact(
                                    open_box_rect,
                                    ui.id().with(("card-open", row.page.id)),
                                    egui::Sense::click(),
                                );
                                let bg = if open_resp.hovered() {
                                    palette.accent
                                } else {
                                    palette.sidebar
                                };
                                let fg = if open_resp.hovered() {
                                    Color32::WHITE
                                } else {
                                    palette.muted
                                };
                                ui.painter().rect_filled(open_box_rect, 3.0, bg);
                                ui.painter().rect_stroke(
                                    open_box_rect,
                                    3.0,
                                    egui::Stroke::new(1.0_f32, palette.border),
                                    egui::StrokeKind::Inside,
                                );
                                ui.painter().text(
                                    open_box_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "OPEN ↗",
                                    FontId::proportional(10.0),
                                    fg,
                                );
                                if open_resp.clicked() && !marquee_active {
                                    intents.push(Intent::Open(row.page.id, None));
                                }
                            }

                            let inner_rect = card_rect.shrink(10.0);
                            let mut card_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(inner_rect)
                                    .layout(egui::Layout::top_down(egui::Align::Min)),
                            );
                            card_ui.set_clip_rect(card_rect.intersect(ui.clip_rect()));
                            card_ui.spacing_mut().item_spacing.y = 4.0;

                            if show_cover {
                                let image_rect = egui::Rect::from_min_size(
                                    card_ui.cursor().min,
                                    egui::vec2(inner_rect.width(), 120.0),
                                );
                                card_ui.allocate_exact_size(
                                    egui::vec2(inner_rect.width(), 120.0),
                                    egui::Sense::hover(),
                                );
                                card_ui.painter().rect_filled(image_rect, 6.0, palette.code);
                                let mut painted = false;
                                if let Some(path) = row.page.cover_image.as_deref() {
                                    let focus = egui::vec2(row.page.cover_x, row.page.cover_y);
                                    painted = crate::ui::editor::media::paint_cover(
                                        &card_ui,
                                        std::path::Path::new(path),
                                        image_rect,
                                        focus,
                                    )
                                    .is_some();
                                }
                                if !painted {
                                    let icon = if row.page.icon.is_empty() {
                                        "📄"
                                    } else {
                                        &row.page.icon
                                    };
                                    if let Some(source) = crate::ui::emoji_image_source(icon) {
                                        let icon_rect = egui::Rect::from_center_size(
                                            image_rect.center(),
                                            egui::vec2(26.0, 26.0),
                                        );
                                        egui::Image::new(source).paint_at(&card_ui, icon_rect);
                                    } else {
                                        card_ui.painter().text(
                                            image_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            icon,
                                            FontId::proportional(26.0),
                                            palette.muted,
                                        );
                                    }
                                }
                            }

                            if show_title {
                                let icon = if row.page.icon.is_empty() {
                                    "📄"
                                } else {
                                    &row.page.icon
                                };
                                let title = row.page.display_title();
                                card_ui.horizontal(|ui| {
                                    if let Some(source) = crate::ui::emoji_image_source(icon) {
                                        ui.add(
                                            egui::Image::new(source)
                                                .fit_to_exact_size(egui::vec2(14.0, 14.0)),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(icon)
                                                .font(FontId::proportional(14.0))
                                                .color(palette.text),
                                        );
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(title)
                                                .font(FontId::proportional(14.0))
                                                .color(palette.text)
                                                .strong(),
                                        )
                                        .truncate(),
                                    );
                                });
                            }

                            if show_created_at {
                                card_ui.add(
                                    egui::Label::new(
                                        RichText::new(format_date(row.page.created_at))
                                            .font(FontId::proportional(11.5))
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }

                            if show_description && !row.preview.trim().is_empty() {
                                card_ui.add(
                                    egui::Label::new(
                                        RichText::new(row.preview.trim())
                                            .font(FontId::proportional(12.0))
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }

                            let summaries: Vec<_> = data
                                .properties
                                .iter()
                                .filter(|property| {
                                    visible_properties.as_deref().map_or_else(
                                        || {
                                            data.properties
                                                .iter()
                                                .take(2)
                                                .any(|visible| visible.id == property.id)
                                        },
                                        |visible| visible.contains(&property.id),
                                    )
                                })
                                .filter_map(|property| {
                                    row.values
                                        .get(&property.id)
                                        .filter(|value| !value.is_empty())
                                        .map(|value| {
                                            let display_val = if property.kind == "multi-select" {
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
                            if !summaries.is_empty() {
                                card_ui.add(
                                    egui::Label::new(
                                        RichText::new(summaries.join("  ·  "))
                                            .font(FontId::proportional(11.5))
                                            .color(palette.muted),
                                    )
                                    .truncate(),
                                );
                            }
                        }
                    });
                }
            });
    }

    if primary_clicked
        && !is_ctrl
        && !is_shift
        && !marquee_active
        && let Some(pos) = pointer_pos
        && cards_area.contains(pos)
        && pos.y > cards_start_cursor.y + natural_h
    {
        selected_rows.clear();
        anchor_row = None;
    }

    if primary_released {
        marquee_anchor = None;
        marquee_active = false;
        marquee_base.clear();
    }

    if !selected_rows.is_empty() {
        let count = selected_rows.len();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{count} record{} selected",
                    if count == 1 { "" } else { "s" }
                ))
                .strong()
                .color(palette.accent),
            );
            if ui.button("Duplicate").clicked() {
                for id in &selected_rows {
                    intents.push(Intent::Action(Action::Duplicate(*id)));
                }
            }
            if ui
                .button(RichText::new("Delete").color(palette.danger))
                .clicked()
            {
                for id in &selected_rows {
                    intents.push(Intent::Action(Action::Trash(*id)));
                }
                selected_rows.clear();
                anchor_row = None;
            }
            if ui.button("Clear selection").clicked() {
                selected_rows.clear();
                anchor_row = None;
            }
        });
    }

    ui.add_space(6.0);
    let new_btn = primary_button("+ New record", palette);
    if ui
        .add_enabled_ui(!busy, |ui| ui.add_sized([total_width, 34.0], new_btn))
        .inner
        .clicked()
    {
        intents.push(Intent::Action(Action::CreateDatabaseRecord(data.page_id)));
    }

    if show_vertical_resize_handle(
        ui,
        data.page_id,
        &mut data.view_settings.table_height,
        total_width,
        active_h,
        card_height,
        palette,
    ) {
        intents.push(Intent::Action(Action::SetDatabaseViewSettings {
            page: data.page_id,
            settings: data.view_settings.clone(),
        }));
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(marquee_anchor_id, marquee_anchor);
        d.insert_temp(marquee_active_id, marquee_active);
        d.insert_temp(marquee_base_id, marquee_base);
        d.insert_temp(selection_id, selected_rows);
        d.insert_temp(anchor_id, anchor_row);
    });
}
