use eframe::egui::{self, Ui};

use crate::domain::editing::split_at_selection;
use crate::domain::model::{Block, BlockKind};
use crate::worker::Action;

use crate::ui::Intent;

use super::text_format::split_text_styles;

pub(super) fn handle_paste(
    ui: &mut Ui,
    block: &mut Block,
    editor_id: egui::Id,
    intents: &mut Vec<Intent>,
) -> bool {
    let position = block.position + usize::from(!block.content.is_empty());
    let replace_block =
        (block.kind == BlockKind::Text && block.content.is_empty()).then_some(block.id);
    if let Some(action) = crate::ui::block_clipboard::take_paste(
        ui,
        block.page_id,
        position,
        replace_block,
    )
    .or_else(|| {
        crate::ui::page_transfer::take_paste(ui, block.page_id, Some(position), replace_block)
    }) {
        intents.push(Intent::Action(action));
        return true;
    }
    let media = crate::ui::block_clipboard::take_media_paste(
        ui,
        block.page_id,
        position,
        block.column_group.map(|_| block.id),
    );
    if !media.is_empty() {
        intents.extend(media.into_iter().map(Intent::Action));
        return false;
    }
    if block.kind == BlockKind::Code {
        return false;
    }
    let pasted = ui.input_mut(|input| {
        let index = input.events.iter().position(
            |event| matches!(event, egui::Event::Paste(text) if text.contains(['\n', '\r'])),
        )?;
        match input.events.remove(index) {
            egui::Event::Paste(text) => Some(text),
            _ => None,
        }
    });
    let Some(pasted) = pasted else {
        return false;
    };
    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), editor_id).unwrap_or_default();
    let selection = state.cursor.char_range().map_or_else(
        || {
            let end = block.content.chars().count();
            end..end
        },
        |range| {
            range.primary.index.min(range.secondary.index)
                ..range.primary.index.max(range.secondary.index)
        },
    );
    let (prefix, suffix) = split_at_selection(&block.content, selection.clone());
    let (prefix_styles, suffix_styles) = split_text_styles(&block.text_styles, selection);
    let mut pasted_blocks = crate::domain::markdown::parse_blocks(&pasted).into_iter();
    let (kind, content, checked) = pasted_blocks.next().expect("parser always returns a block");
    if prefix.is_empty() {
        block.kind = kind;
        block.checked = checked;
    }
    block.content = prefix + &content;
    block.text_styles = prefix_styles;
    let mut remaining: Vec<crate::domain::model::StyledBlockContent> = pasted_blocks
        .map(|(kind, content, checked)| (kind, content, checked, Vec::new()))
        .collect();
    let (last_content, last_styles) = remaining.last_mut().map_or(
        (&mut block.content, &mut block.text_styles),
        |(_, content, _, styles)| (content, styles),
    );
    let cursor = last_content.chars().count();
    last_content.push_str(&suffix);
    last_styles.extend(suffix_styles.into_iter().map(|mut range| {
        range.start += cursor;
        range.end += cursor;
        range
    }));
    if remaining.is_empty() {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(cursor),
            )));
        state.store(ui.ctx(), editor_id);
    } else {
        intents.push(Intent::Action(Action::InsertBlocks {
            column_anchor: block.column_group.map(|_| block.id),
            page: block.page_id,
            position: block.position + 1,
            blocks: remaining,
        }));
    }
    ui.ctx().request_repaint();
    true
}
