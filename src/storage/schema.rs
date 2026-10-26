use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::model::{BlockKind, PROPERTY_TYPES};
use crate::error::{Error, Result};

const SCHEMA_VERSION: i32 = 15;

pub fn migrate(connection: &Connection) -> Result<()> {
    let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(Error::Invalid(
            "This database was created with a newer version of Noteon.".into(),
        ));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version == 0 {
        let schema = include_str!("schema.sql")
            .replace("{{property_types}}", &property_type_values())
            .replace("{{schema_version}}", &SCHEMA_VERSION.to_string());
        connection.execute_batch(&schema)?;
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
        let has_blocks: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='blocks')",
            [],
            |row| row.get(0),
        )?;
        if has_blocks && !has_column(connection, "blocks", "text_styles")? {
            connection.execute_batch(
                "ALTER TABLE blocks ADD COLUMN text_styles TEXT NOT NULL DEFAULT '[]';",
            )?;
        }
        connection.pragma_update(None, "user_version", 11)?;
    }
    if version <= 11 {
        let has_pages: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pages')",
            [],
            |row| row.get(0),
        )?;
        if has_pages && !has_column(connection, "pages", "full_width")? {
            connection.execute_batch(
                "ALTER TABLE pages ADD COLUMN full_width INTEGER NOT NULL DEFAULT 0 CHECK(full_width IN (0,1));",
            )?;
        }
        connection.pragma_update(None, "user_version", 12)?;
    }
    if version <= 12 {
        let has_pages: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pages')",
            [],
            |row| row.get(0),
        )?;
        if has_pages && has_column(connection, "pages", "created_at")? {
            if has_column(connection, "pages", "updated_at")? {
                connection.execute_batch(
                    "UPDATE pages SET created_at=CASE WHEN updated_at > 0 THEN updated_at ELSE unixepoch() END WHERE created_at <= 0;",
                )?;
            } else {
                connection.execute_batch(
                    "UPDATE pages SET created_at=unixepoch() WHERE created_at <= 0;",
                )?;
            }
        }
        connection.pragma_update(None, "user_version", 13)?;
    }
    if version <= 13 {
        connection.execute_batch(
            "BEGIN IMMEDIATE;
             WITH ranked_blocks AS (
                 SELECT id, ROW_NUMBER() OVER (PARTITION BY page_id ORDER BY position, id) - 1 AS position
                 FROM blocks
             )
             UPDATE blocks
             SET position=(SELECT position FROM ranked_blocks WHERE ranked_blocks.id=blocks.id);
             UPDATE pages
             SET block_count=(SELECT COUNT(*) FROM blocks WHERE blocks.page_id=pages.id);
             PRAGMA user_version=14;
             COMMIT;",
        )?;
    }
    if version <= 14 {
        migrate_property_types(connection)?;
    }
    Ok(())
}

fn property_type_values() -> String {
    PROPERTY_TYPES
        .iter()
        .map(|(kind, _)| format!("'{}'", kind.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",")
}

fn migrate_property_types(connection: &Connection) -> Result<()> {
    // Rebuild the parent tables with foreign keys disabled so dropping the old
    // tables cannot cascade-delete their existing property values.
    let foreign_keys: bool =
        connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    connection.pragma_update(None, "foreign_keys", false)?;
    let result = (|| -> Result<()> {
        let transaction = connection.unchecked_transaction()?;
        for (table, owner_column, owner_table, index) in [
            (
                "database_properties",
                "database_page_id",
                "databases",
                "database_properties_order",
            ),
            (
                "custom_page_properties",
                "page_id",
                "pages",
                "custom_page_properties_order",
            ),
        ] {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )?;
            if !exists {
                continue;
            }
            let sequence: i64 = transaction
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name=?1",
                    [table],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0);
            let owner_key = if owner_table == "databases" {
                "page_id"
            } else {
                "id"
            };
            transaction.execute_batch(&format!(
                "CREATE TABLE {table}_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    {owner_column} INTEGER NOT NULL REFERENCES {owner_table}({owner_key}) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ({})),
                    options TEXT NOT NULL DEFAULT '',
                    position INTEGER NOT NULL,
                    UNIQUE({owner_column},name)
                 );
                 INSERT INTO {table}_new(id,{owner_column},name,kind,options,position)
                 SELECT id,{owner_column},name,kind,options,position FROM {table};
                 DROP TABLE {table};
                 ALTER TABLE {table}_new RENAME TO {table};
                 CREATE INDEX {index} ON {table}({owner_column},position);",
                property_type_values(),
            ))?;
            transaction.execute(
                "UPDATE sqlite_sequence SET seq=MAX(seq,?1) WHERE name=?2",
                params![sequence, table],
            )?;
        }
        if transaction
            .prepare("PRAGMA foreign_key_check")?
            .exists([])?
        {
            return Err(Error::Invalid(
                "Property migration found invalid references.".into(),
            ));
        }
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
        Ok(())
    })();
    let restore = connection.pragma_update(None, "foreign_keys", foreign_keys);
    result?;
    restore?;
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

    fn legacy_property_schema() -> String {
        include_str!("schema.sql")
            .replace(
                "{{property_types}}",
                "'text','number','date','status','select','multi-select','checkbox'",
            )
            .replace("{{schema_version}}", "14")
    }

    #[test]
    fn failed_property_migration_rolls_back_and_restores_foreign_keys() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(&legacy_property_schema()).unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO database_properties(database_page_id,name,kind,position)
             VALUES(999,'Orphan','text',0);
             PRAGMA foreign_keys=ON;",
            )
            .unwrap();

        assert!(migrate(&connection).is_err());
        let version: i32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 14);
        let foreign_keys: bool = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert!(foreign_keys);
        let (schema, count): (String, i64) = connection.query_row(
            "SELECT sql,(SELECT COUNT(*) FROM database_properties) FROM sqlite_master WHERE name='database_properties'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert!(!schema.contains("'email'"));
        assert_eq!(count, 1);
        let temporary_tables: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('database_properties_new','custom_page_properties_new')",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(temporary_tables, 0);
    }

    #[test]
    fn all_property_types_work_in_new_and_migrated_workspaces_without_losing_values() {
        use crate::storage::Repository;

        for legacy in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("properties.sqlite");
            let connection = Connection::open(&path).unwrap();
            if legacy {
                connection.execute_batch(&legacy_property_schema()).unwrap();
            } else {
                migrate(&connection).unwrap();
            }
            connection.execute_batch(
                "INSERT INTO pages(id,title) VALUES(1,'Database'),(2,'Record'),(3,'Page');
                 UPDATE pages SET parent_id=1 WHERE id=2;
                 INSERT INTO databases(page_id) VALUES(1);
                 INSERT INTO database_properties(id,database_page_id,name,kind,options,position)
                 VALUES(7,1,'Existing select','select','Keep',0),(1000,1,'Deleted','text','',1);
                 INSERT INTO page_property_values(page_id,property_id,value) VALUES(2,7,'Keep');
                 DELETE FROM database_properties WHERE id=1000;
                 INSERT INTO custom_page_properties(id,page_id,name,kind,position)
                 VALUES(9,3,'Existing text','text',0),(2000,3,'Deleted','text',1);
                 INSERT INTO custom_page_property_values(page_id,property_id,value) VALUES(3,9,'Saved text');
                 DELETE FROM custom_page_properties WHERE id=2000;",
            ).unwrap();
            if legacy {
                assert!(connection.execute(
                    "INSERT INTO database_properties(database_page_id,name,kind,position) VALUES(1,'Email','email',1)", [],
                ).is_err());
            }
            drop(connection);
            let repository = Repository::open(&path).unwrap();
            let database = repository.database(1).unwrap();
            assert_eq!(database.properties[0].id, 7);
            assert_eq!(database.properties[0].options[0].name, "Keep");
            assert_eq!(database.rows[0].values[&7], "Keep");
            let page_properties = repository.page_properties(3).unwrap();
            assert_eq!(page_properties[0].id, 9);
            assert_eq!(page_properties[0].value, "Saved text");

            for (kind, label) in PROPERTY_TYPES {
                repository
                    .add_database_property(1, label, kind, &[])
                    .unwrap();
                repository.add_page_property(3, label, kind, &[]).unwrap();
                repository.set_database_property_type(7, kind).unwrap();
                repository.set_page_property_type(3, 9, kind).unwrap();
            }
            let database = repository.database(1).unwrap();
            assert!(
                database.properties[1..]
                    .iter()
                    .all(|property| property.id > 1000)
            );
            assert!(
                repository.page_properties(3).unwrap()[1..]
                    .iter()
                    .all(|property| property.id > 2000)
            );
            assert!(
                repository
                    .add_database_property(1, "Invalid", "unknown", &[])
                    .is_err()
            );
            assert!(
                repository
                    .connection
                    .execute(
                        "UPDATE database_properties SET kind='unknown' WHERE id=7",
                        [],
                    )
                    .is_err()
            );
            assert!(
                repository
                    .connection
                    .execute(
                        "UPDATE custom_page_properties SET kind='unknown' WHERE id=9",
                        [],
                    )
                    .is_err()
            );
            let foreign_keys: bool = repository
                .connection
                .pragma_query_value(None, "foreign_keys", |row| row.get(0))
                .unwrap();
            assert!(foreign_keys);
            assert!(
                !repository
                    .connection
                    .prepare("PRAGMA foreign_key_check")
                    .unwrap()
                    .exists([])
                    .unwrap()
            );
            repository.delete_database_property(7).unwrap();
            repository.delete_page_property(3, 9).unwrap();
            for table in ["page_property_values", "custom_page_property_values"] {
                let count: i64 = repository
                    .connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(count, 0);
            }
            drop(repository);
            let reopened = Repository::open(&path).unwrap();
            assert_eq!(
                reopened.database(1).unwrap().properties.len(),
                PROPERTY_TYPES.len()
            );
            assert_eq!(
                reopened.page_properties(3).unwrap().len(),
                PROPERTY_TYPES.len()
            );
        }
    }

    #[test]
    fn migration_turns_existing_image_attachment_references_into_media_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let image_path = directory.path().join("photo.png");
        std::fs::write(&image_path, b"image fixture").unwrap();
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE pages(id INTEGER PRIMARY KEY,block_count INTEGER NOT NULL DEFAULT 0);\
                 INSERT INTO pages(id) VALUES(1);\
                 CREATE TABLE blocks(id INTEGER PRIMARY KEY,page_id INTEGER NOT NULL DEFAULT 1,position INTEGER NOT NULL DEFAULT 0,kind TEXT NOT NULL,content TEXT NOT NULL);\
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
