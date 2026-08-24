use url::Url;

use crate::core::dom::SharedDocument;
use crate::core::style::StyleTree;
use crate::css::FocusedNode;
use crate::paint::DisplayList;
use crate::pipeline::page_load::PageLoad;

#[cfg(test)]
mod tests;

pub struct Tab {
    pub id: u64,
    pub url: String,
    /// What relative links in this document resolve against, once a page has parsed.
    pub base: Option<Url>,
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
    pub document_pending: bool,
    pub render_dirty: bool,
    pub(crate) dom_focus: Option<FocusedNode>,
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
            base: None,
            scroll: 0,
            layout_width: 0,
            generation,
            painted,
            message,
            document: None,
            styles: None,
            load: None,
            document_pending: false,
            render_dirty: false,
            dom_focus: None,
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
