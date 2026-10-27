fn text_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text() == label
        {
            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
        } else {
            None
        }
    })
}
use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Event, Modifiers};
use noteon::domain::model::{BlockKind, DatabaseData, InlineTextStyle, SaveBatch, TextStyleRange};
use noteon::storage::Repository;
use noteon::ui::{
    Intent,
    database::{Drafts, show_embedded},
    editor::Editor,
    palette::Palette,
};
use noteon::worker::Action;

fn repository() -> (tempfile::TempDir, Repository) {
    let directory = tempfile::tempdir().unwrap();
    let repository = Repository::open(&directory.path().join("transfers.sqlite")).unwrap();
    (directory, repository)
}

fn create_record(repository: &mut Repository, database: i64) -> i64 {
    let record = repository.create_database_record(database).unwrap();
    repository
        .rename_database_record(record, "Record with content")
        .unwrap();
    let mut block = repository.blocks(record, 0, 1).unwrap().remove(0);
    block.content = "Preserved content".into();
    block.kind = BlockKind::Todo;
    block.checked = true;
    block.text_styles.push(TextStyleRange {
        start: 0,
        end: 9,
        style: InlineTextStyle {
            bold: true,
            ..Default::default()
        },
    });
    repository
        .save(&SaveBatch {
            blocks: vec![block],
            ..Default::default()
        })
        .unwrap();
    record
}

#[test]
fn moving_record_to_page_preserves_identity_content_and_properties() {
    let (_directory, mut repository) = repository();
    let database = repository.create_database(None).unwrap();
    let record = create_record(&mut repository, database);
    repository
        .add_database_property(database, "Effort", "number", &[])
        .unwrap();
    let property = repository
        .database(database)
        .unwrap()
        .properties
        .into_iter()
        .find(|property| property.name == "Effort")
        .unwrap();
    repository
        .set_database_property(record, property.id, "42")
        .unwrap();
    let destination = repository.create_page(None, "Destination").unwrap();
    let empty = repository.blocks(destination, 0, 1).unwrap()[0].id;
    let original = repository.blocks(record, 0, 1).unwrap().remove(0);
    assert_eq!(
        repository
            .transfer_pages(&[record], Some(destination), Some(0), Some(empty), false)
            .unwrap(),
        [record]
    );
    assert_eq!(
        repository.page(record).unwrap().parent_id,
        Some(destination)
    );
    assert!(repository.database(database).unwrap().rows.is_empty());
    assert_eq!(repository.blocks(record, 0, 1).unwrap()[0], original);
    assert!(
        repository
            .page_properties(record)
            .unwrap()
            .iter()
            .any(|property| property.name == "Effort" && property.value == "42")
    );
    repository.ensure_subpage_blocks(destination).unwrap();
    let blocks = repository.blocks(destination, 0, 10).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].kind, BlockKind::PageLink);
    assert_eq!(blocks[0].content, record.to_string());
    repository
        .transfer_pages(&[record], None, None, None, false)
        .unwrap();
    assert_eq!(repository.page(record).unwrap().parent_id, None);
    assert!(repository.blocks(destination, 0, 10).unwrap().is_empty());
}

#[test]
fn copying_multiple_same_named_records_creates_distinct_page_blocks() {
    let (_directory, mut repository) = repository();
    let table = repository.create_database(None).unwrap();
    let records = [
        create_record(&mut repository, table),
        create_record(&mut repository, table),
    ];
    let destination = repository.create_page(None, "Destination").unwrap();
    let empty = repository.blocks(destination, 0, 1).unwrap()[0].id;
    let copies = repository
        .transfer_pages(&records, Some(destination), Some(0), Some(empty), true)
        .unwrap();
    repository.ensure_subpage_blocks(destination).unwrap();
    let blocks = repository.blocks(destination, 0, 10).unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        blocks
            .iter()
            .map(|block| block.content.parse::<i64>().unwrap())
            .collect::<Vec<_>>(),
        copies
    );
    assert_ne!(copies[0], copies[1]);
    assert!(copies.iter().all(|copy| !records.contains(copy)));
    assert_eq!(repository.database(table).unwrap().rows.len(), 2);
}

#[test]
fn copying_record_clones_children_embedded_tables_and_formatted_content() {
    let (_directory, mut repository) = repository();
    let source = repository.create_database(None).unwrap();
    let record = create_record(&mut repository, source);
    let child = repository.create_page(Some(record), "Child").unwrap();
    let embedded = repository.create_database_block(record, 1).unwrap();
    let nested_record = create_record(&mut repository, embedded);
    repository
        .add_database_property(embedded, "Score", "number", &[])
        .unwrap();
    let property = repository
        .database(embedded)
        .unwrap()
        .properties
        .into_iter()
        .find(|property| property.name == "Score")
        .unwrap();
    repository
        .set_database_property(nested_record, property.id, "7")
        .unwrap();
    let destination = repository.create_page(None, "Destination").unwrap();
    let copied = repository
        .transfer_pages(&[record], Some(destination), Some(0), None, true)
        .unwrap()[0];
    assert_ne!(copied, record);
    assert_eq!(repository.page(record).unwrap().parent_id, Some(source));
    let original = repository.blocks(record, 0, 1).unwrap().remove(0);
    let cloned = repository.blocks(copied, 0, 1).unwrap().remove(0);
    assert_ne!(original.id, cloned.id);
    assert_eq!(original.content, cloned.content);
    assert_eq!(original.text_styles, cloned.text_styles);
    assert_eq!(original.checked, cloned.checked);
    let pages = repository.list_pages().unwrap();
    assert!(
        pages.iter().any(|page| page.parent_id == Some(copied)
            && page.title == "Child"
            && page.id != child)
    );
    let embedded_copy = pages
        .iter()
        .find(|page| page.parent_id == Some(copied) && page.is_database)
        .unwrap()
        .id;
    assert_ne!(embedded_copy, embedded);
    assert!(
        repository
            .blocks(copied, 0, 10)
            .unwrap()
            .iter()
            .any(|block| block.kind == BlockKind::Database
                && block.content == embedded_copy.to_string())
    );
    let data = repository.database(embedded_copy).unwrap();
    let score = data
        .properties
        .iter()
        .find(|property| property.name == "Score")
        .unwrap();
    assert_ne!(score.id, property.id);
    assert_eq!(data.rows[0].values[&score.id], "7");
    assert_ne!(data.rows[0].page.id, nested_record);
    repository.delete_pages_permanently(&[record]).unwrap();
    assert_eq!(
        repository.blocks(copied, 0, 1).unwrap()[0].content,
        original.content
    );
    assert_eq!(repository.database(embedded_copy).unwrap().rows.len(), 1);
}

#[test]
fn moving_between_tables_maps_properties_and_failed_moves_are_atomic() {
    let (_directory, mut repository) = repository();
    let source = repository.create_database(None).unwrap();
    let destination = repository.create_database(None).unwrap();
    let record = create_record(&mut repository, source);
    repository
        .add_database_property(source, "Effort", "number", &[])
        .unwrap();
    repository
        .add_database_property(destination, "Effort", "text", &[])
        .unwrap();
    let property = repository
        .database(source)
        .unwrap()
        .properties
        .into_iter()
        .find(|property| property.name == "Effort")
        .unwrap();
    repository
        .set_database_property(record, property.id, "42")
        .unwrap();
    repository
        .transfer_pages(&[record], Some(destination), None, None, false)
        .unwrap();
    let data = repository.database(destination).unwrap();
    let effort = data
        .properties
        .iter()
        .find(|property| property.kind == "number")
        .unwrap();
    assert_eq!(data.rows[0].values[&effort.id], "42");
    assert!(repository.database(source).unwrap().rows.is_empty());
    let child = repository.create_page(Some(record), "Child").unwrap();
    let second = repository.create_database_record(destination).unwrap();
    let before = repository.list_pages().unwrap().len();
    assert!(
        repository
            .transfer_pages(&[second, record], Some(child), None, None, false)
            .is_err()
    );
    assert_eq!(
        repository.page(second).unwrap().parent_id,
        Some(destination)
    );
    assert_eq!(repository.list_pages().unwrap().len(), before);
    assert!(
        repository
            .transfer_pages(&[record], Some(child), None, None, true)
            .is_err()
    );
}

fn show_database(ui: &mut egui::Ui, data: &mut DatabaseData, intents: &mut Vec<Intent>) {
    show_embedded(
        ui,
        data,
        Palette::new(false),
        intents,
        &mut Drafts {
            properties: &mut HashMap::new(),
            titles: &mut HashMap::new(),
            values: &mut HashMap::new(),
        },
    );
}

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 1000.0),
        )),
        events,
        ..Default::default()
    }
}

#[test]
fn selected_record_copy_and_cut_paste_as_page_blocks_in_the_editor() {
    for (cut, focused) in [(false, false), (false, true), (true, false), (true, true)] {
        let (_directory, mut repository) = repository();
        let source = repository.create_database(None).unwrap();
        let record = create_record(&mut repository, source);
        let destination = repository.create_page(None, "Destination").unwrap();
        let mut data = repository.database(source).unwrap();
        let context = egui::Context::default();
        let selection = egui::Id::new(("database-selected-rows", source));
        context.data_mut(|state| state.insert_temp(selection, HashSet::from([record])));
        let clipboard_event = if cut {
            Event::Key {
                key: egui::Key::X,
                physical_key: Some(egui::Key::X),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL,
            }
        } else {
            Event::Copy
        };
        let output = context.run(input(vec![clipboard_event]), |context| {
            egui::CentralPanel::default()
                .show(context, |ui| show_database(ui, &mut data, &mut Vec::new()));
        });
        let copied = output
            .platform_output
            .commands
            .iter()
            .find_map(|command| {
                if let egui::OutputCommand::CopyText(text) = command {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .expect("selected records must be copied to the clipboard");
        assert_eq!(noteon::ui::page_transfer::is_cut(&context, record), cut);
        let mut editor = Editor::default();
        editor.bind(repository.document(destination).unwrap(), 1, None);
        let blocks = repository.blocks(destination, 0, 10).unwrap();
        let empty = blocks[0].id;
        editor.cache.insert(0, blocks);
        let _ = context.run(input(Vec::new()), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                editor.show(ui, Palette::new(false), &[]);
            });
        });
        if focused {
            context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", empty))));
        } else {
            editor.selected_blocks.insert(empty);
        }
        let mut intents = Vec::new();
        let _ = context.run(input(vec![Event::Paste(copied)]), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                intents.extend(editor.show(ui, Palette::new(false), &[]))
            });
        });
        let (pages, parent, position, replace_block, copy) = intents
            .into_iter()
            .find_map(|intent| {
                if let Intent::Action(Action::TransferPages {
                    pages,
                    parent,
                    position,
                    replace_block,
                    copy,
                }) = intent
                {
                    Some((pages, parent, position, replace_block, copy))
                } else {
                    None
                }
            })
            .expect("paste must insert a page, rather than text");
        assert_eq!(copy, !cut);
        assert_eq!(replace_block, Some(empty));
        let pasted = repository
            .transfer_pages(&pages, parent, position, replace_block, copy)
            .unwrap()[0];
        assert_eq!(pasted == record, cut);
        assert_eq!(
            repository.blocks(pasted, 0, 1).unwrap()[0].content,
            "Preserved content"
        );
    }
}

#[test]
fn selected_table_blocks_copy_and_cut_paste_with_their_content() {
    for cut in [false, true] {
        for focused in [false, true] {
            let (_directory, mut repository) = repository();
            let source = repository.create_page(None, "Source").unwrap();
            let database = repository.create_database_block(source, 1).unwrap();
            let record = create_record(&mut repository, database);
            repository
                .add_database_property(database, "Score", "number", &[])
                .unwrap();
            let score = repository
                .database(database)
                .unwrap()
                .properties
                .into_iter()
                .find(|property| property.name == "Score")
                .unwrap();
            repository
                .set_database_property(record, score.id, "42")
                .unwrap();
            let mut settings = repository.database(database).unwrap().view_settings;
            settings.table_visible_properties = Some(vec![score.id]);
            settings.table_column_widths.insert(score.id, 180.0);
            repository
                .set_database_view_settings(database, &settings)
                .unwrap();
            repository
                .set_database_sort(database, &score.id.to_string(), false)
                .unwrap();
            let mut before = repository.blocks(source, 0, 1).unwrap().remove(0);
            before.kind = BlockKind::Heading2;
            before.content = "Before the table".into();
            before.text_styles.push(TextStyleRange {
                start: 0,
                end: 6,
                style: InlineTextStyle {
                    bold: true,
                    ..Default::default()
                },
            });
            repository
                .save(&SaveBatch {
                    blocks: vec![before.clone()],
                    ..Default::default()
                })
                .unwrap();
            repository
                .append_blocks(
                    source,
                    2,
                    &[(BlockKind::Text, "After the table".into(), false)],
                )
                .unwrap();
            let original = repository.blocks(source, 0, 10).unwrap();
            let context = egui::Context::default();
            noteon::ui::theme::configure(&context);
            let mut editor = Editor::default();
            editor.bind(repository.document(source).unwrap(), 1, None);
            editor.cache.insert(0, original.clone());
            editor
                .selected_blocks
                .extend(original.iter().map(|block| block.id));
            let mut intents = Vec::new();
            let output = context.run(
                input(vec![if cut { Event::Cut } else { Event::Copy }]),
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        intents.extend(editor.show(ui, Palette::new(false), &[]));
                    });
                },
            );
            let copied = output
                .platform_output
                .commands
                .iter()
                .find_map(|command| {
                    if let egui::OutputCommand::CopyText(text) = command {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .expect("selected table must produce clipboard content");
            assert!(copied.contains("noteon://page/"));
            if cut {
                let ids = intents
                    .iter()
                    .find_map(|intent| {
                        if let Intent::Action(Action::DeleteBlocks { blocks, .. }) = intent {
                            Some(blocks)
                        } else {
                            None
                        }
                    })
                    .expect("cut removes the selected blocks");
                repository.delete_blocks(source, ids).unwrap();
            }
            let destination = repository.create_page(None, "Destination").unwrap();
            editor.bind(repository.document(destination).unwrap(), 2, None);
            let blocks = repository.blocks(destination, 0, 10).unwrap();
            let empty = blocks[0].id;
            editor.cache.insert(0, blocks);
            for _ in 0..2 {
                let _ = context.run(input(Vec::new()), |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        editor.show(ui, Palette::new(false), &[]);
                    });
                });
            }
            if focused {
                context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", empty))));
            } else {
                editor.selected_blocks.insert(empty);
            }
            intents.clear();
            let _ = context.run(input(vec![Event::Paste(copied)]), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    intents.extend(editor.show(ui, Palette::new(false), &[]));
                });
            });
            let (page, position, replace_block, blocks) = intents
                .into_iter()
                .find_map(|intent| {
                    if let Intent::Action(Action::PasteBlocks {
                        page,
                        position,
                        replace_block,
                        blocks,
                    }) = intent
                    {
                        Some((page, position, replace_block, blocks))
                    } else {
                        None
                    }
                })
                .expect("paste must preserve the table block type");
            assert_eq!(page, destination);
            assert_eq!(replace_block, Some(empty));
            repository
                .paste_blocks(page, position, replace_block, &blocks)
                .unwrap();
            repository.ensure_subpage_blocks(destination).unwrap();
            let pasted = repository.blocks(destination, 0, 10).unwrap();
            assert_eq!(pasted.len(), 3);
            assert_eq!(pasted[0].kind, before.kind);
            assert_eq!(pasted[0].text_styles, before.text_styles);
            assert_eq!(pasted[1].kind, BlockKind::Database);
            assert_eq!(pasted[2].content, "After the table");
            let cloned = pasted[1].content.parse::<i64>().unwrap();
            assert_ne!(cloned, database);
            let cloned_data = repository.database(cloned).unwrap();
            let cloned_score = cloned_data
                .properties
                .iter()
                .find(|property| property.name == "Score")
                .unwrap();
            assert_ne!(cloned_score.id, score.id);
            assert_eq!(
                cloned_data.view_settings.table_visible_properties,
                Some(vec![cloned_score.id])
            );
            assert_eq!(
                cloned_data.view_settings.table_column_widths[&cloned_score.id],
                180.0
            );
            assert_eq!(cloned_data.sort_key, cloned_score.id.to_string());
            assert_eq!(cloned_data.rows.len(), 1);
            assert_eq!(cloned_data.rows[0].values[&cloned_score.id], "42");
            assert_ne!(cloned_data.rows[0].page.id, record);
            assert_eq!(
                repository
                    .blocks(cloned_data.rows[0].page.id, 0, 1)
                    .unwrap()[0]
                    .content,
                "Preserved content"
            );
            repository
                .set_database_property(cloned_data.rows[0].page.id, cloned_score.id, "99")
                .unwrap();
            assert_eq!(
                repository.database(database).unwrap().rows[0].values[&score.id],
                "42"
            );
        }
    }
}

#[test]
fn table_paste_is_atomic_and_can_be_repeated_on_the_source_page() {
    let (_directory, mut repository) = repository();
    let source = repository.create_page(None, "Source").unwrap();
    let database = repository.create_database_block(source, 1).unwrap();
    let record = create_record(&mut repository, database);
    let table = repository.blocks(source, 1, 1).unwrap().remove(0);
    let destination = repository.create_page(None, "Destination").unwrap();
    let empty = repository.blocks(destination, 0, 1).unwrap()[0].id;
    let before_pages = repository.list_pages().unwrap().len();
    let mut invalid = table.clone();
    invalid.content = "missing".into();
    assert!(
        repository
            .paste_blocks(destination, 0, Some(empty), &[table.clone(), invalid])
            .is_err()
    );
    assert_eq!(repository.list_pages().unwrap().len(), before_pages);
    assert_eq!(repository.blocks(destination, 0, 1).unwrap()[0].id, empty);
    assert!(
        repository
            .paste_blocks(record, 0, None, std::slice::from_ref(&table))
            .is_err()
    );
    for _ in 0..2 {
        repository
            .paste_blocks(source, 2, None, std::slice::from_ref(&table))
            .unwrap();
    }
    let tables: Vec<_> = repository
        .blocks(source, 0, 10)
        .unwrap()
        .into_iter()
        .filter(|block| block.kind == BlockKind::Database)
        .collect();
    assert_eq!(tables.len(), 3);
    assert_ne!(tables[1].content, tables[2].content);
    repository.delete_pages_permanently(&[database]).unwrap();
    for table in &tables[1..] {
        assert_eq!(
            repository
                .database(table.content.parse().unwrap())
                .unwrap()
                .rows
                .len(),
            1
        );
    }
}

fn pointer(position: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn focused_property_selects_and_copies_its_text_instead_of_selected_records() {
    let (_directory, mut repository) = repository();
    let table = repository.create_database(None).unwrap();
    let record = create_record(&mut repository, table);
    repository
        .add_database_property(table, "Text", "text", &[])
        .unwrap();
    let property = repository
        .database(table)
        .unwrap()
        .properties
        .into_iter()
        .find(|property| property.name == "Text")
        .unwrap();
    let value = "Property text 🐦";
    repository
        .set_database_property(record, property.id, value)
        .unwrap();
    let mut data = repository.database(table).unwrap();
    let context = egui::Context::default();
    let mut frame = |events| {
        context.run(input(events), |context| {
            egui::CentralPanel::default()
                .show(context, |ui| show_database(ui, &mut data, &mut Vec::new()));
        })
    };
    let output = frame(Vec::new());
    let point = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == value
            {
                Some(text.pos + egui::vec2(8.0, 6.0))
            } else {
                None
            }
        })
        .unwrap();
    frame(vec![Event::PointerMoved(point)]);
    frame(vec![pointer(point, true)]);
    frame(vec![pointer(point, false)]);
    let field = context.memory(|memory| memory.focused()).unwrap();
    let selection_id = egui::Id::new(("database-selected-rows", table));
    context.data_mut(|state| state.insert_temp(selection_id, HashSet::from([record])));
    frame(vec![Event::Key {
        key: egui::Key::A,
        physical_key: Some(egui::Key::A),
        pressed: true,
        repeat: false,
        modifiers: Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    }]);
    assert_eq!(context.memory(|memory| memory.focused()), Some(field));
    let range = egui::text_edit::TextEditState::load(&context, field)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!(range.primary.index.min(range.secondary.index), 0);
    assert_eq!(
        range.primary.index.max(range.secondary.index),
        value.chars().count()
    );
    assert_eq!(
        context
            .data(|state| state.get_temp::<HashSet<i64>>(selection_id))
            .unwrap(),
        HashSet::from([record])
    );
    let output = frame(vec![Event::Copy]);
    assert!(
        output
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == value))
    );
}

#[test]
fn name_cell_whitespace_focuses_editing_and_only_the_hover_button_opens_the_page() {
    let (_directory, mut repository) = repository();
    let table = repository.create_database(None).unwrap();
    let record = create_record(&mut repository, table);
    let mut data = repository.database(table).unwrap();
    let context = egui::Context::default();
    let mut frame = |events| {
        let mut intents = Vec::new();
        let output = context.run(input(events), |context| {
            egui::CentralPanel::default()
                .show(context, |ui| show_database(ui, &mut data, &mut intents));
        });
        (intents, output)
    };

    let (_, output) = frame(Vec::new());
    assert!(
        text_rect(&output, "↗").is_none(),
        "the open button stays hidden until hover"
    );
    let name = text_rect(&output, "Record with content").unwrap();
    let blank = name.right_center() + egui::vec2(16.0, 0.0);
    let (_, output) = frame(vec![Event::PointerMoved(blank)]);
    let open_button =
        text_rect(&output, "↗").expect("hovering the name cell shows its open button");
    frame(vec![pointer(blank, true)]);
    let (intents, _) = frame(vec![pointer(blank, false)]);
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::Open(_, _)))
    );
    let focused = context
        .memory(|memory| memory.focused())
        .expect("the record name must receive focus");
    let state = egui::text_edit::TextEditState::load(&context, focused)
        .expect("the name is focused for text editing");
    let range = state.cursor.char_range().unwrap();
    assert_eq!(
        range.primary.index.abs_diff(range.secondary.index),
        "Record with content".chars().count()
    );
    let outside = egui::pos2(1200.0, 900.0);
    frame(vec![Event::PointerMoved(outside), pointer(outside, true)]);
    frame(vec![pointer(outside, false)]);
    let open = open_button.center();
    frame(vec![Event::PointerMoved(open)]);
    frame(vec![pointer(open, true)]);
    let (intents, _) = frame(vec![pointer(open, false)]);
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::Open(id, None) if *id == record))
    );
}

#[test]
fn record_names_still_save_after_clicking_to_rename() {
    let (_directory, mut repository) = repository();
    let table = repository.create_database(None).unwrap();
    let record = create_record(&mut repository, table);
    let mut data = repository.database(table).unwrap();
    let context = egui::Context::default();
    let mut titles = HashMap::new();
    let mut properties = HashMap::new();
    let mut values = HashMap::new();
    let mut frame = |events| {
        let mut intents = Vec::new();
        let output = context.run(input(events), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                show_embedded(
                    ui,
                    &mut data,
                    Palette::new(false),
                    &mut intents,
                    &mut Drafts {
                        properties: &mut properties,
                        titles: &mut titles,
                        values: &mut values,
                    },
                );
            });
        });
        (intents, output)
    };
    let (_, output) = frame(Vec::new());
    let name = text_rect(&output, "Record with content").unwrap();
    let start = name.right_center() + egui::vec2(16.0, 0.0);
    frame(vec![Event::PointerMoved(start)]);
    frame(vec![pointer(start, true)]);
    frame(vec![pointer(start, false)]);
    frame(vec![Event::Text("Renamed record".into())]);
    let outside = egui::pos2(1200.0, 900.0);
    let (mut intents, _) = frame(vec![Event::PointerMoved(outside), pointer(outside, true)]);
    intents.extend(frame(vec![pointer(outside, false)]).0);
    intents.extend(frame(Vec::new()).0);
    assert!(intents.iter().any(|intent| matches!(intent, Intent::Action(Action::RenameDatabaseRecord { page, title }) if *page == record && title == "Renamed record")));
}

#[test]
fn dragging_a_record_moves_it_to_a_page_or_another_table() {
    for to_table in [false, true] {
        let (_directory, mut repository) = repository();
        let source = repository.create_database(None).unwrap();
        let destination = if to_table {
            repository.create_database(None).unwrap()
        } else {
            repository.create_page(None, "Destination").unwrap()
        };
        let record = create_record(&mut repository, source);
        let mut source_data = repository.database(source).unwrap();
        let mut destination_data = to_table.then(|| repository.database(destination).unwrap());
        let mut editor = Editor::default();
        editor.bind(repository.document(destination).unwrap(), 1, None);
        let blocks = repository.blocks(destination, 0, 10).unwrap();
        let destination_block = blocks[0].id;
        editor.cache.insert(0, blocks);
        let context = egui::Context::default();
        noteon::ui::theme::configure(&context);
        let mut frame = |events| {
            let mut intents = Vec::new();
            let mut target_rect = egui::Rect::NOTHING;
            let output = context.run(input(events), |context| {
                egui::SidePanel::left("source")
                    .exact_width(600.0)
                    .show(context, |ui| {
                        show_database(ui, &mut source_data, &mut intents)
                    });
                egui::CentralPanel::default().show(context, |ui| {
                    target_rect = ui.max_rect();
                    if let Some(data) = destination_data.as_mut() {
                        show_database(ui, data, &mut intents);
                    } else {
                        intents.extend(editor.show(ui, Palette::new(false), &[]));
                        target_rect = context
                            .read_response(egui::Id::new(("row", destination_block)))
                            .unwrap()
                            .rect;
                    }
                });
            });
            (intents, output, target_rect)
        };
        let (_, output, target) = frame(Vec::new());
        let name = text_rect(&output, "Record with content").expect("record name must be rendered");
        let start = name.left_center() - egui::vec2(16.0, 0.0);
        let (_, _hovered, _) = frame(vec![Event::PointerMoved(start)]);
        frame(vec![Event::PointerMoved(start), pointer(start, true)]);
        let end = if to_table {
            egui::pos2(target.left() + 100.0, start.y)
        } else {
            target.center()
        };
        frame(vec![Event::PointerMoved(end)]);
        let payload =
            egui::DragAndDrop::payload::<noteon::ui::page_transfer::PageDragPayload>(&context)
                .expect("record icon must initiate a page drag");
        assert_eq!(payload.pages, [record]);
        let (intents, _, _) = frame(vec![pointer(end, false)]);
        assert!(intents.iter().any(|intent| matches!(intent, Intent::Action(Action::TransferPages { pages, parent: Some(parent), copy: false, .. }) if pages == &[record] && *parent == destination)));
    }
}
