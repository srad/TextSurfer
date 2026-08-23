//! The Unifont tier: everything CP437 never covered.
//!
//! CP437 has 256 glyphs, which is not enough to render the web — curly quotes, en and
//! em dashes and every non-Latin script fall outside it, and they are common enough
//! that without this tier most real pages would be a field of replacement boxes.
//!
//! GNU Unifont fits the DOS grid exactly: narrow glyphs are 8x16 and wide ones 16x16,
//! the same metric the CP437 face uses, so the two tiers share one cell and one
//! blitting loop. Bytes arrive most-significant-bit leftmost, one byte per row for
//! narrow glyphs and two for wide, which is already this module's [`Glyph`] layout.
//!
//! The `unifont-bitmap` crate is MIT/Apache-2.0; the font data it embeds is GNU
//! Unifont, under the SIL Open Font License 1.1 (dual with GPLv2+ with the font
//! embedding exception).

use std::cell::RefCell;

use unifont_bitmap::Unifont;

use super::{CELL_H, Glyph};

/// Stack given to the loader thread. `Unifont` is ~136 KB by value and a debug build
/// copies it several times while constructing it; 4 MB leaves ample room.
const LOADER_STACK: usize = 4 * 1024 * 1024;

thread_local! {
    /// Unifont decompresses lazily, one 256-codepoint page at a time, and caches what
    /// it has decompressed. Rendering is single-threaded, so a thread-local `RefCell`
    /// gives that cache without a lock on a path that runs for every cell.
    ///
    /// Pages are never evicted. A page is a few kilobytes and a session touches only
    /// the scripts it actually renders, so the cache stays small in practice.
    static UNIFONT: RefCell<Box<Unifont>> = RefCell::new(load());
}

/// Build the font on a thread with a stack big enough to hold it.
///
/// `Unifont` carries its 4352-entry page table inline, so the value is about 136 KB,
/// and `Unifont::open` materialises it through several by-value locals that a debug
/// build does not elide. Constructing it on the main thread overflowed the 1 MB stack
/// Windows gives that thread — the frontend died on launch with `STATUS_STACK_OVERFLOW`
/// before drawing anything.
///
/// Building it on a thread with room and moving the boxed result back keeps the large
/// frame off the caller's stack entirely; only a pointer crosses back. The page data
/// itself lives on the heap, so nothing is copied twice.
fn load() -> Box<Unifont> {
    std::thread::Builder::new()
        .stack_size(LOADER_STACK)
        .spawn(|| Box::new(Unifont::open()))
        .expect("spawn the Unifont loader")
        .join()
        .expect("the Unifont loader panicked")
}

/// The glyph Unifont draws for `ch`.
///
/// Never returns `None` for a valid scalar: Unifont substitutes `U+FFFD` for
/// codepoints it has no glyph for, which is the replacement this tier wants anyway.
pub fn glyph(ch: char) -> Option<Glyph> {
    UNIFONT.with(|unifont| {
        let mut unifont = unifont.borrow_mut();
        let bitmap = unifont.load_bitmap(ch as u32);
        let bytes = bitmap.get_bytes();
        let mut rows = [0u16; CELL_H];
        if bitmap.is_wide() {
            for (row, pair) in rows.iter_mut().zip(bytes.chunks_exact(2)) {
                // Two bytes per row, leftmost pixel in the high bit of the first.
                *row = u16::from_be_bytes([pair[0], pair[1]]);
            }
            Some(Glyph::wide(rows))
        } else {
            let mut narrow = [0u8; CELL_H];
            narrow.copy_from_slice(bytes);
            Some(Glyph::narrow(narrow))
        }
    })
}
