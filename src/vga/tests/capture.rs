use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::frontend::Frontend;
use crate::paint::DisplayList;
use crate::ui::theme::DEFAULT;
use crate::vga::capture::{
    CaptureError, PageCapture, ScreenshotRequest, capture_page, file_stem, save_page,
};
use crate::vga::font::{CELL_H, CELL_W};

const COLS: u16 = 20;

fn page(rows: usize) -> DisplayList {
    let lines = (0..rows)
        .map(|row| format!("line {row}"))
        .collect::<Vec<_>>();
    DisplayList::from_lines(&lines)
}

fn request(painted: &DisplayList, max_pixels: usize) -> PageCapture<'_> {
    PageCapture {
        painted,
        theme: &DEFAULT,
        cols: COLS,
        max_pixels,
    }
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn the_capture_covers_every_painted_row_not_just_a_screenful() {
    let painted = page(100);
    let captured = capture_page(request(&painted, 32_000_000)).expect("a capture");

    assert_eq!(captured.rows, 100);
    assert_eq!(captured.page_rows, 100);
    assert_eq!(captured.width, usize::from(COLS) * CELL_W);
    assert_eq!(captured.height, 100 * CELL_H);
}

#[test]
fn the_png_decodes_at_the_captured_size_with_ink_on_it() {
    let painted = page(4);
    let captured = capture_page(request(&painted, 32_000_000)).expect("a capture");

    let decoded = image::load_from_memory(&captured.png).expect("a readable PNG");
    assert_eq!(decoded.width() as usize, captured.width);
    assert_eq!(decoded.height() as usize, captured.height);

    let background = image::Rgba([
        DEFAULT.palette().background.r,
        DEFAULT.palette().background.g,
        DEFAULT.palette().background.b,
        255,
    ]);
    let rgba = decoded.to_rgba8();
    let ink = rgba.pixels().filter(|pixel| **pixel != background).count();
    assert!(ink > 0, "the glyphs should have painted something");
}

#[test]
fn a_page_past_the_pixel_budget_stops_at_the_top_and_says_so() {
    let painted = page(100);
    let budget = usize::from(COLS) * CELL_W * CELL_H * 10;
    let captured = capture_page(request(&painted, budget)).expect("a capture");

    assert_eq!(captured.rows, 10);
    assert_eq!(captured.page_rows, 100);
    assert_eq!(captured.height, 10 * CELL_H);
}

#[test]
fn a_page_with_no_rows_is_not_captured() {
    let painted = DisplayList::default();
    assert!(matches!(
        capture_page(request(&painted, 32_000_000)),
        Err(CaptureError::Empty)
    ));
}

#[test]
fn the_file_name_carries_the_utc_stamp_and_the_frontend() {
    assert_eq!(
        file_stem(Frontend::Vga, at(1_787_844_612)),
        "20260827-153012-vga"
    );
    assert_eq!(
        file_stem(Frontend::Terminal, at(1_787_844_612)),
        "20260827-153012-terminal"
    );
}

#[test]
fn the_file_name_survives_a_leap_day() {
    assert_eq!(
        file_stem(Frontend::Vga, at(1_709_164_801)),
        "20240229-000001-vga"
    );
}

#[test]
fn two_captures_in_one_second_both_survive() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let painted = page(3);
    let save = || {
        save_page(ScreenshotRequest {
            page: request(&painted, 32_000_000),
            directory: directory.path(),
            frontend: Frontend::Vga,
            at: at(1_787_844_612),
        })
        .expect("a saved screenshot")
    };

    let first = save();
    let second = save();

    assert_eq!(first.path, directory.path().join("20260827-153012-vga.png"));
    assert_eq!(
        second.path,
        directory.path().join("20260827-153012-vga-2.png")
    );
    assert!(first.path.exists() && second.path.exists());
}

#[test]
fn the_status_line_reports_a_truncated_capture() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let painted = page(100);
    let budget = usize::from(COLS) * CELL_W * CELL_H * 10;
    let saved = save_page(ScreenshotRequest {
        page: request(&painted, budget),
        directory: directory.path(),
        frontend: Frontend::Terminal,
        at: at(1_787_844_612),
    })
    .expect("a saved screenshot");

    assert!(
        saved.status().ends_with("(160x160, first 10 of 100 rows)"),
        "unexpected status: {}",
        saved.status()
    );
}

#[test]
fn the_directory_is_created_on_demand() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let directory = root.path().join("screenshots");
    let painted = page(2);
    let saved = save_page(ScreenshotRequest {
        page: request(&painted, 32_000_000),
        directory: &directory,
        frontend: Frontend::Vga,
        at: at(1_787_844_612),
    })
    .expect("a saved screenshot");

    assert!(saved.path.starts_with(&directory));
    assert!(saved.path.exists());
}
