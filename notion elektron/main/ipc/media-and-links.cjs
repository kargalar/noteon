const path = require('node:path');
const fs = require('node:fs');
const { TextDecoder } = require('node:util');
const { dialog, shell, clipboard } = require('electron');
const { state, rows, one } = require('../state.cjs');
const { assertPageWritable } = require('../services/leases.cjs');
const { recordPageUndo } = require('../services/history.cjs');
const { parseMarkdownImport, markdownExportFileName } = require('../utils/markdown.cjs');

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'tif', 'tiff']);
const VIDEO_EXTENSIONS = new Set(['mp4', 'mov', 'm4v', 'webm', 'mkv', 'avi', 'wmv']);

function mediaKindForPath(source) {
  const extension = path.extname(String(source || '')).slice(1).toLowerCase();
  if (IMAGE_EXTENSIONS.has(extension)) return 'image';
  if (VIDEO_EXTENSIONS.has(extension)) return 'video';
  return null;
}

function importMediaPath(sender, pageId, position, source, expectedKind) {
  assertPageWritable(sender, pageId);
  source = path.resolve(String(source || ''));
  if (!fs.statSync(source).isFile()) {
    throw new Error('The selected media file could not be found.');
  }
  const kind = mediaKindForPath(source);
  if (!kind || (expectedKind && kind !== expectedKind)) {
    throw new Error('This file type is not supported as the selected media type.');
  }

  const directory = path.join(state.dataDir, 'attachments');
  fs.mkdirSync(directory, { recursive: true });
  const destination = path.join(
    directory,
    `${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}-${path.basename(source)}`,
  );
  fs.copyFileSync(source, destination);
  recordPageUndo(pageId, 'Medya eklendi', true);

  try {
    return state.db.transaction(() => {
      const count = one('SELECT count(*) AS n FROM blocks WHERE page_id=?', pageId).n;
      position = Math.max(0, Math.min(Number(position) || 0, count));
      state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>=?').run(pageId, position);
      const id = state.db
        .prepare('INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,?,?)')
        .run(pageId, position, kind, destination).lastInsertRowid;
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return id;
    })();
  } catch (error) {
    fs.rmSync(destination, { force: true });
    throw error;
  }
}

function registerMediaAndLinksIpc(ipcMain) {
  ipcMain.handle('media:open', (_event, targetPath) => shell.openPath(targetPath));

  ipcMain.handle('link:open-url', async (_event, raw) => {
    let target = String(raw || '').trim();
    if (/^www\./i.test(target)) target = `https://${target}`;
    let url;
    try {
      url = new URL(target);
    } catch {
      throw new Error('This link is not a valid web address.');
    }
    if (!['http:', 'https:'].includes(url.protocol)) {
      throw new Error('Only HTTP and HTTPS links can be opened from notes.');
    }
    await shell.openExternal(url.href);
    return true;
  });

  ipcMain.handle('link:open-path', async (_event, raw) => {
    let target = String(raw || '').trim().replace(/^['"]|['"]$/g, '');
    if (!target) throw new Error('This file path is empty.');

    if (/^file:\/\/[a-z]:[\\/]/i.test(target)) {
      target = target.slice(7).replaceAll('/', '\\');
    } else if (/^file:\/\//i.test(target)) {
      let fileUrl;
      try {
        fileUrl = new URL(target);
      } catch {
        throw new Error('This file path is not valid.');
      }
      const pathname = decodeURIComponent(fileUrl.pathname);
      const host = fileUrl.hostname && fileUrl.hostname.toLowerCase() !== 'localhost' ? fileUrl.hostname : '';
      if (process.platform === 'win32') {
        target = host
          ? `\\\\${host}${pathname.replaceAll('/', '\\')}`
          : pathname.replace(/^\/([a-z]:)/i, '$1').replaceAll('/', '\\');
      } else {
        target = host ? `//${host}${pathname}` : pathname;
      }
    }

    const absolute = process.platform === 'win32' ? path.win32.isAbsolute(target) : path.isAbsolute(target);
    if (!absolute) throw new Error('Only absolute file paths can be opened from notes.');

    const normalized = process.platform === 'win32' ? path.win32.normalize(target) : path.normalize(target);
    let stat;
    try {
      stat = fs.statSync(normalized);
    } catch {}

    if (stat?.isFile()) {
      shell.showItemInFolder(normalized);
      return true;
    }
    if (stat?.isDirectory()) {
      const error = await shell.openPath(normalized);
      if (error) throw new Error(error);
      return true;
    }

    const parent = path.dirname(normalized);
    let parentIsDirectory = false;
    try {
      parentIsDirectory = fs.statSync(parent).isDirectory();
    } catch {}
    if (parentIsDirectory) {
      const error = await shell.openPath(parent);
      if (error) throw new Error(error);
      return true;
    }

    throw new Error('The file and its parent folder could not be found.');
  });

  ipcMain.handle('clipboard:write-text', (_event, value) => {
    clipboard.writeText(String(value ?? ''));
    return true;
  });

  ipcMain.handle('block:choose-file', async () => {
    const { canceled, filePaths } = await dialog.showOpenDialog({ properties: ['openFile'] });
    return canceled ? null : filePaths[0] || null;
  });

  ipcMain.handle('media:import', async (event, { pageId, position, kind }) => {
    assertPageWritable(event.sender, pageId);
    const video = kind === 'video';
    const { canceled, filePaths } = await dialog.showOpenDialog({
      properties: ['openFile'],
      filters: [
        video
          ? { name: 'Videos', extensions: ['mp4', 'mov', 'm4v', 'webm', 'mkv', 'avi', 'wmv'] }
          : { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'tif', 'tiff'] },
      ],
    });
    if (canceled || !filePaths[0]) return null;
    return importMediaPath(event.sender, pageId, position, filePaths[0], kind);
  });

  ipcMain.handle('media:drop-import', (event, { pageId, position, filePath }) => {
    assertPageWritable(event.sender, pageId);
    const source = path.resolve(String(filePath || ''));
    if (!fs.statSync(source).isFile()) throw new Error('The dropped file could not be found.');
    const kind = mediaKindForPath(source);
    if (kind) return importMediaPath(event.sender, pageId, position, source, kind);

    recordPageUndo(pageId, 'Dosya yolu eklendi', true);
    return state.db.transaction(() => {
      const count = one('SELECT count(*) AS n FROM blocks WHERE page_id=?', pageId).n;
      position = Math.max(0, Math.min(Number(position) || 0, count));
      state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>=?').run(pageId, position);
      const id = state.db
        .prepare("INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,'text',?)")
        .run(pageId, position, source).lastInsertRowid;
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return id;
    })();
  });

  ipcMain.handle('media:cover', async (event, pageId) => {
    assertPageWritable(event.sender, pageId);
    const { canceled, filePaths } = await dialog.showOpenDialog({
      properties: ['openFile'],
      filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'tif', 'tiff'] }],
    });
    if (canceled || !filePaths[0]) return null;

    const directory = path.join(state.dataDir, 'attachments', 'covers');
    fs.mkdirSync(directory, { recursive: true });
    const source = filePaths[0];
    const destination = path.join(directory, `${pageId}-${Date.now()}-${path.basename(source)}`);
    fs.copyFileSync(source, destination);
    recordPageUndo(pageId, 'Kapak görseli değiştirildi', true);

    try {
      state.db
        .prepare('UPDATE pages SET cover_image=?,cover=0,updated_at=unixepoch() WHERE id=?')
        .run(destination, pageId);
      return destination;
    } catch (error) {
      fs.rmSync(destination, { force: true });
      throw error;
    }
  });

  ipcMain.handle('markdown:import', async () => {
    if (state.workspaceRestoreActive) throw new Error('The workspace is being restored.');
    const { canceled, filePaths } = await dialog.showOpenDialog({
      properties: ['openFile'],
      filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }],
    });
    if (canceled || !filePaths[0]) return null;
    if (fs.statSync(filePaths[0]).size > 16 * 1024 * 1024) {
      throw new Error('Markdown files cannot exceed 16 MB.');
    }

    const bytes = fs.readFileSync(filePaths[0]);
    let source;
    try {
      source = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
      throw new Error('Markdown files must use valid UTF-8 encoding.');
    }

    const { title, blocks } = parseMarkdownImport(source, filePaths[0]);
    const insertBlock = state.db.prepare('INSERT INTO blocks(page_id,position,kind,content,checked) VALUES(?,?,?,?,?)');
    return state.db.transaction(() => {
      const page = state.db
        .prepare(
          'INSERT INTO pages(title,sort_order,created_at,updated_at) SELECT ?,coalesce(max(sort_order)+1,0),unixepoch(),unixepoch() FROM pages WHERE parent_id IS NULL',
        )
        .run(title).lastInsertRowid;
      blocks.forEach((block, position) =>
        insertBlock.run(page, position, block.kind, block.content, block.checked),
      );
      return page;
    })();
  });

  ipcMain.handle('markdown:export', async (_event, id) => {
    const page = one('SELECT title FROM pages WHERE id=?', id);
    if (!page) return false;
    const title = String(page.title || '').trim() ? page.title : 'Untitled';
    const { canceled, filePath } = await dialog.showSaveDialog({
      defaultPath: markdownExportFileName(title),
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (canceled || !filePath) return false;

    const blocks = rows('SELECT kind,content,checked FROM blocks WHERE page_id=? ORDER BY position', id);
    let number = 0;
    const prefixMap = (checked) => ({
      h1: '# ',
      h2: '## ',
      h3: '### ',
      bullet: '- ',
      todo: checked ? '- [x] ' : '- [ ] ',
      quote: '> ',
      callout: '> [!NOTE] ',
    });

    const rendered = blocks.map((block) => {
      number = block.kind === 'numbered' ? number + 1 : 0;
      if (block.kind === 'code') {
        const longest = Math.max(
          0,
          ...block.content.split('\n').map((line) => (line.match(/^`+/) || [''])[0].length),
        );
        const fence = '`'.repeat(Math.max(3, longest + 1));
        return `${fence}\n${block.content}\n${fence}`;
      }
      if (block.kind === 'divider') return '---';
      if (block.kind === 'page_link') return `[Page](noteon://page/${block.content})`;
      if (block.kind === 'numbered') return `${number}. ${block.content}`;
      const prefix = prefixMap(block.checked)[block.kind] ?? '';
      return prefix + (['quote', 'callout'].includes(block.kind) ? block.content.replaceAll('\n', '\n> ') : block.content);
    });

    const body = rendered.join('\n\n');
    fs.writeFileSync(filePath, `# ${title}\n\n${body}${body ? '\n\n' : ''}`, 'utf8');
    return true;
  });
}

module.exports = {
  mediaKindForPath,
  importMediaPath,
  registerMediaAndLinksIpc,
};
