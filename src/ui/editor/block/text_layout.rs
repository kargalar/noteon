use crate::ui::editor::find::FindHighlight;
use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextFormat, Ui};
use std::sync::Arc;

use crate::domain::model::{InlineFontFamily, InlineTextStyle, TextStyleRange};

use super::text_format::style_at;

pub(super) fn layout_styled_text(
    ui: &Ui,
    text: &str,
    wrap_width: f32,
    base_font: FontId,
    base_color: egui::Color32,
    styles: &[TextStyleRange],
    highlight: FindHighlight<'_>,
) -> Arc<egui::Galley> {
    let FindHighlight {
        query: find_query,
        active_match: find_active_match,
    } = highlight;
    let characters: Vec<_> = text.char_indices().collect();
    let detected_links = crate::ui::editor::links::find_links_and_paths(text);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let is_dark = ui.visuals().dark_mode;
    let web_color = if is_dark {
        egui::Color32::from_rgb(88, 166, 255)
    } else {
        egui::Color32::from_rgb(9, 105, 218)
    };
    let file_color = if is_dark {
        egui::Color32::from_rgb(130, 180, 240)
    } else {
        egui::Color32::from_rgb(30, 100, 190)
    };

    let query_clean = find_query.map(|q| q.trim()).unwrap_or("");
    let find_ranges: Vec<(std::ops::Range<usize>, bool)> = if !query_clean.is_empty() {
        crate::ui::editor::find::find_char_ranges(text, query_clean)
            .into_iter()
            .map(|r| {
                let is_active = find_active_match == Some((r.start, r.end));
                (r, is_active)
            })
            .collect()
    } else {
        Vec::new()
    };

    let find_at = |i: usize| -> Option<bool> {
        find_ranges
            .iter()
            .find(|(r, _)| r.contains(&i))
            .map(|(_, active)| *active)
    };

    let mut start = 0;
    while start < characters.len() {
        let style = style_at(styles, start);
        let link_kind = detected_links
            .iter()
            .find(|l| l.char_range.contains(&start))
            .map(|l| &l.link_type);
        let find_state = find_at(start);
        let mut end = start + 1;
        while end < characters.len()
            && style_at(styles, end) == style
            && detected_links
                .iter()
                .find(|l| l.char_range.contains(&end))
                .map(|l| &l.link_type)
                == link_kind
            && find_at(end) == find_state
        {
            end += 1;
        }
        let byte_start = characters[start].0;
        let byte_end = characters.get(end).map_or(text.len(), |(index, _)| *index);

        let font = styled_font(base_font.clone(), &style);
        let mut color = style.foreground.map(color_from_rgba).unwrap_or(base_color);
        let mut background = style.background.map(color_from_rgba);
        let mut underline = style.underline;

        if let Some(link) = link_kind {
            match link {
                crate::ui::editor::links::LinkType::Web { .. } => {
                    if style.foreground.is_none() {
                        color = web_color;
                    }
                    underline = true;
                }
                crate::ui::editor::links::LinkType::FilePath { .. } => {
                    if style.foreground.is_none() {
                        color = file_color;
                    }
                    underline = true;
                }
            }
        }

        if let Some(is_active) = find_state {
            if is_active {
                background = Some(Color32::from_rgb(255, 115, 0));
                color = Color32::WHITE;
            } else {
                background = Some(Color32::from_rgb(250, 204, 21));
                color = Color32::from_rgb(18, 18, 18);
            }
        }

        let mut format = TextFormat::simple(font, color);
        format.italics = style.italic;
        if underline {
            let underline_stroke_color = match link_kind {
                Some(crate::ui::editor::links::LinkType::Web { .. }) => web_color,
                Some(crate::ui::editor::links::LinkType::FilePath { .. }) => file_color,
                _ => color,
            };
            format.underline = Stroke::new(1.1_f32, underline_stroke_color);
        }
        if style.strikethrough {
            format.strikethrough = Stroke::new(1.0_f32, color);
        }
        if let Some(bg) = background {
            format.background = bg;
        }
        job.append(&text[byte_start..byte_end], 0.0, format);
        start = end;
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

pub(super) fn append_text_with_find(
    job: &mut egui::text::LayoutJob,
    text: &str,
    font_id: FontId,
    base_color: Color32,
    underline: Stroke,
    find_query: Option<&str>,
    is_active: bool,
) {
    let query_clean = find_query.map(|q| q.trim()).unwrap_or("");
    let find_ranges = if !query_clean.is_empty() {
        crate::ui::editor::find::find_char_ranges(text, query_clean)
    } else {
        Vec::new()
    };
    if find_ranges.is_empty() {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id,
                color: base_color,
                underline,
                ..Default::default()
            },
        );
        return;
    }
    let characters: Vec<_> = text.char_indices().collect();
    let find_at = |i: usize| -> bool { find_ranges.iter().any(|r| r.contains(&i)) };
    let mut start = 0;
    while start < characters.len() {
        let matched = find_at(start);
        let mut end = start + 1;
        while end < characters.len() && find_at(end) == matched {
            end += 1;
        }
        let byte_start = characters[start].0;
        let byte_end = characters.get(end).map_or(text.len(), |(index, _)| *index);

        let (color, background) = if matched {
            if is_active {
                (Color32::WHITE, Color32::from_rgb(255, 115, 0))
            } else {
                (
                    Color32::from_rgb(18, 18, 18),
                    Color32::from_rgb(250, 204, 21),
                )
            }
        } else {
            (base_color, Color32::TRANSPARENT)
        };

        job.append(
            &text[byte_start..byte_end],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color,
                background,
                underline,
                ..Default::default()
            },
        );
        start = end;
    }
}

pub fn layout_title_with_find(
    ui: &Ui,
    text: &str,
    wrap_width: f32,
    base_font: FontId,
    base_color: egui::Color32,
    find_query: &str,
    active_range: Option<(usize, usize)>,
) -> Arc<egui::Galley> {
    let characters: Vec<_> = text.char_indices().collect();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let query_clean = find_query.trim();
    let find_ranges = if !query_clean.is_empty() {
        crate::ui::editor::find::find_char_ranges(text, query_clean)
    } else {
        Vec::new()
    };

    let find_at = |i: usize| -> bool { find_ranges.iter().any(|r| r.contains(&i)) };
    let active_at = |i: usize| active_range.is_some_and(|(start, end)| (start..end).contains(&i));

    let mut start = 0;
    while start < characters.len() {
        let matched = find_at(start);
        let active = active_at(start);
        let mut end = start + 1;
        while end < characters.len() && find_at(end) == matched && active_at(end) == active {
            end += 1;
        }
        let byte_start = characters[start].0;
        let byte_end = characters.get(end).map_or(text.len(), |(index, _)| *index);

        let font = base_font.clone();
        let mut color = base_color;
        let mut background = None;

        if matched {
            if active {
                background = Some(Color32::from_rgb(255, 115, 0));
                color = Color32::WHITE;
            } else {
                background = Some(Color32::from_rgb(250, 204, 21));
                color = Color32::from_rgb(18, 18, 18);
            }
        }

        let mut format = TextFormat::simple(font, color);
        if let Some(bg) = background {
            format.background = bg;
        }
        job.append(&text[byte_start..byte_end], 0.0, format);
        start = end;
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn styled_font(mut font: FontId, style: &InlineTextStyle) -> FontId {
    font.family = match (style.font_family, style.bold) {
        (Some(InlineFontFamily::Monospace), _) => FontFamily::Monospace,
        (Some(InlineFontFamily::Serif), true) => FontFamily::Name("noteon-serif-bold".into()),
        (Some(InlineFontFamily::Serif), false) => FontFamily::Name("noteon-serif".into()),
        (None, true) => FontFamily::Name("noteon-bold".into()),
        _ => font.family,
    };
    font
}

pub(super) fn color_from_rgba(rgba: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}
