/**
 * Modal dialogs: icon picker, property management, bulk editor, media preview, page history, record modal, and context menus.
 */

const ICON_PRESETS = [
  '📄', '📝', '📌', '⭐', '✅', '📋', '📊', '📁',
  '💡', '🔥', '🚀', '🎯', '📚', '🔖', '💬', '🧩',
  '🛠️', '🧠', '🏠', '🎉', '🎨', '🗓️', '🔗', '⚙️',
];

function showModalDialog(title, buildBody) {
  return new Promise((resolve) => {
    const dialog = document.createElement('dialog');
    dialog.className = 'modal-dialog';
    dialog.innerHTML = `<header><h2>${esc(title)}</h2><button class="icon" type="button" aria-label="Close">×</button></header><div class="modal-body"></div>`;

    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      if (dialog.open) dialog.close();
      resolve(value);
    };

    dialog.querySelector('.icon').onclick = () => finish(null);
    dialog.addEventListener('click', (event) => {
      if (event.target === dialog) finish(null);
    });
    dialog.addEventListener('cancel', (event) => {
      event.preventDefault();
      finish(null);
    });
    dialog.addEventListener(
      'close',
      () => {
        if (!settled) {
          settled = true;
          resolve(null);
        }
        dialog.remove();
      },
      { once: true },
    );

    buildBody(dialog.querySelector('.modal-body'), finish);
    document.body.append(dialog);
    dialog.showModal();
  });
}

function openIconPickerDialog(currentIcon = '📄') {
  return showModalDialog('Choose page icon', (body, finish) => {
    const form = document.createElement('form');
    form.className = 'modal-form';

    const grid = document.createElement('div');
    grid.className = 'icon-picker-grid';
    for (const icon of ICON_PRESETS) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = icon;
      button.onclick = () => finish(icon);
      grid.append(button);
    }

    const label = document.createElement('label');
    label.textContent = 'Or type an emoji / symbol';
    const input = document.createElement('input');
    input.type = 'text';
    input.maxLength = 16;
    input.value = currentIcon || '📄';
    label.append(input);

    const actions = document.createElement('div');
    actions.className = 'modal-actions';
    const reset = document.createElement('button');
    reset.type = 'button';
    reset.textContent = 'Default';
    reset.onclick = () => finish('📄');

    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    cancel.onclick = () => finish(null);

    const save = document.createElement('button');
    save.type = 'submit';
    save.className = 'primary';
    save.textContent = 'Save';

    actions.append(reset, cancel, save);
    form.append(grid, label, actions);
    form.onsubmit = (event) => {
      event.preventDefault();
      finish(input.value.trim() || '📄');
    };

    body.append(form);
    setTimeout(() => {
      input.focus();
      input.select();
    }, 0);
  });
}

function openAddPropertyDialog(title = 'Add property') {
  return showModalDialog(title, (body, finish) => {
    const form = document.createElement('form');
    form.className = 'modal-form';

    const nameLabel = document.createElement('label');
    nameLabel.textContent = 'Property name';
    const nameInput = document.createElement('input');
    nameInput.type = 'text';
    nameInput.maxLength = 80;
    nameInput.required = true;
    nameInput.placeholder = 'e.g. Status, Priority, Due date';
    nameLabel.append(nameInput);

    const kindLabel = document.createElement('label');
    kindLabel.textContent = 'Property type';
    const kindSelect = document.createElement('select');
    for (const [kind, label] of databaseKinds) {
      const option = document.createElement('option');
      option.value = kind;
      option.textContent = label;
      kindSelect.append(option);
    }
    kindLabel.append(kindSelect);

    const optionsLabel = document.createElement('label');
    optionsLabel.textContent = 'Options (one per line)';
    const optionsInput = document.createElement('textarea');
    optionsInput.rows = 4;
    optionsInput.placeholder = 'To do\nIn progress\nDone';
    optionsLabel.append(optionsInput);

    const syncOptionsVisibility = () => {
      const hasOptions = ['select', 'multi-select', 'status'].includes(kindSelect.value);
      optionsLabel.hidden = !hasOptions;
      if (kindSelect.value === 'status' && !optionsInput.value.trim()) {
        optionsInput.value = 'To do\nIn progress\nDone';
      }
    };
    kindSelect.onchange = syncOptionsVisibility;
    syncOptionsVisibility();

    const actions = document.createElement('div');
    actions.className = 'modal-actions';
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancel';
    cancel.onclick = () => finish(null);

    const submit = document.createElement('button');
    submit.type = 'submit';
    submit.className = 'primary';
    submit.textContent = 'Add property';

    actions.append(cancel, submit);
    form.append(nameLabel, kindLabel, optionsLabel, actions);

    form.onsubmit = (event) => {
      event.preventDefault();
      const name = nameInput.value.trim();
      if (!name) {
        nameInput.focus();
        return;
      }
      const hasOptions = ['select', 'multi-select', 'status'].includes(kindSelect.value);
      const options = hasOptions
        ? optionsInput.value
            .split(/\r?\n/)
            .map((item) => item.trim())
            .filter(Boolean)
            .map((item) => ({ name: item, color: null }))
        : [];
      finish({ name, kind: kindSelect.value, options });
    };

    body.append(form);
    setTimeout(() => nameInput.focus(), 0);
  });
}

function openManagePropertyDialog(property, handlers) {
  return showModalDialog(`Manage property: ${property.name}`, (body, finish) => {
    const container = document.createElement('div');
    container.className = 'modal-form';
    let mutated = false;
    const run = async (fn) => {
      await fn();
      mutated = true;
      finish(true);
    };

    const renameForm = document.createElement('form');
    renameForm.className = 'modal-form';
    const renameLabel = document.createElement('label');
    renameLabel.textContent = 'Property name';
    const renameRow = document.createElement('div');
    renameRow.style.display = 'flex';
    renameRow.style.gap = '8px';
    const renameInput = document.createElement('input');
    renameInput.type = 'text';
    renameInput.maxLength = 80;
    renameInput.value = property.name;
    const renameBtn = document.createElement('button');
    renameBtn.type = 'submit';
    renameBtn.className = 'primary';
    renameBtn.textContent = 'Rename';
    renameRow.append(renameInput, renameBtn);
    renameLabel.append(renameRow);
    renameForm.append(renameLabel);
    renameForm.onsubmit = (event) => {
      event.preventDefault();
      const name = renameInput.value.trim();
      if (name && name !== property.name) {
        run(() => handlers.onRename(name)).catch(showDockError);
      }
    };

    const kindLabel = document.createElement('label');
    kindLabel.textContent = 'Property type';
    const kindSelect = document.createElement('select');
    for (const [kind, label] of databaseKinds) {
      const option = document.createElement('option');
      option.value = kind;
      option.textContent = label;
      kindSelect.append(option);
    }
    kindSelect.value = property.kind;
    kindSelect.onchange = () => {
      if (kindSelect.value !== property.kind) {
        run(() => handlers.onChangeKind(kindSelect.value)).catch(showDockError);
      }
    };
    kindLabel.append(kindSelect);
    container.append(renameForm, kindLabel);

    const existingOptions = propertyOptions(property);
    if (['select', 'multi-select', 'status'].includes(property.kind)) {
      const optForm = document.createElement('form');
      optForm.className = 'modal-form';
      const optLabel = document.createElement('label');
      optLabel.textContent = 'Options (one per line)';
      const optArea = document.createElement('textarea');
      optArea.rows = 4;
      optArea.value = existingOptions.map((item) => item.name).join('\n');
      const optSave = document.createElement('button');
      optSave.type = 'submit';
      optSave.textContent = 'Save options';
      optLabel.append(optArea);
      optForm.append(optLabel, optSave);
      optForm.onsubmit = (event) => {
        event.preventDefault();
        const serialized = normalizePropertyOptions(
          optArea.value.split(/\r?\n/).map((name) => ({ name })),
          existingOptions,
        );
        run(() => handlers.onUpdateOptions(serialized)).catch(showDockError);
      };
      container.append(optForm);

      if (existingOptions.length) {
        const renOptForm = document.createElement('form');
        renOptForm.className = 'modal-form';
        const renLabel = document.createElement('label');
        renLabel.textContent = 'Rename an existing option';
        const renRow = document.createElement('div');
        renRow.style.display = 'flex';
        renRow.style.gap = '6px';
        const optSel = document.createElement('select');
        for (const item of existingOptions) {
          const opt = document.createElement('option');
          opt.value = item.name;
          opt.textContent = item.name;
          optSel.append(opt);
        }
        const newOptInput = document.createElement('input');
        newOptInput.type = 'text';
        newOptInput.maxLength = 80;
        newOptInput.placeholder = 'New option name';
        const renOptBtn = document.createElement('button');
        renOptBtn.type = 'submit';
        renOptBtn.textContent = 'Rename';
        renRow.append(optSel, newOptInput, renOptBtn);
        renLabel.append(renRow);
        renOptForm.append(renLabel);
        renOptForm.onsubmit = (event) => {
          event.preventDefault();
          const next = newOptInput.value.trim();
          if (next) run(() => handlers.onRenameOption(optSel.value, next)).catch(showDockError);
        };

        const remRow = document.createElement('div');
        remRow.style.display = 'flex';
        remRow.style.gap = '6px';
        remRow.style.alignItems = 'center';
        const remSel = document.createElement('select');
        for (const item of existingOptions) {
          const opt = document.createElement('option');
          opt.value = item.name;
          opt.textContent = item.name;
          remSel.append(opt);
        }
        const remBtn = document.createElement('button');
        remBtn.type = 'button';
        remBtn.textContent = 'Remove option';
        remBtn.onclick = () => {
          const target = remSel.value;
          if (target && confirm(`Remove “${target}” and clear matching values?`)) {
            run(() => handlers.onRemoveOption(target)).catch(showDockError);
          }
        };
        remRow.append(remSel, remBtn);
        container.append(renOptForm, remRow);
      }
    }

    const footer = document.createElement('div');
    footer.className = 'modal-actions';
    footer.style.justifyContent = 'space-between';

    const moveGroup = document.createElement('div');
    moveGroup.style.display = 'flex';
    moveGroup.style.gap = '6px';
    const movePrev = document.createElement('button');
    movePrev.type = 'button';
    movePrev.textContent = handlers.isCustom ? '↑ Move up' : '← Move left';
    movePrev.onclick = () => run(() => handlers.onMove(-1)).catch(showDockError);

    const moveNext = document.createElement('button');
    moveNext.type = 'button';
    moveNext.textContent = handlers.isCustom ? '↓ Move down' : 'Move right →';
    moveNext.onclick = () => run(() => handlers.onMove(1)).catch(showDockError);
    moveGroup.append(movePrev, moveNext);

    const rightGroup = document.createElement('div');
    rightGroup.style.display = 'flex';
    rightGroup.style.gap = '6px';
    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'restore-page danger';
    delBtn.textContent = 'Delete';
    delBtn.onclick = () => {
      if (confirm(`Delete “${property.name}” and all its values?`)) {
        run(() => handlers.onDelete()).catch(showDockError);
      }
    };

    const closeBtn = document.createElement('button');
    closeBtn.type = 'button';
    closeBtn.textContent = 'Close';
    closeBtn.onclick = () => finish(mutated);

    rightGroup.append(delBtn, closeBtn);
    footer.append(moveGroup, rightGroup);
    container.append(footer);
    body.append(container);

    setTimeout(() => {
      renameInput.focus();
      renameInput.select();
    }, 0);
  });
}

function openBulkEditDialog(property, count) {
  return showModalDialog(`Set “${property.name}” for ${count} selected`, (body, finish) => {
    const form = document.createElement('form');
    form.className = 'modal-form';
    const options = propertyOptions(property);
    let getValue = () => '';
    let focusEl = null;

    if (['select', 'status'].includes(property.kind)) {
      const label = document.createElement('label');
      label.textContent = 'Value';
      const select = document.createElement('select');
      const clearOpt = document.createElement('option');
      clearOpt.value = '';
      clearOpt.textContent = '(Clear value)';
      select.append(clearOpt);
      for (const item of options) {
        const opt = document.createElement('option');
        opt.value = item.name;
        opt.textContent = item.name;
        select.append(opt);
      }
      label.append(select);
      form.append(label);
      focusEl = select;
      getValue = () => select.value;
    } else if (property.kind === 'multi-select') {
      const list = document.createElement('div');
      list.style.display = 'grid';
      list.style.gap = '4px';
      const boxes = options.map((item) => {
        const row = document.createElement('label');
        row.style.display = 'flex';
        row.style.alignItems = 'center';
        row.style.gap = '8px';
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.style.width = 'auto';
        cb.value = item.name;
        row.append(cb, document.createTextNode(item.name));
        list.append(row);
        return cb;
      });
      const extraLabel = document.createElement('label');
      extraLabel.textContent = 'Additional comma-separated options (optional)';
      const extraInput = document.createElement('input');
      extraInput.type = 'text';
      extraInput.placeholder = 'tag1, tag2';
      extraLabel.append(extraInput);
      form.append(list, extraLabel);
      focusEl = extraInput;
      getValue = () => {
        const chosen = boxes.filter((cb) => cb.checked).map((cb) => cb.value);
        const extra = extraInput.value
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean);
        return [...new Set([...chosen, ...extra])].join('|');
      };
    } else if (property.kind === 'checkbox') {
      const label = document.createElement('label');
      label.textContent = 'Checked state';
      const select = document.createElement('select');
      select.innerHTML = '<option value="true">Checked (true)</option><option value="false">Unchecked (false)</option>';
      label.append(select);
      form.append(label);
      focusEl = select;
      getValue = () => select.value;
    } else {
      const label = document.createElement('label');
      label.textContent = 'Value (leave blank to clear)';
      const input = document.createElement('input');
      input.type = property.kind === 'date' ? 'date' : ['url', 'email', 'phone'].includes(property.kind) ? property.kind : 'text';
      if (property.kind === 'number') input.inputMode = 'decimal';
      input.placeholder = propertyValuePlaceholder(property.kind, '');
      label.append(input);
      form.append(label);
      focusEl = input;
      getValue = () => {
        let value = input.value;
        if (property.kind === 'number' && value.trim()) {
          const evaluated = evaluateNumberExpression(value);
          if (evaluated === null) {
            showDockError(new Error('Enter a valid number expression.'));
            input.focus();
            return undefined;
          }
          value = evaluated;
        }
        return value;
      };
    }

    const actions = document.createElement('div');
    actions.className = 'modal-actions';
    const clearBtn = document.createElement('button');
    clearBtn.type = 'button';
    clearBtn.textContent = 'Clear value';
    clearBtn.onclick = () => finish('');

    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.onclick = () => finish(null);

    const applyBtn = document.createElement('button');
    applyBtn.type = 'submit';
    applyBtn.className = 'primary';
    applyBtn.textContent = 'Apply';

    actions.append(clearBtn, cancelBtn, applyBtn);
    form.append(actions);
    form.onsubmit = (event) => {
      event.preventDefault();
      const value = getValue();
      if (value !== undefined) finish(value);
    };

    body.append(form);
    setTimeout(() => focusEl?.focus(), 0);
  });
}

function openMediaPreview(source) {
  const dialog = document.createElement('dialog');
  dialog.className = 'media-preview';
  dialog.innerHTML = '<button class="media-preview-close" aria-label="Close image preview">✕ Close</button><img alt="Full-size image preview">';
  dialog.querySelector('img').src = source;
  dialog.querySelector('button').onclick = () => dialog.close();
  dialog.onclick = (event) => {
    if (event.target === dialog) dialog.close();
  };
  dialog.addEventListener('close', () => dialog.remove(), { once: true });
  document.body.append(dialog);
  dialog.showModal();
}

async function showPageHistory(pageId) {
  const history = await window.noteon.getPageHistory(pageId);
  const dialog = document.createElement('dialog');
  dialog.className = 'page-history-dialog';
  dialog.innerHTML =
    '<header><h2>Page history</h2><button class="icon" type="button" aria-label="Close">×</button></header><div class="page-history-list"></div>';

  const list = dialog.querySelector('.page-history-list');
  dialog.querySelector('.icon').onclick = () => dialog.close();

  if (!history.undo.length) {
    const empty = document.createElement('p');
    empty.className = 'muted';
    empty.textContent = 'No saved changes yet.';
    list.append(empty);
  }

  for (const entry of history.undo) {
    const row = document.createElement('div');
    row.className = 'page-history-row';
    const text = document.createElement('div');
    text.innerHTML = `<strong>${esc(entry.description)}</strong><div class="muted">${new Date(entry.timestamp).toLocaleString()}</div>`;
    const restore = document.createElement('button');
    restore.className = 'primary';
    restore.textContent = 'Restore';
    restore.onclick = async () => {
      if (pageId === activeId) {
        await flushLocalEdits();
      }
      if (await window.noteon.restorePageHistory({ pageId, entryId: entry.id })) {
        dialog.close();
        await openPage(pageId, { skipTab: true });
      }
    };
    row.append(text, restore);
    list.append(row);
  }

  dialog.addEventListener('close', () => dialog.remove(), { once: true });
  document.body.append(dialog);
  dialog.showModal();
}

async function openDatabaseRecordDialog(_root, recordId) {
  const previousId = activeId;
  const content = $('#content');
  const contentParent = content.parentElement;
  const dialog = document.createElement('dialog');
  dialog.className = 'database-record-dialog';
  dialog.innerHTML =
    '<header><span>Database record</span><div><button type="button" data-open-full>↗ Open full page</button><button type="button" data-close aria-label="Close record dialog">×</button></div></header>';
  dialog._restoreContent = () => {
    if (content.parentElement !== contentParent) contentParent.append(content);
  };

  const closeButton = dialog.querySelector('[data-close]');
  let isClosing = false;
  const close = async () => {
    if (isClosing) return;
    isClosing = true;
    closeButton.disabled = true;
    try {
      await flushLocalEdits();
      dialog.close();
      dialog._restoreContent();
      dialog.remove();
      if (previousId !== null && previousId !== undefined) {
        await openPageNow(previousId, { skipTab: true, suppressHistory: true });
      }
    } catch (error) {
      isClosing = false;
      closeButton.disabled = false;
      showDockError(error);
    }
  };

  closeButton.onclick = () => close();
  dialog.querySelector('[data-open-full]').onclick = async () => {
    await close();
    await openPage(recordId);
  };
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    close();
  });

  document.body.append(dialog);
  dialog.append(content);
  dialog.showModal();

  try {
    await openPageNow(recordId, { skipTab: true, suppressHistory: true, inRecordDialog: true });
    document.querySelector('.page-title-input')?.focus();
  } catch (error) {
    showDockError(error);
    await close();
  }
}

function openFloatingContextMenu(event, actions, extraClass = '') {
  event.preventDefault();
  document.querySelectorAll('.theme-context-menu,.noteon-context-menu').forEach((menu) => menu.remove());

  const menu = document.createElement('div');
  menu.className = `theme-context-menu${extraClass ? ` ${extraClass}` : ''}`;

  const close = () => {
    menu.remove();
    document.removeEventListener('pointerdown', dismiss, true);
    document.removeEventListener('keydown', dismissKey, true);
  };
  const dismiss = (e) => {
    if (!menu.contains(e.target)) close();
  };
  const dismissKey = (e) => {
    if (e.key === 'Escape') close();
  };

  for (const [label, action] of actions) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    button.onclick = () => {
      close();
      Promise.resolve(action()).catch(showDockError);
    };
    menu.append(button);
  }

  document.body.append(menu);
  menu.style.left = `${Math.max(8, Math.min(event.clientX, innerWidth - menu.offsetWidth - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(event.clientY, innerHeight - menu.offsetHeight - 8))}px`;
  setTimeout(() => {
    document.addEventListener('pointerdown', dismiss, true);
    document.addEventListener('keydown', dismissKey, true);
  }, 0);
}
