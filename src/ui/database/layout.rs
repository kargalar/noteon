use std::collections::HashMap;

use eframe::egui::{self, Ui};

use crate::domain::model::{DatabaseData, DatabaseView, PageId};

use crate::ui::palette::Palette;

use super::board::DEFAULT_BOARD_MAX_CARDS_HEIGHT;
use super::table_columns::{DEFAULT_TABLE_MAX_ROWS_HEIGHT, TABLE_HEADER_HEIGHT, TABLE_ROW_HEIGHT};

pub(super) const DEFAULT_GALLERY_MAX_HEIGHT: f32 = 760.0;

pub(super) fn resolve_vertical_height(
    ui: &Ui,
    data: &DatabaseData,
    natural_h: f32,
    default_max_h: f32,
    min_h: f32,
) -> (f32, bool) {
    let default_h = natural_h.max(min_h).min(default_max_h);
    let stored_h = data.view_settings.table_height.unwrap_or(default_h);
    let live_v_id = egui::Id::new(("table-v-resize-live", data.page_id));
    let live_h = ui.ctx().data(|d| d.get_temp::<f32>(live_v_id));
    let has_custom = data.view_settings.table_height.is_some() || live_h.is_some();
    let active_h = live_h.unwrap_or(stored_h).clamp(min_h, 2400.0);
    (active_h, has_custom)
}

pub(super) fn show_vertical_resize_handle(
    ui: &mut Ui,
    page: PageId,
    height: &mut Option<f32>,
    width: f32,
    active_h: f32,
    min_h: f32,
    palette: Palette,
) -> bool {
    let live_v_id = egui::Id::new(("table-v-resize-live", page));
    let handle_w = width.max(160.0);
    let (v_handle_rect, v_handle_resp) =
        ui.allocate_exact_size(egui::vec2(handle_w, 12.0), egui::Sense::click_and_drag());
    let v_handle_resp = v_handle_resp
        .on_hover_cursor(egui::CursorIcon::ResizeVertical)
        .on_hover_text("Drag to resize height · Double-click to auto-fit");

    let grip_color = if v_handle_resp.hovered() || v_handle_resp.dragged() {
        palette.accent
    } else {
        palette.border.linear_multiply(0.8)
    };
    if v_handle_resp.hovered() || v_handle_resp.dragged() {
        ui.painter().line_segment(
            [
                egui::pos2(v_handle_rect.left(), v_handle_rect.center().y),
                egui::pos2(v_handle_rect.right(), v_handle_rect.center().y),
            ],
            egui::Stroke::new(2.0_f32, palette.accent),
        );
    }
    ui.painter().rect_filled(
        egui::Rect::from_center_size(v_handle_rect.center(), egui::vec2(40.0, 4.0)),
        2.0,
        grip_color,
    );

    let mut settings_changed = false;
    if v_handle_resp.double_clicked() {
        ui.ctx().data_mut(|d| d.remove::<f32>(live_v_id));
        *height = None;
        settings_changed = true;
    } else {
        if v_handle_resp.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(live_v_id, active_h));
        }
        if v_handle_resp.dragged() {
            let prev = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(live_v_id))
                .unwrap_or(active_h);
            let next = (prev + v_handle_resp.drag_delta().y).clamp(min_h, 2400.0);
            ui.ctx().data_mut(|d| d.insert_temp(live_v_id, next));
        }
        if v_handle_resp.drag_stopped() {
            let final_h = ui.ctx().data(|d| d.get_temp::<f32>(live_v_id));
            ui.ctx().data_mut(|d| d.remove::<f32>(live_v_id));
            if let Some(final_h) = final_h {
                *height = Some(final_h);
                settings_changed = true;
            }
        }
    }
    settings_changed
}

pub(super) fn gallery_card_metrics(data: &DatabaseData, gallery: bool) -> (f32, f32) {
    let settings = &data.view_settings;
    let show_cover = gallery && settings.gallery_show_cover;
    let show_title = !gallery || settings.gallery_show_title;
    let show_created_at = if gallery {
        settings.gallery_show_created_at
    } else {
        true
    };
    let show_description = gallery && settings.gallery_show_description;
    let visible_properties = if gallery {
        settings.gallery_visible_properties.as_deref()
    } else {
        None
    };
    let property_count =
        visible_properties.map_or_else(|| data.properties.len().min(2), |visible| visible.len());
    let cover_h: f32 = if show_cover { 130.0 } else { 0.0 };
    let title_h: f32 = if show_title { 28.0 } else { 0.0 };
    let created_h: f32 = if show_created_at { 20.0 } else { 0.0 };
    let desc_h: f32 = if show_description { 38.0 } else { 0.0 };
    let prop_h: f32 = if property_count > 0 { 24.0 } else { 0.0 };
    let card_height: f32 = (cover_h + title_h + created_h + desc_h + prop_h + 24.0).max(96.0);
    let row_gap = 12.0;
    (card_height, row_gap)
}

pub(super) fn board_max_column_cards_height(data: &DatabaseData, status_id: i64) -> f32 {
    let mut heights = HashMap::<String, f32>::new();
    let mut counts = HashMap::<String, usize>::new();
    let board_visible_properties = data.view_settings.board_visible_properties.as_deref();
    let has_other_props = data
        .properties
        .iter()
        .filter(|p| p.id != status_id)
        .any(|p| {
            board_visible_properties.map_or_else(
                || {
                    data.properties
                        .iter()
                        .filter(|c| c.id != status_id)
                        .take(2)
                        .any(|v| v.id == p.id)
                },
                |visible| visible.contains(&p.id),
            )
        });
    for row in &data.rows {
        let val = row.values.get(&status_id).map(String::as_str).unwrap_or("");
        let group = if val.is_empty() { "No selection" } else { val };
        let show_desc = data.view_settings.board_show_description && !row.preview.trim().is_empty();
        let card_h =
            36.0 + if has_other_props { 20.0 } else { 0.0 } + if show_desc { 22.0 } else { 0.0 };
        *heights.entry(group.to_owned()).or_insert(0.0) += card_h;
        *counts.entry(group.to_owned()).or_insert(0) += 1;
    }
    heights
        .into_iter()
        .map(|(g, h)| {
            let c = counts.get(&g).copied().unwrap_or(0);
            h + c.saturating_sub(1) as f32 * 8.0 + 8.0
        })
        .fold(96.0_f32, f32::max)
}

pub fn desired_embedded_height(ui: &Ui, data: &DatabaseData) -> f32 {
    let chrome_height = 90.0;
    match data.view {
        DatabaseView::Table => {
            let natural_rows_h = (data.rows.len() as f32 * TABLE_ROW_HEIGHT).max(TABLE_ROW_HEIGHT);
            let (rows_h, _) = resolve_vertical_height(
                ui,
                data,
                natural_rows_h,
                DEFAULT_TABLE_MAX_ROWS_HEIGHT,
                TABLE_ROW_HEIGHT,
            );
            let table_total = TABLE_HEADER_HEIGHT + rows_h + TABLE_ROW_HEIGHT + 24.0 + 24.0;
            (chrome_height + table_total).clamp(180.0, 2600.0)
        }
        DatabaseView::Gallery => {
            let cols = usize::from(data.gallery_columns).max(1);
            let row_count = data.rows.len().div_ceil(cols);
            let (card_h, row_gap) = gallery_card_metrics(data, true);
            let natural_h = if row_count == 0 {
                card_h
            } else {
                row_count as f32 * card_h + row_count.saturating_sub(1) as f32 * row_gap
            };
            let (active_h, _) =
                resolve_vertical_height(ui, data, natural_h, DEFAULT_GALLERY_MAX_HEIGHT, card_h);
            let gallery_total = active_h + 40.0 + 24.0;
            (chrome_height + gallery_total).clamp(200.0, 2600.0)
        }
        DatabaseView::Board => {
            let status_id = data
                .properties
                .iter()
                .find(|p| p.kind == "status" || p.kind == "select")
                .map(|p| p.id);
            if let Some(status_id) = status_id {
                let natural_cards_h = board_max_column_cards_height(data, status_id);
                let (active_cards_h, _) = resolve_vertical_height(
                    ui,
                    data,
                    natural_cards_h,
                    DEFAULT_BOARD_MAX_CARDS_HEIGHT,
                    96.0,
                );
                let board_total = active_cards_h + 104.0 + 24.0;
                (chrome_height + board_total).clamp(240.0, 2600.0)
            } else {
                let (card_h, row_gap) = gallery_card_metrics(data, false);
                let row_count = data.rows.len();
                let natural_h = if row_count == 0 {
                    card_h
                } else {
                    row_count as f32 * card_h + row_count.saturating_sub(1) as f32 * row_gap
                };
                let (active_h, _) = resolve_vertical_height(
                    ui,
                    data,
                    natural_h,
                    DEFAULT_GALLERY_MAX_HEIGHT,
                    card_h,
                );
                (chrome_height + active_h + 88.0).clamp(220.0, 2600.0)
            }
        }
    }
}
