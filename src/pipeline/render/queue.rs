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
use crate::css::{BasicCascade, Cascade, MediaContext, StyleSheet};
use crate::layout::{BoxTree, TaffyLayoutEngine};
use crate::paint::{BasicPainter, DisplayList, Painter};

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
    pub sheets: Vec<StyleSheet>,
    pub forms: FormState,
    pub images: ImageResources,
    pub media: MediaContext,
    pub palette: Palette,
    pub layout: Option<BoxTree>,
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
    pub css_warnings: usize,
    pub timings: RenderTimings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderStage {
    Cascade,
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

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderTimings {
    pub cascade: Duration,
    pub layout: Duration,
    pub paint: Duration,
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

impl RenderJob {
    pub fn execute(self) -> RenderResult {
        execute(self, None)
    }
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

fn execute(job: RenderJob, activity: Option<&Mutex<Option<ActiveRender>>>) -> RenderResult {
    let RenderJob {
        key,
        causes,
        document,
        styles,
        sheets,
        forms,
        images,
        media,
        palette,
        layout,
        css_warnings,
    } = job;
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
        stylesheets = sheets.len(),
        images = images.iter().count()
    );
    let _render_guard = render_span.enter();
    if styles.is_none() {
        set_activity(activity, key, RenderStage::Cascade);
    }
    let started = Instant::now();
    let styles = {
        let span = tracing::trace_span!(
            target: "textsurfer::perf",
            "cascade",
            reused = styles.is_some()
        );
        let _guard = span.enter();
        styles.unwrap_or_else(|| {
            Arc::new(BasicCascade.apply_with_form_state(&sheets, &document.document, media, &forms))
        })
    };
    let cascade = started.elapsed();
    if layout.is_none() {
        set_activity(activity, key, RenderStage::Layout);
    }
    let started = Instant::now();
    let layout = {
        let span = tracing::trace_span!(
            target: "textsurfer::perf",
            "layout",
            reused = layout.is_some()
        );
        let _guard = span.enter();
        layout.unwrap_or_else(|| {
            TaffyLayoutEngine.layout_with_images(
                &document.document,
                styles.as_ref(),
                media.viewport,
                &forms,
                &images,
                media.cell_metric,
            )
        })
    };
    let layout_time = started.elapsed();
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
        css_warnings,
        timings: RenderTimings {
            cascade,
            layout: layout_time,
            paint,
        },
    };
    if let Some(activity) = activity {
        *activity.lock().unwrap() = None;
    }
    result
}

#[derive(Default)]
pub struct InlineRenderQueue {
    results: Mutex<VecDeque<RenderResult>>,
    closed: AtomicBool,
}

impl RenderQueue for InlineRenderQueue {
    fn submit(&self, job: RenderJob) -> RenderSubmitted {
        if self.closed.load(Ordering::Acquire) {
            return RenderSubmitted::Closed(job);
        }
        self.results.lock().unwrap().push_back(execute(job, None));
        RenderSubmitted::Queued
    }

    fn poll(&self) -> RenderPoll {
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .map_or(RenderPoll::Empty, |result| {
                RenderPoll::Ready(Box::new(result))
            })
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
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
            while let Ok(command) = command_rx.recv() {
                match command {
                    Command::Render(job) => {
                        let result = execute(*job, Some(&worker_activity));
                        if result_tx.send(result).is_err() {
                            break;
                        }
                    }
                    Command::Shutdown => break,
                }
            }
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
