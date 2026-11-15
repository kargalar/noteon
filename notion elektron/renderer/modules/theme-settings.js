/**
 * Sidebar pinning, panel resizers, light/dark appearance, and custom section theme settings.
 */

const THEME_FIELD_LABELS = {
  sidebar: 'Sidebar background',
  text: 'Text',
  muted: 'Muted text',
  border: 'Borders',
  accent_soft: 'Soft accent',
  hover: 'Hover background',
  code: 'Code background',
  gallery_card: 'Gallery card',
  gallery_border: 'Gallery border',
  board_card: 'Board card',
};

const THEME_SECTION_FIELDS = {
  general: [],
  sidebar: ['sidebar'],
  editor: ['text', 'muted', 'border', 'accent_soft', 'hover', 'code'],
  header: ['text', 'muted', 'border'],
  gallery: ['gallery_card', 'gallery_border'],
  board: ['board_card'],
  table: ['text', 'muted', 'border'],
};

const THEME_VARIABLES = {
  sidebar: '--sidebar',
  text: '--text',
  muted: '--muted',
  border: '--line',
  accent_soft: '--accent-soft',
  hover: '--hover',
  code: '--code',
  gallery_card: '--gallery-card',
  gallery_border: '--gallery-border',
  board_card: '--board-card',
};

const DARK_THEME_DEFAULTS = {
  sidebar: '#20211f',
  text: '#e8e9e5',
  muted: '#a0a39a',
  border: '#353733',
  accent_soft: '#2b2d2a',
  hover: '#2b2d2a',
  code: '#232522',
  gallery_card: '#20211f',
  gallery_border: '#353733',
  board_card: '#20211f',
};

const LIGHT_THEME_DEFAULTS = {
  sidebar: '#f7f7f5',
  text: '#242521',
  muted: '#86877f',
  border: '#e9e9e5',
  accent_soft: '#eeefeb',
  hover: '#eeefeb',
  code: '#f3f5ec',
  gallery_card: '#f7f7f5',
  gallery_border: '#e9e9e5',
  board_card: '#fafaf6',
};

function setPanelWidth(panel, width) {
  const isSidebar = panel === 'sidebar';
  const key = isSidebar ? 'sidebar_width' : 'assistant_panel_width';
  const min = isSidebar ? 210 : 280;
  const max = isSidebar ? 480 : 640;
  const fallback = isSidebar ? 258 : 380;
  const clamped = Math.round(Math.max(min, Math.min(max, Number(width) || fallback)));

  settings[key] = String(clamped);
  document.documentElement.style.setProperty(isSidebar ? '--sidebar-width' : '--assistant-width', `${clamped}px`);
  return clamped;
}

function applyPanelWidths() {
  setPanelWidth('sidebar', settings.sidebar_width);
  setPanelWidth('assistant', settings.assistant_panel_width);
}

function installPanelResize(handleId, panel) {
  const handle = $(`#${handleId}`);
  const isSidebar = panel === 'sidebar';
  const delta = isSidebar ? 1 : -1;
  const getHost = () => (isSidebar ? $('.sidebar') : $('#assistant-panel'));
  const settingKey = isSidebar ? 'sidebar_width' : 'assistant_panel_width';
  let drag = null;

  handle.addEventListener('pointerdown', (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const host = getHost();
    drag = { x: event.clientX, width: host.getBoundingClientRect().width };
    if (isSidebar) host.classList.add('resizing');
    handle.setPointerCapture(event.pointerId);
  });

  handle.addEventListener('pointermove', (event) => {
    if (!drag) return;
    setPanelWidth(panel, drag.width + (event.clientX - drag.x) * delta);
    syncNativeDockViews().catch(() => {});
  });

  const finish = () => {
    if (!drag) return;
    const width = getHost().getBoundingClientRect().width;
    if (isSidebar) $('.sidebar').classList.remove('resizing');
    drag = null;
    syncNativeDockViews().catch(() => {});
    window.noteon.setSetting({ key: settingKey, value: String(Math.round(width)) }).catch(showDockError);
  };

  handle.addEventListener('pointerup', finish);
  handle.addEventListener('pointercancel', finish);
  handle.addEventListener('keydown', (event) => {
    if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
    event.preventDefault();
    const change = (event.key === 'ArrowRight' ? 1 : -1) * 12 * delta;
    const width = setPanelWidth(panel, getHost().getBoundingClientRect().width + change);
    syncNativeDockViews().catch(() => {});
    window.noteon.setSetting({ key: settingKey, value: String(width) }).catch(showDockError);
  });
}

function applySidebarPin() {
  const pinned = settings.sidebar_pinned !== 'false';
  $('.sidebar').classList.toggle('autohide', !pinned);
  $('#sidebar-pin').textContent = pinned ? '📌' : '📍';
  $('#sidebar-pin').title = pinned ? 'Unpin sidebar for auto-hide' : 'Pin sidebar';
  $('#sidebar-pin').setAttribute('aria-label', pinned ? 'Unpin sidebar' : 'Pin sidebar');
}

function toggleSidebar() {
  $('.sidebar').classList.toggle('collapsed');
  $('#show-sidebar').hidden = !$('.sidebar').classList.contains('collapsed');
  syncNativeDockViews().catch(() => {});
}

function readCustomTheme() {
  try {
    return typeof settings.custom_theme === 'string' ? JSON.parse(settings.custom_theme || '{}') : settings.custom_theme || {};
  } catch {
    return {};
  }
}

function applyAppearance() {
  const dark = settings.theme ? settings.theme === 'dark' : settings.dark === 'true';
  document.body.classList.toggle('dark', dark);

  const background = settings[dark ? 'dark_background' : 'light_background'];
  if (background && /^#[0-9a-f]{6}$/i.test(background)) {
    document.body.style.setProperty('--bg', background);
    document.body.style.setProperty('--panel', background);
  }
  if (settings.accent_color && /^#[0-9a-f]{6}$/i.test(settings.accent_color)) {
    document.body.style.setProperty('--accent', settings.accent_color);
  }

  const theme = readCustomTheme();
  const custom = theme[dark ? 'dark' : 'light'] || {};
  for (const variable of Object.values(THEME_VARIABLES)) {
    document.body.style.removeProperty(variable);
  }
  if (background && /^#[0-9a-f]{6}$/i.test(background)) {
    document.body.style.setProperty('--sidebar', `color-mix(in srgb,${background} 89%,${dark ? 'white' : 'black'})`);
  }
  for (const [key, variable] of Object.entries(THEME_VARIABLES)) {
    if (/^#[0-9a-f]{6}$/i.test(custom[key] || '')) {
      document.body.style.setProperty(variable, custom[key]);
    }
  }
}

async function openSettingsDialog(section = null) {
  settings = await window.noteon.getSettings();
  $('#theme').value = settings.theme || (settings.dark === 'true' ? 'dark' : 'light');
  $('#font-scale').value = settings.font_scale || 1;
  $('#accent-color').value = /^#[0-9a-f]{6}$/i.test(settings.accent_color || '') ? settings.accent_color : '#65875c';

  const dark = $('#theme').value === 'dark';
  const storedBg = settings[dark ? 'dark_background' : 'light_background'];
  $('#background-color').value = /^#[0-9a-f]{6}$/i.test(storedBg || '') ? storedBg : dark ? '#191a18' : '#ffffff';

  if (section) {
    $('#custom-theme-section').value = section;
  }
  renderCustomThemeSettings();
  $('#settings-dialog').showModal();
}

function renderCustomThemeSettings() {
  const root = $('#custom-theme-colors');
  if (!root) return;
  root.innerHTML = '';

  const section = $('#custom-theme-section').value;
  const mode = $('#theme').value;
  const theme = readCustomTheme();
  const custom = theme[mode] || {};
  const defaults = mode === 'dark' ? DARK_THEME_DEFAULTS : LIGHT_THEME_DEFAULTS;

  for (const key of THEME_SECTION_FIELDS[section] || []) {
    const row = document.createElement('label');
    row.className = 'theme-color-row';
    row.append(document.createTextNode(THEME_FIELD_LABELS[key]));

    const controls = document.createElement('span');
    controls.className = 'theme-color-controls';

    const input = document.createElement('input');
    input.type = 'color';
    input.value = /^#[0-9a-f]{6}$/i.test(custom[key] || '') ? custom[key] : defaults[key];
    input.oninput = async () => {
      const next = readCustomTheme();
      next[mode] = { ...(next[mode] || {}), [key]: input.value };
      settings.custom_theme = JSON.stringify(next);
      applyAppearance();
      await window.noteon.setSetting({ key: 'custom_theme', value: settings.custom_theme });
    };

    const resetBtn = document.createElement('button');
    resetBtn.type = 'button';
    resetBtn.className = 'theme-color-reset';
    resetBtn.textContent = '↺';
    resetBtn.title = `Reset ${THEME_FIELD_LABELS[key]} color`;
    resetBtn.onclick = async (event) => {
      event.preventDefault();
      event.stopPropagation();
      const next = readCustomTheme();
      if (next[mode]) delete next[mode][key];
      settings.custom_theme = JSON.stringify(next);
      applyAppearance();
      await window.noteon.setSetting({ key: 'custom_theme', value: settings.custom_theme });
      renderCustomThemeSettings();
    };

    controls.append(input, resetBtn);
    row.append(controls);
    root.append(row);
  }

  if (!root.children.length) {
    const note = document.createElement('p');
    note.className = 'muted';
    note.textContent = 'Choose a section to customize its colors.';
    root.append(note);
  }
}

function initThemeSettings() {
  installPanelResize('sidebar-resize', 'sidebar');
  installPanelResize('assistant-resize', 'assistant');

  $('#settings').onclick = () => openSettingsDialog();
  $('#theme').onchange = async (event) => {
    const dark = event.target.value === 'dark';
    settings.theme = event.target.value;
    settings.dark = String(dark);
    applyAppearance();
    $('#background-color').value = settings[dark ? 'dark_background' : 'light_background'] || (dark ? '#191a18' : '#ffffff');
    await window.noteon.setSetting({ key: 'theme', value: event.target.value });
    await window.noteon.setSetting({ key: 'dark', value: dark });
  };

  $('#accent-color').oninput = async (event) => {
    settings.accent_color = event.target.value;
    applyAppearance();
    await window.noteon.setSetting({ key: 'accent_color', value: event.target.value });
  };

  $('#background-color').oninput = async (event) => {
    const key = $('#theme').value === 'dark' ? 'dark_background' : 'light_background';
    settings[key] = event.target.value;
    applyAppearance();
    await window.noteon.setSetting({ key, value: event.target.value });
  };

  $('#font-scale').oninput = async (event) => {
    document.documentElement.style.fontSize = `${15 * Number(event.target.value)}px`;
    await window.noteon.setSetting({ key: 'font_scale', value: event.target.value });
  };

  $('#collapse').onclick = toggleSidebar;
  $('#show-sidebar').onclick = toggleSidebar;

  $('#sidebar-pin').onclick = async () => {
    settings.sidebar_pinned = String(settings.sidebar_pinned === 'false');
    applySidebarPin();
    await window.noteon.setSetting({ key: 'sidebar_pinned', value: settings.sidebar_pinned });
  };

  $('#toggle-theme').onclick = async () => {
    const dark = !(settings.theme ? settings.theme === 'dark' : settings.dark === 'true');
    settings.theme = dark ? 'dark' : 'light';
    settings.dark = String(dark);
    applyAppearance();
    $('#theme').value = settings.theme;
    await window.noteon.setSetting({ key: 'theme', value: settings.theme });
    await window.noteon.setSetting({ key: 'dark', value: settings.dark });
  };

  $('#custom-theme-section').onchange = renderCustomThemeSettings;
  $('#theme').addEventListener('change', renderCustomThemeSettings);

  $('#reset-theme-section').onclick = async () => {
    const mode = $('#theme').value;
    const section = $('#custom-theme-section').value;
    const theme = readCustomTheme();
    theme[mode] = { ...(theme[mode] || {}) };
    for (const key of THEME_SECTION_FIELDS[section] || []) {
      delete theme[mode][key];
    }
    settings.custom_theme = JSON.stringify(theme);
    applyAppearance();
    await window.noteon.setSetting({ key: 'custom_theme', value: settings.custom_theme });
    renderCustomThemeSettings();
  };

  $('#reset-theme-all').onclick = async () => {
    if (!confirm('Reset custom colors for light and dark themes?')) return;
    settings.custom_theme = JSON.stringify({ light: {}, dark: {} });
    applyAppearance();
    await window.noteon.setSetting({ key: 'custom_theme', value: settings.custom_theme });
    renderCustomThemeSettings();
  };
}
