use std::fmt::Write as _;
use std::path::Path;
#[cfg(feature = "vga")]
use std::path::PathBuf;
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use textsurfer::core::geom::Size;
use textsurfer::core::image::DecodedImage;
use textsurfer::core::style::{RenderContext, RenderMetrics};
use textsurfer::css::ColorScheme;
#[cfg(feature = "vga")]
use textsurfer::layout::LayoutRect;
use textsurfer::net::FetchResponse;
use textsurfer::paint::DisplayList;
use textsurfer::pipeline::page_load::{PageLoad, PageLoadOptions};
use textsurfer::pipeline::render::RenderedPage;
use textsurfer::ui::PAPER_WHITE;
use textsurfer::ui::widgets::content::{Content, ContentLines};
use unicode_width::UnicodeWidthChar;

const CASES: &[&str] = &[
    "ua-flow",
    "inline-state",
    "lists",
    "tables",
    "controls",
    "search-flex",
    "box-layout",
    "flex-grid",
    "floats",
    "images",
    "presentational",
];
const VIEWPORTS: &[(u16, u16)] = &[(40, 30), (100, 38), (160, 40)];

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render_atlas.html");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {}: {error}", path.display()))
}

fn render_context(cols: u16, rows: u16, vga: bool) -> RenderContext {
    let viewport = Size { cols, rows };
    if vga {
        RenderContext {
            viewport,
            metrics: RenderMetrics::VGA,
        }
    } else {
        RenderContext::terminal(viewport)
    }
}

fn atlas_pages(cols: u16, rows: u16, vga: bool) -> (RenderedPage, RenderedPage) {
    let mut load = PageLoad::new(
        &source(),
        url::Url::parse("https://atlas.test/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: render_context(cols, rows, vga),
            palette: PAPER_WHITE.palette(),
            scripting: false,
            color_scheme: ColorScheme::Light,
            started: Duration::ZERO,
        },
    );
    let first = load.force_render();
    let commands = load.take_commands();
    assert_eq!(commands.len(), 1);
    let command = commands.into_iter().next().unwrap();
    assert!(load.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: vec![1],
            content_type: Some("image/png".to_string()),
        })
    ));
    let decode = load.take_image_decode_commands().pop().unwrap();
    assert!(load.deliver_image_decode(
        decode.asset_id,
        decode.revision,
        Ok(atlas_image(decode.asset_id, decode.revision))
    ));
    let decoded = load.render_after_image().unwrap();
    (first, decoded)
}

fn atlas_image(asset_id: textsurfer::core::image::ImageAssetId, revision: u64) -> DecodedImage {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for row in 0..16 {
        for col in 0..16 {
            let pixel = match (col >= 8, row >= 8) {
                (false, false) => [255, 0, 0, 255],
                (true, false) => [0, 255, 0, 255],
                (false, true) => [0, 0, 255, 128],
                (true, true) => [255, 255, 0, 255],
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    DecodedImage {
        asset_id,
        revision,
        width: 16,
        height: 16,
        rgba: rgba.into(),
        source: textsurfer::core::image::DecodedImageSource::Raster,
    }
}

fn assert_image_rects_are_text_free(painted: &DisplayList) {
    for image in &painted.images {
        for row in image.rect.row..image.rect.row.saturating_add(image.rect.height) {
            let Some(painted_row) = painted.rows.get(row) else {
                continue;
            };
            for span in &painted_row.spans {
                let mut col = span.col;
                for character in span.text.chars() {
                    let width = character.width().unwrap_or_default();
                    let overlaps = col < image.rect.col.saturating_add(image.rect.width)
                        && col.saturating_add(width) > image.rect.col;
                    assert!(
                        character.is_whitespace() || !overlaps,
                        "text {character:?} from {:?} overlaps image {:?} at ({col}, {row})",
                        span.text,
                        image.rect,
                    );
                    col = col.saturating_add(width);
                }
            }
        }
    }
}

fn panel_ranges(painted: &DisplayList) -> Vec<(&'static str, usize, usize)> {
    let lines = painted.text_lines();
    let starts = CASES
        .iter()
        .map(|case| {
            let marker = format!("ATLAS:{case}");
            lines
                .iter()
                .position(|line| line.contains(&marker))
                .unwrap_or_else(|| panic!("missing atlas marker {marker}"))
        })
        .collect::<Vec<_>>();
    CASES
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let start = starts[index];
            let end = starts.get(index + 1).copied().unwrap_or(lines.len());
            assert!(start < end, "empty atlas panel {case}");
            (*case, start, end)
        })
        .collect()
}

fn styled_panel(painted: &DisplayList, width: u16, start: usize, end: usize) -> String {
    let height = u16::try_from(end - start).unwrap();
    let area = Rect::new(0, 0, width + 2, height);
    let mut buffer = Buffer::empty(area);
    Content {
        lines: &ContentLines {
            painted,
            scroll: start,
            text_fields: Vec::new(),
        },
        theme: &PAPER_WHITE,
        render_images: true,
    }
    .render(area, &mut buffer);
    let mut out = String::new();
    for row in 0..height {
        for col in 0..area.width {
            out.push_str(buffer[(col, row)].symbol());
        }
        out.push('\n');
        let mut run_start = 0;
        for col in 1..=area.width {
            let changed = col == area.width
                || buffer[(col, row)].fg != buffer[(run_start, row)].fg
                || buffer[(col, row)].bg != buffer[(run_start, row)].bg
                || buffer[(col, row)].modifier != buffer[(run_start, row)].modifier;
            if changed {
                let cell = &buffer[(run_start, row)];
                writeln!(
                    out,
                    "  {run_start}..{col} fg={:?} bg={:?} mod={:?}",
                    cell.fg, cell.bg, cell.modifier
                )
                .unwrap();
                run_start = col;
            }
        }
    }
    out
}

fn structure(page: &RenderedPage, cols: u16) -> String {
    let mut out = format!(
        "width={cols} rows={} parse_errors={} css_warnings={} limits={:?}\n",
        page.painted.len(),
        page.parse_errors,
        page.css_warnings,
        page.painted.limits
    );
    for (case, start, end) in panel_ranges(&page.painted) {
        writeln!(out, "case {case} rows={start}..{end}").unwrap();
    }
    for link in &page.painted.links {
        writeln!(
            out,
            "link href={} rects={:?} paint={}",
            link.href, link.rects, link.paint_order
        )
        .unwrap();
    }
    writeln!(out, "hits={}", page.painted.hits.len()).unwrap();
    for image in &page.painted.images {
        writeln!(
            out,
            "image asset={:?} rect={:?} clip={:?} depth={}",
            image.asset_id, image.rect, image.clip, image.depth
        )
        .unwrap();
    }
    for run in &page.painted.scaled_text {
        writeln!(
            out,
            "scaled text={:?} rect={:?} depth={} ink={}",
            run.text, run.rect, run.depth, run.ink
        )
        .unwrap();
    }
    out
}

#[test]
fn atlas_manifest_and_semantics_cover_the_rendering_contract() {
    let fixture = source();
    for case in CASES {
        assert_eq!(
            fixture
                .matches(&format!("data-atlas-case=\"{case}\""))
                .count(),
            1,
            "atlas case {case} must occur exactly once"
        );
    }
    assert_eq!(fixture.matches("data-atlas-case=").count(), CASES.len());
    for &(cols, rows) in VIEWPORTS {
        let (first, decoded) = atlas_pages(cols, rows, false);
        assert_eq!(first.parse_errors, 0);
        assert_eq!(decoded.parse_errors, 0);
        assert_eq!(decoded.css_warnings, 0);
        assert_eq!(decoded.painted.images.len(), 11);
        assert_image_rects_are_text_free(&decoded.painted);
        assert!(first.painted.text_lines().join("\n").contains("[linked]"));
        let text = decoded.painted.text_lines().join("\n");
        for hidden in [
            "HIDDEN TITLE",
            "HIDDEN META",
            "HIDDEN BODY",
            "HIDDEN CONTROL",
            "HIDDEN OPTION",
        ] {
            assert!(
                !text.contains(hidden),
                "{hidden} reached the {cols}-column render"
            );
        }
        assert!(
            decoded
                .painted
                .links
                .iter()
                .any(|link| link.href == "/atlas-link")
        );
        assert!(
            decoded
                .painted
                .links
                .iter()
                .any(|link| link.href == "/image-link")
        );
        if cols == 160 {
            assert!(text.contains("Search Wikipedia"));
            assert!(text.contains("Search"));
        } else {
            assert!(!text.contains("Search Wikipedia"));
        }
        assert!(text.contains("FLOAT-L"));
        assert!(text.contains("FLOAT-R"));
        assert!(text.contains("clear after floats"));
        assert!(text.contains("FLOAT-IMG"));
        assert!(text.contains("underneath it."));
        assert!(
            text.split_whitespace()
                .collect::<Vec<_>>()
                .windows(4)
                .any(|words| words == ["caption", "words", "wrap", "below"])
        );
        for label in ["List", "Comparison", "Linux portal", "Category"] {
            assert!(text.contains(label), "missing table image label {label}");
        }
        for marker in [
            "MODEMARK",
            "ALPHAONE",
            "BETATWO",
            "GAMMATHREE",
            "WRAPMARK",
            "ALSAMARK",
            "DRIMARK",
            "EVDEVMARK",
            "KLIBCMARK",
            "LVMMARK",
        ] {
            assert!(
                text.contains(marker),
                "missing nested table marker {marker} at {cols} columns"
            );
        }
        for (case, start, end) in panel_ranges(&decoded.painted) {
            assert!(
                end - start <= usize::from(rows),
                "atlas panel {case} exceeds {cols}x{rows}"
            );
        }
    }
}

#[test]
fn atlas_structure_and_styled_cells_match_reviewed_references() {
    for &(cols, rows) in VIEWPORTS {
        let (_, page) = atlas_pages(cols, rows, false);
        insta::assert_snapshot!(format!("atlas_structure_{cols}"), structure(&page, cols));
        for (case, start, end) in panel_ranges(&page.painted) {
            let case = case.replace('-', "_");
            insta::assert_snapshot!(
                format!("atlas_cells_{cols}_{case}"),
                styled_panel(&page.painted, cols, start, end)
            );
        }
    }
}

#[cfg(feature = "vga")]
fn vga_panel(painted: &DisplayList, width: u16, start: usize, end: usize) -> image::RgbaImage {
    use ratatui::Terminal;
    use textsurfer::core::style::CellMetric;
    use textsurfer::vga::{SurfaceConfig, VgaBackend};

    let height = u16::try_from(end - start).unwrap();
    let mut terminal = Terminal::new(VgaBackend::new(SurfaceConfig {
        cols: width + 2,
        rows: height,
        scale: 1,
        default_fg: textsurfer::ui::theme::rgb_of(PAPER_WHITE.text),
        default_bg: textsurfer::ui::theme::rgb_of(PAPER_WHITE.bg),
    }))
    .unwrap();
    terminal
        .draw(|frame| {
            Content {
                lines: &ContentLines {
                    painted,
                    scroll: start,
                    text_fields: Vec::new(),
                },
                theme: &PAPER_WHITE,
                render_images: false,
            }
            .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    terminal.backend_mut().draw_overlays(
        painted,
        (1, 0),
        start,
        LayoutRect {
            col: 1,
            row: 0,
            width: usize::from(width),
            height: usize::from(height),
        },
        &[],
        PAPER_WHITE.palette(),
    );
    let surface = terminal.backend().surface();
    let source_width = surface.pixel_size().0 as usize;
    let cell_width = usize::from(CellMetric::DEFAULT.column_px());
    let cell_height = usize::from(CellMetric::DEFAULT.row_px());
    let mut image = image::RgbaImage::new(
        u32::from(width) * cell_width as u32,
        u32::from(height) * cell_height as u32,
    );
    for y in 0..usize::from(height) * cell_height {
        for x in 0..usize::from(width) * cell_width {
            let pixel = surface.pixels()[y * source_width + x + cell_width];
            image.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([
                    ((pixel >> 16) & 0xff) as u8,
                    ((pixel >> 8) & 0xff) as u8,
                    (pixel & 0xff) as u8,
                    255,
                ]),
            );
        }
    }
    image
}

#[cfg(feature = "vga")]
fn reference_path(cols: u16, case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/reference/vga/render_atlas")
        .join(format!("{cols}-{case}.png"))
}

#[cfg(feature = "vga")]
fn write_failure(cols: u16, case: &str, actual: &image::RgbaImage, expected: &image::RgbaImage) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/render-atlas");
    std::fs::create_dir_all(&root).unwrap();
    actual
        .save(root.join(format!("{cols}-{case}-actual.png")))
        .unwrap();
    let mut diff = image::RgbaImage::new(actual.width(), actual.height());
    for (x, y, pixel) in diff.enumerate_pixels_mut() {
        *pixel = if expected.get_pixel_checked(x, y) == Some(actual.get_pixel(x, y)) {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 0, 255, 255])
        };
    }
    diff.save(root.join(format!("{cols}-{case}-diff.png")))
        .unwrap();
}

#[cfg(feature = "vga")]
#[test]
fn atlas_vga_pixels_match_reviewed_references() {
    for &(cols, rows) in VIEWPORTS {
        let (_, page) = atlas_pages(cols, rows, true);
        for (case, start, end) in panel_ranges(&page.painted) {
            let actual = vga_panel(&page.painted, cols, start, end);
            let path = reference_path(cols, case);
            let expected = image::open(&path)
                .unwrap_or_else(|error| panic!("reference {}: {error}", path.display()))
                .into_rgba8();
            if actual != expected {
                write_failure(cols, case, &actual, &expected);
            }
            assert_eq!(actual, expected, "VGA atlas panel {case} at {cols} columns");
        }
    }
}

#[cfg(feature = "vga")]
#[test]
#[ignore]
fn generate_atlas_vga_references() {
    assert_eq!(std::env::var("TEXTSURFER_UPDATE_ATLAS").as_deref(), Ok("1"));
    for &(cols, rows) in VIEWPORTS {
        let (_, page) = atlas_pages(cols, rows, true);
        for (case, start, end) in panel_ranges(&page.painted) {
            let path = reference_path(cols, case);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            vga_panel(&page.painted, cols, start, end)
                .save(path)
                .unwrap();
        }
    }
}
