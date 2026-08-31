mod actions;
mod delivery;
mod flash;
mod forms;
mod menu;
mod navigation;
mod pointer;
mod session;
mod state;
#[cfg(test)]
mod tests;
mod theme;
mod view;
mod viewport;

use std::sync::Arc;
use std::time::Duration;

use crate::core::event::{InputBatch, InputEvent};
use crate::core::focus::Focus;
use crate::core::frame::{FRAME_INTERVAL, FrameDamage};
use crate::core::geom::{Point, Size};
use crate::core::style::{RenderMetrics, TextRendering};
use crate::pipeline::image::{ImageDecodePool, ImageDecodeQueue, RasterImageDecoder};
#[cfg(test)]
use crate::pipeline::render::BlockingRenderQueue;
#[cfg(not(test))]
use crate::pipeline::render::ThreadedRenderQueue;
use crate::pipeline::render::{RenderJob, RenderKey, RenderQueue};
use crate::ui::mouse::ChromeGeometry;
use crate::ui::widgets::text_field::{ClipboardAction, TextFieldState};

use super::net::{Navigate, NoopNet};
use super::startpage::start_page_for;
use super::tabs::TabManager;
use delivery::apply_rendered_page;
use flash::FlashNotice;
use menu::MainMenuState;
use pointer::{HoverTarget, PressedTarget, ScrollDrag, TextFieldContext, TextFieldTarget};
use viewport::content_viewport;

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };
const STARTUP_HINT: &str = "type a URL and press Enter";

pub struct App {
    focus: Focus,
    tabs: TabManager,
    address: TextFieldState,
    clipboard_request: Option<ClipboardAction>,
    damage: FrameDamage,
    quit: bool,
    generation: u64,
    geometry: ChromeGeometry,
    net: Arc<dyn Navigate>,
    images: Arc<dyn ImageDecodeQueue>,
    renders: Arc<dyn RenderQueue>,
    render_inflight: Option<RenderKey>,
    render_retry: Option<RenderJob>,
    main_menu: MainMenuState,
    theme_index: usize,
    focus_before_menu: Focus,
    now: Duration,
    render_metrics: RenderMetrics,
    pointer: Option<Point>,
    hover: Option<HoverTarget>,
    pressed: Option<PressedTarget>,
    scroll_drag: Option<ScrollDrag>,
    text_context: Option<TextFieldContext>,
    text_drag: Option<TextFieldTarget>,
    input_transaction: bool,
    dynamic_pending: bool,
    pending_resize: Option<(Size, Duration)>,
    dynamic_settle: Option<Duration>,
    net_lost: bool,
    image_decode_lost: bool,
    render_lost: bool,
    screenshot_request: bool,
    flash: Option<FlashNotice>,
    progress_tick: u128,
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
        Self::with_net_and_metrics(net, RenderMetrics::TERMINAL)
    }

    pub fn with_net_and_rendering(net: Arc<dyn Navigate>, text_rendering: TextRendering) -> Self {
        Self::with_net_and_metrics(
            net,
            RenderMetrics {
                cell: crate::core::style::CellMetric::DEFAULT,
                text: text_rendering,
            },
        )
    }

    pub fn with_net_and_metrics(net: Arc<dyn Navigate>, render_metrics: RenderMetrics) -> Self {
        Self::with_net_metrics_and_images(
            net,
            render_metrics,
            Arc::new(ImageDecodePool::new(Arc::new(RasterImageDecoder))),
        )
    }

    pub fn with_net_and_render_queue(
        net: Arc<dyn Navigate>,
        renders: Arc<dyn RenderQueue>,
    ) -> Self {
        Self::with_net_metrics_images_and_renders(
            net,
            RenderMetrics::TERMINAL,
            Arc::new(ImageDecodePool::new(Arc::new(RasterImageDecoder))),
            renders,
        )
    }

    pub(crate) fn with_net_metrics_and_images(
        net: Arc<dyn Navigate>,
        render_metrics: RenderMetrics,
        images: Arc<dyn ImageDecodeQueue>,
    ) -> Self {
        #[cfg(test)]
        let renders: Arc<dyn RenderQueue> = Arc::new(BlockingRenderQueue::default());
        #[cfg(not(test))]
        let renders: Arc<dyn RenderQueue> = Arc::new(ThreadedRenderQueue::new());
        Self::with_net_metrics_images_and_renders(net, render_metrics, images, renders)
    }

    fn with_net_metrics_images_and_renders(
        net: Arc<dyn Navigate>,
        render_metrics: RenderMetrics,
        images: Arc<dyn ImageDecodeQueue>,
        renders: Arc<dyn RenderQueue>,
    ) -> Self {
        let geometry = ChromeGeometry::for_size(DEFAULT_SIZE);
        Self {
            focus: Focus::Address,
            tabs: TabManager::new(
                start_page_for(content_viewport(geometry)),
                STARTUP_HINT.to_string(),
            ),
            address: TextFieldState::new(),
            clipboard_request: None,
            damage: FrameDamage::full(),
            quit: false,
            generation: 0,
            geometry,
            net,
            images,
            renders,
            render_inflight: None,
            render_retry: None,
            main_menu: MainMenuState::default(),
            theme_index: crate::ui::theme::DEFAULT_THEME_INDEX,
            focus_before_menu: Focus::Address,
            now: Duration::ZERO,
            render_metrics,
            pointer: None,
            hover: None,
            pressed: None,
            scroll_drag: None,
            text_context: None,
            text_drag: None,
            input_transaction: false,
            dynamic_pending: false,
            pending_resize: None,
            dynamic_settle: None,
            net_lost: false,
            image_decode_lost: false,
            render_lost: false,
            screenshot_request: false,
            flash: None,
            progress_tick: 0,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Release the fetch pool without joining it. A worker parked in the 30 s fetch
    /// timeout would otherwise hold the process open through `FetchPool::drop`.
    pub fn shutdown_net(&self) {
        self.net.shutdown();
        self.images.shutdown();
        self.renders.shutdown();
    }

    pub fn take_dirty(&mut self) -> bool {
        !self.take_damage().is_empty()
    }

    pub fn take_damage(&mut self) -> FrameDamage {
        std::mem::take(&mut self.damage)
    }

    pub fn process_input(&mut self, batch: &InputBatch) {
        let defer_dynamic = !batch.is_empty()
            && batch.as_slice().iter().all(|event| {
                matches!(
                    event,
                    InputEvent::Mouse(crate::core::event::MouseEvent {
                        kind: crate::core::event::MouseKind::Move
                            | crate::core::event::MouseKind::Wheel { .. },
                        ..
                    }) | InputEvent::Resize { .. }
                        | InputEvent::PointerLeft
                )
            });
        self.input_transaction = true;
        for event in batch.as_slice() {
            match event {
                InputEvent::Key(key) => self.handle_key(key.clone()),
                InputEvent::Paste(text) => self.deliver_clipboard_text(text),
                InputEvent::Mouse(mouse) => self.handle_mouse(*mouse),
                InputEvent::Resize { size, phase } => match phase {
                    crate::core::event::ResizePhase::Preview => self.preview_resize(*size),
                    crate::core::event::ResizePhase::Settled => self.on_resize(*size),
                },
                InputEvent::PointerLeft => self.pointer_left(),
            }
        }
        self.input_transaction = false;
        if self.dynamic_pending {
            if defer_dynamic {
                if self.dynamic_settle.is_none() {
                    self.dynamic_settle = Some(self.now.saturating_add(FRAME_INTERVAL));
                }
            } else {
                self.dynamic_pending = false;
                self.dynamic_settle = None;
                if self.sync_dynamic_state() {
                    self.touch();
                }
            }
        }
        if self.advance_render_queue() {
            self.refresh_hover();
            self.touch();
        }
    }

    pub fn advance(&mut self, batch: &InputBatch, now: Duration) {
        self.now = now;
        self.process_input(batch);
        self.step(now);
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
