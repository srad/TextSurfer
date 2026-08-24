use crate::core::geom::Size;
use crate::pipeline::render::paint_document;
use crate::ui::mouse::ChromeGeometry;
use crate::ui::theme::NORTON;

use super::super::startpage::start_page_for;
use super::{App, apply_rendered_page};

const RESIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(50);

impl App {
    pub fn on_resize(&mut self, size: Size) {
        if self.geometry.size == size && self.pending_resize.is_none() {
            return;
        }
        self.pending_resize = None;
        self.resize_settled(size);
    }

    pub(super) fn preview_resize(&mut self, size: Size) {
        if self.geometry.size == size {
            self.pending_resize = Some((size, self.now.saturating_add(RESIZE_SETTLE)));
            return;
        }
        self.geometry = ChromeGeometry::for_size(size);
        let rows = self.geometry.content_rows();
        for tab in self.tabs.tabs_mut() {
            tab.scroll = tab.scroll.min(tab.painted.len().saturating_sub(rows));
        }
        self.pending_resize = Some((size, self.now.saturating_add(RESIZE_SETTLE)));
        self.touch();
    }

    pub(super) fn settle_resize(&mut self, now: std::time::Duration) {
        let Some((size, deadline)) = self.pending_resize else {
            return;
        };
        if now < deadline {
            return;
        }
        self.pending_resize = None;
        self.resize_settled(size);
    }

    fn resize_settled(&mut self, size: Size) {
        self.geometry = ChromeGeometry::for_size(size);
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let viewport = Size {
            cols: width.min(usize::from(u16::MAX)) as u16,
            rows: rows.min(usize::from(u16::MAX)) as u16,
        };
        let active = self.tabs.active_index();
        for (index, tab) in self.tabs.tabs_mut().iter_mut().enumerate() {
            if tab.url.is_empty() || tab.url == "about:blank" {
                tab.painted = start_page_for(viewport);
            } else if let Some(load) = tab.load.as_mut() {
                if index == active {
                    if let Some(page) = load.resize(viewport) {
                        apply_rendered_page(tab, page, width, rows);
                    }
                } else {
                    load.set_viewport(viewport);
                    tab.render_dirty = true;
                }
            } else if tab.layout_width != width
                && let (Some(document), Some(styles)) = (&tab.document, &tab.styles)
            {
                tab.painted =
                    paint_document(&document.borrow(), styles, viewport, NORTON.palette());
            }
            tab.layout_width = width;
            tab.scroll = tab.scroll.min(tab.painted.len().saturating_sub(rows));
        }
        self.refresh_hover();
        self.touch();
    }

    pub(super) fn max_scroll(&self) -> usize {
        let rows = self.geometry.content_rows();
        self.tabs.active().painted.len().saturating_sub(rows)
    }

    pub(super) fn page_step(&self) -> i32 {
        let rows = self.geometry.content_rows().saturating_sub(1).max(1);
        i32::try_from(rows).unwrap_or(i32::MAX)
    }

    pub(super) fn scroll(&mut self, delta: i32) {
        let max = self.max_scroll();
        let current = self.tabs.active().scroll;
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current.saturating_add(delta as usize).min(max)
        };
        if next == current {
            return;
        }
        self.tabs.active_mut().scroll = next;
        self.damage.scroll(scroll_delta(current, next));
        self.refresh_hover();
    }

    pub(super) fn set_scroll(&mut self, requested: usize) {
        let max = self.max_scroll();
        let current = self.tabs.active().scroll;
        let next = requested.min(max);
        if next == current {
            return;
        }
        self.tabs.active_mut().scroll = next;
        self.damage.scroll(scroll_delta(current, next));
        self.refresh_hover();
    }
}

fn scroll_delta(current: usize, next: usize) -> i32 {
    if next >= current {
        i32::try_from(next - current).unwrap_or(i32::MAX)
    } else {
        -i32::try_from(current - next).unwrap_or(i32::MAX)
    }
}

pub(super) fn content_viewport(geometry: ChromeGeometry) -> Size {
    Size {
        cols: geometry.content_cols().min(usize::from(u16::MAX)) as u16,
        rows: geometry.content_rows().min(usize::from(u16::MAX)) as u16,
    }
}
