use std::time::Duration;

use url::Url;

use crate::core::geom::Size;
use crate::core::style::RenderContext;
use crate::net::{FetchPayload, FetchPoll, ResourceId, charset_from_content_type, decode_text};
use crate::paint::DisplayList;
use crate::pipeline::image::{ImageDecodeJob, ImageDecodePayload, ImageDecodePoll, ImageSubmitted};
use crate::pipeline::page_load::{PageLoad, PageLoadOptions, PendingPageLoad};
use crate::pipeline::render::{
    PaintBaseline, PaintUpdate, RenderCause, RenderKey, RenderPoll, RenderSubmitted, RenderedPage,
    ResponseKind, response_kind,
};

use super::super::tab::Tab;
use super::App;

const FETCH_RESULTS_PER_STEP: usize = 8;
const LOAD_POLL: Duration = Duration::from_millis(8);
const RENDER_POLL: Duration = Duration::from_millis(8);
const PARSE_POLL: Duration = Duration::from_millis(1);
const PARSE_BYTES_PER_STEP: usize = 64 * 1024;

#[derive(Clone, Copy, Default)]
pub(super) struct RenderAdvance {
    pub(super) published: bool,
    pub(super) painted_changed: bool,
}

impl RenderAdvance {
    fn union(self, other: Self) -> Self {
        Self {
            published: self.published || other.published,
            painted_changed: self.painted_changed || other.painted_changed,
        }
    }
}

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
        let render = (self.render_inflight.is_some()
            || self.render_retry.is_some()
            || self
                .tabs
                .active()
                .load
                .as_ref()
                .is_some_and(PageLoad::has_render_work))
        .then_some(self.now.saturating_add(RENDER_POLL));
        let parse = self
            .tabs
            .tabs()
            .iter()
            .any(|tab| tab.pending_load.is_some())
            .then_some(self.now.saturating_add(PARSE_POLL));
        let script = self
            .tabs
            .tabs()
            .iter()
            .filter_map(|tab| {
                let load = tab.load.as_ref()?;
                load.scripts_runnable_at(self.now)
                    .then_some(self.now)
                    .or_else(|| load.next_script_deadline())
            })
            .min();
        [
            load,
            render,
            parse,
            script,
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
        let parsed_page_changed = self.advance_pending_parse();
        let scripted_page_changed = self.advance_script_work();
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
        let display_changed = self.advance_render_queue()
            || display_changed
            || parsed_page_changed
            || scripted_page_changed;
        if display_changed {
            self.refresh_hover();
            self.touch();
        }
        let progress_tick = now.as_millis() / 100;
        if progress_tick != self.progress_tick {
            self.progress_tick = progress_tick;
            if self.load_progress().is_some() {
                self.touch_status();
            }
        }
    }

    fn advance_pending_parse(&mut self) -> bool {
        let active_index = self.tabs.active_index();
        let Some(index) = self
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .find(|(index, tab)| *index == active_index && tab.pending_load.is_some())
            .or_else(|| {
                self.tabs
                    .tabs()
                    .iter()
                    .enumerate()
                    .find(|(_, tab)| tab.pending_load.is_some())
            })
            .map(|(index, _)| index)
        else {
            return false;
        };
        self.advance_pending_parse_at(index)
    }

    fn advance_pending_parse_at(&mut self, index: usize) -> bool {
        let active_index = self.tabs.active_index();
        let completed = self.tabs.tabs_mut()[index]
            .pending_load
            .as_mut()
            .and_then(|pending| pending.step(PARSE_BYTES_PER_STEP, self.now));
        let Some(mut load) = completed else {
            self.touch_status();
            return false;
        };
        let tab_id = self.tabs.tabs()[index].id;
        let generation = self.tabs.tabs()[index].generation;
        tracing::trace!(
            target: "textsurfer::perf",
            tab_id,
            generation,
            parse_errors = load.parse_errors(),
            stylesheets = load.external_occurrences(),
            "document parse completed"
        );
        self.tabs.tabs_mut()[index].pending_load = None;
        self.tabs.tabs_mut()[index].base = Some(load.base_url().clone());
        if let Some(url) = load.immediate_refresh().cloned() {
            self.tabs.tabs_mut()[index].load = Some(load);
            self.tabs.tabs_mut()[index].message = format!("redirecting to {url}");
            self.follow_declarative_refresh(tab_id, &url);
            return index == active_index;
        }
        let commands = load.take_commands();
        let image_commands = load.take_image_decode_commands();
        let cancel = load.take_cancel_requested();
        let _ = (index == active_index).then(|| load.render_if_ready(self.now));
        let title = load.page_title();
        if !title.is_empty() {
            self.tabs.tabs_mut()[index].title = title;
        }
        self.tabs.tabs_mut()[index].load = Some(load);
        self.tabs.tabs_mut()[index].render_dirty = index != active_index;
        let count = self.tabs.tabs()[index]
            .load
            .as_ref()
            .map_or(0, PageLoad::external_occurrences);
        self.tabs.tabs_mut()[index].message = format!(
            "loading {} ({count} stylesheets)",
            self.tabs.tabs()[index].url
        );
        for command in commands {
            let resource_id = command.resource_id;
            let submitted = self
                .net
                .submit(tab_id, generation, resource_id, command.url);
            if submitted == crate::net::Submitted::Closed {
                let _ = self.deliver_fetch(FetchPayload {
                    tab_id,
                    generation,
                    resource_id,
                    result: Err(crate::net::FetchError::Network(
                        "the network is not running".to_string(),
                    )),
                });
            }
        }
        for request in image_commands {
            let asset_id = request.asset_id;
            let revision = request.revision;
            if matches!(
                self.images.submit(ImageDecodeJob {
                    tab_id,
                    generation,
                    request,
                }),
                ImageSubmitted::Refused | ImageSubmitted::Closed
            ) {
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
        index == active_index
    }

    fn advance_script_work(&mut self) -> bool {
        let active_index = self.tabs.active_index();
        let Some(index) = self
            .tabs
            .tabs()
            .iter()
            .enumerate()
            .find(|(index, tab)| {
                *index == active_index
                    && tab
                        .load
                        .as_ref()
                        .is_some_and(|load| load.scripts_runnable_at(self.now))
            })
            .or_else(|| {
                self.tabs.tabs().iter().enumerate().find(|(_, tab)| {
                    tab.load
                        .as_ref()
                        .is_some_and(|load| load.scripts_runnable_at(self.now))
                })
            })
            .map(|(index, _)| index)
        else {
            return false;
        };
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let (page, title, message, navigation, commands, tab_id, generation) = {
            let tab = &mut self.tabs.tabs_mut()[index];
            let load = tab.load.as_mut().expect("selected tabs have a page load");
            let page = load.advance_scripts(self.now);
            let title = load.page_title();
            let message = load.take_script_message();
            let navigation = load.take_script_navigation();
            let commands = load.take_commands();
            (
                page,
                title,
                message,
                navigation,
                commands,
                tab.id,
                tab.generation,
            )
        };
        for command in commands {
            let resource_id = command.resource_id;
            let submitted = self
                .net
                .submit(tab_id, generation, resource_id, command.url);
            if submitted == crate::net::Submitted::Closed {
                let _ = self.deliver_fetch(FetchPayload {
                    tab_id,
                    generation,
                    resource_id,
                    result: Err(crate::net::FetchError::Network(
                        "the network is not running".to_string(),
                    )),
                });
            }
        }
        let had_message = message.is_some();
        let tab = &mut self.tabs.tabs_mut()[index];
        if !title.is_empty() {
            tab.title = title;
        }
        if let Some(message) = message {
            tab.message = message;
        }
        let mut changed = index == active_index && (page.is_some() || had_message);
        if let Some(page) = page {
            if index == active_index {
                apply_rendered_page(tab, page, width, rows);
            } else {
                tab.render_dirty = true;
            }
        }
        if index == active_index
            && let Some((target, kind)) = navigation
        {
            match kind {
                crate::script::NavigationKind::Reload => self.reload(),
                crate::script::NavigationKind::Push | crate::script::NavigationKind::Replace => {
                    let resolved = Url::parse(self.tabs.active().url.as_str())
                        .ok()
                        .and_then(|base| base.join(&target).ok())
                        .map_or(target, |url| url.to_string());
                    self.submit_url(&resolved);
                    if kind == crate::script::NavigationKind::Replace {
                        self.tabs.active_mut().replace_history(&resolved);
                    }
                }
            }
            changed = true;
        }
        changed
    }

    pub(super) fn advance_render_queue(&mut self) -> bool {
        self.advance_render_queue_result().published
    }

    pub(super) fn advance_render_queue_result(&mut self) -> RenderAdvance {
        let mut advance = self.poll_render_result();
        if self.render_inflight.is_none() {
            let (key, baseline) = {
                let active = self.tabs.active();
                (
                    RenderKey {
                        tab_id: active.id,
                        generation: active.generation,
                        epoch: active.load.as_ref().map_or(0, PageLoad::render_epoch),
                        hard_epoch: active.load.as_ref().map_or(0, PageLoad::hard_epoch),
                    },
                    active.display_revision.map(|revision| PaintBaseline {
                        key: revision,
                        painted: std::sync::Arc::clone(&active.painted),
                    }),
                )
            };
            let retry = self.render_retry.take();
            let retrying = retry.is_some();
            let job = retry.or_else(|| {
                self.tabs
                    .active_mut()
                    .load
                    .as_mut()
                    .and_then(|load| load.take_render_job(key))
            });
            if let Some(mut job) = job {
                if job.baseline.is_none() {
                    job.baseline = baseline;
                }
                let job_key = job.key;
                let causes = job.causes;
                match self.renders.submit(job) {
                    RenderSubmitted::Queued => {
                        tracing::trace!(
                            target: "textsurfer::perf",
                            tab_id = job_key.tab_id,
                            generation = job_key.generation,
                            epoch = job_key.epoch,
                            hard_epoch = job_key.hard_epoch,
                            causes = %causes,
                            retrying,
                            "render job submitted"
                        );
                        self.render_inflight = Some(job_key);
                        if !causes.contains(RenderCause::DynamicState) {
                            self.touch_status();
                        }
                    }
                    RenderSubmitted::Refused(job) => {
                        tracing::trace!(
                            target: "textsurfer::perf",
                            tab_id = job.key.tab_id,
                            generation = job.key.generation,
                            epoch = job.key.epoch,
                            causes = %job.causes,
                            "render job refused"
                        );
                        self.render_retry = Some(job);
                    }
                    RenderSubmitted::Closed(job) => {
                        tracing::trace!(
                            target: "textsurfer::perf",
                            tab_id = job.key.tab_id,
                            generation = job.key.generation,
                            epoch = job.key.epoch,
                            causes = %job.causes,
                            "render job lost"
                        );
                        self.report_render_lost();
                    }
                }
            }
        }
        advance = advance.union(self.poll_render_result());
        advance
    }

    fn poll_render_result(&mut self) -> RenderAdvance {
        let result = match self.renders.poll() {
            RenderPoll::Ready(result) => *result,
            RenderPoll::Empty => return RenderAdvance::default(),
            RenderPoll::Disconnected => {
                self.report_render_lost();
                return RenderAdvance::default();
            }
        };
        if self.render_inflight == Some(result.key) {
            self.render_inflight = None;
        }
        tracing::trace!(
            target: "textsurfer::perf",
            tab_id = result.key.tab_id,
            generation = result.key.generation,
            epoch = result.key.epoch,
            hard_epoch = result.key.hard_epoch,
            causes = %result.causes,
            cascade_ms = result.timings.cascade.as_millis(),
            restyle_ms = result.timings.restyle.as_millis(),
            layout_ms = result.timings.layout.as_millis(),
            paint_ms = result.timings.paint.as_millis(),
            "render result received"
        );
        let active_index = self.tabs.active_index();
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let Some((index, tab)) = self
            .tabs
            .find_load_mut(result.key.tab_id, result.key.generation)
        else {
            return RenderAdvance::default();
        };
        let dynamic_state = result.causes.contains(RenderCause::DynamicState);
        let result_key = result.key;
        if result
            .paint
            .base()
            .is_some_and(|base| tab.display_revision != Some(base))
            || !result.paint.applies_to(&tab.painted)
        {
            if let Some(load) = tab.load.as_mut() {
                load.reject_paint_result();
            }
            return RenderAdvance::default();
        }
        let Some(page) = tab
            .load
            .as_mut()
            .and_then(|load| load.apply_render_result(result))
        else {
            return RenderAdvance::default();
        };
        let applied = apply_rendered_page(tab, page, width, rows);
        if !dynamic_state {
            update_load_message(tab);
        }
        let (commands, image_commands) = tab.load.as_mut().map_or_else(
            || (Vec::new(), Vec::new()),
            |load| (load.take_commands(), load.take_image_decode_commands()),
        );
        let published = index == active_index;
        if published {
            for range in applied.rows {
                self.damage.repaint_rows(range);
            }
        }
        for command in commands {
            let resource_id = command.resource_id;
            if self.net.submit(
                result_key.tab_id,
                result_key.generation,
                resource_id,
                command.url,
            ) == crate::net::Submitted::Closed
            {
                let _ = self.deliver_fetch(FetchPayload {
                    tab_id: result_key.tab_id,
                    generation: result_key.generation,
                    resource_id,
                    result: Err(crate::net::FetchError::Network(
                        "the network is not running".to_string(),
                    )),
                });
            }
        }
        for request in image_commands {
            let asset_id = request.asset_id;
            let revision = request.revision;
            if matches!(
                self.images.submit(ImageDecodeJob {
                    tab_id: result_key.tab_id,
                    generation: result_key.generation,
                    request,
                }),
                ImageSubmitted::Refused | ImageSubmitted::Closed
            ) {
                let _ = self.deliver_image_decode(ImageDecodePayload {
                    tab_id: result_key.tab_id,
                    generation: result_key.generation,
                    asset_id,
                    revision,
                    result: Err(crate::core::image::ImageDecodeError::Unavailable),
                });
            }
        }
        RenderAdvance {
            published,
            painted_changed: published && applied.full,
        }
    }

    fn report_render_lost(&mut self) {
        if self.render_lost {
            return;
        }
        self.render_lost = true;
        self.render_inflight = None;
        self.render_retry = None;
        tracing::warn!("render worker disconnected");
        self.tabs.active_mut().message =
            "the renderer stopped responding - reopen TextSurfer".to_string();
        self.touch();
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
        let script_factory = self.script_factory.clone();
        let mut commands = Vec::new();
        let mut image_commands = Vec::new();
        let mut cancel = false;
        let mut visible_change;
        let mut active_display_changed = false;
        let accepted_index;
        let accepted = {
            let Some((index, tab)) = self.tabs.find_load_mut(tab_id, generation) else {
                return false;
            };
            accepted_index = index;
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
                        tab.response_note = status_note.clone();
                        let renders_body = response.is_success()
                            || (matches!(kind, ResponseKind::Html) && !response.body.is_empty());
                        if !renders_body {
                            let reason =
                                status_note.unwrap_or_else(|| "empty response".to_string());
                            tab.load = None;
                            tab.base = None;
                            tab.document = None;
                            tab.styles = None;
                            tab.painted = std::sync::Arc::new(DisplayList::from_lines(&[
                                format!("failed to load {}", tab.url),
                                String::new(),
                                format!("  {reason}"),
                            ]));
                            tab.display_revision = None;
                            tab.message = format!("{reason} - {}", tab.url);
                            active_display_changed = index == active_index;
                        } else {
                            match kind {
                                ResponseKind::Html => {
                                    let options = PageLoadOptions {
                                        render: RenderContext {
                                            viewport,
                                            metrics: self.render_metrics,
                                        },
                                        palette,
                                        scripting: script_factory.is_some(),
                                        color_scheme,
                                        started: self.now,
                                    };
                                    tab.load = None;
                                    tab.pending_load = Some(match script_factory.clone() {
                                        Some(factory) => PendingPageLoad::from_bytes_with_scripts(
                                            response.body,
                                            charset.as_deref(),
                                            response.final_url,
                                            options,
                                            factory,
                                        ),
                                        None => PendingPageLoad::from_bytes(
                                            response.body,
                                            charset.as_deref(),
                                            response.final_url,
                                            options,
                                        ),
                                    });
                                    tab.message = format!("parsing {}", tab.url);
                                    active_display_changed = index == active_index;
                                }
                                ResponseKind::PlainText => {
                                    let decoded = decode_text(&response.body, charset.as_deref());
                                    tab.load = None;
                                    tab.base = None;
                                    tab.document = None;
                                    tab.styles = None;
                                    tab.painted = std::sync::Arc::new(DisplayList::from_lines(
                                        &decoded
                                            .text
                                            .lines()
                                            .map(str::to_string)
                                            .collect::<Vec<_>>(),
                                    ));
                                    tab.display_revision = None;
                                    tab.message = format!("loaded {} (plain text)", tab.url);
                                    active_display_changed = index == active_index;
                                }
                                ResponseKind::Unsupported(content_type) => {
                                    tab.load = None;
                                    tab.base = None;
                                    tab.document = None;
                                    tab.styles = None;
                                    tab.painted = std::sync::Arc::new(DisplayList::from_lines(&[
                                        format!("cannot display {}", tab.url),
                                        String::new(),
                                        format!("  unsupported content type: {content_type}"),
                                    ]));
                                    tab.display_revision = None;
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
                        tab.painted = std::sync::Arc::new(DisplayList::from_lines(&[
                            format!("failed to load {}", tab.url),
                            String::new(),
                            format!("  {error}"),
                        ]));
                        tab.display_revision = None;
                        tab.message = format!("load failed: {error}");
                        active_display_changed = index == active_index;
                    }
                }
                true
            }
        };
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
        if accepted
            && resource_id == ResourceId::DOCUMENT
            && self.advance_pending_parse_at(accepted_index)
        {
            active_display_changed = true;
            visible_change = true;
        }
        if self.advance_render_queue() {
            active_display_changed = true;
            visible_change = true;
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

pub(super) struct PageApply {
    pub(super) full: bool,
    pub(super) rows: Vec<std::ops::Range<usize>>,
}

pub(super) fn apply_rendered_page(
    tab: &mut Tab,
    page: RenderedPage,
    width: usize,
    rows: usize,
) -> PageApply {
    let mut applied = PageApply {
        full: false,
        rows: Vec::new(),
    };
    match page.paint_update {
        Some(PaintUpdate::Unchanged { .. }) => {}
        Some(PaintUpdate::Patch { patch, .. }) => {
            applied.rows = patch.changed_rows();
            if !patch.apply(std::sync::Arc::make_mut(&mut tab.painted)) {
                applied.full = true;
            }
        }
        Some(PaintUpdate::Replace(_)) => unreachable!(),
        None => {
            tab.painted = std::sync::Arc::new(page.painted);
            applied.full = page.painted_changed;
        }
    }
    tab.display_revision = Some(page.revision);
    tab.layout_width = width;
    tab.document = Some(page.document);
    tab.styles = Some(page.styles);
    tab.render_dirty = false;
    tab.scroll = tab.scroll.min(tab.painted.len().saturating_sub(rows));
    applied
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
    let message = if notes.is_empty() {
        format!("loaded {}", tab.url)
    } else {
        format!("loaded {} ({})", tab.url, notes.join(", "))
    };
    tab.message = tab
        .response_note
        .as_ref()
        .map_or(message.clone(), |note| format!("{note} - {message}"));
}
