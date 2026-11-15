/**
 * Dock split-pane management, tab strip rendering, cross-pane tab movement, and back/forward history.
 */

let primaryPaneElement = null;

function renderDock() {
  if (!primaryPaneElement) {
    primaryPaneElement = $('#primary-pane');
    nativePaneHosts.set('primary', primaryPaneElement);
  }

  NoteonDockView.mount($('#dock-root'), dockLayout, {
    pane: (node) => {
      if (node.id === rendererPaneId) return primaryPaneElement;
      let host = nativePaneHosts.get(node.id);
      if (!host) {
        host = document.createElement('div');
        host.className = 'dock-native-pane';
        nativePaneHosts.set(node.id, host);
      }
      return host;
    },
    resize: (path, ratio) => {
      dockLayout = NoteonDock.resize(dockLayout, path, ratio);
      persistDockLayout();
      syncNativeDockViews().catch(showDockError);
    },
  });

  syncNativeDockViews().catch(showDockError);
}

async function persistDockLayout() {
  return window.noteon.setSetting({ key: `${windowLayoutPrefix}_dock_layout`, value: JSON.stringify(dockLayout) });
}

async function persistPrimaryDock() {
  const pane = NoteonDock.find(dockLayout.root, rendererPaneId);
  if (!pane) return;

  pane.tabs = [...openTabs];
  pane.active = Math.max(0, Math.min(activeTabIndex, Math.max(0, pane.tabs.length - 1)));
  await window.noteon.setSetting({ key: `${windowLayoutPrefix}_dock_layout`, value: JSON.stringify(dockLayout) });
  await window.noteon.setSetting({ key: `${windowLayoutPrefix}_tabs`, value: JSON.stringify({ tabs: openTabs, active: pane.active }) });

  if (embeddedPane) {
    window.noteon.reportDockPaneState({ tabs: [...openTabs], active: pane.active });
  } else {
    await syncNativeDockViews();
  }
}

async function syncNativeDockViews() {
  if (embeddedPane) return;
  const overlayOpen = Boolean(document.querySelector('dialog[open], .theme-context-menu, .option-picker.open'));
  const entries = [];

  for (const pane of NoteonDock.leaves(dockLayout.root)) {
    if (pane.id === 'primary') continue;
    const host = nativePaneHosts.get(pane.id);
    if (!host?.isConnected) continue;

    const rect = host.getBoundingClientRect();
    const pageId = pane.tabs[pane.active] || 0;
    const bounds = overlayOpen
      ? { x: 0, y: 0, width: 0, height: 0 }
      : { x: rect.left, y: rect.top, width: rect.width, height: rect.height };
    entries.push({ paneId: pane.id, bounds });

    await window.noteon.ensureDockView({
      paneId: pane.id,
      pageId,
      bounds,
      tabs: pane.tabs,
      active: pane.active,
      focused: dockLayout.focused === pane.id,
    });
  }

  await window.noteon.setDockViewBounds(entries);
}

function setDockFocus(paneId) {
  if (!NoteonDock.find(dockLayout.root, paneId)) return;
  dockLayout = NoteonDock.focus(dockLayout, paneId);
  persistDockLayout().catch(showDockError);
  if (!embeddedPane) {
    $('#close-pane').hidden = paneId === 'primary';
    renderBreadcrumbs(currentDockPageId());
  }
  drawTree();
}

function chooseSplitDirection() {
  const dialog = $('#split-dialog');
  dialog.returnValue = '';
  dialog.showModal();
  return new Promise((resolve) =>
    dialog.addEventListener(
      'close',
      () => resolve(['left', 'right', 'top', 'bottom'].includes(dialog.returnValue) ? dialog.returnValue : null),
      { once: true },
    ),
  );
}

async function splitFocusedPane() {
  if (embeddedPane) return;
  const paneId = dockLayout.focused;
  const pane = NoteonDock.find(dockLayout.root, paneId);
  const pageId = pane?.tabs[pane.active] || activeId;
  if (!pageId) return;

  const direction = await chooseSplitDirection();
  if (!direction) return;

  const newId = `pane-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  dockLayout = NoteonDock.split(dockLayout, paneId, newId, direction, Number(pageId));
  dockLayout.focused = newId;
  await persistDockLayout();
  $('#close-pane').hidden = false;
  renderDock();
}

async function closeFocusedPane() {
  const paneId = dockLayout.focused;
  if (paneId === 'primary') return;
  await window.noteon.removeDockView(paneId);
  dockLayout = NoteonDock.closePane(dockLayout, paneId);
  nativePaneHosts.delete(paneId);
  await persistDockLayout();
  renderDock();
  setDockFocus(dockLayout.focused);
}

function updatePaneState({ paneId, tabs, active }) {
  const pane = NoteonDock.find(dockLayout.root, paneId);
  if (!pane || !Array.isArray(tabs) || !tabs.every((id) => Number.isSafeInteger(Number(id)) && Number(id) > 0)) return;
  pane.tabs = tabs.map(Number);
  pane.active = Math.max(0, Math.min(Number(active) || 0, Math.max(0, pane.tabs.length - 1)));
  persistDockLayout().catch(showDockError);

  if (dockLayout.focused === paneId) {
    refresh()
      .then(() => {
        renderBreadcrumbs(currentDockPageId());
        drawTree();
      })
      .catch(showDockError);
  }
}

async function activateTab(id, newTab = false) {
  const targetId = Number(id);
  if (!openTabs.length) {
    openTabs = [targetId];
    activeTabIndex = 0;
  } else if (newTab) {
    const existing = openTabs.indexOf(targetId);
    if (existing >= 0) activeTabIndex = existing;
    else {
      openTabs.splice(activeTabIndex + 1, 0, targetId);
      activeTabIndex++;
    }
  } else {
    const existing = openTabs.indexOf(targetId);
    if (existing >= 0) activeTabIndex = existing;
    else openTabs[activeTabIndex] = targetId;
  }
  renderTabs();
  await persistPrimaryDock();
}

async function reorderLocalDockTab(pageId, toIndex) {
  const fromIndex = openTabs.indexOf(Number(pageId));
  if (fromIndex < 0) return false;

  dockLayout = NoteonDock.moveTab(dockLayout, rendererPaneId, fromIndex, rendererPaneId, toIndex);
  const pane = NoteonDock.find(dockLayout.root, rendererPaneId);
  openTabs = [...pane.tabs];
  activeTabIndex = pane.active;
  renderTabs();
  await persistPrimaryDock();
  return true;
}

async function removeLocalDockTab(pageId) {
  pageId = Number(pageId);
  const index = openTabs.indexOf(pageId);
  if (index < 0) return false;

  await flushLocalEdits();
  const wasActive = index === activeTabIndex;
  openTabs.splice(index, 1);
  if (index < activeTabIndex) activeTabIndex--;
  if (activeTabIndex >= openTabs.length) activeTabIndex = openTabs.length - 1;
  activeTabIndex = Math.max(0, activeTabIndex);

  await persistPrimaryDock();
  renderTabs();

  if (wasActive) {
    if (openTabs.length) {
      await openPage(openTabs[activeTabIndex], { skipTab: true });
    } else {
      await resetActivePageDisplay();
    }
  }
  return true;
}

async function insertLocalDockTab(pageId, index) {
  pageId = Number(pageId);
  await refresh();
  if (!pages.some((page) => page.id === pageId && page.trashed_by === null)) {
    throw new Error('The page is no longer available.');
  }

  const existing = openTabs.indexOf(pageId);
  if (existing >= 0) {
    activeTabIndex = existing;
  } else {
    const targetIndex = Math.max(0, Math.min(Number(index) || 0, openTabs.length));
    openTabs.splice(targetIndex, 0, pageId);
    activeTabIndex = targetIndex;
  }

  const pane = NoteonDock.find(dockLayout.root, rendererPaneId);
  if (pane) {
    pane.tabs = [...openTabs];
    pane.active = activeTabIndex;
    dockLayout = NoteonDock.focus(dockLayout, rendererPaneId);
  }
  renderTabs();
  await persistPrimaryDock();
  await openPage(pageId, { skipTab: true });
  return true;
}

async function moveDockTabBetween({ fromPaneId, pageId, toPaneId, toIndex }) {
  if (embeddedPane) return false;
  fromPaneId = String(fromPaneId);
  toPaneId = String(toPaneId);
  pageId = Number(pageId);
  toIndex = Number(toIndex);

  const source = NoteonDock.find(dockLayout.root, fromPaneId);
  const target = NoteonDock.find(dockLayout.root, toPaneId);
  if (!source || !target || fromPaneId === toPaneId || !source.tabs.includes(pageId)) {
    throw new Error('The tab move is no longer valid.');
  }

  const oldLayout = structuredClone(dockLayout);
  const sourceIndex = source.tabs.indexOf(pageId);
  const targetAlreadyHasPage = target.tabs.includes(pageId);

  await beforeTreeMutation();
  try {
    if (fromPaneId === 'primary') await removeLocalDockTab(pageId);
    else await window.noteon.removeDockTab({ paneId: fromPaneId, pageId });

    dockLayout = targetAlreadyHasPage
      ? NoteonDock.closeTab(oldLayout, fromPaneId, sourceIndex)
      : NoteonDock.moveTab(oldLayout, fromPaneId, sourceIndex, toPaneId, Math.max(0, Math.min(toIndex, target.tabs.length)));
    await persistDockLayout();

    if (toPaneId === 'primary') await insertLocalDockTab(pageId, toIndex);
    else await window.noteon.insertDockTab({ paneId: toPaneId, pageId, index: toIndex });

    dockLayout = NoteonDock.focus(dockLayout, toPaneId);
    await persistDockLayout();
    renderDock();
    setDockFocus(toPaneId);
    return true;
  } catch (error) {
    try {
      if (fromPaneId === 'primary') await insertLocalDockTab(pageId, sourceIndex);
      else await window.noteon.insertDockTab({ paneId: fromPaneId, pageId, index: sourceIndex });
    } catch {
      // Best-effort rollback before restoring previous dock layout.
    }
    dockLayout = oldLayout;
    await persistDockLayout();
    renderDock();
    throw error;
  }
}

function renderTabs() {
  const bar = $('#tabs');
  if (!bar) return;
  bar.innerHTML = '';

  bar.ondragover = (event) => {
    if (event.dataTransfer.types.includes('application/x-noteon-tab')) event.preventDefault();
  };
  bar.ondragleave = (event) => {
    if (!bar.contains(event.relatedTarget)) {
      bar.querySelectorAll('.tab-drop-target').forEach((tab) => tab.classList.remove('tab-drop-target'));
    }
  };
  bar.ondrop = async (event) => {
    const raw = event.dataTransfer.getData('application/x-noteon-tab');
    if (!raw) return;
    event.preventDefault();
    bar.querySelectorAll('.tab-drop-target').forEach((tab) => tab.classList.remove('tab-drop-target'));

    try {
      const source = JSON.parse(raw);
      const targetTab = event.target.closest('.tab');
      const rect = targetTab?.getBoundingClientRect();
      const targetIndex = targetTab ? Number(targetTab.dataset.index) + (event.clientX > rect.left + rect.width / 2 ? 1 : 0) : openTabs.length;
      if (String(source.paneId) === rendererPaneId) {
        await reorderLocalDockTab(Number(source.pageId), targetIndex);
      } else if (embeddedPane) {
        window.noteon.requestDockTabMove({ pageId: Number(source.pageId), toPaneId: rendererPaneId, toIndex: targetIndex });
      } else {
        await moveDockTabBetween({ ...source, toPaneId: rendererPaneId, toIndex: targetIndex });
      }
    } catch (error) {
      showDockError(error);
    }
  };

  openTabs.forEach((id, index) => {
    const page = pages.find((item) => item.id === id);
    const tab = document.createElement('div');
    tab.className = `tab${index === activeTabIndex ? ' active' : ''}`;
    tab.dataset.index = String(index);
    tab.draggable = true;

    tab.ondragstart = (event) => {
      event.dataTransfer.setData('application/x-noteon-tab', JSON.stringify({ paneId: rendererPaneId, pageId: id, index }));
      event.dataTransfer.effectAllowed = 'move';
    };
    tab.ondragend = () => bar.querySelectorAll('.tab-drop-target').forEach((item) => item.classList.remove('tab-drop-target'));
    tab.ondragover = (event) => {
      if (event.dataTransfer.types.includes('application/x-noteon-tab')) {
        event.preventDefault();
        tab.classList.add('tab-drop-target');
      }
    };
    tab.ondragleave = () => tab.classList.remove('tab-drop-target');
    tab.innerHTML = `<button class="tab-title">${esc(page?.icon || '📄')} ${esc(page?.title || 'Untitled')}</button><button class="tab-close" title="Close tab">×</button>`;

    tab.querySelector('.tab-title').onclick = () => {
      activeTabIndex = index;
      persistPrimaryDock().then(() => openPage(id, { skipTab: true }));
    };

    tab.querySelector('.tab-close').onclick = async (event) => {
      event.stopPropagation();
      await flushLocalEdits();
      openTabs.splice(index, 1);
      if (index < activeTabIndex) activeTabIndex--;
      if (activeTabIndex >= openTabs.length) activeTabIndex = openTabs.length - 1;
      await persistPrimaryDock();
      renderTabs();
      if (openTabs.length) {
        activeTabIndex = Math.max(0, activeTabIndex);
        await openPage(openTabs[activeTabIndex], { skipTab: true });
      } else {
        await resetActivePageDisplay();
      }
    };

    bar.append(tab);
  });
}

function updateHistoryControls() {
  $('#history-back').disabled = historyIndex <= 0;
  $('#history-forward').disabled = historyIndex < 0 || historyIndex >= pageHistory.length - 1;
}

function recordPageHistory(id) {
  if (pageHistory[historyIndex] === id) return;
  pageHistory.splice(historyIndex + 1);
  pageHistory.push(id);
  if (pageHistory.length > 100) pageHistory.shift();
  historyIndex = pageHistory.length - 1;
  updateHistoryControls();
}

function navigateHistory(delta) {
  const target = historyIndex + delta;
  if (target < 0 || target >= pageHistory.length) return;
  openPage(pageHistory[target], { historyTarget: target });
}

function initDockAndTabs() {
  renderDock();

  $('#history-back').onclick = () => navigateHistory(-1);
  $('#history-forward').onclick = () => navigateHistory(1);
  $('#split-window').onclick = splitFocusedPane;
  document.querySelectorAll('[data-split-direction]').forEach((button) => {
    button.onclick = () => $('#split-dialog').close(button.dataset.splitDirection);
  });
  $('#split-dialog-close').onclick = () => $('#split-dialog').close();
  $('#close-pane').onclick = () => closeFocusedPane().catch(showDockError);

  window.noteon.onDockPaneFocus(({ paneId }) => setDockFocus(paneId));
  window.noteon.onDockPaneState(updatePaneState);
  window.noteon.onDockPaneGone(({ paneId }) => {
    if (!NoteonDock.find(dockLayout.root, paneId)) return;
    dockLayout = NoteonDock.closePane(dockLayout, paneId);
    nativePaneHosts.delete(paneId);
    persistDockLayout();
    renderDock();
    $('#close-pane').hidden = dockLayout.focused === 'primary';
    setDockFocus(dockLayout.focused);
  });
  window.noteon.onDockTabMoveRequest((request) => moveDockTabBetween(request).catch(showDockError));
  window.noteon.onPaneNavigate(async ({ pageId, newTab, historyTarget }) => {
    await refresh();
    await openPage(Number(pageId), { newTab, historyTarget });
  });
  window.noteon.onPaneTabRemove(async ({ requestId, pageId }) => {
    try {
      const ok = await removeLocalDockTab(pageId);
      window.noteon.tabChangeReady({ requestId, ok });
    } catch (error) {
      window.noteon.tabChangeReady({ requestId, ok: false, error: error.message });
    }
  });
  window.noteon.onPaneTabInsert(async ({ requestId, pageId, index }) => {
    try {
      const ok = await insertLocalDockTab(pageId, index);
      window.noteon.tabChangeReady({ requestId, ok });
    } catch (error) {
      window.noteon.tabChangeReady({ requestId, ok: false, error: error.message });
    }
  });
  window.noteon.onPanePrepareClose(async ({ requestId, closing: beingRemoved }) => {
    try {
      if (beingRemoved) document.querySelector('.app').inert = true;
      await navigationQueue;
      await flushLocalEdits();
      window.noteon.paneCloseReady({ requestId, ok: true });
    } catch {
      if (beingRemoved) document.querySelector('.app').inert = false;
      window.noteon.paneCloseReady({ requestId, ok: false });
    }
  });

  if (!embeddedPane) {
    const root = $('#dock-root');
    root.addEventListener('pointerdown', (event) => {
      const pane = event.target.closest('.dock-pane');
      if (pane) setDockFocus(pane.dataset.paneId);
    });
    const observer = new ResizeObserver(() => syncNativeDockViews().catch(showDockError));
    observer.observe(root);
    window.addEventListener('resize', () => syncNativeDockViews().catch(showDockError));
    const overlayObserver = new MutationObserver(() => syncNativeDockViews().catch(() => {}));
    overlayObserver.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['open', 'class'] });
  } else {
    document.addEventListener('pointerdown', () => window.noteon.focusDockPane(), { passive: true });
  }
}
