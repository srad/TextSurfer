use std::collections::HashMap;

use url::Url;

use crate::core::dom::SharedDocument;
use crate::core::style::StyleTree;
use crate::css::FocusedNode;
use crate::paint::DisplayList;
use crate::pipeline::page_load::PageLoad;
use crate::ui::widgets::text_field::TextFieldState;

#[cfg(test)]
mod tests;

pub struct Tab {
    pub id: u64,
    pub url: String,
    /// What relative links in this document resolve against, once a page has parsed.
    pub base: Option<Url>,
    pub title: String,
    pub history: Vec<HistoryEntry>,
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
    pub automatic_redirects: u8,
    pub(crate) dom_focus: Option<FocusedNode>,
    pub(crate) text_fields: HashMap<crate::core::dom::NodeId, TextFieldState>,
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
            vec![HistoryEntry {
                url: url.clone(),
                replay: HistoryReplay::Get,
            }]
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
            automatic_redirects: 0,
            dom_focus: None,
            text_fields: HashMap::new(),
        }
    }

    pub fn push_history(&mut self, url: &str) {
        self.push_history_with_replay(url, HistoryReplay::Get);
    }

    pub fn push_history_with_replay(&mut self, url: &str, replay: HistoryReplay) {
        if !self.history.is_empty() {
            self.history.truncate(self.history_pos + 1);
        }
        if self.history.last().map(|entry| entry.url.as_str()) != Some(url) {
            self.history.push(HistoryEntry {
                url: url.to_string(),
                replay,
            });
        } else if let Some(entry) = self.history.last_mut() {
            entry.replay = replay;
        }
        self.history_pos = self.history.len() - 1;
    }

    pub fn replace_history(&mut self, url: &str) {
        if let Some(entry) = self.history.get_mut(self.history_pos) {
            entry.url = url.to_string();
            entry.replay = HistoryReplay::Get;
        } else {
            self.history.push(HistoryEntry {
                url: url.to_string(),
                replay: HistoryReplay::Get,
            });
            self.history_pos = 0;
        }
    }

    pub fn back(&mut self) -> bool {
        if self.history_pos == 0 {
            return false;
        }
        let Some(position) = (0..self.history_pos)
            .rev()
            .find(|position| self.history[*position].replay == HistoryReplay::Get)
        else {
            return false;
        };
        self.history_pos = position;
        true
    }

    pub fn forward(&mut self) -> bool {
        let Some(position) = (self.history_pos + 1..self.history.len())
            .find(|position| self.history[*position].replay == HistoryReplay::Get)
        else {
            return false;
        };
        self.history_pos = position;
        true
    }

    pub fn current(&self) -> Option<&str> {
        self.history
            .get(self.history_pos)
            .map(|entry| entry.url.as_str())
    }

    pub fn current_is_non_replayable(&self) -> bool {
        self.history
            .get(self.history_pos)
            .is_some_and(|entry| entry.replay == HistoryReplay::NonReplayablePost)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplay {
    Get,
    NonReplayablePost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub replay: HistoryReplay,
}

impl PartialEq<&str> for HistoryEntry {
    fn eq(&self, other: &&str) -> bool {
        self.url == *other
    }
}

impl PartialEq<String> for HistoryEntry {
    fn eq(&self, other: &String) -> bool {
        self.url == *other
    }
}
