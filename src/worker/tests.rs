use std::collections::HashMap;

use eframe::egui;

use crate::domain::model::{BlockKind, DropPlacement, PageSnapshot, SaveBatch};
use crate::storage::Repository;

use super::actions::{finish_column_insertion, perform_action};
use super::history::{GlobalHistory, MAX_PAGE_HISTORY, PageHistory};
use super::{Action, Event, FailedOperation, Request, Worker};

#[test]
fn a_later_successful_save_cannot_hide_an_unresolved_failed_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sqlite");
    let mut repository = Repository::open(&path).unwrap();
    let first = repository.create_page(None, "First").unwrap();
    let second = repository.create_page(None, "Second").unwrap();
    let mut first_block = repository.blocks(first, 0, 1).unwrap().remove(0);
    let mut second_block = repository.blocks(second, 0, 1).unwrap().remove(0);
    repository
        .connection
        .execute_batch(
            "CREATE TRIGGER reject_save BEFORE UPDATE OF content ON blocks
         WHEN NEW.content = 'temporary failure'
         BEGIN SELECT RAISE(ABORT, 'test save failed'); END;",
        )
        .unwrap();

    let worker = Worker::start(path, egui::Context::default()).unwrap();
    let receive = || {
        worker
            .events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
    };
    assert!(matches!(receive(), Event::Ready { .. }));
    let save = |id, block| {
        worker
            .send(Request::Save {
                id,
                batch: SaveBatch {
                    pages: Vec::new(),
                    blocks: vec![block],
                },
            })
            .unwrap()
    };

    first_block.content = "temporary failure".into();
    save(1, first_block.clone());
    assert!(matches!(
        receive(),
        Event::Failed {
            operation: FailedOperation::Save(1),
            ..
        }
    ));
    second_block.content = "separate edit".into();
    save(2, second_block);
    assert!(matches!(receive(), Event::Saved { id: 2, .. }));

    worker.send(Request::Action(Action::Undo)).unwrap();
    assert!(matches!(
        receive(),
        Event::Failed {
            operation: FailedOperation::Action,
            ..
        }
    ));
    assert_eq!(
        repository.blocks(second, 0, 1).unwrap()[0].content,
        "separate edit"
    );

    repository
        .connection
        .execute_batch("DROP TRIGGER reject_save")
        .unwrap();
    save(3, first_block);
    assert!(matches!(receive(), Event::Saved { id: 3, .. }));
    worker.send(Request::Action(Action::Undo)).unwrap();
    assert!(
        matches!(receive(), Event::ActionDone { document: Some(doc), .. } if doc.page.id == first)
    );
    assert!(
        repository.blocks(first, 0, 1).unwrap()[0]
            .content
            .is_empty()
    );
    assert_eq!(
        repository.blocks(second, 0, 1).unwrap()[0].content,
        "separate edit"
    );
}

#[test]
fn inline_record_creation_requests_name_focus_and_reordering_refreshes_the_database() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let database = repository.create_database(None).unwrap();
    let first = repository.create_database_record(database).unwrap();
    let mut histories = HashMap::new();
    let mut global_history = GlobalHistory::default();
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::CreateDatabaseRecordInline(database),
    )
    .unwrap();
    let Event::ActionDone {
        document,
        refresh_database,
        created_record_document: Some(record),
        focus_created_record_name,
        ..
    } = event
    else {
        panic!("expected a created record");
    };
    assert!(document.is_none());
    assert_eq!(refresh_database, Some(database));
    assert!(focus_created_record_name);
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::ReorderDatabaseRecords {
            database,
            records: vec![record.page.id],
            target: first,
            after: false,
        },
    )
    .unwrap();
    assert!(
        matches!(event, Event::ActionDone { refresh_database: Some(id), document: None, .. } if id == database)
    );
    assert_eq!(
        repository.database(database).unwrap().rows[0].page.id,
        record.page.id
    );
}

#[test]
fn undoing_record_deletion_refreshes_the_table_without_opening_the_record() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let database = repository.create_database(None).unwrap();
    let first = repository.create_database_record(database).unwrap();
    let second = repository.create_database_record(database).unwrap();
    let mut histories = HashMap::new();
    let mut global_history = GlobalHistory::default();

    for records in [vec![first], vec![first, second]] {
        for action in [
            Action::TrashPages(records.clone()),
            Action::RestorePages(records.clone()),
            Action::TrashPages(records.clone()),
            Action::RestorePages(records.clone()),
        ] {
            let restoring = matches!(&action, Action::RestorePages(_));
            let event =
                perform_action(&mut repository, &mut histories, &mut global_history, action)
                    .unwrap();
            let Event::ActionDone {
                preserve_view,
                refresh_database,
                focus,
                ..
            } = event
            else {
                panic!("deletion history must refresh the current table");
            };
            assert_eq!(preserve_view, restoring);
            assert_eq!(refresh_database, Some(database));
            assert_eq!(focus, None);
            for record in &records {
                assert_eq!(
                    repository.page(*record).unwrap().trashed_by.is_none(),
                    restoring
                );
            }
            assert_eq!(
                repository.database(database).unwrap().rows.len(),
                if restoring { 2 } else { 2 - records.len() }
            );
        }
    }
}

#[test]
fn history_restores_refresh_the_table_without_requesting_navigation() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let database = repository.create_database(None).unwrap();
    let record = repository.create_database_record(database).unwrap();

    for page in [database, record] {
        let original = repository.page_snapshot(page).unwrap();
        let original_title = original.title.clone();
        let mut history = PageHistory::default();
        history.push_undo(original, "Title changed");
        let mut histories = HashMap::from([(page, history)]);
        let mut global_history = GlobalHistory::default();
        repository
            .rename_database_record(page, "Changed title")
            .unwrap();

        for operation in ["undo", "redo", "restore"] {
            let (action, expected_title) = match operation {
                "undo" => (Action::UndoPage(page), original_title.as_str()),
                "redo" => (Action::RedoPage(page), "Changed title"),
                _ => (
                    Action::RestorePageHistory {
                        page,
                        entry_id: histories[&page].summaries()[0].id,
                    },
                    original_title.as_str(),
                ),
            };
            let event =
                perform_action(&mut repository, &mut histories, &mut global_history, action)
                    .unwrap();
            let Event::ActionDone {
                document: Some(document),
                preserve_view,
                refresh_database,
                focus,
                ..
            } = event
            else {
                panic!("history restores must update the document in place");
            };
            assert!(preserve_view);
            assert_eq!(refresh_database, Some(database));
            assert_eq!(focus, None);
            assert_eq!(document.page.id, page);
            assert_eq!(document.page.title, expected_title);
            let data = repository.database(database).unwrap();
            let table_title = if page == database {
                &data.title
            } else {
                &data.rows[0].page.title
            };
            assert_eq!(table_title, expected_title);
        }
    }
}

#[test]
fn cell_edits_persist_and_refresh_only_the_edited_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sqlite");
    let mut repository = Repository::open(&path).unwrap();
    let database = repository.create_database(None).unwrap();
    let page = repository.create_database_record(database).unwrap();
    repository
        .add_database_property(database, "Value", "number", &[])
        .unwrap();
    let property = repository
        .database(database)
        .unwrap()
        .properties
        .iter()
        .find(|property| property.kind == "number")
        .unwrap()
        .id;
    let mut histories = HashMap::new();
    let mut global_history = GlobalHistory::default();
    for value in ["1", "12", "-4", ""] {
        let event = perform_action(
            &mut repository,
            &mut histories,
            &mut global_history,
            Action::SetDatabaseProperty {
                page,
                property,
                value: value.into(),
            },
        )
        .unwrap();
        let Event::Database(data) = event else {
            panic!("cell edits must refresh the database without resetting the workspace");
        };
        assert_eq!(data.page_id, database);
        assert_eq!(data.rows[0].values[&property], value);
    }
    drop(repository);
    let repository = Repository::open(&path).unwrap();
    assert_eq!(
        repository.database(database).unwrap().rows[0].values[&property],
        ""
    );
}

#[test]
fn inserted_blocks_continue_the_column_at_its_end() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let page = repository.create_page(None, "Columns").unwrap();
    let left = repository.blocks(page, 0, 1).unwrap()[0].id;
    let right = repository.insert_block(page, 1, BlockKind::Text).unwrap();
    repository
        .arrange_block(page, right, left, DropPlacement::Right)
        .unwrap();
    for anchor in [left, right] {
        let position = repository.block_position(anchor).unwrap().unwrap() + 1;
        repository
            .append_blocks_with_styles(
                page,
                position,
                &[
                    (
                        BlockKind::Text,
                        "First pasted line".into(),
                        false,
                        Vec::new(),
                    ),
                    (
                        BlockKind::Text,
                        "Second pasted line".into(),
                        false,
                        Vec::new(),
                    ),
                ],
            )
            .unwrap();
        let focused =
            finish_column_insertion(&mut repository, page, position, 2, Some(anchor)).unwrap();
        let blocks = repository.blocks(page, 0, 10).unwrap();
        let anchor_block = blocks.iter().find(|block| block.id == anchor).unwrap();
        let inserted = &blocks[position..position + 2];
        assert_eq!(focused, position + 1);
        assert!(
            inserted
                .iter()
                .all(|block| block.column_group == anchor_block.column_group
                    && block.column_index == anchor_block.column_index)
        );
        assert_eq!(inserted[1].content, "Second pasted line");
    }
}

#[test]
fn clipboard_image_is_saved_in_the_target_column() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let page = repository.create_page(None, "Columns").unwrap();
    let left = repository.blocks(page, 0, 1).unwrap()[0].id;
    let right = repository.insert_block(page, 1, BlockKind::Text).unwrap();
    repository
        .arrange_block(page, right, left, DropPlacement::Right)
        .unwrap();
    let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([42, 65, 120, 128]));

    repository
        .import_clipboard_image(page, 1, Some(right), &image, directory.path())
        .unwrap();

    let blocks = repository.blocks(page, 0, 3).unwrap();
    assert_eq!(blocks[0].id, left);
    assert_eq!(blocks[2].id, right);
    let imported = &blocks[1];
    assert_eq!(imported.kind, BlockKind::Image);
    assert_eq!(imported.column_group, blocks[2].column_group);
    assert_eq!(imported.column_index, blocks[2].column_index);
    assert_eq!(image::open(&imported.content).unwrap().into_rgba8(), image);
}

#[test]
fn page_history_caps_at_maximum_entries_and_clears_redo_on_push() {
    let mut history = PageHistory::default();
    for i in 0..15 {
        let snapshot = PageSnapshot {
            page_id: 1,
            title: format!("Title {i}"),
            icon: "📄".into(),
            blocks: Vec::new(),
        };
        history.push_undo(snapshot, format!("Change {i}"));
    }

    let summaries = history.summaries();
    assert_eq!(summaries.len(), MAX_PAGE_HISTORY);
    assert_eq!(summaries[0].description, "Change 14");
    assert_eq!(summaries[9].description, "Change 5");
}

#[test]
fn page_history_ignores_duplicate_snapshots() {
    let mut history = PageHistory::default();
    let snapshot = PageSnapshot {
        page_id: 1,
        title: "Same Title".into(),
        icon: "📄".into(),
        blocks: Vec::new(),
    };
    history.push_undo(snapshot.clone(), "Change 1");
    history.push_undo(snapshot, "Change 2");
    let summaries = history.summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].description, "Change 1");
}

#[test]
fn global_undo_redo_handles_block_move_and_table_deletion() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let page = repository.create_page(None, "My Page").unwrap();
    let block1 = repository.insert_block(page, 0, BlockKind::Text).unwrap();
    let block2 = repository.insert_block(page, 1, BlockKind::Text).unwrap();

    let database = repository.create_database(Some(page)).unwrap();
    let record = repository.create_database_record(database).unwrap();

    let mut histories = HashMap::new();
    let mut global_history = GlobalHistory::default();

    // Step 1: Move block1 after block2
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::ArrangeBlock {
            page,
            source: block1,
            target: block2,
            placement: DropPlacement::After,
        },
    )
    .unwrap();
    assert!(matches!(event, Event::ActionDone { .. }));
    assert_eq!(repository.block_position(block1).unwrap(), Some(1));

    // Step 2: Delete/trash record from database table
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::Trash(record),
    )
    .unwrap();
    assert!(matches!(event, Event::ActionDone { .. }));
    assert_eq!(repository.database(database).unwrap().rows.len(), 0);

    // Step 3: 1st Undo -> restores the table item!
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::Undo,
    )
    .unwrap();
    let Event::ActionDone {
        refresh_database, ..
    } = event
    else {
        panic!("expected ActionDone on undo");
    };
    assert_eq!(refresh_database, Some(database));
    assert_eq!(repository.database(database).unwrap().rows.len(), 1);
    assert_eq!(
        repository.database(database).unwrap().rows[0].page.id,
        record
    );
    // Block is still at position 1 (not yet undone)
    assert_eq!(repository.block_position(block1).unwrap(), Some(1));

    // Step 4: 2nd Undo -> undoes the block move!
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::Undo,
    )
    .unwrap();
    let Event::ActionDone {
        document: Some(doc),
        ..
    } = event
    else {
        panic!("expected ActionDone with document on second undo");
    };
    assert_eq!(doc.page.id, page);
    // Block1 is back at position 0!
    assert_eq!(repository.block_position(block1).unwrap(), Some(0));
    // Table item is still present!
    assert_eq!(repository.database(database).unwrap().rows.len(), 1);

    // Step 5: 1st Redo -> re-applies the block move!
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::Redo,
    )
    .unwrap();
    assert!(matches!(event, Event::ActionDone { .. }));
    assert_eq!(repository.block_position(block1).unwrap(), Some(1));
    assert_eq!(repository.database(database).unwrap().rows.len(), 1);

    // Step 6: 2nd Redo -> re-trashes the table record!
    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::Redo,
    )
    .unwrap();
    let Event::ActionDone {
        refresh_database, ..
    } = event
    else {
        panic!("expected ActionDone on second redo");
    };
    assert_eq!(refresh_database, Some(database));
    assert_eq!(repository.database(database).unwrap().rows.len(), 0);
}

#[test]
fn a_failed_undo_can_be_retried() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sqlite");
    let mut repository = Repository::open(&path).unwrap();
    let page = repository.create_page(None, "Notes").unwrap();
    let block = repository.insert_block(page, 0, BlockKind::Text).unwrap();
    let mut histories = HashMap::new();
    let mut global_history = GlobalHistory::default();

    perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::SetBlocksKind {
            page,
            blocks: vec![block],
            kind: BlockKind::Heading1,
        },
    )
    .unwrap();

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_history_restore BEFORE DELETE ON blocks
         BEGIN SELECT RAISE(ABORT, 'restore unavailable'); END;",
        )
        .unwrap();

    assert!(
        perform_action(
            &mut repository,
            &mut histories,
            &mut global_history,
            Action::Undo
        )
        .is_err()
    );
    assert_eq!(
        repository.blocks(page, 0, 1).unwrap()[0].kind,
        BlockKind::Heading1
    );
    assert_eq!(histories[&page].summaries().len(), 1);

    connection
        .execute_batch("DROP TRIGGER fail_history_restore;")
        .unwrap();

    assert!(matches!(
        perform_action(
            &mut repository,
            &mut histories,
            &mut global_history,
            Action::Undo
        )
        .unwrap(),
        Event::ActionDone { .. }
    ));
    assert_eq!(
        repository.blocks(page, 0, 1).unwrap()[0].kind,
        BlockKind::Text
    );
}

#[test]
fn redoing_trash_publishes_updated_page_list_for_regular_pages() {
    let directory = tempfile::tempdir().unwrap();
    let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let page = repository.create_page(None, "Notes").unwrap();
    let mut histories = HashMap::new();
    let mut global_history = GlobalHistory::default();

    for action in [Action::Trash(page), Action::Undo] {
        perform_action(&mut repository, &mut histories, &mut global_history, action).unwrap();
    }

    let event = perform_action(
        &mut repository,
        &mut histories,
        &mut global_history,
        Action::Redo,
    )
    .unwrap();
    let Event::ActionDone { pages, .. } = event else {
        panic!("redo did not publish the updated pages");
    };
    assert!(
        pages
            .iter()
            .any(|item| item.id == page && item.trashed_by.is_some())
    );
}
