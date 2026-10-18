use noteon::domain::markdown;
use noteon::domain::model::{BlockKind, DatabaseView, DropPlacement, PageEdit, SaveBatch};
use noteon::domain::virtual_layout::VirtualLayout;
use noteon::storage::Repository;

fn database() -> (tempfile::TempDir, Repository) {
    let directory = tempfile::tempdir().unwrap();
    let repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    (directory, repository)
}

#[test]
fn databases_persist_views_properties_priorities_and_created_dates() {
    let (_directory, mut repository) = database();
    let database = repository.create_database(None).unwrap();
    let record = repository.create_database_record(database).unwrap();
    let property = repository.database(database).unwrap().properties.remove(0);
    repository
        .rename_database_property(property.id, "Aşama")
        .unwrap();
    repository
        .set_database_property_type(property.id, "select")
        .unwrap();
    repository
        .set_database_property_options(property.id, &["Bekliyor".into(), "Bitti".into()])
        .unwrap();
    repository
        .set_database_property(record, property.id, "Bitti")
        .unwrap();
    assert!(
        repository
            .set_database_property(record, property.id, "Bilinmiyor")
            .is_err()
    );
    repository
        .add_database_property(database, "Efor", "number", &[])
        .unwrap();
    let number_property = repository.database(database).unwrap().properties[1].clone();
    let second_record = repository.create_database_record(database).unwrap();
    repository
        .set_database_property(record, number_property.id, "10")
        .unwrap();
    repository
        .set_database_property(second_record, number_property.id, "2")
        .unwrap();
    assert!(
        repository
            .set_database_property(second_record, number_property.id, "many")
            .is_err()
    );
    repository.set_page_priority(record, 3).unwrap();
    repository
        .set_database_view(database, DatabaseView::Board)
        .unwrap();
    repository
        .set_database_gallery_columns(database, 4)
        .unwrap();
    assert!(
        repository
            .set_database_gallery_columns(database, 0)
            .is_err()
    );
    repository
        .set_database_sort(database, &number_property.id.to_string(), false)
        .unwrap();

    let data = repository.database(database).unwrap();
    assert_eq!(data.view, DatabaseView::Board);
    assert_eq!(data.gallery_columns, 4);
    assert_eq!(DatabaseView::from_db("blog"), DatabaseView::Table);
    assert_eq!(data.sort_key, number_property.id.to_string());
    assert_eq!(data.rows.len(), 2);
    assert_eq!(data.properties[0].name, "Aşama");
    assert_eq!(data.properties[0].kind, "select");
    assert_eq!(data.properties[0].options, ["Bekliyor", "Bitti"]);
    assert_eq!(data.rows[0].page.id, second_record);
    let first = data.rows.iter().find(|row| row.page.id == record).unwrap();
    assert!(first.page.created_at > 0);
    assert_eq!(first.page.priority, 3);
    assert_eq!(
        first.values.get(&property.id).map(String::as_str),
        Some("Bitti")
    );
}

#[test]
fn database_blocks_create_persistent_inline_databases_at_the_requested_position() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Projeler").unwrap();
    repository
        .insert_block(page, 1, BlockKind::Heading1)
        .unwrap();

    let database = repository.create_database_block(page, 1).unwrap();
    let blocks = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[1].kind, BlockKind::Database);
    assert_eq!(blocks[1].content, database.to_string());
    assert_eq!(blocks[2].kind, BlockKind::Heading1);
    assert_eq!(BlockKind::from_db("database"), BlockKind::Database);

    let database_page = repository.page(database).unwrap();
    assert_eq!(database_page.parent_id, Some(page));
    assert!(database_page.is_database);
    assert_eq!(
        repository.database(database).unwrap().view,
        DatabaseView::Table
    );

    let slash_page = repository.create_page(None, "Komut testi").unwrap();
    repository
        .insert_block(slash_page, 1, BlockKind::Text)
        .unwrap();
    let placeholder = repository.blocks(slash_page, 1, 1).unwrap().remove(0);
    let database = repository
        .create_database_block_replacing(slash_page, placeholder.id)
        .unwrap();
    let blocks = repository.blocks(slash_page, 0, 2).unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[1].id, placeholder.id);
    assert_eq!(blocks[1].kind, BlockKind::Database);
    assert_eq!(blocks[1].content, database.to_string());
}

#[test]
fn custom_page_properties_persist_values_and_validate_their_types() {
    let (directory, mut repository) = database();
    let page = repository.create_page(None, "Ayakkabı").unwrap();
    repository
        .add_page_property(page, "Ücret", "number", &[])
        .unwrap();
    repository
        .add_page_property(page, "Teslim tarihi", "date", &[])
        .unwrap();
    repository
        .add_page_property(
            page,
            "Renk",
            "multi-select",
            &["Siyah".into(), "Beyaz".into(), "Kırmızı".into()],
        )
        .unwrap();

    let properties = repository.page_properties(page).unwrap();
    let price = properties.iter().find(|item| item.name == "Ücret").unwrap();
    let delivery = properties
        .iter()
        .find(|item| item.name == "Teslim tarihi")
        .unwrap();
    let color = properties.iter().find(|item| item.name == "Renk").unwrap();
    repository
        .set_page_property_value(page, price.id, "2499.90")
        .unwrap();
    repository
        .set_page_property_value(page, delivery.id, "2026-11-20")
        .unwrap();
    repository
        .set_page_property_value(page, color.id, "Siyah|Kırmızı")
        .unwrap();
    assert!(
        repository
            .set_page_property_value(page, price.id, "ücretsiz")
            .is_err()
    );
    assert!(
        repository
            .set_page_property_value(page, delivery.id, "20.11.2026")
            .is_err()
    );
    assert!(
        repository
            .set_page_property_value(page, color.id, "Mavi")
            .is_err()
    );

    let document = repository.document(page).unwrap();
    assert_eq!(document.properties.len(), 3);
    assert_eq!(document.properties[0].value, "2499.90");
    assert_eq!(document.properties[1].value, "2026-11-20");
    assert_eq!(
        document.properties[2].options,
        ["Siyah", "Beyaz", "Kırmızı"]
    );
    assert_eq!(document.properties[2].value, "Siyah|Kırmızı");

    drop(repository);
    let mut reopened = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    assert_eq!(
        reopened.document(page).unwrap().properties[0].value,
        "2499.90"
    );

    // Rename option updates selected value automatically
    reopened
        .rename_page_property_option(page, color.id, "Kırmızı", "Bordo")
        .unwrap();
    let props = reopened.page_properties(page).unwrap();
    let updated_color = props.iter().find(|item| item.id == color.id).unwrap();
    assert_eq!(updated_color.options, ["Siyah", "Beyaz", "Bordo"]);
    assert_eq!(updated_color.value, "Siyah|Bordo");

    // Remove option cleans selected value automatically
    reopened
        .remove_page_property_option(page, color.id, "Siyah")
        .unwrap();
    let props = reopened.page_properties(page).unwrap();
    let updated_color = props.iter().find(|item| item.id == color.id).unwrap();
    assert_eq!(updated_color.options, ["Beyaz", "Bordo"]);
    assert_eq!(updated_color.value, "Bordo");

    // Reorder page properties
    reopened.move_page_property(page, color.id, -1).unwrap();
    let props = reopened.page_properties(page).unwrap();
    assert_eq!(props[1].id, color.id);
    assert_eq!(props[2].id, delivery.id);
}

#[test]
fn version_four_workspaces_migrate_custom_page_properties() {
    let (directory, repository) = database();
    let path = directory.path().join("test.sqlite");
    drop(repository);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TABLE custom_page_property_values;\
             DROP TABLE custom_page_properties;\
             ALTER TABLE databases DROP COLUMN gallery_columns;\
             PRAGMA user_version=4;",
        )
        .unwrap();
    drop(connection);

    let mut migrated = Repository::open(&path).unwrap();
    let page = migrated.create_page(None, "Özellik denemesi").unwrap();
    migrated
        .add_page_property(page, "Tutar", "number", &[])
        .unwrap();
    assert_eq!(migrated.document(page).unwrap().properties.len(), 1);
    let database = migrated.create_database(None).unwrap();
    assert_eq!(migrated.database(database).unwrap().gallery_columns, 3);
    drop(migrated);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 13);
}

#[test]
fn version_two_workspaces_migrate_page_metadata_and_database_tables() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE pages (id INTEGER PRIMARY KEY, parent_id INTEGER, title TEXT NOT NULL, icon TEXT NOT NULL, favorite INTEGER NOT NULL, trashed_by INTEGER, cover INTEGER NOT NULL, updated_at INTEGER NOT NULL, block_count INTEGER NOT NULL);\
             INSERT INTO pages VALUES (1,NULL,'Legacy','✎',0,NULL,0,1700000000,0);\
             PRAGMA user_version=2;",
        )
        .unwrap();
    drop(connection);

    let repository = Repository::open(&path).unwrap();
    let page = repository.page(1).unwrap();
    assert_eq!(page.created_at, 1_700_000_000);
    assert_eq!(page.priority, 0);
    assert!(!page.is_database);
    assert!(repository.database_property_owner(1).unwrap().is_none());
}

#[test]
fn newly_created_database_record_and_page_have_positive_created_at() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Yeni Sayfa").unwrap();
    assert!(repository.page(page).unwrap().created_at > 0);

    let database = repository.create_database(None).unwrap();
    assert!(repository.page(database).unwrap().created_at > 0);

    let record = repository.create_database_record(database).unwrap();
    assert!(repository.page(record).unwrap().created_at > 0);
}

#[test]
fn legacy_pages_with_zero_created_at_are_migrated_to_valid_timestamp() {
    let (directory, mut repository) = database();
    let page_id = repository.create_page(None, "Legacy Test").unwrap();
    drop(repository);
    let path = directory.path().join("test.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE pages SET created_at=0, updated_at=1710000000 WHERE id=?1",
            [page_id],
        )
        .unwrap();
    connection
        .pragma_update(None, "user_version", 12)
        .unwrap();
    drop(connection);

    let repository = Repository::open(&path).unwrap();
    let page = repository.page(page_id).unwrap();
    assert_eq!(page.created_at, 1710000000);
}

#[test]
fn saves_survive_reopening_and_search_tracks_edits() {
    let (directory, mut repository) = database();
    let id = repository
        .create_page(None, "Türkçe çalışma alanı")
        .unwrap();
    let mut block = repository.blocks(id, 0, 1).unwrap().remove(0);
    block.kind = BlockKind::Todo;
    block.content = "Benzersiz fındık ve öğrenme notları".into();
    block.checked = true;
    repository
        .save(&SaveBatch {
            blocks: vec![block.clone()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(repository.search("öğrenme").unwrap()[0].page_id, id);
    assert_eq!(repository.search("Türkçe").unwrap()[0].page_id, id);
    block.content = "Güncellenmiş içerik".into();
    repository
        .save(&SaveBatch {
            blocks: vec![block],
            ..Default::default()
        })
        .unwrap();
    assert!(repository.search("öğrenme").unwrap().is_empty());
    drop(repository);
    let reopened = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let saved = reopened.blocks(id, 0, 1).unwrap().remove(0);
    assert!(saved.checked);
    assert_eq!(saved.kind, BlockKind::Todo);
    assert_eq!(saved.content, "Güncellenmiş içerik");
    assert_eq!(reopened.search("Güncel").unwrap()[0].position, Some(0));
}

#[test]
fn cover_image_path_and_focal_position_persist_and_duplicate() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Cover positioning").unwrap();
    let mut edit = noteon::domain::model::PageEdit::from(&repository.page(page).unwrap());
    edit.cover_image = Some("attachments/cover-test.webp".into());
    edit.cover_x = 0.73;
    edit.cover_y = 0.24;
    repository
        .save(&noteon::domain::model::SaveBatch {
            pages: vec![edit],
            blocks: Vec::new(),
        })
        .unwrap();

    let saved = repository.page(page).unwrap();
    assert_eq!(
        saved.cover_image.as_deref(),
        Some("attachments/cover-test.webp")
    );
    assert!((saved.cover_x - 0.73).abs() < f32::EPSILON);
    assert!((saved.cover_y - 0.24).abs() < f32::EPSILON);

    let duplicate = repository.duplicate_page(page).unwrap();
    let copied = repository.page(duplicate).unwrap();
    assert_eq!(copied.cover_image, saved.cover_image);
    assert_eq!(copied.cover_x, saved.cover_x);
    assert_eq!(copied.cover_y, saved.cover_y);
}

#[test]
fn sidebar_page_reordering_persists_between_loads() {
    let (directory, mut repository) = database();
    let first = repository.create_page(None, "First").unwrap();
    let middle = repository.create_page(None, "Middle").unwrap();
    let last = repository.create_page(None, "Last").unwrap();

    repository.reorder_page(last, first, false).unwrap();
    let ordered = repository
        .list_pages()
        .unwrap()
        .into_iter()
        .filter(|page| [first, middle, last].contains(&page.id))
        .map(|page| page.id)
        .collect::<Vec<_>>();
    assert_eq!(ordered, [last, first, middle]);

    drop(repository);
    let reopened = Repository::open(&directory.path().join("test.sqlite")).unwrap();
    let ordered_after_restart = reopened
        .list_pages()
        .unwrap()
        .into_iter()
        .filter(|page| [first, middle, last].contains(&page.id))
        .map(|page| page.id)
        .collect::<Vec<_>>();
    assert_eq!(ordered_after_restart, [last, first, middle]);
}

#[test]
fn save_batch_is_atomic_on_missing_block() {
    let (_directory, mut repository) = database();
    let id = repository.create_page(None, "Özgün başlık").unwrap();
    let mut page = PageEdit::from(&repository.page(id).unwrap());
    page.title = "Yarım kaydedilmemeli".into();
    let mut block = repository.blocks(id, 0, 1).unwrap().remove(0);
    block.id = i64::MAX;
    let result = repository.save(&SaveBatch {
        pages: vec![page],
        blocks: vec![block],
    });
    assert!(result.is_err());
    assert_eq!(repository.page(id).unwrap().title, "Özgün başlık");
}

#[test]
fn insert_move_and_delete_keep_positions_contiguous_and_counts_correct() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Sıralama").unwrap();
    let first = repository.blocks(page, 0, 1).unwrap()[0].id;
    let inserted = repository
        .insert_block(page, 0, BlockKind::Heading1)
        .unwrap();
    repository.insert_block(page, 999, BlockKind::Todo).unwrap();
    assert_eq!(repository.page(page).unwrap().block_count, 3);
    repository.move_block(inserted, 1).unwrap();
    let blocks = repository.blocks(page, 0, 100).unwrap();
    assert_eq!(blocks[0].id, first);
    assert_eq!(blocks[1].id, inserted);
    repository.delete_block(inserted).unwrap();
    let blocks = repository.blocks(page, 0, 100).unwrap();
    assert_eq!(
        blocks
            .iter()
            .map(|block| block.position)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    for block in blocks {
        repository.delete_block(block.id).unwrap();
    }
    assert_eq!(repository.page(page).unwrap().block_count, 1);
    assert_eq!(repository.blocks(page, 0, 1).unwrap()[0].position, 0);
}

#[test]
fn block_arrangement_persists_columns_and_moves_whole_rows() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Düzen").unwrap();
    repository.insert_block(page, 1, BlockKind::Text).unwrap();
    repository.insert_block(page, 2, BlockKind::Text).unwrap();
    let blocks = repository.blocks(page, 0, 10).unwrap();
    let first = blocks[0].id;
    let second = blocks[1].id;
    let third = blocks[2].id;

    repository
        .arrange_block(page, second, first, DropPlacement::Right)
        .unwrap();
    let paired = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(paired[0].id, first);
    assert_eq!(paired[1].id, second);
    assert_eq!(paired[0].column_group, paired[1].column_group);
    assert_eq!((paired[0].column_index, paired[1].column_index), (0, 1));

    repository
        .arrange_block(page, first, third, DropPlacement::After)
        .unwrap();
    let moved = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(
        moved.iter().map(|block| block.id).collect::<Vec<_>>(),
        [third, first, second]
    );
    assert_eq!(moved[1].column_group, moved[2].column_group);
}

#[test]
fn block_rows_support_more_than_two_resizable_columns() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Çok sütun").unwrap();
    for position in 1..4 {
        repository
            .insert_block(page, position, BlockKind::Text)
            .unwrap();
    }
    let blocks = repository.blocks(page, 0, 10).unwrap();
    let ids: Vec<_> = blocks.iter().map(|block| block.id).collect();

    repository
        .arrange_block(page, ids[1], ids[0], DropPlacement::Right)
        .unwrap();
    repository
        .arrange_block(page, ids[2], ids[1], DropPlacement::Right)
        .unwrap();
    repository
        .arrange_block(page, ids[3], ids[2], DropPlacement::Right)
        .unwrap();

    let arranged = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(
        arranged.iter().map(|block| block.id).collect::<Vec<_>>(),
        ids
    );
    assert!(
        arranged
            .iter()
            .all(|block| block.column_group == Some(ids[0]))
    );
    assert_eq!(
        arranged
            .iter()
            .map(|block| block.column_index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );

    repository
        .resize_columns(page, ids[0], 1, 0.2, 0.3)
        .unwrap();
    let document = repository.document(page).unwrap();
    let widths: Vec<_> = document
        .outline
        .iter()
        .map(|block| block.column_width)
        .collect();
    assert!((widths[0] - 0.25).abs() < 0.001);
    assert!((widths[1] - 0.2).abs() < 0.001);
    assert!((widths[2] - 0.3).abs() < 0.001);
    assert!((widths[3] - 0.25).abs() < 0.001);
}

#[test]
fn trashed_descendants_and_independent_deletions_restore_correctly() {
    let (_directory, mut repository) = database();
    let root = repository.create_page(None, "Kök").unwrap();
    let child = repository.create_page(Some(root), "Alt").unwrap();
    let independent = repository
        .create_page(Some(root), "Ayrıca silinmiş")
        .unwrap();
    repository.trash_page(independent).unwrap();
    repository.trash_page(root).unwrap();
    assert_eq!(repository.page(child).unwrap().trashed_by, Some(root));
    assert!(repository.search("Kök").unwrap().is_empty());
    repository.restore_page(root).unwrap();
    assert!(repository.page(child).unwrap().trashed_by.is_none());
    assert_eq!(
        repository.page(independent).unwrap().trashed_by,
        Some(independent)
    );
    assert!(!repository.search("Kök").unwrap().is_empty());
}

#[test]
fn restoring_a_separately_trashed_child_of_trashed_parent_moves_it_to_root() {
    let (_directory, mut repository) = database();
    let root = repository.create_page(None, "Kök").unwrap();
    let child = repository.create_page(Some(root), "Alt").unwrap();
    repository.trash_page(child).unwrap();
    repository.trash_page(root).unwrap();
    repository.restore_page(child).unwrap();
    assert_eq!(repository.page(child).unwrap().parent_id, None);
    assert_eq!(repository.page(child).unwrap().trashed_by, None);
}

#[test]
fn page_movement_rejects_cycles_and_trashed_parents() {
    let (_directory, mut repository) = database();
    let root = repository.create_page(None, "Kök").unwrap();
    let child = repository.create_page(Some(root), "Alt").unwrap();
    assert!(repository.move_page(root, Some(child)).is_err());
    assert!(repository.move_page(root, Some(root)).is_err());
    repository.move_page(child, None).unwrap();
    assert_eq!(repository.page(child).unwrap().parent_id, None);
    repository.trash_page(child).unwrap();
    assert!(repository.move_page(root, Some(child)).is_err());
}

#[test]
fn duplicate_has_independent_block_ids_and_keeps_content() {
    let (_directory, mut repository) = database();
    let original = repository.create_page(None, "Kaynak").unwrap();
    let mut block = repository.blocks(original, 0, 1).unwrap().remove(0);
    block.content = "Kopyalanacak metin".into();
    repository
        .save(&SaveBatch {
            blocks: vec![block.clone()],
            ..Default::default()
        })
        .unwrap();
    let duplicate = repository.duplicate_page(original).unwrap();
    let copied = repository.blocks(duplicate, 0, 1).unwrap().remove(0);
    assert_ne!(copied.id, block.id);
    assert_eq!(copied.content, block.content);
    assert_eq!(repository.page(duplicate).unwrap().block_count, 1);
}

#[test]
fn arbitrary_search_input_never_becomes_fts_syntax() {
    let (_directory, mut repository) = database();
    repository.create_page(None, "Sözdizimi").unwrap();
    for query in [
        "\"",
        "*",
        "() AND OR NOT",
        "'; DROP TABLE pages;--",
        "😀",
        "başlık: *",
        "a\" OR b",
    ] {
        assert!(repository.search(query).is_ok(), "{query}");
    }
    assert_eq!(repository.list_pages().unwrap().len(), 1);
}

#[test]
fn markdown_import_export_preserves_todos_headings_and_embedded_fences() {
    let (directory, mut repository) = database();
    let source = "# Deneme\r\n\r\n## Başlık\r\n- [x] Tamamlandı\r\n- [ ] Bekliyor\r\n````rust\r\nlet fence = \"```\";\r\n```\r\n````\r\n";
    let input = directory.path().join("girdi.md");
    let output = directory.path().join("çıktı.md");
    std::fs::write(&input, source).unwrap();
    let page = repository.import_markdown(&input).unwrap();
    assert_eq!(repository.page(page).unwrap().title, "Deneme");
    let blocks = repository.blocks(page, 0, 100).unwrap();
    assert_eq!(blocks.len(), 4);
    assert_eq!(blocks[0].kind, BlockKind::Heading2);
    assert!(blocks[1].checked);
    assert!(!blocks[2].checked);
    assert!(blocks[3].content.contains("\n```"));
    repository.export_markdown(page, &output).unwrap();
    let (_, reparsed) = markdown::parse(&std::fs::read_to_string(output).unwrap());
    assert_eq!(reparsed[3].1, blocks[3].content);
}

#[test]
fn backup_includes_committed_wal_data_and_search_index() {
    let (directory, mut repository) = database();
    let page = repository.create_page(None, "Yedeklenecek").unwrap();
    let backup = directory.path().join("backup.sqlite");
    repository.backup(&backup).unwrap();
    let copied = Repository::open(&backup).unwrap();
    assert_eq!(copied.page(page).unwrap().title, "Yedeklenecek");
    assert_eq!(copied.search("Yedek").unwrap()[0].page_id, page);
}

#[test]
fn seed_is_idempotent_and_does_not_modify_existing_workspace() {
    let (_directory, mut repository) = database();
    repository.seed_welcome().unwrap();
    let count = repository.list_pages().unwrap().len();
    repository.seed_welcome().unwrap();
    assert_eq!(repository.list_pages().unwrap().len(), count);
    assert!(repository.last_page().unwrap().is_some());
}

#[test]
fn deep_block_windows_use_index_and_do_not_load_all_text() {
    let (directory, mut repository) = database();
    let page = repository.create_page(None, "Büyük sayfa").unwrap();
    let blocks = (0..10_000)
        .map(|index| (BlockKind::Text, format!("Satır {index}"), false))
        .collect::<Vec<_>>();
    repository.append_blocks(page, 1, &blocks).unwrap();
    let window = repository.blocks(page, 9_000, 128).unwrap();
    assert_eq!(window.len(), 128);
    assert_eq!(window[0].position, 9_000);
    assert_eq!(repository.document(page).unwrap().outline.len(), 10_001);
    let connection = rusqlite::Connection::open(directory.path().join("test.sqlite")).unwrap();
    let plan: String = connection.query_row("EXPLAIN QUERY PLAN SELECT id,content FROM blocks WHERE page_id=?1 AND position>=9000 AND position<9128 ORDER BY position", [page], |row| row.get(3)).unwrap();
    assert!(plan.contains("blocks_page_position"), "{plan}");
    assert!(!plan.contains("SCAN"), "{plan}");
}

#[test]
fn dropped_media_is_copied_into_noteon_data_and_inserted_as_a_block() {
    let (directory, mut repository) = database();
    let page = repository.create_page(None, "Medya test").unwrap();
    let source = directory.path().join("dropped.png");
    std::fs::write(&source, b"fixture").unwrap();
    let data_directory = directory.path().join("noteon-data");

    let kind = repository
        .import_media_block(page, 0, &source, &data_directory)
        .unwrap();

    let block = repository.blocks(page, 0, 1).unwrap().remove(0);
    assert_eq!(kind, BlockKind::Image);
    assert_eq!(block.kind, BlockKind::Image);
    assert_eq!(block.media_width, 1.0);
    assert!(std::path::Path::new(&block.content).is_file());
    assert!(
        block
            .content
            .starts_with(&data_directory.join("attachments").display().to_string())
    );
    assert_eq!(std::fs::read(block.content).unwrap(), b"fixture");
}

#[test]
fn virtual_layout_maps_variable_height_rows_after_updates() {
    let mut layout = VirtualLayout::new([20.0, 40.0, 15.0]);
    assert_eq!(layout.row_at(0.0), 0);
    assert_eq!(layout.row_at(19.0), 0);
    assert_eq!(layout.row_at(20.0), 1);
    assert_eq!(layout.row_at(60.0), 2);
    layout.set(0, 100.0);
    assert_eq!(layout.total(), 155.0);
    assert_eq!(layout.top(2), 140.0);
    assert_eq!(layout.row_at(100.0), 1);
    assert_eq!(layout.row_at(1000.0), 2);
    assert_eq!(VirtualLayout::default().row_at(0.0), 0);
}

#[test]
fn virtual_layout_handles_large_document_without_linear_row_lookup() {
    let mut layout = VirtualLayout::new(std::iter::repeat_n(36.0, 100_000));
    assert_eq!(layout.row_at(3_599_999.0), 99_999);
    layout.set(50_000, 72.0);
    assert_eq!(layout.total(), 3_600_036.0);
    assert_eq!(layout.row_at(layout.top(99_000)), 99_000);
}

#[test]
fn database_options_rename_remove_and_board_reorder_persist() {
    let (_directory, mut repository) = database();
    let db_page = repository.create_database(None).unwrap();
    repository
        .add_database_property(db_page, "Tags", "multi-select", &[])
        .unwrap();
    let data = repository.database(db_page).unwrap();
    let tags_prop = data
        .properties
        .iter()
        .find(|p| p.name == "Tags")
        .unwrap()
        .clone();
    assert!(tags_prop.options.is_empty());

    repository
        .set_database_property_options(
            tags_prop.id,
            &[
                noteon::domain::model::PropertyOption::new("Alpha"),
                noteon::domain::model::PropertyOption::new("Beta"),
                noteon::domain::model::PropertyOption::new("Gamma"),
            ],
        )
        .unwrap();

    let rec1 = repository.create_database_record(db_page).unwrap();
    let rec2 = repository.create_database_record(db_page).unwrap();
    repository
        .set_database_property(rec1, tags_prop.id, "Alpha|Beta")
        .unwrap();
    repository
        .set_database_property(rec2, tags_prop.id, "Beta|Gamma")
        .unwrap();

    repository
        .rename_database_property_option(tags_prop.id, "Beta", "Beta2")
        .unwrap();
    let data = repository.database(db_page).unwrap();
    let r1 = data.rows.iter().find(|r| r.page.id == rec1).unwrap();
    assert_eq!(r1.values.get(&tags_prop.id).unwrap(), "Alpha|Beta2");

    repository
        .remove_database_property_option(tags_prop.id, "Alpha")
        .unwrap();
    let data = repository.database(db_page).unwrap();
    let r1 = data.rows.iter().find(|r| r.page.id == rec1).unwrap();
    assert_eq!(r1.values.get(&tags_prop.id).unwrap(), "Beta2");

    let status_prop = data
        .properties
        .iter()
        .find(|p| p.kind == "status")
        .unwrap()
        .clone();
    repository
        .move_database_record(rec2, status_prop.id, "Done", Some((rec1, false)))
        .unwrap();
    let data = repository.database(db_page).unwrap();
    assert_eq!(data.sort_key, "manual");
    assert_eq!(data.rows[0].page.id, rec2);
    assert_eq!(data.rows[1].page.id, rec1);
    assert_eq!(data.rows[0].values.get(&status_prop.id).unwrap(), "Done");
}

#[test]
fn bulk_block_operations_edit_move_duplicate_group_and_delete() {
    use noteon::domain::model::BulkBlockStyleOp;

    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Toplu Düzenleme").unwrap();
    repository
        .append_blocks(
            page,
            1,
            &[
                (BlockKind::Text, "Birinci".into(), false),
                (BlockKind::Text, "İkinci".into(), false),
                (BlockKind::Text, "Üçüncü".into(), false),
            ],
        )
        .unwrap();
    let initial = repository.blocks(page, 0, 10).unwrap();
    let target_ids = vec![initial[1].id, initial[2].id];

    repository
        .set_blocks_kind(page, &target_ids, BlockKind::Todo)
        .unwrap();
    repository
        .set_blocks_checked(page, &target_ids, true)
        .unwrap();
    repository
        .format_blocks(page, &target_ids, BulkBlockStyleOp::ToggleBold)
        .unwrap();

    let updated = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(updated[1].kind, BlockKind::Todo);
    assert_eq!(updated[2].kind, BlockKind::Todo);
    assert!(updated[1].checked);
    assert!(updated[2].checked);
    assert!(updated[1].text_styles[0].style.bold);
    assert!(updated[2].text_styles[0].style.bold);

    repository
        .group_blocks_in_columns(page, &target_ids)
        .unwrap();
    let grouped = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(grouped[1].column_group, Some(target_ids[0]));
    assert_eq!(grouped[2].column_group, Some(target_ids[0]));

    repository
        .ungroup_block_columns(page, &target_ids)
        .unwrap();
    let ungrouped = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(ungrouped[1].column_group, None);
    assert_eq!(ungrouped[2].column_group, None);

    repository.move_blocks(page, &target_ids, -1).unwrap();
    let moved = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(moved[0].id, target_ids[0]);
    assert_eq!(moved[1].id, target_ids[1]);

    let duplicated = repository.duplicate_blocks(page, &target_ids).unwrap();
    assert_eq!(duplicated.len(), 2);

    repository.delete_blocks(page, &target_ids).unwrap();
    let remaining = repository.blocks(page, 0, 10).unwrap();
    assert!(!remaining.iter().any(|b| target_ids.contains(&b.id)));
}

#[test]
fn full_width_persists_and_duplicates() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Full Width Test").unwrap();
    assert!(!repository.page(page).unwrap().full_width);

    let mut edit = PageEdit::from(&repository.page(page).unwrap());
    edit.full_width = true;
    repository
        .save(&SaveBatch {
            pages: vec![edit],
            blocks: Vec::new(),
        })
        .unwrap();
    assert!(repository.page(page).unwrap().full_width);

    let copy = repository.duplicate_page(page).unwrap();
    assert!(repository.page(copy).unwrap().full_width);
}

#[test]
fn trash_pages_and_restore_pages_work_in_batch() {
    let (_directory, mut repository) = database();
    let p1 = repository.create_page(None, "Page 1").unwrap();
    let p2 = repository.create_page(None, "Page 2").unwrap();
    let p3 = repository.create_page(None, "Page 3").unwrap();

    repository.trash_pages(&[p1, p3]).unwrap();
    assert!(repository.page(p1).unwrap().trashed_by.is_some());
    assert!(repository.page(p2).unwrap().trashed_by.is_none());
    assert!(repository.page(p3).unwrap().trashed_by.is_some());

    repository.restore_pages(&[p1, p3]).unwrap();
    assert!(repository.page(p1).unwrap().trashed_by.is_none());
    assert!(repository.page(p2).unwrap().trashed_by.is_none());
    assert!(repository.page(p3).unwrap().trashed_by.is_none());
}

#[test]
fn page_snapshot_and_restore_page_snapshot_round_trip() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Initial Title").unwrap();
    repository
        .insert_block(page, 1, BlockKind::Todo)
        .unwrap();

    let snapshot = repository.page_snapshot(page).unwrap();
    assert_eq!(snapshot.title, "Initial Title");
    assert_eq!(snapshot.blocks.len(), 2);

    // Make changes: insert block and change title
    repository
        .insert_block(page, 2, BlockKind::Callout)
        .unwrap();
    let mut edit = PageEdit::from(&repository.page(page).unwrap());
    edit.title = "Changed Title".into();
    repository
        .save(&SaveBatch {
            pages: vec![edit],
            blocks: Vec::new(),
        })
        .unwrap();
    assert_eq!(repository.page(page).unwrap().title, "Changed Title");
    assert_eq!(repository.document(page).unwrap().outline.len(), 3);

    // Restore snapshot
    let restored_doc = repository.restore_page_snapshot(&snapshot).unwrap();
    assert_eq!(restored_doc.page.title, "Initial Title");
    assert_eq!(restored_doc.outline.len(), 2);
    assert_eq!(restored_doc.outline[1].kind, BlockKind::Todo);
}

#[test]
fn subpage_blocks_lifecycle_create_sync_ensure_and_trash() {
    let (_directory, mut repository) = database();
    let parent = repository.create_page(None, "Ana Sayfa").unwrap();
    let initial_doc = repository.document(parent).unwrap();
    let first_block_id = initial_doc.outline[0].id;

    // 1. Create subpage block replacing initial block
    let child = repository
        .create_subpage_block_replacing(parent, first_block_id)
        .unwrap();
    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert_eq!(parent_blocks.len(), 1);
    assert_eq!(parent_blocks[0].content, "↗ Untitled");

    // 2. Rename child page and ensure parent block updates
    let mut edit = PageEdit::from(&repository.page(child).unwrap());
    edit.title = "Alt Sayfa Detayları".into();
    repository
        .save(&SaveBatch {
            pages: vec![edit],
            blocks: Vec::new(),
        })
        .unwrap();
    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert_eq!(parent_blocks[0].content, "↗ Alt Sayfa Detayları");

    // 3. Directly create a child page and call ensure_subpage_blocks
    let child2 = repository.create_page(Some(parent), "İkinci Alt").unwrap();
    let _ = repository.ensure_subpage_blocks(parent).unwrap();
    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert_eq!(parent_blocks.len(), 2);
    assert!(parent_blocks.iter().any(|b| b.content == "↗ İkinci Alt"));

    // 4. Delete the first subpage block from parent -> child should be trashed
    let first_block = parent_blocks[0].id;
    repository.delete_block(first_block).unwrap();
    assert!(repository.page(child).unwrap().trashed_by.is_some());

    // 5. Trash the second child -> its block should be removed from parent
    repository.trash_page(child2).unwrap();
    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert!(!parent_blocks.iter().any(|b| b.content == "↗ İkinci Alt"));
}

#[test]
fn subpage_database_gets_link_block_in_parent_and_inline_is_not_duplicated() {
    let (_directory, mut repository) = database();
    let parent = repository.create_page(None, "Ana Sayfa").unwrap();

    // 1. Create a database under parent
    let sub_db = repository.create_database(Some(parent)).unwrap();
    let mut edit = PageEdit::from(&repository.page(sub_db).unwrap());
    edit.title = "Suicide Notes".into();
    repository
        .save(&SaveBatch {
            pages: vec![edit],
            blocks: Vec::new(),
        })
        .unwrap();

    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert!(parent_blocks.iter().any(|b| b.content == "↗ Suicide Notes"));

    // 2. If the block was removed, ensure_subpage_blocks recreates it
    repository.delete_blocks(parent, &[parent_blocks[0].id]).unwrap();
    // Wait, delete_blocks with ↗ might trash the child if it finds it, so let's verify ensure_subpage_blocks recreates it if child is active
    let _ = repository.restore_pages(&[sub_db]);
    repository.ensure_subpage_blocks(parent).unwrap();
    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert!(parent_blocks.iter().any(|b| b.content == "↗ Suicide Notes"));

    // 3. Inline database block is NOT duplicated with a text link block
    let inline_db = repository.create_database_block(parent, 2).unwrap();
    let mut edit_inline = PageEdit::from(&repository.page(inline_db).unwrap());
    edit_inline.title = "Tablo Görünümü".into();
    repository
        .save(&SaveBatch {
            pages: vec![edit_inline],
            blocks: Vec::new(),
        })
        .unwrap();
    repository.ensure_subpage_blocks(parent).unwrap();
    let parent_blocks = repository.blocks(parent, 0, 10).unwrap();
    assert!(parent_blocks.iter().any(|b| b.kind == BlockKind::Database && b.content == inline_db.to_string()));
    assert!(!parent_blocks.iter().any(|b| b.content == "↗ Tablo Görünümü"));
}


#[test]
fn permanent_delete_and_empty_trash_remove_pages_completely() {
    let (_directory, mut repository) = database();

    let page1 = repository.create_page(None, "Sayfa 1").unwrap();
    let child1 = repository.create_page(Some(page1), "Alt Sayfa 1").unwrap();
    let page2 = repository.create_page(None, "Sayfa 2").unwrap();

    repository.trash_pages(&[page1, page2]).unwrap();
    assert!(repository.page(page1).unwrap().trashed_by.is_some());
    assert!(repository.page(child1).unwrap().trashed_by.is_some());
    assert!(repository.page(page2).unwrap().trashed_by.is_some());

    // Permanently delete page2
    repository.delete_pages_permanently(&[page2]).unwrap();
    assert!(repository.page(page2).is_err());
    assert!(repository.page(page1).is_ok());
    assert!(repository.page(child1).is_ok());

    // Empty trash
    repository.empty_trash().unwrap();
    assert!(repository.page(page1).is_err());
    assert!(repository.page(child1).is_err());
}

#[test]
fn bulk_blocks_drag_and_drop_moves_all_selected_blocks_together() {
    let (_directory, mut repository) = database();
    let page = repository.create_page(None, "Toplu Taşıma").unwrap();
    for pos in 1..5 {
        repository.insert_block(page, pos, BlockKind::Text).unwrap();
    }
    let blocks = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(blocks.len(), 5);
    let ids: Vec<_> = blocks.iter().map(|b| b.id).collect();
    let (b0, b1, b2, b3, b4) = (ids[0], ids[1], ids[2], ids[3], ids[4]);

    repository
        .arrange_blocks(page, &[b0, b1], b4, DropPlacement::After)
        .unwrap();
    let moved = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(
        moved.iter().map(|b| b.id).collect::<Vec<_>>(),
        vec![b2, b3, b4, b0, b1]
    );

    repository
        .arrange_blocks(page, &[b0, b1], b2, DropPlacement::Before)
        .unwrap();
    let moved_back = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(
        moved_back.iter().map(|b| b.id).collect::<Vec<_>>(),
        vec![b0, b1, b2, b3, b4]
    );

    repository
        .arrange_blocks(page, &[b1, b3], b0, DropPlacement::Before)
        .unwrap();
    let non_contiguous = repository.blocks(page, 0, 10).unwrap();
    assert_eq!(
        non_contiguous.iter().map(|b| b.id).collect::<Vec<_>>(),
        vec![b1, b3, b0, b2, b4]
    );
}


