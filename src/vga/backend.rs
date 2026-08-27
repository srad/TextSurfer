//! ratatui's [`Backend`] over a [`Surface`].
//!
//! This is what lets the framebuffer frontend reuse the entire existing UI: every
//! widget and `ui::chrome::draw` are written against a backend-agnostic `Frame`, so
//! satisfying this trait is the whole integration.
//!
//! The error type is [`io::Error`] purely so the frontend composes with the rest of
//! the binary's `io::Result` plumbing; none of these operations can actually fail.
//!
//! `scroll_region_up`/`scroll_region_down` are deliberately absent: they sit behind
//! ratatui's `scrolling-regions` feature, which is not in its default set.

use std::io;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size as TerminalSize};

use winit::dpi::PhysicalPosition;

use crate::core::geom::{Point, Size};
use crate::core::style::Palette;
use crate::layout::LayoutRect;
use crate::paint::{DisplayList, ScaledTextRun};

use super::font::{CELL_H, CELL_W};
use super::surface::{Surface, SurfaceConfig};

/// A ratatui backend that paints into a pixel buffer.
pub struct VgaBackend {
    surface: Surface,
    cursor: Position,
    cursor_visible: bool,
}

impl VgaBackend {
    /// Build a backend over a fresh surface.
    pub fn new(config: SurfaceConfig) -> Self {
        Self {
            surface: Surface::new(config),
            cursor: Position::ORIGIN,
            cursor_visible: false,
        }
    }

    /// The surface being painted, for presenting or inspecting.
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    pub fn surface_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }

    /// Resize the grid. Contents are discarded; ratatui redraws in full afterwards.
    pub fn resize(&mut self, size: Size) {
        self.surface.resize(size);
        self.sync_cursor();
    }

    pub fn clear_scaled_overlay(&mut self) {
        self.surface.clear_scaled_overlay();
    }

    pub fn draw_scaled_text(
        &mut self,
        runs: &[ScaledTextRun],
        origin: (u16, u16),
        scroll: usize,
        clip: LayoutRect,
        occlusions: &[LayoutRect],
        palette: Palette,
    ) {
        self.surface
            .draw_scaled_text(runs, origin, scroll, clip, occlusions, palette);
    }

    pub fn draw_overlays(
        &mut self,
        painted: &DisplayList,
        origin: (u16, u16),
        scroll: usize,
        clip: LayoutRect,
        occlusions: &[LayoutRect],
        palette: Palette,
    ) {
        self.surface
            .draw_overlays(painted, origin, scroll, clip, occlusions, palette);
    }

    /// Push the tracked cursor state onto the surface.
    fn sync_cursor(&mut self) {
        let at = self
            .cursor_visible
            .then_some((self.cursor.x, self.cursor.y));
        self.surface.set_cursor(at);
    }
}

impl Backend for VgaBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.surface.set_cells(content);
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = false;
        self.sync_cursor();
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = true;
        self.sync_cursor();
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor = position.into();
        self.sync_cursor();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.surface.clear();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let size = self.surface.size();
        let (col, row) = (self.cursor.x, self.cursor.y);
        // Inclusive cell ranges per the trait's documentation of each variant.
        let (from, to) = match clear_type {
            ClearType::All => (0, size.cols as u32 * size.rows as u32),
            ClearType::AfterCursor => (index(col, row, size) + 1, cell_count(size)),
            ClearType::BeforeCursor => (0, index(col, row, size) + 1),
            ClearType::CurrentLine => (index(0, row, size), index(0, row, size) + size.cols as u32),
            ClearType::UntilNewLine => (
                index(col, row, size),
                index(0, row, size) + size.cols as u32,
            ),
        };
        let blank = Cell::default();
        for offset in from..to.min(cell_count(size)) {
            let cols = size.cols as u32;
            self.surface
                .set_cell((offset % cols) as u16, (offset / cols) as u16, &blank);
        }
        Ok(())
    }

    fn size(&self) -> Result<TerminalSize, Self::Error> {
        let size = self.surface.size();
        Ok(TerminalSize {
            width: size.cols,
            height: size.rows,
        })
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        let size = self.surface.size();
        let (width, height) = self.surface.pixel_size();
        Ok(WindowSize {
            columns_rows: TerminalSize {
                width: size.cols,
                height: size.rows,
            },
            // Unlike a terminal, we know this exactly — it is our own buffer.
            pixels: TerminalSize {
                width: u16::try_from(width).unwrap_or(u16::MAX),
                height: u16::try_from(height).unwrap_or(u16::MAX),
            },
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // Painting already happened in `draw`; presenting is the window's job.
        Ok(())
    }

    fn scroll_region_up(
        &mut self,
        region: std::ops::Range<u16>,
        line_count: u16,
    ) -> Result<(), Self::Error> {
        self.surface.scroll_rows(region, line_count, true);
        Ok(())
    }

    fn scroll_region_down(
        &mut self,
        region: std::ops::Range<u16>,
        line_count: u16,
    ) -> Result<(), Self::Error> {
        self.surface.scroll_rows(region, line_count, false);
        Ok(())
    }
}

/// Total cells in the grid.
fn cell_count(size: Size) -> u32 {
    u32::from(size.cols) * u32::from(size.rows)
}

/// Row-major index of a cell.
fn index(col: u16, row: u16, size: Size) -> u32 {
    u32::from(row) * u32::from(size.cols) + u32::from(col)
}

/// Pixel dimensions a grid of this many cells needs, at `scale`.
pub fn pixel_size(size: Size, scale: usize) -> (usize, usize) {
    (
        size.cols as usize * CELL_W * scale,
        size.rows as usize * CELL_H * scale,
    )
}

/// The cell a window pixel position falls in, at `scale`.
///
/// Deliberately unclamped at the far edge: the grid rarely fills the window exactly, and
/// a click in the right or bottom margin must land outside it so the zone test can call
/// it inert rather than pinning it to the last row.
pub fn cell_at(position: PhysicalPosition<f64>, scale: usize) -> Point {
    let scale = scale.max(1) as f64;
    let cell = |value: f64, size: usize| {
        let cells = (value.max(0.0) / (size as f64 * scale)).floor();
        if cells >= f64::from(u16::MAX) {
            u16::MAX
        } else {
            cells as u16
        }
    };
    Point {
        col: cell(position.x, CELL_W),
        row: cell(position.y, CELL_H),
    }
}

/// Cell grid that fits a window of this pixel size, at `scale`.
///
/// Clamped to at least one cell each way: ratatui panics on a zero-sized area, and a
/// window can legitimately report a tiny or zero size while being resized.
pub fn grid_for(width: u32, height: u32, scale: usize) -> Size {
    let scale = scale.max(1) as u32;
    Size {
        cols: (width / (CELL_W as u32 * scale)).clamp(1, u16::MAX as u32) as u16,
        rows: (height / (CELL_H as u32 * scale)).clamp(1, u16::MAX as u32) as u16,
    }
}
