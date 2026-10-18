use eframe::egui::Color32;

#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct CustomTheme {
    pub sidebar: Option<String>,
    pub text: Option<String>,
    pub muted: Option<String>,
    pub border: Option<String>,
    pub accent_soft: Option<String>,
    pub hover: Option<String>,
    pub code: Option<String>,
    pub gallery_card: Option<String>,
    pub gallery_border: Option<String>,
    pub board_card: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ThemeConfig {
    pub light: CustomTheme,
    pub dark: CustomTheme,
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub background: Color32,
    pub sidebar: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub hover: Color32,
    pub danger: Color32,
    pub code: Color32,
    pub gallery_card: Color32,
    pub gallery_border: Color32,
    pub board_card: Color32,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: Color32::from_rgb(20, 20, 20),
                sidebar: Color32::from_rgb(14, 14, 14),
                text: Color32::from_rgb(230, 230, 230),
                muted: Color32::from_rgb(140, 140, 140),
                border: Color32::from_rgb(38, 38, 38),
                accent: Color32::from_rgb(210, 210, 210),
                accent_soft: Color32::from_rgb(45, 45, 45),
                hover: Color32::from_rgb(32, 32, 32),
                danger: Color32::from_rgb(232, 133, 124),
                code: Color32::from_rgb(28, 28, 28),
                gallery_card: Color32::from_rgb(14, 14, 14),
                gallery_border: Color32::from_rgb(38, 38, 38),
                board_card: Color32::from_rgb(18, 18, 18),
            }
        } else {
            Self {
                background: Color32::from_rgb(255, 254, 250),
                sidebar: Color32::from_rgb(246, 246, 240),
                text: Color32::from_rgb(43, 48, 39),
                muted: Color32::from_rgb(134, 139, 124),
                border: Color32::from_rgb(230, 232, 220),
                accent: Color32::from_rgb(80, 107, 58),
                accent_soft: Color32::from_rgb(231, 238, 220),
                hover: Color32::from_rgb(237, 240, 228),
                danger: Color32::from_rgb(175, 71, 58),
                code: Color32::from_rgb(243, 245, 236),
                gallery_card: Color32::from_rgb(246, 246, 240),
                gallery_border: Color32::from_rgb(230, 232, 220),
                board_card: Color32::from_rgb(250, 250, 246),
            }
        }
    }

    pub fn customized(
        mut self,
        background: Color32,
        accent: Color32,
        custom: &CustomTheme,
    ) -> Self {
        let brightness = (0.2126 * f32::from(background.r())
            + 0.7152 * f32::from(background.g())
            + 0.0722 * f32::from(background.b()))
            / 255.0;
        let light_foreground = brightness > 0.52;
        let contrast_palette = Self::new(!light_foreground);
        let surface = if light_foreground {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        self.background = background;
        self.sidebar = custom
            .sidebar
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or_else(|| {
                mix(
                    background,
                    surface,
                    if light_foreground { 0.035 } else { 0.25 },
                )
            });
        self.text = custom
            .text
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or(contrast_palette.text);
        self.muted = custom
            .muted
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or(contrast_palette.muted);
        self.danger = contrast_palette.danger;
        self.border = custom
            .border
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or_else(|| mix(background, self.text, 0.14));
        self.accent = accent;
        self.accent_soft = custom
            .accent_soft
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or_else(|| {
                mix(
                    background,
                    accent,
                    if light_foreground { 0.15 } else { 0.24 },
                )
            });
        self.hover = custom
            .hover
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or_else(|| mix(background, accent, 0.10));
        self.code = custom
            .code
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or_else(|| mix(background, self.text, 0.045));
        self.gallery_card = custom
            .gallery_card
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or(self.sidebar);
        self.gallery_border = custom
            .gallery_border
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or_else(|| self.border.linear_multiply(0.55));
        self.board_card = custom
            .board_card
            .as_deref()
            .and_then(Self::from_setting)
            .unwrap_or(self.sidebar);
        self
    }

    pub fn from_setting(value: &str) -> Option<Color32> {
        let hex = value.strip_prefix('#').unwrap_or(value);
        let bytes = hex.as_bytes();
        if bytes.len() != 6 {
            return None;
        }
        let pair = |high: u8, low: u8| Some(hex_digit(high)?.checked_mul(16)? + hex_digit(low)?);
        let red = pair(bytes[0], bytes[1])?;
        let green = pair(bytes[2], bytes[3])?;
        let blue = pair(bytes[4], bytes[5])?;
        Some(Color32::from_rgb(red, green, blue))
    }

    pub fn to_setting(color: Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
    }

    pub fn cover(self, style: i32) -> Color32 {
        match style {
            1 => Color32::from_rgb(214, 225, 193),
            2 => Color32::from_rgb(226, 215, 198),
            3 => Color32::from_rgb(207, 222, 226),
            4 => Color32::from_rgb(226, 209, 213),
            _ => self.background,
        }
    }
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let blend = |left: u8, right: u8| {
        (f32::from(left) + (f32::from(right) - f32::from(left)) * amount).round() as u8
    };
    Color32::from_rgb(
        blend(from.r(), to.r()),
        blend(from.g(), to.g()),
        blend(from.b(), to.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::Palette;
    use eframe::egui::Color32;

    #[test]
    fn color_settings_round_trip_as_rgb_hex() {
        let color = Color32::from_rgb(18, 133, 244);
        let setting = Palette::to_setting(color);

        assert_eq!(setting, "#1285F4");
        assert_eq!(Palette::from_setting(&setting), Some(color));
        assert_eq!(Palette::from_setting("#GG0000"), None);
        assert_eq!(Palette::from_setting("ééé"), None);
    }
}
