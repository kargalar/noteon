/**
 * Database schema, property, record, sorting, and view-settings IPC handlers.
 */

const { state, rows, one } = require('../state.cjs');
const {
  utf8ByteLength,
  propertyKinds,
  readPropertyOptions,
  validatePropertyValue,
  serializePropertyOptions,
  normalizePropertyOptions,
} = require('../utils/property-utils.cjs');
const {
  assertPageWritable,
  assertDatabaseWritable,
  assertPropertyWritable,
  assertPageTreeAvailable,
} = require('../services/leases.cjs');
const { pushWorkspaceUndo, recordPageUndo } = require('../services/history.cjs');
const { notifyWorkspaceTreeChanged, notifyWorkspacePagesTrashed } = require('../services/notifications.cjs');
const { createDatabasePage } = require('./pages.cjs');

const ALLOWED_VIEW_SETTINGS = new Set([
  'gallery_show_cover',
  'gallery_show_title',
  'gallery_show_description',
  'gallery_show_created_at',
  'gallery_visible_properties',
  'table_show_created_at',
  'table_visible_properties',
  'table_column_widths',
  'table_name_width',
  'table_created_width',
  'table_height',
  'board_show_description',
  'board_visible_properties',
  'new_record_open_mode',
  'table_column_calculations',
]);

const ALLOWED_CALCULATIONS = new Set(['sum', 'average', 'min', 'max', 'median', 'count', 'none']);

/**
 * Safely parse a database `view_settings` JSON string.
 * @param {string|null|undefined} raw
 * @returns {Record<string, unknown>}
 */
function parseViewSettings(raw) {
  try {
    return JSON.parse(raw || '{}');
  } catch {
    return {};
  }
}

/**
 * Verify a database exists and is not in the trash.
 * @param {number} id
 */
function assertActiveDatabase(id) {
  const exists = one(
    'SELECT 1 FROM databases d JOIN pages p ON p.id=d.page_id WHERE d.page_id=? AND p.trashed_by IS NULL',
    id,
  );
  if (!exists) {
    throw new Error('Database not found.');
  }
}

/**
 * Insert a new empty record page and initial empty text block inside a database.
 * Caller must invoke inside a transaction.
 * @param {number} databaseId
 * @returns {number}
 */
function insertDatabaseRecord(databaseId) {
  const result = state.db
    .prepare(
      `INSERT INTO pages(parent_id,title,sort_order,created_at,updated_at)
       SELECT ?,'',coalesce(max(sort_order)+1,0),unixepoch(),unixepoch() FROM pages WHERE parent_id IS ?`,
    )
    .run(databaseId, databaseId);
  const recordId = Number(result.lastInsertRowid);
  state.db.prepare("INSERT INTO blocks(page_id,position,kind,content,checked) VALUES(?,0,'text','',0)").run(recordId);
  return recordId;
}

/**
 * Validate and normalize a single `database:view-settings` field.
 * @param {string} key
 * @param {unknown} value
 * @param {Map<number, { id: number, kind: string }>} propsById
 * @returns {unknown}
 */
function sanitizeViewSetting(key, value, propsById) {
  if (key.endsWith('_visible_properties')) {
    if (value !== null && (!Array.isArray(value) || value.some((item) => !Number.isSafeInteger(Number(item))))) {
      throw new Error('Invalid visible property list.');
    }
    return value === null ? null : [...new Set(value.map(Number).filter((pid) => propsById.has(pid)))];
  }

  if (key.startsWith('gallery_show_') || key === 'table_show_created_at' || key === 'board_show_description') {
    return Boolean(value);
  }

  if (key === 'new_record_open_mode') {
    if (!['focus_name', 'dialog', 'full_page'].includes(value)) {
      throw new Error('Invalid new-record opening mode.');
    }
    return value;
  }

  if (key === 'table_column_calculations' || key === 'table_column_widths') {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      throw new Error(`Invalid ${key}.`);
    }
    const out = {};
    for (const [rawId, rawValue] of Object.entries(value)) {
      const propertyId = Number(rawId);
      if (!Number.isSafeInteger(propertyId)) {
        throw new Error('Column setting references an invalid property.');
      }
      const property = propsById.get(propertyId);
      if (!property) continue;

      if (key === 'table_column_calculations') {
        if (property.kind !== 'number' || !ALLOWED_CALCULATIONS.has(rawValue)) {
          throw new Error('Invalid number calculation.');
        }
        if (rawValue !== 'none') out[String(propertyId)] = rawValue;
      } else {
        if (!Number.isFinite(Number(rawValue)) || Number(rawValue) < 64 || Number(rawValue) > 1200) {
          throw new Error('Column width must be between 64 and 1200 pixels.');
        }
        out[String(propertyId)] = Number(rawValue);
      }
    }
    return out;
  }

  if (['table_name_width', 'table_created_width', 'table_height'].includes(key)) {
    if (value !== null && (!Number.isFinite(Number(value)) || Number(value) < 64 || Number(value) > 3000)) {
      throw new Error(`Invalid ${key}.`);
    }
    return value === null ? null : Number(value);
  }

  return value;
}

/**
 * Register all database IPC handlers.
 * @param {import('electron').IpcMain} ipcMain
 */
function registerDatabaseIpc(ipcMain) {
  ipcMain.handle('database:block:create', (event, { pageId, position = 0, replaceBlock = null, title = 'New database' }) => {
    pageId = Number(pageId);
    position = Number(position);
    assertPageWritable(event.sender, pageId);

    const parent = one('SELECT id FROM pages WHERE id=? AND trashed_by IS NULL', pageId);
    if (!parent) throw new Error('The parent page is unavailable.');

    let replacement = null;
    if (replaceBlock !== null) {
      replacement = one('SELECT id,position FROM blocks WHERE id=? AND page_id=?', Number(replaceBlock), pageId);
      if (!replacement) throw new Error('Block to replace was not found.');
    }

    recordPageUndo(pageId, 'Veritabanı eklendi', true);
    const id = state.db.transaction(() => {
      const databaseId = createDatabasePage(state.db, pageId, title);
      if (replacement) {
        state.db.prepare("UPDATE blocks SET kind='database',content=? WHERE id=?").run(String(databaseId), replacement.id);
      } else {
        const count = Number(one('SELECT count(*) n FROM blocks WHERE page_id=?', pageId).n);
        const at = Math.max(0, Math.min(count, Number.isFinite(position) ? Math.trunc(position) : count));
        state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>=?').run(pageId, at);
        state.db.prepare("INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,'database',?)").run(pageId, at, String(databaseId));
      }
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return databaseId;
    })();

    notifyWorkspaceTreeChanged(event.sender);
    return id;
  });

  ipcMain.handle('database:get', (_event, id) => {
    const database = one('SELECT * FROM databases WHERE page_id=?', id);
    if (!database) return null;

    database.properties = rows('SELECT * FROM database_properties WHERE database_page_id=? ORDER BY position', id);
    database.records = rows(
      "SELECT p.*,(SELECT b.content FROM blocks b WHERE b.page_id=p.id AND b.kind='text' AND trim(b.content)!='' ORDER BY b.position LIMIT 1) preview FROM pages p WHERE p.parent_id=? AND p.trashed_by IS NULL ORDER BY p.sort_order,p.id",
      id,
    );

    const byPage = new Map();
    const values = rows(
      'SELECT v.page_id,v.property_id,v.value FROM page_property_values v JOIN pages p ON p.id=v.page_id WHERE p.parent_id=? AND p.trashed_by IS NULL',
      id,
    );
    for (const row of values) {
      let map = byPage.get(Number(row.page_id));
      if (!map) {
        map = {};
        byPage.set(Number(row.page_id), map);
      }
      map[row.property_id] = row.value;
    }
    for (const record of database.records) {
      record.values = byPage.get(Number(record.id)) || {};
    }
    return database;
  });

  ipcMain.handle('database:property:add', (event, { id, name, kind = 'text', options = '' }) => {
    assertDatabaseWritable(event.sender, id);
    const cleanName = String(name || '').trim();
    if (!cleanName || utf8ByteLength(cleanName) > 80 || !propertyKinds.has(kind)) {
      throw new Error('Invalid property name or type.');
    }

    const normalizedOptions = normalizePropertyOptions(options, kind, true);
    const position = one('SELECT coalesce(max(position)+1,0) n FROM database_properties WHERE database_page_id=?', id).n;
    return state.db
      .prepare('INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?,?,?,?,?)')
      .run(id, cleanName, kind, normalizedOptions, position).lastInsertRowid;
  });

  ipcMain.handle('database:property:update', (event, { id, name, kind, options }) =>
    state.db.transaction(() => {
      assertPropertyWritable(event.sender, id);
      const existing = one('SELECT * FROM database_properties WHERE id=?', id);
      if (!existing) throw new Error('Database property not found.');

      const nextName = name === undefined ? existing.name : String(name).trim();
      const nextKind = kind === undefined ? existing.kind : String(kind);
      const kindChanged = nextKind !== existing.kind;
      if (!nextName || utf8ByteLength(nextName) > 80 || !propertyKinds.has(nextKind)) {
        throw new Error('Invalid property name or type.');
      }

      let nextOptions =
        options === undefined
          ? existing.options
          : normalizePropertyOptions(options, nextKind, kindChanged && nextKind === 'status');
      if (options === undefined && kindChanged && nextKind === 'status' && !String(nextOptions || '').trim()) {
        nextOptions = serializePropertyOptions(['To do', 'In progress', 'Done'].map((n) => ({ name: n, color: null })));
      }

      state.db.prepare('UPDATE database_properties SET name=?,kind=?,options=? WHERE id=?').run(nextName, nextKind, nextOptions, id);

      if (kindChanged || options !== undefined) {
        const vals = rows('SELECT page_id,value FROM page_property_values WHERE property_id=?', id);
        const delStmt = state.db.prepare('DELETE FROM page_property_values WHERE page_id=? AND property_id=?');
        const updStmt = state.db.prepare('UPDATE page_property_values SET value=? WHERE page_id=? AND property_id=?');
        const allowedOpts = new Set(readPropertyOptions(nextOptions).map((o) => o.name));

        for (const row of vals) {
          if (nextKind === 'multi-select' && ['multi-select', 'select', 'status'].includes(existing.kind)) {
            const filtered = row.value.split('|').filter((v) => v && allowedOpts.has(v)).join('|');
            if (filtered) updStmt.run(filtered, row.page_id, id);
            else delStmt.run(row.page_id, id);
          } else {
            try {
              validatePropertyValue(nextKind, nextOptions, row.value);
            } catch {
              delStmt.run(row.page_id, id);
            }
          }
        }
      }

      if (kindChanged && nextKind !== 'number') {
        const dbRow = one('SELECT view_settings FROM databases WHERE page_id=?', existing.database_page_id);
        if (dbRow) {
          const settings = parseViewSettings(dbRow.view_settings);
          if (settings.table_column_calculations && String(id) in settings.table_column_calculations) {
            delete settings.table_column_calculations[String(id)];
            state.db.prepare('UPDATE databases SET view_settings=? WHERE page_id=?').run(JSON.stringify(settings), existing.database_page_id);
          }
        }
      }

      return 1;
    })(),
  );

  ipcMain.handle('database:property:option:rename', (event, { id, oldName, newName }) =>
    state.db.transaction(() => {
      assertPropertyWritable(event.sender, id);
      const property = one('SELECT kind,options FROM database_properties WHERE id=?', id);
      if (!property) throw new Error('Database property not found.');

      oldName = String(oldName || '').trim();
      newName = String(newName || '').trim();
      if (!newName || utf8ByteLength(newName) > 80 || /[\r\n]/.test(newName)) {
        throw new Error('Option names must be 1–80 characters long.');
      }

      const options = readPropertyOptions(property.options);
      const found = options.find((item) => item.name === oldName);
      if (found) found.name = newName;
      else options.push({ name: newName, color: null });

      const deduped = options.filter((item, index, list) => list.findIndex((other) => other.name === item.name) === index);
      state.db.prepare('UPDATE database_properties SET options=? WHERE id=?').run(serializePropertyOptions(deduped), id);

      if (oldName !== newName) {
        const values = rows('SELECT page_id,value FROM page_property_values WHERE property_id=?', id);
        const updateStmt = state.db.prepare('UPDATE page_property_values SET value=? WHERE page_id=? AND property_id=?');
        for (const row of values) {
          if (property.kind === 'multi-select') {
            const parts = row.value.split('|').filter(Boolean);
            if (!parts.includes(oldName)) continue;
            const next = [];
            for (const part of parts) {
              const mapped = part === oldName ? newName : part;
              if (!next.includes(mapped)) next.push(mapped);
            }
            updateStmt.run(next.join('|'), row.page_id, id);
          } else if (row.value === oldName) {
            updateStmt.run(newName, row.page_id, id);
          }
        }
      }
      return true;
    })(),
  );

  ipcMain.handle('database:property:option:remove', (event, { id, name }) =>
    state.db.transaction(() => {
      assertPropertyWritable(event.sender, id);
      const property = one('SELECT kind,options FROM database_properties WHERE id=?', id);
      if (!property) throw new Error('Database property not found.');

      const targetName = String(name || '').trim();
      const options = readPropertyOptions(property.options).filter((item) => item.name !== targetName);
      state.db.prepare('UPDATE database_properties SET options=? WHERE id=?').run(serializePropertyOptions(options), id);

      const values = rows('SELECT page_id,value FROM page_property_values WHERE property_id=?', id);
      const updateStmt = state.db.prepare('UPDATE page_property_values SET value=? WHERE page_id=? AND property_id=?');
      for (const row of values) {
        if (property.kind === 'multi-select') {
          const next = row.value.split('|').filter((part) => part && part !== targetName).join('|');
          if (next !== row.value) updateStmt.run(next, row.page_id, id);
        } else if (row.value === targetName) {
          updateStmt.run('', row.page_id, id);
        }
      }
      return true;
    })(),
  );

  ipcMain.handle('database:property:move', (event, { id, direction }) =>
    state.db.transaction(() => {
      const property = one('SELECT id,database_page_id FROM database_properties WHERE id=?', id);
      if (!property) return false;
      assertDatabaseWritable(event.sender, property.database_page_id);

      const ids = rows('SELECT id FROM database_properties WHERE database_page_id=? ORDER BY position,id', property.database_page_id).map(
        (row) => row.id,
      );
      const from = ids.indexOf(Number(id));
      const to = Number(direction) < 0 ? from - 1 : from + 1;
      if (from < 0 || to < 0 || to >= ids.length) return true;

      [ids[from], ids[to]] = [ids[to], ids[from]];
      const updateStmt = state.db.prepare('UPDATE database_properties SET position=? WHERE id=?');
      ids.forEach((propertyId, index) => updateStmt.run(index, propertyId));
      return true;
    })(),
  );

  ipcMain.handle('database:property:delete', (event, id) =>
    state.db.transaction(() => {
      const property = one('SELECT id,database_page_id FROM database_properties WHERE id=?', id);
      if (!property) return 0;
      assertDatabaseWritable(event.sender, property.database_page_id);

      state.db.prepare('DELETE FROM page_property_values WHERE property_id=?').run(id);
      state.db.prepare('DELETE FROM database_properties WHERE id=?').run(id);
      state.db
        .prepare("UPDATE databases SET sort_key='created_at',sort_descending=1 WHERE page_id=? AND sort_key=?")
        .run(property.database_page_id, String(id));

      const dbRow = one('SELECT view_settings FROM databases WHERE page_id=?', property.database_page_id);
      if (dbRow) {
        const settings = parseViewSettings(dbRow.view_settings);
        const strId = String(Number(id));
        for (const key of ['gallery_visible_properties', 'table_visible_properties', 'board_visible_properties']) {
          if (Array.isArray(settings[key])) {
            settings[key] = settings[key].filter((item) => Number(item) !== Number(id));
          }
        }
        for (const key of ['table_column_widths', 'table_column_calculations']) {
          if (settings[key] && typeof settings[key] === 'object') {
            delete settings[key][strId];
          }
        }
        state.db.prepare('UPDATE databases SET view_settings=? WHERE page_id=?').run(JSON.stringify(settings), property.database_page_id);
      }
      return 1;
    })(),
  );

  ipcMain.handle('database:records:reorder', (event, { id, recordId, recordIds, targetId, after = false }) =>
    state.db.transaction(() => {
      assertDatabaseWritable(event.sender, id);
      const moving = [...new Set((Array.isArray(recordIds) ? recordIds : [recordId]).map(Number))];
      if (!moving.length || moving.some((value) => !Number.isSafeInteger(value))) {
        throw new Error('Invalid database record selection.');
      }

      const records = rows('SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL ORDER BY sort_order,id', id).map((row) =>
        Number(row.id),
      );
      const selected = new Set(moving);
      const target = Number(targetId);
      if (!records.includes(target) || moving.some((value) => !records.includes(value))) {
        throw new Error('Some selected records are no longer in this database.');
      }
      if (selected.has(target)) return true;

      const moved = records.filter((value) => selected.has(value));
      const remaining = records.filter((value) => !selected.has(value));
      const index = remaining.indexOf(target) + Number(Boolean(after));
      remaining.splice(index, 0, ...moved);

      const updateStmt = state.db.prepare('UPDATE pages SET sort_order=? WHERE id=?');
      remaining.forEach((record, pos) => updateStmt.run(pos, record));
      state.db.prepare("UPDATE databases SET sort_key='manual',sort_descending=0 WHERE page_id=?").run(id);
      return true;
    })(),
  );

  ipcMain.handle('database:records:create', (event, id) => {
    id = Number(id);
    assertDatabaseWritable(event.sender, id);
    assertActiveDatabase(id);

    const recordId = state.db.transaction(() => insertDatabaseRecord(id))();
    pushWorkspaceUndo({ type: 'create-record', pageIds: [recordId] });
    notifyWorkspaceTreeChanged(event.sender);
    return recordId;
  });

  ipcMain.handle('database:records:create-with-property', (event, { id, propertyId, value }) => {
    id = Number(id);
    propertyId = Number(propertyId);
    assertDatabaseWritable(event.sender, id);
    assertActiveDatabase(id);

    const property = one('SELECT kind,options FROM database_properties WHERE id=? AND database_page_id=?', propertyId, id);
    if (!property || !['status', 'select'].includes(property.kind)) {
      throw new Error('Board group property is no longer available.');
    }
    const normalizedValue = String(value ?? '');
    validatePropertyValue(property.kind, property.options, normalizedValue);

    const recordId = state.db.transaction(() => {
      const createdId = insertDatabaseRecord(id);
      if (normalizedValue) {
        state.db.prepare('INSERT INTO page_property_values(page_id,property_id,value) VALUES(?,?,?)').run(createdId, propertyId, normalizedValue);
      }
      return createdId;
    })();

    pushWorkspaceUndo({ type: 'create-record', pageIds: [recordId] });
    notifyWorkspaceTreeChanged(event.sender);
    return recordId;
  });

  ipcMain.handle('database:records:move', (event, { id, recordId, propertyId, value, targetId = null, after = false }) =>
    state.db.transaction(() => {
      id = Number(id);
      recordId = Number(recordId);
      propertyId = Number(propertyId);
      targetId = targetId === null || targetId === undefined ? null : Number(targetId);
      assertDatabaseWritable(event.sender, id);

      const record = one('SELECT parent_id FROM pages WHERE id=? AND trashed_by IS NULL', recordId);
      if (!record || Number(record.parent_id) !== id) {
        throw new Error('Record does not belong to this database.');
      }

      const property = one('SELECT kind,options FROM database_properties WHERE id=? AND database_page_id=?', propertyId, id);
      if (!property || !['status', 'select'].includes(property.kind)) {
        throw new Error('Board group property is no longer available.');
      }

      const normalizedValue = String(value ?? '');
      validatePropertyValue(property.kind, property.options, normalizedValue);

      if (targetId !== null && targetId !== recordId) {
        const target = one('SELECT id FROM pages WHERE id=? AND parent_id=? AND trashed_by IS NULL', targetId, id);
        if (!target) throw new Error('Drop target is no longer in this database.');
      }

      state.db
        .prepare('INSERT INTO page_property_values(page_id,property_id,value) VALUES(?,?,?) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value')
        .run(recordId, propertyId, normalizedValue);
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(recordId);

      if (targetId !== null && targetId !== recordId) {
        const ids = rows('SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL ORDER BY sort_order,id', id).map((item) => Number(item.id));
        const from = ids.indexOf(recordId);
        if (from < 0) throw new Error('Record no longer exists.');
        ids.splice(from, 1);
        const to = ids.indexOf(targetId);
        if (to < 0) throw new Error('Drop target is no longer in this database.');
        ids.splice(to + (after ? 1 : 0), 0, recordId);

        const updateStmt = state.db.prepare('UPDATE pages SET sort_order=? WHERE id=?');
        ids.forEach((pageId, index) => updateStmt.run(index, pageId));
        state.db.prepare("UPDATE databases SET sort_key='manual',sort_descending=0 WHERE page_id=?").run(id);
      }

      return true;
    })(),
  );

  ipcMain.handle('database:records:trash', (event, { id, recordIds }) => {
    assertDatabaseWritable(event.sender, id);
    if (!Array.isArray(recordIds) || !recordIds.length) return 0;

    const ids = [...new Set(recordIds.map(Number))];
    if (ids.some((pageId) => !Number.isSafeInteger(pageId))) {
      throw new Error('Invalid record selection.');
    }

    const placeholders = ids.map(() => '?').join(',');
    const records = rows(
      `SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL AND id IN (${placeholders})`,
      id,
      ...ids,
    );
    if (records.length !== ids.length) {
      throw new Error('Some selected records are no longer in this database.');
    }
    assertPageTreeAvailable(event.sender, records);

    const count = state.db.transaction(() => {
      state.db
        .prepare(
          `WITH RECURSIVE tree(id,root_id) AS (
             SELECT id,id FROM pages WHERE parent_id=? AND id IN (${placeholders})
             UNION ALL
             SELECT p.id,t.root_id FROM pages p JOIN tree t ON p.parent_id=t.id
           )
           UPDATE pages
           SET trashed_by=(SELECT root_id FROM tree WHERE tree.id=pages.id),updated_at=unixepoch()
           WHERE id IN (SELECT id FROM tree) AND trashed_by IS NULL`,
        )
        .run(id, ...ids);
      return records.length;
    })();

    if (count) {
      pushWorkspaceUndo({ type: 'trash', pageIds: ids });
      notifyWorkspacePagesTrashed(event.sender, ids);
    }
    return count;
  });

  ipcMain.handle('database:records:bulk-value', (event, { id, recordIds, propertyId, value }) => {
    id = Number(id);
    propertyId = Number(propertyId);
    if (!Array.isArray(recordIds) || !recordIds.length) return 0;

    const ids = [...new Set(recordIds.map(Number))];
    if (ids.some((pageId) => !Number.isSafeInteger(pageId))) {
      throw new Error('Invalid record selection.');
    }

    assertDatabaseWritable(event.sender, id);
    const property = one('SELECT kind,options FROM database_properties WHERE id=? AND database_page_id=?', propertyId, id);
    if (!property) throw new Error('Property does not belong to this database.');
    if (['created_time', 'last_edited_time'].includes(property.kind)) {
      throw new Error('Generated time properties are read-only.');
    }

    const records = rows(
      `SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL AND id IN (${ids.map(() => '?').join(',')})`,
      id,
      ...ids,
    );
    if (records.length !== ids.length) {
      throw new Error('Some selected records are no longer in this database.');
    }

    const normalizedValue = String(value ?? '');
    validatePropertyValue(property.kind, property.options, normalizedValue);

    const upsertStmt = state.db.prepare(
      'INSERT INTO page_property_values(page_id,property_id,value) VALUES(?,?,?) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value',
    );
    const touchStmt = state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?');

    return state.db.transaction(() => {
      for (const record of records) {
        upsertStmt.run(record.id, propertyId, normalizedValue);
        touchStmt.run(record.id);
      }
      return records.length;
    })();
  });

  ipcMain.handle('database:sort', (event, { id, key, descending = true }) => {
    id = Number(id);
    assertDatabaseWritable(event.sender, id);
    key = String(key ?? '');
    if (!['title', 'priority', 'created_at', 'manual'].includes(key)) {
      if (
        !/^[+-]?\d+$/.test(key) ||
        !Number.isSafeInteger(Number(key)) ||
        !one('SELECT id FROM database_properties WHERE id=? AND database_page_id=?', Number(key), id)
      ) {
        throw new Error('Sort property not found.');
      }
    }
    return state.db.prepare('UPDATE databases SET sort_key=?,sort_descending=? WHERE page_id=?').run(key, descending ? 1 : 0, id).changes;
  });

  ipcMain.handle('database:view-settings', (event, { id, settings }) => {
    assertDatabaseWritable(event.sender, id);
    if (!settings || typeof settings !== 'object' || Array.isArray(settings)) {
      throw new Error('Invalid database view settings.');
    }

    const current = one('SELECT view_settings FROM databases WHERE page_id=?', id);
    if (!current) throw new Error('Database not found.');

    const propsById = new Map(rows('SELECT id,kind FROM database_properties WHERE database_page_id=?', id).map((p) => [Number(p.id), p]));
    const next = parseViewSettings(current.view_settings);

    for (const [key, value] of Object.entries(settings)) {
      if (!ALLOWED_VIEW_SETTINGS.has(key)) continue;
      next[key] = sanitizeViewSetting(key, value, propsById);
    }

    return state.db.prepare('UPDATE databases SET view_settings=? WHERE page_id=?').run(JSON.stringify(next), id).changes;
  });

  ipcMain.handle('database:gallery-columns', (event, { id, columns }) => {
    assertDatabaseWritable(event.sender, id);
    columns = Number(columns);
    if (!Number.isInteger(columns) || columns < 1 || columns > 6) {
      throw new Error('Gallery columns must be between 1 and 6.');
    }
    return state.db.prepare('UPDATE databases SET gallery_columns=? WHERE page_id=?').run(columns, id).changes;
  });

  ipcMain.handle('database:value', (event, { pageId, propertyId, value }) => {
    const normalizedValue = String(value ?? '');
    const property = one(
      'SELECT d.kind,d.options FROM pages p JOIN database_properties d ON d.database_page_id=p.parent_id WHERE p.id=? AND p.trashed_by IS NULL AND d.id=?',
      pageId,
      propertyId,
    );
    if (!property) throw new Error('Record or database property not found.');
    if (['created_time', 'last_edited_time'].includes(property.kind)) {
      throw new Error('Generated time properties are read-only.');
    }

    const parent = one('SELECT parent_id FROM pages WHERE id=?', pageId);
    assertDatabaseWritable(event.sender, parent.parent_id);
    validatePropertyValue(property.kind, property.options, normalizedValue);

    return state.db.transaction(() => {
      const changes = state.db
        .prepare('INSERT INTO page_property_values(page_id,property_id,value) VALUES(?,?,?) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value')
        .run(pageId, propertyId, normalizedValue).changes;
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return changes;
    })();
  });

  ipcMain.handle('database:view', (event, { id, view }) => {
    assertDatabaseWritable(event.sender, id);
    if (!['table', 'gallery', 'board'].includes(view)) {
      throw new Error('Invalid database view.');
    }
    return state.db.prepare('UPDATE databases SET view_mode=? WHERE page_id=?').run(view, id).changes;
  });
}

module.exports = { registerDatabaseIpc };
