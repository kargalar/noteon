use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};
use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::domain::model::{
    DEFAULT_DATABASE_STATUS_OPTIONS, PROPERTY_TYPE_GROUPS, PROPERTY_TYPES, PageId, PageProperty,
    PropertyOption,
};
use crate::ui::{Intent, palette::Palette};
use crate::worker::Action;

pub const LABEL_COLUMN_WIDTH: f32 = 168.0;
const ROW_MIN_HEIGHT: f32 = 30.0;

pub const PRESET_OPTION_COLORS: [(&str, Color32); 10] = [
    ("Default", Color32::from_rgb(140, 140, 140)),
    ("Red", Color32::from_rgb(224, 62, 62)),
    ("Orange", Color32::from_rgb(217, 115, 13)),
    ("Yellow", Color32::from_rgb(204, 148, 56)),
    ("Green", Color32::from_rgb(76, 154, 102)),
    ("Teal", Color32::from_rgb(78, 156, 142)),
    ("Blue", Color32::from_rgb(68, 132, 206)),
    ("Purple", Color32::from_rgb(146, 106, 190)),
    ("Pink", Color32::from_rgb(184, 102, 144)),
    ("Brown", Color32::from_rgb(154, 114, 88)),
];

pub fn resolve_option_color(color_hex: Option<&str>) -> Option<Color32> {
    color_hex.and_then(Palette::from_setting)
}

#[derive(Clone, Copy)]
pub struct PagePropertyTarget {
    pub page: PageId,
    pub widget_scope: Option<u64>,
    pub indent: f32,
    pub max_width: f32,
}

pub fn property_icon(kind: &str) -> &'static str {
    match kind {
        "text" => "≡",
        "number" => "#",
        "select" => "◉",
        "multi-select" => "☰",
        "status" => "◌",
        "date" => "📅",
        "person" => "👥",
        "files" => "📎",
        "checkbox" => "☑",
        "url" => "🔗",
        "phone" => "📞",
        "email" => "@",
        "relation" => "↗",
        "rollup" => "🔍",
        "formula" => "Σ",
        "button" => "⇪",
        "id" => "№",
        "place" => "📍",
        "created_time" => "🕒",
        "last_edited_time" => "🕒",
        "created_by" => "👤",
        "last_edited_by" => "👤",
        _ => "•",
    }
}

pub fn paint_property_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    kind: &str,
    color: Color32,
    bg_color: Color32,
) {
    let c = rect.center();
    let stroke = Stroke::new(1.35, color);
    match kind {
        "text" => {
            let l = rect.left() + 2.0;
            let r = rect.right() - 2.0;
            painter.line_segment([egui::pos2(l, c.y - 4.0), egui::pos2(r, c.y - 4.0)], stroke);
            painter.line_segment([egui::pos2(l, c.y), egui::pos2(r, c.y)], stroke);
            painter.line_segment(
                [
                    egui::pos2(l, c.y + 4.0),
                    egui::pos2(l + (r - l) * 0.6, c.y + 4.0),
                ],
                stroke,
            );
        }
        "number" => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.5, c.y - 5.5),
                    egui::pos2(c.x - 3.0, c.y + 5.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 3.0, c.y - 5.5),
                    egui::pos2(c.x + 1.5, c.y + 5.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 5.5, c.y - 2.0),
                    egui::pos2(c.x + 5.5, c.y - 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 5.5, c.y + 2.0),
                    egui::pos2(c.x + 5.5, c.y + 2.0),
                ],
                stroke,
            );
        }
        "select" => {
            painter.circle_stroke(c, 6.0, stroke);
            let pts = vec![
                egui::pos2(c.x - 2.6, c.y - 1.2),
                egui::pos2(c.x + 2.6, c.y - 1.2),
                egui::pos2(c.x, c.y + 2.2),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        "multi-select" => {
            let l = rect.left() + 2.5;
            let r = rect.right() - 2.0;
            for dy in [-4.0, 0.0, 4.0] {
                painter.circle_filled(egui::pos2(l + 1.0, c.y + dy), 1.15, color);
                painter.line_segment(
                    [egui::pos2(l + 4.5, c.y + dy), egui::pos2(r, c.y + dy)],
                    stroke,
                );
            }
        }
        "status" => {
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                let p1 = egui::pos2(c.x + cos * 3.0, c.y + sin * 3.0);
                let p2 = egui::pos2(c.x + cos * 6.0, c.y + sin * 6.0);
                painter.line_segment([p1, p2], stroke);
            }
        }
        "date" => {
            let box_rect =
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 0.8), vec2(11.5, 10.0));
            painter.rect_stroke(box_rect, 1.8, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    egui::pos2(box_rect.left(), box_rect.top() + 3.2),
                    egui::pos2(box_rect.right(), box_rect.top() + 3.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, box_rect.top() - 1.8),
                    egui::pos2(c.x - 3.0, box_rect.top() + 1.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 3.0, box_rect.top() - 1.8),
                    egui::pos2(c.x + 3.0, box_rect.top() + 1.2),
                ],
                stroke,
            );
        }
        "person" => {
            painter.circle_filled(egui::pos2(c.x - 2.0, c.y - 2.2), 2.1, color);
            let left_body =
                egui::Rect::from_center_size(egui::pos2(c.x - 2.0, c.y + 3.2), vec2(6.8, 4.2));
            painter.rect_filled(left_body, 2.2, color);
            let muted = color.gamma_multiply(0.75);
            painter.circle_filled(egui::pos2(c.x + 3.0, c.y - 2.6), 1.7, muted);
            let right_body =
                egui::Rect::from_center_size(egui::pos2(c.x + 3.2, c.y + 2.4), vec2(5.2, 3.6));
            painter.rect_filled(right_body, 1.8, muted);
        }
        "files" => {
            let p1 = egui::pos2(c.x - 1.5, c.y + 3.5);
            let p2 = egui::pos2(c.x + 3.5, c.y - 1.5);
            let p3 = egui::pos2(c.x + 1.0, c.y - 4.5);
            let p4 = egui::pos2(c.x - 4.5, c.y + 1.0);
            let p5 = egui::pos2(c.x - 1.5, c.y + 4.5);
            let p6 = egui::pos2(c.x + 4.5, c.y - 1.5);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
            painter.line_segment([p3, p4], stroke);
            painter.line_segment([p4, p5], stroke);
            painter.line_segment([p5, p6], stroke);
        }
        "checkbox" => {
            let box_rect = egui::Rect::from_center_size(c, vec2(11.5, 11.5));
            painter.rect_filled(box_rect, 2.2, color);
            let check_stroke = Stroke::new(1.6, bg_color);
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y + 0.2),
                    egui::pos2(c.x - 0.8, c.y + 2.6),
                ],
                check_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 0.8, c.y + 2.6),
                    egui::pos2(c.x + 3.4, c.y - 2.2),
                ],
                check_stroke,
            );
        }
        "url" => {
            let r1 = egui::Rect::from_center_size(egui::pos2(c.x - 2.0, c.y + 2.0), vec2(6.5, 4.5));
            let r2 = egui::Rect::from_center_size(egui::pos2(c.x + 2.0, c.y - 2.0), vec2(6.5, 4.5));
            painter.rect_stroke(r1, 2.2, stroke, egui::StrokeKind::Inside);
            painter.rect_stroke(r2, 2.2, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    egui::pos2(c.x - 2.0, c.y + 2.0),
                    egui::pos2(c.x + 2.0, c.y - 2.0),
                ],
                stroke,
            );
        }
        "phone" => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 4.2, c.y - 4.5),
                    egui::pos2(c.x - 2.0, c.y - 4.5),
                ],
                Stroke::new(2.0, color),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.5, c.y - 4.0),
                    egui::pos2(c.x - 1.5, c.y + 1.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.5, c.y + 1.0),
                    egui::pos2(c.x + 3.5, c.y + 3.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 2.5, c.y + 4.2),
                    egui::pos2(c.x + 4.5, c.y + 2.5),
                ],
                Stroke::new(2.0, color),
            );
        }
        "email" => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "@",
                FontId::proportional(13.5),
                color,
            );
        }
        "relation" => {
            let start = egui::pos2(c.x - 4.2, c.y + 4.2);
            let end = egui::pos2(c.x + 4.2, c.y - 4.2);
            painter.line_segment([start, end], stroke);
            painter.line_segment([egui::pos2(end.x - 4.5, end.y), end], stroke);
            painter.line_segment([egui::pos2(end.x, end.y + 4.5), end], stroke);
        }
        "rollup" => {
            let center = egui::pos2(c.x - 1.2, c.y - 1.2);
            painter.circle_stroke(center, 4.0, stroke);
            painter.line_segment(
                [
                    egui::pos2(c.x + 1.8, c.y + 1.8),
                    egui::pos2(c.x + 5.2, c.y + 5.2),
                ],
                stroke,
            );
        }
        "formula" => {
            let pts = [
                egui::pos2(c.x + 3.8, c.y - 5.0),
                egui::pos2(c.x - 3.8, c.y - 5.0),
                egui::pos2(c.x + 0.8, c.y),
                egui::pos2(c.x - 3.8, c.y + 5.0),
                egui::pos2(c.x + 3.8, c.y + 5.0),
            ];
            for pair in pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }
        "button" => {
            let btn_rect =
                egui::Rect::from_center_size(egui::pos2(c.x - 0.5, c.y - 2.0), vec2(11.0, 5.5));
            painter.rect_filled(btn_rect, 2.0, color);
            let tip = egui::pos2(c.x + 0.5, c.y + 0.5);
            painter.line_segment([tip, egui::pos2(c.x + 2.5, c.y + 5.5)], stroke);
            painter.line_segment([tip, egui::pos2(c.x + 4.5, c.y + 3.0)], stroke);
        }
        "id" => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "№",
                FontId::proportional(12.5),
                color,
            );
        }
        "place" => {
            let top_c = egui::pos2(c.x, c.y - 1.8);
            painter.circle_stroke(top_c, 3.8, stroke);
            painter.circle_filled(top_c, 1.2, color);
            let bottom = egui::pos2(c.x, c.y + 5.8);
            painter.line_segment([egui::pos2(c.x - 3.4, c.y - 0.2), bottom], stroke);
            painter.line_segment([egui::pos2(c.x + 3.4, c.y - 0.2), bottom], stroke);
        }
        "created_time" | "last_edited_time" => {
            painter.circle_stroke(c, 5.8, stroke);
            painter.line_segment([c, egui::pos2(c.x, c.y - 3.4)], stroke);
            painter.line_segment([c, egui::pos2(c.x + 2.8, c.y + 0.5)], stroke);
        }
        "created_by" | "last_edited_by" => {
            painter.circle_stroke(c, 6.0, stroke);
            painter.circle_stroke(egui::pos2(c.x, c.y - 1.5), 1.9, stroke);
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.4, c.y + 4.2),
                    egui::pos2(c.x - 1.8, c.y + 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.8, c.y + 2.0),
                    egui::pos2(c.x + 1.8, c.y + 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 1.8, c.y + 2.0),
                    egui::pos2(c.x + 3.4, c.y + 4.2),
                ],
                stroke,
            );
        }
        _ => {
            painter.circle_filled(c, 2.5, color);
        }
    }
}

pub fn show_property_type_picker(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + Copy,
    current_kind: Option<&str>,
    busy: bool,
    palette: Palette,
) -> Option<&'static str> {
    let col_w = 148.0;
    let col_gap = 6.0;
    let total_w = col_w * 2.0 + col_gap;
    ui.set_min_width(total_w);
    ui.set_max_width(total_w);

    let search_key = ("prop-type-picker-search", id_salt);
    let mut query = read_draft(ui, search_key, "");

    // "Select type 🔍" search/filter bar
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(4.0);
        let edit_w = if query.is_empty() { 265.0 } else { 248.0 };
        let resp = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut query)
                .hint_text("Select type")
                .font(FontId::proportional(12.5))
                .frame(false)
                .margin(egui::Margin::symmetric(2, 4))
                .desired_width(edit_w),
        );
        save_draft(ui, search_key, &query, resp.changed());

        if query.is_empty() {
            let mag_pos = egui::pos2(resp.rect.left() + 76.0, resp.rect.center().y);
            let mag_rect = egui::Rect::from_center_size(mag_pos, vec2(13.0, 13.0));
            paint_property_icon(
                ui.painter(),
                mag_rect,
                "rollup",
                palette.muted,
                palette.background,
            );
        } else if ui
            .add(
                egui::Button::new(
                    RichText::new("✕")
                        .font(FontId::proportional(11.0))
                        .color(palette.muted),
                )
                .frame(false),
            )
            .on_hover_text("Clear filter")
            .clicked()
        {
            query.clear();
            clear_draft(ui, search_key);
        }
    });

    ui.add_space(2.0);

    let query_lower = query.trim().to_ascii_lowercase();
    let mut selected_kind: Option<&'static str> = None;

    egui::ScrollArea::vertical()
        .id_salt(("prop-type-picker-scroll", id_salt))
        .max_height(380.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(col_gap, 2.0);

            let render_pair =
                |ui: &mut Ui,
                 pair: &[(&'static str, &'static str)],
                 selected_kind: &mut Option<&'static str>| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = col_gap;
                        for &(kind, label) in pair {
                            let is_selected = current_kind == Some(kind);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(col_w, 28.0), Sense::click());
                            let hovered = !busy && resp.hovered();
                            if hovered {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let row_bg = if is_selected {
                                palette.accent_soft
                            } else if hovered {
                                palette.hover
                            } else {
                                Color32::TRANSPARENT
                            };
                            if row_bg != Color32::TRANSPARENT {
                                ui.painter().rect_filled(rect, 5.0, row_bg);
                            }
                            let icon_bg = if row_bg != Color32::TRANSPARENT {
                                row_bg
                            } else {
                                palette.background
                            };
                            let icon_color = if is_selected {
                                palette.accent
                            } else if hovered {
                                palette.text
                            } else {
                                palette.muted
                            };
                            let text_color = if is_selected || hovered {
                                palette.text
                            } else {
                                blend_color(palette.text, palette.muted, 0.15)
                            };
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(rect.left() + 14.0, rect.center().y),
                                vec2(15.0, 15.0),
                            );
                            paint_property_icon(ui.painter(), icon_rect, kind, icon_color, icon_bg);
                            ui.painter().text(
                                egui::pos2(rect.left() + 28.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(13.0),
                                text_color,
                            );
                            if is_selected {
                                ui.painter().text(
                                    egui::pos2(rect.right() - 10.0, rect.center().y),
                                    egui::Align2::CENTER_CENTER,
                                    "✓",
                                    FontId::proportional(12.0),
                                    palette.accent,
                                );
                            }
                            if !busy && resp.clicked() {
                                *selected_kind = Some(kind);
                            }
                        }
                    });
                };

            if query_lower.is_empty() {
                for (group_idx, group) in PROPERTY_TYPE_GROUPS.iter().enumerate() {
                    if group_idx > 0 {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                    }
                    for pair in group.chunks(2) {
                        render_pair(ui, pair, &mut selected_kind);
                    }
                }
            } else {
                let matches: Vec<(&'static str, &'static str)> = PROPERTY_TYPES
                    .into_iter()
                    .filter(|(kind, label)| {
                        label.to_ascii_lowercase().contains(&query_lower)
                            || kind.to_ascii_lowercase().contains(&query_lower)
                    })
                    .collect();
                if matches.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("No matching property type")
                            .font(FontId::proportional(12.0))
                            .color(palette.muted),
                    );
                } else {
                    for pair in matches.chunks(2) {
                        render_pair(ui, pair, &mut selected_kind);
                    }
                }
            }
        });

    if selected_kind.is_some() {
        clear_draft(ui, search_key);
    }

    selected_kind
}

pub fn show_property_type_selector_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + Copy,
    current_kind: &str,
    busy: bool,
    palette: Palette,
) -> Option<&'static str> {
    let mut chosen = None;
    ui.push_id(("prop-type-sel-btn", id_salt), |ui| {
        let icon = property_icon(current_kind);
        let label = type_label(current_kind);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("⇄  Type")
                    .font(FontId::proportional(12.5))
                    .color(palette.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.persistent_menu_button(
                    RichText::new(format!("{icon}  {label}  ▸"))
                        .font(FontId::proportional(12.5))
                        .color(palette.text),
                    |ui| {
                        if let Some(kind) = show_property_type_picker(
                            ui,
                            id_salt,
                            Some(current_kind),
                            busy,
                            palette,
                        ) {
                            if kind != current_kind {
                                chosen = Some(kind);
                            }
                            ui.close_menu();
                        }
                    },
                );
            });
        });
    });
    chosen
}

pub fn type_label(kind: &str) -> &'static str {
    PROPERTY_TYPES
        .iter()
        .find_map(|(candidate, label)| (*candidate == kind).then_some(*label))
        .unwrap_or("Text")
}

pub fn option_base_color(
    palette: Palette,
    kind: &str,
    option: &str,
    custom_color: Option<Color32>,
) -> Color32 {
    if let Some(custom) = custom_color {
        return custom;
    }
    let lower = option.trim().to_lowercase();
    if matches!(
        lower.as_str(),
        "done" | "complete" | "completed" | "tamamlandı" | "bitti" | "aktif" | "onaylandı" | "low"
    ) {
        Color32::from_rgb(76, 154, 102)
    } else if matches!(
        lower.as_str(),
        "in progress" | "doing" | "devam ediyor" | "devam" | "working" | "active"
    ) {
        Color32::from_rgb(68, 132, 206)
    } else if matches!(
        lower.as_str(),
        "review" | "beklemede" | "pending" | "medium" | "orta" | "planned"
    ) {
        Color32::from_rgb(204, 148, 56)
    } else if matches!(
        lower.as_str(),
        "high" | "urgent" | "blocked" | "iptal" | "acil" | "yüksek" | "bug" | "critical"
    ) {
        Color32::from_rgb(206, 88, 82)
    } else if kind == "status"
        || matches!(
            lower.as_str(),
            "to do" | "todo" | "bekliyor" | "not started" | "none" | "backlog"
        )
    {
        palette.muted
    } else {
        let palette_wheel = [
            Color32::from_rgb(92, 138, 198),
            Color32::from_rgb(146, 106, 190),
            Color32::from_rgb(196, 118, 84),
            Color32::from_rgb(78, 156, 142),
            Color32::from_rgb(184, 102, 144),
            Color32::from_rgb(122, 154, 78),
        ];
        let hash = option.bytes().fold(0usize, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(usize::from(b))
        });
        palette_wheel[hash % palette_wheel.len()]
    }
}

pub fn option_pill_colors(
    palette: Palette,
    kind: &str,
    option: &str,
    custom_color: Option<Color32>,
) -> (Color32, Color32, Color32) {
    let base_tint = option_base_color(palette, kind, option, custom_color);
    if kind == "multi-select" {
        let bg = base_tint;
        let border = Color32::TRANSPARENT;
        let text = if palette.is_dark() {
            Color32::WHITE
        } else {
            let brightness = (0.2126 * f32::from(base_tint.r())
                + 0.7152 * f32::from(base_tint.g())
                + 0.0722 * f32::from(base_tint.b()))
                / 255.0;
            if brightness > 0.60 {
                Color32::from_rgb(20, 20, 20)
            } else {
                Color32::WHITE
            }
        };
        (bg, border, text)
    } else {
        let bg = blend_color(palette.background, base_tint, 0.18);
        let border = blend_color(palette.background, base_tint, 0.42);
        let text = blend_color(palette.text, base_tint, 0.35);
        (bg, border, text)
    }
}

pub fn show_color_palette_grid<F>(
    ui: &mut Ui,
    palette: Palette,
    current_color: Option<Color32>,
    mut on_select: F,
) where
    F: FnMut(Option<String>),
{
    ui.set_min_width(170.0);
    ui.label(
        RichText::new("Color")
            .font(FontId::proportional(11.0))
            .color(palette.muted),
    );
    ui.add_space(3.0);

    egui::Grid::new(ui.id().with("color_palette_grid"))
        .spacing(vec2(6.0, 6.0))
        .show(ui, |ui| {
            for (idx, (name, color)) in PRESET_OPTION_COLORS.iter().enumerate() {
                let is_selected = current_color == Some(*color);
                let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                ui.painter().circle_filled(rect.center(), 8.5, *color);
                if is_selected {
                    ui.painter()
                        .circle_stroke(rect.center(), 10.0, Stroke::new(2.0, palette.text));
                } else if resp.hovered() {
                    ui.painter().circle_stroke(
                        rect.center(),
                        10.0,
                        Stroke::new(1.5, palette.accent),
                    );
                }
                if resp.on_hover_text(*name).clicked() {
                    on_select(Some(Palette::to_setting(*color)));
                    ui.close_menu();
                }
                if (idx + 1) % 5 == 0 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(4.0);
    let mut custom = current_color.unwrap_or(palette.accent);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Custom")
                .font(FontId::proportional(11.0))
                .color(palette.muted),
        );
        if ui.color_edit_button_srgba(&mut custom).changed() {
            on_select(Some(Palette::to_setting(custom)));
        }
    });

    ui.add_space(2.0);
    ui.separator();
    if ui
        .button(
            RichText::new("↺  Reset to auto")
                .font(FontId::proportional(11.5))
                .color(palette.muted),
        )
        .on_hover_text("Use automatic color")
        .clicked()
    {
        on_select(None);
        ui.close_menu();
    }
}

fn blend_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

pub struct RecordPropertyContext<'a> {
    pub data: &'a crate::domain::model::DatabaseData,
    pub record_page: i64,
    pub property_drafts: &'a mut std::collections::HashMap<i64, (String, String)>,
    pub title_drafts: &'a mut std::collections::HashMap<i64, String>,
    pub value_drafts: &'a mut std::collections::HashMap<(i64, i64), String>,
}

pub fn show(
    ui: &mut Ui,
    target: PagePropertyTarget,
    created_at: i64,
    properties: &[PageProperty],
    mut record_context: Option<&mut RecordPropertyContext<'_>>,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let PagePropertyTarget {
        page,
        widget_scope,
        indent,
        max_width,
    } = target;
    let scope_id = widget_scope.unwrap_or_default();
    let section_width = max_width.clamp(300.0, 680.0);
    let collapse_id = egui::Id::new(("page-properties-collapsed", page));
    let mut collapsed = ui
        .ctx()
        .data_mut(|d| {
            d.get_persisted::<bool>(collapse_id)
                .or_else(|| d.get_temp::<bool>(collapse_id))
        })
        .unwrap_or(false);

    let section_hover_id = egui::Id::new(("page-properties-section-hover", page, scope_id));
    let was_section_hovered = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(section_hover_id))
        .unwrap_or(false);

    let db_props_len = record_context
        .as_ref()
        .map(|ctx| ctx.data.properties.len())
        .unwrap_or(0);
    let total_count = properties.len() + db_props_len;

    ui.horizontal(|ui| {
        ui.add_space(indent);
        let section_resp = ui.vertical(|ui| {
            ui.set_max_width(section_width);
            ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
            ui.push_id(("page-properties", page, scope_id), |ui| {
                // Section header bar
                ui.horizontal(|ui| {
                    let arrow = if collapsed { "▸" } else { "▾" };
                    let count_text = if total_count == 0 {
                        format!("{arrow}  PROPERTIES")
                    } else {
                        format!("{arrow}  PROPERTIES  ·  {total_count}")
                    };
                    let header_btn = ui
                        .add(
                            egui::Button::new(
                                RichText::new(count_text)
                                    .font(FontId::proportional(11.0))
                                    .strong()
                                    .color(palette.muted),
                            )
                            .frame(false),
                        )
                        .on_hover_text(if collapsed {
                            "Özellikleri göster / genişlet · Sağ tık: Menü"
                        } else {
                            "Özellikleri gizle / daralt · Sağ tık: Menü"
                        });

                    if header_btn.clicked() {
                        collapsed = !collapsed;
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(collapse_id, collapsed);
                            d.insert_persisted(collapse_id, collapsed);
                        });
                    }

                    header_btn.persistent_context_menu(|ui| {
                        show_section_context_menu(
                            ui,
                            page,
                            scope_id,
                            properties,
                            record_context.as_deref_mut(),
                            &mut collapsed,
                            collapse_id,
                            busy,
                            palette,
                            intents,
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let toggle_btn = ui
                            .add(
                                egui::Button::new(
                                    RichText::new("👁")
                                        .font(FontId::proportional(12.5))
                                        .color(palette.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text(if collapsed {
                                "Özellikleri göster"
                            } else {
                                "Özellikleri gizle"
                            });

                        if toggle_btn.clicked() {
                            collapsed = !collapsed;
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(collapse_id, collapsed);
                                d.insert_persisted(collapse_id, collapsed);
                            });
                        }
                    });
                });

                if collapsed {
                    return;
                }

                ui.add_space(2.0);

                // Created date row
                show_created_row(ui, created_at, section_width, palette);

                // Database property rows (if page is a database record)
                if let Some(ref mut record_ctx) = record_context {
                    let data = record_ctx.data;
                    let record_page = record_ctx.record_page;
                    let mut drafts = crate::ui::database::Drafts {
                        properties: record_ctx.property_drafts,
                        titles: record_ctx.title_drafts,
                        values: record_ctx.value_drafts,
                    };
                    crate::ui::database::show_record_property_rows(
                        ui,
                        data,
                        record_page,
                        section_width,
                        busy,
                        palette,
                        intents,
                        &mut drafts,
                    );
                }

                // Custom property rows
                let total = properties.len();
                for (index, property) in properties.iter().enumerate() {
                    show_property_row(
                        ui,
                        page,
                        scope_id,
                        created_at,
                        property,
                        index,
                        total,
                        section_width,
                        busy,
                        palette,
                        intents,
                    );
                }

                // Bottom quick "+ Add property" button row
                let add_row_id = egui::Id::new(("add-prop-bottom-row", page, scope_id));
                let add_menu_open_id = egui::Id::new(("add-prop-menu-open", page, scope_id));
                let was_hovered = ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(add_row_id))
                    .unwrap_or(false);
                let was_menu_open = ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(add_menu_open_id))
                    .unwrap_or(false);

                let is_visible = was_section_hovered || was_hovered || was_menu_open;
                let is_highlighted = was_hovered || was_menu_open;
                let row_bg = if is_highlighted {
                    blend_color(palette.background, palette.hover, 0.55)
                } else {
                    Color32::TRANSPARENT
                };

                let mut is_now_open = false;
                let row_resp = egui::Frame::new()
                    .fill(row_bg)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.set_min_width(section_width - 12.0);
                        ui.horizontal(|ui| {
                            if is_visible {
                                let text_color = if is_highlighted {
                                    palette.text
                                } else {
                                    palette.muted
                                };
                                let menu_resp = ui.persistent_menu_button(
                                    RichText::new("+  Add property")
                                        .font(FontId::proportional(12.5))
                                        .color(text_color),
                                    |ui| {
                                        if let Some(ref record_ctx) = record_context {
                                            crate::ui::database::add_property_menu(
                                                ui,
                                                record_ctx.data,
                                                intents,
                                            );
                                        } else {
                                            show_add_property_panel(
                                                ui, page, scope_id, properties, busy, palette,
                                                intents,
                                            );
                                        }
                                    },
                                );
                                if menu_resp.inner.is_some() {
                                    is_now_open = true;
                                }
                                let ctx_resp = menu_resp.response.persistent_context_menu(|ui| {
                                    if let Some(ref record_ctx) = record_context {
                                        crate::ui::database::add_property_menu(
                                            ui,
                                            record_ctx.data,
                                            intents,
                                        );
                                    } else {
                                        show_add_property_panel(
                                            ui, page, scope_id, properties, busy, palette, intents,
                                        );
                                    }
                                });
                                if ctx_resp.is_some() {
                                    is_now_open = true;
                                }
                            } else {
                                let dummy = RichText::new("+  Add property")
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::TRANSPARENT);
                                ui.label(dummy);
                            }
                        });
                    });

                let hovered_now = ui.rect_contains_pointer(row_resp.response.rect);
                if hovered_now != was_hovered || is_now_open != was_menu_open {
                    ui.ctx().request_repaint();
                }
                ui.ctx().data_mut(|d| {
                    d.insert_temp(add_row_id, hovered_now);
                    d.insert_temp(add_menu_open_id, is_now_open);
                });
            });
        });

        let section_hit_rect = section_resp.response.rect.expand2(vec2(0.0, 24.0));
        let section_hovered = ui.rect_contains_pointer(section_hit_rect);
        if section_hovered != was_section_hovered {
            ui.ctx().request_repaint();
        }
        ui.ctx().data_mut(|d| {
            d.insert_temp(section_hover_id, section_hovered);
        });
    });
    ui.add_space(6.0);
}

fn show_section_context_menu(
    ui: &mut Ui,
    page: PageId,
    scope_id: u64,
    properties: &[PageProperty],
    record_context: Option<&mut RecordPropertyContext<'_>>,
    collapsed: &mut bool,
    collapse_id: egui::Id,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    ui.set_min_width(230.0);
    ui.label(
        RichText::new("MANAGE PROPERTIES")
            .font(FontId::proportional(10.5))
            .strong()
            .color(palette.muted),
    );
    ui.add_space(4.0);
    let toggle_label = if *collapsed {
        "👁  Özellikleri göster"
    } else {
        "👁  Özellikleri gizle"
    };
    if ui.button(toggle_label).clicked() {
        *collapsed = !*collapsed;
        ui.ctx().data_mut(|d| {
            d.insert_temp(collapse_id, *collapsed);
            d.insert_persisted(collapse_id, *collapsed);
        });
        ui.close_menu();
    }
    ui.separator();
    if let Some(record_ctx) = record_context {
        ui.persistent_menu_button("+  Add new property", |ui| {
            crate::ui::database::add_property_menu(ui, record_ctx.data, intents);
        });
        if !record_ctx.data.properties.is_empty() {
            ui.separator();
            ui.label(
                RichText::new("Existing properties (click to edit)")
                    .small()
                    .color(palette.muted),
            );
            let mut drafts = crate::ui::database::Drafts {
                properties: record_ctx.property_drafts,
                titles: record_ctx.title_drafts,
                values: record_ctx.value_drafts,
            };
            for prop in &record_ctx.data.properties {
                let icon = property_icon(&prop.kind);
                ui.persistent_menu_button(format!("{icon}  {}", prop.name), |ui| {
                    crate::ui::database::show_property_edit_menu(
                        ui,
                        prop,
                        busy,
                        palette,
                        intents,
                        &mut drafts,
                    );
                });
            }
        }
        if !properties.is_empty() {
            let total = properties.len();
            for (idx, prop) in properties.iter().enumerate() {
                let icon = property_icon(&prop.kind);
                ui.persistent_menu_button(format!("{icon}  {}", prop.name), |ui| {
                    show_property_edit_panel(ui, page, prop, idx, total, busy, palette, intents);
                });
            }
        }
    } else {
        ui.persistent_menu_button("+  Add new property", |ui| {
            show_add_property_panel(ui, page, scope_id, properties, busy, palette, intents);
        });
        if !properties.is_empty() {
            ui.separator();
            ui.label(
                RichText::new("Existing properties (click to edit)")
                    .small()
                    .color(palette.muted),
            );
            let total = properties.len();
            for (idx, prop) in properties.iter().enumerate() {
                let icon = property_icon(&prop.kind);
                ui.persistent_menu_button(format!("{icon}  {}", prop.name), |ui| {
                    show_property_edit_panel(ui, page, prop, idx, total, busy, palette, intents);
                });
            }
        }
    }
}

fn show_created_row(ui: &mut Ui, created_at: i64, section_width: f32, palette: Palette) {
    let created_row_id = ui.id().with("created-row-hover");
    let was_hovered = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(created_row_id))
        .unwrap_or(false);
    let bg_fill = if was_hovered {
        blend_color(palette.background, palette.hover, 0.55)
    } else {
        Color32::TRANSPARENT
    };
    let value_width = (section_width - LABEL_COLUMN_WIDTH - 28.0).max(120.0);

    let row_resp = egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(0, 1))
        .show(ui, |ui| {
            ui.set_min_width(section_width - 12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Left container: Created label
                egui::Frame::new()
                    .fill(bg_fill)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(LABEL_COLUMN_WIDTH - 12.0, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new("🕒")
                                        .font(FontId::proportional(12.5))
                                        .color(palette.muted),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("Created")
                                        .font(FontId::proportional(13.0))
                                        .color(palette.muted),
                                );
                            },
                        );
                    });

                // Right container: Created date
                egui::Frame::new()
                    .fill(bg_fill)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(value_width - 12.0, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(crate::ui::database::format_date(created_at))
                                        .font(FontId::proportional(13.0))
                                        .color(palette.muted),
                                );
                            },
                        );
                    });
            });
        });

    let hovered_now = ui.rect_contains_pointer(row_resp.response.rect);
    if hovered_now != was_hovered {
        ui.ctx().request_repaint();
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(created_row_id, hovered_now));
}

#[allow(clippy::too_many_arguments)]
fn show_property_row(
    ui: &mut Ui,
    page: PageId,
    scope_id: u64,
    created_at: i64,
    property: &PageProperty,
    index: usize,
    total: usize,
    section_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let row_id = ui.id().with(("prop-row", page, scope_id, property.id));
    let is_row_hovered = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(row_id))
        .unwrap_or(false);

    let bg_fill = if is_row_hovered {
        blend_color(palette.background, palette.hover, 0.55)
    } else {
        Color32::TRANSPARENT
    };

    let value_width = (section_width - LABEL_COLUMN_WIDTH - 28.0).max(120.0);

    let frame_resp = egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(0, 1))
        .show(ui, |ui| {
            ui.set_min_width(section_width - 12.0);
            ui.push_id(("page-prop-row-scope", property.id), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Left container: Property Header Button
                    egui::Frame::new()
                        .fill(bg_fill)
                        .corner_radius(5)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            show_property_header_button(
                                ui,
                                page,
                                property,
                                index,
                                total,
                                busy,
                                palette,
                                is_row_hovered,
                                intents,
                            );
                        });

                    // Right container: Property Value Editor
                    egui::Frame::new()
                        .fill(bg_fill)
                        .corner_radius(5)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(
                                vec2(value_width - 12.0, ROW_MIN_HEIGHT - 6.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    property_value_editor(
                                        ui,
                                        page,
                                        created_at,
                                        property,
                                        value_width - 12.0,
                                        busy,
                                        palette,
                                        intents,
                                    );
                                },
                            );
                        });
                });
            });
        });

    let hovered_now = ui.rect_contains_pointer(frame_resp.response.rect);
    if hovered_now != is_row_hovered {
        ui.ctx().request_repaint();
    }
    ui.ctx().data_mut(|d| d.insert_temp(row_id, hovered_now));
}

#[allow(clippy::too_many_arguments)]
fn show_property_header_button(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    index: usize,
    total: usize,
    busy: bool,
    palette: Palette,
    row_hovered: bool,
    intents: &mut Vec<Intent>,
) {
    let icon = property_icon(&property.kind);
    let label_text = format!("{icon}   {}", property.name);

    ui.allocate_ui_with_layout(
        vec2(LABEL_COLUMN_WIDTH - 12.0, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.scope(|ui| {
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                ui.spacing_mut().button_padding = vec2(4.0, 3.0);

                let menu_resp = ui.persistent_menu_button(
                    RichText::new(label_text)
                        .font(FontId::proportional(13.0))
                        .color(if row_hovered {
                            palette.text
                        } else {
                            palette.muted
                        }),
                    |ui| {
                        show_property_edit_panel(
                            ui, page, property, index, total, busy, palette, intents,
                        );
                    },
                );
                menu_resp
                    .response
                    .on_hover_text(format!(
                        "{} ({}) · Click or right-click to edit property",
                        property.name,
                        type_label(&property.kind)
                    ))
                    .persistent_context_menu(|ui| {
                        show_property_edit_panel(
                            ui, page, property, index, total, busy, palette, intents,
                        );
                    });
            });
        },
    );
}

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

#[allow(clippy::too_many_arguments)]
fn property_value_editor(
    ui: &mut Ui,
    page: PageId,
    created_at: i64,
    property: &PageProperty,
    available_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    match property.kind.as_str() {
        "checkbox" => {
            let mut checked = property.value == "true";
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Checkbox::without_text(&mut checked))
                    .changed()
                {
                    set_value(intents, page, property.id, checked.to_string());
                }
            });
        }
        "select" | "status" => {
            show_single_option_editor(ui, page, property, busy, palette, intents);
        }
        "multi-select" => {
            show_multi_option_editor(ui, page, property, available_width, busy, palette, intents);
        }
        "date" => {
            show_date_editor(ui, page, property, busy, palette, intents);
        }
        "created_time" | "last_edited_time" => {
            let display = if property.value.trim().is_empty() {
                crate::ui::database::format_date(created_at)
            } else {
                property.value.clone()
            };
            ui.label(
                RichText::new(display)
                    .font(FontId::proportional(13.0))
                    .color(palette.muted),
            );
        }
        _ => {
            show_text_or_number_editor(ui, page, property, available_width, busy, palette, intents);
        }
    }
}

fn show_single_option_editor(
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
            Stroke::new(1.0, pill_border)
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

fn show_multi_option_editor(
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

fn show_date_editor(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    _busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let has_date = !property.value.is_empty();
    let pill_bg = if has_date {
        palette.accent_soft
    } else {
        Color32::TRANSPARENT
    };
    let btn_label = if has_date {
        format!("📅  {}", format_iso_date_pretty(&property.value))
    } else {
        "📅  Empty".to_owned()
    };

    ui.scope(|ui| {
        ui.style_mut().visuals.widgets.inactive.weak_bg_fill = pill_bg;
        ui.style_mut().visuals.widgets.inactive.bg_fill = pill_bg;
        ui.style_mut().visuals.widgets.inactive.bg_stroke = if has_date {
            Stroke::new(1.0, palette.accent.linear_multiply(0.3))
        } else {
            Stroke::NONE
        };
        ui.spacing_mut().button_padding = vec2(8.0, 3.0);

        ui.persistent_menu_button(
            RichText::new(btn_label)
                .font(FontId::proportional(12.5))
                .color(if has_date {
                    palette.text
                } else {
                    palette.muted
                }),
            |ui| {
                if let Some(chosen) = show_date_picker_popup(
                    ui,
                    ("page-prop-cal", page, property.id),
                    &property.value,
                    palette,
                ) {
                    set_value(intents, page, property.id, chosen);
                    ui.close_menu();
                }
            },
        )
        .response
        .on_hover_text("Click to select date from calendar");
    });
}

pub fn show_date_picker_popup(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + Copy,
    current_value: &str,
    palette: Palette,
) -> Option<String> {
    ui.set_min_width(236.0);
    let today = Local::now().date_naive();
    let parsed_current = NaiveDate::parse_from_str(current_value.trim(), "%Y-%m-%d").ok();

    let nav_id = ui.id().with(("cal-nav-ym", id_salt));
    let default_ym = parsed_current
        .map(|d| (d.year(), d.month()))
        .unwrap_or((today.year(), today.month()));
    let (mut year, mut month) = ui
        .ctx()
        .data(|d| d.get_temp::<(i32, u32)>(nav_id))
        .unwrap_or(default_ym);

    let mut chosen: Option<String> = None;

    // Quick preset buttons
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui.small_button("Today").clicked() {
            chosen = Some(today.format("%Y-%m-%d").to_string());
        }
        if ui.small_button("Tomorrow").clicked() {
            if let Some(tom) = today.checked_add_signed(Duration::days(1)) {
                chosen = Some(tom.format("%Y-%m-%d").to_string());
            }
        }
        if ui.small_button("+7d").clicked() {
            if let Some(next_w) = today.checked_add_signed(Duration::days(7)) {
                chosen = Some(next_w.format("%Y-%m-%d").to_string());
            }
        }
        if !current_value.is_empty()
            && ui
                .small_button(RichText::new("Clear").color(palette.danger))
                .clicked()
        {
            chosen = Some(String::new());
        }
    });

    ui.separator();

    // Month / Year Navigation
    ui.horizontal(|ui| {
        if ui
            .small_button("«")
            .on_hover_text("Previous year")
            .clicked()
        {
            year -= 1;
        }
        if ui
            .small_button("‹")
            .on_hover_text("Previous month")
            .clicked()
        {
            if month == 1 {
                month = 12;
                year -= 1;
            } else {
                month -= 1;
            }
        }
        ui.label(
            RichText::new(format!("{} {year}", month_name(month)))
                .font(FontId::proportional(12.5))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("»").on_hover_text("Next year").clicked() {
                year += 1;
            }
            if ui.small_button("›").on_hover_text("Next month").clicked() {
                if month == 12 {
                    month = 1;
                    year += 1;
                } else {
                    month += 1;
                }
            }
        });
    });
    ui.ctx().data_mut(|d| d.insert_temp(nav_id, (year, month)));

    ui.add_space(2.0);

    // Calendar Grid
    egui::Grid::new(("cal-grid", id_salt))
        .num_columns(7)
        .spacing([2.0, 2.0])
        .min_col_width(28.0)
        .show(ui, |ui| {
            for wd in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(wd)
                            .font(FontId::proportional(11.0))
                            .color(palette.muted)
                            .strong(),
                    );
                });
            }
            ui.end_row();

            let first_day = NaiveDate::from_ymd_opt(year, month, 1)
                .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
            let offset = first_day.weekday().num_days_from_monday() as usize;
            let days_count = days_in_month(year, month);

            let mut cell = 0usize;
            for _ in 0..offset {
                ui.label("");
                cell += 1;
            }
            for day in 1..=days_count {
                let candidate = NaiveDate::from_ymd_opt(year, month, day);
                let is_selected = candidate == parsed_current && parsed_current.is_some();
                let is_today = candidate == Some(today);

                let fill = if is_selected {
                    palette.accent
                } else if is_today {
                    palette.accent_soft
                } else {
                    Color32::TRANSPARENT
                };
                let text_col = if is_selected {
                    palette.background
                } else if is_today {
                    palette.accent
                } else {
                    palette.text
                };
                let stroke = if is_today && !is_selected {
                    Stroke::new(1.0, palette.accent)
                } else {
                    Stroke::NONE
                };

                let btn = egui::Button::new(
                    RichText::new(day.to_string())
                        .font(FontId::proportional(11.5))
                        .color(text_col),
                )
                .fill(fill)
                .stroke(stroke)
                .corner_radius(4);

                if ui.add_sized([28.0, 24.0], btn).clicked()
                    && let Some(date) = candidate
                {
                    chosen = Some(date.format("%Y-%m-%d").to_string());
                }

                cell += 1;
                if cell % 7 == 0 {
                    ui.end_row();
                }
            }
        });

    chosen
}

pub(crate) fn layout_property_value(
    ui: &Ui,
    text: &str,
    wrap_width: f32,
    is_number: bool,
    edit_id: egui::Id,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let display = if is_number && !ui.memory(|memory| memory.has_focus(edit_id)) {
        crate::domain::number_format::format_display(text)
    } else {
        text.to_owned()
    };
    let job = egui::text::LayoutJob::simple(display, FontId::proportional(13.0), color, wrap_width);
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn show_text_or_number_editor(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    available_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let value_id = ("property-value", page, property.id);
    let mut value = read_draft(ui, value_id, &property.value);
    if !ui.ctx().memory(|m| m.has_focus(ui.id().with(value_id))) && value != property.value {
        value = property.value.clone();
    }

    let default_id_hint = format!("#{page}");
    let hint: &str = match property.kind.as_str() {
        "number" => "Empty (number)",
        "url" => "https://…",
        "email" => "name@example.com",
        "phone" => "+90…",
        "person" => "Empty (person)",
        "files" => "File path or URL…",
        "place" => "Empty (location)",
        "relation" => "Empty (relation)",
        "rollup" => "Empty (rollup)",
        "formula" => "Empty (formula)",
        "button" => "Button label…",
        "id" => &default_id_hint,
        _ => "Empty",
    };

    let has_url_action = property.kind == "url" && !value.trim().is_empty();
    let edit_w = if has_url_action {
        (available_width - 42.0).max(60.0)
    } else {
        (available_width - 16.0).max(80.0)
    };

    let edit_id = ui.id().with(value_id);
    let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
        layout_property_value(
            ui,
            text,
            wrap_width,
            property.kind == "number",
            edit_id,
            palette.text,
        )
    };
    let response = ui.add_enabled(
        !busy,
        egui::TextEdit::singleline(&mut value)
            .id(edit_id)
            .layouter(&mut layouter)
            .hint_text(hint)
            .font(FontId::proportional(13.0))
            .frame(false)
            .margin(egui::Margin::symmetric(4, 2))
            .desired_width(edit_w),
    );
    let number_submitted = property.kind == "number"
        && (response.has_focus() || response.lost_focus())
        && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if number_submitted && !value.trim().is_empty() {
        if let Some(result) = crate::domain::number_expression::evaluate(&value) {
            value = result;
            ui.ctx().request_repaint();
        } else {
            response.request_focus();
        }
    }
    save_draft(ui, value_id, &value, response.changed() || number_submitted);
    let is_invalid_number = property.kind == "number"
        && !value.trim().is_empty()
        && value.trim().parse::<f64>().map_or(true, |n| !n.is_finite());

    if has_url_action
        && ui
            .small_button(RichText::new("↗").color(palette.accent))
            .on_hover_text("Open URL")
            .clicked()
    {
        let target_url = value.trim();
        let full_url = if target_url.starts_with("http://") || target_url.starts_with("https://") {
            target_url.to_owned()
        } else {
            format!("https://{target_url}")
        };
        intents.push(Intent::OpenUrl(full_url));
    }

    if is_invalid_number {
        ui.label(
            RichText::new("⚠ Invalid number")
                .font(FontId::proportional(11.0))
                .color(palette.danger),
        );
    } else {
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (response.lost_focus() || enter) && value != property.value {
            set_value(intents, page, property.id, value);
        }
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
        let final_name = unique_property_name(existing_properties, name.trim());
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
        let final_name = unique_property_name(existing_properties, base_name);
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

pub fn default_name_for_kind(kind: &str) -> &'static str {
    match kind {
        "text" => "Text",
        "number" => "Number",
        "select" => "Select",
        "multi-select" => "Multi-select",
        "status" => "Status",
        "date" => "Date",
        "person" => "Person",
        "files" => "Files & media",
        "checkbox" => "Checkbox",
        "url" => "URL",
        "phone" => "Phone",
        "email" => "Email",
        "relation" => "Relation",
        "rollup" => "Rollup",
        "formula" => "Formula",
        "button" => "Button",
        "id" => "ID",
        "place" => "Place",
        "created_time" => "Created time",
        "last_edited_time" => "Last edited time",
        "created_by" => "Created by",
        "last_edited_by" => "Last edited by",
        _ => "Property",
    }
}

fn unique_property_name(existing: &[PageProperty], desired: &str) -> String {
    let desired = desired.trim();
    if !existing
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(desired))
    {
        return desired.to_owned();
    }
    let mut counter = 2usize;
    loop {
        let candidate = format!("{desired} {counter}");
        if !existing
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        counter += 1;
    }
}

pub fn format_iso_date_pretty(value: &str) -> String {
    let Ok(date) = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d") else {
        return value.to_owned();
    };
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    if date.year() == today.year() {
        format!("{} {}", date.day(), month_name(date.month()))
    } else {
        format!(
            "{} {} {}",
            date.day(),
            month_name(date.month()),
            date.year()
        )
    }
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 30,
    }
}

fn set_value(intents: &mut Vec<Intent>, page: PageId, property: i64, value: String) {
    intents.push(Intent::Action(Action::SetPagePropertyValue {
        page,
        property,
        value,
    }));
}

fn read_draft(ui: &Ui, key: impl std::hash::Hash, initial: &str) -> String {
    let id = ui.id().with(key);
    ui.ctx()
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| initial.to_owned())
}

fn save_draft(ui: &Ui, key: impl std::hash::Hash, value: &str, changed: bool) {
    if changed {
        let id = ui.id().with(key);
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, value.to_owned()));
    }
}

fn clear_draft(ui: &Ui, key: impl std::hash::Hash) {
    let id = ui.id().with(key);
    ui.ctx().data_mut(|data| data.remove::<String>(id));
}

fn split_options(options: &str) -> Vec<String> {
    options
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod number_input_tests {
    use super::*;

    #[test]
    fn number_display_formats_only_unfocused_number_properties() {
        let context = egui::Context::default();
        let field = egui::Id::new("number-display-test");
        for (is_number, focused, expected) in [
            (true, false, "40.000,5"),
            (true, true, "40000.5"),
            (false, false, "40000.5"),
        ] {
            context.memory_mut(|memory| {
                if focused {
                    memory.request_focus(field);
                } else {
                    memory.surrender_focus(field);
                }
            });
            let _ = context.run(egui::RawInput::default(), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let galley = layout_property_value(
                        ui,
                        "40000.5",
                        250.0,
                        is_number,
                        field,
                        Color32::WHITE,
                    );
                    assert_eq!(galley.text(), expected);
                });
            });
        }
    }

    #[test]
    fn number_property_calculates_on_enter_and_rejects_invalid_results() {
        for (expression, expected) in [
            ("500+20", Some("520")),
            ("2+3*4", Some("14")),
            ("1/0", None),
        ] {
            let context = egui::Context::default();
            let property = PageProperty {
                id: 7,
                name: "Number".into(),
                kind: "number".into(),
                options: Vec::new(),
                value: String::new(),
            };
            let frame = |events| {
                let mut intents = Vec::new();
                let mut field = egui::Id::NULL;
                let _ = context.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            field = ui.id().with(("property-value", 1_i64, property.id));
                            show_text_or_number_editor(
                                ui,
                                1,
                                &property,
                                250.0,
                                false,
                                Palette::new(false),
                                &mut intents,
                            );
                        });
                    },
                );
                (intents, field)
            };
            let (_, field) = frame(Vec::new());
            context.memory_mut(|memory| memory.request_focus(field));
            frame(Vec::new());
            assert!(
                frame(vec![egui::Event::Text(expression.into())])
                    .0
                    .is_empty()
            );
            let (intents, _) = frame(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            let saved: Vec<_> = intents
                .iter()
                .filter_map(|intent| match intent {
                    Intent::Action(Action::SetPagePropertyValue { value, .. }) => {
                        Some(value.as_str())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                saved,
                expected.into_iter().collect::<Vec<_>>(),
                "{expression}"
            );
            let draft = context
                .data(|state| state.get_temp::<String>(field))
                .unwrap();
            assert_eq!(draft, expected.unwrap_or(expression));
            if expected.is_none() {
                assert_eq!(context.memory(|memory| memory.focused()), Some(field));
            }
        }
    }
}
