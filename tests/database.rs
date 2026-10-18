use std::collections::{HashMap, HashSet};
use eframe::egui::{self, Event, Key, Modifiers};
use noteon::domain::model::{DatabaseData, DatabaseRow, DatabaseView, DatabaseViewSettings, Page, PageId};
use noteon::ui::{Intent, database::{Drafts, show_embedded}, palette::Palette};

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
            show_embedded(ui, database, Palette::new(false), &mut intents, &mut drafts);
        });
    });
    intents
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
fn table_marquee_drag_selects_rows() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut database = test_database();

    // 1. Initial render
    frame_database(&context, &mut database, Vec::new(), Modifiers::NONE);

    let selection_id = egui::Id::new(("database-selected-rows", database.page_id));

    // 2. Drag across rows in 3 frames: press, drag, release
    let start_pos = egui::pos2(50.0, 160.0);
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
        .data(|d| d.get_temp(egui::Id::new(("database-table-marquee-anchor", database.page_id))))
        .flatten();
    assert_eq!(anchor, Some(start_pos));

    frame_database(
        &context,
        &mut database,
        vec![Event::PointerMoved(drag_pos)],
        Modifiers::NONE,
    );

    let active: bool = context
        .data(|d| d.get_temp(egui::Id::new(("database-table-marquee-active", database.page_id))))
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

