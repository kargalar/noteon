use std::collections::{BTreeMap, HashSet, VecDeque};

use crate::config::{BLOCK_BATCH_SIZE, CACHED_BATCHES};
use crate::domain::model::Block;

#[derive(Default)]
pub struct BlockCache {
    pub blocks: BTreeMap<usize, Block>,
    pub pending: HashSet<usize>,
    pub needs_measure: HashSet<usize>,
    batches: VecDeque<usize>,
}

impl BlockCache {
    pub fn insert(&mut self, start: usize, blocks: Vec<Block>) {
        self.pending.remove(&start);
        self.batches.retain(|batch| *batch != start);
        self.batches.push_back(start);
        for block in blocks {
            self.needs_measure.insert(block.position);
            self.blocks.insert(block.position, block);
        }
        while self.batches.len() > CACHED_BATCHES {
            if let Some(oldest) = self.batches.pop_front() {
                self.blocks.retain(|position, _| {
                    *position < oldest || *position >= oldest + BLOCK_BATCH_SIZE
                });
            }
        }
    }

    pub fn request(&mut self, position: usize) -> Option<usize> {
        let start = position / BLOCK_BATCH_SIZE * BLOCK_BATCH_SIZE;
        if self.blocks.contains_key(&position) || !self.pending.insert(start) {
            return None;
        }
        Some(start)
    }

    pub fn touch(&mut self, position: usize) {
        let start = position / BLOCK_BATCH_SIZE * BLOCK_BATCH_SIZE;
        if self.batches.back() != Some(&start) && self.batches.contains(&start) {
            self.batches.retain(|batch| *batch != start);
            self.batches.push_back(start);
        }
    }
}
