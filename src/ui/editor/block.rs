mod clipboard;
mod commands;
mod menu;
mod text_format;
mod text_input;
mod text_layout;
mod text_selection;

use crate::ui::menus::PersistentContextMenu;
use eframe::egui::{
    self, Align2, FontFamily, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};

use crate::domain::model::{Block, BlockId, BlockKind};
use crate::worker::Action;

use crate::ui::{Intent, palette::Palette};

use super::find::FindHighlight;
use menu::show_menu;
use text_input::{TextInputOptions, show_text};
pub use text_layout::layout_title_with_find;
use text_layout::{append_text_with_find, layout_styled_text};

pub const BLOCK_PADDING: f32 = 12.0;
pub const HANDLE_WIDTH: f32 = 30.0;
pub const DATABASE_BLOCK_HEIGHT: f32 = 520.0;
pub const MEDIA_BLOCK_HEIGHT: f32 = 320.0;
pub const PAGE_LINK_BLOCK_HEIGHT: f32 = 48.0;

pub fn parse_file_link(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with('[')
        && trimmed.contains("](")
        && trimmed.ends_with(')')
        && let (Some(b_end), Some(p_start)) = (trimmed.find(']'), trimmed.find("]("))
    {
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
    if block.kind == BlockKind::PageLink {
        return PAGE_LINK_BLOCK_HEIGHT;
    }
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
        FindHighlight::default(),
    );
    (galley.size().y + BLOCK_PADDING * 2.0).max(36.0)
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
    linked_page: Option<&crate::domain::model::Page>,
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
    if block.kind == BlockKind::PageLink {
        if let Ok(page_id) = block.content.parse::<crate::domain::model::PageId>() {
            let body = Rect::from_min_max(
                rect.min + vec2(HANDLE_WIDTH, 4.0),
                rect.max - vec2(4.0, 4.0),
            );
            let response = ui.interact(
                body,
                block_widget_id("page-reference", block.id, widget_scope),
                Sense::click(),
            );
            if response.hovered() {
                ui.painter().rect_filled(body, 4.0, palette.hover);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let title = linked_page
                .map(|page| page.title.as_str())
                .filter(|title| !title.is_empty())
                .unwrap_or("Untitled");
            let icon = linked_page
                .map(|page| page.icon.as_str())
                .filter(|icon| !icon.is_empty())
                .unwrap_or("↗");
            let galley = ui.painter().layout(
                format!("{icon}  {title}"),
                FontId::proportional(15.0),
                palette.text,
                (body.width() - 16.0).max(1.0),
            );
            ui.painter()
                .with_clip_rect(body.intersect(ui.clip_rect()))
                .galley(body.min + vec2(8.0, 8.0), galley, palette.text);
            if response.clicked() || response.middle_clicked() {
                intents.push(if response.middle_clicked() {
                    Intent::OpenInTab(page_id)
                } else {
                    Intent::Open(page_id, None)
                });
            }
            response.persistent_context_menu(|ui| {
                if ui.button("Open page").clicked() {
                    intents.push(Intent::Open(page_id, None));
                    ui.close_menu();
                }
                if ui.button("Open in new tab").clicked() {
                    intents.push(Intent::OpenInTab(page_id));
                    ui.close_menu();
                }
            });
        }
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
            Stroke::new(1.0_f32, palette.border),
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
                    Stroke::new(1.0_f32, palette.muted)
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

            link_resp.persistent_context_menu(|ui| {
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
                    Stroke::new(1.0_f32, palette.muted)
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

            link_resp.persistent_context_menu(|ui| {
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
                block,
                TextInputOptions {
                    rect: text_rect,
                    focus: text_focus,
                    focus_at_end,
                    palette,
                    widget_scope,
                    highlight: FindHighlight {
                        query: find_query,
                        active_match: find_active_match,
                    },
                },
                intents,
            );

            if is_page_link || is_file_link {
                let editor_id = block_widget_id("block", block.id, widget_scope);
                if just_started
                    && let Some(mut state) =
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
            Stroke::new(1.5_f32, grip_color),
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
            Stroke::new(3.0_f32, palette.accent),
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

fn block_widget_id(name: &'static str, block_id: i64, widget_scope: Option<u64>) -> egui::Id {
    widget_scope.map_or_else(
        || egui::Id::new((name, block_id)),
        |scope| egui::Id::new((name, scope, block_id)),
    )
}

#[cfg(test)]
mod tests {
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
}
