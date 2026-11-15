/**
 * Sidebar workspace tree rendering, virtualized scrolling, search hits, multi-select actions, and trash view.
 */

async function refresh() {
  const workspace = await window.noteon.summary();
  pages = workspace.pages;
  $('#page-count').textContent = `${workspace.count.toLocaleString()} pages`;
  $('#data-location').textContent = workspace.dataDir;
  drawTree();
}

function treeDragIds(event) {
  try {
    const value = JSON.parse(event.dataTransfer.getData('application/x-noteon-pages') || '[]');
    if (Array.isArray(value) && value.length) return value.map(Number);
  } catch {
    // Fall back to single text/page-id transfer type.
  }
  const id = Number(event.dataTransfer.getData('text/page-id'));
  return id ? [id] : [];
}

async function treeTransfer(pageIds, parentId, position, copy = false) {
  const wasActive = activeId;
  const refreshActive = !copy && wasActive && treeContainsPage(pageIds, wasActive);
  await beforeTreeMutation();
  await window.noteon.transferPages({ pageIds, parentId, position, copy });
  selectedTreePageIds.clear();
  await refresh();
  if (refreshActive && pages.some((page) => page.id === wasActive)) {
    await openPage(wasActive);
  }
}

async function treeReorder(source, target, after) {
  await beforeTreeMutation();
  await window.noteon.reorderPage({ source, target, after });
  selectedTreePageIds.clear();
  await refresh();
}

async function treeClipboard(copyCut) {
  const pageIds = [...selectedTreePageIds];
  if (!pageIds.length) return;
  await beforeTreeMutation();
  await window.noteon.copyPages({ pageIds, cut: copyCut });
}

function treeContainsPage(roots, pageId) {
  const rootSet = new Set(roots.map(Number));
  let page = pages.find((item) => item.id === Number(pageId));
  let guard = 0;
  while (page && guard++ < pages.length + 1) {
    if (rootSet.has(Number(page.id))) return true;
    page = page.parent_id === null ? null : pages.find((item) => item.id === Number(page.parent_id));
  }
  return false;
}

function closeTrashedOrDeletedTabs(roots) {
  const prevLen = openTabs.length;
  openTabs = openTabs.filter((tabId) => !treeContainsPage(roots, tabId));
  if (openTabs.length === prevLen) return;
  activeTabIndex = Math.max(0, Math.min(activeTabIndex, Math.max(0, openTabs.length - 1)));
  renderTabs();
  persistPrimaryDock().catch(() => {});
}

async function closeActiveTreePage(roots) {
  closeTrashedOrDeletedTabs(roots);
  if (!activeId || !treeContainsPage(roots, activeId)) {
    if (openTabs.length && !activeId) await openPage(openTabs[activeTabIndex], { skipTab: true });
    return;
  }
  if (openTabs.length) {
    await releaseActivePageLease();
    activeId = null;
    activePage = null;
    pageWriteAccess = false;
    $('#content').classList.remove('full-width');
    renderBreadcrumbs(null);
    await openPage(openTabs[activeTabIndex], { skipTab: true });
    return;
  }
  await resetActivePageDisplay();
}

function revealPageInTree(pageId) {
  let current = pages.find((item) => item.id === Number(pageId));
  let guard = 0;
  let changed = false;
  const seen = new Set();

  while (current && current.parent_id !== null && guard++ < pages.length + 1 && !seen.has(current.parent_id)) {
    seen.add(current.parent_id);
    if (!expandedPageIds.has(current.parent_id)) {
      expandedPageIds.add(current.parent_id);
      changed = true;
    }
    current = pages.find((item) => item.id === current.parent_id);
  }

  if (changed) {
    expandedTreeRevision++;
    window.noteon.setSetting({ key: 'electron_expanded', value: JSON.stringify([...expandedPageIds]) }).catch(() => {});
  }
}

function getTreeRowsCache() {
  if (
    treeRowsCache?.pages === pages &&
    treeRowsCache.expandedSet === expandedPageIds &&
    treeRowsCache.expandedRevision === expandedTreeRevision
  ) {
    return treeRowsCache;
  }

  const active = pages.filter((page) => page.trashed_by === null);
  const children = new Map();
  for (const page of active) {
    const key = page.parent_id ?? null;
    if (!children.has(key)) children.set(key, []);
    children.get(key).push(page);
  }
  for (const group of children.values()) {
    group.sort((a, b) => a.sort_order - b.sort_order || a.id - b.id);
  }

  const treeRows = [];
  const seen = new Set();
  function visit(parent, depth) {
    for (const page of children.get(parent) ?? []) {
      if (seen.has(page.id)) continue;
      seen.add(page.id);
      const descendants = children.get(page.id) ?? [];
      treeRows.push({ page, depth, expandable: descendants.length > 0 });
      if (descendants.length && expandedPageIds.has(page.id)) {
        visit(page.id, depth + 1);
      }
    }
  }

  visit(null, 0);
  const activeIds = new Set(active.map((page) => page.id));
  for (const page of active) {
    if (page.parent_id !== null && !activeIds.has(page.parent_id) && !seen.has(page.id)) {
      seen.add(page.id);
      const descendants = children.get(page.id) ?? [];
      treeRows.push({ page, depth: 0, expandable: descendants.length > 0 });
      if (expandedPageIds.has(page.id)) visit(page.id, 1);
    }
  }

  const favorites = pages
    .filter((page) => page.favorite && page.trashed_by === null)
    .sort((a, b) => a.sort_order - b.sort_order)
    .map((page) => ({ page, depth: 0, expandable: false }));

  const trash = pages
    .filter((page) => page.trashed_by === page.id)
    .sort((a, b) => b.updated_at - a.updated_at)
    .map((page) => ({ page, depth: 0, expandable: false }));

  treeRowsCache = {
    pages,
    expandedSet: expandedPageIds,
    expandedRevision: expandedTreeRevision,
    workspace: treeRows,
    workspaceIds: new Set(treeRows.map((item) => item.page.id)),
    favorites,
    favoriteIds: new Set(favorites.map((item) => item.page.id)),
    trash,
    trashIds: new Set(trash.map((item) => item.page.id)),
  };
  return treeRowsCache;
}

function currentVisibleTreeItems() {
  if (searchResults !== null) {
    return searchResults.map((page) => ({ page }));
  }
  const cache = getTreeRowsCache();
  return treeMode === 'favorites' ? cache.favorites : treeMode === 'trash' ? cache.trash : cache.workspace;
}

function buildTreeToolbar(list) {
  const toolbar = document.createElement('div');
  toolbar.className = 'tree-toolbar';
  if (!selectedTreePageIds.size) return toolbar;

  const count = document.createElement('span');
  count.textContent = `${selectedTreePageIds.size} selected`;
  toolbar.append(count);

  const addButton = (label, fn) => {
    const button = document.createElement('button');
    button.textContent = label;
    button.onclick = fn;
    toolbar.append(button);
  };

  if (treeMode === 'trash') {
    addButton('Restore', async () => {
      await beforeTreeMutation();
      await window.noteon.restorePages([...selectedTreePageIds]);
      selectedTreePageIds.clear();
      await refresh();
    });
    addButton('Delete', async () => {
      if (!confirm(`Permanently delete ${selectedTreePageIds.size} selected page(s) and their subpages?`)) return;
      const ids = [...selectedTreePageIds];
      await beforeTreeMutation();
      await window.noteon.deletePagesPermanently(ids);
      await closeActiveTreePage(ids);
      selectedTreePageIds.clear();
      await refresh();
    });
  } else {
    addButton('Copy', () => treeClipboard(false).catch(showDockError));
    addButton('Cut', () => treeClipboard(true).catch(showDockError));
    addButton('Move to trash', async () => {
      const ids = [...selectedTreePageIds];
      await beforeTreeMutation();
      await window.noteon.trashPages(ids);
      await closeActiveTreePage(ids);
      selectedTreePageIds.clear();
      await refresh();
    });
  }

  addButton('Clear', () => {
    selectedTreePageIds.clear();
    treeSelectionAnchorId = null;
    drawTree(list);
  });

  return toolbar;
}

function drawTree(list = searchResults) {
  $('#show-favorites')?.classList.toggle('active', treeMode === 'favorites' && list === null);
  $('#show-trash')?.classList.toggle('active', treeMode === 'trash' && list === null);

  const root = $('#tree');
  if (!root) return;

  const selectedId = currentDockPageId();
  const scrollPosition = root.scrollTop;
  const scrollTop = Math.max(0, scrollPosition - 32 - (treeMode === 'trash' ? 30 : 0));
  root.innerHTML = '';

  let items = [];
  let itemIds;
  if (list !== null) {
    items = list.map((page) => ({ page, depth: 0, expandable: false }));
    itemIds = new Set(items.map((item) => item.page.id));
  } else {
    const cache = getTreeRowsCache();
    if (treeMode === 'favorites') {
      items = cache.favorites;
      itemIds = cache.favoriteIds;
    } else if (treeMode === 'trash') {
      items = cache.trash;
      itemIds = cache.trashIds;
    } else {
      items = cache.workspace;
      itemIds = cache.workspaceIds;
    }
  }

  selectedTreePageIds = new Set([...selectedTreePageIds].filter((id) => itemIds.has(id)));
  root.append(buildTreeToolbar(list));

  root.ondragover = (event) => {
    const onRootOrSpacer = event.target === root || event.target.classList?.contains('tree-spacer');
    if (onRootOrSpacer && (event.dataTransfer.types.includes('application/x-noteon-pages') || event.dataTransfer.types.includes('text/page-id'))) {
      event.preventDefault();
    }
  };
  root.ondrop = async (event) => {
    if (!(event.target === root || event.target.classList?.contains('tree-spacer'))) return;
    event.preventDefault();
    const ids = treeDragIds(event);
    if (!ids.length) return;
    try {
      const copy = event.ctrlKey || event.altKey;
      const at = pages.filter((page) => page.parent_id === null && page.trashed_by === null && (copy || !ids.includes(page.id))).length;
      await treeTransfer(ids, null, at, copy);
    } catch (error) {
      showDockError(error);
    }
  };

  if (treeMode === 'trash') {
    const empty = document.createElement('button');
    empty.className = 'empty-trash';
    empty.textContent = 'Empty trash';
    empty.onclick = async () => {
      if (!confirm('Permanently delete every page in Trash?')) return;
      const trashedIds = pages.filter((p) => p.trashed_by !== null).map((p) => p.id);
      await beforeTreeMutation();
      await window.noteon.emptyTrash();
      await closeActiveTreePage(trashedIds);
      await refresh();
    };
    root.append(empty);
  }

  const rowHeight = list !== null ? 44 : 29;
  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - 5);
  const visibleCount = Math.ceil((root.clientHeight || 500) / rowHeight) + 10;
  const end = Math.min(items.length, start + visibleCount);
  const makeSpacer = (height) => {
    const node = document.createElement('div');
    node.className = 'tree-spacer';
    node.style.height = `${height}px`;
    return node;
  };

  if (start) root.append(makeSpacer(start * rowHeight));

  for (const { page, depth, expandable } of items.slice(start, end)) {
    const row = document.createElement('div');
    const multiSelected = selectedTreePageIds.has(page.id);
    const isSearchHit = list !== null;
    row.className =
      'tree-row' +
      (isSearchHit ? ' search-hit' : '') +
      (page.id === selectedId ? ' active' : '') +
      (multiSelected ? ' multi-selected' : '');
    row.dataset.pageId = page.id;
    row.style.paddingLeft = `${7 + depth * 13}px`;

    if (isSearchHit) {
      row.innerHTML = `<span class="tree-expand">·</span><span>${esc(page.icon || '📄')}</span><span class="search-hit-body"><span class="page-title">${esc(page.title || 'Untitled')}</span>${page.preview ? `<span class="search-hit-preview">${esc(page.preview)}</span>` : ''}</span><span class="row-action child-action" data-child="${page.id}" title="Add subpage">＋</span><span class="row-action" data-duplicate="${page.id}" title="Duplicate page">⧉</span><span class="row-action" data-fav="${page.id}" title="Favorite">${page.favorite ? '★' : '☆'}</span>`;
    } else {
      row.innerHTML = `<span class="tree-expand">${expandable ? (expandedPageIds.has(page.id) ? '⌄' : '›') : '·'}</span><span>${esc(page.icon || '📄')}</span><span class="page-title">${esc(page.title || 'Untitled')}</span><span class="row-action child-action" data-child="${page.id}" title="Add subpage">＋</span><span class="row-action" data-duplicate="${page.id}" title="Duplicate page">⧉</span><span class="row-action" data-fav="${page.id}" title="Favorite">${page.favorite ? '★' : '☆'}</span>`;
    }

    row.onclick = (event) => {
      if (event.ctrlKey || event.metaKey) {
        if (selectedTreePageIds.has(page.id)) selectedTreePageIds.delete(page.id);
        else selectedTreePageIds.add(page.id);
        treeSelectionAnchorId = page.id;
        drawTree(list);
        return;
      }
      if (event.shiftKey) {
        const anchor = items.findIndex((item) => item.page.id === treeSelectionAnchorId);
        const target = items.findIndex((item) => item.page.id === page.id);
        const from = anchor < 0 ? target : Math.min(anchor, target);
        const to = anchor < 0 ? target : Math.max(anchor, target);
        selectedTreePageIds = new Set(items.slice(from, to + 1).map((item) => item.page.id));
        drawTree(list);
        return;
      }
      selectedTreePageIds = new Set([page.id]);
      treeSelectionAnchorId = page.id;
      openPage(page.id, isSearchHit && Number.isInteger(page.position) ? { blockPosition: page.position } : {});
      drawTree(list);
    };

    row.draggable = treeMode === 'workspace' && searchResults === null;
    row.ondragstart = (event) => {
      const ids = selectedTreePageIds.has(page.id) ? [...selectedTreePageIds] : [page.id];
      event.dataTransfer.setData('application/x-noteon-pages', JSON.stringify(ids));
      event.dataTransfer.setData('text/page-id', String(page.id));
      event.dataTransfer.effectAllowed = 'copyMove';
    };
    row.ondragover = (event) => {
      if (event.dataTransfer.types.includes('application/x-noteon-pages') || event.dataTransfer.types.includes('text/page-id')) {
        event.preventDefault();
        row.classList.add('drop-target');
      }
    };
    row.ondragleave = () => row.classList.remove('drop-target');
    row.ondrop = async (event) => {
      event.stopPropagation();
      event.preventDefault();
      row.classList.remove('drop-target');
      const ids = treeDragIds(event);
      if (!ids.length || ids.includes(page.id)) return;

      const rect = row.getBoundingClientRect();
      const ratio = (event.clientY - rect.top) / rect.height;
      const copy = event.ctrlKey || event.altKey;
      let parentId;
      let position;

      if (ratio >= 0.28 && ratio <= 0.72) {
        parentId = page.id;
        position = pages.filter((item) => item.parent_id === parentId && item.trashed_by === null).length;
      } else {
        parentId = page.parent_id;
        const siblings = pages
          .filter((item) => item.parent_id === parentId && item.trashed_by === null && (copy || !ids.includes(item.id)))
          .sort((a, b) => a.sort_order - b.sort_order || a.id - b.id);
        const targetIndex = siblings.findIndex((item) => item.id === page.id);
        position = Math.max(0, targetIndex + (ratio > 0.72 ? 1 : 0));
      }

      try {
        const after = ratio > 0.72;
        if (!copy && ids.length === 1 && parentId === page.parent_id) {
          await treeReorder(ids[0], page.id, after);
        } else {
          await treeTransfer(ids, parentId, position, copy);
        }
      } catch (error) {
        showDockError(error);
      }
    };

    if (treeMode === 'trash') {
      row.querySelector('.child-action').hidden = true;
      row.querySelector('[data-duplicate]').hidden = true;
      row.querySelector('[data-fav]').hidden = true;

      const restoreBtn = document.createElement('button');
      restoreBtn.className = 'row-action trash-restore';
      restoreBtn.textContent = 'Restore';
      restoreBtn.onclick = async (event) => {
        event.stopPropagation();
        await beforeTreeMutation();
        await window.noteon.restorePage(page.id);
        await refresh();
      };

      const removeBtn = document.createElement('button');
      removeBtn.className = 'row-action trash-delete';
      removeBtn.textContent = 'Delete';
      removeBtn.title = 'Permanently delete';
      removeBtn.onclick = async (event) => {
        event.stopPropagation();
        if (!confirm('Permanently delete this page and its subpages?')) return;
        await beforeTreeMutation();
        await window.noteon.deletePagePermanently(page.id);
        await closeActiveTreePage([page.id]);
        await refresh();
      };
      row.append(restoreBtn, removeBtn);
    }

    row.onauxclick = (event) => {
      if (event.button === 1) {
        event.preventDefault();
        openPage(page.id, { newTab: true });
      }
    };
    row.querySelector('.tree-expand').onclick = (event) => {
      event.stopPropagation();
      if (expandedPageIds.has(page.id)) expandedPageIds.delete(page.id);
      else expandedPageIds.add(page.id);
      expandedTreeRevision++;
      window.noteon.setSetting({ key: 'electron_expanded', value: JSON.stringify([...expandedPageIds]) });
      drawTree(list);
    };
    row.querySelector('[data-fav]').onclick = async (event) => {
      event.stopPropagation();
      await window.noteon.favorite({ id: page.id, value: !page.favorite });
      await refresh();
    };
    row.querySelector('[data-duplicate]').onclick = async (event) => {
      event.stopPropagation();
      try {
        await beforeTreeMutation();
        const copy = await window.noteon.duplicatePage(page.id);
        await refresh();
        openPage(copy);
      } catch (error) {
        showDockError(error);
      }
    };
    row.querySelector('[data-child]').onclick = async (event) => {
      event.stopPropagation();
      try {
        await beforeTreeMutation();
        const id = await window.noteon.createPage({ parentId: page.id, title: '' });
        expandedPageIds.add(page.id);
        expandedTreeRevision++;
        await window.noteon.setSetting({ key: 'electron_expanded', value: JSON.stringify([...expandedPageIds]) });
        await refresh();
        await openPage(id);
        const titleInput = document.querySelector('.page-title-input');
        titleInput?.focus();
        titleInput?.select();
      } catch (error) {
        showDockError(error);
      }
    };

    root.append(row);
  }

  if (end < items.length) {
    root.append(makeSpacer((items.length - end) * rowHeight));
  }
  root.scrollTop = scrollPosition;
}

function showTreeRowContextMenu(event, page) {
  const actions = [
    ['Open in new tab', () => openPage(page.id, { newTab: true })],
    ['Open in new window', () => window.noteon.openPageWindow(page.id)],
  ];

  if (treeMode === 'trash') {
    actions.push(['Restore', async () => {
      await beforeTreeMutation();
      await window.noteon.restorePage(page.id);
      await refresh();
    }]);
    actions.push(['Delete permanently', async () => {
      if (!confirm('Permanently delete this page and its subpages?')) return;
      await beforeTreeMutation();
      await window.noteon.deletePagePermanently(page.id);
      await closeActiveTreePage([page.id]);
      await refresh();
    }]);
  } else {
    actions.push(['Rename', async () => {
      await openPage(page.id);
      const ti = document.querySelector('.page-title-input');
      ti?.focus();
      ti?.select();
    }]);
    actions.push(['Duplicate', async () => {
      await beforeTreeMutation();
      const copy = await window.noteon.duplicatePage(page.id);
      await refresh();
      await openPage(copy);
    }]);
    actions.push([
      page.favorite ? 'Remove from favorites' : 'Add to favorites',
      async () => {
        await window.noteon.favorite({ id: page.id, value: !page.favorite });
        await refresh();
      },
    ]);
    actions.push(['Move to trash', async () => {
      await beforeTreeMutation();
      await window.noteon.trashPage(page.id);
      await closeActiveTreePage([page.id]);
      await refresh();
    }]);
  }

  actions.push(['Customize sidebar theme…', () => openSettingsDialog('sidebar')]);
  openFloatingContextMenu(event, actions);
}

function initSidebarTree() {
  $('#show-favorites').onclick = () => {
    treeMode = treeMode === 'favorites' ? 'workspace' : 'favorites';
    searchResults = null;
    drawTree(null);
  };
  $('#show-trash').onclick = () => {
    treeMode = treeMode === 'trash' ? 'workspace' : 'trash';
    searchResults = null;
    drawTree(null);
  };
  $('#new-page').onclick = async () => {
    const id = await window.noteon.createPage({ title: '' });
    await refresh();
    await openPage(id);
    const titleInput = document.querySelector('.page-title-input');
    titleInput?.focus();
    titleInput?.select();
  };
  $('#new-database').onclick = async () => {
    const id = await window.noteon.createPage({ title: 'New database', database: true });
    await refresh();
    await openPage(id);
    const titleInput = document.querySelector('.page-title-input');
    titleInput?.focus();
    titleInput?.select();
  };

  $('#search').oninput = (event) => {
    clearTimeout(searchTimer);
    const q = event.target.value.trim();
    searchTimer = setTimeout(async () => {
      searchResults = q ? await window.noteon.search(q) : null;
      drawTree(searchResults);
    }, 140);
  };
  $('#search').onkeydown = (event) => {
    if (event.key === 'Enter' && searchResults?.length) {
      event.preventDefault();
      const first = searchResults[0];
      openPage(first.id, Number.isInteger(first.position) ? { blockPosition: first.position } : {});
    } else if (event.key === 'Escape') {
      event.preventDefault();
      event.target.value = '';
      searchResults = null;
      drawTree(null);
      event.target.blur();
    }
  };

  $('#tree').addEventListener(
    'scroll',
    () => {
      cancelAnimationFrame(treeScrollFrame);
      treeScrollFrame = requestAnimationFrame(() => drawTree(searchResults));
    },
    { passive: true },
  );
}
