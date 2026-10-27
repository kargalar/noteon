use eframe::egui::{self, Event, Key, Modifiers};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use noteon::domain::editing::split_at_selection;
use noteon::domain::model::{Block, BlockKind, BlockStub, Document, Page};
use noteon::ui::{BreadcrumbItem, Intent, editor::Editor, palette::Palette};

fn editor(content: &str, count: usize) -> Editor {
    editor_with_icon(content, count, "▤")
}

fn editor_with_icon(content: &str, count: usize, icon: &str) -> Editor {
    let mut editor = Editor::default();
    editor.bind(
        Document {
            page: Page {
                id: 1,
                parent_id: None,
                title: "Test".into(),
                icon: icon.into(),
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
fn embedded_tables_resize_the_containing_row_before_following_blocks_are_drawn() {
    use noteon::domain::model::{DatabaseData, DatabaseView, DatabaseViewSettings};
    use noteon::ui::editor::DatabaseContext;
    use std::collections::{HashMap, HashSet};

    for in_columns in [false, true] {
        let context = egui::Context::default();
        let mut editor = editor("Following block", 3);
        for position in 0..2 {
            let stub = &mut editor.document.as_mut().unwrap().outline[position];
            stub.kind = BlockKind::Database;
            stub.column_group = in_columns.then_some(1);
            stub.column_index = if in_columns { position } else { 0 };
            let block = editor.cache.blocks.get_mut(&position).unwrap();
            block.kind = BlockKind::Database;
            block.content = (100 + position).to_string();
            block.column_group = stub.column_group;
            block.column_index = stub.column_index;
        }
        editor.selected_blocks.insert(2);
        let mut databases: HashMap<_, _> = (100..102)
            .map(|id| {
                (
                    id,
                    DatabaseData {
                        page_id: id,
                        title: format!("Table {id}"),
                        icon: String::new(),
                        view: DatabaseView::Table,
                        gallery_columns: 3,
                        view_settings: DatabaseViewSettings::default(),
                        sort_key: "manual".into(),
                        sort_descending: false,
                        properties: Vec::new(),
                        rows: Vec::new(),
                    },
                )
            })
            .collect();

        // Grow, shrink, and resize live without editing either containing block.
        for height in [180.0, 600.0, 100.0, 400.0] {
            let live_height_id = egui::Id::new(("table-v-resize-live", 100_i64));
            let resizing_live = height == 400.0;
            databases.get_mut(&100).unwrap().view_settings.table_height =
                Some(if resizing_live { 100.0 } else { height });
            context.data_mut(|data| {
                data.remove::<f32>(live_height_id);
                if resizing_live {
                    data.insert_temp(live_height_id, height);
                }
            });
            databases.get_mut(&101).unwrap().view_settings.table_height = Some(80.0);
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 2000.0),
                    )),
                    ..Default::default()
                };
                let output = context.run(input, |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        editor.show_with_database_context_in_tab(
                            ui,
                            Palette::new(false),
                            &[],
                            7,
                            &mut DatabaseContext {
                                record_database_id: None,
                                databases: &mut databases,
                                loading: &mut HashSet::new(),
                                property_drafts: &mut HashMap::new(),
                                title_drafts: &mut HashMap::new(),
                                value_drafts: &mut HashMap::new(),
                                visible: &mut HashSet::new(),
                                sidebar: None,
                            },
                        );
                    });
                });
                let following_top = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape
                            && text.galley.text() == "Following block"
                        {
                            Some(text.pos.y)
                        } else {
                            None
                        }
                    })
                    .expect("following block must be drawn");
                let footer_bottom = output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape
                            && text.galley.text() == "+ New"
                        {
                            Some(text.pos.y + text.galley.size().y)
                        } else {
                            None
                        }
                    })
                    .reduce(f32::max)
                    .expect("table footers must be drawn");
                assert!(
                    following_top >= footer_bottom,
                    "following block {following_top} overlaps the table footer {footer_bottom} at height {height}, columns={in_columns}"
                );
                let selected_height = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Rect(rect) = &shape.shape
                            && rect.fill.a() == 34
                        {
                            Some(rect.rect.height())
                        } else {
                            None
                        }
                    })
                    .expect("selected table must have a highlight");
                assert!(
                    selected_height < 300.0,
                    "short table inherited its neighbor's height: {selected_height}"
                );
                assert!(editor.layout.height(0) >= height);
                assert!(editor.layout.height(0) < height + 300.0);
            }
        }
    }
}

#[test]
fn column_separator_drag_accumulates_motion_and_keeps_width_on_release() {
    for (column_count, boundary, stacked) in
        [(2, 0, false), (3, 0, false), (3, 1, false), (2, 0, true)]
    {
        let context = egui::Context::default();
        let count = column_count + usize::from(stacked);
        let mut editor = editor("", count);
        for position in 0..count {
            let column = position.min(column_count - 1);
            let stub = &mut editor.document.as_mut().unwrap().outline[position];
            stub.column_group = Some(1);
            stub.column_index = column;
            stub.column_width = 1.0 / column_count as f32;
            let block = editor.cache.blocks.get_mut(&position).unwrap();
            block.column_group = Some(1);
            block.column_index = column;
            block.content = format!("Column {position}");
            editor.selected_blocks.insert(block.id);
        }
        let render = |editor: &mut Editor, events: Vec<Event>| {
            let mut intents = Vec::new();
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 860.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        intents.extend(editor.show(ui, Palette::new(false), &[]));
                    });
                },
            );
            (output, intents)
        };
        let (initial, _) = render(&mut editor, Vec::new());
        let mut highlights: Vec<_> = initial
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Rect(rect) = &shape.shape
                    && rect.fill.a() == 34
                {
                    Some(rect.rect)
                } else {
                    None
                }
            })
            .collect();
        highlights.sort_by(|left, right| left.left().total_cmp(&right.left()));
        let separator = egui::pos2(
            (highlights[boundary].right() + highlights[boundary + 1].left()) * 0.5,
            highlights[boundary].center().y,
        );
        let column_x = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text() == format!("Column {}", boundary + 1)
                    {
                        Some(text.pos.x)
                    } else {
                        None
                    }
                })
                .expect("second column must be visible")
        };
        render(&mut editor, vec![Event::PointerMoved(separator)]);
        render(
            &mut editor,
            vec![Event::PointerButton {
                pos: separator,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        let moved = separator + egui::vec2(60.0, 0.0);
        render(&mut editor, vec![Event::PointerMoved(moved)]);
        let (paused, _) = render(&mut editor, Vec::new());
        assert!(
            column_x(&paused) > column_x(&initial) + 50.0,
            "separator did not keep the drag movement, columns={column_count}, stacked={stacked}"
        );
        let moved = separator + egui::vec2(120.0, 0.0);
        render(&mut editor, vec![Event::PointerMoved(moved)]);
        let (paused_again, _) = render(&mut editor, Vec::new());
        assert!(column_x(&paused_again) > column_x(&paused) + 50.0);
        let (_, intents) = render(
            &mut editor,
            vec![Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        let (left, right) = intents
            .iter()
            .find_map(|intent| {
                if let Intent::Action(noteon::worker::Action::ResizeColumns {
                    boundary: saved_boundary,
                    left_width,
                    right_width,
                    ..
                }) = intent
                    && *saved_boundary == boundary
                {
                    Some((*left_width, *right_width))
                } else {
                    None
                }
            })
            .expect("releasing the separator must save its widths");
        assert!(left > 1.0 / column_count as f32 + 0.1);
        assert!((left + right - 2.0 / column_count as f32).abs() < 0.001);
        let (released, _) = render(&mut editor, Vec::new());
        assert!((column_x(&released) - column_x(&paused_again)).abs() < 1.0);
    }
}

#[test]
fn columns_stack_their_blocks_independently() {
    let context = egui::Context::default();
    let mut editor = editor("", 6);
    for position in 0..5 {
        let column = usize::from(position > 0);
        let stub = &mut editor.document.as_mut().unwrap().outline[position];
        stub.column_group = Some(1);
        stub.column_index = column;
        let block = editor.cache.blocks.get_mut(&position).unwrap();
        block.column_group = Some(1);
        block.column_index = column;
        block.content = format!("Column block {position}");
    }
    editor.cache.blocks.get_mut(&5).unwrap().content = "After columns".into();
    for _ in 0..2 {
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 860.0),
                )),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    editor.show(ui, Palette::new(false), &[]);
                });
            },
        );
        let text_position = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text() == label
                    {
                        Some(text.pos)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("missing {label}"))
        };
        let left = text_position("Column block 0");
        let right = text_position("Column block 1");
        assert!(right.x > left.x + 100.0);
        assert!((right.y - left.y).abs() < 1.0);
        let mut previous = right;
        for position in 2..5 {
            let current = text_position(&format!("Column block {position}"));
            assert!((current.x - right.x).abs() < 1.0);
            assert!(current.y >= previous.y + 30.0);
            previous = current;
        }
        assert!(text_position("After columns").y > previous.y + 30.0);
        assert_eq!(editor.visual_rows, vec![vec![0, 1, 2, 3, 4], vec![5]]);
    }
}

#[test]
fn ctrl_a_selects_all_blocks_including_unloaded_blocks() {
    for modifiers in [Modifiers::CTRL, Modifiers::COMMAND] {
        let context = egui::Context::default();
        let mut editor = editor("Block", 20_000);
        frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
        assert!(editor.cache.blocks.len() < 20_000);

        frame(
            &context,
            &mut editor,
            vec![Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            modifiers,
        );

        assert_eq!(editor.selected_blocks.len(), 20_000);
        assert_eq!(editor.anchor_block, Some(1));
        assert!(editor.selected_blocks.contains(&20_000));
    }
}

#[test]
fn ctrl_a_in_focused_block_selects_blocks_and_escape_clears_selection() {
    let context = egui::Context::default();
    let mut editor = editor("Block", 3);
    editor.focus_position = Some(0);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    assert!(context.memory(|memory| memory.focused()).is_some());

    frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }],
        Modifiers::CTRL,
    );
    assert_eq!(editor.selected_blocks.len(), 3);
    assert!(context.memory(|memory| memory.focused()).is_none());

    frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    assert!(editor.selected_blocks.is_empty());
}

#[test]
fn long_text_blocks_grow_beyond_the_old_scroll_limit() {
    let context = egui::Context::default();
    for content in ["long text ".repeat(600), "line\n".repeat(80)] {
        let mut editor = editor(&content, 1);
        frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
        assert!(
            editor.layout.height(0) > 260.0,
            "height: {}",
            editor.layout.height(0)
        );
    }
}

#[test]
fn multiline_paste_creates_ordered_blocks_with_markdown_types() {
    let context = egui::Context::default();
    let mut editor = editor("", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 1_i64))));
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Paste("# Heading\r\n\r\nParagraph\r\n- Item\r\n- [x] Done\r\n```rust\r\nfirst();\r\nsecond();\r\n```".into())],
        Modifiers::NONE,
    );
    let first = &editor.cache.blocks[&0];
    assert_eq!(first.kind, BlockKind::Heading1);
    assert_eq!(first.content, "Heading");
    let blocks = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::Action(noteon::worker::Action::InsertBlocks {
                position: 1,
                blocks,
                ..
            }) => Some(blocks),
            _ => None,
        })
        .expect("paste should insert the remaining blocks in one action");
    assert_eq!(blocks.len(), 4);
    assert_eq!(
        (blocks[0].0, blocks[0].1.as_str()),
        (BlockKind::Text, "Paragraph")
    );
    assert_eq!(
        (blocks[1].0, blocks[1].1.as_str()),
        (BlockKind::Bullet, "Item")
    );
    assert_eq!(
        (blocks[2].0, blocks[2].1.as_str(), blocks[2].2),
        (BlockKind::Todo, "Done", true)
    );
    assert_eq!(
        (blocks[3].0, blocks[3].1.as_str()),
        (BlockKind::Code, "first();\nsecond();")
    );
}

#[test]
fn multiline_paste_replaces_unicode_selection_and_preserves_surrounding_styles() {
    use noteon::domain::model::{InlineTextStyle, TextStyleRange};
    let context = egui::Context::default();
    let mut editor = editor("ö🌱ç", 1);
    let style = InlineTextStyle {
        underline: true,
        ..Default::default()
    };
    editor.cache.blocks.get_mut(&0).unwrap().text_styles = vec![TextStyleRange {
        start: 0,
        end: 3,
        style: style.clone(),
    }];
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    let id = egui::Id::new(("block", 1_i64));
    context.memory_mut(|memory| memory.request_focus(id));
    let mut state = egui::text_edit::TextEditState::load(&context, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(1),
            egui::text::CCursor::new(2),
        )));
    state.store(&context, id);
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Paste("bir\niki".into())],
        Modifiers::NONE,
    );
    assert_eq!(editor.cache.blocks[&0].content, "öbir");
    assert_eq!(editor.cache.blocks[&0].text_styles[0].end, 1);
    let blocks = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::Action(noteon::worker::Action::InsertBlocks { blocks, .. }) => Some(blocks),
            _ => None,
        })
        .unwrap();
    assert_eq!(blocks[0].1, "ikiç");
    assert_eq!(
        blocks[0].3,
        vec![TextStyleRange {
            start: 3,
            end: 4,
            style
        }]
    );
}

#[test]
fn code_paste_keeps_newlines_in_the_existing_block() {
    let context = egui::Context::default();
    let mut editor = editor("", 1);
    editor.cache.blocks.get_mut(&0).unwrap().kind = BlockKind::Code;
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 1_i64))));
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Paste("first();\nsecond();".into())],
        Modifiers::NONE,
    );
    assert_eq!(editor.cache.blocks[&0].content, "first();\nsecond();");
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(_)))
    );
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
fn ctrl_click_embedded_record_selects_only_the_record() {
    use noteon::domain::model::{DatabaseData, DatabaseRow, DatabaseView, DatabaseViewSettings};
    use noteon::ui::editor::DatabaseContext;
    use std::collections::{HashMap, HashSet};

    let context = egui::Context::default();
    let mut editor = editor("100", 1);
    editor.document.as_mut().unwrap().outline[0].kind = BlockKind::Database;
    editor.cache.blocks.get_mut(&0).unwrap().kind = BlockKind::Database;
    let mut record = editor.document.as_ref().unwrap().page.clone();
    record.id = 101;
    record.parent_id = Some(100);
    record.title = "Embedded record".into();
    let mut databases = HashMap::from([(
        100,
        DatabaseData {
            page_id: 100,
            title: "Table".into(),
            icon: String::new(),
            view: DatabaseView::Table,
            gallery_columns: 3,
            view_settings: DatabaseViewSettings::default(),
            sort_key: "manual".into(),
            sort_descending: false,
            properties: Vec::new(),
            rows: vec![DatabaseRow {
                page: record,
                preview: String::new(),
                values: HashMap::new(),
            }],
        },
    )]);
    let mut render = |editor: &mut Editor, events, modifiers| {
        context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 860.0),
                )),
                events,
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    editor.show_with_database_context_in_tab(
                        ui,
                        Palette::new(false),
                        &[],
                        7,
                        &mut DatabaseContext {
                            record_database_id: None,
                            databases: &mut databases,
                            loading: &mut HashSet::new(),
                            property_drafts: &mut HashMap::new(),
                            title_drafts: &mut HashMap::new(),
                            value_drafts: &mut HashMap::new(),
                            visible: &mut HashSet::new(),
                            sidebar: None,
                        },
                    );
                });
            },
        )
    };
    let mut output = render(&mut editor, Vec::new(), Modifiers::NONE);
    for _ in 0..2 {
        output = render(&mut editor, Vec::new(), Modifiers::NONE);
    }
    let record_pos = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == "Embedded record"
            {
                Some(text.pos + text.galley.size() * 0.5)
            } else {
                None
            }
        })
        .expect("embedded record must be visible");

    for modifiers in [Modifiers::CTRL, Modifiers::COMMAND, Modifiers::SHIFT] {
        let _ = render(
            &mut editor,
            vec![
                Event::PointerMoved(record_pos),
                Event::PointerButton {
                    pos: record_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
                Event::PointerButton {
                    pos: record_pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                },
            ],
            modifiers,
        );
        let selected_rows: HashSet<i64> = context.data(|data| {
            data.get_temp(egui::Id::new(("database-selected-rows", 100_i64)))
                .unwrap_or_default()
        });
        assert_eq!(
            selected_rows.contains(&101),
            modifiers != Modifiers::COMMAND
        );
        assert!(editor.selected_blocks.is_empty());
    }
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

    // Press Cmd+Z
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

    // Press Ctrl+Z
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::Z,
            physical_key: Some(Key::Z),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }],
        Modifiers::CTRL,
    );
    assert!(intents.iter().any(|intent| matches!(intent, Intent::Undo)));

    // Press Cmd+Shift+Z
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

    // Press Ctrl+Shift+Z
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::Z,
            physical_key: Some(Key::Z),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL | Modifiers::SHIFT,
        }],
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert!(intents.iter().any(|intent| matches!(intent, Intent::Redo)));

    // Press Ctrl+Y
    let intents = frame(
        &context,
        &mut editor,
        vec![Event::Key {
            key: Key::Y,
            physical_key: Some(Key::Y),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }],
        Modifiers::CTRL,
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
    assert_eq!(
        context.data(|d| d.get_temp::<bool>(section_hover_id)),
        Some(false)
    );
    assert_eq!(
        context.data(|d| d.get_temp::<bool>(add_row_id)),
        Some(false)
    );

    // Hover over the properties section area (centered around x = 350, y = 200)
    let hover_pos = egui::pos2(350.0, 200.0);
    frame(
        &context,
        &mut editor,
        vec![Event::PointerMoved(hover_pos)],
        Modifiers::NONE,
    );

    // Section hover is now active
    assert_eq!(
        context.data(|d| d.get_temp::<bool>(section_hover_id)),
        Some(true)
    );

    // Move mouse away to the bottom of the editor
    let away_pos = egui::pos2(350.0, 700.0);
    frame(
        &context,
        &mut editor,
        vec![Event::PointerMoved(away_pos)],
        Modifiers::NONE,
    );

    // Section hover is now inactive again
    assert_eq!(
        context.data(|d| d.get_temp::<bool>(section_hover_id)),
        Some(false)
    );
}

#[test]
fn clicking_file_link_block_opens_file_explorer() {
    let context = egui::Context::default();
    let mut editor = editor(r"C:\Users\kargalar\Documents\note.txt", 1);
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
            .any(|intent| matches!(intent, Intent::OpenFileExplorer(path) if path == r"C:\Users\kargalar\Documents\note.txt"))
    );
}

#[test]
fn editing_file_link_block_allows_editing_and_enter_finishes_edit() {
    let context = egui::Context::default();
    let mut editor = editor(r"C:\Users\kargalar\Documents\note.txt", 1);
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
fn clicking_cover_without_cover_triggers_choose_cover_image() {
    let context = egui::Context::default();
    let mut editor = editor("Sample page", 1);
    frame_with_breadcrumbs(&context, &mut editor, &[], Vec::new());

    let click_pos = egui::pos2(200.0, 80.0);
    let intents = frame_with_breadcrumbs(
        &context,
        &mut editor,
        &[],
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
        intents.iter().any(|intent| matches!(
            intent,
            Intent::Action(noteon::worker::Action::ChooseCoverImage(1))
        )),
        "Expected Intent::Action(Action::ChooseCoverImage(1)), got {} intents",
        intents.len()
    );
}

#[test]
fn add_icon_menu_open_state_persists_when_open() {
    let context = egui::Context::default();
    let mut editor = editor_with_icon("Sample page", 1, "");

    let icon_menu_open_id = egui::Id::new(("add-icon-menu-open", 1_i64, None::<u64>));
    context.data_mut(|d| {
        d.insert_temp(icon_menu_open_id, true);
    });

    // Pointer is at (0, 0), completely outside the title hover area
    let _ = frame_with_breadcrumbs(
        &context,
        &mut editor,
        &[],
        vec![Event::PointerMoved(egui::pos2(0.0, 0.0))],
    );
}

fn backspace() -> Event {
    Event::Key {
        key: Key::Backspace,
        physical_key: Some(Key::Backspace),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn cut() -> Event {
    Event::Key {
        key: Key::X,
        physical_key: Some(Key::X),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    }
}

#[test]
fn empty_block_backspace_removes_block() {
    let context = egui::Context::default();
    let mut editor = editor("", 2);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 2_i64))));
    let intents = frame(&context, &mut editor, vec![backspace()], Modifiers::NONE);
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(noteon::worker::Action::DeleteBlock { block, .. }) if *block == 2)),
        "Expected DeleteBlock intent for empty block on backspace"
    );
}

#[test]
fn focused_block_ctrl_x_without_selection_cuts_entire_block() {
    let context = egui::Context::default();
    let mut editor = editor("Kesilecek blok metni", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 1_i64))));
    let intents = frame(&context, &mut editor, vec![cut()], Modifiers::CTRL);
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(noteon::worker::Action::DeleteBlock { block, .. }) if *block == 1)),
        "Expected DeleteBlock intent on Ctrl+X without selection"
    );
}

#[test]
fn title_enter_inserts_an_empty_text_block_at_the_top() {
    for count in [0, 2] {
        let context = egui::Context::default();
        let mut editor = editor("Existing content", count);
        frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
        let title_id = egui::Id::new(("title", 1_i64));
        context.memory_mut(|memory| memory.request_focus(title_id));

        let intents = frame(
            &context,
            &mut editor,
            vec![enter(Modifiers::NONE)],
            Modifiers::NONE,
        );
        let insertions: Vec<_> = intents
            .iter()
            .filter_map(|intent| match intent {
                Intent::Action(noteon::worker::Action::InsertBlock {
                    page,
                    position,
                    kind,
                    content,
                    text_styles,
                    ..
                }) => Some((*page, *position, *kind, content, text_styles)),
                _ => None,
            })
            .collect();
        assert_eq!(insertions.len(), 1);
        let (page, position, kind, content, text_styles) = insertions[0];
        assert_eq!(page, 1);
        assert_eq!(position, 0);
        assert_eq!(kind, BlockKind::Text);
        assert!(content.is_empty());
        assert!(text_styles.is_empty());
        assert_eq!(editor.document.as_ref().unwrap().page.title, "Test");
        assert!(!context.memory(|memory| memory.has_focus(title_id)));
    }
}

#[test]
fn enter_creates_new_block_below() {
    let context = egui::Context::default();
    let mut editor = editor("İlk satır", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 1_i64))));
    let intents = frame(
        &context,
        &mut editor,
        vec![enter(Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(noteon::worker::Action::InsertBlock { position, .. }) if *position == 1)),
        "Expected InsertBlock at position 1 on Enter"
    );
}

#[test]
fn enter_and_multiline_paste_keep_the_originating_column() {
    for event in [enter(Modifiers::NONE), Event::Paste("First\nSecond".into())] {
        let context = egui::Context::default();
        let mut editor = editor("Text", 2);
        for position in 0..2 {
            let stub = &mut editor.document.as_mut().unwrap().outline[position];
            stub.column_group = Some(1);
            stub.column_index = position;
            let block = editor.cache.blocks.get_mut(&position).unwrap();
            block.column_group = Some(1);
            block.column_index = position;
        }
        frame(&context, &mut editor, Vec::new(), Modifiers::NONE);
        context.memory_mut(|memory| memory.request_focus(egui::Id::new(("block", 2_i64))));
        let intents = frame(&context, &mut editor, vec![event], Modifiers::NONE);
        assert!(intents.iter().any(|intent| matches!(
            intent,
            Intent::Action(noteon::worker::Action::InsertBlock {
                column_anchor: Some(2),
                ..
            }) | Intent::Action(noteon::worker::Action::InsertBlocks {
                column_anchor: Some(2),
                ..
            })
        )));
    }
}

#[test]
fn selected_block_enter_in_viewport() {
    let context = egui::Context::default();
    let mut editor = editor("Seçili blok", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    editor.selected_blocks.insert(1);
    let enter_intents = frame(
        &context,
        &mut editor,
        vec![enter(Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(
        enter_intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(noteon::worker::Action::InsertBlock { position, .. }) if *position == 1)),
        "Expected InsertBlock when block is selected and Enter is pressed"
    );
}

#[test]
fn selected_block_ctrl_x_in_viewport() {
    let context = egui::Context::default();
    let mut editor = editor("Seçili blok", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    editor.selected_blocks.insert(1);
    let cut_intents = frame(&context, &mut editor, vec![cut()], Modifiers::CTRL);
    assert!(
        cut_intents
            .iter()
            .any(|intent| matches!(intent, Intent::Action(noteon::worker::Action::DeleteBlocks { blocks, .. }) if blocks == &[1])),
        "Expected DeleteBlocks when block is selected and Ctrl+X is pressed"
    );
}

#[test]
fn focused_block_ctrl_x_with_selection_cuts_only_selected_text() {
    let context = egui::Context::default();
    let mut editor = editor("Hello world", 1);
    frame(&context, &mut editor, Vec::new(), Modifiers::NONE);

    let editor_id = egui::Id::new(("block", 1_i64));
    context.memory_mut(|memory| memory.request_focus(editor_id));

    let mut state = egui::text_edit::TextEditState::default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(5),
        )));
    state.store(&context, editor_id);

    let intents = frame(&context, &mut editor, vec![cut()], Modifiers::CTRL);
    assert!(
        !intents.iter().any(|intent| matches!(
            intent,
            Intent::Action(noteon::worker::Action::DeleteBlock { .. })
        )),
        "Block should not be deleted when cutting selected text"
    );
    assert_eq!(editor.cache.blocks[&0].content, " world");
}

#[test]
fn test_find_char_ranges_and_case_insensitivity() {
    use noteon::ui::editor::find::find_char_ranges;

    let ranges = find_char_ranges("Noteon ile arama yaparken noteon bulunsun", "noteon");
    assert_eq!(ranges, vec![0..6, 26..32]);

    let ranges_tr = find_char_ranges("İstanbul ve istanbullular", "istanbul");
    assert_eq!(ranges_tr, vec![0..8, 12..20]);
}
