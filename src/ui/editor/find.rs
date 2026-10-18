use std::collections::BTreeMap;
use eframe::egui::{self, FontId, RichText, Stroke, Ui, pos2, vec2};

use crate::domain::model::{Block, BlockId, Document, PageId};
use crate::ui::palette::Palette;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageFindMatch {
    Title {
        char_start: usize,
        char_end: usize,
    },
    Block {
        position: usize,
        block_id: BlockId,
        char_start: usize,
        char_end: usize,
    },
}

#[derive(Clone, Debug, Default)]
pub struct PageFind {
    pub open: bool,
    pub query: String,
    pub matches: Vec<PageFindMatch>,
    pub current_index: usize,
    pub focus_input: bool,
}

impl PageFind {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_input = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn next(&mut self) -> Option<&PageFindMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.current_index = (self.current_index + 1) % self.matches.len();
        self.matches.get(self.current_index)
    }

    pub fn prev(&mut self) -> Option<&PageFindMatch> {
        if self.matches.is_empty() {
            return None;
        }
        if self.current_index == 0 {
            self.current_index = self.matches.len() - 1;
        } else {
            self.current_index -= 1;
        }
        self.matches.get(self.current_index)
    }

    pub fn current_match(&self) -> Option<&PageFindMatch> {
        if self.matches.is_empty() {
            None
        } else {
            self.matches.get(self.current_index)
        }
    }

    pub fn active_range_for_block(&self, block_id: BlockId, position: usize) -> Option<(usize, usize)> {
        if let Some(PageFindMatch::Block {
            position: p,
            block_id: id,
            char_start,
            char_end,
        }) = self.current_match()
        {
            if *id == block_id || *p == position {
                return Some((*char_start, *char_end));
            }
        }
        None
    }

    pub fn is_title_active_match(&self) -> bool {
        matches!(self.current_match(), Some(PageFindMatch::Title { .. }))
    }

    pub fn update_matches(
        &mut self,
        document: Option<&Document>,
        blocks: &BTreeMap<usize, Block>,
    ) {
        let query = self.query.trim().to_lowercase();
        self.matches.clear();
        if query.is_empty() {
            self.current_index = 0;
            return;
        }

        let q_char_len = query.chars().count();

        // 1. Check page title
        if let Some(doc) = document {
            let title_lower = doc.page.title.to_lowercase();
            let mut search_idx = 0;
            while let Some(pos) = title_lower[search_idx..].find(&query) {
                let byte_start = search_idx + pos;
                let byte_end = byte_start + query.len();
                let char_start = doc.page.title[..byte_start].chars().count();
                self.matches.push(PageFindMatch::Title {
                    char_start,
                    char_end: char_start + q_char_len,
                });
                search_idx = byte_end;
            }
        }

        // 2. Check blocks in document order
        for (pos, block) in blocks {
            let content_lower = block.content.to_lowercase();
            let mut search_idx = 0;
            while let Some(match_pos) = content_lower[search_idx..].find(&query) {
                let byte_start = search_idx + match_pos;
                let byte_end = byte_start + query.len();
                let char_start = block.content[..byte_start].chars().count();
                self.matches.push(PageFindMatch::Block {
                    position: *pos,
                    block_id: block.id,
                    char_start,
                    char_end: char_start + q_char_len,
                });
                search_idx = byte_end;
            }
        }

        if self.matches.is_empty() {
            self.current_index = 0;
        } else if self.current_index >= self.matches.len() {
            self.current_index = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show_bar(
        &mut self,
        ui: &mut Ui,
        page_id: PageId,
        widget_scope: Option<u64>,
        palette: Palette,
        document: Option<&Document>,
        blocks: &BTreeMap<usize, Block>,
        jump_to: &mut Option<usize>,
        reset_scroll: &mut bool,
        scroll_offset: &mut f32,
    ) {
        if !self.open {
            return;
        }

        let available = ui.max_rect();
        let bar_width = 300.0;
        let bar_pos = pos2(
            (available.right() - bar_width - 18.0).max(available.left() + 8.0),
            available.top() + 8.0,
        );

        let mut next_clicked = false;
        let mut prev_clicked = false;
        let mut close_clicked = false;
        let mut query_changed = false;

        egui::Area::new(egui::Id::new(("page-find-bar", page_id, widget_scope)))
            .order(egui::Order::Foreground)
            .fixed_pos(bar_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(8.0)
                    .shadow(ui.visuals().window_shadow)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(5.0, 0.0);

                            // Search icon
                            ui.label(RichText::new("🔍").size(12.0).color(palette.muted));

                            // Search Input
                            let input_resp = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .id(egui::Id::new(("page-find-input", page_id, widget_scope)))
                                    .hint_text("Sayfada ara…")
                                    .font(FontId::proportional(13.0))
                                    .desired_width(128.0)
                                    .margin(egui::Margin::symmetric(4, 3)),
                            );

                            if self.focus_input {
                                input_resp.request_focus();
                                self.focus_input = false;
                            }

                            if input_resp.changed() {
                                query_changed = true;
                            }

                            // Enter / Shift+Enter / Escape when typing
                            if input_resp.has_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    close_clicked = true;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    let is_shift = ui.input(|i| i.modifiers.shift);
                                    if is_shift {
                                        prev_clicked = true;
                                    } else {
                                        next_clicked = true;
                                    }
                                }
                            }

                            // Match Counter
                            let count_text = if self.query.trim().is_empty() {
                                String::new()
                            } else if self.matches.is_empty() {
                                "0/0".to_string()
                            } else {
                                format!("{}/{}", self.current_index + 1, self.matches.len())
                            };
                            if !count_text.is_empty() {
                                let count_color = if self.matches.is_empty() {
                                    palette.danger
                                } else {
                                    palette.muted
                                };
                                ui.label(RichText::new(count_text).size(11.5).color(count_color));
                            }

                            // Previous button
                            let prev_btn = ui.add(
                                egui::Button::new(
                                    RichText::new("▲").size(10.5).color(palette.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Önceki eşleşme (Shift + Enter)");
                            if prev_btn.clicked() {
                                prev_clicked = true;
                            }

                            // Next button
                            let next_btn = ui.add(
                                egui::Button::new(
                                    RichText::new("▼").size(10.5).color(palette.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Sonraki eşleşme (Enter)");
                            if next_btn.clicked() {
                                next_clicked = true;
                            }

                            // Close button
                            let close_btn = ui.add(
                                egui::Button::new(
                                    RichText::new("✕").size(10.5).color(palette.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Kapat (Esc)");
                            if close_btn.clicked() {
                                close_clicked = true;
                            }
                        });
                    });
            });

        if close_clicked {
            self.close();
            return;
        }

        if query_changed {
            self.update_matches(document, blocks);
            self.apply_scroll_to_match(jump_to, reset_scroll, scroll_offset);
        } else if next_clicked {
            self.next();
            self.apply_scroll_to_match(jump_to, reset_scroll, scroll_offset);
        } else if prev_clicked {
            self.prev();
            self.apply_scroll_to_match(jump_to, reset_scroll, scroll_offset);
        }
    }

    fn apply_scroll_to_match(
        &self,
        jump_to: &mut Option<usize>,
        reset_scroll: &mut bool,
        scroll_offset: &mut f32,
    ) {
        if let Some(m) = self.current_match() {
            match m {
                PageFindMatch::Title { .. } => {
                    *jump_to = Some(0);
                    *scroll_offset = 0.0;
                    *reset_scroll = true;
                }
                PageFindMatch::Block { position, .. } => {
                    *jump_to = Some(*position);
                }
            }
        }
    }
}
