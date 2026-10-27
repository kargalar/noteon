use std::path::Path;

use serde_json::{Map, Value, json};

use crate::domain::markdown;
use crate::domain::model::{
    Block, BlockKind, DatabaseData, DatabaseProperty, DatabaseView, PROPERTY_TYPES, Page, PageEdit,
    PageId, PropertyOption, StyledBlockContent,
};
use crate::error::{Error, Result};
use crate::storage::Repository;

const SUBPAGE_MARKER: char = '↗';
const MAX_BLOCKS_PER_READ: usize = 1000;
const MAX_ROWS_PER_READ: usize = 1000;
const MAX_TITLE_LENGTH: usize = 500;

pub struct ToolHost {
    repository: Repository,
}

impl ToolHost {
    pub fn open(database: &Path) -> Result<Self> {
        Ok(Self {
            repository: Repository::open(database)?,
        })
    }

    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value> {
        match name {
            "list_pages" => self.list_pages(args),
            "search_notes" => self.search_notes(args),
            "read_page" => self.read_page(args),
            "create_page" => self.create_page(args),
            "update_page" => self.update_page(args),
            "move_page" => self.move_page(args),
            "trash_pages" => self.trash_pages(args),
            "restore_pages" => self.restore_pages(args),
            "duplicate_page" => self.duplicate_page(args),
            "insert_content" => self.insert_content(args),
            "replace_page_content" => self.replace_page_content(args),
            "update_blocks" => self.update_blocks(args),
            "delete_blocks" => self.delete_blocks(args),
            "move_block" => self.move_block(args),
            "set_page_property" => self.set_page_property(args),
            "get_database" => self.get_database(args),
            "create_database" => self.create_database(args),
            "add_property" => self.add_property(args),
            "update_property" => self.update_property(args),
            "delete_property" => self.delete_property(args),
            "add_rows" => self.add_rows(args),
            "update_rows" => self.update_rows(args),
            "set_database_view" => self.set_database_view(args),
            other => Err(Error::Invalid(format!("Unknown tool: {other}"))),
        }
    }

    fn list_pages(&self, args: &Value) -> Result<Value> {
        let include_trashed = opt_bool(args, "include_trashed").unwrap_or(false);
        let pages: Vec<Value> = self
            .repository
            .list_pages()?
            .iter()
            .filter(|page| include_trashed || page.trashed_by.is_none())
            .map(page_summary)
            .collect();
        Ok(json!({ "count": pages.len(), "pages": pages }))
    }

    fn search_notes(&self, args: &Value) -> Result<Value> {
        let hits: Vec<Value> = self
            .repository
            .search(&req_str(args, "query")?)?
            .into_iter()
            .map(|hit| {
                json!({
                    "page_id": hit.page_id,
                    "title": hit.title,
                    "preview": hit.preview,
                    "block_position": hit.position,
                })
            })
            .collect();
        Ok(json!({ "hits": hits }))
    }

    fn read_page(&self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "page_id")?;
        let page = self.repository.page(id)?;
        if page.is_database {
            return self.get_database(args);
        }
        let offset = opt_i64(args, "offset").unwrap_or(0).max(0) as usize;
        let limit =
            (opt_i64(args, "limit").unwrap_or(500).max(1) as usize).min(MAX_BLOCKS_PER_READ);
        let blocks: Vec<Value> = self
            .repository
            .blocks(id, offset, limit)?
            .iter()
            .map(block_json)
            .collect();
        let properties: Vec<Value> = self
            .repository
            .page_properties(id)?
            .iter()
            .map(|property| {
                json!({
                    "id": property.id, "name": property.name,
                    "type": property.kind, "value": property.value,
                    "options": option_names(&property.options),
                })
            })
            .collect();
        Ok(json!({
            "page": page_summary(&page),
            "total_blocks": page.block_count,
            "properties": properties,
            "blocks": blocks,
        }))
    }

    fn create_page(&mut self, args: &Value) -> Result<Value> {
        let parent = opt_i64(args, "parent_id");
        let title = opt_str(args, "title").unwrap_or_default();
        let id = self.repository.create_page(parent, &title)?;
        if let Some(icon) = opt_str(args, "icon") {
            self.repository.set_page_icon(id, &icon)?;
        }
        if let Some(source) = opt_str(args, "markdown") {
            self.fill_page(id, &markdown::parse_styled_blocks(&source), true)?;
        }
        Ok(json!({ "page_id": id, "title": title }))
    }

    fn update_page(&mut self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "page_id")?;
        if let Some(title) = opt_str(args, "title") {
            self.rename_page(id, &title)?;
        }
        if let Some(icon) = opt_str(args, "icon") {
            self.repository.set_page_icon(id, &icon)?;
        }
        if let Some(favorite) = opt_bool(args, "favorite")
            && self.repository.page(id)?.favorite != favorite
        {
            self.repository.toggle_favorite(id)?;
        }
        if let Some(priority) = opt_i64(args, "priority") {
            self.repository
                .set_page_priority(id, priority.clamp(0, 3) as u8)?;
        }
        Ok(json!({ "page": page_summary(&self.repository.page(id)?) }))
    }

    fn move_page(&mut self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "page_id")?;
        self.repository.move_page(id, opt_i64(args, "parent_id"))?;
        Ok(json!({ "ok": true }))
    }

    fn trash_pages(&mut self, args: &Value) -> Result<Value> {
        let ids = req_ids(args, "page_ids")?;
        self.repository.trash_pages(&ids)?;
        Ok(json!({ "trashed": ids }))
    }

    fn restore_pages(&mut self, args: &Value) -> Result<Value> {
        let ids = req_ids(args, "page_ids")?;
        self.repository.restore_pages(&ids)?;
        Ok(json!({ "restored": ids }))
    }

    fn duplicate_page(&mut self, args: &Value) -> Result<Value> {
        let id = self.repository.duplicate_page(req_i64(args, "page_id")?)?;
        Ok(json!({ "page_id": id }))
    }

    fn insert_content(&mut self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "page_id")?;
        let blocks = if let Some(source) = opt_str(args, "markdown") {
            markdown::parse_styled_blocks(&source)
        } else {
            parse_block_specs(args.get("blocks"))?
        };
        let count = self.repository.page(id)?.block_count;
        let position = opt_i64(args, "position").map_or(count, |value| value.max(0) as usize);
        self.repository
            .append_blocks_with_styles(id, position.min(count), &blocks)?;
        self.drop_leading_empty_block(id, !blocks.is_empty())?;
        Ok(json!({ "inserted": blocks.len() }))
    }

    fn replace_page_content(&mut self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "page_id")?;
        let blocks = markdown::parse_styled_blocks(&req_str(args, "markdown")?);
        self.fill_page(id, &blocks, true)?;
        Ok(json!({ "blocks": blocks.len() }))
    }

    fn update_blocks(&mut self, args: &Value) -> Result<Value> {
        let page = req_i64(args, "page_id")?;
        let edits = args
            .get("edits")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Invalid("`edits` must be an array.".into()))?;
        let mut existing = self.all_blocks(page)?;
        let mut changed = Vec::new();
        for edit in edits {
            let block_id = req_i64(edit, "block_id")?;
            let block = existing
                .iter_mut()
                .find(|block| block.id == block_id)
                .ok_or_else(|| {
                    Error::Invalid(format!("Block {block_id} is not on page {page}."))
                })?;
            apply_block_edit(block, edit)?;
            changed.push(block.clone());
        }
        let batch = crate::domain::model::SaveBatch {
            pages: Vec::new(),
            blocks: changed,
        };
        self.repository.save(&batch)?;
        Ok(json!({ "updated": batch.blocks.len() }))
    }

    fn delete_blocks(&mut self, args: &Value) -> Result<Value> {
        let page = req_i64(args, "page_id")?;
        let ids = req_ids(args, "block_ids")?;
        self.repository.delete_blocks(page, &ids)?;
        Ok(json!({ "deleted": ids.len() }))
    }

    fn move_block(&mut self, args: &Value) -> Result<Value> {
        let page = req_i64(args, "page_id")?;
        let direction = req_i64(args, "direction")?.signum() as i32;
        let ids = [req_i64(args, "block_id")?];
        self.repository.move_blocks(page, &ids, direction)?;
        Ok(json!({ "ok": true }))
    }

    fn set_page_property(&mut self, args: &Value) -> Result<Value> {
        let page = req_i64(args, "page_id")?;
        let name = req_str(args, "name")?;
        let value = opt_str(args, "value").unwrap_or_default();
        let kind = opt_str(args, "type").unwrap_or_else(|| "text".into());
        let options = parse_options(args.get("options"));
        let find = |host: &Self| -> Result<Option<i64>> {
            Ok(host
                .repository
                .page_properties(page)?
                .iter()
                .find(|property| property.name.eq_ignore_ascii_case(&name))
                .map(|property| property.id))
        };
        let property = match find(self)? {
            Some(id) => id,
            None => {
                self.repository
                    .add_page_property(page, &name, &kind, &options)?;
                find(self)?.ok_or_else(|| Error::Invalid("Property was not created.".into()))?
            }
        };
        self.repository
            .set_page_property_value(page, property, &value)?;
        Ok(json!({ "property_id": property }))
    }

    fn get_database(&self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "page_id").or_else(|_| req_i64(args, "database_id"))?;
        let data = self.repository.database(id)?;
        Ok(database_json(&data))
    }

    fn create_database(&mut self, args: &Value) -> Result<Value> {
        let title = opt_str(args, "title").unwrap_or_else(|| "New database".into());
        let id = if let Some(host) = opt_i64(args, "inline_in_page_id") {
            let position = self.repository.page(host)?.block_count;
            self.repository.create_database_block(host, position)?
        } else {
            self.repository
                .create_database(opt_i64(args, "parent_id"))?
        };
        self.rename_page(id, &title)?;
        if let Some(icon) = opt_str(args, "icon") {
            self.repository.set_page_icon(id, &icon)?;
        }
        self.create_initial_properties(id, args)?;
        let rows = self.add_rows_to(id, args.get("rows"))?;
        let data = self.repository.database(id)?;
        let mut result = database_json(&data);
        result["created_rows"] = json!(rows);
        Ok(result)
    }

    fn create_initial_properties(&mut self, database: PageId, args: &Value) -> Result<()> {
        let Some(specs) = args.get("properties").and_then(Value::as_array) else {
            return Ok(());
        };
        let defaults = self.repository.database(database)?.properties;
        for spec in specs {
            self.add_property_spec(database, spec)?;
        }
        for default in defaults {
            self.repository.delete_database_property(default.id)?;
        }
        Ok(())
    }

    fn add_property_spec(&mut self, database: PageId, spec: &Value) -> Result<()> {
        let name = req_str(spec, "name")?;
        let kind = opt_str(spec, "type").unwrap_or_else(|| "text".into());
        let options = parse_options(spec.get("options"));
        self.repository
            .add_database_property(database, &name, &kind, &options)
    }

    fn add_property(&mut self, args: &Value) -> Result<Value> {
        let database = req_i64(args, "database_id")?;
        self.add_property_spec(database, args)?;
        Ok(database_json(&self.repository.database(database)?))
    }

    fn update_property(&mut self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "property_id")?;
        if let Some(name) = opt_str(args, "name") {
            self.repository.rename_database_property(id, &name)?;
        }
        if let Some(kind) = opt_str(args, "type") {
            self.repository.set_database_property_type(id, &kind)?;
        }
        if args.get("options").is_some() {
            let options = parse_options(args.get("options"));
            self.repository
                .set_database_property_options(id, &options)?;
        }
        Ok(json!({ "ok": true }))
    }

    fn delete_property(&mut self, args: &Value) -> Result<Value> {
        self.repository
            .delete_database_property(req_i64(args, "property_id")?)?;
        Ok(json!({ "ok": true }))
    }

    fn add_rows(&mut self, args: &Value) -> Result<Value> {
        let database = req_i64(args, "database_id")?;
        let ids = self.add_rows_to(database, args.get("rows"))?;
        Ok(json!({ "created_rows": ids }))
    }

    fn add_rows_to(&mut self, database: PageId, rows: Option<&Value>) -> Result<Vec<PageId>> {
        let mut created = Vec::new();
        for row in rows.and_then(Value::as_array).into_iter().flatten() {
            let record = self.repository.create_database_record(database)?;
            self.apply_row(database, record, row)?;
            created.push(record);
        }
        Ok(created)
    }

    fn update_rows(&mut self, args: &Value) -> Result<Value> {
        let rows = args
            .get("rows")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Invalid("`rows` must be an array.".into()))?;
        let mut touched = 0;
        for row in rows {
            let record = req_i64(row, "record_id")?;
            let database = self
                .repository
                .page(record)?
                .parent_id
                .ok_or_else(|| Error::Invalid(format!("Page {record} is not a record.")))?;
            self.apply_row(database, record, row)?;
            touched += 1;
        }
        Ok(json!({ "updated": touched }))
    }

    fn apply_row(&mut self, database: PageId, record: PageId, row: &Value) -> Result<()> {
        if let Some(title) = opt_str(row, "title") {
            self.rename_page(record, &title)?;
        }
        if let Some(priority) = opt_i64(row, "priority") {
            self.repository
                .set_page_priority(record, priority.clamp(0, 3) as u8)?;
        }
        if let Some(body) = opt_str(row, "markdown") {
            self.fill_page(record, &markdown::parse_styled_blocks(&body), true)?;
        }
        if opt_bool(row, "trashed") == Some(true) {
            return self.repository.trash_pages(&[record]);
        }
        for (name, value) in row
            .get("values")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            self.set_row_value(database, record, name, value)?;
        }
        Ok(())
    }

    fn set_row_value(
        &mut self,
        database: PageId,
        record: PageId,
        property_name: &str,
        value: &Value,
    ) -> Result<()> {
        let data = self.repository.database(database)?;
        let property = data
            .properties
            .iter()
            .find(|property| property.name.eq_ignore_ascii_case(property_name))
            .ok_or_else(|| Error::Invalid(format!("Unknown property: {property_name}")))?;
        let text = value_to_text(value);
        self.ensure_options(property, &text)?;
        self.repository
            .set_database_property(record, property.id, &text)
    }

    fn ensure_options(&mut self, property: &DatabaseProperty, value: &str) -> Result<()> {
        if !matches!(property.kind.as_str(), "select" | "status" | "multi-select") {
            return Ok(());
        }
        let mut options = property.options.clone();
        for name in value.split('|').filter(|name| !name.is_empty()) {
            if !options.iter().any(|option| option.name == name) {
                options.push(PropertyOption::new(name));
            }
        }
        if options.len() != property.options.len() {
            self.repository
                .set_database_property_options(property.id, &options)?;
        }
        Ok(())
    }

    fn set_database_view(&mut self, args: &Value) -> Result<Value> {
        let id = req_i64(args, "database_id")?;
        let view = DatabaseView::from_db(&req_str(args, "view")?);
        self.repository.set_database_view(id, view)?;
        Ok(json!({ "view": view.as_str() }))
    }

    fn rename_page(&mut self, id: PageId, title: &str) -> Result<()> {
        if title.len() > MAX_TITLE_LENGTH {
            return Err(Error::Invalid("Page title is too long.".into()));
        }
        let mut edit = PageEdit::from(&self.repository.page(id)?);
        edit.title = title.to_string();
        self.repository.save(&crate::domain::model::SaveBatch {
            pages: vec![edit],
            blocks: Vec::new(),
        })
    }

    fn all_blocks(&self, page: PageId) -> Result<Vec<Block>> {
        let count = self.repository.page(page)?.block_count.max(1);
        self.repository.blocks(page, 0, count)
    }

    fn fill_page(
        &mut self,
        page: PageId,
        blocks: &[StyledBlockContent],
        replace: bool,
    ) -> Result<()> {
        let existing = self.all_blocks(page)?;
        let count = self.repository.page(page)?.block_count;
        self.repository
            .append_blocks_with_styles(page, count, blocks)?;
        if replace && !blocks.is_empty() {
            let removable: Vec<i64> = existing
                .iter()
                .filter(|block| !is_subpage_marker(block))
                .map(|block| block.id)
                .collect();
            self.repository.delete_blocks(page, &removable)?;
        }
        Ok(())
    }

    fn drop_leading_empty_block(&mut self, page: PageId, inserted: bool) -> Result<()> {
        let blocks = self.all_blocks(page)?;
        let empty_first = blocks
            .first()
            .filter(|block| block.kind == BlockKind::Text && block.content.is_empty());
        if let (true, Some(first), true) = (inserted, empty_first, blocks.len() > 1) {
            self.repository.delete_blocks(page, &[first.id])?;
        }
        Ok(())
    }
}

fn is_subpage_marker(block: &Block) -> bool {
    block.kind == BlockKind::Text && block.content.trim_start().starts_with(SUBPAGE_MARKER)
        || matches!(block.kind, BlockKind::Database | BlockKind::PageLink)
}

fn apply_block_edit(block: &mut Block, edit: &Value) -> Result<()> {
    if let Some(kind) = opt_str(edit, "kind") {
        block.kind = parse_kind(&kind)?;
    }
    if let Some(content) = opt_str(edit, "content") {
        if block.kind == BlockKind::Code {
            block.content = content;
            block.text_styles.clear();
        } else {
            let (plain, styles) = markdown::parse_inline_styles(&content);
            block.content = plain;
            block.text_styles = styles;
        }
    }
    if let Some(checked) = opt_bool(edit, "checked") {
        block.checked = checked;
    }
    Ok(())
}

fn parse_kind(raw: &str) -> Result<BlockKind> {
    let kind = BlockKind::from_db(raw);
    if kind.as_str() == raw && BlockKind::ALL.contains(&kind) {
        Ok(kind)
    } else {
        Err(Error::Invalid(format!("Unsupported block kind: {raw}")))
    }
}

fn parse_block_specs(value: Option<&Value>) -> Result<Vec<StyledBlockContent>> {
    let specs = value
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("Provide `markdown` or a `blocks` array.".into()))?;
    specs
        .iter()
        .map(|spec| {
            let kind = parse_kind(&opt_str(spec, "kind").unwrap_or_else(|| "text".into()))?;
            let content = opt_str(spec, "content").unwrap_or_default();
            let (plain, styles) = if kind == BlockKind::Code {
                (content, Vec::new())
            } else {
                markdown::parse_inline_styles(&content)
            };
            Ok((
                kind,
                plain,
                opt_bool(spec, "checked").unwrap_or(false),
                styles,
            ))
        })
        .collect()
}

fn parse_options(value: Option<&Value>) -> Vec<PropertyOption> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| match item {
            Value::String(name) => Some(PropertyOption::new(name.clone())),
            Value::Object(map) => {
                let name = map.get("name")?.as_str()?.to_string();
                let color = map.get("color").and_then(Value::as_str).map(String::from);
                Some(PropertyOption::with_color(name, color))
            }
            _ => None,
        })
        .collect()
}

fn option_names(options: &[PropertyOption]) -> Vec<Value> {
    options
        .iter()
        .map(|option| json!({ "name": option.name, "color": option.color }))
        .collect()
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Array(items) => items
            .iter()
            .map(value_to_text)
            .collect::<Vec<_>>()
            .join("|"),
        other => other.to_string(),
    }
}

fn page_summary(page: &Page) -> Value {
    json!({
        "id": page.id,
        "parent_id": page.parent_id,
        "title": page.title,
        "icon": page.icon,
        "favorite": page.favorite,
        "is_database": page.is_database,
        "trashed": page.trashed_by.is_some(),
        "block_count": page.block_count,
        "priority": page.priority,
        "updated_at": page.updated_at,
    })
}

fn block_json(block: &Block) -> Value {
    let kind = if is_subpage_marker(block) {
        "subpage_link"
    } else {
        block.kind.as_str()
    };
    json!({
        "id": block.id,
        "position": block.position,
        "kind": kind,
        "content": block.content,
        "checked": block.checked,
    })
}

fn database_json(data: &DatabaseData) -> Value {
    let properties: Vec<Value> = data
        .properties
        .iter()
        .map(|property| {
            json!({
                "id": property.id, "name": property.name,
                "type": property.kind, "options": option_names(&property.options),
            })
        })
        .collect();
    let rows: Vec<Value> = data
        .rows
        .iter()
        .take(MAX_ROWS_PER_READ)
        .map(|row| {
            let values: Map<String, Value> = data
                .properties
                .iter()
                .filter_map(|property| {
                    let value = row.values.get(&property.id)?;
                    Some((property.name.clone(), json!(value)))
                })
                .collect();
            json!({
                "record_id": row.page.id,
                "title": row.page.title,
                "priority": row.page.priority,
                "values": values,
            })
        })
        .collect();
    json!({
        "database_id": data.page_id,
        "title": data.title,
        "view": data.view.as_str(),
        "property_types_available": PROPERTY_TYPES.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        "properties": properties,
        "row_count": data.rows.len(),
        "rows": rows,
    })
}

fn req_i64(args: &Value, key: &str) -> Result<i64> {
    opt_i64(args, key).ok_or_else(|| Error::Invalid(format!("Missing integer argument `{key}`.")))
}

fn opt_i64(args: &Value, key: &str) -> Option<i64> {
    let value = args.get(key)?;
    value
        .as_i64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn req_str(args: &Value, key: &str) -> Result<String> {
    opt_str(args, key).ok_or_else(|| Error::Invalid(format!("Missing string argument `{key}`.")))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)?.as_str().map(String::from)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key)?.as_bool()
}

fn req_ids(args: &Value, key: &str) -> Result<Vec<i64>> {
    let ids: Vec<i64> = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid(format!("`{key}` must be an array of ids.")))?
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    if ids.is_empty() {
        return Err(Error::Invalid(format!("`{key}` must not be empty.")));
    }
    Ok(ids)
}

pub fn definitions() -> Vec<Value> {
    super::schema::tool_definitions()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> (tempfile::TempDir, ToolHost) {
        let directory = tempfile::tempdir().unwrap();
        let host = ToolHost::open(&directory.path().join("test.sqlite")).unwrap();
        (directory, host)
    }

    #[test]
    fn creates_page_with_markdown_and_edits_blocks() {
        let (_directory, mut host) = host();
        let created = host
            .call(
                "create_page",
                &json!({ "title": "Plan", "markdown": "# Başlık\n- bir\n- iki" }),
            )
            .unwrap();
        let page = created["page_id"].as_i64().unwrap();
        let read = host.call("read_page", &json!({ "page_id": page })).unwrap();
        let blocks = read["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0]["kind"], "h1");
        let block_id = blocks[1]["id"].as_i64().unwrap();
        host.call(
            "update_blocks",
            &json!({ "page_id": page, "edits": [{ "block_id": block_id, "content": "değişti", "kind": "todo", "checked": true }] }),
        )
        .unwrap();
        let read = host.call("read_page", &json!({ "page_id": page })).unwrap();
        assert_eq!(read["blocks"][1]["content"], "değişti");
        assert_eq!(read["blocks"][1]["kind"], "todo");
    }

    #[test]
    fn unknown_row_property_is_reported() {
        let (_directory, mut host) = host();
        let page = host
            .call("create_page", &json!({ "title": "Host" }))
            .unwrap()["page_id"]
            .as_i64()
            .unwrap();
        let table = host
            .call(
                "create_database",
                &json!({
                    "title": "Görevler", "inline_in_page_id": page,
                    "properties": [
                        { "name": "Durum", "type": "status", "options": ["Yapılacak", "Bitti"] },
                        { "name": "Puan", "type": "number" },
                    ],
                    "rows": [{ "title": "İlk", "values": { "Durum": "Bitti", "Puan": 5, "Etiket": null } }],
                }),
            );
        assert!(table.is_err(), "unknown property must be reported");
    }

    #[test]
    fn rows_can_be_added_and_updated() {
        let (_directory, mut host) = host();
        let table = host
            .call(
                "create_database",
                &json!({
                    "title": "Görevler",
                    "properties": [
                        { "name": "Durum", "type": "status", "options": ["Yapılacak"] },
                        { "name": "Puan", "type": "number" },
                    ],
                    "rows": [{ "title": "İlk", "values": { "Durum": "Bitti", "Puan": 5 } }],
                }),
            )
            .unwrap();
        let record = table["created_rows"][0].as_i64().unwrap();
        let database = table["database_id"].as_i64().unwrap();
        host.call(
            "update_rows",
            &json!({ "rows": [{ "record_id": record, "values": { "Puan": 9 } }] }),
        )
        .unwrap();
        let data = host
            .call("get_database", &json!({ "database_id": database }))
            .unwrap();
        assert_eq!(data["rows"][0]["values"]["Puan"], "9");
        assert_eq!(data["rows"][0]["values"]["Durum"], "Bitti");
    }

    #[test]
    fn inline_markdown_styles_are_parsed_into_block_styles() {
        let (_directory, mut host) = host();
        let created = host
            .call(
                "create_page",
                &json!({ "title": "Bütçe", "markdown": "- **Maaş:** 70.000 (`net`)" }),
            )
            .unwrap();
        let page = created["page_id"].as_i64().unwrap();
        let blocks = host.repository.blocks(page, 0, 10).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "Maaş: 70.000 (net)");
        assert_eq!(blocks[0].text_styles.len(), 2);
        assert!(blocks[0].text_styles[0].style.bold);
        assert_eq!(blocks[0].text_styles[0].start, 0);
        assert_eq!(blocks[0].text_styles[0].end, 5);
    }
}
