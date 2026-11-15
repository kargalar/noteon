/**
 * Block editor rendering, rich-text formatting, inline links, slash commands,
 * markdown shortcuts, column layout resizing, and block/file drag-and-drop.
 */

const SLASH_COMMAND_TARGETS = [
  { key: 'text', label: 'Text', aliases: ['p', 'paragraph', 'text'], kind: 'text' },
  { key: 'h1', label: 'Heading 1', aliases: ['h1', 'heading 1', 'heading1', '#'], kind: 'h1' },
  { key: 'h2', label: 'Heading 2', aliases: ['h2', 'heading 2', 'heading2', '##'], kind: 'h2' },
  { key: 'h3', label: 'Heading 3', aliases: ['h3', 'heading 3', 'heading3', '###'], kind: 'h3' },
  { key: 'bullet', label: 'Bullet list', aliases: ['bullet', 'list', 'ul', '-'], kind: 'bullet' },
  { key: 'numbered', label: 'Numbered list', aliases: ['numbered', 'ol', 'number', '1.'], kind: 'numbered' },
  { key: 'todo', label: 'To-do list', aliases: ['todo', 'check', 'task', '[]'], kind: 'todo' },
  { key: 'quote', label: 'Quote', aliases: ['quote', 'blockquote', '>'], kind: 'quote' },
  { key: 'callout', label: 'Callout', aliases: ['callout', 'note', 'box'], kind: 'callout' },
  { key: 'code', label: 'Code block', aliases: ['code', 'pre', '```'], kind: 'code' },
  { key: 'divider', label: 'Divider', aliases: ['divider', 'hr', 'line', '---'], kind: 'divider' },
  { key: 'database', label: 'Database (inline)', aliases: ['database', 'db', 'table', 'board', 'gallery'], action: 'database' },
  { key: 'page_link', label: 'Sub-page', aliases: ['page', 'subpage', 'link'], action: 'page_link' },
  { key: 'image', label: 'Image', aliases: ['image', 'img', 'photo', 'picture'], action: 'image' },
  { key: 'video', label: 'Video', aliases: ['video', 'movie', 'mp4'], action: 'video' },
  { key: 'file', label: 'File link', aliases: ['file', 'path', 'attachment'], action: 'file' },
];

const slashCommandTargets = SLASH_COMMAND_TARGETS;

const EDITOR_SHORTCUTS = [
  { key: 'b', shift: false, command: 'bold' },
  { key: 'i', shift: false, command: 'italic' },
  { key: 'u', shift: false, command: 'underline' },
  { key: 's', shift: true, command: 'strikeThrough' },
];

const editorShortcuts = EDITOR_SHORTCUTS;

let activeSlashMenu = null;
let draggedBlockIds = [];

/**
 * Parse stored rich-text style ranges from JSON or array input.
 * @param {string|object[]|null|undefined} raw
 * @returns {Array<{ start: number, end: number, style: object }>}
 */
function styleObject(raw) {
  try {
    const parsed = typeof raw === 'string' ? JSON.parse(raw || '[]') : raw;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (range) =>
        range &&
        typeof range === 'object' &&
        Number.isSafeInteger(range.start) &&
        Number.isSafeInteger(range.end) &&
        range.end > range.start &&
        range.style &&
        typeof range.style === 'object' &&
        !Array.isArray(range.style),
    );
  } catch {
    return [];
  }
}

/**
 * Parse a CSS color or hex string into a byte RGBA tuple `[r, g, b, a]`.
 * @param {string|number[]|null|undefined} value
 * @returns {[number, number, number, number]|null}
 */
function parseRgba(value) {
  if (!value) return null;
  if (Array.isArray(value) && value.length >= 3) {
    return [
      Math.max(0, Math.min(255, Math.round(Number(value[0]) || 0))),
      Math.max(0, Math.min(255, Math.round(Number(value[1]) || 0))),
      Math.max(0, Math.min(255, Math.round(Number(value[2]) || 0))),
      value.length >= 4 ? Math.max(0, Math.min(255, Math.round(Number(value[3]) || 0))) : 255,
    ];
  }
  const text = String(value).trim();
  const hexMatch = text.match(/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i);
  if (hexMatch) {
    let hex = hexMatch[1];
    if (hex.length === 3) {
      hex = hex.split('').map((c) => c + c).join('');
    }
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    const a = hex.length === 8 ? parseInt(hex.slice(6, 8), 16) : 255;
    return [r, g, b, a];
  }
  const rgbMatch = text.match(/^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)(?:\s*,\s*([\d.]+))?\s*\)$/i);
  if (!rgbMatch) return null;
  const alpha = rgbMatch[4] === undefined ? 1 : Number(rgbMatch[4]);
  return [
    Math.max(0, Math.min(255, Math.round(Number(rgbMatch[1])))),
    Math.max(0, Math.min(255, Math.round(Number(rgbMatch[2])))),
    Math.max(0, Math.min(255, Math.round(Number(rgbMatch[3])))),
    Math.max(0, Math.min(255, Math.round(Math.max(0, Math.min(1, alpha)) * 255))),
  ];
}

/**
 * Format an RGBA byte tuple `[r, g, b, a]` as a CSS `rgba(...)` string.
 * @param {number[]|string|null|undefined} value
 * @returns {string}
 */
function rgbaCss(value) {
  if (!value) return '';
  if (typeof value === 'string') return value;
  if (!Array.isArray(value) || value.length < 3) return '';
  const alpha = value.length >= 4 ? Number(value[3]) / 255 : 1;
  return `rgba(${Number(value[0]) || 0}, ${Number(value[1]) || 0}, ${Number(value[2]) || 0}, ${+alpha.toFixed(3)})`;
}

/**
 * Convert an RGBA byte tuple to a `#rrggbb` color input value.
 * @param {number[]} tuple
 * @returns {string}
 */
function rgbaHex(tuple) {
  if (!Array.isArray(tuple) || tuple.length < 3) return '#65875c';
  return `#${tuple
    .slice(0, 3)
    .map((channel) => Math.max(0, Math.min(255, Math.round(Number(channel) || 0))).toString(16).padStart(2, '0'))
    .join('')}`;
}

/**
 * Classify a matched link token as either `'url'` or `'file'`.
 * @param {string} raw
 * @returns {{ kind: 'url'|'file', raw: string, target: string }|null}
 */
function classifyNoteonLink(raw) {
  const token = String(raw || '').trim();
  if (!token) return null;
  if (/^https?:\/\//i.test(token)) {
    return { kind: 'url', raw: token, target: token };
  }
  if (/^www\./i.test(token)) {
    return { kind: 'url', raw: token, target: `https://${token}` };
  }
  if (/^file:\/\//i.test(token) || /^[a-z]:[\\/]/i.test(token) || /^\\\\[^\\\s]+\\[^\\\s]+/.test(token)) {
    return { kind: 'file', raw: token, target: token };
  }
  return null;
}

/**
 * Detect clickable web URLs and local file paths in block text, indexed by Unicode code-point position.
 * @param {string} text
 * @returns {Array<{ start: number, end: number, kind: 'url'|'file', raw: string, target: string }>}
 */
function detectNoteonLinks(text) {
  const source = String(text || '');
  if (!source) return [];

  const codeUnitToChar = new Map();
  let codePointIndex = 0;
  for (let i = 0; i < source.length; ) {
    codeUnitToChar.set(i, codePointIndex);
    const cp = source.codePointAt(i);
    i += cp > 0xffff ? 2 : 1;
    codePointIndex++;
  }
  codeUnitToChar.set(source.length, codePointIndex);

  const pattern = /(https?:\/\/[^\s<>"{}|\\^`[\]]+|www\.[^\s<>"{}|\\^`[\]]+|file:\/\/[^\s<>"{}|^`]+|[a-zA-Z]:[\\/][^\r\n\t<>"{}|^`]+|\\\\[^\\\s]+\\[^\r\n\t<>"{}|^`]+)/g;
  const links = [];
  let match;
  while ((match = pattern.exec(source)) !== null) {
    let raw = match[0];
    while (raw.length && /[.,;:!?\]]$/.test(raw)) {
      raw = raw.slice(0, -1);
    }
    while (raw.endsWith(')') && (raw.match(/\(/g) || []).length < (raw.match(/\)/g) || []).length) {
      raw = raw.slice(0, -1);
    }
    const classified = classifyNoteonLink(raw);
    if (!classified) continue;

    const startUnit = match.index;
    const endUnit = startUnit + raw.length;
    const start = codeUnitToChar.get(startUnit);
    const end = codeUnitToChar.get(endUnit);
    if (start === undefined || end === undefined || end <= start) continue;
    links.push({ start, end, ...classified });
  }
  return links;
}

/**
 * Build inline CSS rules from a Noteon character style object.
 * @param {object} style
 * @returns {string}
 */
function inlineCssFromStyle(style) {
  const declarations = [];
  if (style.bold) declarations.push('font-weight:700');
  if (style.italic) declarations.push('font-style:italic');
  const decorations = [];
  if (style.underline) decorations.push('underline');
  if (style.strikethrough) decorations.push('line-through');
  if (decorations.length) declarations.push(`text-decoration:${decorations.join(' ')}`);
  if (style.font_family === 'Monospace') {
    declarations.push('font-family:Consolas,"Courier New",monospace');
  } else if (style.font_family === 'Serif') {
    declarations.push('font-family:Georgia,"Times New Roman",serif');
  }
  if (style.foreground) {
    const color = rgbaCss(style.foreground);
    if (color) declarations.push(`color:${color}`);
  }
  if (style.background) {
    const bg = rgbaCss(style.background);
    if (bg) declarations.push(`background-color:${bg}`);
  }
  return declarations.join(';');
}

/**
 * Render block text with rich-text style spans and detected Noteon links.
 * @param {string} text
 * @param {string|object[]|null|undefined} raw
 * @param {string} [kind]
 * @returns {string}
 */
function styledHtml(text, raw, kind = 'text') {
  const chars = Array.from(String(text || ''));
  if (!chars.length) return '';

  const styles = styleObject(raw);
  const links = kind === 'code' ? [] : detectNoteonLinks(text);
  const boundaries = new Set([0, chars.length]);

  for (const range of styles) {
    boundaries.add(Math.max(0, Math.min(chars.length, range.start)));
    boundaries.add(Math.max(0, Math.min(chars.length, range.end)));
  }
  for (const link of links) {
    boundaries.add(link.start);
    boundaries.add(link.end);
  }

  const stops = [...boundaries].sort((a, b) => a - b);
  let html = '';

  for (let i = 0; i < stops.length - 1; i++) {
    const from = stops[i];
    const to = stops[i + 1];
    if (to <= from) continue;

    const chunk = esc(chars.slice(from, to).join(''));
    const mergedStyle = Object.assign(
      {},
      ...styles.filter((r) => r.start <= from && r.end >= to).map((r) => r.style),
    );
    const css = inlineCssFromStyle(mergedStyle);
    let rendered = css ? `<span data-noteon-style="1" style="${esc(css)}">${chunk}</span>` : chunk;

    const link = links.find((item) => item.start <= from && item.end >= to);
    if (link) {
      const cls = link.kind === 'file' ? 'noteon-link noteon-link-file' : 'noteon-link';
      const title =
        link.kind === 'file' ? `Click (or Ctrl+Click) to open in Explorer: ${link.raw}` : `Click (or Ctrl+Click) to open: ${link.target}`;
      rendered = `<span class="${cls}" data-noteon-link="${esc(link.target)}" data-noteon-link-kind="${link.kind}" title="${esc(title)}">${rendered}</span>`;
    }
    html += rendered;
  }

  return html;
}

/**
 * Compute code-point character offset within an editor element for a DOM node + offset.
 * @param {HTMLElement} edit
 * @param {Node} node
 * @param {number} offset
 * @returns {number}
 */
function editorOffsetAt(edit, node, offset) {
  if (!edit || !node) return 0;
  try {
    const range = document.createRange();
    range.selectNodeContents(edit);
    range.setEnd(node, offset);
    return Array.from(range.toString()).length;
  } catch {
    return 0;
  }
}

/**
 * Place the DOM selection caret or range inside an editor by code-point offsets.
 * @param {HTMLElement} edit
 * @param {number} startOffset
 * @param {number} [endOffset]
 */
function setEditorSelectionOffsets(edit, startOffset, endOffset = startOffset) {
  const selection = window.getSelection();
  if (!selection || !edit) return;

  const locatePoint = (targetCharIndex) => {
    let remaining = Math.max(0, targetCharIndex);
    const walker = document.createTreeWalker(edit, NodeFilter.SHOW_TEXT);
    let current;
    let lastNode = edit;
    let lastUnitOffset = 0;

    while ((current = walker.nextNode())) {
      const chars = Array.from(current.nodeValue || '');
      if (remaining <= chars.length) {
        const unitOffset = chars.slice(0, remaining).join('').length;
        return { node: current, offset: unitOffset };
      }
      remaining -= chars.length;
      lastNode = current;
      lastUnitOffset = (current.nodeValue || '').length;
    }
    return { node: lastNode, offset: lastUnitOffset };
  };

  const start = locatePoint(startOffset);
  const end = endOffset === startOffset ? start : locatePoint(endOffset);
  const range = document.createRange();
  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

/**
 * Read current code-point `{ start, end }` selection offsets inside an editor element.
 * @param {HTMLElement} edit
 * @returns {{ start: number, end: number }|null}
 */
function getEditorSelectionOffsets(edit) {
  const selection = window.getSelection();
  if (!selection || !selection.rangeCount) return null;
  const range = selection.getRangeAt(0);
  if (!edit.contains(range.startContainer) || !edit.contains(range.endContainer)) return null;
  return {
    start: editorOffsetAt(edit, range.startContainer, range.startOffset),
    end: editorOffsetAt(edit, range.endContainer, range.endOffset),
  };
}

/**
 * Extract canonical Noteon style ranges from the DOM inside a contentEditable editor.
 * @param {HTMLElement} edit
 * @returns {Array<{ start: number, end: number, style: object }>}
 */
function extractStyleRanges(edit) {
  if (!edit) return [];
  const baseComputed = getComputedStyle(edit);
  const baseColor = JSON.stringify(parseRgba(baseComputed.color));
  const charStyles = [];

  const walker = document.createTreeWalker(edit, NodeFilter.SHOW_TEXT);
  let textNode;
  while ((textNode = walker.nextNode())) {
    const chars = Array.from(textNode.nodeValue || '');
    if (!chars.length) continue;

    let style = {};
    const parent = textNode.parentElement;
    if (parent && parent !== edit && !parent.classList.contains('noteon-link') && parent.closest('[data-noteon-style], b, strong, i, em, u, s, strike, del, font, span')) {
      const comp = getComputedStyle(parent);
      const weight = Number(comp.fontWeight);
      if (comp.fontWeight === 'bold' || (Number.isFinite(weight) && weight >= 600)) {
        style.bold = true;
      }
      if (comp.fontStyle === 'italic') {
        style.italic = true;
      }
      if ((comp.textDecorationLine || '').includes('underline') && !parent.closest('.noteon-link')) {
        style.underline = true;
      }
      if ((comp.textDecorationLine || '').includes('line-through')) {
        style.strikethrough = true;
      }
      const family = (comp.fontFamily || '').toLowerCase();
      if (family.includes('consolas') || family.includes('courier') || family.includes('monospace')) {
        style.font_family = 'Monospace';
      } else if (family.includes('georgia') || family.includes('times') || family.includes('serif')) {
        if (!family.includes('sans-serif')) {
          style.font_family = 'Serif';
        }
      }
      const fg = parseRgba(comp.color);
      if (fg && JSON.stringify(fg) !== baseColor) {
        style.foreground = fg;
      }
      const bg = parseRgba(comp.backgroundColor);
      if (bg && bg[3] > 0) {
        style.background = bg;
      }
    }

    for (let i = 0; i < chars.length; i++) {
      charStyles.push(style);
    }
  }

  const ranges = [];
  for (let i = 0; i < charStyles.length; ) {
    const current = charStyles[i];
    const sig = JSON.stringify(current);
    let end = i + 1;
    while (end < charStyles.length && JSON.stringify(charStyles[end]) === sig) {
      end++;
    }
    if (Object.keys(current).length) {
      ranges.push({ start: i, end, style: current });
    }
    i = end;
  }
  return ranges;
}

/**
 * Find the `.noteon-link` span under pointer coordinates inside an editor.
 * @param {HTMLElement} editor
 * @param {number} x
 * @param {number} y
 * @returns {{ kind: string, target: string, raw: string }|null}
 */
function linkAtEditorPoint(editor, x, y) {
  const hit = document.elementFromPoint(x, y);
  const span = hit?.closest?.('.noteon-link');
  if (!span || !editor.contains(span)) return null;
  return {
    kind: span.dataset.noteonLinkKind || 'url',
    target: span.dataset.noteonLink || span.textContent || '',
    raw: span.textContent || span.dataset.noteonLink || '',
  };
}

/**
 * Open a detected Noteon link using the appropriate OS shell handler.
 * @param {{ kind: string, target: string }} link
 */
async function openNoteonLinkTarget(link) {
  if (!link?.target) return;
  if (link.kind === 'file') {
    await window.noteon.openFileExplorer(link.target);
  } else {
    await window.noteon.openUrl(link.target);
  }
}

/**
 * Show a floating context menu for opening or copying a detected Noteon link.
 * @param {MouseEvent} event
 * @param {{ kind: string, target: string, raw: string }} link
 */
function showNoteonLinkMenu(event, link) {
  openFloatingContextMenu(
    event,
    [
      [link.kind === 'file' ? 'Show file in Explorer' : 'Open link in browser', () => openNoteonLinkTarget(link)],
      ['Copy link', () => window.noteon.copyText(link.target)],
    ],
    'noteon-context-menu',
  );
}

/**
 * Install click navigation for inline links on an editor without blocking text selection.
 * @param {HTMLElement} editor
 * @param {object} block
 */
function installLinkInteractions(editor, block) {
  let pointerDownPoint = null;
  editor.addEventListener('pointerdown', (event) => {
    pointerDownPoint = event.button === 0 ? { x: event.clientX, y: event.clientY } : null;
  });
  editor.addEventListener('click', (event) => {
    if (block.kind === 'code') return;
    const link = linkAtEditorPoint(editor, event.clientX, event.clientY);
    if (!link) return;
    const moved =
      pointerDownPoint &&
      (Math.abs(event.clientX - pointerDownPoint.x) > 4 || Math.abs(event.clientY - pointerDownPoint.y) > 4);
    const hasSelection = (window.getSelection()?.toString() || '').length > 0;
    if (moved || hasSelection) return;
    if (event.ctrlKey || event.metaKey || event.detail === 1) {
      openNoteonLinkTarget(link).catch(showDockError);
    }
  });
}

/**
 * Queue a block change into `pendingBlocks` and schedule a debounced SQLite write.
 * @param {object} block
 */
function scheduleBlockSave(block) {
  pendingBlocks.set(block.id, block);
  setSaveState('Saving…');
  clearTimeout(blockSaveTimer);
  blockSaveTimer = setTimeout(() => {
    flushBlockSaves().catch(() => {});
  }, 350);
}

/**
 * Focus the editable element for a block ID at the start or end.
 * @param {number} id
 * @param {boolean} [atEnd]
 */
function focusBlockById(id, atEnd = false) {
  const editor = document.querySelector(`[data-block-id="${id}"] .block-editor`);
  if (!editor) return;
  editor.focus();
  const length = Array.from(editor.textContent || '').length;
  setEditorSelectionOffsets(editor, atEnd ? length : 0);
}

/**
 * Focus a block's editor at an exact code-point offset.
 * @param {number} id
 * @param {number} offset
 */
function focusBlockAtOffset(id, offset) {
  const editor = document.querySelector(`[data-block-id="${id}"] .block-editor`);
  if (!editor) return;
  editor.focus();
  const max = Array.from(editor.textContent || '').length;
  setEditorSelectionOffsets(editor, Math.max(0, Math.min(max, offset)));
}

/**
 * Focus the Nth editor element on the active page.
 * @param {number} index
 */
function focusBlock(index) {
  const editors = document.querySelectorAll('#content .block-editor');
  const target = editors[index] || editors[0];
  target?.focus();
}

/**
 * Remove any visible slash-command popup menu.
 */
function closeSlashCommandMenu() {
  if (activeSlashMenu) {
    activeSlashMenu.remove();
    activeSlashMenu = null;
  }
}

/**
 * Filter slash commands matching an editor's leading `/query` text.
 * @param {string} content
 * @returns {object[]}
 */
function slashCommandMatches(content) {
  const raw = String(content || '');
  if (!raw.startsWith('/')) return [];
  const query = raw.slice(1).trim().toLowerCase();
  if (!query) return SLASH_COMMAND_TARGETS;
  return SLASH_COMMAND_TARGETS.filter(
    (item) =>
      item.label.toLowerCase().includes(query) ||
      item.key.includes(query) ||
      item.aliases.some((alias) => alias.includes(query)),
  );
}

/**
 * Execute a selected slash command on a block element.
 * @param {HTMLElement} editor
 * @param {object} command
 */
async function runSlashCommand(editor, command) {
  closeSlashCommandMenu();
  const blockEl = editor.closest('[data-block-id]');
  const blockId = Number(blockEl?.dataset.blockId);
  const block = activePage?.blocks.find((item) => item.id === blockId);
  if (!block || !activeId) return;

  await flushLocalEdits();

  if (command.action === 'database') {
    await window.noteon.createDatabaseBlock({ pageId: activeId, position: block.position, replaceBlock: block.id });
    await refresh();
    await openPage(activeId);
    return;
  }

  if (command.action === 'page_link') {
    await window.noteon.createLinked({ pageId: activeId, position: block.position, kind: 'page_link', replaceBlock: block.id });
    await refresh();
    await openPage(activeId);
    return;
  }

  if (command.action === 'image' || command.action === 'video') {
    const createdId = await window.noteon.importMedia({ pageId: activeId, position: block.position, kind: command.action });
    if (createdId) {
      if ((block.content || '').startsWith('/')) {
        await window.noteon.deleteBlock(block.id);
      }
      await openPage(activeId);
    }
    return;
  }

  if (command.action === 'file') {
    const picked = await window.noteon.chooseBlockFile();
    if (!picked) return;
    block.content = picked;
    block.text_styles = [];
    await window.noteon.saveBlock({ id: block.id, content: picked, checked: block.checked, text_styles: [] });
    await openPage(activeId);
    return;
  }

  await window.noteon.setBlockKind({
    ids: [block.id],
    kind: command.kind,
    clearContent: true,
    content: '',
  });
  await openPage(activeId);
  if (command.kind !== 'divider') {
    focusBlockById(block.id, false);
  }
}

/**
 * Show or update the slash-command dropdown below the active editor when content starts with `/`.
 * @param {HTMLElement} editor
 */
function updateSlashCommandMenu(editor) {
  closeSlashCommandMenu();
  if (!pageWriteAccess || !editor) return;
  const blockRow = editor.closest('.block');
  if (!blockRow) return;

  const matches = slashCommandMatches(editor.textContent || '');
  if (!matches.length) return;

  const pop = document.createElement('div');
  pop.className = 'command-pop';
  pop.dataset.selectedIndex = '0';

  matches.forEach((cmd, idx) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'command-option';
    btn.textContent = cmd.label;
    btn.setAttribute('aria-selected', idx === 0 ? 'true' : 'false');
    btn.onmousedown = (event) => {
      event.preventDefault();
      runSlashCommand(editor, cmd).catch(showDockError);
    };
    pop.append(btn);
  });

  blockRow.append(pop);
  activeSlashMenu = pop;
  pop._commands = matches;
  pop._editor = editor;
}

/**
 * Add a new block at `position` and focus it once rendered.
 * @param {number} position
 * @param {string} [kind]
 */
async function addBlock(position, kind = 'text') {
  if (!activeId || !pageWriteAccess) return;
  await flushLocalEdits();
  const id = await window.noteon.addBlock({ pageId: activeId, position, kind });
  await openPage(activeId);
  if (id) focusBlockById(id, false);
}

/**
 * Delete a block from the active page and focus its neighbor.
 * @param {object} block
 * @param {number} index
 */
async function deleteBlock(block, index) {
  if (!activeId || !pageWriteAccess) return;
  await flushLocalEdits();
  await window.noteon.deleteBlock(block.id);
  selectedBlockIds.delete(block.id);
  await openPage(activeId);
  const nextIndex = Math.max(0, Math.min(index, (activePage?.blocks?.length || 1) - 1));
  focusBlock(nextIndex);
}

/**
 * Copy or cut currently selected blocks via Electron's clipboard.
 * @param {boolean} [cut]
 */
async function copySelectedBlocks(cut = false) {
  if (!activeId || !selectedBlockIds.size) return;
  const ids = [...selectedBlockIds];
  await flushLocalEdits();
  await window.noteon.copyBlocks({ ids });
  if (cut && pageWriteAccess) {
    await window.noteon.deleteBlocks({ ids });
    selectedBlockIds.clear();
    blockSelectionAnchorId = null;
    await openPage(activeId);
  }
}

/**
 * Paste clipboard blocks/markdown into the active page below the current selection or at the end.
 */
async function pasteBlocksIntoPage() {
  if (!activeId || !pageWriteAccess) return;
  await flushLocalEdits();

  let position = activePage?.blocks?.length || 0;
  if (selectedBlockIds.size && activePage?.blocks?.length) {
    let maxPos = -1;
    for (const block of activePage.blocks) {
      if (selectedBlockIds.has(block.id)) {
        maxPos = Math.max(maxPos, block.position);
      }
    }
    if (maxPos >= 0) position = maxPos + 1;
  }

  const createdIds = await window.noteon.pasteBlocks({ pageId: activeId, position });
  if (!createdIds?.length) return;
  selectedBlockIds = new Set(createdIds.map(Number));
  selectedBlockPageId = activeId;
  await openPage(activeId);
}

/**
 * Apply a bulk formatting operation (`toggle:bold`, `font`, `text-color`, `highlight`, `clear`) to selected blocks.
 * @param {string} operation
 * @param {string|number[]|null} value
 */
async function formatSelectedBlocks(operation, value) {
  if (!activeId || !selectedBlockIds.size || !pageWriteAccess) return;
  await flushLocalEdits();
  await window.noteon.formatBlocks({
    pageId: activeId,
    ids: [...selectedBlockIds],
    operation,
    value,
  });
  await openPage(activeId);
}

/**
 * Populate inline/selection formatting controls on a block toolbar or selection bar.
 * @param {HTMLElement} toolbar
 * @param {{ applyInline?: (op: string, val: any) => void, onComplete?: () => void }} [options]
 */
function addBlockFormatTools(toolbar, options = {}) {
  const apply =
    options.applyInline ||
    ((op, val) => {
      formatSelectedBlocks(op, val).catch(showDockError);
    });

  const toggles = [
    ['B', 'toggle:bold', 'Bold (Ctrl+B)'],
    ['I', 'toggle:italic', 'Italic (Ctrl+I)'],
    ['U', 'toggle:underline', 'Underline (Ctrl+U)'],
    ['S', 'toggle:strikethrough', 'Strikethrough (Ctrl+Shift+S)'],
  ];

  for (const [label, op, title] of toggles) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = label;
    btn.title = title;
    btn.onmousedown = (event) => {
      event.preventDefault();
      apply(op, null);
    };
    toolbar.append(btn);
  }

  const fontSelect = document.createElement('select');
  fontSelect.title = 'Font family';
  for (const [value, label] of [
    ['', 'Default font'],
    ['Monospace', 'Monospace'],
    ['Serif', 'Serif'],
  ]) {
    const opt = document.createElement('option');
    opt.value = value;
    opt.textContent = label;
    fontSelect.append(opt);
  }
  fontSelect.onchange = () => {
    apply('font', fontSelect.value || null);
    fontSelect.value = '';
  };
  toolbar.append(fontSelect);

  const makeColorControl = (label, op, defaultHex) => {
    const wrap = document.createElement('label');
    wrap.className = 'format-color';
    wrap.title = op === 'text-color' ? 'Text color' : 'Highlight color';
    const text = document.createElement('span');
    text.textContent = label;
    const input = document.createElement('input');
    input.type = 'color';
    input.value = defaultHex;
    input.onchange = () => apply(op, input.value);
    wrap.append(text, input);
    return wrap;
  };

  toolbar.append(makeColorControl('A', 'text-color', '#c93c37'));
  toolbar.append(makeColorControl('Bg', 'highlight', '#f4e07c'));

  const clearBtn = document.createElement('button');
  clearBtn.type = 'button';
  clearBtn.textContent = 'Clear';
  clearBtn.title = 'Clear formatting';
  clearBtn.onmousedown = (event) => {
    event.preventDefault();
    apply('clear', null);
  };
  toolbar.append(clearBtn);
}

/**
 * Persist current DOM text & extracted rich styles for an editor element immediately.
 * @param {HTMLElement} editor
 * @param {object} block
 */
function syncEditorBlockState(editor, block) {
  block.content = editor.textContent || '';
  block.text_styles = extractStyleRanges(editor);
  scheduleBlockSave(block);
}

/**
 * Apply an inline formatting operation (`toggle:*`, `font`, `text-color`, `highlight`, `clear`)
 * to the currently highlighted text inside an editor element.
 * @param {HTMLElement} editor
 * @param {object} block
 * @param {string} operation
 * @param {string|null} value
 */
function applyInlineFormattingToEditor(editor, block, operation, value) {
  if (!pageWriteAccess || block.kind === 'code') return;
  const offsets = getEditorSelectionOffsets(editor);
  if (!offsets || offsets.end <= offsets.start) return;

  if (operation.startsWith('toggle:')) {
    const cmdMap = {
      'toggle:bold': 'bold',
      'toggle:italic': 'italic',
      'toggle:underline': 'underline',
      'toggle:strikethrough': 'strikeThrough',
    };
    document.execCommand(cmdMap[operation]);
    syncEditorBlockState(editor, block);
    return;
  }

  const chars = Array.from(editor.textContent || '');
  const existing = styleObject(block.text_styles);
  const perChar = chars.map((_, idx) =>
    Object.assign({}, ...existing.filter((r) => idx >= r.start && idx < r.end).map((r) => ({ ...r.style }))),
  );

  for (let i = offsets.start; i < offsets.end; i++) {
    const cell = perChar[i];
    if (!cell) continue;
    if (operation === 'font') {
      if (value) cell.font_family = value;
      else delete cell.font_family;
    } else if (operation === 'text-color') {
      const rgba = parseRgba(value);
      if (rgba) cell.foreground = rgba;
      else delete cell.foreground;
    } else if (operation === 'highlight') {
      const rgba = parseRgba(value);
      if (rgba) {
        rgba[3] = Math.round(0.45 * 255);
        cell.background = rgba;
      } else {
        delete cell.background;
      }
    } else if (operation === 'clear') {
      delete cell.bold;
      delete cell.italic;
      delete cell.underline;
      delete cell.strikethrough;
      delete cell.font_family;
      delete cell.foreground;
      delete cell.background;
    }
  }

  const ranges = [];
  for (let i = 0; i < perChar.length; ) {
    const cur = perChar[i];
    const sig = JSON.stringify(cur);
    let end = i + 1;
    while (end < perChar.length && JSON.stringify(perChar[end]) === sig) end++;
    if (Object.keys(cur).length) ranges.push({ start: i, end, style: cur });
    i = end;
  }

  block.text_styles = ranges;
  block.content = chars.join('');
  editor.innerHTML = styledHtml(block.content, block.text_styles, block.kind);
  setEditorSelectionOffsets(editor, offsets.start, offsets.end);
  scheduleBlockSave(block);
}

/**
 * Convert a plain text block via leading markdown prefixes (`# `, `- `, `1. `, `[] `, `> `, `---`).
 * @param {object} block
 * @param {string} rawText
 * @returns {Promise<boolean>}
 */
async function tryMarkdownBlockTransform(block, rawText) {
  if (block.kind !== 'text') return false;
  let kind = null;
  let rest = '';
  let markChecked = false;

  if (rawText === '---') {
    kind = 'divider';
    rest = '';
  } else {
    let match;
    if ((match = rawText.match(/^(#{1,3}) (.*)$/su))) {
      kind = `h${match[1].length}`;
      rest = match[2];
    } else if ((match = rawText.match(/^(?:- |\* )(.*)$/su))) {
      kind = 'bullet';
      rest = match[1];
    } else if ((match = rawText.match(/^1\. (.*)$/su))) {
      kind = 'numbered';
      rest = match[1];
    } else if ((match = rawText.match(/^\[\s?\] (.*)$/su))) {
      kind = 'todo';
      rest = match[1];
    } else if ((match = rawText.match(/^\[[xX]\] (.*)$/su))) {
      kind = 'todo';
      rest = match[1];
      markChecked = true;
    } else if ((match = rawText.match(/^> (.*)$/su))) {
      kind = 'quote';
      rest = match[1];
    }
  }

  if (!kind) return false;

  pendingBlocks.delete(block.id);
  clearTimeout(blockSaveTimer);
  await flushLocalEdits();
  await window.noteon.setBlockKind({
    ids: [block.id],
    kind,
    clearContent: true,
    content: rest,
  });
  if (markChecked) {
    await window.noteon.setBlocksChecked({ ids: [block.id], checked: true });
  }
  await openPage(activeId);
  if (kind !== 'divider') {
    focusBlockById(block.id, true);
  }
  return true;
}

/**
 * Handle keyboard shortcuts, slash menu navigation, Enter splitting, and Backspace merging inside a block editor.
 * @param {KeyboardEvent} e
 * @param {object} b
 * @param {number} index
 */
async function blockKey(e, b, index) {
  if (activeSlashMenu && activeSlashMenu._editor === e.currentTarget) {
    const options = [...activeSlashMenu.querySelectorAll('.command-option')];
    const currentIdx = Number(activeSlashMenu.dataset.selectedIndex || 0);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = (currentIdx + (e.key === 'ArrowDown' ? 1 : -1) + options.length) % options.length;
      activeSlashMenu.dataset.selectedIndex = String(next);
      options.forEach((btn, idx) => btn.setAttribute('aria-selected', idx === next ? 'true' : 'false'));
      options[next]?.scrollIntoView({ block: 'nearest' });
      return;
    }
    if ((e.key === 'Enter' || e.key === 'Tab') && !e.shiftKey) {
      e.preventDefault();
      const command = activeSlashMenu._commands?.[currentIdx];
      if (command) await runSlashCommand(e.currentTarget, command);
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      closeSlashCommandMenu();
      return;
    }
  }

  const keyLower = e.key.toLowerCase();
  if ((e.ctrlKey || e.metaKey) && !e.altKey) {
    const shortcut = EDITOR_SHORTCUTS.find((s) => s.key === keyLower && Boolean(s.shift) === Boolean(e.shiftKey));
    if (shortcut && b.kind !== 'code') {
      e.preventDefault();
      document.execCommand(shortcut.command);
      syncEditorBlockState(e.currentTarget, b);
      return;
    }
  }

  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing && b.kind !== 'code') {
    e.preventDefault();
    const editor = e.currentTarget;
    const offsets = getEditorSelectionOffsets(editor) || { start: Array.from(editor.textContent || '').length, end: 0 };
    const chars = Array.from(editor.textContent || '');
    const styles = extractStyleRanges(editor);

    const beforeChars = chars.slice(0, offsets.start);
    const afterChars = chars.slice(Math.max(offsets.start, offsets.end));
    const cutLen = afterChars.length ? Math.max(offsets.start, offsets.end) : offsets.start;

    const beforeStyles = styles
      .filter((r) => r.start < offsets.start)
      .map((r) => ({ ...r, end: Math.min(offsets.start, r.end) }))
      .filter((r) => r.end > r.start);

    const afterStyles = styles
      .filter((r) => r.end > cutLen)
      .map((r) => ({
        start: Math.max(0, r.start - cutLen),
        end: r.end - cutLen,
        style: r.style,
      }))
      .filter((r) => r.end > r.start);

    pendingBlocks.delete(b.id);
    clearTimeout(blockSaveTimer);
    await flushLocalEdits();
    const newId = await window.noteon.splitBlock({
      id: b.id,
      before: beforeChars.join(''),
      after: afterChars.join(''),
      beforeStyles,
      afterStyles,
    });
    await openPage(activeId);
    if (newId) focusBlockById(newId, false);
    return;
  }

  if (e.key === 'Backspace' && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
    const editor = e.currentTarget;
    const offsets = getEditorSelectionOffsets(editor);
    const isEmpty = !(editor.textContent || '').length;
    if (offsets && offsets.start === 0 && offsets.end === 0) {
      if (b.kind !== 'text' && isEmpty) {
        e.preventDefault();
        pendingBlocks.delete(b.id);
        clearTimeout(blockSaveTimer);
        await flushLocalEdits();
        await window.noteon.setBlockKind({ ids: [b.id], kind: 'text', clearContent: true, content: '' });
        await openPage(activeId);
        focusBlockById(b.id, false);
        return;
      }

      const prev = activePage?.blocks?.[index - 1];
      if (
        prev &&
        prev.column_group === b.column_group &&
        prev.layout_column === b.layout_column &&
        !['divider', 'database', 'page_link', 'image', 'video'].includes(prev.kind) &&
        !['divider', 'database', 'page_link', 'image', 'video'].includes(b.kind)
      ) {
        e.preventDefault();
        await flushLocalEdits();
        const merged = await window.noteon.mergeBlocks({ firstId: prev.id, secondId: b.id });
        await openPage(activeId);
        if (merged?.id) {
          focusBlockAtOffset(merged.id, merged.offset || 0);
        }
        return;
      }

      if (isEmpty && (activePage?.blocks?.length || 0) > 1) {
        e.preventDefault();
        await deleteBlock(b, index);
        return;
      }
    }
  }

  if (e.key === 'ArrowUp' && !e.shiftKey && !e.altKey) {
    const offsets = getEditorSelectionOffsets(e.currentTarget);
    if (offsets && offsets.start === 0 && offsets.end === 0) {
      const prevEditors = [...document.querySelectorAll('#content .block-editor')];
      const pos = prevEditors.indexOf(e.currentTarget);
      if (pos > 0) {
        e.preventDefault();
        const prevEditor = prevEditors[pos - 1];
        prevEditor.focus();
        setEditorSelectionOffsets(prevEditor, Array.from(prevEditor.textContent || '').length);
      }
    }
  } else if (e.key === 'ArrowDown' && !e.shiftKey && !e.altKey) {
    const offsets = getEditorSelectionOffsets(e.currentTarget);
    const total = Array.from(e.currentTarget.textContent || '').length;
    if (offsets && offsets.start >= total && offsets.end >= total) {
      const editors = [...document.querySelectorAll('#content .block-editor')];
      const pos = editors.indexOf(e.currentTarget);
      if (pos >= 0 && pos < editors.length - 1) {
        e.preventDefault();
        const nextEditor = editors[pos + 1];
        nextEditor.focus();
        setEditorSelectionOffsets(nextEditor, 0);
      }
    }
  }
}

/**
 * Determine which block IDs should be moved when starting a block drag operation.
 * @param {number} blockId
 * @returns {number[]}
 */
function draggedBlockSelection(blockId) {
  if (selectedBlockIds.has(blockId) && selectedBlockIds.size > 1) {
    return [...selectedBlockIds];
  }
  return [blockId];
}

/**
 * Clear all drag-and-drop highlight classes on block nodes.
 */
function clearBlockDropIndicators() {
  document
    .querySelectorAll('#content .block.drop-before, #content .block.drop-after, #content .block.drop-left, #content .block.drop-right')
    .forEach((node) => node.classList.remove('drop-before', 'drop-after', 'drop-left', 'drop-right'));
}

/**
 * Compute drop placement string from pointer position over a target block row.
 * @param {DragEvent} event
 * @param {HTMLElement} row
 * @param {object} targetBlock
 * @returns {'before'|'after'|'before_in_column'|'after_in_column'|'left'|'right'}
 */
function resolveBlockDropPlacement(event, row, targetBlock) {
  const rect = row.getBoundingClientRect();
  const xFrac = rect.width ? (event.clientX - rect.left) / rect.width : 0.5;
  const yFrac = rect.height ? (event.clientY - rect.top) / rect.height : 0.5;
  if (xFrac <= 0.22) return 'left';
  if (xFrac >= 0.78) return 'right';
  if (targetBlock.column_group !== null) {
    return yFrac < 0.5 ? 'before_in_column' : 'after_in_column';
  }
  return yFrac < 0.5 ? 'before' : 'after';
}

/**
 * Toggle block selection state, supporting Shift+Click range selection on the active page.
 * @param {MouseEvent} event
 * @param {object} block
 */
function toggleBlockSelection(event, block) {
  if (selectedBlockPageId !== activeId) {
    selectedBlockIds.clear();
    selectedBlockPageId = activeId;
    blockSelectionAnchorId = null;
  }

  const orderedIds = (activePage?.blocks || []).map((item) => item.id);
  if (event.shiftKey && blockSelectionAnchorId !== null) {
    const from = orderedIds.indexOf(blockSelectionAnchorId);
    const to = orderedIds.indexOf(block.id);
    if (from >= 0 && to >= 0) {
      const [start, end] = from < to ? [from, to] : [to, from];
      selectedBlockIds = new Set(orderedIds.slice(start, end + 1));
    }
  } else {
    if (selectedBlockIds.has(block.id)) {
      selectedBlockIds.delete(block.id);
    } else {
      selectedBlockIds.add(block.id);
    }
    blockSelectionAnchorId = block.id;
  }

  if (activePage) {
    const blocksRoot = $('#content .blocks');
    if (blocksRoot) {
      openPage(activeId, { suppressHistory: true, skipTab: true }).catch(showDockError);
    }
  }
}

/**
 * Build the leading prefix control/label for a block (`todo` checkbox, number, bullet, quote, etc.).
 * @param {object} b
 * @param {number|null} numberIndex
 * @returns {HTMLElement}
 */
function buildBlockPrefix(b, numberIndex) {
  const prefix = document.createElement('div');
  prefix.className = 'block-prefix';
  prefix.draggable = pageWriteAccess;
  prefix.title = pageWriteAccess ? 'Drag block to reorder or create columns' : '';

  if (b.kind === 'todo') {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = 'todo';
    box.checked = Boolean(b.checked);
    box.onchange = () => {
      b.checked = box.checked ? 1 : 0;
      scheduleBlockSave(b);
      flushBlockSaves().catch(showDockError);
    };
    prefix.append(box);
  } else if (b.kind === 'numbered') {
    prefix.textContent = `${numberIndex || 1}.`;
  } else {
    prefix.textContent = kinds[b.kind] || '⋮⋮';
  }

  prefix.ondragstart = (event) => {
    if (!pageWriteAccess) {
      event.preventDefault();
      return;
    }
    draggedBlockIds = draggedBlockSelection(b.id);
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('application/x-noteon-block', JSON.stringify(draggedBlockIds));
  };
  prefix.ondragend = () => {
    draggedBlockIds = [];
    clearBlockDropIndicators();
  };

  return prefix;
}

/**
 * Build hover action controls (select, format popover, kind selector, duplicate, move, columns, delete) for a block.
 * @param {object} b
 * @param {number} index
 * @param {HTMLElement|null} editor
 * @returns {HTMLElement}
 */
function buildBlockControls(b, index, editor) {
  const controls = document.createElement('div');
  controls.className = 'block-controls';

  const selectBtn = document.createElement('button');
  selectBtn.type = 'button';
  selectBtn.dataset.selectBlock = String(b.id);
  selectBtn.title = 'Select block (Shift+Click for range)';
  selectBtn.textContent = selectedBlockIds.has(b.id) ? '☑' : '☐';
  selectBtn.onclick = (event) => toggleBlockSelection(event, b);
  controls.append(selectBtn);

  if (editor && b.kind !== 'code') {
    const formatTools = document.createElement('div');
    formatTools.className = 'format-tools';
    formatTools.hidden = true;
    addBlockFormatTools(formatTools, {
      applyInline: (op, val) => applyInlineFormattingToEditor(editor, b, op, val),
    });

    const formatToggle = document.createElement('button');
    formatToggle.type = 'button';
    formatToggle.textContent = 'Aa';
    formatToggle.title = 'Format selected text';
    formatToggle.onmousedown = (event) => {
      event.preventDefault();
      formatTools.hidden = !formatTools.hidden;
    };
    controls.append(formatToggle, formatTools);
  }

  if (!['database', 'page_link', 'image', 'video'].includes(b.kind)) {
    const kindSelect = document.createElement('select');
    kindSelect.className = 'block-kind-select';
    kindSelect.title = 'Change block type';
    for (const [value, label] of blockTypeChoices) {
      const opt = document.createElement('option');
      opt.value = value;
      opt.textContent = label;
      opt.selected = b.kind === value;
      kindSelect.append(opt);
    }
    kindSelect.onchange = async () => {
      await flushLocalEdits();
      await window.noteon.setBlockKind({ ids: [b.id], kind: kindSelect.value });
      await openPage(activeId);
    };
    controls.append(kindSelect);
  }

  if (!['database', 'page_link'].includes(b.kind)) {
    const dupBtn = document.createElement('button');
    dupBtn.type = 'button';
    dupBtn.dataset.duplicateBlock = String(b.id);
    dupBtn.title = 'Duplicate block';
    dupBtn.textContent = '⧉';
    dupBtn.onclick = async () => {
      await flushLocalEdits();
      await window.noteon.duplicateBlocks({ ids: [b.id] });
      await openPage(activeId);
    };
    controls.append(dupBtn);
  }

  if (index > 0) {
    const upBtn = document.createElement('button');
    upBtn.type = 'button';
    upBtn.dataset.move = '-1';
    upBtn.title = 'Move up';
    upBtn.textContent = '↑';
    upBtn.onclick = async () => {
      const target = activePage.blocks[index - 1];
      if (!target) return;
      await flushLocalEdits();
      await window.noteon.moveBlock({ id: b.id, targetId: target.id, after: false });
      await openPage(activeId);
    };
    controls.append(upBtn);
  }

  if (index < (activePage?.blocks?.length || 0) - 1) {
    const downBtn = document.createElement('button');
    downBtn.type = 'button';
    downBtn.dataset.move = '1';
    downBtn.title = 'Move down';
    downBtn.textContent = '↓';
    downBtn.onclick = async () => {
      const target = activePage.blocks[index + 1];
      if (!target) return;
      await flushLocalEdits();
      await window.noteon.moveBlock({ id: b.id, targetId: target.id, after: true });
      await openPage(activeId);
    };
    controls.append(downBtn);
  }

  if (b.column_group === null && b.kind !== 'database') {
    const colBtn = document.createElement('button');
    colBtn.type = 'button';
    colBtn.dataset.columns = String(b.id);
    colBtn.title = 'Turn into 2 columns';
    colBtn.textContent = '▥';
    colBtn.onclick = async () => {
      await flushLocalEdits();
      await window.noteon.makeColumns(b.id);
      await openPage(activeId);
    };
    controls.append(colBtn);
  }

  const delBtn = document.createElement('button');
  delBtn.type = 'button';
  delBtn.dataset.delete = String(b.id);
  delBtn.title = 'Delete block';
  delBtn.textContent = '✕';
  delBtn.onclick = () => deleteBlock(b, index).catch(showDockError);
  controls.append(delBtn);

  return controls;
}

/**
 * Render an image or video block with clickable preview (for images) and width slider (`25%..100%`).
 * @param {HTMLElement} body
 * @param {object} b
 */
function renderMediaBlockBody(body, b) {
  const src = localFileUrl(b.content);
  const pct = Math.max(25, Math.min(100, Math.round((Number(b.media_width) || 1) * 100)));

  if (b.kind === 'image') {
    const img = document.createElement('img');
    img.className = 'media';
    img.src = src;
    img.style.width = `${pct}%`;
    img.onclick = () => openMediaPreview(src, b.content);
    body.append(img);
  } else {
    const video = document.createElement('video');
    video.className = 'media';
    video.src = src;
    video.controls = true;
    video.style.width = `${pct}%`;
    body.append(video);
  }

  const sizeControl = document.createElement('div');
  sizeControl.className = 'media-size-control';
  const label = document.createElement('span');
  label.textContent = `Width ${pct}%`;
  const range = document.createElement('input');
  range.type = 'range';
  range.min = '25';
  range.max = '100';
  range.value = String(pct);
  range.oninput = () => {
    label.textContent = `Width ${range.value}%`;
    const media = body.querySelector('.media');
    if (media) media.style.width = `${range.value}%`;
  };
  range.onchange = async () => {
    const width = Number(range.value) / 100;
    b.media_width = width;
    await window.noteon.resizeMedia({ id: b.id, width });
    await updatePageHistoryState(activeId);
  };

  const openExt = document.createElement('button');
  openExt.type = 'button';
  openExt.textContent = 'Open externally';
  openExt.onclick = () => window.noteon.openMedia(b.content).catch(showDockError);

  sizeControl.append(label, range, openExt);
  body.append(sizeControl);
}

/**
 * Render a single block into `root`.
 * @param {HTMLElement} root
 * @param {object} b
 * @param {number} index
 * @param {number|null} [numberIndex]
 */
function renderBlock(root, b, index, numberIndex = null) {
  const row = document.createElement('div');
  row.className = `block ${b.kind}${selectedBlockIds.has(b.id) ? ' selected' : ''}`;
  row.dataset.blockId = String(b.id);

  row.ondragover = (event) => {
    if (!pageWriteAccess || !event.dataTransfer?.types?.includes('application/x-noteon-block')) return;
    if (draggedBlockIds.includes(b.id)) return;
    event.preventDefault();
    clearBlockDropIndicators();
    const placement = resolveBlockDropPlacement(event, row, b);
    const indicatorClass = placement.includes('before')
      ? 'drop-before'
      : placement.includes('after')
        ? 'drop-after'
        : placement === 'left'
          ? 'drop-left'
          : 'drop-right';
    row.classList.add(indicatorClass);
  };

  row.ondragleave = (event) => {
    if (!row.contains(event.relatedTarget)) {
      row.classList.remove('drop-before', 'drop-after', 'drop-left', 'drop-right');
    }
  };

  row.ondrop = async (event) => {
    if (!pageWriteAccess || !event.dataTransfer?.types?.includes('application/x-noteon-block')) return;
    event.preventDefault();
    event.stopPropagation();
    clearBlockDropIndicators();

    let ids = draggedBlockIds;
    try {
      const raw = event.dataTransfer.getData('application/x-noteon-block');
      if (raw) ids = JSON.parse(raw);
    } catch {}
    if (!Array.isArray(ids) || !ids.length || ids.includes(b.id)) return;

    const placement = resolveBlockDropPlacement(event, row, b);
    await flushLocalEdits();
    await window.noteon.arrangeBlocks({
      pageId: activePage.id,
      ids,
      targetId: b.id,
      placement,
    });
    await openPage(activeId);
  };

  const prefix = buildBlockPrefix(b, numberIndex);
  row.append(prefix);

  const body = document.createElement('div');
  body.className = 'block-body';
  body.style.flex = '1';
  body.style.minWidth = '0';

  let editor = null;

  if (b.kind === 'divider') {
    const hr = document.createElement('hr');
    hr.className = 'block-divider';
    body.append(hr);
  } else if (b.kind === 'image' || b.kind === 'video') {
    renderMediaBlockBody(body, b);
  } else if (b.kind === 'database') {
    const dbPageId = Number(b.content);
    const box = document.createElement('div');
    box.className = 'embedded-db-box';
    const header = document.createElement('div');
    header.className = 'embedded-db-header';
    const linked = pages.find((item) => item.id === dbPageId);
    const strong = document.createElement('strong');
    strong.textContent = `${linked?.icon || '▦'} ${linked?.title || 'Database'}`;
    const openBtn = document.createElement('button');
    openBtn.type = 'button';
    openBtn.textContent = 'Open full page →';
    openBtn.onclick = () => openPage(dbPageId);
    header.append(strong, openBtn);
    const dbMount = document.createElement('div');
    box.append(header, dbMount);
    body.append(box);
    renderDatabase(dbMount, dbPageId).catch(showDockError);
  } else if (b.kind === 'page_link') {
    const linkedId = Number(b.content);
    const linked = pages.find((item) => item.id === linkedId);
    const linkBtn = document.createElement('button');
    linkBtn.type = 'button';
    linkBtn.className = 'page-link-btn';
    linkBtn.textContent = `${linked?.icon || '📄'} ${linked?.title || 'Untitled page'}`;
    linkBtn.onclick = () => openPage(linkedId);
    body.append(linkBtn);
  } else {
    editor = document.createElement('div');
    editor.className = 'block-editor';
    editor.contentEditable = pageWriteAccess ? 'true' : 'false';
    editor.dataset.placeholder = b.kind === 'text' ? "Type '/' for commands…" : kinds[b.kind] || 'Write something…';
    editor.innerHTML = styledHtml(b.content, b.text_styles, b.kind);
    installLinkInteractions(editor, b);

    editor.oninput = async () => {
      const rawText = editor.textContent || '';
      if (await tryMarkdownBlockTransform(b, rawText)) return;
      syncEditorBlockState(editor, b);
      updateSlashCommandMenu(editor);
    };

    editor.onblur = () => {
      setTimeout(() => {
        if (activeSlashMenu && activeSlashMenu._editor === editor) {
          closeSlashCommandMenu();
        }
      }, 120);
      if (pendingBlocks.has(b.id)) {
        clearTimeout(blockSaveTimer);
        flushBlockSaves().catch(() => {});
      }
    };

    editor.onkeydown = (event) => {
      blockKey(event, b, index).catch(showDockError);
    };

    editor.onpaste = async (event) => {
      const text = event.clipboardData?.getData('text/plain') || '';
      const hasMultiLineMarkdown = text.includes('\n') || /^(\s*[-*] |\s*\d+\. |#{1,3} |```)/m.test(text);
      if (!hasMultiLineMarkdown) return;
      event.preventDefault();
      await flushLocalEdits();
      const replaceId = (editor.textContent || '').trim() === '' ? b.id : null;
      const created = await window.noteon.pasteBlocks({
        pageId: activePage.id,
        position: b.position + (replaceId ? 0 : 1),
        replaceBlock: replaceId,
      });
      if (created?.length) {
        await openPage(activeId);
        focusBlockById(created[created.length - 1], true);
      }
    };

    body.append(editor);
  }

  row.append(body, buildBlockControls(b, index, editor));
  root.append(row);
}

/**
 * Render the sticky selection toolbar when one or more blocks are checked on the active page.
 * @param {HTMLElement} root
 */
function renderBlockSelectionToolbar(root) {
  const bar = document.createElement('div');
  bar.className = 'block-selection-tools';

  const count = document.createElement('span');
  count.textContent = `${selectedBlockIds.size} selected`;
  bar.append(count);

  const ids = [...selectedBlockIds];
  const makeBtn = (label, onClick) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = label;
    btn.onclick = () => onClick().catch(showDockError);
    return btn;
  };

  bar.append(
    makeBtn('Copy', () => copySelectedBlocks(false)),
    makeBtn('Cut', () => copySelectedBlocks(true)),
    makeBtn('Paste after', () => pasteBlocksIntoPage()),
    makeBtn('Duplicate', async () => {
      await flushLocalEdits();
      await window.noteon.duplicateBlocks({ ids });
      await openPage(activeId);
    }),
    makeBtn('Delete', async () => {
      await flushLocalEdits();
      await window.noteon.deleteBlocks({ ids });
      selectedBlockIds.clear();
      blockSelectionAnchorId = null;
      await openPage(activeId);
    }),
    makeBtn('Move ↑', async () => {
      await flushLocalEdits();
      await window.noteon.moveBlocks({ ids, direction: -1 });
      await openPage(activeId);
    }),
    makeBtn('Move ↓', async () => {
      await flushLocalEdits();
      await window.noteon.moveBlocks({ ids, direction: 1 });
      await openPage(activeId);
    }),
  );

  if (ids.length >= 2 && ids.length <= 4) {
    bar.append(
      makeBtn('Columns', async () => {
        await flushLocalEdits();
        await window.noteon.groupColumns({ pageId: activePage.id, ids });
        await openPage(activeId);
      }),
    );
  }

  const kindSelect = document.createElement('select');
  const defaultOpt = document.createElement('option');
  defaultOpt.value = '';
  defaultOpt.textContent = 'Turn into…';
  kindSelect.append(defaultOpt);
  for (const [value, label] of blockTypeChoices) {
    const opt = document.createElement('option');
    opt.value = value;
    opt.textContent = label;
    kindSelect.append(opt);
  }
  kindSelect.onchange = async () => {
    if (!kindSelect.value) return;
    await flushLocalEdits();
    await window.noteon.setBlockKind({ ids, kind: kindSelect.value });
    await openPage(activeId);
  };
  bar.append(kindSelect);

  addBlockFormatTools(bar);

  const selectedBlocks = (activePage?.blocks || []).filter((block) => selectedBlockIds.has(block.id));
  if (selectedBlocks.some((block) => block.kind === 'todo')) {
    const allChecked = selectedBlocks.filter((b) => b.kind === 'todo').every((b) => Boolean(b.checked));
    bar.append(
      makeBtn(allChecked ? 'Uncheck to-do' : 'Check to-do', async () => {
        await flushLocalEdits();
        await window.noteon.setBlocksChecked({ ids, checked: !allChecked });
        await openPage(activeId);
      }),
    );
  }

  const clearBtn = document.createElement('button');
  clearBtn.type = 'button';
  clearBtn.textContent = 'Clear';
  clearBtn.onclick = () => {
    selectedBlockIds.clear();
    blockSelectionAnchorId = null;
    openPage(activeId, { suppressHistory: true, skipTab: true }).catch(showDockError);
  };
  bar.append(clearBtn);

  root.append(bar);
}

/**
 * Render an array of blocks into `root`, grouping consecutive column-layout blocks side by side.
 * @param {HTMLElement} root
 * @param {object[]} blocks
 */
function renderPageBlocks(root, blocks) {
  closeSlashCommandMenu();
  if (selectedBlockPageId !== activeId) {
    selectedBlockIds = new Set();
    selectedBlockPageId = activeId;
    blockSelectionAnchorId = null;
  }

  if (selectedBlockIds.size) {
    renderBlockSelectionToolbar(root);
  }

  let flowNumberIndex = 0;

  for (let i = 0; i < blocks.length; ) {
    const block = blocks[i];

    if (block.column_group === null || block.column_group === undefined) {
      flowNumberIndex = block.kind === 'numbered' ? flowNumberIndex + 1 : 0;
      renderBlock(root, block, i, block.kind === 'numbered' ? flowNumberIndex : null);
      i++;
      continue;
    }

    flowNumberIndex = 0;
    const groupId = Number(block.column_group);
    const groupBlocks = [];
    while (i < blocks.length && Number(blocks[i].column_group) === groupId) {
      groupBlocks.push({ block: blocks[i], index: i });
      i++;
    }

    const wrap = document.createElement('div');
    wrap.className = 'block-columns-wrap';

    const tools = document.createElement('div');
    tools.className = 'column-group-tools';
    const ungroupBtn = document.createElement('button');
    ungroupBtn.type = 'button';
    ungroupBtn.textContent = 'Ungroup columns';
    ungroupBtn.onclick = async () => {
      await flushLocalEdits();
      await window.noteon.ungroupColumns({ pageId: activePage.id, group: groupId });
      await openPage(activeId);
    };
    tools.append(ungroupBtn);
    wrap.append(tools);

    const columnsRow = document.createElement('div');
    columnsRow.className = 'block-columns';
    columnsRow.style.display = 'flex';
    columnsRow.style.alignItems = 'flex-start';
    columnsRow.style.gap = '6px';

    const byColumn = new Map();
    for (const item of groupBlocks) {
      const colIdx = Number(item.block.layout_column) || 0;
      if (!byColumn.has(colIdx)) byColumn.set(colIdx, []);
      byColumn.get(colIdx).push(item);
    }

    const sortedColumns = [...byColumn.keys()].sort((a, b) => a - b);
    const widths = sortedColumns.map((colIdx) => Number(byColumn.get(colIdx)[0]?.block?.column_width) || 1 / sortedColumns.length);
    const columnNodes = [];

    sortedColumns.forEach((colIdx, boundaryIndex) => {
      const colEl = document.createElement('div');
      colEl.className = 'block-column';
      colEl.style.minWidth = '0';
      colEl.style.flex = `${widths[boundaryIndex]} 1 0`;

      let colNumberIndex = 0;
      for (const item of byColumn.get(colIdx)) {
        colNumberIndex = item.block.kind === 'numbered' ? colNumberIndex + 1 : 0;
        renderBlock(colEl, item.block, item.index, item.block.kind === 'numbered' ? colNumberIndex : null);
      }

      columnsRow.append(colEl);
      columnNodes.push(colEl);

      if (boundaryIndex < sortedColumns.length - 1) {
        const resizer = document.createElement('div');
        resizer.className = 'column-resize';
        resizer.style.width = '8px';
        resizer.style.flex = 'none';
        resizer.style.alignSelf = 'stretch';
        resizer.style.cursor = 'col-resize';
        resizer.style.touchAction = 'none';

        resizer.onpointerdown = (event) => {
          if (!pageWriteAccess || event.button !== 0) return;
          event.preventDefault();
          resizer.setPointerCapture(event.pointerId);
          const startX = event.clientX;
          const totalPx = Math.max(1, columnsRow.getBoundingClientRect().width);
          const leftInit = widths[boundaryIndex];
          const rightInit = widths[boundaryIndex + 1];
          const pairTotal = leftInit + rightInit;
          const minWidth = pairTotal * 0.08;
          let latestLeft = leftInit;
          let latestRight = rightInit;

          const onMove = (moveEvent) => {
            const deltaRatio = ((moveEvent.clientX - startX) / totalPx) * widths.reduce((sum, w) => sum + w, 0);
            latestLeft = Math.max(minWidth, Math.min(pairTotal - minWidth, leftInit + deltaRatio));
            latestRight = pairTotal - latestLeft;
            columnNodes[boundaryIndex].style.flex = `${latestLeft} 1 0`;
            columnNodes[boundaryIndex + 1].style.flex = `${latestRight} 1 0`;
          };

          const finish = async () => {
            resizer.removeEventListener('pointermove', onMove);
            resizer.removeEventListener('pointerup', finish);
            resizer.removeEventListener('pointercancel', finish);
            widths[boundaryIndex] = latestLeft;
            widths[boundaryIndex + 1] = latestRight;
            await window.noteon.resizeColumns({
              pageId: activePage.id,
              group: groupId,
              boundary: boundaryIndex,
              leftWidth: latestLeft,
              rightWidth: latestRight,
            });
            await updatePageHistoryState(activeId);
          };

          resizer.addEventListener('pointermove', onMove);
          resizer.addEventListener('pointerup', finish);
          resizer.addEventListener('pointercancel', finish);
        };

        columnsRow.append(resizer);
      }
    });

    wrap.append(columnsRow);
    root.append(wrap);
  }
}

/**
 * Install OS file drag-and-drop handlers on the page content area for importing images/videos/file paths.
 */
function initBlockDropHandlers() {
  const isExternalFileDrag = (event) =>
    Boolean(event.dataTransfer?.types?.includes('Files')) &&
    !event.dataTransfer?.types?.includes('application/x-noteon-block');

  document.addEventListener('dragover', (event) => {
    const content = $('#content');
    if (!content || !isExternalFileDrag(event) || !pageWriteAccess || !activeId || activePage?.isDatabase) return;
    if (!content.contains(event.target)) return;
    event.preventDefault();
    content.classList.add('file-drop-active');
  });

  document.addEventListener('dragleave', (event) => {
    const content = $('#content');
    if (!content) return;
    if (!event.relatedTarget || !content.contains(event.relatedTarget)) {
      content.classList.remove('file-drop-active');
    }
  });

  document.addEventListener('drop', async (event) => {
    const content = $('#content');
    content?.classList.remove('file-drop-active');
    if (!content || !isExternalFileDrag(event) || !pageWriteAccess || !activeId || activePage?.isDatabase) return;
    if (!content.contains(event.target)) return;

    const files = [...(event.dataTransfer?.files || [])];
    if (!files.length) return;
    event.preventDefault();

    await flushLocalEdits();
    const targetBlockEl = event.target?.closest?.('[data-block-id]');
    const targetBlock = targetBlockEl
      ? activePage?.blocks?.find((item) => item.id === Number(targetBlockEl.dataset.blockId))
      : null;
    let position = targetBlock ? targetBlock.position + 1 : activePage?.blocks?.length || 0;

    for (const file of files) {
      const filePath = window.noteon.getDroppedFilePath(file);
      if (!filePath) continue;
      await window.noteon.importDroppedFile({
        pageId: activeId,
        position,
        filePath,
      });
      position++;
    }
    await openPage(activeId);
  });
}
