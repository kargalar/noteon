use eframe::egui::{self, Ui};

#[derive(Clone)]
struct MultiClickSelection {
    last_click_time: f64,
    last_click_position: Option<egui::Pos2>,
    click_count: u8,
    press_position: Option<egui::Pos2>,
    active: Option<ActiveSelection>,
}

impl Default for MultiClickSelection {
    fn default() -> Self {
        Self {
            last_click_time: f64::NEG_INFINITY,
            last_click_position: None,
            click_count: 0,
            press_position: None,
            active: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct ActiveSelection {
    mode: SelectionMode,
    unit: std::ops::Range<usize>,
}

#[derive(Clone, Copy)]
pub(super) enum SelectionMode {
    Word,
    Line,
}

pub(super) fn update_multi_click_selection(ui: &Ui, output: &mut egui::text_edit::TextEditOutput) {
    let response = &output.response;
    let selection_id = response.id.with("multi-click-selection");
    let mut selection = ui
        .ctx()
        .data(|data| data.get_temp::<MultiClickSelection>(selection_id))
        .unwrap_or_default();
    let (time, pressed, down, released, position) = ui.input(|input| {
        (
            input.time,
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_down(egui::PointerButton::Primary),
            input.pointer.button_released(egui::PointerButton::Primary),
            input.pointer.interact_pos(),
        )
    });
    let (max_click_delay, max_click_distance) = ui.ctx().options(|options| {
        (
            options.input_options.max_double_click_delay,
            options.input_options.max_click_dist,
        )
    });

    if pressed && response.rect.contains(position.unwrap_or_default()) {
        let position = position.unwrap_or_default();
        let is_following_click = selection.click_count > 0
            && time - selection.last_click_time <= max_click_delay
            && selection
                .last_click_position
                .is_some_and(|last| last.distance(position) <= max_click_distance);
        selection.click_count = if is_following_click {
            selection.click_count.saturating_add(1).min(3)
        } else {
            1
        };
        selection.press_position = Some(position);
        selection.active = match selection.click_count {
            2 => Some(ActiveSelection {
                mode: SelectionMode::Word,
                unit: selection_unit_range(
                    output.galley.text(),
                    output
                        .galley
                        .cursor_from_pos(position - output.galley_pos)
                        .ccursor
                        .index,
                    SelectionMode::Word,
                ),
            }),
            3 => Some(ActiveSelection {
                mode: SelectionMode::Line,
                unit: selection_unit_range(
                    output.galley.text(),
                    output
                        .galley
                        .cursor_from_pos(position - output.galley_pos)
                        .ccursor
                        .index,
                    SelectionMode::Line,
                ),
            }),
            _ => {
                selection.active = None;
                None
            }
        };
    }

    if down
        && let (Some(active), Some(position)) = (selection.active.clone(), position)
        && (response.hovered() || ui.ctx().is_being_dragged(response.id))
    {
        let text = output.galley.text();
        let cursor = output
            .galley
            .cursor_from_pos(position - output.galley_pos)
            .ccursor
            .index;
        let range = drag_selection_range(text, cursor, active);
        output
            .state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(range.start),
                egui::text::CCursor::new(range.end),
            )));
        output.state.clone().store(ui.ctx(), response.id);
    }

    if released {
        if response.clicked()
            && let Some(position) = selection.press_position
        {
            selection.last_click_time = time;
            selection.last_click_position = Some(position);
        } else if selection.active.is_some() {
            selection.click_count = 0;
            selection.last_click_time = f64::NEG_INFINITY;
            selection.last_click_position = None;
        }
        selection.active = None;
        selection.press_position = None;
    }

    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selection));
}

pub(super) fn selection_unit_range(
    text: &str,
    cursor: usize,
    mode: SelectionMode,
) -> std::ops::Range<usize> {
    let character_count = text.chars().count();
    let cursor = cursor.min(character_count);
    if character_count == 0 {
        return 0..0;
    }

    match mode {
        SelectionMode::Line => {
            let start = text
                .chars()
                .take(cursor)
                .enumerate()
                .filter(|(_, character)| *character == '\n')
                .map(|(index, _)| index + 1)
                .last()
                .unwrap_or(0);
            let end = text
                .chars()
                .skip(cursor)
                .position(|character| character == '\n')
                .map_or(character_count, |offset| cursor + offset + 1);
            start..end
        }
        SelectionMode::Word => {
            let character_at = |index| text.chars().nth(index);
            let pivot = if cursor < character_count
                && character_at(cursor).is_some_and(is_word_character)
            {
                cursor
            } else if cursor > 0 && character_at(cursor - 1).is_some_and(is_word_character) {
                cursor - 1
            } else {
                cursor.min(character_count - 1)
            };
            let character_class = word_class(character_at(pivot).unwrap_or_default());
            let start = text
                .chars()
                .take(pivot)
                .enumerate()
                .filter(|(_, character)| word_class(*character) != character_class)
                .map(|(index, _)| index + 1)
                .last()
                .unwrap_or(0);
            let end = text
                .chars()
                .skip(pivot + 1)
                .position(|character| word_class(character) != character_class)
                .map_or(character_count, |offset| pivot + 1 + offset);
            start..end
        }
    }
}

pub(super) fn drag_selection_range(
    text: &str,
    cursor: usize,
    active: ActiveSelection,
) -> std::ops::Range<usize> {
    if cursor < active.unit.start {
        let current = selection_unit_range(text, cursor, active.mode);
        current.start..active.unit.start
    } else if cursor >= active.unit.end {
        let current = selection_unit_range(text, cursor, active.mode);
        active.unit.start..current.end
    } else {
        active.unit
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn word_class(character: char) -> u8 {
    if is_word_character(character) {
        0
    } else if character.is_whitespace() {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveSelection, SelectionMode, drag_selection_range, selection_unit_range};

    #[test]
    fn word_selection_uses_unicode_character_boundaries() {
        let text = "Hello, dünya!";
        assert_eq!(selection_unit_range(text, 9, SelectionMode::Word), 7..12);
    }

    #[test]
    fn line_selection_includes_the_line_break() {
        assert_eq!(
            selection_unit_range("first\nsecond", 8, SelectionMode::Line),
            6..12
        );
    }

    #[test]
    fn dragging_a_word_selection_expands_by_whole_words() {
        let active = ActiveSelection {
            mode: SelectionMode::Word,
            unit: 6..10,
        };
        assert_eq!(
            drag_selection_range("alpha beta gamma", 13, active.clone()),
            6..16
        );
        assert_eq!(drag_selection_range("alpha beta gamma", 1, active), 0..6);
    }
}
