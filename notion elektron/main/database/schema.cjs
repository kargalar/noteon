const PROPERTY_TYPE_SQL =
  "'text','number','select','multi-select','status','date','person','files','checkbox','url','phone','email','relation','rollup','formula','button','id','place','created_time','last_edited_time','created_by','last_edited_by'";

const REQUIRED_WORKSPACE_TABLES = [
  'pages',
  'blocks',
  'databases',
  'database_properties',
  'page_property_values',
  'custom_page_properties',
  'custom_page_property_values',
  'settings',
];

const FTS_SEARCH_SCHEMA_SQL = `
  CREATE VIRTUAL TABLE IF NOT EXISTS page_search USING fts5(title, content=pages, content_rowid=id, tokenize='unicode61 remove_diacritics 2');
  CREATE VIRTUAL TABLE IF NOT EXISTS block_search USING fts5(content, content=blocks, content_rowid=id, tokenize='unicode61 remove_diacritics 2');
  CREATE TRIGGER IF NOT EXISTS pages_ai AFTER INSERT ON pages BEGIN
    INSERT INTO page_search(rowid,title) VALUES(new.id,new.title);
  END;
  CREATE TRIGGER IF NOT EXISTS pages_ad AFTER DELETE ON pages BEGIN
    INSERT INTO page_search(page_search,rowid,title) VALUES('delete',old.id,old.title);
  END;
  CREATE TRIGGER IF NOT EXISTS pages_au AFTER UPDATE OF title ON pages WHEN old.title != new.title BEGIN
    INSERT INTO page_search(page_search,rowid,title) VALUES('delete',old.id,old.title);
    INSERT INTO page_search(rowid,title) VALUES(new.id,new.title);
  END;
  CREATE TRIGGER IF NOT EXISTS blocks_ai AFTER INSERT ON blocks BEGIN
    UPDATE blocks SET char_count=length(new.content),line_count=1+length(new.content)-length(replace(new.content,char(10),'')) WHERE id=new.id;
    INSERT INTO block_search(rowid,content) VALUES(new.id,new.content);
    UPDATE pages SET block_count=block_count+1 WHERE id=new.page_id;
  END;
  CREATE TRIGGER IF NOT EXISTS blocks_ad AFTER DELETE ON blocks BEGIN
    INSERT INTO block_search(block_search,rowid,content) VALUES('delete',old.id,old.content);
    UPDATE pages SET block_count=block_count-1 WHERE id=old.page_id;
  END;
  CREATE TRIGGER IF NOT EXISTS blocks_au AFTER UPDATE OF content ON blocks WHEN old.content != new.content BEGIN
    UPDATE blocks SET char_count=length(new.content),line_count=1+length(new.content)-length(replace(new.content,char(10),'')) WHERE id=new.id;
    INSERT INTO block_search(block_search,rowid,content) VALUES('delete',old.id,old.content);
    INSERT INTO block_search(rowid,content) VALUES(new.id,new.content);
  END;
`;

/**
 * Applies SQLite pragmas required for safe concurrent Noteon workspace access.
 * @param {import('better-sqlite3').Database} database
 */
function configureWorkspaceDatabase(database) {
  database.pragma('busy_timeout = 5000');
  database.pragma('journal_mode = WAL');
  database.pragma('synchronous = FULL');
  database.pragma('foreign_keys = ON');
  database.pragma('cache_size = -8192');
}

/**
 * Creates a fresh v15 schema on an empty database or migrates legacy schemas up to v15.
 * @param {import('better-sqlite3').Database} database
 */
function ensureWorkspaceSchema(database) {
  const version = Number(database.pragma('user_version', { simple: true }) || 0);
  if (version > 15) {
    throw new Error('This database was created with a newer version of Noteon.');
  }

  const hasTable = (name) =>
    Boolean(database.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?").get(name));
  const hasColumn = (table, column) =>
    database.prepare(`PRAGMA table_info(${table})`).all().some((col) => col.name === column);

  if (version === 15 && hasTable('page_search') && hasTable('block_search')) {
    return;
  }

  if (version === 0 && !hasTable('pages') && !hasTable('blocks')) {
    database.exec(`
      CREATE TABLE IF NOT EXISTS pages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        parent_id INTEGER REFERENCES pages(id) ON DELETE SET NULL,
        title TEXT NOT NULL DEFAULT '',
        icon TEXT NOT NULL DEFAULT '▤',
        favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)),
        trashed_by INTEGER,
        cover INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
        block_count INTEGER NOT NULL DEFAULT 0,
        priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN 0 AND 3),
        created_at INTEGER NOT NULL DEFAULT 0,
        cover_image TEXT,
        cover_x REAL NOT NULL DEFAULT 0.5 CHECK(cover_x BETWEEN 0.0 AND 1.0),
        cover_y REAL NOT NULL DEFAULT 0.5 CHECK(cover_y BETWEEN 0.0 AND 1.0),
        sort_order INTEGER NOT NULL DEFAULT 0,
        full_width INTEGER NOT NULL DEFAULT 0 CHECK(full_width IN (0,1))
      );
      CREATE INDEX IF NOT EXISTS pages_parent ON pages(parent_id);
      CREATE INDEX IF NOT EXISTS pages_trash ON pages(trashed_by);
      CREATE TABLE IF NOT EXISTS blocks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        kind TEXT NOT NULL DEFAULT 'text',
        content TEXT NOT NULL DEFAULT '',
        char_count INTEGER NOT NULL DEFAULT 0,
        line_count INTEGER NOT NULL DEFAULT 1,
        checked INTEGER NOT NULL DEFAULT 0 CHECK(checked IN (0,1)),
        column_group INTEGER,
        column_index INTEGER NOT NULL DEFAULT 0 CHECK(column_index IN (0,1)),
        layout_column INTEGER NOT NULL DEFAULT 0,
        column_width REAL NOT NULL DEFAULT 1.0 CHECK(column_width > 0 AND column_width <= 1.0),
        media_width REAL NOT NULL DEFAULT 1.0 CHECK(media_width >= 0.25 AND media_width <= 1.0),
        text_styles TEXT NOT NULL DEFAULT '[]'
      );
      CREATE INDEX IF NOT EXISTS blocks_page_position ON blocks(page_id, position);
      CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS databases (
        page_id INTEGER PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE,
        view_mode TEXT NOT NULL DEFAULT 'table' CHECK(view_mode IN ('table','gallery','board')),
        sort_key TEXT NOT NULL DEFAULT 'created_at',
        sort_descending INTEGER NOT NULL DEFAULT 1 CHECK(sort_descending IN (0,1)),
        gallery_columns INTEGER NOT NULL DEFAULT 3 CHECK(gallery_columns BETWEEN 1 AND 6),
        view_settings TEXT NOT NULL DEFAULT '{}'
      );
      CREATE TABLE IF NOT EXISTS database_properties (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        database_page_id INTEGER NOT NULL REFERENCES databases(page_id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL CHECK(kind IN (${PROPERTY_TYPE_SQL})),
        options TEXT NOT NULL DEFAULT '',
        position INTEGER NOT NULL,
        UNIQUE(database_page_id, name)
      );
      CREATE INDEX IF NOT EXISTS database_properties_order ON database_properties(database_page_id, position);
      CREATE TABLE IF NOT EXISTS page_property_values (
        page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
        property_id INTEGER NOT NULL REFERENCES database_properties(id) ON DELETE CASCADE,
        value TEXT NOT NULL DEFAULT '',
        PRIMARY KEY(page_id, property_id)
      );
      CREATE INDEX IF NOT EXISTS page_property_values_property ON page_property_values(property_id, value);
      CREATE TABLE IF NOT EXISTS custom_page_properties (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL CHECK(kind IN (${PROPERTY_TYPE_SQL})),
        options TEXT NOT NULL DEFAULT '',
        position INTEGER NOT NULL,
        UNIQUE(page_id, name)
      );
      CREATE INDEX IF NOT EXISTS custom_page_properties_order ON custom_page_properties(page_id, position);
      CREATE TABLE IF NOT EXISTS custom_page_property_values (
        page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
        property_id INTEGER NOT NULL REFERENCES custom_page_properties(id) ON DELETE CASCADE,
        value TEXT NOT NULL DEFAULT '',
        PRIMARY KEY(page_id, property_id)
      );
      ${FTS_SEARCH_SCHEMA_SQL}
      PRAGMA user_version = 15;
    `);
    return;
  }

  if (version <= 1) {
    database.exec('CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);');
    if (hasTable('pages')) {
      if (!hasColumn('pages', 'favorite')) database.exec('ALTER TABLE pages ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1));');
      if (!hasColumn('pages', 'trashed_by')) database.exec('ALTER TABLE pages ADD COLUMN trashed_by INTEGER;');
      if (!hasColumn('pages', 'cover')) database.exec('ALTER TABLE pages ADD COLUMN cover INTEGER NOT NULL DEFAULT 0;');
      if (!hasColumn('pages', 'updated_at')) database.exec('ALTER TABLE pages ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;');
      if (!hasColumn('pages', 'block_count')) database.exec('ALTER TABLE pages ADD COLUMN block_count INTEGER NOT NULL DEFAULT 0;');
      database.exec('CREATE INDEX IF NOT EXISTS pages_parent ON pages(parent_id); CREATE INDEX IF NOT EXISTS pages_trash ON pages(trashed_by);');
    }
    if (hasTable('blocks')) {
      if (!hasColumn('blocks', 'char_count')) database.exec('ALTER TABLE blocks ADD COLUMN char_count INTEGER NOT NULL DEFAULT 0;');
      if (!hasColumn('blocks', 'line_count')) database.exec('ALTER TABLE blocks ADD COLUMN line_count INTEGER NOT NULL DEFAULT 1;');
      if (!hasColumn('blocks', 'checked')) database.exec('ALTER TABLE blocks ADD COLUMN checked INTEGER NOT NULL DEFAULT 0 CHECK(checked IN (0,1));');
      if (!hasColumn('blocks', 'column_group')) database.exec('ALTER TABLE blocks ADD COLUMN column_group INTEGER;');
      if (!hasColumn('blocks', 'column_index')) database.exec('ALTER TABLE blocks ADD COLUMN column_index INTEGER NOT NULL DEFAULT 0 CHECK(column_index IN (0,1));');
      database.exec('CREATE INDEX IF NOT EXISTS blocks_page_position ON blocks(page_id, position);');
    }
    database.pragma('user_version = 2');
  }

  if (version <= 2) {
    if (hasTable('pages')) {
      if (!hasColumn('pages', 'priority')) database.exec('ALTER TABLE pages ADD COLUMN priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN 0 AND 3);');
      if (!hasColumn('pages', 'created_at')) {
        database.exec('ALTER TABLE pages ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0; UPDATE pages SET created_at=updated_at;');
      }
    }
    database.exec(`
      CREATE TABLE IF NOT EXISTS databases (page_id INTEGER PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE, view_mode TEXT NOT NULL DEFAULT 'table' CHECK(view_mode IN ('table','gallery','board')), sort_key TEXT NOT NULL DEFAULT 'created_at', sort_descending INTEGER NOT NULL DEFAULT 1 CHECK(sort_descending IN (0,1)));
      CREATE TABLE IF NOT EXISTS database_properties (id INTEGER PRIMARY KEY AUTOINCREMENT, database_page_id INTEGER NOT NULL REFERENCES databases(page_id) ON DELETE CASCADE, name TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN (${PROPERTY_TYPE_SQL})), options TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL, UNIQUE(database_page_id, name));
      CREATE INDEX IF NOT EXISTS database_properties_order ON database_properties(database_page_id, position);
      CREATE TABLE IF NOT EXISTS page_property_values (page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE, property_id INTEGER NOT NULL REFERENCES database_properties(id) ON DELETE CASCADE, value TEXT NOT NULL DEFAULT '', PRIMARY KEY(page_id, property_id));
      CREATE INDEX IF NOT EXISTS page_property_values_property ON page_property_values(property_id, value);
      PRAGMA user_version = 3;
    `);
  }

  if (version <= 3 && hasTable('blocks')) {
    if (!hasColumn('blocks', 'layout_column')) database.exec('ALTER TABLE blocks ADD COLUMN layout_column INTEGER NOT NULL DEFAULT 0; UPDATE blocks SET layout_column=column_index;');
    if (!hasColumn('blocks', 'column_width')) database.exec('ALTER TABLE blocks ADD COLUMN column_width REAL NOT NULL DEFAULT 1.0 CHECK(column_width > 0 AND column_width <= 1.0);');
    database.pragma('user_version = 4');
  }

  if (version <= 4) {
    database.exec(`
      CREATE TABLE IF NOT EXISTS custom_page_properties (id INTEGER PRIMARY KEY AUTOINCREMENT, page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE, name TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN (${PROPERTY_TYPE_SQL})), options TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL, UNIQUE(page_id, name));
      CREATE INDEX IF NOT EXISTS custom_page_properties_order ON custom_page_properties(page_id, position);
      CREATE TABLE IF NOT EXISTS custom_page_property_values (page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE, property_id INTEGER NOT NULL REFERENCES custom_page_properties(id) ON DELETE CASCADE, value TEXT NOT NULL DEFAULT '', PRIMARY KEY(page_id, property_id));
      PRAGMA user_version = 5;
    `);
  }

  if (version <= 5 && hasTable('databases')) {
    if (!hasColumn('databases', 'gallery_columns')) database.exec('ALTER TABLE databases ADD COLUMN gallery_columns INTEGER NOT NULL DEFAULT 3 CHECK(gallery_columns BETWEEN 1 AND 6);');
    database.exec("UPDATE databases SET view_mode='table' WHERE view_mode='blog'; PRAGMA user_version = 6;");
  }

  if (version <= 6 && hasTable('blocks')) {
    if (!hasColumn('blocks', 'media_width')) database.exec('ALTER TABLE blocks ADD COLUMN media_width REAL NOT NULL DEFAULT 1.0 CHECK(media_width >= 0.25 AND media_width <= 1.0);');
    database.pragma('user_version = 7');
  }

  if (version <= 7 && hasTable('pages')) {
    if (!hasColumn('pages', 'cover_image')) database.exec('ALTER TABLE pages ADD COLUMN cover_image TEXT;');
    if (!hasColumn('pages', 'cover_x')) database.exec('ALTER TABLE pages ADD COLUMN cover_x REAL NOT NULL DEFAULT 0.5 CHECK(cover_x BETWEEN 0.0 AND 1.0);');
    if (!hasColumn('pages', 'cover_y')) database.exec('ALTER TABLE pages ADD COLUMN cover_y REAL NOT NULL DEFAULT 0.5 CHECK(cover_y BETWEEN 0.0 AND 1.0);');
    database.pragma('user_version = 8');
  }

  if (version <= 8 && hasTable('pages')) {
    if (!hasColumn('pages', 'sort_order')) database.exec('ALTER TABLE pages ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0; UPDATE pages SET sort_order=id;');
    database.pragma('user_version = 9');
  }

  if (version <= 9 && hasTable('databases')) {
    if (!hasColumn('databases', 'view_settings')) database.exec("ALTER TABLE databases ADD COLUMN view_settings TEXT NOT NULL DEFAULT '{}';");
    database.pragma('user_version = 10');
  }

  if (version <= 10 && hasTable('blocks')) {
    if (!hasColumn('blocks', 'text_styles')) database.exec("ALTER TABLE blocks ADD COLUMN text_styles TEXT NOT NULL DEFAULT '[]';");
    database.pragma('user_version = 11');
  }

  if (version <= 11 && hasTable('pages')) {
    if (!hasColumn('pages', 'full_width')) database.exec('ALTER TABLE pages ADD COLUMN full_width INTEGER NOT NULL DEFAULT 0 CHECK(full_width IN (0,1));');
    database.pragma('user_version = 12');
  }

  if (version <= 12 && hasTable('pages') && hasColumn('pages', 'created_at')) {
    database.exec("UPDATE pages SET created_at=CASE WHEN updated_at > 0 THEN updated_at ELSE unixepoch() END WHERE created_at <= 0; PRAGMA user_version = 13;");
  }

  if (version <= 13 && hasTable('blocks') && hasTable('pages')) {
    database.exec(`
      BEGIN IMMEDIATE;
      WITH ranked_blocks AS (
        SELECT id, ROW_NUMBER() OVER (PARTITION BY page_id ORDER BY position, id) - 1 AS position FROM blocks
      )
      UPDATE blocks SET position=(SELECT position FROM ranked_blocks WHERE ranked_blocks.id=blocks.id);
      UPDATE pages SET block_count=(SELECT COUNT(*) FROM blocks WHERE blocks.page_id=pages.id);
      PRAGMA user_version = 14;
      COMMIT;
    `);
  }

  if (version <= 14) {
    const foreignKeys = Boolean(database.pragma('foreign_keys', { simple: true }));
    database.pragma('foreign_keys = OFF');
    try {
      database.transaction(() => {
        for (const [table, ownerColumn, ownerTable, ownerKey, index] of [
          ['database_properties', 'database_page_id', 'databases', 'page_id', 'database_properties_order'],
          ['custom_page_properties', 'page_id', 'pages', 'id', 'custom_page_properties_order'],
        ]) {
          if (!hasTable(table)) continue;
          const seq = database.prepare('SELECT seq FROM sqlite_sequence WHERE name=?').get(table)?.seq ?? 0;
          database.exec(`
            CREATE TABLE ${table}_new (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              ${ownerColumn} INTEGER NOT NULL REFERENCES ${ownerTable}(${ownerKey}) ON DELETE CASCADE,
              name TEXT NOT NULL,
              kind TEXT NOT NULL CHECK(kind IN (${PROPERTY_TYPE_SQL})),
              options TEXT NOT NULL DEFAULT '',
              position INTEGER NOT NULL,
              UNIQUE(${ownerColumn}, name)
            );
            INSERT INTO ${table}_new(id,${ownerColumn},name,kind,options,position) SELECT id,${ownerColumn},name,kind,options,position FROM ${table};
            DROP TABLE ${table};
            ALTER TABLE ${table}_new RENAME TO ${table};
            CREATE INDEX ${index} ON ${table}(${ownerColumn}, position);
          `);
          database.prepare('UPDATE sqlite_sequence SET seq=MAX(seq,?) WHERE name=?').run(seq, table);
        }
        if (database.pragma('foreign_key_check').length) {
          throw new Error('Property migration found invalid references.');
        }
        database.pragma('user_version = 15');
      })();
    } finally {
      database.pragma(`foreign_keys = ${foreignKeys ? 'ON' : 'OFF'}`);
    }
  }

  if (!hasTable('page_search') || !hasTable('block_search')) {
    database.exec(`
      ${FTS_SEARCH_SCHEMA_SQL}
      INSERT INTO page_search(page_search) VALUES('rebuild');
      INSERT INTO block_search(block_search) VALUES('rebuild');
    `);
  }
}

/**
 * Validates SQLite integrity, required Noteon schema tables, and foreign keys.
 * @param {import('better-sqlite3').Database} database
 * @param {string} [label='Workspace']
 */
function validateWorkspaceDatabase(database, label = 'Workspace') {
  const integrity = database.pragma('integrity_check', { simple: true });
  if (integrity !== 'ok') {
    throw new Error(`${label} integrity check failed: ${integrity}`);
  }
  const tables = new Set(
    database
      .prepare("SELECT name FROM sqlite_master WHERE type='table'")
      .all()
      .map((row) => row.name),
  );
  for (const name of REQUIRED_WORKSPACE_TABLES) {
    if (!tables.has(name)) {
      throw new Error(`${label} is missing required table: ${name}`);
    }
  }
  if (database.pragma('foreign_key_check').length) {
    throw new Error(`${label} contains broken page or property references.`);
  }
}

module.exports = {
  PROPERTY_TYPE_SQL,
  configureWorkspaceDatabase,
  ensureWorkspaceSchema,
  validateWorkspaceDatabase,
};
