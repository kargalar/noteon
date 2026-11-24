use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::domain::model::{Page, PageHistorySummary, PageId, PageSnapshot};
use crate::error::{Error, Result};
use crate::storage::Repository;

pub(super) const MAX_PAGE_HISTORY: usize = 10;
const MAX_GLOBAL_HISTORY: usize = 50;
const TRASH_GROUP_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Clone, Copy)]
pub(super) enum HistoryDirection {
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum GlobalUndoEntry {
    Page(PageId),
    Trash(Vec<PageId>),
    Restore(Vec<PageId>),
    CreateRecord(PageId),
}

impl GlobalUndoEntry {
    fn includes_page(&self, page: PageId) -> bool {
        match self {
            Self::Page(id) | Self::CreateRecord(id) => *id == page,
            Self::Trash(ids) | Self::Restore(ids) => ids.contains(&page),
        }
    }
}

struct GlobalUndoRecord {
    entry: GlobalUndoEntry,
    time: Instant,
}

#[derive(Default)]
pub(super) struct GlobalHistory {
    undo_stack: Vec<GlobalUndoRecord>,
    redo_stack: Vec<GlobalUndoRecord>,
}

pub(super) struct HistoryChange {
    pub(super) page: Option<PageId>,
    pub(super) refresh_database: Option<PageId>,
}

impl GlobalHistory {
    pub(super) fn push_undo(&mut self, entry: GlobalUndoEntry) {
        self.redo_stack.clear();
        if let GlobalUndoEntry::Trash(new_ids) = &entry
            && let Some(last) = self.undo_stack.last_mut()
            && let GlobalUndoEntry::Trash(last_ids) = &mut last.entry
            && last.time.elapsed() < TRASH_GROUP_INTERVAL
        {
            for id in new_ids {
                if !last_ids.contains(id) {
                    last_ids.push(*id);
                }
            }
            last.time = Instant::now();
            return;
        }
        push_global_entry(&mut self.undo_stack, entry);
    }

    pub(super) fn apply(
        &mut self,
        repository: &mut Repository,
        page_histories: &mut HashMap<PageId, PageHistory>,
        direction: HistoryDirection,
    ) -> Result<Option<HistoryChange>> {
        let (source, destination) = match direction {
            HistoryDirection::Undo => (&mut self.undo_stack, &mut self.redo_stack),
            HistoryDirection::Redo => (&mut self.redo_stack, &mut self.undo_stack),
        };
        while let Some(record) = source.last() {
            let applied = apply_global_entry(repository, page_histories, direction, &record.entry)?;
            source.pop();
            if let Some((entry, change)) = applied {
                push_global_entry(destination, entry);
                return Ok(Some(change));
            }
        }
        Ok(None)
    }

    pub(super) fn prune_page(&mut self, id: PageId) {
        self.undo_stack
            .retain(|record| !record.entry.includes_page(id));
        self.redo_stack
            .retain(|record| !record.entry.includes_page(id));
    }

    pub(super) fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

fn push_global_entry(stack: &mut Vec<GlobalUndoRecord>, entry: GlobalUndoEntry) {
    stack.push(GlobalUndoRecord {
        entry,
        time: Instant::now(),
    });
    if stack.len() > MAX_GLOBAL_HISTORY {
        stack.remove(0);
    }
}

fn apply_global_entry(
    repository: &mut Repository,
    page_histories: &mut HashMap<PageId, PageHistory>,
    direction: HistoryDirection,
    entry: &GlobalUndoEntry,
) -> Result<Option<(GlobalUndoEntry, HistoryChange)>> {
    let (entry, change) = match entry {
        GlobalUndoEntry::Page(id) => {
            let history = page_histories.entry(*id).or_default();
            if !history.apply(repository, *id, direction)? {
                return Ok(None);
            }
            (
                GlobalUndoEntry::Page(*id),
                HistoryChange {
                    page: Some(*id),
                    refresh_database: owning_database(repository, *id),
                },
            )
        }
        GlobalUndoEntry::Trash(ids) | GlobalUndoEntry::Restore(ids) => {
            let mut valid_ids = Vec::with_capacity(ids.len());
            for &id in ids {
                if existing_page(repository, id)?.is_some() {
                    valid_ids.push(id);
                }
            }
            if valid_ids.is_empty() {
                return Ok(None);
            }
            let trashed = matches!(entry, GlobalUndoEntry::Trash(_));
            let restore = match direction {
                HistoryDirection::Undo => trashed,
                HistoryDirection::Redo => !trashed,
            };
            let database = |repository: &Repository| {
                valid_ids
                    .iter()
                    .rev()
                    .find_map(|&id| owning_database(repository, id))
            };
            let change = if restore {
                repository.restore_pages(&valid_ids)?;
                HistoryChange {
                    page: valid_ids.last().copied(),
                    refresh_database: database(repository),
                }
            } else {
                let refresh_database = database(repository);
                repository.trash_pages(&valid_ids)?;
                HistoryChange {
                    page: None,
                    refresh_database,
                }
            };
            let entry = if trashed {
                GlobalUndoEntry::Trash(valid_ids)
            } else {
                GlobalUndoEntry::Restore(valid_ids)
            };
            (entry, change)
        }
        GlobalUndoEntry::CreateRecord(id) => {
            let Some(record) = existing_page(repository, *id)? else {
                return Ok(None);
            };
            let refresh_database = record
                .parent_id
                .filter(|parent| repository.page(*parent).is_ok_and(|page| page.is_database));
            let page = match direction {
                HistoryDirection::Undo => {
                    repository.trash_pages(&[*id])?;
                    None
                }
                HistoryDirection::Redo => {
                    repository.restore_pages(&[*id])?;
                    Some(*id)
                }
            };
            (
                GlobalUndoEntry::CreateRecord(*id),
                HistoryChange {
                    page,
                    refresh_database,
                },
            )
        }
    };
    Ok(Some((entry, change)))
}

fn existing_page(repository: &Repository, id: PageId) -> Result<Option<Page>> {
    match repository.page(id) {
        Ok(page) => Ok(Some(page)),
        Err(Error::Database(rusqlite::Error::QueryReturnedNoRows)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn owning_database(repository: &Repository, id: PageId) -> Option<PageId> {
    let page = repository.page(id).ok()?;
    if page.is_database {
        Some(id)
    } else {
        page.parent_id
            .filter(|parent| repository.page(*parent).is_ok_and(|page| page.is_database))
    }
}

struct PageHistoryEntry {
    id: u64,
    description: String,
    timestamp: i64,
    snapshot: PageSnapshot,
}

#[derive(Default)]
pub(super) struct PageHistory {
    undo_stack: Vec<PageHistoryEntry>,
    redo_stack: Vec<PageHistoryEntry>,
    next_id: u64,
    pub(super) last_save_time: Option<Instant>,
}

impl PageHistory {
    pub(super) fn push_undo(
        &mut self,
        snapshot: PageSnapshot,
        description: impl Into<String>,
    ) -> bool {
        if self
            .undo_stack
            .last()
            .is_some_and(|top| top.snapshot == snapshot)
        {
            return false;
        }
        let entry = self.new_entry(snapshot, description.into());
        push_page_entry(&mut self.undo_stack, entry);
        self.redo_stack.clear();
        true
    }

    pub(super) fn apply(
        &mut self,
        repository: &mut Repository,
        page: PageId,
        direction: HistoryDirection,
    ) -> Result<bool> {
        let source = match direction {
            HistoryDirection::Undo => &mut self.undo_stack,
            HistoryDirection::Redo => &mut self.redo_stack,
        };
        let Some(entry) = source.last() else {
            return Ok(false);
        };
        let current = repository.page_snapshot(page)?;
        repository.apply_page_snapshot(&entry.snapshot)?;
        let previous = source
            .pop()
            .expect("the restored entry is still on the stack");
        let entry = self.new_entry(current, previous.description);
        let destination = match direction {
            HistoryDirection::Undo => &mut self.redo_stack,
            HistoryDirection::Redo => &mut self.undo_stack,
        };
        push_page_entry(destination, entry);
        self.last_save_time = None;
        Ok(true)
    }

    pub(super) fn restore(
        &mut self,
        repository: &mut Repository,
        page: PageId,
        entry_id: u64,
    ) -> Result<bool> {
        let Some(entry) = self.undo_stack.iter().find(|entry| entry.id == entry_id) else {
            return Ok(false);
        };
        let current = repository.page_snapshot(page)?;
        repository.apply_page_snapshot(&entry.snapshot)?;
        let entry = self.new_entry(current, "Geri yükleme öncesi".into());
        push_page_entry(&mut self.redo_stack, entry);
        self.last_save_time = None;
        Ok(true)
    }

    pub(super) fn summaries(&self) -> Vec<PageHistorySummary> {
        self.undo_stack
            .iter()
            .rev()
            .map(|entry| PageHistorySummary {
                id: entry.id,
                description: entry.description.clone(),
                timestamp: entry.timestamp,
            })
            .collect()
    }

    fn new_entry(&mut self, snapshot: PageSnapshot, description: String) -> PageHistoryEntry {
        self.next_id += 1;
        PageHistoryEntry {
            id: self.next_id,
            description,
            timestamp: chrono::Local::now().timestamp(),
            snapshot,
        }
    }
}

fn push_page_entry(stack: &mut Vec<PageHistoryEntry>, entry: PageHistoryEntry) {
    stack.push(entry);
    if stack.len() > MAX_PAGE_HISTORY {
        stack.remove(0);
    }
}
