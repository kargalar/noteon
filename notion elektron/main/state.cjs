const path = require('node:path');

const projectDir = path.resolve(__dirname, '..');
const importedFrom = path.resolve(projectDir, '..', 'target', 'release', 'data');

/**
 * Shared mutable runtime state for the Electron main process.
 * Consumers should read/write mutable fields via `state.<field>` instead of destructuring them.
 */
const state = {
  db: null,
  dataDir: null,
  databasePath: null,
  workspaceRestoreActive: false,
  assistantChild: null,
  assistantStarting: false,
  shuttingDown: false,
  assistantSnapshot: null,
  blockClipboard: null,
  pageClipboard: null,
  assistantUndo: [],
  dockViews: new Map(),
  pendingPaneClosures: new Map(),
  pendingWorkspaceHistoryFlushes: new Map(),
  pendingDockTabChanges: new Map(),
  pageLeases: new Map(),
  pageHistories: new Map(),
  workspaceUndoStack: [],
  workspaceRedoStack: [],
  projectDir,
  importedFrom,
};

/**
 * Executes a SQL query and returns all matching rows.
 * @param {string} sql
 * @param {...any} args
 */
function rows(sql, ...args) {
  return state.db.prepare(sql).all(...args);
}

/**
 * Executes a SQL query and returns the first matching row.
 * @param {string} sql
 * @param {...any} args
 */
function one(sql, ...args) {
  return state.db.prepare(sql).get(...args);
}

module.exports = {
  state,
  rows,
  one,
};
