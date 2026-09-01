//! Rasterising a whole page into a PNG file.
//!
//! The interactive surface is exactly as tall as the window, so it never holds more of
//! the page than the reader can see. A screenshot wants the opposite: every painted row,
//! whether it has been scrolled to or not. So this builds a second, page-tall [`Surface`]
//! from the same display list, the same glyph table and the same palette, and encodes its
//! pixels — the page as the frontend would draw it if the window were tall enough.
//!
//! Everything up to [`capture_page`] is pure; [`save_page`] and [`save_active_page`] are
//! the only file I/O outside `window`, and both take the directory to write into.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::app::App;
use crate::core::frontend::Frontend;
use crate::layout::LayoutRect;
use crate::paint::DisplayList;
use crate::ui::theme::{Theme, rgb_of};
use crate::ui::widgets::clip_width;
use crate::ui::widgets::content::span_style;

use super::font::{CELL_H, CELL_W};
use super::{SurfaceConfig, VgaBackend};

/// Where a screenshot lands, relative to the working directory.
pub const SCREENSHOT_DIR: &str = "screenshots";

/// How many pixels one capture may cover, so a very long page cannot ask for a buffer
/// that will not fit in memory. At a typical 160-column window this is about 1500 rows;
/// beyond it the capture stops at the top of the page and says so.
pub const MAX_PIXELS: usize = 32_000_000;

/// The page to rasterise, and the budget to do it within.
#[derive(Clone, Copy)]
pub struct PageCapture<'a> {
    pub painted: &'a DisplayList,
    pub theme: &'a Theme,
    pub cols: u16,
    pub max_pixels: usize,
}

/// A whole page rasterised, before it reaches a file.
pub struct CapturedPage {
    pub png: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// Rows the capture covers.
    pub rows: usize,
    /// Rows the page has, which is larger when [`PageCapture::max_pixels`] cut it short.
    pub page_rows: usize,
}

/// A capture that reached the disk.
pub struct SavedScreenshot {
    pub path: PathBuf,
    pub width: usize,
    pub height: usize,
    pub rows: usize,
    pub page_rows: usize,
}

impl SavedScreenshot {
    /// The status-bar line for this capture, which says when it is not the whole page.
    pub fn status(&self) -> String {
        let Self {
            path,
            width,
            height,
            rows,
            page_rows,
        } = self;
        if rows < page_rows {
            format!(
                "saved {} ({width}x{height}, first {rows} of {page_rows} rows)",
                path.display()
            )
        } else {
            format!("saved {} ({width}x{height})", path.display())
        }
    }
}

/// Everything [`save_page`] needs, as a struct: the directory, the frontend and the
/// timestamp are all that decide the file name, and two of them are easy to swap.
pub struct ScreenshotRequest<'a> {
    pub page: PageCapture<'a>,
    pub directory: &'a Path,
    pub frontend: Frontend,
    pub at: SystemTime,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("the page has no painted rows")]
    Empty,
    #[error("PNG encoding failed: {0}")]
    Encode(String),
}

/// Rasterise the whole painted page into PNG bytes.
pub fn capture_page(request: PageCapture<'_>) -> Result<CapturedPage, CaptureError> {
    let PageCapture {
        painted,
        theme,
        cols,
        max_pixels,
    } = request;
    let cols = cols.max(1);
    let page_rows = painted.len();
    if page_rows == 0 {
        return Err(CaptureError::Empty);
    }
    let per_row = usize::from(cols) * CELL_W * CELL_H;
    let budget = (max_pixels / per_row.max(1)).max(1);
    let rows = page_rows.min(budget).min(usize::from(u16::MAX)) as u16;

    let area = Rect::new(0, 0, cols, rows);
    let mut buffer = Buffer::empty(area);
    buffer.set_style(area, Style::default().fg(theme.text).bg(theme.bg));
    for row in 0..rows {
        let Some(painted_row) = painted.row(usize::from(row)) else {
            continue;
        };
        for span in &painted_row.spans {
            let Ok(col) = u16::try_from(span.col) else {
                break;
            };
            if col >= cols {
                break;
            }
            let clipped = clip_width(&span.text, cols - col);
            if clipped.is_empty() {
                continue;
            }
            buffer.set_string(col, row, &clipped, span_style(span, theme));
        }
    }

    let mut backend = VgaBackend::new(SurfaceConfig {
        cols,
        rows,
        scale: 1,
        default_fg: rgb_of(theme.text),
        default_bg: rgb_of(theme.bg),
    });
    let cells: Vec<(u16, u16, &Cell)> = (0..rows)
        .flat_map(|row| (0..cols).map(move |col| (col, row)))
        .map(|(col, row)| (col, row, &buffer[(col, row)]))
        .collect();
    backend.surface_mut().set_cells(cells.into_iter());
    backend.set_image_generation(0);
    backend.draw_overlays(
        painted,
        (0, 0),
        0,
        LayoutRect {
            col: 0,
            row: 0,
            width: usize::from(cols),
            height: usize::from(rows),
        },
        &[],
        theme.palette(),
    );

    let surface = backend.surface();
    let (width, height) = surface.pixel_size();
    Ok(CapturedPage {
        png: encode(surface.pixels(), width, height)?,
        width,
        height,
        rows: usize::from(rows),
        page_rows,
    })
}

/// Rasterise a page and write it into `directory`, creating that directory if needed.
pub fn save_page(request: ScreenshotRequest<'_>) -> io::Result<SavedScreenshot> {
    let ScreenshotRequest {
        page,
        directory,
        frontend,
        at,
    } = request;
    let captured =
        capture_page(page).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::create_dir_all(directory)?;
    let path = free_path(directory, &file_stem(frontend, at));
    fs::write(&path, &captured.png)?;
    Ok(SavedScreenshot {
        path,
        width: captured.width,
        height: captured.height,
        rows: captured.rows,
        page_rows: captured.page_rows,
    })
}

/// Capture the active tab's whole page and flash the outcome, saved or not.
///
/// The frontend calls this because it is the only part that knows which frontend it is —
/// and because `app` is I/O-free and may not write files itself.
pub fn save_active_page(app: &mut App, frontend: Frontend, directory: &Path) {
    let message = {
        let view = app.chrome_view();
        let cols = view.geometry.content_cols().min(usize::from(u16::MAX)) as u16;
        let outcome = save_page(ScreenshotRequest {
            page: PageCapture {
                painted: view.content.painted,
                theme: &view.theme,
                cols,
                max_pixels: MAX_PIXELS,
            },
            directory,
            frontend,
            at: SystemTime::now(),
        });
        match outcome {
            Ok(saved) => saved.status(),
            Err(error) => format!("screenshot failed: {error}"),
        }
    };
    app.flash(message);
}

/// The name a capture takes, without its extension: sortable, and stamped with the
/// frontend that drew it. The clock is UTC, because the standard library has no local
/// zone and a wrong offset is worse than a stated one.
pub fn file_stem(frontend: Frontend, at: SystemTime) -> String {
    let seconds = at
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let time = seconds % 86_400;
    let (hour, minute, second) = (time / 3_600, (time % 3_600) / 60, time % 60);
    format!(
        "{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}-{}",
        frontend.slug()
    )
}

/// The first free `<stem>.png`, `<stem>-2.png`, ... in `directory`, so two captures in
/// one second do not overwrite each other.
fn free_path(directory: &Path, stem: &str) -> PathBuf {
    let path = directory.join(format!("{stem}.png"));
    if !path.exists() {
        return path;
    }
    for suffix in 2..1_000 {
        let candidate = directory.join(format!("{stem}-{suffix}.png"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}

fn encode(pixels: &[u32], width: usize, height: usize) -> Result<Vec<u8>, CaptureError> {
    let mut rgb = Vec::with_capacity(width * height * 3);
    for pixel in pixels {
        rgb.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            &rgb,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| CaptureError::Encode(error.to_string()))?;
    Ok(png)
}

/// Howard Hinnant's `civil_from_days`: the calendar date `days` after 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}
