use rusqlite::Connection;

use crate::domain::model::BlockKind;
use crate::error::{Error, Result};

pub fn migrate(connection: &Connection) -> Result<()> {
    let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > 11 {
        return Err(Error::Invalid(
            "This database was created with a newer version of Noteon.".into(),
        ));
    }
    if version == 11 {
        return Ok(());
    }
    if version == 0 {
        connection.execute_batch(include_str!("schema.sql"))?;
        return Ok(());
    }
    if version == 1 {
        connection.execute_batch(
            "ALTER TABLE blocks ADD COLUMN column_group INTEGER;\
             ALTER TABLE blocks ADD COLUMN column_index INTEGER NOT NULL DEFAULT 0 CHECK(column_index IN (0,1));\
             PRAGMA user_version=2;",
        )?;
    }
    if version <= 2 {
        connection.execute_batch(
            "ALTER TABLE pages ADD COLUMN priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN 0 AND 3);\
             ALTER TABLE pages ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;\
             UPDATE pages SET created_at=updated_at;\
             CREATE TABLE databases (page_id INTEGER PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE, view_mode TEXT NOT NULL DEFAULT 'table' CHECK(view_mode IN ('table','gallery','board')), sort_key TEXT NOT NULL DEFAULT 'created_at', sort_descending INTEGER NOT NULL DEFAULT 1 CHECK(sort_descending IN (0,1)));\
             CREATE TABLE database_properties (id INTEGER PRIMARY KEY AUTOINCREMENT, database_page_id INTEGER NOT NULL REFERENCES databases(page_id) ON DELETE CASCADE, name TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('text','number','date','status','select','multi-select','checkbox')), options TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL, UNIQUE(database_page_id, name));\
             CREATE INDEX database_properties_order ON database_properties(database_page_id, position);\
             CREATE TABLE page_property_values (page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE, property_id INTEGER NOT NULL REFERENCES database_properties(id) ON DELETE CASCADE, value TEXT NOT NULL DEFAULT '', PRIMARY KEY(page_id, property_id));\
             CREATE INDEX page_property_values_property ON page_property_values(property_id, value);\
             PRAGMA user_version=3;",
        )?;
    }
    if version <= 3 {
        let has_blocks: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='blocks')",
            [],
            |row| row.get(0),
        )?;
        if has_blocks {
            connection.execute_batch(
                "ALTER TABLE blocks ADD COLUMN layout_column INTEGER NOT NULL DEFAULT 0;\
                 ALTER TABLE blocks ADD COLUMN column_width REAL NOT NULL DEFAULT 1.0 CHECK(column_width > 0 AND column_width <= 1.0);\
                 UPDATE blocks SET layout_column=column_index;\
                 PRAGMA user_version=4;",
            )?;
        } else {
            connection.pragma_update(None, "user_version", 4)?;
        }
    }
    if version <= 4 {
        connection.execute_batch(
            "CREATE TABLE custom_page_properties (id INTEGER PRIMARY KEY AUTOINCREMENT, page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE, name TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('text','number','date','status','select','multi-select','checkbox')), options TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL, UNIQUE(page_id,name));\
             CREATE INDEX custom_page_properties_order ON custom_page_properties(page_id,position);\
             CREATE TABLE custom_page_property_values (page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE, property_id INTEGER NOT NULL REFERENCES custom_page_properties(id) ON DELETE CASCADE, value TEXT NOT NULL DEFAULT '', PRIMARY KEY(page_id,property_id));\
             PRAGMA user_version=5;",
        )?;
    }
    if version <= 5 {
        connection.execute_batch(
            "ALTER TABLE databases ADD COLUMN gallery_columns INTEGER NOT NULL DEFAULT 3 CHECK(gallery_columns BETWEEN 1 AND 6);\
             UPDATE databases SET view_mode='table' WHERE view_mode='blog';\
             PRAGMA user_version=6;",
        )?;
    }
    if version <= 6 {
        let has_blocks: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='blocks')",
            [],
            |row| row.get(0),
        )?;
        if has_blocks {
            let has_media_width = connection
                .prepare("PRAGMA table_info(blocks)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|column| column == "media_width");
            if !has_media_width {
                connection.execute_batch(
                    "ALTER TABLE blocks ADD COLUMN media_width REAL NOT NULL DEFAULT 1.0 CHECK(media_width >= 0.25 AND media_width <= 1.0);",
                )?;
            }
            migrate_imported_media(connection)?;
        }
        connection.pragma_update(None, "user_version", 7)?;
    }
    if version <= 7 {
        let columns = connection
            .prepare("PRAGMA table_info(pages)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if !columns.iter().any(|column| column == "cover_image") {
            connection.execute_batch("ALTER TABLE pages ADD COLUMN cover_image TEXT;")?;
        }
        if !columns.iter().any(|column| column == "cover_x") {
            connection.execute_batch("ALTER TABLE pages ADD COLUMN cover_x REAL NOT NULL DEFAULT 0.5 CHECK(cover_x BETWEEN 0.0 AND 1.0);")?;
        }
        if !columns.iter().any(|column| column == "cover_y") {
            connection.execute_batch("ALTER TABLE pages ADD COLUMN cover_y REAL NOT NULL DEFAULT 0.5 CHECK(cover_y BETWEEN 0.0 AND 1.0);")?;
        }
        connection.pragma_update(None, "user_version", 8)?;
    }
    if version <= 8 {
        let has_sort_order = connection
            .prepare("PRAGMA table_info(pages)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|column| column == "sort_order");
        if !has_sort_order {
            connection.execute_batch(
                "ALTER TABLE pages ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;\
                 UPDATE pages SET sort_order=id;",
            )?;
        }
        connection.pragma_update(None, "user_version", 9)?;
    }
    if version <= 9 {
        let has_databases: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='databases')",
            [],
            |row| row.get(0),
        )?;
        if has_databases && !has_column(connection, "databases", "view_settings")? {
            connection.execute_batch(
                "ALTER TABLE databases ADD COLUMN view_settings TEXT NOT NULL DEFAULT '{}';",
            )?;
        }
        connection.pragma_update(None, "user_version", 10)?;
    }
    if version <= 10 {
        if !has_column(connection, "blocks", "text_styles")? {
            connection.execute_batch(
                "ALTER TABLE blocks ADD COLUMN text_styles TEXT NOT NULL DEFAULT '[]';",
            )?;
        }
        connection.pragma_update(None, "user_version", 11)?;
    }
    Ok(())
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns.iter().any(|existing| existing == column))
}

fn migrate_imported_media(connection: &Connection) -> Result<()> {
    let mut statement = connection
        .prepare("SELECT id,content FROM blocks WHERE kind='text' AND content LIKE '[Ek:%'")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    for (id, content) in rows {
        let Some(path) = imported_attachment_path(&content) else {
            continue;
        };
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        let Some(kind) = BlockKind::from_media_extension(extension) else {
            continue;
        };
        if path.is_file() {
            connection.execute(
                "UPDATE blocks SET kind=?1,content=?2 WHERE id=?3",
                rusqlite::params![kind.as_str(), path.to_string_lossy(), id],
            )?;
        }
    }
    Ok(())
}

fn imported_attachment_path(content: &str) -> Option<std::path::PathBuf> {
    let path_start = content.find("C:\\\\").or_else(|| content.find("C:\\"))?;
    let path = content[path_start..].trim_end_matches(']').trim();
    Some(std::path::PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_turns_existing_image_attachment_references_into_media_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let image_path = directory.path().join("photo.png");
        std::fs::write(&image_path, b"image fixture").unwrap();
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE pages(id INTEGER PRIMARY KEY);\
                 CREATE TABLE blocks(id INTEGER PRIMARY KEY,kind TEXT NOT NULL,content TEXT NOT NULL);\
                 PRAGMA user_version=6;",
            )
            .unwrap();
        let attachment = format!("[Ek: photo.png — {}]", image_path.display());
        connection
            .execute(
                "INSERT INTO blocks(id,kind,content) VALUES(1,'text',?1)",
                [&attachment],
            )
            .unwrap();

        migrate(&connection).unwrap();

        let (kind, content, width): (String, String, f32) = connection
            .query_row(
                "SELECT kind,content,media_width FROM blocks WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(kind, "image");
        assert_eq!(content, image_path.to_string_lossy());
        assert_eq!(width, 1.0);
    }
}
