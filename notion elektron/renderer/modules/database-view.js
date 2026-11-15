/**
 * Interactive database view rendering: table, gallery, and kanban board views,
 * sorting, column resizing, viewport height resizing, number calculations,
 * inline property editors, bulk editing, drag-and-drop, and page record management.
 */

/**
 * Safely parse a database's `view_settings` JSON string into a plain object.
 * @param {string|object|null|undefined} raw
 * @returns {Record<string, any>}
 */
function parseDbViewSettings(raw) {
  if (!raw) return {};
  if (typeof raw === 'object') return raw;
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

/**
 * Compute visible properties for a given database view (`'table' | 'gallery' | 'board'`).
 * @param {object} d
 * @param {'table'|'gallery'|'board'} view
 * @returns {object[]}
 */
function databaseVisibleProperties(d, view) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const configured = viewSettings[`${view}_visible_properties`];
  const allProps = Array.isArray(d.properties) ? d.properties : [];
  if (!Array.isArray(configured)) return allProps;
  const allowed = new Set(configured.map(Number));
  return allProps.filter((prop) => allowed.has(Number(prop.id)));
}

/**
 * Normalize a page/record title string for natural sorting.
 * @param {string} value
 * @returns {string}
 */
function sortTitle(value) {
  return String(value || '').trim().toLocaleLowerCase();
}

/**
 * Resolve the display/sort raw value of a property on a database record.
 * @param {object} property
 * @param {object} record
 * @returns {string}
 */
function databaseRecordValue(property, record) {
  if (property.kind === 'created_time') return formatGeneratedDate(record.created_at);
  if (property.kind === 'last_edited_time') return formatGeneratedDate(record.updated_at);
  if (property.kind === 'id') return `#${record.id}`;
  return String(record.values?.[property.id] ?? '');
}

/**
 * Render an escaped HTML representation of a record's property value (chips, checkboxes, formatted numbers, etc.).
 * @param {object} property
 * @param {object} record
 * @returns {string}
 */
function databasePropertyDisplayHTML(property, record) {
  const raw = databaseRecordValue(property, record);
  if (property.kind === 'checkbox') {
    return raw === 'true' ? '☑' : '☐';
  }
  if (!raw) return '<span class="muted">—</span>';

  if (property.kind === 'select' || property.kind === 'status' || property.kind === 'multi-select') {
    const options = propertyOptions(property);
    const items = property.kind === 'multi-select' ? raw.split('|').filter(Boolean) : [raw];
    const multi = property.kind === 'multi-select';
    return items
      .map((name) => {
        const option = options.find((opt) => opt.name === name) || { name };
        const color = propertyOptionColor(option, property.kind);
        const textColor = propertyOptionTextColor(color, multi);
        return `<span class="property-chip" style="--option-color:${esc(color)};color:${esc(textColor)}">${esc(name)}</span>`;
      })
      .join(' ');
  }

  if (property.kind === 'number') {
    return esc(formatNumberDisplay(raw));
  }
  return esc(raw);
}

/**
 * Return a sorted shallow copy of `d.records` respecting `sort_key` and `sort_descending`.
 * @param {object} d
 * @returns {object[]}
 */
function sortDatabase(d) {
  const list = [...(d.records || [])];
  if (d.sort_key === 'manual') return list;

  const desc = Boolean(d.sort_descending);
  const dir = desc ? -1 : 1;
  const key = String(d.sort_key || 'created_at');
  const prop = !['title', 'priority', 'created_at'].includes(key)
    ? (d.properties || []).find((item) => String(item.id) === key)
    : null;

  list.sort((a, b) => {
    if (key === 'title') {
      const diff = sortTitle(a.title).localeCompare(sortTitle(b.title), undefined, { numeric: true, sensitivity: 'base' });
      return (diff || a.id - b.id) * dir;
    }
    if (key === 'priority') {
      const diff = (Number(a.priority) || 0) - (Number(b.priority) || 0);
      return diff ? diff * dir : a.id - b.id;
    }
    if (key === 'created_at') {
      const diff = (Number(a.created_at) || 0) - (Number(b.created_at) || 0);
      return diff ? diff * dir : a.id - b.id;
    }
    if (prop) {
      if (prop.kind === 'created_time') {
        return ((Number(a.created_at) || 0) - (Number(b.created_at) || 0)) * dir;
      }
      if (prop.kind === 'last_edited_time') {
        return ((Number(a.updated_at) || 0) - (Number(b.updated_at) || 0)) * dir;
      }
      if (prop.kind === 'id') {
        return (a.id - b.id) * dir;
      }
      const valA = databaseRecordValue(prop, a);
      const valB = databaseRecordValue(prop, b);
      if (prop.kind === 'number') {
        const emptyA = !valA.trim();
        const emptyB = !valB.trim();
        if (emptyA !== emptyB) return emptyA ? 1 : -1;
        const numA = Number(valA);
        const numB = Number(valB);
        if (Number.isFinite(numA) && Number.isFinite(numB) && numA !== numB) {
          return (numA - numB) * dir;
        }
      }
      const cmp = valA.localeCompare(valB, undefined, { numeric: true, sensitivity: 'base' });
      return cmp ? cmp * dir : a.id - b.id;
    }
    return a.id - b.id;
  });

  return list;
}

/**
 * Resolve which container and view title correspond to the current database root element.
 * @param {HTMLElement} root
 * @param {number} id
 */
async function renderDatabaseReplace(root, id) {
  await renderDatabase(root, id);
  applyPageWriteAccess();
}

/**
 * Open the modal dialog allowing configuration of visible columns, card metadata, and new-record opening mode.
 * @param {HTMLElement} root
 * @param {number} id
 * @param {object} d
 */
function openDatabaseViewOptions(root, id, d) {
  const view = d.view_mode || 'table';
  const viewSettings = parseDbViewSettings(d.view_settings);
  const currentVisible = new Set(databaseVisibleProperties(d, view).map((p) => Number(p.id)));

  showModalDialog('View options', (body, finish) => {
    const form = document.createElement('form');
    form.className = 'db-view-options db-view-dialog';

    const makeToggle = (label, checked) => {
      const row = document.createElement('label');
      row.className = 'db-view-toggle';
      const span = document.createElement('span');
      span.textContent = label;
      const input = document.createElement('input');
      input.type = 'checkbox';
      input.checked = checked;
      row.append(span, input);
      return { row, input };
    };

    const headingProps = document.createElement('strong');
    headingProps.textContent = 'Visible properties';
    form.append(headingProps);

    const propBoxes = (d.properties || []).map((prop) => {
      const { row, input } = makeToggle(prop.name, currentVisible.has(Number(prop.id)));
      form.append(row);
      return { propId: Number(prop.id), input };
    });

    const headingDisplay = document.createElement('strong');
    headingDisplay.textContent = 'Display settings';
    form.append(headingDisplay);

    let createdToggle = null;
    let coverToggle = null;
    let titleToggle = null;
    let descToggle = null;

    if (view === 'table') {
      createdToggle = makeToggle('Show created date', viewSettings.table_show_created_at !== false);
      form.append(createdToggle.row);
    } else if (view === 'gallery') {
      coverToggle = makeToggle('Show page cover', viewSettings.gallery_show_cover !== false);
      titleToggle = makeToggle('Show page title', viewSettings.gallery_show_title !== false);
      descToggle = makeToggle('Show page preview', viewSettings.gallery_show_description !== false);
      createdToggle = makeToggle('Show created date', viewSettings.gallery_show_created_at !== false);
      form.append(coverToggle.row, titleToggle.row, descToggle.row, createdToggle.row);
    } else {
      descToggle = makeToggle('Show page preview', viewSettings.board_show_description === true);
      form.append(descToggle.row);
    }

    const modeLabel = document.createElement('label');
    modeLabel.className = 'db-view-toggle';
    const modeSpan = document.createElement('span');
    modeSpan.textContent = 'Open new records as';
    const modeSelect = document.createElement('select');
    for (const [val, text] of [
      ['focus_name', 'Inline focus'],
      ['dialog', 'Side dialog'],
      ['full_page', 'Full page'],
    ]) {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = text;
      modeSelect.append(opt);
    }
    modeSelect.value = viewSettings.new_record_open_mode || 'focus_name';
    modeLabel.append(modeSpan, modeSelect);
    form.append(modeLabel);

    const actions = document.createElement('div');
    actions.className = 'modal-actions';
    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.onclick = () => finish(false);
    const saveBtn = document.createElement('button');
    saveBtn.type = 'submit';
    saveBtn.className = 'primary';
    saveBtn.textContent = 'Save';
    actions.append(cancelBtn, saveBtn);
    form.append(actions);

    form.onsubmit = async (event) => {
      event.preventDefault();
      const next = {
        [`${view}_visible_properties`]: propBoxes.filter((item) => item.input.checked).map((item) => item.propId),
        new_record_open_mode: modeSelect.value,
      };
      if (view === 'table') {
        next.table_show_created_at = createdToggle.input.checked;
      } else if (view === 'gallery') {
        next.gallery_show_cover = coverToggle.input.checked;
        next.gallery_show_title = titleToggle.input.checked;
        next.gallery_show_description = descToggle.input.checked;
        next.gallery_show_created_at = createdToggle.input.checked;
      } else {
        next.board_show_description = descToggle.input.checked;
      }

      await window.noteon.setDatabaseViewSettings({ id, settings: next });
      finish(true);
      await renderDatabaseReplace(root, id);
    };

    body.append(form);
  });
}

/**
 * Open or focus a newly created database record according to the view's opening preference.
 * @param {HTMLElement} root
 * @param {number} databaseId
 * @param {number} recordId
 * @param {'focus_name'|'dialog'|'full_page'} [mode]
 */
async function openCreatedDatabaseRecord(root, databaseId, recordId, mode = 'focus_name') {
  if (mode === 'full_page') {
    await openPage(recordId);
    return;
  }
  if (mode === 'dialog') {
    await renderDatabaseReplace(root, databaseId);
    await openDatabaseRecordDialog(root, recordId);
    return;
  }
  await renderDatabaseReplace(root, databaseId);
  const titleEl = root.querySelector(`[data-record-id="${recordId}"] .record-title`);
  if (titleEl && typeof titleEl.focus === 'function') {
    titleEl.focus();
  } else {
    await openDatabaseRecordDialog(root, recordId);
  }
}

/**
 * Move a list of database records to the workspace trash and update UI selections/tabs.
 * @param {number} databaseId
 * @param {number[]} recordIds
 */
async function trashDatabaseRows(databaseId, recordIds) {
  if (!recordIds?.length) return;
  await beforeTreeMutation();
  await window.noteon.trashDatabaseRecords({ id: databaseId, recordIds });
  recordIds.forEach((rid) => selectedRecordIds.delete(Number(rid)));
  closeTrashedOrDeletedTabs(recordIds);
  await refresh();
}

/**
 * Duplicate all currently selected rows inside `databaseId`.
 * @param {number} databaseId
 */
async function duplicateSelectedDatabaseRows(databaseId) {
  const ids = [...selectedRecordIds];
  if (!ids.length) return;
  await beforeTreeMutation();
  const createdIds = await window.noteon.transferPages({ pageIds: ids, parentId: databaseId, copy: true });
  selectedRecordIds = new Set((createdIds || []).map(Number));
  selectedDatabaseId = databaseId;
  await refresh();
}

/**
 * Copy or cut selected database row pages into the Noteon page clipboard.
 * @param {boolean} [cut]
 */
async function copySelectedDatabasePages(cut = false) {
  if (!selectedRecordIds.size) return;
  await beforeTreeMutation();
  await window.noteon.copyPages({ pageIds: [...selectedRecordIds], cut });
}

/**
 * Paste clipboard pages into the currently focused database (`selectedDatabaseId` or active database page).
 */
async function pasteSelectedPagesIntoCurrentView() {
  const targetDatabaseId = selectedDatabaseId || (activePage?.isDatabase ? activeId : null);
  if (!targetDatabaseId) return;
  await beforeTreeMutation();
  const pasted = await window.noteon.pastePages({ parentId: targetDatabaseId });
  if (pasted?.length) {
    selectedRecordIds = new Set(pasted.map(Number));
    selectedDatabaseId = targetDatabaseId;
    await refresh();
  }
}

/**
 * Serialize selected/target record IDs onto the DataTransfer object for drag-and-drop.
 * @param {DragEvent} event
 * @param {number} recordId
 */
function setDatabaseRecordDrag(event, recordId) {
  const ids =
    selectedRecordIds.has(recordId) && selectedRecordIds.size > 1 ? [...selectedRecordIds] : [recordId];
  event.dataTransfer.effectAllowed = 'move';
  event.dataTransfer.setData('application/x-noteon-record', JSON.stringify(ids));
}

/**
 * Extract dragged database record IDs from a DragEvent (falling back to `[fallbackId]`).
 * @param {DragEvent} event
 * @param {number} fallbackId
 * @returns {number[]}
 */
function databaseDraggedRecordIds(event, fallbackId) {
  try {
    const raw = event.dataTransfer.getData('application/x-noteon-record');
    if (raw) {
      const parsed = JSON.parse(raw).map(Number).filter(Number.isSafeInteger);
      if (parsed.length) return parsed;
    }
  } catch {}
  return fallbackId ? [fallbackId] : [];
}

/**
 * Persist a contentEditable table cell edit (for Name or any editable property column).
 * @param {HTMLElement} cell
 * @param {object} d
 */
async function saveDatabaseTableCell(cell, d) {
  const pageId = Number(cell.dataset.pageId);
  const propId = cell.dataset.propId ? Number(cell.dataset.propId) : null;
  if (!pageId) return;

  if (!propId) {
    const nextTitle = (cell.textContent || '').trim();
    const record = d.records?.find((item) => item.id === pageId);
    if (record && nextTitle === (record.title || '')) return;
    await window.noteon.renamePage({ id: pageId, title: nextTitle });
    if (record) record.title = nextTitle;
    await refresh();
    return;
  }

  const prop = d.properties?.find((item) => item.id === propId);
  if (!prop) return;
  let value = (cell.textContent || '').trim();
  if (prop.kind === 'number' && value) {
    const evaluated = evaluateNumberExpression(value);
    if (evaluated === null) {
      throw new Error('Enter a valid number or math expression.');
    }
    value = evaluated;
    cell.textContent = formatNumberDisplay(value);
  }

  const record = d.records?.find((item) => item.id === pageId);
  const prev = record?.values?.[propId] ?? '';
  if (value === prev) return;
  await window.noteon.setValue({ pageId, propertyId: propId, value });
  if (record) {
    record.values = record.values || {};
    record.values[propId] = value;
  }
}

/**
 * Handle Enter/Tab/Escape/arrow key navigation within a database table cell.
 * @param {KeyboardEvent} event
 * @param {HTMLTableElement} table
 * @param {object} d
 * @param {number} id
 */
function handleDatabaseTableKey(event, table, d, id) {
  const cell = event.target?.closest?.('td[contenteditable="true"], .record-title[contenteditable="true"]');
  if (!cell || !table.contains(cell)) return;

  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    cell.blur();
    const rowsList = [...table.querySelectorAll('tbody tr')];
    const currentRow = cell.closest('tr');
    const colIdx = cell.closest('td')?.cellIndex ?? 1;
    const rowIdx = rowsList.indexOf(currentRow);
    const nextCell = rowsList[rowIdx + 1]?.cells?.[colIdx];
    const focusTarget = nextCell?.querySelector?.('[contenteditable="true"]') || (nextCell?.isContentEditable ? nextCell : null);
    focusTarget?.focus();
    return;
  }

  if (event.key === 'Escape') {
    const pageId = Number(cell.dataset.pageId);
    const propId = cell.dataset.propId ? Number(cell.dataset.propId) : null;
    const record = d.records?.find((item) => item.id === pageId);
    if (record) {
      cell.textContent = propId ? record.values?.[propId] || '' : record.title || '';
    }
    cell.blur();
  }
}

/**
 * Build an inline interactive control (`optionPicker`, checkbox, date/url/number input, or readonly span)
 * for a single database cell or record property.
 * @param {object} property
 * @param {string} value
 * @param {number} pageId
 * @param {{ root?: HTMLElement, databaseId?: number, onUpdated?: () => void }} [hooks]
 * @returns {HTMLElement}
 */
function propertyEditor(property, value, pageId, hooks = {}) {
  const notify = hooks.onUpdated || (() => {});

  if (['created_time', 'last_edited_time', 'id'].includes(property.kind)) {
    const span = document.createElement('span');
    span.className = 'muted';
    if (property.kind === 'id') span.textContent = `#${pageId}`;
    else span.textContent = value || '—';
    return span;
  }

  if (property.kind === 'checkbox') {
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = value === 'true';
    input.onchange = async () => {
      await window.noteon.setValue({ pageId, propertyId: property.id, value: input.checked ? 'true' : 'false' });
      notify();
    };
    return input;
  }

  if (['select', 'multi-select', 'status'].includes(property.kind)) {
    return optionPicker(property, value, {
      onValue: async (nextVal) => {
        await window.noteon.setValue({ pageId, propertyId: property.id, value: nextVal });
        notify();
      },
      onOptions: async (nextOptions) => {
        await window.noteon.updateProperty({ id: property.id, options: nextOptions });
        if (hooks.root && hooks.databaseId) {
          await renderDatabaseReplace(hooks.root, hooks.databaseId);
        }
      },
      onRemove: async (name) => {
        await window.noteon.removePropertyOption({ id: property.id, name });
        if (hooks.root && hooks.databaseId) {
          await renderDatabaseReplace(hooks.root, hooks.databaseId);
        }
      },
    });
  }

  const input = document.createElement('input');
  input.className = 'db-value';
  input.type = property.kind === 'date' ? 'date' : 'text';
  input.placeholder = propertyValuePlaceholder(property.kind, pageId);

  if (property.kind === 'number') {
    input.inputMode = 'decimal';
    let rawValue = String(value ?? '');
    input.value = rawValue ? formatNumberDisplay(rawValue) : '';
    input.onfocus = () => {
      input.value = rawValue;
    };
    input.onblur = async () => {
      const trimmed = input.value.trim();
      if (!trimmed) {
        if (rawValue !== '') {
          rawValue = '';
          await window.noteon.setValue({ pageId, propertyId: property.id, value: '' });
          notify();
        }
        input.value = '';
        return;
      }
      const evaluated = evaluateNumberExpression(trimmed);
      if (evaluated === null) {
        showDockError(new Error('Enter a valid number or math expression.'));
        input.value = rawValue ? formatNumberDisplay(rawValue) : '';
        return;
      }
      if (evaluated !== rawValue) {
        rawValue = evaluated;
        await window.noteon.setValue({ pageId, propertyId: property.id, value: evaluated });
        notify();
      }
      input.value = formatNumberDisplay(rawValue);
    };
    input.onkeydown = (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        input.blur();
      }
    };
    return input;
  }

  input.value = String(value ?? '');
  input.onchange = async () => {
    await window.noteon.setValue({ pageId, propertyId: property.id, value: input.value });
    notify();
  };
  return attachQuickActionWrapper(input, property.kind, input.value, 'db-url-editor', 'db-url-open');
}

/**
 * Open the database property management modal and reload the database after mutations.
 * @param {object} property
 * @param {HTMLElement} root
 * @param {number} databaseId
 */
async function manageDatabaseProperty(property, root, databaseId) {
  await openManagePropertyDialog(property, {
    isCustom: false,
    onRename: (name) => window.noteon.updateProperty({ id: property.id, name }),
    onChangeKind: (kind) => window.noteon.updateProperty({ id: property.id, kind }),
    onUpdateOptions: (options) => window.noteon.updateProperty({ id: property.id, options }),
    onRenameOption: (oldName, newName) => window.noteon.renamePropertyOption({ id: property.id, oldName, newName }),
    onRemoveOption: (name) => window.noteon.removePropertyOption({ id: property.id, name }),
    onMove: (direction) => window.noteon.moveProperty({ id: property.id, direction }),
    onDelete: () => window.noteon.deleteProperty(property.id),
  });
  await renderDatabaseReplace(root, databaseId);
}

/**
 * Create a checkbox input for multi-selecting records in table, gallery, or board views.
 * @param {number} recordId
 * @param {number} databaseId
 * @param {number[]} orderedRecordIds
 * @param {HTMLElement} root
 * @returns {HTMLInputElement}
 */
function recordSelectionControl(recordId, databaseId, orderedRecordIds, root) {
  const box = document.createElement('input');
  box.type = 'checkbox';
  box.className = 'record-select';
  box.checked = selectedRecordIds.has(recordId);
  box.title = 'Select row (Shift+Click for range)';
  box.onclick = (event) => {
    event.stopPropagation();
    if (selectedDatabaseId !== databaseId) {
      selectedRecordIds.clear();
      selectedDatabaseId = databaseId;
      recordSelectionAnchorId = null;
    }

    if (event.shiftKey && recordSelectionAnchorId !== null) {
      const from = orderedRecordIds.indexOf(recordSelectionAnchorId);
      const to = orderedRecordIds.indexOf(recordId);
      if (from >= 0 && to >= 0) {
        const [start, end] = from < to ? [from, to] : [to, from];
        selectedRecordIds = new Set(orderedRecordIds.slice(start, end + 1));
      }
    } else {
      if (box.checked) selectedRecordIds.add(recordId);
      else selectedRecordIds.delete(recordId);
      recordSelectionAnchorId = recordId;
    }
    renderDatabaseReplace(root, databaseId).catch(showDockError);
  };
  return box;
}

/**
 * Apply saved pixel widths to table `<colgroup>` and install horizontal resize grips on `th.resizable-column`.
 * @param {HTMLTableElement} table
 * @param {HTMLTableRowElement} headRow
 * @param {object} d
 * @param {object[]} visible
 * @param {number} id
 */
function applyTableColumnSizing(table, headRow, d, visible, id) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const widths = viewSettings.table_column_widths || {};
  const showCreated = viewSettings.table_show_created_at !== false;

  const colgroup = document.createElement('colgroup');
  const colDefs = [
    { key: '__select', width: 44, fixed: true },
    { key: '__name', settingKey: 'table_name_width', width: Number(viewSettings.table_name_width) || 210 },
    ...visible.map((prop) => ({
      key: String(prop.id),
      propId: prop.id,
      width: Number(widths[String(prop.id)]) || 150,
    })),
    ...(showCreated
      ? [{ key: '__created', settingKey: 'table_created_width', width: Number(viewSettings.table_created_width) || 130 }]
      : []),
    { key: '__actions', width: 64, fixed: true },
  ];

  const colElements = colDefs.map((def) => {
    const col = document.createElement('col');
    col.style.width = `${Math.max(44, def.width)}px`;
    colgroup.append(col);
    return col;
  });
  table.prepend(colgroup);

  const headerCells = [...headRow.children];
  colDefs.forEach((def, index) => {
    if (def.fixed) return;
    const th = headerCells[index];
    if (!th) return;
    th.classList.add('resizable-column');
    const grip = document.createElement('div');
    grip.className = 'column-grip';
    grip.title = 'Drag to resize column';

    grip.onpointerdown = (event) => {
      if (!pageWriteAccess || event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      grip.setPointerCapture(event.pointerId);
      const startX = event.clientX;
      const startWidth = th.getBoundingClientRect().width;
      let nextWidth = startWidth;

      const onMove = (moveEvent) => {
        nextWidth = Math.max(64, Math.min(1200, Math.round(startWidth + (moveEvent.clientX - startX))));
        colElements[index].style.width = `${nextWidth}px`;
      };

      const finish = async () => {
        grip.removeEventListener('pointermove', onMove);
        grip.removeEventListener('pointerup', finish);
        grip.removeEventListener('pointercancel', finish);
        const update = {};
        if (def.settingKey) {
          update[def.settingKey] = nextWidth;
        } else if (def.propId) {
          update.table_column_widths = { ...(parseDbViewSettings(d.view_settings).table_column_widths || {}), [String(def.propId)]: nextWidth };
        }
        await window.noteon.setDatabaseViewSettings({ id, settings: update });
        d.view_settings = JSON.stringify({ ...parseDbViewSettings(d.view_settings), ...update });
      };

      grip.addEventListener('pointermove', onMove);
      grip.addEventListener('pointerup', finish);
      grip.addEventListener('pointercancel', finish);
    };

    th.append(grip);
  });
}

/**
 * Parse a stored number cell string into a finite JS number, or `null` if blank/invalid.
 * @param {string} raw
 * @returns {number|null}
 */
function parseCalculationNumber(raw) {
  const text = String(raw ?? '').trim();
  if (!text) return null;
  const num = Number(text);
  return Number.isFinite(num) ? num : null;
}

/**
 * Format an aggregated calculation metric nicely for table footer display.
 * @param {number} value
 * @returns {string}
 */
function formatCalculationNumber(value) {
  if (!Number.isFinite(value)) return '—';
  const rounded = Math.abs(value - Math.round(value)) < 1e-9 ? String(Math.round(value)) : Number(value.toFixed(4)).toString();
  return formatNumberDisplay(rounded);
}

/**
 * Recompute summary footers (`sum`, `average`, `min`, `max`, `median`, `count`) across table columns.
 * @param {HTMLTableElement} table
 * @param {object} d
 * @param {object[]} visible
 */
function updateTableCalculationFooter(table, d, visible) {
  const tfoot = table.querySelector('tfoot');
  if (!tfoot) return;
  const viewSettings = parseDbViewSettings(d.view_settings);
  const calcs = viewSettings.table_column_calculations || {};
  const records = d.records || [];

  const countCell = tfoot.querySelector('[data-calc-summary="count"]');
  if (countCell) {
    countCell.textContent = `${records.length} items`;
  }

  for (const prop of visible) {
    if (prop.kind !== 'number') continue;
    const output = tfoot.querySelector(`[data-calc-output="${prop.id}"]`);
    if (!output) continue;
    const mode = calcs[String(prop.id)] || 'none';
    if (mode === 'none') {
      output.textContent = '';
      continue;
    }
    const nums = records.map((rec) => parseCalculationNumber(rec.values?.[prop.id])).filter((n) => n !== null);
    if (mode === 'count') {
      output.textContent = String(nums.length);
      continue;
    }
    if (!nums.length) {
      output.textContent = '—';
      continue;
    }
    if (mode === 'sum') {
      output.textContent = formatCalculationNumber(nums.reduce((a, b) => a + b, 0));
    } else if (mode === 'average') {
      output.textContent = formatCalculationNumber(nums.reduce((a, b) => a + b, 0) / nums.length);
    } else if (mode === 'min') {
      output.textContent = formatCalculationNumber(Math.min(...nums));
    } else if (mode === 'max') {
      output.textContent = formatCalculationNumber(Math.max(...nums));
    } else if (mode === 'median') {
      const sorted = [...nums].sort((a, b) => a - b);
      const mid = Math.floor(sorted.length / 2);
      const med = sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
      output.textContent = formatCalculationNumber(med);
    }
  }
}

/**
 * Attach a `<tfoot>` row to the database table with per-column aggregation selectors for numeric fields.
 * @param {HTMLTableElement} table
 * @param {object} d
 * @param {object[]} visible
 */
function appendTableCalculationFooter(table, d, visible) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const calcs = viewSettings.table_column_calculations || {};
  const showCreated = viewSettings.table_show_created_at !== false;

  const tfoot = document.createElement('tfoot');
  const row = document.createElement('tr');

  const spacer = document.createElement('td');
  const nameSummary = document.createElement('td');
  nameSummary.dataset.calcSummary = 'count';
  row.append(spacer, nameSummary);

  for (const prop of visible) {
    const td = document.createElement('td');
    if (prop.kind === 'number') {
      const select = document.createElement('select');
      select.className = 'calc-select';
      select.title = `Calculate ${prop.name}`;
      for (const [val, label] of [
        ['none', 'Calculate'],
        ['sum', 'Sum'],
        ['average', 'Average'],
        ['min', 'Min'],
        ['max', 'Max'],
        ['median', 'Median'],
        ['count', 'Count'],
      ]) {
        const opt = document.createElement('option');
        opt.value = val;
        opt.textContent = label;
        select.append(opt);
      }
      select.value = calcs[String(prop.id)] || 'none';
      const output = document.createElement('div');
      output.dataset.calcOutput = String(prop.id);
      output.style.marginTop = '3px';
      select.onchange = async () => {
        const nextCalcs = { ...(parseDbViewSettings(d.view_settings).table_column_calculations || {}), [String(prop.id)]: select.value };
        await window.noteon.setDatabaseViewSettings({ id: d.page_id, settings: { table_column_calculations: nextCalcs } });
        d.view_settings = JSON.stringify({ ...parseDbViewSettings(d.view_settings), table_column_calculations: nextCalcs });
        updateTableCalculationFooter(table, d, visible);
      };
      td.append(select, output);
    }
    row.append(td);
  }

  if (showCreated) row.append(document.createElement('td'));
  row.append(document.createElement('td'));
  tfoot.append(row);
  table.append(tfoot);
  updateTableCalculationFooter(table, d, visible);
}

/**
 * Attach a shared bottom drag grip to resize a database table or card/board viewport vertically
 * and persist `table_height` in `view_settings`.
 * @param {{ host: HTMLElement, viewport: HTMLElement, databaseId: number, d: object, defaultViewportHeight: number, headerAllowance: number, minViewportHeight: number, maxViewportHeight: number }} config
 */
function attachVerticalResizeGrip({
  host,
  viewport,
  databaseId,
  d,
  defaultViewportHeight,
  headerAllowance,
  minViewportHeight,
  maxViewportHeight,
}) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const applyHeight = (savedHeight) => {
    const targetHeight =
      savedHeight !== null && savedHeight !== undefined
        ? Math.max(minViewportHeight, Math.min(maxViewportHeight, Number(savedHeight) + headerAllowance))
        : defaultViewportHeight;
    viewport.style.maxHeight = `${targetHeight}px`;
  };
  applyHeight(viewSettings.table_height ?? null);

  const grip = document.createElement('div');
  grip.className = 'db-table-size-grip';
  grip.tabIndex = 0;
  grip.title = 'Drag to resize view height (double-click to reset)';

  grip.ondblclick = async () => {
    applyHeight(null);
    await window.noteon.setDatabaseViewSettings({ id: databaseId, settings: { table_height: null } });
  };

  grip.onpointerdown = (event) => {
    if (!pageWriteAccess || event.button !== 0) return;
    event.preventDefault();
    grip.setPointerCapture(event.pointerId);
    const startY = event.clientY;
    const startHeight = viewport.getBoundingClientRect().height || defaultViewportHeight;
    let currentHeight = startHeight;

    const onMove = (moveEvent) => {
      currentHeight = Math.max(minViewportHeight, Math.min(maxViewportHeight, Math.round(startHeight + (moveEvent.clientY - startY))));
      viewport.style.maxHeight = `${currentHeight}px`;
      viewport.style.height = `${currentHeight}px`;
    };

    const finish = async () => {
      grip.removeEventListener('pointermove', onMove);
      grip.removeEventListener('pointerup', finish);
      grip.removeEventListener('pointercancel', finish);
      const saved = Math.max(64, Math.min(2400, Math.round(currentHeight - headerAllowance)));
      await window.noteon.setDatabaseViewSettings({ id: databaseId, settings: { table_height: saved } });
    };

    grip.addEventListener('pointermove', onMove);
    grip.addEventListener('pointerup', finish);
    grip.addEventListener('pointercancel', finish);
  };

  host.append(viewport, grip);
}

/**
 * Mount a scrollable, vertically resizable database table viewport (`106..2442` px).
 * @param {HTMLElement} host
 * @param {HTMLTableElement} table
 * @param {object} d
 * @param {number} id
 */
function mountResizableDatabaseTable(host, table, d, id) {
  const viewport = document.createElement('div');
  viewport.className = 'db-table-viewport';
  viewport.append(table);

  const defaultRowsHeight = Math.min(Math.max((d.records?.length || 0) * 44, 44), 648);
  attachVerticalResizeGrip({
    host,
    viewport,
    databaseId: id,
    d,
    defaultViewportHeight: defaultRowsHeight + 42,
    headerAllowance: 42,
    minViewportHeight: 106,
    maxViewportHeight: 2442,
  });
}

/**
 * Mount a scrollable, vertically resizable gallery or board viewport (`96..2400` px).
 * @param {HTMLElement} host
 * @param {HTMLElement} inner
 * @param {object} d
 * @param {number} id
 * @param {'gallery'|'board'|'fallback'} kind
 * @param {Map<string, number>} [boardCounts]
 */
function mountResizableDatabaseCards(host, inner, d, id, kind, boardCounts = new Map()) {
  const viewport = document.createElement('div');
  viewport.className = kind === 'board' ? 'db-cards-viewport db-cards-board' : 'db-cards-viewport';
  viewport.append(inner);

  const viewSettings = parseDbViewSettings(d.view_settings);
  let defaultHeight;
  if (kind === 'gallery') {
    const cols = Math.max(1, Number(d.gallery_columns) || 3);
    const cardHeight = viewSettings.gallery_show_cover === false ? 190 : 315;
    const rowsCount = Math.ceil((d.records?.length || 0) / cols);
    defaultHeight = Math.min(760, Math.max(96, rowsCount * (cardHeight + 12)));
  } else if (kind === 'board') {
    defaultHeight = Math.min(648, Math.max(96, Math.max(1, ...boardCounts.values()) * 126 + 38));
  } else {
    defaultHeight = Math.min(760, Math.max(96, (d.records?.length || 0) * 126 + 20));
  }

  attachVerticalResizeGrip({
    host,
    viewport,
    databaseId: id,
    d,
    defaultViewportHeight: defaultHeight,
    headerAllowance: 0,
    minViewportHeight: 96,
    maxViewportHeight: 2400,
  });
}

/**
 * Handle clicks on gallery or board cards, ignoring interactive inputs/buttons.
 * @param {MouseEvent} event
 * @param {HTMLElement} root
 * @param {number} databaseId
 * @param {number} recordId
 */
function handleDatabaseCardClick(event, root, databaseId, recordId) {
  if (event.target.closest?.('input, button, select, textarea, a, .option-picker')) return;
  if (event.ctrlKey || event.metaKey) {
    if (selectedDatabaseId !== databaseId) {
      selectedRecordIds.clear();
      selectedDatabaseId = databaseId;
      recordSelectionAnchorId = null;
    }
    if (selectedRecordIds.has(recordId)) selectedRecordIds.delete(recordId);
    else selectedRecordIds.add(recordId);
    recordSelectionAnchorId = recordId;
    renderDatabaseReplace(root, databaseId).catch(showDockError);
    return;
  }
  openDatabaseRecordDialog(root, recordId).catch(showDockError);
}

/**
 * Enable pressing Delete/Backspace on focused card viewports to move selected rows to trash.
 * @param {HTMLElement} container
 * @param {number} databaseId
 */
function attachCardContainerDeleteKey(container, databaseId) {
  container.tabIndex = 0;
  container.onkeydown = (event) => {
    if (!pageWriteAccess) return;
    if ((event.key !== 'Delete' && event.key !== 'Backspace') || selectedDatabaseId !== databaseId || !selectedRecordIds.size) {
      return;
    }
    if (event.target.matches?.('input, textarea, select, [contenteditable="true"]')) return;
    event.preventDefault();
    trashDatabaseRows(databaseId, [...selectedRecordIds]).catch(showDockError);
  };
}

/**
 * Render the table view body inside `host`.
 * @param {HTMLElement} host
 * @param {HTMLElement} root
 * @param {number} id
 * @param {object} d
 */
function renderDatabaseTableBody(host, root, id, d) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const visible = databaseVisibleProperties(d, 'table');
  const showCreated = viewSettings.table_show_created_at !== false;
  const records = sortDatabase(d);
  const orderedIds = records.map((r) => r.id);

  const table = document.createElement('table');
  table.className = 'db-table';

  const thead = document.createElement('thead');
  const headRow = document.createElement('tr');

  const selectTh = document.createElement('th');
  selectTh.className = 'select-cell';
  const selectAll = document.createElement('input');
  selectAll.type = 'checkbox';
  selectAll.dataset.selectAll = '1';
  selectAll.checked = orderedIds.length > 0 && orderedIds.every((rid) => selectedRecordIds.has(rid));
  selectAll.onchange = () => {
    selectedDatabaseId = id;
    if (selectAll.checked) {
      orderedIds.forEach((rid) => selectedRecordIds.add(rid));
    } else {
      orderedIds.forEach((rid) => selectedRecordIds.delete(rid));
    }
    renderDatabaseReplace(root, id).catch(showDockError);
  };
  selectTh.append(selectAll);
  headRow.append(selectTh);

  const nameTh = document.createElement('th');
  nameTh.textContent = 'Name';
  headRow.append(nameTh);

  for (const prop of visible) {
    const th = document.createElement('th');
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = prop.name;
    btn.title = `Manage property "${prop.name}"`;
    btn.onclick = () => manageDatabaseProperty(prop, root, id).catch(showDockError);
    th.append(btn);
    headRow.append(th);
  }

  if (showCreated) {
    const createdTh = document.createElement('th');
    createdTh.textContent = 'Created';
    headRow.append(createdTh);
  }

  const actionTh = document.createElement('th');
  actionTh.textContent = '';
  headRow.append(actionTh);
  thead.append(headRow);
  table.append(thead);

  const tbody = document.createElement('tbody');
  for (const rec of records) {
    const tr = document.createElement('tr');
    tr.dataset.recordId = String(rec.id);
    tr.draggable = pageWriteAccess;

    tr.ondragstart = (event) => {
      if (!pageWriteAccess) return event.preventDefault();
      setDatabaseRecordDrag(event, rec.id);
    };
    tr.ondragover = (event) => {
      if (!pageWriteAccess || !event.dataTransfer?.types?.includes('application/x-noteon-record')) return;
      event.preventDefault();
    };
    tr.ondrop = async (event) => {
      if (!pageWriteAccess || !event.dataTransfer?.types?.includes('application/x-noteon-record')) return;
      event.preventDefault();
      const draggedIds = databaseDraggedRecordIds(event, rec.id);
      if (!draggedIds.length || draggedIds.includes(rec.id)) return;
      const rect = tr.getBoundingClientRect();
      const after = event.clientY - rect.top > rect.height / 2;
      await window.noteon.reorderDatabaseRecord({ id, recordIds: draggedIds, targetId: rec.id, after });
      await renderDatabaseReplace(root, id);
    };

    const selectTd = document.createElement('td');
    selectTd.className = 'select-cell';
    selectTd.append(recordSelectionControl(rec.id, id, orderedIds, root));
    tr.append(selectTd);

    const nameTd = document.createElement('td');
    const titleSpan = document.createElement('span');
    titleSpan.className = 'record-title';
    titleSpan.contentEditable = 'true';
    titleSpan.dataset.pageId = String(rec.id);
    titleSpan.dataset.placeholder = 'Untitled';
    titleSpan.textContent = rec.title || '';
    titleSpan.onblur = () => {
      saveDatabaseTableCell(titleSpan, d).catch(showDockError);
    };

    const openBtn = document.createElement('button');
    openBtn.type = 'button';
    openBtn.className = 'record-dialog-open';
    openBtn.title = 'Open record';
    openBtn.textContent = '↗';
    openBtn.style.float = 'right';
    openBtn.onclick = () => openDatabaseRecordDialog(root, rec.id).catch(showDockError);

    nameTd.append(titleSpan, openBtn);
    tr.append(nameTd);

    for (const prop of visible) {
      const td = document.createElement('td');
      const val = databaseRecordValue(prop, rec);
      if (['text', 'person', 'files', 'place', 'relation', 'rollup', 'formula', 'button'].includes(prop.kind)) {
        td.contentEditable = 'true';
        td.dataset.pageId = String(rec.id);
        td.dataset.propId = String(prop.id);
        td.textContent = val;
        td.onblur = () => {
          saveDatabaseTableCell(td, d).catch(showDockError);
        };
      } else {
        const editor = propertyEditor(prop, val, rec.id, {
          root,
          databaseId: id,
          onUpdated: () => {
            rec.values = rec.values || {};
            if (prop.kind === 'number') {
              renderDatabaseReplace(root, id).catch(showDockError);
            }
          },
        });
        td.append(editor);
      }
      tr.append(td);
    }

    if (showCreated) {
      const createdTd = document.createElement('td');
      createdTd.className = 'muted';
      createdTd.textContent = formatGeneratedDate(rec.created_at);
      tr.append(createdTd);
    }

    const actionsTd = document.createElement('td');
    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'record-open';
    delBtn.textContent = '🗑';
    delBtn.title = 'Move to trash';
    delBtn.onclick = () => trashDatabaseRows(id, [rec.id]).catch(showDockError);
    actionsTd.append(delBtn);
    tr.append(actionsTd);

    tbody.append(tr);
  }
  table.append(tbody);

  table.onkeydown = (event) => handleDatabaseTableKey(event, table, d, id);

  applyTableColumnSizing(table, headRow, d, visible, id);
  appendTableCalculationFooter(table, d, visible);
  mountResizableDatabaseTable(host, table, d, id);

  const addRowBtn = document.createElement('button');
  addRowBtn.type = 'button';
  addRowBtn.className = 'db-add-row-btn';
  addRowBtn.textContent = '＋ New row';
  addRowBtn.onclick = async () => {
    const newId = await window.noteon.createDatabaseRecord(id);
    await refresh();
    await openCreatedDatabaseRecord(root, id, newId, viewSettings.new_record_open_mode || 'focus_name');
  };
  host.append(addRowBtn);
}

/**
 * Render gallery cards inside `host`.
 * @param {HTMLElement} host
 * @param {HTMLElement} root
 * @param {number} id
 * @param {object} d
 */
function renderDatabaseGalleryBody(host, root, id, d) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const visible = databaseVisibleProperties(d, 'gallery');
  const records = sortDatabase(d);
  const orderedIds = records.map((r) => r.id);

  const cards = document.createElement('div');
  cards.className = 'cards';
  const columns = Math.max(1, Math.min(6, Number(d.gallery_columns) || 3));
  cards.style.gridTemplateColumns = `repeat(${columns}, minmax(160px, 1fr))`;

  for (const rec of records) {
    const card = document.createElement('div');
    card.className = 'card';
    card.dataset.recordId = String(rec.id);
    card.draggable = pageWriteAccess;

    card.ondragstart = (event) => {
      if (!pageWriteAccess) return event.preventDefault();
      setDatabaseRecordDrag(event, rec.id);
    };
    card.ondragover = (event) => {
      if (!pageWriteAccess || !event.dataTransfer?.types?.includes('application/x-noteon-record')) return;
      event.preventDefault();
    };
    card.ondrop = async (event) => {
      if (!pageWriteAccess || !event.dataTransfer?.types?.includes('application/x-noteon-record')) return;
      event.preventDefault();
      const draggedIds = databaseDraggedRecordIds(event, rec.id);
      if (!draggedIds.length || draggedIds.includes(rec.id)) return;
      const rect = card.getBoundingClientRect();
      const after = event.clientX - rect.left > rect.width / 2;
      await window.noteon.reorderDatabaseRecord({ id, recordIds: draggedIds, targetId: rec.id, after });
      await renderDatabaseReplace(root, id);
    };
    card.onclick = (event) => handleDatabaseCardClick(event, root, id, rec.id);

    card.append(recordSelectionControl(rec.id, id, orderedIds, root));

    if (viewSettings.gallery_show_cover !== false) {
      if (rec.cover_image) {
        const img = document.createElement('img');
        img.className = 'gallery-cover';
        img.src = localFileUrl(rec.cover_image);
        card.append(img);
      } else if (rec.cover) {
        const div = document.createElement('div');
        div.className = `gallery-cover page-cover-color-${rec.cover}`;
        card.append(div);
      }
    }

    if (viewSettings.gallery_show_title !== false) {
      const titleEl = document.createElement('div');
      titleEl.innerHTML = `<strong class="record-title">${esc(rec.title || 'Untitled')}</strong>`;
      card.append(titleEl);
    }

    if (viewSettings.gallery_show_description !== false && rec.preview) {
      const desc = document.createElement('div');
      desc.className = 'gallery-description';
      desc.textContent = rec.preview;
      card.append(desc);
    }

    for (const prop of visible) {
      const propRow = document.createElement('div');
      propRow.className = 'database-card-property';
      propRow.innerHTML = `<span>${esc(prop.name)}:</span><span>${databasePropertyDisplayHTML(prop, rec)}</span>`;
      card.append(propRow);
    }

    if (viewSettings.gallery_show_created_at !== false) {
      const dateRow = document.createElement('div');
      dateRow.className = 'database-card-property';
      dateRow.textContent = formatGeneratedDate(rec.created_at);
      card.append(dateRow);
    }

    cards.append(card);
  }

  attachCardContainerDeleteKey(cards, id);
  mountResizableDatabaseCards(host, cards, d, id, 'gallery');
}

/**
 * Render the kanban board view inside `host`, or fallback card list if no status/select property exists.
 * @param {HTMLElement} host
 * @param {HTMLElement} root
 * @param {number} id
 * @param {object} d
 */
function renderDatabaseBoardBody(host, root, id, d) {
  const viewSettings = parseDbViewSettings(d.view_settings);
  const visible = databaseVisibleProperties(d, 'board');
  const records = sortDatabase(d);
  const orderedIds = records.map((r) => r.id);

  const groupProp =
    (d.properties || []).find((p) => p.kind === 'status') ||
    (d.properties || []).find((p) => p.kind === 'select');

  if (!groupProp) {
    const fallback = document.createElement('div');
    fallback.className = 'board-fallback';
    const notice = document.createElement('div');
    notice.className = 'board-fallback-notice';
    const text = document.createElement('span');
    text.textContent = 'Board view groups cards by a Status or Select property.';
    const addStatus = document.createElement('button');
    addStatus.type = 'button';
    addStatus.className = 'primary';
    addStatus.textContent = '＋ Add Status property';
    addStatus.onclick = async () => {
      await window.noteon.addProperty({ id, name: 'Status', kind: 'status', options: 'To do\nIn progress\nDone' });
      await renderDatabaseReplace(root, id);
    };
    notice.append(text, addStatus);
    fallback.append(notice);

    if (!records.length) {
      const empty = document.createElement('div');
      empty.className = 'board-fallback-empty';
      empty.textContent = 'No records yet. Click “＋ New” above to add a card.';
      fallback.append(empty);
    } else {
      const cardsWrap = document.createElement('div');
      cardsWrap.className = 'board-fallback-cards';
      for (const rec of records) {
        const card = document.createElement('div');
        card.className = 'card';
        card.dataset.recordId = String(rec.id);
        card.onclick = (event) => handleDatabaseCardClick(event, root, id, rec.id);
        card.append(recordSelectionControl(rec.id, id, orderedIds, root));
        const strong = document.createElement('strong');
        strong.className = 'record-title';
        strong.textContent = rec.title || 'Untitled';
        card.append(strong);
        if (viewSettings.board_show_description && rec.preview) {
          const desc = document.createElement('div');
          desc.className = 'gallery-description';
          desc.textContent = rec.preview;
          card.append(desc);
        }
        cardsWrap.append(card);
      }
      fallback.append(cardsWrap);
    }

    attachCardContainerDeleteKey(fallback, id);
    mountResizableDatabaseCards(host, fallback, d, id, 'fallback');
    return;
  }

  const options = propertyOptions(groupProp);
  const optionNames = new Set(options.map((opt) => opt.name));
  const grouped = new Map();
  const hasEmpty = records.some((rec) => !optionNames.has(String(rec.values?.[groupProp.id] || '')));
  if (hasEmpty || !options.length) grouped.set('', []);
  for (const opt of options) grouped.set(opt.name, []);

  for (const rec of records) {
    const val = String(rec.values?.[groupProp.id] || '');
    const bucket = grouped.has(val) ? val : grouped.has('') ? '' : options[0]?.name || '';
    if (!grouped.has(bucket)) grouped.set(bucket, []);
    grouped.get(bucket).push(rec);
  }

  const board = document.createElement('div');
  board.className = 'board';
  const counts = new Map();
  let draggedGroup = null;

  for (const [colValue, colRecords] of grouped.entries()) {
    counts.set(colValue, colRecords.length);
    const col = document.createElement('div');
    col.className = 'board-col';
    col.dataset.groupValue = colValue;

    const optionMeta = options.find((opt) => opt.name === colValue) || { name: colValue || 'No status' };
    const color = colValue ? propertyOptionColor(optionMeta, groupProp.kind) : '#85877f';

    const header = document.createElement('h3');
    header.draggable = Boolean(pageWriteAccess && colValue);
    header.ondragstart = (event) => {
      if (!pageWriteAccess || !colValue) return event.preventDefault();
      draggedGroup = colValue;
      col.classList.add('dragging');
      event.dataTransfer.setData('application/x-noteon-board-group', colValue);
    };
    header.ondragend = () => {
      draggedGroup = null;
      board.querySelectorAll('.board-col').forEach((node) => node.classList.remove('dragging', 'drop-before', 'drop-after'));
    };

    const chip = document.createElement('span');
    chip.className = 'board-group-chip';
    chip.style.setProperty('--option-color', color);
    chip.textContent = colValue || 'No status';

    const countBadge = document.createElement('span');
    countBadge.className = 'board-group-count';
    countBadge.textContent = String(colRecords.length);

    const colActions = document.createElement('div');
    colActions.className = 'board-col-actions';

    const addBtn = document.createElement('button');
    addBtn.type = 'button';
    addBtn.className = 'board-add-record-button';
    addBtn.title = `Add card in ${colValue || 'No status'}`;
    addBtn.textContent = '+';
    addBtn.onclick = async () => {
      const recId = await window.noteon.createDatabaseRecordWithProperty({
        id,
        propertyId: groupProp.id,
        value: colValue,
      });
      await refresh();
      await openCreatedDatabaseRecord(root, id, recId, viewSettings.new_record_open_mode || 'focus_name');
    };
    colActions.append(addBtn);

    if (colValue) {
      const removeGroupBtn = document.createElement('button');
      removeGroupBtn.type = 'button';
      removeGroupBtn.className = 'board-col-remove';
      removeGroupBtn.title = `Remove group "${colValue}"`;
      removeGroupBtn.textContent = '×';
      removeGroupBtn.onclick = async () => {
        if (!confirm(`Remove group "${colValue}"?`)) return;
        await window.noteon.removePropertyOption({ id: groupProp.id, name: colValue });
        await renderDatabaseReplace(root, id);
      };
      colActions.append(removeGroupBtn);
    }

    header.append(chip, countBadge, colActions);
    col.append(header);

    col.ondragover = (event) => {
      if (!pageWriteAccess) return;
      if (event.dataTransfer?.types?.includes('application/x-noteon-board-group') && colValue && draggedGroup !== colValue) {
        event.preventDefault();
        const rect = col.getBoundingClientRect();
        col.classList.remove('drop-before', 'drop-after');
        col.classList.add(event.clientX - rect.left < rect.width / 2 ? 'drop-before' : 'drop-after');
      } else if (event.dataTransfer?.types?.includes('application/x-noteon-record')) {
        event.preventDefault();
      }
    };

    col.ondragleave = (event) => {
      if (!col.contains(event.relatedTarget)) {
        col.classList.remove('drop-before', 'drop-after');
      }
    };

    col.ondrop = async (event) => {
      if (!pageWriteAccess) return;
      col.classList.remove('drop-before', 'drop-after');

      if (event.dataTransfer?.types?.includes('application/x-noteon-board-group') && colValue && draggedGroup && draggedGroup !== colValue) {
        event.preventDefault();
        const rect = col.getBoundingClientRect();
        const after = event.clientX - rect.left >= rect.width / 2;
        const nextOrder = options.filter((opt) => opt.name !== draggedGroup);
        const movingOpt = options.find((opt) => opt.name === draggedGroup);
        const targetIdx = nextOrder.findIndex((opt) => opt.name === colValue);
        if (movingOpt && targetIdx >= 0) {
          nextOrder.splice(targetIdx + (after ? 1 : 0), 0, movingOpt);
          await window.noteon.updateProperty({ id: groupProp.id, options: nextOrder });
          await renderDatabaseReplace(root, id);
        }
        return;
      }

      if (event.dataTransfer?.types?.includes('application/x-noteon-record')) {
        event.preventDefault();
        const draggedIds = databaseDraggedRecordIds(event, 0);
        if (!draggedIds.length) return;
        const targetCard = event.target?.closest?.('.card[data-record-id]');
        const targetId = targetCard ? Number(targetCard.dataset.recordId) : null;
        let after = false;
        if (targetCard) {
          const rect = targetCard.getBoundingClientRect();
          after = event.clientY - rect.top > rect.height / 2;
        }
        for (const recordId of draggedIds) {
          await window.noteon.moveDatabaseRecord({
            id,
            recordId,
            propertyId: groupProp.id,
            value: colValue,
            targetId: targetId && targetId !== recordId ? targetId : null,
            after,
          });
        }
        await renderDatabaseReplace(root, id);
      }
    };

    for (const rec of colRecords) {
      const card = document.createElement('div');
      card.className = 'card';
      card.dataset.recordId = String(rec.id);
      card.draggable = pageWriteAccess;
      card.ondragstart = (event) => {
        if (!pageWriteAccess) return event.preventDefault();
        setDatabaseRecordDrag(event, rec.id);
      };
      card.onclick = (event) => handleDatabaseCardClick(event, root, id, rec.id);

      card.append(recordSelectionControl(rec.id, id, orderedIds, root));

      const title = document.createElement('strong');
      title.className = 'record-title';
      title.textContent = rec.title || 'Untitled';
      card.append(title);

      if (viewSettings.board_show_description && rec.preview) {
        const desc = document.createElement('div');
        desc.className = 'gallery-description';
        desc.textContent = rec.preview;
        card.append(desc);
      }

      for (const prop of visible) {
        if (prop.id === groupProp.id) continue;
        const propEl = document.createElement('div');
        propEl.className = 'database-card-property';
        propEl.innerHTML = `<span>${esc(prop.name)}:</span><span>${databasePropertyDisplayHTML(prop, rec)}</span>`;
        card.append(propEl);
      }

      col.append(card);
    }

    board.append(col);
  }

  const addGroupCol = document.createElement('div');
  addGroupCol.className = 'board-col board-add-group';
  const addGroupBtn = document.createElement('button');
  addGroupBtn.type = 'button';
  addGroupBtn.className = 'board-add-group-button';
  addGroupBtn.textContent = '+ Add group';

  const addGroupForm = document.createElement('form');
  addGroupForm.className = 'board-add-group-form';
  addGroupForm.hidden = true;
  const addGroupInput = document.createElement('input');
  addGroupInput.type = 'text';
  addGroupInput.maxLength = 80;
  addGroupInput.placeholder = 'Group name…';
  const addGroupSubmit = document.createElement('button');
  addGroupSubmit.type = 'submit';
  addGroupSubmit.className = 'primary';
  addGroupSubmit.textContent = 'Add';
  addGroupForm.append(addGroupInput, addGroupSubmit);

  addGroupBtn.onclick = () => {
    addGroupBtn.hidden = true;
    addGroupForm.hidden = false;
    addGroupInput.focus();
  };
  addGroupForm.onsubmit = async (event) => {
    event.preventDefault();
    const name = addGroupInput.value.trim();
    if (!name) return;
    if (options.some((opt) => opt.name.toLocaleLowerCase() === name.toLocaleLowerCase())) {
      addGroupInput.select();
      return;
    }
    const next = [...options, { name, color: null }];
    await window.noteon.updateProperty({ id: groupProp.id, options: next });
    await renderDatabaseReplace(root, id);
  };

  addGroupCol.append(addGroupBtn, addGroupForm);
  board.append(addGroupCol);

  attachCardContainerDeleteKey(board, id);
  mountResizableDatabaseCards(host, board, d, id, 'board', counts);
}

/**
 * Render the body section of a database view according to its `view_mode` (`table`, `gallery`, or `board`).
 * @param {HTMLElement} root
 * @param {number} id
 * @param {object} d
 */
function renderDbBody(root, id, d) {
  const host = root.querySelector('.db-body') || document.createElement('div');
  host.className = 'db-body';
  host.innerHTML = '';
  if (!host.parentElement) root.append(host);

  const view = d.view_mode || 'table';
  if (view === 'gallery') {
    renderDatabaseGalleryBody(host, root, id, d);
  } else if (view === 'board') {
    renderDatabaseBoardBody(host, root, id, d);
  } else {
    renderDatabaseTableBody(host, root, id, d);
  }
}

/**
 * Fetch and render a complete database view (toolbar, selection controls, sorting, and body) inside `root`.
 * @param {HTMLElement} root
 * @param {number} id
 */
async function renderDatabase(root, id) {
  id = Number(id);
  const d = await window.noteon.getDatabase(id);
  if (!d) {
    root.innerHTML = '<div class="muted">Database not found.</div>';
    return;
  }

  if (selectedDatabaseId !== id) {
    selectedRecordIds.clear();
    selectedDatabaseId = id;
    recordSelectionAnchorId = null;
  }

  const viewSettings = parseDbViewSettings(d.view_settings);
  const currentView = d.view_mode || 'table';
  root.innerHTML = '';

  const toolbar = document.createElement('div');
  toolbar.className = 'db-toolbar';

  for (const [view, label] of [
    ['table', 'Table'],
    ['gallery', 'Gallery'],
    ['board', 'Board'],
  ]) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.view = view;
    btn.textContent = label;
    if (currentView === view) btn.className = 'primary';
    btn.onclick = async () => {
      await window.noteon.setView({ id, view });
      await renderDatabaseReplace(root, id);
    };
    toolbar.append(btn);
  }

  const addRecordBtn = document.createElement('button');
  addRecordBtn.type = 'button';
  addRecordBtn.id = 'add-record';
  addRecordBtn.textContent = '＋ New';
  addRecordBtn.onclick = async () => {
    const recId = await window.noteon.createDatabaseRecord(id);
    await refresh();
    await openCreatedDatabaseRecord(root, id, recId, viewSettings.new_record_open_mode || 'focus_name');
  };

  const addPropBtn = document.createElement('button');
  addPropBtn.type = 'button';
  addPropBtn.id = 'add-prop';
  addPropBtn.textContent = '＋ Property';
  addPropBtn.onclick = async () => {
    const result = await openAddPropertyDialog();
    if (!result) return;
    await window.noteon.addProperty({ id, ...result });
    await renderDatabaseReplace(root, id);
  };

  const viewOptBtn = document.createElement('button');
  viewOptBtn.type = 'button';
  viewOptBtn.id = 'view-options';
  viewOptBtn.textContent = 'View options';
  viewOptBtn.onclick = () => openDatabaseViewOptions(root, id, d);

  toolbar.append(addRecordBtn, addPropBtn, viewOptBtn);

  if (selectedRecordIds.size && selectedDatabaseId === id) {
    const countSpan = document.createElement('span');
    countSpan.className = 'db-selection-count';
    countSpan.textContent = `${selectedRecordIds.size} selected`;
    toolbar.append(countSpan);

    const makeBulkBtn = (label, handler) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.textContent = label;
      btn.onclick = () => handler().catch(showDockError);
      return btn;
    };

    toolbar.append(
      makeBulkBtn('Copy', () => copySelectedDatabasePages(false)),
      makeBulkBtn('Cut', () => copySelectedDatabasePages(true)),
      makeBulkBtn('Duplicate', () => duplicateSelectedDatabaseRows(id)),
      makeBulkBtn('Trash', () => trashDatabaseRows(id, [...selectedRecordIds])),
    );

    const editableProps = (d.properties || []).filter((p) => !['created_time', 'last_edited_time', 'id'].includes(p.kind));
    if (editableProps.length) {
      const bulkSelect = document.createElement('select');
      const def = document.createElement('option');
      def.value = '';
      def.textContent = 'Bulk edit…';
      bulkSelect.append(def);
      for (const prop of editableProps) {
        const opt = document.createElement('option');
        opt.value = String(prop.id);
        opt.textContent = prop.name;
        bulkSelect.append(opt);
      }
      bulkSelect.onchange = async () => {
        const propId = Number(bulkSelect.value);
        bulkSelect.value = '';
        if (!propId) return;
        const prop = editableProps.find((p) => p.id === propId);
        if (!prop) return;
        const val = await openBulkEditDialog(prop, selectedRecordIds.size);
        if (val === undefined || val === null) return;
        await window.noteon.bulkSetDatabaseValue({
          id,
          recordIds: [...selectedRecordIds],
          propertyId: prop.id,
          value: val,
        });
        await renderDatabaseReplace(root, id);
      };
      toolbar.append(bulkSelect);
    }

    toolbar.append(
      makeBulkBtn('Clear', async () => {
        selectedRecordIds.clear();
        recordSelectionAnchorId = null;
        await renderDatabaseReplace(root, id);
      }),
    );
  }

  const sortSelect = document.createElement('select');
  sortSelect.id = 'sort';
  sortSelect.title = 'Sort database rows';
  for (const [val, label] of [
    ['manual', 'Sort: Manual'],
    ['created_at', 'Sort: Created'],
    ['title', 'Sort: Name'],
    ['priority', 'Sort: Priority'],
    ...(d.properties || []).map((p) => [String(p.id), `Sort: ${p.name}`]),
  ]) {
    const opt = document.createElement('option');
    opt.value = val;
    opt.textContent = label;
    sortSelect.append(opt);
  }
  sortSelect.value = String(d.sort_key || 'created_at');
  sortSelect.onchange = async () => {
    await window.noteon.setDatabaseSort({
      id,
      key: sortSelect.value,
      descending: Boolean(d.sort_descending),
    });
    await renderDatabaseReplace(root, id);
  };

  const dirBtn = document.createElement('button');
  dirBtn.type = 'button';
  dirBtn.id = 'sort-direction';
  dirBtn.textContent = d.sort_descending ? '↓' : '↑';
  dirBtn.title = 'Toggle sort direction';
  dirBtn.onclick = async () => {
    await window.noteon.setDatabaseSort({
      id,
      key: sortSelect.value,
      descending: !d.sort_descending,
    });
    await renderDatabaseReplace(root, id);
  };

  toolbar.append(sortSelect, dirBtn);

  if (currentView === 'gallery') {
    const colSelect = document.createElement('select');
    colSelect.id = 'gallery-columns';
    colSelect.title = 'Gallery columns';
    for (let n = 1; n <= 6; n++) {
      const opt = document.createElement('option');
      opt.value = String(n);
      opt.textContent = `${n} col${n > 1 ? 's' : ''}`;
      colSelect.append(opt);
    }
    colSelect.value = String(d.gallery_columns || 3);
    colSelect.onchange = async () => {
      await window.noteon.setGalleryColumns({ id, columns: Number(colSelect.value) });
      await renderDatabaseReplace(root, id);
    };
    toolbar.append(colSelect);
  }

  root.append(toolbar);
  renderDbBody(root, id, d);
}
