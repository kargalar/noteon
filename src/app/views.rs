use crate::ui::menus::PersistentMenuUi;
use eframe::egui::{self, Color32, RichText, Stroke, vec2};
use egui_dock::{DockArea, OverlayType, Style as DockStyle, TabViewer};

use crate::config;
use crate::ui::text;
use crate::ui::{
    CustomColorKey, DockSide, Intent, ThemeSection, palette::Palette, quiet_button, small_label,
};
use crate::worker::Action;

use super::{NoteonApp, tabs::PageTab};

impl NoteonApp {
    pub(super) fn show(&mut self, context: &egui::Context) {
        let palette = self.current_palette();
        let mut intents = Vec::new();
        show_window_chrome(
            context,
            palette,
            self.selected.and_then(|id| self.sidebar.page(id)),
        );
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(palette.sidebar)
                    .inner_margin(egui::Margin::symmetric(17, 7)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if let Some(error) = &self.error {
                        ui.label(RichText::new(error).small().color(palette.danger));
                        if self.saves.failed && ui.small_button(text::RETRY_SAVE).clicked() {
                            intents.push(Intent::RetrySave);
                        }
                        if ui.small_button("Ã—").clicked() {
                            self.error = None;
                        }
                    } else if let Some((notice, _)) = &self.notice {
                        small_label(ui, notice, palette.accent);
                    } else {
                        let (status, color) = if self.closing {
                            ("Saving changes before closingâ€¦", palette.muted)
                        } else if self.busy {
                            ("Workingâ€¦", palette.muted)
                        } else if self.saves.failed {
                            (
                                "Save failed Â· Your changes are still in memory",
                                palette.danger,
                            )
                        } else if !self.saves.is_idle() {
                            ("Savingâ€¦", palette.muted)
                        } else {
                            (text::LOCAL_SAVED, palette.accent)
                        };
                        small_label(ui, status, color);
                        if self.saves.failed && ui.small_button(text::RETRY_SAVE).clicked() {
                            intents.push(Intent::RetrySave);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let active_count = self
                            .sidebar
                            .pages
                            .iter()
                            .filter(|page| page.trashed_by.is_none())
                            .count();
                        small_label(
                            ui,
                            &format!(
                                "{active_count} pages    Â·    Noteon {}",
                                env!("CARGO_PKG_VERSION")
                            ),
                            palette.muted,
                        );
                    });
                });
            });
        let available_area = context.available_rect();
        let sidebar_top = available_area.top();
        let sidebar_height = available_area.height();

        if self.sidebar_pinned {
            egui::SidePanel::left("sidebar")
                .exact_width(config::SIDEBAR_WIDTH)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(palette.sidebar)
                        .inner_margin(egui::Margin::symmetric(15, 9))
                        .stroke(Stroke::new(1.0, palette.border)),
                )
                .show(context, |ui| {
                    ui.add_enabled_ui(self.ready && !self.busy && !self.closing, |ui| {
                        intents.extend(self.sidebar.show(
                            ui,
                            self.selected,
                            self.trash,
                            palette,
                            true,
                        ));
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.background).inner_margin(egui::Margin::same(8)))
            .show(context, |ui| {
                if !self.ready {
                    ui.vertical_centered(|ui| {
                        ui.add_space(180.0);
                        if self.error.is_none() {
                            ui.spinner();
                            ui.label("Opening workspaceâ€¦");
                        } else {
                            ui.heading("Could not open workspace");
                            ui.label("Check the error details and make sure the data folder is accessible.");
                            ui.label(self.data_directory.display().to_string());
                        }
                    });
                } else if self.trash {
                    intents.extend(self.show_trash(ui, palette));
                } else {
                    let (mut visible_pages, mut visible_databases) = {
                        let mut viewer = PageTabViewer {
                            editors: &mut self.editors,
                            editor_lru: &mut self.editor_lru,
                            sidebar: &self.sidebar,
                            selected: &mut self.selected,
                            pending_documents: &self.pending_documents,
                            databases: &mut self.databases,
                            database_lru: &mut self.database_lru,
                            database_loading: &mut self.database_loading,
                            property_drafts: &mut self.database_property_drafts,
                            title_drafts: &mut self.database_title_drafts,
                            value_drafts: &mut self.database_value_drafts,
                            visible_pages: Default::default(),
                            visible_databases: Default::default(),
                            busy: self.busy || self.closing,
                            palette,
                            intents: &mut intents,
                        };
                        let mut dock_style = DockStyle::from_egui(ui.style());
                        if self.dock.iter_all_tabs().take(2).count() == 1 {
                            dock_style.tab_bar.height = 0.0;
                        }
                        dock_style.overlay.feel.center_drop_coverage = 0.0;
                        dock_style.overlay.feel.window_drop_coverage = 0.0;
                        dock_style.overlay.overlay_type = OverlayType::HighlightedAreas;
                        dock_style.overlay.selection_color = egui::Color32::from_rgba_unmultiplied(
                            palette.accent.r(),
                            palette.accent.g(),
                            palette.accent.b(),
                            190,
                        );
                        DockArea::new(&mut self.dock)
                            .id(egui::Id::new("noteon-page-dock"))
                            .style(dock_style)
                            .show_inside(ui, &mut viewer);
                        (viewer.visible_pages, viewer.visible_databases)
                    };
                    if let Some(dialog_id) = self.page_dialog {
                        visible_pages.insert(dialog_id);
                        if let Some(parent) = self.sidebar.page(dialog_id).and_then(|p| p.parent_id) {
                            visible_databases.insert(parent);
                        }
                    }
                    self.evict_inactive_editors(&visible_pages);
                    self.evict_inactive_databases(&visible_databases);
                }
            });
        if !self.sidebar_pinned {
            let mouse_pos = context.input(|i| i.pointer.latest_pos());
            let pointer_down = context.input(|i| i.pointer.any_down());
            let is_open_prev = context
                .data(|d| d.get_temp::<bool>(egui::Id::new("sidebar_autohide_open")))
                .unwrap_or(false);

            let in_y_range = mouse_pos
                .is_some_and(|pos| pos.y >= sidebar_top && pos.y <= available_area.bottom());

            let in_sidebar = in_y_range
                && mouse_pos.is_some_and(|pos| {
                    if is_open_prev {
                        pos.x <= config::SIDEBAR_WIDTH + 14.0
                    } else {
                        !pointer_down && pos.x <= 14.0
                    }
                });

            let clicked_outside = context.input(|input| input.pointer.any_pressed())
                && !in_sidebar
                && !crate::ui::menus::pointer_in_menu(context);
            let is_open = if is_open_prev {
                !clicked_outside
            } else {
                in_sidebar
            };

            context.data_mut(|d| {
                d.insert_temp(egui::Id::new("sidebar_autohide_open"), is_open);
            });

            let open_t = context.animate_bool(egui::Id::new("sidebar_autohide_anim"), is_open);

            if open_t < 0.99 {
                let strip_rect = egui::Rect::from_min_max(
                    egui::pos2(0.0, sidebar_top),
                    egui::pos2(3.0, available_area.bottom()),
                );
                let strip_color = if is_open {
                    palette.accent
                } else {
                    Color32::from_rgba_unmultiplied(
                        palette.border.r(),
                        palette.border.g(),
                        palette.border.b(),
                        80,
                    )
                };
                context
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("sidebar_strip"),
                    ))
                    .rect_filled(strip_rect, 0.0, strip_color);
            }

            if open_t > 0.001 {
                let x_pos = (open_t - 1.0) * config::SIDEBAR_WIDTH;
                egui::Area::new(egui::Id::new("sidebar_overlay"))
                    .fixed_pos(egui::pos2(x_pos, sidebar_top))
                    .order(egui::Order::Foreground)
                    .constrain(false)
                    .show(context, |ui| {
                        ui.set_min_width(config::SIDEBAR_WIDTH);
                        ui.set_max_width(config::SIDEBAR_WIDTH);
                        ui.set_min_height(sidebar_height);
                        ui.set_max_height(sidebar_height);
                        egui::Frame::new()
                            .fill(palette.sidebar)
                            .inner_margin(egui::Margin::symmetric(15, 9))
                            .stroke(Stroke::new(1.0, palette.border))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.set_min_height(sidebar_height - 18.0);
                                ui.add_enabled_ui(
                                    self.ready && !self.busy && !self.closing,
                                    |ui| {
                                        intents.extend(self.sidebar.show(
                                            ui,
                                            self.selected,
                                            self.trash,
                                            palette,
                                            false,
                                        ));
                                    },
                                );
                            });
                    });
            }
        }
        if self.ready {
            self.persist_dock_layout();
        }
        intents.extend(self.search.show(context, palette));
        intents.extend(self.show_settings(context, palette));
        self.show_help(context, palette);
        intents.extend(self.show_page_dialog(context, palette));
        self.handle_intents(intents, context);
    }

    fn show_trash(&mut self, ui: &mut egui::Ui, palette: Palette) -> Vec<Intent> {
        let mut intents = Vec::new();
        ui.add_space(35.0);

        let mut pages: Vec<_> = self
            .sidebar
            .pages
            .iter()
            .filter(|page| page.trashed_by == Some(page.id))
            .collect();

        let sort_id = egui::Id::new("trash_sort_mode");
        let mut current_sort = ui.ctx().data(|d| d.get_temp::<u8>(sort_id)).unwrap_or(0);

        match current_sort {
            0 => pages.sort_by(|a, b| {
                b.updated_at
                    .cmp(&a.updated_at)
                    .then_with(|| a.id.cmp(&b.id))
            }),
            1 => pages.sort_by(|a, b| {
                a.updated_at
                    .cmp(&b.updated_at)
                    .then_with(|| a.id.cmp(&b.id))
            }),
            2 => pages.sort_by(|a, b| {
                a.display_title()
                    .to_lowercase()
                    .cmp(&b.display_title().to_lowercase())
            }),
            _ => pages.sort_by(|a, b| {
                b.display_title()
                    .to_lowercase()
                    .cmp(&a.display_title().to_lowercase())
            }),
        }

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(RichText::new("Trash").size(31.0));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Buradaki sayfalar geri yüklenebilir veya kalıcı olarak silinebilir.",
                    )
                    .color(palette.muted),
                );
            });

            if !pages.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm_id = egui::Id::new("confirm_empty_trash");
                    let confirming = ui
                        .ctx()
                        .data(|d| d.get_temp::<bool>(confirm_id))
                        .unwrap_or(false);

                    if confirming {
                        ui.horizontal(|ui| {
                            if ui
                                .button(
                                    RichText::new("Evet, tümünü sil")
                                        .color(palette.danger)
                                        .strong(),
                                )
                                .clicked()
                            {
                                intents.push(Intent::Action(Action::EmptyTrash));
                                ui.ctx().data_mut(|d| d.insert_temp(confirm_id, false));
                            }
                            if ui.button("İptal").clicked() {
                                ui.ctx().data_mut(|d| d.insert_temp(confirm_id, false));
                            }
                        });
                    } else {
                        let empty_btn = egui::Button::new(
                            RichText::new("🗑  Çöpü tamamen boşalt").color(palette.danger),
                        );
                        if ui
                            .add_enabled(!self.busy, empty_btn)
                            .on_hover_text("Çöp kutusundaki tüm sayfaları kalıcı olarak sil")
                            .clicked()
                        {
                            ui.ctx().data_mut(|d| d.insert_temp(confirm_id, true));
                        }
                    }

                    ui.add_space(12.0);

                    let sort_label = match current_sort {
                        0 => "⇅ Silinme: En yeni ▼",
                        1 => "⇅ Silinme: En eski ▲",
                        2 => "⇅ Başlık (A-Z)",
                        _ => "⇅ Başlık (Z-A)",
                    };

                    ui.persistent_menu_button(
                        RichText::new(sort_label).color(palette.muted),
                        |ui| {
                            if ui
                                .selectable_label(
                                    current_sort == 0,
                                    "Silinme: En yeni (Yeni → Eski)",
                                )
                                .clicked()
                            {
                                current_sort = 0;
                                ui.ctx().data_mut(|d| d.insert_temp(sort_id, 0));
                                ui.close_menu();
                            }
                            if ui
                                .selectable_label(
                                    current_sort == 1,
                                    "Silinme: En eski (Eski → Yeni)",
                                )
                                .clicked()
                            {
                                current_sort = 1;
                                ui.ctx().data_mut(|d| d.insert_temp(sort_id, 1));
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui
                                .selectable_label(current_sort == 2, "Başlık (A → Z)")
                                .clicked()
                            {
                                current_sort = 2;
                                ui.ctx().data_mut(|d| d.insert_temp(sort_id, 2));
                                ui.close_menu();
                            }
                            if ui
                                .selectable_label(current_sort == 3, "Başlık (Z → A)")
                                .clicked()
                            {
                                current_sort = 3;
                                ui.ctx().data_mut(|d| d.insert_temp(sort_id, 3));
                                ui.close_menu();
                            }
                        },
                    );
                });
            }
        });

        ui.add_space(20.0);

        if pages.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(RichText::new("🗑").size(36.0));
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Çöp kutusu boş.")
                        .color(palette.muted)
                        .size(15.0),
                );
            });
        } else {
            egui::ScrollArea::vertical().show_rows(ui, 62.0, pages.len(), |ui, range| {
                for index in range {
                    let page = pages[index];
                    egui::Frame::new()
                        .fill(palette.sidebar)
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .corner_radius(6)
                        .stroke(egui::Stroke::new(1.0, palette.border.linear_multiply(0.5)))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let icon = if page.icon.is_empty() {
                                    "📄"
                                } else {
                                    &page.icon
                                };
                                ui.label(RichText::new(icon).size(15.0));
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(page.display_title()).strong().size(13.5),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "Silinme: {}",
                                            format_trash_date(page.updated_at)
                                        ))
                                        .small()
                                        .color(palette.muted),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let delete_btn = egui::Button::new(
                                            RichText::new("Tamamen sil")
                                                .color(palette.danger)
                                                .size(12.0),
                                        );
                                        if ui
                                            .add_enabled(!self.busy, delete_btn)
                                            .on_hover_text("Bu sayfayı kalıcı olarak sil")
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(
                                                Action::DeletePermanently(page.id),
                                            ));
                                        }

                                        ui.add_space(6.0);

                                        let restore_btn = egui::Button::new(
                                            RichText::new("Geri yükle")
                                                .color(palette.accent)
                                                .size(12.0),
                                        );
                                        if ui
                                            .add_enabled(!self.busy, restore_btn)
                                            .on_hover_text("Bu sayfayı geri yükle")
                                            .clicked()
                                        {
                                            intents.push(Intent::Action(Action::Restore(page.id)));
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        }
        intents
    }

    fn show_settings(&mut self, context: &egui::Context, palette: Palette) -> Vec<Intent> {
        let mut intents = Vec::new();
        let mut settings_open = self.settings_open;
        egui::Window::new("Workspace settings")
            .open(&mut settings_open)
            .resizable(false)
            .collapsible(false)
            .default_width(560.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.settings_theme_tab, "Data").clicked() {
                        self.settings_theme_tab = false;
                    }
                    if ui.selectable_label(self.settings_theme_tab, "Theme").clicked() {
                        self.settings_theme_tab = true;
                    }
                });
                ui.separator();
                ui.add_space(6.0);

                if self.settings_theme_tab {
                    let active_highlight = if let Some((sec, start_time)) = self.settings_theme_highlight {
                        let elapsed = start_time.elapsed().as_secs_f32();
                        if elapsed < 3.5 {
                            context.request_repaint();
                            Some((sec, elapsed))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Görünüm & Tema").size(20.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("↺ Tüm Renkleri Sıfırla")
                                .on_hover_text("Tüm özel renk ayarlarını varsayılana döndür")
                                .clicked()
                            {
                                intents.push(Intent::ResetAllCustomTheme);
                            }
                        });
                    });
                    ui.add_space(2.0);
                    small_label(
                        ui,
                        "Renk özelleştirmeleri Açık ve Koyu mod için ayrı ayrı kaydedilir.",
                        palette.muted,
                    );
                    ui.add_space(8.0);

                    let custom = self.active_custom_theme().clone();

                    egui::ScrollArea::vertical()
                        .id_salt("theme-settings-scroll")
                        .max_height(520.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Section 1: General
                            show_theme_section(
                                ui,
                                ThemeSection::General,
                                "Genel & Temel Renkler",
                                "Tema modu, arka plan, ana vurgu ve genel kenarlıklar",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    let mut selected_dark = self.dark;
                                    let mut theme_changed = false;
                                    ui.horizontal(|ui| {
                                        ui.label("Tema Modu");
                                        theme_changed |= ui.radio_value(&mut selected_dark, false, "Açık (Light)").changed();
                                        theme_changed |= ui.radio_value(&mut selected_dark, true, "Koyu (Dark)").changed();
                                    });
                                    if theme_changed {
                                        intents.push(Intent::SetTheme(selected_dark));
                                    }

                                    let mut background = self.background_color();
                                    ui.horizontal(|ui| {
                                        if ui.color_edit_button_srgba(&mut background).changed() {
                                            intents.push(Intent::SetBackground(background));
                                        }
                                        ui.label("Arka plan rengi");
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(Palette::to_setting(background)).monospace().small().color(palette.muted));
                                        });
                                    });

                                    let mut accent = self.accent_color();
                                    ui.horizontal(|ui| {
                                        if ui.color_edit_button_srgba(&mut accent).changed() {
                                            intents.push(Intent::SetAccent(accent));
                                        }
                                        ui.label("Ana renk (Vurgu)");
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(Palette::to_setting(accent)).monospace().small().color(palette.muted));
                                        });
                                    });

                                    show_color_row(
                                        ui,
                                        "Yumuşak vurgu rengi",
                                        palette.accent_soft,
                                        custom.accent_soft.as_deref(),
                                        CustomColorKey::AccentSoft,
                                        intents,
                                        palette,
                                    );

                                    show_color_row(
                                        ui,
                                        "Genel kenarlık rengi",
                                        palette.border,
                                        custom.border.as_deref(),
                                        CustomColorKey::Border,
                                        intents,
                                        palette,
                                    );

                                    show_color_row(
                                        ui,
                                        "Üzerine gelme / Hover rengi",
                                        palette.hover,
                                        custom.hover.as_deref(),
                                        CustomColorKey::Hover,
                                        intents,
                                        palette,
                                    );

                                    let mut font_scale = self.font_scale;
                                    ui.horizontal(|ui| {
                                        ui.label("Yazı tipi boyutu");
                                        if ui.add(
                                            egui::Slider::new(
                                                &mut font_scale,
                                                config::MIN_FONT_SCALE..=config::MAX_FONT_SCALE,
                                            )
                                            .step_by(0.05)
                                            .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                                        ).changed() {
                                            intents.push(Intent::SetFontScale(font_scale));
                                        }
                                    });
                                },
                            );

                            // Section 2: Sidebar
                            show_theme_section(
                                ui,
                                ThemeSection::Sidebar,
                                "Kenar Çubuğu",
                                "Sol panel arka planı ve gezinme listesi",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    show_color_row(
                                        ui,
                                        "Kenar çubuğu arka planı",
                                        palette.sidebar,
                                        custom.sidebar.as_deref(),
                                        CustomColorKey::Sidebar,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "Başlık ve metin rengi",
                                        palette.text,
                                        custom.text.as_deref(),
                                        CustomColorKey::Text,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "İkincil / Sönük öğeler",
                                        palette.muted,
                                        custom.muted.as_deref(),
                                        CustomColorKey::Muted,
                                        intents,
                                        palette,
                                    );
                                },
                            );

                            // Section 3: Editor
                            show_theme_section(
                                ui,
                                ThemeSection::Editor,
                                "Editör & Not İçeriği",
                                "Sayfa metinleri, başlıklar ve kod blokları",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    show_color_row(
                                        ui,
                                        "Metin rengi",
                                        palette.text,
                                        custom.text.as_deref(),
                                        CustomColorKey::Text,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "İkincil / Sönük metin",
                                        palette.muted,
                                        custom.muted.as_deref(),
                                        CustomColorKey::Muted,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "Kod bloğu arka planı",
                                        palette.code,
                                        custom.code.as_deref(),
                                        CustomColorKey::Code,
                                        intents,
                                        palette,
                                    );
                                },
                            );

                            // Section 4: Header
                            show_theme_section(
                                ui,
                                ThemeSection::Header,
                                "Sayfa Başlığı & Gezinme",
                                "Sayfa üst başlığı, breadcrumb yolu ve ikonlar",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    show_color_row(
                                        ui,
                                        "Başlık metin rengi",
                                        palette.text,
                                        custom.text.as_deref(),
                                        CustomColorKey::Text,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "Gezinme / Sönük yol rengi",
                                        palette.muted,
                                        custom.muted.as_deref(),
                                        CustomColorKey::Muted,
                                        intents,
                                        palette,
                                    );
                                },
                            );

                            // Section 5: Gallery
                            show_theme_section(
                                ui,
                                ThemeSection::Gallery,
                                "Galeri Görünümü",
                                "Veritabanı galeri kartları ve çerçeveleri",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    show_color_row(
                                        ui,
                                        "Kart arka planı",
                                        palette.gallery_card,
                                        custom.gallery_card.as_deref(),
                                        CustomColorKey::GalleryCard,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "Kart kenarlık rengi",
                                        palette.gallery_border,
                                        custom.gallery_border.as_deref(),
                                        CustomColorKey::GalleryBorder,
                                        intents,
                                        palette,
                                    );
                                },
                            );

                            // Section 6: Board
                            show_theme_section(
                                ui,
                                ThemeSection::Board,
                                "Pano (Kanban) Görünümü",
                                "Pano sütunlarındaki kart öğeleri",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    show_color_row(
                                        ui,
                                        "Pano kartı arka planı",
                                        palette.board_card,
                                        custom.board_card.as_deref(),
                                        CustomColorKey::BoardCard,
                                        intents,
                                        palette,
                                    );
                                },
                            );

                            // Section 7: Table
                            show_theme_section(
                                ui,
                                ThemeSection::Table,
                                "Tablo Görünümü",
                                "Veritabanı tablosu, hücre çizgileri ve satır vurguları",
                                active_highlight,
                                palette,
                                &mut intents,
                                |ui, intents| {
                                    show_color_row(
                                        ui,
                                        "Tablo çizgileri / Kenarlık",
                                        palette.border,
                                        custom.border.as_deref(),
                                        CustomColorKey::Border,
                                        intents,
                                        palette,
                                    );
                                    show_color_row(
                                        ui,
                                        "Satır üzerine gelme (Hover)",
                                        palette.hover,
                                        custom.hover.as_deref(),
                                        CustomColorKey::Hover,
                                        intents,
                                        palette,
                                    );
                                },
                            );
                        });
                } else {
                    ui.label(RichText::new("Your data stays on this device.").size(21.0).strong());
                    ui.add_space(9.0);
                    ui.label("No account or internet connection is required. Pages, the search index, and settings are stored in a local SQLite database.");
                    ui.add_space(12.0);
                    small_label(ui, "DATA FOLDER", palette.muted);
                    let path = self.data_directory.display().to_string();
                    ui.label(RichText::new(&path).monospace().small());
                    if quiet_button(ui, "Copy folder path", palette.accent).clicked() {
                        context.copy_text(path);
                    }
                    ui.separator();
                    ui.add_enabled_ui(self.ready && !self.busy, |ui| {
                        if ui.button("Back up workspace…").clicked() {
                            intents.push(Intent::Action(Action::Backup));
                        }
                        if ui.button(text::IMPORT_MARKDOWN).clicked() {
                            intents.push(Intent::Action(Action::Import));
                        }
                    });
                    ui.add_space(8.0);
                    small_label(
                        ui,
                        "To restore a backup, close the app and copy the backup into the data folder as noteon.sqlite. Move the current database and its -wal / -shm files to another folder first.",
                        palette.muted,
                    );
                    ui.separator();
                    let editor = self.selected.and_then(|id| self.editors.get(&id));
                    let rendered_blocks = editor.map_or(0, |editor| editor.rendered_blocks);
                    let cached_blocks = editor.map_or(0, |editor| editor.cache.blocks.len());
                    small_label(
                        ui,
                        &format!(
                            "Visible blocks: {rendered_blocks}   ·   Cached blocks: {cached_blocks}   ·   Cache limit: {} blocks",
                            config::BLOCK_BATCH_SIZE * config::CACHED_BATCHES
                        ),
                        palette.muted,
                    );
                }
            });
        self.settings_open = settings_open;
        intents
    }

    fn show_help(&mut self, context: &egui::Context, palette: Palette) {
        egui::Window::new("Quick guide").open(&mut self.help_open).collapsible(false).resizable(false).default_width(480.0).show(context, |ui| {
            ui.add_space(8.0);
            egui::Grid::new("shortcuts").spacing(vec2(30.0, 12.0)).show(ui, |ui| {
                for (key, description) in [("Ctrl + N", "New page"), ("Ctrl + F", "Sayfada ara"), ("Ctrl + K", "Search all notes"), ("Ctrl + S", "Save now / retry"), ("Alt + ← / →", "Previous / next page"), ("Enter", "Split at cursor and add a block"), ("Shift + Enter", "New line in the same block"), ("Right-click the block menu", "Change type, move, or delete")] {
                    ui.label(RichText::new(key).monospace().color(palette.accent)); ui.label(description); ui.end_row();
                }
            });
            ui.add_space(13.0);
            ui.separator();
            ui.label("Type /h1, /h2, /h3, /todo, /bullet, /number, /quote, /callout, /code, or /divider in an empty block, then press Enter.");
            ui.add_space(8.0);
            small_label(ui, "# + Space, - + Space, [] + Space, and > + Space also change block type. Enter inserts a line break in code blocks.", palette.muted);
        });
    }

    fn show_page_dialog(&mut self, context: &egui::Context, palette: Palette) -> Vec<Intent> {
        let Some(dialog_page_id) = self.page_dialog else {
            return Vec::new();
        };

        if self
            .sidebar
            .page(dialog_page_id)
            .is_none_or(|p| p.trashed_by.is_some())
        {
            self.page_dialog = None;
            return Vec::new();
        }

        let mut intents = Vec::new();
        let mut close_dialog = false;
        let mut open_full_page = false;

        let screen = context.screen_rect();
        let dialog_width = (screen.width() * 0.72).clamp(580.0, 960.0);
        let dialog_height = (screen.height() * 0.85).clamp(420.0, 880.0);

        let breadcrumbs = self.sidebar.ancestors(dialog_page_id);
        let record_database_id = self
            .sidebar
            .page(dialog_page_id)
            .and_then(|page| page.parent_id)
            .filter(|parent| {
                self.sidebar
                    .page(*parent)
                    .is_some_and(|page| page.is_database)
            });
        if let Some(database_id) = record_database_id
            && !self.databases.contains_key(&database_id)
            && !self.database_loading.contains(&database_id)
        {
            intents.push(Intent::LoadDatabase(database_id));
        }

        let frame = egui::Frame::window(&context.style())
            .fill(palette.background)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(10.0)
            .inner_margin(egui::Margin::symmetric(16, 12));

        let modal = egui::Modal::new(egui::Id::new("noteon-page-dialog"))
            .backdrop_color(Color32::from_black_alpha(150))
            .frame(frame)
            .show(context, |ui| {
                ui.set_min_width(dialog_width);
                ui.set_max_width(dialog_width);
                ui.set_min_height(dialog_height);
                ui.set_max_height(dialog_height);

                ui.horizontal(|ui| {
                    let full_page_btn = egui::Button::new(
                        RichText::new("↗  Tam sayfa olarak aç")
                            .font(egui::FontId::proportional(12.5))
                            .color(palette.muted),
                    )
                    .frame(false);
                    if ui
                        .add(full_page_btn)
                        .on_hover_text("Bu sayfayı tam sekmede aç")
                        .clicked()
                    {
                        open_full_page = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            RichText::new("✕")
                                .font(egui::FontId::proportional(14.0))
                                .color(palette.muted),
                        )
                        .frame(false);
                        if ui.add(close_btn).on_hover_text("Kapat (Esc)").clicked() {
                            close_dialog = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                if let Some(editor) = self.editors.get_mut(&dialog_page_id) {
                    let mut visible_dbs = std::collections::HashSet::new();
                    if let Some(db_id) = record_database_id {
                        visible_dbs.insert(db_id);
                    }
                    let mut db_ctx = crate::ui::editor::DatabaseContext {
                        record_database_id,
                        databases: &mut self.databases,
                        loading: &mut self.database_loading,
                        property_drafts: &mut self.database_property_drafts,
                        title_drafts: &mut self.database_title_drafts,
                        value_drafts: &mut self.database_value_drafts,
                        visible: &mut visible_dbs,
                        sidebar: Some(&self.sidebar),
                    };
                    intents.extend(editor.show_with_database_context_in_tab(
                        ui,
                        palette,
                        &breadcrumbs,
                        0xFFFF_FFFF_FFFF_FFFE,
                        &mut db_ctx,
                    ));
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.spinner();
                        ui.label("Sayfa yükleniyor…");
                    });
                }
            });

        if open_full_page {
            self.page_dialog = None;
            self.open_page(dialog_page_id, None, true);
            if let Some(editor) = self.editors.get_mut(&dialog_page_id) {
                editor.title_focus = true;
            }
        } else if close_dialog || modal.should_close() {
            self.page_dialog = None;
        }

        intents
    }
}

fn show_color_row(
    ui: &mut egui::Ui,
    label: &str,
    current_color: Color32,
    custom_val: Option<&str>,
    key: CustomColorKey,
    intents: &mut Vec<Intent>,
    palette: Palette,
) {
    ui.horizontal(|ui| {
        let mut color = current_color;
        let is_custom = custom_val.is_some();
        if ui.color_edit_button_srgba(&mut color).changed() {
            intents.push(Intent::SetCustomThemeColor {
                key,
                value: Some(color),
            });
        }
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if is_custom {
                if ui
                    .small_button("↺")
                    .on_hover_text("Varsayılana döndür")
                    .clicked()
                {
                    intents.push(Intent::SetCustomThemeColor { key, value: None });
                }
                ui.label(
                    RichText::new(Palette::to_setting(current_color))
                        .monospace()
                        .small()
                        .color(palette.accent),
                );
            } else {
                ui.label(RichText::new("Varsayılan").small().color(palette.muted));
            }
        });
    });
}

fn show_theme_section<F>(
    ui: &mut egui::Ui,
    section: ThemeSection,
    title: &str,
    subtitle: &str,
    active_highlight: Option<(ThemeSection, f32)>,
    palette: Palette,
    intents: &mut Vec<Intent>,
    content: F,
) where
    F: FnOnce(&mut egui::Ui, &mut Vec<Intent>),
{
    let is_highlighted = active_highlight.map(|(s, _)| s) == Some(section);
    let elapsed = active_highlight.map(|(_, e)| e).unwrap_or(0.0);
    let pulse = if is_highlighted {
        (elapsed * 6.0).sin() * 0.5 + 0.5
    } else {
        0.0
    };

    let border_color = if is_highlighted {
        palette.accent.linear_multiply(0.5 + 0.5 * pulse)
    } else {
        palette.border
    };
    let border_width = if is_highlighted { 2.0 } else { 1.0 };
    let frame_fill = if is_highlighted {
        Color32::from_rgba_unmultiplied(
            palette.accent_soft.r(),
            palette.accent_soft.g(),
            palette.accent_soft.b(),
            (60.0 + 80.0 * pulse) as u8,
        )
    } else {
        palette.sidebar
    };

    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(12))
        .corner_radius(8.0)
        .fill(frame_fill)
        .stroke(Stroke::new(border_width, border_color));

    let frame_resp = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).strong().size(14.0));
                    if is_highlighted {
                        ui.label(
                            RichText::new("✨ Seçili Bölüm")
                                .size(11.5)
                                .color(palette.accent)
                                .strong(),
                        );
                    }
                });
                if !subtitle.is_empty() {
                    ui.label(RichText::new(subtitle).small().color(palette.muted));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("↺ Bölümü Sıfırla")
                    .on_hover_text("Bu bölümün renklerini varsayılana döndür")
                    .clicked()
                {
                    intents.push(Intent::ResetCustomThemeSection(section));
                }
            });
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        content(ui, intents);
    });

    if is_highlighted {
        frame_resp.response.scroll_to_me(Some(egui::Align::Center));
    }
    ui.add_space(8.0);
}

fn show_window_chrome(
    context: &egui::Context,
    palette: Palette,
    active_page: Option<&crate::domain::model::Page>,
) {
    const HEIGHT: f32 = 20.0;
    const CONTROL_WIDTH: f32 = 26.0;
    const CONTROL_HEIGHT: f32 = 18.0;
    const CONTROL_SPACING: f32 = 2.0;
    const CONTROL_RADIUS: f32 = 3.0;
    const RIGHT_PADDING: f32 = 4.0;

    let pointer_pos = context.input(|i| i.pointer.latest_pos());
    let is_dragging = context.input(|i| i.pointer.is_decidedly_dragging());
    let is_hover_prev = context
        .data(|d| d.get_temp::<bool>(egui::Id::new("window_chrome_hover_state")))
        .unwrap_or(false);

    let is_hovered = pointer_pos.is_some_and(|pos| {
        if is_hover_prev || is_dragging {
            pos.y >= 0.0 && pos.y <= HEIGHT + 4.0
        } else {
            pos.y >= 0.0 && pos.y <= HEIGHT
        }
    });
    context.data_mut(|d| d.insert_temp(egui::Id::new("window_chrome_hover_state"), is_hovered));

    let hover_t = context.animate_bool(egui::Id::new("window_chrome_hover"), is_hovered);

    let fill = if hover_t > 0.0 {
        Color32::from_rgba_unmultiplied(
            palette.sidebar.r(),
            palette.sidebar.g(),
            palette.sidebar.b(),
            (hover_t * 255.0) as u8,
        )
    } else {
        Color32::TRANSPARENT
    };

    let stroke = if hover_t > 0.05 {
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(
                palette.border.r(),
                palette.border.g(),
                palette.border.b(),
                (hover_t * 255.0) as u8,
            ),
        )
    } else {
        Stroke::NONE
    };

    egui::TopBottomPanel::top("window-chrome")
        .exact_height(HEIGHT)
        .frame(egui::Frame::new().fill(fill).stroke(stroke))
        .show(context, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let controls_total_width =
                    CONTROL_WIDTH * 3.0 + CONTROL_SPACING * 2.0 + RIGHT_PADDING;
                let drag_width = if hover_t > 0.05 {
                    (ui.available_width() - controls_total_width).max(0.0)
                } else {
                    ui.available_width()
                };
                let (drag_rect, drag_response) =
                    ui.allocate_exact_size(vec2(drag_width, HEIGHT), egui::Sense::click_and_drag());
                if drag_response.drag_started_by(egui::PointerButton::Primary) {
                    context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_response.double_clicked() {
                    toggle_maximized(context);
                }

                if hover_t > 0.05 {
                    if let Some(page) = active_page {
                        let text_color = Color32::from_rgba_unmultiplied(
                            palette.muted.r(),
                            palette.muted.g(),
                            palette.muted.b(),
                            (hover_t * 255.0) as u8,
                        );
                        let header_title = if page.icon.is_empty() {
                            page.display_title().to_string()
                        } else {
                            format!("{}  {}", page.icon, page.display_title())
                        };
                        ui.painter().text(
                            drag_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            header_title,
                            egui::FontId::proportional(11.0),
                            text_color,
                        );
                    }

                    if chrome_button(
                        ui,
                        "−",
                        CONTROL_WIDTH,
                        CONTROL_HEIGHT,
                        CONTROL_RADIUS,
                        palette,
                        false,
                        hover_t,
                    )
                    .clicked()
                    {
                        context.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    ui.add_space(CONTROL_SPACING);

                    let maximized =
                        context.input(|input| input.viewport().maximized.unwrap_or(false));
                    if chrome_button(
                        ui,
                        if maximized { "❐" } else { "□" },
                        CONTROL_WIDTH,
                        CONTROL_HEIGHT,
                        CONTROL_RADIUS,
                        palette,
                        false,
                        hover_t,
                    )
                    .clicked()
                    {
                        toggle_maximized(context);
                    }
                    ui.add_space(CONTROL_SPACING);

                    if chrome_button(
                        ui,
                        "×",
                        CONTROL_WIDTH,
                        CONTROL_HEIGHT,
                        CONTROL_RADIUS,
                        palette,
                        true,
                        hover_t,
                    )
                    .clicked()
                    {
                        context.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.add_space(RIGHT_PADDING);
                }
            });
        });
}

fn chrome_button(
    ui: &mut egui::Ui,
    icon: &str,
    width: f32,
    height: f32,
    radius: f32,
    palette: Palette,
    close: bool,
    alpha: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            radius,
            if close { palette.danger } else { palette.hover },
        );
    }
    let text_color = if close && response.hovered() {
        palette.background
    } else {
        Color32::from_rgba_unmultiplied(
            palette.text.r(),
            palette.text.g(),
            palette.text.b(),
            (alpha * 255.0) as u8,
        )
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(if close { 14.0 } else { 11.0 }),
        text_color,
    );
    response
}

fn toggle_maximized(context: &egui::Context) {
    let maximized = context.input(|input| input.viewport().maximized.unwrap_or(false));
    context.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

struct PageTabViewer<'a> {
    editors:
        &'a mut std::collections::HashMap<crate::domain::model::PageId, crate::ui::editor::Editor>,
    editor_lru: &'a mut std::collections::VecDeque<crate::domain::model::PageId>,
    sidebar: &'a crate::ui::sidebar::Sidebar,
    selected: &'a mut Option<crate::domain::model::PageId>,
    pending_documents:
        &'a std::collections::HashMap<u64, (crate::domain::model::PageId, Option<usize>)>,
    databases: &'a mut std::collections::HashMap<
        crate::domain::model::PageId,
        crate::domain::model::DatabaseData,
    >,
    database_lru: &'a mut std::collections::VecDeque<crate::domain::model::PageId>,
    database_loading: &'a mut std::collections::HashSet<crate::domain::model::PageId>,
    property_drafts: &'a mut std::collections::HashMap<i64, (String, String)>,
    title_drafts: &'a mut std::collections::HashMap<crate::domain::model::PageId, String>,
    value_drafts: &'a mut std::collections::HashMap<(crate::domain::model::PageId, i64), String>,
    visible_pages: std::collections::HashSet<crate::domain::model::PageId>,
    visible_databases: std::collections::HashSet<crate::domain::model::PageId>,
    busy: bool,
    palette: Palette,
    intents: &'a mut Vec<Intent>,
}

impl TabViewer for PageTabViewer<'_> {
    type Tab = PageTab;

    fn id(&mut self, page: &mut Self::Tab) -> egui::Id {
        egui::Id::new(("noteon-page-tab", page.instance_id))
    }

    fn title(&mut self, page: &mut Self::Tab) -> egui::WidgetText {
        self.sidebar.page(page.page_id).map_or_else(
            || "Page".into(),
            |page| {
                if page.icon.is_empty() {
                    page.display_title().to_string().into()
                } else {
                    format!("{} {}", page.icon, page.display_title()).into()
                }
            },
        )
    }

    fn ui(&mut self, ui: &mut egui::Ui, page: &mut Self::Tab) {
        let page_id = page.page_id;
        let target_instance = page.instance_id;
        let panel_rect = ui.max_rect();
        let drop_response = ui.interact(
            panel_rect,
            ui.id().with(("sidebar-page-drop", page.instance_id)),
            egui::Sense::hover(),
        );
        let hovered_page = drop_response.dnd_hover_payload::<crate::domain::model::PageId>();
        let pointer = ui.ctx().pointer_interact_pos();
        let drop_side = pointer.and_then(|position| dock_drop_side(panel_rect, position));
        if hovered_page.is_some()
            && let (Some(_dragged_page), Some(side)) = (hovered_page, drop_side)
        {
            let preview = dock_preview_rect(panel_rect, side);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                ui.id().with(("sidebar-page-drop-preview", target_instance)),
            ));
            painter.with_clip_rect(panel_rect).rect_filled(
                preview,
                6.0,
                egui::Color32::from_rgba_unmultiplied(
                    self.palette.accent.r(),
                    self.palette.accent.g(),
                    self.palette.accent.b(),
                    110,
                ),
            );
            painter.with_clip_rect(panel_rect).rect_stroke(
                preview,
                6.0,
                Stroke::new(2.0, self.palette.accent),
                egui::StrokeKind::Inside,
            );
            if let Some(released_page) =
                drop_response.dnd_release_payload::<crate::domain::model::PageId>()
            {
                self.intents
                    .push(Intent::DockPage(*released_page, target_instance, side));
            }
        }
        *self.selected = Some(page_id);
        self.visible_pages.insert(page_id);
        self.editor_lru.retain(|candidate| *candidate != page_id);
        self.editor_lru.push_back(page_id);
        let breadcrumbs = self.sidebar.ancestors(page_id);

        let record_database_id = self
            .sidebar
            .page(page_id)
            .and_then(|page| page.parent_id)
            .filter(|parent| {
                self.sidebar
                    .page(*parent)
                    .is_some_and(|page| page.is_database)
            });
        if let Some(database_id) = record_database_id {
            self.visible_databases.insert(database_id);
            self.database_lru
                .retain(|candidate| *candidate != database_id);
            self.database_lru.push_back(database_id);
        }
        if let Some(database_id) = record_database_id
            && !self.databases.contains_key(&database_id)
            && !self.database_loading.contains(&database_id)
        {
            self.intents.push(Intent::LoadDatabase(database_id));
        }

        if self
            .sidebar
            .page(page_id)
            .is_some_and(|page| page.is_database)
        {
            self.visible_databases.insert(page_id);
            self.database_lru.retain(|candidate| *candidate != page_id);
            self.database_lru.push_back(page_id);
            if let Some(database) = self.databases.get_mut(&page_id) {
                let page_icon = self.sidebar.page(page_id).map(|p| p.icon.clone());
                crate::ui::database::show(
                    ui,
                    database,
                    page_icon.as_deref(),
                    &breadcrumbs,
                    self.busy,
                    self.palette,
                    self.intents,
                    &mut crate::ui::database::Drafts {
                        properties: self.property_drafts,
                        titles: self.title_drafts,
                        values: self.value_drafts,
                    },
                );
            } else if self.database_loading.contains(&page_id) {
                ui.vertical_centered(|ui| {
                    ui.add_space(130.0);
                    ui.spinner();
                    ui.label("Loading database…");
                });
            } else {
                self.intents.push(Intent::LoadDatabase(page_id));
            }
            return;
        }

        if let Some(editor) = self.editors.get_mut(&page_id)
            && editor
                .document
                .as_ref()
                .is_some_and(|document| document.page.id == page_id)
        {
            self.intents
                .extend(editor.show_with_database_context_in_tab(
                    ui,
                    self.palette,
                    &breadcrumbs,
                    target_instance,
                    &mut crate::ui::editor::DatabaseContext {
                        record_database_id,
                        databases: self.databases,
                        loading: self.database_loading,
                        property_drafts: self.property_drafts,
                        title_drafts: self.title_drafts,
                        value_drafts: self.value_drafts,
                        visible: &mut self.visible_databases,
                        sidebar: Some(self.sidebar),
                    },
                ));
        } else if self
            .pending_documents
            .values()
            .any(|(pending_page, _)| *pending_page == page_id)
        {
            ui.vertical_centered(|ui| {
                ui.add_space(130.0);
                ui.spinner();
                ui.label("Opening pageâ€¦");
            });
        } else {
            self.intents.push(Intent::Open(page_id, None));
        }
    }

    fn closeable(&mut self, _page: &mut Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, page: &mut Self::Tab) -> bool {
        self.intents.push(Intent::ClosePageTab(page.page_id));
        true
    }

    fn scroll_bars(&self, _page: &Self::Tab) -> [bool; 2] {
        [false, false]
    }
}

fn dock_drop_side(rect: egui::Rect, pointer: egui::Pos2) -> Option<DockSide> {
    if !rect.contains(pointer) {
        return None;
    }
    let horizontal = (pointer.x - rect.left()) / rect.width().max(1.0);
    let vertical = (pointer.y - rect.top()) / rect.height().max(1.0);
    let candidates = [
        (horizontal, DockSide::Left),
        (1.0 - horizontal, DockSide::Right),
        (vertical, DockSide::Top),
        (1.0 - vertical, DockSide::Bottom),
    ];
    candidates
        .into_iter()
        .filter(|(distance, _)| *distance <= 0.24)
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, side)| side)
}

fn dock_preview_rect(rect: egui::Rect, side: DockSide) -> egui::Rect {
    match side {
        DockSide::Left => {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y))
        }
        DockSide::Right => {
            egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max)
        }
        DockSide::Top => {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.center().y))
        }
        DockSide::Bottom => {
            egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.center().y), rect.max)
        }
    }
}

fn format_trash_date(timestamp: i64) -> String {
    let Some(date_time) =
        chrono::DateTime::from_timestamp(timestamp, 0).map(|utc| utc.with_timezone(&chrono::Local))
    else {
        return "—".to_owned();
    };
    use chrono::{Datelike, Timelike};
    let month = match date_time.month() {
        1 => "Oca",
        2 => "Şub",
        3 => "Mar",
        4 => "Nis",
        5 => "May",
        6 => "Haz",
        7 => "Tem",
        8 => "Ağu",
        9 => "Eyl",
        10 => "Eki",
        11 => "Kas",
        12 => "Ara",
        _ => "",
    };
    let today = chrono::Local::now().date_naive();
    if date_time.date_naive() == today {
        format!("Bugün {:02}:{:02}", date_time.hour(), date_time.minute())
    } else if date_time.year() == today.year() {
        format!(
            "{} {} {:02}:{:02}",
            date_time.day(),
            month,
            date_time.hour(),
            date_time.minute()
        )
    } else {
        format!(
            "{} {} {} {:02}:{:02}",
            date_time.day(),
            month,
            date_time.year(),
            date_time.hour(),
            date_time.minute()
        )
    }
}
