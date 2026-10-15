use eframe::egui::{
    self, Align2, FontFamily, FontId, Key, KeyboardShortcut, Modifiers, Rect, RichText, Sense,
    Stroke, Ui, pos2, vec2,
};

use crate::domain::editing::split_at_selection;
use crate::domain::model::{Block, BlockKind, DropPlacement};
use crate::worker::Action;

use crate::ui::text;
use crate::ui::{Intent, palette::Palette};

pub const BLOCK_PADDING: f32 = 12.0;
pub const MAX_BLOCK_HEIGHT: f32 = 260.0;
pub const HANDLE_WIDTH: f32 = 30.0;
pub const DATABASE_BLOCK_HEIGHT: f32 = 520.0;
pub const MEDIA_BLOCK_HEIGHT: f32 = 320.0;

const SLASH_COMMANDS: [(&str, BlockKind); 12] = [
    ("/h1", BlockKind::Heading1),
    ("/h2", BlockKind::Heading2),
    ("/h3", BlockKind::Heading3),
    ("/todo", BlockKind::Todo),
    ("/bullet", BlockKind::Bullet),
    ("/number", BlockKind::Numbered),
    ("/quote", BlockKind::Quote),
    ("/callout", BlockKind::Callout),
    ("/code", BlockKind::Code),
    ("/divider", BlockKind::Divider),
    ("/text", BlockKind::Text),
    ("/table", BlockKind::Database),
];

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
    let text = if block.content.is_empty() {
        " "
    } else {
        &block.content
    };
    let galley = ui.fonts(|fonts| {
        fonts.layout(
            text.into(),
            font(block.kind),
            egui::Color32::WHITE,
            text_width,
        )
    });
    (galley.size().y + BLOCK_PADDING * 2.0).clamp(36.0, MAX_BLOCK_HEIGHT)
}

pub fn show(
    ui: &mut Ui,
    rect: Rect,
    block: &mut Block,
    focus: bool,
    palette: Palette,
    widget_scope: Option<u64>,
    intents: &mut Vec<Intent>,
) -> bool {
    let mut changed = show_menu(ui, rect, block, palette, widget_scope, intents);
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
        changed |= show_decoration(ui, body, block, palette);
        let text_rect = Rect::from_min_max(
            body.min + vec2(text_indent(block.kind), BLOCK_PADDING - 4.0),
            body.max - vec2(8.0, 5.0),
        );
        changed |= show_text(ui, text_rect, block, focus, palette, widget_scope, intents);
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

fn show_menu(
    ui: &mut Ui,
    rect: Rect,
    block: &mut Block,
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
    let mut changed = false;
    let mut kind_change = None;
    let handle = ui.put(
        handle_rect,
        egui::Button::new(RichText::new("⠿").color(if row_response.hovered() {
            palette.muted
        } else {
            egui::Color32::TRANSPARENT
        }))
        .frame(false)
        .sense(Sense::click_and_drag()),
    );
    let handle = handle.on_hover_text("Block menu");
    if handle.drag_started() {
        handle.dnd_set_drag_payload((block.page_id, block.id));
    }
    if let Some(payload) = row_response.dnd_hover_payload::<(i64, i64)>()
        && payload.0 == block.page_id
        && payload.1 != block.id
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
        if let Some(payload) = row_response.dnd_release_payload::<(i64, i64)>() {
            let (page, source) = *payload;
            intents.push(Intent::Action(Action::ArrangeBlock {
                page,
                source,
                target: block.id,
                placement,
            }));
        }
    }
    let popup_id = block_widget_id("block-menu", block.id, widget_scope);
    let right_clicked_on_row = ui.input(|input| {
        input.pointer.button_clicked(egui::PointerButton::Secondary)
            && input
                .pointer
                .interact_pos()
                .is_some_and(|position| rect.contains(position))
    });
    if handle.clicked() || handle.secondary_clicked() || right_clicked_on_row {
        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
    }
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &handle,
        egui::popup::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(190.0);
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
            if ui.button("↑ Move up").clicked() {
                intents.push(Intent::Action(Action::MoveBlock {
                    page: block.page_id,
                    block: block.id,
                    direction: -1,
                }));
                ui.close_menu();
            }
            if ui.button("↓ Move down").clicked() {
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
    palette: Palette,
    widget_scope: Option<u64>,
    intents: &mut Vec<Intent>,
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
                    let mut output = egui::TextEdit::multiline(&mut block.content)
                        .id(block_widget_id("block", block.id, widget_scope))
                        .font(font(block.kind))
                        .text_color(if block.checked && block.kind == BlockKind::Todo {
                            palette.muted
                        } else {
                            palette.text
                        })
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
                        .lock_focus(block.kind == BlockKind::Code)
                        .show(ui);
                    update_multi_click_selection(ui, &mut output);
                    if focus {
                        output.response.request_focus();
                        let cursor = egui::text::CCursor::new(0);
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
                        output.state.store(ui.ctx(), output.response.id);
                    }
                    focused = output.response.has_focus();
                    changed |= output.response.changed();
                    let selected_kind = show_command_suggestions(
                        ui,
                        output.response.rect,
                        block,
                        focused,
                        output.response.changed(),
                        widget_scope,
                    );
                    if let Some(kind) = selected_kind {
                        if kind == BlockKind::Database {
                            intents.push(Intent::Action(Action::CreateDatabaseBlock {
                                page: block.page_id,
                                position: block.position,
                                replace_block: Some(block.id),
                            }));
                        }
                        block.kind = kind;
                        block.content.clear();
                        output.response.request_focus();
                        changed = true;
                    } else {
                        changed |= handle_enter(
                            ui,
                            block,
                            output
                                .cursor_range
                                .map(|range| range.as_sorted_char_range()),
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
                    }
                });
        },
    );
    changed
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
) -> Option<BlockKind> {
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
                for (index, (command, kind)) in matches.iter().enumerate() {
                    let label = if *kind == BlockKind::Database {
                        "Table"
                    } else {
                        kind.label()
                    };
                    if ui
                        .selectable_label(index == selected, format!("{command}   {label}"))
                        .clicked()
                    {
                        activate = Some(*kind);
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

fn matching_commands(content: &str) -> Vec<(&'static str, BlockKind)> {
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
    if !focused || block.kind == BlockKind::Code || !consume_plain_enter(ui) {
        return false;
    }
    if let Some(kind) = BlockKind::shortcut(&block.content) {
        block.kind = kind;
        block.content.clear();
        return true;
    }
    let selection = selection.unwrap_or_else(|| {
        let end = block.content.chars().count();
        end..end
    });
    let (prefix, remainder) = split_at_selection(&block.content, selection);
    block.content = prefix;
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
    }));
    true
}

fn consume_plain_enter(ui: &mut Ui) -> bool {
    ui.input_mut(|input| {
        let plain_enter = input.events.iter().any(|event| matches!(event,
            egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.is_none()));
        plain_enter && input.consume_key(Modifiers::NONE, Key::Enter)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveSelection, SelectionMode, drag_selection_range, matching_commands,
        selection_unit_range, slash_query,
    };
    use crate::domain::model::BlockKind;

    #[test]
    fn slash_command_matches_narrow_as_the_query_grows() {
        let headings = matching_commands("/h");
        assert_eq!(headings.len(), 3);
        assert_eq!(matching_commands("/todo"), [("/todo", BlockKind::Todo)]);
        assert_eq!(
            matching_commands("/table"),
            [("/table", BlockKind::Database)]
        );
        assert!(matching_commands("/unknown").is_empty());
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
