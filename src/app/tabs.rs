use super::tab::Tab;

pub struct TabManager {
    tabs: Vec<Tab>,
    active: usize,
}

impl TabManager {
    pub fn new(content: Vec<String>) -> Self {
        Self {
            tabs: vec![Tab::new(String::new(), 0, content)],
            active: 0,
        }
    }

    pub fn open_tab(&mut self, url: String, generation: u64, content: Vec<String>) {
        self.tabs.push(Tab::new(url, generation, content));
        self.active = self.tabs.len() - 1;
    }

    pub fn close_active(&mut self, generation: u64, fresh_content: Vec<String>) {
        if self.tabs.len() == 1 {
            self.tabs[0] = Tab::new(String::new(), generation, fresh_content);
            self.active = 0;
            return;
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
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

    fn lines(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("line {i}")).collect()
    }

    #[test]
    fn opening_a_tab_activates_it() {
        let mut manager = TabManager::new(lines(2));
        manager.open_tab("https://example.com".to_string(), 1, lines(3));
        assert_eq!(manager.len(), 2);
        assert_eq!(manager.active_index(), 1);
        assert_eq!(manager.active().url, "https://example.com");
        assert_eq!(manager.active().generation, 1);
    }

    #[test]
    fn next_and_prev_cycle_around() {
        let mut manager = TabManager::new(lines(1));
        manager.open_tab("https://a".to_string(), 1, lines(1));
        manager.open_tab("https://b".to_string(), 2, lines(1));
        manager.next();
        assert_eq!(manager.active_index(), 0);
        manager.prev();
        assert_eq!(manager.active_index(), 2);
        manager.prev();
        assert_eq!(manager.active_index(), 1);
    }

    #[test]
    fn closing_the_last_tab_opens_a_fresh_one() {
        let mut manager = TabManager::new(lines(2));
        manager.open_tab("https://a".to_string(), 1, lines(1));
        manager.close_active(2, lines(2));
        manager.close_active(3, lines(2));
        assert_eq!(manager.len(), 1);
        assert!(manager.active().url.is_empty());
        assert_eq!(manager.active().generation, 3);
        assert_eq!(manager.active().content.len(), 2);
    }

    #[test]
    fn history_dedup_keeps_consecutive_unique() {
        let mut manager = TabManager::new(lines(1));
        let tab = manager.active_mut();
        tab.push_history("https://a");
        tab.push_history("https://a");
        tab.push_history("https://b");
        assert_eq!(tab.history, vec!["", "https://a", "https://b"]);
    }
}
