use super::tab::Tab;
use crate::paint::DisplayList;

pub struct TabManager {
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
}

impl TabManager {
    pub fn new(painted: DisplayList, message: String) -> Self {
        Self {
            tabs: vec![Tab::new(0, String::new(), 0, painted, message)],
            active: 0,
            next_id: 1,
        }
    }

    pub fn open_tab(
        &mut self,
        url: String,
        generation: u64,
        painted: DisplayList,
        message: String,
    ) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.tabs
            .push(Tab::new(id, url, generation, painted, message));
        self.active = self.tabs.len() - 1;
    }

    pub fn close_active(&mut self, generation: u64, fresh: DisplayList, message: String) {
        if self.tabs.len() == 1 {
            let id = self.tabs[0].id;
            self.tabs[0] = Tab::new(id, String::new(), generation, fresh, message);
            self.active = 0;
            return;
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
    }

    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || index == self.active {
            return false;
        }
        self.active = index;
        true
    }

    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    pub fn prev(&mut self) {
        self.active = self.active.checked_sub(1).unwrap_or(self.tabs.len() - 1);
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn tabs_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    pub fn find_load_mut(&mut self, tab_id: u64, generation: u64) -> Option<(usize, &mut Tab)> {
        self.tabs
            .iter_mut()
            .enumerate()
            .find(|(_, tab)| tab.id == tab_id && tab.generation == generation)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: usize) -> DisplayList {
        DisplayList::from_lines(
            &(1..=n)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn opening_a_tab_activates_it() {
        let mut manager = TabManager::new(lines(2), "Ready".to_string());
        manager.open_tab(
            "https://example.com".to_string(),
            1,
            lines(3),
            "Loading".to_string(),
        );
        assert_eq!(manager.len(), 2);
        assert_eq!(manager.active_index(), 1);
        assert_eq!(manager.active().url, "https://example.com");
        assert_eq!(manager.active().generation, 1);
    }

    #[test]
    fn next_and_prev_cycle_around() {
        let mut manager = TabManager::new(lines(1), "Ready".to_string());
        manager.open_tab("https://a".to_string(), 1, lines(1), "Ready".to_string());
        manager.open_tab("https://b".to_string(), 2, lines(1), "Ready".to_string());
        manager.next();
        assert_eq!(manager.active_index(), 0);
        manager.prev();
        assert_eq!(manager.active_index(), 2);
        manager.prev();
        assert_eq!(manager.active_index(), 1);
    }

    #[test]
    fn closing_the_last_tab_opens_a_fresh_one() {
        let mut manager = TabManager::new(lines(2), "Ready".to_string());
        manager.open_tab("https://a".to_string(), 1, lines(1), "Ready".to_string());
        manager.close_active(2, lines(2), "Ready".to_string());
        manager.close_active(3, lines(2), "Ready".to_string());
        assert_eq!(manager.len(), 1);
        assert!(manager.active().url.is_empty());
        assert_eq!(manager.active().generation, 3);
        assert_eq!(manager.active().painted.len(), 2);
    }

    #[test]
    fn history_dedup_keeps_consecutive_unique() {
        let mut manager = TabManager::new(lines(1), "Ready".to_string());
        let tab = manager.active_mut();
        tab.push_history("https://a");
        tab.push_history("https://a");
        tab.push_history("https://b");
        assert_eq!(tab.history, vec!["https://a", "https://b"]);
    }
}
