const { BrowserWindow } = require('electron');
const { state, rows, one } = require('../state.cjs');
const { dockOwner, notifyPageLeaseReleased } = require('./notifications.cjs');

function workspaceOwnerWindow(sender) {
  return dockOwner(sender)?.win || BrowserWindow.fromWebContents(sender);
}

function releasePageLeases(sender) {
  for (const [pageId, owner] of state.pageLeases) {
    if (owner === sender) {
      state.pageLeases.delete(pageId);
      notifyPageLeaseReleased(sender, pageId);
    }
  }
}

function effectivePageLeaseId(pageId) {
  pageId = Number(pageId);
  const page = one('SELECT parent_id FROM pages WHERE id=?', pageId);
  if (
    page?.parent_id !== null &&
    page?.parent_id !== undefined &&
    one('SELECT page_id FROM databases WHERE page_id=?', page.parent_id)
  ) {
    return Number(page.parent_id);
  }
  return pageId;
}

function checkPageLeaseOwner(sender, pageId, conflictMessage) {
  if (state.workspaceRestoreActive) {
    throw new Error('The workspace is being restored.');
  }
  const leaseId = effectivePageLeaseId(pageId);
  const owner = state.pageLeases.get(leaseId);
  if (owner?.isDestroyed()) {
    state.pageLeases.delete(leaseId);
    return true;
  }
  if (owner && owner !== sender) {
    throw new Error(conflictMessage);
  }
  return true;
}

function assertPageWritable(sender, pageId) {
  return checkPageLeaseOwner(
    sender,
    pageId,
    'This page is open for editing in another pane. Its changes were protected.',
  );
}

function assertNoConflictingPageLease(sender, pageId) {
  return checkPageLeaseOwner(sender, pageId, 'A page in this operation is open for editing in another pane.');
}

function assertWorkspaceHistoryPageLease(sender, pageId) {
  if (state.workspaceRestoreActive) {
    throw new Error('The workspace is being restored.');
  }
  const leaseId = effectivePageLeaseId(pageId);
  const owner = state.pageLeases.get(leaseId);
  if (owner?.isDestroyed()) {
    state.pageLeases.delete(leaseId);
    return true;
  }
  if (!owner || owner === sender) return true;
  const senderWindow = workspaceOwnerWindow(sender);
  const ownerWindow = workspaceOwnerWindow(owner);
  if (!senderWindow || ownerWindow !== senderWindow) {
    throw new Error('Undo is blocked because another workspace window is editing this page.');
  }
  return true;
}

function assertDatabaseWritable(sender, databaseId) {
  const database = one('SELECT page_id FROM databases WHERE page_id=?', databaseId);
  if (!database) throw new Error('Database not found.');
  return assertPageWritable(sender, database.page_id);
}

function assertPropertyWritable(sender, propertyId) {
  const property = one('SELECT database_page_id FROM database_properties WHERE id=?', propertyId);
  if (!property) throw new Error('Database property not found.');
  return assertDatabaseWritable(sender, property.database_page_id);
}

function assertPageTreeAvailable(sender, roots) {
  for (const root of roots) {
    const tree = rows(
      'WITH RECURSIVE tree(id) AS (SELECT id FROM pages WHERE id=? UNION ALL SELECT p.id FROM pages p JOIN tree t ON p.parent_id=t.id) SELECT id FROM tree',
      root.id,
    );
    for (const page of tree) {
      assertNoConflictingPageLease(sender, page.id);
    }
  }
}

function registerLeaseIpc(ipcMain) {
  ipcMain.handle('page:lease:acquire', (event, pageId) => {
    pageId = Number(pageId);
    if (!Number.isSafeInteger(pageId) || !one('SELECT id FROM pages WHERE id=?', pageId)) return false;
    const leaseId = effectivePageLeaseId(pageId);
    const owner = state.pageLeases.get(leaseId);
    if (owner === event.sender) return true;
    if (owner && !owner.isDestroyed()) return false;
    state.pageLeases.set(leaseId, event.sender);
    return true;
  });

  ipcMain.handle('page:lease:release', (event, pageId) => {
    const leaseId = effectivePageLeaseId(pageId);
    if (state.pageLeases.get(leaseId) !== event.sender) return false;
    state.pageLeases.delete(leaseId);
    notifyPageLeaseReleased(event.sender, leaseId);
    return true;
  });
}

module.exports = {
  workspaceOwnerWindow,
  releasePageLeases,
  effectivePageLeaseId,
  assertPageWritable,
  assertNoConflictingPageLease,
  assertWorkspaceHistoryPageLease,
  assertDatabaseWritable,
  assertPropertyWritable,
  assertPageTreeAvailable,
  registerLeaseIpc,
};
