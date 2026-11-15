const path = require('node:path');

const INLINE_TOKENS = [
  ['***', { bold: true, italic: true }],
  ['**', { bold: true }],
  ['__', { bold: true }],
  ['~~', { strikethrough: true }],
  ['`', { font_family: 'Monospace' }],
  ['*', { italic: true }],
];

function parseClipboardInlineMarkdown(source) {
  const parse = (text) => {
    const chars = Array.from(text);
    const plain = [];
    const styles = [];
    let index = 0;

    while (index < chars.length) {
      let found = null;
      for (const [marker, style] of INLINE_TOKENS) {
        const mark = Array.from(marker);
        const after = index + mark.length;
        const startsWithMark = chars
          .slice(index, index + mark.length)
          .every((value, offset) => value === mark[offset]);
        if (!startsWithMark || after >= chars.length || chars[after] === mark[0] || /\s/u.test(chars[after])) {
          continue;
        }
        let end = -1;
        for (let cursor = after; cursor + mark.length <= chars.length; cursor++) {
          const closes = chars
            .slice(cursor, cursor + mark.length)
            .every((value, offset) => value === mark[offset]);
          if (closes && !/\s/u.test(chars[cursor - 1] || '')) {
            end = cursor;
            break;
          }
        }
        if (end >= 0) {
          found = { mark, style, after, end };
          break;
        }
      }

      if (!found) {
        plain.push(chars[index++]);
        continue;
      }

      const inner = chars.slice(found.after, found.end).join('');
      const nested = found.style.font_family === 'Monospace' ? { text: inner, ranges: [] } : parse(inner);
      const start = plain.length;
      plain.push(...Array.from(nested.text));
      if (plain.length > start) {
        styles.push(
          { start, end: plain.length, style: found.style },
          ...nested.ranges.map((range) => ({
            start: range.start + start,
            end: range.end + start,
            style: { ...found.style, ...range.style },
          })),
        );
      }
      index = found.end + found.mark.length;
    }

    return { text: plain.join(''), ranges: styles };
  };

  return parse(String(source || ''));
}

function parseClipboardMarkdown(source) {
  const blocks = [];
  let code = null;
  let opening = 0;

  for (const line of String(source || '').replace(/\r\n|\r/g, '\n').split('\n')) {
    const fence = (line.match(/^`+/u) || [''])[0].length;
    if (code !== null) {
      if (fence >= opening && !line.slice(fence).trim()) {
        blocks.push({ kind: 'code', content: code.join('\n'), checked: false, media_width: 1, text_styles: '[]' });
        code = null;
      } else {
        code.push(line);
      }
      continue;
    }
    if (fence >= 3) {
      code = [];
      opening = fence;
      continue;
    }
    if (!line.trim()) continue;

    let kind = 'text';
    let content = line;
    let checked = false;
    let match;

    if ((match = line.match(/^- \[([xX ])\] (.*)$/u))) {
      kind = 'todo';
      checked = match[1].toLowerCase() === 'x';
      content = match[2];
    } else if ((match = line.match(/^### (.*)$/u))) {
      kind = 'h3';
      content = match[1];
    } else if ((match = line.match(/^## (.*)$/u))) {
      kind = 'h2';
      content = match[1];
    } else if ((match = line.match(/^# (.*)$/u))) {
      kind = 'h1';
      content = match[1];
    } else if (line.trim() === '---') {
      kind = 'divider';
      content = '';
    } else if ((match = line.match(/^> \[!NOTE\] (.*)$/u))) {
      kind = 'callout';
      content = match[1];
    } else if ((match = line.match(/^> (.*)$/u))) {
      kind = 'quote';
      content = match[1];
    } else if ((match = line.match(/^(?:- |\* )(.*)$/u))) {
      kind = 'bullet';
      content = match[1];
    } else if ((match = line.match(/^([0-9]+)\. (.*)$/u))) {
      kind = 'numbered';
      content = match[2];
    }

    const parsed =
      kind === 'code' || kind === 'divider'
        ? { text: content, ranges: [] }
        : parseClipboardInlineMarkdown(content);
    blocks.push({
      kind,
      content: parsed.text,
      checked,
      media_width: 1,
      text_styles: JSON.stringify(parsed.ranges),
    });
  }

  if (code !== null) {
    blocks.push({ kind: 'code', content: code.join('\n'), checked: false, media_width: 1, text_styles: '[]' });
  }
  return blocks;
}

function clipboardBlockText(block) {
  if (block.kind === 'database') return `[Database](noteon://page/${block.content})`;
  if (block.kind === 'page_link') return `[Page](noteon://page/${block.content})`;
  return block.content;
}

function parseMarkdownImport(source, filePath) {
  const normalized = String(source).replace(/\r\n/g, '\n').replace(/\r/g, '\n');
  const lines = normalized.split('\n');
  if (normalized.endsWith('\n')) lines.pop();

  let title = '';
  const blocks = [];
  let code = null;
  let fenceLength = 0;

  for (const line of lines) {
    const openingLength = (line.match(/^`+/) || [''])[0].length;
    if (code !== null) {
      if (openingLength >= fenceLength && !line.slice(openingLength).trim()) {
        blocks.push({ kind: 'code', content: code.join('\n'), checked: 0 });
        code = null;
      } else {
        code.push(line);
      }
      continue;
    }
    if (openingLength >= 3) {
      fenceLength = openingLength;
      code = [];
      continue;
    }
    if (blocks.length === 0 && title === '' && line.startsWith('# ')) {
      title = line.slice(2);
      continue;
    }
    if (!line.trim()) continue;

    let kind = 'text';
    let content = line;
    let checked = 0;

    if (line.startsWith('- [x] ') || line.startsWith('- [X] ')) {
      kind = 'todo';
      checked = 1;
      content = line.slice(6);
    } else if (line.startsWith('- [ ] ')) {
      kind = 'todo';
      content = line.slice(6);
    } else if (line.startsWith('### ')) {
      kind = 'h3';
      content = line.slice(4);
    } else if (line.startsWith('## ')) {
      kind = 'h2';
      content = line.slice(3);
    } else if (line.startsWith('# ')) {
      kind = 'h1';
      content = line.slice(2);
    } else if (line.trim() === '---') {
      kind = 'divider';
      content = '';
    } else if (line.startsWith('> [!NOTE] ')) {
      kind = 'callout';
      content = line.slice(10);
    } else if (line.startsWith('> ')) {
      kind = 'quote';
      content = line.slice(2);
    } else if (line.startsWith('- ') || line.startsWith('* ')) {
      kind = 'bullet';
      content = line.slice(2);
    } else {
      const numbered = line.match(/^([0-9]+)\. (.*)$/);
      if (numbered) {
        kind = 'numbered';
        content = numbered[2];
      }
    }
    blocks.push({ kind, content, checked });
  }

  if (code !== null) blocks.push({ kind: 'code', content: code.join('\n'), checked: 0 });
  if (!blocks.length) blocks.push({ kind: 'text', content: '', checked: 0 });
  if (!title) title = path.basename(filePath, path.extname(filePath)) || 'Untitled';
  return { title, blocks };
}

function markdownExportFileName(title) {
  let name = Array.from(String(title || 'Untitled').replace(/[\u0000-\u001f<>:"/\\|?*]/g, '_'))
    .slice(0, 120)
    .join('')
    .trim()
    .replace(/[. ]+$/g, '');
  if (!name) name = 'Untitled';
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$/i.test(name)) name = `_${name}`;
  return `${name}.md`;
}

module.exports = {
  parseClipboardInlineMarkdown,
  parseClipboardMarkdown,
  clipboardBlockText,
  parseMarkdownImport,
  markdownExportFileName,
};
