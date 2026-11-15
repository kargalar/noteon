const { BrowserWindow } = require('electron');
const { state } = require('../state.cjs');

/**
 * Finds the parent BrowserWindow and pane ID for a dock WebContents sender.
 * @param {Electron.WebContents} sender
 */
function dockOwner(sender) {
  for (const win of BrowserWindow.getAllWindows()) {
    const views = state.dockViews.get(win.id);
    if (!views) continue;
    for (const [paneId, view] of views) {
      if (view.webContents === sender) return { win, paneId, view };
    }
  }
  return null;
}

/**
 * Broadcasts an IPC message to all open windows and dock WebContentsViews,
 * optionally excluding the originating sender.
 * @param {string} channel
 * @param {any} payload
 * @param {Electron.WebContents|null} [excludeSender=null]
 */
function broadcastToWorkspace(channel, payload, excludeSender = null) {
  const hasPayload = payload !== undefined;
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.webContents.isDestroyed() && win.webContents !== excludeSender) {
      if (hasPayload) win.webContents.send(channel, payload);
      else win.webContents.send(channel);
    }
    for (const view of state.dockViews.get(win.id)?.values() || []) {
      if (!view.webContents.isDestroyed() && view.webContents !== excludeSender) {
        if (hasPayload) view.webContents.send(channel, payload);
        else view.webContents.send(channel);
      }
    }
  }
}

function notifyWorkspaceChanged(sender, page) {
  broadcastToWorkspace(
    'workspace:page-updated',
    {
      id: Number(page.id),
      title: String(page.title || ''),
      icon: String(page.icon || ''),
    },
    sender,
  );
}

function notifyWorkspacePageRestored(sender, pageId) {
  broadcastToWorkspace('workspace:page-restored', { pageId: Number(pageId) }, sender);
}

function notifyWorkspacePagesTrashed(sender, pageIds) {
  broadcastToWorkspace('workspace:pages-trashed', { pageIds: pageIds.map(Number) }, sender);
}

function notifyWorkspaceTreeChanged(sender) {
  broadcastToWorkspace('workspace:tree-changed', undefined, sender);
}

function notifyWorkspaceHistoryChanged() {
  broadcastToWorkspace('workspace:history-changed', {
    canUndo: state.workspaceUndoStack.length > 0,
    canRedo: state.workspaceRedoStack.length > 0,
  });
}

function notifyPageLeaseReleased(sender, pageId) {
  broadcastToWorkspace('page:lease-released', { pageId: Number(pageId) }, sender);
}

module.exports = {
  dockOwner,
  broadcastToWorkspace,
  notifyWorkspaceChanged,
  notifyWorkspacePageRestored,
  notifyWorkspacePagesTrashed,
  notifyWorkspaceTreeChanged,
  notifyWorkspaceHistoryChanged,
  notifyPageLeaseReleased,
};
