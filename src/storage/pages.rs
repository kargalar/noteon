use rusqlite::{OptionalExtension, Row, params};

use crate::config::SettingKey;
use crate::domain::model::{BlockKind, Page, PageId};
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
            "INSERT INTO pages(parent_id,title,sort_order) SELECT ?1,?2,COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS ?1",
            params![parent, title],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position) VALUES(?1,0)", [id])?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn toggle_favorite(&self, id: PageId) -> Result<()> {
        self.connection
            .execute("UPDATE pages SET favorite=NOT favorite WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn trash_page(&self, id: PageId) -> Result<()> {
        self.connection.execute(
            "WITH RECURSIVE descendants(id) AS (SELECT id FROM pages WHERE id=?1 UNION ALL SELECT p.id FROM pages p JOIN descendants d ON p.parent_id=d.id)
             UPDATE pages SET trashed_by=?1 WHERE id IN (SELECT id FROM descendants) AND trashed_by IS NULL", [id])?;
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
        transaction.execute("INSERT INTO pages(parent_id,title,icon,cover,cover_image,cover_x,cover_y,sort_order,full_width) SELECT source.parent_id,source.title || ' (copy)',source.icon,source.cover,source.cover_image,source.cover_x,source.cover_y,(SELECT COALESCE(MAX(sibling.sort_order)+1,0) FROM pages sibling WHERE sibling.parent_id IS source.parent_id),source.full_width FROM pages source WHERE source.id=?1", [id])?;
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
            "INSERT INTO pages(title,icon,favorite,cover) VALUES('Make room for your thoughts.','✦',1,1)",
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
            "INSERT INTO pages(title,icon,cover) VALUES('Idea notebook','◇',2)",
            [],
        )?;
        let ideas = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position,content) VALUES(?1,0,'Make room for ideas that are still taking shape.')", [ideas])?;
        transaction.execute(
            "INSERT INTO pages(parent_id,title,icon) VALUES(?1,'The next big idea','○')",
            [ideas],
        )?;
        let child = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO blocks(page_id,position) VALUES(?1,0)", [child])?;
        transaction.execute(
            "INSERT INTO pages(title,icon,cover) VALUES('Weekly plan','☑',3)",
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
