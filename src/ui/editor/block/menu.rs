use eframe::egui::{self, Rect, RichText, Sense, Ui, vec2};

use crate::domain::model::{Block, BlockId, BlockKind, DropPlacement};
use crate::worker::Action;

use crate::ui::text;
use crate::ui::{Intent, ThemeSection, palette::Palette};

use super::{HANDLE_WIDTH, block_widget_id, parse_file_link};

#[allow(clippy::too_many_arguments)]
pub(super) fn show_menu(
    ui: &mut Ui,
    rect: Rect,
    block: &mut Block,
    selected: bool,
    bulk_selection_active: bool,
    selected_blocks: &[BlockId],
    palette: Palette,
    widget_scope: Option<u64>,
    intents: &mut Vec<Intent>,
) -> bool {
    let row_response = ui.interact(
        rect,
        block_widget_id("row", block.id, widget_scope),
        Sense::hover(),
    );
    let handle_rect = Rect::from_min_size(rect.min, vec2(HANDLE_WIDTH - 3.0, 27.0));
    let popup_id = block_widget_id("block-menu", block.id, widget_scope);
    let popup_open = ui.memory(|memory| memory.is_popup_open(popup_id));
    let handle_hit = handle_rect.expand2(vec2(8.0, 4.0));
    let handle_hovered = ui.rect_contains_pointer(handle_hit);
    let row_hovered = row_response.hovered() || ui.rect_contains_pointer(rect) || handle_hovered;
    let handle_color = if selected {
        palette.accent
    } else if handle_hovered {
        palette.text
    } else if row_hovered || popup_open {
        palette.muted
    } else {
        egui::Color32::TRANSPARENT
    };
    let mut changed = false;
    let mut kind_change = None;
    let handle = ui.put(
        handle_rect,
        egui::Button::new(RichText::new("⠿").color(handle_color))
            .frame(false)
            .sense(Sense::click_and_drag()),
    );
    let handle = handle
        .on_hover_text("Block menu · Drag to move · Hold Shift to drop above or below all columns");
    if handle.drag_started() {
        let payload_blocks = if selected && !selected_blocks.is_empty() {
            selected_blocks.to_vec()
        } else {
            vec![block.id]
        };
        handle.dnd_set_drag_payload((block.page_id, payload_blocks));
    }
    let hover_payload = row_response
        .dnd_hover_payload::<(i64, Vec<i64>)>()
        .map(|p| (p.0, p.1.clone()))
        .or_else(|| {
            row_response
                .dnd_hover_payload::<(i64, i64)>()
                .map(|p| (p.0, vec![p.1]))
        });

    if let Some((page_id, source_blocks)) = hover_payload
        && page_id == block.page_id
        && !source_blocks.contains(&block.id)
        && let Some(pointer) = ui.ctx().pointer_interact_pos()
    {
        let side_zone = rect.width() * 0.22;
        let placement = if pointer.x <= rect.left() + side_zone {
            DropPlacement::Left
        } else if pointer.x >= rect.right() - side_zone {
            DropPlacement::Right
        } else if block.column_group.is_some() && !ui.input(|input| input.modifiers.shift) {
            if pointer.y < rect.center().y {
                DropPlacement::BeforeInColumn
            } else {
                DropPlacement::AfterInColumn
            }
        } else if pointer.y < rect.center().y {
            DropPlacement::Before
        } else {
            DropPlacement::After
        };
        let preview = match placement {
            DropPlacement::Before | DropPlacement::BeforeInColumn => {
                Rect::from_min_max(rect.left_top(), rect.right_top() + vec2(0.0, 3.0))
            }
            DropPlacement::After | DropPlacement::AfterInColumn => {
                Rect::from_min_max(rect.left_bottom() - vec2(0.0, 3.0), rect.right_bottom())
            }
            DropPlacement::Left => {
                Rect::from_min_max(rect.left_top(), rect.left_bottom() + vec2(side_zone, 0.0))
            }
            DropPlacement::Right => {
                Rect::from_min_max(rect.right_top() - vec2(side_zone, 0.0), rect.right_bottom())
            }
        };
        ui.painter().rect_filled(preview, 4.0, palette.accent_soft);

        let release_payload = row_response
            .dnd_release_payload::<(i64, Vec<i64>)>()
            .map(|p| (p.0, p.1.clone()))
            .or_else(|| {
                row_response
                    .dnd_release_payload::<(i64, i64)>()
                    .map(|p| (p.0, vec![p.1]))
            });

        if let Some((page, sources)) = release_payload {
            intents.push(Intent::Action(Action::ArrangeBlocks {
                page,
                sources,
                target: block.id,
                placement,
            }));
        }
    }
    let is_page_link = block.kind == BlockKind::Text && block.content.trim_start().starts_with('↗');
    let file_link_data = if block.kind == BlockKind::Text && !is_page_link {
        parse_file_link(&block.content)
    } else {
        None
    };
    let is_file_link = file_link_data.is_some();
    let editing_id = block_widget_id("page-link-editing", block.id, widget_scope);
    let started_id = block_widget_id("page-link-editing-started", block.id, widget_scope);
    let is_editing = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(editing_id))
        .unwrap_or(false);

    let right_clicked_on_row = !bulk_selection_active
        && !(is_page_link && !is_editing)
        && !(is_file_link && !is_editing)
        && ui.input(|input| {
            input.pointer.button_clicked(egui::PointerButton::Secondary)
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|position| rect.contains(position))
        });
    let has_modifiers =
        ui.input(|input| input.modifiers.shift || input.modifiers.command || input.modifiers.ctrl);
    if !bulk_selection_active
        && !has_modifiers
        && (handle.clicked() || handle.secondary_clicked() || right_clicked_on_row)
    {
        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
    }
    crate::ui::menus::popup_below_widget(ui, popup_id, &handle, |ui| {
        ui.set_min_width(190.0);
        if is_page_link {
            if ui.button("✏  Düzenle").clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(editing_id, true);
                    d.insert_temp(started_id, true);
                });
                ui.close_menu();
            }
            let target_title = block
                .content
                .trim_start()
                .trim_start_matches("↗ ")
                .trim_start_matches('↗')
                .trim();
            let display_title = if target_title.is_empty() {
                "Untitled"
            } else {
                target_title
            };
            if ui.button(format!("↗  '{display_title}' Aç")).clicked() {
                intents.push(Intent::OpenTitle(display_title.to_string()));
                ui.close_menu();
            }
            ui.separator();
        }
        if let Some((file_path, file_name)) = file_link_data.as_ref() {
            if ui.button("✏  Düzenle").clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(editing_id, true);
                    d.insert_temp(started_id, true);
                });
                ui.close_menu();
            }
            if ui.button(format!("📁  '{file_name}' Aç")).clicked() {
                intents.push(Intent::OpenFileExplorer(file_path.clone()));
                ui.close_menu();
            }
            if ui.button("📋  Dosya yolunu kopyala").clicked() {
                ui.ctx().copy_text(file_path.clone());
                ui.close_menu();
            }
            ui.separator();
        }
        ui.label(RichText::new("BLOCK TYPE").small().color(palette.muted));
        for kind in BlockKind::ALL {
            if ui
                .selectable_label(kind == block.kind, kind.label())
                .clicked()
            {
                kind_change = Some(kind);
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button(text::COPY_BLOCKS).clicked() {
            crate::ui::block_clipboard::copy(ui.ctx(), vec![block.clone()]);
            ui.close_menu();
        }
        if ui.button("▲ Move up").clicked() {
            intents.push(Intent::Action(Action::MoveBlock {
                page: block.page_id,
                block: block.id,
                direction: -1,
            }));
            ui.close_menu();
        }
        if ui.button("▼ Move down").clicked() {
            intents.push(Intent::Action(Action::MoveBlock {
                page: block.page_id,
                block: block.id,
                direction: 1,
            }));
            ui.close_menu();
        }
        if ui.button("+ Add text below").clicked() {
            intents.push(Intent::Action(Action::InsertBlock {
                column_anchor: block.column_group.map(|_| block.id),
                page: block.page_id,
                position: block.position + 1,
                kind: BlockKind::Text,
                content: String::new(),
                text_styles: Vec::new(),
            }));
            ui.close_menu();
        }
        ui.separator();
        if ui
            .button(RichText::new("Delete block").color(palette.danger))
            .clicked()
        {
            intents.push(Intent::Action(Action::DeleteBlock {
                page: block.page_id,
                block: block.id,
            }));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("🎨 Temayı düzenle").clicked() {
            intents.push(Intent::OpenSettingsTheme(ThemeSection::Editor));
            ui.close_menu();
        }
    });
    if let Some(kind) = kind_change {
        block.kind = kind;
        changed = true;
    }
    changed
}
