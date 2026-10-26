use eframe::egui::{Id, Key, LayerId, Modifiers, Rect, Ui};

#[derive(Clone)]
struct SelectionScope {
    id: Id,
    rect: Rect,
    layer: LayerId,
    pass: u64,
    nested: bool,
}

pub(super) fn is_active(ui: &Ui, id: Id) -> bool {
    ui.ctx()
        .data(|data| data.get_temp::<Id>(Id::new("select-all-scopes").with("active")))
        == Some(id)
}

/// Route selection to its active scope, preserving text selection inside databases.
pub(super) fn select_all(ui: &Ui, id: Id, nested: bool) -> bool {
    let registry_id = Id::new("select-all-scopes");
    let active_id = registry_id.with("active");
    let pass = ui.ctx().cumulative_pass_nr();
    let rect = ui.max_rect().intersect(ui.clip_rect());
    let focused = ui.memory(|memory| memory.focused());
    let focused_response = focused.and_then(|id| ui.ctx().read_response(id));
    let editing_text = focused
        .is_some_and(|id| eframe::egui::text_edit::TextEditState::load(ui.ctx(), id).is_some());
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let pressed = ui.input(|input| input.pointer.any_pressed());

    let (target, editing_database_text) = ui.ctx().data_mut(|data| {
        let mut scopes = data
            .get_temp::<Vec<SelectionScope>>(registry_id)
            .unwrap_or_default();
        scopes.retain(|scope| scope.pass.saturating_add(1) >= pass && scope.id != id);
        scopes.push(SelectionScope {
            id,
            rect,
            layer: ui.layer_id(),
            pass,
            nested,
        });
        let under_position = scopes
            .iter()
            .filter(|scope| {
                if !pressed && let Some(response) = &focused_response {
                    scope.layer == response.layer_id && scope.rect.intersects(response.rect)
                } else {
                    pointer.is_some_and(|position| scope.rect.contains(position))
                        && scope.layer == ui.layer_id()
                }
            })
            .max_by(|a, b| {
                a.nested
                    .cmp(&b.nested)
                    .then_with(|| b.rect.area().total_cmp(&a.rect.area()))
            })
            .map(|scope| scope.id);
        let active = data
            .get_temp::<Id>(active_id)
            .filter(|active| scopes.iter().any(|scope| scope.id == *active));
        let target = if focused_response.is_some() || pressed {
            under_position
        } else {
            active.or(under_position).or(Some(id))
        };
        if let Some(target) = target {
            data.insert_temp(active_id, target);
        }
        let editing_database_text = editing_text
            && focused_response.as_ref().is_some_and(|response| {
                scopes.iter().any(|scope| {
                    scope.nested
                        && scope.layer == response.layer_id
                        && scope.rect.intersects(response.rect)
                })
            });
        data.insert_temp(registry_id, scopes);
        (target, editing_database_text)
    });
    if target != Some(id) || editing_database_text {
        return false;
    }
    let selected = ui.input_mut(|input| {
        input.consume_key(Modifiers::COMMAND, Key::A) || input.consume_key(Modifiers::CTRL, Key::A)
    });
    if selected && let Some(focused) = focused {
        ui.memory_mut(|memory| memory.surrender_focus(focused));
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::select_all;
    use eframe::egui::{self, Event, Id, Key, Modifiers, Pos2, Rect, UiBuilder, vec2};

    #[test]
    fn focused_database_text_keeps_select_all_before_the_parent_page_can_consume_it() {
        let context = egui::Context::default();
        let field_id = Id::new("database-property-field");
        let mut text = "Property text 🐦".to_owned();
        let mut frame = |events| {
            let mut selected = Vec::new();
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        let page_id = Id::new("parent-page");
                        if select_all(ui, page_id, false) {
                            selected.push(page_id);
                        }
                        ui.scope_builder(
                            UiBuilder::new().max_rect(Rect::from_min_size(
                                egui::pos2(100.0, 100.0),
                                vec2(400.0, 200.0),
                            )),
                            |ui| {
                                let database_id = Id::new("embedded-table");
                                if select_all(ui, database_id, true) {
                                    selected.push(database_id);
                                }
                                ui.add(egui::TextEdit::singleline(&mut text).id(field_id));
                            },
                        );
                    });
                },
            );
            selected
        };
        frame(Vec::new());
        context.memory_mut(|memory| memory.request_focus(field_id));
        frame(Vec::new());
        let selected = frame(vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        }]);
        assert!(selected.is_empty());
        assert_eq!(context.memory(|memory| memory.focused()), Some(field_id));
        let range = egui::text_edit::TextEditState::load(&context, field_id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(range.primary.index.min(range.secondary.index), 0);
        assert_eq!(
            range.primary.index.max(range.secondary.index),
            "Property text 🐦".chars().count()
        );
    }

    #[test]
    fn embedded_scope_keeps_select_all_after_pointer_moves_and_releases_it_on_page_click() {
        let context = egui::Context::default();
        let page_id = Id::new("test-page");
        let database_id = Id::new("test-embedded-database");
        let frame = |events| {
            let mut selected = Vec::new();
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        if select_all(ui, page_id, false) {
                            selected.push(page_id);
                        }
                        ui.scope_builder(
                            UiBuilder::new().max_rect(Rect::from_min_size(
                                egui::pos2(100.0, 100.0),
                                vec2(400.0, 200.0),
                            )),
                            |ui| {
                                if select_all(ui, database_id, true) {
                                    selected.push(database_id);
                                }
                            },
                        );
                    });
                },
            );
            selected
        };
        let click = |position| {
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
        let select = || Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        };

        frame(Vec::new());
        frame(click(egui::pos2(150.0, 150.0)));
        assert_eq!(frame(vec![select()]), vec![database_id]);
        assert_eq!(
            frame(vec![
                Event::PointerMoved(egui::pos2(700.0, 500.0)),
                select()
            ]),
            vec![database_id]
        );
        frame(click(egui::pos2(700.0, 500.0)));
        assert_eq!(frame(vec![select()]), vec![page_id]);
    }
}
