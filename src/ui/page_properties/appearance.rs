use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::ui::palette::Palette;

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
    let stroke = Stroke::new(1.35_f32, color);
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
            let check_stroke = Stroke::new(1.6_f32, bg_color);
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
                Stroke::new(2.0_f32, color),
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
                Stroke::new(2.0_f32, color),
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
                    ui.painter().circle_stroke(
                        rect.center(),
                        10.0,
                        Stroke::new(2.0_f32, palette.text),
                    );
                } else if resp.hovered() {
                    ui.painter().circle_stroke(
                        rect.center(),
                        10.0,
                        Stroke::new(1.5_f32, palette.accent),
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

pub(super) fn blend_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}
