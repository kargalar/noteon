use eframe::egui::Color32;

use crate::ui::palette::Palette;

pub(super) const EMPTY_RECORD_NAME: &str = "Empty";

pub(super) fn table_border_color(palette: Palette) -> Color32 {
    let blend = |border_val: u8, text_val: u8| -> u8 {
        (f32::from(border_val) * 0.82 + f32::from(text_val) * 0.18).round() as u8
    };
    Color32::from_rgb(
        blend(palette.border.r(), palette.muted.r()),
        blend(palette.border.g(), palette.muted.g()),
        blend(palette.border.b(), palette.muted.b()),
    )
}

pub(crate) fn format_date(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "—".to_owned();
    }
    let Some(date_time) =
        chrono::DateTime::from_timestamp(timestamp, 0).map(|utc| utc.with_timezone(&chrono::Local))
    else {
        return "—".to_owned();
    };
    format_date_at(date_time, chrono::Local::now().date_naive())
}

fn format_date_at(date_time: chrono::DateTime<chrono::Local>, today: chrono::NaiveDate) -> String {
    use chrono::{Datelike, Timelike};

    let month = match date_time.month() {
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
        _ => unreachable!(),
    };

    if date_time.date_naive() == today {
        format!(
            "{} {month}, {:02}:{:02}",
            date_time.day(),
            date_time.hour(),
            date_time.minute()
        )
    } else if date_time.year() == today.year() {
        format!("{} {month}", date_time.day())
    } else {
        format!("{} {month} {}", date_time.day(), date_time.year())
    }
}

#[cfg(test)]
mod date_format_tests {
    use super::{format_date, format_date_at};
    use chrono::{Local, NaiveDate, TimeZone};

    #[test]
    fn zero_or_negative_timestamp_returns_dash() {
        assert_eq!(format_date(0), "—");
        assert_eq!(format_date(-1), "—");
    }

    #[test]
    fn date_format_hides_year_for_this_year_and_time_for_other_days() {
        let today = NaiveDate::from_ymd_opt(2025, 10, 15).unwrap();
        let yesterday = Local.with_ymd_and_hms(2025, 10, 14, 17, 42, 31).unwrap();
        let previous_year = Local.with_ymd_and_hms(2024, 10, 15, 17, 42, 31).unwrap();

        assert_eq!(format_date_at(yesterday, today), "14 October");
        assert_eq!(format_date_at(previous_year, today), "15 October 2024");
    }

    #[test]
    fn date_format_shows_time_only_for_today() {
        let today = NaiveDate::from_ymd_opt(2025, 10, 15).unwrap();
        let same_day = Local.with_ymd_and_hms(2025, 10, 15, 9, 7, 53).unwrap();

        assert_eq!(format_date_at(same_day, today), "15 October, 09:07");
    }
}
