use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::domain::model::{Block, BlockId, PageId, PageSnapshot, SaveBatch};
use crate::error::Result;
use crate::storage::Repository;

use super::Event;
use super::history::{GlobalHistory, GlobalUndoEntry, PageHistory};

const HISTORY_SAVE_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub(super) struct PendingSaves {
    failed_pages: HashSet<PageId>,
    failed_blocks: HashSet<BlockId>,
}

impl PendingSaves {
    pub(super) fn has_failed_changes(&self) -> bool {
        !self.failed_pages.is_empty() || !self.failed_blocks.is_empty()
    }

    pub(super) fn save(
        &mut self,
        repository: &mut Repository,
        page_histories: &mut HashMap<PageId, PageHistory>,
        global_history: &mut GlobalHistory,
        id: u64,
        batch: &SaveBatch,
    ) -> Result<Event> {
        let edited_pages: HashSet<_> = batch.pages.iter().map(|page| page.id).collect();
        let mut edited_blocks: HashMap<PageId, Vec<&Block>> = HashMap::new();
        for block in &batch.blocks {
            edited_blocks.entry(block.page_id).or_default().push(block);
        }
        let touched_pages: HashSet<_> = edited_pages
            .iter()
            .copied()
            .chain(edited_blocks.keys().copied())
            .collect();
        let history_entries: Vec<_> = touched_pages
            .iter()
            .filter_map(|&page| {
                let snapshot = repository.page_snapshot(page).ok()?;
                let elapsed = page_histories
                    .get(&page)
                    .and_then(|history| history.last_save_time)
                    .is_none_or(|time| time.elapsed() > HISTORY_SAVE_INTERVAL);
                let blocks = edited_blocks.get(&page).map_or(&[][..], Vec::as_slice);
                let description =
                    describe_edit(&snapshot, blocks, edited_pages.contains(&page), elapsed)?;
                Some((page, snapshot, description))
            })
            .collect();

        if let Err(error) = repository.save(batch) {
            self.failed_pages
                .extend(batch.pages.iter().map(|page| page.id));
            self.failed_blocks
                .extend(batch.blocks.iter().map(|block| block.id));
            return Err(error);
        }

        for page in &batch.pages {
            self.failed_pages.remove(&page.id);
        }
        for block in &batch.blocks {
            self.failed_blocks.remove(&block.id);
        }
        for (page, snapshot, description) in history_entries {
            if page_histories
                .entry(page)
                .or_default()
                .push_undo(snapshot, description)
            {
                global_history.push_undo(GlobalUndoEntry::Page(page));
            }
        }

        let saved_at = Instant::now();
        let histories = touched_pages
            .into_iter()
            .map(|page| {
                let history = page_histories.entry(page).or_default();
                history.last_save_time = Some(saved_at);
                (page, history.summaries())
            })
            .collect();
        Ok(Event::Saved { id, histories })
    }
}

fn describe_edit(
    snapshot: &PageSnapshot,
    edits: &[&Block],
    page_edited: bool,
    interval_elapsed: bool,
) -> Option<&'static str> {
    let previous: HashMap<_, _> = snapshot
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect();
    let mut kind_changed = false;
    for edit in edits {
        if let Some(block) = previous.get(&edit.id) {
            if block.checked != edit.checked {
                return Some("Görev durumu değiştirildi");
            }
            kind_changed |= block.kind != edit.kind;
        }
    }

    if page_edited {
        Some("Başlık düzenlendi")
    } else if interval_elapsed || kind_changed {
        Some("Metin düzenlendi")
    } else {
        None
    }
}
