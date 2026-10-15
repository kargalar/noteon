use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use rusqlite::params;

use crate::config::{BLOCK_BATCH_SIZE, MAX_IMPORT_BYTES, UNTITLED};
use crate::domain::{markdown, model::PageId};
use crate::error::{Error, Result};

use super::{Repository, sql_index};

impl Repository {
    pub fn import_cover_image(&self, source: &Path, data_directory: &Path) -> Result<String> {
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
        ) {
            return Err(Error::Invalid(
                "Choose a supported image file for the cover.".into(),
            ));
        }
        if !source.is_file() {
            return Err(Error::Invalid(
                "The selected cover image could not be found.".into(),
            ));
        }
        static NEXT_COVER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = data_directory.join("attachments");
        std::fs::create_dir_all(&directory)?;
        let name = format!(
            "cover-{}-{}.{}",
            std::process::id(),
            NEXT_COVER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            extension
        );
        let destination = directory.join(name);
        std::fs::copy(source, &destination)?;
        Ok(destination.to_string_lossy().into_owned())
    }

    pub fn import_markdown(&mut self, path: &Path) -> Result<PageId> {
        let file = File::open(path)?;
        let mut source = String::new();
        file.take(MAX_IMPORT_BYTES + 1)
            .read_to_string(&mut source)?;
        if source.len() as u64 > MAX_IMPORT_BYTES {
            return Err(Error::Invalid("Markdown files cannot exceed 16 MB.".into()));
        }
        let (parsed_title, blocks) = markdown::parse(&source);
        let title = if parsed_title.is_empty() {
            path.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or(UNTITLED)
        } else {
            &parsed_title
        };
        let transaction = self.connection.transaction()?;
        transaction.execute("INSERT INTO pages(title,sort_order) SELECT ?1,COALESCE(MAX(sort_order)+1,0) FROM pages WHERE parent_id IS NULL", [title])?;
        let page = transaction.last_insert_rowid();
        {
            let mut insert = transaction.prepare_cached(
                "INSERT INTO blocks(page_id,position,kind,content,checked) VALUES(?1,?2,?3,?4,?5)",
            )?;
            for (position, (kind, content, checked)) in blocks.iter().enumerate() {
                insert.execute(params![
                    page,
                    sql_index(position)?,
                    kind.as_str(),
                    content,
                    checked
                ])?;
            }
        }
        transaction.commit()?;
        Ok(page)
    }

    pub fn export_markdown(&self, page_id: PageId, path: &Path) -> Result<()> {
        let page = self.page(page_id)?;
        let mut writer = BufWriter::new(File::create(path)?);
        writeln!(writer, "# {}\n", page.display_title())?;
        let mut numbered_index = 0;
        for start in (0..page.block_count).step_by(BLOCK_BATCH_SIZE) {
            for block in self.blocks(page_id, start, BLOCK_BATCH_SIZE)? {
                numbered_index = if block.kind == crate::domain::model::BlockKind::Numbered {
                    numbered_index + 1
                } else {
                    0
                };
                writer.write_all(markdown::render_block(&block, numbered_index).as_bytes())?;
            }
        }
        writer.flush()?;
        Ok(())
    }
}
