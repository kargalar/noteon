use std::time::Instant;

use eframe::egui::{self, FontId, RichText, Stroke, Ui, pos2, vec2};

use crate::config::SEARCH_DELAY;
use crate::domain::model::{BlockId, PageId};
use crate::domain::search::PageFindMatch;
use crate::ui::palette::Palette;

pub use crate::domain::search::find_char_ranges;

#[derive(Clone, Debug, Default)]
pub struct PageFind {
    pub open: bool,
    pub query: String,
    matches: Vec<PageFindMatch>,
    current_index: usize,
    focus_input: bool,
    revision: u64,
    changed_at: Option<Instant>,
    loading: bool,
    failed: bool,
    jump_on_result: bool,
}

impl PageFind {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_input = true;
        self.jump_on_result = true;
        self.refresh();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.changed_at = None;
        self.loading = false;
        self.revision += 1;
    }

    pub fn refresh(&mut self) {
        if self.open {
            self.revision += 1;
            self.matches.clear();
            self.failed = false;
            self.loading = !self.query.trim().is_empty();
            self.changed_at = self.loading.then(Instant::now);
        }
    }

    pub fn take_request(&mut self) -> Option<(u64, String)> {
        if self
            .changed_at
            .is_none_or(|time| time.elapsed() < SEARCH_DELAY)
        {
            return None;
        }
        self.changed_at = None;
        Some((self.revision, self.query.clone()))
    }

    pub fn receive_matches(&mut self, revision: u64, matches: Vec<PageFindMatch>) -> bool {
        if revision != self.revision {
            return false;
        }
        self.loading = false;
        self.matches = matches;
        if self.current_index >= self.matches.len() {
            self.current_index = 0;
        }
        std::mem::take(&mut self.jump_on_result)
    }

    pub fn fail(&mut self, revision: u64) -> bool {
        if revision != self.revision {
            return false;
        }
        self.loading = false;
        self.failed = true;
        true
    }

    pub fn active_range_for_block(&self, block_id: BlockId) -> Option<(usize, usize)> {
        if self.open
            && let Some(PageFindMatch::Block {
                block_id: id,
                range,
                ..
            }) = self.matches.get(self.current_index)
            && *id == block_id
        {
            return Some((range.start, range.end));
        }
        None
    }

    pub fn active_title_range(&self) -> Option<(usize, usize)> {
        match self.matches.get(self.current_index) {
            Some(PageFindMatch::Title(range)) if self.open => Some((range.start, range.end)),
            _ => None,
        }
    }

    fn step(&mut self, forward: bool) {
        let count = self.matches.len();
        if count != 0 {
            self.current_index = if forward {
                (self.current_index + 1) % count
            } else {
                (self.current_index + count - 1) % count
            };
        }
    }

    pub fn show_bar(
        &mut self,
        ui: &mut Ui,
        page_id: PageId,
        widget_scope: Option<u64>,
        palette: Palette,
    ) -> bool {
        if !self.open {
            return false;
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

        egui::Area::new(egui::Id::new(("page-find-bar", page_id, widget_scope)))
            .order(egui::Order::Foreground)
            .fixed_pos(bar_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .stroke(Stroke::new(1.0_f32, palette.border))
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
                                    .return_key(None)
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
                                self.current_index = 0;
                                self.jump_on_result = true;
                                self.refresh();
                            }

                            // Escape may already have released focus at the start of the frame.
                            if ui.memory(|m| m.had_focus_last_frame(input_resp.id))
                                && ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close_clicked = true;
                            }
                            if input_resp.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let is_shift = ui.input(|i| i.modifiers.shift);
                                prev_clicked = is_shift;
                                next_clicked = !is_shift;
                            }

                            if self.loading {
                                ui.spinner();
                            } else if self.failed {
                                ui.label(RichText::new("Arama başarısız").color(palette.danger));
                            } else if !self.query.trim().is_empty() {
                                let (count, color) = if self.matches.is_empty() {
                                    ("0/0".to_owned(), palette.danger)
                                } else {
                                    (
                                        format!(
                                            "{}/{}",
                                            self.current_index + 1,
                                            self.matches.len()
                                        ),
                                        palette.muted,
                                    )
                                };
                                ui.label(RichText::new(count).size(11.5).color(color));
                            }

                            // Previous button
                            let prev_btn = ui
                                .add(
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
                            let next_btn = ui
                                .add(
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
                            let close_btn = ui
                                .add(
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
            return false;
        }

        if next_clicked || prev_clicked {
            self.step(next_clicked);
            ui.ctx().request_repaint();
            return true;
        }
        false
    }

    pub fn apply_scroll_to_match(
        &self,
        jump_to: &mut Option<usize>,
        reset_scroll: &mut bool,
        scroll_offset: &mut f32,
    ) {
        if let Some(m) = self.matches.get(self.current_index) {
            match m {
                PageFindMatch::Title(_) => {
                    *jump_to = None;
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
