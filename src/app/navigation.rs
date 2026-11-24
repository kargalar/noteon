use egui_dock::{Node, Split};

use crate::config;
use crate::domain::model::PageId;
use crate::ui::DockSide;
use crate::worker::Request;

use super::NoteonApp;
use super::tabs::PageTab;

impl NoteonApp {
    pub(super) fn open_page(&mut self, id: PageId, jump: Option<usize>, add_history: bool) {
        self.open_page_with_mode(id, jump, add_history, false);
    }

    pub(super) fn open_page_in_tab(&mut self, id: PageId) {
        self.open_page_with_mode(id, None, true, true);
    }

    pub(super) fn open_page_in_dialog(&mut self, id: PageId) {
        if self.busy || self.closing || !self.ready {
            return;
        }
        if self
            .sidebar
            .page(id)
            .is_none_or(|page| page.trashed_by.is_some())
        {
            return;
        }
        if let Some(editor) = self.editors.get_mut(&id) {
            self.pending_dialog = None;
            editor.title_focus = true;
            self.page_dialog = Some(id);
            self.touch_editor(id);
            return;
        }
        self.pending_dialog = Some(id);
        if self.pending_documents.values().any(|(page, _)| *page == id) {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.pending_documents.insert(generation, (id, None));
        if !self.send(Request::Document {
            page: id,
            generation,
        }) {
            self.pending_documents.remove(&generation);
            self.pending_dialog = None;
        }
    }

    fn open_page_with_mode(
        &mut self,
        id: PageId,
        jump: Option<usize>,
        add_history: bool,
        new_tab: bool,
    ) {
        if self.busy || self.closing || !self.ready {
            return;
        }
        if self
            .sidebar
            .page(id)
            .is_none_or(|page| page.trashed_by.is_some())
        {
            return;
        }
        self.activate_page_tab(id, new_tab);
        self.open_page_content(id, jump, add_history);
    }

    fn open_page_content(&mut self, id: PageId, jump: Option<usize>, add_history: bool) {
        self.trash = false;
        if self.sidebar.page(id).is_some_and(|page| page.is_database) {
            self.selected = Some(id);
            self.loading = false;
            self.sidebar.reveal(id);
            self.record_navigation(id, add_history);
            return;
        }
        if let Some(editor) = self.editors.get_mut(&id) {
            self.selected = Some(id);
            self.loading = false;
            if jump.is_some() {
                editor.jump_to = jump;
            } else {
                editor.title_focus = true;
            }
            self.sidebar.reveal(id);
            self.record_navigation(id, add_history);
            self.touch_editor(id);
            return;
        }
        if let Some((_, pending_jump)) = self
            .pending_documents
            .values_mut()
            .find(|(page, _)| *page == id)
        {
            *pending_jump = jump;
            self.selected = Some(id);
            self.loading = true;
            self.sidebar.reveal(id);
            self.record_navigation(id, add_history);
            return;
        }
        self.flush(true);
        self.selected = Some(id);
        self.generation += 1;
        self.pending_documents.insert(self.generation, (id, jump));
        self.loading = self.send(Request::Document {
            page: id,
            generation: self.generation,
        });
        if !self.loading {
            self.pending_documents.remove(&self.generation);
        }
        self.sidebar.reveal(id);
        self.record_navigation(id, add_history);
    }

    pub(super) fn record_navigation(&mut self, id: PageId, add_history: bool) {
        if add_history {
            if !self.history.is_empty() {
                self.history.truncate(self.history_index + 1);
            }
            if self.history.last() != Some(&id) {
                self.history.push(id);
            }
            self.history_index = self.history.len().saturating_sub(1);
        }
        self.send(Request::Setting {
            key: config::SettingKey::LastPage,
            value: id.to_string(),
        });
    }

    pub(super) fn activate_page_tab(&mut self, id: PageId, new_tab: bool) {
        if !new_tab {
            if self
                .dock
                .find_active_focused()
                .is_some_and(|(_, tab)| tab.page_id == id)
            {
                return;
            }
            let existing_tab = self
                .dock
                .iter_all_tabs()
                .map(|(_, tab)| *tab)
                .find(|tab| tab.page_id == id);
            if let Some(existing_tab) = existing_tab
                && let Some((surface, node, tab)) = self.dock.find_tab(&existing_tab)
            {
                self.dock.set_focused_node_and_surface((surface, node));
                self.dock.set_active_tab((surface, node, tab));
                return;
            }
        }
        let page_tab = PageTab {
            page_id: id,
            instance_id: self.next_tab_instance_id,
        };
        self.next_tab_instance_id = self.next_tab_instance_id.saturating_add(1);
        if !new_tab {
            let replaced = self.dock.find_active_focused().map(|(_, active_tab)| {
                let previous = *active_tab;
                *active_tab = page_tab;
                previous.page_id
            });
            if let Some(previous) = replaced
                && previous != id
                && !self
                    .dock
                    .iter_all_tabs()
                    .any(|(_, tab)| tab.page_id == previous)
            {
                self.editors.remove(&previous);
                self.editor_lru.retain(|page| *page != previous);
                self.pending_documents
                    .retain(|_, (page, _)| *page != previous);
            }
            if replaced.is_some() {
                return;
            }
        }
        self.dock.push_to_focused_leaf(page_tab);
    }

    pub(super) fn persist_dock_layout(&mut self) {
        let Ok(layout) = serde_json::to_string(&self.dock) else {
            return;
        };
        if self.saved_dock_layout.as_deref() == Some(&layout) {
            return;
        }
        if self.send(Request::Setting {
            key: config::SettingKey::DockLayout,
            value: layout.clone(),
        }) {
            self.saved_dock_layout = Some(layout);
        }
    }

    pub(super) fn navigate(&mut self, direction: isize) {
        if self.busy || self.closing {
            return;
        }
        let mut index = self.history_index;
        while let Some(next) = index.checked_add_signed(direction)
            && let Some(&page) = self.history.get(next)
        {
            if self
                .sidebar
                .page(page)
                .is_some_and(|page| page.trashed_by.is_none())
            {
                self.history_index = next;
                self.open_page(page, None, false);
                break;
            }
            index = next;
        }
    }

    pub(super) fn dock_page_at_edge(&mut self, id: PageId, target_instance: u64, side: DockSide) {
        if self.busy || self.closing || !self.ready {
            return;
        }
        if self
            .sidebar
            .page(id)
            .is_none_or(|page| page.trashed_by.is_some())
        {
            return;
        }
        let target = self
            .dock
            .iter_all_tabs()
            .map(|(_, tab)| *tab)
            .find(|tab| tab.instance_id == target_instance);
        let Some(target) = target else { return };
        let Some((surface, node, _)) = self.dock.find_tab(&target) else {
            return;
        };
        let page_tab = PageTab {
            page_id: id,
            instance_id: self.next_tab_instance_id,
        };
        self.next_tab_instance_id = self.next_tab_instance_id.saturating_add(1);
        let split = match side {
            DockSide::Left => Split::Left,
            DockSide::Right => Split::Right,
            DockSide::Top => Split::Above,
            DockSide::Bottom => Split::Below,
        };
        self.dock
            .split((surface, node), split, 0.5, Node::leaf_with(vec![page_tab]));
        self.selected = Some(id);
        self.open_page_content(id, None, true);
    }
}
