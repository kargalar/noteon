use eframe::egui;

use crate::assistant::{PageContext, Role};
use crate::ui::assistant_panel::AssistantAction;
use crate::worker::Action;

use super::NoteonApp;

impl NoteonApp {
    pub(super) fn poll_assistant(&mut self) {
        if self.assistant.poll() {
            self.reload_workspace();
        }
    }

    pub(super) fn handle_assistant_actions(
        &mut self,
        actions: Vec<AssistantAction>,
        context: &egui::Context,
    ) {
        for action in actions {
            match action {
                AssistantAction::Send => self.send_assistant_request(context),
                AssistantAction::Stop => self.assistant.cancel(),
                AssistantAction::Undo => self.undo_assistant_changes(),
                AssistantAction::Clear => self.assistant.clear(),
                AssistantAction::Close => self.assistant.open = false,
                AssistantAction::ToggleSettings => {
                    self.assistant.settings_open = !self.assistant.settings_open;
                }
                AssistantAction::SettingsChanged => {
                    self.assistant.save_settings(&self.data_directory);
                }
            }
        }
    }

    fn send_assistant_request(&mut self, context: &egui::Context) {
        if !self.ready || self.busy || self.closing {
            return;
        }
        self.flush(true);
        let page = self.active_page_context();
        self.assistant.submit(context, &self.data_directory, page);
    }

    fn active_page_context(&self) -> Option<PageContext> {
        let page = self.sidebar.page(self.page_dialog.or(self.selected)?)?;
        Some(PageContext {
            id: page.id,
            title: page.title.clone(),
            is_database: page.is_database,
        })
    }

    fn undo_assistant_changes(&mut self) {
        if self.busy || !self.ready || self.closing {
            return;
        }
        self.flush(true);
        let Some(snapshot) = self.assistant.pop_undo_snapshot() else {
            return;
        };
        self.clear_loaded_views();
        self.action_in_batch(Action::RestoreSnapshot(snapshot), true);
        self.assistant.note(
            Role::Activity,
            "Restored the workspace to before the last AI change",
        );
    }

    fn reload_workspace(&mut self) {
        self.flush(true);
        self.clear_loaded_views();
        self.action_in_batch(Action::Reload, true);
    }

    fn clear_loaded_views(&mut self) {
        self.editors.clear();
        self.editor_lru.clear();
        self.pending_documents.clear();
        let databases: Vec<_> = self.databases.keys().copied().collect();
        for database in databases {
            self.invalidate_database(database);
        }
    }
}
