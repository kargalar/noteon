const path = require('node:path');
const fs = require('node:fs');
const crypto = require('node:crypto');
const { state } = require('../state.cjs');

const ATTACHMENT_PREFIX_PATTERN = /(?:[a-z]:[\\/]|\\\\|\/)[^"'<>|\r\n]*?[\\/]attachments[\\/]/gi;

/**
 * Builds a sorted index of relative file paths inside an attachments directory.
 * Longest keys come first so nested matches win over shorter prefixes.
 * @param {string} directory
 */
function attachmentRelativePathIndex(directory) {
  const paths = [];
  if (!fs.existsSync(directory)) return paths;

  const visit = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const target = path.join(current, entry.name);
      if (entry.isDirectory()) {
        visit(target);
      } else if (entry.isFile()) {
        const relative = path.relative(directory, target);
        paths.push({
          key: relative.replaceAll('\\', '/').toLowerCase(),
          target: relative,
        });
      }
    }
  };

  visit(directory);
  return paths.sort((a, b) => b.key.length - a.key.length);
}

function hasAttachmentPath(value) {
  return typeof value === 'string' && /[\\/]attachments[\\/]/i.test(value);
}

function isExternalUrlAttachmentPath(value, index) {
  const prefix = value.slice(0, index);
  const url = prefix.match(/([a-z][a-z0-9+.-]*):\/\/[^"'<>|\r\n]*$/i);
  return Boolean(url && url[1].toLowerCase() !== 'file');
}

function matchAttachmentRelativePath(value, relativePaths) {
  const normalized = value.replaceAll('\\', '/').toLowerCase();
  return (
    relativePaths.find((item) => {
      if (!normalized.startsWith(item.key)) return false;
      const next = normalized[item.key.length];
      return next === undefined || /[\\/\s"'<>()[\],;.!?]/.test(next);
    }) || null
  );
}

function hasUnmatchedAttachmentPath(value, relativePaths) {
  if (!hasAttachmentPath(value)) return false;
  const source = String(value);
  const all = [...source.matchAll(ATTACHMENT_PREFIX_PATTERN)];
  if (!all.length) return true;
  const matches = all.filter((match) => !isExternalUrlAttachmentPath(source, match.index));
  return matches.some(
    (match) => !matchAttachmentRelativePath(source.slice(match.index + match[0].length), relativePaths),
  );
}

function rebaseAttachmentReferences(value, newRoot, relativePaths) {
  if (typeof value !== 'string' || !newRoot || !relativePaths.length || !hasAttachmentPath(value)) {
    return value;
  }
  const edits = [];
  for (const match of value.matchAll(ATTACHMENT_PREFIX_PATTERN)) {
    if (isExternalUrlAttachmentPath(value, match.index)) continue;
    const relative = matchAttachmentRelativePath(value.slice(match.index + match[0].length), relativePaths);
    if (!relative) continue;
    edits.push({
      start: match.index,
      end: match.index + match[0].length + relative.key.length,
      value: path.join(newRoot, relative.target),
    });
  }
  for (const edit of edits.reverse()) {
    value = value.slice(0, edit.start) + edit.value + value.slice(edit.end);
  }
  return value;
}

async function fileSha256(filename) {
  const hash = crypto.createHash('sha256');
  for await (const chunk of fs.createReadStream(filename)) {
    hash.update(chunk);
  }
  return hash.digest('hex');
}

/**
 * Copies Noteon attachment files into the target workspace, reusing identical files by size + SHA-256.
 */
async function copyNoteonAttachments(sourceRoot, targetRoot, importFolder, onProgress = () => {}) {
  const sourcePaths = attachmentRelativePathIndex(sourceRoot);
  const mappedPaths = [];
  let copied = 0;
  let reused = 0;
  let completed = 0;

  for (const item of sourcePaths) {
    const source = path.join(sourceRoot, item.target);
    const existing = path.join(targetRoot, item.target);
    let canReuse = false;
    try {
      const [sourceStat, targetStat] = await Promise.all([
        fs.promises.stat(source),
        fs.promises.stat(existing),
      ]);
      if (sourceStat.isFile() && targetStat.isFile() && sourceStat.size === targetStat.size) {
        canReuse = (await fileSha256(source)) === (await fileSha256(existing));
      }
    } catch {}

    if (canReuse) {
      mappedPaths.push({ key: item.key, target: item.target });
      reused++;
    } else {
      const destination = path.join(importFolder, item.target);
      await fs.promises.mkdir(path.dirname(destination), { recursive: true });
      await fs.promises.copyFile(source, destination, fs.constants.COPYFILE_EXCL);
      mappedPaths.push({ key: item.key, target: path.relative(targetRoot, destination) });
      copied++;
    }

    completed++;
    if (completed % 25 === 0 || completed === sourcePaths.length) {
      onProgress({ completed, total: sourcePaths.length, copied, reused });
    }
  }

  return {
    relativePaths: mappedPaths.sort((a, b) => b.key.length - a.key.length),
    copied,
    reused,
    total: sourcePaths.length,
  };
}

/**
 * Rebases local attachment paths inside `state.db` to match the active `dataDir/attachments` directory.
 */
function rebaseImportedAttachmentPaths() {
  const newPrefix = path.join(state.dataDir, 'attachments');
  if (!fs.existsSync(newPrefix)) return;
  const relativePaths = attachmentRelativePathIndex(newPrefix);
  if (!relativePaths.length) return;

  state.db.transaction(() => {
    const pages = state.db.prepare('SELECT id,cover_image FROM pages WHERE cover_image IS NOT NULL').all();
    const updateCover = state.db.prepare('UPDATE pages SET cover_image=? WHERE id=?');
    for (const page of pages) {
      const next = rebaseAttachmentReferences(page.cover_image, newPrefix, relativePaths);
      if (next !== page.cover_image) updateCover.run(next, page.id);
    }

    const media = state.db.prepare("SELECT id,content FROM blocks WHERE kind IN ('image','video')").all();
    const updateBlock = state.db.prepare('UPDATE blocks SET content=? WHERE id=?');
    for (const block of media) {
      const next = rebaseAttachmentReferences(block.content, newPrefix, relativePaths);
      if (next !== block.content) updateBlock.run(next, block.id);
    }

    const linked = state.db
      .prepare("SELECT id,content FROM blocks WHERE kind='text' AND content LIKE '%attachments%'")
      .all();
    for (const block of linked) {
      const next = rebaseAttachmentReferences(block.content, newPrefix, relativePaths);
      if (next !== block.content) updateBlock.run(next, block.id);
    }
  })();
}

function scanNoteonAttachmentReferences(database) {
  const candidates = database.prepare(
    "SELECT cover_image AS value FROM pages WHERE cover_image IS NOT NULL UNION ALL SELECT content AS value FROM blocks WHERE kind IN ('image','video') UNION ALL SELECT content AS value FROM blocks WHERE kind='text' AND lower(content) LIKE '%attachments%'",
  );
  for (const row of candidates.iterate()) {
    if (!hasAttachmentPath(row.value)) continue;
    const value = String(row.value);
    for (const match of value.matchAll(ATTACHMENT_PREFIX_PATTERN)) {
      if (!isExternalUrlAttachmentPath(value, match.index)) return true;
    }
  }
  return false;
}

function rewriteNoteonAttachmentReferences(database, newRoot, relativePaths) {
  let rebased = 0;
  let unmatched = 0;
  const scans = [
    ['SELECT id,cover_image AS value FROM pages WHERE cover_image IS NOT NULL AND id>? ORDER BY id LIMIT 250', 'UPDATE pages SET cover_image=? WHERE id=?'],
    ["SELECT id,content AS value FROM blocks WHERE kind IN ('image','video') AND id>? ORDER BY id LIMIT 250", 'UPDATE blocks SET content=? WHERE id=?'],
    ["SELECT id,content AS value FROM blocks WHERE kind='text' AND lower(content) LIKE '%attachments%' AND id>? ORDER BY id LIMIT 250", 'UPDATE blocks SET content=? WHERE id=?'],
  ];

  database.transaction(() => {
    for (const [selectSql, updateSql] of scans) {
      const select = database.prepare(selectSql);
      const update = database.prepare(updateSql);
      let after = 0;
      while (true) {
        const batch = select.all(after);
        if (!batch.length) break;
        for (const row of batch) {
          const next = rebaseAttachmentReferences(row.value, newRoot, relativePaths);
          if (next !== row.value) {
            update.run(next, row.id);
            rebased++;
          }
          if (hasUnmatchedAttachmentPath(row.value, relativePaths)) unmatched++;
        }
        after = batch[batch.length - 1].id;
      }
    }
  })();

  return { rebased, unmatched };
}

function countFiles(directory) {
  if (!fs.existsSync(directory)) return 0;
  let count = 0;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) count += countFiles(target);
    else if (entry.isFile()) count++;
  }
  return count;
}

function copyNoteonExtras(sourceDir, targetDir) {
  fs.mkdirSync(targetDir, { recursive: true });
  const ignored = new Set(['noteon.sqlite', 'noteon.sqlite-wal', 'noteon.sqlite-shm', 'attachments']);
  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    if (ignored.has(entry.name.toLowerCase())) continue;
    const source = path.join(sourceDir, entry.name);
    const target = path.join(targetDir, entry.name);
    fs.cpSync(source, target, { recursive: true, errorOnExist: true, force: false });
  }
  return countFiles(targetDir);
}

module.exports = {
  attachmentRelativePathIndex,
  hasAttachmentPath,
  isExternalUrlAttachmentPath,
  matchAttachmentRelativePath,
  hasUnmatchedAttachmentPath,
  rebaseAttachmentReferences,
  fileSha256,
  copyNoteonAttachments,
  rebaseImportedAttachmentPaths,
  scanNoteonAttachmentReferences,
  rewriteNoteonAttachmentReferences,
  countFiles,
  copyNoteonExtras,
};
