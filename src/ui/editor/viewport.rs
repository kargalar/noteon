use super::{DatabaseContext, Editor, block, header::Canvas};
use crate::domain::model::{BlockKind, PageId};
use crate::domain::virtual_layout::VirtualLayout;
use crate::ui::{Intent, palette::Palette};
use crate::worker::Action;
use eframe::egui::{self, Rect, Sense, Ui, pos2, vec2};

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
                if let Some(block) = self.cache.blocks.get_mut(&position) {
                    self.rendered_blocks += 1;
                    let focus = self.focus_position == Some(position);
                    if block::show(ui, block_rect, block, focus, palette, widget_scope, intents) {
                        self.cache.needs_measure.insert(position);
                    }
                    if block.kind == BlockKind::Database
                        && let Ok(database_id) = block.content.parse::<PageId>()
                    {
                        databases.visible.insert(database_id);
                        if let Some(database) = databases.databases.get_mut(&database_id) {
                            let database_rect = Rect::from_min_max(
                                block_rect.min + vec2(block::HANDLE_WIDTH, 0.0),
                                block_rect.max,
                            );
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(database_rect),
                                |ui| {
                                    ui.push_id(("embedded-database", block.id), |ui| {
                                        egui::Frame::NONE.show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.set_min_height(block::DATABASE_BLOCK_HEIGHT - 16.0);
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
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::Text);
        if append_response.clicked() {
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
                }
            }
        }
        for position in touched {
            self.cache.touch(position);
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
