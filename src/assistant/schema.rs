use serde_json::{Value, json};

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
        },
    })
}

fn options_schema() -> Value {
    json!({
        "type": "array",
        "description": "Choices for select/multi-select/status. Strings or {name, color(hex)}.",
        "items": { "anyOf": [
            { "type": "string" },
            { "type": "object", "properties": { "name": {"type": "string"}, "color": {"type": "string"} }, "required": ["name"] }
        ]},
    })
}

fn property_spec_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "type": { "type": "string", "description": "text, number, select, multi-select, status, date (YYYY-MM-DD), checkbox (true/false), url, phone, email, person, files, ..." },
            "options": options_schema(),
        },
        "required": ["name"],
    })
}

fn row_schema(with_id: bool) -> Value {
    let mut properties = json!({
        "title": { "type": "string" },
        "priority": { "type": "integer", "description": "0 none, 1 low, 2 medium, 3 high" },
        "values": { "type": "object", "description": "Property name -> value. Multi-select: array of names. Missing select/status options are created automatically." },
        "markdown": { "type": "string", "description": "Body content of the record page." },
    });
    let mut required = Vec::new();
    if with_id {
        properties["record_id"] = json!({ "type": "integer" });
        properties["trashed"] =
            json!({ "type": "boolean", "description": "true moves the row to trash" });
        required.push("record_id");
    }
    json!({ "type": "object", "properties": properties, "required": required })
}

pub fn tool_definitions() -> Vec<Value> {
    let mut tools = page_tools();
    tools.extend(block_tools());
    tools.extend(database_tools());
    tools
}

fn page_tools() -> Vec<Value> {
    let page_id = json!({ "type": "integer" });
    vec![
        tool(
            "list_pages",
            "List every page (id, parent, title, icon, favorite, is_database). Use this first to learn the workspace tree.",
            json!({ "include_trashed": { "type": "boolean" } }),
            &[],
        ),
        tool(
            "search_notes",
            "Full-text search across page titles and block content.",
            json!({ "query": { "type": "string" } }),
            &["query"],
        ),
        tool(
            "read_page",
            "Read a page: metadata, custom properties and its blocks (id, kind, content, checked). For database pages returns the whole table.",
            json!({ "page_id": page_id, "offset": { "type": "integer" }, "limit": { "type": "integer" } }),
            &["page_id"],
        ),
        tool(
            "create_page",
            "Create a page (optionally under parent_id) with optional icon (emoji) and Markdown body.",
            json!({ "title": { "type": "string" }, "parent_id": page_id, "icon": { "type": "string" }, "markdown": { "type": "string" } }),
            &["title"],
        ),
        tool(
            "update_page",
            "Change a page's title, icon (emoji), favorite flag or priority (0-3).",
            json!({ "page_id": page_id, "title": { "type": "string" }, "icon": { "type": "string" }, "favorite": { "type": "boolean" }, "priority": { "type": "integer" } }),
            &["page_id"],
        ),
        tool(
            "move_page",
            "Move a page under another page (omit parent_id for the top level).",
            json!({ "page_id": page_id, "parent_id": page_id }),
            &["page_id"],
        ),
        tool(
            "trash_pages",
            "Move pages (and their subpages) to the trash. Recoverable.",
            json!({ "page_ids": { "type": "array", "items": page_id } }),
            &["page_ids"],
        ),
        tool(
            "restore_pages",
            "Restore pages from the trash.",
            json!({ "page_ids": { "type": "array", "items": page_id } }),
            &["page_ids"],
        ),
        tool(
            "duplicate_page",
            "Duplicate a page with its content.",
            json!({ "page_id": page_id }),
            &["page_id"],
        ),
        tool(
            "set_page_property",
            "Set a custom property on a normal page; creates it when missing.",
            json!({ "page_id": page_id, "name": { "type": "string" }, "value": { "type": "string" }, "type": { "type": "string" }, "options": options_schema() }),
            &["page_id", "name", "value"],
        ),
    ]
}

fn block_tools() -> Vec<Value> {
    let page_id = json!({ "type": "integer" });
    let block_kinds = json!({ "type": "string", "enum": ["text", "h1", "h2", "h3", "bullet", "numbered", "todo", "quote", "callout", "code", "divider"] });
    vec![
        tool(
            "insert_content",
            "Insert Markdown (or explicit blocks) into a page at a position (default: end). Supports headings, lists, - [ ] to-dos, quotes, code fences.",
            json!({ "page_id": page_id, "markdown": { "type": "string" }, "position": { "type": "integer" },
                    "blocks": { "type": "array", "items": { "type": "object", "properties": { "kind": block_kinds, "content": { "type": "string" }, "checked": { "type": "boolean" } } } } }),
            &["page_id"],
        ),
        tool(
            "replace_page_content",
            "Replace all body content of a page with Markdown. Subpage links and embedded databases are preserved.",
            json!({ "page_id": page_id, "markdown": { "type": "string" } }),
            &["page_id", "markdown"],
        ),
        tool(
            "update_blocks",
            "Edit existing blocks by id: content, kind and/or checked. Setting content clears inline formatting of that block.",
            json!({ "page_id": page_id, "edits": { "type": "array", "items": { "type": "object", "properties": { "block_id": { "type": "integer" }, "content": { "type": "string" }, "kind": block_kinds, "checked": { "type": "boolean" } }, "required": ["block_id"] } } }),
            &["page_id", "edits"],
        ),
        tool(
            "delete_blocks",
            "Delete blocks by id. Never delete `subpage_link` blocks unless the user asked to delete that subpage.",
            json!({ "page_id": page_id, "block_ids": { "type": "array", "items": { "type": "integer" } } }),
            &["page_id", "block_ids"],
        ),
        tool(
            "move_block",
            "Move one block up (-1) or down (+1).",
            json!({ "page_id": page_id, "block_id": { "type": "integer" }, "direction": { "type": "integer" } }),
            &["page_id", "block_id", "direction"],
        ),
    ]
}

fn database_tools() -> Vec<Value> {
    let database_id = json!({ "type": "integer" });
    vec![
        tool(
            "get_database",
            "Read a database/table: properties (with ids and options) and all rows with values.",
            json!({ "database_id": database_id }),
            &["database_id"],
        ),
        tool(
            "create_database",
            "Create a table. Set inline_in_page_id to embed it in a page (like the user's current page); otherwise it becomes a page under parent_id. Optionally define properties and initial rows in one call. When properties are given the default Status column is replaced.",
            json!({ "title": { "type": "string" }, "icon": { "type": "string" }, "inline_in_page_id": database_id, "parent_id": database_id,
                    "properties": { "type": "array", "items": property_spec_schema() },
                    "rows": { "type": "array", "items": row_schema(false) } }),
            &["title"],
        ),
        tool(
            "add_property",
            "Add a column to a table.",
            json!({ "database_id": database_id, "name": { "type": "string" }, "type": { "type": "string" }, "options": options_schema() }),
            &["database_id", "name"],
        ),
        tool(
            "update_property",
            "Rename a column, change its type, or replace its options.",
            json!({ "property_id": { "type": "integer" }, "name": { "type": "string" }, "type": { "type": "string" }, "options": options_schema() }),
            &["property_id"],
        ),
        tool(
            "delete_property",
            "Delete a column and its values.",
            json!({ "property_id": { "type": "integer" } }),
            &["property_id"],
        ),
        tool(
            "add_rows",
            "Add rows to a table in bulk.",
            json!({ "database_id": database_id, "rows": { "type": "array", "items": row_schema(false) } }),
            &["database_id", "rows"],
        ),
        tool(
            "update_rows",
            "Edit rows in bulk by record_id: title, priority, values, markdown body, or trashed=true.",
            json!({ "rows": { "type": "array", "items": row_schema(true) } }),
            &["rows"],
        ),
        tool(
            "set_database_view",
            "Switch a database's view to table, gallery or board.",
            json!({ "database_id": database_id, "view": { "type": "string", "enum": ["table", "gallery", "board"] } }),
            &["database_id", "view"],
        ),
    ]
}
