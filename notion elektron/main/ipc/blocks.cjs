/**
 * Block CRUD, rich-text formatting, clipboard, ordering, and column-layout IPC handlers.
 */

const { clipboard } = require('electron');
const { state, rows, one } = require('../state.cjs');
const { assertPageWritable } = require('../services/leases.cjs');
const { recordPageUndo } = require('../services/history.cjs');
const { notifyWorkspaceTreeChanged } = require('../services/notifications.cjs');
const { parseClipboardMarkdown, clipboardBlockText } = require('../utils/markdown.cjs');

const NON_TEXTUAL_KINDS = new Set(['divider', 'database', 'page_link', 'image', 'video']);
const NON_CONVERTIBLE_KINDS = new Set(['database', 'page_link', 'image', 'video']);
const MERGEABLE_TEXT_KINDS = new Set(['text', 'h1', 'h2', 'h3', 'bullet', 'numbered', 'todo', 'quote', 'callout', 'code']);
const CONVERTIBLE_TARGET_KINDS = new Set(['text', 'h1', 'h2', 'h3', 'bullet', 'numbered', 'todo', 'quote', 'callout', 'code', 'divider']);
const TOGGLE_STYLE_FIELDS = {
  bold: 'bold',
  italic: 'italic',
  underline: 'underline',
  strikethrough: 'strikethrough',
};
const CANONICAL_STYLE_KEYS = [
  'bold',
  'italic',
  'underline',
  'strikethrough',
  'font_family',
  'foreground',
  'background',
];
const PLACEMENT_TYPES = new Set(['before', 'after', 'before_in_column', 'after_in_column', 'left', 'right']);

/**
 * Parse and validate an array of unique integer block IDs.
 * @param {unknown[]} ids
 * @returns {number[]}
 */
function parseUniqueBlockIds(ids) {
  const unique = [...new Set(ids.map(Number))];
  if (unique.some((id) => !Number.isSafeInteger(id))) {
    throw new Error('Invalid block selection.');
  }
  return unique;
}

/**
 * Load blocks for validated IDs and assert they all still exist on a single page.
 * @param {number[]} uniqueIds
 * @param {string} [columns]
 * @returns {{ selected: object[], pageId: number }}
 */
function loadSinglePageBlocks(uniqueIds, columns = '*') {
  const placeholders = uniqueIds.map(() => '?').join(',');
  const selected = rows(`SELECT ${columns} FROM blocks WHERE id IN (${placeholders})`, ...uniqueIds);
  if (selected.length !== uniqueIds.length) {
    throw new Error('Some selected blocks no longer exist.');
  }
  const pageId = selected[0].page_id;
  if (selected.some((block) => block.page_id !== pageId)) {
    throw new Error('Select blocks from one page.');
  }
  return { selected, pageId };
}

/**
 * Update a page's updated_at timestamp.
 * @param {import('better-sqlite3').Database} database
 * @param {number} pageId
 */
function touchPage(database, pageId) {
  database.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
}

/**
 * Ensure a page always contains at least one empty text block.
 * @param {import('better-sqlite3').Database} database
 * @param {number} pageId
 */
function ensurePageHasBlock(database, pageId) {
  const count = database.prepare('SELECT count(*) n FROM blocks WHERE page_id=?').get(pageId).n;
  if (count === 0) {
    database.prepare("INSERT INTO blocks(page_id,position,kind,content) VALUES(?,0,'text','')").run(pageId);
  }
}

/**
 * Parse rich-text style ranges for block formatting operations.
 * @param {string|object[]} raw
 * @returns {object[]}
 */
function parseFormattingStyleRanges(raw) {
  try {
    const parsed = typeof raw === 'string' ? JSON.parse(raw || '[]') : raw;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (range) =>
        range &&
        typeof range === 'object' &&
        Number.isSafeInteger(range.start) &&
        range.start >= 0 &&
        Number.isSafeInteger(range.end) &&
        range.end >= range.start &&
        range.style &&
        typeof range.style === 'object' &&
        !Array.isArray(range.style),
    );
  } catch {
    return [];
  }
}

/**
 * Merge all formatting styles covering a given character index.
 * @param {object[]} ranges
 * @param {number} index
 * @returns {object}
 */
function styleAtCharIndex(ranges, index) {
  return Object.assign({}, ...ranges.filter((range) => index >= range.start && index < range.end).map((range) => range.style));
}

/**
 * Keep only canonical, active formatting keys from a style object.
 * @param {object} style
 * @returns {object}
 */
function canonicalStyle(style) {
  const out = {};
  for (const key of CANONICAL_STYLE_KEYS) {
    const value = style[key];
    if (value !== undefined && value !== null && value !== false) {
      out[key] = value;
    }
  }
  return out;
}

/**
 * Convert a CSS hex color string or RGBA byte array to an RGBA tuple.
 * @param {string|number[]|null|undefined} input
 * @returns {number[]|null}
 */
function parseRgbaColor(input) {
  if (input == null || input === '') return null;
  if (Array.isArray(input)) {
    return input.slice(0, 4).map((value) => Math.max(0, Math.min(255, Math.round(Number(value) || 0))));
  }
  const match = String(input).match(/^#([0-9a-f]{6})([0-9a-f]{2})?$/i);
  if (!match) {
    throw new Error('Invalid color.');
  }
  return [...match[1].match(/../g).map((hex) => parseInt(hex, 16)), match[2] ? parseInt(match[2], 16) : 255];
}

/**
 * Normalize block positions, column indices, and fractional column widths for a page.
 * Single-column groups collapse back to normal flow blocks.
 * @param {import('better-sqlite3').Database} transaction
 * @param {number} pageId
 */
function normalizeElectronBlockColumns(transaction, pageId) {
  const blocks = transaction
    .prepare('SELECT id,position,column_group,layout_column,column_width FROM blocks WHERE page_id=? ORDER BY position,id')
    .all(pageId);

  const groups = new Map();
  for (const block of blocks) {
    if (block.column_group !== null) {
      const key = Number(block.column_group);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(block);
    }
  }

  for (const members of groups.values()) {
    const columns = [...new Set(members.map((block) => Number(block.layout_column) || 0))].sort((a, b) => a - b);
    const widths = columns.map((column) =>
      Math.max(
        0.08,
        Math.min(1, Number(members.find((block) => (Number(block.layout_column) || 0) === column)?.column_width) || 1),
      ),
    );
    const totalWidth = widths.reduce((sum, width) => sum + width, 0);

    for (const block of members) {
      const index = columns.indexOf(Number(block.layout_column) || 0);
      if (columns.length < 2) {
        block.column_group = null;
        block.layout_column = 0;
        block.column_width = 1;
      } else {
        block.layout_column = index;
        block.column_width = widths[index] / totalWidth;
      }
    }
  }

  const ordered = [];
  const emittedGroups = new Set();
  for (const block of blocks) {
    if (block.column_group === null) {
      ordered.push(block);
      continue;
    }
    const groupKey = Number(block.column_group);
    if (emittedGroups.has(groupKey)) continue;
    emittedGroups.add(groupKey);
    ordered.push(...groups.get(groupKey).slice().sort((a, b) => a.layout_column - b.layout_column || a.position - b.position));
  }

  const writeTemp = transaction.prepare(
    'UPDATE blocks SET position=?,column_group=?,layout_column=?,column_width=? WHERE id=? AND page_id=?',
  );
  ordered.forEach((block, index) => {
    writeTemp.run(index + 1000000000, block.column_group, block.layout_column, block.column_width, block.id, pageId);
  });
  const writeFinal = transaction.prepare('UPDATE blocks SET position=? WHERE id=? AND page_id=?');
  ordered.forEach((block, index) => {
    writeFinal.run(index, block.id, pageId);
  });
}

/**
 * Clamp a media block width fraction to the valid [0.25, 1] range.
 * @param {number|string|null|undefined} width
 * @returns {number}
 */
function clampMediaWidth(width) {
  return Math.max(0.25, Math.min(1, Number(width) || 1));
}

/**
 * Resolve stored content string for a pasted block (remapping cloned page references if needed).
 * @param {object} block
 * @param {Map<number, number>} clonedPages
 * @returns {string}
 */
function pastedBlockContent(block, clonedPages) {
  const raw = ['database', 'page_link'].includes(block.kind) ? clonedPages.get(Number(block.content)) : block.content;
  return String(raw ?? '');
}

/**
 * Resolve stored JSON text_styles string for a pasted block.
 * @param {object} block
 * @returns {string}
 */
function pastedBlockStylesJson(block) {
  return typeof block.text_styles === 'string' ? block.text_styles : JSON.stringify(block.text_styles || []);
}

/**
 * Register all block-level IPC handlers.
 * @param {import('electron').IpcMain} ipcMain
 * @param {{ clonePageTree: Function, remapClonedReferences: Function }} pageHelpers
 */
function registerBlockIpc(ipcMain, { clonePageTree, remapClonedReferences }) {
  ipcMain.handle('block:save', (event, { id, content, checked, text_styles }) => {
    const block = one('SELECT page_id,checked,kind FROM blocks WHERE id=?', id);
    if (!block) throw new Error('Block no longer exists.');
    assertPageWritable(event.sender, block.page_id);
    const checkedChanged = Boolean(block.checked) !== Boolean(checked);
    recordPageUndo(
      block.page_id,
      checkedChanged ? 'Görev durumu değiştirildi' : 'Metin düzenlendi',
      checkedChanged,
    );
    state.db
      .prepare('UPDATE blocks SET content=?,checked=?,text_styles=coalesce(?,text_styles) WHERE id=?')
      .run(content, checked ? 1 : 0, text_styles == null ? null : JSON.stringify(text_styles), id);
    state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=(SELECT page_id FROM blocks WHERE id=?)').run(id);
    return true;
  });

  ipcMain.handle('block:set-checked', (event, { ids, checked }) => {
    if (!Array.isArray(ids) || !ids.length) return 0;
    const unique = parseUniqueBlockIds(ids);
    const { selected, pageId } = loadSinglePageBlocks(unique, 'id,page_id,kind');
    const todos = selected.filter((block) => block.kind === 'todo').map((block) => Number(block.id));
    if (!todos.length) return 0;

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Görev durumu değiştirildi', true);
    return state.db.transaction(() => {
      const changes = state.db
        .prepare(`UPDATE blocks SET checked=? WHERE page_id=? AND id IN (${todos.map(() => '?').join(',')})`)
        .run(checked ? 1 : 0, pageId, ...todos).changes;
      touchPage(state.db, pageId);
      return changes;
    })();
  });

  ipcMain.handle('block:format-many', (event, { pageId, ids, operation, value }) => {
    pageId = Number(pageId);
    if (!Array.isArray(ids) || !ids.length) return 0;

    const placeholders = ids.map(() => '?').join(',');
    const pageBlocks = rows(
      `SELECT id,kind,content,text_styles FROM blocks WHERE page_id=? AND id IN (${placeholders})`,
      pageId,
      ...ids.map(Number),
    );
    const textual = pageBlocks.filter((block) => !NON_TEXTUAL_KINDS.has(block.kind));
    if (!textual.length) return 0;

    const allowedOps = new Set([
      'font',
      'text-color',
      'highlight',
      'clear',
      ...Object.keys(TOGGLE_STYLE_FIELDS).map((field) => `toggle:${field}`),
    ]);
    if (!allowedOps.has(operation)) {
      throw new Error('Unsupported formatting operation.');
    }

    let toggleTarget;
    if (operation.startsWith('toggle:')) {
      const field = TOGGLE_STYLE_FIELDS[operation.slice(7)];
      const nonempty = textual.filter((block) => block.content);
      const allActive =
        nonempty.length > 0 &&
        nonempty.every((block) => {
          const styles = parseFormattingStyleRanges(block.text_styles);
          const length = Array.from(block.content).length;
          return Array.from({ length }, (_, index) => styleAtCharIndex(styles, index)[field] === true).every(Boolean);
        });
      toggleTarget = !allActive;
    }

    const font = value == null ? null : String(value);
    if (operation === 'font' && font !== null && !['Monospace', 'Serif'].includes(font)) {
      throw new Error('Unsupported font.');
    }
    const color = operation === 'text-color' || operation === 'highlight' ? parseRgbaColor(value) : undefined;

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Biçimlendirme uygulandı', true);
    const updateStmt = state.db.prepare('UPDATE blocks SET text_styles=? WHERE id=? AND page_id=?');

    state.db.transaction(() => {
      for (const block of textual) {
        const chars = Array.from(block.content);
        const raw = parseFormattingStyleRanges(block.text_styles);
        const styles = chars.map((_, index) => canonicalStyle(styleAtCharIndex(raw, index)));

        for (const style of styles) {
          if (operation.startsWith('toggle:')) {
            const key = TOGGLE_STYLE_FIELDS[operation.slice(7)];
            if (toggleTarget) style[key] = true;
            else delete style[key];
          } else if (operation === 'font') {
            if (font === null) delete style.font_family;
            else style.font_family = font;
          } else if (operation === 'text-color') {
            if (color) style.foreground = color;
            else delete style.foreground;
          } else if (operation === 'highlight') {
            if (color) style.background = color;
            else delete style.background;
          } else if (operation === 'clear') {
            for (const key of CANONICAL_STYLE_KEYS) delete style[key];
          }
        }

        const ranges = [];
        for (let i = 0; i < styles.length; ) {
          const current = canonicalStyle(styles[i]);
          if (!Object.keys(current).length) {
            i++;
            continue;
          }
          const signature = JSON.stringify(current);
          let end = i + 1;
          while (end < styles.length && JSON.stringify(canonicalStyle(styles[end])) === signature) {
            end++;
          }
          ranges.push({ start: i, end, style: current });
          i = end;
        }
        updateStmt.run(JSON.stringify(ranges), block.id, pageId);
      }
      touchPage(state.db, pageId);
    })();

    return textual.length;
  });

  ipcMain.handle('block:clipboard:copy', (_event, { ids }) => {
    if (!Array.isArray(ids) || !ids.length) return false;
    const unique = parseUniqueBlockIds(ids);
    const { selected } = loadSinglePageBlocks(unique);
    selected.sort((a, b) => a.position - b.position || a.id - b.id);

    state.blockClipboard = selected.map(({ kind, content, checked, media_width, text_styles }) => ({
      kind,
      content,
      checked: Boolean(checked),
      media_width,
      text_styles,
    }));
    state.pageClipboard = null;
    clipboard.writeText(state.blockClipboard.map(clipboardBlockText).join('\n'));
    return true;
  });

  ipcMain.handle('block:clipboard:paste', (event, { pageId, position, replaceBlock = null }) => {
    assertPageWritable(event.sender, pageId);
    pageId = Number(pageId);
    if (!Number.isSafeInteger(pageId) || !one('SELECT id FROM pages WHERE id=? AND trashed_by IS NULL', pageId)) {
      throw new Error('Destination page is unavailable.');
    }

    let replacement = null;
    if (replaceBlock !== null && replaceBlock !== undefined) {
      replacement = one(
        'SELECT id,position,column_group,layout_column,column_width FROM blocks WHERE id=? AND page_id=?',
        Number(replaceBlock),
        pageId,
      );
      if (!replacement) throw new Error('Block to replace was not found.');
    }

    const rich = state.blockClipboard && clipboard.readText() === state.blockClipboard.map(clipboardBlockText).join('\n');
    let blocks;
    if (rich) {
      blocks = state.blockClipboard.map((block) => ({ ...block }));
    } else {
      const text = clipboard.readText();
      if (!text) return [];
      blocks = parseClipboardMarkdown(text);
    }
    if (!blocks.length) return [];

    recordPageUndo(pageId, 'Bloklar eklendi', true);
    const result = state.db.transaction((transaction) => {
      const pages = new Map();
      const properties = new Map();
      for (const block of blocks) {
        if (!['database', 'page_link'].includes(block.kind)) continue;
        const source = Number(block.content);
        if (!Number.isSafeInteger(source)) throw new Error('Copied page reference is invalid.');
        if (pages.has(source)) continue;

        const valid = transaction
          .prepare(
            `WITH RECURSIVE descendants(id) AS (
               SELECT ?
               UNION ALL
               SELECT p.id FROM pages p JOIN descendants d ON p.parent_id=d.id
             )
             SELECT EXISTS(SELECT 1 FROM pages WHERE id=? AND trashed_by IS NULL)
                AND NOT EXISTS(SELECT 1 FROM descendants WHERE id=?) valid`,
          )
          .get(source, source, pageId)?.valid;
        if (!valid) throw new Error('Copied page is unavailable or contains the destination.');
        clonePageTree(transaction, source, pageId, pages, properties);
      }
      if (pages.size) {
        remapClonedReferences(transaction, pages, properties);
      }

      const ids = [];
      if (replacement) {
        const first = blocks[0];
        transaction
          .prepare('UPDATE blocks SET kind=?,content=?,checked=?,media_width=?,text_styles=? WHERE id=? AND page_id=?')
          .run(
            first.kind,
            pastedBlockContent(first, pages),
            first.checked ? 1 : 0,
            clampMediaWidth(first.media_width),
            pastedBlockStylesJson(first),
            replacement.id,
            pageId,
          );
        ids.push(Number(replacement.id));

        if (blocks.length > 1) {
          const insertAt = replacement.position + 1;
          transaction.prepare('UPDATE blocks SET position=position+1000000 WHERE page_id=? AND position>=?').run(pageId, insertAt);
          transaction
            .prepare('UPDATE blocks SET position=position-1000000+? WHERE page_id=? AND position>=?')
            .run(blocks.length - 1, pageId, insertAt + 1000000);
          const insertStmt = transaction.prepare(
            'INSERT INTO blocks(page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles) VALUES(?,?,?,?,?,?,?,?,?,?)',
          );
          for (let index = 1; index < blocks.length; index++) {
            const block = blocks[index];
            const rowId = insertStmt.run(
              pageId,
              insertAt + index - 1,
              block.kind,
              pastedBlockContent(block, pages),
              block.checked ? 1 : 0,
              replacement.column_group,
              replacement.layout_column || 0,
              replacement.column_width || 1,
              clampMediaWidth(block.media_width),
              pastedBlockStylesJson(block),
            ).lastInsertRowid;
            ids.push(Number(rowId));
          }
        }
      } else {
        const count = transaction.prepare('SELECT count(*) count FROM blocks WHERE page_id=?').get(pageId).count;
        const insertAt = Math.max(0, Math.min(Number(count), Number(position) || 0));
        transaction.prepare('UPDATE blocks SET position=position+1000000 WHERE page_id=? AND position>=?').run(pageId, insertAt);
        transaction
          .prepare('UPDATE blocks SET position=position-1000000+? WHERE page_id=? AND position>=?')
          .run(blocks.length, pageId, insertAt + 1000000);
        const insertStmt = transaction.prepare(
          'INSERT INTO blocks(page_id,position,kind,content,checked,media_width,text_styles) VALUES(?,?,?,?,?,?,?)',
        );
        for (let index = 0; index < blocks.length; index++) {
          const block = blocks[index];
          const rowId = insertStmt.run(
            pageId,
            insertAt + index,
            block.kind,
            pastedBlockContent(block, pages),
            block.checked ? 1 : 0,
            clampMediaWidth(block.media_width),
            pastedBlockStylesJson(block),
          ).lastInsertRowid;
          ids.push(Number(rowId));
        }
      }

      normalizeElectronBlockColumns(transaction, pageId);
      touchPage(transaction, pageId);
      return { ids, treeChanged: pages.size > 0 };
    })(state.db);

    if (result.treeChanged) {
      notifyWorkspaceTreeChanged(event.sender);
    }
    return result.ids;
  });

  ipcMain.handle('block:add', (event, { pageId, position, kind = 'text' }) => {
    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Blok eklendi', true);
    return state.db.transaction(() => {
      state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>=?').run(pageId, position);
      const result = state.db.prepare('INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,?,?)').run(pageId, position, kind, '');
      touchPage(state.db, pageId);
      return result.lastInsertRowid;
    })();
  });

  ipcMain.handle('block:split', (event, { id, before, after, beforeStyles, afterStyles }) => {
    const block = one('SELECT * FROM blocks WHERE id=?', id);
    if (!block) return null;
    assertPageWritable(event.sender, block.page_id);
    recordPageUndo(block.page_id, 'Blok eklendi', true);

    return state.db.transaction(() => {
      state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>?').run(block.page_id, block.position);
      state.db.prepare('UPDATE blocks SET content=?,text_styles=? WHERE id=?').run(before, JSON.stringify(beforeStyles || []), id);
      const nextKind = ['todo', 'bullet', 'numbered'].includes(block.kind) ? block.kind : 'text';
      const result = state.db
        .prepare(
          'INSERT INTO blocks(page_id,position,kind,content,checked,column_group,layout_column,column_width,text_styles) VALUES(?,?,?,?,0,?,?,?,?)',
        )
        .run(
          block.page_id,
          block.position + 1,
          nextKind,
          after,
          block.column_group,
          block.layout_column,
          block.column_width,
          JSON.stringify(afterStyles || []),
        );
      touchPage(state.db, block.page_id);
      return result.lastInsertRowid;
    })();
  });

  ipcMain.handle('block:merge', (event, { firstId, secondId }) => {
    const first = one('SELECT * FROM blocks WHERE id=?', firstId);
    const second = one('SELECT * FROM blocks WHERE id=?', secondId);
    if (
      !first ||
      !second ||
      first.page_id !== second.page_id ||
      !MERGEABLE_TEXT_KINDS.has(first.kind) ||
      !MERGEABLE_TEXT_KINDS.has(second.kind) ||
      first.column_group !== second.column_group ||
      first.layout_column !== second.layout_column
    ) {
      throw new Error('These blocks cannot be merged.');
    }

    const next = one(
      'SELECT id FROM blocks WHERE page_id=? AND (position>? OR (position=? AND id>?)) ORDER BY position,id LIMIT 1',
      first.page_id,
      first.position,
      first.position,
      first.id,
    );
    if (!next || next.id !== second.id) {
      throw new Error('Blocks are no longer adjacent.');
    }

    const parseClampedStyles = (raw, length) => {
      try {
        const ranges = JSON.parse(raw || '[]');
        if (!Array.isArray(ranges)) return [];
        return ranges
          .filter(
            (range) =>
              range &&
              Number.isSafeInteger(range.start) &&
              Number.isSafeInteger(range.end) &&
              range.start >= 0 &&
              range.end > range.start &&
              range.start < length &&
              range.style &&
              typeof range.style === 'object' &&
              !Array.isArray(range.style),
          )
          .map((range) => ({ ...range, end: Math.min(range.end, length) }));
      } catch {
        return [];
      }
    };

    assertPageWritable(event.sender, first.page_id);
    const offset = Array.from(first.content).length;
    const styles = [
      ...parseClampedStyles(first.text_styles, offset),
      ...parseClampedStyles(second.text_styles, Array.from(second.content).length).map((range) => ({
        ...range,
        start: range.start + offset,
        end: range.end + offset,
      })),
    ];
    recordPageUndo(first.page_id, 'Bloklar birleştirildi', true);

    return state.db.transaction(() => {
      state.db.prepare('UPDATE blocks SET content=?,text_styles=? WHERE id=?').run(first.content + second.content, JSON.stringify(styles), first.id);
      state.db.prepare('DELETE FROM blocks WHERE id=?').run(second.id);
      state.db.prepare('UPDATE blocks SET position=position-1 WHERE page_id=? AND position>?').run(first.page_id, second.position);
      touchPage(state.db, first.page_id);
      return { id: first.id, offset };
    })();
  });

  ipcMain.handle('block:delete', (event, id) => {
    const block = one('SELECT page_id,position,column_group FROM blocks WHERE id=?', id);
    if (!block) return false;
    assertPageWritable(event.sender, block.page_id);
    recordPageUndo(block.page_id, 'Blok silindi', true);

    state.db.transaction(() => {
      state.db.prepare('DELETE FROM blocks WHERE id=?').run(id);
      normalizeElectronBlockColumns(state.db, block.page_id);
      ensurePageHasBlock(state.db, block.page_id);
      touchPage(state.db, block.page_id);
    })();
    return true;
  });

  ipcMain.handle('block:delete-many', (event, { ids }) => {
    if (!Array.isArray(ids) || !ids.length) return false;
    const unique = parseUniqueBlockIds(ids);
    const { pageId } = loadSinglePageBlocks(unique, 'id,page_id,position,column_group');

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Bloklar silindi', true);
    return state.db.transaction(() => {
      state.db.prepare(`DELETE FROM blocks WHERE id IN (${unique.map(() => '?').join(',')})`).run(...unique);
      normalizeElectronBlockColumns(state.db, pageId);
      ensurePageHasBlock(state.db, pageId);
      touchPage(state.db, pageId);
      return true;
    })();
  });

  ipcMain.handle('block:duplicate', (event, { ids }) => {
    if (!Array.isArray(ids) || !ids.length) return [];
    const unique = parseUniqueBlockIds(ids);
    const { selected, pageId } = loadSinglePageBlocks(unique);
    selected.sort((a, b) => a.position - b.position || a.id - b.id);

    assertPageWritable(event.sender, pageId);
    const toDuplicate = selected.filter((block) => !['database', 'page_link'].includes(block.kind));
    if (!toDuplicate.length) return [];
    recordPageUndo(pageId, 'Bloklar çoğaltıldı', true);

    return state.db.transaction(() => {
      const insertAt = toDuplicate[toDuplicate.length - 1].position + 1;
      state.db.prepare('UPDATE blocks SET position=position+1000000 WHERE page_id=? AND position>=?').run(pageId, insertAt);
      state.db
        .prepare('UPDATE blocks SET position=position-1000000+? WHERE page_id=? AND position>=?')
        .run(toDuplicate.length, pageId, insertAt + 1000000);

      const insertStmt = state.db.prepare(
        'INSERT INTO blocks(page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles) VALUES(?,?,?,?,?,NULL,?,?,?,?)',
      );
      const newIds = [];
      const groupMap = new Map();
      for (let index = 0; index < toDuplicate.length; index++) {
        const block = toDuplicate[index];
        const newId = insertStmt.run(
          pageId,
          insertAt + index,
          block.kind,
          block.content,
          block.checked,
          block.layout_column,
          block.column_width,
          block.media_width,
          block.text_styles,
        ).lastInsertRowid;
        newIds.push(newId);
        if (block.column_group !== null) {
          const newGroup = groupMap.get(block.column_group) ?? newId;
          groupMap.set(block.column_group, newGroup);
          state.db.prepare('UPDATE blocks SET column_group=? WHERE id=?').run(newGroup, newId);
        }
      }

      normalizeElectronBlockColumns(state.db, pageId);
      touchPage(state.db, pageId);
      return newIds;
    })();
  });

  ipcMain.handle('block:move-many', (event, { ids, direction }) => {
    if (!Array.isArray(ids) || !ids.length || ![-1, 1].includes(Number(direction))) return false;
    const unique = parseUniqueBlockIds(ids);
    const { pageId } = loadSinglePageBlocks(unique, 'id,page_id');

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Bloklar taşındı', true);
    return state.db.transaction(() => {
      const order = rows('SELECT id FROM blocks WHERE page_id=? ORDER BY position,id', pageId).map((row) => row.id);
      const selectedSet = new Set(unique);
      if (direction < 0) {
        for (let index = 1; index < order.length; index++) {
          if (selectedSet.has(order[index]) && !selectedSet.has(order[index - 1])) {
            [order[index - 1], order[index]] = [order[index], order[index - 1]];
          }
        }
      } else {
        for (let index = order.length - 2; index >= 0; index--) {
          if (selectedSet.has(order[index]) && !selectedSet.has(order[index + 1])) {
            [order[index], order[index + 1]] = [order[index + 1], order[index]];
          }
        }
      }
      const updateStmt = state.db.prepare('UPDATE blocks SET position=? WHERE id=?');
      order.forEach((id, index) => updateStmt.run(index, id));
      normalizeElectronBlockColumns(state.db, pageId);
      touchPage(state.db, pageId);
      return true;
    })();
  });

  ipcMain.handle('block:set-kind', (event, { ids, kind, clearContent = false, content }) => {
    if (!Array.isArray(ids) || !ids.length || !CONVERTIBLE_TARGET_KINDS.has(kind)) {
      throw new Error('Invalid block type or selection.');
    }
    const unique = parseUniqueBlockIds(ids);
    const { selected, pageId } = loadSinglePageBlocks(unique, 'id,page_id,kind');
    const editable = selected.filter((block) => !NON_CONVERTIBLE_KINDS.has(block.kind));
    if (!editable.length) return 0;

    const editableIds = editable.map((block) => Number(block.id));
    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Blok türü değiştirildi', true);

    return state.db.transaction(() => {
      const result = state.db
        .prepare(
          `UPDATE blocks
           SET kind=?,
               content=CASE WHEN ? THEN ? ELSE content END,
               text_styles=CASE WHEN ? THEN '[]' ELSE text_styles END
           WHERE id IN (${editableIds.map(() => '?').join(',')})`,
        )
        .run(kind, clearContent === true ? 1 : 0, typeof content === 'string' ? content : '', clearContent === true ? 1 : 0, ...editableIds);
      touchPage(state.db, pageId);
      return result.changes;
    })();
  });

  ipcMain.handle('block:move', (event, { id, targetId, after = false }) => {
    const source = one('SELECT id,page_id,position,column_group,layout_column FROM blocks WHERE id=?', id);
    const target = one('SELECT id,page_id,position,column_group,layout_column,column_width FROM blocks WHERE id=?', targetId);
    if (!source || !target || source.page_id !== target.page_id || source.id === target.id) return false;

    assertPageWritable(event.sender, source.page_id);
    recordPageUndo(source.page_id, 'Blok taşındı', true);

    return state.db.transaction(() => {
      const order = rows('SELECT id,column_group,layout_column,column_width FROM blocks WHERE page_id=? ORDER BY position,id', source.page_id);
      const oldGroup = source.column_group;
      const moving = order.findIndex((block) => block.id === source.id);
      const targetIndex = order.findIndex((block) => block.id === target.id);
      if (moving < 0 || targetIndex < 0) return false;

      const next = order.filter((block) => block.id !== source.id);
      const newTarget = next.findIndex((block) => block.id === target.id);
      const insertAt = newTarget + (after ? 1 : 0);
      if ((moving < targetIndex && insertAt === moving) || (moving > targetIndex && insertAt === moving)) {
        return false;
      }

      const item = order[moving];
      item.column_group = target.column_group;
      item.layout_column = target.column_group === null ? 0 : target.layout_column;
      item.column_width = target.column_group === null ? 1 : target.column_width;
      next.splice(insertAt, 0, item);

      if (oldGroup !== null && oldGroup !== target.column_group) {
        const remainingCols = rows(
          'SELECT DISTINCT layout_column FROM blocks WHERE page_id=? AND column_group=? AND id<>?',
          source.page_id,
          oldGroup,
          id,
        );
        if (remainingCols.length < 2) {
          for (const block of next) {
            if (block.column_group === oldGroup) {
              block.column_group = null;
              block.layout_column = 0;
              block.column_width = 1;
            }
          }
        }
      }

      const writeTemp = state.db.prepare('UPDATE blocks SET position=?,column_group=?,layout_column=?,column_width=? WHERE id=?');
      for (let index = 0; index < next.length; index++) {
        writeTemp.run(index + 1000000000, next[index].column_group, next[index].layout_column, next[index].column_width, next[index].id);
      }
      const writePos = state.db.prepare('UPDATE blocks SET position=? WHERE id=?');
      for (let index = 0; index < next.length; index++) {
        writePos.run(index, next[index].id);
      }

      normalizeElectronBlockColumns(state.db, source.page_id);
      touchPage(state.db, source.page_id);
      return true;
    })();
  });

  ipcMain.handle('block:arrange', (event, { pageId, ids, targetId, placement }) => {
    pageId = Number(pageId);
    targetId = Number(targetId);
    if (!PLACEMENT_TYPES.has(placement) || !Array.isArray(ids)) {
      throw new Error('Invalid block placement.');
    }
    const unique = parseUniqueBlockIds(ids);
    const blocks = rows('SELECT id,column_group,layout_column,column_width FROM blocks WHERE page_id=? ORDER BY position,id', pageId);
    const byId = new Map(blocks.map((block) => [Number(block.id), block]));
    if (!byId.has(targetId)) {
      throw new Error('Target block was not found.');
    }

    const sources = unique
      .filter((id) => id !== targetId && byId.has(id))
      .sort((a, b) => blocks.findIndex((block) => Number(block.id) === a) - blocks.findIndex((block) => Number(block.id) === b));
    if (!sources.length) return false;

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Blok taşındı', true);

    return state.db.transaction(() => {
      let order = blocks.map((block) => Number(block.id));
      const columns = new Map(
        blocks.map((block) => [
          Number(block.id),
          {
            group: block.column_group === null ? null : Number(block.column_group),
            column: Number(block.layout_column) || 0,
            width: Number(block.column_width) || 1,
          },
        ]),
      );
      const target = byId.get(targetId);
      if (!order.includes(targetId)) return false;

      if (placement === 'before' || placement === 'after') {
        const targetGroup = target.column_group === null ? null : Number(target.column_group);
        order = order.filter((id) => !sources.includes(id));
        const members = order.filter((id) => id === targetId || (targetGroup !== null && columns.get(id).group === targetGroup));
        const anchor = placement === 'after' ? (members.at(-1) ?? targetId) : (members[0] ?? targetId);
        const insertAt = order.indexOf(anchor) + (placement === 'after' ? 1 : 0);
        sources.forEach((id, index) => {
          order.splice(insertAt + index, 0, id);
          columns.set(id, { ...columns.get(id), group: null, column: 0 });
        });
      } else if (placement === 'before_in_column' || placement === 'after_in_column') {
        const group = target.column_group === null ? null : Number(target.column_group);
        if (group === null) throw new Error('Target block is not in a column.');
        order = order.filter((id) => !sources.includes(id));
        const insertAt = order.indexOf(targetId) + (placement === 'after_in_column' ? 1 : 0);
        sources.forEach((id, index) => {
          order.splice(insertAt + index, 0, id);
          const targetColumn = columns.get(targetId);
          columns.set(id, { ...columns.get(id), group, column: targetColumn.column, width: targetColumn.width });
        });
      } else {
        const group = target.column_group === null ? targetId : Number(target.column_group);
        const targetColumn = Number(target.layout_column) || 0;
        order = order.filter((id) => !sources.includes(id));
        const members = order.filter((id) => id === targetId || columns.get(id).group === group);
        const columnIds = [...new Set(members.map((id) => columns.get(id).column))].sort((a, b) => a - b);
        const boundary = Math.max(0, columnIds.indexOf(targetColumn)) + (placement === 'right' ? 1 : 0);
        const width = 1 / (columnIds.length + sources.length);

        for (const id of members) {
          const colIndex = columnIds.indexOf(columns.get(id).column);
          const shifted = colIndex + (colIndex >= boundary ? sources.length : 0);
          columns.set(id, { ...columns.get(id), group, column: shifted, width });
        }
        const anchor = order.findIndex((id) => members.includes(id));
        sources.forEach((id, index) => {
          columns.set(id, { ...columns.get(id), group, column: boundary + index, width });
          order.splice(anchor + index, 0, id);
        });
      }

      const writeTemp = state.db.prepare(
        'UPDATE blocks SET position=?,column_group=?,layout_column=?,column_width=? WHERE id=? AND page_id=?',
      );
      order.forEach((id, index) => {
        const column = columns.get(id);
        writeTemp.run(index + 1000000000, column.group, column.column, column.width, id, pageId);
      });
      const writePos = state.db.prepare('UPDATE blocks SET position=? WHERE id=? AND page_id=?');
      order.forEach((id, index) => {
        writePos.run(index, id, pageId);
      });

      normalizeElectronBlockColumns(state.db, pageId);
      touchPage(state.db, pageId);
      return true;
    })();
  });

  ipcMain.handle('block:group-columns', (event, { pageId, ids }) => {
    pageId = Number(pageId);
    if (!Array.isArray(ids)) throw new Error('Invalid block selection.');
    const unique = parseUniqueBlockIds(ids);
    if (unique.length < 2 || unique.length > 4) return false;

    const all = rows('SELECT id,kind FROM blocks WHERE page_id=? ORDER BY position,id', pageId);
    const selectedSet = new Set(unique);
    const targets = all.filter((block) => selectedSet.has(Number(block.id)) && block.kind !== 'database').map((block) => Number(block.id));
    if (targets.length < 2 || targets.length > 4) return false;

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Sütunlar düzenlendi', true);
    const group = targets[0];
    const anchor = all.findIndex((block) => Number(block.id) === group);
    const order = all.map((block) => Number(block.id)).filter((id) => !targets.includes(id));
    order.splice(Math.min(anchor, order.length), 0, ...targets);
    const width = 1 / targets.length;

    return state.db.transaction(() => {
      const updatePosition = state.db.prepare('UPDATE blocks SET position=? WHERE id=? AND page_id=?');
      order.forEach((id, index) => updatePosition.run(index, id, pageId));
      const updateColumn = state.db.prepare('UPDATE blocks SET column_group=?,layout_column=?,column_width=? WHERE id=? AND page_id=?');
      targets.forEach((id, index) => updateColumn.run(group, index, width, id, pageId));
      normalizeElectronBlockColumns(state.db, pageId);
      touchPage(state.db, pageId);
      return true;
    })();
  });

  ipcMain.handle('block:make-columns', (event, id) => {
    const source = one('SELECT page_id,position,column_group FROM blocks WHERE id=?', id);
    if (!source || source.column_group !== null) return false;
    assertPageWritable(event.sender, source.page_id);
    recordPageUndo(source.page_id, 'Sütunlar düzenlendi', true);

    return state.db.transaction(() => {
      let next = one('SELECT id,column_group FROM blocks WHERE page_id=? AND position=?', source.page_id, source.position + 1);
      if (next && next.column_group !== null) next = null;
      if (!next) {
        state.db.prepare('UPDATE blocks SET position=position+1 WHERE page_id=? AND position>?').run(source.page_id, source.position);
        const inserted = state.db
          .prepare("INSERT INTO blocks(page_id,position,kind,content,column_group,layout_column,column_width) VALUES(?,?,'text','',?,1,0.5)")
          .run(source.page_id, source.position + 1, id).lastInsertRowid;
        next = { id: inserted };
      }
      state.db.prepare('UPDATE blocks SET column_group=?,layout_column=0,column_width=0.5 WHERE id=?').run(id, id);
      state.db.prepare('UPDATE blocks SET column_group=?,layout_column=1,column_width=0.5 WHERE id=?').run(id, next.id);
      touchPage(state.db, source.page_id);
      return true;
    })();
  });

  ipcMain.handle('block:ungroup-columns', (event, { pageId, group }) => {
    pageId = Number(pageId);
    group = Number(group);
    const members = rows('SELECT id FROM blocks WHERE page_id=? AND column_group=?', pageId, group);
    if (!members.length) return false;

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Sütunlar birleştirildi', true);
    return state.db.transaction(() => {
      state.db.prepare('UPDATE blocks SET column_group=NULL,layout_column=0,column_width=1 WHERE page_id=? AND column_group=?').run(pageId, group);
      normalizeElectronBlockColumns(state.db, pageId);
      touchPage(state.db, pageId);
      return true;
    })();
  });

  ipcMain.handle('block:resize', (event, { pageId, group, boundary, leftWidth, rightWidth }) => {
    pageId = Number(pageId);
    group = Number(group);
    boundary = Number(boundary);
    leftWidth = Number(leftWidth);
    rightWidth = Number(rightWidth);
    if (
      !Number.isSafeInteger(pageId) ||
      !Number.isSafeInteger(group) ||
      !Number.isSafeInteger(boundary) ||
      boundary < 0 ||
      !Number.isFinite(leftWidth) ||
      !Number.isFinite(rightWidth)
    ) {
      throw new Error('Invalid column width.');
    }

    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Sütun genişliği değiştirildi', true);
    const left = Math.max(0.001, Math.min(1, leftWidth));
    const right = Math.max(0.001, Math.min(1, rightWidth));

    return state.db.transaction(() => {
      state.db.prepare('UPDATE blocks SET column_width=? WHERE page_id=? AND column_group=? AND layout_column=?').run(left, pageId, group, boundary);
      state.db.prepare('UPDATE blocks SET column_width=? WHERE page_id=? AND column_group=? AND layout_column=?').run(
        right,
        pageId,
        group,
        boundary + 1,
      );
      touchPage(state.db, pageId);
      return true;
    })();
  });

  ipcMain.handle('block:media-width', (event, { id, width }) => {
    const block = one('SELECT page_id,kind,media_width FROM blocks WHERE id=?', id);
    if (!block || !['image', 'video'].includes(block.kind)) {
      throw new Error('Media block no longer exists.');
    }
    assertPageWritable(event.sender, block.page_id);
    width = Number(width);
    if (!Number.isFinite(width) || width < 0.25 || width > 1) {
      throw new Error('Media width must be between 25% and 100%.');
    }
    if (Math.abs(block.media_width - width) < 0.001) return true;

    recordPageUndo(block.page_id, 'Medya boyutu değiştirildi', true);
    state.db.prepare('UPDATE blocks SET media_width=? WHERE id=?').run(width, id);
    touchPage(state.db, block.page_id);
    return true;
  });
}

module.exports = {
  normalizeElectronBlockColumns,
  registerBlockIpc,
};
