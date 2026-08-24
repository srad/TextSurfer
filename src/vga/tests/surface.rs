use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};

use crate::core::dom::{Document, ElementNs};
use crate::core::geom::Size;
use crate::core::style::{CellStyle, Palette, Rgb, Rgba};
use crate::layout::LayoutRect;
use crate::paint::ScaledTextRun;

use crate::vga::font::{CELL_H, CELL_W, CP437_TO_UNICODE, GlyphWidth, glyph};
use crate::vga::{Surface, SurfaceConfig};

const FG: Rgb = Rgb::new(200, 200, 200);
const BG: Rgb = Rgb::new(0, 0, 128);

fn config(cols: u16, rows: u16, scale: usize) -> SurfaceConfig {
    SurfaceConfig {
        cols,
        rows,
        scale,
        default_fg: FG,
        default_bg: BG,
    }
}

fn surface() -> Surface {
    Surface::new(config(4, 2, 1))
}

fn cell(ch: char, style: Style) -> Cell {
    let mut cell = Cell::default();
    cell.set_char(ch);
    cell.set_style(style);
    cell
}

fn packed(colour: Rgb) -> u32 {
    (u32::from(colour.r) << 16) | (u32::from(colour.g) << 8) | u32::from(colour.b)
}

/// Read one physical pixel.
fn pixel_at(surface: &Surface, x: usize, y: usize) -> u32 {
    let (width, _) = surface.pixel_size();
    surface.pixels()[y * width + x]
}

/// Render one cell as text rows, so a failure shows the shape.
fn cell_rows(surface: &Surface, col: usize, row: usize) -> Vec<String> {
    let fg = packed(FG);
    (0..CELL_H)
        .map(|y| {
            (0..CELL_W)
                .map(|x| {
                    let at = pixel_at(surface, col * CELL_W + x, row * CELL_H + y);
                    if at == fg { '#' } else { '.' }
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_new_surface_is_painted_with_the_default_background() {
    let surface = surface();
    assert_eq!(surface.pixel_size(), (4 * CELL_W, 2 * CELL_H));
    assert!(surface.pixels().iter().all(|&px| px == packed(BG)));
}

#[test]
fn setting_a_cell_paints_the_glyph_in_the_foreground() {
    let mut surface = surface();
    surface.set_cell(0, 0, &cell('A', Style::default()));
    let rows = cell_rows(&surface, 0, 0);
    assert!(
        rows.iter().any(|line| line.contains('#')),
        "the glyph should have painted something: {rows:?}",
    );
    // Cells the update did not touch keep the background.
    assert_eq!(pixel_at(&surface, 3 * CELL_W, 0), packed(BG));
}

#[test]
fn reset_resolves_to_the_theme_rather_than_black() {
    // rgb_of maps Color::Reset to black; using it directly would punch black holes
    // through the DOS field.
    let surface = surface();
    let resolved = surface.resolve(&cell('x', Style::default()));
    assert_eq!(resolved.fg, FG);
    assert_eq!(resolved.bg, BG);
    assert_ne!(resolved.bg, Rgb::BLACK);
}

#[test]
fn bold_applies_the_vga_intensity_bit() {
    // The CGA base palette brightens by +85 per channel: dark red becomes bright red.
    let surface = surface();
    let style = Style::default()
        .fg(Color::Rgb(170, 0, 0))
        .add_modifier(Modifier::BOLD);
    assert_eq!(surface.resolve(&cell('x', style)).fg, Rgb::new(255, 85, 85));
}

#[test]
fn bold_saturates_rather_than_wrapping() {
    let surface = surface();
    let style = Style::default()
        .fg(Color::Rgb(250, 250, 250))
        .add_modifier(Modifier::BOLD);
    assert_eq!(
        surface.resolve(&cell('x', style)).fg,
        Rgb::new(255, 255, 255)
    );
}

#[test]
fn reverse_swaps_foreground_and_background() {
    let surface = surface();
    let style = Style::default()
        .fg(Color::Rgb(1, 2, 3))
        .bg(Color::Rgb(4, 5, 6))
        .add_modifier(Modifier::REVERSED);
    let resolved = surface.resolve(&cell('x', style));
    assert_eq!(resolved.fg, Rgb::new(4, 5, 6));
    assert_eq!(resolved.bg, Rgb::new(1, 2, 3));
}

#[test]
fn bold_brightens_before_reverse_swaps() {
    // Order matters: bold is an intensity bit on the foreground, so it must apply to
    // the authored foreground and then travel into the background on reverse.
    let surface = surface();
    let style = Style::default()
        .fg(Color::Rgb(170, 0, 0))
        .bg(Color::Rgb(0, 0, 0))
        .add_modifier(Modifier::BOLD | Modifier::REVERSED);
    assert_eq!(surface.resolve(&cell('x', style)).bg, Rgb::new(255, 85, 85));
}

#[test]
fn underline_fills_its_row_even_under_a_blank_glyph() {
    let mut surface = surface();
    let style = Style::default().add_modifier(Modifier::UNDERLINED);
    surface.set_cell(0, 0, &cell(' ', style));
    let rows = cell_rows(&surface, 0, 0);
    assert_eq!(rows[14], "########", "row 14 should be the underline");
    assert_eq!(rows[13], "........");
    assert_eq!(rows[15], "........");
}

#[test]
fn scale_multiplies_every_pixel_into_a_block() {
    let mut surface = Surface::new(config(2, 1, 3));
    assert_eq!(surface.pixel_size(), (2 * CELL_W * 3, CELL_H * 3));
    surface.set_cell(0, 0, &cell('█', Style::default()));
    // The full block lights every pixel of its cell, at every scale step.
    for y in 0..CELL_H * 3 {
        for x in 0..CELL_W * 3 {
            assert_eq!(pixel_at(&surface, x, y), packed(FG), "at ({x}, {y})");
        }
    }
}

#[test]
fn writes_outside_the_grid_are_ignored() {
    let mut surface = surface();
    surface.set_cell(9, 0, &cell('A', Style::default()));
    surface.set_cell(0, 9, &cell('A', Style::default()));
    assert!(surface.pixels().iter().all(|&px| px == packed(BG)));
}

#[test]
fn resize_reshapes_the_buffer_and_clears_it() {
    let mut surface = surface();
    surface.set_cell(0, 0, &cell('█', Style::default()));
    surface.resize(Size { cols: 2, rows: 3 });
    assert_eq!(surface.size(), Size { cols: 2, rows: 3 });
    assert_eq!(surface.pixel_size(), (2 * CELL_W, 3 * CELL_H));
    assert!(surface.pixels().iter().all(|&px| px == packed(BG)));
}

#[test]
fn clear_returns_every_cell_to_the_background() {
    let mut surface = surface();
    surface.set_cell(1, 1, &cell('█', Style::default()));
    surface.clear();
    assert!(surface.pixels().iter().all(|&px| px == packed(BG)));
}

#[test]
fn a_neighbour_update_does_not_erase_an_adjacent_glyph() {
    // The sparse diff delivers single cells. Painting one must leave its neighbours
    // intact; this is the same repaint path that keeps wide glyphs whole.
    let mut surface = surface();
    surface.set_cell(0, 0, &cell('█', Style::default()));
    surface.set_cell(1, 0, &cell('A', Style::default()));
    for y in 0..CELL_H {
        for x in 0..CELL_W {
            assert_eq!(pixel_at(&surface, x, y), packed(FG), "block at ({x}, {y})");
        }
    }
}

#[test]
fn every_cp437_glyph_is_narrow() {
    // The DOS tier is single-width throughout; only Unifont supplies wide glyphs.
    for &scalar in CP437_TO_UNICODE.iter() {
        assert_eq!(
            glyph(scalar).width(),
            GlyphWidth::Narrow,
            "U+{:04X}",
            scalar as u32,
        );
    }
}

/// Whether any pixel in a cell is foreground.
fn cell_lit(surface: &Surface, col: usize, row: usize) -> bool {
    let (width, _) = surface.pixel_size();
    let fg = packed(FG);
    (0..CELL_H).any(|y| {
        (0..CELL_W).any(|x| surface.pixels()[(row * CELL_H + y) * width + col * CELL_W + x] == fg)
    })
}

#[test]
fn a_wide_glyph_covers_both_of_its_cells() {
    let mut surface = surface();
    surface.set_cell(0, 0, &cell('漢', Style::default()));
    assert!(cell_lit(&surface, 0, 0), "left half");
    assert!(cell_lit(&surface, 1, 0), "right half");
    assert!(!cell_lit(&surface, 2, 0), "and no further");
}

#[test]
fn a_wide_glyph_survives_an_update_to_its_second_cell() {
    // ratatui stores a double-width character in one cell and leaves the next blank,
    // and its sparse diff can deliver that blank on its own. Painting it naively would
    // slice the glyph down the middle.
    let mut surface = surface();
    surface.set_cell(0, 0, &cell('漢', Style::default()));
    let before: Vec<u32> = surface.pixels().to_vec();

    surface.set_cell(1, 0, &cell(' ', Style::default()));

    assert!(cell_lit(&surface, 0, 0), "left half survives");
    assert!(cell_lit(&surface, 1, 0), "right half survives");
    assert_eq!(
        surface.pixels(),
        before.as_slice(),
        "nothing changed at all"
    );
}

#[test]
fn a_wide_glyph_at_the_last_column_is_clipped_not_overrun() {
    // The grid is 4 wide; a wide glyph in column 3 has nowhere to put its right half.
    let mut surface = surface();
    surface.set_cell(3, 0, &cell('漢', Style::default()));
    assert!(cell_lit(&surface, 3, 0));
    // Must not have wrapped onto the next row's first cell.
    assert!(!cell_lit(&surface, 0, 1));
}

#[test]
fn replacing_a_wide_glyph_clears_its_second_cell() {
    let mut surface = surface();
    surface.set_cell(0, 0, &cell('漢', Style::default()));
    surface.set_cell(0, 0, &cell('.', Style::default()));
    assert!(cell_lit(&surface, 0, 0), "the narrow glyph is there");
    assert!(!cell_lit(&surface, 1, 0), "the old right half is gone");
}

#[test]
fn scaled_overlay_uses_integer_cells_and_restores_the_shadow_grid() {
    let mut document = Document::new();
    let node = document.insert_element(None, "h1", ElementNs::Html, vec![]);
    let mut surface = Surface::new(config(6, 4, 1));
    let run = ScaledTextRun {
        node,
        rect: LayoutRect {
            col: 1,
            row: 1,
            width: 2,
            height: 2,
        },
        text: "A".to_string(),
        style: CellStyle {
            fg: Some(Rgba::opaque(FG)),
            bg: Some(BG),
            scale: 2,
            ..Default::default()
        },
        depth: 0,
        ink: true,
    };
    surface.draw_scaled_text(
        &[run],
        (0, 0),
        0,
        LayoutRect {
            col: 0,
            row: 0,
            width: 6,
            height: 4,
        },
        &[],
        Palette {
            text: FG,
            background: BG,
            link: FG,
        },
    );
    assert!(cell_lit(&surface, 1, 1));
    assert!(cell_lit(&surface, 2, 2));
    let dump = (CELL_H..CELL_H * 3)
        .map(|y| {
            let row = (CELL_W..CELL_W * 3)
                .map(|x| {
                    if pixel_at(&surface, x, y) == packed(FG) {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect::<String>();
            format!("{row}\n")
        })
        .collect::<String>();
    insta::assert_snapshot!(dump);
    surface.clear_scaled_overlay();
    assert!(surface.pixels().iter().all(|pixel| *pixel == packed(BG)));
}

#[test]
fn dim_blends_before_reverse_and_strike_reaches_the_surface() {
    let surface = surface();
    let style = Style::default()
        .fg(Color::Rgb(200, 200, 200))
        .bg(Color::Rgb(0, 0, 100))
        .add_modifier(Modifier::DIM | Modifier::REVERSED | Modifier::CROSSED_OUT);
    let state = surface.resolve(&cell(' ', style));
    assert_eq!(state.fg, Rgb::new(0, 0, 100));
    assert_eq!(state.bg, Rgb::new(100, 100, 150));
    assert!(state.strike);
}

#[test]
fn partially_scrolled_scaled_glyphs_are_clipped_whole() {
    let mut document = Document::new();
    let node = document.insert_element(None, "h1", ElementNs::Html, vec![]);
    let mut surface = Surface::new(config(4, 3, 1));
    let run = ScaledTextRun {
        node,
        rect: LayoutRect {
            col: 0,
            row: 0,
            width: 2,
            height: 2,
        },
        text: "A".to_string(),
        style: CellStyle {
            fg: Some(Rgba::opaque(FG)),
            scale: 2,
            ..Default::default()
        },
        depth: 0,
        ink: true,
    };
    surface.draw_scaled_text(
        &[run],
        (0, 0),
        1,
        LayoutRect {
            col: 0,
            row: 0,
            width: 4,
            height: 3,
        },
        &[],
        Palette {
            text: FG,
            background: BG,
            link: FG,
        },
    );
    assert!(surface.pixels().iter().all(|pixel| *pixel == packed(BG)));
}

#[test]
fn scaled_overlay_obeys_occlusion_and_repaints_the_cursor_last() {
    let mut document = Document::new();
    let node = document.insert_element(None, "h1", ElementNs::Html, vec![]);
    let run = ScaledTextRun {
        node,
        rect: LayoutRect {
            col: 1,
            row: 1,
            width: 2,
            height: 2,
        },
        text: "A".to_string(),
        style: CellStyle {
            fg: Some(Rgba::opaque(FG)),
            scale: 2,
            ..Default::default()
        },
        depth: 0,
        ink: true,
    };
    let clip = LayoutRect {
        col: 0,
        row: 0,
        width: 4,
        height: 4,
    };
    let palette = Palette {
        text: FG,
        background: BG,
        link: FG,
    };
    let mut surface = Surface::new(config(4, 4, 1));
    surface.draw_scaled_text(
        std::slice::from_ref(&run),
        (0, 0),
        0,
        clip,
        &[run.rect],
        palette,
    );
    assert!(surface.pixels().iter().all(|pixel| *pixel == packed(BG)));
    surface.set_cursor(Some((1, 1)));
    surface.draw_scaled_text(std::slice::from_ref(&run), (0, 0), 0, clip, &[], palette);
    for y in CELL_H..CELL_H * 2 {
        for x in CELL_W..CELL_W * 2 {
            assert_eq!(pixel_at(&surface, x, y), packed(FG));
        }
    }
}
