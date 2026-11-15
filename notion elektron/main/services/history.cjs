const { state, rows, one } = require('../state.cjs');
const { assertPageWritable, assertWorkspaceHistoryPageLease } = require('./leases.cjs');
const {
  notifyWorkspaceChanged,
  notifyWorkspacePageRestored,
  notifyWorkspacePagesTrashed,
  notifyWorkspaceTreeChanged,
  notifyWorkspaceHistoryChanged,
} = require('./notifications.cjs');

function pushHistorySnapshot(stack, entry) {
  stack.push(entry);
  if (stack.length > 10) stack.shift();
}

function pushWorkspaceUndo(entry) {
  state.workspaceUndoStack.push(entry);
  if (state.workspaceUndoStack.length > 50) state.workspaceUndoStack.shift();
  state.workspaceRedoStack.length = 0;
  notifyWorkspaceHistoryChanged();
}

function pruneWorkspaceHistory(ids) {
  const removed = new Set(ids.map(Number));
  for (const stack of [state.workspaceUndoStack, state.workspaceRedoStack]) {
    for (let index = stack.length - 1; index >= 0; index--) {
      const entry = stack[index];
      const matches =
        entry.pageId !== undefined
          ? removed.has(Number(entry.pageId))
          : entry.pageIds?.some((id) => removed.has(Number(id)));
      if (matches) stack.splice(index, 1);
    }
  }
  notifyWorkspaceHistoryChanged();
}

function clearWorkspaceHistory() {
  state.workspaceUndoStack.length = 0;
  state.workspaceRedoStack.length = 0;
  notifyWorkspaceHistoryChanged();
}

function snapshotPage(pageId) {
  const page = one(
    'SELECT id,title,icon,cover,cover_image,cover_x,cover_y,full_width,priority FROM pages WHERE id=?',
    pageId,
  );
  if (!page) return null;
  return {
    pageId: Number(page.id),
    title: page.title || '',
    icon: page.icon || '',
    cover: page.cover || 0,
    cover_image: page.cover_image || null,
    cover_x: page.cover_x ?? 0.5,
    cover_y: page.cover_y ?? 0.5,
    full_width: page.full_width || 0,
    priority: page.priority || 0,
    customProperties: rows(
      'SELECT id,name,kind,options,position FROM custom_page_properties WHERE page_id=? ORDER BY position,id',
      pageId,
    ).map((row) => ({
      id: Number(row.id),
      name: String(row.name),
      kind: String(row.kind),
      options: String(row.options || ''),
      position: Number(row.position) || 0,
    })),
    customPropertyValues: rows(
      'SELECT property_id,value FROM custom_page_property_values WHERE page_id=? ORDER BY property_id',
      pageId,
    ).map((row) => ({
      propertyId: Number(row.property_id),
      value: String(row.value),
    })),
    blocks: rows(
      'SELECT id,page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles FROM blocks WHERE page_id=? ORDER BY position,id',
      pageId,
    ),
  };
}

function recordPageUndo(pageId, description, discrete = false) {
  const snapshot = snapshotPage(pageId);
  if (!snapshot) return false;
  pageId = Number(pageId);
  let history = state.pageHistories.get(pageId);
  if (!history) {
    history = { undo: [], redo: [], nextId: 0, lastSaveAt: 0 };
    state.pageHistories.set(pageId, history);
  }
  const now = Date.now();
  if (!discrete && history.lastSaveAt && now - history.lastSaveAt < 1500) {
    history.lastSaveAt = now;
    return false;
  }
  if (history.undo.length && JSON.stringify(history.undo.at(-1).snapshot) === JSON.stringify(snapshot)) {
    return false;
  }
  history.lastSaveAt = discrete ? 0 : now;
  history.nextId++;
  const historyEntryId = history.nextId;
  history.undo.push({ id: historyEntryId, description, timestamp: now, snapshot });
  if (history.undo.length > 10) history.undo.shift();
  history.redo = [];
  pushWorkspaceUndo({ type: 'page', pageId, historyEntryId });
  return true;
}

function pageHistorySummary(pageId) {
  const history = state.pageHistories.get(Number(pageId));
  return {
    undo:
      history?.undo
        .slice()
        .reverse()
        .map(({ id, description, timestamp }) => ({ id, description, timestamp })) || [],
    canUndo: Boolean(history?.undo.length),
    canRedo: Boolean(history?.redo.length),
  };
}

function restorePageSnapshot(snapshot) {
  return state.db.transaction(() => {
    state.db
      .prepare(
        'UPDATE pages SET title=?,icon=?,cover=?,cover_image=?,cover_x=?,cover_y=?,full_width=?,priority=?,updated_at=unixepoch() WHERE id=?',
      )
      .run(
        snapshot.title,
        snapshot.icon,
        snapshot.cover || 0,
        snapshot.cover_image || null,
        snapshot.cover_x ?? 0.5,
        snapshot.cover_y ?? 0.5,
        snapshot.full_width ? 1 : 0,
        snapshot.priority || 0,
        snapshot.pageId,
      );
    state.db.prepare('DELETE FROM blocks WHERE page_id=?').run(snapshot.pageId);

    const insert = state.db.prepare(
      'INSERT INTO blocks(id,page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles) VALUES(?,?,?,?,?,?,?,?,?,?,?)',
    );
    for (const [position, block] of snapshot.blocks.entries()) {
      insert.run(
        block.id,
        snapshot.pageId,
        position,
        block.kind,
        block.content,
        block.checked ? 1 : 0,
        block.column_group,
        block.layout_column || 0,
        block.column_width || 1,
        block.media_width || 1,
        block.text_styles || '[]',
      );
    }

    if (Array.isArray(snapshot.customProperties)) {
      const keepIds = new Set(snapshot.customProperties.map((p) => Number(p.id)));
      for (const existing of rows('SELECT id FROM custom_page_properties WHERE page_id=?', snapshot.pageId)) {
        if (!keepIds.has(Number(existing.id))) {
          state.db
            .prepare('DELETE FROM custom_page_properties WHERE page_id=? AND id=?')
            .run(snapshot.pageId, existing.id);
        }
      }
      for (const prop of snapshot.customProperties) {
        const found = one(
          'SELECT id FROM custom_page_properties WHERE page_id=? AND id=?',
          snapshot.pageId,
          prop.id,
        );
        if (found) {
          state.db
            .prepare(
              'UPDATE custom_page_properties SET name=?,kind=?,options=?,position=? WHERE id=? AND page_id=?',
            )
            .run(prop.name, prop.kind, prop.options || '', prop.position || 0, prop.id, snapshot.pageId);
        } else {
          state.db
            .prepare(
              'INSERT INTO custom_page_properties(id,page_id,name,kind,options,position) VALUES(?,?,?,?,?,?)',
            )
            .run(prop.id, snapshot.pageId, prop.name, prop.kind, prop.options || '', prop.position || 0);
        }
      }
    }

    state.db.prepare('DELETE FROM custom_page_property_values WHERE page_id=?').run(snapshot.pageId);
    for (const property of snapshot.customPropertyValues || []) {
      if (
        one(
          'SELECT 1 FROM custom_page_properties WHERE page_id=? AND id=?',
          snapshot.pageId,
          property.propertyId,
        )
      ) {
        state.db
          .prepare('INSERT INTO custom_page_property_values(page_id,property_id,value) VALUES(?,?,?)')
          .run(snapshot.pageId, property.propertyId, property.value);
      }
    }
    return one('SELECT title,icon FROM pages WHERE id=?', snapshot.pageId);
  })();
}

async function flushWorkspaceHistoryViews(sender) {
  const targets = [...new Set(state.pageLeases.values())].filter(
    (owner) => owner !== sender && !owner.isDestroyed(),
  );
  for (const target of targets) {
    const requestId = `history:${Date.now()}:${Math.random()}`;
    const flushed = await new Promise((resolve) => {
      const timer = setTimeout(() => {
        state.pendingWorkspaceHistoryFlushes.delete(requestId);
        resolve(false);
      }, 12000);
      state.pendingWorkspaceHistoryFlushes.set(requestId, {
        sender: target,
        finish: (ok) => {
          clearTimeout(timer);
          state.pendingWorkspaceHistoryFlushes.delete(requestId);
          resolve(ok);
        },
      });
      target.send('workspace:prepare-history-flush', { requestId });
    });
    if (!flushed) {
      throw new Error('Another editor could not save its changes before workspace undo.');
    }
  }
}

function stepPageHistory(sender, pageId, direction) {
  assertPageWritable(sender, pageId);
  const history = state.pageHistories.get(Number(pageId));
  const isUndo = direction === 'undo';
  const sourceStack = isUndo ? history?.undo : history?.redo;
  const targetStack = isUndo ? history?.redo : history?.undo;
  const workspaceSource = isUndo ? state.workspaceUndoStack : state.workspaceRedoStack;
  const workspaceTarget = isUndo ? state.workspaceRedoStack : state.workspaceUndoStack;

  if (!sourceStack?.length) return false;
  const entry = sourceStack.at(-1);
  const current = snapshotPage(pageId);
  const page = restorePageSnapshot(entry.snapshot);
  sourceStack.pop();

  const workspaceIndex = workspaceSource.findLastIndex(
    (item) =>
      item.type === 'page' &&
      Number(item.pageId) === Number(pageId) &&
      item.historyEntryId === entry.id,
  );
  if (workspaceIndex >= 0) workspaceSource.splice(workspaceIndex, 1);

  if (current) {
    history.nextId++;
    pushHistorySnapshot(targetStack, {
      id: history.nextId,
      description: entry.description,
      timestamp: Date.now(),
      snapshot: current,
    });
    workspaceTarget.push({
      type: 'page',
      pageId: Number(pageId),
      historyEntryId: history.nextId,
    });
    if (workspaceTarget.length > 50) workspaceTarget.shift();
  }

  history.lastSaveAt = 0;
  notifyWorkspaceHistoryChanged();
  notifyWorkspaceChanged(sender, { id: pageId, title: page?.title, icon: page?.icon });
  return true;
}

async function stepWorkspaceHistory(sender, direction, changeWorkspaceTrashState) {
  if (state.workspaceRestoreActive) {
    throw new Error('The workspace is being restored.');
  }
  await flushWorkspaceHistoryViews(sender);

  const isUndo = direction === 'undo';
  const sourceStack = isUndo ? state.workspaceUndoStack : state.workspaceRedoStack;
  const targetStack = isUndo ? state.workspaceRedoStack : state.workspaceUndoStack;

  while (sourceStack.length) {
    const entry = sourceStack.at(-1);
    if (entry.type === 'page') {
      const history = state.pageHistories.get(Number(entry.pageId));
      const pageSource = isUndo ? history?.undo : history?.redo;
      const pageTarget = isUndo ? history?.redo : history?.undo;
      if (!pageSource?.length || pageSource.at(-1).id !== entry.historyEntryId) {
        sourceStack.pop();
        continue;
      }
      assertWorkspaceHistoryPageLease(sender, entry.pageId);
      const stepEntry = pageSource.at(-1);
      const current = snapshotPage(entry.pageId);
      if (!current) {
        sourceStack.pop();
        continue;
      }
      const page = restorePageSnapshot(stepEntry.snapshot);
      pageSource.pop();
      history.nextId++;
      const historyEntryId = history.nextId;
      pushHistorySnapshot(pageTarget, {
        id: historyEntryId,
        description: stepEntry.description,
        timestamp: Date.now(),
        snapshot: current,
      });
      history.lastSaveAt = 0;
      sourceStack.pop();
      targetStack.push({ ...entry, historyEntryId });
      if (targetStack.length > 50) targetStack.shift();

      notifyWorkspaceHistoryChanged();
      notifyWorkspaceChanged(sender, { id: entry.pageId, title: page?.title, icon: page?.icon });
      notifyWorkspacePageRestored(sender, entry.pageId);
      return { type: 'page', pageId: Number(entry.pageId) };
    }

    const restore = isUndo
      ? entry.type === 'trash'
      : entry.type === 'restore' || entry.type === 'create-record';
    const ids = changeWorkspaceTrashState(sender, entry.pageIds, restore);
    sourceStack.pop();
    if (!ids.length) continue;

    targetStack.push({ ...entry, pageIds: ids });
    if (targetStack.length > 50) targetStack.shift();

    notifyWorkspaceHistoryChanged();
    notifyWorkspaceTreeChanged(sender);
    if (isUndo ? entry.type !== 'trash' : entry.type === 'trash') {
      notifyWorkspacePagesTrashed(sender, ids);
    }
    return { type: entry.type, pageIds: ids };
  }

  notifyWorkspaceHistoryChanged();
  return null;
}

function registerHistoryIpc(ipcMain, { changeWorkspaceTrashState }) {
  ipcMain.handle('workspace:history:get', () => ({
    canUndo: state.workspaceUndoStack.length > 0,
    canRedo: state.workspaceRedoStack.length > 0,
  }));

  ipcMain.handle('page:history:get', (_event, pageId) => pageHistorySummary(pageId));

  ipcMain.handle('page:history:undo', (event, pageId) => stepPageHistory(event.sender, pageId, 'undo'));

  ipcMain.handle('page:history:redo', (event, pageId) => stepPageHistory(event.sender, pageId, 'redo'));

  ipcMain.handle('page:history:restore', (event, { pageId, entryId }) => {
    assertPageWritable(event.sender, pageId);
    const history = state.pageHistories.get(Number(pageId));
    const index = history?.undo.findIndex((entry) => entry.id === Number(entryId)) ?? -1;
    if (index < 0) return false;
    const entry = history.undo[index];
    const current = snapshotPage(pageId);
    const page = restorePageSnapshot(entry.snapshot);
    if (current) {
      history.nextId++;
      pushHistorySnapshot(history.redo, {
        id: history.nextId,
        description: 'Restored version',
        timestamp: Date.now(),
        snapshot: current,
      });
      state.workspaceRedoStack.push({
        type: 'page',
        pageId: Number(pageId),
        historyEntryId: history.nextId,
      });
      if (state.workspaceRedoStack.length > 50) state.workspaceRedoStack.shift();
    }
    history.lastSaveAt = 0;
    notifyWorkspaceHistoryChanged();
    notifyWorkspaceChanged(event.sender, { id: pageId, title: page?.title, icon: page?.icon });
    return true;
  });

  ipcMain.handle('workspace:history:undo', (event) =>
    stepWorkspaceHistory(event.sender, 'undo', changeWorkspaceTrashState),
  );

  ipcMain.handle('workspace:history:redo', (event) =>
    stepWorkspaceHistory(event.sender, 'redo', changeWorkspaceTrashState),
  );

  ipcMain.on('workspace:history-flush-ready', (event, { requestId, ok }) => {
    const pending = state.pendingWorkspaceHistoryFlushes.get(requestId);
    if (pending?.sender === event.sender) pending.finish(ok === true);
  });
}

module.exports = {
  pushHistorySnapshot,
  pushWorkspaceUndo,
  pruneWorkspaceHistory,
  clearWorkspaceHistory,
  snapshotPage,
  recordPageUndo,
  pageHistorySummary,
  restorePageSnapshot,
  flushWorkspaceHistoryViews,
  registerHistoryIpc,
};
