BEGIN IMMEDIATE;
CREATE TABLE pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER REFERENCES pages(id) ON DELETE SET NULL,
    title TEXT NOT NULL DEFAULT '',
    icon TEXT NOT NULL DEFAULT '',
    favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)),
    trashed_by INTEGER,
    cover INTEGER NOT NULL DEFAULT 0,
    cover_image TEXT,
    cover_x REAL NOT NULL DEFAULT 0.5 CHECK(cover_x BETWEEN 0.0 AND 1.0),
    cover_y REAL NOT NULL DEFAULT 0.5 CHECK(cover_y BETWEEN 0.0 AND 1.0),
    sort_order INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN 0 AND 3),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    block_count INTEGER NOT NULL DEFAULT 0,
    full_width INTEGER NOT NULL DEFAULT 0 CHECK(full_width IN (0,1))
);
CREATE INDEX pages_parent ON pages(parent_id);
CREATE INDEX pages_trash ON pages(trashed_by);
CREATE TABLE blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL DEFAULT 'text',
    content TEXT NOT NULL DEFAULT '',
    char_count INTEGER NOT NULL DEFAULT 0,
    line_count INTEGER NOT NULL DEFAULT 1,
    checked INTEGER NOT NULL DEFAULT 0 CHECK(checked IN (0,1)),
    column_group INTEGER,
    layout_column INTEGER NOT NULL DEFAULT 0,
    column_width REAL NOT NULL DEFAULT 1.0 CHECK(column_width > 0 AND column_width <= 1.0),
    media_width REAL NOT NULL DEFAULT 1.0 CHECK(media_width >= 0.25 AND media_width <= 1.0),
    text_styles TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX blocks_page_position ON blocks(page_id, position);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE databases (
    page_id INTEGER PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE,
    view_mode TEXT NOT NULL DEFAULT 'table' CHECK(view_mode IN ('table','gallery','board')),
    sort_key TEXT NOT NULL DEFAULT 'created_at',
    sort_descending INTEGER NOT NULL DEFAULT 1 CHECK(sort_descending IN (0,1)),
    gallery_columns INTEGER NOT NULL DEFAULT 3 CHECK(gallery_columns BETWEEN 1 AND 6),
    view_settings TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE database_properties (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    database_page_id INTEGER NOT NULL REFERENCES databases(page_id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ({{property_types}})),
    options TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL,
    UNIQUE(database_page_id, name)
);
CREATE INDEX database_properties_order ON database_properties(database_page_id, position);
CREATE TABLE page_property_values (
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    property_id INTEGER NOT NULL REFERENCES database_properties(id) ON DELETE CASCADE,
    value TEXT NOT NULL DEFAULT '',
    PRIMARY KEY(page_id, property_id)
);
CREATE INDEX page_property_values_property ON page_property_values(property_id, value);
CREATE TABLE custom_page_properties (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ({{property_types}})),
    options TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL,
    UNIQUE(page_id, name)
);
CREATE INDEX custom_page_properties_order ON custom_page_properties(page_id, position);
CREATE TABLE custom_page_property_values (
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    property_id INTEGER NOT NULL REFERENCES custom_page_properties(id) ON DELETE CASCADE,
    value TEXT NOT NULL DEFAULT '',
    PRIMARY KEY(page_id, property_id)
);
CREATE VIRTUAL TABLE page_search USING fts5(title, content=pages, content_rowid=id, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE block_search USING fts5(content, content=blocks, content_rowid=id, tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN
    INSERT INTO page_search(rowid,title) VALUES(new.id,new.title);
END;
CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN
    INSERT INTO page_search(page_search,rowid,title) VALUES('delete',old.id,old.title);
END;
CREATE TRIGGER pages_au AFTER UPDATE OF title ON pages WHEN old.title != new.title BEGIN
    INSERT INTO page_search(page_search,rowid,title) VALUES('delete',old.id,old.title);
    INSERT INTO page_search(rowid,title) VALUES(new.id,new.title);
END;
CREATE TRIGGER blocks_ai AFTER INSERT ON blocks BEGIN
    UPDATE blocks SET char_count=length(new.content),line_count=1+length(new.content)-length(replace(new.content,char(10),'')) WHERE id=new.id;
    INSERT INTO block_search(rowid,content) VALUES(new.id,new.content);
    UPDATE pages SET block_count=block_count+1 WHERE id=new.page_id;
END;
CREATE TRIGGER blocks_ad AFTER DELETE ON blocks BEGIN
    INSERT INTO block_search(block_search,rowid,content) VALUES('delete',old.id,old.content);
    UPDATE pages SET block_count=block_count-1 WHERE id=old.page_id;
END;
CREATE TRIGGER blocks_au AFTER UPDATE OF content ON blocks WHEN old.content != new.content BEGIN
    UPDATE blocks SET char_count=length(new.content),line_count=1+length(new.content)-length(replace(new.content,char(10),'')) WHERE id=new.id;
    INSERT INTO block_search(block_search,rowid,content) VALUES('delete',old.id,old.content);
    INSERT INTO block_search(rowid,content) VALUES(new.id,new.content);
END;
PRAGMA user_version={{schema_version}};
COMMIT;
