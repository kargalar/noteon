use std::collections::{HashMap, HashSet};

use crate::domain::model::{DatabaseData, PageEdit, PageId};
use crate::worker::Request;

use super::NoteonApp;

const MAX_RESIDENT_EDITORS: usize = 12;
const MAX_RESIDENT_DATABASES: usize = 8;

impl NoteonApp {
    pub(super) fn evict_inactive_editors(&mut self, visible_pages: &HashSet<PageId>) {
        while self.editors.len() > MAX_RESIDENT_EDITORS {
            let Some(page) = self.editor_lru.pop_front() else {
                break;
            };
            if visible_pages.contains(&page) {
                self.editor_lru.push_back(page);
                if self
                    .editor_lru
                    .iter()
                    .all(|candidate| visible_pages.contains(candidate))
                {
                    break;
                }
            } else {
                self.editors.remove(&page);
            }
        }
    }

    pub(super) fn touch_editor(&mut self, page: PageId) {
        self.editor_lru.retain(|candidate| *candidate != page);
        self.editor_lru.push_back(page);
    }

    pub(super) fn evict_inactive_databases(&mut self, visible: &HashSet<PageId>) {
        while self.databases.len() > MAX_RESIDENT_DATABASES {
            let Some(page) = self.database_lru.pop_front() else {
                break;
            };
            if visible.contains(&page) {
                self.database_lru.push_back(page);
                if self
                    .database_lru
                    .iter()
                    .all(|candidate| visible.contains(candidate))
                {
                    break;
                }
            } else {
                self.invalidate_database(page);
            }
        }
    }

    pub(super) fn touch_database(&mut self, page: PageId) {
        self.database_lru.retain(|candidate| *candidate != page);
        self.database_lru.push_back(page);
    }

    pub(super) fn invalidate_database(&mut self, page: PageId) {
        if let Some(database) = self.databases.remove(&page) {
            for row in database.rows {
                self.database_title_drafts.remove(&row.page.id);
                for property in &database.properties {
                    self.database_value_drafts
                        .remove(&(row.page.id, property.id));
                }
            }
            for property in database.properties {
                self.database_property_drafts.remove(&property.id);
            }
        }
        self.database_lru.retain(|candidate| *candidate != page);
        self.database_loading.remove(&page);
    }

    pub(super) fn refresh_database(&mut self, page: PageId) {
        if let Some(database) = self.databases.get(&page) {
            for property in &database.properties {
                self.database_property_drafts.remove(&property.id);
            }
        }
        if self.database_loading.insert(page) && !self.send(Request::Database(page)) {
            self.database_loading.remove(&page);
        }
    }
}

pub(super) fn sync_database_value_drafts(
    previous: Option<&DatabaseData>,
    incoming: &DatabaseData,
    drafts: &mut HashMap<(PageId, i64), String>,
) {
    let Some(previous) = previous.filter(|_| !drafts.is_empty()) else {
        return;
    };
    let previous_rows: HashMap<_, _> = previous.rows.iter().map(|row| (row.page.id, row)).collect();
    let incoming_rows: HashMap<_, _> = incoming.rows.iter().map(|row| (row.page.id, row)).collect();
    let properties: HashSet<_> = incoming
        .properties
        .iter()
        .map(|property| property.id)
        .collect();

    drafts.retain(|(page, property), draft| {
        let (Some(previous), Some(incoming)) = (previous_rows.get(page), incoming_rows.get(page))
        else {
            return true;
        };
        if !properties.contains(property) {
            return true;
        }
        let old_value = previous.values.get(property).map_or("", String::as_str);
        let new_value = incoming.values.get(property).map_or("", String::as_str);
        old_value == new_value || draft != old_value
    });
}

pub(super) fn sync_database_title_drafts(
    previous: Option<&DatabaseData>,
    incoming: &DatabaseData,
    title_drafts: &mut HashMap<PageId, String>,
) {
    let Some(previous) = previous else {
        return;
    };
    let previous_titles: HashMap<_, _> = previous
        .rows
        .iter()
        .map(|row| (row.page.id, &row.page.title))
        .collect();
    for row in &incoming.rows {
        if let Some(old_title) = previous_titles.get(&row.page.id)
            && *old_title != &row.page.title
            && title_drafts.get(&row.page.id) == Some(*old_title)
        {
            title_drafts.remove(&row.page.id);
        }
    }
}

pub(super) fn sync_database_page(
    databases: &mut HashMap<PageId, DatabaseData>,
    title_drafts: &mut HashMap<PageId, String>,
    page: &PageEdit,
) {
    title_drafts.remove(&page.id);
    if let Some(database) = databases.get_mut(&page.id) {
        database.title.clone_from(&page.title);
        database.icon.clone_from(&page.icon);
    }
    for database in databases.values_mut() {
        if let Some(row) = database.rows.iter_mut().find(|row| row.page.id == page.id) {
            row.page.title.clone_from(&page.title);
            row.page.icon.clone_from(&page.icon);
            row.page.cover = page.cover;
            row.page.cover_image.clone_from(&page.cover_image);
            row.page.cover_x = page.cover_x;
            row.page.cover_y = page.cover_y;
            row.page.full_width = page.full_width;
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    use crate::ui::{Intent, palette::Palette, theme};
    use crate::worker::Action;

    use super::*;
    use crate::storage::Repository;
    use crate::ui::database::{Drafts, show_embedded};

    #[test]
    fn property_refresh_preserves_typing_newer_than_the_saved_value() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
        let database = repository.create_database(None).unwrap();
        let record = repository.create_database_record(database).unwrap();
        repository
            .add_database_property(database, "Text", "text", &[])
            .unwrap();
        let previous = repository.database(database).unwrap();
        let property = previous
            .properties
            .iter()
            .find(|property| property.kind == "text")
            .unwrap()
            .id;
        let key = (record, property);
        let mut drafts = HashMap::from([(key, "ab".into())]);
        repository
            .set_database_property(record, property, "a")
            .unwrap();
        let incoming = repository.database(database).unwrap();
        sync_database_value_drafts(Some(&previous), &incoming, &mut drafts);
        assert_eq!(drafts[&key], "ab");
        repository
            .set_database_property(record, property, "ab")
            .unwrap();
        let saved = repository.database(database).unwrap();
        sync_database_value_drafts(Some(&incoming), &saved, &mut drafts);
        assert_eq!(drafts[&key], "ab");
        repository
            .set_database_property(record, property, "External update")
            .unwrap();
        let updated = repository.database(database).unwrap();
        sync_database_value_drafts(Some(&saved), &updated, &mut drafts);
        assert!(!drafts.contains_key(&key));
    }

    #[test]
    fn database_refresh_replaces_stale_titles_and_preserves_unfinished_edits() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
        let database_id = repository.create_database(None).unwrap();
        let record_id = repository.create_database_record(database_id).unwrap();
        let sibling_id = repository.create_database_record(database_id).unwrap();
        let previous = repository.database(database_id).unwrap();
        let mut drafts = HashMap::from([
            (record_id, previous.rows[0].page.title.clone()),
            (sibling_id, "Unfinished edit".into()),
        ]);
        repository
            .rename_database_record(record_id, "Renamed record")
            .unwrap();
        repository
            .rename_database_record(sibling_id, "Saved sibling")
            .unwrap();
        let incoming = repository.database(database_id).unwrap();

        sync_database_title_drafts(Some(&previous), &incoming, &mut drafts);

        assert!(!drafts.contains_key(&record_id));
        assert_eq!(
            drafts.get(&sibling_id).map(String::as_str),
            Some("Unfinished edit")
        );
    }

    #[test]
    fn record_page_rename_updates_the_cached_table_and_its_title_draft() {
        let directory = tempfile::tempdir().unwrap();
        let mut repository = Repository::open(&directory.path().join("test.sqlite")).unwrap();
        let database_id = repository.create_database(None).unwrap();
        let record_id = repository.create_database_record(database_id).unwrap();
        let sibling_id = repository.create_database_record(database_id).unwrap();
        let data = repository.database(database_id).unwrap();
        let mut title_drafts = HashMap::from([
            (record_id, "Old table title".into()),
            (sibling_id, "Unfinished sibling title".into()),
        ]);
        let mut databases = HashMap::from([(database_id, data)]);
        let mut edit = PageEdit::from(&repository.page(record_id).unwrap());
        edit.title = "Title changed on the record page".into();

        sync_database_page(&mut databases, &mut title_drafts, &edit);

        let context = egui::Context::default();
        theme::configure(&context);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        let mut property_drafts = HashMap::new();
        let mut value_drafts = HashMap::new();
        let mut intents = Vec::new();
        let data = databases.get_mut(&database_id).unwrap();
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                show_embedded(
                    ui,
                    data,
                    Palette::new(false),
                    &mut intents,
                    &mut Drafts {
                        properties: &mut property_drafts,
                        titles: &mut title_drafts,
                        values: &mut value_drafts,
                    },
                );
            });
        });

        assert_eq!(
            data.rows
                .iter()
                .find(|row| row.page.id == record_id)
                .unwrap()
                .page
                .title,
            edit.title
        );
        assert_eq!(title_drafts.get(&record_id), Some(&edit.title));
        assert_eq!(
            title_drafts.get(&sibling_id).map(String::as_str),
            Some("Unfinished sibling title")
        );
        assert!(intents.iter().all(|intent| !matches!(intent, Intent::Action(Action::RenameDatabaseRecord { page, .. }) if *page == record_id)));
    }
}
