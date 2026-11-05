use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, Result};

pub const APP_NAME: &str = "Noteon";
pub const DATABASE_FILE: &str = "noteon.sqlite";
pub const WINDOW_TITLE: &str = "Noteon — Make room for your thoughts.";
pub const UNTITLED: &str = "Untitled";
pub const BLOCK_BATCH_SIZE: usize = 128;
pub const CACHED_BATCHES: usize = 8;
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(450);
pub const SEARCH_DELAY: Duration = Duration::from_millis(180);
pub const SEARCH_LIMIT: usize = 50;
pub const MAX_IMPORT_BYTES: u64 = 16 * 1024 * 1024;
pub const SIDEBAR_WIDTH: f32 = 258.0;
pub const ASSISTANT_PANEL_WIDTH: f32 = 380.0;
pub const ASSISTANT_PANEL_MIN_WIDTH: f32 = 280.0;
pub const MIN_FONT_SCALE: f32 = 0.8;
pub const MAX_FONT_SCALE: f32 = 1.5;
pub const DEFAULT_FONT_SCALE: f32 = 1.0;
pub const COLOR_ICONS: &[(&str, &str)] = &[
    ("smile gülen gülümse", "😀"),
    ("grin sırıtan", "😃"),
    ("joy kahkaha sevinç", "😂"),
    ("laugh gülme", "😆"),
    ("wink göz kırpma", "😉"),
    ("love aşık", "😍"),
    ("heart eyes kalpli göz", "🥰"),
    ("thinking düşünen", "🤔"),
    ("idea fikir yıldızlı", "🤩"),
    ("cool havalı gözlük", "😎"),
    ("relaxed rahat mutlu", "😊"),
    ("blush utangaç", "☺️"),
    ("sunglasses bıyıklı gözlüklü", "🥸"),
    ("sleep uyku", "😴"),
    ("cry ağlayan", "😢"),
    ("sad üzgün", "🙁"),
    ("worried endişeli", "😟"),
    ("angry kızgın", "😠"),
    ("fire ateş alev", "🔥"),
    ("heart kalp kırmızı", "❤️"),
    ("orange heart turuncu kalp", "🧡"),
    ("yellow heart sarı kalp", "💛"),
    ("green heart yeşil kalp", "💚"),
    ("blue heart mavi kalp", "💙"),
    ("purple heart mor kalp", "💜"),
    ("black heart siyah kalp", "🖤"),
    ("white heart beyaz kalp", "🤍"),
    ("broken heart kırık kalp", "💔"),
    ("star yıldız", "⭐"),
    ("glowing star parlayan yıldız", "🌟"),
    ("shooting star kayan yıldız", "🌠"),
    ("crescent hilal ay", "🌙"),
    ("sun güneş", "☀️"),
    ("cloud bulut", "☁️"),
    ("rainbow gökkuşağı", "🌈"),
    ("lightning şimşek yıldırım", "⚡"),
    ("snowflake kar tanesi", "❄️"),
    ("comet kuyruklu yıldız", "☄️"),
    ("earth dünya gezegen", "🌍"),
    ("moon dolunay ay", "🌕"),
    ("flower çiçek", "🌸"),
    ("blossom papatya çiçek", "🌼"),
    ("rose gül", "🌹"),
    ("tulip lale", "🌷"),
    ("sunflower ayçiçeği", "🌻"),
    ("four leaf clover dört yapraklı yonca", "🍀"),
    ("leaf yaprak", "🍃"),
    ("tree ağaç", "🌳"),
    ("palm palmiye", "🌴"),
    ("cactus kaktüs", "🌵"),
    ("mushroom mantar", "🍄"),
    ("herb ot bitki", "🌿"),
    ("seedling filiz fidan", "🌱"),
    ("maple leaf akçaağaç yaprağı", "🍁"),
    ("cat kedi", "🐱"),
    ("dog köpek", "🐶"),
    ("fox tilki", "🦊"),
    ("bear ayı", "🐻"),
    ("panda panda", "🐼"),
    ("koala koala", "🐨"),
    ("lion aslan", "🦁"),
    ("tiger kaplan", "🐯"),
    ("rabbit tavşan", "🐰"),
    ("mouse fare", "🐭"),
    ("hamster hamster", "🐹"),
    ("unicorn tekboynuz", "🦄"),
    ("butterfly kelebek", "🦋"),
    ("bee arı", "🐝"),
    ("ladybug uğur böceği", "🐞"),
    ("turtle kaplumbağa", "🐢"),
    ("fish balık", "🐟"),
    ("dolphin yunus", "🐬"),
    ("whale balina", "🐳"),
    ("octopus ahtapot", "🐙"),
    ("bird kuş", "🐦"),
    ("penguin penguen", "🐧"),
    ("eagle kartal", "🦅"),
    ("owl baykuş", "🦉"),
    ("food yemek tabak", "🍽️"),
    ("apple elma", "🍎"),
    ("pear armut", "🍐"),
    ("peach şeftali", "🍑"),
    ("cherry kiraz", "🍒"),
    ("strawberry çilek", "🍓"),
    ("grapes üzüm", "🍇"),
    ("watermelon karpuz", "🍉"),
    ("lemon limon", "🍋"),
    ("banana muz", "🍌"),
    ("avocado avokado", "🥑"),
    ("carrot havuç", "🥕"),
    ("pizza pizza", "🍕"),
    ("burger hamburger", "🍔"),
    ("sushi suşi", "🍣"),
    ("bread ekmek", "🍞"),
    ("cake pasta kek", "🎂"),
    ("cookie kurabiye", "🍪"),
    ("coffee kahve", "☕"),
    ("tea çay", "🍵"),
    ("water su damla", "💧"),
    ("sports futbol top", "⚽"),
    ("basketball basketbol", "🏀"),
    ("football amerikan futbolu", "🏈"),
    ("baseball beyzbol", "⚾"),
    ("tennis tenis", "🎾"),
    ("volleyball voleybol", "🏐"),
    ("gaming oyun konsol", "🎮"),
    ("trophy kupa ödül", "🏆"),
    ("medal madalya", "🏅"),
    ("target hedef", "🎯"),
    ("dice zar", "🎲"),
    ("puzzle bulmaca yapboz", "🧩"),
    ("music müzik nota", "🎵"),
    ("notes notalar müzik", "🎶"),
    ("guitar gitar", "🎸"),
    ("piano piyano", "🎹"),
    ("microphone mikrofon", "🎤"),
    ("headphones kulaklık", "🎧"),
    ("movie film sinema", "🎬"),
    ("camera kamera fotoğraf", "📷"),
    ("art sanat palet boya", "🎨"),
    ("paintbrush fırça boya", "🖌️"),
    ("book kitap", "📚"),
    ("open book açık kitap", "📖"),
    ("bookmark yer imi", "🔖"),
    ("pencil kurşun kalem", "✏️"),
    ("pen tükenmez kalem", "🖊️"),
    ("notebook defter", "📓"),
    ("memo not kağıt", "📝"),
    ("paperclip ataş raptiye", "📎"),
    ("folder klasör", "📁"),
    ("file dosya sayfa", "📄"),
    ("calendar takvim ajanda", "📅"),
    ("clipboard pano", "📋"),
    ("chart grafik pasta", "📊"),
    ("graph grafik yükseliş", "📈"),
    ("money para dolar torba", "💰"),
    ("coin madeni para", "🪙"),
    ("credit card kredi kartı", "💳"),
    ("gift hediye", "🎁"),
    ("package paket koli", "📦"),
    ("key anahtar", "🔑"),
    ("lock kilit", "🔒"),
    ("link bağlantı link", "🔗"),
    ("magnifier büyüteç arama", "🔍"),
    ("light bulb ampul fikir", "💡"),
    ("tools araçlar tamir çekiç", "🛠️"),
    ("gear dişli ayarlar", "⚙️"),
    ("computer bilgisayar laptop", "💻"),
    ("desktop masaüstü bilgisayar ekran", "🖥️"),
    ("phone telefon cep telefonu", "📱"),
    ("robot robot", "🤖"),
    ("rocket roket uzay", "🚀"),
    ("car araba otomobil", "🚗"),
    ("bicycle bisiklet", "🚲"),
    ("airplane uçak seyahat", "✈️"),
    ("train tren metro", "🚆"),
    ("ship gemi vapur", "🚢"),
    ("house ev yuva", "🏠"),
    ("office ofis bina plaza", "🏢"),
    ("school okul eğitim", "🏫"),
    ("hospital hastane sağlık", "🏥"),
    ("globe dünya küre internet", "🌐"),
    ("pin raptiye konum pin", "📍"),
    ("map harita", "🗺️"),
    ("clock saat zaman", "🕒"),
    ("hourglass kum saati", "⌛"),
    ("check mark onay tik doğru", "✅"),
    ("cross mark çarpı yanlış iptal", "❌"),
    ("warning uyarı dikkat", "⚠️"),
    ("question soru soru işareti", "❓"),
    ("info bilgi", "ℹ️"),
    ("exclamation ünlem", "❗"),
    ("plus artı ekle", "➕"),
    ("minus eksi çıkar", "➖"),
    ("arrow ok yön sağ", "➡️"),
    ("refresh yenile döngü", "🔄"),
    ("search ara büyüteç", "🔎"),
    ("crown taç kral kraliçe", "👑"),
    ("glasses gözlük", "👓"),
    ("clothing tişört giysi kıyafet", "👕"),
    ("shoe ayakkabı spor", "👟"),
    ("gift box kurdele fiyonk", "🎀"),
    ("party kutlama parti şapka", "🎉"),
    ("confetti konfeti kutlama", "🎊"),
    ("balloon balon", "🎈"),
    ("flag bayrak kırmızı bayrak", "🚩"),
    ("pinwheel dönme dolap rüzgar gülü", "🎡"),
    ("travel seyahat bavul valiz", "🧳"),
    ("camping kamp çadır", "⛺"),
    ("mountain dağ zirve", "⛰️"),
    ("beach plaj kumsal sahil", "🏖️"),
    ("wave dalga deniz sörf", "🌊"),
    ("umbrella şemsiye yağmur", "☂️"),
    ("rain yağmur bulut", "🌧️"),
];

pub const MONOCHROME_ICONS: &[(&str, &str)] = &[
    ("sparkles", "✦"),
    ("four star", "✧"),
    ("page", "▤"),
    ("check", "☑"),
    ("diamond", "◇"),
    ("menu", "☰"),
    ("circle", "○"),
    ("star", "★"),
    ("star hollow", "☆"),
    ("star circled", "✪"),
    ("burst", "✶"),
    ("sunburst", "✹"),
    ("grid", "▦"),
    ("square striped", "▨"),
    ("black square", "■"),
    ("white square", "□"),
    ("ballot x", "☒"),
    ("empty box", "☐"),
    ("black diamond", "◆"),
    ("white diamond", "◇"),
    ("lozenge", "◈"),
    ("hexagon", "⬡"),
    ("hexagon filled", "⬢"),
    ("circle black", "●"),
    ("fisheye", "◉"),
    ("bullseye", "⦿"),
    ("dotted circle", "◌"),
    ("play", "▶"),
    ("triangle", "▲"),
    ("down triangle", "▼"),
    ("pointer right", "▷"),
    ("pointer left", "◁"),
    ("cross", "❖"),
    ("flower", "✿"),
    ("floret", "❀"),
    ("gear", "⚙"),
    ("flag black", "⚑"),
    ("flag white", "⚐"),
    ("anchor", "⚓"),
    ("swords", "⚔"),
    ("scales", "⚖"),
    ("hourglass", "⌛"),
    ("scissors", "✂"),
    ("pencil", "✎"),
    ("envelope", "✉"),
    ("airplane", "✈"),
    ("search", "⌕"),
    ("command", "⌘"),
    ("option", "⌥"),
    ("shift", "⇧"),
    ("sun", "☼"),
    ("crescent", "☽"),
    ("lightning", "⚡"),
    ("heart", "♥"),
    ("music", "♫"),
    ("paragraph", "¶"),
    ("section", "§"),
    ("asterisk", "※"),
];

pub const PAGE_ICONS: &[(&str, &str)] = COLOR_ICONS;

pub fn is_color_icon(icon: &str) -> bool {
    COLOR_ICONS.iter().any(|(_, ic)| *ic == icon)
}

#[derive(Clone, Copy)]
pub enum SettingKey {
    LastPage,
    Dark,
    LightBackground,
    DarkBackground,
    AccentColor,
    FontScale,
    DockLayout,
    SidebarPinned,
    CustomTheme,
}

impl SettingKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LastPage => "last_page",
            Self::Dark => "dark",
            Self::LightBackground => "light_background",
            Self::DarkBackground => "dark_background",
            Self::AccentColor => "accent_color",
            Self::FontScale => "font_scale",
            Self::DockLayout => "dock_layout",
            Self::SidebarPinned => "sidebar_pinned",
            Self::CustomTheme => "custom_theme",
        }
    }
}

pub fn data_directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("NOTEON_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or(Error::DataDirectory)?
        .join("data");
    if let Some(legacy) = directories::ProjectDirs::from("", "Kargalar", APP_NAME) {
        migrate_legacy_data(legacy.data_local_dir(), &directory)?;
    }
    Ok(directory)
}

fn migrate_legacy_data(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() || !source.join(DATABASE_FILE).is_file() {
        return Ok(());
    }
    match std::fs::rename(source, destination) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {}
        Err(error) => return Err(error.into()),
    }
    let staging = destination.with_extension(format!("migration-{}", std::process::id()));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        copy_legacy_files(source, &staging)?;
        let database = rusqlite::Connection::open(source.join(DATABASE_FILE))?;
        database.backup(rusqlite::MAIN_DB, staging.join(DATABASE_FILE), None)?;
        std::fs::rename(&staging, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn copy_legacy_files(source: &Path, destination: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir(&target)?;
            copy_legacy_files(&path, &target)?;
        } else if entry.file_type()?.is_file()
            && !(source.join(DATABASE_FILE).is_file()
                && matches!(
                    entry.file_name().to_str(),
                    Some("noteon.sqlite" | "noteon.sqlite-wal" | "noteon.sqlite-shm")
                ))
        {
            std::fs::copy(path, target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_workspace_moves_without_copying_or_overwriting() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("legacy");
        let destination = root.path().join("data");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join(DATABASE_FILE), b"workspace").unwrap();
        std::fs::write(source.join("attachment"), b"media").unwrap();
        migrate_legacy_data(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_eq!(
            std::fs::read(destination.join("attachment")).unwrap(),
            b"media"
        );
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join(DATABASE_FILE), b"other").unwrap();
        migrate_legacy_data(&source, &destination).unwrap();
        assert_eq!(
            std::fs::read(destination.join(DATABASE_FILE)).unwrap(),
            b"workspace"
        );
    }

    #[test]
    fn test_icon_separation() {
        assert!(!COLOR_ICONS.is_empty());
        assert!(!MONOCHROME_ICONS.is_empty());
        assert!(is_color_icon("😀"));
        assert!(is_color_icon("🚀"));
        assert!(!is_color_icon("✦"));
        assert!(!is_color_icon("▤"));
    }

    #[test]
    fn test_all_color_icons_have_twemoji() {
        for (name, icon) in COLOR_ICONS {
            assert!(
                crate::ui::emoji_image_source(icon).is_some(),
                "Emoji for '{}' ({}) should resolve to a valid Twemoji SVG",
                name,
                icon
            );
        }
    }
}
