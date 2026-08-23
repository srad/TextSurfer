//! The cell grid rendered into a pixel buffer.
//!
//! The surface keeps a shadow copy of every cell's resolved content alongside the
//! pixels. That shadow is what makes wide glyphs safe: ratatui stores a double-width
//! character in one cell and leaves the next one blank, and its sparse diff may deliver
//! either half alone. Repainting a cell's neighbours from the shadow means a two-cell
//! glyph is always drawn whole, instead of being sliced down the middle by an update
//! that only touched one side.

use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier};

use crate::core::geom::Size;
use crate::core::style::Rgb;
use crate::ui::theme::rgb_of;

use super::font::{CELL_H, CELL_W, GlyphWidth, glyph};

/// Pixel row within a cell that an underline fills.
const UNDERLINE_ROW: usize = 14;

/// How the VGA intensity bit brightened a colour: +85 on each channel.
///
/// Applied to the CGA base palette this reproduces the bright half exactly — dark red
/// `(170, 0, 0)` becomes `(255, 85, 85)`, dark blue `(0, 0, 170)` becomes
/// `(85, 85, 255)` — and on an arbitrary author colour it is a mild, safe lift.
const INTENSITY: u8 = 85;

/// Everything needed to build a [`Surface`].
///
/// A struct rather than positional arguments: `cols`/`rows` and `default_fg`/
/// `default_bg` are same-typed pairs that would swap silently at a call site.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceConfig {
    /// Grid width in cells.
    pub cols: u16,
    /// Grid height in cells.
    pub rows: u16,
    /// Integer pixel multiplier, so 8x16 stays legible on a HiDPI display.
    pub scale: usize,
    /// Colour for `Color::Reset` in the foreground role.
    pub default_fg: Rgb,
    /// Colour for `Color::Reset` in the background role.
    pub default_bg: Rgb,
}

/// One cell's content after colour and modifier resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellState {
    /// The character to draw. Combining marks are dropped — a bitmap face has no way
    /// to compose them.
    pub ch: char,
    /// Foreground, with bold and reverse already folded in.
    pub fg: Rgb,
    /// Background, with reverse already folded in.
    pub bg: Rgb,
    /// Whether to fill [`UNDERLINE_ROW`].
    pub underline: bool,
}

/// A cell grid backed by a pixel buffer.
pub struct Surface {
    cols: u16,
    rows: u16,
    scale: usize,
    default_fg: Rgb,
    default_bg: Rgb,
    cells: Vec<CellState>,
    pixels: Vec<u32>,
    cursor: Option<(u16, u16)>,
}

impl Surface {
    /// Build a surface, painted entirely with the default background.
    pub fn new(config: SurfaceConfig) -> Self {
        let SurfaceConfig {
            cols,
            rows,
            scale,
            default_fg,
            default_bg,
        } = config;
        let mut surface = Self {
            cols,
            rows,
            scale: scale.max(1),
            default_fg,
            default_bg,
            cells: Vec::new(),
            pixels: Vec::new(),
            cursor: None,
        };
        surface.resize(Size { cols, rows });
        surface
    }

    /// Show the block cursor at a cell, or hide it with `None`.
    ///
    /// Both the old and new positions are repainted, so the caret leaves no trail.
    pub fn set_cursor(&mut self, at: Option<(u16, u16)>) {
        if self.cursor == at {
            return;
        }
        let previous = self.cursor.take();
        self.cursor = at;
        for (col, row) in previous.into_iter().chain(at) {
            self.paint_cell(col, row);
        }
    }

    /// Where the block cursor currently sits, if it is shown.
    pub fn cursor(&self) -> Option<(u16, u16)> {
        self.cursor
    }

    /// The blank cell this surface clears to.
    fn blank(&self) -> CellState {
        CellState {
            ch: ' ',
            fg: self.default_fg,
            bg: self.default_bg,
            underline: false,
        }
    }

    /// Grid size in cells.
    pub fn size(&self) -> Size {
        Size {
            cols: self.cols,
            rows: self.rows,
        }
    }

    /// Buffer size in physical pixels, as `(width, height)`.
    pub fn pixel_size(&self) -> (usize, usize) {
        (
            self.cols as usize * CELL_W * self.scale,
            self.rows as usize * CELL_H * self.scale,
        )
    }

    /// The pixel buffer, one `0x00RRGGBB` word per pixel, row-major.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Resize the grid, discarding contents and repainting to the background.
    pub fn resize(&mut self, size: Size) {
        self.cols = size.cols;
        self.rows = size.rows;
        let (width, height) = self.pixel_size();
        self.cells = vec![self.blank(); size.cols as usize * size.rows as usize];
        self.pixels = vec![pack(self.default_bg); width * height];
        // The old position may not exist in the new grid; ratatui re-sets it next draw.
        self.cursor = None;
    }

    /// Reset every cell to blank.
    pub fn clear(&mut self) {
        let blank = self.blank();
        self.cells.fill(blank);
        self.pixels.fill(pack(self.default_bg));
        // Filling the buffer wiped the caret; put it back where it was.
        if let Some((col, row)) = self.cursor {
            self.paint_cell(col, row);
        }
    }

    /// Index into the shadow grid, if the position is inside it.
    fn offset(&self, col: u16, row: u16) -> Option<usize> {
        (col < self.cols && row < self.rows)
            .then(|| row as usize * self.cols as usize + col as usize)
    }

    /// Write one cell and repaint it together with its horizontal neighbours.
    ///
    /// The neighbours are what keep a two-cell glyph whole; see the module comment.
    pub fn set_cell(&mut self, col: u16, row: u16, cell: &Cell) {
        let Some(offset) = self.offset(col, row) else {
            return;
        };
        self.cells[offset] = self.resolve(cell);
        for neighbour in [col.saturating_sub(1), col, col.saturating_add(1)] {
            self.paint_cell(neighbour, row);
        }
    }

    /// Resolve a ratatui cell into concrete colours and a single character.
    ///
    /// `Color::Reset` is answered from the theme rather than through [`rgb_of`], which
    /// maps it to black — that would punch black holes through the DOS field.
    pub fn resolve(&self, cell: &Cell) -> CellState {
        let modifier = cell.modifier;
        let mut fg = match cell.fg {
            Color::Reset => self.default_fg,
            other => rgb_of(other),
        };
        let mut bg = match cell.bg {
            Color::Reset => self.default_bg,
            other => rgb_of(other),
        };
        if modifier.contains(Modifier::BOLD) {
            fg = brighten(fg);
        }
        if modifier.contains(Modifier::REVERSED) {
            core::mem::swap(&mut fg, &mut bg);
        }
        CellState {
            ch: cell.symbol().chars().next().unwrap_or(' '),
            fg,
            bg,
            underline: modifier.contains(Modifier::UNDERLINED),
        }
    }

    /// Paint one cell from the shadow grid.
    ///
    /// If the cell to the left holds a wide glyph, this cell is that glyph's right half
    /// and is painted from there instead, so the pair stays whole.
    fn paint_cell(&mut self, col: u16, row: u16) {
        if self.offset(col, row).is_none() {
            return;
        }
        if col > 0
            && let Some(left) = self.offset(col - 1, row)
            && glyph(self.cells[left].ch).width() == GlyphWidth::Wide
        {
            self.blit(col - 1, row);
            return;
        }
        self.blit(col, row);
    }

    /// Blit the glyph owned by `col` — one cell wide, or two if the glyph is wide.
    fn blit(&mut self, col: u16, row: u16) {
        let Some(offset) = self.offset(col, row) else {
            return;
        };
        let state = self.cells[offset];
        let shape = glyph(state.ch);
        // The caret is drawn as a reverse-video block, the way a text-mode cursor was.
        let (fg, bg) = if self.cursor == Some((col, row)) {
            (pack(state.bg), pack(state.fg))
        } else {
            (pack(state.fg), pack(state.bg))
        };
        let (width, _) = self.pixel_size();
        let origin_x = col as usize * CELL_W * self.scale;
        let origin_y = row as usize * CELL_H * self.scale;
        // A wide glyph runs past its own cell; clip at the grid's right edge so the
        // final column of a grid that ends mid-glyph does not write out of bounds.
        let span = (shape.width().pixels()).min((self.cols - col) as usize * CELL_W);

        for y in 0..CELL_H {
            let lit_row = state.underline && y == UNDERLINE_ROW;
            for x in 0..span {
                let colour = if lit_row || shape.pixel(x, y) { fg } else { bg };
                // Each source pixel becomes a scale x scale block.
                for dy in 0..self.scale {
                    let line = (origin_y + y * self.scale + dy) * width;
                    for dx in 0..self.scale {
                        self.pixels[line + origin_x + x * self.scale + dx] = colour;
                    }
                }
            }
        }
    }
}

/// Pack a colour into softbuffer's `0x00RRGGBB` word.
fn pack(colour: Rgb) -> u32 {
    (u32::from(colour.r) << 16) | (u32::from(colour.g) << 8) | u32::from(colour.b)
}

/// Apply the VGA intensity bit.
fn brighten(colour: Rgb) -> Rgb {
    Rgb::new(
        colour.r.saturating_add(INTENSITY),
        colour.g.saturating_add(INTENSITY),
        colour.b.saturating_add(INTENSITY),
    )
}
