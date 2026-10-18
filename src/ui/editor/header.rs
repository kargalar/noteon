use super::block;
use crate::config::UNTITLED;
use crate::domain::model::{Document, Page, PageEdit};
use crate::ui::{Intent, ThemeSection, palette::Palette, quiet_button, small_label, text};
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
    find_query: Option<&str>,
    is_find_title_active: bool,
    history: &[crate::domain::model::PageHistorySummary],
) -> (Canvas, Vec<Intent>) {
    let mut intents = Vec::new();
    let page = &mut document.page;
    let mut page_changed = false;
    breadcrumb(ui, page, palette, breadcrumbs, &mut intents, &mut page_changed, history);
    ui.add_space(8.0);
    ui.separator();
    let width = ui.available_width();
    let has_cover = page.cover != 0 || page.cover_image.is_some();
    let cover_height = if has_cover {
        180.0
    } else {
        28.0
    };
    ui.add_space(10.0);
    let (cover_rect, mut cover_response) =
        ui.allocate_exact_size(vec2(width, cover_height), Sense::click_and_drag());
    if let Some(path) = page.cover_image.as_deref() {
        ui.painter().rect_filled(cover_rect, 6.0, palette.code);
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

    if !has_cover {
        if cover_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            ui.painter().rect_stroke(
                cover_rect.shrink(2.0),
                4.0,
                egui::Stroke::new(1.0, palette.border.linear_multiply(0.5)),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                cover_rect.center(),
                egui::Align2::CENTER_CENTER,
                "🖼  Kapak ekle",
                egui::FontId::proportional(12.0),
                palette.muted,
            );
        }
        cover_response = cover_response.on_hover_text("Kapak görseli seçmek için tıklayın");
        if cover_response.clicked() {
            intents.push(Intent::Action(Action::ChooseCoverImage(page.id)));
        }
    } else {
        if cover_response.hovered() && !cover_response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            cover_response = cover_response.on_hover_text(
                "Kapağı değiştirmek, hizalamak veya silmek için tıklayın.\nGörseli sürükleyerek de hizalayabilirsiniz.",
            );
        }
        if cover_response.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(cover_response.id.with("__context_menu")));
        }
        cover_response.context_menu(|ui| {
            ui.set_min_width(200.0);

            ui.label(
                RichText::new("DEĞİŞTİR")
                    .font(FontId::proportional(10.5))
                    .strong()
                    .color(palette.muted),
            );
            ui.add_space(2.0);
            if ui.button("🖼  Görsel seç…").clicked() {
                intents.push(Intent::Action(Action::ChooseCoverImage(page.id)));
                ui.close_menu();
            }
            ui.menu_button("🎨  Renk Seç", |ui| {
                for (style, label) in [
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
                        page_changed = true;
                        ui.close_menu();
                    }
                }
            });

            if page.cover_image.is_some() {
                ui.separator();
                ui.label(
                    RichText::new("HİZALA")
                        .font(FontId::proportional(10.5))
                        .strong()
                        .color(palette.muted),
                );
                ui.label(
                    RichText::new("Görseli sürükleyerek de hizalayabilirsiniz.")
                        .small()
                        .color(palette.muted),
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if ui.button("Üst").clicked() {
                        page.cover_y = 0.0;
                        page_changed = true;
                        ui.close_menu();
                    }
                    if ui.button("Orta").clicked() {
                        page.cover_y = 0.5;
                        page_changed = true;
                        ui.close_menu();
                    }
                    if ui.button("Alt").clicked() {
                        page.cover_y = 1.0;
                        page_changed = true;
                        ui.close_menu();
                    }
                    if ui.button("⟲ Sıfırla").clicked() {
                        page.cover_x = 0.5;
                        page.cover_y = 0.5;
                        page_changed = true;
                        ui.close_menu();
                    }
                });
            }

            ui.separator();
            if ui
                .button(RichText::new("🗑  Kapağı Kaldır").color(palette.danger))
                .clicked()
            {
                page.cover = 0;
                page.cover_image = None;
                page_changed = true;
                ui.close_menu();
            }
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
        find_query,
        is_find_title_active,
        history,
    );
    if page_changed {
        intents.push(Intent::PageChanged(PageEdit::from(&*page)));
    }
    let mut record_context = record_properties.record.map(|(data, record_page)| {
        crate::ui::page_properties::RecordPropertyContext {
            data,
            record_page,
            property_drafts: record_properties.property_drafts,
            title_drafts: record_properties.title_drafts,
            value_drafts: record_properties.value_drafts,
        }
    });

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
        record_context.as_mut(),
        false,
        palette,
        &mut intents,
    );
    ui.add_space(4.0);
    let line_y = ui.cursor().min.y;
    let line_left = inset + block::HANDLE_WIDTH;
    let line_right = inset + document_width - 4.0;
    ui.painter().line_segment(
        [egui::pos2(line_left, line_y), egui::pos2(line_right, line_y)],
        Stroke::new(1.0, palette.border),
    );
    ui.add_space(4.0);
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
    history: &[crate::domain::model::PageHistorySummary],
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
        let current_label = if page.icon.is_empty() {
            page.display_title().to_string()
        } else {
            format!("{}  {}", page.icon, page.display_title())
        };
        ui.label(RichText::new(current_label).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                ui.menu_button("🕒  Değişiklik Geçmişi", |ui| {
                    show_history_menu(ui, page.id, history, palette, intents);
                });
                ui.separator();
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
                let collapse_id = egui::Id::new(("page-properties-collapsed", page.id));
                let is_collapsed = ui
                    .ctx()
                    .data_mut(|d| d.get_persisted::<bool>(collapse_id).or_else(|| d.get_temp::<bool>(collapse_id)))
                    .unwrap_or(false);
                let toggle_label = if is_collapsed {
                    "👁  Özellikleri göster"
                } else {
                    "👁  Özellikleri gizle"
                };
                if ui.button(toggle_label).clicked() {
                    let new_state = !is_collapsed;
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(collapse_id, new_state);
                        d.insert_persisted(collapse_id, new_state);
                    });
                    ui.close_menu();
                }
                if ui.button("🔍  Sayfada ara\tCtrl + F").clicked() {
                    intents.push(Intent::FindInPage);
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
            ui.menu_button(
                RichText::new("🕒")
                    .font(FontId::proportional(12.5))
                    .color(if history.is_empty() { palette.muted } else { palette.accent }),
                |ui| {
                    show_history_menu(ui, page.id, history, palette, intents);
                },
            )
            .response
            .on_hover_text("Değişiklik Geçmişi (Son 10)");
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
    find_query: Option<&str>,
    is_find_title_active: bool,
    history: &[crate::domain::model::PageHistorySummary],
) -> bool {
    let mut changed = false;

    let title_hover_id = egui::Id::new(("title-hover-area", page.id, widget_scope));
    let was_hovered = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(title_hover_id))
        .unwrap_or_default();
    let icon_menu_open_id = egui::Id::new(("add-icon-menu-open", page.id, widget_scope));
    let was_icon_menu_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(icon_menu_open_id))
        .unwrap_or_default();
    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let title_zone = egui::Rect::from_min_size(
        egui::pos2(inset, ui.cursor().min.y),
        egui::vec2(document_width, 76.0),
    );
    let is_pointer_over = pointer_pos.map_or(false, |pos| title_zone.contains(pos));
    let show_add_button = page.icon.is_empty() && (is_pointer_over || was_hovered || was_icon_menu_open);

    let mut add_button_hovered = false;
    let mut is_icon_menu_open = false;
    if page.icon.is_empty() {
        if show_add_button {
            ui.horizontal(|ui| {
                ui.add_space(inset);
                let (new_icon, is_open, btn_hovered) = crate::ui::add_icon_picker_button(
                    ui,
                    ("add-icon-btn", page.id, widget_scope),
                    palette,
                );
                if let Some(icon) = new_icon {
                    page.icon = icon;
                    changed = true;
                }
                is_icon_menu_open = is_open;
                add_button_hovered = btn_hovered;
            });
        } else {
            ui.add_space(26.0);
        }
    }

    let title_w = if page.icon.is_empty() {
        document_width
    } else {
        document_width - 42.0
    };

    let mut title_resp_hovered = false;
    ui.horizontal(|ui| {
        ui.add_space(inset);
        if !page.icon.is_empty() {
            changed |= identity(ui, page, palette);
            ui.add_space(4.0);
        }
        let find_clean = find_query.map(|q| q.trim()).unwrap_or("");
        let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
            if !find_clean.is_empty() {
                block::layout_title_with_find(
                    ui,
                    text,
                    wrap_width,
                    FontId::proportional(34.0),
                    palette.text,
                    find_clean,
                    is_find_title_active,
                )
            } else {
                let font = FontId::proportional(34.0);
                let job = egui::text::LayoutJob::simple(text.to_string(), font, palette.text, wrap_width);
                ui.fonts(|f| f.layout_job(job))
            }
        };
        let title_edit = egui::TextEdit::singleline(&mut page.title)
            .id(title_widget_id(page.id, widget_scope))
            .font(FontId::proportional(34.0))
            .frame(false)
            .hint_text(UNTITLED)
            .margin(egui::Margin::ZERO)
            .layouter(&mut layouter);
        let title_resp = ui.add_sized([title_w, 51.0], title_edit);
        title_resp_hovered = title_resp.hovered();
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
                if let Some(new_icon) = crate::ui::icon_picker_button(
                    ui,
                    ("ctx-icon-picker", page.id, widget_scope),
                    if page.icon.is_empty() { "☺" } else { &page.icon },
                    16.0,
                    Some(palette.accent),
                ) {
                    page.icon = new_icon;
                    changed = true;
                }
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
            let collapse_id = egui::Id::new(("page-properties-collapsed", page.id));
            let is_collapsed = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<bool>(collapse_id).or_else(|| d.get_temp::<bool>(collapse_id)))
                .unwrap_or(false);
            let toggle_label = if is_collapsed {
                "👁  Özellikleri göster"
            } else {
                "👁  Özellikleri gizle"
            };
            if ui.button(toggle_label).clicked() {
                let new_state = !is_collapsed;
                ui.ctx().data_mut(|d| {
                    d.insert_temp(collapse_id, new_state);
                    d.insert_persisted(collapse_id, new_state);
                });
                ui.close_menu();
            }
            if ui.button("🔍  Sayfada ara\tCtrl + F").clicked() {
                intents.push(Intent::FindInPage);
                ui.close_menu();
            }
            ui.menu_button("🕒  Değişiklik Geçmişi", |ui| {
                show_history_menu(ui, page.id, history, palette, intents);
            });
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
            ui.separator();
            if ui.button("🎨 Temayı düzenle").clicked() {
                intents.push(Intent::OpenSettingsTheme(ThemeSection::Header));
                ui.close_menu();
            }
        });
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(title_hover_id, title_resp_hovered || add_button_hovered);
        data.insert_temp(icon_menu_open_id, is_icon_menu_open);
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

fn format_history_timestamp(timestamp: i64) -> String {
    let Some(date_time) = chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|utc| utc.with_timezone(&chrono::Local))
    else {
        return "—".to_owned();
    };
    let now = chrono::Local::now();
    let diff = now.signed_duration_since(date_time);
    if diff.num_seconds() < 60 && diff.num_seconds() >= 0 {
        return "Az önce".to_owned();
    }
    if diff.num_minutes() < 60 && diff.num_minutes() > 0 {
        return format!("{} dk önce", diff.num_minutes());
    }
    use chrono::{Datelike, Timelike};
    if date_time.date_naive() == now.date_naive() {
        format!("{:02}:{:02}", date_time.hour(), date_time.minute())
    } else {
        format!(
            "{}.{:02} {:02}:{:02}",
            date_time.day(),
            date_time.month(),
            date_time.hour(),
            date_time.minute()
        )
    }
}

pub(super) fn show_history_menu(
    ui: &mut Ui,
    page_id: crate::domain::model::PageId,
    history: &[crate::domain::model::PageHistorySummary],
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    ui.set_min_width(300.0);
    ui.label(
        RichText::new("🕒  DEĞİŞİKLİK GEÇMİŞİ")
            .font(FontId::proportional(11.0))
            .strong()
            .color(palette.muted),
    );
    ui.label(
        RichText::new("Son 10 değişiklik (Ctrl + Z / Ctrl + Shift + Z)")
            .font(FontId::proportional(10.0))
            .color(palette.muted),
    );
    ui.separator();

    if history.is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new("Henüz kayıtlı bir değişiklik geçmişi bulunmuyor.")
                .font(FontId::proportional(12.0))
                .color(palette.muted),
        );
        ui.add_space(6.0);
    } else {
        egui::ScrollArea::vertical()
            .max_height(340.0)
            .show(ui, |ui| {
                for (idx, entry) in history.iter().enumerate() {
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let icon = match entry.description.as_str() {
                            s if s.contains("taşındı") => "↕",
                            s if s.contains("silindi") => "🗑",
                            s if s.contains("eklendi") => "➕",
                            s if s.contains("türü") => "🔄",
                            s if s.contains("Görev") => "☑",
                            s if s.contains("Başlık") => "🏷",
                            s if s.contains("Özellik") => "⚙",
                            _ => "📝",
                        };
                        ui.label(
                            RichText::new(icon)
                                .font(FontId::proportional(13.0))
                                .color(palette.accent),
                        );
                        ui.vertical(|ui| {
                            let title_text = if idx == 0 {
                                format!("{} (Son)", entry.description)
                            } else {
                                entry.description.clone()
                            };
                            ui.label(RichText::new(title_text).strong().font(FontId::proportional(12.0)));
                            ui.label(
                                RichText::new(format_history_timestamp(entry.timestamp))
                                    .font(FontId::proportional(10.5))
                                    .color(palette.muted),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn = egui::Button::new(
                                RichText::new("Geri yükle")
                                    .font(FontId::proportional(11.0))
                                    .color(palette.accent),
                            );
                            if ui.add(btn).clicked() {
                                intents.push(Intent::Action(Action::RestorePageHistory {
                                    page: page_id,
                                    entry_id: entry.id,
                                }));
                                ui.close_menu();
                            }
                        });
                    });
                    ui.separator();
                }
            });
    }
}
