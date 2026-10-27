use eframe::egui::{self, FontFamily, FontId, RichText, ScrollArea, Stroke, TextEdit, TextFormat};

use crate::assistant::{Assistant, ChatMessage, EFFORT_LEVELS, Role};
use crate::domain::markdown;
use crate::domain::model::{BlockKind, InlineFontFamily, InlineTextStyle, TextStyleRange};
use crate::ui::palette::Palette;
use crate::ui::{quiet_button, small_label};

const INPUT_ID: &str = "assistant-input";
const SUGGESTIONS: [&str; 4] = [
    "Summarize this page",
    "Create a table on this page for my tasks",
    "Turn the notes on this page into a to-do list",
    "Find notes about…",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssistantAction {
    Send,
    Stop,
    Undo,
    Clear,
    Close,
    ToggleSettings,
    SettingsChanged,
}

pub fn show(
    ui: &mut egui::Ui,
    assistant: &mut Assistant,
    palette: Palette,
) -> Vec<AssistantAction> {
    let mut actions = Vec::new();
    show_header(ui, assistant, palette, &mut actions);
    if assistant.settings_open {
        show_settings(ui, assistant, palette, &mut actions);
    }
    ui.separator();
    let input_height = 96.0;
    let history_height = (ui.available_height() - input_height).max(80.0);
    ScrollArea::vertical()
        .max_height(history_height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| show_history(ui, assistant, palette));
    show_input(ui, assistant, palette, &mut actions);
    actions
}

fn show_header(
    ui: &mut egui::Ui,
    assistant: &Assistant,
    palette: Palette,
    actions: &mut Vec<AssistantAction>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("✦ Assistant").strong().color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if quiet_button(ui, "×", palette.muted)
                .on_hover_text("Close (Ctrl+J)")
                .clicked()
            {
                actions.push(AssistantAction::Close);
            }
            if quiet_button(ui, "⚙", palette.muted)
                .on_hover_text("Model settings")
                .clicked()
            {
                actions.push(AssistantAction::ToggleSettings);
            }
            if quiet_button(ui, "🗑", palette.muted)
                .on_hover_text("Clear chat")
                .clicked()
            {
                actions.push(AssistantAction::Clear);
            }
            let undo = ui.add_enabled(
                assistant.can_undo(),
                egui::Button::new(RichText::new("↶ Undo AI").color(palette.text)).frame(false),
            );
            if undo
                .on_hover_text("Restore the workspace to just before the last AI change")
                .clicked()
            {
                actions.push(AssistantAction::Undo);
            }
        });
    });
}

fn show_settings(
    ui: &mut egui::Ui,
    assistant: &mut Assistant,
    palette: Palette,
    actions: &mut Vec<AssistantAction>,
) {
    egui::Frame::new()
        .fill(palette.hover)
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            small_label(ui, "Model", palette.muted);
            let model = ui.add(
                TextEdit::singleline(&mut assistant.settings.model)
                    .hint_text("gpt-6-luna")
                    .desired_width(f32::INFINITY),
            );
            small_label(ui, "Reasoning effort", palette.muted);
            let mut effort_changed = false;
            ui.horizontal(|ui| {
                for level in EFFORT_LEVELS {
                    let selected = assistant.settings.effort == level;
                    if ui.selectable_label(selected, level).clicked() {
                        assistant.settings.effort = level.to_string();
                        effort_changed = true;
                    }
                }
            });
            if model.lost_focus() || effort_changed {
                actions.push(AssistantAction::SettingsChanged);
            }
        });
}

fn show_history(ui: &mut egui::Ui, assistant: &mut Assistant, palette: Palette) {
    ui.add_space(6.0);
    if assistant.messages.is_empty() {
        show_welcome(ui, assistant, palette);
    }
    for message in &assistant.messages {
        show_message(ui, message, palette);
    }
    if assistant.is_running() {
        ui.horizontal(|ui| {
            ui.spinner();
            small_label(ui, "Working…", palette.muted);
        });
    }
}

fn show_welcome(ui: &mut egui::Ui, assistant: &mut Assistant, palette: Palette) {
    ui.label(
        RichText::new("Ask me to read, write, organize or restructure your notes and tables.")
            .color(palette.muted),
    );
    ui.add_space(6.0);
    for suggestion in SUGGESTIONS {
        if ui.button(suggestion).clicked() {
            assistant.input = suggestion.to_string();
            assistant.focus_input = true;
        }
    }
}

fn show_message(ui: &mut egui::Ui, message: &ChatMessage, palette: Palette) {
    match message.role {
        Role::Activity => {
            small_label(ui, &format!("• {}", message.text), palette.muted);
        }
        Role::Error => {
            ui.label(RichText::new(&message.text).color(palette.danger));
        }
        Role::User => show_bubble(ui, &message.text, palette.accent_soft, palette),
        Role::Assistant => show_bubble(ui, &message.text, palette.sidebar, palette),
    }
    ui.add_space(4.0);
}

fn show_bubble(ui: &mut egui::Ui, text: &str, fill: egui::Color32, palette: Palette) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            show_markdown(ui, text, palette);
        });
}

fn show_markdown(ui: &mut egui::Ui, source: &str, palette: Palette) {
    let blocks = markdown::parse_styled_blocks(source);
    let mut numbered_index = 0usize;
    for (index, (kind, content, checked, styles)) in blocks.iter().enumerate() {
        if index > 0 {
            ui.add_space(4.0);
        }
        if *kind == BlockKind::Numbered {
            numbered_index += 1;
        } else {
            numbered_index = 0;
        }
        show_markdown_block(
            ui,
            *kind,
            content,
            *checked,
            numbered_index,
            styles,
            palette,
        );
    }
}

fn show_markdown_block(
    ui: &mut egui::Ui,
    kind: BlockKind,
    content: &str,
    checked: bool,
    numbered_index: usize,
    styles: &[TextStyleRange],
    palette: Palette,
) {
    match kind {
        BlockKind::Divider => {
            ui.separator();
        }
        BlockKind::Code => {
            egui::Frame::new()
                .fill(palette.code)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            RichText::new(content)
                                .font(FontId::monospace(12.5))
                                .color(palette.text),
                        )
                        .selectable(true),
                    );
                });
        }
        BlockKind::Quote | BlockKind::Callout => {
            egui::Frame::new()
                .fill(palette.hover)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .stroke(Stroke::new(1.0, palette.border))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    show_styled_line(ui, content, styles, 13.5, true, palette);
                });
        }
        BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo => {
            let marker = match kind {
                BlockKind::Bullet => "•".to_string(),
                BlockKind::Numbered => format!("{numbered_index}."),
                BlockKind::Todo if checked => "☑".to_string(),
                _ => "☐".to_string(),
            };
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(marker).color(palette.muted));
                ui.vertical(|ui| {
                    show_styled_line(ui, content, styles, 13.5, false, palette);
                });
            });
        }
        BlockKind::Heading1 | BlockKind::Heading2 | BlockKind::Heading3 => {
            let size = match kind {
                BlockKind::Heading1 => 17.5,
                BlockKind::Heading2 => 15.5,
                _ => 14.2,
            };
            let bold_all = [TextStyleRange {
                start: 0,
                end: content.chars().count(),
                style: InlineTextStyle {
                    bold: true,
                    ..Default::default()
                },
            }];
            show_styled_line(ui, content, &bold_all, size, false, palette);
        }
        _ => {
            show_styled_line(ui, content, styles, 13.5, false, palette);
        }
    }
}

fn show_styled_line(
    ui: &mut egui::Ui,
    text: &str,
    styles: &[TextStyleRange],
    font_size: f32,
    force_italic: bool,
    palette: Palette,
) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let style_at = |char_idx: usize| -> InlineTextStyle {
        let mut merged = InlineTextStyle {
            italic: force_italic,
            ..Default::default()
        };
        for range in styles {
            if range.start <= char_idx && char_idx < range.end {
                merged.bold |= range.style.bold;
                merged.italic |= range.style.italic;
                merged.strikethrough |= range.style.strikethrough;
                merged.font_family = range.style.font_family.or(merged.font_family);
            }
        }
        merged
    };
    let mut start = 0usize;
    while start < chars.len() {
        let style = style_at(start);
        let mut end = start + 1;
        while end < chars.len() && style_at(end) == style {
            end += 1;
        }
        let byte_start = chars[start].0;
        let byte_end = chars.get(end).map_or(text.len(), |(idx, _)| *idx);
        job.append(
            &text[byte_start..byte_end],
            0.0,
            text_format_for(&style, font_size, palette),
        );
        start = end;
    }
    ui.add(egui::Label::new(job).selectable(true));
}

fn text_format_for(style: &InlineTextStyle, font_size: f32, palette: Palette) -> TextFormat {
    let is_code = style.font_family == Some(InlineFontFamily::Monospace);
    let family = if is_code {
        FontFamily::Monospace
    } else if style.bold {
        FontFamily::Name("noteon-bold".into())
    } else {
        FontFamily::Proportional
    };
    let size = if is_code { font_size - 1.0 } else { font_size };
    let mut format = TextFormat::simple(FontId::new(size, family), palette.text);
    format.italics = style.italic;
    if style.strikethrough {
        format.strikethrough = Stroke::new(1.0, palette.muted);
    }
    if is_code {
        format.background = palette.code;
    }
    format
}

fn show_input(
    ui: &mut egui::Ui,
    assistant: &mut Assistant,
    palette: Palette,
    actions: &mut Vec<AssistantAction>,
) {
    let id = egui::Id::new(INPUT_ID);
    let has_focus = ui.memory(|memory| memory.has_focus(id));
    let submit = has_focus
        && !assistant.is_running()
        && ui.input_mut(|input| {
            !input.modifiers.shift && input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
        });
    let response = ui.add(
        TextEdit::multiline(&mut assistant.input)
            .id(id)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("Ask about your notes…  (Enter to send, Shift+Enter for a new line)"),
    );
    if std::mem::take(&mut assistant.focus_input) {
        response.request_focus();
    }
    ui.horizontal(|ui| {
        if assistant.is_running() {
            if ui.button("■ Stop").clicked() {
                actions.push(AssistantAction::Stop);
            }
        } else if ui
            .add_enabled(
                !assistant.input.trim().is_empty(),
                egui::Button::new(RichText::new("Send").color(palette.text)),
            )
            .clicked()
            || submit
        {
            actions.push(AssistantAction::Send);
        }
        let status = assistant.quota.as_ref().map_or_else(
            || "Uses your Codex plan".to_string(),
            |quota| quota.compact_label(),
        );
        let color = if assistant
            .quota
            .as_ref()
            .is_some_and(|quota| quota.remaining_percent <= 10.0)
        {
            palette.danger
        } else {
            palette.muted
        };
        small_label(ui, &status, color);
    });
}
