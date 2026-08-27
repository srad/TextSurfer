use crate::core::focus::Focus;
use crate::core::url::url_fix;
use crate::net::{ResourceId, Submitted};
use crate::paint::DisplayList;
use crate::pipeline::page_load::MAX_DECLARATIVE_REFRESHES;

use super::super::net::{Route, route};
use super::super::startpage::{content_for, start_page_for};
use super::{App, content_viewport};

enum HistoryUpdate {
    Keep,
    Push,
    Replace,
}

impl App {
    pub fn submit_url(&mut self, url: &str) {
        let fixed = url_fix(url);
        if !fixed.is_empty() {
            self.open_url(&fixed);
        }
    }

    pub(super) fn submit_address(&mut self) {
        let fixed = url_fix(self.address.text());
        if fixed.is_empty() {
            self.tabs.active_mut().message = "type a URL or search terms first".to_string();
            self.touch();
            return;
        }
        self.navigate(&fixed, true);
        self.address.set_text(&fixed);
    }

    pub(super) fn open_url(&mut self, url: &str) {
        self.navigate(url, true);
    }

    fn navigate(&mut self, raw: &str, record: bool) {
        let index = self.tabs.active_index();
        self.tabs.tabs_mut()[index].automatic_redirects = 0;
        let history = if record {
            HistoryUpdate::Push
        } else {
            HistoryUpdate::Keep
        };
        self.navigate_at(index, raw, history);
    }

    pub(super) fn follow_declarative_refresh(&mut self, tab_id: u64, url: &url::Url) {
        let Some(index) = self.tabs.tabs().iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        if self.tabs.tabs()[index].automatic_redirects >= MAX_DECLARATIVE_REFRESHES {
            let tab = &mut self.tabs.tabs_mut()[index];
            tab.document_pending = false;
            tab.load = None;
            tab.message = "automatic redirect limit reached".to_string();
            self.touch();
            return;
        }
        self.tabs.tabs_mut()[index].automatic_redirects += 1;
        self.tabs.tabs_mut()[index].load = None;
        self.navigate_at(index, url.as_str(), HistoryUpdate::Replace);
    }

    fn navigate_at(&mut self, index: usize, raw: &str, history: HistoryUpdate) {
        let fixed = url_fix(raw);
        if fixed.is_empty() {
            return;
        }
        let parsed = match url::Url::parse(&fixed) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.tabs.tabs_mut()[index].message = format!("cannot parse url: {fixed}");
                self.touch();
                return;
            }
        };
        match route(&parsed) {
            Route::StartPage => {
                let tab = &self.tabs.tabs()[index];
                self.net.cancel(tab.id, tab.generation);
                self.images.cancel(tab.id, tab.generation);
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                let page = start_page_for(content_viewport(self.geometry));
                self.repoint(index, &fixed, generation, page);
                self.update_history(index, &fixed, history);
                self.tabs.tabs_mut()[index].message = "Ready".to_string();
            }
            Route::Fetch => {
                let tab = &self.tabs.tabs()[index];
                self.net.cancel(tab.id, tab.generation);
                self.images.cancel(tab.id, tab.generation);
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                self.repoint(index, &fixed, generation, content_for(&fixed));
                self.update_history(index, &fixed, history);
                let tab = &mut self.tabs.tabs_mut()[index];
                tab.message = format!("loading {fixed}");
                tab.document_pending = true;
                let submitted = self
                    .net
                    .submit(tab.id, generation, ResourceId::DOCUMENT, parsed);
                if submitted != Submitted::Queued {
                    // Nothing accepted the job, so no payload will ever arrive. Without
                    // this the tab sits on "loading" until the user gives up.
                    tab.document_pending = false;
                    tab.message = format!("cannot load {fixed}: the network is not running");
                }
            }
            Route::Reject => {
                self.tabs.tabs_mut()[index].message =
                    format!("unsupported scheme: {}", parsed.scheme());
                self.touch();
                return;
            }
        }
        if index == self.tabs.active_index() {
            self.focus = Focus::Content;
        }
        self.touch();
    }

    fn update_history(&mut self, index: usize, url: &str, update: HistoryUpdate) {
        let tab = &mut self.tabs.tabs_mut()[index];
        match update {
            HistoryUpdate::Keep => {}
            HistoryUpdate::Push => tab.push_history(url),
            HistoryUpdate::Replace => tab.replace_history(url),
        }
    }

    fn repoint(&mut self, index: usize, url: &str, generation: u64, painted: DisplayList) {
        let tab = &mut self.tabs.tabs_mut()[index];
        tab.url = url.to_string();
        tab.title = url.to_string();
        tab.painted = painted;
        tab.scroll = 0;
        tab.layout_width = self.geometry.content_cols();
        tab.generation = generation;
        tab.document = None;
        tab.styles = None;
        tab.load = None;
        tab.document_pending = false;
        tab.base = None;
        tab.render_dirty = false;
        tab.dom_focus = None;
        if index == self.tabs.active_index() {
            self.pressed = None;
            self.refresh_hover();
        }
    }

    pub(super) fn go_back(&mut self) {
        if self.tabs.active_mut().back() {
            let url = self
                .tabs
                .active()
                .current()
                .expect("back moved to a history entry")
                .to_string();
            self.navigate(&url, false);
        } else {
            self.tabs.active_mut().message = "already at the first page".to_string();
            self.touch();
        }
    }

    pub(super) fn go_forward(&mut self) {
        if self.tabs.active_mut().forward() {
            let url = self
                .tabs
                .active()
                .current()
                .expect("forward moved to a history entry")
                .to_string();
            self.navigate(&url, false);
        } else {
            self.tabs.active_mut().message = "already at the last page".to_string();
            self.touch();
        }
    }

    pub(super) fn reload(&mut self) {
        let url = self.tabs.active().url.clone();
        if url.is_empty() {
            self.tabs.active_mut().message = "nothing to reload".to_string();
            self.touch();
            return;
        }
        self.navigate(&url, false);
    }

    pub(super) fn home(&mut self) {
        self.navigate("about:blank", true);
    }
}
