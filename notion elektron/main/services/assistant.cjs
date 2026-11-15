const path = require('node:path');
const fs = require('node:fs');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const { app, BrowserWindow } = require('electron');
const { state, one } = require('../state.cjs');
const { setupWorkspace } = require('../database/workspace.cjs');
const { clearWorkspaceHistory } = require('./history.cjs');

const ASSISTANT_SYSTEM_PREAMBLE =
  "You are the Noteon Assistant, built into the user's local notes app Noteon. Use ONLY the `noteon` MCP tools to read and change the workspace (pages, blocks, tables/databases). Do not run shell commands and do not touch files directly. Reply in the user's language (Turkish if unsure) and keep answers short. Workflow: look things up first (list_pages / search_notes / read_page / get_database), then act with bulk tools (add_rows, update_rows, insert_content, update_blocks) instead of many single calls, and finally say briefly what you changed. Blocks of kind `subpage_link` represent subpages: never delete them unless the user asks. 'This page' / 'bu sayfa' means the active page given below. A 'table' means a database: use create_database with inline_in_page_id to put one inside the active page. The user can undo your changes, so act directly without asking for confirmation unless the request is ambiguous or destructive.";

const ASSISTANT_READONLY_TOOLS = new Set(['list_pages', 'search_notes', 'read_page', 'get_database']);

function toml(value) {
  return `'${String(value).replaceAll("'", "\\'")}'`;
}

function findNpmNativeCodex(npmDirectory) {
  try {
    const scoped = path.join(npmDirectory, 'node_modules', '@openai', 'codex', 'node_modules', '@openai');
    for (const pkg of fs.readdirSync(scoped, { withFileTypes: true })) {
      const vendor = path.join(scoped, pkg.name, 'vendor');
      if (!fs.existsSync(vendor)) continue;
      for (const target of fs.readdirSync(vendor, { withFileTypes: true })) {
        const exe = path.join(vendor, target.name, 'bin', 'codex.exe');
        if (fs.existsSync(exe)) return exe;
      }
    }
  } catch {}
  return null;
}

function findCodex() {
  if (process.env.NOTEON_CODEX_PATH) return process.env.NOTEON_CODEX_PATH;
  const roots = [];
  if (process.env.LOCALAPPDATA) roots.push(path.join(process.env.LOCALAPPDATA, 'OpenAI', 'Codex', 'bin'));

  for (const root of roots) {
    try {
      const entries = fs
        .readdirSync(root, { withFileTypes: true })
        .filter((entry) => entry.isDirectory())
        .map((entry) => {
          const file = path.join(root, entry.name, 'codex.exe');
          return fs.existsSync(file) ? { file, time: fs.statSync(file).mtimeMs } : null;
        })
        .filter(Boolean)
        .sort((a, b) => b.time - a.time);
      if (entries[0]) return entries[0].file;
    } catch {}
  }

  for (const dir of (process.env.PATH || '').split(path.delimiter)) {
    const direct = path.join(dir, 'codex.exe');
    if (fs.existsSync(direct)) return direct;
    const native = findNpmNativeCodex(dir);
    if (native) return native;
  }

  for (const dir of (process.env.PATH || '').split(path.delimiter)) {
    for (const name of ['codex.cmd', 'codex']) {
      const file = path.join(dir, name);
      if (fs.existsSync(file)) return file;
    }
  }

  return 'codex';
}

function readAssistantSettings() {
  try {
    return {
      model: 'gpt-6-luna',
      effort: 'high',
      ...JSON.parse(fs.readFileSync(path.join(state.dataDir, 'assistant.json'), 'utf8')),
    };
  } catch {
    return { model: 'gpt-6-luna', effort: 'high' };
  }
}

function pruneAssistantSnapshots(directory) {
  try {
    const protectedPaths = new Set(state.assistantUndo.map((entry) => path.resolve(entry.path)));
    const files = fs
      .readdirSync(directory, { withFileTypes: true })
      .filter((entry) => entry.isFile() && entry.name.endsWith('.sqlite'))
      .map((entry) => path.join(directory, entry.name))
      .sort();
    const excess = Math.max(0, files.length - 20);
    for (const file of files.slice(0, excess)) {
      if (!protectedPaths.has(path.resolve(file))) fs.rmSync(file, { force: true });
    }
  } catch {}
}

function killAssistantChild() {
  if (!state.assistantChild) return;
  const child = state.assistantChild;
  try {
    if (process.platform === 'win32' && child.pid) {
      spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    }
    child.kill();
  } catch {}
}

function registerAssistantIpc(ipcMain) {
  ipcMain.handle('assistant:settings:get', () => readAssistantSettings());

  ipcMain.handle('assistant:settings:set', (_event, value) => {
    fs.writeFileSync(path.join(state.dataDir, 'assistant.json'), JSON.stringify(value, null, 2));
    return true;
  });

  ipcMain.handle('assistant:start', async (event, { request, page, history = [] }) => {
    if (state.workspaceRestoreActive) throw new Error('The workspace is being restored.');
    if (state.assistantChild || state.assistantStarting) return false;
    state.assistantStarting = true;

    const emit = (payload) => {
      if (!event.sender.isDestroyed()) event.sender.send('assistant:event', payload);
    };

    const stamp = Date.now();
    const snapshotDir = path.join(state.dataDir, 'ai-snapshots');
    const snapshot = path.join(snapshotDir, `before-electron-${stamp}.sqlite`);
    let baselineChanges;

    try {
      fs.mkdirSync(snapshotDir, { recursive: true });
      baselineChanges = one('SELECT total_changes() AS n').n;
      await state.db.backup(snapshot);
      pruneAssistantSnapshots(snapshotDir);
      if (state.shuttingDown) {
        state.assistantStarting = false;
        fs.rmSync(snapshot, { force: true });
        return false;
      }
      state.assistantSnapshot = snapshot;
    } catch (error) {
      state.assistantStarting = false;
      fs.rmSync(snapshot, { force: true });
      emit({ type: 'error', text: `Could not create assistant snapshot: ${error.message}` });
      return false;
    }

    const settings = readAssistantSettings();
    const mcpExecutable = app.isPackaged
      ? path.join(process.resourcesPath, 'noteon-mcp.exe')
      : path.resolve(state.projectDir, '..', 'target', 'release', 'noteon.exe');

    if (!fs.existsSync(mcpExecutable)) {
      fs.rmSync(snapshot, { force: true });
      state.assistantSnapshot = null;
      state.assistantStarting = false;
      emit({
        type: 'error',
        text: 'The Noteon MCP helper is missing. Build the original Noteon release first.',
      });
      return false;
    }

    const executable = findCodex();
    const server = 'mcp_servers.noteon';
    const model = settings.model || 'gpt-6-luna';
    const effort = settings.effort || 'high';
    const args = [
      'exec',
      '--json',
      '--skip-git-repo-check',
      '--ignore-user-config',
      '--ignore-rules',
      '-s',
      'read-only',
      '--disable',
      'apps',
      '--disable',
      'plugins',
      '-C',
      state.dataDir,
      '-c',
      'approval_policy="never"',
      '-c',
      `${server}.command=${toml(mcpExecutable)}`,
      '-c',
      `${server}.args=["--mcp"]`,
      '-c',
      `${server}.env={ NOTEON_DATA_DIR = ${toml(state.dataDir)} }`,
      '-c',
      `${server}.startup_timeout_sec=30`,
      '-c',
      `${server}.default_tools_approval_mode="approve"`,
      '-m',
      model,
      '-c',
      `model_reasoning_effort="${effort}"`,
      '-',
    ];

    const child = spawn(executable, args, {
      cwd: state.dataDir,
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
      shell: executable.endsWith('.cmd'),
    });
    state.assistantChild = child;
    state.assistantStarting = false;

    let stderr = '';
    let changed = false;
    let lastLiveRefresh = 0;

    const recent = history
      .filter((item) => item.role === 'user' || item.role === 'assistant')
      .slice(-12)
      .map((item) => `${item.role === 'user' ? 'User' : 'Assistant'}: ${item.text}`)
      .join('\n');
    const pageCtx = page?.id
      ? `Active page: id=${page.id} title=${JSON.stringify(page.title || '')} is_database=${Boolean(page.isDatabase)}`
      : 'Active page: none';
    const prompt = `${ASSISTANT_SYSTEM_PREAMBLE}\n${pageCtx}\n${recent}\nUser: ${request}\n`;

    const lines = readline.createInterface({ input: child.stdout });
    lines.on('line', (line) => {
      let value;
      try {
        value = JSON.parse(line);
      } catch {
        return;
      }
      const { type, item = {} } = value;
      if (type === 'thread.started') emit({ type: 'started' });
      if (type === 'item.started' && item.type === 'reasoning') emit({ type: 'thinking' });
      if (type === 'item.started' && item.type === 'mcp_tool_call' && item.server === 'noteon') {
        emit({ type: 'activity', text: item.tool });
        if (!ASSISTANT_READONLY_TOOLS.has(item.tool)) changed = true;
      }
      if (
        type === 'item.completed' &&
        item.type === 'mcp_tool_call' &&
        item.server === 'noteon' &&
        !ASSISTANT_READONLY_TOOLS.has(item.tool)
      ) {
        changed = true;
        const now = Date.now();
        if (now - lastLiveRefresh >= 1500) {
          lastLiveRefresh = now;
          emit({ type: 'workspace-mutated' });
        }
      }
      if (type === 'item.completed' && item.type === 'agent_message' && item.text) {
        emit({ type: 'message', text: item.text });
      }
      if (type === 'turn.failed') {
        emit({ type: 'error', text: value.error?.message || 'The assistant run failed.' });
      }
    });

    child.stderr.on('data', (data) => {
      stderr = (stderr + data.toString()).slice(-3000);
    });

    child.on('error', (error) => {
      emit({
        type: 'error',
        text: `Could not start Codex: ${error.message}. Install and sign in to the Codex CLI.`,
      });
    });

    child.on('close', (code) => {
      state.assistantChild = null;
      if (state.shuttingDown || !state.db) {
        state.assistantSnapshot = null;
        return;
      }
      if (changed && state.assistantSnapshot && one('SELECT total_changes() AS n').n === baselineChanges) {
        state.assistantUndo.push({
          path: state.assistantSnapshot,
          dataVersion: state.db.pragma('data_version', { simple: true }),
          totalChanges: baselineChanges,
        });
        emit({ type: 'undo-ready' });
      } else if (changed && state.assistantSnapshot) {
        emit({
          type: 'error',
          text: 'Undo is unavailable because the workspace was edited while the assistant was running.',
        });
      } else if (state.assistantSnapshot) {
        fs.rmSync(state.assistantSnapshot, { force: true });
      }
      state.assistantSnapshot = null;
      emit({
        type: 'done',
        code,
        stderr: code ? stderr : '',
        changed,
        canUndo: state.assistantUndo.length > 0,
      });
    });

    child.stdin.end(prompt);
    return true;
  });

  ipcMain.handle('assistant:stop', () => {
    killAssistantChild();
    return true;
  });

  ipcMain.handle('assistant:undo', () => {
    if (state.assistantChild || state.assistantStarting) return false;
    if (BrowserWindow.getAllWindows().length !== 1) {
      return { error: 'Close the other workspace windows before undoing assistant changes.' };
    }
    const entry = state.assistantUndo.at(-1);
    if (!entry || !fs.existsSync(entry.path)) return false;

    if (
      state.db.pragma('data_version', { simple: true }) !== entry.dataVersion ||
      one('SELECT total_changes() AS n').n !== entry.totalChanges
    ) {
      return {
        error: 'The workspace changed after the assistant finished. Undo was stopped to protect those changes.',
      };
    }

    const currentPath = path.join(state.dataDir, 'noteon.sqlite');
    const safetyPath = path.join(state.dataDir, 'ai-snapshots', `before-undo-${Date.now()}.sqlite`);
    let replaced = false;

    try {
      const checkpoint = state.db.pragma('wal_checkpoint(TRUNCATE)');
      if (checkpoint.some((row) => row.busy !== 0)) {
        throw new Error('The database is in use by another connection.');
      }
      fs.copyFileSync(currentPath, safetyPath, fs.constants.COPYFILE_EXCL);
      state.db.close();
      state.db = null;
      for (const suffix of ['-wal', '-shm']) {
        if (fs.existsSync(currentPath + suffix)) fs.rmSync(currentPath + suffix, { force: true });
      }
      fs.copyFileSync(entry.path, currentPath);
      replaced = true;
      setupWorkspace();
      state.pageHistories.clear();
      clearWorkspaceHistory();
      state.assistantUndo.pop();
      fs.rmSync(entry.path, { force: true });

      const canUndo = state.assistantUndo.length > 0;
      for (const win of BrowserWindow.getAllWindows()) {
        if (!win.webContents.isDestroyed()) win.webContents.send('assistant:event', { type: 'restored', canUndo });
      }
      return true;
    } catch (error) {
      try {
        if (replaced && state.db) {
          state.db.close();
          state.db = null;
        }
        if (!state.db) {
          for (const suffix of ['-wal', '-shm']) {
            if (fs.existsSync(currentPath + suffix)) fs.rmSync(currentPath + suffix, { force: true });
          }
          if (fs.existsSync(safetyPath)) fs.copyFileSync(safetyPath, currentPath);
          setupWorkspace();
        }
      } catch (recovery) {
        return {
          error: `Undo failed: ${error.message}. Recovery failed: ${recovery.message}. Safety copy: ${safetyPath}`,
        };
      }
      return { error: `Undo failed: ${error.message}. The workspace was preserved.` };
    }
  });
}

module.exports = {
  killAssistantChild,
  registerAssistantIpc,
};
