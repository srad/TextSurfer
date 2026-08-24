//! The window and its event loop — the only part of this module that does I/O.
//!
//! Kept as thin as `main.rs`'s terminal loop, and structured to preserve its timing.
//! The terminal frontend polls for 50 ms, then pumps `App::step`, which is what
//! delivers fetch results. A window event loop that simply waits for input would never
//! run that pump, and every page load would hang with the response sitting in the
//! channel — so the tick is reproduced with `ControlFlow::WaitUntil`.

use std::io;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

use crate::app::App;
use crate::app::net::Navigate;
use crate::core::event::{InputBatch, InputEvent, MouseEvent, MouseKind, ResizePhase};
use crate::core::frame::{FrameDamage, FrameScheduler, RowDamage};
use crate::core::geom::{Point, Size};
use crate::core::style::{Cursor, Rgb, TextRendering};
use crate::layout::LayoutRect;
use crate::ui::chrome;
use crate::ui::frame::FrameComposer;
use crate::ui::mouse::WHEEL_ROWS;
use crate::ui::theme::{NORTON, rgb_of};

use super::backend::{VgaBackend, cell_at, grid_for, pixel_size};
use super::font::{CELL_H, CELL_W};
use super::input::{WheelAccumulator, from_window_button, from_window_key};
use super::surface::SurfaceConfig;

/// How the framebuffer frontend should start up.
#[derive(Clone, Copy, Debug)]
pub struct VgaOptions {
    /// Integer pixel multiplier for the 8x16 cell.
    pub scale: usize,
    /// Initial window size, in cells.
    pub cells: Size,
}

impl Default for VgaOptions {
    fn default() -> Self {
        Self {
            scale: 1,
            // 1280x800 at 8x16 — about double a classic terminal's columns.
            cells: Size {
                cols: 160,
                rows: 50,
            },
        }
    }
}

/// Open a window and run the browser in it until the user quits.
pub fn run(net: Arc<dyn Navigate>, options: VgaOptions, url: Option<String>) -> io::Result<()> {
    let event_loop = EventLoop::new().map_err(into_io)?;
    let mut handler = VgaApp::new(net, options, url)?;
    event_loop.run_app(&mut handler).map_err(into_io)?;
    handler.failure.map_or(Ok(()), Err)
}

/// What one pump step wants the event loop to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Tick {
    /// The app asked to quit.
    pub quit: bool,
    /// Painted state changed and the window should be redrawn.
    pub redraw: bool,
}

/// The live window and its presentation surface, created once the loop resumes.
struct Presented {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    size: Option<PhysicalSize<u32>>,
    damage_history: Vec<Vec<softbuffer::Rect>>,
}

pub(super) struct VgaApp {
    backend: VgaBackend,
    composer: FrameComposer,
    pending_damage: FrameDamage,
    scheduler: FrameScheduler,
    pending_size: Option<Size>,
    app: App,
    options: VgaOptions,
    started: Instant,
    modifiers: ModifiersState,
    /// The cell the pointer was last seen in; `MouseInput` and `MouseWheel` carry no
    /// position of their own.
    pointer: Option<Point>,
    wheel: WheelAccumulator,
    cursor: CursorPresentation,
    background: Rgb,
    presented: Option<Presented>,
    /// The first error to escape a callback. `ApplicationHandler` cannot return one,
    /// so it is stashed here and surfaced by [`run`] after the loop exits.
    failure: Option<io::Error>,
}

impl VgaApp {
    pub(super) fn new(
        net: Arc<dyn Navigate>,
        options: VgaOptions,
        url: Option<String>,
    ) -> io::Result<Self> {
        let background = rgb_of(NORTON.bg);
        let backend = VgaBackend::new(SurfaceConfig {
            cols: options.cells.cols,
            rows: options.cells.rows,
            scale: options.scale,
            default_fg: rgb_of(NORTON.text),
            default_bg: background,
        });
        let mut app = App::with_net_and_rendering(net, TextRendering::ScaledBitmap);
        app.on_resize(options.cells);
        if let Some(url) = url {
            app.submit_url(&url);
        }
        Ok(Self {
            backend,
            composer: FrameComposer::new(ratatui::layout::Rect::new(
                0,
                0,
                options.cells.cols,
                options.cells.rows,
            )),
            pending_damage: FrameDamage::default(),
            scheduler: FrameScheduler::default(),
            pending_size: None,
            app,
            options,
            started: Instant::now(),
            modifiers: ModifiersState::empty(),
            pointer: None,
            wheel: WheelAccumulator::default(),
            cursor: CursorPresentation {
                visible: true,
                icon: CursorIcon::Default,
            },
            background,
            presented: None,
            failure: None,
        })
    }

    /// Run one pump step: collect fetch results and report what the loop should do.
    ///
    /// This is the frontend's most load-bearing logic and the reason it is a method
    /// rather than inline in `about_to_wait`: `App::step` is what delivers fetch
    /// results, so a loop that merely waits for input would leave every page load
    /// hanging in the channel. Splitting it out lets that be driven — and asserted —
    /// without a window.
    pub(super) fn tick_at(&mut self, now: Duration) -> Tick {
        let mut advanced = false;
        if let Some(input) = self.scheduler.take_due(now) {
            if let Some(size) = self.pending_size.take() {
                self.backend.resize(size);
            }
            self.app.advance(&input, now);
            advanced = true;
        } else if self.app.next_wake().is_some_and(|deadline| deadline <= now) {
            self.app.advance(&InputBatch::new(), now);
            advanced = true;
        }
        let damage = self.app.take_damage();
        let changed = !damage.is_empty();
        self.pending_damage.merge(damage);
        if advanced || changed {
            self.sync_cursor_icon();
        }
        if self.app.should_quit() {
            return Tick {
                quit: true,
                redraw: false,
            };
        }
        Tick {
            quit: false,
            redraw: !self.pending_damage.is_empty(),
        }
    }

    /// Show the hand over links, the arrow everywhere else, and only when it changes —
    /// a cursor request per pointer move would be a round trip to the window system for
    /// nothing.
    fn sync_cursor_icon(&mut self) {
        let wanted = cursor_presentation(self.app.pointer_cursor());
        let update = cursor_update(self.cursor, wanted);
        if update == CursorUpdate::default() {
            return;
        }
        if let Some(presented) = self.presented.as_ref() {
            if let Some(visible) = update.visible {
                presented.window.set_cursor_visible(visible);
            }
            if let Some(icon) = update.icon {
                presented.window.set_cursor(icon);
            }
        }
        self.cursor = wanted;
    }

    /// Record the first failure and ask the loop to stop.
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: io::Error) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        event_loop.exit();
    }

    /// Re-derive the cell grid from a physical window size.
    pub(super) fn resize(&mut self, physical: PhysicalSize<u32>) {
        let cells = grid_for(physical.width, physical.height, self.options.scale);
        if cells
            == self
                .pending_size
                .unwrap_or_else(|| self.backend.surface().size())
        {
            return;
        }
        self.pending_size = Some(cells);
        self.scheduler.push(
            InputEvent::Resize {
                size: cells,
                phase: ResizePhase::Preview,
            },
            self.started.elapsed(),
        );
    }

    pub(super) fn move_pointer(&mut self, at: Point) -> bool {
        if self.pointer == Some(at) {
            return false;
        }
        self.pointer = Some(at);
        self.scheduler.push(
            InputEvent::Mouse(MouseEvent {
                kind: MouseKind::Move,
                at,
            }),
            self.started.elapsed(),
        );
        true
    }

    /// Draw the chrome, then copy the pixels into the window.
    pub(super) fn redraw(&mut self) -> io::Result<()> {
        let damage = std::mem::take(&mut self.pending_damage);
        if damage.is_empty() {
            return Ok(());
        }
        let content_changed = damage.content.full
            || damage.content.scroll_rows != 0
            || damage.content.repaint != RowDamage::None;
        let reset_overlay = damage.content.full || damage.content.repaint != RowDamage::None;
        let view = self.app.chrome_view();
        let size = self.backend.surface().size();
        let area = ratatui::layout::Rect::new(0, 0, size.cols, size.rows);
        let scaled = view.content.painted.scaled_text.clone();
        let scroll = view.content.scroll;
        let content = chrome::content_rect(&view, area);
        let occlusions = chrome::occlusion_rects(&view, area)
            .into_iter()
            .map(layout_rect)
            .collect::<Vec<_>>();
        if reset_overlay {
            self.backend.clear_scaled_overlay();
        }
        self.composer.present(&mut self.backend, &view, &damage)?;
        let scaled = if damage.content.scroll_rows != 0
            && !damage.content.full
            && damage.content.repaint == RowDamage::None
        {
            let height = content.map_or(0, |rect| usize::from(rect.height));
            let amount = damage.content.scroll_rows.unsigned_abs() as usize;
            let exposed = if damage.content.scroll_rows > 0 {
                scroll.saturating_add(height.saturating_sub(amount))..scroll.saturating_add(height)
            } else {
                scroll..scroll.saturating_add(amount.min(height))
            };
            scaled
                .into_iter()
                .filter(|run| {
                    run.rect.row < exposed.end
                        && run.rect.row.saturating_add(run.rect.height) > exposed.start
                })
                .collect::<Vec<_>>()
        } else {
            scaled
        };
        if content_changed && let Some(content) = content {
            self.backend.draw_scaled_text(
                &scaled,
                (content.x, content.y),
                scroll,
                layout_rect(content),
                &occlusions,
                NORTON.palette(),
            );
        }
        self.present()
    }

    /// Blit the surface into the window's buffer and show it.
    ///
    /// The window is rarely an exact multiple of the cell size, so the grid usually
    /// leaves a margin at the right and bottom. Copying row by row and filling the
    /// remainder keeps the strides honest — writing the grid's rows straight into a
    /// wider window buffer would shear the image diagonally.
    fn present(&mut self) -> io::Result<()> {
        let Some(presented) = self.presented.as_mut() else {
            return Ok(());
        };
        let physical = presented.window.inner_size();
        let (Some(width), Some(height)) = (
            NonZeroU32::new(physical.width),
            NonZeroU32::new(physical.height),
        ) else {
            // Minimised: nothing to present, and softbuffer rejects a zero size.
            return Ok(());
        };
        let resized = presented.size != Some(physical);
        if resized {
            presented.surface.resize(width, height).map_err(into_io)?;
            presented.size = Some(physical);
            presented.damage_history.clear();
        }

        let damage = self.backend.surface_mut().take_damage_regions();
        if damage.is_empty() {
            return Ok(());
        }
        let backend_surface = self.backend.surface();
        let (grid_width, grid_height) = backend_surface.pixel_size();
        let pixels = backend_surface.pixels();
        let fill = (u32::from(self.background.r) << 16)
            | (u32::from(self.background.g) << 8)
            | u32::from(self.background.b);

        let window_width = width.get() as usize;
        let mut buffer = presented.surface.buffer_mut().map_err(into_io)?;
        let age = usize::from(buffer.age());
        let full = resized || age == 0 || age.saturating_sub(1) > presented.damage_history.len();
        let current_damage = if full {
            vec![softbuffer::Rect {
                x: 0,
                y: 0,
                width,
                height,
            }]
        } else {
            damage
                .into_iter()
                .map(|damage| softbuffer::Rect {
                    x: damage.x.min(u32::MAX as usize) as u32,
                    y: damage.y.min(u32::MAX as usize) as u32,
                    width: NonZeroU32::new(damage.width.min(u32::MAX as usize) as u32)
                        .expect("surface damage has width"),
                    height: NonZeroU32::new(damage.height.min(u32::MAX as usize) as u32)
                        .expect("surface damage has height"),
                })
                .collect()
        };
        let mut repair = current_damage.clone();
        repair.extend(
            presented
                .damage_history
                .iter()
                .take(age.saturating_sub(1))
                .flatten()
                .copied(),
        );
        if full {
            buffer.fill(fill);
        }
        let copied_width = grid_width.min(window_width);
        for damage in &repair {
            let first_row = damage.y as usize;
            let last_row = first_row
                .saturating_add(damage.height.get() as usize)
                .min(grid_height)
                .min(height.get() as usize);
            for row in first_row..last_row {
                let from = row * grid_width;
                let to = row * window_width;
                let first_col = (damage.x as usize).min(copied_width);
                let last_col = first_col
                    .saturating_add(damage.width.get() as usize)
                    .min(copied_width);
                buffer[to + first_col..to + last_col]
                    .copy_from_slice(&pixels[from + first_col..from + last_col]);
            }
        }
        presented.damage_history.insert(0, current_damage);
        presented.damage_history.truncate(8);
        buffer.present_with_damage(&repair).map_err(into_io)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CursorPresentation {
    pub(super) visible: bool,
    pub(super) icon: CursorIcon,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CursorUpdate {
    pub(super) visible: Option<bool>,
    pub(super) icon: Option<CursorIcon>,
}

pub(super) fn cursor_presentation(cursor: Cursor) -> CursorPresentation {
    let visible = cursor != Cursor::None;
    let icon = match cursor {
        Cursor::Auto | Cursor::Default | Cursor::None => CursorIcon::Default,
        Cursor::ContextMenu => CursorIcon::ContextMenu,
        Cursor::Help => CursorIcon::Help,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Cell => CursorIcon::Cell,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Text => CursorIcon::Text,
        Cursor::VerticalText => CursorIcon::VerticalText,
        Cursor::Alias => CursorIcon::Alias,
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Move => CursorIcon::Move,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EResize => CursorIcon::EResize,
        Cursor::NResize => CursorIcon::NResize,
        Cursor::NeResize => CursorIcon::NeResize,
        Cursor::NwResize => CursorIcon::NwResize,
        Cursor::SResize => CursorIcon::SResize,
        Cursor::SeResize => CursorIcon::SeResize,
        Cursor::SwResize => CursorIcon::SwResize,
        Cursor::WResize => CursorIcon::WResize,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
    };
    CursorPresentation { visible, icon }
}

pub(super) fn cursor_update(
    current: CursorPresentation,
    wanted: CursorPresentation,
) -> CursorUpdate {
    CursorUpdate {
        visible: (current.visible != wanted.visible).then_some(wanted.visible),
        icon: (wanted.visible && (!current.visible || current.icon != wanted.icon))
            .then_some(wanted.icon),
    }
}

fn layout_rect(rect: ratatui::layout::Rect) -> LayoutRect {
    LayoutRect {
        col: usize::from(rect.x),
        row: usize::from(rect.y),
        width: usize::from(rect.width),
        height: usize::from(rect.height),
    }
}

#[cfg(test)]
pub(super) fn union_damage(left: softbuffer::Rect, right: softbuffer::Rect) -> softbuffer::Rect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let far_x = left
        .x
        .saturating_add(left.width.get())
        .max(right.x.saturating_add(right.width.get()));
    let far_y = left
        .y
        .saturating_add(left.height.get())
        .max(right.y.saturating_add(right.height.get()));
    softbuffer::Rect {
        x,
        y,
        width: NonZeroU32::new(far_x - x).expect("damage union has width"),
        height: NonZeroU32::new(far_y - y).expect("damage union has height"),
    }
}

impl ApplicationHandler for VgaApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.presented.is_some() {
            return;
        }
        let (width, height) = pixel_size(self.options.cells, self.options.scale);
        let attributes = Window::default_attributes()
            .with_title("TextSurfer")
            .with_inner_size(PhysicalSize::new(width as u32, height as u32))
            .with_resize_increments(PhysicalSize::new(
                (CELL_W * self.options.scale.max(1)) as u32,
                (CELL_H * self.options.scale.max(1)) as u32,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(error) => return self.fail(event_loop, into_io(error)),
        };
        let context = match softbuffer::Context::new(window.clone()) {
            Ok(context) => context,
            Err(error) => return self.fail(event_loop, into_io(error)),
        };
        let surface = match softbuffer::Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(error) => return self.fail(event_loop, into_io(error)),
        };
        self.presented = Some(Presented {
            window,
            surface,
            _context: context,
            size: None,
            damage_history: Vec::new(),
        });
        let tick = self.tick_at(self.started.elapsed());
        if tick.redraw
            && let Err(error) = self.redraw()
        {
            self.fail(event_loop, error);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = from_window_key(&event.logical_key, event.state, self.modifiers)
                {
                    self.scheduler
                        .push(InputEvent::Key(key), self.started.elapsed());
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = cell_at(position, self.options.scale);
                self.move_pointer(at);
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                self.scheduler
                    .push(InputEvent::PointerLeft, self.started.elapsed());
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // A press before any `CursorMoved` has no position to aim at; aiming it
                // at the origin would click the menu bar.
                if let Some(at) = self.pointer
                    && let Some(event) = from_window_button(button, state, at)
                {
                    self.scheduler
                        .push(InputEvent::Mouse(event), self.started.elapsed());
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let notch = (CELL_H * self.options.scale.max(1)) as f64 * f64::from(WHEEL_ROWS);
                if let Some(at) = self.pointer
                    && let Some(rows) = self.wheel.push(delta, notch)
                {
                    self.scheduler.push(
                        InputEvent::Mouse(MouseEvent {
                            kind: MouseKind::Wheel { rows },
                            at,
                        }),
                        self.started.elapsed(),
                    );
                }
            }
            // Both carry a new physical size; the scale-factor case also arrives when a
            // window moves between displays of different density.
            WindowEvent::Resized(physical) => self.resize(physical),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(presented) = self.presented.as_ref() {
                    self.resize(presented.window.inner_size());
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = self.started.elapsed();
        let tick = self.tick_at(now);
        if tick.quit {
            event_loop.exit();
            return;
        }
        if tick.redraw
            && let Some(presented) = self.presented.as_ref()
        {
            presented.window.request_redraw();
        }
        match self.scheduler.next_deadline(self.app.next_wake()) {
            Some(deadline) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.started + deadline));
            }
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// Flatten a windowing error into the `io::Error` the binary already threads around.
fn into_io(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
