mod blocks;
mod databases;
mod files;
mod page_properties;
mod pages;
mod schema;
mod search;
mod transfers;

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs::File, io::Read};

use rusqlite::Connection;

use crate::config::{SettingKey, UNTITLED};
use crate::domain::model::{PageId, SaveBatch};
use crate::error::{Error, Result};

pub(crate) fn sql_index(value: usize) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| Error::Invalid("The row index exceeds the supported limit.".into()))
}

pub(crate) fn read_usize(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<usize> {
    let value: i64 = row.get(column)?;
    usize::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn attachment_component(title: &str, id: PageId) -> String {
    let title = title.trim();
    let title = if title.is_empty() { UNTITLED } else { title };
    let mut safe = String::new();
    for character in title.chars().take(48) {
        if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
            safe.push(character);
        } else {
            safe.push('_');
        }
    }
    let safe = safe.trim_matches([' ', '.', '_']);
    let safe = if safe.is_empty() { UNTITLED } else { safe };
    format!("{safe}-{id}")
}

fn remove_empty_attachment_directories(directory: &Path, attachments: &Path) {
    let mut current = directory;
    while current != attachments && current.starts_with(attachments) {
        let is_empty = std::fs::read_dir(current)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_none();
        if !is_empty || std::fs::remove_dir(current).is_err() {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
}

fn attachment_files_match(first: &Path, second: &Path) -> Result<bool> {
    if std::fs::metadata(first)?.len() != std::fs::metadata(second)?.len() {
        return Ok(false);
    }
    let mut first = File::open(first)?;
    let mut second = File::open(second)?;
    let mut first_buffer = [0; 8192];
    let mut second_buffer = [0; 8192];
    loop {
        let first_read = first.read(&mut first_buffer)?;
        let second_read = second.read(&mut second_buffer)?;
        if first_read != second_read || first_buffer[..first_read] != second_buffer[..second_read] {
            return Ok(false);
        }
        if first_read == 0 {
            return Ok(true);
        }
    }
}

fn attachment_relative_path(path: &Path) -> Option<PathBuf> {
    path.ancestors().find_map(|ancestor| {
        (ancestor.file_name()?.to_str()? == "attachments")
            .then(|| path.strip_prefix(ancestor).ok().map(Path::to_path_buf))
            .flatten()
    })
}

fn linked_attachment(content: &str) -> Option<(&str, &str, PathBuf)> {
    let (prefix, path) = content.rsplit_once(" — ")?;
    let path = path.trim();
    let relative = attachment_relative_path(Path::new(path))?;
    Some((prefix, path, relative))
}

fn referenced_attachment_files(
    connection: &Connection,
) -> Result<std::collections::HashSet<PathBuf>> {
    let mut referenced = std::collections::HashSet::new();
    for query in [
        "SELECT cover_image FROM pages WHERE cover_image IS NOT NULL",
        "SELECT content FROM blocks WHERE kind IN ('image','video')",
    ] {
        let mut statement = connection.prepare(query)?;
        for row in statement.query_map([], |row| row.get::<_, String>(0))? {
            if let Ok(path) = std::fs::canonicalize(row?) {
                referenced.insert(path);
            }
        }
    }
    let mut statement = connection.prepare("SELECT content FROM blocks WHERE kind='text'")?;
    for row in statement.query_map([], |row| row.get::<_, String>(0))? {
        if let Some((_, path, _)) = linked_attachment(&row?)
            && let Ok(path) = std::fs::canonicalize(path)
        {
            referenced.insert(path);
        }
    }
    Ok(referenced)
}

// Attachment paths in older workspaces are absolute. Refresh them whenever the
// workspace moves, but leave unrelated external paths untouched.
fn rebase_attachments(connection: &Connection, database: &Path) -> Result<()> {
    let Some(directory) = database.parent() else {
        return Ok(());
    };
    let attachments = directory.join("attachments");
    if !attachments.is_dir() {
        return Ok(());
    }
    for (query, update) in [
        (
            "SELECT id, cover_image FROM pages WHERE cover_image IS NOT NULL",
            "UPDATE pages SET cover_image=?1 WHERE id=?2",
        ),
        (
            "SELECT id, content FROM blocks WHERE kind IN ('image', 'video')",
            "UPDATE blocks SET content=?1 WHERE id=?2",
        ),
    ] {
        let mut statement = connection.prepare(query)?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, value) = row?;
            let old = Path::new(&value);
            let relative = attachment_relative_path(old);
            if let Some(relative) = relative {
                let new = attachments.join(relative);
                if new.is_file() && new != old {
                    connection.execute(update, rusqlite::params![new.to_string_lossy(), id])?;
                }
            }
        }
    }
    let mut statement = connection.prepare("SELECT id,content FROM blocks WHERE kind='text'")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, content) = row?;
        let Some((prefix, old_value, relative)) = linked_attachment(&content) else {
            continue;
        };
        let new = attachments.join(relative);
        if new.is_file() && new.to_string_lossy() != old_value {
            connection.execute(
                "UPDATE blocks SET content=?1 WHERE id=?2",
                rusqlite::params![format!("{prefix} — {}", new.to_string_lossy()), id],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod attachment_relocation_tests {
    use super::*;

    #[test]
    fn rebases_nested_attachments_and_preserves_external_paths() {
        let root = tempfile::tempdir().unwrap();
        let attachments = root.path().join("attachments").join("nested");
        std::fs::create_dir_all(&attachments).unwrap();
        let image = attachments.join("image.png");
        std::fs::write(&image, b"image").unwrap();
        let old = root
            .path()
            .join("old")
            .join("attachments")
            .join("nested")
            .join("image.png");
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE pages(id INTEGER PRIMARY KEY, cover_image TEXT); CREATE TABLE blocks(id INTEGER PRIMARY KEY, kind TEXT, content TEXT);").unwrap();
        connection
            .execute(
                "INSERT INTO pages VALUES (1, ?1)",
                [old.to_string_lossy().as_ref()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO blocks VALUES (1, 'image', ?1)",
                [old.to_string_lossy().as_ref()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO blocks VALUES (2, 'image', 'https://example.com/image.png')",
                [],
            )
            .unwrap();
        rebase_attachments(&connection, &root.path().join("noteon.sqlite")).unwrap();
        let cover: String = connection
            .query_row("SELECT cover_image FROM pages", [], |row| row.get(0))
            .unwrap();
        let media: String = connection
            .query_row("SELECT content FROM blocks WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let external: String = connection
            .query_row("SELECT content FROM blocks WHERE id=2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(cover, image.to_string_lossy());
        assert_eq!(media, cover);
        assert_eq!(external, "https://example.com/image.png");
    }
}

pub struct Repository {
    pub(crate) connection: Connection,
}

impl Repository {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA cache_size=-8192;")?;
        schema::migrate(&connection)?;
        rebase_attachments(&connection, path)?;
        let repository = Self { connection };
        repository
            .organize_database_attachments(path.parent().unwrap_or_else(|| Path::new(".")))?;
        Ok(repository)
    }

    pub(crate) fn attachment_directory(&self, page: i64, data_directory: &Path) -> Result<PathBuf> {
        let ancestors = {
            let mut statement = self.connection.prepare(
                "WITH RECURSIVE ancestors(id,parent_id,title,depth) AS (
                    SELECT id,parent_id,title,0 FROM pages WHERE id=?1
                    UNION ALL
                    SELECT page.id,page.parent_id,page.title,ancestors.depth+1
                    FROM pages page JOIN ancestors ON page.id=ancestors.parent_id
                 )
                 SELECT id,title,depth,EXISTS(SELECT 1 FROM databases WHERE page_id=ancestors.id)
                 FROM ancestors ORDER BY depth",
            )?;
            statement
                .query_map([page], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let Some(database_index) = ancestors.iter().position(|ancestor| ancestor.3) else {
            return Ok(data_directory.join("attachments"));
        };
        let (database_id, database_title, _, _) = &ancestors[database_index];
        let mut directory = data_directory
            .join("attachments")
            .join(attachment_component(database_title, *database_id));
        for (id, title, _, _) in ancestors[..database_index].iter().rev() {
            directory.push(attachment_component(title, *id));
        }
        Ok(directory)
    }

    fn organize_database_attachments(&self, data_directory: &Path) -> Result<()> {
        let attachments = data_directory.join("attachments");
        if !attachments.is_dir() {
            return Ok(());
        }
        let mut moves = Vec::new();
        {
            let mut statement = self
                .connection
                .prepare("SELECT id,cover_image FROM pages WHERE cover_image IS NOT NULL")?;
            for row in statement.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })? {
                let (page, path) = row?;
                self.plan_attachment_move(&attachments, page, &path, |new_path| {
                    moves.push(("pages", page, page, path.clone(), new_path, None));
                })?;
            }
        }
        {
            let mut statement = self
                .connection
                .prepare("SELECT id,page_id,content FROM blocks WHERE kind IN ('image','video')")?;
            for row in statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })? {
                let (block, page, path) = row?;
                self.plan_attachment_move(&attachments, page, &path, |new_path| {
                    moves.push(("media", block, page, path.clone(), new_path, None));
                })?;
            }
        }
        {
            let mut statement = self
                .connection
                .prepare("SELECT id,page_id,content FROM blocks WHERE kind='text'")?;
            for row in statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })? {
                let (block, page, content) = row?;
                let Some((prefix, path, _)) = linked_attachment(&content) else {
                    continue;
                };
                self.plan_attachment_move(&attachments, page, path, |new_path| {
                    moves.push((
                        "text",
                        block,
                        page,
                        path.to_owned(),
                        new_path,
                        Some(prefix.to_owned()),
                    ));
                })?;
            }
        }
        if moves.is_empty() {
            return Ok(());
        }

        let mut destinations = std::collections::HashMap::new();
        for (table, id, page, old_path, requested_path, _) in &moves {
            let source = Path::new(old_path);
            let mut destination = PathBuf::from(requested_path);
            if source != destination
                && destination.exists()
                && !attachment_files_match(&destination, source)?
            {
                let requested_name = destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("attachment");
                let requested_name = Path::new(requested_name);
                let stem = requested_name
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("attachment");
                let extension = requested_name
                    .extension()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                let mut suffix = 1;
                loop {
                    let name = if extension.is_empty() {
                        format!("{stem}-{page}-{suffix}")
                    } else {
                        format!("{stem}-{page}-{suffix}.{extension}")
                    };
                    let candidate = destination.with_file_name(name);
                    if !candidate.exists() || attachment_files_match(&candidate, source)? {
                        destination = candidate;
                        break;
                    }
                    suffix += 1;
                }
            }
            if source != destination {
                std::fs::create_dir_all(destination.parent().ok_or(Error::DataDirectory)?)?;
                if !destination.exists() {
                    std::fs::copy(source, &destination)?;
                }
            }
            destinations.insert(
                (table.to_string(), *id),
                destination.to_string_lossy().into_owned(),
            );
        }

        let transaction = self.connection.unchecked_transaction()?;
        for (table, id, _, _, _, prefix) in &moves {
            let new_path = &destinations[&(table.to_string(), *id)];
            match *table {
                "pages" => {
                    transaction.execute(
                        "UPDATE pages SET cover_image=?1 WHERE id=?2",
                        rusqlite::params![new_path, id],
                    )?;
                }
                "media" => {
                    transaction.execute(
                        "UPDATE blocks SET content=?1 WHERE id=?2",
                        rusqlite::params![new_path, id],
                    )?;
                }
                "text" => {
                    let content = format!("{} — {new_path}", prefix.as_deref().unwrap_or("📎 Ek"));
                    transaction.execute(
                        "UPDATE blocks SET content=?1 WHERE id=?2",
                        rusqlite::params![content, id],
                    )?;
                }
                _ => unreachable!(),
            }
        }
        transaction.commit()?;

        let referenced_files = referenced_attachment_files(&self.connection)?;
        for old_path in moves
            .iter()
            .map(|(_, _, _, old_path, _, _)| old_path)
            .collect::<std::collections::HashSet<_>>()
        {
            if !destinations.values().any(|new_path| new_path == old_path) {
                let still_referenced = std::fs::canonicalize(old_path)
                    .is_ok_and(|canonical| referenced_files.contains(&canonical));
                if !still_referenced {
                    if std::fs::remove_file(old_path).is_ok()
                        && let Some(parent) = Path::new(old_path).parent()
                    {
                        remove_empty_attachment_directories(parent, &attachments);
                    }
                }
            }
        }
        Ok(())
    }

    fn plan_attachment_move(
        &self,
        attachments: &Path,
        page: i64,
        path: &str,
        mut plan: impl FnMut(String),
    ) -> Result<()> {
        let source = Path::new(path);
        if !source.is_file() || !source.starts_with(attachments) {
            return Ok(());
        }
        let destination_directory =
            self.attachment_directory(page, attachments.parent().unwrap_or(attachments))?;
        if destination_directory == attachments {
            return Ok(());
        }
        if destination_directory == source.parent().unwrap_or(attachments) {
            return Ok(());
        }
        let destination =
            destination_directory.join(source.file_name().ok_or(Error::DataDirectory)?);
        plan(destination.to_string_lossy().into_owned());
        Ok(())
    }

    pub fn save(&mut self, batch: &SaveBatch) -> Result<()> {
        let transaction = self.connection.transaction()?;
        {
            let mut page_statement = transaction.prepare_cached(
                "UPDATE pages SET title=?1,icon=?2,cover=?3,cover_image=?4,cover_x=?5,cover_y=?6,full_width=?7,updated_at=unixepoch() WHERE id=?8",
            )?;
            for page in &batch.pages {
                let parent_info: Option<(Option<crate::domain::model::PageId>, String)> =
                    transaction
                        .query_row(
                            "SELECT parent_id, title FROM pages WHERE id=?1",
                            [page.id],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .ok();
                let affected = page_statement.execute(rusqlite::params![
                    page.title,
                    page.icon,
                    page.cover,
                    page.cover_image,
                    page.cover_x.clamp(0.0, 1.0),
                    page.cover_y.clamp(0.0, 1.0),
                    i32::from(page.full_width),
                    page.id
                ])?;
                if affected != 1 {
                    return Err(Error::Invalid(format!(
                        "Page to save was not found: {}",
                        page.id
                    )));
                }
                if let Some((Some(parent_id), old_title)) = parent_info {
                    let clean_title = page.title.trim();
                    let display_title = if clean_title.is_empty() {
                        UNTITLED
                    } else {
                        clean_title
                    };
                    let new_marker = format!("↗ {display_title}");
                    let old_target = if old_title.trim().is_empty() {
                        UNTITLED
                    } else {
                        old_title.trim()
                    };
                    let old_marker = format!("↗ {old_target}");
                    transaction.execute(
                        "UPDATE blocks SET content=?1 WHERE page_id=?2 AND kind='text' AND (trim(content)=?3 OR (?4 AND trim(content)='↗'))",
                        rusqlite::params![new_marker, parent_id, old_marker, old_title.trim().is_empty()],
                    )?;
                }
            }
            let mut block_statement = transaction.prepare_cached(
                "UPDATE blocks SET kind=?1,content=?2,checked=?3,media_width=?4,text_styles=?5 WHERE id=?6 AND page_id=?7",
            )?;
            let mut touch_statement = transaction
                .prepare_cached("UPDATE pages SET updated_at=unixepoch() WHERE id=?1")?;
            let mut touched = std::collections::HashSet::new();
            for block in &batch.blocks {
                let affected = block_statement.execute(rusqlite::params![
                    block.kind.as_str(),
                    block.content,
                    block.checked,
                    block.media_width.clamp(0.25, 1.0),
                    serde_json::to_string(&block.text_styles).map_err(|error| {
                        Error::Invalid(format!("Invalid text formatting: {error}"))
                    })?,
                    block.id,
                    block.page_id
                ])?;
                if affected != 1 {
                    return Err(Error::Invalid(format!(
                        "Block to save was not found: {}",
                        block.id
                    )));
                }
                touched.insert(block.page_id);
            }
            for id in touched {
                touch_statement.execute([id])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn backup(&self, path: &Path) -> Result<()> {
        self.connection.backup(rusqlite::MAIN_DB, path, None)?;
        Ok(())
    }

    pub fn restore_from(&mut self, path: &Path) -> Result<()> {
        self.connection.restore(
            rusqlite::MAIN_DB,
            path,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        Ok(())
    }

    pub fn setting(&self, key: SettingKey) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [key.as_str()],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: SettingKey, value: &str) -> Result<()> {
        self.connection.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key.as_str(), value])?;
        Ok(())
    }
}
