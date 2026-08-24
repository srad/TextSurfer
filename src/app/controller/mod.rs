mod actions;
mod delivery;
mod navigation;
mod session;
#[cfg(test)]
mod tests;
mod view;
mod viewport;

use std::sync::Arc;
use std::time::Duration;

use crate::core::focus::Focus;
use crate::core::geom::Size;
use crate::core::style::TextRendering;
use crate::ui::editing::EditBuffer;
use crate::ui::mouse::ChromeGeometry;

use super::net::{Navigate, NoopNet};
use super::startpage::start_page_for;
use super::tabs::TabManager;
use delivery::{apply_rendered_page, update_load_message};
use viewport::content_viewport;

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };
const STARTUP_HINT: &str = "type a URL and press Enter";

pub struct App {
    focus: Focus,
    tabs: TabManager,
    address: EditBuffer,
    dirty: bool,
    quit: bool,
    generation: u64,
    geometry: ChromeGeometry,
    net: Arc<dyn Navigate>,
    menu_open: bool,
    menu_active: usize,
    menu_item: usize,
    focus_before_menu: Focus,
    now: Duration,
    text_rendering: TextRendering,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_net(Arc::new(NoopNet))
    }

    pub fn with_net(net: Arc<dyn Navigate>) -> Self {
        Self::with_net_and_rendering(net, TextRendering::Cell)
    }

    pub fn with_net_and_rendering(net: Arc<dyn Navigate>, text_rendering: TextRendering) -> Self {
        let geometry = ChromeGeometry::for_size(DEFAULT_SIZE);
        Self {
            focus: Focus::Address,
            tabs: TabManager::new(
                start_page_for(content_viewport(geometry)),
                STARTUP_HINT.to_string(),
            ),
            address: EditBuffer::new(),
            dirty: true,
            quit: false,
            generation: 0,
            geometry,
            net,
            menu_open: false,
            menu_active: 0,
            menu_item: 0,
            focus_before_menu: Focus::Address,
            now: Duration::ZERO,
            text_rendering,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn message(&self) -> &str {
        &self.tabs.active().message
    }

    pub fn active_url(&self) -> &str {
        &self.tabs.active().url
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}
