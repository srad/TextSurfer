use std::collections::HashSet;
use std::fs;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use textsurfer::core::geom::Size;
use textsurfer::core::image::ImageDecoder;
use textsurfer::core::style::{RenderContext, RenderMetrics};
use textsurfer::css::ColorScheme;
use textsurfer::net::FetchResponse;
use textsurfer::paint::DisplayList;
use textsurfer::pipeline::image::RasterImageDecoder;
use textsurfer::pipeline::page_load::{PageLoad, PageLoadOptions};
use textsurfer::ui::PAPER_WHITE;
use textsurfer::ui::widgets::content::{Content, ContentLines};
use url::Url;

use super::manifest::{
    Case, CaseKind, CaseOutcome, Manifest, OracleProfile, Relation, corpus_root, local_url_path,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct VisualGrid {
    columns: u16,
    rows: u16,
    cells: Vec<Cell>,
}

impl VisualGrid {
    fn has_authored_background(&self) -> bool {
        self.cells.iter().any(|cell| {
            cell.style()
                .bg
                .is_some_and(|background| background != PAPER_WHITE.bg)
        })
    }
}

pub fn evaluate_case(manifest: &Manifest, case: &Case) -> CaseOutcome {
    match evaluate_case_inner(manifest, case) {
        Ok(outcome) => outcome,
        Err(error) => CaseOutcome::HarnessError(error),
    }
}

fn evaluate_case_inner(manifest: &Manifest, case: &Case) -> Result<CaseOutcome, String> {
    if case.profile == OracleProfile::VgaPixelV1 {
        return evaluate_vga_case(manifest, case);
    }
    let allowed = allowed_files(case);
    match case.kind {
        CaseKind::Crashtest => {
            let _ = render_grid(manifest, &case.path, &allowed)?;
            render_vga(manifest, &case.path, &allowed)?;
            Ok(CaseOutcome::Pass)
        }
        CaseKind::Reftest => {
            let test = render_grid(manifest, &case.path, &allowed)?;
            if case
                .capabilities
                .iter()
                .any(|capability| capability == "authored-background-sentinel")
                && !test.has_authored_background()
            {
                return Err(format!(
                    "{} did not paint its authored-background sentinel",
                    case.path
                ));
            }
            render_vga(manifest, &case.path, &allowed)?;
            let mut matched = false;
            let mut mismatch_equal = Vec::new();
            let mut first_difference = None;
            for reference in &case.references {
                let expected = render_grid(manifest, &reference.path, &allowed)?;
                render_vga(manifest, &reference.path, &allowed)?;
                let equal = test == expected;
                match reference.relation {
                    Relation::Match if equal => matched = true,
                    Relation::Match => {
                        first_difference.get_or_insert_with(|| {
                            format!(
                                "match {} differs at {}",
                                reference.path,
                                first_difference_between(&test, &expected)
                            )
                        });
                    }
                    Relation::Mismatch if equal => mismatch_equal.push(reference.path.as_str()),
                    Relation::Mismatch => {}
                }
            }
            if matched && mismatch_equal.is_empty() {
                Ok(CaseOutcome::Pass)
            } else {
                let detail = if !mismatch_equal.is_empty() {
                    format!("mismatch references unexpectedly agree: {mismatch_equal:?}")
                } else {
                    first_difference.unwrap_or_else(|| "no match reference agreed".to_string())
                };
                Ok(CaseOutcome::AssertionMismatch(detail))
            }
        }
        CaseKind::Testharness => Err(format!("{} is not a runnable static case", case.path)),
    }
}

fn allowed_files(case: &Case) -> HashSet<&str> {
    let mut allowed = HashSet::from([case.path.as_str()]);
    allowed.extend(
        case.references
            .iter()
            .map(|reference| reference.path.as_str()),
    );
    allowed.extend(case.resources.iter().map(String::as_str));
    allowed
}

fn render_grid(
    manifest: &Manifest,
    path: &str,
    allowed: &HashSet<&str>,
) -> Result<VisualGrid, String> {
    let painted = render_page(&manifest.profile, path, allowed, RenderMetrics::TERMINAL)?;
    visible_grid(&painted, manifest.profile.columns, manifest.profile.rows)
}

fn visible_grid(painted: &DisplayList, columns: u16, rows: u16) -> Result<VisualGrid, String> {
    let backend = TestBackend::new(columns.saturating_add(2), rows);
    let mut terminal = Terminal::new(backend).map_err(|error| format!("terminal: {error}"))?;
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.buffer_mut().set_style(
                area,
                Style::default().fg(PAPER_WHITE.text).bg(PAPER_WHITE.bg),
            );
            Content {
                lines: &ContentLines { painted, scroll: 0 },
                theme: &PAPER_WHITE,
            }
            .render(area, frame.buffer_mut());
        })
        .map_err(|error| format!("terminal draw: {error}"))?;
    let buffer = terminal.backend().buffer();
    let mut cells = Vec::with_capacity(usize::from(columns) * usize::from(rows));
    for row in 0..rows {
        for column in 0..columns {
            cells.push(buffer[(column + 1, row)].clone());
        }
    }
    Ok(VisualGrid {
        columns,
        rows,
        cells,
    })
}

pub fn visually_equal(
    left: &DisplayList,
    right: &DisplayList,
    columns: u16,
    rows: u16,
) -> Result<bool, String> {
    Ok(visible_grid(left, columns, rows)? == visible_grid(right, columns, rows)?)
}

fn render_page(
    profile: &super::manifest::Profile,
    path: &str,
    allowed: &HashSet<&str>,
    metrics: RenderMetrics,
) -> Result<DisplayList, String> {
    let root = corpus_root();
    let source_path = root.join(path);
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("page {}: {error}", source_path.display()))?;
    let document_url = Url::parse("https://wpt.test/")
        .expect("fixed origin")
        .join(path)
        .map_err(|error| format!("document URL {path}: {error}"))?;
    let mut load = PageLoad::new(
        &source,
        document_url,
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext {
                viewport: Size {
                    cols: profile.columns,
                    rows: profile.rows,
                },
                metrics,
            },
            palette: PAPER_WHITE.palette(),
            scripting: false,
            color_scheme: ColorScheme::Light,
            started: Duration::ZERO,
        },
    );
    for _ in 0..=128 {
        let commands = load.take_commands();
        let had_commands = !commands.is_empty();
        for command in commands {
            let relative = local_url_path(&command.url)?;
            if !allowed.contains(relative.as_str()) {
                return Err(format!("{path} requested unlisted resource {relative}"));
            }
            let content_type = if relative.ends_with(".css") {
                "text/css"
            } else if relative.ends_with(".png") {
                "image/png"
            } else if relative.ends_with(".jpg") || relative.ends_with(".jpeg") {
                "image/jpeg"
            } else if relative.ends_with(".webp") {
                "image/webp"
            } else if relative.ends_with(".gif") {
                "image/gif"
            } else {
                return Err(format!(
                    "{path} requested unsupported resource type {relative}"
                ));
            };
            let resource_path = root.join(&relative);
            let body = fs::read(&resource_path)
                .map_err(|error| format!("resource {}: {error}", resource_path.display()))?;
            let delivered = load.deliver(
                command.resource_id,
                Ok(FetchResponse {
                    final_url: command.url,
                    status: 200,
                    body,
                    content_type: Some(content_type.to_string()),
                }),
            );
            if !delivered {
                return Err(format!("{path} refused resource {relative}"));
            }
        }
        let decodes = load.take_image_decode_commands();
        let had_decodes = !decodes.is_empty();
        for request in decodes {
            let asset_id = request.asset_id;
            let revision = request.revision;
            let result = RasterImageDecoder.decode(request);
            if !load.deliver_image_decode(asset_id, revision, result) {
                return Err(format!("{path} refused decoded image {asset_id:?}"));
            }
        }
        if !had_commands && !had_decodes {
            if !load.applicable_is_settled() {
                return Err(format!("{path} has an unsettled stylesheet graph"));
            }
            if load.failed_resources() != 0 || load.external_disabled() {
                return Err(format!("{path} rejected a stylesheet resource"));
            }
            if load.failed_images() != 0 {
                return Err(format!("{path} rejected an image resource"));
            }
            return Ok(load.force_render().painted);
        }
    }
    Err(format!("{path} exceeded the resource command limit"))
}

#[cfg(feature = "vga")]
fn render_vga(manifest: &Manifest, path: &str, allowed: &HashSet<&str>) -> Result<(), String> {
    let _ = render_page(&manifest.profile, path, allowed, RenderMetrics::VGA)?;
    Ok(())
}

#[cfg(not(feature = "vga"))]
fn render_vga(_manifest: &Manifest, _path: &str, _allowed: &HashSet<&str>) -> Result<(), String> {
    Ok(())
}

fn first_difference_between(left: &VisualGrid, right: &VisualGrid) -> String {
    if left.columns != right.columns || left.rows != right.rows {
        return format!(
            "grid size {}x{} vs {}x{}",
            left.columns, left.rows, right.columns, right.rows
        );
    }
    left.cells
        .iter()
        .zip(&right.cells)
        .enumerate()
        .find_map(|(index, (actual, expected))| {
            (actual != expected).then(|| {
                let column = index % usize::from(left.columns);
                let row = index / usize::from(left.columns);
                format!(
                    "({column},{row}): {:?}/{:?} vs {:?}/{:?}",
                    actual.symbol(),
                    actual.style(),
                    expected.symbol(),
                    expected.style()
                )
            })
        })
        .unwrap_or_else(|| "unknown cell".to_string())
}

#[cfg(feature = "vga")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct VisualPixels {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

#[cfg(feature = "vga")]
fn evaluate_vga_case(manifest: &Manifest, case: &Case) -> Result<CaseOutcome, String> {
    if case.kind != CaseKind::Reftest {
        return Err(format!("{} is not a VGA reftest", case.path));
    }
    let allowed = allowed_files(case);
    let test = render_pixels(manifest, &case.path, &allowed)?;
    let mut matched = false;
    let mut mismatch_equal = Vec::new();
    let mut first_difference = None;
    for reference in &case.references {
        let expected = render_pixels(manifest, &reference.path, &allowed)?;
        let equal = test == expected;
        match reference.relation {
            Relation::Match if equal => matched = true,
            Relation::Match => {
                write_vga_failure(&case.path, &reference.path, &test, &expected);
                first_difference.get_or_insert_with(|| {
                    format!(
                        "match {} differs at {}",
                        reference.path,
                        first_pixel_difference(&test, &expected)
                    )
                });
            }
            Relation::Mismatch if equal => mismatch_equal.push(reference.path.as_str()),
            Relation::Mismatch => {}
        }
    }
    if matched && mismatch_equal.is_empty() {
        Ok(CaseOutcome::Pass)
    } else if !mismatch_equal.is_empty() {
        Ok(CaseOutcome::AssertionMismatch(format!(
            "mismatch references unexpectedly agree: {mismatch_equal:?}"
        )))
    } else {
        Ok(CaseOutcome::AssertionMismatch(
            first_difference.unwrap_or_else(|| "no match reference agreed".to_string()),
        ))
    }
}

#[cfg(not(feature = "vga"))]
fn evaluate_vga_case(_manifest: &Manifest, case: &Case) -> Result<CaseOutcome, String> {
    Err(format!("{} requires the vga feature", case.path))
}

#[cfg(feature = "vga")]
fn render_pixels(
    manifest: &Manifest,
    path: &str,
    allowed: &HashSet<&str>,
) -> Result<VisualPixels, String> {
    use textsurfer::layout::LayoutRect;
    use textsurfer::vga::{SurfaceConfig, VgaBackend};

    let profile = &manifest.vga_profile;
    let painted = render_page(profile, path, allowed, RenderMetrics::VGA)?;
    let mut terminal = Terminal::new(VgaBackend::new(SurfaceConfig {
        cols: profile.columns.saturating_add(2),
        rows: profile.rows,
        scale: 1,
        default_fg: textsurfer::ui::theme::rgb_of(PAPER_WHITE.text),
        default_bg: textsurfer::ui::theme::rgb_of(PAPER_WHITE.bg),
    }))
    .map_err(|error| format!("VGA terminal: {error}"))?;
    terminal
        .draw(|frame| {
            Content {
                lines: &ContentLines {
                    painted: &painted,
                    scroll: 0,
                },
                theme: &PAPER_WHITE,
            }
            .render(frame.area(), frame.buffer_mut());
        })
        .map_err(|error| format!("VGA draw: {error}"))?;
    terminal.backend_mut().draw_overlays(
        &painted,
        (1, 0),
        0,
        LayoutRect {
            col: 1,
            row: 0,
            width: usize::from(profile.columns),
            height: usize::from(profile.rows),
        },
        &[],
        PAPER_WHITE.palette(),
    );
    let surface = terminal.backend().surface();
    let source_width = surface.pixel_size().0 as usize;
    let cell_width = usize::from(profile.cell.column_px);
    let width = usize::from(profile.columns) * cell_width;
    let height = usize::from(profile.rows) * usize::from(profile.cell.row_px);
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        let start = row * source_width + cell_width;
        pixels.extend_from_slice(&surface.pixels()[start..start + width]);
    }
    Ok(VisualPixels {
        width,
        height,
        pixels,
    })
}

/// Writes the two rendered surfaces and their difference mask so a failing pixel
/// reftest can be inspected instead of only reported as a coordinate. Nothing here
/// is an oracle: the files land under `target/`, are never read back, and a write
/// failure must not turn a rendering mismatch into a harness error.
#[cfg(feature = "vga")]
fn write_vga_failure(case: &str, reference: &str, test: &VisualPixels, expected: &VisualPixels) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/wpt-vga");
    if fs::create_dir_all(&root).is_err() {
        return;
    }
    let slug = |path: &str| path.replace(['/', '\\'], "-");
    save_pixels(&root.join(format!("{}-actual.png", slug(case))), test);
    save_pixels(
        &root.join(format!("{}-expected.png", slug(reference))),
        expected,
    );
    if test.width != expected.width || test.height != expected.height {
        return;
    }
    let mut diff = VisualPixels {
        width: test.width,
        height: test.height,
        pixels: test
            .pixels
            .iter()
            .zip(&expected.pixels)
            .map(|(actual, expected)| {
                if actual == expected {
                    0x00_00_00
                } else {
                    0xff_00_ff
                }
            })
            .collect(),
    };
    diff.pixels.truncate(test.width * test.height);
    save_pixels(&root.join(format!("{}-diff.png", slug(case))), &diff);
}

#[cfg(feature = "vga")]
fn save_pixels(path: &std::path::Path, pixels: &VisualPixels) {
    let Ok(width) = u32::try_from(pixels.width) else {
        return;
    };
    let Ok(height) = u32::try_from(pixels.height) else {
        return;
    };
    let mut buffer = image::RgbaImage::new(width, height);
    for (index, pixel) in pixels.pixels.iter().enumerate() {
        let x = (index % pixels.width) as u32;
        let y = (index / pixels.width) as u32;
        buffer.put_pixel(
            x,
            y,
            image::Rgba([
                (pixel >> 16) as u8,
                (pixel >> 8) as u8,
                *pixel as u8,
                u8::MAX,
            ]),
        );
    }
    let _ = buffer.save(path);
}

#[cfg(feature = "vga")]
fn first_pixel_difference(left: &VisualPixels, right: &VisualPixels) -> String {
    if left.width != right.width || left.height != right.height {
        return format!(
            "pixel size {}x{} vs {}x{}",
            left.width, left.height, right.width, right.height
        );
    }
    left.pixels
        .iter()
        .zip(&right.pixels)
        .enumerate()
        .find_map(|(index, (actual, expected))| {
            (actual != expected).then(|| {
                let column = index % left.width;
                let row = index / left.width;
                format!("({column},{row}): {actual:#08x} vs {expected:#08x}")
            })
        })
        .unwrap_or_else(|| "unknown pixel".to_string())
}
