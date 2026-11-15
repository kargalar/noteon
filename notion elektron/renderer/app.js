/**
 * Application entry point: wires topbar/workspace actions, IPC event listeners,
 * global keyboard shortcuts, context menus, and initial session restoration.
 */

/**
 * Bind topbar and utility dialog button handlers.
 */
function bindToolbarActions() {
  $('#help').onclick = () => $('#help-dialog').showModal();
  $('#help-close').onclick = () => $('#help-dialog').close();
  $('#retry-save').onclick = () => retryPendingSave();
  $('#find-page').onclick = () => showPageFind();

  $('#workspace-undo').onclick = () => workspaceHistoryStep('undo').catch(showDockError);
  $('#workspace-redo').onclick = () => workspaceHistoryStep('redo').catch(showDockError);

  $('#import-md').onclick = async () => {
    await beforeTreeMutation();
    const id = await window.noteon.importMarkdown();
    if (id) {
      await refresh();
      await openPage(id);
    }
  };

  $('#export-md').onclick = async () => {
    if (!activeId) return;
    await flushLocalEdits();
    await window.noteon.exportMarkdown(activeId);
  };

  $('#backup').onclick = async () => {
    await beforeTreeMutation();
    const ok = await window.noteon.backup();
    if (ok) setSaveState('Backup saved');
  };

  $('#restore-backup').onclick = async () => {
    await beforeTreeMutation();
    setSaveState('Restoring backup…');
    const result = await window.noteon.restoreWorkspace();
    if (!result) {
      setSaveState('All changes saved');
      return;
    }
    setSaveState('Workspace restored');
  };

  $('#import-noteon').onclick = async () => {
    await beforeTreeMutation();
    setSaveState('Importing Noteon workspace…');
    const result = await window.noteon.importNoteonWorkspace();
    if (!result) {
      setSaveState('All changes saved');
      return;
    }
    setSaveState('Import completed');
  };
}

/**
 * Subscribe to workspace synchronization, cross-pane lease, settings, and window close IPC events.
 */
function installWorkspaceEventListeners() {
  window.noteon.onPrepareWorkspaceHistoryFlush(async ({ requestId }) => {
    try {
      await navigationQueue;
      await flushLocalEdits();
      window.noteon.workspaceHistoryFlushReady({ requestId, ok: true });
    } catch {
      window.noteon.workspaceHistoryFlushReady({ requestId, ok: false });
    }
  });

  window.noteon.onWorkspaceTreeChanged(() => {
    refresh().catch(() => {});
  });

  window.noteon.onWorkspaceHistoryChanged((historyState) => {
    $('#workspace-undo').disabled = !historyState?.canUndo;
    $('#workspace-redo').disabled = !historyState?.canRedo;
  });

  window.noteon.onWorkspacePageRestored(async ({ pageId }) => {
    await refresh();
    if (activeId === Number(pageId)) {
      await openPage(pageId, { skipTab: true });
    }
  });

  window.noteon.onWorkspacePagesTrashed(async ({ pageIds }) => {
    await refresh();
    await closeActiveTreePage(pageIds || []);
  });

  window.noteon.onWorkspacePageUpdated(({ id, title, icon }) => {
    const listed = pages.find((page) => page.id === Number(id));
    if (listed) {
      if (title !== undefined) listed.title = title;
      if (icon !== undefined) listed.icon = icon;
      drawTree(searchResults);
      renderTabs();
      renderBreadcrumbs(currentDockPageId());
    }
  });

  window.noteon.onSettingsChanged(({ key, value }) => {
    settings[key] = value;
    if (['theme', 'dark', 'accent_color', 'light_background', 'dark_background', 'custom_theme'].includes(key)) {
      applyAppearance();
    } else if (key === 'font_scale') {
      document.documentElement.style.fontSize = `${15 * (Number(value) || 1)}px`;
    } else if (key === 'sidebar_pinned') {
      applySidebarPin();
    } else if (key === 'sidebar_width' || key === 'assistant_panel_width') {
      applyPanelWidths();
    }
  });

  window.noteon.onWorkspaceImportProgress((progress) => {
    if (progress?.total) {
      setSaveState(`Importing attachments (${progress.completed}/${progress.total})…`);
    }
  });

  window.noteon.onWorkspaceReloaded(async () => {
    openTabs = [];
    activeTabIndex = 0;
    pageHistory = [];
    historyIndex = -1;
    selectedBlockIds.clear();
    selectedRecordIds.clear();
    selectedTreePageIds.clear();
    await refresh();
    await updateWorkspaceHistoryControls();
    const firstActive = pages.find((page) => page.trashed_by === null);
    if (firstActive) {
      await openPage(firstActive.id);
    } else {
      await resetActivePageDisplay();
    }
  });

  window.noteon.onPageLeaseReleased(async ({ pageId }) => {
    if (activeId === Number(pageId) && !pageWriteAccess) {
      await openPage(activeId, { skipTab: true, suppressHistory: true });
    }
  });

  window.noteon.onPrepareClose(async () => {
    closing = true;
    const app = document.querySelector('.app');
    if (app) app.inert = true;
    try {
      await navigationQueue;
      await flushLocalEdits();
      if (!embeddedPane) {
        await window.noteon.closeDockViews();
      }
    } finally {
      window.noteon.closeReady();
    }
  });
}

/**
 * Handle tree keyboard navigation and page clipboard actions when `#tree` has focus.
 * @param {KeyboardEvent} event
 */
async function handleTreeShortcutKey(event) {
  const mod = event.ctrlKey || event.metaKey;
  const visible = currentVisibleTreeItems();
  if (!visible.length && !(mod && event.key.toLowerCase() === 'v')) return;

  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const ids = visible.map((item) => item.page.id);
    const currentIdx = ids.indexOf(treeSelectionAnchorId ?? activeId);
    const nextIdx =
      currentIdx < 0
        ? 0
        : Math.max(0, Math.min(ids.length - 1, currentIdx + (event.key === 'ArrowDown' ? 1 : -1)));
    const nextId = ids[nextIdx];
    if (nextId !== undefined) {
      if (event.shiftKey) {
        selectedTreePageIds.add(nextId);
      } else {
        selectedTreePageIds = new Set([nextId]);
      }
      treeSelectionAnchorId = nextId;
      drawTree(searchResults);
    }
    return;
  }

  if (event.key === 'Enter' && selectedTreePageIds.size) {
    event.preventDefault();
    const targetId = treeSelectionAnchorId || [...selectedTreePageIds][0];
    if (targetId) await openPage(targetId);
    return;
  }

  if (mod && (event.key.toLowerCase() === 'c' || event.key.toLowerCase() === 'x')) {
    event.preventDefault();
    await treeClipboard(event.key.toLowerCase() === 'x');
    return;
  }

  if (mod && event.key.toLowerCase() === 'v') {
    event.preventDefault();
    await beforeTreeMutation();
    const pasted = await window.noteon.pastePages({ parentId: treeSelectionAnchorId || null });
    await refresh();
    if (pasted?.length) await openPage(pasted[pasted.length - 1]);
    return;
  }

  if ((event.key === 'Delete' || event.key === 'Backspace') && selectedTreePageIds.size && treeMode !== 'trash') {
    event.preventDefault();
    const ids = [...selectedTreePageIds];
    await beforeTreeMutation();
    await window.noteon.trashPages(ids);
    selectedTreePageIds.clear();
    await refresh();
    await closeActiveTreePage(ids);
  }
}

/**
 * Install global application keyboard shortcuts across tree, block editor, database, and workspace scopes.
 */
function installGlobalShortcuts() {
  document.addEventListener('keydown', (event) => {
    const mod = event.ctrlKey || event.metaKey;
    const keyLower = event.key.toLowerCase();
    const inEditable = Boolean(event.target?.closest?.('input, textarea, select, [contenteditable="true"]'));
    const inTree = Boolean(event.target?.closest?.('#tree'));

    if (inTree && !event.target.matches?.('input')) {
      handleTreeShortcutKey(event).catch(showDockError);
      if (['ArrowUp', 'ArrowDown', 'Enter', 'Delete', 'Backspace'].includes(event.key) || (mod && ['c', 'x', 'v'].includes(keyLower))) {
        return;
      }
    }

    if (selectedBlockIds.size && !inEditable) {
      if (event.key === 'Escape') {
        selectedBlockIds.clear();
        blockSelectionAnchorId = null;
        openPage(activeId, { suppressHistory: true, skipTab: true }).catch(showDockError);
        return;
      }
      if ((event.key === 'Delete' || event.key === 'Backspace') && pageWriteAccess) {
        event.preventDefault();
        (async () => {
          await flushLocalEdits();
          await window.noteon.deleteBlocks({ ids: [...selectedBlockIds] });
          selectedBlockIds.clear();
          blockSelectionAnchorId = null;
          await openPage(activeId);
        })().catch(showDockError);
        return;
      }
      if (mod && keyLower === 'd' && pageWriteAccess) {
        event.preventDefault();
        (async () => {
          await flushLocalEdits();
          await window.noteon.duplicateBlocks({ ids: [...selectedBlockIds] });
          await openPage(activeId);
        })().catch(showDockError);
        return;
      }
      if (mod && (keyLower === 'c' || keyLower === 'x')) {
        event.preventDefault();
        copySelectedBlocks(keyLower === 'x').catch(showDockError);
        return;
      }
      if (mod && keyLower === 'v' && pageWriteAccess) {
        event.preventDefault();
        pasteBlocksIntoPage().catch(showDockError);
        return;
      }
    }

    if (selectedRecordIds.size && selectedDatabaseId && !inEditable) {
      if (mod && keyLower === 'd' && pageWriteAccess) {
        event.preventDefault();
        duplicateSelectedDatabaseRows(selectedDatabaseId).catch(showDockError);
        return;
      }
      if (mod && (keyLower === 'c' || keyLower === 'x')) {
        event.preventDefault();
        copySelectedDatabasePages(keyLower === 'x').catch(showDockError);
        return;
      }
      if (mod && keyLower === 'v' && pageWriteAccess) {
        event.preventDefault();
        pasteSelectedPagesIntoCurrentView().catch(showDockError);
        return;
      }
    }

    if (mod && keyLower === 'z' && !event.altKey) {
      if (inEditable && !event.target.closest?.('#content')) return;
      if (inEditable && event.target.closest?.('.block-editor, .page-title-input')) {
        event.preventDefault();
        if (event.shiftKey) redoActivePage().catch(showDockError);
        else undoActivePage().catch(showDockError);
        return;
      }
      if (!inEditable) {
        event.preventDefault();
        workspaceHistoryStep(event.shiftKey ? 'redo' : 'undo').catch(showDockError);
        return;
      }
    }

    if (mod && keyLower === 'y' && !event.shiftKey && !event.altKey) {
      if (inEditable && event.target.closest?.('.block-editor, .page-title-input')) {
        event.preventDefault();
        redoActivePage().catch(showDockError);
        return;
      }
      if (!inEditable) {
        event.preventDefault();
        workspaceHistoryStep('redo').catch(showDockError);
        return;
      }
    }

    if (mod && keyLower === 'b' && !event.shiftKey && !inEditable && !embeddedPane) {
      event.preventDefault();
      toggleSidebar().catch(showDockError);
      return;
    }

    if (mod && keyLower === 'f') {
      event.preventDefault();
      showPageFind();
      return;
    }

    if (event.altKey && event.key === 'ArrowLeft') {
      event.preventDefault();
      navigateHistory(-1);
      return;
    }

    if (event.altKey && event.key === 'ArrowRight') {
      event.preventDefault();
      navigateHistory(1);
      return;
    }

    if (mod && keyLower === 'j' && !embeddedPane) {
      event.preventDefault();
      setAssistantOpen($('#assistant-panel').hidden);
      return;
    }

    if (mod && keyLower === 'w' && openTabs.length) {
      event.preventDefault();
      removeLocalDockTab(openTabs[activeTabIndex]).catch(showDockError);
      return;
    }

    if (mod && keyLower === 'k' && !embeddedPane) {
      event.preventDefault();
      $('#search')?.focus();
      $('#search')?.select();
      return;
    }

    if (mod && keyLower === 'n' && !embeddedPane) {
      event.preventDefault();
      $('#new-page')?.click();
      return;
    }

    if (mod && keyLower === 's') {
      event.preventDefault();
      retryPendingSave().catch(showDockError);
    }
  });
}

/**
 * Install the right-click context menu for inline Noteon links, sidebar tree rows, and section themes.
 */
function installContextMenuHandler() {
  document.addEventListener('contextmenu', (event) => {
    const editor = event.target?.closest?.('.block-editor');
    if (editor) {
      const link = linkAtEditorPoint(editor, event.clientX, event.clientY);
      if (link) {
        showNoteonLinkMenu(event, link);
        return;
      }
    }

    const treeRow = event.target?.closest?.('.tree-row[data-page-id]');
    if (treeRow) {
      const pageId = Number(treeRow.dataset.pageId);
      const page = pages.find((item) => item.id === pageId);
      if (page) showTreeRowContextMenu(event, page);
      return;
    }

    if (event.target?.closest?.('.db-table')) {
      openFloatingContextMenu(event, [['Customize table theme…', () => openSettingsDialog('table')]]);
    } else if (event.target?.closest?.('.cards')) {
      openFloatingContextMenu(event, [['Customize gallery theme…', () => openSettingsDialog('gallery')]]);
    } else if (event.target?.closest?.('.board')) {
      openFloatingContextMenu(event, [['Customize board theme…', () => openSettingsDialog('board')]]);
    } else if (event.target?.closest?.('.page-head')) {
      openFloatingContextMenu(event, [['Customize header theme…', () => openSettingsDialog('header')]]);
    } else if (event.target?.closest?.('.block')) {
      openFloatingContextMenu(event, [['Customize editor theme…', () => openSettingsDialog('editor')]]);
    } else if (event.target?.closest?.('.sidebar')) {
      openFloatingContextMenu(event, [['Customize sidebar theme…', () => openSettingsDialog('sidebar')]]);
    }
  });
}

/**
 * Restore dock layout, open tabs, expanded tree state, and initial active page from settings or URL query.
 */
async function restoreDockAndInitialPage() {
  try {
    expandedPageIds = new Set(JSON.parse(settings.electron_expanded || '[]').map(Number).filter(Number.isSafeInteger));
  } catch {
    expandedPageIds = new Set();
  }

  const rawDock = settings[`${windowLayoutPrefix}_dock_layout`] || (!embeddedPane ? settings.dock_layout : null);
  if (rawDock) {
    try {
      const parsedDock = JSON.parse(rawDock);
      const validated = NoteonDock.validate(parsedDock);
      if (validated && NoteonDock.find(validated.root, rendererPaneId)) {
        dockLayout = validated;
        renderDock();
      }
    } catch {}
  }

  const activeSet = new Set(pages.filter((page) => page.trashed_by === null).map((page) => page.id));
  try {
    const savedTabs = JSON.parse(settings[`${windowLayoutPrefix}_tabs`] || '{}');
    if (Array.isArray(savedTabs.tabs)) {
      openTabs = savedTabs.tabs.map(Number).filter((id) => activeSet.has(id));
      activeTabIndex = Math.max(0, Math.min(Number(savedTabs.active) || 0, Math.max(0, openTabs.length - 1)));
    }
  } catch {}

  const requestedPage = Number(pageQuery.get('pageId'));
  const targetPage =
    (Number.isSafeInteger(requestedPage) && activeSet.has(requestedPage) && requestedPage) ||
    openTabs[activeTabIndex] ||
    (settings.last_page && activeSet.has(Number(settings.last_page)) && Number(settings.last_page)) ||
    pages.find((page) => page.trashed_by === null)?.id ||
    null;

  drawTree();
  renderTabs();

  if (targetPage) {
    await openPage(targetPage);
  } else {
    renderEmptyWorkspace();
  }

  if (embeddedPane) {
    window.noteon.reportDockPaneState({ tabs: [...openTabs], active: activeTabIndex });
  }
}

/**
 * Boot sequence executed after all module scripts have loaded.
 */
async function bootstrapApplication() {
  if (embeddedPane) {
    document.body.classList.add('embedded-mode');
  }

  initThemeSettings();
  initDockAndTabs();
  initSidebarTree();
  initBlockDropHandlers();
  initAssistant();
  bindToolbarActions();
  installWorkspaceEventListeners();
  installGlobalShortcuts();
  installContextMenuHandler();

  try {
    settings = await window.noteon.getSettings();
    applySidebarPin();
    applyPanelWidths();
    applyAppearance();
    if (settings.font_scale) {
      document.documentElement.style.fontSize = `${15 * (Number(settings.font_scale) || 1)}px`;
    }

    await refresh();
    await updateWorkspaceHistoryControls();
    await restoreDockAndInitialPage();

    const overlay = $('#startup-overlay');
    if (overlay) {
      overlay.remove();
    }
  } catch (error) {
    const overlay = $('#startup-overlay');
    if (overlay) {
      overlay.innerHTML = '';
      const card = document.createElement('div');
      card.className = 'startup-card startup-failure';
      const title = document.createElement('strong');
      title.textContent = 'Could not open workspace';
      const message = document.createElement('div');
      message.className = 'startup-message';
      message.textContent = error?.message || String(error);
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.textContent = 'Retry';
      retry.onclick = () => location.reload();
      card.append(title, message, retry);
      overlay.append(card);
    }
  }
}

bootstrapApplication();
