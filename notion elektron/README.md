# Notion Elektron

Electron desktop edition of Noteon. It reads and writes Noteon's SQLite schema so pages, nested page trees, block content, database views, properties, settings and media are shared without a lossy Markdown round trip.

## Run

Install Node.js 20 or later, then from this directory:

```powershell
npm install
npm start
```

The start command installs the matching Electron runtime if needed, rebuilds the native SQLite module, then opens the window.

The Electron workspace is in `data/` beside this project. Set `NOTEON_DATA_DIR` to use another folder. When packaged, the default data folder is beside the executable.

## Included features

- Nested pages and inline subpages, drag-to-move page trees, favorites, workspace and page search, page icons and covers with image positioning and color presets, typed page properties with option management, full-width pages, and a recoverable trash with restore, permanent-delete and empty-trash controls.
- Tabs (middle-click opens a page in a new tab), side-by-side windows, and autosaved text, heading, list, to-do, quote, callout, code and divider blocks.
- Bold, italic, underline, strikethrough, text/highlight colors, and serif/monospace text. Blocks support batch selection/formatting, in-session undo/history, reorder controls, resizable and cross-column draggable blocks, persistent media sizing, and an ungroup action.
- Embedded images and videos, page links and database pages. Imported media is copied into the local attachments folder.
- Table, gallery and board database views with editable typed and legacy properties, bulk value changes, resizable table columns, and numeric calculations.
- Markdown import/export and consistent SQLite workspace backups with a validated restore flow and an automatic pre-restore recovery copy. Backups contain the SQLite database; attachments remain in the workspace folder.
- Noteon data-folder import validates the SQLite workspace, preserves extra settings and snapshots, reuses matching attachment files by SHA-256, copies changed/missing files, rebases available attachment paths, and keeps a recovery copy of the current Electron workspace.
- Assistant chat with the Codex CLI, MCP workspace tools, change snapshots and undo. The Codex CLI must be installed and signed in.
- Light and dark themes with section color customization, adjustable type size, and Ctrl+N / Ctrl+K / Ctrl+J / Ctrl+F shortcuts.

## Imported workspace

The included `data/` folder was copied from the Noteon workspace and keeps SQLite schema version 15. The database contains 9,860 pages, 51,739 blocks, 376 databases, 1,024 database properties and 25,983 property values. The 1,885 attachment files (about 2.84 GB), Assistant settings, six backups, two AI snapshots and 18 import files were also copied. Attachment references in the active workspace and SQLite backups were rebased to this folder. The source Noteon workspace was left intact.

Run `npm start` to open the imported workspace, or launch `dist/Notion Elektron 1.0.0.exe`. The packaged app checks the adjacent folder and the parent project `data/` folder. Electron edits are written directly to this copy.
