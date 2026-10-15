# Repeatable performance benchmark

The benchmark creates **10,000 pages and 119,990 blocks** in a separate temporary SQLite database. The first page contains **20,000 blocks**; every other page contains 10. Each operation is measured 31 times, and p50 and p95 are reported. The write benchmark changes text on every repetition and updates the FTS index.

Results are collected on the same Windows machine with a `release` build. These are warmed, in-process database operations; they do not measure application startup, cold disk, full window rendering, or end-to-end user latency. Power settings, storage, antivirus, drivers, and document contents can change the results.

Raw output from the latest verification: [performance-results.txt](performance-results.txt).

`tests/editor.rs` checks that drawing a 20,000-block document is limited to the visible area and that deep scrolling requests only the relevant content batches. `tests/repository.rs` verifies that SQLite uses the `blocks_page_position` index for a deep range query on a 10,000-block page.

Range loading, virtualization, and the background worker are the primary optimizations for pages with thousands of blocks. A UI that is fast only on a small sample is not assumed to scale. These measurements do not guarantee a fixed frame rate on all hardware.

Database views are also loaded by the background SQLite worker. Table, gallery, and board views draw only visible records, so thousands of records do not create widgets on every frame. Sorting runs once per view in memory. The database cache is limited to eight views, excluding records shown in active panels.
