use std::time::Duration;

use crate::core::geom::Size;
use crate::core::style::RenderContext;
use crate::net::{
    FetchPayload, FetchPoll, ResourceId, charset_from_content_type, decode, decode_text,
};
use crate::paint::DisplayList;
use crate::pipeline::image::{ImageDecodeJob, ImageDecodePayload, ImageDecodePoll, ImageSubmitted};
use crate::pipeline::page_load::{PageLoad, PageLoadOptions};
use crate::pipeline::render::{RenderedPage, ResponseKind, response_kind};

use super::super::tab::Tab;
use super::App;

const FETCH_RESULTS_PER_STEP: usize = 256;
const LOAD_POLL: Duration = Duration::from_millis(50);

impl App {
    pub fn next_wake(&self) -> Option<Duration> {
        let load = self
            .tabs
            .tabs()
            .iter()
            .any(|tab| {
                tab.document_pending || tab.load.as_ref().is_some_and(|load| !load.is_settled())
            })
            .then_some(self.now.saturating_add(LOAD_POLL));
        [
            load,
            self.pending_resize.map(|(_, deadline)| deadline),
            self.dynamic_settle,
            self.flash_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn step(&mut self, now: Duration) {
        self.now = now;
        self.settle_resize(now);
        self.settle_dynamic_state(now);
        self.expire_flash(now);
        for _ in 0..FETCH_RESULTS_PER_STEP {
            match self.net.poll_result() {
                FetchPoll::Ready(payload) => {
                    let _ = self.deliver_fetch(payload);
                }
                FetchPoll::Empty => break,
                FetchPoll::Disconnected => {
                    self.report_net_lost();
                    break;
                }
            }
        }
        let mut image_payloads = Vec::new();
        let mut image_decode_lost = false;
        for _ in 0..FETCH_RESULTS_PER_STEP {
            match self.images.poll() {
                ImageDecodePoll::Ready(payload) => image_payloads.push(payload),
                ImageDecodePoll::Empty => break,
                ImageDecodePoll::Disconnected => {
                    image_decode_lost = true;
                    break;
                }
            }
        }
        self.deliver_image_decodes(image_payloads);
        if image_decode_lost {
            self.report_image_decode_lost();
        }
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let display_changed = {
            let active = self.tabs.active_mut();
            let page = active
                .load
                .as_mut()
                .and_then(|load| load.render_if_ready(now));
            if let Some(page) = page {
                apply_rendered_page(active, page, width, rows);
                update_load_message(active);
                true
            } else {
                false
            }
        };
        if display_changed {
            self.refresh_hover();
            self.touch();
        }
    }

    /// Every fetch worker is gone, so nothing pending can ever complete. Say so once,
    /// on every tab still waiting, instead of leaving them on "loading" forever.
    fn report_net_lost(&mut self) {
        if self.net_lost {
            return;
        }
        self.net_lost = true;
        tracing::warn!("fetch worker pool disconnected");
        for tab in self.tabs.tabs_mut() {
            let was_pending =
                tab.document_pending || tab.load.as_ref().is_some_and(|load| !load.is_settled());
            if let Some(load) = tab.load.as_mut() {
                load.fail_pending_image_fetches();
            }
            if was_pending {
                tab.document_pending = false;
                tab.message = "the network stopped responding - reopen TextSurfer".to_string();
            }
        }
        self.touch();
    }

    fn report_image_decode_lost(&mut self) {
        if self.image_decode_lost {
            return;
        }
        self.image_decode_lost = true;
        tracing::warn!("image decode worker disconnected");
        let active = self.tabs.active_index();
        let mut visible_change = false;
        for (index, tab) in self.tabs.tabs_mut().iter_mut().enumerate() {
            let changed = tab
                .load
                .as_mut()
                .is_some_and(PageLoad::fail_pending_image_decodes);
            if changed {
                update_load_message(tab);
                tab.render_dirty = index != active;
                visible_change |= index == active;
            }
        }
        if visible_change {
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
        let palette = self.theme().palette();
        let color_scheme = self.color_scheme();
        let mut commands = Vec::new();
        let mut image_commands = Vec::new();
        let mut cancel = false;
        let mut declarative_refresh = None;
        let mut visible_change;
        let mut active_display_changed = false;
        let accepted = {
            let Some((index, tab)) = self.tabs.find_load_mut(tab_id, generation) else {
                return false;
            };
            visible_change = resource_id == ResourceId::DOCUMENT && index == active_index;
            if resource_id != ResourceId::DOCUMENT {
                let Some(load) = tab.load.as_mut() else {
                    return false;
                };
                if !load.deliver(resource_id, payload.result) {
                    return false;
                }
                commands = load.take_commands();
                image_commands = load.take_image_decode_commands();
                cancel = load.take_cancel_requested();
                let page = (index == active_index)
                    .then(|| load.render_if_ready(self.now))
                    .flatten();
                if let Some(page) = page {
                    apply_rendered_page(tab, page, width, rows);
                    update_load_message(tab);
                    visible_change = true;
                    active_display_changed = true;
                } else if index != active_index {
                    tab.render_dirty = true;
                }
                true
            } else {
                tab.document_pending = false;
                match payload.result {
                    Ok(response) => {
                        let kind = response_kind(response.content_type.as_deref(), &response.body);
                        let charset = response
                            .content_type
                            .as_deref()
                            .and_then(charset_from_content_type);
                        tab.title = response.final_url.clone().into();
                        tab.url = response.final_url.clone().into();
                        // A 4xx/5xx page is rendered when the server sent a real HTML
                        // one, which is what a browser does. Anything else — an empty
                        // body, plain text, an unsupported type — is a failure the
                        // reader needs told about, so it falls through to the failure
                        // arms below rather than rendering bare.
                        let status_note =
                            (!response.is_success()).then(|| format!("HTTP {}", response.status));
                        let renders_body = response.is_success()
                            || (matches!(kind, ResponseKind::Html) && !response.body.is_empty());
                        if !renders_body {
                            let reason =
                                status_note.unwrap_or_else(|| "empty response".to_string());
                            tab.load = None;
                            tab.base = None;
                            tab.document = None;
                            tab.styles = None;
                            tab.painted = DisplayList::from_lines(&[
                                format!("failed to load {}", tab.url),
                                String::new(),
                                format!("  {reason}"),
                            ]);
                            tab.message = format!("{reason} - {}", tab.url);
                            active_display_changed = index == active_index;
                        } else {
                            match kind {
                                ResponseKind::Html => {
                                    let decoded = decode(&response.body, charset.as_deref());
                                    let mut load = PageLoad::new(
                                        &decoded.text,
                                        response.final_url,
                                        decoded.encoding,
                                        PageLoadOptions {
                                            render: RenderContext {
                                                viewport,
                                                metrics: self.render_metrics,
                                            },
                                            palette,
                                            scripting: false,
                                            color_scheme,
                                            started: self.now,
                                        },
                                    );
                                    tab.base = Some(load.base_url().clone());
                                    if let Some(url) = load.immediate_refresh().cloned() {
                                        tab.message = format!("redirecting to {url}");
                                        tab.load = Some(load);
                                        declarative_refresh = Some(url);
                                    } else {
                                        commands = load.take_commands();
                                        image_commands = load.take_image_decode_commands();
                                        cancel = load.take_cancel_requested();
                                        let page = (index == active_index)
                                            .then(|| load.render_if_ready(self.now))
                                            .flatten();
                                        tab.load = Some(load);
                                        if let Some(page) = page {
                                            apply_rendered_page(tab, page, width, rows);
                                            update_load_message(tab);
                                            // The server's own error page renders, but the
                                            // reader still has to know it is one.
                                            if let Some(note) = &status_note {
                                                tab.message = format!("{note} - {}", tab.message);
                                            }
                                            active_display_changed = true;
                                        } else {
                                            tab.render_dirty = index != active_index;
                                            let count = tab
                                                .load
                                                .as_ref()
                                                .map_or(0, PageLoad::external_occurrences);
                                            tab.message = format!(
                                                "loading {} ({count} stylesheets)",
                                                tab.url
                                            );
                                        }
                                    }
                                }
                                ResponseKind::PlainText => {
                                    let decoded = decode_text(&response.body, charset.as_deref());
                                    tab.load = None;
                                    tab.base = None;
                                    tab.document = None;
                                    tab.styles = None;
                                    tab.painted = DisplayList::from_lines(
                                        &decoded
                                            .text
                                            .lines()
                                            .map(str::to_string)
                                            .collect::<Vec<_>>(),
                                    );
                                    tab.message = format!("loaded {} (plain text)", tab.url);
                                    active_display_changed = index == active_index;
                                }
                                ResponseKind::Unsupported(content_type) => {
                                    tab.load = None;
                                    tab.base = None;
                                    tab.document = None;
                                    tab.styles = None;
                                    tab.painted = DisplayList::from_lines(&[
                                        format!("cannot display {}", tab.url),
                                        String::new(),
                                        format!("  unsupported content type: {content_type}"),
                                    ]);
                                    tab.message =
                                        format!("unsupported content type: {content_type}");
                                    active_display_changed = index == active_index;
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tab.load = None;
                        tab.base = None;
                        tab.document = None;
                        tab.styles = None;
                        tab.painted = DisplayList::from_lines(&[
                            format!("failed to load {}", tab.url),
                            String::new(),
                            format!("  {error}"),
                        ]);
                        tab.message = format!("load failed: {error}");
                        active_display_changed = index == active_index;
                    }
                }
                true
            }
        };
        if let Some(url) = declarative_refresh {
            self.follow_declarative_refresh(tab_id, &url);
            return accepted;
        }
        let mut closed_resources = Vec::new();
        for command in commands {
            let resource_id = command.resource_id;
            let submitted = self
                .net
                .submit(tab_id, generation, resource_id, command.url);
            tracing::debug!(tab_id, generation, resource_id = resource_id.0, result = ?submitted, "resource fetch submitted");
            if submitted == crate::net::Submitted::Closed {
                closed_resources.push(resource_id);
            }
        }
        for resource_id in closed_resources {
            let _ = self.deliver_fetch(FetchPayload {
                tab_id,
                generation,
                resource_id,
                result: Err(crate::net::FetchError::Network(
                    "the network is not running".to_string(),
                )),
            });
        }
        for request in image_commands {
            let asset_id = request.asset_id;
            let revision = request.revision;
            let submitted = self.images.submit(ImageDecodeJob {
                tab_id,
                generation,
                request,
            });
            tracing::debug!(tab_id, generation, asset_id = asset_id.0, revision, result = ?submitted, "image decode submitted");
            if matches!(submitted, ImageSubmitted::Refused | ImageSubmitted::Closed) {
                let _ = self.deliver_image_decode(ImageDecodePayload {
                    tab_id,
                    generation,
                    asset_id,
                    revision,
                    result: Err(crate::core::image::ImageDecodeError::Unavailable),
                });
            }
        }
        if cancel {
            self.net.cancel(tab_id, generation);
        }
        if active_display_changed {
            self.refresh_hover();
        }
        if visible_change {
            self.touch();
        }
        accepted
    }

    pub fn deliver_image_decode(&mut self, payload: ImageDecodePayload) -> bool {
        self.deliver_image_decodes(std::iter::once(payload)) > 0
    }

    fn deliver_image_decodes(
        &mut self,
        payloads: impl IntoIterator<Item = ImageDecodePayload>,
    ) -> usize {
        let active_index = self.tabs.active_index();
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let mut accepted = 0;
        let mut active_changed = false;
        for payload in payloads {
            let Some((index, tab)) = self.tabs.find_load_mut(payload.tab_id, payload.generation)
            else {
                tracing::debug!(
                    tab_id = payload.tab_id,
                    generation = payload.generation,
                    asset_id = payload.asset_id.0,
                    revision = payload.revision,
                    "stale image decode discarded"
                );
                continue;
            };
            let Some(load) = tab.load.as_mut() else {
                tracing::debug!(
                    tab_id = payload.tab_id,
                    generation = payload.generation,
                    asset_id = payload.asset_id.0,
                    revision = payload.revision,
                    "image decode discarded without active load"
                );
                continue;
            };
            if !load.deliver_image_decode(payload.asset_id, payload.revision, payload.result) {
                tracing::debug!(
                    tab_id = payload.tab_id,
                    generation = payload.generation,
                    asset_id = payload.asset_id.0,
                    revision = payload.revision,
                    "image decode rejected by page load"
                );
                continue;
            }
            accepted += 1;
            if index == active_index {
                active_changed = true;
            } else {
                tab.render_dirty = true;
            }
        }
        if active_changed {
            let tab = self.tabs.active_mut();
            if let Some(load) = tab.load.as_mut() {
                if let Some(page) = load.render_after_image() {
                    apply_rendered_page(tab, page, width, rows);
                }
                update_load_message(tab);
            }
            self.refresh_hover();
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

/// What layout could not do for this page.
fn layout_note(tab: &Tab) -> Option<&'static str> {
    let limits = tab.painted.limits;
    if limits.engine_failed {
        Some("layout failed")
    } else if limits.truncated_depth {
        Some("nesting truncated")
    } else {
        None
    }
}

pub(super) fn update_load_message(tab: &mut Tab) {
    let Some(load) = tab.load.as_ref() else {
        return;
    };
    let failures = load.failed_resources();
    let mut notes = Vec::new();
    if failures > 0 {
        let suffix = if failures == 1 { "" } else { "s" };
        notes.push(format!("{failures} stylesheet{suffix} failed"));
    }
    if load.external_disabled() {
        notes.push("external CSS disabled".to_string());
    }
    let image_failures = load.image_failures();
    if image_failures.rate_limited > 0 {
        let suffix = if image_failures.rate_limited == 1 {
            ""
        } else {
            "s"
        };
        notes.push(format!(
            "{} image{suffix} rate-limited",
            image_failures.rate_limited
        ));
    }
    let format_failures = image_failures
        .unknown_format
        .saturating_add(image_failures.unsupported_format);
    if format_failures > 0 {
        let suffix = if format_failures == 1 { "" } else { "s" };
        notes.push(format!(
            "{format_failures} image format{suffix} unsupported or unrecognized"
        ));
    }
    let other_failures = image_failures
        .total()
        .saturating_sub(image_failures.rate_limited)
        .saturating_sub(format_failures);
    if other_failures > 0 {
        let suffix = if other_failures == 1 { "" } else { "s" };
        notes.push(format!("{other_failures} image{suffix} failed"));
    }
    if let Some(note) = layout_note(tab) {
        notes.push(note.to_string());
    }
    tab.message = if notes.is_empty() {
        format!("loaded {}", tab.url)
    } else {
        format!("loaded {} ({})", tab.url, notes.join(", "))
    };
}
