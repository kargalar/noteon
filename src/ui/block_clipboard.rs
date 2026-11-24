use crate::domain::model::{Block, BlockId, BlockKind, PageId};
use crate::worker::{Action, MediaSource};
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

/// Turns media on the OS clipboard (copied files or raw image data) into import actions.
pub fn take_media_paste(
    ui: &Ui,
    page: PageId,
    position: usize,
    column_anchor: Option<BlockId>,
) -> Vec<Action> {
    let (paste, release) = ui.input(|input| {
        (
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_)) || paste_key(event, true)),
            input.events.iter().any(|event| paste_key(event, false)),
        )
    });
    if !ui.is_enabled() || (!paste && !release) {
        return Vec::new();
    }
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return Vec::new();
    };
    // egui-winit swallows the key press and emits Paste only for nonempty text.
    // A release-only event can therefore recover media-only pastes without duplicating text pastes.
    if !paste && clipboard.get_text().is_ok_and(|text| !text.is_empty()) {
        return Vec::new();
    }
    let mut sources: Vec<MediaSource> = clipboard
        .get()
        .file_list()
        .unwrap_or_default()
        .into_iter()
        .filter(|path| BlockKind::from_media_path(path).is_some())
        .map(MediaSource::File)
        .collect();
    if sources.is_empty() {
        let Ok(image) = clipboard.get_image() else {
            return Vec::new();
        };
        let (Ok(width), Ok(height)) = (u32::try_from(image.width), u32::try_from(image.height))
        else {
            return Vec::new();
        };
        let Some(buffer) = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
        else {
            return Vec::new();
        };
        sources.push(MediaSource::Image(buffer));
    }
    ui.input_mut(|input| {
        input.events.retain(|event| {
            !matches!(event, egui::Event::Paste(_))
                && !paste_key(event, true)
                && !paste_key(event, false)
        });
    });
    sources
        .into_iter()
        .enumerate()
        .map(|(offset, source)| Action::ImportMedia {
            page,
            position: position + offset,
            column_anchor,
            source,
        })
        .collect()
}

fn paste_key(event: &egui::Event, pressed: bool) -> bool {
    matches!(
        event,
        egui::Event::Key { key, physical_key, pressed: state, modifiers, .. }
            if (*key == egui::Key::V || *physical_key == Some(egui::Key::V))
                && *state == pressed && modifiers.command && !modifiers.alt && !modifiers.shift
    )
}
