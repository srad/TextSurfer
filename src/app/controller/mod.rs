mod actions;
mod delivery;
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
use crate::core::frame::FrameDamage;
use crate::core::geom::{Point, Size};
use crate::core::style::{RenderMetrics, TextRendering};
use crate::pipeline::image::{ImageDecodePool, ImageDecodeQueue, RasterImageDecoder};
use crate::ui::editing::EditBuffer;
use crate::ui::mouse::ChromeGeometry;

use super::net::{Navigate, NoopNet};
use super::startpage::start_page_for;
use super::tabs::TabManager;
use delivery::{apply_rendered_page, update_load_message};
use pointer::{HoverTarget, PressedTarget, ScrollDrag};
use viewport::content_viewport;

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };
const STARTUP_HINT: &str = "type a URL and press Enter";

pub struct App {
    focus: Focus,
    tabs: TabManager,
    address: EditBuffer,
    damage: FrameDamage,
    quit: bool,
    generation: u64,
    geometry: ChromeGeometry,
    net: Arc<dyn Navigate>,
    images: Arc<dyn ImageDecodeQueue>,
    menu_open: bool,
    menu_active: usize,
    menu_item: usize,
    theme_index: usize,
    focus_before_menu: Focus,
    now: Duration,
    render_metrics: RenderMetrics,
    pointer: Option<Point>,
    hover: Option<HoverTarget>,
    pressed: Option<PressedTarget>,
    scroll_drag: Option<ScrollDrag>,
    input_transaction: bool,
    dynamic_pending: bool,
    pending_resize: Option<(Size, Duration)>,
    dynamic_settle: Option<Duration>,
    net_lost: bool,
    image_decode_lost: bool,
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

    pub(crate) fn with_net_metrics_and_images(
        net: Arc<dyn Navigate>,
        render_metrics: RenderMetrics,
        images: Arc<dyn ImageDecodeQueue>,
    ) -> Self {
        let geometry = ChromeGeometry::for_size(DEFAULT_SIZE);
        Self {
            focus: Focus::Address,
            tabs: TabManager::new(
                start_page_for(content_viewport(geometry)),
                STARTUP_HINT.to_string(),
            ),
            address: EditBuffer::new(),
            damage: FrameDamage::full(),
            quit: false,
            generation: 0,
            geometry,
            net,
            images,
            menu_open: false,
            menu_active: 0,
            menu_item: 0,
            theme_index: crate::ui::theme::DEFAULT_THEME_INDEX,
            focus_before_menu: Focus::Address,
            now: Duration::ZERO,
            render_metrics,
            pointer: None,
            hover: None,
            pressed: None,
            scroll_drag: None,
            input_transaction: false,
            dynamic_pending: false,
            pending_resize: None,
            dynamic_settle: None,
            net_lost: false,
            image_decode_lost: false,
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
                self.dynamic_settle = Some(self.now.saturating_add(Duration::from_millis(50)));
            } else {
                self.dynamic_pending = false;
                self.dynamic_settle = None;
                if self.sync_dynamic_state() {
                    self.touch();
                }
            }
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
