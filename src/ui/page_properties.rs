mod appearance;
mod date;
mod drafts;
mod property_menu;
mod select;
mod type_picker;
mod value;

use crate::ui::menus::{PersistentContextMenu, PersistentMenuUi};
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Ui, vec2};

use crate::domain::model::{PageId, PageProperty};
use crate::ui::{Intent, palette::Palette};
use crate::worker::Action;

use appearance::blend_color;
pub use appearance::{
    PRESET_OPTION_COLORS, option_base_color, option_pill_colors, paint_property_icon,
    property_icon, resolve_option_color, show_color_palette_grid,
};
pub use date::{format_iso_date_pretty, show_date_picker_popup};
pub use property_menu::{show_add_property_panel, show_property_edit_panel};
pub use type_picker::{
    default_name_for_kind, show_property_type_picker, show_property_type_selector_button,
    type_label,
};
pub(crate) use value::layout_property_value;
use value::property_value_editor;

pub const LABEL_COLUMN_WIDTH: f32 = 168.0;
const ROW_MIN_HEIGHT: f32 = 30.0;

#[derive(Clone, Copy)]
pub struct PagePropertyTarget {
    pub page: PageId,
    pub widget_scope: Option<u64>,
    pub indent: f32,
    pub max_width: f32,
}

pub struct RecordPropertyContext<'a> {
    pub data: &'a crate::domain::model::DatabaseData,
    pub record_page: i64,
    pub property_drafts: &'a mut std::collections::HashMap<i64, (String, String)>,
    pub title_drafts: &'a mut std::collections::HashMap<i64, String>,
    pub value_drafts: &'a mut std::collections::HashMap<(i64, i64), String>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the public page properties API"
)]
pub fn show(
    ui: &mut Ui,
    target: PagePropertyTarget,
    created_at: i64,
    properties: &[PageProperty],
    mut record_context: Option<&mut RecordPropertyContext<'_>>,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let PagePropertyTarget {
        page,
        widget_scope,
        indent,
        max_width,
    } = target;
    let scope_id = widget_scope.unwrap_or_default();
    let section_width = max_width.clamp(300.0, 680.0);
    let collapse_id = egui::Id::new(("page-properties-collapsed", page));
    let mut collapsed = ui
        .ctx()
        .data_mut(|d| {
            d.get_persisted::<bool>(collapse_id)
                .or_else(|| d.get_temp::<bool>(collapse_id))
        })
        .unwrap_or(false);

    let section_hover_id = egui::Id::new(("page-properties-section-hover", page, scope_id));
    let was_section_hovered = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(section_hover_id))
        .unwrap_or(false);

    let db_props_len = record_context
        .as_ref()
        .map(|ctx| ctx.data.properties.len())
        .unwrap_or(0);
    let total_count = properties.len() + db_props_len;

    ui.horizontal(|ui| {
        ui.add_space(indent);
        let section_resp = ui.vertical(|ui| {
            ui.set_max_width(section_width);
            ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
            ui.push_id(("page-properties", page, scope_id), |ui| {
                // Section header bar
                ui.horizontal(|ui| {
                    let arrow = if collapsed { "▸" } else { "▾" };
                    let count_text = if total_count == 0 {
                        format!("{arrow}  PROPERTIES")
                    } else {
                        format!("{arrow}  PROPERTIES  ·  {total_count}")
                    };
                    let header_btn = ui
                        .add(
                            egui::Button::new(
                                RichText::new(count_text)
                                    .font(FontId::proportional(11.0))
                                    .strong()
                                    .color(palette.muted),
                            )
                            .frame(false),
                        )
                        .on_hover_text(if collapsed {
                            "Özellikleri göster / genişlet · Sağ tık: Menü"
                        } else {
                            "Özellikleri gizle / daralt · Sağ tık: Menü"
                        });

                    if header_btn.clicked() {
                        collapsed = !collapsed;
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(collapse_id, collapsed);
                            d.insert_persisted(collapse_id, collapsed);
                        });
                    }

                    header_btn.persistent_context_menu(|ui| {
                        show_section_context_menu(
                            ui,
                            PropertySection {
                                page,
                                scope_id,
                                properties,
                                record_context: record_context.as_deref_mut(),
                            },
                            &mut collapsed,
                            collapse_id,
                            busy,
                            palette,
                            intents,
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let toggle_btn = ui
                            .add(
                                egui::Button::new(
                                    RichText::new("👁")
                                        .font(FontId::proportional(12.5))
                                        .color(palette.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text(if collapsed {
                                "Özellikleri göster"
                            } else {
                                "Özellikleri gizle"
                            });

                        if toggle_btn.clicked() {
                            collapsed = !collapsed;
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(collapse_id, collapsed);
                                d.insert_persisted(collapse_id, collapsed);
                            });
                        }
                    });
                });

                if collapsed {
                    return;
                }

                ui.add_space(2.0);

                // Created date row
                show_created_row(ui, created_at, section_width, palette);

                // Database property rows (if page is a database record)
                if let Some(ref mut record_ctx) = record_context {
                    let data = record_ctx.data;
                    let record_page = record_ctx.record_page;
                    let mut drafts = crate::ui::database::Drafts {
                        properties: record_ctx.property_drafts,
                        titles: record_ctx.title_drafts,
                        values: record_ctx.value_drafts,
                    };
                    crate::ui::database::show_record_property_rows(
                        ui,
                        data,
                        record_page,
                        section_width,
                        busy,
                        palette,
                        intents,
                        &mut drafts,
                    );
                }

                // Custom property rows
                let total = properties.len();
                for (index, property) in properties.iter().enumerate() {
                    show_property_row(
                        ui,
                        page,
                        scope_id,
                        created_at,
                        property,
                        index,
                        total,
                        section_width,
                        busy,
                        palette,
                        intents,
                    );
                }

                // Bottom quick "+ Add property" button row
                let add_row_id = egui::Id::new(("add-prop-bottom-row", page, scope_id));
                let add_menu_open_id = egui::Id::new(("add-prop-menu-open", page, scope_id));
                let was_hovered = ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(add_row_id))
                    .unwrap_or(false);
                let was_menu_open = ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(add_menu_open_id))
                    .unwrap_or(false);

                let is_visible = was_section_hovered || was_hovered || was_menu_open;
                let is_highlighted = was_hovered || was_menu_open;
                let row_bg = if is_highlighted {
                    blend_color(palette.background, palette.hover, 0.55)
                } else {
                    Color32::TRANSPARENT
                };

                let mut is_now_open = false;
                let row_resp = egui::Frame::new()
                    .fill(row_bg)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.set_min_width(section_width - 12.0);
                        ui.horizontal(|ui| {
                            if is_visible {
                                let text_color = if is_highlighted {
                                    palette.text
                                } else {
                                    palette.muted
                                };
                                let menu_resp = ui.persistent_menu_button(
                                    RichText::new("+  Add property")
                                        .font(FontId::proportional(12.5))
                                        .color(text_color),
                                    |ui| {
                                        if let Some(ref record_ctx) = record_context {
                                            crate::ui::database::add_property_menu(
                                                ui,
                                                record_ctx.data,
                                                intents,
                                            );
                                        } else {
                                            show_add_property_panel(
                                                ui, page, scope_id, properties, busy, palette,
                                                intents,
                                            );
                                        }
                                    },
                                );
                                if menu_resp.inner.is_some() {
                                    is_now_open = true;
                                }
                                let ctx_resp = menu_resp.response.persistent_context_menu(|ui| {
                                    if let Some(ref record_ctx) = record_context {
                                        crate::ui::database::add_property_menu(
                                            ui,
                                            record_ctx.data,
                                            intents,
                                        );
                                    } else {
                                        show_add_property_panel(
                                            ui, page, scope_id, properties, busy, palette, intents,
                                        );
                                    }
                                });
                                if ctx_resp.is_some() {
                                    is_now_open = true;
                                }
                            } else {
                                let dummy = RichText::new("+  Add property")
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::TRANSPARENT);
                                ui.label(dummy);
                            }
                        });
                    });

                let hovered_now = ui.rect_contains_pointer(row_resp.response.rect);
                if hovered_now != was_hovered || is_now_open != was_menu_open {
                    ui.ctx().request_repaint();
                }
                ui.ctx().data_mut(|d| {
                    d.insert_temp(add_row_id, hovered_now);
                    d.insert_temp(add_menu_open_id, is_now_open);
                });
            });
        });

        let section_hit_rect = section_resp.response.rect.expand2(vec2(0.0, 24.0));
        let section_hovered = ui.rect_contains_pointer(section_hit_rect);
        if section_hovered != was_section_hovered {
            ui.ctx().request_repaint();
        }
        ui.ctx().data_mut(|d| {
            d.insert_temp(section_hover_id, section_hovered);
        });
    });
    ui.add_space(6.0);
}

struct PropertySection<'a, 'drafts> {
    page: PageId,
    scope_id: u64,
    properties: &'a [PageProperty],
    record_context: Option<&'a mut RecordPropertyContext<'drafts>>,
}

fn show_section_context_menu(
    ui: &mut Ui,
    section: PropertySection<'_, '_>,
    collapsed: &mut bool,
    collapse_id: egui::Id,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let PropertySection {
        page,
        scope_id,
        properties,
        record_context,
    } = section;
    ui.set_min_width(230.0);
    ui.label(
        RichText::new("MANAGE PROPERTIES")
            .font(FontId::proportional(10.5))
            .strong()
            .color(palette.muted),
    );
    ui.add_space(4.0);
    let toggle_label = if *collapsed {
        "👁  Özellikleri göster"
    } else {
        "👁  Özellikleri gizle"
    };
    if ui.button(toggle_label).clicked() {
        *collapsed = !*collapsed;
        ui.ctx().data_mut(|d| {
            d.insert_temp(collapse_id, *collapsed);
            d.insert_persisted(collapse_id, *collapsed);
        });
        ui.close_menu();
    }
    ui.separator();
    if let Some(record_ctx) = record_context {
        ui.persistent_menu_button("+  Add new property", |ui| {
            crate::ui::database::add_property_menu(ui, record_ctx.data, intents);
        });
        if !record_ctx.data.properties.is_empty() {
            ui.separator();
            ui.label(
                RichText::new("Existing properties (click to edit)")
                    .small()
                    .color(palette.muted),
            );
            let mut drafts = crate::ui::database::Drafts {
                properties: record_ctx.property_drafts,
                titles: record_ctx.title_drafts,
                values: record_ctx.value_drafts,
            };
            for prop in &record_ctx.data.properties {
                let icon = property_icon(&prop.kind);
                ui.persistent_menu_button(format!("{icon}  {}", prop.name), |ui| {
                    crate::ui::database::show_property_edit_menu(
                        ui,
                        prop,
                        busy,
                        palette,
                        intents,
                        &mut drafts,
                    );
                });
            }
        }
        if !properties.is_empty() {
            let total = properties.len();
            for (idx, prop) in properties.iter().enumerate() {
                let icon = property_icon(&prop.kind);
                ui.persistent_menu_button(format!("{icon}  {}", prop.name), |ui| {
                    show_property_edit_panel(ui, page, prop, idx, total, busy, palette, intents);
                });
            }
        }
    } else {
        ui.persistent_menu_button("+  Add new property", |ui| {
            show_add_property_panel(ui, page, scope_id, properties, busy, palette, intents);
        });
        if !properties.is_empty() {
            ui.separator();
            ui.label(
                RichText::new("Existing properties (click to edit)")
                    .small()
                    .color(palette.muted),
            );
            let total = properties.len();
            for (idx, prop) in properties.iter().enumerate() {
                let icon = property_icon(&prop.kind);
                ui.persistent_menu_button(format!("{icon}  {}", prop.name), |ui| {
                    show_property_edit_panel(ui, page, prop, idx, total, busy, palette, intents);
                });
            }
        }
    }
}

fn show_created_row(ui: &mut Ui, created_at: i64, section_width: f32, palette: Palette) {
    let created_row_id = ui.id().with("created-row-hover");
    let was_hovered = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(created_row_id))
        .unwrap_or(false);
    let bg_fill = if was_hovered {
        blend_color(palette.background, palette.hover, 0.55)
    } else {
        Color32::TRANSPARENT
    };
    let value_width = (section_width - LABEL_COLUMN_WIDTH - 28.0).max(120.0);

    let row_resp = egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(0, 1))
        .show(ui, |ui| {
            ui.set_min_width(section_width - 12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Left container: Created label
                egui::Frame::new()
                    .fill(bg_fill)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(LABEL_COLUMN_WIDTH - 12.0, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new("🕒")
                                        .font(FontId::proportional(12.5))
                                        .color(palette.muted),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("Created")
                                        .font(FontId::proportional(13.0))
                                        .color(palette.muted),
                                );
                            },
                        );
                    });

                // Right container: Created date
                egui::Frame::new()
                    .fill(bg_fill)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(value_width - 12.0, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(crate::ui::database::format_date(created_at))
                                        .font(FontId::proportional(13.0))
                                        .color(palette.muted),
                                );
                            },
                        );
                    });
            });
        });

    let hovered_now = ui.rect_contains_pointer(row_resp.response.rect);
    if hovered_now != was_hovered {
        ui.ctx().request_repaint();
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(created_row_id, hovered_now));
}

#[allow(clippy::too_many_arguments)]
fn show_property_row(
    ui: &mut Ui,
    page: PageId,
    scope_id: u64,
    created_at: i64,
    property: &PageProperty,
    index: usize,
    total: usize,
    section_width: f32,
    busy: bool,
    palette: Palette,
    intents: &mut Vec<Intent>,
) {
    let row_id = ui.id().with(("prop-row", page, scope_id, property.id));
    let is_row_hovered = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(row_id))
        .unwrap_or(false);

    let bg_fill = if is_row_hovered {
        blend_color(palette.background, palette.hover, 0.55)
    } else {
        Color32::TRANSPARENT
    };

    let value_width = (section_width - LABEL_COLUMN_WIDTH - 28.0).max(120.0);

    let frame_resp = egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(0, 1))
        .show(ui, |ui| {
            ui.set_min_width(section_width - 12.0);
            ui.push_id(("page-prop-row-scope", property.id), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Left container: Property Header Button
                    egui::Frame::new()
                        .fill(bg_fill)
                        .corner_radius(5)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            show_property_header_button(
                                ui,
                                page,
                                property,
                                index,
                                total,
                                busy,
                                palette,
                                is_row_hovered,
                                intents,
                            );
                        });

                    // Right container: Property Value Editor
                    egui::Frame::new()
                        .fill(bg_fill)
                        .corner_radius(5)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(
                                vec2(value_width - 12.0, ROW_MIN_HEIGHT - 6.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    property_value_editor(
                                        ui,
                                        page,
                                        created_at,
                                        property,
                                        value_width - 12.0,
                                        busy,
                                        palette,
                                        intents,
                                    );
                                },
                            );
                        });
                });
            });
        });

    let hovered_now = ui.rect_contains_pointer(frame_resp.response.rect);
    if hovered_now != is_row_hovered {
        ui.ctx().request_repaint();
    }
    ui.ctx().data_mut(|d| d.insert_temp(row_id, hovered_now));
}

#[allow(clippy::too_many_arguments)]
fn show_property_header_button(
    ui: &mut Ui,
    page: PageId,
    property: &PageProperty,
    index: usize,
    total: usize,
    busy: bool,
    palette: Palette,
    row_hovered: bool,
    intents: &mut Vec<Intent>,
) {
    let icon = property_icon(&property.kind);
    let label_text = format!("{icon}   {}", property.name);

    ui.allocate_ui_with_layout(
        vec2(LABEL_COLUMN_WIDTH - 12.0, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.scope(|ui| {
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                ui.spacing_mut().button_padding = vec2(4.0, 3.0);

                let menu_resp = ui.persistent_menu_button(
                    RichText::new(label_text)
                        .font(FontId::proportional(13.0))
                        .color(if row_hovered {
                            palette.text
                        } else {
                            palette.muted
                        }),
                    |ui| {
                        show_property_edit_panel(
                            ui, page, property, index, total, busy, palette, intents,
                        );
                    },
                );
                menu_resp
                    .response
                    .on_hover_text(format!(
                        "{} ({}) · Click or right-click to edit property",
                        property.name,
                        type_label(&property.kind)
                    ))
                    .persistent_context_menu(|ui| {
                        show_property_edit_panel(
                            ui, page, property, index, total, busy, palette, intents,
                        );
                    });
            });
        },
    );
}

fn set_value(intents: &mut Vec<Intent>, page: PageId, property: i64, value: String) {
    intents.push(Intent::Action(Action::SetPagePropertyValue {
        page,
        property,
        value,
    }));
}
