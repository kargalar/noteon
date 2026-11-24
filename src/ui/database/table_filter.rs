use eframe::egui::{self, RichText, Ui};

use crate::domain::model::{DatabaseData, DatabaseProperty, DatabaseRow};

use crate::ui::palette::Palette;

pub(super) fn show_table_find_bar(ui: &mut Ui, data: &DatabaseData, palette: Palette) {
    let open_id = egui::Id::new(("database-table-find-open", data.page_id));
    if !ui
        .ctx()
        .data(|state| state.get_temp::<bool>(open_id).unwrap_or(false))
    {
        return;
    }

    let query_id = egui::Id::new(("database-table-find-query", data.page_id));
    let focus_id = egui::Id::new(("database-table-find-focus", data.page_id));
    let mut query = ui
        .ctx()
        .data(|state| state.get_temp::<String>(query_id))
        .unwrap_or_default();
    let mut close = false;

    ui.horizontal(|ui| {
        ui.label(RichText::new("⌕").color(palette.muted));
        let response = ui.add(
            egui::TextEdit::singleline(&mut query)
                .id(query_id)
                .hint_text("Tabloda ara…")
                .desired_width(240.0),
        );
        if ui
            .ctx()
            .data(|state| state.get_temp::<bool>(focus_id).unwrap_or(false))
        {
            response.request_focus();
            ui.ctx().data_mut(|state| state.remove::<bool>(focus_id));
        }
        if ui.memory(|memory| memory.had_focus_last_frame(response.id))
            && ui.input(|input| input.key_pressed(egui::Key::Escape))
        {
            close = true;
        }
        if !query.trim().is_empty() {
            let matching_rows = data
                .rows
                .iter()
                .filter(|row| table_row_matches(row, &data.properties, &query))
                .count();
            ui.label(
                RichText::new(format!("{matching_rows}/{}", data.rows.len()))
                    .size(11.5)
                    .color(palette.muted),
            );
        }
        if ui
            .add(egui::Button::new(RichText::new("×").color(palette.muted)).frame(false))
            .on_hover_text("Kapat (Esc)")
            .clicked()
        {
            close = true;
        }
    });

    ui.ctx().data_mut(|state| {
        state.insert_temp(query_id, query);
        if close {
            state.insert_temp(open_id, false);
        }
    });
}

pub(super) fn table_row_matches(
    row: &DatabaseRow,
    properties: &[DatabaseProperty],
    query: &str,
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    row.page.display_title().to_lowercase().contains(&query)
        || row.preview.to_lowercase().contains(&query)
        || properties.iter().any(|property| {
            row.values
                .get(&property.id)
                .is_some_and(|value| value.to_lowercase().contains(&query))
        })
}
