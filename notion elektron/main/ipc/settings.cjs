const { state, rows } = require('../state.cjs');
const { broadcastToWorkspace } = require('../services/notifications.cjs');

function registerSettingsIpc(ipcMain) {
  ipcMain.handle('settings:get', () =>
    Object.fromEntries(rows('SELECT key,value FROM settings').map((x) => [x.key, x.value])),
  );

  ipcMain.handle('settings:set', (event, { key, value }) => {
    if (state.workspaceRestoreActive) throw new Error('The workspace is being restored.');
    const str = String(value);
    const changes = state.db
      .prepare('INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value')
      .run(key, str).changes;
    broadcastToWorkspace('settings:changed', { key, value: str }, event.sender);
    return changes;
  });
}

module.exports = {
  registerSettingsIpc,
};
