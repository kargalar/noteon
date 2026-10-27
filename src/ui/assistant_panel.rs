use eframe::egui::{self, RichText, ScrollArea, TextEdit};

use crate::assistant::{Assistant, ChatMessage, EFFORT_LEVELS, Role};
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
            small_label(ui, "Model (empty = Codex default)", palette.muted);
            let model = ui.add(
                TextEdit::singleline(&mut assistant.settings.model)
                    .hint_text("e.g. gpt-5-codex")
                    .desired_width(f32::INFINITY),
            );
            small_label(ui, "Reasoning effort", palette.muted);
            let mut effort_changed = false;
            ui.horizontal(|ui| {
                for level in EFFORT_LEVELS {
                    let label = if level.is_empty() { "default" } else { level };
                    let selected = assistant.settings.effort == level;
                    if ui.selectable_label(selected, label).clicked() {
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
            ui.add(egui::Label::new(RichText::new(text).color(palette.text)).selectable(true));
        });
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
        small_label(ui, "Uses your Codex plan", palette.muted);
    });
}
