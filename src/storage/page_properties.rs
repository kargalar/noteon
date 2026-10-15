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
        let options = normalize_options(options)?;
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
        ensure_property_exists(self, page, property)?;
        self.connection.execute(
            "UPDATE custom_page_properties SET kind=?1 WHERE page_id=?2 AND id=?3",
            params![kind, page, property],
        )?;
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
    let options: Vec<_> = options
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    if options.len() > 100 || options.iter().any(|value| value.len() > 80) {
        return Err(Error::Invalid(
            "You can add up to 100 options; each option must be 80 characters or fewer.".into(),
        ));
    }
    Ok(options)
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
