use crate::ui::menus::PersistentMenuUi;
use eframe::egui::{self, Rect, RichText, Ui, pos2};

use crate::domain::model::{Block, InlineFontFamily, InlineTextStyle, TextStyleRange};

use crate::ui::palette::Palette;

use super::block_widget_id;
use super::text_layout::color_from_rgba;

pub(super) fn show_text_format_toolbar(
    ui: &Ui,
    anchor: Rect,
    range: std::ops::Range<usize>,
    block: &mut Block,
    palette: Palette,
    widget_scope: Option<u64>,
) -> bool {
    let viewport = ui.ctx().screen_rect();
    let toolbar_height = 38.0;
    let top = if anchor.top() - toolbar_height >= viewport.top() {
        anchor.top() - toolbar_height
    } else {
        anchor.bottom() + 4.0
    };
    let position = pos2(anchor.left(), top).max(viewport.min);
    let mut changed = false;
    egui::Area::new(block_widget_id(
        "text-format-toolbar",
        block.id,
        widget_scope,
    ))
    .order(egui::Order::Foreground)
    .fixed_pos(position)
    .show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, description, field) in [
                    ("B", "Bold", StyleToggle::Bold),
                    ("I", "Italic", StyleToggle::Italic),
                    ("U", "Underline", StyleToggle::Underline),
                    ("S", "Strikethrough", StyleToggle::Strikethrough),
                ] {
                    let active = range_has_toggle(&block.text_styles, range.clone(), field);
                    if ui
                        .selectable_label(active, label)
                        .on_hover_text(description)
                        .clicked()
                    {
                        toggle_range_style(block, range.clone(), field);
                        changed = true;
                    }
                }
                ui.persistent_menu_button("Font", |ui| {
                    for (label, family) in [
                        ("Default", None),
                        ("Serif", Some(InlineFontFamily::Serif)),
                        ("Monospace", Some(InlineFontFamily::Monospace)),
                    ] {
                        if ui
                            .selectable_label(
                                range_has_font(&block.text_styles, range.clone(), family),
                                label,
                            )
                            .clicked()
                        {
                            apply_range_style(block, range.clone(), |style| {
                                style.font_family = family;
                            });
                            changed = true;
                            ui.close_menu();
                        }
                    }
                });
                ui.persistent_menu_button(RichText::new("A").strong(), |ui| {
                    ui.label("Text color");
                    let mut color = range_color(&block.text_styles, range.clone(), true)
                        .unwrap_or(palette.text);
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        apply_range_style(block, range.clone(), |style| {
                            style.foreground = Some(color.to_array());
                        });
                        changed = true;
                    }
                    if ui.button("Reset text color").clicked() {
                        apply_range_style(block, range.clone(), |style| {
                            style.foreground = None;
                        });
                        changed = true;
                        ui.close_menu();
                    }
                });
                ui.persistent_menu_button("Highlight", |ui| {
                    ui.label("Background color");
                    let mut color = range_color(&block.text_styles, range.clone(), false)
                        .unwrap_or(egui::Color32::from_rgb(255, 230, 120));
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        apply_range_style(block, range.clone(), |style| {
                            style.background = Some(color.to_array());
                        });
                        changed = true;
                    }
                    if ui.button("Remove highlight").clicked() {
                        apply_range_style(block, range.clone(), |style| {
                            style.background = None;
                        });
                        changed = true;
                        ui.close_menu();
                    }
                });
            });
        });
    });
    changed
}

#[derive(Clone, Copy)]
enum StyleToggle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
}

fn range_has_toggle(
    styles: &[TextStyleRange],
    range: std::ops::Range<usize>,
    field: StyleToggle,
) -> bool {
    let characters = expand_text_styles(styles, range.end);
    let selected = &characters[range.start.min(characters.len())..range.end.min(characters.len())];
    !selected.is_empty()
        && selected.iter().all(|style| match field {
            StyleToggle::Bold => style.bold,
            StyleToggle::Italic => style.italic,
            StyleToggle::Underline => style.underline,
            StyleToggle::Strikethrough => style.strikethrough,
        })
}

fn range_has_font(
    styles: &[TextStyleRange],
    range: std::ops::Range<usize>,
    font_family: Option<InlineFontFamily>,
) -> bool {
    let characters = expand_text_styles(styles, range.end);
    let selected = &characters[range.start.min(characters.len())..range.end.min(characters.len())];
    !selected.is_empty()
        && selected
            .iter()
            .all(|style| style.font_family == font_family)
}

fn range_color(
    styles: &[TextStyleRange],
    range: std::ops::Range<usize>,
    foreground: bool,
) -> Option<egui::Color32> {
    let characters = expand_text_styles(styles, range.end);
    let selected = &characters[range.start.min(characters.len())..range.end.min(characters.len())];
    let first = selected.first()?;
    let rgba = if foreground {
        first.foreground
    } else {
        first.background
    }?;
    selected
        .iter()
        .all(|style| {
            (if foreground {
                style.foreground
            } else {
                style.background
            }) == Some(rgba)
        })
        .then(|| color_from_rgba(rgba))
}

fn toggle_range_style(block: &mut Block, range: std::ops::Range<usize>, field: StyleToggle) {
    let enabled = !range_has_toggle(&block.text_styles, range.clone(), field);
    apply_range_style(block, range, |style| match field {
        StyleToggle::Bold => style.bold = enabled,
        StyleToggle::Italic => style.italic = enabled,
        StyleToggle::Underline => style.underline = enabled,
        StyleToggle::Strikethrough => style.strikethrough = enabled,
    });
}

fn apply_range_style(
    block: &mut Block,
    range: std::ops::Range<usize>,
    mut apply: impl FnMut(&mut InlineTextStyle),
) {
    let mut characters = expand_text_styles(&block.text_styles, block.content.chars().count());
    let start = range.start.min(characters.len());
    let end = range.end.min(characters.len());
    for style in &mut characters[start..end] {
        apply(style);
    }
    block.text_styles = compress_text_styles(&characters);
}

pub(super) fn adjust_text_styles(old: &str, new: &str, styles: &mut Vec<TextStyleRange>) {
    let old_chars: Vec<_> = old.chars().collect();
    let new_chars: Vec<_> = new.chars().collect();
    let previous_styles = expand_text_styles(styles, old_chars.len());
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old_chars[prefix..]
        .iter()
        .rev()
        .zip(new_chars[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let inserted = new_chars.len().saturating_sub(prefix + suffix);
    let mut next_styles = previous_styles[..prefix].to_vec();
    let inherited = previous_styles
        .get(prefix.saturating_sub(1))
        .or_else(|| previous_styles.get(prefix))
        .cloned()
        .unwrap_or_default();
    next_styles.extend((0..inserted).map(|_| inherited.clone()));
    next_styles.extend_from_slice(&previous_styles[old_chars.len().saturating_sub(suffix)..]);
    styles.clone_from(&compress_text_styles(&next_styles));
}

pub(super) fn split_text_styles(
    styles: &[TextStyleRange],
    selection: std::ops::Range<usize>,
) -> (Vec<TextStyleRange>, Vec<TextStyleRange>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for range in styles {
        let left_end = range.end.min(selection.start);
        if range.start < left_end {
            left.push(TextStyleRange {
                start: range.start,
                end: left_end,
                style: range.style.clone(),
            });
        }
        let right_start = range.start.max(selection.end);
        if right_start < range.end {
            right.push(TextStyleRange {
                start: right_start - selection.end,
                end: range.end - selection.end,
                style: range.style.clone(),
            });
        }
    }
    (left, right)
}

fn expand_text_styles(styles: &[TextStyleRange], length: usize) -> Vec<InlineTextStyle> {
    let mut expanded = vec![InlineTextStyle::default(); length];
    for range in styles {
        let start = range.start.min(length);
        let end = range.end.min(length);
        if start >= end {
            continue;
        }
        for style in &mut expanded[start..end] {
            style.clone_from(&range.style);
        }
    }
    expanded
}

fn compress_text_styles(styles: &[InlineTextStyle]) -> Vec<TextStyleRange> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < styles.len() {
        let style = &styles[start];
        if style == &InlineTextStyle::default() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < styles.len() && styles[end] == *style {
            end += 1;
        }
        ranges.push(TextStyleRange {
            start,
            end,
            style: style.clone(),
        });
        start = end;
    }
    ranges
}

pub(super) fn style_at(styles: &[TextStyleRange], character_index: usize) -> InlineTextStyle {
    styles
        .iter()
        .find(|range| range.start <= character_index && character_index < range.end)
        .map_or_else(InlineTextStyle::default, |range| range.style.clone())
}
