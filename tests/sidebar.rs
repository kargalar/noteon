use eframe::egui::{self, Event, Key, Modifiers};
use noteon::domain::model::Page;
use noteon::ui::{Intent, palette::Palette, sidebar::Sidebar};

fn test_pages() -> Vec<Page> {
    (1..=5)
        .map(|id| Page {
            id,
            parent_id: None,
            title: format!("Page {id}"),
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
        })
        .collect()
}

fn frame_sidebar(
    context: &egui::Context,
    sidebar: &mut Sidebar,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> Vec<Intent> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(260.0, 800.0),
        )),
        events,
        modifiers,
        ..Default::default()
    };
    let mut intents = Vec::new();
    let _ = context.run(input, |context| {
        egui::SidePanel::left("sidebar").show(context, |ui| {
            intents.extend(sidebar.show(ui, None, false, Palette::new(false), true));
        });
    });
    intents
}

#[test]
fn sidebar_select_single_sets_page_and_anchor() {
    let mut sidebar = Sidebar::default();
    sidebar.set_pages(test_pages());

    sidebar.select_single(3);
    assert_eq!(sidebar.selected_pages.len(), 1);
    assert!(sidebar.selected_pages.contains(&3));
    assert_eq!(sidebar.anchor_page, Some(3));
}

#[test]
fn sidebar_ctrl_click_and_shift_click_and_delete() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut sidebar = Sidebar::default();
    sidebar.set_pages(test_pages());

    // Initial render to lay out
    frame_sidebar(&context, &mut sidebar, Vec::new(), Modifiers::NONE);

    // Header is ~50px, Row 0 is WORKSPACE header (~33px), so Row 1 (Page 1) is at y ≈ 95-100px.
    // Each row is 33px high.
    let page1_pos = egui::pos2(80.0, 115.0);
    let page2_pos = egui::pos2(80.0, 150.0);
    let page3_pos = egui::pos2(80.0, 195.0);

    // 1. Regular click on Page 1 -> selects Page 1
    frame_sidebar(
        &context,
        &mut sidebar,
        vec![
            Event::PointerMoved(page1_pos),
            Event::PointerButton {
                pos: page1_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: page1_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        Modifiers::NONE,
    );
    assert_eq!(sidebar.selected_pages.len(), 1);
    assert!(sidebar.selected_pages.contains(&1));
    assert_eq!(sidebar.anchor_page, Some(1));

    // 2. Shift-click Page 3 -> selects Page 1, 2, 3
    frame_sidebar(
        &context,
        &mut sidebar,
        vec![
            Event::PointerMoved(page3_pos),
            Event::PointerButton {
                pos: page3_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::SHIFT,
            },
            Event::PointerButton {
                pos: page3_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::SHIFT,
            },
        ],
        Modifiers::SHIFT,
    );
    assert_eq!(sidebar.selected_pages.len(), 3);
    assert!(sidebar.selected_pages.contains(&1));
    assert!(sidebar.selected_pages.contains(&2));
    assert!(sidebar.selected_pages.contains(&3));

    // 3. Ctrl-click Page 2 -> removes Page 2 from selection
    frame_sidebar(
        &context,
        &mut sidebar,
        vec![
            Event::PointerMoved(page2_pos),
            Event::PointerButton {
                pos: page2_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::COMMAND,
            },
            Event::PointerButton {
                pos: page2_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::COMMAND,
            },
        ],
        Modifiers::COMMAND,
    );
    assert_eq!(sidebar.selected_pages.len(), 2);
    assert!(sidebar.selected_pages.contains(&1));
    assert!(!sidebar.selected_pages.contains(&2));
    assert!(sidebar.selected_pages.contains(&3));

    // 4. Delete key trashes all selected pages
    let intents = frame_sidebar(
        &context,
        &mut sidebar,
        vec![Event::Key {
            key: Key::Delete,
            physical_key: Some(Key::Delete),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    assert!(sidebar.selected_pages.is_empty());
    assert_eq!(sidebar.anchor_page, None);
    let has_trash_pages = intents
        .iter()
        .any(|i| matches!(i, Intent::Action(noteon::worker::Action::TrashPages(_))));
    let trashed: Vec<_> = intents
        .into_iter()
        .flat_map(|intent| match intent {
            Intent::Action(noteon::worker::Action::Trash(id)) => vec![id],
            Intent::Action(noteon::worker::Action::TrashPages(ids)) => ids,
            _ => Vec::new(),
        })
        .collect();
    assert!(has_trash_pages);
    assert!(trashed.contains(&1));
    assert!(trashed.contains(&3));
}

#[test]
fn sidebar_escape_clears_selection() {
    let context = egui::Context::default();
    noteon::ui::theme::configure(&context);
    let mut sidebar = Sidebar::default();
    sidebar.set_pages(test_pages());

    frame_sidebar(&context, &mut sidebar, Vec::new(), Modifiers::NONE);
    sidebar.selected_pages.insert(1);
    sidebar.selected_pages.insert(2);
    sidebar.anchor_page = Some(1);

    frame_sidebar(
        &context,
        &mut sidebar,
        vec![
            Event::PointerMoved(egui::pos2(50.0, 50.0)),
            Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        Modifiers::NONE,
    );
    assert!(sidebar.selected_pages.is_empty());
    assert_eq!(sidebar.anchor_page, None);
}

#[test]
fn sidebar_ancestors_returns_hierarchy_in_root_to_leaf_order() {
    let mut sidebar = Sidebar::default();
    let mut pages = test_pages();
    // 1 -> 2 -> 3
    pages[1].parent_id = Some(1);
    pages[2].parent_id = Some(2);
    sidebar.set_pages(pages);

    let ancestors = sidebar.ancestors(3);
    assert_eq!(ancestors.len(), 2);
    assert_eq!(ancestors[0].id, 1);
    assert_eq!(ancestors[0].title, "Page 1");
    assert_eq!(ancestors[1].id, 2);
    assert_eq!(ancestors[1].title, "Page 2");

    let ancestors_root = sidebar.ancestors(1);
    assert!(ancestors_root.is_empty());
}
