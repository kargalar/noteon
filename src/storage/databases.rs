use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};

use crate::domain::model::{
    DEFAULT_DATABASE_STATUS_OPTIONS, DatabaseData, DatabaseProperty, DatabaseRow, DatabaseView,
    PROPERTY_TYPES, PageId,
};
use crate::error::{Error, Result};

use super::pages::{PAGE_COLUMNS, read_page};
use super::{Repository, read_usize, sql_index};

impl Repository {
    pub fn create_database_block(&mut self, page: PageId, position: usize) -> Result<PageId> {
        self.create_database_block_inner(page, position, None)
    }

    pub fn create_database_block_replacing(&mut self, page: PageId, block: i64) -> Result<PageId> {
        self.create_database_block_inner(page, 0, Some(block))
    }

    fn create_database_block_inner(
        &mut self,
        page: PageId,
        position: usize,
        replace_block: Option<i64>,
    ) -> Result<PageId> {
        let transaction = self.connection.transaction()?;
        let active: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pages WHERE id=?1 AND trashed_by IS NULL)",
            [page],
            |row| row.get(0),
        )?;
        if !active {
            return Err(Error::Invalid(
                "A database cannot be added to an inactive page.".into(),
            ));
        }

        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon,sort_order) SELECT ?1,'New database','▦',COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1",
            [page],
        )?;
        let database = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO databases(page_id) VALUES(?1)", [database])?;
        transaction.execute(
            "INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?1,'Status','status',?2,0)",
            params![database, DEFAULT_DATABASE_STATUS_OPTIONS.join("\n")],
        )?;
        transaction.execute(
            "INSERT INTO blocks(page_id,position) VALUES(?1,0)",
            [database],
        )?;

        if let Some(block) = replace_block {
            let belongs_to_page: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM blocks WHERE id=?1 AND page_id=?2)",
                params![block, page],
                |row| row.get(0),
            )?;
            if !belongs_to_page {
                return Err(Error::Invalid("Block to replace was not found.".into()));
            }
            transaction.execute(
                "UPDATE blocks SET kind='database',content=?1 WHERE id=?2",
                params![database.to_string(), block],
            )?;
        } else {
            let block_count: usize = transaction.query_row(
                "SELECT block_count FROM pages WHERE id=?1",
                [page],
                |row| read_usize(row, 0),
            )?;
            let position = sql_index(position.min(block_count))?;
            transaction.execute(
                "UPDATE blocks SET position=position+1 WHERE page_id=?1 AND position>=?2",
                params![page, position],
            )?;
            transaction.execute(
                "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,'database',?3)",
                params![page, position, database.to_string()],
            )?;
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(database)
    }

    pub fn database_property_owner(&self, property: i64) -> Result<Option<PageId>> {
        Ok(self
            .connection
            .query_row(
                "SELECT database_page_id FROM database_properties WHERE id=?1",
                [property],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn create_database(&mut self, parent: Option<PageId>) -> Result<PageId> {
        let transaction = self.connection.transaction()?;
        if let Some(parent) = parent {
            let active: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pages WHERE id=?1 AND trashed_by IS NULL)",
                [parent],
                |row| row.get(0),
            )?;
            if !active {
                return Err(Error::Invalid(
                    "A database cannot be added under an inactive page.".into(),
                ));
            }
        }
        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon,sort_order) SELECT ?1,'New database','▦',COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1",
            [parent],
        )?;
        let page_id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO databases(page_id) VALUES(?1)", [page_id])?;
        transaction.execute(
            "INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?1,'Durum','status',?2,0)",
            params![page_id, DEFAULT_DATABASE_STATUS_OPTIONS.join("\n")],
        )?;
        transaction.execute(
            "INSERT INTO blocks(page_id,position) VALUES(?1,0)",
            [page_id],
        )?;
        transaction.commit()?;
        Ok(page_id)
    }

    pub fn create_database_record(&mut self, database: PageId) -> Result<PageId> {
        let transaction = self.connection.transaction()?;
        let valid: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM databases WHERE page_id=?1) AND EXISTS(SELECT 1 FROM pages WHERE id=?1 AND trashed_by IS NULL)",
            [database],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(Error::Invalid("Database not found.".into()));
        }
        transaction.execute(
            "INSERT INTO pages(parent_id,title,sort_order) SELECT ?1,'New record',COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1",
            [database],
        )?;
        let page_id = transaction.last_insert_rowid();
        transaction.execute(
            "INSERT INTO blocks(page_id,position) VALUES(?1,0)",
            [page_id],
        )?;
        transaction.commit()?;
        Ok(page_id)
    }

    pub fn database(&self, page_id: PageId) -> Result<DatabaseData> {
        let (view, gallery_columns, sort_key, sort_descending, title): (String, u8, String, bool, String) = self.connection.query_row(
            "SELECT d.view_mode,d.gallery_columns,d.sort_key,d.sort_descending,p.title FROM databases d JOIN pages p ON p.id=d.page_id WHERE d.page_id=?1",
            [page_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;
        let mut property_statement = self.connection.prepare_cached(
            "SELECT id,name,kind,options FROM database_properties WHERE database_page_id=?1 ORDER BY position,id",
        )?;
        let properties = property_statement
            .query_map([page_id], |row| {
                let options: String = row.get(3)?;
                Ok(DatabaseProperty {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    options: options
                        .lines()
                        .map(str::trim)
                        .filter(|option| !option.is_empty())
                        .map(str::to_owned)
                        .collect(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut page_statement = self.connection.prepare_cached(&format!(
            "SELECT {PAGE_COLUMNS},(SELECT content FROM blocks WHERE page_id=pages.id AND kind!='divider' ORDER BY position LIMIT 1) FROM pages WHERE parent_id=?1 AND trashed_by IS NULL"
        ))?;
        let pages = page_statement
            .query_map([page_id], |row| {
                Ok((
                    read_page(row)?,
                    row.get::<_, Option<String>>(12)?.unwrap_or_default(),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut value_statement = self.connection.prepare_cached(
            "SELECT v.page_id,v.property_id,v.value FROM page_property_values v JOIN pages p ON p.id=v.page_id WHERE p.parent_id=?1 AND p.trashed_by IS NULL",
        )?;
        let mut values_by_page: HashMap<PageId, HashMap<i64, String>> = HashMap::new();
        for value in value_statement.query_map([page_id], |row| {
            Ok((
                row.get::<_, PageId>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (record, property, value) = value?;
            values_by_page
                .entry(record)
                .or_default()
                .insert(property, value);
        }
        let mut rows: Vec<_> = pages
            .into_iter()
            .map(|(page, preview)| DatabaseRow {
                values: values_by_page.remove(&page.id).unwrap_or_default(),
                page,
                preview,
            })
            .collect();
        let numeric_sort = sort_key
            .parse::<i64>()
            .ok()
            .and_then(|id| properties.iter().find(|property| property.id == id))
            .is_some_and(|property| property.kind == "number");
        rows.sort_by(|left, right| {
            let ordering = match sort_key.as_str() {
                "title" => left
                    .page
                    .display_title()
                    .to_lowercase()
                    .cmp(&right.page.display_title().to_lowercase()),
                "priority" => left.page.priority.cmp(&right.page.priority),
                "created_at" => left.page.created_at.cmp(&right.page.created_at),
                key => {
                    let property = key.parse::<i64>().unwrap_or_default();
                    if numeric_sort {
                        compare_optional_numbers(
                            left.values
                                .get(&property)
                                .and_then(|value| value.parse().ok()),
                            right
                                .values
                                .get(&property)
                                .and_then(|value| value.parse().ok()),
                        )
                    } else {
                        left.values.get(&property).cmp(&right.values.get(&property))
                    }
                }
            };
            if sort_descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        Ok(DatabaseData {
            page_id,
            title,
            view: DatabaseView::from_db(&view),
            gallery_columns,
            sort_key,
            sort_descending,
            properties,
            rows,
        })
    }

    pub fn add_database_property(
        &self,
        database: PageId,
        name: &str,
        kind: &str,
        options: &[String],
    ) -> Result<()> {
        const KINDS: &[&str] = &[
            "text",
            "number",
            "date",
            "status",
            "select",
            "multi-select",
            "checkbox",
        ];
        let name = name.trim();
        if name.is_empty() || name.len() > 80 || !KINDS.contains(&kind) {
            return Err(Error::Invalid("Invalid property name or type.".into()));
        }
        let options = options.join("\n");
        self.connection.execute(
            "INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?1,?2,?3,?4,(SELECT COALESCE(MAX(position)+1,0) FROM database_properties WHERE database_page_id=?1))",
            params![database, name, kind, options],
        )?;
        Ok(())
    }

    pub fn rename_database_property(&self, id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(Error::Invalid(
                "Property names must be 1–80 characters long.".into(),
            ));
        }
        self.connection.execute(
            "UPDATE database_properties SET name=?1 WHERE id=?2",
            params![name, id],
        )?;
        Ok(())
    }

    pub fn set_database_property_type(&self, id: i64, kind: &str) -> Result<()> {
        if !PROPERTY_TYPES
            .iter()
            .any(|(supported, _)| *supported == kind)
        {
            return Err(Error::Invalid("Invalid property type.".into()));
        }
        self.connection.execute(
            "UPDATE database_properties SET kind=?1 WHERE id=?2",
            params![kind, id],
        )?;
        Ok(())
    }

    pub fn set_database_property_options(&self, id: i64, options: &[String]) -> Result<()> {
        let options: Vec<_> = options
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect();
        if options.len() > 100 || options.iter().any(|value| value.len() > 80) {
            return Err(Error::Invalid(
                "You can add up to 100 options; each option must be 80 characters or fewer.".into(),
            ));
        }
        self.connection.execute(
            "UPDATE database_properties SET options=?1 WHERE id=?2",
            params![options.join("\n"), id],
        )?;
        Ok(())
    }

    pub fn delete_database_property(&self, id: i64) -> Result<()> {
        let owner = self.database_property_owner(id)?;
        self.connection
            .execute("DELETE FROM database_properties WHERE id=?1", [id])?;
        if let Some(owner) = owner {
            self.connection.execute(
                "UPDATE databases SET sort_key='created_at',sort_descending=1 WHERE page_id=?1 AND sort_key=?2",
                params![owner, id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn set_database_property(&self, page: PageId, property: i64, value: &str) -> Result<()> {
        let definition: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT d.kind,d.options FROM pages p JOIN database_properties d ON d.database_page_id=p.parent_id WHERE p.id=?1 AND p.trashed_by IS NULL AND d.id=?2",
                params![page, property],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((kind, options)) = definition else {
            return Err(Error::Invalid("Record or property not found.".into()));
        };
        validate_property_value(&kind, &options, value)?;
        self.connection.execute(
            "INSERT INTO page_property_values(page_id,property_id,value) VALUES(?1,?2,?3) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value",
            params![page, property, value],
        )?;
        Ok(())
    }

    pub fn set_page_priority(&self, page: PageId, priority: u8) -> Result<()> {
        if priority > 3 {
            return Err(Error::Invalid("Invalid priority value.".into()));
        }
        self.connection.execute(
            "UPDATE pages SET priority=?1,updated_at=unixepoch() WHERE id=?2",
            params![priority, page],
        )?;
        Ok(())
    }

    pub fn rename_database_record(&self, page: PageId, title: &str) -> Result<()> {
        if title.len() > 500 {
            return Err(Error::Invalid("Page name is too long.".into()));
        }
        self.connection.execute(
            "UPDATE pages SET title=?1,updated_at=unixepoch() WHERE id=?2 AND trashed_by IS NULL",
            params![title, page],
        )?;
        Ok(())
    }

    pub fn set_database_view(&self, page: PageId, view: DatabaseView) -> Result<()> {
        self.connection.execute(
            "UPDATE databases SET view_mode=?1 WHERE page_id=?2",
            params![view.as_str(), page],
        )?;
        Ok(())
    }

    pub fn set_database_gallery_columns(&self, page: PageId, columns: u8) -> Result<()> {
        if !(1..=6).contains(&columns) {
            return Err(Error::Invalid(
                "Gallery columns must be between 1 and 6.".into(),
            ));
        }
        self.connection.execute(
            "UPDATE databases SET gallery_columns=?1 WHERE page_id=?2",
            params![columns, page],
        )?;
        Ok(())
    }

    pub fn set_database_sort(&self, page: PageId, key: &str, descending: bool) -> Result<()> {
        let valid = matches!(key, "title" | "priority" | "created_at")
            || self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM database_properties WHERE database_page_id=?1 AND id=?2)",
                params![page, key.parse::<i64>().unwrap_or_default()],
                |row| row.get::<_, bool>(0),
            )?;
        if !valid {
            return Err(Error::Invalid("Sort property not found.".into()));
        }
        self.connection.execute(
            "UPDATE databases SET sort_key=?1,sort_descending=?2 WHERE page_id=?3",
            params![key, descending, page],
        )?;
        Ok(())
    }
}

fn compare_optional_numbers(left: Option<f64>, right: Option<f64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

pub(crate) fn validate_property_value(kind: &str, raw_options: &str, value: &str) -> Result<()> {
    let invalid = match kind {
        "number" => {
            !value.is_empty()
                && value
                    .parse::<f64>()
                    .map_or(true, |number| !number.is_finite())
        }
        "date" => !value.is_empty() && !is_iso_date(value),
        "checkbox" => !matches!(value, "true" | "false"),
        "select" | "status" => {
            !value.is_empty() && !raw_options.lines().any(|option| option.trim() == value)
        }
        "multi-select" => value
            .split('|')
            .filter(|option| !option.is_empty())
            .any(|value| !raw_options.lines().any(|option| option.trim() == value)),
        _ => false,
    };
    if invalid {
        return Err(Error::Invalid(
            "The property value does not match its type or available options.".into(),
        ));
    }
    Ok(())
}

fn is_iso_date(value: &str) -> bool {
    let mut parts = value.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 || !value.is_ascii() {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        year.parse::<i32>(),
        month.parse::<u32>(),
        day.parse::<u32>(),
    ) else {
        return false;
    };
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}
