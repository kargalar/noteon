use std::time::Instant;

use eframe::egui::{self, RichText, vec2};

use crate::config::SEARCH_DELAY;
use crate::domain::model::SearchHit;

use super::text;
use super::{Intent, palette::Palette, small_label};

#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub generation: u64,
    pub hits: Vec<SearchHit>,
    pub loading: bool,
    query: String,
    changed_at: Option<Instant>,
    focus: bool,
}

impl Search {
    pub fn open(&mut self) {
        self.open = true;
        self.focus = true;
    }

    pub fn take_request(&mut self) -> Option<(String, u64)> {
        if self
            .changed_at
            .is_some_and(|time| time.elapsed() >= SEARCH_DELAY)
        {
            self.changed_at = None;
            self.loading = !self.query.trim().is_empty();
            return Some((self.query.clone(), self.generation));
        }
        None
    }

    pub fn show(&mut self, context: &egui::Context, palette: Palette) -> Vec<Intent> {
        let mut intents = Vec::new();
        if !self.open {
            return intents;
        }
        let mut open = true;
        egui::Window::new(text::SEARCH_TITLE)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(590.0)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 95.0])
            .show(context, |ui| {
                ui.add_space(7.0);
                let input = ui.add_sized(
                    [ui.available_width(), 37.0],
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(text::SEARCH_PLACEHOLDER)
                        .font(egui::FontId::proportional(18.0)),
                );
                if self.focus {
                    input.request_focus();
                    self.focus = false;
                }
                if input.changed() {
                    self.generation += 1;
                    self.changed_at = Some(Instant::now());
                    self.hits.clear();
                    self.loading = true;
                }
                ui.add_space(9.0);
                ui.separator();
                if self.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        small_label(ui, "Searching your notes…", palette.muted);
                    });
                } else if self.query.trim().is_empty() {
                    ui.add_space(20.0);
                    small_label(
                        ui,
                        "Search page titles and all page content.",
                        palette.muted,
                    );
                    ui.add_space(20.0);
                } else if self.hits.is_empty() {
                    ui.add_space(20.0);
                    ui.label("No notes match these words.");
                    ui.add_space(20.0);
                } else {
                    egui::ScrollArea::vertical().max_height(360.0).show_rows(
                        ui,
                        62.0,
                        self.hits.len(),
                        |ui, range| {
                            for index in range {
                                let hit = &self.hits[index];
                                let button = egui::Button::new(
                                    RichText::new(format!(
                                        "{}  {}\n{}",
                                        hit.icon, hit.title, hit.preview
                                    ))
                                    .size(13.0),
                                )
                                .min_size(vec2(ui.available_width(), 58.0))
                                .frame(false);
                                if ui.add(button).clicked() {
                                    intents.push(Intent::Open(hit.page_id, hit.position));
                                }
                            }
                        },
                    );
                }
                ui.separator();
                small_label(
                    ui,
                    "Enter  open first result     ·     Esc  close     ·     Up to 50 results",
                    palette.muted,
                );
                if input.has_focus()
                    && context.input(|input| input.key_pressed(egui::Key::Enter))
                    && let Some(hit) = self.hits.first()
                {
                    intents.push(Intent::Open(hit.page_id, hit.position));
                }
            });
        if context.input(|input| input.key_pressed(egui::Key::Escape)) || !intents.is_empty() {
            open = false;
        }
        self.open = open;
        intents
    }
}
