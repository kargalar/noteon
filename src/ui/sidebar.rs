use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::domain::model::{Page, PageId};
use crate::worker::Action;

use super::text;
use super::{Intent, palette::Palette, quiet_button};

const ROW_HEIGHT: f32 = 33.0;

enum TreeRow {
    Section(&'static str),
    Page {
        id: PageId,
        depth: usize,
        expandable: bool,
    },
}

#[derive(Default)]
pub struct Sidebar {
    pub pages: Vec<Page>,
    by_id: HashMap<PageId, usize>,
    children: HashMap<Option<PageId>, Vec<PageId>>,
    expanded: HashSet<PageId>,
    rows: Vec<TreeRow>,
}

impl Sidebar {
    pub fn set_pages(&mut self, pages: Vec<Page>) {
        self.pages = pages;
        self.by_id = self
            .pages
            .iter()
            .enumerate()
            .map(|(index, page)| (page.id, index))
            .collect();
        self.children.clear();
        for page in &self.pages {
            if page.trashed_by.is_none() {
                self.children
                    .entry(page.parent_id)
                    .or_default()
                    .push(page.id);
            }
        }
        self.rebuild();
    }

    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.by_id.get(&id).map(|index| &self.pages[*index])
    }

    pub fn update_title(&mut self, id: PageId, title: &str, icon: &str) {
        if let Some(index) = self.by_id.get(&id) {
            self.pages[*index].title = title.to_owned();
            self.pages[*index].icon = icon.to_owned();
        }
    }

    pub fn reveal(&mut self, id: PageId) {
        let mut parent = self.page(id).and_then(|page| page.parent_id);
        let mut visited = HashSet::new();
        while let Some(id) = parent {
            if !visited.insert(id) {
                break;
            }
            self.expanded.insert(id);
            parent = self.page(id).and_then(|page| page.parent_id);
        }
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        let favorites: Vec<_> = self
            .pages
            .iter()
            .filter(|page| page.favorite && page.trashed_by.is_none())
            .map(|page| page.id)
            .collect();
        if !favorites.is_empty() {
            self.rows.push(TreeRow::Section("FAVORITES"));
            self.rows
                .extend(favorites.into_iter().map(|id| TreeRow::Page {
                    id,
                    depth: 0,
                    expandable: false,
                }));
        }
        self.rows.push(TreeRow::Section("WORKSPACE"));
        let mut stack: Vec<_> = self
            .children
            .get(&None)
            .into_iter()
            .flatten()
            .rev()
            .map(|id| (*id, 0))
            .collect();
        let mut visited = HashSet::new();
        while let Some((id, depth)) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            let children = self.children.get(&Some(id));
            self.rows.push(TreeRow::Page {
                id,
                depth,
                expandable: children.is_some_and(|items| !items.is_empty()),
            });
            if self.expanded.contains(&id)
                && let Some(children) = children
            {
                stack.extend(children.iter().rev().map(|child| (*child, depth + 1)));
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        selected: Option<PageId>,
        trash: bool,
        palette: Palette,
    ) -> Vec<Intent> {
        let mut intents = Vec::new();
        show_header(ui, palette, &mut intents);
        self.show_tree(ui, selected, trash, palette, &mut intents);
        show_footer(ui, trash, palette, &mut intents);
        intents
    }

    fn show_tree(
        &mut self,
        ui: &mut Ui,
        selected: Option<PageId>,
        trash: bool,
        palette: Palette,
        intents: &mut Vec<Intent>,
    ) {
        let footer = 128.0;
        let mut toggle = None;
        egui::ScrollArea::vertical()
            .id_salt("page-tree")
            .max_height((ui.available_height() - footer).max(80.0))
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
                for index in range {
                    match &self.rows[index] {
                        TreeRow::Section(title) => {
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), ROW_HEIGHT),
                                Sense::hover(),
                            );
                            ui.painter().text(
                                rect.left_center() + vec2(6.0, 4.0),
                                Align2::LEFT_CENTER,
                                title,
                                FontId::proportional(10.0),
                                palette.muted,
                            );
                            if *title == "WORKSPACE" {
                                let add_rect = egui::Rect::from_center_size(
                                    egui::pos2(rect.right() - 15.0, rect.center().y),
                                    vec2(28.0, 28.0),
                                );
                                if ui
                                    .put(
                                        add_rect,
                                        egui::Button::new(
                                            RichText::new("+").size(18.0).color(palette.muted),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text("New page")
                                    .clicked()
                                {
                                    intents.push(Intent::Action(Action::CreatePage(None)));
                                }
                            }
                        }
                        TreeRow::Page {
                            id,
                            depth,
                            expandable,
                        } => {
                            if let Some(page) = self.page(*id) {
                                let row = PageRow {
                                    page,
                                    depth: *depth,
                                    expandable: *expandable,
                                    expanded: self.expanded.contains(id),
                                    active: !trash && selected == Some(*id),
                                    index,
                                };
                                if row.show(ui, palette, intents, &self.pages) {
                                    toggle = Some(*id);
                                }
                            }
                        }
                    }
                }
            });
        if let Some(id) = toggle {
            if !self.expanded.remove(&id) {
                self.expanded.insert(id);
            }
            self.rebuild();
        }
    }
}

fn show_header(ui: &mut Ui, palette: Palette, intents: &mut Vec<Intent>) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("noteon").size(24.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if quiet_button(ui, "⌕", palette.muted)
                .on_hover_text("Search workspace · Ctrl + K")
                .clicked()
            {
                intents.push(Intent::Search);
            }
        });
    });
    ui.add_space(15.0);
    ui.add_space(2.0);
    ui.menu_button(
        RichText::new("+   New").color(palette.accent).strong(),
        |ui| {
            if ui.button("New page").clicked() {
                intents.push(Intent::Action(Action::CreatePage(None)));
                ui.close_menu();
            }
            if ui.button("New database").clicked() {
                intents.push(Intent::Action(Action::CreateDatabase(None)));
                ui.close_menu();
            }
        },
    );
    ui.add_space(14.0);
}

fn show_footer(ui: &mut Ui, trash: bool, palette: Palette, intents: &mut Vec<Intent>) {
    ui.add_space(9.0);
    ui.separator();
    if quiet_button(
        ui,
        "▱   Trash",
        if trash { palette.accent } else { palette.muted },
    )
    .clicked()
    {
        intents.push(Intent::Trash);
    }
    ui.horizontal(|ui| {
        if quiet_button(ui, "⚙  Settings", palette.muted).clicked() {
            intents.push(Intent::Settings);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if quiet_button(ui, "?", palette.muted)
                .on_hover_text("Shortcuts and help")
                .clicked()
            {
                intents.push(Intent::Help);
            }
            if quiet_button(ui, "◐", palette.muted)
                .on_hover_text("Toggle theme")
                .clicked()
            {
                intents.push(Intent::ToggleTheme);
            }
        });
    });
    ui.add_space(5.0);
}

struct PageRow<'a> {
    page: &'a Page,
    depth: usize,
    expandable: bool,
    expanded: bool,
    active: bool,
    index: usize,
}

impl PageRow<'_> {
    fn show(
        &self,
        ui: &mut Ui,
        palette: Palette,
        intents: &mut Vec<Intent>,
        pages: &[Page],
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), ROW_HEIGHT),
            Sense::click_and_drag(),
        );
        response.dnd_set_drag_payload(self.page.id);
        if let Some(source) = response.dnd_hover_payload::<PageId>()
            && *source != self.page.id
            && pages
                .iter()
                .find(|page| page.id == *source)
                .map(|page| page.parent_id)
                == Some(self.page.parent_id)
        {
            let pointer_y = ui
                .ctx()
                .pointer_interact_pos()
                .map(|point| point.y)
                .unwrap_or(rect.center().y);
            let after = pointer_y >= rect.center().y;
            let y = if after {
                rect.bottom() - 1.0
            } else {
                rect.top() + 1.0
            };
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 4.0, y),
                    egui::pos2(rect.right() - 4.0, y),
                ],
                Stroke::new(2.0, palette.accent),
            );
            if let Some(source) = response.dnd_release_payload::<PageId>() {
                intents.push(Intent::Action(Action::ReorderPage {
                    source: *source,
                    target: self.page.id,
                    after,
                }));
            }
        }
        if self.active || response.hovered() {
            ui.painter().rect_filled(
                rect,
                5.0,
                if self.active {
                    palette.accent_soft
                } else {
                    palette.hover
                },
            );
        }
        let x = rect.left() + 9.0 + self.depth.min(7) as f32 * 13.0;
        let toggled = self.expansion_arrow(ui, rect, x, palette);
        self.paint_title(ui, rect, x, palette);
        if response.middle_clicked() && !toggled {
            intents.push(Intent::OpenInTab(self.page.id));
        } else if response.clicked() && !toggled {
            intents.push(Intent::Open(self.page.id, None));
        }
        response
            .on_hover_text(self.page.display_title())
            .context_menu(|ui| show_page_menu(ui, self.page, palette, intents));
        toggled
    }

    fn expansion_arrow(&self, ui: &mut Ui, rect: egui::Rect, x: f32, palette: Palette) -> bool {
        if !self.expandable {
            return false;
        }
        let hit = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(16.0, ROW_HEIGHT));
        let response = ui.interact(hit, ui.id().with(("expand", self.index)), Sense::click());
        ui.painter().text(
            hit.center(),
            Align2::CENTER_CENTER,
            if self.expanded { "⌄" } else { "›" },
            FontId::proportional(16.0),
            palette.muted,
        );
        response.clicked()
    }

    fn paint_title(&self, ui: &Ui, rect: egui::Rect, x: f32, palette: Palette) {
        let icon_x = x + 20.0;
        ui.painter().text(
            egui::pos2(icon_x, rect.center().y),
            Align2::CENTER_CENTER,
            &self.page.icon,
            FontId::proportional(15.0),
            if self.active {
                palette.accent
            } else {
                palette.muted
            },
        );
        let text_x = icon_x + 16.0;
        let galley = ui.painter().layout(
            self.page.display_title().into(),
            FontId::proportional(13.0),
            if self.active {
                palette.accent
            } else {
                palette.text
            },
            (rect.right() - text_x - 7.0).max(15.0),
        );
        ui.painter().with_clip_rect(rect).galley(
            egui::pos2(text_x, rect.center().y - 9.0),
            galley,
            palette.text,
        );
    }
}

fn show_page_menu(ui: &mut Ui, page: &Page, palette: Palette, intents: &mut Vec<Intent>) {
    if ui.button(text::NEW_CHILD).clicked() {
        intents.push(Intent::Action(Action::CreatePage(Some(page.id))));
        ui.close_menu();
    }
    if ui
        .button(if page.favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        })
        .clicked()
    {
        intents.push(Intent::Action(Action::Favorite(page.id)));
        ui.close_menu();
    }
    if ui.button(text::DUPLICATE).clicked() {
        intents.push(Intent::Action(Action::Duplicate(page.id)));
        ui.close_menu();
    }
    if page.parent_id.is_some() && ui.button("Move to top level").clicked() {
        intents.push(Intent::Action(Action::MovePage(page.id, None)));
        ui.close_menu();
    }
    ui.separator();
    if ui
        .button(RichText::new(text::MOVE_TO_TRASH).color(palette.danger))
        .clicked()
    {
        intents.push(Intent::Action(Action::Trash(page.id)));
        ui.close_menu();
    }
}
