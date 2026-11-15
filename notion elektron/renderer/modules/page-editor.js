/**
 * Page lifecycle, lease ownership, header/cover editing, custom properties, and undo/redo stepping.
 */

async function releaseActivePageLease() {
  const target = pageLeaseKey ?? activeId;
  if (target !== null && target !== undefined) {
    await window.noteon.releasePageLease(target);
  }
  pageLeaseKey = null;
}

async function resetActivePageDisplay(heading = 'Choose a page', detail = 'Pick a page from the sidebar or create a new one.') {
  await releaseActivePageLease();
  activeId = null;
  activePage = null;
  pageWriteAccess = false;
  $('#content').classList.remove('full-width');
  renderBreadcrumbs(null);
  renderEmptyWorkspace(heading, detail);
}

function openPage(id, options = {}) {
  if (!embeddedPane && dockLayout.focused !== 'primary') {
    window.noteon.navigateDockPane({
      paneId: dockLayout.focused,
      pageId: Number(id),
      newTab: options.newTab === true,
      historyTarget: options.historyTarget,
    });
    if (Number.isInteger(options.historyTarget)) {
      historyIndex = options.historyTarget;
      updateHistoryControls();
    } else {
      recordPageHistory(Number(id));
    }
    return Promise.resolve();
  }
  const request = navigationQueue.then(() => openPageNow(id, options));
  navigationQueue = request.catch(() => {});
  return request;
}

async function openPageNow(id, options = {}) {
  await flushLocalEdits();
  if (!options.inRecordDialog) {
    const stray = document.querySelector('.database-record-dialog');
    if (stray) {
      stray._restoreContent?.();
      stray.close();
      stray.remove();
    }
  }
  if (!options.skipTab) {
    await activateTab(id, options.newTab === true);
  }

  const targetId = Number(id);
  if (targetId !== activeId) {
    selectedRecordIds.clear();
    selectedDatabaseId = null;
  }
  if (selectedBlockPageId !== null && selectedBlockPageId !== targetId) {
    selectedBlockIds.clear();
    selectedBlockPageId = null;
  }

  activePage = await window.noteon.getPage(targetId);
  if (!activePage) {
    await releaseActivePageLease();
    activeId = null;
    pageWriteAccess = false;
    $('#content').classList.remove('full-width');
    renderBreadcrumbs(null);
    return;
  }

  if (pageLeaseKey !== activePage.leasePageId) {
    if (pageLeaseKey !== null) await window.noteon.releasePageLease(pageLeaseKey);
    pageLeaseKey = activePage.leasePageId;
  }
  activeId = targetId;
  pageWriteAccess = await window.noteon.acquirePageLease(activeId);

  if (Number.isInteger(options.historyTarget)) {
    historyIndex = options.historyTarget;
    updateHistoryControls();
  } else if (!options.suppressHistory) {
    recordPageHistory(targetId);
  }

  revealPageInTree(targetId);
  drawTree();
  renderBreadcrumbs(targetId);

  if (!embeddedPane && !options.inRecordDialog && activePage.trashed_by === null) {
    settings.last_page = String(targetId);
    window.noteon.setSetting({ key: 'last_page', value: targetId }).catch(() => {});
  }

  const content = $('#content');
  content.classList.toggle('full-width', Boolean(activePage.full_width));
  content.innerHTML = '';

  if (!pageWriteAccess) {
    const notice = document.createElement('div');
    notice.className = 'page-lock-notice';
    notice.textContent = 'Read only: this page is being edited in another panel. Editing will unlock when that panel leaves the page.';
    content.append(notice);
  }

  const head = document.createElement('div');
  head.className = 'page-head';
  head.innerHTML = `
    ${activePage.cover_image
      ? `<img class="page-cover" style="object-position:${Math.round((activePage.cover_x ?? 0.5) * 100)}% ${Math.round((activePage.cover_y ?? 0.5) * 100)}%" src="${esc(localFileUrl(activePage.cover_image))}">`
      : activePage.cover
        ? `<div class="page-cover page-cover-color-${activePage.cover}"></div>`
        : ''}
    <div class="page-icon" title="Edit icon">${esc(activePage.icon || '📄')}</div>
    <input class="page-title-input" value="${esc(activePage.title)}" placeholder="Untitled">
    <div class="page-meta">${new Date(activePage.created_at * 1000).toLocaleDateString()} · ${activePage.full_width ? 'Full width' : 'Page'}
      <select id="priority" title="Page priority">
        <option value="0" ${activePage.priority === 0 ? 'selected' : ''}>Priority: None</option>
        <option value="1" ${activePage.priority === 1 ? 'selected' : ''}>Low</option>
        <option value="2" ${activePage.priority === 2 ? 'selected' : ''}>Medium</option>
        <option value="3" ${activePage.priority === 3 ? 'selected' : ''}>High</option>
      </select>
      <button id="wide">${activePage.full_width ? 'Narrow' : 'Full width'}</button>
      <details class="cover-menu">
        <summary>Cover</summary>
        <div class="cover-menu-body">
          <button id="cover">Choose image</button>
          <div class="cover-options">${[[1, 'Sage'], [2, 'Sand'], [3, 'Sky'], [4, 'Rose']]
            .map(([value, label]) => `<button data-cover-style="${value}" title="${label}" aria-label="${label} cover"></button>`)
            .join('')}</div>
          <div class="cover-options">${[
            ['cover_x', 0, 'Left'],
            ['cover_x', 0.5, 'Center'],
            ['cover_x', 1, 'Right'],
            ['cover_y', 0, 'Top'],
            ['cover_y', 0.5, 'Middle'],
            ['cover_y', 1, 'Bottom'],
          ]
            .map(([axis, value, label]) => `<button data-cover-axis="${axis}" data-cover-value="${value}">${label}</button>`)
            .join('')}</div>
          <button data-cover-reset>Reset position</button>
          <button data-cover-remove>Remove cover</button>
        </div>
      </details>
      <button id="toggle-favorite" class="favorite-toggle${activePage.favorite ? ' active' : ''}" type="button" title="Toggle favorite">${activePage.favorite ? '★ Favorited' : '☆ Favorite'}</button>
      <button id="page-undo" title="Undo (Ctrl+Z)" ${activePage.history?.canUndo ? '' : 'disabled'}>↶</button>
      <button id="page-redo" title="Redo (Ctrl+Shift+Z)" ${activePage.history?.canRedo ? '' : 'disabled'}>↷</button>
      <button id="page-history">History</button>
      <button id="trash">Move to trash</button>
    </div>`;

  content.append(head);
  renderPageProperties(head, targetId);

  if (activePage.trashed_by === activePage.id) {
    const restoreBtn = document.createElement('button');
    restoreBtn.className = 'restore-page';
    restoreBtn.textContent = 'Restore page';
    restoreBtn.onclick = async () => {
      await window.noteon.restorePage(targetId);
      await refresh();
      treeMode = 'workspace';
      drawTree();
      openPage(targetId);
    };

    const purgeBtn = document.createElement('button');
    purgeBtn.className = 'restore-page danger';
    purgeBtn.textContent = 'Delete permanently';
    purgeBtn.onclick = async () => {
      if (!confirm('Permanently delete this page and its contents? This cannot be undone.')) return;
      await window.noteon.deletePagePermanently(targetId);
      closeTrashedOrDeletedTabs([targetId]);
      await refresh();
      treeMode = 'trash';
      drawTree();
      if (openTabs.length) {
        await releaseActivePageLease();
        activeId = null;
        activePage = null;
        pageWriteAccess = false;
        $('#content').classList.remove('full-width');
        renderBreadcrumbs(null);
        await openPage(openTabs[activeTabIndex], { skipTab: true });
      } else {
        await resetActivePageDisplay('Page deleted', '');
      }
    };

    head.querySelector('.page-meta').append(restoreBtn, purgeBtn);
  }

  const titleInput = head.querySelector('.page-title-input');
  titleInput.oninput = (event) => {
    activePage.title = event.target.value;
    const listed = pages.find((p) => p.id === targetId);
    if (listed) listed.title = event.target.value;
    pendingPageSave = true;
    debounce(() => savePage());
    renderBreadcrumbs(targetId);
    drawTree(searchResults);
    renderTabs();
  };
  titleInput.onblur = () => {
    if (pendingPageSave) {
      clearTimeout(titleSaveTimer);
      savePage().catch(() => {});
    }
  };
  titleInput.onkeydown = async (event) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing && pageWriteAccess) {
      event.preventDefault();
      if (pendingPageSave) {
        clearTimeout(titleSaveTimer);
        await savePage().catch(() => {});
      }
      if (activePage.isDatabase) {
        document.querySelector('#content .record-title')?.focus();
        return;
      }
      if (activePage.blocks.length) focusBlock(0);
      else await addBlock(0, 'text');
    }
  };

  head.querySelector('.page-icon').onclick = async () => {
    const icon = await openIconPickerDialog(activePage.icon || '📄');
    if (icon !== null) {
      activePage.icon = icon;
      pendingPageSave = true;
      await savePage();
      openPage(targetId);
    }
  };

  $('#toggle-favorite').onclick = async () => {
    const next = !activePage.favorite;
    await window.noteon.favorite({ id: targetId, value: next });
    activePage.favorite = next;
    const listed = pages.find((p) => p.id === targetId);
    if (listed) listed.favorite = next;
    const btn = $('#toggle-favorite');
    btn.classList.toggle('active', next);
    btn.textContent = next ? '★ Favorited' : '☆ Favorite';
    drawTree(searchResults);
  };

  $('#priority').onchange = async (event) => {
    activePage.priority = Number(event.target.value);
    pendingPageSave = true;
    await savePage();
  };
  $('#wide').onclick = async () => {
    activePage.full_width = !activePage.full_width;
    pendingPageSave = true;
    await savePage();
    openPage(targetId);
  };
  $('#cover').onclick = async () => {
    activePage.cover_image = await window.noteon.setCover(targetId);
    if (activePage.cover_image) {
      activePage.cover = 0;
      pendingPageSave = true;
      await savePage();
      openPage(targetId);
    }
  };

  const coverImage = head.querySelector('.page-cover');
  if (activePage.cover_image && coverImage) {
    let dragStart = null;
    coverImage.style.cursor = 'move';
    coverImage.onpointerdown = (event) => {
      dragStart = {
        x: event.clientX,
        y: event.clientY,
        coverX: activePage.cover_x ?? 0.5,
        coverY: activePage.cover_y ?? 0.5,
        moved: false,
      };
      coverImage.setPointerCapture(event.pointerId);
    };
    coverImage.onpointermove = (event) => {
      if (!dragStart) return;
      const rect = coverImage.getBoundingClientRect();
      const nextX = Math.max(0, Math.min(1, dragStart.coverX - (event.clientX - dragStart.x) / Math.max(1, rect.width)));
      const nextY = Math.max(0, Math.min(1, dragStart.coverY - (event.clientY - dragStart.y) / Math.max(1, rect.height)));
      dragStart.moved ||= Math.abs(nextX - dragStart.coverX) > 0.001 || Math.abs(nextY - dragStart.coverY) > 0.001;
      activePage.cover_x = nextX;
      activePage.cover_y = nextY;
      coverImage.style.objectPosition = `${nextX * 100}% ${nextY * 100}%`;
    };
    coverImage.onpointerup = async () => {
      if (!dragStart) return;
      const moved = dragStart.moved;
      dragStart = null;
      if (moved) {
        pendingPageSave = true;
        await savePage();
      }
    };
  }

  head.querySelectorAll('[data-cover-style]').forEach((button) => {
    button.onclick = async () => {
      activePage.cover = Number(button.dataset.coverStyle);
      activePage.cover_image = null;
      pendingPageSave = true;
      await savePage();
      openPage(targetId);
    };
  });
  head.querySelectorAll('[data-cover-axis]').forEach((button) => {
    button.onclick = async () => {
      activePage[button.dataset.coverAxis] = Number(button.dataset.coverValue);
      pendingPageSave = true;
      await savePage();
      openPage(targetId);
    };
  });
  head.querySelector('[data-cover-reset]').onclick = async () => {
    activePage.cover_x = 0.5;
    activePage.cover_y = 0.5;
    pendingPageSave = true;
    await savePage();
    openPage(targetId);
  };
  head.querySelector('[data-cover-remove]').onclick = async () => {
    activePage.cover = 0;
    activePage.cover_image = null;
    pendingPageSave = true;
    await savePage();
    openPage(targetId);
  };

  $('#page-undo').onclick = () => undoActivePage().catch(showDockError);
  $('#page-redo').onclick = () => redoActivePage().catch(showDockError);
  $('#page-history').onclick = () => showPageHistory(targetId).catch(showDockError);
  $('#trash').onclick = async () => {
    await window.noteon.trashPage(targetId);
    closeTrashedOrDeletedTabs([targetId]);
    await refresh();
    if (openTabs.length) {
      await releaseActivePageLease();
      activeId = null;
      activePage = null;
      pageWriteAccess = false;
      $('#content').classList.remove('full-width');
      renderBreadcrumbs(null);
      await openPage(openTabs[activeTabIndex], { skipTab: true });
    } else {
      await resetActivePageDisplay('Moved to trash', '');
    }
  };

  const blocks = document.createElement('div');
  blocks.className = 'blocks';
  content.append(blocks);

  if (activePage.isDatabase) {
    await renderDatabase(blocks, targetId);
    applyPageWriteAccess();
    return;
  }

  renderPageBlocks(blocks, activePage.blocks);
  const addBtn = document.createElement('button');
  addBtn.className = 'add-block';
  addBtn.textContent = '＋ Add a block';
  addBtn.onclick = () => addBlock(activePage.blocks.length, 'text').catch(showDockError);
  blocks.append(addBtn);
  applyPageWriteAccess();

  if (Number.isInteger(options.blockPosition)) {
    const blockNodes = blocks.querySelectorAll('[data-block-id]');
    const match = blockNodes[options.blockPosition];
    if (match) {
      match.scrollIntoView({ block: 'center', behavior: 'smooth' });
      match.classList.add('find-match');
    }
  }
}

function applyPageWriteAccess() {
  if (pageWriteAccess) return;
  const content = $('#content');
  const editableSelectors =
    '.page-title-input,#priority,#wide,#cover,#toggle-favorite,#page-undo,#page-redo,#page-history,#trash,.block-selection-tools button,.block-selection-tools select,.block-selection-tools input,.add-property,.property-row input,.property-row select,.property-menu,.option-picker-trigger,.block-kind-select,.add-block,[contenteditable="true"],.todo,.db-value,.calc-select,.db-add-row-btn,.record-select,[data-select-all],.db-toolbar button,.db-toolbar select,th button';

  content.querySelectorAll(editableSelectors).forEach((element) => {
    if (
      element.classList.contains('record-open') ||
      element.classList.contains('record-dialog-open') ||
      element.classList.contains('page-link-btn') ||
      element.closest('.page-link')
    ) {
      return;
    }
    element.disabled = true;
    if (element.hasAttribute('contenteditable')) {
      element.setAttribute('contenteditable', 'false');
    }
  });

  content.querySelectorAll('.block-controls').forEach((element) => {
    element.hidden = true;
  });
  const icon = content.querySelector('.page-icon');
  if (icon) {
    icon.style.pointerEvents = 'none';
    icon.title = 'Read only while this page is being edited elsewhere';
  }
}

function renderPageProperties(head, pageId) {
  const box = document.createElement('div');
  box.className = 'page-properties';

  if (activePage?.parentDatabaseId && Array.isArray(activePage.databaseProperties) && activePage.databaseProperties.length) {
    for (const property of activePage.databaseProperties) {
      const row = document.createElement('div');
      row.className = 'property-row record-db-property';
      const label = document.createElement('span');
      label.textContent = property.name;
      label.title = `Database ${property.kind}`;
      row.append(label, propertyEditor(property, String(property.value ?? ''), pageId));
      box.append(row);
    }
  }

  for (const property of activePage?.properties || []) {
    const row = document.createElement('div');
    row.className = 'property-row';
    const label = document.createElement('span');
    label.textContent = property.name;
    label.title = property.kind;
    row.append(label, customPagePropertyEditor(property, pageId));

    const manageBtn = document.createElement('button');
    manageBtn.className = 'property-menu';
    manageBtn.type = 'button';
    manageBtn.textContent = '⋯';
    manageBtn.title = `Manage ${property.name}`;
    manageBtn.onclick = () => manageCustomPageProperty(property, pageId).catch(showDockError);
    row.append(manageBtn);
    box.append(row);
  }

  if (!activePage?.isDatabase) {
    const addBtn = document.createElement('button');
    addBtn.className = 'add-property';
    addBtn.textContent = '＋ Add property';
    addBtn.onclick = async () => {
      const created = await openAddPropertyDialog();
      if (!created) return;
      await window.noteon.addCustomProperty({ pageId, ...created });
      await openPage(pageId, { skipTab: true, suppressHistory: true });
    };
    box.append(addBtn);
  }

  if (box.children.length) head.append(box);
}

function customPagePropertyEditor(property, pageId) {
  const options = propertyOptions(property);
  const value = String(property.value || '');

  if (['created_time', 'last_edited_time'].includes(property.kind)) {
    const output = document.createElement('span');
    const timestamp = property.kind === 'created_time' ? activePage?.created_at : activePage?.updated_at || activePage?.created_at;
    output.className = 'property-value-generated';
    output.textContent = value.trim() ? value : formatGeneratedDate(timestamp);
    output.title = value.trim() ? 'Stored property value' : 'Generated by the workspace';
    return output;
  }

  if (['select', 'status', 'multi-select'].includes(property.kind)) {
    return optionPicker(property, value, {
      onValue: (next) => window.noteon.setCustomValue({ pageId, propertyId: property.id, value: next }),
      onOptions: (next, optSettings) =>
        window.noteon.updateCustomProperty({
          pageId,
          propertyId: property.id,
          options: normalizePropertyOptions(next, options, optSettings?.preserveExisting !== false),
        }),
      onRemove: (name) => window.noteon.removeCustomPropertyOption({ pageId, propertyId: property.id, name }),
    });
  }

  const input = document.createElement('input');
  if (property.kind === 'checkbox') {
    input.type = 'checkbox';
    input.checked = ['true', '1'].includes(value);
  } else {
    input.type =
      property.kind === 'date'
        ? 'date'
        : property.kind === 'email'
          ? 'email'
          : property.kind === 'url'
            ? 'url'
            : property.kind === 'phone'
              ? 'tel'
              : 'text';
    input.value = property.kind === 'number' ? formatNumberDisplay(value) : value;
    if (property.kind === 'number') input.inputMode = 'decimal';
  }

  input.setAttribute('aria-label', property.name);
  input.placeholder = propertyValuePlaceholder(property.kind, pageId);

  const save = async () => {
    let next = property.kind === 'checkbox' ? String(input.checked) : input.value;
    if (property.kind === 'number' && next.trim()) {
      const result = evaluateNumberExpression(next);
      if (result === null) {
        showDockError(new Error('Enter a valid number expression.'));
        input.focus();
        return false;
      }
      next = result;
      input.value = result;
    }
    if (next === String(property.value ?? '')) return true;
    await window.noteon.setCustomValue({ pageId, propertyId: property.id, value: next });
    property.value = next;
    if (property.kind === 'number') input.dataset.savedValue = next;
    return true;
  };

  if (property.kind === 'number') {
    input.dataset.savedValue = value;
    input.onfocus = () => {
      input.value = input.dataset.savedValue ?? '';
    };
    input.onblur = async () => {
      try {
        const saved = await save();
        if (saved) {
          input.dataset.savedValue = String(property.value ?? '');
          input.value = formatNumberDisplay(input.dataset.savedValue);
        }
      } catch (error) {
        showDockError(error);
      }
    };
    input.onkeydown = (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        save()
          .then((saved) => {
            if (saved) input.blur();
          })
          .catch(showDockError);
      }
    };
  } else {
    input.onchange = () => save().catch(showDockError);
  }

  return attachQuickActionWrapper(input, property.kind, value, 'property-url-editor', 'property-url-open');
}

async function manageCustomPageProperty(property, pageId) {
  const changed = await openManagePropertyDialog(property, {
    isCustom: true,
    onRename: (name) => window.noteon.updateCustomProperty({ pageId, propertyId: property.id, name }),
    onChangeKind: (kind) => window.noteon.updateCustomProperty({ pageId, propertyId: property.id, kind }),
    onUpdateOptions: (options) => window.noteon.updateCustomProperty({ pageId, propertyId: property.id, options }),
    onRenameOption: (oldName, newName) => window.noteon.renameCustomPropertyOption({ pageId, propertyId: property.id, oldName, newName }),
    onRemoveOption: (name) => window.noteon.removeCustomPropertyOption({ pageId, propertyId: property.id, name }),
    onMove: (direction) => window.noteon.moveCustomProperty({ pageId, propertyId: property.id, direction }),
    onDelete: () => window.noteon.deleteCustomProperty({ pageId, propertyId: property.id }),
  });
  if (changed) {
    await openPage(pageId, { skipTab: true, suppressHistory: true });
  }
}

async function updatePageHistoryState(pageId) {
  const history = await window.noteon.getPageHistory(pageId);
  if (activeId !== Number(pageId) || !activePage) return;
  activePage.history = history;
  const undo = document.querySelector('#page-undo');
  const redo = document.querySelector('#page-redo');
  if (undo) undo.disabled = !history.canUndo;
  if (redo) redo.disabled = !history.canRedo;
}

async function undoActivePage() {
  if (!activeId || !pageWriteAccess) return;
  const id = activeId;
  await flushLocalEdits();
  if (await window.noteon.undoPage(id)) {
    await openPage(id, { skipTab: true });
  }
}

async function redoActivePage() {
  if (!activeId || !pageWriteAccess) return;
  const id = activeId;
  await flushLocalEdits();
  if (await window.noteon.redoPage(id)) {
    await openPage(id, { skipTab: true });
  }
}

async function updateWorkspaceHistoryControls() {
  const historyState = await window.noteon.getWorkspaceHistory();
  $('#workspace-undo').disabled = !historyState.canUndo;
  $('#workspace-redo').disabled = !historyState.canRedo;
}

async function workspaceHistoryStep(direction) {
  const activeBefore = activeId;
  await beforeTreeMutation();
  const result = direction === 'undo' ? await window.noteon.undoWorkspace() : await window.noteon.redoWorkspace();
  if (!result) {
    await updateWorkspaceHistoryControls();
    setSaveState(direction === 'undo' ? 'Nothing to undo' : 'Nothing to redo');
    return;
  }

  await refresh();
  if (result.type === 'page' && pages.some((page) => page.id === result.pageId)) {
    await openPage(result.pageId, { skipTab: true });
  } else {
    const removesPages =
      (direction === 'undo' && (result.type === 'restore' || result.type === 'create-record')) ||
      (direction === 'redo' && result.type === 'trash');
    const restoresPages =
      (direction === 'undo' && result.type === 'trash') ||
      (direction === 'redo' && (result.type === 'restore' || result.type === 'create-record'));

    if (removesPages && activeBefore && treeContainsPage(result.pageIds, activeBefore)) {
      await closeActiveTreePage([activeBefore]);
    } else if (restoresPages) {
      const target =
        activeBefore && pages.some((page) => page.id === activeBefore && page.trashed_by === null)
          ? activeBefore
          : result.pageIds?.at(-1);
      if (target && pages.some((page) => page.id === target && page.trashed_by === null)) {
        await openPage(target, { skipTab: true });
      }
    }
  }
  await updateWorkspaceHistoryControls();
}
