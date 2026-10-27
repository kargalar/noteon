use crate::domain::model::{Block, BlockId, BlockKind, PageId};
use crate::worker::Action;
use eframe::egui::{self, Context, Id, Ui};

#[derive(Clone)]
struct BlockClipboard {
    text: String,
    blocks: Vec<Block>,
}

fn clipboard_id() -> Id {
    Id::new("block-clipboard")
}

pub fn copy(context: &Context, blocks: Vec<Block>) {
    if blocks.is_empty() {
        return;
    }
    let text = blocks
        .iter()
        .map(|block| match block.kind {
            BlockKind::Database | BlockKind::PageLink => {
                format!("[{}](noteon://page/{})", block.kind.label(), block.content)
            }
            _ => block.content.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    context.copy_text(text.clone());
    crate::ui::page_transfer::clear_clipboard(context);
    context.data_mut(|data| data.insert_temp(clipboard_id(), BlockClipboard { text, blocks }));
}

pub(super) fn clear(context: &Context) {
    context.data_mut(|data| data.remove::<BlockClipboard>(clipboard_id()));
}

pub fn take_paste(
    ui: &Ui,
    page: PageId,
    position: usize,
    replace_block: Option<BlockId>,
) -> Option<Action> {
    let clipboard = ui
        .ctx()
        .data(|data| data.get_temp::<BlockClipboard>(clipboard_id()))?;
    ui.input_mut(|input| {
        let index = input.events.iter().position(
            |event| matches!(event, egui::Event::Paste(text) if text == &clipboard.text),
        )?;
        input.events.remove(index);
        Some(())
    })?;
    Some(Action::PasteBlocks {
        page,
        position,
        replace_block,
        blocks: clipboard.blocks,
    })
}
