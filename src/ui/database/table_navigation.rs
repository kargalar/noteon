use eframe::egui::{self, Response, Ui};

use crate::domain::model::{DatabaseData, DatabaseProperty, DatabaseRow, PageId};
use crate::ui::Intent;
use crate::worker::Action;

use super::Drafts;

#[derive(Clone, Copy, PartialEq, Eq)]
struct CellTarget {
    record: PageId,
    property: Option<i64>,
}

#[derive(Clone)]
struct Cell {
    target: CellTarget,
    widget: egui::Id,
}

pub(super) struct TableNavigation {
    database: PageId,
    cells: Vec<Cell>,
    pending: Option<CellTarget>,
    create_record: bool,
}

impl TableNavigation {
    pub(super) fn begin(
        ui: &Ui,
        data: &DatabaseData,
        rows: &[DatabaseRow],
        properties: &[DatabaseProperty],
        busy: bool,
        drafts: &mut Drafts<'_>,
    ) -> Self {
        let mut navigation = Self {
            database: data.page_id,
            cells: Vec::new(),
            pending: None,
            create_record: false,
        };
        if busy {
            return navigation;
        }
        let cells = ui
            .ctx()
            .data(|state| state.get_temp::<Vec<Cell>>(navigation.registry_id()))
            .unwrap_or_default();
        let Some(current) = cells
            .iter()
            .find(|cell| ui.memory(|memory| memory.has_focus(cell.widget)))
        else {
            return navigation;
        };
        let enter =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let backwards =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        let forwards = !backwards
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if !enter && !backwards && !forwards {
            return navigation;
        }
        // A submitted number must be valid before leaving its cell or creating a record.
        if let Some(property) = current.target.property
            && properties
                .iter()
                .any(|item| item.id == property && item.kind == "number")
            && let Some(value) = drafts.values.get_mut(&(current.target.record, property))
            && !value.trim().is_empty()
        {
            let Some(result) = crate::domain::number_expression::evaluate(value) else {
                ui.memory_mut(|memory| memory.request_focus(current.widget));
                return navigation;
            };
            *value = result;
        }
        if enter {
            navigation.create_record = true;
            ui.ctx().data_mut(|state| {
                state.remove::<String>(egui::Id::new(("database-table-find-query", data.page_id)));
            });
        } else {
            let columns: Vec<_> = std::iter::once(None)
                .chain(
                    properties
                        .iter()
                        .filter(|property| is_editable(property))
                        .map(|property| Some(property.id)),
                )
                .collect();
            let Some(row) = rows
                .iter()
                .position(|row| row.page.id == current.target.record)
            else {
                return navigation;
            };
            let Some(column) = columns
                .iter()
                .position(|column| *column == current.target.property)
            else {
                return navigation;
            };
            let index = row * columns.len() + column;
            let count = rows.len() * columns.len();
            let next = if backwards {
                (index + count - 1) % count
            } else {
                (index + 1) % count
            };
            navigation.pending = Some(CellTarget {
                record: rows[next / columns.len()].page.id,
                property: columns[next % columns.len()],
            });
        }
        ui.memory_mut(|memory| memory.surrender_focus(current.widget));
        navigation
    }

    pub(super) fn pending_record(&self) -> Option<PageId> {
        self.pending.map(|target| target.record)
    }

    pub(super) fn register(
        &mut self,
        ui: &Ui,
        record: PageId,
        property: Option<i64>,
        response: &Response,
    ) {
        let target = CellTarget { record, property };
        self.cells.push(Cell {
            target,
            widget: response.id,
        });
        if self.pending == Some(target) {
            response.request_focus();
            ui.scroll_to_rect(response.rect, None);
            self.pending = None;
        }
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
    }

    pub(super) fn finish(self, ui: &Ui, intents: &mut Vec<Intent>) {
        let registry_id = self.registry_id();
        ui.ctx()
            .data_mut(|state| state.insert_temp(registry_id, self.cells));
        if self.create_record {
            intents.push(Intent::Action(Action::CreateDatabaseRecordInline(
                self.database,
            )));
        }
    }

    fn registry_id(&self) -> egui::Id {
        egui::Id::new(("database-table-cells", self.database))
    }
}

pub(super) fn is_editable(property: &DatabaseProperty) -> bool {
    !matches!(property.kind.as_str(), "created_time" | "last_edited_time")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{storage::Repository, ui::palette::Palette};

    struct Table {
        context: egui::Context,
        data: DatabaseData,
        properties: HashMap<i64, (String, String)>,
        titles: HashMap<i64, String>,
        values: HashMap<(i64, i64), String>,
    }

    impl Table {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut repository = Repository::open(&directory.path().join("table.sqlite")).unwrap();
            let database = repository.create_database(None).unwrap();
            for kind in ["text", "number", "checkbox", "date", "created_time"] {
                repository
                    .add_database_property(database, kind, kind, &[])
                    .unwrap();
            }
            for _ in 0..3 {
                repository.create_database_record(database).unwrap();
            }
            let mut table = Self {
                context: egui::Context::default(),
                data: repository.database(database).unwrap(),
                properties: HashMap::new(),
                titles: HashMap::new(),
                values: HashMap::new(),
            };
            table.frame(Vec::new());
            table
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> Vec<Intent> {
            let mut intents = Vec::new();
            let _ = self.context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        super::super::show_embedded(
                            ui,
                            &mut self.data,
                            Palette::new(false),
                            &mut intents,
                            &mut Drafts {
                                properties: &mut self.properties,
                                titles: &mut self.titles,
                                values: &mut self.values,
                            },
                        );
                    });
                },
            );
            intents
        }

        fn cells(&self) -> Vec<Cell> {
            self.context
                .data(|state| {
                    state.get_temp(egui::Id::new(("database-table-cells", self.data.page_id)))
                })
                .unwrap()
        }

        fn focus(&mut self, target: CellTarget) -> egui::Id {
            let widget = self
                .cells()
                .iter()
                .find(|cell| cell.target == target)
                .unwrap()
                .widget;
            self.context
                .memory_mut(|memory| memory.request_focus(widget));
            self.frame(Vec::new());
            widget
        }

        fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> Vec<Intent> {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }])
        }
    }

    #[test]
    fn tab_moves_between_primary_editable_cells_and_wraps_rows() {
        let mut table = Table::new();
        let cells = table.cells();
        let first = &cells[0];
        table.focus(first.target);
        for cell in cells.iter().skip(1) {
            let intents = table.key(egui::Key::Tab, egui::Modifiers::NONE);
            assert!(intents.is_empty());
            assert_eq!(
                table.context.memory(|memory| memory.focused()),
                Some(cell.widget)
            );
        }
        table.key(egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(
            table.context.memory(|memory| memory.focused()),
            Some(first.widget)
        );
        table.key(egui::Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(
            table.context.memory(|memory| memory.focused()),
            Some(cells.last().unwrap().widget)
        );
    }

    #[test]
    fn enter_from_each_primary_cell_creates_one_inline_record() {
        let mut table = Table::new();
        for cell in table.cells() {
            table.focus(cell.target);
            let intents = table.key(egui::Key::Enter, egui::Modifiers::NONE);
            assert_eq!(intents.iter().filter(|intent| matches!(intent,
                Intent::Action(Action::CreateDatabaseRecordInline(id)) if *id == table.data.page_id
            )).count(), 1);
        }
    }

    #[test]
    fn navigation_skips_hidden_properties_and_filtered_out_records() {
        let mut table = Table::new();
        let record = table.data.rows[1].page.id;
        table.data.rows[1].page.title = "Matching record".into();
        let property = table
            .data
            .properties
            .iter()
            .find(|property| property.kind == "text")
            .unwrap()
            .id;
        table.data.view_settings.table_visible_properties = Some(vec![property]);
        table.context.data_mut(|state| {
            state.insert_temp(
                egui::Id::new(("database-table-find-query", table.data.page_id)),
                "Matching".to_owned(),
            )
        });
        table.frame(Vec::new());
        let cells = table.cells();
        assert_eq!(cells.len(), 2);
        assert!(cells.iter().all(|cell| cell.target.record == record));
        table.focus(cells[0].target);
        table.key(egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(
            table.context.memory(|memory| memory.focused()),
            Some(cells[1].widget)
        );
        table.key(egui::Key::Tab, egui::Modifiers::NONE);
        assert_eq!(
            table.context.memory(|memory| memory.focused()),
            Some(cells[0].widget)
        );
    }

    #[test]
    fn navigation_reveals_offscreen_cells_and_new_record_name() {
        let mut table = Table::new();
        let template = table.data.rows[0].clone();
        table.data.rows = (1000..1060)
            .map(|id| {
                let mut row = template.clone();
                row.page.id = id;
                row
            })
            .collect();
        table.frame(Vec::new());
        let first = table.cells()[0].target;
        table.focus(first);
        table.key(egui::Key::Tab, egui::Modifiers::SHIFT);
        let focused = table.context.memory(|memory| memory.focused()).unwrap();
        assert!(table.cells().iter().any(|cell| cell.widget == focused
            && cell.target.record == 1059
            && cell.target.property.is_some()));

        table.context.data_mut(|state| {
            state.insert_temp(
                egui::Id::new(("pending-table-focus-record", table.data.page_id)),
                first.record,
            )
        });
        table.frame(Vec::new());
        let focused = table.context.memory(|memory| memory.focused()).unwrap();
        assert!(
            table
                .cells()
                .iter()
                .any(|cell| cell.widget == focused && cell.target == first)
        );
    }

    #[test]
    fn enter_commits_text_and_calculates_numbers_before_creating_a_record() {
        let mut table = Table::new();
        let record = table.data.rows[0].page.id;
        for (kind, input, expected) in [("text", "edited", "edited"), ("number", "2 + 3", "5")] {
            let property = table
                .data
                .properties
                .iter()
                .find(|property| property.kind == kind)
                .unwrap()
                .id;
            table.focus(CellTarget {
                record,
                property: Some(property),
            });
            table.frame(vec![egui::Event::Text(input.into())]);
            let intents = table.key(egui::Key::Enter, egui::Modifiers::NONE);
            assert!(
                matches!(intents.first(), Some(Intent::Action(Action::SetDatabaseProperty { page, property: id, value }))
                if *page == record && *id == property && value == expected)
            );
            assert!(matches!(
                intents.last(),
                Some(Intent::Action(Action::CreateDatabaseRecordInline(_)))
            ));
        }
    }

    #[test]
    fn invalid_number_keeps_focus_without_creating_a_record() {
        let mut table = Table::new();
        let record = table.data.rows[0].page.id;
        let property = table
            .data
            .properties
            .iter()
            .find(|property| property.kind == "number")
            .unwrap()
            .id;
        let widget = table.focus(CellTarget {
            record,
            property: Some(property),
        });
        table.frame(vec![egui::Event::Text("invalid".into())]);
        for key in [egui::Key::Enter, egui::Key::Tab] {
            let intents = table.key(key, egui::Modifiers::NONE);
            assert!(
                intents.is_empty(),
                "key={key:?}, value={:?}, actions={:?}",
                table.values.get(&(record, property)),
                intents
                    .iter()
                    .filter_map(|intent| if let Intent::Action(action) = intent {
                        Some(action)
                    } else {
                        None
                    })
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                table.context.memory(|memory| memory.focused()),
                Some(widget)
            );
        }
    }
}
