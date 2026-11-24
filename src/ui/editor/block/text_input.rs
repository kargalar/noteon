use crate::ui::editor::find::FindHighlight;
use crate::ui::menus::PersistentContextMenu;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Rect, RichText, Ui};

use crate::domain::model::{Block, BlockKind};
use crate::worker::Action;

use crate::ui::{Intent, ThemeSection, palette::Palette};

use super::clipboard::handle_paste;
use super::commands::{SlashCommandTarget, handle_enter, show_command_suggestions};
use super::text_format::{adjust_text_styles, show_text_format_toolbar};
use super::text_layout::layout_styled_text;
use super::text_selection::update_multi_click_selection;
use super::{block_widget_id, font};

pub(super) struct TextInputOptions<'a> {
    pub(super) rect: Rect,
    pub(super) focus: bool,
    pub(super) focus_at_end: bool,
    pub(super) palette: Palette,
    pub(super) widget_scope: Option<u64>,
    pub(super) highlight: FindHighlight<'a>,
}

pub(super) fn show_text(
    ui: &mut Ui,
    block: &mut Block,
    options: TextInputOptions<'_>,
    intents: &mut Vec<Intent>,
) -> bool {
    let TextInputOptions {
        rect: text_rect,
        focus,
        focus_at_end,
        palette,
        widget_scope,
        highlight,
    } = options;
    let mut changed = false;
    let mut focused = false;
    let hint = if block.kind == BlockKind::Text {
        RichText::new("")
    } else {
        RichText::new(block.kind.label())
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(("block-content", block.id))
            .max_rect(text_rect),
        |ui| {
            ui.set_clip_rect(text_rect.intersect(ui.clip_rect()));
            ui.scope(|ui| {
                    let editor_id = block_widget_id("block", block.id, widget_scope);
                    let was_focused = ui.ctx().memory(|m| m.has_focus(editor_id));
                    if (was_focused || focus)
                        && handle_paste(ui, block, editor_id, intents)
                    {
                        changed = true;
                        return;
                    }
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
                            highlight,
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

                    let detected_links = crate::ui::editor::links::find_links_and_paths(&block.content);
                    if !detected_links.is_empty() {
                        let pointer_pos =
                            ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
                        if let Some(pos) = pointer_pos
                            && output.response.rect.contains(pos) {
                                let relative_pos = pos - output.response.rect.min;
                                let cursor = output.galley.cursor_from_pos(relative_pos);
                                let char_index = cursor.ccursor.index;
                                let char_rect = output
                                    .galley
                                    .pos_from_cursor(&cursor)
                                    .translate(output.response.rect.min.to_vec2())
                                    .expand(3.0);

                                if char_rect.contains(pos)
                                    && let Some(link) = detected_links
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
                                                    crate::ui::editor::links::LinkType::Web { url } => {
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
                                                    crate::ui::editor::links::LinkType::FilePath { path } => {
                                                        let name = crate::ui::editor::links::extract_file_or_folder_name(path);
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
                                                crate::ui::editor::links::LinkType::Web { url } => {
                                                    intents.push(Intent::OpenUrl(url.clone()));
                                                }
                                                crate::ui::editor::links::LinkType::FilePath { path } => {
                                                    intents.push(Intent::OpenFileExplorer(
                                                        path.clone(),
                                                    ));
                                                }
                                            }
                                        }
                                    }
                            }
                    }

                    output.response.persistent_context_menu(|ui| {
                        ui.set_min_width(200.0);
                        let context_links = crate::ui::editor::links::find_links_and_paths(&block.content);
                        for link in &context_links {
                            match &link.link_type {
                                crate::ui::editor::links::LinkType::Web { url } => {
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
                                crate::ui::editor::links::LinkType::FilePath { path } => {
                                    let name = crate::ui::editor::links::extract_file_or_folder_name(path);
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
                                let any_popup = ui.ctx().memory(|m| m.any_popup_open()) || crate::ui::menus::pointer_in_menu(ui.ctx());
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
                            && let Some(cursor_range) = state.cursor.char_range() {
                                state.cursor.set_char_range(Some(
                                    egui::text::CCursorRange::one(cursor_range.primary),
                                ));
                                state.store(ui.ctx(), output.response.id);
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
