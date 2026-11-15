/**
 * AI Assistant drawer UI, markdown response rendering, activity indicators,
 * Codex MCP execution controls, snapshot undo, and settings persistence.
 */

const ASSISTANT_STARTER_SUGGESTIONS = [
  { text: 'Summarize this page', fillOnly: false },
  { text: 'Create a table on this page for my tasks', fillOnly: false },
  { text: 'Turn the notes on this page into a to-do list', fillOnly: false },
  { text: 'Find notes about…', fillOnly: true },
];

const ASSISTANT_TOOL_LABELS = {
  list_pages: 'Listing workspace pages…',
  search_notes: 'Searching notes…',
  read_page: 'Reading page content…',
  get_database: 'Loading database…',
  add_rows: 'Adding database rows…',
  update_rows: 'Updating database rows…',
  insert_content: 'Inserting content…',
  update_blocks: 'Updating blocks…',
  create_database: 'Creating database…',
  create_page: 'Creating page…',
  update_page: 'Updating page…',
};

let assistantMessages = [];
let assistantRunning = false;
let assistantRefreshPromise = null;

/**
 * Format an MCP tool name or raw activity string into a user-friendly status line.
 * @param {string} toolName
 * @returns {string}
 */
function formatActivityLabel(toolName) {
  const key = String(toolName || '').trim();
  if (!key) return 'Working…';
  return ASSISTANT_TOOL_LABELS[key] || `Running ${key.replaceAll('_', ' ')}…`;
}

/**
 * Append inline markdown tokens (**bold**, *italic*, `code`, [label](url)) into `parent`.
 * @param {HTMLElement} parent
 * @param {string} text
 */
function appendInlineMarkdown(parent, text) {
  const tokenRegex = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
  let lastIndex = 0;
  let match;

  while ((match = tokenRegex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parent.append(document.createTextNode(text.slice(lastIndex, match.index)));
    }

    const token = match[0];
    if (token.startsWith('**') && token.endsWith('**')) {
      const strong = document.createElement('strong');
      strong.textContent = token.slice(2, -2);
      parent.append(strong);
    } else if (token.startsWith('*') && token.endsWith('*')) {
      const em = document.createElement('em');
      em.textContent = token.slice(1, -1);
      parent.append(em);
    } else if (token.startsWith('`') && token.endsWith('`')) {
      const code = document.createElement('code');
      code.textContent = token.slice(1, -1);
      parent.append(code);
    } else if (token.startsWith('[')) {
      const linkMatch = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      if (linkMatch) {
        const [, label, target] = linkMatch;
        const pageMatch = target.match(/^noteon:\/\/page\/(\d+)$/);
        if (pageMatch) {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.textContent = label;
          btn.style.padding = '0 3px';
          btn.style.color = 'var(--accent)';
          btn.style.textDecoration = 'underline';
          btn.onclick = () => openPage(Number(pageMatch[1])).catch(showDockError);
          parent.append(btn);
        } else {
          const span = document.createElement('span');
          span.textContent = `${label} (${target})`;
          parent.append(span);
        }
      } else {
        parent.append(document.createTextNode(token));
      }
    }

    lastIndex = match.index + token.length;
  }

  if (lastIndex < text.length) {
    parent.append(document.createTextNode(text.slice(lastIndex)));
  }
}

/**
 * Render basic markdown blocks (fenced code, headings, bullet/numbered lists, paragraphs) into a DocumentFragment.
 * @param {string} markdown
 * @returns {DocumentFragment}
 */
function renderMarkdownFragment(markdown) {
  const fragment = document.createDocumentFragment();
  const lines = String(markdown || '').replace(/\r\n?/g, '\n').split('\n');

  let listEl = null;
  let listType = null;
  let paragraphLines = [];

  const flushParagraph = () => {
    if (!paragraphLines.length) return;
    const p = document.createElement('p');
    paragraphLines.forEach((line, idx) => {
      if (idx > 0) p.append(document.createElement('br'));
      appendInlineMarkdown(p, line);
    });
    fragment.append(p);
    paragraphLines = [];
  };

  const flushList = () => {
    if (!listEl) return;
    fragment.append(listEl);
    listEl = null;
    listType = null;
  };

  for (let i = 0; i < lines.length; ) {
    const line = lines[i];

    if (line.trimStart().startsWith('```')) {
      flushParagraph();
      flushList();
      const fence = line.trimStart().match(/^(`+)/)[1];
      const codeLines = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith(fence)) {
        codeLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++;
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      code.textContent = codeLines.join('\n');
      pre.append(code);
      fragment.append(pre);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      i++;
      continue;
    }

    const headingMatch = line.match(/^(#{1,3})\s+(.+)$/);
    if (headingMatch) {
      flushParagraph();
      flushList();
      const el = document.createElement('p');
      const strong = document.createElement('strong');
      appendInlineMarkdown(strong, headingMatch[2]);
      el.append(strong);
      fragment.append(el);
      i++;
      continue;
    }

    const bulletMatch = line.match(/^\s*[-*]\s+(.+)$/);
    if (bulletMatch) {
      flushParagraph();
      if (listType !== 'ul') {
        flushList();
        listEl = document.createElement('ul');
        listType = 'ul';
      }
      const li = document.createElement('li');
      appendInlineMarkdown(li, bulletMatch[1]);
      listEl.append(li);
      i++;
      continue;
    }

    const numMatch = line.match(/^\s*\d+\.\s+(.+)$/);
    if (numMatch) {
      flushParagraph();
      if (listType !== 'ol') {
        flushList();
        listEl = document.createElement('ol');
        listType = 'ol';
      }
      const li = document.createElement('li');
      appendInlineMarkdown(li, numMatch[1]);
      listEl.append(li);
      i++;
      continue;
    }

    flushList();
    paragraphLines.push(line);
    i++;
  }

  flushParagraph();
  flushList();
  return fragment;
}

/**
 * Append a message bubble (user, assistant, activity, or error) to the assistant chat pane.
 * @param {'user'|'assistant'|'activity'|'error'} role
 * @param {string} text
 * @returns {HTMLElement}
 */
function addAssistantMessage(role, text) {
  const container = $('#assistant-messages');
  container.querySelectorAll('.assistant-message.activity').forEach((node) => node.remove());

  const el = document.createElement('div');
  el.className = `assistant-message ${role}`;
  if (role === 'assistant') {
    el.append(renderMarkdownFragment(text));
  } else {
    el.textContent = text;
  }
  container.append(el);
  container.scrollTop = container.scrollHeight;
  return el;
}

/**
 * Open or close the assistant panel drawer and update workspace layout offsets.
 * @param {boolean} open
 */
function setAssistantOpen(open) {
  if (embeddedPane) return;
  const panel = $('#assistant-panel');
  const main = $('.main');
  panel.hidden = !open;
  main.classList.toggle('assistant-open', open);
  syncNativeDockViews().catch(() => {});
  if (open) {
    $('#assistant-input')?.focus();
  }
}

/**
 * Serialized workspace + active-page refresh after the assistant modifies SQLite state.
 * @returns {Promise<void>}
 */
async function refreshAfterAssistantMutation() {
  if (assistantRefreshPromise) return assistantRefreshPromise;
  assistantRefreshPromise = (async () => {
    try {
      await refresh();
      if (activeId && pages.some((page) => page.id === activeId && page.trashed_by === null)) {
        await openPage(activeId, { skipTab: true, suppressHistory: true });
      }
    } finally {
      assistantRefreshPromise = null;
    }
  })();
  return assistantRefreshPromise;
}

/**
 * Handle streamed assistant lifecycle, activity, mutation, message, and completion events from main.
 * @param {object} payload
 */
function assistantEvent(payload) {
  if (!payload || typeof payload !== 'object') return;

  if (payload.type === 'started') {
    addAssistantMessage('activity', 'Starting assistant…');
    return;
  }

  if (payload.type === 'thinking') {
    addAssistantMessage('activity', 'Thinking…');
    return;
  }

  if (payload.type === 'activity') {
    addAssistantMessage('activity', formatActivityLabel(payload.text));
    return;
  }

  if (payload.type === 'workspace-mutated') {
    refreshAfterAssistantMutation().catch(() => {});
    return;
  }

  if (payload.type === 'message') {
    assistantMessages.push({ role: 'assistant', text: payload.text });
    addAssistantMessage('assistant', payload.text);
    return;
  }

  if (payload.type === 'error') {
    addAssistantMessage('error', payload.text || 'Assistant error.');
    return;
  }

  if (payload.type === 'undo-ready') {
    $('#assistant-undo').disabled = false;
    return;
  }

  if (payload.type === 'done') {
    $('#assistant-messages').querySelectorAll('.assistant-message.activity').forEach((node) => node.remove());
    assistantRunning = false;
    $('#assistant-send').hidden = false;
    $('#assistant-stop').hidden = true;
    $('#assistant-undo').disabled = !payload.canUndo;
    if (payload.changed) {
      refreshAfterAssistantMutation().catch(showDockError);
    }
    if (payload.code && payload.stderr) {
      addAssistantMessage('error', payload.stderr.trim());
    }
    return;
  }

  if (payload.type === 'restored') {
    $('#assistant-undo').disabled = !payload.canUndo;
    addAssistantMessage('activity', 'Reverted the last assistant change.');
    refreshAfterAssistantMutation().catch(showDockError);
  }
}

/**
 * Auto-size the assistant compose textarea height smoothly between `82px` and `200px`.
 */
function resizeAssistantInput() {
  const input = $('#assistant-input');
  if (!input) return;
  input.style.height = '82px';
  input.style.height = `${Math.min(200, Math.max(82, input.scrollHeight))}px`;
}

/**
 * Send the current prompt (or supplied text) to the Codex MCP assistant backend.
 * @param {string} [overrideText]
 */
async function sendAssistant(overrideText = null) {
  if (assistantRunning) return;
  const input = $('#assistant-input');
  const text = (overrideText !== null ? overrideText : input.value).trim();
  if (!text) return;

  if (overrideText === null) {
    input.value = '';
    resizeAssistantInput();
  }

  await flushLocalEdits();
  assistantRunning = true;
  $('#assistant-send').hidden = true;
  $('#assistant-stop').hidden = false;

  addAssistantMessage('user', text);
  const historySnapshot = [...assistantMessages];
  assistantMessages.push({ role: 'user', text });

  const started = await window.noteon.startAssistant({
    request: text,
    page: activePage ? { id: activePage.id, title: activePage.title, isDatabase: activePage.isDatabase } : null,
    history: historySnapshot,
  });

  if (!started) {
    assistantRunning = false;
    $('#assistant-send').hidden = false;
    $('#assistant-stop').hidden = true;
  }
}

/**
 * Bind click handlers on starter suggestion buttons inside `#assistant-messages`.
 */
function bindAssistantSuggestions() {
  document.querySelectorAll('#assistant-messages .suggestion').forEach((button) => {
    button.onclick = () => {
      if (button.dataset.fillOnly === 'true') {
        const input = $('#assistant-input');
        input.value = `${button.textContent.replace(/…$/, '')} `;
        input.focus();
        resizeAssistantInput();
      } else {
        sendAssistant(button.textContent).catch(showDockError);
      }
    };
  });
}

/**
 * Reset `#assistant-messages` back to its initial welcome text and starter suggestion chips.
 */
function renderAssistantSuggestions() {
  const container = $('#assistant-messages');
  container.innerHTML = '';

  const intro = document.createElement('p');
  intro.className = 'muted';
  intro.textContent = 'Ask me to read, write, organize or restructure your notes and tables.';
  container.append(intro);

  for (const item of ASSISTANT_STARTER_SUGGESTIONS) {
    const btn = document.createElement('button');
    btn.className = 'suggestion';
    btn.textContent = item.text;
    if (item.fillOnly) btn.dataset.fillOnly = 'true';
    container.append(btn);
  }
  bindAssistantSuggestions();
}

/**
 * Wire UI controls, settings persistence, and IPC listeners for the AI Assistant panel.
 */
function initAssistant() {
  if (embeddedPane) return;

  bindAssistantSuggestions();
  $('#assistant-toggle').onclick = () => setAssistantOpen($('#assistant-panel').hidden);
  $('#assistant-close').onclick = () => setAssistantOpen(false);
  $('#assistant-send').onclick = () => sendAssistant().catch(showDockError);
  $('#assistant-stop').onclick = () => window.noteon.stopAssistant().catch(showDockError);

  $('#assistant-clear').onclick = () => {
    assistantMessages = [];
    renderAssistantSuggestions();
  };

  $('#assistant-undo').onclick = async () => {
    const result = await window.noteon.undoAssistant();
    if (result?.error) {
      addAssistantMessage('error', result.error);
    }
  };

  $('#assistant-settings-toggle').onclick = async () => {
    const panel = $('#assistant-settings');
    panel.hidden = !panel.hidden;
    if (!panel.hidden) {
      const cfg = await window.noteon.getAssistantSettings();
      $('#assistant-model').value = cfg?.model || 'gpt-6-luna';
      $('#assistant-effort').value = cfg?.effort || 'high';
    }
  };

  $('#assistant-settings-save').onclick = async () => {
    await window.noteon.setAssistantSettings({
      model: $('#assistant-model').value.trim() || 'gpt-6-luna',
      effort: $('#assistant-effort').value || 'high',
    });
    $('#assistant-settings').hidden = true;
  };

  const input = $('#assistant-input');
  input.oninput = resizeAssistantInput;
  input.onkeydown = (event) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      sendAssistant().catch(showDockError);
    }
  };

  window.noteon.onAssistantEvent(assistantEvent);
}
