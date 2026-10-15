use rusqlite::{OptionalExtension, Row, params};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::model::{Block, BlockId, BlockKind, BlockStub, Document, DropPlacement, PageId};
use crate::error::{Error, Result};

use super::{Repository, read_usize, sql_index};

fn read_block(row: &Row<'_>) -> rusqlite::Result<Block> {
    let kind: String = row.get(3)?;
    Ok(Block {
        id: row.get(0)?,
        page_id: row.get(1)?,
        position: read_usize(row, 2)?,
        kind: BlockKind::from_db(&kind),
        content: row.get(4)?,
        text_styles: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
        checked: row.get(5)?,
        column_group: row.get(6)?,
        column_index: read_usize(row, 7)?,
        media_width: row.get(8)?,
    })
}

impl Repository {
    pub fn import_media_block(
        &mut self,
        page: PageId,
        position: usize,
        source: &Path,
        data_directory: &Path,
    ) -> Result<BlockKind> {
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let kind = match extension.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => BlockKind::Image,
            "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" | "wmv" => BlockKind::Video,
            _ => {
                return Err(Error::Invalid(
                    "This file type is not supported as an image or video.".into(),
                ));
            }
        };
        if !source.is_file() {
            return Err(Error::Invalid(
                "The dropped media file could not be found.".into(),
            ));
        }

        static NEXT_MEDIA_ID: AtomicU64 = AtomicU64::new(0);
        let media_directory = data_directory.join("attachments");
        std::fs::create_dir_all(&media_directory)?;
        let file_name = format!(
            "{}-{}-{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            NEXT_MEDIA_ID.fetch_add(1, Ordering::Relaxed),
            extension
        );
        let destination = media_directory.join(file_name);
        std::fs::copy(source, &destination)?;

        let result = (|| {
            let transaction = self.connection.transaction()?;
            let count: usize = transaction.query_row(
                "SELECT block_count FROM pages WHERE id=?1",
                [page],
                |row| crate::storage::read_usize(row, 0),
            )?;
            let position = sql_index(position.min(count))?;
            transaction.execute(
                "UPDATE blocks SET position=position+1 WHERE page_id=?1 AND position>=?2",
                params![page, position],
            )?;
            transaction.execute(
                "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,?3,?4)",
                params![page, position, kind.as_str(), destination.to_string_lossy()],
            )?;
            transaction.execute(
                "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
                [page],
            )?;
            transaction.commit()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&destination);
            return Err(error);
        }
        Ok(kind)
    }

    pub fn arrange_block(
        &mut self,
        page: PageId,
        source: BlockId,
        target: BlockId,
        placement: DropPlacement,
    ) -> Result<()> {
        if source == target {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        let mut statement = transaction.prepare_cached(
            "SELECT id,column_group,layout_column FROM blocks WHERE page_id=?1 ORDER BY position",
        )?;
        let rows = statement
            .query_map([page], |row| {
                Ok((
                    row.get::<_, BlockId>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    read_usize(row, 2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let ids: Vec<_> = rows.iter().map(|row| row.0).collect();
        let source_index = ids
            .iter()
            .position(|id| *id == source)
            .ok_or_else(|| Error::Invalid("Block to move was not found.".into()))?;
        let target_index = ids
            .iter()
            .position(|id| *id == target)
            .ok_or_else(|| Error::Invalid("Target block was not found.".into()))?;

        let mut order = ids.clone();
        let mut columns = std::collections::HashMap::<BlockId, (Option<i64>, usize)>::new();
        let mut widths_to_set = Vec::new();
        for (id, group, column) in &rows {
            columns.insert(*id, (*group, *column));
        }

        match placement {
            DropPlacement::Before | DropPlacement::After => {
                let source_group = rows[source_index].1;
                let target_group = rows[target_index].1;
                let source_members: Vec<_> = if let Some(group) = source_group {
                    rows.iter()
                        .filter(|row| row.1 == Some(group))
                        .map(|row| row.0)
                        .collect()
                } else {
                    vec![source]
                };
                let target_members: Vec<_> = if let Some(group) = target_group {
                    rows.iter()
                        .filter(|row| row.1 == Some(group))
                        .map(|row| row.0)
                        .collect()
                } else {
                    vec![target]
                };
                if source_members.iter().any(|id| target_members.contains(id)) {
                    transaction.commit()?;
                    return Ok(());
                }
                order.retain(|id| !source_members.contains(id));
                let target_position = order
                    .iter()
                    .position(|id| target_members.contains(id))
                    .unwrap_or(order.len());
                let insert_at = if placement == DropPlacement::After {
                    target_position + target_members.len()
                } else {
                    target_position
                };
                for (offset, id) in source_members.iter().enumerate() {
                    order.insert((insert_at + offset).min(order.len()), *id);
                }
                for id in &source_members {
                    columns.insert(*id, (None, 0));
                }
            }
            DropPlacement::Left | DropPlacement::Right => {
                let source_group = rows[source_index].1;
                let target_group = rows[target_index].1.unwrap_or(target);
                let mut target_columns: Vec<_> = rows
                    .iter()
                    .filter(|row| row.1 == Some(target_group))
                    .map(|row| (row.2, row.0))
                    .collect();
                if rows[target_index].1.is_none() {
                    target_columns.push((0, target));
                }
                target_columns.sort_by_key(|item| item.0);

                // Pull the dragged block out of its previous column while keeping
                // other blocks from that row as regular vertical content.
                if let Some(group) = source_group {
                    for (id, member_group, _) in &rows {
                        if *member_group == Some(group) && *id != source {
                            columns.insert(*id, (None, 0));
                        }
                    }
                }
                target_columns.retain(|(_, id)| *id != source);
                order.remove(source_index);
                let target_column = target_columns
                    .iter()
                    .position(|(_, id)| *id == target)
                    .unwrap_or(0);
                let insert_at = if placement == DropPlacement::Left {
                    target_column
                } else {
                    target_column + 1
                };
                target_columns.insert(insert_at.min(target_columns.len()), (0, source));
                let target_member_ids: Vec<_> = target_columns
                    .iter()
                    .filter_map(|(_, id)| (*id != source).then_some(*id))
                    .collect();
                let insertion_anchor = order
                    .iter()
                    .position(|id| target_member_ids.contains(id))
                    .unwrap_or(order.len());
                let equal_width = 1.0 / target_columns.len() as f32;
                for (column, (_, id)) in target_columns.iter().enumerate() {
                    columns.insert(*id, (Some(target_group), column));
                    widths_to_set.push((*id, equal_width));
                    let block_position = order.iter().position(|candidate| candidate == id);
                    if let Some(block_position) = block_position {
                        order.remove(block_position);
                    }
                }
                for (offset, (_, id)) in target_columns.iter().enumerate() {
                    order.insert((insertion_anchor + offset).min(order.len()), *id);
                }
            }
        }
        {
            let mut update = transaction.prepare_cached(
                "UPDATE blocks SET position=?1, column_group=?2, layout_column=?3 WHERE id=?4 AND page_id=?5",
            )?;
            for (position, id) in order.iter().enumerate() {
                let (group, column) = columns.get(id).copied().unwrap_or((None, 0));
                update.execute(params![
                    sql_index(position)?,
                    group,
                    sql_index(column)?,
                    id,
                    page
                ])?;
            }
        }
        for (block, width) in widths_to_set {
            transaction.execute(
                "UPDATE blocks SET column_width=?1 WHERE id=?2 AND page_id=?3",
                params![width, block, page],
            )?;
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn resize_columns(
        &mut self,
        page: PageId,
        group: BlockId,
        boundary: usize,
        left_width: f32,
        right_width: f32,
    ) -> Result<()> {
        if !left_width.is_finite() || !right_width.is_finite() {
            return Err(Error::Invalid("Invalid column width.".into()));
        }
        let left_width = left_width.clamp(0.001, 1.0);
        let right_width = right_width.clamp(0.001, 1.0);
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE blocks SET column_width=?1 WHERE page_id=?2 AND column_group=?3 AND layout_column=?4",
            params![left_width, page, group, sql_index(boundary)?],
        )?;
        transaction.execute(
            "UPDATE blocks SET column_width=?1 WHERE page_id=?2 AND column_group=?3 AND layout_column=?4",
            params![right_width, page, group, sql_index(boundary.saturating_add(1))?],
        )?;
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn document(&self, id: PageId) -> Result<Document> {
        let page = self.page(id)?;
        let mut statement = self.connection.prepare_cached(
            "SELECT id,kind,char_count,line_count,column_group,layout_column,column_width,media_width FROM blocks WHERE page_id=?1 ORDER BY position",
        )?;
        let outline = statement
            .query_map([id], |row| {
                let kind: String = row.get(1)?;
                Ok(BlockStub {
                    id: row.get(0)?,
                    kind: BlockKind::from_db(&kind),
                    chars: read_usize(row, 2)?,
                    lines: read_usize(row, 3)?,
                    column_group: row.get(4)?,
                    column_index: read_usize(row, 5)?,
                    column_width: row.get(6)?,
                    media_width: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Document {
            page,
            outline,
            properties: self.page_properties(id)?,
        })
    }

    pub fn blocks(&self, page: PageId, start: usize, count: usize) -> Result<Vec<Block>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT id,page_id,position,kind,content,checked,column_group,layout_column,media_width,text_styles FROM blocks WHERE page_id=?1 AND position>=?2 AND position<?3 ORDER BY position")?;
        Ok(statement
            .query_map(
                params![
                    page,
                    sql_index(start)?,
                    sql_index(start.saturating_add(count))?
                ],
                read_block,
            )?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn insert_block(
        &mut self,
        page: PageId,
        position: usize,
        kind: BlockKind,
    ) -> Result<BlockId> {
        let transaction = self.connection.transaction()?;
        let count =
            transaction.query_row("SELECT block_count FROM pages WHERE id=?1", [page], |row| {
                read_usize(row, 0)
            })?;
        let position = sql_index(position.min(count))?;
        transaction.execute(
            "UPDATE blocks SET position=position+1 WHERE page_id=?1 AND position>=?2",
            params![page, position],
        )?;
        transaction.execute(
            "INSERT INTO blocks(page_id,position,kind) VALUES(?1,?2,?3)",
            params![page, position, kind.as_str()],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn delete_block(&mut self, id: BlockId) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let (page, position): (PageId, i64) = transaction.query_row(
            "SELECT page_id,position FROM blocks WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        transaction.execute("DELETE FROM blocks WHERE id=?1", [id])?;
        transaction.execute(
            "UPDATE blocks SET position=position-1 WHERE page_id=?1 AND position>?2",
            params![page, position],
        )?;
        transaction.execute("INSERT INTO blocks(page_id,position) SELECT ?1,0 WHERE NOT EXISTS(SELECT 1 FROM blocks WHERE page_id=?1)", [page])?;
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn move_block(&mut self, id: BlockId, direction: i32) -> Result<()> {
        if ![-1, 1].contains(&direction) {
            return Err(Error::Invalid("Invalid move direction.".into()));
        }
        let (page, position): (PageId, i64) = self.connection.query_row(
            "SELECT page_id,position FROM blocks WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let target_position = position + i64::from(direction);
        let target: Option<BlockId> = self
            .connection
            .query_row(
                "SELECT id FROM blocks WHERE page_id=?1 AND position=?2",
                params![page, target_position],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(target) = target {
            self.arrange_block(
                page,
                id,
                target,
                if direction < 0 {
                    DropPlacement::Before
                } else {
                    DropPlacement::After
                },
            )?;
        }
        Ok(())
    }

    pub fn append_blocks(
        &mut self,
        page: PageId,
        position: usize,
        blocks: &[(BlockKind, String, bool)],
    ) -> Result<()> {
        self.append_blocks_inner(
            page,
            position,
            blocks
                .iter()
                .map(|(kind, content, checked)| (*kind, content.as_str(), *checked, &[][..])),
        )
    }

    pub fn append_blocks_with_styles(
        &mut self,
        page: PageId,
        position: usize,
        blocks: &[(
            BlockKind,
            String,
            bool,
            Vec<crate::domain::model::TextStyleRange>,
        )],
    ) -> Result<()> {
        self.append_blocks_inner(
            page,
            position,
            blocks.iter().map(|(kind, content, checked, styles)| {
                (*kind, content.as_str(), *checked, styles.as_slice())
            }),
        )
    }

    fn append_blocks_inner<'a>(
        &mut self,
        page: PageId,
        position: usize,
        blocks: impl ExactSizeIterator<
            Item = (
                BlockKind,
                &'a str,
                bool,
                &'a [crate::domain::model::TextStyleRange],
            ),
        >,
    ) -> Result<()> {
        let block_count = blocks.len();
        if block_count == 0 {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        let count =
            transaction.query_row("SELECT block_count FROM pages WHERE id=?1", [page], |row| {
                read_usize(row, 0)
            })?;
        let position = position.min(count);
        transaction.execute(
            "UPDATE blocks SET position=position+?1 WHERE page_id=?2 AND position>=?3",
            params![sql_index(block_count)?, page, sql_index(position)?],
        )?;
        {
            let mut insert = transaction.prepare_cached(
                "INSERT INTO blocks(page_id,position,kind,content,checked,text_styles) VALUES(?1,?2,?3,?4,?5,?6)",
            )?;
            for (offset, (kind, content, checked, text_styles)) in blocks.enumerate() {
                let text_styles = serde_json::to_string(text_styles)
                    .map_err(|error| Error::Invalid(format!("Invalid text formatting: {error}")))?;
                insert.execute(params![
                    page,
                    sql_index(position + offset)?,
                    kind.as_str(),
                    content,
                    checked,
                    text_styles
                ])?;
            }
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }
}
