use super::table_navigation;
use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};

use eframe::egui::{
    self, Color32, FontId, RichText, Ui,
    text::{CCursor, CCursorRange},
};

use crate::domain::model::{DatabaseData, DatabaseProperty, DatabaseRow, DatabaseView};
use crate::worker::Action;

use crate::ui::{Intent, ThemeSection, palette::Palette};

use super::layout::{resolve_vertical_height, show_vertical_resize_handle};
use super::property_menu::add_property_menu;
use super::property_value::{PropertyEditorStyle, property_editor};
use super::shared::{EMPTY_RECORD_NAME, format_date, table_border_color};
use super::table_columns::{
    ColumnResize, DEFAULT_TABLE_MAX_ROWS_HEIGHT, TABLE_ADD_PROP_WIDTH, TABLE_CREATED_WIDTH,
    TABLE_HEADER_HEIGHT, TABLE_NAME_MIN_WIDTH, TABLE_NAME_TITLE_GAP, TABLE_NAME_WIDTH,
    TABLE_OPEN_PAGE_BUTTON_MARGIN, TABLE_OPEN_PAGE_BUTTON_SIZE, TABLE_PROPERTY_WIDTH,
    TABLE_RECORD_ICON_WIDTH, TABLE_ROW_HEIGHT, auto_fit_created_column, auto_fit_name_column,
    auto_fit_property_column, handle_column_resize, live_column_width, show_property_header,
};
use super::table_filter::table_row_matches;
use super::table_footer::{table_footer_cell, table_number_footer_cell};
use super::{Drafts, toggle_sort};

pub(super) fn delete_selected_table_rows(ui: &Ui, data: &DatabaseData, intents: &mut Vec<Intent>) {
    if data.view != DatabaseView::Table {
        return;
    }
    let selection_id = egui::Id::new(("database-selected-rows", data.page_id));
    let selected: std::collections::HashSet<crate::domain::model::PageId> = ui
        .ctx()
        .data(|state| state.get_temp(selection_id))
        .unwrap_or_default();
    let selected_rows: Vec<_> = data
        .rows
        .iter()
        .filter(|row| selected.contains(&row.page.id))
        .collect();
    let focused_row = ui
        .ctx()
        .data(|state| {
            state.get_temp::<(egui::Id, crate::domain::model::PageId)>(egui::Id::new((
                "database-table-focused-row",
                data.page_id,
            )))
        })
        .filter(|(widget, page)| {
            ui.memory(|memory| memory.has_focus(*widget))
                && data.rows.iter().any(|row| row.page.id == *page)
        })
        .map(|(_, page)| page);
    let hovered_row = if ui.ctx().wants_keyboard_input() {
        None
    } else {
        ui.input(|input| input.pointer.hover_pos())
            .and_then(|position| {
                if ui.ctx().layer_id_at(position) != Some(ui.layer_id()) {
                    return None;
                }
                ui.ctx()
                    .data(|state| {
                        state.get_temp::<Vec<(crate::domain::model::PageId, egui::Rect)>>(
                            egui::Id::new(("database-table-row-hit-areas", data.page_id)),
                        )
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .find(|(page, rect)| {
                        rect.contains(position) && data.rows.iter().any(|row| row.page.id == *page)
                    })
                    .map(|(page, _)| page)
            })
    };
    // Multiple selected rows take priority; otherwise Delete acts on the current cell's row.
    let targets: Vec<_> = if selected_rows.len() > 1 {
        selected_rows.iter().map(|row| row.page.id).collect()
    } else {
        focused_row.or(hovered_row).into_iter().collect()
    };
    let delete_pressed =
        ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
    if !targets.is_empty() && delete_pressed {
        for page in targets {
            intents.push(Intent::Action(Action::Trash(page)));
        }
        if let Some(widget) = ui.memory(|memory| memory.focused()) {
            ui.memory_mut(|memory| memory.surrender_focus(widget));
        }
        ui.ctx().data_mut(|state| {
            state.remove::<std::collections::HashSet<crate::domain::model::PageId>>(selection_id);
            state.remove::<Option<crate::domain::model::PageId>>(egui::Id::new((
                "database-anchor-row",
                data.page_id,
            )));
        });
    }
}

pub(super) fn show_table(
    ui: &mut Ui,
    data: &mut DatabaseData,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) {
    let query = ui
        .ctx()
        .data(|state| {
            let open = egui::Id::new(("database-table-find-open", data.page_id));
            let query = egui::Id::new(("database-table-find-query", data.page_id));
            if state.get_temp::<bool>(open).unwrap_or(false) {
                state.get_temp::<String>(query)
            } else {
                None
            }
        })
        .unwrap_or_default();
    let rows: Vec<&DatabaseRow> = data
        .rows
        .iter()
        .filter(|row| table_row_matches(row, &data.properties, &query))
        .collect();
    let visible_properties: Vec<&DatabaseProperty> = data
        .properties
        .iter()
        .filter(|property| {
            data.view_settings
                .table_visible_properties
                .as_ref()
                .is_none_or(|visible| visible.contains(&property.id))
        })
        .collect();
    let mut navigation = table_navigation::TableNavigation::begin(
        ui,
        data,
        &rows,
        &visible_properties,
        busy,
        drafts,
    );
    let show_created_at = data.view_settings.table_show_created_at;

    let base_name_w = data
        .view_settings
        .table_name_width
        .unwrap_or(TABLE_NAME_WIDTH)
        .max(TABLE_NAME_MIN_WIDTH);
    let base_created_w = data
        .view_settings
        .table_created_width
        .unwrap_or(TABLE_CREATED_WIDTH);

    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    let settings = egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let name_w = live_column_width(ui, ("name", data.page_id), base_name_w);
            let created_w = live_column_width(ui, ("created", data.page_id), base_created_w);
            let property_columns: Vec<_> = visible_properties
                .iter()
                .copied()
                .map(|prop| {
                    let stored = data
                        .view_settings
                        .table_column_widths
                        .get(&prop.id)
                        .copied()
                        .unwrap_or(TABLE_PROPERTY_WIDTH);
                    (
                        prop,
                        live_column_width(ui, ("prop", data.page_id, prop.id), stored),
                    )
                })
                .collect();

            let data_columns_width = name_w
                + if show_created_at { created_w } else { 0.0 }
                + property_columns.iter().map(|(_, width)| width).sum::<f32>();
            let table_width = data_columns_width + TABLE_ADD_PROP_WIDTH;

            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.set_min_width(table_width);

            let table_top = ui.cursor().min.y;
            let table_left = ui.cursor().min.x;
            let data_right = table_left + data_columns_width;
            let table_right = table_left + table_width.max(ui.available_width());

            let header = show_table_header(
                ui,
                data,
                HeaderColumns {
                    name: name_w,
                    created: show_created_at.then_some(created_w),
                    properties: &property_columns,
                },
                busy,
                palette,
                intents,
                drafts,
            );
            paint_table_row_extension(
                ui,
                data_right..table_right,
                header.top()..header.bottom(),
                palette,
            );

            // 2. Data Rows
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
            let primary_clicked =
                ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
            let secondary_clicked =
                ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
            let primary_down = ui.input(|i| i.pointer.primary_down());
            let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
            let primary_released =
                ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
            let pointer_pos = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));

            let marquee_anchor_id = egui::Id::new(("database-table-marquee-anchor", data.page_id));
            let marquee_active_id = egui::Id::new(("database-table-marquee-active", data.page_id));
            let marquee_base_id = egui::Id::new(("database-table-marquee-base", data.page_id));

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
                let delete_pressed =
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
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

            let natural_rows_h = (rows.len() as f32 * TABLE_ROW_HEIGHT).max(TABLE_ROW_HEIGHT);
            let (active_rows_h, has_custom_height) = resolve_vertical_height(
                ui,
                data,
                natural_rows_h,
                DEFAULT_TABLE_MAX_ROWS_HEIGHT,
                TABLE_ROW_HEIGHT,
            );

            let rows_start_cursor = ui.cursor().min;
            let rows_area = egui::Rect::from_min_size(
                rows_start_cursor,
                egui::vec2(table_width.max(ui.available_width()), active_rows_h),
            );
            let rows_content = egui::Rect::from_min_size(
                rows_start_cursor,
                egui::vec2(table_width, natural_rows_h.min(active_rows_h)),
            );

            if primary_pressed
                && let Some(pos) = pointer_pos
                && ui.rect_contains_pointer(rows_area)
                && !rows_content.contains(pos)
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

            let selection_rect = if marquee_active
                && let (Some(anchor), Some(pos)) = (marquee_anchor, pointer_pos)
            {
                let rect = egui::Rect::from_two_pos(anchor, pos);
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new(("table-marquee-painter", data.page_id)),
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
                if pos.y < rows_area.top() + edge_zone {
                    let speed = ((rows_area.top() + edge_zone - pos.y) * 0.45).clamp(2.0, 18.0);
                    ui.scroll_with_delta(egui::vec2(0.0, speed));
                } else if pos.y > rows_area.bottom() - edge_zone {
                    let speed =
                        ((pos.y - (rows_area.bottom() - edge_zone)) * 0.45).clamp(2.0, 18.0);
                    ui.scroll_with_delta(egui::vec2(0.0, -speed));
                }
                ui.ctx().request_repaint();
                Some(rect)
            } else {
                None
            };

            let focus_record_key = egui::Id::new(("pending-table-focus-record", data.page_id));
            let pending_focus_record: Option<crate::domain::model::PageId> =
                ui.ctx().data(|d| d.get_temp(focus_record_key));
            let pending_focus_row_idx = pending_focus_record
                .or(navigation.pending_record())
                .and_then(|target_id| rows.iter().position(|r| r.page.id == target_id));

            let mut vscroll = egui::ScrollArea::vertical()
                .id_salt(("table-rows-vscroll", data.page_id))
                .max_height(active_rows_h)
                .min_scrolled_height(active_rows_h)
                .auto_shrink([false, !has_custom_height]);
            if let Some(focus_idx) = pending_focus_row_idx {
                let target_bottom = (focus_idx + 1) as f32 * TABLE_ROW_HEIGHT;
                let offset = (target_bottom - active_rows_h).max(0.0);
                vscroll = vscroll.vertical_scroll_offset(offset);
            }
            let mut row_hit_areas = Vec::new();
            let rows_scroll = vscroll.show_rows(ui, TABLE_ROW_HEIGHT, rows.len(), |ui, visible| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                for row in &rows[visible] {
                    let row_id = row.page.id;
                    let row_cursor = ui.cursor().min;
                    let row_rect = egui::Rect::from_min_size(
                        row_cursor,
                        egui::vec2(table_width, TABLE_ROW_HEIGHT),
                    );
                    row_hit_areas.push((row_id, row_rect.intersect(ui.clip_rect())));
                    if let Some(sel_rect) = selection_rect {
                        if sel_rect.intersects(row_rect) {
                            selected_rows.insert(row_id);
                        } else if !marquee_base.contains(&row_id) {
                            selected_rows.remove(&row_id);
                        }
                    }
                    let is_row_selected = selected_rows.contains(&row_id);
                    let row_hovered = ui.rect_contains_pointer(row_rect);
                    let sel_box_rect = egui::Rect::from_center_size(
                        row_cursor + egui::vec2(8.0, 10.0),
                        egui::vec2(14.0, 14.0),
                    );

                    if primary_clicked
                        && !marquee_active
                        && let Some(pos) = pointer_pos
                        && row_rect.contains(pos)
                        && !sel_box_rect.contains(pos)
                    {
                        if is_ctrl {
                            if selected_rows.contains(&row_id) {
                                selected_rows.remove(&row_id);
                            } else {
                                selected_rows.insert(row_id);
                                anchor_row = Some(row_id);
                            }
                            if let Some(f) = ui.ctx().memory(|m| m.focused()) {
                                ui.ctx().memory_mut(|m| m.surrender_focus(f));
                            }
                        } else if is_shift {
                            let anchor = anchor_row.unwrap_or(row_id);
                            let p1 = rows.iter().position(|r| r.page.id == anchor).unwrap_or(0);
                            let p2 = rows.iter().position(|r| r.page.id == row_id).unwrap_or(0);
                            let start = p1.min(p2);
                            let end = p1.max(p2);
                            selected_rows.clear();
                            for r in &rows[start..=end] {
                                selected_rows.insert(r.page.id);
                            }
                            if let Some(f) = ui.ctx().memory(|m| m.focused()) {
                                ui.ctx().memory_mut(|m| m.surrender_focus(f));
                            }
                        }
                    }
                    if secondary_clicked
                        && !marquee_active
                        && let Some(pos) = pointer_pos
                        && row_rect.contains(pos)
                        && !selected_rows.contains(&row_id)
                    {
                        selected_rows.clear();
                        selected_rows.insert(row_id);
                        anchor_row = Some(row_id);
                    }

                    let focused_before = ui.memory(|memory| memory.focused());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                        table_cell(
                            ui,
                            name_w,
                            TABLE_ROW_HEIGHT,
                            palette,
                            true,
                            is_row_selected,
                            |ui, cell_hovered| {
                                let hovered = row_hovered || cell_hovered;
                                let name_cell_rect = egui::Rect::from_min_size(
                                    row_cursor,
                                    egui::vec2(name_w, TABLE_ROW_HEIGHT),
                                );

                                let mut sel_box_hit: Option<egui::Rect> = None;
                                if hovered || is_row_selected {
                                    sel_box_hit = Some(sel_box_rect.expand(2.0));
                                    let sel_resp = ui.interact(
                                        sel_box_rect,
                                        ui.id().with(("row-sel", row.page.id)),
                                        egui::Sense::click(),
                                    );
                                    let check_stroke = egui::Stroke::new(
                                        1.0_f32,
                                        if is_row_selected {
                                            palette.accent
                                        } else {
                                            palette.muted
                                        },
                                    );
                                    ui.painter().rect_stroke(
                                        sel_box_rect,
                                        3.0,
                                        check_stroke,
                                        egui::StrokeKind::Inside,
                                    );
                                    if is_row_selected {
                                        ui.painter().rect_filled(
                                            sel_box_rect.shrink(2.5),
                                            2.0,
                                            palette.accent,
                                        );
                                    }
                                    if sel_resp.clicked() && !marquee_active {
                                        if let Some(f) = ui.ctx().memory(|m| m.focused()) {
                                            ui.ctx().memory_mut(|m| m.surrender_focus(f));
                                        }
                                        if is_ctrl || !is_shift {
                                            if selected_rows.contains(&row_id) {
                                                selected_rows.remove(&row_id);
                                            } else {
                                                selected_rows.insert(row_id);
                                                anchor_row = Some(row_id);
                                            }
                                        } else if is_shift {
                                            let anchor = anchor_row.unwrap_or(row_id);
                                            let p1 = rows
                                                .iter()
                                                .position(|r| r.page.id == anchor)
                                                .unwrap_or(0);
                                            let p2 = rows
                                                .iter()
                                                .position(|r| r.page.id == row_id)
                                                .unwrap_or(0);
                                            let start = p1.min(p2);
                                            let end = p1.max(p2);
                                            selected_rows.clear();
                                            for r in &rows[start..=end] {
                                                selected_rows.insert(r.page.id);
                                            }
                                        }
                                    }
                                }
                                ui.add_space(18.0);

                                let icon_start_x = ui.cursor().min.x;
                                let icon = if row.page.icon.is_empty() {
                                    "📄"
                                } else {
                                    &row.page.icon
                                };
                                let new_icon = ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(TABLE_RECORD_ICON_WIDTH, TABLE_ROW_HEIGHT),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            crate::ui::icon_picker_button(
                                                ui,
                                                ("table-row-icon", row.page.id),
                                                icon,
                                                13.0,
                                                None,
                                            )
                                        },
                                    )
                                    .inner;
                                if let Some(new_icon) = new_icon {
                                    intents.push(Intent::Action(Action::SetDatabaseRecordIcon {
                                        page: row.page.id,
                                        icon: new_icon,
                                    }));
                                }
                                let icon_hit_rect = egui::Rect::from_min_max(
                                    egui::pos2(icon_start_x, name_cell_rect.top()),
                                    egui::pos2(ui.cursor().min.x + 2.0, name_cell_rect.bottom()),
                                );
                                let icon_drag = ui.interact(
                                    icon_hit_rect,
                                    ui.id().with(("table-row-icon-drag", row.page.id)),
                                    egui::Sense::drag(),
                                );
                                icon_drag.dnd_set_drag_payload(
                                    crate::ui::page_transfer::PageDragPayload {
                                        pages: if selected_rows.contains(&row.page.id) {
                                            rows.iter()
                                                .filter(|row| selected_rows.contains(&row.page.id))
                                                .map(|row| row.page.id)
                                                .collect()
                                        } else {
                                            vec![row.page.id]
                                        },
                                    },
                                );
                                if data.sort_key == "manual" {
                                    icon_drag.on_hover_text("Drag to reorder records");
                                }

                                let title = drafts
                                    .titles
                                    .entry(row.page.id)
                                    .or_insert_with(|| row.page.title.clone());
                                let open_btn_rect = egui::Rect::from_center_size(
                                    egui::pos2(
                                        name_cell_rect.right()
                                            - TABLE_OPEN_PAGE_BUTTON_MARGIN
                                            - TABLE_OPEN_PAGE_BUTTON_SIZE.x * 0.5,
                                        name_cell_rect.center().y,
                                    ),
                                    TABLE_OPEN_PAGE_BUTTON_SIZE,
                                );
                                let width_avail = (open_btn_rect.left()
                                    - TABLE_NAME_TITLE_GAP
                                    - ui.cursor().min.x)
                                    .max(24.0);

                                let title_edit_id =
                                    egui::Id::new(("table-row-title-edit", row.page.id));

                                let text_color =
                                    if crate::ui::page_transfer::is_cut(ui.ctx(), row.page.id) {
                                        palette.muted
                                    } else {
                                        palette.text
                                    };

                                let display_text = if title.is_empty() {
                                    EMPTY_RECORD_NAME
                                } else {
                                    title.as_str()
                                };
                                let text_width = ui.fonts(|f| {
                                    f.layout_no_wrap(
                                        display_text.to_string(),
                                        FontId::proportional(13.0),
                                        Color32::WHITE,
                                    )
                                    .rect
                                    .width()
                                });
                                let title_edit_w = (text_width + 8.0).clamp(24.0, width_avail);

                                let response = ui.add_enabled(
                                    !busy,
                                    egui::TextEdit::singleline(title)
                                        .id(title_edit_id)
                                        .hint_text(
                                            RichText::new(EMPTY_RECORD_NAME).color(palette.muted),
                                        )
                                        .text_color(text_color)
                                        .desired_width(title_edit_w)
                                        .font(FontId::proportional(13.0))
                                        .frame(false)
                                        .margin(egui::Margin::ZERO),
                                );

                                if pending_focus_record == Some(row.page.id) && !busy {
                                    response.request_focus();
                                    let mut state =
                                        egui::text_edit::TextEditState::load(ui.ctx(), response.id)
                                            .unwrap_or_default();
                                    let char_count = title.chars().count();
                                    let range = CCursorRange::two(
                                        CCursor::new(0),
                                        CCursor::new(char_count),
                                    );
                                    state.cursor.set_char_range(Some(range));
                                    state.store(ui.ctx(), response.id);
                                    ui.ctx().data_mut(|d| {
                                        d.remove::<crate::domain::model::PageId>(focus_record_key);
                                    });
                                    ui.ctx().request_repaint();
                                }

                                navigation.register(ui, row.page.id, None, &response);

                                if response.lost_focus() && title != &row.page.title {
                                    intents.push(Intent::Action(Action::RenameDatabaseRecord {
                                        page: row.page.id,
                                        title: title.clone(),
                                    }));
                                }

                                response.persistent_context_menu(|ui| {
                                    let rows_to_copy: Vec<_> =
                                        if selected_rows.contains(&row.page.id) {
                                            rows.iter()
                                                .filter(|row| selected_rows.contains(&row.page.id))
                                                .collect()
                                        } else {
                                            vec![row]
                                        };
                                    for (label, cut) in
                                        [("Copy pages  Ctrl+C", false), ("Cut pages  Ctrl+X", true)]
                                    {
                                        if ui.button(label).clicked() {
                                            crate::ui::page_transfer::copy_pages(
                                                ui.ctx(),
                                                rows_to_copy
                                                    .iter()
                                                    .map(|row| row.page.id)
                                                    .collect(),
                                                &rows_to_copy
                                                    .iter()
                                                    .map(|row| row.page.display_title().to_owned())
                                                    .collect::<Vec<_>>(),
                                                cut,
                                            );
                                            ui.close_menu();
                                        }
                                    }
                                    ui.separator();
                                    if selected_rows.contains(&row.page.id)
                                        && selected_rows.len() > 1
                                    {
                                        let count = selected_rows.len();
                                        ui.label(
                                            RichText::new(format!("{count} records selected"))
                                                .small()
                                                .color(palette.muted),
                                        );
                                        ui.separator();
                                        if ui.button(format!("Duplicate {count} records")).clicked()
                                        {
                                            for id in &selected_rows {
                                                intents
                                                    .push(Intent::Action(Action::Duplicate(*id)));
                                            }
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui
                                            .button(
                                                RichText::new(format!(
                                                    "Move {count} records to trash"
                                                ))
                                                .color(palette.danger),
                                            )
                                            .clicked()
                                        {
                                            for id in &selected_rows {
                                                intents.push(Intent::Action(Action::Trash(*id)));
                                            }
                                            ui.close_menu();
                                        }
                                    } else {
                                        if ui.button("↗  Open page").clicked() {
                                            intents.push(Intent::Open(row.page.id, None));
                                            ui.close_menu();
                                        }
                                        if ui.button("🗖  Diyalog olarak aç").clicked() {
                                            intents.push(Intent::OpenInDialog(row.page.id));
                                            ui.close_menu();
                                        }
                                        if ui.button("Duplicate").clicked() {
                                            intents.push(Intent::Action(Action::Duplicate(
                                                row.page.id,
                                            )));
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui
                                            .button(
                                                RichText::new("Move to trash")
                                                    .color(palette.danger),
                                            )
                                            .clicked()
                                        {
                                            intents
                                                .push(Intent::Action(Action::Trash(row.page.id)));
                                            ui.close_menu();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button("🎨 Temayı düzenle").clicked() {
                                        intents
                                            .push(Intent::OpenSettingsTheme(ThemeSection::Table));
                                        ui.close_menu();
                                    }
                                });

                                if cell_hovered {
                                    let open_resp = ui
                                        .push_id(("table-row-open-page", row.page.id), |ui| {
                                            ui.put(
                                                open_btn_rect,
                                                egui::Button::new("↗").frame(true),
                                            )
                                        })
                                        .inner
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text("Open page");
                                    if open_resp.clicked() {
                                        intents.push(Intent::Open(row.page.id, None));
                                    }
                                }

                                let is_interactive = pointer_pos.is_some_and(|pos| {
                                    sel_box_hit.is_some_and(|r| r.contains(pos))
                                        || icon_hit_rect.contains(pos)
                                        || response.rect.contains(pos)
                                        || (cell_hovered && open_btn_rect.contains(pos))
                                });

                                let in_empty_space = pointer_pos
                                    .is_some_and(|pos| name_cell_rect.contains(pos))
                                    && !is_interactive;

                                if in_empty_space && !marquee_active && !busy {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                    if primary_clicked
                                        && !is_ctrl
                                        && !is_shift
                                        && ui.ctx().dragged_id().is_none()
                                    {
                                        response.request_focus();
                                        let mut state = egui::text_edit::TextEditState::load(
                                            ui.ctx(),
                                            title_edit_id,
                                        )
                                        .unwrap_or_default();
                                        let char_count = title.chars().count();
                                        state.cursor.set_char_range(Some(CCursorRange::two(
                                            CCursor::new(0),
                                            CCursor::new(char_count),
                                        )));
                                        state.store(ui.ctx(), title_edit_id);
                                    }
                                }
                            },
                        );

                        if show_created_at {
                            table_cell(
                                ui,
                                created_w,
                                TABLE_ROW_HEIGHT,
                                palette,
                                false,
                                is_row_selected,
                                |ui, _| {
                                    ui.label(
                                        RichText::new(format_date(row.page.created_at))
                                            .font(FontId::proportional(12.0))
                                            .color(palette.muted),
                                    );
                                },
                            );
                        }

                        for &(property, col_w) in &property_columns {
                            let (cell_rect, response) = table_cell(
                                ui,
                                col_w,
                                TABLE_ROW_HEIGHT,
                                palette,
                                false,
                                is_row_selected,
                                |ui, _| {
                                    ui.push_id(
                                        ("table-prop-cell", row.page.id, property.id),
                                        |ui| {
                                            let response = property_editor(
                                                ui,
                                                row,
                                                property,
                                                PropertyEditorStyle {
                                                    width: col_w,
                                                    busy,
                                                    palette,
                                                },
                                                intents,
                                                drafts.values,
                                            );
                                            if table_navigation::is_editable(property) {
                                                navigation.register(
                                                    ui,
                                                    row.page.id,
                                                    Some(property.id),
                                                    &response,
                                                );
                                            }
                                            response
                                        },
                                    )
                                    .inner
                                },
                            );

                            let is_text_editable = matches!(
                                property.kind.as_str(),
                                "text" | "number" | "url" | "email" | "phone"
                            );
                            if is_text_editable && !busy {
                                let in_cell =
                                    pointer_pos.is_some_and(|pos| cell_rect.contains(pos));
                                let in_widget =
                                    pointer_pos.is_some_and(|pos| response.rect.contains(pos));
                                if in_cell && !marquee_active {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                    if primary_clicked
                                        && !is_ctrl
                                        && !is_shift
                                        && ui.ctx().dragged_id().is_none()
                                        && !in_widget
                                    {
                                        response.request_focus();
                                        let edit_id = egui::Id::new((
                                            "database-property-value",
                                            row.page.id,
                                            property.id,
                                        ));
                                        let mut state =
                                            egui::text_edit::TextEditState::load(ui.ctx(), edit_id)
                                                .unwrap_or_default();
                                        let current_val = drafts
                                            .values
                                            .get(&(row.page.id, property.id))
                                            .or_else(|| row.values.get(&property.id))
                                            .map(String::as_str)
                                            .unwrap_or("");
                                        let char_count = current_val.chars().count();
                                        state.cursor.set_char_range(Some(CCursorRange::two(
                                            CCursor::new(char_count),
                                            CCursor::new(char_count),
                                        )));
                                        state.store(ui.ctx(), edit_id);
                                    }
                                }
                            }
                        }
                    });
                    if !busy && data.sort_key == "manual" {
                        table_reorder_target(ui, data, row_id, row_rect, palette, intents);
                    }
                    if let Some(focused) = ui.memory(|memory| memory.focused())
                        && (Some(focused) != focused_before
                            || ui.ctx().read_response(focused).is_some_and(|response| {
                                response.layer_id == ui.layer_id()
                                    && row_rect.contains(response.rect.center())
                            }))
                    {
                        ui.ctx().data_mut(|state| {
                            state.insert_temp(
                                egui::Id::new(("database-table-focused-row", data.page_id)),
                                (focused, row_id),
                            );
                        });
                    }
                    paint_table_row_extension(
                        ui,
                        data_right..table_right,
                        row_rect.top()..row_rect.bottom(),
                        palette,
                    );
                }
            });

            let empty_top = (rows_scroll.inner_rect.top() + rows.len() as f32 * TABLE_ROW_HEIGHT
                - rows_scroll.state.offset.y)
                .max(rows_scroll.inner_rect.top());
            let empty_bottom = rows_scroll.inner_rect.bottom();
            if empty_top < empty_bottom {
                let stroke = egui::Stroke::new(1.0_f32, table_border_color(palette));
                let mut boundary_x = table_left;
                for width in std::iter::once(0.0)
                    .chain(std::iter::once(name_w))
                    .chain(show_created_at.then_some(created_w))
                    .chain(property_columns.iter().map(|&(_, width)| width))
                {
                    boundary_x += width;
                    ui.painter().line_segment(
                        [
                            egui::pos2(boundary_x, empty_top),
                            egui::pos2(boundary_x, empty_bottom),
                        ],
                        stroke,
                    );
                }
            }

            // 3. "+ New" Bottom Row
            let new_row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_cell(
                    ui,
                    name_w,
                    TABLE_ROW_HEIGHT,
                    palette,
                    true,
                    false,
                    |ui, hovered| {
                        let text_color = if hovered {
                            palette.accent
                        } else {
                            palette.muted
                        };
                        let resp = ui.add_sized(
                            [ui.available_width(), TABLE_ROW_HEIGHT],
                            egui::Button::new(
                                RichText::new("+ New")
                                    .font(FontId::proportional(13.0))
                                    .color(text_color),
                            )
                            .frame(false),
                        );
                        if resp.clicked() && !busy {
                            intents
                                .push(Intent::Action(Action::CreateDatabaseRecord(data.page_id)));
                        }
                    },
                );

                if show_created_at {
                    table_cell(
                        ui,
                        created_w,
                        TABLE_ROW_HEIGHT,
                        palette,
                        false,
                        false,
                        |_ui, _| {},
                    );
                }
                for &(_, col_w) in &property_columns {
                    table_cell(
                        ui,
                        col_w,
                        TABLE_ROW_HEIGHT,
                        palette,
                        false,
                        false,
                        |_ui, _| {},
                    );
                }
            });

            paint_table_row_extension(
                ui,
                data_right..table_right,
                new_row.response.rect.top()..new_row.response.rect.bottom(),
                palette,
            );

            if primary_clicked
                && !is_ctrl
                && !is_shift
                && !marquee_active
                && let Some(pos) = pointer_pos
                && rows_area.contains(pos)
                && pos.y > rows_start_cursor.y + (rows.len() as f32 * TABLE_ROW_HEIGHT)
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

            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new(("database-table-row-hit-areas", data.page_id)),
                    row_hit_areas,
                );
                d.insert_temp(marquee_anchor_id, marquee_anchor);
                d.insert_temp(marquee_active_id, marquee_active);
                d.insert_temp(marquee_base_id, marquee_base);
                d.insert_temp(selection_id, selected_rows.clone());
                d.insert_temp(anchor_id, anchor_row);
            });

            let mut settings = data.view_settings.clone();

            // 4. Table Calculation / Footer
            let mut calc_settings_changed = false;
            let footer = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                table_footer_cell(ui, name_w, palette, true, |ui| {
                    ui.label(
                        RichText::new(format!("Count {}", rows.len()))
                            .font(FontId::proportional(11.0))
                            .color(palette.muted),
                    );
                });
                if show_created_at {
                    table_footer_cell(ui, created_w, palette, false, |_ui| {});
                }
                for &(property, col_w) in &property_columns {
                    if property.kind == "number" {
                        calc_settings_changed |= table_number_footer_cell(
                            ui,
                            &mut settings,
                            (data.page_id, property.id),
                            &rows,
                            drafts.values,
                            col_w,
                            palette,
                        );
                    } else {
                        table_footer_cell(ui, col_w, palette, false, |_ui| {});
                    }
                }
            });

            paint_table_row_extension(
                ui,
                data_right..table_right,
                footer.response.rect.top()..footer.response.rect.bottom(),
                palette,
            );

            let table_bottom = ui.cursor().min.y;
            let total_h = (table_bottom - table_top).max(TABLE_HEADER_HEIGHT);
            let height_changed = show_vertical_resize_handle(
                ui,
                data.page_id,
                &mut settings.table_height,
                data_columns_width,
                active_rows_h,
                TABLE_ROW_HEIGHT,
                palette,
            );
            let mut settings_changed = calc_settings_changed || height_changed;

            let mut boundary_x = table_left + name_w;

            if let Some(new_w) = handle_column_resize(
                ui,
                ("name", data.page_id),
                ColumnResize {
                    x: boundary_x,
                    vertical_range: table_top..(table_top + total_h),
                    width: name_w,
                    minimum_width: TABLE_NAME_MIN_WIDTH,
                },
                palette,
                |ui| auto_fit_name_column(ui, &rows),
            ) {
                settings.table_name_width = Some(new_w);
                settings_changed = true;
            }

            if show_created_at {
                boundary_x += created_w;
                if let Some(new_w) = handle_column_resize(
                    ui,
                    ("created", data.page_id),
                    ColumnResize {
                        x: boundary_x,
                        vertical_range: table_top..(table_top + total_h),
                        width: created_w,
                        minimum_width: 80.0,
                    },
                    palette,
                    |ui| auto_fit_created_column(ui, &rows),
                ) {
                    settings.table_created_width = Some(new_w);
                    settings_changed = true;
                }
            }

            for &(prop, col_w) in &property_columns {
                boundary_x += col_w;
                if let Some(new_w) = handle_column_resize(
                    ui,
                    ("prop", data.page_id, prop.id),
                    ColumnResize {
                        x: boundary_x,
                        vertical_range: table_top..(table_top + total_h),
                        width: col_w,
                        minimum_width: 64.0,
                    },
                    palette,
                    |ui| auto_fit_property_column(ui, prop, &rows),
                ) {
                    settings.table_column_widths.insert(prop.id, new_w);
                    settings_changed = true;
                }
            }

            settings_changed.then_some(settings)
        })
        .inner;

    if let Some(settings) = settings {
        data.view_settings = settings.clone();
        intents.push(Intent::Action(Action::SetDatabaseViewSettings {
            page: data.page_id,
            settings,
        }));
    }
    navigation.finish(ui, intents);
}

struct HeaderColumns<'a> {
    name: f32,
    created: Option<f32>,
    properties: &'a [(&'a DatabaseProperty, f32)],
}

fn show_table_header(
    ui: &mut Ui,
    data: &DatabaseData,
    columns: HeaderColumns<'_>,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
    drafts: &mut Drafts<'_>,
) -> egui::Rect {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        table_cell(
            ui,
            columns.name,
            TABLE_HEADER_HEIGHT,
            palette,
            true,
            false,
            |ui, _| {
                show_sort_header(ui, data, "title", "Aa  Name", palette, intents);
            },
        );
        if let Some(width) = columns.created {
            table_cell(
                ui,
                width,
                TABLE_HEADER_HEIGHT,
                palette,
                false,
                false,
                |ui, _| {
                    show_sort_header(ui, data, "created_at", "🕒  Created", palette, intents);
                },
            );
        }
        for &(property, width) in columns.properties {
            table_cell(
                ui,
                width,
                TABLE_HEADER_HEIGHT,
                palette,
                false,
                false,
                |ui, hovered| {
                    show_property_header(
                        ui,
                        data,
                        property,
                        PropertyEditorStyle {
                            width,
                            busy,
                            palette,
                        },
                        hovered,
                        intents,
                        drafts,
                    );
                },
            );
        }
        show_add_property_header(ui, data, palette, intents);
    })
    .response
    .rect
}

fn show_sort_header(
    ui: &mut Ui,
    data: &DatabaseData,
    key: &str,
    label: &str,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let (arrow, color) = if data.sort_key == key {
        (if data.sort_descending { " ▼" } else { " ▲" }, palette.text)
    } else {
        ("", palette.muted)
    };
    let response = ui.add_sized(
        [ui.available_width(), TABLE_HEADER_HEIGHT],
        egui::Button::new(
            RichText::new(format!("{label}{arrow}"))
                .font(FontId::proportional(12.5))
                .color(color)
                .strong(),
        )
        .frame(false),
    );
    if response.clicked() {
        toggle_sort(data, key, intents);
    }
}

fn show_add_property_header(
    ui: &mut Ui,
    data: &DatabaseData,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    // Hover-only "+ Property" cell on the right of the header
    let (add_rect, add_resp) = ui.allocate_exact_size(
        egui::vec2(TABLE_ADD_PROP_WIDTH, TABLE_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    let popup_open_id = ui.id().with(("add-prop-popup-open", data.page_id));
    let was_open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(popup_open_id))
        .unwrap_or(false);
    let show_add = add_resp.hovered()
        || was_open
        || ui.rect_contains_pointer(add_rect.expand2(egui::vec2(18.0, 12.0)));
    let stroke = egui::Stroke::new(1.0_f32, table_border_color(palette));
    ui.painter()
        .line_segment([add_rect.left_top(), add_rect.left_bottom()], stroke);
    if show_add {
        let fill = if add_resp.hovered() {
            palette.hover
        } else {
            palette.background
        };
        ui.painter().rect_filled(add_rect, 0.0, fill);
        ui.painter()
            .line_segment([add_rect.left_top(), add_rect.right_top()], stroke);
        ui.painter()
            .line_segment([add_rect.left_bottom(), add_rect.right_bottom()], stroke);
        ui.painter()
            .line_segment([add_rect.right_top(), add_rect.right_bottom()], stroke);

        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(add_rect).layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        ));
        let menu_resp = child_ui.persistent_menu_button(
            RichText::new("+").size(16.0).color(palette.muted).strong(),
            |ui| {
                ui.ctx().data_mut(|d| d.insert_temp(popup_open_id, true));
                add_property_menu(ui, data, intents);
            },
        );
        if menu_resp.inner.is_none() {
            ui.ctx().data_mut(|d| d.insert_temp(popup_open_id, false));
        }
    } else {
        ui.ctx().data_mut(|d| d.insert_temp(popup_open_id, false));
    }
}

fn table_reorder_target(
    ui: &Ui,
    data: &DatabaseData,
    target: crate::domain::model::PageId,
    rect: egui::Rect,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let response = ui.interact(
        rect.intersect(ui.clip_rect()),
        ui.id().with(("table-row-reorder", target)),
        egui::Sense::hover(),
    );
    let Some(payload) = response.dnd_hover_payload::<crate::ui::page_transfer::PageDragPayload>()
    else {
        return;
    };
    if payload.pages.contains(&target)
        || !payload
            .pages
            .iter()
            .all(|id| data.rows.iter().any(|row| row.page.id == *id))
    {
        return;
    }
    let after = ui.input(|input| {
        input
            .pointer
            .interact_pos()
            .is_some_and(|position| position.y >= rect.center().y)
    });
    let y = if after { rect.bottom() } else { rect.top() };
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(2.0_f32, palette.accent),
    );
    if let Some(payload) =
        response.dnd_release_payload::<crate::ui::page_transfer::PageDragPayload>()
    {
        intents.push(Intent::Action(Action::ReorderDatabaseRecords {
            database: data.page_id,
            records: payload.pages.clone(),
            target,
            after,
        }));
    }
}

fn paint_table_row_extension(
    ui: &Ui,
    horizontal: std::ops::Range<f32>,
    vertical: std::ops::Range<f32>,
    palette: Palette,
) {
    if horizontal.is_empty() {
        return;
    }
    let stroke = egui::Stroke::new(1.0_f32, table_border_color(palette));
    for y in [vertical.start, vertical.end] {
        ui.painter().line_segment(
            [
                egui::pos2(horizontal.start, y),
                egui::pos2(horizontal.end, y),
            ],
            stroke,
        );
    }
}

fn table_cell<R>(
    ui: &mut Ui,
    width: f32,
    height: f32,
    palette: Palette,
    is_first_column: bool,
    is_selected: bool,
    add_contents: impl FnOnce(&mut Ui, bool) -> R,
) -> (egui::Rect, R) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    let fill = if is_selected {
        palette.accent_soft
    } else if hovered {
        palette.hover
    } else {
        palette.background
    };
    ui.painter().rect_filled(rect, 0.0, fill);

    let stroke = egui::Stroke::new(1.0_f32, table_border_color(palette));
    ui.painter()
        .line_segment([rect.left_top(), rect.right_top()], stroke);
    ui.painter()
        .line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    ui.painter()
        .line_segment([rect.left_top(), rect.left_bottom()], stroke);
    ui.painter()
        .line_segment([rect.right_top(), rect.right_bottom()], stroke);

    if is_selected && is_first_column {
        ui.painter().line_segment(
            [rect.left_top(), rect.left_bottom()],
            egui::Stroke::new(3.0_f32, palette.accent),
        );
    }

    let content_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 4.0, rect.bottom()),
    );
    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("table-cell-content", rect.min.x as i32))
            .max_rect(content_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    let inner = add_contents(&mut child_ui, hovered);
    (rect, inner)
}
