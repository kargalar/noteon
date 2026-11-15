const { contextBridge, ipcRenderer, webUtils } = require('electron');

/**
 * Creates a typed IPC invoke wrapper for a given channel.
 * @param {string} channel
 */
const invoke = (channel) => (payload) => ipcRenderer.invoke(channel, payload);

/**
 * Creates an IPC event subscription helper that returns an unsubscribe function.
 * @param {string} channel
 * @param {boolean} passPayload
 */
function onEvent(channel, passPayload = true) {
  return (callback) => {
    const handler = (_event, payload) => (passPayload ? callback(payload) : callback());
    ipcRenderer.on(channel, handler);
    return () => ipcRenderer.removeListener(channel, handler);
  };
}

contextBridge.exposeInMainWorld('noteon', {
  // Workspace & History
  summary: () => ipcRenderer.invoke('workspace:summary'),
  getWorkspaceHistory: () => ipcRenderer.invoke('workspace:history:get'),
  undoWorkspace: () => ipcRenderer.invoke('workspace:history:undo'),
  redoWorkspace: () => ipcRenderer.invoke('workspace:history:redo'),
  backup: () => ipcRenderer.invoke('workspace:backup'),
  restoreWorkspace: () => ipcRenderer.invoke('workspace:restore'),
  importNoteonWorkspace: () => ipcRenderer.invoke('workspace:import-noteon'),
  search: invoke('search'),

  // Pages, Leases & Page History
  getPage: invoke('page:get'),
  savePage: invoke('page:save'),
  acquirePageLease: invoke('page:lease:acquire'),
  releasePageLease: invoke('page:lease:release'),
  getPageHistory: invoke('page:history:get'),
  undoPage: invoke('page:history:undo'),
  redoPage: invoke('page:history:redo'),
  restorePageHistory: invoke('page:history:restore'),
  createPage: invoke('page:create'),
  duplicatePage: invoke('page:duplicate'),
  createLinked: invoke('page:create-linked'),
  movePage: invoke('page:move'),
  reorderPage: invoke('page:reorder'),
  transferPages: invoke('page:transfer'),
  copyPages: invoke('page:clipboard:copy'),
  pastePages: invoke('page:clipboard:paste'),
  renamePage: invoke('page:rename'),
  trashPage: invoke('page:trash'),
  trashPages: invoke('page:trash-many'),
  emptyTrash: () => ipcRenderer.invoke('page:empty-trash'),
  restorePage: invoke('page:restore'),
  restorePages: invoke('page:restore-many'),
  deletePagePermanently: invoke('page:delete-permanently'),
  deletePagesPermanently: invoke('page:delete-permanently-many'),
  favorite: invoke('page:favorite'),

  // Blocks
  saveBlock: invoke('block:save'),
  setBlocksChecked: invoke('block:set-checked'),
  formatBlocks: invoke('block:format-many'),
  splitBlock: invoke('block:split'),
  mergeBlocks: invoke('block:merge'),
  addBlock: invoke('block:add'),
  deleteBlock: invoke('block:delete'),
  deleteBlocks: invoke('block:delete-many'),
  duplicateBlocks: invoke('block:duplicate'),
  copyBlocks: invoke('block:clipboard:copy'),
  pasteBlocks: invoke('block:clipboard:paste'),
  setBlockKind: invoke('block:set-kind'),
  moveBlocks: invoke('block:move-many'),
  moveBlock: invoke('block:move'),
  arrangeBlocks: invoke('block:arrange'),
  makeColumns: invoke('block:make-columns'),
  groupColumns: invoke('block:group-columns'),
  ungroupColumns: invoke('block:ungroup-columns'),
  resizeColumns: invoke('block:resize'),
  resizeMedia: invoke('block:media-width'),
  chooseBlockFile: () => ipcRenderer.invoke('block:choose-file'),

  // Databases & Database Properties
  createDatabaseBlock: invoke('database:block:create'),
  getDatabase: invoke('database:get'),
  addProperty: invoke('database:property:add'),
  updateProperty: invoke('database:property:update'),
  renamePropertyOption: invoke('database:property:option:rename'),
  removePropertyOption: invoke('database:property:option:remove'),
  moveProperty: invoke('database:property:move'),
  deleteProperty: invoke('database:property:delete'),
  reorderDatabaseRecord: invoke('database:records:reorder'),
  moveDatabaseRecord: invoke('database:records:move'),
  createDatabaseRecord: invoke('database:records:create'),
  createDatabaseRecordWithProperty: invoke('database:records:create-with-property'),
  trashDatabaseRecords: invoke('database:records:trash'),
  bulkSetDatabaseValue: invoke('database:records:bulk-value'),
  setValue: invoke('database:value'),
  setView: invoke('database:view'),
  setDatabaseSort: invoke('database:sort'),
  setDatabaseViewSettings: invoke('database:view-settings'),
  setGalleryColumns: invoke('database:gallery-columns'),

  // Custom Page Properties
  addCustomProperty: invoke('custom:property:add'),
  updateCustomProperty: invoke('custom:property:update'),
  renameCustomPropertyOption: invoke('custom:property:option:rename'),
  removeCustomPropertyOption: invoke('custom:property:option:remove'),
  moveCustomProperty: invoke('custom:property:move'),
  deleteCustomProperty: invoke('custom:property:delete'),
  setCustomValue: invoke('custom:value'),

  // Media, Links, Clipboard & Markdown
  importMedia: invoke('media:import'),
  importDroppedFile: invoke('media:drop-import'),
  getDroppedFilePath: (file) => webUtils.getPathForFile(file),
  setCover: invoke('media:cover'),
  openMedia: invoke('media:open'),
  openUrl: invoke('link:open-url'),
  openFileExplorer: invoke('link:open-path'),
  copyText: invoke('clipboard:write-text'),
  importMarkdown: () => ipcRenderer.invoke('markdown:import'),
  exportMarkdown: invoke('markdown:export'),

  // Settings & Windows
  getSettings: () => ipcRenderer.invoke('settings:get'),
  setSetting: invoke('settings:set'),
  openPageWindow: invoke('window:open-page'),
  closeReady: () => ipcRenderer.send('window:close-ready'),

  // Assistant
  getAssistantSettings: () => ipcRenderer.invoke('assistant:settings:get'),
  setAssistantSettings: invoke('assistant:settings:set'),
  startAssistant: invoke('assistant:start'),
  stopAssistant: () => ipcRenderer.invoke('assistant:stop'),
  undoAssistant: () => ipcRenderer.invoke('assistant:undo'),

  // Dock & Panes
  ensureDockView: invoke('dock:ensure-view'),
  setDockViewBounds: invoke('dock:set-bounds'),
  removeDockView: invoke('dock:remove-view'),
  removeDockTab: invoke('dock:tab:remove'),
  insertDockTab: invoke('dock:tab:insert'),
  flushDockViews: () => ipcRenderer.invoke('dock:flush-views'),
  closeDockViews: () => ipcRenderer.invoke('dock:close-views'),
  requestDockTabMove: (value) => ipcRenderer.send('dock:tab-move-request', value),
  tabChangeReady: (value) => ipcRenderer.send('dock:tab-change-ready', value),
  navigateDockPane: (value) => ipcRenderer.send('dock:navigate-pane', value),
  focusDockPane: () => ipcRenderer.send('dock:pane-focus'),
  reportDockPaneState: (value) => ipcRenderer.send('dock:pane-state', value),
  paneCloseReady: (value) => ipcRenderer.send('pane:close-ready', value),
  workspaceHistoryFlushReady: (value) => ipcRenderer.send('workspace:history-flush-ready', value),

  // Event Listeners
  onPrepareClose: onEvent('window:prepare-close', false),
  onDockPaneFocus: onEvent('dock:pane-focus'),
  onDockPaneState: onEvent('dock:pane-state'),
  onDockPaneGone: onEvent('dock:pane-gone'),
  onDockTabMoveRequest: onEvent('dock:tab-move-request'),
  onPaneNavigate: onEvent('pane:navigate'),
  onPaneTabRemove: onEvent('pane:tab-remove'),
  onPaneTabInsert: onEvent('pane:tab-insert'),
  onPanePrepareClose: onEvent('pane:prepare-close'),
  onWorkspaceReloaded: onEvent('workspace:reloaded', false),
  onWorkspacePageUpdated: onEvent('workspace:page-updated'),
  onWorkspaceImportProgress: onEvent('workspace:import-progress'),
  onWorkspaceTreeChanged: onEvent('workspace:tree-changed', false),
  onWorkspaceHistoryChanged: onEvent('workspace:history-changed'),
  onPrepareWorkspaceHistoryFlush: onEvent('workspace:prepare-history-flush'),
  onWorkspacePageRestored: onEvent('workspace:page-restored'),
  onWorkspacePagesTrashed: onEvent('workspace:pages-trashed'),
  onPageLeaseReleased: onEvent('page:lease-released'),
  onSettingsChanged: onEvent('settings:changed'),
  onAssistantEvent: onEvent('assistant:event'),
});
