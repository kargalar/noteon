mod blocks;
mod databases;
mod files;
mod page_properties;
mod pages;
mod schema;
mod search;

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::config::SettingKey;
use crate::domain::model::SaveBatch;
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
        Ok(Self { connection })
    }

    pub fn save(&mut self, batch: &SaveBatch) -> Result<()> {
        let transaction = self.connection.transaction()?;
        {
            let mut page_statement = transaction.prepare_cached(
                "UPDATE pages SET title=?1,icon=?2,cover=?3,cover_image=?4,cover_x=?5,cover_y=?6,full_width=?7,updated_at=unixepoch() WHERE id=?8",
            )?;
            for page in &batch.pages {
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
