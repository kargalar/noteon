/* Builds a dock tree while allowing the caller to retain each pane's editor DOM. */
(function (root, factory) {
  if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.NoteonDockView = factory();
  }
})(globalThis, function () {
  'use strict';

  /**
   * Wire drag and keyboard resizing handlers onto a split divider element.
   * @param {HTMLElement} divider
   * @param {HTMLElement} group
   * @param {object} node
   * @param {string[]} path
   * @param {Function} onResize
   */
  function attachDividerHandlers(divider, group, node, path, onResize) {
    divider.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      event.preventDefault();

      const rect = group.getBoundingClientRect();
      const horizontal = node.axis === 'horizontal';
      const start = horizontal ? rect.left : rect.top;
      const size = horizontal ? rect.width : rect.height;
      if (!size) return;

      let current = node.ratio;
      divider.setPointerCapture(event.pointerId);

      const handleMove = (moveEvent) => {
        current = Math.max(0.1, Math.min(0.9, ((horizontal ? moveEvent.clientX : moveEvent.clientY) - start) / size));
        group.style.setProperty('--dock-ratio', String(current));
      };

      const cleanup = () => {
        divider.removeEventListener('pointermove', handleMove);
        divider.removeEventListener('pointerup', handleFinish);
        divider.removeEventListener('pointercancel', handleCancel);
      };

      const handleFinish = () => {
        cleanup();
        onResize(path, current);
      };

      const handleCancel = () => {
        cleanup();
        group.style.setProperty('--dock-ratio', String(node.ratio));
      };

      divider.addEventListener('pointermove', handleMove);
      divider.addEventListener('pointerup', handleFinish, { once: true });
      divider.addEventListener('pointercancel', handleCancel, { once: true });
    });

    divider.addEventListener('keydown', (event) => {
      const horizontal = node.axis === 'horizontal';
      const decreaseKey = horizontal ? 'ArrowLeft' : 'ArrowUp';
      const increaseKey = horizontal ? 'ArrowRight' : 'ArrowDown';
      if (event.key !== decreaseKey && event.key !== increaseKey) return;

      event.preventDefault();
      const step = event.key === increaseKey ? 0.05 : -0.05;
      onResize(path, Math.max(0.1, Math.min(0.9, node.ratio + step)));
    });
  }

  /**
   * Render a dock layout tree into `container`, calling `options.pane(leaf)` for each leaf pane.
   * @param {HTMLElement} container
   * @param {object} layout
   * @param {{ pane: Function, resize: Function }} options
   * @returns {HTMLElement}
   */
  function mount(container, layout, options) {
    if (!container || !layout || !options || typeof options.pane !== 'function' || typeof options.resize !== 'function') {
      throw new Error('Invalid dock view arguments');
    }

    const doc = container.ownerDocument;
    const usedPanes = new Set();

    function buildNode(node, path) {
      if (node.type === 'leaf') {
        const pane = options.pane(node);
        if (!pane || usedPanes.has(pane)) {
          throw new Error('Pane renderer must return a distinct element');
        }
        usedPanes.add(pane);
        pane.classList.add('dock-pane');
        pane.dataset.paneId = node.id;
        return pane;
      }

      const group = doc.createElement('div');
      group.className = `dock-split dock-${node.axis}`;
      group.style.setProperty('--dock-ratio', String(node.ratio));

      const first = doc.createElement('div');
      first.className = 'dock-child';
      first.append(buildNode(node.first, path.concat('first')));

      const second = doc.createElement('div');
      second.className = 'dock-child';
      second.append(buildNode(node.second, path.concat('second')));

      const divider = doc.createElement('div');
      divider.className = 'dock-divider';
      divider.setAttribute('role', 'separator');
      divider.setAttribute('aria-orientation', node.axis === 'horizontal' ? 'vertical' : 'horizontal');
      divider.tabIndex = 0;

      group.append(first, divider, second);
      attachDividerHandlers(divider, group, node, path, options.resize);
      return group;
    }

    const tree = buildNode(layout.root, []);
    container.replaceChildren(tree);
    return tree;
  }

  return { mount };
});
