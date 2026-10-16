use crate::config::UNTITLED;

pub type PageId = i64;
pub type BlockId = i64;

pub const PRIORITY_LABELS: [&str; 4] = ["None", "Low", "Medium", "High"];
pub const DEFAULT_DATABASE_STATUS_OPTIONS: [&str; 3] = ["To do", "In progress", "Done"];
pub const PROPERTY_TYPES: [(&str, &str); 7] = [
    ("text", "Text"),
    ("number", "Number"),
    ("date", "Date"),
    ("status", "Status"),
    ("select", "Select"),
    ("multi-select", "Multi-select"),
    ("checkbox", "Checkbox"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockKind {
    #[default]
    Text,
    Heading1,
    Heading2,
    Heading3,
    Bullet,
    Numbered,
    Todo,
    Quote,
    Callout,
    Code,
    Divider,
    Database,
    Image,
    Video,
}

impl BlockKind {
    pub const ALL: [Self; 11] = [
        Self::Text,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Bullet,
        Self::Numbered,
        Self::Todo,
        Self::Quote,
        Self::Callout,
        Self::Code,
        Self::Divider,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Heading1 => "h1",
            Self::Heading2 => "h2",
            Self::Heading3 => "h3",
            Self::Bullet => "bullet",
            Self::Numbered => "numbered",
            Self::Todo => "todo",
            Self::Quote => "quote",
            Self::Callout => "callout",
            Self::Code => "code",
            Self::Divider => "divider",
            Self::Database => "database",
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    pub fn from_db(value: &str) -> Self {
        if value == "database" {
            return Self::Database;
        }
        if value == "image" {
            return Self::Image;
        }
        if value == "video" {
            return Self::Video;
        }
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .unwrap_or_default()
    }

    pub fn from_media_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => Some(Self::Image),
            "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" | "wmv" => Some(Self::Video),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Heading1 => "Heading 1",
            Self::Heading2 => "Heading 2",
            Self::Heading3 => "Heading 3",
            Self::Bullet => "Bulleted list",
            Self::Numbered => "Numbered list",
            Self::Todo => "To-do",
            Self::Quote => "Quote",
            Self::Callout => "Callout",
            Self::Code => "Code",
            Self::Divider => "Divider",
            Self::Database => "Database",
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }

    pub fn font_size(self) -> f32 {
        match self {
            Self::Heading1 => 28.0,
            Self::Heading2 => 23.0,
            Self::Heading3 => 19.0,
            Self::Code => 13.0,
            _ => 15.0,
        }
    }

    pub fn shortcut(value: &str) -> Option<Self> {
        let value = if value.starts_with('/') {
            value.trim_end()
        } else {
            value
        };
        match value {
            "/h1" | "# " => Some(Self::Heading1),
            "/h2" | "## " => Some(Self::Heading2),
            "/h3" | "### " => Some(Self::Heading3),
            "/todo" | "[] " => Some(Self::Todo),
            "/bullet" | "- " => Some(Self::Bullet),
            "/number" | "1. " => Some(Self::Numbered),
            "/quote" | "> " => Some(Self::Quote),
            "/code" => Some(Self::Code),
            "/callout" => Some(Self::Callout),
            "/divider" | "---" => Some(Self::Divider),
            "/text" => Some(Self::Text),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Page {
    pub id: PageId,
    pub parent_id: Option<PageId>,
    pub title: String,
    pub icon: String,
    pub favorite: bool,
    pub trashed_by: Option<PageId>,
    pub cover: i32,
    pub cover_image: Option<String>,
    pub cover_x: f32,
    pub cover_y: f32,
    pub sort_order: i64,
    pub updated_at: i64,
    pub block_count: usize,
    pub created_at: i64,
    pub priority: u8,
    pub is_database: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DatabaseView {
    #[default]
    Table,
    Gallery,
    Board,
}

impl DatabaseView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Gallery => "gallery",
            Self::Board => "board",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "gallery" => Self::Gallery,
            "board" => Self::Board,
            _ => Self::Table,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "Table",
            Self::Gallery => "Gallery",
            Self::Board => "Board",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PropertyOption {
    pub name: String,
    pub color: Option<String>,
}

impl PropertyOption {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: None,
        }
    }

    pub fn with_color(name: impl Into<String>, color: Option<String>) -> Self {
        Self {
            name: name.into(),
            color,
        }
    }

    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some((name, color_part)) = trimmed.rsplit_once("::") {
            let name = name.trim();
            let color = color_part.trim();
            if !name.is_empty() && is_valid_hex_color(color) {
                let hex = if color.starts_with('#') {
                    color.to_ascii_uppercase()
                } else {
                    format!("#{color}").to_ascii_uppercase()
                };
                return Some(Self {
                    name: name.to_owned(),
                    color: Some(hex),
                });
            }
        }
        Some(Self {
            name: trimmed.to_owned(),
            color: None,
        })
    }

    pub fn serialize(&self) -> String {
        if let Some(color) = &self.color {
            format!("{}::{}", self.name.trim(), color.trim())
        } else {
            self.name.trim().to_owned()
        }
    }
}

impl From<&str> for PropertyOption {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for PropertyOption {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

pub fn is_valid_hex_color(s: &str) -> bool {
    let s = s.strip_prefix('#').unwrap_or(s);
    s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn parse_property_options(raw: &str) -> Vec<PropertyOption> {
    raw.lines().filter_map(PropertyOption::parse).collect()
}

pub fn serialize_property_options(options: &[PropertyOption]) -> String {
    options
        .iter()
        .map(PropertyOption::serialize)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseProperty {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub options: Vec<PropertyOption>,
}

impl DatabaseProperty {
    pub fn option_color(&self, option_name: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|opt| opt.name == option_name)
            .and_then(|opt| opt.color.as_deref())
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseData {
    pub page_id: PageId,
    pub title: String,
    pub icon: String,
    pub view: DatabaseView,
    pub gallery_columns: u8,
    pub view_settings: DatabaseViewSettings,
    pub sort_key: String,
    pub sort_descending: bool,
    pub properties: Vec<DatabaseProperty>,
    pub rows: Vec<DatabaseRow>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct DatabaseViewSettings {
    pub gallery_show_cover: bool,
    pub gallery_show_title: bool,
    pub gallery_show_description: bool,
    pub gallery_show_created_at: bool,
    pub gallery_visible_properties: Option<Vec<i64>>,
    pub table_show_created_at: bool,
    pub table_visible_properties: Option<Vec<i64>>,
    pub table_column_widths: std::collections::HashMap<i64, f32>,
    pub table_name_width: Option<f32>,
    pub table_created_width: Option<f32>,
    pub table_height: Option<f32>,
    pub board_show_description: bool,
    pub board_visible_properties: Option<Vec<i64>>,
}

impl Default for DatabaseViewSettings {
    fn default() -> Self {
        Self {
            gallery_show_cover: true,
            gallery_show_title: true,
            gallery_show_description: false,
            gallery_show_created_at: true,
            gallery_visible_properties: None,
            table_show_created_at: true,
            table_visible_properties: None,
            table_column_widths: std::collections::HashMap::new(),
            table_name_width: None,
            table_created_width: None,
            table_height: None,
            board_show_description: false,
            board_visible_properties: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseRow {
    pub page: Page,
    pub preview: String,
    pub values: std::collections::HashMap<i64, String>,
}

impl Page {
    pub fn display_title(&self) -> &str {
        if self.title.trim().is_empty() {
            UNTITLED
        } else {
            &self.title
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub page_id: PageId,
    pub position: usize,
    pub kind: BlockKind,
    pub content: String,
    pub text_styles: Vec<TextStyleRange>,
    pub checked: bool,
    pub column_group: Option<i64>,
    pub column_index: usize,
    pub media_width: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct InlineTextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub font_family: Option<InlineFontFamily>,
    pub foreground: Option<[u8; 4]>,
    pub background: Option<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum InlineFontFamily {
    Serif,
    Monospace,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TextStyleRange {
    pub start: usize,
    pub end: usize,
    pub style: InlineTextStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkBlockStyleOp {
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrikethrough,
    SetFont(Option<InlineFontFamily>),
    SetTextColor(Option<[u8; 4]>),
    SetHighlight(Option<[u8; 4]>),
    ClearFormatting,
}

#[derive(Clone, Debug)]
pub struct BlockStub {
    pub id: BlockId,
    pub kind: BlockKind,
    pub chars: usize,
    pub lines: usize,
    pub column_group: Option<i64>,
    pub column_index: usize,
    pub column_width: f32,
    pub media_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPlacement {
    Before,
    After,
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub page: Page,
    pub outline: Vec<BlockStub>,
    pub properties: Vec<PageProperty>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageProperty {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub options: Vec<PropertyOption>,
    pub value: String,
}

impl PageProperty {
    pub fn option_color(&self, option_name: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|opt| opt.name == option_name)
            .and_then(|opt| opt.color.as_deref())
    }
}

#[derive(Clone, Debug)]
pub struct PageEdit {
    pub id: PageId,
    pub title: String,
    pub icon: String,
    pub cover: i32,
    pub cover_image: Option<String>,
    pub cover_x: f32,
    pub cover_y: f32,
}

impl From<&Page> for PageEdit {
    fn from(page: &Page) -> Self {
        Self {
            id: page.id,
            title: page.title.clone(),
            icon: page.icon.clone(),
            cover: page.cover,
            cover_image: page.cover_image.clone(),
            cover_x: page.cover_x,
            cover_y: page.cover_y,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SaveBatch {
    pub pages: Vec<PageEdit>,
    pub blocks: Vec<Block>,
}

impl SaveBatch {
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty() && self.blocks.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub page_id: PageId,
    pub title: String,
    pub icon: String,
    pub preview: String,
    pub position: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_option_round_trip() {
        let opt_plain = PropertyOption::parse("Feature").unwrap();
        assert_eq!(opt_plain.name, "Feature");
        assert_eq!(opt_plain.color, None);
        assert_eq!(opt_plain.serialize(), "Feature");

        let opt_colored = PropertyOption::parse("Bug::#ef4444").unwrap();
        assert_eq!(opt_colored.name, "Bug");
        assert_eq!(opt_colored.color.as_deref(), Some("#EF4444"));
        assert_eq!(opt_colored.serialize(), "Bug::#EF4444");

        let raw = "Design::#8B5CF6\nReview\nDone::#10B981";
        let parsed = parse_property_options(raw);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "Design");
        assert_eq!(parsed[0].color.as_deref(), Some("#8B5CF6"));
        assert_eq!(parsed[1].name, "Review");
        assert_eq!(parsed[1].color, None);
        assert_eq!(parsed[2].name, "Done");
        assert_eq!(parsed[2].color.as_deref(), Some("#10B981"));

        let serialized = serialize_property_options(&parsed);
        assert_eq!(serialized, raw);
    }

    #[test]
    fn database_property_option_color_lookup() {
        let prop = DatabaseProperty {
            id: 1,
            name: "Tags".into(),
            kind: "multi-select".into(),
            options: vec![
                PropertyOption::with_color("Urgent", Some("#ef4444".into())),
                PropertyOption::new("Low"),
            ],
        };
        assert_eq!(prop.option_color("Urgent"), Some("#ef4444"));
        assert_eq!(prop.option_color("Low"), None);
        assert_eq!(prop.option_color("Unknown"), None);
    }
}

