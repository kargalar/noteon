use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::config::AUTOSAVE_DELAY;
use crate::domain::model::{Block, BlockId, PageEdit, PageId, SaveBatch};

struct Dirty<T> {
    revision: u64,
    value: T,
}

#[derive(Default)]
pub struct Saves {
    blocks: HashMap<BlockId, Dirty<Block>>,
    pages: HashMap<PageId, Dirty<PageEdit>>,
    in_flight: BTreeMap<u64, SaveBatch>,
    revision: u64,
    sent_revision: u64,
    last_edit: Option<Instant>,
    pub failed: bool,
}

impl Saves {
    pub fn edit_block(&mut self, block: Block) {
        self.revision += 1;
        self.blocks.insert(
            block.id,
            Dirty {
                revision: self.revision,
                value: block,
            },
        );
        self.last_edit = Some(Instant::now());
    }

    pub fn edit_page(&mut self, page: PageEdit) {
        self.revision += 1;
        self.pages.insert(
            page.id,
            Dirty {
                revision: self.revision,
                value: page,
            },
        );
        self.last_edit = Some(Instant::now());
    }

    pub fn is_idle(&self) -> bool {
        self.blocks.is_empty() && self.pages.is_empty() && self.in_flight.is_empty()
    }

    pub fn next_batch(&mut self, force: bool) -> Option<(u64, SaveBatch)> {
        if self.failed
            || (!force
                && self
                    .last_edit
                    .is_none_or(|time| time.elapsed() < AUTOSAVE_DELAY))
        {
            return None;
        }
        let batch = SaveBatch {
            blocks: self
                .blocks
                .values()
                .filter(|dirty| dirty.revision > self.sent_revision)
                .map(|dirty| dirty.value.clone())
                .collect(),
            pages: self
                .pages
                .values()
                .filter(|dirty| dirty.revision > self.sent_revision)
                .map(|dirty| dirty.value.clone())
                .collect(),
        };
        if batch.is_empty() {
            return None;
        }
        self.revision += 1;
        let id = self.revision;
        self.sent_revision = id;
        self.in_flight.insert(id, batch.clone());
        Some((id, batch))
    }

    pub fn acknowledge(&mut self, id: u64) {
        if let Some(batch) = self.in_flight.remove(&id) {
            for block in batch.blocks {
                if self
                    .blocks
                    .get(&block.id)
                    .is_some_and(|dirty| dirty.revision <= id)
                {
                    self.blocks.remove(&block.id);
                }
            }
            for page in batch.pages {
                if self
                    .pages
                    .get(&page.id)
                    .is_some_and(|dirty| dirty.revision <= id)
                {
                    self.pages.remove(&page.id);
                }
            }
        }
    }

    pub fn fail(&mut self, id: u64) {
        self.in_flight.remove(&id);
        self.sent_revision = 0;
        self.failed = true;
    }

    pub fn retry(&mut self) {
        self.sent_revision = 0;
        self.failed = false;
    }

    pub fn overlay_blocks(&self, blocks: &mut [Block]) {
        for block in blocks {
            if let Some(dirty) = self.blocks.get(&block.id) {
                // Position belongs to the latest database outline, not to a past save snapshot.
                let position = block.position;
                *block = dirty.value.clone();
                block.position = position;
            }
        }
    }

    pub fn overlay_page(&self, page: &mut crate::domain::model::Page) {
        if let Some(dirty) = self.pages.get(&page.id) {
            page.title.clone_from(&dirty.value.title);
            page.icon.clone_from(&dirty.value.icon);
            page.cover = dirty.value.cover;
            page.cover_image.clone_from(&dirty.value.cover_image);
            page.cover_x = dirty.value.cover_x;
            page.cover_y = dirty.value.cover_y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::BlockKind;

    fn block(content: &str) -> Block {
        Block {
            id: 1,
            page_id: 1,
            position: 0,
            kind: BlockKind::Text,
            content: content.into(),
            text_styles: Vec::new(),
            checked: false,
            column_group: None,
            column_index: 0,
            media_width: 1.0,
        }
    }

    #[test]
    fn old_acknowledgement_does_not_discard_newer_typing() {
        let mut saves = Saves::default();
        saves.edit_block(block("ilk"));
        let (first, _) = saves.next_batch(true).unwrap();
        saves.edit_block(block("son"));
        saves.acknowledge(first);
        assert!(!saves.is_idle());
        let (second, batch) = saves.next_batch(true).unwrap();
        assert_eq!(batch.blocks[0].content, "son");
        saves.acknowledge(second);
        assert!(saves.is_idle());
    }

    #[test]
    fn failed_write_keeps_data_and_retry_resends_it() {
        let mut saves = Saves::default();
        saves.edit_block(block("korunmalı"));
        let (id, _) = saves.next_batch(true).unwrap();
        saves.fail(id);
        assert!(!saves.is_idle());
        assert!(saves.next_batch(true).is_none());
        saves.retry();
        let (id, batch) = saves.next_batch(true).unwrap();
        assert_eq!(batch.blocks[0].content, "korunmalı");
        saves.acknowledge(id);
        assert!(saves.is_idle());
    }
}
