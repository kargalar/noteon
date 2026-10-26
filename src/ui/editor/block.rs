use eframe::egui::{
    self, Align2, Color32, FontFamily, FontId, Key, KeyboardShortcut, Modifiers, Rect, RichText,
    Sense, Stroke, TextFormat, Ui, pos2, vec2,
};
use std::sync::Arc;

use crate::domain::editing::split_at_selection;
use crate::domain::model::{
    Block, BlockId, BlockKind, DropPlacement, InlineFontFamily, InlineTextStyle, TextStyleRange,
};
use crate::worker::Action;

use crate::ui::text;
use crate::ui::{Intent, ThemeSection, palette::Palette};

pub const BLOCK_PADDING: f32 = 12.0;
pub const MAX_BLOCK_HEIGHT: f32 = 260.0;
pub const HANDLE_WIDTH: f32 = 30.0;
pub const DATABASE_BLOCK_HEIGHT: f32 = 520.0;
pub const MEDIA_BLOCK_HEIGHT: f32 = 320.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlashCommandTarget {
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

pub fn parse_file_link(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with('[') && trimmed.contains("](") && trimmed.ends_with(')') {
        if let (Some(b_end), Some(p_start)) = (trimmed.find(']'), trimmed.find("](")) {
            let label = &trimmed[1..b_end];
            let target = trimmed[p_start + 2..trimmed.len() - 1].trim();
            let unquoted_target = target.trim_matches('"').trim_matches('\'');
            if super::links::is_file_path(unquoted_target) {
                let path = super::links::normalize_file_path(unquoted_target);
                let name = if label.trim().is_empty() {
                    super::links::extract_file_or_folder_name(&path)
                } else {
                    label.trim().to_string()
                };
                return Some((path, name));
            }
        }
    }

    let clean = if let Some(stripped) = trimmed.strip_prefix('📁') {
        stripped.trim()
    } else {
        trimmed
    };
    let unquoted = clean.trim_matches('"').trim_matches('\'');
    if super::links::is_file_path(unquoted) {
        let path = super::links::normalize_file_path(unquoted);
        let name = super::links::extract_file_or_folder_name(&path);
        Some((path, name))
    } else {
        None
    }
}

pub fn font(kind: BlockKind) -> FontId {
    FontId::new(
        kind.font_size(),
        if kind == BlockKind::Code {
            FontFamily::Monospace
        } else {
            FontFamily::Proportional
        },
    )
}

pub fn text_indent(kind: BlockKind) -> f32 {
    match kind {
        BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo | BlockKind::Callout => 30.0,
        BlockKind::Quote | BlockKind::Code => 17.0,
        _ => 5.0,
    }
}

pub fn measured_height(ui: &Ui, block: &Block, width: f32) -> f32 {
    if block.kind == BlockKind::Database {
        return DATABASE_BLOCK_HEIGHT;
    }
    if matches!(block.kind, BlockKind::Image | BlockKind::Video) {
        return MEDIA_BLOCK_HEIGHT;
    }
    if block.kind == BlockKind::Divider {
        return 32.0;
    }
    let text_width = (width - HANDLE_WIDTH - text_indent(block.kind) - 14.0).max(80.0);
    let measure_str;
    let text = if let Some((_, name)) = parse_file_link(&block.content) {
        measure_str = format!("📁  {name}");
        &measure_str
    } else if block.content.is_empty() {
        " "
    } else {
        &block.content
    };
    let galley = layout_styled_text(
        ui,
        text,
        text_width,
        font(block.kind),
        egui::Color32::WHITE,
        &block.text_styles,
        None,
        None,
    );
    (galley.size().y + BLOCK_PADDING * 2.0).clamp(36.0, MAX_BLOCK_HEIGHT)
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    rect: Rect,
    block: &mut Block,
    focus: bool,
    focus_at_end: bool,
    selected: bool,
    bulk_selection_active: bool,
    selected_blocks: &[BlockId],
    palette: Palette,
    widget_scope: Option<u64>,
    intents: &mut Vec<Intent>,
    page_link_icon: Option<&str>,
    find_query: Option<&str>,
    find_active_match: Option<(usize, usize)>,
) -> bool {
    let mut changed = show_menu(
        ui,
        rect,
        block,
        selected,
        bulk_selection_active,
        selected_blocks,
        palette,
        widget_scope,
        intents,
    );
    if block.kind == BlockKind::Database {
        return changed;
    }
    if matches!(block.kind, BlockKind::Image | BlockKind::Video) {
        changed |= show_media(ui, rect, block, palette, widget_scope);
        if changed {
            intents.push(Intent::BlockChanged(block.clone()));
        }
        return changed;
    }
    let body = Rect::from_min_max(
        rect.min + vec2(HANDLE_WIDTH, 0.0),
        rect.max - vec2(4.0, 0.0),
    );
    if block.kind == BlockKind::Divider {
        ui.painter().line_segment(
            [body.left_center(), body.right_center()],
            Stroke::new(1.0, palette.border),
        );
    } else {
        let is_page_link =
            block.kind == BlockKind::Text && block.content.trim_start().starts_with('↗');
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

        if is_page_link && !is_editing {
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

            let link_rect = Rect::from_min_max(
                body.min + vec2(text_indent(block.kind), 2.0),
                body.max - vec2(4.0, 2.0),
            );
            let link_id = block_widget_id("page-link-surface", block.id, widget_scope);
            let hover_tip = if let Some(icon) = page_link_icon {
                format!("'{icon} {display_title}' sayfasını aç (Düzenlemek için sağ tıkla)")
            } else {
                format!("'{display_title}' sayfasını aç (Düzenlemek için sağ tıkla)")
            };
            let link_resp = ui
                .interact(link_rect, link_id, Sense::click())
                .on_hover_text(hover_tip);

            if link_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(link_rect, 4.0, palette.hover);
            }

            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = (link_rect.width() - 16.0).max(40.0);
            job.append(
                "↗  ",
                0.0,
                egui::TextFormat {
                    font_id: FontId::proportional(15.0),
                    color: palette.accent,
                    ..Default::default()
                },
            );
            if let Some(icon) = page_link_icon.filter(|i| !i.is_empty()) {
                job.append(
                    &format!("{icon}  "),
                    0.0,
                    egui::TextFormat {
                        font_id: FontId::proportional(15.0),
                        color: palette.text,
                        ..Default::default()
                    },
                );
            }
            append_text_with_find(
                &mut job,
                display_title,
                FontId::proportional(15.0),
                palette.text,
                if link_resp.hovered() {
                    Stroke::new(1.0, palette.muted)
                } else {
                    Stroke::NONE
                },
                find_query,
                find_active_match.is_some(),
            );
            let galley = ui.fonts(|f| f.layout_job(job));
            let y_offset = if galley.size().y < link_rect.height() - 4.0 {
                (link_rect.height() - galley.size().y) * 0.5
            } else {
                4.0
            };
            let text_pos = pos2(link_rect.min.x + 8.0, link_rect.min.y + y_offset);
            ui.painter().galley(text_pos, galley, palette.text);

            if link_resp.clicked() || link_resp.middle_clicked() {
                intents.push(Intent::OpenTitle(display_title.to_string()));
            }

            link_resp.context_menu(|ui| {
                ui.set_min_width(170.0);
                if ui.button("✏  Düzenle").clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(editing_id, true);
                        d.insert_temp(started_id, true);
                    });
                    ui.close_menu();
                }
                let open_label = if let Some(icon) = page_link_icon.filter(|i| !i.is_empty()) {
                    format!("↗  {icon}  '{display_title}' Aç")
                } else {
                    format!("↗  '{display_title}' Aç")
                };
                if ui.button(open_label).clicked() {
                    intents.push(Intent::OpenTitle(display_title.to_string()));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("▲  Yukarı taşı").clicked() {
                    intents.push(Intent::Action(Action::MoveBlock {
                        page: block.page_id,
                        block: block.id,
                        direction: -1,
                    }));
                    ui.close_menu();
                }
                if ui.button("▼  Aşağı taşı").clicked() {
                    intents.push(Intent::Action(Action::MoveBlock {
                        page: block.page_id,
                        block: block.id,
                        direction: 1,
                    }));
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(RichText::new("🗑  Bloğu sil").color(palette.danger))
                    .clicked()
                {
                    intents.push(Intent::Action(Action::DeleteBlock {
                        page: block.page_id,
                        block: block.id,
                    }));
                    ui.close_menu();
                }
            });
        } else if let Some((file_path, file_name)) = file_link_data.filter(|_| !is_editing) {
            let link_rect = Rect::from_min_max(
                body.min + vec2(text_indent(block.kind), 2.0),
                body.max - vec2(4.0, 2.0),
            );
            let link_id = block_widget_id("file-link-surface", block.id, widget_scope);
            let hover_tip =
                format!("📁 '{file_name}'\n{file_path}\n(Dosya Gezgini'nde açmak için tıkla)");
            let link_resp = ui
                .interact(link_rect, link_id, Sense::click())
                .on_hover_text(hover_tip);

            if link_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(link_rect, 4.0, palette.hover);
            }

            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = (link_rect.width() - 16.0).max(40.0);
            job.append(
                "📁  ",
                0.0,
                egui::TextFormat {
                    font_id: FontId::proportional(15.0),
                    color: palette.text,
                    ..Default::default()
                },
            );
            append_text_with_find(
                &mut job,
                &file_name,
                FontId::proportional(15.0),
                palette.text,
                if link_resp.hovered() {
                    Stroke::new(1.0, palette.muted)
                } else {
                    Stroke::NONE
                },
                find_query,
                find_active_match.is_some(),
            );
            let galley = ui.fonts(|f| f.layout_job(job));
            let y_offset = if galley.size().y < link_rect.height() - 4.0 {
                (link_rect.height() - galley.size().y) * 0.5
            } else {
                4.0
            };
            let text_pos = pos2(link_rect.min.x + 8.0, link_rect.min.y + y_offset);
            ui.painter().galley(text_pos, galley, palette.text);

            if link_resp.double_clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(editing_id, true);
                    d.insert_temp(started_id, true);
                });
            } else if link_resp.clicked() || link_resp.middle_clicked() {
                intents.push(Intent::OpenFileExplorer(file_path.clone()));
            }

            link_resp.context_menu(|ui| {
                ui.set_min_width(170.0);
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
                if ui.button("▲  Yukarı taşı").clicked() {
                    intents.push(Intent::Action(Action::MoveBlock {
                        page: block.page_id,
                        block: block.id,
                        direction: -1,
                    }));
                    ui.close_menu();
                }
                if ui.button("▼  Aşağı taşı").clicked() {
                    intents.push(Intent::Action(Action::MoveBlock {
                        page: block.page_id,
                        block: block.id,
                        direction: 1,
                    }));
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(RichText::new("🗑  Bloğu sil").color(palette.danger))
                    .clicked()
                {
                    intents.push(Intent::Action(Action::DeleteBlock {
                        page: block.page_id,
                        block: block.id,
                    }));
                    ui.close_menu();
                }
            });
        } else {
            changed |= show_decoration(ui, body, block, palette);
            let text_rect = Rect::from_min_max(
                body.min + vec2(text_indent(block.kind), BLOCK_PADDING - 4.0),
                body.max - vec2(8.0, 5.0),
            );
            let just_started = ui.ctx().data_mut(|d| {
                if d.get_temp::<bool>(started_id).unwrap_or(false) {
                    d.remove::<bool>(started_id);
                    true
                } else {
                    false
                }
            });
            let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let text_focus = focus || just_started;
            changed |= show_text(
                ui,
                text_rect,
                block,
                text_focus,
                focus_at_end,
                palette,
                widget_scope,
                intents,
                find_query,
                find_active_match,
            );

            if is_page_link || is_file_link {
                let editor_id = block_widget_id("block", block.id, widget_scope);
                if just_started {
                    if let Some(mut state) =
                        egui::text_edit::TextEditState::load(ui.ctx(), editor_id)
                    {
                        let count = block.content.chars().count();
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(count),
                            )));
                        state.store(ui.ctx(), editor_id);
                    }
                }
                let has_focus = ui.ctx().memory(|m| m.has_focus(editor_id));
                if (!just_started && !has_focus) || esc_pressed || enter_pressed {
                    ui.ctx().data_mut(|d| {
                        d.remove::<bool>(editing_id);
                        d.remove::<bool>(started_id);
                    });
                    if esc_pressed || enter_pressed {
                        ui.ctx().memory_mut(|m| m.surrender_focus(editor_id));
                    }
                }
            }
        }
    }
    if changed {
        intents.push(Intent::BlockChanged(block.clone()));
    }
    changed
}

fn show_media(
    ui: &mut Ui,
    rect: Rect,
    block: &mut Block,
    palette: Palette,
    widget_scope: Option<u64>,
) -> bool {
    let body = Rect::from_min_max(
        rect.min + vec2(HANDLE_WIDTH, 4.0),
        rect.max - vec2(12.0, 4.0),
    );
    let width = (body.width() * block.media_width.clamp(0.25, 1.0)).min(body.width());
    let media_rect = Rect::from_min_size(body.min, vec2(width, MEDIA_BLOCK_HEIGHT - 34.0));
    let path = std::path::Path::new(&block.content);
    if path.is_file() {
        ui.scope_builder(egui::UiBuilder::new().max_rect(media_rect), |ui| {
            if block.kind == BlockKind::Image {
                let _ = super::media::show_image(
                    ui,
                    path,
                    media_rect.size(),
                    &format!("{}-{}-{widget_scope:?}", block.page_id, block.id),
                );
            } else {
                super::media::show_video(
                    ui,
                    path,
                    media_rect.size() - vec2(0.0, 34.0),
                    &format!("{}-{}-{widget_scope:?}", block.page_id, block.id),
                );
            }
        });
    } else {
        ui.painter().rect_filled(media_rect, 4.0, palette.hover);
        ui.painter().text(
            media_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Media file not found",
            egui::FontId::proportional(14.0),
            palette.muted,
        );
    }
    let grip_rect = Rect::from_center_size(
        pos2(media_rect.right() - 8.0, media_rect.center().y),
        vec2(20.0, 36.0),
    );
    let resize_id = block_widget_id("media-resize", block.id, widget_scope);
    let resize_start_id = block_widget_id("media-resize-start", block.id, widget_scope);
    let grip = ui
        .interact(grip_rect, resize_id, Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to resize media");
    let grip_color = if grip.hovered() || grip.dragged() {
        palette.accent
    } else {
        palette.border
    };
    ui.painter()
        .rect_filled(grip_rect, 5.0, palette.hover.gamma_multiply(0.9));
    for offset in [-3.0, 3.0] {
        ui.painter().line_segment(
            [
                pos2(grip_rect.center().x + offset, grip_rect.center().y - 8.0),
                pos2(grip_rect.center().x + offset, grip_rect.center().y + 8.0),
            ],
            Stroke::new(1.5, grip_color),
        );
    }
    if grip.drag_started() {
        ui.ctx()
            .data_mut(|data| data.insert_temp(resize_start_id, block.media_width));
    }
    if grip.dragged() || grip.drag_stopped() {
        let start = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(resize_start_id))
            .unwrap_or(block.media_width);
        block.media_width = (start + grip.drag_delta().x / body.width()).clamp(0.25, 1.0);
        if grip.drag_stopped() {
            ui.ctx()
                .data_mut(|data| data.remove::<f32>(resize_start_id));
        }
        return true;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn show_menu(
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
    let handle = handle.on_hover_text("Block menu");
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
        } else if pointer.y < rect.center().y {
            DropPlacement::Before
        } else {
            DropPlacement::After
        };
        let preview = match placement {
            DropPlacement::Before => {
                Rect::from_min_max(rect.left_top(), rect.right_top() + vec2(0.0, 3.0))
            }
            DropPlacement::After => {
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
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &handle,
        egui::popup::PopupCloseBehavior::CloseOnClick,
        |ui| {
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
        },
    );
    if let Some(kind) = kind_change {
        block.kind = kind;
        changed = true;
    }
    changed
}

fn show_decoration(ui: &mut Ui, body: Rect, block: &mut Block, palette: Palette) -> bool {
    let mut changed = false;
    if matches!(block.kind, BlockKind::Callout | BlockKind::Code) {
        ui.painter().rect_filled(
            body.shrink2(vec2(0.0, 3.0)),
            5.0,
            if block.kind == BlockKind::Callout {
                palette.accent_soft
            } else {
                palette.code
            },
        );
    }
    if block.kind == BlockKind::Quote {
        ui.painter().line_segment(
            [
                body.left_top() + vec2(2.0, 8.0),
                body.left_bottom() - vec2(-2.0, 8.0),
            ],
            Stroke::new(3.0, palette.accent),
        );
    }
    let marker_center = body.left_top() + vec2(12.0, 20.0);
    match block.kind {
        BlockKind::Todo => {
            let checkbox = Rect::from_center_size(marker_center, vec2(22.0, 24.0));
            if ui
                .put(checkbox, egui::Checkbox::without_text(&mut block.checked))
                .changed()
            {
                changed = true;
            }
        }
        BlockKind::Bullet => {
            ui.painter().circle_filled(marker_center, 2.5, palette.text);
        }
        BlockKind::Numbered => {
            ui.painter().text(
                marker_center,
                Align2::CENTER_CENTER,
                format!("{}.", block.position + 1),
                FontId::proportional(12.0),
                palette.muted,
            );
        }
        BlockKind::Callout => {
            ui.painter().text(
                marker_center,
                Align2::CENTER_CENTER,
                "✦",
                FontId::proportional(17.0),
                palette.accent,
            );
        }
        _ => {}
    }
    changed
}

fn show_text(
    ui: &mut Ui,
    text_rect: Rect,
    block: &mut Block,
    focus: bool,
    focus_at_end: bool,
    palette: Palette,
    widget_scope: Option<u64>,
    intents: &mut Vec<Intent>,
    find_query: Option<&str>,
    find_active_match: Option<(usize, usize)>,
) -> bool {
    let mut changed = false;
    let mut focused = false;
    let hint = if block.kind == BlockKind::Text {
        RichText::new(text::TEXT_PLACEHOLDER).color(egui::Color32::from_rgba_unmultiplied(
            palette.muted.r(),
            palette.muted.g(),
            palette.muted.b(),
            52,
        ))
    } else {
        RichText::new(block.kind.label())
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(("block-content", block.id))
            .max_rect(text_rect),
        |ui| {
            ui.set_clip_rect(text_rect.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical()
                .id_salt(("block-scroll", block.id))
                .max_height(text_rect.height())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let editor_id = block_widget_id("block", block.id, widget_scope);
                    let was_focused = ui.ctx().memory(|m| m.has_focus(editor_id));
                    let was_empty = block.content.is_empty();

                    let backspace_event = ui.input(|i| {
                        i.events.iter().any(|e| match e {
                            egui::Event::Key {
                                key: Key::Backspace | Key::Delete,
                                pressed: true,
                                modifiers,
                                ..
                            } => modifiers.is_none(),
                            _ => false,
                        })
                    });

                    if (was_focused || focus) && was_empty && backspace_event {
                        ui.input_mut(|i| {
                            i.consume_key(Modifiers::NONE, Key::Backspace);
                            i.consume_key(Modifiers::NONE, Key::Delete);
                            i.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::Backspace | Key::Delete, .. }));
                        });
                        if block.kind != BlockKind::Text {
                            block.kind = BlockKind::Text;
                            block.text_styles.clear();
                            ui.ctx().memory_mut(|m| m.request_focus(editor_id));
                            changed = true;
                            return;
                        } else {
                            ui.ctx().memory_mut(|m| m.surrender_focus(editor_id));
                            intents.push(Intent::Action(Action::DeleteBlock {
                                page: block.page_id,
                                block: block.id,
                            }));
                            changed = true;
                            return;
                        }
                    }

                    let snapshot_id =
                        block_widget_id("text-content-snapshot", block.id, widget_scope);
                    let previous_content =
                        ui.ctx().data(|data| data.get_temp::<String>(snapshot_id));
                    let styles = &block.text_styles;
                    let base_font = font(block.kind);
                    let base_color = if block.checked && block.kind == BlockKind::Todo {
                        palette.muted
                    } else if block.kind == BlockKind::Text && block.content.trim_start().starts_with('↗') {
                        palette.accent
                    } else {
                        palette.text
                    };
                    let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
                        layout_styled_text(
                            ui,
                            text,
                            wrap_width,
                            base_font.clone(),
                            base_color,
                            styles,
                            find_query,
                            find_active_match,
                        )
                    };
                    let format_selection_id = block_widget_id("format-selection", block.id, widget_scope);
                    let prev_edit_state = egui::text_edit::TextEditState::load(ui.ctx(), editor_id);
                    let selected_char_range_before = prev_edit_state
                        .as_ref()
                        .and_then(|s| s.cursor.char_range())
                        .map(|r| {
                            let min = r.primary.index.min(r.secondary.index);
                            let max = r.primary.index.max(r.secondary.index);
                            min..max
                        })
                        .or_else(|| {
                            ui.ctx()
                                .data(|d| d.get_temp::<std::ops::Range<usize>>(format_selection_id))
                        })
                        .filter(|r| r.start < r.end);
                    let initial_char_count = block.content.chars().count();

                    let mut output = egui::TextEdit::multiline(&mut block.content)
                        .id(editor_id)
                        .font(base_font.clone())
                        .text_color(base_color)
                        .layouter(&mut layouter)
                        .hint_text(hint)
                        .frame(false)
                        .margin(egui::Margin::ZERO)
                        .desired_rows(1)
                        .desired_width(text_rect.width())
                        .return_key(if block.kind == BlockKind::Code {
                            KeyboardShortcut::new(Modifiers::NONE, Key::Enter)
                        } else {
                            KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)
                        })
                        .lock_focus(true)
                        .show(ui);
                    update_multi_click_selection(ui, &mut output);
                    if output.response.changed()
                        && let Some(previous_content) = previous_content.as_deref()
                    {
                        adjust_text_styles(
                            previous_content,
                            &block.content,
                            &mut block.text_styles,
                        );
                    }
                    if output.response.has_focus() {
                        if output.response.changed() || previous_content.is_none() {
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(snapshot_id, block.content.clone())
                            });
                        }
                    } else if previous_content.is_some() {
                        ui.ctx().data_mut(|data| data.remove::<String>(snapshot_id));
                    }
                    if focus {
                        output.response.request_focus();
                        let char_count = block.content.chars().count();
                        let cursor = if focus_at_end {
                            egui::text::CCursor::new(char_count)
                        } else {
                            egui::text::CCursor::new(0)
                        };
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
                        output.state.store(ui.ctx(), output.response.id);
                    }
                    focused = output.response.has_focus() || output.response.lost_focus();
                    changed |= output.response.changed();

                    let current_selection = output
                        .cursor_range
                        .map(|range| range.as_sorted_char_range())
                        .filter(|range| range.start < range.end);
                    let selected_char_range = selected_char_range_before.or(current_selection);

                    let content_was_cut = block.content.chars().count() < initial_char_count;
                    let had_selection = selected_char_range.is_some() || content_was_cut;

                    if focused {
                        let cut_pressed = ui.input_mut(|i| {
                            (i.consume_key(Modifiers::CTRL, Key::X) || i.consume_key(Modifiers::COMMAND, Key::X))
                                || i.events.iter().any(|e| matches!(e, egui::Event::Cut))
                        });
                        if cut_pressed {
                            ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Cut)));
                            if had_selection {
                                if !content_was_cut && let Some(range) = selected_char_range {
                                    let text = &block.content;
                                    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
                                    let start_byte = char_indices.get(range.start).map(|(i, _)| *i).unwrap_or(text.len());
                                    let end_byte = char_indices.get(range.end).map(|(i, _)| *i).unwrap_or(text.len());
                                    if start_byte < end_byte {
                                        let cut_text = text[start_byte..end_byte].to_string();
                                        ui.ctx().copy_text(cut_text);
                                        block.content.replace_range(start_byte..end_byte, "");
                                        if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), editor_id) {
                                            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(range.start))));
                                            state.store(ui.ctx(), editor_id);
                                        }
                                        changed = true;
                                    }
                                }
                            } else {
                                if !block.content.is_empty() {
                                    ui.ctx().copy_text(block.content.clone());
                                }
                                ui.ctx().memory_mut(|m| m.surrender_focus(editor_id));
                                intents.push(Intent::Action(Action::DeleteBlock {
                                    page: block.page_id,
                                    block: block.id,
                                }));
                                changed = true;
                                return;
                            }
                        }
                    }

                    let detected_links = super::links::find_links_and_paths(&block.content);
                    if !detected_links.is_empty() {
                        let pointer_pos =
                            ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
                        if let Some(pos) = pointer_pos {
                            if output.response.rect.contains(pos) {
                                let relative_pos = pos - output.response.rect.min;
                                let cursor = output.galley.cursor_from_pos(relative_pos);
                                let char_index = cursor.ccursor.index;
                                let char_rect = output
                                    .galley
                                    .pos_from_cursor(&cursor)
                                    .translate(output.response.rect.min.to_vec2())
                                    .expand(3.0);

                                if char_rect.contains(pos) {
                                    if let Some(link) = detected_links
                                        .iter()
                                        .find(|l| l.char_range.contains(&char_index))
                                    {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

                                        let is_dark = palette.text.r() > 150;
                                        let web_color = if is_dark {
                                            egui::Color32::from_rgb(88, 166, 255)
                                        } else {
                                            egui::Color32::from_rgb(9, 105, 218)
                                        };

                                        egui::show_tooltip(
                                            ui.ctx(),
                                            ui.layer_id(),
                                            egui::Id::new(("link-tooltip", block.id)),
                                            |ui| {
                                                ui.set_max_width(360.0);
                                                match &link.link_type {
                                                    super::links::LinkType::Web { url } => {
                                                        ui.horizontal(|ui| {
                                                            ui.label(RichText::new("🌐").size(15.0));
                                                            ui.vertical(|ui| {
                                                                ui.label(
                                                                    RichText::new("Website Bağlantısı")
                                                                        .strong()
                                                                        .size(12.0),
                                                                );
                                                                ui.label(
                                                                    RichText::new(url)
                                                                        .color(web_color)
                                                                        .size(11.0),
                                                                );
                                                                ui.label(
                                                                    RichText::new(
                                                                        "Tarayıcıda açmak için tıkla",
                                                                    )
                                                                    .italics()
                                                                    .size(10.0)
                                                                    .color(palette.muted),
                                                                );
                                                            });
                                                        });
                                                    }
                                                    super::links::LinkType::FilePath { path } => {
                                                        let name = super::links::extract_file_or_folder_name(path);
                                                        ui.horizontal(|ui| {
                                                            ui.label(RichText::new("📁").size(16.0));
                                                            ui.vertical(|ui| {
                                                                ui.label(
                                                                    RichText::new(&name)
                                                                        .strong()
                                                                        .size(12.0),
                                                                );
                                                                ui.label(
                                                                    RichText::new(path)
                                                                        .color(palette.muted)
                                                                        .size(10.0),
                                                                );
                                                                ui.label(
                                                                    RichText::new(
                                                                        "Dosya Gezgini'nde açmak için tıkla",
                                                                    )
                                                                    .italics()
                                                                    .size(10.0)
                                                                    .color(palette.muted),
                                                                );
                                                            });
                                                        });
                                                    }
                                                }
                                            },
                                        );

                                        let clicked = ui.input(|i| {
                                            i.pointer.primary_clicked()
                                                || i.pointer.button_clicked(egui::PointerButton::Middle)
                                        });
                                        let is_dragging =
                                            ui.input(|i| i.pointer.is_decidedly_dragging());
                                        if clicked && !is_dragging {
                                            match &link.link_type {
                                                super::links::LinkType::Web { url } => {
                                                    intents.push(Intent::OpenUrl(url.clone()));
                                                }
                                                super::links::LinkType::FilePath { path } => {
                                                    intents.push(Intent::OpenFileExplorer(
                                                        path.clone(),
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    output.response.context_menu(|ui| {
                        ui.set_min_width(200.0);
                        let context_links = super::links::find_links_and_paths(&block.content);
                        for link in &context_links {
                            match &link.link_type {
                                super::links::LinkType::Web { url } => {
                                    let label = if url.len() > 30 {
                                        format!("🌐  {}...", &url[..27])
                                    } else {
                                        format!("🌐  {url}")
                                    };
                                    if ui.button(label).clicked() {
                                        intents.push(Intent::OpenUrl(url.clone()));
                                        ui.close_menu();
                                    }
                                }
                                super::links::LinkType::FilePath { path } => {
                                    let name = super::links::extract_file_or_folder_name(path);
                                    let label = format!("📁  {name}");
                                    if ui.button(label).clicked() {
                                        intents.push(Intent::OpenFileExplorer(path.clone()));
                                        ui.close_menu();
                                    }
                                }
                            }
                        }
                        if !context_links.is_empty() {
                            ui.separator();
                        }
                        if ui.button("▲  Yukarı taşı").clicked() {
                            intents.push(Intent::Action(Action::MoveBlock {
                                page: block.page_id,
                                block: block.id,
                                direction: -1,
                            }));
                            ui.close_menu();
                        }
                        if ui.button("▼  Aşağı taşı").clicked() {
                            intents.push(Intent::Action(Action::MoveBlock {
                                page: block.page_id,
                                block: block.id,
                                direction: 1,
                            }));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(RichText::new("🗑  Bloğu sil").color(palette.danger)).clicked() {
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
                    let selection_id = block_widget_id("format-selection", block.id, widget_scope);
                    let current_range = output
                        .cursor_range
                        .map(|range| range.as_sorted_char_range())
                        .filter(|range| range.start < range.end);
                    let editor_id = block_widget_id("block", block.id, widget_scope);
                    let active_selection_id = egui::Id::new("active-text-format-selection");
                    let mut dismiss_selection = false;
                    let active_editor = ui
                        .ctx()
                        .data(|data| data.get_temp::<egui::Id>(active_selection_id));
                    let is_active = active_editor == Some(editor_id);

                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        dismiss_selection = true;
                    } else if is_active {
                        let pointer_pressed =
                            ui.input(|i| i.pointer.any_pressed() || i.pointer.any_click());
                        if pointer_pressed {
                            let press_pos = ui
                                .input(|i| i.pointer.interact_pos().or_else(|| i.pointer.latest_pos()));
                            if let Some(pos) = press_pos {
                                let toolbar_area_id =
                                    block_widget_id("text-format-toolbar", block.id, widget_scope);
                                let toolbar_rect =
                                    ui.ctx().memory(|m| m.area_rect(toolbar_area_id));
                                let inside_toolbar =
                                    toolbar_rect.is_some_and(|r| r.expand(4.0).contains(pos));
                                let any_popup = ui.ctx().memory(|m| m.any_popup_open());
                                let top_layer = ui.ctx().memory(|m| m.layer_id_at(pos));
                                let inside_popup = any_popup
                                    && top_layer
                                        .is_some_and(|l| l.order == egui::Order::Foreground);
                                let inside_panel = inside_toolbar || inside_popup;

                                if !inside_panel {
                                    let selecting_in_editor =
                                        output.response.rect.contains(pos) && current_range.is_some();
                                    if !selecting_in_editor {
                                        dismiss_selection = true;
                                        if !output.response.rect.contains(pos) {
                                            output.response.surrender_focus();
                                        }
                                    }
                                }
                            } else {
                                dismiss_selection = true;
                            }
                        }
                    }

                    if dismiss_selection {
                        ui.ctx().data_mut(|data| {
                            data.remove::<std::ops::Range<usize>>(selection_id);
                            if data.get_temp::<egui::Id>(active_selection_id) == Some(editor_id) {
                                data.remove::<egui::Id>(active_selection_id);
                            }
                        });
                        if let Some(mut state) =
                            egui::text_edit::TextEditState::load(ui.ctx(), output.response.id)
                        {
                            if let Some(cursor_range) = state.cursor.char_range() {
                                state.cursor.set_char_range(Some(
                                    egui::text::CCursorRange::one(cursor_range.primary),
                                ));
                                state.store(ui.ctx(), output.response.id);
                            }
                        }
                    } else if output.response.has_focus() {
                        ui.ctx().data_mut(|data| {
                            if let Some(range) = current_range.clone() {
                                data.insert_temp(selection_id, range);
                                data.insert_temp(active_selection_id, editor_id);
                            } else {
                                data.remove::<std::ops::Range<usize>>(selection_id);
                                if data.get_temp::<egui::Id>(active_selection_id) == Some(editor_id)
                                {
                                    data.remove::<egui::Id>(active_selection_id);
                                }
                            }
                        });
                    }

                    let active_editor = if dismiss_selection {
                        None
                    } else {
                        ui.ctx()
                            .data(|data| data.get_temp::<egui::Id>(active_selection_id))
                    };
                    let selected_range = if active_editor == Some(editor_id) {
                        current_range.or_else(|| {
                            ui.ctx()
                                .data(|data| data.get_temp::<std::ops::Range<usize>>(selection_id))
                        })
                    } else {
                        None
                    };
                    if let Some(range) = selected_range.filter(|range| range.start < range.end) {
                        changed |= show_text_format_toolbar(
                            ui,
                            output.response.rect,
                            range,
                            block,
                            palette,
                            widget_scope,
                        );
                    }
                    let selected_kind = show_command_suggestions(
                        ui,
                        output.response.rect,
                        block,
                        focused,
                        output.response.changed(),
                        widget_scope,
                    );
                    if let Some(target) = selected_kind {
                        match target {
                            SlashCommandTarget::Block(BlockKind::Database) => {
                                intents.push(Intent::Action(Action::CreateDatabaseBlock {
                                    page: block.page_id,
                                    position: block.position,
                                    replace_block: Some(block.id),
                                }));
                                block.kind = BlockKind::Database;
                                block.content.clear();
                            }
                            SlashCommandTarget::Block(kind) => {
                                block.kind = kind;
                                block.content.clear();
                            }
                            SlashCommandTarget::Page => {
                                intents.push(Intent::Action(Action::CreateSubpageBlock {
                                    page: block.page_id,
                                    position: block.position,
                                    replace_block: Some(block.id),
                                }));
                                block.kind = BlockKind::Text;
                                block.content = "↗ Untitled".to_string();
                            }
                            SlashCommandTarget::File => {
                                if let Some(path) = rfd::FileDialog::new().pick_file() {
                                    block.kind = BlockKind::Text;
                                    block.content = path.to_string_lossy().into_owned();
                                } else {
                                    block.kind = BlockKind::Text;
                                    block.content.clear();
                                }
                            }
                        }
                        block.text_styles.clear();
                        output.response.request_focus();
                        changed = true;
                    } else {
                        let cursor_range = output
                            .cursor_range
                            .map(|range| range.as_sorted_char_range())
                            .or_else(|| {
                                egui::text_edit::TextEditState::load(ui.ctx(), editor_id)
                                    .and_then(|state| state.cursor.char_range())
                                    .map(|range| {
                                        let min = range.primary.index.min(range.secondary.index);
                                        let max = range.primary.index.max(range.secondary.index);
                                        min..max
                                    })
                            });
                        changed |= handle_enter(
                            ui,
                            block,
                            cursor_range,
                            focused,
                            intents,
                        );
                    }
                    if selected_kind.is_none()
                        && changed
                        && let Some(kind) = BlockKind::shortcut(&block.content)
                        && block.content.ends_with(' ')
                    {
                        block.kind = kind;
                        block.content.clear();
                        block.text_styles.clear();
                    }
                });
        },
    );
    changed
}

fn layout_styled_text(
    ui: &Ui,
    text: &str,
    wrap_width: f32,
    base_font: FontId,
    base_color: egui::Color32,
    styles: &[TextStyleRange],
    find_query: Option<&str>,
    find_active_match: Option<(usize, usize)>,
) -> Arc<egui::Galley> {
    let characters: Vec<_> = text.char_indices().collect();
    let detected_links = super::links::find_links_and_paths(text);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let is_dark = ui.visuals().dark_mode;
    let web_color = if is_dark {
        egui::Color32::from_rgb(88, 166, 255)
    } else {
        egui::Color32::from_rgb(9, 105, 218)
    };
    let file_color = if is_dark {
        egui::Color32::from_rgb(130, 180, 240)
    } else {
        egui::Color32::from_rgb(30, 100, 190)
    };

    let query_clean = find_query.map(|q| q.trim()).unwrap_or("");
    let find_ranges: Vec<(std::ops::Range<usize>, bool)> = if !query_clean.is_empty() {
        super::find::find_char_ranges(text, query_clean)
            .into_iter()
            .map(|r| {
                let is_active = find_active_match == Some((r.start, r.end));
                (r, is_active)
            })
            .collect()
    } else {
        Vec::new()
    };

    let find_at = |i: usize| -> Option<bool> {
        find_ranges
            .iter()
            .find(|(r, _)| r.contains(&i))
            .map(|(_, active)| *active)
    };

    let mut start = 0;
    while start < characters.len() {
        let style = style_at(styles, start);
        let link_kind = detected_links
            .iter()
            .find(|l| l.char_range.contains(&start))
            .map(|l| &l.link_type);
        let find_state = find_at(start);
        let mut end = start + 1;
        while end < characters.len()
            && style_at(styles, end) == style
            && detected_links
                .iter()
                .find(|l| l.char_range.contains(&end))
                .map(|l| &l.link_type)
                == link_kind
            && find_at(end) == find_state
        {
            end += 1;
        }
        let byte_start = characters[start].0;
        let byte_end = characters.get(end).map_or(text.len(), |(index, _)| *index);

        let font = styled_font(base_font.clone(), &style);
        let mut color = style.foreground.map(color_from_rgba).unwrap_or(base_color);
        let mut background = style.background.map(color_from_rgba);
        let mut underline = style.underline;

        if let Some(link) = link_kind {
            match link {
                super::links::LinkType::Web { .. } => {
                    if style.foreground.is_none() {
                        color = web_color;
                    }
                    underline = true;
                }
                super::links::LinkType::FilePath { .. } => {
                    if style.foreground.is_none() {
                        color = file_color;
                    }
                    underline = true;
                }
            }
        }

        if let Some(is_active) = find_state {
            if is_active {
                background = Some(Color32::from_rgb(255, 115, 0));
                color = Color32::WHITE;
            } else {
                background = Some(Color32::from_rgb(250, 204, 21));
                color = Color32::from_rgb(18, 18, 18);
            }
        }

        let mut format = TextFormat::simple(font, color);
        format.italics = style.italic;
        if underline {
            let underline_stroke_color = match link_kind {
                Some(super::links::LinkType::Web { .. }) => web_color,
                Some(super::links::LinkType::FilePath { .. }) => file_color,
                _ => color,
            };
            format.underline = Stroke::new(1.1, underline_stroke_color);
        }
        if style.strikethrough {
            format.strikethrough = Stroke::new(1.0, color);
        }
        if let Some(bg) = background {
            format.background = bg;
        }
        job.append(&text[byte_start..byte_end], 0.0, format);
        start = end;
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn append_text_with_find(
    job: &mut egui::text::LayoutJob,
    text: &str,
    font_id: FontId,
    base_color: Color32,
    underline: Stroke,
    find_query: Option<&str>,
    is_active: bool,
) {
    let query_clean = find_query.map(|q| q.trim()).unwrap_or("");
    let find_ranges = if !query_clean.is_empty() {
        super::find::find_char_ranges(text, query_clean)
    } else {
        Vec::new()
    };
    if find_ranges.is_empty() {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id,
                color: base_color,
                underline,
                ..Default::default()
            },
        );
        return;
    }
    let characters: Vec<_> = text.char_indices().collect();
    let find_at = |i: usize| -> bool { find_ranges.iter().any(|r| r.contains(&i)) };
    let mut start = 0;
    while start < characters.len() {
        let matched = find_at(start);
        let mut end = start + 1;
        while end < characters.len() && find_at(end) == matched {
            end += 1;
        }
        let byte_start = characters[start].0;
        let byte_end = characters.get(end).map_or(text.len(), |(index, _)| *index);

        let (color, background) = if matched {
            if is_active {
                (Color32::WHITE, Color32::from_rgb(255, 115, 0))
            } else {
                (
                    Color32::from_rgb(18, 18, 18),
                    Color32::from_rgb(250, 204, 21),
                )
            }
        } else {
            (base_color, Color32::TRANSPARENT)
        };

        job.append(
            &text[byte_start..byte_end],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color,
                background,
                underline,
                ..Default::default()
            },
        );
        start = end;
    }
}

pub fn layout_title_with_find(
    ui: &Ui,
    text: &str,
    wrap_width: f32,
    base_font: FontId,
    base_color: egui::Color32,
    find_query: &str,
    is_active: bool,
) -> Arc<egui::Galley> {
    let characters: Vec<_> = text.char_indices().collect();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let query_clean = find_query.trim();
    let find_ranges = if !query_clean.is_empty() {
        super::find::find_char_ranges(text, query_clean)
    } else {
        Vec::new()
    };

    let find_at = |i: usize| -> bool { find_ranges.iter().any(|r| r.contains(&i)) };

    let mut start = 0;
    while start < characters.len() {
        let matched = find_at(start);
        let mut end = start + 1;
        while end < characters.len() && find_at(end) == matched {
            end += 1;
        }
        let byte_start = characters[start].0;
        let byte_end = characters.get(end).map_or(text.len(), |(index, _)| *index);

        let font = base_font.clone();
        let mut color = base_color;
        let mut background = None;

        if matched {
            if is_active {
                background = Some(Color32::from_rgb(255, 115, 0));
                color = Color32::WHITE;
            } else {
                background = Some(Color32::from_rgb(250, 204, 21));
                color = Color32::from_rgb(18, 18, 18);
            }
        }

        let mut format = TextFormat::simple(font, color);
        if let Some(bg) = background {
            format.background = bg;
        }
        job.append(&text[byte_start..byte_end], 0.0, format);
        start = end;
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

fn styled_font(mut font: FontId, style: &InlineTextStyle) -> FontId {
    font.family = match (style.font_family, style.bold) {
        (Some(InlineFontFamily::Monospace), _) => FontFamily::Monospace,
        (Some(InlineFontFamily::Serif), true) => FontFamily::Name("noteon-serif-bold".into()),
        (Some(InlineFontFamily::Serif), false) => FontFamily::Name("noteon-serif".into()),
        (None, true) => FontFamily::Name("noteon-bold".into()),
        _ => font.family,
    };
    font
}

fn show_text_format_toolbar(
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
                ui.menu_button("Font", |ui| {
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
                ui.menu_button(RichText::new("A").strong(), |ui| {
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
                ui.menu_button("Highlight", |ui| {
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

fn adjust_text_styles(old: &str, new: &str, styles: &mut Vec<TextStyleRange>) {
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

fn split_text_styles(
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

fn style_at(styles: &[TextStyleRange], character_index: usize) -> InlineTextStyle {
    styles
        .iter()
        .find(|range| range.start <= character_index && character_index < range.end)
        .map_or_else(InlineTextStyle::default, |range| range.style.clone())
}

fn color_from_rgba(rgba: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

#[derive(Clone)]
struct MultiClickSelection {
    last_click_time: f64,
    last_click_position: Option<egui::Pos2>,
    click_count: u8,
    press_position: Option<egui::Pos2>,
    active: Option<ActiveSelection>,
}

impl Default for MultiClickSelection {
    fn default() -> Self {
        Self {
            last_click_time: f64::NEG_INFINITY,
            last_click_position: None,
            click_count: 0,
            press_position: None,
            active: None,
        }
    }
}

#[derive(Clone)]
struct ActiveSelection {
    mode: SelectionMode,
    unit: std::ops::Range<usize>,
}

#[derive(Clone, Copy)]
enum SelectionMode {
    Word,
    Line,
}

fn update_multi_click_selection(ui: &Ui, output: &mut egui::text_edit::TextEditOutput) {
    let response = &output.response;
    let selection_id = response.id.with("multi-click-selection");
    let mut selection = ui
        .ctx()
        .data(|data| data.get_temp::<MultiClickSelection>(selection_id))
        .unwrap_or_default();
    let (time, pressed, down, released, position) = ui.input(|input| {
        (
            input.time,
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_down(egui::PointerButton::Primary),
            input.pointer.button_released(egui::PointerButton::Primary),
            input.pointer.interact_pos(),
        )
    });
    let (max_click_delay, max_click_distance) = ui.ctx().options(|options| {
        (
            options.input_options.max_double_click_delay,
            options.input_options.max_click_dist,
        )
    });

    if pressed && response.rect.contains(position.unwrap_or_default()) {
        let position = position.unwrap_or_default();
        let is_following_click = selection.click_count > 0
            && time - selection.last_click_time <= max_click_delay
            && selection
                .last_click_position
                .is_some_and(|last| last.distance(position) <= max_click_distance);
        selection.click_count = if is_following_click {
            selection.click_count.saturating_add(1).min(3)
        } else {
            1
        };
        selection.press_position = Some(position);
        selection.active = match selection.click_count {
            2 => Some(ActiveSelection {
                mode: SelectionMode::Word,
                unit: selection_unit_range(
                    output.galley.text(),
                    output
                        .galley
                        .cursor_from_pos(position - output.galley_pos)
                        .ccursor
                        .index,
                    SelectionMode::Word,
                ),
            }),
            3 => Some(ActiveSelection {
                mode: SelectionMode::Line,
                unit: selection_unit_range(
                    output.galley.text(),
                    output
                        .galley
                        .cursor_from_pos(position - output.galley_pos)
                        .ccursor
                        .index,
                    SelectionMode::Line,
                ),
            }),
            _ => {
                selection.active = None;
                None
            }
        };
    }

    if down
        && let (Some(active), Some(position)) = (selection.active.clone(), position)
        && (response.hovered() || ui.ctx().is_being_dragged(response.id))
    {
        let text = output.galley.text();
        let cursor = output
            .galley
            .cursor_from_pos(position - output.galley_pos)
            .ccursor
            .index;
        let range = drag_selection_range(text, cursor, active);
        output
            .state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(range.start),
                egui::text::CCursor::new(range.end),
            )));
        output.state.clone().store(ui.ctx(), response.id);
    }

    if released {
        if response.clicked()
            && let Some(position) = selection.press_position
        {
            selection.last_click_time = time;
            selection.last_click_position = Some(position);
        } else if selection.active.is_some() {
            selection.click_count = 0;
            selection.last_click_time = f64::NEG_INFINITY;
            selection.last_click_position = None;
        }
        selection.active = None;
        selection.press_position = None;
    }

    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selection));
}

fn selection_unit_range(text: &str, cursor: usize, mode: SelectionMode) -> std::ops::Range<usize> {
    let character_count = text.chars().count();
    let cursor = cursor.min(character_count);
    if character_count == 0 {
        return 0..0;
    }

    match mode {
        SelectionMode::Line => {
            let start = text
                .chars()
                .take(cursor)
                .enumerate()
                .filter(|(_, character)| *character == '\n')
                .map(|(index, _)| index + 1)
                .last()
                .unwrap_or(0);
            let end = text
                .chars()
                .skip(cursor)
                .position(|character| character == '\n')
                .map_or(character_count, |offset| cursor + offset + 1);
            start..end
        }
        SelectionMode::Word => {
            let character_at = |index| text.chars().nth(index);
            let pivot = if cursor < character_count
                && character_at(cursor).is_some_and(is_word_character)
            {
                cursor
            } else if cursor > 0 && character_at(cursor - 1).is_some_and(is_word_character) {
                cursor - 1
            } else {
                cursor.min(character_count - 1)
            };
            let character_class = word_class(character_at(pivot).unwrap_or_default());
            let start = text
                .chars()
                .take(pivot)
                .enumerate()
                .filter(|(_, character)| word_class(*character) != character_class)
                .map(|(index, _)| index + 1)
                .last()
                .unwrap_or(0);
            let end = text
                .chars()
                .skip(pivot + 1)
                .position(|character| word_class(character) != character_class)
                .map_or(character_count, |offset| pivot + 1 + offset);
            start..end
        }
    }
}

fn drag_selection_range(
    text: &str,
    cursor: usize,
    active: ActiveSelection,
) -> std::ops::Range<usize> {
    if cursor < active.unit.start {
        let current = selection_unit_range(text, cursor, active.mode);
        current.start..active.unit.start
    } else if cursor >= active.unit.end {
        let current = selection_unit_range(text, cursor, active.mode);
        active.unit.start..current.end
    } else {
        active.unit
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn word_class(character: char) -> u8 {
    if is_word_character(character) {
        0
    } else if character.is_whitespace() {
        1
    } else {
        2
    }
}

fn block_widget_id(name: &'static str, block_id: i64, widget_scope: Option<u64>) -> egui::Id {
    widget_scope.map_or_else(
        || egui::Id::new((name, block_id)),
        |scope| egui::Id::new((name, scope, block_id)),
    )
}

fn show_command_suggestions(
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

fn slash_query(content: &str) -> Option<&str> {
    let query = content.trim_start();
    if !query.starts_with('/') || query.chars().any(char::is_whitespace) {
        return None;
    }
    Some(query)
}

fn matching_commands(content: &str) -> Vec<(&'static str, SlashCommandTarget)> {
    let Some(query) = slash_query(content) else {
        return Vec::new();
    };
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|(command, _)| command.starts_with(query))
        .collect()
}

fn handle_enter(
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
    use super::{
        ActiveSelection, SelectionMode, SlashCommandTarget, drag_selection_range,
        matching_commands, selection_unit_range, slash_query,
    };
    use crate::domain::model::BlockKind;

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
    fn parse_file_link_recognizes_paths_and_markdown() {
        assert_eq!(
            super::parse_file_link(r"C:\Users\kargalar\Documents\note.txt"),
            Some((
                r"C:\Users\kargalar\Documents\note.txt".to_string(),
                "note.txt".to_string()
            ))
        );
        assert_eq!(
            super::parse_file_link(r#""C:\Program Files\App\app.exe""#),
            Some((
                r"C:\Program Files\App\app.exe".to_string(),
                "app.exe".to_string()
            ))
        );
        assert_eq!(
            super::parse_file_link(r"📁 C:\Users\kargalar\Downloads\file.zip"),
            Some((
                r"C:\Users\kargalar\Downloads\file.zip".to_string(),
                "file.zip".to_string()
            ))
        );
        assert_eq!(
            super::parse_file_link(r"[My PDF](C:\Users\kargalar\report.pdf)"),
            Some((
                r"C:\Users\kargalar\report.pdf".to_string(),
                "My PDF".to_string()
            ))
        );
        assert_eq!(super::parse_file_link("regular text without paths"), None);
    }

    #[test]
    fn suggestions_close_after_whitespace_or_without_a_leading_slash() {
        assert_eq!(slash_query(" /bul"), Some("/bul"));
        assert_eq!(slash_query("/bullet "), None);
        assert_eq!(slash_query("text /bullet"), None);
    }

    #[test]
    fn word_selection_uses_unicode_character_boundaries() {
        let text = "Hello, dünya!";
        assert_eq!(selection_unit_range(text, 9, SelectionMode::Word), 7..12);
    }

    #[test]
    fn line_selection_includes_the_line_break() {
        assert_eq!(
            selection_unit_range("first\nsecond", 8, SelectionMode::Line),
            6..12
        );
    }

    #[test]
    fn dragging_a_word_selection_expands_by_whole_words() {
        let active = ActiveSelection {
            mode: SelectionMode::Word,
            unit: 6..10,
        };
        assert_eq!(
            drag_selection_range("alpha beta gamma", 13, active.clone()),
            6..16
        );
        assert_eq!(drag_selection_range("alpha beta gamma", 1, active), 0..6);
    }
}
