use rusqlite::params;

use crate::config::{SEARCH_LIMIT, UNTITLED};
use crate::domain::model::SearchHit;
use crate::error::Result;

use super::{Repository, read_usize, sql_index};

/// Quote every token before constructing MATCH. User input is never FTS syntax.
pub fn search_expression(query: &str) -> Option<String> {
    let words: Vec<_> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(16)
        .map(|word| format!("\"{word}\"*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" AND "))
}

impl Repository {
    pub fn search(&self, query: &str) -> Result<Vec<SearchHit>> {
        let Some(expression) = search_expression(query) else {
            return Ok(Vec::new());
        };
        // Title matches come first. Avoid ranking every match for common words;
        // LIMIT can stop the FTS index traversal as soon as enough hits are found.
        let mut pages = self.connection.prepare_cached(
            "SELECT p.id,p.title,p.icon FROM page_search JOIN pages p ON p.id=page_search.rowid WHERE page_search MATCH ?1 AND p.trashed_by IS NULL LIMIT ?2")?;
        let mut hits = pages
            .query_map(params![expression, sql_index(SEARCH_LIMIT)?], |row| {
                let title: String = row.get(1)?;
                Ok(SearchHit {
                    page_id: row.get(0)?,
                    title: if title.trim().is_empty() {
                        UNTITLED.into()
                    } else {
                        title
                    },
                    icon: row.get(2)?,
                    preview: "Page title".into(),
                    position: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let remaining = SEARCH_LIMIT.saturating_sub(hits.len());
        if remaining > 0 {
            let mut blocks = self.connection.prepare_cached(
                "SELECT p.id,p.title,p.icon,snippet(block_search,0,'','',' … ',24),b.position FROM block_search
                 JOIN blocks b ON b.id=block_search.rowid JOIN pages p ON p.id=b.page_id
                 WHERE block_search MATCH ?1 AND p.trashed_by IS NULL LIMIT ?2")?;
            hits.extend(
                blocks
                    .query_map(params![expression, sql_index(remaining)?], |row| {
                        let title: String = row.get(1)?;
                        Ok(SearchHit {
                            page_id: row.get(0)?,
                            title: if title.trim().is_empty() {
                                UNTITLED.into()
                            } else {
                                title
                            },
                            icon: row.get(2)?,
                            preview: row.get(3)?,
                            position: Some(read_usize(row, 4)?),
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        Ok(hits)
    }
}
