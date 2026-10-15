use super::block;
use crate::config::{PAGE_ICONS, UNTITLED};
use crate::domain::model::{Document, Page, PageEdit};
use crate::ui::{Intent, palette::Palette, quiet_button, small_label, text};
use crate::worker::Action;
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, vec2};

#[derive(Clone, Copy)]
pub(super) struct Canvas {
    pub width: f32,
    pub document_width: f32,
    pub inset: f32,
    pub widget_scope: Option<u64>,
}

pub(super) struct RecordPropertiesContext<'a> {
    pub record: Option<(&'a crate::domain::model::DatabaseData, i64)>,
    pub value_drafts: &'a mut std::collections::HashMap<(i64, i64), String>,
}

pub(super) fn show(
    ui: &mut Ui,
    document: &mut Document,
    title_focus: &mut bool,
    palette: Palette,
    parent_title: Option<&str>,
    widget_scope: Option<u64>,
    record_properties: RecordPropertiesContext<'_>,
) -> (Canvas, Vec<Intent>) {
    let mut intents = Vec::new();
    let page = &mut document.page;
    breadcrumb(ui, page, palette, parent_title, &mut intents);
    ui.add_space(8.0);
    ui.separator();
    let width = ui.available_width();
    let cover_height = if page.cover != 0 || page.cover_image.is_some() {
        116.0
    } else {
        28.0
    };
    ui.add_space(10.0);
    let (cover_rect, cover_response) =
        ui.allocate_exact_size(vec2(width, cover_height), Sense::drag());
    let mut page_changed = false;
    if let Some(path) = page.cover_image.as_deref() {
        let image_size = super::media::paint_cover(
            ui,
            std::path::Path::new(path),
            cover_rect,
            egui::vec2(page.cover_x, page.cover_y),
        );
        if let Some(image_size) = image_size {
            if cover_response.drag_started() {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new(("cover-focus", page.id)),
                        [page.cover_x, page.cover_y],
                    );
                });
            }
            if cover_response.dragged() {
                let initial = ui.ctx().data(|data| {
                    data.get_temp::<[f32; 2]>(egui::Id::new(("cover-focus", page.id)))
                });
                if let Some(initial) = initial {
                    let view_ratio = cover_rect.width() / cover_rect.height().max(1.0);
                    let image_ratio = image_size.x / image_size.y.max(1.0);
                    let crop_x = if image_ratio > view_ratio {
                        view_ratio / image_ratio
                    } else {
                        1.0
                    };
                    let crop_y = if image_ratio > view_ratio {
                        1.0
                    } else {
                        image_ratio / view_ratio
                    };
                    let delta = cover_response.drag_delta();
                    let next_x = (initial[0] - delta.x / cover_rect.width().max(1.0) * crop_x)
                        .clamp(0.0, 1.0);
                    let next_y = (initial[1] - delta.y / cover_rect.height().max(1.0) * crop_y)
                        .clamp(0.0, 1.0);
                    page_changed |= (page.cover_x - next_x).abs() > f32::EPSILON
                        || (page.cover_y - next_y).abs() > f32::EPSILON;
                    page.cover_x = next_x;
                    page.cover_y = next_y;
                }
            }
        } else {
            cover(ui, cover_rect, palette.cover(page.cover.max(1)));
        }
    } else if page.cover != 0 {
        cover(ui, cover_rect, palette.cover(page.cover));
    }
    let cover_controls_id = egui::Id::new(("cover-controls", page.id));
    let controls_were_hovered = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(cover_controls_id).unwrap_or_default());
    let cover_menu_open = ui.ctx().memory(|memory| memory.any_popup_open());
    if cover_response.hovered() || controls_were_hovered || cover_menu_open {
        let controls = egui::Area::new(cover_controls_id)
            .order(egui::Order::Foreground)
            .fixed_pos(cover_rect.right_top() + vec2(-96.0, 6.0))
            .show(ui.ctx(), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    ui.menu_button("Cover", |ui| {
                        if ui.button("Choose image…").clicked() {
                            intents.push(Intent::Action(Action::ChooseCoverImage(page.id)));
                            ui.close_menu();
                        }
                        if page.cover_image.is_some() {
                            ui.label("Drag the cover image to reposition it.");
                        }
                        ui.separator();
                        for (style, label) in [
                            (0, "No cover"),
                            (1, "Sage"),
                            (2, "Sand"),
                            (3, "Sky"),
                            (4, "Rose"),
                        ] {
                            if ui
                                .selectable_label(
                                    page.cover == style && page.cover_image.is_none(),
                                    label,
                                )
                                .clicked()
                            {
                                page.cover = style;
                                page.cover_image = None;
                                page_changed = true;
                                ui.close_menu();
                            }
                        }
                    });
                });
            });
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                cover_controls_id,
                controls.response.hovered() || cover_response.hovered(),
            );
        });
    } else {
        ui.ctx()
            .data_mut(|data| data.insert_temp(cover_controls_id, false));
    }
    if cover_height > 28.0 {
        ui.add_space(8.0);
    }
    let document_width = (width - 72.0).clamp(200.0, 850.0);
    let inset = ((width - document_width) / 2.0).max(12.0);
    page_changed |= title(
        ui,
        page,
        title_focus,
        inset,
        document_width,
        widget_scope,
        palette,
    );
    if page_changed {
        intents.push(Intent::PageChanged(PageEdit::from(&*page)));
    }
    crate::ui::page_properties::show(
        ui,
        crate::ui::page_properties::PagePropertyTarget {
            page: page.id,
            widget_scope,
            indent: inset + block::HANDLE_WIDTH,
        },
        page.created_at,
        &document.properties,
        false,
        palette,
        &mut intents,
    );
    if let Some((database, record_page)) = record_properties.record {
        crate::ui::database::show_record_properties(
            ui,
            database,
            crate::ui::database::RecordPropertiesTarget {
                page_id: record_page,
                indent: inset + block::HANDLE_WIDTH,
            },
            false,
            palette,
            &mut intents,
            record_properties.value_drafts,
        );
    }
    toolbar(ui, document.outline.len(), inset, palette);
    ui.add_space(13.0);
    (
        Canvas {
            width,
            document_width,
            inset,
            widget_scope,
        },
        intents,
    )
}

fn breadcrumb(
    ui: &mut Ui,
    page: &Page,
    palette: Palette,
    parent_title: Option<&str>,
    intents: &mut Vec<Intent>,
) {
    ui.horizontal(|ui| {
        if quiet_button(ui, "‹", palette.muted)
            .on_hover_text("Back · Alt + Left")
            .clicked()
        {
            intents.push(Intent::Back);
        }
        if quiet_button(ui, "›", palette.muted)
            .on_hover_text("Forward · Alt + Right")
            .clicked()
        {
            intents.push(Intent::Forward);
        }
        if let Some(parent) = parent_title {
            ui.label(RichText::new(parent).color(palette.muted));
            ui.label(RichText::new("/").color(palette.muted));
        }
        ui.label(RichText::new(format!("{}  {}", page.icon, page.display_title())).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                if ui.button(text::NEW_CHILD).clicked() {
                    intents.push(Intent::Action(Action::CreatePage(Some(page.id))));
                    ui.close_menu();
                }
                if ui.button(text::DUPLICATE).clicked() {
                    intents.push(Intent::Action(Action::Duplicate(page.id)));
                    ui.close_menu();
                }
                if ui.button(text::EXPORT_MARKDOWN).clicked() {
                    intents.push(Intent::Action(Action::Export(page.id)));
                    ui.close_menu();
                }
                if ui.button(text::IMPORT_MARKDOWN).clicked() {
                    intents.push(Intent::Action(Action::Import));
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
            });
            if quiet_button(
                ui,
                if page.favorite { "★" } else { "☆" },
                if page.favorite {
                    palette.accent
                } else {
                    palette.muted
                },
            )
            .on_hover_text("Favorite")
            .clicked()
            {
                intents.push(Intent::Action(Action::Favorite(page.id)));
            }
            small_label(ui, "PERSONAL", palette.muted);
        });
    });
}

fn identity(ui: &mut Ui, page: &mut Page, palette: Palette) -> bool {
    let mut page_changed = false;
    ui.horizontal(|ui| {
        ui.menu_button(
            RichText::new(&page.icon).size(29.0).color(palette.accent),
            |ui| {
                ui.set_min_width(280.0);
                let search_id = egui::Id::new(("page-icon-search", page.id));
                let mut query = ui
                    .ctx()
                    .data(|data| data.get_temp::<String>(search_id))
                    .unwrap_or_default();
                let search = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("Search icons")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .data_mut(|data| data.insert_temp(search_id, query.clone()));

                let query = query.trim().to_lowercase();
                let matching_icons = PAGE_ICONS
                    .iter()
                    .filter(|(name, icon)| {
                        query.is_empty() || name.contains(&query) || icon.contains(query.as_str())
                    })
                    .collect::<Vec<_>>();
                if matching_icons.is_empty() {
                    ui.label("No icons found");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Grid::new(("page-icon-grid", page.id))
                                .num_columns(8)
                                .spacing([3.0, 3.0])
                                .show(ui, |ui| {
                                    for (index, (name, icon)) in matching_icons.iter().enumerate() {
                                        if ui
                                            .add_sized(
                                                [30.0, 30.0],
                                                egui::Button::new(RichText::new(*icon).size(19.0)),
                                            )
                                            .on_hover_text(*name)
                                            .clicked()
                                        {
                                            page.icon = (*icon).to_string();
                                            page_changed = true;
                                            ui.ctx().data_mut(|data| {
                                                data.insert_temp(search_id, String::new());
                                            });
                                            ui.close_menu();
                                        }
                                        if (index + 1) % 8 == 0 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                }
                if search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    ui.close_menu();
                }
            },
        );
    });
    page_changed
}

fn title(
    ui: &mut Ui,
    page: &mut Page,
    title_focus: &mut bool,
    inset: f32,
    document_width: f32,
    widget_scope: Option<u64>,
    palette: Palette,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(inset);
        changed = identity(ui, page, palette);
        ui.add_space(4.0);
        let title = ui.add_sized(
            [document_width - 42.0, 51.0],
            egui::TextEdit::singleline(&mut page.title)
                .id(title_widget_id(page.id, widget_scope))
                .font(FontId::proportional(34.0))
                .frame(false)
                .hint_text(UNTITLED)
                .margin(egui::Margin::ZERO),
        );
        changed |= title.changed();
        if *title_focus {
            title.request_focus();
            *title_focus = false;
        }
    });
    changed
}

fn title_widget_id(page_id: i64, widget_scope: Option<u64>) -> egui::Id {
    widget_scope.map_or_else(
        || egui::Id::new(("title", page_id)),
        |scope| egui::Id::new(("title", scope, page_id)),
    )
}

fn toolbar(ui: &mut Ui, block_count: usize, inset: f32, palette: Palette) {
    ui.horizontal(|ui| {
        ui.add_space(inset + block::HANDLE_WIDTH);
        small_label(
            ui,
            &format!("{} blocks   ·   Autosave on", block_count),
            palette.muted,
        );
    });
}

fn cover(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 6.0, color);
    let ink = Color32::from_rgba_unmultiplied(73, 95, 62, 35);
    let center = rect.right_center() - vec2(100.0, -36.0);
    for index in 0..8 {
        let radius = 32.0 + index as f32 * 18.0;
        painter.circle_stroke(center, radius, Stroke::new(1.2, ink));
    }
    let x = rect.left() + 32.0;
    painter.text(
        egui::pos2(x, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        "LESS NOISE.",
        FontId::proportional(10.0),
        Color32::from_rgb(80, 99, 66),
    );
    painter.text(
        egui::pos2(x, rect.center().y + 15.0),
        Align2::LEFT_CENTER,
        "More thought.",
        FontId::proportional(22.0),
        Color32::from_rgb(60, 80, 48),
    );
}
