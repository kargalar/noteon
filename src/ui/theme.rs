use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

use super::palette::Palette;

pub fn configure(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows_font = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .map(|path| path.join("Fonts/segoeui.ttf"));
    if let Some(bytes) = windows_font.and_then(|path| std::fs::read(path).ok()) {
        fonts
            .font_data
            .insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".into());
    }
    if let Some(bytes) = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .and_then(|path| std::fs::read(path.join("Fonts/seguisym.ttf")).ok())
    {
        fonts.font_data.insert(
            "segoe-symbols".into(),
            Arc::new(FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push("segoe-symbols".into());
    }
    if let Some(bytes) = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .and_then(|path| std::fs::read(path.join("Fonts/seguiemj.ttf")).ok())
    {
        fonts
            .font_data
            .insert("segoe-emoji".into(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push("segoe-emoji".into());
    }
    let font_directory = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .map(|path| path.join("Fonts"))
        .unwrap_or_default();
    for (family_name, file_name) in [
        ("noteon-bold", "segoeuib.ttf"),
        ("noteon-serif", "georgia.ttf"),
        ("noteon-serif-bold", "georgiab.ttf"),
    ] {
        let font_names = if let Ok(bytes) = std::fs::read(font_directory.join(file_name)) {
            fonts
                .font_data
                .insert(family_name.into(), Arc::new(FontData::from_owned(bytes)));
            vec![family_name.into()]
        } else {
            fonts
                .families
                .get(&FontFamily::Proportional)
                .cloned()
                .unwrap_or_default()
        };
        fonts
            .families
            .insert(FontFamily::Name(family_name.into()), font_names);
    }
    context.set_fonts(fonts);
    apply(
        context,
        false,
        None,
        None,
        1.0,
        &super::palette::CustomTheme::default(),
    );
}

pub fn apply(
    context: &egui::Context,
    dark: bool,
    background: Option<egui::Color32>,
    accent: Option<egui::Color32>,
    font_scale: f32,
    custom: &super::palette::CustomTheme,
) {
    let base = Palette::new(dark);
    let palette = base.customized(
        background.unwrap_or(base.background),
        accent.unwrap_or(base.accent),
        custom,
    );
    context.set_zoom_factor(font_scale);
    let mut style = egui::Style {
        visuals: if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        },
        ..Default::default()
    };
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.visuals.panel_fill = palette.background;
    style.visuals.window_fill = palette.background;
    style.visuals.override_text_color = Some(palette.text);
    style.visuals.extreme_bg_color = palette.background;
    style.visuals.faint_bg_color = palette.code;
    style.visuals.selection.bg_fill = palette.accent_soft;
    style.visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    style.visuals.widgets.inactive.weak_bg_fill = palette.code;
    style.visuals.widgets.inactive.bg_fill = palette.code;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    style.visuals.widgets.hovered.weak_bg_fill = palette.hover;
    style.visuals.widgets.hovered.bg_fill = palette.hover;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.active.bg_fill = palette.accent_soft;
    style.visuals.widgets.active.weak_bg_fill = palette.accent_soft;
    context.set_style(style);
}
