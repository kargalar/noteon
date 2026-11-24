use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle,
};

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
        let mut font_names = fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        if let Ok(bytes) = std::fs::read(font_directory.join(file_name)) {
            fonts
                .font_data
                .insert(family_name.into(), Arc::new(FontData::from_owned(bytes)));
            font_names.insert(0, family_name.into());
        }
        fonts
            .families
            .insert(FontFamily::Name(family_name.into()), font_names);
    }
    context.set_fonts(fonts);
    apply(
        context,
        true,
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
    context.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
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
        .insert(TextStyle::Button, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(18.0, FontFamily::Name("noteon-bold".into())),
    );
    style.animation_time = animation_time();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 27.0;
    style.spacing.window_margin = Margin::same(18);
    style.spacing.menu_margin = Margin::same(8);
    style.visuals.panel_fill = palette.background;
    style.visuals.window_fill = palette.sidebar;
    style.visuals.window_stroke = Stroke::new(1.0_f32, palette.border);
    style.visuals.window_corner_radius = CornerRadius::same(18);
    style.visuals.menu_corner_radius = CornerRadius::same(12);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 3,
        color: Color32::from_black_alpha(110),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 7],
        blur: 23,
        spread: 2,
        color: Color32::from_black_alpha(96),
    };
    style.visuals.extreme_bg_color = palette.field;
    style.visuals.faint_bg_color = palette.code;
    style.visuals.selection.bg_fill = palette.accent_soft;
    style.visuals.selection.stroke = Stroke::new(2.0_f32, palette.accent);
    style.visuals.text_cursor.stroke.color = palette.text;

    let widgets = &mut style.visuals.widgets;
    widgets.noninteractive.corner_radius = CornerRadius::same(12);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    widgets.noninteractive.fg_stroke.color = palette.text;
    for (visuals, fill, outline, width) in [
        (
            &mut widgets.inactive,
            palette.field,
            palette.border,
            1.0_f32,
        ),
        (&mut widgets.hovered, palette.hover, palette.hover, 1.0),
        (
            &mut widgets.active,
            palette.accent_soft,
            palette.accent,
            2.0,
        ),
        (&mut widgets.open, palette.accent_soft, palette.accent, 1.0),
    ] {
        visuals.corner_radius = CornerRadius::same(9);
        visuals.weak_bg_fill = fill;
        visuals.bg_fill = fill;
        visuals.bg_stroke = Stroke::new(width, outline);
        visuals.fg_stroke.color = palette.text;
        visuals.expansion = 0.0;
    }
    context.set_style(style);
}

fn animation_time() -> f32 {
    #[cfg(windows)]
    if windows::UI::ViewManagement::UISettings::new()
        .and_then(|settings| settings.AnimationsEnabled())
        == Ok(false)
    {
        return 0.0;
    }
    0.18
}

pub(crate) fn refresh_motion(context: &egui::Context) {
    if context.input(|input| input.events.contains(&egui::Event::WindowFocused(true))) {
        let duration = animation_time();
        context.style_mut(|style| style.animation_time = duration);
    }
}
