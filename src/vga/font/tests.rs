use sha2::{Digest, Sha256};

use crate::core::style::CellMetric;

use super::{
    CELL_H, CELL_W, CP437_8X16, CP437_8X16_SHA256, CP437_TO_UNICODE, Glyph, GlyphWidth,
    REPLACEMENT, UNICODE_TO_CP437, cp437_index, glyph,
};

#[test]
fn css_default_cell_metric_matches_the_vga_font_cell() {
    assert_eq!(usize::from(CellMetric::DEFAULT.column_px()), CELL_W);
    assert_eq!(usize::from(CellMetric::DEFAULT.row_px()), CELL_H);
}

/// Render a glyph as text rows so failures show the shape, not a bitmask.
fn render(glyph: Glyph) -> Vec<String> {
    (0..CELL_H)
        .map(|y| {
            (0..glyph.width().pixels())
                .map(|x| if glyph.pixel(x, y) { '#' } else { '.' })
                .collect()
        })
        .collect()
}

#[test]
fn generated_bitmaps_match_the_pinned_checksum() {
    let flat: Vec<u8> = CP437_8X16.iter().flatten().copied().collect();
    assert_eq!(flat.len(), 4096, "256 glyphs of 16 rows");
    let actual = Sha256::digest(&flat)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual, CP437_8X16_SHA256,
        "bitmap table changed; rerun tools/gen_cp437.py and review the diff"
    );
}

#[test]
fn reverse_table_is_sorted_and_unique() {
    // `cp437::index` binary-searches this table. Unsorted or duplicated entries make
    // the search silently miss rather than fail, so the invariant is asserted here.
    for pair in UNICODE_TO_CP437.windows(2) {
        assert!(
            pair[0].0 < pair[1].0,
            "not strictly sorted: U+{:04X} then U+{:04X}",
            pair[0].0 as u32,
            pair[1].0 as u32,
        );
    }
}

#[test]
fn reverse_table_excludes_ascii_which_the_fast_path_answers() {
    for &(scalar, _) in UNICODE_TO_CP437 {
        assert!(
            !(' '..='~').contains(&scalar),
            "U+{:04X} belongs to the ASCII fast path",
            scalar as u32,
        );
    }
}

#[test]
fn every_index_round_trips_through_its_character() {
    for (index, &scalar) in CP437_TO_UNICODE.iter().enumerate() {
        assert_eq!(
            cp437_index(scalar),
            Some(index as u8),
            "U+{:04X} should resolve back to CP437 {index}",
            scalar as u32,
        );
    }
}

#[test]
fn ascii_maps_to_itself() {
    for byte in 0x20u8..=0x7E {
        let scalar = byte as char;
        assert_eq!(cp437_index(scalar), Some(byte), "{scalar:?}");
    }
}

#[test]
fn box_drawing_resolves_to_the_dos_glyphs() {
    // The chrome is drawn entirely from these, so a wrong index is immediately visible
    // as a broken frame.
    for (scalar, expected) in [
        ('┌', 0xDAu8),
        ('─', 0xC4),
        ('┐', 0xBF),
        ('│', 0xB3),
        ('└', 0xC0),
        ('┘', 0xD9),
        ('├', 0xC3),
        ('┤', 0xB4),
        ('┼', 0xC5),
        ('█', 0xDB),
        ('▄', 0xDC),
        ('▀', 0xDF),
    ] {
        assert_eq!(cp437_index(scalar), Some(expected), "{scalar:?}");
    }
}

#[test]
fn the_vertical_rule_draws_a_stroke_on_every_row() {
    let rendered = render(glyph('│'));
    for (row, line) in rendered.iter().enumerate() {
        assert!(line.contains('#'), "row {row} of U+2502 is blank: {line}");
    }
}

#[test]
fn the_top_left_corner_opens_down_and_right() {
    // Rows above the joint are blank; the joint runs to the right edge; below it only
    // the stem continues. That is the shape, independent of which column the stem sits
    // in, so this survives a font swap that shifts the stem.
    let rendered = render(glyph('┌'));
    let joint = rendered
        .iter()
        .position(|line| line.matches('#').count() > 2)
        .expect("a horizontal arm");
    assert!(
        rendered[..joint].iter().all(|line| !line.contains('#')),
        "rows above the joint should be blank",
    );
    assert!(
        rendered[joint].ends_with('#'),
        "the arm should reach the right edge: {}",
        rendered[joint],
    );
    assert!(
        rendered[joint + 1..].iter().all(|line| line.contains('#')),
        "the stem should continue below the joint",
    );
}

#[test]
fn characters_outside_cp437_still_draw_something() {
    // These miss the code page, so they exercise the Unifont tier. Rendering them as
    // the replacement box would be a regression: curly quotes and dashes below are
    // ordinary punctuation on real pages.
    for scalar in ['漢', 'ᚠ', '\u{2019}', '—', 'д'] {
        assert_eq!(cp437_index(scalar), None, "{scalar:?} should miss CP437");
        let rendered = render(glyph(scalar));
        assert!(
            rendered.iter().any(|line| line.contains('#')),
            "{scalar:?} rendered blank",
        );
        assert_ne!(glyph(scalar), REPLACEMENT, "{scalar:?}");
    }
}

#[test]
fn cp437_wins_over_unifont_where_both_have_a_glyph() {
    // Tier order is what keeps the chrome in the DOS face rather than Unifont's.
    for scalar in ['A', '│', '┌', '█', 'α'] {
        let index = cp437_index(scalar).expect("covered by CP437");
        assert_eq!(
            glyph(scalar),
            Glyph::narrow(CP437_8X16[index as usize]),
            "{scalar:?} should come from the code page",
        );
    }
}

#[test]
fn wide_scripts_report_a_two_cell_glyph() {
    // Unifont draws CJK at 16x16; the surface relies on this to reserve two cells.
    assert_eq!(glyph('漢').width(), GlyphWidth::Wide);
    assert_eq!(glyph('あ').width(), GlyphWidth::Wide);
}

#[test]
fn lookup_never_panics_across_the_scalar_range() {
    // Unifont asserts on out-of-range codepoints and panics if a page is missing its
    // replacement glyph; walk a spread of planes to prove the wrapper is safe.
    for codepoint in (0u32..=0x10FFFF).step_by(0x40) {
        if let Some(scalar) = char::from_u32(codepoint) {
            let _ = glyph(scalar);
        }
    }
}

#[test]
fn narrow_rows_are_left_aligned() {
    // 0x80 is the leftmost pixel of an eight-bit row; left-aligning must put it at
    // bit 15 so narrow and wide glyphs share one blitting loop.
    let probe = Glyph::narrow([0x80; CELL_H]);
    assert!(
        probe.pixel(0, 0),
        "bit 7 of the source row is the left pixel"
    );
    assert!(!probe.pixel(1, 0));
    assert_eq!(probe.width(), GlyphWidth::Narrow);
    assert_eq!(probe.width().cells(), 1);
    assert_eq!(probe.width().pixels(), 8);
}

#[test]
fn wide_glyphs_span_two_cells() {
    let probe = Glyph::wide([0x8001; CELL_H]);
    assert_eq!(probe.width(), GlyphWidth::Wide);
    assert_eq!(probe.width().cells(), 2);
    assert!(probe.pixel(0, 0), "bit 15 is the left pixel");
    assert!(probe.pixel(15, 0), "bit 0 is the right pixel");
    assert!(!probe.pixel(7, 0));
}

#[test]
fn pixel_reads_outside_the_glyph_are_unset_rather_than_panicking() {
    // A caller blitting a two-cell span over a narrow glyph must get blank pixels.
    let probe = Glyph::narrow([0xFF; CELL_H]);
    assert!(probe.pixel(7, 0));
    assert!(!probe.pixel(8, 0), "past a narrow glyph's right edge");
    assert!(!probe.pixel(0, CELL_H), "past the last row");
    assert!(!probe.pixel(99, 99));
}

#[test]
fn the_blank_glyph_has_no_pixels() {
    assert!(render(glyph(' ')).iter().all(|line| !line.contains('#')));
}
