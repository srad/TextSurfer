use crate::core::dom::SharedDocument;
use crate::core::style::StyleTree;
use crate::paint::DisplayList;

use super::page_load::PageLoad;

pub struct Tab {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub history: Vec<String>,
    pub history_pos: usize,
    pub scroll: usize,
    pub layout_width: usize,
    pub generation: u64,
    pub painted: DisplayList,
    pub message: String,
    pub document: Option<SharedDocument>,
    pub styles: Option<StyleTree>,
    pub load: Option<PageLoad>,
    pub render_dirty: bool,
}

impl Tab {
    pub(crate) fn new(
        id: u64,
        url: String,
        generation: u64,
        painted: DisplayList,
        message: String,
    ) -> Self {
        let title = if url.is_empty() {
            "start".to_string()
        } else {
            url.clone()
        };
        let history = if url.is_empty() {
            Vec::new()
        } else {
            vec![url.clone()]
        };
        Self {
            id,
            title,
            history,
            history_pos: 0,
            url,
            scroll: 0,
            layout_width: 0,
            generation,
            painted,
            message,
            document: None,
            styles: None,
            load: None,
            render_dirty: false,
        }
    }

    pub fn push_history(&mut self, url: &str) {
        if !self.history.is_empty() {
            self.history.truncate(self.history_pos + 1);
        }
        if self.history.last().map(String::as_str) != Some(url) {
            self.history.push(url.to_string());
        }
        self.history_pos = self.history.len() - 1;
    }

    pub fn back(&mut self) -> bool {
        if self.history_pos == 0 {
            return false;
        }
        self.history_pos -= 1;
        true
    }

    pub fn forward(&mut self) -> bool {
        if self.history_pos + 1 >= self.history.len() {
            return false;
        }
        self.history_pos += 1;
        true
    }

    pub fn current(&self) -> Option<&str> {
        self.history.get(self.history_pos).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab() -> Tab {
        Tab::new(
            0,
            "https://start".to_string(),
            0,
            DisplayList::default(),
            "Ready".to_string(),
        )
    }

    #[test]
    fn back_and_forward_walk_the_stack_and_stop_at_the_ends() {
        let mut t = tab();
        t.push_history("https://a");
        t.push_history("https://b");
        assert_eq!(t.current(), Some("https://b"));
        assert!(t.back());
        assert_eq!(t.current(), Some("https://a"));
        assert!(t.back());
        assert_eq!(t.current(), Some("https://start"));
        assert!(!t.back());
        assert!(t.forward());
        assert_eq!(t.current(), Some("https://a"));
        assert!(t.forward());
        assert_eq!(t.current(), Some("https://b"));
        assert!(!t.forward());
    }

    #[test]
    fn a_new_navigation_truncates_the_forward_stack() {
        let mut t = tab();
        t.push_history("https://a");
        t.push_history("https://b");
        t.back();
        t.push_history("https://c");
        assert_eq!(t.history, vec!["https://start", "https://a", "https://c"]);
        assert_eq!(t.current(), Some("https://c"));
        assert!(!t.forward());
    }

    #[test]
    fn navigating_back_then_forward_restores_the_forward_stack() {
        let mut t = tab();
        t.push_history("https://a");
        t.back();
        assert!(t.forward());
        assert_eq!(t.current(), Some("https://a"));
        assert_eq!(t.history, vec!["https://start", "https://a"]);
    }

    #[test]
    fn a_blank_tab_has_no_fake_history_entry() {
        let mut tab = Tab::new(
            0,
            String::new(),
            0,
            DisplayList::default(),
            "Ready".to_string(),
        );
        assert!(tab.history.is_empty());
        assert_eq!(tab.current(), None);
        tab.push_history("https://example.com");
        assert_eq!(tab.current(), Some("https://example.com"));
    }
}
