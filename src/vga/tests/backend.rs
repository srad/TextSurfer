use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, TestBackend};
use ratatui::buffer::Cell;
use ratatui::layout::Position;

use winit::dpi::PhysicalPosition;

use crate::core::dom::{Document, ElementNs, NodeId};
use crate::core::geom::{Point, Size};
use crate::core::style::{CellStyle, Palette, Rgb, Rgba};
use crate::layout::LayoutRect;
use crate::paint::ScaledTextRun;
use crate::ui::chrome;
use crate::ui::test_util::draft_view;
use crate::vga::backend::{VgaBackend, cell_at, grid_for, pixel_size};
use crate::vga::font::{CELL_H, CELL_W};
use crate::vga::{SurfaceConfig, VgaOptions};

const FG: Rgb = Rgb::new(200, 200, 200);
const BG: Rgb = Rgb::new(0, 0, 128);

/// The chrome fixture's own geometry, so the widgets lay out as they were written to.
const VIEW: Size = Size { cols: 60, rows: 10 };

fn config(size: Size) -> SurfaceConfig {
    SurfaceConfig {
        cols: size.cols,
        rows: size.rows,
        scale: 1,
        default_fg: FG,
        default_bg: BG,
    }
}

fn backend(size: Size) -> VgaBackend {
    VgaBackend::new(config(size))
}

fn packed(colour: Rgb) -> u32 {
    (u32::from(colour.r) << 16) | (u32::from(colour.g) << 8) | u32::from(colour.b)
}

/// Whether any pixel in a cell differs from the plain background.
fn cell_is_marked(backend: &VgaBackend, col: u16, row: u16) -> bool {
    let surface = backend.surface();
    let (width, _) = surface.pixel_size();
    let plain = packed(BG);
    (0..CELL_H).any(|y| {
        (0..CELL_W).any(|x| {
            let at = (row as usize * CELL_H + y) * width + col as usize * CELL_W + x;
            surface.pixels()[at] != plain
        })
    })
}

fn cell(ch: char) -> Cell {
    let mut cell = Cell::default();
    cell.set_char(ch);
    cell
}

fn scaled_run(node: NodeId, col: usize, row: usize) -> ScaledTextRun {
    ScaledTextRun {
        node,
        rect: LayoutRect {
            col,
            row,
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
    }
}

#[test]
fn size_reports_the_cell_grid() {
    assert_eq!(backend(VIEW).size().unwrap().width, 60);
    assert_eq!(backend(VIEW).size().unwrap().height, 10);
}

#[test]
fn window_size_reports_real_pixels_unlike_a_terminal() {
    let size = backend(VIEW).window_size().unwrap();
    assert_eq!(size.columns_rows.width, 60);
    assert_eq!(size.pixels.width, 60 * CELL_W as u16);
    assert_eq!(size.pixels.height, 10 * CELL_H as u16);
}

#[test]
fn draw_paints_the_cells_it_is_given() {
    let mut backend = backend(VIEW);
    let glyph = cell('█');
    backend.draw([(2u16, 1u16, &glyph)].into_iter()).unwrap();
    assert!(cell_is_marked(&backend, 2, 1));
    assert!(
        !cell_is_marked(&backend, 3, 1),
        "untouched cell stays blank"
    );
}

#[test]
fn the_cursor_shows_as_a_reverse_video_block() {
    let mut backend = backend(VIEW);
    backend
        .set_cursor_position(Position { x: 4, y: 2 })
        .unwrap();
    assert!(!cell_is_marked(&backend, 4, 2), "hidden by default");

    backend.show_cursor().unwrap();
    assert!(
        cell_is_marked(&backend, 4, 2),
        "a blank cell inverts to solid"
    );
    assert_eq!(backend.surface().cursor(), Some((4, 2)));

    backend.hide_cursor().unwrap();
    assert!(!cell_is_marked(&backend, 4, 2));
    assert_eq!(backend.surface().cursor(), None);
}

#[test]
fn moving_the_cursor_leaves_no_trail() {
    let mut backend = backend(VIEW);
    backend.show_cursor().unwrap();
    backend
        .set_cursor_position(Position { x: 1, y: 0 })
        .unwrap();
    backend
        .set_cursor_position(Position { x: 5, y: 0 })
        .unwrap();
    assert!(!cell_is_marked(&backend, 1, 0), "old position repainted");
    assert!(cell_is_marked(&backend, 5, 0));
    assert_eq!(
        backend.get_cursor_position().unwrap(),
        Position { x: 5, y: 0 }
    );
}

#[test]
fn clear_blanks_every_cell() {
    let mut backend = backend(VIEW);
    let glyph = cell('█');
    backend.draw([(0u16, 0u16, &glyph)].into_iter()).unwrap();
    backend.clear().unwrap();
    assert!(!cell_is_marked(&backend, 0, 0));
}

#[test]
fn clear_region_after_cursor_leaves_earlier_cells_alone() {
    let mut backend = backend(VIEW);
    let glyph = cell('█');
    let filled: Vec<(u16, u16, &Cell)> = (0..6).map(|col| (col, 0u16, &glyph)).collect();
    backend.draw(filled.into_iter()).unwrap();
    backend
        .set_cursor_position(Position { x: 3, y: 0 })
        .unwrap();
    backend.clear_region(ClearType::AfterCursor).unwrap();

    assert!(cell_is_marked(&backend, 2, 0), "before the cursor survives");
    assert!(cell_is_marked(&backend, 3, 0), "the cursor cell survives");
    assert!(
        !cell_is_marked(&backend, 4, 0),
        "after the cursor is cleared"
    );
}

#[test]
fn clear_region_current_line_spares_other_rows() {
    let mut backend = backend(VIEW);
    let glyph = cell('█');
    backend
        .draw([(0u16, 0u16, &glyph), (0u16, 1u16, &glyph)].into_iter())
        .unwrap();
    backend
        .set_cursor_position(Position { x: 0, y: 1 })
        .unwrap();
    backend.clear_region(ClearType::CurrentLine).unwrap();
    assert!(cell_is_marked(&backend, 0, 0), "row 0 untouched");
    assert!(!cell_is_marked(&backend, 0, 1));
}

#[test]
fn the_real_chrome_renders_the_same_cells_as_the_terminal_backend() {
    // The headline check: drive the actual `ui::chrome::draw` through both backends and
    // require they agree, cell by cell, on what is blank. That proves the framebuffer
    // reaches the whole existing UI rather than a mock of it — and it needs no window.
    let view = draft_view();

    let mut reference = Terminal::new(TestBackend::new(VIEW.cols, VIEW.rows)).unwrap();
    reference.draw(|frame| chrome::draw(frame, &view)).unwrap();
    let expected = reference.backend().buffer().clone();

    let mut terminal = Terminal::new(backend(VIEW)).unwrap();
    terminal.draw(|frame| chrome::draw(frame, &view)).unwrap();
    let actual = terminal.backend();

    let mut disagreements = Vec::new();
    for row in 0..VIEW.rows {
        for col in 0..VIEW.cols {
            let reference_cell = &expected[(col, row)];
            // A cell is visibly marked if it draws a glyph or tints its background.
            let reference_marked = reference_cell.symbol() != " "
                || reference_cell.bg != ratatui::style::Color::Rgb(BG.r, BG.g, BG.b);
            if reference_marked != cell_is_marked(actual, col, row) {
                disagreements.push(format!(
                    "({col}, {row}) {:?} reference={reference_marked}",
                    reference_cell.symbol(),
                ));
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "backends disagree on {} cells:\n{}",
        disagreements.len(),
        disagreements.join("\n"),
    );
}

#[test]
fn the_real_chrome_paints_something_in_every_row() {
    // Guards against the previous test passing vacuously if both backends drew nothing.
    let view = draft_view();
    let mut terminal = Terminal::new(backend(VIEW)).unwrap();
    terminal.draw(|frame| chrome::draw(frame, &view)).unwrap();
    let backend = terminal.backend();
    for row in 0..VIEW.rows {
        assert!(
            (0..VIEW.cols).any(|col| cell_is_marked(backend, col, row)),
            "row {row} is entirely blank",
        );
    }
}

#[test]
fn scaled_text_lands_on_the_content_rows_the_chrome_reserved() {
    // The overlay's origin and clip come from `chrome::content_rect`, so a scaled glyph
    // must sit on the same cells the terminal backend reserved for it — including the
    // first and last visible rows, which an inset rect would push off or clip away.
    let mut document = Document::new();
    let node = document.insert_element(None, "h1", ElementNs::Html, vec![]);
    let view = draft_view();
    let mut terminal = Terminal::new(backend(VIEW)).unwrap();
    terminal.draw(|frame| chrome::draw(frame, &view)).unwrap();
    let area = ratatui::layout::Rect::new(0, 0, VIEW.cols, VIEW.rows);
    let content = chrome::content_rect(&view, area).expect("a content rect");
    let clip = LayoutRect {
        col: usize::from(content.x),
        row: usize::from(content.y),
        width: usize::from(content.width),
        height: usize::from(content.height),
    };
    // Anchored to the band the `Content` widget draws, not to the rect under test, so a
    // wrong origin cannot move the expectation with it.
    let band = view.geometry.layout(area).content.expect("a content band");
    let last_pair = usize::from(band.height) - 2;
    let runs = [scaled_run(node, 10, 0), scaled_run(node, 20, last_pair)];
    terminal.backend_mut().draw_scaled_text(
        &runs,
        (content.x, content.y),
        0,
        clip,
        &[],
        Palette {
            text: FG,
            background: BG,
            link: FG,
            link_hover: FG,
        },
    );

    let backend = terminal.backend();
    assert!(
        cell_is_marked(backend, 11, band.y),
        "the top glyph starts on the widget's first content row"
    );
    assert!(
        cell_is_marked(backend, 11, band.y + 1),
        "the top glyph is two rows tall"
    );
    assert!(
        cell_is_marked(backend, 21, band.bottom() - 1),
        "the bottom glyph reaches the widget's last content row"
    );
}

#[test]
fn chrome_pixels_golden() {
    // A pixel-level golden of the tab well, dumped as text so a regression shows which
    // pixels moved rather than a changed hash.
    //
    // Cell row 0 is skipped deliberately: the menu bar paints a solid grey field, so
    // every one of its pixels differs from the background and the dump would be an
    // uninformative block of `#`.
    const FIRST_ROW: usize = 1;
    let view = draft_view();
    let mut terminal = Terminal::new(backend(VIEW)).unwrap();
    terminal.draw(|frame| chrome::draw(frame, &view)).unwrap();
    let surface = terminal.backend().surface();
    let (width, _) = surface.pixel_size();

    let dump: String = (FIRST_ROW * CELL_H..(FIRST_ROW + 3) * CELL_H)
        .map(|y| {
            let row: String = (0..18 * CELL_W)
                .map(|x| {
                    if surface.pixels()[y * width + x] == packed(BG) {
                        '.'
                    } else {
                        '#'
                    }
                })
                .collect();
            format!("{row}\n")
        })
        .collect();
    insta::assert_snapshot!(dump);
}

/// The stack Windows gives the main thread. The test harness runs tests on spawned
/// threads with a larger stack, so without pinning this a frontend that only overflows
/// on the real main thread passes every test and then crashes on launch — which is
/// exactly what happened.
const MAIN_THREAD_STACK: usize = 1024 * 1024;

#[test]
fn the_frontend_starts_within_a_main_thread_stack() {
    // Mirrors what `window::VgaApp::resumed` does at launch, minus the window: build
    // the backend at the real default size and draw the chrome once. Unifont is reached
    // because the toolbar's glyphs are outside CP437.
    //
    // A stack overflow aborts the process rather than failing this assertion, so a
    // regression here takes the whole test run down. That is the intended signal.
    let handle = std::thread::Builder::new()
        .stack_size(MAIN_THREAD_STACK)
        .spawn(|| {
            let cells = VgaOptions::default().cells;
            let view = draft_view();
            let mut terminal = Terminal::new(backend(cells)).unwrap();
            terminal.draw(|frame| chrome::draw(frame, &view)).unwrap();
            terminal.backend().surface().pixels().len()
        })
        .expect("spawn");
    let painted = handle.join().expect("the frontend's first draw");
    let (width, height) = pixel_size(VgaOptions::default().cells, 1);
    assert_eq!(painted, width * height);
}

#[test]
fn a_window_maps_to_the_cells_that_fit_it() {
    assert_eq!(
        grid_for(1280, 800, 1),
        Size {
            cols: 160,
            rows: 50
        }
    );
    assert_eq!(grid_for(1280, 800, 2), Size { cols: 80, rows: 25 });
    // Partial cells are dropped rather than half-drawn.
    assert_eq!(
        grid_for(1284, 807, 1),
        Size {
            cols: 160,
            rows: 50
        }
    );
}

#[test]
fn a_degenerate_window_still_yields_a_drawable_grid() {
    // ratatui panics on a zero-sized area, and a window legitimately reports zero
    // while it is being resized or minimised.
    assert_eq!(grid_for(0, 0, 1), Size { cols: 1, rows: 1 });
    assert_eq!(grid_for(3, 3, 1), Size { cols: 1, rows: 1 });
    assert_eq!(
        grid_for(1280, 800, 0),
        Size {
            cols: 160,
            rows: 50
        }
    );
}

#[test]
fn a_pixel_position_maps_to_the_cell_it_is_inside() {
    let cell = |x: f64, y: f64, scale: usize| cell_at(PhysicalPosition::new(x, y), scale);
    assert_eq!(cell(0.0, 0.0, 1), Point { col: 0, row: 0 });
    assert_eq!(cell(7.9, 15.9, 1), Point { col: 0, row: 0 });
    assert_eq!(cell(8.0, 16.0, 1), Point { col: 1, row: 1 });
    assert_eq!(cell(8.0, 16.0, 2), Point { col: 0, row: 0 });
    assert_eq!(cell(16.0, 32.0, 2), Point { col: 1, row: 1 });
    // Negative coordinates arrive when a drag leaves the window.
    assert_eq!(cell(-4.0, -9.0, 1), Point { col: 0, row: 0 });
}

#[test]
fn every_cell_of_the_grid_round_trips_through_its_pixels() {
    for scale in 1..=4usize {
        let grid = grid_for(1280, 800, scale);
        for (col, row) in [(0u16, 0u16), (1, 1), (grid.cols - 1, grid.rows - 1)] {
            let x = f64::from(col) * (CELL_W * scale) as f64;
            let y = f64::from(row) * (CELL_H * scale) as f64;
            assert_eq!(
                cell_at(PhysicalPosition::new(x, y), scale),
                Point { col, row },
                "scale {scale}"
            );
        }
    }
}

#[test]
fn the_margin_past_the_grid_stays_outside_it() {
    // 1284x807 fits 160x50 cells and leaves a margin; a click in it must not be pinned
    // to the last row, or the frame's edge would swallow it.
    let scale = 1;
    let grid = grid_for(1284, 807, scale);
    let at = cell_at(PhysicalPosition::new(1283.0, 806.0), scale);
    assert!(at.col >= grid.cols || at.row >= grid.rows);
    let geometry = crate::ui::mouse::ChromeGeometry::for_size(grid);
    assert_eq!(geometry.zone_at(at), crate::ui::mouse::MouseZone::Outside);
}
