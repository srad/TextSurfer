use std::time::Duration;

use crate::core::geom::Size;
use crate::css::ColorScheme;
use crate::net::{FetchPayload, ResourceId, charset_from_content_type, decode, decode_text};
use crate::paint::DisplayList;
use crate::pipeline::page_load::{PageLoad, PageLoadOptions};
use crate::pipeline::render::{RenderedPage, ResponseKind, response_kind};
use crate::ui::theme::NORTON;

use super::super::tab::Tab;
use super::App;

impl App {
    pub fn step(&mut self, now: Duration) {
        self.now = now;
        while let Some(payload) = self.net.poll_result() {
            let _ = self.deliver_fetch(payload);
        }
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let active = self.tabs.active_mut();
        let page = active
            .load
            .as_mut()
            .and_then(|load| load.render_if_ready(now));
        if let Some(page) = page {
            let css_warnings = page.css_warnings;
            let parse_errors = page.parse_errors;
            apply_rendered_page(active, page, width, rows);
            update_load_message(active, parse_errors, css_warnings);
            self.touch();
        }
    }

    pub fn deliver_fetch(&mut self, payload: FetchPayload) -> bool {
        let tab_id = payload.tab_id;
        let generation = payload.generation;
        let resource_id = payload.resource_id;
        let active_index = self.tabs.active_index();
        let viewport = Size {
            cols: self.geometry.content_cols().min(usize::from(u16::MAX)) as u16,
            rows: self.geometry.content_rows().min(usize::from(u16::MAX)) as u16,
        };
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let mut commands = Vec::new();
        let mut cancel = false;
        let mut visible_change = resource_id == ResourceId::DOCUMENT;
        let accepted = {
            let Some((index, tab)) = self.tabs.find_load_mut(tab_id, generation) else {
                return false;
            };
            if resource_id != ResourceId::DOCUMENT {
                let Some(load) = tab.load.as_mut() else {
                    return false;
                };
                if !load.deliver(resource_id, payload.result) {
                    return false;
                }
                commands = load.take_commands();
                cancel = load.take_cancel_requested();
                let page = (index == active_index)
                    .then(|| load.render_if_ready(self.now))
                    .flatten();
                if let Some(page) = page {
                    let css_warnings = page.css_warnings;
                    let parse_errors = page.parse_errors;
                    apply_rendered_page(tab, page, width, rows);
                    update_load_message(tab, parse_errors, css_warnings);
                    visible_change = true;
                } else if index != active_index {
                    tab.render_dirty = true;
                }
                true
            } else {
                match payload.result {
                    Ok(response) => {
                        let kind = response_kind(response.content_type.as_deref());
                        let charset = response
                            .content_type
                            .as_deref()
                            .and_then(charset_from_content_type);
                        tab.title = response.final_url.clone().into();
                        tab.url = response.final_url.clone().into();
                        match kind {
                            ResponseKind::Html => {
                                let decoded = decode(&response.body, charset.as_deref());
                                let mut load = PageLoad::new(
                                    &decoded.text,
                                    response.final_url,
                                    decoded.encoding,
                                    PageLoadOptions {
                                        viewport,
                                        palette: NORTON.palette(),
                                        scripting: false,
                                        color_scheme: ColorScheme::Dark,
                                        started: self.now,
                                    },
                                );
                                commands = load.take_commands();
                                cancel = load.take_cancel_requested();
                                let parse_errors = load.parse_errors();
                                let page = (index == active_index)
                                    .then(|| load.render_if_ready(self.now))
                                    .flatten();
                                tab.load = Some(load);
                                if let Some(page) = page {
                                    let css_warnings = page.css_warnings;
                                    apply_rendered_page(tab, page, width, rows);
                                    update_load_message(tab, parse_errors, css_warnings);
                                } else {
                                    tab.render_dirty = index != active_index;
                                    let count =
                                        tab.load.as_ref().map_or(0, PageLoad::external_occurrences);
                                    tab.message =
                                        format!("loading {} ({count} stylesheets)", tab.url);
                                }
                            }
                            ResponseKind::PlainText => {
                                let decoded = decode_text(&response.body, charset.as_deref());
                                tab.load = None;
                                tab.document = None;
                                tab.styles = None;
                                tab.painted = DisplayList::from_lines(
                                    &decoded.text.lines().map(str::to_string).collect::<Vec<_>>(),
                                );
                                tab.message = format!(
                                    "accepted gen {} - {} (plain text)",
                                    generation, tab.url
                                );
                            }
                            ResponseKind::Unsupported(content_type) => {
                                tab.load = None;
                                tab.document = None;
                                tab.styles = None;
                                tab.painted = DisplayList::from_lines(&[
                                    format!("cannot display {}", tab.url),
                                    String::new(),
                                    format!("  unsupported content type: {content_type}"),
                                ]);
                                tab.message = format!("unsupported content type: {content_type}");
                            }
                        }
                    }
                    Err(error) => {
                        tab.load = None;
                        tab.document = None;
                        tab.styles = None;
                        tab.painted = DisplayList::from_lines(&[
                            format!("failed to load {}", tab.url),
                            String::new(),
                            format!("  {error}"),
                        ]);
                        tab.message = format!("accepted gen {generation} - load failed: {error}");
                    }
                }
                true
            }
        };
        for command in commands {
            self.net
                .submit(tab_id, generation, command.resource_id, command.url);
        }
        if cancel {
            self.net.cancel(tab_id, generation);
        }
        if visible_change {
            self.touch();
        }
        accepted
    }
}

pub(super) fn apply_rendered_page(tab: &mut Tab, page: RenderedPage, width: usize, rows: usize) {
    tab.painted = page.painted;
    tab.layout_width = width;
    tab.document = Some(page.document);
    tab.styles = Some(page.styles);
    tab.render_dirty = false;
    tab.scroll = tab.scroll.min(tab.painted.len().saturating_sub(rows));
}

pub(super) fn update_load_message(tab: &mut Tab, parse_errors: usize, css_warnings: usize) {
    let Some(load) = tab.load.as_ref() else {
        return;
    };
    let occurrences = load.external_occurrences();
    let failures = load.failed_resources();
    if occurrences == 0 && failures == 0 && !load.external_disabled() {
        tab.message = if css_warnings == 0 {
            format!(
                "accepted gen {} - {} ({} parse errors)",
                tab.generation, tab.url, parse_errors
            )
        } else {
            format!(
                "accepted gen {} - {} ({} parse errors, {} CSS warnings)",
                tab.generation, tab.url, parse_errors, css_warnings
            )
        };
        return;
    }
    let disabled = if load.external_disabled() {
        ", external CSS disabled"
    } else {
        ""
    };
    tab.message = format!(
        "accepted gen {} - {} ({} parse errors, {} CSS warnings, {} stylesheets, {} failed{})",
        tab.generation, tab.url, parse_errors, css_warnings, occurrences, failures, disabled
    );
}
