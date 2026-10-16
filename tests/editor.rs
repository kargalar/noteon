use eframe::egui::{self, Event, Key, Modifiers};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use noteon::domain::editing::split_at_selection;
use noteon::domain::model::{Block, BlockKind, BlockStub, Document, Page};
use noteon::ui::{BreadcrumbItem, Intent, editor::Editor, palette::Palette};

fn editor(content: &str, count: usize) -> Editor {
    let mut editor = Editor::default();
    editor.bind(
        Document {
            page: Page {
                id: 1,
                parent_id: None,
                title: "Test".into(),
                icon: "▤".into(),
                favorite: false,
                trashed_by: None,
                cover: 0,
                cover_image: None,
                cover_x: 0.5,
                cover_y: 0.5,
                sort_order: 0,
                updated_at: 0,
                block_count: count,
                created_at: 0,
                priority: 0,
                is_database: false,
                full_width: false,
            },
            outline: (0..count)
                .map(|position| BlockStub {
                    id: position as i64 + 1,
                    kind: BlockKind::Text,
                    chars: content.chars().count(),
                    lines: 1,
                    column_group: None,
                    column_index: 0,
                    column_width: 1.0,
                    media_width: 1.0,
                })
                .collect(),
            properties: Vec::new(),
        },
        1,
        None,
    );
    editor.cache.insert(
        0,
        (0..count.min(128))
            .map(|position| Block {
                id: position as i64 + 1,
                page_id: 1,
                position,
                kind: BlockKind::Text,
                content: content.into(),
                text_styles: Vec::new(),
                checked: false,
                column_group: None,
                column_index: 0,
                media_width: 1.0,
            })
            .collect(),
    );
    editor
}

fn frame(
    context: &egui::Context,
    editor: &mut Editor,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> Vec<Intent> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 860.0),
        )),
        events,
        modifiers,
        ..Default::default()
    };
    let mut intents = Vec::new();
    let _ = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            intents.extend(editor.show(ui, Palette::new(false), &[]));
        });
    });
    intents
}

fn enter(modifiers: Modifiers) -> Event {
    Event::Key {
        key: Key::Enter,
        physical_key: Some(Key::Enter),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn twenty_thousand_blocks_only_render_visible_rows() {
    let context = egui::Context::default();
    let mut editor = editor("Bir satır", 20_000);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    assert!(editor.rendered_blocks > 0);
    assert!(
        editor.rendered_blocks < 40,
        "rendered {} blocks",
        editor.rendered_blocks
    );
    editor.jump_to = Some(19_000);
    let intents = frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::LoadBlocks { start, .. } if *start > 18_000))
    );
}

#[test]
fn slash_command_accepts_trailing_space_without_adding_block() {
    let context = egui::Context::default();
    let mut editor = editor("/todo ", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 1_i64))));
    let intents = frame(
        &context,
        &mut editor,
        vec![enter(Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(_)))
    );
    assert!(intents.iter().any(|intent| matches!(intent, Intent::BlockChanged(block) if block.kind == BlockKind::Todo && block.content.is_empty())));
}

#[test]
fn shift_enter_stays_in_same_block() {
    let context = egui::Context::default();
    let mut editor = editor("Türkçe satır", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 1_i64))));
    let intents = frame(
        &context,
        &mut editor,
        vec![enter(Modifiers::SHIFT)],
        Modifiers::SHIFT,
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(_)))
    );
    assert!(editor.cache.blocks[&0].content.contains('\n'));
}

#[test]
fn enter_splits_unicode_selection_by_characters() {
    assert_eq!(
        split_at_selection("Türkçe 🌱 metin", 7..8),
        ("Türkçe ".into(), " metin".into())
    );
    assert_eq!(split_at_selection("çığ", 1..1), ("ç".into(), "ığ".into()));
    assert_eq!(
        split_at_selection("Not", 100..100),
        ("Not".into(), String::new())
    );
}

#[test]
fn docked_page_tabs_round_trip_with_their_panel_layout() {
    let mut dock = DockState::new(vec![1_i64, 2]);
    dock.main_surface_mut()
        .split_right(NodeIndex::root(), 0.65, vec![3]);

    struct Viewer;
    impl TabViewer for Viewer {
        type Tab = i64;

        fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
            format!("Page {tab}").into()
        }

        fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
            ui.label(format!("Page {tab}"));
        }
    }
    let context = egui::Context::default();
    let _ = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 860.0),
            )),
            ..Default::default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                DockArea::new(&mut dock).show_inside(ui, &mut Viewer);
            });
        },
    );

    let encoded = serde_json::to_string(&dock).unwrap();
    let restored: DockState<i64> = serde_json::from_str(&encoded).unwrap();

    assert!(restored.find_tab(&1).is_some());
    assert!(restored.find_tab(&2).is_some());
    assert!(restored.find_tab(&3).is_some());
}

#[test]
fn text_format_toolbar_dismisses_on_click_outside_and_escape() {
    let context = egui::Context::default();
    let mut editor = editor("Hello world", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    let active_selection_id = egui::Id::new("active-text-format-selection");
    let selection_id = egui::Id::new(("format-selection", 1_i64));
    let editor_id = egui::Id::new(("block", 1_i64));

    context.data_mut(|d| {
        d.insert_temp(selection_id, 0..5_usize);
        d.insert_temp(active_selection_id, editor_id);
    });

    // Frame with selection: toolbar is rendered
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    assert_eq!(
        context.data(|d| d.get_temp::<egui::Id>(active_selection_id)),
        Some(editor_id)
    );

    // Click outside toolbar and editor: toolbar should dismiss
    let click_outside = Event::PointerButton {
        pos: egui::pos2(800.0, 800.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    };
    frame(&context, &mut editor, vec![click_outside], Modifiers::NONE);

    assert_eq!(
        context.data(|d| d.get_temp::<egui::Id>(active_selection_id)),
        None
    );
    assert_eq!(
        context.data(|d| d.get_temp::<std::ops::Range<usize>>(selection_id)),
        None
    );

    // Test Escape dismissal as well
    context.data_mut(|d| {
        d.insert_temp(selection_id, 0..5_usize);
        d.insert_temp(active_selection_id, editor_id);
    });
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    assert_eq!(
        context.data(|d| d.get_temp::<egui::Id>(active_selection_id)),
        Some(editor_id)
    );

    let escape_key = Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&context, &mut editor, vec![escape_key], Modifiers::NONE);

    assert_eq!(
        context.data(|d| d.get_temp::<egui::Id>(active_selection_id)),
        None
    );
}

#[test]
fn marquee_drag_from_margin_selects_blocks_and_delete_removes_them() {
    let context = egui::Context::default();
    let mut editor = editor("Satır içeriği", 4);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    let start_pos = egui::pos2(24.0, 140.0);
    let drag_pos = egui::pos2(600.0, 360.0);
    frame(
        &context,
        &mut editor,
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
    frame(
        &context,
        &mut editor,
        vec![Event::PointerMoved(drag_pos)],
        Modifiers::NONE,
    );
    frame(
        &context,
        &mut editor,
        vec![Event::PointerButton {
            pos: drag_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );

    assert!(editor.selected_blocks.len() >= 2);

    let delete_event = Event::Key {
        key: Key::Delete,
        physical_key: Some(Key::Delete),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let intents = frame(&context, &mut editor, vec![delete_event], Modifiers::NONE);
    assert!(intents.iter().any(|intent| matches!(
        intent,
        Intent::Action(noteon::worker::Action::DeleteBlocks { blocks, .. }) if blocks.len() >= 2
    )));
    assert!(editor.selected_blocks.is_empty());
}

#[test]
fn ctrl_click_and_shift_click_select_blocks() {
    let context = egui::Context::default();
    let mut editor = editor("Satır içeriği", 5);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    let block1_pos = egui::pos2(300.0, 260.0);
    let block3_pos = egui::pos2(300.0, 350.0);

    // 1. Ctrl-click block 1
    frame(
        &context,
        &mut editor,
        vec![
            Event::PointerMoved(block1_pos),
            Event::PointerButton {
                pos: block1_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::COMMAND,
            },
            Event::PointerButton {
                pos: block1_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::COMMAND,
            },
        ],
        Modifiers::COMMAND,
    );
    assert_eq!(editor.selected_blocks.len(), 1);
    assert!(editor.selected_blocks.contains(&1));

    // 2. Shift-click block 3 -> range select blocks 1, 2, 3
    frame(
        &context,
        &mut editor,
        vec![
            Event::PointerMoved(block3_pos),
            Event::PointerButton {
                pos: block3_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::SHIFT,
            },
            Event::PointerButton {
                pos: block3_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::SHIFT,
            },
        ],
        Modifiers::SHIFT,
    );
    assert_eq!(editor.selected_blocks.len(), 3);
    assert!(editor.selected_blocks.contains(&1));
    assert!(editor.selected_blocks.contains(&2));
    assert!(editor.selected_blocks.contains(&3));

    // 3. Ctrl-click block 2 -> removes block 2, leaves 1 and 3
    let block2_pos = egui::pos2(300.0, 305.0);
    frame(
        &context,
        &mut editor,
        vec![
            Event::PointerMoved(block2_pos),
            Event::PointerButton {
                pos: block2_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::COMMAND,
            },
            Event::PointerButton {
                pos: block2_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::COMMAND,
            },
        ],
        Modifiers::COMMAND,
    );
    assert_eq!(editor.selected_blocks.len(), 2);
    assert!(editor.selected_blocks.contains(&1));
    assert!(!editor.selected_blocks.contains(&2));
    assert!(editor.selected_blocks.contains(&3));
}

fn frame_with_breadcrumbs(
    context: &egui::Context,
    editor: &mut Editor,
    breadcrumbs: &[BreadcrumbItem],
    events: Vec<Event>,
) -> Vec<Intent> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let mut intents = Vec::new();
    let _ = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            intents.extend(editor.show(ui, Palette::new(false), breadcrumbs));
        });
    });
    intents
}

#[test]
fn clicking_breadcrumb_navigates_to_parent_page() {
    let context = egui::Context::default();
    let mut editor = editor("İçerik", 1);
    let breadcrumbs = vec![BreadcrumbItem {
        id: 42,
        title: "Parent Page".into(),
        icon: "📄".into(),
    }];

    frame_with_breadcrumbs(&context, &mut editor, &breadcrumbs, Vec::new());

    let click_pos = egui::pos2(90.0, 22.0);
    let intents = frame_with_breadcrumbs(
        &context,
        &mut editor,
        &breadcrumbs,
        vec![
            Event::PointerMoved(click_pos),
            Event::PointerButton {
                pos: click_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: click_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );

    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::Open(42, None)))
    );
}

#[test]
fn editor_emits_undo_and_redo_intents() {
    let context = egui::Context::default();
    let mut editor = editor("Sample content", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    // Press Ctrl+Z
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::Z,
            physical_key: Some(Key::Z),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
        Modifiers::COMMAND,
    );
    assert!(intents.iter().any(|intent| matches!(intent, Intent::Undo)));

    // Press Ctrl+Shift+Z
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::Z,
            physical_key: Some(Key::Z),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        }],
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(intents.iter().any(|intent| matches!(intent, Intent::Redo)));
}

#[test]
fn clicking_anywhere_on_page_link_block_navigates_to_page() {
    let context = egui::Context::default();
    let mut editor = editor("↗ Hedef Sayfa", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    // Clicking anywhere in the middle of the block row (e.g. x = 300.0, y = 260.0)
    let click_pos = egui::pos2(300.0, 260.0);
    let intents = frame(
        &context,
        &mut editor,
        vec![
            Event::PointerMoved(click_pos),
            Event::PointerButton {
                pos: click_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: click_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        Modifiers::NONE,
    );

    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::OpenTitle(title) if title == "Hedef Sayfa"))
    );
}

#[test]
fn editing_page_link_block_allows_editing_and_enter_finishes_edit() {
    let context = egui::Context::default();
    let mut editor = editor("↗ Eski Başlık", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    let editing_id = egui::Id::new(("page-link-editing", 1_i64));
    let started_id = egui::Id::new(("page-link-editing-started", 1_i64));
    context.data_mut(|d| {
        d.insert_temp(editing_id, true);
        d.insert_temp(started_id, true);
    });

    let _ = frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    assert_eq!(context.data(|d| d.get_temp::<bool>(editing_id)), Some(true));

    let _ = frame(
        &context,
        &mut editor,
        vec![enter(Modifiers::NONE)],
        Modifiers::NONE,
    );

    assert_eq!(context.data(|d| d.get_temp::<bool>(editing_id)), None);
}

#[test]
fn add_property_button_visible_only_on_hover() {
    let context = egui::Context::default();
    let mut editor = editor("İçerik", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    let add_row_id = egui::Id::new(("add-prop-bottom-row", 1_i64, 0_u64));
    let section_hover_id = egui::Id::new(("page-properties-section-hover", 1_i64, 0_u64));

    // When mouse is not over properties section, section is not hovered
    assert_eq!(context.data(|d| d.get_temp::<bool>(section_hover_id)), Some(false));
    assert_eq!(context.data(|d| d.get_temp::<bool>(add_row_id)), Some(false));

    // Hover over the properties section area (centered around x = 350, y = 200)
    let hover_pos = egui::pos2(350.0, 200.0);
    frame(
        &context,
        &mut editor,
        vec![Event::PointerMoved(hover_pos)],
        Modifiers::NONE,
    );

    // Section hover is now active
    assert_eq!(context.data(|d| d.get_temp::<bool>(section_hover_id)), Some(true));

    // Move mouse away to the bottom of the editor
    let away_pos = egui::pos2(350.0, 700.0);
    frame(
        &context,
        &mut editor,
        vec![Event::PointerMoved(away_pos)],
        Modifiers::NONE,
    );

    // Section hover is now inactive again
    assert_eq!(context.data(|d| d.get_temp::<bool>(section_hover_id)), Some(false));
}

