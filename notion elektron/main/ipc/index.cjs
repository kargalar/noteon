/**
 * Central IPC registration wiring every domain handler module to Electron's ipcMain.
 */

const { registerWorkspaceIpc } = require('../database/workspace.cjs');
const { registerLeaseIpc } = require('../services/leases.cjs');
const { registerHistoryIpc } = require('../services/history.cjs');
const { registerAssistantIpc } = require('../services/assistant.cjs');
const { registerWindowIpc } = require('../window/window-manager.cjs');
const { registerDockIpc } = require('../window/dock-manager.cjs');
const { changeWorkspaceTrashState, clonePageTree, remapClonedReferences, registerPageIpc } = require('./pages.cjs');
const { registerBlockIpc } = require('./blocks.cjs');
const { registerDatabaseIpc } = require('./databases.cjs');
const { registerCustomPropertyIpc } = require('./custom-properties.cjs');
const { registerMediaAndLinksIpc } = require('./media-and-links.cjs');
const { registerSettingsIpc } = require('./settings.cjs');

/**
 * Register all main-process IPC handlers on the provided `ipcMain` instance.
 * @param {import('electron').IpcMain} ipcMain
 */
function registerAllIpc(ipcMain) {
  registerWorkspaceIpc(ipcMain);
  registerLeaseIpc(ipcMain);
  registerHistoryIpc(ipcMain, { changeWorkspaceTrashState });
  registerPageIpc(ipcMain);
  registerBlockIpc(ipcMain, { clonePageTree, remapClonedReferences });
  registerDatabaseIpc(ipcMain);
  registerCustomPropertyIpc(ipcMain);
  registerMediaAndLinksIpc(ipcMain);
  registerSettingsIpc(ipcMain);
  registerWindowIpc(ipcMain);
  registerDockIpc(ipcMain);
  registerAssistantIpc(ipcMain);
}

module.exports = { registerAllIpc };
