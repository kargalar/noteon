use eframe::egui::{self, Key, Modifiers, Rect, RichText, Ui, vec2};

use crate::domain::editing::split_at_selection;
use crate::domain::model::{Block, BlockKind};
use crate::worker::Action;

use crate::ui::Intent;

use super::text_format::split_text_styles;
use super::{block_widget_id, parse_file_link};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlashCommandTarget {
    Block(BlockKind),
    Page,
    File,
}

const SLASH_COMMANDS: [(&str, SlashCommandTarget); 14] = [
    ("/h1", SlashCommandTarget::Block(BlockKind::Heading1)),
    ("/h2", SlashCommandTarget::Block(BlockKind::Heading2)),
    ("/h3", SlashCommandTarget::Block(BlockKind::Heading3)),
    ("/todo", SlashCommandTarget::Block(BlockKind::Todo)),
    ("/bullet", SlashCommandTarget::Block(BlockKind::Bullet)),
    ("/number", SlashCommandTarget::Block(BlockKind::Numbered)),
    ("/quote", SlashCommandTarget::Block(BlockKind::Quote)),
    ("/callout", SlashCommandTarget::Block(BlockKind::Callout)),
    ("/code", SlashCommandTarget::Block(BlockKind::Code)),
    ("/divider", SlashCommandTarget::Block(BlockKind::Divider)),
    ("/text", SlashCommandTarget::Block(BlockKind::Text)),
    ("/table", SlashCommandTarget::Block(BlockKind::Database)),
    ("/page", SlashCommandTarget::Page),
    ("/file", SlashCommandTarget::File),
];

pub(super) fn show_command_suggestions(
    ui: &mut Ui,
    anchor: Rect,
    block: &Block,
    focused: bool,
    text_changed: bool,
    widget_scope: Option<u64>,
) -> Option<SlashCommandTarget> {
    let matches = matching_commands(&block.content);
    if matches.is_empty() {
        return None;
    }

    let selection_id = block_widget_id("slash-command-selection", block.id, widget_scope);
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(selection_id))
        .unwrap_or(0)
        .min(matches.len() - 1);
    if text_changed {
        selected = 0;
    }

    let mut activate = None;
    if focused {
        ui.input_mut(|input| {
            if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                selected = (selected + 1) % matches.len();
            } else if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                selected = selected.checked_sub(1).unwrap_or(matches.len() - 1);
            } else if input.consume_key(Modifiers::NONE, Key::Enter) {
                activate = Some(matches[selected].1);
            }
        });
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected));

    if activate.is_none() {
        egui::Area::new(block_widget_id(
            "slash-command-menu",
            block.id,
            widget_scope,
        ))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor.left_bottom() + vec2(0.0, 4.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(230.0);
                ui.label(RichText::new("BLOCK TYPE").small().weak());
                for (index, (command, target)) in matches.iter().enumerate() {
                    let label = match target {
                        SlashCommandTarget::Block(BlockKind::Database) => "Table",
                        SlashCommandTarget::Block(kind) => kind.label(),
                        SlashCommandTarget::Page => "Page",
                        SlashCommandTarget::File => "File or Folder",
                    };
                    if ui
                        .selectable_label(index == selected, format!("{command}   {label}"))
                        .clicked()
                    {
                        activate = Some(*target);
                    }
                }
            });
        });
    }

    activate
}

pub(super) fn slash_query(content: &str) -> Option<&str> {
    let query = content.trim_start();
    if !query.starts_with('/') || query.chars().any(char::is_whitespace) {
        return None;
    }
    Some(query)
}

pub(super) fn matching_commands(content: &str) -> Vec<(&'static str, SlashCommandTarget)> {
    let Some(query) = slash_query(content) else {
        return Vec::new();
    };
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|(command, _)| command.starts_with(query))
        .collect()
}

pub(super) fn handle_enter(
    ui: &mut Ui,
    block: &mut Block,
    selection: Option<std::ops::Range<usize>>,
    focused: bool,
    intents: &mut Vec<Intent>,
) -> bool {
    if !focused {
        return false;
    }
    let is_code = block.kind == BlockKind::Code;
    let enter_triggered = if is_code {
        ui.input_mut(|input| {
            input.consume_key(Modifiers::SHIFT, Key::Enter)
                || input.consume_key(Modifiers::CTRL, Key::Enter)
                || input.consume_key(Modifiers::COMMAND, Key::Enter)
        })
    } else {
        consume_plain_enter(ui)
    };
    if !enter_triggered {
        return false;
    }
    if block.kind == BlockKind::Text
        && (block.content.trim_start().starts_with('↗')
            || parse_file_link(&block.content).is_some())
    {
        intents.push(Intent::Action(Action::InsertBlock {
            column_anchor: block.column_group.map(|_| block.id),
            page: block.page_id,
            position: block.position + 1,
            kind: BlockKind::Text,
            content: String::new(),
            text_styles: Vec::new(),
        }));
        return true;
    }
    if let Some(kind) = BlockKind::shortcut(&block.content) {
        block.kind = kind;
        block.content.clear();
        block.text_styles.clear();
        return true;
    }
    let selection = selection.unwrap_or_else(|| {
        let end = block.content.chars().count();
        end..end
    });
    let (prefix, remainder) = split_at_selection(&block.content, selection.clone());
    let (prefix_styles, remainder_styles) = split_text_styles(&block.text_styles, selection);
    block.content = prefix;
    block.text_styles = prefix_styles;
    let next_kind = match block.kind {
        BlockKind::Todo | BlockKind::Bullet | BlockKind::Numbered => block.kind,
        _ => BlockKind::Text,
    };
    // The parent persists the edited prefix before dispatching structural commands.
    intents.push(Intent::Action(Action::InsertBlock {
        column_anchor: block.column_group.map(|_| block.id),
        page: block.page_id,
        position: block.position + 1,
        kind: next_kind,
        content: remainder,
        text_styles: remainder_styles,
    }));
    true
}

fn consume_plain_enter(ui: &mut Ui) -> bool {
    ui.input_mut(|input| {
        let has_enter = input.events.iter().any(|event| match event {
            egui::Event::Key {
                key: Key::Enter,
                pressed: true,
                modifiers,
                ..
            } => modifiers.is_none(),
            _ => false,
        });
        if has_enter {
            input.consume_key(Modifiers::NONE, Key::Enter);
            input.events.retain(|event| !matches!(event,
                egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.is_none()
            ));
            true
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{BlockKind, SlashCommandTarget, matching_commands, slash_query};

    #[test]
    fn slash_command_matches_narrow_as_the_query_grows() {
        let headings = matching_commands("/h");
        assert_eq!(headings.len(), 3);
        assert_eq!(
            matching_commands("/todo"),
            [("/todo", SlashCommandTarget::Block(BlockKind::Todo))]
        );
        assert_eq!(
            matching_commands("/table"),
            [("/table", SlashCommandTarget::Block(BlockKind::Database))]
        );
        assert_eq!(
            matching_commands("/page"),
            [("/page", SlashCommandTarget::Page)]
        );
        assert_eq!(
            matching_commands("/file"),
            [("/file", SlashCommandTarget::File)]
        );
        assert!(matching_commands("/unknown").is_empty());
    }

    #[test]
    fn suggestions_close_after_whitespace_or_without_a_leading_slash() {
        assert_eq!(slash_query(" /bul"), Some("/bul"));
        assert_eq!(slash_query("/bullet "), None);
        assert_eq!(slash_query("text /bullet"), None);
    }
}
