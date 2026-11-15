/* Dock layout operations are independent from the renderer and the workspace database. */
(function (root, factory) {
  if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.NoteonDock = factory();
  }
})(globalThis, function () {
  'use strict';

  const VALID_DIRECTIONS = new Set(['left', 'right', 'top', 'bottom']);

  const createLeaf = (id, tabs = [], active = 0) => ({
    type: 'leaf',
    id,
    tabs: [...tabs],
    active,
  });

  const cloneLayout = (layout) => structuredClone(layout);

  /**
   * Find a leaf or split node by ID within a dock tree.
   * @param {object} node
   * @param {string} id
   * @returns {object|null}
   */
  function find(node, id) {
    if (node.type === 'leaf') {
      return node.id === id ? node : null;
    }
    return find(node.first, id) || find(node.second, id);
  }

  /**
   * Collect all leaf panes in tree traversal order.
   * @param {object} node
   * @param {object[]} [result]
   * @returns {object[]}
   */
  function leaves(node, result = []) {
    if (node.type === 'leaf') {
      result.push(node);
    } else {
      leaves(node.first, result);
      leaves(node.second, result);
    }
    return result;
  }

  function replaceNode(node, id, value) {
    if (node.type === 'leaf') {
      return node.id === id ? value : node;
    }
    return {
      ...node,
      first: replaceNode(node.first, id, value),
      second: replaceNode(node.second, id, value),
    };
  }

  function removeNode(node, id) {
    if (node.type === 'leaf') {
      return node.id === id ? null : node;
    }
    const first = removeNode(node.first, id);
    const second = removeNode(node.second, id);
    if (!first) return second;
    if (!second) return first;
    return { ...node, first, second };
  }

  /**
   * Validate a complete dock layout object.
   * @param {unknown} layout
   * @returns {boolean}
   */
  function validate(layout) {
    if (!layout || !layout.root || typeof layout.focused !== 'string') {
      return false;
    }
    const ids = new Set();

    function checkNode(node) {
      if (!node || typeof node !== 'object') return false;
      if (node.type === 'leaf') {
        const validLeaf =
          typeof node.id === 'string' &&
          Boolean(node.id) &&
          !ids.has(node.id) &&
          Array.isArray(node.tabs) &&
          Number.isInteger(node.active) &&
          node.active >= 0 &&
          node.active < Math.max(node.tabs.length, 1) &&
          node.tabs.every((tabId) => Number.isSafeInteger(tabId) && tabId > 0);
        if (!validLeaf) return false;
        ids.add(node.id);
        return true;
      }

      return (
        node.type === 'split' &&
        ['horizontal', 'vertical'].includes(node.axis) &&
        Number.isFinite(node.ratio) &&
        node.ratio >= 0.1 &&
        node.ratio <= 0.9 &&
        checkNode(node.first) &&
        checkNode(node.second)
      );
    }

    return checkNode(layout.root) && ids.has(layout.focused);
  }

  function requireLayout(layout) {
    if (!validate(layout)) {
      throw new Error('Invalid dock layout');
    }
  }

  function create(id = 'pane-1', tabs = []) {
    const result = { root: createLeaf(id, tabs), focused: id };
    requireLayout(result);
    return result;
  }

  function split(layout, targetId, newId, direction, pageId) {
    requireLayout(layout);
    if (
      !VALID_DIRECTIONS.has(direction) ||
      !find(layout.root, targetId) ||
      find(layout.root, newId) ||
      !Number.isSafeInteger(pageId) ||
      pageId <= 0
    ) {
      throw new Error('Invalid split');
    }

    const out = cloneLayout(layout);
    const oldPane = find(out.root, targetId);
    const nextPane = createLeaf(newId, [pageId]);
    const insertBefore = direction === 'left' || direction === 'top';
    const branch = {
      type: 'split',
      axis: ['left', 'right'].includes(direction) ? 'horizontal' : 'vertical',
      ratio: 0.5,
      first: insertBefore ? nextPane : oldPane,
      second: insertBefore ? oldPane : nextPane,
    };

    out.root = replaceNode(out.root, targetId, branch);
    out.focused = newId;
    return out;
  }

  function closePane(layout, id) {
    requireLayout(layout);
    if (!find(layout.root, id)) {
      throw new Error('Unknown pane');
    }
    if (layout.root.type === 'leaf') {
      return layout;
    }

    const out = cloneLayout(layout);
    out.root = removeNode(out.root, id);
    if (out.focused === id) {
      out.focused = leaves(out.root)[0].id;
    }
    return out;
  }

  function setActive(layout, paneId, index) {
    requireLayout(layout);
    const out = cloneLayout(layout);
    const pane = find(out.root, paneId);
    if (!pane || !Number.isInteger(index) || index < 0 || index >= pane.tabs.length) {
      throw new Error('Invalid tab');
    }
    pane.active = index;
    out.focused = paneId;
    return out;
  }

  function moveTab(layout, fromId, index, toId, toIndex) {
    requireLayout(layout);
    const out = cloneLayout(layout);
    const from = find(out.root, fromId);
    const to = find(out.root, toId);
    if (
      !from ||
      !to ||
      !Number.isInteger(index) ||
      index < 0 ||
      index >= from.tabs.length ||
      !Number.isInteger(toIndex) ||
      toIndex < 0 ||
      toIndex > to.tabs.length
    ) {
      throw new Error('Invalid tab move');
    }

    const [page] = from.tabs.splice(index, 1);
    if (from === to && index < toIndex) {
      toIndex--;
    }
    to.tabs.splice(toIndex, 0, page);
    if (from !== to) {
      from.active = Math.max(0, Math.min(from.active - (index < from.active ? 1 : 0), from.tabs.length - 1));
    }
    to.active = toIndex;
    out.focused = toId;
    return out;
  }

  function openTab(layout, paneId, pageId, options = {}) {
    requireLayout(layout);
    if (!Number.isSafeInteger(pageId) || pageId <= 0) {
      throw new Error('Invalid page');
    }

    const out = cloneLayout(layout);
    const pane = find(out.root, paneId);
    if (!pane) {
      throw new Error('Unknown pane');
    }

    const existing = pane.tabs.indexOf(pageId);
    if (existing >= 0 && !options.duplicate) {
      pane.active = existing;
    } else if (options.replace && pane.tabs.length) {
      pane.tabs[pane.active] = pageId;
    } else {
      const index = pane.tabs.length ? pane.active + 1 : 0;
      pane.tabs.splice(index, 0, pageId);
      pane.active = index;
    }

    out.focused = paneId;
    return out;
  }

  function closeTab(layout, paneId, index) {
    requireLayout(layout);
    const out = cloneLayout(layout);
    const pane = find(out.root, paneId);
    if (!pane || !Number.isInteger(index) || index < 0 || index >= pane.tabs.length) {
      throw new Error('Invalid tab');
    }

    pane.tabs.splice(index, 1);
    if (index < pane.active) {
      pane.active--;
    } else if (pane.active >= pane.tabs.length) {
      pane.active = Math.max(0, pane.tabs.length - 1);
    }
    out.focused = paneId;
    return out;
  }

  function restore(raw, validPages) {
    let layout;
    try {
      layout = typeof raw === 'string' ? JSON.parse(raw) : cloneLayout(raw);
    } catch {
      return null;
    }
    if (!validate(layout)) return null;

    const allowed = new Set(validPages);
    for (const pane of leaves(layout.root)) {
      const selected = pane.tabs[pane.active];
      pane.tabs = pane.tabs.filter((id) => allowed.has(id));
      const selectedIndex = pane.tabs.indexOf(selected);
      pane.active = selectedIndex >= 0 ? selectedIndex : Math.max(0, Math.min(pane.active, pane.tabs.length - 1));
    }
    return validate(layout) ? layout : null;
  }

  function focus(layout, paneId) {
    requireLayout(layout);
    if (!find(layout.root, paneId)) {
      throw new Error('Unknown pane');
    }
    const out = cloneLayout(layout);
    out.focused = paneId;
    return out;
  }

  function resize(layout, path, ratio) {
    requireLayout(layout);
    if (!Array.isArray(path) || !Number.isFinite(ratio)) {
      throw new Error('Invalid resize');
    }

    const out = cloneLayout(layout);
    let node = out.root;
    for (const part of path) {
      if (node.type !== 'split' || !['first', 'second'].includes(part)) {
        throw new Error('Invalid split path');
      }
      node = node[part];
    }
    if (node.type !== 'split') {
      throw new Error('Invalid split path');
    }
    node.ratio = Math.max(0.1, Math.min(0.9, ratio));
    return out;
  }

  return {
    create,
    validate,
    find,
    leaves,
    split,
    closePane,
    setActive,
    moveTab,
    resize,
    openTab,
    closeTab,
    restore,
    focus,
  };
});
