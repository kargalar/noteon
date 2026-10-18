use rusqlite::{OptionalExtension, Row, params};

use crate::config::SettingKey;
use crate::domain::model::{BlockId, BlockKind, Page, PageId};
use crate::error::{Error, Result};

use super::{Repository, read_usize, sql_index};

pub(crate) const PAGE_COLUMNS: &str = "id,parent_id,title,icon,favorite,trashed_by,cover,updated_at,block_count,created_at,priority,EXISTS(SELECT 1 FROM databases WHERE page_id=pages.id),cover_image,cover_x,cover_y,sort_order,full_width";

pub(crate) fn read_page(row: &Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        title: row.get(2)?,
        icon: row.get(3)?,
        favorite: row.get(4)?,
        trashed_by: row.get(5)?,
        cover: row.get(6)?,
        updated_at: row.get(7)?,
        block_count: read_usize(row, 8)?,
        created_at: row.get(9)?,
        priority: row.get(10)?,
        is_database: row.get(11)?,
        cover_image: row.get(12)?,
        cover_x: row.get(13)?,
        cover_y: row.get(14)?,
        sort_order: row.get(15)?,
        full_width: row.get::<_, i32>(16)? != 0,
    })
}

impl Repository {
    pub fn list_pages(&self) -> Result<Vec<Page>> {
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT {PAGE_COLUMNS} FROM pages ORDER BY parent_id,sort_order,id"
        ))?;
        Ok(statement
            .query_map([], read_page)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn page(&self, id: PageId) -> Result<Page> {
        Ok(self.connection.query_row(
            &format!("SELECT {PAGE_COLUMNS} FROM pages WHERE id=?1"),
            [id],
            read_page,
        )?)
    }

    pub fn create_page(&mut self, parent: Option<PageId>, title: &str) -> Result<PageId> {
        let title = if title.trim().is_empty() {
            "Untitled"
        } else {
            title
        };
        let transaction = self.connection.transaction()?;
        if let Some(parent) = parent {
            let active: bool = transaction.query_row(
                "SELECT trashed_by IS NULL FROM pages WHERE id=?1",
                [parent],
                |row| row.get(0),
            )?;
            if !active {
                return Err(Error::Invalid(
                    "A subpage cannot be added to a page in the trash.".into(),
                ));
            }
        }
        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon,sort_order,created_at,updated_at) SELECT ?1,?2,'',COALESCE(MAX(sort_order)+1,0),unixepoch(),unixepoch() FROM pages WHERE parent_id IS ?1",
            params![parent, title],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position) VALUES(?1,0)", [id])?;
        if let Some(parent_id) = parent {
            let block_count: usize = transaction.query_row(
                "SELECT block_count FROM pages WHERE id=?1",
                [parent_id],
                |row| read_usize(row, 0),
            )?;
            let empty_id: Option<BlockId> = {
                let mut stmt = transaction.prepare(
                    "SELECT id FROM blocks WHERE page_id=?1 AND kind='text' AND content='' ORDER BY position LIMIT 1",
                )?;
                stmt.query_row([parent_id], |row| row.get(0)).optional()?
            };
            if let Some(eid) = empty_id {
                transaction.execute(
                    "UPDATE blocks SET kind='text',content=?1 WHERE id=?2",
                    params![format!("↗ {title}"), eid],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,'text',?3)",
                    params![parent_id, sql_index(block_count)?, format!("↗ {title}")],
                )?;
                transaction.execute(
                    "UPDATE pages SET block_count=block_count+1 WHERE id=?1",
                    [parent_id],
                )?;
            }
            transaction.execute(
                "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
                [parent_id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn create_subpage_block(&mut self, parent: PageId, position: usize) -> Result<PageId> {
        let transaction = self.connection.transaction()?;
        let active: bool = transaction.query_row(
            "SELECT trashed_by IS NULL FROM pages WHERE id=?1",
            [parent],
            |row| row.get(0),
        )?;
        if !active {
            return Err(Error::Invalid(
                "A subpage cannot be added to a page in the trash.".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon,sort_order,created_at,updated_at) SELECT ?1,'Untitled','',COALESCE(MAX(sort_order)+1,0),unixepoch(),unixepoch() FROM pages WHERE parent_id IS ?1",
            [parent],
        )?;
        let subpage_id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position) VALUES(?1,0)", [subpage_id])?;

        let block_count: usize = transaction.query_row(
            "SELECT block_count FROM pages WHERE id=?1",
            [parent],
            |row| read_usize(row, 0),
        )?;
        let pos = position.min(block_count);
        transaction.execute(
            "UPDATE blocks SET position=position+1 WHERE page_id=?1 AND position>=?2",
            params![parent, sql_index(pos)?],
        )?;
        transaction.execute(
            "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,'text','↗ Untitled')",
            params![parent, sql_index(pos)?],
        )?;
        transaction.execute(
            "UPDATE pages SET block_count=block_count+1,updated_at=unixepoch() WHERE id=?1",
            [parent],
        )?;
        transaction.commit()?;
        Ok(subpage_id)
    }

    pub fn create_subpage_block_replacing(
        &mut self,
        parent: PageId,
        block: BlockId,
    ) -> Result<PageId> {
        let transaction = self.connection.transaction()?;
        let active: bool = transaction.query_row(
            "SELECT trashed_by IS NULL FROM pages WHERE id=?1",
            [parent],
            |row| row.get(0),
        )?;
        if !active {
            return Err(Error::Invalid(
                "A subpage cannot be added to a page in the trash.".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon,sort_order,created_at,updated_at) SELECT ?1,'Untitled','',COALESCE(MAX(sort_order)+1,0),unixepoch(),unixepoch() FROM pages WHERE parent_id IS ?1",
            [parent],
        )?;
        let subpage_id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position) VALUES(?1,0)", [subpage_id])?;

        let belongs_to_page: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM blocks WHERE id=?1 AND page_id=?2)",
            params![block, parent],
            |row| row.get(0),
        )?;
        if belongs_to_page {
            transaction.execute(
                "UPDATE blocks SET kind='text',content='↗ Untitled',checked=0 WHERE id=?1",
                [block],
            )?;
        } else {
            let block_count: usize = transaction.query_row(
                "SELECT block_count FROM pages WHERE id=?1",
                [parent],
                |row| read_usize(row, 0),
            )?;
            transaction.execute(
                "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,'text','↗ Untitled')",
                params![parent, sql_index(block_count)?],
            )?;
            transaction.execute(
                "UPDATE pages SET block_count=block_count+1 WHERE id=?1",
                [parent],
            )?;
        }
        transaction.execute(
            "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
            [parent],
        )?;
        transaction.commit()?;
        Ok(subpage_id)
    }

    pub fn ensure_subpage_blocks(&mut self, parent_id: PageId) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let mut child_statement = transaction.prepare_cached(
            "SELECT id, title FROM pages WHERE parent_id=?1 AND trashed_by IS NULL ORDER BY sort_order, id",
        )?;
        let children = child_statement
            .query_map([parent_id], |row| {
                Ok((row.get::<_, PageId>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(child_statement);

        if children.is_empty() {
            return Ok(());
        }

        let mut block_statement = transaction.prepare_cached(
            "SELECT id, position, kind, content FROM blocks WHERE page_id=?1 ORDER BY position",
        )?;
        let mut blocks = block_statement
            .query_map([parent_id], |row| {
                Ok((
                    row.get::<_, BlockId>(0)?,
                    read_usize(row, 1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(block_statement);

        let mut changed = false;
        for (child_id, child_title) in &children {
            let clean_title = child_title.trim();
            let link_target = if clean_title.is_empty() {
                "Untitled"
            } else {
                clean_title
            };
            let expected_marker = format!("↗ {link_target}");
            let child_id_str = child_id.to_string();

            let has_matching_block = blocks.iter().any(|(_, _, kind, content)| {
                if kind == "database" && content.trim() == child_id_str {
                    true
                } else if kind == "text" && content.trim_start().starts_with('↗') {
                    let block_target = content
                        .trim_start()
                        .trim_start_matches("↗ ")
                        .trim_start_matches('↗')
                        .trim();
                    block_target.eq_ignore_ascii_case(link_target)
                        || (link_target == "Untitled" && block_target.is_empty())
                } else {
                    false
                }
            });

            if !has_matching_block {
                if let Some(pos) = blocks.iter().position(|(_, _, kind, content)| {
                    kind == "text" && content.trim().is_empty()
                }) {
                    let (block_id, _, _, _) = blocks[pos];
                    transaction.execute(
                        "UPDATE blocks SET content=?1 WHERE id=?2",
                        params![expected_marker, block_id],
                    )?;
                    blocks[pos].3 = expected_marker.clone();
                    changed = true;
                } else {
                    let next_pos: usize = transaction.query_row(
                        "SELECT COALESCE(MAX(position)+1,0) FROM blocks WHERE page_id=?1",
                        [parent_id],
                        |row| read_usize(row, 0),
                    )?;
                    transaction.execute(
                        "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,'text',?3)",
                        params![parent_id, sql_index(next_pos)?, expected_marker],
                    )?;
                    blocks.push((0, next_pos, "text".to_string(), expected_marker));
                    transaction.execute(
                        "UPDATE pages SET block_count=block_count+1 WHERE id=?1",
                        [parent_id],
                    )?;
                    changed = true;
                }
            }
        }
        if changed {
            transaction.execute(
                "UPDATE pages SET updated_at=unixepoch() WHERE id=?1",
                [parent_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn toggle_favorite(&self, id: PageId) -> Result<()> {
        self.connection
            .execute("UPDATE pages SET favorite=NOT favorite WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn trash_pages(&mut self, ids: &[PageId]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for &id in ids {
            let parent_info: Option<(PageId, String)> = transaction
                .query_row(
                    "SELECT parent_id, title FROM pages WHERE id=?1 AND parent_id IS NOT NULL",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();
            if let Some((parent_id, title)) = parent_info {
                let clean_title = title.trim();
                let marker = format!("↗ {clean_title}");
                transaction.execute(
                    "DELETE FROM blocks WHERE page_id=?1 AND kind='text' AND (content=?2 OR (content='↗ Untitled' AND ?3=''))",
                    params![parent_id, marker, clean_title],
                )?;
                let count: usize = transaction.query_row(
                    "SELECT COUNT(*) FROM blocks WHERE page_id=?1",
                    [parent_id],
                    |row| read_usize(row, 0),
                )?;
                transaction.execute("UPDATE pages SET block_count=?1 WHERE id=?2", params![count as i64, parent_id])?;
            }
            transaction.execute(
                "WITH RECURSIVE descendants(id) AS (SELECT id FROM pages WHERE id=?1 UNION ALL SELECT p.id FROM pages p JOIN descendants d ON p.parent_id=d.id)
                 UPDATE pages SET trashed_by=?1, updated_at=unixepoch() WHERE id IN (SELECT id FROM descendants) AND trashed_by IS NULL", [id])?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn trash_page(&mut self, id: PageId) -> Result<()> {
        self.trash_pages(&[id])
    }

    pub fn restore_pages(&mut self, ids: &[PageId]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for &id in ids {
            let group: Option<PageId> = transaction
                .query_row("SELECT trashed_by FROM pages WHERE id=?1", [id], |row| row.get(0))
                .optional()?
                .flatten();
            if group == Some(id) {
                transaction.execute("UPDATE pages SET parent_id=NULL WHERE id=?1 AND parent_id IN (SELECT id FROM pages WHERE trashed_by IS NOT NULL AND trashed_by!=?1)", [id])?;
                transaction.execute("UPDATE pages SET trashed_by=NULL WHERE trashed_by=?1", [id])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_pages_permanently(&mut self, ids: &[PageId]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for &id in ids {
            transaction.execute(
                "WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM pages WHERE id=?1
                    UNION ALL
                    SELECT p.id FROM pages p JOIN descendants d ON p.parent_id=d.id
                 )
                 DELETE FROM pages WHERE id IN (SELECT id FROM descendants) OR trashed_by=?1",
                [id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn empty_trash(&mut self) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM pages WHERE trashed_by IS NOT NULL", [])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn restore_page(&mut self, id: PageId) -> Result<()> {
        let transaction = self.connection.transaction()?;
        // Restore only this deletion group; independently trashed descendants stay in trash.
        let group: Option<PageId> =
            transaction.query_row("SELECT trashed_by FROM pages WHERE id=?1", [id], |row| {
                row.get(0)
            })?;
        if group != Some(id) {
            return Err(Error::Invalid(
                "Restore this page together with its parent page.".into(),
            ));
        }
        transaction.execute("UPDATE pages SET parent_id=NULL WHERE id=?1 AND parent_id IN (SELECT id FROM pages WHERE trashed_by IS NOT NULL AND trashed_by!=?1)", [id])?;
        transaction.execute("UPDATE pages SET trashed_by=NULL WHERE trashed_by=?1", [id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn move_page(&self, id: PageId, parent: Option<PageId>) -> Result<()> {
        if let Some(parent) = parent {
            let invalid: bool = self.connection.query_row(
                "WITH RECURSIVE descendants(id) AS (SELECT ?1 UNION ALL SELECT p.id FROM pages p JOIN descendants d ON p.parent_id=d.id)
                 SELECT EXISTS(SELECT 1 FROM descendants WHERE id=?2) OR NOT EXISTS(SELECT 1 FROM pages WHERE id=?2 AND trashed_by IS NULL)",
                params![id, parent], |row| row.get(0))?;
            if invalid {
                return Err(Error::Invalid(
                    "A page cannot be moved into itself or one of its subpages.".into(),
                ));
            }
        }
        self.connection.execute(
            "UPDATE pages SET parent_id=?1,sort_order=(SELECT COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1 AND id!=?2) WHERE id=?2 AND trashed_by IS NULL",
            params![parent, id],
        )?;
        Ok(())
    }

    pub fn reorder_page(&mut self, source: PageId, target: PageId, after: bool) -> Result<()> {
        if source == target {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        let source_parent: Option<PageId> = transaction.query_row(
            "SELECT parent_id FROM pages WHERE id=?1 AND trashed_by IS NULL",
            [source],
            |row| row.get(0),
        )?;
        let target_parent: Option<PageId> = transaction.query_row(
            "SELECT parent_id FROM pages WHERE id=?1 AND trashed_by IS NULL",
            [target],
            |row| row.get(0),
        )?;
        if source_parent != target_parent {
            return Err(Error::Invalid(
                "Pages can only be reordered with their siblings.".into(),
            ));
        }
        let mut statement = transaction.prepare_cached(
            "SELECT id FROM pages WHERE parent_id IS ?1 AND trashed_by IS NULL ORDER BY sort_order,id",
        )?;
        let mut siblings = statement
            .query_map([source_parent], |row| row.get::<_, PageId>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        siblings.retain(|id| *id != source);
        let target_index = siblings
            .iter()
            .position(|id| *id == target)
            .ok_or_else(|| Error::Invalid("The target page was not found.".into()))?;
        let insert_index = target_index + usize::from(after);
        siblings.insert(insert_index, source);
        let mut update =
            transaction.prepare_cached("UPDATE pages SET sort_order=?1 WHERE id=?2")?;
        for (index, id) in siblings.into_iter().enumerate() {
            update.execute(params![sql_index(index)?, id])?;
        }
        drop(update);
        transaction.commit()?;
        Ok(())
    }

    pub fn duplicate_page(&mut self, id: PageId) -> Result<PageId> {
        let transaction = self.connection.transaction()?;
        transaction.execute("INSERT INTO pages(parent_id,title,icon,cover,cover_image,cover_x,cover_y,sort_order,full_width,created_at,updated_at) SELECT source.parent_id,source.title || ' (copy)',source.icon,source.cover,source.cover_image,source.cover_x,source.cover_y,(SELECT COALESCE(MAX(sibling.sort_order)+1,0) FROM pages sibling WHERE sibling.parent_id IS source.parent_id),source.full_width,unixepoch(),unixepoch() FROM pages source WHERE source.id=?1", [id])?;
        let new_id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles) SELECT ?1,position,kind,content,checked,column_group,layout_column,column_width,media_width,text_styles FROM blocks WHERE page_id=?2 ORDER BY position", params![new_id, id])?;
        transaction.commit()?;
        Ok(new_id)
    }

    pub fn seed_welcome(&mut self) -> Result<()> {
        let exists: bool =
            self.connection
                .query_row("SELECT EXISTS(SELECT 1 FROM pages)", [], |row| row.get(0))?;
        if exists {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO pages(title,icon,favorite,cover,created_at,updated_at) VALUES('Make room for your thoughts.','✦',1,1,unixepoch(),unixepoch())",
            [],
        )?;
        let welcome = transaction.last_insert_rowid();
        let intro = [
            (
                BlockKind::Callout,
                "A space to think. No internet, no account. Just you and your ideas.",
            ),
            (BlockKind::Heading2, "Big ideas start with a small note."),
            (
                BlockKind::Text,
                "Plan a project, capture an idea, or learn something new. Every page is yours, and every change is saved automatically.",
            ),
            (BlockKind::Heading3, "Getting started"),
            (
                BlockKind::Todo,
                "Create a page with + New page in the top-left corner.",
            ),
            (
                BlockKind::Todo,
                "Type /todo or /h1 on a line to change its block type.",
            ),
            (BlockKind::Todo, "Search all your notes with Ctrl + K."),
            (BlockKind::Divider, ""),
            (
                BlockKind::Quote,
                "Keep your mind free to think, not to remember.",
            ),
            (
                BlockKind::Text,
                "Tip: Enter creates a new block. Shift + Enter adds a line in the same block. Use the menu on the left to move or transform blocks.",
            ),
        ];
        for (position, (kind, content)) in intro.into_iter().enumerate() {
            transaction.execute(
                "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,?3,?4)",
                params![welcome, sql_index(position)?, kind.as_str(), content],
            )?;
        }
        transaction.execute(
            "INSERT INTO pages(title,icon,cover,created_at,updated_at) VALUES('Idea notebook','◇',2,unixepoch(),unixepoch())",
            [],
        )?;
        let ideas = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position,content) VALUES(?1,0,'Make room for ideas that are still taking shape.')", [ideas])?;
        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon,created_at,updated_at) VALUES(?1,'The next big idea','○',unixepoch(),unixepoch())",
            [ideas],
        )?;
        let child = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position) VALUES(?1,0)", [child])?;
        transaction.execute(
            "INSERT INTO pages(title,icon,cover,created_at,updated_at) VALUES('Weekly plan','☑',3,unixepoch(),unixepoch())",
            [],
        )?;
        let plan = transaction.last_insert_rowid();
        for (position, content) in [
            "What do I want to focus on this week?",
            "Set my priorities",
            "Make time for myself",
        ]
        .into_iter()
        .enumerate()
        {
            let kind = if position == 0 { "h2" } else { "todo" };
            transaction.execute(
                "INSERT INTO blocks(page_id,position,kind,content) VALUES(?1,?2,?3,?4)",
                params![plan, sql_index(position)?, kind, content],
            )?;
        }
        transaction.execute(
            "INSERT INTO settings(key,value) VALUES('last_page',?1)",
            [welcome.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn last_page(&self) -> Result<Option<PageId>> {
        let id = self
            .setting(SettingKey::LastPage)?
            .and_then(|value| value.parse::<PageId>().ok());
        if let Some(id) = id {
            return Ok(self
                .connection
                .query_row(
                    "SELECT id FROM pages WHERE id=?1 AND trashed_by IS NULL",
                    [id],
                    |row| row.get(0),
                )
                .optional()?);
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_page_has_no_icon() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let mut repository = Repository::open(&db_path).unwrap();
        let page_id = repository.create_page(None, "New Page").unwrap();
        let page = repository.page(page_id).unwrap();
        assert_eq!(page.icon, "");
    }

    #[test]
    fn created_subpage_block_has_no_icon() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.sqlite");
        let mut repository = Repository::open(&db_path).unwrap();
        let parent_id = repository.create_page(None, "Parent Page").unwrap();
        let subpage_id = repository.create_subpage_block(parent_id, 0).unwrap();
        let subpage = repository.page(subpage_id).unwrap();
        assert_eq!(subpage.icon, "");
    }
}
