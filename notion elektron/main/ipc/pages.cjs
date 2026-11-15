/**
 * Page tree CRUD, clipboard transfer, trash/restore, cloning, search, and page IPC handlers.
 */

const { clipboard } = require('electron');
const { state, rows, one } = require('../state.cjs');
const { readPropertyOptions, serializePropertyOptions } = require('../utils/property-utils.cjs');
const { notifyWorkspaceChanged, notifyWorkspaceTreeChanged } = require('../services/notifications.cjs');
const {
  effectivePageLeaseId,
  assertPageWritable,
  assertNoConflictingPageLease,
  assertWorkspaceHistoryPageLease,
  assertPageTreeAvailable,
} = require('../services/leases.cjs');
const {
  pushWorkspaceUndo,
  pruneWorkspaceHistory,
  clearWorkspaceHistory,
  recordPageUndo,
  pageHistorySummary,
} = require('../services/history.cjs');
const { normalizeElectronBlockColumns } = require('./blocks.cjs');

/**
 * Remove the parent page's link block and text marker referencing a child page.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number} pageId
 * @param {boolean} matchEmptyTitleAsUntitled
 */
function cleanupParentPageReference(transaction, pageId, matchEmptyTitleAsUntitled) {
  const page = transaction.prepare('SELECT parent_id,title FROM pages WHERE id=?').get(pageId);
  if (!page || page.parent_id === null) return;

  transaction
    .prepare("DELETE FROM blocks WHERE page_id=? AND kind IN ('page_link','database') AND content=?")
    .run(page.parent_id, String(pageId));

  if (matchEmptyTitleAsUntitled) {
    const cleanTitle = String(page.title || '').trim();
    const marker = `↗ ${cleanTitle}`;
    transaction
      .prepare(
        "DELETE FROM blocks WHERE page_id=? AND kind='text' AND (content=? OR (content='↗ Untitled' AND ?='')) AND NOT EXISTS(SELECT 1 FROM pages WHERE parent_id=? AND trashed_by IS NULL AND title=? AND id!=?)",
      )
      .run(page.parent_id, marker, cleanTitle, page.parent_id, page.title, pageId);
  } else {
    const marker = `↗ ${String(page.title || '').trim() || 'Untitled'}`;
    transaction
      .prepare(
        "DELETE FROM blocks WHERE page_id=? AND kind='text' AND content=? AND NOT EXISTS(SELECT 1 FROM pages WHERE parent_id=? AND trashed_by IS NULL AND title=? AND id!=?)",
      )
      .run(page.parent_id, marker, page.parent_id, page.title, pageId);
  }

  normalizeElectronBlockColumns(transaction, page.parent_id);
  const count = transaction.prepare('SELECT count(*) n FROM blocks WHERE page_id=?').get(page.parent_id).n;
  transaction.prepare('UPDATE pages SET block_count=? WHERE id=?').run(count, page.parent_id);
}

function removePageParentReference(transaction, pageId) {
  cleanupParentPageReference(transaction, pageId, false);
}

function removeTrashParentReference(transaction, pageId) {
  cleanupParentPageReference(transaction, pageId, true);
}

/**
 * Update a parent page's `↗ Title` text reference when a child page is renamed.
 * @param {number} pageId
 * @param {string} oldTitle
 * @param {string} newTitle
 * @param {import('electron').WebContents} sender
 */
function syncParentTitleReference(pageId, oldTitle, newTitle, sender) {
  const page = one('SELECT parent_id FROM pages WHERE id=?', pageId);
  if (!page || page.parent_id === null || oldTitle === newTitle) return;
  if (one('SELECT 1 FROM databases WHERE page_id=?', page.parent_id)) return;

  const oldMarker = `↗ ${String(oldTitle || '').trim() || 'Untitled'}`;
  const newMarker = `↗ ${String(newTitle || '').trim() || 'Untitled'}`;
  if (oldMarker === newMarker) return;

  const duplicate = one(
    'SELECT 1 FROM pages WHERE parent_id=? AND trashed_by IS NULL AND title=? AND id!=?',
    page.parent_id,
    oldTitle,
    pageId,
  );
  if (duplicate) return;

  const changed = state.db
    .prepare("UPDATE blocks SET content=? WHERE page_id=? AND kind='text' AND content=?")
    .run(newMarker, page.parent_id, oldMarker).changes;

  if (changed) {
    state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(page.parent_id);
    const parent = one('SELECT title,icon FROM pages WHERE id=?', page.parent_id);
    if (parent) {
      notifyWorkspaceChanged(sender, { id: page.parent_id, title: parent.title, icon: parent.icon });
    }
  }
}

/**
 * Restore a missing parent page reference block when restoring a page from trash.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number} pageId
 */
function ensurePageParentReference(transaction, pageId) {
  const page = transaction.prepare('SELECT parent_id,title,trashed_by FROM pages WHERE id=?').get(pageId);
  if (!page || page.parent_id === null || page.trashed_by !== null) return;
  if (transaction.prepare('SELECT 1 FROM databases WHERE page_id=?').get(page.parent_id)) return;

  const parent = transaction.prepare('SELECT trashed_by FROM pages WHERE id=?').get(page.parent_id);
  if (!parent || parent.trashed_by !== null) return;

  const title = String(page.title || '').trim() || 'Untitled';
  const target = title.toLowerCase();
  const isDatabase = Boolean(transaction.prepare('SELECT page_id FROM databases WHERE page_id=?').get(pageId));
  const blocks = transaction.prepare('SELECT id,kind,content FROM blocks WHERE page_id=? ORDER BY position').all(page.parent_id);

  const alreadyLinked = blocks.some((block) => {
    if (['page_link', 'database'].includes(block.kind) && String(block.content).trim() === String(pageId)) {
      return true;
    }
    if (block.kind !== 'text' || !String(block.content).trimStart().startsWith('↗')) {
      return false;
    }
    const marker = String(block.content).trimStart().replace(/^↗\s*/, '').trim().toLowerCase();
    return marker === target;
  });
  if (alreadyLinked) return;

  const empty = blocks.find((block) => block.kind === 'text' && !String(block.content).trim());
  const kind = isDatabase ? 'database' : 'text';
  const content = isDatabase ? String(pageId) : `↗ ${title}`;

  if (empty) {
    transaction.prepare('UPDATE blocks SET kind=?,content=? WHERE id=?').run(kind, content, empty.id);
  } else {
    const position = transaction.prepare('SELECT coalesce(max(position)+1,0) position FROM blocks WHERE page_id=?').get(page.parent_id).position;
    transaction.prepare('INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,?,?)').run(page.parent_id, position, kind, content);
  }

  transaction.prepare('UPDATE pages SET block_count=(SELECT count(*) FROM blocks WHERE page_id=?) WHERE id=?').run(page.parent_id, page.parent_id);
  transaction.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(page.parent_id);
}

/**
 * Trash or restore a subtree rooted at `rootId`.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number} rootId
 * @param {boolean} restore
 * @returns {number}
 */
function setPageTrashState(transaction, rootId, restore) {
  if (restore) {
    transaction
      .prepare('UPDATE pages SET parent_id=NULL WHERE id=? AND parent_id IN (SELECT id FROM pages WHERE trashed_by IS NOT NULL AND trashed_by!=?)')
      .run(rootId, rootId);
    const changed = transaction.prepare('UPDATE pages SET trashed_by=NULL,updated_at=unixepoch() WHERE trashed_by=?').run(rootId).changes;
    if (changed) ensurePageParentReference(transaction, rootId);
    return changed;
  }

  removeTrashParentReference(transaction, rootId);
  return transaction
    .prepare(
      `WITH RECURSIVE tree(id) AS (
         SELECT id FROM pages WHERE id=?
         UNION ALL
         SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id
       )
       UPDATE pages SET trashed_by=?,updated_at=unixepoch()
       WHERE id IN (SELECT id FROM tree) AND trashed_by IS NULL`,
    )
    .run(rootId, rootId).changes;
}

/**
 * Apply workspace-level trash or restore during workspace undo/redo.
 * @param {import('electron').WebContents} sender
 * @param {number[]} pageIds
 * @param {boolean} restore
 * @returns {number[]}
 */
function changeWorkspaceTrashState(sender, pageIds, restore) {
  const ids = [...new Set((pageIds || []).map(Number))];
  if (ids.some((id) => !Number.isSafeInteger(id))) {
    throw new Error('Invalid page selection.');
  }

  const roots = ids
    .map((id) => one('SELECT id,trashed_by FROM pages WHERE id=?', id))
    .filter((page) => page && (restore ? Number(page.trashed_by) === Number(page.id) : page.trashed_by === null));
  if (!roots.length) return [];

  for (const root of roots) {
    const tree = rows(
      'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) SELECT id FROM tree',
      root.id,
    );
    for (const page of tree) {
      assertWorkspaceHistoryPageLease(sender, page.id);
    }
  }

  return state.db.transaction((transaction) => {
    for (const root of roots) {
      setPageTrashState(transaction, Number(root.id), restore);
    }
    return roots.map((page) => Number(page.id));
  })(state.db);
}

/**
 * Recursively clone a page tree and its blocks, properties, and database settings.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number} source
 * @param {number|null} parent
 * @param {Map<number, number>} pages
 * @param {Map<number, number>} properties
 * @returns {number}
 */
function clonePageTree(transaction, source, parent, pages, properties) {
  const children = transaction.prepare('SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL ORDER BY sort_order,id').all(source).map((row) => row.id);
  const inserted = transaction
    .prepare(
      `INSERT INTO pages(parent_id,title,icon,cover,cover_image,cover_x,cover_y,sort_order,full_width,priority,created_at,updated_at)
       SELECT ?,title,icon,cover,cover_image,cover_x,cover_y,(SELECT coalesce(max(sort_order)+1,0) FROM pages WHERE parent_id IS ?),full_width,priority,unixepoch(),unixepoch()
       FROM pages WHERE id=?`,
    )
    .run(parent, parent, source);

  const target = Number(inserted.lastInsertRowid);
  pages.set(Number(source), target);

  transaction
    .prepare(
      `INSERT INTO blocks(page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles)
       SELECT ?,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles FROM blocks WHERE page_id=? ORDER BY position`,
    )
    .run(target, source);

  transaction
    .prepare(
      `INSERT INTO databases(page_id,view_mode,sort_key,sort_descending,gallery_columns,view_settings)
       SELECT ?,view_mode,sort_key,sort_descending,gallery_columns,view_settings FROM databases WHERE page_id=?`,
    )
    .run(target, source);

  const dbProps = transaction.prepare('SELECT id FROM database_properties WHERE database_page_id=? ORDER BY position').all(source).map((row) => row.id);
  for (const property of dbProps) {
    const insertedProperty = transaction
      .prepare('INSERT INTO database_properties(database_page_id,name,kind,options,position) SELECT ?,name,kind,options,position FROM database_properties WHERE id=?')
      .run(target, property);
    properties.set(Number(property), Number(insertedProperty.lastInsertRowid));
  }

  const customProps = transaction.prepare('SELECT id FROM custom_page_properties WHERE page_id=? ORDER BY position').all(source).map((row) => row.id);
  for (const property of customProps) {
    const insertedProperty = transaction
      .prepare('INSERT INTO custom_page_properties(page_id,name,kind,options,position) SELECT ?,name,kind,options,position FROM custom_page_properties WHERE id=?')
      .run(target, property);
    const newProperty = Number(insertedProperty.lastInsertRowid);
    transaction
      .prepare('INSERT INTO custom_page_property_values(page_id,property_id,value) SELECT ?,?,value FROM custom_page_property_values WHERE page_id=? AND property_id=?')
      .run(target, newProperty, source, property);
  }

  transaction.prepare('INSERT INTO page_property_values(page_id,property_id,value) SELECT ?,property_id,value FROM page_property_values WHERE page_id=?').run(target, source);

  for (const child of children) {
    clonePageTree(transaction, child, target, pages, properties);
  }
  return target;
}

/**
 * Remap cloned internal page references and database view property IDs.
 * @param {import('better-sqlite3').Database} transaction
 * @param {Map<number, number>} pages
 * @param {Map<number, number>} properties
 */
function remapClonedReferences(transaction, pages, properties) {
  for (const [oldId, newId] of properties) {
    const source = transaction.prepare('SELECT database_page_id FROM database_properties WHERE id=?').get(oldId);
    const targetDatabase = source && pages.get(Number(source.database_page_id));
    if (targetDatabase !== undefined) {
      const records = transaction.prepare('SELECT id FROM pages WHERE parent_id=?').all(targetDatabase);
      const updateStmt = transaction.prepare('UPDATE page_property_values SET property_id=? WHERE page_id=? AND property_id=?');
      for (const record of records) {
        updateStmt.run(newId, record.id, oldId);
      }
    }
  }

  for (const page of pages.values()) {
    for (const [oldId, newId] of pages) {
      transaction
        .prepare("UPDATE blocks SET content=? WHERE page_id=? AND kind IN ('database','page_link') AND content=?")
        .run(String(newId), page, String(oldId));
    }

    const source = transaction.prepare('SELECT view_settings,sort_key FROM databases WHERE page_id=?').get(page);
    if (!source) continue;

    let settings = {};
    try {
      settings = JSON.parse(source.view_settings || '{}');
    } catch {
      settings = {};
    }

    for (const key of ['gallery_visible_properties', 'table_visible_properties', 'board_visible_properties']) {
      if (Array.isArray(settings[key])) {
        settings[key] = settings[key].map((id) => properties.get(Number(id)) ?? id);
      }
    }
    for (const key of ['table_column_widths', 'table_column_calculations']) {
      if (settings[key] && typeof settings[key] === 'object') {
        settings[key] = Object.fromEntries(
          Object.entries(settings[key]).map(([id, value]) => [String(properties.get(Number(id)) ?? id), value]),
        );
      }
    }

    const sortId = Number(source.sort_key);
    const sortKey = Number.isSafeInteger(sortId) ? String(properties.get(sortId) ?? source.sort_key) : source.sort_key;
    transaction.prepare('UPDATE databases SET view_settings=?,sort_key=? WHERE page_id=?').run(JSON.stringify(settings), sortKey, page);
  }
}

/**
 * Create a new database page with its default Status property and initial empty block.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number|null} parentId
 * @param {string} [title]
 * @returns {number|bigint}
 */
function createDatabasePage(transaction, parentId, title = 'New database') {
  title = String(title || 'New database').trim().slice(0, 500) || 'New database';
  const id = transaction
    .prepare(
      `INSERT INTO pages(parent_id,title,icon,sort_order,created_at,updated_at)
       VALUES(?,?,'▦',coalesce((SELECT max(sort_order)+1 FROM pages WHERE parent_id IS ?),0),unixepoch(),unixepoch())`,
    )
    .run(parentId, title, parentId).lastInsertRowid;

  transaction.prepare('INSERT INTO databases(page_id) VALUES(?)').run(id);
  transaction
    .prepare("INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?,'Status','status',?,0)")
    .run(id, serializePropertyOptions([{ name: 'To do' }, { name: 'In progress' }, { name: 'Done' }]));
  transaction.prepare("INSERT INTO blocks(page_id,position,kind,content,checked) VALUES(?,0,'text','',0)").run(id);
  return id;
}

/**
 * Resolve top-level selected pages, excluding descendants whose ancestor is also selected.
 * @param {unknown[]} input
 * @returns {object[]}
 */
function topLevelPageSelection(input) {
  if (!Array.isArray(input) || !input.length) {
    throw new Error('Select at least one page.');
  }
  const ids = [...new Set(input.map(Number))];
  if (ids.some((id) => !Number.isSafeInteger(id))) {
    throw new Error('Invalid page selection.');
  }

  const selected = new Set(ids);
  const sources = ids.map((id) => {
    const page = one('SELECT id,parent_id,trashed_by,title FROM pages WHERE id=?', id);
    if (!page || page.trashed_by !== null) {
      throw new Error('A selected page is unavailable.');
    }
    return page;
  });

  return sources.filter((page) => {
    let parent = page.parent_id;
    while (parent !== null) {
      if (selected.has(Number(parent))) return false;
      parent = one('SELECT parent_id FROM pages WHERE id=?', parent)?.parent_id ?? null;
    }
    return true;
  });
}

/**
 * Preserve custom and database property definitions/values when moving or copying a page.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number} source
 * @param {number|null} sourceParent
 * @param {number} target
 * @param {number|null} destinationParent
 * @param {boolean} destinationIsDatabase
 */
function preserveTransferredProperties(transaction, source, sourceParent, target, destinationParent, destinationIsDatabase) {
  const values = transaction
    .prepare(
      `SELECT p.name,p.kind,p.options,coalesce(v.value,'') value
       FROM database_properties p
       LEFT JOIN page_property_values v ON v.property_id=p.id AND v.page_id=?
       WHERE p.database_page_id=?
       UNION ALL
       SELECT p.name,p.kind,p.options,coalesce(v.value,'') value
       FROM custom_page_properties p
       LEFT JOIN custom_page_property_values v ON v.property_id=p.id AND v.page_id=?
       WHERE p.page_id=?`,
    )
    .all(source, sourceParent, source, source);

  const table = destinationIsDatabase ? 'database_properties' : 'custom_page_properties';
  const ownerColumn = destinationIsDatabase ? 'database_page_id' : 'page_id';
  const valueTable = destinationIsDatabase ? 'page_property_values' : 'custom_page_property_values';
  const owner = destinationIsDatabase ? destinationParent : target;

  for (const item of values) {
    let candidate = item.name;
    let suffix = 1;
    let property;

    for (;;) {
      property = transaction.prepare(`SELECT id,kind,options FROM ${table} WHERE ${ownerColumn}=? AND name=?`).get(owner, candidate);
      if (!property) break;
      if (property.kind === item.kind) {
        const options = readPropertyOptions(property.options);
        for (const option of readPropertyOptions(item.options)) {
          if (!options.some((existing) => existing.name === option.name)) {
            options.push(option);
          }
        }
        transaction.prepare(`UPDATE ${table} SET options=? WHERE id=?`).run(serializePropertyOptions(options), property.id);
        break;
      }
      candidate = `${item.name} (${item.kind} ${suffix++})`;
    }

    if (!property) {
      const position = transaction.prepare(`SELECT coalesce(max(position)+1,0) position FROM ${table} WHERE ${ownerColumn}=?`).get(owner).position;
      property = {
        id: Number(
          transaction.prepare(`INSERT INTO ${table}(${ownerColumn},name,kind,options,position) VALUES(?,?,?,?,?)`).run(owner, candidate, item.kind, item.options, position)
            .lastInsertRowid,
        ),
      };
    }

    transaction
      .prepare(`INSERT INTO ${valueTable}(page_id,property_id,value) VALUES(?,?,?) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value`)
      .run(target, property.id, item.value);
  }
}

/**
 * Move or copy top-level page roots into a new parent at the requested position.
 * @param {import('better-sqlite3').Database} transaction
 * @param {object[]} roots
 * @param {number|null} parentId
 * @param {number|undefined} position
 * @param {boolean} copy
 * @returns {number[]}
 */
function transferPageRoots(transaction, roots, parentId, position, copy) {
  const destination = parentId === null ? null : transaction.prepare('SELECT id,trashed_by FROM pages WHERE id=?').get(parentId);
  if (parentId !== null && (!destination || destination.trashed_by !== null)) {
    throw new Error('Destination page is unavailable.');
  }

  const destinationIsDatabase = parentId !== null && Boolean(transaction.prepare('SELECT page_id FROM databases WHERE page_id=?').get(parentId));
  for (const root of roots) {
    if (parentId !== null) {
      const cycle = transaction
        .prepare(
          'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) SELECT 1 found FROM tree WHERE id=? LIMIT 1',
        )
        .get(root.id, parentId);
      if (cycle) {
        throw new Error('A page cannot be placed inside itself or a subpage.');
      }
    }
  }

  const siblingRows =
    parentId === null
      ? transaction.prepare('SELECT id FROM pages WHERE parent_id IS NULL AND trashed_by IS NULL ORDER BY sort_order,id').all()
      : transaction.prepare('SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL ORDER BY sort_order,id').all(parentId);

  const siblings = siblingRows.map((page) => Number(page.id)).filter((id) => copy || !roots.some((root) => root.id === id));
  let at = Math.max(0, Math.min(siblings.length, Number.isFinite(Number(position)) ? Math.trunc(Number(position)) : siblings.length));

  const result = [];
  const pageMapping = new Map();
  const propertyMapping = new Map();

  for (const root of roots) {
    let id = root.id;
    const sourceIsDatabase = root.parent_id !== null && Boolean(transaction.prepare('SELECT page_id FROM databases WHERE page_id=?').get(root.parent_id));
    if (copy) {
      id = clonePageTree(transaction, root.id, parentId, pageMapping, propertyMapping);
    } else {
      if (root.parent_id !== null && !sourceIsDatabase) {
        removePageParentReference(transaction, root.id);
      }
      transaction.prepare('UPDATE pages SET parent_id=?,updated_at=unixepoch() WHERE id=?').run(parentId, id);
    }

    preserveTransferredProperties(transaction, root.id, root.parent_id, id, parentId, destinationIsDatabase);
    if (destinationIsDatabase) {
      transaction.prepare('DELETE FROM custom_page_properties WHERE page_id=?').run(id);
    }
    transaction
      .prepare('DELETE FROM page_property_values WHERE page_id=? AND property_id IN (SELECT id FROM database_properties WHERE database_page_id IS NOT ?)')
      .run(id, destinationIsDatabase ? parentId : null);

    if (!copy && root.parent_id !== null && root.parent_id !== parentId) {
      const oldPages = transaction.prepare('SELECT id FROM pages WHERE parent_id=? AND trashed_by IS NULL ORDER BY sort_order,id').all(root.parent_id);
      const normalizeOrder = transaction.prepare('UPDATE pages SET sort_order=? WHERE id=?');
      oldPages.forEach((page, index) => normalizeOrder.run(index, page.id));
      transaction.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(root.parent_id);
    }

    result.push(Number(id));
    siblings.splice(at++, 0, Number(id));
  }

  if (copy) {
    remapClonedReferences(transaction, pageMapping, propertyMapping);
  }

  const movedSet = new Set(result.map(Number));
  const updateSibling = transaction.prepare(
    'UPDATE pages SET sort_order=?,parent_id=?,updated_at=CASE WHEN ? THEN unixepoch() ELSE updated_at END WHERE id=?',
  );
  siblings.forEach((id, index) => updateSibling.run(index, parentId, movedSet.has(Number(id)) ? 1 : 0, id));

  if (parentId !== null && !destinationIsDatabase) {
    const totalBlocks = Number(transaction.prepare('SELECT count(*) n FROM blocks WHERE page_id=?').get(parentId).n);
    const requested = Number(position);
    let blockAt = Number.isFinite(requested) ? Math.trunc(requested) : totalBlocks;
    blockAt = Math.max(0, Math.min(totalBlocks, blockAt));

    transaction.prepare('UPDATE blocks SET position=position+? WHERE page_id=? AND position>=?').run(result.length, parentId, blockAt);
    const insertBlock = transaction.prepare('INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,?,?)');
    result.forEach((id, index) => {
      const isDb = Boolean(transaction.prepare('SELECT 1 FROM databases WHERE page_id=?').get(id));
      insertBlock.run(parentId, blockAt + index, isDb ? 'database' : 'page_link', String(id));
    });
    transaction.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(parentId);
  }

  return result;
}

/**
 * Load trashed root page rows matching a list of page IDs.
 * @param {unknown[]} pageIds
 * @returns {object[]}
 */
function trashedRootSelection(pageIds) {
  if (!Array.isArray(pageIds) || !pageIds.length) return [];
  return [...new Set(pageIds.map(Number))]
    .map((id) => one('SELECT id,trashed_by FROM pages WHERE id=?', id))
    .filter((page) => page && Number(page.id) === Number(page.trashed_by));
}

/**
 * Register all page and search IPC handlers.
 * @param {import('electron').IpcMain} ipcMain
 */
function registerPageIpc(ipcMain) {
  ipcMain.handle('page:get', (_event, id) => {
    const page = one('SELECT * FROM pages WHERE id=?', id);
    if (!page) return null;

    page.blocks = rows(
      'SELECT id,page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles FROM blocks WHERE page_id=? ORDER BY position,id',
      id,
    );
    page.properties = rows(
      `SELECT p.id,p.name,p.kind,p.options,coalesce(v.value,'') value
       FROM custom_page_properties p
       LEFT JOIN custom_page_property_values v ON v.property_id=p.id AND v.page_id=?
       WHERE p.page_id=? ORDER BY p.position`,
      id,
      id,
    );
    page.isDatabase = !!one('SELECT page_id FROM databases WHERE page_id=?', id);
    page.parentDatabaseId =
      page.parent_id !== null && page.parent_id !== undefined && one('SELECT page_id FROM databases WHERE page_id=?', page.parent_id)
        ? Number(page.parent_id)
        : null;
    page.databaseProperties =
      page.parentDatabaseId !== null
        ? rows(
            `SELECT p.id,p.name,p.kind,p.options,coalesce(v.value,'') value
             FROM database_properties p
             LEFT JOIN page_property_values v ON v.property_id=p.id AND v.page_id=?
             WHERE p.database_page_id=? ORDER BY p.position`,
            id,
            page.parentDatabaseId,
          )
        : [];
    page.leasePageId = effectivePageLeaseId(id);
    page.history = pageHistorySummary(id);
    return page;
  });

  ipcMain.handle('page:save', (event, { id, title, icon, full_width, cover, cover_image, cover_x, cover_y, priority }) => {
    assertPageWritable(event.sender, id);
    const previous = one('SELECT title,icon,cover,cover_image,cover_x,cover_y,full_width,priority FROM pages WHERE id=?', id);
    const discreteChanged = Boolean(
      previous &&
        (previous.icon !== icon ||
          previous.cover !== cover ||
          previous.cover_image !== (cover_image || null) ||
          Boolean(previous.full_width) !== Boolean(full_width) ||
          previous.priority !== priority),
    );
    const coalescedChanged = Boolean(
      previous && (previous.title !== title || previous.cover_x !== cover_x || previous.cover_y !== cover_y),
    );
    if (discreteChanged || coalescedChanged) {
      recordPageUndo(id, 'Sayfa görünümü düzenlendi', discreteChanged);
    }

    state.db
      .prepare('UPDATE pages SET title=?,icon=?,full_width=?,cover=?,cover_image=?,cover_x=?,cover_y=?,priority=?,updated_at=unixepoch() WHERE id=?')
      .run(
        title,
        icon,
        full_width ? 1 : 0,
        Math.max(0, Math.min(4, Number(cover) || 0)),
        cover_image || null,
        Math.max(0, Math.min(1, Number(cover_x) || 0)),
        Math.max(0, Math.min(1, Number(cover_y) || 0)),
        Math.max(0, Math.min(3, Number(priority) || 0)),
        id,
      );

    if (previous && previous.title !== title) {
      syncParentTitleReference(id, previous.title, title, event.sender);
    }
    notifyWorkspaceChanged(event.sender, { id, title, icon });
    return true;
  });

  ipcMain.handle('page:create', (event, { parentId = null, title, database = false }) => {
    if (state.workspaceRestoreActive) {
      throw new Error('The workspace is being restored.');
    }
    title = title === undefined || title === null ? (database ? 'New database' : 'Untitled') : String(title);
    if (database && !title.trim()) title = 'New database';

    const isRecord = parentId !== null && Boolean(one('SELECT page_id FROM databases WHERE page_id=?', parentId));
    const id = state.db.transaction(() => {
      if (parentId !== null) {
        const parent = one('SELECT id FROM pages WHERE id=? AND trashed_by IS NULL', parentId);
        if (!parent) throw new Error('The parent page is unavailable.');
        assertNoConflictingPageLease(event.sender, parentId);
      }

      const createdId = database
        ? createDatabasePage(state.db, parentId, title)
        : state.db
            .prepare(
              `INSERT INTO pages(parent_id,title,sort_order,created_at,updated_at)
               VALUES(?,?,coalesce((SELECT max(sort_order)+1 FROM pages WHERE parent_id IS ?),0),unixepoch(),unixepoch())`,
            )
            .run(parentId, title, parentId).lastInsertRowid;

      if (!database) {
        state.db.prepare("INSERT INTO blocks(page_id,position,kind,content,checked) VALUES(?,0,'text','',0)").run(createdId);
      }

      if (parentId !== null && !isRecord) {
        const refKind = database ? 'database' : 'text';
        const refContent = database ? String(createdId) : `↗ ${title.trim() || 'Untitled'}`;
        const empty = one("SELECT id FROM blocks WHERE page_id=? AND kind='text' AND content='' ORDER BY position LIMIT 1", parentId);
        if (empty) {
          state.db.prepare('UPDATE blocks SET kind=?,content=? WHERE id=?').run(refKind, refContent, empty.id);
        } else {
          const position = Number(one('SELECT count(*) n FROM blocks WHERE page_id=?', parentId).n);
          state.db.prepare('INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,?,?)').run(parentId, position, refKind, refContent);
        }
        state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(parentId);
      }

      return createdId;
    })();

    if (isRecord) {
      pushWorkspaceUndo({ type: 'create-record', pageIds: [Number(id)] });
    }
    notifyWorkspaceTreeChanged(event.sender);
    return id;
  });

  ipcMain.handle('page:duplicate', (event, id) => {
    const source = one('SELECT id,parent_id FROM pages WHERE id=? AND trashed_by IS NULL', id);
    if (!source) throw new Error('Page is unavailable.');
    assertPageWritable(event.sender, source.id);
    assertPageTreeAvailable(event.sender, [source]);

    const copy = state.db.transaction((transaction) => {
      const pages = new Map();
      const properties = new Map();
      const target = clonePageTree(transaction, source.id, source.parent_id, pages, properties);
      remapClonedReferences(transaction, pages, properties);
      return target;
    })(state.db);

    notifyWorkspaceTreeChanged(event.sender);
    return copy;
  });

  ipcMain.handle('page:transfer', (event, { pageIds, parentId = null, position, copy = false }) => {
    const roots = topLevelPageSelection(pageIds);
    if (parentId !== null) parentId = Number(parentId);
    assertPageTreeAvailable(event.sender, roots);
    if (parentId !== null) assertNoConflictingPageLease(event.sender, parentId);

    const result = state.db.transaction((transaction) => transferPageRoots(transaction, roots, parentId, position, copy === true))(state.db);
    notifyWorkspaceTreeChanged(event.sender);
    return result;
  });

  ipcMain.handle('page:clipboard:copy', (_event, { pageIds, cut = false }) => {
    const roots = topLevelPageSelection(pageIds);
    state.blockClipboard = null;
    state.pageClipboard = {
      pageIds: roots.map((page) => Number(page.id)),
      cut: cut === true,
      text: roots.map((page) => `[${page.title || 'Untitled'}](noteon://page/${page.id})`).join('\n'),
    };
    clipboard.writeText(state.pageClipboard.text);
    return state.pageClipboard.pageIds.length;
  });

  ipcMain.handle('page:clipboard:paste', (event, { parentId = null, position }) => {
    if (!state.pageClipboard || clipboard.readText() !== state.pageClipboard.text) return null;
    const roots = topLevelPageSelection(state.pageClipboard.pageIds);
    assertPageTreeAvailable(event.sender, roots);
    if (parentId !== null) {
      parentId = Number(parentId);
      assertNoConflictingPageLease(event.sender, parentId);
    }

    const result = state.db.transaction((transaction) => transferPageRoots(transaction, roots, parentId, position, !state.pageClipboard.cut))(
      state.db,
    );
    if (state.pageClipboard.cut) state.pageClipboard.cut = false;
    notifyWorkspaceTreeChanged(event.sender);
    return result;
  });

  ipcMain.handle('page:trash-many', (event, pageIds) => {
    const roots = topLevelPageSelection(pageIds);
    assertPageTreeAvailable(event.sender, roots);
    const count = state.db.transaction((transaction) =>
      roots.reduce((sum, root) => sum + setPageTrashState(transaction, Number(root.id), false), 0),
    )(state.db);

    if (count) {
      pushWorkspaceUndo({ type: 'trash', pageIds: roots.map((root) => Number(root.id)) });
      notifyWorkspaceTreeChanged(event.sender);
    }
    return count;
  });

  ipcMain.handle('page:restore-many', (event, pageIds) => {
    const roots = trashedRootSelection(pageIds);
    if (!roots.length) return 0;

    for (const root of roots) {
      for (const page of rows('SELECT id FROM pages WHERE trashed_by=?', root.id)) {
        assertNoConflictingPageLease(event.sender, page.id);
      }
    }

    const count = state.db.transaction((transaction) =>
      roots.reduce((sum, root) => sum + setPageTrashState(transaction, Number(root.id), true), 0),
    )(state.db);

    if (count) {
      pushWorkspaceUndo({ type: 'restore', pageIds: roots.map((root) => Number(root.id)) });
      notifyWorkspaceTreeChanged(event.sender);
    }
    return count;
  });

  ipcMain.handle('page:delete-permanently-many', (event, pageIds) => {
    const roots = trashedRootSelection(pageIds);
    if (!roots.length) return 0;

    for (const root of roots) {
      assertPageTreeAvailable(event.sender, [root]);
    }

    const deleted = [];
    const count = state.db.transaction(() => {
      let removed = 0;
      for (const root of roots) {
        const tree = rows(
          'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) SELECT id FROM tree',
          root.id,
        );
        deleted.push(...tree.map((page) => Number(page.id)));
        removed += state.db
          .prepare(
            'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) DELETE FROM pages WHERE id IN (SELECT id FROM tree) OR trashed_by=?',
          )
          .run(root.id, root.id).changes;
        for (const page of tree) {
          state.pageHistories.delete(Number(page.id));
        }
      }
      return removed;
    })();

    if (count) {
      pruneWorkspaceHistory(deleted);
      notifyWorkspaceTreeChanged(event.sender);
    }
    return count;
  });

  ipcMain.handle('page:create-linked', (event, { pageId, position, title = 'Untitled', kind = 'page_link', replaceBlock = null }) => {
    assertPageWritable(event.sender, pageId);
    const replacement = replaceBlock === null ? null : one('SELECT id FROM blocks WHERE id=? AND page_id=?', Number(replaceBlock), pageId);
    if (replaceBlock !== null && !replacement) {
      throw new Error('Block to replace was not found.');
    }
    recordPageUndo(pageId, kind === 'database' ? 'Veritabanı eklendi' : 'Alt sayfa eklendi', true);

    const createdPage = state.db.transaction(() => {
      const created =
        kind === 'database'
          ? createDatabasePage(state.db, pageId, title)
          : state.db
              .prepare(
                'INSERT INTO pages(parent_id,title,sort_order,created_at,updated_at) VALUES(?,?,coalesce((SELECT max(sort_order)+1 FROM pages WHERE parent_id=?),0),unixepoch(),unixepoch())',
              )
              .run(pageId, title, pageId).lastInsertRowid;

      if (replacement) {
        state.db.prepare("UPDATE blocks SET kind=?,content=?,text_styles='[]' WHERE id=?").run(kind, String(created), replacement.id);
      } else {
        state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>=?').run(pageId, position);
        state.db.prepare('INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,?,?)').run(pageId, position, kind, String(created));
      }
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return created;
    })();

    notifyWorkspaceTreeChanged(event.sender);
    return createdPage;
  });

  ipcMain.handle('page:trash', (event, id) => {
    id = Number(id);
    assertPageWritable(event.sender, id);
    const descendants = rows(
      'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) SELECT id FROM tree',
      id,
    );
    for (const page of descendants) {
      assertNoConflictingPageLease(event.sender, page.id);
    }

    const count = state.db.transaction((transaction) => setPageTrashState(transaction, id, false))(state.db);
    if (count) {
      pushWorkspaceUndo({ type: 'trash', pageIds: [id] });
      notifyWorkspaceTreeChanged(event.sender);
    }
    return count;
  });

  ipcMain.handle('page:restore', (event, id) => {
    id = Number(id);
    const root = one('SELECT trashed_by FROM pages WHERE id=?', id);
    if (!root || Number(root.trashed_by) !== id) return 0;

    for (const page of rows('SELECT id FROM pages WHERE trashed_by=?', id)) {
      assertNoConflictingPageLease(event.sender, page.id);
    }

    const count = state.db.transaction((transaction) => setPageTrashState(transaction, id, true))(state.db);
    if (count) {
      pushWorkspaceUndo({ type: 'restore', pageIds: [id] });
      notifyWorkspaceTreeChanged(event.sender);
    }
    return count;
  });

  ipcMain.handle('page:delete-permanently', (event, id) => {
    id = Number(id);
    if (!Number.isSafeInteger(id)) return false;
    assertNoConflictingPageLease(event.sender, id);
    const root = one('SELECT trashed_by FROM pages WHERE id=?', id);
    if (!root || Number(root.trashed_by) !== id) return false;

    const descendants = rows(
      'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) SELECT id FROM tree',
      id,
    );
    for (const page of descendants) {
      assertNoConflictingPageLease(event.sender, page.id);
    }

    const deleted = state.db.transaction(() => {
      state.db
        .prepare(
          'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) DELETE FROM pages WHERE id IN (SELECT id FROM tree) OR trashed_by=?',
        )
        .run(id, id);
      for (const page of descendants) {
        state.pageHistories.delete(Number(page.id));
      }
      return true;
    })();

    pruneWorkspaceHistory(descendants.map((page) => Number(page.id)));
    notifyWorkspaceTreeChanged(event.sender);
    return deleted;
  });

  ipcMain.handle('page:empty-trash', (event) => {
    const trashed = rows('SELECT id FROM pages WHERE trashed_by IS NOT NULL');
    for (const { id } of trashed) {
      assertNoConflictingPageLease(event.sender, id);
    }

    const count = state.db.transaction(() => {
      state.db.prepare('DELETE FROM pages WHERE trashed_by IS NOT NULL').run();
      for (const { id } of trashed) {
        state.pageHistories.delete(Number(id));
      }
      return trashed.length;
    })();

    clearWorkspaceHistory();
    if (count) notifyWorkspaceTreeChanged(event.sender);
    return count;
  });

  ipcMain.handle('page:favorite', (event, { id, value }) => {
    if (state.workspaceRestoreActive) {
      throw new Error('The workspace is being restored.');
    }
    const changed = state.db.prepare('UPDATE pages SET favorite=? WHERE id=?').run(value ? 1 : 0, id).changes;
    if (changed) notifyWorkspaceTreeChanged(event.sender);
    return changed;
  });

  ipcMain.handle('page:move', (event, { id, parentId = null, position }) => {
    id = Number(id);
    parentId = parentId === null ? null : Number(parentId);
    const roots = topLevelPageSelection([id]);
    assertPageTreeAvailable(event.sender, roots);
    if (parentId !== null) assertNoConflictingPageLease(event.sender, parentId);

    const result = state.db.transaction((transaction) => transferPageRoots(transaction, roots, parentId, position, false))(state.db);
    notifyWorkspaceTreeChanged(event.sender);
    return result.length > 0;
  });

  ipcMain.handle('page:reorder', (event, { source, target, after = false }) => {
    if (state.workspaceRestoreActive) {
      throw new Error('The workspace is being restored.');
    }
    source = Number(source);
    target = Number(target);
    if (source === target) return false;

    const from = one('SELECT id,parent_id FROM pages WHERE id=? AND trashed_by IS NULL', source);
    const to = one('SELECT id,parent_id FROM pages WHERE id=? AND trashed_by IS NULL', target);
    if (!from || !to || from.parent_id !== to.parent_id) {
      throw new Error('Pages can only be reordered with their siblings.');
    }

    state.db.transaction(() => {
      const siblings = rows('SELECT id FROM pages WHERE parent_id IS ? AND trashed_by IS NULL ORDER BY sort_order,id', from.parent_id).map((page) =>
        Number(page.id),
      );
      const sourceIndex = siblings.indexOf(source);
      const targetIndex = siblings.indexOf(target);
      if (sourceIndex < 0 || targetIndex < 0) {
        throw new Error('A page is no longer available.');
      }

      siblings.splice(sourceIndex, 1);
      const targetAfterRemoval = siblings.indexOf(target);
      siblings.splice(targetAfterRemoval + (after ? 1 : 0), 0, source);

      const updateStmt = state.db.prepare('UPDATE pages SET sort_order=? WHERE id=?');
      siblings.forEach((id, index) => updateStmt.run(index, id));
    })();

    notifyWorkspaceTreeChanged(event.sender);
    return true;
  });

  ipcMain.handle('page:rename', (event, { id, title }) => {
    const page = one('SELECT parent_id,icon,title AS old_title FROM pages WHERE id=?', id);
    if (!page) return 0;
    const parentIsDatabase = page.parent_id !== null && one('SELECT page_id FROM databases WHERE page_id=?', page.parent_id);
    assertPageWritable(event.sender, parentIsDatabase ? page.parent_id : id);

    if (page.old_title !== title) {
      recordPageUndo(id, 'Başlık düzenlendi', true);
    }

    const changes = state.db.prepare('UPDATE pages SET title=?,updated_at=unixepoch() WHERE id=?').run(title, id).changes;
    if (changes) {
      if (page.old_title !== title) {
        syncParentTitleReference(id, page.old_title, title, event.sender);
      }
      notifyWorkspaceChanged(event.sender, { id, title, icon: page.icon });
    }
    return changes;
  });

  ipcMain.handle('search', (_event, query) => {
    const trimmed = String(query || '').trim();
    if (!trimmed) return [];

    const words = (trimmed.match(/[\p{L}\p{N}]+/gu) || []).slice(0, 16);
    const hits = [];
    if (words.length) {
      try {
        const expr = words.map((word) => `"${word}"*`).join(' AND ');
        const pageHits = rows(
          "SELECT p.id,p.parent_id,p.title,p.icon,p.favorite,p.trashed_by,p.block_count,'Page title' AS preview,NULL AS position FROM page_search JOIN pages p ON p.id=page_search.rowid WHERE page_search MATCH ? AND p.trashed_by IS NULL LIMIT 50",
          expr,
        );
        hits.push(...pageHits.map((row) => ({ ...row, title: String(row.title || '').trim() || 'Untitled' })));

        const remaining = 50 - hits.length;
        if (remaining > 0) {
          const blockHits = rows(
            "SELECT p.id,p.parent_id,p.title,p.icon,p.favorite,p.trashed_by,p.block_count,snippet(block_search,0,'','',' … ',24) AS preview,b.position AS position FROM block_search JOIN blocks b ON b.id=block_search.rowid JOIN pages p ON p.id=b.page_id WHERE block_search MATCH ? AND p.trashed_by IS NULL LIMIT ?",
            expr,
            remaining,
          );
          hits.push(...blockHits.map((row) => ({ ...row, title: String(row.title || '').trim() || 'Untitled' })));
        }
        return hits;
      } catch {
        // Fall back to LIKE search when FTS5 query syntax rejects input.
      }
    }

    const like = `%${trimmed}%`;
    return rows(
      `SELECT p.id,p.parent_id,p.title,p.icon,p.favorite,p.trashed_by,p.block_count,
              CASE WHEN p.title LIKE ? THEN 'Page title'
                   ELSE substr(coalesce((SELECT b.content FROM blocks b WHERE b.page_id=p.id AND b.content LIKE ? ORDER BY b.position LIMIT 1),''),1,120)
              END AS preview,
              CASE WHEN p.title LIKE ? THEN NULL
                   ELSE (SELECT b.position FROM blocks b WHERE b.page_id=p.id AND b.content LIKE ? ORDER BY b.position LIMIT 1)
              END AS position
       FROM pages p
       WHERE p.trashed_by IS NULL
         AND (p.title LIKE ? OR EXISTS(SELECT 1 FROM blocks b WHERE b.page_id=p.id AND b.content LIKE ?))
       ORDER BY CASE WHEN p.title LIKE ? THEN 0 ELSE 1 END,p.updated_at DESC
       LIMIT 50`,
      like,
      like,
      like,
      like,
      like,
      like,
      like,
    ).map((row) => ({ ...row, title: String(row.title || '').trim() || 'Untitled' }));
  });
}

module.exports = {
  createDatabasePage,
  changeWorkspaceTrashState,
  clonePageTree,
  remapClonedReferences,
  registerPageIpc,
};
