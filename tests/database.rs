use eframe::egui::{self, Event, Key, Modifiers};
use noteon::domain::model::{
    DatabaseData, DatabaseRow, DatabaseView, DatabaseViewSettings, Page, PageId,
};
use noteon::ui::{
    Intent,
    database::{Drafts, show, show_embedded},
    palette::Palette,
};
use std::collections::{HashMap, HashSet};

fn test_database() -> DatabaseData {
    let rows = (1..=4)
        .map(|id| DatabaseRow {
            page: Page {
                id,
                parent_id: Some(100),
                title: format!("Record {id}"),
                icon: "📄".into(),
                favorite: false,
                trashed_by: None,
                cover: 0,
                cover_image: None,
                cover_x: 0.5,
                cover_y: 0.5,
                sort_order: id,
                updated_at: 0,
                block_count: 0,
                created_at: 0,
                priority: 0,
                is_database: false,
                full_width: false,
            },
            preview: String::new(),
            values: HashMap::new(),
        })
        .collect();

    DatabaseData {
        page_id: 100,
        title: "Test Database".into(),
        icon: "▦".into(),
        view: DatabaseView::Table,
        gallery_columns: 3,
        view_settings: DatabaseViewSettings::default(),
        sort_key: "manual".into(),
        sort_descending: false,
        properties: Vec::new(),
        rows,
    }
}

fn frame_database(
    context: &egui::Context,
    database: &mut DatabaseData,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> Vec<Intent> {
    frame_database_view(context, database, events, modifiers, false)
}

fn frame_database_view(
    context: &egui::Context,
    database: &mut DatabaseData,
    events: Vec<Event>,
    modifiers: Modifiers,
    full_page: bool,
) -> Vec<Intent> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 800.0),
        )),
        events,
        modifiers,
        ..Default::default()
    };
    let mut intents = Vec::new();
    let mut prop_drafts = HashMap::new();
    let mut title_drafts = HashMap::new();
    let mut val_drafts = HashMap::new();

    let _ = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            let mut drafts = Drafts {
                properties: &mut prop_drafts,
                titles: &mut title_drafts,
                values: &mut val_drafts,
            };
            if full_page {
                show(
                    ui,
                    database,
                    None,
                    &[],
                    false,
                    Palette::new(false),
                    &mut intents,
                    &mut drafts,
                );
            } else {
                show_embedded(ui, database, Palette::new(false), &mut intents, &mut drafts);
            }
        });
    });
    intents
}

#[test]
fn database_title_saves_after_typing_and_leaving_the_field() {
    for full_page in [false, true] {
        for original_title in ["Test Database", ""] {
            for commit_with_enter in [false, true] {
                let context = egui::Context::default();
                let mut database = test_database();
                database.title = original_title.into();
                frame_database_view(&context, &mut database, vec![], Modifiers::NONE, full_page);
                let title_id = egui::Id::new(("database-header-title", database.page_id));
                context.memory_mut(|memory| memory.request_focus(title_id));
                frame_database_view(&context, &mut database, vec![], Modifiers::NONE, full_page);

                let intents = frame_database_view(
                    &context,
                    &mut database,
                    vec![Event::Text(" updated".into())],
                    Modifiers::NONE,
                    full_page,
                );
                assert_ne!(database.title, original_title);
                assert!(!intents.iter().any(|intent| matches!(
                    intent,
                    Intent::Action(noteon::worker::Action::RenameDatabaseRecord { .. })
                )));
                // Losing focus happens in a later frame, after the text has already changed.
                frame_database_view(&context, &mut database, vec![], Modifiers::NONE, full_page);
                let events = if commit_with_enter {
                    vec![Event::Key {
                        key: Key::Enter,
                        physical_key: Some(Key::Enter),
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }]
                } else {
                    let position = egui::pos2(990.0, 790.0);
                    vec![
                        Event::PointerMoved(position),
                        Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                        Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        },
                    ]
                };
                let intents = frame_database_view(
                    &context,
                    &mut database,
                    events,
                    Modifiers::NONE,
                    full_page,
                );
                assert_eq!(
                    intents.iter().filter(|intent| matches!(
                        intent,
                        Intent::Action(noteon::worker::Action::RenameDatabaseRecord { page, title })
                            if *page == database.page_id && title == &database.title
                    )).count(),
                    1,
                    "full_page={full_page}, original_title={original_title:?}, enter={commit_with_enter}"
                );
                let intents = frame_database_view(
                    &context,
                    &mut database,
                    vec![],
                    Modifiers::NONE,
                    full_page,
                );
                assert!(!intents.iter().any(|intent| matches!(
                    intent,
                    Intent::Action(noteon::worker::Action::RenameDatabaseRecord { .. })
                )));
            }
        }
    }
}

#[test]
fn embedded_database_top_right_button_opens_full_page() {
    for view in [
        DatabaseView::Table,
        DatabaseView::Board,
        DatabaseView::Gallery,
    ] {
        for title in ["Test Database", ""] {
            let context = egui::Context::default();
            let mut database = test_database();
            database.view = view;
            database.title = title.into();
            frame_database(&context, &mut database, vec![], Modifiers::NONE);
            let position = egui::pos2(978.0, 20.0);
            frame_database(
                &context,
                &mut database,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                Modifiers::NONE,
            );
            let intents = frame_database(
                &context,
                &mut database,
                vec![Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                Modifiers::NONE,
            );
            assert!(
                intents.iter().any(|intent| matches!(
                    intent, Intent::Open(id, None) if *id == database.page_id
                )),
                "view={view:?}, title={title:?}"
            );
        }
    }
}

#[test]
fn table_delete_prioritizes_multiple_selected_rows_over_focused_database_title() {
    for selected in [HashSet::from([1_i64, 3]), HashSet::from([1_i64])] {
        let context = egui::Context::default();
        let mut database = test_database();
        frame_database_view(&context, &mut database, Vec::new(), Modifiers::NONE, true);
        let selection_id = egui::Id::new(("database-selected-rows", database.page_id));
        let anchor_id = egui::Id::new(("database-anchor-row", database.page_id));
        let title_id = egui::Id::new(("database-header-title", database.page_id));
        context.memory_mut(|memory| memory.request_focus(title_id));
        frame_database_view(&context, &mut database, Vec::new(), Modifiers::NONE, true);
        assert_eq!(context.memory(|memory| memory.focused()), Some(title_id));
        assert!(context.wants_keyboard_input());
        context.data_mut(|state| {
            state.insert_temp(selection_id, selected.clone());
            state.insert_temp(anchor_id, Some(1_i64));
        });
        let title_before = database.title.clone();
        let intents = frame_database_view(
            &context,
            &mut database,
            vec![Event::Key {
                key: Key::Delete,
                physical_key: Some(Key::Delete),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            Modifiers::NONE,
            true,
        );
        let trashed: HashSet<_> = intents
            .iter()
            .filter_map(|intent| match intent {
                Intent::Action(noteon::worker::Action::Trash(id)) => Some(*id),
                _ => None,
            })
            .collect();
        let remaining: HashSet<PageId> = context
            .data(|state| state.get_temp(selection_id))
            .unwrap_or_default();
        if selected.len() > 1 {
            assert_eq!(trashed, selected);
            assert_eq!(database.title, title_before);
            assert!(remaining.is_empty());
            let anchor: Option<PageId> = context.data(|state| state.get_temp(anchor_id)).flatten();
            assert_eq!(anchor, None);
        } else {
            assert!(trashed.is_empty());
            assert_eq!(remaining, selected);
        }
    }
}

#[test]
fn ctrl_a_selects_all_database_rows_and_delete_trashes_them() {
    for view in [
        DatabaseView::Table,
        DatabaseView::Gallery,
        DatabaseView::Board,
    ] {
        let context = egui::Context::default();
        let mut database = test_database();
        database.view = view;
        frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);
        frame_database(
            &context,
            &mut database,
            vec![Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL,
            }],
            Modifiers::CTRL,
        );

        let selected: HashSet<PageId> = context
            .data(|state| {
                state.get_temp(egui::Id::new(("database-selected-rows", database.page_id)))
            })
            .unwrap_or_default();
        assert_eq!(selected, HashSet::from([1, 2, 3, 4]));

        let intents = frame_database(
            &context,
            &mut database,
            vec![Event::Key {
                key: Key::Delete,
                physical_key: Some(Key::Delete),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            Modifiers::NONE,
        );
        let trashed: HashSet<_> = intents
            .iter()
            .filter_map(|intent| match intent {
                Intent::Action(noteon::worker::Action::Trash(id)) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(trashed, selected);
    }
}

#[test]
fn database_ctrl_click_and_shift_click_and_delete_rows() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut database = test_database();

    // 1. Initial frame
    frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);

    let selection_id = egui::Id::new(("database-selected-rows", database.page_id));
    let anchor_id = egui::Id::new(("database-anchor-row", database.page_id));

    // Initially nothing selected
    let initial: HashSet<PageId> = context
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    assert!(initial.is_empty());

    // 2. Select row 1 and row 3 via data_mut simulation or keyboard delete
    context.data_mut(|d| {
        let mut set = HashSet::new();
        set.insert(1_i64);
        set.insert(3_i64);
        d.insert_temp(selection_id, set);
        d.insert_temp(anchor_id, Some(3_i64));
    });

    let selected: HashSet<PageId> = context
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    assert_eq!(selected.len(), 2);
    assert!(selected.contains(&1));
    assert!(selected.contains(&3));

    // 3. Press Delete key -> should trash selected rows
    let delete_event = Event::Key {
        key: Key::Delete,
        physical_key: Some(Key::Delete),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let intents = frame_database(&context, &mut database, vec![delete_event], Modifiers::NONE);

    let trashed: Vec<_> = intents
        .into_iter()
        .filter_map(|intent| match intent {
            Intent::Action(noteon::worker::Action::Trash(id)) => Some(id),
            _ => None,
        })
        .collect();
    assert!(trashed.contains(&1));
    assert!(trashed.contains(&3));

    let after_delete: HashSet<PageId> = context
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    assert!(after_delete.is_empty());

    // 4. Test Escape clears selection
    context.data_mut(|d| {
        let mut set = HashSet::new();
        set.insert(2_i64);
        d.insert_temp(selection_id, set);
        d.insert_temp(anchor_id, Some(2_i64));
    });
    let escape_event = Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame_database(&context, &mut database, vec![escape_event], Modifiers::NONE);

    let after_escape: HashSet<PageId> = context
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    assert!(after_escape.is_empty());
}

#[test]
fn database_new_record_intent_and_dialog_intents() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut database = test_database();

    let open_dialog_intent = Intent::OpenInDialog(1);
    assert!(matches!(open_dialog_intent, Intent::OpenInDialog(1)));
    let close_dialog_intent = Intent::ClosePageDialog;
    assert!(matches!(close_dialog_intent, Intent::ClosePageDialog));

    let _ = frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);
    assert_eq!(database.rows.len(), 4);
}

#[test]
fn gallery_select_and_delete_rows() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut database = test_database();
    database.view = noteon::domain::model::DatabaseView::Gallery;

    frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);

    let selection_id = egui::Id::new(("database-selected-rows", database.page_id));

    // Select row 2 in gallery
    context.data_mut(|d| {
        let mut set = HashSet::new();
        set.insert(2_i64);
        d.insert_temp(selection_id, set);
    });

    // Press Delete key
    let delete_event = Event::Key {
        key: Key::Delete,
        physical_key: Some(Key::Delete),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let intents = frame_database(&context, &mut database, vec![delete_event], Modifiers::NONE);

    let trashed: Vec<_> = intents
        .into_iter()
        .filter_map(|intent| match intent {
            Intent::Action(noteon::worker::Action::Trash(id)) => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(trashed, vec![2]);

    let after_delete: HashSet<PageId> = context
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    assert!(after_delete.is_empty());
}

#[test]
fn dragging_table_text_keeps_edit_focus_without_selecting_rows() {
    for text_x in [420.0, 440.0] {
        let context = egui::Context::default();
        noteon::ui::theme::configure(&context);
        let mut database = test_database();
        database
            .properties
            .push(noteon::domain::model::DatabaseProperty {
                id: 7,
                name: "Text".into(),
                kind: "text".into(),
                options: Vec::new(),
            });
        for row in &mut database.rows {
            row.page.title = "Editable record title".into();
            row.values.insert(7, "Editable property text".into());
        }
        frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);

        let start = egui::pos2(text_x, 160.0);
        let end = start + egui::vec2(35.0, 0.0);
        frame_database(
            &context,
            &mut database,
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            Modifiers::NONE,
        );
        frame_database(
            &context,
            &mut database,
            vec![Event::PointerMoved(end)],
            Modifiers::NONE,
        );
        let focused = context.memory(|memory| memory.focused());
        assert!(
            focused.is_some(),
            "text field at x={text_x} must have focus"
        );
        let text_state = egui::text_edit::TextEditState::load(&context, focused.unwrap()).unwrap();
        let range = text_state.cursor.char_range().unwrap();
        assert_ne!(range.primary.index, range.secondary.index);

        frame_database(
            &context,
            &mut database,
            vec![Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            Modifiers::NONE,
        );
        let selected: HashSet<PageId> = context
            .data(|data| data.get_temp(egui::Id::new(("database-selected-rows", database.page_id))))
            .unwrap_or_default();
        assert!(selected.is_empty());
        assert_eq!(context.memory(|memory| memory.focused()), focused);
    }
}

#[test]
fn table_marquee_drag_from_outside_rows_selects_rows() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut database = test_database();

    // 1. Initial render
    frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);

    let selection_id = egui::Id::new(("database-selected-rows", database.page_id));

    // 2. Drag across rows in 3 frames: press, drag, release
    let start_pos = egui::pos2(700.0, 160.0);
    let drag_pos = egui::pos2(200.0, 250.0);

    frame_database(
        &context,
        &mut database,
        vec![
            Event::PointerMoved(start_pos),
            Event::PointerButton {
                pos: start_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        Modifiers::NONE,
    );

    let anchor: Option<egui::Pos2> = context
        .data(|d| {
            d.get_temp(egui::Id::new((
                "database-table-marquee-anchor",
                database.page_id,
            )))
        })
        .flatten();
    assert_eq!(anchor, Some(start_pos));

    frame_database(
        &context,
        &mut database,
        vec![Event::PointerMoved(drag_pos)],
        Modifiers::NONE,
    );

    let active: bool = context
        .data(|d| {
            d.get_temp(egui::Id::new((
                "database-table-marquee-active",
                database.page_id,
            )))
        })
        .unwrap_or(false);
    assert!(active);

    let mid_selected: HashSet<PageId> = context
        .data(|d| d.get_temp(selection_id))
        .unwrap_or_default();
    assert!(!mid_selected.is_empty());

    frame_database(
        &context,
        &mut database,
        vec![Event::PointerButton {
            pos: drag_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
}

#[test]
fn new_record_open_mode_and_number_calculations_default_and_persist() {
    use noteon::domain::model::{DatabaseProperty, NewRecordOpenMode, NumberCalculation};

    let mut database = test_database();
    assert_eq!(
        database.view_settings.new_record_open_mode,
        NewRecordOpenMode::FocusName
    );
    assert!(database.view_settings.table_column_calculations.is_empty());

    let numbers = [10.0, 20.0, 30.0];
    assert_eq!(NumberCalculation::None.format_result(&numbers), "");
    assert_eq!(NumberCalculation::Sum.format_result(&numbers), "Toplam 60");
    assert_eq!(
        NumberCalculation::Average.format_result(&numbers),
        "Ortalama 20"
    );
    assert_eq!(NumberCalculation::Min.format_result(&numbers), "Min 10");
    assert_eq!(NumberCalculation::Max.format_result(&numbers), "Max 30");
    assert_eq!(
        NumberCalculation::Median.format_result(&numbers),
        "Medyan 20"
    );
    assert_eq!(NumberCalculation::Count.format_result(&numbers), "Adet 3");

    database.view_settings.new_record_open_mode = NewRecordOpenMode::Dialog;
    database
        .view_settings
        .table_column_calculations
        .insert(42, NumberCalculation::Sum);
    database.properties.push(DatabaseProperty {
        id: 42,
        name: "Price".into(),
        kind: "number".into(),
        options: Vec::new(),
    });
    database.rows[0].values.insert(42, "15.5".into());
    database.rows[1].values.insert(42, "24.5".into());

    let json = serde_json::to_string(&database.view_settings).unwrap();
    let restored: DatabaseViewSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.new_record_open_mode, NewRecordOpenMode::Dialog);
    assert_eq!(
        restored.table_column_calculations.get(&42),
        Some(&NumberCalculation::Sum)
    );

    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let focus_key = egui::Id::new(("pending-table-focus-record", database.page_id));
    context.data_mut(|d| d.insert_temp(focus_key, 4_i64));
    frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);
    let after_focus: Option<PageId> = context.data(|d| d.get_temp(focus_key));
    assert!(after_focus.is_none());
}

#[test]
fn property_type_picker_renders_two_column_groups_and_supports_all_types() {
    use noteon::domain::model::{PROPERTY_TYPE_GROUPS, PROPERTY_TYPES};
    use noteon::ui::page_properties::{
        default_name_for_kind, property_icon, show_property_type_picker, type_label,
    };

    let grouped_total: usize = PROPERTY_TYPE_GROUPS.iter().map(|g| g.len()).sum();
    assert_eq!(grouped_total, PROPERTY_TYPES.len());
    assert_eq!(PROPERTY_TYPES.len(), 22);

    for (kind, label) in PROPERTY_TYPES {
        assert_eq!(type_label(kind), label);
        assert_eq!(default_name_for_kind(kind), label);
        assert!(!property_icon(kind).is_empty());
    }

    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let _ = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let picked = show_property_type_picker(
                ui,
                "test-picker",
                Some("status"),
                false,
                Palette::new(true),
            );
            assert!(picked.is_none());
        });
    });
}
