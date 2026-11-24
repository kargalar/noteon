use crate::ui::menus::PersistentMenuUi;
use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Ui, vec2};

use crate::domain::model::{PageId, PageProperty};
use crate::ui::{Intent, palette::Palette};

use super::set_value;

pub(super) fn show_date_editor(
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
            Stroke::new(1.0_f32, palette.accent.linear_multiply(0.3))
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
        if ui.small_button("Tomorrow").clicked()
            && let Some(tom) = today.checked_add_signed(Duration::days(1))
        {
            chosen = Some(tom.format("%Y-%m-%d").to_string());
        }
        if ui.small_button("+7d").clicked()
            && let Some(next_w) = today.checked_add_signed(Duration::days(7))
        {
            chosen = Some(next_w.format("%Y-%m-%d").to_string());
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
                    Stroke::new(1.0_f32, palette.accent)
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
                if cell.is_multiple_of(7) {
                    ui.end_row();
                }
            }
        });

    chosen
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
