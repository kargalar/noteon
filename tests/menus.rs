use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect};
use noteon::ui::menus::{PersistentContextMenu, PersistentMenuUi};

struct MenuHarness {
    root_button: Rect,
    submenu_button: Rect,
    sibling_button: Rect,
    action_button: Rect,
    native_button: Rect,
    native_option: Rect,
    native_selected: bool,
    root_open: bool,
    submenu_open: bool,
    sibling_open: bool,
    selected: bool,
    context_menu: bool,
    popup_menu: bool,
}

impl Default for MenuHarness {
    fn default() -> Self {
        Self {
            root_button: Rect::NOTHING,
            submenu_button: Rect::NOTHING,
            sibling_button: Rect::NOTHING,
            action_button: Rect::NOTHING,
            native_button: Rect::NOTHING,
            native_option: Rect::NOTHING,
            native_selected: false,
            root_open: false,
            submenu_open: false,
            sibling_open: false,
            selected: false,
            context_menu: false,
            popup_menu: false,
        }
    }
}

impl MenuHarness {
    fn frame(&mut self, context: &egui::Context, events: Vec<Event>) {
        self.root_open = false;
        self.submenu_open = false;
        self.sibling_open = false;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            events,
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                if self.context_menu {
                    let response = ui.button("Context menu");
                    self.root_button = response.rect;
                    self.root_open = response
                        .persistent_context_menu(|ui| self.contents(ui))
                        .is_some();
                } else if self.popup_menu {
                    let response = ui.button("Popup menu");
                    self.root_button = response.rect;
                    let popup_id = response.id.with("popup");
                    if response.clicked() {
                        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
                    }
                    self.root_open =
                        noteon::ui::menus::popup_below_widget(ui, popup_id, &response, |ui| {
                            self.contents(ui)
                        })
                        .is_some();
                } else {
                    let response = ui.persistent_menu_button("Menu", |ui| self.contents(ui));
                    self.root_button = response.response.rect;
                    self.root_open = response.inner.is_some();
                }
            });
        });
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        let submenu = ui.persistent_menu_button("Submenu", |ui| {
            ui.label("Editable panel");
            let action = ui.button("Apply");
            self.action_button = action.rect;
            if action.clicked() {
                self.selected = true;
                ui.close_menu();
            }
        });
        self.submenu_button = submenu.response.rect;
        self.submenu_open = submenu.inner.is_some();
        let sibling = ui.persistent_menu_button("Sibling", |ui| {
            ui.label("Another panel");
        });
        self.sibling_button = sibling.response.rect;
        self.sibling_open = sibling.inner.is_some();
        ui.label("Panel contents");
        let native = egui::ComboBox::from_id_salt("native-combo")
            .selected_text("Choose an option")
            .show_ui(ui, |ui| {
                self.native_option = ui
                    .selectable_value(&mut self.native_selected, true, "Native option")
                    .rect;
            });
        self.native_button = native.response.rect;
    }

    fn move_to(&mut self, context: &egui::Context, position: Pos2) {
        self.frame(context, vec![Event::PointerMoved(position)]);
        self.frame(context, Vec::new());
    }

    fn click(&mut self, context: &egui::Context, position: Pos2, button: PointerButton) {
        self.move_to(context, position);
        for pressed in [true, false] {
            self.frame(
                context,
                vec![Event::PointerButton {
                    pos: position,
                    button,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        self.frame(context, Vec::new());
    }

    fn open(&mut self, context: &egui::Context) {
        self.frame(context, Vec::new());
        self.frame(context, Vec::new());
        let button = if self.context_menu {
            PointerButton::Secondary
        } else {
            PointerButton::Primary
        };
        self.click(context, self.root_button.center(), button);
        assert!(self.root_open);
        self.move_to(context, self.submenu_button.center());
        assert!(!self.submenu_open);
        self.click(
            context,
            self.submenu_button.center(),
            PointerButton::Primary,
        );
        assert!(self.submenu_open);
    }
}

#[test]
fn menus_stay_open_when_pointer_leaves_or_crosses_a_sibling() {
    for (context_menu, popup_menu) in [(false, false), (true, false), (false, true)] {
        let context = egui::Context::default();
        let mut harness = MenuHarness {
            context_menu,
            popup_menu,
            ..Default::default()
        };
        harness.open(&context);
        harness.move_to(&context, Pos2::new(800.0, 600.0));
        assert!(harness.root_open && harness.submenu_open);
        harness.move_to(&context, harness.sibling_button.center());
        assert!(harness.submenu_open);
        assert!(!harness.sibling_open);
        harness.move_to(&context, harness.action_button.center());
        assert!(harness.root_open && harness.submenu_open);
        harness.click(&context, Pos2::new(800.0, 600.0), PointerButton::Secondary);
        assert!(!harness.root_open);
    }
}

#[test]
fn submenu_actions_receive_clicks_and_close_the_menu_hierarchy() {
    for (context_menu, popup_menu) in [(false, false), (true, false), (false, true)] {
        let context = egui::Context::default();
        let mut harness = MenuHarness {
            context_menu,
            popup_menu,
            ..Default::default()
        };
        harness.open(&context);
        harness.click(
            &context,
            harness.action_button.center(),
            PointerButton::Primary,
        );
        assert!(harness.selected);
        assert!(!harness.root_open);
    }
}

#[test]
fn clicking_a_sibling_changes_the_open_submenu() {
    let context = egui::Context::default();
    let mut harness = MenuHarness::default();
    harness.open(&context);
    harness.click(
        &context,
        harness.sibling_button.center(),
        PointerButton::Primary,
    );
    assert!(harness.root_open && harness.sibling_open);
    assert!(!harness.submenu_open);
}

#[test]
fn native_dropdowns_inside_menus_remain_interactive() {
    let context = egui::Context::default();
    let mut harness = MenuHarness::default();
    harness.open(&context);
    harness.click(
        &context,
        harness.native_button.center(),
        PointerButton::Primary,
    );
    assert!(harness.root_open);
    harness.click(
        &context,
        harness.native_option.center(),
        PointerButton::Primary,
    );
    assert!(harness.native_selected);
    assert!(harness.root_open);
}
