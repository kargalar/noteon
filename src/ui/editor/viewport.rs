use super::{DatabaseContext, Editor, block, header::Canvas};
use crate::domain::model::{BlockId, BlockKind, BulkBlockStyleOp, InlineFontFamily, PageId};
use crate::domain::virtual_layout::VirtualLayout;
use crate::ui::{Intent, palette::Palette};
use crate::worker::Action;
use eframe::egui::{self, Color32, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

struct RowDrawContext<'a, 'database> {
    viewport: Rect,
    canvas: Canvas,
    palette: Palette,
    generation: u64,
    databases: &'a mut DatabaseContext<'database>,
    intents: &'a mut Vec<Intent>,
}

impl Editor {
    pub(super) fn show_blocks(
        &mut self,
        ui: &mut Ui,
        canvas: Canvas,
        palette: Palette,
        context: &mut DatabaseContext<'_>,
    ) -> Vec<Intent> {
        let Some((page_id, generation)) = self
            .document
            .as_ref()
            .map(|document| (document.page.id, self.generation))
        else {
            return Vec::new();
        };
        self.prepare_layout(canvas.document_width);
        self.measure_changed_blocks(ui, canvas.document_width);
        let scroll = self.scroll_area(page_id);
        let mut intents = Vec::new();
        self.rendered_blocks = 0;
        let output = scroll.show_viewport(ui, |ui, viewport| {
            self.draw_rows(
                ui,
                &mut RowDrawContext {
                    viewport,
                    canvas,
                    palette,
                    generation,
                    databases: context,
                    intents: &mut intents,
                },
            )
        });
        self.scroll_offset = output.state.offset.y;
        let any_block_changed = intents
            .iter()
            .any(|intent| matches!(intent, Intent::BlockChanged(_) | Intent::Action(_)));
        if !any_block_changed {
            let redo_pressed = ui.input_mut(|i| {
                i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::COMMAND, Key::Y)
                    || i.consume_key(Modifiers::CTRL, Key::Y)
            });
            let undo_pressed = ui.input_mut(|i| {
                i.consume_key(Modifiers::COMMAND, Key::Z)
                    || i.consume_key(Modifiers::CTRL, Key::Z)
            });
            if redo_pressed {
                intents.push(Intent::Redo);
            } else if undo_pressed {
                intents.push(Intent::Undo);
            }
        }
        intents
    }

    fn prepare_layout(&mut self, document_width: f32) {
        let Some(document) = &self.document else {
            return;
        };
        if (self.last_width - document_width).abs() > 1.0 {
            self.visual_rows.clear();
            self.position_to_row = vec![0; document.outline.len()];
            let mut row_heights = Vec::new();
            let mut position = 0;
            while position < document.outline.len() {
                let stub = &document.outline[position];
                let mut row = vec![position];
                if let Some(group) = stub.column_group {
                    while let Some(next) = document.outline.get(position + row.len()) {
                        if next.column_group != Some(group) {
                            break;
                        }
                        row.push(position + row.len());
                    }
                }
                let row_index = row_heights.len();
                self.visual_rows.push(row.clone());
                self.position_to_row[position] = row_index;
                let widths = row_column_widths(&row, &document.outline, document_width);
                let mut height: f32 = 0.0;
                for (column, row_position) in row.iter().enumerate() {
                    let block_stub = &document.outline[*row_position];
                    height = height.max(estimate_height(block_stub, widths[column]));
                    self.position_to_row[*row_position] = row_index;
                }
                row_heights.push(height);
                position += row.len();
            }
            self.layout = VirtualLayout::new(row_heights);
            self.last_width = document_width;
            self.cache
                .needs_measure
                .extend(self.cache.blocks.keys().copied());
        }
    }

    fn measure_changed_blocks(&mut self, ui: &Ui, document_width: f32) {
        // Measure once per load/edit/resize, never walk all resident text each frame.
        let anchor = self.layout.row_at(self.scroll_offset);
        let within_row = self.scroll_offset - self.layout.top(anchor);
        let mut measured = false;
        for position in self.cache.needs_measure.drain() {
            if position < self.position_to_row.len()
                && let Some(block) = self.cache.blocks.get(&position)
            {
                let row_index = self.position_to_row[position];
                let row = &self.visual_rows[row_index];
                let widths = self
                    .document
                    .as_ref()
                    .map(|doc| row_column_widths(row, &doc.outline, document_width))
                    .unwrap_or_default();
                let column = row
                    .iter()
                    .position(|row_position| *row_position == position)
                    .unwrap_or(0);
                let width = widths.get(column).copied().unwrap_or(document_width);
                let mut height = block::measured_height(ui, block, width);
                for other in row.iter().copied().filter(|other| *other != position) {
                    let other_column = row
                        .iter()
                        .position(|row_position| *row_position == other)
                        .unwrap_or(0);
                    let other_width = widths.get(other_column).copied().unwrap_or(document_width);
                    if let Some(other_block) = self.cache.blocks.get(&other) {
                        height = height.max(block::measured_height(ui, other_block, other_width));
                    } else if let Some(stub) = self
                        .document
                        .as_ref()
                        .and_then(|doc| doc.outline.get(other))
                    {
                        height = height.max(estimate_height(stub, other_width));
                    }
                }
                self.layout.set(row_index, height);
                measured = true;
            }
        }
        if measured && !self.layout.is_empty() {
            self.scroll_offset = self.layout.top(anchor) + within_row;
            self.reset_scroll = true;
        }
    }

    fn scroll_area(&mut self, page_id: PageId) -> egui::ScrollArea {
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt(("document", page_id))
            .drag_to_scroll(false)
            .auto_shrink([false, false]);
        if let Some(position) = self.jump_to {
            let row = self
                .position_to_row
                .get(position)
                .copied()
                .unwrap_or(position);
            scroll = scroll.vertical_scroll_offset(self.layout.top(row));
            if self.cache.blocks.contains_key(&position) {
                self.jump_to = None;
            }
        } else if self.reset_scroll {
            scroll = scroll.vertical_scroll_offset(self.scroll_offset);
            self.reset_scroll = false;
        }
        scroll
    }

    fn draw_rows(&mut self, ui: &mut Ui, context: &mut RowDrawContext<'_, '_>) {
        let viewport = context.viewport;
        let canvas = context.canvas;
        let palette = context.palette;
        let generation = context.generation;
        let databases = &mut *context.databases;
        let intents = &mut *context.intents;
        let Canvas {
            width,
            document_width,
            inset,
            widget_scope,
        } = canvas;
        let Some(document) = &self.document else {
            return;
        };
        let page_id = document.page.id;
        let mut touched = Vec::new();
        let origin = ui.cursor().min + vec2(inset, 0.0);
        ui.set_min_height(self.layout.total() + 65.0);
        ui.set_min_width(width - 14.0);
        let start = if self.layout.is_empty() {
            0
        } else {
            self.layout.row_at((viewport.min.y - 70.0).max(0.0))
        };
        let end = if self.layout.is_empty() {
            0
        } else {
            (self.layout.row_at(viewport.max.y + 90.0) + 1).min(self.layout.len())
        };
        let bulk_selection_active = !self.selected_blocks.is_empty() || self.marquee_active;
        let ordered_selected_blocks: Vec<BlockId> = if self.selected_blocks.is_empty() {
            Vec::new()
        } else {
            document
                .outline
                .iter()
                .map(|stub| stub.id)
                .filter(|id| self.selected_blocks.contains(id))
                .collect()
        };
        let mut visible_handles = Vec::new();
        let mut visible_blocks = Vec::new();
        for row_index in start..end {
            let row = self.visual_rows[row_index].clone();
            let top = self.layout.top(row_index);
            let rect = Rect::from_min_size(
                origin + vec2(0.0, top),
                vec2(document_width, self.layout.height(row_index)),
            );
            let mut ratios: Vec<_> = row
                .iter()
                .map(|position| document.outline[*position].column_width.max(0.08))
                .collect();
            if let Some(group) = document.outline[row[0]].column_group {
                for boundary in 0..row.len().saturating_sub(1) {
                    if let Some((left, right)) = self.column_resize_preview.get(&(group, boundary))
                    {
                        ratios[boundary] = *left;
                        ratios[boundary + 1] = *right;
                    }
                }
            }
            let widths = widths_for_ratios(&ratios, document_width);
            let mut x = rect.left();
            for (column, position) in row.iter().copied().enumerate() {
                let block_width = widths[column];
                let block_rect = Rect::from_min_size(
                    rect.min + vec2(x - rect.left(), 0.0),
                    vec2(block_width, rect.height()),
                );
                let block_id = document.outline[position].id;
                let is_selected = self.selected_blocks.contains(&block_id);
                visible_handles.push((
                    block_id,
                    Rect::from_min_size(block_rect.min, vec2(block::HANDLE_WIDTH - 3.0, 27.0)),
                ));
                visible_blocks.push((block_id, block_rect, position));
                if is_selected {
                    let highlight_rect = block_rect.shrink2(vec2(1.0, 1.0));
                    ui.painter().rect_filled(
                        highlight_rect,
                        6.0,
                        Color32::from_rgba_unmultiplied(
                            palette.accent.r(),
                            palette.accent.g(),
                            palette.accent.b(),
                            34,
                        ),
                    );
                    ui.painter().rect_stroke(
                        highlight_rect,
                        6.0,
                        Stroke::new(
                            1.0,
                            Color32::from_rgba_unmultiplied(
                                palette.accent.r(),
                                palette.accent.g(),
                                palette.accent.b(),
                                120,
                            ),
                        ),
                        egui::StrokeKind::Inside,
                    );
                }
                if let Some(block) = self.cache.blocks.get_mut(&position) {
                    self.rendered_blocks += 1;
                    let focus = self.focus_position == Some(position);
                    let page_link_icon = if block.kind == BlockKind::Text
                        && block.content.trim_start().starts_with('↗')
                    {
                        let target = block
                            .content
                            .trim_start()
                            .trim_start_matches("↗ ")
                            .trim_start_matches('↗')
                            .trim();
                        databases.find_page_icon(target, Some(block.page_id))
                    } else {
                        None
                    };
                    let find_active = self.find.active_range_for_block(block.id, position);
                    let find_q = if self.find.open {
                        Some(self.find.query.as_str())
                    } else {
                        None
                    };
                    if block::show(
                        ui,
                        block_rect,
                        block,
                        focus,
                        is_selected,
                        bulk_selection_active,
                        &ordered_selected_blocks,
                        palette,
                        widget_scope,
                        intents,
                        page_link_icon,
                        find_q,
                        find_active,
                    ) {
                        self.cache.needs_measure.insert(position);
                    }
                    if block.kind == BlockKind::Database
                        && let Ok(database_id) = block.content.parse::<PageId>()
                    {
                        databases.visible.insert(database_id);
                        if let Some(database) = databases.databases.get_mut(&database_id) {
                            let desired_h =
                                crate::ui::database::desired_embedded_height(ui, database);
                            if (self.layout.height(row_index) - desired_h).abs() > 1.0 {
                                self.layout.set(row_index, desired_h);
                            }
                            let database_rect = Rect::from_min_max(
                                block_rect.min + vec2(block::HANDLE_WIDTH, 0.0),
                                pos2(block_rect.max.x, block_rect.min.y + desired_h),
                            );
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(database_rect),
                                |ui| {
                                    ui.push_id(("embedded-database", block.id), |ui| {
                                        egui::Frame::NONE.show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.set_min_height((desired_h - 16.0).max(120.0));
                                            crate::ui::database::show_embedded(
                                                ui,
                                                database,
                                                palette,
                                                intents,
                                                &mut crate::ui::database::Drafts {
                                                    properties: databases.property_drafts,
                                                    titles: databases.title_drafts,
                                                    values: databases.value_drafts,
                                                },
                                            );
                                        });
                                    });
                                },
                            );
                        } else {
                            if !databases.loading.contains(&database_id) {
                                intents.push(Intent::LoadDatabase(database_id));
                            }
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(Rect::from_min_max(
                                    block_rect.min + vec2(block::HANDLE_WIDTH, 0.0),
                                    block_rect.max,
                                )),
                                |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(100.0);
                                        ui.spinner();
                                        ui.label("Loading database…");
                                    });
                                },
                            );
                        }
                    }
                    if focus {
                        self.focus_position = None;
                    }
                    touched.push(position);
                } else {
                    if let Some(load_start) = self.cache.request(position) {
                        intents.push(Intent::LoadBlocks {
                            page: page_id,
                            generation,
                            start: load_start,
                        });
                    }
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            block_rect.min + vec2(35.0, 12.0),
                            vec2(block_width * 0.55, 9.0),
                        ),
                        4.0,
                        palette.hover,
                    );
                }
                x += block_width;
                if column + 1 < row.len() {
                    let separator =
                        Rect::from_min_size(pos2(x, rect.top()), vec2(COLUMN_GAP, rect.height()));
                    let response = ui
                        .interact(
                            separator,
                            ui.id().with(("column-resize", row_index, column)),
                            Sense::drag(),
                        )
                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    ui.painter().rect_filled(
                        Rect::from_center_size(
                            separator.center(),
                            vec2(2.0, separator.height().min(42.0)),
                        ),
                        1.0,
                        if response.dragged() || response.hovered() {
                            palette.accent
                        } else {
                            palette.border
                        },
                    );
                    if response.dragged() || response.drag_stopped() {
                        let total = widths.iter().sum::<f32>().max(1.0);
                        let delta = response.drag_delta().x / total;
                        let pair_total = document.outline[position].column_width
                            + document.outline[row[column + 1]].column_width;
                        let minimum_column = (pair_total * 0.05).max(0.001);
                        let left = (document.outline[position].column_width + delta)
                            .clamp(minimum_column, pair_total - minimum_column);
                        let right = pair_total - left;
                        if let Some(group) = document.outline[position].column_group {
                            self.column_resize_preview
                                .insert((group, column), (left, right));
                            if response.drag_stopped() {
                                intents.push(Intent::Action(Action::ResizeColumns {
                                    page: page_id,
                                    group,
                                    boundary: column,
                                    left_width: left,
                                    right_width: right,
                                }));
                                self.column_resize_preview.remove(&(group, column));
                            }
                        }
                    }
                    x += COLUMN_GAP;
                }
            }
        }
        let append_area = Rect::from_min_size(
            origin + vec2(0.0, self.layout.total() + 5.0),
            vec2(document_width, 64.0),
        );
        let append_response = ui
            .interact(
                append_area,
                ui.id().with(("append-block", page_id, widget_scope)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Text);
        if append_response.clicked() && !self.marquee_active && self.selected_blocks.is_empty() {
            intents.push(Intent::Action(Action::InsertBlock {
                page: page_id,
                position: document.outline.len(),
                kind: BlockKind::Text,
                content: String::new(),
                text_styles: Vec::new(),
            }));
        }
        let dropped_files = ui.ctx().input_mut(|input| {
            if input
                .pointer
                .interact_pos()
                .is_some_and(|pointer| viewport.contains(pointer))
            {
                std::mem::take(&mut input.raw.dropped_files)
            } else {
                Vec::new()
            }
        });
        if let Some(pointer) = ui.ctx().pointer_interact_pos()
            && viewport.contains(pointer)
            && !dropped_files.is_empty()
        {
            let local_y = pointer.y - origin.y;
            let mut position = document.outline.len();
            for (row_index, row) in self.visual_rows.iter().enumerate() {
                let top = self.layout.top(row_index);
                let bottom = top + self.layout.height(row_index);
                if (top..=bottom).contains(&local_y) {
                    position = row[0] + usize::from(local_y > (top + bottom) * 0.5) * row.len();
                    break;
                }
            }
            for (offset, file) in dropped_files.into_iter().enumerate() {
                let Some(path) = file.path else {
                    continue;
                };
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        crate::domain::model::BlockKind::from_media_extension(extension).is_some()
                    })
                {
                    intents.push(Intent::Action(Action::ImportMedia {
                        page: page_id,
                        position: position + offset,
                        source: path,
                    }));
                } else {
                    intents.push(Intent::Action(Action::InsertBlock {
                        page: page_id,
                        position: position + offset,
                        kind: crate::domain::model::BlockKind::Text,
                        content: path.to_string_lossy().into_owned(),
                        text_styles: Vec::new(),
                    }));
                }
            }
        }
        for position in touched {
            self.cache.touch(position);
        }
        self.handle_marquee_and_bulk_selection(
            ui,
            origin,
            document_width,
            page_id,
            generation,
            widget_scope,
            palette,
            &visible_handles,
            &visible_blocks,
            intents,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_marquee_and_bulk_selection(
        &mut self,
        ui: &mut Ui,
        origin: Pos2,
        document_width: f32,
        page_id: PageId,
        generation: u64,
        widget_scope: Option<u64>,
        palette: Palette,
        visible_handles: &[(BlockId, Rect)],
        visible_blocks: &[(BlockId, Rect, usize)],
        intents: &mut Vec<Intent>,
    ) {
        let screen_viewport = ui.clip_rect();
        let margin_bounds = Rect::from_min_max(
            pos2(screen_viewport.min.x, (screen_viewport.min.y - 120.0).max(40.0)),
            screen_viewport.max,
        );
        let bulk_menu_id = egui::Id::new(("bulk-block-menu", page_id, widget_scope));
        let (
            primary_pressed,
            primary_down,
            primary_released,
            secondary_clicked,
            pointer_pos,
            is_ctrl,
            is_shift,
        ) = ui.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.button_clicked(egui::PointerButton::Secondary),
                input
                    .pointer
                    .interact_pos()
                    .or_else(|| input.pointer.latest_pos()),
                input.modifiers.command || input.modifiers.ctrl,
                input.modifiers.shift,
            )
        });

        let pointer_inside_bulk_menu = pointer_pos.is_some_and(|pos| {
            let inside_area = ui
                .ctx()
                .memory(|m| m.area_rect(bulk_menu_id))
                .is_some_and(|r| r.expand(6.0).contains(pos));
            let any_popup = ui.ctx().memory(|m| m.any_popup_open());
            let top_layer = ui.ctx().memory(|m| m.layer_id_at(pos));
            let inside_popup =
                any_popup && top_layer.is_some_and(|l| l.order == egui::Order::Foreground);
            inside_area || (self.bulk_menu_pos.is_some() && inside_popup)
        });

        if primary_pressed
            && let Some(pos) = pointer_pos
            && !pointer_inside_bulk_menu
        {
            self.bulk_menu_pos = None;
            let clicked_handle = visible_handles
                .iter()
                .find(|(_, rect)| rect.contains(pos))
                .map(|(id, _)| *id);
            let clicked_block = visible_blocks
                .iter()
                .find(|(_, rect, _)| rect.contains(pos))
                .map(|(id, _, pos_idx)| (*id, *pos_idx));

            if is_ctrl {
                if let Some(target_id) = clicked_handle.or_else(|| clicked_block.map(|(id, _)| id)) {
                    if self.selected_blocks.contains(&target_id) {
                        self.selected_blocks.remove(&target_id);
                    } else {
                        self.selected_blocks.insert(target_id);
                        self.anchor_block = Some(target_id);
                    }
                    if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                        ui.ctx().memory_mut(|m| m.surrender_focus(focused));
                    }
                }
            } else if is_shift {
                if let Some(target_id) = clicked_handle.or_else(|| clicked_block.map(|(id, _)| id)) {
                    if let Some(document) = &self.document {
                        let anchor_id = self
                            .anchor_block
                            .or_else(|| self.selected_blocks.iter().copied().next())
                            .unwrap_or(target_id);
                        let anchor_pos = document.outline.iter().position(|stub| stub.id == anchor_id);
                        let target_pos = document.outline.iter().position(|stub| stub.id == target_id);
                        if let (Some(p1), Some(p2)) = (anchor_pos, target_pos) {
                            let start = p1.min(p2);
                            let end = p1.max(p2);
                            self.selected_blocks.clear();
                            for p in start..=end {
                                let bid = document.outline[p].id;
                                self.selected_blocks.insert(bid);
                                if !self.cache.blocks.contains_key(&p) {
                                    if let Some(load_start) = self.cache.request(p) {
                                        intents.push(Intent::LoadBlocks {
                                            page: page_id,
                                            generation,
                                            start: load_start,
                                        });
                                    }
                                }
                            }
                        } else {
                            self.selected_blocks.clear();
                            self.selected_blocks.insert(target_id);
                            self.anchor_block = Some(target_id);
                        }
                    }
                    if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                        ui.ctx().memory_mut(|m| m.surrender_focus(focused));
                    }
                }
            } else if let Some(handle_id) = clicked_handle {
                if self.selected_blocks.contains(&handle_id) {
                    self.bulk_menu_pos = Some(pos);
                } else {
                    self.selected_blocks.clear();
                    self.selected_blocks.insert(handle_id);
                    self.anchor_block = Some(handle_id);
                    self.bulk_menu_pos = Some(pos);
                }
            } else if margin_bounds.contains(pos) {
                let on_selected_handle = !self.selected_blocks.is_empty()
                    && visible_handles
                        .iter()
                        .any(|(id, rect)| self.selected_blocks.contains(id) && rect.contains(pos));
                if on_selected_handle {
                    self.bulk_menu_pos = Some(pos);
                } else {
                    let on_any_handle = visible_handles.iter().any(|(_, rect)| rect.contains(pos));
                    let in_left_margin = pos.x < origin.x + block::HANDLE_WIDTH && !on_any_handle;
                    let in_right_margin = pos.x > origin.x + document_width - 4.0;
                    let in_vertical_margin = screen_viewport.contains(pos)
                        && (pos.y < origin.y || pos.y > origin.y + self.layout.total());
                    if in_left_margin || in_right_margin || in_vertical_margin {
                        self.marquee_anchor = Some(pos2(pos.x - origin.x, pos.y - origin.y));
                        self.marquee_active = false;
                        self.marquee_base_selection.clear();
                        self.selected_blocks.clear();
                    } else if clicked_block.is_none() {
                        self.marquee_anchor = None;
                        self.marquee_active = false;
                        self.selected_blocks.clear();
                    }
                }
            } else {
                self.marquee_anchor = None;
                self.marquee_active = false;
                self.selected_blocks.clear();
            }
        }

        if primary_down
            && let (Some(anchor_rel), Some(pos)) = (self.marquee_anchor, pointer_pos)
        {
            let anchor_screen = origin + anchor_rel.to_vec2();
            if !self.marquee_active && (pos - anchor_screen).length() > 4.0 {
                self.marquee_active = true;
                self.bulk_menu_pos = None;
            }
            if self.marquee_active {
                if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                    ui.ctx().memory_mut(|m| m.surrender_focus(focused));
                }
                let edge_zone = 32.0;
                if pos.y < screen_viewport.top() + edge_zone && self.scroll_offset > 0.0 {
                    let speed =
                        ((screen_viewport.top() + edge_zone - pos.y) * 0.45).clamp(2.0, 18.0);
                    ui.scroll_with_delta(vec2(0.0, speed));
                    ui.ctx().request_repaint();
                } else if pos.y > screen_viewport.bottom() - edge_zone {
                    let speed =
                        ((pos.y - (screen_viewport.bottom() - edge_zone)) * 0.45).clamp(2.0, 18.0);
                    ui.scroll_with_delta(vec2(0.0, -speed));
                    ui.ctx().request_repaint();
                }
                let selection_rect = Rect::from_two_pos(anchor_screen, pos);
                self.update_marquee_selection(
                    origin,
                    document_width,
                    selection_rect,
                    page_id,
                    generation,
                    intents,
                );
                let painter = ui
                    .ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new(("marquee-painter", page_id, widget_scope)),
                    ))
                    .with_clip_rect(margin_bounds);
                painter.rect_filled(
                    selection_rect,
                    3.0,
                    Color32::from_rgba_unmultiplied(
                        palette.accent.r(),
                        palette.accent.g(),
                        palette.accent.b(),
                        28,
                    ),
                );
                painter.rect_stroke(
                    selection_rect,
                    3.0,
                    Stroke::new(1.2, palette.accent),
                    egui::StrokeKind::Inside,
                );
            }
        }

        if primary_released {
            self.marquee_anchor = None;
            self.marquee_active = false;
            self.marquee_base_selection.clear();
        }

        if secondary_clicked
            && !self.selected_blocks.is_empty()
            && let Some(pos) = pointer_pos
            && screen_viewport.contains(pos)
        {
            self.bulk_menu_pos = Some(pos);
        }

        if !self.selected_blocks.is_empty() && !ui.ctx().wants_keyboard_input() {
            let ordered_ids = self.ordered_selected_block_ids();
            let mut clear_selection = false;
            ui.input_mut(|input| {
                if input.consume_key(Modifiers::NONE, Key::Escape) {
                    clear_selection = true;
                } else if input.consume_key(Modifiers::NONE, Key::Delete)
                    || input.consume_key(Modifiers::NONE, Key::Backspace)
                {
                    intents.push(Intent::Action(Action::DeleteBlocks {
                        page: page_id,
                        blocks: ordered_ids.clone(),
                    }));
                    clear_selection = true;
                } else if input.consume_key(Modifiers::COMMAND, Key::D)
                    || input.consume_key(Modifiers::CTRL, Key::D)
                {
                    intents.push(Intent::Action(Action::DuplicateBlocks {
                        page: page_id,
                        blocks: ordered_ids.clone(),
                    }));
                } else if input.consume_key(Modifiers::COMMAND, Key::C)
                    || input.consume_key(Modifiers::CTRL, Key::C)
                    || input.events.iter().any(|e| matches!(e, egui::Event::Copy))
                {
                    let copied = self.selected_blocks_plain_text();
                    if !copied.is_empty() {
                        ui.ctx().copy_text(copied);
                    }
                }
            });
            if clear_selection {
                self.selected_blocks.clear();
                self.bulk_menu_pos = None;
            }
        }

        if self.bulk_menu_pos.is_some() && !self.selected_blocks.is_empty() {
            self.show_bulk_context_menu(ui, bulk_menu_id, page_id, palette, intents);
        }
    }

    fn update_marquee_selection(
        &mut self,
        origin: Pos2,
        document_width: f32,
        selection_rect: Rect,
        page_id: PageId,
        generation: u64,
        intents: &mut Vec<Intent>,
    ) {
        let Some(document) = &self.document else {
            return;
        };
        let mut next_selected = self.marquee_base_selection.clone();
        let mut missing_positions = Vec::new();
        for (row_index, row) in self.visual_rows.iter().enumerate() {
            let top = origin.y + self.layout.top(row_index);
            let height = self.layout.height(row_index);
            if top + height < selection_rect.min.y || top > selection_rect.max.y {
                continue;
            }
            let mut ratios: Vec<_> = row
                .iter()
                .map(|position| document.outline[*position].column_width.max(0.08))
                .collect();
            if let Some(group) = document.outline[row[0]].column_group {
                for boundary in 0..row.len().saturating_sub(1) {
                    if let Some((left, right)) = self.column_resize_preview.get(&(group, boundary))
                    {
                        ratios[boundary] = *left;
                        ratios[boundary + 1] = *right;
                    }
                }
            }
            let widths = widths_for_ratios(&ratios, document_width);
            let mut x = origin.x;
            for (column, position) in row.iter().copied().enumerate() {
                let block_width = widths[column];
                let block_rect = Rect::from_min_size(pos2(x, top), vec2(block_width, height));
                if selection_rect.intersects(block_rect) {
                    next_selected.insert(document.outline[position].id);
                    if !self.cache.blocks.contains_key(&position) {
                        missing_positions.push(position);
                    }
                }
                x += block_width + COLUMN_GAP;
            }
        }
        self.selected_blocks = next_selected;
        for position in missing_positions {
            if let Some(load_start) = self.cache.request(position) {
                intents.push(Intent::LoadBlocks {
                    page: page_id,
                    generation,
                    start: load_start,
                });
            }
        }
    }

    fn ordered_selected_block_ids(&self) -> Vec<BlockId> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        document
            .outline
            .iter()
            .filter_map(|stub| {
                self.selected_blocks
                    .contains(&stub.id)
                    .then_some(stub.id)
            })
            .collect()
    }

    fn selected_blocks_plain_text(&self) -> String {
        let Some(document) = &self.document else {
            return String::new();
        };
        let mut lines = Vec::new();
        for (position, stub) in document.outline.iter().enumerate() {
            if self.selected_blocks.contains(&stub.id)
                && let Some(block) = self.cache.blocks.get(&position)
            {
                lines.push(block.content.as_str());
            }
        }
        lines.join("\n")
    }

    fn show_bulk_context_menu(
        &mut self,
        ui: &mut Ui,
        menu_id: egui::Id,
        page_id: PageId,
        palette: Palette,
        intents: &mut Vec<Intent>,
    ) {
        let Some(anchor_pos) = self.bulk_menu_pos else {
            return;
        };
        let ordered_ids = self.ordered_selected_block_ids();
        if ordered_ids.is_empty() {
            self.bulk_menu_pos = None;
            return;
        }
        let count = ordered_ids.len();
        let (has_todo, has_columns, can_group_columns) = self
            .document
            .as_ref()
            .map(|doc| {
                let selected_stubs: Vec<_> = doc
                    .outline
                    .iter()
                    .filter(|stub| self.selected_blocks.contains(&stub.id))
                    .collect();
                let todo = selected_stubs.iter().any(|s| s.kind == BlockKind::Todo);
                let cols = selected_stubs.iter().any(|s| s.column_group.is_some());
                let non_db = selected_stubs
                    .iter()
                    .filter(|s| s.kind != BlockKind::Database)
                    .count();
                (todo, cols, (2..=4).contains(&non_db))
            })
            .unwrap_or((false, false, false));

        let screen = ui.ctx().screen_rect();
        let clamped_pos = pos2(
            anchor_pos.x.clamp(screen.left() + 8.0, (screen.right() - 250.0).max(8.0)),
            anchor_pos.y.clamp(screen.top() + 8.0, (screen.bottom() - 320.0).max(8.0)),
        );
        let mut close_menu = false;
        let mut clear_selection = false;

        egui::Area::new(menu_id)
            .order(egui::Order::Foreground)
            .fixed_pos(clamped_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.label(
                        RichText::new(if count == 1 {
                            "1 BLOCK SELECTED".to_owned()
                        } else {
                            format!("{count} BLOCKS SELECTED")
                        })
                        .small()
                        .strong()
                        .color(palette.muted),
                    );
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        for (label, tooltip, op) in [
                            ("B", "Bold", BulkBlockStyleOp::ToggleBold),
                            ("I", "Italic", BulkBlockStyleOp::ToggleItalic),
                            ("U", "Underline", BulkBlockStyleOp::ToggleUnderline),
                            ("S", "Strikethrough", BulkBlockStyleOp::ToggleStrikethrough),
                        ] {
                            if ui.button(RichText::new(label).strong()).on_hover_text(tooltip).clicked() {
                                intents.push(Intent::Action(Action::FormatBlocks {
                                    page: page_id,
                                    blocks: ordered_ids.clone(),
                                    op,
                                }));
                                close_menu = true;
                            }
                        }
                        ui.menu_button("Font", |ui| {
                            for (label, family) in [
                                ("Default", None),
                                ("Serif", Some(InlineFontFamily::Serif)),
                                ("Monospace", Some(InlineFontFamily::Monospace)),
                            ] {
                                if ui.button(label).clicked() {
                                    intents.push(Intent::Action(Action::FormatBlocks {
                                        page: page_id,
                                        blocks: ordered_ids.clone(),
                                        op: BulkBlockStyleOp::SetFont(family),
                                    }));
                                    close_menu = true;
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button(RichText::new("A").strong(), |ui| {
                            ui.label(RichText::new("Text color").small().color(palette.muted));
                            for (name, color) in [
                                ("Default", None),
                                ("Accent", Some(palette.accent.to_array())),
                                ("Gray", Some(palette.muted.to_array())),
                                ("Red", Some(Color32::from_rgb(212, 76, 71).to_array())),
                                ("Orange", Some(Color32::from_rgb(217, 115, 13).to_array())),
                                ("Green", Some(Color32::from_rgb(68, 131, 97).to_array())),
                                ("Blue", Some(Color32::from_rgb(51, 126, 169).to_array())),
                                ("Purple", Some(Color32::from_rgb(144, 101, 176).to_array())),
                            ] {
                                let label = if let Some(rgba) = color {
                                    RichText::new(format!("●  {name}")).color(
                                        Color32::from_rgba_unmultiplied(
                                            rgba[0], rgba[1], rgba[2], rgba[3],
                                        ),
                                    )
                                } else {
                                    RichText::new(format!("○  {name}")).color(palette.text)
                                };
                                if ui.button(label).clicked() {
                                    intents.push(Intent::Action(Action::FormatBlocks {
                                        page: page_id,
                                        blocks: ordered_ids.clone(),
                                        op: BulkBlockStyleOp::SetTextColor(color),
                                    }));
                                    close_menu = true;
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Highlight", |ui| {
                            ui.label(RichText::new("Background highlight").small().color(palette.muted));
                            for (name, color) in [
                                ("None", None),
                                ("Yellow", Some(Color32::from_rgba_unmultiplied(255, 230, 120, 110).to_array())),
                                ("Green", Some(Color32::from_rgba_unmultiplied(120, 220, 150, 100).to_array())),
                                ("Blue", Some(Color32::from_rgba_unmultiplied(120, 190, 255, 100).to_array())),
                                ("Pink", Some(Color32::from_rgba_unmultiplied(245, 145, 185, 100).to_array())),
                                ("Purple", Some(Color32::from_rgba_unmultiplied(190, 150, 245, 100).to_array())),
                            ] {
                                if ui.button(name).clicked() {
                                    intents.push(Intent::Action(Action::FormatBlocks {
                                        page: page_id,
                                        blocks: ordered_ids.clone(),
                                        op: BulkBlockStyleOp::SetHighlight(color),
                                    }));
                                    close_menu = true;
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                    if ui.button("✕  Clear text formatting").clicked() {
                        intents.push(Intent::Action(Action::FormatBlocks {
                            page: page_id,
                            blocks: ordered_ids.clone(),
                            op: BulkBlockStyleOp::ClearFormatting,
                        }));
                        close_menu = true;
                    }
                    ui.separator();
                    ui.menu_button("⇄  Turn into", |ui| {
                        ui.set_min_width(170.0);
                        for kind in BlockKind::ALL {
                            if ui.button(kind.label()).clicked() {
                                intents.push(Intent::Action(Action::SetBlocksKind {
                                    page: page_id,
                                    blocks: ordered_ids.clone(),
                                    kind,
                                }));
                                close_menu = true;
                                ui.close_menu();
                            }
                        }
                    });
                    if has_todo {
                        if ui.button("☑  Mark to-dos as done").clicked() {
                            intents.push(Intent::Action(Action::SetBlocksChecked {
                                page: page_id,
                                blocks: ordered_ids.clone(),
                                checked: true,
                            }));
                            close_menu = true;
                        }
                        if ui.button("☐  Mark to-dos as not done").clicked() {
                            intents.push(Intent::Action(Action::SetBlocksChecked {
                                page: page_id,
                                blocks: ordered_ids.clone(),
                                checked: false,
                            }));
                            close_menu = true;
                        }
                    }
                    if can_group_columns && ui.button("▥  Arrange in columns").clicked() {
                        intents.push(Intent::Action(Action::GroupBlocksInColumns {
                            page: page_id,
                            blocks: ordered_ids.clone(),
                        }));
                        close_menu = true;
                    }
                    if has_columns && ui.button("▤  Separate columns").clicked() {
                        intents.push(Intent::Action(Action::UngroupBlockColumns {
                            page: page_id,
                            blocks: ordered_ids.clone(),
                        }));
                        close_menu = true;
                    }
                    ui.separator();
                    if ui.button("▲  Move up").clicked() {
                        intents.push(Intent::Action(Action::MoveBlocks {
                            page: page_id,
                            blocks: ordered_ids.clone(),
                            direction: -1,
                        }));
                        close_menu = true;
                    }
                    if ui.button("▼  Move down").clicked() {
                        intents.push(Intent::Action(Action::MoveBlocks {
                            page: page_id,
                            blocks: ordered_ids.clone(),
                            direction: 1,
                        }));
                        close_menu = true;
                    }
                    if ui.button("⧉  Duplicate blocks").clicked() {
                        intents.push(Intent::Action(Action::DuplicateBlocks {
                            page: page_id,
                            blocks: ordered_ids.clone(),
                        }));
                        close_menu = true;
                    }
                    if ui.button("📋  Copy text").clicked() {
                        let copied = self.selected_blocks_plain_text();
                        if !copied.is_empty() {
                            ui.ctx().copy_text(copied);
                        }
                        close_menu = true;
                    }
                    ui.separator();
                    if ui
                        .button(
                            RichText::new(if count == 1 {
                                "Delete block".to_owned()
                            } else {
                                format!("Delete {count} blocks")
                            })
                            .color(palette.danger),
                        )
                        .clicked()
                    {
                        intents.push(Intent::Action(Action::DeleteBlocks {
                            page: page_id,
                            blocks: ordered_ids,
                        }));
                        close_menu = true;
                        clear_selection = true;
                    }
                });
            });

        if close_menu {
            self.bulk_menu_pos = None;
        }
        if clear_selection {
            self.selected_blocks.clear();
        }
    }
}

const COLUMN_GAP: f32 = 10.0;

fn row_column_widths(
    row: &[usize],
    outline: &[crate::domain::model::BlockStub],
    document_width: f32,
) -> Vec<f32> {
    let ratios: Vec<_> = row
        .iter()
        .map(|position| outline[*position].column_width.max(0.08))
        .collect();
    widths_for_ratios(&ratios, document_width)
}

fn widths_for_ratios(ratios: &[f32], document_width: f32) -> Vec<f32> {
    let gaps = COLUMN_GAP * ratios.len().saturating_sub(1) as f32;
    let available = (document_width - gaps).max(48.0);
    let total = ratios.iter().sum::<f32>().max(0.08);
    ratios
        .iter()
        .map(|ratio| available * ratio.max(0.08) / total)
        .collect()
}

fn estimate_height(stub: &crate::domain::model::BlockStub, width: f32) -> f32 {
    if stub.kind == BlockKind::Database {
        return block::DATABASE_BLOCK_HEIGHT;
    }
    if stub.kind == BlockKind::Divider {
        return 32.0;
    }
    let columns = ((width - 65.0) / 7.4).max(12.0);
    let lines = (stub.chars as f32 / columns)
        .ceil()
        .max(stub.lines as f32)
        .max(1.0);
    (lines * (stub.kind.font_size() + 4.0) + block::BLOCK_PADDING * 2.0)
        .clamp(36.0, block::MAX_BLOCK_HEIGHT)
}
