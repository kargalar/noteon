use eframe::egui::Ui;

pub(super) fn read_draft(ui: &Ui, key: impl std::hash::Hash, initial: &str) -> String {
    let id = ui.id().with(key);
    ui.ctx()
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| initial.to_owned())
}

pub(super) fn save_draft(ui: &Ui, key: impl std::hash::Hash, value: &str, changed: bool) {
    if changed {
        let id = ui.id().with(key);
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, value.to_owned()));
    }
}

pub(super) fn clear_draft(ui: &Ui, key: impl std::hash::Hash) {
    let id = ui.id().with(key);
    ui.ctx().data_mut(|data| data.remove::<String>(id));
}
