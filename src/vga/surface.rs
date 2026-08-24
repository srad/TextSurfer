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
use unicode_width::UnicodeWidthChar;

use crate::core::geom::Size;
use crate::core::style::{Palette, Rgb};
use crate::layout::LayoutRect;
use crate::paint::{ScaledTextRun, resolve_cell_style};
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
    pub strike: bool,
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
    overlay_cells: Vec<(u16, u16)>,
    damage: Vec<PixelRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
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
            overlay_cells: Vec::new(),
            damage: Vec::new(),
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
            strike: false,
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

    pub fn take_damage(&mut self) -> Option<PixelRect> {
        let damage = std::mem::take(&mut self.damage);
        damage.into_iter().reduce(union_pixel_rect)
    }

    pub fn take_damage_regions(&mut self) -> Vec<PixelRect> {
        std::mem::take(&mut self.damage)
    }

    /// Resize the grid, discarding contents and repainting to the background.
    pub fn resize(&mut self, size: Size) {
        self.cols = size.cols;
        self.rows = size.rows;
        let (width, height) = self.pixel_size();
        let blank = self.blank();
        self.cells
            .resize(size.cols as usize * size.rows as usize, blank);
        self.cells.fill(blank);
        self.pixels.resize(width * height, pack(self.default_bg));
        self.pixels.fill(pack(self.default_bg));
        // The old position may not exist in the new grid; ratatui re-sets it next draw.
        self.cursor = None;
        self.overlay_cells.clear();
        self.damage = vec![PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        }];
    }

    /// Reset every cell to blank.
    pub fn clear(&mut self) {
        let blank = self.blank();
        self.cells.fill(blank);
        self.pixels.fill(pack(self.default_bg));
        self.overlay_cells.clear();
        let (width, height) = self.pixel_size();
        self.damage = vec![PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        }];
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
        self.set_cells(std::iter::once((col, row, cell)));
    }

    pub fn set_cells<'a, I>(&mut self, content: I)
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut rows = vec![None::<(u16, u16)>; usize::from(self.rows)];
        for (col, row, cell) in content {
            let Some(offset) = self.offset(col, row) else {
                continue;
            };
            let resolved = self.resolve(cell);
            if self.cells[offset] == resolved {
                continue;
            }
            self.cells[offset] = resolved;
            let start = col.saturating_sub(1);
            let end = col.saturating_add(2).min(self.cols);
            let range = &mut rows[usize::from(row)];
            *range = Some(match *range {
                Some((left, right)) => (left.min(start), right.max(end)),
                None => (start, end),
            });
        }
        for (row, range) in rows.into_iter().enumerate() {
            let Some((start, end)) = range else {
                continue;
            };
            for col in start..end {
                self.paint_cell(col, row as u16);
            }
        }
    }

    pub fn scroll_rows(&mut self, region: std::ops::Range<u16>, amount: u16, up: bool) {
        let start = region.start.min(self.rows);
        let end = region.end.min(self.rows);
        let amount = amount.min(end.saturating_sub(start));
        if amount == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let cell_start = usize::from(start) * cols;
        let cell_end = usize::from(end) * cols;
        let cell_amount = usize::from(amount) * cols;
        let blank = self.blank();
        if up {
            self.cells
                .copy_within(cell_start + cell_amount..cell_end, cell_start);
            self.cells[cell_end - cell_amount..cell_end].fill(blank);
        } else {
            self.cells
                .copy_within(cell_start..cell_end - cell_amount, cell_start + cell_amount);
            self.cells[cell_start..cell_start + cell_amount].fill(blank);
        }
        let (pixel_width, _) = self.pixel_size();
        let row_height = CELL_H * self.scale;
        let pixel_start = usize::from(start) * row_height * pixel_width;
        let pixel_end = usize::from(end) * row_height * pixel_width;
        let pixel_amount = usize::from(amount) * row_height * pixel_width;
        if up {
            self.pixels
                .copy_within(pixel_start + pixel_amount..pixel_end, pixel_start);
            self.pixels[pixel_end - pixel_amount..pixel_end].fill(pack(self.default_bg));
        } else {
            self.pixels.copy_within(
                pixel_start..pixel_end - pixel_amount,
                pixel_start + pixel_amount,
            );
            self.pixels[pixel_start..pixel_start + pixel_amount].fill(pack(self.default_bg));
        }
        let overlays = std::mem::take(&mut self.overlay_cells);
        self.overlay_cells = overlays
            .into_iter()
            .filter_map(|(col, row)| {
                if row < start || row >= end {
                    return Some((col, row));
                }
                if up {
                    (row >= start + amount).then_some((col, row - amount))
                } else {
                    (row + amount < end).then_some((col, row + amount))
                }
            })
            .collect();
        self.mark_damage(0, pixel_start, pixel_width, pixel_end - pixel_start);
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
        if modifier.contains(Modifier::DIM) {
            fg = fg.blend(bg, 0.5);
        }
        if modifier.contains(Modifier::REVERSED) {
            core::mem::swap(&mut fg, &mut bg);
        }
        CellState {
            ch: cell.symbol().chars().next().unwrap_or(' '),
            fg,
            bg,
            underline: modifier.contains(Modifier::UNDERLINED),
            strike: modifier.contains(Modifier::CROSSED_OUT),
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
        self.mark_damage(origin_x, origin_y, span * self.scale, CELL_H * self.scale);

        for y in 0..CELL_H {
            let lit_row = state.underline && y == UNDERLINE_ROW || state.strike && y == CELL_H / 2;
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

    pub fn clear_scaled_overlay(&mut self) {
        let mut cells = std::mem::take(&mut self.overlay_cells);
        cells.sort_unstable();
        cells.dedup();
        for (col, row) in cells {
            self.paint_cell(col, row);
        }
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
        for run in runs {
            let scale = usize::from(run.style.scale);
            if scale <= 1 || run.rect.row < scroll {
                continue;
            }
            let screen_row =
                usize::from(origin.1).saturating_add(run.rect.row.saturating_sub(scroll));
            let mut screen_col = usize::from(origin.0).saturating_add(run.rect.col);
            let style = resolve_cell_style(run.style, palette);
            for ch in run.text.chars() {
                let shape = glyph(ch);
                let base_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if base_width == 0 {
                    continue;
                }
                let cell_width = base_width.saturating_mul(scale);
                let rect = LayoutRect {
                    col: screen_col,
                    row: screen_row,
                    width: cell_width,
                    height: scale,
                };
                if run.ink
                    && contains(clip, rect)
                    && !occlusions
                        .iter()
                        .any(|occlusion| intersects(*occlusion, rect))
                {
                    self.blit_scaled(shape, rect.col, rect.row, scale, style);
                    for row in rect.row..rect.row.saturating_add(rect.height) {
                        for col in rect.col..rect.col.saturating_add(rect.width) {
                            if let (Ok(col), Ok(row)) = (u16::try_from(col), u16::try_from(row)) {
                                self.overlay_cells.push((col, row));
                            }
                        }
                    }
                }
                screen_col = screen_col.saturating_add(cell_width);
            }
        }
        if let Some((col, row)) = self.cursor {
            self.paint_cell(col, row);
        }
    }

    fn blit_scaled(
        &mut self,
        shape: super::font::Glyph,
        col: usize,
        row: usize,
        text_scale: usize,
        style: crate::core::style::CellStyle,
    ) {
        let mut fg = style.fg.map_or(self.default_fg, |color| color.rgb);
        let mut bg = style.bg.unwrap_or(self.default_bg);
        if style.bold {
            fg = brighten(fg);
        }
        if style.dim {
            fg = fg.blend(bg, 0.5);
        }
        if style.reverse {
            core::mem::swap(&mut fg, &mut bg);
        }
        let fg = pack(fg);
        let (surface_width, _) = self.pixel_size();
        let pixel_scale = text_scale.saturating_mul(self.scale);
        let origin_x = col.saturating_mul(CELL_W).saturating_mul(self.scale);
        let origin_y = row.saturating_mul(CELL_H).saturating_mul(self.scale);
        self.mark_damage(
            origin_x,
            origin_y,
            shape.width().pixels().saturating_mul(pixel_scale),
            CELL_H.saturating_mul(pixel_scale),
        );
        for y in 0..CELL_H {
            let decorated =
                style.underline && y == UNDERLINE_ROW || style.strike && y == CELL_H / 2;
            for x in 0..shape.width().pixels() {
                if !decorated && !shape.pixel(x, y) {
                    continue;
                }
                for dy in 0..pixel_scale {
                    let line = (origin_y + y * pixel_scale + dy) * surface_width;
                    for dx in 0..pixel_scale {
                        let at = line + origin_x + x * pixel_scale + dx;
                        if let Some(pixel) = self.pixels.get_mut(at) {
                            *pixel = fg;
                        }
                    }
                }
            }
        }
    }

    fn mark_damage(&mut self, x: usize, y: usize, width: usize, height: usize) {
        let (surface_width, surface_height) = self.pixel_size();
        let right = x.saturating_add(width).min(surface_width);
        let bottom = y.saturating_add(height).min(surface_height);
        if x >= right || y >= bottom {
            return;
        }
        let mut next = PixelRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        };
        let mut index = 0;
        while index < self.damage.len() {
            if pixel_rects_touch(self.damage[index], next) {
                next = union_pixel_rect(self.damage.swap_remove(index), next);
            } else {
                index += 1;
            }
        }
        self.damage.push(next);
        let covered = self
            .damage
            .iter()
            .map(|rect| rect.width.saturating_mul(rect.height))
            .sum::<usize>();
        if self.damage.len() > 32
            || covered.saturating_mul(2) >= surface_width.saturating_mul(surface_height)
        {
            self.damage = vec![PixelRect {
                x: 0,
                y: 0,
                width: surface_width,
                height: surface_height,
            }];
        }
    }
}

fn pixel_rects_touch(left: PixelRect, right: PixelRect) -> bool {
    left.x <= right.x.saturating_add(right.width)
        && right.x <= left.x.saturating_add(left.width)
        && left.y <= right.y.saturating_add(right.height)
        && right.y <= left.y.saturating_add(left.height)
}

fn union_pixel_rect(left: PixelRect, right: PixelRect) -> PixelRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let far_x = left
        .x
        .saturating_add(left.width)
        .max(right.x.saturating_add(right.width));
    let far_y = left
        .y
        .saturating_add(left.height)
        .max(right.y.saturating_add(right.height));
    PixelRect {
        x,
        y,
        width: far_x - x,
        height: far_y - y,
    }
}

fn contains(outer: LayoutRect, inner: LayoutRect) -> bool {
    inner.col >= outer.col
        && inner.row >= outer.row
        && inner.col.saturating_add(inner.width) <= outer.col.saturating_add(outer.width)
        && inner.row.saturating_add(inner.height) <= outer.row.saturating_add(outer.height)
}

fn intersects(a: LayoutRect, b: LayoutRect) -> bool {
    a.col < b.col.saturating_add(b.width)
        && b.col < a.col.saturating_add(a.width)
        && a.row < b.row.saturating_add(b.height)
        && b.row < a.row.saturating_add(a.height)
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
