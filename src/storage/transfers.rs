use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, Transaction, params};

use super::{Repository, blocks::normalize_page_blocks, sql_index};
use crate::domain::model::{
    DatabaseViewSettings, PageId, parse_property_options, serialize_property_options,
};
use crate::error::{Error, Result};

impl Repository {
    /// Atomically move or clone complete page trees and insert page blocks at the destination.
    pub fn transfer_pages(
        &mut self,
        pages: &[PageId],
        parent: Option<PageId>,
        position: Option<usize>,
        replace_block: Option<i64>,
        copy: bool,
    ) -> Result<Vec<PageId>> {
        if pages.is_empty() {
            return Err(Error::Invalid("Select at least one page.".into()));
        }
        let transaction = self.connection.transaction()?;
        let mut seen = HashSet::new();
        let mut sources = Vec::new();
        for &id in pages {
            if !seen.insert(id) {
                continue;
            }
            let source = transaction.query_row(
                "SELECT parent_id,title FROM pages WHERE id=?1 AND trashed_by IS NULL",
                [id],
                |row| Ok((row.get::<_, Option<PageId>>(0)?, row.get::<_, String>(1)?)),
            )?;
            if let Some(parent) = parent {
                let invalid: bool = transaction.query_row(
                    "WITH RECURSIVE descendants(id) AS (SELECT ?1 UNION ALL SELECT p.id FROM pages p JOIN descendants d ON p.parent_id=d.id)
                     SELECT EXISTS(SELECT 1 FROM descendants WHERE id=?2) OR NOT EXISTS(SELECT 1 FROM pages WHERE id=?2 AND trashed_by IS NULL)",
                    params![id, parent], |row| row.get(0),
                )?;
                if invalid {
                    return Err(Error::Invalid(
                        "A page cannot be placed inside itself or a subpage.".into(),
                    ));
                }
            }
            sources.push((id, source.0, source.1));
        }
        // Avoid moving a child twice when an ancestor is already in the payload.
        let selected: HashSet<_> = sources.iter().map(|source| source.0).collect();
        let mut roots = Vec::new();
        for source in sources {
            let mut ancestor = source.1;
            let mut covered = false;
            while let Some(id) = ancestor {
                if selected.contains(&id) {
                    covered = true;
                    break;
                }
                ancestor = transaction.query_row(
                    "SELECT parent_id FROM pages WHERE id=?1",
                    [id],
                    |row| row.get(0),
                )?;
            }
            if !covered {
                roots.push(source);
            }
        }
        let destination_is_database = parent
            .map(|id| {
                transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM databases WHERE page_id=?1)",
                    [id],
                    |row| row.get::<_, bool>(0),
                )
            })
            .transpose()?
            .unwrap_or(false);
        let mut position = position.unwrap_or(usize::MAX);
        if let Some(parent) = parent.filter(|_| !destination_is_database)
            && let Some(block) = replace_block
        {
            let actual_position = transaction.query_row(
                    "SELECT position FROM blocks WHERE id=?1 AND page_id=?2 AND kind='text' AND content=''",
                    params![block, parent], |row| row.get::<_, i64>(0),
                ).optional()?.ok_or_else(|| Error::Invalid("The destination block is no longer empty.".into()))?;
            position = usize::try_from(actual_position)
                .map_err(|_| Error::Invalid("Invalid block position.".into()))?;
            transaction.execute("DELETE FROM blocks WHERE id=?1", [block])?;
            normalize_page_blocks(&transaction, parent)?;
        }
        let mut result = Vec::new();
        let mut page_mapping = HashMap::new();
        let mut property_mapping = HashMap::new();
        for (source, old_parent, title) in roots {
            let id = if copy {
                clone_page_tree(
                    &transaction,
                    source,
                    parent,
                    &mut page_mapping,
                    &mut property_mapping,
                )?
            } else {
                transaction.execute(
                    "UPDATE pages SET parent_id=?1,sort_order=(SELECT COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1 AND id!=?2),updated_at=unixepoch() WHERE id=?2",
                    params![parent, source],
                )?;
                if let Some(old_parent) = old_parent {
                    remove_page_block(&transaction, old_parent, source, &title)?;
                }
                source
            };
            preserve_record_properties(
                &transaction,
                source,
                old_parent,
                id,
                parent,
                destination_is_database,
            )?;
            transaction.execute("DELETE FROM page_property_values WHERE page_id=?1 AND property_id IN (SELECT id FROM database_properties WHERE database_page_id IS NOT ?2)", params![id, parent.filter(|_| destination_is_database)])?;
            result.push(id);
        }
        remap_cloned_references(&transaction, &page_mapping, &property_mapping)?;
        if let Some(parent) = parent.filter(|_| !destination_is_database) {
            let count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM blocks WHERE page_id=?1",
                [parent],
                |row| row.get(0),
            )?;
            let position = sql_index(position.min(usize::try_from(count).unwrap_or(0)))?;
            transaction.execute(
                "UPDATE blocks SET position=position+?1 WHERE page_id=?2 AND position>=?3",
                params![sql_index(result.len())?, parent, position],
            )?;
            for (offset, id) in result.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,'page_link',?3)",
                    params![parent, position + sql_index(offset)?, id.to_string()],
                )?;
            }
            normalize_page_blocks(&transaction, parent)?;
            transaction.execute(
                "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
                [parent],
            )?;
        }
        transaction.commit()?;
        Ok(result)
    }
}

fn remove_page_block(
    transaction: &Transaction<'_>,
    parent: PageId,
    page: PageId,
    title: &str,
) -> Result<()> {
    transaction.execute(
        "DELETE FROM blocks WHERE page_id=?1 AND kind='page_link' AND content=?2",
        params![parent, page.to_string()],
    )?;
    // Old workspaces use title links. Remove only unambiguous links.
    let marker = format!(
        "↗ {}",
        if title.trim().is_empty() {
            "Untitled"
        } else {
            title.trim()
        }
    );
    transaction.execute(
        "DELETE FROM blocks WHERE page_id=?1 AND kind='text' AND content=?2 AND NOT EXISTS(SELECT 1 FROM pages WHERE parent_id=?1 AND title=?3 AND id!=?4)",
        params![parent, marker, title, page],
    )?;
    normalize_page_blocks(transaction, parent)
}

fn clone_page_tree(
    transaction: &Transaction<'_>,
    source: PageId,
    parent: Option<PageId>,
    pages: &mut HashMap<PageId, PageId>,
    properties: &mut HashMap<i64, i64>,
) -> Result<PageId> {
    // Read children before inserting a copy, so the traversal never follows its own output.
    let children = {
        let mut statement = transaction.prepare_cached(
            "SELECT id FROM pages WHERE parent_id=?1 AND trashed_by IS NULL ORDER BY sort_order,id",
        )?;
        statement
            .query_map([source], |row| row.get::<_, PageId>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    transaction.execute(
        "INSERT INTO pages(parent_id,title,icon,cover,cover_image,cover_x,cover_y,sort_order,full_width,priority,created_at,updated_at)
         SELECT ?1,title,icon,cover,cover_image,cover_x,cover_y,(SELECT COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1),full_width,priority,unixepoch(),unixepoch() FROM pages WHERE id=?2",
        params![parent, source],
    )?;
    let target = transaction.last_insert_rowid();
    pages.insert(source, target);
    transaction.execute(
        "INSERT INTO blocks(page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles)
         SELECT ?1,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles FROM blocks WHERE page_id=?2 ORDER BY position",
        params![target, source],
    )?;
    transaction.execute(
        "INSERT INTO databases(page_id,view_mode,sort_key,sort_descending,gallery_columns,view_settings)
         SELECT ?1,view_mode,sort_key,sort_descending,gallery_columns,view_settings FROM databases WHERE page_id=?2", params![target, source],
    )?;
    let database_properties = {
        let mut statement = transaction.prepare_cached(
            "SELECT id FROM database_properties WHERE database_page_id=?1 ORDER BY position",
        )?;
        statement
            .query_map([source], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for property in database_properties {
        transaction.execute("INSERT INTO database_properties(database_page_id,name,kind,options,position) SELECT ?1,name,kind,options,position FROM database_properties WHERE id=?2", params![target, property])?;
        properties.insert(property, transaction.last_insert_rowid());
    }
    let custom_properties = {
        let mut statement = transaction.prepare_cached(
            "SELECT id FROM custom_page_properties WHERE page_id=?1 ORDER BY position",
        )?;
        statement
            .query_map([source], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for property in custom_properties {
        transaction.execute("INSERT INTO custom_page_properties(page_id,name,kind,options,position) SELECT ?1,name,kind,options,position FROM custom_page_properties WHERE id=?2", params![target, property])?;
        let new_property = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO custom_page_property_values(page_id,property_id,value) SELECT ?1,?2,value FROM custom_page_property_values WHERE page_id=?3 AND property_id=?4", params![target, new_property, source, property])?;
    }
    transaction.execute("INSERT INTO page_property_values(page_id,property_id,value) SELECT ?1,property_id,value FROM page_property_values WHERE page_id=?2", params![target, source])?;
    for child in children {
        clone_page_tree(transaction, child, Some(target), pages, properties)?;
    }
    Ok(target)
}

fn remap_cloned_references(
    transaction: &Transaction<'_>,
    pages: &HashMap<PageId, PageId>,
    properties: &HashMap<i64, i64>,
) -> Result<()> {
    for &page in pages.values() {
        for (old, new) in pages {
            transaction.execute("UPDATE blocks SET content=?1 WHERE page_id=?2 AND kind IN ('database','page_link') AND content=?3", params![new.to_string(), page, old.to_string()])?;
        }
        for (old, new) in properties {
            transaction.execute("UPDATE page_property_values SET property_id=?1 WHERE page_id=?2 AND property_id=?3", params![new, page, old])?;
        }
        let settings = transaction
            .query_row(
                "SELECT view_settings,sort_key FROM databases WHERE page_id=?1",
                [page],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((settings, sort_key)) = settings {
            let mut settings: DatabaseViewSettings =
                serde_json::from_str(&settings).map_err(|error| {
                    Error::Invalid(format!("Invalid database view settings: {error}"))
                })?;
            for visible in [
                &mut settings.gallery_visible_properties,
                &mut settings.table_visible_properties,
                &mut settings.board_visible_properties,
            ]
            .into_iter()
            .flatten()
            {
                for id in visible {
                    *id = properties.get(id).copied().unwrap_or(*id);
                }
            }
            settings.table_column_widths = settings
                .table_column_widths
                .into_iter()
                .map(|(id, width)| (properties.get(&id).copied().unwrap_or(id), width))
                .collect();
            settings.table_column_calculations = settings
                .table_column_calculations
                .into_iter()
                .map(|(id, calculation)| (properties.get(&id).copied().unwrap_or(id), calculation))
                .collect();
            let sort_key = sort_key
                .parse::<i64>()
                .ok()
                .and_then(|id| properties.get(&id))
                .map_or(sort_key, |id| id.to_string());
            transaction.execute(
                "UPDATE databases SET view_settings=?1,sort_key=?2 WHERE page_id=?3",
                params![
                    serde_json::to_string(&settings)
                        .map_err(|error| Error::Invalid(error.to_string()))?,
                    sort_key,
                    page
                ],
            )?;
        }
    }
    Ok(())
}

fn preserve_record_properties(
    transaction: &Transaction<'_>,
    source: PageId,
    source_parent: Option<PageId>,
    target: PageId,
    parent: Option<PageId>,
    database: bool,
) -> Result<()> {
    let values = {
        let mut statement = transaction.prepare_cached(
            "SELECT p.name,p.kind,p.options,COALESCE(v.value,'') FROM database_properties p LEFT JOIN page_property_values v ON p.id=v.property_id AND v.page_id=?1 WHERE p.database_page_id=?2
             UNION ALL SELECT p.name,p.kind,p.options,COALESCE(v.value,'') FROM custom_page_properties p LEFT JOIN custom_page_property_values v ON p.id=v.property_id AND v.page_id=?1 WHERE p.page_id=?1",
        )?;
        statement
            .query_map(params![source, source_parent], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (name, kind, options, value) in values {
        let owner = if database {
            parent.unwrap_or(target)
        } else {
            target
        };
        let (table, owner_column, value_table) = if database {
            (
                "database_properties",
                "database_page_id",
                "page_property_values",
            )
        } else {
            (
                "custom_page_properties",
                "page_id",
                "custom_page_property_values",
            )
        };
        let mut candidate = name.clone();
        let mut suffix = 1;
        let property = loop {
            let existing = transaction
                .query_row(
                    &format!(
                        "SELECT id,kind,options FROM {table} WHERE {owner_column}=?1 AND name=?2"
                    ),
                    params![owner, candidate],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            match existing {
                Some((id, existing_kind, existing_options)) if existing_kind == kind => {
                    let mut merged = parse_property_options(&existing_options);
                    for option in parse_property_options(&options) {
                        if !merged.iter().any(|existing| existing.name == option.name) {
                            merged.push(option);
                        }
                    }
                    transaction.execute(
                        &format!("UPDATE {table} SET options=?1 WHERE id=?2"),
                        params![serialize_property_options(&merged), id],
                    )?;
                    break id;
                }
                Some(_) => {
                    candidate = format!("{name} ({kind} {suffix})");
                    suffix += 1;
                }
                None => {
                    transaction.execute(&format!("INSERT INTO {table}({owner_column},name,kind,options,position) SELECT ?1,?2,?3,?4,COALESCE(MAX(position)+1,0) FROM {table} WHERE {owner_column}=?1"), params![owner, candidate, kind, options])?;
                    break transaction.last_insert_rowid();
                }
            }
        };
        transaction.execute(&format!("INSERT INTO {value_table}(page_id,property_id,value) VALUES(?1,?2,?3) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value"), params![target, property, value])?;
    }
    Ok(())
}
