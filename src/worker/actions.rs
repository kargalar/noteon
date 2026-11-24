use std::collections::{HashMap, HashSet};

use crate::domain::model::{BlockId, DropPlacement, PageId};
use crate::error::{Error, Result};
use crate::storage::Repository;

use super::history::{
    GlobalHistory, GlobalUndoEntry, HistoryDirection, PageHistory, owning_database,
};
use super::{Action, Event, MediaSource, PageTransferUpdate};

pub(super) fn perform_action(
    repository: &mut Repository,
    page_histories: &mut HashMap<PageId, PageHistory>,
    global_history: &mut GlobalHistory,
    action: Action,
) -> Result<Event> {
    let preserve_view = matches!(
        &action,
        Action::RestorePages(_)
            | Action::Undo
            | Action::Redo
            | Action::UndoPage(_)
            | Action::RedoPage(_)
            | Action::RestorePageHistory { .. }
    );
    let mut focus = None;
    let mut focus_at_end = false;
    let mut notice = None;
    let mut refresh_database = None;
    let mut refresh_page_properties = None;
    let mut created_record = None;
    let focus_created_record_name = matches!(&action, Action::CreateDatabaseRecordInline(_));
    let mut page_transfer = None;

    let action_info: Option<(PageId, &'static str)> = match &action {
        Action::InsertBlock { page, .. }
        | Action::InsertBlocks { page, .. }
        | Action::PasteBlocks { page, .. } => Some((*page, "Blok eklendi")),
        Action::CreateSubpageBlock { page, .. } => Some((*page, "Alt sayfa eklendi")),
        Action::ImportMedia { page, .. } => Some((*page, "Medya eklendi")),
        Action::DeleteBlock { page, .. } | Action::DeleteBlocks { page, .. } => {
            Some((*page, "Blok silindi"))
        }
        Action::SetBlocksKind { page, .. } => Some((*page, "Blok türü değiştirildi")),
        Action::SetBlocksChecked { page, .. } => Some((*page, "Görev durumu değiştirildi")),
        Action::FormatBlocks { page, .. } => Some((*page, "Biçimlendirme uygulandı")),
        Action::MoveBlocks { page, .. }
        | Action::MoveBlock { page, .. }
        | Action::ArrangeBlock { page, .. }
        | Action::ArrangeBlocks { page, .. } => Some((*page, "Blok taşındı")),
        Action::DuplicateBlocks { page, .. } => Some((*page, "Bloklar çoğaltıldı")),
        Action::GroupBlocksInColumns { page, .. }
        | Action::UngroupBlockColumns { page, .. }
        | Action::ResizeColumns { page, .. } => Some((*page, "Sütunlar düzenlendi")),
        Action::CreateDatabaseBlock { page, .. } => Some((*page, "Veritabanı eklendi")),
        Action::AddPageProperty { page, .. }
        | Action::RenamePageProperty { page, .. }
        | Action::SetPagePropertyType { page, .. }
        | Action::SetPagePropertyOptions { page, .. }
        | Action::DeletePageProperty { page, .. }
        | Action::SetPagePropertyValue { page, .. } => Some((*page, "Özellik düzenlendi")),
        Action::RenameDatabaseRecord { page, .. } => Some((*page, "Kayıt adı değiştirildi")),
        _ => None,
    };
    let previous = action_info
        .map(|(page, description)| (page, description, repository.page_snapshot(page).ok()));

    let page = match action {
        Action::CreatePage(parent) => Some(repository.create_page(parent, "")?),
        Action::CreateSubpageBlock {
            page: parent,
            position,
            replace_block,
        } => {
            let subpage_id = if let Some(block) = replace_block {
                repository.create_subpage_block_replacing(parent, block)?
            } else {
                repository.create_subpage_block(parent, position)?
            };
            Some(subpage_id)
        }
        Action::CreateDatabase(parent) => Some(repository.create_database(parent)?),
        Action::CreateDatabaseBlock {
            page: parent,
            position,
            replace_block,
        } => {
            let database = if let Some(block) = replace_block {
                repository.create_database_block_replacing(parent, block)?
            } else {
                repository.create_database_block(parent, position)?
            };
            refresh_database = Some(database);
            focus = Some(position);
            Some(parent)
        }
        Action::CreateDatabaseRecord(database) | Action::CreateDatabaseRecordInline(database) => {
            let record = repository.create_database_record(database)?;
            refresh_database = Some(database);
            created_record = Some(record);
            global_history.push_undo(GlobalUndoEntry::CreateRecord(record));
            None
        }
        Action::CreateDatabaseRecordWithProperty {
            database,
            property,
            value,
        } => {
            let record = repository
                .create_database_record_with_property(database, Some((property, &value)))?;
            refresh_database = Some(database);
            created_record = Some(record);
            global_history.push_undo(GlobalUndoEntry::CreateRecord(record));
            None
        }
        Action::ReorderDatabaseRecords {
            database,
            records,
            target,
            after,
        } => {
            repository.reorder_database_records(database, &records, target, after)?;
            refresh_database = Some(database);
            None
        }
        Action::MoveDatabaseRecord {
            record,
            property,
            value,
            target,
        } => {
            refresh_database = repository.move_database_record(record, property, &value, target)?;
            None
        }
        Action::AddDatabaseProperty {
            database,
            name,
            kind,
            options,
        } => {
            repository.add_database_property(database, &name, &kind, &options)?;
            refresh_database = Some(database);
            None
        }
        Action::RenameDatabaseProperty { property, name } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.rename_database_property(property, &name)?;
            None
        }
        Action::SetDatabasePropertyType { property, kind } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.set_database_property_type(property, &kind)?;
            None
        }
        Action::SetDatabasePropertyOptions { property, options } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.set_database_property_options(property, &options)?;
            None
        }
        Action::RenameDatabasePropertyOption {
            property,
            old_option,
            new_option,
        } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.rename_database_property_option(property, &old_option, &new_option)?;
            None
        }
        Action::RemoveDatabasePropertyOption { property, option } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.remove_database_property_option(property, &option)?;
            None
        }
        Action::MoveDatabaseProperty {
            property,
            direction,
        } => {
            refresh_database = repository.database_property_owner(property)?;
            repository.move_database_property(property, direction)?;
            None
        }
        Action::DeleteDatabaseProperty(property) => {
            refresh_database = repository.database_property_owner(property)?;
            repository.delete_database_property(property)?;
            None
        }
        Action::SetDatabaseProperty {
            page,
            property,
            value,
        } => {
            let database = repository.page(page)?.parent_id.ok_or_else(|| {
                Error::Invalid("The record does not belong to a database.".into())
            })?;
            repository.set_database_property(page, property, &value)?;
            // Cell edits must not reload the entire workspace or invalidate the
            // active table while the user is still typing.
            return Ok(Event::Database(repository.database(database)?));
        }
        Action::AddPageProperty {
            page: property_page,
            name,
            kind,
            options,
        } => {
            repository.add_page_property(property_page, &name, &kind, &options)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::RenamePageProperty {
            page: property_page,
            property,
            name,
        } => {
            repository.rename_page_property(property_page, property, &name)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePropertyType {
            page: property_page,
            property,
            kind,
        } => {
            repository.set_page_property_type(property_page, property, &kind)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePropertyOptions {
            page: property_page,
            property,
            options,
        } => {
            repository.set_page_property_options(property_page, property, &options)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::RenamePagePropertyOption {
            page: property_page,
            property,
            old_option,
            new_option,
        } => {
            repository.rename_page_property_option(
                property_page,
                property,
                &old_option,
                &new_option,
            )?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::RemovePagePropertyOption {
            page: property_page,
            property,
            option,
        } => {
            repository.remove_page_property_option(property_page, property, &option)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::MovePageProperty {
            page: property_page,
            property,
            direction,
        } => {
            repository.move_page_property(property_page, property, direction)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::DeletePageProperty {
            page: property_page,
            property,
        } => {
            repository.delete_page_property(property_page, property)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePropertyValue {
            page: property_page,
            property,
            value,
        } => {
            repository.set_page_property_value(property_page, property, &value)?;
            refresh_page_properties = Some(property_page);
            None
        }
        Action::SetPagePriority { page, priority } => {
            refresh_database = repository.page(page)?.parent_id;
            repository.set_page_priority(page, priority)?;
            None
        }
        Action::RenameDatabaseRecord { page, title } => {
            refresh_database = repository.page(page)?.parent_id;
            repository.rename_database_record(page, &title)?;
            None
        }
        Action::SetDatabaseRecordIcon { page, icon } => {
            let page_info = repository.page(page)?;
            refresh_database = if page_info.is_database {
                None
            } else {
                page_info.parent_id
            };
            repository.set_page_icon(page, &icon)?;
            None
        }
        Action::SetDatabaseView { page, view } => {
            refresh_database = Some(page);
            repository.set_database_view(page, view)?;
            None
        }
        Action::SetDatabaseGalleryColumns { page, columns } => {
            refresh_database = Some(page);
            repository.set_database_gallery_columns(page, columns)?;
            None
        }
        Action::SetDatabaseViewSettings { page, settings } => {
            refresh_database = Some(page);
            repository.set_database_view_settings(page, &settings)?;
            None
        }
        Action::SetDatabaseSort {
            page,
            key,
            descending,
        } => {
            refresh_database = Some(page);
            repository.set_database_sort(page, &key, descending)?;
            None
        }
        Action::Favorite(id) => {
            repository.toggle_favorite(id)?;
            None
        }
        Action::Trash(id) => {
            let parent = repository.page(id).ok().and_then(|p| p.parent_id);
            repository.trash_pages(&[id])?;
            if let Some(parent_id) = parent
                && repository.page(parent_id).is_ok_and(|p| p.is_database)
            {
                refresh_database = Some(parent_id);
            }
            global_history.push_undo(GlobalUndoEntry::Trash(vec![id]));
            None
        }
        Action::TrashPages(ids) => {
            let parent = ids
                .first()
                .copied()
                .and_then(|id| repository.page(id).ok())
                .and_then(|p| p.parent_id);
            repository.trash_pages(&ids)?;
            if let Some(parent_id) = parent
                && repository.page(parent_id).is_ok_and(|p| p.is_database)
            {
                refresh_database = Some(parent_id);
            }
            if !ids.is_empty() {
                global_history.push_undo(GlobalUndoEntry::Trash(ids.clone()));
            }
            None
        }
        Action::DeletePermanently(id) => {
            repository.delete_pages_permanently(&[id])?;
            global_history.prune_page(id);
            None
        }
        Action::DeletePagesPermanently(ids) => {
            repository.delete_pages_permanently(&ids)?;
            for &id in &ids {
                global_history.prune_page(id);
            }
            None
        }
        Action::EmptyTrash => {
            repository.empty_trash()?;
            global_history.clear();
            None
        }
        Action::Restore(id) => {
            repository.restore_pages(&[id])?;
            let parent = repository.page(id).ok().and_then(|p| p.parent_id);
            if let Some(parent_id) = parent
                && repository.page(parent_id).is_ok_and(|p| p.is_database)
            {
                refresh_database = Some(parent_id);
            }
            global_history.push_undo(GlobalUndoEntry::Restore(vec![id]));
            Some(id)
        }
        Action::RestorePages(ids) => {
            repository.restore_pages(&ids)?;
            let parent = ids
                .first()
                .copied()
                .and_then(|id| repository.page(id).ok())
                .and_then(|p| p.parent_id);
            if let Some(parent_id) = parent
                && repository.page(parent_id).is_ok_and(|p| p.is_database)
            {
                refresh_database = Some(parent_id);
            }
            if !ids.is_empty() {
                global_history.push_undo(GlobalUndoEntry::Restore(ids.clone()));
            }
            ids.last().copied()
        }
        Action::Undo | Action::Redo => {
            let (direction, empty_event) = match action {
                Action::Undo => (HistoryDirection::Undo, Event::UndoEmpty),
                _ => (HistoryDirection::Redo, Event::RedoEmpty),
            };
            let Some(change) = global_history.apply(repository, page_histories, direction)? else {
                return Ok(empty_event);
            };
            refresh_database = change.refresh_database;
            change.page
        }
        Action::UndoPage(page_id) | Action::RedoPage(page_id) => {
            let (direction, empty_event) = match action {
                Action::UndoPage(_) => (HistoryDirection::Undo, Event::PageUndoEmpty { page_id }),
                _ => (HistoryDirection::Redo, Event::PageRedoEmpty { page_id }),
            };
            let history = page_histories.entry(page_id).or_default();
            if !history.apply(repository, page_id, direction)? {
                return Ok(empty_event);
            }
            Some(page_id)
        }
        Action::RestorePageHistory {
            page: page_id,
            entry_id,
        } => {
            let history = page_histories.entry(page_id).or_default();
            if history.restore(repository, page_id, entry_id)? {
                notice = Some("Geçmiş sürüm geri yüklendi.".into());
                Some(page_id)
            } else {
                None
            }
        }
        Action::Duplicate(id) => Some(repository.duplicate_page(id)?),
        Action::TransferPages {
            pages,
            parent,
            position,
            replace_block,
            copy,
        } => {
            let mut affected: HashSet<_> = pages
                .iter()
                .map(|id| repository.page(*id).map(|page| page.parent_id))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            affected.extend(parent);
            if !copy {
                affected.extend(pages.iter().copied());
            }
            repository.transfer_pages(&pages, parent, position, replace_block, copy)?;
            let mut update = PageTransferUpdate {
                moved_pages: if copy { Vec::new() } else { pages },
                databases: Vec::new(),
                documents: Vec::new(),
            };
            for id in affected {
                if repository.page(id)?.is_database {
                    update.databases.push(id);
                } else {
                    update.documents.push(repository.document(id)?);
                }
            }
            page_transfer = Some(Box::new(update));
            None
        }
        Action::MovePage(id, parent) => {
            repository.move_page(id, parent)?;
            None
        }
        Action::ReorderPage {
            source,
            target,
            after,
        } => {
            repository.reorder_page(source, target, after)?;
            None
        }
        Action::InsertBlock {
            column_anchor,
            page,
            position,
            kind,
            content,
            text_styles,
        } => {
            if content.is_empty() {
                repository.insert_block(page, position, kind)?;
            } else {
                repository.append_blocks_with_styles(
                    page,
                    position,
                    &[(kind, content, false, text_styles)],
                )?;
            }
            focus = Some(finish_column_insertion(
                repository,
                page,
                position,
                1,
                column_anchor,
            )?);
            Some(page)
        }
        Action::InsertBlocks {
            column_anchor,
            page,
            position,
            blocks,
        } => {
            repository.append_blocks_with_styles(page, position, &blocks)?;
            focus = Some(finish_column_insertion(
                repository,
                page,
                position,
                blocks.len(),
                column_anchor,
            )?);
            Some(page)
        }
        Action::PasteBlocks {
            page,
            position,
            replace_block,
            blocks,
        } => {
            repository.paste_blocks(page, position, replace_block, &blocks)?;
            Some(page)
        }
        Action::ImportMedia {
            page,
            position,
            column_anchor,
            source,
        } => {
            let data_directory = crate::config::data_directory()?;
            let kind = match source {
                MediaSource::File(path) => repository.import_media_file(
                    page,
                    position,
                    column_anchor,
                    &path,
                    &data_directory,
                )?,
                MediaSource::Image(image) => repository.import_clipboard_image(
                    page,
                    position,
                    column_anchor,
                    &image,
                    &data_directory,
                )?,
            };
            notice = Some(format!("{} eklendi.", kind.label()));
            focus = Some(position);
            Some(page)
        }
        Action::ChooseCoverImage(page_id) => {
            if let Some(source) = rfd::FileDialog::new()
                .add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"],
                )
                .pick_file()
            {
                let data_directory = crate::config::data_directory()?;
                let cover = repository.import_cover_image(page_id, &source, &data_directory)?;
                let mut edit = crate::domain::model::PageEdit::from(&repository.page(page_id)?);
                edit.cover_image = Some(cover);
                edit.cover = 0;
                repository.save(&crate::domain::model::SaveBatch {
                    pages: vec![edit],
                    blocks: Vec::new(),
                })?;
                Some(page_id)
            } else {
                None
            }
        }
        Action::DeleteBlock { page, block } => {
            let pos = repository.block_position(block).ok().flatten();
            repository.delete_block(block)?;
            focus = pos.map(|p| p.saturating_sub(1));
            focus_at_end = true;
            Some(page)
        }
        Action::DeleteBlocks { page, blocks } => {
            let first_pos = blocks
                .first()
                .and_then(|id| repository.block_position(*id).ok().flatten());
            repository.delete_blocks(page, &blocks)?;
            focus = first_pos.map(|p| p.saturating_sub(1));
            focus_at_end = true;
            Some(page)
        }
        Action::SetBlocksKind { page, blocks, kind } => {
            repository.set_blocks_kind(page, &blocks, kind)?;
            Some(page)
        }
        Action::SetBlocksChecked {
            page,
            blocks,
            checked,
        } => {
            repository.set_blocks_checked(page, &blocks, checked)?;
            Some(page)
        }
        Action::FormatBlocks { page, blocks, op } => {
            repository.format_blocks(page, &blocks, op)?;
            Some(page)
        }
        Action::MoveBlocks {
            page,
            blocks,
            direction,
        } => {
            repository.move_blocks(page, &blocks, direction)?;
            Some(page)
        }
        Action::DuplicateBlocks { page, blocks } => {
            repository.duplicate_blocks(page, &blocks)?;
            Some(page)
        }
        Action::GroupBlocksInColumns { page, blocks } => {
            repository.group_blocks_in_columns(page, &blocks)?;
            Some(page)
        }
        Action::UngroupBlockColumns { page, blocks } => {
            repository.ungroup_block_columns(page, &blocks)?;
            Some(page)
        }
        Action::MoveBlock {
            page,
            block,
            direction,
        } => {
            repository.move_block(block, direction)?;
            Some(page)
        }
        Action::ArrangeBlock {
            page,
            source,
            target,
            placement,
        } => {
            repository.arrange_block(page, source, target, placement)?;
            Some(page)
        }
        Action::ArrangeBlocks {
            page,
            sources,
            target,
            placement,
        } => {
            repository.arrange_blocks(page, &sources, target, placement)?;
            Some(page)
        }
        Action::ResizeColumns {
            page,
            group,
            boundary,
            left_width,
            right_width,
        } => {
            repository.resize_columns(page, group, boundary, left_width, right_width)?;
            Some(page)
        }
        Action::Import => {
            let path = rfd::FileDialog::new()
                .add_filter("Markdown", &["md", "markdown", "txt"])
                .pick_file();
            if let Some(path) = path {
                Some(repository.import_markdown(&path)?)
            } else {
                None
            }
        }
        Action::Reload => None,
        Action::RestoreSnapshot(path) => {
            repository.restore_from(&path)?;
            page_histories.clear();
            global_history.clear();
            notice = Some("AI changes undone.".into());
            None
        }
        Action::Export(id) => {
            let title = repository
                .page(id)?
                .display_title()
                .chars()
                .map(|c| {
                    if c.is_control() || "<>:\"/\\|?*".contains(c) {
                        '_'
                    } else {
                        c
                    }
                })
                .take(120)
                .collect::<String>();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Markdown", &["md"])
                .set_file_name(format!("{title}.md"))
                .save_file()
            {
                repository.export_markdown(id, &path)?;
                notice = Some(format!("Exported to: {}", path.display()));
            }
            None
        }
        Action::Backup => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("SQLite backup", &["sqlite"])
                .set_file_name("noteon-backup.sqlite")
                .save_file()
            {
                repository.backup(&path)?;
                notice = Some(format!("Workspace backed up to: {}", path.display()));
            }
            None
        }
    };
    if let Some((page, description, snapshot)) = previous {
        let history = page_histories.entry(page).or_default();
        history.last_save_time = None;
        if let Some(snapshot) = snapshot
            && history.push_undo(snapshot, description)
        {
            global_history.push_undo(GlobalUndoEntry::Page(page));
        }
    }

    let created_record_document = created_record
        .map(|id| {
            let _ = repository.ensure_subpage_blocks(id);
            repository.document(id)
        })
        .transpose()?;
    if preserve_view && let Some(id) = page {
        refresh_database = owning_database(repository, id);
    }
    Ok(Event::ActionDone {
        pages: repository.list_pages()?,
        preserve_view,
        document: page
            .map(|id| {
                let _ = repository.ensure_subpage_blocks(id);
                repository.document(id)
            })
            .transpose()?,
        page_properties: refresh_page_properties
            .map(|id| {
                repository
                    .page_properties(id)
                    .map(|properties| (id, properties))
            })
            .transpose()?,
        focus,
        focus_at_end,
        notice,
        refresh_database,
        page_transfer,
        created_record_document,
        focus_created_record_name,
        history: page
            .and_then(|id| page_histories.get(&id))
            .map(|h| h.summaries()),
    })
}

/// Keep Enter and multiline paste in the originating column, including its last block.
pub(super) fn finish_column_insertion(
    repository: &mut Repository,
    page: PageId,
    position: usize,
    count: usize,
    anchor: Option<BlockId>,
) -> Result<usize> {
    let Some(anchor) = anchor else {
        return Ok(position + count.saturating_sub(1));
    };
    let inserted = repository.blocks(page, position, count)?;
    let ids: Vec<_> = inserted.iter().map(|block| block.id).collect();
    repository.arrange_blocks(page, &ids, anchor, DropPlacement::AfterInColumn)?;
    ids.last().map_or(Ok(position), |id| {
        Ok(repository.block_position(*id)?.unwrap_or(position))
    })
}
