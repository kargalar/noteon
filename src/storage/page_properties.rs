use rusqlite::{OptionalExtension, params};

use crate::domain::model::{PROPERTY_TYPES, PageId, PageProperty};
use crate::error::{Error, Result};

use super::Repository;

impl Repository {
    pub fn page_properties(&self, page: PageId) -> Result<Vec<PageProperty>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT p.id,p.name,p.kind,p.options,COALESCE(v.value,'') \
             FROM custom_page_properties p \
             LEFT JOIN custom_page_property_values v ON v.property_id=p.id AND v.page_id=?1 \
             WHERE p.page_id=?1 ORDER BY p.position,p.id",
        )?;
        Ok(statement
            .query_map([page], |row| {
                let options: String = row.get(3)?;
                Ok(PageProperty {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    options: options
                        .lines()
                        .map(str::trim)
                        .filter(|option| !option.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    value: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_page_property(
        &self,
        page: PageId,
        name: &str,
        kind: &str,
        options: &[String],
    ) -> Result<()> {
        let name = validate_property_name(name)?;
        validate_property_kind(kind)?;
        let mut options = normalize_options(options)?;
        if kind == "status" && options.is_empty() {
            options = crate::domain::model::DEFAULT_DATABASE_STATUS_OPTIONS
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
        }
        self.connection.execute(
            "INSERT INTO custom_page_properties(page_id,name,kind,options,position) \
             VALUES(?1,?2,?3,?4,(SELECT COALESCE(MAX(position)+1,0) FROM custom_page_properties WHERE page_id=?1))",
            params![page, name, kind, options.join("\n")],
        )?;
        Ok(())
    }

    pub fn rename_page_property(&self, page: PageId, property: i64, name: &str) -> Result<()> {
        let name = validate_property_name(name)?;
        ensure_property_exists(self, page, property)?;
        self.connection.execute(
            "UPDATE custom_page_properties SET name=?1 WHERE page_id=?2 AND id=?3",
            params![name, page, property],
        )?;
        Ok(())
    }

    pub fn set_page_property_type(&self, page: PageId, property: i64, kind: &str) -> Result<()> {
        validate_property_kind(kind)?;
        let existing: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT kind,options FROM custom_page_properties WHERE page_id=?1 AND id=?2",
                params![page, property],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((_old_kind, mut raw_options)) = existing else {
            return Err(Error::Invalid("Page property not found.".into()));
        };
        if kind == "status" && raw_options.trim().is_empty() {
            raw_options = crate::domain::model::DEFAULT_DATABASE_STATUS_OPTIONS.join("\n");
        }
        self.connection.execute(
            "UPDATE custom_page_properties SET kind=?1,options=?2 WHERE page_id=?3 AND id=?4",
            params![kind, raw_options, page, property],
        )?;
        let current_value: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM custom_page_property_values WHERE page_id=?1 AND property_id=?2",
                params![page, property],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(val) = current_value
            && !val.is_empty()
            && super::databases::validate_property_value(kind, &raw_options, &val).is_err()
        {
            self.connection.execute(
                "DELETE FROM custom_page_property_values WHERE page_id=?1 AND property_id=?2",
                params![page, property],
            )?;
        }
        Ok(())
    }

    pub fn set_page_property_options(
        &self,
        page: PageId,
        property: i64,
        options: &[String],
    ) -> Result<()> {
        let options = normalize_options(options)?;
        ensure_property_exists(self, page, property)?;
        self.connection.execute(
            "UPDATE custom_page_properties SET options=?1 WHERE page_id=?2 AND id=?3",
            params![options.join("\n"), page, property],
        )?;
        Ok(())
    }

    pub fn rename_page_property_option(
        &mut self,
        page: PageId,
        property: i64,
        old_option: &str,
        new_option: &str,
    ) -> Result<()> {
        let new_option = new_option.trim();
        let old_option = old_option.trim();
        if new_option.is_empty() || new_option.len() > 80 {
            return Err(Error::Invalid(
                "Option names must be 1–80 characters long.".into(),
            ));
        }
        let transaction = self.connection.transaction()?;
        let definition: Option<(String, String)> = transaction
            .query_row(
                "SELECT kind,options FROM custom_page_properties WHERE page_id=?1 AND id=?2",
                params![page, property],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((kind, raw_options)) = definition else {
            return Err(Error::Invalid("Page property not found.".into()));
        };
        let mut options: Vec<String> = raw_options
            .lines()
            .map(str::trim)
            .filter(|opt| !opt.is_empty())
            .map(str::to_owned)
            .collect();
        if let Some(pos) = options.iter().position(|opt| opt == old_option) {
            options[pos] = new_option.to_owned();
        } else if !options.iter().any(|opt| opt == new_option) {
            options.push(new_option.to_owned());
        }
        let mut deduped = Vec::new();
        for opt in options {
            if !deduped.contains(&opt) {
                deduped.push(opt);
            }
        }
        transaction.execute(
            "UPDATE custom_page_properties SET options=?1 WHERE page_id=?2 AND id=?3",
            params![deduped.join("\n"), page, property],
        )?;
        if old_option != new_option {
            let current_val: Option<String> = transaction
                .query_row(
                    "SELECT value FROM custom_page_property_values WHERE page_id=?1 AND property_id=?2",
                    params![page, property],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(val) = current_val {
                if kind == "multi-select" {
                    let parts: Vec<&str> = val.split('|').filter(|s| !s.is_empty()).collect();
                    if parts.contains(&old_option) {
                        let mut updated_parts = Vec::new();
                        for part in parts {
                            let target = if part == old_option { new_option } else { part };
                            if !updated_parts.contains(&target) {
                                updated_parts.push(target);
                            }
                        }
                        transaction.execute(
                            "UPDATE custom_page_property_values SET value=?1 WHERE page_id=?2 AND property_id=?3",
                            params![updated_parts.join("|"), page, property],
                        )?;
                    }
                } else if val == old_option {
                    transaction.execute(
                        "UPDATE custom_page_property_values SET value=?1 WHERE page_id=?2 AND property_id=?3",
                        params![new_option, page, property],
                    )?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_page_property_option(
        &mut self,
        page: PageId,
        property: i64,
        option_to_remove: &str,
    ) -> Result<()> {
        let option_to_remove = option_to_remove.trim();
        let transaction = self.connection.transaction()?;
        let definition: Option<(String, String)> = transaction
            .query_row(
                "SELECT kind,options FROM custom_page_properties WHERE page_id=?1 AND id=?2",
                params![page, property],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((kind, raw_options)) = definition else {
            return Err(Error::Invalid("Page property not found.".into()));
        };
        let options: Vec<String> = raw_options
            .lines()
            .map(str::trim)
            .filter(|opt| !opt.is_empty() && *opt != option_to_remove)
            .map(str::to_owned)
            .collect();
        transaction.execute(
            "UPDATE custom_page_properties SET options=?1 WHERE page_id=?2 AND id=?3",
            params![options.join("\n"), page, property],
        )?;
        let current_val: Option<String> = transaction
            .query_row(
                "SELECT value FROM custom_page_property_values WHERE page_id=?1 AND property_id=?2",
                params![page, property],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(val) = current_val {
            if kind == "multi-select" {
                let parts: Vec<&str> = val
                    .split('|')
                    .filter(|s| !s.is_empty() && *s != option_to_remove)
                    .collect();
                transaction.execute(
                    "UPDATE custom_page_property_values SET value=?1 WHERE page_id=?2 AND property_id=?3",
                    params![parts.join("|"), page, property],
                )?;
            } else if val == option_to_remove {
                transaction.execute(
                    "UPDATE custom_page_property_values SET value='' WHERE page_id=?1 AND property_id=?2",
                    params![page, property],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn move_page_property(&mut self, page: PageId, property: i64, direction: i32) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let mut ids: Vec<i64> = {
            let mut stmt = transaction.prepare(
                "SELECT id FROM custom_page_properties WHERE page_id=?1 ORDER BY position, id",
            )?;
            stmt.query_map([page], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let Some(index) = ids.iter().position(|id| *id == property) else {
            return Err(Error::Invalid("Page property not found.".into()));
        };
        let target = if direction < 0 {
            index.checked_sub(1)
        } else if direction > 0 && index + 1 < ids.len() {
            Some(index + 1)
        } else {
            None
        };
        if let Some(target) = target {
            ids.swap(index, target);
            for (pos, id) in ids.into_iter().enumerate() {
                transaction.execute(
                    "UPDATE custom_page_properties SET position=?1 WHERE page_id=?2 AND id=?3",
                    params![pos as i64, page, id],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_page_property(&self, page: PageId, property: i64) -> Result<()> {
        let affected = self.connection.execute(
            "DELETE FROM custom_page_properties WHERE page_id=?1 AND id=?2",
            params![page, property],
        )?;
        if affected == 0 {
            return Err(Error::Invalid("Page property not found.".into()));
        }
        Ok(())
    }

    pub fn set_page_property_value(&self, page: PageId, property: i64, value: &str) -> Result<()> {
        let definition: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT kind,options FROM custom_page_properties WHERE page_id=?1 AND id=?2",
                params![page, property],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((kind, options)) = definition else {
            return Err(Error::Invalid("Page property not found.".into()));
        };
        super::databases::validate_property_value(&kind, &options, value)?;
        self.connection.execute(
            "INSERT INTO custom_page_property_values(page_id,property_id,value) VALUES(?1,?2,?3) \
             ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value",
            params![page, property, value],
        )?;
        Ok(())
    }
}

fn validate_property_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(Error::Invalid(
            "Property names must be 1–80 characters long.".into(),
        ));
    }
    Ok(name)
}

fn validate_property_kind(kind: &str) -> Result<()> {
    if PROPERTY_TYPES
        .iter()
        .any(|(supported, _)| *supported == kind)
    {
        Ok(())
    } else {
        Err(Error::Invalid("Invalid property type.".into()))
    }
}

fn normalize_options(options: &[String]) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for value in options {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_owned());
        }
    }
    if normalized.len() > 100 || normalized.iter().any(|value| value.len() > 80) {
        return Err(Error::Invalid(
            "You can add up to 100 options; each option must be 80 characters or fewer.".into(),
        ));
    }
    Ok(normalized)
}

fn ensure_property_exists(repository: &Repository, page: PageId, property: i64) -> Result<()> {
    let exists: bool = repository.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM custom_page_properties WHERE page_id=?1 AND id=?2)",
        params![page, property],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(Error::Invalid("Page property not found.".into()))
    }
}
