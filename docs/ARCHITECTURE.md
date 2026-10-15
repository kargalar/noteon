# Architecture

The interface uses `eframe/egui`, and persistent data is stored with `rusqlite`. The application is written in Rust; SQLite, windowing, and graphics are provided by the corresponding libraries. Noteon has no HTML, JavaScript, or local HTTP server.

`domain` contains the application's data models, Markdown conversion, Unicode text editing, and Fenwick tree. It does not manage the UI or SQLite connection.

`storage::Repository` owns one database connection. Schema changes are versioned. Page cycles are rejected. Block insertion, deletion, movement, batch saves, duplication, and Markdown import use atomic transactions. Triggers maintain block counts and content-size metadata. Full-text indexes are updated in the same transaction.

`worker` exclusively owns the connection and communicates with the interface through typed `Request` and `Event` messages. It requests a repaint when a result arrives. If a pending save fails, later structural changes are rejected so unsaved text is not overwritten.

`ui` does not call the database; it emits `Intent` values. The editor and page tree draw visible rows instead of building a persistent widget tree. Requests carry generation IDs so stale results from a previous page are ignored. Search results use a separate generation ID.

`app` handles events, navigation, and side effects. If text editing and Enter occur in the same frame, text changes are processed before structural commands.

## Save consistency

Each edit receives an increasing revision. Sending a save does not clear dirty data. A successful SQLite response clears only the revisions included in that request; newer edits remain intact. If a block leaves the cache before it is saved, its pending copy is retained and reapplied when the block is loaded again.

Missing page or block updates do not silently succeed; the entire save batch is rolled back. Disk errors are shown to the user. Normal window shutdown waits for pending saves. WAL and `synchronous=FULL` protect completed writes; text still inside the save delay is not considered durable if the process is forcibly closed.

## Data and scale

Page metadata and the active document's compact block index use O(n) memory. The full-text cache is limited by batch count; its byte size depends on block lengths. Very long code blocks scroll internally. Editing or measuring one very large block scales with that block's length.

Block positions are contiguous integers. Deep reads use indexed `position >= start AND position < end` ranges rather than OFFSET. Structural insertion and deletion shift positions; sparse sort keys may be considered in the future for workloads with frequent middle insertions across hundreds of thousands of blocks.

Search quotes each token, does not execute user input as an FTS operator, and is limited to the first 16 tokens and 50 results. Title matches appear first, followed by active content matches in index order. There is no global BM25 ranking across all matches. SQLite's `unicode61` tokenizer does not provide full morphological analysis for every language.

Trash uses a deletion-group ID. Restoring a parent does not restore child pages that were deleted separately. The interface does not permanently delete pages.

## Database pages

A database is a regular page described in the `databases` table. Its records are child pages, so each record also opens as a complete editor page. `database_properties` stores field definitions, and `page_property_values` stores record values. Types, options, and values are separate from page blocks. The built-in name, creation date, and 0–3 priority fields remain in the `pages` table.

Database queries run on the storage worker thread. Table, gallery, and board views virtualize visible records. The application keeps up to eight database views in memory; active panels are excluded from this limit.
