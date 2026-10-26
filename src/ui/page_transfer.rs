use crate::domain::model::PageId;
use crate::worker::Action;
use eframe::egui::{self, Context, Id, Key, Modifiers, Ui};

#[derive(Clone, Debug)]
pub struct PageDragPayload {
    pub pages: Vec<PageId>,
}

#[derive(Clone)]
struct PageClipboard {
    text: String,
    pages: Vec<PageId>,
    cut: bool,
}

fn clipboard_id() -> Id {
    Id::new("page-clipboard")
}

pub fn copy_pages(context: &Context, pages: Vec<PageId>, titles: &[String], cut: bool) {
    if pages.is_empty() {
        return;
    }
    // Keep the OS clipboard readable, and accept rich paste only for our own payload.
    let text = pages
        .iter()
        .zip(titles)
        .map(|(id, title)| format!("[{title}](noteon://page/{id})"))
        .collect::<Vec<_>>()
        .join("\n");
    context.copy_text(text.clone());
    context.data_mut(|data| {
        data.insert_temp(clipboard_id(), PageClipboard { text, pages, cut });
    });
}

pub fn is_cut(context: &Context, page: PageId) -> bool {
    context.data(|data| {
        data.get_temp::<PageClipboard>(clipboard_id())
            .is_some_and(|clipboard| clipboard.cut && clipboard.pages.contains(&page))
    })
}

pub fn finish_cut(context: &Context, pages: &[PageId]) {
    context.data_mut(|data| {
        if let Some(mut clipboard) = data.get_temp::<PageClipboard>(clipboard_id())
            && clipboard.cut
            && clipboard.pages == pages
        {
            clipboard.cut = false;
            data.insert_temp(clipboard_id(), clipboard);
        }
    });
}

pub fn take_paste(
    ui: &Ui,
    parent: PageId,
    position: Option<usize>,
    replace_block: Option<i64>,
) -> Option<Action> {
    let clipboard = ui
        .ctx()
        .data(|data| data.get_temp::<PageClipboard>(clipboard_id()))?;
    let pasted = ui.input_mut(|input| {
        let index = input.events.iter().position(
            |event| matches!(event, egui::Event::Paste(text) if text == &clipboard.text),
        )?;
        input.events.remove(index);
        Some(())
    });
    pasted?;
    Some(Action::TransferPages {
        pages: clipboard.pages,
        parent: Some(parent),
        position,
        replace_block,
        copy: !clipboard.cut,
    })
}

pub fn clipboard_shortcut(ui: &Ui) -> Option<bool> {
    ui.input_mut(|input| {
        let cut = input.consume_key(Modifiers::COMMAND, Key::X)
            || input.consume_key(Modifiers::CTRL, Key::X)
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Cut));
        let copy = input.consume_key(Modifiers::COMMAND, Key::C)
            || input.consume_key(Modifiers::CTRL, Key::C)
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Copy));
        if cut || copy {
            input
                .events
                .retain(|event| !matches!(event, egui::Event::Cut | egui::Event::Copy));
            Some(cut)
        } else {
            None
        }
    })
}

pub fn drop_target(
    ui: &Ui,
    rect: egui::Rect,
    parent: Option<PageId>,
    position: Option<usize>,
    palette: crate::ui::palette::Palette,
    intents: &mut Vec<crate::ui::Intent>,
) {
    let response = ui.interact(
        rect.intersect(ui.clip_rect()),
        ui.id().with(("page-transfer-drop", parent, position)),
        egui::Sense::hover(),
    );
    if response.dnd_hover_payload::<PageDragPayload>().is_some() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            4.0,
            egui::Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Inside,
        );
        if let Some(payload) = response.dnd_release_payload::<PageDragPayload>() {
            intents.push(crate::ui::Intent::Action(Action::TransferPages {
                pages: payload.pages.clone(),
                parent,
                position,
                replace_block: None,
                copy: false,
            }));
        }
    }
}
