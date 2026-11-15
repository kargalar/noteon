const path = require('node:path');
const { BrowserWindow, WebContentsView } = require('electron');
const { state } = require('../state.cjs');
const { dockOwner } = require('../services/notifications.cjs');
const { releasePageLeases } = require('../services/leases.cjs');

function viewsFor(win) {
  let views = state.dockViews.get(win.id);
  if (!views) {
    views = new Map();
    state.dockViews.set(win.id, views);
  }
  return views;
}

function paneBounds(value) {
  const x = Math.max(0, Math.floor(Number(value?.x) || 0));
  const y = Math.max(0, Math.floor(Number(value?.y) || 0));
  const width = Math.max(1, Math.floor(Number(value?.width) || 1));
  const height = Math.max(1, Math.floor(Number(value?.height) || 1));
  return { x, y, width, height };
}

function preparePaneClose(win, paneId, closing = false) {
  const view = state.dockViews.get(win.id)?.get(paneId);
  if (!view || view.webContents.isDestroyed()) return Promise.resolve(true);

  const requestId = `${win.id}:${paneId}:${Date.now()}:${Math.random()}`;
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      state.pendingPaneClosures.delete(requestId);
      resolve(false);
    }, 12000);
    state.pendingPaneClosures.set(requestId, {
      sender: view.webContents,
      finish: (ok) => {
        clearTimeout(timer);
        state.pendingPaneClosures.delete(requestId);
        resolve(ok);
      },
    });
    view.webContents.send('pane:prepare-close', { requestId, closing });
  });
}

function requestDockTabChange(view, channel, payload) {
  if (!view || view.webContents.isDestroyed()) {
    return Promise.reject(new Error('Target pane is no longer available.'));
  }
  const requestId = `tab:${Date.now()}:${Math.random()}`;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      state.pendingDockTabChanges.delete(requestId);
      reject(new Error('The tab could not be moved because its pane did not respond.'));
    }, 12000);
    state.pendingDockTabChanges.set(requestId, {
      sender: view.webContents,
      finish: (ok, error) => {
        clearTimeout(timer);
        state.pendingDockTabChanges.delete(requestId);
        if (ok) resolve(true);
        else reject(new Error(error || 'The tab change could not be saved.'));
      },
    });
    view.webContents.send(channel, { ...payload, requestId });
  });
}

function registerDockIpc(ipcMain) {
  ipcMain.handle('dock:ensure-view', async (event, { paneId, pageId, bounds, tabs = [], active = 0, focused = false }) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents || !paneId || paneId === 'primary') {
      throw new Error('Invalid dock pane request.');
    }
    const views = viewsFor(win);
    let view = views.get(paneId);
    if (!view) {
      view = new WebContentsView({
        webPreferences: {
          preload: path.join(state.projectDir, 'preload.cjs'),
          contextIsolation: true,
          nodeIntegration: false,
        },
      });
      views.set(paneId, view);
      win.contentView.addChildView(view);
      view.webContents.once('destroyed', () => releasePageLeases(view.webContents));
      view.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
      view.webContents.on('will-navigate', (navigationEvent, url) => {
        if (!url.startsWith('file://')) navigationEvent.preventDefault();
      });
      view.webContents.on('render-process-gone', () => {
        releasePageLeases(view.webContents);
        if (views.get(paneId) === view) {
          if (!win.isDestroyed()) win.contentView.removeChildView(view);
          views.delete(paneId);
        }
        if (!win.isDestroyed()) win.webContents.send('dock:pane-gone', { paneId });
      });

      const query = {
        embedded: 'true',
        pane: paneId,
        page: String(Number(pageId) || ''),
        tabs: JSON.stringify(tabs),
        active: String(Math.max(0, Number(active) || 0)),
        focused: String(Boolean(focused)),
      };
      try {
        await view.webContents.loadFile(path.join(state.projectDir, 'renderer', 'index.html'), { query });
      } catch (error) {
        if (views.get(paneId) === view) {
          if (!win.isDestroyed()) win.contentView.removeChildView(view);
          views.delete(paneId);
        }
        if (!view.webContents.isDestroyed()) view.webContents.close();
        throw error;
      }
    }
    view.setBounds(paneBounds(bounds));
    view.setVisible(true);
    return true;
  });

  ipcMain.handle('dock:set-bounds', (event, entries) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents || !Array.isArray(entries)) return false;
    const views = viewsFor(win);
    const visible = new Set();
    for (const entry of entries) {
      const view = views.get(entry.paneId);
      if (!view) continue;
      visible.add(entry.paneId);
      view.setBounds(paneBounds(entry.bounds));
      view.setVisible(true);
    }
    for (const [id, view] of views) {
      if (!visible.has(id)) view.setVisible(false);
    }
    return true;
  });

  ipcMain.on('pane:close-ready', (event, { requestId, ok }) => {
    const pending = state.pendingPaneClosures.get(requestId);
    if (pending?.sender === event.sender) pending.finish(ok === true);
  });

  ipcMain.handle('dock:remove-view', async (event, paneId) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents) return false;
    const views = state.dockViews.get(win.id);
    const view = views?.get(paneId);
    if (!view) return true;
    if (!(await preparePaneClose(win, paneId, true))) {
      throw new Error(`Pane ${paneId} could not save its pending changes.`);
    }
    win.contentView.removeChildView(view);
    views.delete(paneId);
    view.webContents.close();
    return true;
  });

  ipcMain.handle('dock:tab:remove', async (event, { paneId, pageId }) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents) {
      throw new Error('Only the workspace window can move dock tabs.');
    }
    const view = state.dockViews.get(win.id)?.get(paneId);
    if (!view) throw new Error('Source pane is no longer available.');
    return requestDockTabChange(view, 'pane:tab-remove', { pageId: Number(pageId) });
  });

  ipcMain.handle('dock:tab:insert', async (event, { paneId, pageId, index }) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents) {
      throw new Error('Only the workspace window can move dock tabs.');
    }
    const view = state.dockViews.get(win.id)?.get(paneId);
    if (!view) throw new Error('Target pane is no longer available.');
    return requestDockTabChange(view, 'pane:tab-insert', { pageId: Number(pageId), index: Number(index) });
  });

  ipcMain.on('dock:tab-change-ready', (event, { requestId, ok, error }) => {
    const pending = state.pendingDockTabChanges.get(requestId);
    if (pending?.sender === event.sender) pending.finish(ok === true, error);
  });

  ipcMain.handle('dock:flush-views', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents) return false;
    for (const paneId of state.dockViews.get(win.id)?.keys() || []) {
      if (!(await preparePaneClose(win, paneId))) {
        throw new Error(`Pane ${paneId} could not save its pending changes.`);
      }
    }
    return true;
  });

  ipcMain.handle('dock:close-views', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win || event.sender !== win.webContents) return false;
    const views = state.dockViews.get(win.id);
    if (!views) return true;
    for (const paneId of [...views.keys()]) {
      if (!(await preparePaneClose(win, paneId, true))) {
        throw new Error(`Pane ${paneId} could not save its pending changes.`);
      }
      const view = views.get(paneId);
      if (view) {
        win.contentView.removeChildView(view);
        views.delete(paneId);
        view.webContents.close();
      }
    }
    return true;
  });

  ipcMain.on('dock:pane-state', (event, payload) => {
    const owner = dockOwner(event.sender);
    if (owner && !owner.win.webContents.isDestroyed()) {
      owner.win.webContents.send('dock:pane-state', { paneId: owner.paneId, ...payload });
    }
  });

  ipcMain.on('dock:tab-move-request', (event, payload) => {
    const owner = dockOwner(event.sender);
    if (owner && !owner.win.webContents.isDestroyed()) {
      owner.win.webContents.send('dock:tab-move-request', { ...payload, fromPaneId: owner.paneId });
    }
  });

  ipcMain.on('dock:pane-focus', (event) => {
    const owner = dockOwner(event.sender);
    if (owner && !owner.win.webContents.isDestroyed()) {
      owner.win.webContents.send('dock:pane-focus', { paneId: owner.paneId });
    }
  });

  ipcMain.on('dock:navigate-pane', (event, { paneId, pageId, newTab = false, historyTarget }) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    const view = win && state.dockViews.get(win.id)?.get(paneId);
    if (event.sender === win?.webContents && view) {
      view.webContents.send('pane:navigate', { pageId, newTab, historyTarget });
    }
  });
}

module.exports = {
  viewsFor,
  paneBounds,
  preparePaneClose,
  requestDockTabChange,
  registerDockIpc,
};
