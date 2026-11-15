const propertyKinds = new Set([
  'text',
  'number',
  'select',
  'multi-select',
  'status',
  'date',
  'person',
  'files',
  'checkbox',
  'url',
  'phone',
  'email',
  'relation',
  'rollup',
  'formula',
  'button',
  'id',
  'place',
  'created_time',
  'last_edited_time',
  'created_by',
  'last_edited_by',
]);

function utf8ByteLength(value) {
  return Buffer.byteLength(String(value), 'utf8');
}

function readPropertyOptions(raw = '') {
  try {
    const value = JSON.parse(raw);
    if (Array.isArray(value)) {
      return value
        .map((item) => (typeof item === 'string' ? { name: item, color: null } : item))
        .filter((item) => item?.name);
    }
  } catch {}

  return String(raw)
    .split(/\r?\n/)
    .map((line) => {
      const trimmed = line.trim();
      if (!trimmed) return null;
      const separator = trimmed.lastIndexOf('::');
      if (separator < 0) return { name: trimmed, color: null };
      const name = trimmed.slice(0, separator).trim();
      const candidate = trimmed.slice(separator + 2).trim();
      if (name && /^#?[0-9a-f]{6}$/i.test(candidate)) {
        return { name, color: `#${candidate.replace(/^#/, '').toUpperCase()}` };
      }
      return { name: trimmed, color: null };
    })
    .filter(Boolean);
}

function validatePropertyName(name) {
  name = String(name || '').trim();
  if (!name || utf8ByteLength(name) > 80) {
    throw new Error('Property names must be 1–80 characters long.');
  }
  return name;
}

function validatePropertyValue(kind, options, value) {
  if (
    kind === 'number' &&
    value !== '' &&
    (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value) || !Number.isFinite(Number(value)))
  ) {
    throw new Error('Enter a valid number.');
  }
  if (kind === 'date' && value) {
    const date = new Date(`${value}T00:00:00Z`);
    if (
      !/^\d{4}-\d{2}-\d{2}$/.test(value) ||
      Number.isNaN(date.getTime()) ||
      date.toISOString().slice(0, 10) !== value
    ) {
      throw new Error('Enter a valid ISO date.');
    }
  }
  if (kind === 'checkbox' && !['true', 'false'].includes(value)) {
    throw new Error('Invalid checkbox value.');
  }
  if (['select', 'status', 'multi-select'].includes(kind)) {
    const names = readPropertyOptions(options).map((option) => option.name);
    const chosen = kind === 'multi-select' ? value.split('|').filter(Boolean) : value ? [value] : [];
    if (chosen.some((name) => !names.includes(name))) {
      throw new Error('Choose a value from the available options.');
    }
  }
}

function serializePropertyOptions(options) {
  return options
    .map((option) => {
      const name = option.name.trim();
      const rawColor = String(option.color || '');
      const color = /^#[0-9a-f]{6}$/i.test(rawColor) ? rawColor.toUpperCase() : '';
      return `${name}${color ? '::' + color : ''}`;
    })
    .join('\n');
}

function normalizePropertyOptions(raw, kind = 'text', defaultStatus = true) {
  let options = Array.isArray(raw) ? raw : readPropertyOptions(raw);
  options = options
    .map((option) => {
      if (typeof option === 'string') return { name: option.trim(), color: null };
      const rawColor = String(option.color || '').trim();
      const color = /^#?[0-9a-f]{6}$/i.test(rawColor)
        ? `#${rawColor.replace(/^#/, '').toUpperCase()}`
        : null;
      return { name: String(option.name || '').trim(), color };
    })
    .filter((option) => option.name)
    .filter((option, index, list) => list.findIndex((other) => other.name === option.name) === index);

  if (defaultStatus && kind === 'status' && !options.length) {
    options = ['To do', 'In progress', 'Done'].map((name) => ({ name, color: null }));
  }
  if (
    options.length > 100 ||
    options.some((option) => utf8ByteLength(option.name) > 80 || /[\r\n]/.test(option.name))
  ) {
    throw new Error('Options must contain up to 100 names of at most 80 characters.');
  }
  return serializePropertyOptions(options);
}

module.exports = {
  utf8ByteLength,
  propertyKinds,
  readPropertyOptions,
  validatePropertyName,
  validatePropertyValue,
  serializePropertyOptions,
  normalizePropertyOptions,
};
