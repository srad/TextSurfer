//! The CP437 tier: mapping a character onto one of the code page's 256 glyphs.

use super::Glyph;
use super::table::{CP437_8X16, UNICODE_TO_CP437};

/// The CP437 index that draws `ch`, if the code page covers it.
///
/// ASCII answers directly — it is the overwhelming majority of characters and CP437
/// is identical to ASCII across `0x20..=0x7E`. Everything else binary-searches the
/// generated table, which the generator emits sorted by scalar.
pub fn index(ch: char) -> Option<u8> {
    if ch.is_ascii_graphic() || ch == ' ' {
        return Some(ch as u8);
    }
    UNICODE_TO_CP437
        .binary_search_by_key(&ch, |&(scalar, _)| scalar)
        .ok()
        .map(|found| UNICODE_TO_CP437[found].1)
}

/// The glyph that draws `ch`, if CP437 covers it.
pub fn glyph(ch: char) -> Option<Glyph> {
    index(ch).map(|found| Glyph::narrow(CP437_8X16[found as usize]))
}
