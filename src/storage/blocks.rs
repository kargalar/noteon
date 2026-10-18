use rusqlite::{OptionalExtension, Row, params};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::model::{
    Block, BlockId, BlockKind, BlockStub, BulkBlockStyleOp, Document, DropPlacement,
    InlineTextStyle, PageId, PageSnapshot, TextStyleRange,
};
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
        self.arrange_blocks(page, &[source], target, placement)
    }

    pub fn arrange_blocks(
        &mut self,
        page: PageId,
        sources: &[BlockId],
        target: BlockId,
        placement: DropPlacement,
    ) -> Result<()> {
        if sources.is_empty() || sources.contains(&target) {
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

        let mut valid_sources: Vec<BlockId> = Vec::new();
        for &s in sources {
            if ids.contains(&s) && s != target && !valid_sources.contains(&s) {
                valid_sources.push(s);
            }
        }
        if valid_sources.is_empty() {
            return Ok(());
        }
        valid_sources.sort_by_key(|s| ids.iter().position(|id| id == s).unwrap_or(usize::MAX));

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
                let target_group = rows[target_index].1;
                let target_members: Vec<_> = if let Some(group) = target_group {
                    rows.iter()
                        .filter(|row| row.1 == Some(group))
                        .map(|row| row.0)
                        .collect()
                } else {
                    vec![target]
                };

                let (source_blocks, preserve_source_columns) = if sources.len() == 1 {
                    let source = sources[0];
                    let source_index = ids.iter().position(|id| *id == source).unwrap();
                    if let Some(group) = rows[source_index].1 {
                        let members: Vec<_> = rows
                            .iter()
                            .filter(|row| row.1 == Some(group))
                            .map(|row| row.0)
                            .collect();
                        (members, true)
                    } else {
                        (vec![source], false)
                    }
                } else {
                    (valid_sources.clone(), false)
                };

                if source_blocks.iter().any(|id| target_members.contains(id)) {
                    transaction.commit()?;
                    return Ok(());
                }
                order.retain(|id| !source_blocks.contains(id));
                let target_position = order
                    .iter()
                    .position(|id| target_members.contains(id))
                    .unwrap_or(order.len());
                let insert_at = if placement == DropPlacement::After {
                    target_position + target_members.len()
                } else {
                    target_position
                };
                for (offset, id) in source_blocks.iter().enumerate() {
                    order.insert((insert_at + offset).min(order.len()), *id);
                    if !preserve_source_columns {
                        columns.insert(*id, (None, 0));
                    }
                }

                if !preserve_source_columns {
                    let mut affected_groups = std::collections::HashSet::new();
                    for id in &source_blocks {
                        if let Some((Some(grp), _)) = rows.iter().find(|r| r.0 == *id).map(|r| (r.1, r.2)) {
                            affected_groups.insert(grp);
                        }
                    }
                    for grp in affected_groups {
                        let remaining: Vec<BlockId> = rows
                            .iter()
                            .filter(|row| row.1 == Some(grp) && !source_blocks.contains(&row.0))
                            .map(|row| row.0)
                            .collect();
                        if remaining.len() <= 1 {
                            for rem_id in remaining {
                                columns.insert(rem_id, (None, 0));
                            }
                        } else {
                            let equal_w = 1.0 / remaining.len() as f32;
                            for (col_idx, rem_id) in remaining.iter().enumerate() {
                                columns.insert(*rem_id, (Some(grp), col_idx));
                                widths_to_set.push((*rem_id, equal_w));
                            }
                        }
                    }
                }
            }
            DropPlacement::Left | DropPlacement::Right => {
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

                let mut affected_groups = std::collections::HashSet::new();
                for id in &valid_sources {
                    if let Some((Some(grp), _)) = rows.iter().find(|r| r.0 == *id).map(|r| (r.1, r.2)) {
                        if grp != target_group {
                            affected_groups.insert(grp);
                        }
                    }
                }
                for grp in affected_groups {
                    let remaining: Vec<BlockId> = rows
                        .iter()
                        .filter(|row| row.1 == Some(grp) && !valid_sources.contains(&row.0))
                        .map(|row| row.0)
                        .collect();
                    if remaining.len() <= 1 {
                        for rem_id in remaining {
                            columns.insert(rem_id, (None, 0));
                        }
                    } else {
                        let equal_w = 1.0 / remaining.len() as f32;
                        for (col_idx, rem_id) in remaining.iter().enumerate() {
                            columns.insert(*rem_id, (Some(grp), col_idx));
                            widths_to_set.push((*rem_id, equal_w));
                        }
                    }
                }

                target_columns.retain(|(_, id)| !valid_sources.contains(id));
                order.retain(|id| !valid_sources.contains(id));

                let target_column = target_columns
                    .iter()
                    .position(|(_, id)| *id == target)
                    .unwrap_or(0);
                let insert_at = if placement == DropPlacement::Left {
                    target_column
                } else {
                    target_column + 1
                };
                for (offset, id) in valid_sources.iter().enumerate() {
                    target_columns.insert((insert_at + offset).min(target_columns.len()), (0, *id));
                }
                let target_member_ids: Vec<_> = target_columns
                    .iter()
                    .filter_map(|(_, id)| (!valid_sources.contains(id)).then_some(*id))
                    .collect();
                let insertion_anchor = order
                    .iter()
                    .position(|id| target_member_ids.contains(id))
                    .unwrap_or(order.len());
                let equal_width = 1.0 / target_columns.len() as f32;
                for (column, (_, id)) in target_columns.iter().enumerate() {
                    columns.insert(*id, (Some(target_group), column));
                    widths_to_set.push((*id, equal_width));
                    if let Some(pos) = order.iter().position(|candidate| candidate == id) {
                        order.remove(pos);
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

    pub fn page_snapshot(&self, page_id: PageId) -> Result<PageSnapshot> {
        let page = self.page(page_id)?;
        let mut statement = self.connection.prepare_cached(
            "SELECT id,page_id,position,kind,content,checked,column_group,layout_column,media_width,text_styles FROM blocks WHERE page_id=?1 ORDER BY position",
        )?;
        let blocks = statement
            .query_map([page_id], read_block)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(PageSnapshot {
            page_id,
            title: page.title,
            icon: page.icon,
            blocks,
        })
    }

    pub fn restore_page_snapshot(&mut self, snapshot: &PageSnapshot) -> Result<Document> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE pages SET title=?1, icon=?2, updated_at=unixepoch() WHERE id=?3",
            params![snapshot.title, snapshot.icon, snapshot.page_id],
        )?;
        transaction.execute("DELETE FROM blocks WHERE page_id=?1", [snapshot.page_id])?;
        if snapshot.blocks.is_empty() {
            transaction.execute(
                "INSERT INTO blocks(page_id,position) VALUES(?1,0)",
                [snapshot.page_id],
            )?;
        } else {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO blocks(id, page_id, position, kind, content, checked, column_group, layout_column, media_width, text_styles)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for (idx, block) in snapshot.blocks.iter().enumerate() {
                statement.execute(params![
                    block.id,
                    block.page_id,
                    sql_index(idx)?,
                    block.kind.as_str(),
                    block.content,
                    block.checked,
                    block.column_group,
                    sql_index(block.column_index)?,
                    block.media_width,
                    serde_json::to_string(&block.text_styles).unwrap_or_else(|_| "[]".into()),
                ])?;
            }
        }
        transaction.commit()?;
        self.document(snapshot.page_id)
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
        let page: PageId = self.connection.query_row(
            "SELECT page_id FROM blocks WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        self.delete_blocks(page, &[id])
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

    pub fn delete_blocks(&mut self, page: PageId, ids: &[BlockId]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        {
            let mut check_stmt = transaction.prepare_cached(
                "SELECT content FROM blocks WHERE id=?1 AND page_id=?2 AND kind='text'",
            )?;
            for id in ids {
                if let Ok(content) =
                    check_stmt.query_row(params![id, page], |row| row.get::<_, String>(0))
                {
                    if content.trim_start().starts_with('↗') {
                        let title = content
                            .trim_start()
                            .trim_start_matches("↗ ")
                            .trim_start_matches('↗')
                            .trim();
                        let child_id: Option<PageId> = transaction
                            .query_row(
                                "SELECT id FROM pages WHERE parent_id=?1 AND trashed_by IS NULL AND (title=?2 OR (?2='Untitled' AND title='')) LIMIT 1",
                                params![page, title],
                                |row| row.get(0),
                            )
                            .ok();
                        if let Some(cid) = child_id {
                            transaction.execute(
                                "UPDATE pages SET trashed_by=?1 WHERE id=?1",
                                [cid],
                            )?;
                        }
                    }
                }
            }
            let mut delete_stmt =
                transaction.prepare_cached("DELETE FROM blocks WHERE id=?1 AND page_id=?2")?;
            for id in ids {
                delete_stmt.execute(params![id, page])?;
            }
        }
        normalize_page_blocks(&transaction, page)?;
        transaction.execute(
            "INSERT INTO blocks(page_id,position) SELECT ?1,0 WHERE NOT EXISTS(SELECT 1 FROM blocks WHERE page_id=?1)",
            [page],
        )?;
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_blocks_kind(
        &mut self,
        page: PageId,
        ids: &[BlockId],
        kind: BlockKind,
    ) -> Result<()> {
        if ids.is_empty() || !BlockKind::ALL.contains(&kind) {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        {
            let mut stmt = transaction.prepare_cached(
                "UPDATE blocks SET kind=?1 WHERE id=?2 AND page_id=?3 AND kind NOT IN ('database','image','video')",
            )?;
            for id in ids {
                stmt.execute(params![kind.as_str(), id, page])?;
            }
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_blocks_checked(
        &mut self,
        page: PageId,
        ids: &[BlockId],
        checked: bool,
    ) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        {
            let mut stmt = transaction
                .prepare_cached("UPDATE blocks SET checked=?1 WHERE id=?2 AND page_id=?3")?;
            for id in ids {
                stmt.execute(params![checked, id, page])?;
            }
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn format_blocks(
        &mut self,
        page: PageId,
        ids: &[BlockId],
        op: BulkBlockStyleOp,
    ) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        let mut target_blocks = Vec::new();
        {
            let mut select = transaction.prepare_cached(
                "SELECT id,content,text_styles,kind FROM blocks WHERE id=?1 AND page_id=?2",
            )?;
            for id in ids {
                if let Some((block_id, content, raw_styles, kind_str)) = select
                    .query_row(params![id, page], |row| {
                        Ok((
                            row.get::<_, BlockId>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })
                    .optional()?
                {
                    let kind = BlockKind::from_db(&kind_str);
                    if !matches!(
                        kind,
                        BlockKind::Divider
                            | BlockKind::Database
                            | BlockKind::Image
                            | BlockKind::Video
                    ) {
                        let styles: Vec<TextStyleRange> =
                            serde_json::from_str(&raw_styles).unwrap_or_default();
                        target_blocks.push((block_id, content, styles));
                    }
                }
            }
        }
        if target_blocks.is_empty() {
            return Ok(());
        }

        let toggle_target = match op {
            BulkBlockStyleOp::ToggleBold
            | BulkBlockStyleOp::ToggleItalic
            | BulkBlockStyleOp::ToggleUnderline
            | BulkBlockStyleOp::ToggleStrikethrough => {
                let non_empty: Vec<_> = target_blocks
                    .iter()
                    .filter(|(_, content, _)| !content.is_empty())
                    .collect();
                let all_enabled = !non_empty.is_empty()
                    && non_empty.iter().all(|(_, content, styles)| {
                        let len = content.chars().count();
                        let expanded = expand_styles(styles, len);
                        expanded.iter().all(|s| match op {
                            BulkBlockStyleOp::ToggleBold => s.bold,
                            BulkBlockStyleOp::ToggleItalic => s.italic,
                            BulkBlockStyleOp::ToggleUnderline => s.underline,
                            BulkBlockStyleOp::ToggleStrikethrough => s.strikethrough,
                            _ => false,
                        })
                    });
                !all_enabled
            }
            _ => false,
        };

        {
            let mut update = transaction.prepare_cached(
                "UPDATE blocks SET text_styles=?1 WHERE id=?2 AND page_id=?3",
            )?;
            for (block_id, content, styles) in target_blocks {
                let len = content.chars().count();
                let next_styles = if matches!(op, BulkBlockStyleOp::ClearFormatting) || len == 0 {
                    Vec::new()
                } else {
                    let mut expanded = expand_styles(&styles, len);
                    for style in &mut expanded {
                        match op {
                            BulkBlockStyleOp::ToggleBold => style.bold = toggle_target,
                            BulkBlockStyleOp::ToggleItalic => style.italic = toggle_target,
                            BulkBlockStyleOp::ToggleUnderline => style.underline = toggle_target,
                            BulkBlockStyleOp::ToggleStrikethrough => {
                                style.strikethrough = toggle_target
                            }
                            BulkBlockStyleOp::SetFont(family) => style.font_family = family,
                            BulkBlockStyleOp::SetTextColor(color) => style.foreground = color,
                            BulkBlockStyleOp::SetHighlight(color) => style.background = color,
                            BulkBlockStyleOp::ClearFormatting => {}
                        }
                    }
                    compress_styles(&expanded)
                };
                let encoded = serde_json::to_string(&next_styles)
                    .map_err(|error| Error::Invalid(format!("Invalid text formatting: {error}")))?;
                update.execute(params![encoded, block_id, page])?;
            }
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn move_blocks(&mut self, page: PageId, ids: &[BlockId], direction: i32) -> Result<()> {
        if ids.is_empty() || ![-1, 1].contains(&direction) {
            return Ok(());
        }
        let selected: std::collections::HashSet<BlockId> = ids.iter().copied().collect();
        let transaction = self.connection.transaction()?;
        let mut order: Vec<BlockId> = {
            let mut stmt = transaction
                .prepare_cached("SELECT id FROM blocks WHERE page_id=?1 ORDER BY position")?;
            let rows = stmt
                .query_map([page], |row| row.get::<_, BlockId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        if direction < 0 {
            for index in 1..order.len() {
                if selected.contains(&order[index]) && !selected.contains(&order[index - 1]) {
                    order.swap(index - 1, index);
                }
            }
        } else {
            for index in (0..order.len().saturating_sub(1)).rev() {
                if selected.contains(&order[index]) && !selected.contains(&order[index + 1]) {
                    order.swap(index, index + 1);
                }
            }
        }
        {
            let mut update = transaction
                .prepare_cached("UPDATE blocks SET position=?1 WHERE id=?2 AND page_id=?3")?;
            for (position, id) in order.iter().enumerate() {
                update.execute(params![sql_index(position)?, id, page])?;
            }
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn duplicate_blocks(&mut self, page: PageId, ids: &[BlockId]) -> Result<Vec<BlockId>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let selected: std::collections::HashSet<BlockId> = ids.iter().copied().collect();
        let transaction = self.connection.transaction()?;
        let rows = {
            let mut stmt = transaction.prepare_cached(
                "SELECT id,position,kind,content,checked,media_width,text_styles FROM blocks WHERE page_id=?1 ORDER BY position",
            )?;
            let mapped = stmt
                .query_map([page], |row| {
                    Ok((
                        row.get::<_, BlockId>(0)?,
                        read_usize(row, 1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, bool>(4)?,
                        row.get::<_, f32>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            mapped
        };
        let to_duplicate: Vec<_> = rows
            .into_iter()
            .filter(|row| selected.contains(&row.0) && row.2 != "database")
            .collect();
        if to_duplicate.is_empty() {
            return Ok(Vec::new());
        }
        let insert_at = to_duplicate
            .last()
            .map(|row| row.1 + 1)
            .unwrap_or(0);
        let dup_count = to_duplicate.len();
        transaction.execute(
            "UPDATE blocks SET position=position+?1 WHERE page_id=?2 AND position>=?3",
            params![sql_index(dup_count)?, page, sql_index(insert_at)?],
        )?;
        let mut created_ids = Vec::with_capacity(dup_count);
        {
            let mut insert = transaction.prepare_cached(
                "INSERT INTO blocks(page_id,position,kind,content,checked,media_width,text_styles) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            )?;
            for (offset, (_, _, kind, content, checked, media_width, text_styles)) in
                to_duplicate.into_iter().enumerate()
            {
                insert.execute(params![
                    page,
                    sql_index(insert_at + offset)?,
                    kind,
                    content,
                    checked,
                    media_width.clamp(0.25, 1.0),
                    text_styles
                ])?;
                created_ids.push(transaction.last_insert_rowid());
            }
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(created_ids)
    }

    pub fn group_blocks_in_columns(&mut self, page: PageId, ids: &[BlockId]) -> Result<()> {
        if !(2..=4).contains(&ids.len()) {
            return Ok(());
        }
        let selected: std::collections::HashSet<BlockId> = ids.iter().copied().collect();
        let transaction = self.connection.transaction()?;
        let rows = {
            let mut stmt = transaction.prepare_cached(
                "SELECT id,kind FROM blocks WHERE page_id=?1 ORDER BY position",
            )?;
            let mapped = stmt
                .query_map([page], |row| {
                    Ok((row.get::<_, BlockId>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            mapped
        };
        let target_ids: Vec<BlockId> = rows
            .iter()
            .filter(|(id, kind)| selected.contains(id) && kind != "database")
            .map(|(id, _)| *id)
            .collect();
        if !(2..=4).contains(&target_ids.len()) {
            return Ok(());
        }
        let group_id = target_ids[0];
        let anchor_index = rows
            .iter()
            .position(|(id, _)| *id == group_id)
            .unwrap_or(0);
        let mut order: Vec<BlockId> = rows.iter().map(|(id, _)| *id).collect();
        order.retain(|id| !target_ids.contains(id));
        let insert_at = anchor_index.min(order.len());
        for (offset, id) in target_ids.iter().enumerate() {
            order.insert(insert_at + offset, *id);
        }
        let equal_width = (1.0 / target_ids.len() as f32).clamp(0.08, 1.0);
        {
            let mut update_pos = transaction
                .prepare_cached("UPDATE blocks SET position=?1 WHERE id=?2 AND page_id=?3")?;
            for (position, id) in order.iter().enumerate() {
                update_pos.execute(params![sql_index(position)?, id, page])?;
            }
            let mut update_col = transaction.prepare_cached(
                "UPDATE blocks SET column_group=?1,layout_column=?2,column_width=?3 WHERE id=?4 AND page_id=?5",
            )?;
            for (col, id) in target_ids.iter().enumerate() {
                update_col.execute(params![
                    Some(group_id),
                    sql_index(col)?,
                    equal_width,
                    id,
                    page
                ])?;
            }
        }
        normalize_page_blocks(&transaction, page)?;
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn ungroup_block_columns(&mut self, page: PageId, ids: &[BlockId]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        {
            let mut stmt = transaction.prepare_cached(
                "UPDATE blocks SET column_group=NULL,layout_column=0,column_width=1.0 WHERE id=?1 AND page_id=?2",
            )?;
            for id in ids {
                stmt.execute(params![id, page])?;
            }
        }
        normalize_page_blocks(&transaction, page)?;
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [page],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn normalize_page_blocks(transaction: &rusqlite::Transaction<'_>, page: PageId) -> Result<()> {
    let rows = {
        let mut stmt = transaction.prepare_cached(
            "SELECT id,column_group,column_width FROM blocks WHERE page_id=?1 ORDER BY position",
        )?;
        let mapped = stmt
            .query_map([page], |row| {
                Ok((
                    row.get::<_, BlockId>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, f32>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        mapped
    };
    let mut group_counts = std::collections::HashMap::<i64, usize>::new();
    for (_, group, _) in &rows {
        if let Some(group) = group {
            *group_counts.entry(*group).or_insert(0) += 1;
        }
    }
    let mut group_next_col = std::collections::HashMap::<i64, usize>::new();
    let mut update = transaction.prepare_cached(
        "UPDATE blocks SET position=?1,column_group=?2,layout_column=?3,column_width=?4 WHERE id=?5 AND page_id=?6",
    )?;
    for (position, (id, group, width)) in rows.into_iter().enumerate() {
        let (next_group, next_col, next_width) = match group {
            Some(g) if group_counts.get(&g).copied().unwrap_or(0) > 1 => {
                let col = group_next_col.entry(g).or_insert(0);
                let current_col = *col;
                *col += 1;
                (Some(g), current_col, width.clamp(0.08, 1.0))
            }
            _ => (None, 0, 1.0),
        };
        update.execute(params![
            sql_index(position)?,
            next_group,
            sql_index(next_col)?,
            next_width,
            id,
            page
        ])?;
    }
    Ok(())
}

fn expand_styles(styles: &[TextStyleRange], length: usize) -> Vec<InlineTextStyle> {
    let mut expanded = vec![InlineTextStyle::default(); length];
    for range in styles {
        let start = range.start.min(length);
        let end = range.end.min(length);
        if start >= end {
            continue;
        }
        for style in &mut expanded[start..end] {
            style.clone_from(&range.style);
        }
    }
    expanded
}

fn compress_styles(styles: &[InlineTextStyle]) -> Vec<TextStyleRange> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < styles.len() {
        let style = &styles[start];
        if style == &InlineTextStyle::default() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < styles.len() && styles[end] == *style {
            end += 1;
        }
        ranges.push(TextStyleRange {
            start,
            end,
            style: style.clone(),
        });
        start = end;
    }
    ranges
}

