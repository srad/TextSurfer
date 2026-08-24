use super::*;
use crate::core::geom::Size;
use crate::core::style::{Palette, TextRendering};

#[test]
fn render_html_applies_the_same_style_discovery_rules_as_a_live_page_load() {
    let page = render_html(
        "<template><style>p { display: none }</style></template>
         <style type='text/plain'>p { display: none }</style>
         <style media='print'>p { display: none }</style>
         <p>visible</p>",
        Size { cols: 40, rows: 24 },
        Palette::default(),
        false,
    );
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line.contains("visible"))
    );
}

#[test]
fn rendering_html_reports_parse_and_style_diagnostics_with_the_painted_page() {
    let page = render_html(
        "<style>p { display: block; color: bogus }</style><p>hello</p>",
        Size { cols: 20, rows: 24 },
        Palette::default(),
        false,
    );
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line.contains("hello"))
    );
    assert_eq!(page.css_warnings, 0);
    assert!(
        page.styles.get(page.document.borrow().roots()[0]).display
            != crate::core::style::Display::NONE
    );
}

#[test]
fn media_types_route_to_the_matching_renderer() {
    assert!(matches!(response_kind(None), ResponseKind::Html));
    assert!(matches!(
        response_kind(Some("text/html; charset=utf-8")),
        ResponseKind::Html
    ));
    assert!(matches!(
        response_kind(Some("text/plain")),
        ResponseKind::PlainText
    ));
    assert!(matches!(
        response_kind(Some("image/png")),
        ResponseKind::Unsupported(kind) if kind == "image/png"
    ));
}

#[test]
fn bitmap_profile_emits_scaled_heading_runs_with_full_geometry() {
    let page = render_html_with_text_rendering(
        "<h1><a href='next'>AB</a></h1><p>x<span style='font-size: 24px'>Y</span></p>",
        Size { cols: 40, rows: 24 },
        Palette::default(),
        false,
        TextRendering::ScaledBitmap,
    );
    let heading = page
        .painted
        .scaled_text
        .iter()
        .find(|run| run.text == "AB")
        .unwrap();
    assert_eq!(heading.style.scale, 4);
    assert_eq!(heading.rect.width, 8);
    assert_eq!(heading.rect.height, 4);
    assert!(page.painted.link_at(7, heading.rect.row + 3).is_some());
    assert!(
        page.painted
            .scaled_text
            .iter()
            .any(|run| run.text == "Y" && run.style.scale == 3)
    );
}

#[test]
fn bitmap_headings_degrade_to_a_common_scale_and_cell_rendering_stays_unscaled() {
    let bitmap = render_html_with_text_rendering(
        "<h1>AB</h1>",
        Size { cols: 4, rows: 24 },
        Palette::default(),
        false,
        TextRendering::ScaledBitmap,
    );
    assert_eq!(bitmap.painted.scaled_text[0].style.scale, 2);
    let cell = render_html(
        "<h1>AB</h1>",
        Size { cols: 4, rows: 24 },
        Palette::default(),
        false,
    );
    assert!(cell.painted.scaled_text.is_empty());
    assert!(
        cell.painted
            .text_lines()
            .iter()
            .any(|line| line.contains("AB"))
    );
}

#[test]
fn table_cells_share_the_bitmap_typography_path() {
    let page = render_html_with_text_rendering(
        "<table><tr><td><span style='font-size: 24px'>cell</span></td></tr></table>",
        Size { cols: 40, rows: 24 },
        Palette::default(),
        false,
        TextRendering::ScaledBitmap,
    );
    let run = page
        .painted
        .scaled_text
        .iter()
        .find(|run| run.text == "cell")
        .unwrap();
    assert_eq!(run.style.scale, 3);
    assert_eq!(run.rect.height, 3);
}

#[test]
fn zero_and_small_font_sizes_have_explicit_bitmap_behavior() {
    let page = render_html_with_text_rendering(
        "<p><span style='font-size: 0'>gone</span><span style='font-size: 10px'>dim</span></p>",
        Size { cols: 20, rows: 24 },
        Palette::default(),
        false,
        TextRendering::ScaledBitmap,
    );
    assert!(
        !page
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("gone"))
    );
    let dim = page
        .painted
        .rows
        .iter()
        .flat_map(|row| &row.spans)
        .find(|span| span.text.contains("dim"))
        .unwrap();
    assert_eq!(dim.col, 0);
    assert!(dim.style.dim);
}

#[test]
fn wide_bitmap_glyphs_keep_unicode_cell_width() {
    let page = render_html_with_text_rendering(
        "<h1>界A</h1>",
        Size { cols: 20, rows: 24 },
        Palette::default(),
        false,
        TextRendering::ScaledBitmap,
    );
    let run = page.painted.scaled_text.first().unwrap();
    assert_eq!(run.style.scale, 4);
    assert_eq!(run.rect.width, 12);
}

#[test]
fn explicit_lines_degrade_independently() {
    let page = render_html_with_text_rendering(
        "<h1 style='white-space: pre'>A\nABCDE</h1>",
        Size { cols: 8, rows: 24 },
        Palette::default(),
        false,
        TextRendering::ScaledBitmap,
    );
    assert!(
        page.painted
            .scaled_text
            .iter()
            .any(|run| run.text == "A" && run.style.scale == 4)
    );
    assert!(
        page.painted
            .rows
            .iter()
            .flat_map(|row| &row.spans)
            .any(|span| span.text.contains("ABCDE") && span.style.scale == 1)
    );
}
