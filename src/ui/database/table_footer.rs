use std::collections::HashMap;

use eframe::egui::{self, FontId, RichText, Ui};

use crate::domain::model::{DatabaseRow, DatabaseViewSettings, NumberCalculation, PageId};

use crate::ui::palette::Palette;

use super::shared::table_border_color;

pub(super) fn table_footer_cell(
    ui: &mut Ui,
    width: f32,
    palette: Palette,
    _is_first_col: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::hover());
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    let fill = if hovered {
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

    let content_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 4.0, rect.bottom()),
    );
    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    add_contents(&mut child_ui);
}

pub(super) fn table_number_footer_cell(
    ui: &mut Ui,
    settings: &mut DatabaseViewSettings,
    cell: (PageId, i64),
    rows: &[&DatabaseRow],
    value_drafts: &HashMap<(i64, i64), String>,
    width: f32,
    palette: Palette,
) -> bool {
    let mut changed = false;
    let (database, property) = cell;
    let current_calc = settings
        .table_column_calculations
        .get(&property)
        .copied()
        .unwrap_or_default();

    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());
    let popup_id = ui.id().with(("table-num-calc-popup", database, property));
    let popup_open = ui.memory(|mem| mem.is_popup_open(popup_id));
    let hovered = response.hovered() || ui.rect_contains_pointer(rect) || popup_open;

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    let fill = if hovered {
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

    let content_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 4.0, rect.bottom()),
    );
    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child_ui.set_clip_rect(rect.intersect(ui.clip_rect()));

    if current_calc != NumberCalculation::None {
        let numbers: Vec<f64> = rows
            .iter()
            .filter_map(|row| {
                let raw = value_drafts
                    .get(&(row.page.id, property))
                    .or_else(|| row.values.get(&property))?;
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return None;
                }
                trimmed
                    .replace(',', ".")
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
            })
            .collect();
        let result_text = current_calc.format_result(&numbers);
        child_ui.label(
            RichText::new(result_text)
                .font(FontId::proportional(11.5))
                .color(palette.text)
                .strong(),
        );
    } else if hovered {
        child_ui.label(
            RichText::new("Hesapla ▾")
                .font(FontId::proportional(11.0))
                .color(palette.muted),
        );
    }

    crate::ui::menus::popup_below_widget(ui, popup_id, &response, |ui| {
        ui.set_min_width(160.0);
        for calc in NumberCalculation::ALL {
            if ui
                .selectable_label(current_calc == calc, calc.menu_label())
                .clicked()
            {
                if calc == NumberCalculation::None {
                    if settings
                        .table_column_calculations
                        .remove(&property)
                        .is_some()
                    {
                        changed = true;
                    }
                } else if settings.table_column_calculations.insert(property, calc) != Some(calc) {
                    changed = true;
                }
                ui.memory_mut(|mem| mem.close_popup());
            }
        }
    });

    changed
}
