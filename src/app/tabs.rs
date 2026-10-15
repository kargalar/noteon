use std::collections::HashSet;

use egui_dock::DockState;
use serde::{Deserialize, Serialize};

use crate::domain::model::PageId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTab {
    pub page_id: PageId,
    pub instance_id: u64,
}

pub fn restore_layout(
    layout: &str,
    valid_pages: &HashSet<PageId>,
) -> Option<(DockState<PageTab>, u64)> {
    if let Ok(dock) = serde_json::from_str::<DockState<PageTab>>(layout) {
        let dock = dock.filter_map_tabs(|tab| valid_pages.contains(&tab.page_id).then_some(*tab));
        let next_id = dock
            .iter_all_tabs()
            .map(|(_, tab)| tab.instance_id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        return Some((dock, next_id));
    }

    let legacy = serde_json::from_str::<DockState<PageId>>(layout).ok()?;
    let mut next_id = 1_u64;
    let dock = legacy.filter_map_tabs(|page_id| {
        if !valid_pages.contains(page_id) {
            return None;
        }
        let tab = PageTab {
            page_id: *page_id,
            instance_id: next_id,
        };
        next_id = next_id.saturating_add(1);
        Some(tab)
    });
    Some((dock, next_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_layouts_restore_as_unique_page_tabs() {
        let legacy = DockState::new(vec![10_i64, 20_i64]);
        let encoded = serde_json::to_string(&legacy).unwrap().replace("null", "0");
        let valid_pages = HashSet::from([10, 20]);

        let (restored, next_id) = restore_layout(&encoded, &valid_pages).unwrap();
        let tabs: Vec<_> = restored.iter_all_tabs().map(|(_, tab)| *tab).collect();

        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].page_id, 10);
        assert_eq!(tabs[1].page_id, 20);
        assert_ne!(tabs[0].instance_id, tabs[1].instance_id);
        assert_eq!(next_id, 3);
    }

    #[test]
    fn separate_tabs_can_reference_the_same_page() {
        let first_tab = PageTab {
            page_id: 10,
            instance_id: 1,
        };
        let second_tab = PageTab {
            page_id: 10,
            instance_id: 2,
        };
        let dock = DockState::new(vec![first_tab, second_tab]);
        let encoded = serde_json::to_string(&dock).unwrap().replace("null", "0");
        let valid_pages = HashSet::from([10]);

        let (restored, next_id) = restore_layout(&encoded, &valid_pages).unwrap();
        let tabs: Vec<_> = restored.iter_all_tabs().map(|(_, tab)| *tab).collect();

        assert_eq!(tabs, [first_tab, second_tab]);
        assert_eq!(next_id, 3);
    }
}
