/**
 * Shared renderer runtime state, DOM helpers, save queue management, option pickers, breadcrumbs, and page find.
 */

const $ = (selector) => document.querySelector(selector);
const pageQuery = new URLSearchParams(location.search);
const rendererPaneId = pageQuery.get('pane') || 'primary';
const embeddedPane = pageQuery.get('embedded') === 'true';
const windowLayoutPrefix = embeddedPane
  ? `electron_pane_${rendererPaneId}`
  : pageQuery.get('window') && pageQuery.get('window') !== 'main'
    ? `electron_window_${pageQuery.get('window')}`
    : 'electron';

let pages = [];
let activeId = null;
let activePage = null;
let pageLeaseKey = null;
let pageWriteAccess = true;
let titleSaveTimer = null;
let blockSaveTimer = null;
let settings = {};
let pendingPageSave = false;
let pendingBlocks = new Map();
let pageSaveInFlight = null;
let blockSaveInFlight = null;

let selectedBlockIds = new Set();
let selectedBlockPageId = null;
let blockSelectionAnchorId = null;

let selectedRecordIds = new Set();
let selectedDatabaseId = null;
let recordSelectionAnchorId = null;

let openTabs = [];
let activeTabIndex = 0;
let expandedPageIds = new Set();
let expandedTreeRevision = 0;
let treeRowsCache = null;
let searchResults = null;
let searchTimer = null;
let treeScrollFrame = 0;
let treeMode = 'workspace';
let selectedTreePageIds = new Set();
let treeSelectionAnchorId = null;

let navigationQueue = Promise.resolve();
let pageHistory = [];
let historyIndex = -1;
let optionPickerDismissInstalled = false;
let dockLayout = NoteonDock.create(rendererPaneId);
const nativePaneHosts = new Map();
let closing = false;

const kinds = {
  text: '',
  h1: 'Heading 1',
  h2: 'Heading 2',
  h3: 'Heading 3',
  bullet: '•',
  numbered: '1.',
  todo: '□',
  quote: '❝',
  callout: '✦',
  code: '</>',
  divider: '',
  image: '▧',
  video: '▶',
  database: '▦',
  page_link: '↗',
};

const blockTypeChoices = [
  ['text', 'Text'],
  ['h1', 'Heading 1'],
  ['h2', 'Heading 2'],
  ['h3', 'Heading 3'],
  ['bullet', 'Bullet list'],
  ['numbered', 'Numbered list'],
  ['todo', 'To-do'],
  ['quote', 'Quote'],
  ['callout', 'Callout'],
  ['code', 'Code'],
  ['divider', 'Divider'],
];

const databaseKinds = [
  ['text', 'Text'],
  ['number', 'Number'],
  ['select', 'Select'],
  ['multi-select', 'Multi-select'],
  ['status', 'Status'],
  ['date', 'Date'],
  ['person', 'Person'],
  ['files', 'Files & media'],
  ['checkbox', 'Checkbox'],
  ['url', 'URL'],
  ['phone', 'Phone'],
  ['email', 'Email'],
  ['relation', 'Relation'],
  ['rollup', 'Rollup'],
  ['formula', 'Formula'],
  ['button', 'Button'],
  ['id', 'ID'],
  ['place', 'Place'],
  ['created_time', 'Created time'],
  ['last_edited_time', 'Last edited time'],
  ['created_by', 'Created by'],
  ['last_edited_by', 'Last edited by'],
];

const utf8Encoder = new TextEncoder();

function esc(value = '') {
  return String(value).replace(/[&<>"']/g, (ch) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[ch]));
}

function setSaveState(message, retry = false) {
  $('#save-state').textContent = message;
  $('#retry-save').hidden = !retry;
}

function showDockError(error) {
  $('#save-state').textContent = `Panel error: ${error?.message || error}`;
}

function currentDockPageId() {
  const pane = NoteonDock.find(dockLayout.root, dockLayout.focused);
  return pane?.tabs[pane.active] || (pane?.id === 'primary' ? activeId : null);
}

function localFileUrl(value) {
  const raw = String(value || '').replaceAll('\\', '/');
  if (raw.startsWith('//')) {
    const [host, ...parts] = raw.slice(2).split('/');
    return `file://${host}/${parts.map(encodeURIComponent).join('/')}`;
  }
  if (/^[a-z]:\//i.test(raw)) {
    const [drive, ...parts] = raw.split('/');
    return `file:///${drive}/${parts.map(encodeURIComponent).join('/')}`;
  }
  return `file://${raw.split('/').map(encodeURIComponent).join('/')}`;
}

function renderEmptyWorkspace(heading = 'Choose a page', detail = 'Pick a page from the sidebar or create a new one.') {
  const detailHtml = detail ? `<p>${esc(detail)}</p>` : '';
  $('#content').innerHTML = `<div class="empty"><h1>${esc(heading)}</h1>${detailHtml}</div>`;
}

async function savePage() {
  if (pageSaveInFlight) {
    await pageSaveInFlight;
    if (!pendingPageSave) return;
    return savePage();
  }
  if (!activePage) return;

  const page = activePage;
  const snapshot = {
    id: activeId,
    title: page.title,
    icon: page.icon,
    full_width: page.full_width,
    cover: page.cover,
    cover_image: page.cover_image,
    cover_x: page.cover_x,
    cover_y: page.cover_y,
    priority: page.priority,
  };

  pageSaveInFlight = window.noteon.savePage(snapshot);
  try {
    const result = await pageSaveInFlight;
    const unchangedSinceStart =
      activePage === page &&
      page.title === snapshot.title &&
      page.icon === snapshot.icon &&
      page.full_width === snapshot.full_width &&
      page.cover === snapshot.cover &&
      page.cover_image === snapshot.cover_image &&
      page.cover_x === snapshot.cover_x &&
      page.cover_y === snapshot.cover_y &&
      page.priority === snapshot.priority;

    if (unchangedSinceStart) {
      pendingPageSave = false;
    }
    await updatePageHistoryState(snapshot.id);
    return result;
  } finally {
    pageSaveInFlight = null;
  }
}

async function flushBlockSaves() {
  clearTimeout(blockSaveTimer);
  if (blockSaveInFlight) {
    await blockSaveInFlight;
    return flushBlockSaves();
  }
  if (!pendingBlocks.size) return;

  setSaveState('Saving…');
  const updates = [...pendingBlocks.values()].map((block) => ({
    id: block.id,
    content: block.content,
    checked: block.checked,
    text_styles: structuredClone(block.text_styles || []),
  }));
  pendingBlocks.clear();

  blockSaveInFlight = (async () => {
    for (let index = 0; index < updates.length; index++) {
      const block = updates[index];
      try {
        await window.noteon.saveBlock(block);
      } catch (error) {
        for (const item of updates.slice(index)) {
          if (!pendingBlocks.has(item.id)) pendingBlocks.set(item.id, item);
        }
        setSaveState('Save failed — retry with Ctrl+S', true);
        throw error;
      }
    }
  })();

  try {
    await blockSaveInFlight;
  } finally {
    blockSaveInFlight = null;
  }

  if (pendingBlocks.size) {
    return flushBlockSaves();
  }
  setSaveState('All changes saved');
  if (activeId) {
    await updatePageHistoryState(activeId);
  }
}

function debounce(fn) {
  setSaveState('Saving…');
  clearTimeout(titleSaveTimer);
  titleSaveTimer = setTimeout(async () => {
    try {
      await fn();
      if (!pendingPageSave && !pendingBlocks.size && !blockSaveInFlight) {
        setSaveState('All changes saved');
      }
    } catch {
      setSaveState('Save failed — retry with Ctrl+S', true);
    }
  }, 400);
}

async function flushLocalEdits() {
  clearTimeout(titleSaveTimer);
  while (pendingPageSave || pageSaveInFlight) {
    await savePage();
  }
  await flushBlockSaves();
}

async function retryPendingSave() {
  const button = $('#retry-save');
  button.disabled = true;
  setSaveState('Saving…');
  try {
    await flushLocalEdits();
    setSaveState('All changes saved');
  } catch (error) {
    setSaveState(`Save failed — ${error.message || 'retry or use Ctrl+S'}`, true);
  } finally {
    button.disabled = false;
  }
}

async function flushPendingWorkspaceEdits() {
  if (!embeddedPane) {
    await window.noteon.flushDockViews();
  }
  await flushLocalEdits();
}

async function beforeTreeMutation() {
  await flushPendingWorkspaceEdits();
}

function pageAncestors(pageId) {
  const chain = [];
  const seen = new Set();
  let current = pages.find((item) => item.id === Number(pageId));
  while (current && !seen.has(current.id) && chain.length <= pages.length) {
    seen.add(current.id);
    chain.unshift(current);
    current = current.parent_id === null ? null : pages.find((item) => item.id === current.parent_id);
  }
  return chain;
}

function renderBreadcrumbs(pageId) {
  const bar = $('#breadcrumbs');
  if (!bar) return;
  bar.innerHTML = '';
  if (!pageId) return;

  const chain = pageAncestors(pageId);
  if (!chain.length && activePage && Number(activePage.id) === Number(pageId)) {
    chain.push({ id: activePage.id, icon: activePage.icon, title: activePage.title });
  }

  chain.forEach((item, idx) => {
    const isCurrent = idx === chain.length - 1;
    const isLoaded = isCurrent && activePage && Number(activePage.id) === Number(item.id);
    const icon = isLoaded ? activePage.icon : item.icon;
    const title = isLoaded ? activePage.title : item.title;

    if (idx > 0) {
      const sep = document.createElement('span');
      sep.className = 'breadcrumb-sep';
      sep.textContent = '/';
      bar.append(sep);
    }

    if (isCurrent) {
      const cur = document.createElement('span');
      cur.className = 'breadcrumb-item current';
      cur.textContent = `${icon || '📄'} ${title || 'Untitled'}`;
      bar.append(cur);
    } else {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'breadcrumb-item';
      btn.textContent = `${icon || '📄'} ${title || 'Untitled'}`;
      btn.title = title || 'Untitled';
      btn.onclick = () => openPage(item.id);
      bar.append(btn);
    }
  });
}

function showPageFind() {
  if (!activePage) return;
  const fold = (value) => String(value || '').toLocaleLowerCase('tr');
  const clearHighlights = () => {
    document.querySelectorAll('.find-match').forEach((element) => element.classList.remove('find-match'));
    if (globalThis.CSS?.highlights) {
      CSS.highlights.delete('noteon-find');
      CSS.highlights.delete('noteon-find-active');
    }
  };

  let bar = document.querySelector('.page-find-bar');
  if (!bar) {
    bar = document.createElement('div');
    bar.className = 'page-find-bar';
    bar.innerHTML =
      '<input type="search" placeholder="Find in this page" aria-label="Find in this page"><span class="page-find-count"></span><button data-prev title="Previous match">↑</button><button data-next title="Next match">↓</button><button data-close title="Close">×</button>';
    document.querySelector('.main').append(bar);

    const input = bar.querySelector('input');
    const count = bar.querySelector('.page-find-count');
    let matches = [];
    let current = -1;
    let searchGeneration = 0;

    const clearTableFilter = () => {
      const table = document.querySelector('#content .db-table');
      if (!table) return;
      delete table.dataset.findRecordIds;
      table.querySelectorAll('tbody tr').forEach((row) => {
        row.hidden = false;
      });
      const databaseId = Number(table.dataset.databaseId);
      if (databaseId) {
        window.noteon
          .getDatabase(databaseId)
          .then((database) => database && updateTableCalculationFooter(table, database, databaseVisibleProperties(database, 'table')))
          .catch(showDockError);
      }
    };

    const selectMatch = (index) => {
      if (!matches.length) return;
      current = (index + matches.length) % matches.length;
      document.querySelectorAll('.find-match').forEach((element) => element.classList.remove('find-match'));
      const match = matches[current];
      if (match.element) {
        match.element.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'smooth' });
        match.element.classList.add('find-match');
      } else if (match.range) {
        match.range.startContainer.parentElement?.scrollIntoView({ block: 'center', behavior: 'smooth' });
        if (globalThis.Highlight && CSS?.highlights) {
          CSS.highlights.set('noteon-find-active', new Highlight(match.range));
        }
      }
      count.textContent = `${current + 1} / ${matches.length}`;
    };

    const search = async () => {
      const generation = ++searchGeneration;
      clearHighlights();
      matches = [];
      current = -1;

      const query = fold(input.value.trim());
      if (!query) {
        clearTableFilter();
        count.textContent = '';
        bar.querySelectorAll('[data-prev],[data-next]').forEach((button) => {
          button.hidden = false;
        });
        return;
      }

      const table = document.querySelector('#content .db-table');
      if (table && activePage?.isDatabase) {
        bar.querySelectorAll('[data-prev],[data-next]').forEach((button) => {
          button.hidden = true;
        });
        try {
          const database = await window.noteon.getDatabase(Number(table.dataset.databaseId));
          if (generation !== searchGeneration) return;
          if (database?.view_mode === 'table') {
            const matched = database.records.filter(
              (record) =>
                fold(record.title).includes(query) ||
                fold(record.preview).includes(query) ||
                Object.values(record.values || {}).some((value) => fold(value).includes(query)),
            );
            const ids = new Set(matched.map((record) => Number(record.id)));
            table.dataset.findRecordIds = JSON.stringify([...ids]);
            const rowById = new Map([...table.querySelectorAll('tbody tr[data-record]')].map((row) => [Number(row.dataset.record), row]));
            for (const [id, row] of rowById) {
              row.hidden = !ids.has(id);
              if (ids.has(id)) matches.push({ element: row });
            }
            count.textContent = `${matched.length} / ${database.records.length}`;
            if (matches.length) {
              matches[0].element.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'smooth' });
              matches[0].element.classList.add('find-match');
            }
            updateTableCalculationFooter(table, database, databaseVisibleProperties(database, 'table'));
            return;
          }
        } catch (error) {
          showDockError(error);
        }
      } else {
        clearTableFilter();
      }

      bar.querySelectorAll('[data-prev],[data-next]').forEach((button) => {
        button.hidden = false;
      });

      const walker = document.createTreeWalker(document.querySelector('#content'), NodeFilter.SHOW_TEXT);
      const ranges = [];
      let node;
      while ((node = walker.nextNode())) {
        if (node.parentElement.closest('script,style,button,.page-find-bar,.muted,.block-controls')) continue;
        const text = node.nodeValue || '';
        const folded = fold(text);
        let offset = 0;
        let found;
        while ((found = folded.indexOf(query, offset)) >= 0) {
          const range = document.createRange();
          range.setStart(node, found);
          range.setEnd(node, found + query.length);
          matches.push({ range });
          ranges.push(range);
          offset = found + Math.max(1, query.length);
        }
      }

      const inputs = document.querySelectorAll('#content .page-title-input, #content .property-row input, #content input.db-value, #content textarea.db-value');
      for (const element of inputs) {
        const folded = fold(element.value || '');
        let offset = 0;
        let found;
        while ((found = folded.indexOf(query, offset)) >= 0) {
          matches.push({ element, start: found, end: found + query.length });
          offset = found + Math.max(1, query.length);
        }
      }

      if (globalThis.Highlight && CSS?.highlights && ranges.length) {
        CSS.highlights.set('noteon-find', new Highlight(...ranges));
      }
      count.textContent = matches.length ? `${matches.length} matches` : 'No matches';
      if (matches.length) selectMatch(0);
    };

    input.oninput = () => search().catch(showDockError);
    input.onkeydown = (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        if (document.querySelector('#content .db-table') && activePage?.isDatabase && matches.length) {
          matches[0].element?.querySelector('.record-title')?.focus();
          return;
        }
        selectMatch(current + (event.shiftKey ? -1 : 1));
      }
      if (event.key === 'Escape') {
        clearHighlights();
        clearTableFilter();
        bar.remove();
      }
    };

    bar.querySelector('[data-prev]').onclick = () => selectMatch(current - 1);
    bar.querySelector('[data-next]').onclick = () => selectMatch(current + 1);
    bar.querySelector('[data-close]').onclick = () => {
      clearHighlights();
      clearTableFilter();
      bar.remove();
    };
    bar._search = search;
  }

  bar.hidden = false;
  bar.querySelector('input').focus();
  bar.querySelector('input').select();
  bar._search().catch(showDockError);
}

function propertyOptions(property) {
  const raw = property.options || '';
  if (Array.isArray(raw)) {
    return raw.map((option) => (typeof option === 'string' ? { name: option, color: null } : option));
  }
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.map((option) => (typeof option === 'string' ? { name: option, color: null } : option)).filter((option) => option.name);
    }
  } catch {
    // Fall back to line-delimited options format.
  }
  return String(raw)
    .split(/\r?\n/)
    .map((line) => {
      const [name, color] = line.split('::');
      return { name, color: color || null };
    })
    .filter((option) => option.name);
}

function normalizePropertyOptions(options, existing = [], preserveExisting = true) {
  const colors = new Map(existing.map((option) => [option.name, option.color || null]));
  return JSON.stringify(
    options
      .map((option) => ({
        name: String(option.name).trim(),
        color: option.color || (preserveExisting ? colors.get(String(option.name).trim()) : null) || null,
      }))
      .filter((option) => option.name),
  );
}

function propertyOptionColor(option, kind = 'select') {
  const stored = String(option?.color || '');
  if (/^#[0-9a-f]{6}$/i.test(stored)) return stored;

  const name = String(option?.name || '').trim();
  const lower = name.toLowerCase();
  const known = {
    done: '#4c9a66',
    complete: '#4c9a66',
    completed: '#4c9a66',
    'tamamlandı': '#4c9a66',
    'onaylandı': '#4c9a66',
    evet: '#4c9a66',
    bitti: '#4c9a66',
    aktif: '#4c9a66',
    low: '#4c9a66',
    'in progress': '#4484ce',
    doing: '#4484ce',
    'devam ediyor': '#4484ce',
    devam: '#4484ce',
    working: '#4484ce',
    active: '#4484ce',
    review: '#cc9438',
    beklemede: '#cc9438',
    pending: '#cc9438',
    medium: '#cc9438',
    orta: '#cc9438',
    planned: '#cc9438',
    high: '#ce5852',
    urgent: '#ce5852',
    blocked: '#ce5852',
    iptal: '#ce5852',
    acil: '#ce5852',
    'yüksek': '#ce5852',
    bug: '#ce5852',
    critical: '#ce5852',
  };
  if (known[lower]) return known[lower];
  if (kind === 'status' || ['to do', 'todo', 'bekliyor', 'not started', 'none', 'backlog'].includes(lower)) {
    return '#85877f';
  }

  const wheel = ['#5c8ac6', '#926abe', '#c47654', '#4e9c8e', '#b86690', '#7a9a4e'];
  let hash = 0;
  for (const byte of utf8Encoder.encode(name)) {
    hash = (Math.imul(hash, 31) + byte) | 0;
  }
  return wheel[Math.abs(hash) % wheel.length];
}

function propertyOptionTextColor(color, multi = false) {
  if (!multi) return 'var(--text)';
  if (document.body.classList.contains('dark')) return '#fff';
  const hex = String(color).replace('#', '');
  const rgb = [0, 2, 4].map((index) => parseInt(hex.slice(index, index + 2), 16) || 0);
  const brightness = (0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]) / 255;
  return brightness > 0.6 ? '#141414' : '#fff';
}

function formatNumberDisplay(input) {
  const trimmed = String(input ?? '').trim();
  if (!trimmed) return String(input ?? '');
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed) || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(trimmed)) {
    return String(input);
  }

  let expanded = trimmed;
  if (/[eE]/.test(trimmed)) {
    const scientific = String(parsed).match(/^(-?)(\d+)(?:\.(\d+))?[eE]([+-]?\d+)$/);
    if (scientific) {
      const sign = scientific[1];
      const digits = scientific[2] + (scientific[3] || '');
      const point = scientific[2].length + Number(scientific[4]);
      if (point <= 0) {
        expanded = `${sign}0.${'0'.repeat(-point)}${digits}`;
      } else if (point >= digits.length) {
        expanded = `${sign}${digits}${'0'.repeat(point - digits.length)}`;
      } else {
        expanded = `${sign}${digits.slice(0, point)}.${digits.slice(point)}`;
      }
    }
  }

  const [sign, ...rest] = expanded.startsWith('-')
    ? ['-', expanded.slice(1)]
    : expanded.startsWith('+')
      ? ['+', expanded.slice(1)]
      : ['', expanded];
  const parts = rest.join('').split('.');
  const integer = parts[0];
  const fraction = parts[1];

  let grouped = '';
  for (let index = 0; index < integer.length; index++) {
    if (index > 0 && (integer.length - index) % 3 === 0) grouped += '.';
    grouped += integer[index];
  }
  if (!grouped) grouped = '0';
  return sign + grouped + (fraction ? `,${fraction}` : '');
}

function evaluateNumberExpression(input) {
  const text = String(input || '')
    .replaceAll(',', '.')
    .replaceAll('×', '*')
    .replaceAll('÷', '/')
    .replaceAll('−', '-');
  let offset = 0;
  const skip = () => {
    while (/\s/.test(text[offset] || '')) offset++;
  };

  function operand(depth) {
    if (depth >= 64) return null;
    skip();
    let negative = false;
    while (text[offset] === '+' || text[offset] === '-') {
      if (text[offset] === '-') negative = !negative;
      offset++;
      skip();
    }
    let value;
    if (text[offset] === '(') {
      offset++;
      value = expression(0, depth + 1);
      skip();
      if (text[offset] !== ')') return null;
      offset++;
    } else {
      const match = text.slice(offset).match(/^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/);
      if (!match) return null;
      value = Number(match[0]);
      offset += match[0].length;
    }
    value = negative ? -value : value;
    return Number.isFinite(value) ? value : null;
  }

  function expression(minimum, depth) {
    if (depth >= 64) return null;
    let value = operand(depth);
    if (value === null) return null;
    while (true) {
      skip();
      const operator = text[offset];
      const precedence = operator === '+' || operator === '-' ? 1 : operator === '*' || operator === '/' ? 2 : 0;
      if (!precedence || precedence < minimum) break;
      offset++;
      const right = expression(precedence + 1, depth + 1);
      if (right === null || (operator === '/' && right === 0)) return null;
      if (operator === '+') value += right;
      else if (operator === '-') value -= right;
      else if (operator === '*') value *= right;
      else value /= right;
      if (!Number.isFinite(value)) return null;
    }
    return value;
  }

  const value = expression(0, 0);
  skip();
  return value !== null && offset === text.length ? String(value === 0 ? 0 : value) : null;
}

function formatGeneratedDate(timestamp) {
  const seconds = Number(timestamp || 0);
  if (!Number.isFinite(seconds) || seconds <= 0) return '—';
  const date = new Date(seconds * 1000);
  if (!Number.isFinite(date.getTime())) return '—';

  const months = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
  const today = new Date();
  const day = date.getDate();
  const month = months[date.getMonth()];

  if (date.getFullYear() === today.getFullYear() && date.getMonth() === today.getMonth() && day === today.getDate()) {
    return `${day} ${month}, ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
  }
  if (date.getFullYear() === today.getFullYear()) {
    return `${day} ${month}`;
  }
  return `${day} ${month} ${date.getFullYear()}`;
}

function propertyValuePlaceholder(kind, pageId) {
  return (
    {
      number: 'Empty (number)',
      url: 'https://…',
      email: 'name@example.com',
      phone: '+90…',
      person: 'Empty (person)',
      files: 'File path or URL…',
      place: 'Empty (location)',
      relation: 'Empty (relation)',
      rollup: 'Empty (rollup)',
      formula: 'Empty (formula)',
      button: 'Button label…',
      id: `#${pageId}`,
    }[kind] || 'Empty'
  );
}

function openQuickActionTarget(kind, value) {
  const raw = String(value || '').trim();
  if (!raw) return;
  let target = raw;
  if (kind === 'url') target = /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
  else if (kind === 'email') target = /^mailto:/i.test(raw) ? raw : `mailto:${raw}`;
  else if (kind === 'phone') target = /^tel:/i.test(raw) ? raw : `tel:${raw.replace(/\s+/g, '')}`;
  window.noteon.openUrl(target).catch(showDockError);
}

function attachQuickActionWrapper(input, kind, value, wrapClass, buttonClass) {
  if (!['url', 'email', 'phone'].includes(kind)) return input;
  const wrap = document.createElement('div');
  wrap.className = wrapClass;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = buttonClass;
  button.textContent = kind === 'email' ? '✉' : kind === 'phone' ? '📞' : '↗';
  button.title = kind === 'email' ? 'Send email' : kind === 'phone' ? 'Call phone number' : 'Open URL';
  button.setAttribute('aria-label', button.title);
  button.disabled = !String(value || '').trim();
  input.addEventListener('input', () => {
    button.disabled = !input.value.trim();
  });
  button.onclick = () => openQuickActionTarget(kind, input.value);
  wrap.append(input, button);
  return wrap;
}

function optionPicker(property, value, { onValue, onOptions, onRemove }) {
  if (!optionPickerDismissInstalled) {
    document.addEventListener('pointerdown', (event) => {
      if (event.target.closest?.('.option-picker')) return;
      document.querySelectorAll('.option-picker.open').forEach((picker) => {
        picker.classList.remove('open');
        picker.querySelector('button[aria-expanded]')?.setAttribute('aria-expanded', 'false');
      });
    });
    optionPickerDismissInstalled = true;
  }

  const kind = property.kind;
  const multi = kind === 'multi-select';
  const options = propertyOptions(property);
  let selected = multi ? String(value || '').split('|').filter(Boolean) : value ? [String(value)] : [];

  const root = document.createElement('div');
  root.className = `option-picker ${multi ? 'multi' : 'single'}`;

  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'option-picker-trigger';
  trigger.setAttribute('aria-expanded', 'false');
  trigger.setAttribute('aria-label', property.name);

  const menu = document.createElement('div');
  menu.className = 'option-picker-menu';
  menu.setAttribute('role', 'listbox');
  menu.setAttribute('aria-multiselectable', String(multi));

  const refreshTrigger = () => {
    trigger.innerHTML = '';
    if (!selected.length) {
      const empty = document.createElement('span');
      empty.className = 'option-empty';
      empty.textContent = '—';
      trigger.append(empty);
      return;
    }
    for (const name of selected) {
      const option = options.find((item) => item.name === name) || { name };
      const color = propertyOptionColor(option, kind);
      const chip = document.createElement('span');
      chip.className = `option-picker-chip${multi ? ' multi' : ''}`;
      chip.style.setProperty('--option-color', color);
      chip.style.setProperty('--option-text', propertyOptionTextColor(color, multi));
      chip.textContent = name;
      trigger.append(chip);
    }
  };

  const choose = async (name) => {
    if (multi) {
      selected = selected.includes(name) ? selected.filter((item) => item !== name) : [...selected, name];
      refreshTrigger();
      try {
        await onValue(selected.join('|'));
      } catch (error) {
        showDockError(error);
      }
      return;
    }

    selected = name ? [name] : [];
    refreshTrigger();
    try {
      await onValue(name);
    } catch (error) {
      showDockError(error);
    }
    root.classList.remove('open');
    trigger.setAttribute('aria-expanded', 'false');
  };

  const renderOptions = () => {
    menu.innerHTML = '';
    if (!multi) {
      const none = document.createElement('button');
      none.type = 'button';
      none.className = 'option-picker-none';
      none.textContent = 'None';
      none.onclick = () => choose('');
      menu.append(none);
    }

    for (const option of options) {
      const row = document.createElement('div');
      row.className = 'option-picker-row';
      const color = propertyOptionColor(option, kind);

      const dot = document.createElement('span');
      dot.className = 'option-picker-dot';
      dot.style.background = color;

      const chooseButton = document.createElement('button');
      chooseButton.type = 'button';
      chooseButton.className = 'option-picker-choice';
      if (multi) {
        const check = document.createElement('span');
        check.className = 'option-picker-check';
        check.textContent = selected.includes(option.name) ? '✓' : '';
        chooseButton.append(check);
      }
      chooseButton.append(dot, document.createTextNode(option.name));
      chooseButton.setAttribute('aria-selected', String(selected.includes(option.name)));
      chooseButton.onclick = () => choose(option.name);

      const colorInput = document.createElement('input');
      colorInput.type = 'color';
      colorInput.className = 'option-picker-color';
      colorInput.value = color;
      colorInput.title = `Change ${option.name} color`;
      colorInput.setAttribute('aria-label', `Change ${option.name} color`);
      colorInput.onchange = async () => {
        const next = options.map((item) => (item.name === option.name ? { ...item, color: colorInput.value } : item));
        try {
          await onOptions(next, { preserveExisting: false });
          option.color = colorInput.value;
          refreshTrigger();
          renderOptions();
        } catch (error) {
          showDockError(error);
        }
      };

      const resetColor = document.createElement('button');
      resetColor.type = 'button';
      resetColor.className = 'option-picker-reset';
      resetColor.textContent = '↺';
      resetColor.title = 'Reset to automatic color';
      resetColor.setAttribute('aria-label', 'Reset option color');
      resetColor.onclick = async () => {
        const next = options.map((item) => (item.name === option.name ? { ...item, color: null } : item));
        try {
          await onOptions(next, { preserveExisting: false });
          option.color = null;
          refreshTrigger();
          renderOptions();
        } catch (error) {
          showDockError(error);
        }
      };

      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'option-picker-remove';
      remove.textContent = '×';
      remove.title = `Remove ${option.name}`;
      remove.onclick = async () => {
        try {
          await onRemove(option.name);
          const at = selected.indexOf(option.name);
          if (at >= 0) {
            selected.splice(at, 1);
            if (multi) await onValue(selected.join('|'));
            else selected = [];
          }
          const next = options.filter((item) => item.name !== option.name);
          options.splice(0, options.length, ...next);
          refreshTrigger();
          renderOptions();
        } catch (error) {
          showDockError(error);
        }
      };

      row.append(chooseButton, colorInput, resetColor, remove);
      menu.append(row);
    }

    const addForm = document.createElement('form');
    addForm.className = 'option-picker-add';
    const input = document.createElement('input');
    input.type = 'text';
    input.maxLength = 80;
    input.placeholder = 'New option…';
    input.setAttribute('aria-label', 'New option');
    const addBtn = document.createElement('button');
    addBtn.type = 'submit';
    addBtn.textContent = 'Add';
    addForm.append(input, addBtn);

    addForm.onsubmit = async (event) => {
      event.preventDefault();
      const name = input.value.trim();
      if (!name) return;
      if (options.some((option) => option.name.toLocaleLowerCase() === name.toLocaleLowerCase())) {
        input.select();
        return;
      }
      const next = [...options, { name, color: null }];
      try {
        await onOptions(next);
        options.splice(0, options.length, ...next);
        await choose(name);
        renderOptions();
        input.value = '';
      } catch (error) {
        showDockError(error);
      }
    };

    menu.append(addForm);
  };

  trigger.onclick = () => {
    const open = root.classList.toggle('open');
    trigger.setAttribute('aria-expanded', String(open));
    if (open) {
      const rect = trigger.getBoundingClientRect();
      const height = Math.min(340, options.length * 31 + (multi ? 66 : 94));
      const width = Math.min(290, window.innerWidth - 16);
      const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
      const top =
        rect.bottom + height + 8 > window.innerHeight && rect.top > height + 8
          ? rect.top - height - 4
          : Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - height - 8));
      menu.style.left = `${left}px`;
      menu.style.top = `${top}px`;
    }
  };

  root.onkeydown = (event) => {
    if (event.key === 'Escape') {
      root.classList.remove('open');
      trigger.setAttribute('aria-expanded', 'false');
      trigger.focus();
    }
  };

  root.append(trigger, menu);
  refreshTrigger();
  renderOptions();
  return root;
}
