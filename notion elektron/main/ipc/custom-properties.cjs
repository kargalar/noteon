const { state, rows, one } = require('../state.cjs');
const { assertPageWritable } = require('../services/leases.cjs');
const { recordPageUndo } = require('../services/history.cjs');
const {
  utf8ByteLength,
  propertyKinds,
  readPropertyOptions,
  validatePropertyName,
  validatePropertyValue,
  serializePropertyOptions,
  normalizePropertyOptions,
} = require('../utils/property-utils.cjs');

function registerCustomPropertyIpc(ipcMain) {
  ipcMain.handle('custom:property:add', (event, { pageId, name, kind = 'text', options = [] }) => {
    assertPageWritable(event.sender, pageId);
    name = validatePropertyName(name);
    if (!propertyKinds.has(kind)) throw new Error('Unsupported property type.');
    const rawOptions = normalizePropertyOptions(options, kind);
    recordPageUndo(pageId, 'Sayfa özelliği eklendi', true);
    const pos = one('SELECT coalesce(max(position)+1,0) n FROM custom_page_properties WHERE page_id=?', pageId).n;
    return state.db
      .prepare('INSERT INTO custom_page_properties(page_id,name,kind,options,position) VALUES(?,?,?,?,?)')
      .run(pageId, name, kind, rawOptions, pos).lastInsertRowid;
  });

  ipcMain.handle('custom:property:update', (event, { pageId, propertyId, name, kind, options }) =>
    state.db.transaction(() => {
      assertPageWritable(event.sender, pageId);
      const property = one(
        'SELECT * FROM custom_page_properties WHERE page_id=? AND id=?',
        pageId,
        propertyId,
      );
      if (!property) throw new Error('Page property not found.');

      const nextName = name === undefined ? property.name : validatePropertyName(name);
      const nextKind = kind === undefined ? property.kind : String(kind);
      const kindChanged = nextKind !== property.kind;
      const typeRequested = kind !== undefined;
      if (!propertyKinds.has(nextKind)) throw new Error('Unsupported property type.');

      let rawOptions = property.options;
      if (options !== undefined) {
        rawOptions = normalizePropertyOptions(options, nextKind, kindChanged && nextKind === 'status');
      } else if (typeRequested && nextKind === 'status' && !String(rawOptions || '').trim()) {
        rawOptions = serializePropertyOptions(['To do', 'In progress', 'Done'].map((opt) => ({ name: opt, color: null })));
      }

      recordPageUndo(pageId, 'Sayfa özelliği güncellendi', true);
      state.db
        .prepare('UPDATE custom_page_properties SET name=?,kind=?,options=? WHERE page_id=? AND id=?')
        .run(nextName, nextKind, rawOptions, pageId, propertyId);

      if (typeRequested || options !== undefined) {
        const current = one(
          'SELECT value FROM custom_page_property_values WHERE page_id=? AND property_id=?',
          pageId,
          propertyId,
        );
        if (current?.value) {
          if (nextKind === 'multi-select' && ['multi-select', 'select', 'status'].includes(property.kind)) {
            const allowedOpts = new Set(readPropertyOptions(rawOptions).map((o) => o.name));
            const filtered = current.value
              .split('|')
              .filter((v) => v && allowedOpts.has(v))
              .join('|');
            if (filtered) {
              state.db
                .prepare('UPDATE custom_page_property_values SET value=? WHERE page_id=? AND property_id=?')
                .run(filtered, pageId, propertyId);
            } else {
              state.db
                .prepare('DELETE FROM custom_page_property_values WHERE page_id=? AND property_id=?')
                .run(pageId, propertyId);
            }
          } else {
            try {
              validatePropertyValue(nextKind, rawOptions, current.value);
            } catch {
              state.db
                .prepare('DELETE FROM custom_page_property_values WHERE page_id=? AND property_id=?')
                .run(pageId, propertyId);
            }
          }
        }
      }

      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return true;
    })(),
  );

  ipcMain.handle('custom:property:option:rename', (event, { pageId, propertyId, oldName, newName }) =>
    state.db.transaction(() => {
      assertPageWritable(event.sender, pageId);
      const property = one(
        'SELECT kind,options FROM custom_page_properties WHERE page_id=? AND id=?',
        pageId,
        propertyId,
      );
      if (!property) throw new Error('Page property not found.');

      oldName = String(oldName || '').trim();
      newName = String(newName || '').trim();
      if (!newName || utf8ByteLength(newName) > 80 || /[\r\n]/.test(newName)) {
        throw new Error('Option names must be 1–80 characters long.');
      }

      recordPageUndo(pageId, 'Özellik seçeneği yeniden adlandırıldı', true);
      const options = readPropertyOptions(property.options);
      const found = options.find((item) => item.name === oldName);
      if (found) found.name = newName;
      else options.push({ name: newName, color: null });

      const unique = options.filter(
        (item, index, list) => list.findIndex((other) => other.name === item.name) === index,
      );
      state.db
        .prepare('UPDATE custom_page_properties SET options=? WHERE page_id=? AND id=?')
        .run(serializePropertyOptions(unique), pageId, propertyId);

      const current = one(
        'SELECT value FROM custom_page_property_values WHERE page_id=? AND property_id=?',
        pageId,
        propertyId,
      );
      if (current && oldName !== newName) {
        let value = current.value;
        if (property.kind === 'multi-select') {
          const parts = value.split('|').filter(Boolean);
          if (parts.includes(oldName)) {
            value = [...new Set(parts.map((item) => (item === oldName ? newName : item)))].join('|');
          }
        } else if (value === oldName) {
          value = newName;
        }
        if (value !== current.value) {
          state.db
            .prepare('UPDATE custom_page_property_values SET value=? WHERE page_id=? AND property_id=?')
            .run(value, pageId, propertyId);
        }
      }
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return true;
    })(),
  );

  ipcMain.handle('custom:property:option:remove', (event, { pageId, propertyId, name }) =>
    state.db.transaction(() => {
      assertPageWritable(event.sender, pageId);
      const property = one(
        'SELECT kind,options FROM custom_page_properties WHERE page_id=? AND id=?',
        pageId,
        propertyId,
      );
      if (!property) throw new Error('Page property not found.');

      name = String(name || '').trim();
      recordPageUndo(pageId, 'Özellik seçeneği kaldırıldı', true);
      const options = readPropertyOptions(property.options).filter((item) => item.name !== name);
      state.db
        .prepare('UPDATE custom_page_properties SET options=? WHERE page_id=? AND id=?')
        .run(serializePropertyOptions(options), pageId, propertyId);

      const current = one(
        'SELECT value FROM custom_page_property_values WHERE page_id=? AND property_id=?',
        pageId,
        propertyId,
      );
      if (current) {
        const value =
          property.kind === 'multi-select'
            ? current.value
                .split('|')
                .filter((item) => item && item !== name)
                .join('|')
            : current.value === name
              ? ''
              : current.value;
        if (value !== current.value) {
          state.db
            .prepare('UPDATE custom_page_property_values SET value=? WHERE page_id=? AND property_id=?')
            .run(value, pageId, propertyId);
        }
      }
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return true;
    })(),
  );

  ipcMain.handle('custom:property:move', (event, { pageId, propertyId, direction }) =>
    state.db.transaction(() => {
      assertPageWritable(event.sender, pageId);
      const ids = rows(
        'SELECT id FROM custom_page_properties WHERE page_id=? ORDER BY position,id',
        pageId,
      ).map((row) => Number(row.id));
      const from = ids.indexOf(Number(propertyId));
      const delta = Math.sign(Number(direction));
      const to = from + delta;
      if (from < 0) throw new Error('Page property not found.');
      if (!delta || to < 0 || to >= ids.length) return true;

      recordPageUndo(pageId, 'Sayfa özellikleri sıralandı', true);
      [ids[from], ids[to]] = [ids[to], ids[from]];
      const update = state.db.prepare('UPDATE custom_page_properties SET position=? WHERE page_id=? AND id=?');
      ids.forEach((id, index) => update.run(index, pageId, id));
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return true;
    })(),
  );

  ipcMain.handle('custom:property:delete', (event, { pageId, propertyId }) => {
    assertPageWritable(event.sender, pageId);
    recordPageUndo(pageId, 'Sayfa özelliği silindi', true);
    const changes = state.db
      .prepare('DELETE FROM custom_page_properties WHERE page_id=? AND id=?')
      .run(pageId, propertyId).changes;
    if (changes) state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
    return changes;
  });

  ipcMain.handle('custom:value', (event, { pageId, propertyId, value }) => {
    assertPageWritable(event.sender, pageId);
    const property = one(
      'SELECT kind,options FROM custom_page_properties WHERE page_id=? AND id=?',
      pageId,
      propertyId,
    );
    if (!property) throw new Error('Page property not found.');
    if (['created_time', 'last_edited_time'].includes(property.kind)) {
      throw new Error('Generated time properties are read-only.');
    }

    value = String(value ?? '');
    validatePropertyValue(property.kind, property.options, value);
    const previous =
      one(
        'SELECT value FROM custom_page_property_values WHERE page_id=? AND property_id=?',
        pageId,
        propertyId,
      )?.value ?? '';
    if (previous === value) return 0;

    recordPageUndo(pageId, 'Özellik düzenlendi', true);
    return state.db.transaction(() => {
      const changes = state.db
        .prepare(
          'INSERT INTO custom_page_property_values(page_id,property_id,value) VALUES(?,?,?) ON CONFLICT(page_id,property_id) DO UPDATE SET value=excluded.value',
        )
        .run(pageId, propertyId, value).changes;
      state.db.prepare('UPDATE pages SET updated_at=unixepoch() WHERE id=?').run(pageId);
      return changes;
    })();
  });
}

module.exports = {
  registerCustomPropertyIpc,
};
