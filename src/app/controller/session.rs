use crate::core::focus::Focus;

use super::super::startpage::start_page_for;
use super::super::tabs::FreshTab;
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

    pub(super) fn close_tab_at(&mut self, index: usize) {
        let Some(closing) = self.tabs.tabs().get(index) else {
            return;
        };
        self.net.cancel(closing.id, closing.generation);
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let geometry = self.geometry;
        self.tabs.close(index, || FreshTab {
            generation,
            painted: start_page_for(content_viewport(geometry)),
            message: STARTUP_HINT.to_string(),
        });
        // A different tab is in front now, and its load may be waiting to be rendered.
        self.activate_current();
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
            apply_rendered_page(tab, page, width, rows);
            update_load_message(tab);
        }
        tab.render_dirty = false;
    }
}
