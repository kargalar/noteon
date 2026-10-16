use super::block;
use crate::config::UNTITLED;
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
    pub property_drafts: &'a mut std::collections::HashMap<i64, (String, String)>,
    pub title_drafts: &'a mut std::collections::HashMap<i64, String>,
    pub value_drafts: &'a mut std::collections::HashMap<(i64, i64), String>,
}

pub(super) fn show(
    ui: &mut Ui,
    document: &mut Document,
    title_focus: &mut bool,
    palette: Palette,
    breadcrumbs: &[crate::ui::BreadcrumbItem],
    widget_scope: Option<u64>,
    record_properties: RecordPropertiesContext<'_>,
) -> (Canvas, Vec<Intent>) {
    let mut intents = Vec::new();
    let page = &mut document.page;
    let mut page_changed = false;
    breadcrumb(ui, page, palette, breadcrumbs, &mut intents, &mut page_changed);
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
    let cover_menu_open_id = egui::Id::new(("cover-menu-open", page.id));
    let was_cover_menu_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(cover_menu_open_id).unwrap_or_default());
    if cover_response.hovered() || controls_were_hovered || was_cover_menu_open {
        let mut is_cover_menu_open = false;
        let controls = egui::Area::new(cover_controls_id)
            .order(egui::Order::Foreground)
            .fixed_pos(cover_rect.right_top() + vec2(-96.0, 6.0))
            .show(ui.ctx(), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    let menu_resp = ui.menu_button("Cover", |ui| {
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
                    if menu_resp.inner.is_some() {
                        is_cover_menu_open = true;
                    }
                });
            });
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                cover_controls_id,
                controls.response.hovered() || cover_response.hovered(),
            );
            data.insert_temp(cover_menu_open_id, is_cover_menu_open);
        });
    } else {
        ui.ctx().data_mut(|data| {
            data.insert_temp(cover_controls_id, false);
            data.insert_temp(cover_menu_open_id, false);
        });
    }
    if cover_height > 28.0 {
        ui.add_space(8.0);
    }
    let (document_width, inset) = if page.full_width {
        let doc_w = (width - 48.0).max(200.0);
        let ins = 24.0;
        (doc_w, ins)
    } else {
        let doc_w = (width - 72.0).clamp(200.0, 850.0);
        let ins = ((width - doc_w) / 2.0).max(12.0);
        (doc_w, ins)
    };
    let props_max_width = (document_width - block::HANDLE_WIDTH).max(280.0);
    page_changed |= title(
        ui,
        page,
        &document.properties,
        title_focus,
        inset,
        document_width,
        widget_scope,
        palette,
        &mut intents,
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
            max_width: props_max_width,
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
                max_width: props_max_width,
            },
            false,
            palette,
            &mut intents,
            &mut crate::ui::database::Drafts {
                properties: record_properties.property_drafts,
                titles: record_properties.title_drafts,
                values: record_properties.value_drafts,
            },
        );
    }
    ui.add_space(8.0);
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
    page: &mut Page,
    palette: Palette,
    breadcrumbs: &[crate::ui::BreadcrumbItem],
    intents: &mut Vec<Intent>,
    page_changed: &mut bool,
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
        for item in breadcrumbs {
            let label = if !item.icon.is_empty() {
                format!("{}  {}", item.icon, item.title)
            } else {
                item.title.clone()
            };
            let resp = ui.add(
                egui::Button::new(RichText::new(label).size(12.0).color(palette.muted))
                    .frame(false),
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                intents.push(Intent::Open(item.id, None));
            } else if resp.middle_clicked() {
                intents.push(Intent::OpenInTab(item.id));
            }
            ui.label(RichText::new("/").size(12.0).color(palette.muted));
        }
        ui.label(RichText::new(format!("{}  {}", page.icon, page.display_title())).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                let full_width_label = if page.full_width {
                    "⤡  Standard width"
                } else {
                    "⤢  Full width"
                };
                if ui.button(full_width_label).clicked() {
                    page.full_width = !page.full_width;
                    *page_changed = true;
                    ui.close_menu();
                }
                ui.separator();
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
                if page.full_width { "⤡" } else { "⤢" },
                if page.full_width {
                    palette.accent
                } else {
                    palette.muted
                },
            )
            .on_hover_text(if page.full_width {
                "Standard width"
            } else {
                "Full width"
            })
            .clicked()
            {
                page.full_width = !page.full_width;
                *page_changed = true;
            }
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
        if let Some(new_icon) = crate::ui::icon_picker_button(
            ui,
            page.id,
            &page.icon,
            29.0,
            Some(palette.accent),
        ) {
            page.icon = new_icon;
            page_changed = true;
        }
    });
    page_changed
}

#[allow(clippy::too_many_arguments)]
fn title(
    ui: &mut Ui,
    page: &mut Page,
    properties: &[crate::domain::model::PageProperty],
    title_focus: &mut bool,
    inset: f32,
    document_width: f32,
    widget_scope: Option<u64>,
    palette: Palette,
    intents: &mut Vec<Intent>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(inset);
        changed = identity(ui, page, palette);
        ui.add_space(4.0);
        let title_resp = ui.add_sized(
            [document_width - 42.0, 51.0],
            egui::TextEdit::singleline(&mut page.title)
                .id(title_widget_id(page.id, widget_scope))
                .font(FontId::proportional(34.0))
                .frame(false)
                .hint_text(UNTITLED)
                .margin(egui::Margin::ZERO),
        );
        changed |= title_resp.changed();
        if *title_focus {
            title_resp.request_focus();
            *title_focus = false;
        }
        title_resp.context_menu(|ui| {
            ui.set_min_width(240.0);
            ui.label(
                RichText::new("PAGE & PROPERTIES")
                    .font(FontId::proportional(10.5))
                    .strong()
                    .color(palette.muted),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&page.icon).size(16.0));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut page.title)
                        .hint_text(UNTITLED)
                        .desired_width(190.0),
                );
                changed |= resp.changed();
            });
            ui.separator();
            let full_width_label = if page.full_width {
                "⤡  Standard width"
            } else {
                "⤢  Full width"
            };
            if ui.button(full_width_label).clicked() {
                page.full_width = !page.full_width;
                changed = true;
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button("+  Add property", |ui| {
                for (kind, label) in crate::domain::model::PROPERTY_TYPES {
                    let icon = crate::ui::page_properties::property_icon(kind);
                    if ui.button(format!("{icon}   {label}")).clicked() {
                        let base = match kind {
                            "text" => "Notes",
                            "number" => "Number",
                            "date" => "Date",
                            "status" => "Status",
                            "select" => "Category",
                            "multi-select" => "Tags",
                            "checkbox" => "Done",
                            _ => "Property",
                        };
                        let mut name = base.to_owned();
                        let mut idx = 2usize;
                        while properties
                            .iter()
                            .any(|p| p.name.eq_ignore_ascii_case(&name))
                        {
                            name = format!("{base} {idx}");
                            idx += 1;
                        }
                        intents.push(Intent::Action(Action::AddPageProperty {
                            page: page.id,
                            name,
                            kind: kind.into(),
                            options: Vec::new(),
                        }));
                        ui.close_menu();
                    }
                }
            });
            if !properties.is_empty() {
                ui.menu_button("⚙  Edit properties", |ui| {
                    let total = properties.len();
                    for (idx, prop) in properties.iter().enumerate() {
                        let icon = crate::ui::page_properties::property_icon(&prop.kind);
                        ui.menu_button(format!("{icon}  {}", prop.name), |ui| {
                            crate::ui::page_properties::show_property_edit_panel(
                                ui, page.id, prop, idx, total, false, palette, intents,
                            );
                        });
                    }
                });
            }
            ui.menu_button("🖼  Cover", |ui| {
                if ui.button("Choose image…").clicked() {
                    intents.push(Intent::Action(Action::ChooseCoverImage(page.id)));
                    ui.close_menu();
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
                        .selectable_label(page.cover == style && page.cover_image.is_none(), label)
                        .clicked()
                    {
                        page.cover = style;
                        page.cover_image = None;
                        changed = true;
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            if ui
                .button(if page.favorite {
                    "★  Remove from Favorites"
                } else {
                    "☆  Add to Favorites"
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
        });
    });
    changed
}

fn title_widget_id(page_id: i64, widget_scope: Option<u64>) -> egui::Id {
    widget_scope.map_or_else(
        || egui::Id::new(("title", page_id)),
        |scope| egui::Id::new(("title", scope, page_id)),
    )
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
