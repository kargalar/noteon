const path = require('node:path');
const fs = require('node:fs');
const { app, BrowserWindow, dialog } = require('electron');
const Database = require('better-sqlite3');
const { state, rows, one } = require('../state.cjs');
const {
  configureWorkspaceDatabase,
  ensureWorkspaceSchema,
  validateWorkspaceDatabase,
} = require('./schema.cjs');
const {
  copyNoteonAttachments,
  rebaseImportedAttachmentPaths,
  scanNoteonAttachmentReferences,
  rewriteNoteonAttachmentReferences,
  copyNoteonExtras,
} = require('./attachments.cjs');
const { broadcastToWorkspace } = require('../services/notifications.cjs');
const { clearWorkspaceHistory } = require('../services/history.cjs');

/**
 * Initializes the SQLite workspace database, migrating schema and rebasing attachments if needed.
 */
function setupWorkspace() {
  const executableDir = process.env.PORTABLE_EXECUTABLE_DIR || path.dirname(process.execPath);
  const packagedData = [
    path.join(executableDir, 'data'),
    path.resolve(executableDir, '..', 'data'),
    path.resolve(executableDir, '..', '..', 'data'),
  ];
  state.dataDir =
    process.env.NOTEON_DATA_DIR ||
    (app.isPackaged
      ? packagedData.find((directory) => fs.existsSync(path.join(directory, 'noteon.sqlite'))) || packagedData[0]
      : path.join(state.projectDir, 'data'));

  fs.mkdirSync(state.dataDir, { recursive: true });
  state.databasePath = path.join(state.dataDir, 'noteon.sqlite');

  if (!fs.existsSync(state.databasePath)) {
    const oldDb = path.join(state.importedFrom, 'noteon.sqlite');
    if (fs.existsSync(oldDb)) {
      fs.copyFileSync(oldDb, state.databasePath);
      const attachments = path.join(state.importedFrom, 'attachments');
      if (fs.existsSync(attachments)) {
        fs.cpSync(attachments, path.join(state.dataDir, 'attachments'), { recursive: true });
      }
    }
  }

  state.db = new Database(state.databasePath);
  configureWorkspaceDatabase(state.db);
  ensureWorkspaceSchema(state.db);
  rebaseImportedAttachmentPaths();
  state.db.pragma('foreign_keys = ON');
}

function removeWalFiles(sqlitePath) {
  for (const suffix of ['-wal', '-shm']) {
    if (fs.existsSync(sqlitePath + suffix)) {
      fs.rmSync(sqlitePath + suffix, { force: true });
    }
  }
}

function registerWorkspaceIpc(ipcMain) {
  ipcMain.handle('workspace:summary', () => ({
    pages: rows(
      'SELECT id,parent_id,title,icon,favorite,trashed_by,sort_order,block_count,created_at,updated_at FROM pages ORDER BY sort_order,id',
    ),
    count: one('SELECT count(*) AS n FROM pages WHERE trashed_by IS NULL').n,
    dataDir: state.dataDir,
  }));

  ipcMain.handle('workspace:backup', async () => {
    if (state.workspaceRestoreActive) throw new Error('The workspace is being restored.');
    const { canceled, filePath } = await dialog.showSaveDialog({
      defaultPath: 'noteon-electron-backup.sqlite',
      filters: [{ name: 'SQLite database', extensions: ['sqlite'] }],
    });
    if (canceled || !filePath) return false;
    if (path.resolve(filePath) === path.resolve(state.databasePath)) {
      throw new Error('Choose a different location from the active workspace database.');
    }
    await state.db.backup(filePath);
    return true;
  });

  ipcMain.handle('workspace:restore', async (event) => {
    const owner = BrowserWindow.fromWebContents(event.sender);
    if (!owner) throw new Error('Workspace restore must be started from an app window.');
    if (BrowserWindow.getAllWindows().some((win) => win !== owner)) {
      throw new Error('Close other Notion Elektron windows before restoring a workspace.');
    }
    if (state.assistantChild || state.assistantStarting) {
      throw new Error('Stop the assistant and wait for its run to finish before restoring a workspace.');
    }
    if (state.workspaceRestoreActive) throw new Error('A workspace restore is already running.');

    state.workspaceRestoreActive = true;
    let selection;
    try {
      selection = await dialog.showOpenDialog(owner, {
        properties: ['openFile'],
        filters: [{ name: 'SQLite workspace backup', extensions: ['sqlite', 'db', 'sqlite3'] }],
      });
    } catch (error) {
      state.workspaceRestoreActive = false;
      throw error;
    }

    const { canceled, filePaths } = selection;
    if (canceled || !filePaths[0]) {
      state.workspaceRestoreActive = false;
      return false;
    }

    const sourcePath = path.resolve(filePaths[0]);
    if (sourcePath === path.resolve(state.databasePath)) {
      state.workspaceRestoreActive = false;
      throw new Error('Choose a backup file, not the active workspace database.');
    }

    const stamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupDir = path.join(state.dataDir, 'backups');
    const stagingPath = path.join(state.dataDir, `.restore-${stamp}.sqlite`);
    const oldPath = path.join(state.dataDir, `.previous-${stamp}.sqlite`);
    const safetyPath = path.join(backupDir, `before-restore-${stamp}.sqlite`);

    let input = null;
    let check = null;
    let closed = false;
    let movedOld = false;

    try {
      fs.mkdirSync(backupDir, { recursive: true });
      input = new Database(sourcePath, { readonly: true, fileMustExist: true });
      await input.backup(stagingPath);
      input.close();
      input = null;

      check = new Database(stagingPath, { readonly: true, fileMustExist: true });
      validateWorkspaceDatabase(check, 'Backup');
      check.close();
      check = null;

      await state.db.backup(safetyPath);
      state.db.pragma('wal_checkpoint(TRUNCATE)');
      state.db.close();
      closed = true;
      removeWalFiles(state.databasePath);
      fs.renameSync(state.databasePath, oldPath);
      movedOld = true;

      try {
        fs.renameSync(stagingPath, state.databasePath);
      } catch (error) {
        fs.renameSync(oldPath, state.databasePath);
        movedOld = false;
        throw error;
      }

      state.db = new Database(state.databasePath);
      configureWorkspaceDatabase(state.db);
      ensureWorkspaceSchema(state.db);
      rebaseImportedAttachmentPaths();
      closed = false;
      fs.rmSync(oldPath, { force: true });
      movedOld = false;

      state.pageHistories.clear();
      clearWorkspaceHistory();
      state.assistantUndo.length = 0;
      state.assistantSnapshot = null;
      broadcastToWorkspace('workspace:reloaded');
      return { backupPath: safetyPath, dataDir: state.dataDir };
    } catch (error) {
      if (input) input.close();
      if (check) check.close();
      if (closed) {
        try {
          if (state.db?.open) state.db.close();
          if (movedOld && fs.existsSync(oldPath)) {
            if (fs.existsSync(state.databasePath)) fs.rmSync(state.databasePath, { force: true });
            fs.renameSync(oldPath, state.databasePath);
          } else if (!fs.existsSync(state.databasePath) && fs.existsSync(safetyPath)) {
            fs.copyFileSync(safetyPath, state.databasePath);
          }
          state.db = new Database(state.databasePath);
          configureWorkspaceDatabase(state.db);
          ensureWorkspaceSchema(state.db);
        } catch (recoveryError) {
          throw new Error(
            `Workspace restore failed and automatic recovery could not reopen the database. Recovery copy: ${safetyPath}. ${recoveryError.message}`,
          );
        }
      }
      throw error;
    } finally {
      state.workspaceRestoreActive = false;
      if (fs.existsSync(stagingPath)) fs.rmSync(stagingPath, { force: true });
    }
  });

  ipcMain.handle('workspace:import-noteon', async (event) => {
    const owner = BrowserWindow.fromWebContents(event.sender);
    if (!owner || owner.webContents !== event.sender) {
      throw new Error('Noteon import must be started from the workspace window.');
    }
    if (BrowserWindow.getAllWindows().some((win) => win !== owner)) {
      throw new Error('Close other Notion Elektron windows before importing Noteon data.');
    }
    if (state.assistantChild || state.assistantStarting) {
      throw new Error('Stop the assistant and wait for its run to finish before importing Noteon data.');
    }
    if (state.workspaceRestoreActive) throw new Error('A workspace import is already running.');

    state.workspaceRestoreActive = true;
    let selection;
    try {
      selection = await dialog.showOpenDialog(owner, { properties: ['openDirectory'] });
    } catch (error) {
      state.workspaceRestoreActive = false;
      throw error;
    }

    const { canceled, filePaths } = selection;
    if (canceled || !filePaths[0]) {
      state.workspaceRestoreActive = false;
      return false;
    }

    let sourceDir;
    try {
      sourceDir = fs.realpathSync(path.resolve(filePaths[0]));
    } catch (error) {
      state.workspaceRestoreActive = false;
      throw new Error(`The selected Noteon data folder is unavailable. ${error.message}`);
    }

    const targetDir = fs.realpathSync(state.dataDir);
    const sourceDatabase = path.join(sourceDir, 'noteon.sqlite');
    const within = (parent, child) => {
      const relative = path.relative(parent, child);
      return !relative || (!relative.startsWith('..') && !path.isAbsolute(relative));
    };

    if (!fs.existsSync(sourceDatabase) || within(sourceDir, targetDir) || within(targetDir, sourceDir)) {
      state.workspaceRestoreActive = false;
      throw new Error('Choose a separate Noteon data folder containing noteon.sqlite.');
    }
    if (fs.realpathSync(sourceDatabase) === fs.realpathSync(state.databasePath)) {
      state.workspaceRestoreActive = false;
      throw new Error('The selected folder is the active Notion Elektron workspace.');
    }

    const stamp = new Date().toISOString().replace(/[:.]/g, '-');
    const stagingPath = path.join(state.dataDir, `.noteon-import-${stamp}.sqlite`);
    const backupDir = path.join(state.dataDir, 'backups');
    const safetyPath = path.join(backupDir, `before-noteon-import-${stamp}.sqlite`);
    const oldPath = path.join(state.dataDir, `.previous-noteon-import-${stamp}.sqlite`);
    const sourceAttachments = path.join(sourceDir, 'attachments');

    let importedAttachments = null;
    let importedExtras = null;
    let assistantStage = null;
    let assistantPrevious = null;
    let assistantSafety = null;
    let assistantInstalled = false;
    let assistantHadPrevious = false;
    let input = null;
    let check = null;
    let closed = false;
    let movedOld = false;
    let committed = false;

    try {
      fs.mkdirSync(backupDir, { recursive: true });
      input = new Database(sourceDatabase, { readonly: true, fileMustExist: true });
      await input.backup(stagingPath);
      input.close();
      input = null;

      check = new Database(stagingPath, { readonly: true, fileMustExist: true });
      validateWorkspaceDatabase(check);
      const counts = {
        pages: Number(check.prepare('SELECT count(*) n FROM pages').get().n),
        blocks: Number(check.prepare('SELECT count(*) n FROM blocks').get().n),
      };
      const hasAttachmentReferences = scanNoteonAttachmentReferences(check);
      if (hasAttachmentReferences && !fs.existsSync(sourceAttachments)) {
        throw new Error('The Noteon database references attachments, but its attachments folder is missing.');
      }
      check.close();
      check = null;

      const importsRoot = path.join(state.dataDir, 'noteon-imports');
      fs.mkdirSync(importsRoot, { recursive: true });
      importedExtras = path.join(importsRoot, `noteon-${stamp}`);
      counts.extraFiles = copyNoteonExtras(sourceDir, importedExtras);

      if (fs.existsSync(path.join(sourceDir, 'assistant.json'))) {
        const assistantSettings = JSON.parse(fs.readFileSync(path.join(sourceDir, 'assistant.json'), 'utf8'));
        if (!assistantSettings || typeof assistantSettings !== 'object' || Array.isArray(assistantSettings)) {
          throw new Error('Noteon assistant settings are not a JSON object.');
        }
        assistantStage = path.join(state.dataDir, `.assistant-import-${stamp}.json`);
        fs.copyFileSync(path.join(sourceDir, 'assistant.json'), assistantStage);
      }

      const attachmentsRoot = path.join(state.dataDir, 'attachments');
      const sourceHasAttachmentFolder = fs.existsSync(sourceAttachments);
      let attachmentMap = { relativePaths: [], copied: 0, reused: 0, total: 0 };

      if (sourceHasAttachmentFolder) {
        fs.mkdirSync(attachmentsRoot, { recursive: true });
        importedAttachments = path.join(attachmentsRoot, `noteon-${Date.now()}-${process.pid}`);
        attachmentMap = await copyNoteonAttachments(
          sourceAttachments,
          attachmentsRoot,
          importedAttachments,
          (progress) => {
            if (!owner.webContents.isDestroyed()) owner.webContents.send('workspace:import-progress', progress);
          },
        );
      }

      counts.attachments = attachmentMap.total;
      counts.attachmentsCopied = attachmentMap.copied;
      counts.attachmentsReused = attachmentMap.reused;
      counts.attachmentRowsRebased = 0;
      counts.attachmentRowsUnmatched = 0;

      if (sourceHasAttachmentFolder) {
        const rewritten = new Database(stagingPath);
        try {
          rewritten.pragma('foreign_keys = ON');
          const rewrittenCounts = rewriteNoteonAttachmentReferences(
            rewritten,
            attachmentsRoot,
            attachmentMap.relativePaths,
          );
          counts.attachmentRowsRebased = rewrittenCounts.rebased;
          counts.attachmentRowsUnmatched = rewrittenCounts.unmatched;
        } finally {
          rewritten.close();
        }
      }

      check = new Database(stagingPath, { readonly: true, fileMustExist: true });
      validateWorkspaceDatabase(check);
      check.close();
      check = null;

      await state.db.backup(safetyPath);
      state.db.pragma('wal_checkpoint(TRUNCATE)');
      state.db.close();
      closed = true;
      removeWalFiles(state.databasePath);
      fs.renameSync(state.databasePath, oldPath);
      movedOld = true;

      try {
        fs.renameSync(stagingPath, state.databasePath);
      } catch (error) {
        fs.renameSync(oldPath, state.databasePath);
        movedOld = false;
        throw error;
      }

      if (assistantStage) {
        const assistantTarget = path.join(state.dataDir, 'assistant.json');
        assistantPrevious = path.join(state.dataDir, `.previous-assistant-${stamp}.json`);
        if (fs.existsSync(assistantTarget)) {
          assistantHadPrevious = true;
          assistantSafety = path.join(backupDir, `before-noteon-import-${stamp}.assistant.json`);
          fs.copyFileSync(assistantTarget, assistantSafety);
          fs.renameSync(assistantTarget, assistantPrevious);
        }
        try {
          fs.renameSync(assistantStage, assistantTarget);
          assistantInstalled = true;
        } catch (error) {
          if (assistantHadPrevious && fs.existsSync(assistantPrevious)) {
            fs.renameSync(assistantPrevious, assistantTarget);
          }
          throw error;
        }
      }

      state.db = new Database(state.databasePath);
      configureWorkspaceDatabase(state.db);
      ensureWorkspaceSchema(state.db);
      state.pageHistories.clear();
      clearWorkspaceHistory();
      state.assistantUndo.length = 0;
      state.assistantSnapshot = null;
      closed = false;
      committed = true;
      movedOld = false;

      try {
        fs.rmSync(oldPath, { force: true });
        if (assistantPrevious) fs.rmSync(assistantPrevious, { force: true });
      } catch {}

      broadcastToWorkspace('workspace:reloaded');
      return { backupPath: safetyPath, ...counts };
    } catch (error) {
      if (input) input.close();
      if (check) check.close();
      if (closed) {
        try {
          if (state.db?.open) state.db.close();
          const assistantTarget = path.join(state.dataDir, 'assistant.json');
          if (assistantInstalled && fs.existsSync(assistantTarget)) {
            fs.rmSync(assistantTarget, { force: true });
          }
          if (assistantHadPrevious && assistantPrevious && fs.existsSync(assistantPrevious)) {
            fs.renameSync(assistantPrevious, assistantTarget);
          }
          if (movedOld && fs.existsSync(oldPath)) {
            if (fs.existsSync(state.databasePath)) fs.rmSync(state.databasePath, { force: true });
            fs.renameSync(oldPath, state.databasePath);
          } else if (!fs.existsSync(state.databasePath) && fs.existsSync(safetyPath)) {
            fs.copyFileSync(safetyPath, state.databasePath);
          }
          state.db = new Database(state.databasePath);
          configureWorkspaceDatabase(state.db);
          ensureWorkspaceSchema(state.db);
        } catch (recoveryError) {
          throw new Error(
            `Noteon import failed and automatic recovery could not reopen the workspace. Recovery copy: ${safetyPath}. ${recoveryError.message}`,
          );
        }
      }
      throw error;
    } finally {
      state.workspaceRestoreActive = false;
      if (fs.existsSync(stagingPath)) fs.rmSync(stagingPath, { force: true });
      if (assistantStage && fs.existsSync(assistantStage)) fs.rmSync(assistantStage, { force: true });
      if (!committed && importedAttachments && fs.existsSync(importedAttachments)) {
        fs.rmSync(importedAttachments, { recursive: true, force: true });
      }
      if (!committed && importedExtras && fs.existsSync(importedExtras)) {
        fs.rmSync(importedExtras, { recursive: true, force: true });
      }
    }
  });
}

module.exports = {
  setupWorkspace,
  registerWorkspaceIpc,
};
