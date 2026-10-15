use eframe::egui::{self, Event, Key, Modifiers};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use noteon::domain::editing::split_at_selection;
use noteon::domain::model::{Block, BlockKind, BlockStub, Document, Page};
use noteon::ui::{Intent, editor::Editor, palette::Palette};

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
            intents.extend(editor.show(ui, Palette::new(false), None));
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
