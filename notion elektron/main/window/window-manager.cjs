const path = require('node:path');
const { BrowserWindow, ipcMain } = require('electron');
const { state } = require('../state.cjs');
const { releasePageLeases } = require('../services/leases.cjs');

/**
 * Opens a workspace BrowserWindow, optionally focused on a specific page.
 * @param {number} [pageId]
 * @param {boolean} [secondary=false]
 */
function createWindow(pageId, secondary = false) {
  const win = new BrowserWindow({
    width: 1440,
    height: 940,
    minWidth: 900,
    minHeight: 600,
    backgroundColor: '#f7f7f5',
    title: 'Notion Elektron',
    webPreferences: {
      preload: path.join(state.projectDir, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  win.webContents.once('destroyed', () => releasePageLeases(win.webContents));

  let safeToClose = false;
  win.on('close', (event) => {
    if (!safeToClose && !win.webContents.isDestroyed()) {
      event.preventDefault();
      win.webContents.send('window:prepare-close');
    }
  });

  const closeReady = (event) => {
    if (event.sender !== win.webContents) return;
    ipcMain.removeListener('window:close-ready', closeReady);
    safeToClose = true;
    win.close();
  };
  ipcMain.on('window:close-ready', closeReady);

  win.on('closed', () => {
    ipcMain.removeListener('window:close-ready', closeReady);
    for (const view of state.dockViews.get(win.id)?.values() || []) {
      if (!view.webContents.isDestroyed()) view.webContents.close();
    }
    state.dockViews.delete(win.id);
  });

  const query = { window: secondary ? String(win.id) : 'main' };
  if (pageId) query.page = String(pageId);
  win.loadFile(path.join(state.projectDir, 'renderer', 'index.html'), { query });
}

function registerWindowIpc(ipcMainInstance) {
  ipcMainInstance.handle('window:open-page', (_event, pageId) => createWindow(Number(pageId), true));
}

module.exports = {
  createWindow,
  registerWindowIpc,
};
