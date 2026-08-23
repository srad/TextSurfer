//! Glyph sourcing for the framebuffer frontend.
//!
//! Every glyph is 16 pixel rows tall and either 8 or 16 pixels wide, so one cell is
//! always `CELL_W` x `CELL_H` and a wide glyph occupies exactly two cells. Rows are
//! stored left-aligned in a `u16` — bit 15 is the leftmost pixel — so narrow and wide
//! glyphs share one representation and one blitting loop.
//!
//! Lookup runs in tiers: CP437 first, so the DOS face wins wherever it has a glyph,
//! then (once `vga-unifont` lands) Unifont for the rest of the BMP, then a replacement
//! box. Ordering matters: it is what keeps the chrome authentically CP437 while still
//! rendering text the code page never covered.

mod cp437;
mod table;
mod unifont;

#[cfg(test)]
mod tests;

pub use table::{CP437_8X16, CP437_8X16_SHA256, CP437_TO_UNICODE, UNICODE_TO_CP437};

/// Pixel width of one character cell.
pub const CELL_W: usize = 8;

/// Pixel height of one character cell.
pub const CELL_H: usize = 16;

/// How many cells a glyph occupies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphWidth {
    /// 8 pixels — one cell.
    Narrow,
    /// 16 pixels — two cells.
    Wide,
}

impl GlyphWidth {
    /// Number of character cells this glyph spans.
    pub const fn cells(self) -> usize {
        match self {
            Self::Narrow => 1,
            Self::Wide => 2,
        }
    }

    /// Number of pixel columns this glyph spans.
    pub const fn pixels(self) -> usize {
        self.cells() * CELL_W
    }
}

/// A glyph bitmap, rows left-aligned so bit 15 is the leftmost pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    rows: [u16; CELL_H],
    width: GlyphWidth,
}

impl Glyph {
    /// Build a narrow glyph from eight-bit rows, left-aligning each row.
    pub const fn narrow(rows: [u8; CELL_H]) -> Self {
        let mut left = [0u16; CELL_H];
        let mut row = 0;
        while row < CELL_H {
            left[row] = (rows[row] as u16) << 8;
            row += 1;
        }
        Self {
            rows: left,
            width: GlyphWidth::Narrow,
        }
    }

    /// Build a wide glyph from sixteen-bit rows, already left-aligned.
    pub const fn wide(rows: [u16; CELL_H]) -> Self {
        Self {
            rows,
            width: GlyphWidth::Wide,
        }
    }

    /// How many cells this glyph occupies.
    pub const fn width(&self) -> GlyphWidth {
        self.width
    }

    /// Whether the pixel at `x` (from the left) and `y` (from the top) is set.
    ///
    /// Out-of-range coordinates read as unset rather than panicking, so a caller
    /// blitting a two-cell span over a narrow glyph gets blank pixels instead of a
    /// crash.
    pub const fn pixel(&self, x: usize, y: usize) -> bool {
        if y >= CELL_H || x >= 16 {
            return false;
        }
        self.rows[y] & (1 << (15 - x)) != 0
    }
}

/// The glyph drawn when no tier covers a character.
///
/// A hollow box, matching the convention every font uses for "no glyph here", so a
/// missing character is visibly missing rather than silently blank.
pub const REPLACEMENT: Glyph = Glyph::narrow([
    0x00, 0x00, 0x00, 0x7e, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x7e, 0x00, 0x00, 0x00, 0x00,
]);

/// Look up `ch`, falling back through the tiers to [`REPLACEMENT`].
///
/// CP437 is tried first so the DOS face wins wherever it has a glyph — that ordering
/// is what keeps the chrome authentic while still rendering text the code page never
/// covered.
pub fn glyph(ch: char) -> Glyph {
    cp437::glyph(ch)
        .or_else(|| unifont::glyph(ch))
        .unwrap_or(REPLACEMENT)
}

/// The CP437 index that draws `ch`, if the code page covers it.
pub fn cp437_index(ch: char) -> Option<u8> {
    cp437::index(ch)
}
