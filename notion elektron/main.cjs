/**
 * Electron main-process entrypoint for Notion Elektron.
 * Initializes the workspace database, registers modular IPC handlers, and manages app lifecycle.
 */

const { app, BrowserWindow, ipcMain } = require('electron');
const { state } = require('./main/state.cjs');
const { setupWorkspace } = require('./main/database/workspace.cjs');
const { registerAllIpc } = require('./main/ipc/index.cjs');
const { createWindow } = require('./main/window/window-manager.cjs');
const { killAssistantChild } = require('./main/services/assistant.cjs');

registerAllIpc(ipcMain);

app.whenReady().then(() => {
  setupWorkspace();
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('will-quit', () => {
  state.shuttingDown = true;
  killAssistantChild();
  if (state.db) {
    state.db.pragma('wal_checkpoint(TRUNCATE)');
    state.db.close();
    state.db = null;
  }
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
