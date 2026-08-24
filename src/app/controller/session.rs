use crate::core::focus::Focus;

use super::super::startpage::start_page_for;
use super::{App, STARTUP_HINT, apply_rendered_page, content_viewport, update_load_message};

impl App {
    pub(super) fn new_tab(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tabs.open_tab(
            String::new(),
            generation,
            start_page_for(content_viewport(self.geometry)),
            STARTUP_HINT.to_string(),
        );
        self.address.set_text("");
        self.focus = Focus::Address;
        self.pressed = None;
        self.refresh_hover();
        self.touch();
    }

    pub(super) fn close_tab(&mut self) {
        self.net
            .cancel(self.tabs.active().id, self.tabs.active().generation);
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tabs.close_active(
            generation,
            start_page_for(content_viewport(self.geometry)),
            STARTUP_HINT.to_string(),
        );
        if self.focus == Focus::Address {
            self.address.set_text(&self.tabs.active().url);
        }
        self.pressed = None;
        self.refresh_hover();
        self.touch();
    }

    pub(super) fn activate_current(&mut self) {
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let now = self.now;
        let tab = self.tabs.active_mut();
        let page = match tab.load.as_mut() {
            Some(load) if tab.render_dirty && load.has_painted() => Some(load.force_render()),
            Some(load) => load.render_if_ready(now),
            None => None,
        };
        if let Some(page) = page {
            let css_warnings = page.css_warnings;
            let parse_errors = page.parse_errors;
            apply_rendered_page(tab, page, width, rows);
            update_load_message(tab, parse_errors, css_warnings);
        }
        tab.render_dirty = false;
    }
}
