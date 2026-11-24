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

/// Turns media on the OS clipboard (copied files or raw image data) into import actions.
pub fn take_media_paste(ui: &Ui, page: PageId, position: usize) -> Vec<Action> {
    let requested = ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(event, egui::Event::Paste(_))
                || matches!(
                    event,
                    egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. }
                        if modifiers.command
                )
        })
    });
    if !requested {
        return Vec::new();
    }
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return Vec::new();
    };
    let mut sources: Vec<std::path::PathBuf> = clipboard
        .get()
        .file_list()
        .unwrap_or_default()
        .into_iter()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| BlockKind::from_media_extension(extension).is_some())
        })
        .collect();
    if sources.is_empty() {
        let has_text = ui.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(text) if !text.is_empty()))
        });
        if has_text {
            return Vec::new();
        }
        let Ok(image) = clipboard.get_image() else {
            return Vec::new();
        };
        let Some(buffer) = image::RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        ) else {
            return Vec::new();
        };
        let path = std::env::temp_dir().join(format!(
            "noteon-paste-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        if buffer.save(&path).is_err() {
            return Vec::new();
        }
        sources.push(path);
    }
    ui.input_mut(|input| {
        input
            .events
            .retain(|event| !matches!(event, egui::Event::Paste(_)));
    });
    sources
        .into_iter()
        .enumerate()
        .map(|(offset, source)| Action::ImportMedia {
            page,
            position: position + offset,
            source,
        })
        .collect()
}
