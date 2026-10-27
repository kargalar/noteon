<div align="center">

# Noteon

**A local-first workspace for notes, pages, and databases.**

Capture ideas, organize projects, and shape your workspace on Windows. Your pages stay on your device, with no account or internet connection required.

[![Windows](https://img.shields.io/badge/Windows-10%2B-0078D4?logo=windows&logoColor=white)](https://github.com/kargalar/noteon/releases/latest) [![Release](https://img.shields.io/github/v/release/kargalar/noteon?color=6B8E4E)](https://github.com/kargalar/noteon/releases/latest) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE) [![Written in Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)

[Download for Windows ↓](https://github.com/kargalar/noteon/releases/latest) · [Build from source](#build-from-source) · [Report an issue](https://github.com/kargalar/noteon/issues)

</div>

## A quieter place to think

Noteon brings flexible pages, block-based writing, and database views into one desktop workspace. Open pages side by side, arrange panels to fit your workflow, and keep working offline. The workspace is stored in a local SQLite database.

## Features

- Create nested pages with searchable icons, covers, favorites, and custom properties.
- Write with text, headings, lists, to-dos, quotes, callouts, code, dividers, images, and videos.
- Arrange blocks in resizable columns and move them by dragging.
- Open pages in tabs and split the workspace into resizable panels.
- Turn a page into a database with table, gallery, and board views, custom properties, and sorting.
- Search page titles and content with SQLite full-text search.
- Import and export Markdown, and create consistent SQLite backups.
- Customize the light and dark themes, accent color, and font size.
- Save changes automatically and keep media files inside Noteon’s local data folder.

## Install

Download the latest **Noteon for Windows** archive from [GitHub Releases](https://github.com/kargalar/noteon/releases/latest), extract it, and run `Noteon.exe`. The packaged app does not require Rust to be installed.

Video playback requires FFmpeg tools (`ffmpeg` and `ffplay`) on `PATH`. Other features work without them.

## Build from source

Install the Rust MSVC toolchain and the **Desktop development with C++** workload from Visual Studio Build Tools. Then run:

```powershell
cargo run --release
```

To validate and package a Windows release build:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
.\scripts\package.ps1
```

`Cargo.lock` pins dependency versions. Dependencies are downloaded during the first build; Noteon itself runs locally and does not require an internet connection.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+N | Create a page |
| Ctrl+K | Search the workspace |
| Ctrl+S | Save now or retry after an error |
| Alt+Left / Alt+Right | Navigate page history |
| Enter | Split at the cursor and create a block |
| Shift+Enter | Insert a line break in the current block |
| Enter in a table cell | Create a record and focus its Name |
| Tab / Shift+Tab in a table | Move to the next / previous editable property |
| Ctrl+Z / Ctrl+Y | Undo / redo in the active text field |

Type `/` in an empty block to search available block types. Right-click a block or page for its context menu. Middle-click a page to open it in a new tab; drag tabs to split panels. Drag a cover image to adjust its crop, and drag media edges to resize media blocks.

Choose **Sort → Custom** in a database table, then drag a record's icon to change its position. Custom order is saved and restored when you switch back from another sort.

## Data and backups

By default, Noteon stores the workspace at `%LOCALAPPDATA%\Kargalar\Noteon\data\noteon.sqlite`. The exact location is shown in Settings. To choose a different data folder, set `NOTEON_DATA_DIR` before launching Noteon:

```powershell
$env:NOTEON_DATA_DIR = 'D:\Notes\Noteon'
.\dist\Noteon\Noteon.exe
```

Use **Settings → Back up workspace** to create a consistent backup. Close Noteon before restoring one. Replace the workspace database with the backup file and reopen the app. The local database is not encrypted.

## Performance

The sidebar and editor virtualize long lists. Noteon loads page contents on demand, fetches block text in indexed batches, and renders only visible rows. SQLite operations, search, file transfers, and file dialogs run on a worker thread. The benchmark creates 10,000 pages and nearly 120,000 blocks; results and limits are documented in [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

## License

Noteon is licensed under the MIT License. See [LICENSE](LICENSE).
