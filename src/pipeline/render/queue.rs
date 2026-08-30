use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};

use crate::core::dom::Document;
use crate::core::form::FormState;
use crate::core::image::ImageResources;
use crate::core::style::{Palette, StyleTree};
use crate::css::MediaContext;
use crate::layout::{BoxTree, TaffyLayoutEngine};
use crate::paint::{BasicPainter, DisplayList, Painter};

use super::StyleInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderKey {
    pub tab_id: u64,
    pub generation: u64,
    pub epoch: u64,
    pub hard_epoch: u64,
}

pub struct RenderJob {
    pub key: RenderKey,
    pub causes: RenderCauses,
    pub document: RenderTree,
    pub styles: Option<Arc<StyleTree>>,
    pub(crate) previous_styles: Option<Arc<StyleTree>>,
    pub style_input: StyleInput,
    pub forms: FormState,
    pub images: ImageResources,
    pub media: MediaContext,
    pub palette: Palette,
    pub layout: Option<BoxTree>,
    pub layout_styles: Option<Arc<StyleTree>>,
    pub css_warnings: usize,
}

pub struct RenderTree {
    document: Document,
}

impl RenderTree {
    pub(crate) fn from_document(document: &Document) -> Self {
        Self {
            document: document.clone(),
        }
    }
}

pub struct RenderResult {
    pub key: RenderKey,
    pub causes: RenderCauses,
    pub layout: BoxTree,
    pub styles: Arc<StyleTree>,
    pub painted: DisplayList,
    pub painted_changed: bool,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "asserted by the page_load tests and traced at the failure site; no \
            production consumer reads it back off the result yet"
        )
    )]
    pub(crate) restyle_failure: Option<crate::layout::RestyleFailure>,
    pub css_warnings: usize,
    pub timings: RenderTimings,
    pub work: RenderWork,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderStage {
    Cascade,
    Restyle,
    Layout,
    Paint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderActivity {
    pub key: RenderKey,
    pub stage: RenderStage,
    pub elapsed: Duration,
}

#[derive(Clone, Copy)]
struct ActiveRender {
    key: RenderKey,
    stage: RenderStage,
    started: Instant,
}

struct PreparedStyles {
    styles: Arc<StyleTree>,
    cascade: Duration,
    restyle: Duration,
    touched: Option<Vec<crate::core::dom::NodeId>>,
    work: RenderWork,
    css_warnings: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderTimings {
    pub cascade: Duration,
    pub restyle: Duration,
    pub layout: Duration,
    pub paint: Duration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderWork {
    pub styled_nodes_visited: usize,
    pub style_entries_mapped: usize,
    pub damage_nodes_checked: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RenderCause {
    Initial,
    Stylesheet,
    Image,
    Viewport,
    DynamicState,
    Theme,
    FormState,
    ResourceSettlement,
    Forced,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderCauses(u16);

impl RenderCauses {
    pub(crate) fn one(cause: RenderCause) -> Self {
        let mut causes = Self::default();
        causes.insert(cause);
        causes
    }

    pub(crate) fn insert(&mut self, cause: RenderCause) {
        self.0 |= 1 << cause as u8;
    }

    pub(crate) fn contains(self, cause: RenderCause) -> bool {
        self.0 & (1 << cause as u8) != 0
    }

    pub fn bits(self) -> u16 {
        self.0
    }
}

impl fmt::Debug for RenderCauses {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for RenderCauses {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for (cause, name) in [
            (RenderCause::Initial, "initial"),
            (RenderCause::Stylesheet, "stylesheet"),
            (RenderCause::Image, "image"),
            (RenderCause::Viewport, "viewport"),
            (RenderCause::DynamicState, "dynamic_state"),
            (RenderCause::Theme, "theme"),
            (RenderCause::FormState, "form_state"),
            (RenderCause::ResourceSettlement, "resource_settlement"),
            (RenderCause::Forced, "forced"),
        ] {
            if self.contains(cause) {
                formatter.write_str(separator)?;
                formatter.write_str(name)?;
                separator = "|";
            }
        }
        if separator.is_empty() {
            formatter.write_str("none")?;
        }
        Ok(())
    }
}

pub enum RenderSubmitted {
    Queued,
    Refused(RenderJob),
    Closed(RenderJob),
}

pub enum RenderPoll {
    Ready(Box<RenderResult>),
    Empty,
    Disconnected,
}

pub trait RenderQueue: Send + Sync {
    fn submit(&self, job: RenderJob) -> RenderSubmitted;
    fn poll(&self) -> RenderPoll;
    fn activity(&self) -> Option<RenderActivity> {
        None
    }
    fn shutdown(&self) {}
}

fn set_activity(
    activity: Option<&Mutex<Option<ActiveRender>>>,
    key: RenderKey,
    stage: RenderStage,
) {
    if let Some(activity) = activity {
        *activity.lock().unwrap() = Some(ActiveRender {
            key,
            stage,
            started: Instant::now(),
        });
    }
}

fn execute_inner(
    job: RenderJob,
    activity: Option<&Mutex<Option<ActiveRender>>>,
    prepared: Option<PreparedStyles>,
) -> RenderResult {
    let RenderJob {
        key,
        causes,
        document,
        styles,
        previous_styles: _,
        style_input: _,
        forms,
        images,
        media,
        palette,
        layout,
        layout_styles,
        css_warnings,
    } = job;
    let cascade_required = styles.is_none() || prepared.is_some();
    let render_span = tracing::trace_span!(
        target: "textsurfer::perf",
        "render",
        tab_id = key.tab_id,
        generation = key.generation,
        epoch = key.epoch,
        hard_epoch = key.hard_epoch,
        causes = %causes,
        causes_bits = causes.bits(),
        reused_styles = styles.is_some(),
        reused_layout = layout.is_some(),
        images = images.iter().count()
    );
    let _render_guard = render_span.enter();
    let mut incremental_style_time = Duration::ZERO;
    let mut touched = None;
    let mut work = RenderWork::default();
    let mut css_warnings = css_warnings;
    let (styles, cascade) = if let Some(prepared) = prepared {
        incremental_style_time = prepared.restyle;
        work = prepared.work;
        css_warnings = prepared.css_warnings.unwrap_or(css_warnings);
        touched = prepared.touched;
        (prepared.styles, prepared.cascade)
    } else if let Some(styles) = styles {
        (styles, Duration::ZERO)
    } else {
        unreachable!("the Stylo worker prepares every cascade")
    };
    let mut layout = layout;
    let mut restyle_time = incremental_style_time;
    let mut painted_changed = true;
    let mut restyle_failure = None;
    if cascade_required
        && let (Some(tree), Some(previous)) = (layout.as_mut(), layout_styles.as_ref())
    {
        set_activity(activity, key, RenderStage::Restyle);
        let started = Instant::now();
        let span = tracing::trace_span!(target: "textsurfer::perf", "restyle");
        let _guard = span.enter();
        let result = match touched.as_deref() {
            Some(nodes) => crate::layout::restyle_nodes(tree, previous, &styles, nodes),
            None => crate::layout::restyle(tree, previous, &styles),
        };
        match result {
            Ok(()) => {
                restyle_time = restyle_time.saturating_add(started.elapsed());
                painted_changed = !touched.as_deref().map_or_else(
                    || previous.paint_compatible_with(&styles),
                    |nodes| previous.paint_compatible_for(&styles, nodes),
                );
            }
            Err(failure) => {
                tracing::trace!(target: "textsurfer::perf", ?failure, "restyle rejected");
                restyle_failure = Some(failure);
                layout = None;
            }
        }
    }
    if layout.is_none() {
        set_activity(activity, key, RenderStage::Layout);
    }
    let (layout, layout_time) = if let Some(layout) = layout {
        (layout, Duration::ZERO)
    } else {
        let started = Instant::now();
        let span = tracing::trace_span!(
            target: "textsurfer::perf",
            "layout",
            reused = false
        );
        let _guard = span.enter();
        let layout = TaffyLayoutEngine.layout_with_images(
            &document.document,
            styles.as_ref(),
            media.viewport,
            &forms,
            &images,
            media.cell_metric,
        );
        (layout, started.elapsed())
    };
    set_activity(activity, key, RenderStage::Paint);
    let started = Instant::now();
    let painted = {
        let span = tracing::trace_span!(target: "textsurfer::perf", "paint");
        let _guard = span.enter();
        let mut painted = BasicPainter.paint(&layout, palette);
        for (_, image) in images.iter() {
            painted.image_assets.insert(image.asset_id, image.clone());
        }
        painted
    };
    let paint = started.elapsed();
    tracing::trace!(
        target: "textsurfer::perf",
        tab_id = key.tab_id,
        generation = key.generation,
        epoch = key.epoch,
        hard_epoch = key.hard_epoch,
        cascade_ms = cascade.as_millis(),
        restyle_ms = restyle_time.as_millis(),
        layout_ms = layout_time.as_millis(),
        paint_ms = paint.as_millis(),
        rows = painted.len(),
        image_placements = painted.images.len(),
        "render worker completed"
    );
    let result = RenderResult {
        key,
        causes,
        layout,
        styles,
        painted,
        painted_changed,
        restyle_failure,
        css_warnings,
        timings: RenderTimings {
            cascade,
            restyle: restyle_time,
            layout: layout_time,
            paint,
        },
        work,
    };
    if let Some(activity) = activity {
        *activity.lock().unwrap() = None;
    }
    result
}

pub struct BlockingRenderQueue {
    worker: ThreadedRenderQueue,
    results: Mutex<VecDeque<RenderResult>>,
    disconnected: AtomicBool,
}

impl BlockingRenderQueue {
    pub fn new() -> Self {
        Self {
            worker: ThreadedRenderQueue::new(),
            results: Mutex::new(VecDeque::new()),
            disconnected: AtomicBool::new(false),
        }
    }

    pub fn render(&self, job: RenderJob) -> Option<RenderResult> {
        match self.submit(job) {
            RenderSubmitted::Queued => {}
            RenderSubmitted::Refused(_) | RenderSubmitted::Closed(_) => return None,
        }
        match self.poll() {
            RenderPoll::Ready(result) => Some(*result),
            RenderPoll::Empty | RenderPoll::Disconnected => None,
        }
    }
}

impl Default for BlockingRenderQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderQueue for BlockingRenderQueue {
    fn submit(&self, job: RenderJob) -> RenderSubmitted {
        match self.worker.submit(job) {
            RenderSubmitted::Queued => {}
            refused => return refused,
        }
        loop {
            match self.worker.poll() {
                RenderPoll::Ready(result) => {
                    self.results.lock().unwrap().push_back(*result);
                    break;
                }
                RenderPoll::Empty => std::thread::yield_now(),
                RenderPoll::Disconnected => {
                    self.disconnected.store(true, Ordering::Release);
                    break;
                }
            }
        }
        RenderSubmitted::Queued
    }

    fn poll(&self) -> RenderPoll {
        let result = self
            .results
            .lock()
            .unwrap()
            .pop_front()
            .map_or(RenderPoll::Empty, |result| {
                RenderPoll::Ready(Box::new(result))
            });
        if matches!(result, RenderPoll::Empty) && self.disconnected.load(Ordering::Acquire) {
            RenderPoll::Disconnected
        } else {
            result
        }
    }

    fn shutdown(&self) {
        self.worker.shutdown();
    }
}

enum Command {
    Render(Box<RenderJob>),
    Shutdown,
}

pub struct ThreadedRenderQueue {
    commands: Sender<Command>,
    results: Receiver<RenderResult>,
    closed: Arc<AtomicBool>,
    activity: Arc<Mutex<Option<ActiveRender>>>,
}

impl ThreadedRenderQueue {
    pub fn new() -> Self {
        let (commands, command_rx) = crossbeam_channel::bounded(1);
        let (result_tx, results) = crossbeam_channel::bounded(1);
        let closed = Arc::new(AtomicBool::new(false));
        let worker_closed = closed.clone();
        let activity = Arc::new(Mutex::new(None));
        let worker_activity = activity.clone();
        std::thread::spawn(move || {
            worker_loop(command_rx, result_tx, &worker_activity);
            worker_closed.store(true, Ordering::Release);
        });
        Self {
            commands,
            results,
            closed,
            activity,
        }
    }
}

fn worker_loop(
    command_rx: Receiver<Command>,
    result_tx: Sender<RenderResult>,
    activity: &Mutex<Option<ActiveRender>>,
) {
    let mut pending = None;
    let mut stopped = false;
    while !stopped {
        let command = match pending.take() {
            Some(command) => command,
            None => match command_rx.recv() {
                Ok(command) => command,
                Err(_) => break,
            },
        };
        let Command::Render(first) = command else {
            break;
        };
        let input = first.style_input.clone();
        let document = first.document.document.clone();
        let forms = first.forms.clone();
        let media = first.media;
        crate::css::stylo::with_session(&input, &document, &forms, media, |session| {
            let mut current = first;
            loop {
                let result = execute_with_stylo_session(*current, session, Some(activity));
                if result_tx.send(result).is_err() {
                    stopped = true;
                    break;
                }
                match command_rx.recv() {
                    Ok(Command::Render(next)) if next.style_input.session == input.session => {
                        current = next;
                    }
                    Ok(command) => {
                        pending = Some(command);
                        break;
                    }
                    Err(_) => {
                        stopped = true;
                        break;
                    }
                }
            }
        });
    }
}

fn execute_with_stylo_session(
    job: RenderJob,
    session: &mut crate::css::stylo::StyloSession<'_, '_>,
    activity: Option<&Mutex<Option<ActiveRender>>>,
) -> RenderResult {
    if job.styles.is_some() {
        return execute_inner(job, activity, None);
    }
    if (job.causes.contains(RenderCause::DynamicState)
        || job.causes.contains(RenderCause::FormState))
        && job.previous_styles.is_some()
    {
        set_activity(activity, job.key, RenderStage::Restyle);
        let started = Instant::now();
        let restyled = session.restyle(
            job.media,
            job.previous_styles.as_deref().expect("checked above"),
            &job.forms,
        );
        let elapsed = started.elapsed();
        tracing::trace!(
            target: "textsurfer::perf",
            visited = restyled.visited,
            touched = restyled.touched.len(),
            mapped = restyled.mapped,
            "stylo restyle completed"
        );
        let damage_nodes_checked = restyled.touched.len();
        return execute_inner(
            job,
            activity,
            Some(PreparedStyles {
                styles: Arc::new(restyled.styles),
                cascade: Duration::ZERO,
                restyle: elapsed,
                touched: Some(restyled.touched),
                work: RenderWork {
                    styled_nodes_visited: restyled.visited,
                    style_entries_mapped: restyled.mapped,
                    damage_nodes_checked,
                },
                css_warnings: Some(session.css_warnings()),
            }),
        );
    }
    set_activity(activity, job.key, RenderStage::Cascade);
    let started = Instant::now();
    let styles = Arc::new(session.cascade(job.media));
    execute_inner(
        job,
        activity,
        Some(PreparedStyles {
            styles,
            cascade: started.elapsed(),
            restyle: Duration::ZERO,
            touched: None,
            work: RenderWork::default(),
            css_warnings: Some(session.css_warnings()),
        }),
    )
}

impl Default for ThreadedRenderQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderQueue for ThreadedRenderQueue {
    fn submit(&self, job: RenderJob) -> RenderSubmitted {
        if self.closed.load(Ordering::Acquire) {
            return RenderSubmitted::Closed(job);
        }
        match self.commands.try_send(Command::Render(Box::new(job))) {
            Ok(()) => RenderSubmitted::Queued,
            Err(TrySendError::Full(Command::Render(job))) => RenderSubmitted::Refused(*job),
            Err(TrySendError::Disconnected(Command::Render(job))) => RenderSubmitted::Closed(*job),
            Err(TrySendError::Full(Command::Shutdown))
            | Err(TrySendError::Disconnected(Command::Shutdown)) => unreachable!(),
        }
    }

    fn poll(&self) -> RenderPoll {
        match self.results.try_recv() {
            Ok(result) => RenderPoll::Ready(Box::new(result)),
            Err(TryRecvError::Empty) => RenderPoll::Empty,
            Err(TryRecvError::Disconnected) => RenderPoll::Disconnected,
        }
    }

    fn activity(&self) -> Option<RenderActivity> {
        self.activity
            .lock()
            .unwrap()
            .map(|activity| RenderActivity {
                key: activity.key,
                stage: activity.stage,
                elapsed: activity.started.elapsed(),
            })
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.commands.try_send(Command::Shutdown);
    }
}
