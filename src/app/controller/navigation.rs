use crate::core::focus::Focus;
use crate::core::url::url_fix;
use crate::net::ResourceId;
use crate::paint::DisplayList;

use super::super::net::{Route, route};
use super::super::startpage::{content_for, start_page_for};
use super::{App, content_viewport};

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
        let fixed = url_fix(raw);
        if fixed.is_empty() {
            return;
        }
        let parsed = match url::Url::parse(&fixed) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.tabs.active_mut().message = format!("cannot parse url: {fixed}");
                self.touch();
                return;
            }
        };
        match route(&parsed) {
            Route::StartPage => {
                self.net
                    .cancel(self.tabs.active().id, self.tabs.active().generation);
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                let page = start_page_for(content_viewport(self.geometry));
                self.repoint_active(&fixed, generation, page);
                if record {
                    self.tabs.active_mut().push_history(&fixed);
                }
                self.tabs.active_mut().message = "Ready".to_string();
            }
            Route::Fetch => {
                self.net
                    .cancel(self.tabs.active().id, self.tabs.active().generation);
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                self.repoint_active(&fixed, generation, content_for(&fixed));
                if record {
                    self.tabs.active_mut().push_history(&fixed);
                }
                self.tabs.active_mut().message = format!("loading {fixed}");
                self.tabs.active_mut().document_pending = true;
                self.net.submit(
                    self.tabs.active().id,
                    generation,
                    ResourceId::DOCUMENT,
                    parsed,
                );
            }
            Route::Reject => {
                self.tabs.active_mut().message = format!("unsupported scheme: {}", parsed.scheme());
                self.touch();
                return;
            }
        }
        self.focus = Focus::Content;
        self.touch();
    }

    fn repoint_active(&mut self, url: &str, generation: u64, painted: DisplayList) {
        let tab = self.tabs.active_mut();
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
        self.pressed = None;
        self.refresh_hover();
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
