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

use ratatui::Terminal;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::app::App;
use crate::app::net::Navigate;
use crate::core::geom::Size;
use crate::core::style::Rgb;
use crate::ui::chrome;
use crate::ui::theme::{NORTON, rgb_of};

use super::backend::{VgaBackend, grid_for, pixel_size};
use super::input::from_window_key;
use super::surface::SurfaceConfig;

/// Matches the terminal frontend's poll interval, so both pump `App::step` alike.
const TICK: Duration = Duration::from_millis(50);

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
}

struct VgaApp {
    terminal: Terminal<VgaBackend>,
    app: App,
    options: VgaOptions,
    started: Instant,
    modifiers: ModifiersState,
    background: Rgb,
    presented: Option<Presented>,
    /// The first error to escape a callback. `ApplicationHandler` cannot return one,
    /// so it is stashed here and surfaced by [`run`] after the loop exits.
    failure: Option<io::Error>,
}

impl VgaApp {
    fn new(net: Arc<dyn Navigate>, options: VgaOptions, url: Option<String>) -> io::Result<Self> {
        let background = rgb_of(NORTON.bg);
        let backend = VgaBackend::new(SurfaceConfig {
            cols: options.cells.cols,
            rows: options.cells.rows,
            scale: options.scale,
            default_fg: rgb_of(NORTON.text),
            default_bg: background,
        });
        let mut app = App::with_net(net);
        app.on_resize(options.cells);
        if let Some(url) = url {
            app.submit_url(&url);
        }
        Ok(Self {
            terminal: Terminal::new(backend)?,
            app,
            options,
            started: Instant::now(),
            modifiers: ModifiersState::empty(),
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
    pub(super) fn tick(&mut self) -> Tick {
        self.app.step(self.started.elapsed());
        if self.app.should_quit() {
            return Tick {
                quit: true,
                redraw: false,
            };
        }
        Tick {
            quit: false,
            redraw: self.app.take_dirty(),
        }
    }

    /// Record the first failure and ask the loop to stop.
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: io::Error) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        event_loop.exit();
    }

    /// Re-derive the cell grid from a physical window size.
    fn resize(&mut self, physical: PhysicalSize<u32>) {
        let cells = grid_for(physical.width, physical.height, self.options.scale);
        if cells == self.terminal.backend().surface().size() {
            return;
        }
        self.terminal.backend_mut().resize(cells);
        self.app.on_resize(cells);
    }

    /// Draw the chrome, then copy the pixels into the window.
    fn redraw(&mut self) -> io::Result<()> {
        let view = self.app.chrome_view();
        self.terminal.draw(|frame| chrome::draw(frame, &view))?;
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
        presented.surface.resize(width, height).map_err(into_io)?;

        let backend_surface = self.terminal.backend().surface();
        let (grid_width, grid_height) = backend_surface.pixel_size();
        let pixels = backend_surface.pixels();
        let fill = (u32::from(self.background.r) << 16)
            | (u32::from(self.background.g) << 8)
            | u32::from(self.background.b);

        let window_width = width.get() as usize;
        let mut buffer = presented.surface.buffer_mut().map_err(into_io)?;
        buffer.fill(fill);
        let copied_width = grid_width.min(window_width);
        for row in 0..grid_height.min(height.get() as usize) {
            let from = row * grid_width;
            let to = row * window_width;
            buffer[to..to + copied_width].copy_from_slice(&pixels[from..from + copied_width]);
        }
        buffer.present().map_err(into_io)
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
            .with_inner_size(PhysicalSize::new(width as u32, height as u32));
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
        });
        if let Err(error) = self.redraw() {
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
                    self.app.handle_key(key);
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
        let tick = self.tick();
        if tick.quit {
            event_loop.exit();
            return;
        }
        if tick.redraw
            && let Some(presented) = self.presented.as_ref()
        {
            presented.window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + TICK));
    }
}

/// Flatten a windowing error into the `io::Error` the binary already threads around.
fn into_io(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
